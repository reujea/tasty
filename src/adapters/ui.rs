pub(crate) mod brand;
pub(crate) mod divider;
mod draw;
mod egui_panels;
pub(crate) mod icons;
mod sidebar;
pub(crate) mod status_bar;
pub(crate) mod switch_overlay;
pub(crate) mod tab_bar;
pub(crate) mod titlebar;

pub(crate) mod attach_loading;
pub(crate) mod attach_notice;
pub(crate) mod attach_size_sync;
pub mod banner;
pub(crate) mod category_actions;
pub(crate) mod dialog;
pub(crate) mod drop_overlay;
pub(crate) mod file_open_tool;
pub mod font_judge;
pub mod font_registry;
pub mod fullscreen;
pub(crate) mod info_modal;
pub mod layout_context;
pub(crate) mod modifier_hint_overlay;
pub(crate) mod mouse_capture_menu;
pub(crate) mod move_source;
pub(crate) mod new_tab_picker;
pub(crate) mod notification;
pub(crate) mod overlay;
pub mod popup;
pub mod preset;
pub(crate) mod restore_notice;
pub(crate) mod search_bar;
pub mod surface;
pub mod terminal_link;
pub mod toast;
pub(crate) mod tools_menu;
pub(crate) mod tutorial;

pub mod input;

pub use banner::{
    BannerManager, BannerScope, BannerState, PluginBannerCloseKind, PluginBannerMeshSlot,
};
pub use divider::{draw_pane_dividers, draw_surface_highlights};
pub use draw::draw_ui;
pub(crate) use egui_panels::apply_explorer_action;
pub use egui_panels::draw_egui_panels;
pub use layout_context::LayoutContext;
pub use popup::{PopupAction, PopupManager};
pub use status_bar::{draw_status_bar, status_bar_bottom_inset};
pub use tab_bar::draw_pane_tab_bars;
pub use toast::{ToastKind, ToastManager, ToastScope};

/// 번역이 없으면 빈 라벨 대신 키를 보여준다. 도구 메뉴와 명령 팔레트가 함께 쓴다.
pub(crate) fn label_or_raw_key(key: &str) -> String {
    let translated = tasty_i18n::t(key);
    if translated == key {
        key.to_string()
    } else {
        translated.to_string()
    }
}

/// Theme에서 아직 배율을 적용하지 않은 고정 치수를 UI 배율로 조정한다.
/// Theme.zoomed와 같은 반올림을 쓰며 토큰이 생기면 해당 Theme 접근자로 옮긴다.
#[inline]
pub(crate) fn zoomed_px(
    theme: &tasty_type_appearance::theme::Theme,
    px: tasty_type_geometry::length::LogicalPx,
) -> tasty_type_geometry::length::LogicalPx {
    tasty_type_geometry::length::LogicalPx((px.value() * theme.ui_zoom).round())
}

/// 일반 프레임과 별도로 전체화면 무대만 그린다. 이 프레임에는 호스트 UI나 팝업을 그리지 않는다.
pub fn draw_fullscreen_stage(
    ctx: &egui::Context,
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    fullscreen::draw_fullscreen_stage(ctx, state, engine);
}

/// 팝업 뒤에 오버레이를 그린다. 같은 프레임의 플러그인 팝업도 소속 범위를 공유하도록 LayoutContext를 반환한다.
pub fn draw_popups(
    ctx: &egui::Context,
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pane_rects: &[(u32, crate::model::PhysicalRect)],
    terminal_rect: crate::model::PhysicalRect,
    scale_factor: f32,
) -> LayoutContext {
    let draw_ctx = layout_context::build_layout_context(
        state,
        engine,
        pane_rects,
        terminal_rect,
        scale_factor,
    );

    // 닫힌 다음 프레임에는 무대 그리기가 실행되지 않아 일반 경로에서도 닫기 훅을 처리한다.
    fullscreen::drain_on_close_hooks(ctx, state, engine);

    popup::frame::draw_popup_layer(ctx, state, engine, &draw_ctx);
    overlay::draw_overlays(ctx, state, engine, &draw_ctx, terminal_rect, scale_factor);
    draw_ctx
}

/// PhysicalRect의 변환을 사용해 egui 논리 좌표를 만든다.
/// egui API가 f32를 받는 이 경계에서만 길이 타입의 값을 꺼낸다.
pub(crate) fn to_egui_rect(rect: crate::model::PhysicalRect, scale_factor: f32) -> egui::Rect {
    let logical = rect.to_logical(scale_factor);
    egui::Rect::from_min_size(
        egui::pos2(logical.x.value(), logical.y.value()),
        egui::vec2(logical.width.value(), logical.height.value()),
    )
}
