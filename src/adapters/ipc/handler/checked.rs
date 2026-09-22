//! 요청과 caller에 묶인 게이트 통과 증거. wire에서 만들거나 역직렬화할 수 없다.
use super::{CallerContext, JsonRpcRequest, JsonRpcResponse};
use crate::core::{Core, CoreState};

/// 라우팅 전에 권한·cap·rate 검사 및 허용 관측을 끝낸 요청.
pub(crate) struct CheckedRequest<'a> {
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
}

impl<'a> CheckedRequest<'a> {
    pub(crate) fn request(&self) -> &'a JsonRpcRequest {
        self.request
    }

    pub(crate) fn caller(&self) -> &'a CallerContext {
        self.caller
    }
}

/// 모든 진입점의 공통 게이트. 거부는 한 번 기록하고, 허용만 한 번 계측한다.
pub(crate) fn check_request<'a>(
    core: &mut Core,
    window: &mut dyn crate::ipc::window_port::IpcWindow,
    engine: &mut CoreState,
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
) -> Result<CheckedRequest<'a>, JsonRpcResponse> {
    let canonical = crate::ipc::alias::canonicalize(&request.method);
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let ws = engine
        .workspaces
        .get(window.active_workspace_index())
        .map(|w| w.id);
    // 거절은 게이트마다 따로 센다 — 처방이 셋 다 다르다(ADR-0548). 앞 게이트가 돌려보내면 뒤
    // 게이트는 안 돌므로 한 요청은 많아야 한 칸에 세진다.
    use tasty_telemetry::GateRefusal;
    core.gate().record_judged();
    let refused = super::check_permission_gate(core, window, engine, caller, canonical, ws, &id)
        .map(|r| (GateRefusal::Permission, r))
        .or_else(|| {
            super::check_cap_gate(core, engine, caller, canonical, ws, &id)
                .map(|r| (GateRefusal::Cap, r))
        })
        .or_else(|| {
            super::check_rate_limit_gate(core, engine, caller, canonical, ws, &id)
                .map(|r| (GateRefusal::Throttle, r))
        });
    if let Some((by, response)) = refused {
        core.gate().record_refusal(by);
        return Err(response);
    }
    super::record_telemetry_and_audit(core, window, engine, caller, canonical, &request.params, ws);
    // 봉투 검사는 **모든 층의 앞**이다 — App 층·namespace forward·engine 라우터 중 어디로
    // 가든 같은 봉투는 같은 판정을 받는다. 게이트 **뒤**인 것은 옛 자리(engine 라우터의
    // 보존소 입구)가 게이트 뒤였기 때문이다: 권한 없는 호출자는 여전히 `-32001` 을 먼저
    // 받고, 허용 관측도 예전처럼 한 번 남는다(ADR-0420).
    super::idempotency::check_envelope(request, &id)?;
    Ok(CheckedRequest { request, caller })
}

/// 창/parked engine이 전혀 없는 GUI 부팅·종료 구간에는 Local만 진입 가능하다.
/// Local의 기존 부팅 예외는 유지한다. Agent를 관측 문맥 없이 통과시키지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn check_without_engine<'a>(
    request: &'a JsonRpcRequest,
    caller: &'a CallerContext,
) -> Result<CheckedRequest<'a>, JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    if matches!(caller, CallerContext::Local) {
        // 창이 없어도 봉투 판정은 같다 — [`check_request`] 와 같은 자리.
        super::idempotency::check_envelope(request, &id)?;
        return Ok(CheckedRequest { request, caller });
    }
    Err(JsonRpcResponse::error(
        id,
        -32000,
        "no application state available for IPC admission",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
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
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params: json!({}),
            session_token: None,
        }
    }

    fn budget(core: &Core) {
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

    fn observations(core: &mut Core, state: &mut AppState, engine: &mut CoreState) -> usize {
        let mut req = request("telemetry.summary");
        req.params = json!({"agent": "gate-probe", "metric": "ipc_calls"});
        super::super::handle_with_caller(core, state, engine, &req, &CallerContext::Local)
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
            let (mut state, mut engine) = crate::state::tests::test_state();
            let caller = agent(&[Permission::SurfaceRead]);
            let req = request("surface.kinds");
            budget(&core);
            let response = if prechecked {
                let checked = check_request(&mut core, &mut state, &mut engine, &req, &caller)
                    .unwrap_or_else(|_| panic!("first request must pass"));
                super::super::handle_checked_request(&mut core, &mut state, &mut engine, &checked)
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
        let (mut state, mut engine) = crate::state::tests::test_state();
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

    // 압력 계측의 경계. 큐 대기는 게이트 **앞**에서 재고(거부된 요청도 큐에 앉아
    // 있었으므로 그 시간은 실재한다) handler 실행 시간은 게이트 **뒤**에서 잰다.
    // 둘을 같은 자리에서 재면 거부가 실행 비용으로 보이고, 느린 응답의 원인을
    // 적체와 handler 중 어느 쪽으로도 고를 수 없게 된다.
    #[test]
    fn only_a_request_that_passed_the_gate_is_timed_as_a_handler() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
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

    /// 게이트 셋이 돌려보낸 요청이 **각자의 칸**으로, 프로덕션 조회(`system.pressure`)까지 나간다.
    ///
    /// 거절을 게이트마다 다른 수로 일으켜, 한 칸이 다른 칸으로 새거나 두 칸이 합쳐지면 대조가
    /// 깨지게 한다. `judged` 는 통과한 요청과 조회 자신까지 센다.
    #[test]
    fn each_gate_refusal_is_counted_in_its_own_slot_of_the_pressure_answer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = super::super::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        let req = request("surface.kinds");
        let mut call = |core: &mut Core, caller: &CallerContext| {
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
            permissions: Arc::new([Permission::SurfaceRead].into_iter().collect()),
        };
        assert_eq!(call(&mut core, &plugin), Some(-32007));
        // Local 은 어느 게이트에도 안 걸린다.
        assert_eq!(call(&mut core, &CallerContext::Local), None);

        let mut query = request("system.pressure");
        query.params = json!({});
        let answer = super::super::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &query,
            &CallerContext::Local,
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
        let (mut state, mut engine) = crate::state::tests::test_state();
        let req = request("surface.kinds");
        budget(&core);
        for _ in 0..3 {
            assert!(
                super::super::handle_with_caller(
                    &mut core,
                    &mut state,
                    &mut engine,
                    &req,
                    &CallerContext::Local
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
        assert!(check_without_engine(&req, &CallerContext::Local).is_ok());
        assert!(check_without_engine(&req, &agent(&[Permission::SurfaceRead])).is_err());
    }
}
