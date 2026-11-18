//! View close producers preserve their target and origin; durable undo and retirement are App-owned.
use crate::app::command::DomainIntent;
use crate::intent::Intent;
use crate::state::tests::test_state;

#[test]
fn surface_close_fixes_the_target_and_snapshot_request_without_closing_it() {
    let (mut state, mut owner) = test_state();
    let engine = owner.borrow_mut();
    let sid = state.focused_surface_id(&engine).unwrap();
    assert!(state.close_surface_by_id(&engine.read(), sid, true));
    assert!(engine.has_surface(sid));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    let Intent::Domain(DomainIntent::CloseSurface {
        surface_id,
        presentation,
    }) = &intents[0].body
    else {
        panic!("close request")
    };
    assert_eq!(*surface_id, sid);
    assert!(presentation.is_some());
    assert_eq!(intents[0].origin.is_user(), cfg!(feature = "gui"));
}

#[test]
fn an_agent_close_has_no_user_origin_and_can_omit_the_view_snapshot() {
    let (mut state, mut owner) = test_state();
    let engine = owner.borrow_mut();
    let sid = state.focused_surface_id(&engine).unwrap();
    assert!(state.close_surface_by_id_no_snapshot(&engine.read(), sid, false));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert!(!intents[0].origin.is_user());
    assert!(
        matches!(&intents[0].body, Intent::Domain(DomainIntent::CloseSurface { surface_id, presentation: None }) if *surface_id == sid)
    );
    assert!(engine.has_surface(sid));
}

#[test]
fn missing_surface_does_not_queue_a_close() {
    let (mut state, mut owner) = test_state();
    let engine = owner.borrow_mut();
    assert!(!state.close_surface_by_id(&engine.read(), u32::MAX, true));
    assert!(state.take_pending_intents().is_empty());
}

#[cfg(feature = "gui")]
#[test]
fn active_pane_and_tab_closes_preserve_the_original_ids_and_user_origin() {
    let (mut state, mut owner) = test_state();
    let engine = owner.borrow_mut();
    let pane = state.focused_pane_id(&engine);
    let tab = state.focused_pane(&engine).unwrap().tabs[0].id;
    assert!(state.close_active_pane(&engine.read()));
    let target = state.command_target(&engine);
    assert!(state.close_tab_in(&engine.read(), &target));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 2);
    assert!(intents.iter().all(|intent| intent.origin.is_user()));
    assert!(
        matches!(&intents[0].body, Intent::Domain(DomainIntent::ClosePane { pane_id }) if *pane_id == pane)
    );
    assert!(
        matches!(&intents[1].body, Intent::Domain(DomainIntent::CloseTab { tab_id }) if *tab_id == tab)
    );
    assert!(engine.find_pane_for_tab(tab).is_some());
}

#[cfg(feature = "gui")]
#[test]
fn mirror_close_is_a_user_request_without_local_structure_mutation() {
    let (mut state, mut owner) = crate::state::tests::test_mirror_state();
    let engine = owner.borrow_mut();
    let sid = state.focused_surface_id(&engine).unwrap();
    assert!(state.close_active_surface(&engine.read()));
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert!(intents[0].origin.is_user());
    assert!(
        matches!(&intents[0].body, Intent::ForwardMirror { op: crate::ipc::stream::StructuralOp::CloseSurface { surface_id }, .. } if *surface_id == sid)
    );
    assert!(engine.has_surface(sid));
}
