use super::*;
use crate::adapters::ipc::handler::deferred::{MemoryReplies, ReplyTo};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tasty_utils::{batch_writer, file_replace};
fn core() -> AppServices {
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
fn request(core: &AppServices, id: u64) -> JsonRpcResponse {
    super::super::handle_register(
        core,
        &crate::ipc::caller::CallerContext::local(),
        json!(id),
        &json!({"persistent":true,"sequence":[{"method":"system.info","params":{}}]}),
    )
}
fn park(core: &AppServices, response: JsonRpcResponse) -> mpsc::Receiver<JsonRpcResponse> {
    assert!(response.error.is_some());
    let pending = core.memory_replies().take_parked().unwrap();
    let (send, recv) = mpsc::sync_channel(2);
    core.defer_reply(pending, ReplyTo::Ipc(send));
    recv
}
fn receive(core: &mut AppServices, receiver: &mpsc::Receiver<JsonRpcResponse>) -> JsonRpcResponse {
    // holds_by 로 바꾸지 않는다. 응답을 꺼내는 poll_file_writer 가 webhook 의 다음 명령 제출과 child cleanup
    // 제출을 함께 해 기한 뒤 관측 읽기에서 부를 수 없고, 새 작업 없이 끝난 결과만 거두는 경로가 따로 없다.
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        core.poll_file_writer();
        assert!(MemoryReplies::poll(core).is_empty());
        if let Ok(answer) = receiver.try_recv() {
            return answer;
        }
        assert!(
            Instant::now() < until,
            "original webhook caller never answered"
        );
        std::thread::yield_now();
    }
}
fn drain(core: &mut AppServices) {
    // holds_by 로 바꾸지 않는다. 종료 poll 은 child cleanup 제출과 webhook 작업 처리를 함께 하는 진행이라
    // 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는 끝났는지 알 수 없다.
    let until = Instant::now() + Duration::from_secs(5);
    while core.poll_file_writer_shutdown() != 0 {
        assert!(MemoryReplies::poll(core).is_empty());
        assert!(Instant::now() < until, "webhook command writer drain");
        std::thread::yield_now();
    }
    assert!(MemoryReplies::poll(core).is_empty());
}
#[test]
fn held_register_and_remove_publish_and_answer_original_callers_once_after_saved_receipt() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let path = crate::webhook::config::config_path();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let reply = park(&core, request(&core, 751));
    core.poll_file_writer();
    assert!(MemoryReplies::poll(&core).is_empty());
    assert!(reply.try_recv().is_err());
    assert!(core.webhooks.list().is_empty());
    let list = super::super::handle_list(&core, json!(101));
    assert!(
        list.result.unwrap()["webhooks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(&path, "format=2\nport=40123\n[listener]\nkeep=true\n").unwrap();
    drop(held);
    let answer = receive(&mut core, &reply);
    assert_eq!(answer.id, json!(751));
    let registered = answer.result.unwrap()["id"].as_str().unwrap().to_owned();
    assert!(core.webhooks.info(&registered).is_some());
    let file = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert_eq!(file["port"].as_integer(), Some(40123));
    assert_eq!(file["listener"]["keep"].as_bool(), Some(true));
    assert!(reply.try_recv().is_err());
    let held = file_replace::lock(&path).unwrap();
    let response = super::super::handle_unregister(&core, json!(752), &json!({"id":registered}));
    let removed = park(&core, response);
    assert!(core.webhooks.info(&registered).is_some());
    assert!(removed.try_recv().is_err());
    drop(held);
    let answer = receive(&mut core, &removed);
    assert_eq!(answer.id, json!(752));
    assert_eq!(answer.result.unwrap()["unregistered"], true);
    assert!(core.webhooks.info(&registered).is_none());
    assert!(removed.try_recv().is_err());
    let counts = core.file_writer_stats().unwrap().1;
    assert_eq!(counts.operations.replace.succeeded, 2);
    assert_eq!(counts.commands.known_success, 2);
    drain(&mut core);
}
#[test]
fn known_ipc_save_error_reports_false_without_publishing_or_installing_anonymous_effect() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let path = crate::webhook::config::config_path();
    let mut core = core();
    std::fs::write(&path, "invalid toml [").unwrap();
    let calls = vec![crate::hook_handler::IpcCall {
        method: "wreg.failed-effects-probe".into(),
        params: json!({}),
    }];
    let handler = super::super::anonymous_handler(&calls).id;
    assert!(crate::hook_handler::global().get(&handler).is_none());
    let response = super::super::handle_register(
        &core,
        &crate::ipc::caller::CallerContext::local(),
        json!(753),
        &json!({"persistent":true,"sequence":calls}),
    );
    let reply = park(&core, response);
    let answer = receive(&mut core, &reply);
    assert_eq!(answer.id, json!(753));
    let data = answer.error.unwrap().data.unwrap();
    assert_eq!(data["applied"], false);
    assert_eq!(data["saved"], false);
    assert_eq!(data["outcome_unknown"], false);
    assert!(core.webhooks.list().is_empty());
    assert!(crate::hook_handler::global().get(&handler).is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid toml [");
    assert!(reply.try_recv().is_err());
    drain(&mut core);
}
#[test]
fn shutdown_keeps_writer_open_until_accepted_registry_command_and_original_reply_are_collected() {
    let _home = crate::test_support::TastyHomeGuard::new();
    let path = crate::webhook::config::config_path();
    let mut core = core();
    let held = file_replace::lock(&path).unwrap();
    let original = park(&core, request(&core, 754));
    assert!(core.poll_file_writer_shutdown() > 0);
    assert!(original.try_recv().is_err());
    let refused = request(&core, 755);
    assert_eq!(refused.error.unwrap().data.unwrap()["saved"], false);
    drop(held);
    // holds_by 로 바꾸지 않는다. 종료 poll 은 child cleanup 제출과 webhook 작업 처리를 함께 하는 진행이라
    // 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는 끝났는지 알 수 없다.
    let until = Instant::now() + Duration::from_secs(5);
    let mut answer = None;
    loop {
        let remaining = core.poll_file_writer_shutdown();
        assert!(MemoryReplies::poll(&core).is_empty());
        if answer.is_none() {
            answer = original.try_recv().ok();
        }
        if remaining == 0 {
            break;
        }
        assert!(
            Instant::now() < until,
            "shutdown did not finish accepted registry command"
        );
        std::thread::yield_now();
    }
    let answer = answer.expect("zero drain must include original caller completion");
    assert_eq!(answer.id, json!(754));
    assert!(answer.result.is_some());
    assert!(original.try_recv().is_err());
    assert_eq!(
        tasty_settings::webhook_port_file::read_table(&path).unwrap()["webhook"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
