//! Explicit mirror structural operation translation; execution belongs to journal Forward.
use super::*;
/// anchor는 로컬 surface ID이며 전송할 때 세션 매핑으로 바꾼다.
/// pane·tab 작업은 대표 surface를 찾는다. MoveSurface는 같은 workspace의 두 대상만 허용한다.
/// 다른 workspace의 로컬 ID를 보내면 원격의 무관한 surface ID와 겹칠 수 있다.
#[cfg(feature = "gui")]
pub(crate) fn build_mirror_forward_op(
    engine: &crate::core::CoreState,
    intent: &DomainIntent,
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
            inherit_cwd: None,
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
            inherit_cwd: None,
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
            inherit_cwd: None,
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
            // 명시된 cwd는 그대로 보낸다. 없으면 서버가 자체 설정에 따라 터미널 cwd를 조회한다.
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
                inherit_cwd: None,
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
            ),
            None
        );
    }
}
