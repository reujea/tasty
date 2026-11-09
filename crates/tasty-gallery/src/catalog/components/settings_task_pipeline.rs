//! 기타 › 작업 파이프라인 설정의 예제. 실제 설정 저장은 하지 않는다.
//!
//! 새 시각 값은 없다. 원격 전송 탭과 같은 settings-row 숫자 행 두 개이며, 단위만 MiB 대신 B 다.
//! 시안처럼 기본 짝(Mocha)과 범위 밖 짝(Latte)을 나란히 둔다. 범위 줄은 값이 범위 밖일 때만 보인다.

use std::cell::RefCell;

use tasty_settings::{
    DEFAULT_REPORT_APPEND_BYTES, DEFAULT_REPORT_BLOCK_BYTES, REPORT_APPEND_BYTES_RANGE,
    REPORT_BLOCK_BYTES_RANGE,
};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Input, SettingsRow, settings_label_column};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

// 프레임 폭은 같은 settings-row 를 쓰는 Remote transfer 예제의 값이다.
use super::settings_remote_transfer::WIDTH;

thread_local! {
    // 짝 둘(Mocha 기본 · Latte 범위 밖) × 행 둘의 입력 버퍼.
    static STATE: RefCell<[[String; 2]; 2]> = RefCell::new([
        [String::from("1024"), String::from("16384")],
        [String::from("70000"), String::from("16384")],
    ]);
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        (
            "Latte · out of range",
            with_zoom(crate::host_shell::latte_theme()),
        ),
    ];
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        STATE.with(|s| {
            let state = &mut *s.borrow_mut();
            for (pair, ((label, th), bufs)) in themes.iter().zip(state.iter_mut()).enumerate() {
                spec::wrap_item(ui, |ui| {
                    ui.push_id(pair, |ui| frame(ui, th, label, bufs));
                });
            }
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "L2 position",
                "Misc · after Scripts · before Tastyrc (Windows)",
            ),
            ("heading", "REPORT LIMITS · mono micro uppercase · muted"),
            ("row grid", "label 150 … 240 · gap 16 (settings-label-gap)"),
            (
                "field",
                "mono Input that fits its value (number-field-min-width 90 floor) · right-aligned · unit B (static, muted, caption)",
            ),
            (
                "caption",
                "under its row · gap 4 · caption 11 · muted · measure-md",
            ),
            ("between rows", "1px separator · settings-row-gap 12"),
            (
                "ranges",
                "note 64 … min(65536, attempt − 1) · attempt max(128, note + 1) … 131072",
            ),
            (
                "range line",
                "only when out of range (danger) · clamp on commit",
            ),
            ("numbers", "raw bytes, no grouping, no KiB"),
        ],
        &[
            TokenChip::without_color("settings-row-gap", "→ space-md 12"),
            TokenChip::without_color("settings-row-caption-gap", "row → caption 4"),
            TokenChip::without_color("number-field-min-width", "→ field-width-xs 90"),
            TokenChip::new(
                "accent-danger",
                "out of range",
                theme.accent_danger().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Two numeric rows for the task report limits. Each input's range follows the other's \
         current value so the note limit always stays below the attempt limit.",
    );
}

/// 한 테마의 설정 본문 — 헤딩, 두 행, 행 사이 구분선. 두 값의 범위는 서로의 현재 값으로 좁힌다.
fn frame(ui: &mut egui::Ui, th: &Theme, label: &str, bufs: &mut [String; 2]) {
    kit::frame_card_flat(ui, th, WIDTH, kit::panel_fill(th), |ui| {
        kit::region_sym(ui, th.spacing_lg, th.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();
            ui.label(
                egui::RichText::new(label)
                    .size(th.font_size_caption.value())
                    .color(th.text_muted().to_egui()),
            );
            mono_head(ui, th, crate::i18n::t("settings.task_pipeline.section"));
            let rows = [
                SettingsRow::new(crate::i18n::t("settings.task_pipeline.report_append"))
                    .caption(crate::i18n::t("settings.task_pipeline.report_append_desc")),
                SettingsRow::new(crate::i18n::t("settings.task_pipeline.report_block"))
                    .caption(crate::i18n::t("settings.task_pipeline.report_block_desc")),
            ];
            let col = settings_label_column(ui, th, &rows);
            let [note, attempt] = rows;
            let [append, block] = bufs;
            // 범위는 본체처럼 확정된 값으로 좁힌다. 범위 밖 짝의 70000 은 아직 확정되지 않은 입력이다.
            let append_now = DEFAULT_REPORT_APPEND_BYTES;
            let block_now = DEFAULT_REPORT_BLOCK_BYTES;
            let note_range = (
                REPORT_APPEND_BYTES_RANGE.0,
                REPORT_APPEND_BYTES_RANGE.1.min(block_now.saturating_sub(1)),
            );
            let attempt_range = (
                REPORT_BLOCK_BYTES_RANGE.0.max(append_now.saturating_add(1)),
                REPORT_BLOCK_BYTES_RANGE.1,
            );
            note.show(ui, th, col, |ui| bytes_control(ui, th, append, note_range));
            separator_line(ui, th);
            attempt.show(ui, th, col, |ui| {
                bytes_control(ui, th, block, attempt_range)
            });
        });
    });
}

fn mono_head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 숫자 Input + 단위 B, 범위 밖이면 그 아래 danger 한 줄(본체 `number_field` 와 같은 자리).
fn bytes_control(ui: &mut egui::Ui, theme: &Theme, buf: &mut String, (min, max): (u64, u64)) {
    let settled = buf
        .trim()
        .parse::<u64>()
        .ok()
        .map(|v| (v, v.clamp(min, max)))
        .and_then(|(v, c)| (v != c).then_some(c));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            Input::new()
                .mono(true)
                .align(egui::Align::RIGHT)
                .width(tasty_ui_widgets::number_field_width(ui, theme, buf).value())
                .invalid(settled.is_some())
                .show(ui, theme, buf);
            ui.label(
                egui::RichText::new("B")
                    .family(tasty_ui_widgets::label_font_family())
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
        if let Some(settled) = settled {
            let line = crate::i18n::t("settings.number.range_between")
                .replacen("{}", &min.to_string(), 1)
                .replacen("{}", &max.to_string(), 1)
                .replacen("{}", &settled.to_string(), 1);
            ui.label(
                egui::RichText::new(line)
                    .size(theme.font_size_caption.value())
                    .color(theme.accent_danger_raised().to_egui()),
            );
        }
    });
}

pub(super) fn separator_line(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
