# ADR-0485: 못 연 `memory.db` 는 in-memory 대체로 계속 뜨되, 그 사실을 진단과 쓰기 응답이 말한다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: sqlite, storage, memory, degraded, durability, ipc, cli, fallback
- **Group**: storage

## Context

호스트는 부팅 때 `memory.db` 를 연다(`tasty_memory::init_with_config`). 그것이 실패하면
(파일 손상 · 권한 · 잠김 · 디스크 가득 · 스키마 불일치 · 홈 없음) 부팅은 `warn` 한 줄을 남기고
**in-memory SQLite 저장소로 대체해 계속 뜬다.** 이 대체는 `16cebb8b4`(2026-05-29)부터 있었다.

검증에서 실측한 결과(손상 DB = urandom 8192 B 로 부팅):

- 로그는 원인을 옳게 분류했다(`memory.db init at boot failed: memory.db corrupted: …`).
- 그러나 `system.pressure` 의 `db_pragmas.memory_db` 는 `in_memory: true` · **`degraded: false`**
  였고, `memory.put` 은 `{"ok": true, "version": 1}` 을 돌려줬다. 재시작 뒤 확인 응답을 받은 키는
  **전부 없었다.** 즉 저장이 durable 이 아니라는 사실이 응답 어디에도 없었다.
- 문서도 어긋났다. `docs/design/systems/storage.md` 의 절 제목 "초기화 실패 = 인메모리 폴백 없음"
  과 "`memory.db` 는 같은 variant 를 가진 자기 타입을 쓴다" 를 함께 읽으면 `memory.db` 도 같은
  fatal 정책으로 읽히는데, 실제는 반대였다. 그 절의 fatal 정책은 `state.db` 의 것이다.

SQLite 적용값 티켓의 체크리스트가 요구하는 것은 "실패 시 기존 fatal 정책과 일치하는 오류 **또는
명시적 degraded 상태**" 와 "실패한 commit 을 durable 성공으로 반환하지 않는다" 다.

## Decision

**대체와 부팅 계속은 유지하고, 대체 상태를 명시적 degraded 로 만든다.** 기존 외부 응답의 키와
그 의미는 바꾸지 않고 칸만 더한다.

- 대체 저장소는 자기가 대체라는 사실과 원인을 들고 연다 —
  `MemoryStore::open_in_memory_after_init_failure(&MemoryInitError)` 가 `InitFallback { cause,
  error }` 를 싣는다. `cause` 는 `MemoryInitError::cause()` 의 이름(`home_missing` ·
  `permission_denied` · `busy` · `disk_full` · `corrupt` · `schema_mismatch` · `other`)이고, 사용자
  안내 i18n 키 `memory_error.<이름>` 의 끝토막과 같으며, 저장 경로와 겹치는 갈래는
  `StorageFailure::as_str` 과 같은 이름이다. 대체의 config 는 이전과 같이 기본값이다.
- `system.pressure` 의 `db_pragmas.memory_db` 에 `init_failure` 칸을 더한다 — 대체면
  `{cause, error}`, 아니면 `null`. 대체면 pragma 가 다 섰어도 `degraded` 가 `true` 다.
  `in_memory` 는 그대로 사실을 말한다. `state_db` 에는 이 칸이 없다(대체가 없다).
- `memory.*` 쓰기 계열 메서드의 성공 응답은 대체 저장소에서 `durable: false` 를 더한다.
  `ok` · `version` 등 기존 칸은 그대로다. **정상 저장소에서는 칸을 싣지 않는다** — 응답이 이 결정
  전과 바이트 단위로 같다. 쓰기 계열은 `tasty_ipc::method_meta` 의 효과 분류가 `Read` 가 아닌
  `memory.*` 이름이고, `memory.export` 하나만 뺀다(표는 `Mutate` 로 적지만 저장소를 읽기만 한다).
  CLI(`tasty memory …`)는 응답 JSON 을 그대로 찍으므로 같은 칸이 보인다.
- 부팅은 대체를 열 때 `warn` 을 하나 더 남긴다(`memory.db falls back to an in-memory store
  (cause=…) — writes will not survive a restart`).
- 화면 안내(modal · toast)는 이 결정에 들지 않는다 — 새 UI 이고 디자인 값이 필요하다.

## Consequences

- **얻은 것**: 손상 DB 로 떠 있는 호스트에서 에이전트·plugin 이 쓰기 응답 하나로, 운영자가
  `tasty list pressure` 하나로 "지금 쓰는 것은 재시작에 사라진다" 와 그 원인을 읽는다. 사용자는
  손상 파일로도 여전히 앱을 쓴다.
- **잃은 것**: 없음을 표지하지 않는다는 선택 때문에, 칸이 없는 응답은 "durable 저장소" 와 "이
  칸을 모르는 옛 호스트" 를 가르지 못한다. 옛 호스트에서는 원래 이 정보가 없었으므로 새로 생긴
  모호함은 아니다.
- **운영 비용 / 유지 부담**: 새 쓰기 메서드가 `written` 대신 `JsonRpcResponse::success` 로
  답하면 대체 저장소에서 durable 로 보인다. 시험
  `every_memory_write_reports_a_fallback_store_as_not_durable` 가 메서드 표에서 쓰기 계열을 뽑아
  라우터 arm → 핸들러 본문으로 따라가 잰다(변이: `handle_goal_clear` 의 `written` 을 되돌리면
  그 이름으로 죽는다).
- `degraded` 의 뜻이 넓어졌다 — "pragma 가 안 섰다" 에서 "pragma 가 안 섰거나 파일을 못 열어
  대체로 떴다" 로. 두 경우 모두 "열린 채로 쓰이지만 정상이 아니다" 라 소비자의 처방 방향(원인
  칸을 읽는다)은 같고, 원인은 각각 `pragmas.*.took` 과 `init_failure` 로 갈린다.

## Alternatives Considered

- **A: `state.db` 와 같이 fatal(안내 후 종료)** — 문서가 이미 그렇게 읽히고 정책이 하나가 된다.
  안 골랐다: 지금 손상된 `memory.db` 로도 앱을 쓰는 사용자가 앱을 못 쓰게 된다. 기존 동작이
  가장 크게 깨지는 선택이고, 종료 안내 UI 도 새로 필요하다.
- **B: 대체 저장소에서 쓰기를 거절한다(오류 응답)** — "실패를 성공으로 돌려주지 않는다" 를 가장
  곧게 만족한다. 안 골랐다: 같은 세션 안에서 쓰고 읽는 소비자(blackboard · plan · cache · goal)가
  지금은 동작하는데, 전부 오류로 바뀐다. 세션 동안은 저장이 **성공한 것이 사실**이고 틀린 것은
  "재시작 뒤에도 남는다" 는 함의뿐이므로, 그 함의만 고친다.
- **C: 정상 경로에서도 `durable: true` 를 싣는다** — 칸의 유무로 호스트 판을 가를 필요가 없어진다.
  안 골랐다: 정상 경로의 모든 쓰기 응답이 바뀐다. 호환을 가장 많이 보존하는 쪽은 비정상일 때만
  칸을 더하는 것이다.
- **D: `in_memory: true` 만으로 충분하다고 보고 문서만 고친다** — 코드 변경이 없다. 안 골랐다:
  시험·테스트 조립의 in-memory 저장소와 대체가 같은 값을 내고, 쓰기 응답에는 여전히 아무것도 없다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 대체 저장소를 여는 자리(`crate::boot::wiring::memory_fallback_after`)가 없어지거나 부팅이
  `memory.db` 초기화 실패에서 종료하게 되면 — 이 결정의 전제(대체가 있다)가 사라진다.
- `memory.*` 밖에서 `memory.db` 에 쓰는 IPC(예: `approval.summary.set` · `agent.*`)가 durable
  여부를 약속해야 하게 되면 — 지금 `durable` 칸은 `memory.*` 표면에만 있다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 사용자가 대체 상태를 모르고 작업을 잃었다는 보고가 나오면 화면 안내를 디자인 요청으로 올린다.
  재는 법: 이슈·피드백에서 "재시작하니 memory 가 사라졌다" 류 보고를 센다.

## References

- 설계 문서: [storage](../design/systems/storage.md) "초기화 실패" 절 · [memory](../design/systems/memory.md)
- 짝 결정: [ADR-0376](0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md)
  (pragma 가 안 선 DB 는 degraded) · [ADR-0377](0377-a-failed-memory-write-names-its-cause-with-the-same-table-as-init.md)
  (저장 실패의 원인 표)
- 코드 근거(현재 위치, 심볼): `tasty_memory::InitFallback` · `MemoryInitError::cause` ·
  `MemoryStore::open_in_memory_after_init_failure` · `crate::boot::wiring::memory_fallback_after` ·
  `crate::adapters::ipc::handler::memory::written` · `pressure::db_pragmas_json`
- 부분 개정: [0518](0518-every-ipc-write-to-memory-db-says-when-it-is-not-durable.md) (쓰기 응답 `durable: false` 의 적용 범위 조항 개정)
