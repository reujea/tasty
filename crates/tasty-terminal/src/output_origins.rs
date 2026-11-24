//! 보존 출력의 바이트마다 그 청크가 들어올 때 정한 출처(일반 출력인가, OS 크기 통지 뒤 재그리기인가).
//!
//! 분류는 청크를 투입할 때 정해 원문 옆에 남긴다. 읽을 때의 재그리기 구간으로 다시 가르면 구간이 다음 통지로
//! 새로 열리며 시작이 옮겨 가 앞 구간의 재그리기를 일반 출력으로 잘못 센다. 이 목록은 스캔 읽기
//! (`surface.read_since_scan_mark`)만 쓴다. 규칙: docs/design/policies/render-on-draw.md#적용이-일으킨-출력.

/// 저장 구간 수의 상한. 넘으면 가장 오래된 절반을 [`ChunkOrigin::Unknown`] 하나로 합친다. 출처는 재그리기
/// 구간 경계에서만 바뀌고 스캔 읽기가 읽은 앞부분을 지우므로 보통은 몇 개다.
pub(crate) const MAX_ORIGIN_SPANS: usize = 4096;

/// 청크 하나의 출처.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChunkOrigin {
    Normal,
    Redraw,
    /// 진행 중인 OS 크기 적용 동안 들어와, 적용이 성공하면 재그리기, 실패하면 일반 출력이 되는 청크.
    /// 적용을 확정하거나 되돌릴 때 같은 상태 락 안에서 둘 중 하나로 바뀐다.
    Pending,
    /// 정할 수 없다(구간 수 상한으로 합친 범위).
    Unknown,
}

/// 원문 절대 위치 기준 출처 구간. 구간 `i`는 `[spans[i].0, spans[i + 1].0)`, 마지막은 버퍼 끝까지다.
///
/// 불변식: 시작 위치는 순증가하고 이웃한 두 구간의 출처는 다르다. 덮어야 할 범위의 첫 바이트를 포함하는
/// 구간은 그 바이트보다 앞에서 시작해도 지우지 않는다(첫 바이트의 출처를 잃지 않게).
#[derive(Debug, Default)]
pub(crate) struct OriginSpans {
    spans: Vec<(u64, ChunkOrigin)>,
}

impl OriginSpans {
    /// `at`부터 들어오는 바이트의 출처를 `origin`으로 적는다. `at`은 마지막 구간 시작 이상이다.
    pub(crate) fn push(&mut self, at: u64, origin: ChunkOrigin) {
        match self.spans.last_mut() {
            Some((_, last)) if *last == origin => return,
            Some((start, last)) if *start == at => {
                // 바이트 없이 남은 구간이라 출처만 바꾼다.
                *last = origin;
                self.merge_neighbours();
            }
            _ => self.spans.push((at, origin)),
        }
        if self.spans.len() > MAX_ORIGIN_SPANS {
            self.collapse_oldest_half();
        }
    }

    /// `pos` 앞에서 끝나는 구간을 지운다. `pos`를 포함하는 구간은 남긴다.
    pub(crate) fn drop_before(&mut self, pos: u64) {
        let dead = self.spans.iter().skip(1).take_while(|s| s.0 <= pos).count();
        self.spans.drain(..dead);
    }

    /// 진행 중이던 청크의 출처를 정한다.
    pub(crate) fn settle_pending(&mut self, to: ChunkOrigin) {
        let mut changed = false;
        for (_, origin) in &mut self.spans {
            if *origin == ChunkOrigin::Pending {
                *origin = to;
                changed = true;
            }
        }
        if changed {
            self.merge_neighbours();
        }
    }

    /// `[from, to)`를 덮는 구간의 복사본. 첫 구간의 시작은 `from`으로 당기고, 진행 중 청크는 지금은 정할 수
    /// 없으므로 [`ChunkOrigin::Unknown`]으로 낸다.
    pub(crate) fn slice(&self, from: u64, to: u64) -> Vec<(u64, ChunkOrigin)> {
        let first = self.spans.iter().rposition(|s| s.0 <= from).unwrap_or(0);
        let mut out: Vec<(u64, ChunkOrigin)> = Vec::new();
        for &(start, origin) in self.spans[first..].iter().take_while(|s| s.0 < to) {
            let origin = match origin {
                ChunkOrigin::Pending => ChunkOrigin::Unknown,
                other => other,
            };
            if out.last().is_some_and(|l| l.1 == origin) {
                continue;
            }
            out.push((start.max(from), origin));
        }
        out
    }

    fn merge_neighbours(&mut self) {
        self.spans.dedup_by(|later, earlier| later.1 == earlier.1);
    }

    fn collapse_oldest_half(&mut self) {
        let half = self.spans.len() / 2;
        self.spans[0].1 = ChunkOrigin::Unknown;
        self.spans.drain(1..half);
        self.merge_neighbours();
    }

    #[cfg(test)]
    pub(crate) fn spans(&self) -> &[(u64, ChunkOrigin)] {
        &self.spans
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkOrigin::{Normal, Pending, Redraw, Unknown};
    use super::*;

    /// Given 같은 출처가 이어지면, When 적으면, Then 한 구간으로 합친다.
    #[test]
    fn neighbours_with_the_same_origin_are_one_span() {
        let mut s = OriginSpans::default();
        s.push(0, Normal);
        s.push(4, Normal);
        s.push(8, Redraw);
        s.push(9, Normal);
        assert_eq!(s.spans(), &[(0, Normal), (8, Redraw), (9, Normal)]);
    }

    /// Given 진행 중 청크 양옆이 재그리기, When 성공으로 정하면, Then 이웃과 합친다.
    #[test]
    fn settling_pending_merges_the_neighbours() {
        let mut s = OriginSpans::default();
        s.push(0, Redraw);
        s.push(3, Pending);
        s.push(5, Redraw);
        s.settle_pending(Redraw);
        assert_eq!(s.spans(), &[(0, Redraw)]);
        let mut s = OriginSpans::default();
        s.push(0, Normal);
        s.push(3, Pending);
        s.settle_pending(Normal);
        assert_eq!(s.spans(), &[(0, Normal)]);
    }

    /// Given 여러 구간, When 앞을 지우면, Then 지운 자리를 포함하는 구간은 남는다.
    #[test]
    fn dropping_keeps_the_span_covering_the_position() {
        let mut s = OriginSpans::default();
        s.push(0, Normal);
        s.push(10, Redraw);
        s.push(20, Normal);
        s.drop_before(15);
        assert_eq!(s.spans(), &[(10, Redraw), (20, Normal)]);
        s.drop_before(20);
        assert_eq!(s.spans(), &[(20, Normal)]);
        s.drop_before(100);
        assert_eq!(s.spans(), &[(20, Normal)]);
    }

    /// Given 출처가 바이트마다 바뀌는 출력, When 상한을 넘으면, Then 오래된 절반이 Unknown 하나가 된다.
    #[test]
    fn too_many_spans_turn_the_oldest_half_unknown() {
        let mut s = OriginSpans::default();
        for i in 0..=MAX_ORIGIN_SPANS as u64 {
            s.push(i, if i % 2 == 0 { Normal } else { Redraw });
        }
        assert!(s.spans().len() <= MAX_ORIGIN_SPANS);
        assert_eq!(s.spans()[0], (0, Unknown));
        assert_ne!(s.spans()[1].1, Unknown);
        assert!(
            s.spans()
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 != w[1].1)
        );
    }
}
