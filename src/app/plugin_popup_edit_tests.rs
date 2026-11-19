//! 플러그인 팝업 편집 요청 시험. 회신을 약속한 플러그인의 열린 팝업에만 요청이 생긴다.

use std::sync::Arc;

use super::*;

const OPTED: &str = "com.example.opted";
const PLAIN: &str = "com.example.plain";

fn popup(plugin_id: &str) -> tasty_host_plugin::PopupInstance {
    use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupTrigger};
    tasty_host_plugin::PopupInstance {
        plugin_id: plugin_id.into(),
        popup_id: "file-open".into(),
        contribute: PopupContribute {
            id: "file-open".into(),
            trigger: PopupTrigger::Ipc,
            size_hint: None,
            anchor: crate::plugin::manifest::PopupAnchor::ScreenCenter,
            scope: PopupScopeDecl::Window,
            dismiss_on_outside_click: true,
            rendering: PopupRendering::EguiMesh,
            layout: Default::default(),
        },
        z_seq: 1,
        scope_surface: None,
    }
}

fn mgr() -> PluginManager {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    mgr.attach_edit_outcome_stub_for_test(OPTED);
    mgr.insert_popup_instance_for_test(1, popup(OPTED));
    mgr.insert_popup_instance_for_test(2, popup(PLAIN));
    mgr
}

/// 회신을 약속한 플러그인의 팝업은 그 팝업을 대상으로 요청을 받는다. 약속하지 않은 플러그인(번들 플러그인)의
/// 팝업과 닫힌 팝업은 요청이 없어 원래 키가 넘어간다.
#[test]
fn only_an_open_popup_of_an_opted_in_plugin_gets_an_edit_request() {
    let mut mgr = mgr();
    let request = edit_request(&mut mgr, &ask(1)).expect("약속한 플러그인의 열린 팝업");
    assert_eq!(request.plugin_id, OPTED);
    assert_eq!(request.target, PluginCopyTarget::Popup(1));
    assert!(edit_request(&mut mgr, &ask(2)).is_none());
    assert!(edit_request(&mut mgr, &ask(3)).is_none());
}

fn ask(instance: u64) -> PluginEditAsk {
    PluginEditAsk {
        target: PluginCopyTarget::Popup(instance),
        plugin_id: None,
        edit: EditKey::Paste,
        text: Some("/tmp/a.md".into()),
        original: Some(WindowEvent::Focused(true)),
        awaiting: None,
    }
}

/// 회신을 약속하지 않은 플러그인의 팝업, 닫힌 팝업, 플러그인 관리자가 없는 경우는 원래 키를 넘긴 편집으로 한 번
/// 재생한다. 번들 플러그인이 지금 모두 타는 경로다. 약속한 플러그인의 열린 팝업만 요청을 붙이고 보류한다.
#[test]
fn a_popup_without_a_reply_promise_replays_the_original_key_once() {
    let mut mgr = mgr();
    let replay = vec![ReplayStep::Original(
        WindowEvent::Focused(true),
        EditKey::Paste,
    )];
    for instance in [2, 3] {
        match popup_edit_plan(Some(&mut mgr), ask(instance)) {
            PopupEditPlan::Replay(steps) => assert_eq!(steps, replay, "팝업 {instance}"),
            other => panic!("팝업 {instance} 이 보류됐다: {other:?}"),
        }
    }
    match popup_edit_plan(None, ask(1)) {
        PopupEditPlan::Replay(steps) => assert_eq!(steps, replay),
        other => panic!("관리자 없이 보류됐다: {other:?}"),
    }
    match popup_edit_plan(Some(&mut mgr), ask(1)) {
        PopupEditPlan::Hold(hold) => {
            let PopupEditHold {
                target,
                wire,
                request,
                original,
                edit,
            } = *hold;
            assert_eq!(target, PluginCopyTarget::Popup(1));
            assert_eq!(request.target, PluginCopyTarget::Popup(1));
            assert_eq!(
                wire,
                RawInputEventWire::EditRequest {
                    edit_id: request.edit_id,
                    edit: tasty_plugin_protocol::EditRequestWire::PasteText {
                        text: "/tmp/a.md".into()
                    },
                }
            );
            assert_eq!(
                (original, edit),
                (Some(WindowEvent::Focused(true)), EditKey::Paste)
            );
        }
        other => panic!("약속한 팝업이 재생됐다: {other:?}"),
    }
}

/// 팔레트처럼 키 없이 보낸 편집은 재생할 키가 없다. 약속한 플러그인이면 요청을 붙이되 붙잡을 키가 없고,
/// 요청을 만들 수 없으면 아무것도 재생하지 않는다.
#[test]
fn a_keyless_edit_holds_no_key_and_replays_nothing() {
    let mut mgr = mgr();
    let keyless = |plugin: Option<&str>| PluginEditAsk {
        original: None,
        ..surface_ask(plugin)
    };
    match popup_edit_plan(Some(&mut mgr), keyless(Some(OPTED))) {
        PopupEditPlan::Hold(hold) => {
            assert_eq!(hold.target, PluginCopyTarget::Surface(5));
            assert_eq!(hold.original, None);
        }
        other => panic!("약속한 플러그인에 요청이 없다: {other:?}"),
    }
    match popup_edit_plan(Some(&mut mgr), keyless(None)) {
        PopupEditPlan::Replay(steps) => assert!(steps.is_empty(), "{steps:?}"),
        other => panic!("플러그인을 모르는데 보류됐다: {other:?}"),
    }
}

fn surface_ask(plugin_id: Option<&str>) -> PluginEditAsk {
    PluginEditAsk {
        target: PluginCopyTarget::Surface(5),
        plugin_id: plugin_id.map(str::to_string),
        edit: EditKey::Cut,
        text: None,
        original: Some(WindowEvent::Focused(true)),
        awaiting: None,
    }
}

/// surface 대상은 view 가 키 시점에 읽은 플러그인에 요청을 만든다. 그 플러그인이 회신을 약속했으면 요청이 그
/// surface 입력에 붙고 원래 키를 보류한다. 약속하지 않았거나 플러그인을 모르면 원래 키를 한 번 재생한다.
#[test]
fn a_surface_edit_goes_to_the_plugin_that_view_read_at_key_time() {
    let mut mgr = mgr();
    match popup_edit_plan(Some(&mut mgr), surface_ask(Some(OPTED))) {
        PopupEditPlan::Hold(hold) => {
            assert_eq!(hold.target, PluginCopyTarget::Surface(5));
            assert_eq!(hold.request.target, PluginCopyTarget::Surface(5));
            assert_eq!(hold.request.plugin_id, OPTED);
            assert_eq!(
                hold.wire,
                RawInputEventWire::EditRequest {
                    edit_id: hold.request.edit_id,
                    edit: tasty_plugin_protocol::EditRequestWire::Cut,
                }
            );
        }
        other => panic!("약속한 플러그인의 surface 가 재생됐다: {other:?}"),
    }
    let replay = vec![ReplayStep::Original(
        WindowEvent::Focused(true),
        EditKey::Cut,
    )];
    for plugin_id in [Some(PLAIN), None] {
        match popup_edit_plan(Some(&mut mgr), surface_ask(plugin_id)) {
            PopupEditPlan::Replay(steps) => assert_eq!(steps, replay, "{plugin_id:?}"),
            other => panic!("{plugin_id:?} 이 보류됐다: {other:?}"),
        }
    }
}

/// `egui_paste` surface 의 글자 없는 붙여넣기는 내용 없는 형식 붙여넣기(`paste`) 요청이 된다. 원래 키가 있으면
/// 붙잡고, 팔레트처럼 키가 없으면 키 없이 기록한다.
#[test]
fn a_format_paste_surface_edit_requests_paste_and_holds_or_tracks() {
    let mut mgr = mgr();
    let format_paste = |original: Option<WindowEvent>| PluginEditAsk {
        edit: EditKey::Paste,
        text: None,
        original,
        ..surface_ask(Some(OPTED))
    };
    for original in [Some(WindowEvent::Focused(true)), None] {
        match popup_edit_plan(Some(&mut mgr), format_paste(original.clone())) {
            PopupEditPlan::Hold(hold) => {
                assert_eq!(hold.target, PluginCopyTarget::Surface(5));
                assert_eq!(
                    hold.wire,
                    RawInputEventWire::EditRequest {
                        edit_id: hold.request.edit_id,
                        edit: tasty_plugin_protocol::EditRequestWire::Paste,
                    }
                );
                assert_eq!((hold.original, hold.edit), (original, EditKey::Paste));
            }
            other => panic!("형식 붙여넣기가 재생됐다: {other:?}"),
        }
    }
}

/// 보류 대상 판정: 팝업은 열려 있고 그 창의 키를 받을 때만, surface 는 그 창의 키를 지금도 받을 때만 살아
/// 있다. 닫힌 팝업, 다른 팝업이나 surface 가 키를 받는 창, 플러그인 관리자가 없는 경우는 끝난다.
#[test]
fn a_hold_target_lives_only_while_it_still_owns_the_window_keys() {
    use crate::app::plugin_edit_holds::{WindowKeyTargets, hold_target_live};
    let mgr = mgr();
    let (w1, w2, w3, w4) = (
        WindowId::from(1),
        WindowId::from(2),
        WindowId::from(3),
        WindowId::from(4),
    );
    let key_targets_of = |window: WindowId| match u64::from(window) {
        1 => WindowKeyTargets {
            popup: Some(1),
            surface: None,
        },
        2 => WindowKeyTargets {
            popup: Some(2),
            surface: None,
        },
        3 => WindowKeyTargets {
            popup: Some(3),
            surface: None,
        },
        4 => WindowKeyTargets {
            popup: None,
            surface: Some(5),
        },
        _ => WindowKeyTargets::default(),
    };
    let live = hold_target_live(Some(&mgr), key_targets_of);
    assert!(live(w1, &PluginCopyTarget::Popup(1)), "열림·소유");
    assert!(
        !live(w2, &PluginCopyTarget::Popup(1)),
        "다른 instance 가 키를 받는다"
    );
    assert!(!live(w3, &PluginCopyTarget::Popup(3)), "닫힌 팝업");
    assert!(
        !live(w4, &PluginCopyTarget::Popup(1)),
        "surface 가 키를 받는다"
    );
    assert!(
        live(w4, &PluginCopyTarget::Surface(5)),
        "surface 가 키를 받는다"
    );
    assert!(
        !live(w4, &PluginCopyTarget::Surface(6)),
        "다른 surface 가 키를 받는다"
    );
    assert!(
        !live(w1, &PluginCopyTarget::Surface(5)),
        "팝업이 키를 받는다"
    );
    assert!(
        !live(WindowId::from(9), &PluginCopyTarget::Surface(5)),
        "키를 받는 대상이 없다"
    );
    let without = hold_target_live(None, key_targets_of);
    assert!(
        !without(w1, &PluginCopyTarget::Popup(1)),
        "플러그인 관리자 없음"
    );
    assert!(
        without(w4, &PluginCopyTarget::Surface(5)),
        "surface 는 관리자와 무관"
    );
}

/// App 은 보류 창의 키 대상을 그 창 view 의 판정(`plugin_popup_key_target`)으로 읽어 위 판정에 넘기고, 그 판정을
/// 그대로 `settle`에 쓴다. 이 연결은 `MainView` 없이 시험할 수 없어 소스로 고정한다.
#[test]
fn settle_reads_the_window_key_target_into_the_hold_judgment() {
    let src = include_str!("plugin_edit_holds.rs");
    let body = src
        .split_once("pub(super) fn settle_plugin_edit_holds(")
        .and_then(|(_, rest)| rest.split_once("fn finish_plugin_edit("))
        .map(|(body, _)| body)
        .expect("settle_plugin_edit_holds 본문");
    let code: String = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
        .collect();
    let wiring = "let views = &self.view.views;\
        let engines = &self.engines;\
        let live = hold_target_live(self.plugin_manager.as_ref(), |window| {\
        let main = views.get(&window).and_then(|v| v.as_main());\
        WindowKeyTargets {\
        popup: main.and_then(|main| main.state.plugin_popup_key_target()),\
        surface: main\
        .zip(engines.window_read(window))\
        .and_then(|(main, engine)| main.plugin_surface_key_target(&engine)),\
        }\
        });\
        let awaited = self\
        .plugin_edit_holds\
        .settle_awaits(&mut outcomes, arrivals, now, &live);\
        let (resolved, cancelled) = self.plugin_edit_holds.settle(outcomes, now, live);";
    let wiring: String = wiring.split_whitespace().collect::<Vec<_>>().join(" ");
    let code: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let squash = |s: &str| s.replace(' ', "");
    assert!(
        squash(&code).contains(&squash(&wiring)),
        "settle_plugin_edit_holds 가 창의 키 대상과 열린 팝업 판정을 settle 에 넘기지 않는다"
    );
}

/// 보류 계획을 실행하는 연결도 `App` 없이 시험할 수 없어 소스로 고정한다. 요청을 대상 입력에 붙인 뒤, 원래 키가
/// 있으면 붙잡고 없으면(팔레트) 키 없는 요청으로 기록한다.
#[test]
fn begin_queues_the_request_then_holds_the_key_or_tracks_a_keyless_edit() {
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let src = include_str!("plugin_popup_edit.rs");
    let (code, _) = src.split_once("#[cfg(test)]").expect("시험 모듈 앞의 코드");
    let code = squash(code);
    let wiring = squash(
        "main.queue_plugin_edit_request(target, wire); } let now = Instant::now(); match original {
            Some(original) => self .plugin_edit_holds .hold(id, request, original, edit, now),
            None => self.plugin_edit_holds.track(id, request, edit, now), }",
    );
    assert!(code.contains(&wiring), "보류 계획 실행 연결이 바뀌었다");
}

/// 글자를 읽는 중인 붙여넣기(`awaiting`)는 회신을 약속한 대상이면 요청 id 를 받아 기다린다. 요청은 아직 붙이지
/// 않으며 원래 키·tag·형식 붙여넣기 여부를 그대로 넘긴다. 약속하지 않은 팝업·닫힌 팝업·관리자 없음은 기다리지
/// 않고 원래 키를 한 번 재생한다(그 읽기는 `begin_plugin_field_edit`이 거둔다).
#[test]
fn a_pending_paste_awaits_only_where_a_reply_is_promised() {
    use crate::view::main::plugin_popup_keys::AwaitedPaste;
    let mut mgr = mgr();
    let pending = |ask: PluginEditAsk, tag, format_paste| PluginEditAsk {
        edit: EditKey::Paste,
        text: None,
        awaiting: Some(AwaitedPaste { tag, format_paste }),
        ..ask
    };
    let cases = [
        (
            pending(ask(1), 41, false),
            PluginCopyTarget::Popup(1),
            41,
            false,
        ),
        (
            pending(surface_ask(Some(OPTED)), 42, true),
            PluginCopyTarget::Surface(5),
            42,
            true,
        ),
    ];
    for (ask, target, want_tag, want_format) in cases {
        match popup_edit_plan(Some(&mut mgr), ask) {
            PopupEditPlan::Await(waiting) => {
                let PopupEditAwait {
                    request,
                    tag,
                    original,
                    format_paste,
                } = *waiting;
                assert_eq!(request.target, target);
                assert_eq!(request.plugin_id, OPTED);
                assert_eq!(
                    (tag, original, format_paste),
                    (want_tag, Some(WindowEvent::Focused(true)), want_format)
                );
            }
            other => panic!("{target:?} 가 기다리지 않는다: {other:?}"),
        }
    }
    let replay = vec![ReplayStep::Original(
        WindowEvent::Focused(true),
        EditKey::Paste,
    )];
    for instance in [2, 3] {
        match popup_edit_plan(Some(&mut mgr), pending(ask(instance), 43, false)) {
            PopupEditPlan::Replay(steps) => assert_eq!(steps, replay, "팝업 {instance}"),
            other => panic!("팝업 {instance} 이 기다렸다: {other:?}"),
        }
    }
    match popup_edit_plan(None, pending(ask(1), 44, false)) {
        PopupEditPlan::Replay(steps) => assert_eq!(steps, replay),
        other => panic!("관리자 없이 기다렸다: {other:?}"),
    }
}

/// 계획 실행의 기다림 연결은 `App` 없이 시험할 수 없어 소스로 고정한다. 재생하는 붙여넣기는 시작한 읽기를 거두고,
/// 기다림은 그 창의 보류에 남긴다.
#[test]
fn begin_cancels_the_read_of_a_replayed_paste_and_keeps_an_await() {
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let src = include_str!("plugin_popup_edit.rs");
    let (code, _) = src.split_once("#[cfg(test)]").expect("시험 모듈 앞의 코드");
    let code = squash(code);
    for (wiring, what) in [
        (
            "let read = ask.awaiting.map(|awaiting| awaiting.tag);
             match popup_edit_plan(self.plugin_manager.as_mut(), ask) {
             PopupEditPlan::Replay(steps) => {
             if let Some(tag) = read && let Some(view) = self.view.views.get_mut(&id) {
             view.base_mut().gpu.cancel_plugin_paste(tag); }
             self.replay_edit_steps(id, steps); }",
            "재생하는 붙여넣기가 읽기를 거두지 않는다",
        ),
        (
            "PopupEditPlan::Await(waiting) => { let PopupEditAwait { request, tag, original, format_paste, } = *waiting;
             self.plugin_edit_holds.await_paste( id, request, tag, original, format_paste, Instant::now(), ); }",
            "기다림을 창의 보류에 남기지 않는다",
        ),
    ] {
        assert!(code.contains(&squash(wiring)), "{what}");
    }
}

/// 중단 연결도 `App` 없이 시험할 수 없어 소스로 고정한다. 창 입력은 줄 서기 전에 포인터 누름·포커스 상실로 기다림을
/// 반복해 끝내고(`interrupt_awaits`), App 의 정리는 요청 취소 → 읽기 취소 → 요청 붙이기 → 재생 순서다.
#[test]
fn window_input_interrupts_awaits_and_the_app_applies_them_in_order() {
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let src = squash(include_str!("plugin_edit_holds.rs"));
    let interrupt = squash(
        "pub(super) fn queue_behind_plugin_edit(&mut self, id: WindowId, event: &WindowEvent) -> bool {
         paste_await::interrupt_awaits(self, id, event);
         self.plugin_edit_holds.queue(id, event) }",
    );
    assert!(src.contains(&interrupt), "창 입력이 기다림을 끝내지 않는다");
    let sink = squash(
        "impl paste_await::AwaitSink for App { fn awaits(&mut self) -> &mut PluginEditHolds { &mut self.plugin_edit_holds }
         fn apply_awaited(&mut self, settled: AwaitSettled) { self.apply_await_settled(settled); } }",
    );
    assert!(src.contains(&sink), "중단 정리가 App 정리를 타지 않는다");
    let body = src
        .split_once(&squash(
            "fn apply_await_settled(&mut self, settled: AwaitSettled) {",
        ))
        .and_then(|(_, rest)| rest.split_once(&squash("pub(super) fn settle_plugin_edit_holds(")))
        .map(|(body, _)| body.to_string())
        .expect("apply_await_settled 본문");
    let at = |needle: &str| {
        body.find(&squash(needle))
            .unwrap_or_else(|| panic!("apply_await_settled 에 {needle} 이 없다"))
    };
    let order = [
        at("mgr.cancel_edit_request(edit_id);"),
        at("view.base_mut().gpu.cancel_plugin_paste(tag);"),
        at("main.queue_plugin_edit_request(target, wire);"),
        at("self.finish_plugin_edit(item);"),
    ];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "정리 순서가 바뀌었다: {order:?}"
    );
    let driver = squash(include_str!("plugin_paste_await.rs"));
    assert!(
        driver.contains(&squash(
            "while let Some(ended) = sink.awaits().interrupt_await(window, event) { sink.apply_awaited(ended); }"
        )),
        "재생이 만든 새 기다림을 다시 끝내지 않는다"
    );
}
