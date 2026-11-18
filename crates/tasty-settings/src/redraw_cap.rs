//! 출력·입력 기인 redraw 빈도 상한(`performance.output_redraw_fps`·`performance.input_redraw_fps`).
//!
//! 값은 초당 프레임 수다. [`UNLIMITED_REDRAW_FPS`]는 상한을 두지 않고 모니터 주사율을 따른다는 뜻이다.
//! 실제 적용은 창이 있는 모니터의 주사율로 다시 제한하며 그 계산은 본체의 repaint gate가 맡는다.

/// 고를 수 있는 상한 값(무제한 제외). 오름차순이다.
pub const REDRAW_FPS_CHOICES: [i64; 6] = [15, 30, 60, 120, 144, 240];
/// 상한 없음. 모니터 주사율까지 그린다.
pub const UNLIMITED_REDRAW_FPS: i64 = -1;
/// 터미널 출력이 유발한 redraw의 기본 상한.
pub const DEFAULT_OUTPUT_REDRAW_FPS: i64 = 30;
/// 사용자 입력이 유발한 redraw의 기본 상한.
pub const DEFAULT_INPUT_REDRAW_FPS: i64 = 60;
/// 모니터가 주사율을 보고하지 않을 때 가정하는 값(Hz). repaint gate의 폴백과 같다.
pub const FALLBACK_REFRESH_HZ: u32 = 60;

/// 저장 값이 허용 값(선택지 또는 무제한)인가.
pub fn is_valid_redraw_fps(value: i64) -> bool {
    value == UNLIMITED_REDRAW_FPS || REDRAW_FPS_CHOICES.contains(&value)
}

/// 저장 값을 적용할 상한으로 바꾼다. `None`은 무제한이다.
/// 허용 값이 아니면(정규화를 거치지 않은 값) `fallback`을 쓴다.
pub fn redraw_fps_cap(value: i64, fallback: i64) -> Option<u32> {
    let value = if is_valid_redraw_fps(value) {
        value
    } else {
        fallback
    };
    u32::try_from(value).ok().filter(|v| *v > 0)
}

/// 설정 화면에 보일 선택지. 연결된 모니터 중 가장 높은 주사율 이하의 값과 무제한(맨 끝)이다.
/// `max_refresh_millihertz`가 없으면 [`FALLBACK_REFRESH_HZ`]로 본다.
/// 59.94Hz처럼 정수가 아닌 보고값은 가장 가까운 Hz로 반올림해 60을 남긴다.
pub fn redraw_fps_choices(max_refresh_millihertz: Option<u32>) -> Vec<i64> {
    let max_hz = max_refresh_millihertz
        .filter(|m| *m > 0)
        .map_or(FALLBACK_REFRESH_HZ, |m| m.saturating_add(500) / 1000);
    REDRAW_FPS_CHOICES
        .iter()
        .copied()
        .filter(|v| *v <= i64::from(max_hz))
        .chain(std::iter::once(UNLIMITED_REDRAW_FPS))
        .collect()
}

/// 연결된 모니터들의 보고값에서 가장 높은 주사율을 고른다. 보고값이 하나도 없으면 `None`.
pub fn max_refresh_millihertz(reports: impl IntoIterator<Item = Option<u32>>) -> Option<u32> {
    reports.into_iter().flatten().filter(|m| *m > 0).max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_stop_at_the_fastest_monitor_and_keep_unlimited() {
        assert_eq!(redraw_fps_choices(Some(75_000)), vec![15, 30, 60, -1]);
        assert_eq!(
            redraw_fps_choices(Some(144_000)),
            vec![15, 30, 60, 120, 144, -1]
        );
        assert_eq!(
            redraw_fps_choices(Some(240_000)),
            vec![15, 30, 60, 120, 144, 240, -1]
        );
        assert_eq!(
            redraw_fps_choices(Some(360_000)),
            vec![15, 30, 60, 120, 144, 240, -1]
        );
    }

    #[test]
    fn choices_assume_60hz_without_a_report_and_round_fractional_rates() {
        assert_eq!(redraw_fps_choices(None), vec![15, 30, 60, -1]);
        assert_eq!(redraw_fps_choices(Some(0)), vec![15, 30, 60, -1]);
        assert_eq!(redraw_fps_choices(Some(59_940)), vec![15, 30, 60, -1]);
        assert_eq!(
            redraw_fps_choices(Some(143_856)),
            vec![15, 30, 60, 120, 144, -1]
        );
        assert_eq!(redraw_fps_choices(Some(50_000)), vec![15, 30, -1]);
    }

    #[test]
    fn max_refresh_picks_the_fastest_reported_monitor() {
        assert_eq!(
            max_refresh_millihertz([Some(60_000), None, Some(144_000)]),
            Some(144_000)
        );
        assert_eq!(max_refresh_millihertz([None, Some(0)]), None);
        assert_eq!(max_refresh_millihertz([]), None);
    }

    #[test]
    fn cap_maps_unlimited_and_falls_back_on_invalid_values() {
        assert_eq!(redraw_fps_cap(30, 60), Some(30));
        assert_eq!(redraw_fps_cap(UNLIMITED_REDRAW_FPS, 60), None);
        assert_eq!(redraw_fps_cap(90, 60), Some(60));
        assert_eq!(redraw_fps_cap(0, 30), Some(30));
        assert_eq!(redraw_fps_cap(-7, UNLIMITED_REDRAW_FPS), None);
    }
}
