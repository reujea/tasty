//! 문서가 인용한 **좌표가 실제로 풀리는가** 를 본다. 두 축이다.
//!
//! ① **경로 축** — 마크다운 문서가 레포 경로 형태(`src/…` · `tests/…` · `crates/…` 등)로
//!    적은 파일 경로는 실재해야 한다.
//! ② **디렉토리 축** — 백틱 안에서 `/` 로 끝나는 같은 형태는 디렉토리로 실재해야 한다.
//! ③ **인접 짝 축** — `` `이름`(`경로`) `` 처럼 이름 바로 뒤 괄호가 파일을 지목하면,
//!    그 이름은 그 파일 안에 있어야 한다.
//!
//! ## 왜 필요한가 — 없는 것을 가리키는 좌표는 "주어 없음" 보다 나쁘다
//!
//! 주어 없이 "가드가 강제한다" 고만 적힌 문장은 읽는 사람이 확인할 수 없다는 것이
//! 눈에 보인다. 반면 **틀린 좌표는 검증된 것처럼 보인다** — 이름과 경로가 붙어 있으니
//! 아무도 다시 세지 않는다. 이 저장소의 실측에서 그 형태가 실제로 있었다: 사라진
//! 파일을 근거로 든 ADR, 크레이트 루트 기준 약칭을 레포 루트 경로처럼 적어 **다른
//! 실재 파일로 조용히 해석되는** 인용, 남의 저장소(egui) 내부 경로를 우리 경로 형태로
//! 적은 참조.
//!
//! ## 무엇을 훑는가 — "무엇을 검사하는가" 만큼 초록의 범위를 정한다
//!
//! 순회는 레포 전체다(바이너리·산출물 확장자와 가지치기 디렉토리만 뺀다). 한동안
//! `.md` 만 훑었고, 그동안 소스·매니페스트 주석의 죽은 좌표는 **빨강 없이 살아
//! 있었다** — 실측으로 스물 남짓이었다. 술어가 옳아도 훑지 않는 자리에서는 아무것도
//! 말하지 않는다. 그래서 순회 범위를 판정 규칙과 같은 무게로 여기 적는다.
//!
//! 다만 축마다 훑는 범위가 다르고, 그 차이는 **문법이 다르기 때문**이다.
//! 경로 축·디렉토리 축은 레포 경로 리터럴을 보므로 언어를 안 탄다 — 레포 전체를
//! 훑는다. 반면 링크 축(`[a](b.md)`)과 인접 짝 축(`` `이름`(`경로`) ``)은 마크다운
//! 표기라 `.md` 안에서만 그 뜻이다. Rust 의 `` [`Self::foo`] `` 는 같은 모양이지만
//! intra-doc 링크로, 파일 경로가 아니다. 그 둘까지 넓히면 판정의 뜻이 파일 종류마다
//! 달라지므로 넓히지 않는다.
//!
//! **비-`.md` 에서는 주석 줄만 본다.** [`citation_lines`] 가 그 경계다 — 코드가
//! 만드는 문자열 안의 경로는 그 코드의 입력이지 읽는 사람에게 준 좌표가 아니다.
//!
//! ## 판정 규칙 — 문맥을 추론하지 않는다
//!
//! 이 가드는 문장의 뜻을 분류하지 않는다. 경로도 이름도 **리터럴**이고, 판정은
//! "그 자리에 파일이 있는가 / 그 파일에 그 토큰이 있는가" 뿐이다. 산문 패턴으로 문맥을
//! 나누는 검출기는 양방향으로 틀리면서 초록일 때 아무것도 보장하지 못한다 — 그 함정을
//! 피하려고 판정 대상을 **좌표를 스스로 들고 있는 인용**으로만 좁혔다.
//!
//! **경로 해석은 두 자리에서 시도한다.** 레포 루트, 그리고 인용한 문서가 크레이트 안에
//! 살면 그 크레이트 루트. 크레이트 README·CHANGELOG 가 `src/…` 를 자기 크레이트 기준으로
//! 적는 것은 정당한 관례이고, 그것을 예외 목록으로 덮는 대신 해석 규칙에 넣었다.
//!
//! **중괄호·와일드카드 축약은 판정 대상이 아니다.** `src/{a,b}.rs` 같은 형태는 여러
//! 경로를 한 번에 쓰는 표기라 단일 파일로 풀 수 없다. 축약을 받아주는 것이 아니라
//! **판정할 수 없는 것을 판정하지 않는 것**이고, 그만큼이 이 가드의 사각이다.
//!
//! 중괄호를 펼쳐서 판정하는 것도 재봤다 — 원소 25 개 중 12 개가 안 풀렸는데, 그 절반이
//! **접두 자체가 약칭**이라 그렇다(`crates/tasty-shm/{lib.rs, footer.rs}` 는 실제로
//! `crates/tasty-shm/src/` 아래다). 즉 위반이 "그 파일이 없다" 를 뜻하지 않는다. 판정의
//! 뜻이 하나가 아니면 초록도 빨강도 못 읽으므로 펼치지 않는다.
//!
//! ## 오차 방향
//!
//! **놓치는 쪽으로 틀린다.** 경로 형태가 아닌 인용(백틱 안의 맨 식별자, 디렉토리 인용,
//! 남의 저장소 경로를 크레이트 이름 없이 적은 것 중 우연히 우리 파일과 겹치는 것)은
//! 판정 밖이다. 존재 판정을 파일시스템으로 하므로 **git 이 추적하지 않지만 디스크에는
//! 있는 파일**도 통과한다. 반대 방향(있는 것을 없다고 하는 것)은 초록이 아니라 빨강으로
//! 나오므로 조용히 넘어가지 않는다.
//!
//! ## 이 가드가 덮지 않는 것 — 밝혀 둔다
//!
//! 문서가 백틱으로 인용하는 **맨 snake_case 식별자**(테스트 이름 · 함수 이름)는 좌표가
//! 함께 적히지 않는 한 판정하지 않는다. 그 형태를 보는 가드는 이 저장소에 **하나도
//! 없다** — 조용히 안 만든 것이 아니라 여기 적어 둔다. 손으로 재는 절차와 그 절차의
//! 함정은 `docs/documentation-model.md` §6 에 있다.
//!
//! 선례: `crates/tasty-doc-guards/tests/no_checkbox_in_docs.rs`(docs 순회 구조) ·
//! `crates/tasty-doc-guards/tests/no_todo_file_citation.rs`(레포 전체 스캔).
//!
//! 판정을 리터럴 좌표로 좁히고 오탐을 예외 목록이 아니라 인용 형태로 없애는 결정의 근거·대안:
//! `docs/adr/0151-cited-coordinates-are-judged-as-literals-not-by-context.md`.

// 이유: 이 타깃은 전부 테스트다. 테스트의 `let _ =` 는 정책이 사유를 요구하지
// 않으므로 `clippy::let_underscore_must_use` 명부(프로덕션 전용)에 섞이면 안 된다
// — docs/dev-guide/error-handling.md.
#![allow(clippy::let_underscore_must_use)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 레포 경로로 읽히는 최상위 디렉토리. 이 목록에 없는 접두(예 `egui/src/…`)는 애초에
/// 우리 경로로 읽히지 않으므로 판정 대상이 아니다 — 남의 저장소 내부 경로를 인용할 때
/// 크레이트 이름을 앞에 붙이면 그것만으로 모호함이 사라진다.
const ROOT_PREFIXES: &[&str] = &[
    "src/",
    "tests/",
    "crates/",
    "scripts/",
    "site/",
    "lang/",
    "assets/",
    "docs/",
    "benches/",
    "examples/",
    ".github/",
    ".cargo/",
];

/// 순회에서 통째로 가지치기할 디렉토리명.
const PRUNE_DIRS: &[&str] = &[
    "target",
    "dist",
    ".worktree",
    ".git",
    "node_modules",
    "_site",
    // Astro 의 자체 캐시 — `astro dev` 를 한 번이라도 돌린 트리에만 있고 gitignored 다.
    // 표식(`CACHEDIR.TAG`)을 안 달아 성질로는 못 가르지만 이름은 그 도구가 박아 뒀다.
    ".astro",
    // 남의 코드를 그대로 들여놓는 자리 — `site/vendor/` 가 그것이다(디자인 시스템
    // 원본을 외부 프로젝트에서 동기화해 온다). 그 안의 주석은 우리가 쓴 좌표가 아니고,
    // 실제로 아직 없는 자리를 "(planned)" 로 든다. 여기서 판정하면 처방이 **upstream 을
    // 고치라**가 되는데 그 파일은 다음 동기화가 덮어쓴다. `js`·`css` 를 `UNJUDGED_FORMS`
    // 에 둔 것과 같은 부류이고, 이름으로 가르는 것은 `vendor` 가 관례로 고정된 이름이라
    // 가능하다. 이름으로 가르므로 루트 `vendor/`(`[patch]` 로 끼운 상류 사본
    // `vendor/tiny_http`)도 같은 부류로 빠진다 — 그 안에서 우리가 쓴 `PATCHES.md` 의 인용까지.
    "vendor",
];

/// gitignored 로컬 폴더 이름의 조각. 리터럴로 두면 이 파일이 비-git 경로 참조 금지
/// (`docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`) 를 어긴다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

/// 이름으로 걸리거나, **디렉토리 자신이 빌드 캐시라고 밝히거나**.
///
/// 이름만 보면 `CARGO_TARGET_DIR` 로 만든 다른 이름의 빌드 디렉토리가 통째로 모수에
/// 들어온다 — 실측(2026-09-06) 그런 디렉토리 하나가 생기자 이 가드의 형식 판정이
/// 8251 건을 "수집됐는데 판정되지 않았다" 로 세고 빨개졌다. 형제 가드
/// (`no_todo_file_citation.rs`) 는 같은 비대칭을 먼저 겪고 표식 판정으로 닫았는데
/// 이쪽은 이름만 본 채로 남아 있었다.
///
/// 판정 근거와 후보 비교는 [`tasty_doc_guards::is_build_cache_dir`].
fn is_pruned_dir(path: &Path, name: &str) -> bool {
    is_pruned(name) || tasty_doc_guards::is_build_cache_dir(path)
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-')
}

/// 경로 인용이 시작할 수 있는 자리인지 — 앞 글자가 경로 글자면 더 긴 경로의 일부다.
fn is_boundary(prev: Option<char>) -> bool {
    !prev.is_some_and(is_path_char)
}

/// 마지막 조각에 1~5 글자 소문자/숫자 확장자가 붙어 있는가. 디렉토리 인용을 거른다.
fn has_file_extension(p: &str) -> bool {
    let last = p.rsplit('/').next().unwrap_or("");
    let Some((stem, ext)) = last.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && (1..=5).contains(&ext.len())
        && ext
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// 링크 대상을 **인용한 문서의 디렉토리** 기준으로 푼다.
///
/// 이 한 줄이 축의 전부라 따로 뽑았다 — 본 판정 안에 인라인으로 두면 합성 픽스처로
/// 극성을 못 건드리고, 회귀를 실재하는 죽은 링크에 걸 수밖에 없다. 그러면 그 링크를
/// 고치는 순간 회귀가 거짓 초록이 된다.
fn link_resolves(root: &Path, doc_rel: &str, target: &str) -> bool {
    let dir = Path::new(doc_rel).parent().unwrap_or(Path::new(""));
    root.join(dir).join(target).exists()
}

/// 한 줄에서 레포 경로 형태 인용을 뽑는다. 중괄호 축약(`…/{a,b}.rs`)은 뺀다.
fn scan_paths(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if !is_boundary(i.checked_sub(1).map(|k| chars[k])) {
            i += 1;
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let Some(prefix) = ROOT_PREFIXES.iter().find(|p| rest.starts_with(**p)) else {
            i += 1;
            continue;
        };
        let mut end = i + prefix.chars().count();
        while end < chars.len() && is_path_char(chars[end]) {
            end += 1;
        }
        // 중괄호 축약은 여러 경로를 한 번에 쓰는 표기라 단일 파일로 풀 수 없다.
        let braced = chars.get(end) == Some(&'{');
        let token: String = chars[i..end]
            .iter()
            .collect::<String>()
            .trim_end_matches(['.', '-', '/'])
            .to_string();
        if !braced && !token.contains('*') && has_file_extension(&token) {
            out.push(token);
        }
        i = end.max(i + 1);
    }
    out
}

/// 한 줄에서 백틱으로 감싼 **디렉토리 인용**(`/` 로 끝나는 레포 경로 형태)을 뽑는다.
/// 백틱을 요구하는 이유는 산문 안의 슬래시 표기(`A/B` 선택지 등)와 갈리기 위해서다.
fn scan_dirs(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '`' {
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&k| chars[k] == '`') else {
            break;
        };
        let span: String = chars[i + 1..close].iter().collect();
        i = close + 1;
        if !span.ends_with('/') || span.contains('*') || span.contains('{') {
            continue;
        }
        if ROOT_PREFIXES.iter().any(|p| span.starts_with(*p))
            && span.chars().all(|c| is_path_char(c) || c == '/')
        {
            out.push(span.trim_end_matches('/').to_string());
        }
    }
    out
}

/// 백틱 span 이 snake_case 식별자인가 — 소문자로 시작하고 밑줄이 하나 이상.
fn is_snake_ident(s: &str) -> bool {
    !s.is_empty()
        && s.contains('_')
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.ends_with('_')
}

/// 한 줄에서 `` `이름`(… `경로` …) `` 인접 짝을 뽑는다. 괄호는 같은 줄에서 닫혀야 한다.
fn scan_pairs(line: &str) -> Vec<(String, Vec<String>)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '`' {
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&k| chars[k] == '`') else {
            break;
        };
        let name: String = chars[i + 1..close].iter().collect();
        i = close + 1;
        if !is_snake_ident(&name) {
            continue;
        }
        let mut j = i;
        while chars.get(j) == Some(&' ') {
            j += 1;
        }
        if chars.get(j) != Some(&'(') {
            continue;
        }
        let Some(rparen) = (j + 1..chars.len()).find(|&k| chars[k] == ')') else {
            continue;
        };
        let inner: String = chars[j + 1..rparen].iter().collect();
        let paths = scan_paths(&inner);
        if !paths.is_empty() {
            out.push((name, paths));
        }
        i = rparen + 1;
    }
    out
}

/// 이 가드가 좌표 인용을 안 찾아 공용 denylist 위에 **더** 빼는 형식. `.svg`·`.lock` 은
/// 우리 소스 경로 좌표를 담지 않는다 — 바이너리 판정은 정본
/// [`tasty_doc_guards::is_binary_artifact_ext`] 가 하고, 이 목록은 그 위에 얹는 이 가드의
/// 모수 축소다(ADR-0180: 판정은 하나, 스캔 범위는 소비자별).
const EXTRA_SKIP_EXTS: &[&str] = &["lock", "svg"];

/// 파일 하나가 스캔 대상인가 — 공용 바이너리 denylist 위에 [`EXTRA_SKIP_EXTS`] 를 더
/// 뺀다(denylist 전수).
fn is_scan_target(name: &str) -> bool {
    let ext = name
        .trim_start_matches('.')
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext {
        Some(e) => {
            !tasty_doc_guards::is_binary_artifact_ext(&e) && !EXTRA_SKIP_EXTS.contains(&e.as_str())
        }
        None => true,
    }
}

/// 확장자·파일명 → 그 언어의 줄 주석 접두. 목록에 없는 형식은 **판정하지 않는다**
/// — 주석 문법을 모르면 산문과 데이터를 가를 수 없고, 못 가르면 데이터를 인용으로
/// 읽어 거짓 빨강을 만든다.
fn comment_prefixes(rel: &str) -> Option<&'static [&'static str]> {
    let name = rel.rsplit('/').next().unwrap_or("");
    let ext = name
        .trim_start_matches('.')
        .rsplit_once('.')
        .map(|(_, e)| e);
    match ext {
        Some("rs" | "mjs" | "jsx" | "ts" | "tsx" | "astro") => Some(&["//"]),
        Some("toml" | "sh" | "bash" | "yml" | "yaml" | "py" | "just" | "ps1") => Some(&["#"]),
        Some("lua") => Some(&["--"]),
        None if matches!(
            name,
            "Justfile"
                | "justfile"
                | "pre-commit"
                | "pre-push"
                | "pre-merge-commit"
                | ".complexity-file-allowlist"
        ) =>
        {
            Some(&["#"])
        }
        _ => None,
    }
}

/// 판정할 줄을 고른다.
///
/// **`.md` 는 본문 전체가 산문이고, 소스는 주석만이 산문이다.** 코드가 만드는 문자열
/// 안의 경로는 그 코드의 입력이지 읽는 사람에게 준 좌표가 아니다 — 컴파일러 에러
/// 정규식이 드는 가짜 소스 이름, 워크플로 파서의 픽스처 이름, 생성 HTML 의 `href`
/// 웹 경로가 전부 그 부류다. 실측으로 그 셋이 위반의 대부분이었다.
///
/// **이 주석 자신이 그 함정이다** — 예시를 레포 경로 꼴로 적으면 이 가드가 자기
/// 설명을 인용으로 읽는다. 그래서 여기서는 형태로 든다.
fn citation_lines<'a>(rel: &str, contents: &'a str) -> Vec<(usize, &'a str)> {
    let lines = prose_lines(contents);
    if rel.ends_with(".md") {
        return lines;
    }
    let Some(prefixes) = comment_prefixes(rel) else {
        return Vec::new();
    };
    lines
        .into_iter()
        .filter(|(_, l)| {
            let t = l.trim_start();
            prefixes.iter().any(|p| t.starts_with(p))
        })
        .collect()
}

/// 면제 — **(파일, 인용) 짝 단위**다. 파일 통째를 빼면 그 파일이 새로 들이는 진짜
/// 죽은 좌표까지 조용히 통과한다.
///
/// 한 부류만 있다 — 그 인용이 **예시**인 자리다. 가드·파서가 설명을 위해 지어낸
/// 이름이라 실재하면 오히려 이상하다.
///
/// **빌드 산출물은 면제로 담지 않는다.** 한때 둘을 담았는데, 그 파일들이 gitignored
/// 산출물이라 **빌드를 돌린 트리에서만 실재**했다 — 같은 커밋이 어떤 워크스페이스에서는
/// 빨갛고 갓 clone 한 트리에서는 초록이 됐다. 판정이 빌드 상태에 좌우된 것이다.
/// 면제를 늘리는 대신 인용 쪽을 고쳤다: 산출물은 레포 경로 꼴로 적지 않고, 따라갈
/// 곳이 필요하면 그 산출물을 설명하는 **추적되는** 문서를 가리킨다. 그것이 이 가드가
/// 실패 메시지에 적어 둔 처방 (c) 다.
///
/// 남은 흔들림 하나는 본 판정에 있다 — `resolve` 가 파일시스템에 묻는다. 산출물을
/// 가리키는 인용은 빌드한 트리에서 초록이고 갓 clone 한 트리에서 빨갛다. 안전한
/// 방향이라(CI 의 fresh checkout 이 잡는다) 여기서 닫지 않았다.
const ALLOWLIST: &[(&str, &str)] = &[
    // ① 예시 — 워크플로 파싱 설명이 지어낸 가드 이름.
    (
        "crates/tasty-doc-guards/tests/ci_channel_claims_match_workflows.rs",
        "tests/X.rs",
    ),
    // ① 예시 — 규칙 본문이 "이렇게 적으면 안 된다" 로 드는 이름.
    (
        "crates/tasty-doc-guards/tests/no_todo_file_citation.rs",
        "docs/CLAUDE.md",
    ),
    // ① 예시 — 컴파일러 에러 줄 정규식의 샘플 입력.
    ("crates/tasty-output/src/parsers/errors.rs", "src/foo.c"),
    ("crates/tasty-output/src/parsers/errors.rs", "src/foo.ts"),
    // ① 예시 — 터미널 링크 검출 설명이 드는 가상의 크레이트.
    (
        "crates/tasty-terminal-link/src/lib.rs",
        "crates/x/Cargo.toml",
    ),
    // ③ ADR 본문의 **결정 시점 좌표.** ADR 템플릿의 "좌표 예외" 는 Context·Decision 의
    //    코드 인용을 결정 시점 기준으로 규정하고 **낡았다는 이유로 고치는 것을 금지한다**
    //    — 그 부재가 결정이 실행됐다는 증거라서다. 그러니 이 부류는 형태를 고쳐 닫을 수
    //    없다. 아래 둘은 사이트가 Astro 로 옮겨 가며 사라진 러스트 생성기이고, 그 ADR 은
    //    ADR-0247 이 대체했다.
    (
        "docs/adr/0201-slug-rules-are-scoped-by-the-tree-that-renders-them.md",
        "site/src/md.rs",
    ),
    (
        "docs/adr/0201-slug-rules-are-scoped-by-the-tree-that-renders-them.md",
        "site/src/main.rs",
    ),
];

fn is_allowed(rel: &str, cited: &str) -> bool {
    ALLOWLIST.contains(&(rel, cited))
}

/// 스캔 대상 파일을 모은다.
fn gather(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(is_scan_target)
        {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        // **이름으로 가지친다 — 종류를 묻지 않는다.** worktree 에서 `.git` 은 디렉토리가
        // 아니라 `gitdir:` 한 줄이 든 파일이라, `is_dir()` 을 먼저 물으면 그 파일이
        // 가지치기를 빠져나가 문서로 읽힌다. 그러면 같은 커밋이 메인 체크아웃과
        // worktree 에서 서로 다른 모집단을 본다 — 이 가드의 답은 환경이 아니라
        // 코드에서 나와야 한다.
        if is_pruned_dir(&p, p.file_name().and_then(|n| n.to_str()).unwrap_or("")) {
            continue;
        }
        gather(&p, out);
    }
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 인용 문서가 사는 크레이트 루트(repo-relative). 레포 루트 자신은 돌려주지 않는다 —
/// 레포 루트도 크레이트라 그것까지 돌려주면 해석이 두 번 같은 자리를 본다.
fn crate_root_of(root: &Path, rel_doc: &str) -> Option<String> {
    let mut dir = Path::new(rel_doc).parent()?;
    loop {
        let s = dir.to_string_lossy().replace('\\', "/");
        if s.is_empty() {
            return None;
        }
        if root.join(&s).join("Cargo.toml").is_file() {
            return Some(s);
        }
        dir = dir.parent()?;
    }
}

/// 레포 루트 또는 인용 문서의 크레이트 루트 기준으로 푼다.
fn resolve(root: &Path, rel_doc: &str, cited: &str) -> Option<PathBuf> {
    let direct = root.join(cited);
    if direct.is_file() {
        return Some(direct);
    }
    let crate_rel = crate_root_of(root, rel_doc)?;
    let nested = root.join(&crate_rel).join(cited);
    nested.is_file().then_some(nested)
}

/// 레포 루트 또는 인용 문서의 크레이트 루트 기준으로 디렉토리를 푼다.
fn resolve_dir(root: &Path, rel_doc: &str, cited: &str) -> Option<PathBuf> {
    let direct = root.join(cited);
    if direct.is_dir() {
        return Some(direct);
    }
    let crate_rel = crate_root_of(root, rel_doc)?;
    let nested = root.join(&crate_rel).join(cited);
    nested.is_dir().then_some(nested)
}

/// 코드 펜스 밖의 줄만 (1-based 줄번호와 함께) 돌려준다.
fn prose_lines(contents: &str) -> Vec<(usize, &str)> {
    let mut fence = false;
    let mut out = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if !fence {
            out.push((i + 1, line));
        }
    }
    out
}

fn docs_of(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    gather(root, &mut files);
    files.sort();
    files
        .iter()
        .filter_map(|f| {
            std::fs::read_to_string(f)
                .ok()
                .map(|c| (rel_of(f, root), c))
        })
        .collect()
}

#[test]
fn cited_repo_paths_resolve() {
    let root = &tasty_doc_guards::repo_root();
    let docs = docs_of(root);
    assert!(
        !docs.is_empty(),
        "스캔 대상 파일이 0 이다 — 순회 경로가 틀어졌다"
    );

    let mut judged = 0usize;
    let mut violations = Vec::new();
    for (rel, contents) in &docs {
        for (line_no, line) in citation_lines(rel, contents) {
            for cited in scan_paths(line) {
                if is_allowed(rel, &cited) {
                    continue;
                }
                judged += 1;
                if resolve(root, rel, &cited).is_none() {
                    violations.push(format!("  {rel}:{line_no} — `{cited}`"));
                }
            }
        }
    }
    assert!(
        judged > 2500,
        "판정한 경로 인용이 {judged} 개뿐이다 — 검출기가 죽었을 때도 이 테스트는 초록이 \
         되므로 모수를 함께 본다"
    );
    assert!(
        violations.is_empty(),
        "문서가 인용한 레포 경로가 실재하지 않는다 — 읽는 사람에게는 확인된 좌표처럼 \
         보이지만 따라갈 곳이 없다. 판정 {judged} 회 중 {} 회:\n{}\n\
         고치는 법: (a) 옮겨졌으면 현재 경로로, (b) 남의 저장소 경로면 크레이트 이름을 \
         앞에 붙여(`egui/src/style.rs`) 우리 경로 형태에서 빼고, (c) 생성물이거나 실재한 \
         적이 없으면 경로 인용 대신 서술로 적는다. 면제는 [`ALLOWLIST`] 에 **(파일, 인용) \
         짝**으로만 두고, 그 두 부류(예시 · 빌드 산출물) 밖은 형태를 \
         고치면 부류가 닫힌다.",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn cited_repo_directories_resolve() {
    let root = &tasty_doc_guards::repo_root();
    let docs = docs_of(root);
    let mut judged = 0usize;
    let mut violations = Vec::new();
    for (rel, contents) in &docs {
        for (line_no, line) in citation_lines(rel, contents) {
            for cited in scan_dirs(line) {
                judged += 1;
                if resolve_dir(root, rel, &cited).is_none() {
                    violations.push(format!("  {rel}:{line_no} — `{cited}/`"));
                }
            }
        }
    }
    assert!(
        judged > 300,
        "판정한 디렉토리 인용이 {judged} 개뿐이다 — 검출기가 죽었을 때도 초록이 되므로 \
         모수를 함께 본다"
    );
    assert!(
        violations.is_empty(),
        "문서가 인용한 레포 디렉토리가 실재하지 않는다. 판정 {judged} 회 중 {} 회:\n{}\n\
         고치는 법은 경로 축과 같다 — 옮겼으면 현재 경로로, 없어졌거나 아직 없으면 경로 \
         인용을 빼고 서술로 적는다.",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn names_paired_with_a_file_live_in_that_file() {
    let root = &tasty_doc_guards::repo_root();
    let docs = docs_of(root);
    let mut cache: HashMap<PathBuf, String> = HashMap::new();
    let mut judged = 0usize;
    let mut violations = Vec::new();

    for (rel, contents) in &docs {
        if !rel.ends_with(".md") {
            continue;
        }
        for (line_no, line) in prose_lines(contents) {
            for (name, cited_paths) in scan_pairs(line) {
                let resolved: Vec<PathBuf> = cited_paths
                    .iter()
                    .filter_map(|p| resolve(root, rel, p))
                    .collect();
                if resolved.is_empty() {
                    continue; // 경로 축이 따로 신고한다.
                }
                judged += 1;
                let found = resolved.iter().any(|p| {
                    let body = cache
                        .entry(p.clone())
                        .or_insert_with(|| std::fs::read_to_string(p).unwrap_or_default());
                    contains_token(body, &name)
                });
                if !found {
                    violations.push(format!(
                        "  {rel}:{line_no} — `{name}` 이 {cited_paths:?} 에 없다"
                    ));
                }
            }
        }
    }
    assert!(
        judged > 20,
        "인접 짝 판정이 {judged} 회뿐이다 — 검출기가 죽었는지 모수로 확인한다"
    );
    assert!(
        violations.is_empty(),
        "이름 바로 뒤 괄호가 지목한 파일에 그 이름이 없다 — 옮겼거나 이름이 바뀌었다. \
         판정 {judged} 회 중 {} 회:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// 식별자 토큰이 통째로 등장하는지. 부분문자열이면 더 긴 이름에 걸려 오탐한다.
fn contains_token(body: &str, name: &str) -> bool {
    let bytes = body.as_bytes();
    let mut from = 0usize;
    while let Some(off) = body[from..].find(name) {
        let start = from + off;
        let end = start + name.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[test]
fn the_path_scanner_takes_repo_shaped_citations_only() {
    // 잡아야 하는 것 — 백틱 안팎, 문장 끝 마침표, 크레이트 경로, 점 파일.
    assert_eq!(
        scan_paths("코드는 `src/state.rs` 에 있다"),
        ["src/state.rs"]
    );
    assert_eq!(
        scan_paths("경로는 tests/layering.rs."),
        ["tests/layering.rs"]
    );
    assert_eq!(
        scan_paths("`.github/workflows/format-check.yml` 이 본다"),
        [".github/workflows/format-check.yml"]
    );
    assert_eq!(
        scan_paths("`crates/tasty-doc-guards/Cargo.toml` 은 비어 있다"),
        ["crates/tasty-doc-guards/Cargo.toml"]
    );
    // 줄 범위 접미는 경로가 아니다 — `:` 에서 끊는다.
    assert_eq!(scan_paths("`src/state.rs:17-18`"), ["src/state.rs"]);

    // 잡지 않아야 하는 것.
    assert!(scan_paths("`src/engine/` 아래").is_empty(), "디렉토리 인용");
    assert!(scan_paths("`src/{a,b}.rs`").is_empty(), "중괄호 축약");
    assert!(scan_paths("`src/**/*.rs`").is_empty(), "와일드카드");
    assert!(
        scan_paths("`egui/src/style.rs`").is_empty(),
        "남의 크레이트 접두"
    );
    assert!(scan_paths("`vendor/src/x.rs`").is_empty(), "모르는 최상위");
    assert!(
        scan_paths("`~/.tasty/tasty.port`").is_empty(),
        "홈 런타임 경로"
    );
    // 더 긴 경로의 내부는 따로 잡지 않는다 — 한 인용은 한 번만 판정한다.
    assert_eq!(
        scan_paths("`crates/tasty-model/src/lib.rs`"),
        ["crates/tasty-model/src/lib.rs"]
    );
}

#[test]
fn the_pair_scanner_needs_the_paren_right_after_the_name() {
    assert_eq!(
        scan_pairs("`repo_root`(`crates/tasty-doc-guards/src/lib.rs`) 가 판다"),
        [(
            "repo_root".to_string(),
            vec!["crates/tasty-doc-guards/src/lib.rs".to_string()]
        )]
    );
    // 괄호 안에 타입이 함께 와도 경로만 뽑는다.
    assert_eq!(
        scan_pairs("`repo_root`(`Root`, `src/lib.rs`)"),
        [("repo_root".to_string(), vec!["src/lib.rs".to_string()])]
    );
    // 이름과 괄호가 떨어져 있으면 짝이 아니다.
    assert!(scan_pairs("`repo_root` 는 어딘가 (`src/lib.rs`)").is_empty());
    // 괄호에 경로가 없으면 판정 대상이 아니다.
    assert!(scan_pairs("`repo_root`(순수 함수)").is_empty());
    // snake_case 가 아닌 백틱 span 은 이름이 아니다.
    assert!(scan_pairs("`Root`(`src/lib.rs`)").is_empty());
    assert!(scan_pairs("`cargo test`(`src/lib.rs`)").is_empty());
}

#[test]
fn the_dir_scanner_needs_backticks_and_a_trailing_slash() {
    assert_eq!(scan_dirs("등록은 `src/core/` 아래"), ["src/core"]);
    assert_eq!(
        scan_dirs("`crates/tasty-doc-guards/tests/` 가 산다"),
        ["crates/tasty-doc-guards/tests"]
    );
    // 파일은 경로 축이 본다 — 여기서 두 번 세지 않는다.
    assert!(scan_dirs("`src/main.rs`").is_empty());
    // 백틱이 없으면 산문의 슬래시 표기와 갈리지 않는다.
    assert!(scan_dirs("src/core/ 아래").is_empty());
    assert!(scan_dirs("`src/{a,b}/`").is_empty(), "중괄호 축약");
    assert!(scan_dirs("`vendor/x/`").is_empty(), "모르는 최상위");
}

#[test]
fn a_dead_coordinate_is_caught_and_a_live_one_is_not() {
    let root = &tasty_doc_guards::repo_root();
    // 살아 있는 좌표 — 이 파일 자신.
    let live = "crates/tasty-doc-guards/tests/cited_coordinates_exist.rs";
    assert_eq!(scan_paths(&format!("`{live}`")), [live]);
    assert!(
        resolve(root, "docs/x.md", live).is_some(),
        "실재하는 경로를 못 푼다 — 판정이 반대로 서 있다"
    );
    // 죽은 좌표 — 같은 자리에서 확장자만 바꾼 것.
    let dead = "crates/tasty-doc-guards/tests/cited_coordinates_exist.rss";
    assert!(resolve(root, "docs/x.md", dead).is_none());

    // 크레이트 상대 해석: 크레이트 안의 문서가 적은 `src/lib.rs` 는 그 크레이트 것이다.
    assert!(
        resolve(root, "crates/tasty-doc-guards/README.md", "src/lib.rs").is_some(),
        "크레이트 루트 기준 해석이 죽었다"
    );
    // 크레이트 밖 문서에는 그 해석이 없다.
    assert_eq!(crate_root_of(root, "docs/dev-guide/build.md"), None);

    // 디렉토리 축도 같은 양극성.
    assert!(resolve_dir(root, "docs/x.md", "crates/tasty-doc-guards/tests").is_some());
    assert!(resolve_dir(root, "docs/x.md", "crates/tasty-doc-guards/no_such_dir").is_none());

    // 인접 짝: 이 파일에 있는 이름과 없는 이름.
    let body = std::fs::read_to_string(root.join(live)).expect("자기 소스를 읽는다");
    assert!(contains_token(&body, "scan_pairs"));
    // 없는 이름은 조각으로 조립한다 — 리터럴로 적으면 이 파일 자신이 그 이름을 갖게 돼
    // 판정이 뒤집힌다. 파서가 자기 소스를 시험대로 삼을 때의 기본 함정이다.
    let absent = format!("{}{}", "scan_pairs_", "with_no_such_name");
    assert!(!contains_token(&body, &absent));
    // 부분문자열은 토큰이 아니다.
    assert!(!contains_token("fn scan_pairs_more() {}", "scan_pairs"));
}

// ─── ④ 링크 축 — 마크다운 링크는 **문서 자신의 자리**에서 푼다 ─────────────────
//
// 위 세 축은 인용을 레포 루트(또는 크레이트 루트) 기준으로 푼다. 그 해석은 산문 인용에는
// 맞지만 **마크다운 링크에는 안 맞는다** — 링크는 렌더러가 그 문서의 디렉토리 기준으로
// 따라간다. 그래서 형제 파일을 가리키는 `](0147-….md)` 같은 링크는 루트 접두가 없어
// ① 의 `ROOT_PREFIXES` 에 아예 안 걸리고, 죽어 있어도 조용하다. 실측(2026-09-05)으로
// 그 형태의 죽은 링크가 ADR 에 실재했다.

/// 인라인 코드 스팬(백틱)을 지운 줄. 링크 **문법을 설명하는 예시**(`` `[text](url)` ``)를
/// 실제 링크로 세지 않기 위해서다. 실측에서 안 지웠을 때 걸린 다섯 건이 전부 그 형태였다.
fn without_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            out.push(ch);
        }
    }
    out
}

/// 링크 축의 면제 — **파일 단위로 열거한다.**
///
/// 템플릿은 복사돼 갈 자리를 기준으로 링크를 적는다. `docs/features/<범주>/<이름>.md` 로
/// 복사되면 `../../concepts/…` 가 `docs/concepts/…` 로 풀리지만, 템플릿 자신의 자리에서는
/// 레포 밖을 가리킨다. 자기 자리에서 안 풀리는 것이 **정상인 유일한 갈래**라 이름 규칙
/// 대신 파일을 적는다 — 이름으로 면제하면 `_` 로 시작하는 아무 문서나 같이 빠진다.
const LINK_EXEMPT_DOCS: &[&str] = &["docs/features/_feature.template.md"];

/// 한 줄에서 **레포 안을 가리키는 마크다운 링크 대상**을 뽑는다.
///
/// 밖으로 나가는 것(스킴 있는 URL), 같은 문서 안의 앵커, 루트 절대 경로는 이 판정의
/// 대상이 아니다 — 앞의 둘은 파일이 아니고, 절대 경로는 렌더러마다 기준이 다르다.
fn scan_links(line: &str) -> Vec<String> {
    let line = without_inline_code(line);
    let mut out = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == ']' && bytes[i + 1] == '(' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != ')' && !bytes[j].is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == ')' {
                let target: String = bytes[i + 2..j].iter().collect();
                let head = target.split('#').next().unwrap_or("").to_string();
                let external = target.contains("://")
                    || target.starts_with("mailto:")
                    || target.starts_with('#')
                    || target.starts_with('/');
                if !external && !head.is_empty() {
                    out.push(head);
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// 마크다운 링크가 **그 문서의 자리에서** 풀리는가.
///
/// 초록의 뜻은 "링크 대상이 실재한다" 까지다. 그 문서가 옳은 것을 가리키는지, 그 링크가
/// 아직 필요한지는 안 본다.
#[test]
fn cited_markdown_links_resolve_from_their_own_document() {
    let root = &tasty_doc_guards::repo_root();
    let docs = docs_of(root);
    assert!(
        !docs.is_empty(),
        "문서를 하나도 못 읽었다 — 모수가 0 이면 언제나 초록이다"
    );

    let mut checked = 0usize;
    let mut broken = Vec::new();
    for (rel, contents) in &docs {
        if LINK_EXEMPT_DOCS.contains(&rel.as_str()) {
            continue;
        }
        if !rel.ends_with(".md") {
            continue;
        }
        for (line_no, line) in prose_lines(contents) {
            for target in scan_links(line) {
                checked += 1;
                if !link_resolves(root, rel, &target) {
                    broken.push(format!("  {rel}:{line_no} — `{target}`"));
                }
            }
        }
    }

    // 측정값은 단정보다 앞에. 이 줄이 생기기 전의 판별식은 "상수를 크게 올려 실패
    // 메시지로 읽어라" 였는데, 그건 재려고 트리를 건드리게 한다(상수 doc 참조).
    println!("[인용 좌표] 확인한 링크 {checked} · 하한 {MIN_LINKS}");
    assert!(
        checked >= MIN_LINKS,
        "링크를 {checked} 개밖에 못 봤다 — 스캐너가 깨졌다면 이 단정이 조용히 통과한다"
    );
    // 실패문이 원인 둘을 갈라 싣는 이유 — **처방이 반대**라서다. 옮겨진 대상은 링크를
    // 고쳐야 하고, 아직 안 온 대상은 링크를 **지우면 안 된다**. 이 빨강을 "없는 좌표" 로
    // 읽고 인용을 지우면 합쳐진 뒤에도 그 참조는 안 생긴다.
    //
    // 둘째 갈래의 뒷문장(좌표를 남겨라)이 여기 있는 이유는, 이 빨강을 본 사람이 **바로 그
    // 자리에서** 산문으로 낮출지를 정하기 때문이다. 전제는 산문이 아니라 실패 문구에 산다.
    //
    // ★ 이 문구가 못 닿는 갈래가 있다: 링크를 **아예 안 걸어 본** 사람. 그 자리는 빨강이
    // 안 나므로 이 문구를 못 읽고, 남긴 문장이 "그 문서는 아직 이 트리에 없다" 뿐이면
    // 좌표가 없어 나중에 훑어도 안 나온다. 그 형태가 실측으로 하나 있었다. 링크 없음은
    // 위반이 아니고 그 문장은 사실 주장이 아니라 절차 서술이라 **어떤 좌변에도 안 걸린다** —
    // 이 판정기가 넓어져서 닿을 자리가 아니라 쓰는 사람만 아는 자리다.
    assert!(
        broken.is_empty(),
        "마크다운 링크가 그 문서의 자리에서 안 풀린다. 원인이 둘이고 **처방이 반대다**:\n\
         \n\
         ① 대상이 **옮겨졌다** — 링크도 옮겨라. 링크는 레포 루트가 아니라 **그 문서의 \
         디렉토리** 기준이다.\n\
         ② 대상이 **아직 이 트리에 안 왔다** — 같은 회차의 다른 lane 이 만드는 중이면 \
         네 트리에 없는 것이 정상이다. **인용을 지우지 마라.** 링크를 산문으로 낮추고 \
         **좌표를 남겨라**(번호·파일 이름처럼 나중에 훑어서 찾을 수 있는 형태로). 회차 \
         끝에 통합이 링크로 승격한다.\n\
         \n\
         가르는 명령 한 줄 — `git log --all --diff-filter=A -- <대상 경로>` 가 0 건이면 \
         ② 다(그 경로가 이 레포에 생긴 적이 없다).\n{}",
        broken.join("\n")
    );
}

/// 본 판정이 보는 링크 수의 하한 — **연기 검사**다. 스캐너가 깨져 0 을 내면 위 단정이
/// 언제나 초록이 된다(ADR-0133).
///
/// 값은 실측의 절반 아래로 잡았다 — 문서가 늘고 주는 것으로는 안 깨지고 스캐너가
/// 무너질 때만 걸리게. **이 하한을 고를 때 실측은 3154 였다**(2026-09-05).
///
/// 그 수는 **지금 값을 주장하지 않는다** — 하한을 왜 1500 으로 잡았는지에 대한
/// 과거형 사실이라 실제 링크 수가 달라져도 안 낡는다. ADR-0139 가 칸 1 을 바꾸라고
/// 준 세 형태 중 ②(과거형 사실)이고, 하한 자신은 칸 3(결정된 임계값)이라 그냥 적는다.
/// 이 주석은 전에 그 ADR 을 "수를 적지 마라" 로 읽고 근거를 지웠었다 — 그 ADR 은
/// 금지가 아니라 변환 규칙이다.
///
/// **판별식** — 종전에 이 자리가 적어 둔 재는 법은 "지금 몇인지 알아야 하면 이 상수를
/// 크게 올려 실패 메시지로 읽는다" 였다. 답은 나오지만 **재려고 트리를 건드려야 한다.**
/// 하한을 확인하는 일이 소스 수정을 요구하면, 재는 사람이 그 수정을 되돌리는 것을
/// 잊는 형태가 남는다. 그래서 단정 앞에서 실측값을 찍게 바꿨다:
///
/// ```text
/// cargo test -p tasty-doc-guards --test cited_coordinates_exist -- --nocapture
///   → [인용 좌표] 확인한 링크 <N> · 하한 1500
/// ```
///
/// 실측 2026-09-07(`de0572359`): **3390** (하한을 고를 때의 3154 에서 늘었다).
/// **여유가 1890 으로 실측의 절반이 넘는다** — 이 자리는 그것이 의도다. 링크 수는 문서를
/// 쓰기만 해도 흔들려서, 하한을 실측에 붙이면 정상 편집마다 빨개진다. 이 하한이 잡는 것은
/// "스캐너가 통째로 무너졌다" 하나이고 그 사고는 수를 절반 밑으로 떨군다.
/// ★ 그래서 이 자리의 여유는 앞의 형제 가드들과 성격이 다르다 — 저쪽은 모수가 사람 손으로만
/// 변해서 붙여 둘 수 있고, 이쪽은 모수 자체가 문서 편집마다 움직인다.
///
/// **이 수를 올려서 "지금 값" 을 박지 마라.** 그러면 문서를 한 편 지울 때마다 빨개지고,
/// 그때 가장 싼 초록화가 다시 이 수를 내리는 것이라 래칫이 실측을 따라다니기만 한다.
///
/// 정당한 수선: 스캐너를 고쳐서 링크 수가 크게 달라졌으면 위 명령으로 새 실측을 읽고,
/// 그 절반 언저리로 이 수를 다시 골라라 — 실측에 붙이는 것이 아니라 **무너짐을 잡는
/// 문턱**으로 고르는 것이 이 상수의 성질이다.
const MIN_LINKS: usize = 1500;

/// 링크 축의 극성을 픽스처로 못박는다 — **실재하는 문서를 상대로만 시험하면 그 문서를
/// 고치는 순간 이 회귀가 거짓 초록이 된다.**
#[test]
fn the_link_scanner_reads_only_repo_local_link_targets() {
    // 잡는다 — 형제 경로, 상위 경로, 앵커가 붙은 것.
    assert_eq!(scan_links("[a](0147-x.md)"), vec!["0147-x.md"]);
    assert_eq!(
        scan_links("[a](../dev-guide/x.md)"),
        vec!["../dev-guide/x.md"]
    );
    assert_eq!(scan_links("[a](x.md#절)"), vec!["x.md"]);

    // 안 잡는다 — 밖으로 나가는 것, 같은 문서 앵커, 루트 절대 경로.
    assert!(scan_links("[a](https://example.com/x.md)").is_empty());
    assert!(scan_links("[a](mailto:x@example.com)").is_empty());
    assert!(scan_links("[a](#절)").is_empty());
    assert!(scan_links("[a](/etc/passwd)").is_empty());

    // 인라인 코드 안의 링크 **문법 예시**는 링크가 아니다.
    assert!(scan_links("일반 상대링크 `[text](문서명.md)` 와 같은 모양").is_empty());

    // 면제는 파일 단위다 — 이름 규칙이 아니다.
    assert!(LINK_EXEMPT_DOCS.iter().all(|rel| root_has(rel)));
}

/// 해석의 **양극성**을 합성 픽스처로 못박는다.
///
/// 이름을 조각에서 조립하는 것은 이 소스 자신이 그 이름을 담아 판정을 뒤집는 것을
/// 막기 위해서다 — 이 파일도 스캔 대상이다.
#[test]
fn a_link_is_resolved_next_to_the_document_that_cites_it() {
    let root = &tasty_doc_guards::repo_root();

    // 산다 — ADR 이 형제 ADR 을 루트 접두 없이 가리키는 형태. ① 의 ROOT_PREFIXES 로는
    // 안 보이는 바로 그 모양이다.
    assert!(link_resolves(root, "docs/adr/0146-x.md", "template.md"));
    // 산다 — 상위로 올라가는 형태.
    assert!(link_resolves(root, "docs/adr/0146-x.md", "../index.md"));

    // 죽는다 — 같은 이름이 **레포 루트에는** 있어도 그 문서 옆에는 없다.
    assert!(root.join("CLAUDE.md").is_file());
    assert!(!link_resolves(root, "docs/adr/0146-x.md", "CLAUDE.md"));

    // 죽는다 — 아무 데도 없는 이름.
    let absent = format!("{}{}", "0133-scan-guards-", "assert-their-population.md");
    assert!(!link_resolves(root, "docs/adr/0146-x.md", &absent));
    assert!(!link_resolves(
        root,
        "docs/adr/0146-x.md",
        &format!("../{absent}")
    ));
}

/// 면제가 가리키는 문서가 실재하는가 — 참조 무결성. 면제가 썩으면 그 문서는 검사받게
/// 되는데 목록에는 "여기는 안 풀려도 된다" 는 신호가 남는다.
fn root_has(rel: &str) -> bool {
    tasty_doc_guards::repo_root().join(rel).is_file()
}

/// `.md` 는 본문 전체가, 소스는 주석만이 판정 대상이다.
#[test]
fn only_comment_lines_of_a_source_file_are_read_as_citations() {
    let src = "//! 좌표는 `src/a.rs` 다.\nlet re = \"src/b.rs\";\n// 그리고 `src/c.rs`.\n";
    let picked: Vec<&str> = citation_lines("crates/x/src/lib.rs", src)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert_eq!(picked.len(), 2, "주석 두 줄만 골라야 한다: {picked:?}");
    assert!(picked.iter().all(|l| l.trim_start().starts_with("//")));

    // 같은 내용을 `.md` 로 주면 코드 줄까지 산문이다.
    assert_eq!(citation_lines("docs/x.md", src).len(), 3);
}

/// 주석 문법을 모르는 형식은 **판정하지 않는다.** 모르는 채로 훑으면 데이터를
/// 인용으로 읽어 거짓 빨강이 된다.
#[test]
fn a_format_whose_comment_syntax_is_unknown_is_not_judged() {
    assert!(comment_prefixes("crates/x/src/lib.rs").is_some());
    assert!(comment_prefixes("Cargo.toml").is_some());
    assert!(comment_prefixes("scripts/x.sh").is_some());
    assert!(comment_prefixes(".githooks/pre-commit").is_some());
    assert!(comment_prefixes("site/index.html").is_none());
    assert!(citation_lines("site/index.html", "<img src=\"assets/x.svg\">").is_empty());
}

/// **모집단이 환경을 읽으면 답도 환경을 읽는다.** worktree 의 `.git` 은 파일이고
/// 메인 체크아웃의 `.git` 은 디렉토리다 — 가지치기가 종류를 물으면 앞쪽에서만
/// 그 파일이 문서로 읽혀 두 트리의 모집단이 갈린다. 실재하는 레포를 상대로 시험하면
/// 이 회귀가 체크아웃 종류에 따라 조용히 사라지므로 임시 디렉토리로 형태를 짓는다.
#[test]
fn pruning_is_by_name_not_by_kind() {
    let dir = std::env::temp_dir().join(format!("tasty-doc-guards-prune-{}", std::process::id()));
    // 앞선 실행이 남긴 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target")).expect("임시 디렉토리를 못 만들었다");
    std::fs::write(dir.join("target").join("buried.md"), "x").expect("쓰기 실패");
    // worktree 의 형태 — `.git` 이 디렉토리가 아니라 파일이다.
    std::fs::write(dir.join(".git"), "gitdir: elsewhere\n").expect("쓰기 실패");
    std::fs::write(dir.join("keep.md"), "x").expect("쓰기 실패");

    let mut files = Vec::new();
    gather(&dir, &mut files);
    let mut seen: Vec<String> = files.iter().map(|f| rel_of(f, &dir)).collect();
    seen.sort();
    // 단정 전에 치운다 — 실패해도 다음 실행이 위에서 다시 치우므로 막을 이유가 없다.
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        seen,
        vec!["keep.md".to_string()],
        "가지치기가 종류를 물었다 — worktree 의 `.git` 파일이 모집단에 들어왔다"
    );
}

/// **이름이 아닌 근거로도 가지치기된다.** 이 절이 없으면 `is_pruned_dir` 이 이름
/// 판정으로 퇴화해도 위 테스트가 초록이라 — 다른 이름의 빌드 디렉토리가 모수에 다시
/// 들어온 것을 아무도 못 본다. 양극성으로 잡는다: 표식이 있으면 걸리고, 이름이 빌드
/// 디렉토리처럼 보여도 표식이 없으면 안 걸린다.
#[test]
fn a_build_dir_under_another_name_is_still_pruned() {
    let dir = std::env::temp_dir().join(format!("tasty-cited-prune-{}", std::process::id()));
    // 정리 실패는 무시한다 — 임시 디렉토리라 남아도 판정에 영향이 없고, 여기서 죽으면
    // 진짜 실패가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리를 못 만들었다");

    assert!(
        !is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 없으면 이름이 빌드 디렉토리처럼 보여도 가지치기하지 않는다"
    );

    std::fs::write(
        dir.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("표식을 못 썼다");
    assert!(
        is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 있으면 이름과 무관하게 가지치기한다"
    );

    // 정리 실패는 무시한다 — 위와 같은 이유다.
    let _ = std::fs::remove_dir_all(&dir);
}

/// 수집됐는데 **한 줄도 판정되지 않는** 형식은 형식 단위로 선언한다 — 이유까지.
///
/// `comment_prefixes` 가 `None` 을 돌려주면 그 파일은 **조용히** 빠진다. 조용하면
/// 아무도 안 본다 — 실측으로 그 사각에 진짜 죽은 좌표가 하나 앉아 있었다
/// (`.complexity-file-allowlist` 이 `#` 주석으로 든 ADR 경로가 다른 번호로 착지했다).
///
/// 명부를 **파일이 아니라 형식**으로 잡는 이유: 파일 단위면 `.json` 하나가 새로
/// 들어올 때마다 낡는다. 형식 단위면 평범한 작업으로는 안 낡고, **새 형식이 들어올
/// 때만** 빨개져 판단을 요구한다.
const UNJUDGED_FORMS: &[(&str, &str)] = &[
    ("json", "주석 문법이 없다 — 데이터다"),
    ("txt", "주석 문법이 없다 — 데이터·픽스처다"),
    ("rtf", "주석 문법이 없다 — 서식 문서다"),
    (
        "LICENSE",
        "라이선스 전문 — 주석 문법이 없고 본문은 상류 템플릿 그대로다",
    ),
    ("wxs", "주석 문법이 없다(XML) — WiX 정의다"),
    ("html", "주석 문법이 없다(XML 계열) — 생성물이거나 자산이다"),
    (
        "desktop",
        "freedesktop 항목 파일 — 값이 경로 꼴이라 산문과 못 가른다",
    ),
    (
        "gitignore",
        "무시 규칙 자체가 경로 목록이다 — 인용이 아니다",
    ),
    (
        "gitattributes",
        "속성 규칙 자체가 경로 패턴이다 — 인용이 아니다",
    ),
    // 아래 셋은 주석 문법이 **있는데도** 안 본다. vendored 최소화 자산이 경로 꼴
    // 문자열을 데이터로 뱉기 때문이다 — 실측: `mermaid.min.js` 하나가 실재하지 않는
    // 경로 6 건을 낸다. 우리가 쓴 `site/static/*` 까지 함께 빠지는 것이 이 선택의
    // 대가이고, 그쪽 인용은 오늘 전부 풀린다.
    ("js", "vendored 최소화 자산이 경로 꼴 데이터를 낸다"),
    ("css", "vendored 최소화 자산이 경로 꼴 데이터를 낸다"),
    (
        "wit",
        "WIT 인터페이스 정의 — 주석은 있으나 인용을 싣지 않는다",
    ),
];

/// 형식 이름을 판다 — 확장자, 없으면 선행 점을 뗀 파일명.
fn form_of(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or("");
    let trimmed = name.trim_start_matches('.');
    trimmed
        .rsplit_once('.')
        .map_or_else(|| trimmed.to_string(), |(_, e)| e.to_ascii_lowercase())
}

/// **명부 밖에 대상이 없다는 것까지 함께 단정한다**(그 반대 방향만 보면 명부가
/// 낡아도 조용하다). 반대 방향(명부의 죽은 항목)은 일부러 안 본다 — 그건 어떤 형식의
/// 마지막 파일이 지워지는 것만으로 빨개져, 평범한 작업이 게이트를 흔든다.
#[test]
fn every_gathered_but_unjudged_file_declares_its_format() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, &mut files);
    assert!(
        !files.is_empty(),
        "순회가 비었다 — 이 단정은 그 상태에서 아무 뜻이 없다"
    );

    let declared: std::collections::BTreeSet<&str> =
        UNJUDGED_FORMS.iter().map(|(f, _)| *f).collect();
    let mut unjudged = 0usize;
    let mut undeclared = Vec::new();
    for f in &files {
        let rel = rel_of(f, root);
        if comment_prefixes(&rel).is_some() || rel.ends_with(".md") {
            continue;
        }
        unjudged += 1;
        let form = form_of(&rel);
        if !declared.contains(form.as_str()) {
            undeclared.push(format!("  {rel} — 형식 `{form}`"));
        }
    }
    assert!(
        unjudged > 0,
        "판정에서 빠지는 파일이 하나도 없다 — 이 단정이 헛돌고 있다는 뜻이다"
    );
    assert!(
        undeclared.is_empty(),
        "수집됐는데 판정되지 않는 형식이 선언 밖에 있다 {} 건. 주석 문법이 있으면 \
         `comment_prefixes` 에 넣고, 없거나 일부러 안 본다면 이유와 함께 \
         `UNJUDGED_FORMS` 에 넣어라:\n{}",
        undeclared.len(),
        undeclared.join("\n")
    );
}

/// 순회가 바이너리·산출물을 빼고 확장자 없는 파일은 담는가.
#[test]
fn the_traversal_skips_binaries_and_keeps_extensionless_files() {
    assert!(is_scan_target("lib.rs"));
    assert!(is_scan_target("Justfile"));
    assert!(is_scan_target(".gitignore"));
    assert!(!is_scan_target("icon.png"));
    assert!(!is_scan_target("tasty-plugin.toml.sig"));
    assert!(!is_scan_target("Cargo.lock"));
}

/// **면제 목록은 썩는다.** 가리키던 자리가 고쳐지거나 사라져도 항목은 남고, 남은
/// 항목은 그 파일이 나중에 들이는 진짜 죽은 좌표를 조용히 덮는다. 그래서 항목마다
/// ① 파일이 실재하고 ② 그 인용이 실제로 그 파일에서 나오고 ③ 지금도 안 풀리는지를
/// 함께 본다 — 셋 중 하나라도 아니면 그 항목은 지워야 한다.
#[test]
fn every_allowlist_entry_still_fires() {
    let root = &tasty_doc_guards::repo_root();
    assert!(
        !ALLOWLIST.is_empty(),
        "면제 목록이 비었다 — 이 테스트가 아무것도 안 본다"
    );
    let mut stale = Vec::new();
    for (rel, cited) in ALLOWLIST {
        let Ok(contents) = std::fs::read_to_string(root.join(rel)) else {
            stale.push(format!("  {rel} — 파일이 없다"));
            continue;
        };
        let emitted = citation_lines(rel, &contents)
            .iter()
            .any(|(_, line)| scan_paths(line).iter().any(|c| c == cited));
        if !emitted {
            stale.push(format!("  {rel} — `{cited}` 가 더는 나오지 않는다"));
            continue;
        }
        if resolve(root, rel, cited).is_some() {
            stale.push(format!("  {rel} — `{cited}` 가 이제 실재한다"));
        }
    }
    assert!(
        stale.is_empty(),
        "면제 항목이 낡았다 — 지워라. {} 건:\n{}",
        stale.len(),
        stale.join("\n")
    );
}

/// [`MIN_LINKS`] 의 **양성 대조** — 수집이 죽으면 이 수가 떨어지나.
///
/// 이 수도 누적 변수가 아니라 합성이다: `docs_of(root)` 가 문서를 모으고
/// `prose_lines` · `scan_links` 가 그 안에서 링크를 뽑는다. **뒤 둘은 이미 대조가 있다**
/// (`the_link_scanner_reads_only_repo_local_link_targets` ·
/// `a_link_is_resolved_next_to_the_document_that_cites_it`). 없던 것은 앞의 수집 하나뿐이라
/// 그것만 채운다 — `docs_of` 는 이 파일의 시험 넷이 쓰므로 이 대조는 그 넷 전부를 받친다.
#[test]
fn the_link_floor_sees_a_collapsed_collection() {
    let root = std::env::temp_dir().join(format!(
        "tasty-linkfloor-{}-{}",
        std::process::id(),
        line!()
    ));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("임시 디렉토리를 못 만들었다");

    assert_eq!(
        docs_of(&root).len(),
        0,
        "빈 뿌리에서 0 이 아니면 이 수집기는 입력을 안 보는 것이고, 그러면 하한도 이 파일의 \
         다른 시험 넷도 무엇을 훑는지 모른 채 초록이 된다"
    );

    // 비영 대조 — 언제나 0 을 내는 것은 아니다.
    std::fs::write(root.join("a.md"), "[x](b.md)\n").expect("쓰기 실패");
    assert!(
        !docs_of(&root).is_empty(),
        "문서를 하나도 못 모으면 위 0 은 수집기가 늘 죽어 있다는 뜻이라 아무것도 안 지킨다"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
