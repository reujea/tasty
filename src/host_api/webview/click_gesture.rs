//! 엔진이 사용자 제스처 값을 주지 않는 탐색의 사용자 입력 증명. 이 뷰가 받은 OS 입력으로 대신 증명한다.
//! - macOS: WKNavigationAction 공개 API 에 Linux `is_user_gesture`·Windows `IsUserInitiated` 에 해당하는
//!   값이 없다. NSResponder 의 `mouseDown:`·`mouseUp:` 이 클릭을 기록한다. 키 입력은 기록하지 않는다.
//! - Windows: 같은 문서 fragment 이동은 `HistoryChanged` 로만 알 수 있고 거기에 `IsUserInitiated` 가
//!   없다. host 입력 경로의 메시지 훅이 WebView 로 가는 왼쪽 버튼 메시지와, 페이지로 통과시키는
//!   Enter·Space 메시지를 기록한다.
//!
//! 페이지 스크립트가 만든 이벤트는 이 OS 경로를 지나지 않으므로 입력으로 세지 않는다.
//! 규칙은 docs/design/systems/webview.md "탐색" 절에 있다.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// 입력 뒤 이 시간 안에 시작한 main frame 탐색만 그 입력이 낸 것으로 본다. WebKit 이 사용자 제스처를
/// 타이머로 넘겨주는 상한(`UserGestureToken::maximumIntervalForUserGestureForwarding`, 1초)과 같다.
pub(crate) const GESTURE_WINDOW: Duration = Duration::from_secs(1);

#[cfg(any(windows, test))]
const VK_RETURN: u32 = 0x0D;
#[cfg(any(windows, test))]
const VK_SPACE: u32 = 0x20;

/// 증명을 만든 입력. 뗌은 자기 누름의 시각만 옮긴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Pointer,
    /// `held`는 이 키의 뗌을 아직 보지 않았다는 뜻이다. 한 누름의 뗌은 한 번만 시각을 옮긴다.
    #[cfg(any(windows, test))]
    Key {
        vk: u32,
        held: bool,
    },
}

/// 소비되기 전에 새 누름이 온 키 누름을 몇 개까지 기억하는지. 넘치면 오래된 것부터 버린다.
#[cfg(any(windows, test))]
const EARLIER_KEY_PRESSES: usize = 8;

/// 마지막 OS 입력. 한 입력은 main frame 탐색 하나만 증명한다.
#[derive(Debug, Default)]
pub(crate) struct ClickGesture {
    armed: Cell<Option<(Instant, Source)>>,
    /// 탐색 통지보다 다음 키 누름이 먼저 온 앞선 누름의 시각. 연타의 각 누름은 탐색을 하나씩 낸다.
    /// 칸이 하나면 뒤 탐색이 거짓이 되어 앞 탐색의 증명 기록을 지운다.
    #[cfg(any(windows, test))]
    earlier_keys: Cell<[Option<Instant>; EARLIER_KEY_PRESSES]>,
}

impl ClickGesture {
    /// OS 가 이 뷰에 왼쪽 버튼 누름을 보냈다.
    pub(crate) fn press(&self, now: Instant) {
        self.armed.set(Some((now, Source::Pointer)));
    }

    /// 누름에 이어 버튼을 뗐다. 페이지의 click 이벤트는 뗄 때 생기므로 시각을 뗀 때로 옮긴다.
    /// 버튼 누름 없이 온 뗌은 증명하지 않는다.
    pub(crate) fn release(&self, now: Instant) {
        if let Some((_, Source::Pointer)) = self.armed.get() {
            self.armed.set(Some((now, Source::Pointer)));
        }
    }

    /// host 가 페이지로 통과시키는 활성화 키의 누름([`proves_activation`]). 자동 반복도 같다.
    /// Chromium 은 Enter 반복마다 버튼을 누르므로, 반복을 빼면 반복이 낸 거짓 탐색이 첫 누름의 기록을 지운다.
    #[cfg(any(windows, test))]
    pub(crate) fn key_press(&self, vk: u32, now: Instant) {
        if let Some((at, Source::Key { vk: down, held })) = self.armed.get()
            && !(held && down == vk && vk == VK_SPACE)
        {
            // Space 자동 반복은 뗄 때 click 하나만 내므로 쌓지 않는다.
            self.remember_earlier_key(at);
        }
        self.armed.set(Some((now, Source::Key { vk, held: true })));
    }

    #[cfg(any(windows, test))]
    fn remember_earlier_key(&self, at: Instant) {
        let mut earlier = self.earlier_keys.get();
        if let Some(slot) = earlier.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(at);
        } else {
            earlier.rotate_left(1);
            earlier[EARLIER_KEY_PRESSES - 1] = Some(at);
        }
        self.earlier_keys.set(earlier);
    }

    /// 시간 창 안의 앞선 키 누름 하나를 소비한다. 지나간 칸은 비운다.
    #[cfg(any(windows, test))]
    fn take_earlier_key(&self, now: Instant) -> bool {
        let mut earlier = self.earlier_keys.get();
        let found = earlier
            .iter_mut()
            .find_map(|slot| slot.take().filter(|at| within_window(now, *at)))
            .is_some();
        self.earlier_keys.set(earlier);
        found
    }

    /// 활성화 키를 뗐다. Space 는 뗄 때 click 이 생기므로 오래 눌렀다 떼도 증명되게 시각을 옮긴다.
    /// 이미 소비됐거나 다른 입력이 덮었거나 같은 누름의 뗌을 이미 봤으면 아무것도 하지 않는다.
    #[cfg(any(windows, test))]
    pub(crate) fn key_release(&self, vk: u32, now: Instant) {
        if self.armed.get().is_some_and(
            |(_, source)| matches!(source, Source::Key { vk: down, held: true } if down == vk),
        ) {
            self.armed.set(Some((now, Source::Key { vk, held: false })));
        }
    }

    /// 호스트(에이전트·IPC 포함)가 로드·이동을 시작했다. 그 탐색을 입력이 낸 것으로 보지 않는다.
    pub(crate) fn cancel(&self) {
        self.armed.set(None);
        #[cfg(any(windows, test))]
        self.earlier_keys.set(Default::default());
    }

    /// 탐색 결정 시점에 사용자 입력 증명을 꺼낸다. main frame 탐색만 입력을 소비한다.
    /// 시간 창을 넘긴 입력은 버린다.
    pub(crate) fn take(&self, now: Instant, main_frame: Option<bool>) -> bool {
        if main_frame != Some(true) {
            return false;
        }
        #[cfg(any(windows, test))]
        if self.take_earlier_key(now) {
            return true;
        }
        self.armed
            .take()
            .is_some_and(|(at, _)| within_window(now, at))
    }
}

fn within_window(now: Instant, at: Instant) -> bool {
    now.saturating_duration_since(at) <= GESTURE_WINDOW
}

/// 이 key-down 이 페이지의 버튼·링크를 활성화하는 사용자 입력인지. 수식키 조합은 활성화가 아니라
/// 단축키일 수 있어서, host 가 소비한 키는 페이지에 닿지 않아서 뺀다. IME 조합 키는 `VK_PROCESSKEY` 로
/// 와서 빠진다. Numpad Enter 도 `VK_RETURN` 이다.
#[cfg(any(windows, test))]
pub(crate) fn proves_activation(
    vk: u32,
    mods: winit::keyboard::ModifiersState,
    consumed_by_host: bool,
) -> bool {
    matches!(vk, VK_RETURN | VK_SPACE) && mods.is_empty() && !consumed_by_host
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn navigation_without_click_is_not_user_gesture() {
        let g = ClickGesture::default();
        assert!(!g.take(Instant::now(), Some(true)));
    }

    #[test]
    fn click_proves_one_main_frame_navigation() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(80));
        assert!(g.take(t + ms(120), Some(true)));
        assert!(
            !g.take(t + ms(130), Some(true)),
            "한 클릭은 한 번만 증명한다"
        );
    }

    #[test]
    fn subframe_or_unknown_frame_neither_proves_nor_consumes() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        assert!(!g.take(t + ms(10), Some(false)));
        assert!(!g.take(t + ms(10), None));
        assert!(g.take(t + ms(20), Some(true)));
    }

    #[test]
    fn expired_click_is_dropped() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(50));
        assert!(!g.take(t + ms(50) + GESTURE_WINDOW + ms(1), Some(true)));
        assert!(!g.take(t + ms(60) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn window_counts_from_release() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.release(t + ms(1500));
        assert!(g.take(t + ms(1500) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn release_without_press_does_not_arm() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.release(t);
        assert!(!g.take(t + ms(10), Some(true)));
    }

    #[test]
    fn host_load_cancels_pending_click() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.cancel();
        assert!(!g.take(t + ms(10), Some(true)));
    }

    const ENTER: u32 = VK_RETURN;
    const SPACE: u32 = VK_SPACE;

    #[test]
    fn a_key_press_proves_one_navigation() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        assert!(g.take(t + ms(5), Some(true)));
        assert!(!g.take(t + ms(6), Some(true)), "한 누름은 한 번만 증명한다");
    }

    #[test]
    fn each_quick_press_proves_its_own_navigation() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        g.key_press(ENTER, t + ms(5));
        assert!(g.take(t + ms(6), Some(true)), "앞 누름의 탐색");
        assert!(g.take(t + ms(12), Some(true)), "뒤 누름의 탐색");
        assert!(!g.take(t + ms(13), Some(true)), "남는 증명이 없다");

        // 서로 다른 키와 Space 의 눌렀다 뗌 두 번도 각각 하나다.
        let g = ClickGesture::default();
        g.key_press(SPACE, t);
        g.key_release(SPACE, t + ms(2));
        g.key_press(SPACE, t + ms(4));
        g.key_release(SPACE, t + ms(6));
        g.key_press(ENTER, t + ms(8));
        assert!(g.take(t + ms(9), Some(true)));
        assert!(g.take(t + ms(10), Some(true)));
        assert!(g.take(t + ms(11), Some(true)));
        assert!(!g.take(t + ms(12), Some(true)));
    }

    #[test]
    fn a_consumed_press_is_not_kept_for_the_next_press() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        assert!(g.take(t + ms(1), Some(true)));
        g.key_press(ENTER, t + ms(50));
        assert!(g.take(t + ms(51), Some(true)));
        assert!(!g.take(t + ms(52), Some(true)));
    }

    #[test]
    fn enter_repeats_stack_but_space_repeats_do_not() {
        let t = Instant::now();
        // Chromium 은 Enter 반복마다 버튼을 누른다.
        let g = ClickGesture::default();
        for i in 0..3 {
            g.key_press(ENTER, t + ms(i * 33));
        }
        for _ in 0..3 {
            assert!(g.take(t + ms(70), Some(true)));
        }
        assert!(!g.take(t + ms(70), Some(true)));

        // Space 는 뗄 때 click 하나만 낸다.
        let g = ClickGesture::default();
        for i in 0..3 {
            g.key_press(SPACE, t + ms(i * 33));
        }
        g.key_release(SPACE, t + ms(70));
        assert!(g.take(t + ms(71), Some(true)));
        assert!(!g.take(t + ms(72), Some(true)));
    }

    #[test]
    fn an_earlier_press_expires_on_its_own_time() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        g.key_press(ENTER, t + ms(900));
        let after_first = t + GESTURE_WINDOW + ms(100);
        assert!(g.take(after_first, Some(true)), "뒤 누름은 아직 창 안이다");
        assert!(!g.take(after_first, Some(true)), "앞 누름은 지났다");
    }

    #[test]
    fn remembered_key_presses_are_bounded() {
        let g = ClickGesture::default();
        let t = Instant::now();
        let presses = EARLIER_KEY_PRESSES as u64 + 3;
        for i in 0..presses {
            g.key_press(ENTER, t + ms(i));
        }
        for _ in 0..=EARLIER_KEY_PRESSES {
            assert!(g.take(t + ms(presses), Some(true)));
        }
        assert!(!g.take(t + ms(presses), Some(true)));
    }

    #[test]
    fn a_host_load_cancels_the_earlier_key_presses_too() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        g.key_press(ENTER, t + ms(1));
        g.cancel();
        assert!(!g.take(t + ms(2), Some(true)));
    }

    #[test]
    fn pointer_presses_keep_one_slot() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.press(t);
        g.press(t + ms(1));
        assert!(g.take(t + ms(2), Some(true)));
        assert!(!g.take(t + ms(3), Some(true)));
    }

    #[test]
    fn a_space_held_past_the_window_proves_from_its_release() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(SPACE, t);
        g.key_release(SPACE, t + ms(1500));
        assert!(g.take(t + ms(1500) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn the_release_of_a_consumed_key_does_not_prove_again() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(SPACE, t);
        assert!(g.take(t + ms(5), Some(true)));
        g.key_release(SPACE, t + ms(50));
        assert!(!g.take(t + ms(60), Some(true)));
    }

    #[test]
    fn a_key_release_only_answers_the_same_key_press() {
        let t = Instant::now();
        let late = t + ms(1500) + GESTURE_WINDOW;

        // 앞선 마우스 누름의 시각을 Space 뗌이 옮기지 않는다.
        let g = ClickGesture::default();
        g.press(t);
        g.key_release(SPACE, t + ms(1500));
        assert!(!g.take(late, Some(true)));

        // 다른 키의 뗌도 옮기지 않는다.
        let g = ClickGesture::default();
        g.key_press(ENTER, t);
        g.key_release(SPACE, t + ms(1500));
        assert!(!g.take(late, Some(true)));

        // 누름 없는 뗌은 증명을 만들지 않는다.
        let g = ClickGesture::default();
        g.key_release(SPACE, t);
        assert!(!g.take(t + ms(1), Some(true)));

        // 한 누름의 뗌은 한 번만 옮긴다(IME 조합 Enter 는 down 이 VK_PROCESSKEY, up 이 VK_RETURN 이다).
        let g = ClickGesture::default();
        g.key_press(ENTER, t);
        g.key_release(ENTER, t + ms(10));
        g.key_release(ENTER, t + ms(1500));
        assert!(!g.take(late, Some(true)));
    }

    #[test]
    fn a_pointer_release_does_not_move_a_key_proof() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        g.release(t + ms(1500));
        assert!(!g.take(t + ms(1500) + GESTURE_WINDOW, Some(true)));
    }

    #[test]
    fn a_host_load_cancels_a_pending_key_proof() {
        let g = ClickGesture::default();
        let t = Instant::now();
        g.key_press(ENTER, t);
        g.cancel();
        g.key_release(ENTER, t + ms(5));
        assert!(!g.take(t + ms(10), Some(true)));
    }

    #[test]
    fn only_unconsumed_plain_activation_keys_prove_a_navigation() {
        use winit::keyboard::ModifiersState as M;
        const VK_PROCESSKEY: u32 = 0xE5;
        const VK_A: u32 = 0x41;
        const VK_TAB: u32 = 0x09;
        assert!(proves_activation(ENTER, M::empty(), false));
        assert!(proves_activation(SPACE, M::empty(), false));
        assert!(!proves_activation(ENTER, M::empty(), true));
        for mods in [M::CONTROL, M::SHIFT, M::ALT, M::SUPER] {
            assert!(!proves_activation(ENTER, mods, false), "{mods:?}");
            assert!(!proves_activation(SPACE, mods, false), "{mods:?}");
        }
        assert!(!proves_activation(VK_PROCESSKEY, M::empty(), false));
        assert!(!proves_activation(VK_A, M::empty(), false));
        assert!(!proves_activation(VK_TAB, M::empty(), false));
    }
}
