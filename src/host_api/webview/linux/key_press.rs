//! native WebView의 키 누름을 host 정책으로 판정한다. 편집 바인딩은 입력 순서 경로로, host 단축키는 host 큐로
//! 보내고 나머지는 WebKit 기본 처리에 둔다. 계약: `docs/design/systems/webview.md`.

use std::rc::Rc;

use gtk::prelude::*;
use webkit2gtk::WebView;

use super::super::keys::WebViewKeySink;
use super::{edit, selection};
use super::{gdk_keyval_to_winit_key, gdk_state_to_winit_mods, x11_keycode_to_physical};

pub(super) fn connect(
    webview: &WebView,
    surface_id: u32,
    bridge: Rc<dyn WebViewKeySink>,
    edit_guard: edit::EditGuard,
) {
    webview.connect_key_press_event(move |wv, ev| {
        // WebKit 기본 처리 전에 Super 계열을 DOM metaKey로 보이게 한다. UI 분류에는 영향이 없다.
        selection::add_meta_for_webkit(ev);
        // 이 press 신호에는 repeat 플래그가 없어 반복 입력도 host로 전달한다.
        if ev.is_modifier() {
            return gtk::glib::Propagation::Proceed;
        }
        // host가 직접 시작한 선택 질의는 이 키 뒤에 실행하지 않는다.
        let newer = edit_guard.note_key(ev.time());
        let Some(key) = gdk_keyval_to_winit_key(ev.keyval()) else {
            return gtk::glib::Propagation::Proceed;
        };
        let physical = x11_keycode_to_physical(ev.hardware_keycode());
        let mods = gdk_state_to_winit_mods(ev.state());
        if bridge.blocks_edit_key(&key, &physical, mods) {
            return gtk::glib::Propagation::Stop;
        }
        // 사용자 편집 바인딩은 키를 받은 이 WebView에서 입력 순서대로 엔진 명령으로 실행한다.
        if let Some(action) = bridge.edit_action(&key, &physical, mods) {
            // 범위 선택이 없어 확장이 막지 않은 Copy·Cut을 페이지도 처리하지 않으면 WebKit이 원래 시각 그대로 UI에 다시
            // 보낸다. 이미 본 누름이므로 다시 무장하지 않고 WebKit 기본 처리에 둔다.
            if !newer {
                return gtk::glib::Propagation::Proceed;
            }
            // 응답 전에 host 모달·overlay가 열리면 실행하지 않도록 같은 키로 다시 묻는다.
            let gate = bridge.clone();
            let blocked: edit::Blocked =
                Box::new(move || gate.blocks_edit_key(&key, &physical, mods));
            let arm_key = selection::ArmKey::from_gdk(ev.keyval(), ev.state());
            return edit::run_key_action(wv, action, arm_key, &edit_guard, blocked);
        }
        // 바인딩에서 빠진 조합은 확장이 더 소유하지 않게 한다. 이 메시지는 키보다 먼저 도착한다.
        if newer
            && let Some(arm_key) = selection::ArmKey::from_gdk(ev.keyval(), ev.state())
            && edit_guard.unclaim(arm_key)
        {
            selection::unclaim(wv, arm_key);
        }
        if bridge.capture_key(surface_id, key, physical, mods) {
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    webview.connect_key_release_event(|_wv, ev| {
        selection::add_meta_for_webkit(ev);
        gtk::glib::Propagation::Proceed
    });
}
