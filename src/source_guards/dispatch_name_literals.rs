//! IPC 라우터에서 비교하는 메서드 이름이 문자열 리터럴인지 확인한다.
//! 리터럴이 아닌 이름은 문서·게이트·커버리지 검사의 메서드 수집에서 빠질 수 있다.
//! 개수 하한만으로는 새로 추가된 비리터럴 이름을 검출할 수 없다.
//!
//! request.method를 직접 읽는 파일과 method 인자로 분기하는 함수를 따로 등록한다.
//! ==, starts_with, match 분기의 값을 검사하며 다른 함수로 전달만 하는 곳은 제외한다.
//! crates의 플러그인 dispatch는 별도 범위다. 위임 함수가 다른 이름의 인자로 다시 전달하는 경로나
//! 호출자의 지역 변수 대입은 추적하지 않는다.

use std::collections::BTreeSet;

use super::{
    CallSite, METHOD_EXPR, arg_at, callers_of, fn_bodies_of, fn_body, fn_spans, mask_non_code,
    matching_delim, opaque_method_sites, opaque_sites_for, repo_root, rust_sources, strip_comments,
};

/// request.method로 분기하는 라우터 파일. 시험의 리터럴 분기도 같은 문법 검사를 받는다.
const ROUTERS: &[&str] = &[
    "src/app/ipc/routing.rs",
    "crates/tasty-cli/src/contract.rs",
    "src/app/journal.rs",
    "src/app/journal/commands.rs",
    "src/app/journal/commands/headless.rs",
    "src/app/journal/commands/workspace.rs",
    "src/app/journal/commands/forwarding.rs",
    "src/app/journal/commands/replacement.rs",
    "src/app/journal/commands/tests.rs",
    "src/app/journal/commands/replacement_settle_tests.rs",
    "src/app/journal/commands/preparation_tests.rs",
    "src/app/journal/commands/child_input_tests.rs",
    "src/app/journal/commands/recovery_tests.rs",
    "src/app/journal/commands/close_history_tests.rs",
    "src/app/journal/commands/remote_selection_tests.rs",
    "src/app/journal/commands/category.rs",
    "src/app/journal/commands/create_spec.rs",
    "src/app/journal/commands/assembly.rs",
    "src/app/journal/commands/create.rs",
    "src/app/journal/commands/live_resume.rs",
    "src/app/journal/commands/view_completion.rs",
    "src/app/journal/commands/close.rs",
    "src/app/journal/commands/completion.rs",
    "src/adapters/ipc/handler/terminal/fixed_input.rs",
    "src/adapters/ipc/handler.rs",
    "src/app/dispatch/list_global.rs",
    "src/app/ipc/app_methods.rs",
    "src/app/ipc/debug_methods.rs",
    "src/app/ipc/debug_remote_download.rs",
    "src/app/ipc/window_required.rs",
    "src/boot/headless_dispatch.rs",
];

/// 라우터 형식을 합성 입력으로 갖는 검사 코드와 IPC를 처리하지 않는 문서 검사 크레이트는 제외한다.
const GUARD_DIRS: &[&str] = &["src/source_guards/", "crates/tasty-doc-guards/"];

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
            "{rel}에서 `{METHOD_EXPR}`로 분기하는 곳을 찾지 못했다. 라우터 이동 여부와 이름을 읽는 표현식을 확인한다."
        );
        let opaque = opaque_method_sites(&src);
        assert!(
            opaque.is_empty(),
            "{rel}에서 문자열 리터럴이 아닌 값으로 메서드를 구분한다. 다른 소스 검사가 이 이름을 수집할 수 있도록 리터럴로 쓴다: {opaque:?}"
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
        "`{METHOD_EXPR}`로 분기하는 파일이 ROUTERS에 없다. 새 라우터를 등록한다: {found:?}"
    );
}

#[test]
fn a_name_a_macro_makes_is_caught() {
    let hidden = "\
fn pump_ipc(app: &mut App) {
    if cmd.request.method == hidden_name!() { go(); }
}
";
    let body = fn_body(hidden, "fn pump_ipc").unwrap();
    // 리터럴 수집으로는 보이지 않는 분기라는 전제를 확인한다.
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

/// 매크로 괄호를 분기의 끝으로 오인하지 않는지, cfg·쉼표 없는 블록을 함께 넣어 확인한다.
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

const DELEGATED_PARAM: &str = "method";

/// method 인자로 분기하는 함수 목록. delegated_routers로 독립 수집한 결과와 비교한다.
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
        "src/adapters/ipc/handler/agent_children.rs",
        "handle_agent_method",
    ),
    (
        "src/adapters/ipc/handler/plugin.rs",
        "dispatch_lifecycle_toggle",
    ),
    ("src/adapters/ipc/handler/plugin.rs", "dispatch_readonly"),
    ("src/adapters/ipc/handler/plugin_busy.rs", "wanted"),
    ("src/adapters/ipc/handler/telemetry.rs", "ipc_call_event"),
    (
        "src/adapters/ipc/handler/terminal_input.rs",
        "handle_input_rule_update",
    ),
    ("src/app/ipc/debug_methods.rs", "ipc_debug_fullscreen"),
    ("src/app/journal/commands.rs", "host_converts"),
    ("src/app/ipc/debug_methods.rs", "project_active_modal"),
    ("src/core/request_target.rs", "method_scoped_resource_id"),
    (
        "src/core/request_target.rs",
        "needs_open_surfaces_of_every_engine",
    ),
];

/// 위임 라우터는 src만 검사한다. crates의 플러그인 dispatch는 이 호스트 라우터 명부에 포함하지 않는다.
const DELEGATED_SCAN_ROOT: &str = "src/";

/// 리터럴 여부를 판별해야 하므로 문자열을 보존하고 주석만 제거한다.
fn router_source(rel: &str) -> String {
    let path = repo_root().join(rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    strip_comments(&src)
}

/// 호출 인자의 위치에서는 self를 제외한다. 함수 타입의 ->는 괄호 깊이를 바꾸지 않는다.
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
    // self 뒤의 인자이므로 idx는 1 이상이다.
    Some(if has_self { idx - 1 } else { idx })
}

/// 명부를 읽지 않고 method 인자로 분기하는 함수를 수집한다.
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

#[test]
fn every_delegated_router_decides_by_a_name_the_scan_can_see() {
    let mut seen = 0usize;
    for (rel, name) in DELEGATED_ROUTERS {
        let src = router_source(rel);
        // 본문 전체에 대한 전칭 명제라 cfg 로 갈린 정의도 모두 대조한다.
        let bodies = fn_bodies_of(&src, &format!("fn {name}("));
        assert!(
            !bodies.is_empty(),
            "{rel}의 fn {name} 본문을 읽지 못했다. 이동·이름 변경과 파서를 확인한다."
        );
        for body in &bodies {
            let opaque = opaque_sites_for(body, DELEGATED_PARAM);
            assert!(
                opaque.is_empty(),
                "{rel}의 fn {name}에서 문자열 리터럴이 아닌 값으로 메서드를 구분한다. 이 함수는 이름을 인자로 받으므로 `{METHOD_EXPR}` 검색만으로는 검사할 수 없다: {opaque:?}"
            );
        }
        seen += 1;
    }
    assert_eq!(
        seen,
        DELEGATED_ROUTERS.len(),
        "읽은 본문 수가 명부의 항목 수와 다르다"
    );
}

#[test]
fn no_delegated_router_escapes_the_roster() {
    let listed: BTreeSet<(String, String)> = DELEGATED_ROUTERS
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let found: BTreeSet<(String, String)> = delegated_routers().into_iter().collect();
    assert!(
        !found.is_empty(),
        "위임 라우터를 하나도 찾지 못했다. 수집 범위와 검색 조건을 확인한다."
    );
    assert_eq!(
        found,
        listed,
        "`{DELEGATED_PARAM}: &str` 로 분기하지만 명부와 어긋나는 자리가 있다.\n  \
         명부에 없음(새 라우터 — `DELEGATED_ROUTERS` 에 넣어라): {:?}\n  \
         명부에만 있음(사라졌거나 이름이 바뀌었다): {:?}",
        found.difference(&listed).collect::<Vec<_>>(),
        listed.difference(&found).collect::<Vec<_>>(),
    );
}

/// 호출 인자에 request.method가 직접 나타나는지 확인한다. 모든 호출자의 값을 입증하는 검사는 아니다.
/// 지역 변수의 정의를 추적하지 않아 canonical을 받는 should_rate_limit·ipc_call_event와
/// 리터럴을 받는 method_scoped_resource_id는 직접 연결되지 않는다.
/// 직접 연결 수가 0이 아닌지, 연결되지 않은 항목 수가 위 세 항목의 상한 3을 넘지 않는지 확인한다.
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
            "`{name}` 호출을 찾지 못했다. 이름 변경이나 호출 삭제 여부를 확인한다."
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
                panic!("{crel}:{line}의 `{name}` 호출에서 {idx}번 인자를 읽지 못했다")
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
        "호출 인자에서 `{METHOD_EXPR}`를 찾지 못해 IPC 메서드 이름과의 연결을 확인할 수 없다"
    );
    assert!(
        indirect <= 3,
        "request.method에서 직접 연결되지 않는 명부 항목이 {indirect}개로 상한 3을 넘었다. 호출 인자와 지역 변수 경로를 확인한다. 줄어든 것은 실패가 아니다."
    );
}
