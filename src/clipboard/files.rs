//! OS 파일 클립보드. Explorer 의 복사·잘라내기를 OS 파일 관리자와 다른 Tasty 창이 받고, 그쪽에서
//! 복사한 파일을 Explorer 가 붙여넣는다.
//!
//! - Linux: X11 CLIPBOARD 선택을 직접 소유하고 읽는다(`x11.rs`). Wayland 세션에서는 XWayland 를
//!   거친다. 터미널 텍스트 클립보드와 달리 네이티브 Wayland 의 data device 는 쓰지 않는다.
//! - Windows: `CF_HDROP` + `Preferred DropEffect`(`windows.rs`).
//! - macOS: 파일 URL(`macos.rs`). Finder 의 클립보드에는 잘라내기 표지가 없어 늘 복사로 읽고, 파일이 아닌 URL 이
//!   섞이면 목록 전체를 이 컴퓨터의 파일이 아닌 것으로 본다.
//!
//! 스크린샷의 경로 텍스트+이미지 게시([`write_text_and_image`])도 같은 플랫폼 소유자를 쓴다.
//!
//! 호출은 [`Handle`] 을 거친다. 시험 빌드의 기본값은 메모리 대역이라 시험이 실제 클립보드를 건드리지 않는다.

mod format;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

/// 스크린샷의 경로 텍스트와 이미지를 한 번의 게시로 함께 올린다. 파일 목록과 같은 플랫폼 소유자를
/// 쓴다. macOS 는 `tasty-platform` 의 pasteboard 쓰기를 쓴다.
pub(crate) fn write_text_and_image(
    text: &str,
    image: &crate::ports::clipboard::ClipboardImage,
) -> Result<(), ClipboardError> {
    #[cfg(target_os = "linux")]
    return x11::write_text_and_png(text, &image.png);
    #[cfg(windows)]
    return windows::write_text_and_image(text, image).map_err(ClipboardError::TransferFailed);
    #[cfg(target_os = "macos")]
    return crate::platform::pasteboard::write_text_and_png(text, &image.png)
        .map_err(ClipboardError::TransferFailed);
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    Err(format!(
        "image clipboard is not supported on this OS ({} bytes)",
        image.png.len()
    )
    .into())
}

use super::error::{ClipboardError, DisplayBackend};
use std::path::PathBuf;
use std::sync::Arc;

/// OS 클립보드의 파일 목록.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsFiles {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) cut: bool,
}

/// OS 클립보드를 읽은 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OsClip {
    /// 파일 목록이 아니다(글자·그림·빈 클립보드).
    Other,
    Files(OsFiles),
    /// 로컬 파일이 아닌 항목(다른 호스트·`sftp://` 등)이 있다. 붙여넣지 않는다.
    NotLocal,
    /// 다른 프로그램이 클립보드를 가졌지만 읽기 한도 안에 내용을 주지 않았다. 그 프로그램이 나중에
    /// 복사한 것이므로 Tasty 목록보다 새롭다. X11 만 소유자를 구분할 수 있어 Linux 에서만 생긴다.
    #[cfg_attr(
        not(any(target_os = "linux", test)),
        allow(dead_code, reason = "X11 만 응답하지 않는 소유자를 구분한다")
    )]
    Unreadable {
        /// 선택을 가진 X11 창. 같은 소유자인지 비교하는 데만 쓴다.
        owner: u32,
    },
}

/// 플랫폼별 OS 파일 클립보드.
pub(crate) trait FileClipboard: Send + Sync {
    /// 파일 목록을 클립보드에 올린다. 클립보드의 이전 내용은 사라진다.
    fn write(&self, files: &OsFiles) -> Result<(), ClipboardError>;
    /// 클립보드를 읽는다. 클립보드에 접근하지 못하면 Err 다.
    fn read(&self) -> Result<OsClip, ClipboardError>;
    /// 클립보드를 비운다.
    fn clear(&self) -> Result<(), ClipboardError>;
    /// `files` 를 올린 뒤 읽으면 돌아오는 목록. 잘라내기 표지를 싣지 못하는 OS 는 복사로 돌아온다.
    fn as_read(&self, files: &OsFiles) -> OsFiles {
        files.clone()
    }
}

impl OsClip {
    /// 플랫폼이 준 경로 목록. 절대 경로가 아닌 항목이 있으면 이 컴퓨터의 파일로 읽지 않는다.
    #[cfg(any(windows, test))]
    fn from_paths(paths: Vec<PathBuf>, cut: bool) -> Self {
        if paths.is_empty() {
            Self::Other
        } else if paths.iter().any(|p| !p.is_absolute()) {
            Self::NotLocal
        } else {
            Self::Files(OsFiles { paths, cut })
        }
    }
}

/// 창 상태가 들고 있는 OS 파일 클립보드.
#[derive(Clone)]
pub(crate) struct Handle(Arc<dyn FileClipboard>, DisplayBackend);

impl Handle {
    pub(crate) fn set_display(&mut self, display: DisplayBackend) {
        self.1 = display;
    }

    /// 이 OS 의 클립보드.
    pub(crate) fn system() -> Self {
        Self(Arc::new(System), DisplayBackend::Unknown)
    }

    /// 프로세스 안에서만 보이는 대역. 시험이 쓴다.
    #[cfg(test)]
    pub(crate) fn memory() -> (Self, Arc<Memory>) {
        let memory = Arc::new(Memory::default());
        (Self(memory.clone(), DisplayBackend::Unknown), memory)
    }

    pub(crate) fn write(&self, files: &OsFiles) -> Result<(), ClipboardError> {
        self.0.write(files).map_err(|e| e.for_display(self.1))
    }

    /// 두 핸들이 같은 클립보드를 가리키는가.
    #[cfg(test)]
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn read(&self) -> Result<OsClip, ClipboardError> {
        self.0.read().map_err(|e| e.for_display(self.1))
    }

    /// Expected readback after a successful publication, including platform cut semantics.
    pub(crate) fn as_read(&self, files: &OsFiles) -> OsFiles {
        self.0.as_read(files)
    }

    /// 클립보드가 아직 `files` 를 들고 있으면 비운다. 잘라낸 파일을 옮긴 뒤 같은 목록을 다시
    /// 붙여넣지 못하게 한다. 그 사이 다른 내용으로 바뀌었으면 건드리지 않는다.
    pub(crate) fn clear_if_holding(&self, files: &OsFiles) {
        let held = self.0.as_read(files);
        match self.0.read() {
            Ok(OsClip::Files(current)) if current == held => {
                if let Err(e) = self.0.clear() {
                    tracing::warn!("file clipboard clear failed: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::debug!("file clipboard read before clear failed: {e}"),
        }
    }
}

impl Default for Handle {
    fn default() -> Self {
        #[cfg(test)]
        {
            Self::memory().0
        }
        #[cfg(not(test))]
        {
            Self::system()
        }
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("file_clipboard::Handle")
    }
}

struct System;

impl FileClipboard for System {
    fn write(&self, files: &OsFiles) -> Result<(), ClipboardError> {
        #[cfg(target_os = "linux")]
        return x11::write(files);
        #[cfg(windows)]
        return windows::write(files).map_err(ClipboardError::TransferFailed);
        #[cfg(target_os = "macos")]
        return macos::write(files).map_err(ClipboardError::TransferFailed);
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        Err(format!(
            "file clipboard is not supported on this OS ({} paths)",
            files.paths.len()
        )
        .into())
    }

    fn read(&self) -> Result<OsClip, ClipboardError> {
        #[cfg(target_os = "linux")]
        return x11::read();
        #[cfg(windows)]
        return windows::read().map_err(ClipboardError::TransferFailed);
        #[cfg(target_os = "macos")]
        return macos::read().map_err(ClipboardError::TransferFailed);
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        Err("file clipboard is not supported on this OS".into())
    }

    fn as_read(&self, files: &OsFiles) -> OsFiles {
        OsFiles {
            paths: files.paths.clone(),
            // macOS pasteboard 에는 잘라내기 표지가 없다.
            cut: files.cut && !cfg!(target_os = "macos"),
        }
    }

    fn clear(&self) -> Result<(), ClipboardError> {
        #[cfg(target_os = "linux")]
        return x11::clear();
        #[cfg(windows)]
        return windows::clear().map_err(ClipboardError::TransferFailed);
        #[cfg(target_os = "macos")]
        return macos::clear().map_err(ClipboardError::TransferFailed);
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        Err("file clipboard is not supported on this OS".into())
    }
}

/// 실제 OS 클립보드를 쓰는 시험들이 같은 선택을 두고 겹치지 않게 하나씩 돌린다.
#[cfg(test)]
pub(crate) fn os_clipboard_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static OS_CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // 앞 시험이 패닉해도 클립보드는 다음 시험이 다시 쓰므로 독은 무시한다.
    OS_CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 메모리 대역. 다른 앱이 클립보드를 바꾸는 상황은 [`Memory::set`] 으로 흉내 낸다.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Memory {
    clip: std::sync::Mutex<Option<OsClip>>,
    /// 참이면 모든 호출이 Err 다(클립보드에 접근하지 못하는 환경).
    pub(crate) unavailable: std::sync::atomic::AtomicBool,
    pub(crate) read_unavailable: std::sync::atomic::AtomicBool,
    /// 참이면 잘라내기 표지를 싣지 못한다(macOS pasteboard 흉내).
    pub(crate) drops_cut_mark: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl Memory {
    pub(crate) fn set(&self, clip: OsClip) {
        *self.clip.lock().unwrap() = Some(clip);
    }

    pub(crate) fn get(&self) -> OsClip {
        self.clip.lock().unwrap().clone().unwrap_or(OsClip::Other)
    }

    fn check(&self) -> Result<(), ClipboardError> {
        if self.unavailable.load(std::sync::atomic::Ordering::Relaxed) {
            Err(ClipboardError::BackendUnavailable(
                "fixture unavailable".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
impl FileClipboard for Memory {
    fn write(&self, files: &OsFiles) -> Result<(), ClipboardError> {
        self.check()?;
        self.set(OsClip::Files(self.as_read(files)));
        Ok(())
    }

    fn as_read(&self, files: &OsFiles) -> OsFiles {
        let drops = self
            .drops_cut_mark
            .load(std::sync::atomic::Ordering::Relaxed);
        OsFiles {
            paths: files.paths.clone(),
            cut: files.cut && !drops,
        }
    }

    fn read(&self) -> Result<OsClip, ClipboardError> {
        self.check()?;
        if self
            .read_unavailable
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ClipboardError::BackendUnavailable(
                "fixture read unavailable".into(),
            ));
        }
        Ok(self.get())
    }

    fn clear(&self) -> Result<(), ClipboardError> {
        self.check()?;
        self.set(OsClip::Other);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_from_the_os_is_not_a_local_file() {
        let abs = crate::test_support::abs_path("a");
        assert_eq!(
            OsClip::from_paths(vec![abs.clone()], true),
            OsClip::Files(OsFiles {
                paths: vec![abs.clone()],
                cut: true
            })
        );
        assert_eq!(
            OsClip::from_paths(vec![abs, PathBuf::from("rel")], false),
            OsClip::NotLocal
        );
        assert_eq!(OsClip::from_paths(Vec::new(), false), OsClip::Other);
    }

    /// 이 OS 의 클립보드가 잘라내기를 어떻게 돌려주는지가 잘라내기 소비의 비교 기준이다.
    #[test]
    fn the_system_clipboard_reports_how_a_cut_reads_back() {
        let files = OsFiles {
            paths: vec![crate::test_support::abs_path("a")],
            cut: true,
        };
        let read = System.as_read(&files);
        assert_eq!(read.paths, files.paths);
        #[cfg(target_os = "macos")]
        assert!(!read.cut);
        #[cfg(not(target_os = "macos"))]
        assert!(read.cut);
    }

    /// 경로 텍스트와 PNG 를 함께 올린 뒤 다른 클립보드 연결이 두 형식을 각각 읽는다. 뒤의 형식이 앞의
    /// 형식을 지우지 않는지 본다. Linux 는 격리된 X 서버에서
    /// `DISPLAY=:N cargo test -- --ignored text_and_image`.
    #[test]
    #[ignore = "writes the real OS clipboard; run on an isolated display"]
    fn the_os_clipboard_keeps_text_and_image_published_together() {
        let _clipboard = os_clipboard_test_lock();
        let pixels = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([(x * 80) as u8, (y * 120) as u8, 200, 255])
        });
        let mut png = Vec::new();
        pixels
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode");
        let image = crate::clipboard::capture_image::decode_png(png).expect("decode");
        write_text_and_image("/tmp/tasty capture 한.png", &image).expect("publish");
        let mut reader =
            crate::clipboard::ClipboardContext::new(crate::clipboard::error::DisplayBackend::Other);
        assert_eq!(
            reader.get_text().expect("text").as_deref(),
            Some("/tmp/tasty capture 한.png")
        );
        let read = reader.get_image().expect("image").expect("an image format");
        assert_eq!((read.width, read.height), (3, 2));
        assert_eq!(read.bytes.as_ref(), image.rgba.as_slice());
    }

    /// 실제 OS 클립보드로 쓰기·읽기·비우기를 한 바퀴 돈다. 사용자 클립보드를 덮어쓰므로 기본으로는
    /// 돌지 않는다. Linux 는 격리된 X 서버에서 `DISPLAY=:N cargo test -- --ignored the_os_clipboard`.
    #[test]
    #[ignore = "writes the real OS clipboard; run on an isolated display"]
    fn the_os_clipboard_round_trips_files_and_the_cut_mark() {
        let _clipboard = os_clipboard_test_lock();
        let handle = Handle::system();
        // 클립보드에는 경로만 오가므로 파일은 없어도 된다.
        let paths = vec![
            crate::test_support::abs_path("tasty clip 한.txt"),
            crate::test_support::abs_path("b"),
        ];
        for cut in [false, true] {
            let files = OsFiles {
                paths: paths.clone(),
                cut,
            };
            handle.write(&files).expect("write");
            // macOS pasteboard 에는 잘라내기 표지가 없어 늘 복사로 읽힌다.
            let seen = OsFiles {
                paths: paths.clone(),
                cut: cut && !cfg!(target_os = "macos"),
            };
            assert_eq!(handle.read().expect("read"), OsClip::Files(seen));
            // 잘라내기 소비처럼 올린 목록으로 비운다. OS 가 표지를 버려도 비워야 한다.
            handle.clear_if_holding(&files);
            assert_eq!(handle.read().expect("read after clear"), OsClip::Other);
        }
    }
}
