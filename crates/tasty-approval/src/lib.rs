#![forbid(unsafe_code)]

//! 휴먼 핸드오프 — 에이전트 ↔ 휴먼 동기 결정 게이트.
//!
//! 에이전트가 [`ApprovalStore::request`] 로 결정 요청을 만들고,
//! [`ApprovalStore::await_response`] 로 응답을 기다린다 (blocking + timeout).
//! 응답은 GUI 클릭, CLI `tasty approval respond`, 또는 다른 에이전트 등
//! 어떤 채널이든 [`ApprovalStore::respond`] 로 수렴한다.
//!
//! ## 책임 범위
//!
//! - 도메인 타입 ([`ApprovalRequest`], [`ApprovalChoice`], [`ApprovalState`])
//! - in-memory pending 큐 + 응답 대기자 (oneshot channel)
//! - 상태 전이 검증 (이미 응답된 요청에 재응답 거부, self-response 거부 등)
//! - 짧은 ID 생성 (`req_<12자>`)
//!
//! ## 비-책임 (호스트가 처리)
//!
//! - **영속**: 호스트가 상태 전이마다 `tasty-memory` 에 write.
//! - **CallerContext 매핑**: `Requester` / `Responder` 의 식별자는 호스트가
//!   `CallerContext` 로부터 도출해 set 한다.
//! - **GUI Popup**: `PopupManager` 에 `PopupDef::Approval` 등록은 호스트가.
//! - **notification**: severity 별 `notification.create` 발행도 호스트.
//!
//! ## 동기 모델
//!
//! sync. `Arc<Mutex<...>>` 로 보호된 단일 store. 응답 대기는
//! `std::sync::mpsc::sync_channel(1)` — capacity 1, sender drop / recv timeout
//! 모두 자연스러운 cancel/timeout 시그널이 된다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================
// ID
// ============================================================

/// Approval 요청 식별자. `req_` + base32 (Crockford) 12자.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApprovalId(pub String);

impl ApprovalId {
    /// 새 ID 생성. unix-ms (5 바이트) + 호출 시점 nanos 일부 (3 바이트) → 8 바이트
    /// base32 인코딩 (Crockford 알파벳, 13자) 의 앞 12자.
    ///
    /// **충돌 처리는 호출자 책임** — 같은 호스트 인스턴스에서는 ApprovalStore 가
    /// 내부 HashMap key 충돌을 reject. 글로벌 유일성은 보장하지 않는다 (CLI 입력
    /// 편의가 우선).
    pub fn generate() -> Self {
        let now = SystemTime::now();
        let ms = now
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0) as u64;
        let nanos = now
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0) as u64;
        let mixed = (ms << 24) | (nanos & 0xFF_FFFF);
        let s = encode_crockford(mixed);
        Self(format!("req_{}", &s[..12]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ApprovalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Crockford base32 (`0-9A-Z` 제외 I/L/O/U). 0-padding 없는 가변 길이지만,
/// `u64` 인 입력은 항상 13자 이내라서 안전.
fn encode_crockford(mut n: u64) -> String {
    const ALPHA: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    if n == 0 {
        return "0".repeat(13);
    }
    let mut buf = [0u8; 13];
    for i in 0..13 {
        buf[12 - i] = ALPHA[(n & 0x1F) as usize];
        n >>= 5;
    }
    String::from_utf8(buf.to_vec()).expect("alphabet ascii")
}

// ============================================================
// 도메인 타입
// ============================================================

/// 요청자. CLI/사용자 = `User`, plugin = `Plugin(prefix)`, child agent = `Agent(id)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Requester {
    User,
    Plugin { id: String },
    Agent { id: String },
}

impl Requester {
    /// 같은 caller 신원인지. self-response 검증에 사용.
    pub fn matches_responder(&self, r: &Responder) -> bool {
        match (self, r) {
            // User caller (Local CallerContext) 는 항상 다른 신원으로 간주 (응답 허용).
            (Requester::User, _) => false,
            (Requester::Plugin { id: a }, Responder::Agent { id: b }) => a == b,
            (Requester::Plugin { .. }, Responder::System) => false,
            (Requester::Agent { id: a }, Responder::Agent { id: b }) => a == b,
            _ => false,
        }
    }

    pub fn wire_id(&self) -> String {
        match self {
            Requester::User => "local".to_string(),
            Requester::Plugin { id } => format!("plugin:{id}"),
            Requester::Agent { id } => format!("agent:{id}"),
        }
    }
}

/// 응답자. GUI 사용자 = `User`, 다른 에이전트 = `Agent`, 호스트 자동 (timeout/cancel) = `System`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Responder {
    User,
    Agent { id: String },
    System,
}

impl Responder {
    pub fn wire_id(&self) -> String {
        match self {
            Responder::User => "local".to_string(),
            Responder::Agent { id } => format!("agent:{id}"),
            Responder::System => "_system".to_string(),
        }
    }
}

/// 한 요청이 제시하는 선택지. `key` 가 응답 식별, `label` 은 GUI 표시.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalChoice {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub destructive: bool,
}

impl ApprovalChoice {
    pub fn approve() -> Self {
        Self {
            key: "approve".to_string(),
            label: "Approve".to_string(),
            destructive: false,
        }
    }

    pub fn deny() -> Self {
        Self {
            key: "deny".to_string(),
            label: "Deny".to_string(),
            destructive: true,
        }
    }
}

/// 위험도. UI/notification 채널 선택에 사용.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Severity {
    #[default]
    Info,
    Warn,
    Danger,
}

/// 요청 본문.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: ApprovalId,
    pub requester: Requester,
    pub workspace_id: Option<u32>,
    pub surface_id: Option<u32>,
    pub title: String,
    pub body: Option<String>,
    pub choices: Vec<ApprovalChoice>,
    pub default_choice: Option<String>,
    pub timeout_ms: Option<u64>,
    pub severity: Severity,
    pub created_at: u64,
    /// 관용 필드. `command`, `cwd` 등을 호스트/CLI 가 인식.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// 상태 머신.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Responded {
        choice: String,
        by: Responder,
        comment: Option<String>,
        at: u64,
    },
    TimedOut {
        default_choice: Option<String>,
        at: u64,
    },
    Cancelled {
        at: u64,
    },
}

impl ApprovalState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, ApprovalState::Pending)
    }
}

/// 한 history 전이. 모든 상태 변화는 여기에 append.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub at: u64,
    pub transition: String, // "created" | "responded" | "timed_out" | "cancelled"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<Responder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// store 안에서 한 요청을 들고 있는 entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub request: ApprovalRequest,
    pub state: ApprovalState,
    pub history: Vec<HistoryEntry>,
}

impl ApprovalRecord {
    pub fn new(request: ApprovalRequest) -> Self {
        let at = request.created_at;
        Self {
            history: vec![HistoryEntry {
                at,
                transition: "created".to_string(),
                choice: None,
                by: None,
                comment: None,
            }],
            request,
            state: ApprovalState::Pending,
        }
    }
}

// ============================================================
// 에러
// ============================================================

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("approval not found: {0}")]
    NotFound(String),
    #[error("already responded: {0}")]
    AlreadyResponded(ApprovalId),
    #[error("self-response forbidden: requester and responder are the same caller")]
    SelfResponse,
    #[error("invalid choice: {0}")]
    InvalidChoice(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("timed out")]
    TimedOut,
    #[error("cancelled")]
    Cancelled,
    /// 다른 스레드가 store 락을 든 채 패닉했다. 기록과 waiter 목록이 서로 어긋난
    /// 중간 상태일 수 있어 **상태를 바꾸는 연산은 거절한다** —
    /// [`error-handling.md`](../../../docs/dev-guide/error-handling.md) "락 poison".
    #[error("approval store lock poisoned")]
    StorePoisoned,
}

// ============================================================
// Store
// ============================================================

/// Approval 들을 담는 메인 컨테이너. 호스트는 단일 인스턴스를 `CoreState::approval_store` 에
/// 들고 모든 IPC handler 가 공유한다. 내부 `Mutex` 로 thread-safe.
pub struct ApprovalStore {
    inner: Arc<Mutex<Inner>>,
    /// poison 을 이미 보고했는가. poison 은 sticky 라 이후 모든 호출이 같은 로그를
    /// 내게 되므로 첫 1 회만 남긴다(`approval.list` 를 폴링하는 에이전트가 있다).
    poison_reported: Arc<AtomicBool>,
}

struct Inner {
    records: HashMap<ApprovalId, ApprovalRecord>,
    waiters: HashMap<ApprovalId, Vec<SyncSender<WaitResult>>>,
}

/// 상태 전이가 일어났을 때 호스트가 받을 후처리 정보. memory 영속, popup 닫기,
/// notification 발행 등에 쓰인다.
#[derive(Debug, Clone)]
pub struct StateChange {
    pub record: ApprovalRecord,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy)]
pub enum ChangeKind {
    Created,
    Responded,
    TimedOut,
    Cancelled,
}

/// `await_response` 의 반환 변종.
#[derive(Debug, Clone)]
pub enum WaitOutcome {
    Responded {
        choice: String,
        by: Responder,
        comment: Option<String>,
    },
    TimedOut {
        default_choice: Option<String>,
    },
    Cancelled,
}

#[derive(Debug, Clone)]
enum WaitResult {
    Responded {
        choice: String,
        by: Responder,
        comment: Option<String>,
    },
    TimedOut {
        default_choice: Option<String>,
    },
    Cancelled,
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalStore {
    /// 상태를 **바꾸는** 연산의 락 획득. poison 이면 [`ApprovalError::StorePoisoned`].
    ///
    /// `respond`/`cancel` 의 임계구역은 `record.state` → `record.history` →
    /// `waiters` 를 **순서대로** 갱신한다. 중간에 패닉이 나면 "응답됨으로 표시됐지만
    /// 대기자에게 통지되지 않은" 기록이 남을 수 있다 — 승인은 에이전트 행동을 막는
    /// 관문이라 그 중간 상태를 신뢰하고 진행하면 안 된다. 그래서 복구하지 않고 거절한다.
    ///
    /// 반대로 **패닉하지도** 않는다: 이 store 는 승인 popup(메인 스레드)에서도
    /// 호출되므로 패닉은 모든 창의 터미널 세션을 함께 죽인다
    /// ([`error-handling.md`](../../../docs/dev-guide/error-handling.md) "락 poison").
    fn lock_for_write(&self) -> Result<MutexGuard<'_, Inner>, ApprovalError> {
        self.inner.lock().map_err(|_| {
            self.report_poison("refusing the state change");
            ApprovalError::StorePoisoned
        })
    }

    /// 읽기·덮어쓰기 전용 락 획득. 호출자에게 에러 채널이 없고(`get`/`list`/`insert`),
    /// 부분 갱신된 기록을 **읽는** 것은 판단이 아니라 표시라 복구해서 진행한다.
    fn lock_recovering(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            self.report_poison("recovering for a read");
            poisoned.into_inner()
        })
    }

    fn report_poison(&self, action: &str) {
        if !self.poison_reported.swap(true, Ordering::Relaxed) {
            tracing::error!(
                "approval store mutex poisoned — a thread panicked while holding it; {action}. \
                 Later occurrences are not logged."
            );
        }
    }
}

impl ApprovalStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                records: HashMap::new(),
                waiters: HashMap::new(),
            })),
            poison_reported: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 신규 요청 등록. `choices` 가 비면 `Approve/Deny` 기본 채택. `default_choice` 가
    /// 명시되면 choices 안에 있어야 한다.
    pub fn request(&self, mut request: ApprovalRequest) -> Result<StateChange, ApprovalError> {
        if request.title.is_empty() {
            return Err(ApprovalError::InvalidRequest("title is empty".into()));
        }
        if request.choices.is_empty() {
            request.choices = vec![ApprovalChoice::approve(), ApprovalChoice::deny()];
        }
        if let Some(default) = &request.default_choice
            && !request.choices.iter().any(|c| &c.key == default)
        {
            return Err(ApprovalError::InvalidChoice(format!(
                "default_choice '{default}' not in choices"
            )));
        }
        if request.created_at == 0 {
            request.created_at = now_ms();
        }
        let id = request.id.clone();
        let record = ApprovalRecord::new(request);
        let mut g = self.lock_for_write()?;
        if g.records.contains_key(&id) {
            return Err(ApprovalError::InvalidRequest(format!("duplicate id: {id}")));
        }
        g.records.insert(id.clone(), record.clone());
        Ok(StateChange {
            record,
            kind: ChangeKind::Created,
        })
    }

    /// 응답 적용. 이미 종료된 요청이면 `AlreadyResponded`. self-response 면 `SelfResponse`.
    pub fn respond(
        &self,
        id: &ApprovalId,
        choice: String,
        by: Responder,
        comment: Option<String>,
    ) -> Result<StateChange, ApprovalError> {
        let mut g = self.lock_for_write()?;
        let record = g
            .records
            .get_mut(id)
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
        if record.state.is_terminal() {
            return Err(ApprovalError::AlreadyResponded(id.clone()));
        }
        if record.request.requester.matches_responder(&by) {
            return Err(ApprovalError::SelfResponse);
        }
        if !record.request.choices.iter().any(|c| c.key == choice) {
            return Err(ApprovalError::InvalidChoice(choice));
        }
        let at = now_ms();
        record.state = ApprovalState::Responded {
            choice: choice.clone(),
            by: by.clone(),
            comment: comment.clone(),
            at,
        };
        record.history.push(HistoryEntry {
            at,
            transition: "responded".to_string(),
            choice: Some(choice.clone()),
            by: Some(by.clone()),
            comment: comment.clone(),
        });
        let record = record.clone();
        if let Some(waiters) = g.waiters.remove(id) {
            for tx in waiters {
                let responded = WaitResult::Responded {
                    choice: choice.clone(),
                    by: by.clone(),
                    comment: comment.clone(),
                };
                let _ = tx.try_send(responded); // 수신측 drop 가능 — try_send 실패 무시.
            }
        }
        Ok(StateChange {
            record,
            kind: ChangeKind::Responded,
        })
    }

    /// 요청 취소. self-cancel 검증 없음 — 어느 caller 든 가능. (사용자 / 권한
    /// 검증은 IPC handler 단에서 처리.)
    pub fn cancel(&self, id: &ApprovalId) -> Result<StateChange, ApprovalError> {
        let mut g = self.lock_for_write()?;
        let record = g
            .records
            .get_mut(id)
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
        if record.state.is_terminal() {
            return Err(ApprovalError::AlreadyResponded(id.clone()));
        }
        let at = now_ms();
        record.state = ApprovalState::Cancelled { at };
        record.history.push(HistoryEntry {
            at,
            transition: "cancelled".to_string(),
            choice: None,
            by: None,
            comment: None,
        });
        let record = record.clone();
        if let Some(waiters) = g.waiters.remove(id) {
            for tx in waiters {
                let _ = tx.try_send(WaitResult::Cancelled); // 수신측이 이미 drop 됐을 수 있음 — 무시
            }
        }
        Ok(StateChange {
            record,
            kind: ChangeKind::Cancelled,
        })
    }

    /// 응답 대기. blocking. timeout 만료되면 자동으로 TimedOut 상태로 전이하고
    /// `WaitOutcome::TimedOut` 반환. `timeout_ms` 가 None 이면 무한 대기.
    /// 이미 종료된 요청이면 즉시 결과 반환.
    pub fn await_response(
        &self,
        id: &ApprovalId,
        timeout_ms: Option<u64>,
    ) -> Result<WaitOutcome, ApprovalError> {
        let rx = {
            let mut g = self.lock_for_write()?;
            let record = g
                .records
                .get(id)
                .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
            if let Some(outcome) = terminal_to_outcome(&record.state) {
                return Ok(outcome);
            }
            let (tx, rx) = sync_channel::<WaitResult>(1);
            g.waiters.entry(id.clone()).or_default().push(tx);
            rx
        };
        let result = match timeout_ms {
            Some(ms) => self.wait_with_timeout(id, &rx, Duration::from_millis(ms)),
            None => match rx.recv() {
                Ok(r) => r,
                Err(_) => WaitResult::Cancelled,
            },
        };
        Ok(match result {
            WaitResult::Responded {
                choice,
                by,
                comment,
            } => WaitOutcome::Responded {
                choice,
                by,
                comment,
            },
            WaitResult::TimedOut { default_choice } => WaitOutcome::TimedOut { default_choice },
            WaitResult::Cancelled => WaitOutcome::Cancelled,
        })
    }

    /// 응답 대기 — timeout 만료 처리. 만료되면 store 상태를 TimedOut 으로 전이.
    fn wait_with_timeout(
        &self,
        id: &ApprovalId,
        rx: &Receiver<WaitResult>,
        timeout: Duration,
    ) -> WaitResult {
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => {
                // store 상태 전이. 이미 누군가 응답했으면 (race) 그 결과를 받아 사용.
                //
                // poison 이면 `Cancelled` 로 접는다 — 이 함수는 `WaitResult` 만 돌려줄 수
                // 있고, record 가 사라졌을 때와 같은 처리다. 여기서 타임아웃 전이를 강행하면
                // 신뢰할 수 없는 중간 상태 위에 또 한 번 상태를 얹게 된다.
                let Ok(mut g) = self.inner.lock() else {
                    self.report_poison("cancelling the wait instead of applying the timeout");
                    return WaitResult::Cancelled;
                };
                let Some(record) = g.records.get_mut(id) else {
                    return WaitResult::Cancelled;
                };
                if let Some(outcome) = terminal_to_outcome(&record.state) {
                    return match outcome {
                        WaitOutcome::Responded {
                            choice,
                            by,
                            comment,
                        } => WaitResult::Responded {
                            choice,
                            by,
                            comment,
                        },
                        WaitOutcome::TimedOut { default_choice } => {
                            WaitResult::TimedOut { default_choice }
                        }
                        WaitOutcome::Cancelled => WaitResult::Cancelled,
                    };
                }
                let default_choice = record.request.default_choice.clone();
                let at = now_ms();
                record.state = ApprovalState::TimedOut {
                    default_choice: default_choice.clone(),
                    at,
                };
                record.history.push(HistoryEntry {
                    at,
                    transition: "timed_out".to_string(),
                    choice: default_choice.clone(),
                    by: Some(Responder::System),
                    comment: None,
                });
                // 같은 id 의 다른 waiter 도 모두 깨운다.
                if let Some(waiters) = g.waiters.remove(id) {
                    for tx in waiters {
                        let timed_out = WaitResult::TimedOut {
                            default_choice: default_choice.clone(),
                        };
                        let _ = tx.try_send(timed_out); // 수신측 drop 가능 — try_send 실패 무시.
                    }
                }
                WaitResult::TimedOut { default_choice }
            }
            Err(RecvTimeoutError::Disconnected) => WaitResult::Cancelled,
        }
    }

    /// 단일 record 조회.
    pub fn get(&self, id: &ApprovalId) -> Option<ApprovalRecord> {
        let g = self.lock_recovering();
        g.records.get(id).cloned()
    }

    /// 모든 record. 필터는 호출자가 적용.
    pub fn list(&self) -> Vec<ApprovalRecord> {
        let g = self.lock_recovering();
        g.records.values().cloned().collect()
    }

    /// 외부 (e.g. memory rehydrate) 가 record 를 강제 주입. 이미 같은 id 있으면 덮어쓴다.
    /// 호스트 부팅 시 memory 에서 복원하는 시나리오용.
    pub fn insert(&self, record: ApprovalRecord) {
        let mut g = self.lock_recovering();
        g.records.insert(record.request.id.clone(), record);
    }
}

fn terminal_to_outcome(state: &ApprovalState) -> Option<WaitOutcome> {
    match state {
        ApprovalState::Pending => None,
        ApprovalState::Responded {
            choice,
            by,
            comment,
            ..
        } => Some(WaitOutcome::Responded {
            choice: choice.clone(),
            by: by.clone(),
            comment: comment.clone(),
        }),
        ApprovalState::TimedOut { default_choice, .. } => Some(WaitOutcome::TimedOut {
            default_choice: default_choice.clone(),
        }),
        ApprovalState::Cancelled { .. } => Some(WaitOutcome::Cancelled),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
