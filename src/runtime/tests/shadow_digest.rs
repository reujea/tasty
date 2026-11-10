//! 구조 digest: 같은 stream의 여러 재구성 경로가 같은 digest를 내고, 구조를 바꾸면 digest가 달라진다.

use crate::runtime::journal_payload::StoreBytes;
use tasty_core::{DataRef, DomainEvent, JournalModel, Pane, Ratio, SplitTree};
use tasty_event_store::EventStore;

use super::common::{commit_events, db_path, open, scenario, stream};
use crate::runtime::journal::{full_replay, load, load_all, save_snapshot};
use tasty_core::canonical::{CanonData, Canonical, IdMode, SkipData, differences};

fn canonical(store: &EventStore, model: &JournalModel) -> Canonical {
    Canonical::of_journal(model, &StoreBytes(store))
}

/// 시나리오 전체를 확정한 journal과 그 모델. 중간에 snapshot을 하나 둔다.
fn scenario_store(dir: &tempfile::TempDir) -> (EventStore, JournalModel) {
    let (mut store, epoch) = open(&db_path(dir));
    store.put_payload(epoch, b"notes").expect("payload");
    let batches = scenario();
    for events in &batches[..2] {
        commit_events(&mut store, epoch, events);
    }
    let mid = load_all(&store).expect("load");
    save_snapshot(&mut store, epoch, &mid).expect("snapshot");
    for events in &batches[2..] {
        commit_events(&mut store, epoch, events);
    }
    let model = load(&store, &stream()).expect("snapshot + tail");
    (store, model)
}

#[test]
fn every_replay_path_gives_the_same_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (store, fast) = scenario_store(&dir);
    let full = full_replay(&store).expect("full").stream(stream().as_str());

    // 같은 이벤트를 저장소 없이 한 batch씩 적용한 모델.
    let mut folded = JournalModel::default();
    for batch in store.read_batches_after(None, usize::MAX).expect("batches") {
        crate::runtime::journal::apply(&mut folded, &stream(), &batch).expect("apply");
    }

    let digest = canonical(&store, &fast).digest();
    assert_eq!(canonical(&store, &full).digest(), digest);
    assert_eq!(canonical(&store, &folded).digest(), digest);
    assert!(canonical(&store, &fast).defects.is_empty());

    // 저장 자료는 번호가 아니라 내용으로 비교한다.
    let surface = &canonical(&store, &fast).workspaces[0].panes[0].tabs[1].surfaces[0];
    assert_eq!(surface.kind, "markdown");
    assert!(matches!(&surface.data, CanonData::Bytes { len: 5, .. }));

    drop(store);
    let (reopened, _) = open(&db_path(&dir));
    let reloaded = load(&reopened, &stream()).expect("reload");
    assert_eq!(canonical(&reopened, &reloaded).digest(), digest);
}

#[test]
fn a_committed_change_changes_the_replayed_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, before) = scenario_store(&dir);
    let digest = canonical(&store, &before).digest();
    let epoch = store.acquire_writer().expect("writer");
    commit_events(
        &mut store,
        epoch,
        &[DomainEvent::TabRenamed {
            id: 1,
            name: "renamed".to_owned(),
        }],
    );
    let after = load(&store, &stream()).expect("load");
    let changed = canonical(&store, &after);
    assert_ne!(changed.digest(), digest);
    let diffs = differences(&canonical(&store, &before), &changed);
    assert_eq!(diffs.len(), 1, "{diffs:?}");
    assert!(diffs[0].path.ends_with(".name"), "{}", diffs[0]);
}

/// 모델 하나에 한 가지 변경을 가한다. 결과 digest는 기준과 달라야 한다.
type Mutation = (&'static str, fn(&mut JournalModel));

const MUTATIONS: &[Mutation] = &[
    ("category name", |m| {
        m.categories.values_mut().for_each(|c| c.name.push('x'))
    }),
    ("workspace name", |m| {
        m.workspaces.values_mut().for_each(|w| w.name.push('x'))
    }),
    ("workspace category", |m| {
        m.workspaces.values_mut().for_each(|w| w.category = 9)
    }),
    ("workspace subtitle", |m| {
        m.workspaces.values_mut().for_each(|w| w.subtitle.push('x'))
    }),
    ("workspace description", |m| {
        m.workspaces
            .values_mut()
            .for_each(|w| w.description.push('x'))
    }),
    ("workspace metadata", |m| {
        m.workspaces.values_mut().for_each(|w| {
            w.metadata.insert("k".to_owned(), "v".to_owned());
        })
    }),
    ("tab name", |m| {
        m.tabs.values_mut().for_each(|t| t.name.push('x'))
    }),
    ("tab explicit name", |m| {
        m.tabs
            .values_mut()
            .for_each(|t| t.explicit_name = Some(String::new()))
    }),
    ("surface split ratio", |m| {
        for tab in m.tabs.values_mut() {
            if let SplitTree::Split { ratio, .. } = &mut tab.layout {
                *ratio = Ratio::from_f32(ratio.to_f32() + f32::EPSILON);
            }
        }
    }),
    ("surface split order", |m| {
        for tab in m.tabs.values_mut() {
            if let SplitTree::Split { first, second, .. } = &mut tab.layout {
                std::mem::swap(first, second);
            }
        }
    }),
    ("surface kind", |m| {
        m.surfaces.values_mut().for_each(|s| s.kind.push('x'))
    }),
    ("surface data content", |m| {
        m.surfaces
            .values_mut()
            .for_each(|s| s.data = Some(DataRef(99)))
    }),
    ("surface metadata", |m| {
        m.surfaces.values_mut().for_each(|s| s.metadata.clear())
    }),
    ("tab order", |m| {
        m.panes.values_mut().for_each(|p| p.tabs.reverse())
    }),
    ("tab moved to another pane", |m| {
        let tab = *m.tabs.keys().next().expect("tab");
        let from = m.tabs[&tab].pane;
        let to = *m.panes.keys().find(|p| **p != from).expect("other pane");
        m.panes
            .get_mut(&from)
            .expect("pane")
            .tabs
            .retain(|t| *t != tab);
        m.panes.get_mut(&to).expect("pane").tabs.push(tab);
        m.tabs.get_mut(&tab).expect("tab").pane = to;
    }),
    ("orphan pane", |m| {
        m.panes.insert(
            77,
            Pane {
                workspace: 1,
                tabs: Vec::new(),
            },
        );
    }),
];

#[test]
fn each_structural_change_changes_the_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, _) = scenario_store(&dir);
    // 옮길 pane과 분할된 tab이 있도록 pane과 surface를 하나씩 더 둔다.
    let epoch = store.acquire_writer().expect("writer");
    commit_events(
        &mut store,
        epoch,
        &[
            DomainEvent::PaneSplit {
                target: 1,
                pane: 3,
                split: tasty_core::SplitSpec {
                    direction: tasty_model::SplitDirection::Horizontal,
                    ratio: Ratio::from_f32(0.5),
                    placement: tasty_core::Placement::After,
                },
            },
            DomainEvent::SurfaceSplit {
                target: 1,
                surface: tasty_core::SurfaceSpec {
                    id: 4,
                    kind: "terminal".to_owned(),
                    data: None,
                },
                split: tasty_core::SplitSpec {
                    direction: tasty_model::SplitDirection::Vertical,
                    ratio: Ratio::from_f32(0.25),
                    placement: tasty_core::Placement::After,
                },
            },
        ],
    );
    let base = load(&store, &stream()).expect("load");
    let digest = canonical(&store, &base).digest();
    for (name, mutate) in MUTATIONS {
        let mut changed = base.clone();
        mutate(&mut changed);
        assert_ne!(changed, base, "{name}: the mutation changed nothing");
        assert_ne!(
            canonical(&store, &changed).digest(),
            digest,
            "{name}: digest did not change"
        );
    }
}

#[test]
fn excluded_journal_position_and_skipped_data_do_not_change_the_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (store, base) = scenario_store(&dir);
    let digest = canonical(&store, &base).digest();
    let mut moved = base.clone();
    moved.applied = Default::default();
    assert_eq!(canonical(&store, &moved).digest(), digest);

    let mut other_data = base.clone();
    other_data
        .surfaces
        .values_mut()
        .for_each(|s| s.data = Some(DataRef(99)));
    assert_eq!(
        Canonical::of_journal(&other_data, &SkipData).digest(),
        Canonical::of_journal(&base, &SkipData).digest()
    );
    assert_eq!(
        canonical(&store, &base).without_data(),
        Canonical::of_journal(&base, &SkipData)
    );
}

#[test]
fn positional_ids_ignore_numbering_but_keep_structure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (store, base) = scenario_store(&dir);
    let exact = canonical(&store, &base);
    let positional = exact.positional();
    assert_eq!(positional.ids, IdMode::Positional);
    assert_ne!(positional.digest(), exact.digest());
    assert_eq!(positional.positional(), positional);

    // 순서를 바꾸면 순서 번호도 달라진다.
    let mut reordered = base.clone();
    reordered.panes.values_mut().for_each(|p| p.tabs.reverse());
    assert_ne!(
        canonical(&store, &reordered).positional().digest(),
        positional.digest()
    );
}
