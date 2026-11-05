//! 접힌 사이드바의 카테고리 버튼 오른쪽에 여는 메뉴.
//! 머리줄은 이름과 오른쪽 끝의 워크스페이스 수(mono · micro · text-muted)이고 둘의 baseline 을 맞춘다(시안 RailCategoryFrame).
//! 시안 이름의 semibold 는 egui UI 에 굵은 글꼴을 등록하지 않아 재현하지 않는다(`popup_title_font` 와 같다).
//! 이름 아래에 워크스페이스 추가와 접기·펼치기를 표시한다.
//! 기본 카테고리가 아니면 이름 변경·삭제도 제공하며 대상이 없으면 닫는다.

use crate::adapters::ui::category_actions;
use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::intent::Intent;
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;

pub const RAIL_CATEGORY_POPUP_ID: &str = "rail_category";

/// 팝업 폭 (디자인 minWidth 176).
const POPUP_WIDTH: LogicalPx = LogicalPx(176.0);
/// 헤더(이름) 영역 높이 — 라벨 + 상하 패딩 + 하단 보더.
const HEADER_HEIGHT: LogicalPx = LogicalPx(30.0);

/// 현재 대상 카테고리가 접혀 있는지 등 팝업 렌더에 필요한 스냅샷.
struct Target {
    label: String,
    /// 이 카테고리에 속한 워크스페이스 수.
    count: usize,
    collapsed: bool,
    /// 기본 카테고리이면 이름 변경·삭제를 표시하지 않는다.
    is_reserved: bool,
}

/// `state.dialogs.rail_category_popup` 의 대상 카테고리를 engine 에서 해석.
fn resolve_target(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<Target> {
    let cat_id = state.dialogs.rail_category_popup?;
    let cat = engine.categories().iter().find(|c| c.id == cat_id)?;
    let label = if cat.is_normal() {
        t("sidebar.workspaces_heading").to_string()
    } else {
        cat.name.clone()
    };
    Some(Target {
        label,
        count: engine.workspaces_in_category(cat.id).len(),
        collapsed: state.navigation.collapsed_categories.contains(&cat.id),
        is_reserved: cat.is_normal(),
    })
}

/// 아이콘 + 라벨 메뉴 행 1개. hover 시 overlay-hover 배경. `danger` 면 라벨/아이콘을
/// accent-danger 로 그린다(삭제). 클릭 여부 반환.
fn menu_row(
    ui: &mut egui::Ui,
    th: &crate::theme::Theme,
    icon: icons::Icon,
    label: &str,
    danger: bool,
) -> bool {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, th.item_height_interactive.value()),
        egui::Sense::click(),
    );
    let radius = th.corner_radius.value();
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, th.hover_overlay.to_egui_premultiplied());
    }
    let color: egui::Color32 = match (danger, resp.hovered()) {
        (true, true) => th.accent_danger_strong().into(),
        (true, false) => th.accent_danger_raised().into(),
        (false, true) => th.text_on_active().into(),
        (false, false) => th.text_muted_raised().into(),
    };
    let icon_size = th.icon_glyph_size_md.value();
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.min.x + th.spacing_sm.value(),
            rect.center().y - icon_size / 2.0,
        ),
        egui::vec2(icon_size, icon_size),
    );
    icon.image(icon_size, color).paint_at(ui, icon_rect);
    ui.painter().text(
        egui::pos2(icon_rect.max.x + th.spacing_sm.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(th.font_size_body.value()),
        color,
    );
    resp.clicked()
}

/// galley 첫 줄의 baseline(galley 위쪽 기준 y). 글리프가 없으면 galley 높이.
fn first_baseline(g: &egui::Galley) -> f32 {
    g.rows
        .first()
        .and_then(|r| r.glyphs.first())
        .map_or(g.rect.height(), |gl| gl.pos.y)
}

/// PopupDef::on_close entry point — 어떤 경로로 닫히든 대상 카테고리 참조를 비운다.
pub fn on_close_rail_category_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.rail_category_popup = None;
}

pub fn draw_rail_category_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;
    }
    let Some(target) = resolve_target(state, engine) else {
        return PopupAction::Close;
    };
    let cat_id = state.dialogs.rail_category_popup.expect("resolved above");
    let th = theme::theme();
    // 시안 RailCategoryPopup 의 행은 간격 없이 붙는다. 간격은 아래에서 add_space 로만 둔다.
    ui.spacing_mut().item_spacing.y = 0.0;

    let width = ui.available_width();
    let (header_rect, _) = ui.allocate_exact_size(
        egui::vec2(width, HEADER_HEIGHT.value()),
        egui::Sense::hover(),
    );
    // 이름은 머리줄 세로 가운데, 수는 이름과 첫 줄 baseline 을 맞춘다(시안 alignItems: baseline).
    // 이름은 시안 gap(space-sm)을 두고 수 앞에서 자른다.
    let name = ui.painter().layout_no_wrap(
        target.label.clone(),
        egui::FontId::proportional(th.font_size_body.value()),
        th.text_primary().into(),
    );
    let count = ui.painter().layout_no_wrap(
        target.count.to_string(),
        egui::FontId::monospace(th.font_size_micro.value()),
        th.text_muted().into(),
    );
    let name_pos = egui::pos2(
        header_rect.min.x + th.spacing_sm.value(),
        header_rect.center().y - name.rect.height() * 0.5,
    );
    let count_pos = egui::pos2(
        header_rect.max.x - th.spacing_sm.value() - count.rect.width(),
        name_pos.y + first_baseline(&name) - first_baseline(&count),
    );
    let mut label_clip = header_rect;
    label_clip.max.x = count_pos.x - th.spacing_sm.value();
    ui.painter()
        .galley(count_pos, count, th.text_muted().into());
    ui.painter()
        .with_clip_rect(label_clip)
        .galley(name_pos, name, th.text_primary().into());
    let border = egui::Rect::from_min_size(
        egui::pos2(
            header_rect.min.x,
            header_rect.max.y - th.border_width.value(),
        ),
        egui::vec2(width, th.border_width.value()),
    );
    ui.painter()
        .rect_filled(border, 0.0, th.separator.to_egui_premultiplied());
    ui.add_space(th.spacing_xs.value());

    if menu_row(
        ui,
        &th,
        icons::PLUS,
        t("workspace_category.add_workspace"),
        false,
    ) {
        state.dispatch_intent(
            Intent::NewWorkspace {
                kind: None,
                params: serde_json::Value::Null,
                category: Some(cat_id),
            }
            .from_user_menu("rail_category/add_workspace"),
        );
        return PopupAction::Close;
    }

    let (collapse_icon, collapse_label) = if target.collapsed {
        (icons::CHEVRON_RIGHT, t("workspace_category.expand"))
    } else {
        (icons::CHEVRON_DOWN, t("workspace_category.collapse"))
    };
    if menu_row(ui, &th, collapse_icon, collapse_label, false) {
        state.dispatch_intent(
            crate::intent::UiIntent::ToggleCategoryCollapsed { id: cat_id }
                .from_user_menu("rail_category/toggle_collapsed"),
        );
        return PopupAction::Close;
    }

    if !target.is_reserved {
        let width = ui.available_width();
        ui.add_space(th.spacing_xs.value());
        let (sep_rect, _) = ui.allocate_exact_size(
            egui::vec2(width, th.border_width.value()),
            egui::Sense::hover(),
        );
        // 메뉴 구분선 — 모든 메뉴와 같은 menu-separator 색.
        ui.painter()
            .rect_filled(sep_rect, 0.0, th.menu_separator().to_egui());
        ui.add_space(th.spacing_xs.value());

        if menu_row(
            ui,
            &th,
            icons::EDIT,
            t("workspace_category.rename_category"),
            false,
        ) {
            category_actions::open_rename_category_dialog(state, engine, cat_id);
            return PopupAction::Close;
        }
        if menu_row(
            ui,
            &th,
            icons::TRASH,
            t("workspace_category.delete_category"),
            true,
        ) {
            category_actions::open_delete_category_confirm(state, cat_id);
            return PopupAction::Close;
        }
    }

    PopupAction::None
}

/// PopupDef.sizer — 헤더 + 행 수로 height 계산. normal 은 Add/Collapse 2행, 비-normal 은
/// separator + Rename/Delete 를 더한 4행. 그리는 순서와 같게 머리줄 아래 space-xs, 붙은 행들,
/// 구분선 위아래 space-xs 를 더한다.
pub fn rail_category_sizer(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let th = theme::theme();
    let reserved = resolve_target(state, engine)
        .map(|t| t.is_reserved)
        .unwrap_or(true);
    let rows = if reserved { 2u32 } else { 4u32 };
    let mut content_h =
        HEADER_HEIGHT + th.spacing_xs + th.item_height_interactive.scaled(rows as f32);
    if !reserved {
        content_h += th.border_width + th.spacing_xs.scaled(2.0);
    }
    // 1은 반올림 오차 여유이며 디자인 치수 토큰이 아니다.
    egui::vec2(
        POPUP_WIDTH.value(),
        (popup::content_margin().scaled(2.0) + content_h + LogicalPx(1.0)).value(),
    )
}

/// PopupDef.default_size — register 시점 placeholder. sizer 가 매 프레임 재계산.
pub fn rail_category_default_size() -> egui::Vec2 {
    let th = theme::theme();
    egui::vec2(
        POPUP_WIDTH.value(),
        (popup::content_margin().scaled(2.0)
            + HEADER_HEIGHT
            + th.item_height_interactive.scaled(2.0))
        .value(),
    )
}
