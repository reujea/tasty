mod navigation_compat;
use super::*;
#[cfg(feature = "gui")]
use crate::model::SplitDirection;
use crate::runtime::engine_access::EngineMut;

// 다른 상태·팝업 시험도 같은 engine/RequestContext 구성을 사용한다.
pub(crate) fn test_state() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        ));
    test_state_with_memory(memory)
}

/// 호출자가 보관한 메모리 mock으로 종료 후 purge 호출을 검사할 수 있게 한다.
pub(crate) fn test_state_with_memory(
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    state_fixture(memory, false, None)
}

/// A display-only mirror fixture; transport/materialization tests install a real Remote session.
pub(crate) fn test_mirror_state() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    state_fixture(
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        )),
        true,
        None,
    )
}

pub(crate) fn test_state_from_model(
    model: tasty_core::JournalModel,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    state_fixture(
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        )),
        false,
        Some(model),
    )
}

/// Complete canonical fixture facts, not an application command or an external effect.
pub(crate) fn test_model(events: Vec<tasty_core::DomainEvent>) -> tasty_core::JournalModel {
    let mut model = tasty_core::JournalModel::default();
    tasty_core::evolve(
        &mut model,
        &tasty_core::DomainBatch {
            batch_id: 1,
            events: events
                .into_iter()
                .enumerate()
                .map(|(index, event)| tasty_core::RecordedEvent {
                    revision: index as u64 + 1,
                    event,
                })
                .collect(),
        },
    )
    .expect("canonical presentation fixture");
    model
}

fn state_fixture(
    memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>>,
    mirror: bool,
    model: Option<tasty_core::JournalModel>,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
    let mut engine = crate::runtime::engine_session::EngineSession::new_with_ids_and_settings(
        crate::runtime::engine_session::EngineSessionSpec {
            cols: 80,
            rows: 24,
            waker,
            shared_ids: None,
            layout_slot: None,
            memory: tasty_memory::writer::MemoryAccess::inline(memory),
            runner_registry: std::sync::Arc::new(tasty_task_runtime::RunnerRegistry::new()),
        },
        crate::settings::Settings::default(),
    )
    .unwrap();
    // Read/presentation fixtures seed canonical facts without executing a shell or a factory.
    // Tests of actual creation, input delivery or retirement use their journal/PTY harness instead.
    static NEXT_FIXTURE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let id = NEXT_FIXTURE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let events = vec![
        tasty_core::DomainEvent::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        tasty_core::DomainEvent::WorkspaceCreated {
            id,
            name: "Workspace 1".into(),
            category: 0,
            index: 0,
            pane: id,
        },
        tasty_core::DomainEvent::TabCreated {
            id,
            pane: id,
            index: 0,
            name: "Terminal".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        },
    ];
    let model = model.unwrap_or_else(|| test_model(events));
    if mirror {
        let mut workspace =
            crate::model::Workspace::new_with_terminal_marker(id, "Workspace 1".into(), id, id, id);
        workspace.mirror = true;
        engine.core_state.push_mirror_workspace(workspace);
    } else {
        tasty_core::projection::bootstrap::initialize(&mut engine.core_state, &model).unwrap();
    }
    for (&id, surface) in &model.surfaces {
        match surface.kind.as_str() {
            "terminal" => {
                engine
                    .runtime
                    .surfaces
                    .insert(id, Box::new(crate::model::TerminalSurface { id }));
                engine.runtime.terminals.insert(
                    id,
                    tasty_terminal::Terminal::new_detached(80, 24),
                    None,
                );
            }
            "empty" => {
                engine
                    .runtime
                    .surfaces
                    .insert(id, Box::new(crate::model::EmptySurface::new(id)));
            }
            _ => {} // Kind-specific test values are installed explicitly by their fixture owner.
        }
    }
    // 플러그인 프로세스 없이 WebView kind와 오버레이 등록을 구성한다.
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "markdown",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "webview",
        // 파일 경로에서 cwd를 구하는 실제 매니페스트 조건을 포함한다.
        "preset_fields": [{
            "id": "file",
            "label_key": "preset.field.file",
            "param_key": "file",
            "input_type": "file_path",
            "required": true,
            "derive_cwd": true,
        }],
    }))
    .expect("test SurfaceKindDecl");
    // 전역 WebView kind 표를 사용하는 다른 시험과 같은 락으로 등록을 보호한다.
    {
        let _g = crate::runtime::surface_registry::webview_kind::WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        crate::runtime::surface_registry::webview_kind::register_webview_kind(
            "com.tasty.markdown",
            &decl,
            &[],
        );
    }
    // 헤드리스 시험은 Markdown 생성 경로를 사용하지 않아 추가 등록을 생략한다.
    #[cfg(feature = "gui")]
    {
        let (host_cmd_tx, host_cmd_rx) = std::sync::mpsc::channel();
        crate::plugin_bridge::remote_kind::register_remote_kind(
            &engine.runtime.surface_registry,
            "com.tasty.markdown",
            &decl,
            host_cmd_tx,
        );
        engine.test_host_commands = Some(host_cmd_rx);
    }
    let preset_store = std::sync::Arc::new(std::sync::Mutex::new(
        tasty_presets::PresetStore::load_default(),
    ));
    let state = RequestContext::new(&engine.as_ref().read(), preset_store);
    #[cfg(not(feature = "gui"))]
    let state = {
        let mut state = state;
        state.engine_id = Some(engine.id);
        state
    };
    (state, engine)
}

fn collect_surface_ids(state: &mut RequestContext, engine: &mut EngineMut<'_>) -> Vec<u32> {
    let read = engine.read();
    let ws = state.active_workspace(&read);
    let ws_ids: std::collections::HashSet<u32> = ws.all_surface_ids().into_iter().collect();
    engine
        .runtime
        .terminals
        .iter()
        .filter_map(|(sid, _)| ws_ids.contains(&sid).then_some(sid))
        .collect()
}

#[test]
fn find_terminal_by_id_exists() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_ids = collect_surface_ids(&mut state, &mut engine);
    assert!(!surface_ids.is_empty());
    let first_id = surface_ids[0];
    assert!(engine.find_terminal_by_id(first_id).is_some());
}

#[test]
fn find_terminal_by_id_nonexistent() {
    let (_state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    assert!(engine.find_terminal_by_id(9999).is_none());
}

pub(crate) fn add_mirror_test_workspace(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
) -> u32 {
    let id = 900_001;
    let mut workspace =
        crate::model::Workspace::new_with_terminal_marker(id, "Mirror".into(), id, id, id);
    workspace.mirror = true;
    engine.core.push_mirror_workspace(workspace);
    engine
        .runtime
        .surfaces
        .insert(id, Box::new(crate::model::TerminalSurface { id }));
    engine
        .runtime
        .terminals
        .insert(id, tasty_terminal::Terminal::new_detached(80, 24), None);
    state.set_active_workspace_index(&engine.read(), engine.workspaces().len() - 1);
    id
}

#[test]
fn local_attention_raise_is_suppressed_on_mirror_surface() {
    use crate::core::AttentionKind;

    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let local_sid = *engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .unwrap()
        .all_surface_ids()
        .first()
        .expect("기본 workspace 에 surface 하나");
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror surface의 attention은 로컬 요청으로 만들지 않는다"
    );

    engine.raise_attention(local_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(local_sid),
        Some(AttentionKind::Completion),
        "로컬 surface에는 attention을 기록해야 한다"
    );
}

#[test]
fn osc133_command_completed_raises_attention_only_off_mirror() {
    use crate::core::AttentionKind;
    use tasty_terminal::TerminalEventKind;

    const OSC133_D: &[u8] = b"\x1b]133;D;0\x07";

    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let local_sid = *engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .unwrap()
        .all_surface_ids()
        .first()
        .expect("기본 workspace 에 surface 하나");
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    for sid in [local_sid, mirror_sid] {
        engine
            .find_terminal_by_id_mut(sid)
            .expect("terminal surface")
            .feed_bytes(OSC133_D);
    }

    // detached terminal에 넣은 OSC를 한 번에 drain한다. 실제 PTY 실행은 이 fixture 범위가 아니다.
    let boundaries: Vec<u32> = [local_sid, mirror_sid]
        .into_iter()
        .filter(|sid| {
            engine
                .find_terminal_by_id_mut(*sid)
                .expect("terminal surface")
                .take_events()
                .iter()
                .any(|e| {
                    matches!(&e.kind, TerminalEventKind::PromptBoundary { phase, .. } if *phase == 'D')
                })
        })
        .collect();
    assert!(
        boundaries.contains(&mirror_sid),
        "mirror 터미널도 OSC 133 D 를 파싱한다(억제 지점은 파서가 아니라 producer): {boundaries:?}"
    );
    assert!(
        boundaries.contains(&local_sid),
        "로컬 터미널의 OSC 133 D 파싱은 그대로여야 한다: {boundaries:?}"
    );

    for sid in [local_sid, mirror_sid] {
        engine.raise_attention(sid, AttentionKind::Completion);
    }
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror surface 에 도착한 OSC 133 D 는 attention 을 만들지 않는다"
    );
    assert_eq!(
        engine.attention_kind(local_sid),
        Some(AttentionKind::Completion),
        "같은 사건이 mirror 아닌 surface 에서는 그대로 attention 을 만든다"
    );
}

// mirror의 로컬 attention 억제가 알림 패널 항목까지 제거하지는 않아야 한다.
#[test]
fn mirror_surface_notification_item_survives_the_attention_gate() {
    use crate::core::AttentionKind;

    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);
    let mirror_ws_id = engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .expect("mirror workspace")
        .id;

    let created = engine.live.notifications.add(
        mirror_ws_id,
        mirror_sid,
        "bell".to_string(),
        "ring".to_string(),
    );
    assert!(
        created.is_some(),
        "mirror surface 의 벨/알림도 패널 아이템은 그대로 만들어야 한다"
    );
    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "알림은 남기되 attention 레코드만 억제한다"
    );
}

#[test]
fn surface_completion_on_mirror_surface_is_suppressed() {
    use crate::core::AttentionKind;

    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    engine.raise_attention(mirror_sid, AttentionKind::NeedsInput);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "mirror 대상 surface.completion 은 억제된다(kind 무관)"
    );
}

#[test]
fn server_push_apply_is_not_blocked_by_the_mirror_gate() {
    use crate::core::AttentionKind;

    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let mirror_sid = add_mirror_test_workspace(&mut state, &mut engine);

    engine.set_mirror_surface_attention(mirror_sid, Some(AttentionKind::NeedsInput));
    assert_eq!(
        engine.attention_kind(mirror_sid),
        Some(AttentionKind::NeedsInput),
        "서버 push 는 억제 대상이 아니다 — 미러의 유일한 attention 소스"
    );

    engine.raise_attention(mirror_sid, AttentionKind::Completion);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        Some(AttentionKind::NeedsInput),
        "로컬 요청이 서버에서 받은 attention을 덮어쓰면 안 된다"
    );

    engine.set_mirror_surface_attention(mirror_sid, None);
    assert_eq!(
        engine.attention_kind(mirror_sid),
        None,
        "서버가 내려준 해제도 그대로 적용된다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn occupancy_suppresses_completion_highlight() {
    use crate::adapters::ui::divider::regions_from_state;
    use crate::core::AttentionKind;
    use crate::model::{PhysicalPx, PhysicalRect};

    let (state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let sids = engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .unwrap()
        .all_surface_ids();
    let sid = *sids.first().expect("기본 workspace 에 surface 하나");

    let term_rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };

    engine.raise_attention(sid, AttentionKind::Completion);
    let regions = regions_from_state(&state, &engine.read(), term_rect, 1.0);
    assert!(
        regions
            .iter()
            .any(|r| r.kind == Some(AttentionKind::Completion)),
        "점유 없이 attention 만이면 완료 테두리가 그려져야 한다"
    );

    engine
        .live
        .occupancy
        .acquire_soft(sid, /* parent */ 9999.into(), Some("agent".into()))
        .expect("soft 점유 획득");
    let regions = regions_from_state(&state, &engine.read(), term_rect, 1.0);
    assert!(
        regions.iter().all(|r| r.kind.is_none()),
        "점유 중이면 완료 테두리를 억제해야 한다(점유 > 완료)"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn needs_input_not_suppressed_by_occupancy() {
    use crate::adapters::ui::divider::regions_from_state;
    use crate::core::AttentionKind;
    use crate::model::{PhysicalPx, PhysicalRect};

    let (state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let sids = engine
        .workspace_at(state.active_workspace_index(&engine.read()))
        .unwrap()
        .all_surface_ids();
    let sid = *sids.first().expect("기본 workspace 에 surface 하나");

    let term_rect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(600.0),
    };

    engine
        .live
        .occupancy
        .acquire_soft(sid, /* parent */ 9999.into(), Some("agent".into()))
        .expect("soft 점유 획득");
    engine.raise_attention(sid, AttentionKind::NeedsInput);
    let regions = regions_from_state(&state, &engine.read(), term_rect, 1.0);
    assert!(
        regions
            .iter()
            .any(|r| r.kind == Some(AttentionKind::NeedsInput)),
        "점유 중이어도 NeedsInput 테두리는 억제되지 않아야 한다(NeedsInput > 점유)"
    );
}

#[test]
fn resolve_inherit_cwd_from_unknown_surface_is_none() {
    let (state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    let _ = &engine;
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine.read(), 99999),
        None
    );
}

/// Static descriptors and a runtime Explorer value; no factory or filesystem request.
pub(crate) fn explorer_fixture(
    mirror: bool,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
    std::path::PathBuf,
) {
    use tasty_core::DomainEvent as E;
    let id = 700_001;
    let model = test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id,
            name: "Explorer".into(),
            category: 0,
            index: 0,
            pane: id,
        },
        E::TabCreated {
            id,
            pane: id,
            index: 0,
            name: "Explorer".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "explorer".into(),
                data: None,
            },
        },
    ]);
    let (state, mut session) = test_state_from_model(model);
    if mirror {
        session.core_state = crate::core::CoreState::new_base();
        let pane = crate::model::Pane::new_with_surface(
            id,
            id,
            "Explorer".into(),
            crate::model::SurfaceDescriptor::new(id, "explorer"),
        );
        let mut workspace = crate::model::Workspace::new_with_pane(id, "Explorer".into(), pane);
        workspace.mirror = true;
        session.core_state.push_mirror_workspace(workspace);
    }
    let root = crate::test_support::abs_path("remote/proj");
    session.runtime.surfaces.insert(
        id,
        Box::new(crate::model::ExplorerPanel::new(id, root.clone())),
    );
    (state, session, id, root)
}

#[test]
fn local_explorer_cwd_is_local_and_inherited() {
    let (state, mut engine_session, sid, root) = explorer_fixture(false);
    let engine = engine_session.borrow_mut();
    assert_eq!(
        engine.surface_cwd(sid),
        Some(crate::core::state::SurfaceCwd::Local(root.clone()))
    );
    assert_eq!(
        state.resolve_inherit_cwd(&engine.read()),
        Some(root.clone())
    );
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine.read(), sid),
        Some(root)
    );
}

/// 포커스가 다른 surface 에 있어도 owner 지정 kind 탭은 owner 의 cwd 를 받는다.
#[cfg(feature = "gui")]
#[test]
fn owner_kind_tab_takes_the_owner_cwd_not_the_focused_one() {
    use tasty_core::DomainEvent as E;
    let (focused, owner) = (700_101, 700_102);
    let tab = |id: u32, index: usize| E::TabCreated {
        id,
        pane: focused,
        index,
        name: "Explorer".into(),
        surface: tasty_core::SurfaceSpec {
            id,
            kind: "explorer".into(),
            data: None,
        },
    };
    let model = test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: focused,
            name: "Explorer".into(),
            category: 0,
            index: 0,
            pane: focused,
        },
        tab(focused, 0),
        tab(owner, 1),
    ]);
    let (mut state, mut session) = test_state_from_model(model);
    let (focused_root, owner_root) = (
        crate::test_support::abs_path("focused/root"),
        crate::test_support::abs_path("owner/root"),
    );
    session.runtime.surfaces.insert(
        focused,
        Box::new(crate::model::ExplorerPanel::new(
            focused,
            focused_root.clone(),
        )),
    );
    session.runtime.surfaces.insert(
        owner,
        Box::new(crate::model::ExplorerPanel::new(owner, owner_root.clone())),
    );
    let engine = session.borrow_mut();
    assert_eq!(state.focused_surface_id(&engine.read()), Some(focused));

    state
        .add_kind_tab_by_owner(&engine.read(), owner, "explorer", &serde_json::json!({}))
        .expect("owner 가 있으면 탭 추가 intent 를 낸다");
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    match &intents[0].body {
        crate::intent::Intent::Domain(crate::app::command::DomainIntent::CreateTab {
            pane_id,
            cwd,
            ..
        }) => {
            assert_eq!(*pane_id, focused);
            assert_eq!(cwd.as_deref(), Some(owner_root.as_path()));
        }
        other => panic!("CreateTab 이 아니다: {other:?}"),
    }
}

#[test]
fn mirror_explorer_cwd_is_remote_and_not_inherited_locally() {
    let (state, mut engine_session, sid, root) = explorer_fixture(true);
    let engine = engine_session.borrow_mut();

    assert_eq!(
        engine.surface_cwd(sid),
        Some(crate::core::state::SurfaceCwd::Remote(
            crate::core::state::RemoteCwd::new(root.to_string_lossy().into_owned())
        )),
        "mirror 워크스페이스의 cwd 는 원격 출처로 분류된다"
    );
    assert_eq!(engine.as_ref().local_surface_cwd(sid), None);
    assert_eq!(state.resolve_inherit_cwd(&engine.read()), None);
    assert_eq!(
        state.resolve_inherit_cwd_from_surface(&engine.read(), sid),
        None
    );
}

#[test]
fn popup_context_splits_cwd_keys_by_gate_and_provenance() {
    let (state, mut engine_session, sid, root) = explorer_fixture(false);
    let engine = engine_session.borrow_mut();
    let root_s = root.to_string_lossy().into_owned();

    let local = state.popup_surface_context(&engine.read(), Some(sid));
    assert_eq!(local["cwd"], serde_json::json!(root_s));
    assert_eq!(local["observed_cwd"], serde_json::json!(root_s));
    assert_eq!(local["origin_surface_id"], serde_json::json!(sid));
    assert!(local.get("remote_cwd").is_none());
    assert!(local.get("mirror").is_none());

    engine.runtime.settings.general.inherit_cwd = false;
    let gated = state.popup_surface_context(&engine.read(), Some(sid));
    assert!(gated["cwd"].is_null());
    assert_eq!(gated["observed_cwd"], serde_json::json!(root_s));

    engine.runtime.settings.general.inherit_cwd = true;
    let (mirror_state, mut mirror_session, sid, _) = explorer_fixture(true);
    let engine = mirror_session.borrow_mut();
    let mirror = mirror_state.popup_surface_context(&engine.read(), Some(sid));
    assert!(
        mirror["cwd"].is_null(),
        "원격 경로는 `cwd` 키로 새지 않는다"
    );
    assert!(mirror.get("observed_cwd").is_none());
    assert_eq!(mirror["remote_cwd"], serde_json::json!(root_s));
    assert_eq!(mirror["mirror"], serde_json::json!(true));
    assert_eq!(mirror["local_surface_id"], serde_json::json!(sid));
}

#[test]
fn surface_display_path_returns_workspace_and_tab_names() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_ids = collect_surface_ids(&mut state, &mut engine);
    let sid = surface_ids[0];
    let path = engine
        .surface_display_path(sid, &state.navigation)
        .expect("path for existing surface");
    let read = engine.read();
    let ws = state.active_workspace(&read);
    assert_eq!(path.workspace_name, ws.name);
    assert!(path.tab_name.is_some());
}

#[test]
fn surface_display_path_unknown_surface_is_none() {
    let (state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    assert!(
        engine
            .surface_display_path(99999, &state.navigation)
            .is_none()
    );
}

// 포커스가 없는 pane에서 발생한 탭 바 입력으로 해당 pane이 선택되는지 검사한다.

#[cfg(feature = "gui")]
pub(crate) fn two_pane_fixture() -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::DomainEvent as E;
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "Workspace".into(),
            category: 0,
            index: 0,
            pane: 10,
        },
        E::PaneSplit {
            target: 10,
            pane: 20,
            split: tasty_core::SplitSpec {
                direction: SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ];
    for (id, pane, index) in [(101, 10, 0), (102, 10, 1), (201, 20, 0)] {
        events.push(E::TabCreated {
            id,
            pane,
            index,
            name: "Terminal".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        });
    }
    let (mut state, session) = test_state_from_model(test_model(events));
    state
        .navigation
        .select_pane(session.core_state.workspace_at(0).unwrap(), 20);
    (state, session)
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn switch_tab_on_other_pane_moves_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine_session) = two_pane_fixture();
    let engine = engine_session.borrow_mut();
    let (pane_a, pane_b) = (10, 20);
    assert_eq!(state.focused_pane_id(&engine), pane_b);

    apply_tab_bar_actions(
        &mut state,
        &engine.read(),
        vec![TabBarAction::SwitchTab {
            pane_id: pane_a,
            tab_index: 0,
        }],
        &[],
        160.0,
        1.0,
    );

    assert_eq!(
        state.focused_pane_id(&engine),
        pane_a,
        "비-focused pane 의 탭 클릭은 그 pane 으로 focus 를 옮겨야 한다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn focus_pane_action_moves_focus_without_switching_tab() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine_session) = two_pane_fixture();
    let engine = engine_session.borrow_mut();
    let (pane_a, pane_b) = (10, 20);

    // 빈 영역 클릭이 활성 탭을 바꾸지 않는지 확인하도록 탭을 두 개 둔다.
    state
        .navigation
        .select_pane(state.active_workspace(&engine.read()), pane_a);
    state
        .navigation
        .goto_tab(engine.find_pane_by_id(pane_a).unwrap(), 1);
    let active_before = state
        .navigation
        .tab_index(engine.find_pane_by_id(pane_a).unwrap());
    assert_eq!(active_before, 1);
    state
        .navigation
        .select_pane(state.active_workspace(&engine.read()), pane_b);

    apply_tab_bar_actions(
        &mut state,
        &engine.read(),
        vec![TabBarAction::FocusPane { pane_id: pane_a }],
        &[],
        160.0,
        1.0,
    );

    assert_eq!(
        state.focused_pane_id(&engine),
        pane_a,
        "탭바 빈 영역 클릭은 그 pane 으로 focus 를 옮겨야 한다"
    );
    let active_after = state
        .navigation
        .tab_index(engine.find_pane_by_id(pane_a).unwrap());
    assert_eq!(
        active_after, active_before,
        "빈 영역 클릭은 focus 만 이동하고 active_tab 은 건드리지 않아야 한다"
    );
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn scroll_left_and_right_on_other_pane_move_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    for action_of in [
        (|pane_id| TabBarAction::ScrollLeft { pane_id }) as fn(u32) -> TabBarAction,
        (|pane_id| TabBarAction::ScrollRight { pane_id }) as fn(u32) -> TabBarAction,
    ] {
        let (mut state, mut engine_session) = two_pane_fixture();
        let engine = engine_session.borrow_mut();
        let (pane_a, pane_b) = (10, 20);
        assert_eq!(state.focused_pane_id(&engine), pane_b);

        apply_tab_bar_actions(
            &mut state,
            &engine.read(),
            vec![action_of(pane_a)],
            &[],
            160.0,
            1.0,
        );

        assert_eq!(
            state.focused_pane_id(&engine),
            pane_a,
            "스크롤 화살표 클릭도 그 pane 으로 focus 를 옮겨야 한다"
        );
    }
}

#[cfg(feature = "gui")] // gui 어댑터(divider / tab_bar / egui 좌표)를 직접 부르는 테스트
#[test]
fn context_menu_actions_do_not_move_focus() {
    use crate::adapters::ui::tab_bar::{TabBarAction, apply_tab_bar_actions};

    let (mut state, mut engine_session) = two_pane_fixture();
    let engine = engine_session.borrow_mut();
    let (pane_a, pane_b) = (10, 20);
    assert_eq!(state.focused_pane_id(&engine), pane_b);

    let actions = vec![
        TabBarAction::OpenContextMenu {
            pane_id: pane_a,
            tab_index: 0,
            pos: egui::Pos2::ZERO,
        },
        TabBarAction::OpenPaneContextMenu {
            pane_id: pane_a,
            pos: egui::Pos2::ZERO,
        },
        TabBarAction::OpenNewTabButtonContextMenu {
            pane_id: pane_a,
            pos: egui::Pos2::ZERO,
        },
    ];
    apply_tab_bar_actions(&mut state, &engine.read(), actions, &[], 160.0, 1.0);

    assert_eq!(
        state.focused_pane_id(&engine),
        pane_b,
        "우클릭 컨텍스트 메뉴는 focus 를 옮기면 안 된다(대상은 pending_native_menu 의 pane_id 로 이미 결정됨)"
    );
}

mod cleanup_forgets_only_the_closed_surface {
    const HOLDER: u32 = 7;
    #[test]
    fn a_workspace_holders_other_surfaces_keep_their_occupancy() {
        let mut reg = crate::core::attach::OccupancyRegistry::new();
        let members = vec![10, 11, 12];
        reg.acquire_workspace(100, &members, &members, HOLDER)
            .expect("워크스페이스 점유");

        assert!(reg.forget_closed_surface(10), "닫힌 자리는 지워진다");

        assert!(!reg.is_hard_occupied(10), "닫힌 surface 는 풀린다");
        assert!(reg.is_hard_occupied(11), "형제는 그대로여야 한다");
        assert!(reg.is_hard_occupied(12), "형제는 그대로여야 한다");
        assert_eq!(
            reg.workspace_holder(100),
            Some(HOLDER),
            "워크스페이스 락 자체는 유지된다"
        );
    }

    #[test]
    fn closing_an_unoccupied_surface_touches_no_lock() {
        let mut reg = crate::core::attach::OccupancyRegistry::new();
        reg.acquire(10, HOLDER).expect("점유");

        assert!(!reg.forget_closed_surface(99), "없던 자리는 지울 것이 없다");
        assert!(reg.is_hard_occupied(10), "무관한 점유는 그대로");
    }
}
