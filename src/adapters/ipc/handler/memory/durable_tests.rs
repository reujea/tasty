//! 쓰기 응답의 `durable: false` — `memory.db` 를 못 열어 in-memory 대체로 뜬 호스트가
//! 쓰기를 정상과 똑같은 `ok` 로만 확인하던 결함의 회귀 시험(ADR-0485).

use serde_json::{Value, json};

use tasty_ipc::caller::CallerContext;
use tasty_ipc::method_meta::{METHOD_TABLE, MethodEffect};

use super::super::cli_entry_tests::test_core_builder;

fn fallback() -> tasty_memory::InitFallback {
    tasty_memory::InitFallback {
        cause: "corrupt",
        error: "memory.db corrupted: /x/memory.db".into(),
    }
}

/// 실제 스토어(in-memory SQLite)를 넣은 `Core`. 시험용 fake 는 `import_regular` 를
/// 구현하지 않는다.
fn core_with(fallback: Option<tasty_memory::InitFallback>) -> crate::core::Core {
    let store = tasty_memory::MemoryStore::open_in_memory().expect("store");
    test_core_builder()
        .with_memory(std::sync::Arc::new(std::sync::Mutex::new(store)))
        .with_memory_init_fallback(fallback)
        .build()
        .expect("core")
}

fn call(core: &mut crate::core::Core, method: &str, params: Value) -> Value {
    let (mut state, mut engine) = crate::state::tests::test_state();
    let req = tasty_ipc::protocol::JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        session_token: None,
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params,
    };
    let resp = super::super::handle_with_caller(
        core,
        &mut state,
        &mut engine,
        &req,
        &CallerContext::Local,
    );
    resp.result
        .unwrap_or_else(|| panic!("{method} 이 실패했다: {:?}", resp.error))
}

fn writes() -> Vec<(&'static str, Value)> {
    vec![
        (
            "memory.put",
            json!({ "scope": "global", "key": "k", "value": "v" }),
        ),
        ("memory.delete", json!({ "scope": "global", "key": "k" })),
        ("memory.import", json!({ "entries": [], "replace": false })),
        ("memory.gc", json!({})),
    ]
}

/// `memory.*` 밖에서 같은 `memory.db` 에 쓰는 메서드 — 이름공간마다 하나 이상, 응답 모양이
/// 다른 것(직렬화한 레코드 · `ok` · `deleted` · 토큰)을 고른다.
fn writes_outside_memory() -> Vec<(&'static str, Value)> {
    vec![
        (
            "surface.meta.set",
            json!({ "surface_id": 1, "key": "role", "value": "x" }),
        ),
        (
            "surface.meta.unset",
            json!({ "surface_id": 1, "key": "role" }),
        ),
        (
            "approval.summary.set",
            json!({ "workspace_id": 1, "content": "s" }),
        ),
        (
            "agent.semaphore_create",
            json!({ "workspace_id": 1, "name": "s", "permits": 1 }),
        ),
        (
            "agent.semaphore_delete",
            json!({ "workspace_id": 1, "name": "s" }),
        ),
        (
            "agent.lease_acquire",
            json!({ "workspace_id": 1, "resource": "r", "holder": "h" }),
        ),
        (
            "telemetry.cap.set",
            json!({ "agent": "a", "metric": "m", "threshold": 1.0, "window": "total", "action": "notify" }),
        ),
        ("session.issue", json!({ "agent_id": "a" })),
    ]
}

/// 대체 저장소에서는 쓰기가 성공해도 `durable: false` 가 붙고, `ok` 등 기존 칸은 그대로다.
/// 응답 모양이 다른 넷(`ok`+`version` · `ok` · `applied`/`skipped` · `regular`/`secret`)을 본다.
#[test]
fn a_write_to_a_fallback_store_says_it_is_not_durable() {
    let mut core = core_with(Some(fallback()));
    for (method, params) in writes() {
        let result = call(&mut core, method, params);
        assert_eq!(
            result["durable"], false,
            "{method}: 대체 저장소의 쓰기가 durable 로 보였다: {result}"
        );
    }
    for (method, params) in writes_outside_memory() {
        let result = call(&mut core, method, params);
        assert_eq!(
            result["durable"], false,
            "{method}: 대체 저장소의 쓰기가 durable 로 보였다: {result}"
        );
    }
    let put = call(
        &mut core,
        "memory.put",
        json!({ "scope": "global", "key": "k2", "value": "v" }),
    );
    assert_eq!(put["ok"], true, "기존 칸 `ok` 가 사라졌다: {put}");
    assert!(
        put.get("version").is_some(),
        "기존 칸 `version` 이 사라졌다: {put}"
    );
}

/// 정상 저장소에서는 칸을 싣지 않는다 — 응답이 이 칸이 생기기 전과 같다.
#[test]
fn a_write_to_a_durable_store_answers_as_before() {
    let mut core = core_with(None);
    for (method, params) in writes().into_iter().chain(writes_outside_memory()) {
        let result = call(&mut core, method, params);
        assert!(
            result.get("durable").is_none(),
            "{method}: 정상 저장소의 응답에 칸이 생겼다: {result}"
        );
    }
}

/// `memory.db` 에 쓰는 메서드는 **전부** [`super::written`] 이나 [`super::mark_durability`]
/// 로 답한다 — `memory.*` 만이 아니라 같은 저장소에 쓰는 이웃 이름공간도 그렇다.
///
/// 쓰기 계열은 메서드 표의 효과 분류(`MethodEffect::Read` 가 아닌 것)로 정한다 — 목록을
/// 여기 따로 두면 표와 갈린다. 표가 쓰기로 적지만 핸들러가 저장소에 쓰지 않는 것만
/// 사유와 함께 뺀다. 이름 → 핸들러 함수는 라우터 본문의 arm 에서, 함수 본문은 아래
/// 소스들에서 읽는다(함수 이름이 두 소스에 있으면 어느 본문인지 모르므로 실패한다).
#[test]
fn every_memory_write_reports_a_fallback_store_as_not_durable() {
    /// 이 저장소(`memory.db`)에 쓰는 이름공간. 여기 없는 이름공간은 다른 저장소
    /// (`state.db` · 설정 파일 · 프로세스 메모리)에 쓴다.
    const NAMESPACES: &[&str] = &[
        "memory.",
        "agent.",
        "approval.",
        "surface.meta.",
        "telemetry.",
        "session.",
    ];
    const NOT_A_STORE_WRITE: &[(&str, &str)] = &[
        (
            "memory.export",
            "표가 Mutate 로 적지만 핸들러는 저장소를 읽기만 한다(export_regular)",
        ),
        (
            "agent.task_run",
            "runner 스레드를 켜고 끌 뿐 응답 전에 저장소에 쓰지 않는다",
        ),
        (
            "agent.task_reduce",
            "저장된 결과를 모아 계산해 돌려줄 뿐 쓰지 않는다",
        ),
    ];
    let router = include_str!("../../handler.rs");
    let sources = [
        include_str!("../memory.rs"),
        include_str!("advanced.rs"),
        include_str!("bb.rs"),
        include_str!("cache.rs"),
        include_str!("goal.rs"),
        include_str!("plan.rs"),
        include_str!("secret.rs"),
        include_str!("../agent/barrier.rs"),
        include_str!("../agent/lease.rs"),
        include_str!("../agent/ratelimit.rs"),
        include_str!("../agent/semaphore.rs"),
        include_str!("../agent/task.rs"),
        include_str!("../approval/read.rs"),
        include_str!("../approval/request.rs"),
        include_str!("../approval/respond.rs"),
        include_str!("../approval/summary.rs"),
        include_str!("../meta.rs"),
        include_str!("../telemetry/cap.rs"),
        include_str!("../telemetry/record.rs"),
        include_str!("../session.rs"),
    ];
    let body_of = |func: &str| -> Option<&'static str> {
        let head = format!("\npub fn {func}(");
        let mut found = sources.iter().filter_map(|src| {
            let at = src.find(&head)?;
            let rest = &src[at + 1..];
            let end = rest[1..].find("\npub fn ").map_or(rest.len(), |i| i + 1);
            Some(&rest[..end])
        });
        let body = found.next()?;
        assert!(
            found.next().is_none(),
            "`{func}` 이 두 소스에 있다 — 어느 본문을 볼지 모른다"
        );
        Some(body)
    };

    let mut checked = Vec::new();
    let mut missing = Vec::new();
    for (method, meta) in METHOD_TABLE {
        if !NAMESPACES.iter().any(|ns| method.starts_with(ns))
            || meta.effect == MethodEffect::Read
            || NOT_A_STORE_WRITE.iter().any(|(m, _)| m == method)
        {
            continue;
        }
        let arm = format!("\"{method}\" =>");
        let at = router
            .find(&arm)
            .unwrap_or_else(|| panic!("라우터에 `{method}` arm 이 없다"));
        let after = &router[at..];
        let call_at = after
            .find("::handle_")
            .unwrap_or_else(|| panic!("`{method}` arm 에서 핸들러를 못 찾았다"));
        let func: String = after[call_at + "::".len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let body = body_of(&func).unwrap_or_else(|| panic!("`{func}` 본문을 못 찾았다"));
        if !body.contains("written(") && !body.contains("mark_durability(") {
            missing.push(format!("{method} → {func}"));
        }
        checked.push(*method);
    }
    for ns in NAMESPACES {
        assert!(
            checked.iter().any(|m| m.starts_with(ns)),
            "`{ns}` 의 쓰기 계열을 하나도 못 찾았다 — 파서나 이름공간 목록이 낡았다: {checked:?}"
        );
    }
    assert!(
        checked.len() >= 50,
        "쓰기 계열을 {}개밖에 못 찾았다 — 파서가 낡았다: {checked:?}",
        checked.len()
    );
    for (exempt, _) in NOT_A_STORE_WRITE {
        assert!(
            METHOD_TABLE.iter().any(|(m, _)| m == exempt),
            "면제 목록의 `{exempt}` 이 표에 없다 — 면제가 낡았다"
        );
    }
    assert!(
        missing.is_empty(),
        "쓰기 성공을 durable 표시 없이 답하는 핸들러가 있다 — 대체 저장소에서 durable 로 보인다:\n{}",
        missing.join("\n")
    );
}
