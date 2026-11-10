//! 원격 attach 연결의 무응답 끊김 시간과 재연결 시도 시간을 편집한다. 저장소는 RemoteAttachSettings다.

use tasty_settings::{HEARTBEAT_TIMEOUT_SECS_RANGE, RECONNECT_TIMEOUT_SECS_RANGE};
use tasty_ui_widgets::{SettingsRow, settings_label_column, vspace};

use crate::i18n::t;
use crate::settings::Settings;

pub fn draw_remote_attach_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);
    ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();

    ui.label(
        egui::RichText::new(t("settings.remote_attach.section").to_uppercase())
            .size(th.font_size_micro.value())
            .family(tasty_ui_widgets::label_font_family())
            .extra_letter_spacing(th.letter_spacing_caps(th.font_size_micro).value())
            .color(th.text_muted()),
    );

    let row = SettingsRow::new(t("settings.remote_attach.heartbeat_timeout"))
        .caption(t("settings.remote_attach.heartbeat_timeout_desc"));
    let reconnect = SettingsRow::new(t("settings.remote_attach.reconnect_window"))
        .caption(t("settings.remote_attach.reconnect_window_desc"));
    let col = settings_label_column(ui, &th, [&row, &reconnect]);

    let heartbeat = &mut settings.remote_attach.heartbeat_timeout_secs;
    seconds_row(
        ui,
        &th,
        &row,
        col,
        "remote_attach_heartbeat_timeout",
        HEARTBEAT_TIMEOUT_SECS_RANGE,
        heartbeat,
    );
    let window = &mut settings.remote_attach.reconnect_timeout_secs;
    seconds_row(
        ui,
        &th,
        &reconnect,
        col,
        "remote_attach_reconnect_timeout",
        RECONNECT_TIMEOUT_SECS_RANGE,
        window,
    );
}

/// 초 단위 정수 칸 하나. 칸이 범위로 맞춰 확정하므로 u64를 벗어나지 않는다.
fn seconds_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    row: &SettingsRow,
    col: tasty_type_geometry::length::LogicalPx,
    id: &str,
    (min, max): (u64, u64),
    value: &mut u64,
) {
    let mut secs = i128::from(*value);
    let mut committed = false;
    row.show(ui, th, col, |ui| {
        // 공용 숫자 칸의 기본 범위 문장은 호출하는 쪽의 세로 간격을 따른다.
        ui.spacing_mut().item_spacing.x = th.number_field_unit_gap().value();
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        committed = super::number::integer_field(
            ui,
            th,
            id,
            &super::number::NumberSpec {
                suffix_label: true,
                ..super::number::NumberSpec::int(min as f64, max as f64)
                    .suffix(t("settings.number.unit_s"))
            },
            &mut secs,
        );
    });
    if committed {
        *value = u64::try_from(secs).unwrap_or(max);
    }
}
