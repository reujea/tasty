//! v2 입력 binding 의 실행 경계 — 그래프 활성화 전 tick, 입력 snapshot, argv·stdin·params 매핑.

use serde_json::{Value, json};
use tasty_agent::TaskState;
use tasty_agent::runner::RunnerLoop;
use tasty_agent::task::contract::FailureStage;
use tasty_agent::task::{TaskGraphSpec, TaskStore};

use super::tests::fresh_ctx;
use super::*;

fn spec(v: Value) -> TaskGraphSpec {
    serde_json::from_value(v).expect("graph spec")
}

fn store_op<R>(ctx: &RunnerContext, f: impl FnOnce(&mut TaskStore) -> R) -> R {
    ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        f(&mut store)
    })
}

fn get(ctx: &RunnerContext, id: &str) -> Task {
    store_op(ctx, |s| s.get(1, &id.to_string()).unwrap().expect("task"))
}

/// 러너 스레드의 한 tick 과 같은 일을 한다. 저장소 snapshot 을 읽고 상태·결과를 기록한다.
fn tick(ctx: &RunnerContext, runner: &mut RunnerLoop<HostExecutor>, now: u64) {
    let snapshot = store_op(ctx, |s| s.list(1).unwrap());
    let set_ctx = ctx.clone();
    let res_ctx = ctx.clone();
    runner.tick(
        1,
        now,
        &snapshot,
        move |ws, id, st, n| store_op(&set_ctx, |s| s.set_state(ws, id, st, n).map(|_| ())),
        move |ws, id, c, n| store_op(&res_ctx, |s| s.complete(ws, id, c, n).map(|_| ())),
    );
}

fn tick_until_terminal(ctx: &RunnerContext, runner: &mut RunnerLoop<HostExecutor>, id: &str) {
    for n in 0..200 {
        tick(ctx, runner, 10 + n);
        if get(ctx, id).state.is_terminal() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // 마지막 sleep 뒤 러너를 더 돌리지 않고 한 번 더 읽는다.
    let state = get(ctx, id).state;
    assert!(state.is_terminal(), "{id} did not finish: {state:?}");
}

fn finish(ctx: &RunnerContext, id: &str, output: Value) {
    store_op(ctx, |s| {
        let id = id.to_string();
        s.set_state(1, &id, TaskState::Running, 1).unwrap();
        s.set_result(
            1,
            &id,
            TaskResult {
                exit_code: None,
                output: Some(output),
                error: None,
            },
        )
        .unwrap();
        s.set_state(1, &id, TaskState::Succeeded, 2).unwrap();
    });
}

#[cfg(unix)]
#[test]
fn a_tick_between_staging_and_activation_dispatches_nothing() {
    let (_td, ctx) = fresh_ctx();
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "first", "command": {"kind": "run", "command": ["true"], "workspace_id": 1}},
        {"id": "second", "command": {"kind": "run", "command": ["true"], "workspace_id": 1},
         "depends_on": ["first"]}
    ]});
    let plan = store_op(&ctx, |s| {
        let plan = s.plan_graph(1, spec(graph), 0).unwrap();
        s.stage_graph(&plan).unwrap();
        plan
    });
    // 저장은 끝났지만 활성화 전이다. 실제 tick 이 돌아도 아무것도 실행되지 않는다.
    for n in 0..3 {
        tick(&ctx, &mut runner, n);
    }
    assert_eq!(get(&ctx, "first").state, TaskState::Waiting);
    assert_eq!(get(&ctx, "first").started_at, None);

    store_op(&ctx, |s| s.activate_graph(&plan, 5).unwrap());
    tick_until_terminal(&ctx, &mut runner, "second");
    assert_eq!(get(&ctx, "first").state, TaskState::Succeeded);
    assert_eq!(get(&ctx, "second").state, TaskState::Succeeded);
}

#[cfg(unix)]
#[test]
fn run_inputs_reach_argv_and_stdin_as_values_without_reinterpretation() {
    let (_td, ctx) = fresh_ctx();
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    let tricky = "${task.p.output} $(echo injected) ${lease.resource}";
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "p", "command": {"kind": "custom", "ipc_method": "system.ping"},
         "output_schema": {"type": "object", "fields": {"msg": {"type": "string"}}}},
        {"id": "c",
         "command": {"kind": "run", "workspace_id": 1,
             "command": ["sh", "-c", "printf '%s|' \"$@\"; cat", "sh"]},
         "input_schema": {"type": "object", "fields": {
             "text": {"type": "string"}, "n": {"type": "int64"}}},
         "bindings": {"text": {"from_task": "p", "pointer": "/msg"},
                      "n": {"literal": "9007199254740993"}},
         "input_mapping": {"args": ["/text", "/n"], "stdin": true}}
    ]});
    store_op(&ctx, |s| s.submit_graph(1, spec(graph), 0).unwrap());
    finish(&ctx, "p", json!({"msg": tricky}));
    tick_until_terminal(&ctx, &mut runner, "c");

    let c = get(&ctx, "c");
    assert_eq!(c.state, TaskState::Succeeded, "{:?}", c.result);
    let stdout = c
        .typed_result
        .as_ref()
        .unwrap()
        .raw
        .execution
        .as_ref()
        .unwrap()["stdout"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    let wire = format!(r#"{{"n":"9007199254740993","text":{}}}"#, json!(tricky));
    assert_eq!(stdout, format!("{tricky}|9007199254740993|{wire}"));

    // 원본 command 는 그대로이고 해석한 실행값은 snapshot 에 따로 있다.
    let TaskCommand::Run { command, .. } = &c.command else {
        panic!("run");
    };
    assert_eq!(command.len(), 4);
    let snap = c.input_snapshot.expect("snapshot");
    assert_eq!(
        snap.execution.args,
        vec![tricky.to_string(), "9007199254740993".into()]
    );
    assert!(snap.execution.stdin);
    assert_eq!(snap.sources[0].from_task, "p");
}

#[test]
fn custom_params_receive_typed_values_at_declared_pointers() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "c", "command": {"kind": "custom", "ipc_method": "system.ping",
             "params": {"keep": "$(not run)", "nested": {}}},
         "input_schema": {"type": "object", "fields": {
             "n": {"type": "int64"}, "rows": {"type": "list", "items": {"type": "string"}}}},
         "bindings": {"n": {"literal": 9007199254740993_i64},
                      "rows": {"literal": ["a", "$(b)"]}},
         "input_mapping": {"params": {"/surface": "/n", "/nested/rows": "/rows"}}}
    ]});
    store_op(&ctx, |s| s.submit_graph(1, spec(graph), 0).unwrap());
    let c = get(&ctx, "c");
    // injector 가 없어 IPC 는 실패하지만 입력 해석과 저장은 그 전에 끝난다.
    match exec.dispatch(&c) {
        DispatchOutcome::PermanentFail(e) => assert!(e.contains(INJECTOR_UNINIT_MSG), "{e}"),
        other => panic!("expected an injector failure, got {other:?}"),
    }
    let snap = get(&ctx, "c").input_snapshot.expect("snapshot");
    assert_eq!(
        snap.execution.params,
        Some(
            json!({"keep": "$(not run)", "nested": {"rows": ["a", "$(b)"]},
                    "surface": 9007199254740993_i64})
        )
    );
}

#[test]
fn an_unresolvable_input_fails_at_the_input_stage_without_running() {
    let (_td, ctx) = fresh_ctx();
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "p", "command": {"kind": "custom", "ipc_method": "system.ping"}},
        {"id": "c",
         "command": {"kind": "run", "workspace_id": 1, "command": ["touch", "should-not-exist"]},
         "input_schema": {"type": "object", "fields": {"n": {"type": "int64"}}},
         "bindings": {"n": {"from_task": "p", "pointer": "/count", "convert": "assert"}},
         "input_mapping": {"args": ["/n"]}}
    ]});
    store_op(&ctx, |s| s.submit_graph(1, spec(graph), 0).unwrap());
    // json 출력의 assert 는 실행 시 검증한다. 값이 없으면 실행하지 않는다.
    finish(&ctx, "p", json!({"other": 1}));
    tick_until_terminal(&ctx, &mut runner, "c");
    let c = get(&ctx, "c");
    assert!(matches!(c.state, TaskState::Failed { .. }), "{:?}", c.state);
    let err = c.typed_result.unwrap().error.unwrap();
    assert_eq!(err.stage, FailureStage::Input);
    assert_eq!(err.location.as_deref(), Some("/bindings/n"));
    assert!(err.message.contains("/count"), "{}", err.message);
    assert!(c.input_snapshot.unwrap().failure.is_some());
}

/// 입력에서 받은 이름의 barrier 를 기다린다. 그 barrier 가 닫혀야 끝난다.
#[test]
fn a_wait_barrier_waits_on_the_barrier_named_by_its_input() {
    use tasty_agent::BarrierStore;
    let (_td, ctx) = fresh_ctx();
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "p", "command": {"kind": "custom", "ipc_method": "system.ping"},
         "output_schema": {"type": "object", "fields": {"gate": {"type": "string"}}}},
        {"id": "w", "command": {"kind": "wait_barrier"},
         "input_schema": {"type": "object", "fields": {"gate": {"type": "string"}}},
         "bindings": {"gate": {"from_task": "p", "pointer": "/gate"}},
         "input_mapping": {"barrier": "/gate"}}
    ]});
    store_op(&ctx, |s| s.submit_graph(1, spec(graph), 0).unwrap());
    ctx.with_store(|mem| {
        let mut b = BarrierStore::new(mem, HOST_OWNER);
        b.create(1, "gate.a", 1, None, 0).unwrap();
        b.create(1, "gate.b", 1, None, 0).unwrap();
    });
    finish(&ctx, "p", json!({"gate": "gate.b"}));
    for n in 0..5 {
        tick(&ctx, &mut runner, 10 + n);
    }
    let w = get(&ctx, "w");
    assert_eq!(w.state, TaskState::Running);
    assert_eq!(w.barrier_name(), Some("gate.b"));
    // 다른 barrier 가 닫혀도 끝나지 않는다.
    ctx.with_store(|mem| BarrierStore::new(mem, HOST_OWNER).signal(1, "gate.a", 20))
        .unwrap();
    for n in 0..5 {
        tick(&ctx, &mut runner, 30 + n);
    }
    assert_eq!(get(&ctx, "w").state, TaskState::Running);
    ctx.with_store(|mem| BarrierStore::new(mem, HOST_OWNER).signal(1, "gate.b", 40))
        .unwrap();
    tick_until_terminal(&ctx, &mut runner, "w");
    let w = get(&ctx, "w");
    assert_eq!(w.state, TaskState::Succeeded, "{:?}", w.result);
    let raw = w.typed_result.unwrap().raw;
    assert_eq!(raw.execution.unwrap()["barrier"], json!("gate.b"));
    assert_eq!(
        w.input_snapshot.unwrap().execution.barrier.as_deref(),
        Some("gate.b")
    );
}

#[cfg(unix)]
fn run_with_file_arg(path: &str, cwd: Option<&std::path::Path>, marker: &std::path::Path) -> Value {
    let mut command = json!({"kind": "run", "workspace_id": 1,
        "command": ["sh", "-c", "touch \"$1\"; cat \"$2\"", "sh", marker]});
    if let Some(cwd) = cwd {
        command["cwd"] = json!(cwd);
    }
    json!({"id": "r", "command": command,
        "input_schema": {"type": "object", "fields": {"path": {"type": "string"}}},
        "bindings": {"path": {"literal": path}},
        "input_mapping": {"args": [{"file": "/path"}]}})
}

/// 이미 있는 파일의 경로를 argv 로 넘긴다. 실행 직전에 읽을 수 있는지만 보고 내용은 프로그램이
/// 읽는다. 없거나 읽을 수 없는 경로는 실행하지 않고 input 단계로 실패한다.
#[cfg(unix)]
#[test]
fn a_file_argument_passes_an_existing_readable_path_and_refuses_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("in.txt");
    std::fs::write(&data, "file body").unwrap();
    let marker = dir.path().join("ran");
    let cases: Vec<(&str, Value, Option<&str>)> = vec![
        (
            "abs",
            run_with_file_arg(data.to_str().unwrap(), None, &marker),
            None,
        ),
        (
            "rel",
            run_with_file_arg("in.txt", Some(dir.path()), &marker),
            None,
        ),
        (
            "missing",
            run_with_file_arg(dir.path().join("nope").to_str().unwrap(), None, &marker),
            Some("cannot be read"),
        ),
        (
            "dir",
            run_with_file_arg(dir.path().to_str().unwrap(), None, &marker),
            Some("not a regular file"),
        ),
        (
            "rel-no-cwd",
            run_with_file_arg("in.txt", None, &marker),
            Some("relative path but the run has no cwd"),
        ),
    ];
    for (label, task, refusal) in cases {
        let (_td, ctx) = fresh_ctx();
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let graph = json!({"contract_version": 2, "tasks": [task]});
        store_op(&ctx, |s| s.submit_graph(1, spec(graph), 0).unwrap());
        tick_until_terminal(&ctx, &mut runner, "r");
        let r = get(&ctx, "r");
        let typed = r.typed_result.clone().unwrap();
        match refusal {
            None => {
                assert_eq!(r.state, TaskState::Succeeded, "{label}: {:?}", r.result);
                let out = &typed.raw.execution.unwrap()["stdout"]["text"];
                assert_eq!(out, &json!("file body"), "{label}");
                let args = &r.input_snapshot.unwrap().execution.args;
                assert_eq!(args.len(), 1, "{label}");
            }
            Some(why) => {
                assert!(matches!(r.state, TaskState::Failed { .. }), "{label}");
                let err = typed.error.unwrap();
                assert_eq!(err.stage, FailureStage::Input, "{label}");
                assert_eq!(
                    err.location.as_deref(),
                    Some("/input_mapping/args/0/file"),
                    "{label}"
                );
                assert!(err.message.contains(why), "{label}: {}", err.message);
                assert!(!marker.exists(), "{label}: the run must not start");
            }
        }
    }
    // Tasty 는 파일을 만들거나 지우지 않는다.
    assert_eq!(std::fs::read_to_string(&data).unwrap(), "file body");
}
