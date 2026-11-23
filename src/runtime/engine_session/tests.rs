use std::sync::Arc;

use tasty_terminal::Terminal;

use super::EngineSession;
use crate::runtime::output_observer::{ObserverSpec, SinkSpec};
use tasty_task_runtime::event_feed::AgentEvent;
use tasty_task_runtime::task_waker::TerminalSnapshot;

fn session() -> EngineSession {
    crate::state::tests::test_state().1
}

#[test]
fn execution_borrows_keep_one_terminal_and_one_task_scope() {
    let mut owner = session();
    let sid = owner
        .core_state
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    owner
        .runtime
        .terminals
        .insert(sid, Terminal::new_detached(80, 24), None);
    let hub = Arc::clone(owner.task_scope.waker_hub());
    let feed = Arc::clone(owner.task_scope.event_queue());
    let runners = Arc::clone(owner.task_scope.runner_registry());
    {
        let engine = owner.borrow_mut();
        assert!(Arc::ptr_eq(&hub, engine.task_scope.waker_hub()));
        assert!(Arc::ptr_eq(&feed, engine.task_scope.event_queue()));
        assert!(Arc::ptr_eq(&runners, engine.task_scope.runner_registry()));
        engine
            .runtime
            .terminals
            .get_mut(sid)
            .unwrap()
            .feed_bytes(b"one-owner");
        assert!(
            engine
                .as_ref()
                .find_terminal_by_id(sid)
                .unwrap()
                .screen_text(false)
                .contains("one-owner")
        );
    }
    assert!(
        owner
            .runtime
            .terminals
            .get(sid)
            .unwrap()
            .screen_text(false)
            .contains("one-owner")
    );
}

#[test]
fn session_drop_joins_observer_and_preserves_external_task_handles() {
    let mut owner = session();
    let sid = owner
        .core_state
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let memory = owner.runtime.memory_access.clone();
    let hub = Arc::clone(owner.task_scope.waker_hub());
    let feed = Arc::clone(owner.task_scope.event_queue());
    let observer = owner
        .observer_router
        .register(
            ObserverSpec {
                surface_id: Some(sid),
                parsers: vec!["path".into()],
                kinds: None,
                sink: SinkSpec::Memory { max_records: 0 },
            },
            memory.clone(),
        )
        .expect("observer worker");
    owner
        .observer_router
        .dispatch_text(sid, "/session-owner/accepted.rs\n");
    // No sleep or explicit join: the owning Session must finish the observer's accepted work.
    drop(owner);
    let records = memory
        .lock_inline_for_tests()
        .unwrap()
        .list(
            &tasty_memory::Scope::Global,
            &tasty_memory::ListOpts {
                prefix: Some(format!("tasty.observer.{observer}.")),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(records.len(), 1);
    assert!(
        feed.take_pending().0.is_empty(),
        "dropping a scope must not invent task cancellation"
    );
    hub.fire(
        1,
        &"held-task".to_string(),
        TerminalSnapshot {
            state: tasty_agent::TaskState::Succeeded,
            result: None,
            attempt_id: None,
            skip: None,
            revision: None,
        },
    );
    assert_eq!(
        feed.take_pending(),
        (
            vec![AgentEvent::TaskFinished {
                workspace_id: 1,
                task_id: "held-task".into(),
                state: "succeeded",
                attempt_id: None,
                revision: None,
                terminal_id: None,
                skip: None,
            }],
            0
        )
    );
}

#[test]
fn retired_terminal_content_cannot_mutate_the_replacement_binding() {
    let mut owner = session();
    let sid = owner
        .core_state
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    owner
        .runtime
        .terminals
        .insert(sid, Terminal::new_detached(80, 24), None);
    let old_epoch = owner
        .runtime
        .terminals
        .get(sid)
        .unwrap()
        .with_content(|v| v.cut().epoch);
    let (mut old, _old_pty) = owner
        .runtime
        .terminals
        .replace(sid, Terminal::new_detached(80, 24), None)
        .unwrap();
    owner
        .runtime
        .terminals
        .get_mut(sid)
        .unwrap()
        .feed_bytes(b"replacement");
    old.feed_bytes(b"late-old-content");
    let current = owner.runtime.terminals.get(sid).unwrap();
    assert_ne!(current.with_content(|v| v.cut().epoch), old_epoch);
    assert!(current.screen_text(false).contains("replacement"));
    assert!(!current.screen_text(false).contains("late-old-content"));
}

#[cfg(feature = "gui")]
#[test]
fn parked_session_keeps_terminal_hook_and_task_identity() {
    use crate::app::engine_registry::EngineRegistry;
    use crate::hook_runtime::HookExecutor;
    use tasty_hooks::{HookBinding, HookEvent};
    let (state, mut owner) = crate::state::tests::test_state();
    let sid = owner
        .core_state
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    owner
        .runtime
        .terminals
        .insert(sid, Terminal::new_detached(80, 24), None);
    let epoch = owner
        .runtime
        .terminals
        .get(sid)
        .unwrap()
        .with_content(|v| v.cut().epoch);
    let hub = Arc::clone(owner.task_scope.waker_hub());
    owner.hooks.add_surface_hook(
        sid,
        HookEvent::Bell,
        HookBinding::Handler("test.missing".into()),
        true,
    );
    let mut registry = EngineRegistry::default();
    let id = registry
        .insert_pending(owner)
        .unwrap_or_else(|_| panic!("empty registry"));
    let window = winit::window::WindowId::from(7);
    registry.attach_window(window, id);
    assert_eq!(registry.park(window, state), Some(id));
    {
        let engine = registry.get_mut(id).unwrap();
        engine
            .runtime
            .terminals
            .get_mut(sid)
            .unwrap()
            .feed_bytes(b"output-while-parked");
    }
    let (restored_id, _state) = registry.unpark_first().unwrap();
    assert_eq!(restored_id, id);
    let engine = registry.get_mut(id).unwrap();
    assert!(Arc::ptr_eq(&hub, engine.task_scope.waker_hub()));
    let terminal = engine.runtime.terminals.get(sid).unwrap();
    assert_eq!(terminal.with_content(|v| v.cut().epoch), epoch);
    assert!(terminal.screen_text(false).contains("output-while-parked"));
    let executor = HookExecutor::new(None);
    assert_eq!(engine.hooks.fire(&executor, sid, HookEvent::Bell).len(), 1);
    assert!(
        engine
            .hooks
            .fire(&executor, sid, HookEvent::Bell)
            .is_empty()
    );
}

#[cfg(all(feature = "gui", unix))]
#[test]
fn a_physical_pty_keeps_its_pid_generation_and_io_while_parked() {
    use crate::app::engine_registry::EngineRegistry;
    use std::time::{Duration, Instant};
    let _home = crate::test_support::IsolatedHome::new();
    let (state, mut owner) = crate::state::tests::test_state();
    let sid = owner
        .core_state
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let (terminal, pty) = tasty_terminal::spawn_terminal(
        tasty_terminal::TerminalConfig {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            args: &["-c", "stty -echo; printf READY; while IFS= read -r value; do printf 'RESULT:%s\\n' \"$value\"; done"],
            surface_id: sid,
            working_dir: None,
            initial_input: None,
            extra_env: &[],
        },
        Arc::new(|| {}),
    ).unwrap();
    let pid = pty.process_id().unwrap();
    let generation = pty.generation();
    owner.runtime.terminals.insert(sid, terminal, Some(pty));
    let mut registry = EngineRegistry::default();
    let engine_id = registry.park_for_test(state, owner);
    let engine = registry.get_mut(engine_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(5), |_| {
            engine
                .runtime
                .terminals
                .get(sid)
                .unwrap()
                .screen_text(false)
                .contains("READY")
        }),
        "parked reader did not ingest actual PTY output"
    );
    engine
        .runtime
        .terminals
        .get_mut(sid)
        .unwrap()
        .send_bytes(b"parked-input\n");
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::from_millis(5), |_| {
            engine
                .runtime
                .terminals
                .get(sid)
                .unwrap()
                .screen_text(false)
                .contains("RESULT:parked-input")
        }),
        "parked writer/reader did not complete actual I/O"
    );
    assert_eq!(registry.unpark_first().unwrap().0, engine_id);
    let engine = registry.get_mut(engine_id).unwrap();
    let pty = engine.runtime.terminals.pty_mut(sid).unwrap();
    assert_eq!(pty.process_id(), Some(pid));
    assert_eq!(pty.generation(), generation);
    assert!(pty.is_alive());
    assert!(
        engine
            .runtime
            .terminals
            .get(sid)
            .unwrap()
            .screen_text(false)
            .contains("RESULT:parked-input")
    );
}
