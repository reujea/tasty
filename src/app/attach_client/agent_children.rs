//! client 쪽 원격 자식 처리. 세션마다 출처 hello·call 송신·새 자식 등록·통지 적용·확인(ACK)을 한다.
//!
//! 통지는 이 client 의 기존 종점에 적용한다. 상태는 child registry, 이벤트는 mirror surface 의 훅,
//! 메타는 surface meta, plugin 고유 결과는 자식을 만든 plugin 의 수신 메서드다. 원격 이벤트·메타는
//! 그 plugin 의 이 머신 매니페스트 선언으로 다시 검사한다.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_ipc::agent_child::{
    AckOutcome, AgentClientMsg, AgentServerMsg, CallOutcome, Notice, PROCESS_EXIT,
};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_remote::client_session::{AttachClientSession, SessionState};

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::remote::agent_children::client::{Adopted, CallState, Outgoing};
use crate::remote::agent_children::{self as agents};
use crate::runtime::engine_access::EngineMut;

/// plugin 수신 메서드의 응답 대기. 메인 루프는 기다리지 않고 다음 pump 에서 거둔다.
enum Waiting {
    Notice {
        workspace: u32,
        key: String,
        rx: Receiver<JsonRpcResponse>,
    },
    Late {
        call_id: u64,
        plugin: String,
        rx: Receiver<JsonRpcResponse>,
    },
}
static WAITING: Mutex<Vec<Waiting>> = Mutex::new(Vec::new());

/// 새 자식 등록의 child registry 영수증.
struct Adopting {
    call_id: u64,
    pending: Vec<(u32, u32, crate::runtime::child_terminal::Ticket)>,
    /// 등록 요청부터 거절된 자식(로컬, 원격).
    failed: Vec<(u32, u32)>,
}
static ADOPTING: Mutex<Vec<Adopting>> = Mutex::new(Vec::new());

/// late_spawn 재결합이 일시 실패했을 때 다시 넣기까지의 간격.
const LATE_RETRY: Duration = Duration::from_secs(5);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("attach agent client wait list was poisoned; keeping its entries");
        poisoned.into_inner()
    })
}

fn send(sess: &AttachClientSession, msg: &AgentClientMsg) -> bool {
    match serde_json::to_value(msg) {
        Ok(value) => super::wire::send_control_frame(&sess.transport.frame_tx, &value)
            .map_err(|e| tracing::warn!("attach agent message not sent: {e}"))
            .is_ok(),
        Err(e) => {
            tracing::warn!("attach agent message did not serialize: {e}");
            false
        }
    }
}

/// 자식을 만든 plugin 의 이 머신 선언.
struct Decl {
    hook_events: Vec<String>,
    relay_meta: Vec<String>,
    receive: Option<String>,
}

fn decl(plugins: &Option<crate::plugin::PluginManager>, plugin: &str) -> Option<Decl> {
    let manifest = &plugins
        .as_ref()?
        .packages()
        .iter()
        .find(|p| p.manifest.id == plugin)?
        .manifest;
    let remote = &manifest.contributes.remote_attach;
    Some(Decl {
        hook_events: manifest
            .contributes
            .hook_events
            .iter()
            .map(|h| h.key.clone())
            .collect(),
        relay_meta: remote.relay_meta.clone(),
        receive: remote.receive.clone(),
    })
}

impl App {
    /// 세션 출력 적용 뒤 매 루프 부른다.
    pub(crate) fn pump_agent_children(&mut self) {
        let now = Instant::now();
        let exec = self.services.hook_executor();
        let mut meta_writes: Vec<(u32, String, Option<String>)> = Vec::new();
        self.collect_plugin_answers();
        let live: Vec<u32> = self
            .remote
            .sessions
            .iter()
            .map(|s| s.state.local_workspace)
            .collect();
        {
            let mut client = agents::client();
            let gone: Vec<u32> = client
                .sessions
                .keys()
                .filter(|ws| !live.contains(ws))
                .copied()
                .collect();
            for ws in gone {
                client.forget_session(ws, now);
            }
        }
        for idx in 0..self.remote.sessions.len() {
            let ws = self.remote.sessions[idx].state.local_workspace;
            self.hello(idx, ws);
            let inbox: Vec<AgentServerMsg> = {
                let mut client = agents::client();
                let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut client.inbox)
                    .into_iter()
                    .partition(|(w, _)| *w == ws);
                client.inbox = rest;
                mine.into_iter().map(|(_, m)| m).collect()
            };
            let host = super::output::mirror_engine_host(self, ws);
            let sess = &mut self.remote.sessions[idx];
            let engines = engines_mut!(self);
            let mut engine = match host {
                Some(super::output::MirrorOutputHost::Window(wid)) => {
                    engines.window_pair(wid).map(|(_, e)| e)
                }
                Some(super::output::MirrorOutputHost::Parked(id)) => {
                    engines.parked_session(id).map(|(_, e)| e)
                }
                None => None,
            };
            for msg in inbox {
                let sink = Sink {
                    engine: engine.as_mut(),
                    plugins: &mut self.plugin_manager,
                    exec: &exec,
                    meta_writes: &mut meta_writes,
                };
                apply(sess, sink, ws, msg);
            }
            flush_outgoing(sess, ws);
            if let Some(engine) = engine.as_mut() {
                adopt(sess, engine, ws);
            }
            late_spawns(&mut self.plugin_manager, ws, now);
        }
        poll_adoptions();
        for (surface, key, value) in meta_writes {
            let request = tasty_memory::writer::MemoryRequest::unit(
                key.len() + value.as_ref().map_or(0, String::len),
                move |m| match &value {
                    Some(value) => {
                        crate::surface_meta::SurfaceMetaStore::put(m, surface, &key, value)
                    }
                    None => crate::surface_meta::SurfaceMetaStore::remove(m, surface, &key),
                },
            );
            self.services
                .memory_access()
                .submit_detached(request, "attach agent child meta", None);
        }
        agents::client().expire(now);
    }

    /// 새 연결마다 출처 토큰을 한 번 보낸다. 구 서버 연결에는 보내지 않는다.
    fn hello(&mut self, idx: usize, ws: u32) {
        let sess = &mut self.remote.sessions[idx];
        let connected = sess.state.phase == SessionState::Connected
            && !sess
                .transport
                .disconnected
                .load(std::sync::atomic::Ordering::SeqCst);
        let token = {
            let mut client = agents::client();
            let entry = client.sessions.entry(ws).or_default();
            entry.capable = sess.transport.agent_children;
            entry.connected = connected;
            entry.token.clone()
        };
        if sess.transport.agent_children && connected && !sess.transport.agent_hello_sent {
            sess.transport.agent_hello_sent = send(
                sess,
                &AgentClientMsg::AgentHello {
                    origin_token: token,
                },
            );
        }
    }

    /// plugin 수신 메서드 응답을 거두고 ACK 나 commit 으로 바꾼다.
    fn collect_plugin_answers(&mut self) {
        let mut acks = Vec::new();
        let mut commits = Vec::new();
        let mut late_failed = Vec::new();
        lock(&WAITING).retain(|w| {
            let (Waiting::Notice { rx, .. } | Waiting::Late { rx, .. }) = w;
            let response = match rx.try_recv() {
                Ok(response) => Some(response),
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => None,
            };
            let status = response
                .as_ref()
                .filter(|r| r.error.is_none())
                .and_then(|r| r.result.as_ref())
                .and_then(|v| v.get("status"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            match w {
                Waiting::Notice { workspace, key, .. } => {
                    let outcome = match status.as_deref() {
                        Some("recorded") => AckOutcome::Recorded,
                        Some("duplicate") => AckOutcome::Duplicate,
                        Some("rejected") => AckOutcome::Rejected {
                            reason: response
                                .as_ref()
                                .and_then(|r| r.result.as_ref())
                                .and_then(|v| v.get("reason"))
                                .and_then(Value::as_str)
                                // 사유는 plugin 이 준 진단 문구다. 없으면 비워 둔다.
                                .map(str::to_owned)
                                .unwrap_or_default(),
                        },
                        _ => AckOutcome::RetryLater,
                    };
                    acks.push((*workspace, key.clone(), outcome));
                }
                Waiting::Late {
                    call_id, plugin, ..
                } => match status.as_deref() {
                    Some("recorded") => commits.push((*call_id, plugin.clone())),
                    Some("rejected") => {
                        tracing::info!(
                            "late remote agent children of call {call_id} were rejected by {plugin}"
                        )
                    }
                    _ => late_failed.push(*call_id),
                },
            }
            false
        });
        let mut client = agents::client();
        for (ws, key, outcome) in acks {
            client.receiving.remove(&key);
            if outcome.settles()
                && let Some(session) = client.sessions.get_mut(&ws)
            {
                session.first_seen(&key);
            }
            client.outgoing.push((ws, Outgoing::Ack { key, outcome }));
        }
        for (call_id, plugin) in commits {
            if let Err(e) = client.commit(call_id, &plugin) {
                tracing::warn!("late remote agent children of call {call_id} not committed: {e}");
            }
        }
        for call_id in late_failed {
            if let Some(call) = client.calls.get_mut(&call_id) {
                call.late_sent = false;
                call.late_retry_at = Some(Instant::now() + LATE_RETRY);
            }
        }
    }
}

/// 원격 자식 mirror surface 가 사라졌다. 늦은 종료 통지를 위해 훅을 옮겨 두고 부모 아래 등록을 푼다.
/// 다른 mirror surface 의 정리는 바꾸지 않는다.
pub(super) fn forget_mirror_child(engine: &mut EngineMut<'_>, surface: u32) {
    if !agents::is_client_child(surface) {
        return;
    }
    engine.hooks.forget_surface(surface);
    if engine
        .runtime
        .child_terminals
        .unregister_child_by_surface(surface)
    {
        engine.runtime.child_terminals.save();
    }
}

/// 통지를 적용할 이 client 의 종점.
struct Sink<'a, 'e> {
    engine: Option<&'a mut EngineMut<'e>>,
    plugins: &'a mut Option<crate::plugin::PluginManager>,
    exec: &'a crate::hook_runtime::HookExecutor,
    meta_writes: &'a mut Vec<(u32, String, Option<String>)>,
}

/// 서버 메시지 하나를 적용한다.
fn apply(sess: &mut AttachClientSession, sink: Sink<'_, '_>, ws: u32, msg: AgentServerMsg) {
    match msg {
        AgentServerMsg::AgentHelloAck {
            origin_token,
            origin,
            resumed,
        } => agents::client().hello_ack(ws, origin_token, origin, resumed),
        AgentServerMsg::AgentCallResult {
            call_id,
            outcome,
            spawned,
        } => agents::client().call_result(call_id, outcome, spawned),
        AgentServerMsg::ChildNotice {
            key,
            seq,
            remote_surface_id,
            notice,
        } => {
            let outcome = notice_outcome(sink, ws, &key, seq, remote_surface_id, notice);
            let Some(outcome) = outcome else {
                // plugin 이 처리 중이다. 응답이 오면 확인한다.
                return;
            };
            let mut client = agents::client();
            if outcome.settles()
                && let Some(session) = client.sessions.get_mut(&ws)
            {
                session.first_seen(&key);
            }
            drop(client);
            send(sess, &AgentClientMsg::ChildNoticeAck { key, outcome });
        }
    }
}

/// 통지를 적용한 결과. None 은 plugin 응답을 기다린다는 뜻이다.
fn notice_outcome(
    sink: Sink<'_, '_>,
    ws: u32,
    key: &str,
    seq: u64,
    remote: u32,
    notice: Notice,
) -> Option<AckOutcome> {
    let Sink {
        engine,
        plugins,
        exec,
        meta_writes,
    } = sink;
    let reject = |reason: String| Some(AckOutcome::Rejected { reason });
    let (local, child) = {
        let mut client = agents::client();
        if client.receiving.contains(key) {
            return None;
        }
        if client
            .sessions
            .get_mut(&ws)
            .is_some_and(|s| s.has_seen(key))
        {
            return Some(AckOutcome::Duplicate);
        }
        let Some(local) = client.local_of(ws, remote) else {
            return reject(format!("remote surface {remote} is not a known child here"));
        };
        (local, client.children[&local].clone())
    };
    let Some(decl) = decl(plugins, &child.plugin) else {
        return Some(AckOutcome::RetryLater);
    };
    match notice {
        Notice::State { state } => {
            let mut client = agents::client();
            let session = client.sessions.entry(ws).or_default();
            if session
                .state_seq
                .get(&remote)
                .is_some_and(|last| *last > seq)
            {
                return Some(AckOutcome::Duplicate);
            }
            session.state_seq.insert(remote, seq);
            drop(client);
            let Some(engine) = engine else {
                return Some(AckOutcome::RetryLater);
            };
            let children = &mut engine.runtime.child_terminals;
            match state.as_str() {
                "idle" => children.set_idle(local, true),
                "active" => children.set_idle(local, false),
                "needs_input" => children.set_needs_input(local, true),
                other => return reject(format!("unknown state {other}")),
            }
            children.save();
            Some(AckOutcome::Recorded)
        }
        Notice::Event { name } => {
            if name != PROCESS_EXIT && !decl.hook_events.contains(&name) {
                return reject(format!("event {name} is not declared by {}", child.plugin));
            }
            let Some(event) = tasty_hooks::HookEvent::parse(&name) else {
                return reject(format!("event {name} is not a hook event"));
            };
            let Some(engine) = engine else {
                return Some(AckOutcome::RetryLater);
            };
            let fired = match &child.hooks {
                Some(hooks) => {
                    static POISONED: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    let mut manager =
                        crate::poison::recover_mutex(hooks.lock(), "remote child hooks", &POISONED);
                    crate::hook_runtime::HookRuntimeState::fire_detached(
                        exec,
                        &mut manager,
                        local,
                        event,
                        &name,
                    )
                }
                None => engine.hooks.fire_as(exec, local, event, name),
            };
            for event in fired {
                engine.enqueue_host_event(event);
            }
            Some(AckOutcome::Recorded)
        }
        Notice::Meta { key: meta, value } => {
            if !decl.relay_meta.contains(&meta) {
                return reject(format!("meta {meta} is not declared by {}", child.plugin));
            }
            meta_writes.push((local, meta, value));
            Some(AckOutcome::Recorded)
        }
        Notice::Plugin { namespace, payload } => {
            if namespace != child.namespace {
                return reject(format!(
                    "payload for {namespace} on a {} child",
                    child.namespace
                ));
            }
            let Some(receive) = decl.receive else {
                return reject(format!("{} declares no receive method", child.plugin));
            };
            let Some(manager) = plugins.as_mut() else {
                return Some(AckOutcome::RetryLater);
            };
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            manager.forward_namespace_call(
                &format!("{namespace}.{receive}"),
                json!({
                    "kind": "notice",
                    "surface_id": local,
                    "remote_surface_id": remote,
                    "key": key,
                    "payload": payload,
                }),
                None,
                Value::Null,
                tx,
                None,
            );
            agents::client().receiving.insert(key.to_owned());
            lock(&WAITING).push(Waiting::Notice {
                workspace: ws,
                key: key.to_owned(),
                rx,
            });
            None
        }
    }
}

/// 쌓인 요청을 세션 ID 로 바꿔 보낸다. 출처가 정해진 연결에만 보낸다.
fn flush_outgoing(sess: &AttachClientSession, ws: u32) {
    let mut client = agents::client();
    let ready = client
        .sessions
        .get(&ws)
        .is_some_and(|s| s.connected && s.origin.is_some() && sess.transport.agent_hello_sent);
    if !ready {
        return;
    }
    let local_to_remote: std::collections::HashMap<u32, u32> = sess
        .state
        .remote_to_local
        .iter()
        .map(|(r, l)| (*l, *r))
        .collect();
    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut client.outgoing)
        .into_iter()
        .partition(|(w, _)| *w == ws);
    client.outgoing = rest;
    for (_, out) in mine {
        let msg = match out {
            Outgoing::Call {
                call_id,
                namespace,
                method,
                anchor,
                target,
                params,
            } => {
                let Some(anchor) = local_to_remote.get(&anchor).copied() else {
                    if let Some(call) = client.calls.get_mut(&call_id) {
                        call.state =
                            CallState::Unknown("the anchor surface left the mirror".into());
                    }
                    continue;
                };
                AgentClientMsg::AgentCallRequest {
                    call_id,
                    namespace,
                    method,
                    anchor_surface_id: anchor,
                    target_surface_id: target,
                    params,
                    inherit_cwd: None,
                }
            }
            Outgoing::Commit { call_id, remote } => AgentClientMsg::AgentCallCommit {
                call_id,
                remote_surface_ids: remote,
            },
            Outgoing::Ack { key, outcome } => AgentClientMsg::ChildNoticeAck { key, outcome },
        };
        let wire = serde_json::to_value(&msg).ok();
        if let AgentClientMsg::AgentCallRequest { call_id, .. } = &msg
            && let Some(call) = client.calls.get_mut(call_id)
        {
            call.sent = true;
            call.wire = wire;
        }
        send(sess, &msg);
    }
    // 재연결 뒤 결과를 받지 못한 call 은 같은 call_id 로 다시 묻는다. 서버는 다시 실행하지 않는다.
    for call in client
        .calls
        .values_mut()
        .filter(|c| c.workspace == ws && !c.sent && c.state == CallState::Waiting)
    {
        let Some(wire) = &call.wire else {
            continue;
        };
        match serde_json::from_value::<AgentClientMsg>(wire.clone()) {
            Ok(msg) => {
                call.sent = send(sess, &msg);
            }
            Err(e) => tracing::warn!("attach agent call could not be resent: {e}"),
        }
    }
}

/// 서버 결과의 새 원격 자식이 mirror 에 나타나면 이 client 의 부모 아래 자식으로 등록한다.
fn adopt(sess: &AttachClientSession, engine: &mut EngineMut<'_>, ws: u32) {
    let mut client = agents::client();
    let ready: Vec<u64> = client
        .calls
        .iter()
        .filter(|(_, c)| c.workspace == ws && c.state == CallState::Adopting)
        // mirror 에 아직 없는 자식은 닫혔다는 근거가 없으므로 기다린다. 상한이 지나면 plugin 은 결과불명을 받고,
        // 나중에 나타나면 late_spawn 으로 재결합하며, 끝내 나타나지 않으면 기록 보관 기한에 정리된다.
        .filter(|(_, c)| {
            c.spawned
                .iter()
                .all(|r| sess.state.remote_to_local.contains_key(r))
        })
        .map(|(id, _)| *id)
        .collect();
    let in_progress: Vec<u64> = lock(&ADOPTING).iter().map(|a| a.call_id).collect();
    for call_id in ready.into_iter().filter(|id| !in_progress.contains(id)) {
        let call = &client.calls[&call_id];
        let Some(parent) = call.parent.clone() else {
            continue;
        };
        let mut pending = Vec::new();
        let mut failed = Vec::new();
        for remote in &call.spawned {
            let Some(&local) = sess.state.remote_to_local.get(remote) else {
                tracing::info!(
                    "remote child r{remote} of call {call_id} closed before it appeared here"
                );
                continue;
            };
            let entry = crate::runtime::child_terminal::ChildEntry {
                child_surface_id: local,
                index: 0,
                cwd: None,
                role: call.role.clone(),
                nickname: call.nickname.clone(),
            };
            match engine.runtime.child_terminals.adopt(parent.clone(), entry) {
                Ok(ticket) => pending.push((local, *remote, ticket)),
                Err(e) => {
                    tracing::warn!("remote child s{local} not registered: {e}");
                    failed.push((local, *remote));
                }
            }
        }
        if pending.is_empty() {
            client.adopted(call_id, Vec::new(), failed);
            continue;
        }
        lock(&ADOPTING).push(Adopting {
            call_id,
            pending,
            failed,
        });
    }
}

fn poll_adoptions() {
    let mut finished = Vec::new();
    lock(&ADOPTING).retain(|a| {
        // poll 은 결과를 꺼내므로 모두 끝났는지는 꺼내지 않는 observation 으로 본다.
        if a.pending.iter().any(|(_, _, t)| t.observation().is_none()) {
            return true;
        }
        let mut adopted = Vec::new();
        let mut failed = a.failed.clone();
        for (local, remote, t) in &a.pending {
            match t.poll() {
                Some(Ok(Some(index))) => adopted.push(Adopted {
                    local: *local,
                    remote: *remote,
                    index,
                }),
                other => {
                    tracing::warn!("remote child s{local} registration failed: {other:?}");
                    failed.push((*local, *remote));
                }
            }
        }
        finished.push((a.call_id, adopted, failed));
        false
    });
    let mut client = agents::client();
    for (call_id, adopted, failed) in finished {
        client.adopted(call_id, adopted, failed);
    }
}

/// plugin 에 실패·결과불명으로 답한 call 이 자식을 남겼으면 그 plugin 수신 메서드로 재결합을 맡긴다.
fn late_spawns(plugins: &mut Option<crate::plugin::PluginManager>, ws: u32, now: Instant) {
    let Some(manager) = plugins.as_mut() else {
        return;
    };
    let mut client = agents::client();
    for (call_id, call) in client.calls.iter_mut().filter(|(_, c)| {
        c.workspace == ws
            && c.late
            && !c.late_sent
            && !c.committed
            && c.state == CallState::Done
            && !(c.adopted.is_empty() && c.unregistered.is_empty())
            && c.late_retry_at.is_none_or(|at| now >= at)
    }) {
        let Some(receive) = manager
            .packages()
            .iter()
            .find(|p| p.manifest.id == call.plugin)
            .and_then(|p| p.manifest.contributes.remote_attach.receive.clone())
        else {
            tracing::warn!("late remote agent children of call {call_id} have no receiver");
            call.late_sent = true;
            continue;
        };
        let (result, error) = match &call.outcome {
            Some(CallOutcome::Ok { result }) => (result.clone(), Value::Null),
            Some(CallOutcome::Error(e)) => {
                (Value::Null, json!({"code": e.code, "message": e.message}))
            }
            None => (Value::Null, Value::Null),
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        manager.forward_namespace_call(
            &format!("{}.{receive}", call.namespace),
            json!({
                "kind": "late_spawn",
                "call_id": call_id,
                "children": call.adopted.iter().map(|a| json!({
                    "surface_id": a.local,
                    "remote_surface_id": a.remote,
                    "index": a.index,
                })).chain(call.unregistered.iter().map(|(local, remote)| json!({
                    "surface_id": local,
                    "remote_surface_id": remote,
                    "index": null,
                }))).collect::<Vec<_>>(),
                "result": result,
                "error": error,
            }),
            None,
            Value::Null,
            tx,
            None,
        );
        call.late_sent = true;
        lock(&WAITING).push(Waiting::Late {
            call_id: *call_id,
            plugin: call.plugin.clone(),
            rx,
        });
    }
}
