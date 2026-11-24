//! 스캔 읽기가 돌려준 `text`의 바이트 범위마다 원문 청크의 출처를 대응시킨다.
//!
//! `text`는 지금과 같은 값(`String::from_utf8_lossy` 한 번, `strip_ansi`면 [`tasty_ansi::strip_ansi`]와 같은
//! 결과)이고, 대응과 함께 한 번의 순회로 만든다. 원문 바이트를 `utf8_chunks`로 돌면 디코드 문자마다 원문
//! 범위가 나온다(유효 문자는 같은 바이트, 비어 있지 않은 무효 조각 하나는 `U+FFFD` 하나). 지운 escape는
//! 같은 정규식의 매치([`tasty_ansi::ansi_escape_ranges`])를 앞에서부터 하나씩 꺼내 쓰고 모아 두지 않는다.
//! 응답 모양: docs/reference/api.md.

use crate::output_origins::ChunkOrigin;

/// `ranges`와 `gaps` 항목 수의 합 상한. 넘으면 출처 정보를 통째로 뺀다. 항목 하나가 JSON으로 60바이트
/// 안팎이라 출처가 응답에 더하는 크기를 약 240 KiB로 묶는다. 정상 출력은 재그리기 구간 경계에서만 출처가
/// 바뀌어 몇 개다.
pub const TEXT_ORIGINS_MAX_ENTRIES: usize = 4096;

/// `text` 한 범위의 출처.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOrigin {
    /// 일반 출력.
    Normal,
    /// OS 크기 통지 뒤 재그리기 출력.
    Redraw,
    /// 한 문자에 일반 바이트와 재그리기 바이트가 함께 들었다.
    Mixed,
    /// 출처를 정할 수 없는 바이트가 들었다.
    Unknown,
}

impl TextOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Redraw => "redraw",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }
}

/// 폭 0 표시의 사유.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapReason {
    /// 보존 한도로 앞이 잘려 읽지 못한 원문이 있다(언제나 오프셋 0).
    Trimmed,
    /// ANSI 제거로 사라진 escape의 원문에 재그리기 바이트가 있었다.
    RedrawRemoved,
}

impl GapReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trimmed => "trimmed",
            Self::RedrawRemoved => "redraw_removed",
        }
    }
}

/// `text`의 UTF-8 바이트 범위 `[start, end)`와 그 출처.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginRange {
    pub start: usize,
    pub end: usize,
    pub origin: TextOrigin,
}

/// `text`의 오프셋 `at`(문자 경계)에 있는 폭 0 표시.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginGap {
    pub at: usize,
    pub reason: GapReason,
}

/// 스캔 읽기 `text`의 출처. `ranges`는 `text`를 빈틈없이 순서대로 덮고 이웃한 같은 출처는 합친다.
/// `gaps`는 오프셋 순서다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextOrigins {
    pub ranges: Vec<OriginRange>,
    pub gaps: Vec<OriginGap>,
}

/// 원문 조각 `raw`(절대 위치 `base`부터)와 그 범위의 출처 구간 `spans`(진행 중 청크는 이미 `Unknown`)로
/// 스캔 읽기 `text`와 그 대응을 만든다. `decoded`는 `raw`의 `from_utf8_lossy` 결과다. 출처를 정할 수 있는
/// 원문 바이트가 하나도 없거나 항목 수가 [`TEXT_ORIGINS_MAX_ENTRIES`]를 넘으면 대응은 None이고 `text`는
/// 그래도 끝까지 만든다.
pub(crate) fn map_text_origins(
    raw: &[u8],
    base: u64,
    spans: &[(u64, ChunkOrigin)],
    decoded: String,
    strip_ansi: bool,
    trimmed: bool,
) -> (String, Option<TextOrigins>) {
    if !raw.is_empty() && spans.iter().all(|s| s.1 == ChunkOrigin::Unknown) {
        let text = if strip_ansi {
            tasty_ansi::strip_ansi(&decoded)
        } else {
            decoded
        };
        return (text, None);
    }
    let (stripped, origins) = walk(raw, base, spans, &decoded, strip_ansi, trimmed);
    (stripped.unwrap_or(decoded), origins)
}

/// 원문을 한 번 돌며 지운 escape를 빼고 남긴 `text`(`strip_ansi`일 때만)와 대응을 만든다. 매치 반복자가
/// `decoded`를 빌리므로 따로 둬, 호출한 쪽이 strip하지 않을 때 `decoded`를 그대로 `text`로 넘기게 한다.
fn walk(
    raw: &[u8],
    base: u64,
    spans: &[(u64, ChunkOrigin)],
    decoded: &str,
    strip_ansi: bool,
    trimmed: bool,
) -> (Option<String>, Option<TextOrigins>) {
    let mut matches = strip_ansi
        .then(|| tasty_ansi::ansi_escape_ranges(decoded))
        .into_iter()
        .flatten();
    let mut m = Mapper {
        spans: SpanCursor {
            spans,
            base,
            idx: 0,
        },
        current: matches.next(),
        matches,
        decoded,
        match_has_redraw: false,
        out: 0,
        text: strip_ansi.then(|| String::with_capacity(decoded.len())),
        origins: Some(TextOrigins::default()),
    };
    if trimmed {
        m.record_gap(GapReason::Trimmed);
    }
    let (mut raw_at, mut decoded_at) = (0usize, 0usize);
    for chunk in raw.utf8_chunks() {
        for c in chunk.valid().chars() {
            let len = c.len_utf8();
            m.char(decoded_at, len, raw_at..raw_at + len);
            raw_at += len;
            decoded_at += len;
        }
        let invalid = chunk.invalid().len();
        if invalid > 0 {
            let len = char::REPLACEMENT_CHARACTER.len_utf8();
            m.char(decoded_at, len, raw_at..raw_at + invalid);
            raw_at += invalid;
            decoded_at += len;
        }
    }
    debug_assert_eq!(
        decoded_at,
        decoded.len(),
        "원문 순회가 from_utf8_lossy와 어긋났다"
    );
    m.close_matches_before(usize::MAX);
    (m.text.take(), m.origins.take())
}

/// 원문 위치가 늘어나는 순서로만 묻는 출처 구간 조회.
struct SpanCursor<'a> {
    spans: &'a [(u64, ChunkOrigin)],
    base: u64,
    idx: usize,
}

impl SpanCursor<'_> {
    /// 원문 상대 범위 `raw`에 든 바이트들의 출처를 합친 값과 확정 재그리기 바이트가 있는지.
    fn origin_of(&mut self, raw: std::ops::Range<usize>) -> (TextOrigin, bool) {
        let (from, to) = (self.base + raw.start as u64, self.base + raw.end as u64);
        while self.spans.get(self.idx + 1).is_some_and(|s| s.0 <= from) {
            self.idx += 1;
        }
        let (mut normal, mut redraw, mut unknown) = (false, false, false);
        // 구간이 덮지 않는 앞부분은 출처를 모른다(불변식상 생기지 않는 방어).
        if self.spans.first().is_none_or(|s| s.0 > from) {
            unknown = true;
        }
        for span in self.spans[self.idx..].iter().take_while(|s| s.0 < to) {
            match span.1 {
                ChunkOrigin::Normal => normal = true,
                ChunkOrigin::Redraw => redraw = true,
                ChunkOrigin::Pending | ChunkOrigin::Unknown => unknown = true,
            }
        }
        let merged = match (unknown, normal, redraw) {
            (true, _, _) => TextOrigin::Unknown,
            (false, true, true) => TextOrigin::Mixed,
            (false, false, true) => TextOrigin::Redraw,
            _ => TextOrigin::Normal,
        };
        (merged, redraw)
    }
}

struct Mapper<'a, M> {
    spans: SpanCursor<'a>,
    /// 아직 닫지 않은 escape 매치. `matches`에서 앞에서부터 하나씩 꺼낸다.
    current: Option<std::ops::Range<usize>>,
    matches: M,
    decoded: &'a str,
    /// 지금 지나는 escape 매치의 원문에 확정 재그리기 바이트가 있었는가.
    match_has_redraw: bool,
    /// 지금까지 남긴 `text` 바이트 수.
    out: usize,
    /// strip할 때 남긴 문자를 모은 `text`.
    text: Option<String>,
    /// 항목 상한을 넘으면 None이 되고 그 뒤로는 기록하지 않는다.
    origins: Option<TextOrigins>,
}

impl<M: Iterator<Item = std::ops::Range<usize>>> Mapper<'_, M> {
    /// 디코드 오프셋 `at`에서 시작하는 `len` 바이트 문자 하나(원문 범위 `raw`).
    fn char(&mut self, at: usize, len: usize, raw: std::ops::Range<usize>) {
        self.close_matches_before(at);
        let (origin, has_redraw) = self.spans.origin_of(raw);
        if self.current.as_ref().is_some_and(|r| r.start <= at) {
            self.match_has_redraw |= has_redraw;
            return;
        }
        if let Some(text) = &mut self.text {
            text.push_str(&self.decoded[at..at + len]);
        }
        let start = self.out;
        self.out += len;
        let end = self.out;
        if let Some(origins) = &mut self.origins {
            match origins.ranges.last_mut() {
                Some(last) if last.origin == origin && last.end == start => last.end = end,
                _ => origins.ranges.push(OriginRange { start, end, origin }),
            }
        }
        self.enforce_limit();
    }

    /// 디코드 오프셋 `at` 전에 끝난 escape 매치를 닫는다. 재그리기 바이트가 있었으면 지금 출력 자리에 표시한다.
    fn close_matches_before(&mut self, at: usize) {
        while self.current.as_ref().is_some_and(|r| r.end <= at) {
            self.current = self.matches.next();
            if std::mem::take(&mut self.match_has_redraw) {
                self.record_gap(GapReason::RedrawRemoved);
            }
        }
    }

    /// 지금 출력 자리에 표시를 더한다. 바로 앞 표시와 같으면 겹쳐 적지 않는다.
    fn record_gap(&mut self, reason: GapReason) {
        let gap = OriginGap {
            at: self.out,
            reason,
        };
        if let Some(origins) = &mut self.origins
            && origins.gaps.last() != Some(&gap)
        {
            origins.gaps.push(gap);
        }
        self.enforce_limit();
    }

    fn enforce_limit(&mut self) {
        if self
            .origins
            .as_ref()
            .is_some_and(|o| o.ranges.len() + o.gaps.len() > TEXT_ORIGINS_MAX_ENTRIES)
        {
            self.origins = None;
        }
    }
}
