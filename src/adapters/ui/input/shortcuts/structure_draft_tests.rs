//! 생성 대기 초안이 대상인 구조 명령. 초안이 기댄 기존 pane 이 대상이면 바로 보내고, 초안이 만들 단위가
//! 대상이면 걸어 두었다가 생성된 surface(또는 그 pane)에서 실행하며, 생성이 실패하면 버린다.

use super::structure;
use super::structure_tests::resolved;
use crate::adapters::ui::tools_menu::run_deferred_tools;
use crate::app::command::DomainIntent;
use crate::intent::{Intent, StructureTarget};
use crate::model::SplitDirection;
use crate::runtime::engine_session::EngineSession;
use crate::state::MainViewState;
use crate::state::attach_pending::deferred::settle_first_created;

/// 생성 대기 초안을 대상으로 한 구조 명령의 장면. 초안의 기준 surface 와 초안 id 를 돌려준다.
fn draft_scene(
    op: impl FnOnce(u32) -> tasty_ipc::stream::StructuralOp,
) -> (
    MainViewState,
    EngineSession,
    u32,
    crate::state::attach_pending::PendingId,
    impl Sized,
) {
    let (mut state, engine) = crate::state::tests::test_mirror_state();
    let read = engine.read();
    let anchor = state.focused_surface_id(&read).expect("focused surface");
    let draft = crate::state::tests::select_pending_draft_for(&mut state, &read, op);
    let id = state
        .command_target(&read)
        .draft
        .expect("the draft is the target");
    drop(read);
    (state, engine, anchor, id, draft)
}

fn split_pane_op(anchor: u32) -> tasty_ipc::stream::StructuralOp {
    tasty_ipc::stream::StructuralOp::SplitPane {
        anchor_surface_id: anchor,
        direction: tasty_ipc::stream::SplitAxis::Vertical,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd: None,
    }
}

fn split_surface_op(anchor: u32) -> tasty_ipc::stream::StructuralOp {
    tasty_ipc::stream::StructuralOp::SplitSurface {
        surface_id: anchor,
        direction: tasty_ipc::stream::SplitAxis::Vertical,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd: None,
    }
}

fn new_tab_op(anchor: u32) -> tasty_ipc::stream::StructuralOp {
    tasty_ipc::stream::StructuralOp::NewTab {
        anchor_surface_id: anchor,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd: None,
    }
}

/// 새 탭 초안이 대상이면 pane 명령은 초안이 기댄 기존 pane 이 대상이라 바로 보낸다.
#[test]
fn a_pane_command_for_a_new_tab_draft_goes_to_its_pane_now() {
    let (mut state, engine, anchor, id, _draft) = draft_scene(new_tab_op);
    let read = engine.read();
    let pane = read.find_pane_for_surface(anchor).expect("anchor pane");
    let target = state.command_target(&read);
    structure::split_pane(
        &mut state,
        &read,
        &target,
        SplitDirection::Vertical,
        "split_pane_vertical",
    );
    structure::restore_closed(&mut state, &read, &target, "restore_closed");
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    let intents = state.take_pending_intents();
    assert!(intents.iter().any(|i| matches!(
        i.body,
        Intent::SplitPane { target: StructureTarget { pane: Some(p), .. }, .. } if p == pane
    )));
    assert!(intents.iter().any(|i| matches!(
        i.body,
        Intent::RestoreClosedItem { pane: Some(p) } if p == pane
    )));
}

/// 새 탭·surface 분할 초안이 대상이면 터미널 새 탭(키·팔레트·더블탭)은 기댄 pane 에 바로 보내고, cwd 원본은
/// 입력 때 초안 뒤 탭에서 골라져 있던 surface 다. mirror 로 넘길 때도 그 surface 를 원본으로 싣는다.
#[test]
fn a_terminal_new_tab_for_a_draft_on_its_pane_keeps_the_base_surface_as_cwd_source() {
    use crate::app::services::impl_mirror::{ClientCwd, build_mirror_forward_op};
    use tasty_ipc::stream::InheritCwd;
    for op in [new_tab_op, split_surface_op] {
        let (mut state, engine, anchor, id, _draft) = draft_scene(op);
        let read = engine.read();
        let pane = read.find_pane_for_surface(anchor).expect("anchor pane");
        let target = state.command_target(&read);
        assert_eq!(target.draft_base_surface, Some(anchor));
        structure::new_terminal_tab(&mut state, &read, &target, "new_tab");
        assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
        let intents = state.take_pending_intents();
        assert_eq!(intents.len(), 1);
        assert!(
            matches!(
                intents[0].body,
                Intent::NewTab {
                    target: StructureTarget {
                        pane: Some(p),
                        cwd_source: Some(s),
                    },
                    ..
                } if p == pane && s == anchor
            ),
            "{:?}",
            intents[0].body
        );
        let domain = resolved(&state, &engine, &intents[0])
            .domain()
            .expect("resolved");
        let client = ClientCwd {
            presentation: Some(&state.navigation),
            inherit: true,
        };
        assert_eq!(
            build_mirror_forward_op(&engine.core_state, &domain, &client)
                .expect("creation op")
                .inherit_cwd(),
            Some(&InheritCwd::Surface(anchor))
        );
    }
}

/// surface 분할 초안이 대상이면 surface 분할은 만들 surface 가 대상이라 걸어 두고, 생성된 surface 를
/// 나눈다. pane 명령은 그 초안이 기댄 기존 pane 이 대상이라 바로 보낸다.
#[test]
fn a_surface_split_for_a_surface_draft_waits_for_the_created_surface() {
    let (mut state, engine, anchor, id, _draft) = draft_scene(split_surface_op);
    let read = engine.read();
    let target = state.command_target(&read);
    structure::split_surface(
        &mut state,
        &read,
        &target,
        SplitDirection::Horizontal,
        "split_surface_horizontal",
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    assert!(state.take_pending_intents().is_empty());
    structure::new_tab(
        &mut state,
        &read,
        &target,
        None,
        serde_json::Value::Null,
        "new_tab",
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    assert_eq!(state.take_pending_intents().len(), 1);
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert!(matches!(
        intents[0].body,
        Intent::SplitSurface {
            direction: SplitDirection::Horizontal,
            surface: Some(s),
        } if s == anchor
    ));
}

/// 새 pane 초안이 대상이면 pane 명령(분할·새 탭·복원)을 걸어 두고, 생성된 surface 의 pane 에 보낸다.
#[test]
fn pane_commands_for_a_pane_draft_wait_for_the_created_pane() {
    let (mut state, engine, anchor, id, _draft) = draft_scene(split_pane_op);
    let read = engine.read();
    let pane = read.find_pane_for_surface(anchor).expect("anchor pane");
    let target = state.command_target(&read);
    structure::split_pane(
        &mut state,
        &read,
        &target,
        SplitDirection::Vertical,
        "split_pane_vertical",
    );
    structure::new_terminal_tab(&mut state, &read, &target, "new_tab");
    structure::restore_closed(&mut state, &read, &target, "restore_closed");
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 3);
    assert!(state.take_pending_intents().is_empty());
    settle_first_created(&mut state, &engine.core_state, anchor);
    run_deferred_tools(&mut state, &read);
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 3);
    let at = StructureTarget {
        pane: Some(pane),
        cwd_source: Some(anchor),
    };
    assert!(matches!(&intents[0].body, Intent::SplitPane { target, .. } if *target == at));
    assert!(matches!(&intents[1].body, Intent::NewTab { target, .. } if *target == at));
    assert!(matches!(
        &intents[2].body,
        Intent::RestoreClosedItem { pane: Some(p) } if *p == pane
    ));
}

/// 걸어 둔 명령은 생성된 `local` 을 대상으로 실행한다. 실행 때의 포커스(pane 20)로 바꿔 읽지 않는다.
#[test]
fn a_held_command_runs_on_the_created_surface_not_the_focus() {
    use crate::state::attach_pending::deferred::HeldStructure;
    let (mut state, engine) = crate::state::tests::two_pane_fixture();
    state.select_pane(&engine.core_state, 20);
    let read = engine.read();
    for command in [
        HeldStructure::SplitPane(SplitDirection::Vertical),
        HeldStructure::SplitSurface(SplitDirection::Vertical),
        HeldStructure::NewTab {
            kind: None,
            params: serde_json::Value::Null,
        },
        HeldStructure::Restore,
    ] {
        structure::run_held(&mut state, &read, command.clone(), "held", 101);
        let intents = state.take_pending_intents();
        let domain = resolved(&state, &engine, &intents[0]).domain();
        let ok = match &domain {
            Some(DomainIntent::SplitPane { target_pane_id, .. }) => *target_pane_id == 10,
            Some(DomainIntent::SplitSurface {
                target_surface_id, ..
            }) => *target_surface_id == 101,
            Some(DomainIntent::CreateTab {
                pane_id,
                cwd_source,
                ..
            }) => *pane_id == 10 && *cwd_source == Some(101),
            Some(DomainIntent::RestoreClosedItem { target_pane_id, .. }) => {
                *target_pane_id == Some(10)
            }
            _ => false,
        };
        assert!(ok, "{command:?} -> {domain:?}");
    }
}

/// 입력 뒤 초안이 실패로 끝났으면 걸어 둔 명령을 실행하지 않고 버린다. 이미 끝난 초안이 대상이면
/// 걸지도 보내지도 않는다.
#[test]
fn a_held_structure_command_of_a_failed_draft_is_dropped() {
    let (mut state, engine, _anchor, id, _draft) = draft_scene(split_pane_op);
    let read = engine.read();
    let target = state.command_target(&read);
    structure::split_pane(
        &mut state,
        &read,
        &target,
        SplitDirection::Vertical,
        "split_pane_vertical",
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 1);
    let entry = &state.attach_pending.entries[0];
    entry.receipt.bind(0);
    let epoch = entry.epoch.clone();
    state.attach_pending.result(&epoch, 0, None, false);
    state.attach_pending.retain(&engine.core_state);
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    run_deferred_tools(&mut state, &read);
    assert!(state.take_pending_intents().is_empty());
    structure::split_pane(
        &mut state,
        &read,
        &target,
        SplitDirection::Vertical,
        "split_pane_vertical",
    );
    structure::split_surface(
        &mut state,
        &read,
        &target,
        SplitDirection::Vertical,
        "split_surface_vertical",
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    assert!(state.take_pending_intents().is_empty());
}

/// 다른 초안이 선택돼 있어도 대상을 직접 가리키는 입력(탭 바 분할 버튼)은 거절하지 않는다.
#[test]
fn a_command_on_an_explicit_target_is_not_held_by_a_selected_draft() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};
    let (mut state, engine, anchor, id, _draft) = draft_scene(split_pane_op);
    let read = engine.read();
    let pane = read.find_pane_for_surface(anchor).expect("anchor pane");
    apply_tab_bar_actions(
        &mut state,
        &read,
        vec![TabBarAction::RequestSplit { pane_id: pane }],
        &[],
        160.0,
        1.0,
    );
    assert_eq!(state.attach_pending.deferred.waiting_for(id), 0);
    assert!(state.take_pending_intents().iter().any(|i| matches!(
        i.body,
        Intent::SplitPane { target: StructureTarget { pane: Some(p), .. }, .. } if p == pane
    )));
}
