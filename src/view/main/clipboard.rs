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

impl MainView {
    pub fn paste_to_terminal(&mut self, engine: &EngineRead<'_>) {
        let scope = self
            .state
            .focused_surface_id(engine)
            .map(crate::adapters::ui::ToastScope::Surface)
            .unwrap_or(crate::adapters::ui::ToastScope::Window);
        let content = match self.clipboard.read_paste() {
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
        let result = if primary {
            self.clipboard.set_text_primary(text)
        } else {
            self.clipboard.set_text(text)
        };
        self.state.clipboard_copy_feedback(result, key, hint, scope)
    }
}

impl crate::state::MainViewState {
    pub(crate) fn clipboard_error_toast(
        &mut self,
        error: &crate::clipboard::error::ClipboardError,
        scope: crate::adapters::ui::ToastScope,
    ) {
        let (key, kind) = error.feedback();
        self.toasts.push(crate::i18n::t(key), kind, scope);
    }

    pub(crate) fn clipboard_copy_feedback(
        &mut self,
        result: Result<(), crate::clipboard::error::ClipboardError>,
        key: &str,
        hint: Vec<String>,
        scope: crate::adapters::ui::ToastScope,
    ) -> bool {
        match result {
            Ok(()) => {
                self.toasts.push_with_hint(
                    crate::i18n::t(key),
                    crate::adapters::ui::ToastKind::Info,
                    hint,
                    scope,
                );
                true
            }
            Err(error) => {
                self.clipboard_error_toast(&error, scope);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_file_directly_in_the_capture_folder_is_a_capture_path() {
        let captures = std::env::temp_dir().join("tasty-captures");
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
