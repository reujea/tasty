//! 확인 필요 탭 항목과 사이드바 배지는 따로 계산한다. 같은 항목을 세는지 함께 본다.

use std::sync::Arc;

use tasty_terminal::waker_factory::NoopWakerFactory;

use super::attention_entries;
use crate::plugin::PluginManager;
use crate::plugins_ui::AttentionKind;

/// 이 스레드의 데이터 루트를 임시 폴더로 고정한다. 환경 변수는 바꾸지 않는다.
struct TempHome(tempfile::TempDir);

impl TempHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        tasty_utils::path::push_home_override(dir.path().to_path_buf());
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        self.0.path()
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        tasty_utils::path::pop_home_override();
    }
}

fn write_plugin(root: &std::path::Path, folder: &str, id: &str) {
    let dir = root.join("plugins").join(folder);
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(
        dir.join("tasty-plugin.toml"),
        format!(
            "manifest_version = 1\nid = \"{id}\"\nname = \"Tasty Official\"\nversion = \"0.1.0\"\napi_version = \"1\"\nauthors = [\"Tasty\"]\ndescription = \"Official Tasty plugin\"\nhomepage = \"https://example.com\"\n[entry]\ntype = \"process\"\ncommand = \"x\"\n"
        ),
    )
    .expect("manifest");
}

fn manager() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.refresh_packages();
    mgr
}

#[test]
fn the_sidebar_and_tab_counts_agree() {
    let home = TempHome::new();
    write_plugin(home.path(), "one", "com.tasty.same");
    write_plugin(home.path(), "two", "com.tasty.same");
    let mgr = manager();
    assert_eq!(attention_entries(&mgr).len(), 2);
    assert_eq!(mgr.attention_count(), 2);
}

/// 예약 id 항목은 폴더와 id 만 싣는다. 매니페스트의 글은 공식 플러그인처럼 적혀 있을 수 있다.
#[test]
fn a_reserved_entry_carries_no_manifest_text() {
    let home = TempHome::new();
    write_plugin(home.path(), "folder-name", "com.tasty.fake");
    let mgr = manager();
    let entries = attention_entries(&mgr);
    let [entry] = entries.as_slice() else {
        panic!("{entries:?}");
    };
    assert_eq!(entry.kind, AttentionKind::ReservedId);
    assert_eq!(entry.name, "folder-name");
    assert_eq!(entry.id, "com.tasty.fake");
    assert!(entry.version.is_empty() && entry.authors.is_empty());
    assert!(entry.description.is_empty() && entry.homepage.is_empty());
    assert!(
        entry
            .folder
            .as_ref()
            .is_some_and(|dir| dir.ends_with("folder-name"))
    );
}
