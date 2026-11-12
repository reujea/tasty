//! Explicit mirror structural operation translation; execution belongs to journal Forward.
use super::*;
/// 원격 생성 요청에 싣는 cwd 상속 원본을 정하는 client 쪽 재료. 서버 View의 선택은 이 client의 선택과
/// 독립이므로, client가 자기 View 선택과 자기 설정으로 원본을 정해 요청에 싣는다.
#[cfg(feature = "gui")]
pub(crate) struct ClientCwd<'a> {
    /// 요청한 창의 View 선택. 창이 없는 임시 engine이면 없다(원본을 싣지 않는다).
    pub presentation: Option<&'a dyn crate::model::StructurePresentation>,
    /// client의 `general.inherit_cwd`.
    pub inherit: bool,
}

/// pane에서 View가 고른 탭의 선택 surface. 로컬 생성이 상속 원본으로 쓰는 것과 같은 surface다.
#[cfg(feature = "gui")]
pub(crate) fn view_selected_surface(
    core: &crate::core::CoreState,
    presentation: &dyn crate::model::StructurePresentation,
    pane: u32,
) -> Option<u32> {
    let pane = core.find_pane_by_id(pane)?;
    let tab = pane
        .tabs
        .get(presentation.tab_index(pane))
        .or_else(|| pane.tabs.first())?;
    presentation
        .surface_id(tab)
        .filter(|surface| tab.contains_surface(*surface))
        .or_else(|| tab.first_surface_id())
}

#[cfg(feature = "gui")]
impl ClientCwd<'_> {
    fn selected(&self, core: &crate::core::CoreState, pane: u32) -> Option<u32> {
        view_selected_surface(core, self.presentation?, pane)
    }

    fn of_pane(
        &self,
        core: &crate::core::CoreState,
        pane: u32,
    ) -> Option<tasty_ipc::stream::InheritCwd> {
        tasty_ipc::stream::InheritCwd::of_client(self.inherit, self.selected(core, pane))
    }

    /// surface 분할·변환은 대상 surface 자신이 원본이다.
    fn of_surface(&self, surface: u32) -> Option<tasty_ipc::stream::InheritCwd> {
        tasty_ipc::stream::InheritCwd::of_client(self.inherit, Some(surface))
    }
}

#[cfg(feature = "gui")]
impl crate::app::App {
    /// mirror 대상 구조 변경이면 원격에 보낼 op다. 요청한 창의 View 선택과 client 설정으로 cwd 상속 원본을 싣는다.
    pub(crate) fn mirror_forward_op(
        &mut self,
        id: crate::runtime::engine_session::EngineId,
        intent: &DomainIntent,
    ) -> Option<tasty_ipc::stream::StructuralOp> {
        if let Some(context) = self.engines_mut().resolve(id) {
            let engine = context.engine.as_ref();
            engine.mirror_workspace_index_for_structural(intent)?;
            let client = ClientCwd {
                presentation: Some(&context.state.navigation),
                inherit: engine.runtime.settings.general.inherit_cwd,
            };
            return build_mirror_forward_op(engine.core, intent, &client);
        }
        let session = self.engines.get(id)?;
        session.mirror_workspace_index_for_structural(intent)?;
        let client = ClientCwd {
            presentation: None,
            inherit: session.runtime.settings.general.inherit_cwd,
        };
        build_mirror_forward_op(session.core, intent, &client)
    }
}

/// anchor는 로컬 surface ID이며 전송할 때 세션 매핑으로 바꾼다.
/// pane·tab 작업은 대표 surface를 찾는다. MoveSurface는 같은 workspace의 두 대상만 허용한다.
/// 다른 workspace의 로컬 ID를 보내면 원격의 무관한 surface ID와 겹칠 수 있다.
#[cfg(feature = "gui")]
pub(crate) fn build_mirror_forward_op(
    engine: &crate::core::CoreState,
    intent: &DomainIntent,
    client: &ClientCwd<'_>,
) -> Option<tasty_ipc::stream::StructuralOp> {
    use crate::app::command::DomainIntent as D;
    use tasty_ipc::stream::{SplitAxis, StructuralOp};

    fn axis(d: &crate::model::SplitDirection) -> SplitAxis {
        match d {
            crate::model::SplitDirection::Horizontal => SplitAxis::Horizontal,
            crate::model::SplitDirection::Vertical => SplitAxis::Vertical,
        }
    }
    let pane_anchor = |pane_id: u32| -> Option<u32> {
        engine
            .find_pane_by_id(pane_id)
            .and_then(|p| p.tabs.first())
            .and_then(|t| t.first_surface_id())
    };
    let tab_anchor = |tab_id: u32| -> Option<u32> {
        for ws in &engine.workspaces() {
            for pid in ws.pane_layout().all_pane_ids() {
                if let Some(pane) = ws.pane_layout().find_pane(pid) {
                    for tab in &pane.tabs {
                        if tab.id == tab_id {
                            return tab.first_surface_id();
                        }
                    }
                }
            }
        }
        None
    };

    match intent {
        D::SplitSurface {
            target_surface_id,
            direction,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::SplitSurface {
            surface_id: *target_surface_id,
            direction: axis(direction),
            surface_kind: kind.clone(),
            params: surface_params.clone(),
            inherit_cwd: client.of_surface(*target_surface_id),
        }),
        D::SplitPane {
            target_pane_id,
            direction,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::SplitPane {
            anchor_surface_id: pane_anchor(*target_pane_id)?,
            direction: axis(direction),
            surface_kind: kind.clone(),
            params: surface_params.clone(),
            inherit_cwd: client.of_pane(engine, *target_pane_id),
        }),
        D::CreateTab {
            pane_id,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::NewTab {
            anchor_surface_id: pane_anchor(*pane_id)?,
            surface_kind: kind.clone(),
            params: surface_params.clone(),
            inherit_cwd: client.of_pane(engine, *pane_id),
        }),
        // 서버는 anchor가 속한 workspace가 이 연결이 점유한 workspace인지 확인한 뒤 전체를 닫는다.
        D::CloseWorkspace { workspace_id } => Some(StructuralOp::CloseWorkspace {
            anchor_surface_id: engine
                .find_workspace_index_for_id(*workspace_id)
                .and_then(|index| engine.workspace_at(index))
                .and_then(|workspace| workspace.all_surface_ids().first().copied())?,
        }),
        D::CloseSurface { surface_id, .. } => Some(StructuralOp::CloseSurface {
            surface_id: *surface_id,
        }),
        D::CloseTab { tab_id } => Some(StructuralOp::CloseTab {
            anchor_surface_id: tab_anchor(*tab_id)?,
        }),
        D::ClosePane { pane_id } => Some(StructuralOp::ClosePane {
            anchor_surface_id: pane_anchor(*pane_id)?,
        }),
        // 복원할 항목은 서버의 복원 목록에서 고르므로 anchor만 보낸다.
        D::RestoreClosedItem { target_pane_id, .. } => Some(StructuralOp::RestoreClosedItem {
            anchor_surface_id: pane_anchor((*target_pane_id)?)?,
        }),
        D::MoveTab {
            pane_id,
            tab_id,
            to_index,
        } => Some(StructuralOp::MoveTab {
            anchor_surface_id: pane_anchor(*pane_id)?,
            from_index: engine
                .find_pane_by_id(*pane_id)?
                .tabs
                .iter()
                .position(|tab| tab.id == *tab_id)?,
            to_index: *to_index,
        }),
        D::ConvertSurface { surface_id, target } => {
            use crate::app::command::ConvertSurfaceTarget;
            // 명시된 cwd는 그대로 보낸다. 없으면 서버가 상속 원본(변환할 surface)의 cwd를 조회한다.
            let (surface_kind, params, cwd) = match target {
                ConvertSurfaceTarget::Terminal { cwd } => {
                    ("terminal".to_string(), serde_json::json!({}), cwd.clone())
                }
                ConvertSurfaceTarget::Kind { kind, params, cwd } => {
                    (kind.clone(), params.clone(), cwd.clone())
                }
            };
            Some(StructuralOp::ConvertSurface {
                surface_id: *surface_id,
                surface_kind,
                params,
                cwd: cwd.map(|p| p.to_string_lossy().into_owned()),
                inherit_cwd: client.of_surface(*surface_id),
            })
        }
        D::MoveSurface {
            source_surface_id,
            target_surface_id,
        } => {
            let src_ws = engine
                .find_workspace_index_for_surface(*source_surface_id)
                .map(|(i, _)| i);
            let tgt_ws = engine
                .find_workspace_index_for_surface(*target_surface_id)
                .map(|(i, _)| i);
            if src_ws.is_some() && src_ws == tgt_ws {
                Some(StructuralOp::MoveSurface {
                    source_surface_id: *source_surface_id,
                    target_surface_id: *target_surface_id,
                })
            } else {
                None
            }
        }
        // 이름·카테고리는 구조가 아니라서 위 분류가 mirror 차단 대상으로 고르지 않는다.
        D::SetWorkspaceCategory { .. }
        | D::CreateCategory { .. }
        | D::RenameCategory { .. }
        | D::DeleteCategory { .. } => None,
        _ => None,
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;

    const NO_VIEW: ClientCwd<'static> = ClientCwd {
        presentation: None,
        inherit: true,
    };

    /// workspace 닫기는 그 workspace의 surface 하나를 anchor로 보낸다. 없는 workspace는 보내지 않는다.
    #[test]
    fn a_workspace_close_is_anchored_on_one_of_its_surfaces() {
        let (_, session) = crate::state::tests::test_state();
        let workspace = session.core_state.local_workspaces()[0].id;
        let surfaces = session.core_state.local_workspaces()[0].all_surface_ids();
        let op = build_mirror_forward_op(
            &session.core_state,
            &DomainIntent::CloseWorkspace {
                workspace_id: workspace,
            },
            &NO_VIEW,
        );
        assert_eq!(
            op,
            Some(tasty_ipc::stream::StructuralOp::CloseWorkspace {
                anchor_surface_id: surfaces[0],
            })
        );
        assert_eq!(
            build_mirror_forward_op(
                &session.core_state,
                &DomainIntent::CloseWorkspace {
                    workspace_id: workspace + 1000,
                },
                &NO_VIEW,
            ),
            None
        );
    }

    fn creations() -> [DomainIntent; 4] {
        let terminal = |kind: &str| (kind.to_string(), serde_json::json!({}));
        let (kind, surface_params) = terminal("terminal");
        [
            DomainIntent::CreateTab {
                pane_id: 10,
                cwd: None,
                kind: kind.clone(),
                name: None,
                surface_params: surface_params.clone(),
                activate: true,
            },
            DomainIntent::SplitPane {
                target_pane_id: 10,
                direction: crate::model::SplitDirection::Vertical,
                cwd: None,
                kind: kind.clone(),
                surface_params: surface_params.clone(),
            },
            DomainIntent::SplitSurface {
                target_surface_id: 201,
                direction: crate::model::SplitDirection::Vertical,
                cwd: None,
                kind,
                surface_params,
            },
            DomainIntent::ConvertSurface {
                surface_id: 201,
                target: crate::app::command::ConvertSurfaceTarget::Terminal { cwd: None },
            },
        ]
    }

    /// 새 탭·pane 분할은 client View가 그 pane에서 고른 탭(둘째 탭 102)을, surface 분할·변환은 대상
    /// surface를 상속 원본으로 싣는다. client 설정이 꺼져 있으면 "상속 안 함"을 싣는다.
    #[test]
    fn creations_carry_the_client_selected_source() {
        use tasty_ipc::stream::InheritCwd;
        let (mut state, session) = crate::state::tests::two_pane_fixture();
        let core = &session.core_state;
        state
            .navigation
            .goto_tab(core.find_pane_by_id(10).expect("pane 10"), 1);
        let sources = |inherit: bool| {
            let client = ClientCwd {
                presentation: Some(&state.navigation),
                inherit,
            };
            creations()
                .iter()
                .map(|intent| {
                    build_mirror_forward_op(core, intent, &client)
                        .expect("creation op")
                        .inherit_cwd()
                        .cloned()
                })
                .collect::<Vec<_>>()
        };
        let (second_tab, target) = (
            Some(InheritCwd::Surface(102)),
            Some(InheritCwd::Surface(201)),
        );
        assert_eq!(
            sources(true),
            vec![second_tab.clone(), second_tab, target.clone(), target]
        );
        assert_eq!(sources(false), vec![Some(InheritCwd::NoInherit); 4]);
    }

    /// View가 없는 임시 engine은 고를 수 없으므로 원본을 싣지 않는다(서버 fallback). 설정이 꺼져 있으면 그래도 "상속 안 함"이다.
    #[test]
    fn without_a_view_a_pane_creation_carries_no_source() {
        use tasty_ipc::stream::InheritCwd;
        let (_, session) = crate::state::tests::two_pane_fixture();
        let tab = &creations()[0];
        let op = |inherit| {
            build_mirror_forward_op(
                &session.core_state,
                tab,
                &ClientCwd {
                    presentation: None,
                    inherit,
                },
            )
            .expect("creation op")
        };
        assert_eq!(op(true).inherit_cwd(), None);
        assert_eq!(op(false).inherit_cwd(), Some(&InheritCwd::NoInherit));
    }
}
