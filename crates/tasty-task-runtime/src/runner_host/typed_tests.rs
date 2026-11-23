//! v2 계약 task 의 실행 경계 — Run 허용 종료 코드, v2 reduce, custom 접수 응답.

use serde_json::{Value, json};
use tasty_agent::task::contract::TaskContract;
use tasty_agent::task::{TaskCreateOpts, TaskStore};
use tasty_agent::{OnFailure, ReducerStrategy, TaskState};

use super::tests::fresh_ctx;
use super::*;

fn contract(v: Value) -> TaskContract {
    serde_json::from_value(v).expect("contract")
}

fn opts(name: &str, command: TaskCommand) -> TaskCreateOpts {
    TaskCreateOpts {
        workspace_id: 1,
        name: name.into(),
        command,
        depends_on: vec![],
        on_failure: OnFailure::Abort,
        metadata: Value::Null,
        now_ms: 0,
    }
}

#[cfg(unix)]
fn wait_outcome(exec: &mut HostExecutor, handle: &DispatchHandle) -> PollOutcome {
    for _ in 0..100 {
        match exec.poll(handle) {
            PollOutcome::Active => std::thread::sleep(Duration::from_millis(50)),
            other => return other,
        }
    }
    // 마지막 sleep 뒤 한 번 더 거둔다. poll 은 끝난 run 의 결과를 거두기만 한다.
    match exec.poll(handle) {
        PollOutcome::Active => panic!("run did not finish"),
        other => other,
    }
}

fn exit_seven() -> TaskCommand {
    TaskCommand::Run {
        command: vec!["sh".into(), "-c".into(), "echo out; exit 7".into()],
        workspace_id: 1,
        cwd: None,
    }
}

#[cfg(unix)]
#[test]
fn v2_run_accepts_declared_exit_codes_and_reports_the_code_as_output() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let allowed = contract(json!({"contract_version": 2, "allowed_exit_codes": [0, 7]}));
    let strict = contract(json!({"contract_version": 2}));
    let (seven_ok, seven_strict) = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let a = store
            .create_typed(opts("allowed", exit_seven()), allowed)
            .unwrap();
        let b = store
            .create_typed(opts("strict", exit_seven()), strict)
            .unwrap();
        (a, b)
    });

    let outcome_of = |exec: &mut HostExecutor, task: &Task| match exec.dispatch(task) {
        DispatchOutcome::Started(h) => wait_outcome(exec, &h),
        other => panic!("expected Started, got {other:?}"),
    };
    let ok = outcome_of(&mut exec, &seven_ok);
    let PollOutcome::Done(result) = ok else {
        panic!("declared exit 7 should succeed: {ok:?}");
    };
    assert_eq!(result.exit_code, Some(7));
    let strict_outcome = outcome_of(&mut exec, &seven_strict);
    let PollOutcome::Exited(failed) = strict_outcome else {
        panic!("exit 7 outside the declared list fails: {strict_outcome:?}");
    };
    // 실패한 회차도 종료 코드와 출력 꼬리를 결과와 report 의 auto 에 남긴다.
    let report = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store
            .set_state(1, &seven_strict.id, TaskState::Running, 1)
            .unwrap();
        let task = store
            .complete(
                1,
                &seven_strict.id,
                tasty_agent::task::Completion::exited(None, failed),
                2,
            )
            .unwrap()
            .task;
        assert!(
            matches!(task.state, TaskState::Failed { .. }),
            "{:?}",
            task.state
        );
        let raw = &task.typed_result.as_ref().unwrap().raw;
        assert_eq!(raw.exit_code, Some(7));
        tasty_agent::task::report::project_task(&task, &[], &[], None, true)
    });
    assert_eq!(report["auto"]["exit_code"], json!(7), "{report}");
    assert_eq!(report["auto"]["raw"]["stdout"]["text"], json!("out\n"));
    assert!(
        report["auto"]["failure"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Run exited with code 7\n")
    );

    let finished = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store
            .set_state(1, &seven_ok.id, TaskState::Running, 1)
            .unwrap();
        store.set_result(1, &seven_ok.id, result).unwrap();
        store
            .set_state(1, &seven_ok.id, TaskState::Succeeded, 2)
            .unwrap()
            .0
    });
    assert_eq!(finished.state, TaskState::Succeeded);
    let typed = finished.typed_result.unwrap();
    assert_eq!(typed.output.to_internal(), json!(7));
    let raw = typed.raw.execution.unwrap();
    assert_eq!(raw["stdout"]["text"], json!("out\n"));
}

/// 두 줄기가 모두 수집 상한을 채운 실패 Run 도 레코드와 재시작용 셀이 memory 값 상한 안에
/// 들어 Failed 로 끝난다. 이스케이프로 크게 불어나는 출력(빈 줄·ANSI·바이너리·NUL)을 포함한다.
#[cfg(unix)]
#[test]
fn a_failed_run_with_full_escape_heavy_tails_still_settles() {
    use super::run_result::{CAPTURE_TAIL_CAP, FAILURE_MESSAGE_TAIL_CAP, persist_run_result};
    use super::store_keys::run_result_key;
    let cases = [
        ("plain", "yes 'hello world log line'"),
        ("blank_lines", "yes ''"),
        ("ansi", "yes \"$(printf '\\033[31mE\\033[0m')\""),
        ("binary", "cat /dev/urandom"),
        ("nul", "cat /dev/zero"),
    ];
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    for (name, generator) in cases {
        let gen_cmd = format!("{generator} | head -c 200000");
        let script = format!("{gen_cmd}; {gen_cmd} >&2; exit 3");
        let task = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            let cmd = TaskCommand::Run {
                command: vec!["sh".into(), "-c".into(), script.clone()],
                workspace_id: 1,
                cwd: None,
            };
            store
                .create_typed(opts(name, cmd), contract(json!({"contract_version": 2})))
                .unwrap()
        });
        let outcome = match exec.dispatch(&task) {
            DispatchOutcome::Started(h) => wait_outcome(&mut exec, &h),
            other => panic!("{name}: expected Started, got {other:?}"),
        };
        let PollOutcome::Exited(result) = &outcome else {
            panic!("{name}: expected Exited, got {outcome:?}");
        };
        let output = result.output.as_ref().unwrap();
        assert!(
            output["stdout"]["truncated"].as_bool().unwrap()
                && output["stderr"]["truncated"].as_bool().unwrap(),
            "{name}: both streams fill the {CAPTURE_TAIL_CAP} byte capture"
        );
        let error = result.error.as_deref().unwrap();
        assert!(
            error.len() <= 2 * FAILURE_MESSAGE_TAIL_CAP + 256,
            "{name}: message {} bytes",
            error.len()
        );
        // 재시작용 셀이 저장된다.
        persist_run_result(&ctx.memory, 1, &task.id, &outcome);
        let stored = ctx.with_store(|mem| {
            mem.get(
                &tasty_memory::Scope::Workspace(1),
                &run_result_key(&task.id),
            )
            .unwrap()
        });
        assert!(stored.is_some(), "{name}: run_result cell stored");
        let settled = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            store.set_state(1, &task.id, TaskState::Running, 1).unwrap();
            store.complete(
                1,
                &task.id,
                tasty_agent::task::Completion::exited(None, result.clone()),
                2,
            )
        });
        let task = settled
            .unwrap_or_else(|e| panic!("{name}: completion stored: {e}"))
            .task;
        assert!(matches!(task.state, TaskState::Failed { .. }), "{name}");
        assert_eq!(task.typed_result.unwrap().raw.exit_code, Some(3), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn v1_run_keeps_treating_nonzero_exit_as_failure() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let task = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        store.create(opts("v1", exit_seven())).unwrap()
    });
    let outcome = match exec.dispatch(&task) {
        DispatchOutcome::Started(h) => wait_outcome(&mut exec, &h),
        other => panic!("expected Started, got {other:?}"),
    };
    assert!(matches!(outcome, PollOutcome::Exited(_)), "{outcome:?}");
}

fn finished_custom(store: &mut TaskStore, name: &str, c: Option<TaskContract>, out: Value) -> Task {
    let cmd = TaskCommand::Custom {
        ipc_method: "system.ping".into(),
        params: Value::Null,
        poll: None,
    };
    let t = match c {
        Some(c) => store.create_typed(opts(name, cmd), c).unwrap(),
        None => store.create(opts(name, cmd)).unwrap(),
    };
    store.set_state(1, &t.id, TaskState::Running, 1).unwrap();
    store
        .set_result(
            1,
            &t.id,
            TaskResult {
                exit_code: None,
                output: Some(out),
                error: None,
            },
        )
        .unwrap();
    store
        .set_state(1, &t.id, TaskState::Succeeded, 2)
        .unwrap()
        .0
}

#[test]
fn v2_reduce_dispatch_uses_typed_inputs_and_v1_reduce_is_unchanged() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let text = contract(json!({"contract_version": 2, "output_schema": {"type": "string"}}));
    let (typed_all, typed_concat, legacy_all) = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let a = finished_custom(&mut store, "a", Some(text.clone()), json!("x"));
        let b = finished_custom(&mut store, "b", Some(text.clone()), json!("y"));
        let ids = vec![a.id.clone(), b.id.clone()];
        let reduce = |strategy| TaskCommand::Reduce {
            inputs: ids.clone(),
            strategy,
        };
        let v2 = contract(json!({"contract_version": 2}));
        (
            store
                .create_typed(opts("all", reduce(ReducerStrategy::All)), v2.clone())
                .unwrap(),
            store
                .create_typed(opts("concat", reduce(ReducerStrategy::ConcatText)), v2)
                .unwrap(),
            {
                // v1 reduce 는 v1 입력만 받는다.
                let c = finished_custom(&mut store, "c", None, json!("x"));
                let d = finished_custom(&mut store, "d", None, json!("y"));
                let legacy = TaskCommand::Reduce {
                    inputs: vec![c.id, d.id],
                    strategy: ReducerStrategy::All,
                };
                store.create(opts("legacy", legacy)).unwrap()
            },
        )
    });
    let immediate = |exec: &mut HostExecutor, t: &Task| match exec.dispatch(t) {
        DispatchOutcome::Started(DispatchHandle::ReduceImmediate(r)) => r,
        other => panic!("expected ReduceImmediate, got {other:?}"),
    };
    let all = immediate(&mut exec, &typed_all);
    assert_eq!(all.exit_code, None);
    let records = all.output.unwrap();
    assert_eq!(records[0]["has_output"], json!(true));
    assert_eq!(records[0]["output"], json!("x"));
    assert_eq!(records[1]["state"], json!("succeeded"));
    assert_eq!(
        immediate(&mut exec, &typed_concat).output,
        Some(json!("xy"))
    );
    // v1 reduce all 은 출력 값만 담은 배열이다.
    assert_eq!(
        immediate(&mut exec, &legacy_all).output,
        Some(json!(["x", "y"]))
    );
}

fn ref_to(id: &str) -> TaskCommand {
    TaskCommand::Custom {
        ipc_method: "system.ping".into(),
        params: json!({ "x": format!("${{task.{id}.output}}") }),
        poll: None,
    }
}

#[test]
fn output_placeholders_cannot_read_typed_results() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let (typed, legacy, stored_ref) = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let v2 = contract(json!({"contract_version": 2}));
        let typed = store.create_typed(opts("v2run", exit_seven()), v2).unwrap();
        let legacy = finished_custom(&mut store, "v1", None, json!({"id": 1}));
        // 생성 검사를 거치지 않고 저장된 참조도 실행 직전에 막히는지 본다.
        let mut o = opts("stored", ref_to(&typed.id));
        o.depends_on = vec![typed.id.clone()];
        let stored_ref = store.create(o).unwrap();
        (typed, legacy, stored_ref)
    });

    let err = ctx
        .with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            crate::task::reject_v1_reads_of_typed(&store, 1, &ref_to(&typed.id), &OnFailure::Abort)
        })
        .unwrap_err();
    let tasty_agent::AgentError::TypeContract(f) = err else {
        panic!("expected a type contract error");
    };
    assert_eq!(f.task_id.as_deref(), Some(typed.id.as_str()));
    let ok = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        crate::task::reject_v1_reads_of_typed(&store, 1, &ref_to(&legacy.id), &OnFailure::Abort)
    });
    assert!(ok.is_ok(), "v1 producers stay readable: {ok:?}");

    match exec.dispatch(&stored_ref) {
        DispatchOutcome::PermanentFail(e) => assert!(e.contains("typed v2"), "{e}"),
        other => panic!("expected PermanentFail, got {other:?}"),
    }
}

#[test]
fn v1_reduce_cannot_take_typed_inputs() {
    let (_td, ctx) = fresh_ctx();
    let mut exec = HostExecutor::new(ctx.clone());
    let (typed, legacy, stored_reduce) = ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        let mut store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
        let v2 = contract(json!({"contract_version": 2}));
        let typed = finished_custom(&mut store, "v2", Some(v2), json!(1));
        let legacy = finished_custom(&mut store, "v1", None, json!(2));
        // 생성 검사를 거치지 않고 저장된 v1 reduce 도 실행 직전에 막히는지 본다.
        let reduce = TaskCommand::Reduce {
            inputs: vec![legacy.id.clone(), typed.id.clone()],
            strategy: ReducerStrategy::All,
        };
        let stored_reduce = store.create(opts("stored", reduce)).unwrap();
        (typed, legacy, stored_reduce)
    });
    let check = |inputs: Vec<String>| {
        ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            let store = TaskStore::new(mem, HOST_OWNER, seq.as_ref());
            crate::task::reject_v1_reads_of_typed(
                &store,
                1,
                &TaskCommand::Reduce {
                    inputs,
                    strategy: ReducerStrategy::All,
                },
                &OnFailure::Abort,
            )
        })
    };
    let err = check(vec![legacy.id.clone(), typed.id.clone()]).unwrap_err();
    let tasty_agent::AgentError::TypeContract(f) = err else {
        panic!("expected a type contract error");
    };
    assert_eq!(f.task_id.as_deref(), Some(typed.id.as_str()));
    assert!(check(vec![legacy.id.clone()]).is_ok());

    match exec.dispatch(&stored_reduce) {
        DispatchOutcome::PermanentFail(e) => assert!(e.contains("v1 reduce"), "{e}"),
        other => panic!("expected PermanentFail, got {other:?}"),
    }
}

/// 완료를 따로 기다리는 v2 custom 은 dispatch 응답을 `raw.accepted` 에, 완료를 알린 poll 응답을
/// `raw.execution` 에 둔다. 너무 큰 접수 응답은 앞부분만 남기고 잘렸다고 표시한다.
#[test]
fn an_async_custom_keeps_its_accepted_response_apart_from_the_final_one() {
    use super::scripted_ipc::{self, Step};
    use tasty_agent::runner::RunnerLoop;
    use tasty_agent::task::TaskGraphSpec;
    use tasty_agent::task::contract::ACCEPTED_RESPONSE_CAP;

    let (_td, ctx) = fresh_ctx();
    let big = "가".repeat(ACCEPTED_RESPONSE_CAP);
    let big_reply = big.clone();
    let _ipc = scripted_ipc::install_responder(&ctx, move |cmd| {
        Step::answer(
            match (cmd.request.method.as_str(), cmd.request.params.get("job")) {
                ("fake.start", _) => match cmd.request.params.get("big") {
                    Some(_) => json!({"job": "J2", "blob": big_reply}),
                    None => json!({"job": "J1"}),
                },
                ("fake.poll", Some(job)) => json!({"state": "done", "job": job}),
                ("system.ping", _) => json!({"pong": true}),
                other => panic!("unexpected call {other:?}"),
            },
        )
    });
    let poll = json!({"poll_method": "fake.poll", "map_from_response": {"job": "job"},
        "state_field": "state", "terminal_states": ["done"], "interval_ms": 1});
    let graph = json!({"contract_version": 2, "tasks": [
        {"id": "small", "command": {"kind": "custom", "ipc_method": "fake.start", "poll": poll}},
        {"id": "large", "command": {"kind": "custom", "ipc_method": "fake.start",
            "params": {"big": true}, "poll": poll}},
        {"id": "now", "command": {"kind": "custom", "ipc_method": "system.ping"}}
    ]});
    let spec: TaskGraphSpec = serde_json::from_value(graph).expect("graph spec");
    ctx.with_store(|mem| {
        let seq = ctx.agent_seq.clone();
        TaskStore::new(mem, HOST_OWNER, seq.as_ref())
            .submit_graph(1, spec, 0)
            .unwrap();
    });
    let get = |id: &str| {
        ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            TaskStore::new(mem, HOST_OWNER, seq.as_ref())
                .get(1, &id.to_string())
                .unwrap()
                .expect("task")
        })
    };
    let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
    for n in 0..200 {
        let snapshot = ctx.with_store(|mem| {
            let seq = ctx.agent_seq.clone();
            TaskStore::new(mem, HOST_OWNER, seq.as_ref())
                .list(1)
                .unwrap()
        });
        let (set_ctx, res_ctx) = (ctx.clone(), ctx.clone());
        runner.tick(
            1,
            10 + n,
            &snapshot,
            move |ws, id, st, now| {
                set_ctx.with_store(|mem| {
                    let seq = set_ctx.agent_seq.clone();
                    TaskStore::new(mem, HOST_OWNER, seq.as_ref())
                        .set_state(ws, id, st, now)
                        .map(|_| ())
                })
            },
            move |ws, id, c, now| {
                res_ctx.with_store(|mem| {
                    let seq = res_ctx.agent_seq.clone();
                    TaskStore::new(mem, HOST_OWNER, seq.as_ref())
                        .complete(ws, id, c, now)
                        .map(|_| ())
                })
            },
        );
        if ["small", "large", "now"]
            .iter()
            .all(|id| get(id).state.is_terminal())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let small = get("small");
    assert_eq!(small.state, TaskState::Succeeded, "{:?}", small.result);
    // 확정하면 접수 응답은 결과로 옮겨 task 쪽에는 남지 않는다.
    assert!(small.accepted.is_none());
    let raw = &small.typed_result.as_ref().unwrap().raw;
    let accepted = raw.accepted.as_ref().expect("accepted response");
    assert_eq!(accepted.response, Some(json!({"job": "J1"})));
    assert!(!accepted.truncated);
    assert_eq!(raw.execution, Some(json!({"state": "done", "job": "J1"})));

    let large = get("large");
    assert_eq!(large.state, TaskState::Succeeded, "{:?}", large.result);
    let accepted = large.typed_result.unwrap().raw.accepted.expect("accepted");
    let full = json!({"job": "J2", "blob": big}).to_string();
    let text = accepted.text.expect("truncated text");
    assert!(accepted.truncated && accepted.response.is_none());
    assert!(text.len() <= ACCEPTED_RESPONSE_CAP, "{}", text.len());
    assert!(full.starts_with(&text));
    assert_eq!(accepted.dropped_bytes as usize, full.len() - text.len());

    // 응답으로 바로 끝나는 custom 은 그 응답이 결과라 접수 응답을 따로 두지 않는다.
    let now = get("now");
    assert_eq!(now.state, TaskState::Succeeded, "{:?}", now.result);
    assert!(now.accepted.is_none());
    let raw = now.typed_result.unwrap().raw;
    assert!(raw.accepted.is_none());
    assert_eq!(raw.execution, Some(json!({"pong": true})));
}
