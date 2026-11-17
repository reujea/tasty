//! Properties 시각 형식. 설정 `general.explorer_props_time_format` 이 고르며 수정·만든·대상 수정
//! 시각에만 쓴다. 모두 이 컴퓨터의 시간대이고, 오프셋은 약어 대신 `+09:00` 같은 숫자로 붙인다.
//! 약어는 OS마다 다르고 모호하다(CST).

use std::time::SystemTime;

/// 설정 값과 strftime 형식. 첫 항목이 기본값이다. 설정 화면의 선택지 순서도 이 순서다.
pub(crate) const FORMATS: [(&str, &str); 3] = [
    ("minutes", "%Y-%m-%d %H:%M"),
    ("seconds", "%Y-%m-%d %H:%M:%S"),
    ("seconds_offset", "%Y-%m-%d %H:%M:%S %:z"),
];

/// 설정 값의 형식. 목록에 없는 값은 기본값(분까지)으로 보인다.
pub(crate) fn pattern(setting: &str) -> &'static str {
    FORMATS
        .iter()
        .find(|(value, _)| *value == setting)
        .unwrap_or(&FORMATS[0])
        .1
}

/// 로컬 시각. 값이 없으면 대시다. 오프셋은 그 시각에 맞는 값(일광 절약 포함)이다.
pub(crate) fn time_text(t: Option<SystemTime>, setting: &str) -> String {
    match t {
        Some(t) => {
            crate::clock::local_system_time(t, pattern(setting)).unwrap_or_else(|| "—".to_owned())
        }
        None => "—".to_owned(),
    }
}

/// 설정 화면 caption 의 예시. 고정한 로컬 시각 2026-10-21 12:12:05 를 고른 형식으로 보인다.
pub(crate) fn example(setting: &str) -> String {
    example_in(&crate::clock::local_zone(), setting)
}

fn example_in(tz: &jiff::tz::TimeZone, setting: &str) -> String {
    let sample = jiff::civil::datetime(2026, 10, 21, 12, 12, 5, 0);
    // 그 시각이 일광 절약 전환으로 없는 시간대면 오프셋 없이 보인다.
    crate::clock::civil_in(sample, tz, pattern(setting))
        .unwrap_or_else(|| sample.strftime(pattern("seconds")).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_setting_has_its_own_pattern_and_unknown_values_use_minutes() {
        assert_eq!(pattern("minutes"), "%Y-%m-%d %H:%M");
        assert_eq!(pattern("seconds"), "%Y-%m-%d %H:%M:%S");
        assert_eq!(pattern("seconds_offset"), "%Y-%m-%d %H:%M:%S %:z");
        assert_eq!(pattern("bogus"), "%Y-%m-%d %H:%M");
    }

    #[test]
    fn the_example_and_a_value_follow_the_chosen_format() {
        assert_eq!(example("minutes"), "2026-10-21 12:12");
        assert_eq!(example("seconds"), "2026-10-21 12:12:05");
        let with_offset = example("seconds_offset");
        assert!(
            with_offset.starts_with("2026-10-21 12:12:05 "),
            "{with_offset}"
        );
        let offset = with_offset.rsplit(' ').next().unwrap();
        assert!(
            offset.len() == 6 && (offset.starts_with('+') || offset.starts_with('-')),
            "a numeric offset like +09:00: {offset}"
        );
        assert_eq!(time_text(None, "seconds_offset"), "—");
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_792_000_000);
        let local = crate::clock::local_zone();
        let at = jiff::Timestamp::from_second(1_792_000_000).unwrap();
        let expected = crate::clock::format_in(at, &local, "%Y-%m-%d %H:%M:%S %:z");
        assert_eq!(time_text(Some(t), "seconds_offset"), expected);
        assert_eq!(time_text(Some(t), "seconds").len(), 19);
        assert_eq!(time_text(Some(t), "minutes").len(), 16);
    }

    /// 고정 오프셋 시간대에서 예시는 벽시계 시각 그대로이고 오프셋은 숫자다.
    #[test]
    fn the_example_in_a_fixed_zone_shows_the_numeric_offset() {
        let tz = jiff::tz::TimeZone::fixed(
            jiff::tz::Offset::from_seconds(-(3 * 3600 + 30 * 60)).unwrap(),
        );
        assert_eq!(example_in(&tz, "minutes"), "2026-10-21 12:12");
        assert_eq!(example_in(&tz, "seconds"), "2026-10-21 12:12:05");
        assert_eq!(
            example_in(&tz, "seconds_offset"),
            "2026-10-21 12:12:05 -03:30"
        );
    }
}
