//! 공식 플러그인 잠금 파일(`official-plugins.lock.toml`)의 가드.
//!
//! 1. 완비: 형식을 읽을 수 있고, 모든 플러그인(`bundle = false` 포함)에 5플랫폼 산출물이 있으며
//!    sha256·파일 이름·URL 형태가 맞다.
//! 2. 계약 호환: 잠금 파일이 가리키는 릴리스가 빌드에 쓴 계약 값이 현재 tasty와 같다.
//!    (a) host_api_version, (b) protocol 크레이트 버전, (c) manifest_version,
//!    (d) Cargo.lock의 epaint 버전, (e) `HEAD:vendor/epaint` 트리 id.
//! 3. 집합: 호스트 `BUILTINS`의 id 집합(두 cfg 판 모두)과 `OFFICIAL_PLUGIN_IDS`가 잠금 파일 전체 id에
//!    tasty에 남는 mesh-demo를 더한 집합과 같다. 지금 배포 스크립트가 고르는 집합(크레이트 매니페스트
//!    중 `bundle = false`가 아닌 것)이 잠금 파일의 `bundle = true` 집합과 같다.
//!
//! 한계: 계약 대조는 버전 값만 본다. 버전을 올리지 않은 protocol schema 변경은 잡지 못한다.
//! tasty 릴리스 사이의 develop에서는 protocol 변경이 `CHANGELOG.md`의 `[Unreleased]`에만 쌓이고
//! 버전은 그대로이므로 이 가드는 그 변경을 원리적으로 보지 못한다. 실제로 잠금 릴리스의 tasty_rev
//! 뒤 편집 결과 회신(Hello `capabilities`, `EditOutcome`)이 버전 0.2.16 그대로 들어왔다. 추가 변형이라
//! 옛 플러그인과 호환되지만, 호환이 아닌 변경도 같은 방식으로 지나갈 수 있다. 트리 id 비교(엄격안)를
//! 쓰지 않은 이유와 대신 지키는 규칙은 docs/dev-guide/plugin-packaging.md의 "계약 호환 가드" 절이다.
//!
//! 배포 집합의 원천은 패키징이 잠금 파일을 읽게 바뀌고 플러그인 크레이트가 저장소를 떠나면 바꿔야
//! 한다. 그때 크레이트 매니페스트가 사라지면 이 가드는 빈 집합으로 실패한다.

use std::collections::BTreeSet;
use std::path::Path;

use tasty_doc_guards::official_plugins::{
    LOCK_FILE, Lock, REFRESH_HINT, check, contract_mismatches, parse_lock, read_host_contract,
};
use tasty_doc_guards::repo_root;

/// 공식 플러그인 레포로 옮기지 않고 tasty에 남는 시범 플러그인.
const STAYS_IN_TASTY: &str = "com.tasty.mesh-demo";

fn lock() -> Lock {
    let path = repo_root().join(LOCK_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", path.display()));
    parse_lock(&text)
        .unwrap_or_else(|errors| panic!("{LOCK_FILE}를 읽지 못했다:\n  {}", errors.join("\n  ")))
}

fn locked_ids(lock: &Lock) -> BTreeSet<String> {
    lock.plugins.iter().map(|p| p.id.clone()).collect()
}

/// `const <name>` 정의마다 그 블록(다음 `];`까지)의 `"com.tasty.…"` 문자열 집합을 돌려준다.
/// 줄 주석은 지운다.
fn id_sets_in_const(src: &str, name: &str) -> Vec<BTreeSet<String>> {
    let needle = format!("const {name}:");
    let mut sets = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(&needle) {
        let block = &rest[at..];
        let end = block
            .find("\n];")
            .unwrap_or_else(|| panic!("{name} 블록의 끝을 못 찾았다"));
        let mut ids = BTreeSet::new();
        for line in block[..end].lines() {
            let code = line.split("//").next().unwrap_or("");
            let mut parts = code.split('"');
            parts.next();
            while let Some(lit) = parts.next() {
                if lit.starts_with("com.tasty.") {
                    ids.insert(lit.to_string());
                }
                parts.next();
            }
        }
        sets.push(ids);
        rest = &block[end..];
    }
    sets
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel}를 읽지 못했다: {e}"))
}

/// 배포 스크립트(`scripts/build-linux.sh` 등)와 같은 판정: 매니페스트가 있고 `bundle = false`가 아닌
/// 플러그인 크레이트. id는 매니페스트 최상위 `id`다.
fn shipped_ids(root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let crates = root.join("crates");
    let entries = std::fs::read_dir(&crates)
        .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", crates.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("crates 항목을 읽지 못했다: {e}"));
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("tasty-plugin-") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path().join("tasty-plugin.toml")) else {
            continue;
        };
        let unbundled = text.lines().any(|l| {
            let t: String = l.chars().filter(|c| !c.is_whitespace()).collect();
            t == "bundle=false" || t.starts_with("bundle=false#")
        });
        if unbundled {
            continue;
        }
        let id = tasty_doc_guards::official_plugins::manifest_top_level_str(&text, "id")
            .unwrap_or_else(|| panic!("{name}/tasty-plugin.toml에서 id를 읽지 못했다"));
        ids.insert(id);
    }
    ids
}

#[test]
fn the_lock_file_is_complete() {
    let lock = lock();
    let problems = check(&lock);
    assert!(
        problems.is_empty(),
        "{LOCK_FILE}가 완비되지 않았다. 손으로 고치지 말고 official-plugins generate로 다시 만든다:\n  {}",
        problems.join("\n  ")
    );
    assert!(!lock.plugins.is_empty(), "{LOCK_FILE}에 플러그인이 없다");
}

#[test]
fn the_lock_matches_the_host_contract() {
    let lock = lock();
    let host = read_host_contract(&repo_root()).unwrap_or_else(|e| panic!("{e}"));
    let mismatches = contract_mismatches(&lock.release, &host);
    assert!(
        mismatches.is_empty(),
        "잠금 파일의 릴리스 {} {}(tasty_rev {})는 현재 tasty와 계약이 다르다:\n  {}\n{REFRESH_HINT}",
        lock.release.repository,
        lock.release.tag,
        lock.release.tasty_rev,
        mismatches.join("\n  ")
    );
}

#[test]
fn builtins_are_the_locked_plugins_plus_mesh_demo() {
    let lock = lock();
    let mut want = locked_ids(&lock);
    want.insert(STAYS_IN_TASTY.to_string());

    let builtin = read("crates/tasty-host-plugin/src/builtin.rs");
    let sets = id_sets_in_const(&builtin, "BUILTINS");
    assert_eq!(
        sets.len(),
        2,
        "BUILTINS 정의가 두 cfg 판(windows, not windows)이 아니다. 판독 전제를 확인한다."
    );
    for (i, set) in sets.iter().enumerate() {
        assert_eq!(
            set,
            &want,
            "BUILTINS {}번째 정의의 id 집합이 잠금 파일 id + {STAYS_IN_TASTY}와 다르다",
            i + 1
        );
    }

    let official = read("crates/tasty-host-plugin/src/official.rs");
    let sets = id_sets_in_const(&official, "OFFICIAL_PLUGIN_IDS");
    assert_eq!(sets.len(), 1, "OFFICIAL_PLUGIN_IDS 정의가 하나가 아니다");
    assert_eq!(
        sets[0], want,
        "OFFICIAL_PLUGIN_IDS가 잠금 파일 id + {STAYS_IN_TASTY}와 다르다"
    );
}

#[test]
fn the_shipped_set_is_the_bundled_lock_entries() {
    let lock = lock();
    let bundled: BTreeSet<String> = lock
        .plugins
        .iter()
        .filter(|p| p.bundle)
        .map(|p| p.id.clone())
        .collect();
    let shipped = shipped_ids(&repo_root());
    assert_eq!(
        shipped, bundled,
        "배포 스크립트가 고르는 플러그인(crates/tasty-plugin-*/tasty-plugin.toml 중 bundle = false가 아닌 것)과 \
         잠금 파일의 bundle = true 집합이 다르다"
    );
}

#[test]
fn the_const_reader_sees_every_literal_and_skips_comments() {
    let src = "const X: &[S] = &[\n    S { id: \"com.tasty.a\" },\n    // S { id: \"com.tasty.gone\" },\n    S { id: \"com.tasty.b\", other: \"x\" },\n];\nconst X: &[S] = &[\n    S { id: \"com.tasty.c\" },\n];\n";
    let sets = id_sets_in_const(src, "X");
    assert_eq!(sets.len(), 2);
    assert_eq!(
        sets[0],
        ["com.tasty.a", "com.tasty.b"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(sets[1].len(), 1);
}
