//! 데이터 루트의 plugins 디렉터리에서 매니페스트를 읽는다.
//! release는 키로 검증했거나 사용자가 미인증 경고를 보고 고른 plugin만 등록한다.
//! debug는 서명·신뢰 검사를 우회하지만 매니페스트 파싱은 수행한다.
//! 공식 목록에 없는 `com.tasty.*` id는 서명 판정 전에 모든 빌드에서 거부한다([`crate::official`]).

use std::path::PathBuf;

use tasty_plugin_manifest::{Manifest, PluginPackage};

pub fn plugin_root() -> Option<PathBuf> {
    tasty_utils::path::tasty_home().map(|d| d.join("plugins"))
}

/// plugin을 찾아 ID순으로 정렬하고 같은 ID는 첫 항목만 남긴다.
pub fn discover() -> Vec<PluginPackage> {
    discover_all().trusted
}

/// 한 번의 탐색 결과.
#[derive(Debug, Default)]
pub struct Discovered {
    /// trust gate를 통과해 등록할 plugin. ID순이며 같은 ID는 첫 항목만 남는다.
    /// 설치 목록(`PluginManager::packages`)은 lifecycle이 이 값으로 갱신한다.
    pub trusted: Vec<PluginPackage>,
    /// trust gate 에서 거부된 plugin(서명 미신뢰 / 검증 실패 / 권한 변경). UI 의 "확인 필요"
    /// 목록과 사이드바 경고 배지가 소비한다. debug 빌드는 trust gate 를 우회하므로
    /// (`trust_outcome` 가 항상 Trusted) 항상 비어 있다.
    pub rejected: Vec<RejectedPlugin>,
    /// 공식 목록에 없는 `com.tasty.*` 폴더. 모든 빌드에서 등록하지 않는다.
    pub reserved: Vec<ReservedIdBlocked>,
}

/// 공식 목록에 없는 `com.tasty.*` id를 선언해 등록하지 않은 폴더.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedIdBlocked {
    pub id: String,
    pub dir: PathBuf,
}

/// `discover` 와 같은 탐색에서 거부한 항목도 함께 돌려준다.
pub fn discover_all() -> Discovered {
    let mut found = Discovered::default();
    let Some(root) = plugin_root() else {
        return found;
    };
    if !root.exists() {
        return found;
    }
    let read_dir = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!("plugin discovery: cannot read {}: {}", root.display(), e);
            return found;
        }
    };
    for entry in read_dir.flatten() {
        classify_discovery_entry(entry.path(), &mut found);
    }
    found
        .trusted
        .sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    // (정렬 + dedup 은 trust gate 와 무관 — 위에서 이미 filter 완료.)
    dedup_packages_by_id(&mut found.trusted);
    found.reserved.sort_by(|a, b| a.dir.cmp(&b.dir));
    found
}

/// 한 디렉토리 항목을 검사해 id 이름 공간과 trust gate 판정에 따라 분류한다.
/// plugin 디렉토리가 아니거나 매니페스트가 없으면 조용히 skip.
fn classify_discovery_entry(dir: PathBuf, found: &mut Discovered) {
    if !dir.is_dir() || !dir.join("tasty-plugin.toml").exists() {
        return;
    }
    // 보일 내용을 읽기 전에 digest 를 잡는다. 확인 필요 탭의 선택은 이 내용에만 적용된다.
    let manifest_sha256 = crate::trust_verdict::manifest_sha256(&dir).unwrap_or_default();
    // 여기서는 매니페스트 스키마를 확인한다. detector/handler 등록 검증은 호스트가 처리한다.
    let manifest = match Manifest::load(&dir) {
        Ok(manifest) => manifest,
        Err(e) => {
            tracing::warn!("plugin '{}' rejected: {}", dir.display(), e);
            return;
        }
    };
    // 서명 판정보다 먼저 본다. debug의 서명 우회는 이 판정에 적용하지 않는다.
    if let Err(blocked) = admit_reserved(&dir, &manifest) {
        found.reserved.push(blocked);
        return;
    }
    match trust_outcome(&dir, &manifest, manifest_sha256) {
        TrustOutcome::Trusted => found.trusted.push(PluginPackage { dir, manifest }),
        TrustOutcome::Rejected(rej) => {
            tracing::warn!("plugin '{}' not auto-loaded ({:?})", rej.id, rej.reason);
            found.rejected.push(*rej);
        }
    }
}

/// 공식 목록에 없는 `com.tasty.*` id면 경고를 남기고 거부 기록을 돌려준다.
fn admit_reserved(dir: &std::path::Path, manifest: &Manifest) -> Result<(), ReservedIdBlocked> {
    crate::official::admit_plugin_id(&manifest.id).map_err(|e| {
        tracing::warn!("plugin at {} not loaded: {e}", dir.display());
        ReservedIdBlocked {
            id: e.id.clone(),
            dir: dir.to_path_buf(),
        }
    })
}

/// 정렬된 packages 에서 중복 id 를 제거 — 먼저 나온(정렬 후 첫) 항목만 보존.
fn dedup_packages_by_id(packages: &mut Vec<PluginPackage>) {
    let mut seen = std::collections::HashSet::new();
    packages.retain(|pkg| {
        if seen.contains(&pkg.manifest.id) {
            tracing::warn!(
                "duplicate plugin id '{}' at {} — keeping first",
                pkg.manifest.id,
                pkg.dir.display()
            );
            false
        } else {
            seen.insert(pkg.manifest.id.clone());
            true
        }
    });
}

/// trust gate 에서 거부된 plugin 한 건. UI "확인 필요" 탭 + 사이드바 경고 배지
/// 가 소비한다. 매니페스트는 파싱에 성공했으나 서명/신뢰 검증에서 떨어진
/// 경우만 기록한다 (매니페스트 파싱 실패는 식별 정보가 없어 제외).
#[derive(Debug, Clone)]
pub struct RejectedPlugin {
    pub id: String,
    /// 플러그인 폴더. 폴더 이름이 ID와 다를 수 있어 확인 필요 탭의 등록이 이 경로를 쓴다.
    pub dir: PathBuf,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    /// 매니페스트의 설명. 서명 검증 전 값이라 표시할지는 UI 가 사유로 정한다.
    pub description: String,
    /// 매니페스트의 homepage. 설명과 같이 서명 검증 전 값이다.
    pub homepage: String,
    /// builtin(번들) plugin 이면 true — UI 가 "built-in" 태그를 붙인다.
    pub builtin: bool,
    pub reason: RejectionReason,
    /// 서명 키 지문 (UnknownKey/PermissionsChanged 일 때). 표시용.
    pub fingerprint: Option<String>,
    /// PermissionsChanged 일 때 신뢰 시점 대비 새로 요구된 권한.
    pub permissions_added: Vec<String>,
    /// PermissionsChanged 일 때 더 이상 쓰지 않는 권한.
    pub permissions_removed: Vec<String>,
    /// SignatureInvalid 일 때 서명 검증이 실패한 원인(`UnverifiedCause::describe`).
    pub cause: Option<String>,
    /// 서명 사유일 때 미인증 원인 이름(`UnverifiedCause::token`).
    pub unverified: Option<&'static str>,
    /// 탐색 때 읽은 매니페스트의 SHA-256. 확인 필요 탭의 "그래도 등록"은 이 내용일 때만 등록한다.
    pub manifest_sha256: String,
}

/// plugin 이 자동 로드에서 거부된 사유. `bundle_sig::TrustDecision` 을 UI 친화적
/// 으로 단순화한 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    /// 신뢰 목록에 없는 키로 서명됨. 사용자가 고르기 전까지 등록하지 않는다.
    UnknownKey,
    /// 서명 누락/손상/검증 실패. 사용자가 고르기 전까지 등록하지 않는다.
    SignatureInvalid,
    /// 한 번 신뢰했으나 매니페스트 권한이 바뀜 → 재승인 필요.
    PermissionsChanged,
}

// 이유: debug는 서명·신뢰 검사를 우회하므로 Rejected를 만들지 않는다.
#[cfg_attr(debug_assertions, allow(dead_code))]
enum TrustOutcome {
    Trusted,
    // 거부 기록은 매니페스트 글을 실어 크므로 상자에 담아 Trusted 와 크기를 맞춘다.
    Rejected(Box<RejectedPlugin>),
}

/// [`crate::trust_verdict::judge`] 로 이 디렉토리 plugin 을 자동 로드해도 되는지 정한다.
/// debug 빌드는 항상 Trusted다. 작업 공간의 번들 plugin 이 서명 없이 빌드되기 때문이다.
/// 설치·등록 경로는 debug 에서도 같은 판정을 쓴다.
///
/// release 빌드:
/// - `Trusted`, 사용자가 고른 `AcceptedUnverified` → 통과
/// - `PermissionsChanged` → 재승인 사유로 Rejected
/// - 모르는 키(공개 키 파일 유무 무관) → UnknownKey Rejected
/// - 서명 없음·검증 오류·공개 키 불일치 → SignatureInvalid Rejected
fn trust_outcome(
    dir: &std::path::Path,
    manifest: &Manifest,
    manifest_sha256: String,
) -> TrustOutcome {
    #[cfg(debug_assertions)]
    {
        let _ = (dir, manifest, manifest_sha256); // debug 빌드 sig-skip — 인자는 release 분기 전용.
        TrustOutcome::Trusted
    }
    #[cfg(not(debug_assertions))]
    {
        use crate::trust_verdict::{TrustVerdict, UnverifiedCause, judge};
        let mk = |reason,
                  fingerprint,
                  added: Vec<String>,
                  removed: Vec<String>,
                  cause,
                  unverified: Option<&'static str>| {
            TrustOutcome::Rejected(Box::new(RejectedPlugin {
                id: manifest.id.clone(),
                dir: dir.to_path_buf(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                authors: manifest.authors.clone(),
                description: manifest.description.clone(),
                homepage: manifest.homepage.clone(),
                builtin: crate::builtin::is_builtin_plugin(&manifest.id),
                reason,
                fingerprint,
                permissions_added: added,
                permissions_removed: removed,
                cause,
                unverified,
                manifest_sha256: manifest_sha256.clone(),
            }))
        };
        match judge(dir) {
            TrustVerdict::Trusted | TrustVerdict::AcceptedUnverified => TrustOutcome::Trusted,
            TrustVerdict::PermissionsChanged {
                fingerprint,
                manifest_permissions,
            } => {
                let (added, removed) = perm_diff(&manifest.id, &manifest_permissions);
                mk(
                    RejectionReason::PermissionsChanged,
                    Some(fingerprint),
                    added,
                    removed,
                    None,
                    None,
                )
            }
            TrustVerdict::Unverified(
                ref cause @ (UnverifiedCause::UnknownKey {
                    ref fingerprint, ..
                }
                | UnverifiedCause::MissingPubkey { ref fingerprint }),
            ) => mk(
                RejectionReason::UnknownKey,
                Some(fingerprint.clone()),
                vec![],
                vec![],
                None,
                Some(cause.token()),
            ),
            TrustVerdict::Unverified(cause) => {
                tracing::warn!(
                    "plugin '{}' signature check failed: {}",
                    manifest.id,
                    cause.describe()
                );
                mk(
                    RejectionReason::SignatureInvalid,
                    None,
                    vec![],
                    vec![],
                    Some(cause.describe()),
                    Some(cause.token()),
                )
            }
        }
    }
}

#[cfg(test)]
#[path = "discovery_reserved_tests.rs"]
mod reserved_tests;

/// 신뢰 시점(known-plugins.toml)의 권한 대비 새 매니페스트 권한의 added/removed.
#[cfg(not(debug_assertions))]
fn perm_diff(plugin_id: &str, new_perms: &[String]) -> (Vec<String>, Vec<String>) {
    let old: Vec<String> = crate::known_plugins::KnownPlugins::load()
        .ok()
        .and_then(|k| k.lookup(plugin_id).map(|e| e.permissions.clone()))
        .unwrap_or_default();
    let added = new_perms
        .iter()
        .filter(|p| !old.contains(p))
        .cloned()
        .collect();
    let removed = old
        .iter()
        .filter(|p| !new_perms.contains(p))
        .cloned()
        .collect();
    (added, removed)
}
