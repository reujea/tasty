//! 사람이 읽는 시각 문자열. 형식은 strftime 지정자로 받고, 로컬 시각은 이 컴퓨터의 시간대를 쓴다.
//! 시간대를 인자로 받는 함수는 시험이 고정 오프셋으로 결과를 확인하는 데 쓴다.
//! `%:z` 는 jiff가 초가 있는 오프셋을 `±HH:MM:SS` 로 쓰므로 직접 분 단위 `±HH:MM` 으로 바꿔 쓴다.

use jiff::Timestamp;
use jiff::ZonedDateTime;
use jiff::civil::DateTime;
use jiff::tz::{AmbiguousOffset, TimeZone};
use std::time::SystemTime;

/// 이 컴퓨터의 시간대. 알아낼 수 없으면 UTC처럼 동작하는 값이다.
pub(crate) fn local_zone() -> TimeZone {
    TimeZone::system()
}

/// 지금 로컬 시각을 `pattern` 으로 쓴다.
pub(crate) fn local_now(pattern: &str) -> String {
    format_in(Timestamp::now(), &local_zone(), pattern)
}

/// `at` 을 `tz` 의 시각으로 `pattern` 에 맞춰 쓴다.
pub(crate) fn format_in(at: Timestamp, tz: &TimeZone, pattern: &str) -> String {
    zoned_text(&at.to_zoned_datetime(tz.clone()), pattern)
}

fn zoned_text(zoned: &ZonedDateTime, pattern: &str) -> String {
    let pattern = with_minute_offset(pattern, zoned.offset().seconds());
    zoned.strftime(&pattern).to_string()
}

/// `pattern` 의 `%:z` 를 분 단위 오프셋 글자로 바꾼다. `%%` 는 글자 `%` 이므로 건너뛴다.
/// 바꿔 넣는 글자에는 `%` 가 없어 strftime이 다시 해석하지 않는다.
fn with_minute_offset(pattern: &str, offset_seconds: i32) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let (text, used) = if tail.starts_with("%:z") {
            (minute_offset(offset_seconds), 3)
        } else if tail.starts_with("%%") {
            ("%%".to_string(), 2)
        } else {
            ("%".to_string(), 1)
        };
        out.push_str(&text);
        rest = &tail[used..];
    }
    out.push_str(rest);
    out
}

/// chrono 0.4의 `%:z` 와 같은 `±HH:MM`. 부호는 반올림 전 오프셋을 따르고, 크기는 가장 가까운 분으로
/// 반올림하며 30초는 0에서 먼 쪽으로 올린다. 그래서 -29초는 `-00:00`, -30초는 `-00:01` 이다.
fn minute_offset(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let minutes = (offset_seconds.unsigned_abs() + 30) / 60;
    format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

/// 시스템 시각을 로컬 시각 문자열로 쓴다. 표현할 수 없는 시각이면 None이다.
pub(crate) fn local_system_time(t: SystemTime, pattern: &str) -> Option<String> {
    let at = Timestamp::try_from(t).ok()?;
    Some(format_in(at, &local_zone(), pattern))
}

/// unix 초를 로컬 시각 문자열로 쓴다. 표현할 수 없는 값이면 None이다.
pub(crate) fn local_unix_seconds(secs: i64, pattern: &str) -> Option<String> {
    let at = Timestamp::from_second(secs).ok()?;
    Some(format_in(at, &local_zone(), pattern))
}

/// epoch 밀리초를 로컬 시각 문자열로 쓴다. 표현할 수 없는 값이면 None이다.
pub(crate) fn local_epoch_millis(ms: i64, pattern: &str) -> Option<String> {
    let at = Timestamp::from_millisecond(ms).ok()?;
    Some(format_in(at, &local_zone(), pattern))
}

/// `tz` 에서 벽시계 시각 `civil` 을 쓴다. 두 번 있는 시각은 앞의 것을 쓰고,
/// 일광 절약 전환으로 없는 시각이면 None이다.
pub(crate) fn civil_in(civil: DateTime, tz: &TimeZone, pattern: &str) -> Option<String> {
    let ambiguous = tz.to_ambiguous_zoned_datetime(civil);
    if matches!(ambiguous.offset(), AmbiguousOffset::Gap { .. }) {
        return None;
    }
    let zoned = ambiguous.earlier().ok()?;
    Some(zoned_text(&zoned, pattern))
}

/// 지금 UTC 시각의 rfc3339 문자열. 초 단위이고 `Z` 로 끝난다(`2026-11-18T01:02:03Z`).
pub(crate) fn utc_now_rfc3339() -> String {
    utc_rfc3339(Timestamp::now())
}

fn utc_rfc3339(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::datetime;
    use jiff::tz::Offset;

    fn plus_0930() -> TimeZone {
        TimeZone::fixed(Offset::from_seconds(9 * 3600 + 30 * 60).expect("valid offset"))
    }

    /// 2026-11-18T01:02:03.456Z
    fn sample() -> Timestamp {
        Timestamp::from_millisecond(1_794_963_723_456).expect("representable")
    }

    #[test]
    fn the_sample_instant_is_the_documented_one() {
        assert_eq!(utc_rfc3339(sample()), "2026-11-18T01:02:03Z");
    }

    #[test]
    fn rfc3339_is_utc_seconds_with_z() {
        let s = utc_now_rfc3339();
        assert_eq!(s.len(), 20, "{s}");
        assert!(s.ends_with('Z'), "{s}");
        assert_eq!(s.as_bytes()[10], b'T', "{s}");
        let whole = Timestamp::from_second(1_794_963_723).expect("representable");
        assert_eq!(utc_rfc3339(whole), "2026-11-18T01:02:03Z");
    }

    #[test]
    fn local_formats_follow_the_given_zone() {
        let tz = plus_0930();
        let at = sample();
        assert_eq!(format_in(at, &tz, "%Y-%m-%d"), "2026-11-18");
        assert_eq!(format_in(at, &tz, "%H:%M:%S"), "10:32:03");
        assert_eq!(format_in(at, &tz, "%Y-%m-%d %H:%M"), "2026-11-18 10:32");
        assert_eq!(
            format_in(at, &tz, "%Y-%m-%d %H:%M:%S %:z"),
            "2026-11-18 10:32:03 +09:30"
        );
        assert_eq!(
            format_in(at, &tz, "%Y-%m-%dT%H:%M:%S%.3f%:z"),
            "2026-11-18T10:32:03.456+09:30"
        );
        let west = TimeZone::fixed(Offset::constant(-5));
        assert_eq!(
            format_in(at, &west, "%Y-%m-%dT%H:%M:%S%.3f%:z"),
            "2026-11-17T20:02:03.456-05:00"
        );
    }

    #[test]
    fn whole_seconds_keep_three_fraction_digits() {
        let at = Timestamp::from_second(1_794_963_723).expect("representable");
        let utc = TimeZone::UTC;
        assert_eq!(
            format_in(at, &utc, "%Y-%m-%dT%H:%M:%S%.3f%:z"),
            "2026-11-18T01:02:03.000+00:00"
        );
    }

    #[test]
    fn unrepresentable_inputs_give_none() {
        assert_eq!(local_unix_seconds(i64::MAX, "%Y"), None);
        assert_eq!(local_epoch_millis(i64::MAX, "%Y"), None);
    }

    #[test]
    fn a_civil_time_in_a_fixed_zone_keeps_its_wall_clock() {
        let civil = datetime(2026, 10, 21, 12, 12, 5, 0);
        assert_eq!(
            civil_in(civil, &plus_0930(), "%Y-%m-%d %H:%M:%S %:z").as_deref(),
            Some("2026-10-21 12:12:05 +09:30")
        );
    }

    fn fixed_seconds(seconds: i32) -> TimeZone {
        TimeZone::fixed(Offset::from_seconds(seconds).expect("valid offset"))
    }

    #[test]
    fn offsets_with_seconds_are_written_in_minutes_like_chrono() {
        let at = sample();
        let pattern = "%Y-%m-%dT%H:%M:%S%.3f%:z";
        // 벽시계는 초까지 그대로 옮기고 오프셋 표기만 분으로 반올림한다.
        let east = fixed_seconds(5 * 3600 + 30 * 60 + 15);
        assert_eq!(
            format_in(at, &east, pattern),
            "2026-11-18T06:32:18.456+05:30"
        );
        let west = fixed_seconds(-(3 * 3600 + 25 * 60 + 45));
        assert_eq!(
            format_in(at, &west, pattern),
            "2026-11-17T21:36:18.456-03:26"
        );
        let civil = datetime(2026, 10, 21, 12, 12, 5, 0);
        assert_eq!(
            civil_in(civil, &east, "%H:%M:%S %:z").as_deref(),
            Some("12:12:05 +05:30")
        );
    }

    #[test]
    fn minute_rounding_takes_thirty_seconds_away_from_zero() {
        let cases = [
            (29, "+00:00"),
            (30, "+00:01"),
            (-29, "-00:00"),
            (-30, "-00:01"),
            (5 * 3600 + 30 * 60 + 29, "+05:30"),
            (5 * 3600 + 30 * 60 + 30, "+05:31"),
            (-(5 * 3600 + 30 * 60 + 30), "-05:31"),
            (23 * 3600 + 59 * 60 + 30, "+24:00"),
        ];
        for (seconds, expected) in cases {
            assert_eq!(
                format_in(sample(), &fixed_seconds(seconds), "%:z"),
                expected,
                "{seconds}"
            );
        }
    }

    #[test]
    fn only_the_colon_offset_directive_is_replaced() {
        let tz = fixed_seconds(-(5 * 3600 + 30 * 60 + 30));
        assert_eq!(format_in(sample(), &tz, "%%:z %z"), "%:z -053030");
    }

    #[test]
    fn a_civil_time_in_a_gap_is_none_and_a_fold_takes_the_earlier() {
        let tz = TimeZone::get("America/New_York").expect("tzdb");
        let gap = datetime(2026, 3, 8, 2, 30, 0, 0);
        assert_eq!(civil_in(gap, &tz, "%H:%M %:z"), None);
        let fold = datetime(2026, 11, 1, 1, 30, 0, 0);
        assert_eq!(
            civil_in(fold, &tz, "%H:%M %:z").as_deref(),
            Some("01:30 -04:00")
        );
    }
}
