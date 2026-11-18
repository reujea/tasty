//! 설정의 redraw 빈도 상한(`performance.output_redraw_fps`·`input_redraw_fps`)을 창들의 gate에 전한다.
//! 창 View는 설정을 갖지 않으므로 설정 적용 경로가 프로세스 공유값을 갱신하고 gate가 요청마다 읽는다.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// 0은 무제한을 뜻한다.
static OUTPUT_FPS: AtomicU32 = AtomicU32::new(fps_slot(tasty_settings::DEFAULT_OUTPUT_REDRAW_FPS));
static INPUT_FPS: AtomicU32 = AtomicU32::new(fps_slot(tasty_settings::DEFAULT_INPUT_REDRAW_FPS));

/// 저장 값을 공유 슬롯 값으로 바꾼다. 양수가 아니면 무제한(0)이다.
const fn fps_slot(value: i64) -> u32 {
    if value > 0 && value <= u32::MAX as i64 {
        value as u32
    } else {
        0
    }
}

/// 유발원별 상한(fps). `None`은 무제한이다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct RedrawCaps {
    pub(super) output: Option<u32>,
    pub(super) input: Option<u32>,
}

impl RedrawCaps {
    #[cfg(test)]
    pub(super) const UNLIMITED: Self = Self {
        output: None,
        input: None,
    };

    /// 설정 적용 경로가 마지막으로 갱신한 값.
    pub(super) fn current() -> Self {
        let read = |slot: &AtomicU32| Some(slot.load(Ordering::Relaxed)).filter(|v| *v > 0);
        Self {
            output: read(&OUTPUT_FPS),
            input: read(&INPUT_FPS),
        }
    }

    /// 출력 기인 프레임 사이 최소 간격. 무제한이면 모니터 간격이고, 상한이 모니터보다 빠르면 모니터 간격이다.
    pub(super) fn output_interval(self, monitor: Duration) -> Duration {
        self.output
            .map_or(monitor, |fps| fps_interval(fps).max(monitor))
    }

    /// 입력 기인 프레임 사이 최소 간격. 규칙은 출력과 같다(무제한이면 모니터 간격).
    pub(super) fn input_interval(self, monitor: Duration) -> Duration {
        self.input
            .map_or(monitor, |fps| fps_interval(fps).max(monitor))
    }
}

fn fps_interval(fps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000 / u64::from(fps.max(1)))
}

/// 부팅과 설정 적용 때 부른다. 이미 열린 창도 다음 요청부터 새 값을 쓴다.
pub(crate) fn install(settings: &tasty_settings::PerformanceSettings) {
    let slot = |cap: Option<u32>| cap.unwrap_or(0);
    OUTPUT_FPS.store(slot(settings.output_redraw_cap()), Ordering::Relaxed);
    INPUT_FPS.store(slot(settings.input_redraw_cap()), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR_60: Duration = Duration::from_nanos(16_666_666);

    #[test]
    fn caps_never_draw_faster_than_the_monitor() {
        let caps = RedrawCaps {
            output: Some(240),
            input: Some(144),
        };
        assert_eq!(caps.output_interval(MONITOR_60), MONITOR_60);
        assert_eq!(caps.input_interval(MONITOR_60), MONITOR_60);
        let slow = RedrawCaps {
            output: Some(30),
            input: Some(15),
        };
        assert_eq!(
            slow.output_interval(MONITOR_60),
            Duration::from_nanos(33_333_333)
        );
        assert_eq!(
            slow.input_interval(MONITOR_60),
            Duration::from_nanos(66_666_666)
        );
    }

    #[test]
    fn unlimited_output_and_input_follow_the_monitor() {
        assert_eq!(
            RedrawCaps::UNLIMITED.output_interval(MONITOR_60),
            MONITOR_60
        );
        assert_eq!(RedrawCaps::UNLIMITED.input_interval(MONITOR_60), MONITOR_60);
    }

    #[test]
    fn slot_defaults_match_the_settings_defaults() {
        let d = tasty_settings::PerformanceSettings::default();
        assert_eq!(
            Some(fps_slot(tasty_settings::DEFAULT_OUTPUT_REDRAW_FPS)),
            d.output_redraw_cap()
        );
        assert_eq!(
            Some(fps_slot(tasty_settings::DEFAULT_INPUT_REDRAW_FPS)),
            d.input_redraw_cap()
        );
        assert_eq!(fps_slot(tasty_settings::UNLIMITED_REDRAW_FPS), 0);
    }
}
