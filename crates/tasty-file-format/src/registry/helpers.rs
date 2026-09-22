//! `FileFormatRegistry` 내부 helpers — install_one + extension priority + rule kind
//! 변환 + parser 등.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::warn;

use super::super::config::{
    DetectorDecl, DetectorRuleDecl, ExtensionPriorityDecl, validate_detector_decl,
};
use super::super::types::{DetectorId, DetectorRule, DetectorRuleKind, RuleOrigin};
use super::{DetectorContribution, ExtensionPriorityEntry, Inner};

pub(super) fn install_one(
    inner: &mut Inner,
    counter: &AtomicU64,
    decl: DetectorDecl,
    origin: RuleOrigin,
    from_plugin: bool,
) {
    // schema 검증
    let validation = validate_detector_decl(&decl, from_plugin);
    let warnings = match validation {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "file_format: rejecting detector decl");
            return;
        }
    };
    for w in warnings {
        warn!(warning = %w, "file_format: detector decl warning");
    }

    let id = DetectorId(decl.id.clone());
    inner
        .install_order
        .entry(id.clone())
        .or_insert_with(|| counter.fetch_add(1, Ordering::SeqCst));
    let entry = inner.contributions.entry(id).or_default();
    // 같은 origin 으로 재install (예: 사용자 설정 reload) 인 경우 기존 동일 origin 제거 후 push.
    entry.retain(|c| c.origin != origin);
    let rule_kinds: Vec<DetectorRuleKind> = decl
        .rule
        .into_iter()
        .filter_map(decl_rule_to_kind)
        .collect();
    // patch semantics: host · plugin 의 `disabled = false` 는 "끄지 않는다" 일 뿐 다른 출처가
    // 끈 것을 켜지 않는다(종전 그대로). user 의 명시적 `false` 는 켠다는 뜻이다 — Settings 의
    // 켜기(`set_user_detector_disabled(.., false)`)가 저장 파일에 `disabled = false` 로 남고,
    // 다음 부팅에 그 값이 살아야 한다.
    let disabled_override = match (decl.disabled, &origin) {
        (Some(true), _) => Some(true),
        (Some(false), RuleOrigin::User) => Some(false),
        _ => None,
    };
    entry.push(DetectorContribution {
        origin,
        display_name_i18n_key: decl.display_name_i18n_key,
        icon: decl.icon,
        disabled_override,
        rules: rule_kinds,
    });
}

/// 파일명에서 마지막 확장자를 추출해 소문자로 반환. 점 없음 / 마지막 점 뒤가 빈 문자열
/// 이면 `None`.
pub(super) fn path_extension_lowercase(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    if ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// 확장자 fast path. 광고 confirmed detector 들 중에서:
/// 1. `extension_priority` 표가 있으면 그 순서대로 첫 enabled + IsDirectory 아닌 detector,
/// 2. 표에 없거나 표의 detector 들이 모두 부적격이면 install_order 오름차순으로 첫 detector.
pub(super) fn identify_by_extension_priority(inner: &Inner, ext: &str) -> Option<DetectorId> {
    // 광고 매칭 + enabled + IsDirectory 아님.
    let mut advertised: Vec<(u64, DetectorId)> = inner
        .finalized
        .iter()
        .filter(|(_, det)| !det.disabled)
        .filter(|(_, det)| {
            !det.rules
                .iter()
                .any(|r| matches!(r.kind, DetectorRuleKind::IsDirectory))
        })
        .filter_map(|(id, det)| {
            let advertises = det.rules.iter().any(|r| {
                matches!(
                    &r.kind,
                    DetectorRuleKind::Extension { values } if values.iter().any(|v| v == ext),
                )
            });
            advertises.then(|| (det.install_order, id.clone()))
        })
        .collect();
    if advertised.is_empty() {
        return None;
    }
    advertised
        .sort_by(|(a_ord, a_id), (b_ord, b_id)| a_ord.cmp(b_ord).then_with(|| a_id.cmp(b_id)));

    // extension_priority 표 적용 — 표에 적힌 detector 가 advertised 안에 있으면 그것이 먼저.
    if let Some(entry) = inner.extension_priority.get(ext) {
        for prio_id in &entry.order {
            if advertised.iter().any(|(_, id)| id == prio_id) {
                return Some(prio_id.clone());
            }
        }
    }
    // fallback: install_order 정렬의 첫 번째.
    advertised.into_iter().next().map(|(_, id)| id)
}

/// `[[extension_priority]]` 한 entry 등록. 같은 확장자 키에 last-writer-wins
/// (host → user 순서로 install 되므로 user 가 host 를 덮어쓰는 효과).
pub(super) fn install_extension_priority(
    inner: &mut Inner,
    decl: ExtensionPriorityDecl,
    origin: RuleOrigin,
) {
    let key = decl.extension.trim_start_matches('.').to_ascii_lowercase();
    if key.is_empty() {
        warn!("file_format: extension_priority entry with empty extension — skipped");
        return;
    }
    if decl.order.is_empty() {
        // 빈 order 는 의미가 없음 — 제거 의도로 해석해 entry 삭제.
        inner.extension_priority.remove(&key);
        return;
    }
    let mut seen = std::collections::HashSet::new();
    let order: Vec<DetectorId> = decl
        .order
        .into_iter()
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .map(DetectorId)
        .collect();
    if order.is_empty() {
        return;
    }
    inner
        .extension_priority
        .insert(key, ExtensionPriorityEntry { order, origin });
}

pub(super) fn decl_rule_to_kind(decl: DetectorRuleDecl) -> Option<DetectorRuleKind> {
    Some(match decl {
        DetectorRuleDecl::Extension { values } => DetectorRuleKind::Extension {
            values: values
                .into_iter()
                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        },
        // 저장·비교는 항상 `/` 로 통일 — 등록 시점에 1회 정규화해두면
        // evaluator/registry 쪽은 재정규화 없이 그대로 globset 에 넘길 수 있다.
        DetectorRuleDecl::PathGlob { pattern } => DetectorRuleKind::PathGlob {
            pattern: tasty_utils::path::to_slash(&pattern),
        },
        DetectorRuleDecl::Mime { types } => DetectorRuleKind::Mime { types },
        DetectorRuleDecl::Magic { offset, bytes_hex } => {
            let bytes = hex_to_bytes(&bytes_hex)?;
            DetectorRuleKind::Magic { offset, bytes }
        }
        DetectorRuleDecl::IsDirectory => DetectorRuleKind::IsDirectory,
        DetectorRuleDecl::Lua { script } => DetectorRuleKind::Lua { script },
        DetectorRuleDecl::StructureCheck { spec } => DetectorRuleKind::StructureCheck {
            spec_path: PathBuf::from(spec),
        },
        DetectorRuleDecl::Unknown { kind_name, raw } => {
            DetectorRuleKind::Unknown { kind_name, raw }
        }
    })
}

/// `DetectorRuleKind` 을 TOML table 로 역직렬화. `parse_detector_section` 의 입력 형식과
/// 1:1 round-trip. `Unknown` 의 raw payload 는 그대로 보존.
pub(super) fn rule_kind_to_toml(kind: &DetectorRuleKind) -> toml::value::Table {
    let mut t = toml::value::Table::new();
    match kind {
        DetectorRuleKind::Extension { values } => {
            t.insert("kind".into(), toml::Value::String("extension".into()));
            t.insert(
                "values".into(),
                toml::Value::Array(values.iter().cloned().map(toml::Value::String).collect()),
            );
        }
        DetectorRuleKind::PathGlob { pattern } => {
            t.insert("kind".into(), toml::Value::String("path_glob".into()));
            t.insert("pattern".into(), toml::Value::String(pattern.clone()));
        }
        DetectorRuleKind::Mime { types } => {
            t.insert("kind".into(), toml::Value::String("mime".into()));
            t.insert(
                "types".into(),
                toml::Value::Array(types.iter().cloned().map(toml::Value::String).collect()),
            );
        }
        DetectorRuleKind::Magic { offset, bytes } => {
            t.insert("kind".into(), toml::Value::String("magic".into()));
            t.insert("offset".into(), toml::Value::Integer(*offset as i64));
            let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
            t.insert("bytes_hex".into(), toml::Value::String(hex));
        }
        DetectorRuleKind::IsDirectory => {
            t.insert("kind".into(), toml::Value::String("is_directory".into()));
        }
        DetectorRuleKind::Lua { script } => {
            t.insert("kind".into(), toml::Value::String("lua".into()));
            t.insert("script".into(), toml::Value::String(script.clone()));
        }
        DetectorRuleKind::StructureCheck { spec_path } => {
            t.insert("kind".into(), toml::Value::String("structure_check".into()));
            t.insert(
                "spec".into(),
                toml::Value::String(spec_path.to_string_lossy().into_owned()),
            );
        }
        DetectorRuleKind::Unknown { kind_name, raw } => {
            t.insert("kind".into(), toml::Value::String(kind_name.clone()));
            // raw 는 원래 table 통째였으나 manual parser 에서 모든 키를 보관했으므로
            // 그대로 평면 복사.
            if let toml::Value::Table(raw_t) = raw {
                for (k, v) in raw_t {
                    if k == "kind" {
                        continue;
                    }
                    t.insert(k.clone(), v.clone());
                }
            }
        }
    }
    t
}

pub(super) fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

pub(super) fn rule_kind_eq(a: &DetectorRuleKind, b: &DetectorRuleKind) -> bool {
    // Unknown 의 raw 비교는 toml::Value PartialEq 가 있어 가능.
    a == b
}

/// host default / user config 공통 표면: `[[detector]]` 섹션을 가진 TOML.
pub(super) fn parse_detector_section(
    toml_text: &str,
) -> Result<Vec<DetectorDecl>, toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct Wrap {
        #[serde(default, rename = "detector")]
        detectors: Vec<DetectorDecl>,
    }
    let w: Wrap = toml::from_str(toml_text)?;
    Ok(w.detectors)
}

/// host default / user config: `[[extension_priority]]` 섹션. Phase E.
pub(super) fn parse_extension_priority_section(
    toml_text: &str,
) -> Result<Vec<ExtensionPriorityDecl>, toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct Wrap {
        #[serde(default)]
        extension_priority: Vec<ExtensionPriorityDecl>,
    }
    let w: Wrap = toml::from_str(toml_text)?;
    Ok(w.extension_priority)
}
