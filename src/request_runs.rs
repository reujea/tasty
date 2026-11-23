//! 접수 응답 뒤에 끝나는 요청의 최종 결과. 접수 응답의 `run` 값으로 `run.result`가 조회한다.
//! 결과를 쓰는 쪽(메인 루프·작업 스레드)과 읽는 IPC 처리가 다르므로 프로세스 전역 잠금 안에 둔다.
//! 최근 [`KEEP`]건만 보관한다. 처음 기록한 최종 결과만 남고 뒤의 기록은 무시한다.
//! 예외는 [`overrule`]이며 시작 성공으로 판정한 뒤 그 판정이 틀렸음이 드러난 경우에만 쓴다.

use std::collections::VecDeque;
#[cfg(feature = "gui")]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{Map, Value, json};

/// 보관할 실행 수. 넘으면 가장 오래된 기록부터 지우며 지운 실행은 알 수 없는 run이 된다.
pub(crate) const KEEP: usize = 128;

/// 최종 상태. 진행 중은 `running`으로 보인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    Succeeded,
    /// 여러 단계 중 일부만 성공했다.
    Partial,
    Failed,
    /// 응답을 기다리다 포기해 결과를 모른다.
    Unknown,
    /// 사용자가 취소했거나 결과를 내기 전에 요청이 끝났다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(dead_code, reason = "only gui picker and view paths cancel a run")
    )]
    Cancelled,
}

impl RunState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug)]
struct Run {
    id: u64,
    method: &'static str,
    /// 조회를 허용할 호출자. None이면 로컬 호출자만 읽는다.
    owner: Option<String>,
    state: Option<RunState>,
    result: String,
    fields: Map<String, Value>,
}

impl Run {
    fn to_json(&self) -> Value {
        let mut v = Value::Object(self.fields.clone());
        v["run"] = json!(self.id);
        v["method"] = json!(self.method);
        v["done"] = json!(self.state.is_some());
        v["state"] = json!(self.state.map_or("running", RunState::as_str));
        v["result"] = json!(self.result);
        v
    }
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static POISONED: AtomicBool = AtomicBool::new(false);

fn runs() -> std::sync::MutexGuard<'static, VecDeque<Run>> {
    static RUNS: OnceLock<Mutex<VecDeque<Run>>> = OnceLock::new();
    crate::poison::recover_mutex(
        RUNS.get_or_init(Default::default).lock(),
        "request runs",
        &POISONED,
    )
}

fn merge(fields: &mut Map<String, Value>, extra: Value) {
    if let Value::Object(extra) = extra {
        fields.extend(extra);
    }
}

/// 진행 중인 실행을 만들고 run 값을 돌려준다. `fields`는 조회 결과에 그대로 실린다.
pub(crate) fn begin(method: &'static str, owner: Option<String>, fields: Value) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut map = Map::new();
    merge(&mut map, fields);
    let mut runs = runs();
    while runs.len() >= KEEP {
        runs.pop_front();
    }
    runs.push_back(Run {
        id,
        method,
        owner,
        state: None,
        result: "running".into(),
        fields: map,
    });
    id
}

/// 플러그인이 다른 호출자의 요청을 호스트로 다시 보내 시작한 실행을 원래 호출자에게 넘긴다.
/// 현재 소유자가 `from`일 때만 바꾼다.
#[cfg(feature = "gui")]
pub(crate) fn hand_over(id: u64, from: &str, to: Option<String>) -> bool {
    let mut runs = runs();
    match runs
        .iter_mut()
        .find(|r| r.id == id && r.owner.as_deref() == Some(from))
    {
        Some(run) => {
            run.owner = to;
            true
        }
        None => false,
    }
}

/// 동기 오류로 답한 실행은 조회 대상에서 지운다.
pub(crate) fn forget(id: u64) {
    runs().retain(|r| r.id != id);
}

/// 최종 결과를 기록한다. 이미 끝난 실행이면 바꾸지 않고 false를 돌려준다.
pub(crate) fn finish(id: u64, state: RunState, result: impl Into<String>, extra: Value) -> bool {
    let mut runs = runs();
    let Some(run) = runs.iter_mut().find(|r| r.id == id) else {
        return false;
    };
    if run.state.is_some() {
        return false;
    }
    run.state = Some(state);
    run.result = result.into();
    merge(&mut run.fields, extra);
    true
}

#[cfg(feature = "gui")]
/// journal에 들어가지 못한 명령의 run을 실패로 끝낸다. 오류가 없으면 적용되지 않았다고 적는다.
pub(crate) fn fail_unapplied(id: u64, applied: &anyhow::Result<()>) {
    let reason = match applied {
        Err(error) => error.to_string(),
        Ok(()) => "the request was not applied".to_string(),
    };
    finish(id, RunState::Failed, reason, Value::Null);
}

/// 이미 기록한 최종 결과를 실패로 바꾼다. 셸 핸들러가 시작 성공으로 끝난 뒤 종료를 확인하지
/// 못한 경우처럼, 앞선 판정의 전제가 무너졌을 때만 쓴다.
pub(crate) fn overrule(id: u64, state: RunState, result: impl Into<String>) {
    if let Some(run) = runs().iter_mut().find(|r| r.id == id) {
        run.state = Some(state);
        run.result = result.into();
    }
}

/// 끝난 뒤에도 정보 필드와 결과 문장을 덧붙인다. 상태는 바꾸지 않는다(셸 종료 코드 등).
pub(crate) fn amend(id: u64, result: Option<String>, extra: Value) {
    if let Some(run) = runs().iter_mut().find(|r| r.id == id) {
        if let Some(result) = result {
            run.result = result;
        }
        merge(&mut run.fields, extra);
    }
}

/// 로컬 호출자(`owner == None`)는 모든 실행을, 그 밖의 호출자는 자기가 시작한 실행만 읽는다.
pub(crate) fn lookup(id: u64, reader: Option<&str>) -> Option<Value> {
    runs()
        .iter()
        .find(|r| r.id == id && (reader.is_none() || r.owner.as_deref() == reader))
        .map(Run::to_json)
}

/// 조회 호출자. 로컬 IPC는 None, 플러그인·에이전트는 그 ID다.
pub(crate) fn reader_of(caller: &tasty_ipc::caller::CallerContext) -> Option<String> {
    match caller {
        tasty_ipc::caller::CallerContext::Local { .. } => None,
        other => Some(other.owner().to_string()),
    }
}

/// 결과를 내기 전에 사라질 수 있는 상태(팝업 등)에 붙여 둔다. 마지막 사본이 사라질 때까지
/// 최종 결과가 없으면 `cancelled`로 끝낸다. 결과를 넘겨받을 때는 [`RunGuard::take`]로 해제한다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
pub(crate) struct RunGuard(Arc<GuardInner>);

#[cfg(feature = "gui")]
#[derive(Debug)]
struct GuardInner {
    id: u64,
    armed: AtomicBool,
    reason: &'static str,
}

#[cfg(feature = "gui")]
impl RunGuard {
    pub(crate) fn new(id: u64, reason: &'static str) -> Self {
        Self(Arc::new(GuardInner {
            id,
            armed: AtomicBool::new(true),
            reason,
        }))
    }

    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }

    /// 감시를 끝내고 run을 돌려준다. 그 뒤의 최종 결과는 받은 쪽이 기록한다.
    pub(crate) fn take(&self) -> u64 {
        self.0.armed.store(false, Ordering::Relaxed);
        self.0.id
    }
}

#[cfg(feature = "gui")]
impl Drop for GuardInner {
    fn drop(&mut self) {
        if self.armed.load(Ordering::Relaxed) {
            finish(self.id, RunState::Cancelled, self.reason, Value::Null);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_final_result_stays() {
        let id = begin("t.run", None, json!({"path": "/a"}));
        assert_eq!(lookup(id, None).unwrap()["state"], "running");
        assert!(finish(
            id,
            RunState::Failed,
            "no handler",
            json!({"why": 1})
        ));
        assert!(!finish(id, RunState::Succeeded, "late", Value::Null));
        let v = lookup(id, None).unwrap();
        assert_eq!(v["state"], "failed");
        assert_eq!(v["result"], "no handler");
        assert_eq!(v["path"], "/a");
        assert_eq!(v["done"], true);
        assert_eq!(v["method"], "t.run");
    }

    #[test]
    fn a_plugin_reads_only_its_own_runs() {
        let mine = begin("t.run", Some("p.one".into()), Value::Null);
        assert!(lookup(mine, Some("p.one")).is_some());
        assert!(lookup(mine, Some("p.two")).is_none());
        assert!(lookup(mine, None).is_some());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn a_dropped_guard_cancels_an_unfinished_run() {
        let dropped = begin("t.run", None, Value::Null);
        drop(RunGuard::new(dropped, "closed"));
        assert_eq!(lookup(dropped, None).unwrap()["state"], "cancelled");

        let taken = begin("t.run", None, Value::Null);
        let guard = RunGuard::new(taken, "closed");
        let copy = guard.clone();
        assert_eq!(guard.take(), taken);
        drop(guard);
        drop(copy);
        assert_eq!(lookup(taken, None).unwrap()["state"], "running");
    }
}
