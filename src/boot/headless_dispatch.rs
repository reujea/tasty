//! 헤드리스 IPC를 단일 engine에 전달한다. 창 선택은 없지만 공용 권한·감사·승인 검사는 유지한다.
//! App 소유 조회를 처리하고, 매니페스트 namespace에 속한 요청은 engine handler보다 먼저 플러그인에 전달한다.

#![cfg(not(feature = "gui"))]

use crate::app::App;
use crate::ipc::handler::gate::{self, GateProgress, Step};
use crate::ipc::server::send_response;
use crate::runtime::engine_access::EngineMut;
use crate::state::RequestContext;

/// GUI와 같은 IPC 회차 예산을 사용한다. 요청이 만든 Intent는 응답 전에 적용한다.
pub(crate) fn pump_ipc(
    app: &mut App,
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
) -> std::ops::ControlFlow<()> {
    if advance_request_gates(app, state, session).is_break() {
        return std::ops::ControlFlow::Break(());
    }
    let mut round = crate::app::ipc_round::IpcRound::begin();
    while let Some(cmd) = round.next(app.hub.ipc_server.as_deref()) {
        // 게이트 결과를 기다리는 요청이 있으면 접수 순서를 지키도록 그 뒤에 선다.
        if !app.gate_queue.is_empty() {
            app.gate_queue.park(
                crate::app::request_gate::Gated::Ipc {
                    cmd,
                    progress: Default::default(),
                },
                false,
            );
            continue;
        }
        // Closes admitted on any path so far must not be republished by a plugin start here.
        sync_closing_surfaces(app, session);
        let observed =
            crate::app::ipc_round::CommandObservation::begin(app.services.pressure(), &cmd);
        let flow = dispatch_command(app, state, session, cmd, Default::default(), false);
        app.services.expect_no_parked_reply();
        observed.finish(app.services.slow_requests());
        if flow.is_break() {
            round.finish(app.services.pressure(), app.services.dispatch());
            return std::ops::ControlFlow::Break(());
        }
    }
    // 남은 명령을 보고 다시 깨우는 일은 호출자가 맡는다.
    round.finish(app.services.pressure(), app.services.dispatch());
    std::ops::ControlFlow::Continue(())
}

/// 맨 앞 요청부터 게이트 결과가 나온 요청을 진행한다. 다시 기다리게 되면 멈춘다.
pub(crate) fn advance_request_gates(
    app: &mut App,
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
) -> std::ops::ControlFlow<()> {
    while let Some(gated) = app.gate_queue.pop_ready(&app.services) {
        match gated {
            crate::app::request_gate::Gated::Ipc { cmd, progress } => {
                sync_closing_surfaces(app, session);
                let flow = dispatch_command(app, state, session, cmd, progress, true);
                app.services.expect_no_parked_reply();
                if flow.is_break() {
                    return std::ops::ControlFlow::Break(());
                }
            }
            crate::app::request_gate::Gated::Plugin {
                call,
                request,
                progress,
            } => super::headless_plugins::dispatch_plugin_call(
                app,
                state,
                &mut session.borrow_mut(),
                call,
                request,
                progress,
                true,
            ),
        }
    }
    std::ops::ControlFlow::Continue(())
}

/// Tell the plugin host which surfaces admitted closes may retire (see the GUI counterpart).
pub(crate) fn sync_closing_surfaces(
    app: &mut App,
    session: &crate::runtime::engine_session::EngineSession,
) {
    if let Some(plugins) = app.plugin_manager.as_mut() {
        plugins.set_closing_surfaces(app.journal.closing_surfaces(&[session]));
    }
}

fn dispatch_command(
    app: &mut App,
    state: &mut RequestContext,
    session: &mut crate::runtime::engine_session::EngineSession,
    mut cmd: crate::ipc::server::IpcCommand,
    mut progress: GateProgress,
    resumed: bool,
) -> std::ops::ControlFlow<()> {
    // 큐 대기는 이미 계측했다. 실행 기한이 지났으면 권한·rate limit을 소비하기 전에 응답한다.
    if progress.claim_once()
        && !crate::app::ipc_round::claim_or_answer(&cmd, app.services.dispatch())
    {
        return std::ops::ControlFlow::Continue(());
    }
    let caller = match gate::resolve_caller(&app.services, &cmd.request, &mut progress) {
        Step::Ready(Ok(c)) => c,
        Step::Ready(Err(resp)) => {
            send_response(&cmd.response_tx, resp);
            return std::ops::ControlFlow::Continue(());
        }
        Step::Pending => return park(app, cmd, progress, resumed),
    };
    if let Some(response) = app.journal.reject_halted_request(&cmd.request) {
        send_response(&cmd.response_tx, response);
        return std::ops::ControlFlow::Continue(());
    }
    if let Step::Pending = gate::prepare(&app.services, &mut cmd.request, &caller, &mut progress) {
        return park(app, cmd, progress, resumed);
    }
    // plugin을 내리는 요청이면 바쁜지 먼저 묻는다 — GUI와 같은 단계(`plugin_busy`).
    if let Step::Pending = progress.ask_busy(app.plugin_manager.as_mut(), &cmd.request) {
        return park(app, cmd, progress, resumed);
    }
    // App 전용 응답도 공용 검사 뒤에 처리한다. checked 요청은 다시 검사하지 않는다.
    let checked = match gate::decide(
        &mut app.services,
        state,
        &mut session.borrow_mut(),
        &cmd.request,
        &caller,
        &mut progress,
    ) {
        Step::Ready(Ok(checked)) => checked,
        Step::Ready(Err(response)) => {
            send_response(&cmd.response_tx, response);
            return std::ops::ControlFlow::Continue(());
        }
        Step::Pending => return park(app, cmd, progress, resumed),
    };
    if let Some(response) =
        progress.busy_refusal(cmd.request.id.clone().unwrap_or(serde_json::Value::Null))
    {
        send_response(&cmd.response_tx, response);
        return std::ops::ControlFlow::Continue(());
    }
    if app.preset_capture_on_session(&cmd, &checked, session, &state.navigation) {
        return std::ops::ControlFlow::Continue(());
    }
    let mut owner = session.borrow_mut();
    let engine = &mut owner;
    if app.journal.admit_ipc(&cmd, &caller) {
        return std::ops::ControlFlow::Continue(());
    }
    if let Some(id) = state.engine_id
        && app
            .journal
            .defer_live_ipc(&cmd, &checked, id, &engine.as_ref())
    {
        return std::ops::ControlFlow::Continue(());
    }
    let mut scope = crate::ipc::request_scope::RequestScope::capture(state, engine.core);
    let intercepted = intercept_app_layer(app, &mut scope, engine, &caller, &cmd);
    scope.finish().apply(state, engine.core);
    match intercepted {
        Some(Intercepted::Answered) => return std::ops::ControlFlow::Continue(()),
        #[cfg(debug_assertions)]
        Some(Intercepted::Shutdown) => return std::ops::ControlFlow::Break(()),
        None => {}
    }
    // 다른 engine으로 보낼 수 없어 잘못 지정한 자원은 거절한다.
    // 플러그인 namespace의 요청까지 차단하지 않도록 호스트 예약 prefix에만 적용한다.
    if let Some(rid) =
        crate::core::request_target::request_resource_id(&cmd.request.method, &cmd.request.params)
        && crate::core::request_target::prefix_is_host_reserved(&cmd.request.method)
        && !crate::core::request_target::engine_has_resource(&engine.as_ref(), rid)
    {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        send_response(
            &cmd.response_tx,
            crate::ipc::protocol::JsonRpcResponse::invalid_params(
                id,
                crate::core::request_target::unowned_target_message(rid, &cmd.request.method),
            ),
        );
        return std::ops::ControlFlow::Continue(());
    }
    // engine 응답 오류를 라우팅 신호로 사용하지 않고 namespace 소유자로 먼저 결정한다.
    if forward_to_plugin_namespace(app, &engine.as_ref(), &caller, &cmd) {
        return std::ops::ControlFlow::Continue(());
    }
    // kind 소유자만 준비한다. namespace 전달과 달리 IPC hook extension은 여기서 시작하지 않는다.
    super::headless_plugins::ensure_plugin_for_surface_kind(app, state, engine, &cmd.request);
    let handled =
        crate::ipc::handler::handle_checked_request(&mut app.services, state, engine, &checked);
    // 응답 전에 요청의 Intent와 후속 이벤트를 적용한다.
    crate::intent::headless::drain_pending_intents_in_app(
        &mut app.services,
        state,
        engine,
        &mut app.journal,
    );
    crate::intent::headless::drain_pending_host_events(&app.services, engine);
    match handled {
        crate::ipc::handler::Handled::Now(response) => send_response(&cmd.response_tx, response),
        crate::ipc::handler::Handled::Later(deferred) => app.services.defer_reply(
            deferred,
            crate::ipc::handler::ReplyTo::Ipc(cmd.response_tx.clone()),
        ),
    }
    std::ops::ControlFlow::Continue(())
}

fn park(
    app: &mut App,
    cmd: crate::ipc::server::IpcCommand,
    progress: GateProgress,
    resumed: bool,
) -> std::ops::ControlFlow<()> {
    app.gate_queue.park(
        crate::app::request_gate::Gated::Ipc { cmd, progress },
        resumed,
    );
    std::ops::ControlFlow::Continue(())
}

enum Intercepted {
    Answered,
    /// debug 전용 system.shutdown 요청은 응답 후 호스트 루프를 끝낸다.
    #[cfg(debug_assertions)]
    Shutdown,
}

/// TimerHub·PluginManager·Lua처럼 App 소유 상태가 필요한 요청을 처리한다.
fn intercept_app_layer(
    app: &mut App,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut EngineMut<'_>,
    caller: &crate::ipc::caller::CallerContext,
    cmd: &crate::ipc::server::IpcCommand,
) -> Option<Intercepted> {
    // 표에 등록된 Mutate 요청은 GUI와 같은 멱등 키 처리를 거친다.
    if let Some(hit) = crate::ipc::handler::idempotency::run_app_layer(
        caller,
        cmd,
        Some(Intercepted::Answered),
        Option::is_some,
        |relayed| intercept_app_layer(app, window, engine, caller, relayed),
    ) {
        return hit;
    }
    if cmd.request.method == "timer.list" {
        let resp = crate::ipc::protocol::JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            app.timer_list_json(std::time::Instant::now()),
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 조회에는 매니저 메타데이터만 필요하다. 플러그인 설치·권한 부여·실행을 하지 않는다.
    if crate::ipc::handler::plugin::is_readonly_method(&cmd.request.method) {
        super::headless_plugins::ensure_plugin_manager_metadata(app, &engine.as_ref());
        let surface_registry = engine.runtime.surface_registry.clone();
        if let Some(resp) = crate::ipc::handler::plugin::dispatch_readonly(
            &app.services,
            app.plugin_manager.as_ref(),
            &surface_registry,
            &cmd.request.method,
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            &cmd.request.params,
        ) {
            // 조회 핸들러는 writer 결과를 기다릴 수 있다.
            app.services.send_handled(&cmd.response_tx, resp);
            return Some(Intercepted::Answered);
        }
    }
    // 지정한 플러그인만 켜야 하므로 전체 discover_and_start 대신 공용 enable/disable을 사용한다.
    if crate::ipc::handler::plugin::is_lifecycle_toggle_method(&cmd.request.method) {
        super::headless_plugins::ensure_plugin_manager_metadata(app, &engine.as_ref());
        if let Some(resp) = crate::ipc::handler::plugin::dispatch_lifecycle_toggle(
            app,
            &cmd.request.method,
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            &cmd.request.params,
        ) {
            app.services.send_handled(&cmd.response_tx, resp);
            return Some(Intercepted::Answered);
        }
    }
    // 승인 popup이 없어도 approval 레코드를 만들고 await·list·respond로 처리할 수 있다.
    if cmd.request.method == "plugin.request_permission" {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let resp = crate::ipc::handler::session::handle_request_permission(
            &mut app.services,
            window,
            engine,
            caller,
            rpc_id,
            &cmd.request.params,
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }

    // 사건 조회도 메타데이터만 준비하며 플러그인 프로세스를 실행하지 않는다.
    if cmd.request.method == "events.fetch" {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        super::headless_plugins::ensure_plugin_manager_metadata(app, &engine.as_ref());
        let args =
            match crate::ipc::handler::events::FetchParams::parse(&cmd.request.params, &rpc_id) {
                Ok(a) => a,
                Err(resp) => {
                    send_response(&cmd.response_tx, resp);
                    return Some(Intercepted::Answered);
                }
            };
        let Some(mgr) = app.plugin_manager.as_ref() else {
            send_response(
                &cmd.response_tx,
                crate::ipc::handler::events::no_bus(rpc_id),
            );
            return Some(Intercepted::Answered);
        };
        let bus = mgr.event_bus.clone();
        if args.wait.is_zero() {
            send_response(
                &cmd.response_tx,
                crate::ipc::handler::events::fetch(&bus, &args, rpc_id),
            );
            return Some(Intercepted::Answered);
        }
        // 긴 조회 대기가 다른 IPC 처리를 막지 않도록 워커에서 기다린다.
        let response_tx = cmd.response_tx.clone();
        std::thread::spawn(move || {
            let resp = crate::ipc::handler::events::fetch(&bus, &args, rpc_id);
            send_response(&response_tx, resp);
        });
        return Some(Intercepted::Answered);
    }

    #[cfg(debug_assertions)]
    if let Some(hit) = intercept_debug_app_layer(app, &engine.as_ref(), cmd) {
        return Some(hit);
    }
    {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        match cmd.request.method.as_str() {
            "clipboard.set_text" => {
                let resp = crate::app::services::clipboard_set_text(
                    &app.services,
                    rpc_id,
                    &cmd.request.params,
                );
                send_response(&cmd.response_tx, resp);
                return Some(Intercepted::Answered);
            }
            "remote.workspaces" => {
                crate::app::services::spawn_remote_workspaces(
                    rpc_id,
                    &cmd.request.params,
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            // 헤드리스의 실제 engine은 App에 없고 이 함수 인자로 전달된다.
            "agent.task_await" => {
                // GUI 라우팅과 같게 이 engine에 없는 workspace는 대상 없음으로 거절한다.
                if let Some(ws) = cmd
                    .request
                    .params
                    .get("workspace_id")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
                    && !engine.has_workspace(ws)
                {
                    send_response(
                        &cmd.response_tx,
                        crate::ipc::handler::agent::task::unowned_await_workspace(rpc_id, ws),
                    );
                    return Some(Intercepted::Answered);
                }
                crate::ipc::handler::agent::task::spawn_task_await(
                    app.services.tasks.awaiter(engine.task_scope),
                    rpc_id,
                    cmd.request.params.clone(),
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            "approval.await" => {
                crate::ipc::handler::approval::spawn_approval_await(
                    app.services.approval_store.clone(),
                    app.services.memory_access().clone(),
                    rpc_id,
                    cmd.request.params.clone(),
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            // winit proxy가 없어 성공 응답 뒤 호출자에게 루프 종료를 반환한다.
            #[cfg(debug_assertions)]
            "system.shutdown" => {
                app.stop_plugin_file_producers();
                send_response(
                    &cmd.response_tx,
                    crate::ipc::protocol::JsonRpcResponse::success(
                        rpc_id,
                        serde_json::json!({"shutdown": true}),
                    ),
                );
                return Some(Intercepted::Shutdown);
            }
            _ => {}
        }
    }
    None
}

/// 창 없이 처리 가능한 debug 메서드. 지원 범위는 docs/dev-guide/headless-ipc-surface.md를 따른다.
#[cfg(debug_assertions)]
fn intercept_debug_app_layer(
    app: &mut App,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    cmd: &crate::ipc::server::IpcCommand,
) -> Option<Intercepted> {
    let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
    if cmd.request.method == "debug.lua.eval" {
        let resp =
            crate::app::services::lua_eval(app.lua_engine.as_ref(), rpc_id, &cmd.request.params);
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 아직 실행한 플러그인이 없어도 매니저를 준비해 빈 조회 결과를 반환한다.
    if cmd.request.method.starts_with("debug.event_bus.") {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        let resp = crate::ipc::handler::debug_plugin::handle_event_bus(
            app.plugin_manager.as_mut(),
            &cmd.request.method,
            &cmd.request.params,
            rpc_id,
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // popup은 조회만 제공한다. 헤드리스에 닫기 처리가 없어 open을 허용하면 정리할 수 없다.
    if cmd.request.method == "debug.popup.list" {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        let resp = crate::ipc::handler::popup::handle_list(app.plugin_manager.as_ref(), rpc_id);
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 전체화면 무대 선언은 조회할 수 있지만 창이 필요한 open·close·state는 처리하지 않는다.
    if cmd.request.method == "debug.fullscreen.list" {
        let resp = crate::app::services::fullscreen_list(rpc_id);
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    if cmd.request.method == "debug.extension.invoke_hook" {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        crate::ipc::handler::debug_plugin::handle_extension_invoke_hook(
            app.plugin_manager.as_mut(),
            &cmd.request.params,
            rpc_id,
            cmd.response_tx.clone(),
        );
        return Some(Intercepted::Answered);
    }
    None
}

/// 메타데이터에서 namespace 소유자를 찾은 뒤 활성 owner와 요청에 맞는 IPC hook extension을 준비한다.
/// 미등록 prefix는 시작하지 않는다. 등록된 prefix 안의 메서드 유효성은 owner가 판단한다.
/// 표에 등록된 Mutate만 멱등 키를 처리하며 플러그인 고유 메서드는 개입하지 않는다.
#[cfg(not(feature = "gui"))]
fn forward_to_plugin_namespace(
    app: &mut App,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &crate::ipc::caller::CallerContext,
    cmd: &crate::ipc::server::IpcCommand,
) -> bool {
    super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
    let owns = app
        .plugin_manager
        .as_ref()
        .is_some_and(|mgr| mgr.owns_namespace(&cmd.request.method));
    if !owns {
        return false;
    }
    let Some(mgr) = app.plugin_manager.as_mut() else {
        return false;
    };
    crate::ipc::handler::idempotency::forward_keeping_the_key(caller, cmd, |c| {
        let id = c.request.id.clone().unwrap_or(serde_json::Value::Null);
        mgr.forward_namespace_call(
            &c.request.method,
            c.request.params.clone(),
            None, // CLI/사용자 호출 — plugin → plugin 호출은 별도 경로(gui 와 같다).
            id,
            c.response_tx.clone(),
            Some(c.request_seq()),
        );
    });
    true
}

/// IPC 명령 하나가 namespace 소유 메서드로 오면 헤드리스 dispatch 전체(공용 검사·journal·
/// App 층 포함)에서 플러그인 전달이 정확히 한 번이고, 그 대기 항목이 명령의 요청 번호를
/// 드는지 stub 플러그인으로 잰다. 파일 밖 헬퍼를 거친 추가 전달도 stub 이 받은 요청 수로 드러난다.
#[cfg(test)]
mod namespace_forward_tests {
    use std::sync::{Arc, mpsc};

    use serde_json::json;

    use super::dispatch_command;
    use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
    use crate::ipc::server::IpcCommand;
    use crate::plugin::PluginManager;

    const OWNER: &str = "com.test.headless-namespace-forward";

    fn command(method: &str, key: Option<&str>) -> (IpcCommand, mpsc::Receiver<JsonRpcResponse>) {
        let (tx, rx) = mpsc::sync_channel(4);
        let request = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: method.into(),
            params: json!({}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: key.map(str::to_string),
        };
        (IpcCommand::new(request, tx), rx)
    }

    fn forwards_once_with_its_seq(prefix: &str, method: &str, key: Option<&str>) {
        let _home = crate::test_support::IsolatedHome::new();
        let (events_tx, _events) = mpsc::channel();
        let waker = crate::adapters::production::headless_waker::HeadlessWaker::new(events_tx);
        let mut app =
            crate::app::App::new_headless(waker.journal_waker(), None, None).expect("headless app");
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, Arc::new(|| {}))
                .expect("engine");
        let mut state = crate::state::RequestContext::new(
            &session.as_ref().read(),
            app.services.preset_store.clone(),
        );
        let mut mgr = PluginManager::with_registries(
            Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
            Arc::new(crate::file::format::FileFormatRegistry::new()),
            Arc::new(crate::file::handler::FileHandlerRegistry::new()),
        );
        let stub = mgr.attach_namespace_stub_for_test(OWNER, prefix);
        app.plugin_manager = Some(mgr);

        let (cmd, rx) = command(method, key);
        let seq = cmd.request_seq();
        let flow = dispatch_command(
            &mut app,
            &mut state,
            &mut session,
            cmd,
            Default::default(),
            false,
        );
        assert!(flow.is_continue());
        let sent = stub.drain_invokes();
        assert_eq!(
            sent.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>(),
            [method],
            "명령 하나에 플러그인 전달이 정확히 한 번이어야 한다. 이미 온 답: {:?}",
            rx.try_recv().ok()
        );
        let mgr = app.plugin_manager.as_ref().expect("manager");
        assert_eq!(
            mgr.pending_origins_for_test(OWNER),
            [(sent[0].0, Some(seq))],
            "전달의 대기 항목이 명령의 요청 번호를 들어야 한다"
        );
    }

    #[test]
    fn a_plugin_method_is_forwarded_once_with_the_commands_request_seq() {
        forwards_once_with_its_seq("hdlfwdns", "hdlfwdns.run", None);
    }

    /// 키가 있는 계약 안 메서드는 보존소의 relay 안에서 같은 전달을 부른다.
    #[test]
    fn a_keyed_table_method_is_forwarded_once_with_the_commands_request_seq() {
        forwards_once_with_its_seq(
            "image",
            "image.open",
            Some("headless-namespace-forward-once"),
        );
    }
}

/// app 계층이 처리하는 조회 메서드도 writer 결과로 원 호출자에게 답한다. 자리표시(-32603)를 보내면
/// 실패한다.
#[cfg(test)]
mod app_layer_reply_tests {
    use std::sync::{Arc, mpsc};

    use serde_json::json;

    use super::dispatch_command;
    use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
    use crate::ipc::server::IpcCommand;

    fn answer(app: &crate::app::App, rx: &mpsc::Receiver<JsonRpcResponse>) -> JsonRpcResponse {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Ok(answer) = rx.try_recv() {
                return answer;
            }
            assert!(std::time::Instant::now() < deadline, "no reply");
            app.services.poll_memory_replies(None);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn read_only_plugin_methods_answer_after_the_writer() {
        let _home = crate::test_support::IsolatedHome::new();
        let (events_tx, _events) = mpsc::channel();
        let waker = crate::adapters::production::headless_waker::HeadlessWaker::new(events_tx);
        let mut app =
            crate::app::App::new_headless(waker.journal_waker(), None, None).expect("headless app");
        assert!(
            app.services.memory_access().is_queued(),
            "writer 있는 구성이어야 한다"
        );
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, Arc::new(|| {}))
                .expect("engine");
        let mut state = crate::state::RequestContext::new(
            &session.as_ref().read(),
            app.services.preset_store.clone(),
        );
        for (id, method) in [
            "plugin.audit_query",
            "plugin.audit_summary",
            "plugin.list_agent_permissions",
        ]
        .into_iter()
        .enumerate()
        {
            let (tx, rx) = mpsc::sync_channel(4);
            let request = JsonRpcRequest {
                caller_agent_id: None,
                jsonrpc: "2.0".into(),
                method: method.into(),
                params: json!({}),
                id: Some(json!(id)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: None,
            };
            let flow = dispatch_command(
                &mut app,
                &mut state,
                &mut session,
                IpcCommand::new(request, tx),
                Default::default(),
                false,
            );
            assert!(flow.is_continue());
            app.services.expect_no_parked_reply();
            let answered = answer(&app, &rx);
            assert!(answered.error.is_none(), "{method}: {answered:?}");
        }
    }
}
