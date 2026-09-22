//! 번들 plugin 이 IPC namespace 를 점유하면, 그 이름 아래 **host 가 구현한 메서드**는
//! 외부 호출에서 plugin 으로 forward 된다. plugin 의 inbound dispatch 에 그 이름의
//! arm 이 없으면 host 구현은 **외부에서만** 안 닿는다 — plugin 이 설치돼 있으면 막히고
//! 빠지면 열리는, 설치 상태에 따라 흔들리는 표면이 된다.
//!
//! 실측(2026-09-05, 두 조합 × plugin 유무 3 세계 실행 census):
//!
//! | 세계 | `markdown.navigate` 응답 |
//! |------|--------------------------|
//! | gui, plugin 없음 | `-32602 missing field surface_id` (host arm 이 답했다) |
//! | gui, plugin 설치 | `-32601 method 'markdown.navigate' not found` (plugin 이 답했다) |
//! | headless | `-32601` (host arm 자체가 gui 게이트) |
//!
//! 같은 이름이 **누가 답하느냐에 따라** 다른 결과를 냈다. `image.open`/`image.list` 는
//! 같은 형태인데 plugin 이 self-call trampoline 로 host 에 돌려주고 있어 어느 세계에서도
//! host arm 에 닿는다. 즉 관례가 둘이었던 것이 아니라 하나였고 이탈이 하나였다.
//!
//! ## 왜 dispatch 본문만 보는가 (이 가드의 핵심)
//!
//! 이탈하던 시점에도 `"markdown.navigate"` 라는 **문자열은 plugin 소스에 있었다** —
//! plugin 이 host 로 *거는* `host.call("markdown.navigate", …)` 자리다. 파일 전체에서
//! 리터럴을 세는 판정은 그래서 그때도 초록이었다. 방향이 반대인 두 자리가 같은 문자열을
//! 쓰므로, **inbound dispatch 함수의 본문**만 잘라 보는 것이 이 판정의 전부다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use tasty_ipc::method_meta::METHOD_TABLE;

use super::{fn_body, repo_root};

const CRATES_DIR: &str = "crates";
const PLUGIN_CRATE_PREFIX: &str = "tasty-plugin-";
const MANIFEST_NAME: &str = "tasty-plugin.toml";

/// plugin 이 host→plugin 호출을 받는 자리. SDK trait 의 메서드 이름이다.
const DISPATCH_FN: &str = "fn handle_ipc_method";

/// **선언된 namespace prefix** 수의 하한 — 연기 검사다. 아래 카운터는 plugin 마다가
/// 아니라 `[[contributes.ipc_namespace]]` **한 블록마다** 증가한다.
///
/// 이 값에 원래 적혀 있던 근거는 "2026-09-05 실측 2 (image, markdown)" 였는데, **그것은
/// 이 카운터의 값이 아니다.** 2 는 "호스트 메서드 prefix 와 겹치는 것" 의 수로 읽힌다 —
/// 그 둘이 이 파일의 판정 대상이라 헷갈리기 쉽다. 이 카운터가 세는 것은 겹침과 무관하게
/// **선언 전부**다.
///
/// 2026-09-06 이 카운터를 실행해 **6** 이었다(agent_stream · claude · codex · html ·
/// image · markdown — 여섯 plugin 이 하나씩).
///
/// 하한을 4 로 둔 근거는 인구가 아니라 **부분 사멸의 형태**다. 완전 사멸(열거 실패 ·
/// 파싱 실패)은 0 이라 어떤 하한에도 걸리지만, 한 종류가 통째로 빠지는 형태는 줄어든
/// 수로 나타난다. 관측한 여섯은 `description_i18n_key` 를 가진 셋과 안 가진 셋으로
/// 갈리므로, 파서가 한 변종을 놓치면 3 이 된다 — 옛 하한 2 는 그것을 통과시켰다.
const MIN_DECLARED_NAMESPACES: usize = 4;

/// 호스트 메서드 수의 하한. 표가 비면 아래 포함 판정이 빈 집합끼리라 그냥 통과한다.
/// 값의 근거: 2026-09-05 실측 `METHOD_TABLE.len()` = 276.
const MIN_HOST_METHODS: usize = 200;

/// **두 변이 실제로 만난 횟수**의 하한 — 이 시험이 무엇이든 판정한 횟수다.
///
/// 위 두 하한은 각 변의 크기만 본다. 그런데 판정이 일어나는 자리는 **교집합**이고,
/// 두 변이 아무리 커도 교집합이 비면 비교는 한 번도 안 일어난 채 초록이다. 그 초록은
/// "가려진 메서드가 없다" 가 아니라 **"아무것도 안 봤다"** 인데 둘의 관측이 같다.
///
/// 2026-09-06 실측 **2** — `image`(host 7 건) · `markdown`(host 1 건).
/// 선언은 여섯인데 넷(`agent_stream` · `claude` · `codex` · `html`)은 host 쪽에 같은
/// prefix 의 메서드가 없어 `continue` 로 빠진다. 즉 이 시험이 실제로 판정하는 것은
/// 선언의 3 분의 1 이다.
///
/// 하한을 2 가 아니라 **1** 로 둔다: `markdown` 의 host 메서드 한 건이 없어지면
/// 1 이 되는 것이 정상이고, 그것은 이 시험이 잡으려는 결함이 아니다. 1 이 주장하는
/// 것은 하나뿐이다 — **두 변을 잇는 join(prefix 문자열 일치)이 살아 있는가.**
/// 그것이 죽으면 0 이 되고, 그때 위 두 하한은 **둘 다 통과한다.**
const MIN_JOINED_PREFIXES: usize = 1;

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// `crates/tasty-plugin-*/` 중 매니페스트를 가진 디렉터리.
fn bundled_plugin_dirs() -> Vec<PathBuf> {
    let root = repo_root().join(CRATES_DIR);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).expect("crates 디렉터리를 읽을 수 없다") {
        let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PLUGIN_CRATE_PREFIX) && path.join(MANIFEST_NAME).is_file() {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// 매니페스트를 **실제 파서로** 읽는다(역직렬화까지, 검증은 별개).
fn parse_manifest(dir: &std::path::Path) -> tasty_plugin_manifest::Manifest {
    let text = read(&dir.join(MANIFEST_NAME));
    toml::from_str(&text)
        .unwrap_or_else(|e| panic!("{}/{MANIFEST_NAME} 파싱 실패: {e}", dir.display()))
}

/// 매니페스트가 선언한 `[[contributes.ipc_namespace]]` prefix — **실제 파서로** 읽는다.
/// 정규식으로 긁으면 주석 처리된 블록이나 다른 테이블의 `prefix =` 를 같이 집는다.
fn declared_prefixes(dir: &std::path::Path) -> Vec<String> {
    parse_manifest(dir)
        .contributes
        .ipc_namespace
        .iter()
        .map(|d| d.prefix.clone())
        .collect()
}

/// plugin 크레이트의 `src/` 전체에서 inbound dispatch 본문들을 모은다.
fn dispatch_bodies(dir: &std::path::Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("src")];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries {
            let entry = entry.expect("디렉터리 항목을 읽을 수 없다");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Some(body) = fn_body(&read(&path), DISPATCH_FN)
            {
                out.push((path, body));
            }
        }
    }
    out
}

/// 본문의 `match` 팔 패턴에 나타나는 `"<prefix>.…"` 이름.
///
/// 팔 패턴만 센다. 본문 아무 데서나 `"<prefix>.` 를 찾던 판독은 주석 속 인용이나 로그
/// 문자열, 다른 메서드로 거는 `host.call("<prefix>.…")` 의 인자까지 "받는다" 로 셌다 — 이
/// 시험의 명제는 포함(host ⊆ 받는 것)이라 넘치게 센 이름은 arm 이 없어도 초록으로 나간다.
/// 팔은 공용 판정기(`tasty_doc_guards::match_arms`)가 뗀다.
fn handled_methods(body: &str, prefix: &str) -> BTreeSet<String> {
    use tasty_doc_guards::match_arms::{Source, matching_close};
    let src = Source::new(body);
    let code = src.code.as_str();
    let want = format!("{prefix}.");
    let mut out = BTreeSet::new();
    let mut from = 0usize;
    while let Some(at) = code[from..].find("match ") {
        let at = from + at;
        from = at + "match ".len();
        if code[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let Some(open) = code[from..].find('{').map(|k| from + k) else {
            break;
        };
        let Some(close) = matching_close(code, open) else {
            continue;
        };
        let arms = src
            .match_arms(open..close + 1)
            .unwrap_or_else(|e| panic!("inbound dispatch 의 팔을 못 읽었다 — {e}"));
        for arm in arms {
            for alt in src.alternatives(&arm.pattern) {
                if let Some(name) = src.plain_string(&alt)
                    && name.starts_with(&want)
                {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

/// prefix → 그 아래 host 가 등재한 메서드.
fn host_methods_by_prefix() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, _) in METHOD_TABLE {
        if let Some((p, _)) = name.split_once('.') {
            out.entry(p.to_string()).or_default().insert((*name).into());
        }
    }
    out
}

/// 번들 plugin 이 점유한 namespace 아래의 host 메서드는 **전부** 그 plugin 의 inbound
/// dispatch 가 받는다.
#[test]
fn every_host_method_under_a_bundled_namespace_is_handled_by_that_plugin() {
    assert!(
        METHOD_TABLE.len() >= MIN_HOST_METHODS,
        "호스트 메서드가 {} 건뿐이다(하한 {MIN_HOST_METHODS}, 2026-09-05 실측 276). \
         표가 비면 아래 포함 판정은 빈 집합끼리라 그냥 통과한다",
        METHOD_TABLE.len()
    );
    let by_prefix = host_methods_by_prefix();

    // 수가 아니라 **목록**으로 모은다. 하한이 터질 때 읽는 사람이 "탐색이 죽었다" 와
    // "선언이 정말 줄었다" 를 가르려면 무엇이 세어졌는지가 보여야 한다.
    let mut declared: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    // 두 변이 실제로 만난 것 — 이 시험이 무엇이든 판정한 자리다. 수가 아니라 목록으로
    // 모은다: 0 이 아닐 때도 "무엇이 빠졌나" 를 declared 와 차집합으로 읽을 수 있다.
    let mut joined: Vec<String> = Vec::new();
    for dir in bundled_plugin_dirs() {
        for prefix in declared_prefixes(&dir) {
            declared.push(format!(
                "{}::{prefix}",
                dir.file_name().unwrap_or_default().to_string_lossy()
            ));
            let Some(host) = by_prefix.get(&prefix) else {
                // host 가 그 이름 아래 아무것도 구현하지 않았다 — 가려질 것이 없다.
                continue;
            };
            joined.push(format!("{prefix}(host {} 건)", host.len()));
            let bodies = dispatch_bodies(&dir);
            assert!(
                !bodies.is_empty(),
                "{} 에서 `{DISPATCH_FN}` 본문을 하나도 못 잘랐다 — 대조군이 죽었다. \
                 SDK trait 의 이름이 바뀌었는지 확인해라",
                dir.display()
            );
            let handled: BTreeSet<String> = bodies
                .iter()
                .flat_map(|(_, b)| handled_methods(b, &prefix))
                .collect();
            for m in host.difference(&handled) {
                missing.push(format!(
                    "{m} — host 가 구현했는데 {} 의 inbound dispatch 에 arm 이 없다",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        joined.len() >= MIN_JOINED_PREFIXES,
        "이 시험이 실제로 판정한 자리가 {} 개다(하한 {MIN_JOINED_PREFIXES}).\n  \
         만난 것: {:?}\n  \
         선언된 것: {:?}\n  \
         판정은 두 변의 **교집합**에서만 일어난다. 위 두 하한(호스트 메서드 수 · 선언 \
         수)은 각 변의 크기만 보므로, 교집합이 비어도 **둘 다 통과한다** — 그때 아래 \
         포함 판정은 한 번도 안 돌고 시험은 초록이다. 그 초록의 뜻은 \"가려진 메서드가 \
         없다\" 가 아니라 \"아무것도 안 봤다\" 인데, 두 관측이 같다.\n  \
         ★ 세계가 둘이고 **이 메시지만으로는 안 갈린다.** 가르는 조작은 하나다 — \
         `host_methods_by_prefix()` 의 키를 찍어 위 '선언된 것' 의 prefix 문자열과 \
         맞춰봐라:\n  \
         (1) 키에 그 prefix 가 **있는데** 안 만났다 → join 이 죽었다(한쪽의 문자열 \
         형태가 달라졌다. 대소문자 · 구분자 · 접미사). 이 하한이 잡으려던 것이 바로 \
         그것이다. 하한을 건드리지 마라.\n  \
         (2) 키에 **없다** → host 쪽에 그 이름의 메서드가 정말 없다. 그러면 이 시험은 \
         지킬 대상 자체가 없다. **하한을 0 으로 내리지 마라** — 0 은 (1) 을 영영 \
         안 보이게 만든다. 대신 이 시험이 무엇을 지키는지 다시 적어라: 그 상태에서 \
         이 시험은 어떤 회귀도 못 잡으므로, 남길 이유가 있으면 그 이유를 쓰고 없으면 \
         시험째 지우는 것이 맞다. 하한만 내려 초록으로 만드는 것이 셋 중 유일하게 \
         틀린 답이다.",
        joined.len(),
        joined,
        declared,
    );

    assert!(
        declared.len() >= MIN_DECLARED_NAMESPACES,
        "선언된 ipc_namespace 가 {} 개뿐이다(하한 {MIN_DECLARED_NAMESPACES}). \
         집힌 것: {:?}\n\
         이 수만으로는 무엇이 일어났는지 안 정해진다. 세계가 셋이고, **목록으로 \
         갈리는 것은 첫째뿐이다.**\n\
         (1) 비었거나, 집힌 `<디렉터리>::<prefix>` 의 디렉터리가 `crates/` 에 실제로 \
         없다 → 열거·파싱이 죽었다. 하한을 건드리지 마라.\n\
         (2) 디렉터리는 실재하는데 이름이 줄어 있다 → 목록은 여기서 더 못 가른다. \
         빠진 디렉터리의 `tasty-plugin.toml` 을 열어 `[[contributes.ipc_namespace]]` 가 \
         정말 없는지 봐라. **있는데 목록에 없으면 파서가 그 형태를 놓친 것이고, 그것이 \
         이 하한이 잡으려던 바로 그 결함이다.** 하한을 건드리지 마라.\n\
         (3) 그 매니페스트에 선언이 없거나, plugin 크레이트 자체가 없어졌다 → 그때만 \
         하한을 내린다.\n\
         어느 세계든 이 검사를 지우거나 `#[ignore]` 로 덮지 마라. 그리고 (3) 에서 새 \
         값 N 을 쓰려면 **'어떤 부분 사멸이 N 미만을 만드는가' 를 갈래 이름과 그 수로** \
         상수 주석에 적어라 — 지금 값은 인구가 아니라 그 형태로 정했다(한 변종이 통째로 \
         빠지면 3). 못 적으면 그 N 은 아무것도 안 잡는 값이고, 그때는 하한이 아니라 \
         검사가 낡은 것이다",
        declared.len(),
        declared
    );
    assert!(
        missing.is_empty(),
        "번들 plugin 이 점유한 namespace 아래에서 host 구현이 외부 호출에 안 닿는다.\n\
         plugin 이 설치돼 있으면 막히고 빠지면 열리는 표면이 된다 — \
         `image.open`/`image.list` 처럼 self-call trampoline arm 을 두거나, host 가 \
         그 이름을 구현하지 않게 해라.\n  {}",
        missing.join("\n  ")
    );
}

/// 판정이 **inbound dispatch 본문만** 본다.
///
/// 이 가드가 잡아야 했던 실제 이탈은 같은 문자열이 파일 안 다른 자리(plugin → host 로
/// *거는* `host.call`)에 있었다. 파일 전체를 세면 그때도 초록이었다 — 그래서 자르기가
/// 실제로 좁혀졌는지를 여기서 못 박는다.
#[test]
fn the_cut_is_the_dispatch_body_not_the_whole_file() {
    let src = "\
fn other_before() { emit(\"ns.before\"); }
fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> R {
    match ctx.method.as_str() {
        \"ns.inside\" => ok(),
        other => not_found(other),
    }
}
fn other_after() { host.call(\"ns.after\", p); }
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    assert!(found.contains("ns.inside"), "본문 안의 이름은 잡아야 한다");
    assert!(
        !found.contains("ns.before") && !found.contains("ns.after"),
        "본문 밖의 같은 prefix 리터럴을 집었다 — 자르기가 안 좁혀졌다: {found:?}"
    );
    assert!(
        !body.contains("other_after"),
        "본문이 함수 끝을 넘어 이어졌다"
    );
}

/// 중괄호 세기가 문자열 안의 `{`/`}` 에 속지 않는다.
#[test]
fn braces_inside_string_literals_do_not_close_the_body() {
    let src = "\
fn handle_ipc_method(&mut self) -> R {
    emit(format!(\"{}\", x));
    emit(\"}\");
    match m { \"ns.inside\" => ok() }
}
fn after() { emit(\"ns.after\"); }
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    assert!(
        found.contains("ns.inside") && !found.contains("ns.after"),
        "문자열 안 중괄호에 속아 본문이 일찍 끊기거나 넘쳤다: {found:?}"
    );
}

/// 팔이 아닌 자리의 같은 prefix 리터럴 — 주석 속 인용, 로그 문자열, 다른 메서드로 거는
/// 호출 인자 — 은 "받는다" 가 아니다. 셌다면 arm 없는 host 메서드가 초록으로 나간다.
#[test]
fn only_arm_patterns_count_as_handled() {
    let src = "\
fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> R {
    // \"ns.commented\" 는 여기서 받지 않는다
    tracing::warn!(\"ns.logged 는 아직 없다\");
    let _ = host.call(\"ns.forwarded\", p);
    match ctx.method.as_str() {
        \"ns.inside\" | \"ns.also\" => ok(),
        \"ns.guarded\" if ready => ok(),
        other => not_found(other),
    }
}
";
    let body = fn_body(src, DISPATCH_FN).expect("본문을 잘라야 한다");
    let found = handled_methods(&body, "ns");
    let want: BTreeSet<String> = ["ns.inside", "ns.also", "ns.guarded"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(found, want, "팔이 아닌 자리의 이름을 셌거나 팔을 놓쳤다");
}

// ─── 새 매니페스트가 들어올 때 무엇이 그것을 처음 보는가 ──────────────────────

/// 매니페스트를 가진 번들 plugin 수의 하한 — **연기 검사**. 디렉터리 열거가 죽으면
/// 아래 전수 명제는 빈 순회라 그냥 통과한다. 값의 근거: 2026-09-06 실측 9.
const MIN_BUNDLED_PLUGINS: usize = 6;

/// 번들 plugin 매니페스트는 **전부** 실제 검증(`Manifest::validate`)을 통과한다.
///
/// ## 왜 이것이 따로 필요했나
///
/// 매니페스트 검증은 지금까지 두 자리에서만 일어났다: **런타임**(`Manifest::load` —
/// plugin 이 뜰 때)과 **plugin 별 통합 테스트**
/// ([본보기](../../crates/tasty-plugin-html/tests/manifest_loads.rs)). 뒤엣것은
/// 새 plugin 크레이트가 들어올 때 **자동으로 안 따라온다** — 손으로 파일을 하나 더
/// 만들어야 하고, 안 만들면 아무 일도 안 일어난다.
///
/// 실측(2026-09-06): 매니페스트를 가진 번들 plugin 9, 그중 `Manifest::load` 를 부르는
/// 테스트를 가진 것 **3**(html · markdown · mesh-demo). 나머지 **6** 은 빌드·테스트가
/// 전부 초록인 채로 **런타임에만** 거절된다 — 그 형태의 실패는 "plugin 이 안 뜬다" 로
/// 나타나고, 매니페스트를 의심하기 전에 다른 것을 먼저 의심하게 된다.
///
/// 이 판정의 모수는 **디렉터리 열거**라 새 plugin 이 자동으로 들어온다. 위 파일
/// 상단의 `bundled_plugin_dirs()` 를 그대로 쓴다 — 같은 물음에 모수를 둘로 만들지
/// 않는다.
#[test]
fn every_bundled_manifest_passes_the_real_validation() {
    let dirs = bundled_plugin_dirs();
    assert!(
        dirs.len() >= MIN_BUNDLED_PLUGINS,
        "매니페스트를 가진 번들 plugin 이 {} 개뿐이다(하한 {MIN_BUNDLED_PLUGINS}, \
         2026-09-06 실측 9). 디렉터리 열거가 죽으면 아래 전수 명제는 빈 순회다 — \
         이 하한이 막는 것이 그 조용한 통과 하나다. 그래서 번들 plugin 이 정말 줄어든 \
         것이면 **내려도 된다**(0 만 아니면 그 일은 계속한다). 다만 이 검사 자체를 \
         지우지는 마라",
        dirs.len()
    );
    for dir in &dirs {
        parse_manifest(dir).validate().unwrap_or_else(|e| {
            panic!(
                "{}/{MANIFEST_NAME} 이 검증을 통과하지 못한다: {e}\n\
                 이 상태의 plugin 은 **런타임에만** 거절된다 — 빌드도 테스트도 초록이다",
                dir.display()
            )
        });
    }
}

/// 위 검증이 실제로 무언가를 **거절하는가.**
///
/// 전수 초록인 불변식은 레포 안에 위반 표본이 없다 — 그래서 이 대조만은 실물의
/// 대칭차로 못 잡는다. 대신 **실물을 최소로 흔든다**: 실제로 namespace 를 선언한
/// 번들 매니페스트를 그대로 읽어, prefix 한 필드만 호스트 예약어로 바꾼다. 합성
/// 픽스처가 아니라서 다른 검증 규칙에 먼저 걸릴 자리가 없고, 흔든 것이 정확히
/// 판정 대상이다.
#[test]
fn the_validation_rejects_a_reserved_prefix() {
    let mut with_namespace = bundled_plugin_dirs()
        .into_iter()
        .map(|d| parse_manifest(&d))
        .filter(|m| !m.contributes.ipc_namespace.is_empty());
    let mut manifest = with_namespace
        .next()
        .expect("namespace 를 선언한 번들 매니페스트가 없다 — 대조군이 죽었다");

    // 팔 1: 흔들기 전 — 통과해야 한다.
    manifest
        .validate()
        .expect("실물 매니페스트가 흔들기 전에 이미 실패한다 — 대조가 성립 안 한다");

    // 팔 2: prefix 한 필드만 호스트 예약어로.
    let reserved = tasty_plugin_manifest::validators::RESERVED_IPC_PREFIXES
        .first()
        .expect("예약 목록이 비었다 — 흔들 값이 없다");
    manifest.contributes.ipc_namespace[0].prefix = (*reserved).to_string();
    let err = manifest
        .validate()
        .expect_err("예약된 prefix 를 선언했는데 검증이 통과했다");
    assert!(
        format!("{err}").contains(reserved),
        "거절은 했는데 이유가 그 prefix 가 아니다 — 다른 규칙에 먼저 걸렸다: {err}"
    );
}
