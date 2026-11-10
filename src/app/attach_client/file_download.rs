//! 원격 파일 다운로드의 송신과 결과 처리. 대기 상태는 [`crate::remote::file_download::client`] 에 있다.

use crate::app::App;
use crate::remote::file_download::client::{self as downloads, Event, SessionView};
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
    }

    fn apply_file_download_event(&mut self, event: Event) {
        match event {
            Event::Done {
                id,
                workspace,
                path,
            } => {
                tracing::info!(id, path = %path.display(), "remote file downloaded; opening it locally");
                self.open_downloaded_file(workspace, path);
            }
            // 확인 팝업과 실패 안내는 디자인 대기라 지금은 기록만 한다.
            other => log_unfinished(&other),
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

fn log_unfinished(event: &Event) {
    match event {
        Event::Confirm { id, name, size } => {
            tracing::info!(
                id,
                name,
                size,
                "remote file download waits for confirmation"
            );
        }
        Event::Failed { id, failure } => {
            tracing::warn!(id, ?failure, "remote file download did not finish");
        }
        Event::Done { .. } => {}
    }
}
