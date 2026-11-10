//! Intent 큐를 처리하고 검사된 IPC 요청을 대상 engine에 전달한다.

use crate::app::App;
use crate::app::window_access::{DispatchCtx, engines_mut};
use crate::ipc;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_session::EngineId;
use crate::view::ui::View;

enum IntentClass {
    Domain,
    Immediate,
}

impl App {
    /// 같은 state의 UI 요청을 먼저 순서대로 처리하고 Domain 요청은 뒤에 모아 처리한다.
    /// 처리 중 추가한 요청은 다음 호출로 남긴다.
    /// Domain 처리는 App 전체를 빌리므로 창별 상태를 빌린 루프 밖에서 수행한다.
    /// docs/design/flows/action-dispatch.md 참조.
    pub(crate) fn dispatch_pending_intents(&mut self) {
        let batches = self.drain_pending_batches();

        let mut domain_batch: Vec<(
            crate::app::dispatch_domain::DispatchSource,
            crate::intent::DispatchedIntent,
        )> = Vec::new();
        self.process_state_batches(batches, &mut domain_batch);

        self.run_domain_cascade(domain_batch);

        self.refresh_approval_presentations();
        #[cfg(feature = "gui")]
        self.redraw_windows_for_shared_lists();
    }

    /// A projection of pending popup records, refreshed after every source of store changes.
    pub(crate) fn refresh_approval_presentations(&mut self) {
        let store = &self.services.approval_store;
        for view in self.view.views.values_mut() {
            let Some(main) = view.as_main_mut() else {
                continue;
            };
            let dialogs = &mut main.state.dialogs;
            let mut changed = false;
            for id in &dialogs.pending_approval_ids {
                match store.get(id) {
                    Some(record) => {
                        changed |= dialogs
                            .approval_records
                            .get(id)
                            .is_none_or(|old| old.state != record.state);
                        dialogs.approval_records.insert(id.clone(), record);
                    }
                    None => {
                        changed |= dialogs.approval_records.remove(id).is_some();
                    }
                }
            }
            dialogs
                .approval_records
                .retain(|id, _| dialogs.pending_approval_ids.contains(id));
            if changed {
                main.mark_dirty();
            }
        }
    }

    /// 큐의 소유권을 옮겨 둔 뒤 창·parked 상태의 빌림을 끝내고 처리한다. 창 → parked 순서다.
    fn drain_pending_batches(&mut self) -> Vec<(EngineId, Vec<crate::intent::DispatchedIntent>)> {
        let mut windowed = Vec::new();
        for (wid, w) in self.view.views.iter_mut() {
            if let Some(main) = w.as_main_mut() {
                let batch = main.state.take_pending_intents();
                if !batch.is_empty() {
                    windowed.push((*wid, batch));
                }
            }
        }
        let mut batches: Vec<_> = windowed
            .into_iter()
            .filter_map(|(wid, batch)| self.engines.of_window(wid).map(|id| (id, batch)))
            .collect();
        for (id, s, _) in self.engines_mut().parked_sessions_with_ids() {
            let batch = s.take_pending_intents();
            if !batch.is_empty() {
                batches.push((id, batch));
            }
        }
        batches
    }

    fn process_state_batches(
        &mut self,
        batches: Vec<(EngineId, Vec<crate::intent::DispatchedIntent>)>,
        domain_batch: &mut Vec<(
            crate::app::dispatch_domain::DispatchSource,
            crate::intent::DispatchedIntent,
        )>,
    ) {
        for (id, batch) in batches {
            let core = &mut self.services;
            let Some(DispatchCtx {
                state,
                mut engine,
                view,
                ..
            }) = engines_mut!(self).resolve(id)
            else {
                continue;
            };
            for intent in batch {
                #[cfg(debug_assertions)]
                crate::intent::watch::observe(&intent);
                match Self::classify_intent(&intent) {
                    IntentClass::Domain => domain_batch.push((
                        crate::app::dispatch_domain::DispatchSource::Engine(id),
                        intent,
                    )),
                    IntentClass::Immediate => Self::dispatch_one_intent(
                        core,
                        state,
                        &mut engine,
                        &intent,
                        self.plugin_manager.as_ref(),
                        view.as_deref(),
                    ),
                }
            }
            if let Some(view) = view {
                view.mark_dirty();
            }
        }
    }

    fn classify_intent(intent: &crate::intent::DispatchedIntent) -> IntentClass {
        use crate::intent::Intent;
        if matches!(
            intent.body,
            Intent::RemoteBrowser(_)
                | Intent::AttachSizeSync(_)
                | Intent::DisconnectMirror { .. }
                | Intent::SavedRow { .. }
                | Intent::PatchSettings(_)
                | Intent::ForwardMirror { .. }
                | Intent::Domain(_)
                | Intent::DirectRename(_)
                | Intent::CommitDivider(_)
                | Intent::ApplyPreset { .. }
                | Intent::CapturePreset { .. }
                | Intent::RestoreClosedItem
                | Intent::NewWorkspace { .. }
                | Intent::PrepareTutorial { .. }
                | Intent::NewTab { .. }
                | Intent::NewTabWithFollowup { .. }
                | Intent::SplitPane { .. }
                | Intent::SplitSurface { .. }
                | Intent::ConvertSurface { .. }
        ) {
            IntentClass::Domain
        } else {
            IntentClass::Immediate
        }
    }

    fn run_domain_cascade(
        &mut self,
        domain_batch: Vec<(
            crate::app::dispatch_domain::DispatchSource,
            crate::intent::DispatchedIntent,
        )>,
    ) {
        for (source, dispatched) in domain_batch {
            let origin = dispatched.origin.clone();
            let context = match &dispatched.body {
                crate::intent::Intent::RemoteBrowser(_) => {
                    crate::intent::FailureContext::RemoteConnect
                }
                crate::intent::Intent::CommitDivider(_) => crate::intent::FailureContext::Divider,
                _ => crate::intent::FailureContext::General,
            };
            let original = self
                .engines_mut()
                .resolve(source.engine())
                .and_then(|resolved| resolved.view.map(|view| view.state.identity()));
            if let Err(error) = self.dispatch_domain_intent(source, dispatched) {
                if let Some(resolved) = self.engines_mut().resolve(source.engine()) {
                    let view = resolved.view.filter(|view| {
                        original
                            .as_ref()
                            .is_some_and(|original| view.state.matches_identity(original))
                    });
                    crate::intent::report_apply_error(
                        view.as_ref().map(|_| resolved.state),
                        &origin,
                        context,
                        &error,
                    );
                    if let Some(view) = view {
                        view.mark_dirty();
                    }
                } else {
                    crate::intent::report_apply_error(None, &origin, context, &error);
                }
            }
        }
    }

    fn dispatch_mouse_capture_hint(
        state: &mut crate::state::MainViewState,
        engine: &mut EngineMut<'_>,
        intent: &crate::intent::DispatchedIntent,
        view: Option<&crate::view::ViewBase>,
    ) {
        use crate::intent::Intent;
        let Intent::MouseCaptureHint {
            target,
            foreground_generation,
            view: origin,
        } = &intent.body
        else {
            return;
        };

        if view.is_some_and(|view| view.state.matches_identity(origin))
            && let Some(surface) = target.take_mouse_capture_hint(engine, *foreground_generation)
        {
            state.banners.push(
                crate::adapters::ui::BannerState::persistent(
                    crate::adapters::ui::banner::defs::BANNER_MOUSE_CAPTURE,
                    crate::adapters::ui::BannerScope::Surface(surface),
                )
                .with_origin_generation(*foreground_generation),
            );
        }
    }

    fn dispatch_approval_response(
        core: &mut crate::app::services::AppServices,
        state: &mut crate::state::MainViewState,
        intent: &crate::intent::DispatchedIntent,
    ) {
        use crate::intent::Intent;
        let Intent::RespondApproval {
            request_id,
            choice,
            comment,
        } = &intent.body
        else {
            return;
        };

        match core.respond_approval(
            request_id,
            choice.clone(),
            tasty_approval::Responder::User,
            comment.clone(),
        ) {
            Ok(transition) => {
                // 기록은 App이 이미 제출했다. 결과는 메인 루프가 거두므로 popup 수명과 무관하다.
                transition.record.detach(core.memory_access());
                state
                    .dialogs
                    .approval_records
                    .insert(request_id.clone(), transition.change.record);
            }
            Err(error) => tracing::warn!("approval response failed: {error}"),
        }
        state.dialogs.approval_submitting = None;
    }
    fn report_intent_dispatch_error(message: &'static str) {
        tracing::error!("{message}");
    }

    fn dispatch_one_intent(
        core: &mut crate::app::services::AppServices,
        state: &mut crate::state::MainViewState,
        engine: &mut EngineMut<'_>,
        intent: &crate::intent::DispatchedIntent,
        plugins: Option<&crate::plugin::PluginManager>,
        view: Option<&crate::view::ViewBase>,
    ) {
        use crate::intent::Intent;
        match &intent.body {
            Intent::MouseCaptureHint { .. } => {
                Self::dispatch_mouse_capture_hint(state, engine, intent, view)
            }
            Intent::Engine(action) => action.apply(engine),
            Intent::Frame(effect) => effect.apply(engine, plugins, &intent.origin),
            Intent::RespondApproval { .. } => Self::dispatch_approval_response(core, state, intent),
            Intent::Ui(_) => {
                crate::intent::popup::handle(state, engine, intent);
            }
            Intent::ApplyPreset { .. } => {
                crate::intent::preset::handle(core, state, engine, intent);
            }
            Intent::SplitSurface { .. }
            | Intent::ConvertSurface { .. }
            | Intent::NewTab { .. }
            | Intent::NewTabWithFollowup { .. }
            | Intent::SplitPane { .. }
            | Intent::NewWorkspace { .. }
            | Intent::PrepareTutorial { .. } => {
                Self::report_intent_dispatch_error("structural intent bypassed journal admission")
            }
            Intent::RestoreClosedItem => {
                Self::report_intent_dispatch_error("undo intent bypassed journal admission");
            }
            Intent::CapturePreset { .. }
            | Intent::RemoteBrowser(_)
            | Intent::AttachSizeSync(_)
            | Intent::DisconnectMirror { .. }
            | Intent::SavedRow { .. }
            | Intent::PatchSettings(_)
            | Intent::ForwardMirror { .. }
            | Intent::Domain(_)
            | Intent::DirectRename(_)
            | Intent::CommitDivider(_) => {
                Self::report_intent_dispatch_error(
                    "dispatch_one_intent reached Intent::Domain (should be handled in domain_batch)",
                );
            }
        }
    }

    /// 권한·telemetry cap·rate limit을 검사한다. 엔진 선택은 감사 저장소용이며 요청 대상은 바꾸지 않는다.
    /// 통과 객체를 같은 요청의 하위 라우팅에 전달해 검사를 다시 소비하지 않는다.
    /// writer 결과를 기다리면 `Pending`이며 호출자가 요청과 `progress`를 보관했다가 다시 부른다.
    pub(crate) fn gates_before_routing<'a>(
        &mut self,
        request: &'a ipc::protocol::JsonRpcRequest,
        caller: &'a ipc::caller::CallerContext,
        progress: &mut ipc::handler::gate::GateProgress,
    ) -> ipc::handler::gate::Step<
        Result<ipc::handler::CheckedRequest<'a>, ipc::protocol::JsonRpcResponse>,
    > {
        let core = &mut self.services;
        if let Some((state, mut engine)) = engines_mut!(self).sessions().next() {
            return ipc::handler::gate::decide(core, state, &mut engine, request, caller, progress);
        }
        ipc::handler::gate::Step::Ready(ipc::handler::check_without_engine(request, caller))
    }

    /// commit된 telemetry 기록 뒤의 cap 액션·RSS 탐지를 첫 engine 문맥으로 실행한다.
    pub(crate) fn run_telemetry_follow_ups(&mut self) {
        if self.services.telemetry_follow_ups().is_empty() {
            return;
        }
        let core = &mut self.services;
        if let Some((state, mut engine)) = engines_mut!(self).sessions().next() {
            ipc::handler::telemetry_follow_up::run(core, state, &mut engine);
            return;
        }
        ipc::handler::telemetry_follow_up::discard(core);
    }

    /// 대상을 지정했으면 소유 창·parked engine을 찾고 없으면 오류를 반환한다.
    /// 대상이 없을 때만 workspace 이름·포커스 창·첫 parked 상태로 선택한다.
    pub(crate) fn dispatch_checked(
        &mut self,
        checked: &ipc::handler::CheckedRequest<'_>,
    ) -> ipc::handler::Handled {
        use ipc::handler::Handled;
        let request = checked.request();
        if let Some(resp) = self.dispatch_list_global(request) {
            return Handled::Now(resp);
        }
        let with_open_surfaces = self.with_open_surfaces_if_needed(checked);
        let checked = with_open_surfaces.as_ref().unwrap_or(checked);
        let named =
            crate::core::request_target::request_resource_id(&request.method, &request.params);
        let target_id = match self.find_request_owner(&request.method, &request.params) {
            Ok(id) if named.is_some() => id,
            Ok(id) => id.or(self.view.focused_view_id),
            Err(msg) => {
                let id = request.id.clone().unwrap_or(serde_json::Value::Null);
                return Handled::Now(ipc::protocol::JsonRpcResponse::invalid_params(id, msg));
            }
        };
        if let Some(id) = target_id {
            let core = &mut self.services;
            let resp_opt = engines_mut!(self).window_pair(id).map(|(w, mut engine)| {
                let r =
                    ipc::handler::handle_checked_request(core, &mut w.state, &mut engine, checked);
                w.base.state.dirty = true;
                r
            });
            if let Some(response) = resp_opt {
                self.dispatch_pending_intents();
                return response;
            }
        }
        let owner_in_parked =
            named.and_then(|rid| engines_mut!(self).parked_session_with_resource(rid));
        if let Some((state, mut engine)) = owner_in_parked {
            let response = ipc::handler::handle_checked_request(
                &mut self.services,
                state,
                &mut engine,
                checked,
            );
            self.dispatch_pending_intents();
            return response;
        }
        if let Some(rid) = named {
            let id = request.id.clone().unwrap_or(serde_json::Value::Null);
            return Handled::Now(ipc::protocol::JsonRpcResponse::invalid_params(
                id,
                crate::core::request_target::unowned_target_message(rid, &request.method),
            ));
        }
        if let Some((state, mut engine)) = engines_mut!(self).first_parked_session() {
            let response = ipc::handler::handle_checked_request(
                &mut self.services,
                state,
                &mut engine,
                checked,
            );
            self.dispatch_pending_intents();
            return response;
        }
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);
        Handled::Now(ipc::protocol::JsonRpcResponse::error(
            id,
            -32000,
            "no application state available",
        ))
    }
}

#[cfg(test)]
mod tests {
    /// 두 GUI 라우팅 파일에서 사전 검사 호출·반환의 원문 위치 또는 CheckedRequest 인자를 확인한다.
    /// 실행 경로나 실제 검사·기록 횟수는 증명하지 않는다. 주석·문자열도 제거하지 않는 텍스트 검사다.
    /// CheckedRequest의 실제 소비는 handler::checked 시험에서 확인하며 헤드리스는 이 스캔에 포함하지 않는다.
    #[test]
    fn every_routing_entry_gates_before_it_answers() {
        /// 호출 이름 후보. CheckedRequest를 받은 경로는 별도로 인정한다.
        const GATES: &[&str] = &[
            "gates_before_routing",
            "check_permission_gate",
            "ensure_allowed",
        ];
        /// 호출 이름과 첫 return 사이에 허용할 원문 바이트 거리. 실행 의미를 분석하는 값은 아니다.
        const RETURN_WINDOW: usize = 120;
        let sources = [
            ("src/app/dispatch/intents.rs", include_str!("intents.rs")),
            ("src/app/ipc/routing.rs", include_str!("../ipc/routing.rs")),
        ];
        let mut checked = 0usize;
        let mut naked = Vec::new();
        for (file, full) in sources {
            // 시험 문자열을 검사 대상으로 다시 읽지 않도록 test 모듈 앞까지만 사용한다.
            let src = full.split("\n#[cfg(test)]").next().unwrap_or(full);
            let mut rest = src;
            while let Some(at) = rest
                .find("\n    pub(crate) fn ")
                .or_else(|| rest.find("\n    fn "))
            {
                let head = &rest[at + 1..];
                let name: String = head[head.find("fn ").map_or(0, |i| i + 3)..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let end = head[1..]
                    .find("\n    pub(crate) fn ")
                    .into_iter()
                    .chain(head[1..].find("\n    fn "))
                    .min()
                    .map_or(head.len(), |i| i + 1);
                let body = &head[..end];
                rest = &head[end..];

                let routes = body.contains("dispatch_list_global(")
                    || body.contains("handle_with_caller(")
                    || body.contains("handle_checked_request(")
                    || body.contains("dispatch_checked(");
                if !routes {
                    continue;
                }
                if body.contains("checked: &") && body.contains("handler::CheckedRequest") {
                    checked += 1;
                    continue;
                }
                if !body.contains("caller: &") {
                    continue;
                }
                checked += 1;
                let gate = GATES.iter().filter_map(|g| body.find(g)).min();
                let first_return = body.find("return ");
                match (gate, first_return) {
                    (None, _) => naked.push(format!("  {file}::{name} — 게이트 호출이 없다")),
                    (Some(g), Some(r)) if r < g => naked.push(format!(
                        "  {file}::{name} — 게이트보다 앞서는 조기 응답이 있다"
                    )),
                    (Some(g), Some(r)) if r - g > RETURN_WINDOW => naked.push(format!(
                        "  {file}::{name} — 게이트 결과가 곧바로 반환되지 않는다"
                    )),
                    (Some(_), None) if body.contains("Err(response) => response") => {}
                    (Some(_), None) => {
                        naked.push(format!("  {file}::{name} — 게이트 결과를 반환하지 않는다"))
                    }
                    _ => {}
                }
            }
        }
        assert!(
            checked >= 2,
            "라우팅 진입점이 {checked}개로 하한보다 적다. 함수 탐색 범위와 파서를 확인한다."
        );
        assert!(
            naked.is_empty(),
            "라우팅의 사전 검사 호출 또는 반환 형태가 예상과 다르다 ({}건):\n{}\n사전 검사를 거쳐 응답하는지 실제 분기와 파서 인식을 함께 확인한다.",
            naked.len(),
            naked.join("\n")
        );
    }

    /// 검사한 요청을 engine 핸들러에 넘기는 GUI 라우터는 첫 전달 전에 모든 engine의 열린 surface를
    /// 붙인 요청으로 바꿔야 한다. 빠지면 memory.import가 다른 창의 열린 surface 항목을 거절한다.
    /// 원문 위치만 보는 텍스트 검사이며 실제 실행 경로는 증명하지 않는다.
    #[test]
    fn every_gui_router_attaches_the_open_surfaces_before_it_hands_over() {
        const ATTACH: &str = "self.with_open_surfaces_if_needed(checked)";
        const SHADOW: &str = "let checked = with_open_surfaces.as_ref().unwrap_or(checked);";
        const HANDOVER: &str = "handle_checked_request(";
        let sources = [
            ("src/app/dispatch/intents.rs", include_str!("intents.rs")),
            ("src/app/ipc/routing.rs", include_str!("../ipc/routing.rs")),
        ];
        let mut routers = Vec::new();
        let mut missing = Vec::new();
        for (file, full) in sources {
            let src = full.split("\n#[cfg(test)]").next().unwrap_or(full);
            for body in src.split("\n    pub(crate) fn ").skip(1) {
                let body = body.split("\n    fn ").next().unwrap_or(body);
                let Some(handover) = body.find(HANDOVER) else {
                    continue;
                };
                let name: String = body
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                routers.push(format!("{file}::{name}"));
                let before = &body[..handover];
                if !(before.contains(ATTACH) && before.contains(SHADOW)) {
                    missing.push(format!("  {file}::{name}"));
                }
            }
        }
        assert!(
            routers.len() >= 2,
            "검사한 요청을 넘기는 라우터를 {}개만 찾았다: {routers:?}. 함수 탐색 범위를 확인한다.",
            routers.len()
        );
        assert!(
            missing.is_empty(),
            "첫 `{HANDOVER}` 전에 `{ATTACH}` 와 `{SHADOW}` 가 없다:\n{}",
            missing.join("\n")
        );
    }
}

#[cfg(test)]
#[path = "intents_approval_tests.rs"]
mod approval_tests;
