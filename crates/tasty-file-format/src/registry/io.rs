//! `FileFormatRegistry` — io 도메인.

use std::path::Path;

use super::FileFormatRegistry;
use super::helpers::rule_kind_to_toml;
use crate::types::RuleOrigin;

impl FileFormatRegistry {
    /// user 출처 contribution 만 모아 TOML 문자열로 직렬화. Settings UI 가 변경 사항을
    /// `~/.tasty/file-formats.toml` 에 저장할 때 사용.
    ///
    /// host default / plugin contribution 은 포함하지 않는다 (그것들은 자기 출처가 다시
    /// install 한다). Round-trip 보장 — `parse_detector_section(&export)` 로 원래 user
    /// contribution 을 그대로 재현 가능. `Unknown` rule 의 raw payload 도 보존.
    ///
    /// 주의: TOML 주석/공백/key 순서는 보존하지 않는다 (재발급). 사용자 손편집 친화적
    /// round-trip 이 필요해지면 `toml_edit` 도입.
    pub fn export_user_config(&self) -> String {
        let inner = self.lock_read();
        let mut doc = toml::value::Table::new();
        let mut detectors = Vec::<toml::Value>::new();
        for (id, contribs) in inner.contributions.iter() {
            let user = contribs
                .iter()
                .find(|c| matches!(c.origin, RuleOrigin::User));
            let Some(user) = user else { continue };
            if user.rules.is_empty()
                && user.display_name_i18n_key.is_none()
                && user.icon.is_none()
                && user.disabled_override.is_none()
            {
                continue;
            }
            let mut det = toml::value::Table::new();
            det.insert(
                "id".to_string(),
                toml::Value::String(id.as_str().to_string()),
            );
            if let Some(k) = &user.display_name_i18n_key {
                det.insert(
                    "display_name_i18n_key".to_string(),
                    toml::Value::String(k.clone()),
                );
            }
            if let Some(icon) = &user.icon {
                det.insert("icon".to_string(), toml::Value::String(icon.clone()));
            }
            if let Some(d) = user.disabled_override {
                det.insert("disabled".to_string(), toml::Value::Boolean(d));
            }
            let rules: Vec<toml::Value> = user
                .rules
                .iter()
                .map(|k| toml::Value::Table(rule_kind_to_toml(k)))
                .collect();
            if !rules.is_empty() {
                det.insert("rule".to_string(), toml::Value::Array(rules));
            }
            detectors.push(toml::Value::Table(det));
        }
        // user origin extension_priority emit.
        let mut priorities = Vec::<toml::Value>::new();
        for (ext, entry) in inner.extension_priority.iter() {
            if !matches!(entry.origin, RuleOrigin::User) {
                continue;
            }
            let mut t = toml::value::Table::new();
            t.insert("extension".into(), toml::Value::String(ext.clone()));
            t.insert(
                "order".into(),
                toml::Value::Array(
                    entry
                        .order
                        .iter()
                        .map(|id| toml::Value::String(id.as_str().to_string()))
                        .collect(),
                ),
            );
            priorities.push(toml::Value::Table(t));
        }
        if detectors.is_empty() && priorities.is_empty() {
            return String::new();
        }
        if !detectors.is_empty() {
            doc.insert("detector".to_string(), toml::Value::Array(detectors));
        }
        if !priorities.is_empty() {
            doc.insert(
                "extension_priority".to_string(),
                toml::Value::Array(priorities),
            );
        }
        toml::to_string(&doc).unwrap_or_default()
    }

    /// `export_user_config` 의 결과를 `path` 에 atomic write. tempfile + rename 으로
    /// 부분 쓰기 방지. 빈 결과 (user contribution 없음) 면 path 가 존재하면 빈 파일로
    /// 덮어쓴다 — 사용자가 모든 항목을 지웠다는 의미.
    pub fn save_user_config(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let text = self.export_user_config();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(text.as_bytes())?;
        tmp.flush()?;
        tmp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}
