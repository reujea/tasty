//! Bind outgoing requests to the original remote session and surface.

#[cfg(test)]
mod tests;

use super::navigation::pending_op_focus_for;
use super::resources::{markdown_content_failure, push_markdown_content_result};
use super::wire::send_control_frame;
use crate::app::App;
use crate::app::attach_client::{GIT_VIEWER_PLUGIN_ID, GIT_VIEWER_QUERY_RESULT_EVENT};
use crate::ipc::stream::{InheritCwd, StreamControl, StreamTag, StructuralOp};
use crate::runtime::engine_session::EngineId;
use crate::state::attach_pending::QueryConsumer;
use std::collections::HashMap;
use tasty_remote::client_session::{AttachClientSession, SessionState, SharedFrameSender};

impl App {
    /// 로컬 구조 변경 큐를 원격으로 보내며 결과는 회신과 delta로 적용한다.
    /// resize 요청만 전송한다. 로컬 mirror grid는 서버의 Resize 회신으로 갱신한다.
    pub(crate) fn dispatch_pending_resize_forwards(&mut self) {
        let mut pending: Vec<(u32, usize, usize)> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            for (sid, (cols, rows)) in engine.remote.pending_resize_forward.drain() {
                pending.push((sid, cols, rows));
            }
        }
        for (local_sid, cols, rows) in pending {
            self.forward_one_resize(local_sid, cols, rows);
        }
    }

    fn forward_one_resize(&mut self, local_sid: u32, cols: usize, rows: usize) {
        let Some((sess, remote_sid)) =
            find_mirror_session_and_remote_id(&mut self.remote.sessions, local_sid, "resize")
        else {
            return;
        };
        if !sess.state.resize_sync.should_send(remote_sid, cols, rows) {
            return;
        }
        let payload = serde_json::to_vec(&StreamControl::ClientResize {
            surface_id: remote_sid,
            cols,
            rows,
        })
        .unwrap_or_default();
        if let Err(e) = sess.send_frame(StreamTag::Control, payload) {
            tracing::warn!("resize forward: 전송 큐가 닫혀 요청을 보내지 못했다: {e}");
            return;
        }
        let now = std::time::Instant::now();
        sess.state
            .resize_sync
            .note_sent(remote_sid, cols, rows, now);
    }

    /// 목록 요청을 원격으로 보낸다. 세션이 없으면 폐기하며 소비자는 자체 timeout으로 실패 처리한다.
    pub(crate) fn dispatch_pending_list_dir_forwards(&mut self) {
        let mut pending: Vec<crate::core::PendingListDirForward> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.append(&mut engine.remote.pending_list_dir_forward);
        }
        for req in pending {
            if let Err(e) =
                self.send_list_dir_request(req.local_ws_id, req.request_id, &req.dir, req.consumer)
            {
                tracing::warn!(
                    "list_dir_request send 실패 (mirror ws {}, request {}): {e}",
                    req.local_ws_id,
                    req.request_id
                );
            }
        }
    }

    /// 원격 git 요청을 보낼 수 없으면 플러그인에 즉시 실패 결과를 전달한다.
    pub(crate) fn dispatch_pending_git_query_forwards(&mut self) {
        let mut pending: Vec<crate::core::PendingGitQueryForward> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.append(&mut engine.remote.pending_git_query_forward);
        }
        for req in pending {
            let send_result = self.send_git_query_request(
                req.local_surface_id,
                req.request_id,
                req.kind,
                req.worktree_path.as_deref(),
                req.diff_path.as_deref(),
            );
            if send_result.is_ok()
                && let Some(consumer) = req.consumer
            {
                self.begin_query_wait(
                    req.local_surface_id,
                    req.request_id,
                    req.admitted,
                    QueryConsumer::Git(consumer),
                );
            }
            if let Err(e) = send_result {
                tracing::warn!(
                    "git_query_request send 실패 (local surface {}, request {}): {e}",
                    req.local_surface_id,
                    req.request_id
                );
                self.fail_pending_git_query(req.request_id, req.kind, &e.to_string());
            }
        }
    }

    /// 원격 원문 요청을 보낼 수 없으면 플러그인에 즉시 실패 결과를 전달한다.
    pub(crate) fn dispatch_pending_markdown_content_forwards(&mut self) {
        let mut pending: Vec<crate::core::PendingMarkdownContentForward> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.append(&mut engine.remote.pending_markdown_content_forward);
        }
        for req in pending {
            let result = self.send_markdown_content_request(&req);
            if result.is_ok() && req.user_wait {
                self.begin_query_wait(
                    req.local_surface_id,
                    req.request_id,
                    req.admitted,
                    QueryConsumer::Markdown(req.local_surface_id),
                );
            }
            if let Err(e) = result {
                tracing::warn!(
                    "markdown_content_request send 실패 (local surface {}, request {}): {e}",
                    req.local_surface_id,
                    req.request_id
                );
                push_markdown_content_result(
                    &mut self.plugin_manager,
                    &markdown_content_failure(req.local_surface_id, req.request_id, &e.to_string()),
                );
            }
        }
    }

    fn fail_pending_git_query(
        &mut self,
        request_id: u64,
        kind: tasty_ipc::stream_hub::GitQueryKind,
        reason: &str,
    ) {
        self.broadcast_git_query_reply(serde_json::json!({
            "request_id": request_id,
            "ok": false,
            "kind": kind.as_wire_str(),
            "data": serde_json::Value::Null,
            "truncated": false,
            "reason": reason,
        }));
    }

    /// request_id=0으로 플러그인이 기다리는 조회를 취소한다.
    /// 호스트는 플러그인의 대기 ID를 몰라 다른 살아 있는 workspace의 조회도 취소될 수 있다.
    pub(super) fn notify_git_viewer_mirror_lost(&mut self) {
        self.broadcast_git_query_reply(serde_json::json!({
            "request_id": 0,
            "ok": false,
            "kind": "",
            "data": serde_json::Value::Null,
            "truncated": false,
            "reason": "mirror workspace disconnected",
        }));
    }

    /// git-viewer에 결과를 전달하고 열린 인스턴스의 repaint를 예약한다.
    fn broadcast_git_query_reply(&mut self, payload: serde_json::Value) {
        let git_viewer_instances: Vec<u64> = match self.plugin_manager.as_mut() {
            Some(mgr) => {
                mgr.emit_host_event_to_plugin(
                    GIT_VIEWER_PLUGIN_ID,
                    GIT_VIEWER_QUERY_RESULT_EVENT,
                    &payload,
                    tasty_plugin_protocol::EventScope::System,
                );
                mgr.popup_instances()
                    .filter(|(_, inst)| inst.plugin_id == GIT_VIEWER_PLUGIN_ID)
                    .map(|(iid, _)| iid)
                    .collect()
            }
            None => Vec::new(),
        };
        if git_viewer_instances.is_empty() {
            return;
        }
        for main in self.main_windows_iter_mut() {
            for iid in &git_viewer_instances {
                main.state.plugin_mesh_popup_pending_repaint.insert(*iid);
            }
        }
    }

    pub(crate) fn dispatch_pending_mesh_context_forwards(&mut self) {
        let mut pending: Vec<(u32, crate::core::AttachMeshContextForward)> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.extend(engine.remote.pending_mesh_context_forward.drain());
        }
        for (local_sid, ctx) in pending {
            self.forward_one_mesh_context(local_sid, ctx);
        }
    }

    fn forward_one_mesh_context(
        &mut self,
        local_sid: u32,
        ctx: crate::core::AttachMeshContextForward,
    ) {
        let Some((sess, remote_sid)) =
            find_mirror_session_and_remote_id(&mut self.remote.sessions, local_sid, "mesh context")
        else {
            return;
        };
        let payload = serde_json::to_vec(&StreamControl::MeshContext {
            surface_id: remote_sid,
            width_px: ctx.width_px,
            height_px: ctx.height_px,
            pixels_per_point: ctx.pixels_per_point,
            theme: ctx.theme,
            focused: ctx.focused,
        })
        .unwrap_or_default();
        if let Err(e) = sess.send_frame(StreamTag::Control, payload) {
            tracing::warn!("mesh context forward: 전송 큐가 닫혀 요청을 보내지 못했다: {e}");
        }
    }

    pub(crate) fn dispatch_pending_mesh_input_forwards(&mut self) {
        let mut pending = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.extend(engine.remote.pending_mesh_input_forward.drain());
        }
        for (local_sid, (admitted, input)) in pending {
            self.forward_one_mesh_input(local_sid, input, admitted);
        }
    }

    fn forward_one_mesh_input(
        &mut self,
        local_sid: u32,
        input: tasty_plugin_protocol::protocol::RawInputWire,
        admitted: std::time::Instant,
    ) {
        let Some((sess, remote_sid)) =
            find_mirror_session_and_remote_id(&mut self.remote.sessions, local_sid, "mesh input")
        else {
            return;
        };
        let payload = serde_json::to_vec(&StreamControl::MeshInput {
            surface_id: remote_sid,
            input,
        })
        .unwrap_or_default();
        if let Err(e) = sess.send_frame(StreamTag::Control, payload) {
            tracing::warn!("mesh input forward: 전송 큐가 닫혀 요청을 보내지 못했다: {e}");
        } else {
            sess.transport.frame_tx.user_input_dispatched(admitted);
        }
    }

    /// texture delta 연결이 끊겨 요청한 full frame 재전송을 원격에 전달한다.
    pub(crate) fn dispatch_pending_mesh_full_resend_forwards(&mut self) {
        let mut pending: Vec<u32> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.extend(engine.remote.pending_mesh_full_resend_forward.drain());
        }
        for local_sid in pending {
            self.forward_one_mesh_full_resend_request(local_sid);
        }
    }

    /// 실제 attention 해제 때만 기록된 큐를 전달한다. 포커스를 유지한다고 반복 전송하지 않는다.
    pub(crate) fn dispatch_pending_attention_clear_forwards(&mut self) {
        let mut pending: Vec<u32> = Vec::new();
        for engine in self.engines_mut().windows_and_pending() {
            pending.extend(engine.remote.pending_attention_clear_forward.drain());
        }
        for local_sid in pending {
            self.forward_one_attention_clear(local_sid);
        }
    }

    /// 해제 전송 실패는 로그를 남기고 폐기한다. 별도 재시도 큐는 없다.
    fn forward_one_attention_clear(&mut self, local_sid: u32) {
        let Some((sess, remote_sid)) = find_mirror_session_and_remote_id(
            &mut self.remote.sessions,
            local_sid,
            "attention clear",
        ) else {
            return;
        };
        let payload = serde_json::to_vec(&StreamControl::ClientAttentionClear {
            surface_id: remote_sid,
        })
        .unwrap_or_default();
        if let Err(e) = sess.send_frame(StreamTag::Control, payload) {
            tracing::warn!("attention clear forward: 전송 큐가 닫혀 요청을 보내지 못했다: {e}");
        }
    }

    fn forward_one_mesh_full_resend_request(&mut self, local_sid: u32) {
        let Some((sess, remote_sid)) = find_mirror_session_and_remote_id(
            &mut self.remote.sessions,
            local_sid,
            "mesh full-resend",
        ) else {
            return;
        };
        let payload = serde_json::to_vec(&StreamControl::MeshFullResendRequest {
            surface_id: remote_sid,
        })
        .unwrap_or_default();
        if let Err(e) = sess.send_frame(StreamTag::Control, payload) {
            tracing::warn!("mesh full-resend forward: 전송 큐가 닫혀 요청을 보내지 못했다: {e}");
        }
    }
}

/// 에이전트의 닫기 요청을 원격 사용자 복원 스택에 넣지 않도록 origin을 전달한다.
fn forward_origin_of(user_triggered: bool) -> tasty_ipc::stream::ForwardOrigin {
    if user_triggered {
        tasty_ipc::stream::ForwardOrigin::User
    } else {
        tasty_ipc::stream::ForwardOrigin::Agent
    }
}

/// origin을 항상 포함한 구조 변경 요청 payload.
fn structural_op_payload(
    op_id: u64,
    op: tasty_ipc::stream::StructuralOp,
    user_triggered: bool,
) -> Vec<u8> {
    serde_json::to_vec(&StreamControl::StructuralOp {
        op_id,
        op,
        origin: Some(forward_origin_of(user_triggered)),
    })
    .unwrap_or_default()
}

/// 로컬 mirror ID로 만든 구조 변경을 원격 ID로 바꾼다. anchor는 호출자가 이미 찾았다.
/// MoveSurface의 target과 생성 op의 cwd 상속 원본도 로컬 ID이므로 같은 세션의 매핑으로 바꾼다.
/// 매핑이 없으면 요청을 거절한다. 로컬 ID를 그대로 보내면 서버의 무관한 surface를 가리키고,
/// 상속 원본을 조용히 빼면 서버가 자기 선택으로 다른 cwd를 고른다.
fn remote_structural_op(
    local_op: &StructuralOp,
    remote_anchor: u32,
    remote_to_local: &HashMap<u32, u32>,
) -> Result<StructuralOp, String> {
    let remote_of = |local: u32| {
        remote_to_local
            .iter()
            .find(|&(_, &l)| l == local)
            .map(|(&r, _)| r)
    };
    let mut wire = local_op.with_anchor_surface_id(remote_anchor);
    if let Some(local_target) = local_op.move_target_surface_id() {
        let remote_target = remote_of(local_target).ok_or_else(|| {
            format!("structural target surface {local_target} is outside the original mirror")
        })?;
        wire = wire.with_move_target_surface_id(remote_target);
    }
    if let Some(local_source) = local_op.inherit_cwd().and_then(InheritCwd::surface) {
        let remote_source = remote_of(local_source).ok_or_else(|| {
            format!("inherit_cwd surface {local_source} is outside the original mirror")
        })?;
        wire = wire.with_inherit_cwd(Some(InheritCwd::Surface(remote_source)));
    }
    Ok(wire)
}

pub(super) fn find_mirror_session_and_remote_id<'a>(
    sessions: &'a mut [AttachClientSession],
    local_sid: u32,
    label: &str,
) -> Option<(&'a mut AttachClientSession, u32)> {
    let Some(sess) = sessions
        .iter_mut()
        .find(|s| s.state.remote_to_local.values().any(|&l| l == local_sid))
    else {
        tracing::warn!(
            "{label} forward: mirror 세션이 로컬 surface {local_sid} 를 갖지 않아 요청을 버린다"
        );
        return None;
    };
    let Some(remote_sid) = sess
        .state
        .remote_to_local
        .iter()
        .find(|&(_, &l)| l == local_sid)
        .map(|(&r, _)| r)
    else {
        tracing::warn!("{label} forward: 로컬 surface {local_sid} 의 원격 ID가 없어 요청을 버린다");
        return None;
    };
    Some((sess, remote_sid))
}

impl App {
    pub(crate) fn capture_remote_target(
        &self,
        workspace: u32,
        surface: Option<u32>,
    ) -> Option<RemoteTarget> {
        let session = self
            .remote
            .sessions
            .iter()
            .find(|session| session.state.local_workspace == workspace)?;
        if !session.transport.frame_tx.epoch().is_active() {
            return None;
        }
        let engine = self
            .engines
            .all_sessions()
            .find(|engine| engine.core_state.has_workspace(workspace))?;
        let surface = match surface {
            Some(id) => Some((
                id,
                *session
                    .state
                    .remote_to_local
                    .iter()
                    .find(|(_, local)| **local == id)?
                    .0,
                engine.runtime.terminals.get(id)?.resource_generation(),
            )),
            None => None,
        };
        Some(RemoteTarget {
            engine: engine.id,
            workspace,
            surface,
            sender: session.transport.frame_tx.clone(),
            port: session.state.bulk_port,
            remote_workspace: session.state.remote_workspace,
        })
    }
    pub(crate) fn remote_target_is_current(&self, target: &RemoteTarget) -> bool {
        if !target.sender.epoch().is_active() {
            return false;
        }
        let Some(session) = self
            .remote
            .sessions
            .iter()
            .find(|session| session.state.local_workspace == target.workspace)
        else {
            return false;
        };
        if !target
            .sender
            .epoch()
            .same(&session.transport.frame_tx.epoch())
        {
            return false;
        }
        let Some(engine) = self.engines.get(target.engine) else {
            return false;
        };
        if !engine.has_workspace(target.workspace) {
            return false;
        }
        target.surface.is_none_or(|(local, remote, generation)| {
            session.state.remote_to_local.get(&remote) == Some(&local)
                && engine
                    .runtime
                    .terminals
                    .matches_generation(local, generation)
        })
    }
}

/// A worker result may use only the engine, mapping and connection that accepted its request.
#[derive(Clone)]
pub(crate) struct RemoteTarget {
    pub(crate) engine: crate::runtime::engine_session::EngineId,
    pub(crate) workspace: u32,
    pub(crate) surface: Option<(u32, u32, tasty_terminal::ResourceGeneration)>,
    pub(crate) sender: SharedFrameSender,
    pub(crate) port: u16,
    pub(crate) remote_workspace: u32,
}

impl App {
    /// Only values are reserved here. The bounded writer cannot receive the instruction before
    /// its operation/outbox commit and the original connection's validated Running claim.
    pub(crate) fn prepare_journal_forward(
        &mut self,
        engine: EngineId,
        op: &StructuralOp,
        user: bool,
        candidates: &[u32],
    ) -> anyhow::Result<crate::app::journal::forward::Draft> {
        let owner = self
            .engines
            .get(engine)
            .ok_or_else(|| anyhow::anyhow!("mirror engine retired"))?;
        let (index, _) = owner
            .core
            .find_workspace_index_for_surface(op.anchor_surface_id())
            .ok_or_else(|| anyhow::anyhow!("mirror anchor missing"))?;
        let workspace = owner
            .core
            .workspace_at(index)
            .filter(|workspace| workspace.mirror)
            .ok_or_else(|| anyhow::anyhow!("target is not a mirror"))?
            .id;
        let stream = owner
            .journal_binding
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("mirror engine has no journal binding"))?
            .stream
            .clone();
        let target = self
            .capture_remote_target(workspace, None)
            .ok_or_else(|| anyhow::anyhow!("no live attach session"))?;
        let session = self
            .remote
            .sessions
            .iter_mut()
            .find(|session| {
                session.state.local_workspace == workspace
                    && session
                        .transport
                        .frame_tx
                        .epoch()
                        .same(&target.sender.epoch())
            })
            .ok_or_else(|| anyhow::anyhow!("mirror connection retired"))?;
        let local_anchor = op.anchor_surface_id();
        let remote_anchor = *session
            .state
            .remote_to_local
            .iter()
            .find(|(_, local)| **local == local_anchor)
            .ok_or_else(|| anyhow::anyhow!("mirror anchor mapping missing"))?
            .0;
        let wire = remote_structural_op(op, remote_anchor, &session.state.remote_to_local)
            .map_err(|reason| anyhow::anyhow!(reason))?;
        let closes_workspace = matches!(op, StructuralOp::CloseWorkspace { .. });
        if closes_workspace {
            super::workspace_close::refusal_of(session)
                .map_or(Ok(()), |refusal| Err(anyhow::anyhow!(refusal.reason())))?;
        }
        let op_id = session.state.op_seq;
        session.state.op_seq = op_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("remote operation sequence exhausted"))?;
        let focus = user
            .then(|| pending_op_focus_for(op, candidates, &session.state.remote_to_local))
            .flatten();
        let payload = structural_op_payload(op_id, wire, user);
        Ok(crate::app::journal::forward::Draft {
            engine,
            stream,
            response: crate::ipc::protocol::JsonRpcResponse::success(
                serde_json::Value::Null,
                serde_json::json!({"forwarded":true,"workspace_index":index}),
            ),
            target,
            local_anchor,
            remote_anchor,
            op_id,
            payload,
            focus,
            silent_failure: !user,
            closes_workspace,
            completion: None,
        })
    }
}

impl App {
    /// 목록 소비자는 wire에 없으므로 요청 ID에 기록한다. None은 picker, Some은 explorer다.
    pub(super) fn send_list_dir_request(
        &mut self,
        local_ws_id: u32,
        request_id: u64,
        dir: &str,
        consumer: Option<u32>,
    ) -> anyhow::Result<()> {
        let Some(sess) = self
            .remote
            .sessions
            .iter_mut()
            .find(|s| s.state.local_workspace == local_ws_id)
        else {
            anyhow::bail!("no attach session for mirror workspace {local_ws_id}");
        };
        let msg = serde_json::json!({
            "event": "list_dir_request",
            "request_id": request_id,
            "dir": dir,
        });
        let result = send_control_frame(&sess.transport.frame_tx, &msg);
        if result.is_ok() {
            sess.state
                .pending_list_dir_consumers
                .insert(request_id, consumer);
        }
        result
    }

    /// 원격 surface ID를 보내 서버가 실제 cwd에서 Git 정보를 찾게 한다.
    fn send_git_query_request(
        &mut self,
        local_surface_id: u32,
        request_id: u64,
        kind: tasty_ipc::stream_hub::GitQueryKind,
        worktree_path: Option<&str>,
        diff_path: Option<&str>,
    ) -> anyhow::Result<()> {
        let Some(sess) = self.remote.sessions.iter().find(|s| {
            s.state
                .remote_to_local
                .values()
                .any(|&l| l == local_surface_id)
        }) else {
            anyhow::bail!("no attach session for mirror surface {local_surface_id}");
        };
        let Some(remote_sid) = sess
            .state
            .remote_to_local
            .iter()
            .find(|&(_, &l)| l == local_surface_id)
            .map(|(&r, _)| r)
        else {
            anyhow::bail!("no remote surface id for mirror surface {local_surface_id}");
        };
        let msg = serde_json::json!({
            "event": "git_query_request",
            "request_id": request_id,
            "surface_id": remote_sid,
            "kind": kind.as_wire_str(),
            "worktree_path": worktree_path,
            "diff_path": diff_path,
        });
        send_control_frame(&sess.transport.frame_tx, &msg)
    }

    fn send_markdown_content_request(
        &mut self,
        req: &crate::core::PendingMarkdownContentForward,
    ) -> anyhow::Result<()> {
        let (local_surface_id, request_id) = (req.local_surface_id, req.request_id);
        let Some(sess) = self
            .remote
            .sessions
            .iter_mut()
            .find(|s| s.state.markdown_locals.contains(&local_surface_id))
        else {
            anyhow::bail!("no attach session holds mirror markdown surface {local_surface_id}");
        };
        let Some(remote_sid) = sess
            .state
            .remote_to_local
            .iter()
            .find(|&(_, &l)| l == local_surface_id)
            .map(|(&r, _)| r)
        else {
            anyhow::bail!("no remote surface id for mirror surface {local_surface_id}");
        };
        if sess.state.phase != SessionState::Connected {
            anyhow::bail!("mirror session is reconnecting");
        }
        let msg = serde_json::json!({
            "event": "markdown_content_request",
            "request_id": request_id,
            "surface_id": remote_sid,
        });
        send_control_frame(&sess.transport.frame_tx, &msg)?;
        sess.state
            .agent_requests
            .note_markdown(req.agent_origin, request_id);
        Ok(())
    }
}
