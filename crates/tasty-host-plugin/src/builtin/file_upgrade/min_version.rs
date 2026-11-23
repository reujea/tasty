//! 번들 원본의 `min_tasty_version`을 복사·교체 전에 비교한다.
//! 버전 읽기(`read_installed_version` 등)는 비교하지 않아 상위 설치본을 그대로 두고,
//! 실제로 파일을 바꾸기 직전에만 이 판정을 쓴다.

use std::path::Path;

use tasty_plugin_manifest::Manifest;

/// 번들 원본이 실행 중인 Tasty보다 높은 버전을 요구하면 그 거절을 돌려준다.
/// 로그는 Display(영어), 사용자 보고는 [`crate::manifest_error::describe`]로 쓴다.
/// 매니페스트를 읽지 못하면 `None`이다. 그 경우는 기존 버전 비교·서명 검사가 다룬다.
pub(crate) fn bundle_requires_newer_tasty(src: &Path) -> Option<anyhow::Error> {
    let manifest = Manifest::load(src).ok()?;
    manifest.require_host_tasty().err()
}

/// 업그레이드 보고서의 실패 항목에 쓸 사유. 현재 언어 문장이다.
pub(crate) fn refusal_reason(src: &Path) -> Option<String> {
    bundle_requires_newer_tasty(src).map(|e| crate::manifest_error::describe(&e))
}

/// 부팅 번들 동기화([`crate::official::bundle_source`]): 거절하면 경고를 남기고 사유를 돌려준다.
pub(crate) fn refuse_boot_copy(id: &str, src: &Path) -> Option<String> {
    let error = bundle_requires_newer_tasty(src)?;
    tracing::warn!("install_builtins: bundle '{id}' not applied: {error}");
    Some(error.to_string())
}

/// upgrade-builtins([`super::verify_trust`]): 파일 교체·프로세스 종료 전에 그 항목을 실패로 돌려준다.
pub(crate) fn refused_item(id: &str, src: &Path) -> Option<crate::builtin::BuiltinUpgradeItem> {
    refusal_reason(src).map(|reason| crate::builtin::BuiltinUpgradeItem {
        id: id.into(),
        action: crate::builtin::BuiltinUpgradeAction::Failed { reason },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 이 시험 바이너리에서 본체 버전을 정하는 곳은 여기뿐이다. 다른 시험의 매니페스트에는
    /// min_tasty_version이 없어 결과가 바뀌지 않는다.
    fn with_host_1_0_0() {
        if tasty_plugin_manifest::host_tasty_version().is_none() {
            // 병렬 시험이 먼저 정했으면 실패한다. 값은 같으므로 무시해도 된다.
            let _already_set = tasty_plugin_manifest::set_host_tasty_version("1.0.0").is_err();
        }
        assert_eq!(
            tasty_plugin_manifest::host_tasty_version().map(|v| v.to_string()),
            Some("1.0.0".into())
        );
    }

    fn bundle(min: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("tasty-plugin.toml"),
            format!(
                "manifest_version = 1\nid = \"com.example.x\"\nname = \"X\"\nversion = \"0.1.0\"\n\
                 api_version = \"1\"\n{min}\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
            ),
        )
        .expect("write");
        dir
    }

    #[test]
    fn a_bundle_requiring_a_newer_tasty_is_refused_with_both_versions() {
        with_host_1_0_0();
        let dir = bundle(r#"min_tasty_version = "1.0.1""#);
        let error = bundle_requires_newer_tasty(dir.path()).expect("refused");
        assert!(
            error
                .downcast_ref::<tasty_plugin_manifest::TastyVersionTooOld>()
                .is_some()
        );
        let reason = refusal_reason(dir.path()).expect("reason");
        assert!(
            reason.contains("1.0.1") && reason.contains("1.0.0"),
            "{reason}"
        );
    }

    #[test]
    fn a_bundle_within_the_host_version_or_without_the_field_is_admitted() {
        with_host_1_0_0();
        for line in [
            "",
            r#"min_tasty_version = "1.0.0""#,
            r#"min_tasty_version = "0.9.9""#,
        ] {
            let dir = bundle(line);
            assert!(bundle_requires_newer_tasty(dir.path()).is_none(), "{line}");
            assert_eq!(refusal_reason(dir.path()), None, "{line}");
        }
    }
}
