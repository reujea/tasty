//! job을 보내기 직전에 트리를 발행하는지 잰다. job이 `tasty.run_cli`로 남긴 HostCommand로
//! 그 job이 읽은 `tasty.tree()` 값을 메인 쪽에서 확인한다.

use super::*;
use std::cell::Cell;
use std::time::{Duration, Instant};
use tasty_lua::{HostCommand, LuaEngine, LuaSnapshot};

fn tree_of(len: usize) -> LuaSnapshot {
    LuaSnapshot {
        tree: vec![serde_json::json!({}); len],
    }
}

fn settle(engine: &LuaEngine) {
    let end = Instant::now() + Duration::from_secs(5);
    assert!(
        tasty_utils::test_wait::holds_by(end, || engine.pending_jobs() == 0),
        "job이 시간 안에 끝난다"
    );
}

/// job들이 `tasty.run_cli`로 남긴 첫 인자.
fn reported(engine: &LuaEngine) -> Vec<String> {
    settle(engine);
    engine
        .drain_commands()
        .into_iter()
        .map(|HostCommand::RunCli(args)| args.concat())
        .collect()
}

fn fire_once(send: Option<&LuaSend<'_>>, event: &str) {
    let scripts = tasty_settings::ScriptRegistry::default();
    let mut guard = super::super::autofire::AutofireGuard::new();
    let mut runs = super::super::lua_runs::LuaRuns::default();
    fire(
        send,
        AutofireCtx {
            scripts: &scripts,
            guard: &mut guard,
            runs: &mut runs,
        },
        event,
        &serde_json::Value::Null,
    );
}

const REPORT_TREE_LEN: &str = "tasty.on('e', function() tasty.run_cli({#tasty.tree()}) end)";

/// Given job이 없어 스냅샷이 비어 있다, When hook 이벤트를 보낸다,
/// Then hook 콜백의 tree()는 송신 시점의 트리를 읽는다.
#[test]
fn a_hook_job_reads_the_tree_published_before_it_was_sent() {
    let engine = LuaEngine::new().expect("init");
    engine.eval(REPORT_TREE_LEN).unwrap();
    let send = LuaSend::new(&engine, || tree_of(2));
    fire_once(Some(&send), "e");
    assert_eq!(reported(&engine), ["2"]);
}

/// Given 한 통로, When job을 여러 번 보낸다, Then 트리는 처음 한 번만 만들어 발행한다.
#[test]
fn one_send_path_publishes_once_for_many_jobs() {
    let engine = LuaEngine::new().expect("init");
    engine.eval(REPORT_TREE_LEN).unwrap();
    let built = Cell::new(0);
    let send = LuaSend::new(&engine, || {
        built.set(built.get() + 1);
        tree_of(3)
    });
    fire_once(Some(&send), "e");
    fire_once(Some(&send), "e");
    assert_eq!(reported(&engine), ["3", "3"]);
    assert_eq!(built.get(), 1);
    assert_eq!(engine.published_snapshots(), 1);
}

/// Given 통로를 만들었지만 job을 보내지 않는다, Then 트리를 만들지도 발행하지도 않는다.
#[test]
fn a_send_path_without_a_job_publishes_nothing() {
    let engine = LuaEngine::new().expect("init");
    let built = Cell::new(false);
    let _send = LuaSend::new(&engine, || {
        built.set(true);
        tree_of(1)
    });
    fire_once(None, "e");
    assert!(!built.get());
    assert_eq!(engine.published_snapshots(), 0);
}

/// Given debug.lua.eval 요청, When 소스가 있으면 송신 직전에 발행하고 없으면 발행하지 않는다.
/// debug IPC는 debug 빌드에만 있다.
#[cfg(debug_assertions)]
#[test]
fn debug_eval_publishes_only_when_it_sends() {
    let engine = LuaEngine::new().expect("init");
    let send = LuaSend::new(&engine, || tree_of(4));
    crate::app::services::lua_eval(Some(&send), serde_json::Value::Null, &serde_json::json!({}));
    assert_eq!(engine.published_snapshots(), 0, "소스 없음 → 송신 없음");
    crate::app::services::lua_eval(
        Some(&send),
        serde_json::Value::Null,
        &serde_json::json!({"source": "tasty.run_cli({#tasty.tree()})"}),
    );
    assert_eq!(reported(&engine), ["4"]);
}
