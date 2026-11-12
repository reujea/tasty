//! 단축키 캡처 결과가 어떤 안내가 되는지 확인한다.

use super::*;

#[test]
fn only_a_user_cancel_is_silent() {
    crate::i18n::init("en");
    assert!(capture_failure_toast(&CaptureError::Cancelled).is_none());
    let kind = |err| capture_failure_toast(&err).map(|(_, kind)| kind);
    assert_eq!(
        kind(CaptureError::NoOutput("exited 0".into())),
        Some(ToastKind::Info),
        "취소일 수도 있는 무출력은 오류로 표시하지 않는다"
    );
    assert_eq!(
        kind(CaptureError::PermissionDenied),
        Some(ToastKind::Warning)
    );
    for err in [
        CaptureError::Unavailable("install one of scrot".into()),
        CaptureError::Tool(anyhow::anyhow!("scrot: exit 2")),
        CaptureError::Storage(anyhow::anyhow!("disk full")),
    ] {
        let (message, kind) = capture_failure_toast(&err).expect("실패는 안내한다");
        assert_eq!(kind, ToastKind::Error, "{message}");
    }
}

#[test]
fn the_failure_reason_is_kept_within_the_toast_limit() {
    crate::i18n::init("en");
    let reason = format!("{}TAIL-OS-ERROR", "x".repeat(400));
    let (message, _) = capture_failure_toast(&CaptureError::Tool(anyhow::anyhow!(reason))).unwrap();
    assert!(
        message.chars().count() <= crate::i18n::TOAST_MAX_CHARS,
        "{message}"
    );
    assert!(
        message.ends_with("TAIL-OS-ERROR"),
        "사유의 끝을 남긴다: {message}"
    );
}

#[test]
fn a_partial_copy_is_not_reported_as_success() {
    crate::i18n::init("en");
    let path = std::path::Path::new("/home/u/.tasty/screenshots/shot.png");
    assert_eq!(
        local_copy_toast(&CaptureCopy::TextAndImage, path).1,
        ToastKind::Success
    );
    let (partial, kind) = local_copy_toast(
        &CaptureCopy::TextOnly {
            image_error: "PNG decode failed".into(),
        },
        path,
    );
    assert_eq!(kind, ToastKind::Warning);
    assert!(partial.contains("PNG decode failed"), "{partial}");
    let (failed, kind) = local_copy_toast(
        &CaptureCopy::Failed {
            error: "no clipboard".into(),
        },
        path,
    );
    assert_eq!(kind, ToastKind::Error);
    assert!(
        failed.contains("shot.png"),
        "저장 경로를 알려 준다: {failed}"
    );
}

#[test]
fn admission_refusals_explain_themselves_except_at_shutdown() {
    crate::i18n::init("en");
    assert!(admission_toast(&CaptureAdmissionError::ShuttingDown).is_none());
    assert_eq!(
        admission_toast(&CaptureAdmissionError::Busy).map(|(_, kind)| kind),
        Some(ToastKind::Warning)
    );
    assert_eq!(
        admission_toast(&CaptureAdmissionError::Spawn("EAGAIN".into())).map(|(_, kind)| kind),
        Some(ToastKind::Error)
    );
}

#[test]
fn the_fifth_waiting_capture_is_refused_as_busy() {
    let mut workers = ScreenshotWorkers::default();
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Arc::new(std::sync::Mutex::new(wait));
    for _ in 0..4 {
        let wait = wait.clone();
        workers
            .spawn(move || {
                // 시험이 끝날 때 보낸 신호나 송신자 drop 으로 깨어난다.
                let lock = wait.lock().expect("wait lock");
                if let Err(error) = lock.recv() {
                    tracing::debug!(%error, "release sender dropped");
                }
            })
            .expect("상한 안의 캡처는 시작한다");
    }
    let refused = workers.spawn(|| {});
    assert!(
        matches!(refused, Err(CaptureAdmissionError::Busy)),
        "{refused:?}"
    );
    for _ in 0..4 {
        release.send(()).expect("worker alive");
    }
    drop(release);
    while workers.poll_shutdown() != 0 {
        std::thread::yield_now();
    }
}

/// Given 네이티브 Wayland로 시작했다, When 원격 캡처 전 지문 읽기가 실패한다, Then 예상된 분기라
/// 캡처마다 warn을 남기지 않는다. X11 시작이나 알 수 없는 시작의 실패는 그대로 warn이다.
#[test]
fn a_fingerprint_failure_is_expected_only_on_a_native_wayland_start() {
    use crate::clipboard::error::DisplayBackend;
    assert!(fingerprint_read_fails_by_design(DisplayBackend::Wayland {
        x_failed: false
    }));
    assert!(fingerprint_read_fails_by_design(DisplayBackend::Wayland {
        x_failed: true
    }));
    assert!(!fingerprint_read_fails_by_design(DisplayBackend::Unknown));
    assert!(!fingerprint_read_fails_by_design(DisplayBackend::Other));
}
