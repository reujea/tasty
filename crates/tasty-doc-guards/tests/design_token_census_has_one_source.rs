//! tasty-design-tokens의 토큰 수 설명을 freshness.rs의 token_census_matches_design_export 기준값과 대조한다.
//! 크레이트 안의 추적 Rust·Markdown 파일에서 census라는 말이나 현재 총 토큰 수 문구가 나오면
//! CLAIMS 또는 사유를 적은 EXCLUDED에 등록하도록 한다. 일반적인 산문 의미를 판독하는 검사는 아니다.
//! doc-guards.yml이 경로 필터 없이 develop·release·main push·PR에서 실행한다.

use std::path::PathBuf;

const CANONICAL: &str = "crates/tasty-design-tokens/tests/freshness.rs";
const CRATE_DIR: &str = "crates/tasty-design-tokens";

/// 숫자 바로 앞 문구로 설명 위치를 찾는다. 문구가 바뀌면 함께 갱신해야 한다.
const CLAIMS: &[(&str, &str)] = &[
    ("crates/tasty-design-tokens/README.md", "토큰 census("),
    ("crates/tasty-design-tokens/src/lib.rs", "component, 총 "),
];

/// 등록 검사에서 제외하는 파일과 근거.
const EXCLUDED: &[(&str, &str)] = &[(
    CANONICAL,
    "검사의 기준값을 직접 선언하는 파일이다. 문서에 복제한 설명 수치와 구분한다.",
)];

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()))
}

/// `at` 부터 이어지는 십진 숫자를 읽는다. 없으면 `None`.
fn number_at(s: &str, at: usize) -> Option<(u32, usize)> {
    let b = s.as_bytes();
    let mut end = at;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if end == at {
        return None;
    }
    Some((s[at..end].parse().ok()?, end))
}

/// 정본 — `token_census_matches_design_export` 가 박은 (primitive, semantic, component, 합).
fn canonical_census() -> (u32, u32, u32, u32) {
    let src = read(CANONICAL);
    let at = src
        .find("fn token_census_matches_design_export()")
        .unwrap_or_else(|| panic!("{CANONICAL}: census 시험 함수를 못 찾았다"));
    let body = &src[at..];
    let end = body
        .find("\n}")
        .unwrap_or_else(|| panic!("{CANONICAL}: census 시험 함수의 끝을 못 찾았다"));
    let body = &body[..end];

    let tier = |needle: &str| -> u32 {
        let hit = body.find(needle).unwrap_or_else(|| {
            panic!(
                "{CANONICAL}: `{needle}` 을 못 찾았다 — 정본의 형태가 \
                                       바뀌었으면 이 판정기를 함께 고쳐라"
            )
        });
        let rest = &body[hit + needle.len()..];
        let digit = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or_else(|| panic!("{CANONICAL}: `{needle}` 뒤에 수가 없다"));
        number_at(rest, digit)
            .unwrap_or_else(|| panic!("{CANONICAL}: `{needle}` 뒤의 수를 못 읽었다"))
            .0
    };
    let p = tier("dtcg::Tier::Primitive)");
    let s = tier("dtcg::Tier::Semantic)");
    let c = tier("dtcg::Tier::Component)");
    let total = tier("set.len(),");
    (p, s, c, total)
}

#[test]
fn the_canonical_census_is_actually_read() {
    let (p, s, c, total) = canonical_census();
    for (what, n) in [
        ("primitive", p),
        ("semantic", s),
        ("component", c),
        ("합계", total),
    ] {
        assert!(
            n > 0,
            "기준 파일에서 {what} 수를 {n}으로 읽었다. 집합 비교 전에 숫자 추출을 확인한다."
        );
    }
    assert_eq!(
        p + s + c,
        total,
        "정본 자신의 티어 합({p}+{s}+{c})이 합계({total})와 다르다 — 정본이 먼저 어긋났다"
    );
}

#[test]
fn every_prose_copy_states_the_canonical_census() {
    let (p, s, c, total) = canonical_census();
    let mut problems: Vec<String> = Vec::new();

    for (path, needle) in CLAIMS {
        let src = read(path);
        let Some(hit) = src.find(needle) else {
            problems.push(format!(
                "{path}: 숫자 앞 문구 {needle}가 없다. 설명을 바꿨다면 CLAIMS도 갱신한다."
            ));
            continue;
        };
        let rest = &src[hit + needle.len()..];
        let Some((claimed, after)) = number_at(rest, 0) else {
            problems.push(format!("{path}: `{needle}` 바로 뒤에 수가 없다"));
            continue;
        };
        if claimed != total {
            problems.push(format!(
                "{path}: 합계를 {claimed} 로 적는데 정본은 {total} 이다"
            ));
        }
        // 총합 뒤에 티어별 a/b/c가 있으면 함께 대조한다.
        let tail = &rest[after..];
        if let Some(eq) = tail.find('=')
            && tail[..eq].trim().is_empty()
        {
            let triple = tail[eq + 1..].trim_start();
            let got: Vec<u32> = triple
                .split('/')
                .take(3)
                .filter_map(|part| number_at(part.trim_start(), 0).map(|(n, _)| n))
                .collect();
            if got != vec![p, s, c] {
                problems.push(format!(
                    "{path}: 티어 내역을 {got:?} 로 적는데 정본은 [{p}, {s}, {c}] 다"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "토큰 수 설명이 {CANONICAL}의 기준값과 다르다. 디자인 원본을 확인해 기준값과 설명을 맞춘다:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn no_unregistered_census_phrase_lives_in_the_crate() {
    let registered: Vec<&str> = CLAIMS.iter().map(|(p, _)| *p).collect();
    let excluded: Vec<&str> = EXCLUDED.iter().map(|(p, _)| *p).collect();
    for (path, why) in EXCLUDED {
        assert!(!why.trim().is_empty(), "{path} 의 제외에 사유가 없다");
        assert!(
            repo_root().join(path).exists(),
            "{path} 가 없다. 제외 명부에서 이동·삭제 여부를 확인한다."
        );
    }

    let (_, _, _, total) = canonical_census();
    let needle = format!("총 {total} 토큰");
    let mut unregistered: Vec<String> = Vec::new();

    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files", CRATE_DIR])
        .output()
        .expect("git ls-files 를 못 돌렸다");
    assert!(out.status.success(), "git ls-files 가 실패했다");
    let listed = String::from_utf8_lossy(&out.stdout);
    let files: Vec<&str> = listed.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        files.len() >= 5,
        "{CRATE_DIR}에서 추적 파일을 {}개만 읽었다. 수집 범위를 확인한다.",
        files.len()
    );

    for rel in files {
        if registered.contains(&rel) || excluded.contains(&rel) {
            continue;
        }
        if !(rel.ends_with(".rs") || rel.ends_with(".md")) {
            continue;
        }
        let src = read(rel);
        if src.contains("census") || src.contains(&needle) {
            unregistered.push(rel.to_string());
        }
    }

    assert!(
        unregistered.is_empty(),
        "토큰 수 설명이 미등록 파일에 있다: {unregistered:?}. 수치 주장이면 CLAIMS에, 다른 용도이면 EXCLUDED에 사유와 함께 등록한다."
    );
}
