mod checked;
#[cfg(test)]
pub(crate) mod cli_entry_tests;
// debug 빌드에만 있는 CLI 진입점 시험 — 배치 규율상 별도 파일이다(그 파일의 doc 참조).
#[cfg(all(test, debug_assertions))]
mod cli_entry_debug_tests;
mod completion_strategy;
#[cfg(all(debug_assertions, feature = "gui"))]
mod debug;
#[cfg(debug_assertions)]
mod debug_nav;
#[cfg(debug_assertions)]
pub(crate) mod debug_plugin;
#[cfg(debug_assertions)]
mod debug_state;
#[cfg(debug_assertions)]
mod debug_terminal;
mod file_handler;
#[cfg(feature = "gui")]
mod file_picker;
#[cfg(feature = "gui")]
mod git_viewer;
mod hook_handler;
pub(crate) mod idempotency;
#[cfg(test)]
mod intent_order_tests;
// 창 라우터 팔의 호출자 명부와 그 집행 — 라우터가 gui 전용이라 시험도 gui 조합에서만 돈다.
#[cfg(all(test, feature = "gui"))]
mod window_router_caller_tests;
// `list_global` 이 두 hook 목록을 합산하므로 크레이트 안에서 보여야 한다.
pub(crate) mod hooks;
// `output`/`pane`/`surface` 와 같은 이유로 열려 있다 — `image.list` 도 전 창 합산
// 대상이다(`app/dispatch/list_global.rs`).
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
// `pane`/`surface`/`workspace` 와 같은 이유로 열려 있다 — 창 소유 자원의 list 를
// 호스트가 전 창 합산으로 답하기 때문(`app/dispatch/list_global.rs`).
pub(crate) mod output;
pub(crate) mod pane;
pub(crate) mod params;
mod passkey;
mod preset;
mod pressure;
pub(crate) mod pty;
mod recent;
mod remote_profile;
mod settings;
pub(crate) mod surface;
pub(crate) mod tab;
mod telemetry;
mod terminal;
pub(crate) mod theme;
#[cfg(all(debug_assertions, feature = "gui"))]
mod tool;
mod webhook;
#[cfg(feature = "gui")]
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
// gui 게이트를 뗐다 — 이 모듈의 `handle_list`/`handle_open` 은 `PluginManager` 만
// 읽는다(창도 egui 도 안 본다). gui 를 요구하면 헤드리스 데몬이 자기 plugin popup 을
// 조회할 수단을 잃는다. 파일 자신의 `#![cfg(debug_assertions)]` 와 이제 일치한다.
#[cfg(debug_assertions)]
pub mod popup;
pub mod session;

#[cfg(feature = "gui")]
pub(crate) use checked::check_without_engine;
pub(crate) use checked::{CheckedRequest, check_request};

use std::borrow::Cow;

use serde_json::json;

use crate::core::CoreState;
use crate::ipc::alias;
use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

/// macOS GUI 빌드에서만 뜻이 있는 메서드를 다른 조합에서 불렀을 때의 답.
///
/// `-32601`("그런 메서드 없음")과 다른 코드를 쓰는 이유: 메서드는 **있다**. 표에
/// 등재돼 있고 CLI 도 내놓는다 — 이 플랫폼이 못 할 뿐이다. 호출자가 "오타" 와
/// "여기선 안 됨" 을 구별할 수 있어야 고칠 방법이 갈린다.
///
/// cfg 가 **쓰는 자리와 같아야 한다** — macOS gui 빌드에서는 그 arm 이 없어 이 상수도
/// 안 쓰이고, 이 크레이트는 dead_code 를 deny 한다. 그 조합은 여기서 빌드할 수 없으므로
/// (실측: macOS 크로스 체크가 libsqlite3-sys 에서 멈춘다) 컴파일러가 아니라 이 짝
/// 맞춤이 유일한 방어다.
///
/// **축이 둘이다.** 쓰는 자리는 `debug_assertions` 로 게이트된 debug 라우터 안에 있으므로
/// 플랫폼 축만 맞추면 release 에서 상수만 남아 `cargo build --release` 가 통째로 깨진다
/// (실측). 그 조합은 자동 채널이 없어서 — `docs/dev-guide/ci-gates.md` 가 release bin 을
/// 보는 잡이 없다고 적는다 — 여기서 안 맞추면 아무 데서도 안 잡힌다.
#[cfg(all(debug_assertions, not(all(target_os = "macos", feature = "gui"))))]
const PLATFORM_ONLY_MACOS_GUI: &str = "input reproduction over the OS event stream is macOS-only and needs the gui build \
     (CGEventPost / TISSelectInputSource have no equivalent here)";
use crate::ipc::window_port::IpcWindow;
use crate::state::AppState;

/// caller가 명시된 라우터 진입점. CLI/네트워크 IPC는 [`CallerContext::Local`],
/// plugin process가 호출한 명령은 [`CallerContext::Plugin`]을 전달한다.
///
/// 라우터 구조:
/// 1. **engine 핸들러** (`route_engine_handler`): 창 상태 자체가 대상이 아닌 핸들러 전부.
///    핸들러는 자기가 닿는 상태만 인자로 받고(ADR-0470), 창에는 `AppState` 가 아니라
///    [`IpcWindow`] 포트와 intent 출구로만 닿는다
///    (`docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md`).
/// 2. **창 핸들러** (`route_window_handler`): gui 빌드 전용. 창 상태 자체를 여는 핸들러
///    (파일 선택기 팝업) — 진입점이 쥔 `AppState` 를 받는다.
/// 3. **debug 핸들러** (`route_debug_handler`): debug build 전용. release 에서는 정의 안 됨.
///    창 상태를 조작하는 debug 표면이라 역시 `AppState` 를 받는다.
///
/// 게이트 3종(권한 / telemetry cap / rate limit)은 라우팅보다 **먼저** 돈다. plugin 이
/// 호출한 명령이 권한을 통과하지 못하면 `permission_denied` 로 즉시 회신한다.
///
/// 직접 진입은 공통 게이트를 수행한다. 바깥에서 이미 검사한 경로는
/// CheckedRequest를 넘겨 handle_checked_request로 실행한다(ADR-0277).
#[cfg(test)]
pub fn handle_with_caller(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    request: &JsonRpcRequest,
    caller: &CallerContext,
) -> JsonRpcResponse {
    match check_request(core, state, engine, request, caller) {
        Ok(checked) => handle_checked_request(core, state, engine, &checked),
        Err(response) => response,
    }
}

/// 공통 게이트를 통과한 동일 요청을 실행한다. 예산과 허용 관측을 다시 소비하지 않는다.
///
/// 실행에 걸린 시간을 여기서 잰다. 이 함수가 그 자리인 이유는 GUI 라우팅·headless
/// pump·plugin host-call·intent cascade 가 **전부 여기로 모이기** 때문이다 — 게이트
/// 안(`check_request`)에서 재면 handler 가 아직 돌지 않았고, 호출부마다 재면 자리가
/// 여덟 곳으로 흩어진다.
///
/// 이 진입점은 요청이 닿은 창의 `AppState` 를 받는다 — 창 상태 자체가 대상인 창·debug
/// 핸들러에 그것을 건네고, 요청의 intent 출구를 그 창 큐로 옮기는 자리이기 때문이다. 그 아래
/// 엔진 핸들러 표는 그 창을 포트로만 본다
/// (`docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md`).
pub(crate) fn handle_checked_request(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut CoreState,
    checked: &CheckedRequest<'_>,
) -> JsonRpcResponse {
    let started = core.now_instant();
    let response = route_checked_request(core, state, engine, checked);
    // 시작·끝 둘 다 `Clock` port 를 지난다 — 한쪽만 port 면 주입한 시계로 잰 값이
    // 실제 벽시계와 섞인다.
    let elapsed = core.now_instant().duration_since(started);
    core.pressure().record_handler(elapsed);
    response
}

/// [`handle_checked_request`] 의 라우팅 본체. 조기 return 이 여럿이라 계측을 이 함수
/// **바깥**에 두어야 모든 갈래가 같은 자리에서 끝난다.
fn route_checked_request(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut CoreState,
    checked: &CheckedRequest<'_>,
) -> JsonRpcResponse {
    let request = checked.request();
    let caller = checked.caller();
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let (_, routed) = canonicalize_and_route(request);
    let request = routed.as_ref();

    // 멱등 키는 **정규화 뒤**에 본다 — 옛 이름과 새 이름이 같은 요청이므로 키도 같은
    // 것을 가리켜야 한다. 그리고 라우팅 **전**이라, 키가 재시도를 가리키면 handler 가
    // 아예 안 돈다(그것이 두 번째 효과를 막는 유일한 지점이다).
    let pending = match idempotency::begin(core.now_instant(), caller, request, &id) {
        Ok(pending) => pending,
        Err(answer) => return answer,
    };
    let response = dispatch_routed(core, state, engine, caller, request, id);
    idempotency::finish(core.now_instant(), pending, &response);
    response
}

/// 정규화된 요청을 실제 handler 로 보낸다. 조기 return 이 여럿이라 보존소 기록을 이
/// 함수 **바깥**에 두어야 모든 갈래의 답이 같은 자리에서 기록된다.
fn dispatch_routed(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut CoreState,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> JsonRpcResponse {
    // 핸들러가 낸 intent 는 요청 하나의 출구에 모였다가 라우팅이 끝난 뒤 이 창의 큐 끝으로
    // 옮겨진다 — 넣은 순서 그대로다(`window_port` 모듈 문서).
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    let routed = route_engine_handler(core, state, &mut out, engine, caller, request, id.clone());
    state.enqueue_intents(out);
    if let Some(resp) = routed {
        return resp;
    }

    #[cfg(feature = "gui")]
    if let Some(resp) = route_window_handler(state, engine, caller, request, id.clone()) {
        return resp;
    }

    #[cfg(debug_assertions)]
    if let Some(resp) = route_debug_handler(state, engine, request, id.clone()) {
        return resp;
    }

    JsonRpcResponse::unrouted_for_external_caller(id, &request.method)
}

/// method alias 정규화 + deprecated 경고 + 라우팅용 request 구성.
/// 옛 이름이면 method 를 새 이름으로 교체한 임시 request 를 반환한다.
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
        Cow::Owned(JsonRpcRequest {
            response_timeout_ms: None,
            // 옛 이름으로 온 것도 **같은 요청**이다 — 키를 여기서 떨어뜨리면 alias 로
            // 부른 호출자만 멱등 계약 밖으로 조용히 빠진다.
            idempotency_key: request.idempotency_key.clone(),
            jsonrpc: request.jsonrpc.clone(),
            method: canonical.to_string(),
            params: request.params.clone(),
            id: request.id.clone(),
            session_token: request.session_token.clone(),
        })
    };
    (canonical, routed)
}

/// 권한 게이트: caller 가 `canonical` 을 호출할 권한이 없으면 거부 응답 + audit Deny.
///
/// 거부가 Agent 의 권한 부족이면 **capability elevation 을 함께 발행한다.** 이것이
/// 없으면 에이전트는 `-32001` 과 `data: null` 만 받고 무엇을 요청해야 하는지도,
/// 요청할 자리도 알지 못한다 — 거부가 회복 불가능해진다.
///
/// GUI·headless·plugin 진입점은 같은 게이트를 사용한다. 권한 부족의
/// capability elevation은 공유 approval store에 기록된다.
pub(crate) fn check_permission_gate(
    core: &mut crate::core::Core,
    window: &mut dyn IpcWindow,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    canonical: &str,
    workspace_id: Option<u32>,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    if let Err(e) = caller.ensure_allowed(canonical) {
        tracing::warn!("ipc permission denied: {e}");
        let seq = engine.telemetry_seq.next();
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
        // 권한 부족만 격상으로 회복된다 — `UnknownMethod`/`NotPluginCallable` 은
        // 어떤 권한을 줘도 통과하지 않으므로 단순 거부다. Plugin caller 도 대상이
        // 아니다: 그쪽 권한은 매니페스트와 grant 로 정해지고 승인 흐름이 따로 있다.
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

/// 텔레메트리 cap 차단 게이트: triggered + (Pause|RequireApproval) 인 cap 이 있는
/// plugin agent 는 모든 IPC 가 거부된다. CLI/Local 은 검사 대상이 아니므로
/// `telemetry.cap.reset` 으로 해제 가능.
pub(crate) fn check_cap_gate(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    canonical: &str,
    workspace_id: Option<u32>,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    if let Some(reason) = telemetry::check_cap_block(core, caller, canonical) {
        tracing::warn!("ipc cap blocked: {reason}");
        let seq = engine.telemetry_seq.next();
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

/// rate_limit 미들웨어: 등록된 (agent, "ipc_calls") 한도 초과 시
/// -32010 throttled 응답 + audit Deny. 자가 회복을 위해 agent.rate_limit_*
/// 자체는 제외 (영구 차단 방지). throttled 호출은 `record_ipc_call` 을 건너
/// 뛰므로 `ipc_calls` telemetry 이벤트로 카운트되지 않는다 — throttle 추적은
/// `RateLimit.throttled_count` 가 담당.
pub(crate) fn check_rate_limit_gate(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    canonical: &str,
    workspace_id: Option<u32>,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    if !should_rate_limit(caller, canonical) {
        return None;
    }
    let agent_id = caller.agent_id();
    let agent = agent_id.as_str();
    match core.rate_limit_try_consume(agent, "ipc_calls", 1, telemetry::now_ms()) {
        Ok(outcome) if !outcome.allowed => {
            let reason = format!("throttled: tokens_left={:.2}", outcome.tokens_left);
            tracing::warn!("ipc rate_limited: {reason}");
            let seq = engine.telemetry_seq.next();
            crate::ipc::audit::record(
                core,
                caller,
                canonical,
                crate::store::audit::AuditDecision::Deny,
                Some(&reason),
                workspace_id,
                seq,
            );
            Some(JsonRpcResponse::error(id.clone(), -32010, reason))
        }
        Ok(_) => None,
        Err(e) => {
            // fail-open: rate_limit 인프라 자체 실패는 전체 IPC 차단보다 통과 + warn.
            tracing::warn!("rate_limit middleware error: {e}");
            None
        }
    }
}

/// 텔레메트리 미들웨어: 비-host caller 의 IPC 호출을 자동 카운트.
/// `telemetry.*` 자체와 `_host` agent 는 카운트 제외 (재귀 폭주 / 자기-측정 방지).
/// 카운트는 cap_eval 직후 호출되며 record 시 cap 평가도 함께 일어난다.
///
/// audit: allow 는 `audit::record` 가 정책에 따라 **버린다**(ADR-0085). 호출을
/// 남겨두는 이유는 정책이 audit 쪽 한 곳에만 있다는 것을 이 자리에서 읽히게 하고,
/// 정책이 바뀌면 게이트 통과 지점을 다시 찾아 붙이지 않아도 되게 하기 위해서다.
fn record_telemetry_and_audit(
    core: &mut crate::core::Core,
    window: &mut dyn IpcWindow,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    canonical: &str,
    params: &serde_json::Value,
    workspace_id: Option<u32>,
) {
    // 게이트가 낸 intent(상한·이상 탐지 알림)도 요청 하나의 출구에 모았다가 이 창의 큐 끝으로
    // 옮긴다 — 핸들러 쪽 출구와 같은 규칙이다(`window_port` 모듈 문서). 핸들러보다 먼저 돌므로
    // 같은 요청의 핸들러 intent 보다 앞에 쌓인다.
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    telemetry::record_ipc_call(core, window, &mut out, engine, caller, canonical, params);
    window.enqueue_intents(out);

    let seq = engine.telemetry_seq.next();
    crate::ipc::audit::record(
        core,
        caller,
        canonical,
        crate::store::audit::AuditDecision::Allow,
        None,
        workspace_id,
        seq,
    );
}

/// IPC rate_limit 미들웨어가 적용되는 caller/method 조합인가?
///
/// 제외 정책:
/// - **Local**: 사용자가 직접 CLI/network 로 호출 — 무제한.
/// - **Agent `_host`**: 호스트 자기 호출 (telemetry.rs:103 의 record_ipc_call
///   제외와 일관). throttle 자체 무의미.
/// - **`telemetry.*` / `agent.rate_limit_*` / `system.info`**:
///   - `telemetry.*` — record_ipc_call 자체가 호출하므로 재귀 폭주 위험.
///   - `agent.rate_limit_*` — throttle 걸린 agent 의 *자가 회복 경로*. 이게
///     막히면 한 번 throttle 된 agent 가 영구 차단됨.
///   - `system.info` — 단순 상태 조회. throttle 대상 아님.
fn should_rate_limit(caller: &CallerContext, method: &str) -> bool {
    use crate::ipc::caller::CallerContext as C;
    match caller {
        C::Local => return false,
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

/// Plugin 타입 RSS 이상탐지(`docs/features/telemetry/index.md` RssSurge) 진입점.
/// `telemetry` 하위모듈이
/// `mod telemetry;`(private) 라 `App::about_to_wait` 같은 crate 외부(다른
/// 서브트리)에서 직접 부를 수 없어, 이 함수가 유일한 공개 경유지다.
///
/// `PluginManager::pump()` 이 sysinfo 로 직접 sampling 한 (plugin_id,
/// rss_bytes) 목록을 그대로 넘기면 된다 — Agent 타입 self-report 는 이
/// 함수를 거치지 않고 `telemetry.record` 경로(`telemetry::record::handle_record`)
/// 에서 처리된다.
#[cfg(feature = "gui")]
pub fn record_plugin_rss_samples(
    core: &crate::core::Core,
    window: &mut dyn IpcWindow,
    engine: &mut crate::core::CoreState,
    samples: &[(String, u64)],
) {
    let ts = telemetry::now_ms();
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    for (plugin_id, rss_bytes) in samples {
        telemetry::record_rss_sample(core, window, &mut out, engine, plugin_id, *rss_bytes, ts);
    }
    window.enqueue_intents(out);
}

/// engine-substate handlers — UI에 의존하지 않음. 권한 게이트(`check_permission_gate`) 대상.
///
/// 현재는 시그니처가 `&mut AppState`이지만 본문이 GUI를 만지지 않는다. 향후
/// AppState 메서드들이 `CoreState`로 이전되면 시그니처를 `&mut CoreState`로
/// 좁힐 예정 (별도 작업).
/// hard-occupied workspace 에 대한 **비-holder** 구조 변경 IPC 를 차단한다
/// (`split`/`tab.create` 생성 계열, `pane.close`/`tab.close`/`tab.move`/
/// `surface.close` close·이동 계열, `markdown.navigate`/`image.open` convert
/// 계열).
///
/// `terminal.spawn` 도 같은 정책의 대상이지만 여기서 걸지 않는다 — 대상이
/// `workspace` 파라미터가 아니라 `pane` 오버라이드까지 반영된 최종 pane 이라,
/// 그것을 아는 [`spawn_target_guard`] 에서 집행한다(같은 자리에서 mirror 판정도
/// 함께 한다). 근거는 그 함수의 doc 과 ADR-0086.
///
/// **convert 계열은 이 두 method 만 커버한다(완전하지 않음, 알려진 한계)**:
/// `ConvertSurface` 를 발행하는 진입점은 kind 별로 흩어져 있고(`markdown.navigate`,
/// `image.open`, 그리고 host 범용 convert 팝업 — 팝업은 `state.dispatch_intent` 를
/// 직접 호출해 이 IPC method-string 라우팅 자체를 타지 않는다) 향후 새 kind 가
/// 자기 전용 convert-진입 method 를 추가하면 이 목록에 없는 한 가드가 적용되지
/// 않는다. 다른 6종 op 도 GUI 로컬 액션(단축키 등)은 `state.dispatch_intent` 로
/// 이 라우팅을 우회하므로(동일한 특성), convert 도 그와 동등한 수준(=IPC 경유만
/// 커버)까지만 맞춘 것으로 범위를 제한했다.
///
/// **`terminal.spawn` 은 예외가 아니다**: 과거엔 "새 리소스를 추가만 하는 생성
/// 경로는 holder 의 화면을 안 흔드니 차단 대상이 아니다"로 문서화돼 있었으나,
/// 그 논거는 holder 관점만 다뤘다 — spawn 을 호출한 로컬 agent 자신이 그 직후
/// `tap_new_workspace_member`(`core/attach_runtime.rs`)로 새 surface 가 즉시 같은
/// hard lock 을 상속받아 자기 결과물에 입력을 못 넣게 되는 부작용은 검토되지
/// 않았다. 이 사각지대 때문에 정책을 뒤집어 `terminal.spawn` 도 차단 대상이
/// 됐다(근거: ADR-0060, ADR-0040 은 유지). 집행 지점만 위에 적은 대로
/// [`spawn_target_guard`] 로 옮겼고, 정책 자체는 그대로다.
///
/// **왜 여기(문자열 method dispatch)인가**: `execute_forwarded_structural_op`
/// (`src/core/attach_runtime.rs`)는 attach 점유 holder 가 forward 한 구조 변경을
/// 실행할 때 `tab::handle_tab_create`/`pane::handle_split`/`tab::handle_tab_close`/
/// `pane::handle_pane_close`/`surface::handle_surface_close`/`tab::handle_tab_move`
/// 를 **직접 함수 호출**해서 이 method-string 라우팅을 우회한다 — "attach 연결
/// 자체가 그 workspace 에 대한 구조 변경 권한을 증명한다"는 모델
/// (`docs/features/remote-attach/index.md` "mirror 워크스페이스 내 구조 변경" 절)
/// 이기 때문이다. 따라서 가드를 `Core::apply` 나 핸들러 함수 내부에 두면 holder
/// 본인의 forward 요청까지 함께 막혀버린다(회귀). 이 dispatch 지점만 두 경로가
/// 갈라지는 유일한 곳이라 여기서만 걸어야 한다.
///
/// 대상을 찾을 수 없거나(params 누락 등) 점유 아님이면 `None`(핸들러가 그대로
/// 진행 — 정상 검증/실행 경로에 위임).
fn hard_occupied_structural_guard(
    core: &crate::core::Core,
    engine: &crate::core::CoreState,
    method: &str,
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    // 이 조회는 **어느 워크스페이스의 상태를 볼지**만 고른다. 잘못된 값의 오류를 여기서
    // 버리는 것은(`.ok().flatten()`) 대상 없음으로 흘려보내기 위해서다 — 그 뒤 핸들러의
    // `require_*` 가 같은 값을 다시 읽고 **이유를 붙여 거절**한다. 중요한 것은 자르지
    // 않는 것이다: 자르면 `None` 이 아니라 실재하는 다른 대상이 되어 라우팅이 성공한다.
    let ws_idx: usize = match method {
        "split" => {
            // 자르지 않는다 — 자르면 `None` 이 아니라 **실재하는 다른 pane** 이 되어
            // 라우팅이 성공한다. 범위 밖은 종전대로 대상 없음으로 흘려보내고, 그 뒤
            // 핸들러의 `require_*` 가 이유를 붙여 거절한다.
            let target_pane = params::read_int::<u32>(params, "target_pane")
                .ok()
                .flatten();
            let target_surface = pane::resolve_surface_target(core, params);
            target_pane
                .and_then(|pid| engine.find_workspace_index_for_pane(pid))
                .or_else(|| {
                    target_surface
                        .and_then(|sid| engine.find_workspace_index_for_surface(sid))
                        .map(|(i, _)| i)
                })?
        }
        // 워크스페이스 통째 닫기 — 대상 자체가 workspace 라 id/index 를 그대로 쓴다.
        "workspace.close" => {
            if let Some(ws_id) = params::read_int::<u32>(params, "id").ok().flatten() {
                engine.workspaces.iter().position(|w| w.id == ws_id)?
            } else {
                params::read_int::<usize>(params, "index").ok().flatten()?
            }
        }
        "tab.create" | "pane.close" | "tab.move" => {
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
        _ => return None,
    };
    let ws_id = engine.workspaces.get(ws_idx)?.id;
    if engine.attach.workspace_holder(ws_id).is_some() {
        return Some(hard_occupied_denial(ws_id, id));
    }
    None
}

/// hard-occupied 거부 응답 — 라우터 가드와 [`spawn_target_guard`] 가 공유한다.
/// 두 진입점이 같은 정책을 집행하므로 문구도 한 곳에서만 만든다.
fn hard_occupied_denial(ws_id: u32, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::invalid_params(
        id.clone(),
        format!(
            "Workspace {ws_id} is occupied by a remote attach session (hard-occupied) — \
             structural changes (new tab/split/close/move) must come from that session. \
             Use a different workspace."
        ),
    )
}

/// `terminal.spawn` 전용 대상 가드 — **최종 확정된 pane 이 실제로 속한 워크스페이스**
/// 를 기준으로 mirror / hard-occupied 를 함께 판정한다. 거부면 tab/surface 를 하나도
/// 만들지 않고 `invalid_params` 를 돌려준다.
///
/// **왜 라우터 가드가 아니라 여기인가.** `terminal.spawn` 의 대상은 `workspace`
/// 파라미터가 아니라 `pane` 오버라이드까지 반영해 확정된 pane 이다. 라우터는
/// `workspace` 문자열만 resolve 할 수 있어 `--workspace <무해한 ws>` +
/// `--pane <차단 대상 ws 의 pane>` 조합을 통과시킨다. `handle_spawn` 은 그 뒤
/// `tab::handle_tab_create` 를 **함수로 직접 호출**하므로 라우터를 다시 타지도
/// 않는다 — 즉 대상을 정확히 아는 유일한 지점이 여기다.
///
/// 다른 구조 op 처럼 [`hard_occupied_structural_guard`] 에 두지 않는 이유는
/// forward 회귀가 없기 때문이다: `execute_forwarded_structural_op`
/// (`src/core/attach_runtime.rs`)이 부르는 도메인 실행 함수 6개(`core::structural_exec`)에
/// spawn 은 포함되지 않는다. 그래서 핸들러 내부에 둬도 holder 본인의 정당한 forward 를
/// 막지 않는다 — 다른 6종은 가드를 도메인 함수나 `Core::apply` 에 두면 forward 까지 막는다.
///
/// **mirror 판정은 `terminal.spawn` 에만 적용된다.** mirror 워크스페이스 안의
/// 나머지 구조 변경은 원격으로 forward 되는 것이 정상 설계이므로
/// (`docs/features/remote-attach/index.md`), 라우터 가드에는 mirror 판정을 넣지
/// 않는다. 근거: ADR-0086.
fn spawn_target_guard(
    engine: &crate::core::CoreState,
    pane_id: u32,
    id: &serde_json::Value,
) -> Option<JsonRpcResponse> {
    let ws_idx = engine.find_workspace_index_for_pane(pane_id)?;
    let ws = engine.workspaces.get(ws_idx)?;
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
    if engine.attach.workspace_holder(ws_id).is_some() {
        return Some(hard_occupied_denial(ws_id, id));
    }
    None
}

fn route_engine_handler(
    core: &mut crate::core::Core,
    window: &mut dyn IpcWindow,
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    if let Some(resp) =
        hard_occupied_structural_guard(core, engine, &request.method, &request.params, &id)
    {
        return Some(resp);
    }
    Some(match request.method.as_str() {
        "system.info" => handle_system_info(window, engine, id),
        // 게이지 조회. `&*core` 인 이유는 읽기가 원자값이라 가변 빌림이 필요 없기
        // 때문이다 — 그 근거는 `Core::pressure` 의 doc 에 있다. `engine` 은 스트림
        // 허브를 거기서만 꺼낼 수 있어서다(`OccupancyRegistry::notifier`).
        "system.pressure" => pressure::handle_system_pressure(&*core, engine, id),
        // workspace
        "workspace.list" => workspace::handle_workspace_list(window, engine, id),
        "workspace.create" => {
            workspace::handle_workspace_create(core, window, engine, id, &request.params)
        }
        "workspace.update" => {
            workspace::handle_workspace_update(core, window, engine, id, &request.params)
        }
        "workspace.move" => {
            workspace::handle_workspace_move(core, window, engine, id, &request.params)
        }
        "workspace.close" => workspace::handle_workspace_close(window, engine, id, &request.params),
        // workspace category (사이드바 폴더 CRUD — 원칙 1·3: active/포커스 불변)
        "workspace_category.list" => workspace_category::handle_list(engine, id),
        "workspace_category.create" => {
            workspace_category::handle_create(engine, id, &request.params)
        }
        "workspace_category.rename" => {
            workspace_category::handle_rename(engine, id, &request.params)
        }
        "workspace_category.delete" => {
            workspace_category::handle_delete(engine, id, &request.params)
        }
        "workspace_category.move" => workspace_category::handle_move(engine, id, &request.params),
        // pane / split
        "pane.list" => pane::handle_pane_list(engine, id),
        "pane.close" => pane::handle_pane_close(core, window, engine, id, &request.params),
        "split" => pane::handle_split(core, window, engine, id, &request.params),
        // tab
        "tab.list" => tab::handle_tab_list(engine, id, &request.params),
        "tab.create" => tab::handle_tab_create(core, window, engine, id, &request.params),
        "tab.close" => tab::handle_tab_close(core, window, engine, id, &request.params),
        "tab.move" => tab::handle_tab_move(core, engine, id, &request.params),
        // terminal (child-terminal 관리, ADR-0040 / occupancy-04)
        "terminal.spawn" => terminal::handle_spawn(core, window, engine, id, &request.params),
        "terminal.tell" => terminal::handle_tell(core, engine, id, &request.params),
        "terminal.children" => terminal::handle_children(engine, id, &request.params),
        "terminal.parent" => terminal::handle_parent(engine, id, &request.params),
        "terminal.state" => terminal::handle_state(engine, id, &request.params),
        "terminal.kill" => terminal::handle_kill(core, window, engine, id, &request.params),
        "terminal.respawn" => terminal::handle_respawn(core, engine, id, &request.params),
        "terminal.broadcast" => terminal::handle_broadcast(core, engine, id, &request.params),
        "terminal.set_state" => terminal::handle_set_state(engine, id, &request.params),
        "terminal.adopt" => terminal::handle_adopt(engine, id, &request.params),
        "terminal.release" => terminal::handle_release(engine, id, &request.params),
        // headless PTY primitive (docs/adr/0050-headless-pty-primitive.md /
        // pty_registry) — Surface 없는 백그라운드 PTY
        "pty.spawn" => pty::handle_spawn(core, engine, caller, id, &request.params),
        "pty.write" => pty::handle_write(engine, id, &request.params),
        "pty.read" => pty::handle_read(engine, id, &request.params),
        "pty.wait" => pty::handle_wait(engine, id, &request.params),
        "pty.kill" => pty::handle_kill(engine, id, &request.params),
        "pty.list" => pty::handle_list(engine, id),
        "pty.attach_surface" => {
            pty::handle_attach_surface(core, window, engine, id, &request.params)
        }
        // preset (layout preset CRUD + apply)
        "preset.list" => preset::handle_list(core, id, &request.params),
        "preset.get" => preset::handle_get(core, id, &request.params),
        "preset.save" => preset::handle_save(core, id, &request.params),
        "preset.delete" => preset::handle_delete(core, id, &request.params),
        "preset.rename" => preset::handle_rename(core, id, &request.params),
        "preset.capture" => preset::handle_capture(core, engine, id, &request.params),
        "preset.apply" => preset::handle_apply(core, window, engine, id, &request.params),
        // surface
        "surface.close" => surface::handle_surface_close(core, window, engine, id, &request.params),
        "surface.close_self" => {
            surface::handle_surface_close_self(core, window, engine, id, &request.params)
        }
        "surface.list" => surface::handle_surface_list(engine, id),
        "surface.kinds" => surface::handle_surface_kinds(engine, id),
        "surface.send" => surface::handle_surface_send(core, engine, id, &request.params),
        "surface.send_key" => surface::handle_surface_send_key(core, engine, id, &request.params),
        "surface.send_combo" => {
            surface::handle_surface_send_combo(core, engine, id, &request.params)
        }
        "surface.send_to" => surface::handle_surface_send_to(core, engine, id, &request.params),
        "surface.wake" => surface::handle_surface_wake(engine, id, &request.params),
        "surface.set_mark" => surface::handle_set_mark(out, engine, id, &request.params),
        "surface.completion" => surface::handle_completion(out, engine, id, &request.params),
        "surface.attention.get" => surface::handle_attention_get(engine, id, &request.params),
        "surface.attention.clear" => {
            surface::handle_attention_clear(out, engine, id, &request.params)
        }
        "surface.read_since_mark" => surface::handle_read_since_mark(engine, id, &request.params),
        "surface.read_since_scan_mark" => {
            surface::handle_read_since_scan_mark(engine, id, &request.params)
        }
        "surface.parse_since_mark" => surface::handle_parse_since_mark(engine, id, &request.params),
        "surface.commands" => surface::handle_commands(core, engine, id, &request.params),
        "surface.last_command" => surface::handle_last_command(core, engine, id, &request.params),
        "surface.command_at" => surface::handle_command_at(core, engine, id, &request.params),
        "output.observe_start" => output::handle_observe_start(core, engine, id, &request.params),
        "output.observe_stop" => output::handle_observe_stop(core, engine, id, &request.params),
        "output.observe_list" => output::handle_observe_list(core, engine, id),
        "output.observe_info" => output::handle_observe_info(core, engine, id, &request.params),
        "surface.screen_text" => surface::handle_screen_text(engine, id, &request.params),
        "surface.cursor_position" => surface::handle_cursor_position(engine, id, &request.params),
        "surface.mouse_tracking" => surface::handle_mouse_tracking(engine, id, &request.params),
        "surface.foreground_process" => {
            surface::handle_foreground_process(engine, id, &request.params)
        }
        "surface.locate" => surface::handle_surface_locate(engine, id, &request.params),
        "surface.respawn_terminal" => {
            surface::handle_surface_respawn_terminal(core, engine, id, &request.params)
        }
        "surface.is_typing" => handle_is_typing(engine, id, &request.params),
        "surface.send_wait_idle" => handle_send_wait_idle(engine, id, &request.params),
        "surface.fire_hook" => {
            hooks::handle_surface_fire_hook(core, window, engine, id, &request.params)
        }
        "surface.meta.set" => meta::handle_surface_meta_set(core, engine, id, &request.params),
        "surface.meta.get" => meta::handle_surface_meta_get(core, engine, id, &request.params),
        "surface.meta.unset" => meta::handle_surface_meta_unset(core, engine, id, &request.params),
        "surface.meta.list" => meta::handle_surface_meta_list(core, engine, id, &request.params),
        "surface.set_cwd" => surface::handle_set_cwd(engine, id, &request.params),
        // hooks
        "hook.set" => hooks::handle_hook_set(core, engine, id, &request.params),
        "hook.list" => hooks::handle_hook_list(engine, id, &request.params),
        "hook.unset" => hooks::handle_hook_unset(core, engine, id, &request.params),
        "global_hook.set" => hooks::handle_global_hook_set(core, engine, id, &request.params),
        "global_hook.list" => hooks::handle_global_hook_list(engine, id),
        "global_hook.unset" => hooks::handle_global_hook_unset(core, engine, id, &request.params),
        // webhook (인바운드 웹훅 — 원칙 2·3: id 지정, list 전범위, 포커스 불변).
        // 상태는 전역 싱글턴이라 core/state/engine 미사용.
        "webhook.register" => webhook::handle_register(caller, id, &request.params),
        "webhook.list" => webhook::handle_list(id),
        "webhook.info" => webhook::handle_info(id, &request.params),
        "webhook.unregister" => webhook::handle_unregister(id, &request.params),
        "webhook.sweep" => webhook::handle_sweep(id),
        "webhook.config" => webhook::handle_config(id, &request.params),
        // webview (plugin 이 webview-enabled surface 의 URL/navigation 제어)
        #[cfg(feature = "gui")]
        "webview.set_url" => webview::handle_set_url(engine, id, &request.params),
        // webview-kind surface(예: markdown) 는 egui-mesh 와 달리 `surface.set_context` 를
        // 받지 않아 Theme 이 자동으로 밀리지 않는다 — 이 read-only 조회가 그 대체 경로다.
        "theme.query" => theme::handle_query(engine, id),
        // tree
        "tree" => handle_tree(window, engine, id),
        // message
        "message.send" => message::handle_message_send(core, engine, id, &request.params),
        "message.read" => message::handle_message_read(core, engine, id, &request.params),
        "message.count" => message::handle_message_count(engine, id, &request.params),
        "message.clear" => message::handle_message_clear(core, engine, id, &request.params),
        // notification (focus-independent — workspace_id/surface_id로 라우팅)
        "notification.list" => notification::handle_notification_list(engine, id),
        "notification.create" => {
            notification::handle_notification_create(out, engine, id, &request.params)
        }
        // file handler: 사용자 설정 reload (host 전용 — plugin 비노출).
        "file_handler.reload" => file_handler::handle_reload(core, engine, id),
        // file handler: finalize 된 detector 와 출처별 contribution 조회 (읽기 전용).
        "file_handler.detectors" => file_handler::handle_detectors(engine, id),
        // file handler: 임의 경로를 dispatch 흐름에 진입시킴. plugin (예: explorer)
        // 또는 CLI 가 호출. plugin 호출은 FsRead 권한 요구.
        // (docs/adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md)
        // 그 intent 를 적용할 identify worker 와 결과를 여는 창이 gui 에만 있어 arm 도
        // gui 에만 둔다. headless 에 두면 accept 만 받고 요청이 버려진다 —
        // `git_viewer.query` 와 같은 모양이고, 빼면 라우터 끝이 `-32017` 로 답한다.
        #[cfg(feature = "gui")]
        "file_handler.dispatch" => {
            file_handler::handle_dispatch(out, &*window, engine, caller, id, request.params.clone())
        }
        // hook handler: 공유 훅 핸들러 레지스트리 조회/재로드/수동 발화. 상태는
        // 전역 싱글턴이라 list/reload 는 core/state/engine 미사용. dispatch 만
        // IpcSequence 실행에 host injector 가 필요해 core 를 받는다.
        "hook_handler.list" => hook_handler::handle_list(id),
        "hook_handler.get" => hook_handler::handle_get(id, &request.params),
        "hook_handler.upsert" => hook_handler::handle_upsert(id, &request.params),
        "hook_handler.remove" => hook_handler::handle_remove(id, &request.params),
        "hook_handler.reload" => hook_handler::handle_reload(id),
        "hook_handler.dispatch" => hook_handler::handle_dispatch(core, id, &request.params),
        // completion_strategy: 완료 판정 전략 레지스트리 조회. 상태는
        // 전역 싱글턴이라 core/state/engine 미사용. reload/dispatch 대응물 없음
        // (전략은 판정 함수, "발화" 대상 아님).
        "completion_strategy.list" => completion_strategy::handle_list(id),
        // markdown 제자리 이동 — markdown plugin 의 주소창이 자기 surface 를 새 파일로 교체.
        #[cfg(feature = "gui")]
        "markdown.navigate" => markdown::handle_navigate(out, id, request.params.clone()),
        // generic per-kind 최근목록 조회 — markdown 주소창 드롭다운 데이터 공급원(markdown
        // plugin 이 kind="markdown" 으로 trampoline). 읽기 전용, 순수 데이터 조회라
        // gui-gate 불필요(headless 포함 항상 존재). host 는 특정 kind 를 모른다.
        "recent.query" => recent::handle_query(window, id, request.params.clone()),
        // (docs/adr/0056-git-viewer-remote-attach-git-query-channel.md) git-viewer
        // 원격 조회 트리거 — 큐잉 + request_id 회신. 큐를 비우는
        // `App::dispatch_pending_git_query_forwards` 가 gui 전용이라 arm 도 gui 에만
        // 둔다. headless 에 두면 accept 만 받고 결과가 안 온다(ADR-0053). 빼면
        // 라우터 끝이 `-32017` 로 답한다.
        #[cfg(feature = "gui")]
        "git_viewer.query" => git_viewer::handle_query(engine, id, &request.params),
        // (docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md) markdown
        // plugin 의 mirror 원문 요청 — `git_viewer.query` 와 동형이라 경계도 같다.
        #[cfg(feature = "gui")]
        "markdown_mirror.content_request" => {
            markdown_mirror::handle_content_request(engine, id, &request.params)
        }
        // image surface 조작 — com.tasty.image plugin namespace 의 호스트 어댑터.
        // host 는 open(ConvertSurface)/list(surface 순회)만 담당하고, 픽셀 편집 계열
        // (save/export_png/paste/next/prev)은 plugin 이 자기 namespace 에서 처리한다.
        #[cfg(feature = "gui")]
        "image.open" => image::handle_open(core, engine, id, &request.params),
        #[cfg(feature = "gui")]
        "image.list" => image::handle_list(engine, id),
        // memory: regular (공유 네임스페이스 + owner enforcement)
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
        "memory.import" => memory::handle_import(core, engine, caller, id, &request.params),
        // memory: secret (plugin 별 사전 분할)
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
        // memory: 유지 보수 (host 전용)
        "memory.gc" => memory::handle_gc(core, engine, caller, id, &request.params),
        // memory: blackboard (workspace-scoped 키-값 컬렉션)
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
        // memory: bb snapshot
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
        // memory: plan (workspace-scoped 선언적 work breakdown)
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
        // memory: cache (workspace-scoped TTL 캐시)
        "memory.cache_put" => memory::handle_cache_put(core, engine, caller, id, &request.params),
        "memory.cache_get" => memory::handle_cache_get(core, engine, caller, id, &request.params),
        "memory.cache_invalidate" => {
            memory::handle_cache_invalidate(core, engine, caller, id, &request.params)
        }
        "memory.cache_clear" => {
            memory::handle_cache_clear(core, engine, caller, id, &request.params)
        }
        "memory.cache_list" => memory::handle_cache_list(core, engine, caller, id, &request.params),
        // memory: goal (surface-scoped 단일 목표 문장)
        "memory.goal_set" => memory::handle_goal_set(core, engine, caller, id, &request.params),
        "memory.goal_get" => memory::handle_goal_get(core, engine, caller, id, &request.params),
        "memory.goal_clear" => memory::handle_goal_clear(core, engine, caller, id, &request.params),
        // settings (plugin 이 자기 plugin_settings 값을 read-back)
        "settings.get_plugin_setting" => {
            settings::handle_get_plugin_setting(engine, caller, id, &request.params)
        }
        // settings.remote_transfer (원격 전송 저장 폴더 + 용량 상한 get/set)
        "settings.get_remote_transfer" => settings::handle_get_remote_transfer(engine, id),
        "settings.set_remote_transfer" => {
            settings::handle_set_remote_transfer(out, engine, id, &request.params)
        }
        // approval (휴먼 핸드오프) — await 는 process_ipc 에서 worker thread 로 분리 처리.
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
        // telemetry (관측 / 비용) — 단계 4.1
        "telemetry.record" => {
            telemetry::handle_record(core, window, out, engine, caller, id, &request.params)
        }
        "telemetry.record_batch" => {
            telemetry::handle_record_batch(core, window, out, engine, caller, id, &request.params)
        }
        "telemetry.summary" => telemetry::handle_summary(core, engine, caller, id, &request.params),
        "telemetry.timeseries" => {
            telemetry::handle_timeseries(core, engine, caller, id, &request.params)
        }
        "telemetry.top" => telemetry::handle_top(core, engine, caller, id, &request.params),
        // telemetry.cap — CRUD + eval/action 발화(cap.rs)/차단(check_cap_block) 완전 결합
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
        // telemetry.anomaly (영속 anomaly 조회만; 검출은 dispatcher 후크)
        "telemetry.anomaly.list" => {
            telemetry::handle_anomaly_list(core, engine, caller, id, &request.params)
        }
        // telemetry.session_summary (메트릭/승인/이상 집계)
        "telemetry.session_summary" => {
            telemetry::handle_session_summary(core, engine, caller, id, &request.params)
        }
        // agent.task_* (DAG + state 머신)
        "agent.task_create" => agent::handle_task_create(core, engine, caller, id, &request.params),
        "agent.task_list" => agent::handle_task_list(core, engine, caller, id, &request.params),
        "agent.task_get" => agent::handle_task_get(core, engine, caller, id, &request.params),
        // agent.task_await 는 여기 없다(`approval.await` 가 빠진 것과 같은 이유) — 진짜 blocking 은
        // gui 빌드의 `App::process_ipc` app_methods 단계(`ipc_dispatch_task_await`)
        // 가 라우팅 전에 가로챈다. headless 빌드(`boot/headless_dispatch.rs`)는 그
        // 단계가 없어 이 라우터로 직접 오는데, 팔을 두면 비차단 fallback 이 진짜
        // blocking 응답과 다른 모양으로 조용히 성공해 버리므로 method_not_found 로
        // 정직하게 떨어지는 쪽을 택한다(local_only 라 plugin 경로에는 영향 없음).
        "agent.task_cancel" => agent::handle_task_cancel(core, engine, caller, id, &request.params),
        "agent.task_retry" => agent::handle_task_retry(core, engine, caller, id, &request.params),
        "agent.task_graph" => agent::handle_task_graph(core, engine, caller, id, &request.params),
        // agent.dag_* (workspace 안의 flat 한 task 를 무관한 그래프 단위로 쪼갠 뷰)
        "agent.dag_list" => agent::handle_dag_list(core, engine, caller, id, &request.params),
        "agent.dag_get" => agent::handle_dag_get(core, engine, caller, id, &request.params),
        // agent.task_set_result (외부 task 완료 신호)
        "agent.task_set_result" => {
            agent::handle_task_set_result(core, engine, caller, id, &request.params)
        }
        // agent.task_run (workspace runner thread 시작/중단/상태)
        "agent.task_run" => agent::handle_task_run(core, engine, caller, id, &request.params),
        // agent.task_delete / agent.task_purge (참조 검사 + 상태 제약을
        // 지키는 단건/일괄 삭제)
        "agent.task_delete" => agent::handle_task_delete(core, engine, caller, id, &request.params),
        "agent.task_purge" => agent::handle_task_purge(core, engine, caller, id, &request.params),
        // agent.barrier_* / semaphore_* (poll-based 동기화 primitive)
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
        // agent.lease_* (협조적 점유 마커 + TTL)
        "agent.lease_acquire" => {
            agent::handle_lease_acquire(core, engine, caller, id, &request.params)
        }
        "agent.lease_release" => {
            agent::handle_lease_release(core, engine, caller, id, &request.params)
        }
        "agent.lease_list" => agent::handle_lease_list(core, engine, caller, id, &request.params),
        // agent.task_reduce (결과 합성: first_success / all / merge_json / concat_text / custom)
        "agent.task_reduce" => agent::handle_task_reduce(core, engine, caller, id, &request.params),
        // agent.rate_limit_* (token bucket 시간당 비율 제한)
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
        // session.* (자식 agent 신원 토큰 관리)
        "session.issue" => session::handle_issue(core, caller, id, &request.params),
        "session.revoke" => session::handle_revoke(core, id, &request.params),
        "session.list" => session::handle_list(core, id),
        // attach.* — attach/detach 단계 3 (배타 점유 제어; session.* 와 별개)
        "attach.acquire" => attach::handle_acquire(engine, id, &request.params),
        "attach.release" => attach::handle_release(engine, id, &request.params),
        "attach.force_detach" => attach::handle_force_detach(engine, id, &request.params),
        "attach.force_detach_workspace" => {
            attach::handle_force_detach_workspace(engine, id, &request.params)
        }
        "attach.into_gui" => attach::handle_into_gui(engine, id, &request.params),
        "attach.list" => attach::handle_list(engine, id),
        // remote.profile.* — 원격 접속 프로필 CRUD (원칙 2). 로컬 파일 I/O.
        // (구 tool.ssh.* / ssh.profile.* 는 alias.rs 에서 정규화되어 여기로 도달.)
        "remote.profile.list" => remote_profile::handle_list(id),
        "remote.profile.get" => remote_profile::handle_get(id, &request.params),
        "remote.profile.add" => remote_profile::handle_add(id, &request.params),
        "remote.profile.detect" => remote_profile::handle_detect(id, &request.params),
        "remote.profile.remove" => remote_profile::handle_remove(id, &request.params),
        // 로컬 ssh config 열거·가져오기 — 파일 읽기만 한다(ssh 실행 없음).
        "remote.profile.list_local" => remote_profile::handle_list_local(id),
        "remote.profile.import" => remote_profile::handle_import(id, &request.params),
        // remote.passkey.* — 자격증명 CRUD (값 마스킹 — 경로/내용 미반환).
        "remote.passkey.list" => passkey::handle_list(id),
        "remote.passkey.get" => passkey::handle_get(id, &request.params),
        "remote.passkey.add" => passkey::handle_add(id, &request.params),
        "remote.passkey.remove" => passkey::handle_remove(id, &request.params),
        _ => return None,
    })
}

/// 창 상태를 **실제로 조작하는** GUI 핸들러의 라우터 — 엔진 핸들러 표(`route_engine_handler`)
/// 에 없는 메서드만 여기 온다. debug 쪽 짝은 [`route_debug_handler`] 다.
///
/// 엔진 핸들러는 창에 [`IpcWindow`] 포트로만 닿는다. 여기와 debug 라우터에 있는 것은 popup ·
/// 파일 선택기 · debug 주입처럼 창 상태 자체가 대상인 핸들러라 포트로 좁힐 것이 없다 — 그래서
/// 진입점이 쥔 `AppState` 를 그대로 받는다
/// (`docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md`).
#[cfg(feature = "gui")]
fn route_window_handler(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    Some(match request.method.as_str() {
        // (ADR-0058) plugin 이 host 소유 file_picker popup 을 연다. popup 을
        // 여는 UI state 변경이라 gui feature 전용.
        "file_picker.trigger" => {
            file_picker::handle_trigger(state, engine, caller, id, &request.params)
        }
        _ => return None,
    })
}

#[cfg(debug_assertions)]
fn route_debug_handler(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    request: &JsonRpcRequest,
    id: serde_json::Value,
) -> Option<JsonRpcResponse> {
    Some(match request.method.as_str() {
        "ui.state" => debug_state::handle_ui_state(state, engine, id),
        // settings cascade 는 headless 에서도 유효 — gui 게이트 없이 둔다 (ui.state 선례).
        // 둘 다 `debug_state` 에 있다: debug 전용이면서 gui 심볼을 안 쓰는 핸들러 모듈
        // (`debug` 는 gui 게이트라 headless 에서 통째로 사라진다).
        "debug.settings.apply" => {
            debug_state::handle_debug_settings_apply(state, engine, id, &request.params)
        }
        // GPU 결함 주입 — 다음 프레임의 present 를 인위적으로 블로킹해 "이벤트 펌프가
        // 통째로 멎는다" 는 구조와 stall 워치독 발화를 재현 검증한다. release 미노출.
        #[cfg(feature = "gui")]
        "debug.gpu.stall" => debug::handle_debug_gpu_stall(id, &request.params),
        // 아래 셋은 터미널 그리드만 본다 — gui 게이트 없이 `debug_terminal` 모듈에
        // 있고 헤드리스 debug 데몬에도 등록된다(그 모듈 doc).
        "debug.cell_info" => debug_terminal::handle_debug_cell_info(engine, id, &request.params),
        "debug.screen_attrs" => {
            debug_terminal::handle_debug_screen_attrs(engine, id, &request.params)
        }
        "debug.glyph_color" => {
            debug_terminal::handle_debug_glyph_color(engine, id, &request.params)
        }
        "debug.feed_bytes" => debug_terminal::handle_debug_feed_bytes(engine, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.inject_mouse" => debug::handle_debug_inject_mouse(engine, id, &request.params),
        #[cfg(feature = "gui")]
        "debug.inject_key" => debug::handle_debug_inject_key(engine, id, &request.params),
        // OS 전역 입력 상태 조작 (macOS) — 사용자 입력 재현이라 debug 격리.
        // 이름은 `surface.*` 이지만 대상 surface 를 받지 못한다(CGEvent/TIS 가
        // OS 전역에 나간다). 자세한 근거는 docs/adr/0115-input-reproduction-ipc-debug-isolation.md.
        #[cfg(all(target_os = "macos", feature = "gui"))]
        "surface.switch_input_source" => {
            input_source::handle_switch_input_source(state, engine, id, &request.params)
        }
        #[cfg(all(target_os = "macos", feature = "gui"))]
        "surface.raw_key" => input_source::handle_raw_key(state, engine, id, &request.params),
        // 위 둘의 짝. 이 플랫폼·조합에서 **왜** 못 하는지를 말한다.
        //
        // 등재(`DEBUG_METHODS`)와 CLI 서브커맨드는 플랫폼 조건이 없다 — 이 저장소에서
        // 그 두 층은 플랫폼 균일하고(실측: 두 파일에 `target_os` 게이트 0 건) 차이는
        // 여기 dispatch 층에 둔다. 그래서 arm 이 없으면 `tasty debug raw-key` 가
        // 도움말에 뜨는데 `-32601`("그런 메서드 없음")로 끝난다 — 메서드는 있고
        // 이 플랫폼이 못 할 뿐이라 그 답은 거짓이다.
        #[cfg(not(all(target_os = "macos", feature = "gui")))]
        "surface.switch_input_source" | "surface.raw_key" => {
            JsonRpcResponse::error(id.clone(), -32015, PLATFORM_ONLY_MACOS_GUI)
        }
        // 아래 셋은 gui feature 게이트가 없다 — 핸들러 본체가 gui 전용 필드를
        // 하나도 안 만져서 headless debug 데몬에도 등록된다(`debug_nav` 모듈 doc).
        "debug.close_workspace" => {
            debug_nav::handle_debug_close_workspace(state, engine, id, &request.params)
        }
        "debug.switch_workspace" => {
            debug_nav::handle_debug_switch_workspace(state, engine, id, &request.params)
        }
        "debug.switch_tab" => {
            debug_nav::handle_debug_switch_tab(state, engine, id, &request.params)
        }
        // 도구 메뉴 — 사용자 클릭 자동화. release 미노출.
        #[cfg(feature = "gui")]
        "debug.tool.list" => tool::handle_list(state, engine, id),
        #[cfg(feature = "gui")]
        "debug.tool.invoke" => tool::handle_invoke(state, engine, id, &request.params),
        // 호스트 빌트인 popup 직접 open/close — 사용자 클릭 경로 없이 시각 검증용.
        // release 미노출. (plugin popup 은 debug.popup.* 가 담당.)
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
        // modifier-hint 오버레이 홀드 주입 + 상태 덤프 — 사용자 modifier 홀드 우회 force-state.
        // release 미노출(원칙1: 오버레이는 실 홀드로만 표시).
        #[cfg(feature = "gui")]
        "debug.modifier_hint.hold" => {
            debug::handle_debug_modhint_hold(state, engine, id, &request.params)
        }
        #[cfg(feature = "gui")]
        "debug.modifier_hint.state" => debug::handle_debug_modhint_state(state, engine, id),
        // 배너 직접 발화/조회/닫기 — 사용자 조작 없이 시각 검증용. release 미노출.
        // 배너는 사용자 행동에서만 발사되므로(발화 정책 §불가침) 이 표면은 debug 전용.
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
        _ => return None,
    })
}

/// Extract a required surface_id from params. Returns Err(JsonRpcResponse) if missing,
/// out of u32 range, or outside the surface id space.
///
/// `>= PTY_ID_BASE` 는 headless PTY id 공간이라 실재하는 surface 가 가질 수 없는 값이다.
/// 통과시키면 `surface.meta.*` 등이 `Scope::Surface(pty id)` 를 memory.db 에 심고, 그
/// scope 가 다음 부팅의 surface 카운터 floor 를 PTY 공간으로 밀어 올린다
/// (`docs/adr/0094-surface-id-space-bounded-below-pty-base.md`). `as u32` 캐스팅도
/// `u32::try_from` 으로 바꿔 2^32 이상 값이 조용히 wrap 되지 않게 한다.
pub(super) fn require_surface_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    // 키가 없는 것과 값이 잘못된 것을 가른다 — 값이 왔는데 "missing" 이라고 답하면
    // 호출자가 자기가 준 값을 안 의심한다(`handler/params.rs`).
    let raw = match params::require_u32(params, "surface_id", id) {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    if !crate::core::pty_registry::is_surface_id_space(raw) {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!("'surface_id' {raw} is inside the headless PTY id space"),
        ));
    }
    Ok(raw)
}

/// Extract a required pane_id from params. Returns Err(JsonRpcResponse) if missing.
fn require_pane_id(
    params: &serde_json::Value,
    id: &serde_json::Value,
) -> Result<u32, JsonRpcResponse> {
    params::require_u32(params, "pane_id", id)
}

/// Extract optional caller_surface_id from params.
///
/// 오류를 버린다 — 이 값은 **부가 정보**(알림을 누구에게 돌려줄지)라 대상 선택에
/// 안 쓰이고, 여기서 거절하면 본 작업까지 막힌다. 다만 판정 자체는 공용 자리를
/// 지난다(자르기가 일어나지 않는다).
pub(super) fn caller_surface_id(params: &serde_json::Value) -> Option<u32> {
    params::read_int::<u32>(params, "caller_surface_id")
        .ok()
        .flatten()
}

/// Check if a surface belongs to a pane (directly or in any tab).
fn surface_belongs_to_pane(engine: &CoreState, surface_id: u32, pane_id: u32) -> bool {
    engine.find_pane_for_surface(surface_id) == Some(pane_id)
}

/// 구조변경 IPC 핸들러 공용 — `Core::apply` 가 반환한 에러를 JSON-RPC 응답으로
/// 변환한다. mirror(원격 attach client) 워크스페이스에서 forward 로 큐잉된 구조
/// op([`crate::core::MirrorStructuralBlocked`] `forwarded: true`)는 로컬 실행이
/// 거부됐지만 `pending_structural_forward` 에 실려 원격으로 전송돼 곧 실행된다 —
/// 이를 실패(`internal_error`)로 오보하지 않고 `{forwarded:true}` success 로
/// 회신한다. 원격 실행 결과는 비동기이며(역반영 delta 로 mirror 트리에 반영),
/// 호출자는 `list surfaces` 등으로 관측한다. forward 대상이 아닌 mirror 거부
/// (`forwarded:false`, 예: 워크스페이스 경계를 넘는 move-surface) 또는 일반 에러는 기존대로
/// internal_error 로 반환한다.
pub(super) fn structural_apply_error(id: serde_json::Value, e: &anyhow::Error) -> JsonRpcResponse {
    if let Some(blocked) = e.downcast_ref::<crate::core::MirrorStructuralBlocked>()
        && blocked.forwarded
    {
        return JsonRpcResponse::success(
            id,
            json!({
                "forwarded": true,
                "workspace_index": blocked.workspace_index,
            }),
        );
    }
    JsonRpcResponse::internal_error(id, e.to_string())
}

/// 구조 변경 도메인 실행(`core::structural_exec`)의 실패를 JSON-RPC 응답으로 바꾼다. 갈래가
/// 코드를 정하고 문구는 그대로 싣는다 — forward 실행이 같은 실패에서 받는 사유 문자열과
/// byte 단위로 같다.
pub(super) fn structural_failure_response(
    id: serde_json::Value,
    failure: crate::core::structural_exec::StructuralFailure,
) -> JsonRpcResponse {
    use crate::core::structural_exec::StructuralFailure;
    match failure {
        StructuralFailure::Rejected(msg) => JsonRpcResponse::invalid_params(id, msg),
        StructuralFailure::MissingEvent(msg) => JsonRpcResponse::internal_error(id, msg),
        StructuralFailure::Apply(e) => structural_apply_error(id, &e),
    }
}

/// `system.info` — engine 서술 + **이 서버가 협상할 수 있는 것의 목록**.
///
/// capability 는 `system_info_fields` 가 아니라 **여기**서 붙인다. 그 함수는
/// `window.list` 가 창마다 재사용하는데, capability 는 창의 성질이 아니라 서버의
/// 성질이라 창 수만큼 같은 배열이 실릴 이유가 없다.
///
/// 구 client 에 미치는 영향은 없다 — 모르는 키는 무시된다. 그래서 이것은 표면 **추가**
/// 이고 동결 baseline 가드와 부딪히지 않는다(그 가드가 보는 것은 메서드 이름 집합이다).
fn handle_system_info(
    window: &dyn IpcWindow,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let mut info = system_info_fields(window, engine);
    info["capabilities"] = tasty_ipc::capability::capabilities_json();
    // capability 목록은 "이 계약을 아는가" 만 답한다. 멱등 키는 그 위에 **얼마나**
    // 가 있고(보존 시간·항목 수·답 크기) 그 값을 모르면 호출자가 자기 재시도 간격이
    // 보장 안에 있는지 판단할 수 없다.
    info["idempotency"] = idempotency::declaration();
    JsonRpcResponse::success(id, info)
}

/// Version is process-wide; the legacy count/index describe this engine. Include
/// its workspace IDs so an observation never silently looks like a global count.
/// window.list reuses the same fields beside the OS window ID.
pub(crate) fn system_info_fields(window: &dyn IpcWindow, engine: &CoreState) -> serde_json::Value {
    let active_workspace = window.active_workspace_index();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "scope": "engine",
        "layout_slot": engine.layout_slot,
        "workspace_count": engine.workspaces.len(),
        "workspace_ids": engine.workspaces.iter().map(|ws| ws.id).collect::<Vec<_>>(),
        "active_workspace": active_workspace,
        "active_workspace_id": engine.workspaces.get(active_workspace).map(|ws| ws.id),
    })
}

fn handle_tree(
    window: &dyn IpcWindow,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!(build_engine_tree(window, engine)))
}

/// 한 (state, engine) 쌍의 워크스페이스 트리를 JSON 배열로 빌드한다.
///
/// IPC `list tree`(단일 라우팅 engine) 와 Lua 스냅샷(전 View/parked 통합, ADR-0031)이
/// **같은 구조**를 내도록 공유하는 빌더 — 노드 필드(active/busy_count/busy, panes/tabs/surface)가
/// 드리프트하지 않게 단일 소스로 유지한다.
pub(crate) fn build_engine_tree(
    window: &dyn IpcWindow,
    engine: &crate::core::CoreState,
) -> Vec<serde_json::Value> {
    engine
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let mut t = ws.to_tree_json();
            t["active"] = json!(i == window.active_workspace_index());
            t["busy_count"] = json!(engine.busy_count(&ws.all_surface_ids()));
            annotate_tree_busy(&mut t, engine);
            t
        })
        .collect()
}

/// Walk a workspace tree JSON value and annotate every node that owns surface
/// ids with a `busy_count` field. Surface-leaf nodes also get a `busy` boolean.
fn annotate_tree_busy(node: &mut serde_json::Value, engine: &CoreState) {
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

        // Recurse into children.
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

        // After children are annotated, sum descendant busy counts.
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
    engine: &CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let typing = engine.is_typing(surface_id);
    let idle_seconds = if let Some(last) = engine.last_key_input.get(&surface_id) {
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
    engine: &mut CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let text = match params.get("text").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing 'text' parameter"),
    };
    if engine.is_typing(surface_id) {
        return JsonRpcResponse::success(id, json!({ "sent": false, "reason": "typing" }));
    }
    engine.ensure_surface_initialized(surface_id);
    if let Some(terminal) = engine.find_terminal_by_id_mut(surface_id) {
        terminal.send_key(&text);
        JsonRpcResponse::success(id, json!({ "sent": true }))
    } else {
        JsonRpcResponse::invalid_params(id, format!("Surface {} not found", surface_id))
    }
}

#[cfg(test)]
mod structural_apply_error_tests {
    //! mirror 워크스페이스 구조 op forward 시 IPC 응답 정합성 회귀 방지.
    //! `forwarded:true`(원격으로 큐잉됨)를 실패로 오보하지 않고 success 로 회신한다.
    use super::structural_apply_error;

    #[test]
    fn forwarded_op_returns_success_not_error() {
        let err = anyhow::Error::new(crate::core::MirrorStructuralBlocked {
            workspace_index: 3,
            forwarded: true,
        });
        let resp = structural_apply_error(serde_json::json!(1), &err);
        assert!(
            resp.error.is_none(),
            "forward 로 큐잉된 op 는 에러로 회신하면 안 된다(원격 실행됨)"
        );
        let result = resp
            .result
            .expect("forwarded op 는 success result 를 가진다");
        assert_eq!(result["forwarded"], true);
        assert_eq!(result["workspace_index"], 3);
    }

    #[test]
    fn non_forwarded_mirror_block_stays_internal_error() {
        // forward 불가 op(워크스페이스 경계를 넘는 move-surface)의 mirror 거부는 기존대로 에러.
        let err = anyhow::Error::new(crate::core::MirrorStructuralBlocked {
            workspace_index: 0,
            forwarded: false,
        });
        let resp = structural_apply_error(serde_json::json!(1), &err);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.expect("internal_error").code, -32603);
    }

    #[test]
    fn plain_error_stays_internal_error() {
        let err = anyhow::anyhow!("some unrelated failure");
        let resp = structural_apply_error(serde_json::json!(1), &err);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.expect("internal_error").code, -32603);
    }
}

#[cfg(test)]
mod require_surface_id_tests {
    use super::require_surface_id;
    use crate::core::pty_registry::PTY_ID_BASE;
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
        // 과거 `as u32` 캐스팅은 이 값을 조용히 0 으로 wrap 시켰다.
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
        let (state, engine) = crate::state::tests::test_state();
        let info = system_info_fields(&state, &engine);
        assert_eq!(info["scope"], "engine");
        assert_eq!(info["workspace_count"], engine.workspaces.len());
        assert_eq!(info["active_workspace"], 0);
        assert_eq!(info["active_workspace_id"], engine.workspaces[0].id);
        assert_eq!(
            info["workspace_ids"],
            serde_json::json!([engine.workspaces[0].id])
        );
    }

    #[test]
    fn system_info_does_not_invent_an_active_workspace_for_an_empty_engine() {
        let (state, mut engine) = crate::state::tests::test_state();
        engine.workspaces.clear();
        let info = system_info_fields(&state, &engine);
        assert_eq!(info["workspace_count"], 0);
        assert_eq!(info["active_workspace"], state.active_workspace);
        assert!(info["active_workspace_id"].is_null());
        assert_eq!(info["workspace_ids"], serde_json::json!([]));
    }

    /// `system.info` 는 이 서버가 협상할 수 있는 것을 함께 답한다. 패키지 버전만으로는
    /// 그것을 못 묻는다 — 같은 버전의 두 빌드가 feature 조합에 따라 다른 것을 한다.
    #[test]
    fn system_info_declares_what_this_server_can_negotiate() {
        let (state, engine) = crate::state::tests::test_state();
        let resp = handle_system_info(&state, &engine, serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        let caps = result["capabilities"]
            .as_array()
            .expect("capabilities 가 배열로 실려야 한다");
        assert!(!caps.is_empty());
        for c in caps {
            assert!(c["name"].is_string(), "{c}");
            assert!(c["version"].is_u64(), "{c}");
        }
        // 기존 키는 그대로다 — 이것은 추가이지 교체가 아니다.
        assert_eq!(result["scope"], "engine");
        assert!(result["version"].is_string());
    }

    /// client 가 **보내기 전에** 묻는 그 이름이 실제로 선언돼 있는가. 이름이 빠지면
    /// 새 client 는 모든 서버를 구 서버로 보고 변경 명령을 통째로 거절한다 — 그 실패는
    /// 조용하지 않지만, 이름과 그것을 요구하는 자리가 **다른 크레이트**에 있어 컴파일러가
    /// 짝을 안 봐 준다.
    #[test]
    fn system_info_declares_the_capability_name_the_client_asks_for() {
        let (state, engine) = crate::state::tests::test_state();
        let resp = handle_system_info(&state, &engine, serde_json::json!(1));
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

    /// capability 목록은 "이 서버가 멱등 키를 읽는가" 까지만 답한다. 호출자가 자기
    /// 재시도 간격이 보장 안에 있는지 판단하려면 **값**이 필요하고, 그 값이 여기 실린다.
    #[test]
    fn system_info_declares_the_bounds_of_the_idempotency_guarantee() {
        let (state, engine) = crate::state::tests::test_state();
        let resp = handle_system_info(&state, &engine, serde_json::json!(1));
        let result = resp.result.expect("성공 응답이어야 한다");
        assert_eq!(result["idempotency"], super::idempotency::declaration());
        assert_eq!(result["idempotency"]["survives_restart"], false);
    }

    /// capability 는 **서버**의 성질이지 창의 성질이 아니다. `window.list` 가 창마다
    /// 재사용하는 공용 필드에 실리면 창 수만큼 같은 배열이 반복된다 — 그 자리가
    /// 갈라져 있다는 것을 여기서 잰다(두 함수가 한 몸이 되면 이 단정이 죽는다).
    #[test]
    fn the_per_window_fields_do_not_repeat_the_server_capabilities() {
        let (state, engine) = crate::state::tests::test_state();
        let shared = system_info_fields(&state, &engine);
        assert!(
            shared.get("capabilities").is_none(),
            "창마다 재사용되는 필드에 capability 가 실렸다: {shared}"
        );
    }
}
