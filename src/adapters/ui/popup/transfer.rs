//! 원격 파일 전송의 진행·실패 팝업. 실제 전송과 진행 갱신은 업로드 처리 경로에서 맡는다.
//! 진행 팝업은 모든 행이 끝나면 닫고 바깥 클릭으로는 닫지 않는다.
//! 실패 팝업은 전송 중 실패에만 재시도를 제공한다.

use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::t;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;

pub const TRANSFER_PROGRESS_POPUP_ID: &str = "transfer_progress";
pub const TRANSFER_ERROR_POPUP_ID: &str = "transfer_error";

// 여백은 모두 transfer-* 토큰이고 폭과 함께 UI 배율을 따른다. 줄 높이는 토큰이 아니라
// 각 줄의 글자 크기 × line-height-ui로 계산한다.

/// 글자 크기 한 줄의 높이.
pub(super) fn line_h(th: &theme::Theme, font: LogicalPx) -> LogicalPx {
    font.scaled(th.line_height_ui)
}

/// 헤더 콘텐츠 높이 — glyph md와 제목 한 줄 중 큰 값.
pub(super) fn header_content_h(th: &theme::Theme) -> LogicalPx {
    th.icon_glyph_size_md.max(line_h(th, th.font_size_max))
}

/// 파일명 줄 높이 — glyph md와 본문 한 줄 중 큰 값.
pub(super) fn file_line_h(th: &theme::Theme) -> LogicalPx {
    th.icon_glyph_size_md.max(line_h(th, th.font_size_body))
}

/// 진행 행 하나 — 파일명 줄 + gap + bar + gap + 통계 줄.
pub(super) fn progress_row_h(th: &theme::Theme) -> LogicalPx {
    file_line_h(th)
        + th.transfer_body_gap()
        + th.progress_height()
        + th.transfer_body_gap()
        + line_h(th, th.font_size_caption)
}

/// 진행 팝업 상태 — 진행 중인 파일 행들. 업로드 워커 진행 이벤트가 `row_by_id` 로 갱신한다.
#[derive(Debug, Default, Clone)]
pub struct TransferProgress {
    pub rows: Vec<TransferRow>,
}

impl TransferProgress {
    /// id 로 행을 찾아 가변 참조 반환(진행 이벤트 적용용).
    pub fn row_by_id(&mut self, id: u64) -> Option<&mut TransferRow> {
        self.rows.iter_mut().find(|r| r.id == id)
    }
}

/// 한 파일의 진행 상태.
#[derive(Debug, Clone)]
pub struct TransferRow {
    /// UI 상관 id(`app::image_upload` 가 발급, 진행 이벤트가 이 id 로 행을 지목).
    pub id: u64,
    /// 표시 파일명(mono 말줄임).
    pub name: String,
    /// 지금까지 전송한 바이트.
    pub sent: u64,
    /// 총 바이트.
    pub total: u64,
    /// 표시용 전송 속도 문자열(예 "2.1 MiB/s"). 워커가 계산해 넣는다.
    pub rate: String,
}

/// 실패 팝업 큐 항목.
pub struct TransferError {
    /// 실패한 파일명.
    pub name: String,
    /// 실패 사유(원격 reason 또는 전송/프로토콜 에러 메시지).
    pub reason: String,
    /// `Some` = 전송 중 실패(재시도 가능) → Retry 버튼 + 재전송 페이로드. `None` = 원격
    /// 거부(수신측 용량 초과 등, 재시도 무의미) → Dismiss 단독. 판정은 업로드 결과 처리가 [`BULK_REJECT_PREFIX`]
    /// 로 한다.
    ///
    /// [`BULK_REJECT_PREFIX`]: crate::app::attach_client::BULK_REJECT_PREFIX
    pub retry: Option<crate::core::PendingImageUpload>,
    /// 업로드 실패인지 "다운로드 후 실행" 실패인지. 제목과 사유 칸의 서체가 다르다.
    pub kind: TransferErrorKind,
}

/// 실패 팝업의 용도.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferErrorKind {
    /// 사유는 기계가 보고한 원인을 그대로 인용한다(mono).
    Upload,
    /// 사유는 원인마다 번역한 문장이다(UI 서체, 본문 크기). 높이는 그린 내용을 재서 정한다.
    DownloadRun { content_height: Option<LogicalPx> },
}

/// 진행 팝업 높이 = header + body(행 N개) + footer. 행 수에 맞춰 딱 맞게(빈 하단 방지).
pub fn transfer_progress_sizer(state: &MainViewState, _e: &EngineRead<'_>) -> egui::Vec2 {
    let n = state
        .dialogs
        .transfer_progress
        .as_ref()
        .map(|p| p.rows.len())
        .unwrap_or(1)
        .max(1);
    let th = theme::theme();
    let header_h = th.transfer_header_pad_y().scaled(2.0) + header_content_h(&th);
    let footer_h = th.transfer_footer_pad_y().scaled(2.0) + LogicalPx(ControlSize::Sm.height(&th));
    let body_h = th.transfer_pad_x().scaled(2.0)
        + progress_row_h(&th).scaled(n as f32)
        + th.transfer_body_gap().scaled((n.saturating_sub(1)) as f32);
    egui::vec2(
        th.transfer_popup_width().value(),
        (header_h + body_h + footer_h).value(),
    )
}

/// 실패 팝업 높이 = header + body(prose + reason well) + footer. reason 길이로 well 줄수 추정.
pub fn transfer_error_sizer(state: &MainViewState, _e: &EngineRead<'_>) -> egui::Vec2 {
    let th = theme::theme();
    if let Some(TransferErrorKind::DownloadRun {
        content_height: Some(h),
    }) = state.dialogs.transfer_error.front().map(|e| e.kind)
    {
        return egui::vec2(th.transfer_popup_width().value(), h.value());
    }
    let header_h = th.transfer_header_pad_y().scaled(2.0) + header_content_h(&th);
    let footer_h = th.transfer_footer_pad_y().scaled(2.0) + LogicalPx(ControlSize::Sm.height(&th));
    let (name_len, reason_len) = state
        .dialogs
        .transfer_error
        .front()
        .map(|e| (e.name.chars().count(), e.reason.chars().count()))
        .unwrap_or((0, 0));
    // 본문 폭 372(배율 1의 transfer-popup-width 400 − 2×14). 대략 문자당 ~7px → prose ~53자/줄, well ~50자/줄.
    let prose_lines = (((name_len + 22) as f32) / 53.0).ceil().max(1.0);
    let prose_h = prose_lines * th.font_size_body.value() * 1.5;
    let well_lines = ((reason_len as f32) / 50.0).ceil().max(1.0);
    // well: 위아래 패딩 + 텍스트 줄.
    let well_h =
        th.transfer_well_pad_y().scaled(2.0) + line_h(&th, th.font_size_caption).scaled(well_lines);
    let body_h =
        th.transfer_pad_x().scaled(2.0) + LogicalPx(prose_h) + th.transfer_body_gap() + well_h;
    egui::vec2(
        th.transfer_popup_width().value(),
        (header_h + body_h + footer_h).value(),
    )
}

/// 닫을 때 진행 표시 상태를 비운다. 전송 자체를 중단하는 훅은 아니다.
pub fn on_close_transfer_progress(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
) {
    state.dialogs.transfer_progress = None;
}

/// 진행 팝업 draw_fn. 헤더(download + "Receiving file" + pct) → 파일 행들(파일명 +
/// determinate bar + done/total·rate) → ghost Cancel. 행이 없으면 self-close.
pub fn draw_transfer_progress(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let Some(progress) = state.dialogs.transfer_progress.as_ref() else {
        return PopupAction::Close;
    };
    if progress.rows.is_empty() {
        return PopupAction::Close;
    }
    // 헤더 pct = 첫 행 기준(단일 파일이 표준; 다중은 행별 pct 를 각 bar 가 보여줌).
    let head_pct = progress.rows.first().map(row_pct).unwrap_or(0);
    let rows = progress.rows.clone();

    let mut cancel = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    ui.vertical(|ui| {
        ui.set_width(th.transfer_popup_width().value());
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        header_band(
            ui,
            &th,
            icons::DOWNLOAD,
            th.text_muted().into(),
            t("transfer.progress.title"),
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
        cancel = footer_buttons(ui, &th, |ui| {
            Button::new(t("transfer.progress.cancel"))
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, &th)
                .clicked()
        });
    });

    if cancel {
        // 진행 관망 중단(실제 전송은 abort 하지 않음 — 동기 워커라 중단 불가). 진행 상태를
        // 비워 완료 이벤트가 팝업을 재오픈하지 않게 한다. Ok 완료는 여전히 경로를 삽입한다.
        state.dialogs.transfer_progress = None;
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 이미 결과를 처리한 경로는 큐를 비웠다. 남은 항목이 있으면 첫 실패를 닫은 것으로
/// 처리하고 다음 실패를 표시한다.
pub fn on_close_transfer_error(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &EngineRead<'_>,
) {
    if state.dialogs.transfer_error.is_empty() {
        return;
    }
    state.dialogs.transfer_error.pop_front();
    if !state.dialogs.transfer_error.is_empty() {
        // intent-exempt: popup 자기-close cleanup — on_close 훅에서 큐의 다음 항목을 잇는다
        state.popups.open_centered_focused(TRANSFER_ERROR_POPUP_ID);
    }
}

/// 실패 팝업 draw_fn. 헤더(warn + "Transfer failed") → prose + reason well → Dismiss
/// /(전송 중 실패만)Retry. Esc/scrim = Dismiss. 큐가 비면 self-close.
pub fn draw_transfer_error(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let th = theme::theme();
    let Some(head) = state.dialogs.transfer_error.front() else {
        return PopupAction::Close;
    };
    let name = head.name.clone();
    let reason = head.reason.clone();
    let retryable = head.retry.is_some();
    let download_run = matches!(head.kind, TransferErrorKind::DownloadRun { .. });

    let esc = ui
        .ctx()
        .input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));

    let mut dismiss = esc;
    let mut retry = false;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let drawn = ui.vertical(|ui| {
        ui.set_width(th.transfer_popup_width().value());
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        header_band(
            ui,
            &th,
            icons::ALERT_TRIANGLE,
            th.accent_danger_raised().into(),
            if download_run {
                t("transfer.error.title_download_run")
            } else {
                t("transfer.error.title")
            },
            None,
        );
        body_region(ui, &th, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(
                    egui::RichText::new(&name)
                        .monospace()
                        .strong()
                        .size(th.font_size_body.value())
                        .color(th.text_primary()),
                );
                ui.label(
                    egui::RichText::new(t("transfer.error.body_suffix"))
                        .size(th.font_size_body.value())
                        .color(th.text_secondary()),
                );
            });
            ui.add_space(th.transfer_body_gap().value());
            reason_well(ui, &th, &reason, download_run);
        });
        // danger-fill 금지 — ghost/secondary 만.
        footer_buttons(ui, &th, |ui| {
            if retryable {
                retry = Button::new(t("transfer.error.retry"))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, &th)
                    .clicked();
                if Button::new(t("transfer.error.dismiss"))
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, &th)
                    .clicked()
                {
                    dismiss = true;
                }
            } else if Button::new(t("transfer.error.dismiss"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .show(ui, &th)
                .clicked()
            {
                dismiss = true;
            }
        });
    });
    if let Some(TransferErrorKind::DownloadRun { content_height }) = state
        .dialogs
        .transfer_error
        .front_mut()
        .map(|e| &mut e.kind)
    {
        *content_height = Some(LogicalPx(drawn.response.rect.height()));
    }

    if retry {
        // 저장한 페이로드를 기존 업로드 큐에 다시 넣는다.
        if let Some(err) = state.dialogs.transfer_error.pop_front()
            && let Some(payload) = err.retry
            && let Some(target) =
                crate::runtime::surface_binding::SurfaceBinding::capture(engine, payload.surface_id)
        {
            state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ImageUpload {
                        target,
                        request: payload,
                    },
                )
                .from_user_context_menu(),
            );
        }
        return if state.dialogs.transfer_error.is_empty() {
            PopupAction::Close
        } else {
            PopupAction::None
        };
    }
    if dismiss {
        state.dialogs.transfer_error.pop_front();
        return if state.dialogs.transfer_error.is_empty() {
            PopupAction::Close
        } else {
            PopupAction::None
        };
    }
    PopupAction::None
}

/// 헤더 띠 — glyph + 제목(+ 우측 trailing mono). 하단 separator.
pub(super) fn header_band(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    glyph: icons::Icon,
    glyph_color: egui::Color32,
    title: &str,
    trailing: Option<&str>,
) {
    let pad_y = th.transfer_header_pad_y();
    let pad_x = th.transfer_pad_x();
    let band_h = pad_y.scaled(2.0) + header_content_h(th);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(th.transfer_popup_width().value(), band_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x.value(), rect.top() + pad_y.value()),
        egui::pos2(rect.right() - pad_x.value(), rect.bottom() - pad_y.value()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    let gsz = th.icon_glyph_size_md.value();
    let (grect, _) = child.allocate_exact_size(egui::vec2(gsz, gsz), egui::Sense::hover());
    glyph.image(gsz, glyph_color).paint_at(&child, grect);
    child.label(
        egui::RichText::new(title)
            .size(th.font_size_max.value())
            .strong()
            .color(th.text_primary()),
    );
    if let Some(pct) = trailing {
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(pct)
                    .monospace()
                    .size(th.font_size_caption.value())
                    .color(th.text_muted()),
            );
        });
    }
}

/// 바디 region (사방 transfer-pad-x, 전체폭).
pub(super) fn body_region(ui: &mut egui::Ui, th: &theme::Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(th.transfer_pad_x().value() as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
}

/// 한 파일 진행 행 — 파일명 → determinate bar → done/total · rate. 줄 높이는 sizer와 같은
/// [`file_line_h`]·[`line_h`]로 잡는다.
pub(super) fn progress_row(ui: &mut egui::Ui, th: &theme::Theme, row: &TransferRow) {
    let w = ui.available_width();
    let left_center = egui::Layout::left_to_right(egui::Align::Center);
    file_name_line(ui, th, row);
    ui.add_space(th.transfer_body_gap().value());
    progress_bar(ui, th, row_pct(row));
    ui.add_space(th.transfer_body_gap().value());
    let stats_h = line_h(th, th.font_size_caption).value();
    ui.allocate_ui_with_layout(egui::vec2(w, stats_h), left_center, |ui| {
        ui.set_min_height(stats_h);
        ui.label(
            egui::RichText::new(format!(
                "{} / {}",
                format_mib(row.sent),
                format_mib(row.total)
            ))
            .monospace()
            .size(th.font_size_caption.value())
            .color(th.text_muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(&row.rate)
                    .monospace()
                    .size(th.font_size_caption.value())
                    .color(th.text_muted()),
            );
        });
    });
}

/// 파일 글리프와 mono 파일 이름(뒤를 말줄임) 한 줄.
pub(super) fn file_name_line(ui: &mut egui::Ui, th: &theme::Theme, row: &TransferRow) {
    let w = ui.available_width();
    let left_center = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(egui::vec2(w, file_line_h(th).value()), left_center, |ui| {
        ui.set_min_height(file_line_h(th).value());
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let gsz = th.icon_glyph_size_md.value();
        let (grect, _) = ui.allocate_exact_size(egui::vec2(gsz, gsz), egui::Sense::hover());
        icons::FILE
            .image(gsz, th.text_muted().into())
            .paint_at(ui, grect);
        let avail = ui.available_width();
        let name = elide_mono(ui, th, &row.name, avail);
        ui.label(
            egui::RichText::new(name)
                .monospace()
                .size(th.font_size_body.value())
                .color(th.text_primary()),
        );
    });
}

/// mono 문자열을 폭에 맞게 앞은 두고 뒤를 `…` 로 자른다(specimen elide_mono 와 동일).
pub(super) fn elide_mono(ui: &egui::Ui, th: &theme::Theme, s: &str, max_w: f32) -> String {
    let font = egui::FontId::monospace(th.font_size_body.value());
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
    let mut cut = s.chars().collect::<Vec<_>>();
    while !cut.is_empty() {
        cut.pop();
        let candidate: String = cut.iter().collect::<String>() + "…";
        if w(&candidate) <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}

/// determinate progress bar — recessed track(bg-app) + accent fill(accent-primary),
/// 0ms 무애니(fill 폭 = pct). 토큰: height=progress-height · radius=radius-sm.
fn progress_bar(ui: &mut egui::Ui, th: &theme::Theme, pct: u32) {
    let h = th.progress_height().value();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let r = th.corner_radius_sm.value();
    ui.painter().rect_filled(rect, r, th.bg_app());
    let frac = (pct.min(100) as f32) / 100.0;
    if frac > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, h));
        ui.painter().rect_filled(fill, r, th.accent_primary());
    }
}

/// command-well 패턴 — bg-app + 1px separator + radius, danger 텍스트. 기계가 보고한 원인은 mono 로
/// 인용하고, 번역한 문장(`sentence`)은 UI 서체 본문 크기로 감아 쓴다.
fn reason_well(ui: &mut egui::Ui, th: &theme::Theme, reason: &str, sentence: bool) {
    egui::Frame::new()
        .fill(th.bg_app().into())
        .stroke(egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ))
        .corner_radius(th.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            th.transfer_well_pad_x().value() as i8,
            th.transfer_well_pad_y().value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if sentence {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(reason)
                            .size(th.font_size_body.value())
                            .line_height(Some(line_h(th, th.font_size_body).value()))
                            .color(th.accent_danger_raised()),
                    )
                    .wrap(),
                );
            } else {
                ui.label(
                    egui::RichText::new(reason)
                        .monospace()
                        .size(th.font_size_caption.value())
                        .color(th.accent_danger_raised()),
                );
            }
        });
}

/// 푸터 (transfer-footer-pad-y / transfer-pad-x, borderTop separator, 우측정렬). `add` 는 우→좌 순서로 위젯을
/// 넣고 원하는 클릭 결과(bool)를 반환한다.
pub(super) fn footer_buttons<R>(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let btn_h = ControlSize::Sm.height(th);
    let pad_y = th.transfer_footer_pad_y();
    let pad_x = th.transfer_pad_x();
    let band_h = pad_y.scaled(2.0) + LogicalPx(btn_h);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(th.transfer_popup_width().value(), band_h.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x.value(), rect.top() + pad_y.value()),
        egui::pos2(rect.right() - pad_x.value(), rect.bottom() - pad_y.value()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    add(&mut child)
}

/// 행 진행률 % (총 0 이면 완료 취급 100).
pub(super) fn row_pct(row: &TransferRow) -> u32 {
    if row.total == 0 {
        return 100;
    }
    ((row.sent.min(row.total) as f64 / row.total as f64) * 100.0).round() as u32
}

/// 바이트를 1024 단위의 B·KiB·MiB·GiB·TiB 문자열로 바꾼다.
fn format_mib(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(sent: u64, total: u64) -> TransferRow {
        TransferRow {
            id: 1,
            name: "x".into(),
            sent,
            total,
            rate: String::new(),
        }
    }

    #[test]
    fn row_pct_zero_total_is_complete() {
        assert_eq!(row_pct(&row(0, 0)), 100);
    }

    #[test]
    fn row_pct_partial_and_clamped() {
        assert_eq!(row_pct(&row(0, 100)), 0);
        assert_eq!(row_pct(&row(27, 100)), 27);
        assert_eq!(row_pct(&row(100, 100)), 100);
        assert_eq!(row_pct(&row(200, 100)), 100);
    }

    /// 여백 토큰과 줄 높이가 폭과 함께 UI 배율을 따르고, 줄 높이는 글자 크기 × line-height-ui다.
    #[test]
    fn insets_and_line_heights_follow_the_ui_scale() {
        let base = tasty_themes::mocha_fallback();
        for zoom in [0.85_f32, 1.0, 1.2] {
            let th = theme::Theme::with_colors_and_zoom(base.to_colors(), false, zoom);
            let z = |px: f32| (px * zoom).round();
            assert_eq!(th.transfer_popup_width().value(), z(400.0), "zoom {zoom}");
            assert_eq!(th.transfer_pad_x(), th.spacing_md, "zoom {zoom}");
            assert_eq!(th.transfer_body_gap(), th.spacing_sm, "zoom {zoom}");
            assert_eq!(th.transfer_footer_pad_y(), th.spacing_sm, "zoom {zoom}");
            assert_eq!(th.transfer_well_pad_x(), th.spacing_md, "zoom {zoom}");
            assert_eq!(th.transfer_header_pad_y(), th.spacing_md, "zoom {zoom}");
            assert_eq!(th.transfer_well_pad_y(), th.spacing_sm, "zoom {zoom}");
            assert_eq!(
                header_content_h(&th).value(),
                th.icon_glyph_size_md
                    .value()
                    .max(th.font_size_max.value() * th.line_height_ui),
                "zoom {zoom}"
            );
            assert_eq!(
                progress_row_h(&th).value(),
                file_line_h(&th).value()
                    + 2.0 * th.transfer_body_gap().value()
                    + th.progress_height().value()
                    + th.font_size_caption.value() * th.line_height_ui,
                "zoom {zoom}"
            );
        }
    }

    #[test]
    fn format_mib_units() {
        assert_eq!(format_mib(0), "0 B");
        assert_eq!(format_mib(512), "512 B");
        assert_eq!(format_mib(1024), "1.0 KiB");
        assert_eq!(format_mib(1024 * 1024), "1.0 MiB");
        assert_eq!(format_mib(1024 * 1024 * 1024), "1.0 GiB");
    }
}
