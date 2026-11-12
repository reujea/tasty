//! Resolve fixed GUI requests and deliver their original live View continuations.
use super::*;

#[cfg(feature = "gui")]
impl crate::app::App {
    pub(crate) fn resolve_journal_requests(&mut self) {
        self.execute_journal_forwards();
        for (ticket, request) in self.journal.requests_needing_resolution() {
            self.resolve_one_journal_request(ticket, &request);
        }
        self.finish_live_inputs();
        self.finish_preset_captures();
        self.deliver_remote_journal_results();
        self.journal
            .deliver_plugin_replies(self.plugin_manager.as_mut());
        self.finish_settings_results();
        self.finish_divider_results();
        self.finish_structural_host_events();
        for result in std::mem::take(&mut self.journal.commands.completed_intents) {
            self.finish_structural_intent(result);
        }
        self.reconcile_journal_views();
    }

    fn bind_journal_ipc_engine(&mut self, ticket: u64, id: EngineId) -> bool {
        match self.engines.get(id) {
            Some(engine) => self
                .journal
                .bind_ipc_command_engine(ticket, id, engine.core),
            None => self.journal.bind_command_engine(ticket, id),
        }
    }

    fn resolve_engine_journal_request(
        &mut self,
        ticket: u64,
        id: EngineId,
        request: &JsonRpcRequest,
    ) {
        if !self.bind_journal_ipc_engine(ticket, id) {
            return;
        }
        if self.try_resolve_mirror_request(ticket, id, request) {
            return;
        }
        let missing_anchor = (request.method == "remote.structural").then(|| {
            self.journal.unresolved_remote_anchor(
                ticket,
                self.engines.all_sessions().map(|session| session.as_ref()),
            )
        });
        if let Some(session) = self.engines.session_mut(id) {
            if let Some(missing_anchor) = missing_anchor {
                self.journal
                    .resolve_remote_request(ticket, session, missing_anchor);
            } else if request.method == "intent.preset-apply" {
                self.journal
                    .resolve_preset(ticket, session, &self.services, None);
            } else {
                self.journal.resolve_ipc_for_engine(ticket, session);
            }
        } else {
            self.journal.reject_resolved_request(
                ticket,
                JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    "intent engine no longer exists",
                ),
            );
        }
    }

    fn resolve_one_journal_request(&mut self, ticket: u64, request: &JsonRpcRequest) {
        if let Some(id) = self
            .journal
            .commands
            .pending
            .get(&ticket)
            .and_then(|pending| pending.reply.engine_scope())
        {
            self.resolve_engine_journal_request(ticket, id, request);
            return;
        }
        let named =
            crate::core::request_target::request_resource_id(&request.method, &request.params);
        let window = match self.find_request_owner(&request.method, &request.params) {
            Ok(found) if named.is_some() => found,
            Ok(found) => found.or(self.view.focused_view_id),
            Err(error) => {
                self.journal.reject_resolved_request(
                    ticket,
                    JsonRpcResponse::invalid_params(serde_json::Value::Null, error),
                );
                return;
            }
        };
        let fixed_engine = self
            .journal
            .commands
            .pending
            .get(&ticket)
            .and_then(|pending| pending.engine_scope);
        let id = fixed_engine.or_else(|| {
            window
                .and_then(|window| self.engines.of_window(window))
                .or_else(|| {
                    self.engines()
                        .parked_with_ids()
                        .find(|(_, engine)| {
                            named.is_none_or(|resource| {
                                crate::core::request_target::engine_has_resource(engine, resource)
                            })
                        })
                        .map(|(id, _)| id)
                })
        });
        if let Some(engine) = id
            && !self.bind_journal_ipc_engine(ticket, engine)
        {
            return;
        }
        if let Some(engine) = id
            && self.try_resolve_mirror_request(ticket, engine, request)
        {
            return;
        }
        let preset_pane = if request.method == "preset.apply" {
            id.and_then(|engine| self.engines_mut().resolve(engine))
                .map(|context| context.state.focused_pane_id(&context.engine.read()))
        } else {
            None
        };
        let creation_cwd = if request.method == "workspace.create" {
            // `inherit_cwd`의 원본은 다른 창이나 parked engine에 있을 수 있다. 모든 engine에서 미리 찾는다.
            let named = crate::ipc::handler::workspace::named_inherit_surface(&request.params).map(
                |surface| {
                    (
                        surface,
                        crate::ipc::handler::workspace::named_source_in(
                            self.engines().all(),
                            surface,
                        ),
                    )
                },
            );
            let lookup = |surface: u32| {
                named
                    .as_ref()
                    .filter(|(found, _)| *found == surface)
                    .and_then(|(_, cwd)| cwd.clone())
            };
            id.and_then(|engine| self.engines_mut().resolve(engine))
                .map(|context| {
                    let kind = request
                        .params
                        .get("type")
                        .and_then(|value| value.as_str())
                        .unwrap_or("terminal");
                    crate::ipc::handler::workspace::resolve_create_cwd(
                        &request.params,
                        kind,
                        &crate::ipc::request_scope::RequestScope::capture(
                            context.state,
                            context.engine.core,
                            None,
                        ),
                        &context.engine.as_ref(),
                        &lookup,
                        &serde_json::Value::Null,
                    )
                })
        } else {
            None
        };
        let Some(session) = id.and_then(|id| self.engines.session_mut(id)) else {
            let response = match named {
                Some(resource) => JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    crate::core::request_target::unowned_target_message(resource, &request.method),
                ),
                None => JsonRpcResponse::invalid_params(
                    serde_json::Value::Null,
                    "no engine is available for structural request",
                ),
            };
            self.journal.reject_resolved_request(ticket, response);
            return;
        };
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
            self.journal.resolve_public_creation(ticket, session);
            return;
        }
        if request.method == "preset.apply" {
            self.journal
                .resolve_preset(ticket, session, &self.services, preset_pane);
            return;
        }
        if let Some(cwd) = creation_cwd {
            match cwd {
                Ok(cwd) => self
                    .journal
                    .resolve_workspace_creation(ticket, session, cwd),
                Err(response) => self.journal.reject_resolved_request(ticket, response),
            }
        } else {
            self.journal.resolve_ipc_for_engine(ticket, session);
        }
    }

    fn finish_settings_results(&mut self) {
        for completion in self.journal.take_settings_results() {
            match completion.into_settings() {
                // 기다리는 요청에는 into_settings가 거절을 넣었고, 표시는 공통 보고가 맡는다.
                Err((origin, message)) => {
                    crate::intent::report_apply_error(
                        None,
                        &origin,
                        crate::intent::FailureContext::CategoryReset,
                        &anyhow::anyhow!(message),
                    );
                    if origin.is_user() {
                        self.toast_settings_result(
                            crate::intent::FailureContext::CategoryReset
                                .message()
                                .to_owned(),
                            crate::adapters::ui::ToastKind::Error,
                        );
                    }
                }
                Ok((settings, origin, done)) => {
                    self.apply_settings_after_structure(settings, &origin, done);
                }
            }
        }
    }

    fn finish_divider_results(&mut self) {
        for (engine, sequence, origin, response) in
            std::mem::take(&mut self.journal.commands.completed_dividers)
        {
            if let Some(context) = self.engines_mut().resolve(engine) {
                context.state.layout_previews.cancel(sequence);
                if let Some(error) = response.error {
                    crate::intent::report_apply_error(
                        context.view.as_ref().map(|_| &mut *context.state),
                        &origin,
                        crate::intent::FailureContext::Divider,
                        &anyhow::anyhow!(error.message),
                    );
                }
                if let Some(view) = context.view {
                    view.mark_dirty();
                }
            }
        }
    }

    fn finish_structural_host_events(&mut self) {
        let mut presentations: std::collections::HashMap<_, _> = self
            .engines()
            .window_pairs()
            .filter_map(|(window, main, engine)| {
                self.engines.of_window(window).map(|id| {
                    (
                        id,
                        crate::model::StructurePresentationSnapshot::capture(
                            engine.workspaces(),
                            engine.categories(),
                            &main.state.navigation,
                        ),
                    )
                })
            })
            .collect();
        for (id, state, engine) in self.engines.parked_sessions() {
            presentations.insert(
                id,
                crate::model::StructurePresentationSnapshot::capture(
                    engine.workspaces(),
                    engine.categories(),
                    &state.navigation,
                ),
            );
        }
        for (id, navigation, _) in self.engines.preserved_closes() {
            if let Some(engine) = self.engines.get(id) {
                presentations.insert(
                    id,
                    crate::model::StructurePresentationSnapshot::capture(
                        engine.workspaces(),
                        engine.categories(),
                        &navigation,
                    ),
                );
            }
        }
        for (engine, event) in std::mem::take(&mut self.journal.commands.completed_host_events) {
            if let Some(session) = self.engines.session_mut(engine) {
                let presentation = presentations
                    .get(&engine)
                    .unwrap_or(&session.remote.presentation);
                if let Some(event) = event.resolve(&session.as_ref(), presentation) {
                    session.borrow_mut().enqueue_host_event(event);
                }
            }
        }
    }

    fn finish_tutorial_continuations(&mut self, result: &IntentResult) {
        if result.origin.is_user()
            && !result.response.idempotent_replay
            && let Some(continuation) = result.view.as_ref()
            && let Some(ticket) = continuation.tutorial_preparation.as_ref()
            && let Some(context) = self.engines_mut().resolve(result.engine)
            && context
                .view
                .as_ref()
                .is_some_and(|view| view.state.matches_identity(&continuation.view))
            && context.state.tutorial.matches_preparation(ticket)
        {
            let practice = continuation
                .tutorial_surface
                .as_ref()
                .filter(|(_, target)| {
                    result.response.error.is_none() && target.current(&context.engine.as_ref())
                })
                .and_then(|(surface, _)| {
                    let (index, pane) =
                        context.engine.find_workspace_index_for_surface(*surface)?;
                    let workspace = context.engine.workspace_at(index)?.id;
                    let tab = context.engine.find_tab_for_surface(*surface)?;
                    Some(crate::adapters::ui::tutorial::PracticeContext {
                        workspace,
                        pane,
                        tab,
                    })
                });
            if let Some(practice) = practice {
                context.state.tutorial.prepared(practice);
            } else {
                context.state.tutorial.preparation_failed();
            }
            if let Some(view) = context.view {
                view.mark_dirty();
            }
        }
        if result.response.error.is_none()
            && result.origin.is_user()
            && !result.response.idempotent_replay
            && let Some(continuation) = result.view.as_ref()
            && let Some(tutorial) = continuation.tutorial.as_ref()
            && let Some(context) = self.engines_mut().resolve(result.engine)
            && context
                .view
                .as_ref()
                .is_some_and(|view| view.state.matches_identity(&continuation.view))
        {
            tutorial.observe(context.state, context.engine.core);
        }
    }

    fn activate_completed_intent(&mut self, result: &mut IntentResult) {
        if result.response.error.is_none()
            && result.origin.is_user()
            && let Some(continuation) = result.view.take()
            && let Some(surface) = continuation.activate_surface
            && let Some(context) = self.engines_mut().resolve(result.engine)
            && context
                .view
                .as_ref()
                .is_some_and(|view| view.state.matches_identity(&continuation.view))
            && context
                .state
                .navigation
                .matches_generation(&continuation.selection)
            && let Some((index, pane_id)) = context.engine.find_workspace_index_for_surface(surface)
            && let Some(workspace) = context.engine.workspace_at(index)
            && let Some(pane) = workspace.pane_layout().find_pane(pane_id)
            && let Some(tab) = pane.tabs.iter().find(|tab| tab.contains_surface(surface))
        {
            if let Some(saved) = result
                .response
                .result
                .as_ref()
                .and_then(|value| value.get("presentation"))
                .and_then(|value| {
                    serde_json::from_value::<tasty_core::UndoPresentation>(value.clone()).ok()
                })
            {
                restore_saved_presentation(context.state, &context.engine.as_ref(), saved);
            }
            context
                .state
                .navigation
                .select_workspace(&context.engine.workspaces(), workspace.id);
            context.state.navigation.select_pane(workspace, pane.id);
            context.state.navigation.select_tab(pane, tab.id);
            context.state.navigation.select_surface(tab, surface);
            if let Some(followup) = continuation.after_create {
                match followup {
                    crate::intent::CreateFollowup::Prompt { kind } => {
                        context.state.enqueue_convert_input_popup(
                            &context.engine.read(),
                            &kind,
                            Some(surface),
                        );
                    }
                }
            }
            if let Some(view) = context.view {
                view.mark_dirty();
            }
        }
    }

    fn finish_structural_intent(&mut self, mut result: IntentResult) {
        result.finish_completion();
        if result.response.error.is_none()
            && let Some(surface) = result
                .response
                .result
                .as_ref()
                .and_then(|value| value.get("restored_surface_id"))
                .and_then(|value| value.as_u64())
                .and_then(|id| u32::try_from(id).ok())
            && let Some(view) = result.view.as_mut()
        {
            view.activate_surface = Some(surface);
        }
        self.finish_tutorial_continuations(&result);
        if result.response.error.is_none()
            && result.origin.is_user()
            && let Some(continuation) = result.view.as_ref()
            && continuation.close_empty_engine
            && let Some(context) = self.engines_mut().resolve(result.engine)
            && context.engine.workspaces().is_empty()
            && let Some(view) = context.view
            && view.state.matches_identity(&continuation.view)
        {
            view.state.close_requested = true;
        }
        self.activate_completed_intent(&mut result);
        if let Some(error) = result.response.error
            && let Some(context) = self.engines_mut().resolve(result.engine)
        {
            report_structural_failure(
                context.state,
                context.view.as_ref().map(|view| &view.state),
                result.view.as_ref(),
                &result.origin,
                &error,
                result.response.idempotent_replay,
            );
            if let Some(view) = context.view {
                view.mark_dirty();
            }
        }
    }

    fn reconcile_journal_views(&mut self) {
        for (engine, replacement) in std::mem::take(&mut self.journal.replacements) {
            if let Some(context) = self.engines_mut().resolve(engine) {
                context.state.navigation.apply_replacement(replacement);
            }
        }
        for id in self.journal.take_changed_engines() {
            if let Some(mut context) = self.engines_mut().resolve(id) {
                context.state.reconcile_presentation(&context.engine);
                context
                    .engine
                    .refresh_attach_presentation(&context.state.navigation);
                if let Some(view) = context.view {
                    view.mark_dirty();
                }
            }
        }
    }
}

fn restore_saved_presentation(
    state: &mut crate::state::RequestContext,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    saved: tasty_core::UndoPresentation,
) {
    for (workspace, pane) in saved.focused_panes {
        if let Some(workspace) = engine
            .find_workspace_index_for_id(workspace)
            .and_then(|index| engine.workspace_at(index))
        {
            state.navigation.select_pane(workspace, pane);
        }
    }
    for (pane, tab) in saved.selected_tabs {
        if let Some(pane) = engine.find_pane_by_id(pane) {
            state.navigation.select_tab(pane, tab);
        }
    }
    for (tab, surface) in saved.selected_surfaces {
        if let Some(tab) = engine
            .find_pane_for_tab(tab)
            .and_then(|pane| engine.find_pane_by_id(pane))
            .and_then(|pane| pane.tabs.iter().find(|candidate| candidate.id == tab))
        {
            state.navigation.select_surface(tab, surface);
        }
    }
}

/// Preset, split refusal and generic failure are mutually exclusive display paths.
fn report_structural_failure(
    state: &mut crate::state::RequestContext,
    view: Option<&crate::view::state::ViewState>,
    continuation: Option<&IntentViewContinuation>,
    origin: &crate::intent::IntentOrigin,
    error: &tasty_ipc::protocol::JsonRpcError,
    replay: bool,
) {
    let view =
        view.filter(|view| !replay && continuation.is_none_or(|c| view.matches_identity(&c.view)));
    let shown = view.is_some_and(|view| {
        continuation.is_some_and(|c| show_failed_preset(&mut state.toasts, view, c, origin, false))
            || show_refused_split(&mut state.toasts, error, origin, false)
    });
    crate::intent::report_apply_error(
        (view.is_some() && !shown).then_some(state),
        origin,
        crate::intent::FailureContext::Layout,
        &anyhow::anyhow!(error.message.clone()),
    );
}

fn show_failed_preset(
    toasts: &mut crate::adapters::ui::toast::ToastManager,
    view: &crate::view::state::ViewState,
    continuation: &IntentViewContinuation,
    origin: &crate::intent::IntentOrigin,
    replay: bool,
) -> bool {
    if continuation.preset_apply
        && origin.is_user()
        && !replay
        && view.matches_identity(&continuation.view)
    {
        toasts.push(
            crate::i18n::t("preset.toast.apply_failed"),
            crate::model::toast_kind::ToastKind::Error,
            crate::model::toast_kind::ToastScope::Window,
        );
        true
    } else {
        false
    }
}

/// 메뉴·단축키로 한 split 이 탐색기 칸 하한 때문에 거절되면 그 창에 info 토스트를 띄운다. 레이아웃은
/// 바뀌지 않았다. 에이전트 split 은 IPC 오류만 받는다.
fn show_refused_split(
    toasts: &mut crate::adapters::ui::toast::ToastManager,
    error: &tasty_ipc::protocol::JsonRpcError,
    origin: &crate::intent::IntentOrigin,
    replay: bool,
) -> bool {
    let refused = error
        .data
        .as_ref()
        .and_then(|data| data.get("reason"))
        .and_then(|reason| reason.as_str())
        == Some(super::split_floor::REFUSAL_REASON);
    if refused && origin.is_user() && !replay {
        toasts.push(
            crate::i18n::t("toast.split_refused"),
            crate::model::toast_kind::ToastKind::Info,
            crate::model::toast_kind::ToastScope::Window,
        );
        true
    } else {
        false
    }
}

#[cfg(test)]
mod preset_failure_tests {
    use super::*;
    use crate::intent::{AgentSource, IntentOrigin, UserSource};

    fn continuation(view: &crate::view::state::ViewState) -> IntentViewContinuation {
        IntentViewContinuation {
            remote_request: None,
            view: view.identity(),
            selection: std::sync::Weak::new(),
            activate_surface: None,
            close_empty_engine: false,
            preset_apply: true,
            after_create: None,
            tutorial: None,
            tutorial_preparation: None,
            tutorial_surface: None,
        }
    }
    fn user() -> IntentOrigin {
        IntentOrigin::User {
            source: UserSource::Shortcut("preset-test"),
        }
    }
    #[test]
    fn failed_user_preset_displays_the_original_view_error() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 문구를 만들기 전에 초기화한다.
        crate::i18n::init("en");
        let view = crate::view::state::ViewState::default();
        let mut toasts = crate::adapters::ui::toast::ToastManager::new();
        show_failed_preset(&mut toasts, &view, &continuation(&view), &user(), false);
        assert_eq!(
            toasts.messages(),
            vec![crate::i18n::t("preset.toast.apply_failed")]
        );
    }
    #[test]
    fn stale_agent_replayed_and_other_intent_failures_do_not_show_preset_toasts() {
        let original = crate::view::state::ViewState::default();
        let replacement = crate::view::state::ViewState::default();
        let mut continuation = continuation(&original);
        let mut toasts = crate::adapters::ui::toast::ToastManager::new();
        show_failed_preset(&mut toasts, &replacement, &continuation, &user(), false);
        show_failed_preset(
            &mut toasts,
            &original,
            &continuation,
            &IntentOrigin::Agent {
                source: AgentSource::Ipc,
            },
            false,
        );
        show_failed_preset(&mut toasts, &original, &continuation, &user(), true);
        continuation.preset_apply = false;
        show_failed_preset(&mut toasts, &original, &continuation, &user(), false);
        assert_eq!(toasts.len(), 0);
    }
    #[test]
    fn structural_failure_displays_one_message_and_respects_view_lifetime() {
        let (mut state, _) = crate::state::tests::test_state();
        let view = crate::view::state::ViewState::default();
        let old = crate::view::state::ViewState::default();
        let preset = continuation(&view);
        report_structural_failure(
            &mut state,
            Some(&view),
            Some(&preset),
            &user(),
            &refusal(),
            false,
        );
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("preset.toast.apply_failed")],
            "preset takes precedence without an extra split or generic toast"
        );
        state.toasts = crate::adapters::ui::toast::ToastManager::new();
        let mut ordinary = continuation(&view);
        ordinary.preset_apply = false;
        report_structural_failure(
            &mut state,
            Some(&view),
            Some(&ordinary),
            &user(),
            &refusal(),
            false,
        );
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("toast.split_refused")]
        );
        state.toasts = crate::adapters::ui::toast::ToastManager::new();
        let error = tasty_ipc::protocol::JsonRpcResponse::invalid_params(
            serde_json::Value::Null,
            "internal.method /private/path",
        )
        .error
        .unwrap();
        for (shown, replay, origin) in [
            (None, false, user()),
            (Some(&old), false, user()),
            (Some(&view), true, user()),
            (Some(&view), false, crate::intent::IntentOrigin::System),
            (
                Some(&view),
                false,
                crate::intent::IntentOrigin::Agent {
                    source: crate::intent::AgentSource::Ipc,
                },
            ),
        ] {
            report_structural_failure(&mut state, shown, Some(&ordinary), &origin, &error, replay);
        }
        assert_eq!(state.toasts.len(), 0);
        report_structural_failure(
            &mut state,
            Some(&view),
            Some(&ordinary),
            &user(),
            &error,
            false,
        );
        assert_eq!(
            state.toasts.messages(),
            vec![crate::intent::FailureContext::Layout.message()]
        );
    }
    fn refusal() -> tasty_ipc::protocol::JsonRpcError {
        super::super::split_floor::refusal_for_tests()
            .error
            .expect("refusal error")
    }
    #[test]
    fn a_refused_user_split_shows_an_info_toast() {
        crate::i18n::init("en");
        let mut toasts = crate::adapters::ui::toast::ToastManager::new();
        show_refused_split(&mut toasts, &refusal(), &user(), false);
        assert_eq!(
            toasts.messages(),
            vec![crate::i18n::t("toast.split_refused")]
        );
    }
    #[test]
    fn agent_replayed_and_other_failures_show_no_split_toast() {
        let mut toasts = crate::adapters::ui::toast::ToastManager::new();
        let agent = IntentOrigin::Agent {
            source: AgentSource::Ipc,
        };
        show_refused_split(&mut toasts, &refusal(), &agent, false);
        show_refused_split(&mut toasts, &refusal(), &user(), true);
        let other = tasty_ipc::protocol::JsonRpcError {
            data: None,
            ..refusal()
        };
        show_refused_split(&mut toasts, &other, &user(), false);
        assert_eq!(toasts.len(), 0);
    }
}
