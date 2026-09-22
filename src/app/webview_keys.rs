//! native webview 가 올린 키/포커스 이벤트의 host 측 소비 경로.
//!
//! webview 는 세 OS 모두 winit 창과 별개의 OS 자식 창/뷰라, 그 자식이 키보드 포커스를
//! 잡으면 winit `WindowEvent::KeyboardInput` 이 아예 오지 않는다. 백엔드가
//! `WebViewKeyBridge` 로 올린 키를 여기서 매 프레임 비워 디스패치한다.
//!
//! winit 키 경로(`view::main::keyboard`)의 **전 단계를 재현하지는 않는다** — 백엔드가
//! 이미 "host 정책이 가져갈 키" 만 골라 올리므로 double-tap 감지·Escape 소비·터미널
//! 포워딩 같은 앞뒤 단계는 여기 없다. 여기서 맞추는 것은 두 가지다: 소비 **순서**
//! (plugin 명령 단축키 → host 단축키)와 소비 직후 **후처리**(`after_shortcut_consumed`
//! 를 그대로 공유 — vi copy-mode 진입, modifier-hint 타이머, IME preedit flush/clear).
//! 결정 배경: `docs/adr/0102-webview-key-forwarding.md`.

use crate::app::App;
use crate::view::ui::View;

impl App {
    /// 한 프레임 분의 webview 키/포커스 이벤트를 소비한다. 반환값은 "Linux 키 폴링
    /// tick 을 세워야 하는가" — 호출자가 tick 재예약에 쓴다.
    ///
    /// 폴링이 필요한 조건은 **드러난 webview 가 있고 그 창이 활성**일 때다. 숨겨졌거나
    /// 창이 최소화/비활성이면 키가 webview 로 갈 수 없어 폴링이 순수 낭비고, 배경
    /// 인스턴스가 16ms 마다 깰 이유도 없다.
    ///
    /// Linux 는 webview 가 GDK 자기 X 연결로 이벤트를 받아 winit 루프를 깨우지
    /// 못하므로, 먼저 GTK 를 non-blocking 으로 펌프해 백엔드 시그널을 발화시킨다.
    /// macOS/Windows 는 native 키 콜백이 winit 과 같은 OS 이벤트 루프에서 발화하므로
    /// 별도 펌프도, 폴링 tick 도 필요 없다(`timers::reschedule_webview_key_poll` 이 그
    /// 판정을 갖는다 — 두 OS 에서는 조건과 무관하게 tick 을 세우지 않는다).
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
            if main.webview_any_visible
                && main.base.focused
                && main.base.winit.is_minimized() != Some(true)
            {
                needs_poll = true;
            }
        }
        if !any_webview {
            return false;
        }

        #[cfg(target_os = "linux")]
        crate::system_tray::pump_gtk_events();

        // (window_id, 이번 프레임에 올라온 키/포커스) 를 먼저 모은다 — 아래 디스패치가
        // `self` 를 가변 대여하므로 view 순회와 겹치면 안 된다.
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

        // 포커스 동기화가 먼저다 — 클릭으로 옮겨진 surface 를 대상으로 단축키가
        // 실행되어야 한다(클릭은 winit 에 도달하지 않아 `try_click_to_activate` 가
        // 실행되지 않는다).
        for (id, surfaces) in focus_batch {
            for sid in surfaces {
                self.focus_surface_from_webview(id, sid);
            }
        }

        for (id, keys) in key_batch {
            for ev in keys {
                self.dispatch_forwarded_webview_key(id, ev);
            }
        }
        needs_poll
    }

    /// native webview 클릭으로 관측된 포커스를 모델(`focused_pane`/`focused_surface`)에
    /// 반영한다. 이미 그 surface 가 포커스면 아무 것도 하지 않는다.
    fn focus_surface_from_webview(&mut self, id: winit::window::WindowId, surface_id: u32) {
        let Some(main) = self.view.views.get_mut(&id).and_then(|w| w.as_main_mut()) else {
            return;
        };
        if main
            .state
            .focus_surface_by_id(&mut main.core_state, surface_id)
        {
            main.mark_dirty();
        }
    }

    /// 포워딩된 키 1건을 디스패치한다. 소비 순서는 winit 경로와 같다 — plugin 명령
    /// 단축키를 먼저 보고, 소비되지 않으면 host 단축키로 넘긴다.
    ///
    /// 모델 포커스는 여기서 건드리지 않는다. 포커스 이동은 **클릭**(백엔드의
    /// `note_focus`)이라는 명시적 사용자 조작에만 붙인다 — X11 은 포인터가 자식 창
    /// 위에 있기만 해도 키를 그 창에 넣으므로, 키 도착을 포커스 이동 근거로 삼으면
    /// tasty 에 없는 focus-follows-mouse 가 생긴다.
    fn dispatch_forwarded_webview_key(
        &mut self,
        id: winit::window::WindowId,
        ev: crate::webview::WebViewKeyEvent,
    ) {
        if self.dispatch_plugin_shortcut_key(id, &ev.key, ev.mods) {
            return;
        }
        let Some(main) = self.view.views.get_mut(&id).and_then(|w| w.as_main_mut()) else {
            return;
        };
        // 큐에 오른 뒤 그 surface 가 닫혔으면 버린다 — 백엔드 콜백과 host drain 사이에
        // 프레임 경계가 끼는 정상적인 레이스다(`notify_navigation_attempt` 와 같은 성질).
        if !main.webviews.contains_key(&ev.surface_id) {
            return;
        }
        // overlay/무대가 키를 가져갈 상태면 포워딩된 키도 host 단축키로 쓰지 않는다
        // (winit 경로 `keyboard.rs` 의 게이트와 같은 조건). 이때 webview 는 이미
        // 숨겨져 있어 실제로는 거의 도달하지 않는 방어선이다.
        if main.state.keyboard_overlay_open() || main.state.fullscreen_stage_active() {
            return;
        }
        if main.handle_shortcut(&ev.key, ev.mods) {
            // 후처리는 winit 경로와 동일 함수를 쓴다(`mark_dirty` 포함).
            main.after_shortcut_consumed();
        }
        // 그 단축키가 마지막 워크스페이스를 닫았을 수 있다(`close_workspace` 등). 여기는
        // 창 이벤트 dispatch 밖이라, 지금 치우지 않으면 워크스페이스가 빈 창이 다음
        // `RedrawRequested` 를 받는다(`App::close_self_requesting_windows`).
        self.close_self_requesting_windows();
    }
}
