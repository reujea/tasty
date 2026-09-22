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
//! 첫 시험은 `handler.rs` 의 `fn route_window_handler` 정의를 **전부** 읽는다(`#[cfg]` 로 갈린
//! 둘째 정의도). 정의마다 본문이 `Some(match request.method.as_str() { … })` 한 식뿐이고 그
//! `match` 의 `_` 팔이 `return None` 인지 먼저 본다 — `match` 앞의 조기 반환이나 `_` 팔 본문의
//! 위임은 팔이 아니라서 명부를 안 거치기 때문이다. 그다음 팔을 앞에서부터 하나씩 떼어, guard 를
//! 뺀 패턴을 `|` 로 나눈 조각 중 **하나라도** 보통 문자열 리터럴이 아니면 실패시킨다 — binding
//! 패턴(`m if m.starts_with(…) =>`) · 상수 경로 · raw 문자열이 그렇게 걸리고, 그 조각이 기존
//! 따옴표 이름 옆에 `|` 로 얹혀도(`"a" | r"b" =>`) 똑같이 걸린다. 못 읽는 조각을 버리면 그
//! 조각이 명부 없이 조용히 들어오기 때문이다.
//!
//! 팔을 떼는 판정기는 `tasty_doc_guards::match_arms` 다. 구조(괄호 짝 · `=>` · `,` · `|` ·
//! ` if `)는 주석·문자열·문자 리터럴을 덮은 사본에서 찾고 이름은 같은 구간의 원본에서 읽으므로,
//! 리터럴 **안의** 구분자는 구조로 읽히지 않는다. 팔 본문 안의 중첩 `match` · 클로저의 `=>` 는
//! 본문에 들어가 팔 머리로 읽히지 않는다. 판정기가 모르는 모양(블록형 식 뒤 쉼표를 생략해 다음
//! 팔로 이어지는 본문)은 조용히 넘기지 않고 이 시험을 실패시킨다.
//!
//! ## 한계 — 이 시험이 잡는 것과 못 잡는 것
//!
//! **아래 목록은 닫혀 있지 않다.** "새 팔은 명부를 거쳐야 들어온다" 가 닿지 않는 자리 중
//! **지금까지 실측·코드 읽기로 확인된 것**만 적었고, 이것이 전부라는 보장은 없다. 판정기는
//! 렉서 위의 구조 주사이지 Rust 문법 파서가 아니다 — 리터럴과 주석은 가르지만 식의 문법은
//! 모른다. 새 자리를 찾으면 여기에 더하고, 이 시험의 ok 를 "명부 밖 팔이 없다" 의 증명으로
//! 읽지 마라.
//!
//! 적대적 작성자를 막는 울타리로도, 실수를 다 잡는 그물로도 읽지 마라. 아래 자리가 아닌
//! 곳에서 "명부를 안 거친 팔은 실패한다" 가 선다.
//!
//! **이 시험이 이제 실패로 답하는 모양**(2026-09-23, 각각 변이를 `handler.rs` 에 넣어 FAILED 를
//! 봤다): guard 의 raw 문자열이 경계 `,` 와 명부 이름을 따옴표째 담은 팔 · 앞 팔 본문의
//! `"closing }}"` 뒤에 둔 팔 · 앞 팔 본문의 `'}'` 뒤에 둔 팔 · `#[cfg]` 로 꺼 둔 둘째
//! `fn route_window_handler` 의 팔 · `_` 팔 본문의 `if request.method == "…"` 분기 · `match`
//! 앞의 `if request.method == "…" { return … }`.
//!
//! 확인된 자리([실측] 은 변이를 넣어 시험이 ok 로 끝나는 것을 본 것, [코드상] 은 코드를 읽은
//! 판단이다):
//! - 이 시험은 **`handler.rs` 의 `route_window_handler` 라는 이름만** 읽는다 [코드상]. 창 상태에
//!   닿는 다른 문 — 다른 이름·다른 파일의 라우터나 `EntryWindow` 에 새로 연 메서드 — 는 명부
//!   밖이다. 실수로 생기는가: 생긴다 — 라우터를 나누거나 옮기는 평범한 리팩터로 생긴다(옮기면
//!   이 시험은 "찾지 못했다" 로 실패하지만, 하나를 **더** 만들면 조용하다).
//! - 명부에 적힌 팔의 **본문 안** 분기 [코드상]: 본문이 params 를 보고 다른 핸들러로 가르면, 둘째
//!   시험은 빈 params(`{}`) 로만 불러 그 갈래의 호출자 판정을 안 잰다. 팔의 이름은 명부를 거쳤으니
//!   "명부 밖 팔" 은 아니지만, 명부의 정책이 그 팔의 모든 갈래에 서는지는 이 시험이 모른다.
//!   실수로 생기는가: 생긴다 — 평범한 params 분기다.
//! - 렉서의 근사 [코드상]: 문자 리터럴과 라이프타임 틱은 `'` 바로 뒤가 `\` 이거나 두 칸 뒤가
//!   `'` 인가로 가른다(`tasty_doc_guards::source_text`). 이 규칙이 Rust 어휘와 갈리는 자리는 아직 확인된 것이
//!   없지만, 갈리면 그 뒤의 덮기가 어긋나 구조가 틀리게 읽힌다. 실수로 생기는가: 모른다.

use serde_json::json;
use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use tasty_doc_guards::match_arms::{Source, matching_close};

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

/// `route_window_handler` 정의 **전부**에서 팔 이름을 뽑는다.
///
/// 정의마다 본문이 `Some(match request.method.as_str() { … })` 한 식뿐인지, 그 `match` 의
/// `_` 팔이 `return None` 인지 먼저 본다 — 그 밖의 자리(`match` 앞의 조기 반환, `_` 팔
/// 본문의 위임)는 팔이 아니라서 명부를 안 거친다. 모양이 다르면 명부가 아니라 이 시험을
/// 실패시킨다.
fn window_router_arms() -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/ipc/handler.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했다: {e}", path.display()));
    let src = Source::new(&text);
    let bodies = src.fn_bodies("route_window_handler");
    assert!(
        !bodies.is_empty(),
        "`fn route_window_handler` 를 찾지 못했다 — 라우터가 옮겨졌으면 이 시험도 옮겨라"
    );
    let mut arms = BTreeSet::new();
    let mut unreadable = Vec::new();
    for body in bodies {
        let line = src.line_of(body.start);
        let block = match router_match_block(&src, &body) {
            Ok(block) => block,
            Err(e) => {
                unreadable.push(format!("{line}행 정의: {e}"));
                continue;
            }
        };
        let parsed = match src.match_arms(block) {
            Ok(parsed) => parsed,
            Err(e) => {
                unreadable.push(format!("{line}행 정의: {e}"));
                continue;
            }
        };
        for arm in parsed {
            let pattern = src.slice(&arm.pattern);
            if pattern == "_" && arm.guard.is_none() {
                // `_` 팔의 본문이 다른 함수로 넘기면 그 함수가 받는 메서드는 명부를 안
                // 거친다 — 팔이 아닌 채로 라우팅된다.
                let body = src
                    .code_slice(&arm.body)
                    .split_whitespace()
                    .collect::<String>();
                if body != "returnNone" {
                    unreadable.push(format!(
                        "{}행: `_` 팔의 본문이 `return None` 이 아니다 — `{}`",
                        src.line_of(arm.body.start),
                        src.slice(&arm.body)
                    ));
                }
                continue;
            }
            // 못 읽은 조각은 버리지 않는다 — 버리면 추출기가 모르는 모양의 팔이, 따옴표
            // 이름 옆에 `|` 로 얹힌 것까지, 명부 없이 조용히 들어온다.
            let mut bad = Vec::new();
            for alt in src.alternatives(&arm.pattern) {
                match src.plain_string(&alt) {
                    Some(name) => {
                        arms.insert(name.to_string());
                    }
                    None => bad.push(src.slice(&alt).to_string()),
                }
            }
            if !bad.is_empty() {
                unreadable.push(format!(
                    "{}행: 팔 `{pattern}` · 따옴표 이름이 아닌 조각 {bad:?}",
                    src.line_of(arm.pattern.start)
                ));
            }
        }
    }
    assert!(
        unreadable.is_empty(),
        "창 라우터에서 이 시험이 읽지 못한 자리가 있다:\n  {}\n★ 명부를 고치지 말고 \
         라우터를 명부가 읽을 수 있는 모양으로 두어라 — 못 읽는 팔은 누가 부르는지 \
         판정할 수 없다",
        unreadable.join("\n  ")
    );
    arms
}

/// 라우터 본문(`{ … }`)이 `Some(match request.method.as_str() { … })` 한 식뿐이면 그
/// `match` 블록 구간을 돌려준다.
fn router_match_block(src: &Source, body: &Range<usize>) -> Result<Range<usize>, String> {
    const HEAD: &str = "Some(match request.method.as_str()";
    let inner = src.trim(body.start + 1..body.end - 1);
    let code = src.code_slice(&inner);
    let squeezed: String = code.split_whitespace().collect();
    let expected = format!("{}{{", HEAD.split_whitespace().collect::<String>());
    if !squeezed.starts_with(&expected) {
        return Err(format!(
            "본문이 `{HEAD} {{ … }})` 로 시작하지 않는다 — `match` 앞의 분기는 팔이 아니라 \
             명부를 안 거친다"
        ));
    }
    let open = inner.start
        + code
            .find('{')
            .ok_or_else(|| "`match` 블록을 못 찾았다".to_string())?;
    let close = matching_close(&src.code, open).ok_or("`match` 블록이 닫히지 않는다")?;
    let tail: String = src.code[close + 1..inner.end].split_whitespace().collect();
    if tail != ")" {
        return Err(format!(
            "`match` 뒤에 `)` 말고 `{tail}` 가 있다 — 본문은 그 식 하나여야 한다"
        ));
    }
    Ok(open..close + 1)
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
