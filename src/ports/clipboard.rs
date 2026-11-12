//! 시스템 클립보드 접근. 헤드리스에서는 동작하지 않는 어댑터를 주입한다.

pub trait ClipboardSystem: Send + Sync {
    fn read_text(&self) -> anyhow::Result<String>;
    fn write_text(&self, text: &str) -> anyhow::Result<()>;
    /// 텍스트와 이미지를 한 번의 게시로 함께 올린다. 성공하면 두 형식을 모두 읽을 수 있다.
    /// 실패하면 클립보드에 무엇이 남았는지 보장하지 않으므로 호출자는 [`publish_text_with_image`] 처럼
    /// 텍스트만 다시 쓴다. 이미지 게시를 지원하지 않는 어댑터는 기본 구현대로 오류를 돌려준다.
    fn write_text_and_image(&self, _text: &str, _image: &ClipboardImage) -> anyhow::Result<()> {
        anyhow::bail!("this clipboard cannot publish images")
    }
    /// 텍스트 읽기를 시작한다. 바로 읽을 수 없는 클립보드(네이티브 Wayland의 다른 앱 선택)는
    /// 토큰을 `Err`로 돌려주고 호출자는 [`Self::finish_read_text`]로 결과를 받는다. 호출 스레드를 막지 않는다.
    fn begin_read_text(&self) -> TextRead {
        Ok(self.read_text())
    }
    /// 토큰으로 시작한 읽기의 결과. 아직 기다리는 중이면 `None`이다. 결과를 한 번 돌려주면 그
    /// 토큰은 끝난다. 상한 초과나 더 새 읽기에 밀린 경우도 오류로 끝난다. 결과를 기다리는 이벤트
    /// 루프는 GUI에만 있다.
    #[cfg(feature = "gui")]
    fn finish_read_text(&self, _token: u64) -> Option<anyhow::Result<String>> {
        Some(Err(anyhow::anyhow!("no pending clipboard read")))
    }
}

/// 텍스트 읽기의 시작 결과. `Ok`는 지금 읽은 결과이고 `Err`는 아직 읽는 중인 읽기의 토큰이다.
/// 그 토큰으로 [`ClipboardSystem::finish_read_text`]를 부른다.
pub type TextRead = Result<anyhow::Result<String>, u64>;

/// 클립보드에 올릴 이미지. `png` 는 저장 파일과 같은 바이트이고 `rgba` 는 그것을 디코딩한 픽셀이다.
/// PNG 를 그대로 받는 OS 형식과 픽셀이 필요한 형식(Windows DIB)이 같은 내용을 갖게 한다.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// 행 우선, 위에서 아래, 픽셀당 RGBA 4바이트.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for ClipboardImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardImage")
            .field("png_bytes", &self.png.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// 경로 텍스트와 이미지를 게시한 결과. 저장 파일과는 별개로 클립보드에 무엇이 올라갔는지만 말한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureCopy {
    /// 경로 텍스트와 이미지를 함께 게시했다.
    TextAndImage,
    /// 경로 텍스트만 게시했다. 이미지는 `image_error` 때문에 빠졌다.
    TextOnly { image_error: String },
    /// 클립보드에 아무것도 게시하지 못했다.
    Failed { error: String },
}

/// 이미지가 준비됐으면 텍스트와 함께 게시하고, 이미지를 준비하지 못했거나 함께 게시하지 못했으면
/// 텍스트만 다시 쓴다. 이미지 게시가 일부 형식만 남긴 상태를 성공으로 보고하지 않기 위해서다.
pub fn publish_text_with_image(
    clipboard: &dyn ClipboardSystem,
    text: &str,
    image: Result<&ClipboardImage, &str>,
) -> CaptureCopy {
    let image_error = match image {
        Ok(image) => match clipboard.write_text_and_image(text, image) {
            Ok(()) => return CaptureCopy::TextAndImage,
            Err(error) => format!("{error:#}"),
        },
        Err(reason) => reason.to_string(),
    };
    match clipboard.write_text(text) {
        Ok(()) => CaptureCopy::TextOnly { image_error },
        Err(error) => CaptureCopy::Failed {
            error: format!("{error:#}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test::mock_clipboard::MockClipboard;

    fn image() -> ClipboardImage {
        ClipboardImage {
            png: vec![1, 2, 3],
            width: 1,
            height: 1,
            rgba: vec![9, 8, 7, 255],
        }
    }

    struct TextOnlyClipboard(MockClipboard);

    impl ClipboardSystem for TextOnlyClipboard {
        fn read_text(&self) -> anyhow::Result<String> {
            self.0.read_text()
        }
        fn write_text(&self, text: &str) -> anyhow::Result<()> {
            self.0.write_text(text)
        }
    }

    struct Broken;

    impl ClipboardSystem for Broken {
        fn read_text(&self) -> anyhow::Result<String> {
            anyhow::bail!("fixture read")
        }
        fn write_text(&self, _text: &str) -> anyhow::Result<()> {
            anyhow::bail!("fixture write")
        }
        fn write_text_and_image(&self, _: &str, _: &ClipboardImage) -> anyhow::Result<()> {
            anyhow::bail!("fixture image")
        }
    }

    #[test]
    fn both_formats_are_published_together() {
        let clipboard = MockClipboard::default();
        let image = image();
        let outcome = publish_text_with_image(&clipboard, "/tmp/a.png", Ok(&image));
        assert_eq!(outcome, CaptureCopy::TextAndImage);
        assert_eq!(clipboard.read_text().unwrap(), "/tmp/a.png");
        assert_eq!(clipboard.image(), Some(image));
    }

    #[test]
    fn an_unsupported_image_falls_back_to_the_text_alone() {
        let clipboard = TextOnlyClipboard(MockClipboard::default());
        let outcome = publish_text_with_image(&clipboard, "/tmp/a.png", Ok(&image()));
        let CaptureCopy::TextOnly { image_error } = outcome else {
            panic!("{outcome:?}");
        };
        assert!(
            image_error.contains("cannot publish images"),
            "{image_error}"
        );
        assert_eq!(clipboard.read_text().unwrap(), "/tmp/a.png");
    }

    #[test]
    fn a_decode_failure_still_publishes_the_text() {
        let clipboard = MockClipboard::default();
        let outcome = publish_text_with_image(&clipboard, "/tmp/a.png", Err("bad png"));
        assert_eq!(
            outcome,
            CaptureCopy::TextOnly {
                image_error: "bad png".into()
            }
        );
        assert_eq!(clipboard.read_text().unwrap(), "/tmp/a.png");
        assert_eq!(clipboard.image(), None);
    }

    #[test]
    fn a_failing_clipboard_reports_failure_not_partial_success() {
        let outcome = publish_text_with_image(&Broken, "/tmp/a.png", Ok(&image()));
        assert_eq!(
            outcome,
            CaptureCopy::Failed {
                error: "fixture write".into()
            }
        );
    }
}
