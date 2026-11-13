use super::MainView;
use crate::app::command::{DomainIntent, SendPayload};
use crate::runtime::engine_read::EngineRead;

/// bracketed paste 시작·본문·끝을 같은 큐 순서로 보낸다.
/// mirror 입력도 원격 PTY로 전달하므로 이미지 업로드 뒤 원격 경로 삽입에 재사용한다.
pub(crate) fn dispatch_paste(w: &mut MainView, surface_id: u32, bracketed: bool, text: String) {
    if text.is_empty() {
        return;
    }
    if bracketed {
        w.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Bytes(b"\x1b[200~".to_vec()),
            }
            .from_user_shortcut("paste"),
        );
        w.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Text(text),
            }
            .from_user_shortcut("paste"),
        );
        w.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Bytes(b"\x1b[201~".to_vec()),
            }
            .from_user_shortcut("paste"),
        );
    } else {
        w.state.dispatch_intent(
            DomainIntent::SendToSurface {
                surface_id,
                payload: SendPayload::Text(text),
            }
            .from_user_shortcut("paste"),
        );
    }
}

/// The complete paste shares one queued generation check, including both bracket markers.
pub(crate) fn dispatch_bound_paste(
    view: &mut MainView,
    surface_id: u32,
    generation: tasty_terminal::ResourceGeneration,
    bracketed: bool,
    text: String,
) {
    let mut bytes = Vec::new();
    if bracketed {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend_from_slice(text.as_bytes());
    if bracketed {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    view.state.dispatch_intent(
        DomainIntent::SendToSurface {
            surface_id,
            payload: SendPayload::Bound { generation, bytes },
        }
        .from_user_shortcut("paste"),
    );
}

/// A terminal paste whose content is still being read. It is delivered only while the surface
/// that was focused at the key press is still focused and still the same surface.
pub(crate) struct PendingPaste {
    target: Option<crate::runtime::surface_binding::SurfaceBinding>,
    scope: crate::adapters::ui::ToastScope,
}

impl MainView {
    pub fn paste_to_terminal(&mut self, engine: &EngineRead<'_>) {
        let scope = self
            .state
            .focused_surface_id(engine)
            .map(crate::adapters::ui::ToastScope::Surface)
            .unwrap_or(crate::adapters::ui::ToastScope::Window);
        self.pending_terminal_paste = None;
        match self.clipboard.start_paste() {
            crate::clipboard::terminal::PasteStart::Now(result) => {
                self.deliver_paste(engine, result, scope);
            }
            #[cfg(target_os = "linux")]
            crate::clipboard::terminal::PasteStart::Pending => {
                let target = self.state.focused_surface_id(engine).and_then(|sid| {
                    crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
                });
                self.pending_terminal_paste = Some(PendingPaste { target, scope });
            }
        }
    }

    /// Delivers a paste read in the background. A result for a window that lost keyboard focus, or
    /// a surface that is no longer focused or was replaced, is dropped; so is its failure toast.
    pub(crate) fn poll_terminal_paste(&mut self, engine: &EngineRead<'_>) {
        if self.pending_terminal_paste.is_none() {
            return;
        }
        let Some(result) = self.clipboard.poll_paste() else {
            if !self.clipboard.paste_pending() {
                self.pending_terminal_paste = None;
            }
            return;
        };
        let Some(pending) = self.pending_terminal_paste.take() else {
            return;
        };
        if !pending_target_is_current(
            &pending,
            engine,
            self.base.state.focused,
            self.state.focused_surface_id(engine),
        ) {
            tracing::debug!("terminal paste dropped: its window or surface is no longer focused");
            return;
        }
        self.deliver_paste(engine, result, pending.scope);
    }

    /// A terminal paste is being read; the event loop keeps polling.
    pub(crate) fn terminal_paste_pending(&self) -> bool {
        self.pending_terminal_paste.is_some()
    }

    fn deliver_paste(
        &mut self,
        engine: &EngineRead<'_>,
        result: Result<
            Option<crate::clipboard::context::PasteContent>,
            crate::clipboard::error::ClipboardError,
        >,
        scope: crate::adapters::ui::ToastScope,
    ) {
        let content = match result {
            Ok(content) => content,
            Err(error) => {
                self.state.clipboard_error_toast(&error, scope);
                return;
            }
        };
        let mirror = self
            .state
            .focused_surface_id(engine)
            .and_then(|surface| engine.mirror_workspace_of(surface));
        if let Some(path) = mirror.and_then(|workspace| {
            let paste = engine.mirror_capture_paste(workspace)?;
            let now = crate::clipboard::context::paste_fingerprint(content.as_ref());
            paste.path_for(now).map(str::to_string)
        }) {
            self.paste_text_to_focused(engine, path);
            return;
        }
        let Some(content) = content else {
            return;
        };
        let content = if mirror.is_some() {
            self.replace_local_capture_path(content)
        } else {
            content
        };
        let image = match content {
            crate::clipboard::context::PasteContent::Text(text) => {
                self.paste_text_to_focused(engine, text);
                return;
            }
            crate::clipboard::context::PasteContent::Image(image) => image,
        };
        let Some(sid) = self.state.focused_surface_id(engine) else {
            return;
        };
        let Some(bracketed) = self
            .state
            .focused_terminal(&engine.as_ref())
            .map(|t| t.bracketed_paste())
        else {
            return;
        };

        let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        else {
            return;
        };
        match encode_clipboard_image_as_png(&image) {
            Ok(png_bytes) => {
                self.state.dispatch_intent(
                    crate::intent::Intent::Engine(
                        crate::app::engine_action::EngineAction::PasteImage {
                            target,
                            view: self.base.state.identity(),
                            bracketed,
                            file_name: clipboard_image_file_name(),
                            png_bytes,
                        },
                    )
                    .from_user_shortcut("paste"),
                );
                self.last_terminal_paste_at = Some(std::time::Instant::now());
            }
            Err(error) => self.state.clipboard_error_toast(
                &crate::clipboard::error::ClipboardError::TransferFailed(error.to_string()),
                scope,
            ),
        }
    }
}

impl MainView {
    fn paste_text_to_focused(&mut self, engine: &EngineRead<'_>, text: String) {
        let surface_id = self.state.focused_surface_id(engine);
        let bracketed = self
            .state
            .focused_terminal(&engine.as_ref())
            .map(|t| t.bracketed_paste());
        if let (Some(sid), Some(bracketed)) = (surface_id, bracketed) {
            dispatch_paste(self, sid, bracketed, text);
            self.last_terminal_paste_at = Some(std::time::Instant::now());
        }
    }

    /// 로컬 캡처는 이 머신의 저장 경로와 이미지를 함께 올린다. 원격은 그 경로를 읽지 못하므로
    /// mirror에는 텍스트 대신 이미지를 보낸다. 이미지를 읽지 못하면 원래 텍스트를 쓴다.
    fn replace_local_capture_path(
        &mut self,
        content: crate::clipboard::context::PasteContent,
    ) -> crate::clipboard::context::PasteContent {
        use crate::clipboard::context::PasteContent;
        let PasteContent::Text(text) = &content else {
            return content;
        };
        let captures = tasty_utils::path::tasty_home().map(|home| home.join("screenshots"));
        if !captures.is_some_and(|dir| is_capture_path(text, &dir)) {
            return content;
        }
        match self.clipboard.get_image() {
            Ok(Some(image)) => PasteContent::Image(image),
            Ok(None) => content,
            Err(error) => {
                tracing::warn!("mirror paste: capture image read failed: {error}");
                content
            }
        }
    }
}

/// The paste's window still has keyboard focus, and its surface is still the focused one and the
/// same surface generation. A paste starts only from a key press, so its window had focus then. A
/// paste asked with no focused surface stays undeliverable.
fn pending_target_is_current(
    pending: &PendingPaste,
    engine: &EngineRead<'_>,
    window_focused: bool,
    focused: Option<u32>,
) -> bool {
    let target = pending.target.as_ref();
    still_targeted(
        target.map(|t| t.surface_id()),
        window_focused,
        focused,
        || target.is_some_and(|t| t.current_in(engine)),
    )
}

fn still_targeted(
    asked: Option<u32>,
    window_focused: bool,
    focused: Option<u32>,
    same: impl FnOnce() -> bool,
) -> bool {
    window_focused && asked.is_some() && asked == focused && same()
}

/// 캡처 저장 폴더 바로 아래의 파일 경로인가.
fn is_capture_path(text: &str, captures: &std::path::Path) -> bool {
    std::path::Path::new(text).parent() == Some(captures)
}

/// `paste-<ms>.png` 파일명 규약 — 로컬 temp 파일명과 원격 저장 basename 이 공유.
fn clipboard_image_file_name() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("paste-{}.png", timestamp)
}

/// Encode clipboard image data (RGBA) as PNG into an in-memory byte buffer.
/// mirror 업로드는 파일을 거치지 않고 이 바이트를 bulk 파일 전송 채널로 바로 올린다.
fn encode_clipboard_image_as_png(image: &arboard::ImageData<'_>) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.bytes)?;
        writer.finish()?;
    }
    Ok(buf)
}

impl MainView {
    /// Returns write success, independently of whether the input action was handled.
    pub(crate) fn copy_text(
        &mut self,
        text: &str,
        primary: bool,
        key: &str,
        hint: Vec<String>,
        scope: crate::adapters::ui::ToastScope,
    ) -> bool {
        crate::clipboard::copy::copy_text(
            &mut self.clipboard,
            &mut self.state.toasts,
            text,
            primary,
            key,
            hint,
            scope,
        )
    }
}

impl crate::state::MainViewState {
    pub(crate) fn clipboard_error_toast(
        &mut self,
        error: &crate::clipboard::error::ClipboardError,
        scope: crate::adapters::ui::ToastScope,
    ) {
        crate::clipboard::copy::error_toast(&mut self.toasts, error, scope);
    }

    pub(crate) fn clipboard_copy_feedback(
        &mut self,
        result: Result<(), crate::clipboard::error::ClipboardError>,
        key: &str,
        hint: Vec<String>,
        scope: crate::adapters::ui::ToastScope,
    ) -> bool {
        crate::clipboard::copy::copy_feedback(&mut self.toasts, result, key, hint, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given 붙여넣기를 읽는 중, When 포커스가 바뀌거나 surface가 바뀌었다, Then 늦은 결과는 버린다.
    #[test]
    fn a_late_paste_reaches_only_the_surface_still_focused() {
        assert!(still_targeted(Some(3), true, Some(3), || true));
        assert!(!still_targeted(Some(3), true, Some(4), || true));
        assert!(!still_targeted(Some(3), true, None, || true));
        assert!(!still_targeted(Some(3), true, Some(3), || false));
        assert!(!still_targeted(None, true, None, || true));
    }

    /// Given 창 A에서 붙여넣기를 읽는 중, When 키보드 포커스가 다른 창이나 앱으로 갔다(A 안의
    /// surface 선택은 그대로), Then 늦은 결과와 실패 토스트를 A에 전달하지 않는다.
    #[test]
    fn a_late_paste_is_dropped_once_its_window_loses_focus() {
        assert!(!still_targeted(Some(3), false, Some(3), || true));
    }

    #[test]
    fn only_a_file_directly_in_the_capture_folder_is_a_capture_path() {
        let captures = std::path::Path::new("fixture-home").join("screenshots");
        let shot = captures.join("screenshot-1.png");
        assert!(is_capture_path(&shot.to_string_lossy(), &captures));
        assert!(!is_capture_path("screenshot-1.png", &captures));
        assert!(!is_capture_path(
            &captures.join("nested").join("a.png").to_string_lossy(),
            &captures
        ));
        assert!(!is_capture_path("some copied text", &captures));
    }

    /// PNG로 인코딩한 크기·RGBA 픽셀이 원본과 같은지 확인한다.
    #[test]
    fn encode_clipboard_image_as_png_roundtrips_rgba() {
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, // (0,0) red
            0, 255, 0, 255, // (1,0) green
            0, 0, 255, 255, // (0,1) blue
            0, 0, 0, 0, // (1,1) transparent
        ];
        let image = arboard::ImageData {
            width: 2,
            height: 2,
            bytes: std::borrow::Cow::Owned(rgba.clone()),
        };

        let png_bytes = encode_clipboard_image_as_png(&image).expect("encode");

        assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG 시그니처");

        let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes));
        let mut reader = decoder.read_info().expect("read_info");
        let mut out = vec![0u8; rgba.len()];
        let info = reader.next_frame(&mut out).expect("next_frame");
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(out, rgba, "RGBA 픽셀 동일");
    }

    /// 파일명 규약이 `paste-` 접두 + `.png` 확장자를 유지하는지(원격 저장 basename 공유).
    #[test]
    fn clipboard_image_file_name_convention() {
        let name = clipboard_image_file_name();
        assert!(name.starts_with("paste-"), "paste- 접두: {name}");
        assert!(name.ends_with(".png"), ".png 확장자: {name}");
    }
}
