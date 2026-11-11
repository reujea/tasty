//! Windows·macOS에서 host가 가져간 키를 처리 결과가 올 때까지 붙잡아 두는 상태.
//!
//! native 키 콜백은 가져갈지를 동기로 정하고 실제 단축키 실행은 나중에 host 큐에서 일어난다. 그래서 가져간
//! key-down의 원래 이벤트를 host 큐 이벤트와 같은 일련번호로 두었다가, Tasty 단축키가 동작하지 않았다는 결과가
//! 오면 그 이벤트를 키를 받은 WebView에 다시 넘긴다(`docs/design/policies/key-mapping.md`의 웹 화면 키 원칙).
//! 자동 반복은 첫 누름의 결과를 따른다. 결과가 오기 전에 온 반복과 key-up은 첫 누름과 함께 붙잡았다가 결과가
//! 오면 순서대로 넘기거나(반복은 동작하지 않았을 때만) 버린다. OS별 이벤트 형태는 `E`, 물리 키 식별은 `K`다.

use std::collections::VecDeque;

/// 결과를 기다리는 누름의 상한. host 큐는 매 이벤트 루프에서 비워지므로 정상 입력은 여기에 닿지 않는다.
pub(crate) const HELD_LIMIT: usize = 64;
/// 결과 전에 한 누름에 붙잡는 반복의 상한. 넘친 반복은 붙잡지 않고 소비한다(결과가 와도 넘기지 않는다).
pub(crate) const REPEAT_LIMIT: usize = 64;
/// 결과를 기다리며 붙잡을 수 있는 이벤트 수의 상한(누름마다 첫 누름·반복·key-up). macOS 반송 표지의 상한이다.
#[cfg(target_os = "macos")]
pub(crate) const HELD_EVENT_LIMIT: usize = HELD_LIMIT * (REPEAT_LIMIT + 2);
/// 누름 결과를 기억하는 물리 키 수의 상한. 동시에 누른 키 수보다 넉넉하다.
const DECIDED_LIMIT: usize = 16;

struct Held<K, E> {
    id: u64,
    key: K,
    press: E,
    repeats: Vec<E>,
    release: Option<E>,
    /// 포커스가 바뀌기 전의 누름이다. 결과가 와도 key-down을 넘기지 않고 기록도 남기지 않는다.
    stale: bool,
}

/// 결과가 온 누름에서 넘길 이벤트.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Settled<E> {
    /// 다시 넘길 key-down(첫 누름과 그 뒤 반복). Tasty 단축키가 동작했으면 비어 있다.
    pub replay: Vec<E>,
    /// 붙잡았던 key-up. 결과와 관계없이 key-down 뒤에 넘긴다. key-up은 세 OS 모두 판정하지 않고 페이지로 보낸다
    /// (Linux는 key-release를 엔진에 그대로 둔다). 동작한 키의 페이지는 key-down 없이 key-up만 받는다.
    pub release: Option<E>,
}

/// 자동 반복 key-down을 어떻게 다룰지.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RepeatRoute<E> {
    /// 첫 누름이 결과를 기다리고 있어 붙잡았다. backend는 이 반복을 소비한다.
    Held,
    /// 첫 누름이 넘어갔다. backend는 반복을 그대로 엔진·페이지에 보낸다.
    Pass(E),
    /// 첫 누름이 동작했거나 입력 게이트에 막혔다. backend는 반복을 소비한다.
    Drop,
    /// 이 물리 키의 첫 누름을 판정한 기록이 없다. backend는 기존 판정(`consumes_repeat`)을 쓴다.
    Unknown(E),
}

/// WebView 하나의 붙잡은 키. GUI 스레드에서만 쓴다. WebView를 다시 만들면 새 상태가 생기므로 옛 결과는
/// 짝이 없어 아무것도 넘기지 않는다.
pub(crate) struct KeyHold<K, E> {
    held: VecDeque<Held<K, E>>,
    /// 물리 키별 첫 누름의 결과(true = 넘김). 그 자리에서 정한 누름과 host 결과가 온 누름을 모두 기록한다.
    /// key-up·새 첫 누름·포커스 변경에서 지운다.
    decided: Vec<(K, bool)>,
    limit: usize,
}

impl<K: Copy + PartialEq, E> Default for KeyHold<K, E> {
    fn default() -> Self {
        Self::with_limit(HELD_LIMIT)
    }
}

impl<K: Copy + PartialEq, E> KeyHold<K, E> {
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            held: VecDeque::new(),
            decided: Vec::new(),
            limit,
        }
    }

    /// 반복이 아닌 key-down을 그 자리에서 정했다(`pass` = 엔진·페이지로 넘김, false = 소비). host 큐로 가져간
    /// 누름처럼 그 물리 키의 반복이 이 결과를 따른다.
    pub(crate) fn decide(&mut self, key: K, pass: bool) {
        self.record(key, pass);
    }

    fn record(&mut self, key: K, pass: bool) {
        self.decided.retain(|(k, _)| *k != key);
        self.decided.push((key, pass));
        if self.decided.len() > DECIDED_LIMIT {
            self.decided.remove(0);
        }
    }

    /// host가 일련번호 `id`로 가져간 첫 누름을 붙잡는다. 넘치면 가장 오래된 누름을 버리고 false다. 버린 누름의
    /// 결과는 짝이 없어 무시된다(두 번 실행하거나 엉뚱한 키를 넘기는 것보다 버리는 편이 안전하다).
    pub(crate) fn forwarded(&mut self, id: u64, key: K, press: E) -> bool {
        self.decided.retain(|(k, _)| *k != key);
        self.held.push_back(Held {
            id,
            key,
            press,
            repeats: Vec::new(),
            release: None,
            stale: false,
        });
        if self.held.len() > self.limit {
            self.held.pop_front();
            return false;
        }
        true
    }

    /// 아직 떼지 않은 마지막 붙잡은 누름. 포커스가 바뀌기 전의 누름은 뒤 입력을 붙잡지 않는다.
    fn open_held(&mut self, key: K) -> Option<&mut Held<K, E>> {
        self.held
            .iter_mut()
            .rev()
            .find(|h| h.key == key && h.release.is_none() && !h.stale)
    }

    /// 자동 반복 key-down의 처리를 정한다.
    pub(crate) fn repeat(&mut self, key: K, event: E) -> RepeatRoute<E> {
        if let Some(held) = self.open_held(key) {
            if held.repeats.len() < REPEAT_LIMIT {
                held.repeats.push(event);
            }
            return RepeatRoute::Held;
        }
        match self.decided.iter().find(|(k, _)| *k == key) {
            Some((_, true)) => RepeatRoute::Pass(event),
            Some((_, false)) => RepeatRoute::Drop,
            None => RepeatRoute::Unknown(event),
        }
    }

    /// key-up이 왔다. 그 물리 키의 누름이 결과를 기다리면 붙잡고 None이다. 아니면 이벤트를 돌려주고 backend가
    /// 그대로 보낸다. 어느 쪽이든 그 물리 키의 누름 결과는 지운다.
    pub(crate) fn release(&mut self, key: K, event: E) -> Option<E> {
        self.decided.retain(|(k, _)| *k != key);
        match self.open_held(key) {
            Some(held) => {
                held.release = Some(event);
                None
            }
            None => Some(event),
        }
    }

    /// 포커스가 바뀌었다. 다른 곳에서 누른 키의 반복이 옛 결과를 따르지 않게 누름 결과를 지운다.
    /// 결과를 기다리는 누름은 옛 세대로 표시한다. 그 결과가 나중에 와도 key-down을 넘기거나 기록을 되살리지
    /// 않고, 붙잡은 key-up만 넘긴다. 짝은 그대로 두어 뒤 번호의 결과가 앞 누름에 붙지 않게 한다.
    pub(crate) fn clear_decisions(&mut self) {
        self.decided.clear();
        for held in &mut self.held {
            held.stale = true;
        }
    }

    /// 일련번호 `id`의 결과가 왔다. `pass`는 Tasty 단축키가 동작하지 않아 키를 넘길지다. 짝이 없으면 None이다.
    /// host는 한 창의 키를 가져간 순서대로 처리하므로 그보다 앞선 누름은 결과가 오지 않을 누름이라 버린다.
    pub(crate) fn finish(&mut self, id: u64, pass: bool) -> Option<Settled<E>> {
        let held = loop {
            let front = self.held.front()?;
            if front.id > id {
                return None;
            }
            let held = self.held.pop_front()?;
            if held.id == id {
                break held;
            }
            tracing::warn!(
                "webview key {} never got a result before key {id}; dropping it",
                held.id
            );
        };
        if held.release.is_none() && !held.stale {
            self.record(held.key, pass);
        }
        let replay = if pass && !held.stale {
            std::iter::once(held.press).chain(held.repeats).collect()
        } else {
            Vec::new()
        };
        Some(Settled {
            replay,
            release: held.release,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(replay: &[&'static str], release: Option<&'static str>) -> Settled<&'static str> {
        Settled {
            replay: replay.to_vec(),
            release,
        }
    }

    /// 넘침으로 버린 누름의 결과는 아무것도 넘기지 않고, 남은 누름은 자기 번호의 결과와 짝짓는다.
    #[test]
    fn results_pair_with_forwarded_keys_by_sequence_even_after_overflow() {
        let mut q = KeyHold::<u32, &str>::with_limit(2);
        assert!(q.forwarded(1, 10, "a"));
        assert!(q.forwarded(2, 11, "b"));
        assert!(!q.forwarded(3, 12, "c"));
        assert_eq!(q.finish(1, true), None);
        assert_eq!(q.finish(2, true), Some(settled(&["b"], None)));
        assert_eq!(q.finish(3, false), Some(settled(&[], None)));
        assert_eq!(q.finish(3, true), None);
    }

    /// 결과가 오지 않은 앞선 누름은 뒤 번호의 결과가 올 때 버리고, 이미 지난 번호(다른 WebView·재생성 전)의
    /// 결과는 짝이 없다.
    #[test]
    fn earlier_keys_without_a_result_are_dropped_and_stale_results_do_nothing() {
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(5, 1, "a");
        q.forwarded(6, 2, "b");
        assert_eq!(q.finish(4, true), None);
        assert_eq!(q.finish(6, true), Some(settled(&["b"], None)));
        assert_eq!(q.finish(5, true), None);
    }

    /// 결과 전에 온 반복과 key-up은 붙잡았다가, 넘기면 누름·반복을 순서대로 넘기고 key-up은 뒤에 둔다.
    /// 동작했으면 누름·반복은 버리고 key-up만 돌려준다.
    #[test]
    fn repeats_and_release_before_the_result_follow_the_first_press() {
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(1, 7, "down");
        assert_eq!(q.repeat(7, "r1"), RepeatRoute::Held);
        assert_eq!(q.repeat(7, "r2"), RepeatRoute::Held);
        assert_eq!(q.release(7, "up"), None);
        assert_eq!(
            q.finish(1, true),
            Some(settled(&["down", "r1", "r2"], Some("up")))
        );

        q.forwarded(2, 7, "down");
        assert_eq!(q.repeat(7, "r1"), RepeatRoute::Held);
        assert_eq!(q.release(7, "up"), None);
        assert_eq!(q.finish(2, false), Some(settled(&[], Some("up"))));
    }

    /// 결과가 온 뒤의 반복은 첫 누름의 결과를 따른다. key-up·새 누름·포커스 변경 뒤에는 기록이 없다.
    #[test]
    fn repeats_after_the_result_follow_it_until_release_press_or_focus_change() {
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(1, 7, "down");
        q.forwarded(2, 8, "down8");
        q.finish(1, true);
        q.finish(2, false);
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Pass("r"));
        assert_eq!(q.repeat(8, "r"), RepeatRoute::Drop);

        assert_eq!(q.release(7, "up"), Some("up"));
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Unknown("r"));

        q.forwarded(4, 8, "down8");
        assert_eq!(q.repeat(8, "r"), RepeatRoute::Held);

        q.forwarded(3, 9, "down9");
        q.finish(3, true);
        q.clear_decisions();
        assert_eq!(q.repeat(9, "r"), RepeatRoute::Unknown("r"));
    }

    /// 결과 전에 뗀 누름은 결과가 와도 기록을 남기지 않는다. 다음 누름의 반복이 옛 결과를 따르지 않는다.
    #[test]
    fn a_press_released_before_its_result_leaves_no_decision() {
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(1, 7, "down");
        assert_eq!(q.release(7, "up"), None);
        q.finish(1, true);
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Unknown("r"));
    }

    /// 같은 물리 키를 결과 전에 다시 누르면 반복과 key-up은 마지막 누름에 붙는다.
    #[test]
    fn a_second_press_of_the_same_key_gets_its_own_repeats() {
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(1, 7, "d1");
        assert_eq!(q.release(7, "u1"), None);
        q.forwarded(2, 7, "d2");
        assert_eq!(q.repeat(7, "r2"), RepeatRoute::Held);
        assert_eq!(q.finish(1, true), Some(settled(&["d1"], Some("u1"))));
        assert_eq!(q.finish(2, true), Some(settled(&["d2", "r2"], None)));
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Pass("r"));
    }

    /// 포커스가 바뀐 뒤 온 옛 누름의 결과는 key-down을 넘기지 않고 기록도 남기지 않는다. 붙잡은 key-up만 넘긴다.
    /// 포커스가 바뀐 뒤의 반복과 key-up은 옛 누름에 붙지 않는다.
    #[test]
    fn a_result_after_a_focus_change_does_not_revive_the_old_press() {
        // Given 결과를 기다리는 누름, When 포커스가 바뀐 뒤 처리 안 됨 결과가 온다,
        // Then 넘길 key-down이 없고 뒤 반복은 기존 판정을 쓴다.
        let mut q = KeyHold::<u32, &str>::default();
        q.forwarded(1, 7, "down");
        q.clear_decisions();
        assert_eq!(q.finish(1, false), Some(settled(&[], None)));
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Unknown("r"));

        q.forwarded(2, 7, "down");
        q.clear_decisions();
        assert_eq!(q.finish(2, true), Some(settled(&[], None)));
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Unknown("r"));

        // Given 결과 전에 key-up까지 붙잡은 누름, When 포커스가 바뀐 뒤 결과가 온다, Then key-up만 넘긴다.
        q.forwarded(3, 8, "down");
        assert_eq!(q.release(8, "up"), None);
        q.clear_decisions();
        assert_eq!(q.finish(3, true), Some(settled(&[], Some("up"))));

        // Given 포커스가 바뀐 뒤의 반복·key-up, Then 옛 누름에 붙지 않는다.
        q.forwarded(4, 9, "down");
        q.clear_decisions();
        assert_eq!(q.repeat(9, "r"), RepeatRoute::Unknown("r"));
        assert_eq!(q.release(9, "up"), Some("up"));
        assert_eq!(q.finish(4, true), Some(settled(&[], None)));
    }

    /// 그 자리에서 정한 첫 누름도 반복이 그 결과를 따른다. 수식키를 뗀 뒤의 반복이 다시 판정되지 않는다.
    #[test]
    fn repeats_follow_a_first_press_decided_on_the_spot() {
        // Given 편집 명령으로 실행해 소비한 첫 누름, When 반복이 온다, Then 반복도 소비한다.
        let mut q = KeyHold::<u32, &str>::default();
        q.decide(7, false);
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Drop);
        // Given 엔진·페이지로 넘긴 첫 누름, When 반복이 온다, Then 반복도 넘긴다.
        q.decide(8, true);
        assert_eq!(q.repeat(8, "r"), RepeatRoute::Pass("r"));
        // Given 같은 키를 다시 누르고 이번에는 소비했다, Then 앞 결과가 아니라 새 결과를 따른다.
        q.decide(8, false);
        assert_eq!(q.repeat(8, "r"), RepeatRoute::Drop);
        // Given 앞서 넘긴 키를 다시 누르고 host로 가져갔다, Then 결과 전 반복은 붙잡는다.
        q.forwarded(1, 7, "down");
        assert_eq!(q.repeat(7, "r"), RepeatRoute::Held);
        // Given key-up, Then 기록이 지워진다.
        assert_eq!(q.release(8, "up"), Some("up"));
        assert_eq!(q.repeat(8, "r"), RepeatRoute::Unknown("r"));
    }

    /// 결과 전 반복은 상한까지만 붙잡고, 넘친 반복은 소비만 한다.
    #[test]
    fn repeats_before_the_result_are_held_up_to_the_limit() {
        let mut q = KeyHold::<u32, u32>::default();
        q.forwarded(1, 7, 0);
        for i in 1..=REPEAT_LIMIT as u32 + 3 {
            assert_eq!(q.repeat(7, i), RepeatRoute::Held);
        }
        let settled = q.finish(1, true).expect("paired result");
        assert_eq!(settled.replay.len(), 1 + REPEAT_LIMIT);
        assert_eq!(settled.replay.last(), Some(&(REPEAT_LIMIT as u32)));
    }

    /// 붙잡은 누름이 없는 key-up은 그대로 돌려준다.
    #[test]
    fn a_release_without_a_held_press_passes() {
        let mut q = KeyHold::<u32, &str>::default();
        assert_eq!(q.release(3, "up"), Some("up"));
    }
}
