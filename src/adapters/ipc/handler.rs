mod agent_children;
mod checked;
#[cfg(all(test, debug_assertions))]
mod cli_entry_debug_tests;
#[cfg(test)]
pub(crate) mod cli_entry_tests;
// 헤드리스에는 attach.into_gui arm이 없다. GUI는 App 층이 먼저 답한다.
#[cfg(all(test, not(feature = "gui")))]
mod attach_into_gui_headless_tests;
mod completion_strategy;
#[cfg(all(debug_assertions, feature = "gui"))]
mod debug;
#[cfg(debug_assertions)]
mod debug_attach;
#[cfg(all(debug_assertions, feature = "gui"))]
mod debug_html_script;
#[cfg(debug_assertions)]
mod debug_nav;
#[cfg(debug_assertions)]
pub(crate) mod debug_plugin;
#[cfg(debug_assertions)]
mod debug_state;
#[cfg(debug_assertions)]
mod debug_terminal;
#[cfg(all(debug_assertions, feature = "gui"))]
mod debug_webview_overlay;
pub(crate) mod deferred;
mod file_handler;
#[cfg(feature = "gui")]
pub(crate) mod file_picker;
pub(crate) mod gate;
#[cfg(test)]
pub(crate) mod gate_faults;
#[cfg(feature = "gui")]
mod git_viewer;
mod hook_handler;
pub(crate) mod idempotency;
#[cfg(test)]
mod intent_order_tests;
// 창 라우터 호출자 검사는 해당 라우터와 같은 GUI 조건에서 실행한다.
#[cfg(all(test, feature = "gui"))]
mod window_router_caller_tests;
// `list_global` 이 두 hook 목록을 합산하므로 크레이트 안에서 보여야 한다.
pub(crate) mod hooks;
// image.list는 호스트의 전 창 합산 조회에서 사용한다.
#[cfg(feature = "gui")]
pub(crate) mod image;
#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]
mod input_source;
#[cfg(feature = "gui")]
mod markdown;
#[cfg(feature = "gui")]
mod markdown_mirror;
mod memory;
mod message;
mod meta;
pub(crate) mod notification;
pub(crate) mod notification_channel;
// 증명을 남기는 file_picker 와 소비하는 markdown 이 GUI 전용이다.
#[cfg(feature = "gui")]
pub(crate) mod picker_proofs;
mod run_result;
// 창별 목록을 호스트에서 합산할 수 있도록 공개한다.
pub(crate) mod journal_pressure;
pub(crate) mod keybinding;
pub(crate) mod output;
pub(crate) mod pane;
pub(crate) mod params;
mod passkey;
pub(crate) mod preset;
mod pressure;
pub(crate) mod pty;
mod recent;
#[cfg(test)]
mod remote_file_tests;
mod remote_profile;
mod settings;
pub(crate) mod surface;
pub(crate) mod tab;
mod telemetry;
pub(crate) mod telemetry_follow_up;
pub(crate) mod terminal;
mod terminal_input;
pub(crate) mod theme;
#[cfg(all(debug_assertions, feature = "gui"))]
mod tool;
mod webhook;
pub(crate) mod webview;
pub(crate) mod workspace;
pub(crate) mod workspace_category;

pub mod agent;
pub mod approval;
pub(crate) mod attach;
pub mod audit;
pub mod events;
#[cfg(all(debug_assertions, feature = "gui"))]
pub mod ime;
pub mod plugin;
// PluginManager 조회는 창 없이도 가능하다. debug 조건은 모듈 안에 있다.
#[cfg(debug_assertions)]
pub mod popup;
pub mod session;

#[cfg(feature = "gui")]
pub(crate) use checked::check_without_engine;
pub(crate) use checked::{CheckedRequest, OwnedCheckedRequest};
pub(crate) use deferred::{Handled, ReplyTo};
#[cfg(test)]
pub(crate) use gate::check_request;

use crate::runtime::engine_access::EngineMut;
use std::borrow::Cow;

use serde_json::json;

use crate::ipc::alias;
use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

/// 이름은 등록돼 있지만 macOS GUI 조건이 맞지 않아 실행할 수 없음을 알린다.
/// 사용처가 debug 라우터이므로 상수도 같은 debug·플랫폼 조건으로 제한한다.
#[cfg(all(debug_assertions, not(all(target_os = "macos", feature = "gui"))))]
const PLATFORM_ONLY_MACOS_GUI: &str = "input reproduction over the OS event stream is macOS-only and needs the gui build \
     (CGEventPost / TISSelectInputSource have no equivalent here)";
use crate::ipc::window_port::IpcWindow;
use crate::state::RequestContext;

/// 호출자를 인증된 종류로 전달하고 공통 권한·cap·rate 검사를 수행한다.
/// 이미 검사한 요청은 handle_checked_request를 사용한다(docs/dev-guide/plugin-permissions.md).
///
/// 엔진 핸들러는 IpcWindow와 IntentOutbox로 창에 접근한다.
/// 창 자체를 조작하는 GUI·debug 핸들러만 RequestContext를 받는다(docs/dev-guide/app-state-ownership.md).
#[cfg(test)]
pub fn handle_with_caller(
    core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &JsonRpcRequest,
    caller: &CallerContext,
) -> JsonRpcResponse {
    match check_request(core, state, engine, request, caller) {
        Ok(checked) => handle_checked_request(core, state, engine, &checked).wait(core),
        Err(response) => response,
    }
}

/// 검사한 요청을 실행하고 소요 시간을 기록한다. 예산과 사용량은 다시 집계하지 않는다.
/// Engine handlers borrow RequestScope values. Returned outputs are applied before a separate GUI/debug route.
/// memory writer 결과를 기다리는 요청은 `Handled::Later`이며 호출자가 원 호출자를 붙여 넘긴다.
pub(crate) fn handle_checked_request(
    core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    checked: &CheckedRequest<'_>,
) -> Handled {
    engine.refresh_attach_presentation(&state.navigation);
    let started = core.now_instant();
    let response = route_checked_request(core, state, engine, checked);
    // 시작과 끝을 같은 Clock으로 재야 주입한 시계와 벽시계가 섞이지 않는다.
    let elapsed = core.now_instant().duration_since(started);
    core.pressure().record_handler(elapsed);
    state.reconcile_presentation(engine);
    engine.refresh_attach_presentation(&state.navigation);
    response
}

/// 조기 반환도 계측되도록 실행 시간은 이 함수 밖에서 기록한다.
fn route_checked_request(
    core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    checked: &CheckedRequest<'_>,
) -> Handled {
    let request = checked.request();
    let caller = checked.caller();
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let (_, routed) = canonicalize_and_route(request);
    let request = routed.as_ref();

    // alias 정규화 뒤 같은 멱등 키를 확인해 재시도는 handler를 다시 실행하지 않는다.
    let pending = match idempotency::begin(core.now_instant(), caller, request, &id) {
        Ok(pending) => pending,
        Err(answer) => return Handled::Now(answer),
    };
    if let Some(stale) = core.memory_replies().take_parked() {
        tracing::error!(id = %stale.id(), "a deferred memory reply was parked outside a request");
    }
    let response = dispatch_routed(core, state, engine, checked, request, id);
    // 핸들러가 writer 결과를 기다리면 멱등 키 기록도 그 결과와 함께 끝낸다.
    if let Some(deferred) = core.memory_replies().take_parked() {
        let pending = idempotency::hold(core.now_instant(), pending);
        return Handled::Later(deferred.with_idempotency(pending));
    }
    idempotency::finish(core.now_instant(), pending, &response);
    Handled::Now(response)
}

/// 모든 반환값을 기록할 수 있도록 멱등 저장소 처리는 이 함수 밖에 둔다.
fn dispatch_routed(
    core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    checked: &CheckedRequest<'_>,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> JsonRpcResponse {
    // 요청에서 생성한 intent를 순서대로 해당 창 큐에 옮긴다.
    let mut scope = crate::ipc::request_scope::RequestScope::capture(
        state,
        engine.core,
        #[cfg(feature = "gui")]
        Some(core.navigation_proofs.clone()),
    );
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    let routed = crate::remote::agent_children::driving_scope(checked.caller(), || {
        route_engine_handler(
            core,
            &mut scope,
            &mut out,
            engine,
            checked,
            request,
            id.clone(),
        )
    });
    scope.enqueue_intents(out);
    let outputs = scope.finish();
    outputs.apply(state, engine.core);
    if let Some(resp) = routed {
        return resp;
    }

    #[cfg(feature = "gui")]
    if let Some(resp) = route_window_handler(
        &core.navigation_proofs,
        state,
        engine,
        checked.caller(),
        request,
        id.clone(),
    ) {
        return resp;
    }

    #[cfg(debug_assertions)]
    if let Some(resp) = route_debug_handler(core, state, engine, request, id.clone()) {
        return resp;
    }

    JsonRpcResponse::unrouted_for_external_caller(id, &request.method)
}

/// alias를 정규화하고 deprecated 이름을 사용한 호출에 경고를 남긴다.
fn canonicalize_and_route(request: &JsonRpcRequest) -> (&str, Cow<'_, JsonRpcRequest>) {
    let canonical = alias::canonicalize(&request.method);
    if alias::is_deprecated(&request.method) {
        tracing::warn!(
            "ipc method '{}' is deprecated; use '{canonical}' (will be removed at 1.0)",
            request.method
        );
    }

    let routed: Cow<JsonRpcRequest> = if canonical == request.method {
        Cow::Borrowed(request)
    } else {
        // alias로 호출해도 멱등 키·세션 토큰·호출자 표시 ID 는 유지한다.
        Cow::Owned(JsonRpcRequest {
            response_timeout_ms: None,
            method: canonical.to_string(),
            ..request.clone()
        })
    };
    (canonical, routed)
}

/// 권한 부족을 감사 기록과 오류로 반환한다.
/// Agent의 권한 부족은 공유 approval store에 승인 요청도 만든다.
pub(crate) fn check_permission_gate(
    core: &mut crate::app::services::AppServices,
    window: &mut dyn IpcWindow,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    canonical: &str,
    workspace_id: Option<u32>,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    if let Err(e) = caller.ensure_allowed(canonical) {
        tracing::warn!("ipc permission denied: {e}");
        let seq = core.telemetry_seq.next();
        crate::ipc::audit::record(
            core,
            caller,
            canonical,
            crate::store::audit::AuditDecision::Deny,
            Some(&format!("{e}")),
            workspace_id,
            seq,
        );
        let mut response =
            JsonRpcResponse::error(id.clone(), -32001, format!("permission_denied: {e}"));
        // 알 수 없는 메서드·plugin 호출 금지는 권한을 추가해도 해결되지 않는다.
        // Plugin 권한은 매니페스트·grant로 관리하므로 Agent 승인 요청을 만들지 않는다.
        if let (
            tasty_ipc::caller::CallerError::MissingPermission { permission, .. },
            CallerContext::Agent { agent_id, .. },
        ) = (&e, caller)
        {
            let perm_token = permission.as_token();
            if let Some(record) = approval::publish_capability_elevation(
                core,
                window,
                engine,
                agent_id,
                canonical,
                &perm_token,
                None,
            ) && let Some(err) = response.error.as_mut()
            {
                err.data = Some(approval::elevation_error_data(
                    &record,
                    &perm_token,
                    canonical,
                ));
            }
        }
        return Some(response);
    }
    None
}

/// Pause·RequireApproval cap이 적용된 agent를 차단한다.
/// Local은 제외하므로 telemetry.cap.reset으로 해제할 수 있다. `caps`는 게이트가 읽은 cap 목록이다.
pub(crate) fn check_cap_gate(
    core: &mut crate::app::services::AppServices,
    caller: &CallerContext,
    canonical: &str,
    workspace_id: Option<u32>,
    id: &serde_json::Value,
    caps: Option<&[tasty_telemetry::CostCap]>,
) -> Option<JsonRpcResponse> {
    if let Some(reason) = telemetry::check_cap_block(caller, caps) {
        tracing::warn!("ipc cap blocked: {reason}");
        let seq = core.telemetry_seq.next();
        crate::ipc::audit::record(
            core,
            caller,
            canonical,
            crate::store::audit::AuditDecision::Deny,
            Some(&format!("cap_blocked: {reason}")),
            workspace_id,
            seq,
        );
        return Some(JsonRpcResponse::error(
            id.clone(),
            -32007,
            format!("cap_blocked: {reason}"),
        ));
    }
    None
}

/// Local과 호스트 호출은 제한하지 않는다.
/// telemetry는 재귀 집계를 피하고 agent.rate_limit은 자가 복구를 위해 제외한다.
/// system.info도 제한 없이 조회할 수 있다.
fn should_rate_limit(caller: &CallerContext, method: &str) -> bool {
    use crate::ipc::caller::CallerContext as C;
    match caller {
        C::Local { .. } => return false,
        C::Agent { .. } if caller.agent_id().is_host() => return false,
        _ => {}
    }
    if method.starts_with("telemetry.") {
        return false;
    }
    if method.starts_with("agent.rate_limit_") {
        return false;
    }
    if method == "system.info" {
        return false;
    }
    true
}

/// Display the already persisted observations in the selected presentation, if one exists.
#[cfg(feature = "gui")]
pub(crate) fn display_plugin_rss_anomalies(
    state: &mut RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    anomalies: &[tasty_telemetry::Anomaly],
) {
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    let mut scope = crate::ipc::request_scope::RequestScope::capture(state, engine, None);
    for anomaly in anomalies {
        telemetry::fire_anomaly_notification(&mut scope, &mut out, engine, anomaly);
    }
    scope.enqueue_intents(out);
    let outputs = scope.finish();
    outputs.apply(state, engine);
}

/// 점유자가 아닌 호출자의 workspace 구조 변경을 IPC 라우터에서 거절한다.
/// holder의 forward는 structural_exec을 직접 호출하므로 이 검사를 거치지 않는다.
/// 공통 도메인 실행부에 검사하면 정당한 forward까지 막힌다(docs/dev-guide/attach-behavior.md).
///
/// terminal.spawn은 pane 재지정 이후의 spawn_target_guard에서 검사한다.
/// convert는 아래 열거한 메서드만 검사하며 GUI의 직접 intent나 새 kind의 진입점은 포함하지 않는다.
/// 대상이 없거나 파라미터가 잘못되면 실제 핸들러가 오류를 반환하도록 넘긴다.
/// 대상을 생략한 preset.apply는 적용 코드와 같게 이 창의 활성 workspace를 대상으로 본다.
pub(crate) fn hard_occupied_structural_guard(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    active_ws_idx: usize,
    method: &str,
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    // 여기서는 소속만 찾고 잘못된 값은 handler의 require_*가 거절하게 한다.
    // 정수를 잘라 변환하면 다른 대상의 ID가 될 수 있으므로 범위를 검사한다.
    let ws_idx: usize = match method {
        "split" => {
            let target_pane = params::read_int::<u32>(params, "target_pane")
                .ok()
                .flatten();
            let target_surface = pane::resolve_surface_target(params);
            target_pane
                .and_then(|pid| engine.find_workspace_index_for_pane(pid))
                .or_else(|| {
                    target_surface
                        .and_then(|sid| engine.find_workspace_index_for_surface(sid))
                        .map(|(i, _)| i)
                })?
        }
        "workspace.close" => {
            if let Some(ws_id) = params::read_int::<u32>(params, "id").ok().flatten() {
                engine
                    .workspaces()
                    .into_iter()
                    .position(|w| w.id == ws_id)?
            } else {
                params::read_int::<usize>(params, "index").ok().flatten()?
            }
        }
        "tab.create" | "pane.close" | "tab.move" | "pty.attach_surface" => {
            let pane_id = params::read_int::<u32>(params, "pane_id").ok().flatten()?;
            engine.find_workspace_index_for_pane(pane_id)?
        }
        "tab.close" => {
            let tab_id = params::read_int::<u32>(params, "tab_id").ok().flatten()?;
            let pane_id = engine.find_pane_for_tab(tab_id)?;
            engine.find_workspace_index_for_pane(pane_id)?
        }
        "surface.close" | "markdown.navigate" | "image.open" => {
            let surface_id = params::read_int::<u32>(params, "surface_id")
                .ok()
                .flatten()?;
            engine
                .find_workspace_index_for_surface(surface_id)
                .map(|(i, _)| i)?
        }
        // workspace preset은 새 workspace를 만들므로 검사하지 않는다.
        "preset.apply" => {
            let active = || {
                engine
                    .workspaces()
                    .len()
                    .checked_sub(1)
                    .map(|last| active_ws_idx.min(last))
            };
            match params.get("kind").and_then(|v| v.as_str())? {
                "tab" => match params::read_int::<u32>(params, "target_pane_id").ok()? {
                    Some(pid) => engine.find_workspace_index_for_pane(pid)?,
                    None => active()?,
                },
                "pane" => match params::read_int::<u32>(params, "target_workspace_id").ok()? {
                    Some(ws_id) => engine.find_workspace_index_for_id(ws_id)?,
                    None => active()?,
                },
                _ => return None,
            }
        }
        _ => return None,
    };
    let ws_id = engine.workspace_at(ws_idx)?.id;
    if engine.live.occupancy.workspace_holder(ws_id).is_some() {
        return Some(hard_occupied_denial(ws_id, id));
    }
    None
}

/// 라우터와 spawn 가드가 같은 점유 거절 응답을 사용한다.
pub(crate) fn hard_occupied_denial(ws_id: u32, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::invalid_params(
        id.clone(),
        format!(
            "Workspace {ws_id} is occupied by a remote attach session (hard-occupied) — \
             structural changes (new tab/split/close/move) must come from that session. \
             Use a different workspace."
        ),
    )
}

/// pane 재지정까지 끝난 실제 대상의 mirror·hard 점유를 생성 전에 검사한다.
/// workspace 인자만 검사하면 다른 workspace의 pane을 지정해 우회할 수 있다.
///
/// spawn은 holder가 forward하는 구조 변경에 포함되지 않아 이 위치에서 거절해도 된다.
/// 나머지 mirror 구조 변경은 원격 전달을 허용하므로 mirror 검사를 공통 라우터에 넣지 않는다(docs/dev-guide/attach-behavior.md).
pub(crate) fn spawn_target_guard(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    pane_id: u32,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    let ws_idx = engine.find_workspace_index_for_pane(pane_id)?;
    let ws = engine.workspace_at(ws_idx)?;
    let ws_id = ws.id;
    if ws.mirror {
        return Some(JsonRpcResponse::invalid_params(
            id.clone(),
            format!(
                "Workspace {ws_id} is a mirror of a remote attach session — child terminals \
                 cannot be spawned into it. Structural changes there are forwarded to the \
                 remote instance and complete asynchronously, so no local child is created. \
                 Use a different workspace, or spawn from the remote instance directly."
            ),
        ));
    }
    if engine.live.occupancy.workspace_holder(ws_id).is_some() {
        return Some(hard_occupied_denial(ws_id, id));
    }
    None
}

fn route_engine_handler(
    core: &mut crate::app::services::AppServices,
    window: &mut dyn IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut EngineMut<'_>,
    checked: &CheckedRequest<'_>,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    let caller = checked.caller();
    if tasty_ipc::method_meta::has_structure_journal_contract(&request.method) {
        return Some(JsonRpcResponse::internal_error(
            id,
            "structural request bypassed journal admission",
        ));
    }
    if let Some(resp) = hard_occupied_structural_guard(
        &engine.as_ref(),
        window.active_workspace_index(engine),
        &request.method,
        &request.params,
        &id,
    ) {
        return Some(resp);
    }
    Some(match request.method.as_str() {
        "system.info" => handle_system_info(window, &engine.read(), id),
        "system.pressure" => pressure::handle_system_pressure(&*core, &engine.as_ref(), id),
        "workspace.list" => workspace::handle_workspace_list(window, &engine.as_ref(), id),
        "workspace_category.list" => {
            workspace_category::handle_list(window.presentation(), engine, id)
        }
        "pane.list" => pane::handle_pane_list(window.presentation(), engine, id),
        "tab.list" => {
            tab::handle_tab_list(window.presentation(), &engine.as_ref(), id, &request.params)
        }
        // terminal: child-terminal 관리와 점유 검사 (docs/dev-guide/attach-behavior.md)
        "terminal.tell" => terminal::handle_tell(engine, id, &request.params),
        "terminal.children" => terminal::handle_children(engine, id, &request.params),
        "terminal.parent" => terminal::handle_parent(engine, id, &request.params),
        "terminal.state" => terminal::handle_state(engine, id, &request.params),
        "terminal.respawn" => terminal::handle_respawn(engine, id, &request.params),
        "terminal.broadcast" => terminal::handle_broadcast(engine, id, &request.params),
        "terminal.set_state" => terminal::handle_set_state(core, engine, id, &request.params),
        "terminal.adopt" => terminal::handle_adopt(core, engine, id, &request.params),
        "terminal.release" => terminal::handle_release(core, engine, id, &request.params),
        // Surface에 연결하지 않은 백그라운드 PTY.
        "pty.spawn" => pty::handle_spawn(core, engine, caller, id, &request.params),
        "pty.write" => pty::handle_write(engine, id, &request.params),
        "pty.read" => pty::handle_read(engine, id, &request.params),
        "pty.wait" => pty::handle_wait(engine, id, &request.params),
        "pty.kill" => pty::handle_kill(engine, id, &request.params),
        "pty.list" => pty::handle_list(engine, id),
        // preset (layout preset CRUD + apply)
        "preset.list" => preset::handle_list(core, id, &request.params),
        "preset.get" => preset::handle_get(core, id, &request.params),
        "preset.save" => preset::handle_save(core, id, &request.params),
        "preset.delete" => preset::handle_delete(core, id, &request.params),
        "preset.rename" => preset::handle_rename(core, id, &request.params),
        "preset.capture" => preset::handle_capture(
            window.presentation(),
            core,
            &engine.as_ref(),
            id,
            &request.params,
        ),
        "surface.list" => surface::handle_surface_list(&engine.as_ref(), id),
        "surface.kinds" => surface::handle_surface_kinds(&engine.read(), id),
        "surface.send" => surface::handle_surface_send(core, engine, id, &request.params),
        "surface.send_key" => surface::handle_surface_send_key(core, engine, id, &request.params),
        "surface.send_combo" => {
            surface::handle_surface_send_combo(core, engine, id, &request.params)
        }
        "surface.send_to" => surface::handle_surface_send_to(core, engine, id, &request.params),
        "surface.set_mark" => {
            surface::remote_read::set_mark(core, out, engine, id, &request.params)
        }
        "surface.completion" => surface::handle_completion(out, engine, id, &request.params),
        "surface.attention.get" => {
            surface::handle_attention_get(&engine.as_ref(), id, &request.params)
        }
        "surface.attention.clear" => {
            surface::handle_attention_clear(out, engine, id, &request.params)
        }
        "surface.read_since_mark" => {
            surface::remote_read::read_since_mark(core, engine, id, &request.params)
        }
        "surface.read_since_scan_mark" => {
            surface::handle_read_since_scan_mark(engine, id, &request.params)
        }
        "surface.parse_since_mark" => surface::handle_parse_since_mark(engine, id, &request.params),
        "surface.commands" => surface::handle_commands(core, engine, id, &request.params),
        "surface.last_command" => surface::handle_last_command(core, engine, id, &request.params),
        "surface.command_at" => surface::handle_command_at(core, engine, id, &request.params),
        "output.observe_start" => output::handle_observe_start(core, engine, id, &request.params),
        "output.observe_stop" => output::handle_observe_stop(core, engine, id, &request.params),
        "output.observe_list" => output::handle_observe_list(core, &engine.as_ref(), id),
        "output.observe_info" => {
            output::handle_observe_info(core, &engine.as_ref(), id, &request.params)
        }
        "surface.screen_text" => {
            surface::remote_read::screen_text(core, engine, id, &request.params)
        }
        "surface.cursor_position" => {
            surface::handle_cursor_position(&engine.as_ref(), id, &request.params)
        }
        "surface.mouse_tracking" => {
            surface::handle_mouse_tracking(&engine.as_ref(), id, &request.params)
        }
        "surface.foreground_process" => {
            surface::handle_foreground_process(&engine.as_ref(), id, &request.params)
        }
        "surface.locate" => surface::handle_surface_locate(engine, id, &request.params),
        "surface.is_typing" => handle_is_typing(&engine.as_ref(), id, &request.params),
        "surface.send_wait_idle" => handle_send_wait_idle(engine, id, &request.params),
        "surface.fire_hook" => {
            hooks::handle_surface_fire_hook(core, window, engine, id, &request.params)
        }
        "surface.meta.set" => meta::relayed(engine, &request.params, true)
            .unwrap_or_else(|| meta::handle_surface_meta_set(core, engine, id, &request.params)),
        "surface.meta.get" => meta::handle_surface_meta_get(core, engine, id, &request.params),
        "surface.meta.unset" => meta::relayed(engine, &request.params, false)
            .unwrap_or_else(|| meta::handle_surface_meta_unset(core, engine, id, &request.params)),
        "surface.meta.list" => meta::handle_surface_meta_list(core, engine, id, &request.params),
        "surface.set_cwd" => surface::handle_set_cwd(&engine.as_ref(), id, &request.params),
        "hook.set" => hooks::handle_hook_set(core, engine, id, &request.params),
        "hook.list" => hooks::handle_hook_list(&engine.as_ref(), id, &request.params),
        "hook.unset" => hooks::handle_hook_unset(core, engine, id, &request.params),
        "global_hook.set" => hooks::handle_global_hook_set(core, engine, id, &request.params),
        "global_hook.list" => hooks::handle_global_hook_list(&engine.as_ref(), id),
        "global_hook.unset" => hooks::handle_global_hook_unset(core, engine, id, &request.params),
        "webhook.register" => webhook::handle_register(core, caller, id, &request.params),
        "webhook.list" => webhook::handle_list(core, id),
        "webhook.info" => webhook::handle_info(core, id, &request.params),
        "webhook.unregister" => webhook::handle_unregister(core, id, &request.params),
        "webhook.sweep" => webhook::handle_sweep(core, id),
        "webhook.config" => webhook::handle_config(core, out, &engine.read(), id, &request.params),
        "webview.set_url" => webview::handle_set_url(&engine.as_ref(), caller, id, &request.params),
        #[cfg(feature = "gui")]
        "webview.post_message" => webview::post_message(engine, caller, id, request),
        #[cfg(feature = "gui")]
        "surface.html_script" => surface::handle_html_script(&engine.as_ref(), id, &request.params),
        #[cfg(feature = "gui")]
        "surface.html_script_allow" => {
            surface::handle_html_script_allow(&engine.as_ref(), id, &request.params)
        }
        #[cfg(feature = "gui")]
        "surface.html_remote_allow" => {
            surface::handle_html_remote_allow(&engine.as_ref(), id, &request.params)
        }
        #[cfg(feature = "gui")]
        "surface.html_nav" => surface::handle_html_nav(&engine.as_ref(), id, &request.params),
        #[cfg(feature = "gui")]
        "surface.html_back" => surface::handle_html_history(
            window,
            &engine.as_ref(),
            id,
            &request.params,
            crate::webview::WebViewHistoryStep::Back,
        ),
        #[cfg(feature = "gui")]
        "surface.html_forward" => surface::handle_html_history(
            window,
            &engine.as_ref(),
            id,
            &request.params,
            crate::webview::WebViewHistoryStep::Forward,
        ),
        #[cfg(feature = "gui")]
        "surface.html_reload" => surface::handle_html_history(
            window,
            &engine.as_ref(),
            id,
            &request.params,
            crate::webview::WebViewHistoryStep::Reload,
        ),
        // WebView는 surface.set_context를 받지 않아 이 조회로 Theme를 읽는다.
        "theme.query" => theme::handle_query(&engine.read(), id),
        // WebView 페이지가 직접 처리하는 동작(find 등)의 조합을 문서에 넣을 때 읽는다.
        "settings.get_keybinding" => keybinding::handle_query(&engine.read(), id, &request.params),
        "tree" => handle_tree(window, &engine.read(), id),
        "message.send" => message::handle_message_send(core, engine, id, &request.params),
        "message.read" => message::handle_message_read(core, engine, id, &request.params),
        "message.count" => message::handle_message_count(engine, id, &request.params),
        "message.clear" => message::handle_message_clear(core, engine, id, &request.params),
        "notification.counts" | "notification.read" => notification_channel::handle(
            engine,
            id,
            &request.params,
            request.method == "notification.read",
        ),
        "notification.list" => notification::handle_notification_list(&engine.as_ref(), id),
        "notification.create" => {
            notification::handle_notification_create(out, engine, id, &request.params)
        }
        "file_handler.reload" => file_handler::handle_reload(core, id),
        "file_handler.detectors" => file_handler::handle_detectors(core, id),
        // identify worker와 결과를 여는 창이 GUI에만 있다.
        // 헤드리스에서는 예약 성공 뒤 요청을 버리지 않도록 라우팅하지 않는다(docs/features/file-handler/index.md).
        #[cfg(feature = "gui")]
        "file_handler.dispatch" => file_handler::handle_dispatch(
            out,
            window,
            &engine.as_ref(),
            caller,
            id,
            request.params.clone(),
        ),
        "hook_handler.list" => hook_handler::handle_list(id),
        "hook_handler.get" => hook_handler::handle_get(id, &request.params),
        "hook_handler.upsert" => hook_handler::handle_upsert(core, id, &request.params),
        "hook_handler.remove" => hook_handler::handle_remove(core, id, &request.params),
        "hook_handler.reload" => hook_handler::handle_reload(core, id),
        "hook_handler.dispatch" => hook_handler::handle_dispatch(core, id, &request.params),
        "hook_handler.result" => hook_handler::handle_result(id, &request.params),
        "run.result" => run_result::handle(id, caller, &request.params),
        "completion_strategy.list" => completion_strategy::handle_list(id),
        #[cfg(feature = "gui")]
        "markdown.navigate" => markdown::handle_navigate(
            out,
            window,
            &engine.as_ref(),
            caller,
            id,
            request.params.clone(),
        ),
        // kind에 상관없이 최근 목록만 조회하므로 GUI가 필요 없다.
        "recent.query" => recent::handle_query(window, id, request.params.clone()),
        // 결과를 전달하는 App::dispatch_pending_git_query_forwards가 GUI 전용이다(docs/features/remote-attach/index.md).
        #[cfg(feature = "gui")]
        "git_viewer.query" => git_viewer::handle_query(
            engine,
            id,
            &request.params,
            matches!(caller, tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } if plugin_id == "com.tasty.git-viewer"),
        ),
        // mirror 원문도 GUI의 전달 큐에서 처리한다(docs/features/remote-attach/index.md).
        #[cfg(feature = "gui")]
        "markdown_mirror.content_request" => markdown_mirror::handle_content_request(
            engine,
            id,
            &request.params,
            matches!(caller, tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } if plugin_id == "com.tasty.markdown"),
        ),
        // host는 surface 변환·목록만 처리하고 픽셀 편집은 plugin이 처리한다.
        #[cfg(feature = "gui")]
        "image.open" => {
            JsonRpcResponse::internal_error(id, "host conversion bypassed journal admission")
        }
        #[cfg(feature = "gui")]
        "image.list" => image::handle_list(&engine.as_ref(), id),
        "memory.put" => memory::handle_put(core, engine, caller, id, &request.params),
        "memory.get" => memory::handle_get(core, engine, caller, id, &request.params),
        "memory.delete" => memory::handle_delete(core, engine, caller, id, &request.params),
        "memory.list" => memory::handle_list(core, engine, caller, id, &request.params),
        "memory.exists" => memory::handle_exists(core, engine, caller, id, &request.params),
        "memory.count" => memory::handle_count(core, engine, caller, id, &request.params),
        "memory.scopes" => memory::handle_scopes(core, engine, caller, id, &request.params),
        "memory.stats" => memory::handle_stats(core, engine, caller, id, &request.params),
        "memory.query" => memory::handle_query(core, engine, caller, id, &request.params),
        "memory.export" => memory::handle_export(core, engine, caller, id, &request.params),
        "memory.import" => memory::handle_import(
            core,
            engine,
            checked.open_surfaces(),
            caller,
            id,
            &request.params,
        ),
        "memory.secret.put" => memory::handle_secret_put(core, engine, caller, id, &request.params),
        "memory.secret.get" => memory::handle_secret_get(core, engine, caller, id, &request.params),
        "memory.secret.delete" => {
            memory::handle_secret_delete(core, engine, caller, id, &request.params)
        }
        "memory.secret.list" => {
            memory::handle_secret_list(core, engine, caller, id, &request.params)
        }
        "memory.secret.exists" => {
            memory::handle_secret_exists(core, engine, caller, id, &request.params)
        }
        "memory.secret.count" => {
            memory::handle_secret_count(core, engine, caller, id, &request.params)
        }
        "memory.secret.scopes" => {
            memory::handle_secret_scopes(core, engine, caller, id, &request.params)
        }
        "memory.secret.stats" => {
            memory::handle_secret_stats(core, engine, caller, id, &request.params)
        }
        "memory.gc" => memory::handle_gc(core, engine, caller, id, &request.params),
        "memory.bb_create" => memory::handle_bb_create(core, engine, caller, id, &request.params),
        "memory.bb_put" => memory::handle_bb_put(core, engine, caller, id, &request.params),
        "memory.bb_get" => memory::handle_bb_get(core, engine, caller, id, &request.params),
        "memory.bb_get_all" => memory::handle_bb_get_all(core, engine, caller, id, &request.params),
        "memory.bb_get_meta" => {
            memory::handle_bb_get_meta(core, engine, caller, id, &request.params)
        }
        "memory.bb_delete_field" => {
            memory::handle_bb_delete_field(core, engine, caller, id, &request.params)
        }
        "memory.bb_delete" => memory::handle_bb_delete(core, engine, caller, id, &request.params),
        "memory.bb_list" => memory::handle_bb_list(core, engine, caller, id, &request.params),
        "memory.bb_exists" => memory::handle_bb_exists(core, engine, caller, id, &request.params),
        "memory.bb_snapshot" => {
            memory::handle_bb_snapshot(core, engine, caller, id, &request.params)
        }
        "memory.bb_snapshot_get" => {
            memory::handle_bb_snapshot_get(core, engine, caller, id, &request.params)
        }
        "memory.bb_snapshot_list" => {
            memory::handle_bb_snapshot_list(core, engine, caller, id, &request.params)
        }
        "memory.bb_snapshot_delete" => {
            memory::handle_bb_snapshot_delete(core, engine, caller, id, &request.params)
        }
        "memory.bb_snapshot_restore" => {
            memory::handle_bb_snapshot_restore(core, engine, caller, id, &request.params)
        }
        "memory.plan_create" => {
            memory::handle_plan_create(core, engine, caller, id, &request.params)
        }
        "memory.plan_get" => memory::handle_plan_get(core, engine, caller, id, &request.params),
        "memory.plan_list" => memory::handle_plan_list(core, engine, caller, id, &request.params),
        "memory.plan_delete" => {
            memory::handle_plan_delete(core, engine, caller, id, &request.params)
        }
        "memory.plan_add_step" => {
            memory::handle_plan_add_step(core, engine, caller, id, &request.params)
        }
        "memory.plan_remove_step" => {
            memory::handle_plan_remove_step(core, engine, caller, id, &request.params)
        }
        "memory.plan_update_step" => {
            memory::handle_plan_update_step(core, engine, caller, id, &request.params)
        }
        "memory.cache_put" => memory::handle_cache_put(core, engine, caller, id, &request.params),
        "memory.cache_get" => memory::handle_cache_get(core, engine, caller, id, &request.params),
        "memory.cache_invalidate" => {
            memory::handle_cache_invalidate(core, engine, caller, id, &request.params)
        }
        "memory.cache_clear" => {
            memory::handle_cache_clear(core, engine, caller, id, &request.params)
        }
        "memory.cache_list" => memory::handle_cache_list(core, engine, caller, id, &request.params),
        "memory.goal_set" => memory::handle_goal_set(core, engine, caller, id, &request.params),
        "memory.goal_get" => memory::handle_goal_get(core, engine, caller, id, &request.params),
        "memory.goal_clear" => memory::handle_goal_clear(core, engine, caller, id, &request.params),
        "settings.get_plugin_setting" => {
            settings::handle_get_plugin_setting(&engine.read(), caller, id, &request.params)
        }
        "settings.get_remote_transfer" => settings::handle_get_remote_transfer(&engine.read(), id),
        "settings.get_input_rules" => terminal_input::get(&engine.read(), id),
        "settings.set_input_rule"
        | "settings.remove_input_rule"
        | "settings.initialize_input_rule" => terminal_input::handle_input_rule_update(
            core,
            out,
            caller,
            id,
            &request.params,
            &request.method,
        ),
        "settings.set_remote_transfer" => {
            settings::handle_set_remote_transfer(core, out, id, &request.params)
        }
        // approval.await는 별도 워커에서 대기한다.
        "approval.request" => {
            approval::handle_request(core, window, engine, caller, id, &request.params)
        }
        "approval.respond" => approval::handle_respond(core, engine, caller, id, &request.params),
        "approval.cancel" => approval::handle_cancel(core, engine, caller, id, &request.params),
        "approval.get" => approval::handle_get(core, engine, caller, id, &request.params),
        "approval.list" => approval::handle_list(core, engine, caller, id, &request.params),
        "approval.history" => approval::handle_history(core, engine, caller, id, &request.params),
        "approval.summary.set" => {
            approval::handle_summary_set(core, engine, caller, id, &request.params)
        }
        "approval.summary.get" => {
            approval::handle_summary_get(core, engine, caller, id, &request.params)
        }
        "telemetry.record" => {
            telemetry::handle_record(core, window, engine, caller, id, &request.params)
        }
        "telemetry.record_batch" => {
            telemetry::handle_record_batch(core, window, engine, caller, id, &request.params)
        }
        "telemetry.summary" => telemetry::handle_summary(core, engine, caller, id, &request.params),
        "telemetry.timeseries" => {
            telemetry::handle_timeseries(core, engine, caller, id, &request.params)
        }
        "telemetry.top" => telemetry::handle_top(core, engine, caller, id, &request.params),
        "telemetry.cap.set" => telemetry::handle_cap_set(core, engine, caller, id, &request.params),
        "telemetry.cap.list" => {
            telemetry::handle_cap_list(core, engine, caller, id, &request.params)
        }
        "telemetry.cap.remove" => {
            telemetry::handle_cap_remove(core, engine, caller, id, &request.params)
        }
        "telemetry.cap.status" => {
            telemetry::handle_cap_status(core, engine, caller, id, &request.params)
        }
        "telemetry.cap.reset" => {
            telemetry::handle_cap_reset(core, engine, caller, id, &request.params)
        }
        "telemetry.anomaly.list" => {
            telemetry::handle_anomaly_list(core, engine, caller, id, &request.params)
        }
        "telemetry.session_summary" => {
            telemetry::handle_session_summary(core, engine, caller, id, &request.params)
        }
        "agent.task_create" => agent::handle_task_create(core, engine, caller, id, &request.params),
        "agent.task_list" => agent::handle_task_list(core, engine, caller, id, &request.params),
        "agent.task_get" => agent::handle_task_get(core, engine, caller, id, &request.params),
        // agent.task_await는 GUI·헤드리스 모두 상위 라우터가 별도 워커로 처리한다.
        "agent.task_cancel" => agent::handle_task_cancel(core, engine, caller, id, &request.params),
        "agent.task_retry" => agent::handle_task_retry(core, engine, caller, id, &request.params),
        "agent.task_graph" => agent::handle_task_graph(core, engine, caller, id, &request.params),
        "agent.task_graph_validate" => {
            agent::handle_task_graph_validate(core, engine, caller, id, &request.params)
        }
        "agent.task_graph_submit" => {
            agent::handle_task_graph_submit(core, engine, caller, id, &request.params)
        }
        "agent.dag_list" => agent::handle_dag_list(core, engine, caller, id, &request.params),
        "agent.dag_get" => agent::handle_dag_get(core, engine, caller, id, &request.params),
        "agent.dag_report" => agent::handle_dag_report(core, engine, caller, id, &request.params),
        "agent.report_append" => {
            agent::handle_report_append(core, engine, caller, id, &request.params)
        }
        "agent.task_set_result" => {
            agent::handle_task_set_result(core, engine, caller, id, &request.params)
        }
        "agent.task_submit_result" => agent::task_submit(core, engine, caller, id, &request.params),
        "agent.task_turn_report" => agent::task_turn_report(core, caller, id, &request.params),
        "agent.task_run" => agent::handle_task_run(core, engine, caller, id, &request.params),
        "agent.task_delete" => agent::handle_task_delete(core, engine, caller, id, &request.params),
        "agent.task_purge" => agent::handle_task_purge(core, engine, caller, id, &request.params),
        "agent.barrier_create" => {
            agent::handle_barrier_create(core, engine, caller, id, &request.params)
        }
        "agent.barrier_signal" => {
            agent::handle_barrier_signal(core, engine, caller, id, &request.params)
        }
        "agent.barrier_await" => {
            agent::handle_barrier_await(core, engine, caller, id, &request.params)
        }
        "agent.barrier_state" => {
            agent::handle_barrier_state(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_create" => {
            agent::handle_semaphore_create(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_set_permits" => {
            agent::handle_semaphore_set_permits(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_acquire" => {
            agent::handle_semaphore_acquire(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_release" => {
            agent::handle_semaphore_release(core, engine, caller, id, &request.params)
        }
        "agent.barrier_list" => {
            agent::handle_barrier_list(core, engine, caller, id, &request.params)
        }
        "agent.barrier_delete" => {
            agent::handle_barrier_delete(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_list" => {
            agent::handle_semaphore_list(core, engine, caller, id, &request.params)
        }
        "agent.semaphore_delete" => {
            agent::handle_semaphore_delete(core, engine, caller, id, &request.params)
        }
        "agent.lease_acquire" => {
            agent::handle_lease_acquire(core, engine, caller, id, &request.params)
        }
        "agent.lease_release" => {
            agent::handle_lease_release(core, engine, caller, id, &request.params)
        }
        "agent.lease_list" => agent::handle_lease_list(core, engine, caller, id, &request.params),
        "agent.task_reduce" => agent::handle_task_reduce(core, engine, caller, id, &request.params),
        "agent.rate_limit_set" => {
            agent::handle_rate_limit_set(core, engine, caller, id, &request.params)
        }
        "agent.rate_limit_list" => {
            agent::handle_rate_limit_list(core, engine, caller, id, &request.params)
        }
        "agent.rate_limit_remove" => {
            agent::handle_rate_limit_remove(core, engine, caller, id, &request.params)
        }
        "agent.rate_limit_status" => {
            agent::handle_rate_limit_status(core, engine, caller, id, &request.params)
        }
        "session.issue" => session::handle_issue(core, caller, id, &request.params),
        "session.revoke" => session::handle_revoke(core, id, &request.params),
        "session.list" => session::handle_list(core, id),
        "attach.acquire" => attach::handle_acquire(engine, id, &request.params),
        "attach.release" => attach::handle_release(engine, id, &request.params),
        // attach.into_gui는 GUI App 층이 답한다. 헤드리스에는 mirror 큐를 처리할 쪽이 없으므로
        // 접수하지 않고 미라우팅 응답(-32017, 메서드 이름 포함)으로 거절한다.
        "attach.list" => attach::handle_list(&engine.read(), id),
        "attach.agent_call" => {
            agent_children::handle_call(core, engine, caller, id, &request.params)
        }
        "attach.agent_target"
        | "attach.agent_commit"
        | "attach.child_notice"
        | "attach.child_notices" => agent_children::handle_agent_method(
            engine,
            caller,
            id,
            request.method.as_str(),
            &request.params,
        ),
        "remote.profile.list" => remote_profile::handle_list(id),
        "remote.profile.get" => remote_profile::handle_get(id, &request.params),
        "remote.profile.add" => remote_profile::handle_add(core, id, &request.params),
        "remote.profile.detect" => remote_profile::handle_detect(core, id, &request.params),
        "remote.profile.remove" => remote_profile::handle_remove(core, id, &request.params),
        // SSH를 실행하지 않고 로컬 설정 파일을 읽는다.
        "remote.profile.list_local" => remote_profile::handle_list_local(id),
        "remote.profile.import" => remote_profile::handle_import(core, id, &request.params),
        // 자격증명 조회는 비밀 값과 파일 경로를 반환하지 않는다.
        "remote.passkey.list" => passkey::handle_list(id),
        "remote.passkey.get" => passkey::handle_get(id, &request.params),
        "remote.passkey.add" => passkey::handle_add(core, id, &request.params),
        "remote.passkey.remove" => passkey::handle_remove(core, id, &request.params),
        _ => return None,
    })
}

/// Explicit GUI handlers receive View state after the engine handler phase has ended.
/// window_router_caller_tests는 아래 match 팔과 호출자 명부를 대조한다.
/// match 밖의 분기는 검사에서 빠질 수 있으므로 새 진입점의 호출자 제한도 직접 확인한다.
#[cfg(feature = "gui")]
fn route_window_handler(
    navigation_proofs: &crate::app::html_runtime::NavigationProofs,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    Some(match request.method.as_str() {
        "file_picker.trigger" => file_picker::handle_trigger_with_proofs(
            state,
            engine,
            caller,
            id,
            &request.params,
            Some(navigation_proofs),
        ),
        _ => return None,
    })
}

#[cfg(debug_assertions)]
fn route_debug_handler(
    core: &crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    Some(match request.method.as_str() {
        "ui.state" => debug_state::handle_ui_state(state, engine, id),
        // 설정 적용은 창이 없어도 동작하므로 debug_state에 둔다.
        "debug.settings.apply" => {
            debug_state::handle_debug_settings_apply(core, state, engine, id, &request.params)
        }
        // present를 막아 이벤트 루프 정지와 watchdog 진단을 재현한다.
        #[cfg(feature = "gui")]
        "debug.gpu.stall" => debug::handle_debug_gpu_stall(id, &request.params),
        // 터미널 그리드 조회·수정은 헤드리스 debug에서도 제공한다.
        "debug.cell_info" => {
            debug_terminal::handle_debug_cell_info(&engine.as_ref(), id, &request.params)
        }
        "debug.screen_attrs" => {
            debug_terminal::handle_debug_screen_attrs(&engine.as_ref(), id, &request.params)
        }
        "debug.glyph_color" => {
            debug_terminal::handle_debug_glyph_color(&engine.as_ref(), id, &request.params)
        }
        "debug.feed_bytes" => debug_terminal::handle_debug_feed_bytes(engine, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.inject_mouse" => debug::handle_debug_inject_mouse(engine, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.inject_key" => debug::handle_debug_inject_key(engine, id, &request.params),
        // surface 이름을 쓰지만 CGEvent/TIS는 OS 전역에 작용한다. debug로 제한한다(docs/dev-guide/plugin-permissions.md).
        #[cfg(all(target_os = "macos", feature = "gui"))]
        "surface.switch_input_source" => {
            input_source::handle_switch_input_source(state, &engine.as_ref(), id, &request.params)
        }
        #[cfg(all(target_os = "macos", feature = "gui"))]
        "surface.raw_key" => {
            input_source::handle_raw_key(state, &engine.as_ref(), id, &request.params)
        }
        // CLI와 메서드 표에는 같은 이름이 있으므로 오타 대신 플랫폼 미지원 오류를 반환한다.
        #[cfg(not(all(target_os = "macos", feature = "gui")))]
        "surface.switch_input_source" | "surface.raw_key" => {
            JsonRpcResponse::error(id.clone(), -32015, PLATFORM_ONLY_MACOS_GUI)
        }
        "debug.close_workspace" => {
            debug_nav::handle_debug_close_workspace(state, engine, id, &request.params)
        }
        "debug.switch_workspace" => {
            debug_nav::handle_debug_switch_workspace(state, engine, id, &request.params)
        }
        "debug.switch_tab" => {
            debug_nav::handle_debug_switch_tab(state, engine, id, &request.params)
        }
        "debug.attach.force_detach" => {
            debug_attach::handle_force_detach(engine, id, &request.params)
        }
        "debug.attach.force_detach_workspace" => {
            debug_attach::handle_force_detach_workspace(engine, id, &request.params)
        }
        #[cfg(feature = "gui")]
        "debug.tool.list" => tool::handle_list(state, engine, id),
        #[cfg(feature = "gui")]
        "debug.tool.invoke" => tool::handle_invoke(state, engine, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.host_popup.list" => debug::handle_debug_host_popup_list(state, id),
        #[cfg(feature = "gui")]
        "debug.host_popup.open" => {
            debug::handle_debug_host_popup_open(state, engine, id, &request.params)
        }
        #[cfg(feature = "gui")]
        "debug.host_popup.close" => {
            debug::handle_debug_host_popup_close(state, id, &request.params)
        }
        #[cfg(feature = "gui")]
        "debug.modifier_hint.hold" => {
            debug::handle_debug_modhint_hold(state, &engine.as_ref(), id, &request.params)
        }
        #[cfg(feature = "gui")]
        "debug.modifier_hint.state" => {
            debug::handle_debug_modhint_state(state, &engine.as_ref(), id)
        }
        #[cfg(feature = "gui")]
        "debug.banner.list" => debug::handle_debug_banner_list(state, id),
        #[cfg(feature = "gui")]
        "debug.banner.show" => debug::handle_debug_banner_show(state, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.banner.close" => debug::handle_debug_banner_close(state, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.banner.set_countdown" => {
            debug::handle_debug_banner_set_countdown(state, id, &request.params)
        }
        // 사용자의 페이지 이동 조작을 재현한다(docs/surfaces/html/index.md).
        #[cfg(feature = "gui")]
        "debug.webview.history" => {
            debug_html_script::handle_history(state, &engine.as_ref(), id, &request.params)
        }
        // native WebView 오버레이 모드 조회·지원 모드 강제(docs/design/systems/webview.md).
        #[cfg(feature = "gui")]
        "debug.webview.overlay" => {
            debug_webview_overlay::handle_overlay(state, id, &request.params)
        }
        _ => return None,
    })
}

/// 필수 surface_id의 타입·u32 범위·Surface ID 공간을 검사한다.
/// PTY_ID_BASE 이상의 값이 memory scope에 들어가면 다음 부팅의 surface 카운터를
/// PTY 공간으로 올릴 수 있으므로 거절한다(docs/design/policies/focus.md).
pub(super) fn require_surface_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    let raw = params::require_u32(params, "surface_id", id)?;
    if !crate::runtime::terminal_store::is_surface_id_space(raw) {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("'surface_id' {raw} is inside the headless PTY id space"),
        ));
    }
    Ok(raw)
}

fn require_pane_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    params::require_u32(params, "pane_id", id)
}

/// 알림을 돌려줄 caller_surface_id는 부가 정보다.
/// 범위는 검사하되 잘못된 값 때문에 본 요청을 거절하지는 않는다.
pub(crate) fn caller_surface_id(params: &serde_json::Value) -> Option<u32> {
    params::read_int::<u32>(params, "caller_surface_id")
        .ok()
        .flatten()
}

/// 서버 capability는 창별로 재사용하는 system_info_fields가 아닌 이 응답에만 추가한다.
fn handle_system_info(
    window: &dyn IpcWindow,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let mut info = system_info_fields(engine, window.active_workspace_index(engine));
    info["capabilities"] = tasty_ipc::capability::capabilities_json();
    // 프로세스마다 다르다. SSH 너머의 상대가 이 프로세스 자신인지 판정할 때 쓴다.
    info["instance_id"] = tasty_ipc::instance::instance_id().into();
    // 재시도 가능 여부를 판단할 수 있도록 보존 시간·개수·응답 크기도 제공한다.
    info["idempotency"] = idempotency::declaration();
    JsonRpcResponse::success(id, info)
}

/// Version is process-wide; the legacy count/index describe this engine. Include
/// its workspace IDs so an observation never silently looks like a global count.
/// window.list reuses the same fields beside the OS window ID.
pub(crate) fn system_info_fields(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    active_workspace: usize,
) -> serde_json::Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "scope": "engine",
        "layout_slot": engine.layout_slot,
        "workspace_count": engine.workspaces().len(),
        "workspace_ids": engine.workspaces().into_iter().map(|ws| ws.id).collect::<Vec<_>>(),
        "active_workspace": active_workspace,
        "active_workspace_id": engine.workspace_at(active_workspace).map(|ws| ws.id),
    })
}

fn handle_tree(
    window: &dyn IpcWindow,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!(build_engine_tree(
            window.presentation(),
            window.active_workspace_index(engine),
            engine
        )),
    )
}

/// IPC와 Lua 스냅샷이 같은 트리 필드를 사용하도록 공통 JSON을 만든다.
pub(crate) fn build_engine_tree(
    presentation: &dyn crate::model::StructurePresentation,
    active_workspace: usize,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Vec<serde_json::Value> {
    engine
        .workspaces()
        .into_iter()
        .enumerate()
        .map(|(i, ws)| {
            let mut t = ws.to_tree_json(&engine.observed_presentation(presentation), &|id| {
                engine
                    .find_surface_by_id(id)
                    .map(|surface| surface.to_tree_json())
                    .unwrap_or_else(|| serde_json::json!({"id":id,"type":"Pending"}))
            });
            t["active"] = json!(i == active_workspace);
            t["busy_count"] = json!(engine.busy_count(&ws.all_surface_ids()));
            annotate_tree_busy(&mut t, engine);
            t
        })
        .collect()
}

/// Walk a workspace tree JSON value and annotate every node that owns surface
/// ids with a `busy_count` field. Surface-leaf nodes also get a `busy` boolean.
fn annotate_tree_busy(
    node: &mut serde_json::Value,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if let Some(obj) = node.as_object_mut() {
        // Surface leaf: has "id" but no "tabs"/"panes"/"first"/"second"
        let is_leaf = !obj.contains_key("tabs")
            && !obj.contains_key("panes")
            && !obj.contains_key("first")
            && !obj.contains_key("second")
            && obj.get("id").is_some();
        if is_leaf {
            if let Some(sid) = obj.get("id").and_then(|v| v.as_u64()) {
                obj.insert("busy".into(), json!(engine.is_surface_busy(sid as u32)));
            }
            return;
        }

        for key in ["panes", "tabs"] {
            if let Some(arr) = obj.get_mut(key).and_then(|v| v.as_array_mut()) {
                for child in arr.iter_mut() {
                    annotate_tree_busy(child, engine);
                }
            }
        }
        for key in ["first", "second", "surface"] {
            if let Some(child) = obj.get_mut(key) {
                annotate_tree_busy(child, engine);
            }
        }

        let mut count: u64 = 0;
        for key in ["panes", "tabs"] {
            if let Some(arr) = obj.get(key).and_then(|v| v.as_array()) {
                for child in arr {
                    count += child
                        .get("busy_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if child.get("busy").and_then(|v| v.as_bool()).unwrap_or(false)
                        && child.get("busy_count").is_none()
                    {
                        count += 1;
                    }
                }
            }
        }
        for key in ["first", "second", "surface"] {
            if let Some(child) = obj.get(key) {
                count += child
                    .get("busy_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if child.get("busy").and_then(|v| v.as_bool()).unwrap_or(false)
                    && child.get("busy_count").is_none()
                {
                    count += 1;
                }
            }
        }
        // Workspaces already had busy_count set by the caller; only insert if missing.
        if !obj.contains_key("busy_count") {
            obj.insert("busy_count".into(), json!(count));
        }
    }
}

fn handle_is_typing(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let typing = engine.is_typing(surface_id);
    let idle_seconds = if let Some(last) = engine.live.last_key_input.get(&surface_id) {
        last.elapsed().as_secs_f64()
    } else {
        f64::MAX
    };
    let idle_seconds_capped = if idle_seconds == f64::MAX {
        -1.0
    } else {
        idle_seconds
    };
    JsonRpcResponse::success(
        id,
        json!({
            "typing": typing,
            "idle_seconds": idle_seconds_capped,
        }),
    )
}

fn handle_send_wait_idle(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let (surface_id, text) =
        match surface::decode_input_header("surface.send_wait_idle", params, &id) {
            Ok(input) => input,
            Err(error) => return error,
        };
    if engine.is_typing(surface_id) {
        return JsonRpcResponse::success(id, json!({ "sent": false, "reason": "typing" }));
    }
    if let Some(terminal) = engine.find_terminal_by_id_mut(surface_id) {
        terminal.send_key(text);
        JsonRpcResponse::success(id, json!({ "sent": true }))
    } else {
        JsonRpcResponse::invalid_params(id, format!("Surface {} not found", surface_id))
    }
}

#[cfg(test)]
mod require_surface_id_tests {
    use super::require_surface_id;
    use crate::runtime::terminal_store::PTY_ID_BASE;
    use serde_json::json;

    #[test]
    fn accepts_surface_space_ids() {
        let id = json!(1);
        assert_eq!(
            require_surface_id(&json!({ "surface_id": 7 }), &id).unwrap(),
            7
        );
        assert_eq!(
            require_surface_id(&json!({ "surface_id": PTY_ID_BASE - 1 }), &id).unwrap(),
            PTY_ID_BASE - 1
        );
    }

    #[test]
    fn rejects_missing_wrong_type_and_out_of_u32_range() {
        let id = json!(1);
        assert!(require_surface_id(&json!({}), &id).is_err());
        assert!(require_surface_id(&json!({ "surface_id": "3" }), &id).is_err());
        assert!(require_surface_id(&json!({ "surface_id": -1 }), &id).is_err());
        assert!(
            require_surface_id(&json!({ "surface_id": u64::from(u32::MAX) + 1 }), &id).is_err()
        );
    }

    #[test]
    fn rejects_pty_id_space() {
        let id = json!(1);
        assert!(require_surface_id(&json!({ "surface_id": PTY_ID_BASE }), &id).is_err());
        // 실사용에서 관측된 오염 id.
        assert!(require_surface_id(&json!({ "surface_id": 2147484147u64 }), &id).is_err());
        assert!(require_surface_id(&json!({ "surface_id": u32::MAX }), &id).is_err());
    }
}

#[cfg(test)]
mod system_info_tests {
    use super::{handle_system_info, system_info_fields};

    #[test]
    fn system_info_identifies_the_engine_and_the_active_workspace_by_id() {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let info = system_info_fields(&engine.read(), state.active_workspace_index(&engine));
        assert_eq!(info["scope"], "engine");
        assert_eq!(info["workspace_count"], engine.workspaces().len());
        assert_eq!(info["active_workspace"], 0);
        assert_eq!(
            info["active_workspace_id"],
            engine.workspace_at(0).expect("workspace index is valid").id
        );
        assert_eq!(
            info["workspace_ids"],
            serde_json::json!([engine.workspace_at(0).expect("workspace index is valid").id])
        );
    }

    #[test]
    fn system_info_does_not_invent_an_active_workspace_for_an_empty_engine() {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        *engine.core = crate::core::CoreState::new_base();
        let info = system_info_fields(&engine.read(), state.active_workspace_index(&engine));
        assert_eq!(info["workspace_count"], 0);
        assert_eq!(
            info["active_workspace"],
            state.active_workspace_index(&engine)
        );
        assert!(info["active_workspace_id"].is_null());
        assert_eq!(info["workspace_ids"], serde_json::json!([]));
    }

    /// 패키지 버전과 별도로 서버 capability를 제공한다.
    #[test]
    fn system_info_declares_what_this_server_can_negotiate() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let scope = crate::adapters::ipc::request_scope::RequestScope::capture(
            &mut state,
            engine.core,
            #[cfg(feature = "gui")]
            None,
        );
        let resp = handle_system_info(&scope, &engine.read(), serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        let caps = result["capabilities"]
            .as_array()
            .expect("capabilities 가 배열로 실려야 한다");
        assert!(!caps.is_empty());
        for c in caps {
            assert!(c["name"].is_string(), "{c}");
            assert!(c["version"].is_u64(), "{c}");
        }
        assert_eq!(result["scope"], "engine");
        assert!(result["version"].is_string());
    }

    /// client가 요구하는 capability가 응답에 실제로 포함되는지 확인한다.
    #[test]
    fn system_info_declares_the_capability_name_the_client_asks_for() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let scope = crate::adapters::ipc::request_scope::RequestScope::capture(
            &mut state,
            engine.core,
            #[cfg(feature = "gui")]
            None,
        );
        let resp = handle_system_info(&scope, &engine.read(), serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        let names: Vec<&str> = result["capabilities"]
            .as_array()
            .expect("배열")
            .iter()
            .filter_map(|c| c["name"].as_str())
            .collect();
        assert!(
            names.contains(&tasty_ipc::client::IDEMPOTENCY_CAPABILITY),
            "client 가 요구하는 이름이 선언에 없다: {names:?}"
        );
    }

    #[test]
    fn system_info_declares_the_bounds_of_the_idempotency_guarantee() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let scope = crate::adapters::ipc::request_scope::RequestScope::capture(
            &mut state,
            engine.core,
            #[cfg(feature = "gui")]
            None,
        );
        let resp = handle_system_info(&scope, &engine.read(), serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        assert_eq!(result["idempotency"], super::idempotency::declaration());
        assert_eq!(result["idempotency"]["survives_restart"], false);
    }

    /// capability를 창별 공통 필드에 중복하지 않는다.
    #[test]
    fn the_per_window_fields_do_not_repeat_the_server_capabilities() {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let shared = system_info_fields(&engine.read(), state.active_workspace_index(&engine));
        assert!(
            shared.get("capabilities").is_none(),
            "창마다 재사용되는 필드에 capability 가 실렸다: {shared}"
        );
    }
}
