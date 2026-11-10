//! 루트와 번들 플러그인의 언어별 키·placeholder·동일 값 예외를 대조한다.
//! 소스에 적힌 번역 키의 존재와 카탈로그 키의 사용처도 각각 검사한다. 카탈로그 평탄화는 실제 로더 함수를 쓴다.
//! 자동 실행은 헤드리스 조합에서만 일어난다(check-headless). 기본 조합의 --lib --bins 호출에는 이 통합 타깃이 없다.
//! 이 실행 범위 설명은 ci_channel_claims_match_workflows 검사의 입력이므로 실제 워크플로와 함께 유지한다.
//!
//! 번역 키는 문자열 안에 있어 리터럴을 지우지 않는다. 소스 스캔은 단순 줄·토큰 판독이며
//! 실제 호출 여부나 모든 동적 키를 증명하지는 못한다. 사용처 없는 키의 삭제 여부는 실제 화면도 확인해 판단한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, normalized_rel, walk_with_floor};

const LANGS: &[&str] = &tasty_i18n::BUILTIN_CODES;

/// 영어와 같은 값이 필요한 키·언어·사유다. 기호·약어 등 형태로 인정하는 값은 따로 등록하지 않는다.
const SAME_AS_ENGLISH_ALLOWLIST: &[(&str, &[&str], &str)] = &[
    (
        "settings.number.unit_pt",
        &["ko", "ja", "zh-Hans"],
        "단위 기호",
    ),
    (
        "settings.number.unit_s",
        &["ko", "ja", "zh-Hans"],
        "단위 기호",
    ),
    (
        "settings.anomaly.rss_surge_min_mib.unit",
        &["ko", "ja", "zh-Hans"],
        "MiB is the same binary size unit in all locales",
    ),
    (
        "image.state.too_large_size",
        &["ko", "ja", "zh-Hans"],
        "숫자 자리와 단위 기호 (× px)",
    ),
    ("app.name", &["ko", "ja", "zh-Hans"], "제품명"),
    (
        "explorer.kind.markdown",
        &["ko", "ja", "zh-Hans"],
        "형식 이름(고유 명사)",
    ),
    (
        "settings.appearance.subtab.tasty",
        &["ko", "ja", "zh-Hans"],
        "제품명",
    ),
    (
        "settings.subtab.claude",
        &["ko", "ja", "zh-Hans"],
        "제품명 (Claude Code)",
    ),
    (
        "settings.subtab.codex",
        &["ko", "ja", "zh-Hans"],
        "제품명 (Codex)",
    ),
    (
        "settings.subtab.markdown",
        &["ko", "ja", "zh-Hans"],
        "포맷명",
    ),
    (
        "git_viewer.tools_menu_item",
        &["ko", "ja", "zh-Hans"],
        "도구명 (Git)",
    ),
    (
        "git_viewer.heading",
        &["ko", "ja", "zh-Hans"],
        "도구명 (Git)",
    ),
    (
        "git_viewer.diff_heading",
        &["ko", "ja"],
        "개발 도구 관용 표기 (Diff)",
    ),
    (
        "git_viewer.detached",
        &["ko", "ja"],
        "git HEAD 상태 토큰 — 명령 출력과 같은 말이어야 한다",
    ),
    (
        "remote_tool.attach_tag_inline",
        &["ko", "ja"],
        "attach 프로필 연결 방식 식별자(인라인 host/user)",
    ),
    (
        "remote_tool.attach_tag_profile",
        &["ko", "ja"],
        "attach 프로필 연결 방식 식별자(ssh_ref 참조)",
    ),
    (
        "settings.macos_permissions.debug_tag",
        &["ko", "ja"],
        "빌드 종류 식별자(debug 빌드) — ko/ja 문구도 'debug 빌드' 로 부른다",
    ),
    (
        "remote_tool.field_passkey",
        &["ko", "ja", "zh-Hans"],
        "WebAuthn 용어 — 같은 팝업의 탭명(remote_tool.tab_passkeys)도 Passkey 로 통일",
    ),
    (
        "remote_tool.tab_attach",
        &["ko"],
        "ko 는 attach 를 제품 용어로 유지(remote_attach.* ko 가 'Attach 프로필' 로 부른다)",
    ),
    (
        "settings.file_handler.handlers.field_action_kind",
        &["ko", "ja"],
        "file handler 탭 용어 통일",
    ),
    (
        "settings.file_handler.handlers.field_ipc_method",
        &["ko", "ja"],
        "file handler 탭 용어 통일",
    ),
    (
        "settings.file_handler.handlers.field_surface_kind",
        &["ko", "ja"],
        "file handler 탭 용어 통일",
    ),
    (
        "settings.file_handler.hook_handlers.prio",
        &["ko", "ja"],
        "file handler 탭 용어 통일 (Priority 약칭)",
    ),
    (
        "settings.keybindings.preset_active_tag",
        &["ko", "ja"],
        "도움말이 태그명 'Active' 를 그대로 인용",
    ),
    (
        "cli.remote_check.alive_basic",
        &["ko", "ja"],
        "remote check 결과 행 — 기계 파싱 대상",
    ),
    (
        "cli.remote_check.alive_version",
        &["ko", "ja"],
        "remote check 결과 행 — 기계 파싱 대상",
    ),
    (
        "cli.remote_check.alive_full",
        &["ko", "ja"],
        "remote check 결과 행 — 기계 파싱 대상",
    ),
    (
        "claude.profile.error_prefix",
        &["ko", "ja"],
        "에러 접두 식별자 (profile:)",
    ),
    (
        "claude.gate.error_prefix",
        &["ko", "ja", "zh-Hans"],
        "에러 접두 식별자 (gate:)",
    ),
    (
        "settings.keybindings.subtab.surface",
        &["zh-Hans"],
        "zh-Hans 용어집이 Surface 를 라틴 표기로 유지한다",
    ),
    (
        "tutorial.step_surface_title",
        &["zh-Hans"],
        "zh-Hans 용어집이 Surface 를 라틴 표기로 유지한다",
    ),
    (
        "remote_tool.field_shell",
        &["zh-Hans"],
        "zh-Hans 용어집이 Shell 을 라틴 표기로 유지한다",
    ),
    (
        "attach.tab_title_fallback",
        &["zh-Hans"],
        "zh-Hans 용어집이 Shell 을 라틴 표기로 유지한다",
    ),
    (
        "surface.restore.tab_title_fallback",
        &["zh-Hans"],
        "zh-Hans 용어집이 Shell 을 라틴 표기로 유지한다",
    ),
    (
        "surface.kind.markdown",
        &["zh-Hans"],
        "zh-Hans 용어집이 Markdown 을 라틴 표기로 유지한다",
    ),
    (
        "file_handler.format.markdown",
        &["zh-Hans"],
        "zh-Hans 용어집이 Markdown 을 라틴 표기로 유지한다",
    ),
];

const MODIFIER_TOKENS: &[&str] = &[
    "Ctrl", "Alt", "Shift", "Cmd", "Command", "Option", "Super", "Meta", "Win", "Fn",
];

/// 번역이 아직 도착하지 않은 키다. 영어 원문이 번역 기준 커밋 뒤에 생기거나 바뀐 키라 해당 언어 파일에 두지 않고,
/// 런타임은 영어 바탕으로 대체한다. 번역이 들어오면 항목을 지운다. 지운 키가 여기 남으면 검사가 실패한다.
/// 영어 값을 복사해 채우지 않는다. 그러면 번역되지 않은 키를 찾을 수 없다.
const PENDING_TRANSLATION_KEYS: &[(&str, &str, &str)] = &[
    ("lang", "ko", "cli.help._root.remote.connect.about"),
    ("lang", "ko", "cli.help._root.remote.connect.long"),
    (
        "lang",
        "ko",
        "cli.help._root.remote.connect.arg.workspace.help",
    ),
    ("lang", "ja", "cli.help._root.remote.connect.about"),
    ("lang", "ja", "cli.help._root.remote.connect.long"),
    (
        "lang",
        "ja",
        "cli.help._root.remote.connect.arg.workspace.help",
    ),
    ("lang", "zh-Hans", "cli.help._root.remote.connect.about"),
    ("lang", "zh-Hans", "cli.help._root.remote.connect.long"),
    (
        "lang",
        "zh-Hans",
        "cli.help._root.remote.connect.arg.workspace.help",
    ),
    ("lang", "ko", "cli.help._root.set.workspace.about"),
    ("lang", "ko", "cli.help._root.remote.refusals.long"),
    ("lang", "ko", "cli.help._root.remote.disconnect.long"),
    ("lang", "ja", "cli.help._root.set.workspace.about"),
    ("lang", "ja", "cli.help._root.remote.refusals.long"),
    ("lang", "ja", "cli.help._root.remote.disconnect.long"),
    ("lang", "zh-Hans", "cli.help._root.set.workspace.about"),
    ("lang", "zh-Hans", "cli.help._root.remote.refusals.long"),
    ("lang", "zh-Hans", "cli.help._root.remote.disconnect.long"),
];

/// 누락이 알려진 키의 임시 예외다. 키가 추가되면 항목도 제거한다. 새 누락은 수정하는 것이 기본이다.
const PENDING_FIX_MISSING_KEYS: &[(&str, &str)] = &[];

const PRUNE_DIRS: &[&str] = &["target", "dist", ".worktree", ".git", "node_modules"];

/// 순회에서 제외하는 로컬 폴더 이름은 인용 검사에 걸리지 않도록 조각으로 조립한다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn rel_of(file: &Path) -> String {
    normalized_rel(file, root())
}

/// 지원 언어 수 자체는 목록과 디스크의 비교로 확인한다. 하한은 작은 언어 수 변경을 허용하면서 빈 순회를 찾는다.
const LANG_DIR_FLOOR: Floor = Floor {
    min: 1,
    measured: 3,
    measured_on: "2026-09-07",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::NEVER_COUNTED,
    why_this_gap: "지원 언어 수는 실제 목록과 별도로 대조한다. 하한을 측정 3에 붙이면 언어 삭제 자체가 수집 실패로 보일 수 있어 작은 하한으로 빈 수집만 확인한다.",
};

/// 언어 파일과 로더의 목록을 대조한다. 나머지 검사가 첫 언어를 영어로 가정하므로 순서도 확인한다.
#[test]
fn builtin_codes_match_the_language_files_on_disk() {
    let dir = root().join("lang");

    let walked = walk_with_floor(
        &dir,
        &dir,
        &LANG_DIR_FLOOR,
        Descend::Everything,
        &|w: &Walked| w.rel.ends_with(".toml"),
    )
    .unwrap_or_else(|why| panic!("{why}"));

    let on_disk: BTreeSet<String> = walked
        .iter()
        .filter_map(|w| w.rel.strip_suffix(".toml"))
        .map(std::string::ToString::to_string)
        .collect();

    let declared: BTreeSet<String> = tasty_i18n::BUILTIN_CODES
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

    assert_eq!(
        declared,
        on_disk,
        "BUILTIN_CODES와 lang의 언어 파일 목록이 다르다. 목록에서 빠진 파일은 검사되지 않으며 파일이 없는 언어는 읽을 수 없다.\n목록에만: {:?}\n파일에만: {:?}",
        declared.difference(&on_disk).collect::<Vec<_>>(),
        on_disk.difference(&declared).collect::<Vec<_>>()
    );

    assert_eq!(
        tasty_i18n::BUILTIN_CODES.first(),
        Some(&"en"),
        "이 검사는 LANGS[1..]을 영어 외 언어로 취급하므로 BUILTIN_CODES의 첫 항목이 en이어야 한다."
    );
}

fn lang_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![root().join("lang")];
    let crates = root().join("crates");
    let entries =
        std::fs::read_dir(&crates).unwrap_or_else(|e| panic!("read_dir {}: {e}", crates.display()));
    let mut plugin_dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|dir| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("tasty-plugin-"))
        })
        .map(|dir| dir.join("lang"))
        .filter(|lang| lang.join("en.toml").is_file())
        .collect();
    plugin_dirs.sort();
    dirs.extend(plugin_dirs);
    dirs
}

/// 평탄화는 실제 로더와 공유하지만 파싱 실패는 즉시 알린다. 빈 카탈로그끼리 같다는 이유로 통과하지 않게 한다.
fn load(dir: &Path, lang: &str) -> BTreeMap<String, String> {
    let path = dir.join(format!("{lang}.toml"));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let mut flat = std::collections::HashMap::new();
    tasty_i18n::flatten_catalog_toml("", &value, &mut flat);
    flat.into_iter().collect()
}

struct Catalog {
    rel: String,
    by_lang: BTreeMap<&'static str, BTreeMap<String, String>>,
}

fn catalogs() -> Vec<Catalog> {
    lang_dirs()
        .into_iter()
        .map(|dir| Catalog {
            rel: rel_of(&dir),
            by_lang: LANGS.iter().map(|lang| (*lang, load(&dir, lang))).collect(),
        })
        .collect()
}

/// 2026-09-06 측정 10세트(루트1·플러그인9). 하한6은 일부 삭제를 허용하면서 큰 수집 누락을 찾는다.
const MIN_CATALOGS: usize = 6;

/// 2026-09-06 루트 키 1311개를 측정했다. 실제 요구 키 수가 아니라 평탄화 결과가 크게 줄었는지 보는 하한이다.
const MIN_ROOT_KEYS: usize = 400;

/// 카탈로그가 모두 비면 키 집합 대조는 통과할 수 있어 세트 수와 키 수도 확인한다.
/// 일부 플러그인만 비는 경우도 찾는다. 하한을 바꾸기 전에 실제 번역 삭제인지 수집·평탄화 오류인지 확인한다.
#[test]
fn every_catalog_carries_more_than_a_handful_of_keys() {
    let cats = catalogs();
    assert!(
        cats.len() >= MIN_CATALOGS,
        "카탈로그를 {} 개만 찾았다(하한 {MIN_CATALOGS}, 2026-09-06 측정 10). lang_dirs의 수집 범위를 확인한다.",
        cats.len()
    );
    for cat in &cats {
        for lang in LANGS {
            let n = cat.by_lang[lang].len();
            assert!(
                n > 0,
                "{}/{lang}.toml에서 키를 찾지 못했다. 파일 내용과 평탄화 결과를 확인한다. 빈 언어끼리는 키 집합 비교를 통과할 수 있다.",
                cat.rel
            );
        }
        if cat.rel == "lang" {
            for lang in LANGS {
                let n = cat.by_lang[lang].len();
                assert!(
                    n >= MIN_ROOT_KEYS,
                    "lang/{lang}.toml 의 키가 {n} 개다(하한 {MIN_ROOT_KEYS}, \
                     2026-09-06 실측 1311)",
                );
            }
        }
    }
}

/// {}는 개수로, {name}은 이름 집합으로 센다. 이름은 식별자 형식만 허용하고 {0} 등은 본문으로 취급한다.
/// 문자열의 자리표시 세 가지: 순서대로 채우는 `{}` 개수, 번호 자리표시 `{N}` 의 번호 집합,
/// 이름 자리표시 `{name}` 집합.
fn placeholder_forms(value: &str) -> (usize, BTreeSet<usize>, BTreeSet<String>) {
    let mut positional = 0;
    let mut numbered = BTreeSet::new();
    let mut named = BTreeSet::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        let inner = &after[..close];
        if inner.is_empty() {
            positional += 1;
        } else if inner.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = inner.parse() {
                numbered.insert(n);
            }
        } else if is_identifier(inner) {
            named.insert(inner.to_string());
        }
        rest = &after[close + 1..];
    }
    (positional, numbered, named)
}

/// 번역문이 받는 인자 칸과 이름 집합. `{}` 가 n 개면 칸은 0..n, 번호 자리표시면 그 번호들이다.
/// 그래서 en 의 `{} / {}` 와 ko 의 `{1} / {0}` 은 같은 두 칸을 받는다.
fn placeholders(value: &str) -> (BTreeSet<usize>, BTreeSet<String>) {
    let (positional, numbered, named) = placeholder_forms(value);
    let slots = if numbered.is_empty() {
        (0..positional).collect()
    } else {
        numbered
    };
    (slots, named)
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn without_placeholders(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..open]);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn is_language_neutral(value: &str) -> bool {
    let body = without_placeholders(value);
    let letters: Vec<char> = body.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return true; // `—` / `99+` / `{} · {}` / `%`
    }
    let trimmed = body.trim();
    if trimmed.contains("://")
        || trimmed.starts_with("~/")
        || trimmed.starts_with('/')
        || trimmed.starts_with("tasty ")
        || trimmed.contains('$')
    {
        return true; // 경로 · URL · 셸 명령
    }
    if letters.len() <= 5 && letters.iter().all(|c| c.is_ascii_uppercase()) {
        return true;
    }
    MODIFIER_TOKENS.contains(&trimmed)
}

fn allowlisted_same(key: &str, lang: &str) -> bool {
    SAME_AS_ENGLISH_ALLOWLIST
        .iter()
        .any(|(k, langs, _)| *k == key && langs.contains(&lang))
}

fn pending_translation(rel: &str, lang: &str, key: &str) -> bool {
    PENDING_TRANSLATION_KEYS
        .iter()
        .any(|(r, l, k)| *r == rel && *l == lang && *k == key)
}

#[test]
fn key_sets_match_english() {
    let mut problems = Vec::new();
    let cats = catalogs();
    for (rel, lang, key) in PENDING_TRANSLATION_KEYS {
        if !cats.iter().any(|cat| cat.rel == *rel) || !LANGS.contains(lang) {
            problems.push(format!(
                "  PENDING_TRANSLATION_KEYS names `{key}` in {rel}/{lang}.toml, which is not a catalog"
            ));
        }
    }
    for cat in cats {
        let en: BTreeSet<&String> = cat.by_lang["en"].keys().collect();
        for lang in &LANGS[1..] {
            let other: BTreeSet<&String> = cat.by_lang[lang].keys().collect();
            // en 단수 변형(`<key>_one`)은 한 문자열로 충분한 언어에 두지 않아도 된다.
            for key in en
                .difference(&other)
                .filter(|k| one_variant_base(k, &cat.by_lang["en"]).is_none())
            {
                if pending_translation(&cat.rel, lang, key) {
                    continue;
                }
                problems.push(format!("  {}/{lang}.toml: missing `{key}`", cat.rel));
            }
            for (_, _, key) in PENDING_TRANSLATION_KEYS
                .iter()
                .filter(|(rel, l, _)| *rel == cat.rel && l == lang)
            {
                if !cat.by_lang["en"].contains_key(*key) {
                    problems.push(format!(
                        "  {}/{lang}.toml: PENDING_TRANSLATION_KEYS names `{key}`, which en does not have",
                        cat.rel
                    ));
                } else if cat.by_lang[lang].contains_key(*key) {
                    problems.push(format!(
                        "  {}/{lang}.toml: `{key}` is translated now — remove it from PENDING_TRANSLATION_KEYS",
                        cat.rel
                    ));
                }
            }
            for key in other.difference(&en) {
                problems.push(format!(
                    "  {}/{lang}.toml: extra `{key}` (not in en)",
                    cat.rel
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "translation catalogs diverge from en (CLAUDE.md 국제화: add every key to all three files):\n{}",
        problems.join("\n")
    );
}

/// `key` 가 en 단수 변형이면 그 기준 키. 기준 키가 카탈로그에 없으면 `_one` 으로 끝나도 변형이 아니다.
fn one_variant_base<'a>(key: &'a str, en: &BTreeMap<String, String>) -> Option<&'a str> {
    key.strip_suffix(tasty_i18n::plural::ONE_SUFFIX)
        .filter(|base| en.contains_key(*base))
}

/// 단수 변형은 기준 키와 같은 인자 칸을 받는다. 수 자리도 남겨 두면 `t_count` 가 같은 인자로 채운다.
#[test]
fn one_variants_take_the_slots_of_their_key() {
    let mut problems = Vec::new();
    let mut variants = 0usize;
    for cat in catalogs() {
        let en = &cat.by_lang["en"];
        for lang in LANGS {
            for (key, value) in &cat.by_lang[*lang] {
                let Some(base) = one_variant_base(key, en) else {
                    continue;
                };
                variants += 1;
                let Some(base_value) = cat.by_lang[*lang].get(base) else {
                    problems.push(format!(
                        "  {}/{lang}.toml `{key}`: `{base}` 가 없다",
                        cat.rel
                    ));
                    continue;
                };
                if placeholders(value) != placeholders(base_value) {
                    problems.push(format!(
                        "  {}/{lang}.toml `{key}`: {value:?} 의 칸이 `{base}` {base_value:?} 와 다르다",
                        cat.rel
                    ));
                }
            }
        }
    }
    assert!(variants > 0, "단수 변형이 하나도 없다 — 수집을 확인한다");
    assert!(
        problems.is_empty(),
        "단수 변형의 인자 칸이 기준 키와 다르다:\n{}",
        problems.join("\n")
    );
}

#[test]
fn placeholders_match_english() {
    let mut problems = Vec::new();
    for cat in catalogs() {
        let en = &cat.by_lang["en"];
        for lang in &LANGS[1..] {
            for (key, value) in &cat.by_lang[lang] {
                let Some(en_value) = en.get(key) else {
                    continue; // key_sets_match_english 가 보고한다
                };
                let expected = placeholders(en_value);
                let actual = placeholders(value);
                if expected != actual {
                    problems.push(format!(
                        "  {}/{lang}.toml `{key}`: en has slots {:?} + {:?}, {lang} has slots {:?} + {:?}\n      en: {en_value:?}\n      {lang}: {value:?}",
                        cat.rel, expected.0, expected.1, actual.0, actual.1
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "placeholder mismatch — argument slots (`{{}}` in order, or `{{0}}` `{{1}}` by number) and `{{name}}` set must equal en for every key:\n{}",
        problems.join("\n")
    );
}

/// 한 번역문에 `{}` 와 `{0}` 을 섞지 않는다. 섞으면 런타임은 번호로 채우고 `{}` 를 그대로 남긴다
/// (tasty_i18n::fill_args).
#[test]
fn no_value_mixes_positional_and_numbered_placeholders() {
    let mut problems = Vec::new();
    for cat in catalogs() {
        for lang in LANGS {
            for (key, value) in &cat.by_lang[lang] {
                let (positional, numbered, _) = placeholder_forms(value);
                if positional > 0 && !numbered.is_empty() {
                    problems.push(format!("  {}/{lang}.toml `{key}` = {value:?}", cat.rel));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "a value mixes `{{}}` and `{{0}}` placeholders — use one form per string \
         (docs/dev-guide/i18n.md):\n{}",
        problems.join("\n")
    );
}

/// 위 두 검사가 쓰는 판독의 합성 확인.
#[test]
fn placeholder_slots_read_both_forms() {
    assert_eq!(placeholders("{} / {}").0, placeholders("{1} / {0}").0);
    assert_ne!(placeholders("{} / {}").0, placeholders("{0} / {0}").0);
    assert_eq!(placeholder_forms("{} {0}").0, 1);
    assert_eq!(placeholder_forms("{} {0}").1.len(), 1);
    assert_eq!(placeholders("{n} {}").1.len(), 1);
}

#[test]
fn same_as_english_values_are_allowlisted() {
    let mut problems = Vec::new();
    for cat in catalogs() {
        let en = &cat.by_lang["en"];
        for lang in &LANGS[1..] {
            for (key, value) in &cat.by_lang[lang] {
                let Some(en_value) = en.get(key) else {
                    continue;
                };
                if value != en_value || is_language_neutral(value) || allowlisted_same(key, lang) {
                    continue;
                }
                problems.push(format!("  {}/{lang}.toml `{key}` = {value:?}", cat.rel));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "values identical to en look untranslated — translate them, or if the value must stay \
         (proper noun, fixed identifier, machine-parsed output) add the key to \
         SAME_AS_ENGLISH_ALLOWLIST with a reason (docs/dev-guide/i18n.md 강제 테스트):\n{}",
        problems.join("\n")
    );
}

fn is_source_target(rel: &str) -> bool {
    if !rel.ends_with(".rs") {
        return false;
    }
    let in_crate_src = rel
        .strip_prefix("crates/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, after)| after.starts_with("src/"));
    if !(rel.starts_with("src/") || in_crate_src) {
        return false;
    }
    !(rel.contains("/tests/") || rel.ends_with("/tests.rs") || rel.ends_with("_tests.rs"))
}

fn gather(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if is_source_target(&rel_of(path)) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // CARGO_TARGET_DIR로 이름이 다른 캐시도 생길 수 있어 이름 목록과 실제 빌드 표식을 함께 본다.
            if is_pruned(name) || tasty_doc_guards::is_build_cache_dir(&p) {
                continue;
            }
        }
        gather(&p, out);
    }
}

/// 정확한 #[cfg(test)]·#[test] 줄 이후를 중괄호 깊이로 건너뛴다. 여러 줄 cfg나 모든 조건식을 해석하지는 않는다.
#[derive(Default)]
struct TestRegion {
    skipping: bool,
    depth: i32,
    opened: bool,
}

impl TestRegion {
    fn skip(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();
        if !self.skipping {
            if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[test]") {
                self.skipping = true;
                self.depth = 0;
                self.opened = false;
                return true;
            }
            return false;
        }
        let (opens, closes) = brace_counts(line);
        if opens > 0 {
            self.opened = true;
        }
        self.depth += opens - closes;
        if self.opened {
            if self.depth <= 0 {
                self.skipping = false; // 본문 끝 — 이 줄까지 테스트 영역
            }
        } else if trimmed.ends_with(';') {
            self.skipping = false; // `#[cfg(test)] mod tests;` / `use …;` 한 줄 아이템
        }
        true
    }
}

/// 일반 문자열·일부 문자 리터럴·줄 주석 밖의 중괄호를 센다. raw 문자열과 여러 줄 구문을 완전히 처리하지 못해 범위를 잘못 판단할 수 있다.
fn brace_counts(line: &str) -> (i32, i32) {
    let mut opens = 0;
    let mut closes = 0;
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '\'' => {
                if let Some(&next) = chars.peek()
                    && (next == '{' || next == '}')
                {
                    chars.next();
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'/') => break,
            '{' => opens += 1,
            '}' => closes += 1,
            _ => {}
        }
    }
    (opens, closes)
}

/// 번역 진입점의 소스 판독 횟수다. 경과 시간 대신 횟수로 캐시의 반복 판독을 검출한다.
static DERIVATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn translation_entry_points() -> BTreeSet<String> {
    DERIVATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    const SOURCES: &[&str] = &[
        "crates/tasty-i18n/src/lib.rs",
        "crates/tasty-plugin-sdk/src/i18n.rs",
    ];
    let mut out = BTreeSet::new();
    for rel in SOURCES {
        let path = root().join(rel);
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel} 을 못 읽었다: {e}"));
        for line in text.lines() {
            let Some(after) = line.trim_start().strip_prefix("pub fn ") else {
                continue;
            };
            let name: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // pub fn의 이름 중 t 또는 t_ 접두사를 수집한다. 인자 타입이나 실제 호출 관계는 분석하지 않는다.
            if name == "t" || name.starts_with("t_") {
                out.insert(name);
            }
        }
    }
    out
}

/// 2026-09-05 진입점6개(t, t_args, t_fmt, t_fmt2, t_fmt_fit, t_replace)를 측정했다. 빈 수집을 찾는 하한이다.
const MIN_ENTRY_POINTS: usize = 5;

#[test]
fn the_entry_point_derivation_is_alive() {
    let found = translation_entry_points();
    assert!(
        found.len() >= MIN_ENTRY_POINTS,
        "번역 진입점을 {} 개만 찾았다(하한 {MIN_ENTRY_POINTS}, 2026-09-05 측정 6): {found:?}. 실제 선언과 pub fn 판독을 대조한다. 실제 통합·삭제라면 하한과 필수 이름 목록을 함께 검토한다.",
        found.len()
    );
    for expected in ["t", "t_fmt", "t_args", "t_replace"] {
        assert!(
            found.contains(expected),
            "`{expected}` 가 도출에서 빠졌다: {found:?}"
        );
    }
}

/// 소스 파일을 줄마다 다시 읽지 않도록 호출 접두사 목록을 프로세스당 한 번만 만든다.
fn translation_call_prefixes() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut names: Vec<String> = translation_entry_points().into_iter().collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        names.iter().map(|n| format!("{n}(")).collect()
    })
}

fn literal_keys(line: &str, next: Option<&str>) -> Vec<String> {
    let mut keys = Vec::new();
    for call in translation_call_prefixes() {
        let mut from = 0;
        while let Some(pos) = line[from..].find(call.as_str()) {
            let start = from + pos;
            from = start + call.len();
            let preceded_by_ident = line[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if preceded_by_ident {
                continue;
            }
            let same_line = line[from..].trim_start();
            let rest = if same_line.is_empty() {
                next.map(str::trim_start).unwrap_or("")
            } else {
                same_line
            };
            let Some(after_quote) = rest.strip_prefix('"') else {
                continue; // 변수·format! — 동적 키
            };
            let Some(end) = after_quote.find('"') else {
                continue;
            };
            keys.push(after_quote[..end].to_string());
        }
    }
    keys
}

/// 같은 줄의 인자 자리가 비었을 때만 다음 줄을 읽어 동적 인자를 다른 문자열로 오인하지 않게 한다.
#[test]
fn a_key_wrapped_to_the_next_line_is_still_seen() {
    let found = literal_keys(
        "        return Err(IpcMethodError::new(tr.t_replace(",
        Some("            \"codex.reboot.screen_unreadable\","),
    );
    assert_eq!(
        found,
        vec!["codex.reboot.screen_unreadable".to_string()],
        "여는 괄호에서 줄이 끊긴 호출의 키를 못 봤다"
    );

    let dynamic = literal_keys("    tr.t(key_var);", Some("    \"not.a.key\","));
    assert!(
        dynamic.is_empty(),
        "동적 키인데 다음 줄의 문자열을 키로 집었다: {dynamic:?}"
    );

    let still_dynamic = literal_keys("    tr.t(", Some("        key_var,"));
    assert!(
        still_dynamic.is_empty(),
        "다음 줄이 변수인데 키를 만들어 냈다: {still_dynamic:?}"
    );
}

#[test]
fn literal_translation_keys_exist_in_catalog() {
    let mut known: BTreeSet<String> = BTreeSet::new();
    for dir in lang_dirs() {
        known.extend(load(&dir, "en").into_keys());
    }
    let mut files = Vec::new();
    gather(root(), &mut files);
    files.sort();

    // 전체 스캔 비용을 치르기 전에 두 번의 호출로 캐시 재사용을 확인한다.
    let probe_a = DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed);
    let probe_keys = literal_keys("    t(\"probe.key\")", None);
    let probe_b = DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed);
    let probe_again = literal_keys("    t(\"probe.key\")", None);
    let probe_c = DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed);
    // 캐시 판독 횟수가 0끼리 같아도 통과하지 않도록 실제 키 추출 결과를 확인한다.
    assert_eq!(
        probe_keys, probe_again,
        "동일 입력의 두 호출이 다른 키를 반환했다"
    );
    assert_eq!(
        probe_keys,
        vec!["probe.key".to_string()],
        "탐침의 키를 읽지 못해 캐시의 판독 횟수를 비교할 수 없다"
    );
    assert_eq!(
        probe_c - probe_b,
        0,
        "번역 진입점 판독이 반복됐다(첫 호출 {} 회, 둘째 호출 {} 회). translation_call_prefixes의 OnceLock 캐시를 확인한다.",
        probe_b - probe_a,
        probe_c - probe_b
    );

    // 제외 판정과 키 스캔이 같은 내용을 보도록 파일을 한 번 읽어 보관한다.
    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .filter_map(|file| {
            let contents = std::fs::read_to_string(file).ok()?;
            Some((PathBuf::from(rel_of(file)), contents))
        })
        .collect();

    // 파일 이름만으로 놓치는 test 전용 모듈은 공용 test_only_files 판독 결과로 제외한다.
    let not_shipped = tasty_doc_guards::shipping_scope::test_only_files(root(), &sources);

    assert!(
        !not_shipped.is_empty(),
        "test 전용 파일을 하나도 찾지 못했다. 제외 판독의 대상과 모듈 선언을 확인한다."
    );
    let shipping_probe = Path::new("src/view/settings/ui/tabs/general.rs");
    assert!(
        sources.iter().any(|(p, _)| p == shipping_probe),
        "대조용 제품 파일 {}이 수집되지 않았다",
        shipping_probe.display()
    );
    assert!(
        !not_shipped.contains(shipping_probe),
        "제품 파일 {}을 test 전용으로 분류했다. 해당 파일의 번역 키가 검사에서 빠진다.",
        shipping_probe.display()
    );

    let derivations_before = DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed);
    let mut problems = Vec::new();
    let mut pending_seen: BTreeSet<&str> = BTreeSet::new();
    for (rel_path, contents) in &sources {
        if not_shipped.contains(rel_path) {
            continue;
        }
        let rel = rel_path.display().to_string();
        let mut tests = TestRegion::default();
        let lines: Vec<&str> = contents.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if tests.skip(line) {
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for key in literal_keys(line, lines.get(idx + 1).copied()) {
                if known.contains(&key) {
                    continue;
                }
                if let Some((pending, _)) = PENDING_FIX_MISSING_KEYS.iter().find(|(k, _)| *k == key)
                {
                    pending_seen.insert(pending);
                    continue;
                }
                problems.push(format!("  {rel}:{}: `{key}`", idx + 1));
            }
        }
    }
    // 스캔 크기와 무관하게 캐시 판독은 한 번이어야 한다. 별도 진입점 판독 시험이 병렬로 한 번 호출할 수 있어 상한 2를 둔다.
    let derivations = DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed) - derivations_before;
    assert!(
        derivations <= 2,
        "번역 진입점 판독이 {derivations} 회 실행됐다(상한 2). 스캔 크기와 무관하게 캐시에서 재사용해야 한다. translation_call_prefixes와 별도 판독 호출을 확인한다."
    );
    assert!(
        problems.is_empty(),
        "t() keys not found in any lang/en.toml (typo, or the key was never added — t() would \
         render the key itself):\n{}",
        problems.join("\n")
    );
    let stale: Vec<String> = PENDING_FIX_MISSING_KEYS
        .iter()
        .filter(|(k, _)| !pending_seen.contains(k))
        .map(|(k, note)| format!("  `{k}` ({note})"))
        .collect();
    assert!(
        stale.is_empty(),
        "PENDING_FIX_MISSING_KEYS entries are no longer missing — remove them:\n{}",
        stale.join("\n")
    );
}

/// 예외의 키와 언어가 존재하는지 확인한다. 번역이 여전히 영어와 같아 예외가 필요한지까지 판단하지는 않는다.
#[test]
fn same_as_english_allowlist_points_at_keys_that_exist() {
    let cats = catalogs();
    let mut known: BTreeSet<&String> = BTreeSet::new();
    for cat in &cats {
        known.extend(cat.by_lang["en"].keys());
    }
    assert!(
        !known.is_empty(),
        "카탈로그가 비어 예외 대상 키를 확인할 수 없다"
    );

    let mut problems = Vec::new();
    for (key, langs, _) in SAME_AS_ENGLISH_ALLOWLIST {
        if !known.iter().any(|k| k.as_str() == *key) {
            problems.push(format!("  없는 키: `{key}`"));
        }
        for lang in *langs {
            if !LANGS.contains(lang) {
                problems.push(format!("  없는 언어: `{key}` → `{lang}`"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "면제가 실재하지 않는 것을 가리킨다 — 키가 개명됐으면 항목도 고치고, 사라졌으면 \
         지워라:\n{}",
        problems.join("\n")
    );
}

// 사용처 확인은 Rust·TOML의 키 토큰과 동적 조립 형태를 본다. 문서와 주석의 단순 언급은 소비로 세지 않는다.

/// 일반 템플릿 검사로 확인할 수 없는 동적 키 namespace와 조립 파일이다.
/// clap 도움말처럼 여러 점 구간을 조립하는 키도 있다. 사용처 검사에서 찾지 못한 키를 기록한 ORPHAN_KEYS와 구별한다.
const ASSEMBLED_NAMESPACES: &[(&str, &str)] = &[
    ("cli.help.", "crates/tasty-cli/src/help_i18n.rs"),
    ("tutorial.step_", "src/adapters/ui/tutorial/catalog.rs"),
];

const ORPHAN_KEYS: &[(&str, &str)] = &[
    (
        "attach.held_body",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "attach.held_title",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "attach.held_workspace_body",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "attach.held_workspace_title",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "badge.overflow",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "cli.terminal.desc",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "clipboard_viewer.popup.title",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "dialog.error.file_not_found",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "dialog.error.invalid_format",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "dialog.html.title",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "dialog.html.url_label",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "dialog.recent_files",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "explorer.popup.add_favorite.path",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "explorer.popup.rename.path",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "explorer.popup.rename.rename",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "explorer.tab.close",
        "소비자 검사에서 사용처를 찾지 못했다. 갤러리의 키 후보 설명은 실제 소비가 아니다.",
    ),
    (
        "explorer.tab.new",
        "호출부 0 — `explorer.tab.close` 와 같은 자리, 같은 사유",
    ),
    (
        "git_viewer.error",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "html.settings.note",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "markdown.addr.no_recent",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "pane_context_menu.new_explorer",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "port_scanner.hint",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "port_scanner.no_ports",
        "호출부 0 — 형제 `…no_ports_*` 넷만 쓰인다",
    ),
    (
        "quit_modal.cancel_button",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "remote_tool.hide",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "remote_tool.refresh",
        "호출부 0 — `remote_tool.refresh_tooltip` 만 쓰인다",
    ),
    (
        "remote_tool.reveal",
        "호출부 0 — `remote_tool.reveal_tooltip` 만 쓰인다",
    ),
    (
        "settings.accessibility.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.appearance.colors.bg_label",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.appearance.colors.fg_label",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.appearance.colors.focused_heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.appearance.colors.unfocused_heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.appearance.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.appearance.theme_label",
        "소비자 검사에서 사용처를 찾지 못했다. 문서의 키 이름 예시는 실제 소비가 아니다.",
    ),
    (
        "settings.appearance.tab_font_size_label",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.appearance.tab_width_label",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.appearance.theme.dark",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.appearance.theme.light",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.file_handler.coming_soon",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.file_handler.common.disabled",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.file_handler.common.enabled",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "settings.general.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.keybindings.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.notifications.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.performance.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "settings.terminal.heading",
        "탭 제목을 L2 사이드바가 그리게 되면서 호출부가 사라졌다",
    ),
    (
        "git_viewer.diff_heading",
        "호출부 0 — 이 파일의 SAME_AS_ENGLISH_ALLOWLIST 가 유일한 언급이다",
    ),
    (
        "toast.copied_files",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "toast.cut_files",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "workspace_category.collapse_all",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
    (
        "workspace_category.expand_all",
        "소비자 검사에서 사용처를 찾지 못했다. 삭제 전 실제 UI 사용 여부를 확인한다.",
    ),
];

/// 문자열 안의 단일 placeholder와 점으로 끝나는 키 접두사를 템플릿 후보로 읽는다. 실제 format 호출인지까지 확인하지는 않는다.
fn dynamic_key_templates() -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let mut files = Vec::new();
    gather(root(), &mut files);
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for lit in string_literals(&text) {
            let Some(open) = lit.find('{') else { continue };
            let Some(close) = lit[open..].find('}') else {
                continue;
            };
            let (head, tail) = (&lit[..open], &lit[open + close + 1..]);
            let inner = &lit[open + 1..open + close];
            if !(inner.is_empty() || is_identifier(inner)) {
                continue;
            }
            if !head.ends_with('.') || head.len() < 2 {
                continue;
            }
            if !head
                .trim_end_matches('.')
                .split('.')
                .all(|seg| !seg.is_empty() && is_identifier(seg))
            {
                continue;
            }
            if !tail
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            {
                continue;
            }
            out.insert((head.to_string(), tail.to_string()));
        }
    }
    out
}

/// 큰따옴표 사이를 읽고 이스케이프를 건너뛴다. 완전한 Rust 렉서가 아니며 주석 안 문자열도 읽는다.
fn string_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != b'"' {
            if bytes[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }
        if let Ok(s) = std::str::from_utf8(&bytes[start..j]) {
            out.push(s.to_string());
        }
        i = j + 1;
    }
    out
}

/// 치환 구간에는 점을 허용하지 않는다. 접두사 하나로 하위 키 전체를 사용 중이라고 판단하지 않게 한다.
fn is_dynamically_assembled(key: &str, templates: &BTreeSet<(String, String)>) -> bool {
    templates.iter().any(|(head, tail)| {
        key.len() > head.len() + tail.len()
            && key.starts_with(head.as_str())
            && key.ends_with(tail.as_str())
            && !key[head.len()..key.len() - tail.len()].contains('.')
    })
}

/// 카탈로그와 자기 검사 파일을 제외한 Rust·TOML을 수집한다. Git 추적 여부를 직접 확인하지는 않는다.
/// 문서는 키를 설명할 뿐 소비하지 않을 수 있어 대상에 넣지 않는다.
fn consumer_files() -> Vec<PathBuf> {
    const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];
    const CONSUMER_EXT: &[&str] = &["rs", "toml"];
    let mut out = Vec::new();
    let mut stack = vec![root().to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let rel = rel_of(&path);
            if LANGS
                .iter()
                .any(|l| rel.ends_with(&format!("lang/{l}.toml")))
            {
                continue;
            }
            // 자기 명부의 키를 사용처로 세면 등록만으로 검사가 통과하므로 이 파일은 제외한다.
            if rel == "tests/i18n_key_parity.rs" {
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !CONSUMER_EXT.contains(&ext.as_str()) {
                continue;
            }
            out.push(path);
        }
    }
    out
}

fn all_english_keys() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for dir in lang_dirs() {
        keys.extend(load(&dir, "en").into_keys());
    }
    keys
}

/// 줄 앞 주석을 제외한다. Rust 블록 주석은 여는 줄부터 닫는 줄까지 통째로 제외하므로 같은 줄의 코드도 빠질 수 있다.
/// raw 문자열 내부의 주석처럼 보이는 줄도 정확히 구별하지 못한다.
fn code_lines(text: &str, rs: bool) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let t = line.trim_start();
        if rs {
            if in_block {
                if t.contains("*/") {
                    in_block = false;
                }
                continue;
            }
            if t.starts_with("//") {
                continue;
            }
            if t.starts_with("/*") {
                if !t.contains("*/") {
                    in_block = true;
                }
                continue;
            }
        } else if t.starts_with('#') {
            continue;
        }
        out.push(line);
    }
    out
}

#[test]
fn every_catalog_key_has_a_consumer() {
    let keys = all_english_keys();
    // 키마다 모든 소스를 다시 검색하지 않도록 키 모양 토큰으로 미리 나눈다.
    let mut files = 0usize;
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for path in consumer_files() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        files += 1;
        // 플러그인 ID가 포함된 키를 분리하지 않도록 하이픈도 토큰 문자에 넣는다.
        let rs = path.extension().is_some_and(|e| e == "rs");
        for line in code_lines(&text, rs) {
            for token in line
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'))
            {
                if token.contains('.') && token.len() > 2 {
                    tokens.insert(token.to_string());
                }
            }
        }
    }

    assert!(
        files > 100,
        "소비자 파일이 {files} 개로 부족하다. 수집 범위를 확인한다."
    );
    assert!(
        tokens.len() > 10_000,
        "키 토큰이 {} 개로 부족하다. 수집과 토큰 분리를 확인한다.",
        tokens.len()
    );
    assert!(keys.len() > 1000, "카탈로그 키가 {} 개뿐이다", keys.len());

    let templates = dynamic_key_templates();
    assert!(
        !templates.is_empty(),
        "동적 키 템플릿을 찾지 못했다. 문자열 판독과 소스 범위를 확인한다."
    );

    let mut orphans = BTreeSet::new();
    for key in &keys {
        // 접두사가 같은 다른 키를 사용처로 오인하지 않도록 토큰 전체가 일치해야 한다.
        if tokens.contains(key.as_str()) {
            continue;
        }
        // 단수 변형은 기준 키를 `t_count` 로 읽는 곳이 함께 쓴다.
        if let Some(base) = key.strip_suffix(tasty_i18n::plural::ONE_SUFFIX)
            && keys.contains(base)
            && (tokens.contains(base) || is_dynamically_assembled(base, &templates))
        {
            continue;
        }
        if is_dynamically_assembled(key, &templates) {
            continue;
        }
        if ASSEMBLED_NAMESPACES
            .iter()
            .any(|(prefix, _)| key.starts_with(prefix))
        {
            continue;
        }
        orphans.insert(key.clone());
    }

    let listed: BTreeSet<String> = ORPHAN_KEYS.iter().map(|(k, _)| (*k).to_string()).collect();
    assert_eq!(
        ORPHAN_KEYS.len(),
        listed.len(),
        "ORPHAN_KEYS 에 같은 키가 두 번 있다"
    );

    let new: Vec<&String> = orphans.difference(&listed).collect();
    let gone: Vec<&String> = listed.difference(&orphans).collect();
    assert!(
        new.is_empty() && gone.is_empty(),
        "키 사용처 판독 결과와 ORPHAN_KEYS가 다르다. 실제 UI 사용 여부를 확인한 뒤 카탈로그 또는 명부를 갱신한다.\n새 미사용 후보({}):\n{}\n명부에만 남은 키({}):\n{}",
        new.len(),
        new.iter()
            .map(|k| format!("  {k}"))
            .collect::<Vec<_>>()
            .join("\n"),
        gone.len(),
        gone.iter()
            .map(|k| format!("  {k}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn a_key_named_only_in_prose_is_not_a_consumer() {
    let rs = "// 이 키는 `demo.only.in_comment` 다\nlet k = \"demo.in_code\";\n";
    let kept: Vec<&str> = code_lines(rs, true);
    assert_eq!(kept.len(), 1, "코드 줄 하나만 남아야 한다: {kept:?}");
    assert!(kept[0].contains("demo.in_code"));

    let block = "/*\n demo.in_block\n*/\nlet k = \"demo.after_block\";\n";
    let kept: Vec<&str> = code_lines(block, true);
    assert_eq!(kept.len(), 1, "블록 주석이 안 걸러졌다: {kept:?}");
    assert!(kept[0].contains("demo.after_block"));

    let attr = "#[derive(Debug)]\nstruct S;\n";
    assert_eq!(code_lines(attr, true).len(), 2, "속성 줄을 주석으로 셌다");

    let toml = "# demo.toml_comment\ndescription_i18n_key = \"demo.toml_code\"\n";
    let kept: Vec<&str> = code_lines(toml, false);
    assert_eq!(kept.len(), 1, "TOML 주석이 안 걸러졌다: {kept:?}");
    assert!(kept[0].contains("demo.toml_code"));
}

/// Markdown 파일이 실제로 있지만 소비자 수집에는 없다는 사실을 함께 확인한다.
#[test]
fn the_consumer_corpus_holds_code_and_manifests_only() {
    let files = consumer_files();
    assert!(files.len() > 100, "말뭉치가 {} 개뿐이다", files.len());
    let bad: Vec<String> = files
        .iter()
        .filter(|p| !p.extension().is_some_and(|e| e == "rs" || e == "toml"))
        .map(|p| rel_of(p))
        .collect();
    assert!(
        bad.is_empty(),
        "코드도 매니페스트도 아닌 소비자 파일: {bad:?}"
    );

    let mut md = 0usize;
    let mut all = Vec::new();
    gather_any(root(), &mut all);
    for p in &all {
        if p.extension().is_some_and(|e| e == "md") {
            md += 1;
        }
    }
    assert!(
        md > 50,
        "Markdown 파일이 {md} 개로 부족해 확장자 제외를 대조할 수 없다"
    );
}

/// 같은 확장자 필터 오류가 두 결과에 함께 생기지 않도록 대조용 순회는 확장자를 제한하지 않는다.
fn gather_any(dir: &Path, out: &mut Vec<PathBuf>) {
    const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            gather_any(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn every_orphan_entry_names_a_catalog_key() {
    let keys = all_english_keys();
    let missing: Vec<&str> = ORPHAN_KEYS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| !keys.contains(*k))
        .collect();
    assert!(
        missing.is_empty(),
        "ORPHAN_KEYS 가 없는 키를 든다: {missing:?}"
    );
    let no_reason: Vec<&str> = ORPHAN_KEYS
        .iter()
        .filter(|(_, why)| why.trim().is_empty())
        .map(|(k, _)| *k)
        .collect();
    assert!(no_reason.is_empty(), "사유 없는 항목: {no_reason:?}");
}

#[test]
fn a_dynamically_assembled_key_is_not_called_an_orphan() {
    let mut templates = BTreeSet::new();
    templates.insert(("convert_popup.".to_string(), String::new()));
    assert!(is_dynamically_assembled("convert_popup.html", &templates));
    assert!(!is_dynamically_assembled("convert_popup.a.b", &templates));
    assert!(!is_dynamically_assembled("convert_popup.", &templates));
}

#[test]
fn a_brace_outside_the_literal_is_not_a_template() {
    let lits = string_literals(r#"probe(port, "attach.list", json!({}))"#);
    assert!(lits.contains(&"attach.list".to_string()), "got {lits:?}");
    assert!(
        lits.iter().all(|l| !l.contains('{')),
        "리터럴 밖 중괄호를 안으로 끌어들였다: {lits:?}"
    );
}

#[test]
fn assembled_namespaces_point_at_a_living_assembler() {
    let keys = all_english_keys();
    let mut problems = Vec::new();
    for (prefix, assembler) in ASSEMBLED_NAMESPACES {
        if !root().join(assembler).is_file() {
            problems.push(format!("  {prefix} — 조립하는 자리가 없다: {assembler}"));
        }
        if !keys.iter().any(|k| k.starts_with(prefix)) {
            problems.push(format!(
                "  {prefix} — 이 앞머리를 가진 카탈로그 키가 하나도 없다. 면제만 남았다"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "동적 namespace의 조립 파일이나 카탈로그 키를 찾지 못했다:\n{}",
        problems.join("\n")
    );
}
