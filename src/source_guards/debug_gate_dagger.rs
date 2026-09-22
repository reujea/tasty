//! `debug-ipc.md` 의 † 표시가 **런타임 게이트라는 코드의 성질**과 어긋나지 않는지 본다.
//!
//! † 는 "이 메서드는 `--enable-input-simulation` 없이는 거부된다" 를 주장하는 표식이다.
//! 그 주장의 진위는 소스에 있다 — `require_input_simulation` 을 부르는가. 표식과 성질이
//! 갈라져도 아무 데서도 안 터지므로, 실제로 갈라져 있었다: † 여섯 행 중 둘
//! (`debug.switch_workspace` · `debug.switch_tab`)이 게이트 없는 메서드에 붙어 있었다.
//!
//! # 세 집합이 같아야 한다
//!
//! 같은 사실을 세 곳이 각각 적는다. 셋이 일치하지 않으면 어느 하나가 거짓이다.
//!
//! 1. **† 가 붙은 표 행** — 사람이 표를 고칠 때마다 손으로 붙인다.
//! 2. **각주 본문의 열거** — 같은 문단이 이름을 다시 적는다.
//! 3. **`require_input_simulation` 호출처** — 실제로 게이트가 걸린 곳.
//!
//! 어긋났을 때 무엇이 정본인지는 이 가드가 정하지 않는다. 셋 중 하나만 고쳐서 통과시키면
//! 나머지 둘과 또 갈라지므로, **셋을 함께 맞추게** 강제하는 것이 이 가드의 일이다.
//!
//! # 왜 † 만 지우는 것이 맞았나 (2026-09-05 판정)
//!
//! [ADR-0115](../../docs/adr/0115-input-reproduction-ipc-debug-isolation.md) 가 게이트의
//! 기준을 적는다 — 대상은 **tasty 프로세스 밖으로 나가는** 입력 조작(OS 이벤트 스트림·
//! 시스템 입력 소스)과 대상 surface 의 **PTY 에 쓰는** 주입이고, 창 내부 상태만 바꾸는
//! in-process 시뮬레이션(`surface.ime_*` · `debug.selection` 계열)에는 걸지 않는다.
//! `debug.switch_workspace`/`switch_tab` 은 후자다 — 둘 다 `route_debug_handler`
//! (`#[cfg(debug_assertions)]`) 안에 있어 release 에는 없고, PTY 에도 OS 에도 안 나간다.
//! ADR 의 결정 · 각주 본문의 열거 · 게이트 호출처 **셋이 이미 일치**했고 † 만 어긋났다.
//!
//! # 사거리
//!
//! 게이트가 **걸렸는지**만 본다. 게이트가 **옳게 동작하는지**는 안 본다 — 그건
//! `tests/ipc_release_table_excludes_input_reproduction.rs` 와 ADR-0115 의 몫이다.
//! 그리고 dispatch 팔을 텍스트로 읽으므로, 팔의 이름이 리터럴이 아니게 되면(매크로가
//! 만들거나 상수와 맞대면) 그 팔은 지도에 안 들어온다. 그러면 **게이트가 걸렸는데 † 가
//! 없는 메서드**가 아무 집합에도 안 나타나 세 집합이 사이좋게 일치한다 — 조용한 초록이다.
//!
//! **아래 하한은 그것을 못 본다.** 변이로 쟀다: 게이트된 핸들러를 부르는 팔을 하나 더
//! 넣되 † 를 안 달면 리터럴일 때는 "게이트만 있고 † 없음" 으로 빨개지는데, 같은 팔을
//! 매크로가 만든 이름으로 바꾸면 **6 개 테스트가 전부 초록**이었다. 팔이 하나 느는 방향
//! 이라 팔 수가 줄지 않고, 수를 세는 검사는 느는 방향을 못 보기 때문이다 — 여유의 문제가
//! 아니다. 그래서 팔의 이름이 리터럴인지를 따로 잰다 — 같은 전제 위에 선 가드가
//! 여럿이라 그 판정은 [`super::dispatch_name_literals`] 가 라우터 전부에 대해 맡는다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::{mask_non_code, repo_root, strip_comments};

const DOC: &str = "docs/dev-guide/debug-ipc.md";
const DISPATCH: &str = "src/adapters/ipc/handler.rs";
const HANDLER_DIR: &str = "src/adapters/ipc/handler";

/// 게이트 함수의 이름. 이 파일이 그 이름을 담으므로 스캔 대상에서 자기를 빼는 대신
/// **스캔 루트를 핸들러 트리로 좁혀** 애초에 자기가 안 들어오게 한다.
const GATE: &str = "require_input_simulation";

/// dispatch 팔 수의 하한 — 연기 검사. 파서가 죽으면 0 이 되고, 0 은 "게이트가 없다" 로
/// 읽혀 조용히 통과한다. 근거: 2026-09-05 실측 214.
const MIN_DISPATCH_ARMS: usize = 150;

fn read(rel: &str) -> String {
    let path: PathBuf = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 를 읽을 수 없다: {e}", path.display()))
}

/// 백틱으로 감싼 `무엇.무엇` 꼴 이름을 순서대로 뽑는다.
fn backticked_methods(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        match after.find('`') {
            None => break,
            Some(j) => {
                let inner = &after[..j];
                let ok = inner.contains('.')
                    && inner
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
                if ok {
                    out.push(inner.to_owned());
                }
                rest = &after[j + 1..];
            }
        }
    }
    out
}

/// 표 행에 † 가 붙은 메서드. 행의 **첫 칸**이 메서드 이름이다.
fn dagger_marked(doc: &str) -> BTreeSet<String> {
    doc.lines()
        .filter(|l| l.trim_start().starts_with('|') && l.contains('†'))
        .filter_map(|l| backticked_methods(l).into_iter().next())
        .collect()
}

/// 각주 본문이 **열거하는** 이름.
///
/// 각주는 열거 뒤에 다른 이름도 언급한다(`engine.input_simulation_enabled` 같은 필드,
/// 그리고 게이트가 **없는** 반례 `surface.ime_*`). 그래서 문장 구조로 열거 구간을 자른다 —
/// `—` 와 첫 조사 `는` 사이가 열거다. 구간을 못 자르면 통과가 아니라 실패다.
fn footnote_enumerated(doc: &str) -> BTreeSet<String> {
    let line = doc
        .lines()
        .find(|l| l.trim_start().starts_with('†'))
        .expect("각주 본문(† 로 시작하는 줄)을 못 찾았다 — 각주가 사라졌거나 형태가 바뀌었다");
    let after_dash = line
        .split_once('—')
        .map(|(_, r)| r)
        .expect("각주에서 `—` 를 못 찾았다 — 열거 구간을 자를 수 없다");
    let span = after_dash
        .split_once('는')
        .map(|(l, _)| l)
        .expect("각주에서 열거 뒤의 조사를 못 찾았다 — 열거 구간을 자를 수 없다");
    let out: BTreeSet<String> = backticked_methods(span).into_iter().collect();
    assert!(
        !out.is_empty(),
        "각주의 열거 구간에서 메서드 이름을 하나도 못 읽었다 — 빈 집합은 아래 비교를 \
         무의미하게 만든다. 구간: {span:?}"
    );
    out
}

/// 응답만 만드는 팔의 호출 대상. 이 팔들은 **아무것도 실행하지 않는다** — 왜 못 하는지를
/// 답할 뿐이다(예: 플랫폼 게이트의 상보 팔, ADR-0154). 게이트가 걸렸는지를 물을 대상이
/// 아니므로 지도에서 뺀다. 안 빼면 한 메서드에 팔이 둘일 때 뒤엣것이 앞엣것을 덮어,
/// **게이트된 실제 핸들러가 사라진 것처럼** 보인다(2026-09-05 실측: `surface.raw_key` 가
/// 그렇게 게이트 없음으로 판정됐다).
const REFUSAL_CALLEES: &[&str] = &["error", "invalid_params"];

/// dispatch 팔 `"a.b" | "c.d" => 모듈::함수(...)` 를 메서드 → 핸들러 함수**들**로 편다.
///
/// 값이 집합인 이유: 한 메서드가 조합마다 다른 팔로 갈릴 수 있다. 하나만 담으면 나중 팔이
/// 앞선 팔을 덮어 **조용히 판정을 뒤집는다.**
fn dispatch_map() -> BTreeMap<String, BTreeSet<String>> {
    dispatch_map_of(&strip_comments(&read(DISPATCH)))
}

/// 위의 순수부 — 합성 입력으로 면제를 찌를 수 있게 분리한다.
///
/// 구분자(`=>` · `{` · `,` · `(`)는 공용 렉서가 문자열·문자 리터럴·주석을 바이트째 덮은
/// 사본(`code`)에서 찾고, 메서드 이름은 같은 구간의 원본에서 읽는다 — guard 문자열 안의 `,`
/// 가 팔 머리를 자르면 그 앞의 이름이 지도에서 빠지고, 한 메서드에 팔이 둘일 때 게이트 없는
/// 팔이 빠져 판정이 조용히 뒤집힌다.
fn dispatch_map_of(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let code = tasty_doc_guards::source_text::mask_non_code_aligned(src);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (i, _) in code.match_indices("=>") {
        // 오른쪽: 첫 `식별자(` 의 마지막 경로 세그먼트가 핸들러 이름이다.
        let rhs = &code[i + 2..];
        let Some(paren) = rhs.find('(') else { continue };
        let callee: String = rhs[..paren]
            .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default()
            .to_owned();
        if callee.is_empty() || REFUSAL_CALLEES.contains(&callee.as_str()) {
            continue;
        }
        // 왼쪽: 이 팔의 문자열 리터럴들. 앞선 `=>` 나 블록 경계까지만 거슬러 본다.
        let lhs_start = code[..i]
            .rfind("=>")
            .map(|p| p + 2)
            .into_iter()
            .chain(code[..i].rfind('{').map(|p| p + 1))
            .chain(code[..i].rfind(',').map(|p| p + 1))
            .max()
            .unwrap_or(0);
        for m in backticked_or_quoted(&src[lhs_start..i]) {
            out.entry(m).or_default().insert(callee.clone());
        }
    }
    out
}

/// 큰따옴표로 감싼 `무엇.무엇` 꼴 리터럴.
fn backticked_or_quoted(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('"') {
        let after = &rest[i + 1..];
        match after.find('"') {
            None => break,
            Some(j) => {
                let inner = &after[..j];
                if inner.contains('.')
                    && inner
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                {
                    out.push(inner.to_owned());
                }
                rest = &after[j + 1..];
            }
        }
    }
    out
}

/// 게이트를 부르는 핸들러 함수 이름들. 정의부(`fn require_input_simulation`)는 뺀다.
fn gated_handler_fns() -> BTreeSet<String> {
    let dir = repo_root().join(HANDLER_DIR);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} 를 읽을 수 없다: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.push(repo_root().join(DISPATCH));
    files.sort();

    let mut out = BTreeSet::new();
    for f in files {
        let Ok(raw) = std::fs::read_to_string(&f) else {
            continue;
        };
        let masked = mask_non_code(&raw);
        for (pos, _) in masked.match_indices(GATE) {
            // 정의부는 호출이 아니다.
            let before = &masked[..pos];
            if before.trim_end().ends_with("fn") {
                continue;
            }
            // 감싸는 함수: 이 위치 앞의 마지막 `fn <이름>`.
            let Some(fi) = before.rfind("fn ") else {
                continue;
            };
            let name: String = masked[fi + 3..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && name != GATE {
                out.insert(name);
            }
        }
    }
    out
}

/// 실제로 게이트가 걸린 **메서드** 이름.
///
/// 판정은 **모든** 실행 팔이 게이트를 부르는가다(`any` 가 아니라 `all`). 하나라도 게이트
/// 없이 실행되는 길이 있으면 † 의 주장("이 메서드는 `--enable-input-simulation` 없이는
/// 거부된다")이 그 길에서 거짓이기 때문이다. 거절만 하는 팔은 애초에 지도에 없다
/// (`REFUSAL_CALLEES`).
fn gated_methods(map: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    gated_methods_with(map, &gated_handler_fns())
}

fn gated_methods_with(
    map: &BTreeMap<String, BTreeSet<String>>,
    fns: &BTreeSet<String>,
) -> BTreeSet<String> {
    map.iter()
        .filter(|(_, callees)| !callees.is_empty() && callees.iter().all(|c| fns.contains(c)))
        .map(|(method, _)| method.clone())
        .collect()
}

#[test]
fn the_dagger_the_footnote_and_the_gate_name_the_same_methods() {
    let doc = read(DOC);
    let map = dispatch_map();

    // 연기 검사 — 파서가 죽으면 세 집합이 다 비고, 빈 집합끼리는 언제나 같다.
    assert!(
        map.len() >= MIN_DISPATCH_ARMS,
        "dispatch 팔을 {} 개밖에 못 읽었다(하한 {MIN_DISPATCH_ARMS}) — 파서가 죽었으면 \
         아래 세 집합이 전부 비어 서로 같아지고, 이 가드는 아무것도 안 본 채 초록이 된다",
        map.len()
    );

    let marked = dagger_marked(&doc);
    let listed = footnote_enumerated(&doc);
    let gated = gated_methods(&map);

    assert!(
        !gated.is_empty(),
        "`{GATE}` 를 부르는 핸들러를 하나도 못 찾았다 — 게이트가 사라졌거나 스캔 루트가 \
         어긋났다. 0 을 '게이트 대상 없음' 으로 읽지 않는다"
    );

    assert_eq!(
        marked,
        gated,
        "† 가 붙은 메서드와 런타임 게이트가 실제로 걸린 메서드가 다르다.\n  \
         † 만 있고 게이트 없음: {:?}\n  게이트만 있고 † 없음: {:?}\n\
         † 는 게이트의 존재를 주장하는 표식이다 — 주장과 성질이 갈리면 문서가 거짓말을 \
         하거나 구현이 빠진 것이다. 어느 쪽인지는 ADR-0115 의 기준(프로세스 밖으로 나가는 \
         입력 조작인가)으로 판단해라.",
        marked.difference(&gated).collect::<Vec<_>>(),
        gated.difference(&marked).collect::<Vec<_>>()
    );
    assert_eq!(
        listed,
        gated,
        "각주 본문이 열거하는 메서드와 실제 게이트 대상이 다르다.\n  \
         열거만 됨: {:?}\n  게이트만 됨: {:?}",
        listed.difference(&gated).collect::<Vec<_>>(),
        gated.difference(&listed).collect::<Vec<_>>()
    );
}

/// 표식과 성질이 갈렸을 때 **정말 잡히는가.** 세 집합을 합성으로 흔든다 — 실제 문서를
/// 고쳐서 재는 변이는 복원이 필요하고, 여기서는 필요 없다.
#[test]
fn a_dagger_without_a_gate_is_caught() {
    let doc = read(DOC);
    let real = dagger_marked(&doc);
    assert!(
        real.len() >= 4,
        "† 가 붙은 행이 {} 개뿐이다 — 변이 대조가 약해진다",
        real.len()
    );

    // ① † 를 한 행에 더 붙인다(게이트 없는 메서드에).
    let victim = "debug.switch_workspace";
    assert!(
        !real.contains(victim),
        "{victim} 에 이미 † 가 붙어 있다 — 이 변이는 아무것도 안 바꾼다"
    );
    let mutated: String = doc
        .lines()
        .map(|l| {
            if l.trim_start().starts_with('|') && l.contains(&format!("`{victim}`")) {
                format!("{} †|", l.trim_end().trim_end_matches('|'))
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let after = dagger_marked(&mutated);
    assert!(
        after.contains(victim),
        "게이트 없는 메서드에 † 를 붙였는데 파서가 못 봤다 — 이 가드는 † 추가를 못 잡는다"
    );
    assert_eq!(
        after.len(),
        real.len() + 1,
        "† 하나를 더했는데 집합 크기가 1 만큼 안 늘었다 — 파서가 행을 잘못 세고 있다"
    );
    // 파서가 본다는 것과 **판정이 뒤집힌다**는 것은 다른 명제다. 고리를 닫는다.
    let gated = gated_methods(&dispatch_map());
    assert_eq!(real, gated, "변이 전에는 두 집합이 같아야 한다");
    assert_ne!(
        after, gated,
        "† 를 게이트 없는 메서드에 붙였는데 본 판정의 집합 동등이 여전히 성립한다 — \
         파서는 봤지만 판정은 안 바뀐다"
    );

    // ② 각주 열거에서 하나를 빼도 잡히는가 — 개수가 줄어드는 방향.
    let listed = footnote_enumerated(&doc);
    let dropped = listed
        .iter()
        .next()
        .expect("열거가 비어 있을 리 없다")
        .clone();
    let shrunk = doc.replace(&format!("`{dropped}` · "), "");
    let after_listed = footnote_enumerated(&shrunk);
    assert_eq!(
        after_listed.len(),
        listed.len() - 1,
        "각주 열거에서 `{dropped}` 를 뺐는데 파서가 여전히 같은 수를 센다"
    );
    assert!(!after_listed.contains(&dropped));
}

/// 각주가 **게이트 없는 반례로 언급하는** 이름을 열거로 오독하지 않는가.
///
/// 각주 본문에는 게이트 대상이 아닌 이름도 나온다(`engine.input_simulation_enabled` 같은
/// 필드, 그리고 반례 `surface.ime_*`). 문장 구조로 열거 구간을 자르는 것이 그 오독을
/// 막는 유일한 수단이라, 그 자름 자체를 못박는다.
#[test]
fn the_footnote_parser_reads_only_the_enumeration() {
    let doc = read(DOC);
    let listed = footnote_enumerated(&doc);
    let whole_line = doc
        .lines()
        .find(|l| l.trim_start().starts_with('†'))
        .expect("각주 줄");
    let everything: BTreeSet<String> = backticked_methods(whole_line).into_iter().collect();

    assert!(
        everything.len() > listed.len(),
        "각주 줄 전체에서 뽑은 이름({})이 열거 구간에서 뽑은 것({})보다 많지 않다 — \
         자름이 아무것도 안 자르고 있다면 이 대조는 의미가 없다: {everything:?}",
        everything.len(),
        listed.len()
    );
    for extra in everything.difference(&listed) {
        assert!(
            !listed.contains(extra),
            "열거 밖의 이름 `{extra}` 이 열거로 읽혔다"
        );
    }
}

/// 거절 팔을 지도에서 빼는 면제가 **진짜 결손을 가리지 않는가.**
///
/// 이 면제는 실제 결함을 고치려고 넣은 것이라(상보 팔이 실제 핸들러를 덮었다), 그 면제
/// 창 안쪽에 진짜 위반을 심었을 때 여전히 잡히는지를 여기서 못 박는다. 안 그러면 면제
/// 자체가 구멍이 된다.
#[cfg(test)]
mod exemption_mutations {
    use super::*;

    fn fns(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    /// 거절 팔이 있어도 **실제 핸들러의 게이트 여부**로 판정한다 — 이번에 고친 형태.
    #[test]
    fn a_refusal_arm_does_not_hide_the_real_handler() {
        let src = r#"
match m {
    "surface.raw_key" => input_source::handle_raw_key(state, engine, id, p),
    "surface.raw_key" => JsonRpcResponse::error(id.clone(), -32015, WHY),
}
"#;
        let map = dispatch_map_of(src);
        assert_eq!(
            map.get("surface.raw_key").map(BTreeSet::len),
            Some(1),
            "거절 팔이 지도에 들어왔거나 실제 핸들러가 사라졌다: {map:?}"
        );
        let gated = gated_methods_with(&map, &fns(&["handle_raw_key"]));
        assert!(
            gated.contains("surface.raw_key"),
            "실제 핸들러가 게이트를 부르는데 거절 팔 때문에 게이트 없음으로 읽혔다"
        );
    }

    /// 면제 창 안의 진짜 위반 — 게이트 없이 **실행되는** 길이 하나라도 있으면 잡는다.
    #[test]
    fn an_ungated_execution_path_is_still_caught() {
        let src = r#"
match m {
    "surface.raw_key" => input_source::handle_raw_key(state, engine, id, p),
    "surface.raw_key" => other::handle_raw_key_fallback(state, engine, id, p),
    "surface.raw_key" => JsonRpcResponse::error(id.clone(), -32015, WHY),
}
"#;
        let map = dispatch_map_of(src);
        let gated = gated_methods_with(&map, &fns(&["handle_raw_key"]));
        assert!(
            !gated.contains("surface.raw_key"),
            "게이트 없이 실행되는 팔이 남아 있는데 게이트됨으로 읽었다 — `all` 이 아니라 \
             `any` 로 판정하고 있다"
        );
    }

    /// 거절 팔**만** 있는 메서드는 게이트된 것이 아니다(빈 집합을 참으로 읽지 않는다).
    #[test]
    fn a_method_with_only_refusals_is_not_gated() {
        let src = r#"
match m {
    "ns.only_refused" => JsonRpcResponse::error(id.clone(), -32015, WHY),
}
"#;
        let map = dispatch_map_of(src);
        let gated = gated_methods_with(&map, &fns(&["handle_raw_key"]));
        assert!(
            gated.is_empty(),
            "빈 호출 집합을 게이트됨으로 읽었다: {gated:?}"
        );
    }
}
