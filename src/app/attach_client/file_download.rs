//! 원격 파일 다운로드의 송신과 결과 처리. 대기 상태는 [`crate::remote::file_download::client`] 에 있다.

use crate::adapters::ui::popup::download_run::{
    DOWNLOAD_RUN_LARGE_POPUP_ID, DOWNLOAD_RUN_PROGRESS_POPUP_ID, DownloadRunLarge, report_failure,
};
use crate::app::App;
use crate::remote::file_download::client::{self as downloads, Event, Failure, SessionView};
use crate::state::MainViewState;
use crate::view::ui::View as _;
use tasty_ipc::file_download::FileDownloadMsg;
use tasty_remote::client_session::SessionState;

impl App {
    /// 세션 상태를 알리고 대기 요청을 보낸 뒤, 끝난 다운로드를 처리한다.
    pub(crate) fn pump_file_downloads(&mut self) {
        let sends = {
            let views: Vec<SessionView> = self
                .remote
                .sessions
                .iter()
                .map(|s| SessionView {
                    workspace: s.state.local_workspace,
                    capable: s.transport.file_download,
                    connected: s.state.phase == SessionState::Connected,
                    epoch: s.transport.frame_tx.epoch(),
                })
                .collect();
            downloads::client().sync(&views)
        };
        for (workspace, msg) in sends {
            let FileDownloadMsg::FileDownloadRequest { request_id, .. } = &msg else {
                continue;
            };
            let request_id = *request_id;
            let sent = self
                .remote
                .sessions
                .iter()
                .find(|s| s.state.local_workspace == workspace)
                .is_some_and(|sess| match serde_json::to_value(&msg) {
                    Ok(value) => super::wire::send_control_frame(&sess.transport.frame_tx, &value)
                        .map_err(|e| tracing::warn!("attach file download not sent: {e}"))
                        .is_ok(),
                    Err(e) => {
                        tracing::warn!("attach file download did not serialize: {e}");
                        false
                    }
                });
            if !sent {
                downloads::client().send_failed(request_id);
            }
        }
        let events = downloads::client().take_events();
        for event in events {
            self.apply_file_download_event(event);
        }
        self.sync_download_progress();
    }

    fn apply_file_download_event(&mut self, event: Event) {
        match event {
            Event::Done {
                id,
                workspace,
                path,
            } => {
                self.with_download_owner(id, |state| {
                    state.dialogs.download_run.runs.retain(|r| r.id != id);
                });
                tracing::info!(id, path = %path.display(), "remote file downloaded; opening it locally");
                self.open_downloaded_file(workspace, path);
            }
            Event::Confirm { id, name, size } => self.ask_download_size(id, name, size),
            Event::Failed { id, failure } => {
                tracing::warn!(id, ?failure, "remote file download did not finish");
                self.with_download_owner(id, |state| download_failed(state, id, &failure));
            }
        }
    }

    /// 1 MiB 를 넘는 파일을 그 다운로드를 시작한 창에서 묻는다. 창에서 시작하지 않았으면 기록만 한다.
    fn ask_download_size(&mut self, id: u64, name: String, size: u64) {
        let asked = self.with_download_owner(id, |state| {
            let runs = &mut state.dialogs.download_run;
            if let Some(run) = runs.runs.iter_mut().find(|r| r.id == id) {
                run.name.clone_from(&name);
            }
            runs.large.push_back(DownloadRunLarge {
                id,
                name,
                size,
                confirmed: false,
                content_height: None,
            });
            if runs.large.len() == 1 {
                state.dispatch_intent(
                    crate::intent::UiIntent::OpenPopup {
                        id: DOWNLOAD_RUN_LARGE_POPUP_ID,
                        mode: crate::intent::OpenPopupMode::CenteredFocused,
                    }
                    .from_user_menu("download_run_large"),
                );
            }
        });
        if !asked {
            tracing::info!(id, size, "remote file download waits for confirmation");
        }
    }

    /// 그 다운로드를 시작한 창의 상태에 적용한다. 창에서 시작하지 않았으면(debug IPC) false 다.
    fn with_download_owner(&mut self, id: u64, apply: impl FnOnce(&mut MainViewState)) -> bool {
        for view in self.view.views.values_mut() {
            if let Some(main) = view.as_main_mut()
                && main
                    .state
                    .dialogs
                    .download_run
                    .runs
                    .iter()
                    .any(|r| r.id == id)
            {
                apply(&mut main.state);
                main.mark_dirty();
                return true;
            }
        }
        false
    }

    /// 받는 중인 다운로드의 진행을 그 창의 진행 팝업에 옮긴다.
    fn sync_download_progress(&mut self) {
        let progress = downloads::client().receiving();
        for view in self.view.views.values_mut() {
            let Some(main) = view.as_main_mut() else {
                continue;
            };
            let mut changed = false;
            for run in &mut main.state.dialogs.download_run.runs {
                let Some(p) = progress.iter().find(|p| p.id == run.id) else {
                    continue;
                };
                if !run.receiving || run.received != p.received || run.total != p.total {
                    changed = true;
                }
                run.receiving = true;
                run.received = p.received;
                run.total = p.total;
                if !p.name.is_empty() {
                    run.name.clone_from(&p.name);
                }
                run.started.get_or_insert_with(std::time::Instant::now);
            }
            let receiving = main
                .state
                .dialogs
                .download_run
                .runs
                .iter()
                .any(|r| r.receiving);
            if receiving && !main.state.popups.is_open(DOWNLOAD_RUN_PROGRESS_POPUP_ID) {
                // intent-exempt: [부재 src/intent.rs ^ *Centered(,|\()] 포커스를 유지하는 centered 모드가 OpenPopupMode에 없어 직접 연다.
                main.state
                    .popups
                    .open_centered(DOWNLOAD_RUN_PROGRESS_POPUP_ID);
                changed = true;
            }
            if changed {
                main.mark_dirty();
            }
        }
    }

    /// 받은 로컬 파일을 사용자 요청으로 연다. 이후는 로컬 핸들러 설정을 그대로 따른다.
    /// mirror 를 가진 창에 보내며, 창이 사라졌으면 첫 창에 보낸다.
    fn open_downloaded_file(&mut self, workspace: u32, path: std::path::PathBuf) {
        let intent = crate::app::command::DomainIntent::DispatchFile {
            target: crate::file::format::FileTarget::new(path),
            depth: crate::file::format::DetectDepth::Deep,
            origin_surface_id: None,
            dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        }
        .from_user_menu("remote_file_download");
        let mut pairs: Vec<_> = self.engines_mut().window_pairs().collect();
        let index = pairs
            .iter()
            .position(|(_, _, engine)| engine.find_workspace_index_for_id(workspace).is_some())
            .unwrap_or(0);
        match pairs.get_mut(index) {
            Some((_, main, _)) => {
                main.state.dispatch_intent(intent);
                main.mark_dirty();
            }
            None => tracing::warn!("no window to open a downloaded file in"),
        }
    }
}

/// 끝나지 못한 다운로드를 창 상태에서 지우고 실패를 알린다. 묻던 크기 확인은 취소하지 않고 닫는다.
fn download_failed(state: &mut MainViewState, id: u64, failure: &Failure) {
    let runs = &mut state.dialogs.download_run;
    let name = runs
        .runs
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    runs.runs.retain(|r| r.id != id);
    match runs.large.front_mut() {
        Some(front) if front.id == id => {
            front.confirmed = true;
            state.popups.close(DOWNLOAD_RUN_LARGE_POPUP_ID); // intent-exempt: popup lifecycle.
        }
        _ => runs.large.retain(|l| l.id != id),
    }
    report_failure(state, name, failure);
}
