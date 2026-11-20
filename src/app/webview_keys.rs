//! native webview가 전달한 키·포커스를 처리한다. winit 키 처리 전체를 재현하지는 않는다.
//! 플러그인 명령을 먼저 시도하고 호스트 단축키를 실행한 뒤 공용 후처리를 사용한다.

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::view::ui::View;

impl App {
    /// 보이는 페이지나 수신 중인 native 작업이 활성·비최소화 창에 있으면 Linux 폴링을 예약한다.
    /// Linux는 먼저 GTK 이벤트를 처리하며 macOS·Windows는 별도 폴링 타이머를 쓰지 않는다.
    pub(crate) fn pump_webview_key_events(&mut self) -> bool {
        let mut any_webview = false;
        let mut needs_poll = false;
        for w in self.view.views.values() {
            let Some(main) = w.as_main() else {
                continue;
            };
            if main.webviews.is_empty() {
                continue;
            }
            any_webview = true;
            if webview_poll_needed(
                main.webview_any_visible || main.webviews.values().any(received_work_pending),
                main.base.winit.is_minimized() == Some(true),
                main.base.state.focused,
                || main.webviews.values().any(|wv| wv.holds_keyboard_focus()),
            ) {
                needs_poll = true;
            }
        }
        #[cfg(target_os = "linux")]
        let any_webview = any_webview || crate::webview::entry_response::callbacks_pending();
        if !any_webview {
            return false;
        }

        #[cfg(target_os = "linux")]
        crate::system_tray::pump_gtk_events();

        // 이후 dispatch가 self를 가변 대여하므로 view 순회 중에는 이벤트만 모은다.
        let mut focus_batch: Vec<(winit::window::WindowId, Vec<u32>)> = Vec::new();
        let mut key_batch: Vec<(
            winit::window::WindowId,
            Vec<crate::webview::WebViewKeyEvent>,
        )> = Vec::new();
        for (id, w) in &self.view.views {
            let Some(main) = w.as_main() else {
                continue;
            };
            let focus = main.webview_key_bridge.take_focus_requests();
            if !focus.is_empty() {
                focus_batch.push((*id, focus));
            }
            let keys = main.webview_key_bridge.take_pending();
            if !keys.is_empty() {
                key_batch.push((*id, keys));
            }
        }

        // 클릭으로 옮긴 포커스를 먼저 반영해야 단축키가 그 surface에 적용된다.
        for (id, surfaces) in focus_batch {
            for sid in surfaces {
                self.focus_surface_from_webview(id, sid);
            }
        }

        consume_forwarded_keys(self, key_batch);
        needs_poll
    }

    fn focus_surface_from_webview(&mut self, id: winit::window::WindowId, surface_id: u32) {
        let Some((main, engine)) = engines_mut!(self).window_pair(id) else {
            return;
        };
        if main.state.focus_surface_by_id(engine.core, surface_id) {
            main.mark_dirty();
        }
    }

    /// 처리 결과를 키를 가져간 WebView에 알린다. Tasty가 처리하지 못한 키는 그 WebView에 다시 넣어
    /// 엔진·페이지로 보낸다. 입력 게이트로 막은 키는 다시 넣지 않는다.
    fn finish_forwarded_webview_key(
        &mut self,
        id: winit::window::WindowId,
        surface_id: u32,
        key_id: u64,
        outcome: ForwardedKeyOutcome,
    ) {
        if let Some(webview) = self
            .view
            .views
            .get(&id)
            .and_then(|w| w.as_main())
            .and_then(|main| main.webviews.get(&surface_id))
        {
            webview.finish_forwarded_key(key_id, outcome.resends_to_page());
        }
    }

    /// 키 도착만으로 모델 포커스를 옮기지 않는다. X11의 자식 창 입력이
    /// 호스트에 없는 focus-follows-mouse 동작을 만들지 않도록 클릭 요청과 구분한다.
    fn dispatch_forwarded_webview_key(
        &mut self,
        id: winit::window::WindowId,
        ev: crate::webview::WebViewKeyEvent,
    ) -> ForwardedKeyOutcome {
        // winit 경로의 modal_blocks_input·입력 게이트와 같이, modal 창이나 overlay·전체화면 무대가
        // 열린 동안에는 plugin·새로고침·host 단축키 어느 것도 처리하지 않는다.
        let modal_active = self.view.is_modal_active();
        let allowed = match engines_mut!(self).window_pair(id) {
            Some((main, engine)) => forwarded_key_gate_open(&mut main.state, &engine, modal_active),
            None => forwarded_key_may_dispatch(modal_active, false, false),
        };
        if !allowed {
            return ForwardedKeyOutcome::Gated;
        }
        if self.dispatch_plugin_shortcut_key(id, &ev.key, ev.mods, Some(ev.surface_id)) {
            return ForwardedKeyOutcome::Handled;
        }
        let Some((main, mut engine)) = engines_mut!(self).window_pair(id) else {
            return ForwardedKeyOutcome::Gated;
        };
        // 플러그인 단축키에서 처리하지 않은 키는 원래 webview가 사라졌으면 버린다.
        if !main.webviews.contains_key(&ev.surface_id) {
            return ForwardedKeyOutcome::Gated;
        }
        if let Some(outcome) = dispatch_native_refresh_key(main, &mut engine, &ev) {
            return outcome;
        }
        // 호스트 단축키는 모델 포커스가 아니라 키를 받은 surface 의 workspace·pane·탭을 대상으로 한다.
        let Some(target) = main
            .state
            .command_target_for_surface(&engine.read(), ev.surface_id)
        else {
            return ForwardedKeyOutcome::Gated;
        };
        let handled = match main.run_received_surface_key(
            &engine.read(),
            &ev.key,
            ev.mods,
            ev.surface_id,
            &target,
        ) {
            Some(handled) => handled,
            None => main.handle_received_shortcut(&engine.read(), &ev.key, ev.mods, &target),
        };
        if handled {
            main.after_shortcut_consumed(&engine.read());
        }
        // 호스트 단축키가 마지막 workspace를 닫았을 수 있어 다음 redraw 전에 닫기 요청을 처리한다.
        self.close_self_requesting_windows();
        ForwardedKeyOutcome::from_handled(handled)
    }
}

/// native WebView 가 넘긴 키 묶음을 키 하나씩 처리하는 쪽. 제품은 `App` 이고, 소비 순서 시험은 대역을 쓴다.
trait ForwardedKeyConsumer {
    /// 키 하나를 plugin·새로고침·host 단축키로 처리한다.
    fn dispatch(
        &mut self,
        id: winit::window::WindowId,
        ev: crate::webview::WebViewKeyEvent,
    ) -> ForwardedKeyOutcome;
    /// 키 하나를 처리한 직후, 그 창 대기열에 포커스를 주는 팝업 열기가 있으면 다음 키 판정 전에 적용한다.
    fn apply_focus_opens(&mut self, id: winit::window::WindowId);
    /// 처리 결과를 키를 가져간 WebView 에 알린다.
    fn finish(
        &mut self,
        id: winit::window::WindowId,
        surface_id: u32,
        key_id: u64,
        outcome: ForwardedKeyOutcome,
    );
}

impl ForwardedKeyConsumer for App {
    fn dispatch(
        &mut self,
        id: winit::window::WindowId,
        ev: crate::webview::WebViewKeyEvent,
    ) -> ForwardedKeyOutcome {
        self.dispatch_forwarded_webview_key(id, ev)
    }

    fn apply_focus_opens(&mut self, id: winit::window::WindowId) {
        self.apply_user_focus_opens(id);
    }

    fn finish(
        &mut self,
        id: winit::window::WindowId,
        surface_id: u32,
        key_id: u64,
        outcome: ForwardedKeyOutcome,
    ) {
        self.finish_forwarded_webview_key(id, surface_id, key_id, outcome);
    }
}

/// 수집한 키 묶음을 창별로 사건 순서대로 소비한다. 수집(`take_pending`)은 view 를 빌린 채 하고, 소비는 그 뒤에
/// 한다. 키마다 처리 → 포커스 열기 적용(`App::apply_user_focus_opens`) → 결과 알림 순서다.
fn consume_forwarded_keys(
    consumer: &mut impl ForwardedKeyConsumer,
    batch: Vec<(
        winit::window::WindowId,
        Vec<crate::webview::WebViewKeyEvent>,
    )>,
) {
    for (id, keys) in batch {
        for ev in keys {
            let (key_id, surface_id) = (ev.id, ev.surface_id);
            let outcome = consumer.dispatch(id, ev);
            // 이 키가 포커스를 주는 팝업을 열었으면 다음 키는 그 팝업의 게이트로 판정한다(winit 키와 같은 조기
            // 적용). 결과와 관계없이 부른다. 대기열에 그런 요청이 없으면 아무것도 하지 않는다.
            consumer.apply_focus_opens(id);
            consumer.finish(id, surface_id, key_id, outcome);
        }
    }
}

/// native WebView가 넘긴 키의 처리 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardedKeyOutcome {
    /// Tasty 단축키가 동작했다. 키를 넘기지 않는다.
    Handled,
    /// 단축키가 동작하지 않았다. 키를 엔진·페이지로 넘긴다.
    NotHandled,
    /// 입력 게이트가 막았거나 키를 받은 WebView·창이 없다. 넘기지 않는다.
    Gated,
}

impl ForwardedKeyOutcome {
    fn from_handled(handled: bool) -> Self {
        if handled {
            Self::Handled
        } else {
            Self::NotHandled
        }
    }

    /// 키를 가져간 WebView 에 다시 넣어 엔진·페이지로 보낼지. 처리하지 못한 키만 보낸다. 입력 게이트가 막은 키는
    /// 보내지 않는다.
    fn resends_to_page(self) -> bool {
        self == Self::NotHandled
    }
}

// A receiving page may be hidden by its host state while WebKit still owes native completions.
// Cached-only stores must not keep a hidden view polling.
fn received_work_pending(webview: &crate::webview::PlatformWebView) -> bool {
    webview.entry_response_source().is_some_and(|source| {
        source.is_current()
            && (webview.nav_state() == crate::webview::NavState::Loading
                || source.pending_resources())
    })
}

/// Linux GDK는 별도 X 연결로 이벤트를 받아 winit을 깨우지 못한다. 보이는 페이지 또는
/// native 완료 대기가 있고 창이 활성·비최소화일 때 폴링한다. native 자식이 포커스를
/// 가진 경우도 활성으로 본다. OS 포커스 조회는 앞 조건과 winit 값으로 결정한다.
fn webview_poll_needed(
    any_visible: bool,
    minimized: bool,
    winit_focused: bool,
    webview_holds: impl FnOnce() -> bool,
) -> bool {
    any_visible
        && !minimized
        && crate::view::main::redraw::host_window_has_os_focus(winit_focused, webview_holds)
}

/// 한 창에서 native WebView 가 넘긴 키를 처리해도 되는지. 포커스 노드를 정규화한 뒤 그 창의 keyboard overlay·전체화면
/// 무대와 `modal_active`(modal 창)를 본다. App 의 디스패치와 제품 상태 시험이 함께 쓴다.
fn forwarded_key_gate_open(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    modal_active: bool,
) -> bool {
    state.normalize_focus(engine);
    forwarded_key_may_dispatch(
        modal_active,
        state.window_keys_to_overlay(),
        state.fullscreen_stage_active(),
    )
}

/// native WebView 가 넘긴 키를 plugin·새로고침·host 단축키로 처리해도 되는지. winit 경로가 막는
/// 상태(modal 창, keyboard overlay, 전체화면 무대)에서는 모두 처리하지 않는다.
fn forwarded_key_may_dispatch(modal_active: bool, overlay_open: bool, fullscreen: bool) -> bool {
    !modal_active && !overlay_open && !fullscreen
}

/// 공통 새로고침은 모델 포커스가 아니라 키를 받은 페이지를 대상으로 한다. 새로고침 키가 아니면 None이다.
fn dispatch_native_refresh_key(
    main: &mut crate::view::main::MainView,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    ev: &crate::webview::WebViewKeyEvent,
) -> Option<ForwardedKeyOutcome> {
    let outcome = native_refresh_outcome(
        &mut main.state,
        &engine.read(),
        &engine.runtime.settings.keybindings.refresh,
        ev,
    )?;
    if outcome == ForwardedKeyOutcome::Handled {
        main.after_shortcut_consumed(&engine.read());
    }
    Some(outcome)
}

/// 새로고침 키를 키를 받은 페이지에 요청한다. 그 페이지를 대상으로 잡지 못하면 다른 surface로 보내지 않고
/// 처리하지 못한 키로 넘긴다.
fn native_refresh_outcome(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    refresh: &[String],
    ev: &crate::webview::WebViewKeyEvent,
) -> Option<ForwardedKeyOutcome> {
    if !tasty_key_match::matches_any_binding(refresh, &ev.key, ev.mods) {
        return None;
    }
    let requested = crate::adapters::ui::input::shortcuts::request_surface_refresh(
        state,
        engine,
        ev.surface_id,
        crate::intent::RefreshTrigger::Shortcut,
    );
    tracing::debug!(
        target: "tasty::refresh",
        "native webview refresh key for surface {}: {}",
        ev.surface_id,
        if requested { "requested" } else { "not handled: the page cannot be targeted" }
    );
    Some(ForwardedKeyOutcome::from_handled(requested))
}

#[cfg(all(test, target_os = "linux"))]
#[path = "webview_keys_state_tests.rs"]
mod state_tests;

#[cfg(test)]
mod tests {
    use super::webview_poll_needed;

    /// 소비 순서 시험의 대역. 첫 키(명령 팔레트 토글)는 포커스를 주는 팝업 열기를 대기열에 넣고, 대기열 적용이 그
    /// 팝업을 연다. 팝업이 열려 있으면 다음 키는 입력 게이트에 막힌다(`Gated`). 열기가 포커스를 주지 않으면
    /// (`focusing` 거짓, 알림 목록 토글) 게이트를 닫지 않는다.
    struct Consumer {
        focusing: bool,
        queued: bool,
        overlay_open: bool,
        sidebar_toggled: bool,
        log: Vec<String>,
        finished: Vec<(u64, bool)>,
    }

    impl Consumer {
        fn new(focusing: bool) -> Self {
            Self {
                focusing,
                queued: false,
                overlay_open: false,
                sidebar_toggled: false,
                log: Vec::new(),
                finished: Vec::new(),
            }
        }
    }

    impl super::ForwardedKeyConsumer for Consumer {
        fn dispatch(
            &mut self,
            _id: winit::window::WindowId,
            ev: crate::webview::WebViewKeyEvent,
        ) -> super::ForwardedKeyOutcome {
            self.log.push(format!("dispatch {}", ev.id));
            if self.overlay_open {
                return super::ForwardedKeyOutcome::Gated;
            }
            if ev.id == 1 {
                self.queued = true;
            } else {
                self.sidebar_toggled = true;
            }
            super::ForwardedKeyOutcome::Handled
        }

        fn apply_focus_opens(&mut self, _id: winit::window::WindowId) {
            self.log.push("apply".into());
            if self.queued && self.focusing {
                self.overlay_open = true;
            }
            self.queued = false;
        }

        fn finish(
            &mut self,
            _id: winit::window::WindowId,
            _surface_id: u32,
            key_id: u64,
            outcome: super::ForwardedKeyOutcome,
        ) {
            self.log.push(format!("finish {key_id}"));
            self.finished
                .push((key_id, outcome == super::ForwardedKeyOutcome::NotHandled));
        }
    }

    fn batch() -> Vec<(
        winit::window::WindowId,
        Vec<crate::webview::WebViewKeyEvent>,
    )> {
        use winit::keyboard::{Key, ModifiersState};
        let key = |id, ch: &str| crate::webview::WebViewKeyEvent {
            id,
            surface_id: 7,
            key: Key::Character(ch.into()),
            mods: ModifiersState::CONTROL,
        };
        vec![(
            winit::window::WindowId::from(1),
            vec![key(1, "P"), key(2, "b")],
        )]
    }

    /// Given 한 번의 수집에 [명령 팔레트를 여는 키, sidebar 접기 키]가 들었을 때, When 소비하면, Then 키마다 처리 →
    /// 포커스 열기 적용 → 결과 알림 순서로 돌아, 둘째 키는 열린 팔레트의 게이트에 막히고(`Gated`) sidebar 가 바뀌지
    /// 않으며 페이지로 다시 넘기지 않는다.
    #[test]
    fn a_focused_popup_opened_by_a_forwarded_key_gates_the_next_key_of_the_batch() {
        let mut consumer = Consumer::new(true);
        super::consume_forwarded_keys(&mut consumer, batch());
        assert_eq!(
            consumer.log,
            [
                "dispatch 1",
                "apply",
                "finish 1",
                "dispatch 2",
                "apply",
                "finish 2"
            ]
        );
        assert!(!consumer.sidebar_toggled, "둘째 키가 sidebar 를 바꿨다");
        assert_eq!(consumer.finished, [(1, false), (2, false)]);
    }

    /// 대조: 첫 키의 열기가 포커스를 주지 않으면(알림 목록 토글) 둘째 키는 그대로 처리된다.
    #[test]
    fn a_passive_open_by_a_forwarded_key_leaves_the_next_key_alone() {
        let mut consumer = Consumer::new(false);
        super::consume_forwarded_keys(&mut consumer, batch());
        assert!(consumer.sidebar_toggled);
        assert_eq!(consumer.finished, [(1, false), (2, false)]);
    }

    /// 자식 WebView가 포커스를 쥐어 winit이 창을 비활성으로 알려도 폴링한다.
    #[test]
    fn a_webview_holding_the_focus_keeps_the_poll_running() {
        assert!(webview_poll_needed(true, false, false, || true));
        assert!(webview_poll_needed(true, false, true, || false));
        assert!(!webview_poll_needed(true, false, false, || false));
    }

    /// 보이는 WebView가 없거나 최소화됐으면 OS에 포커스를 묻지 않고 폴링하지 않는다.
    #[test]
    fn hidden_or_minimized_windows_do_not_poll_or_ask_the_os() {
        let asked = std::cell::Cell::new(false);
        let ask = || {
            asked.set(true);
            true
        };
        assert!(!webview_poll_needed(false, false, false, ask));
        assert!(!webview_poll_needed(true, true, false, ask));
        assert!(!asked.get());
    }

    /// modal 창·overlay·전체화면 무대 가운데 하나라도 열려 있으면 넘어온 키를 처리하지 않는다.
    #[test]
    fn forwarded_keys_wait_for_every_gate() {
        assert!(super::forwarded_key_may_dispatch(false, false, false));
        assert!(!super::forwarded_key_may_dispatch(true, false, false));
        assert!(!super::forwarded_key_may_dispatch(false, true, false));
        assert!(!super::forwarded_key_may_dispatch(false, false, true));
    }

    /// 단축키가 동작했으면 넘기지 않고, 동작하지 않았으면 엔진·페이지로 넘긴다.
    #[test]
    fn the_handler_result_decides_whether_the_key_is_passed_on() {
        use super::ForwardedKeyOutcome as O;
        assert_eq!(O::from_handled(true), O::Handled);
        assert_eq!(O::from_handled(false), O::NotHandled);
    }

    /// 키를 받은 페이지를 새로고침 대상으로 잡으면 처리한 키, 잡지 못하면 처리하지 못한 키다.
    /// 새로고침 바인딩이 아닌 키는 이 경로가 판정하지 않는다.
    #[test]
    fn a_refresh_key_for_a_page_that_cannot_be_targeted_is_not_handled() {
        use super::ForwardedKeyOutcome as O;
        use tasty_core::{DomainEvent as E, SurfaceSpec};
        use winit::keyboard::{Key, ModifiersState};
        let events = vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "w".into(),
                category: 0,
                index: 0,
                pane: 1,
            },
            E::TabCreated {
                id: 11,
                pane: 1,
                index: 0,
                name: "markdown".into(),
                surface: SurfaceSpec {
                    id: 11,
                    kind: "markdown".into(),
                    data: None,
                },
            },
        ];
        let (mut state, mut session) =
            crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
        let engine = session.borrow_mut();
        let refresh = vec!["f5".to_string()];
        let key = |surface_id, key: &str| crate::webview::WebViewKeyEvent {
            id: 0,
            surface_id,
            key: Key::Named(match key {
                "F5" => winit::keyboard::NamedKey::F5,
                _ => winit::keyboard::NamedKey::F6,
            }),
            mods: ModifiersState::empty(),
        };
        let outcome =
            |state: &mut _, ev| super::native_refresh_outcome(state, &engine.read(), &refresh, &ev);
        assert_eq!(outcome(&mut state, key(11, "F5")), Some(O::Handled));
        assert_eq!(outcome(&mut state, key(99, "F5")), Some(O::NotHandled));
        assert_eq!(outcome(&mut state, key(11, "F6")), None);
    }
}
