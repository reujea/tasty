use super::*;
use crate::model::PhysicalPx;
fn rect() -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1000.0),
        height: PhysicalPx(600.0),
    }
}
fn tab(anchor: u32) -> StructuralOp {
    StructuralOp::NewTab {
        anchor_surface_id: anchor,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd: None,
    }
}

#[test]
fn prelayout_has_no_domain_or_input_target_and_failure_removes_only_its_draft() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let original = core.live_surface_ids();
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let first = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .unwrap();
    first.bind(0);
    assert_eq!(state.focused_surface_id(core), None);
    assert_eq!(state.pending_regions(core, rect(), 1.0).len(), 1);
    let second = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .unwrap();
    second.bind(2);
    state.attach_pending.result(&sender.epoch(), 0, None, false);
    state.attach_pending.retain(core);
    assert_eq!(state.attach_pending.entries.len(), 1);
    assert_eq!(state.attach_pending.entries[0].receipt.op(), Some(2));
    assert_eq!(core.live_surface_ids(), original);
    assert!(!state.close_active_tab(&engine.read()));
    assert!(!state.close_active_pane(&engine.read()));
    state.attach_pending.deselect();
    assert_eq!(state.focused_surface_id(core), Some(anchor));
}

#[test]
fn both_split_axes_use_normal_geometry_and_only_clip_real_resources() {
    for direction in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        for pane_split in [false, true] {
            let (mut state, engine) = crate::state::tests::test_mirror_state();
            let core = &engine.core_state;
            let anchor = state.focused_surface_id(core).unwrap();
            let (sender, _queue) = tasty_remote::connection::channel();
            let view = Arc::new(());
            let op = if pane_split {
                StructuralOp::SplitPane {
                    anchor_surface_id: anchor,
                    direction,
                    surface_kind: "terminal".into(),
                    params: serde_json::json!({}),
                    inherit_cwd: None,
                }
            } else {
                StructuralOp::SplitSurface {
                    surface_id: anchor,
                    direction,
                    surface_kind: "terminal".into(),
                    params: serde_json::json!({}),
                    inherit_cwd: None,
                }
            };
            state
                .attach_pending
                .begin(
                    core,
                    &state.navigation,
                    &op,
                    sender.epoch(),
                    Arc::downgrade(&view),
                    std::time::Instant::now(),
                )
                .unwrap();
            let placeholders = state.pending_regions(core, rect(), 1.0);
            assert_eq!(placeholders.len(), 1);
            assert_eq!(placeholders[0].2, pane_split);
            assert!(
                placeholders[0].1.width.value() > 0.0 && placeholders[0].1.height.value() > 0.0
            );
            assert!(state.attach_pending.clips_surface(core, anchor));
            assert_eq!(state.focused_surface_id(core), None);
            assert_eq!(core.live_surface_ids().len(), 1);
        }
    }
}

#[test]
fn result_waits_for_applied_delta_and_preserves_user_selection() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    let receipt = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    receipt.bind(1);
    state
        .attach_pending
        .result(&sender.epoch(), 1, Some(99), true);
    assert!(!receipt.finished(), "Result alone is not applied structure");
    state.attach_pending.deselect();
    let mapping = [(99, anchor)].into_iter().collect();
    assert_eq!(
        state.attach_pending.applied(
            core,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &mapping,
            &[99]
        ),
        None
    );
    assert!(receipt.finished());
}

#[test]
fn retired_epoch_cannot_reappear_or_become_a_new_connection_result() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (old, _queue) = tasty_remote::connection::channel();
    let (new, _new_queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let receipt = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            old.epoch(),
            Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .unwrap();
    receipt.bind(1);
    state.attach_pending.result(&new.epoch(), 1, Some(99), true);
    assert!(!receipt.has_result());
    old.retire();
    state.attach_pending.retain(core);
    assert!(state.attach_pending.entries.is_empty());
    assert_eq!(state.focused_surface_id(core), Some(anchor));
}

#[test]
fn a_closed_view_keeps_the_request_but_never_recreates_its_layout_or_focus() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let receipt = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .unwrap();
    receipt.bind(0);
    drop(view);
    assert!(state.pending_regions(core, rect(), 1.0).is_empty());
    assert_eq!(state.focused_surface_id(core), Some(anchor));
    assert_eq!(state.attach_pending.entries.len(), 1);
    state
        .attach_pending
        .result(&sender.epoch(), 0, Some(99), true);
    assert_eq!(
        state.attach_pending.applied(
            core,
            &state.navigation,
            &sender.epoch(),
            None,
            &[(99, anchor)].into_iter().collect(),
            &[99]
        ),
        None
    );
    assert!(receipt.finished());
}

#[test]
fn an_exact_creation_result_does_not_fold_other_unsent_drafts() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    let a = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    a.bind(0);
    let b = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    state
        .attach_pending
        .result(&sender.epoch(), 0, Some(99), true);
    assert_eq!(
        state.attach_pending.applied(
            core,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &[(99, anchor)].into_iter().collect(),
            &[99]
        ),
        None
    );
    assert!(a.finished());
    assert!(!b.finished());
    assert!(
        state
            .attach_pending
            .entries
            .iter()
            .filter(|entry| !entry.receipt.finished())
            .all(|entry| entry.visible)
    );
    assert_eq!(state.pending_regions(core, rect(), 1.0).len(), 1);
    assert_eq!(state.focused_surface_id(core), None);
}

#[test]
fn keyboard_tab_navigation_uses_the_same_real_and_pending_order_as_the_strip() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    for _ in 0..2 {
        state
            .attach_pending
            .begin(
                core,
                &state.navigation,
                &tab(anchor),
                sender.epoch(),
                Arc::downgrade(&view),
                std::time::Instant::now(),
            )
            .unwrap();
    }
    let ids: Vec<_> = state
        .attach_pending
        .entries
        .iter()
        .map(|entry| entry.id)
        .collect();
    state.prev_tab_in_pane(core);
    assert_eq!(
        state
            .attach_pending
            .selected(core, &state.navigation)
            .unwrap()
            .id,
        ids[0]
    );
    state.prev_tab_in_pane(core);
    assert_eq!(state.focused_surface_id(core), Some(anchor));
    state.next_tab_in_pane(core);
    assert_eq!(
        state
            .attach_pending
            .selected(core, &state.navigation)
            .unwrap()
            .id,
        ids[0]
    );
    assert_eq!(
        state.goto_tab_in_pane(core, 2),
        tasty_model::TabSwitch::Switched
    );
    assert_eq!(
        state
            .attach_pending
            .selected(core, &state.navigation)
            .unwrap()
            .id,
        ids[1]
    );
    assert_eq!(
        state.goto_tab_in_pane(core, 3),
        tasty_model::TabSwitch::OutOfRange { tabs: 3 }
    );
    state.next_tab_in_pane(core);
    assert_eq!(state.focused_surface_id(core), Some(anchor));
}

#[test]
fn exact_result_after_delta_finishes_only_its_receipt_without_stealing_selection() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    let a = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    a.bind(0);
    let b = state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &tab(anchor),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    b.bind(1);
    let mapping = [(99, anchor)].into_iter().collect();
    assert_eq!(
        state.attach_pending.applied(
            core,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &mapping,
            &[99]
        ),
        None
    );
    assert!(!a.finished());
    state
        .attach_pending
        .result(&sender.epoch(), 0, Some(99), true);
    assert_eq!(
        state.attach_pending.applied_before_result(
            &sender.epoch(),
            0,
            &state.navigation,
            Some(&identity),
            &mapping
        ),
        None
    );
    assert!(a.finished());
    assert!(!b.finished());
    assert_eq!(state.focused_surface_id(core), None);
}

#[test]
fn noncreation_requests_wait_for_result_and_delta_without_prelayout_or_focus_change() {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let core = &engine.core_state;
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    for (index, op) in [
        StructuralOp::CloseSurface { surface_id: anchor },
        StructuralOp::CloseTab {
            anchor_surface_id: anchor,
        },
        StructuralOp::ClosePane {
            anchor_surface_id: anchor,
        },
        StructuralOp::MoveTab {
            anchor_surface_id: anchor,
            from_index: 0,
            to_index: 1,
        },
        StructuralOp::RestoreClosedItem {
            anchor_surface_id: anchor,
        },
        StructuralOp::ConvertSurface {
            surface_id: anchor,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            cwd: None,
            inherit_cwd: None,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let receipt = state
            .attach_pending
            .begin(
                core,
                &state.navigation,
                &op,
                sender.epoch(),
                Arc::downgrade(&view),
                std::time::Instant::now(),
            )
            .unwrap();
        receipt.bind(index as u64);
        assert!(state.pending_regions(core, rect(), 1.0).is_empty());
        assert_eq!(state.focused_surface_id(core), Some(anchor));
        state
            .attach_pending
            .result(&sender.epoch(), index as u64, None, true);
        assert!(!receipt.finished());
        state.attach_pending.applied(
            core,
            &state.navigation,
            &sender.epoch(),
            None,
            &Default::default(),
            &[],
        );
        assert!(receipt.finished());
        state.attach_pending.retain(core);
    }
}

mod correlation;
