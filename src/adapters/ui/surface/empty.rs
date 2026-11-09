//! EmptySurface의 egui 렌더링. 중앙에 "Convert surface" 버튼 하나.

use crate::model::EmptySurface;
use crate::theme;

/// Action bubbled up from the empty surface UI.
pub enum EmptyAction {
    /// User clicked the convert button. Carries the surface id to open the popup for.
    OpenConvertPopup(u32),
}

/// Draw the EmptySurface body. Returns an EmptyAction if the user interacted.
/// `bed_focused` picks the bed value that shows focus across the whole surface.
pub fn draw_empty(
    ui: &mut egui::Ui,
    empty: &EmptySurface,
    bed_focused: bool,
) -> Option<EmptyAction> {
    let th = theme::theme();
    let panel_rect = ui.max_rect();
    ui.painter().rect_filled(
        panel_rect,
        0.0,
        tasty_ui_widgets::surface_bed(&th, bed_focused),
    );

    let available = ui.available_size();
    let button_h = 28.0;
    ui.add_space(((available.y - button_h) / 2.0).max(0.0));

    let mut action = None;
    ui.vertical_centered(|ui| {
        let btn = ui.button(
            egui::RichText::new(crate::i18n::t("convert_popup.title"))
                .size(th.font_size_body.value())
                .color(th.text_primary()),
        );
        if btn.clicked() {
            action = Some(EmptyAction::OpenConvertPopup(empty.id));
        }
    });
    action
}
