//! GUI의 app/debug 라우터에 있는 이름이 헤드리스에도 있거나 미지원 사유로 등록됐는지 확인한다.
//! 창·렌더러가 필요한 메서드는 두 조합이 같은 응답을 제공할 수 없어 이름별 사유를 유지한다.
//!
//! 등록한 함수 본문에서 주석을 제거한 뒤 메서드 형태의 리터럴을 수집한다. 실제 응답 동작을
//! 실행하지 않으므로 이름 존재가 성공 응답을 보장하지는 않는다. plugin 읽기 분기는 별도 토큰으로 대조한다.
//! 리터럴이 아닌 이름의 누락은 dispatch_name_literals에서 검사한다.
//!
//! 헤드리스는 test 전용 구간을 제외한 파일 전체에서도 이름을 수집해 함수 명부 밖의 항목을 찾는다.
//! 구조화 로그처럼 응답과 무관한 이름 리터럴은 근거를 남겨 제외한다. GUI 파일의 명부 밖 전체를
//! 같은 방식으로 검사하지는 않는다. 따라서 양쪽 라우터의 모든 경로를 대조하는 검사는 아니다.

use std::collections::BTreeSet;

use super::{callers_of, fn_body, repo_root, strip_comments};
use tasty_doc_guards::cfg_predicate::blank_gated_lines;

const GUI_STEP: &str = "src/app/ipc/app_methods.rs";
const GUI_FN: &str = "fn ipc_step_app_methods";
const HEADLESS_PUMP: &str = "src/boot/headless_dispatch.rs";
const GUI_DEBUG_STEP: &str = "src/app/ipc/debug_methods.rs";
const GUI_DEBUG_FN: &str = "fn ipc_step_debug";

/// 2026-09-10 GUI app 함수에서 메서드·prefix 리터럴 18개를 측정했다. 문서의 메서드 표 개수와는 다르다.
const MIN_GUI_METHODS: usize = 12;

/// 헤드리스에서 이름을 수집하는 함수들. pump_ipc도 다시 분기가 추가될 수 있어 포함하고 모든 함수의 존재를 확인한다.
const HEADLESS_DISPATCH_FNS: &[&str] = &[
    "fn pump_ipc(",
    "fn intercept_app_layer(",
    "fn intercept_debug_app_layer(",
];

/// 2026-09-10 등록된 헤드리스 함수들의 합집합에서 이름 12개를 측정했다. GUI 쪽과 비슷한 비율로 여유를 둔다.
const MIN_HEADLESS_METHODS: usize = 8;

/// 창·GPU 등이 필요해 헤드리스에서 지원하지 않는 메서드의 개별 근거.
const NOT_IN_HEADLESS: &[(&str, &str)] = &[
    (
        "remote.attach",
        "mirror workspace 를 띄울 창이 필요하다 — winit proxy 로 창 생성 이벤트를 보낸다",
    ),
    (
        "attach.into_gui",
        "mirror 를 띄우는 attach 큐 소비자가 GUI 에만 있다. 헤드리스는 접수하지 않고 -32017 로 거절한다",
    ),
    (
        "remote.refusals",
        "거절을 기록하는 remote.attach 와 자동 attach 가 GUI 에만 있다",
    ),
    (
        "remote.disconnect",
        "떼어 낼 mirror 를 만드는 remote.attach 와 attach 세션이 GUI 에만 있다",
    ),
    (
        "system.gpu_stats",
        "창마다의 GpuState 와 wgpu 전역 리포트를 센다. 헤드리스엔 GPU 컨텍스트가 없다",
    ),
    (
        "ui.screenshot",
        "창 표면을 읽어 파일로 쓴다. 그릴 창이 없으면 하는 일 자체가 없다",
    ),
    ("view.close", "`window.close` 의 다른 이름 — 같은 사유"),
    ("view.create", "`window.create` 의 다른 이름 — 같은 사유"),
    ("view.focus", "`window.focus` 의 다른 이름 — 같은 사유"),
    ("view.list", "`window.list` 의 다른 이름 — 같은 사유"),
    (
        "window.close",
        "`App.view.views` 에서 창을 닫는다. 헤드리스엔 그 레지스트리가 없다",
    ),
    (
        "window.create",
        "winit 이벤트루프에 창 생성을 맡긴다. 헤드리스엔 이벤트루프가 없다",
    ),
    (
        "window.focus",
        "포커스 전환이라 애초에 debug 격리(docs/dev-guide/plugin-permissions.md)이고, 대상도 창이다",
    ),
    (
        "window.list",
        "창 목록과 포커스 상태는 GUI의 App.view에서 관리한다. 헤드리스에는 이 상태가 없어 빈 GUI 창 목록으로 응답하지 않는다.",
    ),
];

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

pub(super) fn method_literals(body: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find('"') {
        let after = &rest[at + 1..];
        let Some(end) = after.find('"') else { break };
        let lit = &after[..end];
        let dotted = lit.split('.').count() >= 2;
        // starts_with로 비교하는 prefix도 처리 목록에 포함해야 한다.
        let shaped = !lit.is_empty()
            && lit
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_');
        if dotted && shaped {
            out.insert(lit.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

/// GUI의 plugin. 분기와 헤드리스의 읽기 전용 판정을 연결한다. 토큰 존재만 확인하며 실제 호출 의미를 해석하지 않는다.
const HEADLESS_COVERS: &[(&str, &str)] = &[("plugin.", "is_readonly_method")];

fn gui_methods() -> BTreeSet<String> {
    let src = read(GUI_STEP);
    let body = fn_body(&src, GUI_FN)
        .unwrap_or_else(|| panic!("{GUI_STEP} 에서 `{GUI_FN}` 본문을 못 잘랐다"));
    method_literals(&body)
}

/// 이름과 대조 토큰 모두 같은 함수 본문에서 찾도록 주석을 제거해 합친다.
pub(super) fn headless_dispatch_code() -> String {
    let src = read(HEADLESS_PUMP);
    let mut out = String::new();
    for sig in HEADLESS_DISPATCH_FNS {
        let body = fn_body(&src, sig).unwrap_or_else(|| {
            panic!(
                "{HEADLESS_PUMP}에서 `{sig}` 본문을 읽지 못했다. 이름·이동 여부와 HEADLESS_DISPATCH_FNS를 확인한다."
            )
        });
        out.push_str(&strip_comments(&body));
        out.push('\n');
    }
    out
}

fn headless_methods() -> BTreeSet<String> {
    method_literals(&headless_dispatch_code())
}

/// 등록 함수 밖에서 응답과 무관하게 쓰는 이름 리터럴과 그 근거.
/// 로그 설명 문장의 일부인 이름은 수집되지 않는다. test 전용 코드도 대상에서 제외되므로 등록할 필요가 없다.
/// 실제 처리 함수라면 예외 대신 HEADLESS_DISPATCH_FNS에 등록한다.
const OUTSIDE_ROSTER_LITERALS: &[(&str, &str)] = &[];

fn outside_roster<'a>(
    whole: &'a BTreeSet<String>,
    roster: &BTreeSet<String>,
    excused: &BTreeSet<&str>,
) -> Vec<&'a String> {
    whole
        .iter()
        .filter(|m| !roster.contains(*m) && !excused.contains(m.as_str()))
        .collect()
}

/// 등록 함수 밖에 추가된 이름도 확인한다. 합성 시험의 리터럴은 제품 코드가 아니므로 먼저 test 구간을 제외한다.
#[test]
fn no_method_name_lives_outside_the_roster() {
    let src = read(HEADLESS_PUMP);
    let whole = method_literals(&strip_comments(&blank_gated_lines(&src, "test")));
    assert!(
        whole.len() >= MIN_HEADLESS_METHODS,
        "{HEADLESS_PUMP}에서 이름을 {}개만 수집했다(하한 {MIN_HEADLESS_METHODS}). 수집 범위를 확인한다.",
        whole.len()
    );
    let roster = headless_methods();
    let excused: BTreeSet<&str> = OUTSIDE_ROSTER_LITERALS.iter().map(|(m, _)| *m).collect();
    let outside = outside_roster(&whole, &roster, &excused);
    assert!(
        outside.is_empty(),
        "{HEADLESS_PUMP}의 등록 함수 밖에 메서드 이름이 있다: {outside:?}. 실제 처리 함수라면 HEADLESS_DISPATCH_FNS에 추가한다. 구조화 로그 등 응답과 무관한 이름 리터럴이면 OUTSIDE_ROSTER_LITERALS에 근거를 적는다. 설명 문장의 일부인 이름은 이 수집 대상이 아니다."
    );
    let stale: Vec<&str> = OUTSIDE_ROSTER_LITERALS
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| roster.contains(*m))
        .collect();
    assert!(
        stale.is_empty(),
        "응답과 무관하다고 등록한 이름이 처리 함수 본문에도 있다. OUTSIDE_ROSTER_LITERALS의 오래된 항목을 제거한다: {stale:?}"
    );
}

/// 실제 검사와 같은 순서로 test 구간과 주석을 제거한 뒤 미등록 이름·예외를 확인한다.
#[test]
fn an_outside_name_is_caught_unless_it_is_excused() {
    let src = "\
fn pump_ipc(app: &mut App) {
    if m == \"ns.inside\" { go(); }
}
// 산문이 \"ns.prose\" 를 인용한다.
fn outside_helper(m: &str) -> bool { m == \"ns.outside\" }
#[cfg(test)]
mod fixture {
    const SAMPLE: &str = \"ns.fixture\";
}
";
    let whole = method_literals(&strip_comments(&blank_gated_lines(src, "test")));
    let roster = method_literals(&strip_comments(
        &fn_body(src, "fn pump_ipc(").expect("본문을 잘라야 한다"),
    ));
    assert!(
        !whole.contains("ns.prose"),
        "주석 속 이름을 잔여 후보로 셌다: {whole:?}"
    );
    assert!(
        !whole.contains("ns.fixture"),
        "test 전용 구간의 이름을 수집했다: {whole:?}"
    );
    let none: BTreeSet<&str> = BTreeSet::new();
    let caught: Vec<&str> = outside_roster(&whole, &roster, &none)
        .into_iter()
        .map(String::as_str)
        .collect();
    assert_eq!(
        caught,
        vec!["ns.outside"],
        "등록 함수 밖의 이름을 검출하지 못했다"
    );
    let excused: BTreeSet<&str> = ["ns.outside"].into_iter().collect();
    assert!(
        outside_roster(&whole, &roster, &excused).is_empty(),
        "OUTSIDE_ROSTER_LITERALS에 등록한 이름이 제외되지 않았다"
    );
}

#[test]
fn every_gui_app_layer_method_is_answered_headless_or_carries_a_reason() {
    let gui = gui_methods();
    assert!(
        gui.len() >= MIN_GUI_METHODS,
        "GUI app 함수에서 이름을 {}개만 수집했다(하한 {MIN_GUI_METHODS}, 2026-09-10 측정 18개). 함수 이름과 추출기를 확인한다.",
        gui.len()
    );
    let headless = headless_methods();
    assert!(
        headless.len() >= MIN_HEADLESS_METHODS,
        "헤드리스 dispatch 에서 메서드를 {} 개밖에 못 뽑았다(하한 {MIN_HEADLESS_METHODS}, \
         2026-09-10 실측 12)",
        headless.len()
    );

    let excused: BTreeSet<&str> = NOT_IN_HEADLESS.iter().map(|(m, _)| *m).collect();
    let code = headless_dispatch_code();
    let covered_by_token = |m: &str| {
        HEADLESS_COVERS
            .iter()
            .any(|(item, token)| *item == m && code.contains(token))
    };
    let missing: Vec<&String> = gui
        .iter()
        .filter(|m| !headless.contains(*m) && !excused.contains(m.as_str()) && !covered_by_token(m))
        .collect();
    assert!(
        missing.is_empty(),
        "GUI app 메서드 중 헤드리스의 처리 목록과 미지원 사유에 없는 이름이다. 헤드리스 dispatch에서 처리하거나 NOT_IN_HEADLESS에 구체적인 사유를 등록한다: {missing:?}"
    );

    let stale: Vec<&str> = NOT_IN_HEADLESS
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| headless.contains(*m))
        .collect();
    assert!(
        stale.is_empty(),
        "헤드리스 처리 목록에도 있는 이름이 NOT_IN_HEADLESS에 남아 있다. 사유가 낡았는지 확인한다: {stale:?}"
    );
    let orphan: Vec<&str> = NOT_IN_HEADLESS
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| !gui.contains(*m))
        .collect();
    assert!(
        orphan.is_empty(),
        "gui app 층 step 이 더 이상 부르지 않는 이름이 사유 목록에 남아 있다: {orphan:?}"
    );
}

/// 사유를 빈칸이나 동일한 문장으로 채우지 않도록 길이와 중복을 확인한다.
#[test]
fn each_reason_says_something_and_says_it_once() {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (method, reason) in NOT_IN_HEADLESS {
        assert!(
            reason.len() >= 10,
            "`{method}` 의 사유가 너무 짧아 아무것도 말하지 않는다"
        );
        assert!(
            seen.insert(reason),
            "`{method}`가 다른 항목과 같은 사유를 쓴다. 각 메서드에 필요한 상태나 제약을 구체적으로 적는다."
        );
    }
}

#[test]
fn the_cut_stops_at_the_dispatch_function() {
    let src = "\
fn before() { let m = \"ns.before\"; }
fn pump_ipc(app: &mut App) {
    if m == \"ns.inside\" { go(); }
}
fn after() { let m = \"ns.after\"; }
";
    let body = fn_body(src, "fn pump_ipc").expect("본문을 잘라야 한다");
    let found = method_literals(&body);
    assert!(found.contains("ns.inside"));
    assert!(
        !found.contains("ns.before") && !found.contains("ns.after"),
        "본문 밖 리터럴을 집었다: {found:?}"
    );
}

#[test]
fn a_comment_is_not_an_answer() {
    let src = "\
fn pump_ipc(app: &mut App) {
    // 헤드리스는 \"ui.screenshot\" 을 답하지 않는다 — 창이 없다.
    if m == \"ns.real\" { go(); }
}
fn tail_helper() { let doc = \"remote.attach\"; }
";
    let body = fn_body(src, "fn pump_ipc(").expect("본문을 잘라야 한다");
    let found = method_literals(&strip_comments(&body));
    assert!(found.contains("ns.real"), "답하는 이름을 잃었다: {found:?}");
    assert!(
        !found.contains("ui.screenshot"),
        "주석 속 이름을 답으로 셌다: {found:?}"
    );
    assert!(
        !found.contains("remote.attach"),
        "명부 밖 헬퍼의 이름을 셌다: {found:?}"
    );
    assert!(
        method_literals(&body).contains("ui.screenshot"),
        "합성 원문에 주석의 이름이 없어 주석 제거 전후를 비교할 수 없다"
    );
}

#[test]
fn the_roster_names_real_functions() {
    let src = read(HEADLESS_PUMP);
    for sig in HEADLESS_DISPATCH_FNS {
        assert!(
            fn_body(&src, sig).is_some(),
            "`{sig}` 를 `{HEADLESS_PUMP}` 에서 못 찾았다 — 이름이 바뀌었으면 \
             `HEADLESS_DISPATCH_FNS` 를 함께 고쳐라"
        );
    }
}

/// GUI debug 메서드가 헤드리스에서 지원되지 않는 이유. 창·렌더러·입력 큐 및 정리 경로를 개별 확인한다.
const DEBUG_NOT_IN_HEADLESS: &[(&str, &str)] = &[
    (
        "debug.settings.open",
        "설정 모달을 연다. `AppEvent::OpenSettings` 를 winit proxy 로 보내는데 헤드리스엔 \
         그 proxy 가 없다",
    ),
    (
        "debug.settings.close",
        "설정 모달을 닫는다. 활성 모달을 `self.view.active_modal()` 로 찾고 \
         `close_active_modal()` 이 `self.view.views` 에서 지운다 — view 가 없다",
    ),
    (
        "debug.popup.open",
        "popup 생성은 매니저만 사용하지만 닫기 처리는 GUI dispatch에 있다. 생성만 허용하면 헤드리스에서 닫을 수 없는 인스턴스가 남는다.",
    ),
    (
        "debug.popup.close",
        "렌더의 close 큐를 통해 자식 파일 선택기까지 정리해야 한다(docs/design/systems/popup.md). enqueue_plugin_popup_close는 GUI dispatch에 있다.",
    ),
    (
        "debug.plugin_banner.",
        "소유 view 의 BannerManager 와 host 매니저를 함께 다룬다 — `open`·`close` 둘 다 \
         `self.view.views` 를 순회한다. view 가 없다",
    ),
    (
        "debug.modal.close_request",
        "활성 모달은 `self.view.active_modal_id()` 로 식별하고 `close_active_modal()` 이 \
         `self.view.views` 에서 지운다 — view 가 없다",
    ),
    (
        "debug.window.list",
        "메인 창과 보조 창을 `self.view.views` 에서 나열한다 — view 가 없다",
    ),
    (
        "debug.remote_download.",
        "다운로드는 attach client 세션(`self.remote.sessions`)의 연결로 오가고 받은 파일은 창의 \
         `MainView` 에서 연다. 헤드리스엔 attach client 도 view 도 없다",
    ),
    (
        "debug.toast",
        "토스트 스택은 창마다 따로 있다 — 메인 창은 `MainViewState.toasts`, 설정·preset·plugins 창은 \
         각 view 의 `toasts` 를 `self.view.views` 에서 고른다. view 가 없다",
    ),
    (
        "debug.inject_egui_text",
        "`window_id` 를 준 문자 주입은 `self.view.views` 에서 그 창의 egui 입력을 고른다. \
         헤드리스엔 egui 창이 없다",
    ),
    (
        "debug.inject_egui_mouse",
        "`window_id` 를 준 포인터 주입은 `self.view.views` 에서 그 창의 egui 입력을 고른다. \
         헤드리스엔 egui 창이 없다",
    ),
    (
        "debug.inject_egui_key",
        "`window_id` 를 준 키 주입은 `self.view.views` 에서 그 창의 egui 입력을 고른다. \
         헤드리스엔 egui 창이 없다",
    ),
    // fullscreen.list는 창 없이 제공할 수 있지만 open/close/state는 창이 필요해 각각 기록한다.
    (
        "debug.fullscreen.open",
        "무대는 창 단위다 — `pick_debug_window` 로 `self.view.views` 에서 창을 지목한다",
    ),
    (
        "debug.fullscreen.close",
        "무대는 창 단위다 — `pick_debug_window` 로 `self.view.views` 에서 창을 지목한다",
    ),
    (
        "debug.fullscreen.state",
        "무대는 창 단위다 — `pick_debug_window` 로 `self.view.views` 에서 창을 지목한다",
    ),
];

#[test]
fn every_gui_debug_step_method_is_answered_headless_or_carries_a_reason() {
    let src = read(GUI_DEBUG_STEP);
    let body = fn_body(&src, GUI_DEBUG_FN)
        .unwrap_or_else(|| panic!("{GUI_DEBUG_STEP} 에서 `{GUI_DEBUG_FN}` 본문을 못 잘랐다"));
    let gui = method_literals(&body);
    assert!(
        gui.len() >= MIN_GUI_DEBUG_ITEMS,
        "GUI debug 함수에서 이름을 {}개만 수집했다(하한 {MIN_GUI_DEBUG_ITEMS}, 2026-09-10 측정 13개). 추출 범위를 확인한다.",
        gui.len()
    );
    let headless = headless_methods();

    let excused: BTreeSet<&str> = DEBUG_NOT_IN_HEADLESS.iter().map(|(m, _)| *m).collect();
    let covered = |item: &str| {
        headless.contains(item)
            || headless
                .iter()
                .any(|h| h.ends_with('.') && item.starts_with(h.as_str()))
            || excused.contains(item)
            || excused
                .iter()
                .any(|e| e.ends_with('.') && item.starts_with(*e))
    };
    // prefix 분기는 파일의 위임 함수에 있는 개별 이름까지 모아 모두 처리되거나 사유가 있는지 확인한다.
    let in_file = method_literals(&src);
    let concrete_under = |p: &str| -> Vec<String> {
        gui.iter()
            .chain(in_file.iter())
            .filter(|m| m.as_str() != p && m.starts_with(p) && !m.ends_with('.'))
            .cloned()
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect()
    };
    let missing: Vec<&String> = gui
        .iter()
        .filter(|m| {
            if covered(m) {
                return false;
            }
            if m.ends_with('.') {
                let under = concrete_under(m);
                return under.is_empty() || !under.iter().all(|c| covered(c.as_str()));
            }
            true
        })
        .collect();
    assert!(
        missing.is_empty(),
        "GUI debug 메서드 중 헤드리스의 처리 목록과 미지원 사유에 없는 이름이다. 헤드리스 dispatch에서 처리하거나 DEBUG_NOT_IN_HEADLESS에 필요한 상태·제약을 적는다: {missing:?}"
    );

    // prefix 전체가 미지원이라는 사유는 그 아래 이름 하나만 지원해도 다시 나눠야 한다.
    let stale: Vec<&str> = DEBUG_NOT_IN_HEADLESS
        .iter()
        .map(|(m, _)| *m)
        .filter(|m| {
            headless.contains(*m)
                || (m.ends_with('.') && headless.iter().any(|h| h.starts_with(*m)))
        })
        .collect();
    assert!(
        stale.is_empty(),
        "헤드리스 처리 목록에 있는 이름이 미지원 사유에도 남아 있다. prefix의 일부만 지원한다면 사유를 이름별로 나눈다: {stale:?}"
    );

    for (method, reason) in DEBUG_NOT_IN_HEADLESS {
        assert!(
            reason.len() >= 10,
            "`{method}` 의 사유가 너무 짧아 아무것도 말하지 않는다"
        );
    }
}

/// 2026-09-10 GUI debug 함수에서 이름·prefix 리터럴 13개를 측정했다.
const MIN_GUI_DEBUG_ITEMS: usize = 5;

#[test]
fn a_prefix_literal_is_kept() {
    let src = "\
fn pump_ipc(app: &mut App) {
    if m.starts_with(\"ns.family.\") { go(); }
    if m == \"ns.one\" { go(); }
}
";
    let body = fn_body(src, "fn pump_ipc").expect("본문을 잘라야 한다");
    let found = method_literals(&body);
    assert!(found.contains("ns.family."), "prefix 를 버렸다: {found:?}");
    assert!(found.contains("ns.one"));
}

/// 다른 코드 형태로 같은 분기를 처리한다는 근거 토큰이 사라지면 대응도 해제해야 한다.
#[test]
fn a_cover_claim_dies_with_its_evidence() {
    let pump = headless_dispatch_code();
    for (item, token) in HEADLESS_COVERS {
        assert!(
            pump.contains(token),
            "`{item}` 을 헤드리스가 답한다고 적혀 있는데 그 근거인 `{token}` 이 \
             `{HEADLESS_PUMP}` 에 없다. 처리 경로가 삭제됐다면 이 줄도 지우고, 사유가 \
             필요하면 `NOT_IN_HEADLESS` 로 옮겨라"
        );
        let without = pump.replace(token, "");
        assert!(
            !without.contains(token),
            "대조 토큰이 제거되지 않아 토큰 부재를 검증할 수 없다"
        );
    }
}

const GUARD_DIRS: &[&str] = &["src/source_guards/", "crates/tasty-doc-guards/"];

/// 공용 dispatch 호출을 라우터 파일별로 확인한다. 어느 한쪽에 호출이 남았다는 사실만으로 양쪽의 공유를 보장할 수 없다.
const SHARED_BY_BOTH_ROUTERS: &[&str] = &["dispatch_lifecycle_toggle"];

#[test]
fn a_shared_dispatch_is_called_by_both_routers() {
    for name in SHARED_BY_BOTH_ROUTERS {
        let sites = callers_of(name, GUARD_DIRS);
        assert!(
            !sites.is_empty(),
            "`{name}` 호출을 찾지 못했다. 이름 변경과 명부를 확인한다."
        );
        let files: BTreeSet<&str> = sites.iter().map(|s| s.rel.as_str()).collect();
        for rel in [GUI_STEP, HEADLESS_PUMP] {
            assert!(
                files.contains(rel),
                "`{name}` 호출이 `{rel}`에 없다. 해당 라우터도 공용 처리를 사용하는지 확인한다. 발견한 호출 파일: {files:?}"
            );
        }
    }
}
