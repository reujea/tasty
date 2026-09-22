# ADR-0090: e2e 테스트 격리 단위는 프로세스가 아니라 workspace 다

- **Status**: Accepted
- **Date**: 2026-08-30
- **Tags**: testing, e2e, harness, isolation, workspace, attach, ci
- **Group**: testing

## Context

`tests/` 의 e2e 테스트는 실제 `tasty` 바이너리를 띄워 IPC 로 조작한다. 이 바이너리는 GUI 앱이라 뜰 때마다 창이 생기고 **OS 포커스를 훔친다** — 개발자가 다른 창에서 작업 중이면 테스트가 키 입력을 가로채고, 창 spawn/kill 자체가 dev 프로필 기준 수 초의 비용이다.

`tests/e2e_tests.rs` 는 처음부터 이 비용을 알고 있었고, 33 개 시나리오를 `#[test]` **하나**에 몰아넣어 인스턴스를 1 개만 썼다. 그런데 그 원칙이 소스 주석 두 줄(`tests/e2e_tests.rs` 모듈 doc)에만 존재했고 `docs/` 에는 없었다. 그 결과 후속 작업자가 원칙을 볼 경로가 없어, `attach_*` 5 개 + `hooks_detection_e2e` 1 개 파일의 14 개 테스트가 각자 `TastyInstance::spawn()` 을 호출하게 됐다 — `cargo test --workspace` 한 번에 GUI 창 15 개, `attach_git_query_loopback` 구간에서는 동시에 5 개가 떴다.

그 테스트들이 프로세스를 분리한 실제 이유는 "프로세스 격리가 필요해서" 가 아니었다. 전부 `workspace.list[0]` = **같은 첫 workspace** 를 attach 로 점유했기 때문에 서로 충돌했고, 프로세스를 나누는 것이 가장 손쉬운 회피였다.

한편 격리를 workspace 로 내릴 수 있다는 근거는 이미 코드에 있었다.

- attach 점유는 `OccupancyRegistry` 의 `surface_locks` / `workspace_locks` — **workspace/surface 단위 lock** 이다(`src/core/attach.rs`). 서로 다른 workspace 를 잡는 client 들은 한 인스턴스 위에서 공존한다.
- IPC 로 만든 workspace 는 `IntentOrigin::Agent` 라 active 를 전환하지 않는다(`docs/identity.md` 원칙 1·3). 즉 여러 테스트가 병렬로 workspace 를 만들어도 서로의 활성 상태를 흔들지 않는다.
- `workspace.create` 응답이 `id` 와 `surface_id` 를 함께 돌려주므로, 테스트는 한 번의 호출로 자기 격리 단위 전체를 얻는다.
- `tests/gui_common/mod.rs` 에 이미 `OnceLock` 기반 공유 인스턴스 + "테스트마다 자기 workspace" 전략이 구현돼 있었다. 다만 그것을 쓰는 `gui_tests.rs` 가 전수 `#[ignore]` 라 CI 경로에 한 번도 걸리지 않아 선례로 보이지 않았다.

## Decision

**e2e 테스트의 격리 단위는 workspace 로 하고, tasty 인스턴스는 test binary 당 1 개를 공유한다.** 공유 진입점은 `common::shared()`(`tests/common/mod.rs`) 이며 `&'static TastyInstance` 를 돌려준다 — lock 을 잡지 않으므로 테스트는 그대로 병렬 실행되고, 각 테스트는 `create_workspace()` 로 자기 workspace/surface 를 만들어 그 안에서만 논다. 전용 인스턴스는 **프로세스 경계가 검증 대상 자체일 때만** 정당한 예외이며(기동 시점 config 이 달라야 하는 경우, 프로세스 자원을 외부에서 측정하는 경우), 그 예외는 정적 가드의 allowlist 에 이유와 함께 등록해야 한다.

공유 범위가 **test binary 단위**라는 점을 결정의 일부로 못 박는다. `OnceLock` 은 프로세스 로컬 정적 상태이고 cargo 는 test 타겟마다 별도 프로세스를 띄우므로, 이 구조로 도달 가능한 하한은 *바이너리당 1 개*이지 저장소 전체 1 개가 아니다.

## Consequences

- **얻은 것**: `cargo test --workspace` 의 GUI 인스턴스가 15 개 → 8 개(+ 하네스 자체 검증 1)로 줄고, 동시 생존 최대치가 5 → 2(공유 1 + `inherit_cwd` 전용 1)가 됐다. 포커스 도난과 프로세스 기동 비용이 그만큼 준다. 오래 걸리는 테스트(heartbeat TTL 만료 대기 등)가 프로세스를 독점하지 않고 다른 테스트와 겹쳐 돈다.
- **잃은 것**: 프로세스 경계가 주던 "무조건 깨끗한 상태" 를 잃는다. workspace 로 덮이지 않는 전역 상태(headless PTY registry, `global_hook.*`, notification)는 같은 binary 의 다른 테스트가 만든 항목까지 함께 조회된다 — 목록 검증은 "내 것이 있는가"(`any`) 형태여야 하고 길이나 `[0]` 번째를 assert 하면 안 된다.
- **운영 비용 / 유지 부담**: 원칙이 지켜지는지를 사람 눈이 아니라 `tests/e2e_single_instance_guard.rs` 가 강제한다 — 파일당 전용 spawn 수와 인스턴스를 띄우는 test 파일 목록 두 축을 고정하므로, 예외를 늘릴 때마다 allowlist 에 근거를 적어야 한다. 이 마찰이 곧 이 ADR 의 집행 수단이다.

## Alternatives Considered

- **A: 프로세스 격리 유지(테스트마다 spawn)** — 상태 오염 걱정이 없다는 장점이 있으나, GUI 창 spawn/kill 이 OS 포커스를 훔치는 비용을 테스트 수만큼 지불한다. 애초에 그 비용 때문에 `e2e_tests.rs` 가 33 개 시나리오를 한 함수로 뭉쳐 두었는데, 같은 레포에서 반대 방향으로 가는 셈이다.
- **B: 프로세스 간 락/IPC 로 저장소 전체 인스턴스 1 개** — 인스턴스 수는 최소가 되지만 test binary 간 실행 순서 결합이 생기고 CI 병렬성을 잃는다. cargo 의 타겟별 프로세스 모델과도 어긋난다.
- **C: attach 계열 test 파일을 한 binary 로 통합** — 인스턴스가 8 → 4 로 더 줄지만, 파일별 주제 doc(ADR-0052/0053/0056 등 서로 다른 결정을 검증한다)이 뭉쳐 실패 시 시나리오 분리가 어려워지고, 전역 PTY registry 를 쓰는 테스트가 PTY 목록을 검증하는 테스트와 같은 binary 에 놓인다. 인스턴스 수를 더 줄일 필요가 실제로 생기면 재검토 대상이다.
- **D: 실행 중 tasty PID 개수를 세는 동적 가드** — (a) 가드가 공유 인스턴스 위에서 돌면 자기 프로세스를 포함해 세고, (b) cargo 가 타겟을 순차 실행하므로 "binary 당 1 개" 는 설계상 허용값이라 PID 수만으로 위반과 정상을 구분할 수 없으며, (c) 개발자 머신에 실사용 tasty 가 떠 있으면 그대로 오탐이다. 동적 관찰은 수동 검증 절차로만 남긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 전역 상태(PTY registry / `global_hook` / notification)를 workspace 스코프로 조회하는 IPC 가 생겨, 목록 검증의 `any` 제약이 불필요해질 때.
- test binary 수가 늘어 "바이너리당 1 개" 하한만으로는 총 인스턴스 수를 감당할 수 없을 때 — 대안 C(파일 통합)를 다시 저울질한다.
- 공유 인스턴스 위에서 병렬 실행이 재현 불가능한 간섭을 만들어, 직렬화(`MutexGuard` 반환)나 프로세스 분리가 실측으로 더 싸다고 판명될 때.
- CI 러너가 창 없는 환경으로 통일되어 "GUI 창이 OS 포커스를 훔친다" 는 전제 자체가 사라질 때.

## References

- [`docs/dev-guide/e2e-tests.md`](../dev-guide/e2e-tests.md) — 원칙의 운영 상태(공유 하네스 API, 예외 2 종, 전역 상태 주의사항)
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — 점유 레지스트리(workspace/surface 단위 lock)
- [`docs/identity.md`](../identity.md) — 원칙 1·3(에이전트 행동이 사용자 활성 상태에 닿지 않음)
- `tests/common/mod.rs` — 공유 진입점 `shared()` / 격리 헬퍼 `create_workspace()`
- `tests/e2e_single_instance_guard.rs` — 본 결정의 CI 집행
