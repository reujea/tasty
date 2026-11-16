//! 본체 시험이 dispatch 전체를 지나는 namespace 전달을 잴 수 있도록 여는 도구.
//! 프로세스 없이 prefix를 소유하는 stub 플러그인을 붙이고, 그 stub이 받은 요청과
//! 대기 항목의 원 요청 번호(`origin`)를 읽는다. 호출자 stub 플러그인으로 플러그인 사이 호출도 흉내 낸다.

use std::collections::HashSet;
use std::path::PathBuf;

use tasty_ipc::server::RequestSeq;
use tasty_plugin_manifest::Permission;

use super::{PendingPluginCall, PluginManager};
use crate::process::{PluginProcess, RequestTap};

/// stub 플러그인이 받은 요청을 읽는 수신단. 놓으면 stub으로 가는 송신이 끊긴다.
pub struct NamespaceStub {
    plugin_id: String,
    tap: RequestTap,
}

impl NamespaceStub {
    /// 지금까지 받은 요청을 꺼내 (호스트 req_id, `ipc.invoke` 의 메서드 이름) 으로 돌려준다.
    pub fn drain_invokes(&self) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        while let Ok(req) = self.tap.try_recv() {
            let method = req
                .params
                .get("method")
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            out.push((req.id, method));
        }
        out
    }

    /// 지금까지 받은 `ipc.invoke` 를 꺼내 (호스트 req_id, invoke params 전체) 로 돌려준다.
    pub fn drain_invoke_params(&self) -> Vec<(u64, serde_json::Value)> {
        let mut out = Vec::new();
        while let Ok(req) = self.tap.try_recv() {
            out.push((req.id, req.params));
        }
        out
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

/// 호출자 stub 플러그인이 받은 요청을 읽는 수신단. 놓으면 stub으로 가는 송신이 끊긴다.
pub struct CallerStub {
    plugin_id: String,
    tap: RequestTap,
}

impl CallerStub {
    /// 지금까지 받은 요청을 꺼내 (메서드 이름, params) 로 돌려준다. 회신은 `ipc.result` 다.
    pub fn drain_requests(&self) -> Vec<(String, serde_json::Value)> {
        let mut out = Vec::new();
        while let Ok(req) = self.tap.try_recv() {
            out.push((req.method, req.params));
        }
        out
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

impl PluginManager {
    /// 디스크를 읽지 않고 설치 목록과 각 플러그인의 허가 권한을 정한다. 기여 목록 조회를 시험할 때 쓴다.
    pub fn install_packages_for_test(
        &mut self,
        packages: Vec<crate::PluginPackage>,
        granted: &[&str],
    ) {
        for pkg in &packages {
            self.config.set_granted(
                &pkg.manifest.id,
                granted.iter().map(|t| t.to_string()).collect(),
            );
        }
        self.set_packages_for_tests(packages);
    }

    /// `prefix` namespace를 소유하는 연결된 stub 플러그인을 붙인다. 설치 목록은 이 하나로 바뀐다.
    pub fn attach_namespace_stub_for_test(
        &mut self,
        plugin_id: &str,
        prefix: &str,
    ) -> NamespaceStub {
        let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
            r#"
manifest_version = 1
id = "{plugin_id}"
name = "Namespace Stub"
version = "0.0.1"
api_version = "1.0"

[entry]
type = "process"
command = "echo"
args = []

[[contributes.ipc_namespace]]
prefix = "{prefix}"
"#
        ))
        .expect("stub manifest should parse");
        self.set_packages_for_tests(vec![crate::PluginPackage {
            dir: PathBuf::from("/nonexistent/namespace_stub"),
            manifest,
        }]);
        let (process, tap) = PluginProcess::stub_with_request_rx(plugin_id);
        self.processes.insert(plugin_id.to_string(), process);
        NamespaceStub {
            plugin_id: plugin_id.to_string(),
            tap,
        }
    }

    /// 연결된 호출자 stub 플러그인을 붙이고 권한을 정한다. 설치 목록은 바꾸지 않는다.
    pub fn attach_caller_stub_for_test(
        &mut self,
        plugin_id: &str,
        permissions: HashSet<Permission>,
    ) -> CallerStub {
        let (process, tap) = PluginProcess::stub_with_request_rx(plugin_id);
        self.processes.insert(plugin_id.to_string(), process);
        self.set_plugin_permissions(plugin_id, permissions);
        CallerStub {
            plugin_id: plugin_id.to_string(),
            tap,
        }
    }

    /// 편집 결과 회신을 약속한 연결된 stub 플러그인을 붙인다. 놓은 요청 수신단은 버린다.
    pub fn attach_edit_outcome_stub_for_test(&mut self, plugin_id: &str) {
        let (process, _tap) = PluginProcess::stub_with_request_rx(plugin_id);
        self.processes.insert(plugin_id.to_string(), process);
        self.apply_edit_events(vec![plugin_id.to_string()], Vec::new(), &[]);
    }

    /// stub 플러그인의 연결이 끊긴 것처럼 프로세스를 떼고 편집 상태를 정리한다.
    pub fn disconnect_edit_outcome_stub_for_test(&mut self, plugin_id: &str) {
        self.processes.remove(plugin_id);
        self.forget_edit_requests(plugin_id);
    }

    /// stub 플러그인 `plugin_id` 채널에서 편집 회신 하나가 온 것처럼 넣는다.
    pub fn receive_edit_outcome_for_test(
        &mut self,
        plugin_id: &str,
        target: tasty_plugin_protocol::CopyTargetWire,
        edit_id: u64,
        result: tasty_plugin_protocol::EditResultWire,
        copied: Option<&str>,
    ) {
        let item = super::edit_outcome::ReceivedEditOutcome {
            channel: plugin_id.to_string(),
            target,
            edit_id,
            result,
            copied: copied.map(str::to_string),
        };
        self.apply_edit_events(Vec::new(), vec![item], &[]);
    }

    /// 붙인 호출자 stub이 host call 을 보낸 것처럼 호출 하나를 대기열에 넣는다.
    /// 수집한 호출과 같이 현재 process 결속과 등록된 권한을 싣는다.
    pub fn queue_plugin_call_for_test(
        &mut self,
        plugin_id: &str,
        call_id: u64,
        method: &str,
        params: serde_json::Value,
    ) {
        let binding = self
            .processes
            .get(plugin_id)
            .expect("호출자 stub을 먼저 붙여야 한다")
            .reply_binding();
        let permissions = self
            .plugin_permissions
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        self.pending_plugin_calls.push(PendingPluginCall {
            binding,
            plugin_id: plugin_id.to_string(),
            call_id,
            method: method.to_string(),
            params,
            permissions,
        });
    }

    /// stub 소유자가 req_id 요청에 result 로 답한 것처럼 응답을 처리한다.
    pub fn answer_namespace_stub_for_test(
        &mut self,
        plugin_id: &str,
        req_id: u64,
        result: serde_json::Value,
    ) {
        self.handle_plugin_response(
            plugin_id,
            crate::protocol::PluginResponse {
                id: req_id,
                result: Some(result),
                error: None,
                error_code: None,
            },
        );
    }

    /// `plugin_id` 앞 대기 항목의 (호스트 req_id, 원 요청 번호), req_id 순.
    pub fn pending_origins_for_test(&self, plugin_id: &str) -> Vec<(u64, Option<RequestSeq>)> {
        let mut out: Vec<_> = self
            .pending_requests
            .iter()
            .filter(|(_, p)| p.to == plugin_id)
            .map(|(id, p)| (*id, p.origin))
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use tasty_terminal::waker_factory::NoopWakerFactory;

    use crate::manager::PendingPluginCall;

    use super::*;

    const OWNER: &str = "com.example.owner";
    const CALLER: &str = "com.example.caller";

    /// `ownns` 를 소유한 stub과 `caller_perms` 권한의 호출자 stub을 붙인 매니저.
    fn caller_and_owner(
        caller_perms: HashSet<Permission>,
    ) -> (PluginManager, NamespaceStub, CallerStub) {
        let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
        let owner = mgr.attach_namespace_stub_for_test(OWNER, "ownns");
        let caller = mgr.attach_caller_stub_for_test(CALLER, caller_perms);
        (mgr, owner, caller)
    }

    fn take_one_call(mgr: &mut PluginManager) -> PendingPluginCall {
        let mut calls = mgr.take_pending_plugin_calls();
        assert_eq!(calls.len(), 1);
        calls.remove(0)
    }

    /// 허가된 호출자의 다른 namespace 호출은 소유자에 한 번 가고, 소유자의 답이 호출자에
    /// 같은 call_id 의 `ipc.result` 로 돌아온다. 호출자가 실은 params 는 바꾸지 않는다.
    #[test]
    fn a_granted_cross_namespace_call_round_trips_to_the_caller() {
        let (mut mgr, owner, caller) =
            caller_and_owner(HashSet::from([Permission::IpcInvoke("ownns".into())]));
        let params = serde_json::json!({"_tasty_context": {"provider": "x", "thread_id": "t"}});
        mgr.queue_plugin_call_for_test(CALLER, 41, "ownns.parent", params.clone());
        let call = take_one_call(&mut mgr);
        assert!(mgr.forward_plugin_call_to_other_namespace(&call, false));
        let sent = owner.drain_invoke_params();
        assert_eq!(sent.len(), 1, "소유자에게 한 번만 가야 한다: {sent:?}");
        assert_eq!(sent[0].1["method"], "ownns.parent");
        assert_eq!(sent[0].1["params"], params);
        assert_eq!(sent[0].1["caller_plugin_id"], CALLER);
        assert!(caller.drain_requests().is_empty(), "답 전에는 회신이 없다");

        mgr.answer_namespace_stub_for_test(OWNER, sent[0].0, serde_json::json!({"ok": 1}));
        let replies = caller.drain_requests();
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert_eq!(replies[0].0, crate::protocol::METHOD_IPC_RESULT);
        assert_eq!(replies[0].1["call_id"], 41);
        assert_eq!(replies[0].1["result"], serde_json::json!({"ok": 1}));
    }

    /// 권한이 없으면 소유자에게 보내지 않고 호출자에게 권한 오류로 답한다. 전달 경로가
    /// 처리했으므로 호스트 처리기로 넘기지 않는다(`true`).
    #[test]
    fn an_ungranted_cross_namespace_call_is_refused_to_the_caller() {
        let (mut mgr, owner, caller) = caller_and_owner(HashSet::new());
        mgr.queue_plugin_call_for_test(CALLER, 7, "ownns.parent", serde_json::json!({}));
        let call = take_one_call(&mut mgr);
        assert!(mgr.forward_plugin_call_to_other_namespace(&call, false));
        assert!(owner.drain_invoke_params().is_empty());
        let replies = caller.drain_requests();
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert_eq!(replies[0].1["call_id"], 7);
        assert_eq!(replies[0].1["error_code"], -32001, "{replies:?}");
        assert!(
            replies[0].1["error"]
                .as_str()
                .is_some_and(|e| e.contains("ipc.invoke:ownns")),
            "{replies:?}"
        );
    }

    /// 자기 namespace·소유자 없는 메서드·호스트가 직접 처리하는 메서드는 전달하지 않는다.
    #[test]
    fn calls_the_host_answers_are_not_forwarded() {
        let grant = HashSet::from([Permission::IpcInvoke("ownns".into())]);
        let (mut mgr, owner, caller) = caller_and_owner(grant);
        for (from, method, host_handles) in [
            (OWNER, "ownns.parent", false),
            (CALLER, "workspace.list", false),
            (CALLER, "ownns.parent", true),
        ] {
            mgr.queue_plugin_call_for_test(from, 1, method, serde_json::json!({}));
            let call = take_one_call(&mut mgr);
            assert!(
                !mgr.forward_plugin_call_to_other_namespace(&call, host_handles),
                "{from} {method} host_handles={host_handles}"
            );
        }
        assert!(owner.drain_invoke_params().is_empty());
        assert!(caller.drain_requests().is_empty());
    }

    #[test]
    fn a_forward_reaches_the_stub_with_its_origin() {
        let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
        let stub = mgr.attach_namespace_stub_for_test("com.example.stub", "stubns");
        assert!(mgr.owns_namespace("stubns.run"));
        let seq = RequestSeq::next();
        let (tx, _rx) = mpsc::sync_channel(1);
        mgr.forward_namespace_call(
            "stubns.run",
            serde_json::json!({}),
            None,
            serde_json::json!(1),
            tx,
            Some(seq),
        );
        let sent = stub.drain_invokes();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, "stubns.run");
        assert_eq!(
            mgr.pending_origins_for_test(stub.plugin_id()),
            [(sent[0].0, Some(seq))]
        );
    }
}
