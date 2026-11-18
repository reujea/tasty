//! Resolve View input once into explicit application targets. No structure or resource is changed.
//! 구조 명령의 대상은 입력 시점에 intent에 실려 온다. 해석은 포커스를 다시 읽지 않으며, 대상이 그 사이
//! 사라졌으면 다른 대상으로 넘기지 않고 버린다.
use crate::app::command::{ConvertSurfaceTarget, DomainIntent};
use crate::intent::{ConvertTarget, Intent, IntentOrigin};
use crate::runtime::engine_access::EngineRef;
use crate::state::RequestContext;

/// `resolve` 결과.
#[derive(Debug)]
pub(crate) enum Resolved {
    /// 명시 대상을 담은 도메인 명령. 다른 갈래는 비어 있어 크기 차이를 줄이려 상자에 담는다.
    Domain(Box<DomainIntent>),
    /// 입력 때 정한 대상이 없거나 사라져 실행하지 않는다. 이유는 info 로그로 남겼다.
    Dropped,
    /// 해석할 생성 intent가 아니다. 그대로 다음 단계로 넘긴다.
    Unchanged,
}

impl Resolved {
    #[cfg(all(test, feature = "gui"))]
    pub(crate) fn domain(self) -> Option<DomainIntent> {
        match self {
            Self::Domain(intent) => Some(*intent),
            Self::Dropped | Self::Unchanged => None,
        }
    }
}

/// 입력 때 정한 pane. 없거나 사라졌으면 이유를 남기고 None이다.
fn bound_pane(read: &crate::core::CoreState, pane: Option<u32>, what: &str) -> Option<u32> {
    match pane {
        Some(pane) if read.find_pane_by_id(pane).is_some() => Some(pane),
        Some(pane) => {
            tracing::info!("{what} dropped: its target pane {pane} is gone");
            None
        }
        None => {
            tracing::info!("{what} dropped: no target pane was chosen at input");
            None
        }
    }
}

pub(crate) fn resolve(
    state: &RequestContext,
    engine: &EngineRef<'_>,
    intent: &Intent,
    origin: &IntentOrigin,
) -> anyhow::Result<Resolved> {
    let read = engine.read();
    // cwd 상속 원본은 입력 때 정한 surface이고 cwd는 지금 그 surface에서 읽는다. 원본이 사라졌으면 상속하지 않는다.
    let cwd_of = |source: Option<u32>| {
        source.and_then(|surface| state.resolve_inherit_cwd_from_surface(&read, surface))
    };
    let params_or_empty = |params: &serde_json::Value| {
        if params.is_null() {
            serde_json::json!({})
        } else {
            params.clone()
        }
    };
    Ok(Resolved::Domain(Box::new(match intent {
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
        Intent::RestoreClosedItem { pane } => DomainIntent::RestoreClosedItem {
            target_pane_id: match pane {
                Some(_) => match bound_pane(&read, *pane, "restore closed item") {
                    Some(pane) => Some(pane),
                    None => return Ok(Resolved::Dropped),
                },
                None => None,
            },
            scope: crate::app::command::RestoreScope::Local,
        },
        #[cfg(feature = "gui")]
        Intent::PrepareTutorial { cwd_source, .. } => DomainIntent::CreateWorkspace {
            cwd: cwd_of(*cwd_source),
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
            cwd_source,
        } => {
            let kind = kind.as_deref().unwrap_or("terminal");
            DomainIntent::CreateWorkspace {
                cwd: if kind == "terminal" && params.is_null() {
                    cwd_of(*cwd_source)
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
        Intent::NewTabWithFollowup {
            pane_id,
            cwd_source,
            ..
        } => DomainIntent::CreateTab {
            pane_id: *pane_id,
            cwd: None,
            cwd_source: *cwd_source,
            kind: "empty".into(),
            name: None,
            surface_params: serde_json::json!({}),
            activate: origin.is_user(),
        },
        Intent::NewTab {
            kind,
            params,
            target,
        } => {
            let Some(pane_id) = bound_pane(&read, target.pane, "new tab") else {
                return Ok(Resolved::Dropped);
            };
            let kind = kind.as_deref().unwrap_or("terminal");
            DomainIntent::CreateTab {
                pane_id,
                cwd: if kind == "terminal" {
                    cwd_of(target.cwd_source)
                } else {
                    None
                },
                cwd_source: target.cwd_source,
                kind: kind.into(),
                name: None,
                surface_params: params_or_empty(params),
                activate: origin.is_user(),
            }
        }
        Intent::SplitPane { direction, target } => {
            let Some(target_pane_id) = bound_pane(&read, target.pane, "split pane") else {
                return Ok(Resolved::Dropped);
            };
            DomainIntent::SplitPane {
                target_pane_id,
                direction: *direction,
                cwd: cwd_of(target.cwd_source),
                cwd_source: target.cwd_source,
                kind: "terminal".into(),
                surface_params: serde_json::json!({}),
            }
        }
        Intent::SplitSurface { direction, surface } => {
            let target = match surface {
                Some(surface) if read.has_surface(*surface) => *surface,
                Some(surface) => {
                    tracing::info!("split surface dropped: its target surface {surface} is gone");
                    return Ok(Resolved::Dropped);
                }
                None => {
                    tracing::info!("split surface dropped: no target surface was chosen at input");
                    return Ok(Resolved::Dropped);
                }
            };
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
                // 터미널로 바꿀 때도 cwd는 바꾸는 surface 자신에서 읽는다(Kind 갈래와 같다).
                ConvertTarget::Terminal => ConvertSurfaceTarget::Terminal {
                    cwd: state.resolve_inherit_cwd_from_surface(&read, *surface_id),
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
        _ => return Ok(Resolved::Unchanged),
    })))
}
