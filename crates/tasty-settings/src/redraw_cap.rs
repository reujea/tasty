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

/// 설정 화면 선택지 한 행의 표기 종류. 문구는 호출하는 쪽이 번역 키로 만든다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawFpsLabel {
    /// 목록 안의 값("{} fps").
    Fps(i64),
    /// 가장 빠른 모니터보다 높게 저장된 값("{} fps (above monitor)").
    AboveMonitor(i64),
    /// 무제한(-1). 모니터 주사율까지 그린다("Match monitor").
    MatchMonitor,
}

/// 설정 화면 한 행의 선택지 목록과 선택 인덱스.
///
/// 목록은 [`redraw_fps_choices`]다. 저장 값이 그 목록에 없으면(가장 빠른 모니터보다 높은 값)
/// 숫자 순서 자리, 곧 무제한 앞에 [`RedrawFpsLabel::AboveMonitor`]로 끼워 선택해 둔다. 공용
/// "(unknown)" 행을 쓰지 않는 이유는 값을 알고 있고 유효하기 때문이다. 끼운 행의 값은 저장 값
/// 자체이므로 그 행을 다시 골라도 저장 값은 바뀌지 않는다.
pub fn redraw_fps_options(
    max_refresh_millihertz: Option<u32>,
    saved: i64,
    label: impl Fn(RedrawFpsLabel) -> String,
) -> (Vec<(i64, String)>, usize) {
    let mut values = redraw_fps_choices(max_refresh_millihertz);
    let mut above = None;
    if !values.contains(&saved) {
        // 무제한은 늘 목록에 있으므로 saved 는 양수 선택지다. 무제한 앞 숫자 순서 자리에 둔다.
        let at = values
            .iter()
            .position(|v| *v == UNLIMITED_REDRAW_FPS || *v > saved)
            .unwrap_or(values.len());
        values.insert(at, saved);
        above = Some(saved);
    }
    let selected = values.iter().position(|v| *v == saved).unwrap_or(0);
    let rows = values
        .into_iter()
        .map(|v| {
            let kind = if v == UNLIMITED_REDRAW_FPS {
                RedrawFpsLabel::MatchMonitor
            } else if above == Some(v) {
                RedrawFpsLabel::AboveMonitor(v)
            } else {
                RedrawFpsLabel::Fps(v)
            };
            (v, label(kind))
        })
        .collect();
    (rows, selected)
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

    fn options(max: Option<u32>, saved: i64) -> (Vec<(i64, String)>, usize) {
        redraw_fps_options(max, saved, |l| match l {
            RedrawFpsLabel::Fps(v) => format!("{v} fps"),
            RedrawFpsLabel::AboveMonitor(v) => format!("{v} fps (above monitor)"),
            RedrawFpsLabel::MatchMonitor => "Match monitor".to_string(),
        })
    }

    fn values(rows: &[(i64, String)]) -> Vec<i64> {
        rows.iter().map(|(v, _)| *v).collect()
    }

    #[test]
    fn a_listed_value_selects_its_row_without_extra_rows() {
        let (rows, idx) = options(None, 30);
        assert_eq!(values(&rows), vec![15, 30, 60, -1]);
        assert_eq!(idx, 1);
        assert_eq!(rows[3].1, "Match monitor");

        let (rows, idx) = options(Some(60_000), UNLIMITED_REDRAW_FPS);
        assert_eq!(values(&rows), vec![15, 30, 60, -1]);
        assert_eq!(idx, 3);

        let (rows, idx) = options(Some(240_000), 240);
        assert_eq!(values(&rows), vec![15, 30, 60, 120, 144, 240, -1]);
        assert_eq!(idx, 5);
        assert_eq!(rows[5].1, "240 fps");
    }

    #[test]
    fn a_saved_value_above_the_fastest_monitor_sits_before_match_monitor() {
        let (rows, idx) = options(Some(60_000), 144);
        assert_eq!(values(&rows), vec![15, 30, 60, 144, -1]);
        assert_eq!(idx, 3);
        assert_eq!(rows[3].1, "144 fps (above monitor)");
        assert_eq!(rows[2].1, "60 fps");

        let (rows, idx) = options(Some(50_000), 60);
        assert_eq!(values(&rows), vec![15, 30, 60, -1]);
        assert_eq!(idx, 2);
        assert_eq!(rows[2].1, "60 fps (above monitor)");
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
