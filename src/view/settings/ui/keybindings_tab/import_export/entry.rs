//! 내보내기와 가져오기 버튼이 있는 진입 화면.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use crate::adapters::ui::icons;

use super::paint::glyph_at;

/// 아이콘·제목·설명·버튼과 해당 작업의 실패 안내를 표시한다.
/// 실패 안내의 재시도를 사용하는 동안 기존 버튼은 비활성화한다.
// reason: jsx `IeActionRow` 의 prop(glyph · title · desc · action · notice)을 그대로 받는다 —
// 묶으면 두 호출부가 같은 구조체를 채우는 코드만 늘어난다.
#[allow(clippy::too_many_arguments)]
pub(super) fn action_row(
    ui: &mut egui::Ui,
    th: &Theme,
    glyph: icons::Icon,
    title: &str,
    desc: &str,
    button: &str,
    variant: ButtonVariant,
    enabled: bool,
    notice: Option<&mut dyn FnMut(&mut egui::Ui)>,
) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(th.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_default().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(tasty_ui_widgets::margin_all(th.spacing_md))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_md.value();
                glyph_at(
                    ui,
                    glyph,
                    th.icon_glyph_size_md,
                    th.text_muted_raised().to_egui(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    clicked = Button::new(button)
                        .variant(variant)
                        .size(ControlSize::Sm)
                        .enabled(enabled)
                        .show(ui, th)
                        .clicked();
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y =
                            tasty_ui_widgets::tokens::STRUCT_GAP_2.value();
                        ui.label(
                            egui::RichText::new(title)
                                .size(th.font_size_body.value())
                                .color(th.text_primary()),
                        );
                        ui.scope(|ui| {
                            ui.set_max_width(th.measure_md.value());
                            ui.label(
                                egui::RichText::new(desc)
                                    .size(th.font_size_term_sm.value())
                                    .color(th.text_muted_raised()),
                            );
                        });
                    });
                });
            });
            if let Some(notice) = notice {
                notice(ui);
            }
        });
    clicked
}
