//! Resolve View input once into explicit application targets. No structure or resource is changed.
use crate::app::command::{ConvertSurfaceTarget, DomainIntent};
use crate::intent::{ConvertTarget, Intent, IntentOrigin};
use crate::runtime::engine_access::EngineRef;
use crate::state::RequestContext;

pub(crate) fn resolve(
    state: &RequestContext,
    engine: &EngineRef<'_>,
    intent: &Intent,
    origin: &IntentOrigin,
) -> anyhow::Result<Option<DomainIntent>> {
    let read = engine.read();
    #[cfg(feature = "gui")]
    if origin.is_user()
        && let Some(pending) = state.attach_pending.selected(&read, &state.navigation)
    {
        // 변환은 입력 시점에 정한 surface ID 를 담고 있어 draft 를 대상으로 하지 않는다. draft 에 걸어 두었다가
        // 생성된 surface 에서 실행하는 변환도 그 사이 다른 draft 를 골랐다는 이유로 거절하지 않는다.
        let unsupported = matches!(
            intent,
            Intent::SplitPane { .. } | Intent::SplitSurface { .. }
        ) || (matches!(intent, Intent::NewTab { .. })
            && !matches!(
                pending.creation,
                Some(crate::state::attach_pending::Creation::Tab)
            ));
        if unsupported {
            anyhow::bail!("pending attach area is not a committed operation target");
        }
    }
    let params_or_empty = |params: &serde_json::Value| {
        if params.is_null() {
            serde_json::json!({})
        } else {
            params.clone()
        }
    };
    Ok(Some(match intent {
        #[cfg(feature = "gui")]
        Intent::PatchSettings(patch) => {
            let mut settings = engine.runtime.settings.clone();
            patch.apply(&mut settings);
            DomainIntent::UpdateSettings(settings)
        }
        Intent::ApplyPreset {
            kind,
            name,
            category,
            target_pane_id,
        } => DomainIntent::ApplyPreset {
            kind: *kind,
            name: name.clone(),
            target_pane_id: Some(target_pane_id.unwrap_or_else(|| state.focused_pane_id(&read))),
            category: *category,
        },
        Intent::RestoreClosedItem => DomainIntent::RestoreClosedItem {
            target_pane_id: Some(state.focused_pane_id(&read)),
            scope: crate::app::command::RestoreScope::Local,
        },
        #[cfg(feature = "gui")]
        Intent::PrepareTutorial { .. } => DomainIntent::CreateWorkspace {
            cwd: state.resolve_inherit_cwd(&read),
            kind: "terminal".into(),
            surface_params: serde_json::json!({}),
            name: None,
            subtitle: None,
            description: None,
            category: None,
        },
        Intent::NewWorkspace {
            kind,
            params,
            category,
        } => {
            let kind = kind.as_deref().unwrap_or("terminal");
            DomainIntent::CreateWorkspace {
                cwd: if kind == "terminal" && params.is_null() {
                    state.resolve_inherit_cwd(&read)
                } else {
                    None
                },
                kind: kind.into(),
                surface_params: params_or_empty(params),
                name: None,
                subtitle: None,
                description: None,
                category: *category,
            }
        }
        #[cfg(feature = "gui")]
        Intent::NewTabWithFollowup { pane_id, .. } => DomainIntent::CreateTab {
            pane_id: *pane_id,
            cwd: None,
            kind: "empty".into(),
            name: None,
            surface_params: serde_json::json!({}),
            activate: origin.is_user(),
        },
        Intent::NewTab { kind, params } => {
            let kind = kind.as_deref().unwrap_or("terminal");
            DomainIntent::CreateTab {
                pane_id: state.focused_pane_id(&read),
                cwd: if kind == "terminal" {
                    state.resolve_inherit_cwd(&read)
                } else {
                    None
                },
                kind: kind.into(),
                name: None,
                surface_params: params_or_empty(params),
                activate: origin.is_user(),
            }
        }
        Intent::SplitPane { direction } => DomainIntent::SplitPane {
            target_pane_id: state.focused_pane_id(&read),
            direction: *direction,
            cwd: state.resolve_inherit_cwd(&read),
            kind: "terminal".into(),
            surface_params: serde_json::json!({}),
        },
        Intent::SplitSurface { direction } => {
            let target = state
                .focused_surface_id(&read)
                .ok_or_else(|| anyhow::anyhow!("SplitSurface: no focused surface"))?;
            DomainIntent::SplitSurface {
                target_surface_id: target,
                direction: *direction,
                cwd: state.resolve_inherit_cwd_from_surface(&read, target),
                kind: "terminal".into(),
                surface_params: serde_json::json!({}),
            }
        }
        Intent::ConvertSurface { surface_id, target } => {
            let target = match target {
                ConvertTarget::Terminal => ConvertSurfaceTarget::Terminal {
                    cwd: state.resolve_inherit_cwd(&read),
                },
                ConvertTarget::Kind { kind, cwd, params } => {
                    let mut params = params.clone();
                    if let Some(definition) = engine.runtime.surface_registry.get(kind) {
                        definition.normalize_param_aliases(&mut params);
                    }
                    ConvertSurfaceTarget::Kind {
                        cwd: cwd
                            .clone()
                            .or_else(|| state.resolve_inherit_cwd_from_surface(&read, *surface_id)),
                        kind: kind.clone(),
                        params,
                    }
                }
            };
            DomainIntent::ConvertSurface {
                surface_id: *surface_id,
                target,
            }
        }
        _ => return Ok(None),
    }))
}
