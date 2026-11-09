use super::*;

#[test]
fn an_unknown_reserved_id_is_refused() {
    let err = admit_plugin_id("com.tasty.fake").expect_err("unknown com.tasty id must be refused");
    assert_eq!(err.id, "com.tasty.fake");
    assert!(
        err.to_string().contains("reserved com.tasty. prefix"),
        "{err}"
    );
}

#[test]
fn every_official_id_is_admitted() {
    assert_eq!(OFFICIAL_PLUGIN_IDS.len(), 10);
    for id in OFFICIAL_PLUGIN_IDS {
        assert_eq!(admit_plugin_id(id), Ok(()), "{id}");
    }
    // 번들하지 않는 공식 플러그인도 목록에 있다.
    assert!(OFFICIAL_PLUGIN_IDS.contains(&"com.tasty.agent-stream"));
    assert!(OFFICIAL_PLUGIN_IDS.contains(&"com.tasty.mesh-demo"));
}

#[test]
fn ids_outside_the_reserved_prefix_are_admitted() {
    for id in [
        "com.example.x",
        "com.tastyfoo.x",
        "org.tasty.x",
        "com.tasty",
        "io.github.user.tasty",
    ] {
        assert_eq!(admit_plugin_id(id), Ok(()), "{id}");
    }
}

#[test]
fn bundled_ids_of_this_build_are_official() {
    assert!(!crate::builtin::BUILTINS.is_empty());
    for spec in crate::builtin::BUILTINS {
        assert!(
            OFFICIAL_PLUGIN_IDS.contains(&spec.id),
            "bundled '{}' is missing from OFFICIAL_PLUGIN_IDS",
            spec.id
        );
    }
}

/// 번들 목록은 OS별로 두 벌이다. 이 빌드가 고르지 않은 벌도 원문에서 읽어 대조한다.
#[test]
fn bundled_ids_of_every_os_are_official() {
    let source = include_str!("builtin.rs");
    let table = &source[..source
        .find("pub fn is_builtin_plugin")
        .expect("builtin.rs layout")];
    let ids: Vec<&str> = table
        .lines()
        .filter_map(|l| l.trim().strip_prefix("id: \""))
        .filter_map(|l| l.strip_suffix("\","))
        .collect();
    assert_eq!(ids.len(), 20, "two tables of ten bundled ids: {ids:?}");
    for id in ids {
        assert!(
            OFFICIAL_PLUGIN_IDS.contains(&id),
            "bundled '{id}' is not official"
        );
    }
}

/// 이 레포가 내는 플러그인 매니페스트의 id는 모두 공식 목록에 있어야 설치된다.
#[test]
fn repository_plugin_manifests_are_official() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut seen = 0;
    for entry in std::fs::read_dir(&crates).expect("crates dir").flatten() {
        let dir = entry.path();
        if !dir.join("tasty-plugin.toml").is_file() {
            continue;
        }
        let manifest = tasty_plugin_manifest::Manifest::load(&dir).expect("manifest");
        assert_eq!(admit_plugin_id(&manifest.id), Ok(()), "{}", dir.display());
        seen += 1;
    }
    assert_eq!(seen, 10, "plugin manifests under crates/");
}

#[test]
fn a_bundle_folder_must_declare_its_own_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = |id: &str| {
        format!(
            "manifest_version = 1\nid = \"{id}\"\nname = \"x\"\nversion = \"0.1.0\"\napi_version = \"1\"\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
        )
    };
    std::fs::write(
        dir.path().join("tasty-plugin.toml"),
        manifest("com.tasty.fake"),
    )
    .unwrap();
    let reason = bundle_identity_mismatch("com.tasty.markdown", dir.path())
        .expect("mismatch must be reported");
    assert!(reason.contains("com.tasty.fake"), "{reason}");
    std::fs::write(
        dir.path().join("tasty-plugin.toml"),
        manifest("com.tasty.markdown"),
    )
    .unwrap();
    assert_eq!(
        bundle_identity_mismatch("com.tasty.markdown", dir.path()),
        None
    );
}
