use super::font_rows::{
    PreviewFonts, RowJudge, custom_font_default_line, custom_font_field, family_default_line,
    family_error_line, preview_slot,
};
use crate::i18n::t;
use crate::settings::{
    ActiveTabIndicator, EffectiveFont, FontOverride, FontSettings, HexColor, Settings,
};
use tasty_host_plugin::SettingsPageEntry;

use super::plugin_page::draw_plugin_settings_page;
use tasty_type_appearance::theme::{
    FALLBACK_SURFACE, PartialColors, PartialSurfaceTheme, SurfaceTheme, Theme, ThemeColors,
};
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, Input, OverrideCell, SearchableSelect,
    SearchableSelectLabels, SearchableSelectOption, SearchableSelectResponse, SettingsRow,
    override_row, override_row_with_error, settings_label_column, vspace,
};

/// plugin ID와 page ID를 함께 비교해 다른 plugin의 같은 이름 페이지와 구분한다.
pub(super) fn find_plugin_settings_entry<'a>(
    entries: &'a [SettingsPageEntry],
    plugin_id: &str,
    page_id: &str,
) -> Option<&'a SettingsPageEntry> {
    entries
        .iter()
        .find(|e| e.plugin_id == plugin_id && e.page.id == page_id)
}

/// 선택한 Appearance 섹션을 그린다. 섹션 목록과 필터는 설정 창이 관리한다.
pub fn draw_appearance_tab(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    sub_tab: &crate::settings_ui::AppearanceSubTab,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
    settings_pages: &[SettingsPageEntry],
) {
    use crate::settings_ui::AppearanceSubTab;
    match sub_tab {
        AppearanceSubTab::Theme => {
            draw_appearance_theme(ui, settings);
        }
        AppearanceSubTab::Colors => {
            draw_appearance_colors(ui, settings);
        }
        AppearanceSubTab::General => {
            draw_appearance_general(ui, settings, font_families, preview_fonts);
        }
        AppearanceSubTab::Display => {
            draw_appearance_display(ui, settings);
        }
        AppearanceSubTab::Tasty => {
            draw_appearance_tasty(ui, settings);
        }
        AppearanceSubTab::Terminal => {
            draw_appearance_terminal(ui, settings, font_families, preview_fonts);
        }
        AppearanceSubTab::Explorer => {
            draw_appearance_explorer(ui, settings, font_families, preview_fonts);
        }
        AppearanceSubTab::Plugin { plugin_id, page_id } => {
            if let Some(entry) = find_plugin_settings_entry(settings_pages, plugin_id, page_id) {
                draw_plugin_settings_page(
                    ui,
                    settings,
                    font_families,
                    preview_fonts,
                    plugin_id,
                    &entry.page,
                );
            }
        }
    }
}

/// Appearance > Theme: preset selection + note. Font settings live in
/// Appearance › General.
fn draw_appearance_theme(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    ui.label(
        egui::RichText::new(t("settings.appearance.theme.heading"))
            .strong()
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_sm);

    // ~/.tasty/themes/ 의 디스크 변경을 settings 화면 진입 시 한 번 더 반영.
    if let Err(e) = tasty_themes::rescan() {
        tracing::warn!("themes rescan failed: {e}");
    }
    let entries = tasty_themes::scan_themes();
    for entry in &entries {
        let is_current = settings.appearance.theme == entry.id;
        let clicked = draw_theme_swatch(ui, &th, &entry.label, &entry.file, is_current);
        if clicked && !is_current {
            // 테마를 바꾸면 개별 색 override를 비운다. 실제 적용은 설정 저장 뒤에 처리한다.
            tasty_themes::apply_theme(&mut settings.appearance, &entry.id);
        }
        ui.add_space(th.spacing_xs.value());
    }

    vspace(ui, th.spacing_sm);
    ui.label(
        egui::RichText::new(t("settings.appearance.theme.hint"))
            .small()
            .color(th.text_muted()),
    );
}

/// 한 테마의 대표 5색 `[bg, surface, accent, accent_alt, success]` 을 `ThemeFile`
/// 에서 파생한다. 디자인 `ThemeSwatch` 매핑:
/// bg=`palette.crust`, surface=`palette.base`, accent=`accent.blue`,
/// accent_alt=`accent.mauve`, success=`accent.green`.
///
/// 각 필드는 테마가 일부만 지정할 수 있어 `Option` 이며, `None` 이면 빌트인 mocha
/// fallback 색으로 대체한다(전역 적용 시 실제로 보이는 색과 일치).
fn representative_swatch(file: &tasty_themes::ThemeFile) -> [HexColor; 5] {
    let base = tasty_themes::mocha_fallback_colors();
    [
        file.palette.crust.unwrap_or(base.crust),
        file.palette.base.unwrap_or(base.base),
        file.accent.blue.unwrap_or(base.blue),
        file.accent.mauve.unwrap_or(base.mauve),
        file.accent.green.unwrap_or(base.green),
    ]
}

/// 프리셋 한 개를 5색 스와치 카드(스트라이프 38px + 라벨)로 그린다. 활성(현재
/// 선택) 시 accent 보더 + 1px ring 으로 강조. 반환값은 클릭 여부.
///
/// 색·보더·간격은 모두 `Theme` 토큰 경유(하드코딩 금지). 스와치 색은 `entry.file`
/// 에서 파생([`representative_swatch`]).
fn draw_theme_swatch(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    file: &tasty_themes::ThemeFile,
    is_current: bool,
) -> bool {
    let swatch = representative_swatch(file);

    let pad = th.spacing_sm.value();
    let stripe_h = THEME_SWATCH_STRIPE_HEIGHT.value();
    // 카드 여백도 글꼴·간격과 함께 Theme의 배율을 적용한다.
    let label_h = th.font_size_body.value() + th.spacing_xs.value();
    let gap = th.spacing_xs.value();
    let card_w = ui.available_width();
    let card_h = pad + stripe_h + gap + label_h + pad;

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_w, card_h), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let corner = th.corner_radius.value();

    // 카드 배경 (surface-raised).
    painter.rect_filled(rect, corner, th.surface_raised().to_egui());

    // 5색 스트라이프 — 동일 폭 5분할.
    let stripe_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + pad, rect.min.y + pad),
        egui::vec2(card_w - pad * 2.0, stripe_h),
    );
    let seg_w = stripe_rect.width() / swatch.len() as f32;
    for (i, color) in swatch.iter().enumerate() {
        let seg = egui::Rect::from_min_size(
            egui::pos2(stripe_rect.min.x + seg_w * i as f32, stripe_rect.min.y),
            egui::vec2(seg_w, stripe_h),
        );
        painter.rect_filled(seg, 0.0, color.to_egui());
    }

    // 라벨 (스트라이프 하단).
    painter.text(
        egui::pos2(rect.min.x + pad, stripe_rect.max.y + gap),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(th.font_size_body.value()),
        th.text_primary().to_egui(),
    );

    // 보더 — 활성: accent 보더 + 1px inner ring. 비활성: border-default(hover 시 강조).
    if is_current {
        painter.rect_stroke(
            rect,
            corner,
            egui::Stroke::new(th.border_width.value(), th.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
        let inner = rect.shrink(th.spacing_xs.value() / 2.0);
        painter.rect_stroke(
            inner,
            (corner - 1.0).max(0.0),
            egui::Stroke::new(th.border_width.value(), th.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
    } else {
        let border = if resp.hovered() {
            th.border_strong().to_egui()
        } else {
            th.border_default().to_egui()
        };
        painter.rect_stroke(
            rect,
            corner,
            egui::Stroke::new(th.border_width.value(), border),
            egui::StrokeKind::Inside,
        );
    }

    resp.clicked()
}

/// Convert FontSettings → EffectiveFont (used for default-font preview).
fn effective_from_settings(s: &FontSettings) -> EffectiveFont {
    EffectiveFont {
        font_family: s.font_family.clone(),
        font_size: s.font_size,
        custom_font_path: s.custom_font_path.clone(),
        line_height: s.line_height,
        font_scale_mode: s.font_scale_mode.clone(),
    }
}

/// Appearance > General: default font settings (single source of truth for
/// fields not overridden per-surface) + background opacity.
fn draw_appearance_general(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    ui.label(
        egui::RichText::new(t("settings.appearance.font.default_heading"))
            .strong()
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.appearance.font.default_hint"))
            .small()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    // 글꼴 격자와 아래 합자·배경 투명도 행이 한 서브탭의 라벨 열을 같이 쓴다.
    let ligatures = SettingsRow::new(t("settings.appearance.ligatures_label"));
    let opacity = SettingsRow::new(t("settings.appearance.background_opacity_label"));
    let font_rows = font_setting_rows();
    let col = settings_label_column(ui, &th, font_rows.iter().chain([&ligatures, &opacity]));
    default_font_section(ui, font_rows, col, settings, font_families, preview_fonts);

    vspace(ui, th.spacing_lg);
    ui.separator();
    vspace(ui, th.spacing_sm);

    ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();
    ligatures.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(ui, &th, &mut settings.appearance.ligatures, None, true);
    });
    opacity.show(ui, &th, col, |ui| {
        ui.add(egui::Slider::new(
            &mut settings.appearance.background_opacity,
            0.0..=1.0,
        ));
    });
}

/// UI 배율 선택 카드의 고정 치수.
const DISPLAY_CARD_WIDTH: LogicalPx = LogicalPx(96.0);
const DISPLAY_CARD_HEIGHT: LogicalPx = LogicalPx(76.0);

/// UI 배율 선택 카드. 실제 배율의 글자 샘플과 선택 표시를 그린다.
fn draw_appearance_display(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);
    ui.label(t("settings.appearance.ui_scale_label"));
    vspace(ui, th.spacing_sm);

    // (scale_key, label_key) — 순서대로 가로 배치.
    let cards = [
        ("small", "settings.appearance.ui_scale_small"),
        ("medium", "settings.appearance.ui_scale_medium"),
        ("large", "settings.appearance.ui_scale_large"),
    ];

    let mut selected: Option<&str> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        for (scale_key, label_key) in cards {
            if display_scale_card(ui, &th, settings, scale_key, label_key) {
                selected = Some(scale_key);
            }
        }
    });
    if let Some(scale_key) = selected {
        settings.appearance.ui_scale = scale_key.to_string();
    }
}

/// 한 UI-scale 토글 카드 — surface-raised 배경 + 실제 배율의 "Aa" 프리뷰 +
/// 라벨. 활성 카드는 accent 보더 + 1px ring. 클릭되면 `true`.
fn display_scale_card(
    ui: &mut egui::Ui,
    th: &Theme,
    settings: &Settings,
    scale_key: &str,
    label_key: &str,
) -> bool {
    let is_active = settings.appearance.ui_scale == scale_key;
    let factor = crate::settings::AppearanceSettings::ui_scale_factor_for(scale_key);

    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(DISPLAY_CARD_WIDTH.value(), DISPLAY_CARD_HEIGHT.value()),
        egui::Sense::click(),
    );

    // 배경 (surface-raised).
    let radius = th.corner_radius.value();
    ui.painter()
        .rect_filled(rect, radius, egui::Color32::from(th.surface_raised()));

    // 보더 / ring.
    if is_active {
        // accent 보더 (내측 1px) + 바깥 1px accent ring.
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                th.border_width.value(),
                egui::Color32::from(th.border_focus()),
            ),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_stroke(
            rect.expand(th.border_width.value()),
            radius,
            egui::Stroke::new(
                th.border_width.value(),
                egui::Color32::from(th.border_focus()),
            ),
            egui::StrokeKind::Outside,
        );
    } else {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                th.border_width.value(),
                egui::Color32::from(th.border_frame()),
            ),
            egui::StrokeKind::Inside,
        );
    }

    let text_color = egui::Color32::from(if is_active {
        th.text_primary()
    } else {
        th.text_muted_raised()
    });

    // "Aa" 프리뷰 — 실제 배율로 스케일된 글리프 샘플 (자연어 아님 → i18n 예외).
    let preview_size = th.font_size_heading.value() * factor;
    ui.painter().text(
        egui::pos2(
            rect.center().x,
            rect.min.y + DISPLAY_CARD_HEIGHT.value() * 0.4,
        ),
        egui::Align2::CENTER_CENTER,
        "Aa",
        egui::FontId::proportional(preview_size),
        text_color,
    );

    // 라벨 (Small / Medium / Large).
    ui.painter().text(
        egui::pos2(
            rect.center().x,
            rect.max.y - DISPLAY_CARD_HEIGHT.value() * 0.2,
        ),
        egui::Align2::CENTER_CENTER,
        t(label_key),
        egui::FontId::proportional(th.font_size_body.value()),
        text_color,
    );

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    resp.clicked()
}

/// Reset the Tasty curated overrides (accent / sidebar bg) + active tab
/// indicator back to the current theme's defaults. Pure (no egui) so the reset
/// semantics can be unit-tested.
fn reset_tasty_to_theme_defaults(app: &mut crate::settings::AppearanceSettings) {
    app.theme_overrides.blue = None;
    app.theme_overrides.mantle = None;
    app.active_tab_indicator = ActiveTabIndicator::default();
}

/// Appearance > Tasty: app-chrome theming. Accent / Sidebar background are
/// curated shortcuts into `theme_overrides` (`blue` / `mantle`) — the same
/// single source the Colors picker edits. Active tab indicator selects the
/// `active_tab_indicator` style. "Use theme defaults" clears the curated
/// overrides and resets the indicator.
fn draw_appearance_tasty(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    ui.label(
        egui::RichText::new(t("settings.appearance.tasty.heading"))
            .strong()
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.appearance.tasty.hint"))
            .small()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    // ── Accent → theme_overrides.blue ──
    draw_tasty_color_row(
        ui,
        &th,
        settings,
        "settings.appearance.tasty.accent",
        |c| c.blue,
        |p| p.blue,
        |p, v| p.blue = v,
    );
    // ── Sidebar background → theme_overrides.mantle ──
    draw_tasty_color_row(
        ui,
        &th,
        settings,
        "settings.appearance.tasty.sidebar_bg",
        |c| c.mantle,
        |p| p.mantle,
        |p, v| p.mantle = v,
    );

    vspace(ui, th.spacing_md);

    // ── Active tab indicator (Underline / Fill / Dot) ──
    ui.label(
        egui::RichText::new(t("settings.appearance.tasty.indicator_label"))
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_xs);
    ui.horizontal(|ui| {
        for (variant, key) in [
            (
                ActiveTabIndicator::Underline,
                "settings.appearance.tasty.indicator_underline",
            ),
            (
                ActiveTabIndicator::Fill,
                "settings.appearance.tasty.indicator_fill",
            ),
            (
                ActiveTabIndicator::Dot,
                "settings.appearance.tasty.indicator_dot",
            ),
        ] {
            ui.selectable_value(
                &mut settings.appearance.active_tab_indicator,
                variant,
                t(key),
            );
        }
    });

    vspace(ui, th.spacing_md);

    // ── Use theme defaults ──
    if ui
        .button(t("settings.appearance.tasty.use_defaults"))
        .clicked()
    {
        reset_tasty_to_theme_defaults(&mut settings.appearance);
    }
}

/// 색 행 한 줄 — 높이 control-height(`input_height`)에 세로 가운데 정렬한다.
/// 시안 색 행은 `alignItems: center`, `minHeight: control-height`다.
fn color_row_line(ui: &mut egui::Ui, th: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    let h = th.input_height().value();
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_height(h);
            add(ui);
        },
    );
}

/// Default 행의 hex 칸. 쓰고 있는 기본값을 보여 주는 칸이라 disabled가 아니라 읽기 전용이다
/// (포커스·선택·복사 가능, 편집 불가). 입력이 와도 `hex`는 바뀌지 않는다.
fn default_hex_field(ui: &mut egui::Ui, th: &Theme, hex: &mut String) -> egui::Response {
    Input::new()
        .mono(true)
        .read_only(true)
        .width(th.field_width_xs.value())
        .show(ui, th, hex)
}

/// 기본값을 따르는 글꼴 행의 컨트롤. 지금 적용된 값을 읽기 전용 상자에 보인다.
/// 적용된 값이 없으면 빈 칸이며 플레이스홀더를 두지 않는다.
fn default_value_field(ui: &mut egui::Ui, th: &Theme, value: &str, width: LogicalPx, mono: bool) {
    let mut shown = value.to_owned();
    Input::new()
        .mono(mono)
        .read_only(true)
        .width(width.value())
        .full_value_tooltip(true)
        .show(ui, th, &mut shown);
}

/// 색 override 행의 입력값. 행마다 다른 것은 이름 표기, hex 버퍼 id, 체크박스 문구, 저장 위치다.
struct OverrideRow<'a> {
    name: egui::RichText,
    /// hex 입력 버퍼는 egui 메모리에 행별로 보관 — override 값을 매 프레임 문자열로
    /// 되돌리면 편집 중 부분 입력(`#89b4f`)이 즉시 덮어써져 타이핑이 불가능해진다.
    buf_id: egui::Id,
    default_label: &'a str,
    base: HexColor,
    cur: Option<HexColor>,
}

/// 한 색 행: [override dot] 이름 · hex 입력 · swatch · 기본값 체크박스.
/// 기본값 체크 = `None`(base 추종, 읽기 전용 입력) / 해제 = `Some(hex)`.
fn draw_override_row(
    ui: &mut egui::Ui,
    th: &Theme,
    overrides: &mut PartialColors,
    row: OverrideRow<'_>,
    set: impl Fn(&mut PartialColors, Option<HexColor>),
) {
    let OverrideRow { buf_id, base, .. } = row;
    let is_ov = row.cur.is_some();
    let val = row.cur.unwrap_or(base);

    color_row_line(ui, th, |ui| {
        // ── override dot ──
        let dot = COLOR_OVERRIDE_DOT_SIZE.value();
        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(dot, dot), egui::Sense::hover());
        if is_ov {
            ui.painter().circle_filled(
                dot_rect.center(),
                dot / 2.0,
                egui::Color32::from(th.accent_primary()),
            );
        }

        // ── 행 이름 (override 시 강조) ──
        ui.add_sized(
            egui::vec2(COLOR_FIELD_NAME_WIDTH.value(), COLOR_SWATCH_SIZE.value()),
            egui::Label::new(row.name.color(if is_ov {
                th.text_primary()
            } else {
                th.text_muted()
            })),
        );

        // ── hex 입력 ──
        if is_ov {
            let mut text = ui
                .data(|d| d.get_temp::<String>(buf_id))
                .unwrap_or_else(|| val.to_hex());
            let resp = Input::new()
                .mono(true)
                .width(th.field_width_xs.value())
                .show(ui, th, &mut text);
            if resp.changed()
                && let Some(parsed) = HexColor::from_hex(text.trim())
            {
                set(overrides, Some(parsed));
            }
            ui.data_mut(|d| d.insert_temp(buf_id, text));
        } else {
            default_hex_field(ui, th, &mut base.to_hex());
        }

        // ── swatch ──
        let sw = COLOR_SWATCH_SIZE.value();
        let (sw_rect, _) = ui.allocate_exact_size(egui::vec2(sw, sw), egui::Sense::hover());
        ui.painter()
            .rect_filled(sw_rect, 3.0, egui::Color32::from(val));
        ui.painter().rect_stroke(
            sw_rect,
            3.0,
            egui::Stroke::new(
                th.border_width.value(),
                egui::Color32::from(th.border_strong()),
            ),
            egui::StrokeKind::Inside,
        );

        // ── 기본값 체크박스 (디자인 settings_window.jsx:113 = <Checkbox>) ──
        let mut use_default = !is_ov;
        if tasty_ui_widgets::checkbox(
            ui,
            th,
            &mut use_default,
            row.default_label,
            true,
            tasty_ui_widgets::Ground::Panel,
        )
        .changed()
        {
            if use_default {
                set(overrides, None);
            } else {
                // override 활성화: base 값으로 시드 + 입력 버퍼도 base 로 재설정해
                // 이전 세션의 stale 버퍼가 base 시드를 덮어쓰지 않게 한다.
                set(overrides, Some(base));
                let seed = base.to_hex();
                ui.data_mut(|d| d.insert_temp(buf_id, seed));
            }
        }
    });
}

/// A single Tasty curated color row — friendly label (i18n), hex input, swatch,
/// and a "Default" checkbox. `base`/`get`/`set` point at one field of
/// `theme_base`/`theme_overrides`, giving identical set/clear semantics to the
/// Colors picker's [`draw_color_picker_row`] but with a friendly label instead
/// of the raw mono token name.
fn draw_tasty_color_row(
    ui: &mut egui::Ui,
    th: &Theme,
    settings: &mut Settings,
    label_key: &str,
    base: fn(&ThemeColors) -> HexColor,
    get: fn(&PartialColors) -> Option<HexColor>,
    set: fn(&mut PartialColors, Option<HexColor>),
) {
    let row = OverrideRow {
        name: egui::RichText::new(t(label_key)),
        buf_id: ui.make_persistent_id(("appearance_tasty_hex", label_key)),
        default_label: t("settings.appearance.colors.default"),
        base: base(&settings.appearance.theme_base),
        cur: get(&settings.appearance.theme_overrides),
    };
    let overrides = &mut settings.appearance.theme_overrides;
    draw_override_row(ui, th, overrides, row, set);
}

/// Appearance > Terminal: font override + preview + colors
fn draw_appearance_terminal(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
) {
    draw_surface_font_section(
        ui,
        settings,
        font_families,
        preview_fonts,
        SurfaceFontTarget::Terminal,
    );

    draw_terminal_surface_colors(ui, settings);
}

/// Appearance › Explorer: 내장 파일 관리자 surface 의 폰트 override (T11). 저장
/// 슬롯은 `appearance.plugin_font_overrides["explorer"]` — `effective_font_for_kind
/// ("explorer")` 가 읽어 explorer surface 렌더에 적용된다.
fn draw_appearance_explorer(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
) {
    draw_surface_font_section(
        ui,
        settings,
        font_families,
        preview_fonts,
        SurfaceFontTarget::Plugin {
            storage_key: "explorer",
        },
    );
}

/// 기본 글꼴과 surface별 글꼴 입력이 공유하는 pt 범위. 화면 길이가 아니므로 LogicalPx를 쓰지 않는다.
fn font_size_spec() -> super::number::NumberSpec<'static> {
    super::number::NumberSpec::int(6.0, 72.0)
}

/// 기본 글꼴과 surface별 글꼴 입력이 공유하는 줄 높이 배수 범위.
fn line_height_spec() -> super::number::NumberSpec<'static> {
    super::number::NumberSpec::int(0.8, 2.0).decimals(2)
}

const TERMINAL_SURFACE_ID: &str = "terminal";

/// Which terminal surface background a picker row binds to (Focused/Unfocused).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SurfaceBgField {
    Focused,
    Unfocused,
}

impl SurfaceBgField {
    /// Stable, language-independent key for the per-row hex edit buffer id.
    fn key(self) -> &'static str {
        match self {
            SurfaceBgField::Focused => "surface_focused_bg",
            SurfaceBgField::Unfocused => "surface_unfocused_bg",
        }
    }
    fn get(self, p: &PartialSurfaceTheme) -> Option<HexColor> {
        match self {
            SurfaceBgField::Focused => p.focused_bg,
            SurfaceBgField::Unfocused => p.unfocused_bg,
        }
    }
    fn set(self, p: &mut PartialSurfaceTheme, v: Option<HexColor>) {
        match self {
            SurfaceBgField::Focused => p.focused_bg = v,
            SurfaceBgField::Unfocused => p.unfocused_bg = v,
        }
    }
    fn base(self, st: &SurfaceTheme) -> HexColor {
        match self {
            SurfaceBgField::Focused => st.focused_bg,
            SurfaceBgField::Unfocused => st.unfocused_bg,
        }
    }
}

/// Current terminal surface bg override for `field` (`None` = follow base).
/// Pure (no egui) so set/clear/resolve can be unit-tested.
fn surface_bg_override(ov: &PartialColors, field: SurfaceBgField) -> Option<HexColor> {
    ov.surface_themes
        .get(TERMINAL_SURFACE_ID)
        .and_then(|p| field.get(p))
}

/// Set (`Some`) or clear (`None`) the terminal surface bg override for `field`.
/// Clearing prunes the map entry once its `PartialSurfaceTheme` is empty so the
/// override map doesn't accumulate `{"terminal": {}}` husks.
fn set_surface_bg_override(ov: &mut PartialColors, field: SurfaceBgField, val: Option<HexColor>) {
    match val {
        Some(c) => {
            let entry = ov
                .surface_themes
                .entry(TERMINAL_SURFACE_ID.to_string())
                .or_default();
            field.set(entry, Some(c));
        }
        None => {
            if let Some(p) = ov.surface_themes.get_mut(TERMINAL_SURFACE_ID) {
                field.set(p, None);
                if p.is_empty() {
                    ov.surface_themes.remove(TERMINAL_SURFACE_ID);
                }
            }
        }
    }
}

/// Base (preset) terminal surface bg for `field`, read from resolved
/// `theme_base` (no hardcoding). Falls back to `FALLBACK_SURFACE` if the base
/// has no `terminal` entry.
fn surface_bg_base(base: &ThemeColors, field: SurfaceBgField) -> HexColor {
    base.surface_themes
        .get(TERMINAL_SURFACE_ID)
        .map(|st| field.base(st))
        .unwrap_or_else(|| field.base(&FALLBACK_SURFACE))
}

/// Appearance › Terminal: surface background override pickers (Focused /
/// Unfocused). Writes into `theme_overrides.surface_themes["terminal"]`, the
/// same override layer the Colors picker edits (resolve via `apply_partial`).
fn draw_terminal_surface_colors(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);
    ui.label(
        egui::RichText::new(t("settings.appearance.terminal.surface_heading"))
            .strong()
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.appearance.terminal.surface_hint"))
            .small()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);
    draw_surface_bg_row(
        ui,
        &th,
        settings,
        t("settings.appearance.terminal.focused_bg"),
        SurfaceBgField::Focused,
    );
    draw_surface_bg_row(
        ui,
        &th,
        settings,
        t("settings.appearance.terminal.unfocused_bg"),
        SurfaceBgField::Unfocused,
    );
}

/// One terminal surface bg row — mirrors `draw_color_picker_row`'s visual
/// ([override dot] label · hex input · swatch · "Use default" checkbox) but
/// binds to the nested `surface_themes["terminal"]` override instead of a flat
/// `PartialColors` field. "Use default" checked = base-following (input
/// read-only) / unchecked = `Some(hex)` override.
fn draw_surface_bg_row(
    ui: &mut egui::Ui,
    th: &Theme,
    settings: &mut Settings,
    label: &str,
    field: SurfaceBgField,
) {
    let row = OverrideRow {
        name: egui::RichText::new(label),
        buf_id: ui.make_persistent_id(("appearance_surface_hex", field.key())),
        default_label: t("settings.appearance.terminal.use_default"),
        base: surface_bg_base(&settings.appearance.theme_base, field),
        cur: surface_bg_override(&settings.appearance.theme_overrides, field),
    };
    let overrides = &mut settings.appearance.theme_overrides;
    draw_override_row(ui, th, overrides, row, |ov, value| {
        set_surface_bg_override(ov, field, value)
    });
}

/// Target of a font override section: either the host-internal `terminal_font`
/// field or a plugin-contributed slot keyed by `storage_key` inside
/// `appearance.plugin_font_overrides`.
///
/// Held by reference (`&'a`) because Plugin uses a borrowed storage key — the
/// owning string lives in `SettingsPageContribute::items`.
#[derive(Clone, Copy)]
pub(super) enum SurfaceFontTarget<'a> {
    Terminal,
    /// `storage_key` is the key inside `plugin_font_overrides` *and* doubles as
    /// the surface id used to pick a theme `SurfaceTheme` for the preview.
    Plugin {
        storage_key: &'a str,
    },
}

impl<'a> SurfaceFontTarget<'a> {
    fn salt(self) -> &'a str {
        match self {
            SurfaceFontTarget::Terminal => "terminal",
            SurfaceFontTarget::Plugin { storage_key } => storage_key,
        }
    }

    fn override_mut(self, app: &mut crate::settings::AppearanceSettings) -> &mut FontOverride {
        match self {
            SurfaceFontTarget::Terminal => &mut app.terminal_font,
            SurfaceFontTarget::Plugin { storage_key } => app
                .plugin_font_overrides
                .entry(storage_key.to_string())
                .or_default(),
        }
    }

    fn effective(self, app: &crate::settings::AppearanceSettings) -> EffectiveFont {
        match self {
            SurfaceFontTarget::Terminal => app.effective_terminal_font(),
            SurfaceFontTarget::Plugin { storage_key } => app.effective_font_for_kind(storage_key),
        }
    }

    fn surface_id(self) -> &'a str {
        match self {
            SurfaceFontTarget::Terminal => "terminal",
            SurfaceFontTarget::Plugin { storage_key } => storage_key,
        }
    }
}

pub(super) fn draw_surface_font_section(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
    target: SurfaceFontTarget<'_>,
) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.appearance.font.override_heading"))
            .strong()
            .color(th.text_primary()),
    );
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.appearance.font.override_hint"))
            .small()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    // 미리보기는 창 폭과 관계없이 격자 아래에 둔다 — 옆 열이 없으니 좁은 폭에서도 겹치지 않는다.
    let default_font = settings.appearance.default_font.clone();
    let slot = preview_slot(target.salt());
    let terminal = matches!(target, SurfaceFontTarget::Terminal);
    let judge = RowJudge::for_surface(terminal, preview_fonts, &slot);
    let active = font_override_grid(
        ui,
        target.override_mut(&mut settings.appearance),
        &default_font,
        font_families,
        target.salt(),
        judge,
    );
    vspace(ui, th.spacing_lg);
    let eff = target.effective(&settings.appearance);
    // 터미널 판정은 파일을 읽으므로 입력을 마친 뒤에만 돈다. 줄을 그릴 행이 없으면 돌 이유도 없다.
    if terminal {
        preview_fonts.settle_terminal_row(ui.ctx(), &settings.appearance, &eff, active);
    }
    let colors = crate::theme::theme().surface(target.surface_id()).clone();
    draw_font_preview(
        ui,
        &eff,
        &colors,
        &settings.appearance,
        target.salt(),
        preview_fonts,
    );
}

// Colors 화면은 각 색 필드의 override를 편집한다. None이면 테마 기본값을 따른다.

/// 색 스와치 한 변 (디자인 jsx: 18px). 4px 그리드 밖이고 대응 치수 토큰이 없다.
const COLOR_SWATCH_SIZE: LogicalPx = LogicalPx(18.0);

/// 색 override 여부를 표시하는 점. 상태 점과 역할·크기가 달라 별도 치수를 유지한다(docs/design/systems/theme.md).
const COLOR_OVERRIDE_DOT_SIZE: LogicalPx = LogicalPx(5.0);

/// 색 토큰 이름 컬럼 폭 — 행 간 입력/스와치/체크박스 정렬용.
const COLOR_FIELD_NAME_WIDTH: LogicalPx = LogicalPx(150.0);

/// Theme 프리셋 스와치(design #5): 5색 스트라이프 높이 / 스트라이프~라벨 사이 여백.
const THEME_SWATCH_STRIPE_HEIGHT: LogicalPx = LogicalPx(38.0);

/// 한 색 행: 표시 이름(기술 토큰, 비번역) + base/override 접근자(fn 포인터).
struct ColorRowDef {
    /// 색 토큰 이름(`crust`/`blue`/`ansi_black` …). 자연어가 아닌 기술 식별자라
    /// 번역하지 않는다(i18n 예외 — 디자인도 mono 리터럴로 표기).
    name: &'static str,
    /// resolved `theme_base` 에서 이 필드의 base 색을 읽는다.
    base: fn(&ThemeColors) -> HexColor,
    /// `theme_overrides` 에서 현재 override 값(없으면 `None`).
    get: fn(&PartialColors) -> Option<HexColor>,
    /// `theme_overrides` 의 이 필드를 설정/클리어.
    set: fn(&mut PartialColors, Option<HexColor>),
}

/// collapsible 그룹: 표시명(i18n) + 선택적 note(i18n) + 기본 열림 + 행들.
struct ColorGroupDef {
    name_key: &'static str,
    note_key: Option<&'static str>,
    default_open: bool,
    rows: Vec<ColorRowDef>,
}

/// `PartialColors`/`ThemeColors` 의 한 필드를 fn 포인터 묶음으로 바인딩.
macro_rules! color_row {
    ($field:ident) => {
        ColorRowDef {
            name: stringify!($field),
            base: |c| c.$field,
            get: |p| p.$field,
            set: |p, v| p.$field = v,
        }
    };
}

/// 디자인 `PALETTE_GROUPS` 와 동일한 6그룹/47필드 구성.
/// 기본 열림: Surfaces · Text · Accents / 기본 접힘: Overlays · Terminal-specific · ANSI 16.
fn color_groups() -> Vec<ColorGroupDef> {
    vec![
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.surfaces",
            note_key: Some("settings.appearance.colors.group.surfaces_note"),
            default_open: true,
            rows: vec![
                color_row!(crust),
                color_row!(mantle),
                color_row!(base),
                color_row!(surface0),
                color_row!(surface1),
                color_row!(surface2),
            ],
        },
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.overlays",
            note_key: None,
            default_open: false,
            rows: vec![
                color_row!(overlay0),
                color_row!(overlay1),
                color_row!(overlay2),
            ],
        },
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.text",
            note_key: None,
            default_open: true,
            rows: vec![
                color_row!(text),
                color_row!(subtext1),
                color_row!(subtext0),
                color_row!(placeholder),
            ],
        },
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.accents",
            note_key: None,
            default_open: true,
            rows: vec![
                color_row!(blue),
                color_row!(green),
                color_row!(red),
                color_row!(yellow),
                color_row!(peach),
                color_row!(mauve),
                color_row!(teal),
                color_row!(sky),
                color_row!(lavender),
                color_row!(flamingo),
                color_row!(pink),
                color_row!(maroon),
                color_row!(rosewater),
            ],
        },
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.terminal",
            note_key: None,
            default_open: false,
            rows: vec![
                color_row!(selection_bg),
                color_row!(vi_cursor_bg),
                color_row!(search_match_bg),
                color_row!(search_match_active_bg),
                // 활성 매치 글자색은 그 바탕색 바로 뒤에 두어 둘을 함께 보며 고르게 한다.
                color_row!(search_match_active_fg),
            ],
        },
        ColorGroupDef {
            name_key: "settings.appearance.colors.group.ansi",
            note_key: None,
            default_open: false,
            rows: vec![
                color_row!(ansi_black),
                color_row!(ansi_red),
                color_row!(ansi_green),
                color_row!(ansi_yellow),
                color_row!(ansi_blue),
                color_row!(ansi_magenta),
                color_row!(ansi_cyan),
                color_row!(ansi_white),
                color_row!(ansi_bright_black),
                color_row!(ansi_bright_red),
                color_row!(ansi_bright_green),
                color_row!(ansi_bright_yellow),
                color_row!(ansi_bright_blue),
                color_row!(ansi_bright_magenta),
                color_row!(ansi_bright_cyan),
                color_row!(ansi_bright_white),
            ],
        },
    ]
}

/// 그룹 내 현재 override 가 걸린 행 수.
fn group_changed_count(group: &ColorGroupDef, overrides: &PartialColors) -> usize {
    group
        .rows
        .iter()
        .filter(|r| (r.get)(overrides).is_some())
        .count()
}

/// Appearance > Colors: 프리셋 색 개별 override picker.
fn draw_appearance_colors(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    let groups = color_groups();

    // 전체 override 개수 (picker 가 다루는 47 flat 필드 한정 — surface_themes 등
    // 비-picker override 는 세지도 지우지도 않는다).
    let total: usize = {
        let ov = &settings.appearance.theme_overrides;
        groups.iter().map(|g| group_changed_count(g, ov)).sum()
    };

    // 헤더: 설명 + 우측 Reset all (N).
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if total > 0 {
                crate::i18n::t_fmt("settings.appearance.colors.reset_all_n", &total.to_string())
            } else {
                t("settings.appearance.colors.reset_all").to_string()
            };
            if Button::new(&label)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(total > 0)
                .show(ui, &th)
                .clicked()
            {
                let ov = &mut settings.appearance.theme_overrides;
                for r in groups.iter().flat_map(|g| &g.rows) {
                    (r.set)(ov, None);
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(t("settings.appearance.colors.intro"))
                            .small()
                            .color(th.text_muted()),
                    )
                    .wrap(),
                );
            });
        });
    });

    vspace(ui, th.spacing_xs);

    for group in &groups {
        draw_color_group(ui, &th, settings, group);
    }
}

/// 한 collapsible 색 그룹 — 헤더(이름·note·N changed·Reset) + 행 목록.
fn draw_color_group(ui: &mut egui::Ui, th: &Theme, settings: &mut Settings, group: &ColorGroupDef) {
    let changed = group_changed_count(group, &settings.appearance.theme_overrides);

    let id = ui.make_persistent_id(("appearance_colors_group", group.name_key));
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        group.default_open,
    );

    vspace(ui, th.spacing_xs);
    let header = state.show_header(ui, |ui| {
        ui.label(
            egui::RichText::new(t(group.name_key))
                .family(tasty_ui_widgets::label_font_family())
                .size(th.font_size_caption.value())
                .strong()
                .color(th.text_muted()),
        );
        if let Some(note_key) = group.note_key {
            ui.label(
                egui::RichText::new(format!("· {}", t(note_key)))
                    .small()
                    .color(th.text_muted()),
            );
        }
        if changed > 0 {
            ui.label(
                egui::RichText::new(crate::i18n::t_fmt(
                    "settings.appearance.colors.changed_n",
                    &changed.to_string(),
                ))
                .family(tasty_ui_widgets::label_font_family())
                .size(th.font_size_caption.value())
                .color(th.accent_primary_raised()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("settings.appearance.colors.reset")).clicked() {
                    let ov = &mut settings.appearance.theme_overrides;
                    for r in &group.rows {
                        (r.set)(ov, None);
                    }
                }
            });
        }
    });
    header.body(|ui| {
        for row in &group.rows {
            draw_color_picker_row(ui, th, settings, row);
        }
    });
}

/// 색 토큰 행: 이름은 mono, 체크박스 문구는 "Default".
fn draw_color_picker_row(
    ui: &mut egui::Ui,
    th: &Theme,
    settings: &mut Settings,
    row: &ColorRowDef,
) {
    let row_def = OverrideRow {
        name: egui::RichText::new(row.name).monospace(),
        buf_id: ui.make_persistent_id(("appearance_colors_hex", row.name)),
        default_label: t("settings.appearance.colors.default"),
        base: (row.base)(&settings.appearance.theme_base),
        cur: (row.get)(&settings.appearance.theme_overrides),
    };
    let overrides = &mut settings.appearance.theme_overrides;
    draw_override_row(ui, th, overrides, row_def, row.set);
}

/// 글꼴 패밀리 SearchableSelect. `value` 는 설정의 패밀리 이름이고 `""` 는 monospace 기본값이며,
/// 기본값은 검색어와 관계없이 맨 위에 고정된 행이다. 목록에 없는 이름(미설치·로딩 중)도 트리거에
/// 그대로 보이고 값이 바뀌지 않는다. `salt` 로 같은 화면의 여러 행을 구분한다.
fn font_family_picker(
    ui: &mut egui::Ui,
    value: &mut String,
    font_families: &Option<Vec<String>>,
    salt: &str,
) -> SearchableSelectResponse {
    let th = crate::theme::theme();
    let pinned = [SearchableSelectOption::with_value(
        "",
        t("settings.appearance.font_family_default"),
    )];
    let options: Vec<SearchableSelectOption<'_>> = font_families
        .iter()
        .flatten()
        .map(|family| SearchableSelectOption::new(family))
        .collect();
    let labels = SearchableSelectLabels {
        placeholder: t("settings.appearance.font_family_default"),
        search_placeholder: t("settings.appearance.font_search_placeholder"),
        empty: t("settings.appearance.font_no_match"),
        loading: t("settings.appearance.font_loading"),
    };
    let id_salt = format!("font_family_combo_{salt}");
    // 좁은 설정 창에서 트리거가 남은 폭을 넘으면 오른쪽 끝(chevron)이 행 밖으로 나가 다른 위젯에
    // 가려질 수 있다. 남은 폭 안에 묶고, 긴 이름은 트리거를 늘리지 않고 말줄임한다.
    let width = th.field_width_lg.min(LogicalPx(ui.available_width()));
    let mut selected = Some(std::mem::take(value));
    let out = SearchableSelect::new(&id_salt, &options, labels)
        .pinned(&pinned)
        .loading(font_families.is_none())
        .width(width)
        .show_response(ui, &th, &mut selected);
    *value = selected.unwrap_or_default();
    out
}

/// 기본 글꼴 격자를 그리고 미리보기를 그 아래 콘텐츠 전폭에 둔다(글꼴 override 와 같은 배치).
/// 글꼴 콤보·커스텀 글꼴 입력칸의 응답과 미리보기의 위쪽 경계를 돌려준다(컨트롤이 미리보기와
/// 겹치지 않는지 재는 시험이 읽는다).
fn default_font_section(
    ui: &mut egui::Ui,
    rows: [SettingsRow<'_>; 5],
    col: LogicalPx,
    settings: &mut Settings,
    font_families: &mut Option<Vec<String>>,
    preview_fonts: &mut PreviewFonts,
) -> (egui::Response, egui::Response, f32) {
    let th = crate::theme::theme();
    let slot = preview_slot("default");
    let (family, custom) = font_settings_grid(
        ui,
        rows,
        col,
        &mut settings.appearance.default_font,
        font_families,
        "default",
        RowJudge::for_surface(false, preview_fonts, &slot),
    );
    vspace(ui, th.spacing_lg);
    let preview_top = ui.cursor().top();
    let preview_eff = effective_from_settings(&settings.appearance.default_font);
    let preview_colors = th.surface("terminal").clone();
    draw_font_preview(
        ui,
        &preview_eff,
        &preview_colors,
        &settings.appearance,
        "default",
        preview_fonts,
    );
    (family, custom, preview_top)
}

/// Edit a `FontSettings` (no fallback semantics — every field is always set).
/// 기본 글꼴 격자의 다섯 행(family · custom font · size · line height · scale mode).
fn font_setting_rows() -> [SettingsRow<'static>; 5] {
    [
        SettingsRow::new(t("settings.appearance.font_family_label")),
        SettingsRow::new(t("settings.appearance.custom_font_label")),
        SettingsRow::new(t("settings.appearance.font_size_label")),
        SettingsRow::new(t("settings.appearance.line_height_label"))
            .hint(t("settings.appearance.line_height_tooltip")),
        SettingsRow::new(t("settings.appearance.font_scale_mode_label"))
            .hint(t("settings.appearance.font_scale_mode_tooltip")),
    ]
}

fn font_settings_grid(
    ui: &mut egui::Ui,
    rows: [SettingsRow<'_>; 5],
    col: LogicalPx,
    font: &mut FontSettings,
    font_families: &mut Option<Vec<String>>,
    salt: &str,
    judge: RowJudge<'_>,
) -> (egui::Response, egui::Response) {
    let th = crate::theme::theme();
    let [family, custom, size, line_height, scale_mode] = rows;
    ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();
    let mut family_combo = None;
    family.show_with_error(ui, &th, col, |ui| {
        let out = font_family_picker(ui, &mut font.font_family, font_families, salt);
        let verdict = judge.verdict(&font.font_family, &font.custom_font_path);
        let line = family_error_line(ui.ctx(), salt, out.open, verdict);
        family_combo = Some(out.response);
        line
    });
    let mut custom_field = None;
    custom.show_with_error(ui, &th, col, |ui| {
        let path = &mut font.custom_font_path;
        let (response, line) = custom_font_field(ui, &th, path, &font.font_family, judge, salt);
        custom_field = Some(response);
        line
    });
    size.show(ui, &th, col, |ui| {
        super::number::number_field_f32(
            ui,
            &th,
            ("appearance_font_size", salt),
            &font_size_spec(),
            &mut font.font_size,
        );
    });
    line_height.show(ui, &th, col, |ui| {
        super::number::number_field_f32(
            ui,
            &th,
            ("appearance_line_height", salt),
            &line_height_spec(),
            &mut font.line_height,
        );
    });
    scale_mode.show(ui, &th, col, |ui| {
        font_scale_mode_combo(ui, &mut font.font_scale_mode, salt, None);
    });
    (
        family_combo.expect("SettingsRow::show runs its control"),
        custom_field.expect("SettingsRow::show runs its control"),
    )
}

/// Edit a `FontOverride` against a `FontSettings` default. Each row has a
/// "use default" checkbox: checked → override field is `None` (input
/// disabled, default value shown for reference). Unchecked → override is
/// `Some(current_effective_value)` and the input is enabled.
/// 글꼴 입력에 포커스가 있거나 family 목록이 열려 있으면 참을 돌려준다.
fn font_override_grid(
    ui: &mut egui::Ui,
    ov: &mut FontOverride,
    default: &FontSettings,
    font_families: &mut Option<Vec<String>>,
    salt: &str,
    judge: RowJudge<'_>,
) -> bool {
    let th = crate::theme::theme();
    let mut active = false;
    let use_default = t("settings.appearance.font.use_default_label");
    let rows = [
        SettingsRow::new(t("settings.appearance.font.override_family_label")),
        SettingsRow::new(t("settings.appearance.font.override_custom_font_label")),
        SettingsRow::new(t("settings.appearance.font.override_size_label")),
        SettingsRow::new(t("settings.appearance.font.override_line_height_label"))
            .hint(t("settings.appearance.line_height_tooltip")),
        SettingsRow::new(t("settings.appearance.font.override_scale_mode_label"))
            .hint(t("settings.appearance.font_scale_mode_tooltip")),
    ];
    let col = settings_label_column(ui, &th, &rows);
    let row_h = th.settings_row_min_height();
    let [family, custom, size, line_height, scale_mode] = rows;
    ui.push_id(("font_override_grid", salt), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        // ── Font family ──
        override_row_with_error(
            ui,
            &th,
            col,
            th.field_width_lg,
            use_default,
            |ui, cell, line| match cell {
                OverrideCell::Label => {
                    family.show_label(ui, &th, col, row_h);
                }
                OverrideCell::Control => match ov.font_family.as_mut() {
                    Some(stored) => {
                        let out = font_family_picker(ui, stored, font_families, salt);
                        active |= out.open;
                        let custom = ov.custom_font_path.as_deref();
                        let custom = custom.unwrap_or(&default.custom_font_path);
                        let verdict = judge.verdict(stored, custom);
                        *line = family_error_line(ui.ctx(), salt, out.open, verdict);
                    }
                    None => {
                        let shown = match default.font_family.as_str() {
                            "" => t("settings.appearance.font_family_default"),
                            name => name,
                        };
                        default_value_field(ui, &th, shown, th.field_width_lg, false);
                        *line = family_default_line(ui.ctx(), salt);
                    }
                },
                OverrideCell::UseDefault => {
                    override_checkbox(ui, &mut ov.font_family, || default.font_family.clone())
                }
            },
        );

        // ── Custom font path ──
        override_row_with_error(
            ui,
            &th,
            col,
            th.field_width_lg,
            use_default,
            |ui, cell, line| match cell {
                OverrideCell::Label => {
                    custom.show_label(ui, &th, col, row_h);
                }
                OverrideCell::Control => match ov.custom_font_path.as_mut() {
                    Some(stored) => {
                        let family = ov.font_family.as_deref().unwrap_or(&default.font_family);
                        let (response, error) =
                            custom_font_field(ui, &th, stored, family, judge, salt);
                        active |= response.has_focus();
                        *line = error;
                    }
                    None => {
                        let shown = &default.custom_font_path;
                        default_value_field(ui, &th, shown, th.field_width_lg, true);
                        *line = custom_font_default_line(ui.ctx(), salt);
                    }
                },
                OverrideCell::UseDefault => override_checkbox(ui, &mut ov.custom_font_path, || {
                    default.custom_font_path.clone()
                }),
            },
        );

        // ── Font size ──
        override_row(
            ui,
            &th,
            col,
            th.field_width_xs,
            use_default,
            |ui, cell| match cell {
                OverrideCell::Label => {
                    size.show_label(ui, &th, col, row_h);
                }
                OverrideCell::Control => match ov.font_size.as_mut() {
                    Some(stored) => {
                        super::number::number_field_f32(
                            ui,
                            &th,
                            ("appearance_override_font_size", salt),
                            &font_size_spec(),
                            stored,
                        );
                    }
                    None => {
                        let shown = font_size_spec().format(f64::from(default.font_size));
                        default_value_field(ui, &th, &shown, th.field_width_xs, true);
                    }
                },
                OverrideCell::UseDefault => {
                    override_checkbox(ui, &mut ov.font_size, || default.font_size)
                }
            },
        );

        // ── Line height ──
        override_row(
            ui,
            &th,
            col,
            th.field_width_xs,
            use_default,
            |ui, cell| match cell {
                OverrideCell::Label => {
                    line_height.show_label(ui, &th, col, row_h);
                }
                OverrideCell::Control => match ov.line_height.as_mut() {
                    Some(stored) => {
                        super::number::number_field_f32(
                            ui,
                            &th,
                            ("appearance_override_line_height", salt),
                            &line_height_spec(),
                            stored,
                        );
                    }
                    None => {
                        let shown = line_height_spec().format(f64::from(default.line_height));
                        default_value_field(ui, &th, &shown, th.field_width_xs, true);
                    }
                },
                OverrideCell::UseDefault => {
                    override_checkbox(ui, &mut ov.line_height, || default.line_height)
                }
            },
        );

        // ── Font scale mode ──
        override_row(
            ui,
            &th,
            col,
            th.field_width_md,
            use_default,
            |ui, cell| match cell {
                OverrideCell::Label => {
                    scale_mode.show_label(ui, &th, col, row_h);
                }
                OverrideCell::Control => match ov.font_scale_mode.as_mut() {
                    Some(stored) => {
                        font_scale_mode_combo(ui, stored, salt, Some(th.field_width_md.value()))
                    }
                    None => {
                        let shown = if default.font_scale_mode == "auto" {
                            t("settings.appearance.font_scale_mode_auto")
                        } else {
                            t("settings.appearance.font_scale_mode_fixed")
                        };
                        default_value_field(ui, &th, shown, th.field_width_md, false);
                    }
                },
                OverrideCell::UseDefault => override_checkbox(ui, &mut ov.font_scale_mode, || {
                    default.font_scale_mode.clone()
                }),
            },
        );
    });
    active
}

/// "Use default" checkbox: checked when override is None.
/// Toggling on → set to None; toggling off → seed with the current default.
fn override_checkbox<T, F>(ui: &mut egui::Ui, slot: &mut Option<T>, default_provider: F)
where
    F: FnOnce() -> T,
{
    let th = crate::theme::theme();
    let mut use_default = slot.is_none();
    let label = t("settings.appearance.font.use_default_label");
    if tasty_ui_widgets::checkbox(
        ui,
        &th,
        &mut use_default,
        label,
        true,
        tasty_ui_widgets::Ground::Panel,
    )
    .changed()
    {
        if use_default {
            *slot = None;
        } else if slot.is_none() {
            *slot = Some(default_provider());
        }
    }
}

/// `width` 가 없으면 이전 egui ComboBox 의 기본 폭을 쓴다.
fn font_scale_mode_combo(ui: &mut egui::Ui, value: &mut String, salt: &str, width: Option<f32>) {
    const MODES: [&str; 2] = ["auto", "fixed"];
    let th = crate::theme::theme();
    let labels = [
        t("settings.appearance.font_scale_mode_auto"),
        t("settings.appearance.font_scale_mode_fixed"),
    ];
    let mut idx = usize::from(value.as_str() != "auto");
    let width = width.unwrap_or(ui.spacing().combo_width);
    let salt = format!("font_scale_mode_{salt}");
    if tasty_ui_widgets::select(ui, &th, &salt, &mut idx, &labels, width, true) {
        *value = MODES[idx].to_string();
    }
}

/// Draw a 2-row colored preview block for an `EffectiveFont`. `slot` is a
/// short id ("default"/"terminal"/"markdown"/"explorer") used as the egui font
/// family slot name in `preview_fonts`.
fn draw_font_preview(
    ui: &mut egui::Ui,
    eff: &EffectiveFont,
    colors: &SurfaceTheme,
    appearance: &crate::settings::AppearanceSettings,
    slot: &str,
    preview_fonts: &mut PreviewFonts,
) {
    let th = crate::theme::theme();
    ui.label(
        egui::RichText::new(t("settings.appearance.preview_heading"))
            .size(th.font_size_caption.value())
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    let slot_name = preview_slot(slot);
    let display_family = if eff.font_family.is_empty() {
        "monospace".to_string()
    } else {
        eff.font_family.clone()
    };

    let preview_family = preview_fonts.family_for(ui.ctx(), appearance, &slot_name, eff);

    let sample_lines = [
        "AaBbCcDdEeFfGg",
        "\u{AC00}\u{B098}\u{B2E4}\u{B77C}\u{B9C8}\u{BC14}\u{C0AC}", // 가나다라마바사
        "1234567890",
        "\u{30A2}\u{30AB}\u{30B5}\u{30BF}\u{30CA}\u{30CF}\u{30DE}\u{30E9}\u{30E4}\u{30EF}", // アカサタナハマラヤワ
    ];

    let focused_bg32 = colors.focused_bg.to_egui();
    let unfocused_bg32 = colors.unfocused_bg.to_egui();
    let fg32 = colors.focused_fg.to_egui();

    let font_size = eff.font_size.max(1.0);
    let preview_font = egui::FontId::new(font_size, preview_family);
    let line_height = font_size * th.line_height_ui;
    let pad_x = th.font_preview_padding_x().value();
    let pad_y = th.font_preview_padding_y().value();
    let block_height = line_height * sample_lines.len() as f32 + pad_y * 2.0;

    // 두 칸의 배경이 같은 패널 색과 겹쳐도 경계가 보이도록 1px 테두리를 두른다.
    // Focused 는 border-strong, Unfocused 는 separator(premultiplied 바이트)다.
    let blocks = [
        (
            "settings.appearance.preview_focused",
            focused_bg32,
            th.border_strong().to_egui(),
        ),
        (
            "settings.appearance.preview_unfocused",
            unfocused_bg32,
            th.separator.to_egui_premultiplied(),
        ),
    ];
    let radius = th.corner_radius.value();
    let block = |ui: &mut egui::Ui,
                 (label_key, bg, edge): (&str, egui::Color32, egui::Color32),
                 w: f32| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        ui.label(
            egui::RichText::new(t(label_key))
                .size(th.font_size_caption.value())
                .color(th.text_muted()),
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, block_height), egui::Sense::hover());
        ui.painter().rect_filled(rect, radius, bg);
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(th.border_width.value(), edge),
            egui::StrokeKind::Inside,
        );
        for (i, line) in sample_lines.iter().enumerate() {
            let pos = rect.min + egui::vec2(pad_x, pad_y + line_height * i as f32);
            ui.painter().text(
                pos,
                egui::Align2::LEFT_TOP,
                line,
                preview_font.clone(),
                fg32,
            );
        }
    };
    // Focused · Unfocused 를 나란히 두고 남은 폭을 반씩 나눈다. 한 칸이
    // `font-preview-min-width` 보다 좁아지면 Unfocused 가 아래로 내려간다.
    let gap = th.spacing_md.value();
    let half = (ui.available_width() - gap) / 2.0;
    if half < th.font_preview_min_width().value() {
        let w = ui.available_width();
        for (i, b) in blocks.into_iter().enumerate() {
            if i > 0 {
                vspace(ui, th.spacing_md);
            }
            ui.vertical(|ui| block(ui, b, w));
        }
    } else {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for b in blocks {
                ui.vertical(|ui| {
                    ui.set_width(half);
                    block(ui, b, half);
                });
            }
        });
    }

    vspace(ui, th.spacing_sm);
    ui.label(
        egui::RichText::new(crate::i18n::t_fmt(
            "settings.appearance.preview_font_info",
            &format!("{} / {:.1}px", display_family, font_size),
        ))
        .size(th.font_size_caption.value())
        .color(th.text_muted())
        .family(tasty_ui_widgets::label_font_family()),
    );
}

#[cfg(test)]
mod swatch_tests {
    use super::{HexColor, representative_swatch};
    use tasty_themes::ThemeFile;

    #[test]
    // 테스트 기대값 리터럴 구성 — 색을 "디자인"하는 게 아니라 파서 출력과 비교할
    // 고정 팔레트 값을 만든다 (clippy.toml disallowed-methods 예외 컨벤션).
    #[allow(clippy::disallowed_methods)]
    fn swatch_derives_from_theme_file_colors() {
        let text = r##"
            [palette]
            crust = "#11111b"
            base = "#1e1e2e"
            [accent]
            blue = "#89b4fa"
            mauve = "#cba6f7"
            green = "#a6e3a1"
        "##;
        let file = ThemeFile::parse(text).expect("parse");
        // [bg, surface, accent, accent_alt, success]
        assert_eq!(
            representative_swatch(&file),
            [
                HexColor::from_rgb(0x11, 0x11, 0x1b),
                HexColor::from_rgb(0x1e, 0x1e, 0x2e),
                HexColor::from_rgb(0x89, 0xb4, 0xfa),
                HexColor::from_rgb(0xcb, 0xa6, 0xf7),
                HexColor::from_rgb(0xa6, 0xe3, 0xa1),
            ]
        );
    }

    #[test]
    fn swatch_falls_back_to_mocha_base_when_unspecified() {
        // 색을 하나도 지정하지 않은 테마 → 5색 모두 mocha fallback.
        let file = ThemeFile::default();
        let base = tasty_themes::mocha_fallback_colors();
        assert_eq!(
            representative_swatch(&file),
            [base.crust, base.base, base.blue, base.mauve, base.green]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::find_plugin_settings_entry;
    use tasty_host_plugin::SettingsPageEntry;
    use tasty_plugin_manifest::{SettingsCategory, SettingsPageContribute};

    fn entry(plugin_id: &str, page_id: &str) -> SettingsPageEntry {
        SettingsPageEntry {
            plugin_id: plugin_id.into(),
            page: SettingsPageContribute {
                id: page_id.into(),
                title_key: format!("{plugin_id}.{page_id}.title"),
                category: SettingsCategory::Appearance,
                items: vec![],
            },
        }
    }

    /// 서로 다른 plugin 이 동일한 `page_id` ("theme") 를 contribute 해도
    /// `(plugin_id, page_id)` 복합키 매칭이 각각의 entry 를 정확히 가려낸다.
    #[test]
    fn same_page_id_from_different_plugins_resolves_correctly() {
        let entries = vec![entry("alpha", "theme"), entry("beta", "theme")];

        let found_alpha = find_plugin_settings_entry(&entries, "alpha", "theme")
            .expect("alpha/theme should be found");
        assert_eq!(found_alpha.plugin_id, "alpha");

        let found_beta = find_plugin_settings_entry(&entries, "beta", "theme")
            .expect("beta/theme should be found");
        assert_eq!(found_beta.plugin_id, "beta");

        // beta를 찾을 때 alpha가 반환되면 안 된다.
        assert_ne!(found_beta.plugin_id, found_alpha.plugin_id);
    }

    #[test]
    fn missing_plugin_returns_none() {
        let entries = vec![entry("alpha", "theme")];
        assert!(find_plugin_settings_entry(&entries, "gamma", "theme").is_none());
        assert!(find_plugin_settings_entry(&entries, "alpha", "other").is_none());
    }

    // ── Colors override picker — 상태 변환 로직 (egui 위젯 외) ──
    use super::{ColorGroupDef, color_groups, group_changed_count};
    use crate::settings::HexColor;
    use tasty_type_appearance::theme::PartialColors;

    fn find_row<'a>(groups: &'a [ColorGroupDef], name: &str) -> &'a super::ColorRowDef {
        groups
            .iter()
            .flat_map(|g| &g.rows)
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("row {name} should exist"))
    }

    /// 디자인 `PALETTE_GROUPS` 와 동일한 6그룹/47필드 + 기본 열림/접힘 분류.
    #[test]
    fn color_groups_match_design_layout() {
        let groups = color_groups();
        let counts: Vec<(usize, bool)> = groups
            .iter()
            .map(|g| (g.rows.len(), g.default_open))
            .collect();
        // Surfaces(6,열림) Overlays(3,접힘) Text(4,열림) Accents(13,열림)
        // Terminal-specific(5,접힘) ANSI 16(16,접힘).
        assert_eq!(
            counts,
            vec![
                (6, true),
                (3, false),
                (4, true),
                (13, true),
                (5, false),
                (16, false),
            ]
        );
        let total: usize = groups.iter().map(|g| g.rows.len()).sum();
        assert_eq!(total, 47, "flat PartialColors 47 fields");
    }

    /// "Default" 체크박스 토글 = 필드 Some(base)⇄None — set/get 라운드트립.
    #[test]
    fn toggle_sets_and_clears_override_field() {
        let groups = color_groups();
        let blue = find_row(&groups, "blue");
        let mut ov = PartialColors::default();
        assert!((blue.get)(&ov).is_none());

        let c = HexColor::from_hex("#112233").unwrap();
        (blue.set)(&mut ov, Some(c)); // 체크 해제 → Some
        assert_eq!((blue.get)(&ov), Some(c));

        (blue.set)(&mut ov, None); // 다시 체크 → None
        assert!((blue.get)(&ov).is_none());
    }

    /// base 접근자는 resolved `theme_base` 의 해당 필드를 읽는다 (하드코딩 아님).
    #[test]
    fn base_reads_from_theme_base() {
        let mut base = tasty_themes::mocha_fallback_colors();
        let marker = HexColor::from_hex("#abcdef").unwrap();
        base.crust = marker;
        let groups = color_groups();
        let crust = find_row(&groups, "crust");
        assert_eq!((crust.base)(&base), marker);
    }

    /// 그룹 Reset 은 그 그룹 필드만 클리어하고 다른 그룹 override 는 보존.
    #[test]
    fn group_reset_clears_only_its_fields() {
        let groups = color_groups();
        let blue = find_row(&groups, "blue"); // Accents
        let crust = find_row(&groups, "crust"); // Surfaces
        let c = HexColor::from_hex("#010203").unwrap();

        let mut ov = PartialColors::default();
        (blue.set)(&mut ov, Some(c));
        (crust.set)(&mut ov, Some(c));

        // Accents 그룹만 reset.
        let accents = groups
            .iter()
            .find(|g| g.name_key.ends_with(".accents"))
            .unwrap();
        for r in &accents.rows {
            (r.set)(&mut ov, None);
        }
        assert!((blue.get)(&ov).is_none(), "blue cleared");
        assert_eq!((crust.get)(&ov), Some(c), "crust preserved");
        assert_eq!(group_changed_count(accents, &ov), 0);
    }

    // ── Tasty curated section (accent/sidebar bg → theme_overrides + indicator) ──
    use super::reset_tasty_to_theme_defaults;
    use crate::settings::{ActiveTabIndicator, AppearanceSettings};

    /// Tasty › Accent/Sidebar bg 는 `theme_overrides.blue`/`.mantle` 에 set/clear.
    #[test]
    fn tasty_accent_sidebar_map_to_blue_and_mantle() {
        let mut app = AppearanceSettings::default();
        let c = HexColor::from_hex("#445566").unwrap();
        assert!(app.theme_overrides.blue.is_none());
        assert!(app.theme_overrides.mantle.is_none());

        app.theme_overrides.blue = Some(c); // Accent
        app.theme_overrides.mantle = Some(c); // Sidebar bg
        assert_eq!(app.theme_overrides.blue, Some(c));
        assert_eq!(app.theme_overrides.mantle, Some(c));
    }

    /// "Use theme defaults" = accent/sidebar override clear + indicator 기본값 복귀.
    #[test]
    fn tasty_use_defaults_clears_overrides_and_resets_indicator() {
        let mut app = AppearanceSettings::default();
        let c = HexColor::from_hex("#778899").unwrap();
        app.theme_overrides.blue = Some(c);
        app.theme_overrides.mantle = Some(c);
        app.active_tab_indicator = ActiveTabIndicator::Dot;
        // 무관 override 는 보존되어야 한다 (curated 두 필드만 건드림).
        app.theme_overrides.red = Some(c);

        reset_tasty_to_theme_defaults(&mut app);

        assert!(app.theme_overrides.blue.is_none(), "accent cleared");
        assert!(app.theme_overrides.mantle.is_none(), "sidebar bg cleared");
        assert_eq!(
            app.active_tab_indicator,
            ActiveTabIndicator::default(),
            "indicator reset to default"
        );
        assert_eq!(
            app.theme_overrides.red,
            Some(c),
            "unrelated override preserved"
        );
    }

    /// Reset all 은 47 flat 필드만 클리어하고 surface_themes(비-picker) 는 보존.
    #[test]
    fn reset_all_preserves_surface_themes() {
        let groups = color_groups();
        let mut ov = PartialColors::default();
        let c = HexColor::from_hex("#0a0b0c").unwrap();
        find_row(&groups, "blue");
        (find_row(&groups, "blue").set)(&mut ov, Some(c));
        ov.surface_themes.insert(
            "terminal".to_string(),
            tasty_type_appearance::theme::PartialSurfaceTheme {
                focused_bg: Some(c),
                ..Default::default()
            },
        );

        // Reset all = picker 가 다루는 flat 필드만 None.
        for r in groups.iter().flat_map(|g| &g.rows) {
            (r.set)(&mut ov, None);
        }
        let flat_changed: usize = groups.iter().map(|g| group_changed_count(g, &ov)).sum();
        assert_eq!(flat_changed, 0, "all 47 flat overrides cleared");
        assert!(
            ov.surface_themes.contains_key("terminal"),
            "surface_themes override preserved"
        );
    }

    // ── Terminal surface bg picker (focused/unfocused background) ──
    use super::{SurfaceBgField, set_surface_bg_override, surface_bg_base, surface_bg_override};

    /// override set → resolve = override 색, clear(None) → entry pruned + base 추종.
    #[test]
    fn surface_bg_override_set_clear_roundtrip() {
        let mut ov = PartialColors::default();
        assert!(surface_bg_override(&ov, SurfaceBgField::Focused).is_none());

        let c = HexColor::from_hex("#123456").unwrap();
        set_surface_bg_override(&mut ov, SurfaceBgField::Focused, Some(c));
        assert_eq!(surface_bg_override(&ov, SurfaceBgField::Focused), Some(c));
        // 다른 필드는 독립.
        assert!(surface_bg_override(&ov, SurfaceBgField::Unfocused).is_none());

        // clear → None + 빈 entry 제거.
        set_surface_bg_override(&mut ov, SurfaceBgField::Focused, None);
        assert!(surface_bg_override(&ov, SurfaceBgField::Focused).is_none());
        assert!(
            !ov.surface_themes.contains_key("terminal"),
            "empty surface override entry pruned"
        );
    }

    /// 한 필드 clear 시 같은 entry 의 다른 필드 override 는 보존(entry 유지).
    #[test]
    fn surface_bg_clear_preserves_sibling_field() {
        let mut ov = PartialColors::default();
        let c = HexColor::from_hex("#111111").unwrap();
        set_surface_bg_override(&mut ov, SurfaceBgField::Focused, Some(c));
        set_surface_bg_override(&mut ov, SurfaceBgField::Unfocused, Some(c));

        set_surface_bg_override(&mut ov, SurfaceBgField::Focused, None);
        assert!(surface_bg_override(&ov, SurfaceBgField::Focused).is_none());
        assert_eq!(surface_bg_override(&ov, SurfaceBgField::Unfocused), Some(c));
        assert!(
            ov.surface_themes.contains_key("terminal"),
            "entry kept while sibling override remains"
        );
    }

    /// base 색은 resolved `theme_base.surface_themes["terminal"]` 에서 읽는다.
    #[test]
    fn surface_bg_base_reads_theme_base() {
        let mut base = tasty_themes::mocha_fallback_colors();
        let marker = HexColor::from_hex("#abcdef").unwrap();
        base.surface_themes
            .get_mut("terminal")
            .expect("mocha base has terminal surface")
            .focused_bg = marker;
        assert_eq!(surface_bg_base(&base, SurfaceBgField::Focused), marker);
    }
}

#[cfg(test)]
mod default_hex_field_tests {
    use super::{Theme, color_row_line, default_hex_field};

    /// `hex`를 담은 칸으로 한 프레임을 그리고 응답을 돌려준다.
    fn frame(
        ctx: &egui::Context,
        th: &Theme,
        hex: &mut String,
        events: Vec<egui::Event>,
    ) -> egui::Response {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let mut seen = None;
        drop(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                color_row_line(ui, th, |ui| {
                    seen = Some(default_hex_field(ui, th, hex));
                });
            });
        }));
        seen.expect("frame ran")
    }

    /// Default 행의 hex 칸은 disabled가 아니라 읽기 전용이다. 클릭하면 포커스를 받고,
    /// 포커스 상태에서 글자를 입력하거나 지워도 값은 그대로다.
    #[test]
    fn default_hex_field_is_read_only_and_takes_focus() {
        let th = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut hex = String::from("#89b4fa");
        let first = frame(&ctx, &th, &mut hex, Vec::new());
        let pos = first.rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            &th,
            &mut hex,
            vec![egui::Event::PointerMoved(pos), press(true)],
        );
        frame(&ctx, &th, &mut hex, vec![press(false)]);
        let after = frame(&ctx, &th, &mut hex, Vec::new());
        assert!(after.has_focus(), "Default hex field did not take focus");

        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let typed = frame(
            &ctx,
            &th,
            &mut hex,
            vec![
                egui::Event::Text("x".to_owned()),
                key(egui::Key::Backspace),
                key(egui::Key::Backspace),
            ],
        );
        assert!(typed.has_focus(), "typing moved focus away");
        assert_eq!(hex, "#89b4fa", "Default hex field accepted an edit");
    }
}

#[cfg(test)]
mod default_font_section_tests {
    use super::PreviewFonts;

    use super::{default_font_section, font_override_grid, font_setting_rows};
    use crate::i18n::t;
    use crate::settings::{FontOverride, FontSettings, Settings};
    use tasty_type_geometry::length::LogicalPx;

    /// 한 프레임에서 읽은 것 — 글꼴 콤보·커스텀 글꼴 입력칸 응답, 미리보기 위쪽 경계, 그린 글자.
    struct Seen {
        family: egui::Response,
        custom: egui::Response,
        preview_top: f32,
        texts: Vec<String>,
    }

    impl Seen {
        fn has(&self, text: &str) -> bool {
            self.texts.iter().any(|t| t == text)
        }
    }

    fn texts(out: &egui::FullOutput) -> Vec<String> {
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    /// 설정 창 콘텐츠 컬럼 상한(620)과 같은 폭에 기본 글꼴 격자와 미리보기를 한 프레임 그린다.
    fn frame_with(
        ctx: &egui::Context,
        col: LogicalPx,
        settings: &mut Settings,
        families: &mut Option<Vec<String>>,
        events: Vec<egui::Event>,
    ) -> Seen {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(620.0, 500.0));
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            focused: true,
            events,
            ..Default::default()
        };
        let mut loaded = PreviewFonts::default();
        let mut seen = None;
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    seen = Some(default_font_section(
                        ui,
                        font_setting_rows(),
                        col,
                        settings,
                        families,
                        &mut loaded,
                    ));
                });
        });
        let (family, custom, preview_top) = seen.expect("frame ran");
        Seen {
            family,
            custom,
            preview_top,
            texts: texts(&out),
        }
    }

    fn frame(
        ctx: &egui::Context,
        col: LogicalPx,
        settings: &mut Settings,
        events: Vec<egui::Event>,
    ) -> Seen {
        frame_with(ctx, col, settings, &mut None, events)
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_with(
        ctx: &egui::Context,
        col: LogicalPx,
        settings: &mut Settings,
        families: &mut Option<Vec<String>>,
        pos: egui::Pos2,
    ) -> Seen {
        frame_with(
            ctx,
            col,
            settings,
            families,
            vec![egui::Event::PointerMoved(pos), press(pos, true)],
        );
        frame_with(ctx, col, settings, families, vec![press(pos, false)]);
        frame_with(ctx, col, settings, families, Vec::new())
    }

    fn click(ctx: &egui::Context, col: LogicalPx, settings: &mut Settings, pos: egui::Pos2) {
        click_with(ctx, col, settings, &mut None, pos);
    }

    fn key(k: egui::Key) -> Vec<egui::Event> {
        vec![
            egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn families() -> Option<Vec<String>> {
        Some(
            ["Fira Code", "JetBrains Mono", "Menlo"]
                .map(str::to_owned)
                .to_vec(),
        )
    }

    /// 라벨 열이 하한(150)이든 상한(240)이든 미리보기는 설정 행 아래에 있다. 글꼴 콤보와 커스텀
    /// 글꼴 입력칸은 미리보기 위에서 끝나고, 콤보 응답은 실제 트리거 상자 — `field-width-lg` 폭,
    /// 다른 Select 와 같은 `select-height` 높이 — 다.
    #[test]
    fn the_preview_sits_below_the_font_rows() {
        let th = crate::theme::theme();
        for col in [th.settings_label_width(), th.settings_label_max_width()] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            frame(&ctx, col, &mut settings, Vec::new());
            let Seen {
                family,
                custom,
                preview_top,
                ..
            } = frame(&ctx, col, &mut settings, Vec::new());
            assert!(
                family.rect.bottom() <= preview_top,
                "label column {}: font combo ends at {} below the preview top {}",
                col.value(),
                family.rect.bottom(),
                preview_top
            );
            assert!(
                custom.rect.bottom() <= preview_top,
                "label column {}: custom font field ends at {} below the preview top {}",
                col.value(),
                custom.rect.bottom(),
                preview_top
            );
            assert!(
                (family.rect.width() - th.field_width_lg.value()).abs() < 0.5,
                "label column {}: font combo is {} wide, not field-width-lg",
                col.value(),
                family.rect.width()
            );
            assert_eq!(
                family.rect.height(),
                th.select_height().value(),
                "label column {}: font combo is not as tall as a Select",
                col.value()
            );
        }
    }

    /// 콤보 오른쪽 끝(chevron 자리)을 누르면 목록이 열린다. 미리보기 제목이 그 자리를 덮으면
    /// 클릭이 제목으로 가서 열리지 않는다.
    #[test]
    fn clicking_the_combo_arrow_opens_the_font_list() {
        let th = crate::theme::theme();
        for col in [th.settings_label_width(), th.settings_label_max_width()] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            frame(&ctx, col, &mut settings, Vec::new());
            let family = frame(&ctx, col, &mut settings, Vec::new()).family;
            let arrow = family.rect.right_center() - egui::vec2(8.0, 0.0);
            click(&ctx, col, &mut settings, arrow);
            assert!(
                ctx.memory(|m| m.any_popup_open()),
                "label column {}: clicking the combo arrow at {arrow:?} did not open the list",
                col.value()
            );
        }
    }

    /// 저장된 이름이 목록에 없을 때 — 목록을 받는 중이거나 설치되지 않은 글꼴 — 트리거는 그 이름을
    /// 그대로 보이고, 목록을 열고 닫아도 값이 바뀌지 않는다.
    #[test]
    fn a_saved_family_missing_from_the_list_is_kept() {
        let th = crate::theme::theme();
        let col = th.settings_label_width();
        for (case, mut list) in [("loading", None), ("not installed", families())] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            settings.appearance.default_font.font_family = "Not Installed Mono".to_owned();
            frame_with(&ctx, col, &mut settings, &mut list, Vec::new());
            let seen = frame_with(&ctx, col, &mut settings, &mut list, Vec::new());
            assert!(
                seen.has("Not Installed Mono"),
                "{case}: trigger lost the saved name"
            );
            let opened = click_with(
                &ctx,
                col,
                &mut settings,
                &mut list,
                seen.family.rect.center(),
            );
            assert!(
                ctx.memory(|m| m.any_popup_open()),
                "{case}: the list did not open"
            );
            if list.is_none() {
                assert!(
                    opened.has(t("settings.appearance.font_loading")),
                    "{case}: no loading row"
                );
            }
            let closed = frame_with(&ctx, col, &mut settings, &mut list, key(egui::Key::Escape));
            assert!(
                !ctx.memory(|m| m.any_popup_open()),
                "{case}: Escape did not close"
            );
            assert!(
                closed.has("Not Installed Mono"),
                "{case}: trigger lost the saved name"
            );
            assert_eq!(
                settings.appearance.default_font.font_family, "Not Installed Mono",
                "{case}: the saved name changed"
            );
        }
    }

    /// 검색으로 현재 값이 목록에서 숨어도 트리거는 그 값을 보이고 값은 그대로다. 기본값(monospace)
    /// 행은 일치가 없을 때도 맨 위에 남고, 누르면 값을 비운다.
    #[test]
    fn a_filtered_out_value_stays_and_the_default_row_is_pinned() {
        let th = crate::theme::theme();
        let col = th.settings_label_width();
        let ctx = egui::Context::default();
        let mut list = families();
        let mut settings = Settings::default();
        settings.appearance.default_font.font_family = "Menlo".to_owned();
        frame_with(&ctx, col, &mut settings, &mut list, Vec::new());
        let seen = frame_with(&ctx, col, &mut settings, &mut list, Vec::new());
        click_with(
            &ctx,
            col,
            &mut settings,
            &mut list,
            seen.family.rect.center(),
        );
        let typed = frame_with(
            &ctx,
            col,
            &mut settings,
            &mut list,
            vec![egui::Event::Text("zzz".into())],
        );
        let typed = if typed.has(t("settings.appearance.font_no_match")) {
            typed
        } else {
            frame_with(&ctx, col, &mut settings, &mut list, Vec::new())
        };
        assert!(
            typed.has(t("settings.appearance.font_no_match")),
            "no empty row"
        );
        assert!(
            typed.has(t("settings.appearance.font_family_default")),
            "default row hidden"
        );
        assert!(
            !typed.has("Fira Code"),
            "the filter did not hide other families"
        );
        assert!(typed.has("Menlo"), "trigger lost the filtered-out value");
        assert_eq!(settings.appearance.default_font.font_family, "Menlo");

        // 일치가 없으면 활성 행은 고정 행이다. Enter 로 골라 기본값으로 돌아간다.
        frame_with(&ctx, col, &mut settings, &mut list, key(egui::Key::Enter));
        assert_eq!(
            settings.appearance.default_font.font_family, "",
            "choosing the pinned default row did not clear the family"
        );
    }

    /// 기본값을 따르는 글꼴 행은 비활성 콤보가 아니라 적용 값을 담은 읽기 전용 상자다.
    #[test]
    fn a_default_override_family_shows_the_value_in_a_read_only_box() {
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut ov = FontOverride::default();
        let default = FontSettings::default();
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(620.0, 500.0),
            )),
            ..Default::default()
        };
        let mut run = || {
            ctx.run(raw(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let fonts = PreviewFonts::default();
                        let judge = super::RowJudge::Terminal { fonts: &fonts };
                        font_override_grid(ui, &mut ov, &default, &mut None, "t", judge);
                    });
            })
        };
        drop(run());
        let out = run();
        let label = t("settings.appearance.font_family_default");
        let ink = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t.fallback_color),
                _ => None,
            })
            .expect("the read-only box shows the default family");
        assert_eq!(ink, th.input_readonly_fg().to_egui());
        let fill = th.input_readonly_bg().to_egui();
        assert!(
            out.shapes.iter().any(|s| matches!(
                &s.shape,
                egui::Shape::Rect(r) if r.fill == fill
                    && r.rect.width() == th.field_width_lg.value()
            )),
            "no read-only box for the family row"
        );
    }
}

#[cfg(test)]
#[path = "font_rows_tests.rs"]
mod font_rows_tests;
