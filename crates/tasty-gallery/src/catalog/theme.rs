//! 배경 단계, 텍스트 위계, 강조색 역할, 터미널 색을 Theme 값으로 비교한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Input, StatusKind, TagVariant, status_dot, tag};

use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

/// 배경색 단계는 그림자 없이 비교한다. 그림자는 팝오버·모달 같은 떠 있는 화면에 사용한다.
pub fn elevation(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.scope(|ui| {
            ui.set_max_width(theme.measure_sm.value());
            ramp(ui, theme, ec(theme.bg_app()), "bg-app", |ui| {
                ramp(ui, theme, ec(theme.bg_sidebar()), "bg-sidebar", |ui| {
                    ramp(ui, theme, ec(theme.bg_panel()), "bg-panel", |ui| {
                        ramp(
                            ui,
                            theme,
                            ec(theme.surface_raised()),
                            "surface-raised",
                            |ui| {
                                tile(ui, theme, ec(theme.surface_hover()), "surface-hover");
                                ui.add_space(theme.spacing_sm.value());
                                tile(ui, theme, ec(theme.surface_active()), "surface-active");
                            },
                        );
                    });
                });
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("ramp", "crust → … → surface2"),
            ("border", "1px border-default"),
            ("shadow", "none — floating surfaces only (popover / modal)"),
            ("stack width", "measure-sm 300"),
        ],
        &[
            TokenChip::new("bg-app", "outermost frame", ec(theme.bg_app())),
            TokenChip::new("bg-sidebar", "nav / sidebar", ec(theme.bg_sidebar())),
            TokenChip::new("bg-panel", "content panel", ec(theme.bg_panel())),
            TokenChip::new(
                "surface-raised",
                "raised control",
                ec(theme.surface_raised()),
            ),
            TokenChip::new("surface-hover", "hover overlay", ec(theme.surface_hover())),
            TokenChip::new(
                "surface-active",
                "active / selected",
                ec(theme.surface_active()),
            ),
            TokenChip::new("border-frame", "frame line", ec(theme.border_frame())),
        ],
    );
    note(
        ui,
        theme,
        "깊이는 그림자가 아니라 surface tint 의 한 단계 차이로만 읽힌다 — 터미널 UI 는 평면이다.",
    );
}

/// 한 단계 ramp 프레임 — fill + 1px border + 토큰 라벨 + 중첩 콘텐츠.
fn ramp(
    ui: &mut egui::Ui,
    theme: &Theme,
    fill: egui::Color32,
    label: &str,
    inner: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.border_default()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(ec(theme.text_muted())),
            );
            ui.add_space(theme.spacing_sm.value());
            inner(ui);
        });
}

/// ramp 안쪽 tile (hover/active 행) — fill + 라벨, 풀폭.
fn tile(ui: &mut egui::Ui, theme: &Theme, fill: egui::Color32, label: &str) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin {
            left: theme.spacing_md.value() as i8,
            right: theme.spacing_md.value() as i8,
            top: theme.spacing_sm.value() as i8,
            bottom: theme.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(label)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(ec(theme.text_secondary())),
            );
        });
}

/// Spec "Hierarchy by text color, on any surface".
pub fn text(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        text_row(
            ui,
            theme,
            ec(theme.text_primary()),
            "text-primary",
            "Primary — titles, active labels",
        );
        text_row(
            ui,
            theme,
            ec(theme.text_secondary()),
            "text-secondary",
            "Secondary — body, descriptions",
        );
        text_row(
            ui,
            theme,
            ec(theme.text_muted()),
            "text-muted",
            "Muted — captions, meta, hints",
        );
        text_row(
            ui,
            theme,
            ec(theme.text_disabled()),
            "text-disabled",
            "Disabled — inert controls",
        );
        text_row(
            ui,
            theme,
            ec(theme.glyph_dim()),
            "glyph-dim",
            "Dim chrome glyph — receding, never disabled",
        );
        let mut buf = String::new();
        Input::new()
            .placeholder("Placeholder — text-placeholder")
            .width(theme.toast_max_width.value())
            .show(ui, theme, &mut buf);
    });

    meta(
        ui,
        theme,
        &[
            ("size", "body 13px, all tints"),
            ("contrast", "primary ≥ 4.5:1 on any surface"),
        ],
        &[
            TokenChip::new("text-primary", "titles", ec(theme.text_primary())),
            TokenChip::new("text-secondary", "body", ec(theme.text_secondary())),
            TokenChip::new("text-muted", "captions", ec(theme.text_muted())),
            TokenChip::new("text-disabled", "inert", ec(theme.text_disabled())),
            TokenChip::new(
                "text-placeholder",
                "empty input",
                ec(theme.text_placeholder()),
            ),
            TokenChip::new("glyph-dim", "receding chrome", ec(theme.glyph_dim())),
        ],
    );
}

fn text_row(ui: &mut egui::Ui, theme: &Theme, color: egui::Color32, tok: &str, sample: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        ui.label(
            egui::RichText::new(sample)
                .size(theme.font_size_body.value())
                .color(color),
        );
        ui.label(
            egui::RichText::new(tok)
                .monospace()
                .size(theme.font_size_micro.value())
                .color(ec(theme.text_muted())),
        );
    });
}

/// Spec "Accents map to roles, not decoration".
pub fn accents(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Column, |ui| {
        accent_row(
            ui,
            theme,
            ec(theme.accent_primary()),
            "accent-primary",
            "primary action",
            |ui, theme| {
                tasty_ui_widgets::Button::new("Open").show(ui, theme);
            },
        );
        accent_row(
            ui,
            theme,
            ec(theme.accent_info()),
            "accent-info",
            "informational",
            |ui, theme| {
                tag(ui, theme, "info", TagVariant::Accent, false);
            },
        );
        accent_row(
            ui,
            theme,
            ec(theme.accent_success()),
            "accent-success",
            "running / ok",
            |ui, theme| {
                status_dot(
                    ui,
                    theme,
                    StatusKind::Running,
                    "running",
                    false,
                    false,
                    tasty_ui_widgets::Ground::Panel,
                );
            },
        );
        accent_row(
            ui,
            theme,
            ec(theme.accent_warning()),
            "accent-warning",
            "caution / readonly",
            |ui, theme| {
                tag(ui, theme, "readonly", TagVariant::Warning, true);
            },
        );
        accent_row(
            ui,
            theme,
            ec(theme.accent_danger()),
            "accent-danger",
            "destructive / error",
            |ui, theme| {
                tasty_ui_widgets::Button::new("Delete")
                    .variant(tasty_ui_widgets::ButtonVariant::Danger)
                    .show(ui, theme);
            },
        );
        accent_row(
            ui,
            theme,
            ec(theme.accent_agent()),
            "accent-agent",
            "agent — always mauve",
            |ui, theme| {
                status_dot(
                    ui,
                    theme,
                    StatusKind::Agent,
                    "agent",
                    false,
                    false,
                    tasty_ui_widgets::Ground::Panel,
                );
                tag(ui, theme, "plugin", TagVariant::Agent, false);
            },
        );
    });

    meta(
        ui,
        theme,
        &[
            ("fill text", "text-on-accent"),
            ("focus ring", "2px accent-primary"),
            ("agent", "mauve = always agent, never decorative"),
        ],
        &[
            TokenChip::new("accent-primary", "primary", ec(theme.accent_primary())),
            TokenChip::new("accent-info", "info", ec(theme.accent_info())),
            TokenChip::new("accent-success", "success", ec(theme.accent_success())),
            TokenChip::new("accent-warning", "warning", ec(theme.accent_warning())),
            TokenChip::new("accent-danger", "danger", ec(theme.accent_danger())),
            TokenChip::new("accent-agent", "agent", ec(theme.accent_agent())),
            TokenChip::new(
                "accent-decorative",
                "decoration only",
                ec(theme.accent_decorative()),
            ),
        ],
    );
}

/// accent 한 행 — [16px swatch] [role 150px] [demo 위젯].
fn accent_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    swatch: egui::Color32,
    tok: &str,
    role: &str,
    demo: impl FnOnce(&mut egui::Ui, &Theme),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
        let s = theme.spacing_lg.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
        ui.painter()
            .rect_filled(r, theme.corner_radius_sm.value(), swatch);
        ui.allocate_ui(egui::vec2(theme.tab_width.value(), s), |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(tok)
                        .monospace()
                        .size(theme.font_size_micro.value())
                        .color(ec(theme.text_primary())),
                );
                ui.label(
                    egui::RichText::new(role)
                        .size(theme.font_size_micro.value())
                        .color(ec(theme.text_muted())),
                );
            });
        });
        demo(ui, theme);
    });
}

/// 터미널의 ANSI 색과 선택·검색·커서 색. UI의 강조색과 값이 같아도 역할을 구분한다.
pub fn terminal(ui: &mut egui::Ui, theme: &Theme) {
    let normal: [(&str, &str, egui::Color32); 8] = [
        ("ansi-black", "30", ec(theme.ansi_black)),
        ("ansi-red", "31", ec(theme.ansi_red)),
        ("ansi-green", "32", ec(theme.ansi_green)),
        ("ansi-yellow", "33", ec(theme.ansi_yellow)),
        ("ansi-blue", "34", ec(theme.ansi_blue)),
        ("ansi-magenta", "35", ec(theme.ansi_magenta)),
        ("ansi-cyan", "36", ec(theme.ansi_cyan)),
        ("ansi-white", "37", ec(theme.ansi_white)),
    ];
    let bright: [(&str, &str, egui::Color32); 8] = [
        ("ansi-bright-black", "90", ec(theme.ansi_bright_black)),
        ("ansi-bright-red", "91", ec(theme.ansi_bright_red)),
        ("ansi-bright-green", "92", ec(theme.ansi_bright_green)),
        ("ansi-bright-yellow", "93", ec(theme.ansi_bright_yellow)),
        ("ansi-bright-blue", "94", ec(theme.ansi_bright_blue)),
        ("ansi-bright-magenta", "95", ec(theme.ansi_bright_magenta)),
        ("ansi-bright-cyan", "96", ec(theme.ansi_bright_cyan)),
        ("ansi-bright-white", "97", ec(theme.ansi_bright_white)),
    ];

    stage(ui, theme, StageVariant::Column, |ui| {
        ui.columns(2, |cols| {
            ansi_col(&mut cols[0], theme, "normal · SGR 30–37", &normal);
            ansi_col(&mut cols[1], theme, "bright · SGR 90–97", &bright);
        });

        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("TERMINAL STATE FILLS — PAINTED ONTO CELLS")
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(ec(theme.text_muted())),
            );
            ui.add_space(theme.spacing_sm.value());
            faux_terminal(ui, theme);
            faux_terminal_matches(ui, theme, true);
            faux_terminal_matches(ui, theme, false);
            ui.add_space(theme.spacing_sm.value());
            sem_row(
                ui,
                theme,
                ec(theme.selection_bg),
                "selection-bg",
                "mouse / keyboard selection region",
            );
            sem_row(
                ui,
                theme,
                ec(theme.vi_cursor_bg),
                "vi-cursor-bg",
                "vi-mode block cursor",
            );
            sem_row(
                ui,
                theme,
                ec(theme.search_match_bg),
                "search-match-bg",
                "search matches in scrollback",
            );
            sem_row(
                ui,
                theme,
                ec(theme.search_match_active_bg),
                "search-match-active-bg",
                "the current / active match",
            );
            sem_row(
                ui,
                theme,
                ec(theme.search_match_active_fg),
                "search-match-active-fg",
                "cell ink inside the active match",
            );
        });
    });

    meta(
        ui,
        theme,
        &[
            ("set", "16 ANSI (8 normal + 8 bright)"),
            ("source", "derived from the Catppuccin palette"),
            ("scope", "terminal cells — not UI chrome"),
            ("state fills", "selection · vi cursor · search"),
            (
                "match ink",
                "Default, bold and dim read text-primary; ANSI cells keep their color.",
            ),
        ],
        &[
            TokenChip::new("ansi-red", "SGR 31 red", ec(theme.ansi_red)),
            TokenChip::new("ansi-green", "SGR 32 green", ec(theme.ansi_green)),
            TokenChip::new(
                "ansi-bright-blue",
                "SGR 94 br.blue",
                ec(theme.ansi_bright_blue),
            ),
            TokenChip::new("selection-bg", "selection fill", ec(theme.selection_bg)),
            TokenChip::new(
                "search-match-active-bg",
                "active match",
                ec(theme.search_match_active_bg),
            ),
        ],
    );
    note(
        ui,
        theme,
        "Active match ink. Glyphs inside the active match are drawn in search-match-active-fg, \
         not the cell's own foreground: Mocha's light text on yellow 70% was 1.61 : 1. Mocha ink \
         → neutral-200 (base) 6.95 : 1. Latte keeps text-primary ink and drops the active fill \
         to 50% → 4.68 : 1 (70% was 3.94). In ordinary matches with 30% fill, default-ink cells, \
         including bold and dim, read text-primary on focused beds (6.67 Mocha, 6.06 Latte) and \
         unfocused beds (4.75 Mocha, 4.88 Latte). Dim ink does not stay blended in a match \
         because it would measure 1.81 in both themes on a focused bed. ANSI cells keep their \
         color in ordinary matches, including when dim; only the active match overrides every \
         cell's color set by the app. A link under the pointer uses selection fill and \
         text-on-active at 4.62 Mocha, 5.40 Latte, with a blue accent-primary-raised underline \
         at 3.17 Mocha, 3.84 Latte. Neither ordinary nor active match fill paints on it; on the \
         focused active fill, the blue underline would measure 1.25 Mocha, 5.23 Latte. \
         Selection, the vi cursor and a link under the pointer read the same on unfocused and \
         focused terminals because term-selection and term-cursor are opaque: selection \
         text-on-active measures 4.62 Mocha, 5.40 Latte, and vi-cursor-fg measures 10.48 Mocha, \
         6.60 Latte. An unfocused terminal draws no block cursor. Selected default-ink cells, \
         including bold and dim, use text-on-active on both beds at 4.62 Mocha, 5.40 Latte; \
         keeping cell ink would measure 4.62 Mocha, 3.69 Latte focused and 3.00 Mocha, 2.89 \
         Latte unfocused. ANSI and reverse cells keep their ink in a selection.",
    );
    dont(
        ui,
        theme,
        "Don't use an ANSI color for UI chrome or an accent for terminal output — \
         ansi-green and accent-success are different roles that only share a hue.",
    );
}

/// ANSI 한 열 — micro uppercase 라벨 + 8 개 `ansi_row`.
fn ansi_col(ui: &mut egui::Ui, theme: &Theme, heading: &str, rows: &[(&str, &str, egui::Color32)]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(heading.to_uppercase())
                .monospace()
                .size(theme.font_size_micro.value())
                .color(ec(theme.text_muted())),
        );
        for (tok, sgr, color) in rows {
            ansi_row(ui, theme, *color, tok, sgr);
        }
    });
}

/// ANSI 한 행 — [16px swatch] [token 1fr] [SGR NN 우측정렬].
fn ansi_row(ui: &mut egui::Ui, theme: &Theme, swatch: egui::Color32, tok: &str, sgr: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        swatch_box(ui, theme, swatch);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("SGR {sgr}"))
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(ec(theme.text_muted())),
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(tok)
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(ec(theme.text_primary())),
                );
            });
        });
    });
}

/// 터미널 상태 채움 한 행 — [16px swatch] [token] [role muted].
fn sem_row(ui: &mut egui::Ui, theme: &Theme, swatch: egui::Color32, tok: &str, role: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        swatch_box(ui, theme, swatch);
        ui.label(
            egui::RichText::new(tok)
                .monospace()
                .size(theme.font_size_caption.value())
                .color(ec(theme.text_primary())),
        );
        ui.label(
            egui::RichText::new(role)
                .size(theme.font_size_caption.value())
                .color(ec(theme.text_muted())),
        );
    });
}

/// 16px 색 swatch (border-strong 1px).
fn swatch_box(ui: &mut egui::Ui, theme: &Theme, fill: egui::Color32) {
    let s = theme.spacing_lg.value();
    let (r, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
    ui.painter()
        .rect_filled(r, theme.corner_radius_sm.value(), fill);
    ui.painter().rect_stroke(
        r,
        theme.corner_radius_sm.value(),
        egui::Stroke::new(theme.border_width.value(), ec(theme.border_strong())),
        egui::StrokeKind::Inside,
    );
}

/// 두 바탕의 상태 칸. 찾기 매치의 기본·dim 글자 셀은 text-primary, ANSI 색 셀은 자기 색이다.
/// 선택과 링크, vi 커서는 두 바탕에서 값이 같다.
/// 매치 밖 dim은 프로그램이 정하는 층이라 렌더러처럼 글자와 바탕을 반씩 섞는다.
fn faux_terminal_matches(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    let term = theme.surface("terminal");
    let (bed, fg, active_ink) = if focused {
        (
            term.focused_bg,
            term.focused_fg,
            theme.search_match_active_fg,
        )
    } else {
        (
            term.unfocused_bg,
            term.unfocused_fg,
            theme.search_match_active_unfocused_fg(),
        )
    };
    let (bed, fg) = (ec(bed), ec(fg));
    let mono = theme.font_size_caption.value();
    let matched = ec(theme.search_match_bg);
    let primary = ec(theme.text_primary());
    let selection = ec(theme.selection_bg);
    let on_active = ec(theme.text_on_active());
    egui::Frame::new()
        .fill(bed)
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let label = if focused {
                    "focused: a "
                } else {
                    "unfocused: a "
                };
                let mut segments = vec![
                    (label, None, fg),
                    ("match", Some(matched), primary),
                    (", a ", None, fg),
                    ("dim match", Some(matched), primary),
                ];
                if !focused {
                    segments.push((", an ", None, fg));
                    segments.push(("ANSI match", Some(matched), ec(theme.ansi_red)));
                }
                segments.extend([
                    (" and the active ", None, fg),
                    (
                        "match",
                        Some(ec(theme.search_match_active_bg)),
                        ec(active_ink),
                    ),
                    (". ", None, fg),
                    ("Dim outside matches", None, fg.lerp_to_gamma(bed, 0.5)),
                    (". ", None, fg),
                    ("Selected words", Some(selection), on_active),
                    (". ", None, fg),
                ]);
                if !focused {
                    segments.push((
                        "Vi cursor",
                        Some(ec(theme.vi_cursor_bg)),
                        ec(theme.vi_cursor_fg()),
                    ));
                    segments.push((". ", None, fg));
                }
                for (text, bg, color) in segments {
                    let mut run = egui::RichText::new(text)
                        .monospace()
                        .size(mono)
                        .color(color);
                    if let Some(bg) = bg {
                        run = run.background_color(bg);
                    }
                    ui.label(run);
                }
                // 본체처럼 밑줄을 채움 위, 글자 아래에 칠하려고 자리를 먼저 잡는다.
                let slot = ui.painter().add(egui::Shape::Noop);
                let link = ui.label(
                    egui::RichText::new("Link under pointer")
                        .monospace()
                        .size(mono)
                        .color(on_active),
                );
                let line = theme.border_width.value();
                let bottom = link.rect.bottom() - line;
                ui.painter().set(
                    slot,
                    egui::Shape::Vec(vec![
                        egui::Shape::rect_filled(link.rect, 0.0, selection),
                        egui::Shape::hline(
                            link.rect.x_range(),
                            bottom,
                            egui::Stroke::new(line, ec(theme.accent_primary_raised())),
                        ),
                    ]),
                );
            });
        });
}

/// 상태 채움을 보여주는 가짜 터미널 줄 — 가장 깊은 surface(bg-app) 위 mono 텍스트.
fn faux_terminal(ui: &mut egui::Ui, theme: &Theme) {
    let fg = ec(theme.text_primary());
    let mono = theme.font_size_caption.value();
    egui::Frame::new()
        .fill(ec(theme.bg_app()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            ec(theme.separator.to_egui_premultiplied()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(
                    egui::RichText::new("~/tasty")
                        .monospace()
                        .size(mono)
                        .color(ec(theme.ansi_green)),
                );
                ui.label(
                    egui::RichText::new(" $ vi notes.md")
                        .monospace()
                        .size(mono)
                        .color(fg),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let seg = |ui: &mut egui::Ui,
                           txt: &str,
                           bg: Option<egui::Color32>,
                           color: egui::Color32| {
                    let mut rt = egui::RichText::new(txt).monospace().size(mono).color(color);
                    if let Some(b) = bg {
                        rt = rt.background_color(b);
                    }
                    ui.label(rt);
                };
                seg(ui, "The ", None, fg);
                seg(
                    ui,
                    "selected words",
                    Some(ec(theme.selection_bg)),
                    ec(theme.text_on_active()),
                );
                seg(ui, " wrap the ", None, fg);
                seg(ui, "match", Some(ec(theme.search_match_bg)), fg);
                seg(ui, " and the ", None, fg);
                seg(
                    ui,
                    "active match",
                    Some(ec(theme.search_match_active_bg)),
                    ec(theme.search_match_active_fg),
                );
                seg(ui, " sit ", None, fg);
                // vi 블록 커서: 밝은 채움 위 어두운 글리프.
                seg(
                    ui,
                    "h",
                    Some(ec(theme.vi_cursor_bg)),
                    ec(theme.text_on_accent()),
                );
                seg(ui, "ere", None, fg);
            });
        });
}
