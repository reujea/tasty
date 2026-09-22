//! `FileFormatRegistry` — install 도메인.

use std::path::Path;

use tracing::warn;

use super::FileFormatRegistry;
use super::helpers::{
    install_extension_priority, install_one, parse_detector_section,
    parse_extension_priority_section,
};
use crate::config::{DetectorDecl, DetectorRuleDecl};
use crate::types::RuleOrigin;

impl FileFormatRegistry {
    pub fn install_host_defaults(&self, toml_text: &str) {
        let decls = match parse_detector_section(toml_text) {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "file_format: failed to parse host defaults");
                return;
            }
        };
        let priorities = parse_extension_priority_section(toml_text).unwrap_or_default();
        let mut inner = self.lock_write();
        for decl in decls {
            install_one(
                &mut inner,
                &self.next_install_order,
                decl,
                RuleOrigin::HostDefault,
                false,
            );
        }
        for p in priorities {
            install_extension_priority(&mut inner, p, RuleOrigin::HostDefault);
        }
        inner.dirty = true;
    }

    pub fn install_user_config(&self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "file_format: user config read failed");
                return;
            }
        };
        let decls = match parse_detector_section(&text) {
            Ok(d) => d,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "file_format: user config parse failed");
                return;
            }
        };
        let priorities = parse_extension_priority_section(&text).unwrap_or_default();
        let mut inner = self.lock_write();
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

    pub fn install_plugin_detectors(&self, plugin_id: &str, decls: &[DetectorDecl]) {
        let mut inner = self.lock_write();
        for decl in decls {
            // Plugin 출처의 Lua detector rule 은 차단: host/user 만 사용자 권한으로
            // 실행되는 신뢰 영역. plugin 의 임의 Lua 는 sandbox 가 있어도 정책상 금지.
            let mut decl = decl.clone();
            let before = decl.rule.len();
            decl.rule
                .retain(|r| !matches!(r, DetectorRuleDecl::Lua { .. }));
            let dropped = before - decl.rule.len();
            if dropped > 0 {
                warn!(
                    plugin = plugin_id,
                    detector = %decl.id,
                    dropped,
                    "file_format: dropping Lua detector rules from plugin (host/user only)",
                );
            }
            if decl.rule.is_empty() {
                // 모든 rule 이 Lua 였다면 detector 자체 install 의미 없음.
                continue;
            }
            install_one(
                &mut inner,
                &self.next_install_order,
                decl,
                RuleOrigin::Plugin(plugin_id.to_string()),
                true,
            );
        }
        inner.dirty = true;
    }
}
