//! 미설치 plugin 토글은 메모리 상태와 설정 파일을 바꾸지 않는다.
use std::sync::Arc;

use super::PluginManager;
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;

const MISSING: &str = "com.example.never.installed";

#[test]
fn unknown_toggles_do_not_create_a_config_file() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let before = toml::to_string(&mgr.config).unwrap();
    for enable in [true, false] {
        let result = if enable {
            mgr.enable(MISSING)
        } else {
            mgr.disable(MISSING)
        };
        assert_eq!(
            result.unwrap_err().to_string(),
            format!("plugin '{MISSING}' not installed")
        );
        assert_eq!(toml::to_string(&mgr.config).unwrap(), before);
        assert!(!home.path().join("plugins.toml").exists());
        assert!(!mgr.is_running(MISSING));
    }
}

#[test]
fn unknown_toggles_preserve_existing_config_and_recovery_state() {
    let home = HomeEnvGuard::tasty_home();
    let path = home.path().join("plugins.toml");
    // Preserve even an old dangling disabled mark; rejection is not a cleanup command.
    let bytes = format!(
        "# user formatting stays intact\n[disabled]\nids = [\"{MISSING}\", \"com.example.other\"]\n"
    );
    std::fs::write(&path, &bytes).unwrap();
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.auto_disabled.insert(MISSING.to_string());
    let before = toml::to_string(&mgr.config).unwrap();
    for enable in [true, false] {
        let result = if enable {
            mgr.enable(MISSING)
        } else {
            mgr.disable(MISSING)
        };
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
        assert_eq!(toml::to_string(&mgr.config).unwrap(), before);
        assert!(mgr.is_auto_disabled(MISSING));
        assert!(!mgr.is_running(MISSING));
    }
}

fn installed(home: &HomeEnvGuard) -> PluginManager {
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let manifest = toml::from_str(
        r#"
manifest_version=1
id="com.example.test"
name="test"
version="1.0.0"
api_version="1"
[entry]
type="process"
command="unused"
"#,
    )
    .unwrap();
    mgr.set_packages_for_tests(vec![crate::PluginPackage {
        dir: home.path().join("plugin"),
        manifest,
    }]);
    mgr
}

#[test]
fn prepared_config_change_waits_for_receipt_and_failure_keeps_cache_unchanged() {
    use super::file_config::ConfigChange;
    use crate::registry_state::ShortcutOverride;
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    let change = ConfigChange::Shortcuts(vec![(
        "com.example.test".into(),
        "open".into(),
        Some(ShortcutOverride::None),
    )]);
    let cleared = |mgr: &PluginManager| {
        matches!(
            mgr.config.shortcut_override("com.example.test", "open"),
            Some(ShortcutOverride::None)
        )
    };
    let request = mgr.prepare_config_change(&change).unwrap();
    assert!(!cleared(&mgr));
    assert!(!home.path().join("plugins.toml").exists());
    let committed = std::thread::spawn(move || request.execute())
        .join()
        .unwrap()
        .unwrap();
    assert!(!cleared(&mgr));
    mgr.apply_config_change(committed, &change).unwrap();
    assert!(cleared(&mgr));
    let before = toml::to_string(&mgr.config).unwrap();
    std::fs::write(home.path().join("plugins.toml"), "PRIVATE = [[[\n").unwrap();
    let disabled = ConfigChange::Enabled {
        id: "com.example.test".into(),
        enabled: false,
    };
    assert!(
        mgr.prepare_config_change(&disabled)
            .unwrap()
            .execute()
            .is_err()
    );
    assert_eq!(toml::to_string(&mgr.config).unwrap(), before);
    assert!(!mgr.config.is_disabled("com.example.test"));
}

#[test]
fn failed_legacy_toggle_does_not_mutate_live_state_or_recovery_flags() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = installed(&home);
    mgr.auto_disabled.insert("com.example.test".into());
    std::fs::create_dir(home.path().join("plugins.toml")).unwrap();
    assert!(mgr.disable("com.example.test").is_err());
    assert!(!mgr.config.is_disabled("com.example.test"));
    assert!(mgr.is_auto_disabled("com.example.test"));
    assert_eq!(mgr.retiring_count(), 0);
}

#[test]
fn finishing_a_file_swap_does_not_claim_an_existing_process_was_restarted() {
    let _home = HomeEnvGuard::tasty_home();
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    let _stub = mgr.attach_namespace_stub_for_test("com.example.swap-report", "swapreport");
    assert!(mgr.is_running("com.example.swap-report"));
    assert!(!mgr.finish_file_swap("com.example.swap-report").unwrap());
    assert!(mgr.is_running("com.example.swap-report"));
    assert!(mgr.finish_file_swap(MISSING).is_err());
}
