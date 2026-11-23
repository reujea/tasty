//! 번들 원본의 `min_tasty_version`을 복사·교체 전에 비교한다.
//! 버전 읽기(`read_installed_version` 등)는 비교하지 않아 상위 설치본을 그대로 두고,
//! 실제로 파일을 바꾸기 직전에만 이 판정을 쓴다.

use std::path::Path;

use tasty_plugin_manifest::Manifest;

/// 번들 원본의 `min_tasty_version`이 형식 오류이거나 실행 중인 Tasty보다 높은 버전을 요구해
/// 받아들일 수 없으면 그 거절을 돌려준다. 버전 부족을 경고만 하는 빌드에서는 `None`이다.
/// 로그는 Display(영어), 사용자 보고는 [`crate::manifest_error::describe`]로 쓴다.
/// 매니페스트를 읽지 못하면 `None`이다. 그 경우는 기존 버전 비교·서명 검사가 다룬다.
pub(crate) fn bundle_requires_newer_tasty(src: &Path) -> Option<anyhow::Error> {
    // 형식 오류도 '제약 없음'으로 넘기지 않고 복사 전에 거절한다.
    Manifest::admit_host_tasty(src).err()
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
    use crate::builtin::file_upgrade::{Entry, prepare};
    use crate::builtin::{BuiltinUpgradeAction, PluginsConfig};

    fn plugin(dir: &Path, id: &str, min: &str) {
        std::fs::create_dir_all(dir).expect("dir");
        std::fs::write(
            dir.join("tasty-plugin.toml"),
            format!(
                "manifest_version = 1\nid = \"{id}\"\nname = \"X\"\nversion = \"9.0.0\"\n\
                 api_version = \"1\"\n{min}\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
            ),
        )
        .expect("write");
        std::fs::write(dir.join("x"), "unused").expect("entry");
    }

    #[test]
    fn a_bundle_requiring_a_newer_tasty_is_refused_with_both_versions() {
        crate::test_support::host_tasty_1_0_0();
        let dir = tempfile::tempdir().expect("tempdir");
        plugin(
            dir.path(),
            "com.example.x",
            r#"min_tasty_version = "1.0.1""#,
        );
        let error = bundle_requires_newer_tasty(dir.path()).expect("refused");
        assert!(error.is::<tasty_plugin_manifest::TastyVersionTooOld>());
        let reason = refusal_reason(dir.path()).expect("reason");
        assert!(
            reason.contains("1.0.1") && reason.contains("1.0.0"),
            "{reason}"
        );
    }

    #[test]
    fn a_bundle_with_a_malformed_minimum_is_refused_not_treated_as_unconstrained() {
        crate::test_support::host_tasty_1_0_0();
        let dir = tempfile::tempdir().expect("tempdir");
        plugin(dir.path(), "com.example.x", r#"min_tasty_version = "0.15""#);
        let error = bundle_requires_newer_tasty(dir.path()).expect("refused");
        assert!(error.to_string().contains("min_tasty_version"), "{error}");
    }

    #[test]
    fn a_bundle_within_the_host_version_or_without_the_field_is_admitted() {
        crate::test_support::host_tasty_1_0_0();
        for line in [
            "",
            r#"min_tasty_version = "1.0.0""#,
            r#"min_tasty_version = "0.9.9""#,
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            plugin(dir.path(), "com.example.x", line);
            assert!(bundle_requires_newer_tasty(dir.path()).is_none(), "{line}");
            assert_eq!(refusal_reason(dir.path()), None, "{line}");
        }
    }

    /// 부팅 번들 동기화: 복사할 원본을 고르는 단계가 거절한다(형식 오류 포함).
    #[test]
    fn boot_bundle_selection_refuses_a_newer_or_malformed_requirement() {
        crate::test_support::host_tasty_1_0_0();
        for line in [
            r#"min_tasty_version = "2.0.0""#,
            r#"min_tasty_version = "2.0""#,
        ] {
            let bundle = tempfile::tempdir().expect("tempdir");
            plugin(
                &bundle.path().join("com.tasty.html"),
                "com.tasty.html",
                line,
            );
            let err =
                crate::official::bundle_source(bundle.path(), "com.tasty.html").expect_err(line);
            assert!(
                err.contains("min_tasty_version") || err.contains("2.0.0"),
                "{line}: {err}"
            );
        }
        let bundle = tempfile::tempdir().expect("tempdir");
        plugin(&bundle.path().join("com.tasty.html"), "com.tasty.html", "");
        assert!(matches!(
            crate::official::bundle_source(bundle.path(), "com.tasty.html"),
            Ok(Some(_))
        ));
    }

    /// upgrade-builtins 계획: 서명이 없는 번들이어도 버전 사유가 먼저다(거절 순서).
    /// release 시험(`cargo test --release`)에서는 서명 판정이 켜져 있어 순서를 실제로 가린다.
    #[test]
    fn upgrade_plan_fails_an_unsigned_newer_bundle_with_the_version_reason_first() {
        crate::test_support::host_tasty_1_0_0();
        for (line, needle) in [
            (r#"min_tasty_version = "2.0.0""#, "2.0.0"),
            (r#"min_tasty_version = "2.0""#, "min_tasty_version"),
        ] {
            let home = tempfile::tempdir().expect("tempdir");
            let bundle = home.path().join("bundle");
            plugin(&bundle.join("com.tasty.html"), "com.tasty.html", line);
            let plan = prepare(
                home.path().join("installed"),
                Some(bundle),
                home.path().join("known.toml"),
                PluginsConfig::load_from(home.path().join("plugins.toml")),
                false,
                &[],
                false,
            )
            .expect("plan");
            let item = plan
                .entries
                .into_iter()
                .find_map(|entry| match entry {
                    Entry::Done(item) if item.id == "com.tasty.html" => Some(item),
                    _ => None,
                })
                .expect("refused before any write");
            match item.action {
                BuiltinUpgradeAction::Failed { reason } => {
                    assert!(reason.contains(needle), "{line}: {reason}")
                }
                other => panic!("{line}: expected Failed, got {other:?}"),
            }
            assert!(!home.path().join("installed/com.tasty.html").exists());
        }
    }

    /// 부팅·재탐색: 더 높은 Tasty를 요구하는 폴더는 어느 목록에도 들어가지 않는다.
    #[test]
    fn discovery_leaves_out_a_plugin_requiring_a_newer_tasty() {
        crate::test_support::host_tasty_1_0_0();
        let home = crate::test_support::HomeEnvGuard::tasty_home();
        let plugins = home.path().join("plugins");
        plugin(
            &plugins.join("com.example.new"),
            "com.example.new",
            r#"min_tasty_version = "1.0.1""#,
        );
        plugin(
            &plugins.join("com.example.ok"),
            "com.example.ok",
            r#"min_tasty_version = "1.0.0""#,
        );
        let found = crate::discovery::discover_all();
        let ids: Vec<&str> = found
            .trusted
            .iter()
            .map(|p| p.manifest.id.as_str())
            .collect();
        #[cfg(debug_assertions)]
        assert_eq!(ids, ["com.example.ok"]);
        assert!(!ids.contains(&"com.example.new"), "{ids:?}");
        assert!(found.rejected.iter().all(|r| r.id != "com.example.new"));
        assert!(found.reserved.is_empty());
    }
}
