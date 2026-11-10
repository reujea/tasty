use super::common::{batch, scenario_batches};
use crate::*;

fn initial() -> JournalModel {
    let mut model = JournalModel::default();
    for input in scenario_batches().into_iter().take(3) {
        evolve(&mut model, &input).unwrap();
    }
    model
}

fn execute(model: &mut JournalModel, cmd: &StructuralCommand) -> StructuralDecision {
    let before = model.clone();
    let decision = decide_structure(model, cmd).unwrap();
    assert_eq!(*model, before, "decide must not publish its candidate");
    let input = batch(
        model.applied.batch.unwrap() + 1,
        model.applied.revision.unwrap(),
        &decision.events,
    );
    evolve(model, &input).unwrap();
    decision
}

#[test]
fn category_delete_preserves_workspace_order_and_moves_members_to_normal() {
    let mut model = initial();
    let order = model.workspace_order.clone();
    execute(&mut model, &StructuralCommand::DeleteCategory { id: 1 });
    assert_eq!(model.workspace_order, order);
    assert!(model.workspaces.values().all(|w| w.category == 0));
    assert!(!model.categories.contains_key(&1));
    assert_eq!(
        decide_structure(&model, &StructuralCommand::DeleteCategory { id: 0 })
            .unwrap_err()
            .0,
        "the 'normal' category cannot be renamed or deleted"
    );
}

#[test]
fn optional_metadata_fields_preserve_unspecified_values_and_empty_names() {
    let mut model = initial();
    let surfaces = model.surfaces.clone();
    execute(
        &mut model,
        &StructuralCommand::UpdateWorkspaceMeta {
            workspace_id: 1,
            name: None,
            subtitle: Some("kept".into()),
            description: Some("old".into()),
        },
    );
    execute(
        &mut model,
        &StructuralCommand::UpdateWorkspaceMeta {
            workspace_id: 1,
            name: Some(String::new()),
            subtitle: None,
            description: Some("new".into()),
        },
    );
    assert_eq!(model.workspaces[&1].name, "");
    assert_eq!(model.workspaces[&1].subtitle, "kept");
    assert_eq!(model.workspaces[&1].description, "new");
    execute(
        &mut model,
        &StructuralCommand::RenameTab {
            tab_id: 1,
            name: Some(String::new()),
        },
    );
    assert_eq!(model.tabs[&1].explicit_name.as_deref(), Some(""));
    assert_eq!(model.surfaces, surfaces);
}

#[test]
fn queued_reordering_keeps_its_explicit_source_and_invalid_moves_stay_noops() {
    let mut model = initial();
    let queued = StructuralCommand::MoveWorkspace {
        workspace_id: 1,
        to_index: 1,
    };
    execute(
        &mut model,
        &StructuralCommand::MoveWorkspace {
            workspace_id: 2,
            to_index: 0,
        },
    );
    let response = execute(&mut model, &queued);
    assert_eq!(model.workspace_order, vec![2, 1]);
    assert_eq!(response.result, StructuralResult::Moved { moved: false });
    let invalid = execute(
        &mut model,
        &StructuralCommand::MoveWorkspace {
            workspace_id: 999,
            to_index: 0,
        },
    );
    assert!(invalid.events.is_empty());
    assert_eq!(invalid.result, StructuralResult::Moved { moved: false });
}

#[test]
fn ratio_target_precondition_prevents_a_delayed_drag_from_touching_another_split() {
    let model = initial();
    let before = model.clone();
    assert!(
        decide_structure(
            &model,
            &StructuralCommand::SetSurfaceRatio {
                tab_id: 1,
                path: vec![],
                expected_revision: model.applied.revision.unwrap(),
                expected_leaves: vec![999, 1],
                ratio: Ratio::from_f32(0.5)
            }
        )
        .is_err()
    );
    assert_eq!(model, before);
    let result = decide_structure(
        &model,
        &StructuralCommand::SetSurfaceRatio {
            tab_id: 1,
            path: vec![],
            expected_revision: model.applied.revision.unwrap(),
            expected_leaves: vec![3, 1],
            ratio: Ratio::from_f32(0.5),
        },
    )
    .unwrap();
    assert_eq!(result.events.len(), 1);
}

#[test]
fn a_recreated_split_with_the_same_leaves_rejects_the_old_revision() {
    let mut model = initial();
    let old = StructuralCommand::SetSurfaceRatio {
        tab_id: 1,
        path: vec![],
        expected_leaves: vec![3, 1],
        expected_revision: model.applied.revision.unwrap(),
        ratio: Ratio::from_f32(0.6),
    };
    assert!(decide_structure(&model, &old).is_ok());
    // A later committed structure with identical leaves must still reject the old drag.
    model.applied.revision = Some(model.applied.revision.unwrap() + 2);
    let before = model.clone();
    assert_eq!(
        decide_structure(&model, &old).unwrap_err().0,
        "split changed since the drag began"
    );
    assert_eq!(model, before);
}

fn remote_row(index: usize, category: u32) -> RemoteRow {
    RemoteRow {
        name: "build".into(),
        subtitle: String::new(),
        description: String::new(),
        category,
        index,
        binding: tasty_model::RemoteBinding {
            profile: "mac".into(),
            remote_workspace: 4,
        },
    }
}

#[test]
fn a_saved_remote_row_is_stored_changed_and_removed_without_touching_local_order() {
    let mut model = initial();
    let order = model.workspace_order.clone();
    execute(
        &mut model,
        &StructuralCommand::SetRemoteRow {
            id: 50,
            row: remote_row(1, 999),
        },
    );
    assert_eq!(
        model.remote_rows[&50].category,
        tasty_model::NORMAL_CATEGORY_ID
    );
    assert_eq!(model.workspace_order, order);
    let same = execute(
        &mut model,
        &StructuralCommand::SetRemoteRow {
            id: 50,
            row: remote_row(1, 999),
        },
    );
    assert!(same.events.is_empty(), "an unchanged row emits nothing");
    execute(&mut model, &StructuralCommand::DeleteCategory { id: 1 });
    assert!(
        decide_structure(
            &model,
            &StructuralCommand::SetRemoteRow {
                id: order[0],
                row: remote_row(0, 0),
            },
        )
        .is_err(),
        "a local workspace id cannot become a remote row"
    );
    execute(&mut model, &StructuralCommand::RemoveRemoteRow { id: 50 });
    assert!(model.remote_rows.is_empty());
    let missing = execute(&mut model, &StructuralCommand::RemoveRemoteRow { id: 50 });
    assert!(missing.events.is_empty());
}

#[test]
fn deleting_a_category_moves_its_saved_remote_rows_to_normal() {
    let mut model = initial();
    execute(
        &mut model,
        &StructuralCommand::SetRemoteRow {
            id: 50,
            row: remote_row(0, 1),
        },
    );
    assert_eq!(model.remote_rows[&50].category, 1);
    execute(&mut model, &StructuralCommand::DeleteCategory { id: 1 });
    assert_eq!(
        model.remote_rows[&50].category,
        tasty_model::NORMAL_CATEGORY_ID
    );
}

#[test]
fn bootstrap_places_saved_remote_rows_as_bare_mirrors_at_their_index() {
    let mut model = initial();
    // live projection은 metadata를 따로 싣는다. 구조 비교만 남긴다.
    model
        .workspaces
        .values_mut()
        .for_each(|w| w.metadata.clear());
    model.surfaces.values_mut().for_each(|s| s.metadata.clear());
    for (id, index) in [(50, 0), (51, 99)] {
        execute(
            &mut model,
            &StructuralCommand::SetRemoteRow {
                id,
                row: remote_row(index, 0),
            },
        );
    }
    let mut core = CoreState::new_base();
    projection::bootstrap::initialize(&mut core, &model).unwrap();
    let ids: Vec<_> = core.workspaces().into_iter().map(|w| w.id).collect();
    assert_eq!(ids.first(), Some(&50));
    assert_eq!(ids.last(), Some(&51));
    let row = core.workspace_at(0).unwrap();
    assert!(row.is_saved_remote_row() && row.is_bare());
    assert_eq!(row.remote_row.as_ref().unwrap().profile, "mac");
}

#[test]
fn a_remote_projection_keeps_the_saved_row_name_and_binding() {
    let mut core = CoreState::new_base();
    let binding = tasty_model::RemoteBinding {
        profile: "mac".into(),
        remote_workspace: 4,
    };
    let mut row = tasty_model::Workspace::new_remote_row(50, "mine".into(), binding.clone());
    row.category = 3;
    core.insert_mirror_workspace(0, row);
    let mut remote = tasty_model::Workspace::new_with_pane(50, "theirs".into(), Default::default());
    remote.mirror = true;
    assert!(core.replace_mirror_workspace(remote).is_ok());
    let ws = core.workspace_at(0).unwrap();
    assert_eq!(ws.name, "mine");
    assert_eq!(ws.category, 3);
    assert_eq!(ws.remote_row.as_ref(), Some(&binding));
}
