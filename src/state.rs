mod accessors;
mod cascade_window;
mod ipc_window;
// 포커스·탭 변화의 polling 감지 — GUI tick 만 부른다. headless 의 Event Bus 발화는 cascade 가
// 직접 세운다.
#[cfg(feature = "gui")]
mod detect;
// dialog·popup 입력 상태는 GUI 가 소유한다 — headless 에는 세우는 쪽도 비우는 쪽도 없다
// (`docs/dev-guide/app-state-ownership.md`).
#[cfg(feature = "gui")]
mod dialogs;
// 키보드 라우팅용 포커스 surface 분류와 방향 포커스 이동 — 사용자 입력 경로만 쓴다.
#[cfg(feature = "gui")]
mod events;
#[cfg(feature = "gui")]
mod focus;
// gui 전용 상태(popup/모달/스테이지)를 단정하는 테스트라 headless 빌드에는
// 대상 자체가 없다. `#[cfg(test)]` 만 걸면 `--no-default-features` 테스트 빌드가
// 통째로 깨진다 — `docs/dev-guide/unit-test-isolation.md` "feature 별 테스트 게이팅".
#[cfg(all(test, feature = "gui"))]
mod fullscreen_stage_tests;
// 화면 좌표 → surface/pane 영역 계산 — 그리기와 마우스 히트 판정만 쓴다.
#[cfg(any(feature = "gui", test))]
mod layout;
pub mod mouse;
pub(crate) mod pane;
// gui 전용 상태(popup/모달/스테이지)를 단정하는 테스트라 headless 빌드에는
// 대상 자체가 없다. `#[cfg(test)]` 만 걸면 `--no-default-features` 테스트 빌드가
// 통째로 깨진다 — `docs/dev-guide/unit-test-isolation.md` "feature 별 테스트 게이팅".
#[cfg(all(test, feature = "gui"))]
mod popup_close_tests;
// gui 전용 상태(popup/모달/스테이지)를 단정하는 테스트라 headless 빌드에는
// 대상 자체가 없다. `#[cfg(test)]` 만 걸면 `--no-default-features` 테스트 빌드가
// 통째로 깨진다 — `docs/dev-guide/unit-test-isolation.md` "feature 별 테스트 게이팅".
#[cfg(all(test, feature = "gui"))]
mod popup_ownership_tests;
mod tab;
#[cfg(test)]
pub(crate) mod tests;
mod workspace;

// 팔레트 — 여는 것도 실행하는 것도 GUI 다. 매칭 로직은 시험이 headless 에서도 부른다.
#[cfg(any(feature = "gui", test))]
pub mod command_palette;
pub mod preset_apply;
// 터미널 검색 바의 상태 — 검색 바 popup 만 세우고 읽는다.
#[cfg(feature = "gui")]
pub mod search;
/// 텍스트 선택 — 실체는 `tasty-selection` 크레이트에 있다(렌더러가 앱 상태 모듈을
/// 거꾸로 보지 않도록 타입 소속만 내렸다). 기존 `state::selection::…` 호출부는 그대로다.
#[cfg(feature = "gui")]
pub use tasty_selection as selection;

pub use crate::core::host_event::{PendingHostEvent, PendingSurfaceClosed};
#[cfg(feature = "gui")]
pub use dialogs::{
    DialogState, FileHandlerPickerData, FileHandlerPickerResult, PendingNativeMenu,
    PendingScriptConfirm, PickerHandlerSummary, RenameTarget, TabDragState, TerminalLinkMenu,
    WsDragState,
};
#[cfg(feature = "gui")]
pub(crate) use dialogs::{FilePickerData, FilePickerRequester, FilePickerResult, FpLoadState};
#[cfg(feature = "gui")]
pub use events::FocusedSurfaceType;
pub use workspace::WorkspaceCloseOrigin;

use crate::core::CoreState;
#[cfg(feature = "gui")]
use crate::model::LogicalPx;
#[cfg(any(feature = "gui", test))]
use crate::model::PhysicalPx;

// IdGenerator is now in core_state.rs

/// 열려 있는 모달의 종류. `AppState::active_modal_kind` 의 값이며,
/// `App::open_modal` 이 여는 쪽에서 받아 세운다 — 열린 `View` 를 downcast 해서
/// 되짚지 않는다. 여는 쪽은 자기가 무엇을 여는지 이미 알고, downcast 로 되짚으면
/// 새 모달을 추가한 사람이 이 열거를 안 늘려도 조용히 `None` 이 된다.
// 이유: 여는 자리(`App::open_modal`)가 GUI 뿐이라 headless 에서 variant 가 만들어지지 않는다.
// 열거는 headless 에도 남는다. `active_modal_kind` 는 debug 빌드의 두 조합에 있고 `ui.state` 덤프가
// 거기서 같은 키로 그 값(`None`)을 찍는다 — release 헤드리스에는 그 필드가 없다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui opens a modal, so headless never builds a variant"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalKind {
    Settings,
    Plugins,
    Quit,
}

impl ModalKind {
    /// IPC 직렬화용 납작한 이름. JSON 에 열거가 없어서만 존재한다 — 내부 판정은
    /// 이 문자열이 아니라 열거로 한다.
    ///
    /// **`debug_assertions` 에 걸린다.** 이 이름을 읽는 자리는
    /// `src/adapters/ipc/handler/debug_state.rs` 의 `ui.state` 덤프 하나뿐이고 그 모듈이
    /// `#[cfg(debug_assertions)]` 이다 — release 에는 소비자가 없어 `dead_code` 가 문다
    /// (그 lint 는 이 크레이트에서 error 라 컴파일이 죽는다). 열거 자체와
    /// `active_modal_kind` 필드는 release 에도 산다: 여는 쪽이 세우고 닫는 쪽이 지운다.
    #[cfg(debug_assertions)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Plugins => "plugins",
            Self::Quit => "quit",
        }
    }
}

pub struct AppState {
    // ── Window-level UI state ──
    pub(crate) active_workspace: usize,
    /// 카테고리별 마지막 active 워크스페이스의 **id**. 카테고리 quick-switch(T4WS ②⑤)가
    /// 대상 카테고리로 점프할 때 그 카테고리의 마지막 포커스 워크스페이스로 착지하기 위한
    /// 세션-런타임 상태(영속 안 함 — "never visited" 는 first 로 폴백).
    ///
    /// **전역 인덱스가 아니라 id 다.** 인덱스를 들면 워크스페이스 제거·재정렬마다
    /// 이 맵을 함께 밀어줘야 하고, 밀어주는 것을 잊은 경로가 생기면 "같은 카테고리의
    /// 다른 워크스페이스로 착지" 하는 조용한 오작동이 된다(실제로 재정렬 경로 두 곳이
    /// 그랬다). id 는 순서 변경과 무관하므로 그 유지보수 자체가 없어진다 —
    /// 착지 시점에 id → 인덱스로 한 번 찾고, 못 찾으면(제거됐으면) first 로 폴백한다.
    // 읽는 자리가 [`Self::switch_workspace`] 와 카테고리 전환뿐이라 그 게이트를 따른다.
    #[cfg(any(feature = "gui", debug_assertions, test))]
    pub(crate) category_last_active: std::collections::HashMap<
        tasty_utils::id::WorkspaceCategoryId,
        tasty_utils::id::WorkspaceId,
    >,
    /// 설정 모달 **열기 요청** 플래그 — "열려 있는가" 가 아니다.
    ///
    /// 세우는 자리는 사이드바 버튼 경로 하나뿐이고(`adapters/ui/draw.rs` 의
    /// `settings_clicked`), 다음 프레임에 `view/main/redraw.rs` 의
    /// `dispatch_pending_modal_opens` 가 **소비하며 즉시 false 로 되돌린다**
    /// (`view/main/keyboard.rs` 의 escape 처리도 지운다). 그래서 이 값이 한
    /// 프레임 넘게 true 인 적이 없다.
    ///
    /// 키보드 단축키 경로는 이 필드를 **아예 안 거친다** —
    /// `adapters/ui/input/shortcuts/keybinding.rs` 가 `AppEvent::OpenSettings` 를
    /// 보내고 `app/modal/settings.rs` 가 별도 winit 창을 만든다.
    /// ⇒ **창이 떠 있는지를 이 필드로는 관측할 수 없다.** 그걸 물어야 하면
    ///    `view::View::is_modal_active()`(= `active_modal_id`)를 봐라. 그 값은
    ///    모달 등록에서 세워져 닫힐 때까지 남는다.
    ///
    /// **소비자가 넷이고, 한 이름이 세 가지 물음에 답하고 있다.** 전수(식별자로
    /// `src/`·`crates/` 를 훑었다):
    ///
    /// | 자리 | 술어 | 무엇을 정하나 | 이 플래그로 옳은가 |
    /// |---|---|---|---|
    /// | `state.rs` `has_egui_overlay_open` | "egui 오버레이가 **이 창의** wgpu 표면을 덮는가" | WebView `set_visible` · `release_keyboard_focus` | **아니다** |
    /// | `state.rs` `keyboard_overlay_open` | "키가 터미널 대신 egui 로 가야 하는가" | 키 라우팅 | 판정 필요 |
    /// | `view/main/mouse.rs` `mouse_overlay_open` | 마우스 최상위 차단 | 세 핸들러 공통 게이트 | 판정 필요 |
    /// | `view/main/keyboard.rs` escape 분기 | "설정 창이 **떠 있는가**" | 대기 중인 열기 요청 취소 | 판정 필요 |
    ///
    /// 첫째가 "아니다" 인 이유: `handle_redraw` 안의 순서가 지움(`dispatch_pending_modal_opens`)
    /// → egui 패스(`render_if_dirty`, 여기서 플래그가 선다) → 읽음(`sync_webviews`) 이라,
    /// 이 항은 **버튼을 누른 그 한 프레임에만** true 다. 그 프레임에 WebView 를 전부
    /// 숨겼다가 다음 프레임에 되살린다 — 설정 창은 별도 winit 창이라 이 창의 wgpu 표면을
    /// 덮지 않으므로 **가릴 이유도 없고**, 한 프레임짜리라 **가리는 구실도 못 한다.**
    ///
    /// 넷째가 한때 "아니다" 였던 이유: 그 자리를 "설정 창이 떠 있다" 로 읽으면 틀린다 —
    /// `app/modal/settings.rs` 는 창을 만든 뒤 이 플래그를 **다시 세우지 않는다**(그
    /// 파일에 이 식별자가 0 건이다). 지속하는 참값은 `view::View::is_modal_active()` 다.
    /// 지금 그 분기는 창이 떠 있는지를 묻지 않고 **대기 중인 열기 요청을 취소**하며,
    /// 그 물음에는 이 래치가 맞는 값이다. 같은 표에 있던 double-tap 분기는 물음이
    /// 캡처였고 그 경로가 `SettingsView` 안에 따로 살아 있어 **지웠다**.
    ///
    /// ⇒ 배선을 고치는 쪽은 **소비자마다 따로 판정해야 한다.** 하나의 지속 값으로
    ///    넷을 한꺼번에 바꾸면 첫째가 조용히 반대로 는다(설정 창이 열려 있는 내내
    ///    WebView 가 숨는다).
    /// ⇒ `mouse_overlay_open` 은 `crates/tasty-doc-guards/tests/fullscreen_stage_input_gate.rs` 가 **문자열로
    ///    못박고** 있어(정의를 그대로 단언한다) 바꾸면 거기서 큰 소리로 깨진다. 나머지
    ///    셋에는 그런 고정이 없다.
    // release 헤드리스에는 읽는 자리(gui · debug `ui.state` 덤프)가 없다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) settings_open_requested: bool,
    /// 지금 열려 있는 모달의 창 id — **`view.active_modal_id` 의 거울**이다.
    ///
    /// 원본은 `View` 에 있고 `AppState` 는 `View` 에 안 닿는다. 그런데 이 값을 물어야 하는
    /// 쪽(`ui.state` 조회)은 `AppState` 만 받는다. 그 사이를 여는 길이 둘인데, `&View` 를
    /// 조회 경로까지 전파하면 **View 가 없는 헤드리스 호출자**들이 `Option<&View>` 를 받게
    /// 되고 그러면 "모달 없음" 과 "View 가 없음" 이 같은 모양이 된다. 그래서 사본을 둔다.
    ///
    /// 사본이라 원본과 어긋날 수 있다. 어긋나지 않는 근거는 **쓰는 자리가 둘뿐**이라는
    /// 것이고(`app/modal.rs` 의 open/close), 그 둘이 유일한 쓰기 자리임을
    /// `tests/modal_state_has_one_writer.rs` 가 원문 대조로 고정한다.
    ///
    /// ★ `settings_open_requested` 과 성질이 다르다. 그쪽은 **열기 요청** 래치이고 이쪽은 모달이
    /// 등록된 동안 유지되는 **지속 값**이다. 둘을 같은 물음에 쓰지 마라.
    // release 헤드리스에는 읽는 자리(gui · debug `ui.state` 덤프)가 없다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) active_modal_id: Option<u64>,
    /// **어느** 모달인가 — `active_modal_id` 와 같은 자리에서 같이 움직이는 짝이다.
    ///
    /// `active_modal_id` 는 `WindowId` 라 "무언가 떠 있다" 까지만 말한다. 그 값으로
    /// 설정 창을 기다리면 plugins 창이나 quit 창이 떠도 같은 모양이 되어, 시험이
    /// **자기가 안 연 창을 보고 통과할 수 있다.** 종류를 따로 낸다.
    ///
    /// 열거인 것이 요점이다 — 문자열이면 오타가 "그 모달이 아니다" 와 같은 모양이 된다.
    /// IPC 로 나갈 때만 `as_str()` 로 납작해진다(JSON 에 열거가 없다).
    // release 헤드리스에는 읽는 자리(gui · debug `ui.state` 덤프)가 없다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) active_modal_kind: Option<ModalKind>,
    /// plugins 모달 **열기 요청** 플래그 — `settings_open_requested` 과 같은 생애다.
    /// 사이드바 경로만 세우고 같은 `dispatch_pending_modal_opens` 가 다음
    /// 프레임에 소비하며 false 로 되돌린다.
    ///
    /// ⇒ plugins 창을 키보드로 여는 시험은 **아직 없다.** 쓰는 순간 이 필드로는
    ///    관측이 안 돼 설정 쪽과 똑같이 죽는다. 그때는 시험이 아니라 채널을 고쳐라.
    #[cfg(feature = "gui")]
    pub(crate) plugins_open: bool,
    /// Cached sidebar width from settings (logical pixels).
    #[cfg(feature = "gui")]
    pub(crate) sidebar_width: LogicalPx,
    /// Sidebar visibility: false = completely hidden.
    #[cfg(feature = "gui")]
    pub(crate) sidebar_visible: bool,
    /// Sidebar collapsed: true = compact mode (narrow width, icons only).
    #[cfg(feature = "gui")]
    pub(crate) sidebar_collapsed: bool,
    /// 통합 리사이즈 커서 피드백 — `handle_cursor_moved` 가 창 가장자리 hover 를
    /// 감지하면 그 8방향을 저장하고, egui 프레임(`run_egui_frame`)이 매 프레임
    /// `set_cursor_icon` 으로 적용한다. egui 가 winit 커서를 매 프레임 덮으므로
    /// 프레임 내에서만 적용할 수 있어 상태로 보관한다. 데코 없는 Windows/Linux
    /// 창에서만 채워진다(macOS 는 네이티브 데코라 항상 None). 콘텐츠/오버레이 위에서는
    /// None 으로 리셋된다(콘텐츠 우선 입력모델).
    #[cfg(feature = "gui")]
    pub(crate) pending_resize_cursor: Option<winit::window::ResizeDirection>,
    /// switch-number overlay 활성 스냅샷. 현재 눌린 modifier 가 tab/workspace 전환
    /// 단축키와 일치하면 그 대상(+Tab 이면 focused pane id)을 담는다. `MainView` 의
    /// `ModifiersChanged` 가 [`crate::adapters::ui::switch_overlay::switch_target_for`]
    /// 로 갱신하고, 창 비활성/포커스 상실 시 `None` 으로 clear 된다. draw 경로(탭 바
    /// / 사이드바)가 매 프레임 읽어 숫자 키캡 오버레이를 표시할지 결정한다.
    #[cfg(feature = "gui")]
    pub(crate) switch_overlay: Option<crate::adapters::ui::switch_overlay::SwitchOverlayState>,
    /// modifier-hint 오버레이 런타임 상태 — 홀드 시작 시각·anchor modifier·세션 dismiss·
    /// 진행 중 드래그 working rect. `MainView` 의 `ModifiersChanged` 가 `update_hold` 로
    /// 갱신하고, 창 포커스 상실 시 `clear` 된다. draw 경로(`overlay::draw_overlays`)가 매
    /// 프레임 `draw_modifier_hint` 로 읽어 500ms 홀드 후 오버레이를 그린다. 지오메트리 영속값은
    /// `Settings::modifier_hint`(pos/size), 이 필드는 홀드/드래그 세션 상태만.
    #[cfg(feature = "gui")]
    pub(crate) modifier_hint: crate::adapters::ui::modifier_hint_overlay::ModifierHintRuntime,
    /// 튜토리얼(마커 오버레이) 런타임 상태 — 진행 중 주제/step, 목록 팝업 선택·시작
    /// 큐. GUI 전용(사용자 클릭으로만 진행, IPC/CLI 발화 없음 — 불가침 원칙 1).
    /// `overlay::draw_overlays` 말미의 `draw_tutorial_overlay` 가 매 프레임 읽고 전이시킨다.
    #[cfg(feature = "gui")]
    pub(crate) tutorial: crate::adapters::ui::tutorial::TutorialRuntime,
    /// All transient dialog/popup state.
    #[cfg(feature = "gui")]
    pub(crate) dialogs: DialogState,
    /// 측정된 탭바 높이(물리 픽셀). 매 프레임 `adapters::ui::tab_bar` 가 실측값으로
    /// 덮는다 — 여기 있는 것은 **아직 안 쟀다**는 뜻의 자리표시자다.
    ///
    /// 그래서 0 으로 시작한다. 논리 토큰(`SIZING.tab_bar_height`)과 같은 수를 넣으면
    /// **배율 1 에서만 우연히 맞는 값**이 되어, 덮는 자리가 조건부가 되거나 첫 프레임
    /// 전에 읽는 경로가 생겼을 때 배율 2 에서 정확히 절반인 그럴듯한 수로 조용히
    /// 지나간다. 0 은 그 상황에서 탭바가 사라져 눈에 띈다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) tab_bar_height: PhysicalPx,
    /// Popup manager for internal popups (notification panel, etc.).
    #[cfg(feature = "gui")]
    pub(crate) popups: crate::adapters::ui::PopupManager,
    /// 활성 전체화면 무대. 창(=`MainView`)당 **최대 하나**라 `Vec` 이 아니라 `Option`
    /// 이다 — popup 과 달리 z-order·다중 인스턴스 관리가 필요 없다. 창이 여럿이면
    /// 창마다 독립적으로 무대를 가질 수 있다(각 `MainView` 가 자기 `AppState` 를
    /// 가지므로 이 필드 배치 자체가 그 계약이다). 영속화 대상이 아니다 — 재시작이
    /// 무대 상태로 부팅되면 사용자가 창을 조작할 수 없다.
    #[cfg(feature = "gui")]
    pub(crate) fullscreen_stage: Option<crate::adapters::ui::fullscreen::StageState>,
    /// 닫힌 무대의 `on_close` 대기열. 닫는 경로가 무엇이든
    /// [`AppState::close_fullscreen_stage`] 한 곳을 지나 여기 쌓이고, draw 경로의
    /// `fullscreen::drain_on_close_hooks` 가 정확히 1 회 발화시킨다(ADR-0063 패턴).
    #[cfg(feature = "gui")]
    pub(crate) stage_closed_queue: Vec<crate::adapters::ui::fullscreen::StageId>,
    /// 무대 중 DPI/모니터 전환으로 **보류된** 기본 grid 갱신이 있는지.
    ///
    /// 무대는 "원본은 진입 시점 그대로" 가 계약이라 무대 중에는
    /// `CoreState::update_grid_size`(신규 터미널의 기본 cols/rows)를 갱신하지 않는다.
    /// 그대로 버리면 무대를 나온 뒤에도 기본값이 옛 DPI 에 머물므로, 보류 사실을
    /// 여기 남겼다가 무대를 나온 첫 프레임에 한 번 적용한다
    /// (`MainView::resync_scale_factor`).
    #[cfg(feature = "gui")]
    pub(crate) stage_deferred_grid_resync: bool,
    /// Terminal text search state.
    #[cfg(feature = "gui")]
    pub(crate) search: crate::search_state::SearchState,
    /// Listening-port scanner async state machine. Driven by the port scanner
    /// popup: Idle → Loading (background thread + mpsc channel) → Ready / Failed.
    /// Reset to `Idle` when the popup closes.
    #[cfg(feature = "gui")]
    pub(crate) port_scan: crate::adapters::ui::popup::port_scanner::PortScanState,
    /// System-wide scan backing the favorites section's LISTEN/NONE badges.
    /// Independent of `port_scan`'s scope (Tasty/System) — always a full
    /// `scan_all()`, kicked only while at least one favorite is registered.
    /// Reset to `Idle` when the popup closes.
    #[cfg(feature = "gui")]
    pub(crate) port_favorites_scan: crate::adapters::ui::popup::port_scanner::PortScanState,
    /// Command palette UI state — query buffer, selection cursor, and a pending
    /// dispatch slot that MainView drains each frame.
    #[cfg(feature = "gui")]
    pub(crate) command_palette: crate::state::command_palette::CommandPaletteState,
    /// Toast manager for transient in-app notifications (copy feedback, etc.).
    /// 사용자 행동에서만 발사한다. CLI/IPC 경유 동작은 토스트를 만들지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) toasts: crate::adapters::ui::ToastManager,
    /// Banner manager — 4번째 오버레이(공지+action). Toast/Popup 과 별도 매니저.
    /// 사용자 행동에서만 발사한다. IPC/release cascade 는 배너를 띄울 수 없다.
    #[cfg(feature = "gui")]
    pub(crate) banners: crate::adapters::ui::BannerManager,
    /// Cached recent files list (markdown/html open popups). Loaded from disk at
    /// startup and mutated in-place; each mutation saves back to disk.
    pub(crate) recent_files: crate::recent_files::RecentFiles,
    /// Whether the mouse is currently over an open popup (input layer state).
    /// Updated each frame by PopupManager::draw(). Mouse handlers check this
    /// to block events from reaching lower layers (terminal, dividers).
    #[cfg(feature = "gui")]
    pub(crate) popup_hovered: bool,
    /// plugin egui-mesh popup 이 하나라도 열려 있는가 (입력 계층 상태).
    ///
    /// 키/IME 게이트(`view::main`, `view::main::keyboard`)는 `PluginManager` 에
    /// 접근할 수 없다 — 그 타입은 `App` 소유이고 게이트가 있는 `handle_event` 로는
    /// 흘러들지 않는다. 그래서 `popup_hovered` 와 같은 방식으로 렌더 프레임이 채워
    /// 두는 캐시를 읽는다. 갱신은 `plugin_bridge::popup_render::draw_plugin_popups`
    /// 최상단 리셋 + popup 이 있을 때 set 이며, popup 이 없다는 두 조기 반환 경로도
    /// 리셋이 덮는다(stale `true` 는 키보드가 영영 터미널로 못 가는 상태가 된다).
    // release 헤드리스에는 읽는 자리(gui · debug `ui.state` 덤프)가 없다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) plugin_popup_open: bool,
    /// Whether the mouse is currently over a banner (input layer state).
    /// Updated each frame by BannerManager::draw(). 배너는 자기 영역의 마우스를
    /// 소비(뒤로 전파 X)하므로 mouse 핸들러가 이 값으로 하위 레이어 전파를 막는다.
    /// (포커스는 받지 않음 — 마우스 소비만.)
    #[cfg(feature = "gui")]
    pub(crate) banner_hovered: bool,
    /// 마우스가 modifier-hint 오버레이 위인지(입력 레이어 상태). `draw_modifier_hint` 가
    /// 매 프레임 갱신한다. 오버레이는 **키보드 포커스를 받지 않고 마우스만 소비**하므로
    /// (원칙3), mouse 핸들러가 이 값으로 click-to-activate/휠/드래그가 하위 surface 로
    /// 새지 않게 막는다. banner_hovered 와 동일 성질.
    #[cfg(feature = "gui")]
    pub(crate) modifier_hint_hovered: bool,
    /// 이번 프레임에 그려진 각 popup 의 `LayerId`. `PopupManager::draw()` 가 갱신.
    /// `enforce_foreground_z_order`(`src/gfx/gpu/egui_bridge.rs`)가 modifier-hint
    /// 레이어를 부모로 이들을 `set_sublayer` 자식으로 묶을 때 읽는다 — `egui::LayerId`
    /// 의존이라 gui 전용.
    #[cfg(feature = "gui")]
    pub(crate) popup_layers: Vec<egui::LayerId>,
    /// 이번 프레임에 그려진 각 plugin egui-mesh popup 셸의 `LayerId`.
    /// `draw_plugin_popups`(`plugin_bridge/popup_render.rs`)가 갱신. host popup
    /// (`popup_layers`)과 이 목록 사이의 상대 순서를 host↔plugin popup z-order
    /// (`docs/design/systems/popup.md` 규칙 7)에 따라 `set_sublayer`로 강제할 때 쓴다
    /// (`gfx/gpu/egui_bridge.rs`) — 두 popup 종류 모두 `ctx.layer_painter`로 직접
    /// 그리는 raw layer 라 `egui::Area`를 거치지 않고, 따라서 `Areas::order`(Area 기반
    /// 위젯만 자동 등록됨)에 자연히 편입되지 않는다. 등록 호출 순서는 이 `order`에
    /// 전혀 반영되지 않으므로(각 프레임 `GraphicLayers::drain`이 `order`에 없는 레이어를
    /// 별도 맵 순회로 덧붙임 — egui 소스), `set_sublayer`로 명시적으로 관계를 걸지
    /// 않으면 두 popup 종류 사이의 상대 순서는 사실상 비결정적이다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_layers: Vec<egui::LayerId>,
    /// 이번 프레임에 그려진 host popup 들의 히트테스트 rect + z_seq
    /// (`PopupManager::draw` 가 갱신). `draw_plugin_popups` 가 **같은 프레임에** 읽어
    /// "내 위에 host popup 이 이 좌표를 덮는가" 를 판정한다(규칙 7 — 겹친 영역의
    /// 마우스 이벤트는 최상단만 받는다). host draw 가 plugin draw 보다 먼저 돌므로
    /// 이 방향은 stale 이 아니다.
    #[cfg(feature = "gui")]
    pub(crate) host_popup_hittest: Vec<crate::adapters::ui::popup::occlusion::Occluder>,
    /// 이번 프레임 Esc 를 소비할 자격이 있는 host popup(규칙 7 의 키보드 판, ADR-0084).
    /// host/plugin 통틀어 최상단이 host popup 일 때만 `Some` — plugin popup 이 위면
    /// `None` 이고, 그 프레임의 Esc 는 plugin 쪽이 가져간다. popup 의 view 가 Esc 를
    /// 소비하기 전에 이 값을 확인한다.
    #[cfg(feature = "gui")]
    pub(crate) popup_escape_owner: Option<crate::adapters::ui::popup::PopupId>,
    /// 직전 프레임에 그려진 plugin egui-mesh popup 셸 rect + z_seq
    /// (`draw_plugin_popups` 가 갱신). host 쪽 히트테스트가 읽는다 — host draw 가
    /// 먼저 돌기 때문에 **1 프레임 stale** 이다(`popup/draw.rs` 의 outside-click
    /// 분기 주석 참고). 그 "먼저" 를 정하는 순서 계약의 자리는
    /// `gfx/gpu/egui_bridge.rs::run_egui_frame` 의 두 draw 호출이고, 뒤집히면 이 1 이
    /// 조용히 0 이 된다 — `source_guards::frame_draw_order` 가 그것을 문다. 셸 rect(마진 포함)라 `plugin_mesh_popup_regions`(콘텐츠
    /// rect, 물리 px)와는 다른 값이다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_hittest: Vec<crate::adapters::ui::popup::occlusion::Occluder>,
    /// 이번 프레임에 그려진 `banner_layer` Area 의 `LayerId`(banner 는 매 프레임 항상
    /// 그려지므로 첫 프레임 이후 항상 `Some`). `BannerManager::draw()` 가 갱신,
    /// `enforce_foreground_z_order` 가 읽는다.
    #[cfg(feature = "gui")]
    pub(crate) banner_layer: Option<egui::LayerId>,
    /// 이번 프레임에 `modhint_layer` Area 를 실제로 그렸으면 그 `LayerId`(표시 조건
    /// 미충족이면 `None`). `draw_modifier_hint()` 가 갱신, `enforce_foreground_z_order`
    /// 가 읽는다.
    #[cfg(feature = "gui")]
    pub(crate) modifier_hint_layer: Option<egui::LayerId>,
    /// 마우스가 창 가장자리 리사이즈 우선권을 가져야 하는 실제 인터랙티브 chrome
    /// 위젯(타이틀바 창 버튼·Windows 캡션 버튼·상태바 클릭 요소) 위인지(입력 레이어
    /// 상태). 각 위젯이 매 프레임 자신의 `Response::hovered()` 로 갱신한다 —
    /// `egui_consumed`(패널/Area 전체의 bounding rect 단위)와 달리 위젯 단위라
    /// 빈 여백까지 리사이즈를 막지 않는다. `try_begin_os_resize` 가 가장자리 margin
    /// 안에서 리사이즈를 양보할지 판단할 때만 쓰인다.
    #[cfg(feature = "gui")]
    pub(crate) resize_edge_widget_hovered: bool,
    /// Preset store 의 Arc clone — Core 가 owner. UI popup 이 draw 흐름에서
    /// core 인자 없이 lock 으로 read 할 수 있도록 AppState 에 *clone 보유* 만
    /// 한다 (allocation 동일, owner 는 Core). `create_app_state` 가 inject.
    ///
    /// headless 도 `new` 로 이 사본을 받지만 읽는 자(preset popup)가 GUI 뿐이다. 에이전트의
    /// preset IPC 는 이 사본이 아니라 `Core.preset_store` 를 잠근다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "headless receives the preset store copy but only gui popups read it"
        )
    )]
    pub(crate) preset_store: std::sync::Arc<std::sync::Mutex<tasty_presets::PresetStore>>,
    /// Memory store 의 Arc clone — Core 가 owner. UI thread (popup draw_fn) 와
    /// engine state cleanup 이 dispatcher cascade 없이 직접 영속할 때 사용한다.
    /// `Core::with_memory` 와 같은 lock 정책 (poisoning 시 inner 사용).
    pub(crate) memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    /// Surface close lifecycle 알림 큐. close 직후 enqueue되고, App 메인 루프가
    /// drain하여 `surface.closed` 로 broadcast한다(`App::dispatch_pending_surface_lifecycle`).
    /// `state/`는 `plugin/` 의존이 없어 별도 plain struct로 둔다.
    pub(crate) pending_lifecycle_events: Vec<PendingSurfaceClosed>,
    /// Event Bus 1.0 호스트 자동 발화 큐. 호스트 코드 곳곳에서 `enqueue_host_event`로
    /// push하고, App 메인 루프가 drain해 wire payload로 변환·발화한다.
    pub(crate) pending_host_events: Vec<PendingHostEvent>,
    /// `surface.focused` 발화용 변화 감지 상태. tick마다 `focused_surface_id()`와
    /// 비교해 달라졌으면 `SurfaceFocused`를 enqueue한다. focus 전환 경로가 많아
    /// (키보드/마우스/IPC/탭전환/워크스페이스전환) 각각을 hook하기보다 polling이 단순.
    #[cfg(feature = "gui")]
    pub(crate) last_focused_surface_id: Option<u32>,
    /// `workspace.activated` 발화용 변화 감지 상태. `active_workspace` 인덱스가
    /// 가리키는 워크스페이스 ID를 기록해 두고, 다음 tick에서 달라졌다면
    /// `WorkspaceActivated`를 enqueue한다.
    #[cfg(feature = "gui")]
    pub(crate) last_active_workspace_id: Option<u32>,
    /// `tab.focused` 발화용 변화 감지 상태. 활성 워크스페이스의 focused pane이 보유한
    /// 현재 active tab의 (pane_id, tab_id)를 기록. 다음 tick에서 달라졌다면
    /// `TabFocused`를 enqueue한다. pane 전환·in-pane tab 전환을 한꺼번에 다룬다.
    #[cfg(feature = "gui")]
    pub(crate) last_focused_tab: Option<(u32, u32)>,
    /// `tab.created`/`tab.closed`/`tab.moved` 발화용 polling 상태. tab_id →
    /// (pane_id, workspace_id, kind) 스냅샷. `None`은 아직 한 번도 polling하지
    /// 않은 상태(초기 로드된 탭에 대해 spurious `tab.created`가 발화되는 것을 막기
    /// 위해 첫 호출에서는 스냅샷만 만들고 이벤트를 enqueue하지 않는다).
    #[cfg(feature = "gui")]
    pub(crate) last_tab_locations: Option<std::collections::HashMap<u32, (u32, u32, String)>>,

    /// Per-surface host view state for `ExplorerPanel` (directory entry cache, selection,
    /// sidebar tree expansion). `ExplorerPanel` itself only holds navigation/tab state.
    #[cfg(feature = "gui")]
    pub(crate) explorer_views: crate::adapters::ui::surface::explorer::view::ExplorerViewStore,

    /// Per-surface host view state for `DagGraphSurface` (폴링 결과 캐시, 레이아웃
    /// 캐시, 줌/팬/선택). surface 모델은 "어떤 DAG 를 어느 방향으로" 만 들고 있다.
    #[cfg(feature = "gui")]
    pub(crate) dag_graph_views: crate::adapters::ui::surface::dag_graph::DagGraphViewStore,

    /// 사이드바 도구 메뉴 항목. 활성 plugin의 `[[contributes.tool]]`
    /// 항목을 합쳐 관리. PluginManager가 plugin 라이프사이클 변경 시
    /// `set_plugin_items(mgr.plugin_tool_items())`로 갱신한다.
    #[cfg(feature = "gui")]
    pub(crate) tool_registry: crate::plugin::tool_registry::ToolRegistry,

    /// Command palette에 노출할 plugin 전역 command snapshot. `tool_registry`와
    /// 동형 — PluginManager가 plugin 라이프사이클 변경 시
    /// `mgr.plugin_palette_commands()`로 갱신한다(`App::refresh_palette_plugin_commands`,
    /// `tool_registry_dirty`와 동일 트리거 조건). draw 함수는 `PluginManager`에 직접
    /// 접근할 수 없는 `PopupDef` 고정 시그니처 제약 때문에 이 snapshot을 대신 읽는다.
    #[cfg(feature = "gui")]
    pub(crate) palette_plugin_commands: Vec<crate::plugin::command_registry::PluginCommandEntry>,

    /// Command palette에서 plugin 전역 command를 실행했을 때의 (plugin_id, command_id)
    /// 큐. palette popup은 `&mut AppState`만 가지므로 PluginManager에 직접 접근할 수
    /// 없어, 실행 시점에 enqueue하고 App 메인 루프가 drain해
    /// `PluginManager::command_registry`로 action/IPC를 dispatch한다
    /// (`App::dispatch_pending_palette_plugin_commands`, `pending_tool_events`와 동형).
    #[cfg(feature = "gui")]
    pub(crate) pending_plugin_command_invokes: Vec<(String, String)>,

    /// 도구 메뉴 항목 클릭 시 publish해야 할 이벤트 큐. tools_menu가 `&mut AppState`만
    /// 가지므로 PluginManager에 직접 접근할 수 없어, 클릭 시점에 enqueue하고 App 메인
    /// 루프가 drain해 `PluginManager::emit_host_event`로 발화한다.
    #[cfg(feature = "gui")]
    pub(crate) pending_tool_events: Vec<(String, serde_json::Value)>,

    /// 열어야 할 plugin popup 큐(도구 메뉴 · 변환 입력 popup). App 메인 루프가 drain해
    /// `PluginManager::open_popup_instance`로 dispatch.
    #[cfg(feature = "gui")]
    pub(crate) pending_popup_opens: Vec<PendingPopupOpen>,

    /// file_handler 디스패치 결과가 plugin IPC method 일 때의 호출 큐.
    /// `(ipc_method, target)`. App 메인 루프가 drain 해 `PluginManager` 로 forward.
    /// 넣는 자리(`file::dispatch` 의 핸들러 실행)도 비우는 자리도 GUI 다.
    #[cfg(feature = "gui")]
    pub(crate) pending_handler_ipc: Vec<(String, crate::file::format::FileTarget)>,

    /// 외부 drag&drop 으로 파일이 hover 중인 상태. `HoveredFile` 마다 path 누적,
    /// `HoveredFileCancelled` / `DroppedFile` 시 해제. 비주얼 overlay 의 입력.
    #[cfg(feature = "gui")]
    pub(crate) drop_hover: Option<DropHoverState>,

    /// `DroppedFile` 이벤트로 받은 경로 큐. frame end 에서 drain 해
    /// `DomainIntent::DispatchFile` 으로 발화.
    #[cfg(feature = "gui")]
    pub(crate) pending_file_drops: Vec<std::path::PathBuf>,

    /// plugin popup 렌더 중 감지된 close 사유 (outside-click / Escape).
    /// App 메인 루프가 drain해 `PluginManager::close_popup_instance`를 호출한다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_closes: Vec<(u64, tasty_plugin_protocol::PopupCloseReason)>,

    /// plugin popup 콘텐츠 영역 내부 클릭으로 z-order 순번 갱신이 필요한 instance_id 큐
    /// (`docs/design/systems/popup.md` 규칙 7 "클릭된 것이 앞"). 렌더 경로(`draw_plugin_popups`)는
    /// `&PluginManager` 불변 참조만 가지므로 직접 갱신할 수 없어 여기 적재하고, App 메인
    /// 루프가 drain해 `PluginManager::touch_popup_instance_z`를 호출한다(close queue 와 같은 모양).
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_focus_bumps: Vec<u64>,

    /// plugin egui-mesh banner(A3) host 측 생명주기(TTL/close X)로 닫힌 사유.
    /// `draw_plugin_banners` 가 적재하고, App 메인 루프가 drain 해
    /// `PluginManager::close_banner_instance` 를 호출한다(popup closes 와 같은 모양 — 렌더
    /// 경로가 manager 를 직접 mutate 하지 않도록 지연).
    #[cfg(feature = "gui")]
    pub(crate) plugin_banner_closes: Vec<(u64, tasty_plugin_protocol::BannerCloseReason)>,

    /// egui-mesh popup(A2) 합성 영역. `draw_plugin_popups` 가 매 egui frame 채우고,
    /// `gpu.render` 가 host egui pass *후* 각 (instance_id, 물리 콘텐츠 rect)에 plugin
    /// mesh 를 합성한다. 셸(scrim/bg/border)은 host egui 가, 내용만 plugin mesh 가 그린다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_popup_regions: Vec<(u64, crate::model::PhysicalRect)>,

    /// 키 포커스를 가진 egui-mesh popup 이 알려온 IME 커서 영역(창 물리 좌표).
    /// `draw_plugin_popups` 가 매 egui frame 채우고 `MainView::update_ime_cursor_area` 가
    /// 읽어 winit `set_ime_cursor_area` 로 OS IME 후보창 위치를 정한다.
    ///
    /// 렌더 프레임이 채우는 캐시라 `plugin_popup_open` 과 같은 프레임 간 전달 패턴이다 —
    /// IME 커서 영역을 아는 것은 plugin 프로세스의 egui 뿐이고(host egui 에는 대응 위젯이
    /// 없어 `platform_output.ime` 가 늘 `None`), 그 값은 `PopupPaintFrame` 알림으로
    /// 돌아온다. `None` 이면 그 popup 에 편집 위젯 포커스가 없다는 뜻이라 후보창 위치를
    /// 정하지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_ime_cursor_area: Option<crate::model::PhysicalRect>,

    /// egui-mesh popup 인스턴스별 forward 추적 상태. **칸의 정의도 dirty 판정도
    /// [`crate::plugin_bridge::MeshForwardCommon`] 한 곳에서 나온다** — banner·surface 와
    /// "같은 모양" 이라서가 아니라 *같은 타입*이라서 갈릴 자리가 없다. 무입력 강제
    /// repaint 는 그 타입 밖에 있고 **popup 만의 칸이 아니다** — banner 도 같은 칸을
    /// 갖는다(아래 `plugin_mesh_popup_pending_repaint` 와
    /// `plugin_mesh_banner_pending_repaint`).
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_popup_forward:
        std::collections::HashMap<u64, crate::plugin_bridge::MeshForwardCommon>,

    /// 사용자의 확정형 입력(포인터 버튼 누름 · 키 누름)을 받은 egui-mesh popup 인스턴스와 그
    /// 소유 plugin id. `draw_plugin_popups` 가 입력을 forward 하는 자리에서 세우고, 닫힌
    /// 인스턴스는 `plugin_mesh_popup_forward` 와 같은 자리에서 걷힌다.
    ///
    /// plugin 이 자기 popup 안의 사용자 조작으로 host 를 부를 때, host 가 그 호출을 **사용자
    /// 행동**으로 칠 수 있는 유일한 근거다 — release 에는 입력 주입이 없으므로 이 칸에 오른
    /// 인스턴스는 사람이 만졌다(ADR-0526).
    #[cfg(feature = "gui")]
    pub(crate) plugin_popup_user_activated: std::collections::HashMap<u64, String>,

    /// plugin webview surface 마다 가장 최근 navigation 시도가 근거일 때(엔진이 사용자 제스처로
    /// 보고했고 소유 plugin 이 쓴 페이지 위에서 났다) 그 시도와 그것을 통지받은 plugin.
    /// `sync_webviews` 가 시도를 plugin 에 통지하는 자리에서 정하고(근거가 못 되는 시도와, 작성자가
    /// 소유 plugin 으로 바뀐 프레임은 그 surface 의 기록을 지운다), webview 가 사라진 surface 의 기록은 같은 자리에서 걷힌다. plugin 이 그 시도의 URL 을 되대면 한 번
    /// 쓰이고 사라진다 — webview 안의 사용자 클릭을 host 가 사용자 행동으로 칠 유일한 근거다
    /// (ADR-0568, [`crate::plugin_bridge::user_navigation`]).
    #[cfg(feature = "gui")]
    pub(crate) webview_user_navigations: crate::plugin_bridge::user_navigation::UserNavigations,

    /// egui-mesh banner(A3) 합성 영역. `draw_plugin_banners` 가 매 egui frame 채우고,
    /// `gpu.render` 가 host egui pass *후* 각 (instance_id, 물리 콘텐츠 rect)에 plugin
    /// mesh 를 합성한다. 셸(컨테이너/border/close X/카운트다운)은 host egui(banner
    /// manager)가, 내용만 plugin mesh 가 그린다. popup regions 와 **같은 튜플 타입이고
    /// 같은 소비자**(`gpu.render` 의 합성 pass)를 먹인다 — 한쪽 모양만 바뀌면 그 소비자가
    /// 컴파일되지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_banner_regions: Vec<(u64, crate::model::PhysicalRect)>,

    /// egui-mesh banner 인스턴스별 forward 추적 상태. popup 무리와 **같은 타입**이다
    /// ([`crate::plugin_bridge::MeshForwardCommon`]) — 두 채널의 dirty 판정이 한 곳에서
    /// 나오므로 한쪽만 고쳐지는 형태의 drift 가 생기지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_banner_forward:
        std::collections::HashMap<u64, crate::plugin_bridge::MeshForwardCommon>,

    /// 무입력 강제 repaint 를 요청받은 egui-mesh popup 인스턴스. 일반 dirty
    /// 판정(geom/input/theme 변경)은 "plugin 내부 상태만 바뀐" 갱신을 감지하지
    /// 못하므로(`draw_plugin_popups`), 이 요청을 채워두면 다음 frame 이 geometry/입력
    /// 변화 없이도 `set_context` 를 재forward 해 plugin 이 새 데이터로 다시 그리게
    /// 한다(`MeshForwardCommon::pending_full` 이 텍스처를 요구하는 것과 달리 repaint
    /// 자체를 강제한다).
    ///
    /// 아래 `plugin_mesh_banner_pending_repaint` 와 **평행한 칸**이다 — 같은 타입·같은
    /// 목적이고, 두 칸 다 plugin 의 self-repaint 요청(`mark_invalidated_popups_dirty`·
    /// `mark_invalidated_banners_dirty`)이 채운다. 다른 것은 **추가 진입로**뿐이다:
    /// popup 은 그 위에 ADR-0056 의 비동기 host→plugin push 결과(git-viewer 원격 조회
    /// 결과, `attach_client.rs` 두 자리)가 같은 칸을 쓰고, banner 는 self-repaint 하나
    /// 뿐이다. 두 칸을 `MeshForwardCommon` 으로 합치지 않은 이유는
    /// [`crate::plugin_bridge::MeshForwardCommon`] 의 doc 에 있다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_popup_pending_repaint: std::collections::HashSet<u64>,

    /// 위 popup 칸의 banner 대응 — 무입력 강제 repaint 를 요청받은 egui-mesh banner
    /// 인스턴스. banner 도 같은 `EguiMeshCore` 를 쓰므로 egui 가 다음 pass 를 요구할
    /// 수 있고(hover fade·스크롤 스무딩·스피너), 그 요구는 geom/입력/theme 어느 것도
    /// 안 바꾸므로 `draw_plugin_banners` 의 일반 dirty 판정에 안 걸린다.
    ///
    /// **채우는 자리는 popup 보다 하나 적다.** popup 은 이 칸을 두 종류의 사건이
    /// 채운다 — (1) plugin 의 self-repaint 요청, (2) git-viewer 원격 조회 결과처럼
    /// 비동기 host→plugin push 뒤의 강제 repaint(`attach_client.rs` 두 자리). (2) 는
    /// `com.tasty.git-viewer` 전용 경로이고 그 plugin 은 banner 를 기여하지 않으므로
    /// banner 에는 대응 자리가 **없다** — 대칭을 맞추려고 만들지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) plugin_mesh_banner_pending_repaint: std::collections::HashSet<u64>,

    /// 호스트 내부 Intent 큐. 발화자가 push 만 하고, `App::dispatch_pending_intents`
    /// 가 메인 루프에서 drain 한다. UI Intent (`Intent::Ui`) 와 Domain Intent
    /// (`Intent::Domain`) 가 한 큐 위에서 처리됨. 설계:
    /// `docs/design/flows/action-dispatch.md`, `intent-ui-vs-domain.md`.
    pub(crate) pending_intents: Vec<crate::intent::DispatchedIntent>,
}

/// 열기를 기다리는 plugin popup 한 건.
///
/// `context` 는 plugin 에 넘기는 open context 이고, `target_surface` 는 **host 가** 이 popup
/// 의 소속 범위 대상으로 바인딩할 surface 다. 둘을 가르는 이유는 plugin context 의 키
/// 이름(`surface_id` 등)을 host 가 해석하지 않기 위해서다. 선언이 `scope = "surface"` 가
/// 아니면 `target_surface` 는 쓰이지 않는다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
pub(crate) struct PendingPopupOpen {
    pub(crate) plugin_id: String,
    pub(crate) popup_id: String,
    pub(crate) context: serde_json::Value,
    pub(crate) target_surface: Option<u32>,
}

/// 외부 drag&drop hover 중 누적되는 파일 경로 + 시작 cursor 좌표.
/// winit `HoveredFile` 이 N 파일에 대해 N번 발화하므로 `paths` 에 누적.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Default)]
pub struct DropHoverState {
    pub(crate) paths: Vec<std::path::PathBuf>,
    /// hover 시작 시점의 cursor position (physical pixels). `CursorLeft` 또는
    /// `CursorMoved` 가 drag 중에 발화되지 않을 수 있어 보수적으로 시작점만 기록.
    /// 향후 drop indicator 정밀화 시 read 예정.
    #[allow(dead_code)]
    pub(crate) cursor: Option<(f32, f32)>,
}

impl AppState {
    /// Memory store 의 lock 안에서 함수를 실행한다. Mutex poisoning 시
    /// poison 해제 후 inner 를 사용 — `Core::with_memory` 와 동일한 정책.
    /// state cleanup / popup draw_fn 등 Core 인자가 cascade 로 도달하지 못하는
    /// 표면에서 동일 port handle 로 접근한다.
    pub(crate) fn with_memory<R>(
        &self,
        f: impl FnOnce(&mut dyn tasty_memory::MemoryStorage) -> R,
    ) -> R {
        let mut guard = crate::poison::recover_mutex(
            self.memory.lock(),
            crate::core::MEMORY_WHAT,
            &crate::core::MEMORY_POISONED,
        );
        f(&mut *guard)
    }

    /// Creates initial state with one workspace, one pane, one tab, one terminal.
    pub fn new(
        engine: &mut CoreState,
        preset_store: std::sync::Arc<std::sync::Mutex<tasty_presets::PresetStore>>,
        memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    ) -> Self {
        let active_workspace = engine.restored_active_workspace.take().unwrap_or(0);
        Self {
            preset_store,
            memory,
            active_workspace,
            #[cfg(any(feature = "gui", debug_assertions, test))]
            category_last_active: std::collections::HashMap::new(),
            #[cfg(any(feature = "gui", debug_assertions))]
            settings_open_requested: false,
            #[cfg(any(feature = "gui", debug_assertions))]
            active_modal_id: None,
            #[cfg(any(feature = "gui", debug_assertions))]
            active_modal_kind: None,
            #[cfg(feature = "gui")]
            plugins_open: false,
            #[cfg(feature = "gui")]
            sidebar_width: engine.settings.appearance.sidebar_width,
            #[cfg(feature = "gui")]
            sidebar_visible: true,
            #[cfg(feature = "gui")]
            sidebar_collapsed: false,
            #[cfg(feature = "gui")]
            pending_resize_cursor: None,
            #[cfg(feature = "gui")]
            switch_overlay: None,
            #[cfg(feature = "gui")]
            modifier_hint: crate::adapters::ui::modifier_hint_overlay::ModifierHintRuntime::default(
            ),
            #[cfg(feature = "gui")]
            tutorial: crate::adapters::ui::tutorial::TutorialRuntime::default(),
            #[cfg(feature = "gui")]
            dialogs: DialogState::new(),
            #[cfg(any(feature = "gui", test))]
            tab_bar_height: PhysicalPx(0.0),
            pending_lifecycle_events: Vec::new(),
            pending_host_events: Vec::new(),
            #[cfg(feature = "gui")]
            last_focused_surface_id: None,
            #[cfg(feature = "gui")]
            last_active_workspace_id: None,
            #[cfg(feature = "gui")]
            last_focused_tab: None,
            #[cfg(feature = "gui")]
            last_tab_locations: None,
            #[cfg(feature = "gui")]
            popup_hovered: false,
            #[cfg(any(feature = "gui", debug_assertions))]
            plugin_popup_open: false,
            #[cfg(feature = "gui")]
            banner_hovered: false,
            #[cfg(feature = "gui")]
            modifier_hint_hovered: false,
            #[cfg(feature = "gui")]
            popup_layers: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_layers: Vec::new(),
            #[cfg(feature = "gui")]
            host_popup_hittest: Vec::new(),
            #[cfg(feature = "gui")]
            popup_escape_owner: None,
            #[cfg(feature = "gui")]
            plugin_popup_hittest: Vec::new(),
            #[cfg(feature = "gui")]
            banner_layer: None,
            #[cfg(feature = "gui")]
            modifier_hint_layer: None,
            #[cfg(feature = "gui")]
            resize_edge_widget_hovered: false,
            recent_files: crate::recent_files::RecentFiles::load(),
            #[cfg(feature = "gui")]
            popups: {
                let mut pm = crate::adapters::ui::PopupManager::new();
                let ui_zoom = engine.settings.appearance.ui_scale_factor();
                for def in crate::adapters::ui::popup::defs::all_defs() {
                    pm.register_def(def, ui_zoom);
                }
                pm
            },
            #[cfg(feature = "gui")]
            fullscreen_stage: None,
            #[cfg(feature = "gui")]
            stage_closed_queue: Vec::new(),
            #[cfg(feature = "gui")]
            stage_deferred_grid_resync: false,
            #[cfg(feature = "gui")]
            search: crate::search_state::SearchState::new(),
            #[cfg(feature = "gui")]
            port_scan: crate::adapters::ui::popup::port_scanner::PortScanState::Idle,
            #[cfg(feature = "gui")]
            port_favorites_scan: crate::adapters::ui::popup::port_scanner::PortScanState::Idle,
            #[cfg(feature = "gui")]
            command_palette: crate::state::command_palette::CommandPaletteState::default(),
            #[cfg(feature = "gui")]
            toasts: crate::adapters::ui::ToastManager::new(),
            #[cfg(feature = "gui")]
            banners: crate::adapters::ui::BannerManager::new(),
            #[cfg(feature = "gui")]
            explorer_views: Default::default(),
            #[cfg(feature = "gui")]
            dag_graph_views: Default::default(),
            #[cfg(feature = "gui")]
            tool_registry: crate::plugin::tool_registry::ToolRegistry::new(),
            #[cfg(feature = "gui")]
            palette_plugin_commands: Vec::new(),
            #[cfg(feature = "gui")]
            pending_plugin_command_invokes: Vec::new(),
            #[cfg(feature = "gui")]
            pending_tool_events: Vec::new(),
            #[cfg(feature = "gui")]
            pending_popup_opens: Vec::new(),
            #[cfg(feature = "gui")]
            pending_handler_ipc: Vec::new(),
            #[cfg(feature = "gui")]
            drop_hover: None,
            #[cfg(feature = "gui")]
            pending_file_drops: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_closes: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_focus_bumps: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_banner_closes: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_popup_regions: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_popup_ime_cursor_area: None,
            #[cfg(feature = "gui")]
            plugin_mesh_popup_forward: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            plugin_popup_user_activated: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            webview_user_navigations: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_regions: Vec::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_forward: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_popup_pending_repaint: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            plugin_mesh_banner_pending_repaint: std::collections::HashSet::new(),
            pending_intents: Vec::new(),
        }
    }

    /// Intent 발화. `App::dispatch_pending_intents` 가 메인 루프에서 drain.
    /// UI Intent / Domain Intent 모두 본 큐로 발화.
    pub fn dispatch_intent(&mut self, intent: crate::intent::DispatchedIntent) {
        self.pending_intents.push(intent);
    }

    /// 현재까지 발화된 Intent 를 모두 꺼내고 큐를 비운다.
    pub fn take_pending_intents(&mut self) -> Vec<crate::intent::DispatchedIntent> {
        std::mem::take(&mut self.pending_intents)
    }

    /// 파일 기반 surface 를 여는 인텐트가 수렴하는 지점에서 `kind` 의 최근 목록에 1회
    /// 기록한다. surface params 의 `file` 키를 정규화 dedup 으로 기록하며, `file` 키가
    /// 없거나 비어 있으면 no-op(파일 없는 변환·kind 만 바뀌는 케이스는 기록 대상 아님).
    ///
    /// **generic per-kind**: host 는 특정 kind 이름을 모른다. 호출부가 매니페스트
    /// `records_recent` capability 로 기록 대상 여부를 판정한 뒤 이 함수에 kind 를 넘긴다.
    ///
    /// 배치 근거: 파일-open 진입점(파일-열기 팝업·주소창 navigate·링크 클릭·convert)이
    /// 모두 `Intent::NewTab`/`Intent::ConvertSurface`(또는 file-dispatch 의 직접
    /// `CreateTab`)로 수렴하므로, 그 인텐트 계층에서 공용으로 1회 기록한다. generic
    /// surface factory 는 `AppState` 접근이 없어 여기서 처리한다.
    pub(crate) fn record_recent(&mut self, kind: &str, params: &serde_json::Value) {
        if let Some(file) = params.get("file").and_then(|v| v.as_str())
            && !file.is_empty()
        {
            self.recent_files.add(kind, file.to_string());
        }
    }

    /// `convert_requires_input` kind 의 파일 입력 팝업을 여는 요청을 enqueue 한다.
    ///
    /// host 는 kind 이름을 모른다 — registry 의 `convert_input_popup`(등록 시점에
    /// `<plugin_id>/<popup_id>` 로 qualify 됨) 데이터만 따라 `open_popup_instance` 로
    /// 여는 요청을 `pending_popup_opens` 에 넣는다(App 메인 루프가 drain →
    /// PluginManager). `convert_surface_id` 가 `Some` 이면 context 에 실어 plugin 이
    /// 제자리 변환(`markdown.navigate`), `None` 이면 새 탭으로 연다.
    ///
    /// 반환값: 요청을 enqueue 했으면 `true`, kind/팝업 미상이면 `false`(warn 로그).
    #[cfg(feature = "gui")]
    pub(crate) fn enqueue_convert_input_popup(
        &mut self,
        engine: &CoreState,
        kind: &str,
        convert_surface_id: Option<u32>,
    ) -> bool {
        let Some(popup_ref) = engine
            .surface_registry
            .get(kind)
            .and_then(|d| d.convert_input_popup.clone())
        else {
            tracing::warn!(
                "convert-input popup: kind '{kind}' has no convert_input_popup capability"
            );
            return false;
        };
        let Some((plugin_id, local_id)) = popup_ref.split_once('/') else {
            tracing::warn!("convert-input popup: malformed convert_input_popup '{popup_ref}'");
            return false;
        };
        // cwd 는 **팝업의 대상 surface** 기준이다 — 제자리 변환이면 그 surface, 새 탭이면
        // focus. 대상을 지정한 호출자(컨텍스트 메뉴 등)에서 focus 와 대상이 어긋나도 그 대상의
        // 폴더를 본다.
        let origin = convert_surface_id.or_else(|| self.focused_surface_id(engine));
        let mut context = self.popup_surface_context(engine, origin);
        if let Some(sid) = convert_surface_id {
            context["surface_id"] = serde_json::json!(sid);
        }
        // 소속 범위의 대상도 같은 origin 이다 — popup 이 다루는 surface 와 popup 이 뜨는
        // surface 가 갈리지 않는다. 쓸지는 매니페스트 `scope` 가 정한다.
        self.pending_popup_opens.push(PendingPopupOpen {
            plugin_id: plugin_id.to_string(),
            popup_id: local_id.to_string(),
            context,
            target_surface: origin,
        });
        true
    }

    /// plugin popup 에 넘기는 surface 컨텍스트 — Tools 메뉴 popup 과 변환 입력 popup 이 같은
    /// 식을 쓴다(둘로 두면 한쪽만 바뀐다).
    ///
    /// 키:
    /// - `cwd` — `inherit_cwd` 게이트를 건 **로컬** cwd(없으면 `null`). 새 surface 를 만드는
    ///   소비자용이다. mirror surface 면 원격 경로라 `null` 이다.
    /// - `observed_cwd` — 게이트 없는 로컬 cwd. "지금 어느 폴더를 보고 있나" 를 알려 주는
    ///   소비자(파일 피커 시작 위치)용이다(ADR-0267 결정 5).
    /// - `remote_cwd` — mirror surface 의 원격 cwd 문자열. `cwd` 키에는 절대 싣지 않는다 —
    ///   이 구분을 모르는 plugin 이 원격 경로를 로컬 경로로 쓰지 못하게 한다(ADR-0267 결정 3).
    /// - `origin_surface_id` — 이 컨텍스트가 유래한 로컬 surface id.
    /// - `mirror: true` · `local_surface_id` — mirror workspace 판별
    ///   (`docs/adr/0056-git-viewer-remote-attach-git-query-channel.md`). `inherit_cwd` 와
    ///   무관하게 항상 판정한다 — "원격 인지" 는 그 설정이 꺼져 있어도 필요한 정보다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn popup_surface_context(
        &self,
        engine: &CoreState,
        surface_id: Option<u32>,
    ) -> serde_json::Value {
        use crate::core::state::SurfaceCwd;
        let Some(sid) = surface_id else {
            return serde_json::json!({ "cwd": null });
        };
        let cwd = self
            .resolve_inherit_cwd_from_surface(engine, sid)
            .map(|p| p.to_string_lossy().into_owned());
        let mut context = serde_json::json!({ "cwd": cwd, "origin_surface_id": sid });
        match engine.surface_cwd(sid) {
            Some(SurfaceCwd::Local(p)) => {
                context["observed_cwd"] = serde_json::json!(p.to_string_lossy());
            }
            Some(SurfaceCwd::Remote(r)) => {
                context["remote_cwd"] = serde_json::json!(r.as_str());
            }
            None => {}
        }
        if engine.is_mirror_surface(sid) {
            context["mirror"] = serde_json::json!(true);
            context["local_surface_id"] = serde_json::json!(sid);
        }
        context
    }

    /// Returns true if any dialog with text input is open.
    ///
    /// headless 빌드에는 dialog 가 없으므로 항상 `false` 다. 그래도 이 판정이 debug 빌드의
    /// 두 조합에 다 있는 것은(release 헤드리스에는 없다) `ui.state` debug 덤프가 이 값과
    /// [`Self::keyboard_overlay_open`] 을 두 조합에서 같은 키로 찍기 때문이다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub fn has_input_dialog_open(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.dialogs.has_text_input_open()
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    /// 키/IME 를 host egui 로 들여보낼지(그리고 터미널 포워딩을 막을지) 판정한다.
    ///
    /// 같은 식을 두 게이트(`view::main` 의 egui feed, `view::main::keyboard` 의 터미널
    /// 포워딩)가 **각자** 계산하던 것을 단일 출처로 합쳤다 — 한쪽만 바뀌면 "egui 에는
    /// 먹였는데 터미널로도 갔다"(이중 처리) 또는 "egui 에 안 먹였는데 터미널도
    /// 차단"(입력 유실)이 된다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub(crate) fn keyboard_overlay_open(&self) -> bool {
        #[cfg(feature = "gui")]
        let host_popup_focused = self.popups.has_focused()
            || (self.tutorial.active.is_some() && self.tutorial.keyboard_focus);
        #[cfg(not(feature = "gui"))]
        let host_popup_focused = false;
        keyboard_overlay_open(
            self.settings_open_requested,
            self.has_input_dialog_open(),
            host_popup_focused,
            self.plugin_popup_open,
        )
    }

    /// 이 plugin popup instance 가 연 host popup(자식)이 아직 살아 있는가 (ADR-0084).
    ///
    /// 소유 관계는 자식 쪽(`FilePickerRequester.owner_popup_instance`)에만 기록되므로
    /// 이 조회가 곧 단일 진실이다 — 부모 쪽에 사본을 두지 않아 둘이 어긋날 수 없다.
    /// host 는 plugin id/kind 를 보지 않는다(핵심 원칙 2 — generic 계약).
    #[cfg(feature = "gui")]
    pub(crate) fn plugin_popup_has_open_child(&self, instance_id: u64) -> bool {
        self.dialogs
            .file_picker
            .as_ref()
            .and_then(|d| d.requester.as_ref())
            .and_then(|r| r.owner_popup_instance)
            == Some(instance_id)
    }

    /// Returns true if any egui overlay is visible.
    ///
    /// plugin egui-mesh popup 도 센다 — 이 값의 소비처(webview 가리기)는 "네이티브
    /// 뷰가 egui 오버레이를 덮지 않게" 하는 목적이고, plugin popup 도 같은 wgpu
    /// 표면 위에 그려지므로 host popup 과 구분할 이유가 없다.
    #[cfg(feature = "gui")]
    pub fn has_egui_overlay_open(&self) -> bool {
        // `settings_open_requested`/`plugins_open` 은 **여기 안 든다.** 그 둘은 "열려 있는가" 가 아니라
        // 한 프레임짜리 **열기 요청**이고(선언부 주석 참조), 두 모달은 `event_loop.create_window`
        // 로 뜨는 **별도 winit 창**이라 이 창의 wgpu 표면을 덮지 않는다.
        //
        // 넣었을 때 실제로 벌어지던 일: `handle_redraw` 의 순서가 지움
        // (`dispatch_pending_modal_opens`) → egui 패스(`render_if_dirty` 안, 여기서
        // `adapters/ui/draw.rs` 가 플래그를 세운다) → 읽음(`sync_webviews`)이라, 버튼을 누른
        // **그 한 프레임에만** 참이었다. (줄번호로 적었더니 그 함수에 주석 한 덩이가 들어간
        // 것만으로 셋 다 낡았다 — 이름으로 적는다.) 그 프레임에 WebView 를 전부
        // `set_visible(false)` 하고 키보드 포커스를 풀었다가 다음 프레임에 되살렸다. **가릴 이유도 없고(다른 창이다), 한 프레임이라 가리는 구실도
        // 못 했다** — 둘 다 아니면 죽은 항이 아니라 틀린 항이다.
        //
        // ★ 그러니 여기에 **지속하는 모달 상태를 대신 넣지 마라.** 그러면 설정 창이 열려 있는
        //   내내 WebView 가 숨는다 — 한 프레임짜리 결함이 분 단위 결함이 된다. "설정 창이 떠
        //   있는가" 를 물어야 하는 소비자가 볼 값은 이 래치가 아니라
        //   `view::View::is_modal_active()` 다.
        let open = self.dialogs.has_any_overlay() || self.plugin_popup_open;
        // 전체화면 무대도 오버레이로 친다. 이 판정의 소비자 중 하나가 WebView 표시
        // 여부(`MainView::sync_webviews`)인데, WebView 는 OS 네이티브 자식 뷰라 wgpu
        // 표면 **위**에 있다 — 안 그리는 것만으로는 사라지지 않고 무대를 뚫고 나온다.
        // 반드시 `set_visible(false)` 가 필요하고, 그 게이트가 바로 이 함수다.
        open || self.popups.has_visible_open()
            || self.fullscreen_stage.is_some()
            || self.tutorial.active.is_some()
    }

    /// 전체화면 무대 진입. 정의 테이블에 없는 id 는 거부하고 `false` 를 반환한다
    /// (선언하지 않은 것은 무대에 올라갈 수 없다).
    ///
    /// 이미 다른 무대가 올라와 있으면 **그 무대를 닫고**(닫힘 훅 경유) 새 무대를
    /// 올린다 — 무대는 창당 하나라는 계약을 호출부가 신경 쓰지 않아도 되게 한다.
    /// 같은 id 를 다시 열면 no-op 이다(닫았다 여는 것이 아니다).
    ///
    /// 호출부는 이 뒤에 `mark_dirty()` 로 프레임을 유도해야 한다. `AppState` 라우팅을
    /// 지나는 IPC 경로는 `dirty` 가 이미 서지만, App 레벨 경로
    /// (`debug.fullscreen.*`)는 직접 세운다.
    // 사용자 진입 경로는 popup 타이틀바의 전체화면 버튼
    // (`popup::frame::draw_popup_layer`), 에이전트 진입 경로는 debug 전용
    // `debug.fullscreen.open` (`docs/dev-guide/debug-ipc.md`).
    #[cfg(feature = "gui")]
    pub fn open_fullscreen_stage(&mut self, id: &str) -> bool {
        let Some(def) = crate::adapters::ui::fullscreen::defs::find(id) else {
            return false;
        };
        if self
            .fullscreen_stage
            .as_ref()
            .is_some_and(|s| s.id == def.id())
        {
            return true;
        }
        self.close_fullscreen_stage();
        self.fullscreen_stage = Some(crate::adapters::ui::fullscreen::StageState { id: def.id() });
        true
    }

    /// 전체화면 무대 종료 — **닫는 경로 전부가 지나는 유일한 지점**(ADR-0063 패턴).
    /// 닫힌 무대 id 를 훅 대기열에 넣고, draw 경로가 `on_close` 를 1 회 발화한다.
    /// 활성 무대가 없었으면 `false`.
    #[cfg(feature = "gui")]
    pub fn close_fullscreen_stage(&mut self) -> bool {
        match self.fullscreen_stage.take() {
            Some(stage) => {
                self.stage_closed_queue.push(stage.id);
                true
            }
            None => false,
        }
    }

    /// 지금 올라와 있는 무대 id.
    #[cfg(feature = "gui")]
    pub fn fullscreen_stage_id(&self) -> Option<crate::adapters::ui::fullscreen::StageId> {
        self.fullscreen_stage.as_ref().map(|s| s.id)
    }

    /// 전체화면 무대가 활성인지. 렌더 파이프라인 게이트가 읽는다.
    ///
    /// headless 빌드에는 무대 개념이 없으므로 항상 `false` — 무대는 화면 투영이라
    /// 대응 도메인이 없다(`docs/identity.md` §2.2).
    // 읽는 자리가 gui 게이트들과 debug `ui.state` 덤프뿐이다.
    #[cfg(any(feature = "gui", debug_assertions))]
    pub fn fullscreen_stage_active(&self) -> bool {
        #[cfg(feature = "gui")]
        {
            self.fullscreen_stage.is_some()
        }
        #[cfg(not(feature = "gui"))]
        {
            false
        }
    }

    /// Clean up all state associated with a closed surface:
    /// surface metadata, per-surface host view state, and memory entries
    /// scoped to this surface (regular + secret).
    ///
    /// `persist_id` 는 닫히는 surface 가 들고 있던 `scrollback_persist_id` 필드값을
    /// 호출자가 미리 뽑아 넘긴다. `Some` 일 때만 `~/.tasty/scrollback/<id>.bin` 파일이
    /// 삭제된다.
    pub(crate) fn cleanup_surface(
        &mut self,
        engine: &mut CoreState,
        surface_id: u32,
        persist_id: Option<String>,
    ) {
        let mut sink = crate::close_trace::CleanupSums::default();
        self.cleanup_surface_traced(engine, surface_id, persist_id, &mut sink);
    }

    /// `cleanup_surface` 와 같은 일을 하되 단계별 소요를 `sums` 에 누적한다
    /// (close 계측 C5a~C5e). surface 마다 로그를 찍지 않고 합계만 모으는 이유는
    /// `crate::close_trace` 모듈 문서 참조.
    pub(crate) fn cleanup_surface_traced(
        &mut self,
        engine: &mut CoreState,
        surface_id: u32,
        persist_id: Option<String>,
        sums: &mut crate::close_trace::CleanupSums,
    ) {
        use std::time::Instant;
        sums.surfaces += 1;
        let t = Instant::now();
        Self::delete_scrollback_persist(persist_id);
        sums.scrollback_delete += t.elapsed();
        let t = Instant::now();
        self.drop_terminal(engine, surface_id);
        sums.terminal_drop += t.elapsed();
        let t = Instant::now();
        self.drop_surface_indices(engine, surface_id);
        sums.indices_drop += t.elapsed();
        let t = Instant::now();
        self.purge_surface_memory_scope(surface_id);
        sums.memory_purge += t.elapsed();
        // surface 가 사라졌으니 그 자리의 점유 흔적도 지운다. 안 지우면 레지스트리가
        // 없는 surface 를 점유 중이라고 계속 말한다(`attach.list` · `surface_held_by`).
        // 워크스페이스 락은 건드리지 않는다 — 형제 surface 는 아직 살아 있다.
        engine.attach.forget_closed_surface(surface_id);
    }

    fn delete_scrollback_persist(persist_id: Option<String>) {
        if let Some(pid) = persist_id {
            crate::scrollback_store::delete(&pid);
        }
    }

    /// TerminalStore 의 Terminal/부속 데이터 cascade 정리.
    /// store.remove 가 Terminal drop → PTY SIGHUP 발사 + busy/scrollback_persist
    /// /deferred/pending_scrollback_inject 까지 함께 정리.
    fn drop_terminal(&mut self, engine: &mut CoreState, surface_id: u32) {
        engine.pending_scrollback_inject.remove(&surface_id);
        if let Some(old_terminal) = engine.terminals.remove(surface_id) {
            drop(old_terminal); // SIGHUP — 명시 drop.
        }
    }

    /// per-surface 로 유지되는 host-side 인덱스/게이트를 모두 잊는다 — 미제거 시
    /// surface 마다 영구 누적(누수).
    fn drop_surface_indices(&mut self, engine: &mut CoreState, surface_id: u32) {
        #[cfg(feature = "gui")]
        {
            self.explorer_views.drop_view(surface_id);
            self.dag_graph_views.drop_view(surface_id);
        }
        engine.command_index.drop_surface(surface_id);
        engine.observer_router.drop_surface(surface_id);
        engine.hook_manager.remove_surface_hooks(surface_id);
        engine.forget_shell_integration_hint(surface_id);
        // waker dedup 게이트 제거 — 미제거 시 surface 마다 영구 누적(누수).
        if let Some(factory) = engine.waker_factory.as_ref() {
            factory.forget_surface(surface_id);
        }
    }

    /// 닫힌 surface 의 memory scope 를 통째로 정리한다 — regular + secret 양쪽.
    ///
    /// **surface close 당 `purge_scope(Scope::Surface)` 는 여기 한 번뿐이다.** 과거엔
    /// `SurfaceMetaStore::remove` 도 같은 인자로 같은 함수를 불러 surface 마다 2회
    /// 돌았고, `purge_scope` 는 매 호출 끝에 `SELECT SUM(LENGTH(value)) FROM memory`
    /// 풀스캔을 하므로 탭 N 개 워크스페이스 close 가 풀스캔 2N 회를 렌더 스레드에서
    /// 직렬로 태웠다. scope 전체 teardown 은 meta 키 네임스페이스 facade 가 아니라
    /// 이 memory 수명 경로가 소유한다 — `Scope::Surface` 에는 plugin/Lua 가 memory
    /// API 로 직접 쓴 키도 들어 있어 meta 만의 관심사가 아니기 때문이다.
    fn purge_surface_memory_scope(&mut self, surface_id: u32) {
        let scope = tasty_memory::Scope::Surface(surface_id);
        match self.with_memory(|m| m.purge_scope(&scope)) {
            Ok(stats) if stats.regular + stats.secret > 0 => tracing::debug!(
                surface_id,
                regular = stats.regular,
                secret = stats.secret,
                "memory: purged closed-surface scope",
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!(surface_id, "memory: purge_scope failed: {e}"),
        }
    }

    // ── Workspace close — `close_case_workspace`(pane.rs)/`close_workspace_at`
    //    (workspace.rs) 공유 헬퍼. 두 함수는 "ws 안 마지막 pane 이 닫혀 workspace
    //    자체가 사라지는" 동일 로직이라 여기 모아 dedup 한다. ──

    /// 지정 workspace 의 `ClosedItem` snapshot 을 만든다(push 는 호출자 책임 —
    /// 두 호출자 모두 조건부다: `close_case_workspace` 는 `save_snapshot` 인자로,
    /// `close_workspace_at` 은 [`WorkspaceCloseOrigin`] 에서 파생한 값으로 가른다).
    fn capture_workspace_snapshot(engine: &CoreState, ws_idx: usize) -> crate::model::ClosedItem {
        let mut snap_fn = crate::core::surface_registry::snapshot_fn_for(&engine.surface_registry);
        let ws = &engine.workspaces[ws_idx];
        let terminals = &engine.terminals;
        crate::model::ClosedItem::from_workspace(ws, &mut snap_fn, &|id| terminals.get(id))
    }

    /// workspace 전체(모든 pane 의 모든 tab)의 leaf surface `(id, persist_id)` 를
    /// 제거 전에 수집한다.
    fn collect_workspace_close_targets(
        engine: &CoreState,
        ws_idx: usize,
    ) -> Vec<(u32, Option<String>)> {
        let mut targets = Vec::new();
        let ws = &engine.workspaces[ws_idx];
        for pid in ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pid) {
                for tab in &pane.tabs {
                    crate::core::impl_close::collect_close_targets(tab, engine, &mut targets);
                }
            }
        }
        targets
    }

    /// 워크스페이스가 `engine.workspaces` 에서 **제거된 직후** 반드시 도는 뒷정리 —
    /// plugin 에 나가는 `workspace.closed` 발화 + workspace scope memory purge.
    ///
    /// **제거 경로가 셋이라 초크포인트로 모았다.** 각자 쏘던 때 실제로 하나가
    /// 빠져 있었다(인라인 cascade — 마지막 터미널이 스스로 종료돼 워크스페이스가
    /// 사라지는 경로에서 `workspace.closed` 가 안 나갔다). 넷째 경로가 생겨도
    /// 여기를 지나기만 하면 같은 누락이 반복되지 않는다. 현재 호출자:
    ///
    /// - [`AppState::close_workspace_at`] — GUI 닫기 · `workspace.close` IPC
    /// - `AppState::close_case_workspace`(`state/pane.rs`) — 인라인 cascade
    /// - `core::structural_cascade::cascade_surface_closed` — Core cascade
    ///
    /// `path` 는 close 계측의 경로 구분값(`"gui"`/`"ipc"`/`"inline"`/`"cascade"`)이다.
    pub(crate) fn after_workspace_removed(&mut self, workspace_id: u32, path: &'static str) {
        self.enqueue_host_event(PendingHostEvent::WorkspaceClosed { workspace_id });
        let t = std::time::Instant::now();
        self.purge_workspace_memory_scope(workspace_id);
        crate::close_trace::log_ws_purge(t, path);
    }

    /// workspace scope 의 memory entry 정리(안의 surface 들은 각자
    /// `cleanup_surface` 가 자기 scope 를 purge). 발화와 짝지어 돌아야 하므로
    /// 직접 부르지 말고 [`AppState::after_workspace_removed`] 를 쓴다.
    fn purge_workspace_memory_scope(&mut self, workspace_id: u32) {
        let ws_scope = tasty_memory::Scope::Workspace(workspace_id);
        match self.with_memory(|m| m.purge_scope(&ws_scope)) {
            Ok(stats) if stats.regular + stats.secret > 0 => tracing::debug!(
                workspace_id,
                regular = stats.regular,
                secret = stats.secret,
                "memory: purged closed-workspace scope",
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!(workspace_id, "memory: purge_scope failed: {e}"),
        }
    }

    /// 수집된 `(surface_id, persist_id, kind)` 각각을 cleanup + lifecycle 알림
    /// enqueue. `kind` 를 어느 시점에 구하는지(remove 전/후)는 두 호출자가 서로
    /// 다르므로 — remove 후엔 `surface_kind` 가 None 을 반환할 수 있다 — 여기서
    /// 재계산하지 않고 호출자가 미리 resolve 한 값을 그대로 받는다.
    fn cleanup_targets(
        &mut self,
        engine: &mut CoreState,
        targets: Vec<(u32, Option<String>, Option<&'static str>)>,
        is_user_close: bool,
        trace: Option<&'static str>,
    ) {
        let t_loop = std::time::Instant::now();
        let mut sums = crate::close_trace::CleanupSums::default();
        for (sid, pid, kind) in targets {
            self.cleanup_surface_traced(engine, sid, pid, &mut sums);
            self.enqueue_surface_closed(sid, kind, is_user_close);
        }
        if let Some(path) = trace {
            sums.log(t_loop.elapsed(), path);
        }
    }

    /// Determine the type of the currently focused surface.
    #[cfg(feature = "gui")]
    pub fn focused_surface_type(&self, engine: &CoreState) -> FocusedSurfaceType {
        let pane = match self.focused_pane(engine) {
            Some(p) => p,
            None => return FocusedSurfaceType::None,
        };
        let tab = match pane.tabs.get(pane.active_tab) {
            Some(t) => t,
            None => return FocusedSurfaceType::None,
        };

        // Find the focused leaf surface in the layout
        if let Some(leaf) = tab.layout().find_surface(tab.focused_surface) {
            return Self::surface_to_type(leaf);
        }

        FocusedSurfaceType::None
    }

    #[cfg(feature = "gui")]
    fn surface_to_type(surface: &dyn crate::model::Surface) -> FocusedSurfaceType {
        match surface.kind() {
            "terminal" => FocusedSurfaceType::Terminal,
            other => FocusedSurfaceType::Kind(other.to_string()),
        }
    }

    // Explorer 관련 호스트 헬퍼 (focused_explorer_*, explorer_file_*, explorer_select_all
    // 등)는 ExplorerPanel과 함께 제거됨 — 동일 동작을 com.tasty.explorer plugin이
    // 자체 RemoteSurface 안에서 처리한다.

    /// Surface가 close되기 직전에 `kind` 식별자를 얻는다. plugin lifecycle 알림에
    /// payload로 채워 보낸다. None이면 lifecycle 알림을 발행하지 않는다.
    pub fn surface_kind(&self, engine: &CoreState, surface_id: u32) -> Option<&'static str> {
        engine.find_surface_by_id(surface_id).map(|s| s.kind())
    }

    /// Surface close lifecycle 알림 큐에 항목을 추가한다. App 메인 루프가
    /// `take_pending_lifecycle_events`로 drain해서 plugin manager로 dispatch한다.
    pub fn enqueue_surface_closed(
        &mut self,
        surface_id: u32,
        kind: Option<&'static str>,
        is_user_close: bool,
    ) {
        self.pending_lifecycle_events.push(PendingSurfaceClosed {
            surface_id,
            kind,
            is_user_close,
        });
    }

    /// Surface close lifecycle 큐를 비우고 항목을 반환한다.
    #[cfg(any(feature = "gui", test))]
    pub fn take_pending_lifecycle_events(&mut self) -> Vec<PendingSurfaceClosed> {
        std::mem::take(&mut self.pending_lifecycle_events)
    }

    /// Event Bus 자동 발화 큐에 항목을 추가한다.
    pub fn enqueue_host_event(&mut self, event: PendingHostEvent) {
        self.pending_host_events.push(event);
    }

    /// Event Bus 자동 발화 큐를 비우고 항목을 반환한다.
    pub fn take_pending_host_events(&mut self) -> Vec<PendingHostEvent> {
        std::mem::take(&mut self.pending_host_events)
    }

    /// Cascade 가 host event 를 enqueue 한 직후 polling baseline 을 동기화한다.
    /// 다음 `detect_tab_lifecycle` 호출이 같은 변경을 중복 enqueue 하지 않도록
    /// baseline 을 *현재 engine 상태* 와 일치시키는 역할. baseline 이 아직 `None`
    /// (한 번도 polling 안 함) 이면 no-op — 첫 detect 가 알아서 현재 상태를
    /// 베이스라인으로 잡는다.
    #[cfg(feature = "gui")]
    pub fn lifecycle_baseline_insert_tab(
        &mut self,
        tab_id: u32,
        pane_id: u32,
        workspace_id: u32,
        kind: String,
    ) {
        if let Some(map) = self.last_tab_locations.as_mut() {
            map.insert(tab_id, (pane_id, workspace_id, kind));
        }
    }

    /// `lifecycle_baseline_insert_tab` 의 close 대응. 닫힌 tab 을 baseline 에서
    /// 제거해 polling 이 중복 `TabClosed` 발화하지 않도록 한다.
    #[cfg(feature = "gui")]
    pub fn lifecycle_baseline_remove_tab(&mut self, tab_id: u32) {
        if let Some(map) = self.last_tab_locations.as_mut() {
            map.remove(&tab_id);
        }
    }

    /// Get the working directory to inherit from the focused surface, if enabled.
    ///
    /// 사용자가 현재 포커스한 surface 본인의 cwd 를 사용한다
    /// (terminal/explorer/markdown/html → 자체 cwd, image/empty/clipboard → None).
    ///
    /// **로컬 출처만** 돌려준다 — 이 값은 로컬 PTY `working_dir` 등 로컬에서 실행되는
    /// 생성 자리에 들어가므로, mirror surface 의 원격 경로는 `None` 이 된다
    /// (`docs/design/policies/cwd.md#surface-cwd-invariant` §3-2). mirror 워크스페이스 안의 구조
    /// 변경은 원격으로 forward 되고 서버가 자기 PTY 에서 cwd 를 resolve 하므로 이 `None`
    /// 으로 잃는 것이 없다.
    pub(crate) fn resolve_inherit_cwd(&self, engine: &CoreState) -> Option<std::path::PathBuf> {
        if !engine.settings.general.inherit_cwd || engine.workspaces.is_empty() {
            return None;
        }
        let sid = self.focused_surface_id(engine)?;
        engine.local_surface_cwd(sid)
    }

    /// Get the working directory to inherit from a specific surface, if enabled.
    /// [`Self::resolve_inherit_cwd`] 와 같이 **로컬 출처만** 돌려준다.
    pub(crate) fn resolve_inherit_cwd_from_surface(
        &self,
        engine: &CoreState,
        surface_id: u32,
    ) -> Option<std::path::PathBuf> {
        if !engine.settings.general.inherit_cwd {
            return None;
        }
        engine.local_surface_cwd(surface_id)
    }
}

/// [`AppState::keyboard_overlay_open`] 의 순수 술어 — 상태 접근 없이 단언할 수 있게
/// 분리했다.
///
/// `plugin_popup_open` 이 술어에 들어가는 이유: plugin egui-mesh popup 은 host
/// `PopupManager` 소속이 아니라 `has_focused()` 로 잡히지 않는데, 그 popup 의 키 입력은
/// host egui 의 `ctx.input` 을 거쳐 plugin 으로 forward 된다. 게이트가 닫혀 있으면 키가
/// egui 큐에 아예 안 들어가 forward 소스가 비고, 그 키는 그대로 터미널로 샌다.
#[cfg(any(feature = "gui", debug_assertions, test))]
pub(crate) fn keyboard_overlay_open(
    settings_open_requested: bool,
    input_dialog_open: bool,
    host_popup_focused: bool,
    plugin_popup_open: bool,
) -> bool {
    settings_open_requested || input_dialog_open || host_popup_focused || plugin_popup_open
}

#[cfg(test)]
mod keyboard_overlay_tests {
    use super::keyboard_overlay_open;

    /// plugin popup 하나만 열려 있어도 키는 egui 로 가야 한다 — 이 케이스가 빠져 있어
    /// 팝업 입력창 대신 뒤 터미널에 글자가 찍혔다.
    #[test]
    fn plugin_popup_alone_opens_the_gate() {
        assert!(keyboard_overlay_open(false, false, false, true));
    }

    #[test]
    fn nothing_open_keeps_the_gate_closed() {
        assert!(!keyboard_overlay_open(false, false, false, false));
    }

    /// 기존 세 술어의 동작은 그대로다(회귀 고정).
    #[test]
    fn each_existing_predicate_still_opens_the_gate() {
        assert!(keyboard_overlay_open(true, false, false, false));
        assert!(keyboard_overlay_open(false, true, false, false));
        assert!(keyboard_overlay_open(false, false, true, false));
    }
}

#[cfg(test)]
mod tab_bar_height_seed_tests {
    use super::*;

    /// 논리 토큰과 같은 수로 씨앗을 주면 배율 1 에서만 맞는 값이 된다. 이 필드는
    /// 실측이 채우는 자리이므로 씨앗은 "안 쟀다" 여야 한다.
    #[test]
    fn the_seed_is_not_the_logical_token() {
        let (state, _engine) = super::tests::test_state();
        let seeded = state.tab_bar_height;
        assert_eq!(
            seeded,
            PhysicalPx(0.0),
            "씨앗은 '안 쟀다' 를 뜻하는 0 이어야 한다"
        );
        assert_ne!(
            seeded.value(),
            tasty_type_appearance::theme::SIZING.tab_bar_height.value(),
            "씨앗이 논리 토큰과 같은 수다 — 배율 1 에서만 맞는 값이라 배율 2 에서 절반으로 조용히 지나간다"
        );
    }
}
