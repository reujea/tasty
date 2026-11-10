//! attach 스트림 요청을 헤드리스의 단일 engine에 적용한다.
//! attach 전에 플러그인을 준비하고 끊긴 client를 표시해 같은 배치의 재attach를 허용한다.
//! 입력을 먼저 적용할 수 있도록 점유 해제·연결별 정리는 마지막에 수행한다.

#![cfg(not(feature = "gui"))]

use crate::app::App;
use crate::runtime::engine_access::EngineMut;
use crate::state::RequestContext;
use tasty_ipc::stream_hub::{PumpOutcome, StreamClientId, StreamHub};

pub(crate) fn handle_stream_ready(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
) {
    // 채널을 읽기 전에 표시를 내려야 그 뒤에 들어온 메시지가 새 깨움을 만든다.
    app.stream_wake.open_round();
    let (mut outcome, cut) = app.stream_hub.pump_inbound_budgeted(
        &app.stream_inbound_rx,
        tasty_ipc::stream_hub::PUMP_MESSAGE_BUDGET,
    );
    apply(app, state, engine, &mut outcome);
    // 예산 때문에 남긴 메시지는 채널 끝에 다시 알려 다른 깨움이 먼저 처리되게 한다.
    if cut {
        app.stream_wake.notify();
    }
}

fn apply(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    outcome: &mut PumpOutcome,
) {
    if !outcome.attach_requests.is_empty() || !outcome.workspace_attach_requests.is_empty() {
        super::headless_plugins::ensure_plugin_manager(app, &engine.as_ref());
    }
    engine.refresh_attach_presentation(&state.navigation);
    engine
        .live
        .occupancy
        .mark_clients_disconnected(&outcome.disconnected);
    apply_attach_requests(app, state, engine, outcome);
    apply_input_events(app, engine, outcome);
    apply_structural_ops(app, state, engine, outcome);
    apply_mirror_state(&app.stream_hub, engine, outcome);
    apply_mesh_requests(app, engine, outcome);
    apply_capture_uploads(app, engine, outcome);
    apply_file_requests(app, engine, outcome);
    apply_explorer_ops(app, engine, outcome);
    apply_agent_children(app, engine, outcome);
    apply_bulk_events(app, engine, outcome);
    apply_disconnects(engine, outcome);
    // 직접 구조 op 외의 변경도 점유 client에 전달한다.
    state.reconcile_presentation(engine);
    engine.refresh_attach_presentation(&state.navigation);
    engine.push_structure_changes();
}

fn apply_attach_requests(
    app: &mut App,
    state: &RequestContext,
    engine: &mut EngineMut<'_>,
    outcome: &mut PumpOutcome,
) {
    for (client_id, surface_id) in std::mem::take(&mut outcome.attach_requests) {
        if let Some(id) = state.engine_id {
            crate::app::attach_activation::begin(
                &mut app.pending_server_attaches,
                &app.journal,
                id,
                engine,
                crate::app::attach_activation::Target::Surface(surface_id),
                client_id,
                &app.stream_hub,
            );
        }
    }
    for (client_id, workspace_id) in std::mem::take(&mut outcome.workspace_attach_requests) {
        if let Some(id) = state.engine_id {
            crate::app::attach_activation::begin(
                &mut app.pending_server_attaches,
                &app.journal,
                id,
                engine,
                crate::app::attach_activation::Target::Workspace(workspace_id),
                client_id,
                &app.stream_hub,
            );
        }
    }
}

fn apply_input_events(app: &mut App, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    use tasty_ipc::stream_hub::InputEvent;
    for (client, event) in std::mem::take(&mut outcome.input_events) {
        match event {
            InputEvent::Data(bytes) => apply_input_frames(app, engine, [(client, bytes)]),
            InputEvent::Mesh { surface_id, input } => {
                if !engine.apply_attached_mesh_input(surface_id, client, input) {
                    push_mesh_error(app, client, surface_id);
                }
            }
            InputEvent::Context(context) => {
                if !engine.apply_attached_mesh_context(
                    context.surface_id,
                    client,
                    context.width_px,
                    context.height_px,
                    context.pixels_per_point,
                    context.theme,
                    context.focused,
                ) {
                    push_mesh_error(app, client, context.surface_id);
                }
            }
            InputEvent::Checkpoint(sequence) => app.stream_hub.acknowledge_input(client, sequence),
        }
    }
}

fn apply_input_frames(
    #[cfg_attr(
        not(debug_assertions),
        expect(
            unused_variables,
            reason = "only the debug echo of unrouted input writes to the stream hub"
        )
    )]
    app: &mut App,
    engine: &mut EngineMut<'_>,
    frames: impl IntoIterator<Item = (u32, Vec<u8>)>,
) {
    for (client_id, bytes) in frames {
        if app
            .pending_server_attaches
            .iter()
            .any(|pending| pending.client() == client_id)
        {
            crate::remote::server::reject_attach(&app.stream_hub, client_id, "not_ready", None);
            for pending in &mut app.pending_server_attaches {
                if pending.client() == client_id {
                    pending.cancel();
                }
            }
            app.journal.wake_application();
            continue;
        }
        // workspace 입력은 surface ID로 나누고 단일 surface 입력은 그대로 전달한다.
        let routed = if engine.live.occupancy.client_holds_workspace(client_id) {
            match crate::ipc::stream::decode_mux(&bytes) {
                Some((sid, payload)) => {
                    engine.feed_attached_workspace_input(client_id, sid, payload)
                }
                None => false,
            }
        } else {
            engine.feed_attached_input(client_id, &bytes)
        };
        #[cfg(debug_assertions)]
        if !routed {
            let echo_frame =
                crate::ipc::stream::StreamFrame::new(crate::ipc::stream::StreamTag::Data, bytes);
            let _ = app.stream_hub.push(client_id, echo_frame); // 연결 종료·손실은 허브가 처리하며 echo는 재시도하지 않는다.
        }
        #[cfg(not(debug_assertions))]
        let _ = routed; // release에는 echo 분기가 없어 라우팅 여부를 사용하지 않는다.
    }
}

fn apply_structural_ops(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    outcome: &mut PumpOutcome,
) {
    let Some(id) = state.engine_id else {
        return;
    };
    let Some(epoch) = app.journal.runtime_epoch() else {
        return;
    };
    for (client, op_id, op, origin) in std::mem::take(&mut outcome.structural_ops) {
        if let Some((ticket, workspace)) = app.journal.admit_remote(
            id,
            engine.live,
            epoch,
            &app.stream_hub,
            crate::app::journal::commands::inbound::InboundOp {
                client,
                op_id,
                op,
                origin,
            },
        ) {
            engine
                .remote
                .pending_structure_replies
                .insert(ticket, workspace);
        }
    }
}

fn apply_mirror_state(hub: &StreamHub, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client_id, remote_surface_id) in std::mem::take(&mut outcome.attention_clear_requests) {
        // attention 해제는 다음 상태 diff에서 mirror로 전달한다.
        engine.apply_attached_attention_clear(client_id, remote_surface_id);
    }
    for (client_id, remote_surface_id, cols, rows) in std::mem::take(&mut outcome.resize_requests) {
        // 적용·같은 크기는 PTY resize tap이 응답한다. 헤드리스 engine은 하나라 받지 않으면 거절이다.
        let outcome =
            engine.apply_attached_workspace_resize(client_id, remote_surface_id, cols, rows);
        crate::remote::server::finish_resize_request(
            hub,
            client_id,
            remote_surface_id,
            cols,
            rows,
            Some(outcome),
        );
    }
}

fn apply_mesh_requests(app: &mut App, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client_id, surface_id) in std::mem::take(&mut outcome.mesh_full_resend_requests) {
        let ok = engine.apply_attached_mesh_full_resend(surface_id, client_id);
        if !ok {
            push_mesh_error(app, client_id, surface_id);
        }
    }
}

fn push_mesh_error(app: &App, client_id: StreamClientId, surface_id: u32) {
    let reply = crate::ipc::stream::StreamControl::MeshError {
        surface_id,
        reason: "not_attached".to_string(),
    };
    let frame = crate::ipc::stream::StreamFrame::new(
        crate::ipc::stream::StreamTag::Control,
        serde_json::to_vec(&reply).unwrap_or_default(),
    );
    let _ = app.stream_hub.push(client_id, frame); // 연결 종료·손실은 허브가 처리하며 오류 응답은 재시도하지 않는다.
}

fn apply_capture_uploads(app: &mut App, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client_id, msg) in std::mem::take(&mut outcome.capture_uploads) {
        use tasty_ipc::stream_hub::CaptureUploadMsg;
        match msg {
            CaptureUploadMsg::CaptureChunk {
                upload_id,
                data_b64,
                ..
            } => {
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(&data_b64) {
                    Ok(bytes) if engine.live.occupancy.client_holds_workspace(client_id) => {
                        crate::remote::server::append_capture_upload(
                            engine,
                            &app.stream_hub,
                            client_id,
                            upload_id,
                            &bytes,
                            std::time::Instant::now(),
                        );
                    }
                    Ok(_) => tracing::warn!(
                        "capture upload: client {client_id} does not hold a workspace — dropping chunk"
                    ),
                    Err(_) => tracing::warn!(
                        "capture upload: invalid base64 chunk (client {client_id}, upload {upload_id})"
                    ),
                }
            }
            CaptureUploadMsg::CaptureCommit {
                upload_id,
                file_name,
            } => {
                crate::remote::server::finalize_capture_upload(
                    engine,
                    &app.services,
                    &app.stream_hub,
                    client_id,
                    upload_id,
                    &file_name,
                );
            }
        }
    }
}

fn apply_file_requests(app: &mut App, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client, msg) in outcome.html_requests.drain(..) {
        engine.handle_html_control(&app.stream_hub, client, msg);
    }
    for (client_id, msg) in std::mem::take(&mut outcome.list_dir_requests) {
        use tasty_ipc::stream_hub::ListDirRequestMsg;
        let ListDirRequestMsg::ListDirRequest { request_id, dir } = msg;
        crate::remote::server::handle_list_dir_request(
            engine,
            &app.stream_hub,
            client_id,
            request_id,
            &dir,
        );
    }
    for (client_id, msg) in std::mem::take(&mut outcome.terminal_reads) {
        use crate::remote::terminal_read as reads;
        if !reads::handle(engine, &app.stream_hub, client_id, &msg) {
            reads::reply_missing(&app.stream_hub, client_id, &msg);
        }
    }
    for (client_id, msg) in std::mem::take(&mut outcome.file_downloads) {
        let holds = engine.live.occupancy.client_holds_workspace(client_id);
        crate::remote::file_download::handle(&app.stream_hub, client_id, holds, &msg);
    }
    for (client_id, msg) in std::mem::take(&mut outcome.git_query_requests) {
        use tasty_ipc::stream_hub::GitQueryRequestMsg;
        let GitQueryRequestMsg::GitQueryRequest {
            request_id,
            surface_id,
            kind,
            worktree_path,
            diff_path,
        } = msg;
        crate::remote::server::handle_git_query_request(
            engine,
            &app.stream_hub,
            client_id,
            request_id,
            surface_id,
            kind,
            worktree_path,
            diff_path,
        );
    }
    for (client_id, msg) in std::mem::take(&mut outcome.markdown_content_requests) {
        use tasty_ipc::stream_hub::MarkdownContentRequestMsg;
        let MarkdownContentRequestMsg::MarkdownContentRequest {
            request_id,
            surface_id,
        } = msg;
        crate::remote::server::handle_markdown_content_request(
            engine,
            &app.stream_hub,
            client_id,
            request_id,
            surface_id,
        );
    }
}

/// begin·chunk·commit의 도착 순서를 유지한다. workspace는 연결의 bulk 태그에서 찾는다.
fn apply_bulk_events(app: &mut App, engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client_id, event) in std::mem::take(&mut outcome.bulk_events) {
        use tasty_ipc::stream_hub::BulkEvent;
        let Some(ws) = app.stream_hub.bulk_workspace(client_id) else {
            tracing::warn!("bulk transfer: event from non-bulk client {client_id} — ignoring");
            continue;
        };
        match event {
            BulkEvent::Begin {
                transfer_id,
                filename,
                total_size,
            } => {
                crate::remote::server::begin_bulk_transfer(
                    engine,
                    &app.stream_hub,
                    client_id,
                    transfer_id,
                    filename,
                    total_size,
                );
            }
            BulkEvent::Chunk {
                transfer_id,
                seq,
                bytes,
            } => {
                if !crate::remote::server::append_bulk_transfer(
                    engine,
                    &app.stream_hub,
                    client_id,
                    transfer_id,
                    seq,
                    &bytes,
                ) {
                    tracing::warn!(
                        "bulk transfer: chunk for unknown transfer (client {client_id}, transfer {transfer_id}) — no begin? dropping"
                    );
                }
            }
            BulkEvent::Commit { transfer_id } => {
                // 용량 사전판정과 같은 저장 폴더를 사용한다.
                let dir =
                    crate::remote::server::resolve_bulk_transfer_dir(&engine.runtime.settings);
                crate::remote::server::finalize_bulk_transfer(
                    engine,
                    &app.stream_hub,
                    client_id,
                    transfer_id,
                    ws,
                    dir,
                );
            }
        }
    }
}

/// 원격 explorer 파일 작업. 점유 확인은 디렉터리 조회와 같다. 끊긴 연결의 작업은 취소한다.
fn apply_explorer_ops(app: &App, engine: &EngineMut<'_>, outcome: &mut PumpOutcome) {
    for (client_id, msg) in std::mem::take(&mut outcome.explorer_ops) {
        let holder = engine.live.occupancy.client_holds_workspace(client_id);
        app.remote_file_jobs
            .handle(&app.stream_hub, client_id, holder, msg);
    }
    for &client_id in &outcome.disconnected {
        app.remote_file_jobs.forget_client(client_id);
    }
}

/// 원격 자식 에이전트 요청·확인. plugin 응답과 통지 송신은 1초 tick 이 맡는다.
fn apply_agent_children(app: &mut App, engine: &EngineMut<'_>, outcome: &mut PumpOutcome) {
    for &client_id in &outcome.disconnected {
        crate::remote::agent_children::disconnect(client_id);
    }
    let agent = std::mem::take(&mut outcome.agent_children);
    if !agent.is_empty() {
        crate::remote::agent_children::handle(
            &app.stream_hub,
            app.plugin_manager.as_mut(),
            &[engine.as_ref()],
            agent,
        );
    }
}

fn apply_disconnects(engine: &mut EngineMut<'_>, outcome: &mut PumpOutcome) {
    for client_id in std::mem::take(&mut outcome.disconnected) {
        engine.live.occupancy.release_all_for_client(client_id);
        engine.remote.forget_client_structures(client_id);
        engine.remote.bulk_transfers.clear_client(client_id);
        engine.remote.capture_uploads.clear_client(client_id);
        engine.remote.mesh_mirror.remove_for_client(client_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_ipc::stream::StreamControl;

    /// 헤드리스 루프가 받지 않은 크기 요청에 거절을 회신하고, 받은 요청은 tap 몫으로 남기는지.
    #[test]
    fn the_headless_loop_rejects_only_requests_its_engine_did_not_take() {
        let (_, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let hub = StreamHub::new();
        let client = hub.alloc_id();
        let rx = hub.register(client);
        let ws = engine.workspace_at(0).expect("workspace index is valid");
        let (ws_id, members) = (ws.id, ws.all_surface_ids());
        engine
            .live
            .occupancy
            .acquire_workspace(ws_id, &members, &members, client)
            .expect("workspace is free in the fixture");
        let sid = *members
            .iter()
            .find(|id| engine.runtime.terminals.contains(**id))
            .expect("fixture workspace has a terminal");
        let terminal = engine.runtime.terminals.get(sid).expect("terminal");
        let (cols, rows) = (terminal.cols(), terminal.rows());

        let mut outcome = PumpOutcome::default();
        outcome.resize_requests.push((client, sid, cols, rows));
        outcome.resize_requests.push((client, u32::MAX, 80, 24));
        apply_mirror_state(&hub, &mut engine, &mut outcome);

        let reply: StreamControl =
            serde_json::from_slice(&rx.try_recv().expect("rejection reply").payload)
                .expect("control payload");
        assert_eq!(
            reply,
            StreamControl::ResizeRejected {
                surface_id: u32::MAX,
                cols: 80,
                rows: 24
            }
        );
        assert!(
            rx.try_recv().is_err(),
            "받은 요청에는 루프가 회신하지 않는다(tap이 Resize로 응답한다)"
        );
    }
}
