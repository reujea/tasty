//! 실제 프로세스로 후처리 실행을 확인한다. 셸 명령을 쓰므로 Unix 에서만 실행한다.
#![cfg(unix)]

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tasty_agent::task::postprocess::{
    PostprocessCause, PostprocessOutcome, PostprocessReport, StdoutFormat, StdoutSpec,
};

use super::process::{ProcessRequest, STOP_CANCELLED, STOP_NONE, spawn};
use tasty_agent::child_env::child_env;

fn json_out() -> StdoutSpec {
    StdoutSpec::default()
}

fn run_with(script: &str, stdin: Vec<u8>, timeout_ms: u64, cancel: &AtomicU8) -> PostprocessReport {
    let req = ProcessRequest {
        command: vec!["sh".into(), "-c".into(), script.into()],
        cwd: None,
        stdin,
        stdout: json_out(),
        timeout: Duration::from_millis(timeout_ms),
        run: 1,
        env: child_env(std::env::vars_os()),
    };
    match spawn(req) {
        Ok(started) => started.wait(cancel),
        Err(report) => report,
    }
}

fn run(script: &str) -> PostprocessReport {
    run_with(script, b"{}".to_vec(), 10_000, &AtomicU8::new(STOP_NONE))
}

fn stdout_of(r: &PostprocessReport) -> Value {
    match &r.outcome {
        PostprocessOutcome::Collected { stdout } => stdout.clone(),
        other => panic!(
            "expected collected stdout, got {other:?} (stderr {:?})",
            r.stderr
        ),
    }
}

fn cause_of(r: &PostprocessReport) -> PostprocessCause {
    r.cause()
        .unwrap_or_else(|| panic!("expected a failure, got {r:?}"))
}

#[test]
fn values_on_stdout_are_collected_and_stderr_stays_a_log() {
    for (printed, want) in [
        ("'\"revise\"'", json!("revise")),
        ("false", json!(false)),
        ("0", json!(0)),
        (
            "'{\"verdict\":\"pass\",\"n\":0}'",
            json!({"verdict": "pass", "n": 0}),
        ),
    ] {
        let r = run(&format!(
            "cat >/dev/null; echo progress >&2; printf %s {printed}"
        ));
        assert_eq!(stdout_of(&r), want, "{printed}");
        assert_eq!(r.exit_code, Some(0));
        assert_eq!(r.stderr.as_deref(), Some("progress\n"));
    }
}

#[test]
fn the_stdin_document_reaches_the_command_and_is_closed() {
    // cat 은 EOF 를 받아야 끝난다.
    let doc = json!({"draft": "hello", "n": 0});
    let r = run_with(
        "cat",
        doc.to_string().into_bytes(),
        10_000,
        &AtomicU8::new(STOP_NONE),
    );
    assert_eq!(stdout_of(&r), doc);
}

/// 입력을 읽지 않고 끝나는 명령도 정상이다. 파이프 버퍼보다 큰 입력이라 쓰기는 반드시 파이프
/// 끊김으로 끝나지만 실패가 아니며 결과는 종료 상태와 stdout 으로 정한다.
#[test]
fn a_command_that_never_reads_a_large_stdin_is_judged_by_its_exit() {
    let big = json!({"draft": "x".repeat(4 * 1024 * 1024)})
        .to_string()
        .into_bytes();
    let r = run_with(
        "printf true",
        big.clone(),
        10_000,
        &AtomicU8::new(STOP_NONE),
    );
    assert_eq!(stdout_of(&r), json!(true));
    let r = run_with("exit 3", big, 10_000, &AtomicU8::new(STOP_NONE));
    assert_eq!(cause_of(&r), PostprocessCause::NonzeroExit);
}

#[test]
fn failures_are_reported_with_distinct_causes() {
    let r = run("echo why >&2; exit 3");
    assert_eq!(cause_of(&r), PostprocessCause::NonzeroExit);
    assert_eq!(r.exit_code, Some(3));
    assert_eq!(r.stderr.as_deref(), Some("why\n"));
    assert_eq!(cause_of(&run("printf pass")), PostprocessCause::InvalidJson);
    assert_eq!(
        cause_of(&run("printf '1 2'")),
        PostprocessCause::MultipleDocuments
    );
    assert_eq!(
        cause_of(&run("printf '\\377'")),
        PostprocessCause::InvalidUtf8
    );
    assert_eq!(cause_of(&run("true")), PostprocessCause::EmptyOutput);
    assert_eq!(cause_of(&run("kill -9 $$")), PostprocessCause::Signal);
    let missing = spawn(ProcessRequest {
        command: vec!["/nonexistent/tasty-postprocess-cli".into()],
        cwd: None,
        stdin: Vec::new(),
        stdout: json_out(),
        timeout: Duration::from_secs(5),
        run: 2,
        env: child_env(std::env::vars_os()),
    })
    .err()
    .expect("spawn failure");
    assert_eq!(cause_of(&missing), PostprocessCause::Spawn);
    assert_eq!(missing.run, 2);
}

#[test]
fn oversized_stdout_fails_even_when_it_starts_as_valid_json() {
    // 앞부분은 올바른 JSON 문자열이고 상한(256 KiB)을 넘긴다. 잘린 값으로 성공하지 않는다.
    let r = run("printf '\"'; head -c 400000 /dev/zero | tr '\\0' a; printf '\"'");
    assert_eq!(cause_of(&r), PostprocessCause::StdoutTooLarge);
    // 상한 안의 큰 값은 그대로 수집한다.
    let r = run("printf '\"'; head -c 200000 /dev/zero | tr '\\0' a; printf '\"'");
    assert_eq!(stdout_of(&r).as_str().map(str::len), Some(200_000));
}

#[test]
fn huge_stderr_keeps_only_the_tail_and_does_not_block() {
    let r = run("head -c 5000000 /dev/zero | tr '\\0' e >&2; echo END >&2; printf 1");
    assert_eq!(stdout_of(&r), json!(1));
    assert!(r.stderr_truncated);
    let tail = r.stderr.expect("stderr");
    assert!(tail.len() <= tasty_agent::task::postprocess::POSTPROCESS_STDERR_TAIL_BYTES);
    assert!(tail.ends_with("END\n"));
}

#[test]
fn a_command_that_ignores_a_large_stdin_still_finishes() {
    let stdin = json!({"blob": "x".repeat(2 * 1024 * 1024)})
        .to_string()
        .into_bytes();
    let r = run_with("printf true", stdin, 10_000, &AtomicU8::new(STOP_NONE));
    assert_eq!(stdout_of(&r), json!(true));
}

#[test]
fn a_command_that_never_reads_stdin_times_out() {
    let stdin = vec![b' '; 4 * 1024 * 1024];
    let started = Instant::now();
    let r = run_with("sleep 30", stdin, 300, &AtomicU8::new(STOP_NONE));
    assert_eq!(cause_of(&r), PostprocessCause::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn endless_stdout_is_drained_until_the_timeout() {
    let started = Instant::now();
    let r = run_with("yes", Vec::new(), 300, &AtomicU8::new(STOP_NONE));
    assert_eq!(cause_of(&r), PostprocessCause::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
fn gone(pid: i32) -> bool {
    crate::process_gone_tests_support::gone(pid as u32)
}

#[cfg(target_os = "linux")]
#[test]
fn a_grandchild_holding_the_pipe_is_killed_at_the_timeout() {
    let started = Instant::now();
    let r = run_with(
        "sleep 30 & echo $! >&2; printf true",
        Vec::new(),
        500,
        &AtomicU8::new(STOP_NONE),
    );
    // 직접 자식은 바로 끝나지만 손자가 stdout 을 쥐고 있어 EOF 를 기다리다 시간이 다 된다.
    assert_eq!(cause_of(&r), PostprocessCause::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    let pid: i32 = r
        .stderr
        .expect("stderr")
        .trim()
        .parse()
        .expect("grandchild pid");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !gone(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gone(pid), "grandchild {pid} is still running");
}

#[test]
fn cancellation_stops_the_process_group() {
    let cancel = std::sync::Arc::new(AtomicU8::new(STOP_NONE));
    let flag = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        flag.store(STOP_CANCELLED, Ordering::Release);
    });
    let started = Instant::now();
    let r = run_with("sleep 30 & sleep 30", Vec::new(), 30_000, &cancel);
    trigger.join().expect("trigger");
    assert_eq!(cause_of(&r), PostprocessCause::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn text_format_keeps_stdout_verbatim() {
    let req = ProcessRequest {
        command: vec!["sh".into(), "-c".into(), "printf 'not json\\n'".into()],
        cwd: None,
        stdin: Vec::new(),
        stdout: StdoutSpec {
            format: StdoutFormat::Text,
            pointer: None,
        },
        timeout: Duration::from_secs(10),
        run: 1,
        env: child_env(std::env::vars_os()),
    };
    let r = spawn(req).expect("spawn").wait(&AtomicU8::new(STOP_NONE));
    assert_eq!(stdout_of(&r), json!("not json\n"));
}

#[test]
fn the_child_gets_the_host_environment_without_the_outer_session_identities() {
    use std::ffi::OsString;
    let os = |k: &str, v: &str| (OsString::from(k), OsString::from(v));
    // 번들 Claude 플러그인이 설치·활성인 상태처럼 선언을 공용 표에 올린다.
    tasty_test_support::strip_env_keys::install_bundled_plugin_rules();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let inherited = vec![
        (OsString::from("PATH"), path),
        os("HOME", "/home/someone"),
        // 활성 Claude 플러그인이 선언한 바깥 세션의 표지·비밀은 넘기지 않는다.
        os("CLAUDECODE", "1"),
        os("CLAUDE_CODE_SESSION_ID", "s-1"),
        os("CLAUDE_CODE_ENTRYPOINT", "cli"),
        os("CLAUDE_CODE_MESSAGING_TOKEN", "secret"),
        os("CLAUDE_PLUGIN_OPTION_X", "1"),
        os("AI_AGENT", "claude-code_2.1.291_bash"),
        // 사용자가 넣는 Claude Code 설정과 인증은 남긴다.
        os("CLAUDE_CODE_OAUTH_TOKEN", "user-token"),
        os("ANTHROPIC_API_KEY", "user-key"),
        // 바깥 Tasty 인스턴스의 신원은 넘기지 않는다.
        os("TASTY_SESSION_TOKEN", "outer-token"),
        os("TASTY_SURFACE_ID", "777"),
        os("TASTY_PARENT_HOME", "/tmp/outer-home"),
        os("TASTY_AGENT_ID", "outer-agent"),
        // 그 밖의 TASTY_* 는 그대로 넘긴다.
        os("TASTY_HOME", "/tmp/tasty-home"),
        os("TASTY_LOCALE", "ko"),
    ];
    let req = ProcessRequest {
        command: vec!["env".into()],
        cwd: None,
        stdin: Vec::new(),
        stdout: StdoutSpec {
            format: StdoutFormat::Text,
            pointer: None,
        },
        timeout: Duration::from_secs(10),
        run: 1,
        env: child_env(inherited),
    };
    let r = spawn(req).expect("spawn").wait(&AtomicU8::new(STOP_NONE));
    let out = stdout_of(&r);
    let mut seen: Vec<&str> = out
        .as_str()
        .expect("text")
        .lines()
        .filter(|l| !l.starts_with("PATH="))
        .collect();
    seen.sort_unstable();
    // 넘긴 것만 있고 Tasty 가 더한 변수는 없다.
    assert_eq!(
        seen,
        [
            "ANTHROPIC_API_KEY=user-key",
            "CLAUDE_CODE_OAUTH_TOKEN=user-token",
            "HOME=/home/someone",
            "TASTY_HOME=/tmp/tasty-home",
            "TASTY_LOCALE=ko",
        ]
    );
}

// ── executor 와 runner 를 거친 실행 ──────────────────────────────────────────

mod through_the_runner {
    use std::path::Path;

    use serde_json::{Value, json};
    use tasty_agent::runner::RunnerLoop;
    use tasty_agent::task::postprocess::{PostprocessCause, PostprocessOutcome};
    use tasty_agent::task::{TaskGraphSpec, TaskStore};
    use tasty_agent::{SemaphoreStore, TaskState};

    use super::super::super::tests::fresh_ctx;
    use super::super::super::*;

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

    fn tick_until(
        ctx: &RunnerContext,
        runner: &mut RunnerLoop<HostExecutor>,
        what: &str,
        mut done: impl FnMut(&RunnerContext, &RunnerLoop<HostExecutor>) -> bool,
    ) {
        for _ in 0..500 {
            tick(ctx, runner, now_ms());
            if done(ctx, runner) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("{what} did not happen");
    }

    fn submit(ctx: &RunnerContext, graph: Value) {
        let spec: TaskGraphSpec = serde_json::from_value(graph).expect("graph");
        store_op(ctx, |s| s.submit_graph(1, spec, 0).unwrap());
    }

    fn log_lines(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn sh(script: String) -> Value {
        json!(["sh", "-c", script])
    }

    #[test]
    fn a_retried_postprocess_runs_again_without_rerunning_the_main_work() {
        let (td, ctx) = fresh_ctx();
        let log = td.path().join("order.log");
        let log_s = log.display().to_string();
        // 후처리는 세 번째 실행에서야 성공한다. 다음 task 는 그 뒤에 실행돼야 한다.
        let pp = format!(
            "cat >/dev/null; echo pp >> '{log_s}'; n=$(grep -c '^pp$' '{log_s}'); \
             echo try $n >&2; [ $n -ge 3 ] && printf true || exit 1"
        );
        submit(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "judge",
                 "command": {"kind": "run", "workspace_id": 1,
                             "command": sh(format!("echo main >> '{log_s}'; printf draft"))},
                 "output_schema": {"type": "boolean"},
                 "postprocess": {"command": sh(pp), "timeout_ms": 10000,
                                 "retry": {"max_retries": 2}}},
                {"id": "after", "depends_on": ["judge"],
                 "command": {"kind": "run", "workspace_id": 1,
                             "command": sh(format!("echo after >> '{log_s}'"))}}
            ]}),
        );
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        tick_until(&ctx, &mut runner, "after finished", |c, _| {
            get(c, "after").state.is_terminal()
        });
        assert_eq!(log_lines(&log), ["main", "pp", "pp", "pp", "after"]);
        let judge = get(&ctx, "judge");
        assert_eq!(judge.state, TaskState::Succeeded, "{:?}", judge.result);
        let typed = judge.typed_result.expect("typed");
        assert_eq!(serde_json::to_value(&typed.output).unwrap(), json!(true));
        assert_eq!(
            typed.raw.execution.unwrap()["stdout"]["text"],
            json!("draft")
        );
        let pp = typed.raw.postprocess.expect("postprocess raw");
        assert_eq!((pp.run, pp.failed_runs.len()), (3, 2));
        assert_eq!(pp.stderr.as_deref(), Some("try 3\n"));
    }

    #[test]
    fn the_main_raw_result_reaches_the_postprocess_stdin() {
        let (_td, ctx) = fresh_ctx();
        submit(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "judge",
                 "command": {"kind": "run", "workspace_id": 1, "command": ["printf", "draft"]},
                 "output_schema": {"type": "string"},
                 "postprocess": {"command": ["cat"], "timeout_ms": 10000,
                                 "stdin": {"text": {"from": "raw", "pointer": "/execution/stdout/text"},
                                           "code": {"from": "raw", "pointer": "/exit_code"}},
                                 "stdout": {"pointer": "/text"}}}
            ]}),
        );
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        tick_until(&ctx, &mut runner, "judge finished", |c, _| {
            get(c, "judge").state.is_terminal()
        });
        let typed = get(&ctx, "judge").typed_result.expect("typed");
        assert_eq!(serde_json::to_value(&typed.output).unwrap(), json!("draft"));
        assert_eq!(
            typed.raw.postprocess.unwrap().stdout,
            Some(json!({"text": "draft", "code": 0}))
        );
    }

    fn holders(ctx: &RunnerContext) -> Vec<String> {
        ctx.with_store(|mem| {
            SemaphoreStore::new(mem, HOST_OWNER)
                .get(1, "gpu")
                .unwrap()
                .map(|s| s.holders.into_iter().map(|h| h.id).collect())
                .unwrap_or_default()
        })
    }

    #[test]
    fn the_permit_is_held_until_the_postprocess_finishes() {
        let (td, ctx) = fresh_ctx();
        ctx.with_store(|mem| {
            SemaphoreStore::new(mem, HOST_OWNER)
                .create(1, "gpu", 1, 0)
                .unwrap()
        });
        let log_s = td.path().join("order.log").display().to_string();
        submit(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "judge", "metadata": {"semaphore": {"name": "gpu"}},
                 "command": {"kind": "run", "workspace_id": 1, "command": ["true"]},
                 "postprocess": {"command": sh(format!(
                     "echo pp-start >> '{log_s}'; sleep 0.3; echo pp-end >> '{log_s}'; printf 1")),
                     "timeout_ms": 10000}},
                {"id": "other", "metadata": {"semaphore": {"name": "gpu"}},
                 "command": {"kind": "run", "workspace_id": 1,
                             "command": sh(format!("echo other >> '{log_s}'"))}}
            ]}),
        );
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        tick_until(&ctx, &mut runner, "both finished", |c, _| {
            get(c, "judge").state.is_terminal() && get(c, "other").state.is_terminal()
        });
        let order = log_lines(&td.path().join("order.log"));
        // judge 가 먼저 permit 을 얻으면 후처리가 끝날 때까지 other 는 시작하지 않는다.
        if order.first().map(String::as_str) == Some("pp-start") {
            assert_eq!(order, ["pp-start", "pp-end", "other"]);
        } else {
            assert_eq!(order, ["other", "pp-start", "pp-end"]);
        }
        assert!(holders(&ctx).is_empty());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cancelling_kills_the_postprocess_before_the_permit_is_released() {
        let (td, ctx) = fresh_ctx();
        ctx.with_store(|mem| {
            SemaphoreStore::new(mem, HOST_OWNER)
                .create(1, "gpu", 1, 0)
                .unwrap()
        });
        let pidfile = td.path().join("child.pid");
        let pid_s = pidfile.display().to_string();
        submit(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "judge", "metadata": {"semaphore": {"name": "gpu"}},
                 "command": {"kind": "run", "workspace_id": 1, "command": ["true"]},
                 "postprocess": {"command": sh(format!(
                     "sleep 30 & echo $! > '{pid_s}'.tmp; mv '{pid_s}'.tmp '{pid_s}'; wait")),
                     "timeout_ms": 60000}}
            ]}),
        );
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        tick_until(&ctx, &mut runner, "postprocess child started", |_, _| {
            pidfile.exists()
        });
        let child: i32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        store_op(&ctx, |s| {
            s.set_state(1, &"judge".to_string(), TaskState::Cancelled, now_ms())
                .unwrap()
        });
        // 취소를 반영한 tick 에서는 종료를 확인하기 전이라 permit 을 아직 쥐고 있다.
        tick(&ctx, &mut runner, now_ms());
        assert_eq!(holders(&ctx), ["judge"]);
        tick_until(&ctx, &mut runner, "permit released", |c, _| {
            holders(c).is_empty()
        });
        assert!(
            super::gone(child),
            "postprocess grandchild {child} survived the cancel"
        );
        assert_eq!(get(&ctx, "judge").state, TaskState::Cancelled);
    }

    #[test]
    fn stopping_the_runner_kills_the_postprocess_and_records_a_cancelled_report() {
        let (td, ctx) = fresh_ctx();
        let pidfile = td.path().join("child.pid");
        let pid_s = pidfile.display().to_string();
        submit(
            &ctx,
            json!({"contract_version": 2, "tasks": [
                {"id": "judge",
                 "command": {"kind": "run", "workspace_id": 1, "command": ["true"]},
                 "postprocess": {"command": sh(format!(
                     "sleep 30 & echo $! > '{pid_s}'.tmp; mv '{pid_s}'.tmp '{pid_s}'; wait")),
                     "timeout_ms": 60000, "retry": {"max_retries": 3}}}
            ]}),
        );
        let mut runner = RunnerLoop::new(HostExecutor::new(ctx.clone()));
        tick_until(&ctx, &mut runner, "postprocess child started", |_, _| {
            pidfile.exists()
        });
        let child: i32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // runner 정지 = executor drop. 돌아온 시점에 그룹 종료와 보고 저장이 끝나 있어야 한다.
        drop(runner);
        assert!(
            super::gone(child),
            "postprocess grandchild {child} survived the runner stop"
        );
        let judge = get(&ctx, "judge");
        assert_eq!(judge.state, TaskState::Running);
        // 재시작 복원은 저장된 보고를 쓰고, 재시도가 남아 있어도 다시 실행하지 않는다.
        let stored = DispatchHandle::ShellProcess { pid: 0 };
        match super::super::restored_handle(&ctx, 1, &judge, &stored, None) {
            Some(DispatchHandle::PostprocessResolved(report)) => {
                assert_eq!(report.cause(), Some(PostprocessCause::Cancelled));
                assert!(
                    matches!(&report.outcome,
                        PostprocessOutcome::Failed { message, .. } if message.contains("runner stopped")),
                    "{report:?}"
                );
            }
            other => panic!("expected the stored report, got {other:?}"),
        }
    }
}

/// 호스트 역할 프로세스에서 후처리를 띄우는 helper 실행. 아래 시험이 테스트 바이너리를 이 이름으로
/// 다시 실행한다. 환경변수가 없으면 아무것도 하지 않는다.
#[cfg(target_os = "linux")]
#[test]
fn host_death_helper() {
    let Some(dir) = std::env::var_os("TASTY_PP_HOST_DEATH_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let leader = dir.join("leader.pid").display().to_string();
    let grandchild = dir.join("grandchild.pid").display().to_string();
    let script = format!(
        "sleep 30 & echo $! > '{grandchild}'; echo $$ > '{leader}'.tmp; mv '{leader}'.tmp '{leader}'; wait"
    );
    let req = ProcessRequest {
        command: vec!["sh".into(), "-c".into(), script],
        cwd: None,
        stdin: Vec::new(),
        stdout: json_out(),
        timeout: Duration::from_secs(60),
        run: 1,
        env: child_env(std::env::vars_os()),
    };
    let started = spawn(req).expect("spawn");
    // 신호로 끝나기를 기다린다.
    std::thread::sleep(Duration::from_secs(60));
    drop(started);
}

#[cfg(target_os = "linux")]
#[test]
fn the_postprocess_leader_gets_sigterm_when_the_host_dies() {
    let td = tempfile::tempdir().expect("tempdir");
    let mut host = std::process::Command::new(std::env::current_exe().expect("exe"))
        .args([
            "--exact",
            "runner_host::postprocess::tests::host_death_helper",
            "--test-threads=1",
        ])
        .env("TASTY_PP_HOST_DEATH_DIR", td.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("helper host");
    let read_pid = |name: &str| -> Option<i32> {
        std::fs::read_to_string(td.path().join(name))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    let leader = loop {
        if let Some(pid) = read_pid("leader.pid") {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "the helper host never started the postprocess"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let grandchild = read_pid("grandchild.pid").expect("grandchild pid");
    let host_pid = i32::try_from(host.id()).expect("pid");
    // SAFETY: 이 시험이 띄운 helper 에만 보낸다.
    assert_eq!(unsafe { libc::kill(host_pid, libc::SIGTERM) }, 0);
    host.wait().expect("reap helper host");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !gone(leader) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let leader_gone = gone(leader);
    // 그룹의 다른 프로세스는 신호를 받지 않는다(문서화한 한계). 시험이 남긴 것은 정리한다.
    if !gone(grandchild) {
        // SAFETY: 이 시험이 띄운 helper 의 후처리가 만든 프로세스다.
        unsafe { libc::kill(grandchild, libc::SIGKILL) };
    }
    assert!(
        leader_gone,
        "the postprocess leader {leader} outlived the host"
    );
}
