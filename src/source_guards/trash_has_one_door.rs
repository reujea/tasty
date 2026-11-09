//! OS 휴지통 호출은 `os_trash.rs` 한 곳에서만 한다. 그 파일이 시험 빌드에서 실제 호출을 막으므로,
//! 다른 자리의 호출은 시험을 사용자 휴지통과 macOS Finder 에 다시 닿게 한다.
//!
//! src, crates, 루트 tests 의 주석·문자열을 뺀 소스에서 줄 단위로 찾는다. 크레이트를 부르는 경로와
//! `use`·`extern crate` 로 이름을 들여오거나 바꾸는 형태를 금지한다. `trash::Error` 처럼 호출이 아닌
//! 경로는 허용한다. 여러 줄에 나눠 쓴 선언과 `use {trash};` 처럼 별칭 없는 묶음은 찾지 못한다.

use tasty_doc_guards::source_text::mask_non_code;

const DOOR: &str = "src/app/file_jobs/os_trash.rs";

const CALLS: &[&str] = &[
    "trash::delete",
    "trash::TrashContext",
    "trash::os_limited",
    "use trash::",
    "use trash;",
    "use ::trash",
    "trash as ",
    "extern crate trash",
];

/// 줄이 금지 형태를 쓰는가. `os_trash::delete` 처럼 이름의 뒷부분이 겹치는 경우는 세지 않는다.
fn calls_the_crate(line: &str) -> bool {
    CALLS.iter().any(|call| {
        line.match_indices(call).any(|(at, _)| {
            !line[..at]
                .chars()
                .next_back()
                .is_some_and(|before| before.is_alphanumeric() || before == '_')
        })
    })
}

#[test]
fn each_way_to_reach_the_crate_is_found() {
    for line in [
        "trash::delete(path)",
        "trash::delete_all(paths)",
        "let context = trash::TrashContext::new();",
        "trash::os_limited::list()",
        "use trash::delete;",
        "use trash::delete as d;",
        "use {trash::delete as d};",
        "use trash;",
        "use trash as t;",
        "pub(crate) use trash as t;",
        "use {trash as t};",
        "use ::trash as t;",
        "extern crate trash;",
        "extern crate trash as t;",
        "::trash::delete(path)",
    ] {
        assert!(calls_the_crate(line), "{line}");
    }
    for line in [
        "fn trash_refused(error: &trash::Error, path: &Path) -> bool {",
        "os_trash::send(shared, path)",
        "my_trash::delete(path)",
        "let trashed = 1;",
    ] {
        assert!(!calls_the_crate(line), "{line}");
    }
}

#[test]
fn the_os_trash_is_only_called_through_its_one_door() {
    let mut door_calls = 0usize;
    let mut integration_files = 0usize;
    let mut offenders = Vec::new();
    let sources = super::rust_sources_with_integration_tests();
    for (rel_path, raw) in &sources {
        let rel = rel_path.to_string_lossy().replace('\\', "/");
        integration_files += usize::from(rel.starts_with("tests/"));
        for (i, line) in mask_non_code(raw).lines().enumerate() {
            if !calls_the_crate(line) {
                continue;
            }
            if rel == DOOR {
                door_calls += 1;
            } else {
                offenders.push(format!("  {rel}:{}  {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        integration_files > 0,
        "루트 tests 를 수집하지 못했다({}개 수집). 순회 범위를 확인한다.",
        sources.len()
    );
    assert!(
        offenders.is_empty(),
        "`{DOOR}` 밖에서 휴지통을 부르는 곳이 {}개다:\n{}\n`job::os_trash::send` 를 쓴다. 시험 빌드의 실제 호출은 사용자 휴지통과 Finder 에 닿는다.",
        offenders.len(),
        offenders.join("\n")
    );
    assert!(
        door_calls > 0,
        "`{DOOR}` 에서 휴지통 호출을 찾지 못했다. 파일을 옮겼으면 이 검사를 따라 고친다."
    );
}
