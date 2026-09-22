//! Settings window — 디자인(4) Overlays `settings` Spec.
//!
//! 권위 원본: `ui_kits/terminal/overlays/settings_window.jsx` (settings-ia-restructure,
//! 2026-06-17 · 크기확대 2026-06-26). 1100×700 카드, **7탭 L1 IA**
//! (General / Terminal / Appearance / Keybindings / File Handler / Misc / Plugins),
//! 좌측 "Settings" 타이틀 + 세로 구분선을 가진 상단 밴드(높이 44 — close ✕ 없음,
//! 닫기는 footer Cancel + OS 타이틀바),
//! **L2 200px** 섹션 사이드바(검색 아이콘 필터 + 섹션 리스트, plugin 섹션은
//! accent-agent dot), content, footer(Cancel/Save).
//!
//! **boolean = `switch()`** (디자인 규약: 모든 boolean 설정행은 `<Switch>`).
//! 유일한 예외는 Appearance › Colors override 행의 "Default" 토글로, 거기서만
//! `checkbox()` 를 쓴다 (jsx `ColorOverridePicker`).
//!
//! 정적 specimen 은 L2 를 탐색할 수 없으므로, content 영역은 선택된 Theme 섹션
//! (스와치 그리드)을 1차로 보이고, 그 아래 구분선 뒤에 Appearance 의 나머지
//! 컨트롤 어휘(General 의 switch 행 + Colors 의 checkbox 행)를 **카탈로그**로 함께
//! 노출한다 — 본체 설정창이 쓰는 컨트롤을 cut 하지 않는다(ADR-0510 갤러리 완전성).
//! General(L1) › General 의 **언어 콤보**(`language_select`)도 같은 카탈로그에 있다 —
//! 내장 3 + 사용자 언어팩 N, `[meta] name` 이 없는 팩의 코드 폴백, 설정값이 목록에
//! 없을 때의 `<code> (not found)` 행(값을 덮어쓰지 않는다)까지 세 케이스를 한 번에 보인다.
//!
//! 본체 트랙과 **같은 위젯·같은 토큰**: `tasty_ui_widgets::{switch,checkbox,select,Input}`
//! (셸/밴드/사이드바/푸터 토큰은 `widgets::dialog` 키트 공유). 스와치 strip 색은
//! 갤러리 토큰 규율상 literal preset hex 대신 **theme 팔레트 토큰**으로 구조만 전사한다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, Input, LanguageOption, LanguageSelectLabels, checkbox, language_select,
    select, switch,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const WIDTH: LogicalPx = LogicalPx(1100.0);
const HEIGHT: LogicalPx = LogicalPx(700.0);
const L2_WIDTH: LogicalPx = LogicalPx(200.0);
/// 콘텐츠 컬럼 상한 — 디자인 `--tasty-settings-content-max-width`. 창이 아무리 넓어도
/// 한 블록이 이보다 넓어지지 않는다. full-bleed 서브탭(자기 레이아웃을 들고 컬럼을
/// 대체하는 것)만 예외이고, 본문 산문은 이것과 **다른 축**인 `measure-md` 를 그대로 쓴다.
const CONTENT_MAX_W: LogicalPx = LogicalPx(620.0);
/// jsx `Row` 라벨 폭 (width 150, flex none) — 디자인 고정 치수.
const ROW_LABEL_W: LogicalPx = LogicalPx(150.0);

/// L1 상단 탭 (jsx `L1_LABEL`: FileHandler → "Handler" — S13 일반화, 내부 키는
/// FileHandler 유지). 활성 = Appearance.
const L1_TABS: &[&str] = &[
    "General",
    "Terminal",
    "Appearance",
    "Keybindings",
    "Handler",
    "Misc",
    "Plugins",
];
const L1_ACTIVE: usize = 2;

/// Appearance 섹션 L2 (jsx `L2.Appearance`). plugin-기여 "Diff colors" 는 dot 표시.
/// `(label, plugin_dot)`. 선택 = "Theme".
const L2_SECTIONS: &[(&str, bool)] = &[
    ("Theme", false),
    ("Colors", false),
    ("General", false),
    ("Display", false),
    ("Tasty", false),
    ("Terminal", false),
    ("Diff colors", true),
    ("HTML", false),
];
const L2_SELECTED: usize = 0;

const FONT_FAMILIES: &[&str] = &["D2Coding", "JetBrains Mono", "Cascadia Code"];

/// 언어 콤보 행 — 내장 3 + 사용자 언어팩 N. `fr` 은 `[meta] name` 을 가진 팩, `xx` 는
/// `[meta] name` 이 없어 코드가 그대로 라벨이 되는 폴백 케이스.
const LANGUAGES: &[LanguageOption<'static>] = &[
    LanguageOption {
        code: "en",
        label: "English",
    },
    LanguageOption {
        code: "ko",
        label: "한국어",
    },
    LanguageOption {
        code: "ja",
        label: "日本語",
    },
    LanguageOption {
        code: "fr",
        label: "Français",
    },
    LanguageOption {
        code: "xx",
        label: "xx",
    },
];
const LANGUAGE_LABELS: LanguageSelectLabels<'static> = LanguageSelectLabels {
    missing_suffix: "(not found)",
};

struct State {
    filter: String,
    font_family: usize,
    font_size: String,
    ligatures: bool,
    opacity: f32,
    /// Colors override 행: checked = "Default"(프리셋 추종), unchecked = 개별 override.
    color_default: bool,
    /// 언어 콤보 — 목록에 있는 코드(팩 `fr`).
    language: String,
    /// 언어 콤보 — 목록에 **없는** 코드(팩이 지워진 `zz`): `zz (not found)` 행으로 유지.
    language_missing: String,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        filter: String::new(),
        font_family: 0,
        font_size: String::from("14"),
        ligatures: true,
        opacity: 1.0,
        color_default: false,
        language: String::from("fr"),
        language_missing: String::from("zz"),
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let band_h = theme.titlebar_height + theme.spacing_sm; // 44
    let footer_h = theme.item_height_interactive + theme.spacing_sm.scaled(2.0); // 44
    let mid_h = (HEIGHT - band_h - footer_h - theme.border_width.scaled(2.0)).max(theme.measure_sm);
    // content 폭은 명시 계산(측정 패스에서 available_width 0 → 음수 폭 패닉 회피).
    let content_w = (WIDTH - L2_WIDTH - theme.border_width - theme.spacing_lg.scaled(2.0))
        .max(theme.measure_sm);

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // Settings 는 별도 창(`SettingsView`)이라 살아 있는 콘텐츠 위에 뜨지도,
        // 터미널 뷰포트를 점유하지도 않는다 — 본체 `src/view/settings/` 도 그림자를
        // 그리지 않는다. SCOPE RULE(ADR-0254)의 세 번째 갈래.
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            l1_band(ui, theme, band_h);
            kit::hsep(ui, theme);
            ui.horizontal_top(|ui| {
                l2_sidebar(ui, theme, mid_h);
                vsep(ui, theme, mid_h);
                content(ui, theme, content_w, mid_h);
            });
            kit::hsep(ui, theme);
            footer(ui, theme);
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "1100×700 · bg-panel · border-strong"),
            ("L1 band", "h44 · Settings title · 7 tabs"),
            ("active tab", "2px accent underline"),
            ("L2", "sidebar 200 · search filter · plugin dot"),
            ("content", "padding 16 · row label 150 · gap 16"),
            (
                "content column",
                "capped at 620 — every non-full-bleed subtab inherits",
            ),
            ("boolean", "switch() — Colors Default = checkbox()"),
            (
                "language",
                "language_select() — built-in 3 + packs · code fallback · missing row",
            ),
            ("footer", "Cancel (ghost) · Save (primary)"),
        ],
        &[
            TokenChip::new("bg-sidebar", "band + L2", theme.bg_sidebar().to_egui()),
            TokenChip::new("bg-panel", "content", theme.bg_panel().to_egui()),
            TokenChip::new(
                "accent-primary",
                "active tab · switch on",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "surface-active",
                "selected section",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-agent",
                "plugin section dot",
                theme.accent_agent().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Three tiers: the L1 band picks a domain, the L2 sidebar the section within it, \
         and content shows the controls. Every boolean is a Switch; only the Colors \
         override 'Default' row is a Checkbox.",
    );
}

// ── L1 상단 밴드 ───────────────────────────────────────────────────────────

fn l1_band(ui: &mut egui::Ui, theme: &Theme, band_h: LogicalPx) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .inner_margin(egui::Margin::symmetric(theme.spacing_md.value() as i8, 0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(band_h.value());
                ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
                // 좌측 "Settings" 타이틀 (bold 14px) + 세로 구분선.
                ui.label(
                    egui::RichText::new("Settings")
                        .size(theme.font_size_max.value())
                        .strong()
                        .color(theme.text_primary().to_egui()),
                );
                ui.add_space(theme.spacing_sm.value());
                let (vr, _) = ui.allocate_exact_size(
                    egui::vec2(theme.border_width.value(), theme.spacing_xl.value()),
                    egui::Sense::hover(),
                );
                ui.painter().vline(
                    vr.center().x,
                    vr.y_range(),
                    egui::Stroke::new(theme.border_width.value(), theme.separator.to_egui()),
                );
                ui.add_space(theme.spacing_sm.value());
                // 탭들. 우측 close ✕ 는 없다 — 닫기는 footer Cancel + OS 타이틀바
                // 로 일원화(중복 닫기 동작 방지).
                for (i, t) in L1_TABS.iter().enumerate() {
                    l1_tab(ui, theme, t, band_h, i == L1_ACTIVE);
                }
            });
        });
}

fn l1_tab(ui: &mut egui::Ui, theme: &Theme, label: &str, band_h: LogicalPx, active: bool) {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        egui::Color32::PLACEHOLDER,
    );
    let pad = theme.spacing_md.value();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.rect.width() + pad * 2.0, band_h.value()),
        egui::Sense::hover(),
    );
    let fg = if active {
        theme.text_primary()
    } else {
        theme.text_muted()
    };
    ui.painter().galley(
        rect.center() - galley.rect.size() * 0.5,
        galley,
        fg.to_egui(),
    );
    if active {
        let bar = egui::Rect::from_min_size(
            egui::pos2(
                rect.left(),
                rect.bottom() - theme.tab_indicator_width.value(),
            ),
            egui::vec2(rect.width(), theme.tab_indicator_width.value()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
    }
}

// ── L2 섹션 사이드바 ───────────────────────────────────────────────────────

fn l2_sidebar(ui: &mut egui::Ui, theme: &Theme, mid_h: LogicalPx) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .show(ui, |ui| {
            ui.set_width(L2_WIDTH.value());
            ui.set_min_height(mid_h.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            // 부모(`draw` 의 `horizontal_top`)가 가로 레이아웃이라 Frame 의 child Ui 도 가로로
            // 흐른다 — 사이드바 내부(필터 → 구분선 → 섹션 리스트)는 세로 적층으로 명시한다.
            ui.vertical(|ui| {
                // 검색 필터 (search 아이콘 + placeholder) — 자체 padding + border-bottom.
                kit::region_sym(ui, theme.spacing_sm, theme.spacing_sm, |ui| {
                    STATE.with(|s| {
                        let st = &mut *s.borrow_mut();
                        Input::new()
                            .placeholder("Filter sections…")
                            .icon(&|ui, rect, c| {
                                icons::SEARCH.image(rect.height(), c).paint_at(ui, rect)
                            })
                            .show(ui, theme, &mut st.filter);
                    });
                });
                kit::hsep(ui, theme);
                // 섹션 리스트.
                kit::region_sym(ui, theme.spacing_sm, theme.spacing_sm, |ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                    for (i, (label, plugin)) in L2_SECTIONS.iter().enumerate() {
                        l2_item(ui, theme, label, *plugin, i == L2_SELECTED);
                    }
                });
            });
        });
}

fn l2_item(ui: &mut egui::Ui, theme: &Theme, label: &str, plugin: bool, active: bool) {
    let h = theme.item_height_interactive.value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if active {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.surface_active().to_egui(),
        );
    }
    let mut x = rect.left() + theme.spacing_sm.value();
    if plugin {
        let d = theme.status_dot_size.value();
        ui.painter().circle_filled(
            egui::pos2(x + d * 0.5, rect.center().y),
            d * 0.5,
            theme.accent_agent().to_egui(),
        );
        x += d + theme.spacing_sm.value();
    }
    let fg = if active {
        theme.text_primary()
    } else {
        theme.text_muted()
    };
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme.font_size_body.value()),
        fg.to_egui(),
    );
}

fn vsep(ui: &mut egui::Ui, theme: &Theme, mid_h: LogicalPx) {
    let (r, _) = ui.allocate_exact_size(
        egui::vec2(theme.border_width.value(), mid_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        r.center().x,
        r.y_range(),
        egui::Stroke::new(theme.border_width.value(), theme.separator.to_egui()),
    );
}

// ── content ────────────────────────────────────────────────────────────────

fn content(ui: &mut egui::Ui, theme: &Theme, content_w: LogicalPx, mid_h: LogicalPx) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
        .show(ui, |ui| {
            ui.set_min_width(content_w.value());
            ui.set_min_height(mid_h.value());
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            // 아래 `theme_swatch` 가 f32 폭을 받는다 — 그 관문까지가 이번 회차 밖이라
            // 여기서 한 번 벗긴다. 상한은 컬럼에 **한 번** 건다 — 블록마다 걸면 블록끼리
            // 값이 갈린다.
            let inner = (content_w - theme.spacing_lg.scaled(2.0))
                .min(CONTENT_MAX_W)
                .value();
            // 위 사이드바와 같은 이유 — content 의 섹션/행은 세로 적층.
            ui.vertical(|ui| {
                // ── Theme preset (선택된 L2 = Theme) ──
                mono(ui, theme, "Theme preset");
                let cw =
                    ((inner - theme.spacing_sm.value()) * 0.5).max(theme.field_width_md.value());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    // strip 색은 theme 팔레트 토큰 (literal preset hex 대신 구조 전사).
                    theme_swatch(
                        ui,
                        theme,
                        cw,
                        "Catppuccin Mocha",
                        true,
                        &[
                            theme.crust,
                            theme.base,
                            theme.blue,
                            theme.mauve,
                            theme.green,
                        ],
                    );
                    theme_swatch(
                        ui,
                        theme,
                        cw,
                        "Catppuccin Latte",
                        false,
                        &[
                            theme.surface2,
                            theme.subtext0,
                            theme.sky,
                            theme.lavender,
                            theme.teal,
                        ],
                    );
                });
                note(
                    ui,
                    theme,
                    "Selecting a preset resets all custom colors. Fine-tune individual colors \
                 in the Colors section — switching presets clears those overrides.",
                );

                // ── 컨트롤 어휘 카탈로그 (정적 specimen 은 L2 탐색 불가 → 나머지 섹션 동봉) ──
                ui.add_space(theme.spacing_sm.value());
                kit::hsep(ui, theme);
                ui.add_space(theme.spacing_sm.value());
                note(ui, theme, "Control vocabulary — other Appearance sections");

                STATE.with(|s| {
                    let st = &mut *s.borrow_mut();

                    // General — boolean = Switch.
                    mono(ui, theme, "General");
                    row(ui, theme, "Font family:", |ui| {
                        select(
                            ui,
                            theme,
                            "settings_font_family",
                            &mut st.font_family,
                            FONT_FAMILIES,
                            theme.field_width_lg.value(),
                            true,
                        );
                    });
                    row(ui, theme, "Font size:", |ui| {
                        Input::new()
                            .mono(true)
                            .addon("px")
                            .width(theme.field_width_xs.value())
                            .show(ui, theme, &mut st.font_size);
                    });
                    row(ui, theme, "Ligatures:", |ui| {
                        switch(ui, theme, &mut st.ligatures, None, true);
                    });
                    row(ui, theme, "Background opacity:", |ui| {
                        range_track(ui, theme, theme.field_width_lg.value(), st.opacity);
                    });

                    // Colors — 유일한 checkbox 예외 ("Default" override 행).
                    mono(ui, theme, "Colors");
                    override_row(ui, theme, "blue", "#74c7ec", &mut st.color_default);

                    // General(L1) › General — 언어 콤보. 내장 3 + 언어팩 N(`fr` 표시 이름 · `xx` 코드
                    // 폴백) + 설정값이 목록에 없을 때의 `zz (not found)` 행(값 보존).
                    mono(ui, theme, "General › Language");
                    row(ui, theme, "Language:", |ui| {
                        language_select(
                            ui,
                            theme,
                            "settings_language",
                            &mut st.language,
                            LANGUAGES,
                            &LANGUAGE_LABELS,
                            theme.field_width_lg.value(),
                            true,
                        );
                    });
                    row(ui, theme, "Language (pack removed):", |ui| {
                        language_select(
                            ui,
                            theme,
                            "settings_language_missing",
                            &mut st.language_missing,
                            LANGUAGES,
                            &LANGUAGE_LABELS,
                            theme.field_width_lg.value(),
                            true,
                        );
                    });
                    note(
                        ui,
                        theme,
                        "Built-in en/ko/ja plus every ~/.tasty/lang/<code>/pack.toml. A pack without a \
                         [meta] name shows its code (xx); a configured code with no pack stays \
                         selected as 'zz (not found)' instead of being overwritten.",
                    );
                });
            });
        });
}

/// jsx `Row` — 라벨(width 150, text-secondary) 좌 / 컨트롤 우, gap 16, min-height row.
fn row(ui: &mut egui::Ui, theme: &Theme, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        let (lr, _) = ui.allocate_exact_size(
            egui::vec2(ROW_LABEL_W.value(), theme.item_height_interactive.value()),
            egui::Sense::hover(),
        );
        ui.painter().text(
            egui::pos2(lr.left(), lr.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(theme.font_size_body.value()),
            theme.text_secondary().to_egui(),
        );
        control(ui);
    });
}

/// jsx `ColorOverridePicker` 행 — dot + mono 필드명 + mono 값 + 스와치 + "Default" checkbox.
fn override_row(ui: &mut egui::Ui, theme: &Theme, field: &str, value: &str, default: &mut bool) {
    let overridden = !*default;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        // override 표식 dot.
        let d = theme.status_dot_size.value();
        let (dr, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
        if overridden {
            ui.painter()
                .circle_filled(dr.center(), d * 0.5, theme.accent_primary().to_egui());
        }
        // 필드명 (mono).
        let fg = if overridden {
            theme.text_primary()
        } else {
            theme.text_muted()
        };
        let (nr, _) = ui.allocate_exact_size(
            egui::vec2(
                theme.field_width_xs.value(),
                theme.item_height_interactive.value(),
            ),
            egui::Sense::hover(),
        );
        ui.painter().text(
            egui::pos2(nr.left(), nr.center().y),
            egui::Align2::LEFT_CENTER,
            field,
            egui::FontId::monospace(theme.font_size_body.value()),
            fg.to_egui(),
        );
        // 값 (mono 정적 필드).
        kit::field(
            ui,
            theme,
            Some(theme.field_width_color),
            value,
            !overridden,
            true,
        );
        // 스와치 (값 색 — theme.blue 토큰으로 근사).
        let s = theme.icon_glyph_size_sm.value();
        let (sr, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
        ui.painter().rect(
            sr,
            theme.corner_radius_sm.value(),
            theme.blue.to_egui(),
            egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
            egui::StrokeKind::Inside,
        );
        // "Default" — 유일한 checkbox.
        checkbox(ui, theme, default, "Default", true);
    });
}

/// jsx `ThemeSwatch` — 상단 색 strip(높이 38) + 하단 라벨 바(bg-panel). active 시 accent border + ring.
fn theme_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    label: &str,
    active: bool,
    strip: &[tasty_type_appearance::color::HexColor],
) {
    let strip_h = theme.titlebar_height.value(); // ≈ 38 (디자인 height 38)
    let label_h = theme.item_height_interactive.value();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, strip_h + label_h), egui::Sense::hover());
    let radius = theme.corner_radius.value();
    // 색 strip (가로 균등 분할).
    let strip_rect = egui::Rect::from_min_size(rect.min, egui::vec2(width, strip_h));
    let n = strip.len().max(1) as f32;
    let seg = width / n;
    for (i, c) in strip.iter().enumerate() {
        let x = strip_rect.left() + seg * i as f32;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, strip_rect.top()), egui::vec2(seg, strip_h)),
            0.0,
            c.to_egui(),
        );
    }
    // 라벨 바 (bg-panel).
    let label_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + strip_h),
        egui::vec2(width, label_h),
    );
    ui.painter()
        .rect_filled(label_rect, 0.0, theme.bg_panel().to_egui());
    ui.painter().text(
        egui::pos2(
            label_rect.left() + theme.spacing_sm.value(),
            label_rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_primary().to_egui(),
    );
    // 외곽 border + active ring.
    let (border, bw) = if active {
        (theme.accent_primary(), theme.focus_ring_width.value())
    } else {
        (theme.border_default(), theme.border_width.value())
    };
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, border.to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 정적 range 트랙 — 둥근 레일 + accent 채움 + thumb (디자인 input[type=range]).
fn range_track(ui: &mut egui::Ui, theme: &Theme, width: f32, frac: f32) {
    let h = theme.item_height_interactive.value();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::hover());
    let rail_h = theme.spacing_xs.value();
    let rail = egui::Rect::from_center_size(rect.center(), egui::vec2(width, rail_h));
    ui.painter()
        .rect_filled(rail, rail_h * 0.5, theme.surface_active().to_egui());
    let filled_w = width * frac.clamp(0.0, 1.0);
    let filled = egui::Rect::from_min_size(rail.min, egui::vec2(filled_w, rail_h));
    ui.painter()
        .rect_filled(filled, rail_h * 0.5, theme.accent_primary().to_egui());
    let thumb_x = rail.left() + filled_w;
    ui.painter().circle_filled(
        egui::pos2(thumb_x, rect.center().y),
        theme.icon_glyph_size_sm.value() * 0.5,
        theme.accent_primary().to_egui(),
    );
}

// ── footer ─────────────────────────────────────────────────────────────────

fn footer(ui: &mut egui::Ui, theme: &Theme) {
    kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Button::new("Save")
                    .variant(ButtonVariant::Primary)
                    .show(ui, theme);
                Button::new("Cancel")
                    .variant(ButtonVariant::Ghost)
                    .show(ui, theme);
            });
        });
    });
}

// ── content 헬퍼 ─────────────────────────────────────────────────────────────

/// Mono 섹션 헤더 — mono 10 uppercase muted (jsx `Mono`).
fn mono(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .monospace()
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// Note 산문 — 12px muted (jsx `Note`).
fn note(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}
