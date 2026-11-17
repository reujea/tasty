//! 멱등성 키가 같은 변경 요청을 다시 실행하지 않고 보관한 응답을 반환한다.
//! JSON-RPC id와 같은 params만으로는 재시도를 구분할 수 없으므로 호출자가 키를 지정한다.
//!
//! 호스트가 아는 Mutate 메서드만 대상이다. 읽기·멱등 메서드는 매번 현재 상태를 처리하고,
//! 플러그인 고유 메서드의 중복 방지는 해당 플러그인에 맡긴다. 지원 범위는 KeyContract로 알린다.
//! engine 라우터는 begin/finish, App 및 GUI debug 라우터는 run_app_layer,
//! 플러그인 전달은 forward_keeping_the_key를 사용한다.
//!
//! engine 라우터는 직렬 실행한다. 나중에 응답하는 App·플러그인 요청은 진행 중 항목을 남겨
//! 같은 키의 재시도가 기존 실행의 결과를 기다리게 한다. 오류 응답도 보관한다.
//! 따라서 플러그인 전달이 실행 전에 거절됐어도 보관 기간 안의 재시도에는 같은 거절을 반환한다.
//!
//! 보관 시간·항목 수·총 바이트에 한도가 있고 재시작 시 모두 사라진다.
//! 키가 만료·퇴출되면 새 요청으로 실행한다. 너무 큰 응답만 버린 경우에는 키를 유지하고
//! ERR_IDEMPOTENT_RESULT_DISCARDED로 결과를 조회하도록 안내한다.
//! declaration이 system.info에 이 한계를 제공한다.

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

use tasty_ipc::caller::CallerContext;
use tasty_ipc::method_meta::{MethodEffect, method_meta};
use tasty_ipc::protocol::{
    ERR_IDEMPOTENCY_KEY_CONFLICT, ERR_IDEMPOTENT_RESULT_DISCARDED,
    ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN, JsonRpcRequest, JsonRpcResponse,
};
use tasty_ipc::server::{IpcCommand, send_response};

/// 보관 시간은 재연결·백오프와 수동 재시도를 고려해 분 단위로 둔다.
/// 더 길게 둘 측정 근거는 없으며, 늘려도 용량 상한 때문에 먼저 지워질 수 있다.
pub(crate) const RETENTION: Duration = Duration::from_secs(600);

/// 동시에 기억하는 키의 수.
pub(crate) const CAPACITY: usize = 256;

/// 응답 하나의 최대 보관 크기. 초과하면 키만 남겨 중복 실행을 막는다.
pub(crate) const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024;

/// 모든 응답의 총 크기 한도. 항목 수와 별도로 제한한다.
pub(crate) const TOTAL_BUDGET_BYTES: usize = 4 * 1024 * 1024;

/// 호출자가 정하는 키도 바이트 크기를 제한한다.
pub(crate) const MAX_KEY_BYTES: usize = 256;

/// 보관 중인 요청의 결과 또는 진행 상태.
#[derive(Debug)]
enum Stored {
    /// 답을 그대로 들고 있다.
    Response(Box<JsonRpcResponse>),
    /// 요청은 처리했지만 응답이 너무 커서 보관하지 않았다.
    Discarded,
    /// 응답을 기다리는 중. run_app_layer를 사용하는 비동기 경로에서 생긴다.
    InFlight {
        /// 늦게 도착한 응답이 같은 키의 새 실행을 덮지 않도록 실행을 구별한다.
        ticket: u64,
        /// 같은 실행의 결과를 기다리는 재시도 요청.
        waiters: Vec<Waiter>,
    },
}

/// 같은 키의 결과를 기다리는 요청 ID와 응답 채널.
#[derive(Debug)]
struct Waiter {
    id: serde_json::Value,
    reply: mpsc::SyncSender<JsonRpcResponse>,
}

/// 결과를 기다리던 재시도 요청들에 전달할 응답.
#[derive(Debug, Default)]
struct Handoff {
    waiters: Vec<Waiter>,
    /// 답을 보관했는가(거짓이면 상한을 넘어 버렸다).
    kept: bool,
}

#[derive(Debug)]
struct Entry {
    /// 키를 고른 주체. [`caller_scope`] 참조.
    scope: String,
    key: String,
    /// 같은 키에 다른 요청을 보내는지 검사할 메서드·params의 다이제스트.
    /// 큰 params 원문을 저장하지 않는 대신 64비트 해시 충돌 가능성은 남는다.
    digest: u64,
    stored_at: Instant,
    outcome: Stored,
    /// 이 항목이 [`TOTAL_BUDGET_BYTES`] 에서 차지하는 몫.
    bytes: usize,
}

/// 키 하나에 대한 판정.
#[derive(Debug)]
pub(crate) enum Decision {
    /// 처음 보는 키다. 실행하고 이 다이제스트로 기록한다.
    Execute(u64),
    /// 같은 키·같은 요청의 답이 있다. **실행하지 않는다.**
    Replay(Box<JsonRpcResponse>),
    /// 같은 키인데 요청이 다르다.
    Conflict,
    /// 실행은 됐고 답은 버려졌다.
    Discarded,
    /// 같은 키·같은 요청의 실행이 **아직 끝나지 않았다.**
    InFlight,
}

/// 키를 지정한 요청의 판정별 누계. 실행 수와 재사용 수를 함께 비교한다.
/// 호출자가 메서드·키·ID별로 집계 항목을 무한히 늘리지 못하도록 별도 레이블은 두지 않는다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RetryCounts {
    /// 처음 보는 키 — 실행했다([`Decision::Execute`]).
    pub(crate) executed: u64,
    /// 같은 키·같은 요청 — 실행하지 않고 보관된 답을 냈다([`Decision::Replay`]).
    pub(crate) replayed: u64,
    /// 같은 키·다른 요청 — 아무것도 실행하지 않았다([`Decision::Conflict`]).
    pub(crate) conflicted: u64,
    /// 응답을 버린 기존 요청의 재시도를 실행하지 않았다(Decision::Discarded).
    pub(crate) discarded: u64,
    /// 진행 중인 요청. 비동기 경로는 결과를 함께 기다리고 engine 경로는 결과 불명으로 답한다.
    pub(crate) in_flight: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryOutcome {
    Executed,
    Replayed,
    Conflicted,
    Discarded,
    InFlight,
}

impl RetryCounts {
    fn note(&mut self, outcome: RetryOutcome) {
        let count = match outcome {
            RetryOutcome::Executed => &mut self.executed,
            RetryOutcome::Replayed => &mut self.replayed,
            RetryOutcome::Conflicted => &mut self.conflicted,
            RetryOutcome::Discarded => &mut self.discarded,
            RetryOutcome::InFlight => &mut self.in_flight,
        };
        *count = count.saturating_add(1);
    }
}

/// Journal and legacy routing report to the same process-wide bounded counters.
pub(crate) fn note_journal_retry(outcome: RetryOutcome) {
    store().counts.note(outcome);
}

/// 프로세스당 하나인 멱등성 기록 저장소.
#[derive(Debug, Default)]
pub(crate) struct Store {
    /// 삽입 순서. 밀어내는 쪽은 항상 앞이다.
    entries: VecDeque<Entry>,
    total_bytes: usize,
    /// 다음 실행에 발급할 식별 번호.
    next_ticket: u64,
    /// decide에서 집계한다. 현재 라우터가 처리하지 않아 다음 라우터로 넘기면
    /// abandon_unhandled가 집계를 되돌려 중복을 막는다.
    counts: RetryCounts,
}

impl Store {
    pub(crate) const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            total_bytes: 0,
            next_ticket: 0,
            counts: RetryCounts {
                executed: 0,
                replayed: 0,
                conflicted: 0,
                discarded: 0,
                in_flight: 0,
            },
        }
    }

    /// 이 키로 무엇을 할지 정한다. 만료된 항목은 이 자리에서 걷힌다.
    pub(crate) fn decide(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> Decision {
        self.purge_expired(now);
        let digest = digest(method, params);
        let decision = match self
            .entries
            .iter()
            .find(|e| e.scope == scope && e.key == key)
        {
            None => Decision::Execute(digest),
            Some(e) if e.digest != digest => Decision::Conflict,
            Some(e) => match &e.outcome {
                Stored::Response(r) => Decision::Replay(r.clone()),
                Stored::Discarded => Decision::Discarded,
                Stored::InFlight { .. } => Decision::InFlight,
            },
        };
        self.counts.note(match decision {
            Decision::Execute(_) => RetryOutcome::Executed,
            Decision::Replay(_) => RetryOutcome::Replayed,
            Decision::Conflict => RetryOutcome::Conflicted,
            Decision::Discarded => RetryOutcome::Discarded,
            Decision::InFlight => RetryOutcome::InFlight,
        });
        decision
    }

    /// 판정 누계의 사본.
    pub(crate) fn counts(&self) -> RetryCounts {
        self.counts
    }

    /// 실행을 연다 — 결말이 올 때까지 이 키는 [`Decision::InFlight`] 로 답한다.
    fn open(&mut self, now: Instant, scope: &str, key: &str, digest: u64) -> u64 {
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        self.remove(scope, key);
        self.entries.push_back(Entry {
            scope: scope.to_string(),
            key: key.to_string(),
            digest,
            stored_at: now,
            outcome: Stored::InFlight {
                ticket,
                waiters: Vec::new(),
            },
            bytes: 0,
        });
        self.enforce_bounds();
        ticket
    }

    /// 진행 중인 실행에 재시도를 합류시킨다. 그 자리가 이미 진행 중이 아니면 거짓이다.
    fn join(&mut self, scope: &str, key: &str, waiter: Waiter) -> Result<(), Waiter> {
        match self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.key == key)
            .map(|e| &mut e.outcome)
        {
            Some(Stored::InFlight { waiters, .. }) => {
                waiters.push(waiter);
                Ok(())
            }
            _ => Err(waiter),
        }
    }

    /// 해당 실행 번호의 진행 중 항목만 지운다. 같은 키의 새 실행은 건드리지 않는다.
    /// 기다리던 재시도 요청의 채널도 함께 닫힌다.
    fn abandon(&mut self, scope: &str, key: &str, ticket: u64) {
        if let Some(i) = self.entries.iter().position(|e| {
            e.scope == scope
                && e.key == key
                && matches!(e.outcome, Stored::InFlight { ticket: t, .. } if t == ticket)
        }) && let Some(e) = self.entries.remove(i)
        {
            self.total_bytes -= e.bytes;
        }
    }

    /// 이 라우터가 처리하지 않는 요청의 기록과 executed 집계를 취소한다.
    /// 다음 라우터에서 다시 판정하므로 여기서 세면 중복되거나 지원 밖 요청까지 포함된다.
    fn abandon_unhandled(&mut self, scope: &str, key: &str, ticket: u64) {
        self.abandon(scope, key, ticket);
        self.counts.executed = self.counts.executed.saturating_sub(1);
    }

    /// 실행 번호 없이 결과를 보관한다. 제품 경로는 [`finish`] 가 `settle`로 기록한다.
    #[cfg(test)]
    pub(crate) fn record(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        digest: u64,
        response: &JsonRpcResponse,
    ) {
        self.settle(now, scope, key, digest, None, response);
    }

    /// open이 발급한 실행 번호로 결과를 기록한다. 같은 키가 다른 실행으로 바뀌었으면 무시한다.
    fn settle(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        digest: u64,
        ticket: Option<u64>,
        response: &JsonRpcResponse,
    ) -> Handoff {
        let mut handoff = Handoff::default();
        let slot = self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.key == key);
        if let (Some(mine), Some(Entry { outcome, .. })) = (ticket, slot) {
            match outcome {
                Stored::InFlight { ticket: t, waiters } if *t == mine => {
                    handoff.waiters = std::mem::take(waiters);
                }
                Stored::InFlight { .. } => return handoff,
                _ => {}
            }
        }
        self.remove(scope, key);
        let size = serde_json::to_string(response)
            .map(|s| s.len())
            .unwrap_or(0);
        let (outcome, bytes) = if size > MAX_STORED_RESPONSE_BYTES {
            (Stored::Discarded, 0)
        } else {
            (Stored::Response(Box::new(response.clone())), size)
        };
        handoff.kept = matches!(outcome, Stored::Response(_));
        self.entries.push_back(Entry {
            scope: scope.to_string(),
            key: key.to_string(),
            digest,
            stored_at: now,
            outcome,
            bytes,
        });
        self.total_bytes += bytes;
        self.enforce_bounds();
        handoff
    }

    fn remove(&mut self, scope: &str, key: &str) {
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.scope == scope && e.key == key)
            && let Some(e) = self.entries.remove(i)
        {
            self.total_bytes -= e.bytes;
        }
    }

    fn purge_expired(&mut self, now: Instant) {
        // 앞에서부터 삽입 순이므로 만료도 앞에서부터다.
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.stored_at) < RETENTION {
                break;
            }
            self.pop_front();
        }
    }

    fn enforce_bounds(&mut self) {
        while self.entries.len() > CAPACITY || self.total_bytes > TOTAL_BUDGET_BYTES {
            if self.pop_front().is_none() {
                break;
            }
        }
    }

    fn pop_front(&mut self) -> Option<Entry> {
        let e = self.entries.pop_front()?;
        self.total_bytes -= e.bytes;
        Some(e)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// 프로세스 안에서만 사용하는 메서드·params 해시. 재시작 사이의 동일성은 필요 없다.
fn digest(method: &str, params: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    method.hash(&mut h);
    // 직렬화 실패를 빈 값으로 처리하면 다른 params가 같은 해시 입력이 된다.
    match serde_json::to_string(params) {
        Ok(s) => {
            s.hash(&mut h);
        }
        Err(_) => {
            format!("{params:?}").hash(&mut h);
        }
    }
    h.finish()
}

/// 키는 연결이 아니라 호출자별로 구분한다. 재시도는 다른 연결에서 올 수 있다.
/// Local은 한 주체로, 플러그인과 에이전트는 각각의 ID로 구분해 다른 호출자의 응답을 섞지 않는다.
pub(crate) fn caller_scope(caller: &CallerContext) -> String {
    match caller {
        CallerContext::Local { .. } => "local".to_string(),
        CallerContext::Plugin { plugin_id, .. } => format!("plugin:{plugin_id}"),
        CallerContext::Agent { agent_id, .. } => format!("agent:{agent_id}"),
    }
}

static STORE: Mutex<Store> = Mutex::new(Store::new());
static POISON_REPORTED: AtomicBool = AtomicBool::new(false);
const WHAT: &str = "the IPC idempotency store";

fn store() -> MutexGuard<'static, Store> {
    lock(&STORE)
}

/// 시험은 별도 저장소를 주입해 병렬 시험의 누계가 섞이지 않도록 한다.
fn lock(store: &'static Mutex<Store>) -> MutexGuard<'static, Store> {
    tasty_utils::poison::recover_mutex(store.lock(), WHAT, &POISON_REPORTED)
}

/// 실행을 기다리는 키. [`begin`] 이 주고 [`finish`] 가 받는다.
#[derive(Debug)]
pub(crate) struct Pending {
    scope: String,
    key: String,
    digest: u64,
    /// [`hold`] 가 연 실행 번호. 결과를 기다리는 동안 같은 키의 재시도는 InFlight로 답한다.
    ticket: Option<u64>,
}

/// 빈 키와 크기 초과를 라우팅 전에 거절한다. 메서드나 저장소 상태와 무관한 형식 검사다.
pub(crate) fn check_envelope(
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<(), JsonRpcResponse> {
    let Some(key) = request.idempotency_key.as_deref() else {
        return Ok(());
    };
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!(
                "idempotency_key must be 1..={MAX_KEY_BYTES} bytes (got {})",
                key.len()
            ),
        ));
    }
    Ok(())
}

/// 라우팅 전에 중복 실행 여부를 판정한다. 키가 없거나 적용 대상이 아니면 Ok(None)이다.
/// 키 형식은 이미 진입 검사에서 확인했으므로 반복하지 않는다.
pub(crate) fn begin(
    now: Instant,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<Option<Pending>, JsonRpcResponse> {
    begin_in(&STORE, now, caller, request, id)
}

fn begin_in(
    store: &'static Mutex<Store>,
    now: Instant,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<Option<Pending>, JsonRpcResponse> {
    let Some(key) = request.idempotency_key.as_deref() else {
        return Ok(None);
    };
    // 표가 이 이름을 모르면 라우팅도 못 한다 — 그 답은 라우터가 낸다.
    if method_meta(&request.method).map(|m| m.effect) != Some(MethodEffect::Mutate) {
        return Ok(None);
    }
    let scope = caller_scope(caller);
    match lock(store).decide(now, &scope, key, &request.method, &request.params) {
        Decision::Execute(digest) => Ok(Some(Pending {
            scope,
            key: key.to_string(),
            digest,
            ticket: None,
        })),
        Decision::Replay(stored) => Err(stored.replayed_for(id.clone())),
        Decision::Conflict => Err(conflict_error(key, id)),
        Decision::Discarded => Err(discarded_error(key, id)),
        // memory writer 결과를 기다리는 요청의 재시도가 여기 닿는다. 기다릴 수 없어 결과 불명으로 답한다.
        Decision::InFlight => Err(still_running_error(key, id)),
    }
}

fn conflict_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_IDEMPOTENCY_KEY_CONFLICT,
        format!(
            "idempotency_key '{key}' was already used for a different request; \
             use a new key for a new request"
        ),
    )
}

fn discarded_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_IDEMPOTENT_RESULT_DISCARDED,
        format!(
            "the request for idempotency_key '{key}' did run, but its response exceeded \
             {MAX_STORED_RESPONSE_BYTES} bytes and was not kept: do not resend, query instead"
        ),
    )
}

fn still_running_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN,
        format!(
            "the request for idempotency_key '{key}' is still running and its outcome is not \
             known yet: do not resend, query instead"
        ),
    )
}

/// 실행이 끝났다. [`begin`] 이 키를 줬을 때만 기록한다. 메서드 없음 오류도 포함한다.
/// 라우팅 전 권한·cap·rate 거절은 보관하지 않아, 권한을 얻은 뒤 다시 시도할 수 있다.
pub(crate) fn finish(now: Instant, pending: Option<Pending>, response: &JsonRpcResponse) {
    finish_in(&STORE, now, pending, response);
}

fn finish_in(
    store: &'static Mutex<Store>,
    now: Instant,
    pending: Option<Pending>,
    response: &JsonRpcResponse,
) {
    let Some(p) = pending else {
        return;
    };
    let handoff = lock(store).settle(now, &p.scope, &p.key, p.digest, p.ticket, response);
    for w in handoff.waiters {
        let answer = if handoff.kept {
            response.replayed_for(w.id)
        } else {
            discarded_error(&p.key, &w.id)
        };
        send_response(&w.reply, answer);
    }
}

/// 응답이 memory writer 결과를 기다리는 동안 키를 진행 중으로 둔다. 그동안의 재시도는
/// 다시 실행하지 않는다. 결과는 [`finish`] 가 같은 실행 번호로 기록한다.
pub(crate) fn hold(now: Instant, pending: Option<Pending>) -> Option<Pending> {
    hold_in(&STORE, now, pending)
}

fn hold_in(
    store: &'static Mutex<Store>,
    now: Instant,
    pending: Option<Pending>,
) -> Option<Pending> {
    let mut p = pending?;
    p.ticket = Some(lock(store).open(now, &p.scope, &p.key, p.digest));
    Some(p)
}

/// 응답을 채널로 보내는 라우터에 멱등성 검사를 적용한다.
/// 키가 없거나 Mutate가 아니면 원래 경로에 맡긴다. 재사용·충돌·폐기 판정은 즉시 답하고,
/// 진행 중이면 같은 실행의 결과를 기다린다. 새 요청은 relay가 결과를 받아 기록·전달한다.
///
/// dispatch에는 키를 뗀 사본을 준다. 키를 두면 같은 함수에 다시 들어와 자기 결과를 기다리게 된다.
/// 해당 라우터가 처리하지 않으면 기록과 집계를 취소해 다음 라우터가 판정하게 한다.
/// 응답 없이 채널이 닫히면 기록과 재시도 응답 채널도 닫는다.
/// relay 스레드 생성 실패 시에는 키 보장 없이 원래 채널로 실행한다.
/// 보관 시간은 실행 시작과 relay 모두 Instant::now를 사용한다.
pub(crate) fn run_app_layer<T>(
    caller: &CallerContext,
    cmd: &IpcCommand,
    answered: T,
    handled: impl FnOnce(&T) -> bool,
    dispatch: impl FnOnce(&IpcCommand) -> T,
) -> Option<T> {
    run_app_layer_in(
        &STORE,
        spawn_relay,
        caller,
        cmd,
        answered,
        handled,
        dispatch,
    )
}

/// KeyContract::Kept로 선언된 호스트 메서드의 플러그인 전달에 중복 방지를 적용한다.
/// 플러그인이 호스트로 다시 호출할 때는 키가 없으므로 최초 전달에서 검사해야 한다.
/// 응답이 나중에 오므로 run_app_layer와 같은 진행 중 기록과 relay를 사용한다.
///
/// 플러그인 고유 이름은 원래 요청을 그대로 전달한다. namespace가 등록되면 그런 이름도
/// Mutate로 해석되므로 Mutate 판정만으로는 부족하고 Kept 확인이 필요하다(docs/dev-guide/api-conventions.md).
pub(crate) fn forward_keeping_the_key(
    caller: &CallerContext,
    cmd: &IpcCommand,
    forward: impl FnOnce(&IpcCommand),
) {
    let method = tasty_ipc::alias::canonicalize(&cmd.request.method);
    let mut forward = Some(forward);
    let engaged = matches!(
        tasty_ipc::method_meta::key_contract(method),
        tasty_ipc::method_meta::KeyContract::Kept { .. }
    ) && run_app_layer(
        caller,
        cmd,
        (),
        |_| true,
        |relayed| {
            if let Some(f) = forward.take() {
                f(relayed);
            }
        },
    )
    .is_some();
    // 개입한 경우에는 이미 전달했거나, 기존 결과를 사용할 요청이므로 다시 넘기지 않는다.
    if !engaged && let Some(f) = forward.take() {
        f(cmd);
    }
}

/// 시험에서 스레드 생성 실패를 주입할 수 있도록 둔 함수 타입.
type Spawn = fn(Box<dyn FnOnce() + Send>) -> std::io::Result<()>;

/// 스레드 생성 실패를 이벤트 루프의 패닉 대신 오류로 받는다.
fn spawn_relay(work: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ipc-idempotency-relay".into())
        .spawn(work)
        .map(|_| ())
}

fn run_app_layer_in<T>(
    store: &'static Mutex<Store>,
    spawn: Spawn,
    caller: &CallerContext,
    cmd: &IpcCommand,
    answered: T,
    handled: impl FnOnce(&T) -> bool,
    dispatch: impl FnOnce(&IpcCommand) -> T,
) -> Option<T> {
    let request = &cmd.request;
    let key = request.idempotency_key.as_deref()?;
    let method = tasty_ipc::alias::canonicalize(&request.method);
    if method_meta(method).map(|m| m.effect) != Some(MethodEffect::Mutate) {
        return None;
    }
    let now = Instant::now();
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let scope = caller_scope(caller);
    let mut guard = lock(store);
    let answer = match guard.decide(now, &scope, key, method, &request.params) {
        Decision::Execute(digest) => {
            let ticket = guard.open(now, &scope, key, digest);
            drop(guard);
            let relay = Relay {
                store,
                scope,
                key: key.to_string(),
                digest,
                ticket,
                reply: cmd.response_tx.clone(),
                request_seq: cmd.request_seq(),
            };
            return Some(relay.run(spawn, request, handled, dispatch));
        }
        Decision::InFlight => {
            let waiter = Waiter {
                id,
                reply: cmd.response_tx.clone(),
            };
            match guard.join(&scope, key, waiter) {
                Ok(()) => return Some(answered),
                // 같은 잠금 안에서 판정해 보통 닿지 않는다. 기다릴 항목이 없으면 결과 불명으로 답한다.
                Err(w) => still_running_error(key, &w.id),
            }
        }
        Decision::Replay(stored) => stored.replayed_for(id),
        Decision::Conflict => conflict_error(key, &id),
        Decision::Discarded => discarded_error(key, &id),
    };
    drop(guard);
    send_response(&cmd.response_tx, answer);
    Some(answered)
}

/// 비동기 실행 결과를 기록하고 원래 응답 채널로 전달한다.
struct Relay {
    store: &'static Mutex<Store>,
    scope: String,
    key: String,
    digest: u64,
    ticket: u64,
    reply: mpsc::SyncSender<JsonRpcResponse>,
    /// 키를 제거해도 원 요청의 호스트 번호는 유지한다.
    request_seq: tasty_ipc::server::RequestSeq,
}

impl Relay {
    /// relay를 먼저 만든다. 실패하면 아직 실행 전이므로 원래 채널로 실행할 수 있다.
    /// 키 보장은 잃지만 응답 경로는 유지한다. 처리하지 않는 요청이면 채널과 relay가 종료된다.
    fn run<T>(
        self,
        spawn: Spawn,
        request: &JsonRpcRequest,
        handled: impl FnOnce(&T) -> bool,
        dispatch: impl FnOnce(&IpcCommand) -> T,
    ) -> T {
        let (store, ticket, request_seq) = (self.store, self.ticket, self.request_seq);
        let (scope, key) = (self.scope.clone(), self.key.clone());
        let reply = self.reply.clone();
        let stripped = JsonRpcRequest {
            idempotency_key: None,
            ..request.clone()
        };
        let (tx, rx) = mpsc::sync_channel::<JsonRpcResponse>(1);
        if let Err(e) = spawn(Box::new(move || self.settle_when_answered(&rx))) {
            tracing::warn!(
                method = %request.method,
                error = %e,
                "could not start the idempotency relay thread; running the request \
                 without the key guarantee"
            );
            // 실행 전 기록을 지워 재시도가 결과 없는 항목을 기다리지 않게 한다.
            lock(store).abandon(&scope, &key, ticket);
            let out = dispatch(&IpcCommand::continuing(stripped, reply, request_seq));
            if !handled(&out) {
                lock(store).abandon_unhandled(&scope, &key, ticket);
            }
            return out;
        }
        let relayed = IpcCommand::continuing(stripped, tx, request_seq);
        let out = dispatch(&relayed);
        drop(relayed);
        if !handled(&out) {
            lock(store).abandon_unhandled(&scope, &key, ticket);
        }
        out
    }

    fn settle_when_answered(self, rx: &mpsc::Receiver<JsonRpcResponse>) {
        let Ok(response) = rx.recv() else {
            lock(self.store).abandon(&self.scope, &self.key, self.ticket);
            return;
        };
        let handoff = lock(self.store).settle(
            Instant::now(),
            &self.scope,
            &self.key,
            self.digest,
            Some(self.ticket),
            &response,
        );
        for w in handoff.waiters {
            let answer = if handoff.kept {
                response.replayed_for(w.id)
            } else {
                discarded_error(&self.key, &w.id)
            };
            send_response(&w.reply, answer);
        }
        send_response(&self.reply, response);
    }
}

/// system.pressure의 keyed_requests에 제공할 누계.
pub(crate) fn retry_counts() -> RetryCounts {
    store().counts()
}

/// system.info에 실제 상수로부터 보장 범위를 제공한다.
pub(crate) fn declaration() -> serde_json::Value {
    serde_json::json!({
        "retention_ms": RETENTION.as_millis() as u64,
        "capacity": CAPACITY,
        "max_response_bytes": MAX_STORED_RESPONSE_BYTES,
        "max_key_bytes": MAX_KEY_BYTES,
        "survives_restart": false,
        "structure_journal": {
            "methods": tasty_ipc::method_meta::METHOD_TABLE.iter().filter(|(method, _)| tasty_ipc::method_meta::has_structure_journal_contract(method)).map(|(method, _)| *method).collect::<Vec<_>>(),
            "survives_restart": true,
            "scope": "data_home_journal",
            "implicit_target": "first_admission",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resp(id: i32, body: &str) -> JsonRpcResponse {
        JsonRpcResponse::success(json!(id), json!({ "body": body }))
    }

    #[test]
    fn the_same_key_and_request_is_executed_once_and_then_replayed() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"}))
        else {
            panic!("처음 보는 키는 실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));

        match s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"})) {
            Decision::Replay(r) => assert_eq!(r.result.as_ref().unwrap()["body"], "first"),
            other => panic!("재조회여야 한다: {other:?}"),
        }
    }

    #[test]
    fn a_replay_carries_the_new_id_and_says_it_is_a_replay() {
        let stored = resp(1, "first");
        let again = stored.replayed_for(json!(99));
        assert_eq!(
            again.id,
            json!(99),
            "앞선 요청의 id 로 답하면 대응이 깨진다"
        );
        assert!(again.idempotent_replay);
        assert!(
            !stored.idempotent_replay,
            "처음 실행된 답에 재조회 표지가 붙으면 두 사건이 구별되지 않는다"
        );
    }

    #[test]
    fn the_same_key_with_a_different_request_is_a_conflict() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"}))
        else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));

        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "b"})),
            Decision::Conflict
        ));
        assert!(
            matches!(
                s.decide(t0, "local", "k", "tab.create", &json!({"name": "a"})),
                Decision::Conflict
            ),
            "메서드가 다른 것도 다른 요청이다"
        );
    }

    #[test]
    fn two_callers_that_picked_the_same_string_do_not_share_an_answer() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let params = json!({ "name": "same" });
        let Decision::Execute(d) = s.decide(t0, "agent:a", "k", "workspace.create", &params) else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "agent:a", "k", d, &resp(1, "answer-for-a"));

        assert!(
            matches!(
                s.decide(t0, "agent:b", "k", "workspace.create", &params),
                Decision::Execute(_)
            ),
            "다른 주체가 같은 키로 남의 답을 받았다"
        );
        assert!(matches!(
            s.decide(t0, "agent:a", "k", "workspace.create", &params),
            Decision::Replay(_)
        ));
    }

    // 같은 ID 문자열이어도 Local/Plugin/Agent는 서로 다른 호출자다.
    #[test]
    fn the_three_caller_kinds_get_three_scopes() {
        use std::collections::HashSet;
        use std::sync::Arc;
        let scopes: HashSet<String> = [
            caller_scope(&CallerContext::local()),
            caller_scope(&CallerContext::Plugin {
                plugin_id: "p".into(),
            }),
            caller_scope(&CallerContext::Agent {
                agent_id: "p".into(),
                permissions: Arc::new(Default::default()),
            }),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            scopes.len(),
            3,
            "서로 다른 호출자의 범위가 같아졌다: {scopes:?}"
        );
    }

    #[test]
    fn the_digest_does_not_move_with_object_key_order() {
        let a: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
        assert_eq!(digest("m", &a), digest("m", &b));
        assert_ne!(digest("m", &a), digest("m", &json!({"a": 1, "b": 3})));
    }

    #[test]
    fn an_expired_key_is_indistinguishable_from_a_fresh_one() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) = s.decide(t0, "local", "k", "workspace.create", &json!({}))
        else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));
        assert!(matches!(
            s.decide(
                t0 + RETENTION - Duration::from_millis(1),
                "local",
                "k",
                "workspace.create",
                &json!({})
            ),
            Decision::Replay(_)
        ));
        assert!(matches!(
            s.decide(t0 + RETENTION, "local", "k", "workspace.create", &json!({})),
            Decision::Execute(_)
        ));
    }

    #[test]
    fn the_oldest_key_is_pushed_out_past_the_capacity() {
        let mut s = Store::new();
        let t0 = Instant::now();
        for i in 0..=CAPACITY {
            let key = format!("k{i}");
            let Decision::Execute(d) =
                s.decide(t0, "local", &key, "workspace.create", &json!({ "i": i }))
            else {
                panic!("실행이어야 한다");
            };
            s.record(t0, "local", &key, d, &resp(1, "x"));
        }
        assert_eq!(s.len(), CAPACITY);
        assert!(
            matches!(
                s.decide(t0, "local", "k0", "workspace.create", &json!({"i": 0})),
                Decision::Execute(_)
            ),
            "밀려난 키는 처음 보는 키와 구별되지 않는다"
        );
        assert!(matches!(
            s.decide(
                t0,
                "local",
                &format!("k{CAPACITY}"),
                "workspace.create",
                &json!({"i": CAPACITY})
            ),
            Decision::Replay(_)
        ));
    }

    #[test]
    fn an_oversized_response_keeps_the_key_and_drops_the_answer() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "surface.read_since_scan_mark", &json!({}))
        else {
            panic!("실행이어야 한다");
        };
        let big = JsonRpcResponse::success(json!(1), json!("x".repeat(MAX_STORED_RESPONSE_BYTES)));
        s.record(t0, "local", "k", d, &big);
        assert!(matches!(
            s.decide(t0, "local", "k", "surface.read_since_scan_mark", &json!({})),
            Decision::Discarded
        ));
    }

    #[test]
    fn the_byte_budget_evicts_before_the_capacity_does() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let payload = "y".repeat(MAX_STORED_RESPONSE_BYTES - 128);
        let n = TOTAL_BUDGET_BYTES / MAX_STORED_RESPONSE_BYTES + 2;
        assert!(
            n < CAPACITY,
            "이 시험은 항목 수 상한 아래에서 돌아야 뜻이 있다"
        );
        for i in 0..n {
            let key = format!("k{i}");
            let Decision::Execute(d) =
                s.decide(t0, "local", &key, "workspace.create", &json!({ "i": i }))
            else {
                panic!("실행이어야 한다");
            };
            s.record(
                t0,
                "local",
                &key,
                d,
                &JsonRpcResponse::success(json!(1), json!(payload)),
            );
        }
        assert!(s.len() < n, "바이트 상한을 넘은 항목이 제거되지 않았다");
        assert!(s.total_bytes <= TOTAL_BUDGET_BYTES);
    }

    #[test]
    fn a_request_without_a_key_is_untouched() {
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: "workspace.create".into(),
            params: json!({}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        assert!(
            begin(Instant::now(), &CallerContext::local(), &req, &json!(1))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_key_on_a_non_mutating_method_does_nothing() {
        for method in ["workspace.list", "workspace.update"] {
            let req = JsonRpcRequest {
                caller_agent_id: None,
                jsonrpc: "2.0".into(),
                method: method.into(),
                params: json!({}),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: Some("shared-key".into()),
            };
            assert!(
                begin(Instant::now(), &CallerContext::local(), &req, &json!(1))
                    .unwrap()
                    .is_none(),
                "{method} 가 보존소에 들어갔다"
            );
        }
    }

    #[test]
    fn a_key_outside_the_length_bound_is_rejected_before_anything_runs() {
        fn keyed(key: String) -> JsonRpcRequest {
            JsonRpcRequest {
                caller_agent_id: None,
                jsonrpc: "2.0".into(),
                method: "workspace.create".into(),
                params: json!({}),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: Some(key),
            }
        }
        for key in ["".to_string(), "k".repeat(MAX_KEY_BYTES + 1)] {
            let err =
                check_envelope(&keyed(key), &json!(1)).expect_err("길이 밖 키는 거절이어야 한다");
            assert_eq!(err.error.expect("에러").code, -32602);
        }
        for key in ["k".to_string(), "k".repeat(MAX_KEY_BYTES)] {
            assert!(check_envelope(&keyed(key), &json!(1)).is_ok());
        }
    }

    // App·플러그인·engine 요청이 같은 길이 검사를 받는지, 정상 길이도 함께 확인한다.
    // 창이 없는 GUI의 check_without_engine 경로도 포함한다.
    #[test]
    fn the_envelope_is_judged_at_the_gate_whatever_the_destination() {
        use crate::ipc::handler::check_request;
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = crate::ipc::handler::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        for method in ["window.create", "someplugin.do_thing", "workspace.create"] {
            for (key, ok) in [
                (String::new(), false),
                ("k".repeat(MAX_KEY_BYTES + 1), false),
                ("fine".to_string(), true),
            ] {
                let req = JsonRpcRequest {
                    caller_agent_id: None,
                    jsonrpc: "2.0".into(),
                    method: method.into(),
                    params: json!({}),
                    id: Some(json!(7)),
                    session_token: None,
                    response_timeout_ms: None,
                    idempotency_key: Some(key.clone()),
                };
                let local = CallerContext::local();
                let r = check_request(&mut core, &mut state, &mut engine, &req, &local);
                #[cfg(feature = "gui")]
                let without_engine = Some((
                    "check_without_engine",
                    crate::ipc::handler::check_without_engine(&req, &local).map(|_| ()),
                ));
                #[cfg(not(feature = "gui"))]
                let without_engine = None;
                let gates = std::iter::once(("check_request", r.map(|_| ()))).chain(without_engine);
                for (gate, r) in gates {
                    match (r, ok) {
                        (Ok(()), true) => {}
                        (Err(e), false) => {
                            assert_eq!(e.error.expect("에러").code, -32602, "{gate} {method}")
                        }
                        (Ok(()), false) => {
                            panic!("{gate} {method}: 길이 {} 키가 게이트를 지났다", key.len())
                        }
                        (Err(e), true) => panic!("{gate} {method}: 경계 안 키가 거절됐다: {e:?}"),
                    }
                }
            }
        }
    }

    // replay=false여도 중복 방지 대상일 수 있다. 충돌 거절과 성공 재사용을 함께 확인한다.
    #[test]
    fn a_blocked_retry_carries_no_replay_marker_though_the_contract_did_engage() {
        fn req(name: &str) -> JsonRpcRequest {
            JsonRpcRequest {
                caller_agent_id: None,
                jsonrpc: "2.0".into(),
                method: "workspace.create".into(),
                params: json!({ "name": name }),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                // 보존소는 프로세스 전역이라 다른 시험과 안 겹치는 키를 쓴다.
                idempotency_key: Some("a-blocked-retry-probe".into()),
            }
        }
        let now = Instant::now();
        let first = req("probe-a");
        let pending = begin(now, &CallerContext::local(), &first, &json!(1))
            .expect("처음 보는 키는 실행이다");
        finish(now, pending, &resp(1, "ok"));

        let replay =
            begin(now, &CallerContext::local(), &first, &json!(2)).expect_err("재생은 답을 낸다");
        assert!(replay.idempotent_replay, "재생에는 표지가 붙어야 한다");

        let conflict = begin(now, &CallerContext::local(), &req("probe-b"), &json!(3))
            .expect_err("같은 키·다른 요청은 거절이다");
        assert_eq!(
            conflict.error.as_ref().expect("에러").code,
            ERR_IDEMPOTENCY_KEY_CONFLICT
        );
        assert!(
            !conflict.idempotent_replay,
            "충돌 응답에는 재생 표지가 없어야 한다"
        );
    }

    // 전역 저장소와 별도 relay 스레드를 쓰므로 고유 키와 유한 응답 대기를 사용한다.

    const WAIT: Duration = Duration::from_secs(5);

    fn app_cmd(key: &str, id: i64, name: &str) -> (IpcCommand, mpsc::Receiver<JsonRpcResponse>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: "window.create".into(),
            params: json!({ "name": name }),
            id: Some(json!(id)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some(key.into()),
        };
        (IpcCommand::new(req, tx), rx)
    }

    fn answer_now(runs: &std::cell::Cell<u32>) -> impl FnOnce(&IpcCommand) -> bool + '_ {
        move |c: &IpcCommand| {
            runs.set(runs.get() + 1);
            assert!(
                c.request.idempotency_key.is_none(),
                "dispatch에는 idempotency 키를 제거한 요청을 전달해야 한다"
            );
            send_response(
                &c.response_tx,
                JsonRpcResponse::success(c.request.id.clone().unwrap(), json!({"run": runs.get()})),
            );
            true
        }
    }

    #[test]
    fn an_app_layer_call_runs_once_per_key_and_the_retry_is_a_replay() {
        let runs = std::cell::Cell::new(0);
        let (first, rx1) = app_cmd("app-once-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::local(),
            &first,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(out, Some(true));
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        assert!(!r1.idempotent_replay);

        let (retry, rx2) = app_cmd("app-once-probe", 2, "a");
        let out = run_app_layer(
            &CallerContext::local(),
            &retry,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(
            out,
            Some(false),
            "재생은 dispatch 없이 `answered` 를 돌려준다"
        );
        let r2 = rx2.recv_timeout(WAIT).expect("재생 답");
        assert_eq!(runs.get(), 1, "같은 키의 재시도가 두 번째 실행을 냈다");
        assert!(r2.idempotent_replay, "재생 표지가 없다");
        assert_eq!(r2.id, json!(2), "재생은 이번 요청의 id 로 답한다");
        assert_eq!(r2.result, r1.result);

        let (tx, _rx) = mpsc::sync_channel(1);
        let mut req = first.request.clone();
        req.idempotency_key = None;
        let unkeyed = IpcCommand::new(req, tx);
        assert!(
            run_app_layer(
                &CallerContext::local(),
                &unkeyed,
                false,
                |h| *h,
                answer_now(&runs)
            )
            .is_none()
        );
    }

    #[test]
    fn the_stripped_copy_keeps_the_request_seq_of_the_original() {
        let seen = std::cell::Cell::new(None);
        let (first, _rx) = app_cmd("app-seq-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::local(),
            &first,
            false,
            |h| *h,
            |c: &IpcCommand| {
                seen.set(Some(c.request_seq()));
                send_response(
                    &c.response_tx,
                    JsonRpcResponse::success(json!(1), json!({})),
                );
                true
            },
        );
        assert_eq!(out, Some(true));
        assert_eq!(seen.get(), Some(first.request_seq()));

        let (second, _rx) = app_cmd("app-seq-probe-no-relay", 1, "a");
        let out = run_app_layer_in(
            isolated_store(),
            no_threads,
            &CallerContext::local(),
            &second,
            false,
            |h| *h,
            |c: &IpcCommand| {
                seen.set(Some(c.request_seq()));
                true
            },
        );
        assert_eq!(out, Some(true));
        assert_eq!(seen.get(), Some(second.request_seq()));
    }

    #[test]
    fn a_retry_that_arrives_while_the_first_is_running_joins_it() {
        let runs = std::cell::Cell::new(0);
        let parked: std::cell::RefCell<Option<IpcCommand>> = std::cell::RefCell::new(None);
        let (first, rx1) = app_cmd("app-join-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::local(),
            &first,
            false,
            |h| *h,
            |c| {
                runs.set(runs.get() + 1);
                *parked.borrow_mut() =
                    Some(IpcCommand::new(c.request.clone(), c.response_tx.clone()));
                true
            },
        );
        assert_eq!(out, Some(true));

        let (retry, rx2) = app_cmd("app-join-probe", 2, "a");
        let out = run_app_layer(
            &CallerContext::local(),
            &retry,
            false,
            |h| *h,
            |_| panic!("진행 중인 키가 두 번째 실행을 냈다"),
        );
        assert_eq!(out, Some(false));
        assert!(
            rx2.try_recv().is_err(),
            "합류자는 첫 실행이 끝나기 전에 답을 받으면 안 된다"
        );

        let (other, rx3) = app_cmd("app-join-probe", 3, "b");
        run_app_layer(
            &CallerContext::local(),
            &other,
            false,
            |h| *h,
            |_| -> bool { panic!("충돌인데 실행됐다") },
        );
        assert_eq!(
            rx3.recv_timeout(WAIT)
                .expect("충돌 답")
                .error
                .expect("에러")
                .code,
            ERR_IDEMPOTENCY_KEY_CONFLICT
        );

        let c = parked
            .borrow_mut()
            .take()
            .expect("응답 완료 핸들을 보관해야 한다");
        send_response(
            &c.response_tx,
            JsonRpcResponse::success(json!(1), json!({"window_id": 7})),
        );
        let r1 = rx1.recv_timeout(WAIT).expect("첫 요청의 답");
        let r2 = rx2.recv_timeout(WAIT).expect("합류자의 답");
        assert_eq!(runs.get(), 1);
        assert!(!r1.idempotent_replay);
        assert!(r2.idempotent_replay, "합류자의 답은 재생이다");
        assert_eq!(r2.id, json!(2));
        assert_eq!(r2.result, r1.result);
    }

    #[test]
    fn a_layer_that_does_not_handle_the_name_leaves_no_trace() {
        let (cmd, _rx) = app_cmd("app-unhandled-probe", 1, "a");
        let out = run_app_layer(&CallerContext::local(), &cmd, false, |h| *h, |_| false);
        assert_eq!(out, Some(false));
        let pending = begin(
            Instant::now(),
            &CallerContext::local(),
            &cmd.request,
            &json!(1),
        )
        .expect("다음 층은 처음 보는 키로 실행해야 한다");
        assert!(pending.is_some());
    }

    #[test]
    fn a_run_that_drops_its_reply_is_forgotten_with_its_joiners() {
        let parked: std::cell::RefCell<Option<IpcCommand>> = std::cell::RefCell::new(None);
        let (first, rx1) = app_cmd("app-dropped-probe", 1, "a");
        run_app_layer(
            &CallerContext::local(),
            &first,
            false,
            |h| *h,
            |c| {
                *parked.borrow_mut() =
                    Some(IpcCommand::new(c.request.clone(), c.response_tx.clone()));
                true
            },
        );
        let (retry, rx2) = app_cmd("app-dropped-probe", 2, "a");
        run_app_layer(
            &CallerContext::local(),
            &retry,
            false,
            |h| *h,
            |_| -> bool { panic!("합류해야 한다") },
        );
        // 원래 통로의 송신 쪽은 명령 자신도 들고 있다 — 서버에서는 처리가 끝나면 버려진다.
        drop((first, retry));
        drop(parked.borrow_mut().take());
        assert!(matches!(
            rx1.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(matches!(
            rx2.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));

        let runs = std::cell::Cell::new(0);
        let (again, rx3) = app_cmd("app-dropped-probe", 3, "a");
        run_app_layer(
            &CallerContext::local(),
            &again,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(runs.get(), 1, "완료 응답이 없던 키는 다시 실행되어야 한다");
        assert!(!rx3.recv_timeout(WAIT).expect("답").idempotent_replay);
    }

    #[test]
    fn a_late_outcome_does_not_overwrite_a_newer_run_of_the_same_key() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let old = s.open(t0, "local", "k", 1);
        s.abandon("local", "k", old); // 밀려남을 흉내 낸다
        let newer = s.open(t0, "local", "k", 1);
        let h = s.settle(t0, "local", "k", 1, Some(old), &resp(1, "late"));
        assert!(h.waiters.is_empty());
        assert!(
            matches!(
                s.entries.front().map(|e| &e.outcome),
                Some(Stored::InFlight { ticket, .. }) if *ticket == newer
            ),
            "이전 실행의 늦은 완료가 새 실행 기록을 덮어썼다"
        );
        let h = s.settle(t0, "local", "k", 1, Some(newer), &resp(2, "mine"));
        assert!(h.kept);
    }

    #[test]
    fn every_decision_is_counted_once_in_its_own_slot() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let p = json!({"name": "a"});
        let Decision::Execute(d) = s.decide(t0, "local", "k", "workspace.create", &p) else {
            panic!("처음 보는 키");
        };
        s.record(t0, "local", "k", d, &resp(1, "ok"));
        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &p),
            Decision::Replay(_)
        ));
        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "b"})),
            Decision::Conflict
        ));
        let Decision::Execute(big) = s.decide(t0, "local", "big", "workspace.create", &p) else {
            panic!("처음 보는 키");
        };
        let huge =
            JsonRpcResponse::success(json!(1), json!("x".repeat(MAX_STORED_RESPONSE_BYTES + 1)));
        s.record(t0, "local", "big", big, &huge);
        assert!(matches!(
            s.decide(t0, "local", "big", "workspace.create", &p),
            Decision::Discarded
        ));
        let Decision::Execute(run) = s.decide(t0, "local", "run", "window.create", &p) else {
            panic!("처음 보는 키");
        };
        s.open(t0, "local", "run", run);
        assert!(matches!(
            s.decide(t0, "local", "run", "window.create", &p),
            Decision::InFlight
        ));

        assert_eq!(
            s.counts(),
            RetryCounts {
                executed: 3,
                replayed: 1,
                conflicted: 1,
                discarded: 1,
                in_flight: 1,
            }
        );
    }

    // 병렬 시험도 전역 누계를 올리므로 정확한 차이 대신 최소 증가량을 확인한다.
    #[test]
    fn the_process_counts_move_where_the_router_decides() {
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: "workspace.create".into(),
            params: json!({"name": "count-probe"}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some("retry-count-probe".into()),
        };
        let before = retry_counts();
        let now = Instant::now();
        let pending = begin(now, &CallerContext::local(), &req, &json!(1)).expect("처음 보는 키");
        finish(now, pending, &resp(1, "ok"));
        begin(now, &CallerContext::local(), &req, &json!(2)).expect_err("재생");
        let after = retry_counts();
        assert!(after.executed > before.executed, "{before:?} → {after:?}");
        assert!(after.replayed > before.replayed, "{before:?} → {after:?}");
    }

    fn isolated_store() -> &'static Mutex<Store> {
        Box::leak(Box::new(Mutex::new(Store::new())))
    }

    fn keyed(method: &str, key: &str) -> (IpcCommand, mpsc::Receiver<JsonRpcResponse>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".into(),
            method: method.into(),
            params: json!({}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some(key.into()),
        };
        (IpcCommand::new(req, tx), rx)
    }

    // App이 처리하지 않은 요청을 engine이 실행한 경우도 한 번만 집계해야 한다.
    // 재시도는 App에서 기록을 찾아 응답하므로 engine까지 내려가지 않는다.
    #[test]
    fn a_request_that_crosses_both_layers_is_executed_once() {
        let store = isolated_store();
        let (cmd, rx) = keyed("workspace.create", "cross-layer");
        let out = run_app_layer_in(
            store,
            spawn_relay,
            &CallerContext::local(),
            &cmd,
            true,
            |h| *h,
            |_| false,
        );
        assert_eq!(out, Some(false), "App은 해당 메서드를 처리하지 않는다");
        let pending = begin_in(
            store,
            Instant::now(),
            &CallerContext::local(),
            &cmd.request,
            &json!(1),
        )
        .expect("처음 보는 키");
        assert!(pending.is_some());
        finish_in(store, Instant::now(), pending, &resp(1, "ok"));
        assert_eq!(
            lock(store).counts(),
            RetryCounts {
                executed: 1,
                ..RetryCounts::default()
            }
        );
        let out = run_app_layer_in(
            store,
            spawn_relay,
            &CallerContext::local(),
            &cmd,
            true,
            |h| *h,
            |_| panic!("캐시 응답을 반환할 때는 본문을 실행하지 않는다"),
        );
        assert_eq!(out, Some(true), "App 층이 재생으로 답한다");
        assert!(rx.try_recv().expect("재생 답").idempotent_replay);
        let after_retry = lock(store).counts();
        assert_eq!(after_retry.executed, 1, "{after_retry:?}");
        assert_eq!(after_retry.replayed, 1, "{after_retry:?}");
    }

    #[test]
    fn a_call_outside_the_contract_is_not_counted_as_executed() {
        let store = isolated_store();
        let mut names = vec!["someplugin.do_thing"];
        if cfg!(debug_assertions) {
            names.push("debug.lua.eval");
        }
        for name in names {
            let (cmd, _rx) = keyed(name, "outside");
            run_app_layer_in(
                store,
                spawn_relay,
                &CallerContext::local(),
                &cmd,
                true,
                |h| *h,
                |_| false,
            );
            assert_eq!(lock(store).counts(), RetryCounts::default(), "{name}");
        }
    }

    fn hold_forward(held: &std::cell::RefCell<Vec<IpcCommand>>) -> impl FnOnce(&IpcCommand) + '_ {
        move |c: &IpcCommand| {
            held.borrow_mut().push(IpcCommand::continuing(
                c.request.clone(),
                c.response_tx.clone(),
                c.request_seq(),
            ));
        }
    }

    #[test]
    fn a_forwarded_host_method_runs_once_per_key_and_the_retry_is_a_replay() {
        let held = std::cell::RefCell::new(Vec::new());
        let (first, rx1) = keyed("image.open", "forward-once-probe");
        forward_keeping_the_key(&CallerContext::local(), &first, hold_forward(&held));
        assert_eq!(held.borrow().len(), 1, "첫 요청이 forward 되지 않았다");
        assert!(
            held.borrow()[0].request.idempotency_key.is_none(),
            "forward 는 키를 뗀 사본을 받아야 한다"
        );
        let answer = JsonRpcResponse::success(json!(1), json!({"surface_id": 7}));
        send_response(&held.borrow()[0].response_tx, answer);
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        assert!(!r1.idempotent_replay);

        let (retry, rx2) = keyed("image.open", "forward-once-probe");
        forward_keeping_the_key(&CallerContext::local(), &retry, hold_forward(&held));
        assert_eq!(
            held.borrow().len(),
            1,
            "같은 키의 재시도가 plugin 으로 두 번째 forward 됐다"
        );
        let r2 = rx2.recv_timeout(WAIT).expect("재생 답");
        assert!(r2.idempotent_replay, "재생 표지가 없다");
        assert_eq!(r2.result, r1.result);
    }

    #[test]
    fn a_retry_before_the_plugin_answers_joins_the_running_forward() {
        let held = std::cell::RefCell::new(Vec::new());
        let (first, rx1) = keyed("image.open", "forward-join-probe");
        forward_keeping_the_key(&CallerContext::local(), &first, hold_forward(&held));
        let (retry, rx2) = keyed("image.open", "forward-join-probe");
        forward_keeping_the_key(&CallerContext::local(), &retry, hold_forward(&held));
        assert_eq!(
            held.borrow().len(),
            1,
            "답 전에 온 같은 키의 재시도가 plugin 으로 두 번째 forward 됐다"
        );
        let answer = JsonRpcResponse::success(json!(1), json!({"surface_id": 9}));
        send_response(&held.borrow()[0].response_tx, answer);
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        let r2 = rx2.recv_timeout(WAIT).expect("합류한 재시도의 답");
        assert!(r2.idempotent_replay, "합류한 재시도에 재생 표지가 없다");
        assert_eq!(r2.result, r1.result);
    }

    // prefix를 등록해야 고유 이름도 Mutate로 해석되어 Kept 검사 자체를 검증한다.
    // 등록하지 않으면 메서드 분류에서 먼저 제외되어 Kept 검사가 없어도 통과한다.
    #[test]
    fn a_plugin_name_is_forwarded_every_time_with_its_request_untouched() {
        const OWNER: &str = "com.test.idempotency-forward-probe";
        const PREFIX: &str = "zzzforwardprobe";
        let table = crate::namespace_table_for_tests::installed_test_table();
        table
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .register(OWNER, PREFIX)
            .expect("test prefix must be free");
        let method = format!("{PREFIX}.run");
        assert_eq!(
            tasty_ipc::method_meta::method_meta(&method).map(|m| m.effect),
            Some(MethodEffect::Mutate),
            "등록된 prefix 아래 이름이 `Mutate` 로 해소되지 않으면 이 시험은 `Kept` 판정을 안 잰다"
        );
        let held = std::cell::RefCell::new(Vec::new());
        for _ in 0..2 {
            let (cmd, _rx) = keyed(&method, "forward-outside-probe");
            forward_keeping_the_key(&CallerContext::local(), &cmd, hold_forward(&held));
        }
        table
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .unregister_plugin(OWNER);
        let held = held.borrow();
        assert_eq!(held.len(), 2, "계약 밖 이름의 두 번째 forward 가 걸러졌다");
        for c in held.iter() {
            assert_eq!(
                c.request.idempotency_key.as_deref(),
                Some("forward-outside-probe"),
                "개입하지 않는 갈래는 원래 요청을 넘긴다"
            );
        }
    }

    fn no_threads(_: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
        Err(std::io::Error::other("no threads left"))
    }

    // relay 생성 실패 시 원래 경로로 실행하며 키 보장은 없다. 미처리 요청의 집계는 되돌린다.
    #[test]
    fn a_relay_that_cannot_start_runs_the_request_without_the_guarantee() {
        let store = isolated_store();
        let runs = std::cell::Cell::new(0);
        for expected_runs in [1, 2] {
            let (cmd, rx) = app_cmd("no-relay", 1, "a");
            let out = run_app_layer_in(
                store,
                no_threads,
                &CallerContext::local(),
                &cmd,
                false,
                |h| *h,
                answer_now(&runs),
            );
            assert_eq!(out, Some(true));
            assert_eq!(runs.get(), expected_runs, "재시도가 다시 실행돼야 한다");
            let answer = rx.try_recv().expect("원래 통로로 답이 와야 한다");
            assert!(!answer.idempotent_replay);
            assert_eq!(lock(store).len(), 0);
        }
        assert_eq!(lock(store).counts().executed, 2);

        let (cmd, _rx) = keyed("workspace.create", "no-relay-unhandled");
        let out = run_app_layer_in(
            store,
            no_threads,
            &CallerContext::local(),
            &cmd,
            true,
            |h| *h,
            |_| false,
        );
        assert_eq!(out, Some(false));
        assert_eq!(
            lock(store).counts().executed,
            2,
            "처리하지 않은 요청은 실행 집계에서 제외한다"
        );
    }

    #[test]
    fn the_declaration_is_derived_from_the_bounds_it_describes() {
        let d = declaration();
        assert_eq!(d["retention_ms"], RETENTION.as_millis() as u64);
        assert_eq!(d["capacity"], CAPACITY);
        assert_eq!(d["max_response_bytes"], MAX_STORED_RESPONSE_BYTES);
        assert_eq!(d["max_key_bytes"], MAX_KEY_BYTES);
        assert_eq!(d["survives_restart"], false);
    }
}
