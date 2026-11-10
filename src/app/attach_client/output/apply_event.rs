//! Dispatch ordered mirror events while retaining the session and engine borrows.
use super::*;

pub(super) fn apply_one_mirror_event(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    ev: MirrorEvent,
) {
    match ev {
        MirrorEvent::Html(reply) => super::super::html::receive(sess, reply),
        MirrorEvent::Http(reply) => {
            #[cfg(all(
                feature = "gui",
                any(target_os = "linux", target_os = "macos", windows)
            ))]
            super::super::html::http::receive(sess, host.engine, reply);
            #[cfg(not(all(
                feature = "gui",
                any(target_os = "linux", target_os = "macos", windows)
            )))]
            sess.transport.html.http_documents.receive(reply);
        }
        MirrorEvent::Desynced { frames } => {
            sess.transport.html.documents.clear();
            sess.transport
                .html
                .http_documents
                .close_all(&sess.transport.frame_tx);
            sess.state.resize_sync.forget_pending();
            begin_resync(sess, host, frames);
        }
        MirrorEvent::Data(remote_id, bytes) => {
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id)
                && let Some(t) = host.engine.runtime.terminals.get_mut(local)
            {
                t.feed_bytes(&bytes);
            }
        }
        MirrorEvent::ResizeRejected(remote_id, cols, rows) => {
            sess.state.resize_sync.on_rejected(remote_id, cols, rows);
        }
        MirrorEvent::Resize(remote_id, cols, rows) => {
            sess.state.resize_sync.on_resize(remote_id, cols, rows);
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id)
                && let Some(t) = host.engine.runtime.terminals.get_mut(local)
            {
                t.resize(cols, rows);
            }
        }
        MirrorEvent::Activity(remote_id, busy) => {
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id) {
                host.engine.set_mirror_surface_busy(local, busy);
            }
        }
        MirrorEvent::Cwd(remote_id, cwd) => {
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id) {
                host.engine.set_mirror_surface_cwd(local, cwd);
            }
        }
        MirrorEvent::Foreground(remote_id, name) => {
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id) {
                host.engine.set_mirror_surface_foreground(local, name);
            }
        }
        MirrorEvent::Attention(remote_id, kind) => {
            // 서버 상태를 반영할 때 로컬 해제 요청을 다시 forward하지 않는다.
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id) {
                host.engine.set_mirror_surface_attention(
                    local,
                    kind.map(crate::core::AttentionKind::from_wire),
                );
            }
        }
        MirrorEvent::StructuralFailed(op_id, reason) => {
            sess.state.workspace_close.settle(op_id, false);
            let epoch = sess.transport.frame_tx.epoch();
            user_waits::structural_failed(&host.state.attach_pending, &epoch, op_id, &reason);
            agent_origin::apply_structural_failed(
                sess,
                |message, kind| host.toast(message, kind),
                op_id,
                reason,
            );
        }
        MirrorEvent::StructuralSucceeded(op_id, created, history_omitted) => {
            // 서버가 workspace를 닫았다고 회신한 연결만 닫힘으로 표시한다. 정리는 이벤트 적용 뒤 출력 루프가 한다.
            sess.state.workspace_close.settle(op_id, true);
            // 결과 정산이 receipt를 바꾸기 전에 원 요청 View를 확인한다.
            if history_omitted > 0
                && let Some(view) = user_waits::unanswered_request_view(
                    &host.state.attach_pending,
                    &sess.transport.frame_tx.epoch(),
                    op_id,
                )
            {
                host.toast_for(
                    &view,
                    crate::app::journal::notices::close_history_omitted_body(history_omitted),
                    crate::adapters::ui::ToastKind::Warning,
                );
            }
            let focus = user_waits::structural_succeeded(
                host.state,
                &sess.transport.frame_tx.epoch(),
                (op_id, created),
                host.view.as_ref(),
                &sess.state.remote_to_local,
            );
            host.focus_surface(sess.state.local_workspace, focus);
            // 성공한 요청의 포커스 의도는 다음 delta가 한 번 소비한다.
            sess.state.agent_requests.forget_structural(op_id);
            if let Some(intent) = sess.state.pending_op_focus.remove(&op_id)
                && host.state.attach_pending.permits_legacy_focus(
                    &sess.transport.frame_tx.epoch(),
                    op_id,
                    &host.state.navigation,
                    host.view.as_ref(),
                )
            {
                sess.state.next_delta_focus = Some(intent);
            }
        }
        MirrorEvent::StructuralDelta {
            workspace_id,
            tree,
            surfaces,
        } => {
            let new_remote =
                user_waits::unmapped_remote_ids(&surfaces, &sess.state.remote_to_local);
            let pending_focus = sess.state.next_delta_focus.take();
            let removed_markdown = apply_mirror_structural_delta(
                &mut host.state.navigation,
                sess,
                host.engine,
                workspace_id,
                &tree,
                &surfaces,
                pending_focus,
            );
            match removed_markdown {
                Ok(removed) => {
                    let focus = user_waits::structural_applied(
                        host.state,
                        host.engine.core,
                        &sess.transport.frame_tx.epoch(),
                        host.view.as_ref(),
                        (&sess.state.remote_to_local, &new_remote),
                    );
                    host.focus_surface(sess.state.local_workspace, focus);
                    host.state.reconcile_presentation(host.engine);
                    destroy_mirror_markdown_surfaces(plugin_manager, removed.markdown);
                    super::super::html::resources::destroy(plugin_manager, removed.html);
                }
                Err(error) => {
                    tracing::warn!(
                        "mirror delta could not obtain its fixed identity reservation: {error}"
                    );
                    begin_resync(sess, host, 1);
                }
            }
        }
        MirrorEvent::ExplorerOp(value) => sess.transport.explorer_inbox.push(value),
        MirrorEvent::AgentChild(msg) => {
            crate::remote::agent_children::client_receive(sess.state.local_workspace, msg)
        }
        MirrorEvent::TerminalRead {
            request_id,
            outcome,
        } => crate::remote::terminal_read::client::receive(
            &sess.transport.frame_tx.epoch(),
            request_id,
            outcome,
        ),
        MirrorEvent::CaptureResult {
            upload_id,
            ok,
            path,
            reason,
            image,
            image_reason,
        } => apply_capture_result(
            sess,
            host,
            upload_id,
            MirrorCaptureResult {
                ok,
                path,
                reason,
                image,
                image_reason,
            },
        ),
        ev @ MirrorEvent::ListDirResult { .. } => list_dir::apply(sess, host, ev),
        MirrorEvent::GitQueryResult {
            request_id,
            ok,
            kind,
            data,
            truncated,
            reason,
        } => {
            host.state.attach_pending.query_result(
                &sess.transport.frame_tx.epoch(),
                request_id,
                true,
            );
            apply_git_query_result_event(
                plugin_manager,
                host.state,
                MirrorResultEnvelope {
                    request_id,
                    ok,
                    truncated,
                    reason,
                },
                kind,
                data,
            );
        }
        MirrorEvent::MarkdownContentResult {
            request_id,
            surface_id,
            ok,
            file,
            source,
            truncated,
            reason,
        } => {
            host.state.attach_pending.query_result(
                &sess.transport.frame_tx.epoch(),
                request_id,
                false,
            );
            apply_markdown_content_result_event(
                sess,
                host,
                plugin_manager,
                request_id,
                surface_id,
                ok,
                file,
                source,
                truncated,
                reason,
            );
        }
        MirrorEvent::MarkdownChanged { surface_id } => {
            #[cfg(all(
                feature = "gui",
                any(target_os = "linux", target_os = "macos", windows)
            ))]
            super::super::html::http::source_changed(sess, host.engine, surface_id);
            // 이 세션이 mirror하지 않는 문서의 신호는 무시한다.
            if let Some(local) = markdown_mirror_local(sess, surface_id) {
                push_markdown_changed(plugin_manager, local);
            }
        }
        MirrorEvent::Mesh(remote_id, generation, frame_seq, full, bytes) => {
            if let Some(&local) = sess.state.remote_to_local.get(&remote_id) {
                host.engine
                    .remote
                    .attach_mesh_frames
                    .update(local, bytes, generation, frame_seq, full);
            }
        }
    }
}
