//! 컴파일된 도움말과 카탈로그의 키 집합·영어 원문을 대조한다.

use clap::{CommandFactory, Parser};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<크레이트> 아래에 있어야 한다")
        .to_path_buf()
}

/// debug 빌드에만 있는 인자의 키. release 조합에서는 컴파일된 도움말에 슬롯이 없으므로 대조에서 뺀다.
const DEBUG_ONLY_KEYS: &[&str] = &[
    "cli.help._root.arg.launch.help",
    "cli.help._root.arg.enable_input_simulation.help",
];

/// debug 빌드에만 있는 하위 명령의 키 접두사(`tasty debug …`).
const DEBUG_ONLY_PREFIXES: &[&str] = &["cli.help._root.debug."];

fn is_debug_only(key: &str) -> bool {
    DEBUG_ONLY_KEYS.contains(&key) || DEBUG_ONLY_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// `lang/en.toml` 에서 `cli.help.` 로 시작하는 키만 평평하게 꺼낸다. release 조합에서는 debug 전용 키를 뺀다.
fn english_catalog() -> BTreeMap<String, String> {
    let path = repo_root().join("lang/en.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 읽지 못했다 — {e}", path.display()));
    let value: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("{} 를 파싱하지 못했다 — {e}", path.display()));
    let mut out = BTreeMap::new();
    flatten(&value, String::new(), &mut out);
    out.retain(|k, _| k.starts_with(&format!("{}.", tasty_cli::help_i18n::PREFIX)));
    if !cfg!(debug_assertions) {
        out.retain(|k, _| !is_debug_only(k));
    }
    out
}

fn flatten(value: &toml::Value, prefix: String, out: &mut BTreeMap<String, String>) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(v, next, out);
            }
        }
        toml::Value::String(s) => {
            out.insert(prefix, s.clone());
        }
        _ => {}
    }
}

fn slots() -> Vec<tasty_cli::help_i18n::Slot> {
    tasty_cli::help_i18n::slots(&tasty_cli::Cli::command())
}

#[test]
fn every_english_help_key_names_a_real_slot() {
    let by_key: BTreeMap<String, String> =
        slots().into_iter().map(|s| (s.key, s.english)).collect();
    let catalog = english_catalog();
    let orphans: Vec<&String> = catalog
        .keys()
        .filter(|k| !by_key.contains_key(*k))
        .collect();
    assert!(
        orphans.is_empty(),
        "컴파일된 도움말에 없는 영어 카탈로그 키: {orphans:#?}\n명령·인자 이름과 번역 키가 일치하는지 확인한다."
    );
}

#[test]
fn every_english_help_value_matches_the_compiled_text() {
    let by_key: BTreeMap<String, String> =
        slots().into_iter().map(|s| (s.key, s.english)).collect();
    let mut drift = Vec::new();
    for (key, value) in english_catalog() {
        let Some(compiled) = by_key.get(&key) else {
            continue; // 위 시험이 보고한다
        };
        if compiled != &value {
            drift.push(format!(
                "  {key}\n    소스: {compiled:?}\n    en.toml: {value:?}"
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "영어 카탈로그가 컴파일된 도움말과 다르다:\n{}\n소스의 doc 주석을 기준으로 en.toml을 맞춘다.",
        drift.join("\n")
    );
}

#[test]
fn every_help_slot_has_a_translation_key() {
    let all: BTreeSet<String> = slots().into_iter().map(|s| s.key).collect();
    assert!(!all.is_empty());
    let catalog: BTreeSet<String> = english_catalog().into_keys().collect();
    assert_eq!(
        all, catalog,
        "compiled help slots and catalog must match exactly"
    );
}

#[test]
fn debug_only_flags_and_commands_exist_only_in_debug_builds() {
    let parsed = tasty_cli::Cli::try_parse_from(["tasty", "--launch"]);
    assert_eq!(parsed.is_ok(), cfg!(debug_assertions), "{:?}", parsed.err());
    for key in DEBUG_ONLY_KEYS {
        let compiled = slots().into_iter().any(|s| s.key == *key);
        assert_eq!(compiled, cfg!(debug_assertions), "{key}");
    }
    for prefix in DEBUG_ONLY_PREFIXES {
        let compiled = slots().into_iter().any(|s| s.key.starts_with(prefix));
        assert_eq!(compiled, cfg!(debug_assertions), "{prefix}");
    }
}

/// 그룹 제목·시작 안내는 clap 슬롯이 아니므로 slots 대조에 들어가지 않는다. 내장 언어마다 따로 둔다.
#[test]
fn group_headings_and_start_lines_exist_in_every_language() {
    let keys = tasty_cli::help_groups::text_keys();
    assert!(keys.len() > 10, "{keys:?}");
    for lang in tasty_i18n::BUILTIN_CODES {
        let path = repo_root().join(format!("lang/{lang}.toml"));
        let value: toml::Value = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        let mut flat = BTreeMap::new();
        flatten(&value, String::new(), &mut flat);
        let missing: Vec<&String> = keys
            .iter()
            .filter(|k| flat.get(*k).is_none_or(|v| v.trim().is_empty()))
            .collect();
        assert!(missing.is_empty(), "{lang}: {missing:?}");
    }
}
