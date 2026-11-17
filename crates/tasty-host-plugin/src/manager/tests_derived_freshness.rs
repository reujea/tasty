//! 확장 상태를 계산한 뒤 원본을 바꾸면 debug 검사가 실패하는지 확인한다.

use std::sync::Arc;

use tasty_plugin_manifest::{Manifest, PluginPackage};
use tasty_terminal::waker_factory::NoopWakerFactory;

use crate::manager::PluginManager;

const TARGET: &str = "com.example.freshness_target";
const EXT: &str = "com.example.freshness_ext";

fn manifest_toml(id: &str, extends: bool) -> String {
    let head = format!(
        r#"
manifest_version = 1
id = "{id}"
name = "Freshness Fixture"
version = "0.1.0"
api_version = "1.0"

[entry]
type = "process"
command = "fake-bin"
args = []
"#
    );
    if extends {
        format!(
            r#"{head}
[extends]
plugin_id = "{TARGET}"
version_req = "*"
api_version = "1.0"
"#
        )
    } else {
        head
    }
}

fn pkg(id: &str, extends: bool) -> PluginPackage {
    let manifest: Manifest =
        toml::from_str(&manifest_toml(id, extends)).expect("fixture manifest should parse");
    PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/freshness-fixture"),
        manifest,
    }
}

/// 현재 설정으로 확장 상태까지 계산한 매니저.
fn manager_with_active_extension() -> PluginManager {
    let mut m = PluginManager::new(Arc::new(NoopWakerFactory));
    m.packages.push(pkg(TARGET, false));
    m.packages.push(pkg(EXT, true));
    m.recompute_extensions();
    m
}

/// 원본과 계산 결과가 같으면 통과한다.
#[test]
fn a_freshly_recomputed_set_passes() {
    let m = manager_with_active_extension();
    m.debug_assert_extensions_fresh();
}

/// 계산 뒤 설정만 바꾸면 실패해야 한다.
#[test]
#[should_panic(expected = "확장 집합이 낡았다")]
fn a_source_write_after_the_derivation_is_caught() {
    let mut m = manager_with_active_extension();
    // 확장 상태를 다시 계산하지 않고 설정만 바꾼다.
    m.config.disable(EXT);
    m.debug_assert_extensions_fresh();
}

/// 변경한 설정으로 다시 계산하면 통과해야 한다.
#[test]
fn recomputing_after_the_write_makes_it_fresh_again() {
    let mut m = manager_with_active_extension();
    m.config.disable(EXT);
    m.recompute_extensions();
    m.debug_assert_extensions_fresh();
}
