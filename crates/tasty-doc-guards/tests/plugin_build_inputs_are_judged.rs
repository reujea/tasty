//! 플러그인 버전 검사(scripts/check-plugin-version-bump.sh)가 비교하지 않는 빌드 입력이 생기면 실패한다.
//! 그 스크립트는 바뀐 파일이 build_affecting 패턴(크레이트의 src·lang·assets·build.rs·Cargo.toml·
//! tasty-plugin.toml)에 맞을 때만 내용을 비교한다. 크레이트 밖 파일이나 다른 폴더를 include_*!로
//! 읽으면 그 파일이 바뀌어도 검사는 조용히 통과한다.
//!
//! 이 시험은 crates/*와 vendor/*의 test 밖 Rust 소스에서 include_str!·include_bytes!·include!의 경로와
//! 크레이트 루트 build.rs의 rerun-if-changed 경로를 모은다. 각 경로는 읽는 크레이트 안의 비교 위치이거나
//! 아래 예외 표에 이유와 함께 있어야 한다. 쓰지 않는 예외도 실패로 본다.
//! 매크로 호출은 이름·`!`·구분자를 따로 읽으므로 세 가지 괄호와 그 사이의 공백·주석을 받는다.
//! 파일 예외는 그 경로만, 디렉터리 예외는 그 아래 경로만 덮는다.
//! "플러그인 폐포 밖" 예외는 읽는 크레이트가 매니페스트 플러그인의 normal·build 폐포(모든 target)에
//! 들어오면 실패한다. 폐포는 Cargo.lock과 각 매니페스트로 구하므로 cargo나 네트워크가 필요 없다.
//! 폐포의 시작점은 tasty-plugin.toml이 있는 크레이트의 패키지 이름이다(디렉터리 이름과 다를 수 있다).
//! Cargo.lock은 optional 의존도 담으므로 폐포는 cargo tree보다 넓을 수 있다(조건부 예외가 더 일찍 실패할 뿐이다).
//!
//! 한계:
//! - "링크에서 빠짐(측정)" 예외는 측정 당시의 사용처를 전제로 한다. 플러그인 쪽 코드가 그 데이터를
//!   쓰게 되는지(예: tasty-i18n의 builtin_toml에 닿는 함수)는 이 시험이 감시하지 않는다.
//!   전제와 다시 측정할 조건은 docs/dev-guide/release.md "플러그인 버전 비교"에 있다.
//! - 크레이트 루트가 아닌 위치의 빌드 스크립트(매니페스트 `build = "..."`)는 읽지 않는다.
//! - build.rs가 rerun-if-changed 없이 std::fs로 읽는 파일, env!로 만든 경로, OUT_DIR 생성물은
//!   입력으로 모으지 않는다. OUT_DIR 생성물은 build.rs 자체가 비교 대상이다.
//! - include 매크로 이름과 `!`를 찾았는데 인자를 읽지 못하면 누락하지 않고 실패로 보고한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tasty_doc_guards::cargo_manifest::declared_deps;
use tasty_doc_guards::cfg_predicate::blank_gated_lines;
use tasty_doc_guards::shipping_scope::test_only_files;
use tasty_doc_guards::source_text::{mask_comments_aligned, mask_non_code_aligned};

/// 스크립트 build_affecting의 case 패턴과 같은 집합이다. 아래 시험이 스크립트 줄과 대조한다.
const JUDGED_PATTERNS: &[&str] = &[
    "*/src/*",
    "*/lang/*",
    "*/assets/*",
    "*/Cargo.toml",
    "*/tasty-plugin.toml",
    "*/build.rs",
];

/// 현재 트리에서 모이는 입력 수의 하한이다. 0개 수집을 통과로 읽지 않기 위한 값이다.
const MIN_INPUTS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// 플러그인 바이너리에 들어가지 않는다는 것을 바이너리 비교로 측정했다.
    NotLinkedMeasured,
    /// 읽는 크레이트가 플러그인 폐포 밖이다. 폐포에 들어오면 시험이 실패한다.
    OutsideClosure,
}

/// 예외가 덮는 경로. 파일은 정확히 같은 경로만, 디렉터리는 그 아래 경로만 덮는다.
#[derive(Clone, Copy, Debug)]
enum Target {
    File(&'static str),
    Dir(&'static str),
}

impl Target {
    fn path(self) -> &'static str {
        match self {
            Target::File(p) | Target::Dir(p) => p,
        }
    }

    /// path는 저장소 상대 경로이고 open이면 정해지지 않은 접두어다.
    fn covers(self, path: &str, open: bool) -> bool {
        match self {
            Target::File(f) => !open && path == f,
            Target::Dir(d) => path == d || path.strip_prefix(d).is_some_and(|r| r.starts_with('/')),
        }
    }
}

struct Exception {
    /// include를 쓰는 크레이트 디렉터리(저장소 상대).
    includer: &'static str,
    target: Target,
    kind: Kind,
    why: &'static str,
}

const EXCEPTIONS: &[Exception] = &[
    Exception {
        includer: "crates/tasty-i18n",
        target: Target::Dir("lang"),
        kind: Kind::NotLinkedMeasured,
        why: "번들 플러그인이 builtin_toml에 닿지 않아 번역표가 링크에서 빠진다. release.md 플러그인 버전 비교의 sha256 측정",
    },
    Exception {
        includer: "crates/tasty-ui-widgets",
        target: Target::File("assets/icons/icon_256.png"),
        kind: Kind::NotLinkedMeasured,
        why: "LOGO_PNG. 번들 플러그인 release 바이너리 9개에 이 PNG 바이트가 없고 같은 빌드의 호스트 바이너리에는 있었다. release.md 플러그인 버전 비교",
    },
    Exception {
        includer: "crates/tasty-gallery",
        target: Target::Dir("lang"),
        kind: Kind::OutsideClosure,
        why: "갤러리 앱의 번역표",
    },
    Exception {
        includer: "crates/tasty-gallery",
        target: Target::Dir("assets/icons"),
        kind: Kind::OutsideClosure,
        why: "갤러리 앱의 아이콘",
    },
    Exception {
        includer: "crates/tasty-platform",
        target: Target::Dir("assets/icons"),
        kind: Kind::OutsideClosure,
        why: "호스트 창 아이콘",
    },
    Exception {
        includer: "crates/tasty-themes",
        target: Target::Dir("crates/tasty-themes/themes"),
        kind: Kind::OutsideClosure,
        why: "내장 테마 TOML",
    },
    Exception {
        includer: "crates/tasty-design-tokens",
        target: Target::Dir("crates/tasty-design-tokens/dtcg"),
        kind: Kind::OutsideClosure,
        why: "디자인 토큰 사본",
    },
    Exception {
        includer: "crates/tasty-ipc",
        target: Target::Dir("crates/tasty-ipc/fixtures"),
        kind: Kind::OutsideClosure,
        why: "IPC 메서드 0.7.0 동결 목록",
    },
    Exception {
        includer: "crates/tasty-host-plugin",
        target: Target::Dir("crates/tasty-host-plugin/keys"),
        kind: Kind::OutsideClosure,
        why: "번들 서명 공개키(build.rs가 OUT_DIR로 옮긴다)",
    },
];

/// 모은 입력 하나. path는 저장소 상대 경로이고 open이면 그 뒤가 정해지지 않은 접두어다.
#[derive(Clone, Debug)]
struct Input {
    includer: String,
    site: String,
    path: String,
    open: bool,
}

/// include 인자를 읽은 결과.
#[derive(Debug, PartialEq, Eq)]
enum Arg {
    /// 기준 디렉터리에 대한 상대 경로. open이면 접두어다.
    Path { base: Base, rel: String, open: bool },
    /// OUT_DIR 생성물.
    Generated,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Base {
    SourceDir,
    CrateDir,
}

/// include_*! 괄호 안을 읽는다. lits는 주석을 가리고 리터럴을 남긴 사본, at은 여는 괄호 다음 위치다.
fn parse_arg(lits: &str, at: usize) -> Result<Arg, String> {
    let rest = lits[at..].trim_start();
    if rest.starts_with('"') {
        let (s, _) = read_literal(rest)?;
        return Ok(Arg::Path {
            base: Base::SourceDir,
            rel: s,
            open: false,
        });
    }
    let Some(inner) = rest.strip_prefix("concat!") else {
        return Err(format!("읽을 수 없는 인자: {}", head(rest)));
    };
    let inner = inner.trim_start();
    let Some(mut cur) = inner.strip_prefix(['(', '[', '{']) else {
        return Err(format!("concat! 뒤에 괄호가 없다: {}", head(inner)));
    };
    let mut base = Base::SourceDir;
    let mut rel = String::new();
    let mut first = true;
    loop {
        cur = cur.trim_start();
        if cur.starts_with([')', ']', '}']) {
            return Ok(Arg::Path {
                base,
                rel,
                open: false,
            });
        }
        if cur.starts_with('"') {
            let (s, n) = read_literal(cur)?;
            rel.push_str(&s);
            cur = &cur[n..];
        } else if let Some(after) = cur.strip_prefix("env!") {
            let after = after
                .trim_start()
                .strip_prefix(['(', '[', '{'])
                .unwrap_or(after)
                .trim_start();
            let (name, n) = read_literal(after)?;
            if !first {
                return Err(format!("concat! 중간의 env!({name})"));
            }
            match name.as_str() {
                "OUT_DIR" => return Ok(Arg::Generated),
                "CARGO_MANIFEST_DIR" => base = Base::CrateDir,
                other => return Err(format!("알 수 없는 env!({other})")),
            }
            cur = after[n..].trim_start();
            cur = cur.strip_prefix([')', ']', '}']).unwrap_or(cur);
        } else {
            // 매크로 변수 등 리터럴이 아닌 조각부터는 정해지지 않는다.
            if rel.is_empty() {
                return Err(format!("concat! 첫 조각이 리터럴이 아니다: {}", head(cur)));
            }
            return Ok(Arg::Path {
                base,
                rel,
                open: true,
            });
        }
        first = false;
        cur = cur.trim_start();
        cur = cur.strip_prefix(',').unwrap_or(cur);
    }
}

fn head(s: &str) -> String {
    s.chars().take(60).collect()
}

/// 큰따옴표 문자열 리터럴을 읽어 (값, 소비한 바이트 수)를 돌려준다.
fn read_literal(s: &str) -> Result<(String, usize), String> {
    let body = s
        .strip_prefix('"')
        .ok_or_else(|| format!("문자열 리터럴이 아니다: {}", head(s)))?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, i + 2)),
            '\\' => match chars.next() {
                Some((_, '\\')) => out.push('\\'),
                Some((_, '"')) => out.push('"'),
                other => return Err(format!("지원하지 않는 이스케이프: {other:?}")),
            },
            _ => out.push(c),
        }
    }
    Err(format!("닫히지 않은 리터럴: {}", head(s)))
}

/// 저장소 상대로 `..`·`.`을 정리한다. 저장소 밖으로 나가면 None.
fn normalize(parts: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for seg in parts.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s),
        }
    }
    Some(out.join("/"))
}

/// 셸 case 패턴처럼 `*`가 `/`를 포함한 임의 문자열과 맞는다.
fn glob(pat: &str, s: &str) -> bool {
    match pat.split_once('*') {
        None => pat == s,
        Some((pre, post)) => {
            let Some(rest) = s.strip_prefix(pre) else {
                return false;
            };
            (0..=rest.len())
                .filter(|&i| rest.is_char_boundary(i))
                .any(|i| glob(post, &rest[i..]))
        }
    }
}

fn judged(root: &Path, input: &Input) -> bool {
    let Some(inside) = input.path.strip_prefix(&format!("{}/", input.includer)) else {
        return false;
    };
    if inside.is_empty() {
        return false;
    }
    let candidate = if input.open {
        format!("{}x", input.path)
    } else if root.join(&input.path).is_dir() {
        format!("{}/x", input.path)
    } else {
        input.path.clone()
    };
    JUDGED_PATTERNS.iter().any(|p| glob(p, &candidate))
}

fn crate_dirs(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for top in ["crates", "vendor"] {
        let dir = root.join(top);
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{} 를 읽을 수 없다: {e}", dir.display()));
        for e in entries {
            let e = e.unwrap_or_else(|err| panic!("{top} 항목을 읽을 수 없다: {err}"));
            if e.path().join("Cargo.toml").is_file() {
                out.push(format!("{top}/{}", e.file_name().to_string_lossy()));
            }
        }
    }
    out.sort();
    out
}

fn gather_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} 를 읽을 수 없다: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("{} 항목을 읽을 수 없다: {e}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() == "target" || tasty_doc_guards::is_build_cache_dir(&path) {
                continue;
            }
            gather_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 크레이트 하나의 출하 소스에서 입력을 모은다. 읽지 못한 형태는 problems에 쌓는다.
fn inputs_of(root: &Path, krate: &str, out: &mut Vec<Input>, problems: &mut Vec<String>) {
    let mut files = Vec::new();
    gather_rs(&root.join(krate), &mut files);
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    for f in files {
        let rel = f.strip_prefix(root).expect("저장소 안").to_path_buf();
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let inner = rel_s.strip_prefix(&format!("{krate}/")).unwrap_or(&rel_s);
        // Cargo 시험·벤치·예제 타깃은 플러그인 바이너리에 들어가지 않는다.
        if ["tests/", "benches/", "examples/"]
            .iter()
            .any(|d| inner.starts_with(d))
        {
            continue;
        }
        let src = std::fs::read_to_string(&f)
            .unwrap_or_else(|e| panic!("{rel_s} 읽기 실패: {e}"))
            .replace("\r\n", "\n");
        sources.push((rel, src));
    }
    let test_only = test_only_files(root, &sources);
    for (rel, src) in &sources {
        if test_only.contains(rel) {
            continue;
        }
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let text = blank_gated_lines(src, "test");
        let lits = mask_comments_aligned(&text);
        let (found, bad) = scan_includes(&rel_s, krate, &text);
        out.extend(found);
        problems.extend(bad);
        if rel_s == format!("{krate}/build.rs") {
            rerun_inputs(krate, &rel_s, &text, &lits, out, problems);
        }
    }
}

const INCLUDE_MACROS: &[&str] = &["include_str", "include_bytes", "include"];

/// 파일 하나에서 include 매크로 호출을 찾아 입력으로 바꾼다. text는 test 범위를 비운 원문이다.
/// 이름 뒤의 `!`와 구분자 사이의 공백·주석을 허용하고 세 가지 괄호를 모두 받는다.
/// 이름과 `!`는 찾았는데 인자를 읽지 못하면 건너뛰지 않고 문제로 돌려준다.
fn scan_includes(rel_s: &str, krate: &str, text: &str) -> (Vec<Input>, Vec<String>) {
    let code = mask_non_code_aligned(text);
    let lits = mask_comments_aligned(text);
    let src_dir = rel_s.rsplit_once('/').map_or("", |(d, _)| d);
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let bytes = code.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i < bytes.len() {
        if !is_ident(bytes[i]) || (i > 0 && is_ident(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_ident(bytes[i]) {
            i += 1;
        }
        let name = &code[start..i];
        if !INCLUDE_MACROS.contains(&name) {
            continue;
        }
        let rest = code[i..].trim_start();
        let Some(after_bang) = rest.strip_prefix('!') else {
            // 함수나 use 경로처럼 매크로 호출이 아닌 같은 이름이다.
            continue;
        };
        let line = text[..start].matches('\n').count() + 1;
        let site = format!("{rel_s}:{line}");
        let after = after_bang.trim_start();
        if !after.starts_with(['(', '[', '{']) {
            problems.push(format!("{site}: {name}! 뒤에 괄호가 없다: {}", head(after)));
            continue;
        }
        let open_at = code.len() - after.len() + 1;
        match parse_arg(&lits, open_at) {
            Ok(Arg::Generated) => {}
            Ok(Arg::Path { base, rel, open }) => {
                let (dir, rel) = match base {
                    Base::SourceDir => (src_dir, rel),
                    Base::CrateDir => (krate, rel.trim_start_matches('/').to_string()),
                };
                if rel.starts_with('/') {
                    problems.push(format!("{site}: 절대 경로 {rel}"));
                    continue;
                }
                match normalize(&format!("{dir}/{rel}")) {
                    Some(path) => out.push(Input {
                        includer: krate.to_string(),
                        site,
                        path: if open && rel.ends_with('/') {
                            format!("{path}/")
                        } else {
                            path
                        },
                        open,
                    }),
                    None => problems.push(format!("{site}: 저장소 밖 경로 {rel}")),
                }
            }
            Err(e) => problems.push(format!("{site}: {name}! {e}")),
        }
    }
    (out, problems)
}

/// build.rs의 rerun-if-changed 경로는 크레이트 디렉터리 기준이다. `{`부터는 접두어로 본다.
fn rerun_inputs(
    krate: &str,
    rel_s: &str,
    text: &str,
    lits: &str,
    out: &mut Vec<Input>,
    problems: &mut Vec<String>,
) {
    let key = "rerun-if-changed=";
    let mut from = 0;
    while let Some(off) = lits[from..].find(key) {
        let start = from + off + key.len();
        from = start;
        let tail = &lits[start..];
        let end = tail.find(['"', '{']).unwrap_or(tail.len());
        let open = tail[end..].starts_with('{');
        let rel = &tail[..end];
        let line = text[..start].matches('\n').count() + 1;
        let site = format!("{rel_s}:{line}");
        match normalize(&format!("{krate}/{rel}")) {
            Some(path) => out.push(Input {
                includer: krate.to_string(),
                site,
                path: if open && rel.ends_with('/') {
                    format!("{path}/")
                } else {
                    path
                },
                open,
            }),
            None => problems.push(format!("{site}: 저장소 밖 경로 {rel}")),
        }
    }
}

/// Cargo.lock의 패키지별 의존 이름. 같은 이름의 여러 판은 합친다.
fn lock_edges(lock: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut in_deps = false;
    for line in lock.lines() {
        let t = line.trim();
        if t == "[[package]]" {
            name = None;
            in_deps = false;
            continue;
        }
        if in_deps {
            if t.starts_with(']') {
                in_deps = false;
                continue;
            }
            let dep = t.trim_end_matches(',').trim_matches('"');
            let dep = dep.split(' ').next().unwrap_or(dep);
            if let Some(n) = &name {
                edges.entry(n.clone()).or_default().insert(dep.to_string());
            }
            continue;
        }
        if let Some(v) = t.strip_prefix("name = ") {
            let n = v.trim_matches('"').to_string();
            edges.entry(n.clone()).or_default();
            name = Some(n);
        } else if t.starts_with("dependencies = [") {
            in_deps = !t.ends_with(']');
        }
    }
    edges
}

fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && let Some(v) = t.strip_prefix("name") {
            let v = v.trim_start();
            if let Some(v) = v.strip_prefix('=') {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// 매니페스트 플러그인들의 normal·build 폐포(모든 target)에 드는 저장소 안 크레이트 디렉터리.
/// 저장소 안 크레이트는 dev 의존으로만 선언한 이름을 Cargo.lock 간선에서 빼고, 매니페스트가 선언한
/// normal·build 의존을 더한다(Cargo.lock 갱신 전의 매니페스트 변경도 본다).
fn plugin_closure(root: &Path, krates: &[String]) -> BTreeSet<String> {
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    let mut edges = lock_edges(&lock);
    let mut dir_of: BTreeMap<String, String> = BTreeMap::new();
    for k in krates {
        let manifest =
            std::fs::read_to_string(root.join(k).join("Cargo.toml")).expect("Cargo.toml");
        let Some(name) = package_name(&manifest) else {
            continue;
        };
        let decls = declared_deps(&manifest);
        let shipping: BTreeSet<&str> = decls
            .iter()
            .filter(|(s, _)| s != "dev-dependencies")
            .map(|(_, n)| n.as_str())
            .collect();
        let dev_only: BTreeSet<&str> = decls
            .iter()
            .filter(|(s, n)| s == "dev-dependencies" && !shipping.contains(n.as_str()))
            .map(|(_, n)| n.as_str())
            .collect();
        let e = edges.entry(name.clone()).or_default();
        e.retain(|d| !dev_only.contains(d.as_str()));
        e.extend(shipping.iter().map(|s| s.to_string()));
        dir_of.insert(name, k.clone());
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = krates
        .iter()
        .filter(|k| k.starts_with("crates/") && root.join(k).join("tasty-plugin.toml").is_file())
        .map(|k| {
            dir_of
                .iter()
                .find(|(_, d)| *d == k)
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| panic!("{k}의 패키지 이름을 읽지 못했다"))
        })
        .collect();
    assert!(!stack.is_empty(), "매니페스트 플러그인을 하나도 못 찾았다");
    while let Some(n) = stack.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(ds) = edges.get(&n) {
            stack.extend(ds.iter().cloned());
        }
    }
    seen.iter().filter_map(|n| dir_of.get(n).cloned()).collect()
}

fn covering<'a>(table: &'a [Exception], input: &Input) -> Option<&'a Exception> {
    table
        .iter()
        .find(|e| e.includer == input.includer && e.target.covers(&input.path, input.open))
}

#[test]
fn every_build_input_is_judged_or_listed_with_a_reason() {
    let root = tasty_doc_guards::repo_root();
    let krates = crate_dirs(&root);
    let mut inputs = Vec::new();
    let mut problems = Vec::new();
    for k in &krates {
        inputs_of(&root, k, &mut inputs, &mut problems);
    }
    assert!(
        inputs.len() >= MIN_INPUTS,
        "모은 입력이 {}개로 하한 {MIN_INPUTS}보다 적다. 수집이 깨졌을 수 있다",
        inputs.len()
    );
    let closure = plugin_closure(&root, &krates);
    assert!(
        closure.iter().any(|c| c == "crates/tasty-plugin-sdk"),
        "플러그인 폐포에 SDK가 없다. 폐포 계산이 깨졌다: {closure:?}"
    );

    let mut used = BTreeSet::new();
    for input in &inputs {
        if judged(&root, input) {
            continue;
        }
        match covering(EXCEPTIONS, input) {
            None => problems.push(format!(
                "{}: {}{} — 플러그인 버전 검사가 비교하지 않는 입력이다. 크레이트의 src·lang·assets로 옮기거나 \
                 예외 표에 근거와 함께 더한다",
                input.site,
                input.path,
                if input.open { "…" } else { "" }
            )),
            Some(e) => {
                used.insert((e.includer, e.target.path()));
                if e.kind == Kind::OutsideClosure && closure.contains(e.includer) {
                    problems.push(format!(
                        "{}: {} — 예외 근거가 '플러그인 폐포 밖'인데 {}가 플러그인 폐포에 들어왔다. \
                         바이너리 비교로 다시 재거나 검사 대상에 넣는다",
                        input.site, input.path, e.includer
                    ));
                }
            }
        }
    }
    for e in EXCEPTIONS {
        if !used.contains(&(e.includer, e.target.path())) {
            problems.push(format!(
                "쓰이지 않는 예외: {} {} ({}). 표에서 지운다",
                e.includer,
                e.target.path(),
                e.why
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "입력 {}개 중 문제 {}건:\n{}",
        inputs.len(),
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_judged_patterns_match_the_gate_script() {
    let root = tasty_doc_guards::repo_root();
    let script = std::fs::read_to_string(root.join("scripts/check-plugin-version-bump.sh"))
        .expect("게이트 스크립트");
    let body = script
        .split_once("build_affecting() {")
        .expect("build_affecting 함수")
        .1;
    let line = body
        .lines()
        .find(|l| l.contains(") return 0"))
        .expect("return 0 패턴 줄");
    let pats: Vec<&str> = line
        .trim()
        .split(')')
        .next()
        .expect("패턴")
        .split('|')
        .collect();
    assert_eq!(
        pats, JUDGED_PATTERNS,
        "스크립트의 build_affecting 패턴과 이 시험의 목록이 다르다. 둘을 함께 고친다"
    );
}

#[test]
fn the_argument_reader_covers_the_forms_in_use() {
    let cases: &[(&str, Arg)] = &[
        (
            r#"("../assets/a.png")"#,
            Arg::Path {
                base: Base::SourceDir,
                rel: "../assets/a.png".into(),
                open: false,
            },
        ),
        (
            r#"(concat!("../../../lang/", $code, ".toml"))"#,
            Arg::Path {
                base: Base::SourceDir,
                rel: "../../../lang/".into(),
                open: true,
            },
        ),
        (
            r#"(concat!(env!("CARGO_MANIFEST_DIR"), "/data/x.bin"))"#,
            Arg::Path {
                base: Base::CrateDir,
                rel: "/data/x.bin".into(),
                open: false,
            },
        ),
        (r#"(concat!(env!("OUT_DIR"), "/gen.rs"))"#, Arg::Generated),
    ];
    for (src, want) in cases {
        assert_eq!(&parse_arg(src, 1).expect(src), want, "{src}");
    }
    assert!(parse_arg("(some_macro!())", 1).is_err());
    assert_eq!(
        normalize("crates/a/src/../../../lang/x").as_deref(),
        Some("lang/x")
    );
    assert!(glob("*/src/*", "crates/a/src/b.rs"));
    assert!(!glob("*/src/*", "crates/a/themes/b.toml"));
}

fn write_fixture(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("부모")).expect("디렉터리");
    std::fs::write(&p, body).expect("쓰기");
}

#[test]
fn a_plugin_whose_package_name_differs_from_its_directory_still_has_a_closure() {
    let scratch = tasty_doc_guards::temp_scratch::Scratch::new("build-inputs-renamed");
    let root = scratch.path();
    write_fixture(
        root,
        "crates/plug-dir/Cargo.toml",
        "[package]\nname = \"renamed-plugin\"\nversion = \"0.1.0\"\n\
         [dependencies]\nshared = { path = \"../shared\" }\n",
    );
    write_fixture(
        root,
        "crates/plug-dir/tasty-plugin.toml",
        "id = \"x\"\nversion = \"0.1.0\"\n",
    );
    write_fixture(
        root,
        "crates/shared/Cargo.toml",
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
    );
    write_fixture(
        root,
        "Cargo.lock",
        "version = 4\n\n[[package]]\nname = \"renamed-plugin\"\nversion = \"0.1.0\"\n\
         dependencies = [\n \"shared\",\n]\n\n[[package]]\nname = \"shared\"\nversion = \"0.1.0\"\n",
    );
    let krates = vec!["crates/plug-dir".to_string(), "crates/shared".to_string()];
    let closure = plugin_closure(root, &krates);
    assert!(
        closure.contains("crates/shared") && closure.contains("crates/plug-dir"),
        "디렉터리 이름으로 시작해 폐포가 비었다: {closure:?}"
    );
}

#[test]
fn every_delimiter_and_spacing_of_an_include_call_is_collected() {
    let forms = [
        r#"include_bytes!("../data/x.bin")"#,
        r#"include_bytes! { "../data/x.bin" }"#,
        r#"include_bytes!["../data/x.bin"]"#,
        r#"include_bytes !("../data/x.bin")"#,
        r#"include_bytes /* c */ ! /* d */ ("../data/x.bin")"#,
        "include_bytes\n    !\n    (\"../data/x.bin\")",
        r#"include_str!(concat!["../data/", "x.bin"])"#,
    ];
    for form in forms {
        let text = format!("pub const X: &[u8] = {form};\n");
        let (found, bad) = scan_includes("crates/demo/src/lib.rs", "crates/demo", &text);
        assert!(bad.is_empty(), "{form}: 문제로 보고했다 {bad:?}");
        assert_eq!(found.len(), 1, "{form}: 호출을 놓쳤다");
        assert_eq!(found[0].path, "crates/demo/data/x.bin", "{form}");
    }
}

#[test]
fn an_include_call_whose_argument_cannot_be_read_is_reported_not_skipped() {
    for form in [
        "include_bytes!(some_path!())",
        "include_str!(r\"../raw.txt\")",
        "include_bytes!;",
        "include!(SOURCE)",
    ] {
        let text = format!("const X: () = {form};\n");
        let (found, bad) = scan_includes("crates/demo/src/lib.rs", "crates/demo", &text);
        assert!(found.is_empty(), "{form}: 입력으로 읽었다 {found:?}");
        assert_eq!(bad.len(), 1, "{form}: 조용히 건너뛰었다");
    }
    let (found, bad) = scan_includes(
        "crates/demo/src/lib.rs",
        "crates/demo",
        "use std::include_str;\nfn include(x: u8) -> u8 { x }\n// include_bytes!(\"a\")\nconst S: &str = \"include_str!(x)\";\n",
    );
    assert!(
        found.is_empty() && bad.is_empty(),
        "매크로 호출이 아닌 이름·주석·문자열을 읽었다: {found:?} {bad:?}"
    );
}

#[test]
fn a_file_exception_covers_only_that_file_and_a_directory_only_its_subtree() {
    let table = [
        Exception {
            includer: "crates/a",
            target: Target::File("assets/icons/icon_256.png"),
            kind: Kind::NotLinkedMeasured,
            why: "시험",
        },
        Exception {
            includer: "crates/a",
            target: Target::Dir("crates/a/keys"),
            kind: Kind::OutsideClosure,
            why: "시험",
        },
    ];
    let input = |path: &str, open: bool| Input {
        includer: "crates/a".into(),
        site: "x".into(),
        path: path.into(),
        open,
    };
    let covered = |path: &str, open: bool| covering(&table, &input(path, open)).is_some();
    assert!(covered("assets/icons/icon_256.png", false));
    assert!(!covered("assets/icons/icon_256.png.extra", false));
    assert!(
        !covered("assets/icons/icon_256.png", true),
        "파일 예외가 접두어를 덮었다"
    );
    assert!(covered("crates/a/keys", false));
    assert!(covered("crates/a/keys/release.pem", false));
    assert!(covered("crates/a/keys/", true));
    assert!(!covered("crates/a/keys-other/x", false));
    assert!(!covered("crates/a/keysx", false));
}
