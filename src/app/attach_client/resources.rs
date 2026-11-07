//! Materialize mirror resources and route their plugin notifications.

use crate::ipc::stream;
use crate::ipc::stream::StreamTag;
use crate::model::{DeferredPlugin, EmptySurface, Surface, Workspace};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tasty_remote::client_session::{OutFrame, SharedFrameSender};
use tasty_terminal::Terminal;
/// 서버의 is_attach_content_allowed와 같은 kind·소유자 쌍만 로컬 문서로 만든다.
pub(super) const MARKDOWN_MIRROR_KIND: &str = "markdown";
const MARKDOWN_PLUGIN_ID: &str = "com.tasty.markdown";
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

/// kind가 허용된 markdown 플러그인에 등록됐는지 확인한다.
pub(super) fn markdown_mirror_available(
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
) -> bool {
    registry.get_live(MARKDOWN_MIRROR_KIND).is_some_and(|def| {
        matches!(
            &def.source,
            crate::runtime::surface_registry::KindSource::Plugin(p) if p == MARKDOWN_PLUGIN_ID
        )
    })
}

/// 원격 경로는 remote.file로 전달한다. file을 쓰면 플러그인이 로컬 경로로 읽는다.
pub(super) fn create_mirror_markdown_surface(
    descriptor: &Value,
    local_id: u32,
    registry: &crate::runtime::surface_registry::SurfaceKindRegistry,
) -> Option<Box<dyn Surface>> {
    let params = mirror_markdown_params(descriptor);
    let definition = registry.get_live(MARKDOWN_MIRROR_KIND)?;
    if !matches!(&definition.source,crate::runtime::surface_registry::KindSource::Plugin(plugin) if plugin==MARKDOWN_PLUGIN_ID)
    {
        return None;
    }
    match (definition.create)(local_id, None, &params).and_then(|prepared| prepared.publish()) {
        Ok(surface) => Some(surface),
        Err(e) => {
            tracing::warn!(
                "attach mirror: markdown surface {local_id} 생성 실패 — 빈 surface: {e}"
            );
            None
        }
    }
}

/// 생성과 deferred 복원이 같은 remote params를 사용한다.
fn mirror_markdown_params(descriptor: &Value) -> Value {
    let file = descriptor
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let display_name = descriptor
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(MARKDOWN_MIRROR_KIND);
    serde_json::json!({
        "display_name": display_name,
        "remote": { "file": file },
    })
}

pub(super) fn deferred_mirror_markdown_surface(
    descriptor: &Value,
    local_id: u32,
) -> Box<dyn Surface> {
    Box::new(EmptySurface::new_deferred_plugin(
        local_id,
        DeferredPlugin {
            kind: MARKDOWN_MIRROR_KIND.to_string(),
            snapshot: mirror_markdown_params(descriptor),
        },
    ))
}

/// mirror 삭제는 일반 lifecycle 큐를 거치지 않아 사라진 문서를 플러그인에 직접 알린다.
pub(super) fn destroy_mirror_markdown_surfaces(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    ids: impl IntoIterator<Item = u32>,
) {
    let Some(mgr) = plugin_manager.as_mut() else {
        return;
    };
    for id in ids {
        mgr.destroy_remote_surface(id, Some(MARKDOWN_MIRROR_KIND));
    }
}

/// 이 이벤트의 surface ID는 로컬 ID다.
pub(super) fn push_markdown_changed(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    local_surface_id: u32,
) {
    let Some(mgr) = plugin_manager.as_mut() else {
        return;
    };
    mgr.emit_host_event_to_plugin(
        MARKDOWN_PLUGIN_ID,
        MARKDOWN_MIRROR_CHANGED_EVENT,
        &serde_json::json!({ "surface_id": local_surface_id }),
        tasty_plugin_protocol::EventScope::System,
    );
}

/// payload.surface_id는 플러그인이 아는 로컬 ID여야 한다.
pub(super) fn push_markdown_content_result(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    payload: &Value,
) {
    let Some(mgr) = plugin_manager.as_mut() else {
        return;
    };
    mgr.emit_host_event_to_plugin(
        MARKDOWN_PLUGIN_ID,
        MARKDOWN_MIRROR_CONTENT_RESULT_EVENT,
        payload,
        tasty_plugin_protocol::EventScope::System,
    );
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
        reconnect,
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
            let available = match deferred.kind.as_str() {
                MARKDOWN_MIRROR_KIND => markdown_mirror_available(&self.runtime.surface_registry),
                "html" => super::html::resources::available(&self.runtime.surface_registry),
                _ => false,
            };
            if !available {
                continue;
            }
            let Some(definition) = self.runtime.surface_registry.get_live(&deferred.kind) else {
                continue;
            };
            let surface = match (definition.restore)(*id, &deferred.snapshot)
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
}
