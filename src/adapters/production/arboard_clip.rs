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
use crate::ports::clipboard::{ClipboardImage, ClipboardSystem, TextRead};
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
    /// On the event loop of a native Wayland start, a foreign selection is read in the background:
    /// the handle waits in this thread's table until `finish_read_text` takes its result.
    fn begin_read_text(&self) -> TextRead {
        #[cfg(target_os = "linux")]
        if let Some(selection) = crate::file::drag::TextSelection::on_this_thread() {
            return wayland_reads::begin(selection);
        }
        Ok(self.read_text())
    }

    fn finish_read_text(&self, token: u64) -> Option<anyhow::Result<String>> {
        #[cfg(target_os = "linux")]
        return wayland_reads::finish(token);
        #[cfg(not(target_os = "linux"))]
        Some(Err(anyhow::anyhow!("no pending clipboard read {token}")))
    }

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

/// Reads of a foreign Wayland selection still waiting, keyed by token. The selection handles are
/// not `Send`, so they stay on the event loop thread that started them.
#[cfg(target_os = "linux")]
mod wayland_reads {
    use crate::file::drag::{SelectionPaste, SelectionReceived, TextSelection};
    use crate::ports::clipboard::TextRead;
    use std::{cell::RefCell, collections::HashMap};

    thread_local! {
        static READS: RefCell<(u64, HashMap<u64, TextSelection>)> =
            RefCell::new((0, HashMap::new()));
    }

    pub(super) fn begin(mut selection: TextSelection) -> TextRead {
        let token = READS.with_borrow_mut(|(last, _)| {
            *last += 1;
            *last
        });
        match selection.request_text(token) {
            Ok(SelectionPaste::Ready(text)) => Ok(Ok(text)),
            Ok(SelectionPaste::Empty) => Ok(Err(anyhow::anyhow!("clipboard has no text"))),
            Ok(SelectionPaste::Pending) => {
                READS.with_borrow_mut(|(_, reads)| reads.insert(token, selection));
                Err(token)
            }
            Err(error) => Ok(Err(anyhow::anyhow!(error))),
        }
    }

    pub(super) fn finish(token: u64) -> Option<anyhow::Result<String>> {
        let outcome = READS.with_borrow_mut(|(_, reads)| {
            let Some(selection) = reads.get_mut(&token) else {
                return Some(Err(anyhow::anyhow!("no pending clipboard read {token}")));
            };
            let received = match selection.poll_received() {
                Ok(received) => received,
                Err(error) => return Some(Err(anyhow::anyhow!(error))),
            };
            settle(token, received, selection.waiting())
        });
        if outcome.is_some() {
            READS.with_borrow_mut(|(_, reads)| reads.remove(&token));
        }
        outcome
    }

    /// The result for `token` among the handle's finished reads; a read that no longer waits
    /// without one was replaced by a newer paste.
    pub(super) fn settle(
        token: u64,
        received: Vec<(u64, SelectionReceived)>,
        waiting: bool,
    ) -> Option<anyhow::Result<String>> {
        match received.into_iter().find(|(tag, _)| *tag == token) {
            Some((_, SelectionReceived::Text(text))) => Some(Ok(text)),
            Some((_, SelectionReceived::Failed(reason))) => Some(Err(anyhow::anyhow!(reason))),
            Some((_, SelectionReceived::Png(_))) => {
                Some(Err(anyhow::anyhow!("clipboard has no text")))
            }
            None if waiting => None,
            None => Some(Err(anyhow::anyhow!("replaced by a newer clipboard read"))),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use SelectionReceived as R;

        #[test]
        fn an_osc52_read_settles_only_with_its_own_result() {
            assert!(settle(2, vec![], true).is_none());
            assert!(settle(2, vec![(1, R::Text("old".into()))], true).is_none());
            assert_eq!(
                settle(2, vec![(2, R::Text("new".into()))], false).map(|r| r.unwrap()),
                Some("new".to_string())
            );
            assert!(
                settle(2, vec![(2, R::Failed("timeout".into()))], false)
                    .unwrap()
                    .is_err()
            );
            // Replaced by a newer paste: finished with an error, so the query is not kept forever.
            assert!(settle(2, vec![], false).unwrap().is_err());
        }

        #[test]
        fn an_unknown_token_is_finished_not_waited_for() {
            assert!(finish(999).unwrap().is_err());
        }
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
