//! Actual App state plus real file writer; ready typed cache results are not capture replies.
use super::*;
use crate::app::journal::{PresetCaptureCompletion, PresetCaptureReply};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use tasty_utils::{batch_writer::FlushPoint, file_replace};

struct LaterGate(Option<mpsc::Sender<()>>);
impl LaterGate {
    fn release(&mut self) {
        if let Some(sender) = self.0.take()
            && sender.send(()).is_err()
        {
            tracing::debug!("test file gate already ended");
        }
    }
}
impl Drop for LaterGate {
    fn drop(&mut self) {
        self.release();
    }
}

pub(crate) fn run_case(
    make_app: impl FnOnce() -> App,
    drain: impl Fn(&mut App),
    timeout_with_later_work: bool,
) {
    let _home = crate::test_support::IsolatedHome::new();
    let home = tasty_utils::path::tasty_home().unwrap();
    std::fs::create_dir_all(home.join("presets/workspace")).unwrap();
    let mut app = make_app();
    let path = home.join("presets/workspace/captured.toml");
    let held = file_replace::lock(&path).unwrap();
    let (sender, receiver) = mpsc::sync_channel(2);
    let completion = PresetCaptureCompletion {
        #[cfg(feature = "gui")]
        engine: crate::runtime::engine_session::EngineId::for_test(),
        #[cfg(feature = "gui")]
        binding: crate::runtime::journal_product::EngineBinding {
            journal_id: "shutdown-test".into(),
            stream: "shutdown-test".into(),
            incarnation: 1,
            runtime_epoch: 1,
            published_cut: None,
            revision: None,
        },
        reply: PresetCaptureReply::Ipc {
            id: serde_json::json!(137),
            sender,
            name: Some("captured".into()),
        },
        result: Ok((
            crate::app::preset_commands::tests::value("shutdown"),
            "base".into(),
        )),
    };
    app.preset_io.captures.push(completion.save(&app.services));
    let queue = app.services.file_command_queue().unwrap();
    // The flush cut separates the capture's batch from a deliberately unfinished later command.
    let _cut = queue.flush(FlushPoint::START).unwrap();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut gate = LaterGate(Some(release));
    let _later = if timeout_with_later_work {
        Some(
            crate::app::file_writer::command::submit(
                Some(&queue),
                128,
                move || {
                    entered.send(()).unwrap();
                    released
                        .recv_timeout(Duration::from_secs(20))
                        .map_err(|error| error.to_string())
                },
                |_| 64,
            )
            .unwrap(),
        )
    } else {
        None
    };
    assert!(receiver.try_recv().is_err());
    assert!(
        app.services
            .preset_store
            .lock()
            .unwrap()
            .get_workspace("captured")
            .is_none()
    );
    assert_ne!(app.services.poll_file_writer_shutdown(), 0);
    drop(held);
    if timeout_with_later_work {
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(app.services.poll_file_writer_shutdown(), 0);
    } else {
        // holds_by 로 바꾸지 않는다. poll_file_writer_shutdown 은 child cleanup 제출과 webhook 처리를 함께
        // 진행해 기한 뒤 관측에서 부를 수 없고, 그것 없이 종료 완료를 볼 길이 없다.
        let end = Instant::now() + Duration::from_secs(5);
        while app.services.poll_file_writer_shutdown() != 0 {
            assert!(Instant::now() < end, "gated file writer failed to join");
            std::thread::yield_now();
        }
    }
    // Force the real final-zero/timeout App boundary: services has committed the cache, while
    // the original reply is still owned solely by App.preset_io, not MemoryReplies.
    assert!(path.is_file());
    assert_eq!(
        app.services
            .preset_store
            .lock()
            .unwrap()
            .get_workspace("captured")
            .unwrap()
            .subtitle,
        "shutdown"
    );
    assert_eq!(app.preset_io.captures.len(), 1);
    assert!(receiver.try_recv().is_err());
    drain(&mut app);
    let response = receiver.try_recv().expect(
        "file saved and cache committed but original capture response missing at App drain return",
    );
    assert_eq!(response.id, serde_json::json!(137));
    assert_eq!(
        response.result.unwrap(),
        serde_json::json!({"name":"captured"})
    );
    assert!(app.preset_io.captures.is_empty());
    assert_eq!(
        app.services.preset_files.status().accepted,
        1,
        "completion collection must not admit a new capture"
    );
    gate.release();
    // holds_by 로 바꾸지 않는다. poll_file_writer_shutdown 은 child cleanup 제출과 webhook 처리를 함께
    // 진행해 기한 뒤 관측에서 부를 수 없고, 그것 없이 종료 완료를 볼 길이 없다.
    let end = Instant::now() + Duration::from_secs(5);
    while app.services.poll_file_writer_shutdown() != 0 {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    drain(&mut app);
    assert!(
        receiver.try_recv().is_err(),
        "original capture reply duplicated"
    );
    drain_other_writers(&mut app, end);
}

#[cfg(feature = "gui")]
pub(crate) fn make_gui(proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>) -> App {
    App::new(
        proxy,
        None,
        None,
        None,
        #[cfg(debug_assertions)]
        false,
    )
    .unwrap()
}
#[cfg(not(feature = "gui"))]
pub(crate) fn make_headless() -> App {
    App::new_headless(std::sync::Arc::new(|| {}), None, None, None).unwrap()
}

fn drain_other_writers(app: &mut App, end: Instant) {
    // 두 poll 은 끝난 writer worker 를 join 할 뿐 새 작업을 만들지 않는다.
    assert!(tasty_utils::test_wait::holds_by(end, || {
        let memory = app.services.poll_memory_writer_shutdown();
        #[cfg(feature = "gui")]
        let state = app.services.poll_state_writer_shutdown();
        #[cfg(not(feature = "gui"))]
        let state = 0;
        memory == 0 && state == 0
    }));
}
