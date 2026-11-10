//! Fixed engine intents and settings continuations share ordinary journal admission.
use super::*;

impl JournalApplication {
    /// 다음 intent admission의 결과를 `run`에 남기게 한다. 남은 값은 [`Self::take_intent_completion`]으로 거둔다.
    #[cfg(feature = "gui")]
    pub(crate) fn set_intent_completion(&mut self, run: Option<u64>) {
        self.commands.intent_completion = run;
    }

    /// admission이 가져가지 않은 run을 돌려준다. journal에 들어가지 않은 명령의 결과다.
    #[cfg(feature = "gui")]
    pub(crate) fn take_intent_completion(&mut self) -> Option<u64> {
        self.commands.intent_completion.take()
    }

    pub(crate) fn admit_metadata_intent(
        &mut self,
        engine_id: EngineId,
        core: &crate::core::CoreState,
        intent: &crate::app::command::DomainIntent,
        origin: &crate::intent::IntentOrigin,
        view: Option<IntentViewContinuation>,
    ) -> bool {
        use crate::app::command::DomainIntent as I;
        if let I::ApplyPreset {
            kind,
            name,
            target_pane_id,
            category,
        } = intent
        {
            #[cfg(feature = "gui")]
            let view = view.map(|mut continuation| {
                continuation.preset_apply = true;
                continuation
            });
            self.admit_intent_request(engine_id,"intent.preset-apply",serde_json::json!({"kind":kind,"name":name,"target_pane_id":target_pane_id,"category":category}),origin,view);
            return true;
        }
        if let I::RestoreClosedItem {
            target_pane_id,
            scope,
        } = intent
        {
            if crate::core::state::mirror_workspace_index_for_structural(core, intent).is_some() {
                return false;
            }
            let scope: Option<u32> = match scope {
                crate::app::command::RestoreScope::Local => None,
            };
            self.admit_intent_request(
                engine_id,
                "intent.restore-closed",
                serde_json::json!({"pane":target_pane_id,"scope":scope}),
                origin,
                view,
            );
            return true;
        }
        let replacement = match intent {
            I::MoveSurface {
                source_surface_id: source,
                target_surface_id: target,
            } => Some((tasty_core::IdKind::Surface, *source, *target)),
            I::ReplaceTabWithTab {
                source_tab_id: source,
                target_tab_id: target,
            } => Some((tasty_core::IdKind::Tab, *source, *target)),
            I::ReplacePaneWithPane {
                source_pane_id: source,
                target_pane_id: target,
            } => Some((tasty_core::IdKind::Pane, *source, *target)),
            _ => None,
        };
        if let Some((kind, source, target)) = replacement {
            let replacement = tasty_core::Replacement {
                source: tasty_core::EntityId { kind, id: source },
                target: tasty_core::EntityId { kind, id: target },
            };
            self.admit_intent_request(
                engine_id,
                "intent.replace",
                serde_json::to_value(replacement).expect("replacement IDs serialize"),
                origin,
                view,
            );
            return true;
        }
        let close = match intent {
            I::CloseWorkspace { workspace_id } => Some((
                tasty_core::CloseTarget::Workspace(*workspace_id),
                origin.is_user(),
                origin.is_user(),
                None,
            )),
            I::CloseTab { tab_id } => Some((
                tasty_core::CloseTarget::Tab(*tab_id),
                origin.is_user(),
                origin.is_user(),
                None,
            )),
            I::ClosePane { pane_id } => Some((
                tasty_core::CloseTarget::Pane(*pane_id),
                origin.is_user(),
                origin.is_user(),
                None,
            )),
            I::CloseSurface {
                surface_id,
                presentation,
            } => Some((
                tasty_core::CloseTarget::Surface(*surface_id),
                origin.is_user() && presentation.is_some(),
                origin.is_user(),
                None,
            )),
            I::RetireExitedSurface { surface_id, .. } => Some((
                tasty_core::CloseTarget::Surface(*surface_id),
                false,
                true,
                Some(
                    core.find_surface_by_id(*surface_id)
                        .and_then(|surface| surface.activation_generation),
                ),
            )),
            _ => None,
        };
        if let Some((target, capture, user_close, expected)) = close {
            if crate::core::state::mirror_workspace_index_for_structural(core, intent).is_some() {
                return false;
            }
            let mut params =
                serde_json::json!({"target":target,"capture":capture,"user_close":user_close});
            if let Some(expected) = expected {
                params["expected_activation"] = serde_json::json!(expected);
            }
            let mut view = view;
            if let Some(view) = view.as_mut() {
                view.close_empty_engine = matches!(target, tasty_core::CloseTarget::Workspace(_));
            }
            let ticket = self.next_ticket;
            self.admit_intent_request(engine_id, "intent.close", params, origin, view);
            if let I::RetireExitedSurface { generation, .. } = intent
                && matches!(origin, crate::intent::IntentOrigin::System)
                && let Some(pending) = self.commands.pending.get_mut(&ticket)
            {
                pending.close_cause = super::close::Cause::ProcessExit(*generation);
            }
            return true;
        }
        if let Some(spec) = super::create_spec::Spec::from_intent(intent) {
            if crate::core::state::mirror_workspace_index_for_structural(core, intent).is_some() {
                return false;
            }
            self.admit_intent_request(
                engine_id,
                "intent.create",
                serde_json::to_value(spec).expect("fixed creation spec serializes"),
                origin,
                view,
            );
            return true;
        }
        let (method, params) = match intent {
            I::CreateCategory { name } => (
                "workspace_category.create",
                serde_json::json!({"name":name}),
            ),
            I::RenameCategory { id, name } => (
                "workspace_category.rename",
                serde_json::json!({"id":id,"name":name}),
            ),
            I::DeleteCategory { id } => ("workspace_category.delete", serde_json::json!({"id":id})),
            I::MoveWorkspace {
                workspace_id,
                to_index,
            } => (
                "workspace.move",
                serde_json::json!({"id":workspace_id,"to_index":to_index}),
            ),
            I::SetWorkspaceCategory {
                workspace_id,
                category,
            } => (
                "workspace.update",
                serde_json::json!({"id":workspace_id,"category":category}),
            ),
            I::MoveTab {
                pane_id,
                tab_id,
                to_index,
            } => {
                if core
                    .find_workspace_index_for_pane(*pane_id)
                    .and_then(|index| core.workspace_at(index))
                    .is_some_and(|workspace| workspace.mirror)
                {
                    return false; // Existing remote forwarding owns mirror structure commands.
                }
                (
                    "intent.tab-move",
                    serde_json::json!({"pane_id":pane_id,"tab_id":tab_id,"to_index":to_index}),
                )
            }
            _ => return false,
        };
        self.admit_intent_request(engine_id, method, params, origin, None);

        true
    }
    pub(crate) fn admit_direct_rename(
        &mut self,
        engine_id: EngineId,
        rename: &crate::intent::rename::DirectRename,
        origin: &crate::intent::IntentOrigin,
    ) {
        use crate::intent::rename::DirectRename as R;
        let (method, params) = match rename {
            R::WorkspaceName { workspace_id, name } => (
                "intent.workspace-rename",
                serde_json::json!({"id":workspace_id,"name":name,"user_direct":origin.is_user()}),
            ),
            R::WorkspaceSubtitle {
                workspace_id,
                subtitle,
            } => (
                "intent.workspace-rename",
                serde_json::json!({"id":workspace_id,"subtitle":subtitle,"user_direct":origin.is_user()}),
            ),
            R::TabName { tab_id, name } => (
                "intent.tab-name",
                serde_json::json!({"tab_id":tab_id,"name":name,"user_direct":origin.is_user()}),
            ),
            R::ClearWorkspaceMapping { workspace_id } => (
                "intent.workspace-mapping",
                serde_json::json!({"id":workspace_id,"mapping":null}),
            ),
        };
        self.admit_intent_request(engine_id, method, params, origin, None);
    }

    /// 저장된 원격 행의 현재 값을 journal에 저장하거나(`remove == false`) 지운다.
    /// 값은 처리 시점의 화면 상태에서 읽으므로 그사이 행이 닫혔으면 저장하지 않는다.
    pub(crate) fn admit_remote_row(
        &mut self,
        engine_id: EngineId,
        workspace_id: u32,
        remove: bool,
    ) {
        self.admit_intent_request(
            engine_id,
            "intent.remote-row",
            serde_json::json!({"id": workspace_id, "remove": remove}),
            &crate::intent::IntentOrigin::System,
            None,
        );
    }

    pub(super) fn admit_intent_request(
        &mut self,
        engine_id: EngineId,
        method: &str,
        params: serde_json::Value,
        origin: &crate::intent::IntentOrigin,
        view: Option<IntentViewContinuation>,
    ) {
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id: None,
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let completion = self.commands.intent_completion.take();
        self.admit_request(
            &request,
            Reply::Intent {
                engine: engine_id,
                origin: origin.clone(),
                view,
                completion,
            },
            match origin {
                crate::intent::IntentOrigin::User { .. } => "user",
                crate::intent::IntentOrigin::Agent { .. } => "agent",
                crate::intent::IntentOrigin::System => "system",
            }
            .into(),
            &format!("intent:{origin:?}"),
        );
    }
}

#[cfg(all(test, feature = "gui"))]
impl JournalApplication {
    pub(crate) fn admit_fixed_intent(
        &mut self,
        session: &EngineSession,
        commands: Vec<tasty_core::StructuralCommand>,
        origin: &crate::intent::IntentOrigin,
    ) -> Result<(), String> {
        let binding = session
            .journal_binding
            .as_ref()
            .ok_or("structural intent engine has no journal binding")?;
        if commands
            .iter()
            .any(|command| !command.reserved_ids().is_empty())
        {
            return Err("fixed intent requires admission-time ID reservation".into());
        }
        let changes: Vec<_> = commands
            .into_iter()
            .map(|command| StreamCommand {
                stream: binding.stream.clone(),
                command,
            })
            .collect();
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: "intent.structural".into(),
            params: serde_json::to_value(&changes).map_err(|error| error.to_string())?,
            id: None,
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let ticket = self.next_ticket;
        self.admit_request(
            &request,
            Reply::Intent {
                engine: session.id,
                origin: origin.clone(),
                view: None,
                completion: None,
            },
            intent_actor(origin).into(),
            &format!("intent:{origin:?}"),
        );
        if let Some(pending) = self.commands.pending.get_mut(&ticket) {
            pending.fixed = Some(changes);
            pending.request.params = serde_json::Value::Null;
        }
        self.refresh_command_weight(ticket);
        Ok(())
    }
}

#[cfg(feature = "gui")]
impl JournalApplication {
    pub(crate) fn admit_settings_reset(
        &mut self,
        generation: u64,
        changes: Vec<StreamCommand>,
        settings: crate::settings::Settings,
        origin: &crate::intent::IntentOrigin,
        done: Option<crate::app::settings_change::SettingsDone>,
    ) {
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: "intent.category-reset".into(),
            params: serde_json::to_value(&changes).expect("fixed commands serialize"),
            id: None,
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let ticket = self.next_ticket;
        self.admit_request(
            &request,
            Reply::Settings {
                settings: Some(Box::new(settings)),
                generation,
                origin: origin.clone(),
                done,
            },
            intent_actor(origin).into(),
            &format!("intent:{origin:?}"),
        );
        if let Some(pending) = self.commands.pending.get_mut(&ticket) {
            pending.fixed = Some(changes);
            pending.request.params = serde_json::Value::Null;
        }
        self.refresh_command_weight(ticket);
    }
}

#[cfg(feature = "gui")]
impl JournalApplication {
    pub(crate) fn note_settings_intent(&mut self) -> Result<u64, String> {
        let generation = self
            .commands
            .settings_generation
            .checked_add(1)
            .ok_or("settings continuation generation exhausted")?;
        self.commands.settings_generation = generation;
        Ok(generation)
    }

    /// 현재 세대의 완료만 돌려준다. 더 새 설정 변경에 밀린 완료는 적용하지 않고, 기다리는 요청에는
    /// 대체됐다고 알린다.
    pub(super) fn take_settings_results(&mut self) -> Vec<super::SettingsCompletion> {
        let current = self.commands.settings_generation;
        std::mem::take(&mut self.commands.completed_settings)
            .into_iter()
            .filter_map(|completion| {
                if completion.generation == current {
                    return Some(completion);
                }
                if let Some(done) = completion.done {
                    done.finish(crate::app::settings_change::SettingsOutcome::Superseded);
                }
                None
            })
            .collect()
    }
}

#[cfg(feature = "gui")]
pub(super) fn intent_actor(origin: &crate::intent::IntentOrigin) -> &'static str {
    match origin {
        crate::intent::IntentOrigin::User { .. } => "user",
        crate::intent::IntentOrigin::Agent { .. } => "agent",
        crate::intent::IntentOrigin::System => "system",
    }
}
