# ADR-0316: DB 는 요청한 pragma 가 아니라 **적용된** pragma 를 보고한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: sqlite, storage, memory, pragma, observability, wal
- **Group**: storage

## Context

`memory.db`(`crates/tasty-memory`)와 `state.db`(`src/db.rs`)는 연결을 열 때마다 같은 네
pragma 를 세운다. 그 코드가 두 자리에 **글자 그대로 복제**돼 있었고, 앞의 셋은 결과를
`.ok()` 로 버리고 넷째(`journal_size_limit`)만 실패를 경고했다. 즉 같은 함수 안에서 셋은
삼키고 하나는 검사하는 형태였다.

사본은 이미 한 번 갈렸다. `journal_size_limit` 이 한쪽에 먼저 들어갔고, 다른 쪽에 남은
주석이 "두 DB 는 각자의 `prepare` 를 쓰므로 한쪽만 고치면 다른 쪽은 그대로 무한히 자란다"
라고 그 사실을 적고 있었다.

**그런데 실측이 문제의 자리를 옮겼다** (2026-09-20, rusqlite 0.32.1):

| 모드 | 요청 | `pragma_update` 반환 | 실제 적용값 |
|---|---|---|---|
| in-memory | `journal_mode=WAL` | `Ok(())` | **`memory`** |
| file | `journal_mode=WAL` | `Ok(())` | `wal` |
| 둘 다 | `synchronous=NORMAL` | `Ok(())` | `1`(NORMAL) |
| 둘 다 | `foreign_keys=ON` | `Ok(())` | `1`(ON) |

**버려지던 `Err` 는 애초에 안 났다.** 삼킴을 고쳐도 아무것도 드러나지 않는다. 드러나지
않던 것은 따로 있다 — SQLite 는 파일이 없는 DB 에 WAL 을 적용할 수 없고(WAL 은 곁에 파일
둘을 쓴다) 그 요청을 **조용히 거절한다.** 반환은 성공이고 값만 `memory` 로 남는다.

즉 **요청값과 적용값은 다른 축**이며, 반환값 검사로는 그 축을 볼 수 없다. 소스에 `"WAL"`
이라고 적혀 있다는 것을 runtime 보장으로 쓰면 안 된다.

## Decision

연결 pragma 를 **함수 하나**(`tasty_memory::pragma::apply_connection_pragmas`)로 모으고,
두 DB 가 그것을 부른다. 그 함수는 **적용된 값을 되읽어 요청과 대조한다.**

- **정상 결과를 모드별로 규정한다.** 파일 DB 는 `wal`, in-memory DB 는 `memory` 다. 후자는
  실패가 아니라 그 모드가 가질 수 있는 유일한 값이므로 경고 대상에서 뺀다. 이것을 안 빼면
  경고가 모든 부팅과 모든 테스트에서 울려, 진짜 어긋남과 구별되지 않는다.
- **그 밖의 값과 되읽기 실패는 `tracing::warn!`** 으로 나간다.
- **`Err` 를 올리지 않는다.** 호출자는 둘 다 DB 를 여는 중이고, pragma 가 안 서는 것은
  열기 자체의 실패와 다른 축이다 — 열린 DB 는 그대로 쓸 수 있다. 옆 줄의
  `journal_size_limit` 이 이미 그 형태였고, 그 선례를 넷 전부로 넓힌 것이다.
- **사본을 두지 않는다.** 이 pragma 집합에는 DB 별 자유도가 없다. 자유도 없는 사본은 다음
  편집에서 갈리므로 값이 아니라 호출을 공유한다.

## Consequences

- **얻은 것**: "WAL 로 돌고 있나" 가 소스가 아니라 **실행 중 값**으로 답해진다. 파일 DB 가
  WAL 을 못 잡은 채 도는 상태가 더 이상 조용하지 않다.
- **얻은 것**: 두 DB 의 pragma 집합이 **갈릴 수 없다.** 한쪽만 고치는 편집이 불가능하다.
- **잃은 것**: 연결마다 `PRAGMA journal_mode` 조회가 한 번 더 붙는다. 연결 생성은 부팅과
  테스트 하네스에서만 일어나므로 경로가 뜨겁지 않다.
- **운영 비용**: in-memory 의 정상값을 명시적으로 허용하므로, SQLite 가 그 값을 바꾸면
  경고가 조용히 사라진다. 아래 재검토 트리거가 그 자리를 잡는다.

## Alternatives Considered

- **A: `.ok()` 를 `if let Err` 로만 바꾼다** — 티켓이 시사한 최소 처방. 안 골랐다: 실측상
  그 `Err` 가 나지 않으므로 **아무것도 관측 가능해지지 않는다.** 고친 것처럼 보이는데
  달라지는 것이 없는 형태가 가장 나쁘다.
- **B: pragma 실패를 초기화 실패로 올린다(`Err` 반환)** — 가장 엄격하다. 안 골랐다:
  동작 변경이다. 지금까지 열리던 DB 가 안 열리게 되고, 그 판단은 이 리팩토링의 범위가
  아니라 `synchronous` 의 내구성 등급을 정하는 별도 결정이다.
- **C: in-memory 를 예외로 두지 않고 그냥 경고한다** — 코드가 가장 단순하다. 안 골랐다:
  `MemoryStore::open_in_memory` 은 테스트와 production placeholder 양쪽에서 쓰이므로
  경고가 상시 울린다. 늘 울리는 경고는 없는 경고와 같다.
- **D: 상수만 공유하고 함수는 각자 둔다** — 지금까지의 형태다. 안 골랐다: 그 형태가
  실제로 갈렸고, 갈린 흔적이 주석으로 남아 있었다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- in-memory DB 의 `journal_mode` 가 `memory` 가 아니게 되면
  `the_effective_journal_mode_differs_between_a_file_and_an_in_memory_database` 가 빨개진다.
  그 값이 바뀌면 "정상 결과" 의 정의를 다시 정해야 한다.
- 정상 경로가 조용하다는 것은 `a_healthy_database_logs_nothing_while_setting_its_pragmas` ·
  `an_in_memory_database_does_not_warn_about_its_own_journal_mode` 가 본다. 경고가 늘
  울리게 되면 그 둘이 먼저 죽는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **경고가 실제 운영에서 읽히는가.** 이 결정은 실패를 로그로만 보고한다. 아무도 그 로그를
  안 보면 관측 가능성이 이름뿐이다. 재는 법: 읽기 전용 파일로 `~/.tasty/state.db` 를 만들고
  호스트를 띄워 `~/.tasty/debug.log` 에 그 경고가 남는지 본다.
- **`synchronous=NORMAL` 이 약속하는 내구성 범위.** 이 ADR 은 그 값을 바꾸지 않고 적용
  여부만 본다. NORMAL 은 전원 장애에서 최신 커밋 보존을 약속하지 않는다. 재는 법: 커밋
  직후 전원을 끊고 재시작해 마지막 커밋의 생존을 본다 — 이 레포에 그 장비는 없다.

## References

- 결정이 실현된 현재 위치: `crates/tasty-memory/src/pragma.rs` 의
  `apply_connection_pragmas` · `effective_journal_mode`, 호출자는
  `crates/tasty-memory/src/lib.rs` 의 `MemoryStore::prepare` 와 `src/db.rs` 의 `Db::prepare`
- 시험: `crates/tasty-memory/src/tests.rs` 의
  `a_read_only_database_says_that_the_requested_pragmas_did_not_take` ·
  `a_healthy_database_logs_nothing_while_setting_its_pragmas` ·
  `an_in_memory_database_does_not_warn_about_its_own_journal_mode` ·
  `the_effective_journal_mode_differs_between_a_file_and_an_in_memory_database`
- WAL 되감기 한도 값의 근거: `crates/tasty-memory/src/lib.rs` 의 `WAL_SIZE_LIMIT_BYTES`
- 보장 범위를 적는 정본 문서: [storage](../design/systems/storage.md) ·
  [memory](../design/systems/memory.md)
- SQLite 가 in-memory DB 에 WAL 을 못 쓰는 근거: <https://www.sqlite.org/wal.html>
- 부분 개정: [0376](0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md) (보고 채널 조항 개정 — 로그 단독에서 값 + IPC·CLI 로, 되읽기를 넷 전부로)
