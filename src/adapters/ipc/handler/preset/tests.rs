use super::*;
use crate::adapters::ipc::handler::deferred::{MemoryReplies, ReplyTo};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use tasty_presets::OwnedPreset;
use tasty_utils::{batch_writer, file_replace};

#[test]
fn preset_save_answers_original_caller_once_after_receipt_and_cache() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("workspace")).unwrap();
    let path = root.path().join("workspace/dev.toml");
    let cache = Arc::new(Mutex::new(tasty_presets::PresetStore::load_from(
        root.path().into(),
    )));
    let writer = crate::app::file_writer::start(
        Arc::new(tasty_settings::FileSettingsStorage),
        batch_writer::Limits::default(),
        Arc::new(batch_writer::SystemClock),
        Arc::new(|| {}),
    )
    .unwrap();
    let mut core = super::super::cli_entry_tests::test_core_builder()
        .with_preset_store(cache.clone())
        .with_file_writer(writer)
        .build()
        .unwrap();
    for fail in [false, true] {
        let held = file_replace::lock(&path).unwrap();
        let OwnedPreset::Workspace(preset) =
            crate::app::preset_commands::tests::value(if fail { "discarded" } else { "saved" })
        else {
            unreachable!()
        };
        let response = handle_save(
            &core,
            json!(91),
            &json!({
                "kind":"workspace", "name":"dev", "overwrite":fail, "data":preset,
            }),
        );
        assert!(
            response.error.is_some(),
            "pending placeholder must stay inside deferred route"
        );
        let deferred = core.memory_replies().take_parked().unwrap();
        let (tx, rx) = mpsc::sync_channel(2);
        core.defer_reply(deferred, ReplyTo::Ipc(tx));
        assert!(MemoryReplies::poll(&core).is_empty());
        assert!(rx.try_recv().is_err());
        if fail {
            assert_eq!(
                cache.lock().unwrap().get_workspace("dev").unwrap().subtitle,
                "saved"
            );
            std::fs::write(&path, "# external edit\n").unwrap();
        } else {
            assert!(cache.lock().unwrap().get_workspace("dev").is_none());
        }
        drop(held);
        let answer = receive_preset_reply(&mut core, &rx);
        assert_eq!(answer.id, json!(91));
        assert_eq!(
            cache.lock().unwrap().get_workspace("dev").unwrap().subtitle,
            "saved"
        );
        if fail {
            assert!(answer.error.is_some());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "# external edit\n");
        } else {
            assert_eq!(answer.result.unwrap(), json!({"name":"dev"}));
            assert_eq!(
                tasty_presets::PresetStore::load_from(root.path().into())
                    .get_workspace("dev")
                    .unwrap()
                    .subtitle,
                "saved"
            );
        }
        MemoryReplies::poll(&core);
        assert!(rx.try_recv().is_err());
    }
    // holds_by 로 바꾸지 않는다. 종료 poll 은 child cleanup 제출과 webhook 작업 처리를 함께 하는 진행이라
    // 기한 뒤 관측 읽기에서 부를 수 없고, 부르지 않고 볼 수 있는 저장 상태로는 끝났는지 알 수 없다.
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn receive_preset_reply(
    core: &mut crate::app::services::AppServices,
    rx: &mpsc::Receiver<JsonRpcResponse>,
) -> JsonRpcResponse {
    // poll_file_writer 는 child cleanup 제출까지 하는 진행이라 기한 뒤 관측 읽기에서는 부르지 않는다. 그 대신
    // 끝난 preset 명령 결과만 거두는 preset_files.poll 과 끝난 응답만 보내는 MemoryReplies::poll 로 받는다.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut answer = None;
    assert!(
        tasty_utils::test_wait::holds_by_every(deadline, Duration::ZERO, |last| {
            if last {
                core.preset_files.poll();
            } else {
                core.poll_file_writer();
            }
            assert!(MemoryReplies::poll(core).is_empty());
            answer = rx.try_recv().ok();
            answer.is_some()
        }),
        "original caller was not completed"
    );
    answer.expect("checked above")
}
