//! mirror 크기 요청의 응답 마감을 처리한다. 마감이 지난 첫 요청은 한 번 다시 보내고,
//! 다시 보낸 요청까지 실패하면 실패 목록에 올린다. 대기 상태 자체는 `tasty_remote::resize_sync`가 갖는다.
//! 실패 목록은 각 창의 배너 상태로 넘기고, 배너의 다시 시도·닫기를 해당 세션에 적용한다.

use crate::adapters::ui::attach_size_sync::SizeSyncFailure;
use crate::app::App;
use crate::intent::AttachSizeSyncAction;
use crate::ipc::stream::{StreamControl, StreamTag};
use crate::view::ui::View as _;
use std::collections::HashMap;
use std::time::Instant;
use tasty_remote::client_session::{AttachClientSession, SessionState};
use tasty_remote::resize_sync::Resend;

impl App {
    /// 마감이 지난 요청을 처리하고 가장 이른 다음 마감에 루프를 깨우도록 타이머를 맞춘다.
    /// 연결이 없는 세션은 응답이 올 수 없으므로 대기를 버린다. 재연결 설치가 크기를 다시 맞춘다.
    pub(crate) fn poll_attach_resize_sync(&mut self, now: Instant) {
        let view = self.mirror_view();
        for sess in &mut self.remote.sessions {
            if sess.state.phase != SessionState::Connected {
                sess.state.resize_sync.forget_pending();
                continue;
            }
            let failed_before: Vec<u32> = sess.state.resize_sync.failed().collect();
            // 숨은 mirror와 목표가 바뀐 자동 요청의 마감은 재전송도 실패도 아니다. 보이는 목표는 스윕이 다시
            // 요청한다. 배너의 다시 시도는 목표가 바뀌어도 판정한다.
            let local = |remote: u32| sess.state.remote_to_local.get(&remote).copied();
            sess.state.resize_sync.abandon_due(
                now,
                |remote| view.hides(local(remote)),
                |remote, target| view.retargets(local(remote), target),
            );
            for resend in sess.state.resize_sync.take_due(now) {
                if send_resize(sess, resend) {
                    sess.state.resize_sync.note_resent(resend, now);
                }
            }
            for remote in sess.state.resize_sync.failed() {
                if !failed_before.contains(&remote) {
                    tracing::warn!(
                        "attach resize: mirror workspace {} 의 원격 surface {remote} 크기 동기화가 자동 재시도 뒤에도 응답을 받지 못했다",
                        sess.state.local_workspace
                    );
                }
            }
        }
        let next = self
            .remote
            .sessions
            .iter()
            .filter_map(|sess| sess.state.resize_sync.next_deadline())
            .min();
        crate::app::timers::sync_attach_resize_timer(&mut self.timers, next, now);
        self.sync_attach_size_sync_banners();
    }

    /// 배너의 다시 시도·닫기를 그 mirror 워크스페이스의 세션에 적용한다.
    /// 세션이 이미 없으면 할 일이 없다. 적용 뒤 마감 타이머와 배너를 다시 맞춘다.
    pub(crate) fn apply_attach_size_sync_action(
        &mut self,
        action: AttachSizeSyncAction,
        now: Instant,
    ) {
        let workspace_id = match action {
            AttachSizeSyncAction::Retry { workspace_id }
            | AttachSizeSyncAction::Dismiss { workspace_id } => workspace_id,
        };
        if let Some(sess) = self
            .remote
            .sessions
            .iter_mut()
            .find(|sess| sess.state.local_workspace == workspace_id)
        {
            match action {
                AttachSizeSyncAction::Retry { .. } => {
                    // 전송하지 못한 요청도 마감에 실패로 판정되어 배너에 다시 오른다.
                    for resend in sess.state.resize_sync.retry_failed(now) {
                        send_resize(sess, resend);
                    }
                }
                AttachSizeSyncAction::Dismiss { .. } => sess.state.resize_sync.dismiss_failed(),
            }
        }
        self.poll_attach_resize_sync(now);
    }

    /// 세션별 실패 목록을 로컬 surface로 바꿔 각 창의 배너 상태에 넘긴다. 바뀐 창만 다시 그린다.
    fn sync_attach_size_sync_banners(&mut self) {
        let failures: HashMap<u32, SizeSyncFailure> = self
            .remote
            .sessions
            .iter()
            .filter_map(|sess| {
                let state = &sess.state;
                let surfaces: Vec<u32> = state
                    .resize_sync
                    .banner_surfaces()
                    .into_iter()
                    .filter_map(|remote| state.remote_to_local.get(&remote).copied())
                    .collect();
                (!surfaces.is_empty()).then(|| {
                    (
                        state.local_workspace,
                        SizeSyncFailure {
                            surfaces,
                            retrying: state.resize_sync.retrying(),
                        },
                    )
                })
            })
            .collect();
        for (_, main, _) in self.engines_mut().window_pairs() {
            if main.state.attach_size_syncs.replace(failures.clone()) {
                main.mark_dirty();
            }
        }
    }
}

/// 같은 크기 요청을 다시 큐에 넣는다. 큐가 닫혔으면 연결 종료가 따로 처리되므로 기다리지 않는다.
fn send_resize(sess: &mut AttachClientSession, resend: Resend) -> bool {
    let payload = match serde_json::to_vec(&StreamControl::ClientResize {
        surface_id: resend.surface_id,
        cols: resend.cols,
        rows: resend.rows,
    }) {
        Ok(payload) => payload,
        Err(e) => {
            tracing::warn!("attach resize: 재전송 요청을 직렬화하지 못했다: {e}");
            return false;
        }
    };
    match sess.send_frame(StreamTag::Control, payload) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("attach resize: 전송 큐가 닫혀 재전송하지 못했다: {e}");
            false
        }
    }
}
