//! 스캔 읽기의 출처 정보: 청크가 들어올 때 정한 분류가 `text` 범위로 나온다.
use super::*;
use crate::{OUTPUT_RETENTION_MAX_BYTES, ScanRead, TEXT_ORIGINS_MAX_ENTRIES};

const MS: Duration = Duration::from_millis(1);

fn idle_terminal() -> Terminal {
    let t = Terminal::new_detached(80, 24);
    let past = Instant::now() - Duration::from_secs(300);
    let mut st = t.lock_state();
    st.last_output_at = past;
    st.last_activity_at = past;
    st.last_activity_chunk_at = past;
    drop(st);
    t
}

/// 로컬 OS 적용이 `at`에 성공한 것처럼 구간을 연다(공개 열기·확정과 같은 전이).
fn notify_at(t: &Terminal, at: Instant) {
    let mut st = t.lock_state();
    let _before = st.begin_os_resize(at); // 성공으로 확정하므로 되돌릴 상태가 필요 없다.
    st.confirm_os_resize(at);
}

fn scan(t: &mut Terminal, strip_ansi: bool) -> ScanRead {
    let read = t.take_since_output_scan_mark(strip_ansi, true);
    assert_partition(&read);
    read
}

fn ranges(read: &ScanRead) -> Vec<(usize, usize, &'static str)> {
    let origins = read.origins.as_ref().expect("text_origins가 있어야 한다");
    origins
        .ranges
        .iter()
        .map(|r| (r.start, r.end, r.origin.as_str()))
        .collect()
}

fn gaps(read: &ScanRead) -> Vec<(usize, &'static str)> {
    let origins = read.origins.as_ref().expect("text_origins가 있어야 한다");
    origins
        .gaps
        .iter()
        .map(|g| (g.at, g.reason.as_str()))
        .collect()
}

/// `ranges`는 `text`를 문자 경계에서 빈틈없이 덮고 이웃은 출처가 다르다. `gaps`는 정렬된 문자 경계다.
fn assert_partition(read: &ScanRead) {
    let Some(origins) = &read.origins else {
        return;
    };
    let text = &read.read.text;
    let mut at = 0;
    for (i, r) in origins.ranges.iter().enumerate() {
        assert_eq!(r.start, at, "빈틈: {origins:?}");
        assert!(r.end > r.start && text.is_char_boundary(r.end), "{r:?}");
        if i > 0 {
            assert_ne!(origins.ranges[i - 1].origin, r.origin, "합치지 않았다");
        }
        at = r.end;
    }
    assert_eq!(at, text.len(), "끝까지 덮지 않았다");
    for w in origins.gaps.windows(2) {
        assert!(w[0].at <= w[1].at && w[0] != w[1], "{:?}", origins.gaps);
    }
    for g in &origins.gaps {
        assert!(g.at <= text.len() && text.is_char_boundary(g.at), "{g:?}");
    }
}

/// Given 구간 밖 출력만, When 읽으면, Then 전체가 normal이고 gap이 없다.
#[test]
fn output_outside_any_window_is_normal() {
    let mut t = idle_terminal();
    t.process_bytes(b"hello world");
    let read = scan(&mut t, false);
    assert_eq!(ranges(&read), [(0, 11, "normal")]);
    assert!(gaps(&read).is_empty());
}

/// Given 통지 뒤 재그리기만, When 첫 읽기, Then 전체가 redraw다.
#[test]
fn redraw_after_a_notice_is_redraw_on_the_first_read() {
    let mut t = idle_terminal();
    // 적용 성공을 흉내 낸다. 성공이면 되돌릴 상태가 필요 없다.
    let _applied = t.open_resize_window();
    t.confirm_resize_window();
    t.process_bytes(b"\x1b[H\x1b[2Jscreen");
    let read = scan(&mut t, true);
    assert_eq!(read.read.text, "screen");
    assert_eq!(ranges(&read), [(0, 6, "redraw")]);
}

/// Given 앞 구간의 재그리기, 구간 밖 제목 OSC, 다음 통지, When 읽으면, Then 앞 재그리기는 redraw다. 읽을 때의
/// 구간(다음 통지가 연 구간)으로 가르면 normal이 된다.
#[test]
fn an_earlier_window_redraw_stays_redraw_after_the_next_notice() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    notify_at(&t, t0);
    t.feed_bytes_at(b"redraw", t0 + 100 * MS);
    t.feed_bytes_at(b"\x1b]0;title\x07", t0 + 800 * MS);
    notify_at(&t, t0 + 900 * MS);
    let current = t.lock_state().resize_redraw.expect("구간");
    assert!(!ResizeRedraw::contains(
        current.output_start,
        current.end,
        t0 + 100 * MS
    ));
    let read = scan(&mut t, true);
    assert_eq!(read.read.text, "redraw");
    assert_eq!(ranges(&read), [(0, 6, "redraw")]);
    assert!(
        gaps(&read).is_empty(),
        "일반 제목 OSC의 제거는 끊긴 자리가 아니다"
    );
}

/// 계약 응답 모양 4의 예: 일반 `API `, 재그리기 `ESC[2J`, 일반 `Error`.
fn api_error(t: &mut Terminal) {
    let t0 = Instant::now();
    t.feed_bytes_at(b"API ", t0 - Duration::from_secs(1));
    notify_at(t, t0);
    t.feed_bytes_at(b"\x1b[2J", t0 + 100 * MS);
    t.feed_bytes_at(b"Error", t0 + Duration::from_secs(1));
}

#[test]
fn a_removed_redraw_sequence_leaves_a_gap_inside_normal_text() {
    let mut t = idle_terminal();
    api_error(&mut t);
    let read = scan(&mut t, true);
    assert_eq!(read.read.text, "API Error");
    assert_eq!(ranges(&read), [(0, 9, "normal")]);
    assert_eq!(gaps(&read), [(4, "redraw_removed")]);
}

/// Given 같은 출력을 strip 없이 읽으면, Then 지운 것이 없어 gap 없이 범위가 셋이다.
#[test]
fn without_stripping_the_redraw_sequence_is_its_own_range() {
    let mut t = idle_terminal();
    api_error(&mut t);
    let read = scan(&mut t, false);
    assert_eq!(read.read.text, "API \x1b[2JError");
    assert_eq!(
        ranges(&read),
        [(0, 4, "normal"), (4, 8, "redraw"), (8, 13, "normal")]
    );
    assert!(gaps(&read).is_empty());
}

#[test]
fn a_removed_normal_title_before_redraw_text_is_not_a_gap() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"\x1b]0;title\x07", t0 - Duration::from_secs(1));
    notify_at(&t, t0);
    t.feed_bytes_at(b"API Error", t0 + 10 * MS);
    let read = scan(&mut t, true);
    assert_eq!(ranges(&read), [(0, 9, "redraw")]);
    assert!(gaps(&read).is_empty());
}

/// Given 세 번에 나눠 읽기, When 가운데가 재그리기 escape뿐이면, Then 빈 text에 오프셋 0 gap이 있다.
#[test]
fn an_empty_text_keeps_the_gap_of_a_removed_redraw() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"API ", t0 - Duration::from_secs(1));
    let first = scan(&mut t, true);
    notify_at(&t, t0);
    t.feed_bytes_at(b"\x1b[2J", t0 + 100 * MS);
    let middle = scan(&mut t, true);
    t.feed_bytes_at(b"Error", t0 + Duration::from_secs(1));
    let last = scan(&mut t, true);
    assert_eq!(ranges(&first), [(0, 4, "normal")]);
    assert_eq!(middle.read.text, "");
    assert!(ranges(&middle).is_empty());
    assert_eq!(gaps(&middle), [(0, "redraw_removed")]);
    assert_eq!(ranges(&last), [(0, 5, "normal")]);
}

/// Given 일반 `C3`과 재그리기 `A9`, When strip 없이 읽으면, Then `é` 한 문자가 mixed다.
#[test]
fn a_character_split_between_origins_is_mixed() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"\xC3", t0 - Duration::from_secs(1));
    notify_at(&t, t0);
    t.feed_bytes_at(b"\xA9", t0 + 10 * MS);
    let read = scan(&mut t, false);
    assert_eq!(read.read.text, "é");
    assert_eq!(ranges(&read), [(0, 2, "mixed")]);
}

/// Given 일반 1바이트 뒤 재그리기 1 MiB, When 읽으면, Then 잃은 1바이트가 skipped이고 남은 것은 redraw다.
#[test]
fn trimming_reports_skipped_and_a_trimmed_gap() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"!", t0 - Duration::from_secs(1));
    notify_at(&t, t0);
    t.feed_bytes_at(&vec![b'r'; OUTPUT_RETENTION_MAX_BYTES], t0 + 10 * MS);
    let read = scan(&mut t, false);
    assert_eq!(read.read.skipped, 1);
    assert_eq!(read.read.cursor, 1);
    assert_eq!(read.read.retention_start, 1);
    assert_eq!(ranges(&read), [(0, OUTPUT_RETENTION_MAX_BYTES, "redraw")]);
    assert_eq!(gaps(&read), [(0, "trimmed")]);
}

/// Given 진행 중 적용, When 청크 뒤 성공하면, Then redraw다.
#[test]
fn a_chunk_during_a_successful_apply_is_redraw() {
    let mut t = idle_terminal();
    let t1 = Instant::now();
    let _before = t.lock_state().begin_os_resize(t1);
    t.feed_bytes_at(b"during", t1 + 10 * MS);
    t.lock_state().confirm_os_resize(t1 + 20 * MS);
    assert_eq!(ranges(&scan(&mut t, false)), [(0, 6, "redraw")]);
}

/// 계약 분류 기준 4의 750ms 예: 확정이 열기 + 700ms보다 늦으면 그 사이 청크도 확정 뒤 redraw다.
#[test]
fn a_chunk_after_the_open_window_but_before_a_late_confirm_is_redraw() {
    let mut t = idle_terminal();
    let t1 = Instant::now();
    let _before = t.lock_state().begin_os_resize(t1);
    t.feed_bytes_at(b"late", t1 + 720 * MS);
    t.lock_state().confirm_os_resize(t1 + 750 * MS);
    assert_eq!(ranges(&scan(&mut t, false)), [(0, 4, "redraw")]);
    assert_ne!(
        t.silence_since(),
        t1 + 720 * MS,
        "침묵 판정도 재그리기로 본다"
    );
}

/// Given 이전 구간이 없는 진행 중 적용, When 청크 뒤 실패하면, Then normal이다.
#[test]
fn a_chunk_during_a_failed_apply_is_normal() {
    let mut t = idle_terminal();
    let t1 = Instant::now();
    let before = t.lock_state().begin_os_resize(t1);
    t.feed_bytes_at(b"during", t1 + 10 * MS);
    t.lock_state().restore_os_resize(before);
    assert_eq!(ranges(&scan(&mut t, false)), [(0, 6, "normal")]);
}

/// Given 이전 구간 안의 진행 중 적용, When 청크 뒤 실패하면, Then 이전 구간 기준으로 redraw다.
#[test]
fn a_failed_apply_inside_an_earlier_window_keeps_redraw() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    notify_at(&t, t0);
    let before = t.lock_state().begin_os_resize(t0 + 100 * MS);
    t.feed_bytes_at(b"during", t0 + 200 * MS);
    t.lock_state().restore_os_resize(before);
    assert_eq!(ranges(&scan(&mut t, false)), [(0, 6, "redraw")]);
}

/// Given 진행 중 적용 동안 읽으면(지금 구조에서는 생기지 않는 방어 경로), Then 잠정 범위만 unknown이고
/// 확정 출처(적용 전 일반 청크, 이전 구간에 든 재그리기 청크)는 제 값이다.
#[test]
fn reading_during_an_apply_reports_only_pending_bytes_unknown() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"before", t0 - Duration::from_secs(1));
    notify_at(&t, t0);
    let _before = t.lock_state().begin_os_resize(t0 + Duration::from_secs(2));
    t.feed_bytes_at(b"old", t0 + 100 * MS);
    t.feed_bytes_at(b"new", t0 + Duration::from_secs(2) + 10 * MS);
    let read = scan(&mut t, false);
    assert_eq!(
        ranges(&read),
        [(0, 6, "normal"), (6, 9, "redraw"), (9, 12, "unknown")]
    );
}

/// Given 진행 중 청크만, When 읽으면, Then 정할 수 있는 바이트가 없어 text_origins를 뺀다.
#[test]
fn a_read_of_only_pending_bytes_omits_the_origins() {
    let mut t = idle_terminal();
    let t1 = Instant::now();
    let _before = t.lock_state().begin_os_resize(t1);
    t.feed_bytes_at(b"pending", t1 + 10 * MS);
    let read = scan(&mut t, false);
    assert_eq!(read.read.text, "pending");
    assert!(read.origins.is_none());
}

/// 연속성: 이어 읽으면 둘째 cursor가 첫째 next_cursor다. 버린 응답 뒤 읽기는 버린 응답의 next_cursor에서
/// 시작한다. 같은 surface의 Terminal 교체는 stream이 바뀌고 0부터다. mirror stream 갱신은 위치를 지킨다.
#[test]
fn continuity_fields_chain_across_reads() {
    let mut t = idle_terminal();
    t.process_bytes(b"one");
    let first = scan(&mut t, false).read;
    t.process_bytes(b"two");
    let dropped = scan(&mut t, false).read;
    t.process_bytes(b"three");
    let next = scan(&mut t, false).read;
    assert_eq!(dropped.cursor, first.next_cursor);
    assert_eq!(next.cursor, dropped.next_cursor);
    assert!(next.cursor > first.next_cursor);
    assert!(first.stream == dropped.stream && dropped.stream == next.stream);
    assert_eq!((next.cursor, next.next_cursor, next.raw_bytes), (6, 11, 5));

    t.lock_state().renew_output_stream();
    t.process_bytes(b"four");
    let renewed = scan(&mut t, false).read;
    assert_ne!(renewed.stream, next.stream);
    assert_eq!(renewed.cursor, next.next_cursor);

    let mut replaced = idle_terminal();
    replaced.process_bytes(b"x");
    let fresh = scan(&mut replaced, false).read;
    assert_ne!(fresh.stream, next.stream);
    assert_eq!(fresh.cursor, 0);
}

/// Given 청크 투입 뒤 같은 시각에 새 구간을 열면, When 읽으면, Then 그 청크는 normal이다. 지금 구간의 포함
/// 판정은 참이지만 투입 때 분류가 정의다.
#[test]
fn a_window_opened_at_the_same_instant_as_a_chunk_does_not_reclassify_it() {
    let mut t = idle_terminal();
    let at = Instant::now();
    t.feed_bytes_at(b"same", at);
    notify_at(&t, at);
    let current = t.lock_state().resize_redraw.expect("구간");
    assert!(ResizeRedraw::contains(
        current.output_start,
        current.end,
        at
    ));
    assert_eq!(ranges(&scan(&mut t, false)), [(0, 4, "normal")]);
}

/// Given 무효 UTF-8 조각 하나가 여러 원문 바이트를 소비하면, Then 그 문자는 원문 바이트들의 출처를 합친다.
#[test]
fn an_invalid_sequence_takes_the_origin_of_all_its_raw_bytes() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    t.feed_bytes_at(b"a\xF0\x9F", t0 - Duration::from_secs(1));
    notify_at(&t, t0);
    t.feed_bytes_at(b"\x98b", t0 + 10 * MS);
    let read = scan(&mut t, false);
    assert_eq!(read.read.text, "a\u{FFFD}b");
    assert_eq!(
        ranges(&read),
        [(0, 1, "normal"), (1, 4, "mixed"), (4, 5, "redraw")]
    );
}

/// Given 문자와 escape가 두 응답에 갈리면, Then 각 응답은 제 바이트만으로 분할을 지킨다.
#[test]
fn a_character_or_sequence_split_across_reads_maps_each_part() {
    let mut t = idle_terminal();
    t.process_bytes(b"x\xC3");
    let first = scan(&mut t, false);
    t.process_bytes(b"\xA9\x1b[");
    let second = scan(&mut t, true);
    t.process_bytes(b"2Jy");
    let third = scan(&mut t, true);
    assert_eq!(first.read.text, "x\u{FFFD}");
    assert_eq!(ranges(&first), [(0, 4, "normal")]);
    assert_eq!(second.read.text, "\u{FFFD}\x1b[");
    assert_eq!(third.read.text, "2Jy");
    assert_eq!(ranges(&third), [(0, 3, "normal")]);
}

/// Given 한 문자에 잠정(unknown) 바이트와 재그리기 바이트가 함께 들고 그 문자가 제거된 OSC 안이면, Then 그
/// 문자는 unknown이지만 원문에 확정 재그리기가 있어 gap이 남는다. unknown 바이트만 제거된 자리는 gap이 아니다.
#[test]
fn a_removed_sequence_with_redraw_bytes_is_a_gap_even_if_merged_unknown() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    notify_at(&t, t0);
    let _before = t.lock_state().begin_os_resize(t0 + Duration::from_secs(2));
    t.feed_bytes_at(b"\x1b]0;x\xC3", t0 + 100 * MS);
    t.feed_bytes_at(
        b"\xA9\x07rest\x1b[2J",
        t0 + Duration::from_secs(2) + 10 * MS,
    );
    let read = scan(&mut t, true);
    assert_eq!(read.read.text, "rest");
    assert_eq!(ranges(&read), [(0, 4, "unknown")]);
    assert_eq!(gaps(&read), [(0, "redraw_removed")]);
}

/// Given 한 재그리기 구간 안 `x ESC[2J` 반복으로 항목이 상한을 넘으면, Then text_origins만 빠지고 text와
/// 커서는 그대로다.
#[test]
fn too_many_entries_omit_the_origins_but_keep_the_text() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    notify_at(&t, t0);
    let repeated = b"x\x1b[2J".repeat(TEXT_ORIGINS_MAX_ENTRIES + 1);
    t.feed_bytes_at(&repeated, t0 + 10 * MS);
    let read = scan(&mut t, true);
    assert!(read.origins.is_none());
    assert_eq!(read.read.text, "x".repeat(TEXT_ORIGINS_MAX_ENTRIES + 1));
    assert_eq!(read.read.next_cursor, repeated.len() as u64);
    let fits = b"x\x1b[2J".repeat(TEXT_ORIGINS_MAX_ENTRIES - 1);
    t.feed_bytes_at(&fits, t0 + 20 * MS);
    let read = scan(&mut t, true);
    assert_eq!(gaps(&read).len(), TEXT_ORIGINS_MAX_ENTRIES - 1);
}

/// Given 출처가 청크마다 바뀌어 저장 구간이 상한을 넘으면, Then 오래된 범위만 unknown이고 뒤쪽 경계와
/// 확정 출처는 그대로다.
#[test]
fn the_stored_span_cap_turns_only_old_bytes_unknown() {
    let mut t = idle_terminal();
    let t0 = Instant::now();
    notify_at(&t, t0);
    let chunks = crate::output_origins::MAX_ORIGIN_SPANS + 4;
    for i in 0..chunks {
        let at = if i % 2 == 0 {
            t0 + 10 * MS
        } else {
            t0 + Duration::from_secs(1)
        };
        t.feed_bytes_at(b"ab", at);
    }
    let read = scan(&mut t, false);
    let r = ranges(&read);
    assert_eq!(r[0].2, "unknown");
    let tail: Vec<_> = r[r.len() - 4..].to_vec();
    let end = chunks * 2;
    assert_eq!(
        tail,
        [
            (end - 8, end - 6, "redraw"),
            (end - 6, end - 4, "normal"),
            (end - 4, end - 2, "redraw"),
            (end - 2, end, "normal"),
        ]
    );
}

/// 투입 분류와 침묵 판정이 같은 청크를 같은 쪽으로 가른다(마지막 청크 기준).
#[test]
fn ingest_classification_agrees_with_silence_since() {
    let t0 = Instant::now();
    let cases: [(&[u8], Option<Instant>, Instant); 4] = [
        (b"plain", None, t0),
        (b"in window", Some(t0), t0 + 100 * MS),
        (b"after window", Some(t0), t0 + 800 * MS),
        (b"at end", Some(t0), t0 + 700 * MS),
    ];
    for (bytes, notice, at) in cases {
        let mut t = idle_terminal();
        if let Some(n) = notice {
            notify_at(&t, n);
        }
        t.feed_bytes_at(bytes, at);
        let redraw_for_silence = t.silence_since() != at;
        let read = scan(&mut t, false);
        let expected = if redraw_for_silence {
            "redraw"
        } else {
            "normal"
        };
        assert_eq!(ranges(&read), [(0, bytes.len(), expected)], "{bytes:?}");
    }
}

/// 결정적 의사난수 바이트열: 새 경로의 text가 옛 식과 같고 분할 불변식을 지킨다.
#[test]
fn the_scan_text_matches_the_previous_formula_for_arbitrary_bytes() {
    const ALPHABET: &[u8] = b"\x1b\x1b[];\x07\\0aZ ?\xC3\xA9\xE3\x84\xB1\xF0\x9F\x98\x80\xFF\x80\n";
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let t0 = Instant::now();
    for round in 0..400 {
        let mut t = idle_terminal();
        notify_at(&t, t0);
        let mut raw = Vec::new();
        for _ in 0..(next() % 6 + 1) {
            let len = (next() % 24) as usize;
            let chunk: Vec<u8> = (0..len)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
                .collect();
            let at = if next() % 2 == 0 {
                t0 + 10 * MS
            } else {
                t0 + Duration::from_secs(1)
            };
            t.feed_bytes_at(&chunk, at);
            raw.extend_from_slice(&chunk);
        }
        let strip = round % 2 == 0;
        let decoded = String::from_utf8_lossy(&raw).to_string();
        let expected = if strip {
            tasty_ansi::strip_ansi(&decoded)
        } else {
            decoded
        };
        let read = scan(&mut t, strip);
        assert_eq!(read.read.text, expected, "{raw:?}");
        assert_eq!(read.read.raw_bytes, raw.len());
        if !raw.is_empty() {
            assert!(read.origins.is_some(), "확정 출처만 있는 읽기다: {raw:?}");
        }
    }
}
