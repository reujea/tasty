//! `FileFormatRegistry` — 등록된 detector 들을 관리하고 file 을 identify 한다.
//!
//! 출처별 contribution 을 따로 보관해 plugin uninstall 시 원본 복원 가능.
//! finalize 는 incremental — install/uninstall 호출 후 dirty 표시 + identify 시 1회.

mod cleanup;
mod helpers;
mod install;
mod io;
mod path_glob;
mod priority;
mod query;
mod snapshot;
#[cfg(test)]
mod tests;
mod user_edit;

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;

use helpers::rule_kind_eq;
use path_glob::PathGlobCache;
pub use snapshot::{ContributionSnapshot, DetectorSnapshot};

use super::config::DetectorDecl;
use super::info::DetectorInfo;
use super::types::{DetectorId, DetectorRule, DetectorRuleKind, FileFormatDetector, RuleOrigin};

/// 한 출처가 단일 detector 에 기여한 내용.
#[derive(Debug, Clone)]
pub(super) struct DetectorContribution {
    pub(super) origin: RuleOrigin,
    pub(super) display_name_i18n_key: Option<String>,
    pub(super) icon: Option<String>,
    /// `disabled` 는 명시된 출처만 적용 (patch). `None` 이면 무시.
    pub(super) disabled_override: Option<bool>,
    pub(super) rules: Vec<DetectorRuleKind>,
}

/// 확장자별 우선순위 항목 (출처 메타 포함). user export 시 origin 으로 필터.
#[derive(Debug, Clone)]
pub(super) struct ExtensionPriorityEntry {
    pub(super) order: Vec<DetectorId>,
    pub(super) origin: RuleOrigin,
}

pub(super) struct Inner {
    /// detector id → 출처별 contribution. install 순서대로 push 한다 — 병합 순서는 이
    /// 순서가 아니라 [`merge_order`] 가 정한다.
    pub(super) contributions: BTreeMap<DetectorId, Vec<DetectorContribution>>,
    /// detector id → 최초 install 시점의 monotonic counter 값. 후속 patch 에 의해 변하지
    /// 않는다 (`install_one` 이 entry 가 비었을 때만 부여).
    pub(super) install_order: BTreeMap<DetectorId, u64>,
    /// 확장자별 우선순위 표 (Phase E). 같은 확장자에 둘 이상의 출처가 적으면
    /// last-writer-wins (install 순서 host → user). user export 시에는 user origin 만 emit.
    pub(super) extension_priority: BTreeMap<String, ExtensionPriorityEntry>,
    /// finalize 결과 cache. dirty 시 lazy 재계산.
    pub(super) finalized: BTreeMap<DetectorId, FileFormatDetector>,
    /// 전체 PathGlob 패턴을 하나로 묶어 컴파일한 매처. `finalized` 와 함께
    /// dirty 시에만 재구성 — `identify` 는 파일마다 재컴파일하지 않고 재사용한다.
    pub(super) path_globs: PathGlobCache,
    pub(super) dirty: bool,
}

pub struct FileFormatRegistry {
    pub(super) inner: RwLock<Inner>,
    /// poison 을 이미 보고했는가 — 로그 폭주 방지용 1 회 게이트.
    pub(super) poison_reported: std::sync::atomic::AtomicBool,
    /// 다음 install 시 부여할 monotonic 카운터. install_one 이 새 detector id 에 대해
    /// fetch_add 로 받아온다.
    pub(super) next_install_order: AtomicU64,
}

impl FileFormatRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                contributions: BTreeMap::new(),
                install_order: BTreeMap::new(),
                extension_priority: BTreeMap::new(),
                finalized: BTreeMap::new(),
                path_globs: PathGlobCache::default(),
                dirty: false,
            }),
            next_install_order: AtomicU64::new(0),
            poison_reported: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Poison 을 복구해 read guard 를 잡는다.
    ///
    /// 이전에는 락 획득 24 곳이 전부 `read().ok()?` / `Err(_) => return` 으로 **조용히**
    /// 빠져나갔다. 그 결과는 "그 확장자를 아무 detector 도 못 알아본다" 인데 관측
    /// 지점이 0 이었다. `Inner` 는 `BTreeMap` 들과 glob 캐시·`bool` 이고 임계구역은
    /// 자료구조 조작만 하므로 패닉이 나도 불변식은 성립한다 — 복구가 맞다
    /// ([`error-handling.md`](../../../docs/dev-guide/error-handling.md) "락 poison").
    pub(super) fn lock_read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        tasty_utils::poison::recover_read(
            self.inner.read(),
            "file format registry",
            &self.poison_reported,
        )
    }

    /// Poison 을 복구해 write guard 를 잡는다. 근거는 [`Self::lock_read`] 와 같다.
    pub(super) fn lock_write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        tasty_utils::poison::recover_write(
            self.inner.write(),
            "file format registry",
            &self.poison_reported,
        )
    }

    pub(super) fn ensure_finalized(&self) {
        let needs_finalize = self.lock_read().dirty;
        if !needs_finalize {
            return;
        }
        let mut inner = self.lock_write();
        if !inner.dirty {
            return;
        }
        let mut next = BTreeMap::new();
        for (id, contribs) in inner.contributions.iter() {
            let mut display = None;
            let mut icon = None;
            let mut disabled = false;
            let mut rules: Vec<DetectorRule> = Vec::new();
            for c in merge_order(contribs) {
                if c.display_name_i18n_key.is_some() {
                    display = c.display_name_i18n_key.clone();
                }
                if c.icon.is_some() {
                    icon = c.icon.clone();
                }
                if let Some(d) = c.disabled_override {
                    disabled = d;
                }
                for kind in &c.rules {
                    let candidate = DetectorRule {
                        kind: kind.clone(),
                        origin: c.origin.clone(),
                    };
                    // dedupe — 동일 kind 두 번 등록되면 처음 origin 만 보존.
                    if !rules.iter().any(|r| rule_kind_eq(&r.kind, &candidate.kind)) {
                        rules.push(candidate);
                    }
                }
            }
            let install_order = inner.install_order.get(id).copied().unwrap_or(u64::MAX);
            next.insert(
                id.clone(),
                FileFormatDetector {
                    id: id.clone(),
                    display_name_i18n_key: display,
                    icon,
                    rules,
                    disabled,
                    install_order,
                },
            );
        }
        inner.path_globs = PathGlobCache::rebuild(next.values().flat_map(|det| {
            det.rules.iter().filter_map(|r| match &r.kind {
                DetectorRuleKind::PathGlob { pattern } => Some(pattern.as_str()),
                _ => None,
            })
        }));
        inner.finalized = next;
        inner.dirty = false;
    }
}

/// 병합 순서 — 출처 순(Host → Plugin → User)으로 안정 정렬한다. 같은 출처 안에서는 설치
/// 순서를 그대로 둔다.
///
/// 병합은 "마지막 non-None 이 이긴다" 라서 순서가 곧 우선순위다. 설치 순서로 병합하면 부팅
/// (user 설정을 plugin 보다 먼저 읽는다)이나 plugin 재기동(끈 plugin 의 contribution 이 빠졌다가
/// 다시 뒤에 붙는다) 뒤에 plugin 의 값이 user patch 를 덮는다. user patch 는 host · plugin 의
/// 값을 덮어쓰는 것이 뜻이므로 늘 마지막에 둔다.
///
/// detector id 에는 출처 이름공간이 없어 host 와 plugin 이 같은 id(`markdown` 등)를 함께
/// contribute 할 수 있다. 그 둘 사이는 Host → Plugin — plugin 은 늘 host 기본값 뒤에 설치돼
/// 왔으므로 지금까지의 결과(plugin 이 host 를 덮는다)를 그대로 둔다. 서로 다른 plugin 끼리는
/// 설치 순서다. 근거는 `docs/adr/0520-file-format-merge-applies-user-patches-last.md`.
fn merge_order(contribs: &[DetectorContribution]) -> Vec<&DetectorContribution> {
    let mut ordered: Vec<&DetectorContribution> = contribs.iter().collect();
    ordered.sort_by_key(|c| match c.origin {
        RuleOrigin::HostDefault => 0u8,
        RuleOrigin::Plugin(_) => 1,
        RuleOrigin::User => 2,
    });
    ordered
}

impl DetectorInfo for FileFormatRegistry {
    fn advertised_extensions(&self, detector: &DetectorId) -> Vec<String> {
        self.ensure_finalized();
        let inner = self.lock_read();
        let Some(det) = inner.finalized.get(detector) else {
            return Vec::new();
        };
        let mut out: Vec<String> = det
            .rules
            .iter()
            .filter_map(|r| match &r.kind {
                DetectorRuleKind::Extension { values } => Some(values.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn detectors_for_extension(&self, ext: &str) -> Vec<DetectorId> {
        self.ensure_finalized();
        let ext_lower = ext.trim_start_matches('.').to_ascii_lowercase();
        if ext_lower.is_empty() {
            return Vec::new();
        }
        let inner = self.lock_read();
        let mut hits: Vec<(u64, DetectorId)> = inner
            .finalized
            .iter()
            .filter(|(_, det)| !det.disabled)
            .filter_map(|(id, det)| {
                let advertises = det.rules.iter().any(|r| {
                    matches!(
                        &r.kind,
                        DetectorRuleKind::Extension { values } if values.iter().any(|v| v == &ext_lower),
                    )
                });
                advertises.then(|| (det.install_order, id.clone()))
            })
            .collect();
        hits.sort_by(|(a_ord, a_id), (b_ord, b_id)| a_ord.cmp(b_ord).then_with(|| a_id.cmp(b_id)));
        hits.into_iter().map(|(_, id)| id).collect()
    }

    fn all_advertised_extensions(&self) -> Vec<String> {
        self.ensure_finalized();
        let inner = self.lock_read();
        let mut out: Vec<String> = inner
            .finalized
            .values()
            .filter(|det| !det.disabled)
            .flat_map(|det| {
                det.rules.iter().filter_map(|r| match &r.kind {
                    DetectorRuleKind::Extension { values } => Some(values.clone()),
                    _ => None,
                })
            })
            .flatten()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn is_enabled(&self, detector: &DetectorId) -> bool {
        self.ensure_finalized();
        let inner = self.lock_read();
        inner.finalized.get(detector).is_some_and(|d| !d.disabled)
    }
}

impl Default for FileFormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl tasty_plugin_protocol::host_port::FileFormatRegistryPort for FileFormatRegistry {
    fn install_plugin_detectors(&self, plugin_id: &str, detectors: &[serde_json::Value]) {
        let mut decls: Vec<DetectorDecl> = Vec::with_capacity(detectors.len());
        for v in detectors {
            match serde_json::from_value::<DetectorDecl>(v.clone()) {
                Ok(d) => decls.push(d),
                Err(e) => {
                    tracing::warn!("plugin '{plugin_id}' detector decode failed: {e} ({v:?})")
                }
            }
        }
        FileFormatRegistry::install_plugin_detectors(self, plugin_id, &decls);
    }

    fn uninstall_plugin(&self, plugin_id: &str) {
        FileFormatRegistry::uninstall_plugin(self, plugin_id);
    }
}
