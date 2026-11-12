//! 화면은 항상 로컬에서 캡처하고, 처음 요청할 때 정한 로컬·mirror 클립보드로 결과를 보낸다.
//! 사용자의 선택을 기다리는 OS 캡처는 워커에서 실행한다.

#[cfg(test)]
mod tests;

use winit::window::WindowId;

use crate::adapters::ui::ToastKind;
use crate::app::App;
use crate::platform::screen_capture::CaptureError;
use crate::ports::clipboard::CaptureCopy;
use crate::view::ui::View as _;

pub(crate) struct ScreenshotCaptureOutcome {
    pub(crate) engine: crate::runtime::engine_session::EngineId,
    pub(crate) source_view: Option<std::sync::Weak<()>>,
    pub(crate) remote_target: Option<crate::app::attach_client::RemoteTarget>,
    /// 요청의 원 View에 해당하는 창. View 없는 요청은 worker 시작 전에 버린다.
    pub(crate) source_window: Option<WindowId>,
    pub(crate) result: Result<CapturedFile, CaptureError>,
}

/// 워커가 준비한 캡처 파일. 로컬 게시에는 디코딩한 이미지를, mirror 전송에는 파일 바이트를 싣는다.
pub(crate) struct CapturedFile {
    pub(crate) path: std::path::PathBuf,
    pub(crate) payload: CapturePayload,
}

pub(crate) enum CapturePayload {
    /// 로컬 클립보드에 올릴 이미지. 디코딩하지 못했으면 그 사유다.
    Local(Result<crate::ports::clipboard::ClipboardImage, String>),
    /// 원격에 올릴 파일 바이트와 캡처 전의 로컬 클립보드 지문. 읽지 못했으면 지문이 없다.
    Mirror {
        bytes: Vec<u8>,
        clipboard_before: Option<u64>,
    },
}

impl App {
    pub(crate) fn poll_screenshot_captures(&mut self) {
        self.trigger_pending_screenshot_captures();
        self.drain_screenshot_capture_results();
    }

    fn trigger_pending_screenshot_captures(&mut self) {
        self.screenshot_workers.reap();
        let mut reqs = Vec::new();
        for session in self.engines.all_sessions_mut() {
            reqs.extend(
                std::mem::take(&mut session.remote.pending_screenshot_captures)
                    .into_iter()
                    .map(|workspace| (session.id, workspace)),
            );
        }
        for (engine, (mirror_ws_id, origin_view)) in reqs {
            self.start_screenshot_capture(engine, mirror_ws_id, origin_view);
        }
    }

    fn screenshot_view_is_current(
        &self,
        source_window: Option<WindowId>,
        origin_view: &std::sync::Weak<()>,
    ) -> bool {
        source_window
            .and_then(|window| self.view.views.get(&window))
            .and_then(|view| view.as_main())
            .is_some_and(|main| main.base.state.matches_identity(origin_view))
    }

    fn start_screenshot_capture(
        &mut self,
        engine: crate::runtime::engine_session::EngineId,
        mirror_ws_id: Option<u32>,
        origin_view: std::sync::Weak<()>,
    ) {
        let source_window = self.engines.window_of(engine);
        if !self.screenshot_view_is_current(source_window, &origin_view) {
            tracing::debug!("discarding screenshot request from retired View");
            return;
        }
        let source_view = Some(origin_view.clone());

        let remote_target =
            mirror_ws_id.and_then(|workspace| self.capture_remote_target(workspace, None));
        if mirror_ws_id.is_some() && remote_target.is_none() {
            tracing::debug!("capture mirror disappeared before start");
            self.toast_screenshot_origin(
                &origin_view,
                crate::i18n::t("attach.toast.mirror_capture_unavailable").to_string(),
                ToastKind::Warning,
            );
            return;
        }
        self.spawn_screenshot_capture(
            engine,
            source_view,
            remote_target,
            mirror_ws_id,
            source_window,
        );
    }

    fn spawn_screenshot_capture(
        &mut self,
        engine: crate::runtime::engine_session::EngineId,
        source_view: Option<std::sync::Weak<()>>,
        remote_target: Option<crate::app::attach_client::RemoteTarget>,
        mirror_ws_id: Option<u32>,
        source_window: Option<WindowId>,
    ) {
        let tx = self.screenshot_capture_tx.clone();
        let proxy = self.view.proxy.clone();
        let origin = source_view.clone();
        if let Err(error) = self.screenshot_workers.spawn(move || {
            let result = capture_and_maybe_read(mirror_ws_id.is_some());
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            // 수신자가 사라지면 캡처 결과를 전달할 곳이 없어 오류를 무시한다.
            let _ = tx.send(ScreenshotCaptureOutcome {
                engine,
                source_view,
                remote_target,
                source_window,
                result,
            });
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            // 이벤트 루프가 끝났으면 깨우기 실패를 무시한다.
            let _ = proxy.send_event(crate::AppEvent::ScreenshotCaptureReady);
        }) {
            tracing::warn!(%error, "screenshot capture admission failed");
            if let (Some(origin), Some((message, kind))) = (origin, admission_toast(&error)) {
                self.toast_screenshot_origin(&origin, message, kind);
            }
        }
    }

    pub(crate) fn drain_screenshot_capture_results(&mut self) {
        while let Ok(outcome) = self.screenshot_capture_rx.try_recv() {
            let ScreenshotCaptureOutcome {
                engine,
                source_view,
                remote_target,
                source_window,
                result,
            } = outcome;
            if self.engines.get(engine).is_none()
                || source_window.is_some_and(|window| {
                    self.engines.of_window(window) != Some(engine)
                        || self
                            .view
                            .views
                            .get(&window)
                            .and_then(|view| view.as_main())
                            .is_none_or(|main| {
                                source_view.as_ref().is_none_or(|identity| {
                                    !main.base.state.matches_identity(identity)
                                })
                            })
                })
            {
                tracing::debug!("discarding screenshot result for retired origin");
                continue;
            }
            // 시작 전에 View 없는 요청을 버리므로 여기 도달한 결과에는 원 View가 있다.
            let Some(origin) = source_view else {
                continue;
            };
            let CapturedFile { path, payload } = match result {
                Ok(v) => v,
                Err(err) => {
                    self.report_capture_failure(&origin, err);
                    continue;
                }
            };
            match payload {
                CapturePayload::Local(image) => {
                    let copy = self.write_capture_to_local_clipboard(&path, image);
                    let (message, kind) = local_copy_toast(&copy, &path);
                    self.toast_screenshot_origin(&origin, message, kind);
                }
                CapturePayload::Mirror {
                    bytes,
                    clipboard_before,
                } => {
                    self.send_capture_to_mirror(
                        &origin,
                        remote_target,
                        &path,
                        bytes,
                        clipboard_before,
                    );
                }
            }
        }
    }

    /// 보낸 업로드는 원격 회신이나 연결 종료 안내가 결과를 알린다. 보내지 못했으면 여기서 알린다.
    fn send_capture_to_mirror(
        &mut self,
        origin: &std::sync::Weak<()>,
        remote_target: Option<crate::app::attach_client::RemoteTarget>,
        path: &std::path::Path,
        bytes: Vec<u8>,
        clipboard_before: Option<u64>,
    ) {
        let sent = match remote_target.filter(|target| self.remote_target_is_current(target)) {
            Some(target) => {
                let sent = self.upload_capture_to_mirror(&target, path, bytes, origin.clone());
                if sent {
                    self.remember_mirror_capture(&target, clipboard_before);
                }
                sent
            }
            None => {
                tracing::debug!("capture remote connection retired before upload");
                false
            }
        };
        if !sent {
            self.toast_screenshot_origin(
                origin,
                crate::i18n::t_fmt_fit(
                    "attach.toast.mirror_capture_not_sent",
                    &path.display().to_string(),
                ),
                ToastKind::Warning,
            );
        }
    }

    /// 이전 캡처의 경로는 새 캡처가 대신한다. 지문이 없으면 변경을 판정할 수 없어 기록하지 않는다.
    fn remember_mirror_capture(
        &mut self,
        target: &crate::app::attach_client::RemoteTarget,
        clipboard_before: Option<u64>,
    ) {
        let Some(engine) = self
            .engines
            .all_sessions_mut()
            .find(|session| session.id == target.engine)
        else {
            return;
        };
        match clipboard_before {
            Some(clipboard_before) => {
                engine.remote.mirror_capture_pastes.insert(
                    target.workspace,
                    crate::remote::state::MirrorCapturePaste {
                        clipboard_before,
                        remote_path: None,
                    },
                );
            }
            None => {
                engine
                    .remote
                    .mirror_capture_pastes
                    .remove(&target.workspace);
            }
        }
    }

    /// 사용자 취소만 조용히 끝낸다. 나머지는 원 View에 사유를 알린다.
    fn report_capture_failure(&mut self, origin: &std::sync::Weak<()>, err: CaptureError) {
        log_capture_failure(&err);
        if let Some((message, kind)) = capture_failure_toast(&err) {
            self.toast_screenshot_origin(origin, message, kind);
        }
    }

    /// 단축키 요청의 결과는 요청한 View에만 알린다. 그 사이 포커스가 다른 창으로 옮겨도 같다.
    /// 그 View가 닫혔으면 다른 창을 대신 고르지 않고 로그만 남긴다.
    pub(crate) fn toast_screenshot_origin(
        &mut self,
        origin: &std::sync::Weak<()>,
        message: String,
        kind: ToastKind,
    ) {
        let main = self
            .view
            .views
            .values_mut()
            .filter_map(|view| view.as_main_mut())
            .find(|main| main.base.state.matches_identity(origin));
        let Some(main) = main else {
            tracing::debug!("screenshot result for a closed View: {message}");
            return;
        };
        main.state
            .toasts
            .push(message, kind, crate::adapters::ui::ToastScope::Window);
        main.mark_dirty();
    }

    /// 회신을 받을 수 없게 된 원격 캡처 업로드를 요청한 View에 알린다.
    pub(crate) fn abandon_capture_uploads(
        &mut self,
        uploads: std::collections::HashMap<u64, std::sync::Weak<()>>,
    ) {
        for origin in uploads.into_values() {
            self.toast_screenshot_origin(
                &origin,
                crate::i18n::t("attach.toast.mirror_capture_unconfirmed").to_string(),
                ToastKind::Warning,
            );
        }
    }

    /// 경로 텍스트와 이미지를 함께 올린다. 이미지를 올리지 못하면 경로만 올리고 그 결과를 구분한다.
    fn write_capture_to_local_clipboard(
        &mut self,
        path: &std::path::Path,
        image: Result<crate::ports::clipboard::ClipboardImage, String>,
    ) -> CaptureCopy {
        let path_str = path.to_string_lossy().to_string();
        let clipboard = self.services.clipboard_arc();
        let outcome = crate::ports::clipboard::publish_text_with_image(
            clipboard.as_ref(),
            &path_str,
            image.as_ref().map_err(String::as_str),
        );
        if outcome != CaptureCopy::TextAndImage {
            tracing::warn!("screenshot capture: local clipboard publish incomplete: {outcome:?}");
        }
        outcome
    }

    /// 업로드를 원격에 모두 넘겼으면 true 다.
    fn upload_capture_to_mirror(
        &mut self,
        target: &crate::app::attach_client::RemoteTarget,
        path: &std::path::Path,
        bytes: Vec<u8>,
        origin: std::sync::Weak<()>,
    ) -> bool {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "screenshot.png".to_string());
        match self.forward_capture_to_remote_clipboard(target, &file_name, &bytes, origin) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    "screenshot capture: forward to mirror workspace {} failed: {e}",
                    target.workspace
                );
                false
            }
        }
    }
}

/// 캡처 실패 안내. 사용자 취소는 안내하지 않는다. 파일 없이 끝났지만 취소인지 알 수 없는
/// 경우(일부 도구는 취소해도 성공 코드로 끝난다)는 오류가 아니라 정보로 알린다.
fn capture_failure_toast(err: &CaptureError) -> Option<(String, ToastKind)> {
    use crate::i18n::{t, t_fmt_fit};
    Some(match err {
        CaptureError::Cancelled => return None,
        CaptureError::PermissionDenied => (
            t("toast.screen_recording_permission_required").to_string(),
            ToastKind::Warning,
        ),
        CaptureError::NoOutput(_) => (t("toast.screenshot_not_taken").to_string(), ToastKind::Info),
        CaptureError::Unavailable(detail) => (
            t_fmt_fit("toast.screenshot_tool_unavailable", detail),
            ToastKind::Error,
        ),
        CaptureError::Tool(err) => (
            t_fmt_fit("toast.screenshot_tool_failed", &format!("{err:#}")),
            ToastKind::Error,
        ),
        CaptureError::Storage(err) => (
            t_fmt_fit("toast.screenshot_save_failed", &format!("{err:#}")),
            ToastKind::Error,
        ),
    })
}

/// 로컬 클립보드에 실제로 올라간 범위를 알린다. 경로만 올렸으면 성공으로 표시하지 않는다.
fn local_copy_toast(copy: &CaptureCopy, path: &std::path::Path) -> (String, ToastKind) {
    use crate::i18n::{t, t_fmt_fit};
    match copy {
        CaptureCopy::TextAndImage => (t("toast.screenshot_copied").to_string(), ToastKind::Success),
        CaptureCopy::TextOnly { image_error } => (
            t_fmt_fit("toast.screenshot_path_only", image_error),
            ToastKind::Warning,
        ),
        CaptureCopy::Failed { .. } => (
            t_fmt_fit("toast.screenshot_copy_failed", &path.display().to_string()),
            ToastKind::Error,
        ),
    }
}

/// 앱 종료 중의 거절은 안내하지 않는다.
fn admission_toast(error: &CaptureAdmissionError) -> Option<(String, ToastKind)> {
    match error {
        CaptureAdmissionError::ShuttingDown => None,
        CaptureAdmissionError::Busy => Some((
            crate::i18n::t("toast.screenshot_busy").to_string(),
            ToastKind::Warning,
        )),
        CaptureAdmissionError::Spawn(reason) => Some((
            crate::i18n::t_fmt_fit("toast.screenshot_start_failed", reason),
            ToastKind::Error,
        )),
    }
}

/// 캡처 워커를 시작하지 못한 이유.
#[derive(Debug)]
pub(crate) enum CaptureAdmissionError {
    ShuttingDown,
    /// 사용자 선택을 기다리는 캡처가 이미 상한만큼 진행 중이다.
    Busy,
    Spawn(String),
}

impl std::fmt::Display for CaptureAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShuttingDown => write!(f, "screenshot capture is shutting down"),
            Self::Busy => write!(f, "screenshot capture capacity exhausted"),
            Self::Spawn(reason) => write!(f, "screenshot capture thread failed: {reason}"),
        }
    }
}

/// 사용자 취소와 취소인지 알 수 없는 무출력은 debug, 나머지는 warn 이다.
fn log_capture_failure(err: &CaptureError) {
    if matches!(err, CaptureError::Cancelled | CaptureError::NoOutput(_)) {
        tracing::debug!("screenshot capture ended without a file: {err}");
    } else {
        tracing::warn!("screenshot capture failed: {err}");
    }
}

/// 캡처한 파일을 읽어 목적지에 맞게 준비한다. 큰 그림의 디코딩도 여기(워커)에서 한다.
fn capture_and_maybe_read(for_mirror: bool) -> Result<CapturedFile, CaptureError> {
    // 사용자가 영역을 고르는 동안 복사한 내용을 캡처 이전 상태로 잘못 기록하지 않게 먼저 읽는다.
    let clipboard_before = for_mirror.then(local_clipboard_fingerprint).flatten();
    let path = crate::platform::screen_capture::capture_interactive()?;
    // 캡처 뒤 파일 읽기 실패는 사용자 취소·캡처 권한 거절과 구별한다.
    let bytes = std::fs::read(&path).map_err(|e| CaptureError::Storage(e.into()))?;
    let payload = if for_mirror {
        CapturePayload::Mirror {
            bytes,
            clipboard_before,
        }
    } else {
        CapturePayload::Local(crate::clipboard::capture_image::decode_png(bytes))
    };
    Ok(CapturedFile { path, payload })
}

/// 읽지 못하면 없음으로 돌려준다. 이 캡처만 붙여넣기 대상에서 빠지고 전송은 그대로 진행한다.
fn local_clipboard_fingerprint() -> Option<u64> {
    let mut clipboard =
        crate::clipboard::ClipboardContext::new(crate::clipboard::error::DisplayBackend::Unknown);
    match clipboard.read_paste() {
        Ok(content) => Some(crate::clipboard::context::paste_fingerprint(
            content.as_ref(),
        )),
        Err(error) => {
            if fingerprint_read_fails_by_design(crate::clipboard::error::DisplayBackend::at_start())
            {
                tracing::debug!(
                    "screenshot capture: no local clipboard fingerprint on a native Wayland start: {error}"
                );
            } else {
                tracing::warn!("screenshot capture: local clipboard read failed: {error}");
            }
            None
        }
    }
}

/// The fingerprint reads through arboard, which cannot read a native Wayland selection; there
/// every read fails and is an expected branch, not a fault worth a warning per capture.
fn fingerprint_read_fails_by_design(start: crate::clipboard::error::DisplayBackend) -> bool {
    matches!(
        start,
        crate::clipboard::error::DisplayBackend::Wayland { .. }
    )
}

/// Native capture may wait for the user. No timeout is treated as completion or OS cancellation.
#[derive(Default)]
pub(crate) struct ScreenshotWorkers {
    jobs: Vec<std::thread::JoinHandle<()>>,
    stopping: bool,
}
impl ScreenshotWorkers {
    fn spawn(&mut self, work: impl FnOnce() + Send + 'static) -> Result<(), CaptureAdmissionError> {
        if self.stopping {
            return Err(CaptureAdmissionError::ShuttingDown);
        }
        if self.jobs.len() >= 4 {
            return Err(CaptureAdmissionError::Busy);
        }
        let job = std::thread::Builder::new()
            .name("screenshot-capture".into())
            .spawn(work)
            .map_err(|error| CaptureAdmissionError::Spawn(error.to_string()))?;
        self.jobs.push(job);
        Ok(())
    }
    fn reap(&mut self) {
        let mut pending = Vec::new();
        for job in std::mem::take(&mut self.jobs) {
            if job.is_finished() {
                if job.join().is_err() {
                    tracing::warn!("screenshot capture worker panicked");
                }
            } else {
                pending.push(job);
            }
        }
        self.jobs = pending;
    }
    pub(crate) fn has_pending(&self) -> bool {
        !self.jobs.is_empty()
    }
    pub(crate) fn begin_shutdown(&mut self) {
        self.stopping = true;
    }
    pub(crate) fn poll_shutdown(&mut self) -> usize {
        self.begin_shutdown();
        self.reap();
        self.jobs.len()
    }
}
impl Drop for ScreenshotWorkers {
    fn drop(&mut self) {
        let remaining = self.poll_shutdown();
        if remaining != 0 {
            tracing::warn!(
                remaining,
                "screenshot capture workers still unjoined at owner drop"
            );
        }
    }
}
