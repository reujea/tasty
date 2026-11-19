//! 일정 프레임마다 렌더 구간별 시간·인스턴스 수·바뀐 양·draw call·atlas 사용량을 기록한다.
//! TASTY_LOG=tasty::gfx::perf=info로 켜며 기본 필터에서는 출력하지 않는다.
//! 구간·잠금 시간은 [`enabled`](info)로, 바뀐 칸 집계는 [`changes_enabled`](trace)로 켠다.
//! 칸 서명은 화면 잠금 안에서 계산되므로 info만 켜면 구간·잠금 시간을 서명 비용 없이 잰다.

use crate::gfx::renderer::FrameChanges;
use crate::perf_stats::{Percentiles, SampleWindow};
use tasty_terminal::lock_timing::LockSiteStats;

pub(crate) fn wheel_received() {
    tracing::trace!(target: "tasty::explorer::perf", "wheel_received");
}

const WINDOW: usize = 300;
const DUMP_EVERY: u32 = 300;

/// perf 집계가 로그로 나가는가. debug 빌드는 파일 로그가 debug 이상을 받으므로 늘 참이다.
pub(crate) fn enabled() -> bool {
    tracing::enabled!(target: "tasty::gfx::perf", tracing::Level::INFO)
}

/// 바뀐 칸 집계(직전 프레임과의 칸 서명 비교)를 하는가. `tasty::gfx::perf=trace`일 때만 참이다.
/// 서명을 화면 잠금 안에서 계산하므로 켜진 동안의 구간·잠금 시간에는 서명 비용이 들어 있다.
pub(crate) fn changes_enabled() -> bool {
    tracing::enabled!(target: "tasty::gfx::perf", tracing::Level::TRACE)
}

/// `GpuState::render` 한 번의 값. 시간은 모두 CPU 벽시계(ms)다.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSample {
    pub gpu_total_ms: f64,
    pub terminals_ms: f64,
    /// `surface.get_current_texture()` 대기 시간.
    pub acquire_ms: f64,
    pub egui_frame_ms: f64,
    /// egui 프레임 뒤 테마 재적용(`apply_theme_to_egui`)과 터미널 글꼴 갱신.
    pub post_egui_ms: f64,
    pub tessellate_ms: f64,
    pub clear_ms: f64,
    pub egui_pass_ms: f64,
    pub present_ms: f64,
    /// `terminals_ms` 중 인스턴스를 쌓는 구간(`begin_frame`부터 마지막 `append_terminal_viewport`까지).
    pub terminals_build_ms: f64,
    /// `terminals_ms` 중 버퍼 업로드·draw 기록·제출 구간.
    pub terminals_submit_ms: f64,
    /// WebView 오버레이 창의 별도 acquire 대기. `acquire_ms`에 들어가지 않는다.
    pub overlay_acquire_ms: f64,
    pub bg_instances: u32,
    pub glyph_instances: u32,
    /// 직전 프레임과 바뀐 양. 계측이 꺼진 프레임은 `None`이다.
    pub changes: Option<FrameChanges>,
    pub draw_calls_total: u32,
    pub surfaces: u32,
    pub atlas_evictions: u64,
    pub atlas_active_pages: u32,
    pub atlas_entry_count_sum: u32,
}

/// redraw 콜백 안이지만 `GpuState::render` 밖에서 쓴 시간(ms).
#[derive(Clone, Copy, Debug, Default)]
pub struct RedrawShellSample {
    /// 공용 목록 동기화와 `prepare_redraw`.
    pub prepare_ms: f64,
    /// 구독 중인 mesh surface 중계.
    pub mesh_relay_ms: f64,
    /// WebView 동기화와 오버레이 host 맞춤.
    pub webview_sync_ms: f64,
}

/// perf 줄의 필드 이름과 표본에서 그 값을 고르는 함수.
type NamedField = (&'static str, fn(&FrameSample) -> f64);

pub struct PerfAggregator {
    frames: SampleWindow<FrameSample>,
    frames_since_dump: u32,
    shells: SampleWindow<RedrawShellSample>,
}

impl PerfAggregator {
    pub fn new() -> Self {
        Self {
            frames: SampleWindow::new(WINDOW),
            frames_since_dump: 0,
            shells: SampleWindow::new(WINDOW),
        }
    }

    /// 한 프레임의 표본을 더한다. 계측이 꺼져 있으면 보관·집계·문자열 생성을 하지 않는다.
    pub fn push(&mut self, s: FrameSample) {
        self.record(s, enabled());
    }

    /// 켜짐 판정을 받아 기록한다. 이번 호출이 집계 줄을 냈으면 `true`다.
    fn record(&mut self, s: FrameSample, enabled: bool) -> bool {
        if !enabled {
            return false;
        }
        let changes = s.changes.unwrap_or_default();
        tracing::trace!(
            target: "tasty::gfx::perf",
            gpu_total_ms = s.gpu_total_ms,
            terminals_ms = s.terminals_ms,
            terminals_build_ms = s.terminals_build_ms,
            terminals_submit_ms = s.terminals_submit_ms,
            acquire_ms = s.acquire_ms,
            overlay_acquire_ms = s.overlay_acquire_ms,
            egui_frame_ms = s.egui_frame_ms,
            post_egui_ms = s.post_egui_ms,
            tessellate_ms = s.tessellate_ms,
            clear_ms = s.clear_ms,
            egui_pass_ms = s.egui_pass_ms,
            present_ms = s.present_ms,
            surfaces = s.surfaces,
            bg_instances = s.bg_instances,
            glyph_instances = s.glyph_instances,
            changes_tracked = s.changes.is_some(),
            changed_surfaces = changes.surfaces,
            changed_cells = changes.cells,
            changed_instance_bytes = changes.instance_bytes,
            "frame_sample"
        );
        self.frames.push(s);
        self.frames_since_dump += 1;
        if self.frames_since_dump >= DUMP_EVERY {
            if let Some(line) = self.frame_line() {
                tracing::info!(target: "tasty::gfx::perf", "{line}");
            }
            if let Some(line) = self.shell_line() {
                tracing::info!(target: "tasty::gfx::perf", "{line}");
            }
            tracing::info!(
                target: "tasty::gfx::perf",
                "{}",
                lock_line(&tasty_terminal::lock_timing::take())
            );
            self.frames_since_dump = 0;
            return true;
        }
        false
    }

    /// redraw 콜백 하나의 렌더 밖 시간을 더한다. 다음 frame 줄과 함께 나간다.
    /// 계측이 꺼져 있으면 보관하지 않는다.
    pub fn push_shell(&mut self, s: RedrawShellSample) {
        self.record_shell(s, enabled());
    }

    fn record_shell(&mut self, s: RedrawShellSample, enabled: bool) {
        if enabled {
            self.shells.push(s);
        }
    }

    fn frame_line(&self) -> Option<String> {
        let last = *self.frames.last()?;
        let p = |sel: fn(&FrameSample) -> f64| self.frames.percentiles(sel);
        let terminals = p(|s| s.terminals_ms);
        let total = p(|s| s.gpu_total_ms);
        let acquire = p(|s| s.acquire_ms);
        let mut line = format!(
            "perf n={} surfaces={} draws={} terminals_ms {} gpu_total_ms {} acquire_ms {} \
             atlas_evictions={} atlas_pages={} atlas_entries={}",
            self.frames.len(),
            last.surfaces,
            last.draw_calls_total,
            ms3(terminals),
            ms3(total),
            ms3(acquire),
            last.atlas_evictions,
            last.atlas_active_pages,
            last.atlas_entry_count_sum,
        );
        let stages: [NamedField; 9] = [
            ("terminals_build_ms", |s| s.terminals_build_ms),
            ("terminals_submit_ms", |s| s.terminals_submit_ms),
            ("egui_frame_ms", |s| s.egui_frame_ms),
            ("post_egui_ms", |s| s.post_egui_ms),
            ("tessellate_ms", |s| s.tessellate_ms),
            ("clear_ms", |s| s.clear_ms),
            ("egui_pass_ms", |s| s.egui_pass_ms),
            ("present_ms", |s| s.present_ms),
            ("overlay_acquire_ms", |s| s.overlay_acquire_ms),
        ];
        for (name, sel) in stages {
            line.push_str(&format!(" {name} {}", ms2(p(sel))));
        }
        let counts: [NamedField; 2] = [
            ("bg_instances", |s| f64::from(s.bg_instances)),
            ("glyph_instances", |s| f64::from(s.glyph_instances)),
        ];
        for (name, sel) in counts {
            line.push_str(&format!(" {name} {}", count3(p(sel))));
        }
        line.push_str(&self.changes_part());
        Some(line)
    }

    /// 바뀐 양의 분포. 창 안에 계측이 꺼진 프레임이 있으면 켜진 프레임만 세고 그 수를 함께 적는다.
    fn changes_part(&self) -> String {
        let tracked: Vec<FrameChanges> = self.frames.iter().filter_map(|s| s.changes).collect();
        if tracked.is_empty() {
            return " changes=untracked".to_string();
        }
        let of = |sel: fn(&FrameChanges) -> f64| {
            let mut values: Vec<f64> = tracked.iter().map(sel).collect();
            Percentiles::of(&mut values)
        };
        format!(
            " changes_n={} changed_surfaces {} changed_cells {} changed_instance_bytes {}",
            tracked.len(),
            count3(of(|c| f64::from(c.surfaces))),
            count3(of(|c| f64::from(c.cells))),
            count3(of(|c| c.instance_bytes as f64)),
        )
    }

    fn shell_line(&self) -> Option<String> {
        if self.shells.is_empty() {
            return None;
        }
        let p = |sel: fn(&RedrawShellSample) -> f64| self.shells.percentiles(sel);
        Some(format!(
            "perf_redraw n={} prepare_ms {} mesh_relay_ms {} webview_sync_ms {}",
            self.shells.len(),
            ms3(p(|s| s.prepare_ms)),
            ms3(p(|s| s.mesh_relay_ms)),
            ms3(p(|s| s.webview_sync_ms)),
        ))
    }
}

impl Default for PerfAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// 직전 dump 뒤 TerminalState 잠금 누계. 모든 터미널과 모든 창을 합친 값이라 창마다 줄이 나오면
/// 먼저 dump한 창이 누계를 가져간다.
fn lock_line(sites: &[(tasty_terminal::lock_timing::LockSite, LockSiteStats)]) -> String {
    let mut line = String::from("perf_lock");
    for (site, stats) in sites {
        let name = match site {
            tasty_terminal::lock_timing::LockSite::Ingest => "ingest",
            tasty_terminal::lock_timing::LockSite::View => "view",
        };
        if stats.count == 0 {
            // 표본이 없으면 평균·최댓값을 0으로 쓰지 않는다. 0은 실제로 잰 값과 구별되지 않는다.
            line.push_str(&format!(
                " {name} n=0 wait_us mean=none max=none hold_us mean=none max=none"
            ));
            continue;
        }
        let mean_us = |sum: u64| sum as f64 / stats.count as f64 / 1000.0;
        line.push_str(&format!(
            " {name} n={} wait_us mean={:.1} max={:.1} hold_us mean={:.1} max={:.1}",
            stats.count,
            mean_us(stats.wait_ns_sum),
            stats.wait_ns_max as f64 / 1000.0,
            mean_us(stats.hold_ns_sum),
            stats.hold_ns_max as f64 / 1000.0,
        ));
    }
    line
}

fn ms3(p: Percentiles) -> String {
    format!("p50={:.2} p99={:.2} max={:.2}", p.p50, p.p99, p.max)
}

fn ms2(p: Percentiles) -> String {
    format!("p50={:.2} p99={:.2}", p.p50, p.p99)
}

fn count3(p: Percentiles) -> String {
    format!("p50={:.0} p99={:.0} max={:.0}", p.p50, p.p99, p.max)
}

#[cfg(test)]
mod tests {
    use super::*;

    impl PerfAggregator {
        /// 시험 프로세스에는 로그 구독자가 없어 [`enabled`]가 거짓이므로 켜진 상태를 직접 넘긴다.
        fn push_enabled(&mut self, s: FrameSample) {
            self.record(s, true);
        }

        fn push_shell_enabled(&mut self, s: RedrawShellSample) {
            self.record_shell(s, true);
        }
    }

    /// 계측이 꺼져 있으면 표본을 보관하지 않고 집계 줄도 만들지 않는다.
    #[test]
    fn disabled_aggregator_keeps_no_samples_and_never_dumps() {
        let mut agg = PerfAggregator::new();
        for i in 0..(DUMP_EVERY * 2) {
            assert!(!agg.record(mk(f64::from(i), 1.0), false));
            agg.record_shell(RedrawShellSample::default(), false);
        }
        assert_eq!(agg.frames.len(), 0);
        assert_eq!(agg.shells.len(), 0);
        assert_eq!(agg.frames_since_dump, 0);

        let dumps = (0..DUMP_EVERY)
            .filter(|i| agg.record(mk(f64::from(*i), 1.0), true))
            .count();
        assert_eq!(dumps, 1);
    }

    fn mk(g: f64, t: f64) -> FrameSample {
        FrameSample {
            gpu_total_ms: g,
            terminals_ms: t,
            draw_calls_total: 20,
            surfaces: 10,
            atlas_active_pages: 1,
            ..Default::default()
        }
    }

    #[test]
    fn aggregator_caps_window() {
        let mut agg = PerfAggregator::new();
        for i in 0..(WINDOW + 50) {
            agg.push_enabled(mk(i as f64, i as f64));
        }
        assert_eq!(agg.frames.len(), WINDOW);
    }

    /// 구간·인스턴스·바뀐 양이 한 줄에 모두 나온다.
    #[test]
    fn frame_line_reports_stages_instances_and_changes() {
        let mut agg = PerfAggregator::new();
        for i in 0..10u32 {
            agg.push_enabled(FrameSample {
                egui_frame_ms: 1.0,
                post_egui_ms: 0.75,
                tessellate_ms: 0.5,
                clear_ms: 0.25,
                egui_pass_ms: 2.0,
                present_ms: 3.0,
                terminals_build_ms: 4.0,
                terminals_submit_ms: 5.0,
                overlay_acquire_ms: 0.0,
                bg_instances: 2000,
                glyph_instances: 900,
                changes: Some(FrameChanges {
                    surfaces: 1,
                    cells: 7 + i % 2,
                    instance_bytes: 320,
                }),
                ..mk(10.0, 9.0)
            });
        }
        let line = agg.frame_line().expect("line");
        for field in [
            "terminals_ms p50=9.00",
            "egui_frame_ms p50=1.00 p99=1.00",
            "post_egui_ms p50=0.75",
            "tessellate_ms p50=0.50",
            "clear_ms p50=0.25",
            "egui_pass_ms p50=2.00",
            "present_ms p50=3.00",
            "terminals_build_ms p50=4.00",
            "terminals_submit_ms p50=5.00",
            "overlay_acquire_ms p50=0.00",
            "bg_instances p50=2000",
            "glyph_instances p50=900",
            "changes_n=10",
            "changed_surfaces p50=1",
            "changed_cells p50=8 p99=8 max=8",
            "changed_instance_bytes p50=320",
        ] {
            assert!(line.contains(field), "{field} missing in {line}");
        }
    }

    /// 계측이 꺼진 프레임만 있으면 바뀐 양을 0으로 쓰지 않고 untracked로 쓴다.
    #[test]
    fn frame_line_marks_untracked_changes() {
        let mut agg = PerfAggregator::new();
        agg.push_enabled(mk(1.0, 1.0));
        let line = agg.frame_line().expect("line");
        assert!(line.contains("changes=untracked"), "{line}");
        assert!(!line.contains("changed_cells"), "{line}");
    }

    #[test]
    fn lock_line_reports_mean_and_max_per_site_and_none_without_samples() {
        use tasty_terminal::lock_timing::LockSite;
        let stats = LockSiteStats {
            count: 4,
            wait_ns_sum: 8_000,
            wait_ns_max: 5_000,
            hold_ns_sum: 40_000,
            hold_ns_max: 20_000,
        };
        let line = lock_line(&[
            (LockSite::Ingest, stats),
            (LockSite::View, LockSiteStats::default()),
        ]);
        assert_eq!(
            line,
            "perf_lock ingest n=4 wait_us mean=2.0 max=5.0 hold_us mean=10.0 max=20.0 \
             view n=0 wait_us mean=none max=none hold_us mean=none max=none"
        );
    }

    #[test]
    fn shell_line_reports_redraw_segments() {
        let mut agg = PerfAggregator::new();
        assert!(agg.shell_line().is_none());
        agg.push_shell_enabled(RedrawShellSample {
            prepare_ms: 0.5,
            mesh_relay_ms: 0.25,
            webview_sync_ms: 1.5,
        });
        let line = agg.shell_line().expect("line");
        assert!(line.starts_with("perf_redraw n=1 "), "{line}");
        assert!(line.contains("prepare_ms p50=0.50"), "{line}");
        assert!(line.contains("mesh_relay_ms p50=0.25"), "{line}");
        assert!(line.contains("webview_sync_ms p50=1.50"), "{line}");
    }
}
