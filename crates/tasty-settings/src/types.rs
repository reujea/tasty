use serde::{Deserialize, Serialize};
use tasty_type_geometry::length::LogicalPx;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceSettings {
    /// When enabled, only terminals with new PTY output are processed each frame
    /// instead of polling all terminals. Reduces CPU usage with many surfaces.
    /// Requires restart to apply.
    pub targeted_pty_polling: bool,
    /// When enabled, swap old scrollback lines to disk to reduce memory usage.
    /// Applies to new terminals; running terminals keep the value they started with.
    pub scrollback_disk_swap: bool,
    /// Most lines disk swap keeps per terminal, on top of `general.scrollback_lines` in
    /// memory. The oldest lines past it are dropped. `-1` keeps every line; other negative
    /// values are invalid and normalized to the default. Applies to new terminals.
    pub scrollback_disk_lines: i64,
    /// Redraw cap (frames per second) for frames caused by terminal output. One of
    /// [`crate::REDRAW_FPS_CHOICES`] or `-1` (no cap: the monitor refresh rate). The
    /// applied cap never exceeds the refresh rate of the monitor the window is on.
    pub output_redraw_fps: i64,
    /// Redraw cap (frames per second) for frames caused by user input. Same values as
    /// [`Self::output_redraw_fps`]. A frame that has both input and output follows this cap.
    pub input_redraw_fps: i64,
}

/// Default of [`PerformanceSettings::scrollback_disk_lines`].
pub const DEFAULT_SCROLLBACK_DISK_LINES: i64 = 10_000;
/// [`PerformanceSettings::scrollback_disk_lines`] value that keeps every line.
pub const UNLIMITED_SCROLLBACK_DISK_LINES: i64 = -1;

impl PerformanceSettings {
    /// Disk line cap for a terminal: `None` keeps every line. An invalid negative value
    /// counts as the default, the value normalization stores.
    pub fn scrollback_disk_cap(&self) -> Option<usize> {
        disk_scrollback_cap(self.scrollback_disk_lines)
    }

    /// `-1` means unlimited; any other negative value is invalid and becomes the default.
    pub(crate) fn normalize(&mut self, changed: &mut bool) {
        if self.scrollback_disk_lines < UNLIMITED_SCROLLBACK_DISK_LINES {
            tracing::warn!(
                "invalid performance.scrollback_disk_lines {}; using {}",
                self.scrollback_disk_lines,
                DEFAULT_SCROLLBACK_DISK_LINES
            );
            self.scrollback_disk_lines = DEFAULT_SCROLLBACK_DISK_LINES;
            *changed = true;
        }
        for (label, field, default) in [
            (
                "output_redraw_fps",
                &mut self.output_redraw_fps,
                crate::DEFAULT_OUTPUT_REDRAW_FPS,
            ),
            (
                "input_redraw_fps",
                &mut self.input_redraw_fps,
                crate::DEFAULT_INPUT_REDRAW_FPS,
            ),
        ] {
            if !crate::is_valid_redraw_fps(*field) {
                tracing::warn!("invalid performance.{label} {}; using {default}", *field);
                *field = default;
                *changed = true;
            }
        }
    }

    /// Runtime admission check for the redraw caps. File loads use `normalize` instead.
    pub fn validate(&self) -> Result<(), String> {
        for (label, value) in [
            ("output_redraw_fps", self.output_redraw_fps),
            ("input_redraw_fps", self.input_redraw_fps),
        ] {
            if !crate::is_valid_redraw_fps(value) {
                return Err(format!(
                    "performance.{label} must be one of {:?} or -1, got {value}",
                    crate::REDRAW_FPS_CHOICES
                ));
            }
        }
        Ok(())
    }

    /// Applied output redraw cap: `None` is no cap (monitor refresh rate).
    pub fn output_redraw_cap(&self) -> Option<u32> {
        crate::redraw_fps_cap(self.output_redraw_fps, crate::DEFAULT_OUTPUT_REDRAW_FPS)
    }

    /// Applied input redraw cap: `None` is no cap (monitor refresh rate).
    pub fn input_redraw_cap(&self) -> Option<u32> {
        crate::redraw_fps_cap(self.input_redraw_fps, crate::DEFAULT_INPUT_REDRAW_FPS)
    }
}

/// Map a `scrollback_disk_lines` value to a line cap (`None` = unlimited).
pub fn disk_scrollback_cap(lines: i64) -> Option<usize> {
    match lines {
        UNLIMITED_SCROLLBACK_DISK_LINES => None,
        n if n < 0 => usize::try_from(DEFAULT_SCROLLBACK_DISK_LINES).ok(),
        n => Some(usize::try_from(n).unwrap_or(usize::MAX)),
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            targeted_pty_polling: true,
            scrollback_disk_swap: false,
            scrollback_disk_lines: DEFAULT_SCROLLBACK_DISK_LINES,
            output_redraw_fps: crate::DEFAULT_OUTPUT_REDRAW_FPS,
            input_redraw_fps: crate::DEFAULT_INPUT_REDRAW_FPS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub sound: bool,
    pub coalesce_ms: u64,
}

/// 수동 접근성 설정. OS 상태 자동 감지는 구현하지 않았다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AccessibilitySettings {
    /// 이 설정을 따르는 UI 애니메이션을 즉시 끝낸다. 토스트 페이드 시간도 0으로 적용한다.
    pub reduced_motion: bool,
}

/// 오버레이 표시 설정. 현재는 토스트 수명을 보관한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    /// 토스트가 자동 소멸하기 전 화면에 머무는 시간(ms). UI 는 초 단위로 노출하되
    /// (1.0~10.0s, 0.5s step) 내부 저장은 ms. 기본 2000ms.
    pub toast_duration_ms: u64,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            toast_duration_ms: 2000,
        }
    }
}

/// Modifier 키 안내의 표시 여부·위치·크기. 화면 밖 저장값의 보정은 렌더 단계가 맡는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModifierHintSettings {
    /// 오버레이 표시 여부. 기본값은 true다.
    pub enabled: bool,
    /// 사용자가 이동한 위치. `None` = 기본 위치(렌더 단계 결정).
    pub pos: Option<(LogicalPx, LogicalPx)>,
    /// 사용자가 리사이즈한 크기. `None` = 기본 크기(렌더 단계 결정).
    pub size: Option<(LogicalPx, LogicalPx)>,
}

impl Default for ModifierHintSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            pos: None,
            size: None,
        }
    }
}

/// 메모리 저장소 quota 설정(MiB). unsigned 정수로 음수는 읽지 못하며, 값의 유효성은 호출자가 검사한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorySettings {
    /// 단일 entry 의 value 최대 byte (MiB).
    pub entry_max_mb: u64,
    /// 각 plugin 의 secret 영역 (`owner` 별) 최대 byte (MiB).
    pub secret_quota_mb_per_plugin: u64,
    /// Regular 영역 전체 합산 최대 byte (MiB).
    pub regular_quota_mb_total: u64,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            entry_max_mb: 1,
            secret_quota_mb_per_plugin: 10,
            regular_quota_mb_total: 1024,
        }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sound: false,
            coalesce_ms: 500,
        }
    }
}

/// 웹훅 리스너 설정. 포트는 데이터 폴더의 webhooks.toml에 따로 저장한다.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WebhookSettings {
    /// true면 리스너가 모든 IPv4 인터페이스(0.0.0.0)에서 받는다. 기본은 loopback(127.0.0.1)만이다.
    /// 다음 실행부터 적용된다.
    pub allow_external: bool,
}

/// 원격 파일 수신 폴더와 용량 상한. 호스트는 begin의 total_size와 현재 폴더 사용량을 비교한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteTransferSettings {
    /// 저장 폴더 경로. 빈 문자열이면 기본 폴더(`~/.tasty/transfers/`)로 도출한다
    /// (경로 관례는 `GeneralSettings.shell`/`startup_command` 와 동형 — 빈=미설정).
    pub dir: String,
    /// 저장 폴더 최대 용량(MiB). MemorySettings 와 동일한 MiB u64 관례. 기본 500 MiB.
    /// 용량 비교 시 `* 1024 * 1024` 로 바이트 환산한다.
    pub max_mb: u64,
}

impl Default for RemoteTransferSettings {
    fn default() -> Self {
        Self {
            dir: String::new(),
            max_mb: 500,
        }
    }
}
