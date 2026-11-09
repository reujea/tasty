//! 원격 프로필·passkey 추가 화면. 원격 연결 창 안의 경로이며 [112 · 1fr] 행 격자를 공유한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, select};

use super::{FORM_WIDTH, LABEL_COL, attach_header, form_row, seg_chip, tab_bar};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// Type 행의 형식 선택 칸 폭.
const TYPE_SELECT_W: LogicalPx = LogicalPx(64.0);
/// Port 입력 칸 폭.
const PORT_FIELD_W: LogicalPx = LogicalPx(96.0);
/// inline secret 입력은 세 줄 높이다.
const SECRET_ROWS: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Ssh,
    SshHostError,
    Generic,
    PasskeyPath,
    PasskeyInline,
}

impl Variant {
    fn is_passkey(self) -> bool {
        matches!(self, Variant::PasskeyPath | Variant::PasskeyInline)
    }
}

fn hint(ui: &mut egui::Ui, theme: &Theme, text: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.add_space((LABEL_COL + theme.spacing_md).value());
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme.font_size_caption.value())
                    .color(color),
            )
            .wrap(),
        );
    });
}

/// 등록되지 않은 형식을 알리는 경고 테두리 배지. 저장은 막지 않는다.
fn warning_badge(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let warn = theme.accent_warning().to_egui();
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        tasty_ui_widgets::label_font(theme.font_size_micro.value()),
        warn,
    );
    let h = theme.badge_size().value();
    let w = galley.rect.width() + theme.badge_padding_x().value() * 2.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = theme.corner_radius_sm.value();
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), warn),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.rect.size() * 0.5, galley, warn);
}

fn section_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(theme.spacing_xs.value());
    ui.label(
        egui::RichText::new(text)
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
}

fn full_select(ui: &mut egui::Ui, theme: &Theme, salt: &str, items: &[&str]) {
    let mut sel = 0usize;
    select(ui, theme, salt, &mut sel, items, ui.available_width(), true);
}

/// 세 줄 secret 입력 칸. 비어 있는 상태라 placeholder만 보인다.
fn secret_area(ui: &mut egui::Ui, theme: &Theme) {
    let line = theme.font_size_term_sm.value() * theme.line_height_ui;
    let h = line * f32::from(SECRET_ROWS) + theme.spacing_sm.value() * 2.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
    let p = ui.painter();
    let radius = theme.corner_radius.value();
    p.rect_filled(rect, radius, theme.input_bg().to_egui());
    p.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.input_border().to_egui()),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.min + egui::vec2(theme.spacing_sm.value(), theme.spacing_sm.value()),
        egui::Align2::LEFT_TOP,
        crate::i18n::t("remote_tool.value_inline_hint"),
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        theme.text_placeholder().to_egui(),
    );
}

fn generic_field_row(ui: &mut egui::Ui, theme: &Theme, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        kit::field(ui, theme, Some(LABEL_COL), key, false, true);
        let value_w =
            ui.available_width() - theme.item_height_interactive.value() - theme.spacing_md.value();
        kit::field(ui, theme, Some(LogicalPx(value_w)), value, false, true);
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            });
    });
}

fn form_card(ui: &mut egui::Ui, theme: &Theme, variant: Variant, width: LogicalPx) {
    kit::frame_card(ui, theme, width, kit::panel_fill(theme), |ui| {
        attach_header(ui, theme);
        tab_bar(ui, theme, if variant.is_passkey() { 2 } else { 0 });

        kit::region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(if variant.is_passkey() {
                    "New passkey"
                } else {
                    "New profile"
                })
                .size(theme.font_size_body.value())
                .strong()
                .color(theme.text_primary().to_egui()),
            );
            match variant {
                Variant::Ssh | Variant::SshHostError | Variant::Generic => {
                    let generic = variant == Variant::Generic;
                    form_row(ui, theme, "Type", |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                        let field_w =
                            ui.available_width() - TYPE_SELECT_W.value() - theme.spacing_xs.value();
                        kit::field(
                            ui,
                            theme,
                            Some(LogicalPx(field_w)),
                            if generic { "smb" } else { "ssh" },
                            false,
                            false,
                        );
                        let mut sel = usize::from(generic);
                        select(
                            ui,
                            theme,
                            "remote_form_type",
                            &mut sel,
                            &["ssh", "smb", "http"],
                            TYPE_SELECT_W.value(),
                            true,
                        );
                    });
                    if generic {
                        ui.horizontal(|ui| {
                            ui.add_space((LABEL_COL + theme.spacing_md).value());
                            warning_badge(ui, theme, "Unknown type");
                        });
                    }
                    form_row(ui, theme, "Name", |ui| {
                        kit::field(
                            ui,
                            theme,
                            None,
                            if generic { "fileshare" } else { "prod-web" },
                            false,
                            false,
                        );
                    });
                    if generic {
                        section_label(ui, theme, "FIELDS");
                        generic_field_row(ui, theme, "host", "fs.local");
                        generic_field_row(ui, theme, "share", "team");
                        Button::new("Add field")
                            .variant(ButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .leading_icon(&|ui, rect, c| {
                                icons::PLUS.image(rect.height(), c).paint_at(ui, rect)
                            })
                            .show(ui, theme);
                        form_row(ui, theme, "Passkey", |ui| {
                            full_select(
                                ui,
                                theme,
                                "remote_form_generic_passkey",
                                &["(none)", "smb-cred"],
                            );
                        });
                    } else {
                        form_row(ui, theme, "Host", |ui| {
                            let host_error = variant == Variant::SshHostError;
                            kit::field(
                                ui,
                                theme,
                                None,
                                if host_error { "" } else { "10.0.0.4" },
                                false,
                                false,
                            );
                        });
                        form_row(ui, theme, "User", |ui| {
                            kit::field(ui, theme, None, "deploy", false, false);
                        });
                        form_row(ui, theme, "Port", |ui| {
                            kit::field(ui, theme, Some(PORT_FIELD_W), "22", false, false);
                        });
                        form_row(ui, theme, "Label", |ui| {
                            kit::field(ui, theme, None, "optional", true, false);
                        });
                        form_row(ui, theme, "Shell", |ui| {
                            full_select(
                                ui,
                                theme,
                                "remote_form_shell",
                                &["auto", "bash", "zsh", "fish"],
                            );
                        });
                        hint(
                            ui,
                            theme,
                            crate::i18n::t("remote_tool.shell_auto_hint"),
                            theme.text_muted().to_egui(),
                        );
                        form_row(ui, theme, "Passkey", |ui| {
                            full_select(
                                ui,
                                theme,
                                "remote_form_ssh_passkey",
                                &["(none)", "id_ed25519", "deploy-key"],
                            );
                        });
                    }
                }
                Variant::PasskeyPath | Variant::PasskeyInline => {
                    let path = variant == Variant::PasskeyPath;
                    form_row(ui, theme, "Name", |ui| {
                        kit::field(ui, theme, None, "id_ed25519", false, false);
                    });
                    form_row(ui, theme, "Kind", |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                        seg_chip(ui, theme, "path", path);
                        seg_chip(ui, theme, "inline", !path);
                    });
                    if path {
                        form_row(ui, theme, "Value", |ui| {
                            kit::field(ui, theme, None, "~/.ssh/id_ed25519", false, true);
                        });
                        hint(
                            ui,
                            theme,
                            "References a key file you own.",
                            theme.text_muted().to_egui(),
                        );
                    } else {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                            ui.allocate_ui_with_layout(
                                egui::vec2(
                                    LABEL_COL.value(),
                                    theme.item_height_interactive.value(),
                                ),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new("Value")
                                            .size(theme.font_size_body.value())
                                            .color(theme.text_muted().to_egui()),
                                    );
                                },
                            );
                            secret_area(ui, theme);
                        });
                        hint(
                            ui,
                            theme,
                            "Pasted secret is materialized to a 0600-managed file.",
                            theme.text_muted().to_egui(),
                        );
                    }
                    hint(
                        ui,
                        theme,
                        crate::i18n::t("remote_tool.passkey_value_note"),
                        theme.text_muted().to_egui(),
                    );
                }
            }
            if variant == Variant::SshHostError {
                ui.add_space(theme.spacing_xs.value());
                ui.label(
                    egui::RichText::new("Host is required.")
                        .size(theme.font_size_caption.value())
                        .color(theme.accent_danger_raised().to_egui()),
                );
            }
        });

        kit::hsep(ui, theme);
        kit::region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    Button::new("Save")
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

/// 카드 두 장이 한 줄에 놓이도록 줄 단위로 나눠 그린다. 세 장을 한 줄에 두면 문서 칸을 넘는다.
fn captioned_forms(ui: &mut egui::Ui, theme: &Theme, rows: &[&[(&str, Variant)]]) {
    // 시안은 width 100% · max-width 460이다. 두 장이 문서 칸에 들어가도록 줄인다.
    // 무대 안쪽 폭은 무대 여백과 테두리를 뺀 값이며 무대 밖에서 잰다.
    let gap = theme.spacing_lg.value();
    let inner =
        ui.available_width() - (theme.spacing_xl.value() + theme.border_width.value()) * 2.0;
    let width = LogicalPx(FORM_WIDTH.value().min((inner - gap) * 0.5));
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (r, cards) in rows.iter().enumerate() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (i, (caption, variant)) in cards.iter().enumerate() {
                    // 같은 폼을 여러 번 그리므로 위젯 ID의 범위를 나눈다.
                    ui.push_id((r, i), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                            kit::caption(ui, theme, caption, false);
                            form_card(ui, theme, *variant, width);
                        });
                    });
                }
            });
        }
    });
}

/// Overlays › Remote connections — SSH 프로필 추가 경로와 검증 오류.
pub fn draw_profile_form(ui: &mut egui::Ui, theme: &Theme) {
    captioned_forms(
        ui,
        theme,
        &[&[
            ("SSH · add mode", Variant::Ssh),
            ("validation error", Variant::SshHostError),
        ]],
    );
    spec::meta(
        ui,
        theme,
        &[
            ("frame", "520 × 460 · resizable · headless"),
            ("row", "[remote-label-col 112 · 1fr] · gap 12/8"),
            ("label", "right-aligned · text-muted · 13"),
            ("body/footer", "flex:1 scroll · pinned footer"),
            ("footer sep", "full-width 1px · buttons inset 16/12"),
            ("error", "accent-danger caption (11)"),
        ],
        &[
            TokenChip::without_color("remote-label-col", "112px label column"),
            TokenChip::new("text-muted", "labels / hints", theme.text_muted().to_egui()),
            TokenChip::without_color("separator", "footer + tab divider"),
            TokenChip::new(
                "accent-primary",
                "active tab · Save",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "validation error",
                theme.accent_danger().to_egui(),
            ),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Do keep the label column fixed at 112px (not 1fr-negotiated) — egui's auto grid collapsed it and truncated labels, so the form uses a manual two-column row.",
    );
}

/// Overlays › Remote connections — 일반 key-value 폼과 passkey 폼.
pub fn draw_generic_passkey_forms(ui: &mut egui::Ui, theme: &Theme) {
    captioned_forms(
        ui,
        theme,
        &[
            &[
                ("generic · unknown type", Variant::Generic),
                ("passkey · kind = path", Variant::PasskeyPath),
            ],
            &[("passkey · kind = inline", Variant::PasskeyInline)],
        ],
    );
    spec::meta(
        ui,
        theme,
        &[
            ("generic", "Type · Name · FIELDS · Passkey"),
            ("field row", "[112 key · 1fr value · 28 ✕]"),
            ("unknown type", "peach badge (saves anyway)"),
            ("passkey kind", "path = singleline · inline = 3-row"),
            ("secret", "name-reference only · note always shown"),
            ("footer", "ghost Cancel / primary Save (both forms)"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "unknown-type / dangling badge",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "selected kind segment (accent fill)",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "input-bg",
                "inline secret field",
                theme.input_bg().to_egui(),
            ),
            TokenChip::without_color("remote-label-col", "shared 112 column"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Other states: a dangling passkey reuses the same peach badge (\"passkey missing\"); detecting shows \"detecting…\" / \"detection failed (disabled)\" + Re-detect after a shell=auto save. Secrets live only in passkeys — a profile holds a name reference, never an inline secret.",
    );
}
