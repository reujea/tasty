//! mirror 이미지 붙여넣기를 워커에서 원격 업로드하고 결과 경로를 터미널 입력으로 전달한다.
//! 포커스가 바뀌어도 처음 지정한 surface를 사용한다. 진행·실패 popup은 메인 루프가 갱신한다.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::adapters::ui::popup::transfer::{
    TRANSFER_ERROR_POPUP_ID, TRANSFER_PROGRESS_POPUP_ID, TransferError, TransferErrorKind,
    TransferProgress, TransferRow,
};
use crate::app::App;
use crate::view::ui::View as _;

/// UI 진행 행의 ID. wire의 bulk transfer_id와는 별개다.
static NEXT_UI_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

fn next_ui_transfer_id() -> u64 {
    NEXT_UI_TRANSFER_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) struct ImageUploadOutcome {
    pub(crate) attempt: tasty_remote::outbound::AttemptToken,
    pub(crate) view: Option<(winit::window::WindowId, std::sync::Weak<()>)>,
    pub(crate) remote_target: Option<crate::app::attach_client::RemoteTarget>,
    /// 대상 surface가 사라졌을 때 실패 popup을 표시할 workspace.
    pub(crate) mirror_ws_id: u32,
    /// 붙여넣기를 시작할 때 지정한 로컬 mirror surface.
    pub(crate) surface_id: u32,
    pub(crate) bracketed: bool,
    pub(crate) transfer_id: u64,
    pub(crate) file_name: String,
    /// 실패 후 재시도에 필요한 원본. 성공하면 버린다.
    pub(crate) png_bytes: Vec<u8>,
    /// 성공 경로는 원격 파일시스템 경로다.
    pub(crate) result: anyhow::Result<String>,
}

pub(crate) struct TransferProgressMsg {
    pub(crate) id: u64,
    pub(crate) sent: u64,
    pub(crate) total: u64,
    /// 누적 전송량과 경과 시간으로 계산한 평균 속도.
    pub(crate) rate: String,
}

impl App {
    pub(crate) fn poll_image_uploads(&mut self) {
        self.trigger_pending_image_uploads();
        self.drain_image_upload_results();
    }

    /// 업로드는 결과를 기다리며 블로킹할 수 있어 워커에서 수행한다.
    /// 워커에는 세션 전체 대신 접속 포트와 원격 workspace ID를 전달한다.
    fn trigger_pending_image_uploads(&mut self) {
        let mut reqs: Vec<crate::core::PendingImageUpload> = Vec::new();
        let mut engines = self.engines_mut();
        for engine in engines.reborrow().windows_and_pending() {
            reqs.append(&mut engine.remote.pending_image_uploads);
        }
        for engine in engines.parked() {
            reqs.append(&mut engine.remote.pending_image_uploads);
        }
        for req in reqs {
            let crate::core::PendingImageUpload {
                origin_view,
                mirror_ws_id,
                surface_id,
                bracketed,
                file_name,
                png_bytes,
            } = req;
            let view = self.find_main_with_surface(surface_id).and_then(|window| {
                self.view
                    .views
                    .get(&window)
                    .and_then(|view| view.as_main())
                    .filter(|main| main.base.state.matches_identity(&origin_view))
                    .map(|_| (window, origin_view))
            });
            if view.is_none() {
                tracing::debug!("discarding image upload request from retired View");
                continue;
            }
            let attempt = match self.remote.begin_attempt(None, None) {
                Ok(attempt) => attempt,
                Err(error) => {
                    self.push_transfer_error(
                        surface_id,
                        mirror_ws_id,
                        file_name,
                        error.into(),
                        None,
                    );
                    continue;
                }
            };
            let transfer_id = next_ui_transfer_id();
            let total = png_bytes.len() as u64;
            self.begin_transfer_progress_row(surface_id, transfer_id, &file_name, total);
            let remote_target = self.capture_remote_target(mirror_ws_id, Some(surface_id));
            let heartbeat_timeout = self.services.attach_heartbeat.get();
            let target = remote_target.as_ref().map(|target| {
                (
                    crate::app::attach_client::BulkEndpoint {
                        port: target.port,
                        remote_ws: target.remote_workspace,
                        heartbeat_timeout,
                    },
                    target.sender.epoch(),
                )
            });
            let tx = self.image_upload_tx.clone();
            let progress_tx = self.transfer_progress_tx.clone();
            let proxy = self.view.proxy.clone();
            let rejected_name = file_name.clone();
            let worker_attempt = attempt.clone();
            let spawned = self.remote.spawn_attempt(attempt, move || {
                let start = std::time::Instant::now();
                let result = match target {
                    Some((endpoint, epoch)) => {
                        let progress_tx = progress_tx.clone();
                        let proxy = proxy.clone();
                        crate::app::attach_client::upload_file_over_bulk(
                            endpoint,
                            &file_name,
                            &png_bytes,
                            move |sent, tot| {
                                let rate = format_rate(sent, start.elapsed());
                                #[expect(clippy::let_underscore_must_use, reason = "The receiving session or event loop may have already ended.")]
                                // 수신자 종료/이벤트루프 종료 시에만 실패 — 무시.
                                let _ = progress_tx.send(TransferProgressMsg {
                                    id: transfer_id,
                                    sent,
                                    total: tot,
                                    rate,
                                });
                                #[expect(clippy::let_underscore_must_use, reason = "The receiving session or event loop may have already ended.")]
                                // 이벤트 루프 종료 시에만 실패 — 무시.
                                let _ = proxy.send_event(crate::AppEvent::TransferProgressTick);
                            },
                            &epoch,
                            &worker_attempt,
                        )
                    }
                    None => Err(anyhow::anyhow!(
                        "no attach session for mirror workspace {mirror_ws_id}"
                    )),
                };
                #[expect(clippy::let_underscore_must_use, reason = "The receiving session or event loop may have already ended.")]
                // 수신자(메인 루프)가 종료돼 채널이 닫힌 경우에만 실패 — 무시.
                let _ = tx.send(ImageUploadOutcome {
                    attempt: worker_attempt,
                    view,
                    remote_target,
                    mirror_ws_id,
                    surface_id,
                    bracketed,
                    transfer_id,
                    file_name,
                    png_bytes,
                    result,
                });
                #[expect(clippy::let_underscore_must_use, reason = "The receiving session or event loop may have already ended.")]
                // event loop 가 종료된 경우에만 실패 — 무시.
                let _ = proxy.send_event(crate::AppEvent::ImageUploadReady);
            });
            if let Err(error) = spawned {
                self.finish_transfer_progress_row(surface_id, mirror_ws_id, transfer_id);
                self.push_transfer_error(surface_id, mirror_ws_id, rejected_name, error, None);
            }
        }
    }

    /// 행 ID로 진행 상태를 찾는다. 사용자가 표시를 닫아 행이 없으면 무시한다.
    pub(crate) fn drain_transfer_progress(&mut self) {
        let mut msgs: Vec<TransferProgressMsg> = Vec::new();
        while let Ok(m) = self.transfer_progress_rx.try_recv() {
            msgs.push(m);
        }
        if msgs.is_empty() {
            return;
        }
        for m in msgs {
            for w in self.view.views.values_mut() {
                if let Some(main) = w.as_main_mut()
                    && let Some(prog) = main.state.dialogs.transfer_progress.as_mut()
                    && let Some(row) = prog.row_by_id(m.id)
                {
                    row.sent = m.sent;
                    row.total = m.total;
                    row.rate = m.rate;
                    main.mark_dirty();
                    break;
                }
            }
        }
    }

    /// 업로드 진행 popup이 사용자의 입력 포커스를 가져가지 않게 연다.
    fn begin_transfer_progress_row(
        &mut self,
        surface_id: u32,
        transfer_id: u64,
        file_name: &str,
        total: u64,
    ) {
        let Some(wid) = self.find_main_with_surface(surface_id) else {
            return;
        };
        if let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut()) {
            let prog = main
                .state
                .dialogs
                .transfer_progress
                .get_or_insert_with(TransferProgress::default);
            prog.rows.push(TransferRow {
                id: transfer_id,
                name: file_name.to_string(),
                sent: 0,
                total,
                rate: String::new(),
            });
            // intent-exempt: [부재 src/intent.rs ^ *Centered(,|\()] 포커스를 유지하는 centered 모드가 OpenPopupMode에 없어 직접 연다.
            main.state.popups.open_centered(TRANSFER_PROGRESS_POPUP_ID);
            main.mark_dirty();
        }
    }

    /// 진행 행을 지우고 성공 경로를 원래 surface에 삽입하거나 실패 popup을 연다.
    pub(crate) fn drain_image_upload_results(&mut self) {
        while let Ok(outcome) = self.image_upload_rx.try_recv() {
            self.apply_image_upload_outcome(outcome);
        }
    }

    fn apply_image_upload_outcome(&mut self, outcome: ImageUploadOutcome) {
        let ImageUploadOutcome {
            attempt,
            view,
            remote_target,
            mirror_ws_id,
            surface_id,
            bracketed,
            transfer_id,
            file_name,
            png_bytes,
            result,
        } = outcome;
        let accepted = self.remote.finish_attempt(&attempt).is_some();
        self.finish_transfer_progress_row(surface_id, mirror_ws_id, transfer_id);
        if !accepted
            || view.as_ref().is_none_or(|(window, identity)| {
                self.view
                    .views
                    .get(window)
                    .and_then(|view| view.as_main())
                    .is_none_or(|main| !main.base.state.matches_identity(identity))
            })
        {
            return;
        }
        if remote_target
            .as_ref()
            .is_some_and(|target| !self.remote_target_is_current(target))
        {
            tracing::debug!("discarding upload result for retired remote target");
            return;
        }
        match result {
            Ok(remote_path) => {
                self.paste_uploaded_image_path(surface_id, bracketed, remote_path, remote_target);
            }
            Err(e) => {
                tracing::warn!("image upload to mirror workspace {mirror_ws_id} failed: {e}");
                // 원격 정책 거절은 닫기만, 전송 오류는 재시도를 제공한다. 접두사는 표시에서 제외한다.
                let raw = e.to_string();
                let (retryable, reason) =
                    match raw.strip_prefix(crate::app::attach_client::BULK_REJECT_PREFIX) {
                        Some(clean) => (false, clean.to_string()),
                        None => (true, raw),
                    };
                let name = file_name.clone();
                let retry = if retryable {
                    Some(crate::core::PendingImageUpload {
                        origin_view: view.as_ref().expect("origin View checked").1.clone(),
                        mirror_ws_id,
                        surface_id,
                        bracketed,
                        file_name,
                        png_bytes,
                    })
                } else {
                    None
                };
                self.push_transfer_error(surface_id, mirror_ws_id, name, reason, retry);
            }
        }
    }

    fn paste_uploaded_image_path(
        &mut self,
        surface_id: u32,
        bracketed: bool,
        remote_path: String,
        remote_target: Option<crate::app::attach_client::RemoteTarget>,
    ) {
        let Some(target) = remote_target else {
            tracing::error!("upload succeeded without its originating connection binding");
            return;
        };
        let Some(wid) = self.find_main_with_surface(surface_id) else {
            tracing::warn!("image upload: mirror surface {surface_id} gone before path insertion");
            return;
        };
        if let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut()) {
            if let Some((_, _, generation)) = target.surface {
                crate::view::main::clipboard::dispatch_bound_paste(
                    main,
                    surface_id,
                    generation,
                    bracketed,
                    remote_path,
                );
            }
            main.mark_dirty();
        }
    }

    fn finish_transfer_progress_row(
        &mut self,
        surface_id: u32,
        mirror_ws_id: u32,
        transfer_id: u64,
    ) {
        let wid = self
            .find_main_with_surface(surface_id)
            .or_else(|| self.find_main_with_workspace(mirror_ws_id));
        if let Some(wid) = wid
            && let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut())
        {
            if let Some(prog) = main.state.dialogs.transfer_progress.as_mut() {
                prog.rows.retain(|r| r.id != transfer_id);
                if prog.rows.is_empty() {
                    // popup의 on_close가 상태를 정리하므로 여기서 중복 삭제하지 않는다.
                    main.state.popups.close(TRANSFER_PROGRESS_POPUP_ID); // intent-exempt: popup lifecycle.
                }
            }
            main.mark_dirty();
        }
    }

    /// 대상 surface의 창, 없으면 mirror workspace의 창에 실패를 알린다. retry가 있으면 재시도를 제공한다.
    fn push_transfer_error(
        &mut self,
        surface_id: u32,
        mirror_ws_id: u32,
        name: String,
        reason: String,
        retry: Option<crate::core::PendingImageUpload>,
    ) {
        let wid = self
            .find_main_with_surface(surface_id)
            .or_else(|| self.find_main_with_workspace(mirror_ws_id));
        if let Some(wid) = wid
            && let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut())
        {
            main.state.dialogs.transfer_error.push_back(TransferError {
                name,
                reason,
                retry,
                kind: TransferErrorKind::Upload,
            });
            // 업로드는 사용자의 붙여넣기나 재시도 버튼에서 시작된다.
            main.state.dispatch_intent(
                crate::intent::UiIntent::OpenPopup {
                    id: TRANSFER_ERROR_POPUP_ID,
                    mode: crate::intent::OpenPopupMode::CenteredFocused,
                }
                .from_user_menu("transfer_error"),
            );
            main.mark_dirty();
        }
    }
}

/// 평균 전송률을 표시한다. 경과 시간이 0이면 대시를 반환한다.
pub(crate) fn format_rate(sent: u64, elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 {
        return "—".to_string();
    }
    let bps = sent as f64 / secs;
    const UNITS: &[&str] = &["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let mut v = bps;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}
