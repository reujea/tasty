//! 문서와 소스 주석이 가리키는 파일·디렉터리의 존재를 확인한다.
//! 백틱 이름 바로 뒤 괄호에 경로가 있으면 해당 파일 안에 그 식별자가 있는지도 확인한다.
//! Markdown 링크는 문서 위치를 기준으로 풀고, 일반 경로 인용은 저장소 루트와 소속 크레이트 루트에서 찾는다.
//!
//! Markdown은 코드 펜스를 제외한 본문을 읽는다. 다른 형식은 등록된 줄 주석 접두어로만 고른다.
//! 완전한 언어 파서는 아니며, 형식별 제외 대상은 UNJUDGED_FORMS에 기록한다.
//! 링크와 이름·경로 인접 짝은 Markdown에서만 검사한다.
//! 중괄호·와일드카드 축약과 경로 없는 식별자는 판정하지 않는다.
//! 파일시스템의 존재 여부를 확인하므로 미추적 파일도 통과할 수 있다.
//! 인용이 현재 설명에 적합한지는 검사하지 않는다. 작성 원칙은 docs/documentation-model.md를 따른다.

// 이유: 테스트의 반환값 무시는 제품 코드의 lint 예외 명부에 포함하지 않는다.
#![allow(clippy::let_underscore_must_use)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 저장소 경로로 해석할 접두어. 외부 저장소 경로는 해당 저장소 이름을 앞에 붙여 구별한다.
const ROOT_PREFIXES: &[&str] = &[
    "src/",
    "tests/",
    "crates/",
    "scripts/",
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
    ".idea",
    // 개발자 도구의 캐시는 프로젝트 소스가 아니므로 순회에서 제외한다.
    // 이 검사는 Git 추적 여부 대신 디렉터리 이름으로 제외 대상을 구분한다.
    ".serena",
    ".playwright-mcp",
    "node_modules",
    "_site",
    // Astro 캐시는 표식이 없어 이름으로 제외한다.
    ".astro",
    // vendor는 외부 원본이라 다음 동기화가 수정을 덮어쓴다.
    // 이름으로 제외하므로 그 안에서 직접 작성한 PATCHES.md도 검사되지 않는다.
    "vendor",
];

/// 로컬 작업 폴더 이름은 추적 문서에서 인용을 금지하므로 조각으로 조립한다(docs/documentation-model.md).
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

/// 이름 제외뿐 아니라 빌드 캐시 표식도 확인해 임의 CARGO_TARGET_DIR를 제외한다.
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

/// Markdown 링크는 인용 문서의 디렉터리에서 해석한다.
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

/// 공유 바이너리 제외 목록에 더해 이 검사에서 제외하는 형식.
const EXTRA_SKIP_EXTS: &[&str] = &["lock", "svg"];

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

/// 형식별 줄 주석 접두어. 미지원 형식은 인용을 판독하지 않는다.
fn comment_prefixes(rel: &str) -> Option<&'static [&'static str]> {
    let name = rel.rsplit('/').next().unwrap_or("");
    let ext = name
        .trim_start_matches('.')
        .rsplit_once('.')
        .map(|(_, e)| e);
    match ext {
        Some("rs" | "m" | "mjs" | "jsx" | "ts" | "tsx" | "astro") => Some(&["//"]),
        Some("toml" | "sh" | "bash" | "yml" | "yaml" | "py" | "just" | "ps1") => Some(&["#"]),
        Some("lua") => Some(&["--"]),
        // 블록 주석의 첫 줄(`/*`)과 이어지는 줄(` *`)도 읽는다.
        Some("c" | "h") => Some(&["//", "/*", "*"]),
        None if matches!(
            name,
            "Justfile"
                | "justfile"
                | "pre-commit"
                | "pre-push"
                | "pre-merge-commit"
                | ".complexity-file-allowlist"
                | ".complexity-frozen-files"
        ) =>
        {
            Some(&["#"])
        }
        _ => None,
    }
}

/// Markdown은 본문, 다른 형식은 등록된 접두어로 시작하는 줄만 읽는다.
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

/// 가상의 예시만 (파일, 인용) 쌍으로 면제한다. 파일 전체를 면제하지 않는다.
/// 빌드 산출물은 예외에 넣지 않고 추적 문서로 안내한다.
/// 존재 확인은 디스크 기준이라 산출물 인용이 로컬에서 통과하고 clean clone에서는 실패할 수 있다.
const ALLOWLIST: &[(&str, &str)] = &[
    // 워크플로 판독 설명의 가상 타깃.
    (
        "crates/tasty-doc-guards/tests/ci_channel_claims_match_workflows.rs",
        "tests/X.rs",
    ),
    // 컴파일러 오류 정규식의 예시.
    ("crates/tasty-output/src/parsers/errors.rs", "src/foo.c"),
    ("crates/tasty-output/src/parsers/errors.rs", "src/foo.ts"),
    // 링크 검출 설명의 가상 크레이트.
    (
        "crates/tasty-terminal-link/src/lib.rs",
        "crates/x/Cargo.toml",
    ),
];

fn is_allowed(rel: &str, cited: &str) -> bool {
    ALLOWLIST.contains(&(rel, cited))
}

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
        // worktree의 .git은 파일이므로 종류를 확인하기 전에 이름으로 제외한다.
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

/// 소속 크레이트 루트를 찾는다. 저장소 루트는 이미 시도하므로 중복 반환하지 않는다.
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
        judged > 1800,
        "경로 인용을 {judged}개만 판정했다. 수집 범위와 경로 추출을 확인한다."
    );
    assert!(
        violations.is_empty(),
        "인용한 저장소 경로가 없다. 판정 {judged}건 중 {}건:\n{}\n이동한 파일은 현재 경로로 고친다. 외부 저장소 경로는 저장소 이름을 앞에 붙여 구별한다. 생성물은 이를 설명하는 추적 문서로 안내한다. 가상 예시만 ALLOWLIST에 (파일, 인용) 쌍으로 등록한다.",
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
    // 수집 누락을 확인하는 보조 하한이며 현재 인용 수나 품질 기준을 뜻하지 않는다.
    assert!(
        judged > 200,
        "디렉터리 인용을 {judged}개만 판정했다. 수집 범위와 추출을 확인한다."
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

/// 더 긴 식별자의 일부를 일치로 세지 않도록 토큰 경계를 확인한다.
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
    assert_eq!(scan_paths("`src/state.rs:17-18`"), ["src/state.rs"]);

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
    assert_eq!(
        scan_pairs("`repo_root`(`Root`, `src/lib.rs`)"),
        [("repo_root".to_string(), vec!["src/lib.rs".to_string()])]
    );
    assert!(scan_pairs("`repo_root` 는 어딘가 (`src/lib.rs`)").is_empty());
    assert!(scan_pairs("`repo_root`(순수 함수)").is_empty());
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
    assert!(scan_dirs("`src/main.rs`").is_empty());
    assert!(scan_dirs("src/core/ 아래").is_empty());
    assert!(scan_dirs("`src/{a,b}/`").is_empty(), "중괄호 축약");
    assert!(scan_dirs("`vendor/x/`").is_empty(), "모르는 최상위");
}

#[test]
fn a_dead_coordinate_is_caught_and_a_live_one_is_not() {
    let root = &tasty_doc_guards::repo_root();
    let live = "crates/tasty-doc-guards/tests/cited_coordinates_exist.rs";
    assert_eq!(scan_paths(&format!("`{live}`")), [live]);
    assert!(
        resolve(root, "docs/x.md", live).is_some(),
        "존재하는 경로를 해석하지 못했다"
    );
    let dead = "crates/tasty-doc-guards/tests/cited_coordinates_exist.rss";
    assert!(resolve(root, "docs/x.md", dead).is_none());

    assert!(
        resolve(root, "crates/tasty-doc-guards/README.md", "src/lib.rs").is_some(),
        "크레이트 루트 기준 경로를 해석하지 못했다"
    );
    assert_eq!(crate_root_of(root, "docs/dev-guide/build.md"), None);

    assert!(resolve_dir(root, "docs/x.md", "crates/tasty-doc-guards/tests").is_some());
    assert!(resolve_dir(root, "docs/x.md", "crates/tasty-doc-guards/no_such_dir").is_none());

    let body = std::fs::read_to_string(root.join(live)).expect("자기 소스를 읽는다");
    assert!(contains_token(&body, "scan_pairs"));
    // 소스 자체를 검사하므로 없는 이름을 리터럴로 적으면 그 이름이 존재하게 된다. 조각으로 조립한다.
    let absent = format!("{}{}", "scan_pairs_", "with_no_such_name");
    assert!(!contains_token(&body, &absent));
    assert!(!contains_token("fn scan_pairs_more() {}", "scan_pairs"));
}

// Markdown 링크는 루트 접두어 없이도 문서 위치에서 해석한다.

/// 백틱 안의 링크 문법 예시를 실제 링크로 검사하지 않도록 제거한다.
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

/// 복사 후 위치를 기준으로 링크를 적는 템플릿만 예외로 둔다. 이름 접두어로 다른 문서까지 제외하지 않는다.
const LINK_EXEMPT_DOCS: &[&str] = &["docs/features/_feature.template.md"];

/// 외부 URL·같은 문서 앵커·루트 절대 경로를 제외한 인라인 링크 대상.
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

/// 링크 대상의 존재만 확인한다. 설명에 적합한 대상인지, 링크가 여전히 필요한지는 판단하지 않는다.
#[test]
fn cited_markdown_links_resolve_from_their_own_document() {
    let root = &tasty_doc_guards::repo_root();
    let docs = docs_of(root);
    assert!(!docs.is_empty(), "문서를 읽지 못해 인용을 검사할 수 없다");

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

    println!("[인용 좌표] 확인한 링크 {checked} · 하한 {MIN_LINKS}");
    assert!(
        checked >= MIN_LINKS,
        "링크를 {checked}개만 찾았다. 문서 수집과 링크 추출을 확인한다."
    );
    // 이동한 문서와 아직 통합되지 않은 문서는 수정 방법이 다르므로 실패 안내에서 구분한다.
    assert!(
        broken.is_empty(),
        "문서 위치에서 해석되지 않는 Markdown 링크다:\n{}\n대상이 이동했다면 문서 디렉터리 기준으로 링크를 고친다. 다른 작업에서 아직 통합되지 않았다면 담당자와 대상 경로를 확인하고 통합 때 연결한다. Git 기록에 경로가 없다는 사실만으로 어느 경우인지 단정하지 않는다.",
        broken.join("\n")
    );
}

/// 링크 수집 실패를 찾는 보조 하한(docs/dev-guide/guard-verification.md).
/// 2026-09-05 실측 3154의 절반보다 낮은 1500으로 정했다. 현재 링크 수를 뜻하지 않는다.
/// 문서 편집에 따른 증감을 허용하는 여유이며 개별 누락을 검출하지 않는다.
/// 파서 변경으로 측정 범위가 크게 달라지면 --nocapture 출력의 실제 수를 확인해 하한을 다시 정한다.
const MIN_LINKS: usize = 1500;

#[test]
fn the_link_scanner_reads_only_repo_local_link_targets() {
    assert_eq!(scan_links("[a](0147-x.md)"), vec!["0147-x.md"]);
    assert_eq!(
        scan_links("[a](../dev-guide/x.md)"),
        vec!["../dev-guide/x.md"]
    );
    assert_eq!(scan_links("[a](x.md#절)"), vec!["x.md"]);

    assert!(scan_links("[a](https://example.com/x.md)").is_empty());
    assert!(scan_links("[a](mailto:x@example.com)").is_empty());
    assert!(scan_links("[a](#절)").is_empty());
    assert!(scan_links("[a](/etc/passwd)").is_empty());

    assert!(scan_links("일반 상대링크 `[text](문서명.md)` 와 같은 모양").is_empty());

    assert!(LINK_EXEMPT_DOCS.iter().all(|rel| root_has(rel)));
}

#[test]
fn a_link_is_resolved_next_to_the_document_that_cites_it() {
    let root = &tasty_doc_guards::repo_root();

    assert!(link_resolves(root, "docs/adr/0146-x.md", "template.md"));
    assert!(link_resolves(root, "docs/adr/0146-x.md", "../index.md"));

    assert!(root.join("CLAUDE.md").is_file());
    assert!(!link_resolves(root, "docs/adr/0146-x.md", "CLAUDE.md"));

    let absent = format!("{}{}", "0133-scan-guards-", "assert-their-population.md");
    assert!(!link_resolves(root, "docs/adr/0146-x.md", &absent));
    assert!(!link_resolves(
        root,
        "docs/adr/0146-x.md",
        &format!("../{absent}")
    ));
}

fn root_has(rel: &str) -> bool {
    tasty_doc_guards::repo_root().join(rel).is_file()
}

#[test]
fn only_comment_lines_of_a_source_file_are_read_as_citations() {
    let src = "//! 좌표는 `src/a.rs` 다.\nlet re = \"src/b.rs\";\n// 그리고 `src/c.rs`.\n";
    let picked: Vec<&str> = citation_lines("crates/x/src/lib.rs", src)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert_eq!(picked.len(), 2, "주석 두 줄만 골라야 한다: {picked:?}");
    assert!(picked.iter().all(|l| l.trim_start().starts_with("//")));

    assert_eq!(citation_lines("docs/x.md", src).len(), 3);
}

#[test]
fn objective_c_line_comments_are_checked_without_treating_code_as_citations() {
    let root = tasty_doc_guards::repo_root();
    let present = "docs/dev-guide/guard-population.md";
    let missing = format!("src/{}{}", "objective_c_absent_citation", ".rs");
    let source = format!(
        "// {present}\n  // {missing}\n#import <Foundation/Foundation.h>\n\
         NSString *path = @\"src/string_literal.rs\";\n/* src/block_comment.rs */\n"
    );
    assert!(is_scan_target("control.m"));
    let lines = citation_lines("tests/control.m", &source);
    assert_eq!(lines.iter().map(|(n, _)| *n).collect::<Vec<_>>(), [1, 2]);
    let paths: Vec<_> = lines
        .iter()
        .flat_map(|(_, line)| scan_paths(line))
        .collect();
    assert_eq!(paths, [present.to_string(), missing.clone()]);
    assert!(resolve(&root, "tests/control.m", present).is_some());
    assert!(resolve(&root, "tests/control.m", &missing).is_none());
}

#[test]
fn a_format_whose_comment_syntax_is_unknown_is_not_judged() {
    assert!(comment_prefixes("crates/x/src/lib.rs").is_some());
    assert!(comment_prefixes("Cargo.toml").is_some());
    assert!(comment_prefixes("scripts/x.sh").is_some());
    assert!(comment_prefixes("src/x/ext.c").is_some());
    assert!(comment_prefixes(".githooks/pre-commit").is_some());
    assert!(comment_prefixes("site/index.html").is_none());
    assert!(citation_lines("site/index.html", "<img src=\"assets/x.svg\">").is_empty());
}

/// worktree의 .git 파일을 실제 체크아웃 종류와 무관하게 검증하도록 합성 트리를 사용한다.
#[test]
fn pruning_is_by_name_not_by_kind() {
    let dir = std::env::temp_dir().join(format!("tasty-doc-guards-prune-{}", std::process::id()));
    // 앞선 실행이 남긴 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target")).expect("임시 디렉토리를 못 만들었다");
    std::fs::write(dir.join("target").join("buried.md"), "x").expect("쓰기 실패");
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

#[test]
fn a_build_dir_under_another_name_is_still_pruned() {
    let dir = std::env::temp_dir().join(format!("tasty-cited-prune-{}", std::process::id()));
    // 이유: 이전 실행의 임시 경로가 없어도 된다. 아래에서 필요한 파일을 다시 만든다.
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

/// 주석을 판독하지 않는 형식을 이유와 함께 기록한다. 같은 형식의 파일 추가마다 예외를 늘리지 않는다.
const UNJUDGED_FORMS: &[(&str, &str)] = &[
    (
        "DS_Store",
        "주석 문법이 없다 — Finder 가 폴더마다 남기는 바이너리 메타데이터다",
    ),
    ("json", "주석 문법이 없다 — 데이터다"),
    (
        "jsonl",
        "주석 문법이 없다 — 한 줄에 JSON 값 하나인 측정 데이터다",
    ),
    (
        "SOURCE",
        "주석 문법이 없다 — 디자인 정본 커밋 해시 한 줄인 데이터다(crates/tasty-design-tokens/dtcg/SOURCE)",
    ),
    ("txt", "주석 문법이 없다 — 데이터·픽스처다"),
    ("rtf", "주석 문법이 없다 — 서식 문서다"),
    (
        "LICENSE",
        "라이선스 전문 — 주석 문법이 없고 본문은 상류 템플릿 그대로다",
    ),
    ("wxs", "XML 주석 판독을 지원하지 않는 WiX 정의다"),
    (
        "html",
        "HTML 주석 판독을 지원하지 않는다. 생성물과 자산이다",
    ),
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
    // JS·CSS는 외부 최소화 자산의 경로 데이터를 인용으로 오해할 수 있어 제외한다. 직접 작성한 자산도 함께 제외되는 한계가 있다.
    ("js", "vendored 최소화 자산이 경로 꼴 데이터를 낸다"),
    ("css", "vendored 최소화 자산이 경로 꼴 데이터를 낸다"),
    (
        "wit",
        "WIT 인터페이스 정의 — 주석은 있으나 인용을 싣지 않는다",
    ),
];

/// 확장자가 없으면 선행 점을 뗀 파일명을 형식 이름으로 사용한다.
fn form_of(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or("");
    let trimmed = name.trim_start_matches('.');
    trimmed
        .rsplit_once('.')
        .map_or_else(|| trimmed.to_string(), |(_, e)| e.to_ascii_lowercase())
}

/// 미등록 형식의 누락을 찾는다. 마지막 파일이 삭제됐다는 이유로 기존 형식 선언을 오류로 보지는 않는다.
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
        "제외되는 파일을 찾지 못해 제외 범위를 대조할 수 없다"
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

#[test]
fn the_traversal_skips_binaries_and_keeps_extensionless_files() {
    assert!(is_scan_target("lib.rs"));
    assert!(is_scan_target("Justfile"));
    assert!(is_scan_target(".gitignore"));
    assert!(!is_scan_target("icon.png"));
    assert!(!is_scan_target("tasty-plugin.toml.sig"));
    assert!(!is_scan_target("Cargo.lock"));
}

/// 오래된 예외가 새 오류를 숨기지 않도록 파일·인용이 남아 있고 대상은 여전히 없는지 확인한다.
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

/// 링크 파서와 별개로 문서 수집도 입력에 따라 빈 결과와 실제 문서를 구별하는지 확인한다.
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

    assert_eq!(docs_of(&root).len(), 0, "빈 디렉터리에서 문서가 수집됐다");

    std::fs::write(root.join("a.md"), "[x](b.md)\n").expect("쓰기 실패");
    assert!(
        !docs_of(&root).is_empty(),
        "합성 문서를 수집하지 못해 빈 디렉터리와 구별할 수 없다"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
