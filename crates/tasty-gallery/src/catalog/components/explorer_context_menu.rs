//! `explorer_context_menu` specimen — 디자인 T11 우클릭 컨텍스트 메뉴 4 variant
//! (design §3.3).
//!
//! 기존 `menu_item()` / `menu_separator()` 재사용 — 메뉴 컨테이너만 조립한다.
//! 타겟별 항목 구성 4종(빈 영역 / 파일 / 폴더 / 다중선택)을 2×2 로 전시.
//! 컨테이너 = surface-raised + 1px border-strong + radius. Delete 는 Danger variant.
//!
//! i18n 키 후보(본체): `explorer.menu.copy_path` / `copy_path_multi` /
//! `add_to_favorites` / `copy` / `cut` / `paste` / `paste_into` / `delete` /
//! `rename` / `open_in_system`.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{MenuItemVariant, menu_item, menu_separator};

use crate::catalog::icons::{COPY, EDIT, MockGlyph, STAR, TRASH};
use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

/// 메뉴 한 줄.
enum Mi {
    /// (leading glyph, label, danger 여부)
    Item(Option<MockGlyph>, &'static str, bool),
    Sep,
}

fn empty_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(STAR), "Add to favorites", false),
        Mi::Sep,
        Mi::Item(None, "Paste", false),
    ]
}

fn file_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Item(Some(EDIT), "Rename", false),
    ]
}

fn folder_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path", false),
        Mi::Item(Some(STAR), "Add to favorites", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Item(None, "Paste (into)", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
        Mi::Item(Some(EDIT), "Rename", false),
        Mi::Item(None, "Open in system", false),
    ]
}

fn multi_menu() -> Vec<Mi> {
    vec![
        Mi::Item(Some(COPY), "Copy path (newline-sep)", false),
        Mi::Item(Some(COPY), "Copy", false),
        Mi::Item(None, "Cut", false),
        Mi::Sep,
        Mi::Item(Some(TRASH), "Delete", true),
    ]
}

/// (caption, 항목 빌더) — 한 컨텍스트 메뉴 variant.
type Variant = (&'static str, fn() -> Vec<Mi>);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let menu_w = theme.field_width_lg.value() + theme.spacing_xl.value(); // ≈224
    let variants: [Variant; 4] = [
        ("empty area → cwd", empty_menu),
        ("file (single)", file_menu),
        ("folder (single)", folder_menu),
        ("multi-select", multi_menu),
    ];

    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_xl.value(), theme.spacing_lg.value());
        for (caption, build) in variants {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.label(
                    egui::RichText::new(caption.to_uppercase())
                        .size(theme.font_size_micro.value())
                        .color(egui::Color32::from(theme.text_muted())),
                );
                render_menu(ui, theme, menu_w, &build());
            });
        }
    });

    meta(
        ui,
        theme,
        &[
            ("container", "surface-raised · 1px border-strong"),
            ("item", "menu_item (28 control-height)"),
            ("separator", "menu_separator (1px)"),
            ("danger", "Delete = accent-danger label"),
            ("targets", "empty · file · folder · multi"),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "menu fill",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::new(
                "border-strong",
                "menu edge",
                egui::Color32::from(theme.border_strong()),
            ),
            TokenChip::new(
                "accent-danger",
                "delete label",
                egui::Color32::from(theme.accent_danger()),
            ),
        ],
    );

    note(
        ui,
        theme,
        "Target rule (body): a right-clicked item inside the selection acts on the whole \
         selection; outside it, the selection resets to that item; on the background, the \
         cwd. Add-to-favorites shows only on a single folder or the background; Open in \
         system only on a folder.",
    );
}

fn render_menu(ui: &mut egui::Ui, theme: &Theme, width: f32, items: &[Mi]) {
    egui::Frame::new()
        .fill(egui::Color32::from(theme.surface_raised()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_strong()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            for it in items {
                match it {
                    Mi::Sep => menu_separator(ui, theme),
                    Mi::Item(glyph, label, danger) => {
                        let variant = if *danger {
                            MenuItemVariant::Danger
                        } else {
                            MenuItemVariant::Normal
                        };
                        match glyph {
                            Some(g) => {
                                let g = *g;
                                menu_item(
                                    ui,
                                    theme,
                                    Some(&|ui, rect, c| {
                                        g.image(rect.height(), c).paint_at(ui, rect)
                                    }),
                                    label,
                                    None,
                                    variant,
                                    false,
                                    true,
                                );
                            }
                            None => {
                                menu_item(ui, theme, None, label, None, variant, false, true);
                            }
                        }
                    }
                }
            }
        });
}
