# ADR-0376: 요청한 pragma 가 안 선 DB 는 치명이 아니라 degraded 이고, 그 상태는 값으로 밖에 나간다 — ADR-0316 의 보고 채널 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: sqlite, storage, memory, pragma, observability, ipc, cli, adr-0316
- **Group**: storage

## Context

[ADR-0316](0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md) 은 연결
pragma 를 함수 하나(`tasty_memory::pragma::apply_connection_pragmas`)로 모으고, 그 함수가
`journal_mode` 를 되읽어 요청과 대조하게 했다. 보고 채널은 **`tracing::warn!` 하나**였고
함수는 `()` 를 돌려줬다. 그 ADR 의 재검토 트리거가 스스로 그 한계를 적었다 — "이 결정은
실패를 로그로만 보고한다. 아무도 그 로그를 안 보면 관측 가능성이 이름뿐이다."

그 한계가 실제 결함으로 드러났다(2026-09-21 조사):

- `effective_journal_mode` 는 `pub` 이지만 부르는 자리가 그 함수 자신과 시험 둘뿐이다.
  "WAL 이 실제로 섰는가" 를 **IPC·CLI 로 물을 수 없다.** 에이전트 기능은 IPC + CLI 양면이어야
  한다는 원칙(identity 원칙 2)에 걸린다.
- 호출자 두 곳(`MemoryStore::prepare` · `Db::prepare`)이 결과를 들고 있지 않아, 누가
  물어도 답할 값이 프로세스 안에 없다.
- ADR-0316 Decision 은 "정상 결과를 모드별로 규정한다 — 파일 DB 는 `wal`, in-memory DB 는
  `memory`" 라고 적었는데, 코드의 판정 함수는 모드를 보지 않고 **둘 중 하나면** 정상으로
  쳤다. 파일 DB 가 `memory` 로 서도 조용했다.
- `synchronous` · `foreign_keys` · `journal_size_limit` 은 되읽지 않았다. 반환값이 `Ok` 라는
  것은 "요청이 거절되지 않았다" 이지 "그 값이 지금 섰다" 가 아니다.

## Decision

`apply_connection_pragmas` 가 **적용 결과를 값으로 돌려준다**(`AppliedPragmas`). 두 DB 가
그 값을 들고 있고, `system.pressure` 가 새 덩어리 `db_pragmas` 로 내보낸다.

- **넷 다 되읽는다.** pragma 마다 요청값 · 되읽은 실제값 · 설정/되읽기 오류 · `took`
  판정을 한 줄로 남긴다.
- **허용 결과는 모드별이다** — ADR-0316 Decision 이 적은 그대로를 코드가 따른다. 파일 DB 의
  `journal_mode` 는 `wal` 만, in-memory DB 는 `memory` 만 정상이다. 나머지 셋은 두 모드 모두
  요청값 그대로여야 한다. 모드는 연결이 보고하는 파일 이름이 비었는가로 가른다.
- **하나라도 안 섰으면 그 DB 는 `degraded` 다.** 오류가 아니라 **상태**다 — DB 는 열린 채로
  쓰인다. `Err` 를 올리지 않는 ADR-0316 의 조항은 그대로다.
- **노출 자리는 `system.pressure` 의 새 덩어리 `db_pragmas` 다**(`memory_db` · `state_db`
  두 칸, 열리지 않은 DB 는 `null`). CLI 는 같은 응답을 그대로 보이는 `tasty list pressure`.
  `db` 덩어리와 같은 DB 를 말하므로 한 응답 안에서 "commit 이 느리다" 와 "WAL 이 안 섰다" 가
  함께 읽힌다.
- **경고 로그는 그대로 나간다.** 값이 생겼다고 로그를 빼지 않는다 — 로그는 사건 시점을,
  값은 지금 상태를 답한다.

**개정하지 않는 것**: 함수 하나로 모으는 결정 · `Err` 를 올리지 않는 결정 · in-memory 의
`memory` 를 경고 대상에서 빼는 결정 · 네 pragma 의 요청값(`WAL` · `NORMAL` · `ON` ·
`WAL_SIZE_LIMIT_BYTES`)과 순서. 특히 **`synchronous=NORMAL` 을 바꾸지 않는다** — 내구성
등급은 별도 결정이다.

## Consequences

- **얻은 것**: 운영자가 실행 중 인스턴스에 "두 DB 의 pragma 가 실제로 섰나" 를 CLI 한 줄로
  묻는다. ADR-0316 이 "원리적으로 안 붙는다" 로 적어 둔 "경고가 실제 운영에서 읽히는가" 가
  로그를 안 뒤져도 답해진다.
- **얻은 것**: 파일 DB 가 `memory` 로 서는 갈래가 더 이상 조용하지 않다.
- **잃은 것**: 연결마다 되읽기 쿼리가 셋 더 붙는다. 연결 생성은 부팅과 시험 하네스에서만
  일어나므로 뜨거운 경로가 아니다.
- **잃은 것**: `system.pressure` 가 "모수마다 한 덩어리" 인 누계 응답인데, 이 덩어리만 누계가
  아니라 열 때 한 번 정해지는 값이다. 성격이 다른 덩어리가 한 응답에 섞인다 — 핸들러 doc 과
  문서가 그 차이를 명시한다.
- **운영 비용**: 덩어리 수를 세는 문장이 여러 자리에 있다(핸들러 머리말 · CLI 도움말 ·
  api 참조 · telemetry 기능 문서 · 사용자 가이드 양 언어). 이 결정이 그 수를 다섯에서 여섯으로
  옮겼다.

## Alternatives Considered

- **A: 새 메서드 `system.storage` + `tasty list storage`** — 덩어리 성격이 섞이지 않는다.
  안 골랐다: 새 메서드는 메서드 메타 표 등재·권한 분류·dispatch 팔·CLI 하위 명령을 새로
  요구하고, 같은 DB 의 지연(`db`)과 설정이 서로 다른 응답으로 갈라진다. 기존 응답에 키를
  **더하는** 편이 소비자 호환을 가장 많이 보존한다(기존 키는 하나도 안 바뀐다).
- **B: `system.info` 에 싣는다** — 안 골랐다: `system.info` 는 plugin 도 부를 수 있는 표면이고,
  DB 경로의 설정 상태는 plugin 에게 나눌 값이 아니다. `system.pressure` 는 local-only 다.
- **C: pragma 가 안 서면 초기화 실패로 올린다(치명)** — ADR-0316 이 이미 기각한 대안 B 와
  같다. 지금까지 열리던 DB 가 안 열리게 되는 동작 변경이다.
- **D: 로그만 두고 값은 안 만든다(현상 유지)** — IPC·CLI 로 물을 수 없다는 결함이 그대로다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 모드별 허용 결과가 바뀌면(파일 DB 의 정상값이 `wal` 이 아니거나 in-memory 의 정상값이
  `memory` 가 아니게 되면) `an_open_store_carries_the_pragmas_that_took_in_each_mode` 가
  빨개진다.
- 응답 모양이 바뀌면 `the_pragma_block_carries_requested_effective_and_degraded_per_database`
  가 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **degraded 가 실제 운영에서 나타나는가.** 나타나는데 아무도 안 보면 이 덩어리도 이름뿐이다.
  재는 법: 사용자 보고나 진단 덤프에서 `db_pragmas.*.degraded == true` 가 관측되는지 본다.
- **`synchronous` 등급을 FULL 로 올리자는 결정이 나오면** 이 ADR 이 아니라 그 결정이 요청값을
  바꾼다 — 그때 허용 결과표를 함께 고친다.

## References

- 개정 대상: [ADR-0316](0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md) (보고 채널 조항 — 로그 단독 → 값 + IPC·CLI)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 후속: [ADR-0485](0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md) (`degraded` 의 뜻을 파일을 못 열어 in-memory 대체로 뜬 경우까지 넓혔다)
- 결정이 실현된 현재 위치: `crates/tasty-memory/src/pragma.rs` 의 `apply_connection_pragmas` ·
  `AppliedPragmas` · `PragmaReading`, 보유자는 `MemoryStore::applied_pragmas` 와 `src/db.rs` 의
  `Db::applied_pragmas`, 노출은 `src/adapters/ipc/handler/pressure.rs` 의 `db_pragmas_json`
- 저장 실패 분류는 짝 결정 [ADR-0377](0377-a-failed-memory-write-names-its-cause-with-the-same-table-as-init.md)
- 노출 표면의 선례: [ADR-0333](0333-the-pressure-gauge-is-read-by-one-local-only-method-and-split-by-population.md)
- 정본 문서: [storage](../design/systems/storage.md) · [memory](../design/systems/memory.md) ·
  [telemetry](../features/telemetry/index.md)
