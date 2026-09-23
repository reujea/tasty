//! 이름 표의 멱등 키 선언(`method_meta::KeyContract`)이 **실제 배선과 맞는가.**
//!
//! 선언은 대부분 `MethodEffect` 에서 유도되고(`Mutate` → 판 1 의 `Kept`), 손으로 적는 것은 판이
//! 다른 이름 무리뿐이다 — App 층이 끝내는 `Mutate`(판 2), 그리고 그 뒤의 GUI debug step 이 끝내는
//! `Mutate` 와 plugin namespace forward 로 나가는 표 이름(판 3). 그 무리가 틀리면 client 가 거짓을
//! 믿는다: 판을 낮게 적으면 그 경로가 키를 무시하던 서버에 키를 실어 재시도가 두 번째 실행이 된다.
//!
//! 그래서 이 가드는 두 가지를 잰다.
//!
//! **선언이 dispatch 본문이 부르는 이름과 맞는가** — 양방향으로.
//!
//! - GUI app_methods step(`ipc_step_app_methods` 본문)이 부르는 `Mutate` 의 집합
//!   = 표에서 판 [`KEY_KEPT_BY_APP_LAYER`] 로 선언된 이름의 집합.
//! - GUI debug step(`debug_methods.rs` · `window_required.rs`)이 부르는 `Mutate` 의 집합
//!   = debug 표에서 판 [`KEY_KEPT_ON_EVERY_HOST_PATH`] 로 선언된 이름의 집합. forward 로 나가는
//!   표 이름 쪽 대조는 예약 prefix 목록에서 유도하므로 `tasty-ipc` 의 `method_meta` 시험이 한다.
//! - 헤드리스 App 층 가로채기가 부르는 `Mutate` 는 판 1 로 선언돼 있지 않다 — 그 가로채기도 첫
//!   줄에서 보존소를 지나지만(`run_app_layer`), 판 1 서버에는 그 층의 보존소가 없었다.
//!
//! **보존소를 지나는 자리가 실제로 그 경로 앞에 있는가** — App 층 뒤의 두 경로에 대해.
//!
//! - GUI dispatch 는 debug step · window-required step 을 직접 안 부르고 한 묶음
//!   (`ipc_step_debug_layers`)으로 부르며, 그 묶음은 두 step 보다 먼저 `run_app_layer` 를 부른다.
//! - GUI 라우터와 헤드리스의 namespace forward 는 `forward_namespace_call` 을
//!   `forward_keeping_the_key` 의 **인자 안에서만** 부른다.
//! - 세 자리(두 forward · debug 묶음)가 보존소 함수에 넘기는 relay closure 는 **자기 인자만**
//!   쓴다 — 보존소에 넘긴 바깥 명령 바인딩을 본문에서 쓰지 않는다. 쓰면 키를 뗀 사본과 relay
//!   통로가 버려지고 답이 원 통로로 나가 결말이 기록되지 않는다.
//! - debug 묶음의 `handled` 판정 closure 는 자기 인자를 `IpcStep::Handled` 와 견준다 — "늘
//!   맡았다" 가 되면 안 맡은 이름의 자리를 맡은 것으로 닫는다.
//!
//! closure 쪽 두 판정은 주석·문자열을 덮은 사본(`mask_non_code`)에서 식별자 경계로 한다.
//!
//! 뒤쪽이 텍스트로 재는 것은 "부르는 자리의 모양" 까지다 — 그 함수가 옳게 판정하는지는
//! `idempotency` 모듈의 시험이 행동으로 잰다. 둘이 합쳐서 "그 경로의 재시도가 한 번만 실행된다"
//! 가 된다(ADR-0566).
//!
//! 이름 추출은 옆 가드(`headless_app_layer_coverage`)의 것을 그대로 쓴다 — 같은 본문에서
//! 같은 물음(그 층이 무엇을 부르는가)을 두 방식으로 답하면 갈린다. 끝이 `.` 인 리터럴은
//! prefix 판정이라 표의 이름으로 펼친다(`plugin.` → `plugin.install` 등).
//!
//! **텍스트로 읽는 한계**는 옆 가드와 같다 — 리터럴이 아닌 이름은 안 보인다. 그 사각을 이
//! 가드가 따로 좁히지 않는 이유는 옆 가드가 이미 "명부 밖에 이름이 사는가" 를 잰다는 것이다.

use std::collections::BTreeSet;

use tasty_ipc::method_meta::{
    DEBUG_METHODS, KEY_KEPT_BY_APP_LAYER, KEY_KEPT_ON_EVERY_HOST_PATH, KeyContract, METHOD_TABLE,
    MethodEffect, MethodMeta,
};

use super::headless_app_layer_coverage::{headless_dispatch_code, method_literals};
use super::{fn_body, mask_non_code, repo_root, strip_comments, word_positions};

const GUI_DISPATCH: &str = "src/app/ipc.rs";
const GUI_ROUTING: &str = "src/app/ipc/routing.rs";
const HEADLESS_DISPATCH: &str = "src/boot/headless_dispatch.rs";
const GUI_APP_STEP: &str = "src/app/ipc/app_methods.rs";
const GUI_APP_FN: &str = "fn ipc_step_app_methods";
/// GUI debug step 의 두 파일. 파일 전체가 그 step 이다(모듈 doc 이 그렇게 적는다).
const GUI_DEBUG_STEPS: &[&str] = &[
    "src/app/ipc/debug_methods.rs",
    "src/app/ipc/window_required.rs",
];

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

/// 시험 모듈을 뺀 본문 — 시험 안의 리터럴은 dispatch 가 아니다.
fn shipped(src: &str) -> &str {
    src.split("\n#[cfg(test)]").next().unwrap_or(src)
}

fn table() -> impl Iterator<Item = &'static (&'static str, MethodMeta)> {
    METHOD_TABLE.iter().chain(DEBUG_METHODS)
}

/// 리터럴 집합을 표의 `Mutate` 이름 집합으로 — prefix 리터럴은 펼친다.
fn mutating(literals: &BTreeSet<String>) -> BTreeSet<&'static str> {
    table()
        .filter(|(_, m)| m.effect == MethodEffect::Mutate)
        .map(|(name, _)| *name)
        .filter(|name| {
            literals.iter().any(|lit| {
                if lit.ends_with('.') {
                    name.starts_with(lit.as_str())
                } else {
                    lit == name
                }
            })
        })
        .collect()
}

fn declared(pred: impl Fn(KeyContract) -> bool) -> BTreeSet<&'static str> {
    table()
        .filter(|(_, m)| pred(m.key_contract))
        .map(|(name, _)| *name)
        .collect()
}

fn kept_by_app_layer(c: KeyContract) -> bool {
    c == KeyContract::Kept {
        since: KEY_KEPT_BY_APP_LAYER,
    }
}

fn kept_on_every_host_path(c: KeyContract) -> bool {
    c == KeyContract::Kept {
        since: KEY_KEPT_ON_EVERY_HOST_PATH,
    }
}

fn gui_app_step_mutations() -> BTreeSet<&'static str> {
    let src = read(GUI_APP_STEP);
    let body = fn_body(&src, GUI_APP_FN)
        .unwrap_or_else(|| panic!("{GUI_APP_STEP} 에서 `{GUI_APP_FN}` 본문을 못 잘랐다"));
    mutating(&method_literals(&strip_comments(&body)))
}

fn gui_debug_step_mutations() -> BTreeSet<&'static str> {
    let mut lits = BTreeSet::new();
    for rel in GUI_DEBUG_STEPS {
        let src = read(rel);
        lits.extend(method_literals(&strip_comments(shipped(&src))));
    }
    mutating(&lits)
}

/// GUI app_methods step 이 끝내는 `Mutate` 는 전부 판 2 이고, 판 2 는 그것뿐이다.
#[test]
fn the_app_layer_mutations_are_exactly_the_names_kept_from_version_two() {
    let answered = gui_app_step_mutations();
    // 좌변이 비면 아래 대조가 "둘 다 빈 집합" 으로 초록이 된다.
    assert!(
        answered.len() >= 6,
        "app_methods step 에서 Mutate 를 {} 개만 읽었다 — 추출이 망가졌다: {answered:?}",
        answered.len()
    );
    let declared = declared(kept_by_app_layer);
    assert_eq!(
        answered, declared,
        "App 층이 끝내는 Mutate(왼쪽)와 판 {KEY_KEPT_BY_APP_LAYER} 선언(오른쪽)이 다르다. \
         App 층에 Mutate 를 더했으면 표에서 `.kept_by_app_layer()` 를 붙이고, engine \
         라우터로 옮겼으면 뗀다"
    );
}

/// GUI debug step 이 끝내는 `Mutate` 는 전부 판 3 이고, debug 표의 판 3 은 그것뿐이다.
/// release 빌드에서는 debug 표가 비어 양쪽이 다 빈 집합이다.
#[test]
fn the_debug_step_mutations_are_exactly_the_debug_names_kept_from_version_three() {
    let answered = gui_debug_step_mutations();
    if cfg!(debug_assertions) {
        assert!(
            !answered.is_empty(),
            "debug step 에서 Mutate 를 하나도 못 읽었다 — 추출이 망가졌다"
        );
    }
    let declared: BTreeSet<&'static str> = DEBUG_METHODS
        .iter()
        .filter(|(_, m)| kept_on_every_host_path(m.key_contract))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        answered, declared,
        "GUI debug step 이 끝내는 Mutate(왼쪽)와 debug 표의 판 {KEY_KEPT_ON_EVERY_HOST_PATH} \
         선언(오른쪽)이 다르다. debug step 에 Mutate 를 더했으면 표에서 \
         `.kept_on_every_host_path()` 를 붙인다"
    );
}

/// 헤드리스 App 층 가로채기가 끝내는 `Mutate` 는 판 1 로 선언돼 있지 않다.
#[test]
fn a_headless_app_layer_mutation_is_never_declared_kept_from_version_one() {
    let answered = mutating(&method_literals(&headless_dispatch_code()));
    assert!(
        answered.contains("plugin.request_permission"),
        "헤드리스 가로채기에서 plugin.request_permission 을 못 읽었다 — 추출이 망가졌다: {answered:?}"
    );
    for name in answered {
        let contract = table()
            .find(|(n, _)| *n == name)
            .map(|(_, m)| m.key_contract)
            .expect("mutating() 은 표의 이름만 낸다");
        assert!(
            kept_by_app_layer(contract) || kept_on_every_host_path(contract),
            "{name}: 헤드리스 App 층이 끝내는데 {contract:?} 로 선언됐다"
        );
    }
}

/// `open` 바로 뒤의 `(` 에서 시작하는 호출의 인자 목록 — 괄호 균형으로 자른다.
fn call_args<'a>(src: &'a str, call: &str) -> Option<&'a str> {
    let at = src.find(call)? + call.len();
    let mut depth = 1usize;
    for (i, c) in src[at..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[at..at + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// GUI dispatch 가 debug step · window-required step 을 보존소를 연 **뒤에** 부른다.
///
/// 이 모양이 깨지는 형태는 둘이다 — dispatch 가 두 step 을 묶음 밖에서 직접 부르거나(그 호출은
/// app_methods step 이 닫은 뒤라 보존소를 안 지난다), 묶음이 보존소보다 step 을 먼저 부른다.
#[test]
fn the_gui_debug_steps_run_behind_the_store() {
    let src = strip_comments(shipped(&read(GUI_DISPATCH)));
    let dispatch = fn_body(&src, "fn ipc_dispatch_command")
        .unwrap_or_else(|| panic!("{GUI_DISPATCH} 에서 `ipc_dispatch_command` 를 못 잘랐다"));
    assert!(
        dispatch.contains("ipc_step_debug_layers("),
        "dispatch 가 debug 묶음을 안 부른다"
    );
    for step in ["ipc_step_debug(", "ipc_step_window_required("] {
        assert!(
            !dispatch.contains(step),
            "dispatch 가 `{step}` 를 묶음 밖에서 부른다 — app_methods step 이 연 자리를 닫은 뒤라 \
             보존소를 안 지난다"
        );
    }
    let layers = fn_body(&src, "fn ipc_step_debug_layers")
        .unwrap_or_else(|| panic!("{GUI_DISPATCH} 에서 `ipc_step_debug_layers` 를 못 잘랐다"));
    let store = layers
        .find("run_app_layer(")
        .expect("debug 묶음이 보존소(`run_app_layer`)를 안 부른다");
    for step in ["ipc_step_debug(", "ipc_step_window_required("] {
        let at = layers
            .find(step)
            .unwrap_or_else(|| panic!("debug 묶음이 `{step}` 를 안 부른다"));
        assert!(
            store < at,
            "debug 묶음이 보존소보다 `{step}` 를 먼저 부른다"
        );
    }
}

/// 두 조합의 namespace forward 가 `forward_namespace_call` 을 보존소 함수의 **인자 안에서만**
/// 부른다 — 밖에서 부르면 표가 `Kept` 로 선언한 이름(`image.open` 등)의 재시도가 plugin 으로 두
/// 번 나간다.
#[test]
fn both_namespace_forwards_go_through_the_store() {
    for rel in [GUI_ROUTING, HEADLESS_DISPATCH] {
        let src = strip_comments(shipped(&read(rel)));
        assert_eq!(
            src.matches("forward_namespace_call(").count(),
            1,
            "{rel}: forward 자리가 하나가 아니다"
        );
        let args = call_args(&src, "forward_keeping_the_key(")
            .unwrap_or_else(|| panic!("{rel}: `forward_keeping_the_key` 를 안 부른다"));
        assert!(
            args.contains("forward_namespace_call("),
            "{rel}: `forward_namespace_call` 이 보존소 함수 밖에 있다"
        );
    }
}

/// 호출 인자 목록을 **최상위** `,` 로 가른다 — 괄호 안의 `,` 는 그 인자의 것이다.
fn top_level_args(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut from = 0usize;
    for (i, c) in args.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&args[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    if !args[from..].trim().is_empty() {
        out.push(&args[from..]);
    }
    out
}

/// `|p| 본문` 모양의 closure 인자를 (매개변수 이름, 본문) 으로. 타입 표기(`p: &T`)는 뗀다.
fn closure_parts(arg: &str) -> Option<(&str, &str)> {
    let s = arg.trim();
    let s = s.strip_prefix("move").map_or(s, str::trim_start);
    let s = s.strip_prefix('|')?;
    let end = s.find('|')?;
    let param = s[..end].split(':').next()?.trim();
    Some((param, &s[end + 1..]))
}

/// 보존소 함수에 넘기는 closure 하나를 재는 자리.
struct StoreCall {
    rel: &'static str,
    /// 이 함수 본문 안에서 찾는다. `None` 이면 파일 전체.
    scope_fn: Option<&'static str>,
    call: &'static str,
}

/// 세 자리 — 두 조합의 forward 와 GUI 의 debug 묶음. 셋 다 `(caller, 명령, …, closure)` 모양이다.
const STORE_CALLS: &[StoreCall] = &[
    StoreCall {
        rel: GUI_ROUTING,
        scope_fn: None,
        call: "forward_keeping_the_key(",
    },
    StoreCall {
        rel: HEADLESS_DISPATCH,
        scope_fn: None,
        call: "forward_keeping_the_key(",
    },
    StoreCall {
        rel: GUI_DISPATCH,
        scope_fn: Some("fn ipc_step_debug_layers"),
        call: "run_app_layer(",
    },
];

/// 주석·문자열을 덮은 사본에서 그 호출의 최상위 인자들.
fn store_call_args(site: &StoreCall) -> Vec<String> {
    let masked = mask_non_code(shipped(&read(site.rel)));
    let scope = match site.scope_fn {
        Some(sig) => fn_body(&masked, sig)
            .unwrap_or_else(|| panic!("{}: `{sig}` 본문을 못 잘랐다", site.rel)),
        None => masked,
    };
    let args = call_args(&scope, site.call)
        .unwrap_or_else(|| panic!("{}: `{}` 를 안 부른다", site.rel, site.call));
    top_level_args(args)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// 보존소 함수에 넘기는 relay closure 가 **자기 인자만** 쓴다 — 바깥의 명령 바인딩을 쓰지 않는다.
///
/// 보존소는 처음 보는 키면 **키를 뗀 사본**에 relay 통로를 달아 closure 에 넘긴다. closure 가
/// 그 인자 대신 바깥 명령(보존소에 넘긴 둘째 인자)을 쓰면 두 형태로 깨진다 — 원 통로로 답이
/// 나가 relay 가 결말을 기록하지 못하거나(재시도가 두 번째 실행이 된다), 키를 단 원 명령이
/// 같은 층으로 돌아와 자기 진행 중 자리에 합류한다. 둘 다 컴파일되고 단위 시험은 closure 를
/// 시험이 직접 만들므로 못 본다 — 그래서 호출 자리의 모양을 잰다.
///
/// 판정은 주석·문자열을 덮은 사본(`mask_non_code`)에서 식별자 경계로 한다 — 주석이 바깥
/// 바인딩을 설명하려고 인용해도 세지 않는다.
#[test]
fn the_relay_closure_uses_only_its_own_argument() {
    for site in STORE_CALLS {
        let args = store_call_args(site);
        assert!(
            args.len() >= 3,
            "{}: `{}` 의 인자를 {} 개만 읽었다 — 추출이 망가졌다: {args:?}",
            site.rel,
            site.call,
            args.len()
        );
        let outer = args[1].trim();
        assert!(
            !outer.is_empty() && outer.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "{}: 보존소에 넘기는 명령이 바인딩 하나가 아니다(`{outer}`) — 이 가드의 전제가 깨졌다",
            site.rel
        );
        let last = args.last().expect("길이를 위에서 쟀다");
        let (param, body) = closure_parts(last).unwrap_or_else(|| {
            panic!(
                "{}: 마지막 인자가 `|인자| 본문` closure 가 아니다: `{}`",
                site.rel,
                last.trim()
            )
        });
        assert!(
            !param.starts_with('_') && !word_positions(body, param).is_empty(),
            "{}: relay closure 가 자기 인자 `{param}` 를 안 쓴다 — 보존소가 넘긴 키를 뗀 사본과 \
             relay 통로가 버려진다",
            site.rel
        );
        assert!(
            word_positions(body, outer).is_empty(),
            "{}: relay closure 가 바깥 명령 `{outer}` 를 쓴다 — `{param}` 를 써야 한다(원 통로로 \
             답이 나가면 결말이 기록되지 않는다)",
            site.rel
        );
    }
}

/// GUI debug 묶음의 `handled` 판정이 **`IpcStep::Handled` 를 실제로 본다.**
///
/// 이 판정이 "늘 맡았다" 가 되면, debug step 이 안 맡은 이름도 연 자리를 맡은 것으로 닫아
/// 다음 층(라우터 등)이 같은 키를 또 판정한다 — 한 요청이 실행 수를 두 번 올린다(ADR-0422).
/// 판정 closure 가 자기 인자를 쓰고 그 인자를 `IpcStep::Handled` 와 견주는지 모양으로 잰다.
#[test]
fn the_debug_layers_judge_handled_by_the_step() {
    let site = STORE_CALLS
        .iter()
        .find(|s| s.rel == GUI_DISPATCH)
        .expect("위 표에 debug 묶음이 있다");
    let args = store_call_args(site);
    // `run_app_layer(caller, cmd, answered, handled, dispatch)` — 넷째가 판정이다.
    assert_eq!(
        args.len(),
        5,
        "{}: `run_app_layer` 인자가 다섯이 아니다 — 시그니처가 바뀌었으면 이 가드를 같이 고친다: {args:?}",
        site.rel
    );
    let (param, body) = closure_parts(&args[3]).unwrap_or_else(|| {
        panic!(
            "{}: 판정 인자가 closure 가 아니다: `{}`",
            site.rel,
            args[3].trim()
        )
    });
    assert!(
        !param.starts_with('_') && !word_positions(body, param).is_empty(),
        "{}: 판정 closure 가 자기 인자 `{param}` 를 안 본다 — 어느 step 결과든 같은 답이 된다",
        site.rel
    );
    // 경로 사이 공백(`IpcStep :: Handled`)도 같은 것이라 공백을 걷고 식별자 경계로 찾는다 —
    // `IpcStep::HandledDirty` 는 다른 갈래다.
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    let handled = !word_positions(&compact, "IpcStep::Handled").is_empty();
    assert!(
        handled,
        "{}: 판정 closure 가 `IpcStep::Handled` 를 안 본다: `{}`",
        site.rel,
        body.trim()
    );
}
