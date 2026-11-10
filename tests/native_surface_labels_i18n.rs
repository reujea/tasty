//! 플랫폼 전용 메뉴·트레이·Jump List의 번역 키와 placeholder를 모든 내장 언어에서 확인한다.
//! 플랫폼 소스를 실행하지 않고 읽으므로 다른 운영체제의 키도 검사할 수 있다.

use std::collections::{BTreeMap, HashMap};

/// 플랫폼 소스에서 추출한 키와 별도 검사로 대조하는 필수 키 목록이다.
const REQUIRED_KEYS: &[&str] = &[
    // macOS 메뉴
    "menu.macos.about",
    "menu.macos.hide",
    "menu.macos.hide_others",
    "menu.macos.show_all",
    "menu.macos.quit",
    "menu.macos.file",
    "menu.macos.new_window",
    "menu.macos.window",
    "menu.macos.minimize",
    "menu.macos.zoom",
    "menu.macos.close_window",
    // 시스템 트레이
    "tray.show_window",
    "tray.new_window",
    "tray.quit",
    "tray.tooltip",
    // Windows Jump List
    "jump_list.new_window",
    "jump_list.new_window_desc",
];

const APP_NAME_FMT_KEYS: &[&str] = &["menu.macos.about", "menu.macos.hide", "menu.macos.quit"];

fn lang_files() -> Vec<(&'static str, String)> {
    tasty_i18n::BUILTIN_CODES
        .iter()
        .map(|code| {
            let path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang/{code}.toml"));
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} 읽기 실패: {e}", path.display()));
            (*code, text)
        })
        .collect()
}

/// 평탄화는 로더 함수를 사용하고 비교를 위해 정렬된 맵으로 옮긴다.
fn load(lang: &str, toml_str: &str) -> BTreeMap<String, String> {
    let value: toml::Value = toml_str
        .parse()
        .unwrap_or_else(|e| panic!("lang/{lang}.toml 파싱 실패: {e}"));
    let mut flat = HashMap::new();
    tasty_i18n::flatten_catalog_toml("", &value, &mut flat);
    flat.into_iter().collect()
}

#[test]
fn native_surface_keys_exist_in_all_languages() {
    for (lang, toml_str) in lang_files() {
        let table = load(lang, &toml_str);
        let missing: Vec<&str> = REQUIRED_KEYS
            .iter()
            .copied()
            .filter(|k| table.get(*k).is_none_or(|v| v.trim().is_empty()))
            .collect();
        assert!(
            missing.is_empty(),
            "lang/{lang}.toml 에 네이티브 표면 키가 없거나 비어 있음: {missing:?}"
        );
    }
}

#[test]
fn app_name_keys_have_exactly_one_placeholder() {
    for (lang, toml_str) in lang_files() {
        let table = load(lang, &toml_str);
        for key in APP_NAME_FMT_KEYS {
            let value = &table[*key];
            assert_eq!(
                value.matches("{}").count(),
                1,
                "lang/{lang}.toml `{key}` = {value:?} — 앱 이름 placeholder `{{}}` 는 정확히 1 개"
            );
        }
    }
}

#[test]
fn non_fmt_keys_have_no_placeholder() {
    for (lang, toml_str) in lang_files() {
        let table = load(lang, &toml_str);
        for key in REQUIRED_KEYS
            .iter()
            .filter(|k| !APP_NAME_FMT_KEYS.contains(k))
        {
            let value = &table[*key];
            assert!(
                !value.contains("{}"),
                "lang/{lang}.toml `{key}` = {value:?} — `t()` 로 조회하는 키에 placeholder 가 있음"
            );
        }
    }
}

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const PLATFORM_DIR: &str = "crates/tasty-platform/src";

/// 2026-09-20 플랫폼 크레이트 Rust 파일 17개를 측정했다. 목록과 수집이 함께 비는 경우를 찾도록 넉넉한 하한을 둔다.
const MIN_PLATFORM_FILES: usize = 10;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn gather_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            gather_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Call {
    Plain,
    Fmt,
}

/// 일반 호출의 리터럴 키만 읽는다. 앞 식별자 문자를 검사해 insert·expect 등을 제외하며 줄 앞 주석은 건너뛴다.
/// 여러 줄 호출이나 블록 주석 등 모든 Rust 구문을 처리하지는 않는다.
fn calls_in(src: &str) -> Vec<(String, Call)> {
    let mut out = Vec::new();
    for line in src.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let b = line.as_bytes();
        let mut i = 0usize;
        while i < b.len() {
            if b[i] == b't'
                && !i.checked_sub(1).is_some_and(|p| is_ident_byte(b[p]))
                && let Some((key, call, next)) = call_at(b, i)
            {
                out.push((key, call));
                i = next;
                continue;
            }
            i += 1;
        }
    }
    out
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn call_at(b: &[u8], at: usize) -> Option<(String, Call, usize)> {
    let (call, open) = if b[at..].starts_with(br#"t(""#) {
        (Call::Plain, at + 3)
    } else if b[at..].starts_with(br#"t_fmt(""#) {
        (Call::Fmt, at + 7)
    } else {
        return None;
    };
    let close = open + b[open..].iter().position(|c| *c == b'"')?;
    let key = std::str::from_utf8(&b[open..close]).ok()?;
    Some((key.to_string(), call, close + 1))
}

fn scan_platform_calls() -> (usize, Vec<(String, Call)>) {
    let root = repo_root().join(PLATFORM_DIR);
    let mut files = Vec::new();
    gather_rs(&root, &mut files);
    files.sort();
    let mut calls = Vec::new();
    for file in &files {
        let src = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("{} 읽기 실패: {e}", file.display()));
        calls.extend(calls_in(&src));
    }
    (files.len(), calls)
}

#[test]
fn the_required_key_list_matches_what_the_platform_sources_ask_for() {
    let (file_count, calls) = scan_platform_calls();
    assert!(
        file_count >= MIN_PLATFORM_FILES,
        "{PLATFORM_DIR}에서 Rust 파일을 {file_count} 개만 수집했다(2026-09-20 측정 17개). 실제 파일 목록과 재귀 순회를 대조한다. Git 미추적 파일도 수집되므로 두 수가 반드시 같지는 않다. 실제 감소라면 근거와 함께 하한을 검토한다."
    );

    let found: BTreeSet<&str> = calls.iter().map(|(k, _)| k.as_str()).collect();
    let listed: BTreeSet<&str> = REQUIRED_KEYS.iter().copied().collect();

    let unlisted: Vec<&&str> = found.difference(&listed).collect();
    let stale: Vec<&&str> = listed.difference(&found).collect();

    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "플랫폼 소스의 키 후보와 REQUIRED_KEYS가 다르다. 실제 사용처를 확인해 목록과 번역을 갱신한다.\n목록에 없는 키: {unlisted:?}\n목록에만 있는 키: {stale:?}"
    );
}

/// t와 t_fmt의 키 집합을 별도로 대조해야 placeholder 규칙을 올바르게 적용할 수 있다.
#[test]
fn the_fmt_key_list_matches_which_call_the_source_uses() {
    let (_, calls) = scan_platform_calls();
    let found_fmt: BTreeSet<&str> = calls
        .iter()
        .filter(|(_, c)| *c == Call::Fmt)
        .map(|(k, _)| k.as_str())
        .collect();
    let listed_fmt: BTreeSet<&str> = APP_NAME_FMT_KEYS.iter().copied().collect();
    assert_eq!(
        found_fmt, listed_fmt,
        "APP_NAME_FMT_KEYS 가 소스의 t_fmt 호출과 다르다"
    );

    let found_plain: BTreeSet<&str> = calls
        .iter()
        .filter(|(_, c)| *c == Call::Plain)
        .map(|(k, _)| k.as_str())
        .collect();
    let both: Vec<&&str> = found_fmt.intersection(&found_plain).collect();
    assert!(
        both.is_empty(),
        "같은 키를 t 와 t_fmt 로 모두 부른다 — placeholder 규칙이 갈린다: {both:?}"
    );
}

#[test]
fn the_extractor_sees_calls_and_not_lookalikes() {
    let fixture = concat!(
        "let a = ",
        "t(\"real.plain\");\n",
        "let b = ",
        "t_fmt(\"real.fmt\", &name);\n",
        "info.insert(\"not.a.key\".into(), v);\n",
        "let c = x.expect(\"not.a.key.either\");\n",
        "// ",
        "t(\"in.a.comment\")\n",
        "/// ",
        "t_fmt(\"in.a.doc.comment\", x)\n",
        "let d = wide_null(",
        "t(\"nested.call\"));\n",
    );
    let got: Vec<(String, Call)> = calls_in(fixture);
    assert_eq!(
        got,
        vec![
            ("real.plain".to_string(), Call::Plain),
            ("real.fmt".to_string(), Call::Fmt),
            ("nested.call".to_string(), Call::Plain),
        ],
        "호출·비호출 예시의 키 추출 결과가 달라졌다"
    );
}

/// 자기 합성 키를 실제 사용처로 세지 않도록 수집 루트에서 이 검사 파일을 제외한다.
#[test]
fn the_scan_root_does_not_contain_this_guard() {
    let root = repo_root().join(PLATFORM_DIR);
    let me = Path::new(file!());
    assert!(
        !me.starts_with(PLATFORM_DIR),
        "이 가드({}) 가 스캔 루트({}) 안에 있다 — 자기 픽스처를 실제 키로 센다",
        me.display(),
        root.display()
    );
}
