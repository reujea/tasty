//! 확인 필요 탭의 예약 id 폴더 제거. 재귀 삭제이므로 화면 값을 믿지 않고 지우기 직전에 다시 확인한다.
//!
//! 등록된 적이 없는 폴더라 id 로 찾지 않는다. 폴더 이름은 id 와 다를 수 있고, 같은 id 의 폴더가
//! 둘일 수 있다. 신뢰 기록과 설정 기록은 같은 id 의 다른 폴더에 영향을 주므로 건드리지 않는다.

use std::io::ErrorKind;
use std::path::Path;

use tasty_plugin_manifest::Manifest;

/// 제거 실패. `deletion_started` 면 일부 파일이 이미 지워졌을 수 있다. `message` 는 토스트에 실린다.
/// 고정 사유에는 경로를 넣지 않는다. 매니페스트·OS 오류 원문에는 전체 경로가 들어갈 수 있어 길이가
/// 정해지지 않으므로 호출자가 토스트 길이에 맞춘다.
#[derive(Debug)]
pub struct ReservedRemovalError {
    pub message: String,
    pub deletion_started: bool,
}

impl ReservedRemovalError {
    fn refused(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            deletion_started: false,
        }
    }
}

/// `root` 바로 아래의 예약 id 폴더 `dir` 을 지운다.
///
/// 이미 없으면 목표 상태이므로 성공이다. 링크는 대상을 건드리지 않도록 거부한다. 매니페스트가 있는데
/// 읽지 못하거나 지금은 허용되는 id 면 거부한다. 매니페스트가 없으면 앞선 제거가 도중에 멈춘 잔여로
/// 보고 지운다. 이 구분이 없으면 매니페스트까지 지워진 항목을 다시 시도할 수 없다.
pub fn remove_reserved_folder(root: &Path, dir: &Path) -> Result<(), ReservedRemovalError> {
    if dir.file_name().is_none() || dir.parent() != Some(root) {
        return Err(ReservedRemovalError::refused(
            "the folder is not directly inside the plugins folder",
        ));
    }
    let metadata = match std::fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ReservedRemovalError::refused(error.to_string())),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ReservedRemovalError::refused(
            "plugin path is not an owned directory",
        ));
    }
    if still_reserved(dir)? {
        return std::fs::remove_dir_all(dir).map_err(|error| ReservedRemovalError {
            message: error.to_string(),
            deletion_started: true,
        });
    }
    Err(ReservedRemovalError::refused(
        "the folder no longer declares a reserved id",
    ))
}

/// 매니페스트가 없거나 여전히 예약 id 를 선언하면 true.
fn still_reserved(dir: &Path) -> Result<bool, ReservedRemovalError> {
    let manifest = dir.join("tasty-plugin.toml");
    match std::fs::symlink_metadata(&manifest) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(ReservedRemovalError::refused(error.to_string())),
    }
    let manifest =
        Manifest::load(dir).map_err(|error| ReservedRemovalError::refused(error.to_string()))?;
    Ok(crate::official::admit_plugin_id(&manifest.id).is_err())
}

#[cfg(test)]
#[path = "reserved_removal_tests.rs"]
mod tests;
