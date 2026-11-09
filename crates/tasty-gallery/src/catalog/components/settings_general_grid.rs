//! General › General 의 행 격자 예제. 라벨 열은 가장 긴 라벨을 150 … 240 으로 clamp 한 폭이고,
//! 탐색기 파일 정보·휠 거리·언어·Properties 시각 형식 행은 자기 행 아래 caption 을, 웹훅 외부 수신 행은
//! 경고 callout 을 가진다. 시각 형식 caption 은 고른 형식의 예시를 mono 로 붙인다.
//! 실제 설정 저장은 하지 않는다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Input, SettingsRow, select, settings_label_column, switch};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 디자인 specimen 의 행 묶음 폭.
const WIDTH: LogicalPx = LogicalPx(640.0);

/// 본체 settings.general.* 의 영어 문구.
fn wheel_caption() -> &'static str {
    crate::i18n::t("settings.general.wheel_line_scroll_desc")
}
fn explorer_details_caption() -> &'static str {
    crate::i18n::t("settings.general.explorer_refresh_file_details_desc")
}
fn language_caption() -> &'static str {
    crate::i18n::t("settings.general.language_restart_notice")
}
fn notice() -> &'static str {
    crate::i18n::t("settings.general.webhook_allow_external_notice")
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        restore: true,
        explorer_details: false,
        close: 0,
        wheel: "50".to_owned(),
        language: 0,
        time_format: 0,
        external: false,
    });
}

struct State {
    restore: bool,
    explorer_details: bool,
    close: usize,
    wheel: String,
    language: usize,
    time_format: usize,
    external: bool,
}

/// 시각 형식 선택지마다 caption 에 붙이는 예시(시안 `TF`). 오프셋은 시안의 고정 예시다.
const TIME_EXAMPLES: [&str; 3] = [
    "2026-10-21 12:12",
    "2026-10-21 12:12:05",
    "2026-10-21 12:12:05 +09:00",
];

/// 본체 `props_time_caption` 과 같은 배치: muted caption + mono text-secondary 예시, measure-md 에서 줄바꿈.
fn time_caption(ui: &mut egui::Ui, theme: &Theme, pick: usize) {
    let size = theme.font_size_caption.value();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &format!(
            "{} {} ",
            crate::i18n::t("settings.general.props_time_format_caption"),
            crate::i18n::t("settings.general.props_time_format_example")
        ),
        0.0,
        egui::TextFormat::simple(
            egui::FontId::proportional(size),
            theme.text_muted().to_egui(),
        ),
    );
    job.append(
        TIME_EXAMPLES[pick.min(TIME_EXAMPLES.len() - 1)],
        0.0,
        egui::TextFormat::simple(
            egui::FontId::monospace(size),
            theme.text_secondary().to_egui(),
        ),
    );
    job.wrap.max_width = theme.measure_md.value().min(ui.available_width());
    ui.add(egui::Label::new(job).wrap());
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_lg, theme.spacing_lg, |ui| {
                ui.spacing_mut().item_spacing.y = theme.settings_row_gap().value();
                let alert = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                    icons::ALERT_TRIANGLE
                        .image(rect.height(), c)
                        .paint_at(ui, rect);
                };
                let rows = [
                    SettingsRow::new("Restore layout:"),
                    SettingsRow::new(crate::i18n::t(
                        "settings.general.explorer_refresh_file_details_label",
                    ))
                    .caption(explorer_details_caption()),
                    SettingsRow::new("Close behavior:"),
                    SettingsRow::new(crate::i18n::t("settings.general.wheel_line_scroll_label"))
                        .caption(wheel_caption()),
                    SettingsRow::new("Language:").caption(language_caption()),
                    SettingsRow::new(crate::i18n::t("settings.general.props_time_format_label")),
                    SettingsRow::new(crate::i18n::t(
                        "settings.general.webhook_allow_external_label",
                    ))
                    .warning(notice(), &alert),
                ];
                let col = settings_label_column(ui, theme, &rows);
                let [
                    restore,
                    explorer_details,
                    close,
                    wheel,
                    language,
                    time_format,
                    webhook,
                ] = rows;
                STATE.with(|s| {
                    let st = &mut *s.borrow_mut();
                    restore.show(ui, theme, col, |ui| {
                        switch(ui, theme, &mut st.restore, None, true);
                    });
                    explorer_details.show(ui, theme, col, |ui| {
                        switch(ui, theme, &mut st.explorer_details, None, true);
                    });
                    close.show(ui, theme, col, |ui| {
                        select(
                            ui,
                            theme,
                            "gallery_general_close",
                            &mut st.close,
                            &[
                                "Ask",
                                crate::i18n::t("settings.general.close_behavior_minimize"),
                                "Quit",
                            ],
                            theme.field_width_lg.value(),
                            true,
                        );
                    });
                    wheel.show(ui, theme, col, |ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                        Input::new()
                            .mono(true)
                            .width(theme.field_width_xs.value())
                            .show(ui, theme, &mut st.wheel);
                        ui.label(
                            egui::RichText::new("pt")
                                .size(theme.font_size_caption.value())
                                .color(theme.text_muted().to_egui()),
                        );
                    });
                    language.show(ui, theme, col, |ui| {
                        select(
                            ui,
                            theme,
                            "gallery_general_language",
                            &mut st.language,
                            &["English", "한국어", "日本語", "简体中文"],
                            theme.field_width_lg.value(),
                            true,
                        );
                    });
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = theme.settings_row_caption_gap().value();
                        time_format.show(ui, theme, col, |ui| {
                            select(
                                ui,
                                theme,
                                "gallery_general_time_format",
                                &mut st.time_format,
                                &[
                                    crate::i18n::t("settings.general.props_time_format_minutes"),
                                    crate::i18n::t("settings.general.props_time_format_seconds"),
                                    crate::i18n::t(
                                        "settings.general.props_time_format_seconds_offset",
                                    ),
                                ],
                                theme.field_width_lg.value(),
                                true,
                            );
                        });
                        time_caption(ui, theme, st.time_format);
                    });
                    webhook.show(ui, theme, col, |ui| {
                        switch(ui, theme, &mut st.external, None, true);
                    });
                });
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "label column",
                "longest label of the subtab, clamp 150 … 240, wraps past 240 (en: webhook label → 240, 2 lines)",
            ),
            ("gap", "16 label → control"),
            ("between rows", "settings-row-gap 12"),
            (
                "row caption",
                "directly under its row · gap 4 · left edge · measure-md · caption muted",
            ),
            (
                "callout",
                "same slot as a caption · warning · alertTriangle 16 · always visible · measure-md",
            ),
            (
                "placement",
                "webhook row last (after Language and Time format in Properties)",
            ),
            (
                "time format",
                "[general] explorer_props_time_format · minutes (default) | seconds | seconds_offset · caption + mono example in the chosen format · open Properties follow on next draw",
            ),
            (
                "explorer file details",
                "[general] explorer_refresh_file_details · switch + caption (cost trade-off) · default off",
            ),
            (
                "setting",
                "[webhook] allow_external · default off · Save / Cancel · from next start",
            ),
        ],
        &[
            TokenChip::without_color("settings-label-width", "label column floor 150"),
            TokenChip::without_color("settings-label-max-width", "label column cap 240"),
            TokenChip::without_color("settings-label-gap", "16"),
            TokenChip::without_color("settings-row-gap", "between rows 12"),
            TokenChip::without_color("settings-row-caption-gap", "row → caption 4"),
            TokenChip::new(
                "accent-warning",
                "callout edge + icon",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::without_color("measure-md", "caption / callout width"),
        ],
    );

    spec::note(
        ui,
        theme,
        "Copy is the app's shipped text (settings.general.*). Every settings subtab draws this grid with tasty_ui_widgets::SettingsRow: the column is measured per subtab and locale.",
    );
    // 좁은 폭에서 행이 쌓이는 모습은 같은 행 격자의 연장이라 이 페이지 끝에 둔다.
    super::settings_narrow_rows::draw(ui, theme);
}
