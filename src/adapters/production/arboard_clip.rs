//! Application-owned clipboard; retain the selection owner across RPC/OSC52 requests.
//!
//! On a native Wayland start, a call on the event loop thread (OSC 52, IPC, a local capture, a
//! remote capture published there) writes the Wayland selection that the windows' data device owner
//! holds; arboard there speaks only X11. Other threads cannot reach that owner and keep arboard, so
//! the remote capture job leaves its publish to the event loop when [`publishes_on_this_thread`]
//! says the selection lives here.

use crate::clipboard::{
    ClipboardContext,
    error::{ClipboardError, DisplayBackend},
};
use crate::ports::clipboard::{ClipboardImage, ClipboardSystem};
use std::sync::Mutex;

#[derive(Default)]
pub struct ArboardClipboard {
    clipboard: Mutex<Option<ClipboardContext>>,
}

impl ArboardClipboard {
    fn with_clipboard<T>(
        &self,
        operation: impl FnOnce(&mut ClipboardContext) -> Result<T, ClipboardError>,
    ) -> anyhow::Result<T> {
        let mut slot = self.clipboard.lock().map_err(|_| {
            ClipboardError::BackendUnavailable("clipboard owner lock poisoned".into())
        })?;
        let clipboard = slot.get_or_insert_with(|| ClipboardContext::new(DisplayBackend::Unknown));
        Ok(operation(clipboard)?)
    }
}

impl ClipboardSystem for ArboardClipboard {
    fn read_text(&self) -> anyhow::Result<String> {
        self.with_clipboard(|cb| cb.get_text())?
            .ok_or_else(|| anyhow::anyhow!("clipboard has no text"))
    }

    fn write_text(&self, text: &str) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(mut selection) = crate::file::drag::TextSelection::on_this_thread() {
            return Ok(selection
                .copy_checked(text.to_string())
                .map_err(ClipboardError::TransferFailed)?);
        }
        self.with_clipboard(|cb| cb.set_text(text))
    }

    /// arboard 의 setter 는 매번 이전 형식을 지워 두 형식을 함께 올리지 못한다. 파일 클립보드와 같은
    /// 플랫폼 소유자로 한 번에 올린다. 이 소유자가 선택을 가져가면 arboard 의 이전 텍스트는 사라진다.
    fn write_text_and_image(&self, text: &str, image: &ClipboardImage) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some(mut selection) = crate::file::drag::TextSelection::on_this_thread() {
            return Ok(selection
                .copy_with_png(text.to_string(), image.png.clone())
                .map_err(ClipboardError::TransferFailed)?);
        }
        Ok(crate::clipboard::files::write_text_and_image(text, image)?)
    }
}

/// The native Wayland selection lives on this thread, so a clipboard write must happen here rather
/// than on a worker. Always false off Linux and outside a native Wayland start.
pub(crate) fn publishes_on_this_thread() -> bool {
    #[cfg(target_os = "linux")]
    return crate::file::drag::TextSelection::on_this_thread().is_some();
    #[cfg(not(target_os = "linux"))]
    false
}
