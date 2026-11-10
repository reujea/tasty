//! docs/**/*.md에서 원격 HTTP(S) viewer를 선언하지 않는 macOS 범위(`macOS 11–N`)의 상한이
//! src/host_api/webview/macos/http.rs의 FIRST_MEASURED보다 1 작은지 대조한다.
//! 이 범위는 산문으로 여러 문서(한·영)에 복제돼 있어, 하한을 바꿀 때 일부 문장이 옛 값으로 남을 수 있다.
//! 단위는 macOS 메이저 버전이다.
//!
//! 판정 대상은 `macOS` 뒤 공백(줄바꿈 포함) 다음에 오는 `11`, 구분자, 숫자 형태뿐이다.
//! 구분자는 en dash·하이픈·물결·em dash이며 앞뒤 공백을 허용한다. 표기 방식을 바꿔 가드를 피하지 않게 한다.
//! docs/ 안의 이 형태는 모두 HTTP viewer 미지원 범위라는 전제를 둔다. 다른 사실(예: per-store proxy API가
//! 없는 버전)은 범위 표기 대신 "macOS 14 이전" 같은 형태로 쓴다. 문장 식별로 예외를 두면 줄바꿈이나
//! 문구가 바뀔 때 예외가 조용히 빠지거나 넓어지므로 예외 목록을 두지 않는다.
//! `macOS 27 이상`·`27 or later` 같은 지원 표기와 `macOS 14–26` 같은 다른 시작점의 범위는 검사하지 않는다.

use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};

/// 하한 상수가 있는 소스. 이 파일이 실물 기준이다.
const SOURCE: &str = "src/host_api/webview/macos/http.rs";

/// 하한 상수 선언의 앞부분. 값은 이 뒤부터 `;` 전까지다.
const CONST_PREFIX: &str = "const FIRST_MEASURED: isize = ";

/// 미지원 범위의 시작 버전. 앱 최소 macOS(LSMinimumSystemVersion)와 같다.
const RANGE_START: &str = "11";

/// 범위 구분자. 문서가 쓰는 en dash 외에 손으로 쓰기 쉬운 형태도 잡는다.
const SEPARATORS: [char; 4] = ['–', '-', '~', '—'];

/// 하한 규칙을 설명하려고 범위를 직접 적는 문서. 이 문서의 일치는 0건 판정에서 세지 않는다.
/// 실제 사용자·개발 문서가 모두 다른 형태로 바뀌었는데 설명 문서 하나로 통과하지 않게 한다.
const SELF_DOC: &str = "docs/dev-guide/duplicated-sets.md";

/// 문서 수집 실패로 빈 결과가 통과하지 않게 한다.
const DOCS_FLOOR: Floor = Floor {
    min: 162,
    measured: tasty_doc_guards::floored_walk::populations::DOCS_MD.measured,
    measured_on: tasty_doc_guards::floored_walk::populations::DOCS_MD.measured_on,
    counted_on: tasty_doc_guards::floored_walk::populations::DOCS_MD.counted_on,
    why_this_gap: "실측 246개보다 84개 낮다. 가장 큰 비-ADR 분류인 docs/features의 52개를 \
                   잃어도 통과하는 여유다. \
                   한 분류의 통합을 허용하는 기존 하한을 유지하지만 그보다 작은 누락은 검출하지 못한다. \
                   ADR 구성 방식이나 검사 범위를 바꾸면 모수와 하한을 함께 다시 검토한다.",
};

/// 소스에서 FIRST_MEASURED 값을 읽는다. 선언이 정확히 하나가 아니거나 숫자가 아니면 오류다.
fn first_measured(source: &str) -> Result<u32, String> {
    let values: Vec<&str> = source
        .lines()
        .filter_map(|line| line.trim().strip_prefix(CONST_PREFIX))
        .collect();
    let [value] = values.as_slice() else {
        return Err(format!(
            "{SOURCE}에서 `{CONST_PREFIX}N;` 선언을 정확히 하나 찾아야 하는데 {}개를 찾았다",
            values.len()
        ));
    };
    let digits = value
        .strip_suffix(';')
        .ok_or_else(|| format!("{SOURCE}의 FIRST_MEASURED 선언이 `;`로 끝나지 않는다: {value}"))?;
    digits
        .trim()
        .parse()
        .map_err(|e| format!("{SOURCE}의 FIRST_MEASURED 값 `{digits}`을 읽지 못했다: {e}"))
}

/// 본문에서 `macOS 11–N` 범위를 찾아 (줄 번호, 상한)을 돌려준다.
/// `macOS`와 숫자 사이의 줄바꿈도 공백으로 본다.
fn ranges(contents: &str) -> Vec<(usize, u32)> {
    let mut found = Vec::new();
    for (start, _) in contents.match_indices("macOS") {
        let rest = contents[start + "macOS".len()..].trim_start();
        let Some(rest) = rest.strip_prefix(RANGE_START) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix(SEPARATORS) else {
            continue;
        };
        let rest = rest.trim_start();
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Ok(upper) = digits.parse() else {
            continue;
        };
        let line = contents[..start].matches('\n').count() + 1;
        found.push((line, upper));
    }
    found
}

#[test]
fn macos_http_viewer_floor_matches_the_source() {
    let root = &tasty_doc_guards::repo_root();
    let source = std::fs::read_to_string(root.join(SOURCE))
        .unwrap_or_else(|e| panic!("{SOURCE}를 읽지 못했다: {e}"));
    let floor = first_measured(&source).unwrap_or_else(|why| panic!("{why}"));
    let expected = floor - 1;

    let files = walk_with_floor(
        &root.join("docs"),
        root,
        &DOCS_FLOOR,
        Descend::Everything,
        &|found| found.rel.starts_with("docs/") && found.rel.ends_with(".md"),
    )
    .unwrap_or_else(|why| panic!("{why}"));

    let mut matched = 0usize;
    let mut matched_outside_self = 0usize;
    let mut violations = Vec::new();
    for file in &files {
        let contents = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|e| panic!("{}를 읽지 못했다: {e}", file.rel));
        for (line, upper) in ranges(&contents) {
            matched += 1;
            if file.rel != SELF_DOC {
                matched_outside_self += 1;
            }
            if upper != expected {
                violations.push(format!(
                    "  {}:{line} — macOS {RANGE_START}–{upper}",
                    file.rel
                ));
            }
        }
    }

    assert!(
        matched_outside_self > 0,
        "{SELF_DOC} 밖의 docs/에서 `macOS {RANGE_START}–N` 범위를 하나도 찾지 못했다. 문서 표기가 바뀌었다면 이 가드의 판정 형태를 함께 고친다"
    );
    assert!(
        violations.is_empty(),
        "{SOURCE}의 FIRST_MEASURED는 {floor}이므로 원격 HTTP(S) viewer 미지원 범위는 macOS {RANGE_START}–{expected}이다. \
         다른 값으로 적힌 곳({matched}건 중):\n{}\n\
         다른 사실을 적는 문장이면 범위 표기 대신 \"macOS N 이전\" 형태로 쓴다(docs/dev-guide/duplicated-sets.md)",
        violations.join("\n")
    );
}

#[test]
fn the_floor_reader_and_range_matcher_react_to_planted_text() {
    assert_eq!(
        first_measured("fn f() {\n    const FIRST_MEASURED: isize = 27;\n}"),
        Ok(27)
    );
    assert!(first_measured("const LAST_MEASURED: isize = 27;").is_err());
    assert!(first_measured("const FIRST_MEASURED: isize = x;").is_err());
    assert!(
        first_measured("const FIRST_MEASURED: isize = 27;\nconst FIRST_MEASURED: isize = 28;")
            .is_err()
    );

    assert_eq!(ranges("macOS 11–26 client"), vec![(1, 26)]);
    assert_eq!(ranges("a\nmacOS 11-13의"), vec![(2, 13)]);
    assert_eq!(
        ranges("native Wayland와 macOS\n11–26 client"),
        vec![(1, 26)]
    );
    assert_eq!(ranges("macOS 11~13"), vec![(1, 13)]);
    assert_eq!(ranges("macOS 11 – 13"), vec![(1, 13)]);
    assert_eq!(ranges("macOS 11—13"), vec![(1, 13)]);
    assert_eq!(ranges("macOS 11 ~ 26"), vec![(1, 26)]);
    assert!(ranges("macOS 14–26 have").is_empty());
    assert!(ranges("macOS 11 – 기타").is_empty());
    assert!(ranges("macOS 11은 바뀌지 않는다").is_empty());
    assert!(ranges("macOS 27 이상").is_empty());
    assert!(ranges("macOS 11-compatible").is_empty());
}
