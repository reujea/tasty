pub(crate) mod approval;
mod arrange;
pub(crate) mod body_keys;
pub(crate) mod command_palette;
pub(crate) mod confirm_close_remote;
pub(crate) mod confirm_delete_category;
pub(crate) mod confirm_force_detach_workspace;
pub(crate) mod convert;
pub(crate) mod dag_list;
#[cfg(all(debug_assertions, feature = "gui"))]
mod debug_view;
pub(crate) mod defs;
pub(crate) mod display;
pub(crate) mod download_run;
mod draw;
pub(crate) mod explorer_conflict;
pub(crate) mod explorer_properties;
pub(crate) mod file_handler_picker;
pub(crate) mod file_picker;
pub(crate) mod focus;
#[cfg(test)]
#[path = "popup/focus_owner_tests.rs"]
mod focus_owner_tests;
pub(crate) mod frame;
pub(crate) mod frame_keys;
pub(crate) mod header_drag;
pub(crate) mod keys;
pub(crate) mod occlusion;
pub(crate) mod open_when;
mod payload;
pub(crate) mod port_scanner;
pub(crate) mod port_scanner_close;
pub(crate) mod preset_apply;
pub(crate) mod press;
pub(crate) mod rail_category;
pub(crate) mod remote_attach;
pub(crate) mod remote_row;
pub(crate) mod remote_tool;
pub(crate) mod route;
/// 프레임을 그려 scrim의 실제 영역을 검사한다.
#[cfg(test)]
#[path = "popup/scrim_scope_tests.rs"]
mod scrim_scope_tests;
pub(crate) mod script_confirm;
pub(crate) mod transfer;
mod view_local;
#[cfg(all(test, feature = "gui"))]
pub(crate) mod window_input_test_support;
#[cfg(all(test, feature = "gui"))]
#[path = "popup/window_key_gate_tests.rs"]
mod window_key_gate_tests;
pub use arrange::NoticeColumn;
pub use view_local::drawn_by_view;

use crate::state::MainViewState;
use tasty_type_geometry::length::LogicalPx;

// 새 팝업은 popup::defs의 all_defs()에 등록한다.

pub use crate::model::popup_kind::{PopupId, PopupScope};
pub use payload::{PayloadSlot, PopupPayload};
pub use tasty_plugin_protocol::PopupCloseReason;
pub use tasty_plugin_protocol::popup_decl::{
    PopupAnchor, PopupChrome, PopupContentMargin, PopupDecl, PopupDragHandle, PopupEscape,
    PopupFit, PopupInstances, PopupKeyboard, PopupOutsideClick, PopupResizeHandle, PopupScopeKind,
    PopupShadow, PopupShell, PopupSize,
};

/// host 범위 값의 종류. 대상 id는 버린다.
// 이유: 지금은 debug 팝업 목록과 시험만 읽는다. 열기 요청이 범위 종류를 검사하면 release에서도 쓴다.
#[cfg_attr(
    not(any(test, debug_assertions)),
    expect(
        dead_code,
        reason = "only the debug popup list reads scope kinds for now"
    )
)]
pub(crate) fn scope_kind(scope: &PopupScope) -> PopupScopeKind {
    match scope {
        PopupScope::Window => PopupScopeKind::Window,
        PopupScope::Workspace(_) => PopupScopeKind::Workspace,
        PopupScope::Pane(_) => PopupScopeKind::Pane,
        PopupScope::Tab(_) => PopupScopeKind::Tab,
        PopupScope::Surface(_) => PopupScopeKind::Surface,
    }
}

/// `egui::Vec2`를 선언 크기로 옮긴다.
pub const fn popup_size(v: egui::Vec2) -> PopupSize {
    PopupSize {
        width: LogicalPx(v.x),
        height: LogicalPx(v.y),
    }
}

fn vec2_of(size: PopupSize) -> egui::Vec2 {
    egui::vec2(size.width.value(), size.height.value())
}

/// 정의 없이 만든 팝업의 선언 값. 덮지 않는 보통 팝업이다. 크기는 생성자가 따로 받는다.
const PLAIN_DECL: PopupDecl = PopupDecl {
    covers_scope: false,
    ..defs::host_decl(PopupSize {
        width: LogicalPx(0.0),
        height: LogicalPx(0.0),
    })
};

/// 셸을 그릴 때 읽는 선언 값.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PopupLook {
    pub(crate) covers_scope: bool,
    pub(crate) shell: PopupShell,
    pub(crate) shadow: PopupShadow,
    pub(crate) content_margin: PopupContentMargin,
}

impl PopupLook {
    /// 열 때 정한 덮음 값. 덮으면 modal 그림자, 덮지 않으면 popover 그림자다. 둘은 늘 함께 바뀐다.
    fn covering(self, covers_scope: bool) -> Self {
        Self {
            covers_scope,
            shadow: if covers_scope {
                PopupShadow::Modal
            } else {
                PopupShadow::Popover
            },
            ..self
        }
    }

    fn of(decl: &PopupDecl) -> Self {
        Self {
            covers_scope: decl.covers_scope,
            shell: decl.shell,
            shadow: decl.shadow,
            content_margin: decl.content_margin,
        }
    }
}

/// 열린 팝업 인스턴스의 식별자. 같은 선언으로 연 인스턴스끼리도 다르다.
///
/// host·plugin 팝업 공용 순번(`next_popup_z_seq`)에서 받는다. 닫았다 다시 열면 새 값이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PopupInstanceId(pub u64);

impl PopupInstanceId {
    /// 아직 열리지 않은 등록 원본의 값.
    pub const NONE: Self = Self(0);

    fn next() -> Self {
        Self(tasty_host_plugin::next_popup_z_seq())
    }
}

/// 그리기 함수가 받는 인스턴스 문맥.
pub struct PopupCx<'a> {
    pub id: PopupId,
    pub instance: PopupInstanceId,
    /// 이 인스턴스가 이번 프레임을 시작할 때 키보드 포커스를 가졌는지.
    pub focused: bool,
    /// 인스턴스 데이터.
    pub payload: &'a mut PopupPayload,
}

#[cfg(test)]
impl ClosedPopup {
    /// 시험용: 닫기 훅을 직접 부를 때의 값. 사유는 팝업 밖 요청이다.
    pub fn test(id: PopupId) -> Self {
        Self {
            id,
            instance: PopupInstanceId::NONE,
            reason: PopupCloseReason::Command,
            scope: PopupScope::Window,
            payload: PopupPayload::None,
        }
    }
}

/// 닫힌 인스턴스. 닫기 훅이 사유와 데이터를 함께 받는다. 데이터는 훅이 끝나면 버려진다.
#[derive(Debug)]
pub struct ClosedPopup {
    pub id: PopupId,
    pub instance: PopupInstanceId,
    pub reason: PopupCloseReason,
    /// 닫히기 직전의 범위.
    pub scope: PopupScope,
    pub payload: PopupPayload,
}

/// Result of a popup's draw call.
pub enum PopupAction {
    /// No action needed.
    None,
    /// The popup requests to be closed.
    Close,
    /// 사유를 붙여 닫는다. 콘텐츠가 Esc를 직접 처리하는 팝업이 쓴다.
    CloseBy(PopupCloseReason),
    /// Keep the popup open and return keyboard input to its surface.
    ReleaseFocus,
}

/// 팝업 이동 영역. 콘텐츠 위젯이 포인터를 사용 중이면 이동·리사이즈를 시작하지 않으므로
/// 검색창이나 버튼을 포함한 헤더 전체를 이동 영역으로 지정할 수 있다.
#[derive(Clone, Copy)]
pub enum DragHandle {
    /// 이동 불가.
    None,
    /// 타이틀바를 이동 영역으로 사용한다. headless에는 타이틀바가 없어 이동할 수 없다.
    TitleBar,
    /// 팝업이 pos/size 로부터 전용 핸들 띠를 계산. headless 팝업도 이동 가능.
    Region(fn(&PopupState) -> egui::Rect),
}

impl std::fmt::Debug for DragHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DragHandle::None => write!(f, "DragHandle::None"),
            DragHandle::TitleBar => write!(f, "DragHandle::TitleBar"),
            DragHandle::Region(_) => write!(f, "DragHandle::Region(..)"),
        }
    }
}

/// 리사이즈 중 사용자가 잡은 테두리 엣지 조합. 모서리는 인접한 두 엣지가 함께 true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResizeEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// Result of PopupManager::draw(), including input layer hit information.
pub struct PopupDrawResult {
    /// Popup IDs that were closed this frame.
    pub closed: Vec<PopupId>,
    /// 포인터가 그린 팝업 노드(상자, 덮는 범위) 위인지. 창의 포인터 판정은 이 값이 아니라 배치한 사각형을 쓴다
    /// (`MainViewState::popup_pointer_target`). 시험이 이 프레임의 판정을 확인할 때 읽는다.
    #[cfg(test)]
    pub hovered: bool,
    /// 그린 팝업 가운데 포커스를 받는 팝업의 레이어. egui_bridge가 plugin 팝업과의 순서를 정할 때 쓴다.
    /// 포커스를 받지 않는 팝업은 관리자가 이 레이어들 아래에 둔다.
    pub layers: Vec<egui::LayerId>,
    /// 전체화면 버튼으로 요청한 무대 ID. 실제 진입은 MainViewState를 가진 호출부에서 처리한다.
    pub fullscreen_requested: Option<crate::adapters::ui::fullscreen::StageId>,
    /// 실제 그린 팝업의 영역과 z_seq. plugin 팝업이 위에 덮인 host 팝업을 판별할 때 쓴다.
    /// 숨겨진 팝업은 마우스 입력을 가로채지 않도록 제외한다.
    pub hit_rects: Vec<occlusion::Occluder>,
    /// 보이는 덮는 팝업의 덮는 사각형과 z_seq. 포인터 대상 찾기가 그 아래로 내려가지 않는다.
    pub covers: Vec<occlusion::Occluder>,
    /// native WebView 위에 합성할 대상(non-passive 팝업). passive 팝업은 들어 있지 않다.
    pub overlay_targets: Vec<crate::state::webview_overlay::OverlayTarget>,
    /// 안내 열에 놓은 인스턴스가 차지한 사각형의 합. 열이 비었거나 자리가 없으면 `None`이다.
    pub column_rect: Option<egui::Rect>,
    /// 사용자가 옮기거나 크기를 바꾼 뒤 이 프레임에 놓은 인스턴스와 그 사각형. 놓을 때 한 번만 들어 있다.
    pub released: Vec<(PopupInstanceId, egui::Rect)>,
}

/// 팝업 정의. 공통 선언 `decl`과 host만 쓰는 함수 값을 담는다.
pub struct PopupDef {
    pub id: PopupId,
    /// 팝업 사이 동작 차이를 정하는 공통 선언.
    pub decl: PopupDecl,
    /// i18n 키. `t()`로 런타임 번역하여 popup title로 사용.
    pub title_key: &'static str,
    /// 동적 타이틀. 매 프레임 호출. `title_key` 대신 사용된다. (예: rename popup의
    /// 대상별 제목)
    pub title_fn: Option<PopupTitleFn>,
    /// 매 프레임 크기를 계산한다. 사용자가 크기를 지정한 팝업에는 적용하지 않는다.
    pub sizer: Option<PopupSizerFn>,
    /// `decl.drag_handle`이 `Region`일 때 끌어 옮길 영역을 계산한다. 다른 값이면 `None`이다.
    pub drag_region: Option<fn(&PopupState) -> egui::Rect>,
    /// 렌더링 함수. 매 프레임 호출. MainViewState에서 필요한 데이터를 꺼낸다.
    pub draw_fn: fn(
        &mut egui::Ui,
        &mut MainViewState,
        &crate::runtime::engine_read::EngineRead<'_>,
        &mut PopupCx<'_>,
    ) -> PopupAction,
    /// 전체화면 버튼으로 열 무대 ID. None이거나 headless면 버튼이 없다.
    /// 무대는 별도 콘텐츠이며 원본 팝업은 열린 채 그 아래 남는다.
    pub fullscreen_stage: Option<crate::adapters::ui::fullscreen::StageId>,
    /// 사용자·에이전트 요청으로 인스턴스를 열기 직전에 부른다. 열려 있는 인스턴스를 다시 쓰는 요청에서는 부르지 않는다.
    pub on_open: Option<fn(&mut MainViewState)>,
    /// 열린 팝업이 close()를 통해 닫히면 closed_queue에 기록하고 frame에서 훅을 호출한다.
    /// egui 임시 상태를 지울 수 있도록 Context를 받는다.
    pub on_close: Option<PopupCloseHook>,
    /// debug IPC가 사용자 클릭 없이 열 때 실을 인스턴스 데이터. 포커스된 surface를 받는다.
    /// `None`이거나 `PopupPayload::None`을 돌려주면 데이터 없이 연다.
    #[cfg(debug_assertions)]
    pub debug_payload: Option<PopupDebugPayloadFn>,
}

/// debug IPC 열기의 인스턴스 데이터. 세 번째 인자는 surface 범위로 열 때의 대상 surface다.
#[cfg(debug_assertions)]
pub type PopupDebugPayloadFn =
    fn(&MainViewState, &crate::core::CoreState, Option<u32>) -> PopupPayload;

/// 닫기 훅. 닫힌 인스턴스의 사유와 데이터를 받는다.
/// 인스턴스마다 제목을 정한다. 인스턴스 데이터를 함께 받는다.
pub type PopupTitleFn =
    fn(&MainViewState, &crate::runtime::engine_read::EngineRead<'_>, &PopupPayload) -> String;

/// 인스턴스마다 크기를 정한다. 인스턴스 데이터를 함께 받는다.
pub type PopupSizerFn =
    fn(&MainViewState, &crate::runtime::engine_read::EngineRead<'_>, &PopupPayload) -> egui::Vec2;

pub type PopupCloseHook = fn(
    &egui::Context,
    &mut MainViewState,
    &crate::runtime::engine_read::EngineRead<'_>,
    &ClosedPopup,
);

/// State for a single popup instance.
#[derive(Debug)]
pub struct PopupState {
    /// 선언 id. 같은 선언의 인스턴스끼리 같다.
    pub id: PopupId,
    /// 열린 인스턴스의 식별자. 닫힌 등록 원본은 `PopupInstanceId::NONE`이다.
    pub instance: PopupInstanceId,
    /// 같은 선언으로 인스턴스를 몇 개 여는지.
    instances: PopupInstances,
    /// 이 인스턴스를 연 부모 팝업. 부모가 닫히면 함께 닫힌다.
    pub parent: Option<PopupInstanceId>,
    /// 이 인스턴스를 연 plugin 팝업(instance id). host 부모처럼 그 표시 상태(펼침·접힘)를 따른다.
    pub plugin_parent: Option<u64>,
    /// 인스턴스 데이터. 닫으면 비운다.
    pub payload: PopupPayload,
    /// Title text displayed in the title bar.
    pub title: String,
    /// Whether the popup is open (a hidden scope does not close it).
    pub open: bool,
    /// Scope visibility from the latest draw; focus intent survives while hidden.
    scope_visible: bool,
    /// Position in logical pixels (top-left corner).
    pub pos: egui::Pos2,
    /// 그린 크기(logical px). 매 프레임 `needed_size`에서 범위 경계로 줄여 다시 정한다.
    pub size: egui::Vec2,
    /// 필요 크기(logical px). sizer 결과, 사용자가 바꾼 크기, 없으면 선언 크기 × ui_zoom.
    /// 범위가 좁아 줄여 그려도 이 값은 그대로라 범위가 넓어지면 원래 크기로 돌아온다.
    needed_size: egui::Vec2,
    /// Whether the popup is currently being dragged.
    dragging: bool,
    /// Drag offset from popup top-left to mouse position.
    drag_offset: egui::Vec2,
    /// Scope determines visibility and boundary clamping.
    pub scope: PopupScope,
    /// 사용자 조작 없이 포커스 없이 열려 아직 사용자가 누르지 않은 상태.
    /// native WebView를 모두 숨기거나 그 키보드 포커스를 회수하지 않고, 겹치는 WebView만
    /// 키보드 입력이 없을 때 숨긴다. 포커스를 받거나 닫히면 해제한다.
    passive: bool,
    /// If true, clicking outside this popup will close it (not just unfocus).
    pub close_on_outside_click: bool,
    /// true면 타이틀바·닫기 버튼 없이 콘텐츠만 렌더링한다.
    pub headless: bool,
    /// true면 팝업 바깥 클릭해도 키보드 포커스가 유지된다.
    pub sticky_focus: bool,
    /// If true, PopupManager will center this popup on the next draw and clear the flag.
    pub request_center: bool,
    /// If true, PopupManager will position this popup at the top of its scope on the
    /// next draw (horizontally centered, small margin from top) and clear the flag.
    pub request_top: bool,
    /// 이동(드래그) 핸들 선언. `register_def`가 `PopupDef`에서 전파한다.
    drag_handle: DragHandle,
    /// 테두리 드래그 리사이즈 허용 여부.
    resizable: bool,
    /// 리사이즈 최소 크기. `default_size`로 초기화된 뒤 `with_min_size`로 덮인다.
    min_size: egui::Vec2,
    /// 리사이즈 진행 중이면 잡은 엣지 조합. `None`이면 리사이즈 중 아님.
    resizing: Option<ResizeEdges>,
    /// host와 plugin 팝업이 공유하는 전역 순번. 열거나 맨 앞으로 올릴 때 갱신한다.
    /// host 팝업끼리는 Vec 순서로 비교하고, 서로 다른 매니저 사이는 이 값으로 비교한다.
    pub(crate) z_seq: u64,
    /// 리사이즈 시작 시점의 팝업 rect (드래그 누적 계산 기준).
    resize_start_rect: egui::Rect,
    /// 사용자가 한 번이라도 리사이즈했으면 true. true 동안 sizer 의 size 덮어쓰기를
    /// 막는다(`popup::frame::draw_popup_layer`). `close()`에서 리셋되어 다음 open 시 sizer 복원.
    pub size_user_overridden: bool,
    /// 타이틀바 전체화면 버튼이 올릴 무대 id. `register_def` 가 `PopupDef` 에서
    /// 전파한다. `None` 이면 버튼 없음.
    fullscreen_stage: Option<crate::adapters::ui::fullscreen::StageId>,
    /// 셸을 그릴 때 읽는 값. 열기 요청이 덮음을 정하면 그 인스턴스만 선언 값과 다르다.
    look: PopupLook,
    /// 선언에서 온 셸 값. 닫으면 `look`을 이 값으로 되돌린다.
    declared_look: PopupLook,
    /// 선언에서 온 배치·포커스 값.
    arrange: arrange::Arrange,
    /// 선언의 Esc 동작. 선언 없이 만든 팝업은 `PopupDecl::new`의 기본(`Close`)이다.
    escape: PopupEscape,
    /// 선언의 키 소비 집합. 선언 없이 만든 팝업은 `PopupDecl::new`의 기본(`Keys`)이다. 선언으로 만든
    /// 인스턴스와 그 사본에서 `arrange.takes_focus == (keyboard != None)`이다.
    keyboard: PopupKeyboard,
    /// 좁은 범위의 표시 상태(그대로·접힘·창 위). 매 프레임 `judge_display`가 정한다.
    display: display::PopupDisplay,
}

/// UI 배율을 적용한 Theme.item_height_interactive를 반올림한 타이틀바 높이.
pub fn title_bar_height() -> LogicalPx {
    use egui::emath::GuiRounding as _;
    LogicalPx(
        crate::theme::theme()
            .item_height_interactive
            .value()
            .round_ui(),
    )
}

/// Theme.spacing_xs를 반올림한 팝업 내부 여백. 앵커 메뉴 팝업의 안쪽 둘레 `popup-content-margin`(→ space-xs)과 같은 값이다.
pub fn content_margin() -> LogicalPx {
    use egui::emath::GuiRounding as _;
    LogicalPx(crate::theme::theme().spacing_xs.value().round_ui())
}

/// 타이틀바 우측 버튼 사이 간격 — `popup-title-btn-gap` 의 round_ui.
pub fn title_btn_gap() -> f32 {
    use egui::emath::GuiRounding as _;
    crate::theme::theme()
        .popup_title_btn_gap()
        .value()
        .round_ui()
}

/// 헤더 드래그 rect 를 담는 egui temp memory Id (popup id 로 네임스페이스).
fn header_drag_rect_id(popup_id: PopupId) -> egui::Id {
    egui::Id::new("popup.header_drag_rect").with(popup_id)
}

/// 뷰가 그린 헤더 영역을 팝업 ID별로 보고한다. 크기·배율에 따라 달라지는 영역을
/// hit-test에서 이동 손잡이로 사용하므로 매 프레임 보고해야 한다.
pub fn report_header_drag_rect(ctx: &egui::Context, popup_id: PopupId, rect: egui::Rect) {
    ctx.memory_mut(|m| m.data.insert_temp(header_drag_rect_id(popup_id), rect));
}

/// 뷰가 보고한 헤더 드래그 rect 를 읽는다(hit-test 용). 아직 보고 전이면 None.
pub(crate) fn reported_header_drag_rect(
    ctx: &egui::Context,
    popup_id: PopupId,
) -> Option<egui::Rect> {
    ctx.memory(|m| m.data.get_temp(header_drag_rect_id(popup_id)))
}

/// 팝업 안의 드롭다운 영역을 저장할 공용 슬롯. overlay_key로 각각 구분한다.
fn child_overlay_registry_id() -> egui::Id {
    egui::Id::new("popup.child_overlay_registry")
}

type ChildOverlayMap = std::collections::HashMap<&'static str, (PopupId, egui::Rect)>;

/// 드롭다운이 부모 밖으로 나와도 바깥 클릭으로 처리하지 않도록 실제 영역을 보고한다.
/// 같은 팝업의 여러 드롭다운은 고유 overlay_key로 구분한다. 닫히면 None을 보고해 지운다.
pub fn report_child_overlay_rect(
    ctx: &egui::Context,
    popup_id: PopupId,
    overlay_key: &'static str,
    rect: Option<egui::Rect>,
) {
    let id = child_overlay_registry_id();
    ctx.memory_mut(|m| {
        let mut map: ChildOverlayMap = m.data.get_temp(id).unwrap_or_default();
        match rect {
            Some(r) => {
                map.insert(overlay_key, (popup_id, r));
            }
            None => {
                map.remove(overlay_key);
            }
        }
        m.data.insert_temp(id, map);
    });
}

/// egui popup `dropdown_id` 의 열림 상태와 영역을 [`report_child_overlay_rect`] 로 보고한다.
pub fn report_dropdown_overlay(
    ui: &egui::Ui,
    popup_id: PopupId,
    overlay_key: &'static str,
    dropdown_id: egui::Id,
) {
    let rect = ui
        .memory(|m| m.is_popup_open(dropdown_id))
        .then(|| ui.memory(|m| m.area_rect(dropdown_id)))
        .flatten();
    report_child_overlay_rect(ui.ctx(), popup_id, overlay_key, rect);
}

/// 자식 드롭다운이 열려 있는지 확인한다. 부모의 Escape 처리가 본문보다 먼저 실행되므로
/// 이 값이 true면 부모를 닫지 않고 드롭다운에 Escape 처리를 맡긴다.
pub fn child_overlay_open(
    ctx: &egui::Context,
    popup_id: PopupId,
    overlay_key: &'static str,
) -> bool {
    let id = child_overlay_registry_id();
    ctx.memory(|m| {
        let map: ChildOverlayMap = m.data.get_temp(id).unwrap_or_default();
        map.get(overlay_key)
            .is_some_and(|(pid, _)| *pid == popup_id)
    })
}

/// `popup_id` 소유의 열린 자식 오버레이 사각형.
fn child_overlay_rects(ctx: &egui::Context, popup_id: PopupId) -> Vec<egui::Rect> {
    let id = child_overlay_registry_id();
    ctx.memory(|m| {
        let map: ChildOverlayMap = m.data.get_temp(id).unwrap_or_default();
        map.values()
            .filter(|(pid, _)| *pid == popup_id)
            .map(|(_, rect)| *rect)
            .collect()
    })
}

/// `popup_id` 소유의 자식 오버레이 중 `pos` 를 포함하는 것이 있는지 hit-test.
fn child_overlay_hit(ctx: &egui::Context, popup_id: PopupId, pos: egui::Pos2) -> bool {
    let id = child_overlay_registry_id();
    ctx.memory(|m| {
        let map: ChildOverlayMap = m.data.get_temp(id).unwrap_or_default();
        map.values()
            .any(|(pid, rect)| *pid == popup_id && rect.contains(pos))
    })
}

impl PopupState {
    /// 배치 범위. 배치 사각형·scrim·포인터 차단·바깥 누름 판정이 이 범위를 쓴다. 키 경로·자식 상속·보임
    /// 여부는 논리 범위 `scope`를 쓴다.
    pub(crate) fn placement_scope(&self) -> PopupScope {
        if self.expanded() {
            PopupScope::Window
        } else {
            self.scope.clone()
        }
    }

    /// 셸을 그릴 때 읽는 선언 값.
    #[cfg(test)]
    pub(crate) fn look(&self) -> PopupLook {
        self.look
    }

    /// 범위는 만들 때 받는다. 기본값을 두지 않는다. 열 때마다 열기 함수가 다시 정한다.
    pub fn new(
        id: PopupId,
        title: impl Into<String>,
        default_size: egui::Vec2,
        scope: PopupScope,
    ) -> Self {
        Self {
            id,
            instance: PopupInstanceId::NONE,
            instances: PopupInstances::Single,
            parent: None,
            plugin_parent: None,
            payload: PopupPayload::None,
            title: title.into(),
            open: false,
            scope_visible: true,
            pos: egui::pos2(100.0, 100.0),
            size: default_size,
            needed_size: default_size,
            dragging: false,
            drag_offset: egui::Vec2::ZERO,
            scope,
            passive: false,
            close_on_outside_click: false,
            headless: false,
            sticky_focus: false,
            request_center: false,
            request_top: false,
            // 직접 생성한 타이틀바 팝업도 이동할 수 있게 한다.
            drag_handle: DragHandle::TitleBar,
            resizable: false,
            min_size: default_size,
            resizing: None,
            z_seq: 0,
            resize_start_rect: egui::Rect::ZERO,
            size_user_overridden: false,
            fullscreen_stage: None,
            look: PopupLook::of(&PLAIN_DECL),
            declared_look: PopupLook::of(&PLAIN_DECL),
            arrange: arrange::Arrange::PLAIN,
            escape: PopupEscape::Close,
            keyboard: PopupKeyboard::Keys,
            display: display::PopupDisplay::Fits,
        }
    }

    /// 선언으로 팝업을 만든다. 크기와 최소 크기는 선언 값 그대로다.
    /// `drag_region`은 `decl.drag_handle`이 `Region`일 때 쓴다.
    pub fn declared(
        id: PopupId,
        title: impl Into<String>,
        decl: &PopupDecl,
        drag_region: Option<fn(&PopupState) -> egui::Rect>,
        scope: PopupScope,
    ) -> Self {
        let size = vec2_of(decl.size);
        let drag_handle = match (decl.drag_handle, drag_region) {
            (PopupDragHandle::None, _) => DragHandle::None,
            (PopupDragHandle::TitleBar, _) => DragHandle::TitleBar,
            (PopupDragHandle::Region, Some(f)) => DragHandle::Region(f),
            (PopupDragHandle::Region, None) => {
                tracing::error!("popup {id} declares a drag region without a region function");
                DragHandle::None
            }
        };
        let mut popup = Self::new(id, title, size, scope)
            .with_close_on_outside_click(decl.closes_on_outside_click())
            .with_headless(decl.is_headless())
            .with_sticky_focus(decl.sticky_focus)
            .with_drag_handle(drag_handle)
            .with_resizable(decl.resizable)
            .with_min_size(decl.min_size.map_or(size, vec2_of));
        popup.look = PopupLook::of(decl);
        popup.declared_look = popup.look;
        popup.arrange = arrange::Arrange::of(decl);
        popup.escape = decl.escape;
        popup.keyboard = decl.keyboard;
        popup.instances = decl.instances;
        popup
    }

    /// 같은 선언으로 인스턴스를 몇 개 여는지 정한다.
    #[cfg(test)]
    pub fn with_instances(mut self, v: PopupInstances) -> Self {
        self.instances = v;
        self
    }

    /// Set whether clicking outside this popup should close it.
    pub fn with_close_on_outside_click(mut self, v: bool) -> Self {
        self.close_on_outside_click = v;
        self
    }

    /// Set headless mode (no title bar / close button).
    pub fn with_headless(mut self, v: bool) -> Self {
        self.headless = v;
        self
    }

    /// Set sticky focus (keyboard focus persists even when clicking outside).
    pub fn with_sticky_focus(mut self, v: bool) -> Self {
        self.sticky_focus = v;
        self
    }

    /// 드래그(이동) 핸들 선언을 설정한다.
    pub fn with_drag_handle(mut self, h: DragHandle) -> Self {
        self.drag_handle = h;
        self
    }

    /// 테두리 드래그 리사이즈 허용 여부를 설정한다.
    pub fn with_resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }

    /// 리사이즈 최소 크기를 설정한다.
    pub fn with_min_size(mut self, sz: egui::Vec2) -> Self {
        self.min_size = sz;
        self
    }

    /// 타이틀바 전체화면 버튼이 올릴 무대를 설정한다. `None` 이면 버튼 없음.
    pub fn with_fullscreen_stage(
        mut self,
        stage: Option<crate::adapters::ui::fullscreen::StageId>,
    ) -> Self {
        self.fullscreen_stage = stage;
        self
    }

    /// 같은 선언의 새 인스턴스를 만들 바탕. 위치·크기·선언 값은 복사하고 인스턴스 데이터는 비운다.
    /// 인스턴스 데이터는 진행 중인 읽기처럼 복제할 수 없는 값을 담을 수 있다.
    fn fresh_copy(&self) -> Self {
        Self {
            id: self.id,
            instance: self.instance,
            instances: self.instances,
            parent: self.parent,
            plugin_parent: self.plugin_parent,
            title: self.title.clone(),
            open: self.open,
            scope_visible: self.scope_visible,
            pos: self.pos,
            size: self.size,
            needed_size: self.needed_size,
            dragging: self.dragging,
            drag_offset: self.drag_offset,
            scope: self.scope.clone(),
            passive: self.passive,
            close_on_outside_click: self.close_on_outside_click,
            headless: self.headless,
            sticky_focus: self.sticky_focus,
            request_center: self.request_center,
            request_top: self.request_top,
            drag_handle: self.drag_handle,
            resizable: self.resizable,
            min_size: self.min_size,
            resizing: self.resizing,
            z_seq: self.z_seq,
            resize_start_rect: self.resize_start_rect,
            size_user_overridden: self.size_user_overridden,
            fullscreen_stage: self.fullscreen_stage,
            look: self.look,
            declared_look: self.declared_look,
            arrange: self.arrange,
            escape: self.escape,
            keyboard: self.keyboard,
            display: self.display,
            payload: PopupPayload::None,
        }
    }

    /// 닫힌 상태로 되돌린다. 위치·크기와 선언 값은 남긴다.
    fn reset_closed(&mut self) {
        self.open = false;
        self.dragging = false;
        self.passive = false;
        // 리사이즈 상태 리셋 → 다음 open 시 sizer 가 크기를 다시 결정하도록 복원.
        self.resizing = None;
        self.size_user_overridden = false;
        self.instance = PopupInstanceId::NONE;
        self.parent = None;
        self.plugin_parent = None;
        self.payload = PopupPayload::None;
        self.look = self.declared_look;
        self.display = display::PopupDisplay::Fits;
    }

    /// 팝업 사각형(논리 좌표). 직전 프레임에 그린 자리다.
    pub(crate) fn popup_rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.pos, self.size)
    }

    fn title_rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(
            self.pos,
            egui::vec2(self.size.x, title_bar_height().value()),
        )
    }

    /// 현재 이동(드래그) 핸들 영역. `None`이면 이동 불가.
    /// - `TitleBar`: 타이틀바(`title_rect`). 단 headless 면 타이틀바가 없으므로 None.
    /// - `Region(f)`: 팝업이 pos/size 로부터 계산한 전용 핸들 띠.
    fn drag_handle_rect(&self) -> Option<egui::Rect> {
        match self.drag_handle {
            DragHandle::None => None,
            DragHandle::TitleBar => {
                if self.headless {
                    None
                } else {
                    Some(self.title_rect())
                }
            }
            DragHandle::Region(f) => Some(f(self)),
        }
    }

    /// 뷰가 보고한 실제 헤더 영역을 우선 사용한다. 보고는 콘텐츠를 그린 뒤이므로
    /// 직전 프레임 값을 읽으며, 첫 프레임에는 drag_handle_rect로 계산한다.
    fn effective_drag_handle_rect(&self, ctx: &egui::Context) -> Option<egui::Rect> {
        header_drag::reported_for(ctx, self.instance, self.id).or_else(|| self.drag_handle_rect())
    }

    fn content_rect(&self) -> egui::Rect {
        let popup = self.popup_rect();
        // 구역별로 여백을 주는 팝업은 공통 내부 여백을 적용하지 않는다.
        let margin = match self.look.content_margin {
            PopupContentMargin::None => LogicalPx(0.0),
            PopupContentMargin::Default => content_margin(),
        };
        let top_offset = if self.has_title_bar() {
            title_bar_height() + margin
        } else {
            margin
        };
        let min = egui::pos2(
            popup.min.x + margin.value(),
            popup.min.y + top_offset.value(),
        );
        // 범위가 제목줄과 여백보다 좁으면 콘텐츠 자리는 비어 있다. 뒤집힌 사각형을 만들지 않는다.
        let max = egui::pos2(popup.max.x - margin.value(), popup.max.y - margin.value()).max(min);
        egui::Rect::from_min_max(min, max)
    }

    pub(crate) fn close_btn_rect(&self) -> egui::Rect {
        display::title_end_btn_rect(self.title_rect())
    }

    /// 타이틀바가 있고 무대를 지정했을 때 전체화면 버튼 영역을 반환한다.
    /// 닫기 버튼 왼쪽에 배치하며 닫기 버튼 위치는 바꾸지 않는다.
    pub(crate) fn fullscreen_btn_rect(&self) -> Option<egui::Rect> {
        if self.headless || self.fullscreen_stage.is_none() {
            return None;
        }
        let close = self.close_btn_rect();
        Some(egui::Rect::from_center_size(
            egui::pos2(
                close.center().x - close.width() - title_btn_gap(),
                close.center().y,
            ),
            close.size(),
        ))
    }

    /// 제목을 줄일 기준인 오른쪽 버튼 영역의 왼쪽 경계.
    fn title_buttons_left_x(&self) -> f32 {
        self.collapse_btn_rect()
            .or_else(|| self.fullscreen_btn_rect())
            .unwrap_or_else(|| self.close_btn_rect())
            .min
            .x
    }

    /// 제목을 그리고 말줄임할 띠. 버튼 수와 관계없이 양쪽에 같은 예약을 비운다
    /// (`popup-title-text-gap` 포함, 버튼 하나 32 · 둘 60).
    fn title_text_rect(&self) -> egui::Rect {
        tasty_ui_widgets::popup_title_text_rect(
            self.title_rect(),
            self.title_buttons_left_x(),
            crate::theme::theme().popup_title_text_gap(),
        )
    }

    /// 필요 크기를 정한다. 그린 크기도 같은 값에서 다시 시작한다.
    pub fn set_needed_size(&mut self, size: egui::Vec2) {
        self.needed_size = size;
        self.size = size;
    }

    /// 필요 크기. 범위에 맞춰 줄인 그린 크기(`size`)와 다르다.
    #[cfg(test)]
    pub(crate) fn needed_size(&self) -> egui::Vec2 {
        self.needed_size
    }

    /// 그린 크기를 필요 크기에서 `screen` 안으로 줄이고 위치를 그 안에 둔다. 필요 크기는 바꾸지 않는다.
    fn clamp_to_screen(&mut self, screen: egui::Rect) {
        let size = self.needed_size + self.chrome_extra();
        // 펼친 팝업은 프레임을 창에 맞추고 내용은 필요 크기를 유지한다(`expanded_overflow`).
        if self.expanded() {
            let placed = tasty_ui_widgets::place_expanded(screen, self.pos, size);
            self.pos = placed.frame.min;
            self.size = placed.frame.size();
            return;
        }
        self.size = size.min(screen.size());
        self.pos.x = self
            .pos
            .x
            .clamp(screen.min.x, (screen.max.x - self.size.x).max(screen.min.x));
        self.pos.y = self
            .pos
            .y
            .clamp(screen.min.y, (screen.max.y - self.size.y).max(screen.min.y));
    }
}

/// 창 하나의 host 팝업 관리자. 인스턴스의 z 순서, 이동, 범위 안 제한을 맡는다.
///
/// 목록에는 열린 인스턴스와, 선언마다 많아야 하나인 닫힌 등록 원본이 함께 있다. 열린 인스턴스가
/// 없는 선언은 원본이 남아 다음 인스턴스의 처음 위치·크기가 된다. 같은 선언에 다른 항목이 있을 때
/// 닫힌 인스턴스는 목록에서 빠진다.
pub struct PopupManager {
    /// z 순서(마지막이 맨 위).
    popups: Vec<PopupState>,
    /// 실제로 닫힌(그 직전엔 열려 있던) 인스턴스 대기열.
    /// `on_close` 훅 drain 이 프레임당 1회 `take_closed_queue()` 로 비운다.
    closed_queue: Vec<ClosedPopup>,
    /// 상태 술어로 연 팝업과 그때의 범위. 술어 값이 바뀔 때만 열고 닫는다.
    driven: Vec<(PopupId, open_when::DrivenOpen)>,
    /// 안내 열의 자리. 호출자가 프레임마다 정한다.
    notice_column: Option<NoticeColumn>,
    /// plugin 팝업의 펼침과 표시 상태(`display.rs`).
    plugin_parents: display::PluginParents,
    /// 호출자가 그리기 전에 정한 이번 프레임의 그리기 순서(`prepare_frame_for_draw`). 그리기가 꺼내 쓴다.
    prepared: Option<Vec<usize>>,
    /// 창이 입력 시점에 묶은 키 주인과 이번 프레임 키 주인(`frame_keys.rs`).
    keys: frame_keys::KeySlots,
    /// 이 창의 포커스 위치가 팝업이면 그 노드(`focus.rs`). 창마다 하나다. 없으면 포커스 위치는 팝업이 아니다
    /// (엔진 포커스 surface, html 주소 띠, tutorial 말풍선).
    focus: Option<focus::PopupNode>,
}

impl PopupManager {
    pub fn new() -> Self {
        Self {
            popups: Vec::new(),
            closed_queue: Vec::new(),
            driven: Vec::new(),
            notice_column: None,
            plugin_parents: Default::default(),
            prepared: None,
            keys: Default::default(),
            focus: None,
        }
    }

    /// 정의를 등록하고 제목을 번역한다. 언어 변경 시 draw에서 다시 번역한다.
    /// sizer가 없을 때만 default_size에 ui_zoom을 곱한다. sizer는 배율을 직접 적용한다.
    pub fn register_def(&mut self, def: &PopupDef, ui_zoom: f32) {
        if self.popups.iter().any(|p| p.id == def.id) {
            return;
        }
        // 등록 때는 대상 id가 없으므로 Window 범위로 둔다. 다른 범위는 여는 쪽이 대상과 함께 정한다.
        let mut popup = PopupState::declared(
            def.id,
            crate::i18n::t(def.title_key),
            &def.decl,
            def.drag_region,
            PopupScope::Window,
        )
        .with_fullscreen_stage(def.fullscreen_stage);
        if def.sizer.is_none() {
            popup.set_needed_size(popup.needed_size * ui_zoom);
            popup.min_size *= ui_zoom;
        }
        self.popups.push(popup);
    }

    /// 정의 없이 팝업을 등록한다. 시험용 정의만 쓴다. 이미 등록돼 있으면 아무 일도 하지 않는다.
    #[cfg(test)]
    pub fn register(&mut self, popup: PopupState) {
        if !self.popups.iter().any(|p| p.id == popup.id) {
            self.popups.push(popup);
        }
    }

    /// 열 요청이 쓸 항목을 인스턴스 정책으로 고른다. 없으면 같은 선언의 항목을 본떠 만든다.
    fn slot_for_open(&mut self, id: PopupId, scope: &PopupScope) -> Option<usize> {
        let policy = self.popups.iter().find(|p| p.id == id)?.instances;
        let open_same_scope = || {
            self.popups
                .iter()
                .position(|p| p.id == id && p.open && p.scope == *scope)
        };
        let closed = || self.popups.iter().position(|p| p.id == id && !p.open);
        let found = match policy {
            PopupInstances::Single => self
                .popups
                .iter()
                .position(|p| p.id == id && p.open)
                .or_else(closed),
            PopupInstances::PerScopeTarget | PopupInstances::Queue => {
                open_same_scope().or_else(closed)
            }
            PopupInstances::Multi => closed(),
        };
        if found.is_some() {
            return found;
        }
        let base = self.popups.iter().rposition(|p| p.id == id)?;
        let mut fresh = self.popups[base].fresh_copy();
        fresh.reset_closed();
        fresh.request_center = false;
        fresh.request_top = false;
        self.popups.push(fresh);
        Some(self.popups.len() - 1)
    }

    /// 항목을 열고 범위를 정해 맨 앞으로 올린다. 닫혀 있던 항목은 새 인스턴스 id를 받는다.
    /// 올린 뒤의 위치(맨 끝)를 돌려준다.
    fn raise_open(&mut self, i: usize, scope: PopupScope) -> usize {
        let p = &mut self.popups[i];
        if !p.open {
            p.open = true;
            p.instance = PopupInstanceId::next();
        }
        p.scope = scope;
        // 포커스를 받지 않는 팝업은 host·plugin 어느 팝업보다도 아래다. 순번을 쓰는 판정(가림·키 주인)도 같다.
        p.z_seq = if p.arrange.takes_focus {
            tasty_host_plugin::next_popup_z_seq()
        } else {
            0
        };
        let popup = self.popups.remove(i);
        self.popups.push(popup);
        self.popups.len() - 1
    }

    /// 인스턴스 정책에 따라 열 항목을 골라 맨 앞으로 연다. 선언이 등록되지 않았으면 `None`이다.
    fn open_slot(&mut self, id: PopupId, scope: PopupScope) -> Option<&mut PopupState> {
        let i = self.slot_for_open(id, &scope)?;
        let i = self.raise_open(i, scope);
        Some(&mut self.popups[i])
    }

    /// 인스턴스 정책으로 보아 이 요청이 쓸 열린 인스턴스. `Multi`는 늘 새로 열므로 없다.
    pub fn instance_for(&self, id: PopupId, scope: &PopupScope) -> Option<PopupInstanceId> {
        let policy = self.popups.iter().find(|p| p.id == id)?.instances;
        let found = match policy {
            PopupInstances::Single => self.topmost_of(id),
            PopupInstances::PerScopeTarget | PopupInstances::Queue => self
                .popups
                .iter()
                .rev()
                .find(|p| p.id == id && p.open && p.scope == *scope),
            PopupInstances::Multi => None,
        };
        found.map(|p| p.instance)
    }

    /// 요청이 실제로 열릴 범위. 부모가 있으면 부모의 범위를 따른다(부모가 보이지 않으면 자식도 보이지 않는다).
    /// 중복 판정과 실제 열기가 같은 범위를 쓰도록 둘 다 이 값을 쓴다.
    pub fn request_scope(&self, request: &crate::intent::OpenRequest) -> PopupScope {
        request
            .parent
            .and_then(|parent| self.instance(parent))
            .map_or_else(|| request.scope.clone(), |parent| parent.scope.clone())
    }

    /// 이 요청으로 연 인스턴스가 포커스를 받는지. 선언이 등록됐고, 사용자 출처이며, 요청이 포커스를 원하고, 선언이
    /// 포커스를 받는 배치(`Arrange::takes_focus`)일 때다. 실제 열기(`open_request`)와 즉시 적용 판정
    /// (`intent::popup::gives_user_focus`)이 이 값 하나를 쓴다.
    pub fn request_takes_focus(&self, request: &crate::intent::OpenRequest, by_user: bool) -> bool {
        by_user
            && request.focus
            && self
                .popups
                .iter()
                .find(|p| p.id == request.id)
                .is_some_and(|p| p.arrange.takes_focus)
    }

    /// 열기 요청 하나를 인스턴스로 연다. 위치·포커스·부모·데이터는 요청을 따른다.
    /// 사용자 출처가 아니면 포커스 없이 passive로 연다. 선언이 등록되지 않았으면 `None`이다.
    pub fn open_request(
        &mut self,
        request: &crate::intent::OpenRequest,
        by_user: bool,
    ) -> Option<PopupInstanceId> {
        use crate::intent::PopupPlacement;
        let focused = self.request_takes_focus(request, by_user);
        let p = self.open_slot(request.id, self.request_scope(request))?;
        match request.placement {
            PopupPlacement::Keep => {}
            PopupPlacement::ScopeCenter => p.request_center = true,
            PopupPlacement::ScopeTop => {
                p.request_center = false;
                p.request_top = true;
            }
            PopupPlacement::AtPoint(pos) => {
                p.pos = pos;
                p.request_center = false;
            }
        }
        if !by_user {
            p.passive = true;
        }
        p.parent = request.parent;
        p.plugin_parent = request.plugin_parent;
        p.payload = request.payload.take();
        if let Some(covers) = request.covers_scope {
            p.look = p.declared_look.covering(covers);
        }
        let iid = p.instance;
        // 사용자가 연 팝업은 창의 포커스 위치가 된다. passive 로 연 팝업은 포커스 위치를 바꾸지 않는다.
        if focused {
            self.give_focus(iid);
        }
        Some(iid)
    }

    /// 큐 팝업이면 열린 인스턴스의 차례 뒤에 요청의 항목을 잇는다. 큐 팝업이 아니면 아무것도 하지 않는다.
    /// 위치·포커스는 그대로다. 이었으면 true다.
    pub fn append_queued(
        &mut self,
        iid: PopupInstanceId,
        request: &crate::intent::OpenRequest,
    ) -> bool {
        let Some(p) = self.instance_mut(iid) else {
            return false;
        };
        if p.instances != PopupInstances::Queue {
            return false;
        }
        p.payload.append_queue(request.payload.take());
        true
    }

    /// 닫힌 큐 팝업의 남은 항목을 다시 연다. 닫기 훅은 프레임 끝에 돌므로 그 사이 같은 범위에 새 인스턴스가
    /// 열렸을 수 있다. 그러면 남은 항목을 그 큐의 앞에 넣고 그 인스턴스의 위치·포커스·출처를 그대로 둔다.
    /// 남은 항목이 먼저 들어온 요청이므로 차례가 앞선다. 열린 인스턴스가 없으면 새로 연다.
    pub fn reopen_queue_rest(&mut self, request: &crate::intent::OpenRequest, by_user: bool) {
        let scope = self.request_scope(request);
        if let Some(iid) = self.instance_for(request.id, &scope)
            && let Some(p) = self.instance_mut(iid)
            && p.instances == PopupInstances::Queue
        {
            p.payload.prepend_queue(request.payload.take());
            return;
        }
        self.open_request(request, by_user);
    }

    /// 열린 인스턴스의 데이터를 바꾼다. 위치·포커스는 그대로다.
    pub fn replace_payload(&mut self, iid: PopupInstanceId, payload: PopupPayload) {
        if let Some(p) = self.instance_mut(iid) {
            p.payload = payload;
        }
    }

    /// 팝업을 맨 앞으로 열고 범위를 정한다. 위치와 포커스는 바꾸지 않는다.
    /// 모든 열기 함수가 범위를 받는다. 등록 값이나 직전에 열렸을 때의 범위를 쓰지 않는다.
    #[cfg(test)]
    pub fn open(&mut self, id: PopupId, scope: PopupScope) -> Option<PopupInstanceId> {
        self.open_slot(id, scope).map(|p| p.instance)
    }

    /// 범위의 중앙에 열고 포커스를 준다. 요청 출처를 보지 않으므로 메인 창의 팝업은 `open_request`로 열고,
    /// 이 함수는 별도 창(설정 창·파일 선택 창)이 자기 관리자에서만 쓴다.
    pub fn open_centered_focused(
        &mut self,
        id: PopupId,
        scope: PopupScope,
    ) -> Option<PopupInstanceId> {
        let p = self.open_slot(id, scope)?;
        p.passive = false;
        p.request_center = true;
        let iid = p.instance;
        self.give_focus(iid);
        Some(iid)
    }

    /// 지정한 위치에 열고 포커스를 준다.
    #[cfg(test)]
    pub fn open_at_focused(
        &mut self,
        id: PopupId,
        scope: PopupScope,
        pos: egui::Pos2,
    ) -> Option<PopupInstanceId> {
        let p = self.open_slot(id, scope)?;
        p.passive = false;
        p.pos = pos;
        p.request_center = false;
        let iid = p.instance;
        self.give_focus(iid);
        Some(iid)
    }

    /// 열린 인스턴스를 닫는다. `record`이면 closed_queue에 사유와 데이터를 남긴다.
    /// 같은 선언의 다른 항목이 있으면 목록에서 빼고, 없으면 닫힌 원본으로 남긴다.
    /// 이 인스턴스를 부모로 둔 자식도 함께 닫는다.
    fn close_at(&mut self, i: usize, reason: PopupCloseReason, record: bool) {
        let p = &mut self.popups[i];
        if !p.open {
            return;
        }
        let closed = ClosedPopup {
            id: p.id,
            instance: p.instance,
            reason,
            scope: p.scope.clone(),
            payload: std::mem::take(&mut p.payload),
        };
        let parents = (p.parent, p.plugin_parent);
        p.reset_closed();
        let id = closed.id;
        let iid = closed.instance;
        if record {
            self.closed_queue.push(closed);
        }
        let has_other = self
            .popups
            .iter()
            .enumerate()
            .any(|(j, q)| j != i && q.id == id);
        if has_other {
            self.popups.remove(i);
        }
        let children: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|q| q.open && q.parent == Some(iid))
            .map(|q| q.instance)
            .collect();
        for child in children {
            self.close_instance_with(child, PopupCloseReason::ParentClosed, record);
        }
        self.focus_after_close(iid, parents);
    }

    fn close_instance_with(
        &mut self,
        iid: PopupInstanceId,
        reason: PopupCloseReason,
        record: bool,
    ) {
        if let Some(i) = self.index_of(iid) {
            self.close_at(i, reason, record);
        }
    }

    fn index_of(&self, iid: PopupInstanceId) -> Option<usize> {
        self.popups.iter().position(|p| p.open && p.instance == iid)
    }

    /// 열린 인스턴스 하나를 사유와 함께 닫는다. 이미 닫혔으면 아무 일도 하지 않는다.
    pub fn close_instance(&mut self, iid: PopupInstanceId, reason: PopupCloseReason) {
        self.close_instance_with(iid, reason, true);
    }

    /// 이 선언의 열린 인스턴스를 모두 닫는다. 사유는 팝업 밖 요청(`Command`)이다.
    pub fn close(&mut self, id: PopupId) {
        self.close_with(id, PopupCloseReason::Command);
    }

    /// 이 선언의 열린 인스턴스를 모두 사유와 함께 닫는다.
    pub fn close_with(&mut self, id: PopupId, reason: PopupCloseReason) {
        let open: Vec<PopupInstanceId> = self.instances_of(id).map(|p| p.instance).collect();
        for iid in open {
            self.close_instance(iid, reason);
        }
    }

    /// 위치·크기는 유지하고 이동·리사이즈만 끝낸다. 전체화면 무대 등으로 팝업을
    /// 그리지 않는 동안에는 draw가 포인터 해제를 처리하지 못하므로 전환 전에 호출한다.
    pub fn cancel_pointer_interactions(&mut self) {
        for p in &mut self.popups {
            p.dragging = false;
            p.resizing = None;
        }
    }

    /// 범위 대상이 사라진 열린 팝업을 닫는다. ID 범위는 어떤 화면과도 다시 맞지 않아 열린 채 숨기 때문이다.
    /// 닫힌 팝업은 closed_queue에 들어가 on_close 훅이 이어진다.
    pub fn close_scope_orphans(&mut self, target_exists: impl Fn(&PopupScope) -> bool) {
        let orphans: Vec<PopupInstanceId> = self
            .popups
            .iter()
            .filter(|p| p.open && !target_exists(&p.scope))
            .map(|p| p.instance)
            .collect();
        for iid in orphans {
            self.close_instance(iid, PopupCloseReason::TargetGone);
        }
    }

    /// 닫힌 인스턴스 목록을 꺼내 비운다. 호출부가 훅을 실행하며 다른 팝업도 닫을 수 있다.
    pub fn take_closed_queue(&mut self) -> Vec<ClosedPopup> {
        std::mem::take(&mut self.closed_queue)
    }

    /// 닫혔지만 닫기 훅이 아직 처리하지 않은 인스턴스 중 `id`·`scope` 가 같은 것이 있는지.
    pub fn is_closing_in(&self, id: PopupId, scope: &PopupScope) -> bool {
        self.closed_queue
            .iter()
            .any(|c| c.id == id && c.scope == *scope)
    }

    /// 닫힌 인스턴스의 선언 id만 꺼낸다. 사유와 데이터가 필요 없는 호출자가 쓴다.
    pub fn take_closed_ids(&mut self) -> Vec<PopupId> {
        self.take_closed_queue().into_iter().map(|c| c.id).collect()
    }

    /// 선언이 소속 범위를 덮는다고 한 팝업인지. 등록되지 않았으면 false다.
    #[cfg(test)]
    pub fn covers_scope(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.look.covers_scope)
    }

    /// 다른 범위로 다시 열기 위해 팝업을 닫는다. 내용은 새 요청이 이미 채웠으므로
    /// closed_queue에 넣지 않아 on_close 훅이 새 요청을 지우지 않는다.
    pub fn close_for_reopen(&mut self, id: PopupId) {
        let open: Vec<PopupInstanceId> = self.instances_of(id).map(|p| p.instance).collect();
        for iid in open {
            self.close_instance_with(iid, PopupCloseReason::Command, false);
        }
    }

    /// 이 선언의 열린 인스턴스. 아래에서 위 순서다.
    pub fn instances_of(&self, id: PopupId) -> impl Iterator<Item = &PopupState> + '_ {
        self.popups.iter().filter(move |p| p.id == id && p.open)
    }

    /// 이 선언의 맨 위 열린 인스턴스의 데이터.
    #[cfg(test)]
    pub fn topmost_payload(&self, id: PopupId) -> Option<&PopupPayload> {
        self.topmost_of(id).map(|p| &p.payload)
    }

    /// 이 선언의 맨 위 열린 인스턴스.
    fn topmost_of(&self, id: PopupId) -> Option<&PopupState> {
        self.popups.iter().rev().find(|p| p.id == id && p.open)
    }

    /// 열린 인스턴스.
    pub fn instance(&self, iid: PopupInstanceId) -> Option<&PopupState> {
        self.popups.iter().find(|p| p.open && p.instance == iid)
    }

    /// 열린 인스턴스.
    pub fn instance_mut(&mut self, iid: PopupInstanceId) -> Option<&mut PopupState> {
        self.popups.iter_mut().find(|p| p.open && p.instance == iid)
    }

    /// Check if a popup is open.
    pub fn is_open(&self, id: PopupId) -> bool {
        self.topmost_of(id).is_some()
    }

    /// 열려 있고 직전 그리기에서 범위가 보인 팝업의 사각형. 인스턴스가 여럿이면 맨 위 것이다.
    #[cfg(test)]
    pub fn visible_rect(&self, id: PopupId) -> Option<egui::Rect> {
        self.popups
            .iter()
            .rev()
            .find(|p| p.id == id && p.open && p.scope_visible)
            .map(|p| p.popup_rect())
    }

    /// 열린 팝업의 범위. 닫혀 있으면 `None`이다. 인스턴스가 여럿이면 맨 위 것이다.
    /// 범위가 지금 보이는지는 따지지 않는다.
    pub fn open_scope(&self, id: PopupId) -> Option<&PopupScope> {
        self.topmost_of(id).map(|p| &p.scope)
    }

    /// 열린 popup이 passive인지. 닫혔으면 false다.
    #[cfg(test)]
    pub fn is_passive(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.passive)
    }

    /// 최신 draw에서 보이는 열린 popup 중 native WebView를 모두 숨기고 키보드 포커스를 회수해야 하는 것이
    /// 있는지. passive popup은 [`Self::visible_passive_rects`]로 겹치는 WebView만 숨긴다. 포커스를 받지 않는
    /// 팝업은 어느 쪽에도 세지 않고 [`Self::quiet_popup_rects`]로 WebView에 구멍만 낸다.
    pub fn has_visible_open(&self) -> bool {
        self.popups
            .iter()
            .any(|p| p.open && p.scope_visible && !p.passive && p.arrange.takes_focus)
    }

    /// 최신 draw에서 보이는 passive popup의 화면 영역(logical px). 포커스를 받지 않는 팝업은 뺀다.
    pub fn visible_passive_rects(&self) -> Vec<egui::Rect> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.passive && p.arrange.takes_focus)
            .map(|p| p.popup_rect())
            .collect()
    }

    /// 맨 앞으로 올리고 host·plugin 공용 순번도 갱신한다.
    /// 포커스를 받지 않는 인스턴스는 올리지 않는다. 그런 팝업은 다른 팝업 아래에 그린다.
    fn bring_to_front(&mut self, iid: PopupInstanceId) {
        if self.never_focused(iid) {
            return;
        }
        if let Some(i) = self.index_of(iid) {
            let mut popup = self.popups.remove(i);
            popup.z_seq = tasty_host_plugin::next_popup_z_seq();
            self.popups.push(popup);
        }
    }

    /// 열린 host 팝업의 가장 큰 z_seq. plugin 팝업과 표시 순서를 비교할 때 쓴다.
    /// 포커스를 받지 않는 팝업은 다른 팝업 아래에 그리므로 뺀다.
    pub fn max_open_z_seq(&self) -> Option<u64> {
        self.popups
            .iter()
            .filter(|p| p.open && p.arrange.takes_focus)
            .map(|p| p.z_seq)
            .max()
    }

    /// 열린 팝업의 순번과 화면 영역. 없거나 닫혔으면 None이다. 인스턴스가 여럿이면 맨 위 것이다.
    pub fn open_geometry(&self, id: PopupId) -> Option<(u64, egui::Rect)> {
        self.topmost_of(id).map(|p| (p.z_seq, p.popup_rect()))
    }

    /// 이 선언의 맨 위 열린 인스턴스. 열린 것이 없으면 닫힌 등록 원본이다.
    pub fn get_mut(&mut self, id: PopupId) -> Option<&mut PopupState> {
        let i = self
            .popups
            .iter()
            .rposition(|p| p.id == id && p.open)
            .or_else(|| self.popups.iter().position(|p| p.id == id))?;
        Some(&mut self.popups[i])
    }

    /// 이 선언의 항목 전부(열린 인스턴스와 닫힌 원본). 매 프레임 제목·크기를 갱신할 때 쓴다.
    /// 이 선언의 모든 항목(열린 인스턴스와 닫힌 원본). `entries_of_mut`와 순서가 같다.
    pub fn entries_of(&self, id: PopupId) -> impl Iterator<Item = &PopupState> + '_ {
        self.popups.iter().filter(move |p| p.id == id)
    }

    pub fn entries_of_mut(&mut self, id: PopupId) -> impl Iterator<Item = &mut PopupState> + '_ {
        self.popups.iter_mut().filter(move |p| p.id == id)
    }
}

#[cfg(test)]
#[path = "popup/parent_tests.rs"]
mod parent_tests;

#[cfg(test)]
#[path = "popup/open_when_tests.rs"]
mod open_when_tests;

#[cfg(test)]
#[path = "popup/arrange_tests.rs"]
mod arrange_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY_ID: PopupId = "on_close_hook_test_dummy";

    fn dummy(close_on_outside_click: bool) -> PopupState {
        PopupState::new(
            DUMMY_ID,
            "dummy",
            egui::vec2(200.0, 100.0),
            PopupScope::Window,
        )
        .with_close_on_outside_click(close_on_outside_click)
    }

    /// 타이틀바 전체화면 버튼용 더미 — 크기/위치를 고정해 rect 산술을 단정한다.
    fn titled(
        stage: Option<crate::adapters::ui::fullscreen::StageId>,
        headless: bool,
    ) -> PopupState {
        PopupState::new(
            DUMMY_ID,
            "dummy",
            egui::vec2(300.0, 200.0),
            PopupScope::Window,
        )
        .with_headless(headless)
        .with_fullscreen_stage(stage)
    }

    /// 버튼은 **무대를 선언한 non-headless popup 에만** 그려진다. 플래그가 없으면
    /// (대부분의 popup) rect 자체가 없어 렌더/hit-test 블록이 통째로 돌지 않는다.
    #[test]
    fn fullscreen_button_rect_only_when_flag_set() {
        assert!(titled(Some("blank"), false).fullscreen_btn_rect().is_some());
        assert!(titled(None, false).fullscreen_btn_rect().is_none());
        // headless 는 타이틀바 자체가 없다 — 플래그와 무관하게 버튼 없음.
        assert!(titled(Some("blank"), true).fullscreen_btn_rect().is_none());
    }

    /// 키보드 탈출구가 고르는 대상 — 포커스된 **하나**뿐이다.
    fn managed(entries: Vec<(PopupState, bool)>) -> PopupManager {
        let focus = entries
            .iter()
            .rev()
            .find(|(_, focused)| *focused)
            .map(|(p, _)| focus::PopupNode::Host(p.instance));
        PopupManager {
            popups: entries.into_iter().map(|(p, _)| p).collect(),
            closed_queue: Vec::new(),
            driven: Vec::new(),
            notice_column: None,
            plugin_parents: Default::default(),
            prepared: None,
            keys: Default::default(),
            focus,
        }
    }

    /// 시험용 인스턴스. `focused`면 `managed`가 이 인스턴스를 포커스 위치로 둔다.
    fn entry(id: PopupId, open: bool, focused: bool, closes: bool) -> (PopupState, bool) {
        let mut p = dummy(closes);
        p.id = id;
        p.open = open;
        p.instance = PopupInstanceId::next();
        (p, focused)
    }

    /// 포커스 위치가 숨은 범위의 팝업이나 닫힌 팝업이면 host 키 주인이 없다. 순번이 더 큰 포커스 없는(passive
    /// 포함) 팝업이 있어도 보이는 포커스 위치가 키를 받는다.
    #[test]
    fn a_hidden_or_closed_focus_node_is_no_key_owner() {
        let (mut shown, _) = entry("shown", true, true, false);
        shown.z_seq = 1;
        let (mut passive, _) = entry("passive", true, false, false);
        passive.z_seq = 11;
        let m = managed(vec![(shown, true), (passive, false)]);
        assert_eq!(m.host_key_owner().map(|p| p.id), Some("shown"));

        let (mut hidden, _) = entry("hidden", true, true, false);
        hidden.scope_visible = false;
        assert!(managed(vec![(hidden, true)]).host_key_owner().is_none());
        assert!(
            managed(vec![entry("closed", false, true, false)])
                .host_key_owner()
                .is_none()
        );
    }

    #[test]
    fn dismissal_target_is_the_focused_popup_and_carries_its_outside_click_flag() {
        let m = managed(vec![entry("a", true, true, true)]);
        assert_eq!(
            m.focused_dismissal_target().map(|(id, _, c)| (id, c)),
            Some(("a", true))
        );

        let m = managed(vec![entry("a", true, true, false)]);
        assert_eq!(
            m.focused_dismissal_target().map(|(id, _, c)| (id, c)),
            Some(("a", false))
        );
    }

    /// 포커스 없는 팝업은 Escape로 닫을 대상이 아니다.
    #[test]
    fn an_open_but_unfocused_popup_is_not_a_dismissal_target() {
        let m = managed(vec![entry("a", true, false, true)]);
        assert_eq!(m.focused_dismissal_target(), None);
    }

    /// 닫힌 popup 이 포커스 위치로 남아 있어도 대상이 아니다. `has_focused` 와 같은 술어를 써야 둘이 어긋나지
    /// 않는다.
    #[test]
    fn a_closed_popup_is_not_a_dismissal_target_even_if_it_kept_the_focus_node() {
        let m = managed(vec![entry("a", false, true, true)]);
        assert_eq!(m.focused_dismissal_target(), None);
        assert!(!m.has_focused());
    }

    /// sticky_focus와 close_on_outside_click=false를 함께 쓰면 바깥 클릭과 Escape의
    /// 포커스 해제 동작이 달라진다. 현재 등록에는 이 조합이 없는지 검사한다.
    #[test]
    fn no_popup_is_both_sticky_focused_and_immune_to_an_outside_click() {
        let offenders: Vec<&str> = defs::all_defs()
            .iter()
            .filter(|d| d.decl.sticky_focus && !d.decl.closes_on_outside_click())
            .map(|d| d.id)
            .collect();
        assert!(
            offenders.is_empty(),
            "sticky_focus와 close_on_outside_click=false를 함께 쓴 팝업: {offenders:?}. \
             이 조합은 바깥 클릭과 Escape의 포커스 해제 동작이 다르다. \
             의도한 조합이라면 팝업 정책과 문서, 이 검사를 함께 갱신해야 한다."
        );
    }

    /// 옆에 열린 popup 이 더 있어도 **포커스된 것만** 고른다.
    #[test]
    fn other_open_popups_are_left_alone() {
        let m = managed(vec![
            entry("bystander", true, false, true),
            entry("focused", true, true, false),
        ]);
        assert_eq!(
            m.focused_dismissal_target().map(|(id, _, c)| (id, c)),
            Some(("focused", false))
        );
    }

    #[test]
    fn close_button_rect_is_untouched_by_the_fullscreen_button() {
        assert_eq!(
            titled(Some("blank"), false).close_btn_rect(),
            titled(None, false).close_btn_rect()
        );
    }

    /// 제목 elide 기준(우측 버튼군의 좌변)은 버튼이 없으면 close 좌변 그대로이고,
    /// 버튼이 생기면 정확히 "버튼 폭 + 간격" 만큼 왼쪽으로 이동한다.
    #[test]
    fn title_elide_basis_accounts_for_the_fullscreen_button() {
        let without = titled(None, false);
        assert_eq!(
            without.title_buttons_left_x(),
            without.close_btn_rect().min.x
        );

        let with = titled(Some("blank"), false);
        let close = with.close_btn_rect();
        assert_eq!(
            with.title_buttons_left_x(),
            close.min.x - close.width() - title_btn_gap()
        );
        assert!(with.fullscreen_btn_rect().unwrap().max.x <= close.min.x);
    }

    /// 제목 띠는 버튼 하나면 양쪽 32, 둘이면 양쪽 60을 비우고 스트립 가운데에 놓인다.
    #[test]
    fn title_band_reserves_32_or_60_on_both_sides() {
        for (stage, reserve) in [(None, 32.0), (Some("blank"), 60.0)] {
            let p = titled(stage, false);
            let strip = p.title_rect();
            let band = p.title_text_rect();
            assert_eq!(p.close_btn_rect().width(), 24.0, "{stage:?}");
            assert_eq!(band.min.x - strip.min.x, reserve, "{stage:?}");
            assert_eq!(strip.max.x - band.max.x, reserve, "{stage:?}");
        }
    }

    /// `close()` 는 호출 직전 `open` 이었던 popup 만 `closed_queue` 에 push 한다.
    #[test]
    fn close_pushes_to_queue_only_when_was_open() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));

        mgr.close(DUMMY_ID);
        assert!(mgr.take_closed_ids().is_empty());

        mgr.open(DUMMY_ID, crate::model::popup_kind::PopupScope::Window);
        mgr.close(DUMMY_ID);
        assert_eq!(mgr.take_closed_ids(), vec![DUMMY_ID]);
    }

    #[test]
    fn close_on_already_closed_popup_does_not_repush() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));
        mgr.open(DUMMY_ID, crate::model::popup_kind::PopupScope::Window);
        mgr.close(DUMMY_ID);
        assert_eq!(mgr.take_closed_ids(), vec![DUMMY_ID]);

        mgr.close(DUMMY_ID); // 이미 닫힌 상태에서 재호출.
        assert!(mgr.take_closed_ids().is_empty());
    }

    #[test]
    fn take_closed_queue_drains() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));
        mgr.open(DUMMY_ID, crate::model::popup_kind::PopupScope::Window);
        mgr.close(DUMMY_ID);

        assert_eq!(mgr.take_closed_ids(), vec![DUMMY_ID]);
        assert!(mgr.take_closed_ids().is_empty());
    }

    fn policy_mgr(policy: PopupInstances) -> PopupManager {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false).with_instances(policy));
        mgr
    }

    fn open_ids(mgr: &PopupManager) -> Vec<PopupInstanceId> {
        mgr.instances_of(DUMMY_ID).map(|p| p.instance).collect()
    }

    /// 범위 대상마다 하나: 다른 범위면 새 인스턴스, 같은 범위면 그 인스턴스를 앞으로 가져온다.
    #[test]
    fn per_scope_target_opens_one_instance_per_scope() {
        let mut mgr = policy_mgr(PopupInstances::PerScopeTarget);
        let l = mgr.open(DUMMY_ID, PopupScope::Surface(1)).unwrap();
        let r = mgr.open(DUMMY_ID, PopupScope::Surface(2)).unwrap();
        assert_ne!(l, r);
        assert_eq!(open_ids(&mgr), vec![l, r]);
        assert_eq!(mgr.open(DUMMY_ID, PopupScope::Surface(1)), Some(l));
        assert_eq!(
            open_ids(&mgr),
            vec![r, l],
            "the reopened instance comes to the front"
        );
        assert_eq!(mgr.instance(r).unwrap().scope, PopupScope::Surface(2));
    }

    /// 한 인스턴스를 닫아도 다른 범위의 인스턴스는 열린 채 위치·범위가 그대로다.
    #[test]
    fn closing_one_scope_instance_leaves_the_other() {
        let mut mgr = policy_mgr(PopupInstances::PerScopeTarget);
        let l = mgr.open(DUMMY_ID, PopupScope::Surface(1)).unwrap();
        let r = mgr.open(DUMMY_ID, PopupScope::Surface(2)).unwrap();
        mgr.instance_mut(r).unwrap().pos = egui::pos2(40.0, 60.0);
        mgr.close_instance(l, PopupCloseReason::Escape);
        assert_eq!(open_ids(&mgr), vec![r]);
        let rest = mgr.instance(r).unwrap();
        assert_eq!(rest.pos, egui::pos2(40.0, 60.0));
        assert_eq!(rest.scope, PopupScope::Surface(2));
        let closed = mgr.take_closed_queue();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].instance, l);
        assert_eq!(closed[0].reason, PopupCloseReason::Escape);
    }

    /// 인스턴스 하나: 다른 범위로 열면 같은 인스턴스가 옮겨진다.
    #[test]
    fn single_moves_the_one_instance() {
        let mut mgr = policy_mgr(PopupInstances::Single);
        let a = mgr.open(DUMMY_ID, PopupScope::Surface(1)).unwrap();
        let b = mgr.open(DUMMY_ID, PopupScope::Surface(2)).unwrap();
        assert_eq!(a, b);
        assert_eq!(open_ids(&mgr), vec![a]);
        assert_eq!(mgr.instance(a).unwrap().scope, PopupScope::Surface(2));
    }

    /// 여럿을 선언하면 같은 범위에서 두 번 열어 인스턴스가 둘이 된다.
    #[test]
    fn multi_opens_a_new_instance_each_time_in_the_same_scope() {
        let mut mgr = policy_mgr(PopupInstances::Multi);
        let a = mgr.open(DUMMY_ID, PopupScope::Window).unwrap();
        let b = mgr.open(DUMMY_ID, PopupScope::Window).unwrap();
        assert_ne!(a, b);
        assert_eq!(open_ids(&mgr), vec![a, b]);
        mgr.close_instance(a, PopupCloseReason::Command);
        mgr.close_instance(b, PopupCloseReason::Command);
        assert!(!mgr.is_open(DUMMY_ID));
        // 모두 닫혀도 다음 인스턴스의 원본은 하나 남는다. 이 매니저에는 이 선언 하나만 있다.
        assert!(mgr.get_mut(DUMMY_ID).is_some());
        assert_eq!(mgr.popups.len(), 1, "one closed template remains");
    }

    /// 부모 인스턴스를 닫으면 자식도 사유 `ParentClosed`로 닫힌다.
    #[test]
    fn closing_a_parent_closes_its_children() {
        const CHILD: PopupId = "on_close_hook_test_child";
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));
        mgr.register(PopupState::new(
            CHILD,
            "child",
            egui::vec2(10.0, 10.0),
            PopupScope::Window,
        ));
        let parent = mgr.open(DUMMY_ID, PopupScope::Window).unwrap();
        let child = mgr.open(CHILD, PopupScope::Window).unwrap();
        mgr.instance_mut(child).unwrap().parent = Some(parent);
        mgr.close_instance(parent, PopupCloseReason::OutsideClick);
        assert!(!mgr.is_open(CHILD));
        let closed = mgr.take_closed_queue();
        let reasons: Vec<_> = closed.iter().map(|c| (c.id, c.reason)).collect();
        assert_eq!(
            reasons,
            vec![
                (DUMMY_ID, PopupCloseReason::OutsideClick),
                (CHILD, PopupCloseReason::ParentClosed)
            ]
        );
    }

    /// 닫았다 다시 열면 새 인스턴스 id다.
    #[test]
    fn reopening_after_close_gives_a_new_instance_id() {
        let mut mgr = policy_mgr(PopupInstances::Single);
        let a = mgr.open(DUMMY_ID, PopupScope::Window).unwrap();
        mgr.close(DUMMY_ID);
        let b = mgr.open(DUMMY_ID, PopupScope::Window).unwrap();
        assert_ne!(a, b);
        assert!(mgr.instance(a).is_none());
    }

    /// 바깥 클릭으로 닫혀도 closed_queue에 기록되는지 실제 draw로 확인한다.
    #[test]
    fn outside_click_close_path_pushes_to_queue() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(true));
        mgr.open_at_focused(
            DUMMY_ID,
            crate::model::popup_kind::PopupScope::Window,
            egui::pos2(500.0, 500.0),
        );

        let ctx = egui::Context::default();
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1920.0, 1080.0),
            )),
            ..Default::default()
        };
        // 팝업(rect ≈ [500,500]-[700,600]) 에서 멀리 떨어진 바깥 좌표 클릭.
        raw.events.push(egui::Event::PointerButton {
            pos: egui::pos2(10.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });

        drop(ctx.run(raw, |ctx| {
            mgr.draw(ctx, &mut |_, _| {}, None, &[]);
        }));

        let closed = mgr.take_closed_queue();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].id, DUMMY_ID);
        assert_eq!(closed[0].reason, PopupCloseReason::OutsideClick);
    }
}
