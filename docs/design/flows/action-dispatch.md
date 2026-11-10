# Action Dispatch (Intent 큐)

호스트의 모듈 경계를 넘는 동작은 Intent 큐에 등록해 처리한다. 팝업 열기, 프리셋 적용, surface 분할 등이 여기에 해당한다. 이미 일어난 일을 plugin에 알리는 [Event Bus](../../reference/event-catalog.md)와는 역할이 다르다. 구현은 `src/intent.rs`와 `src/intent/<domain>.rs`에 있다.

## 왜 큐인가

명령 팔레트 열기 같은 동작은 단축키·메뉴·IPC·plugin 등 여러 곳에서 요청한다. 그리기 콜백에서 매니저를 직접 변경하려 하면 대여 충돌이 나거나, `mem::take`로 잠시 비워 둔 매니저에 요청해 아무 일도 일어나지 않을 수 있다. 요청을 큐에 넣고 메인 루프에서 처리하면 요청 위치와 처리 시점을 분리할 수 있다.

## 용어

| 용어 | 정의 |
|------|------|
| **Intent** | 다음 메인 루프 단계에서 처리할 호스트 내부 명령. Event Bus 키와 별도 이름 공간을 사용 |
| **Event** | 이미 일어난 일을 [Event Bus](../../reference/event-catalog.md)로 알리는 이벤트. plugin이 구독 |
| **Origin** | 요청 출처인 `User { source }` 또는 `Agent { source }`. 핸들러가 사용자·에이전트 정책을 구분할 때 사용 |
| **Cascade** | Intent 처리 중 추가로 등록하는 Intent. 원래 요청의 origin을 유지 |
| **Bridge** | 처리 결과를 Event Bus 메시지로 바꾸는 공통 경로. `meta.origin`과 `trace_id`를 생성 |

## 핵심 원칙 — Intent 의 위치

Intent는 동작을 요청할 때 사용한다. 처리 결과나 중간 데이터를 전달하는 용도로 반환하지 않는다.

흐름은 `이벤트 → Intent 등록 → 큐 처리 → 상태 변경 또는 추가 Intent 등록` 순서다.

- 이벤트를 해석하는 함수는 `fn parse(e) -> Option<Intent>`처럼 Intent를 반환할 수 있다.
- 처리 핸들러가 추가 작업을 요청할 때는 본문에서 `state.dispatch_intent(...)`를 호출한다. 반환값으로 Intent를 전달해 호출자가 재귀 처리하게 만들지 않는다.
- Intent에는 응답 데이터를 넣지 않는다. 구조 명령의 새 ID와 처리 상태는 App journal admission의 원 reply/continuation으로 전달한다. commit·projection과 필요한 effect 완료를 거친 결과이며, Core의 동기 생성 메서드 반환값으로 응답하지 않는다. 상태 조회에는 Query를 사용한다.

| 유형 | 처리 방식 | 응답 |
|------|----------|------|
| Query | 일관된 publication 관측 시점의 CoreState/EngineRead 조회 | 데이터 |
| Intent | 큐에 등록한 뒤 처리 | 없음 |
| 구조 명령 | App admission → journal 결정·publication → 필요한 effect 완료 | 원 reply/continuation으로 완료 결과 |

### 사용자 입력 대기 = 반드시 2 Intent 분리

확인 팝업의 사용자 응답을 한 Intent 안에서 기다리지 않는다. 첫 Intent는 팝업을 열고 대기 상태를 저장한 뒤 끝낸다. 응답이 오면 두 번째 Intent가 저장된 상태를 읽어 실행하거나 취소한다.

```rust
Intent::PresetApplyRequest{kind,name} → handler: state.dialogs.pending_preset_apply = Some(ctx);
                                                  state.dispatch_intent(OpenPopup{"confirm_preset_apply"});
Intent::PresetApplyConfirmed → handler: let Some(ctx)=…take() else {return}; apply(ctx);
Intent::PresetApplyCancelled → handler: …pending_preset_apply = None;
```

사용자는 언제 응답할지 정해져 있지 않다. 그동안 큐를 막지 않아야 하며, 창 닫기나 설정 변경도 처리해야 한다. 대기 상태를 별도로 저장하면 진행 중인 작업도 확인할 수 있다. 사용자 대기 없이 끝나는 내부 후속 작업은 핸들러에서 추가 Intent를 등록하면 된다.

## Intent 자료형

`Intent`는 `Ui(UiIntent)`, `Domain(DomainIntent)`와 프리셋·탭 등 개별 작업 변종을 가진다. 정의는 `src/intent.rs`, 도메인 핸들러는 `src/intent/<domain>.rs`에 있다. 큐에 넣을 때 요청 출처를 함께 전달한다.

이름 변경 팝업은 workspace 이름·부제목과 탭 이름을 `Intent::DirectRename`으로 넣는다. App의 journal 명령 경로가 commit·projection 뒤 `user_direct`를 담은 완료 통지를 만든다. Domain 큐의 후속 처리는 이 표시를 싣지 않는다.

```rust
pub struct DispatchedIntent { pub body: Intent, pub origin: IntentOrigin, pub trace_id: Option<String> }
pub enum IntentOrigin { User { source: UserSource }, Agent { source: AgentSource }, System }
//   UserSource: Shortcut(&str) / Menu(&str) / ContextMenu
//   AgentSource: Ipc / Plugin(String) / Cli / Remote
```

`System`은 OSC 등 자동 후속 처리다. `Remote`는 원격 client가 forward한 요청의 출처다. 로컬 View 포커스를 바꾸지 않으며 user/agent wire 표지는 원격 닫기 기록 정책에 사용한다. 구조 명령 admission(`src/app/journal/commands`)은 원 origin과 대상 ID를 유지한다. 구조 요청의 호출자는 `IntentOrigin`이 아니라 admission이 남기는 `actor`·`origin`이 기록한다. `actor`는 호출자 범위(`local`·`plugin:<id>`·`agent:<id>`)이고 `origin`은 유입 경로(`ipc`·`plugin`·`remote-structure`)다. 비구조 IPC 핸들러는 호출자가 plugin이어도 `Ipc`를 싣는다.

팝업 A의 처리에서 B를 열면 B에도 A의 origin을 전달한다. 별도의 Cascade 출처를 만들지 않는다. `DispatchedIntent.trace_id`는 현재 생성자에서 `None`이며 Event Bus의 `trace_id`와는 별개다.

<a id="발화--처리"></a>

## 요청 등록과 처리

`state.dispatch_intent(dispatched)`는 요청을 큐에 넣는다. 요청 출처를 붙이는 빌더로 `.from_user_shortcut("…")`, `.from_user_menu("…")`, `.cascaded_from(intent)`를 사용한다.

IPC 엔진 핸들러는 창 대신 요청별 `IntentOutbox`를 받는다. 핸들러는 `out.push(dispatched)`로 요청을 넣고, 진입점의 게이트·`dispatch_routed`·`record_plugin_rss_samples`가 요청 종료 시 창 큐로 옮긴다. 한 요청 안에서는 추가한 순서를 유지하므로 게이트가 만든 Intent가 먼저 처리된다. [App·Engine·View 상태 소유권](../../dev-guide/app-state-ownership.md)를 따른다(대체: [App·Engine·View 상태 소유권](../../dev-guide/app-state-ownership.md), 이행 중).

GUI의 `App::dispatch_pending_intents`는 창과 parked state의 큐를 처리한다. 전체 등록 순서를 그대로 따르지는 않으며 `classify_intent`가 정한 종류별로 처리한다.

| 종류 | 처리 순서 |
|---|---|
| `Immediate` | 같은 state에서 FIFO로 즉시 처리 |
| `Intent::Domain` | 단계 C인 `run_domain_cascade`에서 FIFO로 처리 |
| `PatchSettings` | 원 kind/plugin과 상대 변경을 고정하고 App settings 적용에서 순서대로 처리 |

Domain 처리는 App 전체를 대여하므로 state별 처리 뒤로 분리한다. 큐를 `mem::take`로 꺼낸 뒤 순회하며, 처리 중 새로 등록한 Intent는 재진입을 피하기 위해 다음 프레임에 처리한다. journal admission·완료 오류는 원 요청의 engine 및 View binding을 확인해 표시한다. 그래서 사용자 메뉴에서 고른 mirror 차단 이동도 차단 toast를 띄운다.

그리는 중인 창은 `App::redraw_main_window`(`src/app/view_frame.rs`)의 세 지점에서 그 창의 큐를 한 번 더 비운다. 실행 요청은 두 타입으로 나뉜다. 프레임 단계 효과는 `Intent::Frame(FrameEffect)`(`src/app/frame_effect.rs`)이고, 그 밖의 실행 요청은 일반 명령 `Intent::Engine(EngineAction)`(`src/app/engine_action.rs`)이다. 효과의 단계는 `FrameEffect`의 변종이 정한다.

| drain 지점 | 단계 | 이 단계의 효과 | 다음 단계보다 먼저 적용하는 이유 |
|---|---|---|---|
| prepare_redraw 뒤 | `FramePhase::Geometry` | `GeometryEffect`: `DefaultGrid`, `Resize` | 입력 context와 IME가 새 grid를 본다(grid → tap → 입력) |
| prepare_render_inputs 뒤 | `FramePhase::Inputs` | `InputEffect`: `FocusObserved`, `LocalMesh`, `RemoteMesh` | mesh 중계와 draw 전에 포커스 관측과 mesh context를 전달한다 |
| render_if_dirty 뒤, finish_redraw와 native webview 동기화 전 | `FramePhase::Draw` | `DrawEffect`: `Html`, `PluginDisplay`, `RemoteMeshFull` | 이번 프레임의 HTML 배너 결정이 native reload·동기화 전에 gate에 반영된다 |

세 지점 모두 private 함수 `apply_frame_effects`에 도달한 단계를 넘긴다. 이 함수는 단계가 도달한 단계 이하인 `Intent::Frame`만 그 자리에서 원 요청의 `IntentOrigin`과 함께 적용한다. 일반 명령과 아직 오지 않은 단계의 효과는 출처 봉투를 그대로 둔 채 원래 순서로 큐에 되돌린다. 뒤 단계 효과는 같은 프레임의 해당 drain이, 일반 명령은 다음 일반 처리(`about_to_wait` 또는 다음 프레임 시작)가 처리한다. 그래서 draw 중 egui가 낸 탐색기 변경·원격 목록 조회·즐겨찾기·이미지 업로드 같은 일반 명령은 finish_redraw 전에 실행되지 않는다. 이 지점은 `classify_intent`와 debug intent 관측을 거치지 않는다. 단계 순서는 호출 위치와 넘기는 단계가 함께 정하며 `src/source_guards/frame_phase_order.rs`가 소스 순서를 검사한다.

일반 처리의 `dispatch_one_intent`는 일반 명령과 함께 프레임 밖에서 만든 효과도 적용한다. 창 resize 이벤트·단축키·분할선 드래그가 내는 grid·resize 효과가 여기에 해당한다. 프레임 사이에는 모든 단계가 이미 지났기 때문이다. 화면 프레임 밖의 IPC 핸들러(`file_picker.trigger`)가 내는 원격 목록 조회도 일반 명령이므로 일반 처리가 실행한다. 대상 binding이 오래된 효과·명령은 조용히 버리며 사용자 오류 토스트를 만들지 않는다. 배너가 만든 HTML 스크립트 허용 요청은 사용자 출처일 때만 적용하고, 사용자 출처가 아닌 요청은 거절하고 로그만 남긴다. 에이전트의 허용은 이 요청을 거치지 않고 release IPC `surface.html_script_allow`가 처리한다.

헤드리스는 `crate::intent::headless::drain_pending_intents`로 engine 하나의 큐를 다음 시점에 처리한다.

- IPC 요청 처리 직후, 응답을 보내기 전
- plugin 호출 결과를 회신하기 전
- 메인 루프가 대기하기 직전

attention·notification 적재, terminal mark, 설정 적용·저장처럼 engine 상태만으로 끝나는 작업을 처리한다. 화면 다시 그리기·토스트·테마 재설치·알림음은 제외한다. OSC 7 cwd의 탭 이름 갱신은 Intent가 아니라 PTY 처리에서 `intent::headless::apply_terminal_cwd_changed`를 직접 호출한다.

헤드리스의 `drain_pending_host_events`도 위 세 시점과 PTY 출력 처리 끝에서 실행한다. `pending_host_events`를 비우고 `HookFired`로 push 완료를 기다리는 agent task를 마감한다. 일반 plugin Event Bus 전달 경로는 없어 나머지 이벤트는 버린다. 지원 범위와 이유는 [헤드리스 IPC 지원 범위](../../dev-guide/headless-ipc-surface.md)을 따른다.

핸들러는 `match &intent.body`로 도메인 함수를 선택한다. MainViewState 전체를 trait object가 대여하면 필요한 필드만 따로 변경하기 어렵기 때문이다. 오류는 `tracing::warn!`으로 기록하고, 사용자에게 알려야 할 실패는 사용자·에이전트 정책에 따라 토스트로 표시한다. 패닉을 일으키거나 `let _ =`로 오류를 버리지 않는다.

## Intent → Event Bus Bridge

엔진의 확정 결과는 `EngineRuntime.pending_host_events`에, 엔진 밖 process 이벤트는 `AppState.pending_host_events`에 쌓는다. View는 포커스 변화 같은 표시 관측값을 제공하며 두 실행 큐의 원본을 소유하지 않는다. `src/app/dispatch/host_events.rs`의 공통 경로가 `PluginManager::emit_host_event`를 호출해 [Event Bus](../../reference/event-catalog.md) 메시지로 바꾼다.

| 필드 | 규칙 |
|------|------|
| `meta.origin` | 항상 `{kind:host}`(`EventOrigin::Host`). Intent의 origin을 복사하지 않음 |
| `meta.trace_id` | 이벤트마다 새로 발급하는 `h<seq 16진>`. Intent 값을 이어받지 않음 |
| `meta.scope` | 핸들러가 변경 결과를 기준으로 지정 |
| `meta.hop` | 호스트가 만든 이벤트이므로 `0` |

## User vs Agent 정책

핸들러는 `origin.is_user()`로 요청 출처를 구분한다. dispatcher가 모든 정책 위반을 자동 거절하지는 않으므로 핸들러를 검토할 때 확인한다.

| 동작 | User | Agent |
|------|------|-------|
| UI Intent로 팝업 열기 | 허용 | 금지. debug 사용자 입력 재현 IPC는 예외. 사용자 입력을 증명하지 못한 plugin 중계 파일 열기(`FileDispatchOrigin::PluginUnverified`)의 무매칭 fallback picker·원격 picker·원격 파일 다운로드 후 실행 대화상자도 예외다(외부 IPC 요청에는 열지 않는다). 확정해도 선택은 유지한다 |
| 닫은 항목 복원 기록 추가·OS 창 포커스·workspace 활성화 | 허용 | 금지 |

에이전트의 새 창 생성은 별도 `window.create` 경로로 허용하며 기존 사용자 포커스를 유지한다. [포커스 정책](../policies/focus.md)을 따른다.

Core는 `UiIntent`를 모르므로 release의 Domain 처리에서 UI Intent를 만들 수 없다. 팝업·토스트·대화상자의 표시 조건은 [팝업](../systems/popup.md)과 [토스트](../systems/toast.md)를 따른다. 원격 연결 상태 토스트처럼 허용된 예외를 일반 에이전트 요청의 성공·실패 알림으로 확대하지 않는다.

## intent-discipline 강제

`scripts/check-intent-discipline.sh`는 popup·preset·surface·tab·pane·workspace의 직접 변경 API 호출을 검사한다. `script-gates.yml`의 main push와 PR에서 실행하도록 설정돼 있다.

검사 대상은 `open`·`open_centered`·`open_centered_focused`·`open_with_scope`·`open_at_top_of_scope`·`open_at_focused`·`close`·`toggle*`다. 조회 함수인 `is_open`·`get_mut`·`open_geometry`는 포함하지 않는다.

다음 내용은 제외한다.

- 실제 호출이 아닌 주석·문자열 리터럴
- 도메인 자체를 시험하는 `#[cfg(test)] mod` 본문과 `*_tests.rs`
- 이름만 같은 다른 타입의 메서드. 패턴별 예외 경로로 관리한다.

rustfmt가 `state` / `.popups` / `.open(`처럼 여러 줄로 나눈 호출도 같은 호출로 본다.

허용할 호출은 같은 줄이나 바로 위·아래에 `// intent-exempt: <사유>`를 적는다. 여러 줄로 나뉜 호출은 수신자 체인의 첫 줄부터 메서드 이름이 있는 줄까지를 같은 줄로 보고, 그 범위의 바로 위·아래 줄까지 인정한다. 체인 첫 줄 위와 메서드 줄 위가 모두 인정된다. 체인의 첫 줄은 `.`으로 시작하는 줄을 위로 따라가 찾으므로, 한 줄 호출이라도 앞 줄에서 이어진 `.` 체인이면 그 앞 줄 위의 표지도 인정된다. 현재 예외에는 팝업의 `on_close` 정리, 핸들러의 후속 요청, 응답이 필요한 동기 Core 호출, 포커스 보존을 위해 큐를 우회한 호출이 있다.

외부 `tests.rs` 모듈에서 실제 command/backend를 직접 시험하거나 고유 임시 저장소를 준비하는
호출도 호출별 `intent-exempt` 사유를 적는다. command의 순서·receipt·cache·파일 결과가 검증
대상이면 Intent로 우회하지 않는다. 같은 시험 파일의 다른 호출까지 일괄 면제하지 않는다.

`intent::watch`(`src/intent/watch.rs`)는 GUI debug 빌드의 큐 처리에서 `tracing::debug!` 로그를 남긴다. release와 헤드리스 큐 처리에는 이 로그가 없다.

## dedup

같은 처리 주기에 같은 popup ID의 OpenPopup이 중복되면 핸들러가 `state.popups.is_open(id)`로 두 번째 요청을 무시한다. 큐에 넣는 시점에는 origin과 trace_id가 다른 요청을 같은 것으로 판단하지 않는다.

## 관련

- [popup](../systems/popup.md) · [toast](../systems/toast.md) — 표시 조건
- [focus](../policies/focus.md) — 사용자 포커스 보호
- [reference/event-catalog](../../reference/event-catalog.md) — Event Bus 메시지
- [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md) — 팝업 구현

### Source guard의 실행 예외

Intent discipline 검사는 옛 direct mutation 이름과 popup/store 호출을 마스킹 source에서 찾는다.
App preset editor의 `apply`에서 실행하는 여섯 preset save 메서드는 함수·메서드 조합으로만 허용한다.
같은 App 파일의 다른 함수나 View의 store save는 면제되지 않는다. 구조 publication/evolve/mirror
entry는 별도의 core-writer guard가 명시 호출 위치로 검사하며, 두 검사는 수신자 타입이나 전체 동작을 증명하지 않는다.

## 구조 명령의 확정과 실행

App은 명시 대상과 원 요청 identity를 고정하고 데이터 홈 journal worker에 제출한다. CommandExecutor는 멱등 키 조회 뒤 순수 decide를 호출하며 EventStore가 사건·명령 결과·effect 의무를 함께 commit한다. App의 publication은 모든 대상 CoreState projection을 같은 cut으로 공개하고 ACK한다. EffectRunner와 SurfaceRestorer는 확정된 의무에 따라 실행 owner를 준비·회수하며 원 요청은 필요한 완료까지 보존된다. replay는 이 외부 실행을 반복하지 않는다.

비저널 입력·관측은 기존 intent/host event 경계로 처리한다. terminal tell·broadcast·respawn 입력은 App의 typed 실행 함수가 요청 처리 안에서 바로 쓴다. broadcast·respawn의 write는 `apply_live`의 `SendToSurface`를 큐 없이 적용하고, tell은 본문을 쓴 뒤 CR 제출을 Engine의 ack 대기에 예약한다. wire 해석과 응답·오류 변환은 IPC 봉투에 남는다. 이 큐의 drain이나 메시지 전달 성공은 구조 commit·자원 회수의 완료 조건을 대신하지 않는다. 상세 공개·Recovery 계약은 [자원 회수](../../architecture/resource-recovery.md), 저장 원본은 [저장소](../systems/storage.md)를 따른다.
