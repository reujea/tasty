//! 카테고리 삭제 확인. 워크스페이스는 순서를 유지한 채 기본 카테고리로 옮긴다.
//! 대상이 없거나 기본 카테고리이면 닫는다.

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::{t, t_count};
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;

pub const CONFIRM_DELETE_CATEGORY_POPUP_ID: &str = "confirm_delete_category";

/// 다이얼로그 폭 (디자인 380px).
const WIDTH: LogicalPx = LogicalPx(380.0);

/// 대상 카테고리 스냅샷(이름 + 소속 워크스페이스 수).
struct Target {
    name: String,
    count: usize,
}

/// `pending_category_delete` 의 대상 카테고리를 해석. 없거나 normal 이면 None(닫힘).
fn resolve_target(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<Target> {
    let cat_id = state.dialogs.pending_category_delete?;
    let cat = engine.categories().iter().find(|c| c.id == cat_id)?;
    if cat.is_normal() {
        return None;
    }
    Some(Target {
        name: cat.name.clone(),
        count: engine.workspaces_in_category(cat_id).len(),
    })
}

/// PopupDef.title_fn — headless 라 실제 타이틀바는 없지만, 접근성/디버그용 라벨.
pub fn confirm_delete_category_title(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> String {
    t("workspace_category.delete_confirm_title").to_string()
}

/// 대상 이름을 뺀 안내문 자체의 길이. 대상이 아직 안 잡힌 상태의 기준이기도 하다.
const BASE_BODY_LEN: usize = 60;

/// 첫 프레임의 크기가 어긋나지 않도록 등록 크기와 sizer가 같은 계산을 쓴다.
fn size_for(body_len: usize) -> egui::Vec2 {
    let approx_lines = (body_len as f32 / 42.0).ceil().max(2.0);
    let body_h = approx_lines * theme::theme().font_size_body.value() * 1.5;
    let content_h = 24.0 + body_h + 40.0;
    egui::vec2(
        WIDTH.value(),
        (popup::content_margin().scaled(2.0) + LogicalPx(content_h)).value(),
    )
}

/// PopupDef.decl.size — 등록 시점의 placeholder. 대상이 아직 없을 때 sizer 가 내는
/// 값과 **같은 식에서** 나온다.
pub fn confirm_delete_category_default_size() -> egui::Vec2 {
    size_for(BASE_BODY_LEN)
}

/// PopupDef.sizer — 본문 길이에 따라 height 조정(소형 모달).
pub fn confirm_delete_category_sizer(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let body_len = resolve_target(state, engine)
        .map(|tgt| tgt.name.chars().count() + BASE_BODY_LEN)
        .unwrap_or(BASE_BODY_LEN);
    size_for(body_len)
}

/// PopupDef::on_close entry point — 어떤 경로로 닫히든(취소/외부/Escape) 삭제 대상을 비운다.
pub fn on_close_confirm_delete_category(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    state.dialogs.pending_category_delete = None;
}

pub fn draw_confirm_delete_category(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let ctx = ui.ctx().clone();
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.dialogs.pending_category_delete = None;
        return PopupAction::Close;
    }
    let Some(target) = resolve_target(state, engine) else {
        state.dialogs.pending_category_delete = None;
        return PopupAction::Close;
    };
    let cat_id = state
        .dialogs
        .pending_category_delete
        .expect("resolved above");
    let th = theme::theme();

    let margin = th.spacing_sm.value();
    let available = ui.available_rect_before_wrap();
    let inner_rect = available.shrink2(egui::vec2(margin, th.spacing_xs.value()));
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));
    let ui = &mut child_ui;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let icon_size = th.icon_glyph_size_md.value();
        let (icon_rect, _) =
            ui.allocate_exact_size(egui::vec2(icon_size, icon_size), egui::Sense::hover());
        icons::TRASH
            .image(icon_size, th.accent_danger().into())
            .paint_at(ui, icon_rect);
        ui.label(
            egui::RichText::new(t("workspace_category.delete_confirm_title"))
                .color(th.text_primary())
                .size(th.font_size_body.value())
                .strong(),
        );
    });

    ui.add_space(th.spacing_sm.value());

    ui.label(
        egui::RichText::new(t_count(
            "workspace_category.delete_confirm_body",
            target.count as u64,
            &[&target.name, &target.count.to_string()],
        ))
        .color(th.text_secondary())
        .size(th.font_size_body.value()),
    );

    let mut confirm = false;
    let mut cancel = false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
        ui.add_space(th.spacing_sm.value());
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let del = egui::Button::new(
                    egui::RichText::new(t("workspace_category.delete_category"))
                        .color(th.text_on_accent()),
                )
                .fill(th.accent_danger());
                if ui.add(del).clicked() {
                    confirm = true;
                }
                if ui.button(t("button.cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if cancel {
        state.dialogs.pending_category_delete = None;
        return PopupAction::Close;
    }
    if confirm {
        state.dispatch_intent(
            crate::app::command::DomainIntent::DeleteCategory { id: cat_id }
                .from_user_menu("confirm_delete_category"),
        );
        state.dialogs.pending_category_delete = None;
        return PopupAction::Close;
    }
    PopupAction::None
}
