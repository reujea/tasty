//! `let _ =` 사유 주석 전수 가드 — 프로덕션 코드에서 이유 없이 값을 버리면 fail 한다.
//!
//! 배경: `CLAUDE.md` "에러 처리 (필수)" 와
//! [`docs/dev-guide/error-handling.md`](../docs/dev-guide/error-handling.md) 는
//! "무시는 명시적 정책 결정이어야 하고 그 이유가 주석으로 남는다" 를 요구하는데,
//! 그걸 **전수로** 보는 검사가 지금까지 없었다. 세 층 중 어느 것도 못 본다:
//!
//! - `clippy::let_underscore_must_use` 는 타입은 정확히 보지만 **주석을 모른다** —
//!   사유가 제대로 달린 정상 코드까지 경고하므로 경고 총량이 크고, 신규 위반 한 건이
//!   그 안에 묻힌다.
//! - `.githooks/pre-commit` C.6 은 주석은 보지만 **staged diff 의 추가 라인만** 본다.
//! - 리뷰는 사람이라 전수가 아니다.
//!
//! ## 판정 규칙
//!
//! `let _ =` 한 건마다, **문장 범위 + 그 다음 한 줄** 안에 `//` 주석이 있거나,
//! **바로 앞의 유의미한 줄**(빈 줄과 `#[..]` 속성은 건너뛴다)이 `//` 주석이면 통과다.
//! 문장 범위는 `let` 부터 괄호 깊이가 0 으로 돌아오며 `;` 가 나오는 줄까지다.
//!
//! 이 규칙은 pre-commit C.6 이 인정하는 세 형태(같은 줄 · 윗줄 · 다음 줄)를 **모두
//! 포함하고 조금 더 넓다**(빈 줄·속성 건너뛰기, 멀티라인 문장 내부 주석). 방향이
//! 중요하다 — 훅이 통과시킨 코드를 CI 가 떨어뜨리면 안 되므로 가드가 훅보다 좁아서는
//! 안 된다. 반대 방향(가드가 더 넓다)은 훅이 먼저 막을 뿐이라 안전하다.
//!
//! **블록 상단 주석은 인정하지 않는다.** 몇 줄 위 블록 머리에 적힌 설명을 사유로
//! 인정하려면 "어디까지 거슬러 올라갈 것인가" 를 정해야 하는데, 함수 doc 주석까지
//! 닿을 만큼 넓히면 사실상 모든 `let _ =` 가 통과해 가드가 가드를 그만둔다. 사유는
//! 그 문장 옆에서 읽혀야 한다 — 위에 이미 적었더라도 한 줄을 더 적는 비용이
//! 가드를 유지하는 값보다 싸다.
//!
//! ## 대상 범위
//!
//! 테스트 코드는 제외한다 — `tests/` 디렉토리와 벤치 디렉토리 아래 전부, 남은 파일
//! 안의 `#[cfg(test)]` / `#[test]` 아이템 본문, 파일 단위 `#![cfg(test)]` 전체다.
//!
//! **`cfg` 술어는 이름이 아니라 성질로 판정한다**([`cfg_requires_test`]). `#[cfg(test)]`
//! 만 문자열로 맞춰 보면 `#[cfg(all(test, unix))]` 형태를 통째로 프로덕션으로 세어
//! 사유 주석을 요구한다(실측: 아이템 9 · 파일 2 자리). 반대 방향이 더 위험하다 —
//! `any(test, …)` 를 테스트로 세면 test 아닌 조합에서 살아 있는 **프로덕션 코드가
//! 조용히 검사 밖으로 나간다**. 그래서 판정은 "test 를 **요구**하는가" 다: `all(…)` 은
//! 한 갈래만 요구해도 요구이고, `any(…)` · `not(test)` 는 요구가 아니다. 테스트에서
//! 값을 버리는 것은 그 자체가 의도라 사유가 자명하고, 여기까지 강제하면 통과시키기
//! 위한 형식적 주석만 늘어난다. 정책이 지키려는 것은 **프로덕션에서 조용히 사라지는
//! 실패**다.
//!
//! 규칙을 "Result 만" 이 아니라 **모든 `let _ =`** 로 넓힌 이유는 텍스트 스캔이
//! 타입을 알 수 없기 때문이다. 변수 바인딩 억제(`let _ = path;`)도 "왜 여기서
//! 안 쓰는가" 가 읽는 사람에게 궁금한 지점이라 한 줄 주석이 손해가 아니다.
//!
//! ## 어휘 마스킹 (이전 결정을 뒤집었다)
//!
//! 이 doc 은 한때 "두 방향의 부정확을 **의도적으로 남긴다** — 렉서를 들이는 비용이
//! 이 가드가 지키려는 값보다 크다" 고 적고 있었다. **그 비용 전제가 틀렸다**: 같은
//! 마스커가 레포에 이미 여러 벌 적혀 있었고(측정), 하나를 [`rust_mask`] 로 올리는
//! 것이 새로 쓰는 것보다 쌌다. 남긴다고 적은 두 형태는 이제 둘 다 잡는다.
//!
//! - **미탐이었던 것** — 문장 범위의 `//` 가 주석인지 문자열 내용인지 안 봤다.
//!   `let _ = get("http://x");` 가 사유 없이 통과했다. 이제 [`tasty_doc_guards::source_text::mask_literals`]
//!   가 문자열만 지운 사본에서 `//` 를 찾으므로 이 형태가 잡힌다.
//! - **오탐이었던 것** — 문자열 리터럴 안의 금지 형태를 코드로 봤다. 한 lane 이 실제로
//!   이것 때문에 커밋을 막혀 스니펫을 바꿔 우회했다. 이제 [`tasty_doc_guards::source_text::mask_non_code`]
//!   를 거친 사본에서만 찾는다.
//!
//! 두 방향 각각에 회귀 테스트가 있고([`helper_tests`]), 마스커를 무효화하는 변이가
//! 그 둘을 각각 하나씩 죽이는 것을 확인했다.
//!
//! **남은 근사**: `.githooks/pre-commit` C.6 은 awk 라 같은 렉서를 못 쓴다. 거기서는
//! 한 줄 안에서 닫히는 문자열만 지운다 — **여러 줄에 걸친 문자열 리터럴은 훅이 여전히
//! 원문으로 본다**(diff 가 줄 단위라 그 층에서는 피할 수 없다). 그래서 두 층의 정확도가
//! 갈리는 방향은 **훅이 더 거칠다** 쪽이고, 전수 판정의 정본은 이 가드다.
//!
//! 어느 쪽도 사람 리뷰나 `.githooks/pre-commit` C.6 을 대체하지 않는다. 이 가드는
//! **전수로 도는 하한선**이지 상한선이 아니다.
//!
//! 테스트 코드 제외에도 비용이 있다는 실례는
//! [`docs/dev-guide/error-handling.md`](../docs/dev-guide/error-handling.md) 참조 —
//! 값을 버린 것이 검증 자체를 무력화한 경우가 있었고, 그 자리는 `tests/` 아래라
//! 이 가드가 보지 않는다.

use std::fs;
use std::path::{Path, PathBuf};

/// 금지 형태를 담는 것이 본질인 자리의 **면제 조각**. 파일 통째가 아니라
/// `(경로, 그 파일에서 면제할 코드 조각)` 쌍이다 — 등록한 조각을 담은 줄만 넘어가고,
/// 같은 파일이 **다른 형태의 위반을 새로 들이면 그건 잡힌다.** 조립도 근거도
/// `crates/tasty-doc-guards/tests/no_todo_file_citation.rs` 의 ALLOWLIST 와 같다(루트 `CLAUDE.md` 가 그
/// 이유를 적어놨다 — "파일 통째가 아니라 패턴 단위로 면제해, 그 파일이 다른 형태의
/// 위반을 새로 들이면 그건 잡히게 한다").
///
/// 등록은 하나다 — `vendor/tiny_http/` 는 상류 크레이트 사본이고, 사본을 상류와 같게
/// 두는 것이 그 디렉토리의 규칙이다(무엇이 tasty 패치인지가 상류와의 차이로 읽혀야
/// 한다 — `docs/adr/0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md`).
/// 그래서 상류가 쓴 두 줄에 사유 주석을 더하지 않고 줄 단위로 면제한다. 그 파일에 새
/// `let _` 이 들어오면 여전히 잡힌다. 이 가드 파일 자신은 `tests/` 아래라
/// [`is_test_path`] 로 이미 빠지므로 여기 등록될 일이 없다 — 그럼에도 픽스처는 런타임에
/// 조립한다([`ignore_stmt`]). `.githooks/pre-commit` C.6 에는 면제 장치가 **아예 없어서**
/// 어차피 그래야 하고, 덕분에 훅이 이 파일도 검사한다.
const ALLOWLIST: &[(&str, &[&str])] = &[(
    "vendor/tiny_http/src/lib.rs",
    &[
        "let _ = stream.shutdown(Shutdown::Both);",
        "let _ = std::fs::remove_file(path);",
    ],
)];

/// 위반 줄이 `entries` 에 등록된 조각을 담고 있으면 면제다. 판정이 **파일 단위가
/// 아니라 줄 단위**라, 등록된 파일이 새로 들이는 다른 위반은 그대로 잡힌다.
fn is_allowlisted(entries: &[(&str, &[&str])], rel: &str, line: &str) -> bool {
    entries
        .iter()
        .any(|(path, snippets)| *path == rel && snippets.iter().any(|s| line.contains(s)))
}

const PRUNE_DIRS: &[&str] = &[
    "target",
    "dist",
    "_site",
    ".worktree",
    ".git",
    ".idea",
    "node_modules",
    "assets",
];

/// 테스트 코드로 보는 디렉토리 이름 — 경로 성분 어디에 있어도 제외한다.
const TEST_DIRS: &[&str] = &["tests", "benches"];

/// gitignored 로컬 폴더 이름의 조각. 리터럴로 두면 이 파일이 비-git 경로 참조 금지
/// (`docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`) 를 어긴다 — 인용이
/// 아니라 순회 입력이지만, 조각으로 조립하면 예외 등록 없이 규칙을 지킬 수 있다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 가지치기 대상 디렉토리인지 — 빌드 산출물 + gitignored 로컬 폴더(선행 `.`).
///
/// **선행 `.` 전부를 자르지 않는다.** "gitignored 폴더는 전부 선행 `.` 을 갖는다" 는
/// 참이지만 역이 거짓이다 — 선행 `.` 이면서 추적되는 디렉토리가 `.github`·`.githooks`·
/// `.cargo` 셋이다(2026-09-06 실측). 예외를 하나씩 뚫는 방식은 넷째가 생기는 날
/// 조용히 틀리고, 그 틀림은 순회 대상을 **줄이는** 쪽이라 가드가 더 초록으로 보인다.
/// 그래서 근사가 아니라 이름 열거로 판정한다.
fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

fn gather(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if p.is_dir() {
            // 이름은 성질이 아니다 — `CARGO_TARGET_DIR` 로 다른 이름을 준 빌드
            // 디렉토리는 이름 목록에 안 걸린다. 표식 판정을 **보태서** 부른다(흉내
            // 내지 않는다): 근거와 후보 비교는 [`tasty_doc_guards::is_build_cache_dir`].
            if !is_pruned(name) && !tasty_doc_guards::is_build_cache_dir(&p) {
                gather(&p, out);
            }
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_test_path(rel: &str) -> bool {
    rel.split('/').any(|seg| TEST_DIRS.contains(&seg))
}

/// `let _ =` 가 코드에 나타나는가. **인자는 [`tasty_doc_guards::source_text::mask_non_code`] 를 거친 줄**이라
/// 주석·문자열·문자 리터럴은 이미 공백이다 — 여기서 다시 자르지 않는다.
///
/// 잘라내기를 없앤 것이 곧 거짓 음성 하나를 없앤 것이다: 예전에는 첫 `//` 앞까지만
/// 코드로 봐서, 같은 줄 앞쪽 문자열에 `//` 가 있으면(`"http://…"`) 그 뒤의 진짜
/// `let _ =` 를 못 봤다.
fn has_let_underscore(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while let Some(off) = code[i..].find("let") {
        let start = i + off;
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let rest = &code[start + 3..];
        if before_ok {
            let trimmed = rest.trim_start();
            if rest.len() != trimmed.len()
                && let Some(after_underscore) = trimmed.strip_prefix('_')
                && after_underscore.trim_start().starts_with('=')
                && !after_underscore.trim_start().starts_with("==")
            {
                return true;
            }
        }
        i = start + 3;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `cfg` 술어가 **`test` 를 요구하는가.** 이름으로 묻지 않고 성질로 묻는다 —
/// `#[cfg(test)]` 만 문자열로 맞춰 보면 `#[cfg(all(test, unix))]` 같은 형태를 통째로
/// 놓친다(실측: 레포에 `all(test, …)` 형태가 아이템 9 · 파일 2 자리 있다).
///
/// 방향이 중요하다. `all(test, …)` 는 **test 없이는 컴파일되지 않으므로** 테스트 범위다.
/// `any(test, …)` 는 아니다 — `#[cfg(any(unix, test))]` · `#[cfg(any(test, feature =
/// "test-support"))]` 는 test 가 아닌 조합에서도 살아 있어 그 코드는 프로덕션이다.
/// 이 구분을 안 하면 프로덕션 코드가 조용히 검사 밖으로 나간다.
fn cfg_requires_test(pred: &str) -> bool {
    let p = pred.trim();
    if p == "test" {
        return true;
    }
    match p.strip_prefix("all(").and_then(|r| r.strip_suffix(')')) {
        Some(inner) => cfg_args(inner).iter().any(|a| cfg_requires_test(a)),
        None => false,
    }
}

/// 최상위 쉼표로만 자른다 — 중첩 괄호 안의 쉼표는 인자 경계가 아니다.
fn cfg_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in inner.chars() {
        match ch {
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// `#[cfg(<test 를 요구하는 술어>)]` 또는 `#[test]` 인가. 여는 형태(`#!`)도 함께 본다.
fn is_test_attr(line: &str, inner_attr: bool) -> bool {
    let t = line.trim_start();
    let prefix = if inner_attr { "#![cfg(" } else { "#[cfg(" };
    if !inner_attr && t.starts_with("#[test]") {
        return true;
    }
    t.strip_prefix(prefix)
        .and_then(|r| r.trim_end().strip_suffix(")]"))
        .is_some_and(cfg_requires_test)
}

/// `#[cfg(test)]` / `#[test]` 아이템 본문에 속하는 줄 번호(0-based). 파일 단위
/// `#![cfg(<test 요구>)]` 가 있으면 파일 전체를 테스트로 본다.
fn test_regions(lines: &[&str]) -> Vec<bool> {
    let mut marked = vec![false; lines.len()];
    if lines.iter().any(|l| is_test_attr(l, true)) {
        return vec![true; lines.len()];
    }
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if is_test_attr(t, false) {
            let mut depth: i32 = 0;
            let mut started = false;
            let mut j = i;
            while j < lines.len() {
                for ch in lines[j].chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                            started = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                marked[j] = true;
                if started && depth <= 0 {
                    break;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    marked
}

/// 문장의 마지막 줄 index — 괄호 깊이가 0 으로 돌아오며 `;` 가 나오는 줄.
fn statement_end(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    for (offset, line) in lines.iter().enumerate().skip(start).take(80) {
        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && line.contains(';') {
            return offset;
        }
    }
    start
}

/// 바로 앞의 유의미한 줄이 `//` 주석인가 — 빈 줄과 `#[..]` 속성은 건너뛴다.
fn preceded_by_comment(lines: &[&str], start: usize) -> bool {
    let mut j = start;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if t.is_empty() || t.starts_with("#[") || t.starts_with("#![") {
            continue;
        }
        return t.starts_with("//");
    }
    false
}

fn violations_in(text: &str) -> Vec<(usize, String)> {
    let raw: Vec<&str> = text.lines().collect();
    // 두 판정에 서로 다른 마스킹이 필요하다 — 모듈 doc 참조.
    //   "코드에 `let _ =` 가 있나"  -> 주석·문자열 다 지운 사본
    //   "사유 주석이 달려 있나"     -> 문자열만 지우고 주석은 남긴 사본
    let code_src = tasty_doc_guards::source_text::mask_non_code(text);
    let lit_src = tasty_doc_guards::source_text::mask_literals(text);
    let code: Vec<&str> = code_src.lines().collect();
    let lit: Vec<&str> = lit_src.lines().collect();
    let in_test = test_regions(&code);
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        if in_test[i] || !has_let_underscore(line) {
            continue;
        }
        let end = statement_end(&code, i);
        // 문장 범위 + 그 다음 한 줄 (rustfmt 가 trailing 주석을 다음 줄로 밀어내는 형태).
        let scan_to = (end + 1).min(code.len().saturating_sub(1));
        let inline = lit[i..=scan_to].iter().any(|l| l.contains("//"));
        if inline || preceded_by_comment(&lit, i) {
            continue;
        }
        // 보고는 원문으로 한다 — 마스킹된 줄은 사람이 못 읽는다.
        out.push((i + 1, raw[i].trim().to_string()));
    }
    out
}

/// 스캔 하한 — [ADR-0133] 의 두 용도 중 **연기 검사**다("경로가 틀렸거나 읽기에 실패했다"
/// 를 잡는 용도). **모수 고정**("이만큼 봤으니 다 봤다")으로 쓰지 않는다 — 실제 모수가
/// 하한보다 크면 그 차이만큼 사각을 갖고도 초록이기 때문이다.
///
/// 이 가드가 "위반 0" 을 내는 이유는 둘이다: 정말 없거나, **아무것도 안 봤거나.**
/// [`gather`] 는 디렉토리를 못 읽으면 `return` 으로 조용히 빠져나가므로, 하한이 없으면
/// 순회가 깨진 날 정확히 초록이 뜬다. 실측으로 확인했다 — 스캔 루트를 빈 디렉토리로
/// 바꾸면 이 가드는 아무 말 없이 통과했다.
///
/// 값의 근거: 2026-09-05 기준 **[`gather`] 가 실제로 걷어 온 `.rs` 수 1180** 이다(가지치기
/// 후의 집합이라 추적 파일 전체와 다르다). 아래쪽으로 넉넉한 여유를 둔다 — 순회가 통째로
/// 깨진 경우를 결정적으로 잡는 것이 목적이고, 몇 퍼센트의 누락까지 조이면 레포가 줄어드는
/// 날 거짓 빨강이 된다.
///
/// [ADR-0133]: ../docs/adr/0133-guard-scan-population-is-pinned-not-enumerated.md
const MIN_SCANNED_FILES: usize = 700;

/// 스캔이 믿을 만한가.
///
/// 판정을 함수로 뽑아 둔다 — 단언 안에 인라인으로 두면 그 값이 무엇을 가르는지 시험할
/// 자리가 없고, 하한 자신이 장식이 된다.
fn scan_is_credible(found: usize) -> bool {
    found >= MIN_SCANNED_FILES
}

/// 하한을 겨냥한 변이 — 하한 자신이 판정을 하는지 본다.
#[test]
fn the_scan_refuses_to_report_zero_from_an_empty_walk() {
    assert!(!scan_is_credible(0), "빈 스캔을 믿을 만하다고 판정했다");
    assert!(!scan_is_credible(MIN_SCANNED_FILES - 1));
    assert!(scan_is_credible(MIN_SCANNED_FILES));
}

#[test]
fn every_let_underscore_in_production_code_says_why() {
    // `CARGO_MANIFEST_DIR` 이 곧 레포 루트가 아니다(여기서는 크레이트 디렉토리다).
    // 공용 `repo_root()` 는 표지 파일 넷으로 자기가 잡은 경로를 검증한다.
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut files = Vec::new();
    gather(root, &mut files);
    files.sort();
    assert!(
        scan_is_credible(files.len()),
        "스캔한 `.rs` 가 {}개다(하한 {MIN_SCANNED_FILES}) — 순회가 깨졌다. 위반 0 은 이 \
         상태에서 아무 뜻도 없다.\n\
         ★ 판별 — 이 모수는 추적 `.rs` 에서 가지치기를 뺀 것이다. 바깥 값을 한 번 세라:\n\
             git ls-files '*.rs' | wc -l\n\
         2026-09-06 실측 1286 이고 [`gather`] 는 1180 을 걷었다(차 106 은 가지친 가지들이다). \
         **git 쪽이 그대로인데 여기만 줄었으면 가지치기가 과해졌거나 순회가 죽은 것**이고, \
         둘이 함께 줄었으면 레포가 정말 줄어든 것이다. 앞쪽이면 [`PRUNE_DIRS`] 를 먼저 봐라 — \
         가지 하나가 늘면 수백 개가 한꺼번에 빠진다.\n\
         ★ 이 하한을 내려서 통과시키지 마라 — 이 가드의 산출은 명부이고, 명부가 짧아진 것과 \
         명부를 안 만든 것은 내려 놓은 하한 아래에서 구별되지 않는다.\n\
         가지치기가 정당하게 늘었으면 그 가지를 [`PRUNE_DIRS`] 에 적은 커밋에서 값도 함께 \
         다시 재라.",
        files.len()
    );

    let mut report = Vec::new();
    for file in &files {
        let rel = rel_of(file, root);
        if is_test_path(&rel) {
            continue;
        }
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (line, snippet) in violations_in(&text) {
            if is_allowlisted(ALLOWLIST, &rel, &snippet) {
                continue;
            }
            report.push(format!("  {rel}:{line}  {snippet}"));
        }
    }

    assert!(
        report.is_empty(),
        "사유 없이 값을 버리는 `let _` 이 있다 ({} 건).\n{}\n\n\
         왜 값을 버리는지 한 줄 주석을 같은 줄·윗줄·다음 줄 중 한 곳에 단다 \
         (docs/dev-guide/error-handling.md \"의도적 무시\").\n\
         값을 버리면 안 되는 것이었다면 주석 대신 처리하거나 로그를 남긴다.",
        report.len(),
        report.join("\n")
    );
}

// ── 명부 순수성: 테스트 범위가 clippy 명부에 섞이지 않는가 ─────────────

/// 테스트 타깃(그 자체가 별개 크레이트다)에 다는 면제.
const TARGET_EXEMPTION: &str = "#![allow(clippy::let_underscore_must_use)]";

/// 크레이트 루트에 다는 면제. **`cfg_attr(test, ..)` 인 것이 핵심이다** — 라이브러리
/// 타깃은 `cfg(test)` 없이 컴파일되므로 프로덕션 자리는 그대로 명부에 오른다.
/// 실측으로 확인했다: `tasty-cli` 에 이 줄을 넣어도 명부 자리 33 이 33 그대로였고,
/// `tasty-terminal` 에서는 테스트 자리 2 가 0 이 됐다.
const CRATE_EXEMPTION: &str = "#![cfg_attr(test, allow(clippy::let_underscore_must_use))]";

/// 면제 표기 — 위 두 형태와 아이템 단위 `#[allow(..)]` 를 한 술어로 본다.
fn mentions_exemption(line: &str) -> bool {
    line.contains("allow(clippy::let_underscore_must_use)")
}

/// `rel` 이 **자기 자신이 하나의 컴파일 타깃**인가. 통합 테스트·벤치는 각자 크레이트라
/// 크레이트 루트의 면제가 닿지 않는다 — 그 파일에 직접 달아야 한다.
fn is_own_target(rel: &str) -> bool {
    is_test_path(rel)
}

/// `rel` 이 속한 크레이트의 루트 소스 경로들(있는 것만).
fn crate_roots(root: &Path, rel: &str) -> Vec<PathBuf> {
    let dir = if let Some(rest) = rel.strip_prefix("crates/") {
        match rest.split_once('/') {
            Some((name, _)) => root.join("crates").join(name),
            None => root.to_path_buf(),
        }
    } else {
        root.to_path_buf()
    };
    ["src/lib.rs", "src/main.rs"]
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| p.is_file())
        .collect()
}

/// 테스트 범위에서 값을 버리는 자리 — [`violations_in`] 의 **여집합**이다. 저쪽이
/// 프로덕션만 보는 것과 같은 스캐너·같은 마스킹을 쓴다(사본을 만들지 않는다 — 사본이
/// 둘이면 갈리고, 갈린 쪽은 조용하다).
///
/// `whole_file` 은 파일 자체가 테스트 타깃일 때다. [`test_regions`] 만으로는 부족하다 —
/// 통합 테스트의 **헬퍼 함수**는 `#[test]` 아이템 밖에 있어서 표식이 없다. 실측으로
/// 걸렸다: 그 형태를 안 세면 통합 테스트 파일의 면제를 지워도 이 가드가 초록이었다.
fn test_scope_sites(text: &str, whole_file: bool) -> Vec<usize> {
    let code_src = tasty_doc_guards::source_text::mask_non_code(text);
    let code: Vec<&str> = code_src.lines().collect();
    let in_test = test_regions(&code);
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        if (whole_file || in_test[i]) && has_let_underscore(line) {
            out.push(i);
        }
    }
    out
}

/// 그 줄을 감싸는 테스트 아이템에 `#[allow(..)]` 이 붙어 있는가. 아이템의 속성 묶음은
/// 연속된 `#[..]` 줄이라, 표식 줄 위아래로 붙어 있는 것만 본다.
fn item_is_exempt(code: &[&str], line: usize) -> bool {
    let mut i = line;
    loop {
        let t = code[i].trim_start();
        if is_test_attr(t, false) {
            let mut a = i;
            while a > 0 && code[a - 1].trim_start().starts_with("#[") {
                a -= 1;
            }
            let mut b = i;
            while b + 1 < code.len() && code[b + 1].trim_start().starts_with("#[") {
                b += 1;
            }
            return code[a..=b].iter().any(|l| mentions_exemption(l));
        }
        if i == 0 {
            return false;
        }
        i -= 1;
    }
}

/// **테스트 범위의 `let _ =` 는 명부 밖에 있어야 한다.**
///
/// `clippy::let_underscore_must_use` 의 출력은 위반 목록이 아니라 **프로덕션에서 값을
/// 의도적으로 버리는 자리의 명부**다(`docs/dev-guide/error-handling.md`). 정책이 애초에
/// 아무것도 요구하지 않는 테스트 본문이 거기 섞이면, 테스트가 늘 때마다 숫자만 흔들리고
/// **새로 들어오는 프로덕션 자리가 그 안에 묻힌다.** 실제로 한 번 났다 — 직전 측정 이후
/// 하루 만에 테스트 범위 18 자리가 섞였고 수동으로 되돌렸다.
///
/// **판정 단위가 자리가 아니라 타깃이다.** 텍스트 스캔은 타입을 모르므로 `let _ =` 하나
/// 하나에 면제를 요구하면 clippy 가 애초에 경고하지 않는 자리까지 요구한다(실측: 면제
/// 없는 테스트 범위 자리 80 중 지금 명부에 오른 것은 5 뿐이다). 그래서 요구를 **크레이트
/// 루트 한 줄 / 테스트 타깃 한 줄**로 올린다 — 요구 수가 80 에서 20 으로 떨어지고, 그
/// 한 줄이 **그 크레이트의 앞으로 생길 테스트까지 함께 덮는다.** 회귀가 들어온 경로
/// (프로덕션 파일 안 `#[cfg(test)]` 모듈 8 자리)가 정확히 그 형태였다.
#[test]
fn test_scope_stays_out_of_the_lint_roster() {
    // `CARGO_MANIFEST_DIR` 이 곧 레포 루트가 아니다(여기서는 크레이트 디렉토리다).
    // 공용 `repo_root()` 는 표지 파일 넷으로 자기가 잡은 경로를 검증한다.
    let root = tasty_doc_guards::repo_root();
    let root = root.as_path();
    let mut files = Vec::new();
    gather(root, &mut files);
    files.sort();
    assert!(
        scan_is_credible(files.len()),
        "스캔한 `.rs` 가 {}개다(하한 {MIN_SCANNED_FILES}) — 순회가 깨졌다",
        files.len()
    );

    // 자기-공허 검사를 **갈래마다** 센다. 하나로 합치면 한쪽이 죽어도 다른 쪽이 수를
    // 채워 초록이 된다 — 실측으로 걸렸다: `test_regions` 를 통째로 무력화해도 경로 기반
    // 갈래가 자리를 채워 이 검사가 통과했다. 회귀가 들어온 경로(프로덕션 파일 안
    // `#[cfg(test)]` 모듈)가 정확히 죽은 그 갈래였다.
    let mut seen_by_path = 0usize;
    let mut seen_by_region = 0usize;
    let mut report = Vec::new();
    for file in &files {
        let rel = rel_of(file, root);
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let own_target = is_own_target(&rel);
        let sites = test_scope_sites(&text, own_target);
        if sites.is_empty() {
            continue;
        }
        if own_target {
            seen_by_path += sites.len();
        } else {
            seen_by_region += sites.len();
        }
        let code_src = tasty_doc_guards::source_text::mask_non_code(&text);
        let code: Vec<&str> = code_src.lines().collect();
        if code.iter().any(|l| {
            let t = l.trim_start();
            t.starts_with("#![") && mentions_exemption(t)
        }) {
            continue;
        }
        if own_target {
            report.push(format!("  {rel}  ← 파일 머리에 `{TARGET_EXEMPTION}`"));
            continue;
        }
        let uncovered: Vec<usize> = sites
            .iter()
            .copied()
            .filter(|&i| !item_is_exempt(&code, i))
            .collect();
        if uncovered.is_empty() {
            continue;
        }
        let roots = crate_roots(root, &rel);
        let covered = roots.iter().any(|p| {
            fs::read_to_string(p).is_ok_and(|t| {
                tasty_doc_guards::source_text::mask_non_code(&t)
                    .lines()
                    .any(|l| l.trim_start().starts_with("#![") && mentions_exemption(l))
            })
        });
        if covered {
            continue;
        }
        let where_to = roots
            .first()
            .map(|p| rel_of(p, root))
            .unwrap_or_else(|| "(크레이트 루트를 못 찾았다)".to_string());
        report.push(format!(
            "  {rel}:{}  ← 크레이트 루트 `{where_to}` 에 `{CRATE_EXEMPTION}`",
            uncovered[0] + 1
        ));
    }

    assert!(
        seen_by_path > 0,
        "테스트 타깃(`tests/`·`benches/`)에서 `let _` 무시를 한 자리도 못 봤다 — 순회가 깨졌다"
    );
    assert!(
        seen_by_region > 0,
        "프로덕션 파일 안의 `#[cfg(test)]`/`#[test]` 본문에서 `let _` 무시를 한 자리도 \
         못 봤다 — 범위 판정(`test_regions`)이 깨졌다"
    );

    report.sort();
    report.dedup();
    assert!(
        report.is_empty(),
        "테스트 범위에서 `let _` 로 값을 버리는데 면제가 없다 ({} 곳).\n{}\n\n\
         이 lint 의 출력은 위반 목록이 아니라 **프로덕션 명부**다 \
         (docs/dev-guide/error-handling.md). 테스트가 거기 섞이면 새 프로덕션 자리가 \
         묻힌다.\n\
         자리마다가 아니라 위에 적힌 **한 줄**을 달면 그 타깃 전체가 덮인다.",
        report.len(),
        report.join("\n")
    );
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    /// 픽스처 조립 — 이 파일은 [`ALLOWLIST`] 면제가 없고 `.githooks/pre-commit` C.6
    /// 에는 면제 장치 자체가 없다. 금지 형태를 소스에 그대로 적으면 이 가드와 훅이
    /// 자기 자신을 잡으므로 런타임에 만든다.
    fn ignore_stmt() -> String {
        format!("let {}= g();", "_ ")
    }

    fn scan(src: &str) -> Vec<usize> {
        violations_in(src).into_iter().map(|(l, _)| l).collect()
    }

    /// `{}` 자리에 [`ignore_stmt`] 를 끼운다.
    fn fixture(template: &str) -> Vec<usize> {
        scan(&template.replace("{IGNORE}", &ignore_stmt()))
    }

    #[test]
    fn detects_a_bare_ignore() {
        assert_eq!(fixture("fn f() {\n    {IGNORE}\n}\n"), vec![2]);
    }

    #[test]
    fn same_line_previous_line_and_next_line_comments_all_count() {
        assert!(fixture("fn f() {\n    {IGNORE} // 이유\n}\n").is_empty());
        assert!(fixture("fn f() {\n    // 이유\n    {IGNORE}\n}\n").is_empty());
        assert!(fixture("fn f() {\n    {IGNORE}\n    // 이유\n}\n").is_empty());
    }

    #[test]
    fn blank_lines_and_attributes_are_skipped_when_looking_up() {
        assert!(fixture("fn f() {\n    // 이유\n\n    {IGNORE}\n}\n").is_empty());
        assert!(
            fixture("fn f() {\n    // 이유\n    #[allow(unused)]\n    {IGNORE}\n}\n").is_empty()
        );
    }

    #[test]
    fn a_comment_inside_a_multiline_statement_counts() {
        let src = format!(
            "fn f() {{\n    let {}= g(Bar {{\n        // 이유\n        a: 1,\n    }});\n}}\n",
            "_ "
        );
        assert!(scan(&src).is_empty());
    }

    #[test]
    fn a_block_comment_far_above_does_not_count() {
        assert_eq!(
            fixture("fn f() {\n    // 블록 상단 설명\n    h();\n    {IGNORE}\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn test_bodies_are_exempt() {
        assert!(fixture("#[test]\nfn t() {\n    {IGNORE}\n}\n").is_empty());
        assert!(
            fixture("#[cfg(test)]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n")
                .is_empty()
        );
    }

    /// 술어를 **성질로** 묻는다는 것의 회귀 — `#[cfg(test)]` 만 문자열로 맞추면
    /// 이 형태가 프로덕션으로 세어져 사유 주석을 요구받는다.
    #[test]
    fn a_cfg_that_requires_test_among_other_predicates_is_still_test() {
        assert!(
            fixture(
                "#[cfg(all(test, unix))]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n"
            )
            .is_empty()
        );
        assert!(
            fixture(
                "#[cfg(all(unix, all(test, feature = \"gui\")))]\nmod tests {\n    fn t() {\n        {IGNORE}\n    }\n}\n"
            )
            .is_empty()
        );
    }

    /// **반대 방향이 이 판정의 위험한 쪽이다.** `any(test, …)` 는 test 가 아닌
    /// 조합에서도 컴파일되므로 프로덕션이다 — 이것을 테스트로 세면 프로덕션 코드가
    /// 조용히 검사 밖으로 나간다.
    #[test]
    fn a_cfg_that_merely_mentions_test_is_not_test_scope() {
        assert_eq!(
            fixture("#[cfg(any(unix, test))]\nmod m {\n    fn f() {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
        assert_eq!(
            fixture("#[cfg(not(test))]\nmod m {\n    fn f() {\n        {IGNORE}\n    }\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn a_file_level_cfg_that_requires_test_exempts_the_whole_file() {
        assert!(fixture("#![cfg(all(test, unix))]\n\nfn f() {\n    {IGNORE}\n}\n").is_empty());
        // 파일 단위라도 test 를 요구하지 않으면 프로덕션이다.
        assert_eq!(
            fixture("#![cfg(unix)]\n\nfn f() {\n    {IGNORE}\n}\n"),
            vec![4]
        );
    }

    #[test]
    fn a_whole_file_marked_cfg_test_is_exempt() {
        assert!(fixture("#![cfg(test)]\nfn f() {\n    {IGNORE}\n}\n").is_empty());
    }

    #[test]
    fn identifier_lookalikes_are_not_matched() {
        // `let _y` 바인딩, `==` 비교, 식별자 꼬리(`outlet _ =`)는 대상이 아니다.
        assert!(scan("fn f() {\n    let _y = g();\n}\n").is_empty());
        assert!(scan("fn f() {\n    if _ == g() {}\n}\n").is_empty());
        assert!(fixture("fn f() {\n    out{IGNORE}\n}\n").is_empty());
    }

    /// 문자열 리터럴 안의 금지 형태는 코드가 아니다 — 렉서 없이 원문을 보면
    /// 여기서 틀렸다. 실제로 한 lane 이 이 형태 때문에 커밋을 막혀 스니펫을 바꿔
    /// 우회했다.
    #[test]
    fn a_form_inside_a_string_literal_is_not_a_violation() {
        let src = format!(
            "fn f() {{\n    let s = \"{}\";\n    drop(s);\n}}\n",
            ignore_stmt()
        );
        assert!(scan(&src).is_empty());
    }

    /// 반대 방향 — 문자열 안의 `//` 는 사유 주석이 아니다. URL 이 대표적이고,
    /// 이쪽은 **아무도 아프지 않게 통과**시키므로 더 오래 남는다.
    #[test]
    fn a_slash_slash_inside_a_string_is_not_a_reason() {
        let src = format!("fn f() {{\n    let {}= get(\"http://x\");\n}}\n", "_ ");
        assert_eq!(scan(&src), vec![2]);
    }

    #[test]
    fn a_form_inside_a_line_comment_is_not_a_violation() {
        assert!(fixture("fn f() {\n    // 예: {IGNORE}\n}\n").is_empty());
    }

    /// 면제는 파일 통째가 아니라 조각 단위다 — 등록한 파일이 새로 들이는 다른
    /// 위반은 그대로 잡혀야 한다. 이 성질이 깨지면 ALLOWLIST 는 "그 파일에서
    /// 가드를 끄는" 스위치가 되고, 등록 한 번이 그 파일의 미래 위반까지 전부
    /// 통과시킨다.
    #[test]
    fn the_allowlist_exempts_a_registered_snippet_not_the_whole_file() {
        let entries: &[(&str, &[&str])] = &[("src/x.rs", &["RULE_TEXT"])];

        assert!(is_allowlisted(
            entries,
            "src/x.rs",
            "let s = \"RULE_TEXT\";"
        ));
        assert!(
            !is_allowlisted(entries, "src/x.rs", &ignore_stmt()),
            "등록 파일의 다른 위반까지 면제되면 파일 통째 면제와 같아진다"
        );
        assert!(
            !is_allowlisted(entries, "src/y.rs", "let s = \"RULE_TEXT\";"),
            "같은 조각이어도 등록되지 않은 파일은 면제가 아니다"
        );
    }

    #[test]
    fn test_dirs_are_recognised_by_any_path_segment() {
        assert!(is_test_path("tests/common/mod.rs"));
        assert!(is_test_path(
            "crates/tasty-agent/tests/runner_integration.rs"
        ));
        assert!(!is_test_path("src/core/agent/task.rs"));
    }
}
