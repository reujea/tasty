#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub type HookId = u64;

/// 훅이 트리거됐을 때 무엇을 실행할지에 대한 바인딩.
///
/// S9 로 hook 은 더 이상 셸 명령 문자열을 직접 들지 않고, 공유 훅 핸들러
/// 레지스트리의 핸들러를 **id 로 참조**한다([`HookBinding::Handler`]). 기존 API
/// (`hook.set --command`) 호환을 위해 인라인 셸 명령은 **익명 hook 핸들러**로 감싸
/// [`HookBinding::InlineShell`] 로 보존한다 — 레지스트리를 오염시키지 않는 인라인
/// 핸들러다(export/영속화 대상이 아님).
///
/// 실제 실행(레지스트리 조회 + `source` 게이트 + ShellCommand/IpcSequence 분기)은
/// 본체 `src/hook_handler/trigger.rs` 가 담당한다 — 이 크레이트는 leaf 라 레지스트리를
/// 볼 수 없으므로 (surface, event) → binding 매칭만 하고 바인딩을 호출자에게
/// 되돌려준다.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookBinding {
    /// 공유 훅 핸들러 레지스트리의 핸들러 id 참조.
    Handler(String),
    /// 하위호환 익명 셸 핸들러 — 인라인 셸 명령 문자열.
    InlineShell(String),
}

impl HookBinding {
    /// 사람이 읽을 표시 문자열 (`hook.list` 응답용). 인라인 셸은 명령 그대로,
    /// 핸들러 참조는 `handler:<id>`.
    pub fn to_display_string(&self) -> String {
        match self {
            HookBinding::Handler(id) => format!("handler:{id}"),
            HookBinding::InlineShell(cmd) => cmd.clone(),
        }
    }
}

/// `check_and_fire` 가 반환하는 발사된 훅 1건 — hook id + 실행할 바인딩 + 매칭 이벤트.
///
/// 이 크레이트는 바인딩을 실행하지 않는다(leaf). 호출자(본체)가 `binding` 을
/// `hook_handler::trigger::execute_binding` 으로 실행하고 `hook_id` 로 host event 를
/// 큐잉한다. `event` 는 **등록된** 훅 이벤트의 사본이다(수신 이벤트가 아님 —
/// OutputMatch 는 매칭 텍스트가 아니라 등록 패턴) — 셸 핸들러 env
/// (`TASTY_HOOK_EVENT`) 등 트리거 컨텍스트 전파용. `received` 는 실제 관측값 —
/// `CommandCompleted` 의 실제 exit code, `OutputMatch` 의 실제 매칭 텍스트,
/// `IdleTimeout` 의 실제 경과초 등 — 트리거 payload(`TASTY_HOOK_*` / `${body.*}`)
/// 조립에 쓴다.
#[derive(Clone, Debug)]
pub struct FiredHook {
    pub hook_id: HookId,
    pub binding: HookBinding,
    pub event: HookEvent,
    pub received: HookEvent,
}

#[derive(Clone, Debug)]
pub struct SurfaceHook {
    pub id: HookId,
    pub surface_id: u32,
    pub event: HookEvent,
    pub binding: HookBinding,
    pub once: bool,
    /// Pre-compiled regex for OutputMatch events (cached at registration time).
    pub compiled_regex: Option<regex::Regex>,
    /// `check_idle_timeouts`가 마지막으로 발사했을 때의 `last_output_at` epoch.
    /// 같은 epoch 동안엔 다시 발사하지 않고(anti-spam), 새 출력이 그 epoch을
    /// 갱신하면 재무장된다. `IdleTimeout` 외 이벤트는 항상 `None`.
    pub idle_fired_epoch: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    ProcessExit,
    /// Output matches a regex pattern (pre-compiled at registration time). Matches only
    /// against *completed* lines (`\n`-terminated) — the pattern is checked against the
    /// same per-surface `LineBuffer` that `output.observe` observers share
    /// (`ObserverRouter::dispatch_text`), so an OutputMatch hook alone (with no
    /// `output.observe` observer registered) is enough to open that buffer's gate.
    OutputMatch(String),
    Bell,
    Notification,
    /// Fire after N seconds of no PTY output. Piggybacks on the host's existing 1Hz
    /// tick (no separate timer/watcher) — each tick compares elapsed time since
    /// `Terminal::last_output_at()` against the threshold. `SurfaceHook::idle_fired_epoch`
    /// gates re-firing to once per idle epoch (anti-spam) until new output re-arms it.
    IdleTimeout(u64),
    /// OSC 133 D phase — a shell-integrated command finished. `None` registered
    /// (e.g. `command-completed`) matches any exit code; `Some(code)` registered
    /// (e.g. `command-completed:1`) matches only that exact code. The *observed*
    /// instance the core fires always carries `Some(actual exit code)`.
    CommandCompleted(Option<i32>),
    /// An arbitrary event identifier owned by a plugin (or any caller). The core
    /// does not know these names — it matches them by exact string equality. This
    /// keeps agent-specific events (e.g. claude's `claude-idle`) out of the core.
    Custom(String),
}

impl HookEvent {
    fn matches(&self, other: &HookEvent, compiled_regex: Option<&regex::Regex>) -> bool {
        match (self, other) {
            (HookEvent::ProcessExit, HookEvent::ProcessExit) => true,
            (HookEvent::Bell, HookEvent::Bell) => true,
            (HookEvent::Notification, HookEvent::Notification) => true,
            (HookEvent::Custom(a), HookEvent::Custom(b)) => a == b,
            (HookEvent::IdleTimeout(threshold), HookEvent::IdleTimeout(elapsed)) => {
                elapsed >= threshold
            }
            (HookEvent::CommandCompleted(want), HookEvent::CommandCompleted(actual)) => {
                match want {
                    None => true,
                    Some(w) => actual.as_ref() == Some(w),
                }
            }
            (HookEvent::OutputMatch(_pattern), HookEvent::OutputMatch(text)) => {
                // Use pre-compiled regex if available, otherwise compile on-the-fly
                if let Some(re) = compiled_regex {
                    re.is_match(text)
                } else {
                    regex::Regex::new(_pattern)
                        .map(|re| re.is_match(text))
                        .unwrap_or(false)
                }
            }
            _ => false,
        }
    }

    /// Parse a hook event from a CLI string like "process-exit", "bell", "output-match:pattern".
    pub fn parse(s: &str) -> Option<Self> {
        if s == "process-exit" {
            Some(HookEvent::ProcessExit)
        } else if s == "bell" {
            Some(HookEvent::Bell)
        } else if s == "notification" {
            Some(HookEvent::Notification)
        } else if let Some(pattern) = s.strip_prefix("output-match:") {
            Some(HookEvent::OutputMatch(pattern.to_string()))
        } else if let Some(secs) = s.strip_prefix("idle-timeout:") {
            secs.parse::<u64>().ok().map(HookEvent::IdleTimeout)
        } else if s == "command-completed" {
            Some(HookEvent::CommandCompleted(None))
        } else if let Some(code) = s.strip_prefix("command-completed:") {
            code.parse::<i32>()
                .ok()
                .map(|c| HookEvent::CommandCompleted(Some(c)))
        } else {
            // Unknown identifiers fall back to a plugin-owned custom event,
            // matched later by exact string equality.
            Some(HookEvent::Custom(s.to_string()))
        }
    }

    /// Serialize to a display string.
    pub fn to_display_string(&self) -> String {
        match self {
            HookEvent::ProcessExit => "process-exit".to_string(),
            HookEvent::Bell => "bell".to_string(),
            HookEvent::Notification => "notification".to_string(),
            HookEvent::OutputMatch(pattern) => format!("output-match:{}", pattern),
            HookEvent::IdleTimeout(secs) => format!("idle-timeout:{}", secs),
            HookEvent::CommandCompleted(None) => "command-completed".to_string(),
            HookEvent::CommandCompleted(Some(code)) => format!("command-completed:{}", code),
            HookEvent::Custom(s) => s.clone(),
        }
    }
}

pub struct HookManager {
    hooks: Vec<SurfaceHook>,
    /// id 카운터. **engine 사이에서 공유한다** — 라우팅이 hook id 를 창을 건너 풀기
    /// 때문이다(`request_target::Kind::Hook`). 카운터가 engine 마다면 두 창이 같은 id 를
    /// 발급하고, 창을 건너 찾는 쪽은 **먼저 찾힌 engine 이 항상 이겨서** 나머지 하나는
    /// 어떤 요청으로도 닿지 않는다. pty · observer 가 같은 이유로 공유 카운터다.
    next_id: Arc<AtomicU64>,
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HookManager {
    /// 자기 카운터로 만든다 — 단독 engine(테스트)용이다.
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// engine 들이 공유하는 카운터로 만든다 — production 경로는 이쪽이다.
    pub fn with_counter(next_id: Arc<AtomicU64>) -> Self {
        Self {
            hooks: Vec::new(),
            next_id,
        }
    }

    pub fn add_hook(
        &mut self,
        surface_id: u32,
        event: HookEvent,
        binding: HookBinding,
        once: bool,
    ) -> HookId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // Pre-compile regex for OutputMatch events
        let compiled_regex = if let HookEvent::OutputMatch(ref pattern) = event {
            regex::Regex::new(pattern).ok()
        } else {
            None
        };
        self.hooks.push(SurfaceHook {
            id,
            surface_id,
            event,
            binding,
            once,
            compiled_regex,
            idle_fired_epoch: None,
        });
        id
    }

    pub fn remove_hook(&mut self, hook_id: HookId) -> bool {
        let len_before = self.hooks.len();
        self.hooks.retain(|h| h.id != hook_id);
        self.hooks.len() < len_before
    }

    /// Retire every binding owned by a closed surface, including persistent hooks.
    /// Bindings already returned by `check_and_fire` remain available to execute.
    pub fn remove_surface_hooks(&mut self, surface_id: u32) {
        self.hooks.retain(|h| h.surface_id != surface_id);
    }

    pub fn list_hooks(&self, surface_id: Option<u32>) -> Vec<&SurfaceHook> {
        self.hooks
            .iter()
            .filter(|h| surface_id.is_none_or(|id| h.surface_id == id))
            .collect()
    }

    /// Check events and return matching hooks' bindings, retiring once-hooks.
    ///
    /// **실행하지 않는다.** 이 크레이트는 leaf 라 공유 훅 핸들러 레지스트리를 볼 수
    /// 없다 — (surface, event) 매칭만 하고 각 발사 훅의 [`HookBinding`] 을 돌려준다.
    /// 실제 실행(레지스트리 조회 + `source` 게이트 + ShellCommand/IpcSequence 분기)은
    /// 본체 `hook_handler::trigger::execute_binding` 이 담당한다. once 훅은 여기서
    /// 발사 즉시 제거된다(옛 동작 보존).
    ///
    /// **once 훅은 한 호출 안에서도 한 번만 발사한다.** `events` 에 그 훅과 맞는 사건이
    /// 여럿 있어도 첫 사건에서 멈춘다 — 제거가 훑기가 끝난 뒤에 일어나므로, 멈추지 않으면
    /// 같은 once 훅이 사건 수만큼 돌려진다. 지속 훅은 맞는 사건마다 발사한다. 근거는
    /// `docs/adr/0567-a-scanned-id-is-consumed-once-by-structure-or-by-a-replay-test.md`.
    pub fn check_and_fire(&mut self, surface_id: u32, events: &[HookEvent]) -> Vec<FiredHook> {
        let mut fired = Vec::new();

        for hook in &self.hooks {
            if hook.surface_id != surface_id {
                continue;
            }
            for event in events {
                if hook.event.matches(event, hook.compiled_regex.as_ref()) {
                    fired.push(FiredHook {
                        hook_id: hook.id,
                        binding: hook.binding.clone(),
                        event: hook.event.clone(),
                        received: event.clone(),
                    });
                    if hook.once {
                        break;
                    }
                }
            }
        }

        // Remove once-hooks that fired
        let fired_set: HashSet<HookId> = fired.iter().map(|f| f.hook_id).collect();
        self.hooks.retain(|h| !h.once || !fired_set.contains(&h.id));

        fired
    }

    /// 이 surface 에 `OutputMatch` 훅이 하나라도 등록돼 있는가 — PTY `OutputAppended`
    /// emit 게이트(`CoreState::sync_output_event_gates`/`process_surface`)가
    /// 참조한다. observer 가 없어도 이 훅이 있으면 라인 버퍼링을 켜야 한다.
    pub fn has_output_match_hook(&self, surface_id: u32) -> bool {
        self.hooks
            .iter()
            .any(|h| h.surface_id == surface_id && matches!(h.event, HookEvent::OutputMatch(_)))
    }

    /// 이 surface 에 `IdleTimeout` 훅이 하나라도 등록돼 있는가 — idle 폴링
    /// (`CoreState::poll_idle_timeout_hooks`)이 어느 surface 를 검사할지 고를 때
    /// 쓴다.
    pub fn has_idle_timeout_hook(&self, surface_id: u32) -> bool {
        self.hooks
            .iter()
            .any(|h| h.surface_id == surface_id && matches!(h.event, HookEvent::IdleTimeout(_)))
    }

    /// 한 surface 의 idle 경과시간을 등록된 `IdleTimeout` 훅과 비교해 발사한다.
    ///
    /// anti-spam: `idle_fired_epoch` 에 발사 당시의 `last_output_at`(epoch)을
    /// 기록해, 같은 epoch(=그 사이 새 출력이 없었음) 동안엔 다시 발사하지
    /// 않는다. `last_output_at` 이 새 출력으로 갱신되면 epoch 이 달라져
    /// 재무장된다(`GlobalHookManager::tick()` 의 `File` 조건 anti-spam 과 동형).
    pub fn check_idle_timeouts(
        &mut self,
        surface_id: u32,
        elapsed_secs: u64,
        last_output_epoch: Instant,
    ) -> Vec<FiredHook> {
        let observed = HookEvent::IdleTimeout(elapsed_secs);
        let mut fired = Vec::new();

        for hook in &mut self.hooks {
            if hook.surface_id != surface_id {
                continue;
            }
            if !matches!(hook.event, HookEvent::IdleTimeout(_)) {
                continue;
            }
            if hook.idle_fired_epoch == Some(last_output_epoch) {
                continue; // 이 epoch 에서 이미 발사됨 — 스팸 방지.
            }
            if hook.event.matches(&observed, None) {
                hook.idle_fired_epoch = Some(last_output_epoch);
                fired.push(FiredHook {
                    hook_id: hook.id,
                    binding: hook.binding.clone(),
                    event: hook.event.clone(),
                    received: observed.clone(),
                });
            }
        }

        let fired_set: HashSet<HookId> = fired.iter().map(|f| f.hook_id).collect();
        self.hooks.retain(|h| !h.once || !fired_set.contains(&h.id));

        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_parse_process_exit() {
        assert_eq!(
            HookEvent::parse("process-exit"),
            Some(HookEvent::ProcessExit)
        );
    }

    #[test]
    fn hook_event_parse_bell() {
        assert_eq!(HookEvent::parse("bell"), Some(HookEvent::Bell));
    }

    #[test]
    fn hook_event_parse_notification() {
        assert_eq!(
            HookEvent::parse("notification"),
            Some(HookEvent::Notification)
        );
    }

    #[test]
    fn hook_event_parse_output_match() {
        match HookEvent::parse("output-match:error.*") {
            Some(HookEvent::OutputMatch(p)) => assert_eq!(p, "error.*"),
            _ => panic!("expected OutputMatch"),
        }
    }

    #[test]
    fn hook_event_parse_idle_timeout() {
        match HookEvent::parse("idle-timeout:30") {
            Some(HookEvent::IdleTimeout(30)) => {}
            _ => panic!("expected IdleTimeout(30)"),
        }
    }

    #[test]
    fn parse_unknown_event_becomes_custom() {
        assert_eq!(
            HookEvent::parse("claude-idle"),
            Some(HookEvent::Custom("claude-idle".to_string()))
        );
        assert_eq!(
            HookEvent::parse("anything-a-plugin-invents"),
            Some(HookEvent::Custom("anything-a-plugin-invents".to_string()))
        );
    }

    #[test]
    fn custom_events_match_by_exact_string() {
        let registered = HookEvent::Custom("claude-idle".into());
        let fired_same = HookEvent::Custom("claude-idle".into());
        let fired_other = HookEvent::Custom("claude-child-idle".into());
        assert!(registered.matches(&fired_same, None));
        assert!(!registered.matches(&fired_other, None));
    }

    #[test]
    fn custom_roundtrip_display_parse() {
        let ev = HookEvent::Custom("claude-error".into());
        assert_eq!(ev.to_display_string(), "claude-error");
        assert_eq!(HookEvent::parse(&ev.to_display_string()), Some(ev));
    }

    #[test]
    fn hook_event_display_roundtrip() {
        let events = vec![
            HookEvent::ProcessExit,
            HookEvent::Bell,
            HookEvent::Notification,
            HookEvent::OutputMatch("pattern".into()),
            HookEvent::IdleTimeout(60),
        ];
        for ev in &events {
            let s = ev.to_display_string();
            let parsed = HookEvent::parse(&s);
            assert!(parsed.is_some(), "failed to roundtrip: {}", s);
        }
    }

    #[test]
    fn hook_event_parse_command_completed_any() {
        assert_eq!(
            HookEvent::parse("command-completed"),
            Some(HookEvent::CommandCompleted(None))
        );
    }

    #[test]
    fn hook_event_parse_command_completed_specific_code() {
        assert_eq!(
            HookEvent::parse("command-completed:1"),
            Some(HookEvent::CommandCompleted(Some(1)))
        );
        assert_eq!(
            HookEvent::parse("command-completed:0"),
            Some(HookEvent::CommandCompleted(Some(0)))
        );
    }

    #[test]
    fn hook_event_parse_command_completed_bad_code_is_none() {
        assert_eq!(HookEvent::parse("command-completed:not-a-number"), None);
    }

    #[test]
    fn command_completed_matches_any_exit_code_when_unspecified() {
        let registered = HookEvent::CommandCompleted(None);
        assert!(registered.matches(&HookEvent::CommandCompleted(Some(0)), None));
        assert!(registered.matches(&HookEvent::CommandCompleted(Some(1)), None));
        assert!(registered.matches(&HookEvent::CommandCompleted(Some(127)), None));
    }

    #[test]
    fn command_completed_matches_only_specific_exit_code_when_filtered() {
        let registered = HookEvent::CommandCompleted(Some(1));
        assert!(registered.matches(&HookEvent::CommandCompleted(Some(1)), None));
        assert!(!registered.matches(&HookEvent::CommandCompleted(Some(0)), None));
        assert!(!registered.matches(&HookEvent::CommandCompleted(Some(2)), None));
    }

    #[test]
    fn command_completed_display_roundtrip() {
        let any = HookEvent::CommandCompleted(None);
        assert_eq!(any.to_display_string(), "command-completed");
        assert_eq!(HookEvent::parse(&any.to_display_string()), Some(any));

        let specific = HookEvent::CommandCompleted(Some(127));
        assert_eq!(specific.to_display_string(), "command-completed:127");
        assert_eq!(
            HookEvent::parse(&specific.to_display_string()),
            Some(specific)
        );
    }

    #[test]
    fn hook_manager_check_and_fire_matches_command_completed_any() {
        let mut manager = HookManager::new();
        manager.add_hook(
            1,
            HookEvent::CommandCompleted(None),
            shell("echo done"),
            false,
        );
        let fired = manager.check_and_fire(1, &[HookEvent::CommandCompleted(Some(0))]);
        assert_eq!(fired.len(), 1);
        // 등록은 와일드카드(None)지만, received 는 실제 관측값(exit code 0)이어야
        // 한다 — exit code 소실 결함의 회귀 방지.
        assert_eq!(fired[0].event, HookEvent::CommandCompleted(None));
        assert_eq!(fired[0].received, HookEvent::CommandCompleted(Some(0)));
    }

    #[test]
    fn hook_manager_check_and_fire_filters_command_completed_by_code() {
        let mut manager = HookManager::new();
        manager.add_hook(
            1,
            HookEvent::CommandCompleted(Some(1)),
            shell("echo failed"),
            false,
        );
        assert!(
            manager
                .check_and_fire(1, &[HookEvent::CommandCompleted(Some(0))])
                .is_empty()
        );
        assert_eq!(
            manager
                .check_and_fire(1, &[HookEvent::CommandCompleted(Some(1))])
                .len(),
            1
        );
    }

    #[test]
    fn hook_event_matches_same_type() {
        assert!(HookEvent::ProcessExit.matches(&HookEvent::ProcessExit, None));
        assert!(HookEvent::Bell.matches(&HookEvent::Bell, None));
        assert!(HookEvent::Notification.matches(&HookEvent::Notification, None));
    }

    #[test]
    fn hook_event_matches_different_type() {
        assert!(!HookEvent::ProcessExit.matches(&HookEvent::Bell, None));
        assert!(!HookEvent::Bell.matches(&HookEvent::Notification, None));
    }

    #[test]
    fn hook_event_output_match_regex() {
        let pattern = HookEvent::OutputMatch("error.*".into());
        let text = HookEvent::OutputMatch("error: something went wrong".into());
        assert!(pattern.matches(&text, None));
    }

    #[test]
    fn check_and_fire_received_carries_actual_matched_output_text() {
        let mut manager = HookManager::new();
        manager.add_hook(
            1,
            HookEvent::OutputMatch("error.*".into()),
            shell("echo matched"),
            false,
        );
        let fired = manager.check_and_fire(
            1,
            &[HookEvent::OutputMatch("error: something went wrong".into())],
        );
        assert_eq!(fired.len(), 1);
        // event 는 등록 패턴, received 는 실제 매칭된 라인 — 이전엔 둘 다 등록
        // 패턴만 전달돼 실제 매칭 텍스트가 소실됐다(회귀 방지).
        assert_eq!(fired[0].event, HookEvent::OutputMatch("error.*".into()));
        assert_eq!(
            fired[0].received,
            HookEvent::OutputMatch("error: something went wrong".into())
        );
    }

    fn shell(cmd: &str) -> HookBinding {
        HookBinding::InlineShell(cmd.into())
    }

    #[test]
    fn hook_manager_add_and_list() {
        let mut manager = HookManager::new();
        let id = manager.add_hook(1, HookEvent::Bell, shell("echo bell"), false);
        let hooks = manager.list_hooks(Some(1));
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id, id);
    }

    #[test]
    fn hook_manager_remove() {
        let mut manager = HookManager::new();
        let id = manager.add_hook(1, HookEvent::Bell, shell("echo bell"), false);
        assert!(manager.remove_hook(id));
        assert_eq!(manager.list_hooks(None).len(), 0);
    }

    #[test]
    fn hook_manager_remove_nonexistent() {
        let mut manager = HookManager::new();
        assert!(!manager.remove_hook(999));
    }

    #[test]
    fn closed_surface_hooks_are_retired_without_losing_fired_bindings() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::ProcessExit, shell("exit notice"), true);
        manager.add_hook(1, HookEvent::Bell, shell("persistent"), false);
        manager.add_hook(2, HookEvent::Bell, shell("other surface"), false);
        let fired = manager.check_and_fire(1, &[HookEvent::ProcessExit]);
        manager.remove_surface_hooks(1);
        assert!(manager.list_hooks(Some(1)).is_empty());
        assert_eq!(manager.list_hooks(Some(2)).len(), 1);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].binding.to_display_string(), "exit notice");
    }

    #[test]
    fn hook_manager_once_hook_removed_after_fire() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::Bell, shell("echo once"), true);
        let fired = manager.check_and_fire(1, &[HookEvent::Bell]);
        assert_eq!(fired.len(), 1);
        // Hook should be removed after firing
        assert_eq!(manager.list_hooks(None).len(), 0);
    }

    /// once 훅은 한 호출에 맞는 사건이 여럿 와도 **한 번만** 돌려진다 — 제거는 훑기가
    /// 끝난 뒤라, 사건마다 돌려주면 같은 once 훅이 사건 수만큼 실행된다. 지속 훅은 맞는
    /// 사건마다 돌려진다(once 의 멈춤이 지속 훅까지 번지지 않는다). 다음 호출에서는
    /// once 훅이 이미 없다.
    #[test]
    fn a_once_hook_fires_once_even_when_several_events_match() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::Bell, shell("echo once"), true);
        manager.add_hook(1, HookEvent::Bell, shell("echo each"), false);
        let fired = manager.check_and_fire(1, &[HookEvent::Bell, HookEvent::Bell]);
        let count = |cmd: &str| {
            fired
                .iter()
                .filter(|f| f.binding == HookBinding::InlineShell(cmd.into()))
                .count()
        };
        assert_eq!(
            count("echo once"),
            1,
            "once 훅이 한 호출에서 두 번 돌려졌다"
        );
        assert_eq!(
            count("echo each"),
            2,
            "지속 훅이 맞는 사건마다 돌려지지 않았다"
        );
        let again = manager.check_and_fire(1, &[HookEvent::Bell]);
        assert!(
            again
                .iter()
                .all(|f| f.binding != HookBinding::InlineShell("echo once".into())),
            "발사한 once 훅이 다음 호출에서 또 돌려졌다"
        );
    }

    #[test]
    fn hook_manager_persistent_hook_stays() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::Bell, shell("echo persistent"), false);
        let fired = manager.check_and_fire(1, &[HookEvent::Bell]);
        assert_eq!(fired.len(), 1);
        // Hook should still be there
        assert_eq!(manager.list_hooks(None).len(), 1);
    }

    #[test]
    fn check_and_fire_returns_binding() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::Bell, shell("echo hi"), false);
        manager.add_hook(
            1,
            HookEvent::Bell,
            HookBinding::Handler("host/webhook-notify".into()),
            false,
        );
        let fired = manager.check_and_fire(1, &[HookEvent::Bell]);
        assert_eq!(fired.len(), 2);
        assert_eq!(fired[0].binding, HookBinding::InlineShell("echo hi".into()));
        assert_eq!(fired[0].event, HookEvent::Bell);
        assert_eq!(
            fired[1].binding,
            HookBinding::Handler("host/webhook-notify".into())
        );
    }

    #[test]
    fn check_and_fire_only_matches_surface() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::Bell, shell("s1"), false);
        manager.add_hook(2, HookEvent::Bell, shell("s2"), false);
        let fired = manager.check_and_fire(1, &[HookEvent::Bell]);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].binding, HookBinding::InlineShell("s1".into()));
    }

    #[test]
    fn binding_display_string() {
        assert_eq!(shell("echo x").to_display_string(), "echo x");
        assert_eq!(
            HookBinding::Handler("user/foo".into()).to_display_string(),
            "handler:user/foo"
        );
    }

    #[test]
    fn idle_timeout_matches_when_elapsed_exceeds_threshold() {
        let registered = HookEvent::IdleTimeout(30);
        let observed_short = HookEvent::IdleTimeout(10);
        let observed_long = HookEvent::IdleTimeout(31);
        assert!(!registered.matches(&observed_short, None));
        assert!(registered.matches(&observed_long, None));
    }

    #[test]
    fn has_output_match_and_idle_timeout_hook_are_per_surface() {
        let mut manager = HookManager::new();
        assert!(!manager.has_output_match_hook(1));
        assert!(!manager.has_idle_timeout_hook(1));
        manager.add_hook(
            1,
            HookEvent::OutputMatch("ERROR".into()),
            shell("echo matched"),
            false,
        );
        manager.add_hook(1, HookEvent::IdleTimeout(30), shell("echo idle"), false);
        assert!(manager.has_output_match_hook(1));
        assert!(manager.has_idle_timeout_hook(1));
        // 다른 surface 는 영향받지 않는다.
        assert!(!manager.has_output_match_hook(2));
        assert!(!manager.has_idle_timeout_hook(2));
    }

    #[test]
    fn check_idle_timeouts_fires_once_per_epoch_then_rearms_on_new_output() {
        let mut manager = HookManager::new();
        manager.add_hook(1, HookEvent::IdleTimeout(30), shell("echo idle"), false);
        let epoch_a = Instant::now();

        // 임계값 미만 — 발사 없음.
        assert!(manager.check_idle_timeouts(1, 10, epoch_a).is_empty());
        // 임계값 초과 — 발사. received 는 등록 임계값(30)이 아니라 실제 경과초(31).
        let fired = manager.check_idle_timeouts(1, 31, epoch_a);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].event, HookEvent::IdleTimeout(30));
        assert_eq!(fired[0].received, HookEvent::IdleTimeout(31));
        // 같은 epoch(=그 사이 새 출력 없음) 이면 다시 발사하지 않는다(anti-spam).
        assert!(manager.check_idle_timeouts(1, 32, epoch_a).is_empty());

        // 새 출력으로 epoch 이 바뀌면 재무장된다.
        let epoch_b = epoch_a + std::time::Duration::from_secs(5);
        assert!(manager.check_idle_timeouts(1, 10, epoch_b).is_empty());
        assert_eq!(manager.check_idle_timeouts(1, 31, epoch_b).len(), 1);
    }

    #[test]
    fn check_idle_timeouts_removes_once_hook_after_fire() {
        let mut manager = HookManager::new();
        let id = manager.add_hook(1, HookEvent::IdleTimeout(5), shell("echo once"), true);
        let epoch = Instant::now();
        let fired = manager.check_idle_timeouts(1, 6, epoch);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].hook_id, id);
        assert!(manager.list_hooks(None).is_empty());
    }
}
