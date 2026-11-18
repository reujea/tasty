//! 공통 새로고침 요청이 키를 누른 시점의 대상으로 고정되고 바뀐 대상에는 전달되지 않는지 본다.

use crate::intent::{Intent, RefreshTrigger, UiIntent};
use crate::state::PendingHostEvent;
use tasty_core::{DomainEvent as E, SurfaceSpec};

/// 워크스페이스 하나에 탭 두 개(surface 11 = `first`, 12 = `second`)를 둔다. `extra` 는 그 뒤에 적용한다.
fn fixture(
    first: &str,
    second: &str,
    extra: Vec<E>,
) -> (
    crate::state::MainViewState,
    crate::runtime::engine_session::EngineSession,
) {
    let mut events = base_events(first, second);
    events.extend(extra);
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(events))
}

fn base_events(first: &str, second: &str) -> Vec<E> {
    let mut events = vec![
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
    ];
    for (index, (id, kind)) in [(11, first), (12, second)].into_iter().enumerate() {
        events.push(E::TabCreated {
            id,
            pane: 1,
            index,
            name: kind.into(),
            surface: SurfaceSpec {
                id,
                kind: kind.into(),
                data: None,
            },
        });
    }
    events
}

fn refresh_intents(
    state: &mut crate::state::MainViewState,
) -> Vec<crate::intent::DispatchedIntent> {
    state
        .take_pending_intents()
        .into_iter()
        .filter(|i| matches!(i.body, Intent::Ui(UiIntent::RequestSurfaceRefresh { .. })))
        .collect()
}

fn refresh_events(engine: &mut crate::runtime::engine_access::EngineMut<'_>) -> Vec<(u32, String)> {
    engine
        .take_pending_host_events()
        .into_iter()
        .filter_map(|e| match e {
            PendingHostEvent::SurfaceRefreshRequested { target, kind, .. } => {
                Some((target.surface_id(), kind))
            }
            _ => None,
        })
        .collect()
}

/// 한 번의 요청은 지정한 surface 로 신호 하나가 된다. 다른 surface 로는 가지 않는다.
#[test]
fn one_request_becomes_one_signal_for_its_surface() {
    let (mut state, mut session) = fixture("markdown", "markdown", Vec::new());
    let mut engine = session.borrow_mut();
    drop(engine.take_pending_host_events());
    assert!(super::request_surface_refresh(
        &mut state,
        &engine.read(),
        11,
        RefreshTrigger::Shortcut
    ));
    let intents = refresh_intents(&mut state);
    assert_eq!(intents.len(), 1);
    assert!(intents[0].origin.is_user());
    crate::intent::popup::handle(&mut state, &mut engine, &intents[0]);
    assert_eq!(
        refresh_events(&mut engine),
        vec![(11, "markdown".to_string())]
    );
}

/// 없는 surface 는 요청을 만들지 않고 false 를 돌려 키를 다른 경로에 남긴다.
#[test]
fn an_unknown_surface_makes_no_request() {
    let (mut state, mut session) = fixture("terminal", "markdown", Vec::new());
    let engine = session.borrow_mut();
    assert!(!super::request_surface_refresh(
        &mut state,
        &engine.read(),
        99,
        RefreshTrigger::Shortcut
    ));
    assert!(refresh_intents(&mut state).is_empty());
}

/// 키를 누른 뒤 대상이 닫혔거나 다른 kind 로 변환됐으면 Intent 처리에서 버린다. 같은 ID 의 새 kind 에 보내지 않는다.
#[test]
fn a_closed_or_converted_target_drops_the_request() {
    let (mut state, mut session) = fixture("markdown", "terminal", Vec::new());
    let intent = {
        let engine = session.borrow_mut();
        assert!(super::request_surface_refresh(
            &mut state,
            &engine.read(),
            11,
            RefreshTrigger::Shortcut
        ));
        refresh_intents(&mut state).remove(0)
    };
    let cases = [
        (
            "converted",
            vec![E::SurfaceConverted {
                id: 11,
                kind: "html".into(),
                data: None,
            }],
        ),
        ("closed", vec![E::TabClosed { id: 11 }]),
    ];
    for (name, extra) in cases {
        let (mut state, mut session) = fixture("markdown", "terminal", extra);
        let mut engine = session.borrow_mut();
        drop(engine.take_pending_host_events());
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
        assert!(refresh_events(&mut engine).is_empty(), "{name}");
    }
    // 대조: 바뀌지 않은 엔진에서는 같은 Intent 가 신호를 낸다.
    let (mut state, mut session) = fixture("markdown", "terminal", Vec::new());
    let mut engine = session.borrow_mut();
    drop(engine.take_pending_host_events());
    crate::intent::popup::handle(&mut state, &mut engine, &intent);
    assert_eq!(refresh_events(&mut engine).len(), 1);
}

/// Intent 처리로 큐에 들어간 뒤 전달 전에 core 의 대상이 변환되거나 닫히면, 실제 전달 직전 drain
/// (`take_current_host_events`)이 버린다. 같은 ID 의 새 kind 로 보내지 않는다.
#[test]
fn a_target_converted_or_closed_after_enqueue_is_not_delivered() {
    let cases = [
        (
            "converted",
            Some(E::SurfaceConverted {
                id: 11,
                kind: "html".into(),
                data: None,
            }),
        ),
        ("closed", Some(E::TabClosed { id: 11 })),
        ("unchanged", None),
    ];
    for (name, change) in cases {
        let (mut state, mut session) = fixture("markdown", "terminal", Vec::new());
        let mut engine = session.borrow_mut();
        drop(engine.take_pending_host_events());
        assert!(super::request_surface_refresh(
            &mut state,
            &engine.read(),
            11,
            RefreshTrigger::Shortcut
        ));
        let intent = refresh_intents(&mut state).remove(0);
        crate::intent::popup::handle(&mut state, &mut engine, &intent);
        if let Some(event) = change {
            apply_to_core(&mut engine, event);
        }
        let delivered: Vec<u32> =
            crate::app::dispatch::host_events::take_current_host_events(&mut engine)
                .into_iter()
                .filter_map(|e| match e {
                    PendingHostEvent::SurfaceRefreshRequested { target, .. } => {
                        Some(target.surface_id())
                    }
                    _ => None,
                })
                .collect();
        let expected: Vec<u32> = if name == "unchanged" {
            vec![11]
        } else {
            Vec::new()
        };
        assert_eq!(delivered, expected, "{name}");
    }
}

/// fixture 의 core 에 구조 이벤트 하나를 실제 projection 경로로 적용한다.
fn apply_to_core(engine: &mut crate::runtime::engine_access::EngineMut<'_>, event: E) {
    let before = crate::state::tests::test_model(base_events("markdown", "terminal"));
    let mut retired = Vec::new();
    tasty_core::projection::apply(
        engine.core,
        &before,
        &tasty_core::DomainBatch {
            batch_id: 2,
            events: vec![tasty_core::RecordedEvent { revision: 5, event }],
        },
        &mut retired,
    )
    .expect("apply");
}

/// 에이전트 출처의 같은 Intent 는 신호를 내지 않는다.
#[test]
fn an_agent_origin_request_emits_nothing() {
    let (mut state, mut session) = fixture("markdown", "terminal", Vec::new());
    let mut engine = session.borrow_mut();
    drop(engine.take_pending_host_events());
    let target =
        crate::runtime::surface_binding::SurfaceBinding::capture(&engine.read(), 11).unwrap();
    let intent = UiIntent::RequestSurfaceRefresh {
        target,
        kind: "markdown".into(),
        trigger: RefreshTrigger::Shortcut,
    }
    .from_agent_ipc();
    crate::intent::popup::handle(&mut state, &mut engine, &intent);
    assert!(refresh_events(&mut engine).is_empty());
}

/// 포커스 경로는 지금 포커스된 surface 하나를 대상으로 한다. 포커스가 옮겨도 이미 만든 요청의 대상은 그대로다.
#[test]
fn the_focused_path_targets_the_focused_surface_at_key_time() {
    let (mut state, mut session) = fixture("terminal", "markdown", Vec::new());
    let mut engine = session.borrow_mut();
    drop(engine.take_pending_host_events());
    let focused = state
        .focused_surface_id(&engine.read())
        .expect("a focused surface");
    let target = state.command_target(&engine.read());
    assert!(super::request_target_surface_refresh(
        &mut state,
        &engine.read(),
        &target,
        RefreshTrigger::Shortcut
    ));
    let intent = refresh_intents(&mut state).remove(0);
    let other = if focused == 11 { 12 } else { 11 };
    let pane = engine.core.find_pane_by_id(1).expect("pane 1");
    assert!(state.navigation.select_tab(pane, other));
    assert_eq!(state.focused_surface_id(&engine.read()), Some(other));
    crate::intent::popup::handle(&mut state, &mut engine, &intent);
    let events = refresh_events(&mut engine);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, focused);
}
