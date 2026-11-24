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
            result_place: Default::default(),
        },
    );
    (state, engine_session, surfaces)
}

#[test]
fn a_mirror_open_queues_one_user_create_tab_without_local_effects() {
    let (mut state, mut engine_session, surfaces) = apply_on_mirror(FileDispatchOrigin::User);
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
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
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
fn a_mirror_open_with_only_an_ipc_handler_runs_nothing_and_asks_to_download() {
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
            result_place: Default::default(),
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    // 대화상자를 여는 요청 하나만 쌓인다.
    assert_eq!(state.pending_intents.len(), 1);
    assert!(
        crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none(),
        "로컬 핸들러 전체 picker를 띄우지 않는다"
    );
    assert_asked_to_download(&state, "/remote/doc.bin");
}

/// 사용자 요청은 토스트 대신 "다운로드 후 실행" 대화상자로 묻는다. 누르기 전에는 아무것도 받지 않는다.
fn assert_asked_to_download(state: &crate::state::RequestContext, path: &str) {
    assert_eq!(state.toasts.len(), 0, "the dialog replaces the toast");
    let ask = state
        .dialogs
        .download_run
        .ask
        .as_ref()
        .expect("the download and run dialog asks");
    assert_eq!(ask.path, path);
    assert!(
        state.dialogs.download_run.runs.is_empty(),
        "nothing starts before the button"
    );
    assert!(state.pending_intents.iter().any(|i| matches!(
        &i.body,
        crate::intent::Intent::Ui(crate::intent::UiIntent::ReopenPopup(crate::intent::OpenRequest { id, .. }))
            if *id == crate::adapters::ui::popup::download_run::DOWNLOAD_RUN_POPUP_ID
    ) && i.origin.is_user()));
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
            result_place: Default::default(),
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
    assert_asked_to_download(&state, "/remote/unknown");
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
            result_place: Default::default(),
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let picker = crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state)
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
            result_place: Default::default(),
        },
    );
    assert!(
        crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none(),
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
            result_place: Default::default(),
        },
    );
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let picker = crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state)
        .expect("unverified plugin open shows the remote picker");
    assert_eq!(picker.dispatch_origin, FileDispatchOrigin::PluginUnverified);
    assert_eq!(picker.origin_surface_id, Some(sid));
    assert!(picker.default_handler.is_none());
    assert_eq!(state.toasts.len(), 0);
}

/// 원격에 열 수 없으면 이름으로 연 원격 경로도 경로 링크와 같이 "다운로드 후 실행" 대화상자로 묻는다.
/// 사용자 입력을 증명하지 못한 plugin 중계 요청도 picker와 같이 묻고 출처를 보존한다. 외부 IPC 요청은 로그로 끝낸다.
#[test]
fn an_unopenable_remote_file_opened_by_name_asks_unless_an_agent_opened_it() {
    for origin in [
        FileDispatchOrigin::User,
        FileDispatchOrigin::PluginUnverified,
        FileDispatchOrigin::Agent,
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
                result_place: Default::default(),
            },
        );
        assert!(
            crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none(),
            "{origin:?}"
        );
        assert!(queued_creates(&state).is_empty(), "{origin:?}");
        assert_eq!(state.toasts.len(), 0, "{origin:?}");
        let asks = state.dialogs.download_run.ask.is_some();
        assert_eq!(asks, origin != FileDispatchOrigin::Agent, "{origin:?}");
        let popup = state.pending_intents.iter().find(|i| {
            matches!(
                &i.body,
                crate::intent::Intent::Ui(crate::intent::UiIntent::ReopenPopup(crate::intent::OpenRequest { id, .. }))
                    if *id == crate::adapters::ui::popup::download_run::DOWNLOAD_RUN_POPUP_ID
            )
        });
        assert_eq!(popup.is_some(), asks, "{origin:?}");
        if let Some(popup) = popup {
            assert_eq!(
                popup.origin.is_user(),
                origin == FileDispatchOrigin::User,
                "{origin:?} keeps its origin on the popup"
            );
        }
        assert!(state.dialogs.download_run.runs.is_empty(), "{origin:?}");
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
            result_place: Default::default(),
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
            result_place: Default::default(),
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
        _policy: crate::file::dispatch::FileDispatchPolicy,
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
                result_place: Default::default(),
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
    let request = super::terminal_link_file_intent(FileTarget::new(path), sid, None, is_mirror);
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
            result_place: Default::default(),
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
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
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
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
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
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
    assert_asked_to_download(&state, "/remote/data.bin");
}

#[test]
fn a_local_terminal_link_click_keeps_the_local_dispatch() {
    let request = super::terminal_link_file_intent(FileTarget::new("/tmp/a.md"), 7, None, false);
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
    assert!(queued_creates(&state).is_empty());
    assert!(state.pending_handler_ipc.is_empty());
    let pending = state.take_pending_intents();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].origin.is_user());
    let picker = match &pending[0].body {
        crate::intent::Intent::Ui(crate::intent::UiIntent::ReopenPopup(request))
            if request.id == crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID =>
        {
            match request.payload.take() {
                crate::adapters::ui::popup::PopupPayload::HandlerPicker(picker) => picker,
                other => panic!("the remote link menu opens a picker, got {other:?}"),
            }
        }
        other => panic!("the remote link menu opens a picker, got {other:?}"),
    };
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
    assert!(crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state).is_none());
    assert_asked_to_download(&state, "/remote/data.bin");
}

/// 대화상자의 경로 줄 앞 표지는 그 mirror 연결의 호스트 글자를 싣는다.
#[test]
fn the_download_dialog_carries_the_mirror_host() {
    let (mut state, mut engine_session) = crate::state::tests::test_mirror_state();
    let engine = engine_session.borrow_mut();
    let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
    let ws = engine
        .read()
        .mirror_workspace_of(sid)
        .expect("mirror surface");
    // 연결 정보가 아직 없으면 빈 글자다(화면은 Remote 낱말).
    super::open_remote_link_picker(
        &mut state,
        &engine.read(),
        FileTarget::new("/remote/a.bin"),
        sid,
    );
    assert_eq!(state.dialogs.download_run.ask.as_ref().unwrap().host, "");
    engine.remote.mirror_locations.insert(
        ws,
        crate::remote::state::MirrorLocation {
            host: "host".into(),
            reconnecting: false,
        },
    );
    super::open_remote_link_picker(
        &mut state,
        &engine.read(),
        FileTarget::new("/remote/b.bin"),
        sid,
    );
    let ask = state.dialogs.download_run.ask.as_ref().unwrap();
    assert_eq!(
        (ask.path.as_str(), ask.host.as_str()),
        ("/remote/b.bin", "host")
    );
}

/// 받은 파일을 origin 없이 연 결과로 쌓인 요청. mirror 가 활성이면 이 경로에서 로컬 임시 파일이
/// 원격으로 forward 되지 않아야 한다.
fn open_locally(
    mirror: bool,
    path: impl FnOnce() -> std::path::PathBuf,
    origin: FileDispatchOrigin,
) -> Vec<crate::intent::DispatchedIntent> {
    open_locally_at(mirror, path, origin, |_| Default::default())
}

/// [`open_locally`] 와 같고, 결과 자리를 고정 픽스처의 engine 에서 정한다.
fn open_locally_at(
    mirror: bool,
    path: impl FnOnce() -> std::path::PathBuf,
    origin: FileDispatchOrigin,
    place: impl FnOnce(&crate::core::CoreState) -> crate::file::dispatch::FileResultPlace,
) -> Vec<crate::intent::DispatchedIntent> {
    let (mut core, _) = build_test_core();
    let (mut state, mut engine_session) = if mirror {
        crate::state::tests::test_mirror_state()
    } else {
        crate::state::tests::test_state()
    };
    let mut engine = engine_session.borrow_mut();
    register_kind(&engine, "com.tasty.markdown", "markdown");
    install_handler(
        &engine,
        "com.tasty.markdown",
        serde_json::json!({"kind": "open_surface", "surface_kind": "markdown", "param_key": "file"}),
    );
    let result_place = place(&engine);
    apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        FileTarget::new(path()),
        Some(DetectorId::new("remote-test")),
        None,
        crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin: origin,
            ignore_size_limit: false,
            run: None,
            result_place,
        },
    );
    state.take_pending_intents()
}

fn creates_workspace(pending: &[crate::intent::DispatchedIntent]) -> bool {
    pending.iter().any(|request| {
        matches!(
            request.body,
            crate::intent::Intent::Domain(DomainIntent::CreateWorkspace { .. })
        )
    })
}

fn opens_tab(pending: &[crate::intent::DispatchedIntent]) -> bool {
    pending
        .iter()
        .any(|request| matches!(request.body, crate::intent::Intent::NewTab { .. }))
}

/// 다운로드 후 실행: 탭을 여는 로컬 핸들러가 mirror 활성 상태에서 받은 파일을 열면 새 로컬
/// workspace 를 만들어 거기서 연다. 그 밖의 경우는 지금까지처럼 활성 workspace 에 탭을 연다.
#[test]
fn a_downloaded_file_opened_on_an_active_mirror_goes_to_a_new_local_workspace() {
    // 픽스처가 데이터 홈을 테스트 폴더로 바꾸므로 경로는 픽스처를 만든 뒤 계산한다.
    let downloaded = || {
        crate::remote::file_download::client::run_root()
            .join("run-1")
            .join("notes.md")
    };
    let pending = open_locally(true, downloaded, FileDispatchOrigin::User);
    assert!(creates_workspace(&pending), "{pending:?}");
    assert!(!opens_tab(&pending));
    let request = pending
        .iter()
        .find(|request| {
            matches!(
                request.body,
                crate::intent::Intent::Domain(DomainIntent::CreateWorkspace { .. })
            )
        })
        .unwrap();
    assert!(request.origin.is_user());
    let crate::intent::Intent::Domain(DomainIntent::CreateWorkspace {
        kind,
        surface_params,
        name,
        ..
    }) = &request.body
    else {
        unreachable!();
    };
    assert_eq!(kind, "markdown");
    assert!(
        surface_params["file"]
            .as_str()
            .is_some_and(|f| std::path::Path::new(f).ends_with(
                std::path::Path::new("download-run")
                    .join("run-1")
                    .join("notes.md")
            )),
        "{surface_params}"
    );
    assert!(name.is_none(), "the name waits for a design");

    // 로컬 workspace 가 활성이면 그 workspace 에 탭을 연다.
    let pending = open_locally(false, downloaded, FileDispatchOrigin::User);
    assert!(
        opens_tab(&pending) && !creates_workspace(&pending),
        "{pending:?}"
    );
    // 받은 파일이 아니면 mirror 활성이어도 바꾸지 않는다.
    let pending = open_locally(true, || "/tmp/other.md".into(), FileDispatchOrigin::User);
    assert!(!creates_workspace(&pending), "{pending:?}");
    // `..` 로 폴더 밖을 가리키면 받은 파일로 보지 않는다.
    let outside = || {
        crate::remote::file_download::client::run_root()
            .join("..")
            .join("escaped.md")
    };
    let pending = open_locally(true, outside, FileDispatchOrigin::User);
    assert!(!creates_workspace(&pending), "{pending:?}");
    // 에이전트 요청은 활성 workspace 를 바꾸지 않는다.
    let pending = open_locally(true, downloaded, FileDispatchOrigin::Agent);
    assert!(!creates_workspace(&pending), "{pending:?}");
}

/// 받은 파일은 받기를 요청한 mirror 가 정한 자리라 완료 때 활성 workspace 가 로컬이어도 새 로컬
/// workspace 에 연다. 입력이 정한 pane 이면 그 pane 의 workspace 가 mirror 인지 본다.
#[test]
fn a_downloaded_file_follows_the_place_its_request_chose() {
    use crate::file::dispatch::FileResultPlace;
    let downloaded = || {
        crate::remote::file_download::client::run_root()
            .join("run-1")
            .join("notes.md")
    };
    let pending = open_locally_at(false, downloaded, FileDispatchOrigin::User, |_| {
        FileResultPlace::NewLocalWorkspace
    });
    assert!(
        creates_workspace(&pending) && !opens_tab(&pending),
        "{pending:?}"
    );
    let first_pane = |engine: &crate::core::CoreState| {
        let workspace = engine.workspace_at(0).expect("workspace");
        FileResultPlace::Pane(workspace.pane_layout().all_pane_ids()[0])
    };
    // mirror workspace 의 pane 에 열 받은 파일은 새 로컬 workspace 로 간다.
    let pending = open_locally_at(true, downloaded, FileDispatchOrigin::User, first_pane);
    assert!(creates_workspace(&pending), "{pending:?}");
    // 로컬 workspace 의 pane 이면 그 pane 에 탭을 연다.
    let pending = open_locally_at(false, downloaded, FileDispatchOrigin::User, first_pane);
    assert!(
        opens_tab(&pending) && !creates_workspace(&pending),
        "{pending:?}"
    );
}

/// 원격 picker 의 표지는 picker 를 연 mirror 의 위치 값을 그릴 때마다 읽는다. 열 때 이미 재연결 중이면
/// 재연결 형태로 시작하고, 위치 값이 바뀌면 따라가며, 위치 값이 빠지면 마지막 값을 둔다. 호스트 글자도
/// 같은 값을 따른다.
#[test]
fn the_remote_picker_mark_follows_the_mirror_reconnecting_state() {
    use crate::adapters::ui::popup::file_handler_picker::{follow_remote_location, opened_picker};
    use crate::remote::state::MirrorLocation;
    let location = |reconnecting| MirrorLocation {
        host: "work-laptop-attach".into(),
        reconnecting,
    };
    let open = |reconnecting: Option<bool>| {
        let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
        let mut engine = engine_session.borrow_mut();
        let ws = engine.workspace_at(0).expect("mirror workspace").id;
        if let Some(r) = reconnecting {
            engine.remote.mirror_locations.insert(ws, location(r));
        }
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
                result_place: Default::default(),
            },
        );
        let picker = opened_picker(&mut state).expect("user open shows the remote picker");
        (picker, engine_session, ws)
    };

    // 열 때 이미 재연결 중이면 재연결 형태로 시작한다.
    let (mut picker, mut engine_session, ws) = open(Some(true));
    assert_eq!(picker.opened_in, Some(ws));
    assert!(
        picker.row_link.is_none(),
        "an agent mirror, not a saved row"
    );
    assert_eq!(picker.remote_host.as_deref(), Some("work-laptop-attach"));
    assert!(picker.remote_reconnecting);
    let engine = engine_session.borrow_mut();
    assert!(follow_remote_location(&mut picker, &engine.read()));
    // 다시 붙으면 평소 표지로 돌아오고, 다시 끊기면 재연결 형태가 된다.
    engine.remote.mirror_locations.insert(ws, location(false));
    assert!(!follow_remote_location(&mut picker, &engine.read()));
    engine.remote.mirror_locations.insert(ws, location(true));
    assert!(follow_remote_location(&mut picker, &engine.read()));
    // 호스트 글자가 바뀌면 따라간다.
    engine.remote.mirror_locations.insert(
        ws,
        MirrorLocation {
            host: "build-eu".into(),
            reconnecting: true,
        },
    );
    assert!(follow_remote_location(&mut picker, &engine.read()));
    assert_eq!(picker.remote_host.as_deref(), Some("build-eu"));
    // 위치 값이 빠지면 마지막으로 본 값(재연결 여부와 호스트 글자)을 둔다.
    engine.remote.mirror_locations.remove(&ws);
    assert!(follow_remote_location(&mut picker, &engine.read()));
    assert_eq!(picker.remote_host.as_deref(), Some("build-eu"));
    // 저장된 원격 행에서 연 picker 는 연결 여부와 관계없이 평소 표지다(행이 끊기면 picker 가 닫힌다).
    engine.remote.mirror_locations.insert(ws, location(true));
    for linked in [true, false] {
        picker.row_link = Some((ws, linked));
        assert!(
            !follow_remote_location(&mut picker, &engine.read()),
            "{linked}"
        );
    }

    // 첫 위치 동기화 전에 열리면 평소 표지와 빈 호스트(화면은 Remote 낱말)로 시작한다.
    let (mut picker, mut engine_session, ws) = open(None);
    assert_eq!(picker.remote_host.as_deref(), Some(""));
    assert!(!picker.remote_reconnecting);
    let engine = engine_session.borrow_mut();
    assert!(!follow_remote_location(&mut picker, &engine.read()));
    engine.remote.mirror_locations.insert(ws, location(true));
    assert!(follow_remote_location(&mut picker, &engine.read()));
    assert_eq!(picker.remote_host.as_deref(), Some("work-laptop-attach"));
    // 로컬 대상 picker 에는 원격 표지가 없으므로 재연결 형태도 없다.
    picker.remote_host = None;
    assert!(!follow_remote_location(&mut picker, &engine.read()));
}

/// 사용자가 연 원격 picker 와 그 엔진. `location` 이 Some 이면 열기 전에 출처 mirror 의 위치를 게시한다.
fn open_user_remote_picker(
    location: Option<crate::remote::state::MirrorLocation>,
) -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
    crate::adapters::ui::popup::PopupPayload,
    u32,
) {
    let (mut core, mut state, mut engine_session, sid) = mirror_with_ipc_first();
    let mut engine = engine_session.borrow_mut();
    let ws = engine.workspace_at(0).expect("mirror workspace").id;
    if let Some(location) = location {
        engine.remote.mirror_locations.insert(ws, location);
    }
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
            result_place: Default::default(),
        },
    );
    let picker = crate::adapters::ui::popup::file_handler_picker::opened_picker(&mut state)
        .expect("user open shows the remote picker");
    drop(engine);
    (
        state,
        engine_session,
        crate::adapters::ui::popup::PopupPayload::HandlerPicker(Box::new(picker)),
        ws,
    )
}

/// 다른 mirror workspace(에이전트 mirror) 하나를 더 두고 그 id 를 돌려준다.
fn push_other_mirror(engine_session: &mut crate::runtime::engine_session::EngineSession) -> u32 {
    let engine = engine_session.borrow_mut();
    let id = 900_002;
    let mut workspace =
        crate::model::Workspace::new_with_terminal_marker(id, "other".into(), id, id, id);
    workspace.mirror = true;
    engine.core.push_mirror_workspace(workspace);
    id
}

/// picker 를 실제 그리기 함수로 한 프레임 그리고 칠한 글자들을 돌려준다.
fn painted_picker(
    state: &mut crate::state::RequestContext,
    engine_session: &mut crate::runtime::engine_session::EngineSession,
    payload: &mut crate::adapters::ui::popup::PopupPayload,
) -> Vec<String> {
    use crate::adapters::ui::popup::file_handler_picker::{
        PICKER_POPUP_ID, draw_file_handler_picker,
    };
    let engine = engine_session.borrow_mut();
    let read = engine.read();
    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut cx = crate::adapters::ui::popup::PopupCx {
                id: PICKER_POPUP_ID,
                instance: crate::adapters::ui::popup::PopupInstanceId::NONE,
                focused: false,
                keys: crate::adapters::ui::popup::keys::PopupKeys::new(
                    ctx,
                    false,
                    crate::adapters::ui::popup::PopupKeyboard::Keys,
                    &crate::adapters::ui::input::edit_key::EditBindings::default(),
                ),
                payload,
            };
            let _action = draw_file_handler_picker(ui, state, &read, &mut cx);
        });
    });
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        if let egui::Shape::Text(text) = &clipped.shape {
            texts.push(text.galley.text().to_owned());
        }
    }
    texts
}

/// 그리는 경로가 프레임마다 출처 mirror 의 위치 값을 읽는다. 열린 동안 재연결이 되면 상태 단어가 붙고,
/// 호스트 글자가 바뀌면 표지가 따라가며, 위치 값이 빠지면 마지막으로 본 값을 그린다.
#[test]
fn the_drawn_remote_mark_follows_the_location_while_the_picker_is_open() {
    use crate::remote::state::MirrorLocation;
    crate::i18n::init("en");
    let word = crate::i18n::t("status_bar.remote_reconnecting");
    let (mut state, mut engine_session, mut payload, ws) =
        open_user_remote_picker(Some(MirrorLocation {
            host: "work-laptop-attach".into(),
            reconnecting: false,
        }));
    let texts = painted_picker(&mut state, &mut engine_session, &mut payload);
    assert!(texts.iter().any(|t| t == "work-laptop-attach"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == word), "{texts:?}");

    let set = |session: &mut crate::runtime::engine_session::EngineSession, host: &str, r| {
        session.borrow_mut().remote.mirror_locations.insert(
            ws,
            MirrorLocation {
                host: host.into(),
                reconnecting: r,
            },
        );
    };
    // 열려 있는 동안 재연결이 되면 상태 단어를 그린다.
    set(&mut engine_session, "work-laptop-attach", true);
    let texts = painted_picker(&mut state, &mut engine_session, &mut payload);
    assert!(texts.iter().any(|t| t == word), "{texts:?}");
    // 호스트 글자가 바뀌면 표지가 따라간다.
    set(&mut engine_session, "build-eu", false);
    let texts = painted_picker(&mut state, &mut engine_session, &mut payload);
    assert!(texts.iter().any(|t| t == "build-eu"), "{texts:?}");
    assert!(
        !texts.iter().any(|t| t == "work-laptop-attach"),
        "{texts:?}"
    );
    assert!(!texts.iter().any(|t| t == word), "{texts:?}");
    // 위치 값이 빠지면(사라짐·동기화 전) 마지막으로 본 값을 그린다.
    set(&mut engine_session, "build-eu", true);
    let _seen = painted_picker(&mut state, &mut engine_session, &mut payload);
    engine_session
        .borrow_mut()
        .remote
        .mirror_locations
        .remove(&ws);
    let texts = painted_picker(&mut state, &mut engine_session, &mut payload);
    assert!(texts.iter().any(|t| t == "build-eu"), "{texts:?}");
    assert!(texts.iter().any(|t| t == word), "{texts:?}");
}
