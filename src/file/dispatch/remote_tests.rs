use super::is_remote_openable;
use crate::app::command::DomainIntent;
use crate::core::identify_port::IdentifySpawner;
use crate::file::dispatch::picker_apply::tests::build_test_core;
use crate::file::dispatch::{FileDispatchOrigin, apply_identify_result};
use crate::file::format::{DetectDepth, DetectorId, FileTarget};
use crate::file::handler::{FileHandler, HandlerAction, HandlerId, HandlerOwner};

use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;

fn queued_creates(state: &crate::state::RequestContext) -> Vec<&crate::intent::DispatchedIntent> {
    state
        .pending_intents
        .iter()
        .filter(|request| {
            matches!(
                request.body,
                crate::intent::Intent::Domain(DomainIntent::CreateTab { .. })
            )
        })
        .collect()
}

fn register_kind(
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    plugin_id: &str,
    kind: &str,
) {
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": kind,
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "webview",
        "records_recent": true,
    }))
    .expect("kind decl");
    let (host_cmd_tx, _host_cmd_rx) = std::sync::mpsc::channel();
    crate::plugin_bridge::remote_kind::register_remote_kind(
        &engine.runtime.surface_registry,
        plugin_id,
        &decl,
        host_cmd_tx,
    );
}

fn install_handler(
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    plugin_id: &str,
    action: serde_json::Value,
) {
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
        plugin_id,
        &[
            serde_json::json!({"id": "open", "detector": "remote-test", "priority": 0,
            "action": action}),
        ],
    );
}

fn handler(action: HandlerAction) -> FileHandler {
    FileHandler {
        id: HandlerId::new("host/remote-test"),
        detector: DetectorId::new("remote-test"),
        priority: 0,
        owner: HandlerOwner::Host,
        action,
        display_name_i18n_key: None,
        disabled: false,
    }
}

fn open_surface(kind: &str) -> HandlerAction {
    HandlerAction::OpenSurface {
        surface_kind: kind.into(),
        param_key: "file".into(),
    }
}

/// mirror workspace의 첫 surface를 origin으로 식별 결과를 적용한다.
fn apply_on_mirror(
    origin: FileDispatchOrigin,
) -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    usize,
) {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.markdown", "markdown");
    install_handler(
        &engine,
        "com.tasty.markdown",
        serde_json::json!({"kind": "open_surface", "surface_kind": "markdown", "param_key": "file"}),
    );
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let surfaces = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()
        .len();
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/doc.md"),
        Some(DetectorId::new("remote-test")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: origin,
            ignore_size_limit: false,
            run: None,
        },
    );
    (state, engine_session, surfaces)
}

#[test]
fn a_mirror_open_queues_one_user_create_tab_without_local_effects() {
    let (state, mut engine_session, surfaces) = apply_on_mirror(FileDispatchOrigin::User);
    let engine = engine_session.borrow_mut();
    assert_eq!(queued_creates(&state).len(), 1);
    let forward = &queued_creates(&state)[0];
    let crate::intent::Intent::Domain(DomainIntent::CreateTab {
        kind: surface_kind,
        surface_params: params,
        ..
    }) = &forward.body
    else {
        panic!("expected a NewTab forward, got {:?}", forward.body);
    };
    assert_eq!(surface_kind, "markdown");
    assert_eq!(params, &serde_json::json!({"file": "/remote/doc.md"}));
    assert!(
        forward.origin.is_user(),
        "사용자 더블클릭은 원격 새 탭을 선택한다"
    );
    assert_eq!(
        engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()
            .len(),
        surfaces
    );
    assert_eq!(state.recent_files.get("markdown"), Vec::<String>::new());
    assert_eq!(state.pending_intents.len(), 1);
    assert!(state.dialogs.file_handler_picker.is_none());
    assert_eq!(state.toasts.len(), 0);
}

#[test]
fn an_agent_mirror_open_keeps_agent_origin_on_queued_creation() {
    let (state, _engine_session, _) = apply_on_mirror(FileDispatchOrigin::Agent);
    assert_eq!(queued_creates(&state).len(), 1);
    let forward = &queued_creates(&state)[0];
    assert!(!forward.origin.is_user());
    assert!(
        !forward.origin.is_user(),
        "에이전트 요청의 실패는 로그로만 남긴다"
    );
}

#[test]
fn a_mirror_open_with_only_an_ipc_handler_runs_nothing_and_toasts() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    install_handler(
        &engine,
        "com.example.ipc",
        serde_json::json!({"kind": "ipc", "method": "example.open"}),
    );
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/doc.bin"),
        Some(DetectorId::new("remote-test")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    assert!(state.pending_intents.is_empty());
    assert!(
        state.dialogs.file_handler_picker.is_none(),
        "로컬 핸들러 전체 picker를 띄우지 않는다"
    );
    assert_eq!(state.toasts.len(), 1);
}

#[test]
fn a_mirror_open_without_a_detector_opens_no_picker() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/unknown"),
        None,
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.dialogs.file_handler_picker.is_none());
    assert_eq!(state.toasts.len(), 1);
}

#[test]
fn only_open_surface_kinds_that_mirror_content_are_remote_openable() {
    let (_state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.markdown", "markdown");
    register_kind(&engine, "com.tasty.html", "html");
    // 같은 kind라도 허용 목록과 다른 plugin이 등록했으면 client가 원문을 받지 않는다.
    register_kind(&engine, "com.example.other", "probe_other_markdown");

    for action in [
        HandlerAction::System,
        HandlerAction::Ipc {
            method: "example.open".into(),
            owner_plugin_id: "example".into(),
        },
        open_surface("html"),
        open_surface("probe_other_markdown"),
        open_surface("not_registered"),
    ] {
        assert!(
            !is_remote_openable(&engine.read(), &handler(action.clone()), None),
            "{action:?} must not open on the remote",
        );
    }
    assert!(is_remote_openable(
        &engine.read(),
        &handler(open_surface("markdown")),
        None
    ));
}

#[test]
fn egui_mesh_kinds_are_remote_openable_regardless_of_plugin_id() {
    let (_state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "widget",
        "display_name_i18n_key": "surface.kind.markdown",
        "rendering": "egui-mesh",
    }))
    .expect("kind decl");
    assert!(
        crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
            &engine.runtime.surface_registry,
            "com.example.widget",
            &decl,
            tasty_plugin_manifest::HOST_API_VERSION,
        )
    );
    assert!(is_remote_openable(
        &engine.read(),
        &handler(open_surface("widget")),
        None
    ));
}

/// 1순위가 원격에 열 수 없는 Ipc이고 2순위가 markdown인 mirror 환경을 만든다.
fn mirror_with_ipc_first() -> (
    crate::app::services::AppServices,
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    u32,
) {
    let (core, _) = build_test_core();
    let (state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.markdown", "markdown");
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
        "com.example.ipc",
        &[
            serde_json::json!({"id": "open", "detector": "remote-test", "priority": 0,
            "action": {"kind": "ipc", "method": "example.open"}}),
        ],
    );
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.runtime.file_handler.as_ref(),
        "com.tasty.markdown",
        &[
            serde_json::json!({"id": "open", "detector": "remote-test", "priority": 10,
            "action": {"kind": "open_surface", "surface_kind": "markdown", "param_key": "file"}}),
        ],
    );
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    (core, state, engine_session, sid)
}

#[test]
fn a_user_open_with_an_unopenable_first_handler_shows_only_remote_candidates() {
    let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let mut engine = engine_session.borrow_mut();
    // 최근 목록에 원격에 열 수 없는 핸들러가 있어도 picker에 보이면 안 된다.
    state
        .file_handler_recent
        .record(&HandlerId::new("com.example.ipc/open"));
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/doc.md"),
        Some(DetectorId::new("remote-test")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let picker = state
        .dialogs
        .file_handler_picker
        .as_ref()
        .expect("user open shows the remote picker");
    let ids: Vec<&str> = picker
        .candidates
        .iter()
        .chain(picker.recent.iter())
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, vec!["com.tasty.markdown/open"]);
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert!(picker.default_handler.is_none());
    assert!(!picker.candidates_are_fallback);
    // 원격 경로 picker는 머리 경로 줄 앞에 원격 표지를 둔다(연결 정보가 없으면 빈 호스트 글자).
    assert!(picker.remote_host.is_some());
    assert_eq!(state.toasts.len(), 0);
}

#[test]
fn an_agent_open_with_an_unopenable_first_handler_runs_the_first_remote_one() {
    let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let mut engine = engine_session.borrow_mut();
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/doc.md"),
        Some(DetectorId::new("remote-test")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::Agent,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(
        state.dialogs.file_handler_picker.is_none(),
        "에이전트 요청은 사용자 화면에 picker를 띄우지 않는다"
    );
    assert!(state.pending_handler_ipc.is_empty());
    assert_eq!(queued_creates(&state).len(), 1);
    assert!(!queued_creates(&state)[0].origin.is_user());
}

/// 사용자 입력을 증명하지 못한 plugin 중계 요청도 사용자 클릭일 수 있어 원격 picker를 연다.
/// 확정 뒤에도 같은 출처로 실행하도록 picker에 전달된 출처를 싣는다.
#[test]
fn an_unverified_plugin_open_with_an_unopenable_first_handler_shows_the_remote_picker() {
    let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let mut engine = engine_session.borrow_mut();
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new("/remote/doc.md"),
        Some(DetectorId::new("remote-test")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::PluginUnverified,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let picker = state
        .dialogs
        .file_handler_picker
        .as_ref()
        .expect("unverified plugin open shows the remote picker");
    assert_eq!(picker.dispatch_origin, FileDispatchOrigin::PluginUnverified);
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert!(picker.default_handler.is_none());
    assert_eq!(state.toasts.len(), 0);
}

/// 원격에 열 수 없으면 사용자와 증명하지 못한 plugin 중계 요청은 toast로, 외부 IPC 요청은 로그로 알린다.
#[test]
fn an_unopenable_remote_file_toasts_unless_an_external_ipc_asked() {
    for (origin, toasts) in [
        (FileDispatchOrigin::User, 1),
        (FileDispatchOrigin::PluginUnverified, 1),
        (FileDispatchOrigin::Agent, 0),
    ] {
        let (mut core, _) = build_test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
        let mut engine = engine_session.borrow_mut();
        let sid = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        apply_identify_result(
            &mut core,
            &mut state,
            &mut engine,
            FileTarget::new("/remote/unknown"),
            None,
            Some(sid),
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: origin,
                ignore_size_limit: false,
                run: None,
            },
        );
        assert!(state.dialogs.file_handler_picker.is_none(), "{origin:?}");
        assert!(queued_creates(&state).is_empty(), "{origin:?}");
        assert_eq!(state.toasts.len(), toasts, "{origin:?}");
    }
}

#[test]
fn a_remote_picker_selection_queues_user_creation_and_rejects_local_handlers() {
    let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let mut engine = engine_session.borrow_mut();
    let target = crate::file::dispatch::DispatchTarget::File(FileTarget::new("/remote/doc.md"));

    crate::file::dispatch::apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        target.clone(),
        crate::state::FileHandlerPickerResult::Selected(HandlerId::new("com.example.ipc/open")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert!(state.pending_handler_ipc.is_empty());
    assert!(queued_creates(&state).is_empty());

    crate::file::dispatch::apply_file_picker_result(
        &mut core,
        &mut state,
        &mut engine,
        target,
        crate::state::FileHandlerPickerResult::Selected(HandlerId::new("com.tasty.markdown/open")),
        Some(sid),
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: FileDispatchOrigin::User,
            ignore_size_limit: false,
            run: None,
        },
    );
    assert_eq!(queued_creates(&state).len(), 1);
    let forward = &queued_creates(&state)[0];
    let crate::intent::Intent::Domain(DomainIntent::CreateTab {
        kind: surface_kind,
        surface_params: params,
        ..
    }) = &forward.body
    else {
        panic!("expected a NewTab forward, got {:?}", forward.body);
    };
    assert_eq!(surface_kind, "markdown");
    assert_eq!(params, &serde_json::json!({"file": "/remote/doc.md"}));
    assert!(forward.origin.is_user());
    assert_eq!(state.recent_files.get("markdown"), Vec::<String>::new());
}

#[derive(Default)]
struct RecordingSpawner(std::sync::Mutex<Vec<(DetectDepth, Option<u32>)>>);

impl IdentifySpawner for RecordingSpawner {
    fn spawn_identify(
        &self,
        _target: FileTarget,
        depth: DetectDepth,
        origin_surface_id: Option<u32>,
        _dispatch_origin: FileDispatchOrigin,
        _ignore_size_limit: bool,
        _run: Option<u64>,
    ) {
        self.0.lock().unwrap().push((depth, origin_surface_id));
    }
}

#[test]
fn a_mirror_origin_identifies_by_name_only() {
    let (mut core, _) = build_test_core();
    let (_, mut local_session) = crate::state::tests::test_state();
    let (_, mut mirror_session) = crate::state::tests::test_mirror_state();
    let spawner = std::sync::Arc::new(RecordingSpawner::default());
    let mut ids = Vec::new();
    for session in [&mut local_session, &mut mirror_session] {
        let mut engine = session.borrow_mut();
        engine.runtime.identify_worker = Some(spawner.clone());
        let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
        ids.push(sid);
        core.apply_live(
            &mut engine,
            DomainIntent::DispatchFile {
                target: FileTarget::new("/remote/doc.md"),
                depth: DetectDepth::Deep,
                origin_surface_id: Some(sid),
                dispatch_origin: FileDispatchOrigin::User,
                ignore_size_limit: false,
                run: None,
            },
        )
        .unwrap();
    }
    assert_eq!(
        *spawner.0.lock().unwrap(),
        vec![
            (DetectDepth::Deep, Some(ids[0])),
            (DetectDepth::Name, Some(ids[1]))
        ]
    );
}

#[test]
fn html_openability_is_bound_to_the_origin_connection_and_owned_plugin() {
    let (_state, mut session) = crate::state::tests::test_mirror_state();
    let engine = session.borrow_mut();
    register_kind(&engine, "com.tasty.html", "html");
    let workspace = engine.workspace_at(0).unwrap();
    let (workspace, sid) = (workspace.id, workspace.all_surface_ids()[0]);
    let html = handler(open_surface("html"));
    assert!(!is_remote_openable(&engine.read(), &html, Some(sid)));
    engine.remote.html_entry_hosts.insert(workspace);
    assert!(is_remote_openable(&engine.read(), &html, Some(sid)));
    assert!(!is_remote_openable(&engine.read(), &html, None));
    assert!(!is_remote_openable(&engine.read(), &html, Some(u32::MAX)));
    engine.remote.html_entry_hosts.remove(&workspace);
    assert!(!is_remote_openable(&engine.read(), &html, Some(sid)));
}

/// 터미널 링크 Ctrl+클릭 입구가 만든 요청을 식별 요청과 식별 결과 적용까지 그대로 흘려 보낸다.
fn click_terminal_link(
    core: &mut crate::app::services::AppServices,
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    path: &str,
    sid: u32,
    is_mirror: bool,
    detector: Option<DetectorId>,
) -> Vec<(DetectDepth, Option<u32>)> {
    let request = super::terminal_link_file_intent(FileTarget::new(path), sid, is_mirror);
    assert!(request.origin.is_user(), "Ctrl+클릭은 사용자 요청이다");
    let crate::intent::Intent::Domain(intent) = request.body else {
        panic!("expected a domain intent");
    };
    let DomainIntent::DispatchFile {
        target,
        origin_surface_id,
        dispatch_origin,
        ..
    } = intent.clone()
    else {
        panic!("expected DispatchFile, got {intent:?}");
    };
    assert_eq!(dispatch_origin, FileDispatchOrigin::User);
    let spawner = std::sync::Arc::new(RecordingSpawner::default());
    engine.runtime.identify_worker = Some(spawner.clone());
    core.apply_live(engine, intent).unwrap();
    apply_identify_result(
        core,
        state,
        engine,
        target,
        detector,
        origin_surface_id,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin,
            ignore_size_limit: false,
            run: None,
        },
    );
    spawner.0.lock().unwrap().clone()
}

#[test]
fn a_mirror_terminal_link_click_opens_a_remote_tab_without_reading_the_client_file() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.markdown", "markdown");
    install_handler(
        &engine,
        "com.tasty.markdown",
        serde_json::json!({"kind": "open_surface", "surface_kind": "markdown", "param_key": "file"}),
    );
    // 같은 경로의 파일이 client에도 있어도 내용으로 판정하지 않는다(이름만 본다).
    let local = tempfile::NamedTempFile::new().expect("client file");
    let path = local.path().to_string_lossy().into_owned();
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    let spawned = click_terminal_link(
        &mut core,
        &mut state,
        &mut engine,
        &path,
        sid,
        true,
        Some(DetectorId::new("remote-test")),
    );
    assert_eq!(spawned, vec![(DetectDepth::Name, Some(sid))]);
    assert!(state.dialogs.file_handler_picker.is_none());
    assert_eq!(queued_creates(&state).len(), 1);
    let forward = &queued_creates(&state)[0];
    let crate::intent::Intent::Domain(DomainIntent::CreateTab {
        kind,
        surface_params,
        ..
    }) = &forward.body
    else {
        panic!("expected a NewTab forward");
    };
    assert_eq!(kind, "markdown");
    assert_eq!(surface_params, &serde_json::json!({ "file": path }));
    assert!(
        forward.origin.is_user(),
        "사용자 클릭은 원격 새 탭을 선택한다"
    );
    assert_eq!(state.recent_files.get("markdown"), Vec::<String>::new());
}

#[test]
fn a_mirror_terminal_link_click_opens_negotiated_html_on_the_remote() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.html", "html");
    install_handler(
        &engine,
        "com.tasty.html",
        serde_json::json!({"kind": "open_surface", "surface_kind": "html", "param_key": "file"}),
    );
    let workspace = engine.workspace_at(0).unwrap();
    let (workspace, sid) = (workspace.id, workspace.all_surface_ids()[0]);
    engine.remote.html_entry_hosts.insert(workspace);
    click_terminal_link(
        &mut core,
        &mut state,
        &mut engine,
        "/remote/page.html",
        sid,
        true,
        Some(DetectorId::new("remote-test")),
    );
    assert_eq!(queued_creates(&state).len(), 1);
    assert!(state.dialogs.file_handler_picker.is_none());
}

/// 원격에 열 수 있는 핸들러가 없으면 로컬 핸들러를 실행하지 않고 설정 등록을 권하는 빈 picker도 띄우지 않는다.
#[test]
fn a_mirror_terminal_link_click_without_a_remote_handler_runs_nothing_locally() {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let mut engine = engine_session.borrow_mut();
    install_handler(
        &engine,
        "com.example.ipc",
        serde_json::json!({"kind": "ipc", "method": "example.open"}),
    );
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    click_terminal_link(
        &mut core,
        &mut state,
        &mut engine,
        "/remote/data.bin",
        sid,
        true,
        Some(DetectorId::new("remote-test")),
    );
    assert!(state.pending_handler_ipc.is_empty());
    assert!(queued_creates(&state).is_empty());
    assert!(state.dialogs.file_handler_picker.is_none());
    assert_eq!(state.toasts.len(), 1);
}

#[test]
fn a_local_terminal_link_click_keeps_the_local_dispatch() {
    let request = super::terminal_link_file_intent(FileTarget::new("/tmp/a.md"), 7, false);
    assert!(request.origin.is_user());
    assert!(matches!(
        request.body,
        crate::intent::Intent::Domain(DomainIntent::DispatchFile {
            depth: DetectDepth::Deep,
            origin_surface_id: None,
            dispatch_origin: FileDispatchOrigin::User,
            ..
        })
    ));
}

/// 우클릭 "연결 동작"은 자동 실행 없이 원격에 열 수 있는 핸들러만 담은 picker를 연다.
#[test]
fn the_remote_link_menu_lists_only_remote_openable_handlers_without_running_one() {
    let (_core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let engine = engine_session.borrow_mut();
    state
        .file_handler_recent
        .record(&HandlerId::new("com.example.ipc/open"));
    super::open_remote_link_picker(
        &mut state,
        &engine.read(),
        FileTarget::new("/remote/doc.md"),
        sid,
    );
    let picker = state
        .dialogs
        .file_handler_picker
        .as_ref()
        .expect("the remote link menu opens a picker");
    let ids: Vec<&str> = picker
        .candidates
        .iter()
        .chain(picker.recent.iter())
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, vec!["com.tasty.markdown/open"]);
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert_eq!(picker.dispatch_origin, FileDispatchOrigin::User);
    assert!(picker.default_handler.is_none());
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let pending = state.take_pending_intents();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].origin.is_user());
    assert!(matches!(
        &pending[0].body,
        crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup { id, .. })
            if *id == crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID
    ));
    assert_eq!(state.toasts.len(), 0);
}

#[test]
fn the_remote_link_menu_without_a_remote_handler_shows_no_empty_picker() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    super::open_remote_link_picker(
        &mut state,
        &engine.read(),
        FileTarget::new("/remote/data.bin"),
        sid,
    );
    assert!(state.dialogs.file_handler_picker.is_none());
    assert!(state.pending_intents.is_empty());
    assert_eq!(state.toasts.len(), 1);
}
