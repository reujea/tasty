//! Settings › Appearance 의 거절된 글꼴 표시 — 시안 Spec `appearance-font-file-rejected`,
//! `appearance-font-family-rejected`, `font-rejected-toast`. 본체와 같은 위젯을 상태별로 그린다.

use std::cell::{Cell, RefCell};

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    FieldErrorLine, FieldErrorState, Input, OverrideCell, SearchableSelect, SearchableSelectLabels,
    SearchableSelectOption, SearchableSelectSpecimen, SettingsRow, Tooltip, TooltipPlacement,
    checkbox, field_error_state, override_row_with_error, settings_label_column,
};

use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage, wrap_item};
use crate::catalog::toast_card::{self, ToastKind};
use crate::i18n::{t, t_lang};

use super::settings_font_override as grid;

mod held;

// 시안 프레임의 전시 치수. 대응 역할 토큰이 없어 갤러리 무대 치수로 둔다.
/// `--tasty-size-560`: 기본 프레임 폭.
const FRAME_W: LogicalPx = LogicalPx(560.0);
/// `--tasty-size-620`: 격자와 미리보기 프레임 폭.
const GRID_FRAME_W: LogicalPx = LogicalPx(620.0);
/// `--tasty-size-460`: 체크박스가 줄 아래로 내려가는 폭.
const DROPPED_W: LogicalPx = LogicalPx(460.0);
/// `--tasty-size-360`: 컨트롤이 행 시작으로 내려가는 폭.
const STACKED_W: LogicalPx = LogicalPx(360.0);
/// `--tasty-size-320`: 기본 격자의 좁은 폭.
const BASE_NARROW_W: LogicalPx = LogicalPx(320.0);
/// `--tasty-size-400`: 메인 창 모서리 폭.
const CORNER_W: LogicalPx = LogicalPx(400.0);
/// `--tasty-size-180`: 메인 창 모서리 최소 높이.
const CORNER_MIN_H: LogicalPx = LogicalPx(180.0);
/// `--tasty-size-300`: 열린 목록이 들어가는 칸의 최소 높이.
const OPEN_MIN_H: LogicalPx = LogicalPx(300.0);
/// `--tasty-size-64`: hover Tooltip 이 들어가도록 위에 비우는 높이.
const HOVER_TOP: LogicalPx = LogicalPx(64.0);

const NOTES_TXT: &str = "~/fonts/notes.txt";
const GOOD_FILE: &str = "~/fonts/D2Coding.ttf";
const REMOVED_FILE: &str = "~/fonts/Removed.ttf";
const NOT_A_FONT_NOTE: &str = "~/notes/todo.txt";
const NEGATIVE_FILE: &str = "~/fonts/NegativeLineHeight.ttf";
const LONG_PATH: &str =
    "/usr/local/share/fonts/NerdFonts/JetBrainsMono/JetBrainsMonoNerdFont-Regular.ttf";
const TOAST_LONG: &str = "/usr/local/…/JetBrainsMono/JetBrainsMonoNerdFont-Regular.ttf";

fn themes(theme: &Theme) -> [(&'static str, Theme); 2] {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ]
}

fn mono_label(ui: &mut egui::Ui, th: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .monospace()
            .size(th.font_size_micro.value())
            .color(th.text_muted().to_egui()),
    );
}

/// 시안 `Frame`: 라벨 + bg-panel 상자. 상자 폭은 `width` 이고 안쪽 여백은 `space-lg` 다.
fn frame(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    width: LogicalPx,
    add: impl FnOnce(&mut egui::Ui, &Theme),
) {
    wrap_item(ui, |ui| {
        ui.push_id(label, |ui| {
            egui::Frame::new()
                .fill(th.bg_app().to_egui())
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                        mono_label(ui, th, label);
                        let pad = th.spacing_lg.value();
                        egui::Frame::new()
                            .fill(th.bg_panel().to_egui())
                            .stroke(egui::Stroke::new(
                                th.border_width.value(),
                                th.border_frame().to_egui(),
                            ))
                            .corner_radius(th.corner_radius.value())
                            .inner_margin(egui::Margin::same(pad as i8))
                            .show(ui, |ui| {
                                let inner = width.value() - pad * 2.0;
                                ui.set_width(inner);
                                ui.set_max_width(inner);
                                ui.vertical(|ui| add(ui, th));
                            });
                    });
                });
        });
    });
}

/// 시안 `St`: 프레임 안에 이름 붙은 상태 하나. 아래 `space-md` 를 비운다.
fn state(ui: &mut egui::Ui, th: &Theme, name: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.push_id(name, |ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        mono_label(ui, th, name);
        add(ui);
        ui.add_space(th.spacing_md.value());
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum File {
    Rest,
    Focus,
    Rejected,
    FocusRejected,
    Default,
    /// 칸만 있고 포커스도 빨강도 없다.
    Held,
    /// 테두리만 있고 칸이 아직 없다.
    Due,
    /// Use default 를 켰고 칸이 이번 프레임까지 남는다.
    DefaultHeld,
}

impl File {
    fn line(self, text: &str) -> Option<FieldErrorLine<'_>> {
        match self {
            Self::Rejected => FieldErrorState::Shown.line(text),
            Self::FocusRejected | Self::Held | Self::DefaultHeld => {
                FieldErrorState::Held.line(text)
            }
            Self::Due => FieldErrorState::Due.line(text),
            Self::Rest | Self::Focus | Self::Default => None,
        }
    }

    fn default_on(self) -> bool {
        matches!(self, Self::Default | Self::DefaultHeld)
    }
}

/// 파일 입력 하나. 본체처럼 mono `field-width-lg` 이고 포커스 상태는 invalid 가 아니다.
fn file_input(ui: &mut egui::Ui, th: &Theme, value: &str, st: File) -> egui::Rect {
    let mut shown = value.to_owned();
    Input::new()
        .mono(true)
        .width(th.field_width_lg.value())
        .invalid(matches!(st, File::Rejected | File::Due))
        .focus_look(matches!(st, File::Focus | File::FocusRejected))
        .read_only(st.default_on())
        .full_value_tooltip(true)
        .show(ui, th, &mut shown)
        .rect
}

fn file_line(lang: &str) -> &'static str {
    t_lang(lang, "settings.appearance.font.custom_font_rejected")
}

/// 기본 글꼴 격자의 파일 행. 콜론이 붙은 라벨에 체크박스가 없다.
fn base_file_row(ui: &mut egui::Ui, th: &Theme, lang: &str, value: &str, st: File) {
    let row = SettingsRow::new(t_lang(lang, "settings.appearance.custom_font_label"));
    let col = th.settings_label_width();
    row.show_with_error(ui, th, col, |ui| {
        file_input(ui, th, value, st);
        st.line(file_line(lang))
    });
}

/// override 행 한 줄. 라벨 · 컨트롤 · 체크박스 · 오류 줄을 `override_row_with_error` 로 배치한다.
fn override_font_row<'a>(
    ui: &mut egui::Ui,
    th: &Theme,
    lang: &str,
    label: &str,
    checked: bool,
    mut control: impl FnMut(&mut egui::Ui) -> Option<FieldErrorLine<'a>>,
) {
    let col = th.settings_label_width();
    let use_default = t_lang(lang, "settings.appearance.font.use_default_label");
    let row = SettingsRow::new(label);
    override_row_with_error(
        ui,
        th,
        col,
        th.field_width_lg,
        use_default,
        |ui, cell, line| match cell {
            OverrideCell::Label => {
                row.show_label(ui, th, col, th.settings_row_min_height());
            }
            OverrideCell::Control => *line = control(ui),
            OverrideCell::UseDefault => {
                let mut on = checked;
                checkbox(
                    ui,
                    th,
                    &mut on,
                    use_default,
                    true,
                    tasty_ui_widgets::Ground::Panel,
                );
            }
        },
    );
}

fn override_file_row(ui: &mut egui::Ui, th: &Theme, lang: &str, value: &str, st: File) {
    override_file_row_checked(ui, th, lang, value, st, st.default_on());
}

/// 체크박스 상태가 행 상태와 따로 움직이는 단계용.
fn override_file_row_checked(
    ui: &mut egui::Ui,
    th: &Theme,
    lang: &str,
    value: &str,
    st: File,
    checked: bool,
) {
    let label = t_lang(lang, "settings.appearance.font.override_custom_font_label");
    override_font_row(ui, th, lang, label, checked, |ui| {
        file_input(ui, th, value, st);
        st.line(file_line(lang))
    });
}

/// 시안 `FontOverrideG`: 파일 행만 거절된 격자와 그 아래 미리보기.
fn rejected_grid(ui: &mut egui::Ui, th: &Theme) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let labels: Vec<SettingsRow<'_>> = grid::ROWS
        .iter()
        .enumerate()
        .map(|(i, (label, _))| match i {
            3 => SettingsRow::new(label).hint(grid::line_height_hint()),
            4 => SettingsRow::new(label).hint(grid::scale_mode_hint()),
            _ => SettingsRow::new(label),
        })
        .collect();
    let col = settings_label_column(ui, th, &labels);
    let row_h = th.settings_row_min_height();
    let check = t("settings.appearance.font.use_default_label");
    for (i, row) in labels.iter().enumerate() {
        let control_w = match i {
            0 | 1 => th.field_width_lg,
            2 | 3 => th.field_width_xs,
            _ => th.field_width_md,
        };
        let def = i != 1 && grid::ROWS[i].1;
        ui.push_id(i, |ui| {
            override_row_with_error(ui, th, col, control_w, check, |ui, cell, line| match cell {
                OverrideCell::Label => {
                    row.show_label(ui, th, col, row_h);
                }
                OverrideCell::Control if i == 1 => {
                    file_input(ui, th, NOTES_TXT, File::Rejected);
                    *line = File::Rejected.line(file_line("en"));
                }
                OverrideCell::Control => {
                    let mut value = match i {
                        0 => "D2Coding".to_owned(),
                        2 => "14".to_owned(),
                        3 => "1.2".to_owned(),
                        _ => String::new(),
                    };
                    grid::control(ui, th, i, &mut value, !def);
                }
                OverrideCell::UseDefault => {
                    let mut on = def;
                    checkbox(
                        ui,
                        th,
                        &mut on,
                        check,
                        true,
                        tasty_ui_widgets::Ground::Panel,
                    );
                }
            });
        });
    }
    grid::preview_below(ui, th);
}

thread_local! {
    static LIVE: RefCell<String> = RefCell::new(NOT_A_FONT_NOTE.to_owned());
    static LIVE_BELOW: Cell<bool> = const { Cell::new(false) };
}

/// 시안 live 프레임과 그 아래 행. 파일 없이 동작하도록 확장자만 본다. 본체는 파일을 읽어 판정한다.
fn live_file_row(ui: &mut egui::Ui, th: &Theme) {
    LIVE.with(|buf| {
        let buf = &mut *buf.borrow_mut();
        let id = ui.id().with("live_file");
        let bad = |value: &str| {
            let lower = value.to_ascii_lowercase();
            !value.is_empty() && ![".ttf", ".otf", ".ttc"].iter().any(|e| lower.ends_with(e))
        };
        override_font_row(ui, th, "en", "Custom font file", false, |ui| {
            let rejected = bad(buf);
            let resp = Input::new()
                .id(id)
                .mono(true)
                .width(th.field_width_lg.value())
                .invalid_at_rest(rejected)
                .full_value_tooltip(true)
                .show(ui, th, buf);
            field_error_state(ui.ctx(), id, resp.has_focus(), bad(buf)).line(file_line("en"))
        });
    });
    LIVE_BELOW.with(|below| {
        let mut on = below.get();
        held::below_row(ui, th, &mut on);
        below.set(on);
    });
}

fn stage_row(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.horizontal_wrapped(|ui| {
            let gap = theme.spacing_lg.value();
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            add(ui);
        });
    });
}

/// Appearance › Custom font file rejected.
pub fn file_spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "appearance-font-file-rejected",
        "Appearance › Custom font file rejected",
        Some(
            "invalid edge and one line under the Input · nothing red while focused · the box height changes only with no pointer button held · the Terminal row judges by what the terminal can use",
        ),
        draw_file,
    )
}

pub fn family_spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "appearance-font-family-rejected",
        "Appearance › Font family rejected",
        Some(
            "same line as the file field · no invalid edge · line box kept while the list is open · the box height changes only with no pointer button held",
        ),
        draw_family,
    )
}

pub fn toast_spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "font-rejected-toast",
        "Font rejected toast",
        Some(
            "one warning card per saved value in the main window · the body names the value · no button",
        ),
        draw_toast,
    )
}

pub fn draw_file(ui: &mut egui::Ui, theme: &Theme) {
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            frame(
                ui,
                th,
                &format!("{name} · default grid"),
                FRAME_W,
                |ui, th| {
                    state(ui, th, "rest", |ui| {
                        base_file_row(ui, th, "en", GOOD_FILE, File::Rest)
                    });
                    state(ui, th, "focus — no error", |ui| {
                        base_file_row(ui, th, "en", GOOD_FILE, File::Focus)
                    });
                    state(ui, th, "rejected", |ui| {
                        base_file_row(ui, th, "en", NOTES_TXT, File::Rejected)
                    });
                    state(
                        ui,
                        th,
                        "rejected · focus — no error, line box kept",
                        |ui| base_file_row(ui, th, "en", NOTES_TXT, File::FocusRejected),
                    );
                },
            );
        }
    });
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            frame(
                ui,
                th,
                &format!("{name} · override row"),
                FRAME_W,
                |ui, th| {
                    state(ui, th, "rest", |ui| {
                        override_file_row(ui, th, "en", GOOD_FILE, File::Rest)
                    });
                    state(ui, th, "focus — no error", |ui| {
                        override_file_row(ui, th, "en", GOOD_FILE, File::Focus)
                    });
                    state(ui, th, "rejected — not a usable font", |ui| {
                        override_file_row(ui, th, "en", NOTES_TXT, File::Rejected)
                    });
                    state(
                        ui,
                        th,
                        "rejected — no file at this path, same form",
                        |ui| override_file_row(ui, th, "en", REMOVED_FILE, File::Rejected),
                    );
                    state(
                        ui,
                        th,
                        "rejected · focus — no error, line box kept",
                        |ui| override_file_row(ui, th, "en", NOTES_TXT, File::FocusRejected),
                    );
                    state(
                        ui,
                        th,
                        "Use default on — read-only, the value in effect, never an error",
                        |ui| override_file_row(ui, th, "en", "", File::Default),
                    );
                },
            );
        }
    });
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            let label = format!("{name} · Terminal override row");
            frame(ui, th, &label, FRAME_W, |ui, th| {
                state(
                    ui,
                    th,
                    "the terminal can't use the file — same form",
                    |ui| override_file_row(ui, th, "en", NOT_A_FONT_NOTE, File::Rejected),
                );
                state(
                    ui,
                    th,
                    "the terminal can use the file, the preview can't — no error",
                    |ui| override_file_row(ui, th, "en", NEGATIVE_FILE, File::Rest),
                );
            });
        }
    });
    held::draw_file_steps(ui, theme);
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            let label = format!("{name} · grid and preview · the row grows, nothing else changes");
            frame(ui, th, &label, GRID_FRAME_W, rejected_grid);
        }
    });
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            let label = format!("{name} · long path · clipped at the end · hover shows all of it");
            frame(ui, th, &label, FRAME_W, |ui, th| {
                state(ui, th, "rest", |ui| {
                    override_file_row(ui, th, "en", LONG_PATH, File::Rest)
                });
                state(ui, th, "rejected", |ui| {
                    override_file_row(ui, th, "en", LONG_PATH, File::Rejected)
                });
                ui.add_space(HOVER_TOP.value());
                state(ui, th, "hover", |ui| {
                    let label = t_lang("en", "settings.appearance.font.override_custom_font_label");
                    override_font_row(ui, th, "en", label, false, |ui| {
                        let rect = file_input(ui, th, LONG_PATH, File::Rejected);
                        // 스크롤로 벗어나면 버블이 창 가장자리로 끌려오므로 보일 때만 그린다.
                        if ui.is_rect_visible(rect) {
                            Tooltip::new(LONG_PATH)
                                .mono(true)
                                .placement(TooltipPlacement::Top)
                                .id_source((*name, "font_file_long_tooltip"))
                                .show(ui, th, rect);
                        }
                        File::Rejected.line(file_line("en"))
                    });
                });
            });
        }
    });
    stage_row(ui, theme, |ui| {
        let [(_, mocha), (_, latte)] = themes(theme);
        frame(ui, &mocha, "Mocha · ko", FRAME_W, |ui, th| {
            override_file_row(ui, th, "ko", NOTES_TXT, File::Rejected)
        });
        frame(ui, &latte, "Latte · ja", FRAME_W, |ui, th| {
            override_file_row(ui, th, "ja", NOTES_TXT, File::Rejected)
        });
        let label = "Mocha · en · width 460 · checkbox under the line";
        frame(ui, &mocha, label, DROPPED_W, |ui, th| {
            override_file_row(ui, th, "en", NOTES_TXT, File::Rejected)
        });
        let label = "Latte · ja · width 360 · control at the row start";
        frame(ui, &latte, label, STACKED_W, |ui, th| {
            override_file_row(ui, th, "ja", NOTES_TXT, File::Rejected)
        });
        let label = "Mocha · ko · default grid · width 320";
        frame(ui, &mocha, label, BASE_NARROW_W, |ui, th| {
            base_file_row(ui, th, "ko", NOTES_TXT, File::Rejected)
        });
        let label =
            "Latte · live — demo judges by extension; app reads file; row below is clickable";
        frame(ui, &latte, label, FRAME_W, live_file_row);
    });

    meta(
        ui,
        theme,
        &[
            ("control", "Input · mono · field-width-lg · both grids"),
            ("rejected", "Input invalid: red edge, no ring"),
            (
                "focus",
                "Input focused only. Never invalid while focused, so the red ring never shows here",
            ),
            (
                "line",
                "caption · accent-danger-raised · measure-md · in the control column",
            ),
            ("input → line", "settings-row-caption-gap"),
            (
                "line position",
                "directly under the line holding the Input. The checkbox never sits between them",
            ),
            (
                "row height",
                "grows by the line. Only the rows below it move. Height changes only on a frame with no pointer button held",
            ),
            (
                "taking focus",
                "the line's box stays and its ink goes, so the rows don't jump. The height follows the result once focus is gone and no pointer button is held",
            ),
            (
                "value on focus, then fixed",
                "the box closes once, on leaving, or on the frame after the release if a pointer button is held",
            ),
            (
                "pointer held",
                "any pointer button, from the frame of the press through the frame that handles its release. The window losing focus counts as a release",
            ),
            (
                "box rule",
                "while the field is active or a pointer button is held, the box stays as it is. Otherwise it exists only for a rejected value. Active means focused or waiting for a verdict",
            ),
            (
                "ink rule",
                "edge and line ink show only when the field isn't active and the value is rejected. The edge doesn't wait for the pointer button's release. The line needs its box; if there's no box yet, it shows after the release",
            ),
            (
                "box motion",
                "snaps to its height in one frame, with no animation",
            ),
            (
                "long path",
                "clipped at the Input's end, no ellipsis in an editable value. Hover shows the whole value in a Tooltip",
            ),
            ("empty value", "no file set, no error"),
            ("Use default on", "read-only box, no invalid, no line"),
            (
                "Terminal row",
                "invalid and line only when the terminal can't use the file. The preview's result doesn't count",
            ),
            ("Save", "enabled in every state"),
        ],
        &[
            TokenChip::new(
                "input-border-invalid",
                "edge",
                egui::Color32::from(theme.input_border_invalid()),
            ),
            TokenChip::new(
                "accent-danger-raised",
                "line",
                egui::Color32::from(theme.accent_danger_raised()),
            ),
            TokenChip::without_color("font-size-caption", "line"),
            TokenChip::without_color("line-height-ui", "line"),
            TokenChip::without_color("settings-row-caption-gap", "Input → line"),
            TokenChip::without_color("measure-md", "line width"),
            TokenChip::without_color("field-width-lg", "Input"),
            TokenChip::without_color("settings-label-width", "line indent"),
            TokenChip::without_color("settings-label-gap", "line indent"),
        ],
    );
    note(
        ui,
        theme,
        "One line for every cause and every row. It doesn't ask for a new path: a file replaced with a good font works after a restart with the same value. No token was added.",
    );
    note(
        ui,
        theme,
        "Clicks. The control under the pointer at press must still be there at release. Changing the box after the release keeps it there when the box closes or opens, for one line or several.",
    );
    dont(
        ui,
        theme,
        "Don't disable Save, clear the value, or show the line while the field has focus: most values on the way to a full path are not a font.",
    );
    dont(
        ui,
        theme,
        "Don't change the box height while a pointer button is held, or keep an empty box after the release on a row with a usable or empty value.",
    );
}

const FAMILIES: [SearchableSelectOption<'static>; 5] = [
    SearchableSelectOption::new("D2Coding"),
    SearchableSelectOption::new("Fira Code"),
    SearchableSelectOption::new("JetBrains Mono"),
    SearchableSelectOption::new("Noto Sans Mono"),
    SearchableSelectOption::new("Sarasa Term K"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Rest,
    Rejected,
    Open,
    /// 목록은 닫혔고 칸만 있다.
    Held,
}

fn family_control(ui: &mut egui::Ui, th: &Theme, value: &str, st: Family) {
    let pinned = [SearchableSelectOption::with_value(
        "",
        t("settings.appearance.font_family_default"),
    )];
    let labels = SearchableSelectLabels {
        placeholder: t("settings.appearance.font_family_default"),
        search_placeholder: t("settings.appearance.font_search_placeholder"),
        empty: t("settings.appearance.font_no_match"),
        loading: t("settings.appearance.font_loading"),
    };
    let spec = SearchableSelectSpecimen {
        open: st == Family::Open,
        active: Some(4),
        focus_search: st == Family::Open,
        ..Default::default()
    };
    // 실제 목록처럼 행 배치를 밀지 않고 트리거 아래에 떠 있게 한다.
    let width = th.field_width_lg.value();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, th.select_height().value()),
        egui::Sense::hover(),
    );
    let floating = egui::Rect::from_min_size(rect.min, egui::vec2(width, OPEN_MIN_H.value()));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(floating)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    SearchableSelect::new("g_font_family_rejected", &FAMILIES, labels)
        .pinned(&pinned)
        .width(th.field_width_lg)
        .show_specimen(&mut child, th, Some(value), &spec);
}

fn family_line(lang: &str, st: Family) -> Option<FieldErrorLine<'static>> {
    let text = t_lang(lang, "settings.appearance.font.family_rejected");
    match st {
        Family::Rest => FieldErrorState::Clear.line(text),
        Family::Rejected => FieldErrorState::Shown.line(text),
        Family::Open | Family::Held => FieldErrorState::Held.line(text),
    }
}

fn base_family_row(ui: &mut egui::Ui, th: &Theme, value: &str, st: Family) {
    SettingsRow::new(t("settings.appearance.font_family_label")).show_with_error(
        ui,
        th,
        th.settings_label_width(),
        |ui| {
            family_control(ui, th, value, st);
            family_line("en", st)
        },
    );
}

fn override_family_row(ui: &mut egui::Ui, th: &Theme, lang: &str, st: Family) {
    override_family_row_with(ui, th, lang, "Noto Sans Mono", st);
}

fn override_family_row_with(ui: &mut egui::Ui, th: &Theme, lang: &str, value: &str, st: Family) {
    let label = t_lang(lang, "settings.appearance.font.override_family_label");
    override_font_row(ui, th, lang, label, false, |ui| {
        family_control(ui, th, value, st);
        family_line(lang, st)
    });
}

/// Appearance › Font family rejected.
pub fn draw_family(ui: &mut egui::Ui, theme: &Theme) {
    stage_row(ui, theme, |ui| {
        for (name, th) in &themes(theme) {
            frame(
                ui,
                th,
                &format!("{name} · family rows"),
                FRAME_W,
                |ui, th| {
                    state(ui, th, "default grid · rest", |ui| {
                        base_family_row(ui, th, "JetBrains Mono", Family::Rest)
                    });
                    state(ui, th, "default grid · rejected", |ui| {
                        base_family_row(ui, th, "Noto Sans Mono", Family::Rejected)
                    });
                    state(ui, th, "override row · rejected", |ui| {
                        override_family_row(ui, th, "en", Family::Rejected)
                    });
                    state(
                        ui,
                        th,
                        "Terminal override row · the terminal can use the font, the preview can't — no line",
                        |ui| override_family_row(ui, th, "en", Family::Rest),
                    );
                    ui.scope(|ui| {
                        ui.set_min_height(OPEN_MIN_H.value());
                        state(
                            ui,
                            th,
                            "override row · rejected, list open — line box kept",
                            |ui| override_family_row(ui, th, "en", Family::Open),
                        );
                    });
                },
            );
        }
    });
    held::draw_family_settle(ui, theme);
    stage_row(ui, theme, |ui| {
        let [(_, mocha), (_, latte)] = themes(theme);
        frame(ui, &mocha, "Mocha · ko", FRAME_W, |ui, th| {
            override_family_row(ui, th, "ko", Family::Rejected)
        });
        frame(ui, &latte, "Latte · ja", FRAME_W, |ui, th| {
            override_family_row(ui, th, "ja", Family::Rejected)
        });
    });
    meta(
        ui,
        theme,
        &[
            (
                "trigger",
                "unchanged in every state. SearchableSelect has no invalid state",
            ),
            ("line", "same form and position as the file field's line"),
            ("list open", "line box kept, no ink"),
            (
                "list closes",
                "same box rule as the file field. The family row has no edge, so nothing shows until the line's box opens",
            ),
            ("Save", "enabled in every state"),
            ("toast", "font-rejected-toast, with the family name"),
        ],
        &[
            TokenChip::new(
                "accent-danger-raised",
                "line",
                egui::Color32::from(theme.accent_danger_raised()),
            ),
            TokenChip::without_color("settings-row-caption-gap", "control → line"),
        ],
    );
    note(
        ui,
        theme,
        "It names the file as the fault, which is what the user can act on. No token and no contract row was added.",
    );
}

fn toast_text(lang: &str, value: &str) -> String {
    t_lang(lang, "toast.font_rejected").replacen("{}", value, 1)
}

/// 시안 `MainCorner`: 메인 창 오른쪽 아래 모서리. surface 위 토스트 스택과 상태 바.
fn main_corner(ui: &mut egui::Ui, th: &Theme, label: &str, cards: &[String]) {
    wrap_item(ui, |ui| {
        ui.push_id(label, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                mono_label(ui, th, label);
                let pad = th.spacing_md.value();
                let bottom = th.toast_stack_offset_bottom().value();
                let frame = egui::Frame::new()
                    .fill(th.bg_app().to_egui())
                    .stroke(egui::Stroke::new(
                        th.border_width.value(),
                        th.border_frame().to_egui(),
                    ))
                    .corner_radius(th.corner_radius.value())
                    .inner_margin(egui::Margin {
                        left: pad as i8,
                        right: pad as i8,
                        top: pad as i8,
                        bottom: bottom as i8,
                    })
                    .show(ui, |ui| {
                        let inner = CORNER_W.value() - pad * 2.0;
                        ui.set_width(inner);
                        ui.set_min_height(CORNER_MIN_H.value() - pad - bottom);
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                            for card in cards.iter().rev() {
                                toast_card::draw_single_card(
                                    ui,
                                    th,
                                    ToastKind::Warning,
                                    card,
                                    &[],
                                    1.0,
                                );
                            }
                        });
                    });
                // 시안의 바는 테두리 안쪽에 놓인다.
                let rect = frame.response.rect.shrink(th.border_width.value());
                let bar = egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom() - th.status_bar_height.value()),
                    rect.max,
                );
                let painter = ui.painter();
                painter.rect_filled(bar, 0.0, th.bg_panel().to_egui());
                painter.hline(
                    bar.x_range(),
                    bar.top(),
                    egui::Stroke::new(
                        th.border_width.value(),
                        th.separator.to_egui_premultiplied(),
                    ),
                );
            });
        });
    });
}

/// Font rejected toast.
pub fn draw_toast(ui: &mut egui::Ui, theme: &Theme) {
    let [(_, mocha), (_, latte)] = themes(theme);
    stage_row(ui, theme, |ui| {
        let path = [toast_text("en", NOTES_TXT)];
        main_corner(ui, &mocha, "Mocha · once, path", &path);
        main_corner(ui, &latte, "Latte · once, path", &path);
        let long = [toast_text("en", TOAST_LONG)];
        let label = "long path, already shortened · no such file reads the same";
        main_corner(ui, &mocha, &format!("Mocha · {label}"), &long);
        main_corner(ui, &latte, &format!("Latte · {label}"), &long);
        main_corner(ui, &mocha, "Mocha · ko", &[toast_text("ko", NOTES_TXT)]);
        main_corner(ui, &latte, "Latte · ja", &[toast_text("ja", NOTES_TXT)]);
        main_corner(
            ui,
            &mocha,
            "Mocha · two different values, newest at the bottom",
            &[toast_text("en", NOTES_TXT), toast_text("en", REMOVED_FILE)],
        );
        main_corner(
            ui,
            &latte,
            "Latte · beside the language pack font notice",
            &[
                crate::i18n::t_fmt("i18n.warn.font_unresolved", "Noto Sans KR"),
                toast_text("en", NOTES_TXT),
            ],
        );
    });
    meta(
        ui,
        theme,
        &[
            (
                "card",
                "Toast variant warning, which is ToastKind::Warning in the app",
            ),
            ("boxes", "rail and body, no icon, no hint, no button"),
            (
                "value",
                "in the body, same font as the body. Long paths shorten as the language pack notice does",
            ),
            (
                "long text",
                "wraps inside toast-max-width, breaking anywhere in a path",
            ),
            ("once", "per value. Several surfaces, one card"),
            ("lifetime", "overlay.toast_duration_ms, unchanged"),
            ("position", "main window stack, toast-stack-offset-bottom"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "rail",
                egui::Color32::from(theme.toast_accent_warning()),
            ),
            TokenChip::without_color("toast-max-width", "card cap"),
            TokenChip::without_color("toast-stack-offset-bottom", "main window offset"),
        ],
    );
    note(
        ui,
        theme,
        "The same key takes a path or a family name. It doesn't ask for a new value: a file replaced with a good font works after a restart with the same value. Its danger sibling, ToastKind::Error, is for an action that failed, which this isn't. No token was added.",
    );
    dont(
        ui,
        theme,
        "Don't raise this toast from the preview's check, or add a button: the row in Settings is where the value is changed.",
    );
}
