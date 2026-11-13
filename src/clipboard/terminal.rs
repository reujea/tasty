//! One window's terminal clipboard. On a native Wayland start the window's Wayland selection handle
//! (the shared data-device owner that egui text fields and incoming drags also use) carries copies
//! and pastes; everywhere else, and on a Wayland window whose compositor gave no data device, the
//! arboard path stays. The primary selection (vi `"*`) always uses arboard.
//!
//! A foreign Wayland selection is read without blocking the winit thread, so a paste may finish
//! later through `poll_paste`. The caller checks that its target is still the same.

use super::context::{ClipboardContext, PasteContent};
#[cfg(target_os = "linux")]
use super::error::ClipboardError;
use super::error::DisplayBackend;

pub(crate) struct TerminalClipboard {
    host: ClipboardContext,
    #[cfg(target_os = "linux")]
    wayland: Option<WaylandPaste>,
}

#[cfg(target_os = "linux")]
struct WaylandPaste {
    selection: crate::file::drag::TextSelection,
    last_tag: u64,
    /// The tag of the paste still being read.
    pending: Option<u64>,
}

/// How a paste request went.
pub(crate) enum PasteStart {
    /// The content (or the failure) is known now.
    Now(Result<Option<PasteContent>, super::error::ClipboardError>),
    /// A foreign Wayland selection is being read; the result comes from `poll_paste`.
    #[cfg(target_os = "linux")]
    Pending,
}

impl TerminalClipboard {
    pub(crate) fn new(
        window: &winit::window::Window,
        #[cfg_attr(
            not(target_os = "linux"),
            expect(unused_variables, reason = "only Linux reads a Wayland selection")
        )]
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Self {
        let display = DisplayBackend::of(window);
        #[cfg(target_os = "linux")]
        let wayland =
            crate::file::drag::TextSelection::new(window, display_owner).map(|selection| {
                WaylandPaste {
                    selection,
                    last_tag: 0,
                    pending: None,
                }
            });
        Self {
            host: ClipboardContext::new(display),
            #[cfg(target_os = "linux")]
            wayland,
        }
    }

    #[cfg(test)]
    pub(crate) fn unavailable(error: super::error::ClipboardError) -> Self {
        Self {
            host: ClipboardContext::unavailable(error),
            #[cfg(target_os = "linux")]
            wayland: None,
        }
    }

    /// The window copies through its Wayland selection rather than arboard.
    pub(crate) fn uses_wayland_selection(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.wayland.is_some();
        #[cfg(not(target_os = "linux"))]
        false
    }

    pub(crate) fn set_text(&mut self, text: &str) -> Result<(), super::error::ClipboardError> {
        #[cfg(target_os = "linux")]
        if let Some(wayland) = self.wayland.as_mut() {
            return wayland
                .selection
                .copy_checked(text.to_string())
                .map_err(ClipboardError::TransferFailed);
        }
        self.host.set_text(text)
    }

    /// The primary selection has no Wayland owner here, so it stays on arboard. Its native Wayland
    /// failure names only `"*` when ordinary copies still reach other apps.
    pub(crate) fn set_text_primary(
        &mut self,
        text: &str,
    ) -> Result<(), super::error::ClipboardError> {
        let has_selection = self.uses_wayland_selection();
        self.host
            .set_text_primary(text)
            .map_err(|error| error.for_primary(has_selection))
    }

    /// Starts a paste. A newer paste replaces one still being read.
    pub(crate) fn start_paste(&mut self) -> PasteStart {
        #[cfg(target_os = "linux")]
        if let Some(wayland) = self.wayland.as_mut() {
            use crate::file::drag::SelectionPaste;
            wayland.last_tag += 1;
            let tag = wayland.last_tag;
            wayland.pending = None;
            return match wayland.selection.request_terminal(tag) {
                Ok(SelectionPaste::Ready(text)) => {
                    PasteStart::Now(Ok(Some(PasteContent::Text(text)).filter(has_content)))
                }
                Ok(SelectionPaste::Empty) => PasteStart::Now(Ok(None)),
                Ok(SelectionPaste::Pending) => {
                    wayland.pending = Some(tag);
                    PasteStart::Pending
                }
                Err(error) => PasteStart::Now(Err(ClipboardError::TransferFailed(error))),
            };
        }
        PasteStart::Now(self.host.read_paste())
    }

    /// The result of the paste still being read, once it is known. `None` while it waits, and
    /// also when a newer paste (from any window or text field) replaced it: that one is dropped
    /// silently.
    pub(crate) fn poll_paste(
        &mut self,
    ) -> Option<Result<Option<PasteContent>, super::error::ClipboardError>> {
        #[cfg(target_os = "linux")]
        if let Some(wayland) = self.wayland.as_mut() {
            let tag = wayland.pending?;
            let received = match wayland.selection.poll_received() {
                Ok(received) => received,
                Err(error) => {
                    wayland.pending = None;
                    return Some(Err(ClipboardError::TransferFailed(error)));
                }
            };
            let outcome = finish(tag, received, wayland.selection.waiting());
            if !matches!(outcome, Finish::Waiting) {
                wayland.pending = None;
            }
            return match outcome {
                Finish::Waiting => None,
                Finish::Replaced => {
                    tracing::debug!("terminal paste replaced by a newer paste");
                    None
                }
                Finish::Done(result) => Some(result),
            };
        }
        None
    }

    /// Runs the Wayland selection's queue for a window whose loop does not poll text selections
    /// (the home-in-use screen). True while copied text is still being sent, so the caller polls
    /// again soon. Without a Wayland selection there is nothing to run.
    pub(crate) fn pump_selection(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        if let Some(wayland) = self.wayland.as_mut() {
            wayland.selection.pump();
            return wayland.selection.busy();
        }
        false
    }

    /// A paste is being read; the window must keep polling.
    pub(crate) fn paste_pending(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.wayland.as_ref().is_some_and(|w| w.pending.is_some());
        #[cfg(not(target_os = "linux"))]
        false
    }

    /// The clipboard image now, for a mirror paste of a local capture path. On a Wayland selection
    /// only one this process holds is known without waiting: a capture published with its PNG
    /// gives that image. Another app's selection is not read here, so the path text stays.
    pub(crate) fn get_image(
        &mut self,
    ) -> Result<Option<arboard::ImageData<'static>>, super::error::ClipboardError> {
        #[cfg(target_os = "linux")]
        if let Some(wayland) = self.wayland.as_mut() {
            let png = wayland
                .selection
                .own_png()
                .map_err(ClipboardError::TransferFailed)?;
            return own_image(png);
        }
        self.host.get_image()
    }
}

/// The image of our own selection's PNG; no PNG, no image.
#[cfg(target_os = "linux")]
fn own_image(
    png: Option<std::sync::Arc<[u8]>>,
) -> Result<Option<arboard::ImageData<'static>>, ClipboardError> {
    let Some(png) = png else {
        return Ok(None);
    };
    super::capture_image::decode_png(png.to_vec())
        .map(|image| {
            Some(arboard::ImageData {
                width: image.width as usize,
                height: image.height as usize,
                bytes: image.rgba.into(),
            })
        })
        .map_err(ClipboardError::TransferFailed)
}

/// An empty text pastes nothing, as on the arboard path.
#[cfg(target_os = "linux")]
fn has_content(content: &PasteContent) -> bool {
    !matches!(content, PasteContent::Text(text) if text.is_empty())
}

#[cfg(target_os = "linux")]
enum Finish {
    Waiting,
    Replaced,
    Done(Result<Option<PasteContent>, ClipboardError>),
}

/// Picks the result for the paste `tag` from what the selection finished. Results for older tags
/// (pastes already replaced) are ignored.
#[cfg(target_os = "linux")]
fn finish(
    tag: u64,
    received: Vec<(u64, crate::file::drag::SelectionReceived)>,
    waiting: bool,
) -> Finish {
    use crate::file::drag::SelectionReceived;
    let Some((_, received)) = received.into_iter().rev().find(|(t, _)| *t == tag) else {
        return if waiting {
            Finish::Waiting
        } else {
            Finish::Replaced
        };
    };
    Finish::Done(match received {
        SelectionReceived::Text(text) => Ok(Some(PasteContent::Text(text)).filter(has_content)),
        SelectionReceived::Png(png) => super::capture_image::decode_png(png)
            .map(|image| {
                Some(PasteContent::Image(arboard::ImageData {
                    width: image.width as usize,
                    height: image.height as usize,
                    bytes: image.rgba.into(),
                }))
            })
            .map_err(ClipboardError::TransferFailed),
        SelectionReceived::Failed(reason) => Err(ClipboardError::TransferFailed(reason)),
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::file::drag::SelectionReceived as R;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]));
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode");
        out
    }

    #[test]
    fn only_the_result_for_the_current_paste_is_delivered() {
        assert!(matches!(finish(2, vec![], true), Finish::Waiting));
        // A result for an older paste does not finish the current one.
        assert!(matches!(
            finish(2, vec![(1, R::Text("old".into()))], true),
            Finish::Waiting
        ));
        // Nothing for us and nothing waiting: a newer paste replaced ours.
        assert!(matches!(
            finish(2, vec![(1, R::Text("old".into()))], false),
            Finish::Replaced
        ));
        assert!(matches!(
            finish(2, vec![(1, R::Text("old".into())), (2, R::Text("new".into()))], false),
            Finish::Done(Ok(Some(PasteContent::Text(text)))) if text == "new"
        ));
    }

    #[test]
    fn a_failed_read_is_a_transfer_failure_and_empty_text_pastes_nothing() {
        assert!(matches!(
            finish(1, vec![(1, R::Failed("timeout".into()))], false),
            Finish::Done(Err(ClipboardError::TransferFailed(reason))) if reason == "timeout"
        ));
        assert!(matches!(
            finish(1, vec![(1, R::Text(String::new()))], false),
            Finish::Done(Ok(None))
        ));
    }

    #[test]
    fn a_png_selection_becomes_the_same_image_as_the_arboard_path() {
        let Finish::Done(Ok(Some(PasteContent::Image(image)))) =
            finish(1, vec![(1, R::Png(png(3, 2)))], false)
        else {
            panic!("image expected");
        };
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(&image.bytes[..4], &[1, 2, 3, 255]);
        assert!(matches!(
            finish(1, vec![(1, R::Png(b"not a png".to_vec()))], false),
            Finish::Done(Err(ClipboardError::TransferFailed(_)))
        ));
    }

    /// Given 이 프로세스가 캡처를 PNG와 함께 게시했다, When mirror 붙여넣기가 이미지를 묻는다, Then
    /// X11과 같이 그 이미지를 받는다. 자기 PNG가 없으면(다른 앱 소유) 없음.
    #[test]
    fn our_own_capture_png_is_the_mirror_paste_image() {
        let image = own_image(Some(std::sync::Arc::from(png(3, 2))))
            .expect("decode")
            .expect("image");
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(&image.bytes[..4], &[1, 2, 3, 255]);
        assert!(own_image(None).expect("no png").is_none());
    }

    #[test]
    fn a_window_without_a_wayland_selection_keeps_the_host_clipboard() {
        let error = ClipboardError::BackendUnavailable("fixture".into())
            .for_display(DisplayBackend::Wayland { x_failed: false });
        let mut clipboard = TerminalClipboard::unavailable(error.clone());
        assert!(!clipboard.uses_wayland_selection());
        assert_eq!(clipboard.set_text("x"), Err(error.clone()));
        // No selection owner: nothing reaches other apps, so "* keeps the terminal key.
        assert_eq!(clipboard.set_text_primary("x"), Err(error.clone()));
        assert!(matches!(
            clipboard.start_paste(),
            PasteStart::Now(Err(e)) if e == error
        ));
        assert!(clipboard.poll_paste().is_none());
        assert!(!clipboard.paste_pending());
    }
}
