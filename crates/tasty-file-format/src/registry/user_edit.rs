//! `FileFormatRegistry` — user_edit 도메인.

use std::path::Path;

use tracing::warn;

use super::helpers::{
    install_extension_priority, install_one, parse_detector_section,
    parse_extension_priority_section,
};
use super::{DetectorContribution, FileFormatRegistry};
use crate::config::{DetectorDecl, ExtensionPriorityDecl};
use crate::types::{DetectorId, RuleOrigin};

impl FileFormatRegistry {
    /// Settings UI 가 host/plugin detector 를 user-origin override 로 disable/enable.
    /// 명시적 user 의도를 표현하므로 항상 `disabled_override = Some(value)` 로 push 한다.
    /// "default 로 되돌리기" 는 `clear_user_detector_override` 또는 `remove_user_detector`.
    pub fn set_user_detector_disabled(&self, id: &DetectorId, disabled: bool) {
        let mut inner = self.lock_write();
        let Some(entry) = inner.contributions.get_mut(id) else {
            warn!(
                detector = id.as_str(),
                "file_format: set_user_detector_disabled — unknown detector"
            );
            return;
        };
        // 같은 detector 에 이미 user contribution 이 있으면 (rules/메타 포함) 그 disabled_override 만
        // 갱신해 다른 필드를 보존. 없으면 disabled-only contribution 신규 push.
        if let Some(existing) = entry
            .iter_mut()
            .find(|c| matches!(c.origin, RuleOrigin::User))
        {
            existing.disabled_override = Some(disabled);
        } else {
            entry.push(DetectorContribution {
                origin: RuleOrigin::User,
                display_name_i18n_key: None,
                icon: None,
                disabled_override: Some(disabled),
                rules: Vec::new(),
            });
        }
        inner.dirty = true;
    }

    /// User-origin contribution 의 `disabled_override` 만 None 으로 비운다. 다른 user 필드
    /// (rule/메타) 는 보존. user 가 명시적 disable 의도를 철회할 때 사용.
    pub fn clear_user_detector_override(&self, id: &DetectorId) {
        let mut inner = self.lock_write();
        let Some(entry) = inner.contributions.get_mut(id) else {
            return;
        };
        let mut empty_user = false;
        if let Some(existing) = entry
            .iter_mut()
            .find(|c| matches!(c.origin, RuleOrigin::User))
        {
            existing.disabled_override = None;
            empty_user = existing.rules.is_empty()
                && existing.display_name_i18n_key.is_none()
                && existing.icon.is_none();
        }
        if empty_user {
            entry.retain(|c| !matches!(c.origin, RuleOrigin::User));
            if entry.is_empty() {
                inner.contributions.remove(id);
                inner.install_order.remove(id);
            }
        }
        inner.dirty = true;
    }

    /// Settings UI 가 user-origin contribution 전체를 제거. host/plugin 은 보존.
    /// 해당 detector 의 다른 출처가 없었다면 (= user-only) detector 전체가 사라진다.
    pub fn remove_user_detector(&self, id: &DetectorId) {
        let mut inner = self.lock_write();
        let Some(entry) = inner.contributions.get_mut(id) else {
            return;
        };
        entry.retain(|c| !matches!(c.origin, RuleOrigin::User));
        if entry.is_empty() {
            inner.contributions.remove(id);
            inner.install_order.remove(id);
        }
        inner.dirty = true;
    }

    /// Settings UI 가 user-origin detector 를 추가/갱신. 기존 host/plugin 이 있으면 patch
    /// (rule union + 메타 last-writer-wins). schema validation 실패 시 변경 없이 에러 반환.
    pub fn upsert_user_detector(
        &self,
        decl: DetectorDecl,
    ) -> Result<(), crate::config::DetectorDeclError> {
        // user 영역도 `$` 예약 id 는 host 만 정의 가능 → from_plugin = true 와 동일 규칙.
        // 단 user 가 host 의 예약 detector ($directory) 를 patch 하는 시나리오는 있을 수 있어,
        // 이미 등록된 id 면 허용.
        let already_exists = self
            .lock_read()
            .contributions
            .contains_key(&DetectorId(decl.id.clone()));
        let from_restricted = !already_exists;
        let warnings = crate::config::validate_detector_decl(&decl, from_restricted)?;
        for w in warnings {
            warn!(warning = %w, "file_format: user detector decl warning");
        }
        // poison 을 `InvalidId("lock poisoned")` 으로 보고하던 자리다 — 사용자에게
        // detector id 가 틀렸다고 말하면서 진짜 원인은 남기지 않았다.
        let mut inner = self.lock_write();
        install_one(
            &mut inner,
            &self.next_install_order,
            decl,
            RuleOrigin::User,
            false,
        );
        inner.dirty = true;
        Ok(())
    }

    /// 사용자 설정 파일을 다시 읽어 user origin contribution 만 교체. host + plugin 은 그대로.
    ///
    /// **Transactional**: 파일 read/parse 단계에서 실패하면 기존 user contribution 을 보존한다
    /// (write lock 잡기 전에 검증). 파일이 없으면 user contribution 만 제거 (= 설정 삭제).
    pub fn reload_user_config(&self, path: &Path) {
        let Some((decls, priorities)) = Self::load_user_decls(path) else {
            return;
        };
        let mut inner = self.lock_write();
        Self::purge_user_contributions(&mut inner);
        // 새 user contribution install.
        for decl in decls {
            install_one(
                &mut inner,
                &self.next_install_order,
                decl,
                RuleOrigin::User,
                false,
            );
        }
        for p in priorities {
            install_extension_priority(&mut inner, p, RuleOrigin::User);
        }
        inner.dirty = true;
    }

    /// user config 파일을 읽어 detector/extension-priority 선언을 파싱한다. 파일이
    /// 없으면 빈 목록(= user override 전부 해제), 읽기/파싱 실패는 이미 warn 로그
    /// 후 `None`(호출자는 기존 user config 를 그대로 유지).
    fn load_user_decls(path: &Path) -> Option<(Vec<DetectorDecl>, Vec<ExtensionPriorityDecl>)> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let d = match parse_detector_section(&text) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "file_format: reload aborted — parse failed, keeping previous user config",
                        );
                        return None;
                    }
                };
                let p = parse_extension_priority_section(&text).unwrap_or_default();
                Some((d, p))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some((Vec::new(), Vec::new())),
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "file_format: reload aborted — read failed, keeping previous user config",
                );
                None
            }
        }
    }

    /// 기존 user-origin contribution + extension_priority 항목을 모두 제거한다
    /// (reload 는 항상 전체 재설치이므로 재설치 전 정리).
    fn purge_user_contributions(inner: &mut super::Inner) {
        let mut empty_ids = Vec::new();
        for (id, contribs) in inner.contributions.iter_mut() {
            contribs.retain(|c| !matches!(c.origin, RuleOrigin::User));
            if contribs.is_empty() {
                empty_ids.push(id.clone());
            }
        }
        for id in empty_ids {
            inner.contributions.remove(&id);
            inner.install_order.remove(&id);
        }
        let user_keys: Vec<String> = inner
            .extension_priority
            .iter()
            .filter(|(_, v)| matches!(v.origin, RuleOrigin::User))
            .map(|(k, _)| k.clone())
            .collect();
        for k in user_keys {
            inner.extension_priority.remove(&k);
        }
    }
}
