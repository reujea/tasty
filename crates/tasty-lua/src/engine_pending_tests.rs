//! 진행 중 job 수와 발행 횟수를 잰다. 짧은 job은 송신 직후 끝날 수 있으므로 워커를 붙잡는
//! 장벽(빈 트리 동안 도는 스크립트)을 두고, 트리를 발행해 풀어 준다.

use super::*;
use serde_json::json;

/// 트리가 비어 있는 동안 워커를 붙잡는다. 기본 기한(5초) 안에 풀어야 한다.
const HOLD: &str = "while #tasty.tree() == 0 do end";

fn release(engine: &LuaEngine) {
    engine.publish_snapshot(LuaSnapshot {
        tree: vec![json!({"name": "released"})],
    });
}

fn hold_again(engine: &LuaEngine) {
    engine.publish_snapshot(LuaSnapshot::default());
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by(end, done),
        "시간 안에 {what}"
    );
}

fn wait_settled(engine: &LuaEngine) {
    wait_until("진행 중 job이 0이 된다", || {
        engine.pending_jobs() == 0
    });
}

/// Given 워커를 붙잡는 job, When fire·run_script·submit_script로 보낸다,
/// Then 실행 중에는 1이고 장벽을 풀면 0이 된다.
#[test]
fn fire_and_scripts_count_until_the_worker_finishes() {
    let engine = LuaEngine::new().expect("init");
    engine
        .eval(&format!("tasty.on('hold', function() {HOLD} end)"))
        .unwrap();
    wait_settled(&engine);

    engine.fire("hold", &json!({}));
    assert_eq!(engine.pending_jobs(), 1, "fire");
    release(&engine);
    wait_settled(&engine);

    hold_again(&engine);
    engine.run_script(HOLD, None);
    assert_eq!(engine.pending_jobs(), 1, "run_script");
    release(&engine);
    wait_settled(&engine);

    hold_again(&engine);
    engine.submit_script(HOLD, None, None).unwrap();
    assert_eq!(engine.pending_jobs(), 1, "submit_script");
    release(&engine);
    wait_settled(&engine);
}

/// Given 앞 job이 워커를 붙잡고 있다, When 블로킹 eval·reset_hooks를 보낸다,
/// Then 대기 중인 그 job도 세고, 끝나면 0이 된다.
#[test]
fn blocking_jobs_count_while_they_wait_in_the_queue() {
    let engine = LuaEngine::new().expect("init");
    for blocking in ["eval", "reset_hooks"] {
        hold_again(&engine);
        engine.run_script(HOLD, None);
        let pending = engine.pending_jobs.clone();
        let snapshot = engine.snapshot.clone();
        let releaser = std::thread::spawn(move || {
            wait_until("블로킹 job이 큐에 들어간다", || {
                pending.load(Ordering::SeqCst) == 2
            });
            *snapshot.lock().unwrap() = Arc::new(LuaSnapshot {
                tree: vec![json!({})],
            });
        });
        match blocking {
            "eval" => engine.eval("return 0").unwrap(),
            _ => engine.reset_hooks().unwrap(),
        }
        releaser.join().unwrap();
        wait_settled(&engine);
    }
}

/// Given 큐가 가득 찼거나 워커가 끝났다, When 더 보낸다,
/// Then 거절된 job은 수를 남기지 않고 이미 큐에 있던 job의 수는 그대로다.
#[test]
fn rejected_jobs_leave_no_count() {
    let mut engine = LuaEngine::new().expect("init");
    engine.run_script(&format!("tasty.run_cli('started') {HOLD}"), None);
    wait_until("워커가 장벽 job을 시작한다", || {
        !engine.drain_commands().is_empty()
    });
    for _ in 0..JOB_QUEUE_CAP {
        engine.fire("nobody", &json!({}));
    }
    let queued = engine.pending_jobs();
    assert_eq!(queued, 1 + JOB_QUEUE_CAP, "실행 중 1 + 큐 가득");
    engine.fire("nobody", &json!({}));
    engine.run_script("local x = 1", None);
    assert_eq!(
        engine.submit_script("local x = 1", None, None),
        Err(RunRejected::QueueFull)
    );
    assert_eq!(engine.pending_jobs(), queued, "거절된 job은 세지 않는다");
    release(&engine);
    wait_settled(&engine);

    engine.join_worker();
    engine.fire("nobody", &json!({}));
    engine.run_script("local x = 1", None);
    assert!(engine.submit_script("local x = 1", None, None).is_err());
    assert!(engine.eval("return 0").is_err());
    assert!(engine.reset_hooks().is_err());
    assert_eq!(engine.pending_jobs(), 0, "워커가 없으면 접수하지 않는다");
}

/// Given 오류·기한 초과로 끝나는 job, When 워커가 처리한다, Then 수가 정산된다.
#[test]
fn failed_and_timed_out_jobs_are_settled() {
    let engine = LuaEngine::with_deadline(Duration::from_millis(50)).expect("init");
    engine
        .submit_script("while true do end", None, None)
        .unwrap();
    engine.run_script("error('x')", None);
    engine
        .eval("tasty.on('boom', function() error('y') end)")
        .unwrap();
    engine.fire("boom", &json!({}));
    wait_settled(&engine);
}

/// Given 실행 중인 job과 대기 중인 job, When 엔진을 drop한다, Then 모든 job의 수가 정산된다.
/// 첫 job은 장벽으로 붙잡으므로 기본 기한(5초) 안에서는 송신 직후의 수가 4다. drop은 워커를 join하므로
/// 다른 스레드가 장벽을 푼다. 푸는 시점은 단언에 영향을 주지 않는다.
#[test]
fn dropping_the_engine_settles_every_job() {
    let engine = LuaEngine::new().expect("init");
    engine.run_script(HOLD, None);
    for _ in 0..3 {
        engine.run_script("local x = 1", None);
    }
    let pending = engine.pending_jobs.clone();
    assert_eq!(pending.load(Ordering::SeqCst), 4);
    let snapshot = engine.snapshot.clone();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        *snapshot.lock().unwrap() = Arc::new(LuaSnapshot {
            tree: vec![json!({})],
        });
    });
    drop(engine);
    releaser.join().unwrap();
    assert_eq!(pending.load(Ordering::SeqCst), 0);
}

/// Given 실행 중인 job, When 메인이 새 스냅샷을 발행한다, Then 같은 job의 다음 tree()가 새 값을 읽는다.
#[test]
fn a_running_job_reads_a_snapshot_published_after_it_started() {
    let engine = LuaEngine::new().expect("init");
    engine.run_script(
        &format!("{HOLD} tasty.run_cli({{tasty.tree()[1].name}})"),
        None,
    );
    assert_eq!(engine.pending_jobs(), 1);
    release(&engine);
    wait_settled(&engine);
    let cmds = engine.drain_commands();
    assert!(
        matches!(cmds.as_slice(), [HostCommand::RunCli(args)] if args == &["released"]),
        "{cmds:?}"
    );
}

#[test]
fn publishing_is_counted() {
    let engine = LuaEngine::new().expect("init");
    assert_eq!(engine.published_snapshots(), 0);
    release(&engine);
    hold_again(&engine);
    assert_eq!(engine.published_snapshots(), 2);
}

/// Given `__gc` finalizer가 있는 값, When 엔진을 drop해 VM을 닫는다,
/// Then finalizer의 tree()는 발행값과 관계없이 실패한다. mlua는 VM을 닫는 중에 표를 만들지 못한다.
/// 그래서 앱은 job이 없을 때 발행하지 않고 종료 직전에도 다시 발행하지 않는다.
/// 이 시험이 깨지면 finalizer가 트리를 읽을 수 있게 된 것이므로, VM drop 전에 스냅샷을 다시 발행해
/// 마지막 상태를 보게 해야 한다.
#[test]
fn a_finalizer_at_vm_close_cannot_read_the_tree() {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "tasty-lua-gc-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let engine = LuaEngine::new().expect("init");
    engine
        .eval(&format!(
            "keep = setmetatable({{}}, {{ __gc = function() \
             local f = io.open({path:?}, 'w') \
             f:write(tostring((pcall(tasty.tree)))) f:close() end }})"
        ))
        .unwrap();
    engine.publish_snapshot(LuaSnapshot {
        tree: vec![json!({}), json!({}), json!({})],
    });
    wait_settled(&engine);
    drop(engine);
    let written = std::fs::read_to_string(&path).expect("finalizer wrote");
    std::fs::remove_file(&path).unwrap();
    assert_eq!(written, "false");
}
