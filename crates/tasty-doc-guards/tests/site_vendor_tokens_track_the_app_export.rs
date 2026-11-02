//! 앱의 DTCG JSON과 사이트의 CSS 토큰 이름을 티어별로 비교한다.
//! 앱에만 있는 이름은 LAGGING의 기록과 같아야 하고 사이트에만 있는 이름은 없어야 한다.
//! 두 사본은 저장소 안에서 읽으며 원격 디자인은 조회하지 않는다.
//!
//! 토큰 값이나 문구·레이아웃·컨트롤의 추가·삭제는 검사하지 않는다.
//! 통과는 이름 차이가 기록과 같다는 뜻이며 사이트가 최신이라는 뜻은 아니다.
//! 원격 원본과의 대조는 site/vendor/README.md와 design-change-workflow의 갱신 절차를 따른다.
//!
//! CSS에서는 소문자·숫자·하이픈 이름만 읽는다. 다른 표기가 추가되면 파서가 놓친 이름을
//! 실제 누락으로 오인할 수 있으므로 파싱 형식부터 확인한다. 아이콘은 별도 검사에서 대조한다.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// 앱 DTCG 의 최상위 티어 이름과, 그 티어에 대응하는 사이트 CSS 파일.
const TIERS: [(&str, &str); 3] = [
    ("primitive", "site/vendor/tokens/primitives.css"),
    ("semantic", "site/vendor/tokens/semantic.css"),
    ("component", "site/vendor/tokens/components.css"),
];

/// 사이트에 반영하지 못한 토큰을 이름별로 기록한다. 개수만 같고 이름이 달라져도 실패한다.
/// 갱신된 토큰은 목록에서 제거한다. 반영을 미루며 항목을 추가하려면 커밋 메시지에 사유를 적는다.
const LAGGING: &[(&str, &str)] = &[];

/// 빈 결과끼리 비교해 통과하지 않도록 토큰 수 하한을 둔다.
/// 앱 818·사이트 791개를 측정했을 때 수집 실패를 찾을 수 있도록 그보다 낮게 정한 값이다.
const PARSE_FLOOR: usize = 400;

/// 두 사본에서 알려진 토큰 하나를 실제로 읽었는지도 확인한다.
const PRESENT_ON_BOTH: (&str, &str) = ("primitive", "color-neutral-0");

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()))
}

/// JSON 깊이 1을 티어, 깊이 2를 토큰으로 읽는다. $로 시작하는 메타 키는 제외한다.
fn tier_keys(json: &str, tier: &str) -> BTreeSet<String> {
    let bytes = json.as_bytes();
    let mut keys = BTreeSet::new();
    let mut path: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' => {
                depth += 1;
                path.resize(depth + 1, String::new());
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                path.truncate(depth + 1);
                i += 1;
            }
            b'"' => {
                let (text, next) = scan_string(bytes, i);
                let mut j = next;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    if depth < path.len() {
                        path[depth] = text.clone();
                    }
                    if depth == 2
                        && path.get(1).map(String::as_str) == Some(tier)
                        && !text.starts_with('$')
                    {
                        keys.insert(text);
                    }
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    keys
}

/// `i` 가 여는 따옴표일 때 그 문자열의 내용과 **닫는 따옴표 다음 위치**를 돌려준다.
fn scan_string(bytes: &[u8], i: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => {
                // 이스케이프의 값은 해석하지 않고 닫는 따옴표 위치만 구분한다.
                out.push(bytes[j]);
                if j + 1 < bytes.len() {
                    out.push(bytes[j + 1]);
                }
                j += 2;
            }
            b'"' => return (String::from_utf8_lossy(&out).into_owned(), j + 1),
            c => {
                out.push(c);
                j += 1;
            }
        }
    }
    (String::from_utf8_lossy(&out).into_owned(), j)
}

/// 이름 뒤에 콜론이 있는 CSS 선언만 모은다. var 참조는 제외하고 중복 선언은 합친다.
fn css_names(css: &str) -> BTreeSet<String> {
    const PREFIX: &str = "--tasty-";
    let mut names = BTreeSet::new();
    let bytes = css.as_bytes();
    let mut from = 0usize;
    while let Some(off) = css[from..].find(PREFIX) {
        let start = from + off + PREFIX.len();
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_lowercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'-')
        {
            end += 1;
        }
        let mut j = end;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if end > start && j < bytes.len() && bytes[j] == b':' {
            names.insert(css[start..end].to_string());
        }
        from = start;
    }
    names
}

#[test]
fn both_copies_are_actually_parsed() {
    let json = read("crates/tasty-design-tokens/dtcg/tasty.tokens.json");
    let mut app_total = 0usize;
    let mut site_total = 0usize;
    for (tier, css_path) in TIERS {
        let app = tier_keys(&json, tier);
        let site = css_names(&read(css_path));
        assert!(
            !app.is_empty(),
            "앱 DTCG의 {tier}에서 토큰 키를 읽지 못했다. JSON 구조와 파서를 확인한다."
        );
        assert!(
            !site.is_empty(),
            "사이트 CSS {css_path}에서 토큰 선언을 읽지 못했다. CSS 구조와 파서를 확인한다."
        );
        app_total += app.len();
        site_total += site.len();
    }
    assert!(
        app_total >= PARSE_FLOOR && site_total >= PARSE_FLOOR,
        "읽은 토큰이 앱 {app_total} · 사이트 {site_total} 로 하한 {PARSE_FLOOR} 아래다 — \
         파서가 일부만 읽고 있다"
    );

    // 개수만으로는 엉뚱한 항목을 읽는 오류를 찾지 못하므로 알려진 이름도 확인한다.
    let (tier, name) = PRESENT_ON_BOTH;
    let css_path = TIERS
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, p)| *p)
        .expect("PRESENT_ON_BOTH 의 티어는 TIERS 에 있어야 한다");
    assert!(
        tier_keys(&json, tier).contains(name),
        "앱 DTCG의 {tier}에서 알려진 토큰 {name}을 찾지 못했다"
    );
    assert!(
        css_names(&read(css_path)).contains(name),
        "사이트 `{css_path}` 에서 `--tasty-{name}` 을 못 찾았다 — 같은 이유다"
    );
}

/// 등록한 티어가 비교에서 빠지지 않도록 존재를 확인한다.
#[test]
fn every_ledger_entry_names_a_real_tier() {
    for (tier, name) in LAGGING {
        assert!(
            TIERS.iter().any(|(t, _)| t == tier),
            "명부 항목 `{tier}` / `{name}` 의 티어 이름이 `{:?}` 중에 없다 — 오타면 그 항목은 \
             아무 비교에도 안 들어간다",
            TIERS.map(|(t, _)| t)
        );
    }
}

#[test]
fn the_site_copy_lags_the_app_export_by_exactly_the_recorded_set() {
    let json = read("crates/tasty-design-tokens/dtcg/tasty.tokens.json");
    let mut problems: Vec<String> = Vec::new();

    for (tier, css_path) in TIERS {
        let app = tier_keys(&json, tier);
        let site = css_names(&read(css_path));

        let missing: BTreeSet<&str> = app
            .iter()
            .filter(|k| !site.contains(*k))
            .map(String::as_str)
            .collect();
        let recorded: BTreeSet<&str> = LAGGING
            .iter()
            .filter(|(t, _)| *t == tier)
            .map(|(_, n)| *n)
            .collect();

        let newly_behind: Vec<&&str> = missing.difference(&recorded).collect();
        if !newly_behind.is_empty() {
            problems.push(format!(
                "[{tier}] 등록되지 않은 사이트 누락 토큰이 {}종이다: {newly_behind:?}. 파싱 형식을 확인하고 site/vendor/README.md의 절차로 사본을 갱신한다. 반영을 미룬다면 해당 이름과 커밋 사유를 남긴다.",
                newly_behind.len()
            ));
        }

        let caught_up: Vec<&&str> = recorded.difference(&missing).collect();
        if !caught_up.is_empty() {
            problems.push(format!(
                "[{tier}] 이미 반영된 토큰 {}종이 누락 목록에 남았다: {caught_up:?}. 해당 항목을 제거한다.",
                caught_up.len()
            ));
        }

        let site_only: Vec<&str> = site
            .iter()
            .filter(|k| !app.contains(*k))
            .map(String::as_str)
            .collect();
        if !site_only.is_empty() {
            problems.push(format!(
                "[{tier}] 사이트에만 있는 토큰이 {}종이다: {site_only:?}. {css_path}의 수동 수정인지 앱 사본이 뒤처진 것인지 원격 원본과 대조한다. 사이트 vendor 파일은 갱신 절차로 받아야 한다.",
                site_only.len()
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "사이트와 앱의 토큰 이름 차이가 기록과 다르다:\n\n{}",
        problems.join("\n\n")
    );
}
