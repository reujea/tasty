use std::time::Instant;
use winit::keyboard::{Key, NamedKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleTapKey {
    Shift,
    Ctrl,
    Alt,
}

impl DoubleTapKey {
    pub fn binding_str(&self) -> &'static str {
        match self {
            DoubleTapKey::Shift => "shift+shift",
            DoubleTapKey::Ctrl => "ctrl+ctrl",
            DoubleTapKey::Alt => "alt+alt",
        }
    }
}

/// 감지기가 본 키의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TapInput {
    /// 더블탭 대상 수식키.
    Tap(DoubleTapKey),
    /// 더블탭 대상이 아닌 수식키. 누르고 있는 동안 다른 수식키의 탭을 받지 않는다.
    /// macOS Option(`option` 토큰)이 여기 든다. `alt+alt`는 Command 두 번이다.
    Blocker,
    /// 그 밖의 키. 누르면 진행 중인 탭을 취소한다.
    Other,
}

/// 수식키 단독 누름·떼기 뒤 제한 시간 안에 같은 키를 다시 누르면 double tap으로 처리한다.
/// 누르는 동안 다른 키가 들어오면 해당 탭은 취소한다.
pub struct DoubleTapDetector {
    /// Maximum time between two taps (ms).
    threshold_ms: u128,
    /// The modifier currently being pressed (waiting for release).
    pending_key: Option<DoubleTapKey>,
    /// Whether another key was pressed during the current modifier hold.
    contaminated: bool,
    /// First tap: which key and when it was released.
    first_tap: Option<(DoubleTapKey, Instant)>,
    /// Fired double-tap event, consumed by the next poll.
    fired: Option<DoubleTapKey>,
    /// 더블탭 대상이 아닌 수식키(`TapInput::Blocker`)를 누르고 있다.
    blocker_held: bool,
    /// 키 토큰을 macOS 대응(`alt`=Command, `option`=Option)으로 읽는다.
    macos_mapping: bool,
}

impl DoubleTapDetector {
    pub fn new() -> Self {
        Self::with_mapping(cfg!(target_os = "macos"))
    }

    fn with_mapping(macos_mapping: bool) -> Self {
        Self {
            threshold_ms: 400,
            pending_key: None,
            contaminated: false,
            first_tap: None,
            fired: None,
            blocker_held: false,
            macos_mapping,
        }
    }

    /// `at` 은 그 키 사건이 창에 도착한 시각이다. 보류 뒤 재생한 탭도 도착 간격으로 판정한다.
    pub fn on_key_event(&mut self, key: &Key, pressed: bool, at: Instant) {
        let input = Self::classify(key, self.macos_mapping);
        if input == TapInput::Blocker {
            self.blocker_held = pressed;
        }
        // 대상이 아닌 수식키를 누르고 있는 동안의 수식키 탭은 조합 입력이므로 일반 키처럼 다룬다.
        let modifier = match input {
            TapInput::Tap(m) if !self.blocker_held => Some(m),
            _ => None,
        };

        if pressed {
            if let Some(m) = modifier {
                if self.pending_key.is_none() {
                    self.pending_key = Some(m);
                    self.contaminated = false;

                    if let Some((first_key, first_time)) = &self.first_tap
                        && *first_key == m
                        && at.saturating_duration_since(*first_time).as_millis() < self.threshold_ms
                    {
                        self.fired = Some(m);
                        self.first_tap = None;
                        self.pending_key = None;
                    }
                }
            } else {
                self.contaminated = true;
                self.first_tap = None;
            }
        } else {
            // 뗀 키는 차단 수식키를 누르고 있어도 식별한다. 누름 기록을 정리하지 않으면 다음 탭을 막는다.
            // 차단 수식키를 누른 뒤의 누름은 이미 오염 표지가 있어 첫 탭으로 기록하지 않는다.
            if let TapInput::Tap(m) = input
                && self.pending_key == Some(m)
            {
                if !self.contaminated {
                    self.first_tap = Some((m, at));
                }
                self.pending_key = None;
                self.contaminated = false;
            }
        }
    }

    pub fn take(&mut self) -> Option<DoubleTapKey> {
        self.fired.take()
    }

    /// 포커스 획득·상실 때 이전 탭과 누름 상태를 버린다. 창 밖의 release는 합성 이벤트로 걸러질 수
    /// 있어 이전 press와 돌아온 뒤 입력을 연결하면 탭 한 번을 두 번으로 오인하거나, 누른 채로 남은
    /// Option이 이후 탭을 계속 막을 수 있다.
    pub fn reset(&mut self) {
        self.pending_key = None;
        self.contaminated = false;
        self.first_tap = None;
        self.fired = None;
        self.blocker_held = false;
    }

    /// 키 위치 대응은 `docs/design/policies/key-mapping.md`의 토큰 표를 따른다. macOS에서
    /// winit은 Option을 `NamedKey::Alt`, Command를 `NamedKey::Super`로 보고한다.
    fn classify(key: &Key, macos_mapping: bool) -> TapInput {
        match key {
            Key::Named(NamedKey::Shift) => TapInput::Tap(DoubleTapKey::Shift),
            Key::Named(NamedKey::Control) => TapInput::Tap(DoubleTapKey::Ctrl),
            Key::Named(NamedKey::Alt) if macos_mapping => TapInput::Blocker,
            Key::Named(NamedKey::Alt) => TapInput::Tap(DoubleTapKey::Alt),
            Key::Named(NamedKey::Super) if macos_mapping => TapInput::Tap(DoubleTapKey::Alt),
            _ => TapInput::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt() -> Key {
        Key::Named(NamedKey::Alt)
    }

    #[test]
    fn two_clean_taps_fire() {
        let mut d = DoubleTapDetector::with_mapping(false);
        d.on_key_event(&alt(), true, Instant::now());
        d.on_key_event(&alt(), false, Instant::now());
        d.on_key_event(&alt(), true, Instant::now());
        assert_eq!(d.take(), Some(DoubleTapKey::Alt));
    }

    #[test]
    fn focus_change_reset_prevents_stale_first_tap() {
        let mut d = DoubleTapDetector::with_mapping(false);
        d.on_key_event(&alt(), true, Instant::now());
        d.reset();

        d.on_key_event(&alt(), false, Instant::now());
        d.on_key_event(&alt(), true, Instant::now());
        assert_eq!(d.take(), None);
    }

    fn named(k: NamedKey) -> Key {
        Key::Named(k)
    }

    fn tap_twice(d: &mut DoubleTapDetector, key: &Key) -> Option<DoubleTapKey> {
        d.on_key_event(key, true, Instant::now());
        d.on_key_event(key, false, Instant::now());
        d.on_key_event(key, true, Instant::now());
        d.on_key_event(key, false, Instant::now());
        d.take()
    }

    #[test]
    fn macos_mapping_takes_command_as_alt_and_ignores_option() {
        let mut d = DoubleTapDetector::with_mapping(true);
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Super)),
            Some(DoubleTapKey::Alt)
        );
        assert_eq!(tap_twice(&mut d, &named(NamedKey::Alt)), None);
        // Option 탭 뒤에도 Command 더블탭은 그대로 동작한다.
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Super)),
            Some(DoubleTapKey::Alt)
        );
    }

    #[test]
    fn macos_mapping_held_option_blocks_other_modifier_taps() {
        let mut d = DoubleTapDetector::with_mapping(true);
        d.on_key_event(&named(NamedKey::Alt), true, Instant::now());
        assert_eq!(tap_twice(&mut d, &named(NamedKey::Super)), None);
        assert_eq!(tap_twice(&mut d, &named(NamedKey::Shift)), None);
        d.on_key_event(&named(NamedKey::Alt), false, Instant::now());
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Shift)),
            Some(DoubleTapKey::Shift)
        );
    }

    #[test]
    fn macos_mapping_option_between_command_taps_cancels() {
        let mut d = DoubleTapDetector::with_mapping(true);
        d.on_key_event(&named(NamedKey::Super), true, Instant::now());
        d.on_key_event(&named(NamedKey::Super), false, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), true, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), false, Instant::now());
        d.on_key_event(&named(NamedKey::Super), true, Instant::now());
        assert_eq!(d.take(), None);
    }

    #[test]
    fn other_os_mapping_keeps_alt_and_ignores_super() {
        let mut d = DoubleTapDetector::with_mapping(false);
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Alt)),
            Some(DoubleTapKey::Alt)
        );
        assert_eq!(tap_twice(&mut d, &named(NamedKey::Super)), None);
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Control)),
            Some(DoubleTapKey::Ctrl)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn this_build_maps_option_away_from_alt_alt() {
        let mut d = DoubleTapDetector::new();
        assert_eq!(tap_twice(&mut d, &named(NamedKey::Alt)), None);
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Super)),
            Some(DoubleTapKey::Alt)
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn this_build_keeps_alt_double_tap() {
        let mut d = DoubleTapDetector::new();
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Alt)),
            Some(DoubleTapKey::Alt)
        );
    }

    /// Command를 누른 채 Option을 누르고 Command를 먼저 떼도 Command 누름 기록이 정리되어야 한다.
    /// 남으면 이후 Shift·Ctrl 더블탭이 모두 막힌다. 이 Command 누름은 조합이므로 첫 탭도 아니다.
    #[test]
    fn macos_mapping_command_released_under_option_clears_its_press() {
        let mut d = DoubleTapDetector::with_mapping(true);
        d.on_key_event(&named(NamedKey::Super), true, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), true, Instant::now());
        d.on_key_event(&named(NamedKey::Super), false, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), false, Instant::now());
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Shift)),
            Some(DoubleTapKey::Shift)
        );
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Control)),
            Some(DoubleTapKey::Ctrl)
        );

        let mut d = DoubleTapDetector::with_mapping(true);
        d.on_key_event(&named(NamedKey::Super), true, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), true, Instant::now());
        d.on_key_event(&named(NamedKey::Super), false, Instant::now());
        d.on_key_event(&named(NamedKey::Alt), false, Instant::now());
        d.on_key_event(&named(NamedKey::Super), true, Instant::now());
        assert_eq!(d.take(), None, "조합의 Command가 첫 탭으로 남았다");
    }

    /// Option을 누른 채 포커스를 잃고 창 밖에서 떼면 release가 오지 않는다. 포커스 리셋이 막는
    /// 상태를 함께 지워야 돌아온 뒤의 Command·Shift 더블탭이 동작한다.
    #[test]
    fn macos_mapping_reset_clears_a_held_option() {
        let mut d = DoubleTapDetector::with_mapping(true);
        d.on_key_event(&named(NamedKey::Alt), true, Instant::now());
        d.reset();
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Super)),
            Some(DoubleTapKey::Alt)
        );
        assert_eq!(
            tap_twice(&mut d, &named(NamedKey::Shift)),
            Some(DoubleTapKey::Shift)
        );
    }

    /// 보류 없이 바로 처리한 두 탭은 도착 시각이 곧 지금이라 지금 판정과 같다.
    #[test]
    fn a_double_tap_without_a_hold_is_judged_now() {
        let mut d = DoubleTapDetector::with_mapping(false);
        assert_eq!(tap_twice(&mut d, &alt()), Some(DoubleTapKey::Alt));
    }

    /// 판정은 처리 시각이 아니라 넘겨받은 도착 시각의 간격이다. 늦게 처리해도 도착 간격이 짧으면 더블탭이고, 바로
    /// 처리해도 도착 간격이 길면 아니다.
    #[test]
    fn taps_are_judged_by_the_arrival_times_given() {
        let start = Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        let mut d = DoubleTapDetector::with_mapping(false);
        d.on_key_event(&alt(), true, at(0));
        d.on_key_event(&alt(), false, at(50));
        d.on_key_event(&alt(), true, at(300));
        assert_eq!(d.take(), Some(DoubleTapKey::Alt));
        d.on_key_event(&alt(), false, at(310));
        d.on_key_event(&alt(), true, at(2000));
        d.on_key_event(&alt(), false, at(2050));
        d.on_key_event(&alt(), true, at(2500));
        assert_eq!(d.take(), None, "도착 간격 450ms 는 문턱 400ms 를 넘는다");
    }

    #[test]
    fn reset_discards_recorded_first_tap() {
        let mut d = DoubleTapDetector::with_mapping(false);
        d.on_key_event(&alt(), true, Instant::now());
        d.on_key_event(&alt(), false, Instant::now());
        d.reset();
        d.on_key_event(&alt(), true, Instant::now());
        assert_eq!(d.take(), None);
    }
}
