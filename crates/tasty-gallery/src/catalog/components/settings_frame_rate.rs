//! Terminal › Frame rate 예제. 실제 설정 저장은 하지 않는다.
//!
//! 출력·입력 다시 그리기 상한 두 행(Select, field-width-lg)과 그 아래 muted 캡션 한 줄이다. 적용
//! 안내는 두지 않는다. 선택지와 표기는 본체와 같은 `tasty_settings::redraw_fps_rows`로, 본문은 본체와 같은
//! `tasty_ui_widgets::frame_rate_section`으로 그린다. 가장 빠른
//! 모니터보다 높게 저장된 값은 숫자 순서 자리(Match monitor 앞)에 "above monitor" 라벨로 남는다.
//! 열린 목록은 본체 팝업과 같은 몸통(`select_list`)과 폭(`select_list_width`)을 팝오버 틀에 고정해 그린다.

use std::cell::RefCell;

use tasty_settings::{
    DEFAULT_INPUT_REDRAW_FPS, DEFAULT_OUTPUT_REDRAW_FPS, RedrawFpsCopy, UNLIMITED_REDRAW_FPS,
    redraw_fps_rows,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    FpsField, FrameRateCopy, HelpHint, TooltipPlacement, frame_rate_section, select_list,
    select_list_width,
};

use crate::catalog::spec::{self, StageVariant, TokenChip, wrap_item};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::t_lang;

use super::settings_remote_transfer::WIDTH;

/// 디자인 견본의 좁은 폭(size-360). 행이 라벨 아래로 쌓인다.
const NARROW: LogicalPx = LogicalPx(360.0);
/// 열린 HelpHint 견본의 틀 폭(size-480).
const HINT_FRAME_W: LogicalPx = LogicalPx(480.0);
/// 열린 HelpHint 견본 아래 말풍선 자리(size-160).
const HINT_FRAME_BOTTOM: LogicalPx = LogicalPx(160.0);
/// 견본 페이지가 가정하는 가장 빠른 모니터(60Hz, millihertz).
const PAGE_MONITOR: Option<u32> = Some(60_000);

const OUTPUT: &str = "settings.frame_rate.output";
const OUTPUT_DESC: &str = "settings.frame_rate.output_desc";
const INPUT: &str = "settings.frame_rate.input";
const INPUT_DESC: &str = "settings.frame_rate.input_desc";
const CAPTION: &str = "settings.frame_rate.caption";

/// 한 언어의 선택지 행. 본체 탭과 같은 함수(`redraw_fps_rows`)·같은 번역 키를 쓴다.
fn options(lang: &str, max_mhz: Option<u32>, saved: i64) -> Vec<(i64, String)> {
    let copy = RedrawFpsCopy {
        fps: t_lang(lang, "settings.frame_rate.fps"),
        above_monitor: t_lang(lang, "settings.frame_rate.fps_above_monitor"),
        match_monitor: t_lang(lang, "settings.frame_rate.match_monitor"),
    };
    redraw_fps_rows(max_mhz, saved, &copy, tasty_i18n::fill_args)
}

/// 시안 `FrPage`·`FrHintOpen` 의 견본 틀. 정본은 bg-panel 바탕에 border-frame 테두리다.
fn page_card(ui: &mut egui::Ui, th: &Theme, width: LogicalPx, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(kit::panel_fill(th))
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_frame().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .show(ui, |ui| {
            ui.set_width(width.value());
            ui.vertical(|ui| {
                ui.set_width(width.value());
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                add(ui);
            });
        });
}

/// 설정 섹션 한 벌(두 행 + 캡션).
struct Page {
    label: &'static str,
    lang: &'static str,
    width: LogicalPx,
    output: i64,
    input: i64,
}

fn page(label: &'static str, lang: &'static str, output: i64, input: i64) -> Page {
    Page {
        label,
        lang,
        width: WIDTH,
        output,
        input,
    }
}

const DEFAULTS: (i64, i64) = (DEFAULT_OUTPUT_REDRAW_FPS, DEFAULT_INPUT_REDRAW_FPS);

thread_local! {
    /// 기본 · 입력 Match monitor — Mocha 둘, Latte 둘.
    static TWO_CAPS: RefCell<Vec<Page>> = RefCell::new(
        (0..2)
            .flat_map(|_| {
                [
                    page("default · output 30 · input 60", "en", DEFAULTS.0, DEFAULTS.1),
                    page("input · Match monitor", "en", DEFAULTS.0, UNLIMITED_REDRAW_FPS),
                ]
            })
            .collect(),
    );
    /// 60Hz 에서 출력 144 저장 — Mocha·Latte 짝.
    static ABOVE_PAIR: RefCell<Vec<Page>> = RefCell::new(
        (0..2).map(|_| page("60 Hz monitor · output saved 144", "en", 144, DEFAULTS.1)).collect(),
    );
    /// 언어별 범위 밖 트리거. 열린 목록은 따로 그린다.
    static ABOVE_LANGS: RefCell<Vec<Page>> = RefCell::new(vec![
        page("en · 60 Hz · input saved 240 — the longest trigger", "en", DEFAULTS.0, 240),
        page("ko · 60 Hz · input saved 240", "ko", DEFAULTS.0, 240),
        page("ja · 60 Hz · output saved 144", "ja", 144, DEFAULTS.1),
        page("zh-Hans · 60 Hz · output saved 144", "zh-Hans", 144, DEFAULTS.1),
    ]);
    /// 언어와 좁은 폭.
    static LANGS: RefCell<Vec<Page>> = RefCell::new(vec![
        page("ko · 560", "ko", DEFAULTS.0, DEFAULTS.1),
        page("ja · 560 · input Match monitor", "ja", DEFAULTS.0, UNLIMITED_REDRAW_FPS),
        page("zh-Hans · 560", "zh-Hans", DEFAULTS.0, DEFAULTS.1),
        Page { width: NARROW, ..page("ko · narrow 360 — rows stack", "ko", DEFAULTS.0, DEFAULTS.1) },
        Page { width: NARROW, ..page("ja · narrow 360 · above monitor", "ja", 144, DEFAULTS.1) },
        Page {
            width: NARROW,
            ..page("en · narrow 360 · Match monitor", "en", DEFAULTS.0, UNLIMITED_REDRAW_FPS)
        },
    ]);
}

fn themes(theme: &Theme) -> [(&'static str, Theme); 2] {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ]
}

/// 시안 `FrPair` — 테마 하나의 바탕(bg-app) 위에 캡션과 견본들을 줄바꿈 줄로 둔다.
fn theme_panel(ui: &mut egui::Ui, th: &Theme, name: &str, add: impl FnOnce(&mut egui::Ui)) {
    // 시안 FrPair 는 stage 한 줄을 넘지 않고 안에서 견본을 줄바꿈한다. 줄 폭(max_rect)에서 바탕
    // 안쪽 여백을 빼 견본 줄의 폭으로 둔다. 남은 폭만 쓰면 첫 프레임에 견본이 한 줄로 넘친다.
    let inner_max = ui.max_rect().width() - th.spacing_md.value() * 2.0;
    wrap_item(ui, |ui| {
        egui::Frame::new()
            .fill(th.bg_app().to_egui())
            .corner_radius(th.corner_radius.value())
            .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                    ui.label(
                        egui::RichText::new(name)
                            .size(th.font_size_caption.value())
                            .color(th.text_muted().to_egui()),
                    );
                    ui.set_max_width(inner_max);
                    wrap_row(ui, th, add);
                });
            });
    });
}

/// 시안 `FrCase` — micro 캡션 아래 견본 하나.
fn case(ui: &mut egui::Ui, th: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        ui.label(
            egui::RichText::new(label)
                .size(th.font_size_micro.value())
                .color(th.text_muted().to_egui()),
        );
        add(ui);
    });
}

/// 시안 `FrPage` — 본체 Frame rate 섹션과 같은 공용 본문(`frame_rate_section`). 트리거는 실제 Select 다.
fn page_frame(ui: &mut egui::Ui, th: &Theme, p: &mut Page) {
    let lang = p.lang;
    let t = |key| t_lang(lang, key);
    let copy = FrameRateCopy {
        output: t(OUTPUT),
        output_hint: t(OUTPUT_DESC),
        input: t(INPUT),
        input_hint: t(INPUT_DESC),
        caption: t(CAPTION),
    };
    let output = options(lang, PAGE_MONITOR, p.output);
    let input = options(lang, PAGE_MONITOR, p.input);
    page_card(ui, th, p.width, |ui| {
        kit::region_sym(ui, th.spacing_lg, th.spacing_lg, |ui| {
            frame_rate_section(
                ui,
                th,
                &copy,
                FpsField {
                    salt: "frame_rate_output",
                    choices: &output,
                    value: &mut p.output,
                },
                FpsField {
                    salt: "frame_rate_input",
                    choices: &input,
                    value: &mut p.input,
                },
            );
        });
    });
}

/// 시안 `FrList` — 트리거 아래 열린 목록을 고정 틀로 그린다. 본체 팝업과 같은 틀 토큰
/// (menu-bg · menu-border · menu-radius · shadow-popover · popup-content-margin)과 몸통이다.
fn open_list(ui: &mut egui::Ui, th: &Theme, lang: &str, max_mhz: Option<u32>, saved: i64) {
    let rows = options(lang, max_mhz, saved);
    let idx = rows.iter().position(|(v, _)| *v == saved);
    let labels: Vec<&str> = rows.iter().map(|(_, l)| l.as_str()).collect();
    let width = select_list_width(ui.ctx(), th, &labels, th.field_width_lg.value());
    let bw = th.border_width.value();
    egui::Frame::new()
        .fill(th.menu_bg().to_egui())
        .stroke(egui::Stroke::new(bw, th.menu_border().to_egui()))
        .corner_radius(th.menu_radius().value())
        .shadow(th.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(th.popup_content_margin().value() as i8))
        .show(ui, |ui| {
            ui.set_width(width);
            // 고정 견본이라 고른 행을 저장하지 않는다.
            select_list(ui, th, &labels, idx, width);
        });
}

/// 시안 `FrHintOpen` — 행 라벨과 열린 HelpHint. 말풍선이 틀 안에 들도록 라벨을 말풍선 반 폭만큼
/// 안쪽에서 시작하고 아래에 말풍선 자리를 둔다.
fn hint_open(ui: &mut egui::Ui, th: &Theme, lang: &'static str, output: bool) {
    let t = |key| t_lang(lang, key);
    let (label, desc) = if output {
        (t(OUTPUT), t(OUTPUT_DESC))
    } else {
        (t(INPUT), t(INPUT_DESC))
    };
    page_card(ui, th, HINT_FRAME_W, |ui| {
        let m = egui::Margin {
            left: (th.tooltip_max_width().value() / 2.0) as i8,
            right: th.spacing_md.value() as i8,
            top: th.spacing_md.value() as i8,
            bottom: 0,
        };
        kit::region(ui, m, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = th.help_hint_gap().value();
                // 본체 설정 행(SettingsRow)처럼 라벨 뒤에 콜론을 붙이지 않는다.
                ui.label(
                    egui::RichText::new(label)
                        .size(th.font_size_body.value())
                        .color(th.text_secondary().to_egui()),
                );
                HelpHint::new(desc)
                    .placement(TooltipPlacement::Bottom)
                    .open(true)
                    .id_source(("frame_rate_hint", lang, output))
                    .show(ui, th);
            });
            ui.add_space(HINT_FRAME_BOTTOM.value());
        });
    });
}

fn draw_pages(ui: &mut egui::Ui, th: &Theme, salt: &str, pages: &mut [Page]) {
    for (i, p) in pages.iter_mut().enumerate() {
        let label = p.label;
        case(ui, th, label, |ui| {
            ui.push_id((salt, i), |ui| page_frame(ui, th, p));
        });
    }
}

/// `terminal-frame-rate-two-redraw-caps`.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let themes = themes(theme);
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            TWO_CAPS.with(|s| {
                let mut s = s.borrow_mut();
                for ((name, th), pages) in themes.iter().zip(s.chunks_mut(2)) {
                    theme_panel(ui, th, name, |ui| draw_pages(ui, th, name, pages));
                }
            });
        });
    });
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            for lang in ["en", "ko", "ja", "zh-Hans"] {
                for output in [true, false] {
                    let row = if output { "output" } else { "input" };
                    case(ui, theme, &format!("{lang} · {row} HelpHint open"), |ui| {
                        ui.push_id(("hint", lang, row), |ui| hint_open(ui, theme, lang, output));
                    });
                }
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "place",
                "Terminal L2, last section after Performance · no heading above the rows (the L2 entry names the section, as for Performance) · no apply note",
            ),
            (
                "rows",
                "settings Row: label column + HelpHint (?) · gap settings-label-gap · min-height settings-row-min-height · rows gap settings-row-gap · output row first, input row second",
            ),
            (
                "control",
                "the shared Select · width field-width-lg 200 · height select-height",
            ),
            (
                "caption",
                "one, under the input row · caption text-muted · wraps at measure-md",
            ),
            ("defaults", "output 30 fps · input 60 fps"),
            ("applies", "on save, to every open window (no restart)"),
            (
                "not added",
                "no apply note · no per-window applied value · no restart hint",
            ),
        ],
        &[
            TokenChip::without_color("field-width-lg", "Select width (200)"),
            TokenChip::without_color("select-height", "trigger height"),
            TokenChip::without_color("settings-label-width", "label column"),
            TokenChip::new("text-muted", "caption ink", theme.text_muted().to_egui()),
        ],
    );
}

/// `terminal-frame-rate-choice-list-by-monitor`.
pub fn draw_choice_list(ui: &mut egui::Ui, theme: &Theme) {
    let lists: [(&str, Option<u32>, i64); 4] = [
        (
            "60 Hz (or no report) · 4 items · input 60 selected",
            PAGE_MONITOR,
            DEFAULTS.1,
        ),
        (
            "144 Hz · 6 items · output 30 selected",
            Some(144_000),
            DEFAULTS.0,
        ),
        (
            "240 Hz or faster · 7 items",
            Some(240_000),
            UNLIMITED_REDRAW_FPS,
        ),
        ("50 Hz · 3 items", Some(50_000), DEFAULTS.0),
    ];
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            for (name, th) in themes(theme).iter() {
                theme_panel(ui, th, name, |ui| {
                    for (label, mhz, saved) in lists {
                        case(ui, th, label, |ui| open_list(ui, th, "en", mhz, saved));
                    }
                });
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "choices",
                "15 · 30 · 60 · 120 · 144 · 240 · Match monitor (stored -1) · the same list for both rows",
            ),
            (
                "shown",
                "values ≤ the fastest connected monitor's rate (rounded) · Match monitor always, last",
            ),
            ("no report", "treated as 60 Hz"),
            (
                "selected",
                "MenuItem selected (text-primary + accent check)",
            ),
            (
                "refresh",
                "built from the monitors connected when the Frame rate section draws (every trigger click)",
            ),
            (
                "list frame",
                "menu-bg · menu-border · menu-radius · shadow-popover · at least field-width-lg, wider when a row needs it",
            ),
        ],
        &[
            TokenChip::new("menu-bg", "open list ground", theme.menu_bg().to_egui()),
            TokenChip::new(
                "menu-border",
                "open list edge",
                theme.menu_border().to_egui(),
            ),
            TokenChip::without_color("shadow-popover", "floating list"),
        ],
    );
}

/// `terminal-frame-rate-saved-above-monitor`.
pub fn draw_above_monitor(ui: &mut egui::Ui, theme: &Theme) {
    let themes = themes(theme);
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            ABOVE_PAIR.with(|s| {
                let mut s = s.borrow_mut();
                for ((name, th), pages) in themes.iter().zip(s.chunks_mut(1)) {
                    theme_panel(ui, th, name, |ui| {
                        draw_pages(ui, th, name, pages);
                        case(ui, th, "open list", |ui| {
                            open_list(ui, th, "en", PAGE_MONITOR, 144);
                        });
                    });
                }
            });
        });
    });
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            ABOVE_LANGS.with(|s| {
                for (i, p) in s.borrow_mut().iter_mut().enumerate() {
                    let (lang, label) = (p.lang, p.label);
                    case(ui, theme, label, |ui| {
                        ui.push_id(("above", i), |ui| page_frame(ui, theme, p));
                    });
                    // 영어는 트리거만 보인다(가장 긴 트리거). 나머지 언어는 열린 목록을 옆에 둔다.
                    if lang != "en" {
                        let saved = if p.output == DEFAULTS.0 {
                            p.input
                        } else {
                            p.output
                        };
                        case(ui, theme, &format!("{lang} · open list"), |ui| {
                            open_list(ui, theme, lang, PAGE_MONITOR, saved);
                        });
                    }
                }
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "when",
                "the saved value is a listed choice above the fastest connected monitor",
            ),
            (
                "trigger",
                "its own template “{} fps (above monitor)” · Select ink unchanged, no warning colour",
            ),
            (
                "list",
                "the saved value in numeric order, before Match monitor, selected, same template",
            ),
            ("applied value", "not shown"),
            ("write", "unchanged until the user picks another value"),
        ],
        &[],
    );
}

/// `terminal-frame-rate-languages-and-narrow`.
pub fn draw_languages(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        wrap_row(ui, theme, |ui| {
            LANGS.with(|s| draw_pages(ui, theme, "langs", &mut s.borrow_mut()));
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "stack",
                "label over control · gap settings-row-stack-gap · control keeps field-width-lg (max 100%)",
            ),
            ("label", "wraps in the label column, never ellipsises"),
            ("caption", "wraps at the column width"),
        ],
        &[],
    );
}

/// Terminal › Frame rate 표본. catalog 의 Performance 표본 뒤에 이어 붙는다.
pub fn specs() -> Vec<crate::catalog::Spec> {
    use crate::catalog::Spec;
    vec![
        Spec {
            id: "terminal-frame-rate-two-redraw-caps",
            title: "Terminal › Frame rate — two redraw caps, one choice list",
            when: Some(
                "output and input caps in their own section after Performance · Select at field-width-lg · one muted caption · no apply note · HelpHints open (en / ko / ja / zh-Hans)",
            ),
            draw,
        },
        Spec {
            id: "terminal-frame-rate-choice-list-by-monitor",
            title: "Terminal › Frame rate — the choice list follows the fastest monitor",
            when: Some(
                "open lists at 60 Hz (or no report) · 144 Hz · 240 Hz · 50 Hz · Match monitor always last",
            ),
            draw: draw_choice_list,
        },
        Spec {
            id: "terminal-frame-rate-saved-above-monitor",
            title: "Terminal › Frame rate — a saved value above every monitor",
            when: Some(
                "“{} fps (above monitor)” on the trigger and in numeric order before Match monitor · no unknown suffix · ko / ja / zh-Hans",
            ),
            draw: draw_above_monitor,
        },
        Spec {
            id: "terminal-frame-rate-languages-and-narrow",
            title: "Terminal › Frame rate — languages and narrow windows",
            when: Some("ko / ja / zh-Hans at 560 · narrow 360 rows stack"),
            draw: draw_languages,
        },
    ]
}

/// 시안 stage·FrPair 의 줄바꿈 줄(gap space-lg, align-items flex-start). 높이가 다른 견본도 위에 맞춘다.
fn wrap_row(ui: &mut egui::Ui, th: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
        |ui| {
            ui.spacing_mut().item_spacing =
                egui::vec2(th.spacing_lg.value(), th.spacing_lg.value());
            add(ui);
        },
    );
}
