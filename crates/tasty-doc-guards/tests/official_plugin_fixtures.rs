//! 공식 플러그인 호스트 시험 픽스처(`crates/tasty-test-support/fixtures/official-plugins/<id>/`)가
//! 잠금 파일(`official-plugins.lock.toml`)과 맞는지 네트워크 없이 본다.
//!
//! 1. 픽스처 폴더 집합이 잠금 파일 id 집합과 같다.
//! 2. 픽스처 매니페스트의 `id`·`version`·`bundle`(없으면 true)이 잠금 항목과 같다.
//! 3. 픽스처마다 `lang/en.toml`이 있다.
//! 4. 픽스처 파일에 CR 바이트가 없다. Git이 줄끝을 바꾸면 산출물과 바이트가 달라진다.
//!
//! 한계: 같은 version의 내용 변이(잠금 해시만 바뀐 재발행)는 보지 못한다. 그 대조는 CI의
//! `scripts/sync-official-plugin-fixtures.sh --check`가 받은 산출물과 바이트로 한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tasty_doc_guards::official_plugins::{LOCK_FILE, Lock, manifest_top_level_str, parse_lock};
use tasty_doc_guards::{OFFICIAL_PLUGIN_FIXTURES_DIR, repo_root};

fn lock() -> Lock {
    let path = repo_root().join(LOCK_FILE);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", path.display()));
    parse_lock(&text)
        .unwrap_or_else(|errors| panic!("{LOCK_FILE}를 읽지 못했다:\n  {}", errors.join("\n  ")))
}

fn fixtures_root() -> PathBuf {
    repo_root().join(OFFICIAL_PLUGIN_FIXTURES_DIR)
}

fn fixture_ids(root: &Path) -> BTreeSet<String> {
    let entries = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("픽스처 폴더를 읽지 못했다: {} — {e}", root.display()));
    entries
        .map(|e| e.unwrap_or_else(|e| panic!("픽스처 폴더 항목을 읽지 못했다: {e}")))
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// 매니페스트 최상위(첫 표 머리 앞)의 `bundle` 값. 키가 없으면 호스트 기본값 true다.
fn top_level_bundle(text: &str) -> Result<bool, String> {
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            break;
        }
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        if k.trim() != "bundle" {
            continue;
        }
        let v = v.split('#').next().unwrap_or("").trim();
        return match v {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("bundle 값을 읽지 못했다: {other}")),
        };
    }
    Ok(true)
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("폴더를 읽지 못했다: {} — {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("폴더 항목을 읽지 못했다: {} — {e}", dir.display()))
            .path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn fixtures_are_the_locked_plugins() {
    let lock = lock();
    let want: BTreeSet<String> = lock.plugins.iter().map(|p| p.id.clone()).collect();
    assert_eq!(
        fixture_ids(&fixtures_root()),
        want,
        "{OFFICIAL_PLUGIN_FIXTURES_DIR}의 폴더 집합이 잠금 파일 id 집합과 다르다. \
         잠금 파일을 갱신했으면 scripts/sync-official-plugin-fixtures.sh --update 를 실행한다"
    );
}

#[test]
fn fixture_manifests_match_their_lock_entries() {
    let lock = lock();
    let root = fixtures_root();
    let mut problems = BTreeMap::new();
    for plugin in &lock.plugins {
        let path = root.join(&plugin.id).join("tasty-plugin.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            problems.insert(
                plugin.id.clone(),
                "tasty-plugin.toml 을 읽지 못했다".to_string(),
            );
            continue;
        };
        let id = manifest_top_level_str(&text, "id");
        let version = manifest_top_level_str(&text, "version");
        let bundle = top_level_bundle(&text);
        let mut wrong = Vec::new();
        if id.as_deref() != Some(plugin.id.as_str()) {
            wrong.push(format!("id {id:?}"));
        }
        if version.as_deref() != Some(plugin.version.as_str()) {
            wrong.push(format!("version {version:?} (잠금 {})", plugin.version));
        }
        match bundle {
            Ok(b) if b == plugin.bundle => {}
            Ok(b) => wrong.push(format!("bundle {b} (잠금 {})", plugin.bundle)),
            Err(why) => wrong.push(why),
        }
        if !root.join(&plugin.id).join("lang/en.toml").is_file() {
            wrong.push("lang/en.toml 없음".to_string());
        }
        if !wrong.is_empty() {
            problems.insert(plugin.id.clone(), wrong.join(", "));
        }
    }
    assert!(
        problems.is_empty(),
        "픽스처가 잠금 파일과 다르다. scripts/sync-official-plugin-fixtures.sh --update 로 다시 쓴다:\n  {}",
        problems
            .iter()
            .map(|(id, why)| format!("{id}: {why}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn fixtures_carry_no_carriage_returns() {
    let mut files = Vec::new();
    files_under(&fixtures_root(), &mut files);
    assert!(
        !files.is_empty(),
        "{OFFICIAL_PLUGIN_FIXTURES_DIR}에서 파일을 하나도 읽지 못했다"
    );
    let with_cr: Vec<String> = files
        .iter()
        .filter(|p| {
            std::fs::read(p)
                .unwrap_or_else(|e| panic!("{} 를 읽지 못했다 — {e}", p.display()))
                .contains(&b'\r')
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        with_cr.is_empty(),
        "픽스처에 CR 바이트가 있다. 산출물 원문과 바이트가 달라진다:\n  {}",
        with_cr.join("\n  ")
    );
}

#[test]
fn a_missing_bundle_key_means_bundled() {
    assert_eq!(
        top_level_bundle("id = \"x\"\n[entry]\nbundle = false\n"),
        Ok(true)
    );
    assert_eq!(
        top_level_bundle("bundle = false # 개발 경로 전용\n"),
        Ok(false)
    );
    assert!(top_level_bundle("bundle = \"no\"\n").is_err());
}
