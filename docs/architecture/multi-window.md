# 멀티 윈도우 아키텍처

tasty 는 **단일 프로세스 · 메인 스레드 단일 winit 이벤트 루프**로 여러 OS 윈도우(모달 포함)를 돌린다. `App`(winit `ApplicationHandler`)이 도메인·외부통신·GUI 세 부분을 합성한다.

## 구조

```
App  (1 프로세스, 메인 스레드, winit ApplicationHandler)
├── core: Core         도메인 본체 (워크스페이스/세션/attach/registries) — 항상 빌드
├── hub: Hub           외부 통신 (IPC 서버, 포트 파일) — 항상 빌드
└── view: ViewRegistry GUI 어댑터 — #[cfg(feature = "gui")]
    ├── proxy              winit EventLoopProxy<AppEvent>
    ├── views: HashMap<WindowId, Box<dyn View>>
    ├── active_modal_id: Option<WindowId>   (모달 전역 최대 1)
    └── focused_view_id: Option<WindowId>
```

`focused_view_id` 는 대상 없는 IPC 요청이 떨어지는 main 창이다. 창을 등록할 때는 그 창을 사용자가 만들었을 때만 옮긴다 — 에이전트가 만든 창은 옮기지 않는다([포커스 정책](../design/policies/focus.md#에이전트가-만든-창과-포커스), [ADR-0497](../adr/0497-an-agent-created-window-does-not-take-the-users-focus.md)).

모든 윈도우(모달 포함)는 단일 `views` 맵에 저장된다. 모달은 별개 엔티티가 아니라 `active_modal_id: Option<WindowId>` 로 식별되는 View 상태이며, 활성 모달은 이 필드로 식별한다. (옛 `Engine` struct 는 삭제됐고 그 역할이 Core/Hub/ViewRegistry 로 분산됐다.)

## Window 트레잇 계층 (`src/view/`)

```text
View (sealed trait, : sealed::Sealed + std::any::Any)
├── ModalView         (supertrait)
│   ├── SettingsView   (설정 모달)
│   ├── QuitView       (종료 확인 모달)
│   └── PluginsView    (plugin 매니저 모달)
└── (그 외 — View + sealed::Sealed 직접 구현)
    ├── MainView       (터미널 호스트 — 워크스페이스/페인/탭/서피스)
    └── PresetView     (프리셋 편집기, modeless)
```

- **`View`**: 공통 인터페이스. `sealed::Sealed` supertrait 으로 크레이트 외부 구현 차단. `Any` 로 downcast.
- **`ModalView`**: 모달 계열의 default 동작 marker(`shown`/`set_shown`/`reveal_after_first_render`/`on_escape`). 그 외 구현체(`MainView`/`PresetView`)는 `View` + `sealed::Sealed` 를 직접 구현한다.
- **`ViewBase`**: 모든 구현체가 `pub base: ViewBase` 로 합성하는 공통 필드(gpu·winit·dirty·modifiers·focused·close_requested).

> 용어: 여기서 "윈도우"는 winit OS-level 윈도우다. tasty 도메인 계층의 Window/Workspace/Pane/Tab/Surface 와 다르다 — [구조 계층](../concepts/hierarchy.md).

## 모달 (Modal modality)

엔진 전역 최대 1개. 설정창·종료 다이얼로그가 대표.

- Modal View 가 열리면 `active_modal_id` 설정, 닫히면 `None`(`src/app/modal.rs`).
- 이벤트 디스패처는 다른 modeless View 에 `modal_active: true` 를 전달하고, 각 View 의 입력 핸들러가 이때 입력을 차단한다(Resized/RedrawRequested/ModifiersChanged/Focused 만 허용).
- 모달도 같은 `views` 맵에 있어 단일 이벤트 디스패처가 전부 처리한다.

## parked states — PTY 생존

모든 윈도우가 닫혀도 PTY 세션을 잃지 않도록, `App.parked_states` 에 `(AppState, CoreState)` 를 보관한다. 새 윈도우 생성 시 옮겨 담거나 IPC 가 직접 쓴다. 윈도우가 0개여도 프로세스(트레이)는 살아 있을 수 있다 — [system-tray 정책](../design/policies/system-tray.md).

파킹은 engine 을 살려 두는 것이므로 아래 레이아웃 슬롯 점유도 함께 유지된다 — 창은 없지만 슬롯은 여전히 그 engine 것이다.

**"engine 이 살아 있는가"를 묻는 판정은 `views` 와 `parked_states` 를 함께 봐야 한다.** 창 유무로 대신 판정하면 파킹이 곧 소멸로 오인된다. 원격 attach 세션의 고아 판정이 그 사례다 — mirror 워크스페이스를 들고 있는 engine 이 parked 라는 이유로 세션을 끊으면 사용자가 창을 최소화했을 뿐인데 원격 점유가 풀린다([remote-attach — 창 없는 상태(parked)에서의 세션 수명](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명)). 그 세션에 도착하는 mirror 이벤트의 적용 대상 탐색도 같은 범위를 돈다 — parked engine 의 mirror 터미널에 즉시 적용되고, 창 복원 시 그대로 그려진다([ADR-0110](../adr/0110-mirror-events-apply-to-parked-engines.md)).

### 창이 스스로 닫히는 자리 — `close_requested`

마지막 워크스페이스가 닫힌 MainView 는 `request_close()` 로 `ViewBase.close_requested` 만 세우고, App 이 그 플래그를 보고 창을 치운다(마지막 main 창이면 파킹, 아니면 은퇴). **플래그는 세운 경로 안에서 소비한다** — 세운 채 이벤트 루프로 돌아가면 워크스페이스가 빈 창이 다음 창 이벤트를 받고, 그것이 `RedrawRequested` 면 렌더 경로가 active workspace 를 묻다 죽는다.

- **창 이벤트 경로**(winit 키 · 마우스 · redraw 안의 메뉴 continuation): `App::dispatch_window_event_to_view` 가 handler 직후 그 창을 치운다.
- **창 이벤트 밖의 App 경로**(`about_to_wait` 의 webview 포워딩 키 단축키 · 네이티브 메뉴 폴링): 그 App 함수가 MainView 를 부른 뒤 같은 함수 안에서 `App::close_self_requesting_windows` 를 부른다.

렌더 경로 쪽을 "빈 워크스페이스면 건너뛴다" 로 누그러뜨리지 않는다 — 그 창은 이미 닫혀야 할 창이고, 건너뛰어도 다음 IPC·입력 핸들러가 같은 전제로 active workspace 를 묻는다. 두 번째 목록의 짝과 `request_close()` 생산자 명부는 `crates/tasty-doc-guards/tests/close_request_consumed_in_place.rs` 가 고정한다.

## 레이아웃 슬롯

창 ↔ engine ↔ **레이아웃 슬롯**은 1:1 이다. 각 `CoreState` 는 자기 슬롯 번호(`layout_slot`)를 들고, 자기 슬롯 파일에만 저장한다. 창마다 워크스페이스 목록이 독립이라는 구조적 사실이 저장소까지 이어진 형태다 — 두 창이 같은 목록을 복제하거나 서로의 저장을 덮어쓰지 않는다.

**점유는 살아있는 engine 에서 파생된다.** 별도 레지스트리도, 디스크 기록도 없다. 점유 집합은 그때그때 `views` 의 MainView 들과 `parked_states` 를 훑어 만든다 — 여기에 `App.core_state` 도 포함된다. 갓 만들어진 engine 은 `views` 에 등록되기 전까지 거기 임시로 머물기 때문에, 그 구간을 빠뜨리면 같은 슬롯이 두 번 배정된다. 따라서

- engine 이 drop 되면(창 닫힘) 그 슬롯은 그 순간 free 가 된다 — 해제 호출이 없으니 해제 누락도 없다.
- **parked engine 은 슬롯을 계속 쥔다.** 창이 없어도 engine 이 살아 있으므로 점유에 포함되고, 다시 창을 열 때 그 engine 이 같은 슬롯을 이어쓴다. 재배정했다면 남의 슬롯 파일을 덮어썼을 것이다.
- 프로세스가 죽으면 점유는 전부 사라진다 — 크래시가 슬롯을 영구 점유로 남기지 않는다.

결정의 근거·대안·재검토 조건은 [ADR-0087](../adr/0087-layout-slot-occupancy-model.md), 배정 규칙과 창 닫힘 정책의 현재 동작은 [layout-persistence](../features/layout-persistence/index.md).

## 윈도우 간 GPU·통신

- 각 View 는 자체 wgpu surface + egui 컨텍스트 보유(`ViewBase.gpu`). wgpu adapter/device 는 공유.
- 윈도우 간 직접 통신 없음 — 모두 도메인(Core)·Intent 큐를 경유.

## 단일 프로세스 · 단일 스레드 근거

| 관점 | 근거 |
|------|------|
| 상태 공유 | `Arc`/`Mutex` 없이 Core 상태 직접 공유, 윈도우 간 IPC 불필요 |
| GPU 리소스 | wgpu adapter/device 윈도우 간 공유 가능 |
| winit 호환 | winit 은 프로세스당 이벤트 루프 1개를 전제 |
| 크래시 격리 | 셸은 이미 별도 OS 프로세스(PTY) — 셸 크래시가 tasty 로 전파 안 됨 |
| 리소스 방어 | 스크롤백 상한 + PTY 읽기 채널 버퍼 제한 |

Chrome 의 멀티 프로세스 사유(신뢰 불가 웹 코드 보안 격리)는 tasty 에 해당하지 않는다. (plugin 은 별도 프로세스지만 sandbox 경계는 IPC 권한 게이트로 — [plugins](../concepts/plugins.md).)

## 관련

- [아키텍처 개요](index.md) — headless `gui` feature 분리
- [input-layer](input-layer.md) — 윈도우 내부 마우스 입력 계층
- [concepts/hierarchy](../concepts/hierarchy.md) — 도메인 Window/Workspace/Pane/Tab/Surface
- [ADR-0087](../adr/0087-layout-slot-occupancy-model.md) — 레이아웃 슬롯 점유 모델의 근거·대안
- [features/layout-persistence](../features/layout-persistence/index.md) — 슬롯 배정·저장·복원의 현재 동작
