//! 1.0 이벤트 카탈로그의 페이로드 Rust 타입.
//!
//! 호스트는 이 타입을 만들어 `serde_json::to_value`로 직렬화한 뒤
//! [`super::EventEnvelope`]의 `payload`에 싣는다. Plugin은 envelope에서
//! payload를 꺼내 자기가 관심 있는 타입으로 `from_value`해 사용한다.
//!
//! 이벤트 키, 발생 시점, 범위와 안정성 등급은 docs/reference/event-catalog.md에 있다.

use serde::{Deserialize, Serialize};

use super::LifecycleReason;

// ── Surface ──────────────────────────────────────────────────────────────────

/// `surface.created` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceCreated {
    pub surface_id: u32,
    pub kind: String,
    pub tab_id: u32,
    pub pane_id: u32,
    pub workspace_id: u32,
    pub created_by: SurfaceCreatedBy,
}

/// `surface.created`의 `created_by` 필드 — agent spawn과 user split을 구분.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceCreatedBy {
    /// 사용자가 UI로 split/탭 추가한 결과.
    User,
    /// plugin이 IPC로 spawn한 결과. `source_plugin`이 spawn 주체.
    Agent { source_plugin: String },
}

/// `surface.closed` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceClosed {
    pub surface_id: u32,
    pub kind: String,
    pub reason: LifecycleReason,
}

/// `surface.focused` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceFocused {
    pub surface_id: u32,
    pub prev_surface_id: Option<u32>,
}

/// `surface.resized` 페이로드. scope=Surface.
/// 호스트가 150ms leading+trailing 쓰로틀 적용 후 발화.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceResized {
    pub surface_id: u32,
    pub width_px: u32,
    pub height_px: u32,
}

/// `surface.title_changed` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceTitleChanged {
    pub surface_id: u32,
    pub title: String,
}

/// `surface.refresh_requested` 페이로드. scope=Surface.
/// 사용자가 이 surface 를 대상으로 공통 새로고침을 실행했다. 발행은 요청 접수일 뿐 갱신 성공이 아니며,
/// 구독한 plugin 모두에게 가므로 수신자는 `surface_id` 가 자기 문서인지 확인한다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceRefreshRequested {
    pub surface_id: u32,
    /// 키를 누른 시점의 surface kind. 전달 직전에도 같은 kind 임을 host 가 확인했다.
    pub kind: String,
    /// 사용자 입력 경로. `shortcut` 또는 `palette`. 에이전트·IPC 는 이 이벤트를 만들지 않는다.
    pub trigger: String,
}

// ── Tab ──────────────────────────────────────────────────────────────────────

/// `tab.created` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TabCreated {
    pub tab_id: u32,
    pub pane_id: u32,
    pub workspace_id: u32,
    pub kind: String,
}

/// `tab.closed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TabClosed {
    pub tab_id: u32,
    pub pane_id: u32,
    pub reason: LifecycleReason,
}

/// `tab.focused` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TabFocused {
    pub tab_id: u32,
    pub pane_id: u32,
    pub prev_tab_id: Option<u32>,
}

/// `tab.moved` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TabMoved {
    pub tab_id: u32,
    pub from_pane: u32,
    pub to_pane: u32,
}

/// `tab.renamed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TabRenamed {
    pub tab_id: u32,
    pub title: String,
}

// ── Pane ─────────────────────────────────────────────────────────────────────

/// `pane.created` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaneCreated {
    pub pane_id: u32,
    pub parent_pane_group: Option<u32>,
    pub workspace_id: u32,
}

/// `pane.closed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaneClosed {
    pub pane_id: u32,
    pub reason: LifecycleReason,
}

/// `pane.split` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaneSplit {
    pub original_pane: u32,
    pub new_pane: u32,
    pub direction: SplitDirection,
}

/// pane 분할 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

// ── Split (PaneGroup / SurfaceGroup) ─────────────────────────────────────────

/// `split.ratio_changed` 페이로드. scope=System.
/// 호스트가 150ms 쓰로틀 적용 후 발화. 드래그 시작·종료에는 무조건 1회씩.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SplitRatioChanged {
    pub group_id: u32,
    pub level: SplitLevel,
    pub ratio: f32,
}

/// split이 적용된 레이어 — pane-level 또는 surface-level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitLevel {
    Pane,
    Surface,
}

// ── Workspace ────────────────────────────────────────────────────────────────

/// `workspace.created` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceCreated {
    pub workspace_id: u32,
    /// 발행 시점에 workspace가 속한 창의 id. 창이 없는 engine(보관 중인 engine 등)에서 만들었거나
    /// 발행 전에 창에서 사라졌으면 0이며, 0은 창 없음을 뜻한다.
    pub window_id: u64,
    pub name: String,
}

/// `workspace.closed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceClosed {
    pub workspace_id: u32,
    pub reason: LifecycleReason,
}

/// `workspace.activated` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceActivated {
    pub workspace_id: u32,
    pub prev_workspace_id: Option<u32>,
}

/// `workspace.renamed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceRenamed {
    pub workspace_id: u32,
    pub name: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
}

// ── Window (OS) ──────────────────────────────────────────────────────────────

/// `window.created` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindowCreated {
    pub window_id: u64,
    pub kind: String,
    pub modality: WindowModality,
}

/// 윈도우 modality — 유비쿼터스 언어(`docs/concepts/ubiquitous-language.md`)와 일치.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowModality {
    Modeless,
    Modal,
}

/// `window.closed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindowClosed {
    pub window_id: u64,
    pub reason: LifecycleReason,
}

/// `window.focused` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindowFocused {
    pub window_id: u64,
}

// ── Plugin lifecycle ─────────────────────────────────────────────────────────

/// `plugin.loaded` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginLoaded {
    pub plugin_id: String,
    pub version: String,
}

/// `plugin.unloaded` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginUnloaded {
    pub plugin_id: String,
    pub reason: LifecycleReason,
}

/// `plugin.error` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginError {
    pub plugin_id: String,
    /// 자유 문자열 카테고리. 예: "spawn_failed", "handshake_rejected", "panicked".
    pub error_kind: String,
    pub message: String,
}

/// `plugin.enabled` / `plugin.disabled` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginEnableToggled {
    pub plugin_id: String,
}

// ── Extension lifecycle ──────────────────────────────────────────────────────

/// `extension.activated` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionActivated {
    pub extension_id: String,
    pub target_id: String,
}

/// `extension.pending` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionPending {
    pub extension_id: String,
    pub target_id: String,
    /// 자유 문자열. 예: "target_not_loaded", "version_incompatible".
    pub reason: String,
}

/// `extension.conflict` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtensionConflict {
    pub extension_id: String,
    pub target_id: String,
    pub conflicting_id: String,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

/// `tool.invoked` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolInvoked {
    pub tool_id: String,
    pub source: ToolSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Plugin { plugin_id: String },
}

// Command

/// `command.invoked` 페이로드 — owner plugin에 unicast로 전달.
/// trigger=shortcut & scope=Surface인 경우 envelope scope=Surface, 그 외 System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandInvoked {
    pub plugin_id: String,
    pub command_id: String,
    pub scope: CommandScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_surface_id: Option<u32>,
    pub trigger: CommandTrigger,
}

/// command scope — 매니페스트의 `[[contributes.command]] scope`에 대응.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandScope {
    /// 어디서나 동작. 단축키는 조합키만 허용.
    Global,
    /// owner plugin이 만든 surface에 포커스가 있을 때만 동작. 단일 키 허용.
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandTrigger {
    Shortcut,
    Menu,
    Ipc,
}

/// `command.shortcut_changed` 페이로드 — 사용자가 설정창에서 매핑 변경 시 broadcast.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandShortcutChanged {
    pub plugin_id: String,
    pub command_id: String,
    /// 새 단축키. `None`이면 매핑 해제.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// 변경 전 단축키. `None`이면 새 매핑 추가.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_shortcut: Option<String>,
}

// ── IME ──────────────────────────────────────────────────────────────────────

/// `ime.composition_start` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImeCompositionStart {
    pub surface_id: u32,
}

/// `ime.composition_end` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImeCompositionEnd {
    pub surface_id: u32,
    pub committed_text: String,
}

// ── Theme / Language ─────────────────────────────────────────────────────────

/// `theme.changed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeChanged {
    pub theme_id: String,
}

/// `language.changed` 페이로드. scope=System. UI 언어 변경.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LanguageChanged {
    /// BCP 47 언어 코드 또는 토큰 (예: "en", "ko", "ja", "zh-Hans").
    pub language_code: String,
}

// ── Notification / Hook ──────────────────────────────────────────────────────

/// `notification.created` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationCreated {
    pub id: String,
    /// error, warning, or info. Older hosts omit this and mean info.
    #[serde(default = "default_notification_level")]
    pub level: String,
    pub title: String,
    pub body: String,
    /// "host" 또는 plugin_id.
    pub source: String,
}

fn default_notification_level() -> String {
    "info".to_owned()
}

/// `notification.dismissed` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationDismissed {
    pub id: String,
}

/// `hook.fired` 페이로드 — `tasty-hooks` 시스템의 fire 결과.
/// surface hook이면 scope=Surface(surface_id 있음), global hook이면 scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookFired {
    pub hook_id: String,
    /// `process-exit`, `bell`, `output-match:<pattern>`, `idle-timeout:<secs>`,
    /// `interval:<secs>`, `once:<secs>`, `file:<path>` 등.
    pub event_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<u32>,
    pub payload: serde_json::Value,
}

// ── Process (PTY) ────────────────────────────────────────────────────────────

/// `process.started` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessStarted {
    pub surface_id: u32,
    pub pid: u32,
    pub command: String,
}

/// `process.exited` 페이로드. scope=Surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessExited {
    pub surface_id: u32,
    /// OS exit code. 신호 종료 시 일부 플랫폼에서 음수 또는 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

// ── Memory ───────────────────────────────────────────────────────────────────

/// `memory.changed` 페이로드. scope=System.
///
/// regular 메모리의 put/delete/expire/scope cleanup 변경을 알린다.
/// secret 변경은 다른 플러그인에 소유자와 키를 노출하지 않도록 알리지 않는다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryChanged {
    /// `surface:42`, `workspace:1`, `global` 등 scope token.
    pub scope: String,
    pub key: String,
    pub kind: MemoryChangeKind,
    /// 새 version (Created/Updated). Deleted/Expired 는 생략.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
}

/// `memory.changed` 의 변경 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChangeKind {
    Created,
    Updated,
    Deleted,
    Expired,
}

// ── Agent ────────────────────────────────────────────────────────────────────

/// agent.task_finished 페이로드. 종료 상태로 바뀔 때만 보내며 scope는 System이다.
/// 종결 사실과 레코드를 다시 읽을 좌표(task_id·attempt_id·revision)만 싣는다. 실패 사유와
/// 결과는 레코드가 정본이므로 이 이벤트에 넣지 않고, 필요한 호출자는 agent.task_get으로 읽는다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentTaskFinished {
    /// 작업이 속한 workspace. 이벤트 scope는 system이다.
    pub workspace_id: u32,
    pub task_id: String,
    /// `succeeded` · `failed` · `cancelled` · `skipped` 넷 중 하나.
    pub state: String,
    /// v2 task 의 실행 회차 id(`agent.task_get` 의 `attempt.id`). 회차가 없는 task 는 생략한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    /// 종결을 기록한 뒤 읽은 레코드 revision(`agent.task_get` 의 `revision`). 다시 읽은 레코드의
    /// revision 이 이 값보다 작지 않으면 이 사건 이후의 레코드다. 읽지 못했으면 생략한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    /// Durable terminal notification identity. A replay after restart retains this id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    /// 건너뛴 이유. 이유가 기록된 v2 task 만 싣는다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<AgentTaskSkip>,
}

/// `agent.task_finished` 의 건너뛴 이유. `agent.task_get` 의 `skip` 과 같은 모양이다.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentTaskSkip {
    /// `branch_not_selected`(경로가 선택되지 않음) 또는 `upstream_unavailable`(선행 결과를 쓸 수 없음).
    pub reason: String,
    /// `upstream_unavailable` 의 원본 task id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `upstream_unavailable` 의 원본 상태.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_state: Option<String>,
}

/// 요구한 신호 수를 채워 barrier가 닫혔을 때 보내는 이벤트.
/// 시간 초과는 조회 시점에 판단하므로 이 이벤트에 포함하지 않는다.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentBarrierClosed {
    pub workspace_id: u32,
    /// barrier 이름. 만든 쪽이 정하는 식별자다.
    pub name: String,
    /// 닫힐 때 요구된 신호 수.
    pub count_required: u32,
}

// ── System ───────────────────────────────────────────────────────────────────

/// `system.startup_complete` 페이로드. scope=System.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SystemStartupComplete {}

/// `system.shutdown_initiated` 페이로드. scope=System.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemShutdownInitiated {
    /// 자유 문자열. 예: "user_quit", "os_shutdown", "panic".
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_level_defaults_for_older_hosts_and_round_trips() {
        let mut payload =
            serde_json::json!({"id":"7","title":"Notice","body":"body","source":"host"});
        let legacy: NotificationCreated = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(legacy.level, "info");
        payload["level"] = serde_json::json!("error");
        let error: NotificationCreated = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(serde_json::to_value(error).unwrap(), payload);
    }

    #[test]
    fn surface_created_round_trip() {
        let p = SurfaceCreated {
            surface_id: 1,
            kind: "terminal".into(),
            tab_id: 2,
            pane_id: 3,
            workspace_id: 4,
            created_by: SurfaceCreatedBy::Agent {
                source_plugin: "com.tasty.claude".into(),
            },
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["surface_id"], 1);
        assert_eq!(v["created_by"]["kind"], "agent");
        assert_eq!(v["created_by"]["source_plugin"], "com.tasty.claude");
        let back: SurfaceCreated = serde_json::from_value(v).unwrap();
        assert_eq!(back.surface_id, 1);
    }

    #[test]
    fn surface_closed_uses_lifecycle_reason() {
        let p = SurfaceClosed {
            surface_id: 9,
            kind: "terminal".into(),
            reason: LifecycleReason::Ipc,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["reason"], "ipc");
    }

    #[test]
    fn command_invoked_optional_surface_id_skipped_when_none() {
        let p = CommandInvoked {
            plugin_id: "com.example.x".into(),
            command_id: "open_popup".into(),
            scope: CommandScope::Global,
            source_surface_id: None,
            trigger: CommandTrigger::Shortcut,
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(!s.contains("source_surface_id"));
        assert!(s.contains("\"trigger\":\"shortcut\""));
        assert!(s.contains("\"scope\":\"global\""));
    }

    #[test]
    fn process_exited_optional_exit_code() {
        let p = ProcessExited {
            surface_id: 1,
            exit_code: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(!s.contains("exit_code"));
    }

    /// 구독 이벤트에 실패 사유나 명령 출력이 포함되지 않는지 확인한다.
    #[test]
    fn a_finished_task_carries_its_verdict_but_not_what_it_printed() {
        let p = AgentTaskFinished {
            workspace_id: 3,
            task_id: "t-1716800000123-7".into(),
            state: "failed".into(),
            attempt_id: Some("t-1716800000123-7#2".into()),
            revision: Some(9),
            terminal_id: Some("terminal-identity".into()),
            skip: None,
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("\"state\":\"failed\""), "{s}");
        assert!(s.contains("\"attempt_id\":\"t-1716800000123-7#2\""), "{s}");
        assert!(s.contains("\"revision\":9"), "{s}");
        assert!(!s.contains("skip"), "{s}");
        assert!(!s.contains("error"), "실패 사유가 실렸다: {s}");
        assert!(!s.contains("result"), "결과가 실렸다: {s}");
        assert!(!s.contains("output"), "출력이 실렸다: {s}");
    }

    /// 이전 버전 호스트가 보낸 세 필드 페이로드도 읽힌다.
    #[test]
    fn a_finished_task_without_attempt_revision_or_skip_still_parses() {
        let p: AgentTaskFinished = serde_json::from_value(serde_json::json!({
            "workspace_id": 1, "task_id": "t", "state": "skipped"
        }))
        .unwrap();
        assert_eq!(p.attempt_id, None);
        assert_eq!(p.revision, None);
        assert_eq!(p.skip, None);
        let p: AgentTaskFinished = serde_json::from_value(serde_json::json!({
            "workspace_id": 1, "task_id": "t", "state": "skipped",
            "skip": {"reason": "upstream_unavailable", "source": "a", "source_state": "failed"}
        }))
        .unwrap();
        assert_eq!(p.skip.and_then(|s| s.source).as_deref(), Some("a"));
    }

    #[test]
    fn a_closed_barrier_names_itself_and_the_count_it_needed() {
        let p = AgentBarrierClosed {
            workspace_id: 1,
            name: "wave-1".into(),
            count_required: 4,
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back["name"], "wave-1");
        assert_eq!(back["count_required"], 4);
        assert_eq!(back["workspace_id"], 1);
    }
}
