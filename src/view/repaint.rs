//! 프레임 사이 최소 간격을 유발원별로 둔다. 입력은 입력 상한, 터미널 출력은 출력 상한
//! (`performance.input_redraw_fps`·`output_redraw_fps`), 그 밖의 요청은 모니터 주사율까지다.
//! 어느 상한도 창이 있는 모니터의 주사율보다 빠르게 그리지 않는다.
//! dirty는 유지하며 미룬 요청은 deferred_deadline을 통해 about_to_wait에서 예약한다.
//! attach mesh 중계도 dirty 프레임에 의존하므로 요청 자체를 버리지는 않는다.
//!
//! 요청의 통과·지연과 실제 present 횟수를 1초 단위로 기록한다.
//! 로그: TASTY_LOG=tasty::view::repaint=info.

mod caps;
mod surface_retry;

use caps::RedrawCaps;
pub(crate) use caps::install as install_redraw_caps;

use std::time::{Duration, Instant};

/// 리페인트 요청의 유발원. 상한 적용 여부와 계측 분류를 함께 결정한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RepaintSource {
    /// 키·마우스·IME·리사이즈·포커스 등 사용자 조작과 구조 변경. 입력 상한 대상.
    Interactive,
    /// PTY 출력 반영. 출력 상한 대상.
    TerminalOutput,
    /// 입력 직후 같은 surface에서 온 PTY 출력(입력 에코). 입력 상한 대상.
    /// 구간은 busy 판정의 에코 규칙과 같다(`Terminal::last_output_is_input_echo`).
    InputEcho,
    /// egui 내부의 delay 0 즉시 repaint(진행 중 애니메이션 등). 상한 대상.
    EguiAnimation,
    /// attach mirror(원격 화면 중계) 갱신. 상한 대상.
    AttachMirror,
}

impl RepaintSource {
    /// 보이는 surface들의 PTY 출력이 일으킨 요청의 유발원. 하나라도 입력 에코면 입력 에코다.
    /// 인자는 출력이 생긴 보이는 surface마다 `Terminal::last_output_is_input_echo` 결과다.
    pub(crate) fn for_output(echoes: impl IntoIterator<Item = bool>) -> Self {
        if echoes.into_iter().any(|echo| echo) {
            Self::InputEcho
        } else {
            Self::TerminalOutput
        }
    }
}

/// 한 요청이 따르는 최소 간격의 종류.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Governor {
    Input,
    Output,
    Monitor,
}

/// 모니터 주사율을 얻지 못했을 때 사용할 기본 상한.
const FALLBACK_REFRESH_HZ: u32 = 60;
/// 주사율 재조회 주기. 창을 다른 모니터로 옮기거나 모드가 바뀌는 것을 따라가되,
/// `current_monitor()` 조회(X11 은 서버 왕복이 있을 수 있다)를 프레임마다 하지는 않는다.
const REFRESH_RATE_TTL: Duration = Duration::from_secs(5);
/// 주사율 clamp 하한 — 비정상적으로 낮은 보고값이 화면을 얼리지 못하게 한다.
const MIN_REFRESH_HZ: u32 = 24;
/// 주사율 clamp 상한 — 비정상적으로 높은 보고값이 상한을 사실상 무력화하지 못하게 한다.
const MAX_REFRESH_HZ: u32 = 480;
/// 계측 집계 창.
const STATS_WINDOW: Duration = Duration::from_secs(1);

/// 모니터가 보고한 millihertz에서 최소 프레임 간격을 구한다.
/// 값이 없거나 0이면 기본값을 쓰고, 그 외에는 허용 범위로 제한한다.
fn min_interval_from_millihertz(millihertz: Option<u32>) -> Duration {
    let mhz = millihertz
        .filter(|m| *m > 0)
        .unwrap_or(FALLBACK_REFRESH_HZ * 1000)
        .clamp(MIN_REFRESH_HZ * 1000, MAX_REFRESH_HZ * 1000);
    // ns/frame = 1e12 / mHz — Hz 로 내림하지 않아 59.94Hz 같은 값도 그대로 반영된다.
    Duration::from_nanos(1_000_000_000_000u64 / u64::from(mhz))
}

/// 요청이 따를 간격의 종류.
fn governor(source: RepaintSource) -> Governor {
    match source {
        RepaintSource::Interactive | RepaintSource::InputEcho => Governor::Input,
        RepaintSource::TerminalOutput => Governor::Output,
        RepaintSource::EguiAnimation | RepaintSource::AttachMirror => Governor::Monitor,
    }
}

/// 창 하나의 리페인트 요청 상한 상태.
pub struct RepaintGate {
    /// 창이 있는 모니터의 주사율에서 구한 최소 프레임 간격.
    min_interval: Duration,
    /// 설정의 유발원별 상한. [`Self::admit`]이 요청마다 공유값에서 갱신한다.
    caps: RedrawCaps,
    /// 미룬 요청 중에 입력 기인 요청이 있다. 그 프레임 전체가 입력 상한을 따른다.
    input_pending: bool,
    /// `min_interval` 을 마지막으로 모니터에서 다시 구한 시각.
    interval_checked_at: Option<Instant>,
    /// 마지막으로 실제 프레임을 그린 시각. 상한 창의 기준점.
    last_present: Option<Instant>,
    /// 미룬 요청을 다시 처리할 예정 시각.
    deferred_until: Option<Instant>,
    stats: RepaintStats,
    retry: surface_retry::SurfaceRetry,
}

impl RepaintGate {
    pub fn new() -> Self {
        Self::with_caps(min_interval_from_millihertz(None), RedrawCaps::current())
    }

    #[cfg(test)]
    fn with_min_interval(min_interval: Duration) -> Self {
        Self::with_caps(min_interval, RedrawCaps::UNLIMITED)
    }

    fn with_caps(min_interval: Duration, caps: RedrawCaps) -> Self {
        Self {
            min_interval,
            caps,
            input_pending: false,
            interval_checked_at: None,
            last_present: None,
            deferred_until: None,
            stats: RepaintStats::default(),
            retry: Default::default(),
        }
    }

    /// 지금 redraw를 요청할지 반환한다. false면 deferred_deadline까지 미루며
    /// 호출자가 그 시각을 예약해야 한다.
    pub fn admit(
        &mut self,
        source: RepaintSource,
        now: Instant,
        window: &winit::window::Window,
    ) -> bool {
        self.refresh_min_interval(now, window);
        self.caps = RedrawCaps::current();
        self.admit_at(source, now)
    }

    /// [`Self::admit`] 에서 모니터 조회만 뺀 판정 본체 — 시각을 주입할 수 있어
    /// 헤드리스 테스트가 가능하다.
    fn admit_at(&mut self, source: RepaintSource, now: Instant) -> bool {
        if self.retry.active() {
            return self.admit_retry(source, now);
        }
        let governor = governor(source);
        if governor == Governor::Input {
            self.input_pending = true;
        }
        let interval = if self.input_pending {
            self.caps.input_interval(self.min_interval)
        } else {
            self.interval_for(governor)
        };
        let admitted = self.window_open(now, interval);
        if admitted {
            // 지금 그리는 프레임이 미뤄뒀던 요청까지 덮는다.
            self.deferred_until = None;
            self.input_pending = false;
        } else {
            let at = self.next_allowed(now, interval);
            // 입력이 섞인 프레임은 입력 상한이 정한 시각에 그린다.
            self.deferred_until = Some(match self.deferred_until {
                Some(cur) if !self.input_pending => cur.min(at),
                _ => at,
            });
        }
        self.stats
            .note_request(source, admitted, now, self.caps, self.min_interval);
        admitted
    }

    /// 재시도 상태(실패 뒤 성공·취소 전)의 판정. 정상 상한과 섞인 프레임 계산을 하지 않는다.
    /// 만기 전에는 예약된 재시도 시각을 지키고, 만기 뒤에는 바로 그린다.
    fn admit_retry(&mut self, source: RepaintSource, now: Instant) -> bool {
        let admitted = self.retry.ready(now);
        if admitted {
            self.deferred_until = None;
        } else {
            assert!(
                self.deferred_until.is_some(),
                "a failed frame reserves its retry"
            );
        }
        self.stats
            .note_request(source, admitted, now, self.caps, self.min_interval);
        admitted
    }

    /// 입력이 섞인 미룬 프레임의 시각이 아직 오지 않았다. 그 전에 이미 발행된 redraw가
    /// 처리되더라도 그리지 않고(dirty와 미룬 예약은 그대로) WaitUntil 깨움에 맡긴다.
    /// 재시도 상태(만기 전후 모두)에는 재시도 계약이 먼저라 여기서 판정하지 않는다.
    pub(crate) fn holds_mixed_frame(&self, now: Instant) -> bool {
        self.input_pending && !self.retry.active() && self.deferred_until.is_some_and(|at| now < at)
    }

    /// 입력이 섞이지 않은 요청의 최소 간격.
    fn interval_for(&self, governor: Governor) -> Duration {
        match governor {
            Governor::Output => self.caps.output_interval(self.min_interval),
            Governor::Input | Governor::Monitor => self.min_interval,
        }
    }

    /// 미뤄둔 요청이 만기됐으면 슬롯을 비우고 `true` — 호출자가 `request_redraw()` 한다.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.deferred_until {
            Some(at) if at <= now => {
                self.deferred_until = None;
                true
            }
            _ => false,
        }
    }

    /// 미룬 요청의 예정 시각. about_to_wait에서 WaitUntil로 예약한다.
    pub fn deferred_deadline(&self) -> Option<Instant> {
        self.deferred_until
    }

    /// 실제로 프레임을 그리기로 확정한 지점. 다음 상한 창의 기준점을 갱신한다.
    pub fn note_present(&mut self, now: Instant) {
        self.retry.recovered();
        self.last_present = Some(now);
        // 이 프레임이 미뤄뒀던 요청을 소진한다.
        self.deferred_until = None;
        self.input_pending = false;
        self.stats.note_present(now, self.caps, self.min_interval);
    }

    /// Preserve dirty after a returned surface error and reserve a delayed wakeup.
    pub(crate) fn surface_failed(&mut self, now: Instant) -> bool {
        let (at, report) = self.retry.failed(now);
        self.deferred_until = Some(at);
        report
    }

    pub(crate) fn cancel_surface_retry(&mut self) {
        self.retry.recovered();
        self.deferred_until = None;
        self.input_pending = false;
    }

    pub(crate) fn surface_ready(&self, now: Instant) -> bool {
        self.retry.ready(now)
    }

    /// 지금 즉시 그려도 `interval` 상한을 넘지 않는가. 쉬던 창의 첫 요청은 늘 열려 있다.
    fn window_open(&self, now: Instant, interval: Duration) -> bool {
        self.last_present
            .is_none_or(|t| now.duration_since(t) >= interval)
    }

    /// `interval` 상한에서 다음으로 그려도 되는 가장 이른 시각.
    fn next_allowed(&self, now: Instant, interval: Duration) -> Instant {
        self.last_present.map_or(now, |t| (t + interval).max(now))
    }

    /// TTL 이 지났으면 현재 모니터의 주사율에서 상한을 다시 구한다.
    fn refresh_min_interval(&mut self, now: Instant, window: &winit::window::Window) {
        if self
            .interval_checked_at
            .is_some_and(|t| now.duration_since(t) < REFRESH_RATE_TTL)
        {
            return;
        }
        self.interval_checked_at = Some(now);
        let millihertz = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz());
        self.min_interval = min_interval_from_millihertz(millihertz);
    }
}

impl Default for RepaintGate {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Clone, Copy)]
struct SourceCount {
    admitted: u32,
    deferred: u32,
}

/// 1 초 창 집계. 요청 수와 **실제 present 수**를 따로 세야 "요청이 많아도 실제
/// 렌더는 합쳐졌다" 를 구분할 수 있다.
#[derive(Default)]
struct RepaintStats {
    interactive: SourceCount,
    terminal_output: SourceCount,
    input_echo: SourceCount,
    egui: SourceCount,
    attach_mirror: SourceCount,
    presents: u32,
    window_start: Option<Instant>,
}

impl RepaintStats {
    fn note_request(
        &mut self,
        source: RepaintSource,
        admitted: bool,
        now: Instant,
        caps: RedrawCaps,
        min_interval: Duration,
    ) {
        let slot = match source {
            RepaintSource::Interactive => &mut self.interactive,
            RepaintSource::TerminalOutput => &mut self.terminal_output,
            RepaintSource::InputEcho => &mut self.input_echo,
            RepaintSource::EguiAnimation => &mut self.egui,
            RepaintSource::AttachMirror => &mut self.attach_mirror,
        };
        if admitted {
            slot.admitted += 1;
        } else {
            slot.deferred += 1;
        }
        self.maybe_dump(now, caps, min_interval);
    }

    fn note_present(&mut self, now: Instant, caps: RedrawCaps, min_interval: Duration) {
        self.presents += 1;
        self.maybe_dump(now, caps, min_interval);
    }

    fn maybe_dump(&mut self, now: Instant, caps: RedrawCaps, min_interval: Duration) {
        let start = *self.window_start.get_or_insert(now);
        let elapsed = now.duration_since(start);
        if elapsed < STATS_WINDOW {
            return;
        }
        tracing::info!(
            target: "tasty::view::repaint",
            "repaint {:.1}s cap={:.1}fps out_cap={:.1}fps in_cap={:.1}fps present={} \
             interactive={}+{}d output={}+{}d echo={}+{}d egui={}+{}d mirror={}+{}d",
            elapsed.as_secs_f64(),
            1.0 / min_interval.as_secs_f64(),
            1.0 / caps.output_interval(min_interval).as_secs_f64(),
            1.0 / caps.input_interval(min_interval).as_secs_f64(),
            self.presents,
            self.interactive.admitted,
            self.interactive.deferred,
            self.terminal_output.admitted,
            self.terminal_output.deferred,
            self.input_echo.admitted,
            self.input_echo.deferred,
            self.egui.admitted,
            self.egui.deferred,
            self.attach_mirror.admitted,
            self.attach_mirror.deferred,
        );
        *self = Self {
            window_start: Some(now),
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 상한값은 모니터 주사율을 따라간다 — 50Hz(xrdp 가상 디스플레이)든 144Hz 든.
    #[test]
    fn min_interval_tracks_reported_refresh_rate() {
        assert_eq!(
            min_interval_from_millihertz(Some(50_000)),
            Duration::from_millis(20)
        );
        assert_eq!(
            min_interval_from_millihertz(Some(144_000)),
            Duration::from_nanos(6_944_444)
        );
    }

    /// 주사율을 알 수 없으면 60Hz를 쓴다.
    #[test]
    fn min_interval_falls_back_to_60hz() {
        assert_eq!(
            min_interval_from_millihertz(None),
            min_interval_from_millihertz(Some(FALLBACK_REFRESH_HZ * 1000))
        );
        assert_eq!(
            min_interval_from_millihertz(None),
            Duration::from_nanos(16_666_666)
        );
    }

    /// 비정상 보고값은 clamp 한다 — 0 은 폴백으로, 극단값은 범위 안으로.
    #[test]
    fn min_interval_clamps_absurd_rates() {
        assert_eq!(
            min_interval_from_millihertz(Some(0)),
            min_interval_from_millihertz(None)
        );
        assert_eq!(
            min_interval_from_millihertz(Some(1)),
            min_interval_from_millihertz(Some(MIN_REFRESH_HZ * 1000))
        );
        assert_eq!(
            min_interval_from_millihertz(Some(10_000_000)),
            min_interval_from_millihertz(Some(MAX_REFRESH_HZ * 1000))
        );
    }

    /// 무제한 입력도 모니터 간격 안의 요청은 묶고, 간격이 지나면 바로 통과한다.
    #[test]
    fn unlimited_input_requests_coalesce_to_the_monitor_interval() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        let t0 = Instant::now();
        gate.note_present(t0);
        for i in 1..10 {
            assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(i)));
        }
        assert_eq!(
            gate.deferred_deadline(),
            Some(t0 + Duration::from_millis(20))
        );
        assert!(gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(20)));
        assert_eq!(gate.deferred_deadline(), None);
    }

    /// 출력발 요청은 상한 창 안에서 묶이고, 만기 시각이 예약된다.
    #[test]
    fn output_requests_coalesce_within_the_window() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        let t0 = Instant::now();
        gate.note_present(t0);

        // 최소 간격이 지나기 전 요청은 하나의 예정 시각으로 합친다.
        for i in 1..=5 {
            assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(i)));
        }
        assert_eq!(
            gate.deferred_deadline(),
            Some(t0 + Duration::from_millis(20))
        );

        // 예정 시각 전에는 처리하지 않는다.
        assert!(!gate.take_due(t0 + Duration::from_millis(19)));
        // 예정 시각 뒤 한 번 처리하고 비운다.
        assert!(gate.take_due(t0 + Duration::from_millis(20)));
        assert!(!gate.take_due(t0 + Duration::from_millis(21)));
        assert_eq!(gate.deferred_deadline(), None);
    }

    /// 창이 이미 열려 있으면 출력발 요청도 즉시 통과한다(상시 20ms 지연이 아니다).
    #[test]
    fn output_request_passes_once_the_window_reopens() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(gate.admit_at(
            RepaintSource::TerminalOutput,
            t0 + Duration::from_millis(20)
        ));
    }

    /// 즉시 통과한 요청은 미뤄뒀던 요청까지 덮는다 — 같은 프레임이 둘 다 반영한다.
    #[test]
    fn immediate_pass_absorbs_a_pending_deferral() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(1)));
        assert!(gate.deferred_deadline().is_some());
        assert!(gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(20)));
        assert_eq!(gate.deferred_deadline(), None);
    }

    /// present 는 미뤄둔 요청을 소진한다 — 이미 그린 화면을 위해 한 프레임 더 깨우지 않는다.
    #[test]
    fn present_consumes_the_deferral() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::EguiAnimation, t0 + Duration::from_millis(1)));
        gate.note_present(t0 + Duration::from_millis(2));
        assert_eq!(gate.deferred_deadline(), None);
    }

    /// 아직 한 번도 그리지 않은 창은 첫 프레임을 막지 않는다.
    #[test]
    fn first_frame_is_never_deferred() {
        let mut gate = RepaintGate::with_min_interval(Duration::from_millis(20));
        assert!(gate.admit_at(RepaintSource::TerminalOutput, Instant::now()));
    }

    const MONITOR_60: Duration = Duration::from_nanos(16_666_666);
    const OUT_30: Duration = Duration::from_nanos(33_333_333);

    /// 기본값(출력 30fps·입력 60fps)으로 60Hz 모니터에 있는 창.
    fn default_caps_gate() -> RepaintGate {
        RepaintGate::with_caps(
            MONITOR_60,
            RedrawCaps {
                output: Some(30),
                input: Some(60),
            },
        )
    }

    /// 출력 기인 프레임 사이는 출력 상한 간격 이상이다.
    #[test]
    fn output_frames_are_spaced_by_the_output_cap() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(
            RepaintSource::TerminalOutput,
            t0 + Duration::from_millis(17)
        ));
        assert_eq!(gate.deferred_deadline(), Some(t0 + OUT_30));
        assert!(gate.admit_at(RepaintSource::TerminalOutput, t0 + OUT_30));
    }

    /// 쉬던 창의 첫 입력은 기다리지 않는다. 직전 프레임에 가까운 입력만 입력 상한으로 미룬다.
    #[test]
    fn first_input_after_idle_draws_at_once_and_close_input_waits_for_the_input_cap() {
        let mut fresh = default_caps_gate();
        assert!(fresh.admit_at(RepaintSource::Interactive, Instant::now()));

        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(200)));
        gate.note_present(t0 + Duration::from_millis(200));
        let t1 = t0 + Duration::from_millis(205);
        assert!(!gate.admit_at(RepaintSource::Interactive, t1));
        assert_eq!(
            gate.deferred_deadline(),
            Some(t0 + Duration::from_millis(200) + MONITOR_60)
        );
    }

    /// 출력과 입력이 함께 기다리는 프레임은 입력 상한 시각에 그린다.
    #[test]
    fn a_frame_with_input_and_output_follows_the_input_cap() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(1)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + OUT_30));
        assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(2)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + MONITOR_60));
        // 입력이 기다리는 동안 들어온 출력도 그 프레임에 실린다.
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(3)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + MONITOR_60));

        // 입력 상한이 출력 상한보다 느려도 섞인 프레임은 입력 상한을 따른다.
        let mut slow_input = RepaintGate::with_caps(
            MONITOR_60,
            RedrawCaps {
                output: Some(60),
                input: Some(15),
            },
        );
        slow_input.note_present(t0);
        assert!(!slow_input.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(1)));
        assert_eq!(slow_input.deferred_deadline(), Some(t0 + MONITOR_60));
        assert!(!slow_input.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(2)));
        assert_eq!(
            slow_input.deferred_deadline(),
            Some(t0 + Duration::from_nanos(66_666_666))
        );
    }

    /// 입력 에코로 분류된 출력은 입력 상한, 에코 구간 밖 출력은 출력 상한을 따른다.
    #[test]
    fn input_echo_follows_the_input_cap_and_other_output_the_output_cap() {
        let t0 = Instant::now();
        let mut echo = default_caps_gate();
        echo.note_present(t0);
        assert!(!echo.admit_at(RepaintSource::InputEcho, t0 + Duration::from_millis(3)));
        assert_eq!(echo.deferred_deadline(), Some(t0 + MONITOR_60));

        let mut output = default_caps_gate();
        output.note_present(t0);
        assert!(!output.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(3)));
        assert_eq!(output.deferred_deadline(), Some(t0 + OUT_30));
    }

    /// 무제한: 입력과 출력 모두 모니터 간격까지다. 쉬던 창의 첫 요청은 바로 그린다.
    #[test]
    fn unlimited_caps_keep_the_monitor_rate_for_input_and_output() {
        let mut gate = RepaintGate::with_caps(MONITOR_60, RedrawCaps::UNLIMITED);
        let t0 = Instant::now();
        assert!(gate.admit_at(RepaintSource::Interactive, t0));
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(1)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + MONITOR_60));
        assert!(!gate.admit_at(RepaintSource::InputEcho, t0 + Duration::from_millis(2)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + MONITOR_60));
        gate.note_present(t0 + MONITOR_60);
        let t1 = t0 + MONITOR_60;
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t1 + Duration::from_millis(2)));
        assert_eq!(gate.deferred_deadline(), Some(t1 + MONITOR_60));
    }

    /// 출력 요청이 승인돼 redraw가 이미 발행된 뒤 입력이 와서 입력 상한 시각으로 미뤄지면,
    /// 그 redraw를 처리할 때 그리지 않고 미룬 시각을 기다린다(섞인 프레임은 입력 상한).
    #[test]
    fn an_issued_redraw_waits_when_input_joins_before_it_is_drawn() {
        let mut gate = RepaintGate::with_caps(
            MONITOR_60,
            RedrawCaps {
                output: Some(60),
                input: Some(15),
            },
        );
        let t0 = Instant::now();
        gate.note_present(t0);
        let input_at = t0 + Duration::from_nanos(66_666_666);
        assert!(gate.admit_at(
            RepaintSource::TerminalOutput,
            t0 + Duration::from_millis(20)
        ));
        assert!(!gate.holds_mixed_frame(t0 + Duration::from_millis(20)));
        assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(21)));
        assert_eq!(gate.deferred_deadline(), Some(input_at));
        // 발행된 redraw가 t=22ms에 처리돼도 그리지 않는다.
        assert!(gate.holds_mixed_frame(t0 + Duration::from_millis(22)));
        // 미룬 시각에 깨워 그린다.
        assert!(!gate.holds_mixed_frame(input_at));
        assert!(gate.take_due(input_at));
        assert!(!gate.holds_mixed_frame(input_at));
        gate.note_present(input_at);
        assert!(!gate.holds_mixed_frame(input_at));
    }

    /// 입력이 섞이지 않은 미룬 요청은 이미 발행된 redraw를 막지 않는다.
    #[test]
    fn an_output_only_deferral_does_not_hold_an_issued_redraw() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(gate.admit_at(RepaintSource::EguiAnimation, t0 + Duration::from_millis(20)));
        assert!(!gate.admit_at(
            RepaintSource::TerminalOutput,
            t0 + Duration::from_millis(21)
        ));
        assert_eq!(gate.deferred_deadline(), Some(t0 + OUT_30));
        assert!(!gate.holds_mixed_frame(t0 + Duration::from_millis(22)));
    }

    /// 재시도 대기 중에는 섞인 프레임 판정을 하지 않는다(재시도 시각 계약이 먼저다).
    #[test]
    fn a_retry_wait_is_not_judged_as_a_mixed_frame() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        gate.surface_failed(t0 + Duration::from_millis(1));
        let due = gate.deferred_deadline().unwrap();
        assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(2)));
        // 만기 전 입력은 예약된 재시도 시각을 바꾸지 않는다.
        assert_eq!(gate.deferred_deadline(), Some(due));
        assert!(!gate.holds_mixed_frame(t0 + Duration::from_millis(2)));
    }

    /// 만기된 재시도도 성공 전까지는 재시도 상태다. 입력이 합류해도 정상 상한과
    /// 섞인 프레임 보류를 받지 않고 바로 그린다(60Hz, 출력 60·입력 15).
    #[test]
    fn an_expired_retry_draws_at_once_when_input_joins_before_it() {
        let mut gate = RepaintGate::with_caps(
            MONITOR_60,
            RedrawCaps {
                output: Some(60),
                input: Some(15),
            },
        );
        let t0 = Instant::now();
        gate.note_present(t0);
        let failed_at = t0 + Duration::from_millis(20);
        gate.surface_failed(failed_at);
        let due = gate.deferred_deadline().unwrap();
        assert_eq!(due, failed_at + Duration::from_millis(16));
        // 만기에 깨워 redraw를 발행한다.
        assert!(gate.take_due(due));
        // 처리 전에 입력이 온다: 재시도 상태라 바로 승인하고 입력 상한 예약을 만들지 않는다.
        let input_at = due + Duration::from_millis(1);
        assert!(gate.admit_at(RepaintSource::Interactive, input_at));
        assert_eq!(gate.deferred_deadline(), None);
        // 발행된 재시도 redraw는 보류되지 않는다.
        let redraw_at = due + Duration::from_millis(2);
        assert!(gate.surface_ready(redraw_at));
        assert!(!gate.holds_mixed_frame(redraw_at));
        // 성공하면 정상 상한이 다시 적용된다.
        gate.note_present(redraw_at);
        assert!(!gate.admit_at(
            RepaintSource::Interactive,
            redraw_at + Duration::from_millis(1)
        ));
        assert_eq!(
            gate.deferred_deadline(),
            Some(redraw_at + Duration::from_nanos(66_666_666))
        );
    }

    /// DEC 2026 블록 안의 입력 에코 출력은 유발원과 관계없이 보류되고, 블록이 끝난 출력은
    /// 입력 에코로서 입력 상한을 따른다(`ViewBase::mark_surface_output`과 같은 순서).
    #[test]
    fn input_echo_inside_a_sync_block_is_held_then_follows_the_input_cap() {
        use crate::view::sync_hold::SyncHold;
        let mut gate = RepaintGate::with_caps(
            MONITOR_60,
            RedrawCaps {
                output: Some(60),
                input: Some(15),
            },
        );
        let mut hold = SyncHold::default();
        let t0 = Instant::now();
        gate.note_present(t0);
        // 블록 안의 에코: 보류되어 게이트에 요청하지 않는다.
        assert!(hold.hold_output(7, true, t0 + Duration::from_millis(5)));
        assert_eq!(gate.deferred_deadline(), None);
        // 블록을 끝낸 에코 출력: 보류가 풀리고 입력 상한으로 미뤄진다.
        let end = t0 + Duration::from_millis(20);
        assert!(!hold.hold_output(7, false, end));
        assert!(!gate.admit_at(RepaintSource::InputEcho, end));
        hold.note_deferred(Some(7));
        let input_at = t0 + Duration::from_nanos(66_666_666);
        assert_eq!(gate.deferred_deadline(), Some(input_at));
        assert!(gate.holds_mixed_frame(end + Duration::from_millis(1)));
        // 입력 상한 시각에 깨우면 보류 중인 surface가 없으므로 그린다.
        assert!(gate.take_due(input_at));
        assert!(hold.admit_due());
    }

    /// 취소하면 재시도 상태가 끝나고 정상 상한으로 돌아간다.
    #[test]
    fn a_cancelled_retry_returns_to_the_normal_caps() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        gate.surface_failed(t0 + Duration::from_millis(1));
        gate.cancel_surface_retry();
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(2)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + OUT_30));
    }

    /// 보이는 surface 중 하나라도 입력 에코면 그 창의 출력 요청은 입력 에코다.
    #[test]
    fn output_source_is_input_echo_when_any_visible_output_is_echo() {
        assert_eq!(
            RepaintSource::for_output([false, true]),
            RepaintSource::InputEcho
        );
        assert_eq!(
            RepaintSource::for_output([false, false]),
            RepaintSource::TerminalOutput
        );
        assert_eq!(
            RepaintSource::for_output(std::iter::empty()),
            RepaintSource::TerminalOutput
        );
    }

    /// 적용값은 min(설정값, 창이 있는 모니터 주사율)이다.
    #[test]
    fn caps_faster_than_the_monitor_are_clamped_to_it() {
        let monitor_30 = min_interval_from_millihertz(Some(30_000));
        let mut gate = RepaintGate::with_caps(
            monitor_30,
            RedrawCaps {
                output: Some(240),
                input: Some(120),
            },
        );
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::Interactive, t0 + Duration::from_millis(10)));
        assert_eq!(gate.deferred_deadline(), Some(t0 + monitor_30));
        gate.note_present(t0 + monitor_30);
        let t1 = t0 + monitor_30;
        assert!(!gate.admit_at(
            RepaintSource::TerminalOutput,
            t1 + Duration::from_millis(10)
        ));
        assert_eq!(gate.deferred_deadline(), Some(t1 + monitor_30));
    }

    /// 출력이 멈춰도 미룬 출력은 출력 상한 간격 안에 만기돼 다시 그려진다.
    #[test]
    fn a_deferred_output_frame_comes_due_without_further_events() {
        let mut gate = default_caps_gate();
        let t0 = Instant::now();
        gate.note_present(t0);
        assert!(!gate.admit_at(RepaintSource::TerminalOutput, t0 + Duration::from_millis(1)));
        assert!(!gate.take_due(t0 + Duration::from_millis(30)));
        assert!(gate.take_due(t0 + OUT_30));
        assert_eq!(gate.deferred_deadline(), None);

        // 입력이 섞여 미룬 프레임도 같다.
        gate.note_present(t0 + OUT_30);
        let t1 = t0 + OUT_30;
        assert!(!gate.admit_at(RepaintSource::Interactive, t1 + Duration::from_millis(1)));
        assert!(gate.take_due(t1 + MONITOR_60));
        gate.note_present(t1 + MONITOR_60);
        assert!(!gate.input_pending);
    }

    /// 재시도 계약만 보는 시험의 창. 상한은 무제한(모니터 간격)으로 둔다.
    fn retry_gate() -> RepaintGate {
        RepaintGate::with_caps(min_interval_from_millihertz(None), RedrawCaps::UNLIMITED)
    }

    #[test]
    fn surface_timeout_reserves_wakeup_and_blocks_input_and_stale_redraw() {
        let now = Instant::now();
        let mut gate = retry_gate();
        assert!(gate.surface_failed(now));
        let due = gate.deferred_deadline().unwrap();
        assert!(due > now);
        assert!(!gate.surface_ready(now));
        for source in [
            RepaintSource::Interactive,
            RepaintSource::TerminalOutput,
            RepaintSource::EguiAnimation,
            RepaintSource::AttachMirror,
        ] {
            assert!(!gate.admit_at(source, now));
            assert_eq!(gate.deferred_deadline(), Some(due));
        }
        assert!(!gate.take_due(now));
        assert!(gate.take_due(due));
        assert!(gate.surface_ready(due));
        assert!(!gate.take_due(due));
        gate.note_present(due);
        assert_eq!(gate.deferred_deadline(), None);
        assert!(gate.admit_at(
            RepaintSource::Interactive,
            due + min_interval_from_millihertz(None)
        ));
    }

    #[test]
    fn repeated_surface_errors_back_off_until_actual_present() {
        let now = Instant::now();
        let mut gate = retry_gate();
        gate.surface_failed(now);
        let due = gate.deferred_deadline().unwrap();
        assert!(gate.take_due(due));
        gate.surface_failed(due);
        let next = gate.deferred_deadline().unwrap();
        assert_eq!(next - due, Duration::from_millis(32));
        gate.note_present(next);
        gate.surface_failed(next);
        assert_eq!(
            gate.deferred_deadline().unwrap() - next,
            Duration::from_millis(16)
        );
    }

    #[test]
    fn fatal_error_cancels_retry_without_recording_a_present() {
        let now = Instant::now();
        let mut gate = retry_gate();
        gate.surface_failed(now);
        gate.cancel_surface_retry();
        assert_eq!(gate.deferred_deadline(), None);
        assert_eq!(gate.last_present, None);
        assert_eq!(gate.stats.presents, 0);
        assert!(!gate.take_due(now + Duration::from_secs(2)));
    }
}
