//! agent task 계약: 생성 검사, 기본 출력, 결과 확정, 세션 연결 기록.

use std::sync::atomic::AtomicU64;

use serde_json::{Value, json};
use tasty_memory::MemoryStore;
use tempfile::TempDir;

use super::agent::{self, AgentLink, AgentSession, report};
use super::contract::{FailureCode, FailureStage, TaskContract};
use super::*;
use crate::AgentError;

fn fresh_store() -> (TempDir, MemoryStore, AtomicU64) {
    tasty_test_support::namespace_owners::publish_fixture();
    let td = tempfile::tempdir().expect("tempdir");
    let mem = MemoryStore::open(&td.path().join("mem.db")).expect("mem");
    (td, mem, AtomicU64::new(0))
}

fn agent_cmd(provider: &str) -> TaskCommand {
    TaskCommand::Agent {
        provider: provider.into(),
        workspace_id: 1,
        session: AgentSession::Existing { surface_id: 7 },
        instruction: "review the diff".into(),
        timeout_ms: None,
    }
}

fn opts(command: TaskCommand) -> TaskCreateOpts {
    TaskCreateOpts {
        workspace_id: 1,
        name: "review".into(),
        command,
        depends_on: vec![],
        on_failure: OnFailure::Abort,
        metadata: Value::Null,
        now_ms: 0,
    }
}

fn contract(v: Value) -> TaskContract {
    serde_json::from_value(v).expect("contract")
}

fn v2() -> TaskContract {
    contract(json!({ "contract_version": 2 }))
}

fn review_contract() -> TaskContract {
    contract(json!({
        "contract_version": 2,
        "output_schema": { "type": "enum", "values": ["approve", "revise"] }
    }))
}

fn input_failure(e: AgentError) -> contract::TaskFailure {
    match e {
        AgentError::TypeContract(f) => *f,
        other => panic!("expected a contract failure, got {other:?}"),
    }
}

/// Running 으로 만들고 회차 id 를 돌려준다.
fn start(store: &mut TaskStore, id: &TaskId) -> String {
    store
        .set_state(1, id, TaskState::Running, 1)
        .expect("running");
    store
        .get(1, id)
        .unwrap()
        .unwrap()
        .attempt
        .expect("attempt")
        .id
}

fn done(output: Value) -> Completion {
    Completion {
        attempt_id: None,
        result: TaskResult {
            exit_code: None,
            output: Some(output),
            error: None,
        },
        outcome: CompletionOutcome::Succeeded,
        postprocess: None,
        main_copy_dropped: false,
    }
}

fn failed(error: String) -> Completion {
    Completion {
        attempt_id: None,
        result: TaskResult {
            exit_code: None,
            output: None,
            error: Some(error.clone()),
        },
        outcome: CompletionOutcome::Failed { error },
        postprocess: None,
        main_copy_dropped: false,
    }
}

#[test]
fn agent_tasks_need_a_v2_contract_and_a_supported_provider() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let e = store
        .create(opts(agent_cmd("claude")))
        .expect_err("v1 agent");
    assert!(
        matches!(&e, AgentError::InvalidArgument(m) if m == agent::AGENT_NEEDS_CONTRACT),
        "{e}"
    );
    let f = input_failure(
        store
            .create_typed(opts(agent_cmd("gemini")), v2())
            .expect_err("unknown provider"),
    );
    assert!(
        f.message.contains("does not support result collection"),
        "{f:?}"
    );
    let t = store
        .create_typed(opts(agent_cmd("codex")), v2())
        .expect("codex agent");
    assert_eq!(
        t.contract.as_ref().unwrap().output_schema(&t.command),
        super::types::TypeSchema::string(),
        "기본 출력은 최종 답변 string"
    );
}

/// provider 는 고정 목록이 아니라 설치 플러그인 정보로 판정하며 원인을 구분해 알린다.
#[test]
fn agent_provider_qualification_follows_plugin_information() {
    use tasty_test_support::namespace_owners::DISABLED;
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    store
        .create_typed(opts(agent_cmd("foo")), v2())
        .expect("a custom provider plugin qualifies");
    for (provider, reason) in [
        ("bar", "no installed plugin owns the 'bar' namespace"),
        (
            DISABLED,
            "plugin 'com.example.off' that owns the 'off' namespace is disabled",
        ),
    ] {
        let f = input_failure(
            store
                .create_typed(opts(agent_cmd(provider)), v2())
                .expect_err(provider),
        );
        assert!(
            f.message.contains("does not support result collection"),
            "{f:?}"
        );
        assert!(f.message.contains(reason), "{provider}: {f:?}");
    }
}

#[test]
fn agent_input_reaches_the_session_only_through_the_input_block() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let typed_input = json!({
        "contract_version": 2,
        "input_schema": { "type": "object", "fields": { "diff": { "type": "string" } } },
        "bindings": { "diff": { "literal": "x" } }
    });
    let f = input_failure(
        store
            .create_typed(opts(agent_cmd("claude")), contract(typed_input.clone()))
            .expect_err("unmapped input"),
    );
    assert!(f.message.contains("input_block"), "{f:?}");
    let mut with_block = typed_input;
    with_block["input_mapping"] = json!({ "input_block": true });
    store
        .create_typed(opts(agent_cmd("claude")), contract(with_block))
        .expect("input block");
    let run = TaskCommand::Run {
        command: vec!["true".into()],
        workspace_id: 1,
        cwd: None,
    };
    let f = input_failure(
        store
            .create_typed(
                opts(run),
                contract(json!({
                    "contract_version": 2,
                    "input_mapping": { "input_block": true }
                })),
            )
            .expect_err("input_block on run"),
    );
    assert!(f.message.contains("applies to agent"), "{f:?}");
}

#[test]
fn the_composed_instruction_keeps_the_input_as_a_json_block() {
    let text = agent::compose_instruction("do it", Some(&json!({ "n": "1" })));
    assert!(text.starts_with("do it\n\n"));
    assert!(text.contains(agent::INPUT_BLOCK_HEADER));
    assert!(text.contains("\"n\": \"1\""));
    assert_eq!(agent::compose_instruction("do it", None), "do it");
}

#[test]
fn the_final_answer_is_the_default_output() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let t = store.create_typed(opts(agent_cmd("claude")), v2()).unwrap();
    start(&mut store, &t.id);
    let receipt = store
        .complete(
            1,
            &t.id,
            done(json!({ report::FINAL_ANSWER: "looks good", report::PROVIDER: "claude" })),
            2,
        )
        .expect("complete");
    assert_eq!(receipt.task.state, TaskState::Succeeded);
    let typed = receipt.task.typed_result.expect("typed");
    assert_eq!(typed.output.to_wire(), json!("looks good"));
    assert_eq!(typed.provenance.output_source, "agent.final_answer");
    assert_eq!(
        typed.raw.execution.expect("raw")[report::FINAL_ANSWER],
        "looks good",
        "원본 보고를 raw 에 보존한다"
    );
}

#[test]
fn a_submitted_revise_is_a_successful_business_result() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let t = store
        .create_typed(opts(agent_cmd("claude")), review_contract())
        .unwrap();
    assert!(agent::needs_submission(
        t.contract.as_ref().unwrap(),
        &t.command
    ));
    start(&mut store, &t.id);
    let receipt = store
        .complete(
            1,
            &t.id,
            done(json!({ report::FINAL_ANSWER: "please fix", report::SUBMITTED: "revise" })),
            2,
        )
        .expect("complete");
    assert_eq!(receipt.task.state, TaskState::Succeeded);
    let typed = receipt.task.typed_result.unwrap();
    assert_eq!(typed.output.to_wire(), json!("revise"));
    assert_eq!(typed.provenance.output_source, "agent.submitted");
}

#[test]
fn missing_result_exit_and_turn_error_stay_distinguishable() {
    let cases = [
        (FailureCode::ResultMissing, FailureStage::OutputValidation),
        (FailureCode::AgentExited, FailureStage::Execution),
        (FailureCode::AgentTurnError, FailureStage::Execution),
        (FailureCode::TimedOut, FailureStage::Execution),
    ];
    for (code, stage) in cases {
        let (_td, mut mem, seq) = fresh_store();
        let mut store = TaskStore::new(&mut mem, "host", &seq);
        let t = store
            .create_typed(opts(agent_cmd("claude")), review_contract())
            .unwrap();
        start(&mut store, &t.id);
        let receipt = store
            .complete(1, &t.id, failed(code.message("detail")), 2)
            .expect("complete");
        assert!(matches!(receipt.task.state, TaskState::Failed { .. }));
        let err = receipt.task.typed_result.unwrap().error.expect("error");
        assert_eq!((err.code, err.stage), (Some(code), stage), "{err:?}");
    }
}

/// 제출이 필요한 출력에서 답만 있으면 result_missing 이고, 그 답은 raw 에 남는다.
#[test]
fn a_missing_submission_keeps_the_last_answer_in_the_record() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let t = store
        .create_typed(opts(agent_cmd("claude")), review_contract())
        .unwrap();
    start(&mut store, &t.id);
    // "revise" 는 출력 타입에 맞지만 제출되지 않았으므로 결과가 아니다.
    let receipt = store
        .complete(1, &t.id, done(json!({ report::FINAL_ANSWER: "revise" })), 2)
        .expect("complete");
    assert!(matches!(receipt.task.state, TaskState::Failed { .. }));
    let typed = receipt.task.typed_result.unwrap();
    let err = typed.error.expect("error");
    assert_eq!(err.code, Some(FailureCode::ResultMissing));
    assert_eq!(err.stage, FailureStage::OutputValidation);
    assert!(
        err.message.starts_with("result_missing: "),
        "{}",
        err.message
    );
    assert_eq!(
        typed.raw.execution.expect("raw")[report::FINAL_ANSWER],
        "revise"
    );
    assert_eq!(typed.provenance.output_source, "agent.submitted");
}

#[test]
fn a_report_without_any_answer_is_result_missing() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let t = store.create_typed(opts(agent_cmd("claude")), v2()).unwrap();
    start(&mut store, &t.id);
    let receipt = store
        .complete(1, &t.id, done(json!({ report::FINAL_ANSWER: null })), 2)
        .expect("complete");
    let err = receipt.task.typed_result.unwrap().error.expect("error");
    assert_eq!(err.code, Some(FailureCode::ResultMissing));
}

#[test]
fn the_session_link_belongs_to_the_running_attempt_only() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let t = store.create_typed(opts(agent_cmd("claude")), v2()).unwrap();
    let link = AgentLink {
        provider: "claude".into(),
        surface_id: 7,
        awaiting_input_since: Some(5),
    };
    let attempt = start(&mut store, &t.id);
    assert!(
        !store
            .set_agent_link(1, &t.id, "other#9", link.clone())
            .unwrap(),
        "다른 회차에는 쓰지 않는다"
    );
    assert!(
        store
            .set_agent_link(1, &t.id, &attempt, link.clone())
            .unwrap()
    );
    let got = store.get(1, &t.id).unwrap().unwrap();
    assert_eq!(got.attempt.as_ref().unwrap().agent, Some(link));
    assert_eq!(agent::phase(&got), Some("awaiting_input"));
    let dags = group_tasks_into_dags(&store.list(1).unwrap());
    assert_eq!(
        dags[0].state_counts.awaiting_input, 1,
        "DAG 요약이 입력 대기를 센다"
    );
    assert_eq!(
        dags[0].rollup_state, "running",
        "입력 대기도 rollup 은 실행 중이다"
    );
    store
        .set_state(1, &t.id, TaskState::Cancelled, 3)
        .expect("cancel");
    let got = store.get(1, &t.id).unwrap().unwrap();
    assert_eq!(agent::phase(&got), None);
    assert!(
        !store
            .set_agent_link(
                1,
                &t.id,
                &attempt,
                AgentLink {
                    awaiting_input_since: None,
                    ..got.attempt.unwrap().agent.unwrap()
                }
            )
            .unwrap(),
        "끝난 task 에는 쓰지 않는다"
    );
}

#[test]
fn failure_codes_round_trip_through_the_message() {
    for c in FailureCode::ALL {
        assert_eq!(FailureCode::parse_message(&c.message("x: y")), Some(c));
    }
    assert_eq!(FailureCode::parse_message("plain error"), None);
    assert_eq!(FailureCode::parse_message("other_code: x"), None);
}

/// 실행 보고는 raw 에 상한까지만 둔다. 출력은 따로 저장하므로 최종 답변이 아무리 길어도 유효한
/// 제출은 출력이 되고, 레코드는 memory 값 상한 안에 머문다.
#[test]
fn an_agent_report_over_the_raw_cap_keeps_its_output_and_only_a_head_in_raw() {
    use super::contract::EXECUTION_RESPONSE_CAP;
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let string_out = contract(json!({"contract_version": 2, "output_schema": {"type": "string"}}));
    let mut complete = |c: TaskContract, report: Value| {
        let t = store.create_typed(opts(agent_cmd("codex")), c).unwrap();
        let attempt = start(&mut store, &t.id);
        let mut d = done(report);
        d.attempt_id = Some(attempt);
        let t = store.complete(1, &t.id, d, 2).expect("stored").task;
        assert_eq!(store.get(1, &t.id).unwrap().as_ref(), Some(&t));
        t
    };

    let small = json!({"provider": "codex", "surface_id": 7, "final_answer": "approve"});
    let t = complete(v2(), small.clone());
    let raw = &t.typed_result.as_ref().unwrap().raw;
    assert_eq!(raw.execution, Some(small));
    assert!(raw.execution_truncated.is_none());

    // 제출 250 KiB 와 memory 값 상한보다 긴 최종 답변. 변경 전에는 레코드가 1 MiB 를 넘어
    // 저장에 실패했다.
    let submitted = "y".repeat(250 * 1024);
    let report = json!({
        "provider": "codex", "surface_id": 7,
        "final_answer": "x".repeat(1100 * 1024), "submitted": submitted,
    });
    let t = complete(string_out, report.clone());
    assert_eq!(t.state, TaskState::Succeeded);
    let typed = t.typed_result.as_ref().unwrap();
    assert_eq!(typed.output.to_wire(), json!(submitted));
    assert!(typed.raw.execution.is_none());
    let head = typed.raw.execution_truncated.as_ref().expect("truncated");
    let full = report.to_string();
    let text = head.text.as_deref().expect("text");
    assert!(head.truncated && head.response.is_none());
    assert!(text.len() <= EXECUTION_RESPONSE_CAP && full.starts_with(text));
    assert_eq!(head.dropped_bytes as usize, full.len() - text.len());

    // 출력이 되는 최종 답변이 출력 값 상한을 넘으면 출력 검증 실패로 끝나되 저장은 된다.
    let t = complete(
        v2(),
        json!({"provider": "codex", "surface_id": 7, "final_answer": "x".repeat(1100 * 1024)}),
    );
    assert!(matches!(t.state, TaskState::Failed { .. }), "{:?}", t.state);
    let typed = t.typed_result.as_ref().unwrap();
    assert_eq!(
        typed.error.as_ref().unwrap().stage,
        FailureStage::OutputValidation
    );
    assert!(typed.raw.execution_truncated.as_ref().unwrap().truncated);
}

/// 후처리 입력으로 쓸 실행 보고가 값 상한을 넘으면 자르지 않고, 후처리 없이 출력 검증 실패로
/// 끝낸다. 회차에 보고를 저장하지 않으므로 memory 값 상한을 넘는 보고도 저장 실패가 없다.
#[test]
fn an_agent_report_too_large_for_the_postprocess_input_fails_without_running_it() {
    let (_td, mut mem, seq) = fresh_store();
    let mut store = TaskStore::new(&mut mem, "host", &seq);
    let c = contract(json!({
        "contract_version": 2,
        "output_schema": {"type": "boolean"},
        "postprocess": {"command": ["judge"], "timeout_ms": 1000},
    }));
    let t = store.create_typed(opts(agent_cmd("codex")), c).unwrap();
    let attempt = start(&mut store, &t.id);
    let mut d = done(json!({
        "provider": "codex", "surface_id": 7, "final_answer": "x".repeat(1100 * 1024),
    }));
    d.attempt_id = Some(attempt);
    let t = store.complete(1, &t.id, d, 2).expect("stored").task;
    assert!(matches!(t.state, TaskState::Failed { .. }), "{:?}", t.state);
    assert!(t.attempt.as_ref().unwrap().postprocess.is_none());
    let typed = t.typed_result.as_ref().unwrap();
    assert_eq!(
        typed.error.as_ref().unwrap().stage,
        FailureStage::OutputValidation
    );
    assert!(typed.raw.execution_truncated.as_ref().unwrap().truncated);
    assert_eq!(store.get(1, &t.id).unwrap().as_ref(), Some(&t));
}
