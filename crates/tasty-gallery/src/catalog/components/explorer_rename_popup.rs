//! `explorer_rename_popup` specimen — 디자인 T11 explorer "Rename" 팝업
//! (design §3.5).
//!
//! 본체는 `rename` popup 의 한 target(`RenameTarget::ExplorerEntry`)으로 연다 —
//! `OpenPopupMode::WithScope`(스코프 중앙)라 SCOPE RULE(ADR-0254) 상 modal 그림자다.
//!
//! Add-favorite 팝업과 동일 골격(title / path caption / input / Cancel·primary
//! footer) — 타이틀·초기값(현재 파일명)·primary 라벨만 다르다. 완전성을 위해
//! 별도 specimen 으로 등재(design §3.5). 공유 frame 키트(`widgets::dialog`) 재사용.
//!
//! 기존 Overlays `rename` specimen(workspace/tab rename)과는 별개 — 이쪽은
//! explorer 파일/폴더 이름 변경용으로, path caption 을 동반한다.
//!
//! i18n 키 후보(본체): `explorer.popup.rename.title/path/rename`, `common.cancel`.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let width = theme.measure_sm; // ≈300 (narrow column, design w≈280)
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card(ui, theme, width, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                kit::title(ui, theme, "Rename");
                kit::caption(ui, theme, "Path: ~/Downloads/photo.png", false);
                // 초기값 = 현재 파일명(확장자 포함). gallery 는 정적 표시.
                kit::field(ui, theme, None, "photo.png", false, false);
                ui.add_space(theme.spacing_xs.value());
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                        Button::new("Rename")
                            .variant(ButtonVariant::Primary)
                            .show(ui, theme);
                        Button::new("Cancel")
                            .variant(ButtonVariant::Ghost)
                            .show(ui, theme);
                    });
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "≈280 · bg-panel · 1px border-strong"),
            ("title", "heading semibold"),
            ("path", "caption · text-muted"),
            ("input", "28 · surface-raised · seeded current name"),
            ("footer", "right — Cancel · Rename (primary)"),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new("surface-raised", "input", theme.surface_raised().to_egui()),
            TokenChip::new(
                "accent-primary",
                "Rename button",
                theme.accent_primary().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Shares the Add-to-favorites skeleton — title, seeded value (current file name, \
         extension included), and primary label differ. Anchored to the target row.",
    );
}
