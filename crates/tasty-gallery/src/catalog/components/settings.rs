//! 설정 창의 탭·사이드바·입력 컨트롤 예제.
//! 사이드바 탐색은 구현하지 않아 테마와 일반 컨트롤, 언어 선택을 한 화면에 모았다.
//! 테마 스와치는 실제 프리셋 파일을 읽지 않고 현재 Theme 팔레트로 구성한다.

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

/// 시안 갤러리 `SettingsFrame`의 전시 크기 620×380. 제품 창(settings-window-* 1100×700)을
/// 줄여 보이는 Stage 치수다.
const FRAME_W: LogicalPx = LogicalPx(620.0);
const FRAME_H: LogicalPx = LogicalPx(380.0);
/// 시안 L2 사이드바 폭 168(전시 크기 기준).
const L2_WIDTH: LogicalPx = LogicalPx(168.0);
/// 시안 L1 탭 좌우 여백 13, 제목 뒤 구분선의 왼쪽 6·오른쪽 14 여백과 높이 20.
const L1_TAB_PAD_X: LogicalPx = LogicalPx(13.0);
const L1_SEP_MARGIN_L: LogicalPx = LogicalPx(6.0);
const L1_SEP_MARGIN_R: LogicalPx = LogicalPx(14.0);
const L1_SEP_H: LogicalPx = LogicalPx(20.0);
/// 시안 L2 목록 바깥 여백 6, 항목 세로 여백 5.
const L2_LIST_PAD: LogicalPx = LogicalPx(6.0);
const L2_ITEM_PAD_Y: LogicalPx = LogicalPx(5.0);
/// 시안 콘텐츠 여백 18, 블록 간격 12, 테마 카드 사이 10, 카드 색 띠 34, 카드 라벨 여백 6·9.
const CONTENT_PAD: LogicalPx = LogicalPx(18.0);
const CONTENT_GAP: LogicalPx = LogicalPx(12.0);
const CARD_GAP: LogicalPx = LogicalPx(10.0);
const CARD_STRIP_H: LogicalPx = LogicalPx(34.0);
const CARD_LABEL_PAD: (LogicalPx, LogicalPx) = (LogicalPx(9.0), LogicalPx(6.0));
/// jsx `Row` 라벨 폭 (width 150, flex none) — 디자인 고정 치수.
const ROW_LABEL_W: LogicalPx = LogicalPx(150.0);

/// 시안 L1 탭 네 개. Appearance 를 선택한다.
const L1_TABS: &[&str] = &["General", "Appearance", "Keybindings", "Plugins"];
const L1_ACTIVE: usize = 1;

/// 시안 L2 세 항목. Theme 를 선택한다.
const L2_SECTIONS: &[&str] = &["Theme", "General", "Terminal"];
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

/// Overlays › Settings window — 시안 `SettingsFrame`: L1 탭 줄 아래 L2 사이드바와 콘텐츠, 하단 버튼 줄.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let band_h = theme.titlebar_height + theme.spacing_sm; // 44
    spec::stage(ui, theme, StageVariant::Center, |ui| {
        kit::frame_card(ui, theme, FRAME_W, kit::panel_fill(theme), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            l1_band(ui, theme, band_h);
            kit::hsep(ui, theme);
            // 콘텐츠 높이는 버튼 줄을 뺀 나머지다. 버튼 줄 높이는 직전 프레임에 잰 값을 쓴다.
            let footer_id = ui.id().with("settings_footer_h");
            let footer_h: f32 = ui
                .data(|d| d.get_temp(footer_id))
                .unwrap_or(theme.item_height_interactive.value());
            let bw = theme.border_width.value();
            // 프레임 위아래 테두리와 L1 줄 아래 구분선을 뺀 높이가 사이드바·콘텐츠 열의 높이다.
            let mid_h = (FRAME_H.value() - band_h.value() - bw * 3.0).max(theme.measure_sm.value());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                l2_sidebar(ui, theme, mid_h);
                vsep(ui, theme, mid_h);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    content(ui, theme, mid_h - footer_h - bw);
                    kit::hsep(ui, theme);
                    let r = footer(ui, theme);
                    ui.data_mut(|d| d.insert_temp(footer_id, r.height()));
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "1100 × 700 (canonical)"),
            ("L1 tabs", "44px bar, accent underline"),
            ("L2 sidebar", "200px, filter + list"),
            ("footer", "Cancel / Save, right"),
        ],
        &[
            TokenChip::new("bg-sidebar", "L1 + L2", theme.bg_sidebar().to_egui()),
            TokenChip::new("bg-panel", "content", theme.bg_panel().to_egui()),
            TokenChip::new(
                "accent-primary",
                "active tab",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "surface-active",
                "active section",
                theme.surface_active().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "L1 stays small forever; growth happens only in L2. Plugins are their own L1 tab — never crammed into another group's sidebar.",
    );
}

/// 설정 창 콘텐츠의 컨트롤 어휘. 시안 갤러리에는 없는 갤러리 전용 예제로, 본체의 행·스위치·
/// 색 override·언어 선택을 콘텐츠 열 최대 폭(settings-content-max-width)으로 보인다.
pub fn draw_controls(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        ui.set_max_width(theme.settings_content_max_width().value());
        ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
        controls(ui, theme);
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "content column",
                "capped at settings-content-max-width (620)",
            ),
            ("row", "label 150 · gap 16 · control"),
            (
                "boolean",
                "switch() — Colors Default = checkbox(, tasty_ui_widgets::Ground::Panel)",
            ),
            (
                "language",
                "language_select() — built-in 3 + packs · code fallback · missing row",
            ),
        ],
        &[
            TokenChip::without_color("font-mono", "value text"),
            TokenChip::new(
                "accent-primary",
                "switch on · override dot",
                theme.accent_primary().to_egui(),
            ),
        ],
    );
}

fn l1_band(ui: &mut egui::Ui, theme: &Theme, band_h: LogicalPx) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), band_h.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(theme.spacing_md.value(), 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // 시안 탭 사이 gap 2.
    row.spacing_mut().item_spacing.x = theme.border_width.value() * 2.0;
    row.label(
        egui::RichText::new("Settings")
            .size(theme.font_size_max.value())
            .strong()
            .color(theme.text_primary().to_egui()),
    );
    row.add_space(L1_SEP_MARGIN_L.value());
    let (vr, _) = row.allocate_exact_size(
        egui::vec2(theme.border_width.value(), L1_SEP_H.value()),
        egui::Sense::hover(),
    );
    row.painter().vline(
        vr.center().x,
        vr.y_range(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    row.add_space(L1_SEP_MARGIN_R.value());
    for (i, t) in L1_TABS.iter().enumerate() {
        l1_tab(&mut row, theme, t, band_h, i == L1_ACTIVE);
    }
}

fn l1_tab(ui: &mut egui::Ui, theme: &Theme, label: &str, band_h: LogicalPx, active: bool) {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        egui::Color32::PLACEHOLDER,
    );
    let pad = L1_TAB_PAD_X.value();
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
                rect.bottom() - theme.tab_indicator_width().value(),
            ),
            egui::vec2(rect.width(), theme.tab_indicator_width().value()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
    }
}

fn l2_sidebar(ui: &mut egui::Ui, theme: &Theme, mid_h: f32) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .show(ui, |ui| {
            ui.set_width(L2_WIDTH.value());
            ui.set_min_height(mid_h);
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                kit::region_sym(ui, theme.spacing_sm, theme.spacing_sm, |ui| {
                    STATE.with(|s| {
                        let st = &mut *s.borrow_mut();
                        Input::new()
                            .placeholder("Filter…")
                            .icon(&|ui, rect, c| {
                                icons::SEARCH.image(rect.height(), c).paint_at(ui, rect)
                            })
                            .show(ui, theme, &mut st.filter);
                    });
                });
                kit::hsep(ui, theme);
                kit::region_sym(ui, L2_LIST_PAD, L2_LIST_PAD, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, label) in L2_SECTIONS.iter().enumerate() {
                        l2_item(ui, theme, label, i == L2_SELECTED);
                    }
                });
            });
        });
}

fn l2_item(ui: &mut egui::Ui, theme: &Theme, label: &str, active: bool) {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let row_h = ui.fonts(|f| f.row_height(&font));
    let h = row_h + L2_ITEM_PAD_Y.value() * 2.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if active {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.surface_active().to_egui(),
        );
    }
    let fg = if active {
        theme.text_on_active()
    } else {
        theme.text_muted()
    };
    ui.painter().text(
        egui::pos2(rect.left() + theme.spacing_sm.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        fg.to_egui(),
    );
}

fn vsep(ui: &mut egui::Ui, theme: &Theme, mid_h: f32) {
    let (r, _) = ui.allocate_exact_size(
        egui::vec2(theme.border_width.value(), mid_h),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        r.center().x,
        r.y_range(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// 시안 콘텐츠: Theme preset 캡션, 테마 카드 두 장, 안내 한 줄.
fn content(ui: &mut egui::Ui, theme: &Theme, h: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.max(0.0)), egui::Sense::hover());
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(CONTENT_PAD.value()))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    inner.spacing_mut().item_spacing.y = CONTENT_GAP.value();
    mono(&mut inner, theme, "Theme preset");
    let cw = (inner.available_width() - CARD_GAP.value()) * 0.5;
    inner.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP.value();
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
        &mut inner,
        theme,
        "Selecting a preset resets all surface colors.",
    );
}

/// 갤러리 전용 컨트롤 어휘 — 본체 Appearance·General 의 행 컨트롤.
fn controls(ui: &mut egui::Ui, theme: &Theme) {
    STATE.with(|s| {
        let st = &mut *s.borrow_mut();

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

        mono(ui, theme, "Colors");
        override_row(ui, theme, "blue", "#74c7ec", &mut st.color_default);

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
            "Built-in en/ko/ja plus packs in the Tasty home's lang/<code>/pack.toml. A pack without a \
             [meta] name shows its code (xx); a configured code with no pack stays \
             selected as 'zz (not found)' instead of being overwritten.",
        );
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
        let d = theme.status_dot_size.value();
        let (dr, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
        if overridden {
            ui.painter()
                .circle_filled(dr.center(), d * 0.5, theme.accent_primary().to_egui());
        }
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
        kit::field(
            ui,
            theme,
            Some(theme.field_width_color),
            value,
            !overridden,
            true,
        );
        let s = theme.icon_glyph_size_sm.value();
        let (sr, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
        ui.painter().rect(
            sr,
            theme.corner_radius_sm.value(),
            theme.blue.to_egui(),
            egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
            egui::StrokeKind::Inside,
        );
        checkbox(
            ui,
            theme,
            default,
            "Default",
            true,
            tasty_ui_widgets::Ground::Panel,
        );
    });
}

/// 시안 테마 카드 — 상단 색 띠(높이 34) + 하단 라벨 바(bg-panel). active 시 accent border + ring.
fn theme_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: f32,
    label: &str,
    active: bool,
    strip: &[tasty_type_appearance::color::HexColor],
) {
    let strip_h = CARD_STRIP_H.value();
    let label_font = egui::FontId::proportional(theme.font_size_caption.value());
    let label_h = ui.fonts(|f| f.row_height(&label_font)) + CARD_LABEL_PAD.1.value() * 2.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, strip_h + label_h), egui::Sense::hover());
    let radius = theme.corner_radius.value();
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
    let label_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + strip_h),
        egui::vec2(width, label_h),
    );
    ui.painter()
        .rect_filled(label_rect, 0.0, theme.bg_panel().to_egui());
    ui.painter().text(
        egui::pos2(
            label_rect.left() + CARD_LABEL_PAD.0.value(),
            label_rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        label,
        label_font,
        theme.text_primary().to_egui(),
    );
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
pub(super) fn range_track(ui: &mut egui::Ui, theme: &Theme, width: f32, frac: f32) {
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

/// 시안 버튼 줄 — transfer-footer-pad-y · transfer-pad-x 여백, 오른쪽 정렬 Cancel · Save.
fn footer(ui: &mut egui::Ui, theme: &Theme) -> egui::Rect {
    let margin = egui::Margin::symmetric(
        theme.transfer_pad_x().value() as i8,
        theme.transfer_footer_pad_y().value() as i8,
    );
    egui::Frame::new()
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Save")
                        .variant(ButtonVariant::Primary)
                        .show(ui, theme);
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                });
            });
        })
        .response
        .rect
}

/// 섹션 헤더 — font-label 10 uppercase muted (jsx `Mono`).
fn mono(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
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
