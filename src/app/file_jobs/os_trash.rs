//! OS 휴지통 호출의 유일한 자리. 시험 빌드는 기본으로 실제 휴지통을 부르지 않는다. 실제 호출은 사용자
//! 휴지통에 시험 파일을 쌓고, macOS 에서는 Finder 를 부른다(이 호출에는 제한 시간이 없다).
//! 실제 구현의 계약은 Linux 의 격리한 자식 프로세스 시험만 잰다.

use super::Shared;
use std::path::Path;

#[cfg(not(test))]
pub(super) fn send(_shared: &Shared, path: &Path) -> Result<(), trash::Error> {
    trash::delete(path)
}

#[cfg(test)]
pub(super) fn send(shared: &Shared, path: &Path) -> Result<(), trash::Error> {
    #[cfg(target_os = "linux")]
    if super::tests::in_isolated_trash_child() {
        return trash::delete(path);
    }
    let result = stand_in(shared, path);
    if shared
        .cancel_after_trash
        .load(std::sync::atomic::Ordering::Acquire)
    {
        shared.cancel();
    }
    result
}

/// 휴지통을 대신해 항목을 지운다.
#[cfg(test)]
fn stand_in(shared: &Shared, path: &Path) -> Result<(), trash::Error> {
    if let Some(description) = super::lock(&shared.trash_refusal).clone() {
        return Err(trash::Error::Unknown { description });
    }
    remove_within(&temp_limit(), path)
}

/// 대역이 지울 수 있는 한계. 시험의 임시 항목이 놓이는 바로 그 자리다.
#[cfg(test)]
pub(super) fn temp_limit() -> std::path::PathBuf {
    let probe = tempfile::tempdir().expect("a temp folder");
    let temp = probe.path().parent().expect("its parent");
    temp.canonicalize().expect("the temp folder")
}

/// 휴지통과 달리 되살릴 수 없으므로 `limit` 밖 경로면 항목이 있든 없든 시험을 멈춘다. 없는 항목은 휴지통처럼
/// 오류로 돌려준다. 있는지를 먼저 보지 않아야 시험이 없는 경로로 한계를 잴 수 있다.
#[cfg(test)]
pub(super) fn remove_within(limit: &Path, path: &Path) -> Result<(), trash::Error> {
    let target = path.display().to_string();
    let parent = path.parent().and_then(|parent| parent.canonicalize().ok());
    let meta = path.symlink_metadata();
    let inside = parent.as_ref().is_some_and(|p| p.starts_with(limit));
    assert!(
        inside || (parent.is_none() && meta.is_err()),
        "a test sent {target} to the trash from outside {}",
        limit.display()
    );
    let Ok(meta) = meta else {
        return Err(trash::Error::CouldNotAccess { target });
    };
    let removed = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.map_err(|e| trash::Error::Unknown {
        description: e.to_string(),
    })
}
