//! pure evolve: 결정성, batch 단위 적용, 순서 검사, 구조 변경 결과.

use std::collections::BTreeMap;

use super::common::{batch, scenario, scenario_batches, surface};
use crate::{Applied, DomainBatch, DomainEvent, EvolveError, JournalModel, SplitTree, evolve};

fn run(batches: &[DomainBatch]) -> JournalModel {
    let mut model = JournalModel::default();
    for b in batches {
        evolve(&mut model, b).expect("evolve");
    }
    model
}

#[test]
fn same_batches_give_the_same_model() {
    let batches = scenario_batches();
    let first = run(&batches);
    let second = run(&batches);
    assert_eq!(first, second);
    let events: usize = scenario().iter().map(Vec::len).sum();
    assert_eq!(
        first.applied,
        Applied {
            batch: Some(batches.len() as u64),
            revision: Some(events as u64),
        }
    );
}

#[test]
fn scenario_ends_in_the_expected_structure() {
    let model = run(&scenario_batches());
    assert_eq!(model.category_order, vec![0]);
    assert_eq!(model.workspace_order, vec![1]);
    let ws = &model.workspaces[&1];
    assert_eq!(ws.layout, SplitTree::Leaf(1));
    assert!(ws.metadata.is_empty());
    assert_eq!(
        (ws.subtitle.as_str(), ws.description.as_str()),
        ("sub", "desc")
    );
    assert_eq!(model.tabs[&1].explicit_name.as_deref(), Some("Build!"));
    assert_eq!(model.panes.keys().copied().collect::<Vec<_>>(), vec![1]);
    assert_eq!(model.panes[&1].tabs, vec![1]);
    assert_eq!(model.tabs[&1].name, "build");
    assert_eq!(model.tabs[&1].layout, SplitTree::Leaf(1));
    assert_eq!(model.surfaces.keys().copied().collect::<Vec<_>>(), vec![1]);
    assert_eq!(
        model.surfaces[&1].metadata,
        BTreeMap::from([("role".to_owned(), "orchestrator".to_owned())])
    );
}

#[test]
fn intermediate_state_keeps_moves_and_split_ratios() {
    let batches = scenario_batches();
    let model = run(&batches[..5]);
    assert_eq!(model.category_order, vec![1, 0]);
    assert_eq!(model.workspace_order, vec![2, 1]);
    assert_eq!(model.workspaces[&2].category, 0);
    assert_eq!(model.panes[&2].workspace, 2);
    assert_eq!(model.workspaces[&2].layout.leaves(), vec![2, 3]);
    let SplitTree::Split { ratio, .. } = model.workspaces[&2].layout else {
        panic!("expected split");
    };
    assert_eq!(ratio.to_f32(), 0.75);
    assert_eq!(model.panes[&1].tabs, vec![1, 3]);
    assert_eq!(model.surfaces[&3].tab, 2);
    assert_eq!(model.tabs[&2].layout.leaves(), vec![2, 3]);
    // 옛 매핑 기록은 해독되지만 모델에 남지 않는다.
    let after_details = run(&batches[..4]);
    assert_eq!(after_details.workspaces[&1].subtitle, "sub");
}

#[test]
fn failing_batch_leaves_the_model_unchanged() {
    let batches = scenario_batches();
    let mut model = run(&batches[..1]);
    let before = model.clone();
    let bad = batch(
        2,
        4,
        &[
            DomainEvent::TabRenamed {
                id: 1,
                name: "renamed".to_owned(),
            },
            DomainEvent::TabClosed { id: 99 },
        ],
    );
    assert!(matches!(
        evolve(&mut model, &bad),
        Err(EvolveError::Missing(name)) if name == "tab:99"
    ));
    assert_eq!(model, before);
}

#[test]
fn stale_batch_and_revision_gap_are_rejected() {
    let batches = scenario_batches();
    let mut model = run(&batches[..2]);
    let before = model.clone();
    assert!(matches!(
        evolve(&mut model, &batches[1]),
        Err(EvolveError::StaleBatch { last: 2, got: 2 })
    ));
    let gap = batch(3, 99, &scenario()[2]);
    assert!(matches!(
        evolve(&mut model, &gap),
        Err(EvolveError::RevisionGap {
            expected: 8,
            got: 100
        })
    ));
    assert_eq!(model, before);
}

#[test]
fn structural_invariants_are_enforced() {
    let mut model = run(&scenario_batches()[..1]);
    let cases = [
        (DomainEvent::PaneClosed { id: 1 }, "last"),
        (DomainEvent::SurfaceClosed { id: 1 }, "last"),
        (DomainEvent::CategoryClosed { id: 0 }, "non-empty"),
        (
            DomainEvent::TabCreated {
                id: 1,
                pane: 1,
                index: 0,
                name: "dup".to_owned(),
                surface: surface(9, "terminal"),
            },
            "duplicate",
        ),
        (
            DomainEvent::TabMoved {
                id: 1,
                pane: 1,
                index: 5,
            },
            "index",
        ),
    ];
    for (i, (event, what)) in cases.into_iter().enumerate() {
        let before = model.clone();
        let result = evolve(&mut model, &batch(10 + i as u64, 4, &[event]));
        let ok = matches!(
            (what, &result),
            ("last", Err(EvolveError::LastLeaf(_)))
                | ("non-empty", Err(EvolveError::CategoryNotEmpty(_)))
                | ("duplicate", Err(EvolveError::Duplicate(_)))
                | (
                    "index",
                    Err(EvolveError::IndexOutOfRange { index: 5, len: 0 })
                )
        );
        assert!(ok, "{what}: {result:?}");
        assert_eq!(model, before);
    }
}
