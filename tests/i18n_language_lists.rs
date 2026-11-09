//! Rust 번역 로더를 쓸 수 없는 곳에 손으로 적은 언어 목록을 내장 언어 정본과 대조한다.
//! Claude mod의 TypeScript는 Rust 번역기를 호출할 수 없고 없는 언어는 영어로 돌아간다.
//! 문구가 있는지만 보며 번역이 맞는지는 판단하지 않는다.

use std::collections::BTreeSet;
use std::path::Path;

const MOD_DIR: &str = "crates/tasty-plugin-claude/src/notify_mod/claude";

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn builtin() -> BTreeSet<String> {
    tasty_i18n::BUILTIN_CODES
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

/// `opening`이 여는 객체 리터럴의 키를 읽는다. 한 줄에 키 하나인 형태만 받고, 다른 형태는 누락하지 않고 실패한다.
fn object_keys(source: &str, opening: &str) -> BTreeSet<String> {
    let start = source
        .find(opening)
        .unwrap_or_else(|| panic!("`{opening}`를 찾지 못했다"));
    let body = &source[start + opening.len()..];
    let end = body
        .find("};")
        .unwrap_or_else(|| panic!("`{opening}`의 끝을 찾지 못했다"));
    body[..end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (key, _) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("`{opening}`에서 읽지 못한 줄: {line}"));
            key.trim_matches(['\'', '"']).to_string()
        })
        .collect()
}

/// 표제 시험이 언어별 문구를 기대값으로 둔 언어. `unknown`은 영어로 돌아가는지 보는 입력이다.
fn expected_heading_codes(test_source: &str) -> BTreeSet<String> {
    const CALL: &str = "expect(heading('";
    test_source
        .match_indices(CALL)
        .filter_map(|(at, _)| {
            let rest = &test_source[at + CALL.len()..];
            let (code, after) = rest.split_once("')")?;
            after.starts_with(").toContain(").then(|| code.to_string())
        })
        .collect()
}

fn assert_same(file: &str, what: &str, found: &BTreeSet<String>) {
    let want = builtin();
    assert_eq!(
        found,
        &want,
        "{file}의 {what}가 BUILTIN_CODES와 다르다. 빠진 언어는 영어로 조용히 나온다.\n빠진 언어: {:?}\n남는 언어: {:?}",
        want.difference(found).collect::<Vec<_>>(),
        found.difference(&want).collect::<Vec<_>>()
    );
}

#[test]
fn notify_heading_covers_every_builtin_language() {
    let file = format!("{MOD_DIR}/hooks/messages.ts");
    let keys = object_keys(&read(&file), "const messages = {");
    assert_same(&file, "messages", &keys);
}

#[test]
fn notify_gate_reason_covers_every_builtin_language() {
    let file = format!("{MOD_DIR}/gate/hooks/register.ts");
    let keys = object_keys(&read(&file), "const duplicates: Record<string, string> = {");
    assert_same(&file, "duplicates", &keys);
}

#[test]
fn notify_heading_test_expects_every_builtin_language() {
    let file = format!("{MOD_DIR}/tests/notify.test.ts");
    assert_same(
        &file,
        "heading 기대값",
        &expected_heading_codes(&read(&file)),
    );
}

#[test]
fn the_readers_take_the_named_object_and_the_expected_calls_only() {
    let source = "const other = {\n  xx: 'no',\n};\nconst messages = {\n  en: 'a: b',\n  'zh-Hans': \"c\",\n};\nconst after = {\n  yy: 'no',\n};\n";
    assert_eq!(
        object_keys(source, "const messages = {"),
        BTreeSet::from(["en".to_string(), "zh-Hans".to_string()])
    );
    let test = "expect(heading('en')).toContain('x');\nexpect(heading('unknown')).toBe(heading('en'));\nexpect(heading('zh-Hans')).toContain('y');\n";
    assert_eq!(
        expected_heading_codes(test),
        BTreeSet::from(["en".to_string(), "zh-Hans".to_string()])
    );
}

#[test]
#[should_panic(expected = "읽지 못한 줄")]
fn a_line_without_a_key_fails_instead_of_being_skipped() {
    object_keys("const m = {\n  ...base\n};\n", "const m = {");
}
