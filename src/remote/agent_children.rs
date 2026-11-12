//! attach holder 가 서버 plugin 으로 자식 에이전트를 만들고 관리하며, 그 자식의 상태·훅·결과를 holder 연결로
//! 돌려보내는 서버 쪽 처리. 상태는 [`state`], 확인 대기 통지는 [`outbox`] 에 있다.
//!
//! host 는 plugin 메서드의 의미를 해석하지 않는다. 실행할 수 있는 메서드는 plugin 매니페스트의
//! `contributes.remote_attach` 가 정하고, 통지는 host 가 이미 아는 효과(child 상태·surface 훅 이벤트·메타)와
//! plugin 이 넘긴 불투명 본문뿐이다. 별도 포트·주소 없이 기존 attach 연결의 control 경로로 오간다.

#[cfg(feature = "gui")]
pub(crate) mod client;
mod outbox;
mod state;

#[cfg(feature = "gui")]
pub(crate) use client::{
    call_reply, client, client_receive, client_summary, client_surface_closing, is_client_child,
    remote_child_connected,
};

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Instant;

use serde_json::{Value, json};
use tasty_ipc::agent_child::{
    AgentClientMsg, AgentServerMsg, CallError, CallOutcome, Notice, PARENT_PREFIX,
};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_ipc::stream::{StreamFrame, StreamTag};
use tasty_ipc::stream_hub::{StreamClientId, StreamHub};

use crate::core::child_parent::ParentKey;
use crate::runtime::engine_access::EngineRef;

pub(crate) use state::PluginNoticeStatus;
use state::{Begin, Decl, Server, error};

static SERVER: LazyLock<Mutex<Server>> = LazyLock::new(|| Mutex::new(Server::new()));

/// plugin 응답을 기다리는 call. 메인 루프가 매 tick 확인하며 기다리느라 멈추지 않는다.
struct Waiting {
    origin: u64,
    call_id: u64,
    rx: Receiver<JsonRpcResponse>,
}
static WAITING: Mutex<Vec<Waiting>> = Mutex::new(Vec::new());

fn server() -> MutexGuard<'static, Server> {
    SERVER.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("attach agent child state was poisoned; keeping its entries");
        poisoned.into_inner()
    })
}

fn waiting() -> MutexGuard<'static, Vec<Waiting>> {
    WAITING.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("attach agent call wait list was poisoned; keeping its entries");
        poisoned.into_inner()
    })
}

/// 서버 child registry 의 부모 키에서 원격 출처 번호를 읽는다.
pub(crate) fn origin_of_parent(key: &str) -> Option<u64> {
    key.strip_prefix(PARENT_PREFIX)?.parse().ok()
}

/// 원격 출처 자식이면 그 출처 번호.
pub(crate) fn origin_of_child(
    children: &crate::runtime::child_terminal::ChildTerminalRegistry,
    surface: u32,
) -> Option<u64> {
    match children.parent_of_child(surface)? {
        ParentKey::External(key) => origin_of_parent(&key),
        ParentKey::Surface(_) => None,
    }
}

/// 출처 부모 아래 자식의 번호 → surface.
fn origin_children(engine: &EngineRef<'_>, origin: u64) -> Vec<(u32, u32)> {
    engine
        .runtime
        .child_terminals
        .list_children(&ParentKey::External(Server::parent_key(origin)))
        .iter()
        .map(|c| (c.index, c.child_surface_id))
        .collect()
}

/// 원격 holder 문맥의 spawn 이 출처 부모 아래 번호를 예약했다. 그 문맥의 call 에 귀속한다.
pub(crate) fn record_spawn_slot(context: &str, index: u32) {
    server().record_slot(context, index);
}

fn send(hub: &StreamHub, client: StreamClientId, msg: &AgentServerMsg) {
    match serde_json::to_vec(msg) {
        Ok(payload) => {
            // 손실·끊김은 허브가 처리한다. 손실 통지로 client 가 다시 붙으면 확인받지 못한 통지는 outbox 가,
            // 받지 못한 call 결과는 같은 call_id 재조회가 다시 보낸다.
            let _ = hub.push(client, StreamFrame::new(StreamTag::Control, payload));
        }
        Err(e) => tracing::warn!("attach agent message did not serialize: {e}"),
    }
}

// ── 입력·종료 범위 ───────────────────────────────────────────────

thread_local! {
    static DRIVING_PLUGIN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// plugin 호출을 처리하는 동안 그 plugin 을 기억한다. 원격 출처 자식의 hard 점유 입력 예외는 이 범위에서만 판단한다.
pub(crate) fn driving_scope<R>(
    caller: &crate::ipc::caller::CallerContext,
    f: impl FnOnce() -> R,
) -> R {
    let plugin = match caller {
        crate::ipc::caller::CallerContext::Plugin { plugin_id, .. } => Some(plugin_id.clone()),
        _ => None,
    };
    let previous = DRIVING_PLUGIN.with(|p| p.replace(plugin));
    let result = f();
    DRIVING_PLUGIN.with(|p| *p.borrow_mut() = previous);
    result
}

pub(crate) fn driving_plugin() -> Option<String> {
    DRIVING_PLUGIN.with(|p| p.borrow().clone())
}

/// 지금 처리 중인 plugin 이 이 원격 출처 자식에 holder 대신 쓸 수 있는지. 출처가 그 workspace 의
/// 현재 holder 연결에 묶여 있고, 자식을 만든 plugin 이어야 한다. 서버 로컬 사용자·다른 plugin 은 해당하지 않는다.
pub(crate) fn may_drive(
    occupancy: &crate::core::attach::OccupancyRegistry,
    children: &crate::runtime::child_terminal::ChildTerminalRegistry,
    surface: u32,
) -> bool {
    let Some(plugin) = DRIVING_PLUGIN.with(|p| p.borrow().clone()) else {
        return false;
    };
    may_drive_as(occupancy, children, surface, &plugin)
}

pub(crate) fn may_drive_as(
    occupancy: &crate::core::attach::OccupancyRegistry,
    children: &crate::runtime::child_terminal::ChildTerminalRegistry,
    surface: u32,
    plugin: &str,
) -> bool {
    let Some(origin) = origin_of_child(children, surface) else {
        return false;
    };
    let Some(holder) = occupancy
        .workspace_of_surface(surface)
        .and_then(|ws| occupancy.workspace_holder(ws))
    else {
        return false;
    };
    server().plugin_may_drive(origin, surface, plugin, holder)
}

/// 원격 출처 부모로 자식을 만드는 spawn 을 확인한다. 실행 문맥이 이 plugin·출처·workspace 의 것이고 그
/// workspace 의 holder 가 아직 문맥의 연결이어야 한다. 통과하면 hard 점유 거절만 건너뛴다.
pub(crate) fn spawn_allowed(
    occupancy: &crate::core::attach::OccupancyRegistry,
    plugin: Option<&str>,
    parent: &str,
    context: Option<&str>,
    workspace: u32,
) -> Result<(), String> {
    let refuse = |why: &str| {
        Err(format!(
            "parent '{parent}' is reserved for remote attach children: {why}"
        ))
    };
    let Some(origin) = origin_of_parent(parent) else {
        return refuse("malformed key");
    };
    let (Some(plugin), Some(context)) = (plugin, context) else {
        return refuse("only the plugin running a holder's call may use it");
    };
    let server = server();
    let Some(ctx) = server.context(context) else {
        return refuse("the call context expired");
    };
    if ctx.origin != origin || ctx.plugin != plugin || ctx.workspace != workspace {
        return refuse("the call context belongs to another plugin, origin or workspace");
    }
    if occupancy.workspace_holder(workspace) != Some(ctx.client) {
        return refuse("the workspace is no longer held by that connection");
    }
    Ok(())
}

// ── 통지 ───────────────────────────────────────────────────────

/// 원격 출처 자식에 생긴 일을 쌓는다. 원격 출처 자식이 아니면 아무것도 하지 않는다.
pub(crate) fn relay(engine: &EngineRef<'_>, surface: u32, notice: Notice) {
    let Some(origin) = origin_of_child(&engine.runtime.child_terminals, surface) else {
        return;
    };
    let workspace = engine
        .live
        .occupancy
        .workspace_of_surface(surface)
        .or_else(|| {
            engine
                .core
                .find_workspace_index_for_surface(surface)
                .and_then(|(index, _)| engine.core.workspace_at(index))
                .map(|ws| ws.id)
        });
    let Some(workspace) = workspace else {
        tracing::warn!("attach agent notice for s{surface} dropped: no workspace");
        return;
    };
    let mut server = server();
    // 자식을 만든 plugin 이 선언하지 않은 이벤트·메타는 client 의 훅·복원 명령에 닿지 않게 옮기지 않는다.
    if !server.notice_allowed(origin, surface, &notice) {
        tracing::debug!(
            "attach agent notice for s{surface} not relayed: {notice:?} is not declared"
        );
        return;
    }
    server.relay(origin, workspace, surface, notice, Instant::now());
}

/// `attach.child_notice`: plugin 이 원격 출처 자식의 고유 결과를 보낸다. client 가 확인할 때까지 pending 이다.
pub(crate) fn plugin_notice(
    caller: &crate::ipc::caller::CallerContext,
    params: &Value,
) -> Result<PluginNoticeStatus, String> {
    let crate::ipc::caller::CallerContext::Plugin { plugin_id, .. } = caller else {
        return Err("attach.child_notice is only for plugins".into());
    };
    let surface = params
        .get("surface_id")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("missing 'surface_id'")?;
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty() && k.len() <= 256)
        .ok_or("missing 'key'")?;
    let payload = params.get("payload").cloned().ok_or("missing 'payload'")?;
    // 받을 수 없는 대상은 오류가 아니라 rejected 로 답한다. 오류는 plugin 이 일시 실패로 보고 계속 다시 보낸다.
    // 출처·workspace 는 이 서버 프로세스의 자식 기록에서 정하므로, 자식이 닫힌 뒤에도 보존 기간 안이면 받는다.
    Ok(server().plugin_notice(plugin_id, key, surface, payload, Instant::now()))
}

// ── 연결 메시지 ─────────────────────────────────────────────────

/// 출처를 지정하지 않는 파라미터. 공개 IPC 에서처럼 호출자가 부모·위치를 정하지 못하게 한다.
const RESERVED_PARAMS: &[&str] = &[
    "parent",
    "surface",
    "surface_id",
    "parent_external",
    "workspace",
    "pane",
    "caller_surface",
    "attach_context",
    "attach_target_surface",
    tasty_ipc::agent_child::INHERIT_CWD_PARAM,
];

fn sanitized(params: Value) -> serde_json::Map<String, Value> {
    let mut map = match params {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    map.retain(|k, _| !k.starts_with("_tasty") && !RESERVED_PARAMS.contains(&k.as_str()));
    map
}

/// holder 연결에서 온 메시지를 처리한다. plugin 호출은 응답을 기다리지 않고 [`tick`] 이 거둔다.
pub(crate) fn handle(
    hub: &StreamHub,
    plugins: Option<&mut crate::plugin::PluginManager>,
    engines: &[EngineRef<'_>],
    messages: Vec<(StreamClientId, AgentClientMsg)>,
) {
    let mut plugins = plugins;
    let now = Instant::now();
    for (client, msg) in messages {
        match msg {
            AgentClientMsg::AgentHello { origin_token } => {
                let ack = server().hello(client, origin_token.as_deref(), now);
                send(hub, client, &ack);
            }
            AgentClientMsg::AgentCallRequest {
                call_id,
                namespace,
                method,
                anchor_surface_id,
                target_surface_id,
                params,
                inherit_cwd,
            } => {
                let started = start_call(
                    plugins.as_deref_mut(),
                    engines,
                    client,
                    Target {
                        call_id,
                        namespace: &namespace,
                        method: &method,
                        anchor: anchor_surface_id,
                        target: target_surface_id,
                        inherit_cwd,
                    },
                    params,
                    now,
                );
                if let Some(reply) = started {
                    send(hub, client, &reply);
                }
            }
            AgentClientMsg::AgentCallCommit {
                call_id,
                remote_surface_ids,
            } => {
                let opened = server().commit(client, call_id, &remote_surface_ids);
                if opened != remote_surface_ids.len() {
                    tracing::warn!(
                        "attach agent commit for call {call_id} opened {opened} of {} children",
                        remote_surface_ids.len()
                    );
                }
            }
            AgentClientMsg::ChildNoticeAck { key, outcome } => {
                server().ack(client, &key, &outcome, now);
            }
        }
    }
}

fn call_error(call_id: u64, error: CallError) -> AgentServerMsg {
    AgentServerMsg::AgentCallResult {
        call_id,
        outcome: CallOutcome::Error(error),
        spawned: Vec::new(),
    }
}

struct Target<'a> {
    call_id: u64,
    namespace: &'a str,
    method: &'a str,
    anchor: u32,
    target: Option<u32>,
    /// client 가 실은 새 자식 cwd 상속 원본. 확인한 뒤 host 예약 인자로 넘긴다.
    inherit_cwd: Option<tasty_ipc::stream::InheritCwd>,
}

/// holder 출처 대신 실행하는 데 필요한 기존 권한. 새 승인 흐름 없이 매니페스트 선언·사용자 허가를 그대로 쓴다.
const DRIVE_PERMISSIONS: &[&str] = &["terminal.spawn", "terminal.write"];

/// holder·대상·매니페스트 선언을 확인하고 plugin 에 넘긴다. 바로 답할 것이 있으면 돌려준다.
fn start_call(
    plugins: Option<&mut crate::plugin::PluginManager>,
    engines: &[EngineRef<'_>],
    client: StreamClientId,
    call: Target<'_>,
    params: Value,
    now: Instant,
) -> Option<AgentServerMsg> {
    let Target {
        call_id,
        namespace,
        method,
        anchor,
        target,
        inherit_cwd,
    } = call;
    let reject = |code, message: String| Some(call_error(call_id, error(code, message)));
    let Some(engine) = engines.iter().find(|e| {
        e.live
            .occupancy
            .workspace_of_surface(anchor)
            .is_some_and(|ws| e.live.occupancy.workspace_holder(ws) == Some(client))
    }) else {
        return reject(
            -32602,
            format!("surface {anchor} is not in a workspace this connection holds"),
        );
    };
    let occupancy = &engine.live.occupancy;
    let workspace = occupancy.workspace_of_surface(anchor)?;
    if !occupancy.workspace_attachment_ready(workspace) {
        return reject(-32003, format!("workspace {workspace} is not ready yet"));
    }
    let Some(pane) = engine.core.find_pane_for_surface(anchor) else {
        return reject(-32602, format!("surface {anchor} has no pane"));
    };
    if let Err(reason) = check_inherit_cwd(engine, inherit_cwd.as_ref(), workspace, pane) {
        return reject(-32602, reason);
    }
    let Some(plugins) = plugins else {
        return reject(
            -32003,
            "plugins are not running on the remote instance".into(),
        );
    };
    let full = format!("{namespace}.{method}");
    let Some((plugin_id, decl)) = declared(plugins, namespace, method) else {
        return reject(
            -32601,
            format!("{full} is not declared for remote attach on the remote instance"),
        );
    };
    let origin = server().origin_of_client(client);
    if let Some(target) = target {
        let recorded = origin.is_some_and(|o| server().is_child_of(o, target));
        let owned = recorded
            && occupancy.workspace_of_surface(target) == Some(workspace)
            && may_drive_as(
                occupancy,
                &engine.runtime.child_terminals,
                target,
                &plugin_id,
            );
        if !owned {
            return reject(
                -32602,
                format!("surface {target} is not a child this connection created with {namespace}"),
            );
        }
    }
    let mut state = server();
    state.set_decl(&plugin_id, decl);
    let (origin, context) = match state.begin_call(client, call_id, &plugin_id, workspace, now) {
        Begin::Cached(result) => return Some(result),
        Begin::Running => return None,
        Begin::Rejected(e) => return Some(call_error(call_id, e)),
        Begin::Start { origin, context } => (origin, context),
    };
    drop(state);
    let mut params = sanitized(params);
    params.insert("workspace".into(), json!(workspace.to_string()));
    params.insert("pane".into(), json!(pane));
    params.insert("parent_external".into(), json!(Server::parent_key(origin)));
    params.insert("attach_context".into(), json!(context));
    if let Some(target) = target {
        params.insert("attach_target_surface".into(), json!(target));
    }
    if let Some(inherit_cwd) = &inherit_cwd {
        params.insert(
            tasty_ipc::agent_child::INHERIT_CWD_PARAM.into(),
            json!(inherit_cwd),
        );
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    plugins.forward_attach_call(&full, Value::Object(params), None, Value::Null, tx, None);
    waiting().push(Waiting {
        origin,
        call_id,
        rx,
    });
    None
}

/// client 가 실은 cwd 상속 원본은 이 연결이 점유한 workspace 의, anchor 와 같은 pane 의 surface 여야 한다.
/// 모르는 값도 거절한다. 원본의 kind 나 cwd 를 모르는 것은 거절 사유가 아니다.
fn check_inherit_cwd(
    engine: &EngineRef<'_>,
    value: Option<&tasty_ipc::stream::InheritCwd>,
    workspace: u32,
    pane: u32,
) -> Result<(), String> {
    use tasty_ipc::stream::InheritCwd;
    match value {
        None | Some(InheritCwd::NoInherit) => Ok(()),
        Some(unknown @ InheritCwd::Unknown(_)) => Err(format!(
            "unsupported inherit_cwd value {}",
            unknown.wire_text()
        )),
        Some(InheritCwd::Surface(source)) => {
            if engine.live.occupancy.workspace_of_surface(*source) != Some(workspace) {
                return Err(format!(
                    "inherit_cwd surface {source} is not in a workspace this connection holds"
                ));
            }
            if engine.core.find_pane_for_surface(*source) != Some(pane) {
                return Err(format!(
                    "inherit_cwd surface {source} is not in the pane of the anchor surface"
                ));
            }
            Ok(())
        }
    }
}

/// namespace 소유 plugin 이 이 메서드를 원격 실행용으로 선언했고, 켜져 있으며 필요한 기존 권한을 허가받았는지.
fn declared(
    plugins: &crate::plugin::PluginManager,
    namespace: &str,
    method: &str,
) -> Option<(String, Decl)> {
    let owner = plugins.namespace_owners().get(namespace)?;
    if !owner.enabled
        || !DRIVE_PERMISSIONS
            .iter()
            .all(|p| owner.permissions.contains(*p))
    {
        return None;
    }
    let manifest = &plugins
        .packages()
        .iter()
        .find(|p| p.manifest.id == owner.plugin_id)?
        .manifest;
    let decl = &manifest.contributes.remote_attach;
    if !decl.allows(method) {
        return None;
    }
    Some((
        owner.plugin_id.clone(),
        Decl {
            namespace: namespace.to_owned(),
            hook_events: manifest
                .contributes
                .hook_events
                .iter()
                .map(|h| h.key.clone())
                .collect(),
            relay_meta: decl.relay_meta.clone(),
        },
    ))
}

/// 끝난 plugin 호출의 결과와 보낼 차례인 통지를 보내고 오래된 기록을 정리한다. 메인 루프의 1초 tick 에서 부른다.
pub(crate) fn tick(hub: &StreamHub, engines: &[EngineRef<'_>]) {
    let now = Instant::now();
    let mut finished = Vec::new();
    waiting().retain(|w| match w.rx.try_recv() {
        Ok(response) => {
            let result = match response.error {
                Some(e) => Err(error(e.code, e.message)),
                None => Ok(response.result.unwrap_or(Value::Null)),
            };
            finished.push((w.origin, w.call_id, result));
            false
        }
        Err(TryRecvError::Empty) => true,
        Err(TryRecvError::Disconnected) => {
            finished.push((
                w.origin,
                w.call_id,
                Err(error(-32603, "the remote plugin did not answer")),
            ));
            false
        }
    });
    for (origin, call_id, result) in finished {
        let children: BTreeMap<u32, u32> = engines
            .iter()
            .flat_map(|e| origin_children(e, origin))
            .collect();
        let mut server = server();
        let Some(msg) = server.finish_call(origin, call_id, &children, result) else {
            continue;
        };
        // 끊긴 사이에 끝났으면 다시 연결한 client 가 같은 call_id 로 물을 때 보낸다.
        if let Some(client) = server.client_of_origin(origin) {
            send(hub, client, &msg);
        }
    }
    let holder_of = |workspace: u32| {
        engines
            .iter()
            .find_map(|e| e.live.occupancy.workspace_holder(workspace))
    };
    let due = server().due(now, holder_of);
    for (client, msg) in due {
        send(hub, client, &msg);
    }
    let live = |surface: u32| {
        engines
            .iter()
            .any(|e| e.runtime.terminals.get(surface).is_some())
    };
    server().expire(now, live);
}

/// 연결이 끊겼다. 출처 묶음과 진행 중 문맥을 지운다.
pub(crate) fn disconnect(client: StreamClientId) {
    server().disconnect(client, Instant::now());
}

/// `attach.child_notices`: 확인을 기다리는 통지 요약.
pub(crate) fn pending_summary() -> Value {
    json!({ "notices": server().outbox.summary(Instant::now()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 다시 보내도 바뀌지 않는 거절은 오류가 아니라 rejected 다. 오류면 plugin 이 끝없이 다시 보낸다.
    #[test]
    fn a_notice_that_can_never_be_accepted_is_rejected_not_an_error() {
        let caller = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: "com.example.never".into(),
            permissions: std::sync::Arc::new(std::collections::HashSet::new()),
        };
        // 이 서버 프로세스에서 이 plugin 이 만든 자식 기록이 없다(로컬 surface, 없는 surface, 서버 재시작 뒤).
        for surface in [1, 9_100_099] {
            let r = plugin_notice(
                &caller,
                &json!({ "surface_id": surface, "key": "k1", "payload": {} }),
            );
            assert!(matches!(r, Ok(PluginNoticeStatus::Rejected(_))), "{r:?}");
        }
        // 형식이 틀린 요청은 호출자 결함이라 오류다.
        assert!(plugin_notice(&caller, &json!({ "surface_id": 1 })).is_err());
    }

    #[test]
    fn reserved_params_never_reach_the_plugin() {
        let map = sanitized(json!({
            "prompt": "hi",
            "parent": 3,
            "parent_external": "attach:o9",
            "_tasty_context": {"provider": "codex"},
            "attach_context": "x",
            "workspace": "1",
            "inherit_cwd": {"surface": 4},
        }));
        assert_eq!(Value::Object(map), json!({"prompt": "hi"}));
    }

    #[test]
    fn only_the_attach_prefix_with_a_number_is_an_origin() {
        assert_eq!(origin_of_parent("attach:o12"), Some(12));
        assert_eq!(origin_of_parent("attach:x"), None);
        assert_eq!(origin_of_parent("codex:abc"), None);
    }

    /// client 가 실은 cwd 원본은 이 연결이 점유한 workspace 의, anchor 와 같은 pane 에 있어야 한다.
    #[cfg(feature = "gui")]
    #[test]
    fn the_client_cwd_source_must_be_in_the_anchor_pane() {
        use tasty_ipc::stream::InheritCwd;
        let (_, mut session) = crate::state::tests::two_pane_fixture();
        let workspace = session.core_state.workspace_at(0).expect("workspace").id;
        let surfaces = session
            .core_state
            .workspace_at(0)
            .unwrap()
            .all_surface_ids();
        session
            .live
            .occupancy
            .acquire_workspace(workspace, &surfaces, &surfaces, 7)
            .unwrap();
        let engine = session.borrow_mut();
        let engine = engine.as_ref();
        let check =
            |value: Option<InheritCwd>| check_inherit_cwd(&engine, value.as_ref(), workspace, 10);
        for ok in [
            None,
            Some(InheritCwd::NoInherit),
            Some(InheritCwd::Surface(101)),
            Some(InheritCwd::Surface(102)),
        ] {
            assert_eq!(check(ok.clone()), Ok(()), "{ok:?}");
        }
        assert!(
            check(Some(InheritCwd::Surface(201)))
                .unwrap_err()
                .contains("not in the pane")
        );
        assert!(
            check(Some(InheritCwd::Surface(9_999)))
                .unwrap_err()
                .contains("not in a workspace this connection holds")
        );
        assert!(
            check(Some(InheritCwd::Unknown(json!(3))))
                .unwrap_err()
                .contains("unsupported inherit_cwd value 3")
        );
    }
}
