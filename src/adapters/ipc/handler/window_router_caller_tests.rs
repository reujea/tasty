//! gui 창 라우터(`route_window_handler`)의 팔마다 **누가 부를 수 있는가**를 명부로 적고 지킨다.
//!
//! 창 라우터에는 창 상태 자체가 대상인 핸들러가 산다 — popup 을 열고 입력 포커스를 옮길 수
//! 있는 자리다. 권한 게이트는 `CallerContext::Local` 을 무조건 통과시키고, 권한 표의
//! `plugin_callable` 은 agent 토큰도 막지 않는다. 그래서 이 문을 지나는 CLI·agent 호출이
//! 사용자 상태에 닿지 않는다는 것(원칙 2.1 ① · 2.3)은 핸들러가 직접 판정해야 하고, 판정을
//! 빠뜨려도 조용하다 — `file_picker.trigger` 가 그랬다
//! (`docs/adr/0504-the-file-picker-trigger-answers-only-a-plugin-caller.md`).
//!
//! 두 시험이 짝이다. 하나는 라우터의 팔과 명부가 같은 집합인지 본다 — 새 팔은 명부에 호출자
//! 정책을 적어야 들어온다(아래 "한계" 의 자리는 제외). 다른 하나는 `PluginOnly` 인 팔을
//! CLI·agent 로 실제로 불러, `-32016` 으로 끝나고 창에 아무것도 남기지 않는지 본다.
//!
//! 첫 시험은 라우터 본문의 `=>` 를 **전부** 팔 머리로 읽고, guard 를 자른 패턴을 `|` 로 나눈
//! 조각 중 **하나라도** 따옴표 이름이 아니면 실패시킨다 — 머리 전체가 guard 없는 `_` 인 팔만
//! 예외다. binding 패턴(`m if m.starts_with(…) =>`) · 상수 경로 · raw 문자열 · 따옴표 이름
//! 안에 `|` · ` if ` 가 든 팔이 그렇게 걸리고, 그 조각이 기존 따옴표 이름 옆에 `|` 로 얹혀도
//! (`"a" | r"b" =>` · `"a" | CONST =>`) 똑같이 걸린다 — 추출기가 못 읽는 조각을 버리면 그
//! 조각이 명부 없이 조용히 들어오기 때문이다. guard 문자열이 `,` 를 담아 머리가 잘린 팔도
//! 잘린 조각이 따옴표 이름이 아니면 걸린다(`!= "x,y"` 는 조각 `y"` 로 실패한다).
//!
//! 그 대가로 팔 머리가 아닌 `=>` 도 머리로 읽혀 실패한다 — 팔 본문 안의 중첩 `match`
//! (`Some(x) =>`) · 매크로 규칙 · `=>` 를 담은 문자열이다. **오늘은 0 곳이다**: 본문의 `=>` 는
//! 팔 둘(`"file_picker.trigger"` · `_`)의 것뿐이다. 재는 법 —
//! `awk '/^fn route_window_handler\(/,/^}/' src/adapters/ipc/handler.rs | grep -c '=>'` 가 팔
//! 수와 같은가. 그런 `=>` 가 생기면 그 모양만 좁게 건너뛰는 예외를 `arm_head` 호출부에 근거와
//! 함께 둔다(명부를 고쳐 통과시키지 않는다).
//!
//! ## 한계 — 이 시험이 잡는 것과 못 잡는 것
//!
//! **아래 목록은 닫혀 있지 않다.** "새 팔은 명부를 거쳐야 들어온다" 가 닿지 않는 자리 중
//! **지금까지 실측·코드 읽기로 확인된 것**만 적었고, 이것이 전부라는 보장은 없다. 이 추출기는
//! Rust 구문을 파싱하지 않고 문자 주사로 근사하므로, 근사가 어긋나는 자리를 앞에서 셀 수 없다.
//! 새 자리를 찾으면 여기에 더하고, 이 시험의 ok 를 "명부 밖 팔이 없다" 의 증명으로 읽지 마라.
//!
//! **①과 ⑤ 의 뿌리**: 팔 머리를 찾는 역주사(`arm_head`)도 본문 끝을 찾는 전진 주사
//! (`window_router_arms`)도 **문자열·char 리터럴 안의 구분자를 모른다**(주석은 가리지만 문자열은
//! 안 가린다). 그래서 문자열이 경계 문자나 `{`·`}` 를 담으면 머리가 잘리거나 **본문이 일찍
//! 끝난다.** 근본 해결은 구문 파싱으로 본문과 팔 머리를 뽑는 것이고, 이 파일은 그것을 하지
//! 않는다. 그 해결도 ②③④ 는 닫지 못한다 — 그 셋은 문자열과 무관하게 시험이 읽는 범위 밖이다.
//!
//! "실수로 명부를 안 거친 팔은 잡는다" 도 좁게 읽어라. 그 말은 아래 자리가 아닌 곳에서만
//! 선다. 각 자리의 "실수로 생기는가" 를 함께 적었고, 실수로 생길 수 있는 자리가 여럿이다.
//! 적대적 작성자를 막는 울타리로도, 실수를 다 잡는 그물로도 읽지 마라.
//!
//! 확인된 자리(2026-09-23 기준. [실측] 은 변이를 넣어 시험이 ok 로 끝나는 것을 본 것,
//! [코드상] 은 코드를 읽은 판단이다):
//! - ① guard 문자열 안의 경계 문자 뒤에 **명부 이름을 따옴표째** 담은 팔 [실측: `,` 로].
//!   `"window.x" if … == r#"a,"file_picker.trigger" if "# =>` 는 머리가 `"file_picker.trigger" if "#`
//!   로 잘려 기존 이름 하나만 나온다. 보통 문자열로는 안쪽 따옴표가 `\"` 가 되어 조각이 실패
//!   쪽으로 가므로 raw 문자열이 필요하다[코드상]. 같은 뿌리의 변형 — 경계 문자가 `{` · `}` 이거나
//!   문자열 안 괄호가 역주사의 깊이를 어긋내는 경우 — 도 같은 모양을 만들 수 있다[코드상, 안 잼].
//!   실수로 생기는가: 어렵다 — 명부 이름을 raw 문자열에 따옴표째 심어야 한다.
//! - ② 라우터 정의가 `#[cfg]` 로 둘 이상일 때 **첫 정의만** 읽는다 [실측]. `src.find` 가 첫
//!   `fn route_window_handler(` 만 잡으므로, 둘째 정의(예: macOS 전용)의 팔은 어느 플랫폼에서도
//!   명부 대조를 받지 않는다. 실수로 생기는가: 생긴다 — 원칙 4 의 `#[cfg]` 분기로 평범하게 생긴다.
//! - ③ `_` 팔의 **본문** [코드상]: 오늘은 `return None` 이지만, 거기서 다른 함수로 넘기면 그
//!   함수가 받는 메서드는 명부를 안 거친다. 실수로 생기는가: 생긴다 — 평범한 위임 코드다.
//! - ④ `match` 밖의 분기 [코드상]: `match` 앞에 `if request.method == "…" { return … }` 를 두면
//!   팔이 아니다. 실수로 생기는가: 생긴다 — 평범한 조기 반환이다.
//! - ⑤ 본문 안 문자열·char 리터럴의 `}` 가 본문 끝으로 읽힌다 [실측: `"closing }}"` 로]. 팔 본문에
//!   `tracing::trace!("closing }}");` 한 줄이 있으면 깊이가 거기서 0 이 되어 본문이 끝나고, **그
//!   뒤의 팔은 전부** 안 읽힌다 — 그 자리 뒤에 명부 없는 팔을 더해도 두 시험이 ok 였다. `'}'` char
//!   리터럴도 같은 경로다[코드상, 안 잼]. 실수로 생기는가: 생긴다 — `}}` 는 format escape 의 평범한
//!   모양이다. (반대로 `"{{"` 는 본문을 다음 함수까지 늘려 실패 쪽으로 간다고 판단했다[코드상, 안 잼].)

use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::ipc::caller::CallerContext;
use crate::ipc::protocol::JsonRpcRequest;

/// 창 라우터 팔의 호출자 정책.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowCallers {
    /// plugin 만 부른다. CLI·agent 호출은 `-32016` 으로 끝나고 창을 건드리지 않는다.
    PluginOnly,
}

/// `(메서드, 정책, 근거)`. 근거를 한 줄로 못 적을 거면 팔을 더하지 마라.
const WINDOW_ROUTER_CALLERS: &[(&str, WindowCallers, &str)] = &[(
    "file_picker.trigger",
    WindowCallers::PluginOnly,
    "고른 경로는 호출한 plugin 에만 push 된다 — CLI·agent 호출은 사용자 포커스만 가져간다(ADR-0504)",
)];

/// `route_window_handler` 본문에서 `"<메서드>" =>` · `"<a>" | "<b>" =>` · `"<메서드>" if … =>` 팔
/// 이름을 전부 뽑는다.
fn window_router_arms() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/ipc/handler.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()));
    let src = tasty_doc_guards::source_text::mask_comments(&src);
    let sig = "fn route_window_handler(";
    let start = src
        .find(sig)
        .unwrap_or_else(|| panic!("`{sig}` 를 찾지 못했다 — 라우터가 옮겨졌으면 이 시험도 옮겨라"));
    let open = start + src[start..].find('{').expect("함수 본문");
    let mut depth = 0usize;
    let mut end = src.len();
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &src[open..end];
    let mut arms = BTreeSet::new();
    let mut unreadable = Vec::new();
    for (i, _) in body.match_indices("=>") {
        let head = arm_head(&body[..i]);
        let (names, bad) = arm_head_names(head);
        // 못 읽은 조각은 버리지 않는다 — 버리면 추출기가 모르는 모양의 팔이, 따옴표 이름 옆에
        // `|` 로 얹힌 것까지, 명부 없이 조용히 들어온다. 통과시켜도 되는 것은 머리 전체가
        // guard 없는 `_` 인 팔 하나뿐이다.
        if head != "_" && (names.is_empty() || !bad.is_empty()) {
            let line = body[..i].rfind('\n').map_or(0, |n| n + 1);
            unreadable.push(format!(
                "머리 `{head}` · 못 읽은 조각 {bad:?} — 그 줄 `{}`",
                body[line..i + 2].trim()
            ));
        }
        arms.extend(names);
    }
    assert!(
        unreadable.is_empty(),
        "창 라우터에서 추출기가 읽지 못한 팔 머리가 있다:\n  {}\n★ 명부를 고치지 말고 \
         추출기(`arm_head` · `arm_head_names`)를 고쳐라 — 못 읽는 팔은 누가 부르는지 \
         판정할 수 없다",
        unreadable.join("\n  ")
    );
    arms
}

/// `=>` 바로 앞까지의 본문(`before`)에서 그 팔 머리를 떼어 낸다.
///
/// 팔 머리는 직전 경계(괄호 밖의 `,` · `{` · `}`)부터 `=>` 까지이고, 속성(`#[...]`)은 걷어낸다.
/// 경계 역주사는 문자열 리터럴을 모른다 — guard 안 문자열이 `,` 를 담으면 머리가 그 뒤
/// 조각으로 잘린다. 잘린 조각이 따옴표 이름이 아니면 호출부가 실패시키지만, 명부에 있는 이름
/// 모양이면 조용히 통과한다(모듈 doc "한계" ①).
fn arm_head(before: &str) -> &str {
    let bytes = before.as_bytes();
    let mut depth = 0usize;
    let mut from = 0;
    for j in (0..bytes.len()).rev() {
        match bytes[j] {
            b')' | b']' => depth += 1,
            b'(' | b'[' => depth = depth.saturating_sub(1),
            b',' | b'{' | b'}' if depth == 0 => {
                from = j + 1;
                break;
            }
            _ => {}
        }
    }
    let mut head = before[from..].trim();
    while let Some(rest) = head.strip_prefix("#[") {
        let mut d = 1usize;
        let mut cut = rest.len();
        for (k, c) in rest.char_indices() {
            match c {
                '[' => d += 1,
                ']' => {
                    d -= 1;
                    if d == 0 {
                        cut = k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        head = rest[cut..].trim_start();
    }
    head
}

/// 팔 머리(`arm_head`)에서 따옴표 이름을 전부 뽑고, 따옴표 이름이 아닌 조각을 따로 돌려준다.
///
/// match guard(` if …`)를 잘라낸 뒤 `|` 로 나눈 조각마다 따옴표 이름이면 이름으로, 아니면
/// 못 읽은 조각으로 넣는다(빈 조각 — 앞머리 `|` — 은 패턴이 없어 뺀다). 닫는 따옴표 바로 뒤의
/// ` =>` 만 팔로 보면 `"a" if … =>` 처럼 guard 가 붙은 팔이 명부 없이 들어오고, 마지막 이름만
/// 보면 기존 팔에 `|` 로 얹은 새 이름이, 못 읽은 조각을 버리면 `"a" | r"b"` 의 `r"b"` 가 명부
/// 없이 들어온다.
fn arm_head_names(head: &str) -> (Vec<String>, Vec<String>) {
    // guard 는 공백 뒤의 `if` 낱말부터다(rustfmt 가 guard 를 다음 줄로 내려도 같다).
    let guard = head.match_indices("if").find(|(k, _)| {
        head[..*k].ends_with(char::is_whitespace)
            && head[k + 2..].starts_with(|c: char| c.is_whitespace() || c == '(')
    });
    let pattern = guard.map_or(head, |(k, _)| &head[..k]);
    let mut names = Vec::new();
    let mut bad = Vec::new();
    for alt in pattern.split('|').map(str::trim).filter(|a| !a.is_empty()) {
        match alt.strip_prefix('"').and_then(|a| a.strip_suffix('"')) {
            Some(name) => names.push(name.to_string()),
            None => bad.push(alt.to_string()),
        }
    }
    (names, bad)
}

#[test]
fn every_window_router_arm_declares_who_may_call_it() {
    let arms = window_router_arms();
    assert!(
        !arms.is_empty(),
        "창 라우터에서 팔을 하나도 못 찾았다 — 추출기가 깨졌다"
    );
    let declared: BTreeSet<String> = WINDOW_ROUTER_CALLERS
        .iter()
        .map(|(m, _, _)| (*m).to_string())
        .collect();
    let undeclared: Vec<&String> = arms.difference(&declared).collect();
    let stale: Vec<&String> = declared.difference(&arms).collect();
    assert!(
        undeclared.is_empty() && stale.is_empty(),
        "창 라우터 팔과 호출자 명부가 다르다.\n  명부에 없는 팔: {undeclared:?}\n  \
         팔이 없는 명부 항목: {stale:?}\n★ 창 라우터 팔은 사용자 상태(popup·포커스)에 닿는다. \
         새 팔이면 누가 불러도 되는지 정해 `WINDOW_ROUTER_CALLERS` 에 근거와 함께 적고, \
         CLI·agent 가 사용자 상태를 바꾸지 않게 핸들러에서 판정하라"
    );
}

fn request(method: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params: json!({}),
        session_token: None,
    }
}

#[test]
fn a_plugin_only_window_arm_refuses_cli_and_agent_without_touching_the_window() {
    let agent = CallerContext::Agent {
        agent_id: "child:1".into(),
        permissions: Arc::new(Default::default()),
    };
    let mut checked = 0usize;
    for (method, policy, _) in WINDOW_ROUTER_CALLERS {
        if *policy != WindowCallers::PluginOnly {
            continue;
        }
        for caller in [CallerContext::Local, agent.clone()] {
            let (mut state, mut engine) = crate::state::tests::test_state();
            let active_before = state.active_workspace;
            let resp = super::route_window_handler(
                &mut state,
                &mut engine,
                &caller,
                &request(method),
                json!(1),
            )
            .unwrap_or_else(|| panic!("`{method}` 이 창 라우터에서 라우팅되지 않았다"));
            let err = resp
                .error
                .unwrap_or_else(|| panic!("`{method}` 을 {caller:?} 가 불렀는데 성공했다"));
            assert_eq!(err.code, -32016, "`{method}` · {caller:?}");
            assert!(
                state.take_pending_intents().is_empty(),
                "`{method}` 을 {caller:?} 가 불렀는데 창 큐에 intent 가 남았다"
            );
            assert!(
                !state.popups.has_focused(),
                "`{method}` 을 {caller:?} 가 불렀는데 포커스를 가진 popup 이 있다"
            );
            assert_eq!(
                state.active_workspace, active_before,
                "`{method}` · {caller:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "PluginOnly 팔을 하나도 재지 않았다");
}
