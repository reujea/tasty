//! SearchableSelect 예제. 시안 `components-searchselect.jsx` `#searchable-select` 의 세 무대
//! (같은 높이 트리거 짝 · 트리거 상태 · 열린 목록)를 Mocha·Latte 상자로 나란히 그린다.
//! 상태 칸은 공용 위젯의 `show_specimen` 으로, live 칸은 공용 위젯 그대로 그린다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    MultiSelectLabels, SearchableSelect, SearchableSelectLabels, SearchableSelectOption,
    SearchableSelectSpecimen, multi_select, select,
};

use crate::catalog::spec::{self, StageVariant, TokenChip, wrap_item};

// 시안 무대·그리드의 전시 치수. 대응 토큰이 없다.
/// 무대 안쪽 여백 — `padding: 20`.
const STAGE_PAD: LogicalPx = LogicalPx(20.0);
/// 테마 상자 사이 — 무대 `gap: 16`.
const THEME_GAP: LogicalPx = LogicalPx(16.0);
/// 짝 비교 줄의 이름 칸 — `gridTemplateColumns: "136px auto"`.
const PAIR_LABEL_W: LogicalPx = LogicalPx(136.0);
/// 상태 그리드 한 칸 폭 — `repeat(3, 200px)`.
const CELL_W: LogicalPx = LogicalPx(200.0);
/// 열린 목록 칸 최소 높이 — `h={340}` · `h={200}` · `h={180}`.
const CELL_H_TALL: LogicalPx = LogicalPx(340.0);
const CELL_H_MID: LogicalPx = LogicalPx(200.0);
const CELL_H_SHORT: LogicalPx = LogicalPx(180.0);

/// 시안 `FONTS`.
const FONTS: [SearchableSelectOption<'static>; 12] = [
    SearchableSelectOption::new("JetBrains Mono"),
    SearchableSelectOption::new("Fira Code"),
    SearchableSelectOption::new("Cascadia Code"),
    SearchableSelectOption::new("D2Coding"),
    SearchableSelectOption::new("Iosevka Term"),
    SearchableSelectOption::new("Source Code Pro"),
    SearchableSelectOption::new("SF Mono"),
    SearchableSelectOption::new("Menlo"),
    SearchableSelectOption::new("Hack"),
    SearchableSelectOption::new("IBM Plex Mono"),
    SearchableSelectOption::new("Noto Sans Mono CJK KR"),
    SearchableSelectOption::new("Ubuntu Mono"),
];
/// 시안 `LONG` 의 마지막 항목. 트리거 끝 말줄임 칸에 쓴다.
const LONG_NAME: &str = "Sarasa Term K Nerd Font Mono Semibold Extended";
const LONG: [SearchableSelectOption<'static>; 1] = [SearchableSelectOption::new(LONG_NAME)];
/// 시안 `PIN`.
const PIN: [SearchableSelectOption<'static>; 1] = [SearchableSelectOption::with_value(
    "",
    "Default (monospace)",
)];
/// 시안 i18n 칸의 독일어 고정 행.
const PIN_DE: [SearchableSelectOption<'static>; 1] = [SearchableSelectOption::with_value(
    "",
    "Standard (Festbreitenschrift)",
)];

/// 시안 기본 문구 + 글꼴 문구. 위젯은 문구를 소유하지 않는다.
/// 검색 안내 · 결과 없음 · 로딩은 본체 글꼴 행과 같은 번역 키(갤러리 손 사본을 두지 않는다).
fn font_labels() -> SearchableSelectLabels<'static> {
    SearchableSelectLabels {
        placeholder: "Choose a font",
        search_placeholder: crate::i18n::t("settings.appearance.font_search_placeholder"),
        empty: crate::i18n::t("settings.appearance.font_no_match"),
        loading: crate::i18n::t("settings.appearance.font_loading"),
    }
}
fn generic_labels() -> SearchableSelectLabels<'static> {
    SearchableSelectLabels {
        search_placeholder: "Search",
        ..font_labels()
    }
}
const DE_LABELS: SearchableSelectLabels<'static> = SearchableSelectLabels {
    placeholder: "Schriftart wählen",
    search_placeholder: "Schriftarten durchsuchen",
    empty: "Keine passenden Schriftarten",
    loading: "Schriftarten werden geladen…",
};
const MS_LABELS: MultiSelectLabels<'static> = MultiSelectLabels {
    none: "No status",
    some: "{} selected",
    some_one: "{} selected",
    all: "All",
};

/// 테마 상자별 live 칸 상태. 0 = Mocha, 1 = Latte.
struct LiveState {
    pair_select: [usize; 2],
    pair_none: [Option<String>; 2],
    pair_ms: [[bool; 3]; 2],
    pair_ss: [Option<String>; 2],
    live: [Option<String>; 2],
}

thread_local! {
    static STATE: RefCell<LiveState> = RefCell::new(LiveState {
        pair_select: [0, 0],
        pair_none: [None, None],
        pair_ms: [[true, false, false]; 2],
        pair_ss: [Some("JetBrains Mono".into()), Some("JetBrains Mono".into())],
        live: [Some("Fira Code".into()), Some("Fira Code".into())],
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        for (stage_i, title) in [
            "same trigger, side by side (200 wide)",
            "trigger states",
            "open list",
        ]
        .iter()
        .enumerate()
        {
            // 열린 목록 무대는 줄바꿈 칸에 넣지 않는다. 줄바꿈 칸은 직전 프레임 크기로 자리를 잡아
            // 목록의 스크롤 영역이 그 높이에 묶여 자라지 못한다. 두 상자는 어차피 세로로 쌓인다.
            let wrap = stage_i < 2;
            app_stage(ui, theme, wrap, |ui| {
                for (ti, (label, th)) in themes.iter().enumerate() {
                    let mut boxed = |ui: &mut egui::Ui| {
                        ui.push_id((stage_i, ti), |ui| {
                            themed(ui, th, &format!("{label} · {title}"), |ui| match stage_i {
                                0 => pairs(ui, th, ti, &mut st),
                                1 => trigger_states(ui, th, ti, &mut st),
                                _ => open_lists(ui, th),
                            });
                        });
                    };
                    if wrap {
                        wrap_item(ui, boxed);
                    } else {
                        boxed(ui);
                    }
                }
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "trigger",
                "= Select · select-height 28 · pad 12 left / 28 chevron room · 1px select-border · chevron flips when open",
            ),
            (
                "trigger states",
                "rest select-border · hover border-control-raised · open/focus select-border-focus + 1px ring · disabled state-disabled-* · placeholder text-placeholder · long value end-ellipsis",
            ),
            (
                "popup",
                "4 below trigger · min-width = trigger · max-width 320 · max-height 300 (whole popup) · menu-bg · menu-border · menu-radius · padding 4",
            ),
            (
                "search field",
                "popup − 2 × 4 wide · 28 tall · pad-x 8 · search icon 14 + gap 8 · focus = border-focus edge, no ring",
            ),
            (
                "separator",
                "1px menu-separator (border-strong) · 4 above · 4 below · a second one under pinned rows",
            ),
            (
                "option row",
                "MenuItem · 28 · selected = menu-item-selected-fg + check (no fill) · keyboard-active surface-active",
            ),
            (
                "status rows",
                "no match / loading · 28 · pad-x 12 · text-secondary-raised · not clickable · loading = Spinner 14 + gap 8",
            ),
            (
                "scroll",
                "only the option list scrolls; field, separators, pinned rows stay",
            ),
            (
                "keys",
                "open focuses search · ↑↓ visible enabled rows (stop at ends) · Enter commit+close · Esc close → trigger · Tab close",
            ),
        ],
        &[
            TokenChip::without_color("select-search-list-max-height", "300 whole popup"),
            TokenChip::without_color("select-search-inset", "4 popup padding"),
            TokenChip::without_color("select-search-menu-gap", "4 trigger → popup"),
            TokenChip::without_color("select-search-menu-max-width", "320"),
            TokenChip::new(
                "select-search-field-bg",
                "search field fill",
                theme.select_search_field_bg().to_egui(),
            ),
            TokenChip::new(
                "select-search-field-border",
                "search field edge",
                theme.select_search_field_border().to_egui(),
            ),
            TokenChip::new(
                "select-search-field-border-focus",
                "focused field edge",
                theme.select_search_field_border_focus().to_egui(),
            ),
            TokenChip::new(
                "select-search-icon-fg",
                "search icon",
                theme.select_search_icon_fg().to_egui(),
            ),
            TokenChip::new(
                "select-search-status-fg",
                "no match / loading",
                theme.select_search_status_fg().to_egui(),
            ),
            TokenChip::new(
                "select-search-trigger-border-hover",
                "trigger hover",
                theme.select_search_trigger_border_hover().to_egui(),
            ),
            TokenChip::new(
                "select-search-value-placeholder",
                "no value",
                theme.select_search_value_placeholder().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Decisions: opening moves focus into the search field (border-focus edge only — the \
         trigger carries the ring). ↑↓ walk visible enabled rows, pinned first; Enter commits \
         and closes, Esc closes and returns focus to the trigger, Tab closes. On open the \
         active row is the current value; each keystroke moves it to the first match. The \
         query is cleared on every close. Search stays inside the list so the trigger keeps \
         the Select shape and a filtered-out or unknown value keeps its label. Pinned rows \
         ignore the query and sit above their own separator. The widget owns no strings.",
    );
    spec::note(
        ui,
        theme,
        "A family whose file can't be loaded keeps this trigger and gets a line under it. See \
         Overlays › Windows, under Settings.",
    );
}

/// 시안 무대: bg-app 위에 테마 상자를 놓는다. `wrap` 이면 줄바꿈하며 옆으로, 아니면 세로로 쌓는다.
fn app_stage(ui: &mut egui::Ui, theme: &Theme, wrap: bool, add: impl FnOnce(&mut egui::Ui)) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                let gap = egui::vec2(THEME_GAP.value(), THEME_GAP.value());
                if wrap {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = gap;
                        add(ui);
                    });
                } else {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = gap;
                        add(ui);
                    });
                }
            });
    });
}

/// 시안 `Themed`: bg-panel 상자 · padding lg · 줄 간격 md · 캡션.
fn themed(ui: &mut egui::Ui, th: &Theme, caption: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(th.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.border_default().to_egui(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::same(th.spacing_lg.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                caption_label(ui, th, caption);
                add(ui);
            });
        });
}

fn caption_label(ui: &mut egui::Ui, th: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(th.font_size_caption.value())
            .color(th.text_muted().to_egui()),
    );
}

/// 시안 짝 비교: Select · 값 없는 SearchableSelect · MultiSelect · SearchableSelect 가 같은 높이다.
fn pairs(ui: &mut egui::Ui, th: &Theme, ti: usize, st: &mut LiveState) {
    let w = th.field_width_lg;
    ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
    let row = |ui: &mut egui::Ui, name: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_md.value();
            ui.allocate_ui_with_layout(
                egui::vec2(PAIR_LABEL_W.value(), th.select_height().value()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(PAIR_LABEL_W.value());
                    ui.label(
                        egui::RichText::new(name)
                            .monospace()
                            .size(th.font_size_micro.value())
                            .color(th.text_muted().to_egui()),
                    );
                },
            );
            add(ui);
        });
    };
    row(ui, "Select", &mut |ui| {
        select(
            ui,
            th,
            &format!("g_ss_pair_select_{ti}"),
            &mut st.pair_select[ti],
            &["Catppuccin Mocha", "Catppuccin Latte"],
            w.value(),
            true,
        );
    });
    row(ui, "Searchable · no value", &mut |ui| {
        SearchableSelect::new(&format!("g_ss_pair_none_{ti}"), &FONTS, font_labels())
            .width(th.field_width_lg)
            .show(ui, th, &mut st.pair_none[ti]);
    });
    row(ui, "MultiSelect", &mut |ui| {
        multi_select(
            ui,
            th,
            &format!("g_ss_pair_ms_{ti}"),
            &mut st.pair_ms[ti],
            &["Waiting", "Ready", "Running"],
            None,
            &MS_LABELS,
            None,
            w.value(),
            true,
        );
    });
    row(ui, "SearchableSelect", &mut |ui| {
        SearchableSelect::new(&format!("g_ss_pair_ss_{ti}"), &FONTS, font_labels())
            .pinned(&PIN)
            .width(th.field_width_lg)
            .show(ui, th, &mut st.pair_ss[ti]);
    });
}

/// 칸 하나: 캡션 + 내용, 고정 폭과 최소 높이. 열린 목록이 최소 높이를 넘으면 칸이 늘어난다
/// (시안은 팝업을 칸 밖으로 띄우지만 정적 견본은 자리를 차지한다).
fn cell(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    min_h: LogicalPx,
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.push_id(label, |ui| {
        ui.vertical(|ui| {
            ui.set_width(CELL_W.value());
            ui.set_min_height(min_h.value());
            ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
            caption_label(ui, th, label);
            add(ui);
        });
    });
}

/// 3열 그리드 한 줄.
fn grid_row(ui: &mut egui::Ui, gap_x: LogicalPx, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap_x.value();
        add(ui);
    });
}

#[allow(clippy::too_many_arguments)] // reason: 시안 칸마다 다른 옵션·문구·상태를 그대로 넘긴다
fn specimen(
    ui: &mut egui::Ui,
    th: &Theme,
    options: &[SearchableSelectOption<'_>],
    pinned: &[SearchableSelectOption<'_>],
    labels: SearchableSelectLabels<'_>,
    value: Option<&str>,
    spec: SearchableSelectSpecimen<'_>,
    enabled: bool,
    loading: bool,
) {
    SearchableSelect::new("g_ss_specimen", options, labels)
        .pinned(pinned)
        .enabled(enabled)
        .loading(loading)
        .width(CELL_W)
        .show_specimen(ui, th, value, &spec);
}

/// 시안 트리거 상태 6칸.
fn trigger_states(ui: &mut egui::Ui, th: &Theme, ti: usize, st: &mut LiveState) {
    let closed = SearchableSelectSpecimen::default();
    let gap_x = th.spacing_lg;
    ui.spacing_mut().item_spacing.y = th.spacing_md.value();
    let h = th.select_height();
    grid_row(ui, gap_x, |ui| {
        cell(ui, th, "closed", h, |ui| {
            specimen(
                ui,
                th,
                &FONTS,
                &[],
                font_labels(),
                Some("JetBrains Mono"),
                closed,
                true,
                false,
            );
        });
        cell(ui, th, "hover — border-control-raised", h, |ui| {
            let hover = SearchableSelectSpecimen {
                hover_trigger: true,
                ..closed
            };
            specimen(
                ui,
                th,
                &FONTS,
                &[],
                font_labels(),
                Some("JetBrains Mono"),
                hover,
                true,
                false,
            );
        });
        cell(ui, th, "disabled — neutral box", h, |ui| {
            specimen(
                ui,
                th,
                &FONTS,
                &[],
                font_labels(),
                Some("JetBrains Mono"),
                closed,
                false,
                false,
            );
        });
    });
    grid_row(ui, gap_x, |ui| {
        cell(ui, th, "placeholder — no value", h, |ui| {
            specimen(
                ui,
                th,
                &FONTS,
                &[],
                font_labels(),
                None,
                closed,
                true,
                false,
            );
        });
        cell(ui, th, "long value — end ellipsis", h, |ui| {
            specimen(
                ui,
                th,
                &LONG,
                &[],
                font_labels(),
                Some(LONG_NAME),
                closed,
                true,
                false,
            );
        });
        cell(ui, th, "live — click, type, ↑↓ Enter", h, |ui| {
            SearchableSelect::new(&format!("g_ss_live_{ti}"), &FONTS, font_labels())
                .pinned(&PIN)
                .width(CELL_W)
                .show(ui, th, &mut st.live[ti]);
        });
    });
}

/// 시안 열린 목록 9칸.
fn open_lists(ui: &mut egui::Ui, th: &Theme) {
    let open = |query, active| SearchableSelectSpecimen {
        open: true,
        focus_search: true,
        query,
        active,
        ..Default::default()
    };
    let gap = th.spacing_lg;
    ui.spacing_mut().item_spacing.y = gap.value();
    let tall = CELL_H_TALL;
    grid_row(ui, gap, |ui| {
        cell(
            ui,
            th,
            "open — focus in search, selected = check",
            tall,
            |ui| {
                specimen(
                    ui,
                    th,
                    &FONTS,
                    &PIN,
                    font_labels(),
                    Some("JetBrains Mono"),
                    open("", Some(1)),
                    true,
                    false,
                );
            },
        );
        cell(
            ui,
            th,
            "typed “co” → filtered, active = 1st match",
            tall,
            |ui| {
                specimen(
                    ui,
                    th,
                    &FONTS,
                    &PIN,
                    font_labels(),
                    Some("JetBrains Mono"),
                    open("co", Some(1)),
                    true,
                    false,
                );
            },
        );
        cell(ui, th, "no match — pinned still shown", tall, |ui| {
            specimen(
                ui,
                th,
                &FONTS,
                &PIN,
                font_labels(),
                Some("JetBrains Mono"),
                open("zzz", Some(0)),
                true,
                false,
            );
        });
    });
    let short = CELL_H_SHORT;
    grid_row(ui, gap, |ui| {
        cell(ui, th, "loading — list still empty", short, |ui| {
            specimen(
                ui,
                th,
                &[],
                &PIN,
                font_labels(),
                Some(""),
                open("", Some(0)),
                true,
                true,
            );
        });
        cell(ui, th, "loading — partial list, row at end", short, |ui| {
            specimen(
                ui,
                th,
                &FONTS[..3],
                &[],
                font_labels(),
                Some("Fira Code"),
                open("", None),
                true,
                true,
            );
        });
        cell(
            ui,
            th,
            "selected row filtered out — trigger keeps it",
            short,
            |ui| {
                specimen(
                    ui,
                    th,
                    &FONTS,
                    &[],
                    font_labels(),
                    Some("Fira Code"),
                    open("mono", Some(0)),
                    true,
                    false,
                );
            },
        );
    });
    let mid = CELL_H_MID;
    grid_row(ui, gap, |ui| {
        cell(
            ui,
            th,
            "hover (row 4) vs keyboard-active (row 2)",
            mid,
            |ui| {
                let spec = SearchableSelectSpecimen {
                    open: true,
                    active: Some(1),
                    hover: Some(3),
                    ..Default::default()
                };
                specimen(
                    ui,
                    th,
                    &FONTS[..5],
                    &[],
                    font_labels(),
                    Some("JetBrains Mono"),
                    spec,
                    true,
                    false,
                );
            },
        );
        cell(ui, th, "no pinned option (generic use)", mid, |ui| {
            let spec = SearchableSelectSpecimen {
                open: true,
                active: Some(0),
                ..Default::default()
            };
            specimen(
                ui,
                th,
                &FONTS[..5],
                &[],
                generic_labels(),
                Some("JetBrains Mono"),
                spec,
                true,
                false,
            );
        });
        cell(ui, th, "i18n — de, long strings", mid, |ui| {
            specimen(
                ui,
                th,
                &FONTS,
                &PIN_DE,
                DE_LABELS,
                Some(""),
                open("qq", Some(0)),
                true,
                false,
            );
        });
    });
}
