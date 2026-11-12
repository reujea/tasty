//! Map only explicit mirror targets after original-key lookup. No local journal tree is mutated.
use super::*;
use tasty_ipc::stream::{SplitAxis, StructuralOp};

fn local_op(
    request: &JsonRpcRequest,
    session: &crate::runtime::engine_access::EngineRef<'_>,
    view: &crate::runtime::journal_product::CompletionView,
) -> Option<StructuralOp> {
    let core = session.core;
    let pane_anchor = |id| {
        core.find_pane_by_id(id)
            .and_then(|pane| pane.tabs.first())
            .and_then(|tab| tab.first_surface_id())
    };
    let tab_anchor = |id| {
        core.find_pane_for_tab(id)
            .and_then(|pane| core.find_pane_by_id(pane))
            .and_then(|pane| pane.tabs.iter().find(|tab| tab.id == id))
            .and_then(|tab| tab.first_surface_id())
    };
    let id = |field| {
        request
            .params
            .get(field)
            .and_then(|value| value.as_u64())
            .and_then(|id| u32::try_from(id).ok())
    };
    match request.method.as_str() {
        "terminal.kill" => {
            let parent = kill_parent(&request.params, || {
                session
                    .runtime
                    .child_terminals
                    .single_parent(&session.core.live_surface_ids())
            })?;
            let entry = session
                .runtime
                .child_terminals
                .find_child(&parent, id("child")?)?;
            Some(StructuralOp::CloseSurface {
                surface_id: entry.child_surface_id,
            })
        }
        "intent.remote-structural" => serde_json::from_value(request.params["op"].clone()).ok(),
        "surface.close" | "surface.close_self" => Some(StructuralOp::CloseSurface {
            surface_id: id("surface_id")?,
        }),
        "tab.close" => Some(StructuralOp::CloseTab {
            anchor_surface_id: tab_anchor(id("tab_id")?)?,
        }),
        "pane.close" => Some(StructuralOp::ClosePane {
            anchor_surface_id: pane_anchor(id("pane_id")?)?,
        }),
        "intent.close" => match serde_json::from_value(request.params["target"].clone()).ok()? {
            tasty_core::CloseTarget::Surface(id) => {
                Some(StructuralOp::CloseSurface { surface_id: id })
            }
            tasty_core::CloseTarget::Tab(id) => Some(StructuralOp::CloseTab {
                anchor_surface_id: tab_anchor(id)?,
            }),
            tasty_core::CloseTarget::Pane(id) => Some(StructuralOp::ClosePane {
                anchor_surface_id: pane_anchor(id)?,
            }),
            _ => None,
        },
        "intent.restore-closed" => Some(StructuralOp::RestoreClosedItem {
            anchor_surface_id: pane_anchor(id("pane")?)?,
        }),
        "tab.move" => Some(StructuralOp::MoveTab {
            anchor_surface_id: pane_anchor(id("pane_id")?)?,
            from_index: request.params["from_index"].as_u64()?.try_into().ok()?,
            to_index: request.params["to_index"].as_u64()?.try_into().ok()?,
        }),
        "tab.create" | "split" | "intent.create" | "image.open" => {
            let spec = if request.method == "intent.create" {
                serde_json::from_value::<super::create_spec::Spec>(request.params.clone()).ok()?
            } else {
                super::create_spec::Spec::from_public(
                    request,
                    session,
                    view,
                    super::create_spec::CwdSource::View,
                )
                .ok()?
            };
            let axis = |direction| match direction {
                crate::model::SplitDirection::Horizontal => SplitAxis::Horizontal,
                crate::model::SplitDirection::Vertical => SplitAxis::Vertical,
            };
            match spec.destination {
                super::create_spec::Destination::Tab { pane, .. } => Some(StructuralOp::NewTab {
                    anchor_surface_id: pane_anchor(pane)?,
                    surface_kind: spec.kind,
                    params: spec.params,
                    inherit_cwd: None,
                }),
                super::create_spec::Destination::Pane { target, direction } => {
                    Some(StructuralOp::SplitPane {
                        anchor_surface_id: pane_anchor(target)?,
                        direction: axis(direction),
                        surface_kind: spec.kind,
                        params: spec.params,
                        inherit_cwd: None,
                    })
                }
                super::create_spec::Destination::Surface { target, direction } => {
                    Some(StructuralOp::SplitSurface {
                        surface_id: target,
                        direction: axis(direction),
                        surface_kind: spec.kind,
                        params: spec.params,
                        inherit_cwd: None,
                    })
                }
                super::create_spec::Destination::Convert {
                    surface,
                    respawn: false,
                } => Some(StructuralOp::ConvertSurface {
                    surface_id: surface,
                    surface_kind: spec.kind,
                    params: spec.params,
                    cwd: spec.cwd.map(|path| path.to_string_lossy().into_owned()),
                    inherit_cwd: None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}
/// mirror kill 의 부모. 일반 해석이 거절할 요청(두 부모 지정, 규칙에 어긋난 `parent_external`)은
/// None 이라 전달하지 않고 일반 해석의 -32602 거절에 맡긴다. 잘못된 External 키를 생략으로 읽어
/// 단일 부모 폴백으로 다른 부모의 자식을 닫지 않게 한다.
fn kill_parent(
    params: &serde_json::Value,
    single_parent: impl FnOnce() -> Option<u32>,
) -> Option<crate::core::child_parent::ParentKey> {
    use crate::core::child_parent::ParentKey;
    let surface = params
        .get("surface")
        .and_then(|value| value.as_u64())
        .and_then(|id| u32::try_from(id).ok());
    let external =
        crate::ipc::handler::params::optional_external_parent(params, &serde_json::Value::Null)
            .ok()?
            .map(ParentKey::External);
    match (surface, external) {
        (Some(_), Some(_)) => None,
        (Some(surface), None) => Some(ParentKey::Surface(surface)),
        (None, Some(external)) => Some(external),
        (None, None) => single_parent().map(ParentKey::Surface),
    }
}
impl JournalApplication {
    pub(crate) fn admit_remote_intent(
        &mut self,
        engine: EngineId,
        op: StructuralOp,
        origin: &crate::intent::IntentOrigin,
        view: Option<IntentViewContinuation>,
        candidates: Vec<u32>,
    ) {
        self.admit_intent_request(
            engine,
            "intent.remote-structural",
            serde_json::json!({"op":op,"close_focus_candidates":candidates}),
            origin,
            view,
        );
    }
    pub(crate) fn prepare_forward(&mut self, ticket: u64, draft: super::super::forward::Draft) {
        let Some(pending) = self.commands.pending.get_mut(&ticket) else {
            return;
        };
        pending.forward_reserved = draft.weight().saturating_mul(2);
        pending.queued = Some(Work::PutPayload(draft.payload.clone()));
        pending.forward = Some(draft);
        pending.request.params = serde_json::Value::Null;
        self.refresh_command_weight(ticket);
        (self.wake)();
    }
}
impl crate::app::App {
    pub(crate) fn try_resolve_mirror_request(
        &mut self,
        ticket: u64,
        engine: EngineId,
        request: &JsonRpcRequest,
    ) -> bool {
        let Some(session) = self.engines.get(engine) else {
            return false;
        };
        let view = self
            .journal
            .completion_views
            .get(&engine)
            .cloned()
            .unwrap_or_default();
        let Some(op) = local_op(request, &session, &view) else {
            return false;
        };
        let mirrored = session
            .core
            .find_workspace_index_for_surface(op.anchor_surface_id())
            .and_then(|(index, _)| session.core.workspace_at(index))
            .is_some_and(|workspace| workspace.mirror);
        if !mirrored {
            return false;
        }
        let mut protected = request.clone();
        if request.method == "terminal.kill" {
            protected.method = "surface.close".into();
            protected.params["surface_id"] = serde_json::json!(op.anchor_surface_id());
        }
        if let Some(response) = super::close::caller_refusal(&protected, session.core) {
            self.journal.reject_resolved_request(ticket, response);
            return true;
        }
        let user = self.journal.commands.pending.get(&ticket).is_some_and(
            |pending| matches!(&pending.reply,Reply::Intent {origin,..} if origin.is_user()),
        );
        let candidates =
            serde_json::from_value::<Vec<u32>>(request.params["close_focus_candidates"].clone())
                .unwrap_or_default();
        match self.prepare_journal_forward(engine, &op, user, &candidates) {
            Ok(mut draft) => {
                if let Some(pending) = self.journal.commands.pending.get(&ticket)
                    && let Reply::Intent {
                        view: Some(view), ..
                    } = &pending.reply
                    && let Some(receipt) = &view.remote_request
                {
                    receipt.bind(draft.op_id);
                    if matches!(
                        op,
                        StructuralOp::NewTab { .. }
                            | StructuralOp::SplitPane { .. }
                            | StructuralOp::SplitSurface { .. }
                    ) {
                        draft.focus = None;
                    }
                }
                if request.method == "terminal.kill" {
                    draft.response = JsonRpcResponse::success(
                        serde_json::Value::Null,
                        serde_json::json!({"forwarded":true,"surface_id":op.anchor_surface_id(),"child_index":request.params["child"]}),
                    );
                }
                self.journal.prepare_forward(ticket, draft)
            }
            Err(error) => self.journal.reject_resolved_request(
                ticket,
                JsonRpcResponse::internal_error(serde_json::Value::Null, error.to_string()),
            ),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::kill_parent;
    use crate::core::child_parent::ParentKey;
    use serde_json::json;

    fn parent(params: serde_json::Value) -> Option<ParentKey> {
        kill_parent(&params, || Some(7))
    }

    /// 잘못된 parent_external 은 단일 부모로 폴백해 다른 부모의 자식을 닫지 않는다. 일반 해석이
    /// -32602 로 거절하도록 전달하지 않는다.
    #[test]
    fn an_invalid_external_parent_does_not_fall_back_to_another_parent() {
        for invalid in [
            json!(5),
            json!(["codex:t"]),
            json!({ "key": "codex:t" }),
            json!(true),
            json!(""),
            json!("a b"),
            json!("a\nb"),
            json!("x".repeat(257)),
        ] {
            // 전달하지 않은 요청은 kill 의 일반 해석(close.rs)이 같은 공통 검증으로 거절한다.
            let refused = crate::ipc::handler::params::optional_parent(
                &json!({ "parent_external": invalid, "child": 0 }),
                "surface",
                &json!(null),
            )
            .expect_err("refused");
            assert_eq!(refused.error.expect("error").code, -32602, "{invalid}");
            assert_eq!(
                parent(json!({ "parent_external": invalid, "child": 0 })),
                None,
                "{invalid}"
            );
            assert_eq!(
                parent(json!({ "surface": 9, "parent_external": invalid, "child": 0 })),
                None,
                "{invalid}"
            );
        }
    }

    #[test]
    fn valid_parents_and_the_omitted_fallback_are_unchanged() {
        assert_eq!(
            parent(json!({ "parent_external": "codex:t", "child": 0 })),
            Some(ParentKey::External("codex:t".into()))
        );
        assert_eq!(
            parent(json!({ "surface": 9, "child": 0 })),
            Some(ParentKey::Surface(9))
        );
        assert_eq!(parent(json!({ "child": 0 })), Some(ParentKey::Surface(7)));
        assert_eq!(
            parent(json!({ "parent_external": null, "child": 0 })),
            Some(ParentKey::Surface(7))
        );
        assert_eq!(
            parent(json!({ "surface": 9, "parent_external": "codex:t", "child": 0 })),
            None
        );
        assert_eq!(kill_parent(&json!({ "child": 0 }), || None), None);
    }
}
