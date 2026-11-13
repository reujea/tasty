//! `upgrade_builtins`가 이번 호출로 실제 교체할 내장 plugin 을 파일을 쓰지 않고 고른다.
//! 바쁜지 묻는 쪽이 교체 대상에만 묻기 위해 읽는다 — 판정 기준은 [`super::upgrade_builtins`]와 같다.

use std::path::Path;

use super::{
    BUILTINS, BuiltinUpgradeDecision, PluginManager, bundle_root, decide_builtin_upgrade,
    discovery, read_bundle_version, read_installed_version, sync_probe,
    verify_builtin_bundle_trust,
};

/// `upgrade_builtins`가 실제로 파일을 바꾸고 재시작할 내장 plugin의 id 들 — 파일은 쓰지 않는다.
///
/// 바쁜지 묻는 쪽이 교체 대상만 고르려고 읽는다. 판정 기준은 [`super::upgrade_builtins`]와 같다 —
/// 사용자가 지운 것·번들에 없는 것·신뢰 검증에 걸린 것·`installed >= bundle`은 빼고,
/// 같은 버전은 내용이 실제로 달라 다시 쓸 때만 넣는다. `restore_removed`·`restore_all`로
/// 이번 호출이 되살릴 id 는 지워진 것으로 보지 않는다.
///
/// 조회에 실패하면 그 id 를 후보에 넣는다 — 물어서 손해 보는 쪽이 모르고 내리는 쪽보다 낫다.
pub fn builtin_restart_candidates(
    mgr: &PluginManager,
    force: bool,
    restore_removed: &[String],
    restore_all: bool,
) -> Vec<String> {
    let Some(dest_root) = discovery::plugin_root() else {
        return Vec::new();
    };
    let Some(bundle) = bundle_root() else {
        return Vec::new();
    };
    BUILTINS
        .iter()
        .filter(|spec| {
            let restored = restore_all || restore_removed.iter().any(|id| id == spec.id);
            !mgr.config.is_builtin_removed(spec.id) || restored
        })
        .filter(|spec| {
            let (src, dest) = (bundle.join(spec.id), dest_root.join(spec.id));
            if verify_builtin_bundle_trust(spec, &src, &dest).is_err() {
                return false;
            }
            paths_would_restart(&src, &dest, force)
        })
        .map(|spec| spec.id.to_string())
        .collect()
}

/// 번들과 설치본의 경로만 보고 교체 여부를 판정한다 — 파일은 쓰지 않는다.
/// [`builtin_restart_candidates`]가 신뢰 검증을 통과한 spec마다 부른다.
pub(super) fn paths_would_restart(src: &Path, dest: &Path, force: bool) -> bool {
    if !src.is_dir() {
        return false;
    }
    if !dest.exists() {
        return true;
    }
    match decide_builtin_upgrade(
        read_installed_version(dest).as_ref(),
        read_bundle_version(src).as_ref(),
        force,
    ) {
        BuiltinUpgradeDecision::Skip => false,
        // 같은 버전은 내용이 달라 다시 쓸 때만 프로세스를 건드린다.
        BuiltinUpgradeDecision::ResyncSameVersion => sync_probe::sync_would_touch(src, dest),
        BuiltinUpgradeDecision::UpgradeVersion { .. } | BuiltinUpgradeDecision::ForceOverwrite => {
            true
        }
    }
}
