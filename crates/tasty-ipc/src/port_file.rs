//! 데이터 루트의 tasty.port 조회와 저장. 기본 루트는 debug/release에 따라 구분한다.

mod identity;

use std::path::{Path, PathBuf};

use anyhow::Result;

use tasty_utils::path::tasty_home;

/// 포트 조회 실패 조건. 소비자가 번역하며 Display의 기본 영어는 CLI 카탈로그와 맞춘다.
#[derive(Debug, thiserror::Error)]
pub enum PortFileError {
    /// tasty home 을 확정하지 못해 포트 파일 경로 자체를 만들 수 없다.
    #[error("Could not determine config directory")]
    HomeUnresolved,
    /// 지정한 포트 파일이 없다. 인스턴스 미실행이나 다른 데이터 루트를 확인한다.
    #[error("No running tasty instance found (port file not found at {})", .path.display())]
    NotFound {
        /// 실제로 찾아본 경로. 홈이 갈릴 때(`TASTY_HOME`) 사용자가 대조하는 값이라
        /// 문구를 만드는 쪽에 그대로 넘긴다.
        path: PathBuf,
    },
    /// 파일은 있으나 포트 번호로 읽히지 않는다.
    #[error("Invalid port file contents")]
    Invalid,
}

/// Port file 경로 (`tasty_home()/tasty.port`).
///
/// debug/release 격리는 루트(`tasty_home()`)가 담당한다 — debug 빌드는
/// `~/.tasty-debug/tasty.port`, release 는 `~/.tasty/tasty.port`. 루트가 갈리므로
/// 파일명 접미사(`-debug`)는 두지 않는다.
pub fn port_file_path() -> Option<PathBuf> {
    tasty_home().map(|dir| dir.join("tasty.port"))
}

/// 기본 port file 에서 포트 읽기 — CLI 클라이언트가 사용.
pub fn read_port_file() -> Result<u16, PortFileError> {
    read_port_file_from(None)
}

/// 지정 경로가 있으면 그 경로, 없으면 기본 port file 경로.
pub fn resolve_port_file_path(port_file: Option<&str>) -> Result<PathBuf, PortFileError> {
    match port_file {
        Some(p) => Ok(PathBuf::from(p)),
        None => port_file_path().ok_or(PortFileError::HomeUnresolved),
    }
}

/// 지정 경로 (또는 기본) 의 port file 에서 포트 읽기.
pub fn read_port_file_from(port_file: Option<&str>) -> Result<u16, PortFileError> {
    let path = resolve_port_file_path(port_file)?;
    let contents = std::fs::read_to_string(&path)
        .map_err(|_| PortFileError::NotFound { path: path.clone() })?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|_| PortFileError::Invalid)
}

/// port file이 있는 폴더의 인스턴스 파일로 그 포트를 쓴 인스턴스가 끝났는지 본다. 기본 port
/// file은 홈에 있으므로 홈의 인스턴스 파일을 읽는다. 다른 폴더를 지정했고 그 폴더에 인스턴스 파일이
/// 없으면 판별하지 않는다.
pub fn port_owner(port_file_path: &Path, port: u16) -> crate::instance_file::PortOwner {
    match port_file_path.parent() {
        Some(dir) => crate::instance_file::port_owner_in(dir, port),
        None => crate::instance_file::PortOwner::Unknown,
    }
}

/// Completed lifecycle publication. This is not a queued save receipt.
#[derive(Debug)]
pub struct PortFilePublication {
    path: PathBuf,
    target: PathBuf,
    contents: String,
    identity: Option<identity::PinnedFile>,
}

impl PortFilePublication {
    /// Try the participant lock once; contention or an unknown/replaced generation is preserved.
    /// Nonparticipating writers can still race the final check/unlink.
    pub fn cleanup_if_owned(&self) -> std::io::Result<bool> {
        use tasty_utils::file_replace;
        let Some(_lock) = file_replace::try_lock(&self.path)? else {
            return Ok(false);
        };
        let Some(identity) = &self.identity else {
            return Ok(false);
        };
        if file_replace::resolve_link(&self.path)? != self.target {
            return Ok(false);
        }
        // Reject nonregular replacements before open (a FIFO open could wait for a peer).
        match std::fs::metadata(&self.path) {
            Ok(meta) if !meta.is_file() => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        if !file.metadata()?.is_file() || !identity.matches(&file)? {
            return Ok(false);
        }
        if !has_contents(&file, &self.contents)? {
            return Ok(false);
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Publish only after a complete atomic replacement; unreadable originals are preserved.
/// New Unix files are 0600. Existing metadata and live symlinks use the common replacement policy.
pub fn publish_port_file_to(port: u16, custom_path: Option<&Path>) -> Result<PortFilePublication> {
    use tasty_utils::file_replace::{self, Fingerprint};
    let path = custom_path
        .map(Path::to_path_buf)
        .or_else(port_file_path)
        .ok_or(PortFileError::HomeUnresolved)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let target = file_replace::resolve_link(&path)?;
    match std::fs::metadata(&target) {
        Ok(meta) if !meta.is_file() => anyhow::bail!("port file is not a regular file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    let observed = Fingerprint::read(&target)?;
    publish_observed(port, path, target, observed)
}

fn publish_observed(
    port: u16,
    path: PathBuf,
    target: PathBuf,
    observed: tasty_utils::file_replace::Fingerprint,
) -> Result<PortFilePublication> {
    use tasty_utils::file_replace::{self, Fingerprint, Options};
    let _lock = file_replace::lock(&path)?;
    if file_replace::resolve_link(&path)? != target || Fingerprint::read(&target)? != observed {
        anyhow::bail!("port file changed while preparing publication");
    }
    let bytes = port.to_string();
    file_replace::replace_with(
        &path,
        bytes.as_bytes(),
        Options {
            new_mode: Some(0o600),
            sync: false,
        },
    )?;
    // Capture while the publication lock is held; the open file pins this generation's ID.
    // Unknown identity disables deletion, never startup discovery or the numeric wire format.
    let identity = match identity::PinnedFile::open(&path, &bytes) {
        Ok(identity) => Some(identity),
        Err(error) => {
            tracing::warn!(%error, "port published without cleanup identity; preserving it on teardown");
            None
        }
    };
    tracing::info!(path = %path.display(), "published port file");
    Ok(PortFilePublication {
        path,
        target,
        contents: bytes,
        identity,
    })
}

fn has_contents(file: &std::fs::File, expected: &str) -> std::io::Result<bool> {
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes == expected.as_bytes())
}

/// Blocking startup barrier. Returning Ok means the complete port is observable by discovery.
pub fn write_port_file_to(port: u16, custom_path: Option<&Path>) -> Result<()> {
    publish_port_file_to(port, custom_path).map(|_| ())
}

#[cfg(test)]
#[path = "port_file/tests.rs"]
mod tests;
