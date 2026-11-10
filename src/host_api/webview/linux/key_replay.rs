//! host가 가져갔지만 처리하지 못한 키를 키를 받은 WebView에 다시 넣는다. Tasty 단축키가 동작하지 않았으면
//! 키를 엔진·페이지로 넘긴다는 원칙의 Linux 경로다(`docs/design/policies/key-mapping.md`).
//! GTK 키 콜백은 가져갈지를 동기로 정해야 하고 실행은 나중에 host 큐에서 일어나므로, 가져간 키의 사본을
//! host 큐 이벤트와 같은 일련번호로 두었다가 그 번호의 처리 결과를 받으면 버리거나 다시 넣는다.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gtk::gdk;
use gtk::prelude::*;

/// 처리 결과를 기다리는 키 사본의 상한. host 큐는 매 폴링에서 비워지므로 정상 입력은 여기에 닿지 않는다.
const FORWARDED_LIMIT: usize = 64;
/// 같은 X 서버 시각에 판정한 키를 따로 기억하는 상한. 넘친 뒤에는 그 시각의 모든 키를 판정한 키로 본다.
const SAME_TIME_LIMIT: usize = 16;
/// 반송으로 볼 수 있는 나이의 상한. WebKit 반송과 다시 넣은 키는 같은 GTK 루프에서 길어야 몇 초 안에 돌아온다.
/// 그보다 넉넉히 잡고, 이 시간이 지나면 X 서버 시각의 순환(약 49일)과 관계없이 새 누름으로 본다.
const ECHO_AGE_LIMIT: Duration = Duration::from_secs(300);
/// X 서버 시각(ms)으로 잰 같은 상한. 마지막 판정보다 이만큼 넘게 이른 시각은 반송이 아니라 순환한 새 누름이다.
const ECHO_AGE_LIMIT_MS: u32 = 300_000;
/// 시각 0인 편집 키의 반송을 기다리는 시간. 시각으로 순서를 알 수 없어 짧게 잡는다.
const ZERO_TIME_ECHO_LIMIT: Duration = Duration::from_secs(2);
/// 시각 0인 편집 키 표지의 상한. 넘치면 가장 오래된 표지를 버린다.
const ZERO_TIME_LIMIT: usize = 16;

/// 가져간 키 사본. host 큐 이벤트의 일련번호로 짝을 짓는다. 큐가 넘쳐 버린 사본이나 다른 WebView의
/// 번호는 짝이 없으므로 그 결과로 아무 키도 다시 넣지 않는다. 키를 두 번 실행하는 것보다 버리는 편이 안전하다.
pub(super) struct ForwardedCopies<T> {
    copies: VecDeque<(u64, T)>,
    limit: usize,
}

impl<T> ForwardedCopies<T> {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            copies: VecDeque::new(),
            limit,
        }
    }

    /// 넘치면 가장 오래된 사본을 버린다. 그 사본의 결과는 짝이 없어 무시된다.
    pub(super) fn push(&mut self, id: u64, copy: T) -> bool {
        self.copies.push_back((id, copy));
        if self.copies.len() > self.limit {
            self.copies.pop_front();
            return false;
        }
        true
    }

    /// `id`의 사본을 꺼낸다. host는 한 WebView의 키를 가져간 순서대로 처리하므로 그보다 앞선 사본은 결과가 오지
    /// 않는 사본이라 함께 버린다. 짝이 없으면 None이다.
    pub(super) fn take(&mut self, id: u64) -> Option<T> {
        while let Some((front, _)) = self.copies.front() {
            if *front > id {
                return None;
            }
            let (front, copy) = self.copies.pop_front()?;
            if front == id {
                return Some(copy);
            }
        }
        None
    }
}

/// 이미 판정한 키 누름. WebKit은 페이지가 처리하지 않은 누름을 원래 시각 그대로 UI에 다시 보내고, 다시 넣은
/// 키도 원래 시각을 유지한다. 새 누름은 이미 판정한 누름보다 시각이 늦으므로 마지막 판정 시각 이하의 누름은
/// 반송이나 재주입으로 본다. 다만 반송은 오래 걸리지 않으므로 마지막 판정 뒤 `ECHO_AGE_LIMIT`가 지났거나
/// 마지막 판정 시각보다 `ECHO_AGE_LIMIT_MS`를 넘게 이른 시각은 새 누름이다. 그렇지 않으면 X 서버 시각이 반 순환
/// 넘게 흐른 뒤의 새 누름을 반송으로 잘못 본다. 시각 0(CurrentTime)은 순서를 알 수 없어 따로 다룬다.
#[derive(Default)]
pub(super) struct JudgedKeys {
    last: Option<(u32, Instant)>,
    /// `last`와 같은 시각에 판정한 (keyval, keycode).
    same_time: Vec<(u32, u16)>,
    /// 같은 시각 목록이 넘쳤다. 그 시각의 키를 다시 판정해 두 번 실행하지 않도록 모두 판정한 키로 본다.
    same_time_full: bool,
    /// 범위 선택이 없으면 확장이 엔진·페이지로 넘기는 시각 0 편집 키(Copy·Cut)와 그 무장 번호. 그 반송이 다시
    /// 무장하지 않게 짧은 시간 동안 기억하고, 확장이 그 키를 처리했다고 답하면 지운다.
    zero_time: VecDeque<((u32, u16), u64, Instant)>,
}

impl JudgedKeys {
    /// 반송을 비교할 마지막 판정 시각. 나이 상한이 지났으면 없다.
    fn live_last(&self, now: Instant) -> Option<u32> {
        let (last, at) = self.last?;
        (now.saturating_duration_since(at) <= ECHO_AGE_LIMIT).then_some(last)
    }

    pub(super) fn contains(&mut self, time: u32, key: (u32, u16), now: Instant) -> bool {
        if time == 0 {
            return self.take_zero_time(key, now);
        }
        let Some(last) = self.live_last(now) else {
            return false;
        };
        if time == last {
            return self.same_time_full || self.same_time.contains(&key);
        }
        // 마지막 판정보다 이르고 그 차이가 반송이 될 수 있는 나이 안이면 반송이다.
        last.wrapping_sub(time) <= ECHO_AGE_LIMIT_MS
    }

    pub(super) fn note(&mut self, time: u32, key: (u32, u16), now: Instant) {
        if time == 0 {
            return;
        }
        let same = self.live_last(now) == Some(time);
        self.last = Some((time, now));
        if !same {
            self.same_time.clear();
            self.same_time_full = false;
        }
        if self.same_time.len() < SAME_TIME_LIMIT {
            self.same_time.push(key);
        } else {
            self.same_time_full = true;
        }
    }

    /// 무장 번호 `arm`으로 넘긴 시각 0 편집 키를 기억한다.
    pub(super) fn note_zero_time(&mut self, key: (u32, u16), arm: u64, now: Instant) {
        self.zero_time.push_back((key, arm, now));
        if self.zero_time.len() > ZERO_TIME_LIMIT {
            self.zero_time.pop_front();
        }
    }

    /// 기억한 시각 0 편집 키의 반송이면 그 표지를 꺼내고 true다. 한 번 넘긴 키의 반송은 한 번만 온다.
    fn take_zero_time(&mut self, key: (u32, u16), now: Instant) -> bool {
        self.zero_time
            .retain(|(_, _, at)| now.saturating_duration_since(*at) <= ZERO_TIME_ECHO_LIMIT);
        let Some(i) = self.zero_time.iter().position(|(k, _, _)| *k == key) else {
            return false;
        };
        self.zero_time.remove(i);
        true
    }

    /// 확장이 무장 번호 `arm`의 키를 처리해 기본 동작을 막았다. 그 키는 반송되지 않으므로 표지를 지운다.
    pub(super) fn settle_zero_time(&mut self, arm: u64) {
        self.zero_time.retain(|(_, a, _)| *a != arm);
    }
}

fn key_of(ev: &gdk::EventKey) -> (u32, u16) {
    (*ev.keyval(), ev.hardware_keycode())
}

pub(super) struct KeyReplay {
    forwarded: RefCell<ForwardedCopies<gdk::Event>>,
    judged: RefCell<JudgedKeys>,
    /// 다시 넣는 호출 안이다. 그 안에서 들어온 키는 판정하지 않는다.
    replaying: Cell<bool>,
}

impl Default for KeyReplay {
    fn default() -> Self {
        Self {
            forwarded: RefCell::new(ForwardedCopies::new(FORWARDED_LIMIT)),
            judged: RefCell::default(),
            replaying: Cell::new(false),
        }
    }
}

impl KeyReplay {
    /// 이미 판정한 키이면 true다. 다시 넣는 호출 안에서와, 페이지가 처리하지 않아 WebKit이 다시 보낸 경우 모두 해당한다.
    pub(super) fn is_judged(&self, ev: &gdk::EventKey) -> bool {
        self.replaying.get()
            || self
                .judged
                .borrow_mut()
                .contains(ev.time(), key_of(ev), Instant::now())
    }

    /// 판정을 시작한 키를 표시한다. 그 키가 WebKit에서 다시 와도 다시 판정하지 않는다.
    pub(super) fn note_judged(&self, ev: &gdk::EventKey) {
        self.judged
            .borrow_mut()
            .note(ev.time(), key_of(ev), Instant::now());
    }

    /// 무장 번호 `arm`으로 넘긴, 범위 선택이 없으면 엔진·페이지로 갈 수 있는 편집 키를 표시한다. 시각이 있는 키는
    /// `note_judged`로 충분하고, 시각 0인 키만 반송을 구분할 표지를 따로 둔다.
    pub(super) fn note_passed_edit(&self, ev: &gdk::EventKey, arm: u64) {
        if ev.time() == 0 {
            self.judged
                .borrow_mut()
                .note_zero_time(key_of(ev), arm, Instant::now());
        }
    }

    /// 확장이 무장 번호 `arm`의 키를 처리했다.
    pub(super) fn settle_edit(&self, arm: u64) {
        self.judged.borrow_mut().settle_zero_time(arm);
    }

    /// host 큐에 일련번호 `id`로 넣은 키의 사본을 둔다. 시각 0인 키는 사본을 두지 않아 host가 처리하지 못해도
    /// 다시 넣지 않고 소비한다. 다시 넣은 키를 WebKit이 반송하면 시각으로 반송임을 알 수 없어 다시 host로 가져가고,
    /// 같은 키가 되풀이될 수 있다. 되풀이보다 버리는 편을 택한다.
    pub(super) fn note_forwarded(&self, id: u64, ev: &gdk::EventKey) {
        if ev.time() == 0 {
            return;
        }
        if !self.forwarded.borrow_mut().push(id, (**ev).clone()) {
            tracing::warn!(
                "webview key replay queue overflowed; the oldest forwarded key will not be replayed"
            );
        }
    }

    /// host가 일련번호 `id`의 키를 처리했다. `replay`이면 그 키를 WebView에 다시 넣는다.
    pub(super) fn finish(&self, widget: &impl IsA<gtk::Widget>, id: u64, replay: bool) {
        let Some(event) = self.forwarded.borrow_mut().take(id) else {
            return;
        };
        if !replay {
            return;
        }
        self.replaying.set(true);
        // 키 콜백은 위 표시를 보고 host 판정 없이 WebKit 기본 처리로 넘긴다.
        widget.as_ref().event(&event);
        self.replaying.set(false);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::webview::WebViewEditAction;

    use super::super::edit;
    use super::{
        ECHO_AGE_LIMIT, ForwardedCopies, JudgedKeys, SAME_TIME_LIMIT, ZERO_TIME_ECHO_LIMIT,
    };

    /// 결과는 같은 번호의 사본에만 적용된다. 넘쳐 버린 사본의 결과는 다른 키에 적용되지 않는다.
    #[test]
    fn results_pair_with_copies_by_id_even_after_an_overflow() {
        let mut copies = ForwardedCopies::new(2);
        assert!(copies.push(1, "a"));
        assert!(copies.push(2, "b"));
        assert!(!copies.push(3, "c"));
        assert_eq!(copies.take(1), None);
        assert_eq!(copies.take(2), Some("b"));
        assert_eq!(copies.take(3), Some("c"));
        assert_eq!(copies.take(3), None);
    }

    /// 다시 만든 WebView에는 옛 번호의 사본이 없다. 옛 결과는 새 사본을 꺼내지 않는다.
    #[test]
    fn a_result_for_another_view_leaves_the_copies_alone() {
        let mut copies = ForwardedCopies::new(4);
        copies.push(10, "new");
        assert_eq!(copies.take(7), None);
        assert_eq!(copies.take(10), Some("new"));
    }

    /// 앞선 번호의 결과가 오지 않았으면 그 사본은 다음 번호의 결과와 함께 버린다.
    #[test]
    fn copies_without_a_result_are_dropped_by_a_later_result() {
        let mut copies = ForwardedCopies::new(4);
        copies.push(1, "lost");
        copies.push(2, "b");
        assert_eq!(copies.take(2), Some("b"));
        assert_eq!(copies.take(1), None);
    }

    /// 반송은 판정한 키가 몇 개 쌓였든 늦게 와도 판정한 키로 남는다.
    #[test]
    fn a_late_echo_stays_judged_after_many_later_keys() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        judged.note(100, (99, 54), now);
        for t in 101..200 {
            judged.note(t, (t, 38), now);
        }
        assert!(judged.contains(100, (99, 54), now));
        assert!(judged.contains(199, (199, 38), now));
        assert!(!judged.contains(200, (200, 38), now));
    }

    /// 같은 시각의 다른 키는 새 누름이다. 기억하지 않은 시각 0 키도 새 누름이다.
    #[test]
    fn same_time_keys_and_unknown_times_are_new_presses() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        judged.note(50, (1, 10), now);
        assert!(judged.contains(50, (1, 10), now));
        assert!(!judged.contains(50, (2, 11), now));
        judged.note(0, (3, 12), now);
        assert!(!judged.contains(0, (3, 12), now));
        assert!(!judged.contains(0, (1, 10), now));
    }

    /// X 서버 시각이 0으로 돌아간 뒤의 누름도 새 누름이다.
    #[test]
    fn keys_after_the_server_time_wraps_are_new_presses() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        judged.note(u32::MAX - 5, (1, 10), now);
        assert!(!judged.contains(3, (1, 10), now));
        judged.note(3, (1, 10), now);
        assert!(judged.contains(u32::MAX - 5, (1, 10), now));
    }

    /// 반 순환 넘게 입력이 없던 뒤의 누름은 새 누름이고, 그 시각부터 다시 판정한다.
    #[test]
    fn a_press_after_half_the_server_time_range_is_new() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        judged.note(1000, (1, 10), now);
        let later = 1000u32.wrapping_add(1 << 31).wrapping_add(5);
        assert!(!judged.contains(later, (2, 11), now));
        judged.note(later, (2, 11), now);
        assert!(judged.contains(later, (2, 11), now));
        assert!(!judged.contains(later.wrapping_add(1), (2, 11), now));
    }

    /// 나이 상한이 지나면 같은 시각이 다시 와도 새 누름이다.
    #[test]
    fn the_same_time_after_the_age_limit_is_a_new_press() {
        let start = Instant::now();
        let mut judged = JudgedKeys::default();
        judged.note(1000, (1, 10), start);
        assert!(judged.contains(1000, (1, 10), start + ECHO_AGE_LIMIT));
        let after = start + ECHO_AGE_LIMIT + Duration::from_millis(1);
        assert!(!judged.contains(1000, (1, 10), after));
        assert!(!judged.contains(999, (1, 10), after));
    }

    /// 같은 시각 목록이 넘친 뒤에는 그 시각의 어느 키도 다시 판정하지 않는다.
    #[test]
    fn keys_past_the_same_time_limit_stay_judged() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        for k in 0..=SAME_TIME_LIMIT as u32 {
            judged.note(50, (k, 10), now);
        }
        assert!(judged.contains(50, (SAME_TIME_LIMIT as u32, 10), now));
        judged.note(51, (1, 10), now);
        assert!(!judged.contains(51, (2, 10), now));
    }

    /// key_press.rs가 시각 0 편집 키를 누를 때 하는 일: 판정한 키가 아니면 무장하고, 반송을 기다릴 무장이면
    /// 표지를 남긴다. 판정했으면(무장했으면) true다.
    fn press_zero_time_edit(
        judged: &mut JudgedKeys,
        key: (u32, u16),
        action: WebViewEditAction,
        arm: u64,
        now: Instant,
    ) -> bool {
        if judged.contains(0, key, now) {
            return false;
        }
        if let Some(arm) = edit::echo_arm(action, arm) {
            judged.note_zero_time(key, arm, now);
        }
        true
    }

    /// on_view_message가 확장의 답을 받을 때 하는 일.
    fn reply(judged: &mut JudgedKeys, action: WebViewEditAction, arm: u64, range: bool) {
        if edit::extension_took_key(action, range) {
            judged.settle_zero_time(arm);
        }
    }

    const CTRL_V: (u32, u16) = (118, 55);
    const CTRL_C: (u32, u16) = (99, 54);

    /// 확장이 처리해 반송이 오지 않는 Paste는 표지를 남기지 않아, 시각 0으로 연달아 눌러도 두 번 모두 무장한다.
    #[test]
    fn zero_time_paste_twice_arms_twice() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_V,
            WebViewEditAction::Paste,
            1,
            now
        ));
        reply(&mut judged, WebViewEditAction::Paste, 1, false);
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_V,
            WebViewEditAction::Paste,
            2,
            now
        ));
    }

    /// 범위가 있어 확장이 처리한 Copy는 표지가 지워져 같은 키를 다시 누르면 다시 무장한다.
    #[test]
    fn zero_time_copy_with_a_range_leaves_no_mark() {
        let now = Instant::now();
        let mut judged = JudgedKeys::default();
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_C,
            WebViewEditAction::Copy,
            1,
            now
        ));
        reply(&mut judged, WebViewEditAction::Copy, 1, true);
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_C,
            WebViewEditAction::Copy,
            2,
            now
        ));
    }

    /// 범위가 없어 페이지로 간 Copy의 반송은 짧은 시간 안에 한 번만 판정한 키로 본다.
    #[test]
    fn an_echo_of_a_zero_time_copy_without_a_range_is_judged_once() {
        let start = Instant::now();
        let mut judged = JudgedKeys::default();
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_C,
            WebViewEditAction::Copy,
            1,
            start
        ));
        reply(&mut judged, WebViewEditAction::Copy, 1, false);
        assert!(!judged.contains(0, (120, 53), start));
        assert!(judged.contains(0, CTRL_C, start));
        assert!(press_zero_time_edit(
            &mut judged,
            CTRL_C,
            WebViewEditAction::Copy,
            2,
            start
        ));
        let late = start + ZERO_TIME_ECHO_LIMIT + Duration::from_millis(1);
        assert!(!judged.contains(0, CTRL_C, late));
    }
}
