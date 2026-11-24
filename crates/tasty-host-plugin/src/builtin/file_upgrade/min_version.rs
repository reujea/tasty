//! 번들 원본의 `min_tasty_version`을 복사·교체 전에 비교한다.
//! 버전 읽기(`read_installed_version` 등)는 비교하지 않아 상위 설치본을 그대로 두고,
//! 실제로 파일을 바꾸기 직전에만 이 판정을 쓴다.

use std::path::Path;

use tasty_plugin_manifest::Manifest;

/// 번들 원본의 매니페스트가 TOML이 아니거나, `min_tasty_version`이 형식 오류이거나 실행 중인
/// Tasty보다 높은 버전을 요구해 받아들일 수 없으면 그 거절을 돌려준다. 버전 부족을 경고만 하는
/// 빌드에서는 그 경우만 `None`이다. 로그는 Display(영어), 사용자 보고는
/// [`crate::manifest_error::describe`]로 쓴다. 매니페스트 파일을 읽지 못하면 `None`이다.
/// 번들 경로는 이 판정을 id 대조·서명 읽기보다 먼저 한다.
pub(crate) fn bundle_requires_newer_tasty(src: &Path) -> Option<anyhow::Error> {
    Manifest::admit_bundle_host_tasty(src).err()
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
    /// 계획은 서명 파일을 읽기 전에 버전을 판정하므로 이 시험은 그 앞선 판정을 잰다.
    /// `verify_trust` 안의 순서는 `verify_trust_reports_the_version_before_identity_and_signature`가
    /// 잰다. debug 빌드는 서명 결과로 거절하지 않아 서명 판정과의 순서는 release 프로필로
    /// 돌릴 때만 가려진다(자동 실행은 debug).
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

    /// `bundle`의 com.tasty.html 하나로 upgrade-builtins 계획을 세워 그 항목을 돌려준다.
    fn plan_html(home: &Path) -> BuiltinUpgradeAction {
        let plan = prepare(
            home.join("installed"),
            Some(home.join("bundle")),
            home.join("known.toml"),
            PluginsConfig::load_from(home.join("plugins.toml")),
            false,
            &[],
            false,
        )
        .expect("plan");
        plan.entries
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Done(item) if item.id == "com.tasty.html" => Some(item.action),
                _ => None,
            })
            .expect("refused before any write")
    }

    fn failed_reason(action: BuiltinUpgradeAction) -> String {
        match action {
            BuiltinUpgradeAction::Failed { reason } => reason,
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// 다른 id를 선언한 번들도 더 높은 Tasty를 요구하면 버전 사유가 먼저다(부팅·업그레이드).
    #[test]
    fn a_newer_bundle_with_another_id_is_refused_for_the_version_first() {
        crate::test_support::host_tasty_1_0_0();
        let home = tempfile::tempdir().expect("tempdir");
        let src = home.path().join("bundle/com.tasty.html");
        plugin(&src, "com.tasty.other", r#"min_tasty_version = "99.0.0""#);
        let err = crate::official::bundle_source(&home.path().join("bundle"), "com.tasty.html")
            .expect_err("refused");
        assert!(err.contains("99.0.0"), "{err}");
        let reason = failed_reason(plan_html(home.path()));
        assert!(reason.contains("99.0.0"), "{reason}");
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }

    /// 서명 파일을 읽지 못하는 번들도 더 높은 Tasty를 요구하면 버전 사유가 먼저다.
    #[test]
    fn a_newer_bundle_with_an_unreadable_signature_is_refused_for_the_version_first() {
        crate::test_support::host_tasty_1_0_0();
        let home = tempfile::tempdir().expect("tempdir");
        let src = home.path().join("bundle/com.tasty.html");
        plugin(&src, "com.tasty.html", r#"min_tasty_version = "99.0.0""#);
        // 폴더는 '없음'이 아닌 읽기 오류라 서명 파일 읽기가 실패한다.
        std::fs::create_dir(src.join("tasty-plugin.toml.sig")).expect("sig dir");
        let reason = failed_reason(plan_html(home.path()));
        assert!(reason.contains("99.0.0"), "{reason}");
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }

    /// TOML로 읽히지 않는 번들 매니페스트는 복사 전에 거절한다(부팅 선택·업그레이드 계획).
    #[test]
    fn a_bundle_manifest_that_is_not_toml_is_never_copied() {
        crate::test_support::host_tasty_1_0_0();
        let home = tempfile::tempdir().expect("tempdir");
        let src = home.path().join("bundle/com.tasty.html");
        plugin(&src, "com.tasty.html", "min_tasty_version = 0.15.0");
        let err = crate::official::bundle_source(&home.path().join("bundle"), "com.tasty.html")
            .expect_err("refused");
        assert!(err.contains("invalid manifest"), "{err}");
        let reason = failed_reason(plan_html(home.path()));
        assert!(reason.contains("invalid manifest"), "{reason}");
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }

    /// UTF-8로 풀리지 않는 번들 매니페스트는 복사 전에 거절한다(부팅 선택·업그레이드 계획).
    #[test]
    fn a_bundle_manifest_that_is_not_utf8_is_never_copied() {
        crate::test_support::host_tasty_1_0_0();
        let home = tempfile::tempdir().expect("tempdir");
        let src = home.path().join("bundle/com.tasty.html");
        plugin(&src, "com.tasty.html", "");
        let mut bytes = std::fs::read(src.join("tasty-plugin.toml")).expect("read");
        bytes.extend_from_slice(b"description = \"\xff\xfe\"\n");
        std::fs::write(src.join("tasty-plugin.toml"), bytes).expect("write");
        let err = crate::official::bundle_source(&home.path().join("bundle"), "com.tasty.html")
            .expect_err("refused");
        assert!(err.contains("cannot read"), "{err}");
        let reason = failed_reason(plan_html(home.path()));
        assert!(reason.contains("cannot read"), "{reason}");
        assert!(!home.path().join("installed/com.tasty.html").exists());
    }

    /// `verify_trust`(복사본 재확인·공개 `verify_builtin_bundle_trust`가 부른다) 안에서도
    /// 버전 판정이 id 대조·서명 판정보다 먼저다. 계획의 앞선 판정을 거치지 않게 직접 부른다.
    /// 서명과의 순서는 서명으로 거절하는 release 프로필에서만 가려진다.
    #[test]
    fn verify_trust_reports_the_version_before_identity_and_signature() {
        crate::test_support::host_tasty_1_0_0();
        let spec = crate::builtin::BUILTINS
            .iter()
            .find(|spec| spec.id == "com.tasty.html")
            .expect("html builtin");
        let known = crate::known_plugins::KnownPlugins::default();
        for (name, declared, bad_signature) in [
            ("another id", "com.tasty.other", false),
            ("bad signature", "com.tasty.html", true),
        ] {
            let home = tempfile::tempdir().expect("tempdir");
            let src = home.path().join("com.tasty.html");
            plugin(&src, declared, r#"min_tasty_version = "99.0.0""#);
            if bad_signature {
                std::fs::write(src.join("tasty-plugin.toml.sig"), "not a signature").expect("sig");
            }
            let item = super::super::verify_trust(spec, &src, &home.path().join("dest"), &known)
                .expect_err(name);
            let reason = failed_reason(item.action);
            assert!(reason.contains("99.0.0"), "{name}: {reason}");
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
