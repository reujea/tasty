//! Settings › Appearance 색 행 — Default일 때 hex 칸은 disabled가 아니라 읽기 전용 Input이다.
//! 시안 Spec "Appearance › colour rows"를 Mocha·Latte 패널로 옮긴다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Input, checkbox};

use crate::catalog::spec::{StageVariant, TokenChip, meta, stage};

/// 시안 Spec 패널의 바깥 폭 `--tasty-size-360`(시안은 border-box라 padding·border 포함).
/// 공개 역할 토큰이 없어 갤러리 무대 치수로 둔다.
const THEME_PANEL_WIDTH: LogicalPx = LogicalPx(360.0);

/// (필드 이름, 기본값, override 값) — 시안과 같은 세 행.
const ROWS: [(&str, &str, Option<&str>); 3] = [
    ("accent", "#89b4fa", None),
    ("surface_bg", "#1e1e2e", Some("#181825")),
    ("selection", "#45475a", None),
];

/// Terminal-specific 그룹의 행 순서. 본체 `color_groups`와 같다.
const TERMINAL_FIELDS: [&str; 5] = [
    "selection_bg",
    "vi_cursor_bg",
    "search_match_bg",
    "search_match_active_bg",
    "search_match_active_fg",
];

thread_local! {
    // 패널 둘 × 행 셋의 hex 버퍼. 선택·복사를 시험할 수 있게 프레임마다 다시 만들지 않는다.
    static BUFS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    // Terminal-specific 예제의 패널 둘 × 행 다섯 hex 버퍼.
    static TERMINAL_BUFS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

fn hex_color(hex: &str) -> egui::Color32 {
    egui::Color32::from_hex(hex).unwrap_or(egui::Color32::PLACEHOLDER)
}

fn color_row(ui: &mut egui::Ui, th: &Theme, row: (&str, &str, Option<&str>), buf: &mut String) {
    let (field, base, ov) = row;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), th.input_height().value()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            let dot = th.status_dot_size().value();
            let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(dot, dot), egui::Sense::hover());
            if ov.is_some() {
                ui.painter().circle_filled(
                    dot_rect.center(),
                    dot * 0.5,
                    egui::Color32::from(th.accent_primary()),
                );
            }
            let name_fg = if ov.is_some() {
                th.text_primary()
            } else {
                th.text_muted()
            };
            ui.label(
                egui::RichText::new(field)
                    .monospace()
                    .size(th.font_size_term_sm.value())
                    .color(egui::Color32::from(name_fg)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut use_default = ov.is_none();
                checkbox(
                    ui,
                    th,
                    &mut use_default,
                    "Default",
                    true,
                    tasty_ui_widgets::Ground::Panel,
                );
                let sw = th.swatch_size().value();
                let (sw_rect, _) = ui.allocate_exact_size(egui::vec2(sw, sw), egui::Sense::hover());
                let fill = hex_color(ov.unwrap_or(base));
                let fill = if ov.is_some() {
                    fill
                } else {
                    fill.gamma_multiply(th.state_dim_opacity())
                };
                let radius = th.swatch_radius().value();
                ui.painter().rect_filled(sw_rect, radius, fill);
                ui.painter().rect_stroke(
                    sw_rect,
                    radius,
                    egui::Stroke::new(
                        th.border_width.value(),
                        egui::Color32::from(th.border_strong()),
                    ),
                    egui::StrokeKind::Inside,
                );
                Input::new()
                    .mono(true)
                    .read_only(ov.is_none())
                    .width(th.field_width_xs.value())
                    .show(ui, th, buf);
            });
        },
    );
}

fn panel(
    ui: &mut egui::Ui,
    th: &Theme,
    name: &str,
    rows: &[(&str, &str, Option<&str>)],
    bufs: &mut [String],
) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_default()),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8));
    // 바깥 폭이 시안 값이 되도록 padding·border를 뺀 폭을 콘텐츠에 준다.
    let content_w = THEME_PANEL_WIDTH.value() - frame.total_margin().sum().x;
    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(content_w);
            ui.spacing_mut().item_spacing.y = th.label_detail_gap.value();
            ui.label(
                egui::RichText::new(name)
                    .size(th.font_size_caption.value())
                    .color(egui::Color32::from(th.text_muted())),
            );
            ui.add_space(th.spacing_xs.value());
            for (row, buf) in rows.iter().copied().zip(bufs.iter_mut()) {
                ui.push_id((name, row.0), |ui| color_row(ui, th, row, buf));
            }
        });
    });
}

/// Appearance 색 행 — Default의 hex는 읽기 전용 Input, override는 일반 Input.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    BUFS.with(|b| {
        let mut slot = b.borrow_mut();
        let bufs = slot.get_or_insert_with(|| {
            (0..2)
                .flat_map(|_| {
                    ROWS.iter()
                        .map(|(_, base, ov)| ov.unwrap_or(base).to_string())
                })
                .collect()
        });
        let (left, right) = bufs.split_at_mut(ROWS.len());
        stage(ui, theme, StageVariant::Column, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                panel(ui, &mocha, "Mocha", &ROWS, left);
                panel(ui, &latte, "Latte", &ROWS, right);
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("Default hex", "Input readOnly"),
            ("box", "state-disabled fill + border (same as disabled)"),
            ("value ink", "text-secondary"),
            ("select · copy", "allowed; focusable"),
            ("override", "normal Input"),
            ("disabled", "reserved for an unavailable control"),
        ],
        &[
            TokenChip::new(
                "input-readonly-bg",
                "→ state-disabled-fill",
                egui::Color32::from(theme.input_readonly_bg()),
            ),
            TokenChip::new(
                "input-readonly-border",
                "→ state-disabled-border",
                egui::Color32::from(theme.input_readonly_border()),
            ),
            TokenChip::new(
                "input-readonly-fg",
                "→ text-secondary",
                egui::Color32::from(theme.input_readonly_fg()),
            ),
        ],
    );
}

/// Terminal-specific 다섯 행의 테마 기본값. 행 이름은 본체와 같은 키 이름이다.
fn terminal_bases(th: &Theme) -> Vec<String> {
    let c = th.to_colors();
    [
        c.selection_bg,
        c.vi_cursor_bg,
        c.search_match_bg,
        c.search_match_active_bg,
        c.search_match_active_fg,
    ]
    .map(|v| v.to_hex())
    .to_vec()
}

/// (이름, 기본값) 목록을 모두 Default 상태인 행으로 본다.
fn default_rows<'a>(
    rows: &'a [(&'static str, String)],
) -> Vec<(&'static str, &'a str, Option<&'a str>)> {
    rows.iter().map(|(f, b)| (*f, b.as_str(), None)).collect()
}

/// Appearance › Colors › Terminal-specific — 활성 매치 글자색 행이 그 바탕색 행 바로 뒤에 온다.
/// 기본값은 각 테마 파일의 값이고, 모든 행이 Default 상태다.
pub fn draw_terminal_rows(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    let mocha_bases = terminal_bases(&mocha);
    let latte_bases = terminal_bases(&latte);
    let rows_of = |bases: &[String]| -> Vec<(&'static str, String)> {
        TERMINAL_FIELDS
            .iter()
            .copied()
            .zip(bases.iter().cloned())
            .collect()
    };
    let (mocha_rows, latte_rows) = (rows_of(&mocha_bases), rows_of(&latte_bases));
    TERMINAL_BUFS.with(|b| {
        let mut slot = b.borrow_mut();
        let bufs = slot.get_or_insert_with(|| {
            mocha_bases
                .iter()
                .chain(latte_bases.iter())
                .cloned()
                .collect()
        });
        let (left, right) = bufs.split_at_mut(TERMINAL_FIELDS.len());
        stage(ui, theme, StageVariant::Column, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                panel(
                    ui,
                    &mocha,
                    "Mocha · Terminal-specific (5)",
                    &default_rows(&mocha_rows),
                    left,
                );
                panel(
                    ui,
                    &latte,
                    "Latte · Terminal-specific (5)",
                    &default_rows(&latte_rows),
                    right,
                );
            });
        });
    });

    meta(
        ui,
        theme,
        &[
            ("group", "Terminal-specific · 5 rows"),
            (
                "order",
                "selection_bg · vi_cursor_bg · search_match_bg · search_match_active_bg · search_match_active_fg",
            ),
            ("label", "the key name, like every row (no translation)"),
            ("swatch", "unchanged plain swatch"),
            ("Reset all", "includes this row"),
            ("total", "6 groups / 47 rows"),
        ],
        &[TokenChip::new(
            "search-match-active-fg",
            "the value this row edits",
            egui::Color32::from(theme.search_match_active_fg),
        )],
    );
}

/// 본체 Colors 헤더의 설명 문장(`settings.appearance.colors.intro` 영어 원문).
fn colors_intro() -> &'static str {
    crate::i18n::t("settings.appearance.colors.intro")
}

/// Colors 헤더 한 줄 — 설명 + 오른쪽 Reset all. 본체 `draw_appearance_colors` 헤더와 같은 배치다.
fn colors_header(ui: &mut egui::Ui, th: &Theme, changed: usize) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if changed > 0 {
                format!("Reset all ({changed})")
            } else {
                "Reset all".to_string()
            };
            Button::new(&label)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(changed > 0)
                .show(ui, th);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(colors_intro())
                            .small()
                            .color(egui::Color32::from(th.text_muted())),
                    )
                    .wrap(),
                );
            });
        });
    });
}

fn header_panel(ui: &mut egui::Ui, th: &Theme, name: &str) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from(th.bg_panel()))
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            egui::Color32::from(th.border_default()),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8));
    let content_w = THEME_PANEL_WIDTH.value() - frame.total_margin().sum().x;
    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(content_w);
            ui.spacing_mut().item_spacing.y = th.label_detail_gap.value();
            ui.label(
                egui::RichText::new(name)
                    .size(th.font_size_caption.value())
                    .color(egui::Color32::from(th.text_muted())),
            );
            ui.add_space(th.spacing_xs.value());
            for changed in [0, 3] {
                ui.push_id((name, changed), |ui| colors_header(ui, th, changed));
                ui.add_space(th.spacing_sm.value());
            }
        });
    });
}

/// Appearance › Colors 헤더 — override 가 없으면 Reset all 은 비활성, 있으면 개수를 붙인다.
pub fn draw_header(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            header_panel(ui, &mocha, "Mocha");
            header_panel(ui, &latte, "Latte");
        });
    });

    meta(
        ui,
        theme,
        &[
            ("intro", "caption · text-muted · wraps"),
            ("action", "Button ghost sm · right"),
            ("label", "Reset all · Reset all (N) with N overrides"),
            ("no override", "disabled — disabled ink, no box"),
            ("scope", "clears the palette overrides only"),
        ],
        &[TokenChip::new(
            "state-disabled-fg",
            "disabled ink",
            egui::Color32::from(theme.state_disabled_fg()),
        )],
    );
}
