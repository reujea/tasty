# E2E 테스트 — 인스턴스 공유 + 격리 + timeout 정책

`tests/e2e_tests.rs` 를 비롯한 e2e 테스트는 실 tasty 바이너리를 spawn 하여 IPC 로 조작한다. `tests/common/mod.rs` 가 공통 하네스이며 진입점이 둘이다 — 공유 인스턴스 `common::shared()`(기본)와 전용 인스턴스 `TastyInstance::spawn`(예외). 자체 검증 절차는 [self-verification.md](self-verification.md), debug 전용 IPC 는 [debug-ipc.md](debug-ipc.md).


## 0. 전제: plugin 바이너리 최신화 (필수)

번들을 사용하는 E2E를 개별 실행하기 전에 `cargo build --workspace`를 실행하거나
처음부터 `cargo test --workspace`를 사용한다. 패키지 한정 test는 플러그인 bin을 모두
빌드하지 않는다. 개발 번들은 매니페스트와 바이너리를 각각 복사하므로 옛 바이너리에 새
매니페스트가 붙을 수 있다. 실패 시 stderr의 version drift와 identity drift를 확인한다.

spawn_diag::staged_bundle_note는 번들 사용 스위트의 실패 메시지에 바이너리가 0개일 때
빌드 안내를 붙인다. 부분 스테이징이나 모든 버전 불일치를 검출하는 검사는 아니다.
플러그인을 쓰지 않는 스위트의 정상 실행을 막지 않도록 부팅 전 강제 실패 대신 진단에 사용한다.

## 0-1. 어느 바이너리를 띄우는가

기본은 CARGO_BIN_EXE_tasty이며 테스트와 같은 feature로 빌드된다.
GUI 바이너리는 창·GPU 초기화 뒤 IPC를 시작하고, headless 데몬은 창·GPU를 만들지 않는다.
바이너리 선택은 spawn_diag::instance_bin을 공유한다.

`scripts/build-e2e-headless.sh` 또는 `just e2e-headless-bin`으로 별도의 headless 데몬을
준비할 수 있다. 스크립트가 빌드에 실패하거나 낡은 결과를 발견하면 경로를 출력하지 않는다.
override를 넘기지 않았을 때의 기본 실행 통과가 headless override 사용을 증명하지는 않는다.

TASTY_E2E_BIN은 스위트 분류에 따라 적용된다. DaemonKind::SameCombo는 자기 feature의
데몬을 유지하고 HeadlessOk만 override를 받는다. 테스트 쪽 cfg뿐 아니라 데몬의 호출 경로가
다른 경우도 SameCombo다. attach_structure_sync_loopback이 그 예다.
HEADLESS_OK_SUITES와 EXPECTED_INSTANCE_TESTS는 양방향으로 대조하며 미분류는 SameCombo다.

override의 소스 최신성은 하네스와 빌드 스크립트가 모두 검사한다. src/crates의 Rust 소스
mtime이 바이너리보다 새롭거나 같으면 거부한다. 동률은 빌드 순서를 알 수 없기 때문이다.
mtime을 읽지 못하는 파일은 두 검사 모두 놓치는 한계가 있다.

```bash
CARGO_TARGET_DIR=target-e2e-headless cargo build --no-default-features --workspace
TASTY_E2E_BIN=$PWD/target-e2e-headless/debug/tasty cargo test --test shared_instance_harness
```

기본 target/debug/tasty와 별도 경로를 사용해야 다음 GUI test 빌드가 override를 덮어쓰지 않는다.
플러그인도 필요한 target에 빌드한다. 번들은 exe 옆 builtin-plugins에서 찾고, 없으면 exe 위치로
개발 저장소를 찾아 구성한다. 저장소 밖 target에서는 이 추론이 실패할 수 있으므로 번들을
명시적으로 준비해야 한다. 복사한 번들은 자동 갱신되지 않아 플러그인을 바꾸면 다시 준비한다.
현재 빌드 스크립트의 스테이징 절차를 우선 사용한다.

namespace 첫 호출은 활성 소유자와 필요한 IPC hook extension의 기동을 기다린다.
소속은 매니페스트로 먼저 확인한다. -32601이면 이름·설치를, -32002이면 비활성 상태나
기동 실패를 조사한다. 단순히 첫 호출이니 무시하지 않는다.

headless override는 GUI 자원 경쟁을 줄일 수 있지만 CPU·IPC·디스크 부하로 인한 간헐 실패까지
없애지는 않는다. 실제 GUI 또는 조합별 계약을 검사하는 스위트는 그대로 해당 조합을 사용한다.
GUI 테스트 바이너리와 headless 데몬의 혼합을 동종 조합의 검증으로 보고하지 않는다.
워크스페이스 feature 통합도 산출물에 영향을 주므로 별도 빌드를 CI와 바이트가 같은 바이너리로
가정하지 않는다. 창이 필요해 headless 조합에서 실행할 수 없는 테스트는 `#[cfg(feature = "gui")]`로
그 조합에서 컴파일하지 않고, 헤드리스 CI 스텝에서 이름으로 건너뛰지 않는다([유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)).

## 1. 인스턴스 공유 원칙 (필수)

**tasty 인스턴스는 test binary 당 1 개가 기본이고, 격리는 프로세스가 아니라 workspace 단위로 한다.** 새 e2e 테스트를 쓸 때 인스턴스를 새로 띄우지 말고 `common::shared()` 를 받아 `create_workspace()` 로 자기 workspace 를 만들어라. 결정의 근거·대안·재검토 조건은 [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md).

**왜 인스턴스를 아끼나**

- tasty 는 GUI 앱이라 뜰 때마다 창이 생기고 **OS 포커스를 훔친다** — 개발자가 다른 창에서 작업 중이면 테스트가 키 입력을 가로챈다.
- 창 spawn/kill 자체가 dev 프로필 기준 수 초다. `#[test]` 마다 띄우면 그 비용이 테스트 수만큼 곱해진다.

**왜 workspace 로 격리해도 되나**

- attach 점유는 `OccupancyRegistry` 의 `surface_locks` / `workspace_locks` — **workspace/surface 단위 lock** 이다([attach-behavior.md](attach-behavior.md)). 서로 다른 workspace 를 잡는 테스트들은 한 인스턴스 위에서 병렬 공존한다.
- IPC 로 만든 workspace 는 `IntentOrigin::Agent` 라 active 를 전환하지 않는다([포커스 정책](../design/policies/focus.md)). 여러 테스트가 병렬로 만들어도 서로의 활성 상태를 흔들지 않는다.
- `workspace.create` 응답이 `id` 와 `surface_id` 를 함께 주므로 한 번의 호출로 격리 단위 전체를 얻는다.

**공유 범위는 test binary 단위다 — `cargo test` 전체가 아니다.** `OnceLock` 은 프로세스 로컬 정적 상태이고 cargo 는 test 타겟마다 별도 프로세스를 띄우므로, 이 구조로 도달 가능한 하한은 *바이너리당 1 개*다. 이걸 "저장소 전체 1 개" 로 오해하면 인스턴스 총량을 잘못 계산하게 된다. 총량을 더 줄이려면 test binary 개수 자체를 줄여야 한다.

**전용 인스턴스가 정당한 예외는 세 종류뿐이다** — 모두 *프로세스 경계 자체가 검증 대상*인 경우다.

| 예외 | 사례 | 이유 |
|------|------|------|
| 기동 시점 config 이 달라야 함 | `spawn_with_inherit_cwd(true)` — `tests/attach_convert_cwd_loopback.rs` · `spawn_with_env(&[("TASTY_DEBUG_IPC_ROUND_TIME_BUDGET_MS", "0")])` — `tests/e2e_tests.rs` 의 `concurrent_requests_are_all_answered_when_every_round_is_cut` | `inherit_cwd` 는 격리 HOME 의 `config.toml` 에 미리 쓰는 값이라 이미 떠 있는 인스턴스에 런타임으로 바꿔 끼울 수 없다. debug 전용 회차 시간 예산([debug-ipc.md](debug-ipc.md))도 부팅 뒤 한 번 읽는 환경 변수라 같은 이유다 — 공유 인스턴스에 켜면 다른 시험이 모두 잘린 회차 위에서 돈다 |
| 프로세스 자원을 외부에서 측정 | `tests/soak_memory.rs` | 프로세스 트리 RSS 를 `pid()` 로 밖에서 잰다. 다른 테스트의 활동이 섞이면 측정이 무의미해진다 |
| 연결 상대가 다른 프로세스여야 함 | `tests/remote_attach_token_caller.rs` | mirror attach 는 자기 자신을 거절하므로 서버를 받는 쪽과 다른 프로세스로 띄운다. 받는 쪽은 공유 인스턴스를 쓴다 |

웹훅 하네스(`tests/webhook_common/mod.rs`)는 인스턴스마다 `TASTY_HOME`/`webhooks.toml` 을 시딩해야 해서 공유 진입점이 없고, 재시작 영속성 테스트는 같은 HOME 을 물려받는 두 번째 인스턴스가 검증 대상 그 자체다.

### 1-1. 시나리오 하나에 `#[test]` 하나

각 시나리오를 별도 `#[test]`로 만들고 common::shared와 create_workspace로 격리한다.
한 시나리오의 실패가 뒤 시나리오를 실행하지 못하게 하거나 파일 전체를 skip하게 만들지 않는다.
실제 창이 필요한 단언은 해당 테스트에 모아 테스트 단위로 제외할 수 있게 한다.

**따라오는 제약 — 전역 목록 위에서는 길이 산술을 쓰지 않는다.** 테스트는 병렬로 도는데
`pane.list` / `workspace.list` / `hook.list` / `pty.list` / notification 은 workspace 로
격리되지 않는 전역 목록이라, `before + 1` 같은 델타는 다른 테스트가 동시에 만들고 지우면
깨진다. **"내 것이 있는가/없는가"**(`any` / `all`) 로 쓰거나, 세야 한다면 자기 workspace 로
필터한 뒤 센다.

**선례**: `tests/gui_common/mod.rs` 는 `OnceLock` + `atexit` 기반 공유 인스턴스와 "테스트마다 자기 workspace" 전략을 이미 구현해 둔 참조 구현이다. 다만 그것을 쓰는 `gui_tests.rs` 는 전수 `#[ignore]` 라 `cargo test --workspace` 에서도 실행되지 않는다 — 어느 채널에도 한 번도 걸리지 않았고, 선례로 보이지도 않았다.

**집행**: 이 원칙은 `tests/e2e_single_instance_guard.rs` 가 강제한다 — 통합 테스트라 **컴파일은 두 조합 모두 자동, 실행은 헤드리스 조합에서만 자동**이다(기본 조합의 전체 스위트는 자동 채널이 없다). 조합별 실태와 단서(`paths-ignore` 등)는 [ci-gates](ci-gates.md) 가 정본이고 여기 복제하지 않는다. 세 축을 본다 — ① 파일당 전용 spawn 호출 수(`DEDICATED_SPAWN_MARKERS` 에 든 생성자만 센다 — `spawn_with_env`·`spawn_with_restore_layout` 은 안 센다. 미등록 파일은 0 회, 예외는 `ALLOWLIST_FILES` 에 이유와 함께 등록), ② 인스턴스를 띄우는 test 파일 목록 고정(`EXPECTED_INSTANCE_TESTS`), ③ 바이너리 선택이 §0-1 의 한 곳을 거치는지(`BIN_SELECTION_ALLOWLIST` — **면제는 파일 통째가 아니라 횟수까지 묶는다**). ②가 필요한 이유는 파일당 spawn 을 아무리 조여도 binary 가 늘면 총량이 다시 증가하기 때문이다. 실행 중 tasty PID 개수를 세는 **동적 가드는 일부러 쓰지 않는다** — 테스트 밖에서 실행 중인 사용자 프로세스까지 세면 공유 하네스의 위반 여부를 구분할 수 없기 때문이다.

## 2. 공유 하네스 (`common::shared()`)

`common::shared()` 는 **호출한 test binary 하나가 공유하는** tasty 인스턴스를 돌려준다. 첫 호출에서만 프로세스를 띄우고 이후 호출은 같은 `&'static` 핸들을 준다.

| 축 | 정책 | 이유 |
|----|------|------|
| 공유 범위 | test binary 당 1 개 | §1 참조 |
| 직렬화 | **안 한다** — lock 없이 `&'static` 만 공유 | IPC 서버는 연결마다 별도 스레드로 받아 mpsc 로 큐잉하므로 동시 호출이 안전하다. (`gui_common::shared()` 가 `MutexGuard` 로 완전 직렬화하는 건 실제 데스크톱 마우스/포커스를 뺏는 입력 주입을 쓰기 때문이고, 이쪽은 IPC 전용이라 해당 없음) |
| 테스트 격리 | `TastyInstance::create_workspace()` 로 테스트마다 자기 workspace | IPC 생성은 `IntentOrigin::Agent` 라 active 를 전환하지 않고([포커스 정책](../design/policies/focus.md)), attach 점유도 workspace/surface 단위 lock 이라 서로 다른 workspace 는 병렬 공존한다 |
| 정리 | `Drop` 이 아니라 `atexit` | 정적 저장이라 `Drop` 이 영원히 돌지 않는다. atexit 가 graceful `system.shutdown` → force kill → port file·격리 HOME 삭제를 수행한다. `Drop` 은 전용 인스턴스 경로로 그대로 남는다. 다만 Drop/atexit 둘 다 프로세스가 강제로 죽으면 실행되지 않는다 — 그 구멍은 §2-1 참조 |
| spawn 실패 | 첫 실패 후 **재시도하지 않는다** | `OnceLock::get_or_init` 은 초기화 클로저가 panic 하면 미초기화로 남아 다음 테스트가 그대로 재시도한다 — 부팅 timeout 상황에서 테스트 수만큼 GUI 프로세스가 더 뜨는 증폭을 막는다. 핸들이 서기 **전**의 패닉(S1 timeout · 조기 종료 · 입력 장치 생성 실패 등)은 `Drop` 을 못 부르므로, 그 구간의 자식은 `spawn_diag::ChildReaper` 가 소유해 되감기에서 kill 하고 거둔다(`Child` 의 Drop 은 kill 하지 않아 그냥 두면 orphan 이 된다). 세 하네스(`common`·`webhook_common`·`gui_common`) 모두 같은 타입을 거친다. 회수기 자체의 계약은 `spawn_diag` 의 단위 시험 둘이 잰다(패닉하면 죽이고 거둔다 · 넘기면 산 채로 넘긴다). 하네스가 그것을 실제 갈래에서 쓰는지는 시험이 없다 — 재는 법은 §2-1 끝 |
| 요청 재전송 | `call()`·`call_raw()` 는 아무것도 보내지 못한 **연결 실패만** 다시 시도한다(최대 3 회). 요청을 보낸 뒤 응답을 못 받으면 다시 보내지 않고 실패한다 | 응답이 늦었을 뿐 요청은 실행됐을 수 있다. 다시 보내면 `workspace.create` 가 두 번 실행되고, `pty.attach_surface` 재전송은 이미 입양된 PTY 를 찾지 못해 `headless pty … not found` 로 실패한다. 중복 요청은 직렬 journal worker 의 대기열도 늘려 뒤 요청을 더 늦춘다 |
| 응답 대기 상한 | 요청 한 건당 `RESPONSE_WAIT`(60 초) | 응답 속도를 단언하는 값이 아니라 멈춘 인스턴스에서 시험을 끝내는 값이다. 구조를 바꾸는 요청은 journal worker 가 한 스레드에서 단계마다 동기 commit(WAL · `synchronous=FULL`)을 하므로 저장 장치의 fsync 지연이 그대로 쌓인다. 다른 프로세스가 동기 쓰기로 디스크를 채운 상태에서 단계 하나가 수 초, 공유 인스턴스의 `workspace.create` 응답이 33 초까지 걸린 것을 측정했다(2026-10-16, Linux 개발 머신 한 대에서 측정). attach holder 가 보낸 구조 요청도 같은 commit 을 거치므로 `attach_structure_sync_loopback` 의 구조 프레임 대기(`STRUCTURAL_WAIT`)도 이 값을 쓴다 |

격리 헬퍼가 돌려주는 `TestWorkspace` 는 `workspace.create` 응답의 `id` / `index` / `surface_id` 를 그대로 담는다. 공유 경로에서는 `first_surface_id()` / `first_pane_id()`(목록의 `[0]` 번째를 집는다 — 전용 인스턴스 전용) 대신 `first_surface_id_in_workspace()` / `first_pane_id_in_workspace()` 를 쓴다. 갓 만든 workspace 의 PTY 가 필요하면 `wait_for_shell()` 로 첫 프롬프트를 기다린다.

**workspace 로 격리되지 않는 전역 상태**: headless PTY(`pty.*`), `global_hook.*`, notification 은 전역 목록이라 같은 binary 의 다른 테스트가 만든 항목까지 함께 조회된다. 공유 인스턴스 위의 목록 검증은 "내 것이 있는가"(`any`) 형태로 쓰고 길이나 `[0]` 번째를 assert 하지 않는다. surface hook(`hook.unset`)과 headless PTY(`pty.kill`)는 인스턴스가 test 프로세스와 함께 죽으므로 회수가 필수는 아니지만, 같은 binary 의 후속 테스트를 오염시키지 않도록 만든 테스트가 회수하는 것을 기본으로 한다. workspace 자체는 회수하지 않는다 — 인스턴스가 test 프로세스와 함께 죽으므로 회수할 이유가 없다.

`attach_*` test binary 들이 쓰는 attach 스트림 frame/handshake 헬퍼(`read_frame` / `write_control_frame` / `open_workspace_attach` / `open_surface_attach` / `open_stream_without_attach` / `wait_for_control_event`)는 **`tests/attach_common/mod.rs`** 한 곳에 있다 — `tests/common`(인스턴스 하네스)·`tests/webhook_common`(웹훅 하네스)과 같은 층위의 세 번째 공유 test 모듈이다. 개별 `#[test]` 파일끼리는 서로 `mod` 할 수 없지만 디렉토리 모듈은 여러 test binary 가 각자 `mod attach_common;` 으로 가져갈 수 있으므로, 파일마다 복제하지 않는다. 이 모듈에는 "첫 workspace 를 집는" 헬퍼를 두지 않는다 — 공유 인스턴스 위에서 그 습관이 남으면 남의 격리 단위를 밟는다.

하네스 자체 검증은 `tests/shared_instance_harness.rs` — 공유 재사용(spawn 횟수 1 · 동일 port), workspace id 유일성과 그것이 `workspace.list[0]` 이 아님, 전역 `pty.list` 의 `any` assert 가 병렬/`--test-threads=1` 양쪽에서 통과하는지를 확인한다. 같은 타깃의 `call_tests` 는 시험이 소유한 loopback 서버로 요청 교환(`common::exchange`)을 검사한다. 대기 상한보다 늦게 답하는 서버가 요청을 한 번만 받는지, 상한 안의 늦은 응답은 그대로 돌려주는지 본다. 응답을 못 받으면 재전송하던 이전 동작에서는 서버가 같은 요청을 3 번 받아 첫 시험이 실패한다.

### 2-1. 강제 종료 시 자식 정리 — `PR_SET_PDEATHSIG` (Linux)

위 표의 "정리"(Drop/atexit)는 **test 프로세스가 정상적으로든 panic 으로든 unwind 하며 끝날 때만** 동작한다. test 프로세스 자체가 `SIGKILL` 등으로 즉사하면(예: CI 러너 timeout, 셸 도구의 강제 종료) Drop 도 atexit 도 실행되지 않아, 이미 spawn 된 tasty 자식이 영구히 orphan 으로 남는다 — 실제로 이 경로로 leak 된 프로세스가 발견된 적이 있다.

`tests/spawn_diag/mod.rs` 의 `spawn_child()` 가 이 구멍을 막는다: 자식에 `prctl(PR_SET_PDEATHSIG, SIGKILL)` 을 걸어, 이를 설정한 부모 스레드가 종료되면 커널이 자식에 SIGKILL을 보내게 한다(Linux 전용 — `#[cfg(target_os = "linux")]`, 다른 OS에서는 이 보호 없이 `Command::spawn()`을 사용). 세 하네스(`common`·`webhook_common`·`gui_common`)가 모두 이 함수로 띄운다.

**함정 — PDEATHSIG 는 프로세스가 아니라 스레드에 묶인다** (`man 2 prctl` 경고: "the parent ... is considered to be the thread that created this process"). `common::shared()` 의 최초 호출자는 자기 테스트가 끝나면 죽는 cargo test 워커 스레드라, naive 하게 호출 스레드에서 그대로 fork 하면 **그 스레드가 죽는 순간 공유 인스턴스까지 죽어** 이후 다른 스레드에서 도는 나머지 테스트가 전부 "Connection reset" 으로 깨진다(호출한 테스트 워커의 종료가 공유 인스턴스 수명에 영향을 주기 때문이다). 그래서 실제 fork 는 **프로세스 수명 동안 파킹만 하는 전용 스레드**에서 수행한다 — 커널이 추적하는 "부모 스레드" 를 프로세스 수명과 맞추는 것이 핵심이다. 이 하네스를 고칠 때 fork 지점을 다시 호출 스레드로 되돌리지 말 것.

**두 회수 경로가 하네스에서 실제로 도는지는 자동 채널이 없다.** 둘 다 실패하는 부팅이나 죽는 바이너리에서만 드러나고, 그 조건을 만드는 잡이 없다. 손으로 재는 법 — 대조 팔(회수 코드를 뺀 사본)과 같이 돌려야 값이 된다:

- **핸들 전 패닉**(`ChildReaper`): 웹훅·범용 하네스는 `TASTY_E2E_BIN` 을 포트 파일을 안 쓰고 자기 pid 를 파일에 적은 뒤 오래 자는 셸 스크립트로 덮고, override 를 받는 스위트에 `catch_unwind` 로 spawn 을 감싼 임시 시험을 넣어 패닉 **뒤에** 그 pid 가 사는지 본다. 바이너리가 끝나면 PDEATHSIG 가 어차피 죽이므로 반드시 **같은 프로세스 안에서** 물어야 한다. GUI 하네스는 바이너리를 덮을 수 없어 부팅 상한 상수를 0 으로 바꾼 사본을 전용 Xvfb 에서 돌리고 `/proc/*/environ` 의 격리 `TASTY_HOME` 접두(`tasty-gui-test-home-<시험 pid>-`)로 남은 프로세스를 센다.
- **바이너리 즉사**(`spawn_child`): 부팅에 성공한 뒤 시험이 자기에게 `SIGKILL` 을 보내게 하고, 자식 pid 가 남는지 본다. `xvfb-run` 은 끝나며 X 서버를 내려 GUI 를 함께 죽이므로 이 물음을 가린다 — Xvfb 를 직접 띄우고 그 PID 를 저장해 두었다가 확인 뒤에 내린다.

실측 2026-09-14(Linux, 대조 팔 = 회수 코드가 없던 사본): 웹훅 하네스 상한 초과 뒤 가짜 데몬 생존 `true` → `false`, GUI 하네스 상한 초과 뒤 생존 프로세스 2 → 0, GUI 하네스 바이너리 `SIGKILL` 뒤 창 생존(부모가 init 계열로 바뀐 고아) → 소멸.

### 2-1-1. 도는 인스턴스를 찾을 때 port 파일을 **글롭으로 잡지 마라**

임시 디렉터리에는 이전 실행의 포트 파일이 남을 수 있다. 파일 존재나 mtime만으로
현재 검증 인스턴스를 고르지 않는다. 실행 직후 기록한 PID와 격리 홈을 사용하고,
하네스의 홈·포트 파일 접미사를 대조한다. 이번 실행과의 소유 관계를 확인하지 못한
인스턴스에 조작을 보내거나 종료하지 않는다.

### 2-2. 웹훅 포트 선택 (`webhook_common`)

웹훅 리스너의 포트는 **기동 시점 설정값**이라(`TASTY_HOME/webhooks.toml` 의 `port = N`) 하네스가 미리 정해 시딩해야 한다. 그 순간부터 자식이 실제로 bind 할 때까지 아무도 그 번호를 지키지 않으면, 같은 실행의 다른 테스트·다른 워크트리·무관한 프로세스가 가져갈 수 있다. 두 장치로 막는다.

- **예약(`PortLease`)** — `free_port()` 는 번호만 주지 않고 `TcpListener` 를 살린 채 돌려준다. 예약은 `Command::spawn` 직전에만 풀린다. 번호만 빼내 버리는 형태를 타입이 막는다.
- **재시도** — 예약을 풀고 자식이 부팅을 마칠 때까지의 구간(수 초)은 예약으로 닫을 수 없다(같은 포트를 두 소켓이 동시에 listen 할 수 없다). 이 구간에서 뺏기면 리스너가 남긴 bind 실패 경고를 근거로 해당 포트의 bind 실패를 확인하고, 새 포트로 다시 띄운다(최대 2 회).

재시작 시나리오(같은 홈의 두 번째 인스턴스)는 재시도 대상이 아니다 — 웹훅 URL 이 재시작 간 고정이어야 해서 하네스가 번호를 바꿀 수 없다. 그래서 전용 진입점 `WebhookInstance::builder_for_restart()` 는 포트를 인자로 받지 않고 홈의 `webhooks.toml` 을 기준으로 읽는다. 호출부가 번호를 따로 들고 다니면 1 차 인스턴스가 재시도로 포트를 바꿨을 때 그 값이 조용히 낡는다.

포트를 뺏겨 실패할 때의 메시지는 "웹훅이 안 떴다" 가 아니라 선택한 포트와 리스너 경고 원문을 싣는다. bind 실패 경고가 없는 경우(부팅 지연·리스너 init 미호출)와 문구로 구분된다.

## 3. 환경 격리

인스턴스마다 `$TMPDIR/tasty-test-home-{pid}-{nanos}/` 를 새 HOME 으로 만들고, host 환경 누수를 spawn 직전에 차단한다. ("인스턴스마다" 이지 "테스트마다" 가 아니다 — §1 참조.)

| env | 처리 | 이유 |
|-----|------|------|
| `HOME` / `ZDOTDIR` | 격리 HOME override | zsh rc 위치 격리 (macOS/Linux 의 `~/.tasty/` 격리도 겸함) |
| `TASTY_HOME` | 격리 `.tasty` 로 명시 | tasty 루트 해석은 `directories::BaseDirs`(=Windows 는 USERPROFILE) 기반이라 **HOME 만으로는 Windows 에서 격리되지 않는다** — 실사용자 `~/.tasty-debug` 세션 복원이 새어든다. `TASTY_HOME` 이 루트 override 의 SoT |
| `SHELL` | 제거 | host login shell 누수 차단(`detect_bash` 의 `$SHELL` 경로) |
| `OH_MY_ZSH` / `ZSH` | 제거 | oh-my-zsh customization 누수 차단 |
| `TASTY_SURFACE_ID` | 제거 | 부모가 tasty 안일 때 augmented-help 분기 차단 |
| `TASTY_DEBUG_OS_OPEN_LOG` | 격리 홈 아래 `os-open.log` 로 지정. 정의 자리는 `tests/spawn_diag` 의 `apply_os_open_record` 하나이고 세 하네스가 부른다. 범용 하네스는 `os_open_log()` 로 그 경로를 준다 | 자식의 OS 열기(브라우저 · 파일 관리자)가 **실행자의 이미 떠 있는 브라우저**로 URL 을 넘긴다 — 격리 HOME 도 `TASTY_E2E_DISPLAY` 도 그 채널을 못 막는다. 이 값 아래에서 자식은 띄우지 않고 기록만 한다([debug-ipc.md](debug-ipc.md), [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)). debug 스위치라 release 로 지은 자식은 무시한다. `e2e_tests` 의 `directory_dispatch_is_recorded_instead_of_opened_under_the_harness`(gui 조합 · debug 빌드)가 기록을 단언한다 |
| `BROWSER` (unix) | 격리 홈 아래 `os-open-browser.sh` — 받은 인자를 `os-open.log` 에 `BROWSER\t<인자>` 로 적기만 한다. 같은 `apply_os_open_record` 가 준다. 경로에 공백·`:` 가 있으면 `true`, 스크립트를 못 쓰면 하네스가 선다 | tasty 가 띄운 다른 프로세스(PTY 셸 · plugin)가 스스로 여는 것은 위 스위치 밖이다(번들 markdown 의 외부 링크는 host 를 거쳐 스위치 안이다 — [플러그인 제작 가이드](plugin-development.md)). `webbrowser` 는 Linux·BSD 에서 `BROWSER` 를 먼저 보고 성공하면 멈춘다 — macOS·Windows 는 이것으로 안 막힌다. 빈 `BROWSER=` 는 xdg desktop entry 를 직접 실행하므로 쓰지 않는다([유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)). 빌드 프로필과 무관하게 준다 |
| `DISPLAY` / `WAYLAND_DISPLAY` | **격리하지 않는다 — 이름을 요구한다.** linux 의 gui 조합에서 `TASTY_E2E_DISPLAY` 를 읽어 자식 `DISPLAY` 로 명시 전달하고(그때 `WAYLAND_DISPLAY` 는 제거), 값이 없으면 spawn 을 세운다. 아래 "어느 디스플레이에 뜨는가" | gui 데몬은 창을 만들고 그 창이 어디 뜨는지는 이 값이 정한다. 지우면 winit 이 즉사해 부팅 자체가 없다 — 다른 축과 성질이 다르다 |
| `TASTY_LOG` | 본체 기본 필터와 **같은 모양** (`warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off`, 웹훅 하네스는 뒤에 `,tasty::webhook::listener=info`). 정의 자리는 `tests/spawn_diag` 의 `LOG_ENV`/`LOG_FILTER` 하나다 | child stderr 폭주에 의한 OS pipe backpressure 회피 + host 의 `TASTY_LOG` 누수 차단. 본체가 읽는 변수는 `TASTY_LOG` 다 — `RUST_LOG` 는 무시된다([crash-diagnostics](crash-diagnostics.md)). **`warn` 한 단어만 주면 안 된다** — 지정하는 순간 본체 기본 필터가 통째로 대체돼 `wgpu_hal=error` 등 억제가 풀리고 로그가 오히려 늘어난다(실측: 미지정 7줄 · `warn` 12줄 · 이 값 7줄) |

### 어느 디스플레이에 뜨는가

**gui 조합의 데몬은 창을 만든다**(§0-1). 그 창이 어디에 뜨는지는 `DISPLAY` 가 정하는데,
하네스가 그것을 정하지 않으면 `Command` 가 부모의 값을 물려주므로 **실행자가 보고 있는
화면이 그대로 시험의 디스플레이가 된다.** 그 상태는 조용하다 — 시험은 통과하고 창만
사람 화면에 뜬다. 실측 2026-09-20: 전용 Xvfb `:77` 위에서 돌린 자식의
`/proc/<pid>/environ` 이 `DISPLAY=:77` 이었고 그 디스플레이에 `1280x720` 짜리
`Tasty (Debug)` 창이 떴다. 격리 `HOME` 은 이것을 막지 못한다 — X 서버가 로컬 사용자를
인증하면 `~/.Xauthority` 없이도 붙는다.

**그렇다고 지울 수는 없다.** 같은 바이너리를 `DISPLAY`·`WAYLAND_DISPLAY` 없이 띄우면
winit 이 `neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` 로 즉사하고
port file 이 안 써진다(§5 의 `NO_DISPLAY_MARKERS` 가 그 시그니처다). 디스플레이는
격리할 누수가 아니라 **필요한 입력**이다. 그래서 하네스는 격리 대신 **이름**을 요구한다
([유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)).

```
Xvfb :77 -screen 0 1920x1080x24 -nolisten tcp -ac &
XVFB_PID=$!
TASTY_E2E_DISPLAY=:77 cargo test --test e2e_tests
```

| `TASTY_E2E_DISPLAY` | 하네스가 하는 일 |
|---|---|
| 디스플레이 이름(`:77` 등) | 자식 `DISPLAY` 로 명시 전달 + `WAYLAND_DISPLAY` 제거 |
| `inherit` | 부모 값을 그대로 물려준다(변경 전 동작) — 창을 눈으로 보며 고칠 때 |
| 없음 / 빈 값 | spawn 을 세운다. 실패 문구가 위 두 길을 다 찍는다 |

요구가 서는 조건은 셋의 곱이다 — linux · `gui` feature · `TASTY_E2E_BIN` override 가 안
먹히는 스위트. 그래서 **헤드리스 조합은 이 변수를 요구받지 않는다**(`check-headless` 가
그대로 돈다). macOS/Windows 도 요구하지 않는다 — 거기서는 창이 언제나 사용자 세션에
뜨고, 줄 수 있는 다른 답이 없다.

**선언된 사각**: override 가 *gui* 바이너리를 가리키면 창이 뜨는데 요구가 안 선다. 경로만
보고 그 바이너리의 조합을 알 방법이 없어서다.

예제의 디스플레이 번호는 사용 중이지 않은 번호로 정한다. Xvfb 를 직접 띄웠으면 **저장한 PID와 소유를 확인한 뒤 회수한다** — `xvfb-run` 의 `$!` 는 래퍼라 안의
프로세스가 고아로 남는다([screenshot-methods](../ai-verification/screenshot-methods.md)).

### 번들 plugin 은 opt-in 이다

통합 테스트는 기본적으로 빈 `TASTY_BUILTIN_PLUGINS_DIR`를 사용한다.
플러그인 namespace를 호출하거나 플러그인이 제공하는 surface kind를 여는 테스트 바이너리만
`tests/spawn_diag`의 `SUITES_THAT_CALL_BUNDLED_PLUGINS` 목록에 넣는다.
`common`, `webhook_common`, `gui_common`을 포함한 부팅 harness는 `apply_bundle_opt_in`을 공유한다.

대상은 테스트 이름으로 추측하지 않는다. Explorer는 호스트 builtin이며 `webhook.*`도 호스트가 처리한다.
목록은 실제 호출·surface 종류를 기준으로 정한다.
플러그인이 필요한 테스트를 빠뜨리면 해당 호출이 실패해야 하며 목록과 harness 사용은 기존 검사가 확인한다.

선택은 테스트 함수가 아닌 바이너리 단위다. 프로세스별 OnceLock 공유 인스턴스가 있으므로
함수 안에서 결정하면 첫 실행 순서에 따라 번들 사용 여부가 달라진다.
목적은 불필요한 디스크 쓰기와 메모리 압박을 줄이는 것이며 테스트 실행시간 단축을 보장하지 않는다.

### 명부 안 스위트는 번들을 hardlink 로 받는다

번들이 필요한 테스트는 자식 프로세스를 띄우기 전에 격리 HOME의 `plugins/<id>/`를 hardlink로 채운다.
원본은 개발 번들이 아닌 harness 소유 스냅숏이다. 사용자 HOME으로의 제품 설치는 기존 복사를 유지한다.

- 스냅숏은 `target/<profile>/test-bundle-links/<서명>/`에 둔다. 경로·크기·mtime이 바뀌면 한 번 복사하고 이전 스냅숏을 정리한다.
- 번들 위치는 제품의 `bundle_root_from_exe_dir`로 구한다. `TASTY_BUILTIN_PLUGINS_DIR` override를 존중한다.
  `TASTY_E2E_BIN` override에서는 자식 프로필을 알 수 없어 미리 채우지 않는다.
- 처음 hardlink를 시도해 파일시스템이 다른지 확인한다. 스냅숏 생성 실패나 일부 링크 실패는 warning을 남기고
  나머지를 호스트의 기존 복사로 채운다. 통과한 테스트의 로그까지 보려면 `--nocapture`를 쓴다.
- 정확성은 호스트가 번들과 설치본을 바이트로 비교해 확인한다. 경로·크기·mtime 서명은 스냅숏 재사용 최적화다.
- 설치 파일을 제자리 수정하면 공유 inode가 바뀐다. 설치·갱신은 임시 파일+rename, 삭제는 unlink를 사용하고
  플러그인 영속 쓰기는 `TASTY_PLUGIN_DATA_DIR`로 보낸다. 설치 폴더가 자식 cwd이므로 상대경로 쓰기에도 주의한다.

개발 번들에 직접 링크하면 빌드의 덮어쓰기가 실행 중 테스트 파일을 바꾸거나 `Text file busy`를 만들 수 있다.
스냅숏이 오염돼도 개발 번들과 사용자 HOME으로 번지지는 않지만, 호스트가 내용을 고쳐 복사하는 비용이 다시 발생한다.
스냅숏은 빌드 트리에 남으며 `cargo clean`으로 정리된다. 쓰기 감소와 실행시간 변화는 별도로 측정한다.

격리 HOME에는 빈 `.zshrc`·`.bashrc`와 테스트용 config를 미리 쓴다.
config는 유효한 테스트 셸과 `restore_layout=false`를 지정해 초기 셸 설정 화면을 피한다.
초기 설정에 멈추면 discovery 파일이 만들어지지 않아 harness가 부팅 timeout으로 실패할 수 있다.

## 4. Timeout (2단계)

| 단계 | 상수 | 값 | 조건 |
|------|------|-----|------|
| S1 | `SPAWN_PORT_TIMEOUT` | 40 s | `--port-file` 에 port 가 쓰여짐 |
| S2 | `SPAWN_SHELL_TIMEOUT` | 20 s | first surface `screen_text` 가 non-empty (첫 PTY prompt) |

두 상수는 tests/spawn_diag에서 공통으로 관리하며 tests/common과 tests/webhook_common이
사용한다. 설정 준비와 부팅 상태를 확인하지 않고 시간 제한만 늘리지 않는다.
S2의 non-empty 화면은 첫 출력 관측이며 셸 명령 실행 가능성을 모두 증명하지는 않는다.

**상한을 다 기다리지 않는 경우**: port file 대기 루프가 매 바퀴 자식의 종료를 확인한다. 자식이 이미 죽었으면(디스플레이 부재·설정 오류 등 즉사 계열) 그 자리에서 실패시킨다 — 죽은 프로세스를 40 초 더 기다릴 이유가 없다.

## 5. stderr tail 진단

spawn timeout panic 시 child stderr 마지막 30 라인을 panic 메시지에 첨부한다. `Stdio::piped()` + background drain thread + 링버퍼(capacity 256)로 OS pipe buffer(Linux 64KB / macOS 16KB)가 차서 child 가 write block 되는 것을 방지. `TASTY_LOG` 로 verbosity 를 cap 한다(drain 1차 + cap 2차 방어) — 값은 §4 의 env 표대로 **본체 기본 필터와 같은 모양**이어야 한다. 30 줄짜리 tail 은 노이즈 몇 줄에도 밀려나므로, 필터를 느슨하게 주는 것이 곧 진단 손실이다.

GUI 하네스도 `StderrCapture` 를 `GuiTestInstance` 필드로 보관한다. 부팅 뒤 IPC
연결·송신·응답 해석 실패와 UI 조건 대기 만료는 원래 오류/상태와 함께 **지금까지 수집한**
stderr 꼬리를 낸다. 살아 있는 자식의 실패 진단에서 drain을 join하지 않는다.
전용 인스턴스의 `Drop` 은 자식 kill → wait → stderr join → 임시파일 정리 순서다.
공유 `static` 인스턴스는 Rust가 Drop하지 않으므로 기존 atexit 정리 경로를 사용하며,
그 경로의 drain 합류를 이 Drop 계약이 보장하지는 않는다.

`gui_common::stderr_tests` 는 실제 Tasty 대신 가짜 셸·IPC 서버로 부팅 후 진단과
자식 회수를 검사한다. Enigo 필드 생성에 디스플레이가 필요해 기본은 ignored이며,
Linux 격리 X 디스플레이에서 `cargo test --locked --test gui_tests gui_common::stderr_tests -- --ignored --test-threads=1` 로 실행한다. 데스크톱 입력은 보내지 않는다.
자동 실행 채널은 없다. 이 시험은 자식 회수와 꼬리 보존을 검사하며, drain의 명시적
join 호출 자체는 별도 계측 없이는 합류 누락과 빠른 자연 종료를 구분하지 못한다.

### 범용 하네스의 부팅 이정표

`tests/common`은 spawn 호출 직전의 `Instant` 하나를 기준으로 자식 PID와
`spawn_returned_ms`, `port_found_ms`, `first_ipc_response_ms`, `shell_ready_ms`를 기록한다.
첫 IPC 응답은 JSON 해석이 성공한 응답(error 응답 포함), shell-ready는 초기 surface의
첫 non-empty 화면 관측이다. 이후 IPC 호출과 새 workspace의 출력은 이 첫 시각들을
덮어쓰지 않는다. `pending`은 아직 관측하지 못한 다음 단계이며 원인 판정은 아니다.

이정표는 stderr ring 밖의 고정 슬롯에 보관한다. spawn/IPC 패닉의 unwind에서
`startup failure` 스냅샷을 다시 출력하므로, 후속 stderr가 30줄 tail을 밀어내도 초기
시각은 실패 출력에 남는다. `startup_diagnostics()`로 성공한 인스턴스의 값도 읽는다.
출력은 기존 `spawn_diag::init_test_tracing()`의 libtest writer를 사용한다. 이미 등록된
subscriber는 교체하지 않으므로 그 subscriber가 로그를 차단하면 수집되지 않는다.
새 ring, 전역 panic hook, 타임아웃 상향은 사용하지 않는다. 외부 SIGKILL이나 출력
수집기 자체의 유실은 unwind 진단이 보장하지 못한다.

`shared_instance_harness`의 `startup_tests`는 소유한 fake 자식과 loopback IPC로
포트 전 지연·포트 후 응답 지연·조기사망·응답 오류를 유발해 실제 공용 경로를 검사한다.
포트 전 지연은 자식의 준비 신호 → 부모의 포트 부재 확인 → 해제 신호 → 포트 공개로
동기화한다. 부모 관측 시작과 자식 진행은 별개이므로 포트 구간에 시간 하한을 두지 않는다.
부모가 늦게 실행되고, 공개 완료 신호까지 받은 뒤 부팅 관측을 시작하는 경우도 검사한다.
이 경우에도 실제 부팅 완료·이정표 순서·첫 응답 시각 보존을 단언한다. 준비 신호 전에
포트를 공개하는 변이는 포트 부재 단언으로 검출한다.
필터 실행: `cargo test --locked --test shared_instance_harness startup_tests -- --nocapture`.
실제 Tasty/GUI를 띄우는 같은 타깃의 다른 시험과 구분한다.

시각은 부모가 관측한 경과이며 자식 내부의 복사·bootstrap 시간으로 귀속하지 않는다.
GUI 내부 구간은 기존 `tasty::boot` trace를 별도로 읽는다. headless port 파일도 dispatch
준비 완료를 뜻하지 않는다. 내부 원인 분해에는 같은 실행의 추가 근거가 필요하다.
attention의 quiet window와 frame 대기에는 이 계측을 일반화하지 않는다. 재발한 시험의
handshake/raise/read/clear/quiet 경과와 frame tag가 없는 과거 총시간은 미귀속이다.

## 5-1. 마커 대기 만료 진단 (`tests/marker_wait`)

훅이 남기는 마커 파일을 기다리는 자리는 셋이다(`hooks_detection_e2e` · `hook_env_integration` ·
`webhook_integration`). 대기 함수는 `tests/marker_wait` 한 곳에 있고 세 타깃이 `mod` 로 함께 쓴다 —
같은 물음에 답하는 사본이 셋이면 하나를 고쳐도 나머지 둘은 안 고쳐진다.

**만료 메시지가 두 사건을 가른다.** 마커가 안 나온 것과, 마커는 나왔는데 이 폴링 루프가 굶어
못 본 것은 처방이 정반대인데 종전 메시지(`marker file … not written within 15s`)는 둘을 같은
말로 덮었다. 가르는 값은 **실제 확인 횟수**다 — 예산을 폴 간격으로 나눈 기대치와 비교해,
기대의 절반에 못 미치면 굶주림이라고 메시지가 직접 적는다. 그때 상한을 올리는 것은 처방이
아니다(폴링이 실행되지 못한 원인을 시간 상한 증가로 숨겨서는 안 된다 — [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md)).

메시지에 함께 싣는 것: 경과 · 예산 · 확인 횟수와 기대치 · 1 분 부하 · 호출자가 준 증거(선택).
**부하는 기록이지 판정이 아니다** — 실측에서 부하 평균은 지연을 예측하지 못했다(최대 지연이
낮은 부하 회차에서 났다).

## 6. Flaky 대응 절차

0. **panic 메시지 두 번째 줄의 판정문을 먼저 읽는다.** 하네스가 stderr 시그니처로 단서를 미리 갈라 놓는다. 단서마다 **확신 수준이 다르다** — 문장이 단정하는 것만 원인으로 받아들인다.
   - "디스플레이 서버가 없다 — 코드 인과가 아니다" → **단정.** 이 시그니처는 정상 부팅 stderr 에 나오지 않는다. `xvfb-run -a` 위에서 다시 돌린다.
   - "GPU 가속 경로 폴백 흔적이 있다 — 이것만으로는 원인 판정이 되지 않는다" → **단정이 아니다.** 아래 6-1 로 가되, 거기서 GPU 경합이 아니라고 판명되면 1~4 의 일반 절차를 그대로 밟는다. 이 줄을 봤다고 코드를 건너뛰지 않는다.
   - "부팅 차단 시그니처는 없다" → 아래 1~4 의 일반 절차로.
1. panic 의 `--- stderr (last 30 lines) ---` 확인.
2. 범용 하네스는 `startup failure`의 `pending`과 네 이정표로 포트 전·첫 응답 전·셸 출력 전을 구분한다. 다른 하네스는 실패한 호출 단계를 함께 읽는다. `IPC server listening` 줄의 부재는 기본 필터·tail 탈락으로도 생기고, 존재는 dispatch 준비를 보증하지 않으므로 그 한 줄로 S1/S2를 판정하지 않는다.
3. 재현: `cargo test --test e2e_tests -- --nocapture`.
4. 결정적 차단이 깨졌으면 `TastyInstance::spawn` 의 env/config 보강.


### 6-1. GPU 분기

renderD128·VK_ERROR_·DRI3·libEGL·tu_knl·failed to open device 같은 메시지는
소프트웨어 렌더링으로 전환한 뒤 정상 부팅한 경우에도 나타날 수 있다.
이 문자열만으로 GPU가 부팅 실패의 원인이라고 단정하지 않는다.

다른 GUI 검증과 자원 사용이 겹치는지 확인하고, 같은 커밋·명령을 단독으로 실행해
실패 단계와 이정표를 비교한다. 매번 다른 스위트가 실패하거나 단독으로 통과하면
경쟁을 의심할 근거가 되지만 원인을 확정하지는 못한다.
다른 GUI 인스턴스가 없어도 내부 GPU 작업·드라이버·다른 자원 문제가 남을 수 있다.
부팅 지연·플러그인·셸 설정도 실제 로그로 함께 조사한다.

새 시그니처를 원인 판정에 사용하려면 정상 실행에서도 나타나는지 먼저 확인한다.
현재 GPU_FALLBACK_MARKERS는 단서이며 확정 진단이 아니다.

## 7. WebView 배치 측정 시험 (ignored)

`e2e_tests`의 세 시험은 Linux X11에서 native WebView X 자식 창의 위치·픽셀·입력 영역을 읽어 WebView 배치를 잰다.

`webview_page_viewport_fills_its_native_window_under_gtk_scale_two`는 GTK 배율이 2일 때 html surface의 페이지 viewport가 native WebView 창과 같은 크기인지 잰다. 조건은 `GDK_SCALE=2`와 `WINIT_X11_SCALE_FACTOR=2`로 만든다. GNOME이 XSETTINGS(`Gdk/WindowScalingFactor`)로 배율을 정하는 경로는 이 시험이 재지 않는다. 페이지는 viewport 전체에 고정한 파란 테두리를 그리고, 시험은 WebView 창의 네 변 가운데 픽셀이 모두 테두리 색인지 확인한다. host가 GTK 크기·allocation에 GDK 배율로 나눈 논리 px를 주지 않으면 viewport가 창의 두 배로 잡혀 오른쪽·아래 변에 페이지 바탕색이 나오고 실패한다.

- `#[cfg(all(target_os = "linux", feature = "gui"))]`와 `#[ignore]`로 두어 기본 `cargo test`와 CI 워크플로에서 실행하지 않는다. CI의 GUI e2e 단계는 `multi_window_owner_routing`만 이름으로 실행한다.
- 이 시험은 창이 배율 2로 2560x1440이므로 그보다 큰 격리 Xvfb와 번들 plugin(html)이 필요하다. 자식 창이 화면 밖으로 나가면 시험은 픽셀을 읽지 않고 화면을 키우라는 오류로 끝난다.

```
Xvfb :<n> -screen 0 2600x1600x24 -nolisten tcp -ac &
XVFB_PID=$!
TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact \
  webview_page_viewport_fills_its_native_window_under_gtk_scale_two
```

Xvfb는 저장한 PID로 회수한다.

`webview_fills_its_surface_and_leaves_host_input_bands_to_the_host`는 html surface의 WebView X 창이 surface를 채우고, host 입력 띠만 입력 영역에서 빠졌는지 잰다. 기준값은 `debug.surface_rect`, 입력 영역은 SHAPE 확장의 `XShapeGetRectangles(ShapeInput)`이다. x11-dl에 SHAPE가 없어 시험이 `libXext.so.6`을 실행 시점에 연다. 포인터는 XTest로 움직인다.

- 분할 배치에서 WebView 창 = surface 사각형이다. WebView 왼쪽 끝(분할선 hit 띠)과 오른쪽 8 물리 px(창 리사이즈 밴드)은 입력 영역 밖이고 가운데는 안이다.
- 가운데 링크를 누르면 페이지가 바뀐다(띠 밖 입력은 페이지로 간다). WebView 왼쪽 끝을 눌러 끌면 분할선이 움직인다.
- 왼쪽 pane을 닫은 단일 pane 배치에서도 WebView 창 = surface 사각형이다. 사이드바와 닿는 왼쪽 끝은 입력 영역 안이고, 오른쪽 밴드는 밖이다.
- 창 리사이즈 자체는 창 관리자가 있어야 일어나므로 이 시험이 재지 않는다. 입력 영역에서 밴드가 빠졌는지만 확인한다.
- 포인터를 움직이므로 다른 GUI 시험과 같은 디스플레이에서 동시에 돌리지 않는다. 번들 plugin(html)과 기본 크기 Xvfb에서 실행할 수 있다.

```
TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact \
  webview_fills_its_surface_and_leaves_host_input_bands_to_the_host
```

`webview_lets_the_first_press_drag_the_pane_divider_below_it_at_scale_two`는 배율 2(`GDK_SCALE=2`, `WINIT_X11_SCALE_FACTOR=2`)에서 터미널 / html / 터미널 세로 3단 배치를 만들고, 창 관리자 없이 X 창을 1600x1000으로 바꾼 뒤 잰다.

- WebView 창 = surface 사각형이다. 아래쪽 7행(host 판정 `|y - d| < 8` 중 WebView 안쪽)은 입력 영역 밖이고 그 위 행은 안이다.
- 포커스가 없는 html 의 아래쪽 띠를 XTest 로 한 번 눌러 끌면 분할선이 움직이고 포커스된 surface 는 그대로다.
- 2560x1440 창에서는 egui 가 그 press 를 잡지 않아 드래그 중 이동 소비 결함이 드러나지 않았다. 그래서 창 크기를 리뷰에서 재현한 1600x1000으로 맞춘다.
- 2600x1600 이상 Xvfb 가 필요하다(기본 창 2560x1440 이 화면 안에 있어야 한다).

```
TASTY_E2E_DISPLAY=:<n> cargo test --locked --test e2e_tests -- --ignored --exact \
  webview_lets_the_first_press_drag_the_pane_divider_below_it_at_scale_two
```

## VTE 시뮬레이터 (`tasty-tui-simulator`)

터미널 동작 검증용 도구 — 고수준 명령을 raw VTE escape 시퀀스로 변환해 출력한다(터미널 입장에선 실제 TUI 앱과 같은 바이트 스트림). **인터랙티브 모드**(stdin REPL — 외부에서 `surface.send` 로 명령 단계 전송, 명령마다 `OK` 동기화)와 원샷 시나리오를 제공한다. 명령: cursor/print/sgr/fg·bg/altscreen/scroll-region/erase/raw/esc 등, 종료 제어 `quit`/`exit-code N`/`crash`(SIGABRT)/`panic`. debug 의 `debug.cell_info`/`debug.screen_attrs`([debug-ipc](debug-ipc.md))와 조합하면 셀 속성을 결정적으로 자동 검증할 수 있다.

로직은 `lib.rs` 에 있고 두 진입점이 공유한다(SoT 하나) — 독립 바이너리 `tasty-tui-sim`(`cargo build -p tasty-tui-simulator`, release 빌드 가능) 과 `tasty debug sim <subcommand>`(debug 빌드 한정). **debug 빌드에선 별도 빌드/PATH 설정 없이** `tasty debug sim ...` 으로 바로 호출할 수 있다(이미 `tasty` 가 PATH 에 있으므로). surface 안에서 stdout 에 직접 VTE 를 뿜는 로컬 동작이라 IPC 를 거치지 않는다. 자세한 명령 목록·부하 모드(`flood`)는 아래 [TUI 테스트 가이드](#tui-테스트-가이드--시뮬레이터--셀-검증--골든-스냅샷).

### TUI 테스트 가이드 — 시뮬레이터 · 셀 검증 · 골든 스냅샷

터미널 에뮬레이션 버그를 결정적으로 재현·검증하는 방법. E2E 격리/timeout 정책은 이 문서 위 §1–§6.

#### 원칙 — 재현 먼저

TUI 버그 발견 시: ① 최소 재현 VTE 시퀀스 특정 → ② `tasty-tui-sim` 에 시나리오 추가 → ③ `debug.cell_info`/`debug.screen_attrs` 로 정상 상태를 검증하는 E2E 테스트 작성 → ④ 버그 상태에서 테스트가 **실패하는지 확인** → ⑤ 수정 후 통과. "수정 먼저, 테스트 나중" 금지.

#### tasty-tui-sim (VTE 시뮬레이터)

`crates/tasty-tui-simulator/` — 고수준 명령("cursor 5 3", "bold", "print hello")을 raw VTE escape 로 변환해 출력한다. 터미널 입장에선 실제 TUI 앱과 동일한 바이트 스트림. 테스트 전용이 아닌 독립 도구(`cargo build -p tasty-tui-simulator`).

로직은 `lib.rs` 에 있고 두 진입점이 공유한다(SoT 하나):

- 독립 바이너리 `tasty-tui-sim` — release 빌드 가능. PATH 에서 어느 surface 든 실행(부하 테스트는 보통 release 본체 대상이라 이쪽).
- `tasty debug sim <subcommand>` — debug 빌드 한정. **별도 빌드/PATH 설정 없이** 바로 호출 가능(이미 `tasty` 가 PATH 에 있으므로). surface 안에서 stdout 에 직접 VTE 를 뿜는 로컬 동작(IPC 미경유). `tasty debug sim flood` 가 화면 갱신 부하(full-screen truecolor redraw)를 거는 스트레스 모드.

##### 인터랙티브 모드 (핵심)

서브커맨드 없이 실행하면 stdin REPL. E2E 가 `surface.send` 로 명령을 한 줄씩 보내 터미널 상태를 단계 구성한다. 시작 시 `READY`, 매 명령 후 `OK` 출력 → 테스트는 `wait_for_output("OK")` 로 동기화.

```bash
tasty-tui-sim   # = tasty-tui-sim interactive
```

##### 명령 카테고리

raw escape 직접 출력이 목적이라 crossterm/ratatui 같은 추상화는 쓰지 않는다. 전체 목록은 `crates/tasty-tui-simulator/src/lib.rs`. 주요 카테고리:

- **화면/커서**: `clear` `reset` `cursor <r> <c>` `cursor-{up,down,left,right} [N]` `cursor-save`/`-restore`
- **텍스트**: `print`/`println <text>` `newline` `cr` `tab` `bell`
- **SGR**: `sgr <params>` `bold` `italic` `underline[-double|-curly|-dotted|-dashed]` `underline-color` `strikethrough` `inverse` `dim` `blink[-rapid]` `invisible` `overline` `fg <N|r;g;b>` `bg <…>` + `*-off`
- **지우기/스크롤**: `erase-display [N]` `erase-line [N]` `scroll-region <top> <bottom>` `scroll-{up,down} [N]`
- **모드**: `altscreen-enter`/`-exit` `decset/decrst <mode>` `mouse-track[-motion|-all]` `size`
- **raw**: `raw <hex>` `esc <seq>`
- **프리셋**: `scenario {cursor,colors,attrs,unicode,scroll-region}`
- **종료**: `quit`/`exit` (`BYE`) `exit-code <N>` `crash`(SIGABRT) `panic`

원샷(`tasty-tui-sim cursor --row 5 --col 10 --exit` 등)은 수동 눈 확인용 — `--exit` 없으면 키 대기. E2E 는 인터랙티브 모드 사용.

#### 디버그 IPC (debug 빌드 전용, `#[cfg(debug_assertions)]`)

##### `debug.cell_info` — termwiz 파싱 단계 셀 속성

`tasty debug cell-info --row 0 --col 0`. termwiz `CellAttributes` 단계를 그대로 노출한다. **그려졌는지는 알 수 없다.** 색 계산은 `debug.glyph_color`로, 밑줄·취소선처럼 도형이 있는 속성은 `tasty screenshot`의 칠한 픽셀로 확인한다. 필드: `text` `fg`/`bg`(`default`/`palette:N`/`#rrggbb`) `bold` `italic` `underline` `strikethrough` `inverse` `width`(1/2) `intensity`(normal/bold/half) `underline_style` `underline_color` `blink` `invisible` `overline` `vertical_align`.

> `overline`/`underline_color`/`vertical_align`은 termwiz `AttributeChange`에 variant가 없어 SGR 파이프라인(`crates/tasty-terminal/src/vte_handler/control.rs`의 `map_sgr`)이 현재 펜을 복제해 `AllAttributes`로 넣는다. 다른 속성과 같이 셀에 전달된다. 그리는 속성은 [터미널 surface](../surfaces/terminal/index.md)의 문자 속성 행을 따른다.

##### `debug.glyph_color` — 렌더러가 GPU 에 push 하는 색

`tasty debug glyph-color --row 0 --col 0 [--bg-mode unfocused] [--surface 3]`. 응답: `in_bounds` `bg_mode`(focused/unfocused) `default_bg`/`bg`/`fg`(각 `{r,g,b,a,hex}`). `compute_cell_colors`(`src/cell_palette.rs`)가 GPU 인스턴스 색의 단일 출처 — 렌더러와 `debug.glyph_color` 가 같은 함수를 쓰므로 누락된 SGR 처리가 즉시 드러난다. (faint/dim 회귀: cell_info `intensity == "half"` → glyph_color 에서 fg 가 어둡게 적용됐는지 비교.)

##### 입력 시뮬레이션 (debug + `--enable-input-simulation`)

2단계 게이트: `#[cfg(debug_assertions)]` + `--enable-input-simulation` 플래그(없으면 "input simulation not enabled" 거부). `debug.inject_mouse`(SGR 마우스: surface_id/col/row/button/event_type) · `debug.inject_key`(text 또는 hex bytes).

#### E2E 테스트 패턴

```rust
tasty.set_mark(sid);
tasty.send_text(sid, "tasty-tui-sim\n");
tasty.wait_for_output(sid, "READY", Duration::from_secs(5));
tasty.send_text(sid, "print 한글\n");
tasty.wait_for_output(sid, "OK", Duration::from_secs(2));
let cell = tasty.call("debug.cell_info", json!({ "surface_id": sid, "row": 0, "col": 0 }));
assert_eq!(cell["text"], "한");
assert_eq!(cell["width"], 2);
tasty.send_text(sid, "quit\n");
tasty.wait_for_output(sid, "BYE", Duration::from_secs(2));
```

같은 프로세스에서 여러 단계 연속 수행 가능. 프리셋 시나리오의 기대 출력(cursor/colors/attrs/altscreen/unicode/scroll-region 의 행·열별 값)은 `tasty-tui-sim` 소스의 각 `scenario_*` 함수가 SoT — 검증값은 거기서 읽는다.

#### 골든 셀 그리드 스냅샷 (`cargo test`, headless)

E2E 와 달리 **GUI surface 없이** 도는 결정적 회귀 가드. `crates/tasty-terminal/tests/golden_grid.rs` 가 production VTE ingest 경로(`Terminal::new_detached` → `feed_bytes`, 실제 PTY 와 동일한 핸들러)에 `tasty-tui-sim` 의 고수준 명령에 대응하는 raw escape 를 먹이고, 결과 셀 그리드의 **결정적 텍스트 표현**을 골든으로 고정한다. `cargo test --workspace` 에 포함되어 별도 인프라 없이 돈다 — 기본 조합의 그 잡은 수동 전용이고, 자동 실행은 `check-headless` 잡에서만 일어난다([ci-gates](ci-gates.md)).

- `grid_text()` — 가시 그리드를 행당 한 줄로(후행 공백 trim, 빈 행 보존) 덤프. 레이아웃/커서/줄바꿈/스크롤/지우기 회귀용.
- `styled_cells()` — populated 셀별 SGR 속성(bold/italic/underline/inverse/strike + palette fg/bg)을 `"{row},{col} {glyph} {flags}"` 로 덤프. SGR 적용 회귀의 정식 가드(예: bold 처리 누락 시 `0,0 B bold` → `0,0 B -` 로 골든이 깨짐).

**커버 범위(의도적으로 좁게):**

- 커버: 커서 절대 위치/레이아웃, autowrap 줄바꿈, scroll-region(DECSTBM) 스크롤업, erase-display(CSI J), per-cell SGR 속성.
- **미커버**: GPU 픽셀 렌더(환경의존 → 골든 부적합, `debug.glyph_color` 로 검증), chrome/위젯 레이아웃(`src/view` — GUI 하니스 필요). 둘 다 수동 시각 검증(`docs/ai-verification/screenshot-methods.md#시각-판정-체크리스트`) 영역으로 남는다.

픽셀이 아닌 **텍스트**를 고정하는 이유: 그리드의 텍스트 표현은 OS/GPU 무관하게 안정적이라, 로직 회귀가 골든 한 줄을 뒤집어 실패시키되 픽셀 골든처럼 flaky 하지 않다.

#### 시나리오 추가

`crates/tasty-tui-simulator/src/lib.rs` 에 서브커맨드 + 함수 추가. `clear_and_setup()` 시작 → raw escape 직접 출력 → `finish(out, "{NAME}_TEST_DONE", exit)`.

#### 주의

- **ratatui 금지** — raw escape 직접 출력해야 "터미널이 시퀀스를 올바로 해석하는가"를 검증할 수 있다.
- 디버그 IPC 는 release 에 없다 — E2E 는 debug 빌드 전용.
- **shell ZLE/readline 함정**: E2E 하네스는 격리 config 에 `shell = "/bin/sh"` 를 박고 `SHELL` 을 지우지만(`tests/common/mod.rs`), 셸을 직접 띄운 surface 나 수동 검증 인스턴스는 사용자 로그인 셸을 쓴다(`GeneralSettings::detect_shell`). `Alt+X`(execute-named-cmd), `Ctrl+R`(history-search), `Ctrl+X Ctrl+E`(edit-command-line) 등은 prompt 를 바꿔 후속 명령을 오염시킨다. shell-disruptive 키는 별도 임시 surface 에서 보내고 닫거나, `Ctrl+G`(abort)로 reset 후 진행. 증상: stripped 출력에 글자 사이 `_` 나 BEL 다수면 ZLE incremental 모드에 갇힌 것.

## 관련

- [유닛 테스트 격리 — 사용자 환경을 읽지 않는다](unit-test-isolation.md) — 격리 단위를 workspace 로 정한 근거·대안·재검토 조건
- [self-verification.md](self-verification.md) — 커밋 전 시나리오 재현
- [attach-behavior.md](attach-behavior.md) — 점유 레지스트리(workspace/surface 단위 lock)
- [build.md](build.md) — dev/release/dist 프로필 (timeout 값 산정 근거)
