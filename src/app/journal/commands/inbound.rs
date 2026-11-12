//! Remote holder authorization precedes admission; key lookup precedes target resolution.
use super::*;
use tasty_ipc::stream::{
    ForwardOrigin, InheritCwd, StreamControl, StreamFrame, StreamTag, StructuralOp,
};
use tasty_ipc::stream_hub::StreamHub;

pub(crate) struct RemoteReply {
    pub engine: EngineId,
    pub ticket: u64,
    pub client: u32,
    pub op_id: u64,
    pub workspace: u32,
    pub binding: std::sync::Weak<()>,
    pub user: bool,
    pub restore: bool,
    /// 전체 workspace 닫기. 서버가 닫지 않았다는 회신(`closed:false`)은 실패로 돌려준다.
    pub closes_workspace: bool,
    pub converted: Option<u32>,
    /// 생성 op가 실은 cwd 상속 원본. 접수 뒤 대상 해석에서 확인하고, 생성 해석이 이 값으로 상속 원본을 정한다.
    pub inherit_cwd: Option<InheritCwd>,
}
/// 원격 홀더가 보낸 구조 변경 요청 하나. `PumpOutcome::structural_ops` 의 항목이
/// 그대로 이 네 값이다.
pub(crate) struct InboundOp {
    pub client: u32,
    pub op_id: u64,
    pub op: StructuralOp,
    pub origin: ForwardOrigin,
}

impl JournalApplication {
    pub(crate) fn admit_remote(
        &mut self,
        engine: EngineId,
        live: &crate::core::live::LiveDomainState,
        runtime_epoch: u64,
        hub: &StreamHub,
        inbound: InboundOp,
    ) -> Option<(u64, u32)> {
        let InboundOp {
            client,
            op_id,
            op,
            origin,
        } = inbound;
        let (registration, binding) = hub.client_identity(client)?;
        let Some(workspace) = live.occupancy.workspace_held_by(client) else {
            reply(
                hub,
                client,
                op_id,
                false,
                Some("not workspace holder".into()),
            );
            return None;
        };
        if !live.occupancy.workspace_attachment_ready(workspace) {
            reply(
                hub,
                client,
                op_id,
                false,
                Some("workspace attachment is still initializing".into()),
            );
            return None;
        }
        let ticket = self.next_ticket;
        let remote = RemoteReply {
            engine,
            ticket,
            client,
            op_id,
            workspace,
            binding,
            user: origin == ForwardOrigin::User,
            restore: matches!(op, StructuralOp::RestoreClosedItem { .. }),
            closes_workspace: matches!(op, StructuralOp::CloseWorkspace { .. }),
            converted: match &op {
                StructuralOp::ConvertSurface { surface_id, .. } => Some(*surface_id),
                _ => None,
            },
            inherit_cwd: op.inherit_cwd().cloned(),
        };
        let request = remote_request(client, runtime_epoch, registration, op_id, op, origin);
        self.admit_request(
            &request,
            Reply::Remote(remote),
            format!("remote-holder:{runtime_epoch}/{registration}"),
            "remote-structure",
        );
        self.commands
            .pending
            .contains_key(&ticket)
            .then_some((ticket, workspace))
    }
    pub(crate) fn unresolved_remote_anchor<'a>(
        &self,
        ticket: u64,
        engines: impl IntoIterator<Item = crate::runtime::engine_access::EngineRef<'a>>,
    ) -> String {
        let Some(pending) = self.commands.pending.get(&ticket) else {
            return "workspace not found".into();
        };
        let Reply::Remote(remote) = &pending.reply else {
            return "workspace not found".into();
        };
        let Ok(op) = serde_json::from_value::<StructuralOp>(pending.request.params["op"].clone())
        else {
            return "workspace not found".into();
        };
        crate::remote::structure_sync::unresolved_forward_reason(engines, remote.client, &op)
    }

    pub(crate) fn resolve_remote_request(
        &mut self,
        ticket: u64,
        session: &mut EngineSession,
        unresolved_anchor: String,
    ) {
        if !self.bind_command_engine(ticket, session.id) {
            return;
        }
        let Some(pending) = self.commands.pending.get(&ticket) else {
            return;
        };
        let Reply::Remote(remote) = &pending.reply else {
            return;
        };
        if remote.binding.upgrade().is_none()
            || session.live.occupancy.workspace_holder(remote.workspace) != Some(remote.client)
        {
            self.reject_resolved_request(
                ticket,
                JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    "remote workspace holder changed",
                ),
            );
            return;
        }
        let result = (|| -> Result<JsonRpcRequest, String> {
            let op: StructuralOp = serde_json::from_value(pending.request.params["op"].clone())
                .map_err(|error| error.to_string())?;
            let core = &session.core_state;
            let anchor = op.anchor_surface_id();
            let (index, pane) = core
                .find_workspace_index_for_surface(anchor)
                .ok_or(unresolved_anchor)?;
            if core
                .workspace_at(index)
                .is_none_or(|workspace| workspace.id != remote.workspace)
            {
                return Err("workspace not found".into());
            }
            check_inherit_cwd(core, &op, pane, remote.workspace)?;
            let mut request = pending.request.clone();
            request.idempotency_key = None;
            let merged = |params: &serde_json::Value, fields: serde_json::Value| {
                let mut value = params.as_object().cloned().unwrap_or_default();
                if let Some(fields) = fields.as_object() {
                    value.extend(fields.clone());
                }
                serde_json::Value::Object(value)
            };
            let close = |target| serde_json::json!({"target":target,"capture":remote.user,"user_close":remote.user});
            match op {
                StructuralOp::NewTab {
                    surface_kind,
                    params,
                    ..
                } => {
                    request.method = "tab.create".into();
                    request.params = merged(
                        &params,
                        serde_json::json!({"pane_id":pane,"type":surface_kind}),
                    );
                }
                StructuralOp::SplitPane {
                    direction,
                    surface_kind,
                    params,
                    ..
                } => {
                    request.method = "split".into();
                    request.params = merged(
                        &params,
                        serde_json::json!({"level":"pane","target_surface":anchor,"direction":direction.as_ipc_str(),"type":surface_kind}),
                    );
                }
                StructuralOp::SplitSurface {
                    direction,
                    surface_kind,
                    params,
                    ..
                } => {
                    request.method = "split".into();
                    request.params = merged(
                        &params,
                        serde_json::json!({"level":"surface","target_surface":anchor,"direction":direction.as_ipc_str(),"type":surface_kind}),
                    );
                }
                StructuralOp::CloseSurface { surface_id } => {
                    request.method = "intent.close".into();
                    request.params = close(tasty_core::CloseTarget::Surface(surface_id));
                }
                StructuralOp::CloseTab { .. } => {
                    request.method = "intent.close".into();
                    request.params = close(tasty_core::CloseTarget::Tab(
                        core.find_tab_for_surface(anchor)
                            .ok_or("remote tab missing")?,
                    ));
                }
                StructuralOp::ClosePane { .. } => {
                    request.method = "intent.close".into();
                    request.params = close(tasty_core::CloseTarget::Pane(pane));
                }
                StructuralOp::MoveTab {
                    from_index,
                    to_index,
                    ..
                } => {
                    request.method = "tab.move".into();
                    request.params = serde_json::json!({"pane_id":pane,"from_index":from_index,"to_index":to_index});
                }
                StructuralOp::CloseWorkspace { .. } => {
                    request.method = "intent.close".into();
                    request.params = close(tasty_core::CloseTarget::Workspace(remote.workspace));
                }
                StructuralOp::RestoreClosedItem { .. } => {
                    request.method = "intent.restore-closed".into();
                    request.params = serde_json::json!({"pane":pane,"scope":remote.workspace});
                }
                StructuralOp::ConvertSurface {
                    surface_id,
                    surface_kind,
                    params,
                    cwd,
                    inherit_cwd,
                } => {
                    if let Some(reason) = missing_markdown_file(&surface_kind, &params) {
                        return Err(reason);
                    }
                    let cwd = convert_cwd(
                        &session.as_ref(),
                        surface_id,
                        cwd.as_deref(),
                        inherit_cwd.as_ref(),
                    );
                    let spec = super::create_spec::Spec {
                        destination: super::create_spec::Destination::Convert {
                            surface: surface_id,
                            respawn: false,
                        },
                        kind: surface_kind,
                        cwd,
                        params,
                    };
                    request.method = "intent.create".into();
                    request.params =
                        serde_json::to_value(spec).map_err(|error| error.to_string())?;
                }
                StructuralOp::MoveSurface {
                    source_surface_id,
                    target_surface_id,
                } => {
                    if core
                        .find_workspace_index_for_surface(target_surface_id)
                        .and_then(|(index, _)| core.workspace_at(index))
                        .is_none_or(|workspace| workspace.id != remote.workspace)
                    {
                        return Err(tasty_utils::target::unowned_target_message(
                            "surface",
                            u64::from(target_surface_id),
                            "structural_op.move_surface",
                        ));
                    }
                    request.method = "intent.move-surface".into();
                    request.params =
                        serde_json::json!({"source":source_surface_id,"target":target_surface_id});
                }
            }
            Ok(request)
        })();
        let normalized = match result {
            Ok(request) => request,
            Err(reason) => {
                self.reject_resolved_request(
                    ticket,
                    JsonRpcResponse::invalid_params(serde_json::Value::Null, reason),
                );
                return;
            }
        };
        let method = normalized.method.clone();
        let cause = close::Cause::RemoteHolder {
            client: remote.client,
            workspace: remote.workspace,
        };
        let pending = self
            .commands
            .pending
            .get_mut(&ticket)
            .expect("remote admission remains owned");
        pending.close_cause = cause;
        // The original request/digest was already admitted. Only the execution target is normalized.
        pending.request = normalized;
        if matches!(method.as_str(), "tab.create" | "split") {
            self.resolve_public_creation(ticket, session);
        } else {
            self.resolve_ipc_for_engine(ticket, session);
        }
    }
    pub(crate) fn take_remote_results(&mut self) -> Vec<(RemoteReply, JsonRpcResponse)> {
        std::mem::take(&mut self.commands.completed_remote)
    }
}
/// 원격 kind 변환의 cwd. 명시 cwd가 먼저다. 기준이 없으면 이 서버의 설정으로 변환할 surface의 cwd를 잇고
/// (fallback), 명시한 기준은 터미널일 때만 잇는다.
pub(super) fn convert_cwd(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    surface: u32,
    cwd: Option<&str>,
    inherit_cwd: Option<&InheritCwd>,
) -> Option<std::path::PathBuf> {
    use super::create_spec::CwdSource;
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        return Some(std::path::PathBuf::from(cwd));
    }
    match CwdSource::of_remote(inherit_cwd) {
        CwdSource::View => engine
            .runtime
            .settings
            .general
            .inherit_cwd
            .then(|| engine.local_surface_cwd(surface))
            .flatten(),
        CwdSource::Surface(source) => super::create_spec::named_source_cwd(engine, source),
        CwdSource::NoInherit => None,
    }
}
/// 생성 op가 실은 cwd 상속 원본을 확인한다. 원본 surface는 이 연결이 점유한 workspace에 있고, 새 탭·pane
/// 분할은 anchor와, surface 분할·변환은 대상 surface와 같은 pane에 있어야 한다. 모르는 값도 거절한다.
/// 거절하면 아무것도 만들지 않는다. 원본이 터미널이 아니거나 cwd를 모르는 것은 거절 사유가 아니다(상속하지 않는다).
pub(super) fn check_inherit_cwd(
    core: &crate::core::CoreState,
    op: &StructuralOp,
    pane: u32,
    workspace: u32,
) -> Result<(), String> {
    match op.inherit_cwd() {
        None | Some(InheritCwd::NoInherit) => Ok(()),
        Some(unknown @ InheritCwd::Unknown(_)) => Err(format!(
            "unsupported inherit_cwd value {} in structural_op.{}",
            unknown.wire_text(),
            op.wire_kind()
        )),
        Some(InheritCwd::Surface(source)) => {
            let owned = core
                .find_workspace_index_for_surface(*source)
                .and_then(|(index, _)| core.workspace_at(index))
                .is_some_and(|held| held.id == workspace);
            if !owned {
                return Err(tasty_utils::target::unowned_target_message(
                    "surface",
                    u64::from(*source),
                    &format!("structural_op.{}.inherit_cwd", op.wire_kind()),
                ));
            }
            if core.find_pane_for_surface(*source) != Some(pane) {
                return Err(format!(
                    "inherit_cwd surface {source} is not in pane {pane} of structural_op.{}",
                    op.wire_kind()
                ));
            }
            Ok(())
        }
    }
}

/// 원격 경로의 존재는 원격이 판정한다. 로컬 `markdown.navigate` 처럼 없는 파일로는 markdown surface 를
/// 바꾸지 않고 거절 사유를 돌려준다. 바꾸면 보던 문서가 불러오지 못한 문서로 대체된다.
fn missing_markdown_file(surface_kind: &str, params: &serde_json::Value) -> Option<String> {
    let file = params.get("file")?.as_str()?;
    (surface_kind == "markdown" && !std::path::Path::new(file).exists()).then(|| {
        format!(
            "{}{file}",
            tasty_ipc::stream::STRUCTURAL_REASON_PATH_NOT_FOUND_PREFIX
        )
    })
}

pub(crate) fn reply(
    hub: &StreamHub,
    client: u32,
    op_id: u64,
    ok: bool,
    reason: Option<String>,
) -> bool {
    reply_with_created(hub, client, op_id, ok, reason, None, None)
}

fn reply_with_created(
    hub: &StreamHub,
    client: u32,
    op_id: u64,
    ok: bool,
    reason: Option<String>,
    created_surface_id: Option<u32>,
    history_omitted: Option<u32>,
) -> bool {
    let result = StreamControl::StructuralResult {
        op_id,
        ok,
        reason,
        created_surface_id,
        history_omitted,
    };
    match serde_json::to_vec(&result) {
        Ok(payload) => {
            hub.push(client, StreamFrame::new(StreamTag::Control, payload))
                == tasty_ipc::stream_hub::PushResult::Sent
        }
        Err(error) => {
            tracing::error!("remote result encoding failed: {error}");
            false
        }
    }
}

/// 이 회신으로 처음 확정된 닫기가 내용 없이 기록한 terminal 수. replay 회신은 이미 알린 결과라 싣지 않는다.
pub(super) fn history_omitted(response: &JsonRpcResponse) -> Option<u32> {
    if response.idempotent_replay {
        return None;
    }
    let omitted = response
        .result
        .as_ref()?
        .get("history_omitted")?
        .as_array()?;
    u32::try_from(omitted.len()).ok().filter(|count| *count > 0)
}

pub(crate) fn deliver_result(
    remote: RemoteReply,
    response: JsonRpcResponse,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    plugins: Option<&mut crate::plugin::PluginManager>,
    hub: &StreamHub,
) {
    engine
        .remote
        .pending_structure_replies
        .remove(&remote.ticket);
    if !hub.matches_client_binding(remote.client, &remote.binding) {
        return;
    }
    if let Some(error) = response.error {
        reply(hub, remote.client, remote.op_id, false, Some(error.message));
        return;
    }
    if remote.restore
        && response.result.as_ref().is_some_and(|value| {
            value.get("restored").and_then(|value| value.as_bool()) == Some(false)
        })
    {
        reply(
            hub,
            remote.client,
            remote.op_id,
            false,
            Some(tasty_ipc::stream::STRUCTURAL_REASON_RESTORE_EMPTY.into()),
        );
        return;
    }
    if remote.closes_workspace
        && response.result.as_ref().is_some_and(|value| {
            value.get("closed").and_then(|value| value.as_bool()) == Some(false)
        })
    {
        reply(
            hub,
            remote.client,
            remote.op_id,
            false,
            Some("workspace was not closed".into()),
        );
        return;
    }
    let created = response.result.as_ref().and_then(|result| {
        result
            .get("new_surface_id")
            .or_else(|| result.get("surface_id"))
            .and_then(|value| value.as_u64())
            .and_then(|id| u32::try_from(id).ok())
    });
    if !reply_with_created(
        hub,
        remote.client,
        remote.op_id,
        true,
        None,
        created,
        history_omitted(&response),
    ) {
        hub.unregister(remote.client);
        return;
    }
    if response.idempotent_replay {
        return;
    }
    let Some(index) = engine.find_workspace_index_for_id(remote.workspace) else {
        engine.force_detach_workspace(remote.workspace);
        return;
    };
    if engine.live.occupancy.workspace_holder(remote.workspace) != Some(remote.client) {
        return;
    }
    engine.remote.clear_structure_changed(remote.workspace);
    let class = engine.classify_attach_surfaces(remote.workspace);
    let (tree, surfaces) = engine.build_workspace_tree_surfaces(index, &class);
    let snapshot = serde_json::to_vec(&(&tree, &surfaces)).unwrap_or_default();
    let delta = StreamControl::StructuralDelta {
        workspace_id: remote.workspace,
        tree,
        surfaces,
    };
    match serde_json::to_vec(&delta) {
        Ok(bytes) => match hub.push(remote.client, StreamFrame::new(StreamTag::Control, bytes)) {
            tasty_ipc::stream_hub::PushResult::Sent => {
                engine.flush_committed_workspace_taps(remote.workspace, remote.client, hub);
                engine
                    .remote
                    .record_structure_sent(remote.workspace, remote.client, snapshot);
            }
            tasty_ipc::stream_hub::PushResult::Dropped => {
                engine.remote.mark_structure_changed(remote.workspace)
            }
            _ => engine
                .remote
                .pending_workspace_taps
                .retain(|_, value| value.0 != remote.workspace),
        },
        Err(error) => {
            tracing::error!("remote committed delta encoding failed: {error}");
            engine.remote.mark_structure_changed(remote.workspace);
        }
    }
    if let Some(surface) = remote.converted
        && let Some(plugins) = plugins
    {
        plugins.drop_egui_mesh_frame(surface);
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn deliver_remote_journal_results(&mut self) {
        for (remote, response) in self.journal.take_remote_results() {
            if let Some(session) = self.engines.session_mut(remote.engine) {
                deliver_result(
                    remote,
                    response,
                    &mut session.borrow_mut(),
                    self.plugin_manager.as_mut(),
                    &self.stream_hub,
                );
            }
        }
    }
}

// Correlation is internal durable metadata, not the StreamControl wire or an idempotency key.
// Registration spans all 128 bits; serde_json::Value numbers cannot represent that range.
fn remote_request(
    client: u32,
    runtime_epoch: u64,
    registration: u128,
    op_id: u64,
    op: StructuralOp,
    origin: ForwardOrigin,
) -> JsonRpcRequest {
    JsonRpcRequest {
        caller_agent_id: None,
        jsonrpc: "2.0".into(),
        method: "remote.structural".into(),
        params: serde_json::json!({"op":op,"origin":origin,"correlation":{
            "client":client,"runtime_epoch":runtime_epoch,"registration":registration.to_string(),"op_id":op_id
        }}),
        id: None,
        session_token: None,
        response_timeout_ms: None,
        // Remote op_id correlates Result/Delta; it is not an explicit retry key.
        idempotency_key: None,
    }
}

#[cfg(test)]
mod correlation_tests {
    use super::*;

    #[test]
    fn split_correlation_preserves_full_registration_in_durable_request_bytes() {
        let op = StructuralOp::SplitSurface {
            surface_id: 7,
            direction: tasty_ipc::stream::SplitAxis::Horizontal,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: None,
        };
        let request = remote_request(3, u64::MAX, u128::MAX, 1, op.clone(), ForwardOrigin::User);
        let stored: serde_json::Value = serde_json::from_slice(&request_digest(&request)).unwrap();
        assert_eq!(stored[0], "remote.structural");
        let params = &stored[1];
        assert_eq!(params["correlation"]["registration"], u128::MAX.to_string());
        assert_eq!(params["correlation"]["runtime_epoch"], u64::MAX);
        assert_eq!(params["correlation"]["client"], 3);
        assert_eq!(params["correlation"]["op_id"], 1);
        assert_eq!(
            serde_json::from_value::<StructuralOp>(params["op"].clone()).unwrap(),
            op
        );
        assert!(request.idempotency_key.is_none());
        let successor = remote_request(3, u64::MAX, u128::MAX - 1, 1, op, ForwardOrigin::User);
        assert_ne!(request_digest(&request), request_digest(&successor));
    }

    /// 원격에 없는 파일로 바꾸는 markdown 변환만 거절한다. 있는 파일, 파일 없는 변환, 다른 kind 는 통과한다.
    #[test]
    fn markdown_conversion_to_a_missing_remote_file_is_refused() {
        let missing = "/definitely/not/on/this/server.md";
        let reason = missing_markdown_file("markdown", &serde_json::json!({ "file": missing }))
            .expect("missing markdown file must be refused");
        assert_eq!(reason, format!("path not found: {missing}"));
        let here = env!("CARGO_MANIFEST_DIR");
        assert_eq!(
            missing_markdown_file("markdown", &serde_json::json!({ "file": here })),
            None
        );
        assert_eq!(
            missing_markdown_file("markdown", &serde_json::json!({})),
            None
        );
        assert_eq!(
            missing_markdown_file("image", &serde_json::json!({ "file": missing })),
            None
        );
    }
}
