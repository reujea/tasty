//! importer 시험. 실제 journal에 가져온 뒤 replay한 모델에서 슬롯을 다시 만들어 원본과 비교한다.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use tasty_core::{JournalModel, SplitTree, encode_snapshot};
use tasty_event_store::{EventStore, PayloadRef, StoreError, StreamId, WriterEpoch};

use super::super::schema::{
    SavedCategory, SavedLayout, SavedPane, SavedPaneNode, SavedSurface, SavedSurfaceLayout,
    SavedTab, SavedWorkspace,
};
use super::surface_data::SurfaceData;
use super::{
    ImportError, ImportMapping, ImportOutcome, ImportedView, ScrollbackSource, import_slot,
};
use crate::model::{PaneId, SurfaceId, WorkspaceCategoryId};
use crate::runtime::journal::{self, engine_stream};

const JOURNAL: &str = "journal-import-test";

/// 모든 필드와 여러 단계의 분할, 0이 아닌 선택 위치를 담은 현재 형식 슬롯.
const FULL_SLOT: &str = r#"{
  "version": 2,
  "active_workspace": 1,
  "categories": [
    { "id": 0, "name": "normal", "collapsed": false },
    { "id": 7, "name": "work", "collapsed": true },
    { "id": 3, "name": "side" }
  ],
  "workspaces": [
    {
      "name": "main",
      "subtitle": "sub",
      "description": "desc",
      "category": 7,
      "attach_mapping": { "target": { "Profile": { "name": "box" } }, "remote_workspace": 4 },
      "focused_pane_index": 2,
      "pane_layout": { "Split": {
        "direction": "Vertical", "ratio": 0.3,
        "first": { "Leaf": { "active_tab": 1, "tabs": [
          { "name": "shell", "explicit_name": null,
            "surface": { "Leaf": { "Terminal": { "cwd": "/home", "restore_command": "vim", "scrollback_ref": "sb1" } } } },
          { "name": "build", "explicit_name": "Build!",
            "surface": { "Split": { "direction": "Horizontal", "ratio": 0.25,
              "first": { "Leaf": { "Terminal": { "cwd": "/tmp" } } },
              "second": { "Split": { "direction": "Vertical", "ratio": 0.6,
                "first": { "Leaf": { "Generic": { "kind": "markdown", "data": { "path": "/x.md", "n": [1, 2] } } } },
                "second": { "Leaf": { "Terminal": { "scrollback_ref": "gone" } } } } } } } }
        ] } },
        "second": { "Split": {
          "direction": "Horizontal", "ratio": 0.1,
          "first": { "Leaf": { "active_tab": 0, "tabs": [
            { "name": "notes", "explicit_name": "",
              "surface": { "Leaf": { "Generic": { "kind": "explorer" } } } }
          ] } },
          "second": { "Leaf": { "active_tab": 0, "tabs": [
            { "name": "logs", "explicit_name": null, "surface": { "Leaf": { "Terminal": {} } } }
          ] } }
        } }
      } }
    },
    {
      "name": "side",
      "subtitle": "",
      "description": "",
      "category": 3,
      "focused_pane_index": 0,
      "pane_layout": { "Leaf": { "active_tab": 0, "tabs": [
        { "name": "one", "explicit_name": null, "surface": { "Leaf": { "Terminal": {} } } }
      ] } }
    }
  ]
}"#;

/// categories·category·attach_mapping이 없는 이전 형식 슬롯.
const LEGACY_SLOT: &str = r#"{
  "version": 1,
  "active_workspace": 0,
  "workspaces": [
    {
      "name": "ws", "subtitle": "", "description": "",
      "focused_pane_index": 0,
      "pane_layout": { "Leaf": { "active_tab": 0, "tabs": [
        { "name": "Shell", "explicit_name": null, "surface": { "Leaf": { "Terminal": {} } } }
      ] } }
    }
  ]
}"#;

#[derive(Default)]
struct Files(BTreeMap<String, Vec<u8>>);

impl ScrollbackSource for Files {
    fn read_bytes(&self, persist_id: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.0.get(persist_id).cloned())
    }
}

fn files() -> Files {
    Files(BTreeMap::from([(
        "sb1".to_owned(),
        b"scrollback bytes".to_vec(),
    )]))
}

/// 같은 시험 바이너리의 다른 시험이 fork한 자식이 잠금 파일 설명을 잠시 공유할 수 있어
/// writer 잠금 획득을 짧게 다시 시도한다.
fn open(path: &Path) -> (EventStore, WriterEpoch) {
    let mut store = EventStore::open(path, JOURNAL).expect("open journal");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match store.acquire_writer() {
            Ok(epoch) => return (store, epoch),
            Err(StoreError::WriterLocked) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("acquire writer: {error}"),
        }
    }
}

fn parse(json: &str) -> SavedLayout {
    serde_json::from_str(json).expect("fixture")
}

fn import(store: &mut EventStore, epoch: WriterEpoch, json: &str) -> ImportOutcome {
    import_slot(store, epoch, 1, json, &files()).expect("import")
}

/// 슬롯 1 엔진의 구조 stream.
fn slot1() -> StreamId {
    engine_stream(1)
}

fn load(store: &EventStore, stream: &StreamId) -> JournalModel {
    journal::load(store, stream).expect("replay")
}

fn surface_data(store: &EventStore, model: &JournalModel, id: SurfaceId) -> Option<SurfaceData> {
    let data = model.surfaces[&id].data?;
    let bytes = store.read_payload(PayloadRef(data.0)).expect("payload");
    Some(SurfaceData::decode(&bytes).expect("surface data"))
}

/// replay한 모델·surface 저장 자료와 대응·화면 상태로 슬롯을 다시 만든다.
fn reconstruct(
    store: &EventStore,
    model: &JournalModel,
    mapping: &ImportMapping,
    view: &ImportedView,
    version: u32,
) -> SavedLayout {
    let saved_category =
        |id: WorkspaceCategoryId| mapping.saved_category(id).expect("mapped category");
    SavedLayout {
        version,
        active_workspace: position(&model.workspace_order, view.active_workspace),
        categories: model
            .category_order
            .iter()
            .map(|id| SavedCategory {
                id: saved_category(*id),
                name: model.categories[id].name.clone(),
                collapsed: view.collapsed_categories.contains(id),
            })
            .collect(),
        workspaces: model
            .workspace_order
            .iter()
            .map(|id| {
                let ws = &model.workspaces[id];
                SavedWorkspace {
                    name: ws.name.clone(),
                    subtitle: ws.subtitle.clone(),
                    description: ws.description.clone(),
                    pane_layout: pane_node(store, model, view, &ws.layout),
                    focused_pane_index: position(
                        &ws.layout.leaves(),
                        view.focused_panes.get(id).copied(),
                    ),
                    category: saved_category(ws.category),
                }
            })
            .collect(),
    }
}

fn position(list: &[u32], selected: Option<u32>) -> usize {
    selected
        .and_then(|id| list.iter().position(|x| *x == id))
        .unwrap_or(usize::MAX)
}

fn pane_node(
    store: &EventStore,
    model: &JournalModel,
    view: &ImportedView,
    node: &SplitTree<PaneId>,
) -> SavedPaneNode {
    match node {
        SplitTree::Leaf(pane) => {
            let tabs = &model.panes[pane].tabs;
            SavedPaneNode::Leaf(SavedPane {
                active_tab: position(tabs, view.active_tabs.get(pane).copied()),
                tabs: tabs
                    .iter()
                    .map(|tab| SavedTab {
                        name: model.tabs[tab].name.clone(),
                        explicit_name: model.tabs[tab].explicit_name.clone(),
                        surface: surface_node(store, model, &model.tabs[tab].layout),
                    })
                    .collect(),
            })
        }
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => SavedPaneNode::Split {
            direction: (*direction).into(),
            ratio: ratio.to_f32(),
            first: Box::new(pane_node(store, model, view, first)),
            second: Box::new(pane_node(store, model, view, second)),
        },
    }
}

fn surface_node(
    store: &EventStore,
    model: &JournalModel,
    node: &SplitTree<SurfaceId>,
) -> SavedSurfaceLayout {
    match node {
        SplitTree::Leaf(id) => {
            let surface = &model.surfaces[id];
            let data = surface_data(store, model, *id);
            SavedSurfaceLayout::Leaf(match (surface.kind.as_str(), data) {
                ("terminal", None) => SavedSurface::Terminal {
                    cwd: None,
                    restore_command: None,
                    scrollback_ref: None,
                },
                (
                    "terminal",
                    Some(SurfaceData::Terminal {
                        cwd,
                        restore_command,
                        scrollback_ref,
                        ..
                    }),
                ) => SavedSurface::Terminal {
                    cwd,
                    restore_command,
                    scrollback_ref,
                },
                (kind, Some(SurfaceData::Generic { data, .. })) => SavedSurface::Generic {
                    kind: kind.to_owned(),
                    data,
                },
                (kind, None) => SavedSurface::Generic {
                    kind: kind.to_owned(),
                    data: serde_json::Value::Null,
                },
                (kind, other) => panic!("{kind} surface has unexpected data {other:?}"),
            })
        }
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => SavedSurfaceLayout::Split {
            direction: (*direction).into(),
            ratio: ratio.to_f32(),
            first: Box::new(surface_node(store, model, first)),
            second: Box::new(surface_node(store, model, second)),
        },
    }
}

fn value(layout: &SavedLayout) -> serde_json::Value {
    serde_json::to_value(layout).expect("serialize")
}

#[test]
fn full_slot_round_trips_through_the_journal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let outcome = import(&mut store, epoch, FULL_SLOT);
    assert!(outcome.batch.is_some());
    assert_eq!(outcome.missing_scrollback, vec!["gone".to_owned()]);
    assert!(outcome.moved_to_normal.is_empty());

    let model = load(&store, &slot1());
    assert_eq!(
        journal::full_replay(&store)
            .expect("full replay")
            .stream(slot1().as_str()),
        model
    );
    let rebuilt = reconstruct(&store, &model, &outcome.mapping, &outcome.view, 2);
    assert_eq!(value(&rebuilt), value(&parse(FULL_SLOT)));
}

#[test]
fn legacy_slot_round_trips_with_default_categories() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let outcome = import(&mut store, epoch, LEGACY_SLOT);
    let model = load(&store, &slot1());

    let mut expected = parse(LEGACY_SLOT);
    // 복원과 같이 분류 목록이 없던 슬롯은 normal 하나를 가진 것으로 본다.
    expected.categories = vec![SavedCategory {
        id: 0,
        name: "normal".to_owned(),
        collapsed: false,
    }];
    let rebuilt = reconstruct(&store, &model, &outcome.mapping, &outcome.view, 1);
    assert_eq!(value(&rebuilt), value(&expected));
}

#[test]
fn mapping_translates_every_position_both_ways() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let mapping = import(&mut store, epoch, FULL_SLOT).mapping;
    let model = load(&store, &slot1());

    assert_eq!(mapping.slot, 1);
    assert_eq!(mapping.category(0), Some(0));
    let work = mapping.category(7).expect("work");
    assert_eq!(mapping.saved_category(work), Some(7));
    assert!(model.categories.contains_key(&work));

    let mut surfaces = 0;
    for (wi, ws) in mapping.workspaces.iter().enumerate() {
        assert_eq!(mapping.workspace_index(ws.id), Some(wi));
        assert_eq!(model.workspace_order[wi], ws.id);
        assert_eq!(
            model.workspaces[&ws.id].layout.leaves(),
            ws.panes.iter().map(|p| p.id).collect::<Vec<_>>()
        );
        for (pi, pane) in ws.panes.iter().enumerate() {
            assert_eq!(mapping.pane_index(pane.id), Some((wi, pi)));
            for (ti, tab) in pane.tabs.iter().enumerate() {
                assert_eq!(mapping.tab_index(tab.id), Some((wi, pi, ti)));
                assert_eq!(model.panes[&pane.id].tabs[ti], tab.id);
                assert_eq!(model.tabs[&tab.id].layout.leaves(), tab.surfaces);
                for (si, surface) in tab.surfaces.iter().enumerate() {
                    assert_eq!(mapping.surface_index(*surface), Some((wi, pi, ti, si)));
                    surfaces += 1;
                }
            }
        }
    }
    assert_eq!(surfaces, 7);
    assert_eq!(model.surfaces.len(), 7);
    assert_eq!(mapping.workspace_index(9999), None);
}

#[test]
fn view_state_is_returned_but_not_recorded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let outcome = import(&mut store, epoch, FULL_SLOT);
    let mapping = &outcome.mapping;
    let main = &mapping.workspaces[0];
    assert_eq!(
        outcome.view.active_workspace,
        Some(mapping.workspaces[1].id)
    );
    assert_eq!(outcome.view.focused_panes[&main.id], main.panes[2].id);
    assert_eq!(
        outcome.view.active_tabs[&main.panes[0].id],
        main.panes[0].tabs[1].id
    );
    assert_eq!(
        outcome.view.collapsed_categories,
        vec![mapping.category(7).expect("work")]
    );
    let model = load(&store, &slot1());
    let recorded = serde_json::to_string(&model).expect("model");
    assert!(!recorded.contains("collapsed") && !recorded.contains("active"));
}

#[test]
fn scrollback_is_stored_as_a_pinned_payload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let mapping = import(&mut store, epoch, FULL_SLOT).mapping;
    let model = load(&store, &slot1());
    let first = mapping.workspaces[0].panes[0].tabs[0].surfaces[0];
    let data = model.surfaces[&first].data.expect("terminal data payload");
    let payload = PayloadRef(data.0);
    assert_eq!(
        surface_data(&store, &model, first),
        Some(SurfaceData::Terminal {
            cwd: Some("/home".to_owned()),
            restore_command: Some("vim".to_owned()),
            scrollback_ref: Some("sb1".to_owned()),
            scrollback: Some(b"scrollback bytes".to_vec()),
        })
    );
    let holders = store.payload_holders(payload).expect("holders");
    assert!(
        holders.iter().any(|h| h.starts_with("event:")),
        "{holders:?}"
    );

    // 파일이 없던 참조는 내용 없이 저장 자료에 남는다.
    let gone = mapping.workspaces[0].panes[0].tabs[1].surfaces[2];
    assert_eq!(
        surface_data(&store, &model, gone),
        Some(SurfaceData::Terminal {
            cwd: None,
            restore_command: None,
            scrollback_ref: Some("gone".to_owned()),
            scrollback: None,
        })
    );
    // 저장할 값이 없는 terminal은 자료 참조가 없다.
    let empty = mapping.workspaces[1].panes[0].tabs[0].surfaces[0];
    assert_eq!(model.surfaces[&empty].data, None);
}

#[test]
fn reimport_returns_the_stored_mapping_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let first = import(&mut store, epoch, FULL_SLOT);
    let cut = store.current_cut().expect("cut");

    let again = import(&mut store, epoch, FULL_SLOT);
    assert_eq!(again.batch, None);
    assert_eq!(again.mapping, first.mapping);
    assert_eq!(again.view, first.view);
    assert_eq!(store.current_cut().expect("cut"), cut);

    let changed = FULL_SLOT.replace("\"main\"", "\"renamed\"");
    assert!(matches!(
        import_slot(&mut store, epoch, 1, &changed, &files()),
        Err(ImportError::AlreadyImportedDifferently(1))
    ));
    assert_eq!(store.current_cut().expect("cut"), cut);
}

#[test]
fn each_slot_goes_to_its_own_engine_stream_with_global_ids() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let first = import(&mut store, epoch, FULL_SLOT);
    let second = import_slot(&mut store, epoch, 2, LEGACY_SLOT, &files()).expect("import");
    let slot2 = engine_stream(2);

    // 두 엔진 모델은 각자 자기 슬롯 내용만 담는다.
    let one = load(&store, &slot1());
    let two = load(&store, &slot2);
    let rebuilt_one = reconstruct(&store, &one, &first.mapping, &first.view, 2);
    assert_eq!(value(&rebuilt_one), value(&parse(FULL_SLOT)));
    let mut expected_two = parse(LEGACY_SLOT);
    expected_two.categories = vec![SavedCategory {
        id: 0,
        name: "normal".to_owned(),
        collapsed: false,
    }];
    let rebuilt_two = reconstruct(&store, &two, &second.mapping, &second.view, 1);
    assert_eq!(value(&rebuilt_two), value(&expected_two));
    // 엔진마다 normal 카테고리가 있고 각 stream의 revision은 1부터 센다.
    assert!(one.categories.contains_key(&0) && two.categories.contains_key(&0));
    assert_eq!(
        store.stream_revision(&slot2).expect("head"),
        two.applied.revision
    );

    // ID는 journal 예약에서 받으므로 엔진 사이에서 겹치지 않는다.
    fn disjoint<T: Ord + Copy + std::fmt::Debug>(a: impl Iterator<Item = T>, b: &[T]) {
        let a: Vec<T> = a.collect();
        assert!(a.iter().all(|x| !b.contains(x)), "{a:?} and {b:?} overlap");
    }
    let keys = |m: &JournalModel| {
        (
            m.workspaces.keys().copied().collect::<Vec<_>>(),
            m.panes.keys().copied().collect::<Vec<_>>(),
            m.tabs.keys().copied().collect::<Vec<_>>(),
            m.surfaces.keys().copied().collect::<Vec<_>>(),
        )
    };
    let (w1, p1, t1, s1) = keys(&one);
    let (w2, p2, t2, s2) = keys(&two);
    disjoint(w1.into_iter(), &w2);
    disjoint(p1.into_iter(), &p2);
    disjoint(t1.into_iter(), &t2);
    disjoint(s1.into_iter(), &s2);
    assert!(
        one.surfaces
            .keys()
            .chain(two.surfaces.keys())
            .all(|id| *id < crate::runtime::terminal_store::PTY_ID_BASE)
    );
}

#[test]
fn typed_details_survive_import_replay_and_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("journal.db");
    let (mut store, epoch) = open(&path);
    let outcome = import(&mut store, epoch, FULL_SLOT);
    import_slot(&mut store, epoch, 2, LEGACY_SLOT, &files()).expect("import");

    let model = load(&store, &slot1());
    let main = &model.workspaces[&outcome.mapping.workspaces[0].id];
    assert_eq!(
        (main.subtitle.as_str(), main.description.as_str()),
        ("sub", "desc")
    );
    let tabs = &outcome.mapping.workspaces[0].panes;
    assert_eq!(
        model.tabs[&tabs[0].tabs[1].id].explicit_name.as_deref(),
        Some("Build!")
    );
    assert_eq!(
        model.tabs[&tabs[1].tabs[0].id].explicit_name.as_deref(),
        Some("")
    );
    assert_eq!(model.tabs[&tabs[0].tabs[0].id].explicit_name, None);

    // import. 임시 키를 쓰지 않는다. metadata는 사용자 정의 키만 담는다.
    for engine in journal::load_all(&store).expect("load").streams.values() {
        assert!(engine.workspaces.values().all(|w| w.metadata.is_empty()));
        assert!(engine.surfaces.values().all(|s| s.metadata.is_empty()));
    }

    // snapshot+tail과 전체 로그가 같은 모델과 같은 인코딩을 낸다.
    let models = journal::load_all(&store).expect("load");
    journal::save_snapshot(&mut store, epoch, &models).expect("snapshot");
    drop(store);
    let (store, _) = open(&path);
    let fast = journal::load_all(&store).expect("snapshot + tail");
    let full = journal::full_replay(&store).expect("full replay");
    assert_eq!(fast, full);
    assert_eq!(
        encode_snapshot(&fast).expect("encode"),
        encode_snapshot(&models).expect("encode")
    );
}

#[test]
fn unreadable_slots_are_rejected_without_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    assert!(matches!(
        import_slot(&mut store, epoch, 1, "{ not json", &files()),
        Err(ImportError::Unparsable)
    ));
    let newer = LEGACY_SLOT.replace("\"version\": 1", "\"version\": 99");
    assert!(matches!(
        import_slot(&mut store, epoch, 1, &newer, &files()),
        Err(ImportError::UnsupportedVersion)
    ));
    assert_eq!(store.current_cut().expect("cut").last_batch, None);
}

#[test]
fn unknown_category_moves_the_workspace_to_normal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let slot = LEGACY_SLOT.replace("\"name\": \"ws\",", "\"name\": \"ws\", \"category\": 42,");
    let outcome = import_slot(&mut store, epoch, 1, &slot, &files()).expect("import");
    assert_eq!(outcome.moved_to_normal, vec![0]);
    let model = load(&store, &slot1());
    assert_eq!(
        model.workspaces[&outcome.mapping.workspaces[0].id].category,
        0
    );
}

#[test]
fn imported_out_of_range_view_positions_match_legacy_restore_clamping() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut store, epoch) = open(&dir.path().join("journal.db"));
    let mut layout: serde_json::Value = serde_json::from_str(FULL_SLOT).unwrap();
    layout["active_workspace"] = serde_json::json!(9999);
    layout["workspaces"][0]["focused_pane_index"] = serde_json::json!(9999);
    layout["workspaces"][0]["pane_layout"]["Split"]["first"]["Leaf"]["active_tab"] =
        serde_json::json!(9999);
    let imported = import(&mut store, epoch, &layout.to_string());
    assert_eq!(
        imported.view.active_workspace,
        Some(imported.mapping.workspaces.last().unwrap().id)
    );
    let first = &imported.mapping.workspaces[0];
    let pane = &first.panes[0];
    assert_eq!(
        imported.view.active_tabs[&pane.id],
        pane.tabs.last().unwrap().id
    );
    assert!(
        !imported.view.focused_panes.contains_key(&first.id),
        "missing pane selection uses the presentation's first living pane"
    );
}
