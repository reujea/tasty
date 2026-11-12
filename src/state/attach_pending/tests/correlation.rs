use super::*;
use tasty_core::{DomainEvent as E, Placement, Ratio, SplitSpec, SurfaceSpec};
fn split(direction: SplitDirection) -> SplitSpec {
    SplitSpec {
        direction,
        ratio: Ratio::from_f32(0.5),
        placement: Placement::After,
    }
}
fn new_tab(pane: u32, id: u32) -> E {
    E::TabCreated {
        id,
        pane,
        index: 1,
        name: "Shell".into(),
        surface: SurfaceSpec {
            id,
            kind: "terminal".into(),
            data: None,
        },
    }
}
fn fixture(
    extra: Vec<E>,
) -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "Mirror".into(),
            category: 0,
            index: 0,
            pane: 10,
        },
        E::TabCreated {
            id: 100,
            pane: 10,
            index: 0,
            name: "A".into(),
            surface: SurfaceSpec {
                id: 100,
                kind: "terminal".into(),
                data: None,
            },
        },
        E::PaneSplit {
            target: 10,
            pane: 20,
            split: split(SplitDirection::Vertical),
        },
        E::TabCreated {
            id: 200,
            pane: 20,
            index: 0,
            name: "B".into(),
            surface: SurfaceSpec {
                id: 200,
                kind: "terminal".into(),
                data: None,
            },
        },
    ];
    events.extend(extra);
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(events))
}
#[test]
fn unrelated_delta_preserves_sent_and_unsent_drafts() {
    let (mut state, before) = fixture(vec![]);
    let (_, after) = fixture(vec![new_tab(20, 300)]);
    let (sender, _) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    for sent in [true, false] {
        let r = state
            .attach_pending
            .begin(
                &before.core_state,
                &state.navigation,
                &tab(100),
                sender.epoch(),
                identity.clone(),
                std::time::Instant::now(),
            )
            .unwrap();
        if sent {
            r.bind(0);
        }
    }
    assert_eq!(
        state.attach_pending.applied(
            &after.core_state,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &[(900, 300)].into_iter().collect(),
            &[900]
        ),
        None
    );
    assert!(
        state
            .attach_pending
            .entries
            .iter()
            .all(|e| e.visible && !e.receipt.finished())
    );
}
#[test]
fn old_peer_result_does_not_claim_another_anchors_creation() {
    let (mut state, before) = fixture(vec![]);
    let (_, after) = fixture(vec![new_tab(20, 300)]);
    let (sender, _) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    let r = state
        .attach_pending
        .begin(
            &before.core_state,
            &state.navigation,
            &tab(100),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    r.bind(0);
    state.attach_pending.result(&sender.epoch(), 0, None, true);
    assert_eq!(
        state.attach_pending.applied(
            &after.core_state,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &[(900, 300)].into_iter().collect(),
            &[900]
        ),
        None
    );
    assert!(!r.finished());
    assert!(state.attach_pending.entries[0].visible);
}
#[test]
fn same_anchor_delta_never_collapses_an_unsent_request() {
    let (mut state, before) = fixture(vec![]);
    let (_, after) = fixture(vec![new_tab(10, 300)]);
    let (sender, _) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    state
        .attach_pending
        .begin(
            &before.core_state,
            &state.navigation,
            &tab(100),
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    state.attach_pending.applied(
        &after.core_state,
        &state.navigation,
        &sender.epoch(),
        Some(&identity),
        &[(900, 300)].into_iter().collect(),
        &[900],
    );
    assert!(state.attach_pending.entries[0].visible);
}

#[test]
fn an_old_peer_creation_needs_the_matching_kind_and_anchor_shape() {
    for creation in [
        Creation::Tab,
        Creation::Surface(SplitDirection::Vertical),
        Creation::Surface(SplitDirection::Horizontal),
        Creation::Pane(SplitDirection::Vertical),
        Creation::Pane(SplitDirection::Horizontal),
    ] {
        let op = match creation {
            Creation::Tab => tab(100),
            Creation::Surface(d) => StructuralOp::SplitSurface {
                surface_id: 100,
                direction: if d == SplitDirection::Vertical {
                    SplitAxis::Vertical
                } else {
                    SplitAxis::Horizontal
                },
                surface_kind: "terminal".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
            Creation::Pane(d) => StructuralOp::SplitPane {
                anchor_surface_id: 100,
                direction: if d == SplitDirection::Vertical {
                    SplitAxis::Vertical
                } else {
                    SplitAxis::Horizontal
                },
                surface_kind: "terminal".into(),
                params: serde_json::json!({}),
                inherit_cwd: None,
            },
        };
        let changes = match creation {
            Creation::Tab => vec![new_tab(10, 300)],
            Creation::Surface(d) => vec![E::SurfaceSplit {
                target: 100,
                surface: SurfaceSpec {
                    id: 300,
                    kind: "terminal".into(),
                    data: None,
                },
                split: split(d),
            }],
            Creation::Pane(d) => vec![
                E::PaneSplit {
                    target: 10,
                    pane: 30,
                    split: split(d),
                },
                E::TabCreated {
                    id: 300,
                    pane: 30,
                    index: 0,
                    name: "Shell".into(),
                    surface: SurfaceSpec {
                        id: 300,
                        kind: "terminal".into(),
                        data: None,
                    },
                },
            ],
        };
        let (mut state, before) = fixture(vec![]);
        let (_, after) = fixture(changes);
        let (sender, _) = tasty_remote::connection::channel();
        let view = Arc::new(());
        let identity = Arc::downgrade(&view);
        let r = state
            .attach_pending
            .begin(
                &before.core_state,
                &state.navigation,
                &op,
                sender.epoch(),
                identity.clone(),
                std::time::Instant::now(),
            )
            .unwrap();
        r.bind(0);
        r.result(None);
        assert_eq!(
            state.attach_pending.applied(
                &after.core_state,
                &state.navigation,
                &sender.epoch(),
                Some(&identity),
                &[(900, 300)].into_iter().collect(),
                &[900]
            ),
            Some(300),
            "{creation:?}"
        );
        assert!(r.finished());
    }
}
#[test]
fn merged_creation_candidates_fold_only_related_sent_drafts_and_keep_tracking() {
    let (mut state, before) = fixture(vec![]);
    let (_, after) = fixture(vec![new_tab(10, 300), new_tab(10, 400)]);
    let (sender, _) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    for (op, request) in [
        (Some(0), tab(100)),
        (Some(1), tab(100)),
        (None, tab(100)),
        (Some(2), tab(200)),
    ] {
        let r = state
            .attach_pending
            .begin(
                &before.core_state,
                &state.navigation,
                &request,
                sender.epoch(),
                identity.clone(),
                std::time::Instant::now(),
            )
            .unwrap();
        if let Some(op) = op {
            r.bind(op);
        }
    }
    state.attach_pending.result(&sender.epoch(), 0, None, true);
    assert_eq!(
        state.attach_pending.applied(
            &after.core_state,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &[(900, 300), (901, 400)].into_iter().collect(),
            &[900, 901]
        ),
        None
    );
    assert_eq!(
        state
            .attach_pending
            .entries
            .iter()
            .map(|e| e.visible)
            .collect::<Vec<_>>(),
        vec![false, false, true, true]
    );
    assert!(
        state
            .attach_pending
            .entries
            .iter()
            .all(|e| !e.receipt.finished())
    );
    state.attach_pending.result(&sender.epoch(), 1, None, true);
    state.attach_pending.applied(
        &after.core_state,
        &state.navigation,
        &sender.epoch(),
        Some(&identity),
        &[(900, 300), (901, 400)].into_iter().collect(),
        &[],
    );
    assert!(
        state
            .attach_pending
            .entries
            .iter()
            .all(|e| !e.receipt.finished()),
        "an empty later Delta cannot identify either result"
    );
}
#[test]
fn a_different_creation_shape_cannot_complete_a_surface_split() {
    let (mut state, before) = fixture(vec![]);
    let (_, after) = fixture(vec![new_tab(10, 300)]);
    let (sender, _) = tasty_remote::connection::channel();
    let view = Arc::new(());
    let identity = Arc::downgrade(&view);
    let op = StructuralOp::SplitSurface {
        surface_id: 100,
        direction: SplitAxis::Vertical,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd: None,
    };
    let r = state
        .attach_pending
        .begin(
            &before.core_state,
            &state.navigation,
            &op,
            sender.epoch(),
            identity.clone(),
            std::time::Instant::now(),
        )
        .unwrap();
    r.bind(0);
    r.result(None);
    assert_eq!(
        state.attach_pending.applied(
            &after.core_state,
            &state.navigation,
            &sender.epoch(),
            Some(&identity),
            &[(900, 300)].into_iter().collect(),
            &[900]
        ),
        None
    );
    assert!(!r.finished());
    assert!(state.attach_pending.entries[0].visible);
}
