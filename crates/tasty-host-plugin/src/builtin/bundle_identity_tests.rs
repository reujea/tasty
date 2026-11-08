//! 폴더 이름과 다른 id를 선언한 번들은 복사하지 않고 권한도 기록하지 않는다.

use std::sync::Arc;

use super::*;
use crate::test_support::HomeEnvGuard;
use tasty_terminal::waker_factory::NoopWakerFactory;

fn markdown_spec() -> &'static BuiltinSpec {
    BUILTINS
        .iter()
        .find(|s| s.id == "com.tasty.markdown")
        .expect("markdown is bundled")
}

/// `dir`에 `id`를 선언하고 권한 하나를 요구하는 매니페스트를 쓴다.
fn write_manifest(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            "manifest_version = 1\nid = \"{id}\"\nname = \"x\"\nversion = \"0.1.0\"\napi_version = \"1\"\npermissions = [\"clipboard.read\"]\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
        ),
    )
    .unwrap();
}

fn manager() -> PluginManager {
    PluginManager::new(Arc::new(NoopWakerFactory))
}

#[test]
fn boot_install_skips_a_bundle_folder_that_declares_another_id() {
    let home = HomeEnvGuard::tasty_home();
    let spec = markdown_spec();
    let bundle = home.path().join("bundle");
    write_manifest(&bundle.join(spec.id), "com.tasty.fake");
    let dest_root = home.path().join("plugins");
    let dest = dest_root.join(spec.id);
    let mgr = manager();
    assert!(
        install_builtin_bundle_step(&mgr, spec, &dest_root, &dest, false, Some(&bundle)),
        "a mismatched bundle folder is a failure so the grant step is skipped"
    );
    assert!(!dest.exists(), "nothing is copied");
}

#[test]
fn boot_grant_ignores_an_installed_folder_that_declares_another_id() {
    let home = HomeEnvGuard::tasty_home();
    let spec = markdown_spec();
    let dest = home.path().join("plugins").join(spec.id);
    write_manifest(&dest, "com.tasty.fake");
    let mut mgr = manager();
    assert!(!install_builtin_grant_step(&mut mgr, spec, &dest));
    assert!(!mgr.config.grants.contains_key(spec.id));
    assert!(!mgr.config.grants.contains_key("com.tasty.fake"));
    write_manifest(&dest, spec.id);
    assert!(
        install_builtin_grant_step(&mut mgr, spec, &dest),
        "precondition: a matching folder records its grant"
    );
}

#[test]
fn upgrade_refuses_a_bundle_folder_that_declares_another_id() {
    let home = HomeEnvGuard::tasty_home();
    let spec = markdown_spec();
    let bundle = home.path().join("bundle");
    write_manifest(&bundle.join(spec.id), "com.tasty.fake");
    let dest_root = home.path().join("plugins");
    let mut mgr = manager();
    let result = process_builtin_upgrade(&mut mgr, spec, &dest_root, Some(&bundle), false, false);
    assert!(
        matches!(&result.item.action, BuiltinUpgradeAction::Failed { reason } if reason.contains("com.tasty.fake")),
        "{:?}",
        result.item
    );
    assert!(!result.changed);
    assert!(!dest_root.join(spec.id).exists(), "nothing is copied");
    assert!(!mgr.config.grants.contains_key(spec.id));
}
