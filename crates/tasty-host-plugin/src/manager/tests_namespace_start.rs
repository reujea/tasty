//! Namespace admission rejects before allocating a listener or starting a process.
use std::sync::Arc;

use super::PluginManager;
use crate::test_support::HomeEnvGuard;
use tasty_plugin_manifest::{Manifest, PluginPackage};
use tasty_terminal::waker_factory::NoopWakerFactory;

fn owner_manager() -> PluginManager {
    let manifest: Manifest = toml::from_str(
        r#"manifest_version=1
id="com.example.owner"
name="Owner"
version="0.1.0"
api_version="1"
[entry]
type="process"
command="must-not-start"
[[contributes.ipc_namespace]]
prefix="owner"
"#,
    )
    .unwrap();
    let mut mgr = PluginManager::new(Arc::new(NoopWakerFactory));
    mgr.set_packages_for_tests(vec![PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/owner"),
        manifest,
    }]);
    mgr
}

#[test]
fn unknown_or_own_namespace_does_not_start_a_listener() {
    let _home = HomeEnvGuard::tasty_home();
    let mut mgr = owner_manager();
    assert_eq!(
        mgr.validate_namespace_call("unknown.echo", None)
            .unwrap_err()
            .0,
        -32601
    );
    assert_eq!(
        mgr.validate_namespace_call("owner.echo", Some("com.example.owner"))
            .unwrap_err()
            .0,
        -32001
    );
    assert!(mgr.listener.is_none());
    assert!(mgr.processes.is_empty());
}

#[test]
fn disabled_and_auto_disabled_owners_remain_stopped() {
    let home = HomeEnvGuard::tasty_home();
    let mut mgr = owner_manager();
    mgr.disable("com.example.owner").unwrap();
    let path = home.path().join("plugins.toml");
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(
        mgr.validate_namespace_call("owner.echo", None)
            .unwrap_err()
            .0,
        -32002
    );
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(mgr.listener.is_none());
    assert!(mgr.processes.is_empty());
    let mut mgr = owner_manager();
    // Separate the automatic health stop from the user-disabled configuration.
    mgr.config.enable("com.example.owner");
    mgr.auto_disabled.insert("com.example.owner".into());
    assert_eq!(
        mgr.validate_namespace_call("owner.echo", None)
            .unwrap_err()
            .0,
        -32002
    );
    assert!(mgr.listener.is_none());
    assert!(mgr.processes.is_empty());
}

/// 등록 호출을 순서대로 기록한다.
#[derive(Default)]
struct RecordingRegistrar(std::sync::Mutex<Vec<(String, std::path::PathBuf)>>);

impl tasty_plugin_protocol::host_port::I18nNamespaceRegistrar for RecordingRegistrar {
    fn register(&self, namespace: &str, lang_dir: &std::path::Path) {
        self.0
            .lock()
            .unwrap()
            .push((namespace.to_string(), lang_dir.to_path_buf()));
    }
    fn unregister(&self, _namespace: &str) {}
}

/// headless의 kind 기동은 전체 기동을 거치지 않으므로 이 경로에서도 탭 이름의 번역이 준비돼야 한다.
#[test]
fn starting_one_owner_registers_its_translations() {
    let _home = HomeEnvGuard::tasty_home();
    let mut mgr = owner_manager();
    let registrar = Arc::new(RecordingRegistrar::default());
    mgr.set_i18n_registrar(registrar.clone());
    mgr.start_one_enabled("com.example.owner");
    assert_eq!(
        *registrar.0.lock().unwrap(),
        vec![(
            "com.example.owner".to_string(),
            std::path::PathBuf::from("/nonexistent/owner/lang")
        )]
    );
}

/// 비활성 플러그인은 실행하지 않으며 이 경로에서 번역도 등록하지 않는다.
#[test]
fn a_disabled_owner_is_not_registered_by_a_kind_start() {
    let _home = HomeEnvGuard::tasty_home();
    let mut mgr = owner_manager();
    mgr.disable("com.example.owner").unwrap();
    let registrar = Arc::new(RecordingRegistrar::default());
    mgr.set_i18n_registrar(registrar.clone());
    assert!(!mgr.start_one_enabled("com.example.owner"));
    assert!(registrar.0.lock().unwrap().is_empty());
}
