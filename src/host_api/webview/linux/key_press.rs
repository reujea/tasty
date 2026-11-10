//! native WebView의 키 누름을 host 정책으로 판정한다. 편집 바인딩은 입력 순서 경로로, host 단축키는 host 큐로
//! 보내고 나머지는 WebKit 기본 처리에 둔다. 계약: `docs/design/systems/webview.md`.

use std::rc::Rc;

use gtk::prelude::*;
use webkit2gtk::WebView;

use super::super::keys::WebViewKeySink;
use super::key_replay::KeyReplay;
use super::{edit, selection};
use super::{gdk_keyval_to_winit_key, gdk_state_to_winit_mods, x11_keycode_to_physical};

pub(super) fn connect(
    webview: &WebView,
    surface_id: u32,
    bridge: Rc<dyn WebViewKeySink>,
    edit_guard: edit::EditGuard,
    key_replay: Rc<KeyReplay>,
) {
    webview.connect_key_press_event(move |wv, ev| {
        // WebKit 기본 처리 전에 Super 계열을 DOM metaKey로 보이게 한다. UI 분류에는 영향이 없다.
        selection::add_meta_for_webkit(ev);
        // host가 처리하지 못해 다시 넣은 키와 페이지가 처리하지 않아 WebKit이 다시 보낸 키는 판정하지 않고
        // 엔진·페이지로 보낸다.
        if key_replay.is_judged(ev) {
            return gtk::glib::Propagation::Proceed;
        }
        key_replay.note_judged(ev);
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
            // 응답 전에 host 모달·overlay가 열리면 실행하지 않도록 같은 키로 다시 묻는다.
            let gate = bridge.clone();
            let blocked: edit::Blocked =
                Box::new(move || gate.blocks_edit_key(&key, &physical, mods));
            let arm_key = selection::ArmKey::from_gdk(ev.keyval(), ev.state());
            let (propagation, echo_arm) =
                edit::run_key_action(wv, action, arm_key, &edit_guard, blocked);
            if let Some(arm) = echo_arm {
                // 범위 선택이 없는 Copy·Cut은 확장이 엔진·페이지로 넘기고, 처리되지 않으면 WebKit이 다시 보낸다.
                // 그 반송이 다시 무장하지 않게 한다. 확장이 범위가 있어 처리했다고 답하면 표지를 지운다.
                key_replay.note_passed_edit(ev, arm);
            }
            return propagation;
        }
        // 바인딩에서 빠진 조합은 확장이 더 소유하지 않게 한다. 이 메시지는 키보다 먼저 도착한다.
        if newer
            && let Some(arm_key) = selection::ArmKey::from_gdk(ev.keyval(), ev.state())
            && edit_guard.unclaim(arm_key)
        {
            selection::unclaim(wv, arm_key);
        }
        if let Some(id) = bridge.forward_key(surface_id, key, physical, mods) {
            key_replay.note_forwarded(id, ev);
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
