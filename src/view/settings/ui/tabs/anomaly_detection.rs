//! Edit the existing detector configuration without changing observation history.
use tasty_settings::{
    CALL_BURST_THRESHOLD_RANGE, CALL_BURST_WINDOW_SECS_RANGE, RSS_SURGE_MIN_MIB_RANGE,
    RSS_SURGE_SAMPLES_RANGE, SLOW_LOOP_THRESHOLD_RANGE, SLOW_LOOP_WINDOW_SECS_RANGE,
};
use tasty_ui_widgets::{AnomalySection, SettingsRow, settings_anomaly, vspace};

use crate::i18n::t;
use crate::settings::Settings;

pub fn draw_anomaly_detection_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);
    let ids = ["rss_surge", "slow_loop", "call_burst"];
    let fields = [
        "rss_surge_samples",
        "rss_surge_min_mib",
        "slow_loop_window_secs",
        "slow_loop_threshold",
        "call_burst_window_secs",
        "call_burst_threshold",
    ];
    let titles = [
        t("settings.anomaly.rss_surge.title"),
        t("settings.anomaly.slow_loop.title"),
        t("settings.anomaly.call_burst.title"),
    ];
    let captions = [
        t("settings.anomaly.rss_surge.caption"),
        t("settings.anomaly.slow_loop.caption"),
        t("settings.anomaly.call_burst.caption"),
    ];
    let labels = [
        t("settings.anomaly.rss_surge_samples.label"),
        t("settings.anomaly.rss_surge_min_mib.label"),
        t("settings.anomaly.slow_loop_window_secs.label"),
        t("settings.anomaly.slow_loop_threshold.label"),
        t("settings.anomaly.call_burst_window_secs.label"),
        t("settings.anomaly.call_burst_threshold.label"),
    ];
    let desc = [
        t("settings.anomaly.rss_surge_samples.caption"),
        t("settings.anomaly.rss_surge_min_mib.caption"),
        t("settings.anomaly.slow_loop_window_secs.caption"),
        t("settings.anomaly.slow_loop_threshold.caption"),
        t("settings.anomaly.call_burst_window_secs.caption"),
        t("settings.anomaly.call_burst_threshold.caption"),
    ];
    let sections = std::array::from_fn(|i| AnomalySection {
        title: titles[i],
        caption: captions[i],
        detector: ids[i],
        rows: [
            SettingsRow::new(t("settings.anomaly.show_notifications")),
            SettingsRow::new(labels[i * 2]).caption(desc[i * 2]),
            SettingsRow::new(labels[i * 2 + 1]).caption(desc[i * 2 + 1]),
        ],
    });
    let a = &mut settings.anomaly_detection;
    settings_anomaly(
        ui,
        &th,
        t("settings.anomaly.notice"),
        t("settings.anomaly.detector"),
        &sections,
        |ui, section, row| {
            if row == 0 {
                let value = match section {
                    0 => &mut a.rss_surge_notify,
                    1 => &mut a.slow_loop_notify,
                    _ => &mut a.call_burst_notify,
                };
                tasty_ui_widgets::switch(ui, &th, value, None, true);
            } else {
                let i = section * 2 + row - 1;
                let (value, range) = match i {
                    0 => (&mut a.rss_surge_samples, RSS_SURGE_SAMPLES_RANGE),
                    1 => (&mut a.rss_surge_min_mib, RSS_SURGE_MIN_MIB_RANGE),
                    2 => (&mut a.slow_loop_window_secs, SLOW_LOOP_WINDOW_SECS_RANGE),
                    3 => (&mut a.slow_loop_threshold, SLOW_LOOP_THRESHOLD_RANGE),
                    4 => (&mut a.call_burst_window_secs, CALL_BURST_WINDOW_SECS_RANGE),
                    _ => (&mut a.call_burst_threshold, CALL_BURST_THRESHOLD_RANGE),
                };
                let mut wide = i128::from(*value);
                let unit = [
                    t("settings.anomaly.rss_surge_samples.unit"),
                    t("settings.anomaly.rss_surge_min_mib.unit"),
                    t("settings.anomaly.slow_loop_window_secs.unit"),
                    t("settings.anomaly.slow_loop_threshold.unit"),
                    t("settings.anomaly.call_burst_window_secs.unit"),
                    t("settings.anomaly.call_burst_threshold.unit"),
                ][i];
                // Keep field + unit together. Expose their natural width to settings_stack_row
                // rather than letting the shared wrapping control hide it on a second line.
                let unit_width = tasty_type_geometry::length::LogicalPx(
                    ui.painter()
                        .layout_no_wrap(
                            unit.to_owned(),
                            tasty_ui_widgets::label_font(th.font_size_caption.value()),
                            th.number_field_unit_fg().to_egui(),
                        )
                        .size()
                        .x,
                );
                ui.set_min_width(
                    (th.number_field_min_width() + th.number_field_unit_gap() + unit_width).value(),
                );
                let spec = super::number::NumberSpec::int(range.0 as f64, range.1 as f64)
                    .unit(unit)
                    .range_note(t("settings.anomaly.range"));
                if super::number::integer_field(ui, &th, fields[i], &spec, &mut wide) {
                    // The integer field commits only a value in the exported u64 range.
                    *value = u64::try_from(wide).unwrap_or(range.0);
                }
            }
        },
    );
}
