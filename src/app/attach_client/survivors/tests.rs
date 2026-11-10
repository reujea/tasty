use super::*;
use crate::app::attach_client::projection::build_mirror_workspace;
use crate::app::attach_client::resources::MARKDOWN_MIRROR_KIND;
use crate::app::attach_client::tests::{
    MARKDOWN_PLUGIN_ID, created_surfaces, markdown_descriptor, register_markdown_kind,
    single_leaf_tree, supply_ids, test_ids,
};
use crate::model::{EmptySurface, ExplorerPanel};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tasty_remote::client_session::{MirrorStructureIds, SharedFrameSender};

#[test]
fn merge_survivor_mapping_prefers_server_display_name_and_falls_back_to_kind() {
    let ids = test_ids();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let surfaces = vec![
        serde_json::json!({
            "remote_id": 10,
            "role": "mesh",
            "kind": "markdown",
            "plugin_id": "com.tasty.markdown",
            "display_name": "README.md",
        }),
        serde_json::json!({
            "remote_id": 11,
            "role": "mesh",
            "kind": "image",
            "plugin_id": "com.tasty.image",
        }),
    ];

    let mapping = merge_survivor_mapping(&HashMap::new(), &surfaces, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let mesh = &mapping.mesh;

    let local_10 = mapping.remote_to_local[&10];
    let local_11 = mapping.remote_to_local[&11];
    assert_eq!(mesh[&local_10].display_name, "README.md");
    assert_eq!(
        mesh[&local_11].display_name, "image",
        "display_name 필드가 없으면 kind 로 fallback 해야 한다"
    );
}

#[test]
fn merge_survivor_mapping_cleans_up_stale_terminal_on_convert_to_mesh() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    // 별도 발급기를 만들면 기본 workspace의 ID와 충돌하므로 engine의 발급기를 공유한다.
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let surfaces_v1 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 80, "rows": 24,
    })];
    let mut m1 =
        merge_survivor_mapping(&HashMap::new(), &surfaces_v1, &ids, &frame_tx, &mut engine)
            .expect("fixture construction");
    let map1 = m1.remote_to_local.clone();
    let local_10 = map1[&10];
    assert!(
        engine.runtime.terminals.get(local_10).is_some(),
        "최초 terminal survivor 는 Terminal 을 만들어야 한다"
    );
    engine
        .remote
        .attach_mesh_frames
        .update(local_10, vec![1, 2, 3], 1, 1, true);
    engine.set_mirror_surface_foreground(local_10, Some("claude".to_string()));

    // 다음 병합이 이전 kind를 조회할 수 있도록 먼저 실제 트리에 반영한다.
    let tree = serde_json::json!({
        "id": 9, "name": "mirror", "focused_pane": 7,
        "panes": [ {
            "id": 7,
            "tabs": [ {
                "id": 3, "name": "Shell", "active": true, "focused_surface": 10,
                "layout": { "type": "Leaf", "id": 10, "kind": "terminal" }
            } ]
        } ]
    });
    let mut ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        999,
        "mirror",
        &tree,
        &ids,
        &map1,
        &m1.terminals,
        &m1.mesh,
        &m1.explorer,
        &mut m1.markdown,
    )
    .expect("fixture construction");
    ws.mirror = true;
    engine.push_mirror_workspace(ws);

    let surfaces_v2 = vec![serde_json::json!({
        "remote_id": 10,
        "role": "mesh",
        "kind": "markdown",
        "plugin_id": "com.tasty.markdown",
        "display_name": "a.md",
    })];
    let m2 = merge_survivor_mapping(&map1, &surfaces_v2, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let (map2, term2, mesh2, new2) = (
        &m2.remote_to_local,
        &m2.terminals,
        &m2.mesh,
        &m2.newly_created_remote_ids,
    );

    assert_eq!(
        map2[&10], local_10,
        "local id 는 convert 후에도 유지돼야 한다"
    );
    assert!(new2.is_empty(), "survivor 는 신규 취급되면 안 된다");
    assert!(
        !term2.contains(&local_10),
        "markdown 으로 바뀐 뒤에는 더 이상 terminal_locals 에 없어야 한다"
    );
    assert!(
        mesh2.contains_key(&local_10),
        "mesh_locals 에는 새로 등록돼야 한다"
    );
    assert!(
        engine.runtime.terminals.get(local_10).is_none(),
        "옛 Terminal 객체는 즉시 제거돼야 한다"
    );
    assert!(
        engine.remote.attach_mesh_frames.get(local_10).is_none(),
        "옛(terminal 시절의 무의미한) mesh frame 캐시도 제거돼야 한다"
    );
    assert_eq!(
        engine.mirror_surface_foreground(local_10),
        None,
        "terminal 에서 출발한 kind 전환은 옛 전경 이름을 버린다"
    );
}

/// 원격이 닫은 mirror surface는 로컬 닫기처럼 attach 점유 기록도 남기지 않는다.
/// forwarded terminal.kill이 남긴 soft 점유가 이 경로로 사라져야 한다.
#[test]
fn merge_survivor_mapping_forgets_the_occupancy_of_a_remotely_closed_surface() {
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;
    let parent = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];

    let surfaces_v1 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 80, "rows": 24,
    })];
    let m1 = merge_survivor_mapping(&HashMap::new(), &surfaces_v1, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let local_10 = m1.remote_to_local[&10];
    engine
        .occupy_soft(local_10, parent, None)
        .expect("soft 점유");
    engine.set_mirror_surface_foreground(local_10, Some("claude".to_string()));

    let m2 = merge_survivor_mapping(&m1.remote_to_local, &[], &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    assert!(m2.remote_to_local.is_empty());
    assert!(
        engine.live.occupancy.occupancy_of(local_10).is_none(),
        "원격이 닫은 surface 의 soft 점유가 남았다"
    );
    assert_eq!(
        engine.mirror_surface_foreground(local_10),
        None,
        "원격이 닫은 surface 의 전경 이름이 남았다"
    );
}

#[test]
fn merge_survivor_mapping_creates_terminal_when_mesh_survivor_converts_to_terminal() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    // 기본 workspace와 ID가 충돌하지 않도록 engine의 발급기를 공유한다.
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let surfaces_v1 = vec![serde_json::json!({
        "remote_id": 20,
        "role": "mesh",
        "kind": "markdown",
        "plugin_id": "com.tasty.markdown",
        "display_name": "a.md",
    })];
    let mut m1 =
        merge_survivor_mapping(&HashMap::new(), &surfaces_v1, &ids, &frame_tx, &mut engine)
            .expect("fixture construction");
    let map1 = m1.remote_to_local.clone();
    let local_20 = map1[&20];
    assert!(
        engine.runtime.terminals.get(local_20).is_none(),
        "mesh survivor 는 애초에 Terminal 이 없어야 한다"
    );

    let tree = serde_json::json!({
        "id": 9, "name": "mirror", "focused_pane": 7,
        "panes": [ {
            "id": 7,
            "tabs": [ {
                "id": 3, "name": "a.md", "active": true, "focused_surface": 20,
                "layout": { "type": "Leaf", "id": 20, "kind": "markdown" }
            } ]
        } ]
    });
    let mut ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        999,
        "mirror",
        &tree,
        &ids,
        &map1,
        &m1.terminals,
        &m1.mesh,
        &m1.explorer,
        &mut m1.markdown,
    )
    .expect("fixture construction");
    ws.mirror = true;
    engine.push_mirror_workspace(ws);
    // terminal이 아니었던 surface의 이전 cwd도 지워야 한다.
    engine.set_mirror_surface_cwd(local_20, Some("/srv/remote/docs".to_string()));

    let surfaces_v2 = vec![serde_json::json!({
        "remote_id": 20, "role": "terminal", "cols": 80, "rows": 24,
    })];
    let m2 = merge_survivor_mapping(&map1, &surfaces_v2, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let (map2, term2, new2) = (
        &m2.remote_to_local,
        &m2.terminals,
        &m2.newly_created_remote_ids,
    );

    assert_eq!(
        map2[&20], local_20,
        "local id 는 convert 후에도 유지돼야 한다"
    );
    assert!(new2.is_empty(), "survivor 는 신규 취급되면 안 된다");
    assert!(term2.contains(&local_20));
    assert!(
        !engine.remote.mirror_surface_cwd.contains_key(&local_20),
        "kind 전환은 비-terminal 출발이어도 옛 cwd 를 버린다"
    );
    assert!(
        engine.runtime.terminals.get(local_20).is_some(),
        "mesh → terminal convert 는 새 Terminal 을 만들어야 한다(안 그러면 입력이 안 감)"
    );
}

#[test]
fn merge_survivor_mapping_builds_local_markdown_surface_for_markdown_role() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let rx = register_markdown_kind(&engine, MARKDOWN_PLUGIN_ID);
    let ids = test_ids();
    let (tx, _frames) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let mut mapping = merge_survivor_mapping(
        &HashMap::new(),
        &[markdown_descriptor(30)],
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("fixture construction");
    let local = mapping.remote_to_local[&30];
    assert_eq!(mapping.markdown_ids(), HashSet::from([local]));

    let created = created_surfaces(&rx);
    assert_eq!(created.len(), 1, "plugin 에 surface.create 가 한 번 간다");
    assert_eq!(created[0].0, local);
    assert_eq!(created[0].1["remote"]["file"], "/remote/docs/README.md");
    assert!(
        created[0].1.get("file").is_none(),
        "원격 경로를 `file` 로 실으면 plugin 이 client 로컬 파일을 읽는다"
    );

    let ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        999,
        "mirror",
        &single_leaf_tree(30),
        &ids,
        &mapping.remote_to_local,
        &mapping.terminals,
        &mapping.mesh,
        &mapping.explorer,
        &mut mapping.markdown,
    )
    .expect("fixture construction");
    let pane = ws.pane_layout().first_pane().expect("pane");
    let leaf = pane.tabs[0]
        .layout_if_initialized()
        .and_then(|l| l.find_surface(local))
        .expect("markdown leaf");
    assert_eq!(
        leaf.kind(),
        "markdown",
        "빈 surface 가 아니라 markdown surface"
    );
}

/// 다른 소유자의 markdown kind는 사용하지 않고 빈 surface로 둔다.
#[test]
fn markdown_role_stays_empty_when_another_plugin_owns_the_kind() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let rx = register_markdown_kind(&engine, "com.example.other-markdown");
    let ids = test_ids();
    let (tx, _frames) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let mut mapping = merge_survivor_mapping(
        &HashMap::new(),
        &[markdown_descriptor(30)],
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("fixture construction");
    assert!(mapping.markdown.is_empty());
    assert!(created_surfaces(&rx).is_empty());
    let local = mapping.remote_to_local[&30];
    let ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        999,
        "mirror",
        &single_leaf_tree(30),
        &ids,
        &mapping.remote_to_local,
        &mapping.terminals,
        &mapping.mesh,
        &mapping.explorer,
        &mut mapping.markdown,
    )
    .expect("fixture construction");
    let pane = ws.pane_layout().first_pane().expect("pane");
    let leaf = pane.tabs[0]
        .layout_if_initialized()
        .and_then(|l| l.find_surface(local))
        .expect("leaf");
    assert_eq!(leaf.kind(), "empty");
    assert!(
        engine
            .runtime
            .surfaces
            .get(&local)
            .and_then(|surface| surface.as_any().downcast_ref::<EmptySurface>())
            .is_none_or(|empty| empty.deferred.is_none())
    );
}

#[test]
fn markdown_role_waits_for_the_plugin_kind_and_reifies_as_a_mirror_document() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    // The common fixture registers markdown; this scenario starts with no live kind.
    assert_eq!(
        engine
            .runtime
            .surface_registry
            .withdraw_plugin(MARKDOWN_PLUGIN_ID),
        vec![MARKDOWN_MIRROR_KIND],
    );
    assert!(
        engine
            .runtime
            .surface_registry
            .get_live(MARKDOWN_MIRROR_KIND)
            .is_none()
    );
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _frames) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let mut mapping = merge_survivor_mapping(
        &HashMap::new(),
        &[markdown_descriptor(30)],
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("fixture construction");
    let local = mapping.remote_to_local[&30];
    assert_eq!(
        mapping.markdown_ids(),
        HashSet::from([local]),
        "placeholder 도 이 세션의 markdown leaf 로 센다 — destroy·끊김 통지 대상"
    );
    let ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        999,
        "mirror",
        &single_leaf_tree(30),
        &ids,
        &mapping.remote_to_local,
        &mapping.terminals,
        &mapping.mesh,
        &mapping.explorer,
        &mut mapping.markdown,
    )
    .expect("fixture construction");
    assert!(
        engine
            .runtime
            .surfaces
            .get(&local)
            .and_then(|surface| surface.as_any().downcast_ref::<EmptySurface>())
            .is_some_and(|empty| empty.deferred.is_some()),
        "kind 가 없으면 kind 대기 placeholder"
    );
    engine.push_mirror_workspace(ws);

    engine.reify_displayed_mirror_resources(&[local]);
    assert_eq!(engine.find_surface_by_id(local).unwrap().kind(), "empty");
    let rx = register_markdown_kind(&engine, MARKDOWN_PLUGIN_ID);
    engine.reify_displayed_mirror_resources(&[local]);

    let leaf = engine.find_surface_by_id(local).expect("leaf");
    assert_eq!(leaf.kind(), "markdown");
    let restored: Vec<Value> = rx
        .try_iter()
        .filter_map(|cmd| match cmd {
            crate::plugin_bridge::host_cmd::HostCmd::RemoteSurfaceRestored {
                surface_id,
                data,
                ..
            } if surface_id == local => Some(data),
            _ => None,
        })
        .collect();
    assert_eq!(restored.len(), 1, "plugin 에 surface.restore 가 한 번 간다");
    assert_eq!(restored[0]["remote"]["file"], "/remote/docs/README.md");
    assert_eq!(restored[0]["display_name"], "README.md");
    assert!(restored[0].get("file").is_none());
}

fn terminal_dims(engine: &mut EngineMut<'_>, local: u32) -> (usize, usize) {
    let t = engine
        .runtime
        .terminals
        .get_mut(local)
        .expect("terminal mirror");
    (t.cols(), t.rows())
}

/// 재연결에서 kind 가 그대로인 terminal mirror 는 병합만으로는 옛 grid 를 유지한다. 재연결 설치는
/// descriptor 가 알린 서버 크기로 맞춰야 하고, 크기가 없는 descriptor 는 건드리지 않는다.
#[test]
fn reconnect_sizes_a_surviving_terminal_mirror_to_the_server_grid() {
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let v1 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 100, "rows": 30,
    })];
    let m1 = merge_survivor_mapping(&HashMap::new(), &v1, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let local = m1.remote_to_local[&10];
    assert_eq!(terminal_dims(&mut engine, local), (100, 30));

    let v2 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 120, "rows": 40,
    })];
    let m2 = merge_survivor_mapping(&m1.remote_to_local, &v2, &ids, &frame_tx, &mut engine)
        .expect("reconnect merge");
    assert_eq!(m2.remote_to_local[&10], local, "같은 로컬 ID 를 재사용한다");
    assert_eq!(
        terminal_dims(&mut engine, local),
        (100, 30),
        "병합은 살아남은 mirror 의 크기를 바꾸지 않는다 — 이 시험의 전제"
    );

    let sizeless = vec![serde_json::json!({ "remote_id": 10, "role": "terminal" })];
    apply_reconnect_terminal_sizes(
        &m2.remote_to_local,
        &sizeless,
        &mut engine.runtime.terminals,
    );
    assert_eq!(
        terminal_dims(&mut engine, local),
        (100, 30),
        "크기가 없는 descriptor 로 기본값에 맞추지 않는다"
    );

    apply_reconnect_terminal_sizes(&m2.remote_to_local, &v2, &mut engine.runtime.terminals);
    assert_eq!(terminal_dims(&mut engine, local), (120, 40));
}

/// 재연결 설치 핵심부는 병합 뒤 살아남은 terminal mirror 를 서버 크기로 맞추고, 옛 연결의 크기 요청
/// 대기와 중복 전송 기록을 비운다. 크기 맞춤이나 상태 초기화 중 하나라도 빠지면 이 시험이 실패한다.
#[test]
fn reconnect_install_sizes_survivors_and_clears_the_resize_wait() {
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let v1 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 100, "rows": 30,
    })];
    let m1 = merge_survivor_mapping(&HashMap::new(), &v1, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let local = m1.remote_to_local[&10];
    let mut remote_to_local = m1.remote_to_local.clone();
    let mut sync = tasty_remote::resize_sync::ResizeSync::for_connection(true);
    sync.note_sent(10, 120, 40, std::time::Instant::now());
    assert!(
        sync.next_deadline().is_some(),
        "이 시험의 전제: 옛 연결의 대기"
    );

    let v2 = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 120, "rows": 40,
    })];
    install_reconnected_survivors(
        &mut remote_to_local,
        &mut sync,
        false,
        &v2,
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("reconnect install");
    assert_eq!(remote_to_local[&10], local, "같은 로컬 ID 를 재사용한다");
    assert_eq!(terminal_dims(&mut engine, local), (120, 40));
    assert_eq!(
        sync.next_deadline(),
        None,
        "옛 연결의 응답은 기다리지 않는다"
    );
    assert!(
        sync.should_send(10, 120, 40),
        "새 연결에서 같은 크기도 다시 보낸다"
    );
    assert!(!sync.acks_expected(), "새 연결의 서버 기능을 따른다");
}

/// terminal → terminal 재연결: 옛 연결의 전경 이름은 새 연결의 첫 Foreground 전까지 쓰지 않는다.
/// 같은 연결의 구조 변경(병합만)에서는 살아남은 terminal 의 이름을 유지한다.
#[test]
fn reconnect_install_forgets_the_old_foreground_but_a_structural_merge_keeps_it() {
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;
    let terminal = vec![serde_json::json!({
        "remote_id": 10, "role": "terminal", "cols": 80, "rows": 24,
    })];
    let m1 = merge_survivor_mapping(&HashMap::new(), &terminal, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let local = m1.remote_to_local[&10];
    engine.set_mirror_surface_foreground(local, Some("claude".to_string()));
    let old_generation = engine.foreground_generation(local);

    let m2 = merge_survivor_mapping(&m1.remote_to_local, &terminal, &ids, &frame_tx, &mut engine)
        .expect("structural merge");
    assert_eq!(m2.remote_to_local[&10], local);
    assert_eq!(
        engine.mirror_surface_foreground(local),
        Some("claude"),
        "같은 연결의 구조 변경은 이름을 지우지 않는다"
    );

    let mut remote_to_local = m2.remote_to_local.clone();
    let mut sync = tasty_remote::resize_sync::ResizeSync::for_connection(true);
    install_reconnected_survivors(
        &mut remote_to_local,
        &mut sync,
        true,
        &terminal,
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("reconnect install");
    assert_eq!(remote_to_local[&10], local, "같은 로컬 ID 를 재사용한다");
    assert_eq!(
        engine.mirror_surface_foreground(local),
        None,
        "새 연결의 첫 값 전(구버전 서버면 계속) 이름 미상"
    );
    assert_ne!(
        engine.foreground_generation(local),
        old_generation,
        "옛 연결에서 연 캡처 배너는 다음 tick 에 닫힌다"
    );

    engine.set_mirror_surface_foreground(local, Some("claude".to_string()));
    assert_ne!(
        engine.foreground_generation(local),
        old_generation,
        "같은 이름이 다시 와도 옛 배너의 세대와 겹치지 않는다"
    );
}

fn explorer_fixture(
    engine: &mut EngineMut<'_>,
) -> (
    crate::runtime::id_reservations::ReservedIds,
    SharedFrameSender,
    Vec<Value>,
    SurvivorMapping,
    u32,
) {
    let ids = test_ids();
    supply_ids(engine);
    let (tx, _rx) = tasty_remote::connection::channel();
    let descriptors =
        vec![serde_json::json!({"remote_id": 10, "role": "explorer", "root": "/remote/a"})];
    let first = merge_survivor_mapping(&HashMap::new(), &descriptors, &ids, &tx, engine).unwrap();
    let local = first.remote_to_local[&10];
    (ids, tx, descriptors, first, local)
}

fn explorer_mut<'a>(engine: &'a mut EngineMut<'_>, local: u32) -> &'a mut ExplorerPanel {
    engine
        .runtime
        .surfaces
        .get_mut(&local)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<ExplorerPanel>()
        .unwrap()
}

#[test]
fn explorer_survivor_preserves_all_tabs_and_view_state_after_client_navigation() {
    use crate::adapters::ui::surface::explorer::view::{ExplorerViewStore, LoadState};
    use tasty_model::{ExplorerViewMode, SortColumn, SortDir};
    use tasty_type_geometry::length::LogicalPx;
    let (_, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let (ids, tx, descriptors, first, local) = explorer_fixture(&mut engine);
    let panel = explorer_mut(&mut engine, local);
    panel.add_tab();
    panel.add_tab();
    panel.active = 1;
    panel
        .active_tab_mut()
        .navigate_to("/remote/client-choice".into());
    panel.active_tab_mut().view_mode = ExplorerViewMode::Grid;
    panel.active_tab_mut().sort_column = SortColumn::Size;
    panel.active_tab_mut().sort_dir = SortDir::Desc;
    panel.show_hidden = true;
    panel.preview.open = true;
    panel.preview.width = Some(LogicalPx(240.0));
    let tab_ids: Vec<_> = panel.tabs.iter().map(|tab| tab.id()).collect();
    let mut views = ExplorerViewStore::default();
    for index in 0..3 {
        panel.active = index;
        let name = format!("tab{index}.txt");
        let path = panel.current_root().join(&name);
        let view = views.get_or_init(panel, Some(7));
        let request = view.drain_outbox().pop().unwrap().request_id;
        assert!(view.apply_remote_list_dir_result(
            request,
            panel,
            Ok(vec![crate::core::fs_list::DirEntryInfo {
                path: path.clone(),
                name: name.clone(),
                ..Default::default()
            }])
        ));
        view.select_only(&path);
        view.open_find();
        view.find.as_mut().unwrap().set_query(&name);
    }
    panel.active = 1;
    views.get_or_init(panel, Some(7));
    // The server descriptor is unchanged, even though the client active path differs.
    merge_survivor_mapping(&first.remote_to_local, &descriptors, &ids, &tx, &mut engine).unwrap();
    let panel = explorer_mut(&mut engine, local);
    assert_eq!(panel.tabs.len(), 3);
    assert_eq!(panel.active, 1);
    assert_eq!(
        panel.tabs.iter().map(|tab| tab.id()).collect::<Vec<_>>(),
        tab_ids
    );
    assert_eq!(
        panel.current_root(),
        std::path::Path::new("/remote/client-choice")
    );
    assert!(panel.active_tab().can_go_back());
    assert_eq!(panel.active_tab().view_mode, ExplorerViewMode::Grid);
    assert_eq!(panel.active_tab().sort_column, SortColumn::Size);
    assert_eq!(panel.active_tab().sort_dir, SortDir::Desc);
    assert!(panel.show_hidden && panel.preview.open);
    assert_eq!(panel.preview.width, Some(LogicalPx(240.0)));
    for index in 0..3 {
        panel.active = index;
        let name = format!("tab{index}.txt");
        let path = panel.current_root().join(&name);
        let view = views.get_or_init(panel, Some(7));
        assert_eq!(view.state, LoadState::Ok);
        assert!(view.drain_outbox().is_empty());
        assert_eq!(view.selected.len(), 1);
        assert!(view.selected.contains(&path));
        assert_eq!(view.find.as_ref().unwrap().query(), name);
    }
    panel.active = 2;
    merge_survivor_mapping(&first.remote_to_local, &descriptors, &ids, &tx, &mut engine).unwrap();
    assert_eq!(explorer_mut(&mut engine, local).active, 2);
}

#[test]
fn explorer_server_root_change_replaces_tabs_and_discards_old_view_receipts() {
    use crate::adapters::ui::surface::explorer::view::ExplorerViewStore;
    let (_, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let (ids, tx, _descriptors, first, local) = explorer_fixture(&mut engine);
    let panel = explorer_mut(&mut engine, local);
    let old_id = panel.active_tab().id();
    panel.add_tab();
    panel
        .active_tab_mut()
        .navigate_to("/remote/new-server-root".into());
    let mut views = ExplorerViewStore::default();
    let view = views.get_or_init(panel, Some(7));
    let pending = view.drain_outbox().pop().unwrap().request_id;
    view.select_only(&panel.current_root().join("old"));
    view.open_find();
    // A client may already be looking at the new server root: compare server-to-server.
    let changed = vec![
        serde_json::json!({"remote_id": 10, "role": "explorer", "root": "/remote/new-server-root"}),
    ];
    merge_survivor_mapping(&first.remote_to_local, &changed, &ids, &tx, &mut engine).unwrap();
    let panel = explorer_mut(&mut engine, local);
    assert_eq!(panel.tabs.len(), 1);
    assert_ne!(panel.active_tab().id(), old_id);
    let view = views.get_or_init(panel, Some(7));
    assert!(view.selected.is_empty() && view.find.is_none());
    assert_eq!(view.parked_tab_count(), 0);
    assert!(!view.apply_remote_list_dir_result(pending, panel, Ok(Vec::new())));
    assert_ne!(view.drain_outbox().pop().unwrap().request_id, pending);
}

#[test]
fn explorer_new_connection_epoch_replaces_same_root_tabs_and_old_requests() {
    use crate::adapters::ui::surface::explorer::view::ExplorerViewStore;
    let (_, mut session) = crate::state::tests::test_state();
    let mut engine = session.borrow_mut();
    let (ids, old_tx, descriptors, mut first, local) = explorer_fixture(&mut engine);
    let panel = explorer_mut(&mut engine, local);
    panel.add_tab();
    let old_id = panel.active_tab().id();
    let mut views = ExplorerViewStore::default();
    let view = views.get_or_init(panel, Some(7));
    let pending = view.drain_outbox().pop().unwrap().request_id;
    view.select_only(&panel.current_root().join("old"));
    view.open_find();
    old_tx.retire();
    let (new_tx, _rx) = tasty_remote::connection::channel();
    let mut resize = tasty_remote::resize_sync::ResizeSync::for_connection(false);
    install_reconnected_survivors(
        &mut first.remote_to_local,
        &mut resize,
        false,
        &descriptors,
        &ids,
        &new_tx,
        &mut engine,
    )
    .unwrap();
    let panel = explorer_mut(&mut engine, local);
    assert_eq!(panel.tabs.len(), 1);
    assert_ne!(panel.active_tab().id(), old_id);
    let view = views.get_or_init(panel, Some(7));
    assert!(view.selected.is_empty() && view.find.is_none());
    assert_eq!(view.parked_tab_count(), 0);
    assert!(!view.apply_remote_list_dir_result(pending, panel, Ok(Vec::new())));
}
