//! Handler › File Detectors · File Handlers 예제. 시안 `file_handlers.jsx`의 Pane을 시드 상태로 고정해
//! 그린다. 행·카드·머리는 본체와 같은 `tasty_ui_widgets::fh_*`를 쓰고, 상태 전이는 시안 kit을 옮긴
//! 갤러리 전용 모델이다(실제 레지스트리를 읽지 않는다).

use std::cell::RefCell;
use std::collections::HashMap;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    FhCardLabels, FhLine, FhOrigin, FhRowView, FhSummarySeg, FhTrailing, Ground, Input, fh_card,
    fh_empty, fh_field, fh_intro, fh_list_head, fh_row,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 시안 `FhPair`의 한 칸 폭 `--tasty-size-560`. 설정 콘텐츠 컬럼 근사이며 공개 역할 토큰이 없다.
const PAIR_WIDTH: LogicalPx = LogicalPx(560.0);
/// 시안 좁은 패널 `--tasty-size-360`.
const NARROW_WIDTH: LogicalPx = LogicalPx(360.0);

/// 시안 `NARROW_SPECS` — 좁은 열에서 1줄을 줄이는 순서.
const NARROW_SPECS: &[(&str, &str)] = &[
    (
        "line 1 fit order",
        "1 origin Tags collapse → “N sources” · 2 pending Tag leaves line 1 · 3 id ellipsizes — each step only while the id would ellipsize; undone once the line is back to the width it had when the step was taken",
    ),
    (
        "id floor",
        "min(file-handler-id-min-width 96, full id width) once the pending Tag has dropped · tooltip = full id",
    ),
];

/// 시안 raised 카드 Spec 표.
const RAISED_SPECS: &[(&str, &str)] = &[
    (
        "error line",
        "file-handler-form-error-fg → accent-danger-raised · glyph takes the same ink",
    ),
    (
        "Cancel",
        "Button ground=\"raised\" → button-ghost-raised-fg → text-secondary-raised · hover button-ghost-raised-fg-hover → text-primary",
    ),
    (
        "Input / Select box",
        "ground=\"raised\" in every card field: bg → bg-panel, edge → border-strong (input-raised-* · select-raised-*). Focus: border-focus. Invalid: accent-danger edge. Disabled: state-disabled-fill-raised + state-disabled-border-raised",
    ),
    (
        "scope",
        "same fix on the Hook Handlers add card (labels text-secondary-raised, Inputs raised, Cancel raised) · card background unchanged (surface-raised)",
    ),
];

fn raised_tokens(th: &Theme) -> Vec<TokenChip> {
    vec![
        TokenChip::new(
            "accent-danger-raised",
            "error ink on the card",
            th.accent_danger_raised().to_egui(),
        ),
        TokenChip::new(
            "button-ghost-raised-fg",
            "→ text-secondary-raised",
            th.button_ghost_raised_fg().to_egui(),
        ),
        TokenChip::new(
            "select-raised-bg",
            "→ bg-panel",
            th.select_raised_bg().to_egui(),
        ),
        TokenChip::new(
            "select-raised-border",
            "→ border-strong",
            th.select_raised_border().to_egui(),
        ),
        TokenChip::new(
            "select-raised-disabled-bg",
            "→ state-disabled-fill-raised",
            th.select_raised_disabled_bg().to_egui(),
        ),
        TokenChip::new(
            "select-raised-disabled-border",
            "→ state-disabled-border-raised",
            th.select_raised_disabled_border().to_egui(),
        ),
    ]
}

/// 시안 raised 카드 상태 칸: Mocha·Latte마다 surface-raised 위에 Input 넷 · Select 셋 · ghost Cancel.
fn raised_card_states(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
        for (name, th) in &themes {
            egui::Frame::new()
                .fill(th.file_handler_form_bg().to_egui())
                .stroke(egui::Stroke::new(
                    th.border_width.value(),
                    th.file_handler_form_border().to_egui(),
                ))
                .corner_radius(th.corner_radius.value())
                .inner_margin(tasty_ui_widgets::margin_all(th.file_handler_form_padding()))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                    ui.label(
                        egui::RichText::new(*name)
                            .size(th.font_size_caption.value())
                            .color(th.file_handler_form_label_fg().to_egui()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing =
                            egui::vec2(th.spacing_md.value(), th.spacing_md.value());
                        raised_cells(ui, th);
                    });
                    tasty_ui_widgets::fh_line(
                        ui,
                        th,
                        FhLine::Error(crate::i18n::t(
                            "settings.file_handler.detectors.err_id_chars",
                        )),
                    );
                });
        }
    });
}

fn raised_cells(ui: &mut egui::Ui, th: &Theme) {
    let w = th.field_width_lg;
    let cell = |ui: &mut egui::Ui, label: &str, draw: &mut dyn FnMut(&mut egui::Ui)| {
        spec::wrap_item(ui, |ui| {
            ui.set_width(w.value());
            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
            ui.label(
                egui::RichText::new(label)
                    .size(th.font_size_micro.value())
                    .color(th.file_handler_form_label_fg().to_egui()),
            );
            draw(ui);
        });
    };
    let input =
        |ui: &mut egui::Ui, value: &str, placeholder: &str, invalid: bool, enabled: bool| {
            let mut buf = value.to_string();
            Input::new()
                .ground(Ground::Raised)
                .mono(true)
                .placeholder(placeholder)
                .invalid(invalid)
                .enabled(enabled)
                .width(w.value())
                .show(ui, th, &mut buf);
        };
    cell(ui, "Input · normal", &mut |ui| {
        input(ui, "", "my-detector", false, true)
    });
    cell(ui, "Input · value", &mut |ui| {
        input(ui, "dockerfile", "", false, true)
    });
    cell(ui, "Input · invalid", &mut |ui| {
        input(ui, "Docker_File", "", true, true)
    });
    cell(ui, "Input · disabled", &mut |ui| {
        input(ui, "dockerfile", "", false, false)
    });
    let pinned = |ui: &mut egui::Ui, value: Option<&str>, enabled: bool| {
        tasty_ui_widgets::select_pinned(
            ui,
            th,
            Ground::Raised,
            value,
            crate::i18n::t("settings.file_handler.handlers.field_detector_select"),
            w,
            tasty_ui_widgets::SelectLook::Normal,
            enabled,
        );
    };
    cell(ui, "Select · placeholder", &mut |ui| {
        pinned(ui, None, true)
    });
    cell(ui, "Select · value", &mut |ui| {
        pinned(ui, Some("markdown"), true)
    });
    cell(ui, "Select · disabled", &mut |ui| {
        pinned(ui, Some("markdown"), false)
    });
    cell(ui, "Ghost sm · raised", &mut |ui| {
        tasty_ui_widgets::Button::new(crate::i18n::t("button.cancel"))
            .variant(tasty_ui_widgets::ButtonVariant::Ghost)
            .size(tasty_ui_widgets::ControlSize::Sm)
            .ground(Ground::Raised)
            .show(ui, th);
    });
}

mod copy;
mod model;
mod panes;

use copy::*;
use model::*;
use panes::*;

// ── Stage ───────────────────────────────────────────────────────────

enum PaneState {
    Det(DetState),
    Hdl(HdlState),
}

thread_local! {
    static PANES: RefCell<HashMap<(String, &'static str), PaneState>> = RefCell::new(HashMap::new());
}

/// 시안 `FhPair` + `FhPanel`: Mocha·Latte 두 칸에 같은 시드의 Pane을 각각 그린다.
fn pane(ui: &mut egui::Ui, theme: &Theme, key: &str, det: bool, seed: Seed) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    let copy = if seed.de { copy_de() } else { copy_en() };
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing =
                egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
            for (label, th) in &themes {
                let cell = PAIR_WIDTH * theme.ui_zoom;
                // 시안 `flex: 1 1 560` + wrap: 두 칸이 문서 칸을 넘으면 Latte 칸이 아래 줄로 간다.
                spec::wrap_item(ui, |ui| {
                    egui::Frame::new()
                        .fill(th.bg_app().to_egui())
                        .corner_radius(th.corner_radius.value())
                        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                        .show(ui, |ui| {
                            ui.set_width(cell.value());
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                                ui.label(
                                    egui::RichText::new(*label)
                                        .size(th.font_size_caption.value())
                                        .color(th.text_muted().to_egui()),
                                );
                                let panel_w = if seed.narrow {
                                    NARROW_WIDTH * theme.ui_zoom
                                } else {
                                    cell
                                };
                                panel(ui, th, panel_w, |ui| {
                                    PANES.with(|p| {
                                        let mut p = p.borrow_mut();
                                        let st = p.entry((key.to_string(), *label)).or_insert_with(
                                            || {
                                                if det {
                                                    PaneState::Det(det_state(&seed))
                                                } else {
                                                    PaneState::Hdl(hdl_state(&seed))
                                                }
                                            },
                                        );
                                        ui.push_id((key, *label), |ui| match st {
                                            PaneState::Det(s) => detectors_pane(ui, th, &copy, s),
                                            PaneState::Hdl(s) => handlers_pane(ui, th, &copy, s),
                                        });
                                    });
                                });
                            });
                        });
                });
            }
        });
    });
}

/// 설정 콘텐츠 컬럼 — bg-panel · 1px border-frame · radius · padding space-lg.
fn panel(ui: &mut egui::Ui, th: &Theme, width: LogicalPx, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(th.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_frame().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_lg.value() as i8))
        .show(ui, |ui| {
            let inner = (width - (th.spacing_lg + th.border_width) * 2.0).max(LogicalPx(0.0));
            ui.set_width(inner.value());
            ui.vertical(add);
        });
}

const ROW_SPECS: &[(&str, &str)] = &[
    (
        "row",
        "2 lines · padding space-sm space-xs · gap space-xs · bottom 1px separator",
    ),
    (
        "line 1",
        "min-height button-height-sm · gap space-sm · id mono body 13 semibold (ellipsis, full id in tooltip) · origin Tag · [prio N micro] · [pending Tag] · spacer · Switch · trailing slot",
    ),
    (
        "origin Tag(s)",
        "host / user neutral · plugin:<id> agent variant · detectors: one Tag per source, order host → plugin → user, gap space-sm; a rule-less user patch shows “user override” · collapse → one neutral “N sources” Tag (tooltip = full list) once the Tags would ellipsize the id",
    ),
    (
        "trailing slot",
        "any user contribution → Remove (ghost sm) · pending → Undo (ghost sm), same slot · no user contribution → lock glyph icon-size-sm, glyph-dim, tooltip “Provided by host, plugin:logkit”",
    ),
    (
        "line 2",
        "summary mono term-sm, one line, ellipsis, full value in tooltip · detectors: every source's rules in merge order “ext md, markdown · ext mdx” · handlers “<detector> → <action>”",
    ),
    (
        "switched off",
        "id text-secondary · summary text-muted (still editable)",
    ),
    (
        "sort",
        "detectors: registry order · handlers: run order — priority ↑ → owner user > plugin > host → id",
    ),
];

const CARD_SPECS: &[(&str, &str)] = &[
    (
        "card",
        "file-handler-form-bg · 1px file-handler-form-border · radius · padding file-handler-form-padding 12 · row gap file-handler-form-row-gap 8",
    ),
    ("title", "font-label micro caps, file-handler-form-label-fg"),
    (
        "field row",
        "label column settings-label-width 150 (wraps, body 13) · gap settings-label-gap 16 · control column flex ≥ field-width-lg; wraps under the label when the card is narrower",
    ),
    (
        "control column",
        "control (control-height 28) + one line under it, gap space-xs — priority error > info > caption",
    ),
    (
        "info line",
        "info glyph icon-size-sm + caption 12, file-handler-form-info-fg, role status; same box as the error line; live while typing (not only after Add)",
    ),
    (
        "widths",
        "text Inputs block (fill) · Detector Select file-handler-select-width 200 · Action Select file-handler-kind-width 160 · Priority Input file-handler-priority-width 90",
    ),
    (
        "buttons",
        "right-aligned, gap space-sm: Cancel ghost sm · Add primary sm",
    ),
    (
        "error",
        "alertCircle icon-size-sm + caption 12, file-handler-form-error-fg; replaces that field's caption; Input invalid. Cross-field error (no rule) sits above the button row",
    ),
];

fn row_tokens(th: &Theme) -> Vec<TokenChip> {
    vec![
        TokenChip::new(
            "file-handler-prio-fg",
            "→ text-muted",
            th.file_handler_prio_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-summary-fg",
            "→ text-secondary",
            th.file_handler_summary_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-summary-removed-fg",
            "→ text-muted",
            th.file_handler_summary_removed_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-empty-fg",
            "→ text-muted",
            th.file_handler_empty_fg().to_egui(),
        ),
    ]
}

fn card_tokens(th: &Theme) -> Vec<TokenChip> {
    vec![
        TokenChip::new(
            "file-handler-form-bg",
            "→ surface-raised",
            th.file_handler_form_bg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-form-border",
            "→ border-default",
            th.file_handler_form_border().to_egui(),
        ),
        TokenChip::new(
            "file-handler-form-label-fg",
            "→ text-secondary-raised",
            th.file_handler_form_label_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-form-caption-fg",
            "→ text-secondary-raised",
            th.file_handler_form_caption_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-form-info-fg",
            "→ text-secondary-raised",
            th.file_handler_form_info_fg().to_egui(),
        ),
        TokenChip::new(
            "file-handler-form-error-fg",
            "→ accent-danger",
            th.file_handler_form_error_fg().to_egui(),
        ),
        TokenChip::without_color("file-handler-priority-width", "→ field-width-xs 90"),
        TokenChip::without_color("file-handler-select-width", "→ field-width-lg 200"),
        TokenChip::without_color("file-handler-kind-width", "→ field-width-md 160"),
    ]
}

/// File Detectors 쪽 Spec — 목록, 카드, 검증, 여러 출처, 사용자 기여 삭제, patch, 대기, 빈 목록, 긴 문구.
pub fn draw_detectors(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Registered rows — two lines, no table",
        Some(
            "Line 1: id · one origin Tag per source · Switch · trailing slot. Line 2: every source's rules in merge order. Only rows with a user contribution have Remove; the rest show the padlock.",
        ),
    );
    pane(ui, theme, "det-list", true, Seed::default());
    spec::meta(ui, theme, ROW_SPECS, &row_tokens(theme));

    spec::spec(
        ui,
        theme,
        "Add card — open, empty",
        Some(
            "“Add detector” opens the card between the intro and the list and is disabled while it is open. Add stays disabled until Id has a value.",
        ),
    );
    pane(
        ui,
        theme,
        "det-adding",
        true,
        Seed {
            adding: true,
            ..Seed::default()
        },
    );
    spec::meta(ui, theme, CARD_SPECS, &card_tokens(theme));

    spec::spec(
        ui,
        theme,
        "Add card — validation errors",
        Some(
            "Format errors show after Add is pressed: id characters, and the cross-field “no extension and no glob” line above the buttons.",
        ),
    );
    pane(
        ui,
        theme,
        "det-attempted",
        true,
        Seed {
            adding: true,
            attempted: true,
            draft: Draft {
                id: "Docker_File",
                ..Draft::default()
            },
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Several sources on one detector",
        Some(
            "markdown host + user rule · log host + plugin:logkit + user rule · notebook plugin:md-plus + user override (switched off, no user rule). In the narrow column the Tags collapse into one “N sources” Tag.",
        ),
    );
    pane(ui, theme, "det-sources", true, Seed::default());
    pane(
        ui,
        theme,
        "det-sources-narrow",
        true,
        Seed {
            narrow: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Remove a user contribution — the row stays",
        Some(
            "Only the user Tag and the user rule pieces are struck; the Switch is disabled and shows the state after Save. A user-only row strikes the whole id.",
        ),
    );
    pane(
        ui,
        theme,
        "det-unpatch",
        true,
        Seed {
            unpatch: true,
            ..Seed::default()
        },
    );
    pane(
        ui,
        theme,
        "det-unpatch-pending",
        true,
        Seed {
            unpatch: true,
            pending: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Add with an existing id — patch, not an error",
        Some(
            "An existing id shows the info line at once and the button reads Update detector. A registered $ id is patched the same way; a new $ id fails with the reserved error on Add.",
        ),
    );
    for (key, draft, attempted) in [
        (
            "det-patch-image",
            Draft {
                id: "image",
                ext: "heic",
                ..Draft::default()
            },
            false,
        ),
        (
            "det-patch-markdown",
            Draft {
                id: "markdown",
                ext: "mdx, mdown",
                ..Draft::default()
            },
            false,
        ),
        (
            "det-patch-directory",
            Draft {
                id: "$directory",
                glob: "**/.git",
                ..Draft::default()
            },
            false,
        ),
        (
            "det-reserved",
            Draft {
                id: "$foo",
                ext: "foo",
                ..Draft::default()
            },
            true,
        ),
    ] {
        pane(
            ui,
            theme,
            key,
            true,
            Seed {
                adding: true,
                attempted,
                draft,
                ..Seed::default()
            },
        );
    }
    pane(
        ui,
        theme,
        "det-patched",
        true,
        Seed {
            patched: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Pending — added on save · removed on save",
        Some(
            "Add closes the card and lists the entry with “added on save”; Remove strikes the id. Undo takes either back.",
        ),
    );
    pane(
        ui,
        theme,
        "det-pending",
        true,
        Seed {
            pending: true,
            ..Seed::default()
        },
    );

    spec::spec(ui, theme, "Empty list", None);
    pane(
        ui,
        theme,
        "det-empty",
        true,
        Seed {
            empty: true,
            ..Seed::default()
        },
    );

    spec::spec(ui, theme, "Long labels · narrow column", None);
    pane(
        ui,
        theme,
        "det-de-narrow",
        true,
        Seed {
            de: true,
            narrow: true,
            unpatch: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Narrow column · long labels — pending rows keep the id",
        Some(
            "Line 1 gives room up in a fixed order: the origin Tags collapse first; if the id would still ellipsize, the pending Tag moves to its own line under line 1 (ellipsis + tooltip if even that line is short); only then does the id ellipsize, never below 96 px (or its full width if shorter). Switch and the trailing slot never shrink. Wide columns keep the one-line layout.",
        ),
    );
    for (key, narrow) in [("det-narrow-set", true), ("det-narrow-set-wide", false)] {
        pane(
            ui,
            theme,
            key,
            true,
            Seed {
                de: true,
                narrow,
                narrow_set: true,
                ..Seed::default()
            },
        );
    }
    spec::meta(
        ui,
        theme,
        NARROW_SPECS,
        &[TokenChip::without_color(
            "file-handler-id-min-width",
            "→ size-96",
        )],
    );
    spec::do_(
        ui,
        theme,
        "Keep the summary on one line and put the full value in the tooltip, so every row has the same height.",
    );
}

/// File Handlers 쪽 Spec — 목록, 카드, 검증, 동작 종류, 같은 이름 교체, 대기, 빈 목록, 긴 문구.
pub fn draw_file_handlers(ui: &mut egui::Ui, theme: &Theme) {
    spec::spec(
        ui,
        theme,
        "Registered rows — two lines, no table",
        Some(
            "Line 1: id · origin Tag · prio N · Switch · trailing slot. Line 2: <detector> → <action>. Rows are in run order.",
        ),
    );
    pane(ui, theme, "hdl-list", false, Seed::default());
    spec::meta(ui, theme, ROW_SPECS, &row_tokens(theme));

    spec::spec(
        ui,
        theme,
        "Handler run order — the list is the order handlers run",
        Some(
            "Priority ↑, then owner user > plugin > host, then id. Reading one detector's rows top to bottom is its run order; pending rows take the same slot. Scene: markdown at priority 10 with user/my-viewer · com.example.md/preview · host/markdown-view.",
        ),
    );
    pane(
        ui,
        theme,
        "hdl-tie",
        false,
        Seed {
            tie: true,
            ..Seed::default()
        },
    );
    pane(
        ui,
        theme,
        "hdl-tie-de-narrow",
        false,
        Seed {
            tie: true,
            de: true,
            narrow: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Add card — open, empty",
        Some(
            "Add stays disabled until Short name, Detector and the action's required field have a value. The Saved as caption shows the stored id.",
        ),
    );
    pane(
        ui,
        theme,
        "hdl-adding",
        false,
        Seed {
            adding: true,
            ..Seed::default()
        },
    );
    spec::meta(ui, theme, CARD_SPECS, &card_tokens(theme));

    spec::spec(
        ui,
        theme,
        "Add card — validation errors",
        Some(
            "Short name characters (≤ 32), priority not a whole number, IPC method not dotted — shown after Add.",
        ),
    );
    pane(
        ui,
        theme,
        "hdl-attempted",
        false,
        Seed {
            adding: true,
            attempted: true,
            draft: Draft {
                short: "docker.system",
                det: "dockerfile",
                prio: "1.5",
                surface: "text_view",
                ..Draft::default()
            },
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Add card on a raised ground — error ink · Cancel · field edges",
        Some(
            "The card is surface-raised. Error lines and glyphs take accent-danger-raised, the Cancel ghost takes the raised ghost label, and every Input and Select in the card uses ground=\"raised\" (bg-panel + border-strong). Invalid edges stay accent-danger; the card background is unchanged.",
        ),
    );
    raised_card_states(ui, theme);
    spec::meta(ui, theme, RAISED_SPECS, &raised_tokens(theme));

    spec::spec(
        ui,
        theme,
        "Handler add card — action kinds",
        Some(
            "Open surface: Surface kind + Param key. IPC: Method. System: no rows; the Action caption says what it does.",
        ),
    );
    for (key, draft) in [
        (
            "hdl-kind-surface",
            Draft {
                short: "notes-view",
                det: "markdown",
                kind: 0,
                surface: "markdown_view",
                ..Draft::default()
            },
        ),
        (
            "hdl-kind-ipc",
            Draft {
                short: "notes-ipc",
                det: "markdown",
                prio: "40",
                kind: 1,
                method: "com.example.notes.open",
                ..Draft::default()
            },
        ),
        (
            "hdl-kind-system",
            Draft {
                short: "notes-os",
                det: "markdown",
                kind: 2,
                ..Draft::default()
            },
        ),
    ] {
        pane(
            ui,
            theme,
            key,
            false,
            Seed {
                adding: true,
                draft,
                ..Seed::default()
            },
        );
    }

    spec::spec(
        ui,
        theme,
        "Handler — same short name replaces",
        Some(
            "An existing user/<short> shows the info line in place of Saved as, and the button reads Update handler.",
        ),
    );
    pane(
        ui,
        theme,
        "hdl-replace",
        false,
        Seed {
            adding: true,
            draft: Draft {
                short: "dockerfile-system",
                det: "dockerfile",
                kind: 2,
                ..Draft::default()
            },
            ..Seed::default()
        },
    );

    spec::spec(ui, theme, "Pending — added on save · removed on save", None);
    pane(
        ui,
        theme,
        "hdl-pending",
        false,
        Seed {
            pending: true,
            ..Seed::default()
        },
    );

    spec::spec(ui, theme, "Empty list", None);
    pane(
        ui,
        theme,
        "hdl-empty",
        false,
        Seed {
            empty: true,
            ..Seed::default()
        },
    );

    spec::spec(
        ui,
        theme,
        "Long labels · narrow column",
        Some(
            "Labels wrap inside the 150 column. When the card is narrower than label + gap + field-width-lg the control drops under its label.",
        ),
    );
    pane(
        ui,
        theme,
        "hdl-de",
        false,
        Seed {
            de: true,
            adding: true,
            draft: Draft {
                short: "notizen",
                det: "markdown",
                kind: 0,
                ..Draft::default()
            },
            ..Seed::default()
        },
    );
    pane(
        ui,
        theme,
        "hdl-narrow",
        false,
        Seed {
            narrow: true,
            adding: true,
            draft: Draft {
                short: "notes-view",
                det: "markdown",
                kind: 1,
                method: "com.example.notes.open",
                ..Draft::default()
            },
            ..Seed::default()
        },
    );
    spec::dont(
        ui,
        theme,
        "Shrink the label column below settings-label-width to fit long labels; wrap them instead.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_follow_the_kit_states() {
        let un = det_state(&Seed {
            unpatch: true,
            ..Seed::default()
        });
        let ids: Vec<&str> = un
            .xs
            .iter()
            .filter(|x| x.pending == Some(DetPending::Unpatch))
            .map(|x| x.id.as_str())
            .collect();
        assert_eq!(ids, ["markdown", "log", "notebook"]);
        let pend = det_state(&Seed {
            pending: true,
            ..Seed::default()
        });
        assert_eq!(
            pend.xs
                .iter()
                .filter(|x| x.pending == Some(DetPending::Remove))
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>(),
            ["dockerfile"]
        );
        let h = hdl_state(&Seed {
            draft: Draft {
                det: "dockerfile",
                ..Draft::default()
            },
            ..Seed::default()
        });
        assert_eq!(h.det, Some(5));
    }

    /// tie 장면의 markdown 행은 시안처럼 user → plugin → host 순이다.
    #[test]
    fn the_tie_scene_lists_one_detector_in_run_order() {
        let st = hdl_state(&Seed {
            tie: true,
            ..Seed::default()
        });
        let ids: Vec<&str> = run_order(&st.xs)
            .into_iter()
            .map(|i| &st.xs[i])
            .filter(|x| x.det == "markdown")
            .map(|x| x.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "user/my-viewer",
                "com.example.md/preview",
                "host/markdown-view"
            ]
        );
    }
}
