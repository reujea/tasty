//! 공식 플러그인 호스트 시험 픽스처(`crates/tasty-test-support/fixtures/official-plugins/<id>/`)가
//! 잠금 파일(`official-plugins.lock.toml`)과 맞는지 네트워크 없이 본다.
//!
//! 1. 픽스처 폴더 집합이 잠금 파일 id 집합과 같다.
//! 2. 픽스처 매니페스트의 `id`·`version`·`bundle`(없으면 true)이 잠금 항목과 같다.
//! 3. 픽스처마다 `lang/en.toml`이 있다.
//! 4. 픽스처 파일에 CR 바이트가 없다. Git이 줄끝을 바꾸면 산출물과 바이트가 달라진다.
//!
//! 2번은 `scripts/sync-official-plugin-fixtures.sh --update`가 교체 전에 후보에 적용하는 판독과 같은
//! 스캐너(`scripts/lib/toml-scan.awk`)로 매니페스트를 읽는다. 두 검사가 같은 원문을 다르게 읽지 않게 한다.
//! 스캐너는 awk로 돌므로 2번은 Unix에서만 실행한다. 픽스처 원문은 OS마다 같아 Linux 실행으로 덮인다.
//!
//! 한계: 같은 version의 내용 변이(잠금 해시만 바뀐 재발행)는 보지 못한다. 그 대조는 CI의
//! `scripts/sync-official-plugin-fixtures.sh --check`가 받은 산출물과 바이트로 한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{
    CountedOn, Descend, Floor, Pick, walk_dirs_with_floor, walk_with_floor,
};

use tasty_doc_guards::official_plugins::{LOCK_FILE, Lock, parse_lock};
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

/// 픽스처 루트 바로 아래 폴더 순회의 하한.
const FIXTURE_DIRS_FLOOR: Floor = Floor {
    min: 6,
    measured: 8,
    measured_on: "2026-11-21",
    counted_on: CountedOn::Tree(
        "50c1597c7d — 픽스처 폴더는 잠금 id마다 하나이고, 이 커밋의 잠금 파일 [[plugin]]이 8개다.",
    ),
    why_this_gap: "픽스처 폴더 수는 잠금 파일의 플러그인 수를 따른다. 공식 플러그인 한두 개를 잠금 파일에서 빼는 변경을 허용하고, 그보다 줄면 루트를 잘못 잡았거나 read_dir이 실패한 것으로 본다.",
};

/// 픽스처 파일 순회의 하한.
const FIXTURE_FILES_FLOOR: Floor = Floor {
    min: 30,
    measured: 40,
    measured_on: "2026-11-21",
    counted_on: CountedOn::Tree(
        "50c1597c7d — 이 커밋의 잠금 파일이 가리키는 산출물 8개에서 매니페스트 하나와 lang 4개씩을 받아 40개다.",
    ),
    why_this_gap: "플러그인마다 매니페스트 하나와 내장 언어 4개의 lang 파일이다. 공식 플러그인 두 개(10파일)가 빠지는 변경까지 허용하고, 그보다 적으면 수집 실패로 본다.",
};

fn fixture_ids(root: &Path) -> BTreeSet<String> {
    walk_dirs_with_floor(root, root, &FIXTURE_DIRS_FLOOR, &|_| Pick::TakeAndStop)
        .unwrap_or_else(|why| panic!("{why}"))
        .into_iter()
        .map(|w| w.rel)
        .collect()
}

/// 스캐너가 읽은 매니페스트 최상위(첫 표 머리 앞) 값.
#[derive(Debug, PartialEq, Eq)]
struct TopLevel {
    id: Option<String>,
    version: Option<String>,
    /// 키가 없으면 호스트 기본값 true다.
    bundle: bool,
}

/// `toml-scan.awk` 출력(탭 구분 레코드)에서 최상위 `id`·`version`·`bundle`을 뽑는다. sync 스크립트의
/// 교체 전 검사와 같은 규칙이다: 읽지 못한 키나 불 값이 아닌 `bundle`은 오류다.
fn top_level(scan: &str) -> Result<TopLevel, String> {
    let mut top = TopLevel {
        id: None,
        version: None,
        bundle: true,
    };
    for record in scan.lines() {
        let fields: Vec<&str> = record.split('\t').collect();
        match fields.as_slice() {
            ["H", ..] => break,
            ["E", why] => return Err(format!("스캐너가 읽지 못했다: {why}")),
            ["K", "unknown", ..] => return Err("최상위에 읽을 수 없는 키가 있다".to_string()),
            ["K", _, "bundle", "bool", v] => top.bundle = *v == "true",
            ["K", _, "bundle", ..] => return Err("bundle 값이 불 값이 아니다".to_string()),
            ["K", _, "id", "basic" | "literal", v] => top.id = Some((*v).to_string()),
            ["K", _, "version", "basic" | "literal", v] => top.version = Some((*v).to_string()),
            _ => {}
        }
    }
    Ok(top)
}

/// 매니페스트를 스캐너로 읽은 레코드. 스캐너가 실패하면 그 이유를 돌려준다.
#[cfg(unix)]
fn scan_file(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}를 읽지 못했다: {e}", path.display()))?;
    scan_text(&text)
}

/// 원문을 표준 입력으로 스캐너에 넘긴다.
#[cfg(unix)]
fn scan_text(body: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new("awk")
        .arg("-f")
        .arg(repo_root().join("scripts/lib/toml-scan.awk"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("awk를 실행하지 못했다: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("awk 표준 입력을 열지 못했다")?
        .write_all(body.as_bytes())
        .map_err(|e| format!("awk에 원문을 넘기지 못했다: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("awk를 기다리지 못했다: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(text);
    }
    let why = text
        .lines()
        .find_map(|l| l.strip_prefix("E\t"))
        .unwrap_or("이유 없음");
    Err(format!(
        "스캐너가 종료 {:?}로 끝났다: {why}",
        out.status.code()
    ))
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

#[cfg(unix)]
#[test]
fn fixture_manifests_match_their_lock_entries() {
    let lock = lock();
    let root = fixtures_root();
    let mut problems = BTreeMap::new();
    for plugin in &lock.plugins {
        let path = root.join(&plugin.id).join("tasty-plugin.toml");
        let mut wrong = Vec::new();
        match scan_file(&path).and_then(|scan| top_level(&scan)) {
            Err(why) => wrong.push(format!("tasty-plugin.toml: {why}")),
            Ok(top) => {
                if top.id.as_deref() != Some(plugin.id.as_str()) {
                    wrong.push(format!("id {:?}", top.id));
                }
                if top.version.as_deref() != Some(plugin.version.as_str()) {
                    wrong.push(format!(
                        "version {:?} (잠금 {})",
                        top.version, plugin.version
                    ));
                }
                if top.bundle != plugin.bundle {
                    wrong.push(format!("bundle {} (잠금 {})", top.bundle, plugin.bundle));
                }
            }
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
    let root = fixtures_root();
    let files = walk_with_floor(
        &root,
        &root,
        &FIXTURE_FILES_FLOOR,
        Descend::Everything,
        &|_| true,
    )
    .unwrap_or_else(|why| panic!("{why}"));
    let with_cr: Vec<String> = files
        .iter()
        .filter(|w| {
            std::fs::read(&w.path)
                .unwrap_or_else(|e| panic!("{} 를 읽지 못했다 — {e}", w.path.display()))
                .contains(&b'\r')
        })
        .map(|w| w.rel.clone())
        .collect();
    assert!(
        with_cr.is_empty(),
        "픽스처에 CR 바이트가 있다. 산출물 원문과 바이트가 달라진다:\n  {}",
        with_cr.join("\n  ")
    );
}

#[test]
fn the_top_level_reader_follows_the_scanner_records() {
    assert_eq!(
        top_level(
            "K\tbare\tid\tbasic\tcom.tasty.x\nH\t[entry]\tbare\nK\tbare\tbundle\tbool\tfalse\n"
        ),
        Ok(TopLevel {
            id: Some("com.tasty.x".to_string()),
            version: None,
            bundle: true,
        }),
        "표 머리 뒤 bundle 을 최상위로 읽었거나 빠진 bundle 을 true 로 보지 않았다"
    );
    assert_eq!(
        top_level("K\tquoted\tbundle\tbool\tfalse\n").map(|t| t.bundle),
        Ok(false)
    );
    assert!(top_level("K\tbare\tbundle\tother\t\n").is_err());
    assert!(top_level("K\tunknown\t\tbasic\tx\n").is_err());
    assert!(top_level("E\t3: 표 머리를 읽지 못했다\n").is_err());
}

/// 오프라인 가드와 sync 스크립트가 다르게 읽던 원문 두 가지. 인용 키 `"bundle"`과 여러 줄 문자열 안의
/// 가짜 `bundle = true`는 TOML로 둘 다 `bundle = false`다.
#[cfg(unix)]
#[test]
fn quoted_and_string_hidden_bundle_keys_read_like_toml() {
    for (what, body) in [
        ("인용 키", "id = \"com.tasty.x\"\n\"bundle\" = false\n"),
        (
            "여러 줄 문자열",
            "id = \"com.tasty.x\"\ndescription = \"\"\"\nbundle = true\n\"\"\"\nbundle = false\n",
        ),
    ] {
        let top = scan_text(body).and_then(|scan| top_level(&scan));
        assert_eq!(top.map(|t| t.bundle), Ok(false), "{what}");
    }
}
