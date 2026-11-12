//! 번들 플러그인의 매니페스트 version과 Cargo 버전, 매니페스트 id와 코드의 PLUGIN_ID를 대조한다.
//! 플러그인 매니페스트가 없는 라이브러리 크레이트는 제외한다.
//! 자동 실행 경로는 docs/dev-guide/ci-gates.md에 있다.

use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, walk_with_floor};

/// 매니페스트의 최상위 표와 Cargo의 package 표만 읽어 의존성의 version을 잘못 가져오지 않게 한다.
fn owning_table_lines(contents: &str) -> impl Iterator<Item = &str> {
    let mut table: Option<String> = None;
    contents.lines().filter(move |line| {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            table = Some(trimmed.to_string());
            return false;
        }
        match &table {
            None => true,
            Some(name) => name == "[package]",
        }
    })
}

/// 값을 소유한 표에서 version 필드만 읽는다.
fn extract_version(contents: &str, file: &Path) -> String {
    for line in owning_table_lines(contents) {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(after_eq) = rest.strip_prefix('=') {
                let after_eq = after_eq.trim();
                let value = after_eq
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .to_string();
                assert!(!value.is_empty(), "빈 version 필드: {}", file.display());
                return value;
            }
        }
    }
    panic!("version 필드를 찾지 못함: {}", file.display());
}

/// 버전·ID 검사가 같은 플러그인 목록을 사용한다.
fn bundled_plugin_dirs() -> Vec<(String, PathBuf)> {
    let crates_dir = tasty_doc_guards::repo_root().join("crates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).expect("crates 디렉토리 read_dir 실패") {
        let entry = entry.expect("crates dir entry 읽기 실패");
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !name.starts_with("tasty-plugin-") {
            continue;
        }
        if !dir.join("tasty-plugin.toml").exists() {
            continue;
        }
        out.push((name, dir));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 들여쓰기와 무관하게 매니페스트의 id 필드를 읽는다.
fn extract_id(contents: &str, file: &Path) -> String {
    for line in owning_table_lines(contents) {
        if let Some(rest) = line.trim_start().strip_prefix("id") {
            let rest = rest.trim_start();
            if let Some(after_eq) = rest.strip_prefix('=') {
                let value = after_eq.trim().trim_matches('"').to_string();
                assert!(!value.is_empty(), "빈 id 필드: {}", file.display());
                return value;
            }
        }
    }
    panic!("id 필드를 찾지 못함: {}", file.display());
}

type Source = (String, String);

/// 이름과 두 원문의 짝. `(크레이트 이름, 매니페스트 원문, Cargo.toml 원문)`.
type Manifested = (String, String, String);

fn read_manifested(dirs: &[(String, PathBuf)]) -> Vec<Manifested> {
    dirs.iter()
        .map(|(name, dir)| {
            let manifest = dir.join("tasty-plugin.toml");
            let cargo = dir.join("Cargo.toml");
            (
                name.clone(),
                std::fs::read_to_string(&manifest).expect("매니페스트 read 실패"),
                std::fs::read_to_string(&cargo).expect("Cargo.toml read 실패"),
            )
        })
        .collect()
}

fn version_mismatches(pairs: &[Manifested]) -> Vec<String> {
    let mut out = Vec::new();
    for (name, manifest_src, cargo_src) in pairs {
        let manifest_v = extract_version(manifest_src, Path::new(name));
        let cargo_v = extract_version(cargo_src, Path::new(name));
        if manifest_v != cargo_v {
            out.push(format!(
                "  {name}: manifest={manifest_v} vs Cargo={cargo_v}"
            ));
        }
    }
    out
}

#[test]
fn manifest_version_matches_cargo_version() {
    let pairs = read_manifested(&bundled_plugin_dirs());
    let checked = pairs.len();
    let mismatches = version_mismatches(&pairs);

    assert!(
        checked > 0,
        "스캔된 builtin plugin 이 0 개 — 경로/네이밍이 바뀌었는지 확인"
    );
    assert!(
        mismatches.is_empty(),
        "플러그인 매니페스트와 Cargo 버전이 다르다. 버전을 함께 갱신하고 다시 서명한다:\n{}",
        mismatches.join("\n")
    );
}

/// crates 하위 src의 Rust 파일 수집 하한. 과거 실측보다 여유를 두어 크레이트 정리를 허용한다.
const CRATES_FLOOR: Floor = Floor {
    min: 450,
    measured: 576,
    measured_on: "2026-09-08",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "b134d28e3에서 측정한 과거 값이다. 이력 재작성 전 커밋이라 현재 develop의 조상에서는 재현할 수 없다.",
    ),
    why_this_gap: "crates/<크레이트>/src 아래 Rust 파일 수다. 2026-09-08의 b134d28e3에서 576개를 측정했고 가장 큰 크레이트 하나의 당시 파일 수 126개를 여유로 둬 하한 450으로 정했다. tests 등 src 밖의 Rust 파일은 이 값에 포함하지 않는다. 옛 측정값이며 현재 트리에서 다시 센 값은 아니다.",
};

/// PLUGIN_ID 뒤의 콜론까지 비교해 비슷한 이름의 다른 상수를 제외한다.
/// 매니페스트의 ID와 코드의 ID는 별도로 선언되므로 둘의 값을 대조해야 한다.
fn plugin_id_in_line(line: &str) -> Option<&str> {
    let (_, rest) = line.trim_start().split_once("const PLUGIN_ID:")?;
    rest.split('"').nth(1)
}

/// 플러그인별 미발견 목록. 한 플러그인의 여분 상수가 다른 플러그인의 누락을 가리지 않게 한다.
struct IdScan {
    blind: Vec<String>,
    mismatches: Vec<String>,
}

/// 소스 원문과 플러그인 목록을 받아 합성 입력도 같은 방식으로 검사한다.
fn id_scan(sources: &[Source], plugins: &[(String, String)]) -> IdScan {
    let mut blind = Vec::new();
    let mut mismatches = Vec::new();

    for (name, manifest_id) in plugins {
        let mut found_here = 0usize;
        let prefix = format!("crates/{name}/src/");
        for (rel, text) in sources.iter().filter(|(rel, _)| rel.starts_with(&prefix)) {
            for line in text.lines() {
                let Some(value) = plugin_id_in_line(line) else {
                    continue;
                };
                found_here += 1;
                if value != manifest_id {
                    mismatches.push(format!(
                        "  {rel}: manifest id={manifest_id} vs const PLUGIN_ID={value}"
                    ));
                }
            }
        }
        if found_here == 0 {
            blind.push(format!("  {name}"));
        }
    }

    IdScan { blind, mismatches }
}

#[test]
fn manifest_id_matches_the_plugin_id_constant() {
    let root = tasty_doc_guards::repo_root();
    let walked = walk_with_floor(
        &root.join("crates"),
        &root,
        &CRATES_FLOOR,
        Descend::SkipBuildCaches,
        &|w: &Walked| w.rel.ends_with(".rs") && w.rel.contains("/src/"),
    )
    .unwrap_or_else(|why| panic!("{why}"));
    let sources: Vec<Source> = walked
        .iter()
        .map(|w| {
            (
                w.rel.clone(),
                std::fs::read_to_string(&w.path).unwrap_or_default(),
            )
        })
        .collect();

    let dirs = bundled_plugin_dirs();
    let plugins: Vec<(String, String)> = dirs
        .iter()
        .map(|(name, dir)| {
            let manifest = dir.join("tasty-plugin.toml");
            let id = extract_id(
                &std::fs::read_to_string(&manifest).expect("매니페스트 read 실패"),
                &manifest,
            );
            (name.clone(), id)
        })
        .collect();

    let checked = plugins.len();
    let IdScan { blind, mismatches } = id_scan(&sources, &plugins);

    assert!(
        checked > 0,
        "스캔된 builtin plugin 이 0 개 — 경로/네이밍이 바뀌었는지 확인"
    );
    assert!(
        blind.is_empty(),
        "PLUGIN_ID 상수를 찾지 못한 플러그인이다. 상수 이름과 소스 수집 범위를 확인한다:\n{}",
        blind.join("\n")
    );
    assert!(
        mismatches.is_empty(),
        "매니페스트 id와 코드의 PLUGIN_ID가 다르다. 플러그인이 보내는 식별자와 등록 설정을 일치시킨다:\n{}",
        mismatches.join("\n")
    );
}

/// 실제 저장소 경로로 오인하지 않도록 합성 파일명은 문자열 안에만 둔다.
fn corpus(pairs: &[(&str, &str)]) -> Vec<Source> {
    pairs
        .iter()
        .map(|(rel, src)| ((*rel).to_string(), (*src).to_string()))
        .collect()
}

fn roster(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, id)| ((*name).to_string(), (*id).to_string()))
        .collect()
}

#[test]
fn a_suffixed_name_is_not_the_version_field() {
    let src =
        "manifest_version = 1\napi_version = \"3\"\nversions = [\"9\"]\nversion = \"0.1.2\"\n";
    assert_eq!(extract_version(src, Path::new("zone")), "0.1.2");

    assert_eq!(
        extract_version("version=\"7.7.7\"\n", Path::new("zone")),
        "7.7.7",
        "공백 없는 형태를 못 읽는다"
    );
}

#[test]
fn a_drifted_pair_is_named_and_a_matching_pair_is_not() {
    let same: Manifested = (
        "zonealpha".to_string(),
        "version = \"0.2.0\"\n".to_string(),
        "[package]\nversion = \"0.2.0\"\n".to_string(),
    );
    let drifted: Manifested = (
        "zonebeta".to_string(),
        "version = \"0.1.1\"\n".to_string(),
        "[package]\nversion = \"0.1.9\"\n".to_string(),
    );

    assert_eq!(
        version_mismatches(std::slice::from_ref(&same)),
        Vec::<String>::new()
    );
    assert_eq!(
        version_mismatches(&[same, drifted]),
        vec!["  zonebeta: manifest=0.1.1 vs Cargo=0.1.9".to_string()]
    );
}

#[test]
fn the_constant_needle_needs_the_colon_and_the_quotes() {
    assert_eq!(
        plugin_id_in_line("    pub const PLUGIN_ID: &str = \"zone.alpha\";"),
        Some("zone.alpha")
    );
    assert_eq!(
        plugin_id_in_line("const PLUGIN_IDENT: &str = \"zone.alpha\";"),
        None,
        "다른 상수를 이 상수로 센다"
    );
    assert_eq!(
        plugin_id_in_line("    let sent = PLUGIN_ID;"),
        None,
        "사용처를 선언으로 센다"
    );
    assert_eq!(
        plugin_id_in_line("const PLUGIN_ID: &str = OTHER;"),
        None,
        "따옴표 없는 값에서 값을 지어낸다"
    );
}

#[test]
fn a_plugin_with_no_constant_is_blind_not_matching() {
    let sources = corpus(&[(
        "crates/tasty-plugin-zonealpha/src/lib.rs",
        "pub const PLUGIN_ID: &str = \"zone.alpha\";",
    )]);
    let plugins = roster(&[
        ("tasty-plugin-zonealpha", "zone.alpha"),
        ("tasty-plugin-zonebeta", "zone.beta"),
    ]);

    let scan = id_scan(&sources, &plugins);
    assert_eq!(scan.blind, vec!["  tasty-plugin-zonebeta".to_string()]);
    assert_eq!(
        scan.mismatches,
        Vec::<String>::new(),
        "상수 미발견을 값 불일치로도 집계했다"
    );
}

/// 한 플러그인에 상수가 두 개 있어도 다른 플러그인에서 빠진 상수를 대신할 수 없다.
#[test]
fn the_blind_check_is_per_plugin_not_a_sum() {
    let sources = corpus(&[
        (
            "crates/tasty-plugin-zonealpha/src/lib.rs",
            "pub const PLUGIN_ID: &str = \"zone.alpha\";",
        ),
        (
            "crates/tasty-plugin-zonealpha/src/main.rs",
            "const PLUGIN_ID: &str = \"zone.alpha\";",
        ),
    ]);
    let plugins = roster(&[
        ("tasty-plugin-zonealpha", "zone.alpha"),
        ("tasty-plugin-zonebeta", "zone.beta"),
    ]);

    assert_eq!(
        id_scan(&sources, &plugins).blind,
        vec!["  tasty-plugin-zonebeta".to_string()]
    );
}

/// 접두가 비슷한 다른 크레이트를 같은 플러그인으로 세지 않아야 한다.
#[test]
fn a_constant_under_another_prefix_does_not_cover_this_plugin() {
    let sources = corpus(&[(
        "crates/tasty-plugin-zonealphax/src/lib.rs",
        "pub const PLUGIN_ID: &str = \"zone.alpha\";",
    )]);
    let plugins = roster(&[("tasty-plugin-zonealpha", "zone.alpha")]);

    assert_eq!(
        id_scan(&sources, &plugins).blind,
        vec!["  tasty-plugin-zonealpha".to_string()],
        "접두가 겹치는 다른 crate 의 상수를 이 plugin 것으로 센다"
    );
}

#[test]
fn a_drifted_constant_is_named_with_the_file_that_holds_it() {
    let sources = corpus(&[
        (
            "crates/tasty-plugin-zonealpha/src/lib.rs",
            "pub const PLUGIN_ID: &str = \"zone.alpha\";",
        ),
        (
            "crates/tasty-plugin-zonealpha/src/main.rs",
            "const PLUGIN_ID: &str = \"zone.alpha.old\";",
        ),
    ]);
    let plugins = roster(&[("tasty-plugin-zonealpha", "zone.alpha")]);

    let scan = id_scan(&sources, &plugins);
    assert_eq!(scan.blind, Vec::<String>::new());
    assert_eq!(
        scan.mismatches,
        vec![
            "  crates/tasty-plugin-zonealpha/src/main.rs: manifest id=zone.alpha vs const PLUGIN_ID=zone.alpha.old"
                .to_string()
        ]
    );
}

#[test]
fn a_version_in_another_table_is_not_the_packages_version() {
    let cargo = "[dependencies.zonebox]\nversion = \"9.9.9\"\n\n[package]\nversion = \"0.1.2\"\n";
    assert_eq!(extract_version(cargo, Path::new("zone")), "0.1.2");

    assert_eq!(
        extract_version("version = \"0.3.0\"\n[permissions]\n", Path::new("zone")),
        "0.3.0",
        "섹션 앞의 최상위 값을 못 읽는다"
    );
    assert_eq!(
        extract_version(
            "[package]\nname = \"zone\"\nversion = \"0.4.0\"\n",
            Path::new("zone")
        ),
        "0.4.0",
        "`[package]` 안의 값을 못 읽는다"
    );
}

#[test]
fn the_owning_tables_id_is_found_even_when_indented() {
    assert_eq!(
        extract_id(
            "  id = \"zone.right\"\n[meta]\nid = \"zone.wrong\"\n",
            Path::new("zone")
        ),
        "zone.right"
    );
}

#[test]
#[should_panic(expected = "id 필드를 찾지 못함")]
fn an_id_that_lives_only_in_another_table_is_not_found() {
    extract_id("[meta]\nid = \"zone.wrong\"\n", Path::new("zone"));
}
