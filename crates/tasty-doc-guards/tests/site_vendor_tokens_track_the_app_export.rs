//! 레포에는 같은 원격 canonical 의 vendored 사본이 **둘** 있다. 이 타깃은 그 둘의
//! **토큰 이름 집합**을 대조한다 — 앱 쪽 `crates/tasty-design-tokens/dtcg/tasty.tokens.json`
//! 과 사이트 쪽 `site/vendor/tokens/{primitives,semantic,components}.css`.
//!
//! ## 왜 필요한가 — 한쪽에만 절차와 판정기가 있었다
//!
//! 앱 사본에는 갱신 절차(`crates/tasty-design-tokens/README.md`)와 census 스냅샷
//! (`crates/tasty-design-tokens/tests/freshness.rs`)이 있어, 디자인 결정이 착지하면
//! 따라가고 안 따라가면 빨개진다. 사이트 사본에는 **둘 다 없었다.** 그래서 앱 쪽이
//! 새 결정을 받아 토큰이 26 종 늘어난 뒤에도 사이트 사본은 그대로였고, 빌드도 시험도
//! CI 도 전부 초록이었다 — 공개 사이트가 결정 이전 UI 를 현재형으로 전시하는 동안.
//!
//! 이 타깃이 그 침묵을 없앤다. 두 사본이 **둘 다 레포 안**이라 원격 접근이 필요 없다.
//!
//! ## 무엇을 재는가
//!
//! 티어별로 두 방향을 따로 본다.
//!
//! - **사이트 결손** (앱에 있고 사이트에 없는 이름) — [`LAGGING`] 명부와 **집합으로 같아야**
//!   한다. 수가 아니라 이름이다: 명부 크기가 유지되면서 **내용이 바뀌는** 것도 잡는다.
//! - **사이트에만 있는 이름** — **0** 이어야 한다. 사이트가 앞서갈 길은 없다(사본은 원격에서
//!   내려오기만 한다). 0 이 아니면 그 사본을 손으로 고친 것이고, 그것은
//!   `site/vendor/README.md` 가 금지한 방향이다.
//!
//! ## ★ 이 타깃이 **안 보는 것** — 절차가 맡는 층
//!
//! **이 등식은 토큰 층만 본다.** 새 토큰을 열지 않는 디자인 결정 — 문구 변경, 구성 변경,
//! **컨트롤 삭제** — 은 두 사본의 토큰 이름 집합을 똑같이 남겨두므로 **여전히 안 잡힌다.**
//! 실물이 있었다: 제거하기로 결정된 체크박스 한 줄이 사이트 사본에 남아 공개 페이지로
//! 나갔는데, 그 결정은 토큰을 하나도 안 건드렸다.
//!
//! 그 층을 재려면 원격 canonical 을 파일 단위로 회수해 대조해야 하고, 그것은 판정기가 아니라
//! **사람이 도는 단계**가 맡는다 — `site/vendor/README.md` 의 "vendor 갱신 절차" 와
//! `docs/dev-guide/design-change-workflow.md` 의 정합 단계다.
//!
//! **이웃이 하나 있다.** `site_vendor_icons_match_the_app_transcription.rs` 가 같은 사본의
//! **아이콘 기하와 그 그릇**(`viewBox` · 선 굵기 · cap/join)을 같은 방식으로 본다. 둘을 합쳐도 위 문단은 그대로다 — 토큰도 아이콘도
//! 안 건드리는 결정은 여전히 두 판정기 모두의 좌변 밖이다.
//!
//! **이 두 범위를 갈라서 읽어라.** 이 타깃이 초록이라는 것은 "사이트 사본이 최신" 이 아니라
//! **"토큰 이름 집합의 차이가 기록된 그대로"** 라는 뜻뿐이다. 그 이상을 읽으면 "가드가 있으니
//! 괜찮다" 가 되고, 그때 이 파일은 보호가 아니라 알리바이가 된다.
//!
//! ## 오차 방향
//!
//! **놓치는 쪽으로 틀린다.** 토큰의 **값**은 안 본다(이름만 본다) — 같은 이름에 다른 값이
//! 실려도 초록이다. 값 층은 앱 쪽 `sizing_parity`·`color_drift` 가 앱 사본에 대해서만 본다.
//! 사이트 CSS 의 문법 오류로 선언이 안 읽히면 그 이름은 "사이트 결손" 으로 잡히므로 빨강이다.
//!
//! 이름을 읽는 규약도 한 방향으로만 틀린다. CSS 쪽은 `--tasty-` 뒤에 **소문자·숫자·하이픈**이
//! 오는 선언만 센다 — 지금 두 사본 어디에도 대문자가 든 토큰 이름이 없어서다. 원격이 그 규약을
//! 깨고 대문자를 들이면 그 선언은 **안 세어지고**, 앱에는 있으니 "사이트 결손" 으로 잡혀 빨강이
//! 된다. 즉 그 경우에도 놓치는 것이 아니라 **없는 결손을 보고한다** — 처방은 명부에 더하는 것이
//! 아니라 여기 문자 집합을 넓히는 것이다.
//!
//! ## 자동 채널
//!
//! `doc-guards.yml` 이 main push · PR 마다 이 크레이트를 돌리고(경로 필터 없음),
//! `check-headless` 의 전체 스위트에서도 돈다(`docs/dev-guide/ci-gates.md`).

use std::collections::BTreeSet;
use std::path::PathBuf;

/// 앱 DTCG 의 최상위 티어 이름과, 그 티어에 대응하는 사이트 CSS 파일.
const TIERS: [(&str, &str); 3] = [
    ("primitive", "site/vendor/tokens/primitives.css"),
    ("semantic", "site/vendor/tokens/semantic.css"),
    ("component", "site/vendor/tokens/components.css"),
];

/// **사이트 사본이 아직 안 받은 토큰의 명부 — 부채 대장이다. 목표는 비는 것이다.**
///
/// 수가 아니라 이름으로 적는다. 수만 고정하면 한 종이 들어오고 다른 한 종이 빠지는 교체가
/// 조용히 통과한다 — 그것도 "사이트가 안 따라왔다" 의 한 형태다.
///
/// **여유 0 의 양방향 래칫이다.** 늘면 새 결정이 사이트를 건너뛴 것이고, 줄면 재-vendoring 이
/// 일어난 것이니 그만큼 여기서 지워야 한다. 비면 이 명부를 지우고 판정을 그냥 집합 동등으로
/// 바꾼다 — 그때가 이 파일이 원래 되려던 모습이다.
///
/// 항목을 **더해서** 초록을 만드는 것은 마지막 수단이다. 그 편집은 "이번 결정을 사이트에
/// 안 반영하기로 했다" 는 선언이고, 커밋 메시지에 그 사유가 있어야 한다.
const LAGGING: &[(&str, &str)] = &[
    ("primitive", "font-size-30"),
    ("primitive", "opacity-tint-border"),
    ("primitive", "opacity-tint-fill"),
    ("primitive", "size-6"),
    ("primitive", "size-64"),
    ("primitive", "size-96"),
    ("semantic", "accent-decorative"),
    ("semantic", "border-frame"),
    ("semantic", "font-size-brand-display"),
    ("semantic", "glyph-dim"),
    ("semantic", "tint-border-alpha"),
    ("semantic", "tint-fill-alpha"),
    ("component", "fh-when-width"),
    ("component", "fp-bar-hysteresis"),
    ("component", "fp-crumb-current-min-width"),
    ("component", "fp-crumb-max-width"),
    ("component", "fp-crumb-menu-max-width"),
    ("component", "fp-crumb-menu-min-width"),
    ("component", "fp-crumb-min-width"),
    ("component", "fp-popup-min-width"),
    ("component", "git-toolbar-height"),
    ("component", "plugins-header-glyph"),
    ("component", "port-process-col-min-width"),
    ("component", "status-dot-size-compact"),
    ("component", "statusbar-dot-size"),
    ("component", "statusbar-glyph"),
    ("component", "statusbar-glyph-size"),
    ("component", "statusbar-theme-glyph"),
];

/// 두 사본이 실제로 읽혔는지 보는 하한. 파서가 죽으면 양쪽이 모두 빈 집합이 되어
/// "차이 없음" 으로 조용히 통과한다 — 그 상태는 초록이 아니라 측정 실패다.
/// 값은 현재 규모(앱 818 · 사이트 791)보다 한참 아래로 잡는다. 이 수는 판정이 아니라
/// **판정이 성립하는지**를 묻는 자리라 정확할 필요가 없다.
const PARSE_FLOOR: usize = 400;

/// 두 사본에 **분명히 있는** 토큰. 술어가 죽었을 때 위 하한만으로는 못 가르는 경우
/// (한쪽만 읽히는 경우)를 이것이 가른다.
const PRESENT_ON_BOTH: (&str, &str) = ("primitive", "color-neutral-0");

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 못 읽었다: {e}", path.display()))
}

/// DTCG json 에서 한 티어의 **직속 자식 키**를 모은다. `$` 로 시작하는 메타 키는 뺀다.
///
/// 의존 0 이 이 크레이트의 존재 이유라(ADR-0138) json 라이브러리를 못 쓴다. 대신 깊이를
/// 세는 최소 스캐너를 쓴다 — 문자열 안과 밖을 가르고(이스케이프 포함), `{`·`[` 로 깊이를
/// 올린다. 키는 객체 안에서만 나오므로, 깊이 1 의 키가 티어이고 깊이 2 의 키가 그 티어의
/// 토큰이다.
fn tier_keys(json: &str, tier: &str) -> BTreeSet<String> {
    let bytes = json.as_bytes();
    let mut keys = BTreeSet::new();
    // 깊이별로 "마지막에 읽은 키". 깊이 1 의 값이 지금 어느 티어 안인지를 말해 준다.
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
                // 뒤따르는 `:` 이 있어야 키다. 값 문자열은 여기서 걸러진다.
                let mut j = next;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    if depth < path.len() {
                        path[depth] = text.clone();
                    }
                    // 깊이 1 의 키가 티어, 깊이 2 의 키가 그 티어의 토큰이다.
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
                // 이스케이프는 내용으로 안 푼다 — 토큰 이름에는 안 나오고, 여기서 필요한
                // 것은 따옴표의 끝을 놓치지 않는 것뿐이다.
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

/// CSS 에서 **선언된** `--tasty-*` 이름을 모은다.
///
/// `var(--tasty-x)` 같은 **사용**은 이름 뒤가 `:` 이 아니라 걸러진다. 같은 이름이 테마 블록
/// 마다 다시 선언되는 것은 집합이 흡수한다.
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

/// 파서가 살아 있는지 — 이것이 먼저 통과해야 아래 판정의 초록이 뜻을 갖는다.
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
            "앱 DTCG 의 `{tier}` 티어에서 키를 하나도 못 읽었다 — json 구조가 바뀌었거나 \
             스캐너가 죽었다. 그 상태의 \"차이 없음\" 은 통과가 아니라 측정 실패다"
        );
        assert!(
            !site.is_empty(),
            "사이트 CSS `{css_path}` 에서 `--tasty-*` 선언을 하나도 못 읽었다 — 같은 이유로 \
             측정 실패다"
        );
        app_total += app.len();
        site_total += site.len();
    }
    assert!(
        app_total >= PARSE_FLOOR && site_total >= PARSE_FLOOR,
        "읽은 토큰이 앱 {app_total} · 사이트 {site_total} 로 하한 {PARSE_FLOOR} 아래다 — \
         파서가 일부만 읽고 있다"
    );

    // 비영 대조 — 양쪽에서 **같은 이름 하나**가 잡히는지 본다. 위 하한은 한쪽이 통째로
    // 엉뚱한 것을 읽고 있어도 통과할 수 있다.
    let (tier, name) = PRESENT_ON_BOTH;
    let css_path = TIERS
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, p)| *p)
        .expect("PRESENT_ON_BOTH 의 티어는 TIERS 에 있어야 한다");
    assert!(
        tier_keys(&json, tier).contains(name),
        "앱 DTCG 의 `{tier}` 에서 `{name}` 을 못 찾았다 — 술어가 죽었다면 아래 판정의 \
         \"사이트 결손\" 은 안 봐서 나온 값이다"
    );
    assert!(
        css_names(&read(css_path)).contains(name),
        "사이트 `{css_path}` 에서 `--tasty-{name}` 을 못 찾았다 — 같은 이유다"
    );
}

/// 명부의 티어 이름이 실재하는지. 오타가 나면 그 항목은 어느 비교에도 안 들어가
/// **조용히 사면**된다.
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

/// 두 사본의 차이가 **기록된 그대로**인가. 늘어도 줄어도 실패한다.
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
                "[{tier}] 명부에 없는 결손 {}종: {newly_behind:?}\n  \
                 앱이 새 결정을 받았고 사이트 사본이 안 따라왔다. 처방은 재-vendoring 이다 \
                 (`site/vendor/README.md` 의 \"vendor 갱신 절차\"). \
                 의식적으로 미룬다면 그 이름을 이 파일의 명부에 더하고 **커밋 메시지에 사유를 적어라** \
                 — 명부를 늘리는 편집은 \"이번 결정을 사이트에 안 반영한다\" 는 선언이다.",
                newly_behind.len()
            ));
        }

        let caught_up: Vec<&&str> = recorded.difference(&missing).collect();
        if !caught_up.is_empty() {
            problems.push(format!(
                "[{tier}] 명부에 있는데 이미 따라온 {}종: {caught_up:?}\n  \
                 재-vendoring 이 일어났다. 그 이름을 이 파일의 명부에서 **지워라** — \
                 남겨 두면 그만큼이 다음 결손을 가린다. 명부가 비면 명부 자체를 지우고 \
                 판정을 집합 동등으로 바꾼다.",
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
                "[{tier}] 앱에 없는데 사이트에만 있는 {}종: {site_only:?}\n  \
                 사본은 원격에서 내려오기만 하므로 사이트가 앞서갈 길이 없다. \
                 `{css_path}` 를 손으로 고쳤거나, 앱 사본이 오히려 뒤처졌다 — \
                 어느 쪽인지 먼저 가려라. 손 편집이면 `site/vendor/README.md` 가 금지한 방향이다.",
                site_only.len()
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "사이트 vendor 토큰이 앱 export 와 기록된 차이만큼 떨어져 있지 않다:\n\n{}",
        problems.join("\n\n")
    );
}
