//! 서버가 강제로 끊은 저장된 원격 행의 연결 대상을 remote-force-detached.toml에 남긴다.
//! 이 목록에 있는 행은 클라이언트를 다시 시작해도 자동으로 붙지 않는다. 명시적 연결(연결 버튼·메뉴·
//! `tasty remote connect`)을 시작하거나 그 행의 연결이 성공하면 지운다. 저장 행 ID는 실행마다 바뀔 수 있어
//! 연결 대상(attach 프로필 이름과 원격 workspace id)으로 찾는다.
//! 저장은 [`crate::core::shared_list::SharedList`]가 맡는다.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::RemoteBinding;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForceDetachedRows {
    #[serde(default, rename = "row")]
    pub items: Vec<RemoteBinding>,
}

impl ForceDetachedRows {
    pub fn config_path() -> Option<PathBuf> {
        tasty_utils::path::tasty_home().map(|dir| dir.join("remote-force-detached.toml"))
    }

    pub fn contains(&self, binding: &RemoteBinding) -> bool {
        self.items.contains(binding)
    }

    pub fn insert(&mut self, binding: RemoteBinding) {
        if !self.contains(&binding) {
            self.items.push(binding);
        }
    }

    pub fn remove(&mut self, binding: &RemoteBinding) {
        self.items.retain(|item| item != binding);
    }
}

impl crate::core::shared_list::PersistedList for ForceDetachedRows {
    const NAME: &'static str = "remote force-detached rows";
    fn config_path() -> Option<PathBuf> {
        Self::config_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(profile: &str, remote_workspace: u32) -> RemoteBinding {
        RemoteBinding {
            profile: profile.into(),
            remote_workspace,
        }
    }

    #[test]
    fn a_mark_is_kept_once_and_survives_a_toml_roundtrip() {
        let mut rows = ForceDetachedRows::default();
        rows.insert(binding("gx10", 7));
        rows.insert(binding("gx10", 7));
        rows.insert(binding("mac", 3));
        assert_eq!(rows.items.len(), 2);
        let text = toml::to_string_pretty(&rows).unwrap();
        let back: ForceDetachedRows = toml::from_str(&text).unwrap();
        assert_eq!(back, rows);
        rows.remove(&binding("gx10", 7));
        assert!(!rows.contains(&binding("gx10", 7)));
        assert!(rows.contains(&binding("mac", 3)));
    }

    #[test]
    fn a_missing_file_body_reads_as_no_marks() {
        let rows: ForceDetachedRows = toml::from_str("").unwrap();
        assert!(rows.items.is_empty());
    }
}
