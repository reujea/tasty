//! 플러그인 IPC를 검사한 뒤 호스트 또는 다른 플러그인으로 전달한다.

use serde_json::json;
use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;
use crate::ipc;

impl App {
    /// 원 process가 살아 있는 호출은 모든 실행 분기 전에 공통 게이트를 통과한다.
    /// 권한용 ensure_allowed만 따로 호출하지 않는다.
    /// 다른 플러그인의 namespace 호출은 비동기로 전달하며 그 응답을 기다린다.
    pub(crate) fn process_plugin_ipc_calls(&mut self) {
        let calls = match self.plugin_manager.as_mut() {
            Some(mgr) => mgr.take_pending_plugin_calls(),
            None => return,
        };
        for call in calls {
            if !self
                .plugin_manager
                .as_ref()
                .is_some_and(|manager| manager.plugin_call_is_current(&call))
            {
                continue;
            }
            let caller = Self::plugin_caller(&call);
            let request = Self::plugin_call_request(&call);
            // 게이트 결과를 기다리는 요청이 있으면 접수 순서를 지키도록 그 뒤에 선다.
            if !self.gate_queue.is_empty() {
                self.gate_queue.park(
                    crate::app::request_gate::Gated::Plugin {
                        call,
                        request,
                        progress: Default::default(),
                    },
                    false,
                );
                continue;
            }
            self.dispatch_plugin_call(call, caller, request, Default::default(), false);
        }
    }

    /// 게이트를 통과한 호출을 실행 분기로 보낸다. 게이트가 writer 결과를 기다리면 호출을 보관하고,
    /// 결과가 나오면 같은 함수로 다시 들어온다.
    pub(crate) fn dispatch_plugin_call(
        &mut self,
        call: PendingPluginCall,
        caller: ipc::caller::CallerContext,
        mut request: ipc::protocol::JsonRpcRequest,
        mut progress: ipc::handler::gate::GateProgress,
        resumed: bool,
    ) {
        use ipc::handler::gate::Step;
        // 기다리는 동안 plugin process가 바뀌었으면 원 호출에 답할 곳이 없다.
        if resumed
            && !self
                .plugin_manager
                .as_ref()
                .is_some_and(|manager| manager.plugin_call_is_current(&call))
        {
            return;
        }
        if let Step::Pending =
            ipc::handler::gate::prepare(&self.services, &mut request, &caller, &mut progress)
        {
            self.gate_queue.park(
                crate::app::request_gate::Gated::Plugin {
                    call,
                    request,
                    progress,
                },
                resumed,
            );
            return;
        }
        let checked = match self.gates_before_routing(&request, &caller, &mut progress) {
            Step::Ready(Ok(checked)) => checked,
            Step::Ready(Err(resp)) => {
                let (msg, code) = match resp.error {
                    Some(e) => (Some(e.message), Some(e.code)),
                    None => (None, None),
                };
                if let Some(mgr) = self.plugin_manager.as_mut() {
                    mgr.send_plugin_call_result(&call, None, msg, code);
                }
                return;
            }
            Step::Pending => {
                self.gate_queue.park(
                    crate::app::request_gate::Gated::Plugin {
                        call,
                        request,
                        progress,
                    },
                    resumed,
                );
                return;
            }
        };
        #[cfg(feature = "gui")]
        {
            let decision = self.remote_row_use(&request);
            if decision != crate::app::auto_attach::row_use::RowUse::Pass {
                let gated = crate::app::request_gate::Gated::Plugin {
                    call,
                    request,
                    progress,
                };
                self.hold_for_remote_row(gated, decision);
                return;
            }
        }
        if call.method == tasty_plugin_protocol::METHOD_HOST_SHARED_BUFFER_CREATE {
            self.handle_ipc_shared_buffer_create(&call);
            return;
        }
        if call.method == "popup.close" {
            self.handle_ipc_popup_close(&call);
            return;
        }
        if call.method == "banner.open" {
            self.handle_ipc_banner_open(&call);
            return;
        }
        if call.method == "banner.close" {
            self.handle_ipc_banner_close(&call);
            return;
        }
        if call.method == "webview.open_external" {
            self.handle_ipc_webview_open_external(&call);
            return;
        }
        // 자기 namespace 요청은 호스트 구현으로 위임할 수 있어 다른 플러그인 요청만 forward한다.
        // 호스트 변환 메서드는 소유 플러그인을 거치지 않고 아래 journal 접수로 간다.
        if let Some(mgr) = self.plugin_manager.as_mut()
            && mgr.forward_plugin_call_to_other_namespace(
                &call,
                crate::app::journal::commands::host_converts(&call.method),
            )
        {
            return;
        }
        if self.defer_plugin_preset_capture(&call) {
            return;
        }
        if self.defer_plugin_attach_into_gui(&call) {
            return;
        }
        if self
            .journal
            .admit_plugin(&request, &caller, &call, self.plugin_manager.as_ref())
        {
            return;
        }
        if self.defer_live_plugin(&checked, &call) {
            return;
        }
        self.handle_ipc_default_dispatch(&call, &checked);
    }

    /// 외부 IPC와 같은 메서드다. 메인 루프가 답하는 system.info로 대상 포트의 상대를 확인하므로
    /// engine 처리기 대신 App 층에서 판정한 뒤 답한다.
    fn defer_plugin_attach_into_gui(&mut self, call: &PendingPluginCall) -> bool {
        if call.method != "attach.into_gui" {
            return false;
        }
        self.plugin_dispatch_attach_into_gui(call);
        true
    }

    /// Gates and cross-plugin namespace routing have already selected the host fallback.
    fn defer_plugin_preset_capture(&mut self, call: &PendingPluginCall) -> bool {
        if call.method != "preset.capture" {
            return false;
        }
        if !self
            .plugin_manager
            .as_ref()
            .is_some_and(|manager| manager.plugin_call_is_current(call))
        {
            tracing::debug!(plugin=%call.plugin_id,"discarding preset capture from a retired plugin process");
            return true;
        }
        let binding = call.binding.clone();
        let id = serde_json::Value::from(call.call_id);
        let response = match crate::ipc::handler::preset::decode_capture_request(&call.params, &id)
        {
            Err(response) => Some(response),
            Ok((kind, source, name)) => {
                let owns = |engine: &crate::core::CoreState| match kind {
                    tasty_presets::PresetKind::Workspace => engine
                        .workspaces()
                        .into_iter()
                        .any(|workspace| workspace.id == source),
                    tasty_presets::PresetKind::Pane => engine.find_pane_by_id(source).is_some(),
                    tasty_presets::PresetKind::Tab => engine.find_pane_for_tab(source).is_some(),
                };
                let presentation = self
                    .engines()
                    .sessions()
                    .find(|(_, engine)| owns(engine.core))
                    .map(|(state, engine)| {
                        crate::model::StructurePresentationSnapshot::capture(
                            engine.workspaces(),
                            engine.categories(),
                            &state.navigation,
                        )
                    });
                let session = self
                    .engines
                    .all_sessions()
                    .find(|session| owns(&session.core_state));
                let result = match (session, presentation) {
                    (Some(session), Some(presentation)) => self.journal.queue_preset_capture(
                        session,
                        &presentation,
                        kind,
                        source,
                        super::super::journal::PresetCaptureReply::Plugin {
                            plugin: call.plugin_id.clone(),
                            binding: binding.clone(),
                            call_id: call.call_id,
                            name,
                        },
                    ),
                    _ => Err(format!("preset capture source {source} not found")),
                };
                result
                    .err()
                    .map(|error| ipc::protocol::JsonRpcResponse::invalid_params(id, error))
            }
        };
        if let Some(response) = response
            && let Some(manager) = self.plugin_manager.as_mut()
        {
            manager.send_bound_ipc_result(&call.plugin_id, &binding, call.call_id, response);
        }
        true
    }

    /// main·보조 채널을 함께 사용하는 공유 버퍼 생성은 매니저가 처리한다.
    fn handle_ipc_shared_buffer_create(&mut self, call: &PendingPluginCall) {
        let size = call
            .params
            .get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if let Some(mgr) = self.plugin_manager.as_mut() {
            let (result, error) =
                match mgr.create_shared_buffer_for(&call.plugin_id, call.call_id, size) {
                    Ok(r) => (serde_json::to_value(&r).ok(), None),
                    Err(e) => (None, Some(e)),
                };
            mgr.send_plugin_call_result(call, result, error, None);
        }
    }

    /// 공통 권한 검사 뒤 인스턴스 소유자를 확인한다.
    fn handle_ipc_popup_close(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            let instance_id = call.params.get("instance_id").and_then(|v| v.as_u64());
            match instance_id {
                None => (None, Some("popup.close: missing 'instance_id'".to_string())),
                Some(id) => {
                    let owns = self.plugin_manager.as_ref().map(|m| {
                        m.popup_instances()
                            .find(|(iid, _)| *iid == id)
                            .is_some_and(|(_, inst)| inst.plugin_id == call.plugin_id)
                    });
                    match owns {
                        None => (None, Some("popup.close: plugin manager unavailable".into())),
                        Some(false) => (
                            None,
                            Some(format!(
                                "popup.close: instance {id} not owned by plugin '{}'",
                                call.plugin_id
                            )),
                        ),
                        Some(true) => {
                            // 공통 닫기 큐를 거쳐 자식 파일 피커도 취소되게 한다.
                            self.enqueue_plugin_popup_close(
                                id,
                                tasty_plugin_protocol::PopupCloseReason::PluginRequest,
                            );
                            (Some(serde_json::Value::Object(Default::default())), None)
                        }
                    }
                }
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_plugin_call_result(call, result, error, None);
        }
    }

    /// 권한은 진입부 게이트, surface 소유권은 open_plugin_banner에서 검사한다.
    fn handle_ipc_banner_open(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            let banner_id = call.params.get("banner_id").and_then(|v| v.as_str());
            // 범위를 넘는 ID를 잘라 다른 surface ID로 바꾸지 않는다.
            let surface_id =
                crate::adapters::ipc::handler::params::read_u32(&call.params, "surface_id");
            match (banner_id, surface_id) {
                (Some(bid), Ok(Some(sid))) => {
                    let bid = bid.to_string();
                    match self.open_plugin_banner(Some(&call.plugin_id), &bid, sid) {
                        Ok(iid) => (Some(json!({ "instance_id": iid })), None),
                        Err(e) => (None, Some(e)),
                    }
                }
                (_, Err(msg)) => (None, Some(format!("banner.open: {msg}"))),
                _ => (
                    None,
                    Some("banner.open: missing 'banner_id' or 'surface_id'".to_string()),
                ),
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_plugin_call_result(call, result, error, None);
        }
    }

    fn handle_ipc_banner_close(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            match call.params.get("instance_id").and_then(|v| v.as_u64()) {
                None => (
                    None,
                    Some("banner.close: missing 'instance_id'".to_string()),
                ),
                Some(iid) => {
                    if !self.plugin_owns_banner(&call.plugin_id, iid) {
                        (
                            None,
                            Some(format!(
                                "banner.close: instance {iid} not owned by plugin '{}'",
                                call.plugin_id
                            )),
                        )
                    } else {
                        self.close_plugin_banner(
                            iid,
                            tasty_plugin_protocol::BannerCloseReason::PluginRequest,
                        );
                        (Some(json!({ "closed": iid })), None)
                    }
                }
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_plugin_call_result(call, result, error, None);
        }
    }

    fn handle_ipc_default_dispatch(
        &mut self,
        call: &PendingPluginCall,
        checked: &ipc::handler::CheckedRequest<'_>,
    ) {
        let response = match self.dispatch_checked(checked) {
            ipc::handler::Handled::Now(response) => response,
            ipc::handler::Handled::Later(deferred) => {
                self.services
                    .defer_reply(deferred, ipc::handler::ReplyTo::plugin(call));
                return;
            }
        };
        // 원래 오류 코드를 보존해야 인자 오류가 일반 서버 오류로 바뀌지 않는다.
        let (result, error, code) = match response.error {
            Some(err) => (None, Some(err.message), Some(err.code)),
            None => (response.result, None, None),
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_plugin_call_result(call, result, error, code);
        }
    }

    /// 사전 검사와 실제 라우팅에서 같은 caller를 사용한다.
    pub(crate) fn plugin_caller(call: &PendingPluginCall) -> ipc::caller::CallerContext {
        ipc::caller::CallerContext::Plugin {
            plugin_id: call.plugin_id.clone(),
            permissions: call.permissions.clone(),
        }
    }

    fn plugin_call_request(call: &PendingPluginCall) -> ipc::protocol::JsonRpcRequest {
        ipc::protocol::JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::from(call.call_id)),
            method: call.method.clone(),
            params: call.params.clone(),
            session_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use tasty_plugin_manifest::Permission;

    /// 시험 문자열을 실제 호출로 오인하지 않도록 test 모듈 앞까지만 읽는다.
    fn source() -> &'static str {
        let full = include_str!("plugin_ipc.rs");
        full.split("\n#[cfg(test)]").next().unwrap_or(full)
    }

    /// 실제 소스에서 검사할 호출·분기와 주석 속 이름이 모두 남아 있는지 확인한다.
    #[test]
    fn the_scanner_sees_this_file() {
        let src = source();
        assert!(
            src.contains("pub(crate) fn process_plugin_ipc_calls"),
            "진입 함수를 찾지 못했다. 이름이나 형태가 바뀌었는지 확인한다."
        );
        assert!(
            !src.contains("#[cfg(test)]"),
            "스캔 대상에 테스트 모듈이 남아 있어 시험 문자열을 실제 소스로 읽을 수 있다."
        );
        assert!(
            src.matches("if call.method ==").count() >= 4,
            "메서드 분기가 하한보다 적다: {}",
            src.matches("if call.method ==").count()
        );
        // 아래 ensure_allowed 설명은 호출 형태와 주석 속 이름을 구별하는 대조 입력이다.
        assert!(
            src.contains("ensure_allowed"),
            "ensure_allowed 설명이 없어 호출 형태와 주석 속 이름의 구별을 확인할 수 없다."
        );
    }

    /// 원문의 게이트 호출 위치가 첫 메서드 분기보다 앞서는지 확인한다.
    #[test]
    fn the_entry_gates_before_it_branches() {
        let src = source();
        let gate = src
            .find("self.gates_before_routing(")
            .expect("진입부에서 gates_before_routing 호출을 못 찾았다");
        let first_branch = src
            .find("if call.method ==")
            .expect("갈래 분기를 못 찾았다");
        assert!(
            gate < first_branch,
            "원문에서 게이트({gate})가 첫 메서드 분기({first_branch})보다 뒤에 있다."
        );
    }

    /// 다른 플러그인의 호스트 변환 요청이 소유 플러그인으로 전달되지 않는지 원문으로 확인한다.
    /// GUI App 을 시험에서 만들 수 없어 공용 전달 호출에 넘기는 인자의 형태만 검사한다.
    /// 전달 판정 자체는 매니저 시험과 헤드리스 dispatch 시험이 실제 호출로 잰다.
    #[test]
    fn a_host_conversion_skips_the_cross_plugin_forward() {
        let src = source();
        let (_, args) = src
            .split_once("mgr.forward_plugin_call_to_other_namespace(")
            .expect("플러그인 간 전달을 못 찾았다");
        let (args, _) = args.split_once(')').expect("전달 인자를 못 찾았다");
        assert!(
            args.contains("crate::app::journal::commands::host_converts(&call.method"),
            "호스트 변환이 소유 플러그인으로 전달된다: {args}"
        );
    }

    /// 분기 안에서 권한 검사만 따로 호출하는 형태가 남았는지 확인한다.
    #[test]
    fn no_branch_carries_its_own_permission_check() {
        let src = source();
        let n = src.matches(".ensure_allowed(").count();
        assert_eq!(
            n, 0,
            "분기 안의 권한 검사 호출이 {n}곳이다. 진입부의 공통 게이트를 사용하는지 확인한다."
        );
    }

    /// shared buffer도 공통 게이트를 통과한다. 면제 표지나 알려진 조기 continue 형태를 검사한다.
    #[test]
    fn the_entry_gates_every_branch_without_exemption() {
        let src = source();
        assert!(
            !src.contains("PRE_GATE_EXEMPT"),
            "면제 목록 표지가 있다. METHOD_TABLE 등록과 공통 게이트 적용 여부를 확인한다."
        );
        assert!(
            has_only_process_refusal_before_the_gate(src),
            "게이트 전에는 원 process가 끝난 호출만 거절할 수 있다"
        );
    }

    fn has_only_process_refusal_before_the_gate(src: &str) -> bool {
        let Some((_, body)) = src.split_once("for call in calls {") else {
            return false;
        };
        let Some((prefix, _)) = body.split_once("let caller =") else {
            return false;
        };
        let tokens: String = prefix.chars().filter(|ch| !ch.is_whitespace()).collect();
        tokens
            == "if!self.plugin_manager.as_ref().is_some_and(|manager|manager.plugin_call_is_current(&call)){continue;}"
    }

    #[test]
    fn method_exemptions_cannot_hide_behind_the_process_refusal() {
        let src = source();
        assert!(has_only_process_refusal_before_the_gate(src));
        let bypass = src.replacen(
            "let caller =",
            "if call.method == \"banner.open\" { continue; } let caller =",
            1,
        );
        assert!(!has_only_process_refusal_before_the_gate(&bypass));
        let unfenced = src.replacen("manager.plugin_call_is_current(&call)", "true", 1);
        assert!(!has_only_process_refusal_before_the_gate(&unfenced));
    }

    /// 헤드리스 소스에서도 게이트와 첫 인터셉트의 원문 위치를 비교한다.
    #[test]
    fn the_headless_entry_also_gates_before_it_intercepts() {
        let src = include_str!("../../boot/headless_plugins.rs");
        let src = src.split("\n#[cfg(test)]").next().unwrap_or(src);
        let body = src
            .split_once("fn dispatch_plugin_ipc_calls_headless(")
            .expect("헤드리스 진입 함수를 못 찾았다")
            .1;
        let gate = body
            .find("gates_before_intercept(")
            .expect("헤드리스 진입부에서 pre-gate 호출을 못 찾았다");
        let intercept = body
            .find("call.method ==")
            .expect("헤드리스 인터셉트를 못 찾았다");
        assert!(
            gate < intercept,
            "원문에서 게이트({gate})가 인터셉트({intercept})보다 뒤에 있다."
        );
        assert_eq!(
            body.matches("call.method ==").count(),
            1,
            "헤드리스의 메서드명 분기 수가 달라졌다. 각 분기가 게이트 뒤에 있는지 확인한다."
        );
    }

    /// 허용·거절을 모두 확인하며 실제 매니페스트 변경에 영향받지 않도록 합성 권한을 사용한다.
    #[test]
    fn the_gate_denies_without_the_token_and_allows_with_it() {
        let without = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: "test.synthetic".into(),
            permissions: HashSet::from([Permission::SurfaceRead]).into(),
        };
        let with = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: "test.synthetic".into(),
            permissions: HashSet::from([Permission::UiBanner]).into(),
        };
        assert!(
            without.ensure_allowed("banner.open").is_err(),
            "ui.banner 없이 banner.open 이 통과했다"
        );
        assert!(
            with.ensure_allowed("banner.open").is_ok(),
            "ui.banner 권한이 있는 호출도 거절됐다. 허용 입력이 통과하는지 확인한다."
        );
    }
}
