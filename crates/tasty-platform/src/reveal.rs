//! OS 파일 관리자/기본 앱으로 경로 열기 (explorer 컨텍스트 메뉴 "Open in system").
//!
//! 폴더면 OS 파일 관리자에서 그 폴더를 연다. 크로스 플랫폼: Windows `explorer`,
//! macOS `open`, Linux `xdg-open`. 입력 검증/식별은 호출부 책임 — 여기선 launch 만.

use std::path::Path;
use std::process::Command;

/// `path` 를 OS 기본 핸들러로 연다 (폴더 → 파일 관리자). 실패는 Err 로 반환.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    #[cfg(debug_assertions)]
    if crate::debug_os_open::intercepted("open_path", &path.to_string_lossy()) {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_os = "macos")))))]
    {
        // 지원하지 않는 플랫폼 — 열 수단이 없어 path 를 쓸 곳이 없다.
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "open_path: unsupported platform",
        ))
    }
}
