//! 일정 프레임마다 렌더 시간·draw call·atlas 사용량을 기록한다.
//! TASTY_LOG=tasty::gfx::perf=info로 켜며 기본 필터에서는 출력하지 않는다.

use std::collections::VecDeque;

pub(crate) fn wheel_received() {
    tracing::trace!(target: "tasty::explorer::perf", "wheel_received");
}

const WINDOW: usize = 300;
const DUMP_EVERY: u32 = 300;

#[derive(Clone, Copy, Debug)]
pub struct FrameSample {
    pub gpu_total_ms: f64,
    pub terminals_ms: f64,
    /// `surface.get_current_texture()` 대기 시간.
    pub acquire_ms: f64,
    pub draw_calls_total: u32,
    pub surfaces: u32,
    pub atlas_evictions: u64,
    pub atlas_active_pages: u32,
    pub atlas_entry_count_sum: u32,
}

pub struct PerfAggregator {
    frames: VecDeque<FrameSample>,
    frames_since_dump: u32,
}

impl PerfAggregator {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(WINDOW),
            frames_since_dump: 0,
        }
    }

    pub fn push(&mut self, s: FrameSample) {
        tracing::trace!(
            target: "tasty::gfx::perf",
            gpu_total_ms = s.gpu_total_ms,
            terminals_ms = s.terminals_ms,
            acquire_ms = s.acquire_ms,
            surfaces = s.surfaces,
            "frame_sample"
        );
        if self.frames.len() == WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(s);
        self.frames_since_dump += 1;
        if self.frames_since_dump >= DUMP_EVERY {
            self.dump();
            self.frames_since_dump = 0;
        }
    }

    fn dump(&self) {
        if self.frames.is_empty() {
            return;
        }
        let (tp50, tp99, tmax) = percentiles(&self.frames, |s| s.terminals_ms);
        let (gp50, gp99, gmax) = percentiles(&self.frames, |s| s.gpu_total_ms);
        let (ap50, ap99, amax) = percentiles(&self.frames, |s| s.acquire_ms);
        let last = *self.frames.back().unwrap();
        tracing::info!(
            target: "tasty::gfx::perf",
            "perf n={} surfaces={} draws={} terminals_ms p50={:.2} p99={:.2} max={:.2} \
             gpu_total_ms p50={:.2} p99={:.2} max={:.2} \
             acquire_ms p50={:.2} p99={:.2} max={:.2} \
             atlas_evictions={} atlas_pages={} atlas_entries={}",
            self.frames.len(),
            last.surfaces,
            last.draw_calls_total,
            tp50,
            tp99,
            tmax,
            gp50,
            gp99,
            gmax,
            ap50,
            ap99,
            amax,
            last.atlas_evictions,
            last.atlas_active_pages,
            last.atlas_entry_count_sum,
        );
    }
}

impl Default for PerfAggregator {
    fn default() -> Self {
        Self::new()
    }
}

fn percentiles<F: Fn(&FrameSample) -> f64>(buf: &VecDeque<FrameSample>, sel: F) -> (f64, f64, f64) {
    let mut v: Vec<f64> = buf.iter().map(sel).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    let p50 = v[n / 2];
    let p99_idx = ((n * 99) / 100).min(n - 1);
    let p99 = v[p99_idx];
    let max = *v.last().unwrap();
    (p50, p99, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(g: f64, t: f64) -> FrameSample {
        FrameSample {
            gpu_total_ms: g,
            terminals_ms: t,
            acquire_ms: 0.0,
            draw_calls_total: 20,
            surfaces: 10,
            atlas_evictions: 0,
            atlas_active_pages: 1,
            atlas_entry_count_sum: 0,
        }
    }

    #[test]
    fn percentiles_sorted_window() {
        let mut buf: VecDeque<FrameSample> = VecDeque::new();
        for i in 1..=100 {
            buf.push_back(mk(i as f64, i as f64));
        }
        let (p50, p99, max) = percentiles(&buf, |s| s.gpu_total_ms);
        assert_eq!(p50, 51.0);
        assert_eq!(p99, 100.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn aggregator_caps_window() {
        let mut agg = PerfAggregator::new();
        for i in 0..(WINDOW + 50) {
            agg.push(mk(i as f64, i as f64));
        }
        assert_eq!(agg.frames.len(), WINDOW);
    }
}
