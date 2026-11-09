//! 원격 파일 수신 폴더와 용량 상한을 편집한다. 저장소는 RemoteTransferSettings다.

use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, Input, SettingsRow, settings_label_column, vspace,
};

use crate::adapters::ui::icons;
use crate::i18n::t;
use crate::settings::Settings;

pub fn draw_remote_transfer_tab(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut browse = false;
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);
    // 헤딩·행·구분선 사이가 모두 settings-row-gap 이다(디자인의 세로 flex gap).
    ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();

    // 섹션 헤딩 "Received files" — font-label micro uppercase text-muted (misc new_card 관례).
    ui.label(
        egui::RichText::new(t("settings.remote_transfer.section").to_uppercase())
            .size(th.font_size_micro.value())
            .family(tasty_ui_widgets::label_font_family())
            .color(th.text_muted()),
    );

    let rows = [
        SettingsRow::new(t("settings.remote_transfer.dir"))
            .caption(t("settings.remote_transfer.dir_desc")),
        SettingsRow::new(t("settings.remote_transfer.max_capacity"))
            .caption(t("settings.remote_transfer.max_capacity_desc")),
    ];
    let col = settings_label_column(ui, &th, &rows);
    let [dir_row, capacity_row] = rows;

    // ── 행 1: Save folder — mono path Input + Browse…(secondary, folder 아이콘) ──
    dir_row.show(ui, &th, col, |ui| {
        // 디자인: [Input flex:1][Browse flex:none], gap 8. right_to_left 로 Browse 를
        // 먼저(우측) 배치하고 Input 이 남은 폭을 채운다(misc add_card 선례).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            if Button::new(t("settings.remote_transfer.browse"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .leading_icon(&|ui, rect, c| {
                    icons::FOLDER.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, &th)
                .clicked()
            {
                browse = true;
            }

            Input::new()
                .mono(true)
                .placeholder(t("settings.remote_transfer.dir_placeholder"))
                .show(ui, &th, &mut settings.remote_transfer.dir);
        });
    });
    row_separator(ui, &th);

    // 용량은 1MiB 이상이며 입력 상한은 두지 않는다. 단위는 입력 칸 밖에 표시한다.
    let mut max_mb = i128::from(settings.remote_transfer.max_mb);
    let mut committed = false;
    capacity_row.show(ui, &th, col, |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            committed = super::number::integer_field(
                ui,
                &th,
                "remote_transfer_max_mb",
                &super::number::NumberSpec {
                    min: Some(1.0),
                    max: None,
                    step: None,
                    decimals: 0,
                    suffix: Some("MiB"),
                    suffix_label: true,
                    unit: None,
                    no_limit: None,
                    range_note: None,
                    align: egui::Align::RIGHT,
                    enabled: true,
                },
                &mut max_mb,
            );
        });
    });
    if committed {
        // 상한이 없으므로 u64 를 넘게 친 값은 u64 끝으로 저장한다.
        settings.remote_transfer.max_mb = u64::try_from(max_mb).unwrap_or(u64::MAX);
    }
    browse
}

/// 행 사이 1px separator(디자인 `borderTop: 1px solid separator`). base bg 위이므로
/// `th.separator`(misc ScriptRow 하단 보더와 동일 관례)로 hline. 위아래 간격은 호출하는 쪽의
/// `item_spacing.y`(settings-row-gap)가 정한다.
pub(super) fn row_separator(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme) {
    let w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, th.border_width.value()), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
}
