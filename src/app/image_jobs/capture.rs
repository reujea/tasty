//! attach client 가 올린 스크린샷을 서버에 저장하고 서버 클립보드에 경로·이미지를 올린다.
//! 접수 검사는 이벤트 루프에서, 저장·재읽기·PNG 디코딩은 이미지 worker 에서 한다. 클립보드
//! 게시는 macOS·Linux 는 worker 에서, Windows 는 이벤트 루프에서 한다([`PUBLISH_ON_EVENT_LOOP`]).
//! 네이티브 Wayland 시작이면 Linux 도 이벤트 루프에서 게시한다. Wayland 선택 소유자가 이벤트 루프
//! 스레드에만 있기 때문이다.
//! 회신은 업로드를 시작한 연결이 그대로 붙어 있을 때만 보낸다.

use super::{ImageJobDone, ImageJobRejected};
use crate::remote::server::{PreparedCapture, SavedCapture};
use crate::remote::transfer_spool::{Spool, TransferOwner};
use tasty_ipc::stream_hub::StreamHub;

/// Windows 는 클립보드 게시를 이벤트 루프에서 한다. 소유 창(`clipboard/files/windows.rs`)이
/// 호출 스레드마다 만들어지는데, worker 스레드는 `mpsc::recv` 에서 막혀 창 메시지를 처리하지
/// 않는다. 다른 프로그램의 `EmptyClipboard` 는 소유 창에 `WM_DESTROYCLIPBOARD` 를 보내므로,
/// 메시지를 처리하지 않는 소유 창은 그 프로그램의 복사를 늦추거나 멈출 수 있다. 이벤트 루프의
/// 스레드는 메시지를 처리한다. macOS(`NSPasteboard` 는 main thread 전용이 아니다)와 Linux X11
/// (선택 소유 스레드를 따로 둔다)은 worker 에서 게시한다.
pub(crate) const PUBLISH_ON_EVENT_LOOP: bool = cfg!(windows);

/// worker 가 끝낸 캡처 저장 단계.
pub(crate) enum CaptureStage {
    /// 클립보드까지 올렸다.
    Published(SavedCapture),
    /// 저장·디코딩만 마쳤다. 이벤트 루프가 게시한다.
    Prepared(PreparedCapture),
}

/// worker 가 끝낸 캡처 저장 결과.
pub(crate) struct StoredCapture {
    pub(crate) client_id: u32,
    pub(crate) upload_id: u64,
    pub(crate) owner: TransferOwner,
    pub(crate) result: Result<CaptureStage, String>,
}

/// worker 에서 실행한다. `publish_in_worker` 가 거짓이면 게시를 이벤트 루프에 남긴다.
pub(super) fn run_capture_job(
    clipboard: &dyn crate::ports::clipboard::ClipboardSystem,
    dir: Option<std::path::PathBuf>,
    file_name: &str,
    bytes: Spool,
    publish_in_worker: bool,
) -> Result<CaptureStage, String> {
    let prepared = crate::remote::server::prepare_capture(dir, file_name, bytes)?;
    Ok(if publish_in_worker {
        CaptureStage::Published(crate::remote::server::publish_capture(clipboard, prepared))
    } else {
        CaptureStage::Prepared(prepared)
    })
}

impl crate::app::App {
    /// 접수 검사를 통과한 업로드만 worker 에 넘긴다. 거절은 그 자리에서 회신한다.
    pub(crate) fn store_capture_off_thread(
        &mut self,
        hub: &StreamHub,
        client_id: u32,
        upload_id: u64,
        file_name: String,
        admitted: Result<(TransferOwner, Spool), String>,
    ) {
        let (owner, bytes) = match admitted {
            Ok(admitted) => admitted,
            Err(reason) => {
                crate::remote::server::push_capture_result(hub, client_id, upload_id, Err(reason));
                return;
            }
        };
        let clipboard = self.services.clipboard_arc();
        let dir = crate::remote::server::capture_dir();
        // The worker cannot reach the native Wayland selection; it waits for nothing here.
        let publish_in_worker = !PUBLISH_ON_EVENT_LOOP
            && !crate::adapters::production::arboard_clip::publishes_on_this_thread();
        // 업로드 바이트는 디스크 spool 에 있으므로 메모리 상한에 세지 않는다.
        let submitted = self.image_jobs.submit(0, move || {
            let result = run_capture_job(
                clipboard.as_ref(),
                dir,
                &file_name,
                bytes,
                publish_in_worker,
            );
            ImageJobDone::Capture(StoredCapture {
                client_id,
                upload_id,
                owner,
                result,
            })
        });
        if let Err(error) = submitted {
            tracing::warn!(%error, "capture store not accepted");
            let reason = match error {
                ImageJobRejected::Busy => "server is busy storing other captures".to_string(),
                other => other.to_string(),
            };
            crate::remote::server::push_capture_result(hub, client_id, upload_id, Err(reason));
        }
    }

    pub(super) fn apply_stored_capture(&mut self, stored: StoredCapture) {
        let clipboard = self.services.clipboard_arc();
        if !reply_stored_capture(&self.stream_hub, clipboard.as_ref(), stored) {
            tracing::debug!("capture stored after its upload connection ended; reply dropped");
        }
    }
}

/// 남은 게시를 마친 뒤 업로드를 시작한 연결에만 회신한다. 보냈으면 true 다.
/// 게시는 worker 에서 했을 때와 같이 연결 여부와 관계없이 한다.
pub(super) fn reply_stored_capture(
    hub: &StreamHub,
    clipboard: &dyn crate::ports::clipboard::ClipboardSystem,
    stored: StoredCapture,
) -> bool {
    let StoredCapture {
        client_id,
        upload_id,
        owner,
        result,
    } = stored;
    let result = result.map(|stage| match stage {
        CaptureStage::Published(saved) => saved,
        CaptureStage::Prepared(prepared) => {
            crate::remote::server::publish_capture(clipboard, prepared)
        }
    });
    if !owner.same_connection(hub, client_id) {
        return false;
    }
    crate::remote::server::push_capture_result(hub, client_id, upload_id, result);
    true
}
