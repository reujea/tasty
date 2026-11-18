//! 생성 대기 초안이 선택된 동안 기존 대상으로 넘어가던 내장 명령.

use crate::state::tests::{select_pending_draft, test_mirror_state};

/// 초안 선택 중 `close_pane` 은 초안을 닫을 대상으로 보지 않고, 활성 workspace 닫기로 넘어가지도 않는다.
#[test]
fn close_pane_does_not_close_the_workspace_while_a_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    crate::shortcuts::close_pane_action(&mut state, &read, &target);
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.entries.len(), 1);
}

/// 초안 선택 중 `close_active` 도 탭·pane·workspace 어느 것도 닫지 않는다.
#[test]
fn close_active_does_not_close_the_workspace_while_a_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    crate::shortcuts::close_active_action(&mut state, &read, &target);
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(state.attach_pending.entries.len(), 1);
}

/// 초안이 없으면 `close_pane` 은 지금처럼 닫기 요청을 보낸다. 위 두 시험이 아무 요청도 없는 경로만
/// 재지 않도록 대조한다.
#[test]
fn close_pane_without_a_draft_still_requests_a_close() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let target = state.command_target(&read);
    crate::shortcuts::close_pane_action(&mut state, &read, &target);
    crate::shortcuts::close_active_action(&mut state, &read, &target);
    assert!(!state.take_pending_intents().is_empty());
}

/// 새 탭 초안이 선택돼 있으면 navigation 의 선택 탭은 기존 탭이다. `rename_tab` 은 그 탭을 대상으로 잡지
/// 않는다.
#[test]
fn rename_tab_has_no_target_while_a_draft_is_selected() {
    let (mut state, engine) = test_mirror_state();
    let read = engine.read();
    let rename = |state: &crate::state::RequestContext| {
        state.tab_rename_in(&read, &state.command_target(&read))
    };
    assert!(rename(&state).is_some());
    let _draft = select_pending_draft(&mut state, &read);
    assert!(rename(&state).is_none());
}

/// 새 탭 초안이 선택돼 있으면 탐색기 단축키가 쓰는 포커스 탐색기는 없다. navigation 의 선택 탭에 남은
/// 기존 탐색기를 대상으로 잡지 않는다.
#[test]
fn explorer_shortcuts_have_no_target_while_a_draft_is_selected() {
    let (mut state, engine, sid, _dir) = crate::state::tests::explorer_fixture(true);
    let read = engine.read();
    let focused = |state: &crate::state::RequestContext| {
        crate::shortcuts::focused_explorer_surface_id(state, &read)
    };
    assert_eq!(focused(&state), Some(sid));
    let _draft = select_pending_draft(&mut state, &read);
    assert_eq!(focused(&state), None);
}

/// 다른 pane 의 초안을 다시 고르면 명령 대상의 workspace·pane·탭은 모델 포커스 pane 이 아니라 그 초안 자리다.
#[test]
fn a_draft_reselected_from_another_pane_targets_its_own_pane() {
    let (mut state, engine) = crate::state::tests::two_pane_fixture();
    let read = engine.read();
    let ws = read.workspace_at(0).expect("workspace");
    state.navigation.select_pane(ws, 10);
    let _draft = select_pending_draft(&mut state, &read);
    let id = state.attach_pending.entries[0].id;
    let ws = read.workspace_at(0).expect("workspace");
    state.navigation.select_pane(ws, 20);
    state.attach_pending.select(id, &state.navigation);
    assert_eq!(state.focused_pane_id(&read), 20);
    let target = state.command_target(&read);
    assert_eq!(target.draft, Some(id));
    assert_eq!(
        (target.workspace, target.pane, target.tab),
        (Some(1), Some(10), Some(101))
    );
    assert_eq!(
        target.draft_creation,
        Some(crate::state::attach_pending::Creation::Tab)
    );
    assert_eq!(target.draft_base_surface, Some(101));
}

/// 탭 하나에 surface 11·13, 11 을 고른 채 초안을 만들어 입력 대상을 얻은 뒤 13 을 고른 장면.
pub(crate) fn split_tab_with_a_draft() -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
    crate::state::command_target::CommandTarget,
    impl Sized,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "w".into(),
            category: 0,
            index: 0,
            pane: 1,
        },
        E::TabCreated {
            id: 11,
            pane: 1,
            index: 0,
            name: "t".into(),
            surface: SurfaceSpec {
                id: 11,
                kind: "terminal".into(),
                data: None,
            },
        },
        E::SurfaceSplit {
            target: 11,
            surface: SurfaceSpec {
                id: 13,
                kind: "terminal".into(),
                data: None,
            },
            split: tasty_core::SplitSpec {
                direction: crate::model::SplitDirection::Horizontal,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ];
    let (mut state, session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    let read = session.read();
    let pane = read.find_pane_by_id(1).expect("pane");
    state.navigation.select_surface(&pane.tabs[0], 11);
    let draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    assert_eq!(target.draft_base_surface, Some(11));
    let pane = read.find_pane_by_id(1).expect("pane");
    state.navigation.select_surface(&pane.tabs[0], 13);
    drop(read);
    (state, session, target, draft)
}

fn selected_surface(
    state: &crate::state::MainViewState,
    read: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<u32> {
    let pane = read.find_pane_by_id(1).expect("pane");
    state.navigation.surface_id(&pane.tabs[0])
}

/// 초안을 대상으로 한 surface 이동은 입력 때 골라져 있던 surface 에서 움직인다. 그 뒤 고른 surface 를 기준으로
/// 다시 읽지 않는다.
#[test]
fn a_draft_surface_step_starts_from_the_surface_chosen_at_input() {
    let (mut state, session, target, _draft) = split_tab_with_a_draft();
    let read = session.read();
    state.move_surface_focus_in(&read, &target, false);
    assert_eq!(selected_surface(&state, &read), Some(13));
}

/// 대상 초안이 실행 전에 사라졌으면 surface·탭 이동은 기존 탭·surface 로 바꿔 읽지 않고 버린다.
#[test]
fn a_step_for_a_draft_that_is_gone_is_dropped() {
    let (mut state, session, target, _draft) = split_tab_with_a_draft();
    let read = session.read();
    state.attach_pending.entries.clear();
    let serial = state.attach_pending.selection_serial;
    state.move_surface_focus_in(&read, &target, false);
    assert_eq!(selected_surface(&state, &read), Some(13));
    assert_eq!(state.attach_pending.selection_serial, serial);

    let (mut state, session) = crate::state::tests::two_pane_fixture();
    let read = session.read();
    let ws = read.workspace_at(0).expect("workspace");
    state.navigation.select_pane(ws, 10);
    let _draft = select_pending_draft(&mut state, &read);
    let target = state.command_target(&read);
    state.attach_pending.entries.clear();
    let pane = read.find_pane_by_id(10).expect("pane");
    assert_eq!(state.navigation.tab_index(pane), 0);
    state.step_visible_tab_in(&read, &target, true);
    let pane = read.find_pane_by_id(10).expect("pane");
    assert_eq!(state.navigation.tab_index(pane), 0);
}
