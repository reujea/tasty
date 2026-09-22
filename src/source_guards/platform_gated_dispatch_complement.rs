//! `target_os` 로 게이트된 dispatch arm 에는 **상보 arm 이 있다.**
//!
//! IPC 메서드는 세 층에 걸쳐 있다 — 등재(`METHOD_TABLE`/`DEBUG_METHODS`), CLI
//! 서브커맨드, 그리고 dispatch arm. 이 저장소에서 앞의 두 층은 **플랫폼 균일하다**
//! (2026-09-05 실측: `crates/tasty-cli/src/` 와 `crates/tasty-ipc/src/method_meta.rs`
//! 에 `target_os` 게이트 0 건). 그래서 플랫폼 차이는 오직 dispatch 층에만 있다.
//!
//! 그 층에서 arm 을 `#[cfg(all(target_os = "…", …))]` 하나로만 두면, 다른 플랫폼에서는
//! arm 자체가 사라져 `match` 의 `_` 로 떨어진다. 그 답이 `-32601`("그런 메서드 없음")
//! 이다 — **거짓이다.** 메서드는 있다. 표에 있고 CLI 도 내놓는다(`tasty debug raw-key`
//! 가 도움말에 뜬다). 이 플랫폼이 못 할 뿐이다. 호출자 입장에서 "오타" 와 "여기선
//! 안 됨" 은 고칠 방법이 다르므로, 그 둘을 같은 코드로 답하면 안 된다.
//!
//! 실측(2026-09-05, Linux debug 빌드 실행 census):
//!
//! | 상보 arm | `surface.raw_key` 응답 |
//! |----------|------------------------|
//! | 없음 | `-32601 Method not found: surface.raw_key` |
//! | 있음 | `-32015 input reproduction over the OS event stream is macOS-only …` |
//!
//! 근거와 대안은 [ADR-0154](../../docs/adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md).
//!
//! ## 왜 컴파일러가 아니라 이 가드인가
//!
//! 빠진 상보 arm 은 **어느 플랫폼에서도 컴파일 오류가 아니다.** macOS 에서는 arm 이
//! 있으니 정상이고, Linux/Windows 에서는 없는 채로 `_` 가 받으니 역시 정상이다.
//! 게다가 이 저장소에서 macOS 조합은 로컬에서 빌드조차 되지 않는다(실측: 크로스 체크가
//! `libsqlite3-sys` 에서 멈춘다). 짝이 맞는지를 보는 곳은 소스 텍스트뿐이다.

use std::collections::{BTreeMap, BTreeSet};

use super::repo_root;

/// dispatch arm 이 사는 파일. 플랫폼 게이트가 새 파일로 번지면 여기 더한다.
const DISPATCH_SOURCES: &[&str] = &["src/adapters/ipc/handler.rs"];

/// `target_os` 가 걸린 arm 수의 하한 — **연기 검사**다. 0 이면 아래 짝 판정은 빈
/// 집합이라 그냥 통과한다. 값의 근거: 2026-09-05 실측 2 건
/// (`surface.switch_input_source` · `surface.raw_key`).
const MIN_PLATFORM_ARMS: usize = 2;

/// arm 패턴 자리로 볼 수 있는 최대 길이. `#[cfg(...)]` 뒤가 arm 이 아니라 `const`·`use`
/// 같은 항목이면 다음 `=>` 까지가 멀거나 `;`·`{` 를 품는다 — 그것으로 가른다.
const MAX_ARM_PATTERN: usize = 200;

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

/// 공백을 전부 지운 cfg 조건 — `not(all(a, b))` 와 `not(all(a,b))` 를 같게 본다.
fn normalize(cond: &str) -> String {
    cond.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `#[cfg(` 뒤의 괄호 균형을 세어 조건 문자열을 잘라낸다.
fn cfg_condition(src: &str, open_paren: usize) -> Option<(String, usize)> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    for (i, _) in src[open_paren..].char_indices() {
        match bytes[open_paren + i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((
                        src[open_paren + 1..open_paren + i].to_string(),
                        open_paren + i,
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// 문자열 리터럴만 뽑는다 — arm 패턴의 메서드 이름이다.
fn literals(seg: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = seg;
    while let Some(at) = rest.find('"') {
        let after = &rest[at + 1..];
        let Some(end) = after.find('"') else { break };
        out.insert(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// 한 파일에서 읽어낸 arm 지도.
pub(super) struct Arms {
    /// 메서드 이름 → 그 이름을 받는 arm 들의 cfg 조건(공백 제거형).
    by_method: BTreeMap<String, BTreeSet<String>>,
    /// `target_os` 를 품은 arm 들 — (메서드, 조건).
    platform: Vec<(String, String)>,
}

/// dispatch arm 을 훑어 위 지도를 만든다.
///
/// 구조(`#[cfg(` · 괄호 짝 · `=>` · `;` · `{`)는 공용 렉서가 주석·문자열·문자 리터럴을 바이트째
/// 덮은 사본(`code`)에서 찾고, 조건과 메서드 이름은 같은 구간의 원본에서 읽는다 — guard
/// 문자열 안의 `{` · `;` 가 그 arm 을 "arm 이 아님" 으로 떨궈 짝 검사를 조용히 건너뛰었고,
/// 주석 속 `#[cfg(` 인용이 arm 으로 읽힐 수 있었다.
pub(super) fn scan(src: &str) -> Arms {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let mut by_method: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut platform: Vec<(String, String)> = Vec::new();
    let mut from = 0usize;
    while let Some(at) = code[from..].find("#[cfg(") {
        let at = from + at;
        let open = at + "#[cfg".len();
        let Some((_, close)) = cfg_condition(&code, open) else {
            break;
        };
        let cond = src[open + 1..close].to_string();
        from = close + 1;
        // `]` 다음부터 첫 `=>` 까지가 arm 패턴 자리다.
        let Some(arrow) = code[from..].find("=>") else {
            continue;
        };
        let seg_code = &code[from..from + arrow];
        let seg = &src[from..from + arrow];
        let looks_like_arm = arrow <= MAX_ARM_PATTERN
            && !seg_code.contains(';')
            && !seg_code.contains('{')
            && seg.contains('"');
        if !looks_like_arm {
            continue;
        }
        let cond_n = normalize(&cond);
        for m in literals(seg) {
            by_method
                .entry(m.clone())
                .or_default()
                .insert(cond_n.clone());
            if cond.contains("target_os") {
                platform.push((m, cond_n.clone()));
            }
        }
    }
    Arms {
        by_method,
        platform,
    }
}

/// 조건이 `cond` 인 arm 의 짝은 조건이 `not(cond)` 인 arm 이다.
fn complement_of(cond: &str) -> String {
    if let Some(inner) = cond.strip_prefix("not(").and_then(|s| s.strip_suffix(')')) {
        inner.to_string()
    } else {
        format!("not({cond})")
    }
}

/// 플랫폼 게이트가 걸린 메서드는 **다른 플랫폼에서도 답한다.**
#[test]
fn a_platform_gated_method_still_answers_elsewhere() {
    let mut arms = 0usize;
    let mut orphan: Vec<String> = Vec::new();
    for rel in DISPATCH_SOURCES {
        let found = scan(&read(rel));
        arms += found.platform.len();
        for (method, cond) in &found.platform {
            let want = complement_of(cond);
            let has = found
                .by_method
                .get(method)
                .is_some_and(|conds| conds.contains(&want));
            if !has {
                orphan.push(format!(
                    "{rel}: `{method}` — `{cond}` 만 있고 `{want}` 가 없다"
                ));
            }
        }
    }
    assert!(
        arms >= MIN_PLATFORM_ARMS,
        "`target_os` 가 걸린 dispatch arm 을 {arms} 개밖에 못 찾았다(하한 \
         {MIN_PLATFORM_ARMS}, 2026-09-05 실측 2). 대조군이 죽었다 — 추출기나 \
         `DISPATCH_SOURCES` 를 확인해라"
    );
    assert!(
        orphan.is_empty(),
        "플랫폼 게이트가 걸린 dispatch arm 에 상보 arm 이 없다. 그 플랫폼에서는 arm 이 \
         사라져 `_` 가 받고, 답이 `-32601`(그런 메서드 없음)이 된다 — 메서드는 등재돼 \
         있고 CLI 도 내놓으므로 그 답은 거짓이다. 왜 못 하는지를 말하는 arm 을 짝으로 \
         두거나(예: `-32015`), 등재·CLI 에서도 함께 빼라.\n  {}",
        orphan.join("\n  ")
    );
}

#[cfg(test)]
mod exemption_mutations {
    use super::*;

    /// 짝이 없으면 잡는다 — 이 가드가 겨냥한 바로 그 형태.
    #[test]
    fn a_lone_platform_arm_is_caught() {
        let src = "\
match m {
    #[cfg(all(target_os = \"macos\", feature = \"gui\"))]
    \"surface.raw_key\" => go(),
    _ => not_found(),
}
";
        let found = scan(src);
        assert_eq!(found.platform.len(), 1, "게이트 걸린 arm 을 못 찾았다");
        let (m, cond) = &found.platform[0];
        assert!(
            !found.by_method[m].contains(&complement_of(cond)),
            "짝이 없는데 있다고 판정했다"
        );
    }

    /// 짝이 있으면 통과한다 — 그리고 그 짝은 `not(...)` 하나만 인정한다.
    #[test]
    fn the_complement_closes_it() {
        let src = "\
match m {
    #[cfg(all(target_os = \"macos\", feature = \"gui\"))]
    \"surface.raw_key\" => go(),
    #[cfg(not(all(target_os = \"macos\", feature = \"gui\")))]
    \"surface.raw_key\" => why_not(),
    _ => not_found(),
}
";
        let found = scan(src);
        let (m, cond) = &found.platform[0];
        assert!(found.by_method[m].contains(&complement_of(cond)));

        // 다른 조건의 arm 은 짝이 아니다.
        let wrong = "\
match m {
    #[cfg(all(target_os = \"macos\", feature = \"gui\"))]
    \"surface.raw_key\" => go(),
    #[cfg(not(feature = \"gui\"))]
    \"surface.raw_key\" => something_else(),
}
";
        let found = scan(wrong);
        let (m, cond) = &found.platform[0];
        assert!(
            !found.by_method[m].contains(&complement_of(cond)),
            "조건이 다른 arm 을 짝으로 셌다 — 그러면 macOS 에서만 도는 arm 이 헤드리스 \
             게이트로 가려진다"
        );
    }

    /// `#[cfg(...)]` 가 arm 이 아닌 항목(`const`·`use`)에 붙은 자리는 세지 않는다.
    #[test]
    fn a_gated_item_is_not_mistaken_for_an_arm() {
        let src = "\
#[cfg(not(all(target_os = \"macos\", feature = \"gui\")))]
const WHY: &str = \"platform\";
fn f() { let x = a => b; }
";
        let found = scan(src);
        assert!(
            !found.by_method.contains_key("platform"),
            "게이트된 항목의 문자열을 arm 패턴으로 집었다: {:?}",
            found.by_method
        );
    }

    /// 여러 이름을 `|` 로 묶은 arm 은 이름마다 따로 센다.
    #[test]
    fn an_or_pattern_counts_each_name() {
        let src = "\
match m {
    #[cfg(all(target_os = \"macos\", feature = \"gui\"))]
    \"a.one\" | \"a.two\" => go(),
    _ => x(),
}
";
        let found = scan(src);
        assert_eq!(found.platform.len(), 2, "`|` 로 묶인 이름을 하나로 셌다");
    }
}

/// ADR-0154 의 **전제**를 못 박는다 — 등재와 CLI 는 플랫폼 균일하다.
///
/// 이 결정은 "dispatch 층에서만 플랫폼을 본다" 인데, 그 근거가 취향이 아니라 실측이었다:
/// 2026-09-05 기준 CLI 서브커맨드 정의와 메서드 등재표에 `target_os` 게이트가 **0 건**이다.
/// 그래서 위 상보 arm 규칙이 "차이를 한 곳에 모은다" 는 뜻을 가진다.
///
/// 어느 한쪽에 플랫폼 조건이 처음 들어오면 그 뜻이 깨진다 — 그때는 이 가드를 지우는 것이
/// 아니라 ADR 을 다시 여는 것이 맞다(ADR-0154 의 재검토 트리거가 이것이다). 그래서
/// 실패 메시지가 "하지 마라" 가 아니라 "결정을 다시 열어라" 라고 말한다.
mod platform_uniform_layers {
    use super::*;

    /// 검사 대상. 앞은 CLI 서브커맨드 정의 트리, 뒤는 메서드 등재표.
    const UNIFORM_TREES: &[&str] = &["crates/tasty-cli/src"];
    const UNIFORM_FILES: &[&str] = &["crates/tasty-ipc/src/method_meta.rs"];

    /// 스캔한 `.rs` 파일 수의 하한 — **연기 검사**다. 워커가 죽어 0 개를 읽으면
    /// "게이트 0 건" 이 언제나 참이 된다. 값의 근거: 2026-09-05 실측 84 개.
    const MIN_SCANNED: usize = 40;

    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn registration_and_cli_do_not_branch_on_the_platform() {
        let root = repo_root();
        let mut files = Vec::new();
        for t in UNIFORM_TREES {
            rs_files(&root.join(t), &mut files);
        }
        for f in UNIFORM_FILES {
            files.push(root.join(f));
        }
        files.sort();
        assert!(
            files.len() >= MIN_SCANNED,
            "`.rs` 를 {} 개밖에 못 읽었다(하한 {MIN_SCANNED}, 2026-09-05 실측 84). \
             파일을 못 읽으면 아래 판정은 언제나 통과한다 — 대조군이 죽었다",
            files.len()
        );

        let mut hits: Vec<String> = Vec::new();
        for f in &files {
            let Ok(raw) = std::fs::read_to_string(f) else {
                continue;
            };
            // 주석·문자열은 지운 사본에서만 본다 — 이 규칙을 **설명하는** 문장이
            // 스스로를 위반으로 잡지 않게 한다.
            let masked = crate::source_guards::mask_non_code(&raw);
            for (i, line) in masked.lines().enumerate() {
                if line.contains("target_os") {
                    let rel = tasty_doc_guards::source_text::repo_relative(
                        f.strip_prefix(&root).unwrap_or(f),
                    );
                    let rel = rel.display();
                    hits.push(format!("{rel}:{}", i + 1));
                }
            }
        }
        assert!(
            hits.is_empty(),
            "메서드 등재표나 CLI 서브커맨드가 플랫폼으로 갈린다. \
             [ADR-0154](docs/adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md) \
             는 **그 두 층이 플랫폼 균일하다는 실측** 위에서 \"차이는 dispatch 층에만 \
             둔다\" 를 골랐다. 여기에 조건이 생기면 그 전제가 깨지므로, 이 가드를 지우는 \
             것이 아니라 ADR 을 다시 여는 것이 맞다(그 ADR 의 재검토 트리거다):\n  {}",
            hits.join("\n  ")
        );
    }

    /// 판정기가 살아 있는가 — 합성 입력에 위반을 심으면 본다.
    #[test]
    fn the_scan_would_see_a_platform_branch() {
        let masked = crate::source_guards::mask_non_code(
            "#[cfg(target_os = \"macos\")]\nfn only_here() {}\n",
        );
        assert!(
            masked.contains("target_os"),
            "마스킹이 cfg 속성까지 지운다 — 그러면 이 가드는 아무것도 못 본다"
        );
        let commented = crate::source_guards::mask_non_code("// target_os 는 여기서 안 쓴다\n");
        assert!(
            !commented.contains("target_os"),
            "주석 안의 이름을 위반으로 집는다 — 규칙을 설명하는 문장마다 빨개진다"
        );
    }
}
