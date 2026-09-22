# ADR-0611: `memory.db` 에 쓰는 IPC 는 이름공간과 무관하게 durable 이 아님을 말한다 — ADR-0485 의 적용 범위 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: sqlite, storage, memory, degraded, durability, ipc, agent, approval, telemetry, session

## Context

[ADR-0485](0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md) 는 `memory.db`
를 못 열어 in-memory 대체로 뜬 호스트에서 **`memory.*` 쓰기 계열**의 성공 응답에 `durable: false`
를 더하기로 했고, 재검토 조건으로 "`memory.*` 밖에서 `memory.db` 에 쓰는 IPC 가 durable 여부를
약속해야 하게 되면" 을 적었다.

그 조건이 섰다. 같은 `memory.db` 에 쓰는 이웃 이름공간 — `surface.meta.set/unset` ·
`approval.request/respond/cancel/summary.set` · `agent.*`(task · barrier · semaphore · lease ·
rate-limit) · `telemetry.record/record_batch/cap.*` · `session.issue/revoke` — 은 대체 상태에서도
칸 없이 성공만 돌려줬다. 실측(손상 DB 홈): `tasty surface-meta set` → `{"ok":true,"surface_id":1}`.
에이전트는 surface 메타 · 승인 요약 · task 를 저장했다고 믿지만 재시작에 사라진다.
ADR-0485 의 근거("실패한 commit 을 durable 성공으로 반환하지 않는다")는 이름공간을 가리지 않는다.

## Decision

ADR-0485 의 **적용 범위 조항**을 개정한다: `durable: false` 를 더하는 대상은 `memory.*` 가 아니라
**`memory.db` 에 쓰는 IPC 쓰기 계열 전부**다 — 이름공간 `memory.` · `agent.` · `approval.` ·
`surface.meta.` · `telemetry.` · `session.` 에서 `tasty_ipc::method_meta` 의 효과 분류가 `Read` 가
아닌 메서드. 표가 쓰기로 적지만 저장소에 쓰지 않는 것만 사유와 함께 뺀다(`memory.export` ·
`agent.task_run` · `agent.task_reduce`). 칸은 성공 응답의 객체 결과에만 붙고, 오류 응답은 그대로다.

**개정하지 않는 것**: 대체로 계속 뜬다는 결정 · `system.pressure` 의 `init_failure`/`degraded` ·
정상 저장소에서는 칸을 싣지 않는다는 결정(응답이 종전과 바이트 단위로 같다) · 칸 이름과 값 ·
부팅 `warn` · 화면 안내를 두지 않는다는 결정. 모두 ADR-0485 그대로다.

## Consequences

- **얻은 것**: 대체 상태의 호스트에서 에이전트 primitive · 승인 · surface 메타 · 텔레메트리 ·
  세션을 쓰는 호출자도 응답 하나로 "재시작에 사라진다" 를 읽는다.
- **잃은 것**: 표가 쓰기로 분류하지만 실제로는 저장소에 안 쓰는 응답(예: `agent.task_purge` 의
  `dry_run`)도 대체 상태에서는 칸을 싣는다 — 그 칸은 "이 저장소는 durable 이 아니다" 로 읽혀 거짓은
  아니다. 반대로 `Read` 로 분류되지만 부수적으로 쓰는 조회(`agent.semaphore_list` 가 만료 홀더를
  회수해 영속하는 것 등)는 칸이 없다.
- **운영 비용 / 유지 부담**: 위 이름공간에 새 쓰기 메서드가 생기면 `written` 이나
  `mark_durability`(둘 다 `src/adapters/ipc/handler/memory.rs`)로 답해야 한다. 시험
  `every_memory_write_reports_a_fallback_store_as_not_durable` 가 이름공간 목록 × 메서드 표로 쓰기
  계열을 뽑아 라우터 arm → 핸들러 본문으로 따라가 잰다(변이: `handle_barrier_delete` 의 감싸기를
  벗기면 그 이름으로 죽는다). 새 이름공간이 `memory.db` 에 쓰기 시작하면 목록에 더해야 한다 —
  그것은 시험이 못 본다.

## Alternatives Considered

- **결함 보고가 열거한 곳(surface.meta · approval · agent)만 고친다** — 같은 근거가 `telemetry.*` ·
  `session.*` 에도 서는데 그 둘만 약속 밖에 남는다. 범위를 이름공간 목록으로 적는 편이 규칙이
  한 문장이 된다.
- **라우터에서 응답을 일괄 후처리한다** — 핸들러를 안 건드려도 되지만, 라우터는 어느 메서드가
  어느 저장소에 쓰는지 모르므로 결국 같은 목록이 라우터에 생긴다. 목록이 핸들러와 떨어진 자리에
  사는 쪽이 더 쉽게 낡는다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 위 여섯 이름공간 밖의 IPC 가 `memory.db` 에 쓰기 시작한다. 재는 법: `src/adapters/ipc/handler`
  아래에서 `with_memory` 안에서 `put`/`delete` 를 부르는 핸들러의 이름공간을 센다.
- ADR-0485 의 대체 자체가 없어진다 — 이 개정도 대상이 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- `Read` 로 분류된 조회의 부수 쓰기(만료 회수)가 사라지는 것이 문제로 보고된다. 재는 법: 대체
  상태에서 재시작 뒤 만료 홀더가 되살아났다는 보고를 센다.

## References

- 개정 대상: [ADR-0485](0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md) (쓰기 응답 `durable: false` 의 적용 범위 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 설계 문서: [storage](../design/systems/storage.md) "저장 실패의 의미" · "초기화 실패"
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/ipc/handler/memory.rs` 의 `written` ·
  `mark_durability`, 시험 `src/adapters/ipc/handler/memory/durable_tests.rs`.
