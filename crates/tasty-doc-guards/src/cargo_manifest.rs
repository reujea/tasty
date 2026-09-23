//! Cargo 매니페스트를 **원문으로** 읽는다. 의존 절이 무엇을 선언하는지 묻는 가드가
//! 여럿이라, 그 물음의 판사를 하나로 둔다.
//!
//! 파싱이 아니라 **원문 스캔**이다 — 이 크레이트는 의존이 0 이고
//! ([ADR-0138](../../../docs/adr/0138-doc-guards-live-in-a-dependency-free-crate.md)) toml 파서를
//! 들이지 않는다. 그 대가로 형태를 손으로 다뤄야 하므로, 네 형태를 모두 다루는지는
//! 소비자 쪽 픽스처가 고정한다.

/// 매니페스트가 선언하는 의존들 — `(절 이름, 크레이트 이름)`.
///
/// 절 이름은 **잎**이다: `[target.'cfg(unix)'.dev-dependencies]` 는 `dev-dependencies`.
/// 네 형태를 다룬다 — 평서(`serde = "1"`) · 인라인 테이블(`serde = { … }`) ·
/// `[dependencies.serde]` · target 조건부.
pub fn declared_deps(manifest: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut section: Option<String> = None;
    for line in manifest.lines() {
        let t = line.trim();
        if let Some(header) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let leaf = header.rsplit('.').next().unwrap_or(header);
            if is_dep_section(leaf) {
                section = Some(leaf.to_string());
                continue;
            }
            // `[dependencies.foo]` · `[target.'cfg(unix)'.build-dependencies.foo]` 는
            // 헤더 자신이 이름을 담는다.
            let mut parts = header.rsplitn(2, '.');
            let last = parts.next().unwrap_or_default();
            let head = parts.next().unwrap_or_default();
            let head_leaf = head.rsplit('.').next().unwrap_or(head);
            if is_dep_section(head_leaf) && !last.is_empty() {
                out.push((head_leaf.to_string(), last.to_string()));
            }
            section = None;
            continue;
        }
        let Some(sec) = section.as_deref() else {
            continue;
        };
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = t.split_once('=') {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() {
                out.push((sec.to_string(), name.to_string()));
            }
        }
    }
    out
}

/// 그 이름을 선언한 절들. 없으면 빈 벡터다.
pub fn sections_declaring(manifest: &str, name: &str) -> Vec<String> {
    declared_deps(manifest)
        .into_iter()
        .filter(|(_, n)| n == name)
        .map(|(s, _)| s)
        .collect()
}

fn is_dep_section(leaf: &str) -> bool {
    matches!(
        leaf,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    )
}

/// `[features]` 절의 한 feature 가 `dep:` 로 켜는 optional 의존 이름들 — 선언 순서대로.
///
/// `tasty-platform/gui` 처럼 **다른 크레이트의 feature** 를 켜는 항목은 안 담는다. 그 크레이트는
/// feature 없이도 그래프에 있으므로 "이 feature 가 있어야만 링크되는 크레이트" 가 아니다.
/// feature 가 없거나 `dep:` 항목이 없으면 빈 벡터다.
pub fn feature_enabled_deps(manifest: &str, feature: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_features = false;
    let mut in_array = false;
    for line in manifest.lines() {
        let t = line.split('#').next().unwrap_or("").trim();
        if !in_array {
            if let Some(header) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                in_features = header.trim() == "features";
                continue;
            }
            if !in_features {
                continue;
            }
            let Some((name, rest)) = t.split_once('=') else {
                continue;
            };
            if name.trim().trim_matches('"') != feature {
                continue;
            }
            in_array = true;
            collect_dep_entries(rest, &mut out);
            if rest.contains(']') {
                return out;
            }
            continue;
        }
        collect_dep_entries(t, &mut out);
        if t.contains(']') {
            return out;
        }
    }
    out
}

fn collect_dep_entries(text: &str, out: &mut Vec<String>) {
    for piece in text.split('"').skip(1).step_by(2) {
        if let Some(dep) = piece.strip_prefix("dep:") {
            out.push(dep.to_string());
        }
    }
}
