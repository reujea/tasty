//! 제품 상태 시험: native WebView 가 넘긴 키 묶음을 제품 소비 루프(`consume_forwarded_keys`)로 소비하고 창 상태를 본다.
//! 시험 창이 제품 소비자 trait(`ForwardedKeyConsumer`)를 구현한다. 키 처리는 제품 디스패치(`App::dispatch_forwarded_webview_key`)
//! 와 같은 순서로 게이트·plugin 단축키 단계(`forwarded_gate_and_plugin_steps`, `Gated`·`Handled` 결과값까지 제품과 같은
//! 함수), 새로고침(`native_refresh_outcome`), 내장 단축키(`MainView::handle_keybinding_shortcuts`)를 부른다. 조기 적용은 제품과
//! 같은 판정(`user_focus_open_queued`)이 참이면 꺼낸 요청 묶음을 제품의 세션 묶음 처리(`dispatch_state_intents`: 요청 분류
//! `classify_intent`, 즉시 요청 처리 `dispatch_one_intent`)에 `view` 없이 넘기고, 돌아온 Domain 요청이 없음을 확인한 뒤
//! plugin 팝업 열기를 `open_pending_popups` 로 처리한다. 결과 알림은 루프가 건넨 키 ID·surface 와 재전송 판정
//! (`resends_to_page`)을 기록한다. 제품 디스패치의 단계 순서와 App 소비자의 위임(처리·적용·알림)은
//! `source_guards::press_focus_call_sites` 와 `source_guards::window_key_gate_call_sites` 가, 조기 적용의 App 디스패치
//! 함수(`apply_user_focus_opens` ~ `EngineScanMut::resolve`, `dispatch_pending_popup_opens`)의 본문은
//! `window_key_gate_call_sites::early_focus_opens_reach_the_handlers_the_state_test_calls` 의 표가 고정한다.
//!
//! 범위 밖: App 의 창 조회와 modal 창, html surface view 단계(`run_received_surface_key`)와 창 제어·스크립트 단계, WebView
//! 사본에 결과를 넣는 일, 조기 적용의 다른 창·parked 세션과 domain 요청 처리, 공유 즉시 처리기의 `view` 가 Some 인 갈래,
//! engine 접근 공용 함수(`EngineScanMut::from_fields`·`split`·`sessions`, `EngineRegistry::split_by_id`)와 View
//! 다운캐스트(`View::as_main`·`as_main_mut`. 시험은 세션을 직접 빌린다), native 콜백의 편집 판정과 수집.
//! 내장 단축키 단계가 `EventLoopProxy` 를 받으므로 X11 `EventLoop` 가 필요하고, `EventLoop` 는 프로세스에 하나뿐이라
//! 시험마다 새 프로세스에서 혼자 돈다.

use std::sync::Arc;

use winit::keyboard::{Key, ModifiersState};

use super::{
    ForwardedKeyConsumer, ForwardedKeyOutcome, consume_forwarded_keys,
    forwarded_gate_and_plugin_steps, native_refresh_outcome,
};
use crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID;
use crate::adapters::ui::popup::focus::PopupNode;
use crate::plugin::PluginManager;
use crate::state::MainViewState;
use crate::state::focus_node::FocusNode;
use crate::view::main::MainView;
use crate::webview::WebViewKeyEvent;

const PLUGIN: &str = "com.example.webview-key-popup";
/// plugin 전역 명령의 기본 조합. 기본 프리셋의 host 바인딩과 겹치지 않는다.
const PLUGIN_COMBO: &str = "ctrl+alt+y";

/// 키를 받은 WebView 가 있는 창 하나.
struct Window {
    state: MainViewState,
    session: crate::runtime::engine_session::EngineSession,
    /// 제품 즉시 요청 처리(`dispatch_state_intents`)가 받는 App 서비스.
    core: crate::app::services::AppServices,
    mgr: Option<PluginManager>,
    proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>,
    /// 키를 받은 WebView 의 surface. 창의 포커스 surface 다.
    received: u32,
    /// 결과 알림에 건넨 (키 ID, 결과).
    finished: Vec<(u64, ForwardedKeyOutcome)>,
}

impl Window {
    fn new(proxy: winit::event_loop::EventLoopProxy<crate::AppEvent>) -> Self {
        let (state, mut session) = crate::state::tests::test_state();
        let received = {
            let engine = session.borrow_mut();
            state
                .focused_surface_id(&engine.read())
                .expect("포커스 surface")
        };
        Self {
            state,
            session,
            core: crate::adapters::ipc::handler::cli_entry_tests::test_core(),
            mgr: None,
            proxy,
            received,
            finished: Vec::new(),
        }
    }

    fn key(&self, id: u64, ch: &str, mods: ModifiersState) -> WebViewKeyEvent {
        WebViewKeyEvent {
            id,
            surface_id: self.received,
            key: Key::Character(ch.into()),
            mods,
        }
    }

    /// 한 번의 수집으로 받은 키 묶음을 제품 소비 루프로 소비하고 키마다의 결과를 돌려준다.
    fn consume(&mut self, keys: Vec<WebViewKeyEvent>) -> Vec<ForwardedKeyOutcome> {
        consume_forwarded_keys(self, vec![(winit::window::WindowId::from(1), keys)]);
        self.finished.iter().map(|(_, outcome)| *outcome).collect()
    }

    /// 결과 알림마다 (키 ID, 페이지로 다시 넣는지).
    fn resent(&self) -> Vec<(u64, bool)> {
        self.finished
            .iter()
            .map(|(key_id, outcome)| (*key_id, outcome.resends_to_page()))
            .collect()
    }

    fn focus_node(&mut self) -> FocusNode {
        let engine = self.session.borrow_mut();
        self.state.normalize_focus(&engine);
        self.state.focus_node(&engine)
    }
}

impl ForwardedKeyConsumer for Window {
    /// 제품 디스패치의 창 단계: 게이트·plugin 단축키 → 새로고침 → 받은 surface 의 내장 단축키.
    fn dispatch(
        &mut self,
        _id: winit::window::WindowId,
        ev: WebViewKeyEvent,
    ) -> ForwardedKeyOutcome {
        let engine = self.session.borrow_mut();
        let received = self.received;
        if let Some(outcome) = forwarded_gate_and_plugin_steps(
            &mut self.state,
            &engine,
            false,
            self.mgr.as_mut(),
            |sid| sid == received,
            &ev,
        ) {
            return outcome;
        }
        let read = engine.read();
        if let Some(outcome) = native_refresh_outcome(
            &mut self.state,
            &read,
            &read.settings.keybindings.refresh,
            &ev,
        ) {
            return outcome;
        }
        let target = self
            .state
            .command_target_for_surface(&read, ev.surface_id)
            .expect("키를 받은 surface 의 대상");
        let kb = read.settings.keybindings.clone();
        let cells = crate::adapters::ui::input::shortcuts::CellGeometry {
            w: crate::model::PhysicalPx(8.0),
            h: crate::model::PhysicalPx(16.0),
            scale_factor: 1.0,
        };
        let rect = crate::model::PhysicalRect {
            x: crate::model::PhysicalPx(0.0),
            y: crate::model::PhysicalPx(0.0),
            width: crate::model::PhysicalPx(1280.0),
            height: crate::model::PhysicalPx(800.0),
        };
        ForwardedKeyOutcome::from_handled(MainView::handle_keybinding_shortcuts(
            &mut self.state,
            &read,
            &kb,
            &ev.key,
            ev.mods,
            &target,
            rect,
            cells,
            &self.proxy,
        ))
    }

    /// 조기 적용: 제품과 같은 판정이 참이면 host 요청과 plugin 팝업 열기를 그 자리에서 처리한다.
    fn apply_focus_opens(&mut self, _id: winit::window::WindowId) {
        if !self.state.user_focus_open_queued(self.mgr.as_ref()) {
            return;
        }
        let batch = self.state.take_pending_intents();
        let mut engine = self.session.borrow_mut();
        let domain = crate::app::dispatch::intents::dispatch_state_intents(
            &mut self.core,
            &mut self.state,
            &mut engine,
            batch,
            self.mgr.as_ref(),
            None,
        );
        assert!(
            domain.is_empty(),
            "이 장면의 요청은 즉시 처리하는 host 요청뿐이다: {domain:?}"
        );
        crate::app::dispatch::popup_opens::open_pending_popups(
            self.mgr.as_mut(),
            std::iter::once((&mut self.state, engine)),
        );
    }

    fn finish(
        &mut self,
        _id: winit::window::WindowId,
        surface_id: u32,
        key_id: u64,
        outcome: ForwardedKeyOutcome,
    ) {
        assert_eq!(
            surface_id, self.received,
            "키 {key_id} 의 결과를 다른 WebView 에 알린다"
        );
        self.finished.push((key_id, outcome));
    }
}

fn event_loop() -> winit::event_loop::EventLoop<crate::AppEvent> {
    crate::x11_test_loop::x11_event_loop()
}

fn ctrl_shift() -> ModifiersState {
    ModifiersState::CONTROL | ModifiersState::SHIFT
}

/// 기본 프리셋의 두 바인딩이 이 시험이 넣는 키와 실제 수식키 그대로 맞는다.
fn assert_default_bindings(w: &mut Window) {
    let keys = [
        w.key(0, "P", ctrl_shift()),
        w.key(0, "I", ctrl_shift()),
        w.key(0, "b", ModifiersState::CONTROL),
    ];
    let engine = w.session.borrow_mut();
    let read = engine.read();
    let kb = &read.settings.keybindings;
    for (binding, key) in [
        &kb.toggle_command_palette,
        &kb.toggle_notifications,
        &kb.toggle_sidebar_collapse,
    ]
    .into_iter()
    .zip(keys)
    {
        assert!(
            crate::shortcuts::matches_any_binding(binding, &key.key, key.mods),
            "{binding:?} 가 {key:?} 와 맞지 않는다"
        );
    }
}

/// host 갈래. Given WebView 가 키 포커스를 쥔 창, When 한 번의 수집에 [Ctrl+Shift+P(명령 팔레트), Ctrl+B(sidebar
/// 접기)] 가 들면, Then 첫 키는 처리되고 그 자리에서 팔레트가 열려 포커스 노드가 된다. 둘째 키는 팔레트의 게이트에
/// 막혀(`Gated`) sidebar 가 그대로이고, 두 키 모두 페이지로 다시 넣지 않는다.
#[test]
fn a_palette_opened_by_a_forwarded_key_gates_the_next_key_in_the_window_state() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_palette_opened_by_a_forwarded_key_gates_the_next_key_in_the_window_state",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut w = Window::new(event_loop.create_proxy());
    assert_default_bindings(&mut w);
    let collapsed = w.state.sidebar_collapsed;
    let batch = vec![
        w.key(1, "P", ctrl_shift()),
        w.key(2, "b", ModifiersState::CONTROL),
    ];
    let outcomes = w.consume(batch);
    assert_eq!(
        outcomes,
        [ForwardedKeyOutcome::Handled, ForwardedKeyOutcome::Gated]
    );
    assert_eq!(
        w.state.sidebar_collapsed, collapsed,
        "둘째 키가 sidebar 를 바꿨다"
    );
    assert_eq!(
        w.resent(),
        [(1, false), (2, false)],
        "Gated 를 페이지로 다시 넣었다"
    );
    let FocusNode::Popup(PopupNode::Host(iid)) = w.focus_node() else {
        panic!("포커스 노드가 host 팝업이 아니다: {:?}", w.focus_node());
    };
    assert_eq!(
        w.state.popups.instance(iid).map(|p| p.id),
        Some(COMMAND_PALETTE_POPUP_ID)
    );
}

/// passive 대조. Given 같은 창, When [Ctrl+Shift+I(알림 목록 토글, 포커스를 요청하지 않는 열기), Ctrl+B] 가 들면,
/// Then 조기 적용이 일어나지 않아 둘째 키가 실행되고(sidebar 반전), 그때 알림 목록은 닫힌 채이며 그 요청이 대기열에
/// 남는다.
#[test]
fn a_passive_open_by_a_forwarded_key_stays_queued_and_the_next_key_runs() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_passive_open_by_a_forwarded_key_stays_queued_and_the_next_key_runs",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut w = Window::new(event_loop.create_proxy());
    assert_default_bindings(&mut w);
    let collapsed = w.state.sidebar_collapsed;
    let batch = vec![
        w.key(1, "I", ctrl_shift()),
        w.key(2, "b", ModifiersState::CONTROL),
    ];
    let outcomes = w.consume(batch);
    assert_eq!(
        outcomes,
        [ForwardedKeyOutcome::Handled, ForwardedKeyOutcome::Handled]
    );
    assert_eq!(w.state.sidebar_collapsed, !collapsed, "둘째 키가 실행됐다");
    assert_eq!(w.resent(), [(1, false), (2, false)]);
    assert!(
        !w.state.popups.is_open("notifications"),
        "알림 목록이 열렸다"
    );
    let queued: Vec<_> = w
        .state
        .pending_intents
        .iter()
        .map(|i| format!("{:?}", i.body))
        .collect();
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert!(
        queued[0].contains("TogglePopup") && queued[0].contains("notifications"),
        "{queued:?}"
    );
}

/// 창 범위 팝업 `viewer` 를 선언하고, 그 팝업을 여는 전역 명령을 기본 조합 `PLUGIN_COMBO` 로 등록한 관리자.
fn plugin_manager() -> (PluginManager, tasty_host_plugin::manager::CallerStub) {
    let mut mgr = PluginManager::with_registries(
        Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        Arc::new(crate::file::format::FileFormatRegistry::new()),
        Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let manifest: tasty_plugin_manifest::Manifest = toml::from_str(&format!(
        "manifest_version = 1\nid = \"{PLUGIN}\"\nname = \"WebView key popup\"\nversion = \"1.0.0\"\n\
         api_version = \"{}\"\n[entry]\ntype = \"process\"\ncommand = \"echo\"\n\
         [[contributes.commands]]\nid = \"open\"\ntitle_i18n_key = \"open.title\"\n\
         default_keybinding = \"{PLUGIN_COMBO}\"\nscope = \"global\"\n\
         action = {{ kind = \"open_popup\", popup_id = \"{PLUGIN}/viewer\" }}\n",
        crate::plugin::manifest::HOST_API_VERSION
    ))
    .expect("manifest");
    mgr.install_packages_for_test(vec![tasty_host_plugin::PluginPackage {
        dir: std::path::PathBuf::from("/nonexistent/webview-key-popup"),
        manifest: manifest.clone(),
    }]);
    mgr.command_registry.register_plugin(&manifest);
    let stub = mgr.attach_caller_stub_for_test(PLUGIN);
    let decl = serde_json::from_value::<tasty_plugin_protocol::PopupDeclSpec>(
        serde_json::json!({ "scope_kind": "window", "size": { "width": 200, "height": 100 } }),
    )
    .expect("spec")
    .to_decl();
    mgr.declare_popup(PLUGIN, "viewer", decl).expect("declare");
    (mgr, stub)
}

/// plugin 갈래. Given plugin 전역 명령이 창 범위 팝업(덮지 않음)을 여는 창, When [그 명령의 조합, Ctrl+B] 가 들면,
/// Then 첫 키는 plugin 단축키로 처리되고 그 자리에서 plugin 팝업이 열려 포커스 노드가 된다. 둘째 키는 받은 surface 에서
/// 시작한 경로가 덮이지 않았으므로 그 surface 의 단축키(sidebar 접기)로 처리된다. 그 조합은 WebView 가 호스트로 넘기는
/// plugin 전역 조합에 든다(`webview_command_bindings`).
#[test]
fn a_plugin_popup_opened_by_a_forwarded_key_leaves_the_next_key_to_the_received_surface() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_plugin_popup_opened_by_a_forwarded_key_leaves_the_next_key_to_the_received_surface",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut w = Window::new(event_loop.create_proxy());
    let (mgr, _stub) = plugin_manager();
    {
        let engine = w.session.borrow_mut();
        let bindings = crate::plugin_bridge::key_dispatch::webview_command_bindings(
            &mgr,
            &engine.read().settings.keybindings,
        );
        assert!(
            bindings.global.iter().any(|b| b == PLUGIN_COMBO),
            "{bindings:?}"
        );
    }
    w.mgr = Some(mgr);
    let collapsed = w.state.sidebar_collapsed;
    let batch = vec![
        w.key(1, "y", ModifiersState::CONTROL | ModifiersState::ALT),
        w.key(2, "b", ModifiersState::CONTROL),
    ];
    let outcomes = w.consume(batch);
    assert_eq!(
        outcomes,
        [ForwardedKeyOutcome::Handled, ForwardedKeyOutcome::Handled]
    );
    assert_ne!(
        w.state.sidebar_collapsed, collapsed,
        "둘째 키가 sidebar 를 바꾸지 않았다"
    );
    assert_eq!(w.resent(), [(1, false), (2, false)]);
    let opened: Vec<u64> = w
        .mgr
        .as_ref()
        .expect("관리자")
        .popup_instances()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert_eq!(
        w.focus_node(),
        FocusNode::Popup(PopupNode::Plugin(opened[0]))
    );
}

/// T-12b-7 래치 갈래. Given 설정 열기 요청을 아직 그리지 않은 창(래치), When 받은 WebView 가 Ctrl+B(sidebar 접기)를
/// 넘기면, Then 창 전체 차단에 막혀(`Gated`) sidebar 가 그대로이고 페이지로 다시 넣지 않는다.
#[test]
fn a_settings_open_request_gates_a_forwarded_shortcut() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_settings_open_request_gates_a_forwarded_shortcut",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut w = Window::new(event_loop.create_proxy());
    assert_default_bindings(&mut w);
    w.state.settings_open_requested = true;
    let collapsed = w.state.sidebar_collapsed;
    let batch = vec![w.key(1, "b", ModifiersState::CONTROL)];
    assert_eq!(w.consume(batch), [ForwardedKeyOutcome::Gated]);
    assert_eq!(w.state.sidebar_collapsed, collapsed, "sidebar 를 바꿨다");
    assert_eq!(w.resent(), [(1, false)], "Gated 를 페이지로 다시 넣었다");
}

/// T-12b-7 래치 대조. Given 래치·모달·무대가 없는 같은 창, When 같은 Ctrl+B 가 들면, Then 처리되어 sidebar 가 바뀐다.
#[test]
fn a_forwarded_shortcut_without_a_window_block_runs() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_forwarded_shortcut_without_a_window_block_runs",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let event_loop = event_loop();
    let mut w = Window::new(event_loop.create_proxy());
    assert_default_bindings(&mut w);
    let collapsed = w.state.sidebar_collapsed;
    let batch = vec![w.key(1, "b", ModifiersState::CONTROL)];
    assert_eq!(w.consume(batch), [ForwardedKeyOutcome::Handled]);
    assert_eq!(w.state.sidebar_collapsed, !collapsed);
    assert_eq!(w.resent(), [(1, false)]);
}
