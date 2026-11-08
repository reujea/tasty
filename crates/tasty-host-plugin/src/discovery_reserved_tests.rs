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
fn an_unknown_reserved_id_is_not_loaded_and_stays_out_of_the_rejected_list() {
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
fn attention_count_includes_reserved_folders() {
    let home = HomeEnvGuard::tasty_home();
    write_plugin(home.path(), "one", "com.tasty.same");
    write_plugin(home.path(), "two", "com.tasty.same");
    let mut mgr = crate::PluginManager::new(std::sync::Arc::new(
        tasty_terminal::waker_factory::NoopWakerFactory,
    ));
    mgr.refresh_packages();
    assert_eq!(mgr.attention_count(), 2);
}

/// 경로가 같은 항목만 빠진다. 같은 id 의 다른 폴더는 남고, 다시 생긴 폴더는 새로 알린다.
#[test]
fn forgetting_a_folder_leaves_the_other_folder_with_its_id() {
    let home = HomeEnvGuard::tasty_home();
    write_plugin(home.path(), "one", "com.tasty.same");
    write_plugin(home.path(), "two", "com.tasty.same");
    let mut mgr = crate::PluginManager::new(std::sync::Arc::new(
        tasty_terminal::waker_factory::NoopWakerFactory,
    ));
    mgr.refresh_packages();
    assert_eq!(mgr.take_failure_reports().len(), 2);
    mgr.forget_reserved(&home.path().join("plugins/one"));
    assert_eq!(mgr.attention_count(), 1);
    assert!(mgr.reserved_blocked()[0].dir.ends_with("two"));
    mgr.refresh_packages();
    assert_eq!(mgr.take_failure_reports().len(), 1);
}

/// 플러그인 설정 변경은 예약 id 를 받으면 지우지 않고 Attention 탭으로 안내한다.
#[test]
fn a_reserved_id_is_told_apart_from_an_unknown_id() {
    let home = HomeEnvGuard::tasty_home();
    write_plugin(home.path(), "fake", "com.tasty.fake");
    let mut mgr = crate::PluginManager::new(std::sync::Arc::new(
        tasty_terminal::waker_factory::NoopWakerFactory,
    ));
    mgr.refresh_packages();
    let change = |id: &str| crate::manager::file_config::ConfigChange::Enabled {
        id: id.to_string(),
        enabled: false,
    };
    let reserved = mgr
        .prepare_config_change(&change("com.tasty.fake"))
        .err()
        .expect("reserved")
        .to_string();
    assert!(reserved.contains("Attention"), "{reserved}");
    let unknown = mgr
        .prepare_config_change(&change("com.tasty.other"))
        .err()
        .expect("unknown")
        .to_string();
    assert!(unknown.contains("not installed"), "{unknown}");
    assert!(home.path().join("plugins/fake/tasty-plugin.toml").exists());
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
