//! 라우팅이 **메서드로 한정해서** 인식하는 id 키를, 그 한정 밖에서 읽는 메서드가 없는가.
//!
//! ## 짝인 가드가 못 보는 것
//!
//! [`super::routing_key_coverage`] 는 명제를 **키 단위**로 세운다 — "핸들러가 읽는 id
//! 키가 `request_target.rs` 에 나온다". 그런데 그 파일의 인식은 두 층이다:
//!
//! - `params_resource_id` 의 배열 — **모든 메서드**에 걸리는 범용 키.
//! - `method_scoped_resource_id` — **적힌 메서드에서만** 걸리는 키.
//!
//! 키 단위 명제는 둘을 구분하지 않는다. 그래서 `"id"` 는 `pty.read` 하나 때문에
//! "인식됨" 이 되고, `"id"` 를 대상으로 읽는 **다른** 메서드는 전부 초록으로 통과한다 —
//! 실제로는 아무것도 안 풀려 포커스된 창으로 간다. `hook_id` · `observer_id` ·
//! `source_id` 도 같은 형태다. 짝인 가드가 초록인 채로 이 축이 비어 있었다.
//!
//! 그래서 여기서는 명제를 **(메서드, 키) 쌍**으로 세운다.
//!
//! ## 모수 — 좁고, 짝인 가드를 대체하지 않는다
//!
//! 쌍을 세려면 메서드를 알아야 하고, 메서드는 dispatch arm 에만 있다. 그래서 모수는
//! `handler.rs` 의 `Some(match request.method.as_str() {` 두 블록에서 닿는 것뿐이다 —
//! plugin·host_call 경로로만 불리는 핸들러는 여기 안 들어온다. 짝인 가드는 핸들러
//! **파일 전수**를 보므로 그쪽이 넓다. **둘 다 둔다**: 이 가드로 저쪽을 대체하면
//! 모수가 좁아진 만큼 사각이 생긴다.
//!
//! ## 도달 판정의 깊이
//!
//! arm 의 식에서 부른 함수 본문까지 따라간다(`require_surface_id` 처럼 키를 안에 박아
//! 둔 헬퍼가 있어서 한 단계로는 모자란다). 재수출도 따라간다 — `surface::handle_x` 의
//! 본체가 `surface/close.rs` 에 있는 형태가 흔하고, 그 한 걸음이 없으면 그 핸들러의
//! 키 읽기가 통째로 안 보인다. 실측 고정점은 **깊이 4**(5·7 도 같은 값).
//!
//! ## 면제는 쌍으로 적는다
//!
//! [ADR-0133](../../docs/adr/0133-guard-scan-population-is-pinned-not-enumerated.md) ③
//! 대로 **집합 동등**이다. 한정 밖에서 읽는 쌍의 집합이 [`PAIR_EXEMPT`] 와 정확히
//! 같아야 한다 — 새 쌍이 생기는 것과 면제가 stale 이 되는 것을 둘 다 잡는다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::repo_root;

const HANDLER_DIR: &str = "src/adapters/ipc/handler";
pub(super) const HANDLER_ROOT: &str = "src/adapters/ipc/handler.rs";
const ROUTING_SOURCE: &str = "src/core/request_target.rs";

/// arm 의 식에서 따라 들어갈 호출 깊이(arm 자신이 1 단계다).
/// 실측 고정점은 4 이고 5·7 에서도 값이 같다 — 고정점 바로 위를 쓴다.
pub(super) const RESOLVE_DEPTH: u32 = 5;

/// dispatch arm 수의 하한 — **연기 검사**다. 파서가 죽으면 예외가 아니라 조용한 0 이
/// 되고, 모수가 비면 아래 집합 동등은 양쪽이 빈 집합이라 그냥 통과한다.
/// 값의 근거: 2026-09-05 실측 **259 개**.
const MIN_METHODS: usize = 200;

/// 라우팅이 메서드로 한정해 인식하는 키를 **그 한정 밖에서** 대상처럼 읽는 쌍.
///
/// `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부와 **물음이
/// 다르다**: 저쪽은 "이 목록을 합쳐야 하는가"(읽기, 전 창), 여기는 "이 요청이 주인 창을
/// 찾는가"(지목, 한 창). 같은 자원이 양쪽에 다른 답으로 들어갈 수 있으므로 합치지 않는다.
///
/// 각 항목에 왜 라우팅이 필요 없는지를 적는다. 여기 적히지 않은 쌍은 그 메서드가
/// 조용히 **포커스된 창**으로 간다는 뜻이다.
const PAIR_EXEMPT: &[(&str, &str, &str)] = &[(
    "pty.attach_surface",
    "id",
    "같은 요청의 `pane_id` 가 범용 키라 이미 주인을 짚는다 — `request_target.rs` 가 \
         이 메서드를 pty 한정에서 뺀 이유가 그것이다",
)];

/// 키 리터럴 뒤에서 **이 호출에 이어진 메서드 사슬**의 끝까지(닫는 괄호 포함).
///
/// 글자 수 창이 아닌 이유는 물음이 거리가 아니기 때문이다 — `as_str()` 은 자기가 어느
/// 키에 붙는지를 토큰만으로 모른다. 거리로 물으면 창에 들어온 **다음 블록**의
/// `as_str()` 을 이 키 것으로 세고(실측: `request_target.rs` 의 `"observer_id"` 뒤
/// 172 자에 있는 `as_str()` 은 `preset.capture` 의 `params.get("kind")` 것이다), 그
/// 오답은 키를 라우팅 명부에서 **조용히 빼는** 방향이다. 그래서 수신자로 되돌려
/// 묻는다: 그 `as_str()` 이 이 호출의 사슬 안에 있는가.
///
/// `after` 는 마커(여는 괄호까지) 뒤의 조각이고 `end` 는 키 리터럴을 닫는 `"` 의 자리다.
/// 즉 이 함수는 괄호 깊이 1 에서 시작해 그 호출을 닫고, 이어지는 `.method(..)` 를
/// 붙어 있는 동안 따라간다. 문자열 리터럴 안의 괄호는 안 센다.
fn call_chain_after(after: &str, end: usize) -> &str {
    let b = after.as_bytes();
    // 키를 닫는 `"` 는 건너뛴다 — 그 자리에서 시작하면 그 따옴표가 **문자열을 여는
    // 것**으로 읽혀 괄호 세기가 통째로 어긋난다.
    let Some(mut i) = close_of_group(b, end + 1, 1) else {
        return &after[end..];
    };
    // 이어지는 메서드 사슬. `.` 이 붙어 있는 동안만 따라간다.
    while i + 1 < b.len() && b[i + 1] == b'.' {
        let mut j = i + 2;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
            j += 1;
        }
        if j >= b.len() || b[j] != b'(' {
            break;
        }
        let Some(close) = close_of_group(b, j, 0) else {
            break;
        };
        i = close;
    }
    &after[end..=i.min(b.len() - 1)]
}

/// `from` 부터 괄호(대괄호 포함) 깊이를 세어 이 묶음을 닫는 자리. 못 닫으면 `None`.
///
/// `depth` 는 시작 깊이다 — 여는 괄호를 이미 지나온 자리에서 부르면 1, 여는 괄호
/// 자신에서 부르면 0 이다. 문자열 리터럴 안의 괄호는 안 센다.
fn close_of_group(b: &[u8], from: usize, depth: i32) -> Option<usize> {
    let (mut depth, mut i) = (depth, from);
    let (mut in_str, mut esc) = (false, false);
    while i < b.len() {
        let c = b[i] as char;
        if in_str {
            match c {
                _ if esc => esc = false,
                '\\' => esc = true,
                '"' => in_str = false,
                _ => {}
            }
        } else {
            match c {
                '"' => in_str = true,
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn is_id_shaped(key: &str) -> bool {
    key == "id" || key.ends_with("_id")
}

/// 여는 중괄호 위치에서 짝을 찾아 블록을 돌려준다. 주석·문자열·문자 리터럴 안의 중괄호는
/// 안 센다 — 짝은 공용 렉서가 그것들을 덮은 사본에서 찾는다(손으로 센 따옴표는 `'"'` · `'}'`
/// · raw 문자열에서 어긋나 블록을 일찍 닫았다).
pub(super) fn balanced(src: &str, open_at: usize) -> &str {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(&src[open_at..]);
    match tasty_doc_guards::match_arms::matching_close(&code, 0) {
        Some(close) => &src[open_at..=open_at + close],
        None => &src[open_at..],
    }
}

/// `"a" | "b" => 식` 형태의 arm 을 걷는다 — guard 가 붙은 팔(`"a" if … =>`)도 팔이다.
///
/// 팔은 공용 판정기(`tasty_doc_guards::match_arms`)가 뗀다. 손으로 뒤로 훑던 판독은 guard 가
/// 붙은 팔을 통째로 빠뜨리고 그 본문을 앞 팔에 붙였다 — 빠진 메서드가 읽는 키는 조용히
/// 통과했다. 판정기가 못 읽는 블록은 여기서 실패한다(건너뛰면 그 안의 팔이 전부 빠진다).
pub(super) fn dispatch_arms(src: &str) -> Vec<(Vec<String>, String)> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    const HEAD: &str = "Some(match request.method.as_str() {";
    let source = Source::new(src);
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(at) = source.code[from..].find(HEAD) {
        let open = from + at + HEAD.len() - 1;
        let close = matching_close(&source.code, open).unwrap_or_else(|| {
            panic!(
                "dispatch `match` 가 닫히지 않는다({}행)",
                source.line_of(open)
            )
        });
        let arms = source
            .match_arms(open..close + 1)
            .unwrap_or_else(|e| panic!("dispatch 팔을 못 읽었다 — {e}"));
        for arm in arms {
            let names: Option<Vec<String>> = source
                .alternatives(&arm.pattern)
                .iter()
                .map(|alt| {
                    source
                        .plain_string(alt)
                        .filter(|name| {
                            !name.is_empty()
                                && name.chars().all(|c| {
                                    c.is_ascii_lowercase()
                                        || c.is_ascii_digit()
                                        || c == '.'
                                        || c == '_'
                                })
                        })
                        .map(str::to_string)
                })
                .collect();
            if let Some(names) = names.filter(|n| !n.is_empty()) {
                // guard 도 그 팔이 params 를 읽는 자리다 — 본문만 보면 guard 에서 읽는 키가
                // 빠진다.
                let guard = arm.guard.as_ref().map_or("", |g| source.slice(g));
                out.push((names, format!("{guard}\n{}", source.slice(&arm.body))));
            }
        }
        from = close + 1;
    }
    out
}

/// 파일 경로 → 모듈 경로. `handler.rs` 는 빈 경로(모듈 루트)다.
fn module_of(rel: &str) -> Vec<String> {
    if rel == HANDLER_ROOT {
        return Vec::new();
    }
    let stem = rel
        .strip_prefix(&format!("{HANDLER_DIR}/"))
        .unwrap_or(rel)
        .trim_end_matches(".rs");
    let mut parts: Vec<String> = stem.split('/').map(str::to_string).collect();
    if parts.last().is_some_and(|p| p == "mod") {
        parts.pop();
    }
    parts
}

pub(super) fn handler_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut paths = Vec::new();
    gather_rs(&root.join(HANDLER_DIR), &mut paths);
    paths.push(root.join(HANDLER_ROOT));
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("{} 읽기 실패: {e}", p.display()));
            (rel, super::strip_comments(&src.replace("\r\n", "\n")))
        })
        .collect()
}

fn gather_rs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            gather_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

pub(super) type FnKey = (Vec<String>, String);

/// (모듈, 함수이름) → 본문. 같은 모듈에 같은 이름이 둘이면 먼저 나온 것이 이긴다.
pub(super) fn fn_index(files: &[(String, String)]) -> BTreeMap<FnKey, String> {
    let mut out = BTreeMap::new();
    for (rel, src) in files {
        let module = module_of(rel);
        for (name, body) in fn_bodies(src) {
            out.entry((module.clone(), name)).or_insert(body);
        }
    }
    out
}

fn fn_bodies(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(at) = src[from..].find("fn ") {
        let start = from + at;
        let prev_ok = start == 0
            || !src.as_bytes()[start - 1].is_ascii_alphanumeric()
                && src.as_bytes()[start - 1] != b'_';
        let after = &src[start + 3..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        from = start + 3;
        if !prev_ok || name.is_empty() {
            continue;
        }
        let Some(brace) = src[start..].find('{') else {
            break;
        };
        let body = balanced(src, start + brace);
        out.push((name, body.to_string()));
    }
    out
}

/// 호출 경로를 함수 색인의 키로 푼다.
///
/// 순서: `super`/`self`/`crate` 접두 → 명시 모듈 → 같은 모듈 → 모듈 루트 → 이름 유일.
/// 이름만으로 고르면 `handle_list` 처럼 모듈마다 있는 이름에서 엉뚱한 정의가 이긴다.
pub(super) fn resolve(
    index: &BTreeMap<FnKey, String>,
    caller: &[String],
    path: &str,
) -> Option<FnKey> {
    let mut parts: Vec<&str> = path.split("::").collect();
    let name = parts.pop()?.to_string();
    if parts.is_empty() {
        for cand in [caller.to_vec(), Vec::new()] {
            let key = (cand, name.clone());
            if index.contains_key(&key) {
                return Some(key);
            }
        }
        let mut hits = index.keys().filter(|(_, n)| *n == name);
        let only = hits.next()?;
        return hits.next().is_none().then(|| only.clone());
    }
    let base: Vec<String> = match parts[0] {
        "self" => caller.to_vec(),
        "super" => caller
            .split_last()
            .map_or_else(Vec::new, |(_, r)| r.to_vec()),
        "crate" => Vec::new(),
        _ => caller.to_vec(),
    };
    let tail: Vec<String> = parts
        .iter()
        .skip(usize::from(matches!(parts[0], "self" | "super" | "crate")))
        .filter(|p| !matches!(**p, "adapters" | "ipc" | "handler"))
        .map(|p| (*p).to_string())
        .collect();
    let prefixes = [
        [base.clone(), tail.clone()].concat(),
        tail.clone(),
        [caller.to_vec(), tail.clone()].concat(),
    ];
    for cand in &prefixes {
        let key = (cand.clone(), name.clone());
        if index.contains_key(&key) {
            return Some(key);
        }
    }
    // 자식 모듈에서 **재수출**된 것. `surface::handle_surface_close` 의 본체는
    // `handler/surface/close.rs` 에 있고 `surface.rs` 는 `pub(crate) use` 로 내보낼 뿐이다.
    // 이 한 걸음이 없으면 그런 핸들러의 키 읽기가 통째로 안 보인다 — 실측으로 걸렸다.
    for pre in &prefixes {
        let mut hits = index
            .keys()
            .filter(|(m, n)| *n == name && m.len() > pre.len() && m.starts_with(pre));
        // `?` 를 쓰면 **첫 접두가 비었을 때 함수를 통째로 빠져나간다** — 뒤 접두를
        // 못 본다. 실측으로 걸렸다(`terminal::…` 안에서 부른 `surface::handle_x` 넷).
        let Some(only) = hits.next() else { continue };
        if hits.next().is_none() {
            return Some(only.clone());
        }
    }
    None
}

/// 공백을 없앤 사본. 키 추출은 고정 마커 뒤의 리터럴만 보므로 공백을 다 지워도 된다.
fn flatten(src: &str) -> String {
    src.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 한 조각이 `params` 에서 읽는 id 키 — [`params_keys_in`] 에서 모양으로 좁힌 것.
fn id_keys_in(fragment: &str) -> BTreeSet<String> {
    params_keys_in(fragment)
        .into_iter()
        .filter(|k| is_id_shaped(k))
        .collect()
}

/// 한 조각이 `params` 에서 **숫자로** 읽는 키 전부(모양 필터 없음).
pub(super) fn params_keys_in(fragment: &str) -> BTreeSet<String> {
    const MARKERS: &[&str] = &[
        "params.get(\"",
        "(params,\"",
        "(&params,\"",
        "(&request.params,\"",
        "(request.params,\"",
    ];
    let flat = flatten(fragment);
    let mut out = BTreeSet::new();
    for marker in MARKERS {
        let mut rest = flat.as_str();
        while let Some(at) = rest.find(marker) {
            let after = &rest[at + marker.len()..];
            let Some(end) = after.find('"') else { break };
            let key = &after[..end];
            // 라우팅은 **숫자만** 본다(`as_u64`). 바로 문자열로 꺼내는 읽기는 대상
            // 지목이 아니다 — `"id"` 하나가 메서드에 따라 숫자이기도 문자열이기도
            // 하므로(agent dag id · approval id 는 문자열), 키 이름으로는 못 가른다.
            let tail = call_chain_after(after, end);
            if !key.is_empty()
                && key.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && !tail.contains("as_str()")
            {
                out.insert(key.to_string());
            }
            rest = &after[end..];
        }
    }
    out
}

/// 조각 안에서 부른 함수 경로.
///
/// **공백을 지운 사본에서 뽑으면 안 된다.** `match require_surface_id(…)` 가
/// `matchrequire_surface_id` 로 붙어 이름이 통째로 달라지고, 그러면 그 헬퍼 안의 키
/// 읽기가 안 보인다 — 위반이 아니라 **침묵**이라 가드는 초록인 채로 비어 간다.
/// (실측으로 걸렸다: 이 형태 하나 때문에 쌍 67 중 30 을 못 봤다.)
pub(super) fn called_paths(fragment: &str) -> Vec<String> {
    let b = fragment.as_bytes();
    let mut out = Vec::new();
    for (i, c) in fragment.char_indices() {
        if c != '(' {
            continue;
        }
        let mut s = i;
        while s > 0 {
            let p = b[s - 1];
            if p.is_ascii_alphanumeric() || p == b'_' || p == b':' {
                s -= 1;
            } else {
                break;
            }
        }
        let path = &fragment[s..i];
        if path.is_empty() || path.ends_with(':') {
            continue;
        }
        if path
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
            || path.contains("::")
        {
            out.push(path.trim_start_matches(':').to_string());
        }
    }
    out
}

/// 조각에서 도달 가능한 id 키 — 부른 함수 본문까지 `depth` 만큼 따라간다.
fn reachable_keys(
    index: &BTreeMap<FnKey, String>,
    caller: &[String],
    fragment: &str,
    depth: u32,
    seen: &mut BTreeSet<FnKey>,
) -> BTreeSet<String> {
    reachable_keys_with(index, caller, fragment, depth, seen, id_keys_in)
}

/// 같은 순회를 **추출기를 갈아 끼워** 돌린다. 옆 가드가 모양 필터 없는 키 집합을 같은
/// 도달 판정으로 얻어야 하는데, 순회를 복제하면 깊이·재수출 해석이 두 벌이 되어 한쪽만
/// 고쳐지는 순간 갈린다.
pub(super) fn reachable_keys_with(
    index: &BTreeMap<FnKey, String>,
    caller: &[String],
    fragment: &str,
    depth: u32,
    seen: &mut BTreeSet<FnKey>,
    extract: fn(&str) -> BTreeSet<String>,
) -> BTreeSet<String> {
    let mut keys = extract(fragment);
    if depth == 0 {
        return keys;
    }
    for path in called_paths(fragment) {
        let Some(key) = resolve(index, caller, &path) else {
            continue;
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        let body = index[&key].clone();
        keys.extend(reachable_keys_with(
            index,
            &key.0,
            &body,
            depth - 1,
            seen,
            extract,
        ));
    }
    keys
}

/// 메서드 → 그 메서드가 대상으로 읽는 id 키.
fn method_id_keys() -> BTreeMap<String, BTreeSet<String>> {
    let files = handler_sources();
    let index = fn_index(&files);
    let root_src = files
        .iter()
        .find(|(rel, _)| rel == HANDLER_ROOT)
        .map(|(_, s)| s.clone())
        .unwrap_or_default();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (names, expr) in dispatch_arms(&root_src) {
        let mut seen = BTreeSet::new();
        let keys = reachable_keys(&index, &[], &expr, RESOLVE_DEPTH, &mut seen);
        for name in names {
            out.entry(name).or_default().extend(keys.iter().cloned());
        }
    }
    assert!(
        out.len() >= MIN_METHODS,
        "dispatch arm 을 {} 개만 걷었다(하한 {MIN_METHODS}, 2026-09-05 실측 259). \
         파서가 죽으면 아래 집합 동등은 양쪽이 빈 집합이라 그냥 통과한다",
        out.len()
    );
    out
}

/// 모든 메서드에 걸리는 범용 키 — `params_resource_id` 의 배열 리터럴.
fn generic_keys(routing: &str) -> BTreeSet<String> {
    generic_keys_all(routing)
        .into_iter()
        .filter(|k| is_id_shaped(k))
        .collect()
}

/// 같은 배열을 **모양 필터 없이** 돌려준다. `surface` · `parent` · `target` · `pane` 은
/// `_id` 로 안 끝나지만 라우팅이 인식하는 대상 키라, 그 넷을 빼고 "이 메서드가 대상을
/// 지목하는가" 를 물으면 `terminal.*` 일곱이 지목 없음으로 잡힌다(실측).
pub(super) fn generic_keys_all(routing: &str) -> BTreeSet<String> {
    let at = routing
        .find("fn params_resource_id")
        .expect("params_resource_id 가 사라졌다 — 대조군이 죽었다");
    let brace = routing[at..].find('{').expect("본문이 없다") + at;
    let body = balanced(routing, brace);
    let list_at = body.find("for key in [").expect("범용 키 배열을 못 찾았다");
    let list_end = body[list_at..].find(']').expect("배열이 안 닫힌다") + list_at;
    literals(&body[list_at..list_end])
}

/// 메서드 한정 인식 — `method_scoped_resource_id` 의 각 `if` 블록에서 (메서드, 키).
///
/// 그 함수의 모든 분기는 **긍정형 `if`** 여야 한다. `if !matches!(…) { return None; }`
/// 처럼 뒤집힌 가드를 쓰면 메서드 목록이 블록 밖의 코드에 걸려 여기서 안 보인다.
/// [`the_scoped_side_has_no_inverted_guard`] 가 그 형태를 못박는다.
pub(super) fn scoped_pairs(routing: &str) -> BTreeSet<(String, String)> {
    let at = routing
        .find("fn method_scoped_resource_id")
        .expect("method_scoped_resource_id 가 사라졌다 — 대조군이 죽었다");
    let brace = routing[at..].find('{').expect("본문이 없다") + at;
    let body = balanced(routing, brace);
    let mut out = BTreeSet::new();
    let mut from = 1usize;
    while let Some(rel) = body[from..].find("if ") {
        let if_at = from + rel;
        let Some(open_rel) = body[if_at..].find('{') else {
            break;
        };
        let open = if_at + open_rel;
        let block = balanced(body, open);
        let methods: BTreeSet<String> = literals(&body[if_at..open])
            .into_iter()
            .filter(|s| s.contains('.'))
            .collect();
        let keys: BTreeSet<String> = literals(block)
            .into_iter()
            .filter(|k| is_id_shaped(k))
            .collect();
        for m in &methods {
            for k in &keys {
                out.insert((m.clone(), k.clone()));
            }
        }
        from = open + block.len();
    }
    out
}

/// 조각 안의 소문자 문자열 리터럴.
pub(super) fn literals(fragment: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = fragment;
    while let Some(at) = rest.find('"') {
        let after = &rest[at + 1..];
        let Some(end) = after.find('"') else { break };
        let lit = &after[..end];
        if !lit.is_empty()
            && lit
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
        {
            out.insert(lit.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

pub(super) fn routing_source() -> String {
    let path = repo_root().join(ROUTING_SOURCE);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{ROUTING_SOURCE}: {e}"));
    let production = src.split("#[cfg(test)]").next().unwrap_or(&src).to_string();
    super::strip_comments(&production.replace("\r\n", "\n"))
}

/// 메서드 한정 키를 그 한정 밖에서 읽는 쌍은 **면제 목록과 정확히 같다.**
#[test]
fn every_method_scoped_key_is_read_only_where_it_routes() {
    let routing = routing_source();
    let generic = generic_keys(&routing);
    let scoped = scoped_pairs(&routing);
    assert!(!generic.is_empty(), "범용 키를 못 뽑았다 — 대조군이 죽었다");
    assert!(!scoped.is_empty(), "한정 쌍을 못 뽑았다 — 대조군이 죽었다");
    let key_exempt: BTreeSet<&str> = super::routing_key_coverage::NOT_A_ROUTING_TARGET
        .iter()
        .map(|(k, _)| *k)
        .collect();
    let pair_exempt: BTreeSet<(String, String)> = PAIR_EXEMPT
        .iter()
        .map(|(m, k, _)| ((*m).to_string(), (*k).to_string()))
        .collect();

    let mut unscoped: BTreeSet<(String, String)> = BTreeSet::new();
    for (method, keys) in method_id_keys() {
        for key in keys {
            if generic.contains(&key) || key_exempt.contains(key.as_str()) {
                continue;
            }
            if scoped.contains(&(method.clone(), key.clone())) {
                continue;
            }
            unscoped.insert((method.clone(), key));
        }
    }
    let missing: Vec<_> = unscoped.difference(&pair_exempt).collect();
    let stale: Vec<_> = pair_exempt.difference(&unscoped).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "메서드 한정 인식과 실제 읽기가 어긋난다.\n\
         \x20 한정 밖인데 면제도 없는 쌍: {missing:?}\n\
         \x20 면제에 있으나 그렇게 읽는 메서드가 없는 쌍: {stale:?}\n\
         앞의 것은 그 메서드가 조용히 **포커스된 창**으로 간다는 뜻이다. 창이 소유한 \
         리소스를 가리키면 `method_scoped_resource_id` 에 그 메서드를 넣고, 아니면 \
         PAIR_EXEMPT 에 **사유와 함께** 적어라."
    );
}

/// 한정 쪽 분기는 전부 긍정형 `if` 다.
///
/// `if !matches!(method, …) { return None; }` 는 메서드 목록을 블록 **밖**에 두므로
/// [`scoped_pairs`] 가 그 쌍을 못 본다 — 인식하고 있는데 안 하는 것으로 세어 면제
/// 목록이 부풀고, 부푼 면제는 검토받지 않는다.
#[test]
fn the_scoped_side_has_no_inverted_guard() {
    let routing = routing_source();
    let at = routing
        .find("fn method_scoped_resource_id")
        .expect("함수가 없다");
    let brace = routing[at..].find('{').unwrap() + at;
    let body = flatten(balanced(&routing, brace));
    assert!(
        !body.contains("if!matches!(method") && !body.contains("if!method"),
        "`method_scoped_resource_id` 에 뒤집힌 가드가 있다 — 긍정형 `if` 로 써라"
    );
}

/// 추출기의 극성.
#[test]
fn the_extractor_reads_arms_calls_and_keys() {
    let arms = dispatch_arms(concat!(
        "Some(match request.method.as_str() {\n",
        "    \"a.one\" => f(state, id, &request.params),\n",
        "    \"a.two\" | \"a_b.three2\" => { g(params) }\n",
        "    other => fallback(other),\n",
        "})"
    ));
    let names: Vec<Vec<String>> = arms.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(
        names,
        vec![
            vec!["a.one".to_string()],
            vec!["a.two".to_string(), "a_b.three2".to_string()]
        ],
        "arm 이름 추출의 극성이 달라졌다 — `|` 로 이어진 것을 다 잡고, 이름이 아닌 arm 은 안 잡아야 한다"
    );
    assert_eq!(
        id_keys_in("let a = params.get(\"surface_id\");\nrequire_u32(params, \"tab_id\", &id);\nresp.get(\"other_id\");\nparams.get(\"kind\");")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["surface_id".to_string(), "tab_id".to_string()],
        "키 추출의 극성이 달라졌다"
    );
    assert!(
        id_keys_in("params.get(\"id\").and_then(|v| v.as_str())").is_empty(),
        "문자열로 꺼내는 읽기가 대상 지목으로 잡혔다 — 라우팅은 숫자만 본다"
    );
    // 문자열 읽기 판정은 **이 호출의 사슬**을 본다 — 뒤에 붙은 다른 호출의 `as_str()`
    // 이 아니다. 글자 수 창으로 물으면 아래가 통째로 빠진다(키가 명부에서 사라진다).
    assert_eq!(
        id_keys_in("params.get(\"observer_id\");x.as_str()")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["observer_id".to_string()],
        "다른 수신자의 as_str() 이 이 키의 읽기로 세어졌다"
    );
    assert!(called_paths("a::b::f(x) + g(y)").contains(&"a::b::f".to_string()));
    assert!(
        called_paths("match require_surface_id(params, &id) {")
            .contains(&"require_surface_id".to_string()),
        "키워드가 호출 이름에 붙었다 — 공백을 지운 사본에서 뽑으면 헬퍼가 통째로 안 보인다"
    );
}

/// 인식 두 층을 **갈라서** 읽는다 — 이 가드의 존재 이유다.
#[test]
fn generic_and_scoped_are_read_as_two_layers() {
    let fake = concat!(
        "fn params_resource_id(p: &V) -> Option<R> {\n",
        "    for key in [\"surface_id\", \"pane_id\"] { use_key(key); }\n",
        "    None\n}\n",
        "fn method_scoped_resource_id(method: &str, p: &V) -> Option<R> {\n",
        "    if method == \"pty.read\" { return numeric(params, \"id\"); }\n",
        "    None\n}\n",
    );
    let generic = generic_keys(fake);
    let scoped = scoped_pairs(fake);
    assert_eq!(
        generic,
        ["pane_id".to_string(), "surface_id".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        scoped,
        [("pty.read".to_string(), "id".to_string())]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert!(
        !generic.contains("id"),
        "한정 키가 범용으로 새면 이 가드가 짝인 가드와 똑같아진다"
    );
}

/// 면제를 겨냥한 변이 — 면제 창 안쪽에 진짜 위반을 심으면 잡히는가.
#[cfg(test)]
mod exemption_mutations {
    use super::*;

    /// 면제된 쌍의 **메서드만** 바꾸면(같은 키, 다른 메서드) 잡혀야 한다.
    #[test]
    fn a_new_method_reading_an_exempt_key_is_not_covered() {
        let pair_exempt: BTreeSet<(String, String)> = PAIR_EXEMPT
            .iter()
            .map(|(m, k, _)| ((*m).to_string(), (*k).to_string()))
            .collect();
        let invented = ("invented.method".to_string(), "id".to_string());
        assert!(
            !pair_exempt.contains(&invented),
            "면제가 키 단위로 새고 있다 — 쌍 단위여야 한다"
        );
    }

    /// 면제 사유가 비어 있지 않다. 사유 없는 면제는 검토받지 못한다.
    #[test]
    fn every_pair_exemption_states_a_reason() {
        for (m, k, why) in PAIR_EXEMPT {
            assert!(
                why.len() > 20,
                "({m}, {k}) 의 면제 사유가 너무 짧다 — 왜 라우팅이 필요 없는지를 적어라"
            );
        }
    }
}

/// 모수의 네 층은 서로 겹치지 않고 쌍 전부를 덮는다.
///
/// 이 가드의 판정은 "어느 층에도 안 드는 쌍" 을 세는 것이라, 층이 겹치면 같은 쌍이 두
/// 번 설명되고 비면 판정이 조용히 좁아진다. 수를 적지 않고 **분할이라는 성질**만
/// 못박는다 — 수는 커밋마다 움직인다.
#[test]
fn the_four_layers_partition_every_pair() {
    let routing = routing_source();
    let generic = generic_keys(&routing);
    let scoped = scoped_pairs(&routing);
    let key_exempt: BTreeSet<&str> = super::routing_key_coverage::NOT_A_ROUTING_TARGET
        .iter()
        .map(|(k, _)| *k)
        .collect();
    let pair_exempt: BTreeSet<(String, String)> = PAIR_EXEMPT
        .iter()
        .map(|(m, k, _)| ((*m).to_string(), (*k).to_string()))
        .collect();
    let mut uncovered = Vec::new();
    let mut doubled = Vec::new();
    for (method, keys) in method_id_keys() {
        for key in keys {
            let pair = (method.clone(), key.clone());
            let layers = u8::from(generic.contains(&key))
                + u8::from(key_exempt.contains(key.as_str()))
                + u8::from(scoped.contains(&pair))
                + u8::from(pair_exempt.contains(&pair));
            if layers == 0 {
                uncovered.push(pair);
            } else if layers > 1 {
                doubled.push(pair);
            }
        }
    }
    assert!(
        uncovered.is_empty() && doubled.is_empty(),
        "층이 분할이 아니다.\n 어느 층에도 안 드는 쌍: {uncovered:?}\n 두 층에 걸친 쌍: {doubled:?}"
    );
}
