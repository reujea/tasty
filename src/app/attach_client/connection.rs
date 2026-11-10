//! Install and retire mirror connections while preserving the originating View.

#[cfg(test)]
mod tests;

use super::dispatch::{AttachSource, Outcome, dispatch_attach};
use super::navigation::{capture_focused_remote, restore_focus_after_delta};
use super::pending;
use super::projection::{build_mirror_workspace, lease_mirror_ids};
use super::resources::{
    bind_mirror_input, destroy_mirror_markdown_surfaces, install_mirror_fallbacks,
    markdown_content_failure, push_markdown_changed, push_markdown_content_result,
};
use super::survivors::{install_reconnected_survivors, merge_survivor_mapping};
use crate::app::App;
use crate::app::window_access::{EngineScanMut, engines_mut};
use crate::ipc::stream::StreamTag;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_session::EngineId;
use crate::view::ui::View as _;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use tasty_remote::client_session::{
    AttachClientSession, ClientSessionState, MirrorStructureIds, SessionState,
};
use tasty_remote::resize_sync::ResizeSync;
use tasty_remote::transport::PreparedConnection;

impl App {
    /// Keep the popup's fixed installation target through the existing user attach guard.
    pub(crate) fn queue_browser_mirror(
        &mut self,
        target: pending::PendingMirrorInstall,
        port: u16,
        workspace: u32,
        tunnel: Option<tasty_ssh::SshTunnel>,
    ) -> anyhow::Result<()> {
        let own_port = self.services.own_ipc_port();
        let mut tunnel = tunnel;
        let outcome = dispatch_attach(own_port, port, workspace, AttachSource::User, || {
            self.queue_mirror_connection(target, port, workspace, tunnel.take())
        });
        self.remote.retire_tunnel(tunnel);
        match outcome {
            Outcome::Connected(result) => result,
            Outcome::RejectedSelf => anyhow::bail!("self attach is not supported"),
        }
    }

    /// IPC `remote.attach`와 자동 attach의 연결도 같은 자기 포트 검사를 거친다.
    /// 자기 포트면 연결을 시작하지 않고 터널을 정리한 뒤 None을 반환한다.
    pub(crate) fn queue_endpoint_mirror(
        &mut self,
        target: pending::PendingMirrorInstall,
        port: u16,
        workspace: u32,
        tunnel: Option<tasty_ssh::SshTunnel>,
    ) -> Option<anyhow::Result<()>> {
        let own_port = self.services.own_ipc_port();
        let mut tunnel = tunnel;
        let outcome = dispatch_attach(own_port, port, workspace, AttachSource::Endpoint, || {
            self.queue_mirror_connection(target, port, workspace, tunnel.take())
        });
        self.remote.retire_tunnel(tunnel);
        match outcome {
            Outcome::Connected(result) => Some(result),
            Outcome::RejectedSelf => None,
        }
    }

    pub(crate) fn dispatch_pending_gui_attach(&mut self) {
        let mut requests = Vec::new();
        for session in self.engines.all_sessions_mut() {
            requests.extend(
                std::mem::take(&mut session.remote.pending_gui_attach)
                    .into_iter()
                    .map(|request| (session.id, request)),
            );
        }
        for (engine, (port, workspace, run)) in requests {
            self.try_dispatch_one_gui_attach_ipc(engine, port, workspace, run);
        }
    }

    fn try_dispatch_one_gui_attach_ipc(
        &mut self,
        engine: EngineId,
        port: u16,
        workspace: u32,
        run: u64,
    ) {
        let own_port = self.services.own_ipc_port();
        let failure = match dispatch_attach(own_port, port, workspace, AttachSource::Ipc, || {
            let mut target = self
                .mirror_install_target(Some(engine), None, None, false)?
                .with_run(Some(run));
            target.manual = Some(format!("port {port} workspace {workspace}"));
            self.queue_mirror_connection(target, port, workspace, None)
        }) {
            Outcome::Connected(Ok(())) => return,
            Outcome::Connected(Err(e)) => {
                tracing::warn!("gui attach failed (port={port}, ws={workspace}): {e}");
                e.to_string()
            }
            Outcome::RejectedSelf => format!("attach target port {port} is this instance"),
        };
        crate::request_runs::finish(
            run,
            crate::request_runs::RunState::Failed,
            failure,
            serde_json::Value::Null,
        );
    }

    pub(super) fn install_new_mirror(
        &mut self,
        target: &pending::PendingMirrorInstall,
        prepared: PreparedConnection,
    ) -> anyhow::Result<u32> {
        let PreparedConnection {
            port,
            remote_workspace: workspace,
            client_id,
            name,
            surfaces,
            tree,
            resize_acks,
            transport,
        } = prepared;
        let frame_tx = transport.frame_tx.clone();
        let local_ws_id;
        let remote_to_local: HashMap<u32, u32>;
        let markdown_locals: HashSet<u32>;
        let html_locals: HashSet<u32>;
        let mut structure_ids = MirrorStructureIds::default();
        {
            let Some(window) = target.window else {
                anyhow::bail!("mirror View was retired");
            };
            let Some((main, mut engine)) = engines_mut!(self).window_pair(window) else {
                anyhow::bail!("no focused window to host mirror workspace");
            };
            let ids = lease_mirror_ids(
                &engine.runtime.ids,
                &engine.runtime.waker,
                &tree,
                surfaces.len(),
                true,
            )?;

            let mut mapping =
                merge_survivor_mapping(&HashMap::new(), &surfaces, &ids, &frame_tx, &mut engine)?;
            markdown_locals = mapping.markdown_ids();
            html_locals = mapping.html.clone();
            remote_to_local = std::mem::take(&mut mapping.remote_to_local);

            // 저장된 원격 행에 처음 붙으면 그 행 자리에 내용을 넣는다. 아니면 새 행을 만든다.
            local_ws_id = match target.row {
                Some(row) => row,
                None => ids.next_workspace()?,
            };
            let mut ws = build_mirror_workspace(
                &mut structure_ids,
                &mut main.state.navigation,
                local_ws_id,
                &name,
                &tree,
                &ids,
                &remote_to_local,
                &mapping.terminals,
                &mapping.mesh,
                &mapping.explorer,
                &mut mapping.markdown,
            )?;
            if target.row.is_some() && !engine.has_workspace(local_ws_id) {
                anyhow::bail!("saved remote row {local_ws_id} is gone");
            }
            install_mirror_fallbacks(&ws, &mut engine.runtime.surfaces);
            ws.mirror = true;
            if target.row.is_some() {
                // 행의 이름·카테고리·연결 대상은 replace가 남긴다.
                if engine.replace_mirror_workspace(ws).is_err() {
                    anyhow::bail!("saved remote row {local_ws_id} is gone");
                }
            } else {
                // 사용자가 원격 연결 팝업에서 붙인 workspace만 저장된 원격 행이 된다.
                if target.user
                    && let Some(profile) = &target.profile
                {
                    ws.remote_row = Some(crate::model::RemoteBinding {
                        profile: profile.clone(),
                        remote_workspace: workspace,
                    });
                }
                engine.push_mirror_workspace(ws);
            }
            main.state.reconcile_presentation(engine.core);
            main.mark_dirty();
        }

        self.remote.sessions.push(AttachClientSession {
            transport,
            state: ClientSessionState {
                structure_ids,
                local_workspace: local_ws_id,
                remote_to_local,
                phase: SessionState::Connected,
                client_id,
                remote_workspace: workspace,
                bulk_port: port,
                saved_row: (target.row.is_some() || (target.user && target.profile.is_some()))
                    .then_some(local_ws_id),
                op_seq: 0,
                pending_op_focus: HashMap::new(),
                agent_requests: Default::default(),
                next_delta_focus: None,
                resize_sync: ResizeSync::for_connection(resize_acks),
                remote_label: format!("127.0.0.1:{port}"),
                profile: target.profile.clone(),
                pending_list_dir_consumers: HashMap::new(),
                capture_uploads: HashMap::new(),
                markdown_locals,
                html_locals,
                resync_pending: None,
                resync_awaiting_window: false,
                workspace_close: Default::default(),
            },
        });
        tracing::info!(
            "gui attach: mirror workspace {local_ws_id} from 127.0.0.1:{port} (remote ws {workspace})"
        );
        self.probe_remote_home(local_ws_id);
        if target.row.is_none() && target.user && target.profile.is_some() {
            self.journal
                .admit_remote_row(target.engine, local_ws_id, false);
        }
        Ok(local_ws_id)
    }

    /// 기존 mirror의 로컬 ID·scrollback·포커스를 보존하며 새 연결의 구조를 반영한다.
    /// A prepared connection retains its snapshot tail until the original engine can install it.
    pub(crate) fn reconnect_session(
        &mut self,
        sess_idx: usize,
        port: u16,
        tunnel: Option<tasty_ssh::SshTunnel>,
    ) -> anyhow::Result<()> {
        let target = match self.mirror_install_target(None, None, Some(sess_idx), false) {
            Ok(target) => target,
            Err(error) => {
                self.remote.retire_tunnel(tunnel);
                return Err(error);
            }
        };
        let workspace = self
            .remote
            .sessions
            .get(sess_idx)
            .ok_or_else(|| anyhow::anyhow!("reconnect session missing"))?
            .state
            .remote_workspace;
        self.queue_mirror_connection(target, port, workspace, tunnel)
    }

    pub(super) fn install_reconnected_mirror(
        &mut self,
        sess_idx: usize,
        target: &pending::PendingMirrorInstall,
        prepared: PreparedConnection,
    ) -> anyhow::Result<()> {
        let PreparedConnection {
            port,
            remote_workspace: workspace,
            client_id,
            name,
            surfaces,
            tree,
            resize_acks,
            transport,
            ..
        } = prepared;
        let shared_frame_tx = transport.frame_tx.clone();
        let local_workspace = target
            .reconnect
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("reconnect target missing"))?
            .0;
        let wid = self.engines.window_of(target.engine).ok_or_else(|| {
            anyhow::anyhow!("reconnect View is parked; retain the pending resource")
        })?;
        let removed_documents;
        {
            let sess = &mut self.remote.sessions[sess_idx];
            sess.transport.frame_tx.retire();
            sess.transport.disconnected.store(true, Ordering::Release);

            let Some((main, mut engine)) = engines_mut!(self).window_pair(wid) else {
                anyhow::bail!("window {wid:?} 가 더 이상 MainView 가 아님 — 재연결 취소");
            };
            let ids = lease_mirror_ids(
                &engine.runtime.ids,
                &engine.runtime.waker,
                &tree,
                surfaces.len(),
                false,
            )?;

            let old_focused_remote: Option<u32> = engine
                .workspaces()
                .into_iter()
                .find(|w| w.id == local_workspace)
                .and_then(|ws| {
                    capture_focused_remote(&main.state.navigation, ws, &sess.state.remote_to_local)
                });

            engine.remote.discard_connection_requests(
                &sess.state.remote_to_local,
                sess.state.local_workspace,
            );
            let mut mapping = install_reconnected_survivors(
                &mut sess.state.remote_to_local,
                &mut sess.state.resize_sync,
                resize_acks,
                &surfaces,
                &ids,
                &shared_frame_tx,
                &mut engine,
            )?;
            // 연결 사이에 빠진 출력을 연속된 스트림으로 읽지 않도록 표지를 바꾼다.
            for (&remote, &local) in &sess.state.remote_to_local {
                if let Some(terminal) = engine.runtime.terminals.get_mut(local) {
                    bind_mirror_input(terminal, remote, &shared_frame_tx, true);
                    terminal.renew_output_stream();
                }
            }
            // 옛 연결의 mesh 캐시와 구독 기록을 비워 full texture를 다시 요청한다.
            for &local in mapping.mesh.keys() {
                engine.remote.attach_mesh_frames.remove(local);
                main.attach_mesh_input.remove(&local);
            }
            removed_documents = sess
                .state
                .replace_document_ids(mapping.markdown_ids(), mapping.html.clone());

            let Some(_) = engine
                .workspaces()
                .into_iter()
                .position(|w| w.id == local_workspace)
            else {
                anyhow::bail!(
                    "mirror workspace {local_workspace} 를 engine 에서 못 찾음 — 재연결 취소"
                );
            };
            let mut ws = build_mirror_workspace(
                &mut sess.state.structure_ids,
                &mut main.state.navigation,
                local_workspace,
                &name,
                &tree,
                &ids,
                &sess.state.remote_to_local,
                &mapping.terminals,
                &mapping.mesh,
                &mapping.explorer,
                &mut mapping.markdown,
            )?;
            install_mirror_fallbacks(&ws, &mut engine.runtime.surfaces);
            ws.mirror = true;
            if !restore_focus_after_delta(
                &mut main.state.navigation,
                &mut ws,
                old_focused_remote,
                &sess.state.remote_to_local,
            ) {
                tracing::info!(
                    "gui reconnect: 이전 focus surface 를 재연결 후 트리에서 찾지 못함 — 원격 기본 focus 유지"
                );
            }
            engine
                .replace_mirror_workspace(ws)
                .unwrap_or_else(|_| panic!("mirror workspace disappeared"));
            main.state.reconcile_presentation(engine.core);
            finish_reconnected_view(&mut main.state, local_workspace);
            main.mark_dirty();
        }
        destroy_mirror_markdown_surfaces(&mut self.plugin_manager, removed_documents.markdown);
        super::html::resources::destroy(&mut self.plugin_manager, removed_documents.html);

        let sess = &mut self.remote.sessions[sess_idx];
        sess.transport = transport;
        sess.state.client_id = client_id;
        sess.state.bulk_port = port;
        sess.state.op_seq = 0;
        sess.state.pending_op_focus.clear();
        sess.state.agent_requests.clear();
        sess.state.next_delta_focus = None;
        // 옛 연결의 목록 요청은 다시 응답하지 않는다. 소비자는 자체 timeout으로 실패 처리한다.
        sess.state.pending_list_dir_consumers.clear();
        // 옛 연결로 올린 캡처의 회신도 오지 않는다.
        let abandoned_captures = std::mem::take(&mut sess.state.capture_uploads);
        sess.state.resync_pending = None;
        sess.state.resync_awaiting_window = false;
        // 옛 연결의 닫기 회신은 오지 않는다. 결과를 모르는 요청은 enter_reconnecting이 이미 알렸다.
        sess.state.workspace_close = Default::default();
        sess.state.phase = SessionState::Connected;
        sess.state.remote_label = format!("127.0.0.1:{port}");
        // 원문은 바로 다시 받지 않는다. 변경 신호를 보내 플러그인이 재요청이나 stale 표시를 선택하게 한다.
        let markdown_locals: Vec<u32> = sess.state.markdown_locals.iter().copied().collect();
        for local in markdown_locals {
            push_markdown_changed(&mut self.plugin_manager, local);
        }
        tracing::info!(
            "gui attach: mirror workspace {local_workspace} 재연결 성공 (remote ws {workspace})"
        );
        self.abandon_capture_uploads(abandoned_captures);
        self.probe_remote_home(local_workspace);
        Ok(())
    }

    /// 사용자가 연결한 mirror로만 포커스를 옮긴다. IPC·자동 연결은 호출하지 않는다.
    pub(crate) fn focus_mirror_workspace(&mut self, ws_id: u32) {
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if let Some(idx) = engine
                .workspaces()
                .into_iter()
                .position(|ws| ws.id == ws_id)
            {
                main.state.set_active_workspace_index(engine.core, idx);
                main.mark_dirty();
                break;
            }
        }
    }

    /// 새 attach snapshot으로 손실 뒤 화면을 재동기화한다. 빠진 출력 이력을 복구하는 것은 아니다.
    pub(super) fn resync_session(&mut self, idx: usize) -> bool {
        let (port, tunnel, frames, local_workspace) = {
            let sess = &mut self.remote.sessions[idx];
            (
                sess.state.bulk_port,
                sess.transport.tunnel.take(),
                sess.state.resync_pending.unwrap_or(0),
                sess.state.local_workspace,
            )
        };
        match self.reconnect_session(idx, port, tunnel) {
            Ok(()) => {
                tracing::info!(
                    "gui attach: mirror workspace {local_workspace} 재동기화 준비 대기 — 프레임 {frames} 장 손실 뒤 재attach"
                );
                true
            }
            Err(e) => {
                tracing::warn!(
                    "gui attach: mirror workspace {local_workspace} 재동기화 실패 — 끊김으로 처리한다: {e}"
                );
                self.remote.sessions[idx].state.resync_pending = None;
                false
            }
        }
    }

    /// 저장된 원격 행의 세션은 연결이 끊겨도 형상을 남기고 재연결 시간 동안 다시 붙기를 시도한다.
    /// 재연결 시간이 0이면 바로 끊긴 행이 된다.
    pub(super) fn enter_reconnecting(&mut self, idx: usize) {
        let (row, local_workspace) =
            self.release_session_connection(idx, SessionState::Reconnecting);
        let window = self.reconnect_window();
        if let Some(row) = row {
            self.remote.active.remove(&row);
            self.remote
                .reconnect
                .insert(row, tasty_remote::outbound::ReconnectSlot::new());
            if window.is_zero() {
                self.enter_disconnected(row);
                return;
            }
        }
        for (_, main, engine) in self.engines_mut().window_pairs() {
            if engine
                .workspaces()
                .into_iter()
                .any(|ws| ws.id == local_workspace)
            {
                main.state.toasts.push(
                    crate::i18n::t("attach.toast.mirror_reconnecting").to_string(),
                    crate::adapters::ui::ToastKind::Warning,
                    crate::adapters::ui::ToastScope::Window,
                );
                main.mark_dirty();
                break;
            }
        }
        tracing::info!(
            "gui attach: mirror workspace {local_workspace} 재연결 대기(Reconnecting) 진입 (row {row:?})"
        );
    }

    /// 세션의 연결을 놓고 형상은 남긴다. 저장된 원격 행 ID와 로컬 workspace를 돌려준다.
    pub(super) fn release_session_connection(
        &mut self,
        idx: usize,
        phase: SessionState,
    ) -> (Option<u32>, u32) {
        let (row, local_workspace, markdown_locals, abandoned_captures) = {
            let sess = &mut self.remote.sessions[idx];
            sess.transport.html.documents.clear();
            for mirror in sess.transport.html.mirrors.values() {
                super::html::lock_mirror(mirror).disconnect();
            }
            sess.state.phase = phase;
            sess.transport.frame_tx.retire();
            // 옛 소켓을 재연결 성공까지 열어 두면 서버는 그 연결을 살아 있는 점유자로 보고 새 attach 를
            // already_attached 로 거절한다(긴 재시도 간격). 끊겼다고 본 지금 닫는다.
            sess.transport.workers.close_socket();
            (
                sess.state.saved_row,
                sess.state.local_workspace,
                sess.state.markdown_locals.clone(),
                std::mem::take(&mut sess.state.capture_uploads),
            )
        };
        self.abandon_capture_uploads(abandoned_captures);
        // 기다리던 원문 요청을 request_id=0으로 취소하고 표시 중인 원문도 끊김 상태로 바꾼다.
        for local in markdown_locals {
            push_markdown_content_result(
                &mut self.plugin_manager,
                &markdown_content_failure(local, 0, "mirror workspace disconnected"),
            );
        }
        (row, local_workspace)
    }

    /// mirror 자원을 정리하며 사용자 닫힌 항목 기록에는 넣지 않는다.
    /// from_disconnect일 때만 재활성화 대기 상태와 끊김 안내를 남긴다.
    pub(super) fn cleanup_mirror_workspace(
        &mut self,
        sess: &AttachClientSession,
        from_disconnect: bool,
    ) {
        log_mirror_cleanup(sess, from_disconnect);
        let saved_row = self.saved_remote_row_engine(sess.state.local_workspace);
        // 창과 parked engine 모두 정리해야 창 복원 때 끊긴 mirror가 되살아나지 않는다.
        // 창을 순회하는 부분의 범위 일치는 자동 검증하지 않는다.
        // window_access::mirror_workspace_engine_alive의 검사 범위 설명을 참고한다.
        let mut removed = false;
        for (_, main, mut engine) in self.engines_mut().window_pairs() {
            if remove_mirror_workspace_from_engine(
                &mut engine,
                &mut main.state,
                sess.state.local_workspace,
                &sess.state.remote_to_local,
            ) {
                // 사용자가 직접 닫은 경우에는 끊김 toast를 표시하지 않는다.
                if from_disconnect {
                    main.state.toasts.push(
                        crate::i18n::t("attach.toast.mirror_disconnected").to_string(),
                        crate::adapters::ui::ToastKind::Warning,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
                main.mark_dirty();
                removed = true;
                break;
            }
        }
        if !removed {
            // parked 상태에는 표시할 창이 없어 toast를 쌓거나 redraw를 요청하지 않는다.
            remove_mirror_workspace_from_parked(
                self.engines_mut(),
                sess.state.local_workspace,
                &sess.state.remote_to_local,
            );
        }
        // 응답을 받을 수 없어진 git-viewer 요청을 취소한다.
        if from_disconnect {
            self.notify_git_viewer_mirror_lost();
            self.abandon_capture_uploads(sess.state.capture_uploads.clone());
        }
        destroy_mirror_markdown_surfaces(
            &mut self.plugin_manager,
            sess.state.markdown_locals.clone(),
        );
        super::html::resources::destroy(&mut self.plugin_manager, sess.state.html_locals.clone());
        // 사용자 닫기도 heartbeat를 멈춰 소켓이 불필요하게 유지되지 않게 한다.
        sess.transport.disconnected.store(true, Ordering::SeqCst);
        if let Err(error) = sess.send_frame(StreamTag::Detach, Vec::new()) {
            tracing::debug!(%error, "detach notice could not reach the closing writer");
        }
        if let Some(engine) = saved_row {
            self.journal
                .admit_remote_row(engine, sess.state.local_workspace, true);
        }
        if let Some(row) = sess.state.saved_row {
            crate::app::auto_attach::forget_row(&mut self.remote, row);
        }
    }

    /// mirror workspace가 창과 parked engine 어디에도 없으면 세션도 정리한다.
    /// 창이 없다는 사실만으로 parked 세션을 고아로 판단하지 않는다.
    pub(crate) fn detach_orphaned_mirror_sessions(&mut self) {
        if self.remote.sessions.is_empty() {
            return;
        }
        let orphaned: Vec<usize> = self
            .remote
            .sessions
            .iter()
            .enumerate()
            .map(|(idx, s)| (idx, s.state.local_workspace))
            .filter(|&(_, ws)| !self.mirror_workspace_engine_alive(ws))
            .map(|(idx, _)| idx)
            .collect();
        for &idx in orphaned.iter().rev() {
            let sess = self.remote.sessions.remove(idx);
            self.cleanup_mirror_workspace(&sess, false);
        }
    }
}
/// 재연결(재동기화 포함) 설치를 마친 mirror 를 View 에 알린다. 옛 연결의 목록 요청은 응답하지
/// 않으므로 이 mirror 의 탐색기는 새 연결로 다시 읽는다.
fn finish_reconnected_view(state: &mut crate::state::MainViewState, local_workspace: u32) {
    state.toasts.push(
        crate::i18n::t("attach.toast.mirror_reconnected").to_string(),
        crate::adapters::ui::ToastKind::Success,
        crate::adapters::ui::ToastScope::Window,
    );
    state.explorer_views.reload_mirror(local_workspace);
}

/// 이 engine에 해당 workspace가 있으면 mirror 자원을 함께 정리하고 활성 인덱스를 보정한다.
fn remove_mirror_workspace_from_engine(
    engine: &mut EngineMut<'_>,
    state: &mut crate::state::MainViewState,
    local_workspace: u32,
    remote_to_local: &HashMap<u32, u32>,
) -> bool {
    let Some(_) = engine
        .workspaces()
        .into_iter()
        .position(|ws| ws.id == local_workspace)
    else {
        return false;
    };
    let _ = engine.task_scope.request_stop_workspace(local_workspace); // RunnerRegistry retains and joins the stop control.
    for &local in remote_to_local.values() {
        engine.runtime.surfaces.remove(&local);
        engine.runtime.terminals.remove(local);
        engine.forget_mirror_surface_busy(local);
        engine.forget_mirror_surface_attention(local);
        engine.forget_mirror_surface_cwd(local);
        engine.forget_mirror_surface_foreground(local);
        engine.remote.explorer_sources.remove(local);
        engine.remote.attach_mesh_frames.remove(local);
        // 로컬 닫기 정리와 같이 soft 점유 등 이 surface의 점유 기록을 지운다.
        engine.forget_closed_surface(local);
        super::agent_children::forget_mirror_child(engine, local);
    }
    engine.remote.mirror_capture_pastes.remove(&local_workspace);
    engine.remove_mirror_workspace(local_workspace);
    state.reconcile_presentation(engine);
    // mirror만 남았다면 원격 끊김 때문에 사용자 창을 닫는 대신 기본 workspace를 만든다.
    state.recreate_workspace_if_empty(&engine.read(), "mirror workspace cleanup");
    true
}

/// 출력 적용·고아 판정과 같은 parked 순회를 사용한다. 첫 항목만 확인해서는 안 된다.
fn remove_mirror_workspace_from_parked(
    mut engines: EngineScanMut<'_>,
    local_workspace: u32,
    remote_to_local: &HashMap<u32, u32>,
) -> bool {
    let parked = engines
        .reborrow()
        .parked_sessions_with_ids()
        .map(|(id, _, e)| (id, &*e.core));
    let Some(id) = find_parked_with_workspace(parked, local_workspace) else {
        return false;
    };
    let Some((state, mut engine)) = engines.parked_session(id) else {
        return false;
    };
    remove_mirror_workspace_from_engine(&mut engine, state, local_workspace, remote_to_local)
}

fn log_mirror_cleanup(sess: &AttachClientSession, from_disconnect: bool) {
    if from_disconnect {
        tracing::warn!(
            "attach mirror cleanup: local ws {} (remote ws {}, row {:?}) — 원격발 disconnect 로 정리",
            sess.state.local_workspace,
            sess.state.remote_workspace,
            sess.state.saved_row
        );
    } else {
        tracing::info!(
            "attach mirror cleanup: local ws {} (remote ws {}, row {:?}) — 사용자 close 로 정리",
            sess.state.local_workspace,
            sess.state.remote_workspace,
            sess.state.saved_row
        );
    }
}

pub(in crate::app) fn find_parked_with_workspace<'a>(
    parked: impl IntoIterator<Item = (EngineId, &'a crate::core::CoreState)>,
    local_workspace: u32,
) -> Option<EngineId> {
    parked
        .into_iter()
        .find(|(_, engine)| engine.has_workspace(local_workspace))
        .map(|(id, _)| id)
}
