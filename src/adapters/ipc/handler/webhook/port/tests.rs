use super::*;
use crate::adapters::ipc::handler::deferred::{MemoryReplies, ReplyTo};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tasty_utils::{batch_writer, file_replace};

type Core = crate::app::services::AppServices;
fn core() -> Core {
    let writer = crate::app::file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    crate::adapters::ipc::handler::cli_entry_tests::test_core_builder()
        .with_file_writer(writer)
        .build()
        .unwrap()
}
fn call(
    core: &Core,
    session: &mut crate::runtime::engine_session::EngineSession,
    id: u64,
    params: Value,
) -> JsonRpcResponse {
    let engine = session.borrow_mut();
    let mut out = crate::ipc::window_port::IntentOutbox::default();
    super::super::handle_config(core, &mut out, &engine.read(), json!(id), &params)
}
fn park(core: &Core, response: JsonRpcResponse) -> mpsc::Receiver<JsonRpcResponse> {
    assert!(
        response.error.is_some(),
        "only the deferred route may see the placeholder"
    );
    let pending = core.memory_replies().take_parked().unwrap();
    let (tx, rx) = mpsc::sync_channel(2);
    core.defer_reply(pending, ReplyTo::Ipc(tx));
    rx
}
fn receive(core: &mut Core, rx: &mpsc::Receiver<JsonRpcResponse>) -> JsonRpcResponse {
    // holds_by 로 바꾸지 않는다. 응답을 꺼내는 poll_file_writer 가 webhook 의 다음 명령 제출과 child cleanup
    // 제출을 함께 해 기한 뒤 관측 읽기에서 부를 수 없고, 새 작업 없이 끝난 결과만 거두는 경로가 따로 없다.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        core.poll_file_writer();
        assert!(MemoryReplies::poll(core).is_empty());
        if let Ok(answer) = rx.try_recv() {
            return answer;
        }
        assert!(
            Instant::now() < deadline,
            "original port caller never completed"
        );
        std::thread::yield_now();
    }
}
fn drain(core: &mut Core) {
    // holds_by 로 바꾸지 않는다. 종료 poll 은 child cleanup 제출과 webhook 작업 처리를 함께 하는 진행이라
    // 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는 끝났는지 알 수 없다.
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline, "file writer did not drain");
        std::thread::yield_now();
    }
    assert!(MemoryReplies::poll(core).is_empty());
}

fn counts_after_bootstrap(core: &Core) -> crate::app::file_writer::FileWriteCounts {
    // Child-registry startup reads share this queue; settle them before measuring port commands.
    core.registries.child_terminals.wait_bootstrap();
    let fence = core
        .file_command_queue()
        .unwrap()
        .flush(batch_writer::FlushPoint::START)
        .unwrap();
    fence
        .wait_timeout(Duration::from_secs(5))
        .expect("bootstrap file fence completed")
        .expect("bootstrap file work succeeded");
    let (queue, counts) = core.file_writer_stats().unwrap();
    assert_eq!(queue.pending_items, 0);
    assert_eq!(queue.held_items, 0);
    counts
}
#[test]
fn held_runtime_port_commands_answer_original_callers_once_and_preserve_latest_sections() {
    let _home = crate::test_support::TastyHomeGuard::new();
    // EngineSession installs a thread-local home; retain it before resolving the actual path.
    let (_state, mut session) = crate::state::tests::test_state();
    let path = crate::webhook::config::config_path();
    std::fs::write(&path, "format=2\nport=40000\n").unwrap();
    let mut core = core();
    let before = counts_after_bootstrap(&core);
    let held = file_replace::lock(&path).unwrap();
    let first = park(&core, call(&core, &mut session, 741, json!({"port":41000})));
    let second = park(
        &core,
        call(&core, &mut session, 742, json!({"unset_port":true})),
    );
    assert!(MemoryReplies::poll(&core).is_empty());
    assert!(first.try_recv().is_err());
    assert!(second.try_recv().is_err());
    assert!(std::fs::read_to_string(&path).unwrap().contains("40000"));
    // A foreign section changed after admission must be read from the latest locked document.
    std::fs::write(
        &path,
        "format=2\nport=40000\n[listener]\nkeep=true\n[[webhook]]\nid='fixture'\n",
    )
    .unwrap();
    assert_eq!(
        core.file_writer_stats()
            .unwrap()
            .1
            .operations
            .replace
            .attempts
            - before.operations.replace.attempts,
        0
    );
    drop(held);
    let answer = receive(&mut core, &first);
    assert_eq!(answer.id, json!(741));
    assert_eq!(answer.result.unwrap()["saved_port"], 41000);
    let answer = receive(&mut core, &second);
    assert_eq!(answer.id, json!(742));
    assert!(answer.result.unwrap()["saved_port"].is_null());
    let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert!(!table.contains_key("port"));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    assert_eq!(table["webhook"][0]["id"].as_str(), Some("fixture"));
    assert!(first.try_recv().is_err());
    assert!(second.try_recv().is_err());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(
        counts.operations.replace.succeeded - before.operations.replace.succeeded,
        2
    );
    assert_eq!(
        counts.commands.known_success - before.commands.known_success,
        2
    );
    assert_eq!(
        counts.commands.unclassified - before.commands.unclassified,
        0
    );
    drain(&mut core);
    assert!(first.try_recv().is_err());
    assert!(second.try_recv().is_err());
}

#[test]
fn runtime_port_query_waits_for_legacy_migration_and_keeps_live_listener_metadata() {
    let _home = crate::test_support::TastyHomeGuard::new();
    // EngineSession installs a thread-local home; retain it before resolving the actual path.
    let (_state, mut session) = crate::state::tests::test_state();
    let path = crate::webhook::config::config_path();
    std::fs::write(&path, "port=40000\n").unwrap();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let reply = park(&core, call(&core, &mut session, 743, json!({})));
    assert!(MemoryReplies::poll(&core).is_empty());
    assert!(reply.try_recv().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "port=40000\n");
    drop(held);
    let answer = receive(&mut core, &reply);
    assert_eq!(answer.id, json!(743));
    let status = answer.result.unwrap();
    assert_eq!(status["saved_port"], 40000);
    assert_eq!(status["bound"], false);
    assert!(status["port"].is_null());
    assert_eq!(
        tasty_settings::webhook_port_file::read_table(&path).unwrap()["format"].as_integer(),
        Some(2)
    );
    assert_eq!(
        core.file_writer_stats()
            .unwrap()
            .1
            .operations
            .replace
            .succeeded,
        1
    );
    drain(&mut core);
    assert!(reply.try_recv().is_err());
}

#[test]
fn runtime_port_failure_keeps_the_original_and_drain_delivers_the_original_error_once() {
    let _home = crate::test_support::TastyHomeGuard::new();
    // EngineSession installs a thread-local home; retain it before resolving the actual path.
    let (_state, mut session) = crate::state::tests::test_state();
    let path = crate::webhook::config::config_path();
    let original = "auth='isolated-port-fixture'\nbroken=[";
    std::fs::write(&path, original).unwrap();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let reply = park(&core, call(&core, &mut session, 744, json!({"port":41000})));
    assert!(MemoryReplies::poll(&core).is_empty());
    assert!(reply.try_recv().is_err());
    drop(held);
    drain(&mut core);
    let answer = reply.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(answer.id, json!(744));
    let error = answer.error.unwrap();
    assert!(!error.message.contains("isolated-port-fixture"));
    let data = error.data.unwrap();
    assert_eq!(data["saved"], false);
    assert_eq!(data["outcome_unknown"], false);
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.attempts, 0);
    assert_eq!(counts.commands.known_failure, 1);
    assert!(reply.try_recv().is_err());
}

#[test]
fn a_closed_runtime_port_queue_refuses_without_synchronous_fallback() {
    let _home = crate::test_support::TastyHomeGuard::new();
    // EngineSession installs a thread-local home; retain it before resolving the actual path.
    let (_state, mut session) = crate::state::tests::test_state();
    let path = crate::webhook::config::config_path();
    let mut core = core();
    drain(&mut core);
    let response = call(&core, &mut session, 745, json!({"port":41000}));
    assert_eq!(response.id, json!(745));
    let error = response.error.unwrap();
    assert_eq!(error.data.unwrap()["saved"], false);
    assert!(!path.exists());
    assert!(core.memory_replies().take_parked().is_none());
    assert_eq!(
        core.file_writer_stats()
            .unwrap()
            .1
            .operations
            .replace
            .attempts,
        0
    );
}
