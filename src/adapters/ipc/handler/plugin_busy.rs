//! plugin을 내리기 전에 바쁜지 묻는다.
//!
//! `plugin.disable`·`plugin.remove`·`plugin.upgrade_builtins`(`restart_running`)는 plugin 프로세스를 내린다.
//! 그 plugin이 일하는 중이면(대화 탭의 에이전트 턴 등) 그 일의 답을 받을 프로세스가 사라진다. 그래서 내리기
//! 전에 떠 있는 대상 plugin에 관례 메서드 `<namespace prefix>.lifecycle_busy`를 묻는다.
//!
//! - 답이 `{"busy": true, "reason": …}`이면 내리지 않고 [`ERR_PLUGIN_BUSY`]로 거절한다.
//! - 그 밖의 답은 바쁘지 않음이다 — 모르는 메서드·오류·[`PROBE_TIMEOUT`] 안에 답 없음·namespace 없음.
//!   바쁨을 낼 줄 모르는 plugin이 정지를 막지 않게 한다.
//! - `ignore_busy: true`면 묻지 않는다.
//!
//! 질문은 IPC 큐를 거치지 않고 plugin에 바로 보내며, 답을 기다리는 동안 요청은 요청 게이트 줄에 머문다
//! ([`crate::ipc::handler::gate::GateProgress`]). 그래서 요청 순서와 원 호출자가 그대로이고 App 스레드는
//! 막히지 않는다. 질문은 권한 게이트 앞에서 시작하지만 거절은 권한 게이트를 통과한 뒤에만 낸다 —
//! 권한이 없는 호출자는 바쁨이 아니라 권한 거절을 받는다.

use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

use crate::plugin::PluginManager;

/// 내리려는 plugin이 일하는 중이라 아무것도 바꾸지 않았다.
pub(crate) const ERR_PLUGIN_BUSY: i32 = -32023;

/// plugin이 답하는 관례 메서드의 꼬리 — `<prefix>.lifecycle_busy`.
pub(crate) const BUSY_METHOD: &str = "lifecycle_busy";

/// 질문 하나를 기다리는 상한. 굳은 plugin이 정지와 뒤 요청을 오래 붙잡지 않게 짧게 둔다.
/// 기한이 지난 것은 다음 루프 회차에 보므로 실제 대기는 루프를 깨우는 주기만큼 더 길 수 있다.
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// 요청이 내리려는 plugin — 떠 있는지는 아직 안 본다.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Wanted {
    Nothing,
    One(String),
    /// `upgrade_builtins`(`restart_running`) — 설치된 전부.
    Every,
}

pub(crate) fn wanted(method: &str, params: &Value) -> Wanted {
    if params.get("ignore_busy").and_then(Value::as_bool) == Some(true) {
        return Wanted::Nothing;
    }
    match method {
        "plugin.disable" | "plugin.remove" => params
            .get("id")
            .and_then(Value::as_str)
            .map_or(Wanted::Nothing, |id| Wanted::One(id.to_string())),
        "plugin.upgrade_builtins"
            if params.get("restart_running").and_then(Value::as_bool) == Some(true) =>
        {
            Wanted::Every
        }
        _ => Wanted::Nothing,
    }
}

/// 이 요청이 내리는 떠 있는 plugin들 — `(plugin id, namespace prefix)`. 물을 것이 없으면 빈 목록.
pub(crate) fn stop_targets(mgr: &PluginManager, wanted: Wanted) -> Vec<(String, String)> {
    let ids: Vec<String> = match wanted {
        Wanted::Nothing => return Vec::new(),
        Wanted::One(id) => vec![id],
        Wanted::Every => mgr
            .packages()
            .iter()
            .map(|p| p.manifest.id.clone())
            .collect(),
    };
    ids.into_iter()
        .filter(|id| mgr.is_running(id))
        .filter_map(|id| {
            let prefix = mgr
                .packages()
                .iter()
                .find(|p| p.manifest.id == id)?
                .manifest
                .contributes
                .ipc_namespace
                .first()?
                .prefix
                .clone();
            Some((id, prefix))
        })
        .collect()
}

/// 답 하나를 읽는다 — 바쁘면 그 사유.
pub(crate) fn busy_reason(answer: &JsonRpcResponse) -> Option<String> {
    if answer.error.is_some() {
        return None;
    }
    let v = answer.result.as_ref()?;
    (v.get("busy").and_then(Value::as_bool) == Some(true)).then(|| {
        v.get("reason")
            .and_then(Value::as_str)
            .unwrap_or("busy")
            .to_string()
    })
}

/// 바쁜 plugin들로 거절하는 응답.
pub(crate) fn busy_refusal(id: Value, busy: &[(String, String)]) -> JsonRpcResponse {
    let list = busy
        .iter()
        .map(|(pid, why)| format!("{pid}: {why}"))
        .collect::<Vec<_>>()
        .join("; ");
    JsonRpcResponse::error_with_data(
        id,
        ERR_PLUGIN_BUSY,
        format!(
            "plugin is busy ({list}); nothing was changed. Wait until it finishes, or pass ignore_busy (CLI --ignore-busy) to stop it anyway"
        ),
        json!({
            "busy": busy
                .iter()
                .map(|(pid, why)| json!({ "plugin_id": pid, "reason": why }))
                .collect::<Vec<_>>(),
        }),
    )
}

/// 진행 중인 질문들.
pub(crate) struct BusyProbe {
    asks: Vec<(String, Receiver<JsonRpcResponse>)>,
    deadline: Instant,
    busy: Vec<(String, String)>,
}

impl BusyProbe {
    #[cfg(test)]
    pub(crate) fn for_test(asks: Vec<(String, Receiver<JsonRpcResponse>)>) -> Self {
        Self {
            asks,
            ..Self::new(Instant::now() + Duration::from_secs(60))
        }
    }

    fn new(deadline: Instant) -> Self {
        Self {
            asks: Vec::new(),
            deadline,
            busy: Vec::new(),
        }
    }

    /// 답을 거둔다. 모두 답했거나 기한이 지났으면 true다. `wait`면 기한까지 기다린다.
    pub(crate) fn poll(&mut self, wait: bool) -> bool {
        loop {
            let busy = &mut self.busy;
            self.asks.retain(|(pid, rx)| match rx.try_recv() {
                Ok(answer) => {
                    if let Some(why) = busy_reason(&answer) {
                        busy.push((pid.clone(), why));
                    }
                    false
                }
                Err(TryRecvError::Empty) => true,
                // 답 없이 끊겼다(plugin이 내려갔다) — 바쁘지 않음이다.
                Err(TryRecvError::Disconnected) => false,
            });
            let now = Instant::now();
            if self.asks.is_empty() || now >= self.deadline {
                return true;
            }
            if !wait {
                return false;
            }
            std::thread::sleep((self.deadline - now).min(Duration::from_millis(10)));
        }
    }

    /// 바쁘다고 답한 plugin들 — 답을 다 거둔 뒤에 읽는다.
    pub(crate) fn busy(&self) -> &[(String, String)] {
        &self.busy
    }
}

/// 정지 요청이면 대상 plugin들에 묻기 시작한다. 물을 것이 없으면 `None`.
pub(crate) fn begin(
    mgr: Option<&mut PluginManager>,
    request: &JsonRpcRequest,
) -> Option<BusyProbe> {
    let mgr = mgr?;
    let targets = stop_targets(mgr, wanted(&request.method, &request.params));
    if targets.is_empty() {
        return None;
    }
    let mut probe = BusyProbe::new(Instant::now() + PROBE_TIMEOUT);
    for (pid, prefix) in targets {
        let (tx, rx) = sync_channel(1);
        mgr.forward_namespace_call(
            &format!("{prefix}.{BUSY_METHOD}"),
            json!({}),
            None,
            json!(0),
            tx,
            None,
        );
        probe.asks.push((pid, rx));
    }
    tracing::debug!(
        "{}: asking {} plugin(s) whether they are busy",
        request.method,
        probe.asks.len()
    );
    Some(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(v: Value) -> JsonRpcResponse {
        JsonRpcResponse::success(json!(0), v)
    }

    #[test]
    fn only_a_busy_true_answer_blocks() {
        assert_eq!(
            busy_reason(&ok(json!({"busy": true, "reason": "turn"}))),
            Some("turn".into())
        );
        assert_eq!(busy_reason(&ok(json!({"busy": true}))), Some("busy".into()));
        assert_eq!(busy_reason(&ok(json!({"busy": false}))), None);
        assert_eq!(busy_reason(&ok(json!(null))), None);
        // 모르는 메서드·오류는 바쁘지 않음이다 — 바쁨을 낼 줄 모르는 plugin이 정지를 막지 않는다.
        let unknown = JsonRpcResponse::error(json!(0), -32601, "unknown method");
        assert_eq!(busy_reason(&unknown), None);
    }

    #[test]
    fn the_refusal_names_the_plugin_and_the_way_out() {
        let r = busy_refusal(json!(1), &[("com.x".into(), "a turn runs".into())]);
        let e = r.error.expect("error");
        assert_eq!(e.code, ERR_PLUGIN_BUSY);
        assert!(e.message.contains("com.x: a turn runs") && e.message.contains("ignore_busy"));
        let data = e.data.expect("data");
        assert_eq!(data["busy"][0]["plugin_id"], "com.x");
    }

    // 답이 오면 그 답으로, 끊기면 바쁘지 않음으로, 답이 없으면 기한에 끝난다. 기한 전에는 끝나지 않는다.
    #[test]
    fn the_probe_settles_on_answers_hangups_and_the_deadline() {
        let mut p = BusyProbe::new(Instant::now() + Duration::from_secs(60));
        let (busy_tx, busy_rx) = sync_channel(1);
        let (gone_tx, gone_rx) = sync_channel::<JsonRpcResponse>(1);
        let (_slow_tx, slow_rx) = sync_channel::<JsonRpcResponse>(1);
        p.asks.push(("com.busy".into(), busy_rx));
        p.asks.push(("com.gone".into(), gone_rx));
        p.asks.push(("com.slow".into(), slow_rx));
        busy_tx
            .send(ok(json!({"busy": true, "reason": "turn"})))
            .unwrap();
        drop(gone_tx);
        assert!(!p.poll(false), "답하지 않은 plugin이 남았는데 끝났다");
        assert_eq!(p.busy(), &[("com.busy".to_string(), "turn".to_string())]);
        p.deadline = Instant::now();
        assert!(p.poll(false), "기한이 지났는데 끝나지 않았다");
        assert_eq!(p.busy().len(), 1, "답 없는 plugin을 바쁘다고 셌다");
    }

    #[test]
    fn ignore_busy_and_other_methods_skip_the_question() {
        let p = json!({"id": "x", "ignore_busy": true});
        assert_eq!(wanted("plugin.disable", &p), Wanted::Nothing);
        assert_eq!(
            wanted("plugin.enable", &json!({"id": "x"})),
            Wanted::Nothing
        );
        assert_eq!(
            wanted(
                "plugin.upgrade_builtins",
                &json!({"restart_running": false})
            ),
            Wanted::Nothing
        );
        assert_eq!(
            wanted("plugin.remove", &json!({"id": "x"})),
            Wanted::One("x".into())
        );
        assert_eq!(
            wanted("plugin.upgrade_builtins", &json!({"restart_running": true})),
            Wanted::Every
        );
    }

    // 게이트 단계: 정지 요청이 아니면 묻지 않고, 바쁘다는 답은 거절 응답 하나가 된다. 거절은 한 번만 낸다.
    #[test]
    fn the_gate_stage_refuses_once_on_a_busy_answer() {
        use crate::ipc::handler::gate::{GateProgress, Step};
        let req = |method: &str| JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            method: method.into(),
            id: Some(json!(7)),
            params: json!({"id": "com.x"}),
            session_token: None,
        };
        let mut idle = GateProgress::default();
        assert!(matches!(
            idle.ask_busy(None, &req("plugin.disable")),
            Step::Ready(())
        ));
        assert!(
            idle.busy_refusal(json!(7)).is_none(),
            "plugin 관리자가 없으면 묻지 않는다"
        );

        let (tx, rx) = sync_channel(1);
        tx.send(ok(json!({"busy": true, "reason": "turn"})))
            .unwrap();
        let mut probe = BusyProbe::for_test(vec![("com.x".into(), rx)]);
        assert!(probe.poll(false));
        let mut busy = GateProgress::with_busy_probe(probe);
        let refusal = busy.busy_refusal(json!(7)).expect("바쁜데 거절하지 않았다");
        assert_eq!(refusal.error.expect("error").code, ERR_PLUGIN_BUSY);
        assert!(busy.busy_refusal(json!(7)).is_none(), "거절을 두 번 냈다");

        let (tx, rx) = sync_channel(1);
        tx.send(ok(json!({"busy": false}))).unwrap();
        let mut probe = BusyProbe::for_test(vec![("com.x".into(), rx)]);
        assert!(probe.poll(false));
        let mut free = GateProgress::with_busy_probe(probe);
        assert!(free.busy_refusal(json!(7)).is_none());
    }

    // GUI와 헤드리스 진입부 모두 권한 게이트 앞에서 묻고, 게이트를 통과한 뒤에 거절한다.
    // 순서가 바뀌면 권한이 없는 호출자가 권한 거절 대신 바쁨을 받거나, 묻기 전에 실행된다.
    #[test]
    fn both_entry_points_ask_before_the_gate_and_refuse_after_it() {
        for (file, src, gate) in [
            (
                "src/app/ipc.rs",
                include_str!("../../../app/ipc.rs"),
                "self.gates_before_routing(",
            ),
            (
                "src/boot/headless_dispatch.rs",
                include_str!("../../../boot/headless_dispatch.rs"),
                "gate::decide(",
            ),
        ] {
            let ask = src
                .find("progress.ask_busy(")
                .unwrap_or_else(|| panic!("{file}: 묻지 않는다"));
            let gate_at = src
                .find(gate)
                .unwrap_or_else(|| panic!("{file}: 게이트가 없다"));
            let refuse = src
                .find("progress.busy_refusal(")
                .unwrap_or_else(|| panic!("{file}: 거절하지 않는다"));
            assert!(
                ask < gate_at && gate_at < refuse,
                "{file}: 묻기·게이트·거절 순서가 어긋났다"
            );
        }
    }
}
