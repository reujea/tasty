//! 한 프레임의 렌더 구간별 시간과 느린 프레임 경고의 빈도 제한.
//! 경고는 메인 스레드에서 파일 로그에 동기로 쓰이므로 프레임마다 내지 않고 구간 요약으로 묶는다.
//! 기준값과 줄 형식은 docs/dev-guide/gpu-rendering.md의 "회귀 임계값"과 같이 관리한다.

use std::time::{Duration, Instant};

/// 이 시간을 넘는 프레임을 느린 프레임으로 센다.
pub(crate) const SLOW_RENDER_MS: f64 = 30.0;
/// 이 시간 이상인 프레임은 요약을 기다리지 않고 바로 한 줄 남긴다.
pub(crate) const IMMEDIATE_RENDER_MS: f64 = 1000.0;
/// 느린 프레임 요약 한 줄이 덮는 구간 길이. 구간은 그 구간의 첫 느린 프레임에서 시작한다.
pub(crate) const SUMMARY_WINDOW: Duration = Duration::from_secs(60);

/// `render()` 한 번의 구간별 시간(ms). `other_ms`는 계측 구간 밖에 쓴 시간이다.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FrameBreakdown {
    pub total_ms: f64,
    pub layout_ms: f64,
    pub acquire_ms: f64,
    pub egui_frame_ms: f64,
    pub post_egui_ms: f64,
    pub platform_output_ms: f64,
    pub tessellate_ms: f64,
    pub clear_ms: f64,
    pub terminals_ms: f64,
    pub egui_pass_ms: f64,
    pub present_ms: f64,
}

const ITEM_COUNT: usize = 11;

impl FrameBreakdown {
    /// 총합에서 계측한 구간을 뺀 나머지. 새로 생긴 계측 밖 구간도 여기에 드러난다.
    pub fn other_ms(&self) -> f64 {
        let measured = self.layout_ms
            + self.acquire_ms
            + self.egui_frame_ms
            + self.post_egui_ms
            + self.platform_output_ms
            + self.tessellate_ms
            + self.clear_ms
            + self.terminals_ms
            + self.egui_pass_ms
            + self.present_ms;
        (self.total_ms - measured).max(0.0)
    }

    fn items(&self) -> [(&'static str, f64); ITEM_COUNT] {
        [
            ("layout", self.layout_ms),
            ("acquire", self.acquire_ms),
            ("egui_frame", self.egui_frame_ms),
            ("post_egui", self.post_egui_ms),
            ("platform_output", self.platform_output_ms),
            ("tessellate", self.tessellate_ms),
            ("clear", self.clear_ms),
            ("terminals", self.terminals_ms),
            ("egui_pass", self.egui_pass_ms),
            ("present", self.present_ms),
            ("other", self.other_ms()),
        ]
    }

    /// `layout=1.0, acquire=2.0, ..., other=0.3` 형식의 분해 문자열.
    pub fn describe(&self) -> String {
        describe_items(&self.items())
    }
}

fn describe_items(items: &[(&'static str, f64); ITEM_COUNT]) -> String {
    items
        .iter()
        .map(|(name, ms)| format!("{name}={ms:.1}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 열린 요약 구간.
#[derive(Debug)]
struct SummaryWindow {
    started: Instant,
    slow_frames: u64,
    worst: FrameBreakdown,
    item_max: [(&'static str, f64); ITEM_COUNT],
}

/// 한 프레임을 넣은 뒤 남길 줄. 둘 다 `None`이면 아무것도 쓰지 않는다.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SlowRenderLines {
    /// [`IMMEDIATE_RENDER_MS`] 이상인 이번 프레임의 줄.
    pub immediate: Option<String>,
    /// 끝난 구간의 요약 줄.
    pub summary: Option<String>,
}

/// 느린 프레임을 구간 단위로 모은다. 판정은 시각을 인자로 받아 시험할 수 있다.
#[derive(Debug, Default)]
pub(crate) struct SlowRenderLog {
    window: Option<SummaryWindow>,
}

impl SlowRenderLog {
    /// 모든 프레임마다 부른다. 끝난 구간의 요약은 구간이 끝난 뒤 처음 렌더한 프레임에서 나온다.
    pub fn observe(&mut self, now: Instant, frame: &FrameBreakdown) -> SlowRenderLines {
        let mut lines = SlowRenderLines::default();
        if self
            .window
            .as_ref()
            .is_some_and(|w| now.saturating_duration_since(w.started) >= SUMMARY_WINDOW)
            && let Some(done) = self.window.take()
        {
            lines.summary = Some(summary_line(&done, now));
        }
        if frame.total_ms <= SLOW_RENDER_MS {
            return lines;
        }
        if frame.total_ms >= IMMEDIATE_RENDER_MS {
            lines.immediate = Some(format!(
                "slow gpu render: {:.1}ms [{}]",
                frame.total_ms,
                frame.describe()
            ));
        }
        let items = frame.items();
        let window = self.window.get_or_insert(SummaryWindow {
            started: now,
            slow_frames: 0,
            worst: *frame,
            item_max: items,
        });
        window.slow_frames += 1;
        if frame.total_ms > window.worst.total_ms {
            window.worst = *frame;
        }
        for (max, (_, ms)) in window.item_max.iter_mut().zip(items) {
            max.1 = max.1.max(ms);
        }
        lines
    }

    /// 프레임 하나를 넣고 나온 줄을 warn으로 남긴다.
    pub fn record(&mut self, frame: &FrameBreakdown) {
        let lines = self.observe(Instant::now(), frame);
        if let Some(line) = lines.summary {
            tracing::warn!("{line}");
        }
        if let Some(line) = lines.immediate {
            tracing::warn!("{line}");
        }
    }
}

fn summary_line(window: &SummaryWindow, now: Instant) -> String {
    format!(
        "slow gpu render summary: {} frames over {SLOW_RENDER_MS:.0}ms in {:.1}s; \
         worst {:.1}ms [{}]; item max [{}]",
        window.slow_frames,
        now.saturating_duration_since(window.started).as_secs_f64(),
        window.worst.total_ms,
        window.worst.describe(),
        describe_items(&window.item_max),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(total_ms: f64) -> FrameBreakdown {
        FrameBreakdown {
            total_ms,
            layout_ms: 1.0,
            acquire_ms: 2.0,
            terminals_ms: 3.0,
            ..FrameBreakdown::default()
        }
    }

    #[test]
    fn a_minute_of_slow_frames_yields_one_immediate_line_and_one_summary() {
        let mut log = SlowRenderLog::default();
        let start = Instant::now();
        let mut immediate = Vec::new();
        let mut summaries = Vec::new();
        let mut push = |lines: SlowRenderLines| {
            immediate.extend(lines.immediate);
            summaries.extend(lines.summary);
        };
        for i in 0..1000u64 {
            let at = start + Duration::from_millis(i * 59);
            push(log.observe(at, &frame(40.0)));
            if i == 500 {
                push(log.observe(at, &frame(1200.0)));
            }
        }
        push(log.observe(start + SUMMARY_WINDOW, &frame(5.0)));

        assert_eq!(immediate.len(), 1, "{immediate:?}");
        assert!(immediate[0].starts_with("slow gpu render: 1200.0ms ["));
        assert_eq!(summaries.len(), 1, "{summaries:?}");
        assert!(
            summaries[0].starts_with("slow gpu render summary: 1001 frames over 30ms in 60.0s;"),
            "{}",
            summaries[0]
        );
        assert!(summaries[0].contains("worst 1200.0ms ["));
        assert!(summaries[0].contains("item max [layout=1.0, acquire=2.0,"));
        assert_eq!(
            log.observe(start + SUMMARY_WINDOW * 3, &frame(5.0)),
            SlowRenderLines::default()
        );
    }

    #[test]
    fn fast_frames_open_no_window() {
        let mut log = SlowRenderLog::default();
        let start = Instant::now();
        for i in 0..10u64 {
            let at = start + SUMMARY_WINDOW * i as u32;
            assert_eq!(
                log.observe(at, &frame(SLOW_RENDER_MS)),
                SlowRenderLines::default()
            );
        }
    }

    #[test]
    fn breakdown_names_acquire_and_other_and_sums_to_the_total() {
        let f = FrameBreakdown {
            total_ms: 67.7,
            layout_ms: 0.4,
            acquire_ms: 60.1,
            egui_frame_ms: 2.0,
            post_egui_ms: 0.1,
            platform_output_ms: 0.1,
            tessellate_ms: 0.5,
            clear_ms: 0.2,
            terminals_ms: 1.5,
            egui_pass_ms: 0.6,
            present_ms: 0.4,
        };
        let text = f.describe();
        assert!(text.contains("acquire=60.1"), "{text}");
        assert!(text.contains("other=1.8"), "{text}");
        let sum: f64 = f.items().iter().map(|(_, ms)| ms).sum();
        assert!(
            (sum - f.total_ms).abs() < 0.1,
            "sum {sum} vs total {}",
            f.total_ms
        );
    }
}
