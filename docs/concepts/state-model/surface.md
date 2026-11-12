# Surface 상태 모델

busy·attention과 surface 점유처럼 상위 계층에도 표시되는 실행 상태는 surface ID별로만 저장되고, 상위 계층은 이 값을 모아 계산한다. 다른 계층도 자기 상태를 저장한다. Tab·Pane·Workspace는 구조와 이름 같은 자기 값을, View는 선택을 가지며, workspace 점유는 surface 점유와 별도의 workspace lock으로 저장된다([상태 모델](index.md#계층-사이의-관계)). 이 문서의 축은 kind와 관계없이 적용되는 것과 특정 kind에만 적용되는 것을 구분한다.

## 상태 축

| 축 | 값 | 적용 대상 | 소유자 | 저장 | 관측·파생 | 변경 주체 |
|---|---|---|---|---|---|---|
| 구조 leaf | 있음·없음. `SurfaceDescriptor`의 `id`·`kind` | 모든 kind | `CoreState` | journal | 확정 구조 | 생성·닫기·변환 명령 |
| 활성화 단계 | `ActivationPhase`의 7개 값과 generation | 활성화를 거치는 surface | journal의 surface 사실 | journal | 확정 사실 | 생성·복원 effect |
| 실행 객체 | `Surface` trait 구현체, 복원 자리, 없음 | 모든 kind | `EngineRuntime.surfaces` | 메모리 | — | effect 실행, 활성화 |
| PTY 연결 | 연결 있음(`ResourceGeneration`)·없음 | terminal | `EngineRuntime.terminals` | 메모리 | 실행 자원 | PTY 생성·교체·종료 |
| busy | 집합에 있음·없음 | 로컬 terminal, mirror surface | `LiveDomainState.busy_surfaces`, mirror는 원격 상태의 별도 집합 | 메모리 | PTY 관측, 원격 push | 시스템 폴링 |
| attention | 없음, `Completion`, `NeedsInput` | 모든 kind | `LiveDomainState.attention` | 메모리 | 사건 기록 | 발생원(완료 보고·훅·OSC 133 등), 로컬 확인, IPC 해제 |
| 점유 | 없음, soft, hard. 두 tier는 따로 저장된다 | 모든 kind. hard surface lock은 terminal | `LiveDomainState.occupancy` | 메모리 | 관계 기록 | child-terminal 처리(soft), attach 연결(hard), 로컬 사용자 강제 해제 |
| 자식 상태 | `ChildState`의 5개 값과 근거·확신도 | child-terminal로 등록된 surface | 조회 시 계산 | 저장하지 않음. 훅 보고는 자식 목록 파일에 저장 | 파생 | 훅 보고, 출력·전경 관측 |
| 선택·표시 | 탭에서 선택됨, 화면에 보임 | 모든 kind | View의 `NavigationState` | 메모리 | View 선택 | 로컬 사용자 |

"N/A"인 조합은 [가능한 조합](#가능한-조합과-적용되지-않는-조합)에 따로 적는다.

### 구조 leaf와 kind

트리의 leaf는 `SurfaceDescriptor`(`crates/tasty-model/src/surface_descriptor.rs`)다. ID, kind 문자열, 적용된 활성화 generation만 가진다. 실행 객체와 PTY는 같은 ID로 runtime에 있다([작업 영역](../../features/work-area/index.md#surface)). kind는 생성과 [surface 변환](../../features/convert-surface/index.md)으로만 바뀐다. 점유되거나 복원 대기 중이어도 트리의 kind는 원래 값을 유지한다.

kind 이름이 `empty`인 객체는 빈 자리, plugin 대기 자리, 복원 자리 셋이다. 셋의 차이와 IPC 보고 형태는 [empty surface](../../surfaces/empty/index.md#목적지원-범위)에 있다. 복원 자리는 `type:"Pending"`으로 보고되지만 확정된 surface ID를 가진 실제 surface다. 원격 생성 대기 그림과 다르다([원격 생성 대기 그림](index.md#원격-생성-대기-그림)).

### 활성화 단계

`ActivationPhase`(`crates/tasty-core/src/operation.rs`)의 값은 `Requested`·`Deferred`·`Ready`·`Exited`·`Failed`·`Retired`·`Uncertain`이다. 각 값은 generation과 묶인 `Activation` 사실로 journal에 남는다. 이 값은 "그 generation의 활성화가 어떻게 기록됐는가"를 뜻한다. `Ready`만으로 지금 프로세스가 살아 있다고 판단하지 않는다.

journal은 같은 generation 안에서 아래 전이만 받는다(`crates/tasty-core/src/evolve/lifecycle.rs`의 `phase_transition`). generation이 줄어들거나 이전 generation이 맞지 않는 사실은 거절한다.

| 이전 | 허용되는 이후 |
|---|---|
| `Requested` | `Deferred`, `Ready`, `Failed`, `Retired`, `Uncertain` |
| `Deferred` | `Requested`, `Ready`, `Failed`, `Retired` |
| `Ready` | `Exited`, `Retired`, `Uncertain` |
| `Exited`, `Failed` | `Retired` |
| `Uncertain` | `Ready`, `Failed`, `Retired` |
| `Retired` | 없음 |

현재 제품 코드가 기록하는 값은 `Deferred`와 `Ready`다. 생성 조립(`crates/tasty-core/src/command/assembly.rs`, `crates/tasty-core/src/command/creation.rs`)은 지연 생성이면 `Deferred`, 아니면 `Ready`를 고른다. 나머지 다섯 값은 전이 규칙에 있지만 기록하는 경로를 찾지 못했다. 따라서 "터미널이 끝나면 `Exited`가 된다"처럼 쓰지 않는다.

### 세 종류의 generation

| 이름 | 무엇을 세나 | 소유자 |
|---|---|---|
| 활성화 generation | journal에 기록된 활성화 시도 | `SurfaceDescriptor.activation_generation`, `Activation.generation` |
| PTY 연결 generation | 물리 PTY 연결 하나. adopt는 유지하고 연결 교체는 새 값을 받는다 | `ResourceGeneration`(`crates/tasty-terminal/src/binding.rs`) |
| 내용 generation | 저장한 surface 자료의 판 | journal 사건 `SurfaceDataRecorded`의 `content_generation` |

세 값은 서로 독립이다. 내용을 다시 저장해도 활성화 generation은 그대로이고, PTY를 바꿔도 내용 generation은 그대로다. 전경 이름이 바뀔 때 올리는 `foreground_generation`(`src/core/live.rs`)은 폴링 결과의 변경 번호이며 프로세스 동일성을 판별하지 않는다.

### busy

busy는 로컬 terminal에 대해 시스템 폴링(`refresh_busy_surfaces`, `src/runtime/terminal_activity.rs`)이 판정하고 집합 전체를 교체한다. 판정 조건과 갱신 주기는 [busy 표시 정책](../../design/policies/busy-indicator.md)에 있다. mirror surface의 busy는 서버가 보낸 값을 별도 집합에 둔다. 조회 `is_surface_busy`는 두 집합 중 하나라도 있으면 true다. non-terminal surface는 항상 busy가 아니다.

busy가 false라는 것은 "작업이 끝났다"나 "성공했다"는 뜻이 아니다. 셸 프롬프트로 돌아왔거나 출력이 2초 넘게 없었다는 관측이다.

### attention

`AttentionStore`(`src/core/state/attention.rs`)는 surface마다 레코드 하나를 둔다. 새 발생은 이전 kind와 관계없이 레코드를 교체한다. 우선순위(`NeedsInput` > `Completion`)는 한 surface의 값을 고를 때가 아니라 여러 surface를 함께 표시하거나 테두리를 고를 때 쓴다. 표시·해제 규칙과 우선순위 값의 정본은 [Surface Highlight](../../features/surface-highlight/index.md#내부-동작-headless-valid)다.

- mirror surface에서는 로컬 발생을 만들지 않는다(`raise_attention`). 서버가 보낸 값만 반영하고, 로컬 해제는 서버로 전달한다.
- 로컬 사용자의 확인 해제는 hard 점유 중에는 일어나지 않는다(`clear_attention_local`). soft 점유는 이 제한에 포함되지 않는다.
- 포커스에 의한 해제는 렌더가 확인한 포커스 대상이 아직 유효하고 창이 OS 포커스를 가질 때만 일어난다(`src/app/frame_effect.rs`의 `FocusObserved`).

### 점유

`OccupancyRegistry`(`src/core/attach.rs`)는 soft와 hard를 별도 표에 둔다. 점유가 없는 상태는 tier 값이 아니라 두 표 모두에 항목이 없는 상태다. 점유 모델의 정의는 [주체](../actors.md#점유-occupation-모델), attach 쪽 동작은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#점유-레지스트리-occupancyregistry)에 있다.

| 사건 | 조건 | 결과 | 실패·거절 |
|---|---|---|---|
| soft 획득 `acquire_soft` | 같은 parent의 soft가 있거나 soft가 없음 | soft 항목 기록(같은 parent면 라벨·순번 갱신) | 다른 parent의 soft가 있으면 `AlreadyOccupied`. hard 점유 대상의 거절은 진입점이 한다(아래 문단) |
| soft 해제 `release_soft` | 기록된 parent와 같음 | 항목 제거 | 항목 없음 `NotOccupied`, parent 불일치 `NotHolder` |
| hard 획득 `acquire` | 같은 client이거나 lock이 없거나 기존 holder가 끊김 표시됨 | surface lock 기록 | 살아 있는 다른 holder가 있으면 `AlreadyAttached` |
| hard 해제 `release` | 실제 holder | lock 제거 | `NotAttached`, `NotHolder` |
| 연결 종료 `release_all_for_client` | 그 client의 lock | 그 client의 surface·workspace lock과 멤버 매핑 제거 | — |
| 강제 해제 `release_occupancy` | 로컬 사용자 GUI 조작 | workspace 멤버면 workspace 점유 전체, 아니면 hard, 그것도 없으면 soft 제거 | release에는 IPC가 없다 |
| surface 닫기 `forget_closed_surface` | 닫힌 surface | 그 surface의 lock·멤버 매핑·soft 제거. 같은 workspace의 다른 멤버는 유지 | — |
| 사용자 포커스 `reconcile_soft_occupancy_on_focus` | 대표 점유가 soft이고 parent surface가 트리에 없음 | soft 제거 | External parent는 이 경로로 정리하지 않는다 |

레지스트리 함수는 다른 tier의 표를 보지 않는다. 진입점별 tier 사이 검사, soft와 hard가 함께 있게 되는 순서, 그때의 대표 점유·입력 제한·정리와 해제는 [주체](../actors.md#점유-occupation-모델)가 정본이다. 자식 spawn은 새로 만든 surface에 soft를 건다(`src/runtime/effect_runner/installation.rs`).

### 자식 상태

자식 상태는 child-terminal로 등록된 surface에만 있다. 저장 값이 아니라 조회할 때 `derive_child_state`(`src/core/state/child_liveness.rs`)가 훅 보고와 PTY·출력 관측을 합쳐 계산한다. 판정 우선순위와 응답 필드의 정본은 [child-terminal](../../features/child-terminal/index.md#훅과-출력-관측을-함께-사용하는-상태-판정)이다.

- `ChildState` 값은 `Exited`·`NeedsInput`·`Idle`·`Active`·`Stale`이다. `Exited`는 surface가 라이브 트리에 없다는 뜻이며 OS 프로세스 종료를 확인한 값이 아니다.
- 입력 `terminal.set_state`는 `idle`·`active`·`needs_input` 세 값만 받는다(`src/adapters/ipc/handler/terminal.rs`). `exited`·`stale`은 입력이 아니라 조회가 계산하는 값이다.
- 원격 자식은 연결이 끊긴 동안 마지막 관측을 쓰지 않는다. 조회는 `liveness`·`state`를 `unknown`, 근거를 `remote_disconnected`, 확신도를 `unobserved`로 보고한다(`remote_connection_fields`). 이것은 관측 불가이며 원격 자식의 종료가 아니다.

### kind별 상태

PTY·busy·자식 상태는 terminal의 축이다. 다른 kind에는 필수 공통 상태로 요구하지 않는다. kind마다의 로딩·실패·편집 상태는 kind 문서가 정본이다.

| kind | 이 문서 밖의 상태 | 정본 |
|---|---|---|
| `terminal` | PTY 수명, 스크롤백, 전경 프로세스 | [terminal](../../surfaces/terminal/index.md) |
| `empty` | 빈 자리·plugin 대기 자리·복원 자리, 활성화 실패 횟수(최대 5회) | [empty](../../surfaces/empty/index.md) |
| `explorer` | 목록·선택·편집 | [explorer](../../surfaces/explorer/index.md) |
| `dag_graph` | task 그래프 표시 | [dag_graph](../../surfaces/dag-graph/index.md) |
| `markdown`·`html` | WebView overlay 생성 시도(View마다, 최대 8회) | [WebView 호스트 계약](../../design/systems/webview.md) |
| `image` 등 egui-mesh | plugin 프로세스의 mesh | [Surface 종류](../../surfaces/index.md) |

## 전이

| 이전 조건 | 사건·조작 | 이후 | 실패·거절 | 다른 계층 영향 |
|---|---|---|---|---|
| 없음 | 생성 명령 확정 | leaf 생김, 활성화 `Ready` 또는 `Deferred`, 실행 객체 또는 복원 자리 | 생성 거절(예: 분할 공간 부족)이면 구조 불변 | 탭 하위 레이아웃 변경. 사용자 생성이면 새 surface 선택, 에이전트 생성이면 선택 불변([포커스 정책](../../design/policies/focus.md#에이전트가-만든-탭과-선택)) |
| 복원 자리 | 부팅 활성화, 화면 표시, `tasty wake` | 목표 kind 실행 객체 | 실패하면 `attempts` 증가. 5회 연속이면 자동 재시도 중지 | 없음 |
| leaf 있음 | 닫기 확정 | leaf 제거. `cleanup_surface_observations`(`src/runtime/surface_cleanup.rs`)가 busy·전경 관측·점유·mirror 관측과 attention을 정리 | hard 점유 대상은 UI 닫기에서 거절한다. IPC 경계는 [포커스 정책](../../design/policies/focus.md#hard-점유-대상의-닫기-보호) | 마지막 surface면 탭, 이어서 pane·workspace 닫기로 올라간다 |
| 실행 객체 있음 | 변환 | kind 변경 | 변환 규칙은 [surface 변환](../../features/convert-surface/index.md) | 없음 |
| 점유·attention·busy | 위 각 절 | — | — | 탭·workspace 집계가 다음 표시에서 바뀐다 |

닫기의 응답 시점과 실행 자원 회수 완료는 다르다. 단계와 결과 불명 처리는 [닫기와 실행 자원 회수](../../architecture/close-sequence.md)에 있다.

## 가능한 조합과 적용되지 않는 조합

| 조합 | 판정 | 이유 |
|---|---|---|
| soft 점유 + 자식 상태 `Idle` | 가능 | soft는 표시이며 자식 상태 계산에 들어가지 않는다. child-terminal 자식은 보통 이 상태로 기다린다 |
| hard 점유 + attention `NeedsInput` | 가능 | 테두리는 입력 요청선 하나만 그린다. 로컬 사용자는 확인 해제할 수 없고 holder만 해제한다. 입력은 holder만 할 수 있다 |
| hard 점유 + attention `Completion` | 가능 | 테두리는 점유선을 그리고 완료선은 그리지 않는다 |
| soft + hard 동시 | 가능 | soft 뒤에 hard attach가 오면 생긴다([주체](../actors.md#점유-occupation-모델)) |
| 다른 parent 둘의 soft | 불가 | `acquire_soft`가 거절한다 |
| 살아 있는 holder 둘의 hard | 불가 | `acquire`가 거절한다 |
| 활성화 `Ready` + PTY 없음 | 가능 | `Ready`는 활성화 기록이다. 현재 프로세스 생존을 뜻하지 않는다 |
| busy false + 작업 실패 | 가능 | busy는 출력·전경 관측이며 결과를 모른다 |
| 자식 상태 `Exited` + OS 프로세스 생존 | 가능 | `Exited`는 트리 부재다 |
| non-terminal + busy true | 불가 | busy 폴링은 PTY가 있는 로컬 terminal만 본다. mirror 집합은 서버가 보낸 값이다 |
| non-terminal + PTY·자식 상태 | N/A | terminal 축이다 |
| 원격 자식 + 연결 끊김 | 관측 불가 | 조회가 `unknown`을 보고한다 |
| 복원 자리 + attention | 가능 | attention은 kind와 실행 객체에 묶이지 않는다 |

## 문서 예시

- **soft 점유 자식이 idle을 보고한다**: 부모가 `terminal` 명령으로 자식을 만들면 자식 surface에 soft 점유가 생긴다. 자식이 `terminal.set_state idle`을 보내면 자식 목록의 보고 값이 idle이 되고 조회 상태는 `idle`이다. soft 점유는 그대로 남고 로컬 사용자는 계속 입력할 수 있다.
- **hard 점유 surface에 NeedsInput이 생긴다**: 서버 쪽 surface는 입력 요청 테두리로 표시된다. 서버의 로컬 사용자가 그 surface를 보거나 알림을 읽어도 attention은 남는다. holder가 해제하거나 점유가 풀린 뒤 로컬 확인이 일어나야 사라진다. 입력은 계속 holder만 할 수 있다.
- **원격 자식의 연결이 끊긴다**: mirror에 남은 전경 이름과 무관하게 조회는 `unknown`이다. 연결이 다시 붙으면 다음 조회부터 관측값으로 계산한다.

## 관련

- [상태 모델](index.md)
- [tab 상태 모델](tab.md) — 선택 surface, 탭 집계
- [Surface 종류](../../surfaces/index.md)
- [Surface Highlight](../../features/surface-highlight/index.md)
- [busy 표시 정책](../../design/policies/busy-indicator.md)
- [child-terminal](../../features/child-terminal/index.md)
