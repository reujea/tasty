//! 마크다운 플러그인의 대용량 파일 확인 팝업 예제.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, TagVariant, tag_mono};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const WIDTH: LogicalPx = LogicalPx(360.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| popup_card(ui, theme));

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "360px · bg-panel"),
            ("path", "mono caption · muted · ellipsis"),
            ("size", "Tag warning · accent-warning"),
            ("note", "caption · text-secondary"),
            ("footer", "Cancel (ghost) · Open (primary)"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "size tag",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new("text-secondary", "note", theme.text_secondary().to_egui()),
            TokenChip::new("bg-panel", "frame fill", theme.bg_panel().to_egui()),
        ],
    );
}

/// 확인 팝업 카드. 갤러리의 타일 배치 예제에서도 같은 카드를 쓴다.
pub(crate) fn popup_card(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
        kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            kit::title(ui, theme, "Open large file?");
            kit::caption(ui, theme, ".../docs/big-notes.md", true);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                tag_mono(ui, theme, "3.2 MB", TagVariant::Warning, false);
                ui.label(
                    egui::RichText::new("Over 1\u{a0}MiB — rendering may be slow.")
                        .size(theme.font_size_caption.value())
                        .color(theme.text_secondary().to_egui()),
                );
            });
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Open")
                        .variant(ButtonVariant::Primary)
                        .show(ui, theme);
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                });
            });
        });
    });
}
