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

        for (id, keys) in key_batch {
            for ev in keys {
                let (key_id, surface_id) = (ev.id, ev.surface_id);
                let outcome = self.dispatch_forwarded_webview_key(id, ev);
                self.finish_forwarded_webview_key(id, surface_id, key_id, outcome);
            }
        }
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

    /// 처리 결과를 키를 가져간 WebView에 알린다. Linux는 Tasty가 처리하지 못한 키를 그 WebView에 다시 넣어
    /// 엔진·페이지로 보낸다. 입력 게이트로 막은 키는 다시 넣지 않는다. 다른 OS는 키를 가져갈 때 이미 소비했다.
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))] // reason: 다른 OS는 가져갈 때 키를 소비해 되돌릴 대상이 없다
    fn finish_forwarded_webview_key(
        &mut self,
        id: winit::window::WindowId,
        surface_id: u32,
        key_id: u64,
        outcome: ForwardedKeyOutcome,
    ) {
        #[cfg(target_os = "linux")]
        if let Some(webview) = self
            .view
            .views
            .get(&id)
            .and_then(|w| w.as_main())
            .and_then(|main| main.webviews.get(&surface_id))
        {
            webview.finish_forwarded_key(key_id, outcome == ForwardedKeyOutcome::NotHandled);
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
        let gates = engines_mut!(self).window_pair(id).map(|(main, _)| {
            (
                main.state.keyboard_overlay_open(),
                main.state.fullscreen_stage_active(),
            )
        });
        let (overlay_open, fullscreen) = gates.unwrap_or((false, false));
        if !forwarded_key_may_dispatch(self.view.is_modal_active(), overlay_open, fullscreen) {
            return ForwardedKeyOutcome::Gated;
        }
        if self.dispatch_plugin_shortcut_key(id, &ev.key, ev.mods) {
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
        let handled = main.handle_shortcut(&engine.read(), &ev.key, ev.mods);
        if handled {
            main.after_shortcut_consumed(&engine.read());
        }
        // 호스트 단축키가 마지막 workspace를 닫았을 수 있어 다음 redraw 전에 닫기 요청을 처리한다.
        self.close_self_requesting_windows();
        ForwardedKeyOutcome::from_handled(handled)
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

#[cfg(test)]
mod tests {
    use super::webview_poll_needed;

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
