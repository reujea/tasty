//! 요청 게이트의 저장 단계. 메인 루프는 writer 결과를 기다리지 않는다.
//!
//! 진입 경로는 저장이 필요한 단계마다 writer에 요청을 넣고 명령을 보관한다. 결과가 나오면 같은 진입
//! 함수로 다시 들어와 [`GateProgress`]에 남은 단계부터 진행한다. 실패 경계는 단계마다 따로 둔다
//! ([저장 계약](../../../../docs/design/systems/storage-contract.md)의 "요청 게이트" 절).
//!
//! 1. 세션 판정(토큰이 있을 때): 만료 세션 삭제와 만료 grant 정리를 함께 하는 복합 연산이다. 확정하지
//!    않았거나 결과를 모르면 같은 판정을 새 요청으로 다시 넣는다. 판정은 다시 실행해도 같다. 세 번 모두
//!    실패하면 쓰기 없는 강한 읽기로 판정하고, 그 읽기도 실패하면 조회 실패로 거절한다.
//! 2. 조회: plugin의 cap 목록과 `split`의 nickname 대상을 강한 읽기 하나로 읽는다. 강한 읽기는 혼자
//!    실행돼 다른 요청의 실패와 함께 되돌려지지 않는다. cap을 읽지 못하면 차단하지 않는다(fail-open).
//! 3. 권한·cap 판정: 저장소를 보지 않는다.
//! 4. 기록: rate limit 소비, ipc_calls 기록, cap 재평가를 복합 연산 하나로 한다. 이 요청이 확정되지
//!    않으면 요청을 허용하고 소비·기록은 남지 않는다(fail-open). 실패를 거절로 바꾸지 않는다.

use serde_json::{Value, json};
use tasty_ipc::SessionResolution;
use tasty_ipc::caller::{CallerContext, EnvelopeCaller, SessionToken};
use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use tasty_memory::writer::{MemoryCompletion, MemoryRequest, MemoryTicket, settle};
use tasty_telemetry::{CostCap, GateRefusal, TelemetryEvent};
use tasty_utils::batch_writer::SubmitError;

use super::CheckedRequest;
use super::telemetry::{self, TriggeredCap};
use crate::app::services::AppServices;
use crate::app::services::session::{SessionJudgement, lookup_failed, peek_in, resolve_in};
use crate::runtime::engine_access::EngineMut;
use crate::state::RequestContext;

/// 세션 판정 복합 연산을 넣는 최대 횟수. 그 뒤에는 쓰기 없는 읽기로 판정한다.
pub(crate) const SESSION_ATTEMPTS: u8 = 3;

/// 시험 경로가 결과를 기다리는 상한. 운영 경로는 기다리지 않는다.
#[cfg(test)]
const TEST_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// 단계가 저장 결과를 기다리는지.
pub(crate) enum Step<T> {
    Ready(T),
    Pending,
}

/// 요청 하나의 게이트 진행. 진입 함수가 다시 들어올 때 넘긴다.
#[derive(Default)]
pub(crate) struct GateProgress {
    claimed: bool,
    session: Option<SessionFetch>,
    session_result: Option<Result<SessionResolution, SubmitError>>,
    caller: Option<CallerContext>,
    lookups: Option<LookupFetch>,
    looked_up: Option<Lookups>,
    rewritten: bool,
    judged: bool,
    record: Option<RecordFetch>,
    outcome: Option<RecordOutcome>,
    /// plugin 정지 요청이면 대상 plugin에 바쁜지 물은 것(`plugin_busy`). 한 번만 묻는다.
    busy_asked: bool,
    busy: Option<super::plugin_busy::BusyProbe>,
}

impl GateProgress {
    /// 처음 부를 때만 true다. 큐 기한 판정처럼 한 번만 할 일을 지킨다.
    pub(crate) fn claim_once(&mut self) -> bool {
        !std::mem::replace(&mut self.claimed, true)
    }

    /// 기다리는 저장 결과를 거둔다. 기다리는 것이 없거나 모두 나왔으면 true다.
    pub(crate) fn poll(&mut self, core: &AppServices) -> bool {
        self.poll_with(core, false)
    }

    /// plugin 정지 요청이면 대상 plugin에 바쁜지 묻기 시작한다. 물었으면 `Pending`이고 요청을 줄에 둔다.
    /// 권한 게이트보다 먼저 부르고, 답은 [`Self::busy_refusal`]로 권한 게이트 뒤에 읽는다.
    pub(crate) fn ask_busy(
        &mut self,
        plugins: Option<&mut crate::plugin::PluginManager>,
        request: &JsonRpcRequest,
    ) -> Step<()> {
        if std::mem::replace(&mut self.busy_asked, true) {
            return Step::Ready(());
        }
        self.busy = super::plugin_busy::begin(plugins, request);
        if self.busy.as_mut().is_some_and(|probe| !probe.poll(false)) {
            Step::Pending
        } else {
            Step::Ready(())
        }
    }

    #[cfg(test)]
    pub(crate) fn with_busy_probe(probe: super::plugin_busy::BusyProbe) -> Self {
        Self {
            busy_asked: true,
            busy: Some(probe),
            ..Self::default()
        }
    }

    /// 바쁘다고 답한 plugin이 있으면 거절 응답. 권한 게이트를 통과한 뒤에 부른다.
    pub(crate) fn busy_refusal(&mut self, id: serde_json::Value) -> Option<JsonRpcResponse> {
        let probe = self.busy.take()?;
        if probe.busy().is_empty() {
            return None;
        }
        tracing::info!("plugin stop refused: busy {:?}", probe.busy());
        Some(super::plugin_busy::busy_refusal(id, probe.busy()))
    }

    fn poll_with(&mut self, core: &AppServices, wait: bool) -> bool {
        if let Some(probe) = self.busy.as_mut()
            && !probe.poll(wait)
        {
            return false;
        }
        if let Some(fetch) = self.session.as_mut() {
            match fetch.poll(core, wait) {
                Some(result) => {
                    self.session = None;
                    self.session_result = Some(result);
                }
                None => return false,
            }
        }
        if let Some(fetch) = self.lookups.as_mut() {
            match fetch.poll(wait) {
                Some(lookups) => {
                    self.lookups = None;
                    self.looked_up = Some(lookups);
                }
                None => return false,
            }
        }
        if let Some(fetch) = self.record.as_mut() {
            match fetch.poll(wait) {
                Some(outcome) => {
                    self.record = None;
                    self.outcome = Some(outcome);
                }
                None => return false,
            }
        }
        true
    }
}

/// writer가 게이트 요청을 받지 않을 때의 응답. 세션을 판정할 수 없어 통과시키지 않는다.
pub(crate) fn gate_unavailable(id: Value, error: &SubmitError) -> JsonRpcResponse {
    match error {
        SubmitError::Closed => shutting_down(id),
        SubmitError::Saturated { .. } => JsonRpcResponse::error_with_data(
            id,
            -32000,
            format!("host is busy: the request gate could not reach the memory writer ({error})"),
            json!({ "storage_failure": "busy", "writer": "saturated" }),
        ),
    }
}

/// 종료가 시작된 뒤 게이트를 기다리던 요청의 응답. 종료 중 새 요청의 응답과 같다.
pub(crate) fn shutting_down(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::error(id, -32000, "host is shutting down")
}

fn take_completion(ticket: &mut MemoryTicket, wait: bool) -> Option<MemoryCompletion> {
    #[cfg(test)]
    if wait {
        return ticket.wait_timeout(TEST_WAIT);
    }
    debug_assert!(!wait, "only tests wait for a gate result");
    ticket.try_take()
}

/// 봉투로 호출자를 정한다. 세션 토큰이 있으면 writer 판정이 끝날 때까지 `Pending`이다.
pub(crate) fn resolve_caller(
    core: &AppServices,
    request: &JsonRpcRequest,
    progress: &mut GateProgress,
) -> Step<Result<CallerContext, JsonRpcResponse>> {
    resolve_caller_with(core, request, progress, false)
}

fn resolve_caller_with(
    core: &AppServices,
    request: &JsonRpcRequest,
    progress: &mut GateProgress,
    wait: bool,
) -> Step<Result<CallerContext, JsonRpcResponse>> {
    loop {
        if !progress.poll_with(core, wait) {
            return Step::Pending;
        }
        if let Some(caller) = &progress.caller {
            return Step::Ready(Ok(caller.clone()));
        }
        let id = request.id.clone().unwrap_or(Value::Null);
        if let Some(result) = progress.session_result.take() {
            let caller = match result {
                Ok(resolution) => tasty_ipc::caller::caller_from_session(request, resolution),
                Err(error) => Err(gate_unavailable(id, &error)),
            };
            if let Ok(caller) = &caller {
                progress.caller = Some(caller.clone());
            }
            return Step::Ready(caller);
        }
        let token = match tasty_ipc::caller::envelope_caller(request) {
            Err(response) => return Step::Ready(Err(response)),
            Ok(EnvelopeCaller::Local(caller)) => {
                progress.caller = Some(caller.clone());
                return Step::Ready(Ok(caller));
            }
            Ok(EnvelopeCaller::Session(token)) => token,
        };
        let Ok(token) = token.parse::<SessionToken>() else {
            return Step::Ready(tasty_ipc::caller::caller_from_session(
                request,
                SessionResolution::NotFound,
            ));
        };
        match SessionFetch::begin(core, token, telemetry::now_ms()) {
            Ok(fetch) => progress.session = Some(fetch),
            Err(error) => return Step::Ready(Err(gate_unavailable(id, &error))),
        }
    }
}

/// 판정에 필요한 저장 값을 읽는다. `split`의 nickname 대상을 찾았으면 요청의 대상을 그 surface ID로
/// 바꾼다. 이후 경로는 접수 때 읽은 대상을 쓴다.
pub(crate) fn prepare(
    core: &AppServices,
    request: &mut JsonRpcRequest,
    caller: &CallerContext,
    progress: &mut GateProgress,
) -> Step<()> {
    prepare_with(core, request, caller, progress, false)
}

fn prepare_with(
    core: &AppServices,
    request: &mut JsonRpcRequest,
    caller: &CallerContext,
    progress: &mut GateProgress,
    wait: bool,
) -> Step<()> {
    loop {
        if !progress.poll_with(core, wait) {
            return Step::Pending;
        }
        if let Some(lookups) = &progress.looked_up {
            if !std::mem::replace(&mut progress.rewritten, true)
                && let Some((nick, sid)) = &lookups.nickname
                && let Some(target) = request.params.get_mut("target_surface")
                && target.as_str() == Some(nick.as_str())
            {
                *target = json!(sid);
            }
            return Step::Ready(());
        }
        let wants_caps = caller.is_plugin();
        let nickname = nickname_wanted(request);
        if !wants_caps && nickname.is_none() {
            progress.looked_up = Some(Lookups::default());
            continue;
        }
        match LookupFetch::begin(core, wants_caps, nickname) {
            Ok(fetch) => progress.lookups = Some(fetch),
            Err(error) => {
                tracing::warn!(%error, "request gate lookups were not accepted; caps are not enforced for this call");
                progress.looked_up = Some(Lookups::default());
            }
        }
    }
}

/// 숫자·숫자 문자열 대상은 저장소를 보지 않는다. nickname으로 대상을 고르는 메서드는 `split`뿐이다.
fn nickname_wanted(request: &JsonRpcRequest) -> Option<String> {
    if crate::ipc::alias::canonicalize(&request.method) != "split" {
        return None;
    }
    let nick = request.params.get("target_surface")?.as_str()?;
    (!nick.is_empty() && nick.parse::<u32>().is_err()).then(|| nick.to_string())
}

/// 권한·cap을 판정하고 통과하면 기록 단계를 진행한다. 판정은 요청마다 한 번만 한다.
pub(crate) fn decide<'a>(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
    progress: &mut GateProgress,
) -> Step<Result<CheckedRequest<'a>, JsonRpcResponse>> {
    decide_with(core, state, engine, request, caller, progress, false)
}

fn decide_with<'a>(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
    progress: &mut GateProgress,
    wait: bool,
) -> Step<Result<CheckedRequest<'a>, JsonRpcResponse>> {
    loop {
        if !progress.poll_with(core, wait) {
            return Step::Pending;
        }
        if let Some(outcome) = progress.outcome.take() {
            return Step::Ready(in_scope(core, state, engine, |core, window, engine| {
                admit(core, window, engine, request, caller, outcome)
            }));
        }
        if std::mem::replace(&mut progress.judged, true) {
            // 기록 결과를 이미 거둔 요청이 다시 들어오지 않는 한 닿지 않는다.
            progress.outcome = Some(RecordOutcome::passed());
            continue;
        }
        let caps = progress
            .looked_up
            .as_ref()
            .and_then(|lookups| lookups.caps.as_deref());
        let judged = in_scope(core, state, engine, |core, window, engine| {
            judge(core, window, engine, request, caller, caps)
        });
        match judged {
            Err(response) => return Step::Ready(Err(response)),
            Ok(None) => progress.outcome = Some(RecordOutcome::passed()),
            Ok(Some(plan)) => match RecordFetch::begin(core, plan) {
                Ok(fetch) => progress.record = Some(fetch),
                Err(error) => {
                    tracing::warn!(%error, "request gate record was not accepted; the call proceeds without rate limit consumption or telemetry");
                    progress.outcome = Some(RecordOutcome::passed());
                }
            },
        }
    }
}

/// 게이트의 표시 출력(알림·승인 요청)을 요청 범위로 모아 같은 회차에 적용한다.
fn in_scope<T>(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    body: impl FnOnce(
        &mut AppServices,
        &mut dyn crate::ipc::window_port::IpcWindow,
        &mut EngineMut<'_>,
    ) -> T,
) -> T {
    let mut scope = crate::ipc::request_scope::RequestScope::capture(
        state,
        engine.core,
        #[cfg(feature = "gui")]
        None,
    );
    let result = body(core, &mut scope, engine);
    let outputs = scope.finish();
    outputs.apply(state, engine.core);
    result
}

/// 거부 사유별 대응이 달라 따로 센다(docs/architecture/ipc-server.md). 첫 거부에서 반환하므로 중복
/// 집계하지 않는다.
fn judge(
    core: &mut AppServices,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut EngineMut<'_>,
    request: &JsonRpcRequest,
    caller: &CallerContext,
    caps: Option<&[CostCap]>,
) -> Result<Option<RecordPlan>, JsonRpcResponse> {
    let canonical = crate::ipc::alias::canonicalize(&request.method);
    let id = request.id.clone().unwrap_or(Value::Null);
    let ws = engine
        .workspace_at(window.active_workspace_index(engine))
        .map(|w| w.id);
    core.gate().record_judged();
    if let Some(response) =
        super::check_permission_gate(core, window, engine, caller, canonical, ws, &id)
    {
        core.gate().record_refusal(GateRefusal::Permission);
        return Err(response);
    }
    if let Some(response) = super::check_cap_gate(core, caller, canonical, ws, &id, caps) {
        core.gate().record_refusal(GateRefusal::Cap);
        return Err(response);
    }
    let now_ms = telemetry::now_ms();
    let throttle =
        super::should_rate_limit(caller, canonical).then(|| caller.agent_id().as_str().to_string());
    let event = telemetry::ipc_call_event(
        caller,
        canonical,
        engine.workspace_at(0).map(|w| w.id),
        now_ms,
    )
    .map(|ev| {
        let key = telemetry::reserve_event_key(core, &ev);
        (ev, key)
    });
    if throttle.is_none() && event.is_none() {
        return Ok(None);
    }
    Ok(Some(RecordPlan {
        throttle,
        event,
        now_ms,
    }))
}

/// 기록 결과를 반영한다. 한도를 넘었으면 거절하고, 통과했으면 cap 액션과 이상 탐지를 실행한다.
fn admit<'a>(
    core: &mut AppServices,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut EngineMut<'_>,
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
    outcome: RecordOutcome,
) -> Result<CheckedRequest<'a>, JsonRpcResponse> {
    let canonical = crate::ipc::alias::canonicalize(&request.method);
    let id = request.id.clone().unwrap_or(Value::Null);
    let ws = engine
        .workspace_at(window.active_workspace_index(engine))
        .map(|w| w.id);
    match outcome {
        RecordOutcome::Throttled { tokens_left } => {
            let reason = format!("throttled: tokens_left={tokens_left:.2}");
            tracing::warn!("ipc rate_limited: {reason}");
            let seq = core.telemetry_seq.next();
            crate::ipc::audit::record(
                core,
                caller,
                canonical,
                crate::store::audit::AuditDecision::Deny,
                Some(&reason),
                ws,
                seq,
            );
            core.gate().record_refusal(GateRefusal::Throttle);
            return Err(JsonRpcResponse::error(id, -32010, reason));
        }
        RecordOutcome::Passed {
            recorded,
            triggered,
        } => {
            // 게이트에서 만든 intent가 같은 요청의 handler intent보다 먼저 큐에 들어간다.
            let mut out = crate::ipc::window_port::IntentOutbox::default();
            telemetry::fire_triggered_caps(core, window, &mut out, engine, &triggered);
            if let Some(ev) = &recorded {
                telemetry::observe_ipc_call(
                    core,
                    window,
                    &mut out,
                    engine,
                    ev,
                    canonical,
                    &request.params,
                );
                core.submit_log_prune_if_due(ev.ts);
            }
            window.enqueue_intents(out);
            let seq = core.telemetry_seq.next();
            crate::ipc::audit::record(
                core,
                caller,
                canonical,
                crate::store::audit::AuditDecision::Allow,
                None,
                ws,
                seq,
            );
        }
    }
    // 멱등성 키는 모든 라우터에 앞서 검사한다. 권한 검사를 먼저 거쳐
    // 권한 없는 호출에는 `-32001`을 반환하고, 허용된 호출은 한 번 집계한다(docs/dev-guide/api-conventions.md).
    super::idempotency::check_envelope(request, &id)?;
    Ok(CheckedRequest::admitted(request, caller))
}

/// 세션 판정 요청의 결과를 기다린다.
struct SessionFetch {
    token: SessionToken,
    now_ms: u64,
    attempts: u8,
    peek: bool,
    ticket: MemoryTicket,
}

impl SessionFetch {
    fn begin(core: &AppServices, token: SessionToken, now_ms: u64) -> Result<Self, SubmitError> {
        let ticket = Self::submit(core, &token, now_ms, false)?;
        Ok(Self {
            token,
            now_ms,
            attempts: 1,
            peek: false,
            ticket,
        })
    }

    fn submit(
        core: &AppServices,
        token: &SessionToken,
        now_ms: u64,
        peek: bool,
    ) -> Result<MemoryTicket, SubmitError> {
        let token = token.clone();
        let request = if peek {
            MemoryRequest::read(0, move |s| peek_in(s, &token, now_ms))
        } else {
            MemoryRequest::unit(0, move |s| resolve_in(s, &token, now_ms))
        };
        core.memory_access().submit(request)
    }

    /// 판정이 끝났으면 결과를 돌려준다. `Err`는 다시 넣은 요청을 writer가 받지 않은 경우다.
    fn poll(
        &mut self,
        core: &AppServices,
        wait: bool,
    ) -> Option<Result<SessionResolution, SubmitError>> {
        loop {
            let completion = take_completion(&mut self.ticket, wait)?;
            let failure = match settle(completion) {
                Ok(output) => {
                    return Some(Ok(output.into_value::<SessionJudgement>().map_or(
                        SessionResolution::NotFound,
                        SessionJudgement::into_resolution,
                    )));
                }
                Err(failure) => failure,
            };
            if self.peek {
                return Some(Ok(lookup_failed(format!("{failure:?}"))));
            }
            let peek = self.attempts >= SESSION_ATTEMPTS;
            tracing::debug!(
                attempt = self.attempts,
                peek,
                "session judgement was not settled ({failure:?}); submitting again"
            );
            match Self::submit(core, &self.token, self.now_ms, peek) {
                Ok(ticket) => {
                    self.ticket = ticket;
                    self.peek = peek;
                    self.attempts += 1;
                }
                Err(error) => return Some(Err(error)),
            }
        }
    }
}

/// 판정에 쓰는 저장 값. 읽지 못한 값은 None이다.
#[derive(Default)]
struct Lookups {
    caps: Option<Vec<CostCap>>,
    nickname: Option<(String, u32)>,
}

struct LookupBody {
    caps: Option<Result<Vec<CostCap>, String>>,
    nickname: Option<(String, Option<u32>)>,
}

struct LookupFetch {
    ticket: MemoryTicket,
}

impl LookupFetch {
    fn begin(
        core: &AppServices,
        wants_caps: bool,
        nickname: Option<String>,
    ) -> Result<Self, SubmitError> {
        let bytes = nickname.as_ref().map_or(0, String::len);
        let ticket = core
            .memory_access()
            .submit(MemoryRequest::read(bytes, move |s| {
                Ok(LookupBody {
                    caps: wants_caps.then(|| telemetry::load_all_caps(s)),
                    nickname: nickname.as_ref().map(|nick| {
                        (
                            nick.clone(),
                            crate::surface_meta::SurfaceMetaStore::find_by_value(
                                s, "nickname", nick,
                            ),
                        )
                    }),
                })
            }))?;
        Ok(Self { ticket })
    }

    fn poll(&mut self, wait: bool) -> Option<Lookups> {
        let completion = take_completion(&mut self.ticket, wait)?;
        let body = match settle(completion) {
            Ok(output) => output.into_value::<LookupBody>(),
            Err(failure) => {
                tracing::warn!(
                    "request gate lookups failed; caps are not enforced for this call: {failure:?}"
                );
                None
            }
        };
        let Some(body) = body else {
            return Some(Lookups::default());
        };
        let caps = match body.caps {
            Some(Ok(caps)) => Some(caps),
            Some(Err(e)) => {
                tracing::warn!("cap block check: load failed: {e}");
                None
            }
            None => None,
        };
        let nickname = body
            .nickname
            .and_then(|(nick, sid)| sid.map(|sid| (nick, sid)));
        Some(Lookups { caps, nickname })
    }
}

/// 판정을 통과한 요청이 기록할 것.
struct RecordPlan {
    throttle: Option<String>,
    event: Option<(TelemetryEvent, String)>,
    now_ms: u64,
}

enum RecordBody {
    Throttled {
        tokens_left: f64,
    },
    Passed {
        recorded: bool,
        triggered: Vec<TriggeredCap>,
        warnings: Vec<String>,
    },
}

/// 기록 단계의 결과.
pub(crate) enum RecordOutcome {
    Throttled {
        tokens_left: f64,
    },
    Passed {
        recorded: Option<TelemetryEvent>,
        triggered: Vec<TriggeredCap>,
    },
}

impl RecordOutcome {
    fn passed() -> Self {
        Self::Passed {
            recorded: None,
            triggered: Vec::new(),
        }
    }
}

struct RecordFetch {
    ticket: MemoryTicket,
    event: Option<TelemetryEvent>,
}

impl RecordFetch {
    fn begin(core: &AppServices, plan: RecordPlan) -> Result<Self, SubmitError> {
        let RecordPlan {
            throttle,
            event,
            now_ms,
        } = plan;
        let recorded = event.as_ref().map(|(ev, _)| ev.clone());
        let bytes = event
            .as_ref()
            .map_or(0, |(ev, key)| key.len() + ev.metric.len() + ev.agent.len());
        let ticket = core
            .memory_access()
            .submit(MemoryRequest::unit(bytes, move |s| {
                let mut warnings = Vec::new();
                if let Some(agent) = &throttle {
                    match crate::app::services::ratelimit::try_consume(
                        s,
                        agent,
                        "ipc_calls",
                        1,
                        now_ms,
                    ) {
                        Ok(outcome) if !outcome.allowed => {
                            return Ok(RecordBody::Throttled {
                                tokens_left: outcome.tokens_left,
                            });
                        }
                        Ok(_) => {}
                        // fail-open: rate_limit 인프라 자체 실패는 전체 IPC 차단보다 통과 + warn.
                        Err(e) => warnings.push(format!("rate_limit middleware error: {e}")),
                    }
                }
                let Some((ev, key)) = &event else {
                    return Ok(RecordBody::Passed {
                        recorded: false,
                        triggered: Vec::new(),
                        warnings,
                    });
                };
                if let Err(e) = telemetry::put_event(s, ev, key) {
                    warnings.push(format!("telemetry middleware: record failed: {e}"));
                    return Ok(RecordBody::Passed {
                        recorded: false,
                        triggered: Vec::new(),
                        warnings,
                    });
                }
                let triggered = telemetry::trigger_caps_in(s, ev, &mut warnings);
                Ok(RecordBody::Passed {
                    recorded: true,
                    triggered,
                    warnings,
                })
            }))?;
        Ok(Self {
            ticket,
            event: recorded,
        })
    }

    fn poll(&mut self, wait: bool) -> Option<RecordOutcome> {
        let completion = take_completion(&mut self.ticket, wait)?;
        let body = match settle(completion) {
            Ok(output) => output.into_value::<RecordBody>(),
            Err(failure) => {
                tracing::warn!(
                    "request gate record was not stored; the call proceeds without rate limit consumption or telemetry: {failure:?}"
                );
                None
            }
        };
        Some(match body {
            Some(RecordBody::Throttled { tokens_left }) => RecordOutcome::Throttled { tokens_left },
            Some(RecordBody::Passed {
                recorded,
                triggered,
                warnings,
            }) => {
                for warning in warnings {
                    tracing::warn!("{warning}");
                }
                RecordOutcome::Passed {
                    recorded: if recorded { self.event.take() } else { None },
                    triggered,
                }
            }
            None => RecordOutcome::passed(),
        })
    }
}

/// 게이트 전체를 결과를 기다리며 실행한다. 시험만 쓴다. 호출자는 이미 정해져 있고, `split`의
/// nickname 대상 바꾸기는 하지 않는다.
#[cfg(test)]
pub(crate) fn check_request<'a>(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
) -> Result<CheckedRequest<'a>, JsonRpcResponse> {
    let mut progress = GateProgress {
        caller: Some(caller.clone()),
        ..GateProgress::default()
    };
    let mut probe = request.clone();
    while let Step::Pending = prepare_with(core, &mut probe, caller, &mut progress, true) {}
    loop {
        if let Step::Ready(result) =
            decide_with(core, state, engine, request, caller, &mut progress, true)
        {
            return result;
        }
    }
}

/// 봉투의 호출자를 결과를 기다려 정한다. 시험만 쓴다.
#[cfg(test)]
pub(crate) fn resolve_caller_waiting(
    core: &AppServices,
    request: &JsonRpcRequest,
) -> Result<CallerContext, JsonRpcResponse> {
    let mut progress = GateProgress::default();
    loop {
        if let Step::Ready(result) = resolve_caller_with(core, request, &mut progress, true) {
            return result;
        }
    }
}

/// 세션 토큰으로 게이트가 정한 권한 토큰. 세션이 agent로 풀리지 않으면 None이다.
#[cfg(test)]
pub(crate) fn session_permissions_waiting(core: &AppServices, token: &str) -> Option<Vec<String>> {
    let request = JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "system.info".to_string(),
        params: json!({}),
        session_token: Some(token.to_string()),
    };
    match resolve_caller_waiting(core, &request).ok()? {
        CallerContext::Agent { permissions, .. } => {
            Some(permissions.iter().map(|p| p.as_token()).collect())
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
