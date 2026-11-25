//! 본체 `lang/ko.toml` 값이 채워 넣는 이름에 맞지 않는 조사를 쓰지 않는지 확인한다.
//! 조사 병기(`을(를)` 등)와, 자리표시자 바로 뒤에 앞말에 따라 형태가 바뀌는 조사를 찾는다.
//! 닫는 괄호 뒤 조사(`플러그인({})이`)는 괄호 앞 낱말에 붙으므로 대상이 아니다.

use std::collections::{BTreeMap, HashMap};

/// 두 형태를 괄호로 함께 적은 조사. 이 꼴은 문맥 없이 병기로 본다.
const PAIRED: &[&str] = &[
    "을(를)", "를(을)", "은(는)", "는(은)", "이(가)", "가(이)", "와(과)", "과(와)",
];

/// 받침에 따라 붙는 앞 글자를 괄호로 적은 병기와, 그 뒤에 이어져 조사를 이루는 어미.
/// 앞이 한글 음절이나 자리표시자이고 뒤가 이 어미일 때만 병기다(`보고서(이).txt` 는 아니다).
const BRACKETED: &[(&str, &[&str])] = &[
    ("(으)", &["로", "면", "며", "니", "세요"]),
    (
        "(이)",
        &[
            "나", "며", "고", "라는", "라고", "란", "라도", "든", "랑", "여", "다", "에요", "면",
        ],
    ),
    ("(아)", &[""]),
    ("(야)", &[""]),
];

/// 형태가 바뀌는 조사 뒤에 이어 붙는 보조사(`{}로는`, `{}와도`, `{}이나`, `{}로만`).
/// `{}가지`·`{}개` 처럼 조사가 아닌 낱말은 이 목록에 걸리지 않는다.
const AUXILIARY: &[&str] = &["라도", "부터", "까지", "는", "도", "만", "나", "든"];

/// 앞말의 받침에 따라 형태가 바뀌는 조사. 긴 것부터 맞춘다.
const VARYING: &[&str] = &[
    "으로부터",
    "로부터",
    "으로서",
    "로서",
    "으로써",
    "로써",
    "으로의",
    "로의",
    "으로",
    "로",
    "이라는",
    "라는",
    "와의",
    "과의",
    "을",
    "를",
    "은",
    "는",
    "이",
    "가",
    "와",
    "과",
];

/// 자리표시자 뒤, 조사 앞에 올 수 있는 닫는 따옴표·백틱.
const CLOSERS: &[char] = &['\'', '"', '”', '’', '`'];

fn ko_values() -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lang/ko.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let mut flat = HashMap::new();
    tasty_i18n::flatten_catalog_toml("", &value, &mut flat);
    flat.into_iter().collect()
}

fn is_hangul(c: char) -> bool {
    ('가'..='힣').contains(&c)
}

/// `{}`·`{name}`·`{0}` 만 자리표시자로 본다. JSON 예시 같은 다른 중괄호는 건너뛴다.
fn placeholder_spans(value: &str) -> Vec<(usize, usize)> {
    let mut ends = Vec::new();
    for (start, _) in value.match_indices('{') {
        let rest = &value[start + 1..];
        let Some(close) = rest.find('}') else {
            continue;
        };
        let inner = &rest[..close];
        let ident = inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if ident {
            ends.push((start, start + 1 + close + 1));
        }
    }
    ends
}

/// 조사 바로 뒤가 낱말의 끝이면 0, 보조사 하나가 이어지고 끝나면 그 보조사 길이를 돌려준다.
fn particle_ends(after: &str) -> Option<usize> {
    let ends = |s: &str| !s.chars().next().is_some_and(is_hangul);
    if ends(after) {
        return Some(0);
    }
    AUXILIARY
        .iter()
        .find(|aux| after.strip_prefix(**aux).is_some_and(ends))
        .map(|aux| aux.len())
}

/// 값에 든 조사 병기를 돌려준다.
fn paired_particles(value: &str) -> Vec<String> {
    let mut found: Vec<String> = PAIRED
        .iter()
        .filter(|p| value.contains(**p))
        .map(|p| p.to_string())
        .collect();
    for (bracket, endings) in BRACKETED {
        for (at, _) in value.match_indices(bracket) {
            let before = value[..at].chars().next_back();
            let after = &value[at + bracket.len()..];
            let attached = before.is_some_and(|c| is_hangul(c) || c == '}');
            // 빈 어미는 괄호 자체가 조사다(`{}(아)`). 그때는 뒤가 낱말의 끝이어야 한다.
            let ending = endings.iter().find(|e| match after.strip_prefix(**e) {
                Some(rest) if e.is_empty() => !rest.chars().next().is_some_and(is_hangul),
                Some(_) => true,
                None => false,
            });
            if let (true, Some(e)) = (attached, ending) {
                found.push(format!("{bracket}{e}"));
            }
        }
    }
    found
}

/// 자리표시자 바로 뒤(닫는 따옴표 하나는 건너뜀)에 붙은 형태가 바뀌는 조사를 돌려준다.
fn varying_after_placeholder(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (start, end) in placeholder_spans(value) {
        let mut rest = &value[end..];
        if let Some(c) = rest.chars().next().filter(|c| CLOSERS.contains(c)) {
            rest = &rest[c.len_utf8()..];
        }
        let hit = VARYING.iter().find_map(|p| {
            let after = rest.strip_prefix(*p)?;
            particle_ends(after).map(|aux| p.len() + aux)
        });
        if let Some(len) = hit {
            let tail = value.len() - rest.len() + len;
            found.push(value[start..tail].to_string());
        }
    }
    found
}

#[test]
fn ko_values_carry_no_paired_particle() {
    let bad: Vec<String> = ko_values()
        .into_iter()
        .filter(|(_, v)| !paired_particles(v).is_empty())
        .map(|(k, v)| format!("{k} = {v:?}"))
        .collect();
    assert!(
        bad.is_empty(),
        "조사 병기가 남은 ko 값 — 자리표시자 뒤에 명사를 두고 그 명사에 조사를 붙인다:\n{}",
        bad.join("\n")
    );
}

#[test]
fn ko_values_put_no_varying_particle_on_a_placeholder() {
    let values = ko_values();
    assert!(
        values.len() > 1000,
        "ko 카탈로그가 비었다: {}",
        values.len()
    );
    let bad: Vec<String> = values
        .iter()
        .flat_map(|(k, v)| {
            varying_after_placeholder(v)
                .into_iter()
                .map(move |hit| format!("{k}: {hit:?} in {v:?}"))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "자리표시자에 바로 붙은 형태가 바뀌는 조사 — 채우는 값의 받침에 따라 틀린다:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_scanner_sees_each_form() {
    assert_eq!(varying_after_placeholder("{}를 닫습니다"), ["{}를"]);
    assert_eq!(varying_after_placeholder("'{}'을 등록"), ["{}'을"]);
    assert_eq!(varying_after_placeholder("{name}은 폴더"), ["{name}은"]);
    assert_eq!(
        varying_after_placeholder("--webhook-port {}로 실행"),
        ["{}로"]
    );
    assert_eq!(varying_after_placeholder("“{}”와 일치"), ["{}”와"]);
    // 조사가 아닌 낱말의 첫 글자, 닫는 괄호 뒤, 서술격 조사는 대상이 아니다.
    assert!(varying_after_placeholder("{}가지 방법").is_empty());
    assert!(varying_after_placeholder("플러그인({})이 없습니다").is_empty());
    assert!(varying_after_placeholder("{}이며 {}입니다").is_empty());
    assert!(varying_after_placeholder("{\"a\": 1}`를 받습니다").is_empty());
    assert!(varying_after_placeholder("{} 항목을 제거").is_empty());
    // 조사 뒤에 보조사가 이어지는 꼴.
    assert_eq!(varying_after_placeholder("{}로는 실행"), ["{}로는"]);
    assert_eq!(varying_after_placeholder("{}와도 같습니다"), ["{}와도"]);
    assert_eq!(varying_after_placeholder("{}이나 {}"), ["{}이나"]);
    assert_eq!(varying_after_placeholder("{}로만 엽니다"), ["{}로만"]);
    assert!(varying_after_placeholder("{}개 항목").is_empty());
}

#[test]
fn the_paired_scanner_needs_a_particle_context() {
    assert_eq!(paired_particles("{}을(를) 제거"), ["을(를)"]);
    assert_eq!(paired_particles("항목이(가) 있습니다"), ["이(가)"]);
    assert_eq!(paired_particles("{}(으)로 이동"), ["(으)로"]);
    assert_eq!(paired_particles("{}(이)나 {}"), ["(이)나"]);
    // 괄호가 조사를 이루지 않는 보통 글.
    assert!(paired_particles("파일 이름 보고서(이).txt").is_empty());
    assert!(paired_particles("조합 중인 음절(아)을 입력").is_empty());
}
