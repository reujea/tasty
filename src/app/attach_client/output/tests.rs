use super::*;
use crate::app::attach_client::projection::build_mirror_workspace;
use crate::app::attach_client::survivors::merge_survivor_mapping;
use crate::app::attach_client::tests::{
    MARKDOWN_PLUGIN_ID, ParkedEngine, created_surfaces, markdown_descriptor, parked_ids,
    parked_with_mirror, register_markdown_kind, single_leaf_tree, supply_ids, test_ids,
    test_session,
};
use crate::app::attach_client::wire::mirror_event_from_control;
use crate::ipc::stream::{StreamControl, StreamTag};
use crate::model::Workspace;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tasty_remote::client_session::{
    MirrorEvent, MirrorOutbox, MirrorStructureIds, OutFrame, SessionState, SharedFrameSender,
};
use tasty_terminal::Terminal;

#[test]
fn cwd_push_applies_as_remote_origin_and_null_clears_it() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ws_id = 9_000u32;
    let (pane_id, tab_id, local_surface) = (9_001u32, 9_002u32, 9_003u32);
    let remote_surface = 42u32;
    let mut mirror_ws = Workspace::new_with_terminal_marker(
        ws_id,
        "mirror".to_string(),
        pane_id,
        tab_id,
        local_surface,
    );
    mirror_ws.mirror = true;
    engine.push_mirror_workspace(mirror_ws);
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    let mut sess = test_session(ws_id, HashMap::from([(remote_surface, local_surface)]));
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Cwd(
                remote_surface,
                Some("/srv/remote/proj".to_string()),
            )],
        );
    }
    assert_eq!(
        engine.surface_cwd(local_surface),
        Some(crate::core::state::SurfaceCwd::Remote(
            crate::core::state::RemoteCwd::new("/srv/remote/proj")
        ))
    );
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine.as_ref().read(), local_surface),
        None,
        "원격 cwd를 로컬 실행 경로로 사용하면 안 된다"
    );

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Cwd(remote_surface, None)],
        );
    }
    assert!(
        engine.remote.mirror_surface_cwd.is_empty(),
        "null push 는 옛 원격 경로를 남기지 않는다"
    );
}

/// 서버의 Foreground 프레임은 wire 파싱과 원격→로컬 ID 치환을 거쳐 mirror 이름이 되고 null 은 지운다.
#[test]
fn foreground_push_lands_on_the_local_id_and_null_clears_it() {
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ws_id = 9_000u32;
    let (pane_id, tab_id, local_surface) = (9_001u32, 9_002u32, 9_003u32);
    let remote_surface = 42u32;
    let mut mirror_ws = Workspace::new_with_terminal_marker(
        ws_id,
        "mirror".to_string(),
        pane_id,
        tab_id,
        local_surface,
    );
    mirror_ws.mirror = true;
    engine.push_mirror_workspace(mirror_ws);
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    let mut sess = test_session(ws_id, HashMap::from([(remote_surface, local_surface)]));
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    let frame = |name: Option<&str>| {
        let payload = serde_json::to_vec(&StreamControl::Foreground {
            surface_id: remote_surface,
            name: name.map(str::to_string),
        })
        .expect("serialize");
        mirror_event_from_control(&payload).expect("foreground event")
    };

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![frame(Some("claude"))],
        );
    }
    assert_eq!(
        engine.mirror_surface_foreground(local_surface),
        Some("claude")
    );
    assert_eq!(
        engine.mirror_surface_foreground(remote_surface),
        None,
        "원격 ID에 저장하지 않는다"
    );

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, vec![frame(None)]);
    }
    assert_eq!(engine.mirror_surface_foreground(local_surface), None);
}

/// 구버전 서버로 재연결: Activity·Cwd 는 오지만 Foreground 는 오지 않으므로 옛 이름이 되살아나지 않는다.
/// 새 서버가 첫 값을 보내면 그때부터 이름을 쓴다.
#[test]
fn after_a_reconnect_without_foreground_frames_the_name_stays_unknown() {
    use crate::app::attach_client::survivors::install_reconnected_survivors;
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _rx) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;
    let terminal = vec![serde_json::json!({
        "remote_id": 42, "role": "terminal", "cols": 80, "rows": 24,
    })];
    let m1 = merge_survivor_mapping(&HashMap::new(), &terminal, &ids, &frame_tx, &mut engine)
        .expect("fixture construction");
    let local = m1.remote_to_local[&42];
    let mut sess = test_session(9_000, m1.remote_to_local.clone());
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Foreground(42, Some("claude".to_string()))],
        );
    }
    assert_eq!(engine.mirror_surface_foreground(local), Some("claude"));

    install_reconnected_survivors(
        &mut sess.state.remote_to_local,
        &mut sess.state.resize_sync,
        false,
        &terminal,
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("reconnect install");
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![
                MirrorEvent::Activity(42, false),
                MirrorEvent::Cwd(42, Some("/srv".to_string())),
            ],
        );
    }
    assert_eq!(
        engine.mirror_surface_foreground(local),
        None,
        "구버전 서버의 재연결 뒤에는 이름 미상"
    );

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Foreground(42, Some("vim".to_string()))],
        );
    }
    assert_eq!(engine.mirror_surface_foreground(local), Some("vim"));
}

/// wire에서 파싱한 실패 사유가 사용자 안내까지 유지되는지 확인한다.
#[test]
fn a_structural_failure_reason_reaches_the_toast_verbatim() {
    // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 기준 문구를 읽기 전에 초기화한다.
    crate::i18n::init("en");
    let toast_for = |reason: Option<&str>| {
        let payload = serde_json::to_vec(&StreamControl::StructuralResult {
            created_surface_id: None,
            op_id: 0,
            ok: false,
            reason: reason.map(str::to_string),
            history_omitted: None,
        })
        .unwrap();
        let ev = mirror_event_from_control(&payload).expect("실패 회신은 이벤트가 된다");
        let mut sess = test_session(9_000, HashMap::new());
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        supply_ids(&engine);
        {
            let mut host = MirrorHost::windowed(&mut state, &mut engine);
            apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, vec![ev]);
        }
        let messages: Vec<String> = state
            .toasts
            .messages()
            .into_iter()
            .map(str::to_string)
            .collect();
        messages
    };
    let base = crate::i18n::t("attach.toast.mirror_structural_forward_failed").to_string();

    assert_eq!(
        toast_for(Some("unknown surface kind: definitely-not-registered")),
        vec![format!(
            "{base} (unknown surface kind: definitely-not-registered)"
        )],
        "원격 사유가 원문 그대로 괄호 안에 실려야 한다"
    );
    assert_eq!(
        toast_for(None),
        vec![base],
        "wire 에 사유가 없으면 괄호 없는 기본 문구다"
    );
}

#[test]
fn a_desync_detaches_once_renews_the_stream_and_sums_later_notices() {
    use tasty_terminal::{OUTPUT_RETENTION_MAX_BYTES, OutputCursor, OutputReadRequest};

    let _home = crate::test_support::TastyHomeGuard::new();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let (remote_surface, local_surface) = (42u32, 9_003u32);
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    let mut sess = test_session(9_000, HashMap::from([(remote_surface, local_surface)]));
    let (tx, frames_out) = tasty_remote::connection::channel();
    sess.transport.frame_tx = tx;
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;

    let read = |engine: &crate::runtime::engine_access::EngineRef<'_>, expect: Option<String>| {
        engine
            .runtime
            .terminals
            .get(local_surface)
            .expect("mirror terminal")
            .read_output(&OutputReadRequest {
                from: OutputCursor::At(0),
                max_bytes: OUTPUT_RETENTION_MAX_BYTES,
                strip_ansi: false,
                expect_stream: expect,
            })
    };
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Data(remote_surface, b"before".to_vec())],
        );
    }
    let before = read(&engine.as_ref(), None).expect("read").stream;

    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![
                MirrorEvent::Desynced { frames: 3 },
                MirrorEvent::Desynced { frames: 2 },
            ],
        );
    }
    assert_eq!(
        sess.state.resync_pending,
        Some(5),
        "기다리는 동안의 통지는 합산한다"
    );
    let sent: Vec<OutFrame> = frames_out.try_iter().map(|queued| queued.frame).collect();
    assert_eq!(
        sent.len(),
        1,
        "재attach 를 위해 옛 연결을 놓는 것은 한 번이다"
    );
    assert_eq!(sent[0].tag, StreamTag::Detach);
    assert!(
        read(&engine.as_ref(), Some(before)).is_err(),
        "손실 전 표지로 읽으면 stream 불일치여야 한다"
    );
    assert_eq!(
        sess.state.phase,
        SessionState::Connected,
        "재attach 는 EOF 를 본 뒤에 건다 — 통지만으로 상태를 바꾸지 않는다"
    );
}

/// 창 없는 수동 mirror는 재attach할 창이 생길 때까지 옛 연결을 유지한다.
#[test]
fn a_loss_on_a_parked_mirror_without_an_anchor_waits_for_a_window_instead_of_closing() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let (remote_surface, local_surface) = (42u32, 9_003u32);
    engine
        .runtime
        .terminals
        .insert(local_surface, Terminal::new_detached(80, 24), None);
    let mut sess = test_session(9_000, HashMap::from([(remote_surface, local_surface)]));
    assert!(
        sess.state.saved_row.is_none(),
        "전제: 에이전트 mirror(저장 행 아님)"
    );
    let (tx, frames_out) = tasty_remote::connection::channel();
    sess.transport.frame_tx = tx;
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;

    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::Desynced { frames: 4 }],
        );
    }
    assert_eq!(sess.state.resync_pending, Some(4));
    assert!(sess.state.resync_awaiting_window);
    assert!(
        frames_out.try_iter().next().is_none(),
        "parked 에서 옛 연결을 놓으면 재attach 가 창을 못 찾아 mirror 가 정리된다"
    );
    assert_eq!(
        disconnect_disposition(
            sess.transport.disconnected.load(Ordering::SeqCst),
            sess.state.phase,
            sess.resync_released(),
            sess.state.saved_row.is_some(),
        ),
        DisconnectDisposition::None
    );
    assert_eq!(
        disconnect_disposition(
            true,
            sess.state.phase,
            sess.resync_released(),
            sess.state.saved_row.is_some(),
        ),
        DisconnectDisposition::Cleanup
    );

    let toasts_before = state.toasts.len();
    {
        let mut host = MirrorHost::parked(&mut state, &mut engine);
        resume_resync_in_window(&mut sess, &mut host);
    }
    assert!(
        frames_out.try_iter().next().is_none(),
        "아직 창이 없으면 놓지 않는다"
    );
    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        resume_resync_in_window(&mut sess, &mut host);
    }
    let sent: Vec<OutFrame> = frames_out.try_iter().map(|queued| queued.frame).collect();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].tag, StreamTag::Detach);
    assert!(!sess.state.resync_awaiting_window);
    assert_eq!(state.toasts.len(), toasts_before + 1, "재동기화 안내 toast");
    assert_eq!(
        disconnect_disposition(
            true,
            sess.state.phase,
            sess.resync_released(),
            sess.state.saved_row.is_some(),
        ),
        DisconnectDisposition::Resync,
        "Detach를 보낸 뒤 EOF를 받으면 재attach한다"
    );
}

#[test]
fn structural_delta_reuses_markdown_survivor_and_reports_removed_ones() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let rx = register_markdown_kind(&engine, MARKDOWN_PLUGIN_ID);
    let ids = test_ids();
    let (tx, _frames) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let mut m1 = merge_survivor_mapping(
        &HashMap::new(),
        &[markdown_descriptor(30)],
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("fixture construction");
    let local = m1.remote_to_local[&30];
    let ws_id = 999;
    let mut ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        ws_id,
        "mirror",
        &single_leaf_tree(30),
        &ids,
        &m1.remote_to_local,
        &m1.terminals,
        &m1.mesh,
        &m1.explorer,
        &mut m1.markdown,
    )
    .expect("fixture construction");
    ws.mirror = true;
    engine.push_mirror_workspace(ws);
    assert_eq!(created_surfaces(&rx).len(), 1);
    let webview_url = engine
        .find_surface_by_id(local)
        .and_then(|s| {
            s.as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
        })
        .map(|rs| Arc::clone(&rs.webview_url))
        .expect("RemoteSurface");

    let mut sess = test_session(ws_id, m1.remote_to_local.clone());
    sess.state.markdown_locals = HashSet::from([local]);

    let removed = apply_mirror_structural_delta(
        &mut navigation,
        &mut sess,
        &mut engine,
        7,
        &single_leaf_tree(30),
        &[markdown_descriptor(30)],
        None,
    )
    .expect("mirror delta");
    assert!(removed.markdown.is_empty() && removed.html.is_empty());
    assert!(
        created_surfaces(&rx).is_empty(),
        "survivor 에 surface.create 를 다시 보내면 문서가 로딩부터 다시 시작한다"
    );
    let shared = engine
        .find_surface_by_id(local)
        .and_then(|s| {
            s.as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
        })
        .expect("still a RemoteSurface");
    assert!(Arc::ptr_eq(&shared.webview_url, &webview_url));

    let removed = apply_mirror_structural_delta(
        &mut navigation,
        &mut sess,
        &mut engine,
        7,
        &single_leaf_tree(30),
        &[serde_json::json!({ "remote_id": 30, "role": "terminal", "cols": 80, "rows": 24 })],
        None,
    )
    .expect("mirror delta");
    assert_eq!(removed.markdown, vec![local]);
    assert!(sess.state.markdown_locals.is_empty());
}

#[test]
fn markdown_mirror_local_maps_only_markdown_leaves() {
    let mut sess = test_session(1, HashMap::from([(30, 300), (31, 310)]));
    sess.state.markdown_locals.insert(300);
    assert_eq!(markdown_mirror_local(&sess, 30), Some(300));
    assert_eq!(markdown_mirror_local(&sess, 31), None, "터미널 leaf");
    assert_eq!(
        markdown_mirror_local(&sess, 99),
        None,
        "이 세션이 mirror 하지 않는 문서"
    );
}

#[test]
fn mirror_output_host_prefers_window_then_parked_then_none() {
    let ws_id = 9_000u32;
    let parked = parked_with_mirror(ws_id, 9_003);
    let wid = winit::window::WindowId::from(7u64);
    assert_eq!(
        mirror_output_host(Some(wid), parked_ids(&parked), ws_id),
        Some(MirrorOutputHost::Window(wid)),
        "창 있는 engine 이 있으면 그쪽"
    );
    assert_eq!(
        mirror_output_host(None, parked_ids(&parked), ws_id),
        Some(MirrorOutputHost::Parked(parked[1].session.id)),
        "창이 없으면 mirror 를 든 parked engine(두 번째)"
    );
    assert_eq!(
        mirror_output_host(None, parked_ids(&parked), 424_242),
        None,
        "어느 engine 에도 없으면 None — drain 하지 않는다"
    );
}

#[test]
fn parked_engine_receives_mirror_data_and_structural_delta() {
    let ws_id = 9_000u32;
    let (survivor_remote, survivor_local, new_remote) = (42u32, 9_003u32, 43u32);
    let mut parked = parked_with_mirror(ws_id, survivor_local);
    let untouched_ws_count = parked[0].session.core_state.workspaces().len();
    let original_generation = parked[1]
        .session
        .runtime
        .terminals
        .generation(survivor_local)
        .expect("original mirror terminal");
    let mut sess = test_session(ws_id, HashMap::from([(survivor_remote, survivor_local)]));

    let tree = serde_json::json!({
        "id": 7, "name": "mirror", "focused_pane": 70,
        "panes": [ {
            "id": 70,
            "tabs": [ {
                "id": 30, "name": "Shell", "active": true, "focused_surface": survivor_remote,
                "layout": {
                    "type": "Split", "direction": "vertical", "ratio": 0.5,
                    "focus_second": false,
                    "first": { "type": "Leaf", "id": survivor_remote, "kind": "terminal" },
                    "second": { "type": "Leaf", "id": new_remote, "kind": "terminal" }
                }
            } ]
        } ]
    });
    let surfaces = vec![
        serde_json::json!({ "remote_id": survivor_remote, "role": "terminal", "cols": 80, "rows": 24 }),
        serde_json::json!({ "remote_id": new_remote, "role": "terminal", "cols": 80, "rows": 24 }),
    ];
    let events = vec![
        MirrorEvent::Data(survivor_remote, b"hello-parked".to_vec()),
        MirrorEvent::StructuralDelta {
            workspace_id: 7,
            tree,
            surfaces,
        },
        MirrorEvent::Data(new_remote, b"world-new".to_vec()),
    ];

    let id =
        find_parked_with_workspace(parked_ids(&parked), ws_id).expect("mirror 를 든 parked engine");
    let pidx = parked
        .iter()
        .position(|p| p.session.id == id)
        .expect("찾은 id의 항목");
    {
        let ParkedEngine {
            view_restore: state,
            session,
        } = &mut parked[pidx];
        let mut engine = session.borrow_mut();
        supply_ids(&engine);
        let mut host = MirrorHost::parked(state, &mut engine);
        let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, events);
    }

    let engine = parked[pidx].session.as_ref();
    assert_eq!(
        engine.runtime.terminals.generation(survivor_local),
        Some(original_generation),
        "a surviving mirror keeps its original terminal owner"
    );
    let survivor = engine
        .runtime
        .terminals
        .get(survivor_local)
        .expect("survivor mirror 터미널은 delta 뒤에도 같은 local id 로 남는다");
    assert!(
        survivor.screen_text(false).contains("hello-parked"),
        "parked 동안 도착한 Data 가 mirror grid 에 남아야 한다: {:?}",
        survivor.screen_text(false)
    );
    let new_local = *sess
        .state
        .remote_to_local
        .get(&new_remote)
        .expect("delta 가 새 remote surface 를 매핑에 넣어야 한다(desync 방지)");
    let fresh = engine
        .runtime
        .terminals
        .get(new_local)
        .expect("delta 가 새 mirror 터미널을 만들어야 한다");
    assert!(
        fresh.screen_text(false).contains("world-new"),
        "delta 이후의 Data 가 갱신된 매핑으로 새 터미널에 라우팅돼야 한다: {:?}",
        fresh.screen_text(false)
    );
    let ws = engine
        .workspaces()
        .into_iter()
        .find(|w| w.id == ws_id)
        .expect("mirror 워크스페이스는 같은 local id 로 교체된다");
    let sids = ws.all_surface_ids();
    assert!(
        sids.contains(&survivor_local) && sids.contains(&new_local),
        "{sids:?}"
    );
    assert_eq!(
        parked[0].session.core_state.workspaces().len(),
        untouched_ws_count,
        "무관한 parked engine 은 건드리지 않는다"
    );
}

/// None 분기와 아래 실제 적용 시험을 함께 검사한다.
/// host 없이 take_for를 호출할 수 없다는 API 제약은 이 실행 시험의 검출 범위와 별개다.
#[test]
fn no_host_leaves_the_mirror_buffer_untouched() {
    let ws_id = 9_000u32;
    let mut sess = test_session(ws_id, HashMap::new());
    for event in [
        MirrorEvent::Data(1, b"a".to_vec()),
        MirrorEvent::Resize(1, 10, 5),
    ] {
        assert!(sess.transport.output.push(event));
    }
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;

    let applied = apply_pending_mirror_output(&mut sess, None, &mut plugin_manager);

    assert!(!applied, "적용 대상이 없으면 적용했다고 보고하지 않는다");
    let buf = sess.transport.output.drain();
    assert_eq!(
        buf.len(),
        2,
        "host 가 없으면 버퍼는 그대로 남아 다음 호출이 다시 시도한다"
    );
    assert!(matches!(buf[0], MirrorEvent::Data(1, ref b) if b == b"a"));
    assert!(matches!(buf[1], MirrorEvent::Resize(1, 10, 5)));
}

#[test]
fn a_host_drains_and_applies_the_mirror_buffer() {
    let ws_id = 9_000u32;
    let local_surface = 9_003u32;
    let remote_surface = 42u32;
    let mut parked = parked_with_mirror(ws_id, local_surface);
    let mut sess = test_session(ws_id, HashMap::from([(remote_surface, local_surface)]));
    sess.transport
        .output
        .push(MirrorEvent::Data(remote_surface, b"applied-here".to_vec()));
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;

    let id =
        find_parked_with_workspace(parked_ids(&parked), ws_id).expect("mirror 를 든 parked engine");
    let pidx = parked
        .iter()
        .position(|p| p.session.id == id)
        .expect("찾은 id의 항목");
    let applied = {
        let ParkedEngine {
            view_restore: state,
            session,
        } = &mut parked[pidx];
        let mut engine = session.borrow_mut();
        supply_ids(&engine);
        apply_pending_mirror_output(
            &mut sess,
            Some(MirrorHost::parked(state, &mut engine)),
            &mut plugin_manager,
        )
    };

    assert!(applied);
    assert!(
        sess.transport.output.drain().is_empty(),
        "적용했으면 버퍼는 비워진다"
    );
    let term = parked[pidx]
        .session
        .runtime
        .terminals
        .get(local_surface)
        .expect("mirror 터미널");
    assert!(term.screen_text(false).contains("applied-here"));
}

#[test]
fn parked_host_does_not_stack_toasts_but_windowed_does() {
    let ws_id = 9_000u32;
    let mut sess = test_session(ws_id, HashMap::new());
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    let failure = || vec![MirrorEvent::StructuralFailed(0, Some("nope".to_string()))];

    let (mut parked_state, mut parked_engine_session) = crate::state::tests::test_state();
    let mut parked_engine = parked_engine_session.borrow_mut();
    {
        let mut host = MirrorHost::parked(&mut parked_state, &mut parked_engine);
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, failure());
    }
    assert_eq!(
        parked_state.toasts.len(),
        0,
        "창이 없는 engine 에는 toast 를 쌓지 않는다"
    );

    let (mut win_state, mut win_engine_session) = crate::state::tests::test_state();
    let mut win_engine = win_engine_session.borrow_mut();
    {
        let mut host = MirrorHost::windowed(&mut win_state, &mut win_engine);
        apply_mirror_events(&mut sess, &mut host, &mut plugin_manager, failure());
    }
    assert_eq!(
        win_state.toasts.len(),
        1,
        "창이 있으면 같은 이벤트가 toast 를 낸다 — 게이트가 창 유무로만 갈린다"
    );
}

/// resize 전후의 출력 순서를 보존하며 버퍼를 비워야 한다.
#[test]
fn outbox_drain_keeps_output_resize_arrival_order() {
    let buf = MirrorOutbox::new(tasty_remote::connection::channel().0.epoch());
    for event in [
        MirrorEvent::Data(1, b"a".to_vec()),
        MirrorEvent::Resize(1, 10, 5),
        MirrorEvent::Data(1, b"b".to_vec()),
    ] {
        assert!(buf.push(event));
    }
    let drained = buf.drain();
    assert!(matches!(drained[0], MirrorEvent::Data(1, ref b) if b == b"a"));
    assert!(matches!(drained[1], MirrorEvent::Resize(1, 10, 5)));
    assert!(matches!(drained[2], MirrorEvent::Data(1, ref b) if b == b"b"));
    assert_eq!(drained.len(), 3);
    assert!(buf.drain().is_empty(), "꺼낸 뒤 버퍼는 비어 있다");
}

#[test]
fn an_agent_forward_failure_does_not_toast() {
    let mut sess = test_session(9_000, HashMap::new());
    sess.state.agent_requests.note_structural(true, 5);
    sess.state.agent_requests.note_structural(false, 6);
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::StructuralFailed(5, Some("nope".to_string()))],
        );
    }
    assert_eq!(state.toasts.len(), 0, "에이전트 op 의 실패는 로그로 끝난다");
    assert!(
        sess.state.agent_requests.structural.is_empty(),
        "회신이 오면 표시를 지운다"
    );

    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![MirrorEvent::StructuralFailed(6, Some("nope".to_string()))],
        );
    }
    assert_eq!(
        state.toasts.len(),
        1,
        "표시 없는 op 의 실패는 toast 를 낸다"
    );
}

#[test]
fn an_agent_markdown_reload_truncation_does_not_toast() {
    let mut sess = test_session(9_000, HashMap::from([(30, 300)]));
    sess.state.markdown_locals.insert(300);
    sess.state.agent_requests.note_markdown(true, 11);
    sess.state.agent_requests.note_markdown(false, 12);
    let mut plugin_manager: Option<crate::plugin::PluginManager> = None;
    let (mut state, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    let truncated = |request_id| MirrorEvent::MarkdownContentResult {
        request_id,
        surface_id: 30,
        ok: true,
        file: Some("/r/a.md".to_string()),
        source: Some("# a".to_string()),
        truncated: true,
        reason: None,
    };
    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![truncated(11)],
        );
    }
    assert_eq!(
        state.toasts.len(),
        0,
        "에이전트 요청의 잘림은 toast 를 안 낸다"
    );
    assert!(
        !sess.state.agent_requests.take_markdown(11),
        "회신이 오면 표시를 지운다"
    );

    {
        let mut host = MirrorHost::windowed(&mut state, &mut engine);
        apply_mirror_events(
            &mut sess,
            &mut host,
            &mut plugin_manager,
            vec![truncated(12)],
        );
    }
    assert_eq!(
        state.toasts.len(),
        1,
        "plugin 자신의 요청은 종전대로 toast 를 낸다"
    );
}

#[test]
fn parked_mirror_deltas_reclaim_retired_navigation_without_a_redraw() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    crate::app::attach_client::tests::supply_ids(&engine);
    let workspace = engine.workspace_at(0).expect("workspace index is valid").id;
    let mut session = crate::app::attach_client::tests::test_session(workspace, HashMap::new());
    for generation in 1..=12 {
        let previous_pane = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .pane_layout()
            .first_pane()
            .unwrap()
            .id;
        state
            .tab_bar_scroll
            .insert(previous_pane, Default::default());
        let tree = serde_json::json!({
            "focused_pane": generation + 10,
            "panes": [{"id": generation + 10, "tabs": [{
                "id": generation + 100, "active": true, "focused_surface": 2,
                "layout": {"type": "Split", "direction": "horizontal", "ratio": 0.5,
                    "focus_second": false,
                    "first": {"type": "Leaf", "id": 1, "kind": "terminal"},
                    "second": {"type": "Leaf", "id": 2, "kind": "terminal"}}
            }]}]
        });
        let surfaces = [1, 2]
            .map(|id| {
                serde_json::json!({
                    "remote_id": id, "role": "terminal", "cols": 80, "rows": 24
                })
            })
            .to_vec();
        apply_one_mirror_event(
            &mut session,
            &mut MirrorHost::parked(&mut state, &mut engine),
            &mut None,
            MirrorEvent::StructuralDelta {
                workspace_id: 7,
                tree,
                surfaces,
            },
        );
        assert_eq!(session.state.structure_ids.panes.len(), 1);
        assert_eq!(session.state.structure_ids.remote_tabs.len(), 1);
        assert_eq!(
            state.navigation.split_hints.len(),
            1,
            "old split keys must not accumulate while no View redraw occurs"
        );
        assert!(!state.tab_bar_scroll.contains_key(&previous_pane));
    }
}

/// 구조 delta 의 descriptor 크기는 출력과 같은 순서의 값이 아니다. 살아남은 terminal mirror 의
/// grid 는 echo 로만 바뀌어야 하므로, delta 가 다른 크기를 실어도 그대로 둔다.
#[test]
fn a_structural_delta_keeps_the_surviving_terminal_mirror_size() {
    let mut navigation = crate::state::navigation::NavigationState::default();
    let mut structure_ids = MirrorStructureIds::default();
    let (_, mut engine_session) = crate::state::tests::test_state();
    let mut engine = engine_session.borrow_mut();
    supply_ids(&engine);
    let ids = test_ids();
    let (tx, _frames) = tasty_remote::connection::channel();
    let frame_tx: SharedFrameSender = tx;

    let mut m1 = merge_survivor_mapping(
        &HashMap::new(),
        &[serde_json::json!({ "remote_id": 10, "role": "terminal", "cols": 100, "rows": 30 })],
        &ids,
        &frame_tx,
        &mut engine,
    )
    .expect("fixture construction");
    let local = m1.remote_to_local[&10];
    let ws_id = 999;
    let mut ws = build_mirror_workspace(
        &mut structure_ids,
        &mut navigation,
        ws_id,
        "mirror",
        &single_leaf_tree(10),
        &ids,
        &m1.remote_to_local,
        &m1.terminals,
        &m1.mesh,
        &m1.explorer,
        &mut m1.markdown,
    )
    .expect("fixture construction");
    ws.mirror = true;
    engine.push_mirror_workspace(ws);

    let mut sess = test_session(ws_id, m1.remote_to_local.clone());
    let removed = apply_mirror_structural_delta(
        &mut navigation,
        &mut sess,
        &mut engine,
        7,
        &single_leaf_tree(10),
        &[serde_json::json!({ "remote_id": 10, "role": "terminal", "cols": 120, "rows": 40 })],
        None,
    )
    .expect("mirror delta");
    assert!(removed.markdown.is_empty() && removed.html.is_empty());
    assert_eq!(sess.state.remote_to_local[&10], local);
    let t = engine
        .runtime
        .terminals
        .get_mut(local)
        .expect("terminal mirror");
    assert_eq!((t.cols(), t.rows()), (100, 30));
}

#[test]
fn html_retired_ids_stay_separate_from_markdown_content_routes() {
    let mut session = test_session(1, HashMap::from([(30, 300), (31, 310)]));
    session.state.markdown_locals = HashSet::from([300, 301]);
    session.state.html_locals = HashSet::from([310, 311]);
    let removed = session
        .state
        .replace_document_ids(HashSet::from([300, 302]), HashSet::from([310, 312]));
    assert_eq!(removed.markdown, vec![301]);
    assert_eq!(removed.html, vec![311]);
    assert_eq!(session.state.markdown_locals, HashSet::from([300, 302]));
    assert_eq!(session.state.html_locals, HashSet::from([310, 312]));
    assert_eq!(markdown_mirror_local(&session, 30), Some(300));
    assert_eq!(markdown_mirror_local(&session, 31), None);
}
