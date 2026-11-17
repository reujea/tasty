//! 호출 경로에 필요한 진입 검사를 마친 요청. 외부 입력을 역직렬화해 만들 수 없다.
use super::{CallerContext, JsonRpcRequest};

/// 진입 검사를 통과한 요청. engine이 없는 GUI 부팅·종료 구간의 Local 호출은
/// 멱등성 봉투만 검사하며 권한·cap·호출 빈도 검사와 집계는 생략한다.
pub(crate) struct CheckedRequest<'a> {
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
    open_surfaces: Option<std::collections::HashSet<u32>>,
}

impl<'a> CheckedRequest<'a> {
    pub(crate) fn request(&self) -> &'a JsonRpcRequest {
        self.request
    }

    pub(crate) fn caller(&self) -> &'a CallerContext {
        self.caller
    }

    /// 여러 engine을 가진 라우터가 모든 engine의 열린 surface를 붙인 같은 요청.
    /// 한 요청이 여러 engine의 surface를 가리킬 수 있는 메서드가 전달받은 engine 밖을 판정할 때 쓴다.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn with_open_surfaces(
        &self,
        open: std::collections::HashSet<u32>,
    ) -> CheckedRequest<'a> {
        CheckedRequest {
            request: self.request,
            caller: self.caller,
            open_surfaces: Some(open),
        }
    }

    /// 라우터가 붙인 모든 engine의 열린 surface. 붙이지 않았으면 None이다.
    pub(crate) fn open_surfaces(&self) -> Option<&std::collections::HashSet<u32>> {
        self.open_surfaces.as_ref()
    }
}

/// An owned proof of the same admitted request, used only across application continuations.
/// No deserialization or unrestricted constructor can fabricate successful gate checks.
pub(crate) struct OwnedCheckedRequest {
    request: JsonRpcRequest,
    caller: CallerContext,
}
impl OwnedCheckedRequest {
    pub(crate) fn borrow(&self) -> CheckedRequest<'_> {
        CheckedRequest {
            request: &self.request,
            caller: &self.caller,
            open_surfaces: None,
        }
    }
    pub(crate) fn weight(&self) -> usize {
        serde_json::to_vec(&self.request).map_or(usize::MAX, |bytes| bytes.len())
    }
}
impl CheckedRequest<'_> {
    pub(crate) fn to_owned_without_key(&self) -> OwnedCheckedRequest {
        let mut request = self.request.clone();
        request.idempotency_key = None;
        OwnedCheckedRequest {
            request,
            caller: self.caller.clone(),
        }
    }
}

impl<'a> CheckedRequest<'a> {
    /// 요청 게이트([`super::gate`])가 모든 단계를 통과시킨 요청.
    pub(super) fn admitted(request: &'a JsonRpcRequest, caller: &'a CallerContext) -> Self {
        Self {
            request,
            caller,
            open_surfaces: None,
        }
    }
}

/// 창/parked engine이 전혀 없는 GUI 부팅·종료 구간에는 Local만 진입 가능하다.
/// Local의 기존 부팅 예외는 유지한다. Agent를 관측 문맥 없이 통과시키지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn check_without_engine<'a>(
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
) -> Result<CheckedRequest<'a>, super::JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    if matches!(caller, CallerContext::Local { .. }) {
        super::idempotency::check_envelope(request, &id)?;
        return Ok(CheckedRequest {
            request,
            caller,
            open_surfaces: None,
        });
    }
    Err(super::JsonRpcResponse::error(
        id,
        -32000,
        "no application state available for IPC admission",
    ))
}

#[cfg(test)]
mod tests {
    use super::super::check_request;
    use super::*;
    use crate::app::services::AppServices;
    use crate::state::RequestContext;
    use serde_json::json;
    use std::sync::Arc;
    use tasty_plugin_manifest::Permission;

    fn agent(permissions: &[Permission]) -> CallerContext {
        CallerContext::Agent {
            agent_id: "gate-probe".into(),
            permissions: Arc::new(permissions.iter().cloned().collect()),
        }
    }

    fn request(method: &str) -> JsonRpcRequest {
        JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params: json!({}),
            session_token: None,
        }
    }

    fn budget(core: &AppServices) {
        core.rate_limit_set(
            "gate-probe".into(),
            "ipc_calls".into(),
            1,
            86_400_000,
            Some(1),
            super::super::telemetry::now_ms(),
        )
        .unwrap();
    }

    fn observations(
        core: &mut AppServices,
        state: &mut RequestContext,
        engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    ) -> usize {
        let mut req = request("telemetry.summary");
        req.params = json!({"agent": "gate-probe", "metric": "ipc_calls"});
        super::super::handle_with_caller(core, state, engine, &req, &CallerContext::local())
            .result
            .unwrap()["total_events"]
            .as_u64()
            .unwrap() as usize
    }

    #[test]
    fn checked_and_direct_handlers_consume_and_observe_once() {
        let _home = crate::test_support::TastyHomeGuard::new();
        for prechecked in [true, false] {
            let mut core = super::super::cli_entry_tests::test_core();
            let (mut state, mut engine_session) = crate::state::tests::test_state();
            let mut engine = engine_session.borrow_mut();
            let caller = agent(&[Permission::SurfaceRead]);
            let req = request("surface.kinds");
            budget(&core);
            let response = if prechecked {
                let checked = check_request(&mut core, &mut state, &mut engine, &req, &caller)
                    .unwrap_or_else(|_| panic!("first request must pass"));
                super::super::handle_checked_request(&mut core, &mut state, &mut engine, &checked)
                    .wait(&core)
            } else {
                super::super::handle_with_caller(&mut core, &mut state, &mut engine, &req, &caller)
            };
            assert!(response.error.is_none(), "{response:?}");
            let second =
                super::super::handle_with_caller(&mut core, &mut state, &mut engine, &req, &caller);
            assert_eq!(second.error.unwrap().code, -32010);
            assert_eq!(observations(&mut core, &mut state, &mut engine), 1);
        }
    }

    #[test]
    fn permission_denial_does_not_consume_or_observe_allow() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let req = request("surface.kinds");
        budget(&core);
        let denied =
            super::super::handle_with_caller(&mut core, &mut state, &mut engine, &req, &agent(&[]));
        let error = denied.error.unwrap();
        assert_eq!(error.code, -32001);
        assert!(error.data.unwrap()["approval_id"].is_string());
        assert_eq!(observations(&mut core, &mut state, &mut engine), 0);
        let allowed = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &agent(&[Permission::SurfaceRead]),
        );
        assert!(allowed.error.is_none());
        assert_eq!(observations(&mut core, &mut state, &mut engine), 1);
    }

    // 거부된 요청도 큐에서 기다렸으므로 대기 시간에 포함한다. 실행 시간은 허용된 요청만 잰다.
    #[test]
    fn only_a_request_that_passed_the_gate_is_timed_as_a_handler() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let req = request("surface.kinds");
        budget(&core);

        let denied =
            super::super::handle_with_caller(&mut core, &mut state, &mut engine, &req, &agent(&[]));
        assert!(denied.error.is_some(), "권한 없는 호출은 거부된다");
        assert_eq!(
            core.pressure().snapshot().handler_calls,
            0,
            "거부는 handler 를 돌리지 않았으므로 실행 시간에 세지 않는다"
        );

        let allowed = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &agent(&[Permission::SurfaceRead]),
        );
        assert!(allowed.error.is_none());
        assert_eq!(
            core.pressure().snapshot().handler_calls,
            1,
            "통과한 요청 하나가 한 번 세져야 한다"
        );
    }

    /// 거부 사유별로 다른 횟수를 만들어 `system.pressure`의 집계가 섞이지 않는지 확인한다.
    /// `judged`에는 조회 요청 자체도 포함된다.
    #[test]
    fn each_gate_refusal_is_counted_in_its_own_slot_of_the_pressure_answer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let req = request("surface.kinds");
        let mut call = |core: &mut AppServices, caller: &CallerContext| {
            super::super::handle_with_caller(core, &mut state, &mut engine, &req, caller)
                .error
                .map(|e| e.code)
        };

        // 권한: 권한 없는 agent 두 번.
        for _ in 0..2 {
            assert_eq!(call(&mut core, &agent(&[])), Some(-32001));
        }
        // 스로틀: 버킷 1 개 — 첫 번째는 통과, 뒤의 셋은 돌려보낸다.
        budget(&core);
        assert_eq!(call(&mut core, &agent(&[Permission::SurfaceRead])), None);
        for _ in 0..3 {
            assert_eq!(
                call(&mut core, &agent(&[Permission::SurfaceRead])),
                Some(-32010)
            );
        }
        // cap: 발동된 Pause cap 이 걸린 plugin 한 번.
        let cap = tasty_telemetry::CostCap {
            id: "cap_gate_probe".into(),
            agent: "gate-plugin".into(),
            metric: "ipc_calls".into(),
            threshold: 1.0,
            window: tasty_telemetry::CapWindow::Total,
            action: tasty_telemetry::CapAction::Pause,
            created_at: 0,
            triggered: Some(tasty_telemetry::CapTriggered { at: 0, value: 1.0 }),
        };
        core.with_memory(|m| {
            m.put(
                tasty_memory::HOST_OWNER,
                &tasty_memory::Scope::Global,
                &tasty_telemetry::cap_key(&cap.id),
                &tasty_memory::MemoryValue::Json(serde_json::to_value(&cap).unwrap()),
                &tasty_memory::PutOpts::default(),
            )
        })
        .unwrap();
        let plugin = CallerContext::Plugin {
            plugin_id: "gate-plugin".into(),
        };
        assert_eq!(call(&mut core, &plugin), Some(-32007));
        // Local 은 어느 게이트에도 안 걸린다.
        assert_eq!(call(&mut core, &CallerContext::local()), None);

        let mut query = request("system.pressure");
        query.params = json!({});
        let answer = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &query,
            &CallerContext::local(),
        )
        .result
        .expect("local 은 조회할 수 있다");
        assert_eq!(
            answer["gate_refusals"],
            json!({
                "judged": 2 + 4 + 1 + 1 + 1,
                "permission_denied": 2,
                "cap_blocked": 1,
                "throttled": 3,
            }),
            "{answer}"
        );
    }

    #[test]
    fn local_remains_exempt_from_consumption_and_allow_observation() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let req = request("surface.kinds");
        budget(&core);
        for _ in 0..3 {
            assert!(
                super::super::handle_with_caller(
                    &mut core,
                    &mut state,
                    &mut engine,
                    &req,
                    &CallerContext::local()
                )
                .error
                .is_none()
            );
        }
        assert_eq!(observations(&mut core, &mut state, &mut engine), 0);
    }

    #[cfg(feature = "gui")]
    #[test]
    fn no_engine_allows_only_local_bootstrap() {
        let req = request("surface.kinds");
        assert!(check_without_engine(&req, &CallerContext::local()).is_ok());
        assert!(check_without_engine(&req, &agent(&[Permission::SurfaceRead])).is_err());
    }
}
