//! Materialize mirror resources and route their plugin notifications.

use crate::ipc::stream;
use crate::ipc::stream::StreamTag;
use crate::model::{EmptySurface, Surface, Workspace};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tasty_remote::client_session::{OutFrame, SharedFrameSender};
use tasty_terminal::Terminal;
const MARKDOWN_MIRROR_CONTENT_RESULT_EVENT: &str = "markdown_mirror.content_result";
const MARKDOWN_MIRROR_CHANGED_EVENT: &str = "markdown_mirror.changed";

/// 로컬 PTY 없이 mirror 터미널을 만든다. 입력은 별도 스레드로 원격에 전달한다.
pub(super) fn make_mirror_surface(
    remote_id: u32,
    local_id: u32,
    cols: usize,
    rows: usize,
    frame_tx: &SharedFrameSender,
    terminals: &mut crate::runtime::terminal_store::TerminalStore,
    observe_output: bool,
) {
    let mut mirror = Terminal::new_detached(cols, rows);
    bind_mirror_input(&mut mirror, remote_id, frame_tx, false);
    // mirror의 feed_bytes는 process의 lazy 동기화를 거치지 않아 옵저버 게이트를 여기서 초기화한다.
    mirror.set_output_events_enabled(observe_output);
    terminals.insert(local_id, mirror, None);
}

pub(super) fn install_mirror_fallbacks(
    workspace: &Workspace,
    surfaces: &mut HashMap<u32, Box<dyn Surface>>,
) {
    for id in workspace.all_surface_ids() {
        surfaces
            .entry(id)
            .or_insert_with(|| Box::new(EmptySurface::new(id)));
    }
}

/// 이 이벤트의 surface ID는 로컬 ID다.
pub(super) fn push_markdown_changed(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    local_surface_id: u32,
) {
    let payload = serde_json::json!({ "surface_id": local_surface_id });
    let event = MARKDOWN_MIRROR_CHANGED_EVENT;
    super::documents::emit_to_owner(plugin_manager, local_surface_id, event, &payload);
}

/// payload.surface_id는 플러그인이 아는 로컬 ID여야 한다.
pub(super) fn push_markdown_content_result(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    payload: &Value,
) {
    let Some(local) = payload["surface_id"]
        .as_u64()
        .and_then(|id| u32::try_from(id).ok())
    else {
        return;
    };
    let event = MARKDOWN_MIRROR_CONTENT_RESULT_EVENT;
    super::documents::emit_to_owner(plugin_manager, local, event, payload);
}

/// 요청을 보낼 수 없거나 연결이 끊기면 실패를 합성한다. request_id=0은 대기 요청 취소다.
pub(super) fn markdown_content_failure(
    local_surface_id: u32,
    request_id: u64,
    reason: &str,
) -> Value {
    serde_json::json!({
        "surface_id": local_surface_id,
        "request_id": request_id,
        "ok": false,
        "file": Value::Null,
        "source": Value::Null,
        "truncated": false,
        "reason": reason,
    })
}

pub(super) fn bind_mirror_input(
    mirror: &mut Terminal,
    remote_id: u32,
    frame_tx: &SharedFrameSender,
    reconnect: bool,
) {
    // 키 인코딩은 클라이언트 OS가 아니라 입력을 읽는 서버 PTY를 따른다.
    mirror.set_win32_console_input(frame_tx.win32_console_input());
    let sender = frame_tx.clone();
    mirror.bind_external_input(
        Arc::new(move |bytes| {
            // 프레임 상한에서 mux 머리(4바이트)를 뺀 크기로 나눈다.
            for part in bytes.chunks(stream::MAX_FRAME_LEN as usize - 4) {
                if sender
                    .send(OutFrame {
                        tag: StreamTag::Data,
                        payload: stream::encode_mux(remote_id, part),
                    })
                    .is_err()
                {
                    // A partial paste is an explicit retired-connection loss, never a replay candidate.
                    return Err(std::sync::mpsc::SendError(bytes));
                }
            }
            Ok(())
        }),
        reconnect.then(|| frame_tx.began_at()),
    );
}
impl crate::runtime::engine_access::EngineMut<'_> {
    /// Remote placeholders materialize on the App side and never enter the local journal.
    pub(crate) fn reify_displayed_mirror_resources(&mut self, selected: &[u32]) {
        for id in selected {
            if !self.core.is_mirror_surface(*id) {
                continue;
            }
            let deferred = self
                .runtime
                .surfaces
                .get(id)
                .and_then(|surface| surface.as_any().downcast_ref::<EmptySurface>())
                .and_then(|empty| empty.deferred.clone());
            let Some(deferred) = deferred else {
                continue;
            };
            if !super::documents::deferred_available(&self.runtime.surface_registry, &deferred) {
                continue;
            }
            let Some(definition) = self.runtime.surface_registry.get_live(&deferred.kind) else {
                continue;
            };
            let surface = match (definition.restore)(*id, None, &deferred.snapshot)
                .and_then(|prepared| prepared.publish())
            {
                Ok(surface) => surface,
                Err(error) => {
                    tracing::warn!(surface = *id, "mirror kind restoration failed: {error}");
                    continue;
                }
            };
            let kind = surface.kind().to_owned();
            drop(self.runtime.surfaces.insert(*id, surface));
            if let Some((index, pane)) = self.core.find_workspace_index_for_surface(*id)
                && let Some(workspace) = self.core.workspace_at(index).map(|workspace| workspace.id)
                && let Some(workspace) = self.core.mirror_workspace_mut(workspace)
                && let Some(pane) = workspace.pane_layout_mut().find_pane_mut(pane)
                && let Some(tab) = pane.tabs.iter_mut().find(|tab| tab.contains_surface(*id))
                && let Some(descriptor) = tab.surface_mut(*id)
            {
                descriptor.kind = kind;
            }
        }
    }
}

#[cfg(test)]
mod key_encoding_tests {
    use super::bind_mirror_input;
    use tasty_terminal::Terminal;

    /// mirror 키 인코딩은 클라이언트 빌드 OS가 아니라 연결된 서버 PTY를 따른다.
    #[test]
    fn a_mirror_takes_the_key_encoding_of_its_server() {
        for server_is_windows in [true, false] {
            let (tx, _rx) = tasty_remote::connection::channel();
            tx.set_win32_console_input(server_is_windows);
            let mut mirror = Terminal::new_detached(80, 24);
            mirror.set_win32_console_input(!server_is_windows);
            bind_mirror_input(&mut mirror, 7, &tx, false);
            assert_eq!(mirror.win32_console_input(), server_is_windows);
        }
    }

    /// Given 옛 연결의 재그리기 구간 안에 새 연결이 준비돼 reader가 출력을 받았고, When 설치가 그보다 늦게
    /// 재연결로 묶으면, Then 새 연결이 받은 출력은 받은 시각이 설치보다 이르더라도 재그리기가 아니다.
    #[test]
    fn a_reconnect_closes_the_old_window_when_the_new_connection_began() {
        let (tx, _rx) = tasty_remote::connection::channel();
        let began = tx.began_at();
        let mut mirror = Terminal::new_detached(80, 24);
        mirror.note_resize_redraw_at(began - std::time::Duration::from_millis(50));
        let received = began + std::time::Duration::from_millis(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
        bind_mirror_input(&mut mirror, 7, &tx, true);
        mirror.feed_bytes_at(b"new connection", received);
        assert_eq!(mirror.silence_since(), received);
    }
}
