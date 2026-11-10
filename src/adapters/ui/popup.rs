pub(crate) mod approval;
pub(crate) mod command_palette;
pub(crate) mod confirm_close_remote;
pub(crate) mod confirm_delete_category;
pub(crate) mod confirm_force_detach_workspace;
pub(crate) mod convert;
pub(crate) mod dag_list;
pub(crate) mod defs;
pub(crate) mod download_run;
mod draw;
pub(crate) mod explorer_conflict;
pub(crate) mod explorer_properties;
pub(crate) mod file_handler_picker;
pub(crate) mod file_picker;
pub(crate) mod frame;
pub(crate) mod occlusion;
pub(crate) mod port_scanner;
pub(crate) mod preset_apply;
pub(crate) mod rail_category;
pub(crate) mod remote_attach;
pub(crate) mod remote_tool;
/// 프레임을 그려 scrim의 실제 영역을 검사한다.
#[cfg(test)]
#[path = "popup/scrim_scope_tests.rs"]
mod scrim_scope_tests;
pub(crate) mod script_confirm;
pub(crate) mod transfer;

use crate::state::MainViewState;
use tasty_type_geometry::length::LogicalPx;

// 새 팝업은 popup::defs의 all_defs()에 등록한다.

pub use crate::model::popup_kind::{PopupId, PopupScope};

/// Result of a popup's draw call.
pub enum PopupAction {
    /// No action needed.
    None,
    /// The popup requests to be closed.
    Close,
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
    /// Whether the mouse is currently over any open popup.
    pub hovered: bool,
    /// 그린 팝업 레이어. egui_bridge에서 modifier-hint 바로 위에 배치한다.
    pub layers: Vec<egui::LayerId>,
    /// 전체화면 버튼으로 요청한 무대 ID. 실제 진입은 MainViewState를 가진 호출부에서 처리한다.
    pub fullscreen_requested: Option<crate::adapters::ui::fullscreen::StageId>,
    /// 실제 그린 팝업의 영역과 z_seq. plugin 팝업이 위에 덮인 host 팝업을 판별할 때 쓴다.
    /// 숨겨진 팝업은 마우스 입력을 가로채지 않도록 제외한다.
    pub hit_rects: Vec<occlusion::Occluder>,
    /// native WebView 위에 합성할 대상(non-passive 팝업). passive 팝업은 들어 있지 않다.
    pub overlay_targets: Vec<crate::state::webview_overlay::OverlayTarget>,
}

/// 등록 속성과 매 프레임 그리기 함수를 담은 팝업 정의.
pub struct PopupDef {
    pub id: PopupId,
    /// i18n 키. `t()`로 런타임 번역하여 popup title로 사용.
    pub title_key: &'static str,
    /// 동적 타이틀. 매 프레임 호출. `title_key` 대신 사용된다. (예: rename popup의
    /// 대상별 제목)
    pub title_fn:
        Option<fn(&MainViewState, &crate::runtime::engine_read::EngineRead<'_>) -> String>,
    /// 기본 크기. 동적 크기가 필요하면 `sizer`로 덮어쓸 수 있다.
    pub default_size: egui::Vec2,
    /// 매 프레임 크기를 계산한다. 사용자가 크기를 지정한 팝업에는 적용하지 않는다.
    pub sizer:
        Option<fn(&MainViewState, &crate::runtime::engine_read::EngineRead<'_>) -> egui::Vec2>,
    pub default_scope: PopupScope,
    pub close_on_outside_click: bool,
    /// true면 타이틀바·닫기 버튼 없이 콘텐츠만 렌더링한다 (컨텍스트 메뉴 스타일).
    pub headless: bool,
    /// true면 팝업 바깥 클릭해도 키보드 포커스가 유지된다.
    /// 닫기(Escape 등)로만 포커스 해제 가능. 검색 바 같은 오버레이용.
    pub sticky_focus: bool,
    /// 이동(드래그) 핸들 선언. `None`이면 이동 불가. `movable` 여부는 별도 bool 없이
    /// 이 값으로 표현한다.
    pub drag_handle: DragHandle,
    /// true면 테두리 8방향 드래그로 크기 조절 가능.
    pub resizable: bool,
    /// 리사이즈 최소 크기. `None`이면 `default_size`를 최소로 사용.
    pub min_size: Option<egui::Vec2>,
    /// 렌더링 함수. 매 프레임 호출. MainViewState에서 필요한 데이터를 꺼낸다.
    pub draw_fn: fn(
        &mut egui::Ui,
        &mut MainViewState,
        &crate::runtime::engine_read::EngineRead<'_>,
    ) -> PopupAction,
    /// 전체화면 버튼으로 열 무대 ID. None이거나 headless면 버튼이 없다.
    /// 무대는 별도 콘텐츠이며 원본 팝업은 열린 채 그 아래 남는다.
    pub fullscreen_stage: Option<crate::adapters::ui::fullscreen::StageId>,
    /// 열린 팝업이 close()를 통해 닫히면 closed_queue에 기록하고 frame에서 훅을 호출한다.
    /// egui 임시 상태를 지울 수 있도록 Context를 받는다.
    pub on_close: Option<
        fn(&egui::Context, &mut MainViewState, &crate::runtime::engine_read::EngineRead<'_>),
    >,
}

/// State for a single popup instance.
#[derive(Debug, Clone)]
pub struct PopupState {
    /// Unique identifier.
    pub id: PopupId,
    /// Title text displayed in the title bar.
    pub title: String,
    /// Whether the popup is open (a hidden scope does not close it).
    pub open: bool,
    /// Scope visibility from the latest draw; focus intent survives while hidden.
    scope_visible: bool,
    /// Position in logical pixels (top-left corner).
    pub pos: egui::Pos2,
    /// Size in logical pixels.
    pub size: egui::Vec2,
    /// Whether the popup is currently being dragged.
    dragging: bool,
    /// Drag offset from popup top-left to mouse position.
    drag_offset: egui::Vec2,
    /// Scope determines visibility and boundary clamping.
    pub scope: PopupScope,
    /// Whether this popup currently has keyboard focus.
    /// When focused, keyboard input should NOT be forwarded to the terminal.
    pub focused: bool,
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
    z_seq: u64,
    /// 리사이즈 시작 시점의 팝업 rect (드래그 누적 계산 기준).
    resize_start_rect: egui::Rect,
    /// 사용자가 한 번이라도 리사이즈했으면 true. true 동안 sizer 의 size 덮어쓰기를
    /// 막는다(`popup::frame::draw_popup_layer`). `close()`에서 리셋되어 다음 open 시 sizer 복원.
    pub size_user_overridden: bool,
    /// 타이틀바 전체화면 버튼이 올릴 무대 id. `register_def` 가 `PopupDef` 에서
    /// 전파한다. `None` 이면 버튼 없음.
    fullscreen_stage: Option<crate::adapters::ui::fullscreen::StageId>,
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
    pub fn new(id: PopupId, title: impl Into<String>, default_size: egui::Vec2) -> Self {
        Self {
            id,
            title: title.into(),
            open: false,
            scope_visible: true,
            pos: egui::pos2(100.0, 100.0),
            size: default_size,
            dragging: false,
            drag_offset: egui::Vec2::ZERO,
            scope: PopupScope::Window,
            focused: false,
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
        }
    }

    /// Create a popup with a specific scope.
    pub fn with_scope(mut self, scope: PopupScope) -> Self {
        self.scope = scope;
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

    fn popup_rect(&self) -> egui::Rect {
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
        reported_header_drag_rect(ctx, self.id).or_else(|| self.drag_handle_rect())
    }

    fn content_rect(&self) -> egui::Rect {
        let popup = self.popup_rect();
        // 구역별로 여백을 주는 팝업은 공통 내부 여백을 적용하지 않는다.
        let margin = if matches!(
            self.id,
            "remote_tool"
                | "notifications"
                | "remote_attach"
                | "command_palette"
                | "port_scanner"
                | "script_changed_confirm"
                | "rename"
                | transfer::TRANSFER_PROGRESS_POPUP_ID
                | transfer::TRANSFER_ERROR_POPUP_ID
                | crate::adapters::ui::info_modal::INFO_MODAL_ID
                | file_picker::FILE_PICKER_POPUP_ID
                | crate::view::file_chooser::FILE_CHOOSER_POPUP_ID
                | explorer_conflict::EXPLORER_CONFLICT_POPUP_ID
        ) {
            LogicalPx(0.0)
        } else {
            content_margin()
        };
        let top_offset = if self.headless {
            margin
        } else {
            title_bar_height() + margin
        };
        let min = egui::pos2(
            popup.min.x + margin.value(),
            popup.min.y + top_offset.value(),
        );
        // 범위가 제목줄과 여백보다 좁으면 콘텐츠 자리는 비어 있다. 뒤집힌 사각형을 만들지 않는다.
        let max = egui::pos2(popup.max.x - margin.value(), popup.max.y - margin.value()).max(min);
        egui::Rect::from_min_max(min, max)
    }

    fn close_btn_rect(&self) -> egui::Rect {
        let title = self.title_rect();
        let th = crate::theme::theme();
        let size = th.popup_title_btn_size().value();
        let edge_pad = th.popup_title_edge_inset().value();
        let center = egui::pos2(title.max.x - size * 0.5 - edge_pad, title.center().y);
        egui::Rect::from_center_size(center, egui::vec2(size, size))
    }

    /// 타이틀바가 있고 무대를 지정했을 때 전체화면 버튼 영역을 반환한다.
    /// 닫기 버튼 왼쪽에 배치하며 닫기 버튼 위치는 바꾸지 않는다.
    fn fullscreen_btn_rect(&self) -> Option<egui::Rect> {
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
        self.fullscreen_btn_rect()
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

    /// Clamp position so popup stays within the given screen rect.
    fn clamp_to_screen(&mut self, screen: egui::Rect) {
        self.size.x = self.size.x.min(screen.width());
        self.size.y = self.size.y.min(screen.height());
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

/// Manager for all internal popups. Handles z-ordering, dragging, and window clamping.
pub struct PopupManager {
    /// Popups in z-order (last = topmost).
    popups: Vec<PopupState>,
    /// `close()` 로 실제로 닫힌(= 그 호출 직전엔 열려 있던) popup id 대기열.
    /// `on_close` 훅 drain 이 프레임당 1회 `take_closed_queue()` 로 비운다.
    closed_queue: Vec<PopupId>,
}

impl PopupManager {
    pub fn new() -> Self {
        Self {
            popups: Vec::new(),
            closed_queue: Vec::new(),
        }
    }

    /// 정의를 등록하고 제목을 번역한다. 언어 변경 시 draw에서 다시 번역한다.
    /// sizer가 없을 때만 default_size에 ui_zoom을 곱한다. sizer는 배율을 직접 적용한다.
    pub fn register_def(&mut self, def: &PopupDef, ui_zoom: f32) {
        if self.popups.iter().any(|p| p.id == def.id) {
            return;
        }
        let initial_size = if def.sizer.is_some() {
            def.default_size
        } else {
            def.default_size * ui_zoom
        };
        let resolved_min = def.min_size.unwrap_or(def.default_size);
        let resolved_min = if def.sizer.is_some() {
            resolved_min
        } else {
            resolved_min * ui_zoom
        };
        let popup = PopupState::new(def.id, crate::i18n::t(def.title_key), initial_size)
            .with_scope(def.default_scope.clone())
            .with_close_on_outside_click(def.close_on_outside_click)
            .with_headless(def.headless)
            .with_sticky_focus(def.sticky_focus)
            .with_drag_handle(def.drag_handle)
            .with_resizable(def.resizable)
            .with_min_size(resolved_min)
            .with_fullscreen_stage(def.fullscreen_stage);
        self.popups.push(popup);
    }

    /// Register a popup. Call once during init. Does nothing if already registered.
    pub fn register(&mut self, popup: PopupState) {
        if !self.popups.iter().any(|p| p.id == popup.id) {
            self.popups.push(popup);
        }
    }

    /// Open a popup by id, bringing it to the front.
    pub fn open(&mut self, id: PopupId) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// Open a popup centered on screen, with focus.
    pub fn open_centered_focused(&mut self, id: PopupId) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].focused = true;
            self.popups[i].passive = false;
            self.popups[i].request_center = true;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// Open a popup centered on screen **without** focus (agent-initiated).
    /// 사용자의 포커스를 훔치지 않는다. CLI/IPC 경유 open에 사용.
    pub fn open_centered(&mut self, id: PopupId) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].focused = false;
            self.popups[i].request_center = true;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// Open a popup centered within a specific scope, with focus.
    pub fn open_with_scope(&mut self, id: PopupId, scope: PopupScope) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].focused = true;
            self.popups[i].passive = false;
            self.popups[i].request_center = true;
            self.popups[i].scope = scope;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// Open a popup at the top of a specific scope, with focus.
    /// scope rect의 상단에 가로 중앙 정렬로 배치한다.
    pub fn open_at_top_of_scope(&mut self, id: PopupId, scope: PopupScope) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].focused = true;
            self.popups[i].passive = false;
            self.popups[i].request_center = false;
            self.popups[i].request_top = true;
            self.popups[i].scope = scope;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// Open a popup at a specific position, with focus.
    pub fn open_at_focused(&mut self, id: PopupId, pos: egui::Pos2) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            self.popups[i].open = true;
            self.popups[i].focused = true;
            self.popups[i].passive = false;
            self.popups[i].pos = pos;
            self.popups[i].request_center = false;
            self.popups[i].z_seq = tasty_host_plugin::next_popup_z_seq();
            let popup = self.popups.remove(i);
            self.popups.push(popup);
        }
    }

    /// 열린 팝업을 닫고 closed_queue에 기록한다. 이미 닫혔으면 중복 기록하지 않는다.
    pub fn close(&mut self, id: PopupId) {
        if let Some(p) = self.popups.iter_mut().find(|p| p.id == id) {
            let was_open = p.open;
            p.open = false;
            p.dragging = false;
            p.focused = false;
            p.passive = false;
            // 리사이즈 상태 리셋 → 다음 open 시 sizer 가 크기를 다시 결정하도록 복원.
            p.resizing = None;
            p.size_user_overridden = false;
            if was_open {
                self.closed_queue.push(id);
            }
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
        let orphans: Vec<PopupId> = self
            .popups
            .iter()
            .filter(|p| p.open && !target_exists(&p.scope))
            .map(|p| p.id)
            .collect();
        for id in orphans {
            self.close(id);
        }
    }

    /// 닫힌 팝업 목록을 꺼내 비운다. 호출부가 훅을 실행하며 다른 팝업도 닫을 수 있다.
    pub fn take_closed_queue(&mut self) -> Vec<PopupId> {
        std::mem::take(&mut self.closed_queue)
    }

    /// Check if a popup is open.
    pub fn is_open(&self, id: PopupId) -> bool {
        self.popups.iter().any(|p| p.id == id && p.open)
    }

    /// 열려 있고 직전 그리기에서 범위가 보인 팝업의 사각형.
    pub fn visible_rect(&self, id: PopupId) -> Option<egui::Rect> {
        self.popups
            .iter()
            .find(|p| p.id == id && p.open && p.scope_visible)
            .map(|p| p.popup_rect())
    }

    /// 열린 팝업의 범위. 닫혀 있으면 `None`이다. 범위가 지금 보이는지는 따지지 않는다.
    pub fn open_scope(&self, id: PopupId) -> Option<&PopupScope> {
        self.popups
            .iter()
            .find(|p| p.id == id && p.open)
            .map(|p| &p.scope)
    }

    /// Check if any popup currently has keyboard focus.
    pub fn has_focused(&self) -> bool {
        self.popups
            .iter()
            .any(|p| p.open && p.scope_visible && p.focused)
    }

    /// Check whether a specific popup currently has keyboard focus.
    pub fn is_focused(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.scope_visible && p.focused)
    }

    /// Escape 대상인 포커스된 팝업 하나와 바깥 클릭 시 닫기 여부를 반환한다.
    /// Escape에는 좌표가 없으므로 바깥 클릭처럼 여러 팝업을 한꺼번에 닫지 않는다.
    pub fn focused_dismissal_target(&self) -> Option<(PopupId, bool)> {
        self.popups
            .iter()
            .find(|p| p.open && p.scope_visible && p.focused)
            .map(|p| (p.id, p.close_on_outside_click))
    }

    /// Set the keyboard-focus flag of a specific popup. 다른 popup 의 포커스는
    /// 건드리지 않는다 (검색창↔터미널 포커스 토글용).
    pub fn set_focused(&mut self, id: PopupId, focused: bool) {
        if let Some(p) = self.popups.iter_mut().find(|p| p.id == id) {
            p.focused = focused;
            p.passive &= !focused;
        }
    }

    /// 사용자 조작 근거가 없는 열기다. 키보드 포커스를 빼고 passive로 표시한다.
    pub fn mark_passive(&mut self, id: PopupId) {
        if let Some(p) = self.popups.iter_mut().find(|p| p.id == id && p.open) {
            p.focused = false;
            p.passive = true;
        }
    }

    /// 호스트가 확인한 사용자 입력으로 passive popup에 포커스를 준다. 팝업 안을 누른 것과 같은 전이다:
    /// 보이는 다른 팝업의 포커스를 풀고 맨 앞으로 올린다. 열린 passive popup이 보일 때만 바뀐다.
    pub fn focus_passive(&mut self, id: PopupId) -> bool {
        if !self.is_visible_passive(id) {
            return false;
        }
        for popup in &mut self.popups {
            if popup.scope_visible {
                popup.focused = popup.id == id;
                popup.passive &= !popup.focused;
            }
        }
        self.bring_to_front(id);
        true
    }

    /// 지금 화면에 보이는 passive popup인지. 범위 밖에 숨은 popup은 올릴 수 없다.
    pub fn is_visible_passive(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.scope_visible && p.passive)
    }

    /// 열린 popup이 passive인지. 닫혔으면 false다.
    #[cfg(test)]
    pub fn is_passive(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.passive)
    }

    /// 최신 draw에서 보이는 열린 popup 중 native WebView를 모두 숨겨야 하는 것이 있는지.
    /// passive popup은 [`Self::visible_passive_rects`]로 겹치는 WebView만 숨긴다.
    pub fn has_visible_open(&self) -> bool {
        self.popups
            .iter()
            .any(|p| p.open && p.scope_visible && !p.passive)
    }

    /// 최신 draw에서 보이는 passive popup의 화면 영역(logical px).
    pub fn visible_passive_rects(&self) -> Vec<egui::Rect> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.passive)
            .map(|p| p.popup_rect())
            .collect()
    }

    /// 키보드 포커스를 가진 열린 popup ID. draw 동안에는 매니저를 빌려 갈 수 없어 그리기 전에 복사해 둔다.
    pub fn keyboard_focused_ids(&self) -> Vec<PopupId> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.focused)
            .map(|p| p.id)
            .collect()
    }

    /// 맨 앞으로 올리고 host·plugin 공용 순번도 갱신한다.
    fn bring_to_front(&mut self, id: PopupId) {
        if let Some(i) = self.popups.iter().position(|p| p.id == id) {
            let mut popup = self.popups.remove(i);
            popup.z_seq = tasty_host_plugin::next_popup_z_seq();
            self.popups.push(popup);
        }
    }

    /// 열린 host 팝업의 가장 큰 z_seq. plugin 팝업과 표시 순서를 비교할 때 쓴다.
    pub fn max_open_z_seq(&self) -> Option<u64> {
        self.popups.iter().filter(|p| p.open).map(|p| p.z_seq).max()
    }

    /// 열린 팝업의 순번과 화면 영역. 없거나 닫혔으면 None이다.
    pub fn open_geometry(&self, id: PopupId) -> Option<(u64, egui::Rect)> {
        self.popups
            .iter()
            .find(|p| p.id == id && p.open)
            .map(|p| (p.z_seq, p.popup_rect()))
    }

    /// Get mutable access to a popup's state.
    pub fn get_mut(&mut self, id: PopupId) -> Option<&mut PopupState> {
        self.popups.iter_mut().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY_ID: PopupId = "on_close_hook_test_dummy";

    fn dummy(close_on_outside_click: bool) -> PopupState {
        PopupState::new(DUMMY_ID, "dummy", egui::vec2(200.0, 100.0))
            .with_close_on_outside_click(close_on_outside_click)
    }

    /// 타이틀바 전체화면 버튼용 더미 — 크기/위치를 고정해 rect 산술을 단정한다.
    fn titled(
        stage: Option<crate::adapters::ui::fullscreen::StageId>,
        headless: bool,
    ) -> PopupState {
        PopupState::new(DUMMY_ID, "dummy", egui::vec2(300.0, 200.0))
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
    fn managed(states: Vec<PopupState>) -> PopupManager {
        PopupManager {
            popups: states,
            closed_queue: Vec::new(),
        }
    }

    fn entry(id: PopupId, open: bool, focused: bool, closes: bool) -> PopupState {
        let mut p = dummy(closes);
        p.id = id;
        p.open = open;
        p.focused = focused;
        p
    }

    #[test]
    fn dismissal_target_is_the_focused_popup_and_carries_its_outside_click_flag() {
        let m = managed(vec![entry("a", true, true, true)]);
        assert_eq!(m.focused_dismissal_target(), Some(("a", true)));

        let m = managed(vec![entry("a", true, true, false)]);
        assert_eq!(m.focused_dismissal_target(), Some(("a", false)));
    }

    /// 포커스 없는 팝업은 Escape로 닫을 대상이 아니다.
    #[test]
    fn an_open_but_unfocused_popup_is_not_a_dismissal_target() {
        let m = managed(vec![entry("a", true, false, true)]);
        assert_eq!(m.focused_dismissal_target(), None);
    }

    /// 닫힌 popup 에 포커스 플래그가 남아 있어도 대상이 아니다 — `has_focused` 와 같은
    /// 술어(`open && focused`)를 써야 둘이 어긋나지 않는다.
    #[test]
    fn a_closed_popup_is_not_a_dismissal_target_even_if_it_kept_the_focus_flag() {
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
            .filter(|d| d.sticky_focus && !d.close_on_outside_click)
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
        assert_eq!(m.focused_dismissal_target(), Some(("focused", false)));
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
        assert!(mgr.take_closed_queue().is_empty());

        mgr.open(DUMMY_ID);
        mgr.close(DUMMY_ID);
        assert_eq!(mgr.take_closed_queue(), vec![DUMMY_ID]);
    }

    #[test]
    fn close_on_already_closed_popup_does_not_repush() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));
        mgr.open(DUMMY_ID);
        mgr.close(DUMMY_ID);
        assert_eq!(mgr.take_closed_queue(), vec![DUMMY_ID]);

        mgr.close(DUMMY_ID); // 이미 닫힌 상태에서 재호출.
        assert!(mgr.take_closed_queue().is_empty());
    }

    #[test]
    fn take_closed_queue_drains() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(false));
        mgr.open(DUMMY_ID);
        mgr.close(DUMMY_ID);

        assert_eq!(mgr.take_closed_queue(), vec![DUMMY_ID]);
        assert!(mgr.take_closed_queue().is_empty());
    }

    /// 바깥 클릭으로 닫혀도 closed_queue에 기록되는지 실제 draw로 확인한다.
    #[test]
    fn outside_click_close_path_pushes_to_queue() {
        let mut mgr = PopupManager::new();
        mgr.register(dummy(true));
        mgr.open_at_focused(DUMMY_ID, egui::pos2(500.0, 500.0));

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

        assert_eq!(mgr.take_closed_queue(), vec![DUMMY_ID]);
    }
}
