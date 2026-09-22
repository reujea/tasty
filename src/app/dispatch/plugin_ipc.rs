//! plugin process 가 보낸 IPC 호출 dispatch.

use serde_json::json;
use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;
use crate::ipc;

impl App {
    /// plugin process가 보낸 IPC 호출들을 라우터로 디스패치하고 결과를 plugin에 회신.
    ///
    /// 게이트 3종(권한 / telemetry cap / rate limit)은 **갈래 분기보다 먼저** 돈다
    /// (ADR-0152). 인터셉트 갈래들이 각자 `ensure_allowed` 만 부르던 때에는 권한 한
    /// 축만 걸리고 cap·rate·audit 가 통째로 빠졌다 — 거부가 기록되지도 않았다.
    ///
    /// 호출 메서드가 다른 plugin이 점유한 namespace prefix와 매칭되면
    /// `forward_namespace_call_from_plugin` 경로로 우회 (응답은 target plugin이 줄
    /// 때까지 보류되며 main loop 다음 tick에서 caller plugin에 `ipc.result`로 회신).
    pub(crate) fn process_plugin_ipc_calls(&mut self) {
        let calls = match self.plugin_manager.as_mut() {
            Some(mgr) => mgr.take_pending_plugin_calls(),
            None => return,
        };
        for call in calls {
            let caller = Self::plugin_caller(&call);
            let request = Self::plugin_call_request(&call);
            let checked = match self.gates_before_routing(&request, &caller) {
                Ok(checked) => checked,
                Err(resp) => {
                    let (msg, code) = match resp.error {
                        Some(e) => (Some(e.message), Some(e.code)),
                        None => (None, None),
                    };
                    if let Some(mgr) = self.plugin_manager.as_mut() {
                        mgr.send_ipc_result(&call.plugin_id, call.call_id, None, msg, code);
                    }
                    continue;
                }
            };
            // shared buffer 생성은 main 채널 + 보조 채널을 동시에 다뤄야 해서
            // dispatcher에 노출하지 않고 매니저가 직접 처리한다.
            if call.method == tasty_plugin_protocol::METHOD_HOST_SHARED_BUFFER_CREATE {
                self.handle_ipc_shared_buffer_create(&call);
                continue;
            }
            if call.method == "popup.close" {
                self.handle_ipc_popup_close(&call);
                continue;
            }
            if call.method == "banner.open" {
                self.handle_ipc_banner_open(&call);
                continue;
            }
            if call.method == "banner.close" {
                self.handle_ipc_banner_close(&call);
                continue;
            }
            if call.method == "webview.open_external" {
                self.handle_ipc_webview_open_external(&call);
                continue;
            }
            // namespace forward 경로: 메서드가 다른 plugin의 prefix에 매칭되면
            // 검증/forward를 plugin_manager에 위임한다. 응답은 비동기.
            //
            // self-call(caller가 prefix owner와 동일)인 경우는 forward하지 않고
            // 호스트 dispatcher로 통과시킨다. plugin이 자기 namespace 메서드의
            // 구현을 호스트 본문에 위임하는 trampoline 패턴(예: com.tasty.image)을
            // 지원하기 위함. 호스트에 동명 메서드가 없으면 일반 -32601이 떨어진다.
            if let Some(mgr) = self.plugin_manager.as_mut()
                && mgr.namespace_belongs_to_other(&call.method, &call.plugin_id)
            {
                mgr.forward_namespace_call_from_plugin(
                    &call.method,
                    call.params.clone(),
                    &call.plugin_id,
                    call.call_id,
                );
                continue;
            }
            self.handle_ipc_default_dispatch(&call, &checked);
        }
    }

    /// `host.shared_buffer.create` 인터셉트 — main 채널 + 보조 채널을 동시에
    /// 다뤄야 해서 dispatcher에 노출하지 않고 매니저가 직접 처리한다. params에서
    /// size를 꺼내 manager에 위임 → 매니저가 fd/HANDLE 송신 + RPC 응답을 모두 처리.
    fn handle_ipc_shared_buffer_create(&mut self, call: &PendingPluginCall) {
        let size = call
            .params
            .get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if let Some(mgr) = self.plugin_manager.as_mut() {
            let (result, error) =
                match mgr.create_shared_buffer_for(&call.plugin_id, call.call_id, size) {
                    Ok(r) => (serde_json::to_value(&r).ok(), None),
                    Err(e) => (None, Some(e)),
                };
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, None);
        }
    }

    /// popup.close 인터셉트 — PluginManager가 App에 있어 일반 라우터로 도달 불가.
    /// 권한(ui.popup)은 진입부 pre-gate 가 이미 봤다 — 여기서는 instance_id 가 호출자
    /// plugin 소유인지만 확인하고 PluginRequest 사유로 close.
    fn handle_ipc_popup_close(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            let instance_id = call.params.get("instance_id").and_then(|v| v.as_u64());
            match instance_id {
                None => (None, Some("popup.close: missing 'instance_id'".to_string())),
                Some(id) => {
                    // 소유권 검증만 매니저를 빌려서 하고 곧바로 반납한다 —
                    // 아래 close 는 `&mut self` 를 다시 잡는다.
                    let owns = self.plugin_manager.as_ref().map(|m| {
                        m.popup_instances()
                            .find(|(iid, _)| *iid == id)
                            .is_some_and(|(_, inst)| inst.plugin_id == call.plugin_id)
                    });
                    match owns {
                        None => (None, Some("popup.close: plugin manager unavailable".into())),
                        Some(false) => (
                            None,
                            Some(format!(
                                "popup.close: instance {id} not owned by plugin '{}'",
                                call.plugin_id
                            )),
                        ),
                        Some(true) => {
                            // 매니저를 직접 치지 않는다 — 렌더가 수집하는 close 큐로
                            // 합류시켜야 `cancel_child_file_picker` 연쇄 정리가 이
                            // 경로에서도 돈다(ADR-0084 Decision 3). 큐는 같은 tick 의
                            // `dispatch_plugin_popup_events` 가 drain 하므로 지연 없다.
                            self.enqueue_plugin_popup_close(
                                id,
                                tasty_plugin_protocol::PopupCloseReason::PluginRequest,
                            );
                            (Some(serde_json::Value::Object(Default::default())), None)
                        }
                    }
                }
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, None);
        }
    }

    /// banner.open (A3) — plugin 이 자기 surface 에 egui-mesh 배너를 띄운다.
    /// ui.banner 권한 게이트 + D1 소유권 검증(자기 surface 만)은 open_plugin_banner 가.
    fn handle_ipc_banner_open(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            let banner_id = call.params.get("banner_id").and_then(|v| v.as_str());
            // `surface_id` 를 자르지 않는다 — 잘린 값은 실재하는 **다른 surface** 를
            // 가리켜, plugin 이 남의 배너 자리를 성공적으로 차지한다.
            let surface_id =
                crate::adapters::ipc::handler::params::read_u32(&call.params, "surface_id");
            match (banner_id, surface_id) {
                (Some(bid), Ok(Some(sid))) => {
                    let bid = bid.to_string();
                    match self.open_plugin_banner(Some(&call.plugin_id), &bid, sid) {
                        Ok(iid) => (Some(json!({ "instance_id": iid })), None),
                        Err(e) => (None, Some(e)),
                    }
                }
                (_, Err(msg)) => (None, Some(format!("banner.open: {msg}"))),
                _ => (
                    None,
                    Some("banner.open: missing 'banner_id' or 'surface_id'".to_string()),
                ),
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, None);
        }
    }

    /// banner.close (A3) — plugin 이 자기 배너 인스턴스를 닫는다.
    fn handle_ipc_banner_close(&mut self, call: &PendingPluginCall) {
        let (result, error) = {
            match call.params.get("instance_id").and_then(|v| v.as_u64()) {
                None => (
                    None,
                    Some("banner.close: missing 'instance_id'".to_string()),
                ),
                Some(iid) => {
                    if !self.plugin_owns_banner(&call.plugin_id, iid) {
                        (
                            None,
                            Some(format!(
                                "banner.close: instance {iid} not owned by plugin '{}'",
                                call.plugin_id
                            )),
                        )
                    } else {
                        self.close_plugin_banner(
                            iid,
                            tasty_plugin_protocol::BannerCloseReason::PluginRequest,
                        );
                        (Some(json!({ "closed": iid })), None)
                    }
                }
            }
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, None);
        }
    }

    /// 인터셉트/forward 대상이 아닌 일반 호출 — 호스트 dispatcher로 통과.
    fn handle_ipc_default_dispatch(
        &mut self,
        call: &PendingPluginCall,
        checked: &ipc::handler::CheckedRequest<'_>,
    ) {
        let response = self.dispatch_checked(checked);
        // 코드를 함께 넘긴다. 여기서 버리면 plugin 이 그 실패를 `?` 로 흘릴 때 외부
        // 호출자가 받는 코드가 전부 `-32000` 이 된다 — 호스트가 "인자를 고쳐라"
        // (`-32602`)로 거절한 것까지 "서버 사정" 으로 바뀐다.
        let (result, error, code) = match response.error {
            Some(err) => (None, Some(err.message), Some(err.code)),
            None => (response.result, None, None),
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, code);
        }
    }

    /// plugin 호출의 caller 컨텍스트. pre-gate 와 기본 갈래가 같은 것을 써야 한다 —
    /// 게이트가 본 caller 와 핸들러가 본 caller 가 갈리면 게이트가 무의미해진다.
    fn plugin_caller(call: &PendingPluginCall) -> ipc::caller::CallerContext {
        ipc::caller::CallerContext::Plugin {
            plugin_id: call.plugin_id.clone(),
            permissions: call.permissions.clone(),
        }
    }

    /// plugin 호출을 JSON-RPC 요청으로 옮긴다. pre-gate 와 기본 갈래가 공유한다.
    fn plugin_call_request(call: &PendingPluginCall) -> ipc::protocol::JsonRpcRequest {
        ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::from(call.call_id)),
            method: call.method.clone(),
            params: call.params.clone(),
            session_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use tasty_plugin_manifest::Permission;

    /// 이 파일의 소스 — 테스트 모듈 자신은 뺀다. 안 빼면 아래 문자열 리터럴이
    /// 스캔 대상에 걸려 가드가 자기를 보고 통과한다.
    fn source() -> &'static str {
        let full = include_str!("plugin_ipc.rs");
        full.split("\n#[cfg(test)]").next().unwrap_or(full)
    }

    /// 스캐너가 살아 있는가 — 찾는 형태가 실제로 있고, 없는 형태는 없다고 나오는가.
    /// 이게 없으면 아래 테스트들이 "위반 없음" 인지 "아무것도 안 봄" 인지 못 가른다.
    #[test]
    fn the_scanner_sees_this_file() {
        let src = source();
        assert!(
            src.contains("pub(crate) fn process_plugin_ipc_calls"),
            "진입 함수를 못 찾았다 — 이름이 바뀌었으면 이 가드도 같이 고쳐라"
        );
        assert!(
            !src.contains("#[cfg(test)]"),
            "테스트 모듈이 스캔 대상에 남아 있다 — 가드가 자기 리터럴을 본다"
        );
        assert!(
            src.matches("if call.method ==").count() >= 4,
            "갈래 분기를 못 찾았다: {}",
            src.matches("if call.method ==").count()
        );
        // 판정 형태는 **호출 형태**여야 한다. 맨 이름은 산문(주석)에도 나오므로,
        // 그걸로 세면 주석 한 줄이 가드를 영구히 빨갛게 만든다.
        assert!(
            src.contains("ensure_allowed"),
            "산문 앵커가 사라졌다 — 호출 형태 판정이 산문과 갈리는지 확인할 수 없다"
        );
    }

    /// 게이트는 갈래 분기보다 **먼저** 돈다. 어느 한 갈래라도 게이트 위로 올라오면
    /// 그 갈래는 권한·cap·rate·audit 를 통째로 건너뛴다 — 그게 ADR-0152 가 고친 것이다.
    #[test]
    fn the_entry_gates_before_it_branches() {
        let src = source();
        let gate = src
            .find("self.gates_before_routing(")
            .expect("진입부에서 gates_before_routing 호출을 못 찾았다");
        let first_branch = src
            .find("if call.method ==")
            .expect("갈래 분기를 못 찾았다");
        assert!(
            gate < first_branch,
            "게이트({gate})가 첫 갈래({first_branch})보다 뒤에 있다 — \
             그 사이의 갈래는 검사 없이 응답한다"
        );
    }

    /// 갈래 핸들러가 자기 권한 검사를 따로 들고 있으면 안 된다. 들고 있으면 그 갈래만
    /// 권한 한 축을 보고 cap·rate·audit 는 계속 빠진 채 "게이트가 있다" 로 읽힌다 —
    /// 수정 전 상태가 정확히 그거였다.
    #[test]
    fn no_branch_carries_its_own_permission_check() {
        let src = source();
        let n = src.matches(".ensure_allowed(").count();
        assert_eq!(
            n, 0,
            "갈래가 자기 권한 검사를 들고 있다({n} 곳) — 권한 판단은 진입부 \
             pre-gate 한 자리에서만 한다"
        );
    }

    /// 진입부에는 면제가 없다 — 모든 갈래가 게이트를 탄다. `host.shared_buffer.create`
    /// 는 `METHOD_TABLE` 에 등재돼 있어(현재 요구 토큰 없음) 게이트를 통과하지, 게이트를
    /// 건너뛰는 것이 아니다. 면제를 하나라도 되살리면 그 메서드만 cap·rate·audit 밖으로
    /// 빠지므로 수를 0 으로 박는다.
    #[test]
    fn the_entry_gates_every_branch_without_exemption() {
        let src = source();
        assert!(
            !src.contains("PRE_GATE_EXEMPT"),
            "면제 목록이 되살아났다 — 게이트 밖으로 빼는 대신 METHOD_TABLE 에 등재해라"
        );
        assert!(
            !src.contains("continue;\n            }\n            let caller"),
            "게이트 앞에 조기 continue 가 생겼다"
        );
    }

    /// 헤드리스 진입부도 같은 순서를 지킨다. GUI 만 재고 넘어가면 절반만 닫힌다 —
    /// 헤드리스의 일반 경로는 `handle_with_caller` 직결이라 안쪽 게이트를 타지만,
    /// 인터셉트는 그 함수에 도달하지 않아 게이트 밖이었다.
    #[test]
    fn the_headless_entry_also_gates_before_it_intercepts() {
        let src = include_str!("../../boot/headless_plugins.rs");
        let src = src.split("\n#[cfg(test)]").next().unwrap_or(src);
        let body = src
            .split_once("fn dispatch_plugin_ipc_calls_headless(")
            .expect("헤드리스 진입 함수를 못 찾았다")
            .1;
        let gate = body
            .find("gates_before_intercept(")
            .expect("헤드리스 진입부에서 pre-gate 호출을 못 찾았다");
        let intercept = body
            .find("call.method ==")
            .expect("헤드리스 인터셉트를 못 찾았다");
        assert!(
            gate < intercept,
            "게이트({gate})가 인터셉트({intercept})보다 뒤에 있다 — \
             그 갈래는 검사 없이 응답한다"
        );
        assert_eq!(
            body.matches("call.method ==").count(),
            1,
            "헤드리스가 이름으로 가로채는 갈래가 하나가 아니다 — 늘었으면 각각이 \
             게이트 뒤인지 확인해야 한다"
        );
    }

    /// 양방향 대조 — 막히는 것만 세면 "전부 막았다" 와 구별이 안 된다. 합성 권한
    /// 집합으로 세운다: 실제 plugin 매니페스트를 쓰면 그 매니페스트가 바뀌는 순간
    /// 이 테스트는 조용히 거짓 초록이 된다.
    #[test]
    fn the_gate_denies_without_the_token_and_allows_with_it() {
        let without = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: "test.synthetic".into(),
            permissions: HashSet::from([Permission::SurfaceRead]).into(),
        };
        let with = crate::ipc::caller::CallerContext::Plugin {
            plugin_id: "test.synthetic".into(),
            permissions: HashSet::from([Permission::UiBanner]).into(),
        };
        assert!(
            without.ensure_allowed("banner.open").is_err(),
            "ui.banner 없이 banner.open 이 통과했다"
        );
        assert!(
            with.ensure_allowed("banner.open").is_ok(),
            "ui.banner 를 들고도 banner.open 이 막혔다 — 통제군이 죽으면 \
             '전부 막혔다' 와 구별되지 않는다"
        );
    }
}
