//! 상태 화면 Spec 에 놓는 즐겨찾기 추가·이름 변경 팝업 두 장. 시안 Spec 무대의 팝업 구조와
//! 값을 그대로 옮긴다. 본체를 따르는 단독 팝업 예제(`explorer_favorite_popup` ·
//! `explorer_rename_popup`)와는 구성이 달라 따로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, kbd};

use crate::catalog::widgets::dialog as kit;

/// 시안 팝업 본문 좌우 여백(`padding: 12px 14px 10px` · footer `0 14px 12px`). 대응 토큰이 없다.
const PAD_X: LogicalPx = LogicalPx(14.0);
/// 시안 팝업 본문 아래 여백(`padding: 12px 14px 10px`). 대응 토큰이 없다.
const BODY_PAD_BOTTOM: LogicalPx = LogicalPx(10.0);
/// 시안 블록 아래 간격(`marginBottom: 10`) — 즐겨찾기 제목·경로 값, 이름 변경 안내 줄 아래.
const BLOCK_GAP: LogicalPx = LogicalPx(10.0);
/// 시안 이름 변경 제목 아래 간격(`marginBottom: 2`). 대응 토큰이 없다.
const RENAME_TITLE_GAP: LogicalPx = LogicalPx(2.0);
/// 시안 확장자 줄 위 간격(`marginTop: 6`). 대응 토큰이 없다.
const EXTENSION_GAP: LogicalPx = LogicalPx(6.0);
/// 시안 경로 값과 이름 변경 안내 줄의 글자 크기(`fontSize: 12`). 대응 UI 토큰이 없다.
const LINE_FONT_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 즐겨찾기 추가 팝업 — Path 라벨과 mono 값, Name 라벨과 입력, sm 버튼 footer.
pub(super) fn favorite(ui: &mut egui::Ui, theme: &Theme) {
    shell(ui, theme, "Add", |ui| {
        kit::title(ui, theme, "Add to favorites");
        ui.add_space(BLOCK_GAP.value());
        label(ui, theme, "Path");
        ui.add_space(theme.spacing_xs.value());
        ui.label(
            egui::RichText::new("~/Downloads")
                .font(egui::FontId::monospace(LINE_FONT_PRIMITIVE_12.value()))
                .color(theme.text_secondary().to_egui()),
        );
        ui.add_space(BLOCK_GAP.value());
        label(ui, theme, "Name");
        ui.add_space(theme.spacing_xs.value());
        kit::field(ui, theme, None, "Downloads", false, false);
    });
}

/// 이름 변경 팝업 — Kbd 안내 줄, 입력, 확장자 보존 줄, sm 버튼 footer.
pub(super) fn rename(ui: &mut egui::Ui, theme: &Theme) {
    shell(ui, theme, "Rename", |ui| {
        kit::title(ui, theme, "Rename");
        ui.add_space(RENAME_TITLE_GAP.value());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let hint = |ui: &mut egui::Ui, text: &str| {
                ui.label(
                    egui::RichText::new(text)
                        .size(LINE_FONT_PRIMITIVE_12.value())
                        .color(theme.text_muted().to_egui()),
                );
            };
            hint(ui, "Press ");
            kbd(ui, theme, "↵", tasty_ui_widgets::Ground::Raised);
            hint(ui, " to confirm, ");
            kbd(ui, theme, "Esc", tasty_ui_widgets::Ground::Raised);
            hint(ui, " to cancel.");
        });
        ui.add_space(BLOCK_GAP.value());
        kit::field(ui, theme, None, "diagram.png", false, false);
        ui.add_space(EXTENSION_GAP.value());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            label(ui, theme, "Extension ");
            ui.label(
                egui::RichText::new(".png")
                    .size(theme.font_size_caption.value())
                    .strong()
                    .color(theme.text_secondary().to_egui()),
            );
            label(ui, theme, " preserved.");
        });
    });
}

/// 11px text-muted 라벨.
fn label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 300px 팝업 껍데기 — 본문 여백 12/14/10, footer 오른쪽 정렬 Ghost·Primary sm 버튼.
/// 시안은 border-box 라 폭 300 에 테두리가 들어가므로 안쪽 폭에서 테두리 두 줄을 뺀다.
fn shell(ui: &mut egui::Ui, theme: &Theme, primary: &str, body: impl FnOnce(&mut egui::Ui)) {
    let width = theme.measure_sm - theme.border_width * 2.0;
    kit::frame_card(ui, theme, width, kit::dialog_fill(theme), |ui| {
        let body_margin = egui::Margin {
            left: PAD_X.value() as i8,
            right: PAD_X.value() as i8,
            top: theme.spacing_md.value() as i8,
            bottom: BODY_PAD_BOTTOM.value() as i8,
        };
        kit::region(ui, body_margin, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            body(ui);
        });
        let footer_margin = egui::Margin {
            left: PAD_X.value() as i8,
            right: PAD_X.value() as i8,
            top: 0,
            bottom: theme.spacing_md.value() as i8,
        };
        kit::region(ui, footer_margin, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    Button::new(primary)
                        .variant(ButtonVariant::Primary)
                        .size(ControlSize::Sm)
                        .show(ui, theme);
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, theme);
                });
            });
        });
    });
}
