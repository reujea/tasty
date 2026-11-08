//! 공식 목록에 없는 `com.tasty.*` 폴더는 서명 판정 전에, debug에서도 등록하지 않는다.

use crate::test_support::HomeEnvGuard;

fn write_plugin(root: &std::path::Path, folder: &str, id: &str) {
    let dir = root.join("plugins").join(folder);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            "manifest_version = 1\nid = \"{id}\"\nname = \"x\"\nversion = \"0.1.0\"\napi_version = \"1\"\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
        ),
    )
    .expect("manifest");
}

#[test]
fn an_unknown_reserved_id_is_neither_loaded_nor_shown_for_review() {
    let home = HomeEnvGuard::tasty_home();
    // 폴더 이름이 공식 id여도 매니페스트의 id로 판정한다.
    write_plugin(home.path(), "com.tasty.markdown", "com.tasty.fake");
    write_plugin(home.path(), "other", "com.tasty.other-fake");
    let found = super::discover_all();
    assert!(found.trusted.is_empty(), "{:?}", found.trusted);
    assert!(found.rejected.is_empty(), "{:?}", found.rejected);
    let ids: Vec<&str> = found.reserved.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["com.tasty.fake", "com.tasty.other-fake"]);
    assert!(found.reserved[0].dir.ends_with("com.tasty.markdown"));
    assert!(
        found.reserved[0]
            .reason
            .contains("reserved com.tasty. prefix")
    );
}

/// debug 시험 실행에서 서명 판정은 우회되므로 다른 접두는 등록된다.
#[cfg(debug_assertions)]
#[test]
fn other_prefixes_and_official_ids_still_load_in_debug() {
    let home = HomeEnvGuard::tasty_home();
    write_plugin(home.path(), "com.example.x", "com.example.x");
    write_plugin(home.path(), "com.tasty.image", "com.tasty.image");
    let found = super::discover_all();
    let ids: Vec<&str> = found
        .trusted
        .iter()
        .map(|p| p.manifest.id.as_str())
        .collect();
    assert_eq!(ids, ["com.example.x", "com.tasty.image"]);
    assert!(found.reserved.is_empty());
}

#[test]
fn the_manager_reports_a_blocked_folder_once_per_appearance() {
    let home = HomeEnvGuard::tasty_home();
    write_plugin(home.path(), "fake", "com.tasty.fake");
    let mut mgr = crate::PluginManager::new(std::sync::Arc::new(
        tasty_terminal::waker_factory::NoopWakerFactory,
    ));
    mgr.refresh_packages();
    mgr.refresh_packages();
    let reports = mgr.take_failure_reports();
    assert!(
        matches!(reports.as_slice(), [crate::manager::FailureReport::ReservedId { plugin_id, .. }] if plugin_id == "com.tasty.fake"),
        "{reports:?}"
    );
    assert_eq!(mgr.reserved_blocked().len(), 1);
    assert!(mgr.packages().is_empty());
    std::fs::remove_dir_all(home.path().join("plugins/fake")).expect("remove");
    mgr.refresh_packages();
    assert!(mgr.reserved_blocked().is_empty());
    write_plugin(home.path(), "fake", "com.tasty.fake");
    mgr.refresh_packages();
    assert_eq!(mgr.take_failure_reports().len(), 1);
}
