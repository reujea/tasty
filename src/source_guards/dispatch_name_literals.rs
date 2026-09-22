//! IPC 라우터는 **스캔이 볼 수 있는 이름**으로 갈린다.
//!
//! 이 레포의 여러 가드가 라우터를 텍스트로 읽어 메서드 이름을 뽑는다 — 헤드리스 커버리지,
//! † 게이트 대조, 라우터 표 정합이 다 그렇다. 그 판정들은 이름이 **문자열 리터럴**이라는
//! 전제 위에 있다. 이름이 매크로에서 나오거나 상수와 맞대지면 그 갈래는 어느 목록에도 안
//! 들어오고, 답하지도 사유가 적혀 있지도 않은 메서드가 **조용히** 생긴다.
//!
//! # 이 부류는 수를 세는 검사로 못 잡는다 (실측)
//!
//! 가드들은 저마다 "몇 개나 뽑혔나" 의 하한을 갖고 있고, 그 하한이 이 사각을 본다고
//! 적혀 있었다. 2026-09-05 에 변이로 쟀고 거짓이다:
//!
//! - 리터럴 하나를 매크로 뒤로 숨기면 항목이 **하나** 줄 뿐이라 하한에 안 걸린다.
//! - 매크로가 만든 이름으로 갈래를 **더하면** 항목 수가 아예 안 변한다.
//!
//! 뒤쪽이 핵심이다. 하한은 줄어드는 방향만 볼 수 있어서 "안 보이는 이름이 느는 것" 은
//! **원리적으로** 못 본다 — 하한을 실측값까지 조여도 못 잡는다.
//!
//! # 무엇을 재는가
//!
//! 라우터가 이름을 맞댈 때 그 값이 문자열 리터럴인가. 판정 자리는 넷이다 —
//! `== <값>` · `.starts_with(<값>)` · `match ….as_str()` 의 팔 · `match <식>` 의 팔.
//! 값을 위임 함수 인자로 **넘기기만** 하는 자리는 여기서 이름을 가르지 않으므로 대상이
//! 아니다.
//!
//! # 명부가 둘이다 — 이름을 **어디서** 읽느냐로 갈린다
//!
//! 1. `ROUTERS` — `request.method` 로 직접 가르는 라우터. 파일 단위 명부다.
//! 2. `DELEGATED_ROUTERS` — 이름을 **인자로 받아**(`method: &str`) 가르는 위임 라우터.
//!    함수 단위 명부다.
//!
//! 둘은 같은 물음("갈래를 치는 값이 리터럴인가")에 답하고 **읽는 표현식만 다르다.**
//! 그래서 판정기 본체는 하나이고(`opaque_sites_for`), 표현식을 인자로 받는다 — 상수
//! 하나가 두 모수를 판정하면 둘 중 하나는 반드시 틀린다.
//!
//! 위임 쪽이 오래 **범위 밖**이었던 이유는 "무엇이 메서드 이름인가" 를 함수 경계
//! 너머로 판정할 수단이 없어서였다. 앞 회차가 `callers_of` 로 호출자 방향 한 단계를
//! 지었고, 이 회차의 `arg_at` 이 그 자리의 **인자**를 읽는다. 두 개가 붙어야
//! `the_roster_is_reached_from_the_request_method` 가 성립한다.
//!
//! # 두 명부 다 **두 방향으로** 못박는다
//!
//! 명부에 있는 자리는 리터럴만 쓰고, 명부 밖은 판정 자리를 갖지 않는다. 뒤쪽이 없으면
//! 새 라우터가 명부에 안 들어온 채 아무도 안 보는 자리가 된다.
//!
//! # 초록이 뜻하지 않는 것
//!
//! - **`crates/` 의 같은 모양은 안 본다.** 거기 같은 모양이 16 자리 있고 전부 plugin
//!   dispatch 라 모수가 다르다 — 그 수의 술어·트리·사본은 `DELEGATED_SCAN_ROOT` 옆에 적었다.
//! - **위임의 위임은 안 본다.** 위임 라우터가 자기 안에서 또 다른 함수에 이름을 넘기면
//!   그 함수는 `method: &str` 를 받는 자리로 스캔에 다시 잡히므로 명부 대조에는 들어오지만,
//!   이름이 `method` 가 아닌 인자로 넘어가면 안 잡힌다.
//! - **호출자가 지역 변수로 넘기는 자리는 한 단계 더 못 따라간다.** 아홉 중 셋이 그렇고,
//!   무엇을 재면 그 한계가 깨지는지는 `the_roster_is_reached_from_the_request_method` 의
//!   주석에 적었다("구조적 한계" 라고만 쓰면 영구 면제가 된다).

use std::collections::BTreeSet;

use super::{
    CallSite, METHOD_EXPR, arg_at, callers_of, fn_body, fn_spans, mask_non_code, matching_delim,
    opaque_method_sites, opaque_sites_for, repo_root, rust_sources, strip_comments,
};

/// `request.method` 로 갈래를 치는 라우터 전부. 2026-09-05 실측.
const ROUTERS: &[&str] = &[
    "src/adapters/ipc/handler.rs",
    "src/app/dispatch/list_global.rs",
    "src/app/ipc/app_methods.rs",
    "src/app/ipc/debug_methods.rs",
    "src/app/ipc/window_required.rs",
    "src/boot/headless_dispatch.rs",
];

/// 이 규칙을 **재는** 쪽. 가드는 판정 자리의 모양을 합성 입력으로 담으므로 라우터가
/// 아니면서 같은 형태를 갖는다 — 명부 대조에서 뺀다.
///
/// 두 번째 항은 IPC 를 갖지 않는 판정 전용 크레이트다. 거기 라우터가 들어올 수 없어서
/// 빼는 것이고, 들어올 수 있는 자리를 편의로 빼는 것이 아니다. 이 구분이 필요한 이유는
/// 실측이다 — 재는 쪽 파일 하나가 `src/` 밖 그 크레이트로 옮겨가자 명부 대조가 그것을
/// 라우터로 세면서 조립에서 빨갛게 났다. 경로를 옮기면 소속이 바뀌는 부류다.
const GUARD_DIRS: &[&str] = &["src/source_guards/", "crates/tasty-doc-guards/"];

/// `src` 의 **코드**에 판정 자리가 있는가. 주석·문자열은 덮고 본다 — 그 판정 자리를 설명하는
/// doc 주석이나 그 모양을 담은 문자열(라우터를 읽는 시험이 그렇다)은 라우터가 아니다.
fn has_decision_site(src: &str) -> bool {
    let code = mask_non_code(src);
    let src = code.as_str();
    let mut at = 0usize;
    while let Some(i) = src[at..].find(METHOD_EXPR) {
        at += i + METHOD_EXPR.len();
        let rest = src[at..].trim_start();
        if rest.starts_with("==") || rest.starts_with(".starts_with(") {
            return true;
        }
        if let Some(r) = rest.strip_prefix(".as_str()")
            && r.trim_start().starts_with('{')
        {
            return true;
        }
    }
    false
}

#[test]
fn every_router_decides_by_a_name_the_scan_can_see() {
    for rel in ROUTERS {
        let path = repo_root().join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n");
        assert!(
            has_decision_site(&src),
            "{rel} 에 `{METHOD_EXPR}` 로 갈래를 치는 자리가 하나도 없다. 라우터가 아니게 \
             됐으면 명부에서 빼고, 이름을 읽는 표현식이 바뀌었으면 `METHOD_EXPR` 을 \
             고쳐라 — 안 고치면 이 검사는 아무 자리도 안 보면서 초록이다"
        );
        let opaque = opaque_method_sites(&src);
        assert!(
            opaque.is_empty(),
            "{rel} 이 **문자열 리터럴이 아닌 값**으로 메서드 이름을 가른다. 라우터를 \
             텍스트로 읽는 가드들이 그 이름을 못 보고, 답하지도 사유가 적혀 있지도 않은 \
             메서드가 조용히 생긴다. 리터럴로 적어라: {opaque:?}"
        );
    }
}

#[test]
fn no_router_escapes_the_roster() {
    let listed: BTreeSet<&str> = ROUTERS.iter().copied().collect();
    let mut found: Vec<String> = Vec::new();
    for (path, src) in rust_sources() {
        let rel = path.to_string_lossy().replace('\\', "/");
        if GUARD_DIRS.iter().any(|d| rel.starts_with(d)) {
            continue;
        }
        if has_decision_site(&src) && !listed.contains(rel.as_str()) {
            found.push(rel);
        }
    }
    assert!(
        found.is_empty(),
        "`{METHOD_EXPR}` 로 갈래를 치는데 명부에 없는 파일이 있다. 새 라우터는 아무도 \
         안 보는 자리가 된다 — `ROUTERS` 에 넣어라: {found:?}"
    );
}

/// 매크로가 만든 이름을 문다 — 실측으로 뚫렸던 두 형태 그대로.
#[test]
fn a_name_a_macro_makes_is_caught() {
    let hidden = "\
fn pump_ipc(app: &mut App) {
    if cmd.request.method == hidden_name!() { go(); }
}
";
    let body = fn_body(hidden, "fn pump_ipc").unwrap();
    // 이 본문에는 `"ns.method"` 꼴 리터럴이 하나도 없다 — 리터럴만 걷는 스캔에는 이
    // 갈래가 통째로 안 보인다는 것이 전제다.
    assert!(
        !body.contains('"'),
        "고정 입력에 리터럴이 들어갔다 — 전제가 깨졌다"
    );
    assert!(
        !opaque_method_sites(&body).is_empty(),
        "매크로가 만든 이름을 안 봤다"
    );

    let arm = "\
fn pump_ipc(app: &mut App) {
    let r = match cmd.request.method.as_str() {
        \"ns.one\" => a(),
        HIDDEN_NAME => b(),
        other => c(other),
    };
}
";
    let body = fn_body(arm, "fn pump_ipc").unwrap();
    let sites = opaque_method_sites(&body);
    assert_eq!(
        sites.len(),
        1,
        "상수 팔 하나만 걸려야 한다(리터럴 팔과 전부받기 바인딩은 정상이다): {sites:?}"
    );
}

/// 값을 **넘기기만** 하는 자리는 판정 자리가 아니다 — 거짓 양성을 막는 대조군.
#[test]
fn passing_the_name_along_is_not_a_decision() {
    let src = "\
fn pump_ipc(app: &mut App) {
    delegate(&cmd.request.method, &cmd.request.params, id);
    let s = cmd.request.method.as_str();
}
";
    let body = fn_body(src, "fn pump_ipc").unwrap();
    assert!(
        opaque_method_sites(&body).is_empty(),
        "넘기는 자리를 판정으로 셌다"
    );
}

/// 팔의 패턴이 **괄호를 가질 때도** 잡히는가 — 실측으로 뚫렸던 모양 그대로.
///
/// 매크로 호출 패턴은 `mac!()` 처럼 괄호를 담는다. 팔의 끝을 닫는 괄호로도 인정하면
/// 패턴의 시작 자리가 그 괄호 **뒤로** 밀려 패턴이 빈 문자열이 되고, 빈 패턴은 건너뛰어
/// 진다. 게다가 앞 팔이 블록이고 쉼표가 없으며 그 사이에 `#[cfg(...)]` 이 끼는 것이
/// 실제 dispatch 의 흔한 모양이라, 이 셋이 겹친 자리에서 정확히 통과했다.
#[test]
fn a_macro_arm_with_parentheses_is_caught() {
    let src = "\
fn route(request: &Request) -> Option<Response> {
    Some(match request.method.as_str() {
        \"ns.one\" => {
            one(request)
        }
        #[cfg(feature = \"gui\")]
        probe!() => {
            two(request)
        }
        \"ns.three\" => three(request),
        _ => return None,
    })
}
";
    let found = opaque_method_sites(src);
    assert_eq!(
        found.len(),
        1,
        "괄호를 가진 매크로 팔 하나만 걸려야 한다(리터럴 팔과 `_` 는 정상이다): {found:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// 위임 라우터 — 이름을 **인자로 받아** 그 안에서 가르는 자리
// ───────────────────────────────────────────────────────────────────────────

/// 위임 라우터가 이름을 받는 인자 이름. 이것이 곧 판정 표현식이다.
const DELEGATED_PARAM: &str = "method";

/// `method: &str` 를 받아 **그 값으로 갈래를 치는** `fn` 전부. 2026-09-08 실측,
/// 잰 트리 `b134d28e3`(`callers_of` 가 착지한 뒤의 main).
///
/// 수만 적으면 다음 사람이 다른 술어로 세고 같은 이름을 단다. 이 명부를 낳은 술어는
/// [`delegated_routers`] 고, 아래 `no_delegated_router_escapes_the_roster` 가 그
/// 술어로 명부를 **다시 만들어** 대조한다.
const DELEGATED_ROUTERS: &[(&str, &str)] = &[
    ("src/core/request_target.rs", "request_resource_id"),
    (
        "src/adapters/ipc/handler.rs",
        "hard_occupied_structural_guard",
    ),
    ("src/adapters/ipc/handler.rs", "should_rate_limit"),
    (
        "src/adapters/ipc/handler/debug_plugin.rs",
        "handle_event_bus",
    ),
    ("src/adapters/ipc/handler/ime.rs", "handle_ime_method"),
    (
        "src/adapters/ipc/handler/plugin.rs",
        "dispatch_lifecycle_toggle",
    ),
    ("src/adapters/ipc/handler/plugin.rs", "dispatch_readonly"),
    ("src/adapters/ipc/handler/telemetry.rs", "record_ipc_call"),
    ("src/app/ipc/debug_methods.rs", "ipc_debug_fullscreen"),
    ("src/core/request_target.rs", "method_scoped_resource_id"),
];

/// 명부 대조의 사거리. `crates/` 는 뺀다 — 거기 같은 모양이 **16 자리**(2026-09-08,
/// 잰 트리 `b134d28e3`, 사본은 코드가 아닌 부분을 덮은 것) 있고 전부
/// plugin dispatch(`fn call(method: &str)`)다. 그 자리들은 plugin 쪽 명부가 좌변이라,
/// 이 판정기가 함께 세면 같은 자리를 두 판정기가 서로 다른 명부로 판정한다. 빠진 것이
/// 아니라 **다른 모수**고, 그 수를 여기 적어 두는 것이 안 세는 것과 못 세는 것을
/// 가른다.
const DELEGATED_SCAN_ROOT: &str = "src/";

/// 판정 대상 본문은 **주석만 걷은 원문**으로 읽는다.
///
/// `mask_non_code` 로 읽으면 안 된다 — 그 사본은 문자열을 따옴표째 공백으로 덮으므로
/// `method == "x"` 가 `method ==     ` 이 되고, 리터럴 판정이 **전부 거짓 양성**이 된다
/// (실측: 명부 첫 항목에서 판정 자리 셋이 그렇게 걸렸다). 게다가 마스킹은
/// 글자 단위라 한글 주석이 있는 파일에서 바이트 자리가 원문과 어긋난다 — 마스킹된
/// 자리로 원문을 자를 수도 없다.
fn router_source(rel: &str) -> String {
    let path = repo_root().join(rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    strip_comments(&src)
}

/// 시그니처의 인자 목록에서 `DELEGATED_PARAM` 이 **호출 자리 기준 몇 번째**인가.
///
/// 수신자는 인자가 아니다 — `x.f(a, b)` 에서 `a` 가 0 번이므로 `&self` 를 갖는 함수는
/// 자리를 하나 당긴다.
///
/// 반환 자리의 `->` 를 괄호 깊이로 세면 안 된다. `>` 를 닫는 구분자로 취급하면
/// `fn f(a: &dyn Fn() -> u32, method: &str)` 에서 깊이가 인자 목록 한가운데서 0 이
/// 되고, 그 뒤 자리 계산이 통째로 어긋난다(실측: 뺄셈 넘침으로 터졌다).
fn param_index_at_call(src: &str, name: &str) -> Option<usize> {
    let at = src.find(&format!("fn {name}("))?;
    let open = src[at..].find('(')? + at;
    let close = matching_delim(src, open)?;
    let inner = &src[open + 1..close];
    let mut parts: Vec<String> = Vec::new();
    let (mut depth, mut start, mut i) = (0usize, 0usize, 0usize);
    let bytes = inner.as_bytes();
    while i < inner.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'>') => i += 1,
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(inner[start..].trim().to_string());
    parts.retain(|p| !p.is_empty());
    let has_self = parts
        .first()
        .is_some_and(|p| p.replace(['&', ' '], "").trim_start_matches("mut") == "self");
    let idx = parts.iter().position(|p| {
        p.strip_prefix(DELEGATED_PARAM)
            .is_some_and(|r| r.trim_start().starts_with(':'))
    })?;
    // `self` 는 0 번 자리를 차지하므로 `idx` 는 1 이상이다 — 아래 뺄셈이 안전한 이유다.
    Some(if has_self { idx - 1 } else { idx })
}

/// 명부의 술어 그 자체 — `method: &str` 를 받고 **그 값으로** 갈래를 치는 `fn`.
///
/// 명부를 재료로 쓰지 않는다. 명부와 대조하는 쪽이 명부를 읽으면 그 대조는
/// 동어반복이다.
fn delegated_routers() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (path, src) in rust_sources() {
        let rel = path.to_string_lossy().into_owned();
        if !rel.starts_with(DELEGATED_SCAN_ROOT) || GUARD_DIRS.iter().any(|d| rel.starts_with(d)) {
            continue;
        }
        let masked = mask_non_code(&src);
        if !masked.contains(DELEGATED_PARAM) {
            continue;
        }
        for (name, open, close, _) in fn_spans(&masked) {
            if param_index_at_call(&masked, &name).is_none() {
                continue;
            }
            let body = &masked[open..close];
            let decides = body.contains(&format!("{DELEGATED_PARAM} =="))
                || body.contains(&format!("{DELEGATED_PARAM}.starts_with("))
                || body.contains(&format!("match {DELEGATED_PARAM} {{"));
            if decides {
                out.push((rel.clone(), name));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 위임 라우터도 **스캔이 볼 수 있는 이름**으로 갈린다.
#[test]
fn every_delegated_router_decides_by_a_name_the_scan_can_see() {
    let mut seen = 0usize;
    for (rel, name) in DELEGATED_ROUTERS {
        let src = router_source(rel);
        let body = fn_body(&src, &format!("fn {name}(")).unwrap_or_else(|| {
            panic!(
                "{rel} 에서 `fn {name}` 의 본문을 못 잘랐다 — 이름이 바뀌었거나 \
                 옮겨졌다. 못 자른 것은 통과가 아니라 측정 실패다"
            )
        });
        let opaque = opaque_sites_for(&body, DELEGATED_PARAM);
        assert!(
            opaque.is_empty(),
            "{rel} 의 `fn {name}` 이 **문자열 리터럴이 아닌 값**으로 메서드 이름을 \
             가른다. 이 함수는 이름을 인자로 받으므로 `{METHOD_EXPR}` 을 찾는 판정기에는 \
             이 자리가 통째로 안 보인다 — 답하지도 사유가 적혀 있지도 않은 메서드가 \
             조용히 생긴다. 리터럴로 적어라: {opaque:?}"
        );
        seen += 1;
    }
    assert_eq!(
        seen,
        DELEGATED_ROUTERS.len(),
        "명부 항목 수만큼 본문을 안 봤다 — 빈 좌변의 초록은 통과가 아니다"
    );
}

/// 명부 밖에 위임 라우터가 없다 — 새 라우터가 아무도 안 보는 자리가 되지 않게.
#[test]
fn no_delegated_router_escapes_the_roster() {
    let listed: BTreeSet<(String, String)> = DELEGATED_ROUTERS
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let found: BTreeSet<(String, String)> = delegated_routers().into_iter().collect();
    assert!(
        !found.is_empty(),
        "스캔이 위임 라우터를 하나도 못 찾았다 — 술어가 아무 자리도 안 보면서 초록이다"
    );
    assert_eq!(
        found,
        listed,
        "`{DELEGATED_PARAM}: &str` 로 갈래를 치는데 명부와 어긋나는 자리가 있다.\n  \
         명부에 없음(새 라우터 — `DELEGATED_ROUTERS` 에 넣어라): {:?}\n  \
         명부에만 있음(사라졌거나 이름이 바뀌었다): {:?}",
        found.difference(&listed).collect::<Vec<_>>(),
        listed.difference(&found).collect::<Vec<_>>(),
    );
}

/// 명부가 **IPC 메서드 이름**을 받는 자리인지를 호출자 쪽에서 확인한다.
///
/// 이것이 없으면 명부는 측정이 아니라 내 주장이다 — `method: &str` 라는 **이름만으로는**
/// 그 값이 IPC 메서드 이름인지 HTTP 메서드인지 훅 이벤트인지 안 갈린다. 호출 자리에서
/// 그 인자 자리에 무엇이 넘어가는지를 봐야 갈린다(`callers_of` + 이번의
/// `arg_at`).
///
/// # 초록이 뜻하지 않는 것
///
/// **모든 호출자가 `request.method` 를 넘긴다는 것이 아니다.** 아홉 중 여섯만 그렇고,
/// 나머지 셋은 값을 지역 변수로 받는다:
///
/// - `should_rate_limit` · `record_ipc_call` — 둘 다 `canonical` 을 받는다.
///   `handler.rs` 의 `canonicalize_and_route` 가 `alias::canonicalize(&request.method)`
///   로 만든 값이라 **한 단계 더** 따라가야 이름에 닿는다.
/// - `method_scoped_resource_id` — 호출 자리 대부분이 같은 파일 안의 리터럴이다.
///
/// 그 한 단계를 따라가려면 지역 변수의 정의를 찾는 **이름 해소**가 필요하고, 이
/// 디렉토리에는 그 도구가 아직 없다(`callers_of` 가 지은 것은 호출자 방향 한 단계지 이름
/// 해소가 아니다). 그래서 이 시험은 **직접 닿는 자리가 0 이 아닌가**만 묻고, 못 닿는
/// 항목 수를 값으로 못 박는다 — 그 수가 움직이면 여기 적은 근거도 같이 낡는다.
///
/// 이것을 "구조적 한계" 로만 적으면 영구 면제가 된다. 깨는 측정은 하나다:
/// 지역 변수의 대입 자리를 함수 안에서 찾아 그 우변을 읽는 것 — `fn_spans` 로 자른
/// 본문에서 `let <이름> =` 를 찾으면 되고, 새 도구가 아니라 이미 있는 것의 조합이다.
#[test]
fn the_roster_is_reached_from_the_request_method() {
    let (mut direct, mut indirect) = (0usize, 0usize);
    for (rel, name) in DELEGATED_ROUTERS {
        let src = router_source(rel);
        let idx = param_index_at_call(&src, name).unwrap_or_else(|| {
            panic!("{rel} 의 `fn {name}` 에서 `{DELEGATED_PARAM}` 인자 자리를 못 읽었다")
        });
        let sites = callers_of(name, GUARD_DIRS);
        assert!(
            !sites.is_empty(),
            "`{name}` 을 부르는 자리를 하나도 못 찾았다 — 빈 좌변의 초록은 통과가 아니라 \
             미측정이다. 이름이 바뀌었거나 유일한 호출자가 사라졌다"
        );
        let mut reached = false;
        for CallSite {
            rel: crel,
            masked,
            at,
            line,
            ..
        } in &sites
        {
            let arg = arg_at(masked, *at, idx).unwrap_or_else(|| {
                panic!(
                    "{crel}:{line} 의 `{name}` 호출에서 {idx} 번 인자를 못 잘랐다 — \
                     못 자른 상태를 통과로 접지 않는다"
                )
            });
            if arg.contains(METHOD_EXPR) {
                reached = true;
                direct += 1;
            }
        }
        if reached {
            continue;
        }
        indirect += 1;
    }
    assert!(
        direct > 0,
        "`{METHOD_EXPR}` 로 직접 닿는 호출 자리가 0 이다 — 명부가 IPC 메서드 이름을 받는 \
         자리라는 근거가 사라졌다"
    );
    assert_eq!(
        indirect, 3,
        "지역 변수를 거치는 명부 항목 수가 바뀌었다. 늘었으면 이 판정기가 호출자 쪽에서 \
         확인하는 몫이 줄어든 것이고, 줄었으면 명부가 더 촘촘해진 것이다 — 어느 쪽이든 \
         이 수와 위 주석의 이름 목록을 함께 고쳐라"
    );
}
