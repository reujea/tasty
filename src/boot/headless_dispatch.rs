//! Headless 빌드 전용 IPC dispatch.
//!
//! gui 의 `App::process_ipc` (`src/app/ipc.rs`, `#[cfg(feature="gui")]`) 는 view /
//! parked_states / plugin_manager 의존이 큰 5-step 라우터다. headless 는 engine 이
//! 단 하나뿐이라 그 전부가 불필요하므로, caller 해석 → 공통 게이트 → 검사 완료 요청 실행으로
//! 간소화한다.
//!
//! 생략(gui 대비):
//! - app_methods / window_required / debug step (창/스크린샷/system.shutdown 등).
//!   예외는 `timer.list` 하나 — 읽는 대상(TimerHub)이 `App` 에 있어 engine handler
//!   로는 답할 수 없고, 관측이 gui 에서만 되면 headless 인스턴스의 wakeup 원인을
//!   물어볼 방법이 사라진다.
//! - dispatch_list_global / find_request_owner / parked fallback (engine 1 개)
//!
//! **caller elevation / audit-on-deny 는 생략하지 않는다** — 아래 1b의 공통 게이트 내부
//! `check_permission_gate`(`src/adapters/ipc/handler.rs`)가 deny 를 audit 에
//! `AuditDecision::Deny` 로 남기고, Agent caller 의 `MissingPermission` 이면
//! capability elevation 을 발행해 그 좌표를 오류 `data` 에 싣는다. gui 의
//! `caller_gate.rs` step 1 과 같은 일이다.
//!
//! plugin namespace forward 는 **생략하지 않는다** — plugin 이 contribute 한
//! namespace(`markdown.*` 등)는 CLI 로 노출된 에이전트 표면이라 headless 에서도
//! 답해야 한다(`docs/identity.md` 원칙 2). 배치도 gui 와 **같다**: engine handler
//! **앞**에서 정한다(gui 는 `app/ipc/routing.rs` step 5, 여기서는 아래 2d). 재료도
//! 같은 매니페스트 하나다 — namespace 소속은 `~/.tasty/plugins/` 의 매니페스트가
//! 선언하는 정적 사실이라 plugin 을 하나도 안 띄우고 답이 나고, 기동은 소속이
//! 맞은 뒤에만 한다([ADR-0173](../../docs/adr/0173-namespace-resolution-reads-the-manifest-not-the-process-table.md)).
//!
//! 예전에는 이 판정이 engine handler 가 `-32601` 을 돌려준 **뒤**의 fallback 이었고
//! "engine 이 못 답했나" 를 오류 코드로 물었다. 그 형태는 종단이 내는 코드를 라우팅
//! 신호로 고정해, 종단을 더 정확하게 만드는 변경이 forward 를 조용히 깨뜨렸다 —
//! 실측으로 표에 등재된 채 plugin namespace 아래 있던 여덟이 그렇게 새었다. 지금
//! 코드는 라우팅에 안 쓰인다.

#![cfg(not(feature = "gui"))]

use crate::app::App;
use crate::core::CoreState;
use crate::ipc::caller::resolve_caller_from_envelope;
use crate::ipc::server::send_response;
use crate::state::AppState;

/// IPC 큐를 비차단으로 비우고 각 명령을 단일 engine 으로 dispatch 한다.
/// `IpcReady` 수신 시 headless 메인 루프가 호출.
///
/// `state` 는 gui 와 같은 `AppState` 타입이지만 이 빌드에서는 GUI 소유 필드(`dialogs` 등)가
/// 컴파일되지 않은 형태다 — 좁은 타입을 따로 두지 않은 이유는
/// [ADR-0355](../../docs/adr/0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md).
/// 이 함수는 헤드리스 인스턴스의 창 하나를 **소유하는 자리**라 그것을 받는다 — 요청이 낸
/// intent 를 응답 전에 그 창의 큐에서 비워 적용하고(intent 적용이 창 상태를 받는다, ADR-0111),
/// 창 상태 자체가 대상인 debug 핸들러에 그 창을 건넨다. 엔진 핸들러 표와 공통 게이트는 이
/// 값을 `AppState` 가 아니라 좁은 포트(`IpcWindow`)와 intent 출구로만 본다
/// ([ADR-0471](../../docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md)).
pub(crate) fn pump_ipc(
    app: &mut App,
    state: &mut AppState,
    engine: &mut CoreState,
) -> std::ops::ControlFlow<()> {
    // 한 회차의 규칙(수 예산 · 시간 예산 · 이월)은 gui 쪽 `process_ipc` 와 같은 것을 쓴다
    // (`crate::app::ipc_round`). 두 경로의 집계 범위가 어긋나면 headless 에서만 보이는
    // 적체를 못 읽는다 — 그래서 세는 자리도 그 안의 한 곳이다.
    let mut round = crate::app::ipc_round::IpcRound::begin();
    while let Some(cmd) = round.next(app.hub.ipc_server.as_deref()) {
        let observed = crate::app::ipc_round::CommandObservation::begin(app.core.pressure(), &cmd);
        let flow = dispatch_command(app, state, engine, cmd);
        observed.finish(app.core.slow_requests());
        if flow.is_break() {
            round.finish(app.core.pressure(), app.core.dispatch());
            return std::ops::ControlFlow::Break(());
        }
    }
    // 멈춘 이유는 여기서 쓰지 않는다. 이 루프는 이벤트 하나마다 due 한 타이머를 돌리므로
    // 부하가 타이머를 밀어내지 않고, 남은 명령은 부르는 쪽(`src/boot.rs` 의
    // `dispatch_headless_event`)이 장부를 보고 루프를 다시 깨운다 — 명령마다의 wake 는
    // 게이트로 하나로 접힌다. gui 는 따로 다룬다(`src/app/ipc.rs` 의 `IpcPacer`).
    round.finish(app.core.pressure(), app.core.dispatch());
    std::ops::ControlFlow::Continue(())
}

/// 명령 하나를 caller 해석부터 응답까지 끝까지 다룬다. 답은 이 안에서 나간다.
///
/// `Break` 는 데몬을 멈추라는 명령(debug `system.shutdown`)이었다는 뜻이다 — 회차가 그대로
/// 루프를 끝낸다.
fn dispatch_command(
    app: &mut App,
    state: &mut AppState,
    engine: &mut CoreState,
    cmd: crate::ipc::server::IpcCommand,
) -> std::ops::ControlFlow<()> {
    // 큐 대기는 꺼낸 자리(`CommandObservation::begin`)가 이미 쟀다 — gui 와 같은 자리다.
    // 0) 기한이 큐에서 지났으면 실행하지 않고 답한다 — 게이트보다 앞이다(gui 와 같은 자리,
    //    ADR-0411).
    if !crate::app::ipc_round::claim_or_answer(&cmd, app.core.dispatch()) {
        return std::ops::ControlFlow::Continue(());
    }
    // 1) caller 해석 (Local / Agent / 세션 토큰 검증). 실패 시 에러를 그대로 회신.
    let caller = match resolve_caller_from_envelope(&app.core, &cmd.request) {
        Ok(c) => c,
        Err(resp) => {
            send_response(&cmd.response_tx, resp);
            return std::ops::ControlFlow::Continue(());
        }
    };
    // 1b) 모든 조기 응답보다 먼저 검사·관측한다. 이후 같은 요청은 재소비하지 않는다.
    let checked = match crate::ipc::handler::check_request(
        &mut app.core,
        state,
        engine,
        &cmd.request,
        &caller,
    ) {
        Ok(checked) => checked,
        Err(response) => {
            send_response(&cmd.response_tx, response);
            return std::ops::ControlFlow::Continue(());
        }
    };
    // 2) App 층 가로채기 — 창이 없어도 답이 정의되는 표면들을 engine 앞에서 답한다.
    //    어느 하나가 답했으면 응답은 그 안에서 이미 나갔다. 그 안의 갈래는
    //    **그 함수에 적힌 순서 그대로** `2-hub` / `2-plugin` / `2-toggle` /
    //    `2-elev` / debug / `2-surface` 다 — debug 가 `2-surface` **앞**이라
    //    `system.shutdown`(debug 격리)은 뒤엣것에서 답한다.
    match intercept_app_layer(app, state, engine, &caller, &cmd) {
        Some(Intercepted::Answered) => return std::ops::ControlFlow::Continue(()),
        #[cfg(debug_assertions)]
        Some(Intercepted::Shutdown) => return std::ops::ControlFlow::Break(()),
        None => {}
    }
    // 2c) 지목한 대상을 이 engine 이 안 가졌으면 거절한다 — gui 와 같은 판정
    //     (`app/ipc/routing.rs`). 헤드리스는 engine 이 하나라 라우팅할 곳이 없지만,
    //     **판정은 있어야 한다**: 없으면 대상을 잘못 적은 요청이 그대로 실행된다.
    //     실측(2026-09-05): `workspace.create {workspace_id: <없는 id>}` 가 성공을
    //     돌려주고 워크스페이스를 만들었다 — 핸들러가 그 키를 안 읽기 때문이다.
    //
    //     예약 prefix 로 한정하는 이유는 아래 5) 와의 순서다. 예약되지 않은
    //     prefix 는 plugin 이 점유할 수 있어서, 여기서 자르면 forward 될 호출을
    //     불러 보기도 전에 죽인다. 예약된 것은 어떤 plugin 도 못 가지므로
    //     (매니페스트 검증이 거절한다) 그런 위험이 없다.
    if let Some(rid) =
        crate::core::request_target::request_resource_id(&cmd.request.method, &cmd.request.params)
        && crate::core::request_target::prefix_is_host_reserved(&cmd.request.method)
        && !crate::core::request_target::engine_has_resource(engine, rid)
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
    // 2d) plugin namespace forward — **engine 에 묻기 전에** 정한다. gui 가
    //     라우터 step 5 에서 같은 자리를 잡는 것과 같은 순서이고, 재료도 같은
    //     `mgr.namespace_owner` 하나다.
    //
    //     예전에는 이 판정이 engine 응답 **뒤**에 있었고 "engine 이 못 답했나" 를
    //     오류 코드로 물었다. 그 형태는 종단이 내는 코드를 라우팅 신호로 고정해,
    //     종단을 더 정확하게 만드는 변경이 forward 를 조용히 깨뜨렸다(실측: 표에
    //     등재된 채 plugin namespace 아래 있던 여덟). 이제 코드는 라우팅에 안
    //     쓰인다 — [ADR-0173](../../docs/adr/0173-namespace-resolution-reads-the-manifest-not-the-process-table.md).
    if forward_to_plugin_namespace(app, engine, &caller, &cmd) {
        return std::ops::ControlFlow::Continue(());
    }
    // 2e) plugin 이 선언한 surface kind 를 지목했으면 **그 하나를** 먼저 띄운다.
    //     바로 위 forward 와 **같은 두 층**이다 — 소속은 매니페스트가 답하고,
    //     기동은 소속이 맞은 뒤에만 한다. 다른 것은 묻는 대상뿐이다(메서드 이름 vs
    //     surface kind).
    //
    //     namespace는 매칭 IPC hook extension도 준비할 수 있지만, kind는
    //     그 kind를 선언한 소유자 하나만 준비한다
    //     (`start_one_enabled`). 설치·권한 grant 도 여기엔 없다. 근거·대기 시한은
    //     `headless_plugins::ensure_plugin_for_surface_kind`.
    super::headless_plugins::ensure_plugin_for_surface_kind(app, state, engine, &cmd.request);
    // 3) 이미 검사한 요청을 engine handler에 넘긴다.
    let resp = crate::ipc::handler::handle_checked_request(&mut app.core, state, engine, &checked);
    // 4) 핸들러가 발화한 Intent 를 **응답 전에** 적용한다. gui 의
    //    `App::dispatch_checked` 가 응답 반환 전에 `dispatch_pending_intents`
    //    를 부르는 것과 같은 계약이며, 이게 없으면 큐가 프로세스 수명 동안 쌓이고
    //    (`docs/adr/0111-headless-drains-the-intent-queue.md`) set_mark /
    //    completion / notification 같은 에이전트 표면이 headless 에서 무응답이 된다.
    crate::intent::headless::drain_pending_intents(&mut app.core, state, engine);
    crate::intent::headless::drain_pending_host_events(&app.core, state, engine);
    send_response(&cmd.response_tx, resp);
    std::ops::ControlFlow::Continue(())
}

/// [`intercept_app_layer`] 의 답 — 응답은 그 안에서 이미 보냈다.
enum Intercepted {
    /// 이 명령은 여기서 끝났다. 다음 명령으로 넘어간다.
    Answered,
    /// 데몬을 멈추라는 답이었다. 호출자(run loop)가 break 한다.
    ///
    /// 이 답을 내는 `system.shutdown` 이 debug 격리라 variant 도 같은 cfg 다.
    #[cfg(debug_assertions)]
    Shutdown,
}

/// engine handler 앞에서 답하는 **App 층 표면** 전부.
///
/// 가르는 축은 하나다 — *읽는 것이 `App` 필드인가*. `timer.list` 의 TimerHub,
/// `plugin.*` 의 `plugin_manager`, `lua_engine` 이 그렇다. `CoreState` 만 받는 engine
/// handler 는 그 셋에 닿지 못하므로, 여기서 안 답하면 헤드리스에서만 `-32601` 이 된다
/// (`docs/identity.md` 원칙 2 — 에이전트 표면이 조합에 따라 사라지지 않는다).
/// gui 는 같은 자리를 라우터의 app_methods / debug step 이 맡고, **부르는 함수는 같다.**
///
/// `pump_ipc` 에서 갈라낸 이유는 길이가 아니라 축이다 — 그쪽에 남은 것은 caller 해석 ·
/// 게이트 · 라우팅이고, 여기 있는 것은 종단 응답이다. 갈라 두면 새 App 층 표면이
/// 라우팅 코드를 건드리지 않고 는다.
fn intercept_app_layer(
    app: &mut App,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut CoreState,
    caller: &crate::ipc::caller::CallerContext,
    cmd: &crate::ipc::server::IpcCommand,
) -> Option<Intercepted> {
    // 멱등 키를 실은 `Mutate` 는 보존소를 먼저 지난다 — gui 의 app_methods step 과 같은
    // 함수다(ADR-0421).
    if let Some(hit) = crate::ipc::handler::idempotency::run_app_layer(
        caller,
        cmd,
        Some(Intercepted::Answered),
        Option::is_some,
        |relayed| intercept_app_layer(app, window, engine, caller, relayed),
    ) {
        return hit;
    }
    // 2-hub) 허브 관측만 App 층에서 가로챈다 — `timer.list` 가 읽는 TimerHub 는
    //    `App` 필드(+ plugin manager 자기 허브)라 `CoreState` 만 받는 engine
    //    handler 에서는 닿지 않는다. gui 의 app_methods step 과 같은 함수를 쓴다.
    if cmd.request.method == "timer.list" {
        let resp = crate::ipc::protocol::JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            app.timer_list_json(std::time::Instant::now()),
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 2-plugin) 읽기 전용 `plugin.*` 조회도 App 층에서 답한다 — `plugin_manager` 는 `App`
    //     필드라 engine handler 가 닿지 않는다. gui 라우터와 **같은 함수**를 쓴다
    //     (`handler::plugin::dispatch_readonly`), 두 벌로 두면 갈라지기 때문이다.
    //
    //     매니저는 여기서 **메타데이터 층까지만** 세운다. plugin 프로세스를 띄우거나
    //     번들을 설치·권한 grant 하는 것은 조회의 부수효과일 수 없다
    //     (`headless_plugins::ensure_plugin_manager_metadata` 주석).
    if crate::ipc::handler::plugin::is_readonly_method(&cmd.request.method) {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        let surface_registry = engine.surface_registry.clone();
        if let Some(resp) = crate::ipc::handler::plugin::dispatch_readonly(
            &app.core,
            app.plugin_manager.as_ref(),
            &surface_registry,
            &cmd.request.method,
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            &cmd.request.params,
        ) {
            send_response(&cmd.response_tx, resp);
            return Some(Intercepted::Answered);
        }
    }
    // 2-toggle) `plugin.enable` / `plugin.disable` — 창을 안 보는 수명주기 쓰기 둘.
    //     gui 라우터와 **같은 함수**를 부른다(`handler::plugin::dispatch_lifecycle_toggle`).
    //
    //     매니저는 여기서도 **메타데이터 층까지만** 세운다. 부팅이 이미 번들을
    //     설치해 뒀으므로(`src/boot.rs` 의 `install_builtins_if_needed`) 이 층이면
    //     package 표가 차 있고, `PluginManager::enable` 은 그 표에서 **지목한
    //     하나만** 찾아 기동한다. `ensure_plugin_manager`(= `discover_and_start`)를
    //     부르면 안 되는 이유가 그것이다 — 하나를 켜라는 명령이 전부를 띄운다.
    //
    //     낸 이벤트는 창 큐가 없으므로 그 자리에서 소비한다. gui 의
    //     `cascade_plugin_events` 가 하는 일 중 이 둘에 해당하는 부분이다.
    if crate::ipc::handler::plugin::is_lifecycle_toggle_method(&cmd.request.method) {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        let hook_events = engine.plugin_hook_events.clone();
        let surface_registry = engine.surface_registry.clone();
        if let Some((resp, events)) = crate::ipc::handler::plugin::dispatch_lifecycle_toggle(
            app.plugin_manager.as_mut(),
            &surface_registry,
            &cmd.request.method,
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            &cmd.request.params,
        ) {
            if let Some(mgr) = app.plugin_manager.as_mut() {
                crate::ipc::handler::plugin::cascade_toggle_events_headless(
                    mgr,
                    &hook_events,
                    events,
                );
            }
            send_response(&cmd.response_tx, resp);
            return Some(Intercepted::Answered);
        }
    }
    // 2-elev) `plugin.request_permission` — agent 가 권한 부족을 미리 알고
    //     capability_elevation 을 자체 발행하는 자리. gui 는 첫 main window 의
    //     state 를 빌려 이것을 답하는데(`app/ipc/app_methods.rs`), 헤드리스는
    //     `state`·`engine` 을 이미 손에 들고 있어 같은 핸들러가 그대로 선다.
    //
    //     **팝업이 없다고 하는 일이 없는 것은 아니다.** 이 메서드가 만드는 것은
    //     approval **레코드**이고, 헤드리스에서도 `approval.await`·`approval.list`
    //     ·`approval.respond` 가 그 레코드에 닿는다. 팝업은 창이 있을 때 더해지는
    //     표시일 뿐이라 `publish_capability_elevation` 안에서 gui feature 로
    //     갈린다. 이것이 없으면 거부당한 헤드리스 agent 는 권한을 **요청할 자리**가
    //     없다 — 위 1b) 가 거부에 격상 레코드를 실어 주는 것과 같은 축의 나머지
    //     절반이다.
    if cmd.request.method == "plugin.request_permission" {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let resp = crate::ipc::handler::session::handle_request_permission(
            &mut app.core,
            window,
            engine,
            &caller,
            rpc_id,
            &cmd.request.params,
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }

    // 2-events) 사건 피드 조회. 버스는 `PluginManager` 가 소유하므로 여기서도
    //     **메타데이터 층까지만** 세운다 — 조회가 plugin 프로세스를 띄우면 관측이
    //     자기 대상을 바꾼다(ADR-0136). release 에도 있어야 하는 표면이라 아래
    //     debug 층이 아니라 이 자리다.
    if cmd.request.method == "events.fetch" {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
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
        // 기다리는 동안 데몬의 dispatch 루프를 막으면 그 사이 다른 호출이 전부
        // 밀린다 — `agent.task_await` 가 같은 이유로 워커로 나간다.
        let response_tx = cmd.response_tx.clone();
        std::thread::spawn(move || {
            let resp = crate::ipc::handler::events::fetch(&bus, &args, rpc_id);
            send_response(&response_tx, resp);
        });
        return Some(Intercepted::Answered);
    }

    #[cfg(debug_assertions)]
    if let Some(hit) = intercept_debug_app_layer(app, engine, cmd) {
        return Some(hit);
    }
    // 2-surface) app 층 표면 중 **창이 없어도 답이 정의되는 것**을 여기서 답한다.
    //     gui 의 `app_methods` step 과 **같은 함수**를 부른다
    //     (`crate::core::app_surface`) — 두 벌로 두면 한쪽만 고쳐지는 순간 갈라진다.
    //
    //     여기 없는 app 층 메서드(`window.*` · `view.*` · `ui.screenshot` ·
    //     `remote.attach` · `system.gpu_stats`)는 읽는 것이 `App.view` 라서
    //     헤드리스에 대응물이 없다. 그 판정은 메서드별로
    //     `docs/dev-guide/headless-ipc-surface.md` 에 적혀 있다.
    {
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        match cmd.request.method.as_str() {
            "clipboard.set_text" => {
                let resp = crate::core::app_surface::clipboard_set_text(
                    &app.core,
                    rpc_id,
                    &cmd.request.params,
                );
                send_response(&cmd.response_tx, resp);
                return Some(Intercepted::Answered);
            }
            "remote.workspaces" => {
                crate::core::app_surface::spawn_remote_workspaces(
                    rpc_id,
                    &cmd.request.params,
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            // 대기 대상 store 는 **이 engine** 것이다. gui 처럼 창·parked 를 훑을
            // 일이 없다 — 헤드리스는 engine 이 하나뿐이고 `app.core_state` 는
            // 부팅이 채우지 않는다(`boot::bootstrap_engine` 이 새로 만들어 돌려준다).
            "agent.task_await" => {
                crate::ipc::handler::agent::task::spawn_task_await(
                    engine.task_waker_hub.clone(),
                    app.core.memory_arc(),
                    engine.agent_seq.clone(),
                    rpc_id,
                    cmd.request.params.clone(),
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            "approval.await" => {
                crate::ipc::handler::approval::spawn_approval_await(
                    engine.approval_store.clone(),
                    app.core.memory_arc(),
                    rpc_id,
                    cmd.request.params.clone(),
                    &cmd.response_tx,
                );
                return Some(Intercepted::Answered);
            }
            // 데몬은 자기 IPC 로 멈출 수 있어야 한다. gui 는 `AppEvent::Shutdown` 을
            // winit proxy 로 보내는데 헤드리스엔 proxy 가 없으므로, 호출자에게 성공을
            // 회신한 뒤 **호출자(run loop)에게 break 를 돌려준다.** gui 와 같은
            // debug 격리(`DEBUG_METHODS`)를 유지한다.
            #[cfg(debug_assertions)]
            "system.shutdown" => {
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

/// debug 빌드의 app 층 표면 중 **창이 없어도 답이 정의되는 것.**
///
/// gui 는 이것들을 라우터의 debug step(`src/app/ipc/debug_methods.rs`)에서 처리하는데
/// 그 step 자체가 헤드리스에 없다. 읽는 것은 `App` 의 `lua_engine` / `plugin_manager`
/// 이고 둘 다 feature 게이트가 없는 필드다 — 창을 안 보는데 자리가 없어서 `-32601`
/// 이던 것이라, 에이전트 검증 표면이 헤드리스에서만 사라지는 형태였다
/// (`docs/identity.md` 원칙 2).
///
/// 여기 **없는** debug 메서드(popup · banner · fullscreen · modifier_hint · tool ·
/// inject_* · selection · pending_menu · focused_surface · info · gpu.stall)는 창·
/// 렌더러·egui 입력 큐를 읽는다. 판정은 메서드별로
/// `docs/dev-guide/headless-ipc-surface.md` 에 있다.
#[cfg(debug_assertions)]
fn intercept_debug_app_layer(
    app: &mut App,
    engine: &CoreState,
    cmd: &crate::ipc::server::IpcCommand,
) -> Option<Intercepted> {
    let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
    if cmd.request.method == "debug.lua.eval" {
        let resp = crate::core::app_surface_debug::lua_eval(
            app.lua_engine.as_ref(),
            rpc_id,
            &cmd.request.params,
        );
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 아래 둘은 매니저를 본다. **메타데이터 층까지만** 세운다 — 조회가 plugin
    // 프로세스를 띄우면 관측이 자기 대상을 바꾼다(ADR-0136). 그래서 아직 아무
    // plugin 도 안 뜬 데몬에서는 구독자가 0 으로 나오고, 그것이 그 시점의
    // 사실이다(매니저가 아예 없을 때의 `-32000` 과 구분된다).
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
    // plugin popup **조회**. 매니저만 읽어 창이 없어도 답이 정의된다
    // (gui 와 같은 함수를 부른다).
    //
    // `open`/`close` 는 여기 없다. 컴파일이 막아서가 아니라 — 헤드리스에는
    // plugin popup 을 **닫는 경로가 하나도 없기** 때문이다(debug close 도,
    // plugin 자신의 release `popup.close` 도 gui 게이트 안의 `app::dispatch`
    // 에 산다). open 만 열면 그 빌드에서 닫을 수 없는 인스턴스가 남는다 —
    // 표면을 넓히면서 정리 책임을 새로 지는 형태라 열지 않았다.
    if cmd.request.method == "debug.popup.list" {
        super::headless_plugins::ensure_plugin_manager_metadata(app, engine);
        let resp = crate::ipc::handler::popup::handle_list(app.plugin_manager.as_ref(), rpc_id);
        send_response(&cmd.response_tx, resp);
        return Some(Intercepted::Answered);
    }
    // 등록된 전체화면 무대 **조회**. 읽는 것이 gui 무관 메타 표뿐이라 창이
    // 없어도 답이 정의된다(gui 와 같은 함수를 부른다).
    //
    // 같은 갈래의 `open`/`close`/`state` 는 여기 없다 — 셋 다
    // `pick_debug_window` 로 창을 지목한다. 무대는 창 단위라 창이 없으면 답이
    // 정의되지 않는다. "컴파일되는가" 와 "열어도 되는가" 는 다른 물음이다.
    if cmd.request.method == "debug.fullscreen.list" {
        let resp = crate::core::app_surface_debug::fullscreen_list(rpc_id);
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

/// plugin namespace forward — 넘겼으면 `true`(응답은 plugin 이 준다).
///
/// **소속은 디스크만 읽어 묻고, 기동은 소속이 맞을 때만 한다.**
///
/// 두 층을 나눠 부른다. 먼저 `ensure_plugin_manager_metadata` 는 `~/.tasty/plugins/`
/// 를 스캔해 매니페스트를 읽을 뿐 프로세스를 하나도 안 띄운다. namespace 소유는
/// 그 매니페스트가 선언하는 정적 사실이므로 그것만으로 답이 난다(ADR-0173).
/// 소속이 맞으면 공통 manager forward가 활성 owner와 매칭 IPC hook extension만 준비한다.
///
/// 이 순서가 왜 필요한지는 실측돼 있다(2026-09-05, 설치 끝난 홈): 소속을 묻기 위해
/// 먼저 기동하던 형태에서는 **호스트가 모르는 이름을 한 번 부르는 것만으로**(오타
/// 포함) 그 데몬이 plugin 9 개를 띄웠고 첫 응답이 1272 ms(기동 후 92 ms)였으며 그
/// 프로세스들은 데몬 수명 내내 남았다. 미등록 prefix는 스캔에서 끝난다. 등록 prefix
/// 안의 메서드 오타는 전체 메서드 명부가 없는 기존 계약대로 owner가 판정한다.
///
/// 멱등 키를 실은 **표의** `Mutate`(`image.open` 등)는 넘기기 전에 보존소를 지난다 — gui 라우터와
/// 같은 함수다(ADR-0566). plugin 고유 이름은 거기서 개입하지 않는다(ADR-0361).
#[cfg(not(feature = "gui"))]
fn forward_to_plugin_namespace(
    app: &mut App,
    engine: &CoreState,
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
