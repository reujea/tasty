//! host가 가져갔지만 Tasty 단축키가 동작하지 않은 키를 WKWebView에 다시 넘긴다(macOS).
//!
//! `KeyWebView`가 가져간 `NSEvent`를 host 큐 이벤트와 같은 일련번호로 붙잡았다가, 결과가 오면 그 뷰의 super
//! 구현(`performKeyEquivalent:`로 받은 키는 `performKeyEquivalent:` 뒤 처리되지 않았으면 `keyDown:`, 나머지는
//! `keyDown:`)으로 넘긴다. 메뉴와 다른 뷰는 다시 판정하지 않는다. 결과 전에 온 자동 반복과 `keyUp:`도 함께 붙잡는다.
//!
//! WebKit은 페이지가 처리하지 않은 key-down을 같은 `NSEvent`로 앱에 다시 보낸다. 다시 넘긴 이벤트가 그렇게 돌아와
//! 다시 판정되면 같은 키를 또 가져가므로, 다시 넘긴 이벤트(시각·keyCode)를 짧은 시간 기억해 판정하지 않는다.
//! 표지는 각 이벤트를 super에 넘기기 직전에 남기고, 개수 상한은 결과 전에 붙잡을 수 있는 이벤트 수와 같다. 그래서
//! 한 번에 넘기는 이벤트의 표지가 넘기기 전에 밀려나지 않는다.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::{DefinedClass, Message, msg_send};
use objc2_app_kit::NSEvent;

use super::keyboard::{macos_keycode_to_physical, ns_chars_to_winit_key, nsevent_mods_to_winit};
use super::{KeyWebView, run_editing_command};
use crate::host_api::webview::key_hold::{HELD_EVENT_LIMIT, KeyHold, RepeatRoute, Settled};

/// 다시 넘긴 이벤트가 WebKit 반송으로 돌아올 수 있는 시간.
const ECHO_LIMIT: Duration = Duration::from_secs(5);
/// 반송 표지의 상한. 한 결과로 넘길 수 있는 이벤트보다 많고, 결과 전에 붙잡을 수 있는 이벤트 수와 같다.
const ECHO_COUNT_LIMIT: usize = HELD_EVENT_LIMIT;

/// 다시 넘긴 이벤트의 (시각, keyCode)와 넘긴 때.
#[derive(Default)]
struct EchoMarks(VecDeque<(f64, u16, Instant)>);

impl EchoMarks {
    fn note(&mut self, stamp: f64, code: u16, now: Instant) {
        self.0.push_back((stamp, code, now));
        if self.0.len() > ECHO_COUNT_LIMIT {
            self.0.pop_front();
        }
    }

    /// 같은 이벤트가 performKeyEquivalent:와 keyDown:로 두 번 올 수 있어 표지를 지우지 않고 시간으로만 버린다.
    fn contains(&mut self, stamp: f64, code: u16, now: Instant) -> bool {
        self.0
            .retain(|(_, _, at)| now.saturating_duration_since(*at) <= ECHO_LIMIT);
        self.0.iter().any(|(t, c, _)| *t == stamp && *c == code)
    }
}

/// key-down을 받은 입구.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    KeyEquivalent,
    KeyDown,
    KeyUp,
}

/// 붙잡은 원래 이벤트.
pub(super) struct HeldEvent {
    pub event: Retained<NSEvent>,
    pub entry: Entry,
}

/// WebView 하나의 붙잡은 키와 반송 표지. main thread에서만 쓴다.
#[derive(Default)]
pub(super) struct KeyReplay {
    hold: RefCell<KeyHold<u16, HeldEvent>>,
    /// 다시 넘기는 중이다. super 호출 안에서 다시 들어온 키는 판정하지 않는다.
    replaying: Cell<bool>,
    echoes: RefCell<EchoMarks>,
}

impl KeyReplay {
    /// 다시 넘기는 중이거나 다시 넘긴 이벤트의 반송이면 true다. backend는 판정 없이 super에 맡긴다.
    pub(super) fn is_replayed(&self, event: &NSEvent) -> bool {
        if self.replaying.get() {
            return true;
        }
        self.echoes
            .borrow_mut()
            .contains(event.timestamp(), event.keyCode(), Instant::now())
    }

    /// 그 자리에서 정한 첫 누름. 반복이 이 결과를 따른다.
    pub(super) fn decide(&self, code: u16, pass: bool) {
        self.hold.borrow_mut().decide(code, pass);
    }

    pub(super) fn forwarded(&self, id: u64, code: u16, held: HeldEvent) {
        if !self.hold.borrow_mut().forwarded(id, code, held) {
            tracing::warn!("webview: too many keys waiting for a result; dropped the oldest");
        }
    }

    pub(super) fn repeat(&self, code: u16, held: HeldEvent) -> RepeatRoute<HeldEvent> {
        self.hold.borrow_mut().repeat(code, held)
    }

    /// `keyUp:`을 붙잡았으면 true다.
    pub(super) fn release(&self, code: u16, held: HeldEvent) -> bool {
        self.hold.borrow_mut().release(code, held).is_none()
    }

    pub(super) fn clear_decisions(&self) {
        self.hold.borrow_mut().clear_decisions();
    }

    /// 일련번호 `id`의 결과를 적용한다. 넘길 이벤트를 `send`로 순서대로 super에 보낸다.
    pub(super) fn finish(&self, id: u64, pass: bool, mut send: impl FnMut(&HeldEvent)) {
        let Some(Settled { replay, release }) = self.hold.borrow_mut().finish(id, pass) else {
            return;
        };
        self.replaying.set(true);
        for held in &replay {
            self.echoes.borrow_mut().note(
                held.event.timestamp(),
                held.event.keyCode(),
                Instant::now(),
            );
            send(held);
        }
        if let Some(held) = &release {
            send(held);
        }
        self.replaying.set(false);
    }
}

impl KeyWebView {
    /// 키를 host가 판정한다. 소비했으면 true이고 super에 넘기지 않는다. 가져간 키는 결과가 올 때까지 붙잡고,
    /// 자동 반복은 첫 누름을 가져간 기록이 있으면 그 결과를 따른다(`key_replay`).
    pub(super) fn forward_key_to_host(&self, event: &NSEvent, entry: Entry) -> bool {
        let ivars = self.ivars();
        if ivars.replay.is_replayed(event) {
            return false;
        }
        let mods = nsevent_mods_to_winit(event.modifierFlags());
        let chars = event.charactersIgnoringModifiers();
        let Some(key) = chars.and_then(|c| ns_chars_to_winit_key(&c.to_string())) else {
            return false;
        };
        let code = event.keyCode();
        let physical = macos_keycode_to_physical(code);
        let held = HeldEvent {
            event: event.retain(),
            entry,
        };
        // 자동 반복은 다시 실행하지 않는다. 첫 누름의 결과(그 자리에서 정했든 host 결과든)를 따르고, 기록이
        // 없으면 첫 누름을 막거나 가져갔을 조합인지로 정한다.
        if event.isARepeat() {
            return match ivars.replay.repeat(code, held) {
                RepeatRoute::Held | RepeatRoute::Drop => true,
                RepeatRoute::Pass(_) => false,
                RepeatRoute::Unknown(_) => {
                    ivars
                        .key_bridge
                        .consumes_repeat(ivars.surface_id, &key, &physical, mods)
                }
            };
        }
        if ivars.key_bridge.blocks_edit_key(&key, &physical, mods) {
            ivars.replay.decide(code, false);
            return true;
        }
        // Edit 메뉴가 없어 Cmd 편집 키(실행 취소 포함)도 사용자 바인딩으로 이 WebView의 편집 명령을 실행한다.
        if let Some(action) = ivars.key_bridge.edit_action(&key, &physical, mods) {
            ivars.replay.decide(code, false);
            run_editing_command(self, action);
            return true;
        }
        match ivars
            .key_bridge
            .forward_key(ivars.surface_id, key, physical, mods)
        {
            Some(id) => {
                ivars.replay.forwarded(id, code, held);
                true
            }
            None => {
                ivars.replay.decide(code, true);
                false
            }
        }
    }

    /// host가 일련번호 `id`의 키를 처리했다. 동작하지 않았으면 붙잡은 key-down을 이 뷰의 super 구현에 넘기고,
    /// 붙잡은 key-up은 결과와 관계없이 그 뒤에 넘긴다.
    pub(super) fn finish_forwarded_key(&self, id: u64, pass: bool) {
        self.ivars().replay.finish(id, pass, |held| {
            let event = &*held.event;
            match held.entry {
                Entry::KeyEquivalent => {
                    // SAFETY: main thread AppKit. 이 뷰의 super 구현에 원래 이벤트를 넘긴다.
                    let handled: bool =
                        unsafe { msg_send![super(self), performKeyEquivalent: event] };
                    if !handled {
                        // SAFETY: 위와 같다.
                        unsafe { msg_send![super(self), keyDown: event] }
                    }
                }
                // SAFETY: 위와 같다.
                Entry::KeyDown => unsafe { msg_send![super(self), keyDown: event] },
                // SAFETY: 위와 같다.
                Entry::KeyUp => unsafe { msg_send![super(self), keyUp: event] },
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Given 결과 전에 붙잡을 수 있는 만큼의 이벤트를 넘긴다, When 그 반송이 온다,
    /// Then 처음 넘긴 이벤트도 표지로 알아본다. 5초가 지난 표지는 버린다.
    #[test]
    fn echo_marks_keep_every_event_a_result_can_replay() {
        let mut marks = EchoMarks::default();
        let start = Instant::now();
        for i in 0..ECHO_COUNT_LIMIT {
            marks.note(i as f64, 7, start);
        }
        assert!(marks.contains(0.0, 7, start));
        assert!(marks.contains((ECHO_COUNT_LIMIT - 1) as f64, 7, start));
        assert!(!marks.contains(0.0, 8, start));
        assert!(!marks.contains(0.0, 7, start + ECHO_LIMIT + Duration::from_millis(1)));
    }
}
