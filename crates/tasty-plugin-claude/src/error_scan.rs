//! Claude 터미널 출력의 오류 패턴과 오래 변하지 않는 출력을 감시한다.
//! surface.read_since_scan_mark의 전용 커서로 새 텍스트를 받아 매칭 창에 누적한다.
//! 에이전트가 쓰는 mark와 별개이므로 에이전트의 조회가 이 커서를 움직이지 않는다.
//!
//! launch·spawn·respawn에서 등록하고 폴링 중 대상 상태를 확인한다.
//! 자식은 surface 존재와 별도로 부모 관계를 확인해 release된 대상도 정리한다.
//! TopLevel 조회 오류는 대상 부재 오류까지 추적 유지로 처리하는 한계가 있다.
//! 새 턴에서는 중복 알림 기록과 정지 관측을 초기화하되 알림 간격 제한은 유지한다.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::json;

use tasty_plugin_agent_common::host_call::HostCall;

use crate::task_watch::{FileActivity, TaskWatch};

/// 출력에서 찾는 오류 패턴.
const CLAUDE_ERROR_PATTERN: &str = r"(?i)(\bAPI Error\b|Output blocked by content filtering policy|\boverloaded_error\b|\brate_limit_error\b|\bInternal Server Error\b|\bnetwork error\b|\bBad Request\b)";

/// 누적 매칭 창의 바이트 상한. 호스트 OUTPUT_RETENTION_MAX_BYTES와 같은 값을 따로 선언한다.
/// 두 값이 달라지면 보관하는 출력 범위도 달라진다
/// (docs/features/terminal-output/index.md#출력-스캐너-전용-커서).
const SCAN_WINDOW_MAX: usize = 1_048_576;

/// 중복 알림 비교에 사용할 매칭 창 앞부분의 문자 수.
const DEDUPE_SNIPPET_CHARS: usize = 200;

static CLAUDE_ERROR_REGEX: OnceLock<Regex> = OnceLock::new();

fn claude_error_regex() -> &'static Regex {
    CLAUDE_ERROR_REGEX.get_or_init(|| {
        Regex::new(CLAUDE_ERROR_PATTERN).expect("ClaudeError catalog regex must compile")
    })
}

/// 오류를 본 뒤 출력 변화가 없을 때 정지 알림까지 기다리는 시간.
/// 재시도의 일시적인 대기를 정지로 오인하지 않도록 지연을 둔다.
const STALL_QUIET: Duration = Duration::from_secs(30);

/// 오류 없이 조용한 경우의 대기 시간. 긴 추론과 정지를 구분할 증거가 적어 더 오래 기다린다.
/// 호스트 CHILD_OUTPUT_SILENCE와 같은 값을 별도로 선언한다.
const STALL_QUIET_NO_ERROR: Duration = Duration::from_secs(120);

/// 백그라운드 작업을 기다리는 자식의 정지 알림 기준. Claude Code가 Stop payload로 알려 준
/// 대기이므로 일반 기준보다 길게 기다리며, 대기 한 번에 한 번만 알린다.
const STALL_QUIET_BACKGROUND_WAIT: Duration = Duration::from_secs(600);

/// 같은 surface에 정지 알림을 다시 보낼 때 지킬 최소 간격.
const STALL_NOTIFY_COOLDOWN: Duration = Duration::from_secs(300);

/// 정지 의심 알림용 이벤트. 매니페스트의 hook_events와 이름이 같아야 한다.
/// 오류 없는 정지도 알리지만 기존 구독과 호환되도록 이름은 유지한다.
pub(crate) const STALLED_EVENT: &str = "claude-error-stalled";

/// `text`(ANSI-stripped 권장)에 알려진 Claude 에러 패턴이 포함됐는지.
pub(crate) fn detect_claude_error(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    claude_error_regex().is_match(text)
}

/// `text` 에서 에러 패턴이 매치된 **첫 줄** 전체를 돌려준다 — 알림에 붙일 힌트용.
/// 매칭 규칙 자체는 [`detect_claude_error`] 와 같은 정규식을 그대로 쓴다.
pub(crate) fn first_error_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && claude_error_regex().is_match(line))
}

/// 독립 surface는 surface.locate, 자식은 terminal.parent로 추적 유지 여부를 판단한다.
/// release는 surface를 남기고 부모 관계만 해제하므로 자식은 존재 여부만 보면 안 된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanTarget {
    TopLevel,
    Child,
}

/// 스캐너의 모든 잠금에서 poison을 알리고 기존 상태를 재사용한다.
/// 복구가 패닉 전의 부분 변경을 되돌리는 것은 아니다.
pub(crate) fn lock_scanner(
    scanner: &std::sync::Mutex<ErrorScanner>,
) -> std::sync::MutexGuard<'_, ErrorScanner> {
    const WHAT: &str = "the claude error scanner";
    static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    tasty_utils::poison::recover_mutex(scanner.lock(), WHAT, &REPORTED)
}

#[derive(Default)]
pub struct ErrorScanner {
    /// 추적할 surface와 등록 경로. launch·spawn·respawn에서 넣고 kill·폴링 정리에서 뺀다.
    enabled: HashMap<u32, ScanTarget>,
    /// surface별로 마지막 알림에 사용한 창의 앞부분. 같은 값이면 중복 알림을 생략한다.
    last_fired: HashMap<u32, String>,
    /// 출력 변화가 멈춘 기간을 추정할 관측값.
    watch: HashMap<u32, OutputWatch>,
    /// 전용 커서로 받은 텍스트를 누적한 매칭 창. SCAN_WINDOW_MAX를 넘으면 앞에서 자른다.
    window: HashMap<u32, String>,
    /// surface별 마지막 정지 알림 시각.
    last_stall_notify: HashMap<u32, Instant>,
    /// 백그라운드 작업을 기다리는 surface. 대기 Stop에서 넣고 턴 종료·새 턴·세션 종료에서 뺀다.
    background_wait: HashMap<u32, BackgroundWait>,
    /// 정지 알림을 meta [`STALL_NOTICE_META_KEY`] 에 남긴 surface. 그 조용한 구간이 끝나면 meta 를 지운다.
    notice_recorded: std::collections::HashSet<u32>,
}

/// 정지 알림을 보낸 시각(`at_ms`, Unix ms)을 남기는 surface meta. 플러그인이 다시 시작돼도 같은 조용한 구간을
/// 다시 알리지 않고 알림 간격도 이어 가려는 것이다. 출력이 바뀌거나 새 턴이 시작되면 지운다.
pub(crate) const STALL_NOTICE_META_KEY: &str = "claude-stall-notice";

/// 현재 시각(Unix ms).
fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 백그라운드 작업 대기 한 번의 기록.
struct BackgroundWait {
    /// 대기가 시작된 시각(Unix ms). 알림 문구의 경과 시간에 쓴다.
    since_ms: u64,
    /// 이 대기에서 정지 알림을 보냈는지.
    notified: bool,
    /// 기다리는 작업과 출력 파일을 찾을 단서. 없으면 화면 출력 기준(10분)만 쓴다.
    watch: Option<TaskWatch>,
    /// 찾은 출력 파일의 관측 상태.
    files: Option<FileActivity>,
    /// 마지막으로 출력 파일을 찾아본 시각. 못 찾으면 [`LOCATE_RETRY`] 뒤에 다시 찾는다.
    last_locate: Option<Instant>,
    /// 대상이 있는 출력 파일을 못 본 채 지난 구간의 시작. 링크 대상은 Stop 직후 잠시 없을 수 있어 바로 경고하지 않는다.
    unresolved_since: Option<Instant>,
    /// 못 찾았다는 경고를 이 대기에서 남겼는지.
    warned: bool,
    /// 기다리는 작업의 종류. 대기 meta 에 다시 쓸 때 쓴다.
    types: Vec<String>,
}

/// surface meta `claude-background-wait` 에 남긴 대기. 플러그인이 다시 시작되면 이 값으로 대기를 되살린다.
pub(crate) struct SavedWait {
    pub since_ms: u64,
    pub notified: bool,
    pub types: Vec<String>,
    pub watch: Option<TaskWatch>,
}

impl SavedWait {
    /// meta 문자열을 읽는다. JSON 이 아니거나 시작 시각이 없으면 `None` 이다.
    pub fn parse(raw: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(raw)
            .inspect_err(|e| tracing::warn!("claude background wait meta is not JSON ({e}): {raw}"))
            .ok()?;
        Some(Self {
            since_ms: v.get("since_ms")?.as_u64()?,
            notified: v
                .get("notified")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            types: v
                .get("types")
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            watch: v.get("watch").and_then(TaskWatch::from_meta),
        })
    }
}

/// 출력 파일을 못 찾았을 때 다시 찾는 간격. Claude Code 가 폴더를 늦게 만들 수 있어 한 번에 포기하지 않는다.
const LOCATE_RETRY: Duration = Duration::from_secs(10);

impl BackgroundWait {
    fn new(since_ms: u64) -> Self {
        Self {
            since_ms,
            notified: false,
            watch: None,
            files: None,
            last_locate: None,
            unresolved_since: None,
            warned: false,
            types: Vec::new(),
        }
    }

    fn restored(saved: SavedWait) -> Self {
        Self {
            notified: saved.notified,
            watch: saved.watch,
            types: saved.types,
            ..Self::new(saved.since_ms)
        }
    }

    /// 대기 meta 값. 알림 여부와 출력 파일 단서를 함께 남겨 플러그인이 다시 시작돼도 같은 대기로 이어 간다.
    fn meta_value(&self) -> String {
        let mut v = json!({
            "since_ms": self.since_ms,
            "tasks": self.types.len(),
            "types": self.types,
            "notified": self.notified,
        });
        if let Some(watch) = &self.watch {
            v["watch"] = watch.to_meta();
        }
        v.to_string()
    }

    /// 기다리는 작업의 출력 파일이 모두 활동 없이 지난 시간. 파일을 찾지 못했거나 대상이 있는 파일이 없으면
    /// (transcript 가 없는 서브에이전트 링크뿐이면) `None` 이며 화면 기준으로 돌아간다.
    fn file_quiet(&mut self, surface_id: u32, now: Instant) -> Option<Duration> {
        let watch = self.watch.as_ref()?;
        let due = self
            .last_locate
            .is_none_or(|t| now.saturating_duration_since(t) >= LOCATE_RETRY);
        if self.files.is_none() && due {
            self.last_locate = Some(now);
            self.files = crate::task_watch::tasks_dir(watch)
                .and_then(|dir| FileActivity::locate(watch, &dir));
        }
        let quiet = self
            .files
            .as_mut()
            .and_then(|f| f.poll(std::time::SystemTime::now()));
        if quiet.is_some() {
            self.unresolved_since = None;
            return quiet;
        }
        let since = *self.unresolved_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= STALL_QUIET_NO_ERROR && !self.warned {
            self.warned = true;
            tracing::warn!(
                "claude stall s{surface_id}: no output file found for background task(s) {:?} of session {} — falling back to the screen output rule",
                watch
                    .tasks
                    .iter()
                    .map(|t| t.id.as_str())
                    .collect::<Vec<_>>(),
                watch.session_id
            );
        }
        None
    }
}

/// 누적 창의 해시 변화로 출력 변화를 추정한다. 실제 작업 진행 여부를 확정하지는 않는다.
struct OutputWatch {
    /// 마지막으로 본 텍스트의 해시.
    fingerprint: u64,
    /// 지문이 마지막으로 바뀐 시각.
    last_change: Instant,
    /// 현재 조용한 구간에서 알림을 보냈는지. 창의 해시가 바뀌면 초기화한다.
    stall_notified: bool,
    /// 마지막 창에 오류가 있었는지. 정지 알림까지 기다릴 시간을 고른다.
    saw_error: bool,
    /// 이 조용한 구간에서 상태가 정지 알림을 허용하지 않음(idle·needs_input·exited)을 확인했는지.
    /// 그 상태는 hook 이 보고한 값이라 출력 변화나 이 surface 의 hook 없이는 바뀌지 않으므로,
    /// 그때까지 대기 기록·상태 조회를 tick 마다 반복하지 않는다.
    settled: bool,
}

fn output_fingerprint(text: &str) -> u64 {
    // 앞부분이 같아도 뒤에 붙은 출력은 감지해야 하므로 창 전체의 해시를 쓴다.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// UTF-8 문자 경계를 지키며 앞에서 잘라 바이트 상한 이하로 줄인다.
fn trim_window(window: &mut String) {
    if window.len() <= SCAN_WINDOW_MAX {
        return;
    }
    let want = window.len() - SCAN_WINDOW_MAX;
    let cut = (want..=window.len())
        .find(|i| window.is_char_boundary(*i))
        .unwrap_or(window.len());
    window.drain(..cut);
}

/// 백그라운드 대기 여부와 오류 유무에 따라 정지 알림까지 기다릴 시간을 고른다.
fn stall_threshold(saw_error: bool, background_wait: bool) -> Duration {
    if background_wait {
        STALL_QUIET_BACKGROUND_WAIT
    } else if saw_error {
        STALL_QUIET
    } else {
        STALL_QUIET_NO_ERROR
    }
}

/// 상태 조회(IPC) 전에 값싸게 거를 수 있는 조건 — 무출력 지속시간 · 중복 · 쿨다운.
fn stall_pre_gate(
    quiet: Duration,
    already_notified: bool,
    since_last_notify: Option<Duration>,
    threshold: Duration,
) -> bool {
    if already_notified || quiet < threshold {
        return false;
    }
    !matches!(since_last_notify, Some(d) if d < STALL_NOTIFY_COOLDOWN)
}

/// active·stale에서만 정지 의심 알림을 허용한다. 두 상태가 실제 정지를 확정하는 것은 아니다.
/// idle·needs_input·exited는 기존 완료 알림 대상이라 여기서 다시 알리지 않는다.
fn state_allows_stall_notice(child_state: &str) -> bool {
    matches!(child_state, "active" | "stale")
}

/// 정지 알림을 보낼지의 최종 판정(순수 함수).
fn should_notify_stall(
    quiet: Duration,
    child_state: &str,
    already_notified: bool,
    since_last_notify: Option<Duration>,
    threshold: Duration,
) -> bool {
    stall_pre_gate(quiet, already_notified, since_last_notify, threshold)
        && state_allows_stall_notice(child_state)
}

impl ErrorScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// 추적 대상을 등록한다. 같은 surface는 등록 경로만 바꾸고 중복 알림 기록은 유지한다.
    pub fn enable(&mut self, surface_id: u32, target: ScanTarget) {
        self.enabled.insert(surface_id, target);
    }

    pub fn disable(&mut self, surface_id: u32) {
        self.enabled.remove(&surface_id);
        self.last_fired.remove(&surface_id);
        self.watch.remove(&surface_id);
        self.last_stall_notify.remove(&surface_id);
        self.window.remove(&surface_id);
        self.background_wait.remove(&surface_id);
        self.notice_recorded.remove(&surface_id);
    }

    /// 재시작 전에 남긴 정지 알림을 되살린다. 누적 창이 빈 상태에서 시작하므로 빈 창의 해시를 기준으로 삼는다.
    /// 첫 스캔에서 새 출력이 없으면 같은 조용한 구간으로 보고 다시 알리지 않는다. 새 출력이 있으면 해시가 바뀌어
    /// 평소처럼 다시 센다. 알림 간격은 남긴 시각부터 잇는다.
    pub(crate) fn restore_stall_notice(&mut self, surface_id: u32, at_ms: u64, now: Instant) {
        if self.watch.contains_key(&surface_id) {
            return;
        }
        let elapsed = Duration::from_millis(unix_now_ms().saturating_sub(at_ms));
        self.watch.insert(
            surface_id,
            OutputWatch {
                fingerprint: output_fingerprint(""),
                last_change: now,
                stall_notified: true,
                saw_error: false,
                settled: false,
            },
        );
        self.last_stall_notify
            .insert(surface_id, now.checked_sub(elapsed).unwrap_or(now));
        self.notice_recorded.insert(surface_id);
    }

    /// 대기 Stop을 기록하고 대기 시작 시각(Unix ms)을 돌려준다. 이어진 대기 Stop은 처음 기록을 유지한다.
    pub fn mark_background_wait(&mut self, surface_id: u32, now_ms: u64) -> u64 {
        self.background_wait
            .entry(surface_id)
            .or_insert_with(|| BackgroundWait::new(now_ms))
            .since_ms
    }

    /// 대기에서 관측할 작업을 바꾼다. 작업 목록이 바뀌면 출력 파일을 다시 찾는다. 알림 여부는 유지한다.
    pub fn set_task_watch(&mut self, surface_id: u32, watch: Option<TaskWatch>) {
        if let Some(b) = self.background_wait.get_mut(&surface_id)
            && b.watch != watch
        {
            b.watch = watch;
            b.files = None;
            b.last_locate = None;
        }
    }

    /// 시험 전용 — 찾은 출력 파일을 직접 넣는다. 실제 임시 폴더를 쓰지 않으려는 것이다.
    #[cfg(test)]
    pub(crate) fn set_file_activity_for_test(
        &mut self,
        surface_id: u32,
        watch: TaskWatch,
        files: FileActivity,
    ) {
        if let Some(b) = self.background_wait.get_mut(&surface_id) {
            b.watch = Some(watch);
            b.files = Some(files);
        }
    }

    /// 대기 Stop 을 기록하고 대기 meta 값을 돌려준다. 이어진 대기 Stop 은 처음 시작 시각과 알림 여부를 유지한다.
    pub fn record_background_wait(
        &mut self,
        surface_id: u32,
        now_ms: u64,
        types: Vec<String>,
        watch: Option<TaskWatch>,
    ) -> String {
        self.mark_background_wait(surface_id, now_ms);
        self.set_task_watch(surface_id, watch);
        let b = self
            .background_wait
            .entry(surface_id)
            .or_insert_with(|| BackgroundWait::new(now_ms));
        b.types = types;
        b.meta_value()
    }

    /// meta 에 남은 대기를 되살린다. 이미 기록이 있으면(그사이 대기 Stop 이 먼저 왔으면) 그 기록을 둔다.
    pub(crate) fn restore_background_wait(&mut self, surface_id: u32, saved: SavedWait) {
        self.background_wait
            .entry(surface_id)
            .or_insert_with(|| BackgroundWait::restored(saved));
    }

    /// 백그라운드 대기 기록을 지운다.
    pub fn clear_background_wait(&mut self, surface_id: u32) {
        self.background_wait.remove(&surface_id);
    }

    /// 테스트 전용 — 대기 기록 존재 여부.
    #[cfg(test)]
    pub(crate) fn is_waiting_on_background_work(&self, surface_id: u32) -> bool {
        self.background_wait.contains_key(&surface_id)
    }

    pub fn is_enabled(&self, surface_id: u32) -> bool {
        self.enabled.contains_key(&surface_id)
    }

    /// 테스트 전용 — 등록 경로 확인.
    #[cfg(test)]
    pub(crate) fn target_of(&self, surface_id: u32) -> Option<ScanTarget> {
        self.enabled.get(&surface_id).copied()
    }

    /// 이 surface 의 hook 이 왔다. hook 은 상태를 바꿀 수 있으므로 조용한 구간의 상태 확인을 다시 하게 한다.
    pub fn note_hook(&mut self, surface_id: u32) {
        if let Some(w) = self.watch.get_mut(&surface_id) {
            w.settled = false;
        }
    }

    /// 시험 전용 — 조용한 구간의 상태 확인을 마친 것으로 둔다.
    #[cfg(test)]
    pub(crate) fn settle_for_test(&mut self, surface_id: u32) {
        self.watch.insert(
            surface_id,
            OutputWatch {
                fingerprint: 0,
                last_change: Instant::now(),
                stall_notified: false,
                saw_error: false,
                settled: true,
            },
        );
    }

    /// 시험 전용 — 정지 관측이 있는지.
    #[cfg(test)]
    pub(crate) fn is_watching(&self, surface_id: u32) -> bool {
        self.watch.contains_key(&surface_id)
    }

    /// 시험 전용 — 조용한 구간의 상태 확인을 마쳤는지.
    #[cfg(test)]
    pub(crate) fn is_settled(&self, surface_id: u32) -> bool {
        self.watch.get(&surface_id).is_some_and(|w| w.settled)
    }

    /// 새 턴을 위해 중복 알림 기록과 정지 관측을 초기화한다.
    /// 턴마다 오류가 반복돼도 알림이 너무 잦아지지 않도록 마지막 알림 시각은 유지한다.
    pub fn reset_dedupe(&mut self, surface_id: u32) {
        self.last_fired.remove(&surface_id);
        self.watch.remove(&surface_id);
        // 호스트 출력 버퍼가 턴마다 비워지지 않으므로 누적 매칭 창도 유지한다.
    }

    /// enabled set의 snapshot. polling thread가 lock을 짧게 잡고 빠져나오도록.
    pub fn enabled_snapshot(&self) -> Vec<(u32, ScanTarget)> {
        self.enabled.iter().map(|(&sid, &t)| (sid, t)).collect()
    }

    /// 호스트 IPC 없이 중복 알림 상태를 준비하는 시험용 함수.
    #[cfg(test)]
    pub(crate) fn seed_dedupe_for_test(&mut self, surface_id: u32, snippet: &str) {
        self.last_fired.insert(surface_id, snippet.to_string());
    }

    /// 테스트 전용 — 현재 조용한 구간에서 정지 알림을 보낸 것으로 기록됐는지.
    #[cfg(test)]
    pub(crate) fn stall_notified_for_test(&self, surface_id: u32) -> bool {
        self.watch
            .get(&surface_id)
            .is_some_and(|w| w.stall_notified)
    }

    /// 테스트 전용 — dedupe 상태 존재 여부.
    #[cfg(test)]
    pub(crate) fn has_dedupe_state(&self, surface_id: u32) -> bool {
        self.last_fired.contains_key(&surface_id)
    }

    /// 새 출력을 누적해 오류를 찾고 정지 의심 여부도 확인한다.
    /// 오류 훅 요청이 성공하면 스니펫을 반환한다. 수신 훅이 실제 실행됐다는 보장은 아니다.
    pub fn scan_one<H: HostCall>(&mut self, host: &H, surface_id: u32) -> Option<String> {
        self.scan_one_at(host, surface_id, Instant::now())
    }

    /// 실제로 기다리지 않고 출력 정지 기간을 시험하도록 시각을 전달받는다.
    fn scan_one_at<H: HostCall>(
        &mut self,
        host: &H,
        surface_id: u32,
        now: Instant,
    ) -> Option<String> {
        let resp = host
            .call(
                "surface.read_since_scan_mark",
                json!({
                    "surface_id": surface_id,
                    "strip_ansi": true,
                }),
            )
            .ok()?;
        // 새로 받은 텍스트만으로는 분할된 패턴을 놓칠 수 있어 창에 누적한다.
        let delta = resp.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let window = self.window.entry(surface_id).or_default();
        window.push_str(delta);
        trim_window(window);
        let has_error = detect_claude_error(window);
        let fingerprint = output_fingerprint(window);
        let snippet: String = window.chars().take(DEDUPE_SNIPPET_CHARS).collect();
        // 오류를 발견하기 전부터 창의 변화를 추적한다.
        self.track_output(surface_id, fingerprint, now, has_error);
        self.forget_ended_notice(host, surface_id);

        let mut fired = None;
        if has_error {
            // 창의 앞부분이 마지막 알림과 같으면 오류 알림을 반복하지 않는다.
            let already_fired = self.last_fired.get(&surface_id) == Some(&snippet);
            if !already_fired {
                let fire_result = host.call(
                    "surface.fire_hook",
                    json!({
                        "surface_id": surface_id,
                        "event": "claude-error",
                    }),
                );
                match fire_result {
                    Ok(_) => {
                        self.last_fired.insert(surface_id, snippet.clone());
                        fired = Some(snippet);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "claude error fire_hook failed for surface {surface_id}: {e}"
                        );
                    }
                }
            }
        }

        // 오류가 없거나 중복 알림을 생략한 경우에도 정지 의심 여부는 확인한다.
        self.maybe_notify_stall(host, surface_id, now);
        fired
    }

    /// 알림을 남긴 조용한 구간이 끝났으면(출력 변화·새 턴) 알림 meta 를 지운다.
    fn forget_ended_notice<H: HostCall>(&mut self, host: &H, surface_id: u32) {
        if !self.notice_recorded.contains(&surface_id)
            || self
                .watch
                .get(&surface_id)
                .is_some_and(|w| w.stall_notified)
        {
            return;
        }
        self.notice_recorded.remove(&surface_id);
        if let Err(e) = host.call(
            "surface.meta.unset",
            json!({ "surface_id": surface_id, "key": STALL_NOTICE_META_KEY }),
        ) {
            tracing::warn!(
                "claude stall s{surface_id}: clearing the stall notice meta failed: {e}"
            );
        }
    }

    /// 창의 해시가 바뀌면 조용한 구간을 다시 센다. 같은 창을 다시 해시하지 않도록 값으로 받는다.
    fn track_output(&mut self, surface_id: u32, fingerprint: u64, now: Instant, has_error: bool) {
        match self.watch.get_mut(&surface_id) {
            Some(w) if w.fingerprint == fingerprint => {}
            Some(w) => {
                w.fingerprint = fingerprint;
                w.last_change = now;
                w.stall_notified = false;
                w.saw_error = has_error;
                w.settled = false;
            }
            None => {
                self.watch.insert(
                    surface_id,
                    OutputWatch {
                        fingerprint,
                        last_change: now,
                        stall_notified: false,
                        saw_error: has_error,
                        settled: false,
                    },
                );
            }
        }
    }

    /// 출력 변화가 오래 없고 자식이 active·stale이면 정지 의심 훅을 요청한다.
    /// terminal.set_state는 호출하지 않아 자식 상태를 바꾸지는 않는다.
    fn maybe_notify_stall<H: HostCall>(&mut self, host: &H, surface_id: u32, now: Instant) {
        let Some(w) = self.watch.get(&surface_id) else {
            return;
        };
        if w.settled {
            return;
        }
        let quiet = now.saturating_duration_since(w.last_change);
        // 플러그인이 다시 시작되면 메모리의 대기 기록은 사라지고 surface meta 는 남는다. 알림 문구는 meta 를
        // 읽으므로, 일반 기준으로 알리기 전에 meta 에서 기록을 되살려 기준과 문구가 같은 대기를 가리키게 한다.
        if !self.background_wait.contains_key(&surface_id)
            && stall_pre_gate(
                quiet,
                w.stall_notified,
                self.last_stall_notify
                    .get(&surface_id)
                    .map(|t| now.saturating_duration_since(*t)),
                stall_threshold(w.saw_error, false),
            )
            && let Some(saved) = saved_wait_from_meta(host, surface_id)
        {
            tracing::info!(
                "claude stall s{surface_id}: restored the background wait from its surface meta"
            );
            self.restore_background_wait(surface_id, saved);
        }
        let Some(w) = self.watch.get(&surface_id) else {
            return;
        };
        // 기다리는 작업의 출력 파일을 찾았으면 화면 대신 그 파일의 활동으로 일반 기준(2분)을 적용한다.
        let file_quiet = self
            .background_wait
            .get_mut(&surface_id)
            .and_then(|b| b.file_quiet(surface_id, now));
        let wait = self.background_wait.get(&surface_id);
        // 대기 한 번에 한 번만 알린다. 출력 변화로 stall_notified가 풀려도 다시 알리지 않는다.
        let already_notified = w.stall_notified || wait.is_some_and(|b| b.notified);
        let (quiet, threshold) = match file_quiet {
            Some(file_quiet) => (file_quiet, STALL_QUIET_NO_ERROR),
            None => (quiet, stall_threshold(w.saw_error, wait.is_some())),
        };
        let since_last_notify = self
            .last_stall_notify
            .get(&surface_id)
            .map(|t| now.saturating_duration_since(*t));
        if !stall_pre_gate(quiet, already_notified, since_last_notify, threshold) {
            return;
        }

        // 시간·중복·알림 간격 조건을 통과했을 때만 호스트 상태를 조회한다.
        let child_state = host
            .call("terminal.state", json!({ "surface": surface_id }))
            .ok()
            .and_then(|r| {
                r.get("state")
                    .and_then(|v| v.as_str())
                    .map(str::to_ascii_lowercase)
            })
            .unwrap_or_default();
        if !should_notify_stall(
            quiet,
            &child_state,
            already_notified,
            since_last_notify,
            threshold,
        ) {
            // 조회에 실패했으면(빈 값) 다음 tick 에 다시 묻는다.
            if !child_state.is_empty()
                && let Some(w) = self.watch.get_mut(&surface_id)
            {
                w.settled = true;
            }
            return;
        }

        if let Some(b) = wait.filter(|_| file_quiet.is_some()) {
            record_quiet_tasks(host, surface_id, b, quiet);
        }
        self.fire_stall(host, surface_id, now);
    }

    /// 정지 의심 훅을 요청하고, 성공하면 같은 정지 구간·대기에서 다시 알리지 않도록 기록한다.
    fn fire_stall<H: HostCall>(&mut self, host: &H, surface_id: u32, now: Instant) {
        if let Err(e) = host.call(
            "surface.fire_hook",
            json!({
                "surface_id": surface_id,
                "event": STALLED_EVENT,
            }),
        ) {
            tracing::warn!("claude stall fire_hook failed for surface {surface_id}: {e}");
            return;
        }
        if let Some(w) = self.watch.get_mut(&surface_id) {
            w.stall_notified = true;
        }
        self.record_notice(host, surface_id);
        self.record_wait_notice(host, surface_id);
        self.last_stall_notify.insert(surface_id, now);
    }

    /// 플러그인이 다시 시작돼도 같은 조용한 구간을 다시 알리지 않도록 알림 시각을 남긴다.
    fn record_notice<H: HostCall>(&mut self, host: &H, surface_id: u32) {
        match host.call(
            "surface.meta.set",
            json!({ "surface_id": surface_id, "key": STALL_NOTICE_META_KEY, "value": json!({ "at_ms": unix_now_ms() }).to_string() }),
        ) {
            Ok(_) => {
                self.notice_recorded.insert(surface_id);
            }
            Err(e) => tracing::warn!("claude stall s{surface_id}: recording the stall notice failed: {e}"),
        }
    }

    /// 대기 중이면 이 대기를 알린 것으로 기록하고 대기 meta 에도 남긴다. 플러그인이 다시 시작돼도 같은 대기를 다시 알리지 않는다.
    fn record_wait_notice<H: HostCall>(&mut self, host: &H, surface_id: u32) {
        if let Some(b) = self.background_wait.get_mut(&surface_id) {
            b.notified = true;
            if let Err(e) = host.call(
                "surface.meta.set",
                json!({ "surface_id": surface_id, "key": crate::hook::BACKGROUND_WAIT_META_KEY, "value": b.meta_value() }),
            ) {
                tracing::warn!("claude stall s{surface_id}: recording the notice in the wait meta failed: {e}");
            }
        }
    }
}

/// 정지 알림 문구가 조용한 작업의 이름과 시간을 적도록 기록한다. 대기 시작 시각으로 같은 대기인지 가린다.
fn record_quiet_tasks<H: HostCall>(
    host: &H,
    surface_id: u32,
    wait: &BackgroundWait,
    quiet: Duration,
) {
    let labels = wait
        .files
        .as_ref()
        .map(FileActivity::labels)
        .unwrap_or_default();
    tracing::info!(
        "claude stall s{surface_id}: background task output quiet for {}s ({}) — notifying",
        quiet.as_secs(),
        labels.join(", ")
    );
    let value = json!({ "since_ms": wait.since_ms, "labels": labels, "quiet_ms": quiet.as_millis() as u64 });
    if let Err(e) = host.call(
        "surface.meta.set",
        json!({ "surface_id": surface_id, "key": crate::task_watch::BACKGROUND_QUIET_META_KEY, "value": value.to_string() }),
    ) {
        tracing::warn!("claude stall s{surface_id}: recording the quiet background tasks failed: {e}");
    }
}

/// surface meta `claude-background-wait` 에 남은 대기. 없거나 읽을 수 없으면 `None` 이다.
pub(crate) fn saved_wait_from_meta<H: HostCall>(host: &H, surface_id: u32) -> Option<SavedWait> {
    let reply = host
        .call(
            "surface.meta.get",
            json!({ "surface_id": surface_id, "key": crate::hook::BACKGROUND_WAIT_META_KEY }),
        )
        .inspect_err(|e| {
            tracing::warn!(
                "claude stall s{surface_id}: reading the background wait meta failed: {e}"
            )
        })
        .ok()?;
    SavedWait::parse(reply.get("value")?.as_str()?)
}

/// 폴링 중 추적 유지 여부를 확인한다. 조회 오류는 추적을 유지하도록 true로 처리한다.
/// TopLevel은 대상 부재를 오류로 돌려받아도 유지한다. Child는 정상 응답의 status=none이면 제거한다.
pub(crate) fn scan_target_is_alive<H: HostCall>(
    host: &H,
    surface_id: u32,
    target: ScanTarget,
) -> bool {
    match target {
        ScanTarget::TopLevel => host
            .call("surface.locate", json!({ "surface_id": surface_id }))
            .ok()
            .and_then(|r| r.get("exists").and_then(|v| v.as_bool()))
            .unwrap_or(true),
        // surface를 남기는 release도 감지하도록 부모 관계를 확인한다.
        ScanTarget::Child => host
            .call("terminal.parent", json!({ "surface": surface_id }))
            .ok()
            .map(|r| r.get("status").and_then(|v| v.as_str()) != Some("none"))
            .unwrap_or(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_api_error() {
        assert!(detect_claude_error("…\nAPI Error: Connection lost\n"));
    }

    #[test]
    fn detects_content_filter_block() {
        assert!(detect_claude_error(
            "Output blocked by content filtering policy"
        ));
    }

    #[test]
    fn detects_overloaded_and_rate_limit() {
        assert!(detect_claude_error("{\"type\":\"overloaded_error\"}"));
        assert!(detect_claude_error("got rate_limit_error from upstream"));
    }

    #[test]
    fn detects_case_insensitive() {
        assert!(detect_claude_error("api error: foo"));
    }

    #[test]
    fn detects_internal_server_error_and_network() {
        assert!(detect_claude_error("HTTP 500 Internal Server Error\n"));
        assert!(detect_claude_error("encountered network error: ECONNRESET"));
    }

    #[test]
    fn detects_bad_request() {
        assert!(detect_claude_error("400 Bad Request — model not allowed"));
    }

    #[test]
    fn ignores_unrelated_text() {
        assert!(!detect_claude_error("compile error fixed"));
        assert!(!detect_claude_error("no errors here"));
        assert!(!detect_claude_error(""));
    }

    #[test]
    fn enable_disable_tracking() {
        let mut s = ErrorScanner::new();
        assert!(!s.is_enabled(7));
        s.enable(7, ScanTarget::TopLevel);
        assert!(s.is_enabled(7));
        assert_eq!(s.enabled_snapshot(), vec![(7, ScanTarget::TopLevel)]);
        s.disable(7);
        assert!(!s.is_enabled(7));
    }

    #[test]
    fn enable_records_target_kind() {
        let mut s = ErrorScanner::new();
        s.enable(1, ScanTarget::TopLevel);
        s.enable(2, ScanTarget::Child);
        assert_eq!(s.target_of(1), Some(ScanTarget::TopLevel));
        assert_eq!(s.target_of(2), Some(ScanTarget::Child));
        assert_eq!(s.target_of(3), None);
    }

    #[test]
    fn re_enabling_same_surface_is_idempotent_and_keeps_dedupe() {
        // 같은 surface를 재등록해도 중복 항목을 만들지 않는다.
        let mut s = ErrorScanner::new();
        s.enable(5, ScanTarget::Child);
        s.seed_dedupe_for_test(5, "API Error: foo");
        s.enable(5, ScanTarget::Child);
        assert_eq!(s.enabled_snapshot(), vec![(5, ScanTarget::Child)]);
        assert!(s.has_dedupe_state(5));
    }

    #[test]
    fn disable_clears_dedupe_state() {
        // 같은 surface가 disable 후 다시 enable 되면 이전 fire 기록은
        // 사라져야 한다 (새 child 인스턴스로 가정).
        let mut s = ErrorScanner::new();
        s.enable(3, ScanTarget::Child);
        s.last_fired
            .insert(3, "API Error: foo".chars().take(200).collect());
        assert!(s.last_fired.contains_key(&3));
        s.disable(3);
        assert!(!s.last_fired.contains_key(&3));
        assert!(!s.has_dedupe_state(3));
    }

    #[test]
    fn reset_dedupe_clears_only_target_surface() {
        let mut s = ErrorScanner::new();
        s.last_fired.insert(1, "a".into());
        s.last_fired.insert(2, "b".into());
        s.reset_dedupe(1);
        assert!(!s.last_fired.contains_key(&1));
        assert!(s.last_fired.contains_key(&2));
    }

    #[test]
    fn stall_needs_sustained_silence() {
        // 정적이 짧으면 아직 판정하지 않는다 — 재시도 백오프 중일 수 있다.
        assert!(!should_notify_stall(
            STALL_QUIET - Duration::from_secs(1),
            "active",
            false,
            None,
            STALL_QUIET
        ));
        assert!(should_notify_stall(
            STALL_QUIET,
            "active",
            false,
            None,
            STALL_QUIET
        ));
    }

    #[test]
    fn stall_only_when_the_turn_has_not_ended() {
        // 기존 완료 알림 대상인 상태에서는 정지 알림을 추가하지 않는다.
        for state in ["idle", "needs_input", "exited", ""] {
            assert!(
                !should_notify_stall(STALL_QUIET * 2, state, false, None, STALL_QUIET),
                "state={state} 에서는 정지 알림이 나가면 안 된다"
            );
        }
        assert!(should_notify_stall(
            STALL_QUIET * 2,
            "active",
            false,
            None,
            STALL_QUIET
        ));
        // stale도 정지 의심 알림 대상이다.
        assert!(should_notify_stall(
            STALL_QUIET * 2,
            "stale",
            false,
            None,
            STALL_QUIET
        ));
    }

    #[test]
    fn stall_notifies_once_per_silence_and_respects_cooldown() {
        // 같은 정적 구간에서 반복 알림 금지.
        assert!(!should_notify_stall(
            STALL_QUIET * 3,
            "active",
            true,
            None,
            STALL_QUIET
        ));
        // 쿨다운 안이면 새 정적 구간이라도 억제 — 에러 반복 세션의 빈도 상한.
        assert!(!should_notify_stall(
            STALL_QUIET * 3,
            "active",
            false,
            Some(STALL_NOTIFY_COOLDOWN - Duration::from_secs(1)),
            STALL_QUIET
        ));
        assert!(should_notify_stall(
            STALL_QUIET * 3,
            "active",
            false,
            Some(STALL_NOTIFY_COOLDOWN),
            STALL_QUIET
        ));
    }

    #[test]
    fn the_threshold_depends_on_whether_an_error_is_on_screen() {
        assert_eq!(stall_threshold(true, false), STALL_QUIET);
        assert_eq!(stall_threshold(false, false), STALL_QUIET_NO_ERROR);
        // 백그라운드 대기는 오류 유무와 무관하게 대기 기준을 쓴다.
        assert_eq!(stall_threshold(true, true), STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(stall_threshold(false, true), STALL_QUIET_BACKGROUND_WAIT);
        assert!(
            STALL_QUIET_NO_ERROR > STALL_QUIET,
            "보강 증거가 없으면 더 길게 본다"
        );
        // 에러 없는 정적은 짧은 문턱으로는 정지가 아니다.
        assert!(!should_notify_stall(
            STALL_QUIET,
            "active",
            false,
            None,
            STALL_QUIET_NO_ERROR
        ));
        assert!(should_notify_stall(
            STALL_QUIET_NO_ERROR,
            "active",
            false,
            None,
            STALL_QUIET_NO_ERROR
        ));
    }

    #[test]
    fn output_fingerprint_tracks_appended_text() {
        // 창 앞부분이 같아도 뒤의 출력이 늘면 해시가 달라져야 한다.
        let head = "x".repeat(300);
        let grown = format!("{head}retrying…");
        assert_eq!(
            head.chars().take(200).collect::<String>(),
            grown.chars().take(200).collect::<String>(),
            "스니펫은 같다(전제)"
        );
        assert_ne!(output_fingerprint(&head), output_fingerprint(&grown));
    }

    #[test]
    fn first_error_line_extracts_matching_line() {
        let text = "building…\n  API Error: connection reset by peer\nnext line\n";
        assert_eq!(
            first_error_line(text),
            Some("API Error: connection reset by peer")
        );
        assert_eq!(first_error_line("all good\n"), None);
    }

    /// 새 출력·자식 상태를 제공하고 훅 요청을 기록하는 stub.
    /// set_text로 지정한 텍스트는 한 번만 읽히며 이후에는 빈 문자열을 반환한다.
    struct ScanHost {
        text: std::cell::RefCell<String>,
        state: std::cell::RefCell<&'static str>,
        fired: std::cell::RefCell<Vec<String>>,
        /// surface meta `claude-background-wait` 값. 없으면 meta 가 없는 것이다.
        wait_meta: Option<String>,
        /// surface.meta.set 으로 받은 (key, value).
        meta_sets: std::cell::RefCell<Vec<(String, String)>>,
        /// 받은 호출의 method 순서.
        calls: std::cell::RefCell<Vec<String>>,
    }

    impl ScanHost {
        fn new(text: &str, state: &'static str) -> Self {
            Self {
                text: std::cell::RefCell::new(text.to_string()),
                state: std::cell::RefCell::new(state),
                fired: std::cell::RefCell::new(Vec::new()),
                wait_meta: None,
                meta_sets: std::cell::RefCell::new(Vec::new()),
                calls: std::cell::RefCell::new(Vec::new()),
            }
        }
        fn count(&self, method: &str) -> usize {
            self.calls.borrow().iter().filter(|m| *m == method).count()
        }
        fn set_text(&self, text: &str) {
            *self.text.borrow_mut() = text.to_string();
        }
        fn events(&self) -> Vec<String> {
            self.fired.borrow().clone()
        }
        fn stalled_count(&self) -> usize {
            self.events().iter().filter(|e| *e == STALLED_EVENT).count()
        }
    }

    impl HostCall for ScanHost {
        fn call(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, tasty_plugin_sdk::PluginError> {
            self.calls.borrow_mut().push(method.to_string());
            match method {
                "surface.read_since_scan_mark" => {
                    let delta = std::mem::take(&mut *self.text.borrow_mut());
                    Ok(json!({ "text": delta }))
                }
                "terminal.state" => Ok(json!({ "state": *self.state.borrow() })),
                "surface.meta.get" if params["key"] == crate::hook::BACKGROUND_WAIT_META_KEY => {
                    Ok(match &self.wait_meta {
                        Some(v) => json!({ "value": v }),
                        None => json!({}),
                    })
                }
                "surface.fire_hook" => {
                    let event = params["event"].as_str().unwrap_or_default().to_string();
                    self.fired.borrow_mut().push(event);
                    Ok(json!({ "fired": 1 }))
                }
                "surface.meta.set" => {
                    let key = params["key"].as_str().unwrap_or_default();
                    self.fired.borrow_mut().push(format!("meta-set:{key}"));
                    self.meta_sets.borrow_mut().push((
                        key.to_string(),
                        params["value"].as_str().unwrap_or_default().to_string(),
                    ));
                    Ok(json!({}))
                }
                "surface.meta.unset" => {
                    let key = params["key"].as_str().unwrap_or_default();
                    self.fired.borrow_mut().push(format!("meta-unset:{key}"));
                    Ok(json!({}))
                }
                other => panic!("unexpected host call: {other}"),
            }
        }
    }

    const ERR: &str = "⎿ API Error: Connection error\n";

    #[test]
    fn transient_error_that_keeps_producing_output_never_stalls() {
        // 반복해서 바뀌는 출력은 조용한 구간으로 판정하지 않는다.
        let host = ScanHost::new(ERR, "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        for i in 1..=6u32 {
            host.set_text(&format!("{ERR}Retrying in {i}s… (attempt {i}/10)"));
            s.scan_one_at(&host, 1, t0 + Duration::from_secs(10 * u64::from(i)));
        }
        assert_eq!(host.stalled_count(), 0, "재시도 중에는 알림이 없어야 한다");
        // 오류 감지 이벤트와 부모에게 전달할 정지 알림은 별개다.
        assert!(
            host.events().iter().all(|e| e == "claude-error"),
            "정지 이벤트가 섞이면 안 된다: {:?}",
            host.events()
        );
    }

    #[test]
    fn error_followed_by_silence_fires_stalled_once() {
        let host = ScanHost::new(ERR, "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        assert_eq!(host.stalled_count(), 0, "에러 직후엔 아직 정지가 아니다");

        // 오류 중복 알림을 생략해도 정지 판정은 계속한다.
        s.scan_one_at(&host, 1, t0 + STALL_QUIET);
        assert_eq!(
            host.stalled_count(),
            1,
            "출력 변화가 없는 시간이 기준에 도달하면 한 번 알린다"
        );

        // 계속 조용해도 같은 정적 구간에서는 다시 알리지 않는다.
        s.scan_one_at(&host, 1, t0 + STALL_QUIET * 3);
        assert_eq!(
            host.stalled_count(),
            1,
            "같은 조용한 구간에서는 한 번만 알린다"
        );
    }

    #[test]
    fn silence_after_turn_ended_does_not_fire_stalled() {
        // idle은 정지 의심 알림에서 제외한다.
        let host = ScanHost::new(ERR, "idle");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET * 2);
        assert_eq!(host.stalled_count(), 0);
    }

    #[test]
    fn new_turn_resets_stall_watch() {
        let host = ScanHost::new(ERR, "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        // 새 턴 신호(prompt-submit 등)로 dedupe + 정지 관측이 초기화된다.
        s.reset_dedupe(1);
        assert!(!s.has_dedupe_state(1));
        // 이전 턴의 정적이 이월되지 않으므로 바로 다음 tick 에 정지로 판정되지 않는다.
        s.scan_one_at(&host, 1, t0 + STALL_QUIET);
        assert_eq!(host.stalled_count(), 0, "정적 구간은 새 턴부터 다시 잰다");
        s.scan_one_at(&host, 1, t0 + STALL_QUIET * 2);
        assert_eq!(host.stalled_count(), 1);
    }

    #[test]
    fn clean_output_never_fires_an_error_event() {
        // 오류 패턴이 없으면 claude-error는 보내지 않는다. 정지 의심 알림은 별개다.
        let host = ScanHost::new("running tests…\nall good\n", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        // 문턱 이전까지는 아무것도 안 나간다.
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR - Duration::from_secs(1));
        assert!(
            host.events().is_empty(),
            "이벤트가 없어야 한다: {:?}",
            host.events()
        );
        // 문턱을 넘겨도 에러 이벤트는 없다.
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR * 5);
        assert!(
            !host.events().iter().any(|e| e == "claude-error"),
            "에러가 없는데 claude-error 가 나갔다: {:?}",
            host.events()
        );
    }

    /// 오류 없이 오래 조용한 경우에도 정지 의심 알림을 보낸다.
    #[test]
    fn silence_without_error_fires_stall() {
        let host = ScanHost::new("waiting for your approval…\n", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        assert_eq!(host.stalled_count(), 0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 1, "에러가 없어도 정적이 길면 알린다");
        assert!(
            host.events()
                .iter()
                .filter(|e| !e.starts_with("meta-set:"))
                .all(|e| e == STALLED_EVENT),
            "에러 이벤트가 섞이면 안 된다: {:?}",
            host.events()
        );
    }

    /// 호스트가 stale로 판정한 경우도 알린다.
    #[test]
    fn stale_state_fires_stall() {
        let host = ScanHost::new("…\n", "stale");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 1);
    }

    /// 기존 완료 알림 대상 상태에서는 정지 의심 알림을 보내지 않는다.
    #[test]
    fn idle_and_needs_input_still_suppressed() {
        for state in ["idle", "needs_input", "exited"] {
            let host = ScanHost::new("…\n", state);
            let mut s = ErrorScanner::new();
            let t0 = Instant::now();
            s.scan_one_at(&host, 1, t0);
            s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR * 2);
            assert_eq!(host.stalled_count(), 0, "state={state}");
        }
    }

    /// 쉬는 Claude 는 하단 상태줄(업데이트 확인 등)을 가끔 다시 그린다. 출력이 바뀔 때마다 새 조용한
    /// 구간이 시작되지만, idle 로 남아 있는 한 구간이 몇 번 반복돼도 정지 알림은 없다.
    /// 같은 출력 변화가 active 대상에는 구간마다 알림을 만든다(대조군).
    #[test]
    fn status_line_redraws_of_an_idle_target_never_fire_a_stall() {
        for (state, expected) in [("idle", 0), ("active", 3)] {
            let host = ScanHost::new("TASK COMPLETE\n", state);
            let mut s = ErrorScanner::new();
            let mut t = Instant::now();
            s.scan_one_at(&host, 1, t);
            for redraw in [
                "\rChecking for updates",
                "\rRestart to update",
                "\r/restart to apply",
            ] {
                t += STALL_NOTIFY_COOLDOWN + STALL_QUIET_NO_ERROR;
                s.scan_one_at(&host, 1, t);
                host.set_text(redraw);
                s.scan_one_at(&host, 1, t);
            }
            t += STALL_QUIET_NO_ERROR;
            s.scan_one_at(&host, 1, t);
            assert_eq!(host.stalled_count(), expected, "state={state}");
        }
    }

    /// 800ms tick 을 `n` 번 돌린다.
    fn ticks(s: &mut ErrorScanner, host: &ScanHost, from: Instant, n: u32) -> Instant {
        let mut t = from;
        for _ in 0..n {
            t += Duration::from_millis(800);
            s.scan_one_at(host, 1, t);
        }
        t
    }

    /// idle 로 조용한 대상은 조용한 구간마다 대기 기록·상태를 한 번만 묻는다.
    #[test]
    fn a_quiet_idle_target_is_checked_once_per_quiet_stretch() {
        for state in ["idle", "needs_input", "exited"] {
            let host = ScanHost::new("…\n", state);
            let mut s = ErrorScanner::new();
            let t0 = Instant::now();
            s.scan_one_at(&host, 1, t0);
            ticks(&mut s, &host, t0 + STALL_QUIET_NO_ERROR, 100);
            assert_eq!(host.count("terminal.state"), 1, "state={state}");
            assert_eq!(host.count("surface.meta.get"), 1, "state={state}");
            assert_eq!(host.stalled_count(), 0, "state={state}");
        }
    }

    /// 상태를 읽지 못했으면 확인한 것으로 치지 않고 다음 tick 에 다시 묻는다.
    #[test]
    fn a_failed_state_read_is_retried_next_tick() {
        let host = ScanHost::new("…\n", "");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        ticks(&mut s, &host, t0 + STALL_QUIET_NO_ERROR, 5);
        assert_eq!(host.count("terminal.state"), 5);
    }

    /// hook 이 상태를 active 로 되돌리면 다음 tick 에 다시 물어 이전과 같은 시점에 알린다.
    #[test]
    fn a_hook_rearms_the_state_check_without_delaying_the_stall() {
        let host = ScanHost::new("…\n", "idle");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        let t = ticks(&mut s, &host, t0 + STALL_QUIET_NO_ERROR, 10);
        assert_eq!(host.count("terminal.state"), 1);
        *host.state.borrow_mut() = "active";
        s.note_hook(1);
        s.scan_one_at(&host, 1, t + Duration::from_millis(800));
        assert_eq!(host.count("terminal.state"), 2);
        assert_eq!(host.stalled_count(), 1, "hook 다음 tick 에 알린다");
    }

    /// 출력이 바뀌면 새 조용한 구간이라 기준 시간이 지난 뒤 다시 묻는다.
    #[test]
    fn new_output_starts_a_new_quiet_stretch_for_the_state_check() {
        let host = ScanHost::new("…\n", "idle");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        let t = ticks(&mut s, &host, t0 + STALL_QUIET_NO_ERROR, 10);
        assert_eq!(host.count("terminal.state"), 1);
        host.set_text("more\n");
        s.scan_one_at(&host, 1, t);
        ticks(&mut s, &host, t, 10);
        assert_eq!(
            host.count("terminal.state"),
            1,
            "새 구간은 기준 시간 전에는 묻지 않는다"
        );
        ticks(&mut s, &host, t + STALL_QUIET_NO_ERROR, 10);
        assert_eq!(host.count("terminal.state"), 2);
    }

    /// 출력이 재개되면 dedupe 가 풀리고 다음 정적 구간은 새 사건으로 센다.
    #[test]
    fn output_resumption_rearms_and_does_not_double_fire() {
        let host = ScanHost::new("thinking…\n", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR * 2);
        assert_eq!(host.stalled_count(), 1, "같은 정적 구간에서는 1 회");

        // 출력 재개 → 다시 정적. 쿨다운을 넘긴 시점이라 두 번째 사건이 나간다.
        let t1 = t0 + STALL_NOTIFY_COOLDOWN + STALL_QUIET_NO_ERROR;
        host.set_text("thinking…\nstill here\n");
        s.scan_one_at(&host, 1, t1);
        assert_eq!(host.stalled_count(), 1, "재개 직후는 정적이 아니다");
        s.scan_one_at(&host, 1, t1 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 2);
    }

    /// 백그라운드 작업을 기다리는 자식은 일반 기준(120초)에 정지 알림을 받지 않는다.
    #[test]
    fn a_surface_waiting_on_background_work_gets_no_notice_at_the_normal_threshold() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR + Duration::from_secs(1));
        assert_eq!(host.stalled_count(), 0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1, "대기 기준에 도달하면 한 번 알린다");
    }

    /// 플러그인이 다시 시작돼 메모리 기록이 없어도 대기 meta 가 남아 있으면 대기 기준을 쓴다.
    /// 알림 문구도 meta 를 읽으므로 기준과 문구가 같은 대기를 가리킨다.
    #[test]
    fn a_wait_left_in_the_surface_meta_is_restored_before_the_normal_notice() {
        let mut host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        host.wait_meta = Some(r#"{"since_ms":1000,"tasks":1,"types":["shell"]}"#.to_string());
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR + Duration::from_secs(1));
        assert_eq!(host.stalled_count(), 0, "일반 기준에서는 알리지 않는다");
        assert!(s.is_waiting_on_background_work(1));
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1, "대기 기준에 도달하면 한 번 알린다");
    }

    /// 화면 기준 정지 알림은 meta 에 시각을 남긴다. 플러그인이 다시 시작돼 그 시각을 되살리면, 새 출력이 없는 한
    /// 같은 조용한 구간을 다시 알리지 않는다. 출력이 다시 바뀌면 meta 를 지우고 다음 조용한 구간은 평소처럼 알린다.
    #[test]
    fn a_screen_notice_before_a_restart_is_not_repeated_while_the_output_stays() {
        let host = ScanHost::new("thinking…\n", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 1);
        let saved = host
            .meta_sets
            .borrow()
            .iter()
            .find(|(k, _)| k == STALL_NOTICE_META_KEY)
            .map(|(_, v)| v.clone())
            .expect("알림 시각을 남긴다");
        let at_ms = serde_json::from_str::<serde_json::Value>(&saved).unwrap()["at_ms"]
            .as_u64()
            .unwrap();

        // 재시작: 메모리 기록 없이 meta 의 시각만 되살린다. 그사이 새 출력은 없다.
        let restarted_host = ScanHost::new("", "active");
        let mut restarted = ErrorScanner::new();
        restarted.restore_stall_notice(1, at_ms, t0);
        for k in 0..6 {
            restarted.scan_one_at(&restarted_host, 1, t0 + STALL_QUIET_NO_ERROR * k);
        }
        assert_eq!(restarted_host.stalled_count(), 0, "같은 조용한 구간");
        assert!(
            !restarted_host
                .events()
                .contains(&format!("meta-unset:{STALL_NOTICE_META_KEY}"))
        );

        // 출력이 바뀌면 그 구간은 끝났다. meta 를 지우고 다음 구간은 알린다(알림 간격은 지났다).
        let t1 = t0 + STALL_QUIET_NO_ERROR * 6;
        restarted_host.set_text("next step\n");
        restarted.scan_one_at(&restarted_host, 1, t1);
        assert!(
            restarted_host
                .events()
                .contains(&format!("meta-unset:{STALL_NOTICE_META_KEY}"))
        );
        restarted.scan_one_at(&restarted_host, 1, t1 + STALL_QUIET_NO_ERROR);
        assert_eq!(restarted_host.stalled_count(), 1);
    }

    /// 되살린 알림 뒤 첫 스캔에 새 출력이 있으면 재시작 사이에 구간이 끝난 것이다. 그 뒤 조용해지면 알림 간격을
    /// 지킨 채 다시 알린다.
    #[test]
    fn a_restored_notice_is_dropped_when_output_arrived_during_the_restart() {
        let host = ScanHost::new("resumed output\n", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        let long_ago = super::unix_now_ms() - STALL_NOTIFY_COOLDOWN.as_millis() as u64;
        s.restore_stall_notice(1, long_ago, t0);
        assert!(s.stall_notified_for_test(1));
        s.scan_one_at(&host, 1, t0);
        assert!(!s.stall_notified_for_test(1));
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 1);
    }

    /// 대기 알림은 대기 meta 에도 남는다. 플러그인이 다시 시작돼 그 meta 로 대기를 되살리면 같은 대기를 다시 알리지 않는다.
    #[test]
    fn a_wait_notified_before_a_restart_is_not_notified_again() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        let mut s = ErrorScanner::new();
        let first = s.record_background_wait(1, 1_000, vec!["shell".into()], None);
        assert!(!SavedWait::parse(&first).unwrap().notified);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1);
        let saved = host
            .meta_sets
            .borrow()
            .iter()
            .rev()
            .find(|(k, _)| k == crate::hook::BACKGROUND_WAIT_META_KEY)
            .map(|(_, v)| v.clone())
            .expect("알림 뒤 대기 meta 를 다시 쓴다");
        let parsed = SavedWait::parse(&saved).unwrap();
        assert!(parsed.notified, "{saved}");
        assert_eq!(
            (parsed.since_ms, parsed.types),
            (1_000, vec!["shell".to_string()])
        );

        let mut restarted_host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        restarted_host.wait_meta = Some(saved);
        let mut restarted = ErrorScanner::new();
        restarted.scan_one_at(&restarted_host, 1, t0);
        restarted.scan_one_at(&restarted_host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT * 2);
        assert_eq!(restarted_host.stalled_count(), 0, "이미 알린 대기");
    }

    /// 대기 meta 는 출력 파일 단서를 싣고, 되살린 대기는 그 단서로 파일을 다시 찾는다.
    #[test]
    fn a_restored_wait_keeps_the_task_watch() {
        let watch = TaskWatch {
            session_id: "s-1".into(),
            cwd: "/w".into(),
            tasks: vec![crate::task_watch::WatchedTask {
                id: "bq1".into(),
                label: "sleep 300".into(),
            }],
        };
        let mut s = ErrorScanner::new();
        let meta = s.record_background_wait(1, 1_000, vec!["shell".into()], Some(watch.clone()));
        let mut restarted = ErrorScanner::new();
        restarted.restore_background_wait(1, SavedWait::parse(&meta).unwrap());
        assert_eq!(restarted.background_wait[&1].watch, Some(watch));
        // 그사이 대기 Stop 이 먼저 기록됐으면 되살린 값이 덮어쓰지 않는다.
        restarted.restore_background_wait(
            1,
            SavedWait::parse(r#"{"since_ms":5,"notified":true}"#).unwrap(),
        );
        assert_eq!(restarted.background_wait[&1].since_ms, 1_000);
        assert!(!restarted.background_wait[&1].notified);
    }

    fn quiet_task_files(age: Duration) -> (tempfile::TempDir, TaskWatch, FileActivity) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bq1.output");
        std::fs::write(&path, "").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() - age)
            .unwrap();
        let watch = TaskWatch {
            session_id: "s-quiet".into(),
            cwd: "/w".into(),
            tasks: vec![crate::task_watch::WatchedTask {
                id: "bq1".into(),
                label: "sleep 300".into(),
            }],
        };
        let files = FileActivity::locate(&watch, dir.path()).unwrap();
        (dir, watch, files)
    }

    /// 기다리는 작업의 출력 파일이 일반 기준(120초) 동안 그대로면 화면 출력과 관계없이 한 번 알린다.
    /// 알림 문구가 쓸 조용한 작업 기록을 이벤트보다 먼저 남긴다.
    #[test]
    fn quiet_task_output_files_are_noticed_at_the_normal_threshold() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let (_dir, watch, files) = quiet_task_files(STALL_QUIET_NO_ERROR + Duration::from_secs(5));
        s.set_file_activity_for_test(1, watch, files);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        assert_eq!(
            host.events(),
            [
                format!("meta-set:{}", crate::task_watch::BACKGROUND_QUIET_META_KEY),
                STALLED_EVENT.to_string(),
                format!("meta-set:{STALL_NOTICE_META_KEY}"),
                format!("meta-set:{}", crate::hook::BACKGROUND_WAIT_META_KEY),
            ]
        );
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT * 2);
        assert_eq!(host.stalled_count(), 1, "대기 한 번에 한 번만 알린다");
    }

    /// 출력 파일이 계속 바뀌면 화면이 대기 기준(10분)보다 오래 조용해도 알리지 않는다.
    #[test]
    fn growing_task_output_files_keep_a_quiet_screen_from_being_noticed() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let (dir, watch, files) = quiet_task_files(Duration::from_secs(1));
        s.set_file_activity_for_test(1, watch, files);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        for i in 1..=3u32 {
            std::fs::write(dir.path().join("bq1.output"), "tick\n".repeat(i as usize)).unwrap();
            s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT * i);
        }
        assert_eq!(host.stalled_count(), 0);
    }

    /// transcript 가 없는 서브에이전트 링크뿐이면 일반 기준에 알리지 않고 대기 기준(10분)으로 돌아간다.
    #[cfg(unix)]
    #[test]
    fn a_wait_with_only_a_dangling_link_uses_the_wait_threshold() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 local agent\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("agent-a1.jsonl"),
            dir.path().join("a1.output"),
        )
        .unwrap();
        let watch = TaskWatch {
            session_id: "s-dangling".into(),
            cwd: "/w".into(),
            tasks: vec![crate::task_watch::WatchedTask {
                id: "a1".into(),
                label: "subagent".into(),
            }],
        };
        let files = FileActivity::locate(&watch, dir.path()).unwrap();
        s.set_file_activity_for_test(1, watch, files);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR + Duration::from_secs(1));
        assert_eq!(host.stalled_count(), 0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1);
        let quiet = format!("meta-set:{}", crate::task_watch::BACKGROUND_QUIET_META_KEY);
        assert!(!host.events().contains(&quiet));
    }

    /// 출력 파일을 찾지 못하면 대기 기준(10분)으로 돌아간다.
    #[test]
    fn without_task_output_files_the_wait_threshold_stays() {
        let host = ScanHost::new("❯ \n⏵⏵ 1 shell\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let (_dir, mut watch, _files) = quiet_task_files(Duration::from_secs(1));
        watch.session_id = "no-such-session-7f3c1e".into();
        s.set_task_watch(1, Some(watch));
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR + Duration::from_secs(1));
        assert_eq!(host.stalled_count(), 0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1);
        let quiet = format!("meta-set:{}", crate::task_watch::BACKGROUND_QUIET_META_KEY);
        assert!(!host.events().contains(&quiet));
    }

    /// 대기 meta 가 없거나 읽을 수 없으면 일반 기준으로 알린다.
    #[test]
    fn without_a_readable_wait_meta_the_normal_notice_stays() {
        for meta in [None, Some("not json".to_string()), Some("{}".to_string())] {
            let mut host = ScanHost::new("thinking…\n", "active");
            host.wait_meta = meta.clone();
            let mut s = ErrorScanner::new();
            let t0 = Instant::now();
            s.scan_one_at(&host, 1, t0);
            s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
            assert_eq!(host.stalled_count(), 1, "{meta:?}");
            assert!(!s.is_waiting_on_background_work(1));
        }
    }

    /// 대기 한 번에 한 번만 알린다. 출력이 바뀌고 쿨다운이 지나도 다시 알리지 않는다.
    #[test]
    fn a_background_wait_is_noticed_once() {
        let host = ScanHost::new("❯ \n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(host.stalled_count(), 1);
        let t1 = t0 + STALL_QUIET_BACKGROUND_WAIT + STALL_NOTIFY_COOLDOWN;
        host.set_text("❯ \n⏵⏵ 2 shells\n");
        s.scan_one_at(&host, 1, t1);
        s.scan_one_at(&host, 1, t1 + STALL_QUIET_BACKGROUND_WAIT);
        assert_eq!(
            host.stalled_count(),
            1,
            "같은 대기에서는 다시 알리지 않는다"
        );
        // 이어진 대기 Stop은 기록을 새로 만들지 않는다.
        assert_eq!(s.mark_background_wait(1, 9_000), 1_000);
        s.scan_one_at(&host, 1, t1 + STALL_QUIET_BACKGROUND_WAIT * 2);
        assert_eq!(host.stalled_count(), 1);
    }

    /// 대기가 끝난 뒤의 조용한 턴은 다시 일반 기준으로 알린다.
    #[test]
    fn a_quiet_turn_after_the_wait_is_cleared_uses_the_normal_threshold() {
        let host = ScanHost::new("thinking…\n", "active");
        let mut s = ErrorScanner::new();
        s.mark_background_wait(1, 1_000);
        s.clear_background_wait(1);
        assert!(!s.is_waiting_on_background_work(1));
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 1, t0 + STALL_QUIET_NO_ERROR);
        assert_eq!(host.stalled_count(), 1);
    }

    #[test]
    fn disable_clears_the_background_wait() {
        let mut s = ErrorScanner::new();
        s.enable(1, ScanTarget::Child);
        s.mark_background_wait(1, 1_000);
        s.disable(1);
        assert!(!s.is_waiting_on_background_work(1));
    }

    /// 두 번의 읽기에 나뉜 오류 문자열도 누적해서 찾아야 한다.
    #[test]
    fn an_error_split_across_two_polls_is_still_detected() {
        let host = ScanHost::new("\u{23bf} API Er", "active");
        let mut s = ErrorScanner::new();
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        assert!(
            host.events().is_empty(),
            "반쪽으로는 아직 매치가 아니다: {:?}",
            host.events()
        );

        host.set_text("ror: Connection error\n");
        s.scan_one_at(&host, 1, t0 + Duration::from_millis(800));
        assert_eq!(
            host.events(),
            vec!["claude-error".to_string()],
            "두 델타를 이어 붙이면 매치여야 한다"
        );
    }

    /// 창 상한과 자르는 자리. 바이트로 자르면 UTF-8 중간을 갈라 패닉한다.
    #[test]
    fn the_window_is_capped_and_cut_on_a_char_boundary() {
        // 세 바이트짜리 문자로만 채워 상한을 조금 넘긴다 — 잘라야 할 바이트 수가
        // char 경계에 안 떨어지는 배치다.
        let mut w = "\u{ac00}".repeat(SCAN_WINDOW_MAX / 3 + 10);
        assert!(
            w.len() > SCAN_WINDOW_MAX,
            "시험 입력이 바이트 상한을 넘지 않았다"
        );
        trim_window(&mut w);
        assert!(w.len() <= SCAN_WINDOW_MAX, "상한을 안 지켰다: {}", w.len());
        assert!(
            w.starts_with('\u{ac00}'),
            "char 경계가 아닌 자리에서 잘렸다"
        );
        assert!(
            w.len() + 3 > SCAN_WINDOW_MAX,
            "필요 이상으로 잘랐다: {}",
            w.len()
        );
    }

    /// 한 surface를 해제해도 다른 surface의 상태는 유지돼야 한다.
    #[test]
    fn disable_clears_stall_state() {
        let host = ScanHost::new(ERR, "active");
        let mut s = ErrorScanner::new();
        s.enable(1, ScanTarget::Child);
        s.enable(2, ScanTarget::Child);
        let t0 = Instant::now();
        s.scan_one_at(&host, 1, t0);
        s.scan_one_at(&host, 2, t0);
        s.disable(1);
        assert!(!s.watch.contains_key(&1));
        assert!(!s.last_stall_notify.contains_key(&1));
        assert!(!s.window.contains_key(&1));
        // 끄라고 하지 않은 surface 의 감시는 그대로다.
        assert!(s.watch.contains_key(&2));
        assert!(s.window.contains_key(&2));
        assert!(s.is_enabled(2));
    }

    struct StubHost {
        locate_exists: Option<bool>,
        parent_status: Option<&'static str>,
    }

    impl HostCall for StubHost {
        fn call(
            &self,
            method: &str,
            _params: serde_json::Value,
        ) -> Result<serde_json::Value, tasty_plugin_sdk::PluginError> {
            match method {
                "surface.locate" => match self.locate_exists {
                    Some(e) => Ok(json!({ "exists": e })),
                    None => Err(tasty_plugin_sdk::PluginError::HostCall {
                        method: method.to_string(),
                        message: "host down".into(),
                        code: None,
                    }),
                },
                "terminal.parent" => match self.parent_status {
                    Some("none") => Ok(json!({ "parent_surface_id": null, "status": "none" })),
                    Some(st) => Ok(json!({ "parent_surface_id": 1, "status": st })),
                    None => Err(tasty_plugin_sdk::PluginError::HostCall {
                        method: method.to_string(),
                        message: "host down".into(),
                        code: None,
                    }),
                },
                other => panic!("unexpected host call: {other}"),
            }
        }
    }

    #[test]
    fn top_level_liveness_uses_surface_locate() {
        let alive = StubHost {
            locate_exists: Some(true),
            parent_status: None,
        };
        let dead = StubHost {
            locate_exists: Some(false),
            parent_status: None,
        };
        assert!(scan_target_is_alive(&alive, 4, ScanTarget::TopLevel));
        assert!(!scan_target_is_alive(&dead, 4, ScanTarget::TopLevel));
    }

    #[test]
    fn child_liveness_uses_parent_relation_not_surface_existence() {
        // release 재현: surface 는 살아있지만(locate 는 아예 호출되지 않아야 한다)
        // 부모-자식 관계가 끊겼으므로 스캔 대상에서 빠져야 한다.
        let released = StubHost {
            locate_exists: None,
            parent_status: Some("none"),
        };
        assert!(!scan_target_is_alive(&released, 4, ScanTarget::Child));

        let attached = StubHost {
            locate_exists: None,
            parent_status: Some("active"),
        };
        assert!(scan_target_is_alive(&attached, 4, ScanTarget::Child));
    }

    #[test]
    fn liveness_lookup_failure_keeps_target_enabled() {
        // 일시적 IPC 오류로 감시를 영구히 끄지 않는다 (재활성화 경로가 없다).
        let broken = StubHost {
            locate_exists: None,
            parent_status: None,
        };
        assert!(scan_target_is_alive(&broken, 4, ScanTarget::TopLevel));
        assert!(scan_target_is_alive(&broken, 4, ScanTarget::Child));
    }

    /// 폴링 루프(`main.rs::error_scan_loop`)의 생존 대조 tick 을 재현한다.
    fn liveness_tick<H: HostCall>(scanner: &mut ErrorScanner, host: &H) {
        for (sid, target) in scanner.enabled_snapshot() {
            if !scan_target_is_alive(host, sid, target) {
                scanner.disable(sid);
            }
        }
    }

    #[test]
    fn spawned_child_survives_ticks_while_relation_holds() {
        let mut s = ErrorScanner::new();
        s.enable(42, ScanTarget::Child); // handle_spawn
        let attached = StubHost {
            locate_exists: None,
            parent_status: Some("active"),
        };
        liveness_tick(&mut s, &attached);
        assert!(s.is_enabled(42));
    }

    #[test]
    fn released_child_is_dropped_with_its_dedupe_state() {
        // release는 surface를 남기므로 부모 관계 해제를 기준으로 정리해야 한다.
        let mut s = ErrorScanner::new();
        s.enable(42, ScanTarget::Child);
        s.seed_dedupe_for_test(42, "API Error: boom");
        let released = StubHost {
            locate_exists: Some(true),
            parent_status: Some("none"),
        };
        liveness_tick(&mut s, &released);
        assert!(!s.is_enabled(42), "release 후에는 스캔 대상에서 빠져야 함");
        assert!(!s.has_dedupe_state(42), "dedupe 상태도 함께 정리돼야 함");
    }

    #[test]
    fn top_level_launch_surface_is_not_judged_by_child_relation() {
        // launch surface 는 child registry 에 없다 — 관계로 판정하면 즉시 꺼진다.
        let mut s = ErrorScanner::new();
        s.enable(9, ScanTarget::TopLevel);
        let host = StubHost {
            locate_exists: Some(true),
            parent_status: Some("none"),
        };
        liveness_tick(&mut s, &host);
        assert!(s.is_enabled(9));
    }
}

#[cfg(test)]
mod poison_tests {
    use super::*;
    use std::sync::Mutex;

    /// poison 상태에서도 등록·조회·해제가 가능하고 폴링 대상 목록을 유지해야 한다.
    #[test]
    fn a_poisoned_scanner_keeps_the_scan_loop_alive() {
        let scanner = Mutex::new(ErrorScanner::default());
        lock_scanner(&scanner).enable(7, ScanTarget::TopLevel);

        let panicked = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _held = scanner.lock().expect("not poisoned yet");
                    panic!("poison the error scanner");
                })
                .join()
        });
        assert!(panicked.is_err(), "the helper thread must have panicked");
        assert!(
            scanner.lock().is_err(),
            "the scanner lock must actually be poisoned now"
        );

        assert_eq!(
            lock_scanner(&scanner).enabled_snapshot().len(),
            1,
            "a poisoned scanner must still hand the polling loop its work list — giving up \
             here kills error scanning for the rest of the process"
        );
        assert!(
            lock_scanner(&scanner).is_enabled(7),
            "reads must survive the poison"
        );

        lock_scanner(&scanner).enable(9, ScanTarget::Child);
        assert!(
            lock_scanner(&scanner).is_enabled(9),
            "registration must land on a poisoned scanner, or that surface is never scanned"
        );
        lock_scanner(&scanner).disable(7);
        assert!(
            !lock_scanner(&scanner).is_enabled(7),
            "deregistration must land too, or a dead surface is scanned forever"
        );
    }
}
