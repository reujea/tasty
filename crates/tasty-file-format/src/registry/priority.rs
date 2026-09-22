//! `FileFormatRegistry` — priority 도메인.

use super::FileFormatRegistry;
use super::helpers::install_extension_priority;
use crate::config::ExtensionPriorityDecl;
use crate::types::{DetectorId, RuleOrigin};

impl FileFormatRegistry {
    /// `extension` 에 대한 사용자 우선순위 표. 적힌 detector id 들 (등록 여부 무관).
    /// 표에 없으면 `None`.
    pub fn extension_priority_order(&self, extension: &str) -> Option<Vec<DetectorId>> {
        let key = extension.trim_start_matches('.').to_ascii_lowercase();
        let inner = self.lock_read();
        inner.extension_priority.get(&key).map(|e| e.order.clone())
    }

    /// Settings UI 가 확장자 우선순위를 변경할 때 호출. `RuleOrigin::User` 로 entry 를
    /// 덮어쓴다 (last-writer-wins). `order` 가 비면 entry 제거.
    pub fn set_user_extension_priority(&self, extension: &str, order: Vec<DetectorId>) {
        let mut inner = self.lock_write();
        let decl = ExtensionPriorityDecl {
            extension: extension.to_string(),
            order: order.into_iter().map(|id| id.0).collect(),
        };
        install_extension_priority(&mut inner, decl, RuleOrigin::User);
    }

    /// 사용자가 명시적으로 우선순위를 해제. `extension_priority` 표에서 해당 entry 삭제.
    /// (host default 가 있던 entry 라 해도 함께 제거 — single-entry 구조)
    pub fn clear_user_extension_priority(&self, extension: &str) {
        let key = extension.trim_start_matches('.').to_ascii_lowercase();
        let mut inner = self.lock_write();
        inner.extension_priority.remove(&key);
    }

    /// 등록된 모든 `extension_priority` entry 의 키 (`md`, `json` 등). UI 에서 "현재 표가
    /// 있는 확장자" 를 보여주기 위해 사용.
    pub fn extension_priority_keys(&self) -> Vec<String> {
        let inner = self.lock_read();
        inner.extension_priority.keys().cloned().collect()
    }
}
