//! `file_handler.dispatch` **입구**가 발화 주체를 새 탭의 선택까지 그대로 나르는지 고정한다.
//!
//! 새 탭 핸들러의 판정(사용자면 선택, 아니면 그대로)은 `intent::headless` 시험이 origin 을 직접
//! 주입해 잰다. 그 시험은 입구가 무엇을 싣는지 안 본다 — 입구가 사용자 popup 을 못 알아보거나
//! 외부 호출자의 주장을 믿으면 그 시험은 초록인 채로 사용자 상태가 움직인다. 그래서 여기서는
//! 요청을 `handle_dispatch` 에 넣고, 그 intent 를 identify 완료 적용과 새 탭 핸들러까지 실제로
//! 흘린 뒤 사용자가 보던 탭을 본다(ADR-0526).

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use tasty_ipc::caller::CallerContext;

use super::handle_dispatch;
use crate::core::intent::DomainIntent;
use crate::file::dispatch::FileDispatchOrigin;
use crate::file::format::DetectorId;
use crate::intent::Intent;

const PLUGIN: &str = "com.example.popup";
const POPUP: u64 = 7;

fn plugin_caller(plugin_id: &str) -> CallerContext {
    CallerContext::Plugin {
        plugin_id: plugin_id.to_string(),
        permissions: Arc::new(HashSet::new()),
    }
}

/// `params` 로 `file_handler.dispatch` 를 부르고, 그 결과를 identify 완료 적용 → 새 탭 핸들러까지
/// 흘린다. 돌려주는 값은 입구가 정한 발화 주체 · 발화 intent 가 사용자인가 · 그 뒤의 상태다.
///
/// `activated` 가 `Some((plugin, instance))` 면 그 popup 이 사용자의 확정형 입력을 받은 상태로
/// 시작한다. `with_origin_surface` 면 focused pane 의 surface 를 `origin_surface_id` 로 더 싣는다 —
/// 그때 새 탭은 `Intent::NewTab` 이 아니라 명시 origin 갈래(`open_surface_tab` 의 `Some(pane)`)로 선다.
/// `mirror` 면 그 워크스페이스를 mirror 로 두고 시작한다 — 새 탭은 원격으로 forward 된다.
fn dispatch_through(
    caller: &CallerContext,
    activated: Option<(&str, u64)>,
    mut params: serde_json::Value,
    with_origin_surface: bool,
    mirror: bool,
) -> (
    FileDispatchOrigin,
    bool,
    crate::state::AppState,
    crate::core::CoreState,
    u32,
) {
    use tasty_plugin_protocol::host_port::FileHandlerRegistryPort;
    let mut core = crate::adapters::ipc::handler::cli_entry_tests::test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    FileHandlerRegistryPort::install_plugin_handlers(
        engine.file_handler.as_ref(),
        PLUGIN,
        &[json!({"id":"open", "detector":"origin-test", "priority":0,
            "action":{"kind":"open_surface", "surface_kind":"empty", "param_key":"file"}})],
    );
    if let Some((plugin, instance)) = activated {
        state
            .plugin_popup_user_activated
            .insert(instance, plugin.to_string());
    }
    let pane_id = state.active_workspace(&engine).focused_pane;
    assert_eq!(engine.find_pane_by_id(pane_id).expect("pane").active_tab, 0);
    if with_origin_surface {
        let sid = engine.workspaces[0].all_surface_ids()[0];
        assert_eq!(engine.find_pane_for_surface(sid), Some(pane_id));
        params["origin_surface_id"] = json!(sid);
    }
    engine.workspaces[0].mirror = mirror;

    let mut out = crate::ipc::window_port::IntentOutbox::default();
    let resp = handle_dispatch(&mut out, &state, &engine, caller, json!(1), params);
    assert!(resp.error.is_none(), "dispatch must be accepted: {resp:?}");
    let mut emitted = out.into_vec();
    assert_eq!(emitted.len(), 1);
    let dispatched = emitted.remove(0);
    let intent_is_user = dispatched.origin.is_user();
    let Intent::Domain(DomainIntent::DispatchFile {
        target,
        origin_surface_id,
        dispatch_origin,
        ignore_size_limit,
        ..
    }) = dispatched.body
    else {
        panic!("file_handler.dispatch 가 DispatchFile 외 intent 를 냈다");
    };

    // `App::handle_identify_done` 이 워커 결과로 부르는 적용 — detector 는 위 핸들러의 것이다.
    crate::file::dispatch::apply_identify_result(
        &mut core,
        &mut state,
        &mut engine,
        target,
        Some(DetectorId::new("origin-test")),
        origin_surface_id,
        dispatch_origin,
        ignore_size_limit,
    );
    for pending in state.take_pending_intents() {
        assert!(matches!(pending.body, Intent::NewTab { .. }));
        crate::intent::tab::handle(&mut core, &mut state, &mut engine, &pending);
    }
    (dispatch_origin, intent_is_user, state, engine, pane_id)
}

/// [`dispatch_through`] 를 로컬 워크스페이스에서 돌리고 focused pane 의 `(탭 수, 활성 탭)` 을 본다.
fn dispatch_then_selection(
    caller: &CallerContext,
    activated: Option<(&str, u64)>,
    params: serde_json::Value,
    with_origin_surface: bool,
) -> (FileDispatchOrigin, bool, (usize, usize)) {
    let (origin, intent_is_user, _state, engine, pane_id) =
        dispatch_through(caller, activated, params, with_origin_surface, false);
    let pane = engine.find_pane_by_id(pane_id).expect("pane");
    (origin, intent_is_user, (pane.tabs.len(), pane.active_tab))
}

fn popup_params() -> serde_json::Value {
    json!({ "path": "/tmp/a.md", "owner_popup_instance": POPUP })
}

#[test]
fn a_dispatch_from_the_users_popup_selects_the_new_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::User, true, (2, 1)));
}

#[test]
fn a_dispatch_without_a_popup_keeps_the_users_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        json!({ "path": "/tmp/a.md" }),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::Agent, false, (2, 0)));
}

/// 외부 IPC 호출자는 같은 키를 실어도 사용자가 될 수 없다 — 요청 값만으로는 모자란다.
#[test]
fn an_external_caller_cannot_claim_a_popup() {
    let got = dispatch_then_selection(
        &CallerContext::Local,
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::Agent, false, (2, 0)));
}

/// 다른 plugin 의 popup 을 대도 사용자가 아니다.
#[test]
fn a_plugin_cannot_claim_another_plugins_popup() {
    let got = dispatch_then_selection(
        &plugin_caller("com.example.other"),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
    );
    assert_eq!(got, (FileDispatchOrigin::Agent, false, (2, 0)));
}

/// 사용자가 만지지 않은(확정형 입력을 안 받은 · 닫혀 걷힌) popup 은 근거가 못 된다.
#[test]
fn an_untouched_popup_is_not_a_user_action() {
    let got = dispatch_then_selection(&plugin_caller(PLUGIN), None, popup_params(), false);
    assert_eq!(got, (FileDispatchOrigin::Agent, false, (2, 0)));
}

/// 사용자 popup 이 열 pane 을 `origin_surface_id` 로 함께 대도 사용자다 — 발화 주체는 그 키가
/// 아니라 popup 판정이 정하므로, 명시 origin 갈래에서도 새 탭이 선택된다.
#[test]
fn a_users_popup_that_names_an_origin_surface_still_selects_the_new_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        popup_params(),
        true,
    );
    assert_eq!(got, (FileDispatchOrigin::User, true, (2, 1)));
}

/// `origin_surface_id` 만으로는 사용자가 아니다 — popup 근거 없이 pane 을 대면 에이전트다.
#[test]
fn an_origin_surface_without_a_popup_keeps_the_users_tab() {
    let got = dispatch_then_selection(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        json!({ "path": "/tmp/a.md" }),
        true,
    );
    assert_eq!(got, (FileDispatchOrigin::Agent, false, (2, 0)));
}

/// mirror 워크스페이스에서 사용자 popup 으로 연 새 탭은 원격으로 forward 되고, 그 op 는 원격 거절이
/// **toast 로 가도록**(표시 없음) 남는다. 같은 요청이 popup 근거 없이 오면 로그로 가도록 표시된다
/// (ADR-0503 의 `silent_failure`). 입구가 사용자 popup 을 못 알아보면 앞쪽 단언이 깨진다.
#[test]
fn a_mirror_tab_from_the_users_popup_keeps_its_remote_failure_toast() {
    let (origin, _, state, engine, _) = dispatch_through(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        popup_params(),
        false,
        true,
    );
    assert_eq!(origin, FileDispatchOrigin::User);
    assert_eq!(
        engine.pending_structural_forward.len(),
        1,
        "새 탭은 forward 된다"
    );
    assert!(
        !engine.pending_structural_forward[0].silent_failure,
        "사용자 발화 op 의 원격 거절은 toast 로 간다"
    );
    assert_eq!(state.toasts.len(), 0);

    let (origin, _, _, engine, _) = dispatch_through(
        &plugin_caller(PLUGIN),
        Some((PLUGIN, POPUP)),
        json!({ "path": "/tmp/a.md" }),
        false,
        true,
    );
    assert_eq!(origin, FileDispatchOrigin::Agent);
    assert_eq!(engine.pending_structural_forward.len(), 1);
    assert!(
        engine.pending_structural_forward[0].silent_failure,
        "에이전트 발화 op 의 원격 거절은 로그로 간다"
    );
}
