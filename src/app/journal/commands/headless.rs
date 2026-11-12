//! headless 요청의 소유 엔진 확인과 완료 응답.

use super::*;

impl JournalApplication {
    #[cfg(not(feature = "gui"))]
    pub(super) fn reject_headless_owner(
        &mut self,
        ticket: u64,
        request: &JsonRpcRequest,
        session: &EngineSession,
    ) -> bool {
        let wrong_engine = self.commands.pending.get(&ticket).is_some_and(|pending| {
            matches!(&pending.reply, Reply::Intent { engine, .. } if *engine != session.id)
        });
        if wrong_engine {
            self.reject_resolved_request(
                ticket,
                JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    "intent engine no longer exists",
                ),
            );
            return true;
        }

        let named =
            crate::core::request_target::request_resource_id(&request.method, &request.params);
        if let Some(resource) = named
            && !crate::core::request_target::engine_has_resource(&session.as_ref(), resource)
        {
            self.reject_resolved_request(
                ticket,
                JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    crate::core::request_target::unowned_target_message(resource, &request.method),
                ),
            );
            return true;
        }
        false
    }

    #[cfg(not(feature = "gui"))]
    pub(crate) fn resolve_headless_requests(
        &mut self,
        session: &mut EngineSession,
        state: &mut crate::state::RequestContext,
        services: &crate::app::services::AppServices,
    ) {
        for (ticket, request) in self.requests_needing_resolution() {
            if self.reject_headless_owner(ticket, &request, session) {
                continue;
            }
            if !self.bind_ipc_command_engine(ticket, session.id, &session.core_state) {
                continue;
            }
            if request.method == "remote.structural" {
                let missing = self.unresolved_remote_anchor(ticket, [session.as_ref()]);
                self.resolve_remote_request(ticket, session, missing);
                continue;
            }
            if matches!(
                request.method.as_str(),
                "terminal.spawn"
                    | "terminal.respawn"
                    | "pty.attach_surface"
                    | "tab.create"
                    | "split"
                    | "surface.respawn_terminal"
                    | "image.open"
            ) {
                self.resolve_public_creation(ticket, session);
                continue;
            }
            if matches!(
                request.method.as_str(),
                "preset.apply" | "intent.preset-apply"
            ) {
                self.resolve_preset(
                    ticket,
                    session,
                    services,
                    Some(state.focused_pane_id(&session.core_state)),
                );
                continue;
            }
            if request.method == "workspace.create" {
                let kind = request
                    .params
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("terminal");
                match crate::ipc::handler::workspace::resolve_create_cwd(
                    &request.params,
                    kind,
                    &crate::ipc::request_scope::RequestScope::capture(state, &session.core_state),
                    &session.as_ref(),
                    &|surface| {
                        crate::ipc::handler::workspace::named_source_cwd(&session.as_ref(), surface)
                    },
                    &serde_json::Value::Null,
                ) {
                    Ok(cwd) => self.resolve_workspace_creation(ticket, session, cwd),
                    Err(response) => self.reject_resolved_request(ticket, response),
                }
            } else {
                self.resolve_ipc_for_engine(ticket, session);
            }
        }
        self.finish_headless_completions(session, state);
    }

    #[cfg(not(feature = "gui"))]
    pub(super) fn finish_headless_completions(
        &mut self,
        session: &mut EngineSession,
        state: &mut crate::state::RequestContext,
    ) {
        for (engine, replacement) in std::mem::take(&mut self.replacements) {
            if engine == session.id {
                state.navigation.apply_replacement(replacement);
            }
        }
        for (engine, event) in std::mem::take(&mut self.commands.completed_host_events) {
            if engine == session.id
                && let Some(event) = event.resolve(&session.as_ref(), &state.navigation)
            {
                session.borrow_mut().enqueue_host_event(event);
            }
        }
        for result in std::mem::take(&mut self.commands.completed_intents) {
            result.finish_completion();
            if let Some(error) = result.response.error {
                tracing::warn!(engine = ?result.engine, origin = ?result.origin, "headless journal intent failed: {}", error.message);
            }
        }
    }
}
