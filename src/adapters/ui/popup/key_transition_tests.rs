//! 키 사건이 포커스 노드를 옮기는 전이(검색 단축키) 뒤 같은 묶음의 키. 전이는 그 키 사건 처리 안에서 적용되고, 뒤
//! 키는 사건 시점의 노드(검색창)로 끊긴다. 노드 차이로 끊은 자리에는 새 노드의 빈 패스가 먼저 서서, 검색창 입력칸이
//! 그 패스에서 egui 키 포커스를 얻고 다음 패스에서 뒤 키를 받는다(`PopupManager::cut_key_frame`).
//!
//! 입력은 창과 같은 끊기·게이트(`WindowInput`), 패스는 egui 패스와 같은 분할(`WindowInput::take_pass`)로 밟는다. 단축키
//! 하나는 창처럼 끊기(`key_focus`) → 단축키 처리(`search_bar::open_or_focus_for`) → 사용자 입력이 포커스를 주며 연 팝업
//! 요청의 즉시 처리(`App::apply_user_focus_opens` 가 `user_focus_open_queued` 로 고르고 대기열을 처리한다) 순서다.

use super::press_cut::{Scene, activate, open_scene, terminal_point_r, type_text};
use super::repro::window_frame_with_find;
use super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, surface_rect_l};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::search_bar::{SEARCH_BAR_POPUP_ID, open_or_focus_for};
use crate::state::focus_node::FocusNode;
use crate::state::input_destination::InputDestination;
use crate::view::main::input_segment::key_focus;

/// 창이 다음 패스 입력을 꺼내 한 프레임을 그린다.
fn pass(s: &mut Scene, input: &mut WindowInput) {
    let events = input.take_pass(&mut s.state, &s.ctx);
    let engine = s.session.borrow_mut();
    window_frame_with_find(
        &s.ctx,
        &mut s.state,
        &engine.read(),
        &s.mgr,
        events,
        &mut s.query,
        false,
    );
}

/// 남은 몫을 모두 그리고 그린 패스 수를 돌려준다.
fn drain(s: &mut Scene, input: &mut WindowInput) -> usize {
    let mut passes = 0;
    while !input.is_empty() {
        pass(s, input);
        passes += 1;
    }
    passes
}

/// `pane` 의 터미널을 눌러 포커스 노드를 그 surface 로 옮기고 그린다.
fn focus_terminal(s: &mut Scene, input: &mut WindowInput, pane: u32, point: egui::Pos2) {
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    activate(s, pane);
    drain(s, input);
    // 검색창 입력칸이 포커스를 놓는 프레임까지 그린다.
    pass(s, input);
}

/// 창이 받은 검색 단축키 하나. 끊기 → 단축키 처리 → 포커스를 주며 연 팝업 요청의 즉시 처리 순서다.
fn find_shortcut(s: &mut Scene, input: &mut WindowInput, sid: u32) {
    let mut engine = s.session.borrow_mut();
    key_focus(&mut s.state, &engine.read(), input);
    open_or_focus_for(&mut s.state, Some(sid), "find");
    if s.state.user_focus_open_queued(Some(&s.mgr)) {
        for intent in s.state.take_pending_intents() {
            crate::intent::popup::handle(&mut s.state, &mut engine, &intent);
        }
    }
}

/// [k, 검색 단축키, w] 를 한 묶음으로 보내고 k 는 `sid` 터미널(PTY), w 는 그 surface 의 검색창 입력칸에 들어가는지 잰다.
fn k_find_w(s: &mut Scene, input: &mut WindowInput, sid: u32) {
    assert_eq!(
        type_text(s, input, false, "k"),
        None,
        "k 는 egui 로 가지 않는다"
    );
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.input_destination(
                &engine.read(),
                s.state.window_key_holds_terminal(
                    crate::adapters::ui::popup::window_input_test_support::typed_decision(
                        &s.state,
                        &engine.read(),
                        "k"
                    )
                )
            ),
            InputDestination::Terminal(sid),
            "k 의 갈래는 터미널(PTY)이다"
        );
    }
    find_shortcut(s, input, sid);
    let search = {
        let engine = s.session.borrow_mut();
        let node = s.state.focus_node(&engine.read());
        let Some(p) = s.state.popups.host_key_owner() else {
            panic!("검색 단축키 뒤 포커스 노드가 검색창이 아니다: {node:?}");
        };
        assert_eq!(p.id, SEARCH_BAR_POPUP_ID);
        assert_eq!(node, FocusNode::Popup(PopupNode::Host(p.instance)));
        p.instance
    };
    assert!(
        type_text(s, input, false, "w").is_some(),
        "w 는 검색창 몫이라 egui 로 간다"
    );
    let passes = drain(s, input);
    assert_eq!(
        s.state.search.query, "w",
        "w 가 검색창 입력칸에 들어간다 (패스 {passes})"
    );
    assert_eq!(
        s.state.popups.instance(search).map(|p| p.scope.clone()),
        Some(crate::adapters::ui::popup::PopupScope::Surface(sid))
    );
}

/// Given R 에 검색창이 열려 있고 포커스 노드가 R 터미널일 때, When 한 묶음에 [k, 검색 단축키, w]가 오면, Then k 는 R
/// 터미널로 가고 단축키가 검색창에 포커스를 즉시 준다. w 는 노드 차이로 끊긴 새 몫이고, 그 앞 빈 패스에서 입력칸이
/// 포커스를 얻어 w 를 받는다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_already_open_search_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    s.state.search.query.clear();
    k_find_w(&mut s, &mut input, SURFACE_R);
}

/// Given 검색창이 R 에 열려 있고 포커스 노드가 L 터미널일 때, When 한 묶음에 [k, L 의 검색 단축키, w]가 오면, Then 단축키의
/// 다시 열기 요청(검색창을 L 로 옮김)이 그 키 처리 안에서 적용되고 w 는 L 의 검색창에 들어간다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_search_field_moved_from_another_surface() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = surface_rect_l().right_bottom() - egui::vec2(20.0, 20.0);
    focus_terminal(&mut s, &mut input, PANE_L, point);
    k_find_w(&mut s, &mut input, SURFACE_L);
}

/// Given 검색창이 열려 있지 않고 포커스 노드가 R 터미널일 때, When 한 묶음에 [k, 검색 단축키, w]가 오면, Then 단축키의
/// 열기 요청이 그 키 처리 안에서 적용되어 새 검색창이 포커스 노드가 되고 w 는 그 입력칸에 들어간다.
#[test]
fn a_letter_after_the_find_shortcut_reaches_the_newly_opened_search_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    s.state.popups.close(SEARCH_BAR_POPUP_ID);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    assert!(!s.state.popups.is_open(SEARCH_BAR_POPUP_ID));
    s.state.search.query.clear();
    k_find_w(&mut s, &mut input, SURFACE_R);
}

/// T-12b-14 [Find 뒤 그리기 전 Find]: Given 검색창이 열려 있지 않고 포커스 노드가 R 터미널일 때, When 한 묶음에 [첫 find
/// 키, 둘째 find 키]가 오면, Then 첫 키의 단축키가 검색창을 R 범위(`PopupScope::Surface`)에 포커스로 열고, 둘째 키는 그리기
/// 전이어도 창 키 경로가 검색창의 노드 키맵에 싣는다(Surface level 의 find 단축키로 다시 올라가지 않는다). 그 몫을 그린 뒤
/// 포커스는 R 터미널로 돌아간다.
#[test]
fn a_second_find_before_drawing_returns_focus_to_the_terminal() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    s.state.popups.close(SEARCH_BAR_POPUP_ID);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    assert!(!s.state.popups.is_open(SEARCH_BAR_POPUP_ID));
    s.session.borrow_mut().runtime.settings.keybindings.find = vec!["ctrl+f".into()];
    find_shortcut(&mut s, &mut input, SURFACE_R);
    assert_eq!(
        s.state.popups.open_scope(SEARCH_BAR_POPUP_ID),
        Some(&crate::adapters::ui::popup::PopupScope::Surface(SURFACE_R)),
        "첫 find 가 검색창을 R 범위에 연다"
    );
    let find = egui::Event::Key {
        key: egui::Key::F,
        physical_key: Some(egui::Key::F),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::CTRL,
    };
    {
        let engine = s.session.borrow_mut();
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, find)
                .is_some(),
            "둘째 find 는 검색창에 실린다"
        );
    }
    drain(&mut s, &mut input);
    pass(&mut s, &mut input);
    let engine = s.session.borrow_mut();
    s.state.normalize_focus(&engine.read());
    assert!(
        s.state.popups.is_open(SEARCH_BAR_POPUP_ID),
        "검색창은 남는다"
    );
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_R)),
        "포커스가 R 터미널로 돌아간다"
    );
}

/// Given L 찾기 입력란에 포커스가 없을 때, When 한 묶음에 [L 찾기 입력란 누름, debug 붙여넣기 주입]이 오면, Then 붙여넣기는
/// 키 주입과 같이 누름 뒤에서 끊겨(`debug_input::inject_clipboard`) 누름 패스 다음 패스로 그려지고 입력란에 들어간다.
#[cfg(debug_assertions)]
#[test]
fn a_debug_paste_after_a_press_is_the_next_pass() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.find.center());
    }
    activate(&mut s, PANE_L);
    {
        let engine = s.session.borrow_mut();
        crate::view::main::debug_input::inject_clipboard(
            &mut s.state,
            &engine.read(),
            &mut input,
            egui::Event::Paste("zz".into()),
        );
    }
    let passes = drain(&mut s, &mut input);
    assert_eq!(passes, 2, "누름 패스와 붙여넣기 패스");
    assert_eq!(
        s.query, "zz",
        "붙여넣기가 누름이 포커스를 준 입력란에 들어간다"
    );
}

const PLUGIN: &str = "com.example.key-popup";

/// `mgr` 에 창 범위 팝업 하나(`viewer`)를 선언한 plugin 을 설치한다. 돌려준 stub 은 시험 동안 둔다.
fn declare_viewer(
    mgr: &mut crate::plugin::PluginManager,
) -> tasty_host_plugin::manager::CallerStub {
    let manifest = toml::from_str(&format!(
        "manifest_version = 1\nid = \"{PLUGIN}\"\nname = \"Key popup\"\nversion = \"1.0.0\"\n\
         api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n",
        crate::plugin::manifest::HOST_API_VERSION
    ))
    .expect("manifest");
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/key-popup"),
        manifest,
    }]);
    let stub = mgr.attach_caller_stub_for_test(PLUGIN);
    let decl = serde_json::from_value::<tasty_plugin_protocol::PopupDeclSpec>(
        serde_json::json!({ "scope_kind": "window", "size": { "width": 200, "height": 100 } }),
    )
    .expect("spec")
    .to_decl();
    mgr.declare_popup(PLUGIN, "viewer", decl).expect("declare");
    stub
}

/// 창이 받은 plugin 팝업 단축키 하나(`App::route_window_input` 의 `try_plugin_shortcut` 갈래). 단축키는 창 view 앞에서
/// 소비되어 끊기 없이 도구를 실행하고(`run_shortcut_action` 이 부르는 `run_tool` 과 같은 열기 대기열), 사용자 입력이
/// 포커스를 주며 연 팝업 요청이 있으면 그 자리에서 연다.
fn plugin_shortcut(s: &mut Scene) {
    queue_plugin_shortcut(s);
    if s.state.user_focus_open_queued(Some(&s.mgr)) {
        crate::app::dispatch::popup_opens::open_pending_popups(
            Some(&mut s.mgr),
            std::iter::once((&mut s.state, s.session.borrow_mut())),
        );
    }
}

/// plugin 팝업 단축키가 실행하는 도구(`run_tool` 과 같은 열기 대기열에 넣는다).
fn queue_plugin_shortcut(s: &mut Scene) {
    {
        let engine = s.session.borrow_mut();
        let read = engine.read();
        let target = s.state.command_target(&read);
        let surface = s.state.focused_surface_id(&read);
        let tool = crate::plugin::tool_registry::ToolItem {
            source: crate::plugin::tool_registry::ToolSource::Plugin {
                plugin_id: PLUGIN.into(),
                tool_id: "open".into(),
            },
            key: format!("{PLUGIN}/open"),
            label_i18n_key: String::new(),
            icon: None,
            action: crate::plugin::manifest::ToolAction::OpenPopup {
                popup_id: format!("{PLUGIN}/viewer"),
            },
            order_hint: 0,
        };
        crate::adapters::ui::tools_menu::invoke_tool_on(
            &mut s.state,
            &read,
            &tool,
            surface,
            target.structure_target(),
        );
    }
}

/// k 를 R 터미널로 보낸 뒤 `shortcut_then_w` 로 plugin 팝업 단축키와 w 를 밟고, w 가 새 plugin 팝업 몫인지 잰다.
fn k_then_plugin_popup_takes_w(
    s: &mut Scene,
    input: &mut WindowInput,
    shortcut_then_w: impl FnOnce(&mut Scene, &mut WindowInput) -> Option<bool>,
) {
    assert_eq!(
        type_text(s, input, false, "k"),
        None,
        "k 는 egui 로 가지 않는다"
    );
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.input_destination(
                &engine.read(),
                s.state.window_key_holds_terminal(
                    crate::adapters::ui::popup::window_input_test_support::typed_decision(
                        &s.state,
                        &engine.read(),
                        "k"
                    )
                )
            ),
            InputDestination::Terminal(SURFACE_R),
            "k 의 갈래는 터미널(PTY)이다"
        );
    }
    let fed = shortcut_then_w(s, input);
    let opened: Vec<u64> = s.mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(opened.len(), 1, "단축키가 plugin 팝업 하나를 연다");
    assert_eq!(
        fed,
        Some(true),
        "w 는 egui 로 가고 입력 시점에 키 주인을 묶는다"
    );
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(opened[0]))
    );
    assert_eq!(
        s.state.popups.bound_input_key_owner(),
        Some(PopupNode::Plugin(opened[0])),
        "w 의 입력 몫은 연 plugin 팝업에 묶였다(지금 포커스로 대신 고르지 않는다)"
    );
    assert_eq!(
        s.state.input_destination(
            &engine.read(),
            s.state.window_key_holds_terminal(
                crate::adapters::ui::popup::window_input_test_support::typed_decision(
                    &s.state,
                    &engine.read(),
                    "w"
                )
            )
        ),
        InputDestination::Overlay,
        "w 는 터미널(PTY)로 가지 않는다"
    );
}

/// Given 포커스 노드가 R 터미널이고 plugin 팝업이 열려 있지 않을 때, When 한 묶음에 [k, plugin 팝업 단축키, w]가 오면,
/// Then k 는 R 터미널로 가고 단축키가 연 plugin 팝업이 그 키 처리 안에서 포커스 노드가 되어 w 는 그 팝업 몫이다.
#[test]
fn a_letter_after_a_plugin_popup_shortcut_reaches_the_opened_popup() {
    let mut s = open_scene(|_| {});
    let _stub = declare_viewer(&mut s.mgr);
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    k_then_plugin_popup_takes_w(&mut s, &mut input, |s, input| {
        plugin_shortcut(s);
        type_text(s, input, false, "w")
    });
}

/// Given plugin 입력칸의 편집 보류 뒤에 [plugin 팝업 단축키, w]가 줄 섰을 때, When 보류가 풀려 재생하면, Then 줄 선
/// 입력은 plugin 단축키 단계부터 다시 거쳐(`ReplayStep::Input`) 단축키가 연 팝업이 그 자리에서 포커스 노드가 되고 w 는
/// 그 팝업 몫이다.
#[test]
fn a_letter_replayed_after_a_plugin_popup_shortcut_reaches_the_opened_popup() {
    use crate::adapters::ui::input::edit_key::Release;
    use crate::app::edit_key_replay::{ReplayStep, replay_steps};
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Queued {
        Shortcut,
        W,
    }
    let mut s = open_scene(|_| {});
    let _stub = declare_viewer(&mut s.mgr);
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    let steps = replay_steps(Release {
        replay: None,
        queued: vec![Queued::Shortcut, Queued::W],
        queued_at: Vec::new(),
    });
    k_then_plugin_popup_takes_w(&mut s, &mut input, |s, input| {
        let mut fed = None;
        for step in steps {
            let ReplayStep::Input(event, _) = step else {
                panic!("plugin 보류 뒤 줄 선 입력은 단축키 단계부터 다시 거친다: {step:?}");
            };
            match event {
                Queued::Shortcut => plugin_shortcut(s),
                Queued::W => fed = type_text(s, input, false, "w"),
            }
        }
        fed
    });
}

/// plugin 팝업 단축키를 밟은 뒤 연 인스턴스가 없고 포커스 노드가 R 터미널 그대로이며, w 가 터미널(PTY)로 가는지 잰다.
fn the_failed_open_leaves_the_terminal_focused(s: &mut Scene, input: &mut WindowInput) {
    assert_eq!(
        s.mgr.popup_instances().count(),
        0,
        "실패한 열기는 인스턴스를 남기지 않는다"
    );
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_R))
        );
    }
    assert_eq!(
        type_text(s, input, false, "w"),
        None,
        "w 는 egui 로 가지 않는다"
    );
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.input_destination(
            &engine.read(),
            s.state.window_key_holds_terminal(
                crate::adapters::ui::popup::window_input_test_support::typed_decision(
                    &s.state,
                    &engine.read(),
                    "w"
                )
            )
        ),
        InputDestination::Terminal(SURFACE_R),
        "w 는 R 터미널 몫이다"
    );
}

/// Given 포커스 노드가 R 터미널이고 plugin 으로 가는 송신이 끊겼을 때, When plugin 팝업 단축키가 오면, Then
/// `popup.open` 을 보내지 못해 인스턴스가 남지 않고 포커스 노드는 R 터미널이다.
#[test]
fn a_plugin_popup_that_cannot_be_sent_keeps_the_terminal_focused() {
    let mut s = open_scene(|_| {});
    let stub = declare_viewer(&mut s.mgr);
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    drop(stub);
    plugin_shortcut(&mut s);
    the_failed_open_leaves_the_terminal_focused(&mut s, &mut input);
}

/// Given plugin 팝업 단축키가 연 팝업이 포커스 노드일 때, When plugin 이 `popup.open` 에 오류로 답하고 창이 plugin
/// 사건을 비우면(`drain_plugin_popup_events`), Then 인스턴스가 지워지고 포커스 노드는 R 터미널로 돌아온다.
#[test]
fn a_plugin_popup_whose_open_fails_returns_the_focus() {
    let mut s = open_scene(|_| {});
    let _stub = declare_viewer(&mut s.mgr);
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    plugin_shortcut(&mut s);
    let opened: Vec<u64> = s.mgr.popup_instances().map(|(id, _)| id).collect();
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Popup(PopupNode::Plugin(opened[0])),
            "열기 응답 전에는 연 팝업이 포커스 노드다"
        );
    }
    s.mgr.fail_popup_opens_for_test(PLUGIN, "no such popup");
    crate::app::dispatch::plugin_popup_events::drain_plugin_popup_events(
        std::iter::once(&mut s.state),
        Some(&mut s.mgr),
    );
    the_failed_open_leaves_the_terminal_focused(&mut s, &mut input);
}

/// Given 다른 창(A)에 결속해 열린 창 범위 plugin 팝업이 있고 이 창(B)의 포커스 노드가 R 터미널일 때, When B 에서 그
/// 팝업의 단축키가 오면, Then 여는 처리는 그 인스턴스를 돌려받고 B 의 포커스를 바꾸지 않으므로 즉시 적용 판정이
/// 거짓이고(`popup_scope::open_would_focus_window`), 대기열을 처리해도 B 의 포커스 노드는 R 터미널이다.
#[test]
fn a_plugin_popup_open_in_another_window_does_not_apply_early() {
    let mut s = open_scene(|_| {});
    let _stub = declare_viewer(&mut s.mgr);
    let mut input = WindowInput::default();
    let point = terminal_point_r(&s);
    focus_terminal(&mut s, &mut input, PANE_R, point);
    let in_a = s
        .mgr
        .open_popup_instance(PLUGIN, "viewer", serde_json::Value::Null)
        .expect("창 A 에 연다");
    assert!(
        !s.state.plugin_window_popups.contains(&in_a),
        "A 의 결속이다"
    );
    queue_plugin_shortcut(&mut s);
    assert_eq!(s.state.pending_popup_opens.len(), 1);
    assert!(
        !s.state.user_focus_open_queued(Some(&s.mgr)),
        "B 의 포커스를 바꾸지 않는 열기는 앞당기지 않는다"
    );
    crate::app::dispatch::popup_opens::open_pending_popups(
        Some(&mut s.mgr),
        std::iter::once((&mut s.state, s.session.borrow_mut())),
    );
    let opened: Vec<u64> = s.mgr.popup_instances().map(|(id, _)| id).collect();
    assert_eq!(opened, [in_a], "같은 인스턴스를 돌려받는다");
    assert!(!s.state.plugin_window_popups.contains(&in_a));
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_R))
    );
}
