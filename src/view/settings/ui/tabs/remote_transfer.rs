//! General › Remote transfer — 원격(mirror) 파일 전송 채널의 수신측 저장 정책
//! 편집. `RemoteTransferSettings{dir, max_mb}`(수신측 백엔드)를 두 행으로
//! 편집한다.
//!
//! 디자인 구조 전사: `gallery/overlays-shared.jsx` `SettingsRemoteTransferFrame`.
//! 콘텐츠 컬럼 =
//! mono uppercase 섹션 헤딩("Received files") + 150px 라벨 grid 2행(Save folder /
//! Maximum size), 각 행 아래 muted 설명 + 행 사이 separator. 갤러리 spec:
//! `gallery/overlays-windows.jsx` "Settings · General › Remote transfer".
//! Browse…/numeric input 페어링은 Scripts(`misc.rs`)·plugin number(`appearance.rs`)
//! 선례를 따른다(rfd folder picker · mono text Input + 정수 파싱).

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, Input, vspace};

use crate::adapters::ui::icons;
use crate::i18n::t;
use crate::settings::Settings;

/// 디자인 settings-row 라벨 컬럼 폭(`gridTemplateColumns: "150px 1fr"`). 4px 그리드
/// 밖 화면 전용 고정 치수(token-policy §c) — 대응 Theme 필드 없음.
const LABEL_COL_WIDTH: LogicalPx = LogicalPx(150.0);

pub fn draw_remote_transfer_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    // 섹션 헤딩 "Received files" — mono micro uppercase text-muted (misc new_card 관례).
    ui.label(
        egui::RichText::new(t("settings.remote_transfer.section").to_uppercase())
            .size(th.font_size_micro.value())
            .monospace()
            .color(th.text_muted()),
    );
    vspace(ui, th.spacing_sm);

    // ── 행 1: Save folder — mono path Input + Browse…(secondary, folder 아이콘) ──
    settings_row(ui, &th, t("settings.remote_transfer.dir"), |ui| {
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
                && let Some(path) = crate::stall_watchdog::without_stall_watch(|| {
                    rfd::FileDialog::new().pick_folder()
                })
            {
                settings.remote_transfer.dir = path.to_string_lossy().into_owned();
            }
            Input::new()
                .mono(true)
                .placeholder(t("settings.remote_transfer.dir_placeholder"))
                .show(ui, &th, &mut settings.remote_transfer.dir);
        });
    });
    row_desc(ui, &th, t("settings.remote_transfer.dir_desc"));
    row_separator(ui, &th);

    // ── 행 2: Maximum size — 설정 창의 숫자 한 모양([`super::number`]) + mono "MiB" ──
    // 폭은 `field_width_xs`(90)이고, 단위는 필드 밖 정적 mono 리터럴이다(i18n 예외 —
    // 단위 기호). 아래 끝이 1 MiB 이고 위 끝은 없다 — 폴더가 담을 수 있는 만큼이다.
    let mut max_mb = settings.remote_transfer.max_mb as f64;
    let mut committed = false;
    settings_row(ui, &th, t("settings.remote_transfer.max_capacity"), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
            committed = super::number::number_field(
                ui,
                &th,
                "remote_transfer_max_mb",
                &super::number::NumberSpec {
                    min: Some(1.0),
                    max: None,
                    step: None,
                    decimals: 0,
                    suffix: Some("MiB"),
                    suffix_mono: true,
                    enabled: true,
                },
                &mut max_mb,
            );
        });
    });
    if committed {
        settings.remote_transfer.max_mb = max_mb as u64;
    }
    row_desc(ui, &th, t("settings.remote_transfer.max_capacity_desc"));
}

/// settings-row 한 행: 150px 좌측 라벨 컬럼(수직 중앙) + `spacing_md`(12) gap +
/// 컨트롤. 행 높이는 `settings_row_min_height`(32) 하한.
fn settings_row(
    ui: &mut egui::Ui,
    th: &tasty_type_appearance::theme::Theme,
    label: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    let min_h = th.settings_row_min_height().value();
    ui.horizontal(|ui| {
        ui.set_min_height(min_h);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_COL_WIDTH.value(), min_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(th.font_size_body.value())
                        .color(th.text_primary()),
                );
            },
        );
        ui.add_space(th.spacing_md.value());
        control(ui);
    });
}

/// 행 아래 muted 설명줄(caption · text-muted, 가용폭 wrap).
fn row_desc(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme, text: &str) {
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(text)
            .size(th.font_size_caption.value())
            .color(th.text_muted()),
    );
}

/// 행 사이 1px separator(디자인 `borderTop: 1px solid separator`). base bg 위이므로
/// `th.separator`(misc ScriptRow 하단 보더와 동일 관례)로 hline.
fn row_separator(ui: &mut egui::Ui, th: &tasty_type_appearance::theme::Theme) {
    vspace(ui, th.spacing_sm);
    let w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w, th.border_width.value()), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(th.border_width.value(), th.separator.to_egui()),
    );
    vspace(ui, th.spacing_sm);
}
