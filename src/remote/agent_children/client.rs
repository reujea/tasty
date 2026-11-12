//! client 쪽 원격 자식 상태. plugin host 호출(IPC 처리기)과 attach 세션 pump 가 함께 쓴다.
//!
//! 처리기는 세션에 직접 닿지 않으므로 보낼 요청을 여기에 쌓고, pump 가 세션의 ID 매핑으로 바꿔 보낸다.
//! 원격 자식 기록(L↔R)은 mirror surface 가 사라져도 종료 통지를 받을 때까지 남긴다(tombstone).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use tasty_ipc::agent_child::{AckOutcome, CallOutcome};

use crate::core::child_parent::ParentKey;

/// plugin 에 응답하기까지 기다리는 상한. plugin SDK 호출 상한(전용 핸들 40초)보다 먼저 답한다.
pub(crate) const CALL_REPLY_LIMIT: Duration = Duration::from_secs(35);
/// 응답 뒤에도 결과를 기다려 늦게 생긴 자식을 재결합하는 기간.
pub(crate) const LATE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// mirror surface 가 사라진 뒤 그 자식의 늦은 통지를 받아 주는 기간.
pub(crate) const TOMBSTONE_TTL: Duration = Duration::from_secs(10 * 60);
/// 중복 확인용으로 기억하는 통지 key 수(세션마다).
const SEEN_CAP: usize = 4096;

/// 원격 자식을 결과불명으로 끝낸 call 의 오류 코드.
pub(crate) const ERR_OUTCOME_UNKNOWN: i32 = -32010;

#[derive(Debug, Clone)]
pub(crate) enum Outgoing {
    Call {
        call_id: u64,
        namespace: String,
        method: String,
        anchor: u32,
        target: Option<u32>,
        params: serde_json::Value,
        /// call 접수 때 client의 `general.inherit_cwd`. 꺼져 있으면 "상속 안 함"을 보낸다.
        inherit: bool,
        /// 새 자식의 cwd 상속 원본(로컬 mirror surface). 보내기 직전 client View가 anchor의 pane에서 고른
        /// 탭의 선택 surface로 정한다. 고르지 못하면 싣지 않는다(서버 fallback).
        source: Option<u32>,
    },
    Commit {
        call_id: u64,
        remote: Vec<u32>,
    },
    Ack {
        key: String,
        outcome: AckOutcome,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CallState {
    /// 아직 서버에 보내지 않았거나 결과를 기다린다.
    Waiting,
    /// 서버 결과를 받았고 새 자식을 이 client 에 등록하는 중이다.
    Adopting,
    /// plugin 에 돌려줄 결과가 준비됐다.
    Done,
    /// 결과를 알 수 없다(상한·서버 재시작).
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Adopted {
    pub local: u32,
    pub remote: u32,
    pub index: u32,
}

#[derive(Debug)]
pub(crate) struct Call {
    pub workspace: u32,
    pub plugin: String,
    pub namespace: String,
    pub parent: Option<ParentKey>,
    pub role: Option<String>,
    pub nickname: Option<String>,
    pub started: Instant,
    pub sent: bool,
    pub state: CallState,
    pub outcome: Option<CallOutcome>,
    pub spawned: Vec<u32>,
    pub adopted: Vec<Adopted>,
    /// mirror 에 나타났지만 부모 아래 등록하지 못한 자식(로컬, 원격). plugin 이 late_spawn 으로 받아 닫는다.
    pub unregistered: Vec<(u32, u32)>,
    /// plugin 에 이미 응답했다. 그 뒤에 온 결과는 늦은 생성으로 처리한다.
    pub replied: bool,
    pub committed: bool,
    /// plugin 에 실패·결과불명으로 답했다. 그 뒤 생긴 자식은 수신 메서드로 재결합한다.
    pub late: bool,
    /// 재결합 요청을 plugin 에 넣었다. 일시 실패면 이 시각 뒤에 다시 넣는다.
    pub late_sent: bool,
    pub late_retry_at: Option<Instant>,
    /// 서버에 보낸 요청 본문. 재연결 뒤 같은 call_id 로 다시 묻는 데 쓴다.
    pub wire: Option<serde_json::Value>,
}

#[derive(Clone)]
pub(crate) struct Child {
    pub workspace: u32,
    pub remote: u32,
    pub plugin: String,
    pub namespace: String,
    /// mirror surface 가 사라진 시각. 그 뒤 [`TOMBSTONE_TTL`] 동안만 남긴다.
    pub gone_at: Option<Instant>,
    /// 사라질 때 옮겨 둔 그 surface 의 훅. 늦은 이벤트를 이 훅으로 실행한다.
    pub hooks: Option<std::sync::Arc<std::sync::Mutex<tasty_hooks::HookManager>>>,
}

#[derive(Debug, Default)]
pub(crate) struct Session {
    pub capable: bool,
    pub connected: bool,
    pub token: Option<String>,
    pub origin: Option<u64>,
    seen: VecDeque<String>,
    seen_set: HashSet<String>,
    /// 원격 surface 별 마지막으로 적용한 State 통지 순번.
    pub state_seq: HashMap<u32, u64>,
}

impl Session {
    pub(crate) fn has_seen(&self, key: &str) -> bool {
        self.seen_set.contains(key)
    }

    /// 처음 본 key 면 기록하고 true.
    pub(crate) fn first_seen(&mut self, key: &str) -> bool {
        if self.seen_set.contains(key) {
            return false;
        }
        if self.seen.len() >= SEEN_CAP
            && let Some(old) = self.seen.pop_front()
        {
            self.seen_set.remove(&old);
        }
        self.seen.push_back(key.to_owned());
        self.seen_set.insert(key.to_owned());
        true
    }
}

#[derive(Default)]
pub(crate) struct Client {
    next_call: u64,
    pub sessions: HashMap<u32, Session>,
    pub outgoing: Vec<(u32, Outgoing)>,
    pub calls: HashMap<u64, Call>,
    /// 로컬 mirror surface → 원격 자식.
    pub children: HashMap<u32, Child>,
    /// plugin 수신 메서드가 처리 중인 통지 key. 재전송을 plugin 에 다시 넣지 않는다.
    pub receiving: HashSet<String>,
    /// holder 연결에서 받은 메시지. 세션 pump 가 처리한다.
    pub inbox: Vec<(u32, tasty_ipc::agent_child::AgentServerMsg)>,
}

impl Client {
    pub(crate) fn next_call_id(&mut self) -> u64 {
        self.next_call += 1;
        self.next_call
    }

    pub(crate) fn local_of(&self, workspace: u32, remote: u32) -> Option<u32> {
        self.children
            .iter()
            .find(|(_, c)| c.workspace == workspace && c.remote == remote)
            .map(|(l, _)| *l)
    }

    /// 서버가 출처를 다시 알려 줬다. 모르는 토큰이었다면(서버 재시작) 진행 중 call 을 결과불명으로 끝낸다.
    pub(crate) fn hello_ack(&mut self, workspace: u32, token: String, origin: u64, resumed: bool) {
        let session = self.sessions.entry(workspace).or_default();
        let had = session.token.is_some();
        session.token = Some(token);
        session.origin = Some(origin);
        if had && !resumed {
            for call in self.calls.values_mut().filter(|c| c.workspace == workspace) {
                if matches!(call.state, CallState::Waiting) {
                    call.state = CallState::Unknown("the remote instance restarted".into());
                }
            }
        } else {
            // 재연결이면 결과를 받지 못한 call 을 같은 id 로 다시 묻는다.
            for call in self.calls.values_mut().filter(|c| c.workspace == workspace) {
                if matches!(call.state, CallState::Waiting) {
                    call.sent = false;
                }
            }
        }
    }

    /// 서버에 보내기 전에 거절한 call 을 실패로 끝낸다. 서버는 아무것도 만들지 않았다.
    pub(crate) fn refuse_call(&mut self, call_id: u64, reason: String) {
        tracing::warn!("attach agent call {call_id} was not sent: {reason}");
        self.call_result(
            call_id,
            CallOutcome::Error(tasty_ipc::agent_child::CallError {
                code: -32602,
                message: reason,
            }),
            Vec::new(),
        );
    }

    /// 결과를 기록한다. 자식이 없거나 등록할 부모가 없으면 바로 끝난다.
    pub(crate) fn call_result(&mut self, call_id: u64, outcome: CallOutcome, spawned: Vec<u32>) {
        let Some(call) = self.calls.get_mut(&call_id) else {
            tracing::warn!("attach agent result for unknown call {call_id}");
            return;
        };
        if !matches!(call.state, CallState::Waiting | CallState::Unknown(_)) {
            return;
        }
        call.outcome = Some(outcome);
        call.spawned = spawned;
        call.state = if call.spawned.is_empty() || call.parent.is_none() {
            CallState::Done
        } else {
            CallState::Adopting
        };
    }

    /// 새 자식 등록이 끝났다.
    /// `unregistered` 는 등록에 실패한 자식이다. 원격 자식으로는 기록해 plugin 이 원격 kill 로 닫을 수 있게 한다.
    pub(crate) fn adopted(
        &mut self,
        call_id: u64,
        adopted: Vec<Adopted>,
        unregistered: Vec<(u32, u32)>,
    ) {
        let Some(call) = self.calls.get_mut(&call_id) else {
            return;
        };
        let surfaces = adopted
            .iter()
            .map(|a| (a.local, a.remote))
            .chain(unregistered.iter().copied());
        for (local, remote) in surfaces {
            self.children.insert(
                local,
                Child {
                    workspace: call.workspace,
                    remote,
                    plugin: call.plugin.clone(),
                    namespace: call.namespace.clone(),
                    gone_at: None,
                    hooks: None,
                },
            );
        }
        call.adopted = adopted;
        call.unregistered = unregistered;
        call.state = CallState::Done;
    }

    /// commit 할 원격 surface. plugin 소유 call 이고 끝났을 때만.
    pub(crate) fn commit(&mut self, call_id: u64, plugin: &str) -> Result<Vec<u32>, String> {
        let call = self
            .calls
            .get_mut(&call_id)
            .filter(|c| c.plugin == plugin)
            .ok_or_else(|| format!("call {call_id} is not this plugin's"))?;
        if call.state != CallState::Done {
            return Err(format!("call {call_id} has no result yet"));
        }
        call.committed = true;
        let remote: Vec<u32> = call.adopted.iter().map(|a| a.remote).collect();
        self.outgoing.push((
            call.workspace,
            Outgoing::Commit {
                call_id,
                remote: remote.clone(),
            },
        ));
        Ok(remote)
    }

    /// 끝났거나 오래된 call·tombstone 을 정리한다.
    /// mirror workspace 의 attach 세션이 없어졌다. 결과를 받지 못한 call 은 결과불명이 되고, 그 workspace 의
    /// 자식 기록은 닫힌 것으로 둔다. 새 attach 는 새 출처라 이전 자식의 통지를 받지 않는다.
    pub(crate) fn forget_session(&mut self, workspace: u32, now: Instant) {
        self.sessions.remove(&workspace);
        for call in self.calls.values_mut().filter(|c| c.workspace == workspace) {
            if matches!(call.state, CallState::Waiting | CallState::Adopting) {
                call.state = CallState::Unknown("the attach connection closed".into());
            }
        }
        for child in self
            .children
            .values_mut()
            .filter(|c| c.workspace == workspace)
        {
            child.gone_at.get_or_insert(now);
        }
    }

    pub(crate) fn expire(&mut self, now: Instant) {
        self.calls.retain(|_, c| {
            let age = now.duration_since(c.started);
            match c.state {
                CallState::Done => !(c.replied && (c.committed || age > LATE_LIMIT)),
                _ => age <= LATE_LIMIT,
            }
        });
        self.children.retain(|_, c| {
            c.gone_at
                .is_none_or(|at| now.duration_since(at) < TOMBSTONE_TTL)
        });
    }
}

use std::sync::{LazyLock, Mutex, MutexGuard};

use serde_json::{Value, json};
use tasty_ipc::agent_child::AgentServerMsg;
use tasty_ipc::protocol::JsonRpcResponse;

static CLIENT: LazyLock<Mutex<Client>> = LazyLock::new(|| Mutex::new(Client::default()));

/// client 쪽 원격 자식 상태.
pub(crate) fn client() -> MutexGuard<'static, Client> {
    CLIENT.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("attach agent client state was poisoned; keeping its entries");
        poisoned.into_inner()
    })
}

/// `attach.agent_call` 의 미룬 응답. 아직이면 None 이다. 상한을 넘으면 결과불명으로 답하고, 그 뒤 생긴
/// 자식은 plugin 수신 메서드로 재결합한다.
pub(crate) fn call_reply(
    call_id: u64,
    id: Value,
    now: Instant,
    limit: std::time::Duration,
) -> Option<JsonRpcResponse> {
    let mut client = client();
    let Some(call) = client.calls.get_mut(&call_id) else {
        return Some(JsonRpcResponse::error(
            id,
            ERR_OUTCOME_UNKNOWN,
            format!("remote agent call {call_id} outcome unknown: its record expired"),
        ));
    };
    let unknown = |why: &str| {
        JsonRpcResponse::error(
            id.clone(),
            ERR_OUTCOME_UNKNOWN,
            format!("remote agent call {call_id} outcome unknown: {why}"),
        )
    };
    let response = match (&call.state, &call.outcome) {
        // 등록하지 못한 자식은 성공으로 돌려주지 않는다. late 로 넘겨 plugin 이 그 자식을 닫게 한다.
        (CallState::Done, Some(CallOutcome::Ok { .. })) if !call.unregistered.is_empty() => {
            call.late = true;
            JsonRpcResponse::error(
                id.clone(),
                -32000,
                format!(
                    "remote agent call {call_id}: the remote child could not be registered under its parent here and is being closed"
                ),
            )
        }
        (CallState::Done, Some(CallOutcome::Ok { result })) => JsonRpcResponse::success(
            id.clone(),
            json!({
                "call_id": call_id,
                "result": result,
                "children": call.adopted.iter().map(|a| json!({
                    "surface_id": a.local,
                    "remote_surface_id": a.remote,
                    "index": a.index,
                })).collect::<Vec<_>>(),
            }),
        ),
        (CallState::Done, Some(CallOutcome::Error(e))) => {
            call.late = true;
            JsonRpcResponse::error(id.clone(), e.code, &e.message)
        }
        (CallState::Done, None) => {
            call.late = true;
            unknown("no result")
        }
        (CallState::Unknown(why), _) => {
            call.late = true;
            unknown(why)
        }
        (CallState::Waiting | CallState::Adopting, _)
            if now.duration_since(call.started) > limit =>
        {
            call.late = true;
            unknown("no answer within the time limit")
        }
        _ => return None,
    };
    call.replied = true;
    Some(response)
}

/// client 쪽 요약(조회용).
pub(crate) fn client_summary() -> Value {
    let client = client();
    json!({
        "calls": client.calls.iter().map(|(id, c)| json!({
            "call_id": id,
            "workspace": c.workspace,
            "namespace": c.namespace,
            "state": format!("{:?}", c.state),
            "committed": c.committed,
            "late": c.late,
        })).collect::<Vec<_>>(),
        "children": client.children.iter().map(|(l, c)| json!({
            "surface_id": l,
            "remote_surface_id": c.remote,
            "workspace": c.workspace,
            "namespace": c.namespace,
            "gone": c.gone_at.is_some(),
        })).collect::<Vec<_>>(),
    })
}

/// holder 연결에서 받은 메시지를 세션 pump 로 넘긴다.
pub(crate) fn client_receive(workspace: u32, msg: AgentServerMsg) {
    client().inbox.push((workspace, msg));
}

/// 원격 자식 mirror surface 가 닫힌다. 늦은 종료 통지를 위해 훅을 옮겨 두고 기록을 남긴다.
pub(crate) fn client_surface_closing(
    surface: u32,
    hooks: Vec<(tasty_hooks::HookEvent, tasty_hooks::HookBinding, bool)>,
) {
    let mut client = client();
    let Some(child) = client.children.get_mut(&surface) else {
        return;
    };
    if child.gone_at.is_some() {
        return;
    }
    child.gone_at = Some(Instant::now());
    let mut manager = tasty_hooks::HookManager::new();
    for (event, binding, once) in hooks {
        manager.add_hook(surface, event, binding, once);
    }
    child.hooks = Some(std::sync::Arc::new(Mutex::new(manager)));
}

/// 이 client 에 등록된 원격 자식이면 그 holder 연결이 지금 이어져 있는지. 원격 자식이 아니면 None 이다.
pub(crate) fn remote_child_connected(surface: u32) -> Option<bool> {
    let client = client();
    let child = client.children.get(&surface)?;
    Some(
        child.gone_at.is_none()
            && client
                .sessions
                .get(&child.workspace)
                .is_some_and(|s| s.connected && s.origin.is_some()),
    )
}

pub(crate) fn is_client_child(surface: u32) -> bool {
    client().children.contains_key(&surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(ws: u32, parent: Option<ParentKey>) -> Call {
        Call {
            workspace: ws,
            plugin: "p".into(),
            namespace: "p".into(),
            parent,
            role: None,
            nickname: None,
            started: Instant::now(),
            sent: true,
            state: CallState::Waiting,
            outcome: None,
            spawned: Vec::new(),
            adopted: Vec::new(),
            unregistered: Vec::new(),
            replied: false,
            committed: false,
            late: false,
            late_sent: false,
            late_retry_at: None,
            wire: None,
        }
    }

    /// 보내기 전에 거절한 call 은 결과불명이 아니라 실패다. plugin 은 다시 보내지 않고 오류로 끝낸다.
    #[test]
    fn a_call_refused_before_sending_ends_as_a_failure() {
        let mut c = Client::default();
        c.calls.insert(1, call(4, Some(ParentKey::Surface(9))));
        c.refuse_call(1, "inherit_cwd surface 7 is outside the mirror".into());
        let refused = &c.calls[&1];
        assert_eq!(refused.state, CallState::Done);
        assert_eq!(
            refused.outcome,
            Some(CallOutcome::Error(tasty_ipc::agent_child::CallError {
                code: -32602,
                message: "inherit_cwd surface 7 is outside the mirror".into(),
            }))
        );
    }

    #[test]
    fn a_server_restart_ends_waiting_calls_as_unknown_and_a_reconnect_resends_them() {
        let mut c = Client::default();
        c.calls.insert(1, call(4, None));
        c.hello_ack(4, "t".into(), 1, false);
        // 처음 hello 는 재시작이 아니다.
        assert_eq!(c.calls[&1].state, CallState::Waiting);
        assert!(!c.calls[&1].sent);
        c.calls.get_mut(&1).unwrap().sent = true;
        c.hello_ack(4, "t2".into(), 2, false);
        assert!(matches!(c.calls[&1].state, CallState::Unknown(_)));
    }

    #[test]
    fn spawned_children_are_adopted_before_the_call_is_done_and_commit_needs_the_owner() {
        let mut c = Client::default();
        c.calls.insert(1, call(4, Some(ParentKey::Surface(3))));
        c.call_result(
            1,
            CallOutcome::Ok {
                result: serde_json::json!({}),
            },
            vec![9],
        );
        assert_eq!(c.calls[&1].state, CallState::Adopting);
        assert!(c.commit(1, "p").is_err());
        c.adopted(
            1,
            vec![Adopted {
                local: 20,
                remote: 9,
                index: 1,
            }],
            Vec::new(),
        );
        assert_eq!(c.local_of(4, 9), Some(20));
        assert!(c.commit(1, "q").is_err());
        assert_eq!(c.commit(1, "p").unwrap(), vec![9]);
        assert!(matches!(
            c.outgoing.last(),
            Some((4, Outgoing::Commit { call_id: 1, .. }))
        ));
    }

    #[test]
    fn a_closed_session_ends_its_calls_as_unknown_and_marks_its_children_gone() {
        let mut c = Client::default();
        c.calls.insert(1, call(4, None));
        c.calls.insert(2, call(5, None));
        c.children.insert(
            20,
            Child {
                workspace: 4,
                remote: 9,
                plugin: "p".into(),
                namespace: "p".into(),
                gone_at: None,
                hooks: None,
            },
        );
        c.sessions.entry(4).or_default();
        c.forget_session(4, Instant::now());
        assert!(matches!(c.calls[&1].state, CallState::Unknown(_)));
        assert_eq!(c.calls[&2].state, CallState::Waiting);
        assert!(c.children[&20].gone_at.is_some());
        assert!(!c.sessions.contains_key(&4));
    }

    /// 등록하지 못한 자식은 성공으로 돌려주지 않고, plugin 이 원격 kill 로 닫을 수 있게 원격 자식으로 남긴다.
    #[test]
    fn a_child_that_could_not_be_registered_goes_to_the_late_path() {
        // 전역 상태를 쓰므로 다른 시험과 겹치지 않는 번호를 쓴다.
        let id = 9_100_002;
        {
            let mut c = client();
            let mut adopting = call(4, Some(ParentKey::Surface(3)));
            adopting.outcome = Some(CallOutcome::Ok {
                result: serde_json::json!({}),
            });
            adopting.spawned = vec![9];
            c.calls.insert(id, adopting);
            c.adopted(id, Vec::new(), vec![(21, 9)]);
        }
        let reply = call_reply(
            id,
            serde_json::json!(1),
            Instant::now(),
            Duration::from_secs(35),
        )
        .expect("answered");
        let mut c = client();
        let call = c.calls.remove(&id).expect("kept");
        let child = c.children.remove(&21);
        assert_eq!(reply.error.as_ref().map(|e| e.code), Some(-32000));
        assert!(call.late, "the plugin closes it through late_spawn");
        assert_eq!(call.unregistered, vec![(21, 9)]);
        assert_eq!(child.map(|ch| ch.remote), Some(9), "kill can target it");
    }

    #[test]
    fn a_child_that_has_not_appeared_leaves_the_outcome_unknown() {
        // 전역 상태를 쓰므로 다른 시험과 겹치지 않는 번호를 쓴다.
        let id = 9_100_001;
        let mut waiting = call(4, Some(ParentKey::Surface(3)));
        waiting.state = CallState::Adopting;
        waiting.spawned = vec![9];
        waiting.started = Instant::now() - Duration::from_secs(40);
        client().calls.insert(id, waiting);
        let reply = call_reply(
            id,
            serde_json::json!(1),
            Instant::now(),
            Duration::from_secs(35),
        )
        .expect("answered");
        let late = client().calls.remove(&id).map(|c| c.late);
        assert_eq!(
            reply.error.as_ref().map(|e| e.code),
            Some(ERR_OUTCOME_UNKNOWN)
        );
        assert_eq!(
            late,
            Some(true),
            "a later appearance is rejoined through late_spawn"
        );
    }

    #[test]
    fn keys_are_seen_once() {
        let mut s = Session::default();
        assert!(s.first_seen("a"));
        assert!(!s.first_seen("a"));
    }
}
