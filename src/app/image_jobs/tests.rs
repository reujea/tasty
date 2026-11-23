use super::paste::{self, PasteDelivery, PasteRequest};
use super::{ImageJobRejected, ImageWorker, MAX_HELD_BYTES, MAX_JOBS};
use crate::runtime::surface_binding::SurfaceBinding;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tasty_utils::test_wait::{holds_by, holds_by_every};

fn worker<T: Send + 'static>() -> (ImageWorker<T>, mpsc::Receiver<()>) {
    let (wake_tx, wake_rx) = mpsc::channel();
    let worker = ImageWorker::new(Arc::new(move || {
        if wake_tx.send(()).is_err() {
            tracing::debug!("test wake receiver closed");
        }
    }));
    (worker, wake_rx)
}

fn wait_results<T: Send + 'static>(worker: &mut ImageWorker<T>, count: usize) -> Vec<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut done = Vec::new();
    // 원래 순서는 읽기 → 기한 검사 → 거두기 → 쉼이다. 거두기는 검사를 통과한 뒤에만 돌리고,
    // 기한 뒤 관측 읽기에서는 끝난 결과만 거둔 뒤 한 번 더 읽는다.
    let mut started = false;
    assert!(
        holds_by_every(deadline, Duration::ZERO, |last| {
            if last || std::mem::replace(&mut started, true) {
                done.extend(worker.take_results());
                if !last {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            done.len() >= count
        }),
        "image job results did not arrive"
    );
    done
}

/// 작업이 끝나지 않아도 접수는 바로 돌아오고, 완료되면 깨우기와 결과가 온다.
#[test]
fn submission_does_not_wait_for_the_file_work() {
    let (mut jobs, wake) = worker::<u32>();
    let (release, gate) = mpsc::channel::<()>();
    let started = Instant::now();
    jobs.submit(1, move || {
        gate.recv().expect("test releases the job");
        7
    })
    .expect("accepted");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(jobs.take_results().is_empty());
    assert!(jobs.has_pending());

    release.send(()).expect("job waits");
    wake.recv_timeout(Duration::from_secs(10))
        .expect("worker wakes the loop");
    assert_eq!(wait_results(&mut jobs, 1), [7]);
    assert!(!jobs.has_pending());
}

/// 대기 건수와 메모리 바이트 상한을 넘는 요청은 Busy 로 거절하고, 결과를 회수하면 다시 받는다.
#[test]
fn admission_is_bounded_by_job_count_and_held_bytes() {
    let (mut jobs, _wake) = worker::<usize>();
    let (release, gate) = mpsc::channel::<()>();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    for index in 0..MAX_JOBS {
        let gate = gate.clone();
        jobs.submit(0, move || {
            gate.lock().expect("gate").recv().expect("released");
            index
        })
        .expect("within the job bound");
    }
    assert_eq!(jobs.submit(0, || 99), Err(ImageJobRejected::Busy));

    for _ in 0..MAX_JOBS {
        release.send(()).expect("job waits");
    }
    let order = wait_results(&mut jobs, MAX_JOBS);
    assert_eq!(order, (0..MAX_JOBS).collect::<Vec<_>>(), "FIFO completion");

    jobs.submit(MAX_HELD_BYTES, || 1)
        .expect("exactly the byte bound");
    assert_eq!(jobs.submit(1, || 2), Err(ImageJobRejected::Busy));
    assert_eq!(wait_results(&mut jobs, 1), [1]);
    jobs.submit(1, || 3)
        .expect("bytes released with the result");
    assert_eq!(wait_results(&mut jobs, 1), [3]);
}

/// 비어 있는 worker 는 바이트 상한을 넘는 한 건을 받고, 그 건이 끝날 때까지 다음 요청은 거절한다.
#[test]
fn an_idle_worker_accepts_one_oversized_job() {
    let (mut jobs, _wake) = worker::<u32>();
    let (release, gate) = mpsc::channel::<()>();
    jobs.submit(MAX_HELD_BYTES + 1, move || {
        gate.recv().expect("released");
        1
    })
    .expect("an oversized job on an idle worker");
    assert_eq!(jobs.submit(0, || 2), Err(ImageJobRejected::Busy));
    release.send(()).expect("job waits");
    assert_eq!(wait_results(&mut jobs, 1), [1]);
    jobs.submit(MAX_HELD_BYTES + 1, || 3)
        .expect("accepted again once idle");
    assert_eq!(wait_results(&mut jobs, 1), [3]);
}

/// 종료가 시작되면 새 요청을 거절하고, 시작하지 않은 작업은 실행하지 않으며 worker 가 끝난다.
#[test]
fn shutdown_skips_queued_jobs_and_joins_the_worker() {
    let (mut jobs, _wake) = worker::<u32>();
    let (release, gate) = mpsc::channel::<()>();
    let (started_tx, started) = mpsc::channel::<()>();
    let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    jobs.submit(0, move || {
        started_tx.send(()).expect("test waits for the start");
        gate.recv().expect("released");
        1
    })
    .expect("accepted");
    for _ in 0..3 {
        let ran = ran.clone();
        jobs.submit(0, move || {
            ran.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            2
        })
        .expect("accepted");
    }
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("first job running");
    jobs.begin_shutdown();
    assert_eq!(jobs.submit(0, || 3), Err(ImageJobRejected::ShuttingDown));
    assert_ne!(jobs.poll_shutdown(), 0, "the running job holds the worker");

    release.send(()).expect("job waits");
    let deadline = Instant::now() + Duration::from_secs(10);
    // poll_shutdown 은 남은 결과를 버리고 끝난 worker 를 join 할 뿐 새 작업을 만들지 않는다.
    assert!(
        holds_by(deadline, || jobs.poll_shutdown() == 0),
        "worker did not finish"
    );
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// 결과를 전달하지 않고 버리면 임시 디렉터리까지 회수된다. 종료 중 버린 결과도 같다.
#[test]
fn a_discarded_saved_file_is_removed() {
    let saved = paste::save("paste-1.png", b"png").expect("saved");
    let path = saved.path().to_path_buf();
    assert_eq!(std::fs::read(&path).expect("readable"), b"png");
    let dir = path.parent().expect("own dir").to_path_buf();
    drop(saved);
    assert!(!dir.exists(), "undelivered paste directory is reclaimed");

    let (mut jobs, _wake) = worker::<paste::SavedFile>();
    let (path_tx, path_rx) = mpsc::channel();
    jobs.submit(3, move || {
        let saved = paste::save("paste-2.png", b"png").expect("saved");
        path_tx
            .send(saved.path().to_path_buf())
            .expect("test waits");
        saved
    })
    .expect("accepted");
    let path = path_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("job ran");
    let deadline = Instant::now() + Duration::from_secs(10);
    // poll_shutdown 은 남은 결과를 버리고 끝난 worker 를 join 할 뿐 새 작업을 만들지 않는다.
    assert!(
        holds_by(deadline, || jobs.poll_shutdown() == 0),
        "worker did not finish"
    );
    assert!(!path.exists(), "shutdown discards results and their files");
}

fn terminal_with_sink(
    engine: &mut crate::runtime::engine_session::EngineSession,
    sid: u32,
) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    let mut terminal = tasty_terminal::Terminal::new_detached(80, 24);
    terminal.set_input_sink(tx);
    engine
        .borrow_mut()
        .runtime
        .terminals
        .insert(sid, terminal, None);
    rx
}

fn request(target: SurfaceBinding, bracketed: bool) -> PasteRequest {
    PasteRequest {
        target,
        origin_view: std::sync::Weak::new(),
        bracketed,
    }
}

/// 원 터미널이 그대로면 경로를 입력하고 파일을 남긴다. View 가 닫혀도 입력 여부는 바뀌지 않는다.
#[test]
fn a_saved_path_reaches_only_the_original_terminal_generation() {
    let (_state, mut engine) = crate::state::tests::test_state();
    let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
    let input = terminal_with_sink(&mut engine, sid);
    let target = SurfaceBinding::capture(&engine.read(), sid).expect("binding");

    let saved = paste::save("paste-3.png", b"png").expect("saved");
    let path = saved.path().to_path_buf();
    let delivery = paste::deliver(&mut engine.borrow_mut(), &request(target, true), saved);
    assert_eq!(delivery, PasteDelivery::Delivered);
    let mut expected = b"\x1b[200~".to_vec();
    expected.extend_from_slice(path.to_string_lossy().as_bytes());
    expected.extend_from_slice(b"\x1b[201~");
    assert_eq!(input.try_recv().expect("path written"), expected);
    assert!(path.exists(), "the shell reads the delivered file later");
    std::fs::remove_dir_all(path.parent().expect("own dir")).expect("test cleanup");
}

/// 같은 번호의 새 터미널에는 입력하지 않고 파일을 회수한다.
#[test]
fn a_replaced_terminal_does_not_receive_the_path() {
    let (_state, mut engine) = crate::state::tests::test_state();
    let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
    drop(terminal_with_sink(&mut engine, sid));
    let target = SurfaceBinding::capture(&engine.read(), sid).expect("binding");
    let replacement = terminal_with_sink(&mut engine, sid);

    let saved = paste::save("paste-4.png", b"png").expect("saved");
    let dir = saved.path().parent().expect("own dir").to_path_buf();
    let delivery = paste::deliver(&mut engine.borrow_mut(), &request(target, false), saved);
    assert_eq!(delivery, PasteDelivery::TargetRetired);
    assert!(replacement.try_recv().is_err());
    assert!(!dir.exists());
}

/// 점유 중인 터미널에는 입력하지 않고 파일을 회수한다.
#[test]
fn a_hard_occupied_terminal_does_not_receive_the_path() {
    let (_state, mut engine) = crate::state::tests::test_state();
    let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
    let input = terminal_with_sink(&mut engine, sid);
    let target = SurfaceBinding::capture(&engine.read(), sid).expect("binding");
    engine
        .borrow_mut()
        .live
        .occupancy
        .acquire(sid, 1)
        .expect("occupied");

    let saved = paste::save("paste-5.png", b"png").expect("saved");
    let dir = saved.path().parent().expect("own dir").to_path_buf();
    let delivery = paste::deliver(&mut engine.borrow_mut(), &request(target, false), saved);
    assert_eq!(delivery, PasteDelivery::HardOccupied);
    assert!(input.try_recv().is_err());
    assert!(!dir.exists());
}

/// 붙여넣기 실행은 파일을 만들거나 경로를 입력하지 않고 저장 요청만 남긴다.
#[test]
fn applying_a_local_image_paste_leaves_file_work_to_the_app() {
    let (_state, mut engine) = crate::state::tests::test_state();
    let sid = engine.read().workspace_at(0).unwrap().all_surface_ids()[0];
    let input = terminal_with_sink(&mut engine, sid);
    let target = SurfaceBinding::capture(&engine.read(), sid).expect("binding");
    crate::app::engine_action::EngineAction::PasteImage {
        target,
        view: std::sync::Weak::new(),
        bracketed: false,
        file_name: "paste-6.png".into(),
        png_bytes: b"png".to_vec(),
    }
    .apply(&mut engine.borrow_mut());
    assert!(
        input.try_recv().is_err(),
        "no path before the file is saved"
    );
    let pending = std::mem::take(&mut engine.borrow_mut().remote.pending_image_saves);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].png_bytes, b"png");
}

fn spool_with(bytes: &[u8]) -> crate::remote::transfer_spool::Spool {
    let mut spool = crate::remote::transfer_spool::Spool::new().expect("spool");
    spool.append(bytes).expect("spool write");
    spool
}

/// 회신은 업로드를 시작한 연결에만 간다. 끊긴 뒤 같은 번호로 새로 붙은 연결은 받지 않는다.
#[test]
fn a_capture_reply_reaches_only_the_uploading_connection() {
    use super::capture::{StoredCapture, reply_stored_capture};
    let clipboard = crate::adapters::test::mock_clipboard::MockClipboard::default();
    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let first = hub.register(7);
    let owner = crate::remote::transfer_spool::TransferOwner::for_connection(
        hub.client_binding(7).expect("registered"),
    );
    let stored = |owner| StoredCapture {
        client_id: 7,
        upload_id: 3,
        owner,
        result: Err("x".into()),
    };
    assert!(reply_stored_capture(
        &hub,
        &clipboard,
        stored(owner.clone())
    ));
    let frame = first.try_recv().expect("reply sent");
    let reply: serde_json::Value = serde_json::from_slice(&frame.payload).expect("json");
    assert_eq!(reply["event"], "capture_result");
    assert_eq!(reply["upload_id"], 3);

    hub.unregister(7);
    let second = hub.register(7);
    assert!(!reply_stored_capture(&hub, &clipboard, stored(owner)));
    assert!(
        second.try_recv().is_err(),
        "a new connection gets no stale reply"
    );
}

fn tiny_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]));
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode");
    png
}

/// 저장·디코딩·게시는 받은 폴더에 하고, 그 결과를 회신 값으로 돌려준다.
#[test]
fn a_stored_capture_is_saved_in_the_given_folder_and_published() {
    use super::capture::{CaptureStage, run_capture_job};
    use crate::ports::clipboard::ClipboardSystem as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let clipboard = crate::adapters::test::mock_clipboard::MockClipboard::default();
    let png = tiny_png();
    let stage = run_capture_job(
        &clipboard,
        Some(dir.path().join("screenshots")),
        "../cap.png",
        spool_with(&png),
        true,
    )
    .expect("stored");
    let CaptureStage::Published(saved) = stage else {
        panic!("published in the worker");
    };
    let path = dir.path().join("screenshots").join("cap.png");
    assert_eq!(saved.path, path.to_string_lossy());
    assert_eq!(std::fs::read(&path).expect("saved"), png);
    assert_eq!(clipboard.read_text().expect("text"), saved.path);
    assert_eq!(clipboard.image().map(|image| image.width), Some(3));

    let missing_home = run_capture_job(&clipboard, None, "c.png", spool_with(&png), true);
    assert!(missing_home.is_err());
}

/// Windows 경로: worker 는 저장·디코딩만 하고 클립보드를 건드리지 않는다. 게시는 회신 직전
/// 이벤트 루프에서 하며, 연결이 바뀌어 회신을 버려도 게시는 한다.
#[test]
fn a_prepared_capture_is_published_on_the_event_loop() {
    use super::capture::{CaptureStage, StoredCapture, reply_stored_capture, run_capture_job};
    use crate::ports::clipboard::ClipboardSystem as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let clipboard = crate::adapters::test::mock_clipboard::MockClipboard::default();
    let stage = run_capture_job(
        &clipboard,
        Some(dir.path().to_path_buf()),
        "cap.png",
        spool_with(&tiny_png()),
        false,
    )
    .expect("prepared");
    assert!(matches!(stage, CaptureStage::Prepared(_)));
    assert!(
        clipboard.read_text().is_err(),
        "the worker does not publish"
    );

    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let connection = hub.register(9);
    let owner = crate::remote::transfer_spool::TransferOwner::for_connection(
        hub.client_binding(9).expect("registered"),
    );
    assert!(reply_stored_capture(
        &hub,
        &clipboard,
        StoredCapture {
            client_id: 9,
            upload_id: 4,
            owner,
            result: Ok(stage),
        },
    ));
    let path = dir.path().join("cap.png");
    assert_eq!(
        clipboard.read_text().expect("published"),
        path.to_string_lossy()
    );
    assert_eq!(clipboard.image().map(|image| image.height), Some(2));
    let frame = connection.try_recv().expect("reply sent");
    let reply: serde_json::Value = serde_json::from_slice(&frame.payload).expect("json");
    assert_eq!(reply["image"], true);
}
