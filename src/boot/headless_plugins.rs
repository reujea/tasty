//! 헤드리스 플러그인 초기화·hello 등록·호스트 IPC 처리·mesh 전달.
//! 조회는 메타데이터만, kind 요청은 소유자만, attach는 전체 활성 플러그인을 준비한다.
//! 입력은 TerminalOutput(None)으로 깨우며 플러그인 타이머와 Busy 주기에서도 pump한다.

use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;
use crate::runtime::engine_access::{EngineMut, EngineRef};
use crate::state::RequestContext;

/// 조회에 필요한 매니저와 설치 목록을 준비한다. 플러그인 설치·권한 부여·프로세스 실행은 하지 않는다.
/// 매니저 생성 과정에서 로그 디렉터리는 만들어질 수 있다. waker_factory가 없으면 경고 후 생략한다.
pub(crate) fn ensure_plugin_manager_metadata(app: &mut App, engine: &EngineRef<'_>) {
    if app.plugin_manager.is_some() {
        return;
    }
    let Some(factory) = engine.runtime.waker_factory.clone() else {
        tracing::warn!(
            "headless plugin manager bootstrap skipped: engine has no waker_factory (invariant violated)"
        );
        return;
    };
    let registries = MetadataRegistries {
        file_format: engine.runtime.file_format.clone(),
        file_handler: engine.runtime.file_handler.clone(),
        surface_registry: engine.runtime.surface_registry.clone(),
    };
    ensure_plugin_manager_metadata_with(app, factory, registries);
}

/// 매니저가 엔진과 공유하는 레지스트리. 엔진의 것은 `app.services.registries`와 같은 Arc다.
pub(crate) struct MetadataRegistries {
    pub(crate) file_format: std::sync::Arc<crate::file::format::FileFormatRegistry>,
    pub(crate) file_handler: std::sync::Arc<crate::file::handler::FileHandlerRegistry>,
    pub(crate) surface_registry:
        std::sync::Arc<crate::runtime::surface_registry::SurfaceKindRegistry>,
}

/// 엔진 없이 매니저를 준비한다. 부팅은 엔진이 첫 셸을 만들기 전에 이 경로를 쓴다.
pub(crate) fn ensure_plugin_manager_metadata_with(
    app: &mut App,
    factory: crate::waker::SharedWakerFactory,
    registries: MetadataRegistries,
) {
    if app.plugin_manager.is_some() {
        return;
    }
    let mut mgr = crate::plugin::PluginManager::with_registries(
        factory,
        registries.file_format,
        registries.file_handler,
    );
    mgr.set_surface_registry(registries.surface_registry);
    // 호스트 요청과 플러그인 대기를 같은 게이지에 기록한다.
    let gauges = app.services.plugin_gauges();
    mgr.set_plugin_wait(gauges.plugin_wait);
    mgr.set_slow_requests(gauges.slow_requests);
    mgr.set_i18n_registrar(std::sync::Arc::new(crate::i18n::BinI18nRegistrar));
    mgr.set_hook_handler_registry(std::sync::Arc::new(
        crate::hook_handler::HostHookHandlerPort,
    ));
    mgr.set_completion_strategy_registry(std::sync::Arc::new(
        crate::completion_strategy::HostCompletionStrategyPort,
    ));
    mgr.refresh_packages();
    tracing::info!("headless plugin manager bootstrapped (metadata only — no plugin started)");
    app.plugin_manager = Some(mgr);
}

/// attach에서 필요한 전체 활성 플러그인을 시작한다. 조회·namespace 요청은 이 경로를 쓰지 않는다.
pub(crate) fn ensure_plugin_manager(app: &mut App, engine: &EngineRef<'_>) {
    if app.state.plugin_started {
        return;
    }
    ensure_plugin_manager_metadata(app, engine);
    let Some(mgr) = app.plugin_manager.as_mut() else {
        return;
    };
    crate::plugin::install_builtins_if_needed(mgr);
    mgr.discover_and_start();
    app.state.plugin_started = true;
    tracing::info!("headless plugin manager started (attach mesh mirror session)");
}

/// hello 등록·플러그인 IPC·mesh 전달을 처리한다. GUI popup·banner 처리는 포함하지 않는다.
pub(crate) fn pump_plugins(app: &mut App, state: &mut RequestContext, engine: &mut EngineMut<'_>) {
    if app.plugin_manager.is_none() {
        return;
    }
    let hello_pairs = {
        let mgr = app.plugin_manager.as_mut().expect("checked Some above");
        let hello_pairs = mgr.pump(std::time::Instant::now());
        // headless에는 다시 그릴 창이 없다. 기록을 비워 쌓이지 않게 한다.
        drop(mgr.take_fresh_frame_surfaces());
        // 복사 결과를 쓸 GUI 클립보드가 없다. 버려서 쌓이지 않게 한다.
        drop(mgr.take_copied_texts());
        crate::app::plugin_failure_notice::report_plugin_failures(mgr, engine);
        hello_pairs
    };
    if !hello_pairs.is_empty() {
        finalize_plugin_hello_headless(app, &engine.as_ref(), hello_pairs);
    }
    dispatch_plugin_ipc_calls_headless(app, state, engine);
    forward_mesh_frames(app, engine);
}

/// 플러그인 허브 기한에 깼으면 pump해 지난 기한이 다음 대기를 계속 0으로 만들지 않게 한다.
pub(crate) fn pump_plugins_if_due(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    now: std::time::Instant,
) -> bool {
    let deadline = app.plugin_manager.as_ref().and_then(|m| m.next_deadline());
    run_if_due(deadline, now, || pump_plugins(app, state, engine))
}

fn run_if_due(
    deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    pump: impl FnOnce(),
) -> bool {
    let due = deadline.is_some_and(|at| at <= now);
    if due {
        pump();
    }
    due
}

/// 공용 mesh 전달 코드가 구독 변경·새 frame·누적 입력을 처리한다.
/// 구독 상태 갱신과 frame 전달은 독립적이다. docs/dev-guide/egui-mesh-channel.md를 참고한다.
fn forward_mesh_frames(app: &mut App, engine: &mut EngineMut<'_>) {
    let Some(mgr) = app.plugin_manager.as_ref() else {
        return;
    };
    crate::plugin_bridge::mesh_forward::forward_mesh_frames_for_engine(
        engine,
        mgr,
        &app.stream_hub,
    );
}

/// 요청의 type에 해당하는 kind가 미등록이면 활성 선언자를 찾아 그 플러그인만 시작한다.
/// 매니페스트·비활성 설정을 먼저 확인하며 설치나 권한 부여는 하지 않는다.
/// 이번 호출이 시작한 프로세스만 연결과 hello 등록을 기다린다. 이 동기 대기 중 다른 IPC는 지연될 수 있다.
pub(crate) fn ensure_plugin_for_surface_kind(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &crate::ipc::protocol::JsonRpcRequest,
) {
    // 메서드명으로 한정하지 않고 type 필드를 읽는다. 이미 등록된 kind면 바로 반환한다.
    let Some(kind) = request.params.get("type").and_then(|v| v.as_str()) else {
        return;
    };
    if engine.runtime.surface_registry.get_live(kind).is_some() {
        return;
    }
    ensure_plugin_manager_metadata(app, &engine.as_ref());
    let Some(owner) = app
        .plugin_manager
        .as_ref()
        .and_then(|mgr| enabled_owner_of_kind(mgr, kind))
    else {
        return;
    };
    let kind = kind.to_string();
    let started = app
        .plugin_manager
        .as_mut()
        .is_some_and(|mgr| mgr.start_one_enabled(&owner));
    if !started {
        // 이미 시작됐거나 이번 시작에 실패했으면 이 요청에서는 기다리지 않는다.
        tracing::debug!(
            "surface kind '{kind}' is declared by '{owner}' but nothing was started here; \
             answering without waiting"
        );
        return;
    }
    wait_for_started_owner(app, state, engine, &kind, &owner);
}

/// 이번 호출이 시작한 소유자의 연결과 kind 등록을 기다리고, 등록되지 않았으면 이유를 남긴다.
fn wait_for_started_owner(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    kind: &str,
    owner: &str,
) {
    let connect_limit = app
        .plugin_manager
        .as_ref()
        .map_or(std::time::Duration::ZERO, |mgr| mgr.connection_wait_limit());
    let outcome = wait_for_kind_registration(connect_limit, KIND_REGISTRATION_WAIT, || {
        pump_plugins(app, state, engine);
        if engine.runtime.surface_registry.get_live(kind).is_some() {
            return OwnerPoll::Registered;
        }
        match app.plugin_manager.as_ref() {
            Some(mgr) if mgr.is_connecting(owner) => OwnerPoll::Connecting,
            Some(mgr) if mgr.is_running(owner) => OwnerPoll::Connected,
            _ => OwnerPoll::Gone,
        }
    });
    match outcome {
        OwnerPoll::Registered => {}
        OwnerPoll::Connecting | OwnerPoll::Gone => tracing::warn!(
            "surface kind '{kind}' is declared by '{owner}' but it did not connect or went \
             down; the request will be answered as an unknown kind"
        ),
        OwnerPoll::Connected => tracing::warn!(
            "surface kind '{kind}' is declared by '{owner}' but was not registered within {:?} \
             of its connection; the request will be answered as an unknown kind",
            KIND_REGISTRATION_WAIT
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerPoll {
    Registered,
    Connecting,
    Connected,
    Gone,
}

/// 연결 대기 기한과 연결 확인 뒤 등록 대기 기한을 따로 센다.
/// poll·sleep이 끝난 뒤 시각을 확인하므로 정확한 반환 시간 상한을 보장하지는 않는다.
fn wait_for_kind_registration(
    connect_limit: std::time::Duration,
    registration_wait: std::time::Duration,
    mut poll: impl FnMut() -> OwnerPoll,
) -> OwnerPoll {
    let connect_give_up = std::time::Instant::now() + connect_limit;
    let mut seen = poll();
    while seen == OwnerPoll::Connecting && std::time::Instant::now() < connect_give_up {
        std::thread::sleep(std::time::Duration::from_millis(5));
        seen = poll();
    }
    if seen != OwnerPoll::Connected {
        return seen;
    }
    let deadline = std::time::Instant::now() + registration_wait;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
        seen = poll();
        if seen != OwnerPoll::Connected {
            return seen;
        }
    }
    seen
}

/// 비활성 플러그인은 시작하지 않을 것이므로 kind 대기 대상에서도 제외한다.
pub(crate) fn enabled_owner_of_kind(
    mgr: &crate::plugin::PluginManager,
    kind: &str,
) -> Option<String> {
    owner_of_kind(
        mgr.packages().iter().map(|pkg| {
            (
                pkg.manifest.id.as_str(),
                pkg.manifest.surface_kinds.as_slice(),
            )
        }),
        |id| mgr.config.is_disabled(id),
        kind,
    )
}

fn owner_of_kind<'a>(
    packages: impl IntoIterator<Item = (&'a str, &'a [crate::plugin::manifest::SurfaceKindDecl])>,
    is_disabled: impl Fn(&str) -> bool,
    kind: &str,
) -> Option<String> {
    packages
        .into_iter()
        .find(|(id, kinds)| !is_disabled(id) && kinds.iter().any(|d| d.kind == kind))
        .map(|(id, _)| id.to_string())
}

/// 연결을 확인한 뒤 hello 등록을 기다리는 시간. 연결 대기는 별도 기한을 사용한다.
const KIND_REGISTRATION_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// hook·surface kind를 등록하되 GUI의 PluginLoaded 등 후속 방송은 하지 않는다.
/// surface_registry가 없으면 surface 등록을 생략하고도 registered_plugins에 표시하므로 재시도를 예약하지 않는다.
fn finalize_plugin_hello_headless(
    app: &mut App,
    engine: &EngineRef<'_>,
    hello_pairs: Vec<(String, String)>,
) {
    let core_registry = engine.runtime.surface_registry.clone();
    let hook_event_registry = engine.runtime.plugin_hook_events.clone();
    let Some(mgr) = app.plugin_manager.as_mut() else {
        return;
    };

    register_hook_events(mgr, &hook_event_registry, &hello_pairs);

    let host_registry = mgr.surface_registry.is_some().then_some(core_registry);
    let Some(registry) = host_registry else {
        tracing::debug!(
            "headless plugin manager has no surface_registry; skipping surface registration of {} plugin(s)",
            hello_pairs.len()
        );
        for (plugin_id, _) in &hello_pairs {
            mgr.registered_plugins.insert(plugin_id.clone());
        }
        return;
    };

    register_surface_kinds(mgr, &registry, &hello_pairs);
}

fn register_hook_events(
    mgr: &crate::plugin::PluginManager,
    hook_event_registry: &std::sync::Arc<crate::core::hook_event_registry::PluginHookEventRegistry>,
    hello_pairs: &[(String, String)],
) {
    for (plugin_id, _) in hello_pairs {
        if let Some(pkg) = mgr.packages().iter().find(|p| &p.manifest.id == plugin_id) {
            let keys: Vec<String> = pkg
                .manifest
                .contributes
                .hook_events
                .iter()
                .map(|h| h.key.clone())
                .collect();
            if !keys.is_empty() {
                hook_event_registry.register(plugin_id, keys);
            }
        }
    }
}

fn register_surface_kinds(
    mgr: &mut crate::plugin::PluginManager,
    registry: &std::sync::Arc<crate::runtime::surface_registry::SurfaceKindRegistry>,
    hello_pairs: &[(String, String)],
) {
    let host_cmd_tx = mgr.host_cmd_tx.clone();
    for (plugin_id, _version) in hello_pairs {
        if let Some(pkg) = mgr
            .packages()
            .iter()
            .find(|p| &p.manifest.id == plugin_id)
            .cloned()
        {
            for decl in &pkg.manifest.surface_kinds {
                if let Some(default) = &decl.default_colors {
                    tasty_themes::add_plugin_surface_default(&decl.kind, default.clone());
                }
                register_one_surface_kind(registry, plugin_id, &pkg.manifest, decl, &host_cmd_tx);
            }
        }
        mgr.registered_plugins.insert(plugin_id.clone());
    }
}

/// remote·webview·egui-mesh를 모두 등록한다. 서버는 원문·제어를 제공하고 실제 렌더는 클라이언트가 할 수 있다.
fn register_one_surface_kind(
    registry: &std::sync::Arc<crate::runtime::surface_registry::SurfaceKindRegistry>,
    plugin_id: &str,
    manifest: &crate::plugin::Manifest,
    decl: &crate::plugin::manifest::SurfaceKindDecl,
    host_cmd_tx: &std::sync::mpsc::Sender<crate::plugin_bridge::host_cmd::HostCmd>,
) {
    match decl.rendering {
        crate::plugin::manifest::SurfaceKindRendering::Webview => {
            crate::runtime::surface_registry::webview_kind::register_webview_kind(
                plugin_id,
                decl,
                &manifest.contributes.settings_pages,
            );
            crate::plugin_bridge::remote_kind::register_remote_kind(
                registry,
                plugin_id,
                decl,
                host_cmd_tx.clone(),
            );
        }
        crate::plugin::manifest::SurfaceKindRendering::Remote => {
            crate::plugin_bridge::remote_kind::register_remote_kind(
                registry,
                plugin_id,
                decl,
                host_cmd_tx.clone(),
            );
        }
        crate::plugin::manifest::SurfaceKindRendering::EguiMesh => {
            crate::runtime::surface_registry::egui_mesh::register_egui_mesh_kind(
                registry,
                plugin_id,
                decl,
                &manifest.api_version,
            );
        }
    }
}

/// GUI와 같은 공용 검사를 인터셉트 전에 실행한다. writer 결과를 기다리면 `Pending`이다.
fn gates_before_intercept<'a>(
    app: &mut App,
    window: &mut crate::state::CommandContext,
    engine: &mut EngineMut<'_>,
    request: &'a crate::ipc::protocol::JsonRpcRequest,
    caller: &'a crate::ipc::caller::CallerContext,
    progress: &mut crate::ipc::handler::gate::GateProgress,
) -> crate::ipc::handler::gate::Step<
    Result<crate::ipc::handler::CheckedRequest<'a>, crate::ipc::protocol::JsonRpcResponse>,
> {
    crate::ipc::handler::gate::decide(&mut app.services, window, engine, request, caller, progress)
}

fn defer_plugin_preset_capture_headless(
    app: &mut App,
    state: &RequestContext,
    engine: &EngineMut<'_>,
    call: &tasty_host_plugin::manager::PendingPluginCall,
) -> bool {
    if call.method != "preset.capture" {
        return false;
    }
    if !app
        .plugin_manager
        .as_ref()
        .is_some_and(|manager| manager.plugin_call_is_current(call))
    {
        tracing::debug!(plugin=%call.plugin_id,"discarding preset capture from a retired plugin process");
        return true;
    }
    let process_binding = call.binding.clone();
    let id = serde_json::Value::from(call.call_id);
    let response = match crate::ipc::handler::preset::decode_capture_request(&call.params, &id) {
        Err(response) => Some(response),
        Ok((kind, source, name)) => {
            let result = match (state.engine_id, engine.journal_binding.cloned()) {
                (Some(engine_id), Some(binding)) => app.journal.queue_preset_capture_borrowed(
                    &engine.as_ref(),
                    engine_id,
                    binding,
                    &state.navigation,
                    kind,
                    source,
                    crate::app::journal::PresetCaptureReply::Plugin {
                        plugin: call.plugin_id.clone(),
                        binding: process_binding.clone(),
                        call_id: call.call_id,
                        name,
                    },
                ),
                _ => Err("preset capture engine is unbound".into()),
            };
            result
                .err()
                .map(|error| crate::ipc::protocol::JsonRpcResponse::invalid_params(id, error))
        }
    };
    if let Some(response) = response
        && let Some(manager) = app.plugin_manager.as_mut()
    {
        manager.send_bound_ipc_result(&call.plugin_id, &process_binding, call.call_id, response);
    }
    true
}

/// shared_buffer.create는 직접 처리하고 다른 플러그인의 namespace 호출은 소유자에게 전달한다.
/// 나머지는 공용 handler에 넘긴다. 창이 필요한 GUI popup·banner 처리는 없다.
fn dispatch_plugin_ipc_calls_headless(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
) {
    let calls = match app.plugin_manager.as_mut() {
        Some(mgr) => mgr.take_pending_plugin_calls(),
        None => return,
    };
    for call in calls {
        if !app
            .plugin_manager
            .as_ref()
            .is_some_and(|manager| manager.plugin_call_is_current(&call))
        {
            continue;
        }
        let request = plugin_call_request(&call);
        // 게이트 결과를 기다리는 요청이 있으면 접수 순서를 지키도록 그 뒤에 선다.
        if !app.gate_queue.is_empty() {
            app.gate_queue.park(
                crate::app::request_gate::Gated::Plugin {
                    call,
                    request,
                    progress: Default::default(),
                },
                false,
            );
            continue;
        }
        dispatch_plugin_call(app, state, engine, call, request, Default::default(), false);
    }
}

fn plugin_caller(call: &PendingPluginCall) -> crate::ipc::caller::CallerContext {
    crate::ipc::caller::CallerContext::Plugin {
        plugin_id: call.plugin_id.clone(),
        permissions: call.permissions.clone(),
    }
}

fn plugin_call_request(call: &PendingPluginCall) -> crate::ipc::protocol::JsonRpcRequest {
    crate::ipc::protocol::JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::Value::from(call.call_id)),
        method: call.method.clone(),
        params: call.params.clone(),
        session_token: None,
    }
}

/// 게이트를 통과한 호출을 실행 분기로 보낸다. 게이트가 writer 결과를 기다리면 호출을 보관하고,
/// 결과가 나오면 같은 함수로 다시 들어온다.
pub(crate) fn dispatch_plugin_call(
    app: &mut App,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    call: PendingPluginCall,
    mut request: crate::ipc::protocol::JsonRpcRequest,
    mut progress: crate::ipc::handler::gate::GateProgress,
    resumed: bool,
) {
    use crate::ipc::handler::gate::Step;
    // 기다리는 동안 plugin process가 바뀌었으면 원 호출에 답할 곳이 없다.
    if resumed
        && !app
            .plugin_manager
            .as_ref()
            .is_some_and(|manager| manager.plugin_call_is_current(&call))
    {
        return;
    }
    let caller = plugin_caller(&call);
    let park = |app: &mut App, call, request, progress| {
        app.gate_queue.park(
            crate::app::request_gate::Gated::Plugin {
                call,
                request,
                progress,
            },
            resumed,
        );
    };
    if let Step::Pending =
        crate::ipc::handler::gate::prepare(&app.services, &mut request, &caller, &mut progress)
    {
        park(app, call, request, progress);
        return;
    }
    // 직접 응답하는 shared_buffer 경로도 권한·cap·rate·audit 검사를 거쳐야 한다.
    let checked = match gates_before_intercept(app, state, engine, &request, &caller, &mut progress)
    {
        Step::Ready(Ok(checked)) => checked,
        Step::Ready(Err(resp)) => {
            let (msg, code) = match resp.error {
                Some(e) => (Some(e.message), Some(e.code)),
                None => (None, None),
            };
            if let Some(mgr) = app.plugin_manager.as_mut() {
                mgr.send_plugin_call_result(&call, None, msg, code);
            }
            return;
        }
        Step::Pending => {
            park(app, call, request, progress);
            return;
        }
    };
    if call.method == tasty_plugin_protocol::METHOD_HOST_SHARED_BUFFER_CREATE {
        let size = call
            .params
            .get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if let Some(mgr) = app.plugin_manager.as_mut() {
            let (result, error) =
                match mgr.create_shared_buffer_for(&call.plugin_id, call.call_id, size) {
                    Ok(r) => (serde_json::to_value(&r).ok(), None),
                    Err(e) => (None, Some(e)),
                };
            mgr.send_plugin_call_result(&call, result, error, None);
        }
        return;
    }
    // GUI와 같은 규칙으로 다른 플러그인의 namespace 호출을 소유자에게 전달한다.
    if let Some(mgr) = app.plugin_manager.as_mut()
        && mgr.forward_plugin_call_to_other_namespace(
            &call,
            crate::app::journal::commands::host_converts(&call.method),
        )
    {
        return;
    }
    if defer_plugin_preset_capture_headless(app, state, engine, &call) {
        return;
    }
    if app
        .journal
        .admit_plugin(&request, &caller, &call, app.plugin_manager.as_ref())
    {
        return;
    }
    if let Some(id) = state.engine_id
        && app.journal.defer_plugin_input(
            &checked,
            id,
            &engine.as_ref(),
            &call,
            app.plugin_manager.as_ref(),
        )
    {
        return;
    }
    let handled =
        crate::ipc::handler::handle_checked_request(&mut app.services, state, engine, &checked);
    // 결과를 보내기 전에 요청의 Intent와 후속 이벤트를 적용한다.
    crate::intent::headless::drain_pending_intents_in_app(
        &mut app.services,
        state,
        engine,
        &mut app.journal,
    );
    crate::intent::headless::drain_pending_host_events(&app.services, engine);
    let response = match handled {
        crate::ipc::handler::Handled::Now(response) => response,
        crate::ipc::handler::Handled::Later(deferred) => {
            app.services
                .defer_reply(deferred, crate::ipc::handler::ReplyTo::plugin(&call));
            return;
        }
    };
    // 오류 코드도 함께 전달해 플러그인이 원래 실패 종류를 알 수 있게 한다.
    let (result, error, code) = match response.error {
        Some(err) => (None, Some(err.message), Some(err.code)),
        None => (response.result, None, None),
    };
    if let Some(mgr) = app.plugin_manager.as_mut() {
        mgr.send_plugin_call_result(&call, result, error, code);
    }
}

#[cfg(test)]
mod cross_plugin_tests;

#[cfg(test)]
mod tests {
    use super::{OwnerPoll, owner_of_kind, run_if_due, wait_for_kind_registration};
    use std::time::{Duration, Instant};

    #[test]
    fn a_passed_plugin_deadline_is_pumped() {
        let now = Instant::now();
        for deadline in [now - Duration::from_millis(50), now] {
            let mut pumped = 0;
            assert!(run_if_due(Some(deadline), now, || pumped += 1));
            assert_eq!(pumped, 1, "기한이 된 플러그인 허브를 pump하지 않았다");
        }
    }

    #[test]
    fn a_future_or_absent_plugin_deadline_is_not_pumped() {
        let now = Instant::now();
        for deadline in [Some(now + Duration::from_millis(50)), None] {
            let mut pumped = 0;
            assert!(!run_if_due(deadline, now, || pumped += 1));
            assert_eq!(pumped, 0);
        }
    }

    #[test]
    fn one_pump_moves_a_passed_plugin_deadline_into_the_future() {
        let factory: tasty_terminal::waker_factory::SharedWakerFactory =
            std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory);
        let mut mgr = crate::plugin::PluginManager::with_registries(
            factory,
            std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
            std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        let first = mgr
            .next_deadline()
            .expect("검사에 사용할 초기 플러그인 타이머가 없다");
        let late = first + Duration::from_millis(1);
        assert!(run_if_due(mgr.next_deadline(), late, || {
            mgr.pump(late);
        }));
        let next = mgr.next_deadline().expect("pump 뒤 주기 타이머가 사라졌다");
        assert!(
            next > late,
            "pump 뒤의 데드라인이 미래로 바뀌지 않았다: {next:?} <= {late:?}"
        );
    }

    fn decl(kind: &str) -> crate::plugin::manifest::SurfaceKindDecl {
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "display_name_i18n_key": format!("surface.kind.{kind}"),
        }))
        .expect("decl 을 만들지 못했다")
    }

    #[test]
    fn a_disabled_plugin_does_not_own_its_kind() {
        let md = [decl("markdown")];
        let pkgs = [("com.tasty.markdown", md.as_slice())];
        assert_eq!(
            owner_of_kind(pkgs, |_| false, "markdown").as_deref(),
            Some("com.tasty.markdown"),
            "활성 선언자를 소유자로 찾아야 한다"
        );
        assert_eq!(
            owner_of_kind(pkgs, |id| id == "com.tasty.markdown", "markdown"),
            None,
            "비활성 플러그인을 기동할 kind 소유자로 선택했다"
        );
    }

    #[test]
    fn an_enabled_plugin_does_not_own_a_kind_it_never_declared() {
        let md = [decl("markdown")];
        let pkgs = [("com.tasty.markdown", md.as_slice())];
        assert_eq!(owner_of_kind(pkgs, |_| false, "image"), None);
    }

    #[test]
    fn the_first_enabled_declarer_answers() {
        let a = [decl("markdown")];
        let b = [decl("markdown")];
        let pkgs = [("com.first", a.as_slice()), ("com.second", b.as_slice())];
        assert_eq!(
            owner_of_kind(pkgs, |id| id == "com.first", "markdown").as_deref(),
            Some("com.second"),
            "첫 선언자가 비활성이면 다음 활성 선언자를 찾아야 한다"
        );
    }

    /// 연결 지연이 등록 대기 시간을 소비하지 않아야 한다.
    #[test]
    fn a_kind_is_registered_when_its_owner_connects_after_the_registration_wait() {
        let started = Instant::now();
        let connects_at = started + Duration::from_millis(1000);
        let registers_at = connects_at + Duration::from_millis(100);
        let outcome =
            wait_for_kind_registration(Duration::from_secs(10), Duration::from_millis(500), || {
                let now = Instant::now();
                if now < connects_at {
                    OwnerPoll::Connecting
                } else if now < registers_at {
                    OwnerPoll::Connected
                } else {
                    OwnerPoll::Registered
                }
            });
        assert_eq!(
            outcome,
            OwnerPoll::Registered,
            "연결 뒤 별도로 등록 대기 시간을 주지 않았다"
        );
    }

    #[test]
    fn an_owner_that_never_connects_ends_at_the_connect_limit() {
        let started = Instant::now();
        let outcome =
            wait_for_kind_registration(Duration::from_millis(100), Duration::from_secs(30), || {
                OwnerPoll::Connecting
            });
        assert_eq!(outcome, OwnerPoll::Connecting);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "연결 상한이 지난 뒤 등록 시한까지 기다렸다"
        );
    }
}
