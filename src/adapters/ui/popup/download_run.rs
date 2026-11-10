//! "다운로드 후 실행" 팝업. 원격에 열 핸들러가 없는 mirror 파일을 이 컴퓨터로 받아 로컬 핸들러 설정대로 연다.
//! 묻는 대화상자, 1 MiB 를 넘는 파일의 확인, 받는 동안의 진행 팝업이다. 실패는 전송 실패 팝업의
//! 다운로드 변형([`TransferErrorKind::DownloadRun`])이 맡는다. 다운로드 상태는
//! [`crate::remote::file_download::client`] 에 있고, 메인 루프가 그 결과를 이 창의 상태로 옮긴다.
//!
//! 대화상자와 큰 파일 확인은 Esc·바깥 클릭이 Cancel 과 같다. 진행 팝업은 Esc·바깥 클릭으로 닫히지
//! 않고, 다운로드를 멈추는 것은 Stop download 하나다.

use std::collections::VecDeque;
use std::time::Instant;

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use super::transfer::{
    TRANSFER_ERROR_POPUP_ID, TransferError, TransferErrorKind, TransferRow, body_region,
    footer_buttons, header_band, header_content_h, line_h, progress_row, progress_row_h, row_pct,
};
use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::t;
use crate::remote::file_download::client::{self as downloads, Failure};
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;
use tasty_ipc::file_download::DownloadRefusal;

pub const DOWNLOAD_RUN_POPUP_ID: &str = "download_run";
pub const DOWNLOAD_RUN_LARGE_POPUP_ID: &str = "download_run_large";
pub const DOWNLOAD_RUN_PROGRESS_POPUP_ID: &str = "download_run_progress";

/// 창 하나의 "다운로드 후 실행" 상태.
#[derive(Debug, Default)]
pub struct DownloadRunState {
    /// 대화상자가 묻고 있는 원격 파일.
    pub ask: Option<DownloadRunAsk>,
    /// 크기 확인을 기다리는 다운로드. 맨 앞 항목을 표시한다.
    pub large: VecDeque<DownloadRunLarge>,
    /// 이 창에서 시작해 아직 끝나지 않은 다운로드.
    pub runs: Vec<DownloadRunEntry>,
}

#[derive(Debug)]
pub struct DownloadRunAsk {
    /// mirror workspace 의 로컬 ID.
    pub workspace: u32,
    /// 원격 호스트의 경로.
    pub path: String,
    /// 경로 줄 앞 표지의 호스트 글자(attach 프로필 이름). 비면 `Remote` 낱말을 쓴다.
    pub host: String,
    /// 서버가 파일 다운로드를 알렸는지. 대화상자를 열 때마다 다시 읽는다.
    pub capable: bool,
    pub content_height: Option<LogicalPx>,
}

#[derive(Debug)]
pub struct DownloadRunLarge {
    pub id: u64,
    pub name: String,
    pub size: u64,
    /// Download and run 으로 답했다. 닫힐 때 취소하지 않는다.
    pub confirmed: bool,
    pub content_height: Option<LogicalPx>,
}

#[derive(Debug)]
pub struct DownloadRunEntry {
    pub id: u64,
    pub name: String,
    pub received: u64,
    pub total: u64,
    /// 조각을 받는 중이다. 진행 팝업은 이 다운로드만 보여 준다.
    pub receiving: bool,
    pub started: Option<Instant>,
}

/// 원격에 열 핸들러가 없는 사용자 파일 열기에서 대화상자를 연다.
pub(crate) fn ask(
    state: &mut MainViewState,
    workspace: u32,
    path: String,
    host: String,
    origin: crate::file::dispatch::FileDispatchOrigin,
) {
    let capable = downloads::client().capable(workspace);
    state.dialogs.download_run.ask = Some(DownloadRunAsk {
        workspace,
        path,
        host,
        capable,
        content_height: None,
    });
    let intent = crate::intent::UiIntent::OpenPopup {
        id: DOWNLOAD_RUN_POPUP_ID,
        mode: crate::intent::OpenPopupMode::CenteredFocused,
    };
    // 핸들러 picker와 같은 출처 규칙이다. 다운로드는 대화상자의 버튼을 누를 때만 시작한다.
    state.dispatch_intent(match origin {
        crate::file::dispatch::FileDispatchOrigin::User => intent.from_user_menu("download_run"),
        crate::file::dispatch::FileDispatchOrigin::Agent
        | crate::file::dispatch::FileDispatchOrigin::PluginUnverified => intent.from_agent_ipc(),
    });
}

/// 실패 이유의 문장 키. 사용자가 멈춘 다운로드는 알리지 않는다.
pub(crate) fn failure_key(failure: &Failure) -> Option<&'static str> {
    Some(match failure {
        Failure::Cancelled => return None,
        Failure::Unsupported => "download_run.dialog.unsupported_server",
        Failure::Disconnected => "download_run.reason.disconnected",
        Failure::TimedOut => "download_run.reason.timeout",
        Failure::Protocol => "download_run.reason.bad_response",
        Failure::LocalWrite => "download_run.reason.local_write",
        Failure::Refused(reason) => match reason {
            DownloadRefusal::Directory => "download_run.reason.directory",
            DownloadRefusal::NotRegular => "download_run.reason.not_regular",
            DownloadRefusal::NotFound => "download_run.reason.not_found",
            DownloadRefusal::PermissionDenied => "download_run.reason.permission_denied",
            DownloadRefusal::Changed => "download_run.reason.changed",
            DownloadRefusal::NotAttached => "download_run.reason.not_attached",
            DownloadRefusal::Io => "download_run.reason.io",
        },
    })
}

/// 실패 팝업을 연다. 사용자가 멈춘 다운로드면 아무것도 하지 않는다.
pub(crate) fn report_failure(state: &mut MainViewState, name: String, failure: &Failure) {
    let Some(key) = failure_key(failure) else {
        return;
    };
    state.dialogs.transfer_error.push_back(TransferError {
        name,
        reason: t(key).to_string(),
        retry: None,
        kind: TransferErrorKind::DownloadRun {
            content_height: None,
        },
    });
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup {
            id: TRANSFER_ERROR_POPUP_ID,
            mode: crate::intent::OpenPopupMode::CenteredFocused,
        }
        .from_user_menu("download_run_failed"),
    );
}

/// 확인 문장의 크기. 0.1 단위로 올림해 1 MiB 를 넘는 파일이 1.0 MiB 로 보이지 않게 한다. 올림한 값이
/// 1024 MiB 이상이면 GiB 로 다시 올림한다.
pub(crate) fn format_size_up(bytes: u64) -> String {
    let tenths = |unit: u128| (u128::from(bytes) * 10).div_ceil(unit);
    let mib = tenths(1 << 20);
    if mib < 10240 {
        return format!("{}.{} MiB", mib / 10, mib % 10);
    }
    let gib = tenths(1 << 30);
    format!("{}.{} GiB", gib / 10, gib % 10)
}

fn measured(h: Option<LogicalPx>, th: &theme::Theme) -> egui::Vec2 {
    // 첫 프레임은 재기 전이라 대략의 높이를 쓰고, 그린 뒤 잰 높이로 맞춘다.
    let h = h.unwrap_or(LogicalPx(th.transfer_popup_width().value() / 2.0));
    egui::vec2(th.transfer_popup_width().value(), h.value())
}

/// PopupDef.default_size — 등록 시점의 placeholder. sizer 가 매 프레임 다시 계산한다.
pub fn download_run_default_size() -> egui::Vec2 {
    let th = theme::theme();
    measured(None, &th)
}

pub fn download_run_sizer(state: &MainViewState, _e: &EngineRead<'_>) -> egui::Vec2 {
    let th = theme::theme();
    let h = state
        .dialogs
        .download_run
        .ask
        .as_ref()
        .and_then(|a| a.content_height);
    measured(h, &th)
}

pub fn download_run_large_sizer(state: &MainViewState, _e: &EngineRead<'_>) -> egui::Vec2 {
    let th = theme::theme();
    let h = state
        .dialogs
        .download_run
        .large
        .front()
        .and_then(|l| l.content_height);
    measured(h, &th)
}

/// 진행 팝업 높이 = header + 받는 다운로드 행 + footer.
pub fn download_run_progress_sizer(state: &MainViewState, _e: &EngineRead<'_>) -> egui::Vec2 {
    let n = state
        .dialogs
        .download_run
        .runs
        .iter()
        .filter(|r| r.receiving)
        .count()
        .max(1);
    let th = theme::theme();
    let header_h = th.transfer_header_pad_y().scaled(2.0) + header_content_h(&th);
    let footer_h = th.transfer_footer_pad_y().scaled(2.0) + LogicalPx(ControlSize::Sm.height(&th));
    let body_h = th.transfer_pad_x().scaled(2.0)
        + progress_row_h(&th).scaled(n as f32)
        + th.transfer_body_gap().scaled(n.saturating_sub(1) as f32);
    egui::vec2(
        th.transfer_popup_width().value(),
        (header_h + body_h + footer_h).value(),
    )
}

/// 닫으면 묻던 파일을 버린다(Cancel·Esc·바깥 클릭).
pub fn on_close_download_run(_ctx: &egui::Context, state: &mut MainViewState, _e: &EngineRead<'_>) {
    state.dialogs.download_run.ask = None;
}

/// 답하지 않고 닫으면 그 다운로드를 취소한다. 남은 확인이 있으면 이어서 연다.
pub fn on_close_download_run_large(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _e: &EngineRead<'_>,
) {
    let Some(front) = state.dialogs.download_run.large.pop_front() else {
        return;
    };
    if !front.confirmed {
        downloads::client().cancel(front.id);
    }
    if !state.dialogs.download_run.large.is_empty() {
        // intent-exempt: popup 자기-close cleanup — on_close 훅에서 큐의 다음 항목을 잇는다
        state
            .popups
            .open_centered_focused(DOWNLOAD_RUN_LARGE_POPUP_ID);
    }
}

/// 묻는 대화상자. 제목·원격 경로 → 설명·한 파일 안내(·옛 서버 이유) → Cancel / Download and run.
pub fn draw_download_run(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let Some(ask) = state.dialogs.download_run.ask.as_ref() else {
        return PopupAction::Close;
    };
    let path = ask.path.clone();
    let host = ask.host.clone();
    let capable = ask.capable;
    let mut cancel = false;
    let mut run = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let drawn = ui.vertical(|ui| {
        ui.set_width(th.transfer_popup_width().value());
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let head = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(
                th.transfer_pad_x().value() as i8,
                th.transfer_header_pad_y().value() as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                wrapped(
                    ui,
                    &th,
                    egui::RichText::new(t("download_run.dialog.title"))
                        .size(th.font_size_max.value())
                        .strong()
                        .color(th.text_primary()),
                    th.font_size_max,
                );
                let empty_label = t("attach.mirror_pill_label");
                remote_path_line(
                    ui,
                    &th,
                    &tasty_ui_widgets::RemoteMark {
                        host: &host,
                        empty_label,
                    },
                    &path,
                );
            });
        let rect = head.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            egui::Stroke::new(
                th.border_width.value(),
                th.separator.to_egui_premultiplied(),
            ),
        );
        body_region(ui, &th, |ui| {
            ui.spacing_mut().item_spacing.y = th.transfer_body_gap().value();
            wrapped(
                ui,
                &th,
                egui::RichText::new(t("download_run.dialog.description"))
                    .size(th.font_size_body.value())
                    .color(th.text_secondary()),
                th.font_size_body,
            );
            wrapped(
                ui,
                &th,
                egui::RichText::new(t("download_run.dialog.single_file"))
                    .size(th.font_size_caption.value())
                    .color(th.text_muted()),
                th.font_size_caption,
            );
            if !capable {
                old_server_well(ui, &th);
            }
        });
        footer_buttons(ui, &th, |ui| {
            run = Button::new(t("download_run.dialog.run"))
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Sm)
                .enabled(capable)
                .leading_icon(&|ui, rect, c| {
                    icons::DOWNLOAD.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, &th)
                .clicked();
            cancel = Button::new(t("download_run.dialog.cancel"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, &th)
                .clicked();
        });
    });
    if let Some(ask) = state.dialogs.download_run.ask.as_mut() {
        ask.content_height = Some(LogicalPx(drawn.response.rect.height()));
    }
    if cancel {
        return PopupAction::Close;
    }
    if run && let Some(ask) = state.dialogs.download_run.ask.take() {
        let name = downloads::safe_file_name(&ask.path);
        match downloads::client().start(ask.workspace, ask.path) {
            Ok(id) => state.dialogs.download_run.runs.push(DownloadRunEntry {
                id,
                name,
                received: 0,
                total: 0,
                receiving: false,
                started: None,
            }),
            Err(failure) => report_failure(state, name, &failure),
        }
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 1 MiB 를 넘는 파일의 확인. attention 글리프·제목 → 파일 이름·크기 문장 → Cancel / Download and run.
pub fn draw_download_run_large(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let Some(front) = state.dialogs.download_run.large.front() else {
        return PopupAction::Close;
    };
    let name = front.name.clone();
    let size = format_size_up(front.size);
    let mut cancel = false;
    let mut run = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let drawn = ui.vertical(|ui| {
        ui.set_width(th.transfer_popup_width().value());
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        header_band(
            ui,
            &th,
            icons::ALERT_TRIANGLE,
            th.accent_attention_raised().into(),
            t("download_run.large.title"),
            None,
        );
        body_region(ui, &th, |ui| {
            ui.spacing_mut().item_spacing.y = th.transfer_body_gap().value();
            let row = TransferRow {
                id: 0,
                name: name.clone(),
                sent: 0,
                total: 0,
                rate: String::new(),
            };
            super::transfer::file_name_line(ui, &th, &row);
            wrapped(
                ui,
                &th,
                egui::RichText::new(t("download_run.large.body").replace("{}", &size))
                    .size(th.font_size_body.value())
                    .color(th.text_secondary()),
                th.font_size_body,
            );
        });
        footer_buttons(ui, &th, |ui| {
            run = Button::new(t("download_run.dialog.run"))
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Sm)
                .leading_icon(&|ui, rect, c| {
                    icons::DOWNLOAD.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, &th)
                .clicked();
            cancel = Button::new(t("download_run.dialog.cancel"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, &th)
                .clicked();
        });
    });
    if let Some(front) = state.dialogs.download_run.large.front_mut() {
        front.content_height = Some(LogicalPx(drawn.response.rect.height()));
        if run {
            front.confirmed = true;
            downloads::client().confirm(front.id);
        }
    }
    if run || cancel {
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 받는 동안의 진행. 업로드 진행 팝업과 같은 형상에 다운로드 제목과 Stop download 를 둔다.
/// 받는 다운로드가 없으면 닫는다.
pub fn draw_download_run_progress(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let rows: Vec<TransferRow> = state
        .dialogs
        .download_run
        .runs
        .iter()
        .filter(|r| r.receiving)
        .map(|r| TransferRow {
            id: r.id,
            name: r.name.clone(),
            sent: r.received,
            total: r.total,
            rate: r.started.map_or_else(String::new, |at| {
                crate::app::image_upload::format_rate(r.received, at.elapsed())
            }),
        })
        .collect();
    if rows.is_empty() {
        return PopupAction::Close;
    }
    let head_pct = rows.first().map(row_pct).unwrap_or(0);
    let mut stop = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    ui.vertical(|ui| {
        ui.set_width(th.transfer_popup_width().value());
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        header_band(
            ui,
            &th,
            icons::DOWNLOAD,
            th.text_muted().into(),
            t("transfer.progress.title_download_run"),
            Some(&format!("{head_pct}%")),
        );
        body_region(ui, &th, |ui| {
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(th.transfer_body_gap().value());
                }
                progress_row(ui, &th, row);
            }
        });
        stop = footer_buttons(ui, &th, |ui| {
            Button::new(t("transfer.progress.stop"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, &th)
                .clicked()
        });
    });
    if stop {
        let ids: Vec<u64> = rows.iter().map(|r| r.id).collect();
        let mut client = downloads::client();
        for id in &ids {
            client.cancel(*id);
        }
        drop(client);
        state
            .dialogs
            .download_run
            .runs
            .retain(|r| !ids.contains(&r.id));
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 옛 서버 이유. info 글리프와 문장을 surface-raised 칸에 항상 보이게 둔다.
fn old_server_well(ui: &mut egui::Ui, th: &theme::Theme) {
    egui::Frame::new()
        .fill(th.surface_raised().into())
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            th.transfer_well_pad_x().value() as i8,
            th.transfer_well_pad_y().value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                let gsz = th.icon_glyph_size_sm.value();
                let (grect, _) = ui.allocate_exact_size(egui::vec2(gsz, gsz), egui::Sense::hover());
                icons::INFO
                    .image(gsz, th.accent_info_raised().into())
                    .paint_at(ui, grect);
                wrapped(
                    ui,
                    th,
                    egui::RichText::new(t("download_run.dialog.unsupported_server"))
                        .size(th.font_size_caption.value())
                        .color(th.text_secondary()),
                    th.font_size_caption,
                );
            });
        });
}

/// 폭에 맞춰 감는 문장. 줄 높이는 글자 크기 × line-height-ui 다.
fn wrapped(ui: &mut egui::Ui, th: &theme::Theme, text: egui::RichText, font: LogicalPx) {
    ui.add(egui::Label::new(text.line_height(Some(line_h(th, font).value()))).wrap());
}

/// 원격 경로 줄. 맨 앞에 호스트 표지(핸들러 picker 와 같은 공용 위젯)를 두고, 경로는 표지와 그 뒤
/// space-sm 을 뺀 상자에서 앞을 줄인다.
fn remote_path_line(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    mark: &tasty_ui_widgets::RemoteMark<'_>,
    path: &str,
) {
    let font = egui::FontId::monospace(th.font_size_caption.value());
    let line_h = ui.fonts(|f| f.row_height(&font));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), line_h),
        egui::Sense::hover(),
    );
    let mark_w = tasty_ui_widgets::paint_fh_remote_mark(
        ui,
        th,
        egui::pos2(rect.left(), rect.center().y),
        mark,
    );
    let left = rect.left() + (mark_w + th.spacing_sm).value();
    let shown = elide_front_mono(ui, th, path, rect.right() - left);
    ui.painter().text(
        egui::pos2(left, rect.top()),
        egui::Align2::LEFT_TOP,
        shown,
        font,
        th.text_muted().into(),
    );
}

/// 경로는 끝(파일 이름)이 보이도록 앞을 `…` 로 줄인다.
fn elide_front_mono(ui: &egui::Ui, th: &theme::Theme, s: &str, max_w: f32) -> String {
    let font = egui::FontId::monospace(th.font_size_caption.value());
    let w = |t: &str| {
        ui.fonts(|f| {
            f.layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .rect
                .width()
        })
    };
    if w(s) <= max_w {
        return s.to_owned();
    }
    let mut cut: VecDeque<char> = s.chars().collect();
    while !cut.is_empty() {
        cut.pop_front();
        let candidate: String = std::iter::once('…').chain(cut.iter().copied()).collect();
        if w(&candidate) <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}

#[cfg(test)]
#[path = "download_run_tests.rs"]
mod tests;
