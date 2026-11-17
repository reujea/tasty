//! 매니페스트의 상속 환경변수 제거 선언과 namespace 소유 상태가 설치·설정만 따라 공용 표에
//! 반영되는지 확인한다.

use std::sync::Arc;

use tasty_plugin_manifest::{Manifest, PluginPackage};
use tasty_terminal::waker_factory::NoopWakerFactory;
use tasty_utils::process::{is_stripped_inherited_env, plugin_inherited_env_rules};

use crate::manager::PluginManager;

// 다른 시험과 공용 표를 공유하므로 이 파일만 쓰는 ID와 키를 사용한다.
const VENDOR: &str = "com.example.inherited_env_vendor";
const PLAIN: &str = "com.example.inherited_env_plain";
const SESSION_KEY: &str = "XTEST_MGR_VENDOR_SESSION";

/// VENDOR 가 소유하는 namespace. 공용 소유 표를 반영하는 시험은 이 파일의 하나뿐이다.
const VENDOR_NS: &str = "xtestvendor";

fn pkg(id: &str, strip: &str) -> PluginPackage {
    let namespace = if id == VENDOR {
        format!("[[contributes.ipc_namespace]]\nprefix = \"{VENDOR_NS}\"\n")
    } else {
        String::new()
    };
    let manifest: Manifest = toml::from_str(&format!(
        r#"
manifest_version = 1
id = "{id}"
name = "Inherited Env Fixture"
version = "0.1.0"
api_version = "1.0"

{namespace}

[contributes.strip_inherited_env]
{strip}

[entry]
type = "process"
command = "fake-bin"
args = []
"#
    ))
    .expect("fixture manifest should parse");
    PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/inherited-env-fixture"),
        manifest,
    }
}

fn manager() -> PluginManager {
    let mut m = PluginManager::new(Arc::new(NoopWakerFactory));
    m.set_packages_for_tests(vec![
        pkg(
            VENDOR,
            &format!(
                r#"names = ["{SESSION_KEY}"]
values = [{{ name = "XTEST_MGR_AGENT", prefixes = ["vendor_"] }}]"#
            ),
        ),
        pkg(PLAIN, ""),
    ]);
    m
}

fn stripped(key: &str, value: &str) -> bool {
    is_stripped_inherited_env(key, std::ffi::OsStr::new(value))
}

/// 프로세스를 실행하지 않아도 설치 목록만으로 규칙이 생기고, 선언이 없는 플러그인은 항목이 없다.
#[test]
fn rules_come_from_installed_manifests_without_starting_processes() {
    let m = manager();
    assert!(!m.is_running(VENDOR));
    let rules = m.inherited_env_rules();
    assert_eq!(rules.keys().collect::<Vec<_>>(), [VENDOR]);
    assert_eq!(rules[VENDOR].names, [SESSION_KEY]);
    assert_eq!(
        rules[VENDOR].value_prefixes,
        [("XTEST_MGR_AGENT".to_string(), vec!["vendor_".to_string()])]
    );
    m.debug_assert_extensions_fresh();
}

/// 반영을 켜지 않은 매니저는 공용 표를 바꾸지 않는다. 반영하는 시험과 병렬로 돌아도 결과가
/// 섞이지 않도록 이 시험에만 쓰는 ID를 쓴다.
#[test]
fn a_manager_that_does_not_publish_leaves_the_shared_table_alone() {
    const QUIET: &str = "com.example.inherited_env_quiet";
    let mut m = PluginManager::new(Arc::new(NoopWakerFactory));
    m.set_packages_for_tests(vec![pkg(QUIET, r#"names = ["XTEST_MGR_QUIET"]"#)]);
    assert!(m.inherited_env_rules().contains_key(QUIET));
    assert!(!plugin_inherited_env_rules().contains_key(QUIET));
}

/// 반영을 켠 뒤 비활성화하면 그 규칙이 빠지고, 다시 활성화하면 이후 판정에 다시 적용된다.
/// 이 파일에서 공용 표에 반영하는 시험은 이것 하나다.
#[test]
fn publishing_follows_disable_and_enable() {
    let mut m = manager();
    m.publish_shared_tables();
    let published = (
        stripped(SESSION_KEY, "outer"),
        stripped("XTEST_MGR_AGENT", "vendor_1"),
        stripped("XTEST_MGR_AGENT", "user"),
    );

    let owner_enabled = tasty_utils::plugin_namespaces::lookup(VENDOR_NS);

    m.config.disable(VENDOR);
    m.recompute_extensions();
    let owner_disabled = tasty_utils::plugin_namespaces::lookup(VENDOR_NS);
    let after_disable = (
        stripped(SESSION_KEY, "outer"),
        plugin_inherited_env_rules().contains_key(VENDOR),
    );

    m.config.enable(VENDOR);
    m.recompute_extensions();
    let after_enable = stripped(SESSION_KEY, "outer");

    // 제거된 것과 같은 상태로 설치 목록에서 빠져도 규칙이 남지 않는다.
    m.set_packages_for_tests(vec![pkg(PLAIN, "")]);
    let after_removal = stripped(SESSION_KEY, "outer");
    let owner_removed = tasty_utils::plugin_namespaces::lookup(VENDOR_NS);

    use tasty_utils::plugin_namespaces::Lookup;
    let Lookup::Owner(enabled) = owner_enabled else {
        panic!("published owner: {owner_enabled:?}");
    };
    assert!(enabled.enabled && enabled.plugin_id == VENDOR);
    assert!(matches!(owner_disabled, Lookup::Owner(o) if !o.enabled && o.plugin_id == VENDOR));
    assert_eq!(owner_removed, Lookup::NoOwner);

    assert_eq!(published, (true, true, false));
    assert_eq!(after_disable, (false, false));
    assert!(after_enable);
    assert!(!after_removal);
}

/// 설정만 바꾸고 다시 계산하지 않으면 debug 신선도 검사가 잡는다.
#[test]
#[should_panic(expected = "상속 환경변수 제거 규칙이 낡았다")]
fn a_config_write_without_recompute_is_caught() {
    let mut m = manager();
    m.config.disable(VENDOR);
    // 확장 상태는 이 설정 변경과 무관하므로 제거 규칙 검사만 실패한다.
    m.debug_assert_extensions_fresh();
}

/// namespace 소유 상태는 프로세스 없이 설치 목록과 설정으로 정해진다. 비활성 소유자도
/// "소유자 없음"과 구분되게 남는다.
#[test]
fn namespace_owners_follow_disable_without_processes() {
    let mut m = manager();
    assert!(!m.is_running(VENDOR));
    let owner = |m: &PluginManager| m.namespace_owners()[VENDOR_NS].clone();
    assert_eq!(owner(&m).plugin_id, VENDOR);
    assert!(owner(&m).enabled);
    m.config.disable(VENDOR);
    m.recompute_extensions();
    assert!(!owner(&m).enabled);
    assert_eq!(
        m.namespace_owners().len(),
        1,
        "plugins without a namespace have no entry"
    );
    m.debug_assert_extensions_fresh();
}

/// namespace 선언만 바꾸고 다시 계산하지 않으면 debug 신선도 검사가 잡는다.
#[test]
#[should_panic(expected = "namespace 소유 상태가 낡았다")]
fn a_namespace_change_without_recompute_is_caught() {
    let mut m = manager();
    let vendor = m
        .packages
        .iter_mut()
        .find(|p| p.manifest.id == VENDOR)
        .expect("vendor package");
    vendor.manifest.contributes.ipc_namespace.clear();
    m.debug_assert_extensions_fresh();
}
