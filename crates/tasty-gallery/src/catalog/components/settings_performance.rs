//! 성능 탭 예제. 실제 설정 저장은 하지 않는다.
//!
//! 탭 맨 위에 적용 시점 안내(info 글리프 + caption)를 한 번 두고, 본체와 같은 settings-row 그리드에
//! 스위치 두 행과 숫자 행 하나를 둔다. 디스크 보관 줄 수는 디스크 스왑이 꺼져 있으면 비활성이고,
//! -1 이 무제한이라 범위 하한이며 그 값이면 단위 자리에 "no limit" 을 적는다. 숫자 칸은 값 전체를 담도록
//! 자라고 단위는 칸 뒤에 둔다(공용 `number_field_width`·`number_field_unit`·`number_field_note`).

use std::cell::RefCell;

use tasty_settings::{DEFAULT_SCROLLBACK_DISK_LINES, UNLIMITED_SCROLLBACK_DISK_LINES};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Input, SettingsRow, number_field_note, number_field_unit, number_field_width,
    settings_apply_note, settings_label_column,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::t_lang;

use super::settings_remote_transfer::WIDTH;

/// 본체 `tabs/performance.rs` 의 상한과 같은 값이다.
const DISK_LINES_MAX: i64 = 100_000_000;
/// 디자인 견본의 좁은 폭(size-360). 안내가 줄을 바꾸고 행이 라벨 아래로 쌓인다.
const NARROW: LogicalPx = LogicalPx(360.0);

struct Case {
    label: &'static str,
    lang: &'static str,
    latte: bool,
    width: LogicalPx,
    polling: bool,
    disk_swap: bool,
    disk_lines: String,
    /// 저장된 값. 칸의 글자가 이와 다를 때만 범위 밖 경고를 보인다(읽은 값은 그대로 둔다).
    stored: String,
}

fn case(label: &'static str, latte: bool, disk_swap: bool, lines: impl ToString) -> Case {
    Case {
        label,
        lang: "en",
        latte,
        width: WIDTH,
        polling: true,
        disk_swap,
        disk_lines: lines.to_string(),
        stored: lines.to_string(),
    }
}

thread_local! {
    static STATE: RefCell<Vec<Case>> = RefCell::new(vec![
        case("Mocha · default · disk swap on", false, true, DEFAULT_SCROLLBACK_DISK_LINES),
        case("Latte · default · disk swap on", true, true, DEFAULT_SCROLLBACK_DISK_LINES),
        case("Mocha · disk swap off · lines disabled", false, false, DEFAULT_SCROLLBACK_DISK_LINES),
        case("Latte · -1 · no limit", true, true, UNLIMITED_SCROLLBACK_DISK_LINES),
        case("Mocha · larger config value, shown as read", false, true, 2_500_000),
        Case {
            width: NARROW,
            ..case("Latte · narrow 360 · disk swap off · -1", true, false, UNLIMITED_SCROLLBACK_DISK_LINES)
        },
    ]);
}

thread_local! {
    /// 시안 `PfLinesSpec` 견본. 10자리·19자리는 설정 파일에서 읽은 값이고 250000000 은 확정 전 입력이다.
    static FIT: RefCell<Vec<Case>> = RefCell::new({
        let mut cases = Vec::new();
        for latte in [false, true] {
            let pair: [(&'static str, bool, &str); 6] = [
                ("10000 (default) — floor 90", true, "10000"),
                ("100000000 — max, fits the floor", true, "100000000"),
                ("4000000000 — 10-digit config value, shown as read", true, "4000000000"),
                ("-1 — “no limit” replaces the unit", true, "-1"),
                ("swap off — disabled, whole value", false, "4000000000"),
                ("typed 250000000 — over range, before commit", true, "250000000"),
            ];
            for (label, swap, lines) in pair {
                cases.push(case(label, latte, swap, lines));
            }
            // 마지막 견본은 저장 값 10000 에서 250000000 을 친 상태다.
            if let Some(typed) = cases.last_mut() {
                typed.stored = DEFAULT_SCROLLBACK_DISK_LINES.to_string();
            }
        }
        let localized = |label, lang, width, swap, lines: &str| Case {
            label,
            lang,
            width,
            ..case(label, false, swap, lines)
        };
        cases.push(localized("ko · 100000000", "ko", WIDTH, true, "100000000"));
        cases.push(localized(
            "ja · 9223372036854775807 — the longest config value",
            "ja",
            WIDTH,
            true,
            "9223372036854775807",
        ));
        cases.push(localized("ko · narrow 360 · 4000000000 — row stacks, same field", "ko", NARROW, true, "4000000000"));
        cases.push(Case {
            stored: DEFAULT_SCROLLBACK_DISK_LINES.to_string(),
            ..localized("ja · narrow 360 · over range", "ja", NARROW, true, "250000000")
        });
        cases.push(localized("en · narrow 360 · swap off · -1", "en", NARROW, false, "-1"));
        cases
    });
}

/// 디스크 보관 줄 수 칸이 값에 맞춰 자라고 단위를 칸 뒤에 두는 견본(시안 `PfLinesSpec`).
pub fn draw_number_field(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        FIT.with(|s| {
            for (i, case) in s.borrow_mut().iter_mut().enumerate() {
                let th = if case.latte { &latte } else { &mocha };
                spec::wrap_item(ui, |ui| {
                    ui.push_id(("fit", i), |ui| frame(ui, th, case));
                });
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "field",
                "mono Input, no addon · width = max(number-field-min-width, (chars + 1) ch + 2 × input-padding-x + 2 × border) · capped at the control column",
            ),
            (
                "chars",
                "typed text while editing, the stored value otherwise (sign included)",
            ),
            (
                "unit",
                "after the field, gap number-field-unit-gap · font-label caption number-field-unit-fg · disabled → state-disabled-fg",
            ),
            ("-1", "“no limit” (UI font, caption) in the unit's place"),
            (
                "rest / blur",
                "whole value, left-aligned · editing grows per typed char, no inner scroll",
            ),
            (
                "out of range",
                "invalid edge + range line under the field, caption number-field-note-fg, measure-md · commit clamps",
            ),
            (
                "narrow",
                "row wraps under the label; the field keeps its width",
            ),
            (
                "shared",
                "every settings integer field grows to its value · File Handlers priority keeps 90",
            ),
        ],
        &[
            TokenChip::without_color("number-field-min-width", "→ field-width-xs 90"),
            TokenChip::without_color("number-field-unit-gap", "→ space-sm 8"),
            TokenChip::new(
                "number-field-unit-fg",
                "→ text-secondary",
                theme.number_field_unit_fg().to_egui(),
            ),
            TokenChip::new(
                "number-field-note-fg",
                "→ text-secondary",
                theme.number_field_note_fg().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "invalid edge",
                theme.accent_danger().to_egui(),
            ),
        ],
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let mocha = with_zoom(tasty_themes::mocha_fallback());
    let latte = with_zoom(crate::host_shell::latte_theme());
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        STATE.with(|s| {
            for (i, case) in s.borrow_mut().iter_mut().enumerate() {
                let th = if case.latte { &latte } else { &mocha };
                spec::wrap_item(ui, |ui| {
                    ui.push_id(i, |ui| frame(ui, th, case));
                });
            }
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "note",
                "top of the tab, above the rows · info glyph icon-size-sm · caption · wraps at measure-md",
            ),
            (
                "rows",
                "polling switch · disk swap switch · lines kept on disk",
            ),
            (
                "field",
                "mono Input that fits its value · unit after the field (see the next specimen)",
            ),
            (
                "-1",
                "no limit in the unit's place (state-disabled while swap is off)",
            ),
            ("range", "-1 (unlimited) … 100000000 · clamp on commit"),
            ("default value", "10000"),
            ("enabled when", "disk swap is on"),
        ],
        &[
            TokenChip::new(
                "settings-apply-note-fg",
                "→ text-secondary",
                theme.settings_apply_note_fg().to_egui(),
            ),
            TokenChip::new(
                "settings-apply-note-glyph",
                "→ text-muted",
                theme.settings_apply_note_glyph().to_egui(),
            ),
            TokenChip::without_color("settings-apply-note-gap", "→ space-sm 8"),
            TokenChip::without_color("settings-row-gap", "→ space-md 12"),
            TokenChip::without_color("number-field-min-width", "→ field-width-xs 90"),
        ],
    );
}

fn frame(ui: &mut egui::Ui, th: &Theme, case: &mut Case) {
    let lang = case.lang;
    let t = |key| t_lang(lang, key);
    kit::frame_card_flat(ui, th, case.width, kit::panel_fill(th), |ui| {
        kit::region_sym(ui, th.spacing_lg, th.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();
            ui.label(
                egui::RichText::new(case.label)
                    .size(th.font_size_caption.value())
                    .color(th.text_muted().to_egui()),
            );
            settings_apply_note(ui, th, t("settings.performance.apply_notice"));
            let rows = [
                SettingsRow::new(t("settings.performance.targeted_pty_polling"))
                    .hint(t("settings.performance.targeted_pty_polling_desc")),
                SettingsRow::new(t("settings.performance.scrollback_disk_swap"))
                    .hint(t("settings.performance.scrollback_disk_swap_desc")),
                SettingsRow::new(t("settings.performance.scrollback_disk_lines"))
                    .hint(t("settings.performance.scrollback_disk_lines_desc")),
            ];
            let col = settings_label_column(ui, th, &rows);
            let [polling, disk_swap, disk_lines] = rows;
            polling.show(ui, th, col, |ui| {
                tasty_ui_widgets::switch(ui, th, &mut case.polling, None, true);
            });
            disk_swap.show(ui, th, col, |ui| {
                tasty_ui_widgets::switch(ui, th, &mut case.disk_swap, None, true);
            });
            let enabled = case.disk_swap;
            disk_lines.show(ui, th, col, |ui| {
                lines_control(ui, th, lang, &mut case.disk_lines, &case.stored, enabled)
            });
        });
    });
}

/// 본체 `number_field` 의 디스크 보관 줄 수 칸과 같은 모양. 칸은 값에 맞춰 자라고 단위(또는 -1 의
/// "no limit")는 칸 뒤, 친 값이 범위 밖이면 칸 아래에 범위 문장을 둔다.
fn lines_control(
    ui: &mut egui::Ui,
    theme: &Theme,
    lang: &str,
    buf: &mut String,
    stored: &str,
    enabled: bool,
) {
    let t = |key| t_lang(lang, key);
    let (min, max) = (
        i128::from(UNLIMITED_SCROLLBACK_DISK_LINES),
        i128::from(DISK_LINES_MAX),
    );
    let parsed = buf.trim().parse::<i128>().ok();
    // 본체처럼 저장 값 그대로인 칸은 범위 밖이어도 경고하지 않는다.
    let over = buf != stored && parsed.is_some_and(|v| v < min || v > max);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.horizontal_wrapped(|ui| {
            let width = number_field_width(ui, theme, buf);
            Input::new()
                .mono(true)
                .align(egui::Align::LEFT)
                .width(width.value())
                .enabled(enabled)
                .invalid(over)
                .show(ui, theme, buf);
            if parsed == Some(min) {
                number_field_unit(
                    ui,
                    theme,
                    t("settings.performance.no_limit"),
                    false,
                    enabled,
                );
            } else {
                number_field_unit(ui, theme, t("settings.number.unit_lines"), true, enabled);
            }
        });
        if over {
            number_field_note(ui, theme, t("settings.performance.disk_lines_range"));
        }
    });
}
