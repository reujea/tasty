//! 도메인 실행이 세우고 App 메인 루프가 소비하는 **호스트 이벤트 큐 항목**.
//!
//! 세우는 자리는 도메인 cascade(`core::structural_cascade` 등)와 `AppState` 의 enqueue
//! 메서드이고, 비우는 자리는 App 메인 루프(plugin event bus · Lua hook 발화)다. 세우는
//! 쪽이 도메인이라 정의는 여기 있고, `crate::state` 가 같은 이름을 재수출한다 — 도메인이
//! 창 상태 모듈을 거꾸로 부르지 않게 하려는 것이다.
//!
//! 두 타입 모두 wire 타입이 아니라 plain 데이터다. `tasty_plugin_protocol` 타입으로
//! 바꾸는 일은 메인 루프가 한다.

/// Surface가 닫혔다는 사실을 plugin 측에 broadcast하기 위해 메인 루프가 소비할
/// 큐 항목. `state/`는 `plugin/` 의존이 없으므로 enum 대신 `is_user_close: bool`로
/// reason을 담고, App 메인 루프에서 `LifecycleReason`으로 매핑한다.
// 이유: headless 빌드에도 이 항목을 세우는 코드가 컴파일된다(`AppState` 의 enqueue 메서드 —
// `state.rs`·`state/pane.rs`·`state/tab.rs`). 비우는 자는 GUI 메인 루프뿐이다. 구조 cascade 의
// enqueue 자리는 gui 로 가려져 있다(`core/structural_cascade.rs` 모듈 문서).
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui main loop drains the host event queue"
    )
)]
#[derive(Debug, Clone)]
pub struct PendingSurfaceClosed {
    pub(crate) surface_id: u32,
    /// kind 가 None 인 경우는 cascade close 경로에서 surface 가 이미 layout 에서
    /// 제거된 뒤 enqueue 되어 식별이 불가능했음을 의미. payload 변환 시 빈 문자열로
    /// 폴백한다 — 구독자(예: plugin-claude)는 surface_id 만으로 cleanup 가능.
    pub(crate) kind: Option<&'static str>,
    pub(crate) is_user_close: bool,
}

/// Event Bus 1.0 호스트 자동 발화용 큐 항목. `state/`가 `plugin/`/`tasty-plugin-protocol`
/// 의존을 갖지 않게, payload 필드는 wire 타입이 아닌 plain 데이터로 보관하고 App
/// 메인 루프가 [`tasty_plugin_protocol`] 타입으로 변환해 발화한다.
// 이유: 위 `PendingSurfaceClosed` 와 같다 — 세우는 코드 일부가 headless 빌드에도 컴파일되지만
// 비우는 자리(`app/dispatch/host_events.rs`)는 GUI 에만 있다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui main loop drains the host event queue"
    )
)]
#[derive(Debug, Clone)]
pub enum PendingHostEvent {
    SurfaceFocused {
        surface_id: u32,
        prev_surface_id: Option<u32>,
    },
    SurfaceTitleChanged {
        surface_id: u32,
        title: String,
    },
    SurfaceCreated {
        surface_id: u32,
        kind: &'static str,
        tab_id: u32,
        pane_id: u32,
        workspace_id: u32,
        /// `None`이면 user-initiated, `Some(plugin_id)`면 agent(plugin)이 spawn한 결과.
        created_by_plugin: Option<String>,
    },
    WorkspaceActivated {
        workspace_id: u32,
        prev_workspace_id: Option<u32>,
    },
    /// 이름/부제/설명 중 변경된 필드만 `Some`. 호스트 발화 측 어디서나 partial
    /// update가 가능하도록 모두 Optional로 둔다.
    ///
    /// `user_direct=true`면 사용자가 GUI 다이얼로그로 직접 변경한 케이스. Lua
    /// hook 의 `workspace.change.post` 는 user_direct 만 발화한다 (observe-only
    /// 단계 명세). IPC/CLI 경유 변경은 false 로 들어와 plugin 이벤트 버스만 받는다.
    WorkspaceRenamed {
        workspace_id: u32,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
        user_direct: bool,
    },
    TabFocused {
        tab_id: u32,
        pane_id: u32,
        prev_tab_id: Option<u32>,
    },
    /// Tab 이름 변경. `user_direct=true`면 사용자가 GUI 다이얼로그로 직접 rename
    /// 한 케이스 — Lua `tab.change.post` hook 은 user_direct 만 발화한다.
    TabRenamed {
        tab_id: u32,
        title: String,
        user_direct: bool,
    },
    /// 자식 프로세스 종료. exit_code는 현재 terminal 이벤트가 노출하지 않아 `None` 고정.
    ProcessExited {
        surface_id: u32,
    },
    /// `NotificationStore::add` 결과. source는 발화 측에서 채워 push (host=`"host"`,
    /// plugin=plugin_id).
    NotificationCreated {
        id: u64,
        title: String,
        body: String,
        source: String,
    },
    /// Tab 생성. `detect_tab_lifecycle` polling으로 발견.
    TabCreated {
        tab_id: u32,
        pane_id: u32,
        workspace_id: u32,
        kind: String,
    },
    /// Tab 종료. polling이 사라진 tab_id를 발견하면 마지막 위치로 enqueue.
    /// 현재 reason은 항상 User (PR 5의 caller context 도입 이후 Ipc 구분 예정).
    TabClosed {
        tab_id: u32,
        pane_id: u32,
    },
    /// Tab이 다른 pane으로 이동. polling diff로 감지.
    TabMoved {
        tab_id: u32,
        from_pane: u32,
        to_pane: u32,
    },
    /// Pane 생성. polling으로 감지. `parent_pane_group`은 트리 구조 노출 비용이
    /// 커 현재 `None` 고정 (필요해지면 PR 5 이후 확장).
    PaneCreated {
        pane_id: u32,
        workspace_id: u32,
    },
    /// Pane 종료. polling이 사라진 pane_id를 발견하면 발화.
    /// reason은 현재 항상 `User` (caller context 구분은 PR 5에서).
    PaneClosed {
        pane_id: u32,
    },
    /// Workspace 생성. polling으로 감지. `window_id`는 caller가 전달.
    WorkspaceCreated {
        workspace_id: u32,
        window_id: u64,
        name: String,
    },
    /// Workspace 종료. reason은 현재 항상 `User`.
    WorkspaceClosed {
        workspace_id: u32,
    },
    /// Pane 분할. polling으로는 direction을 알 수 없어 호출 사이트에서 직접 enqueue.
    PaneSplit {
        original_pane: u32,
        new_pane: u32,
        direction: crate::model::SplitDirection,
    },
    /// `tasty-hooks`의 surface hook 발화. `check_and_fire` 호출자가 fired hook_id
    /// 리스트와 매칭된 event를 묶어 enqueue. surface_id가 0이면 global hook.
    HookFired {
        hook_id: u64,
        event_kind: String,
        surface_id: u32,
        /// 실제 관측된 exit code — `CommandCompleted` 발화일 때만
        /// `Some`. `resolve_hook_fired_task_waits` 가 push 완료 전략의 성공/실패
        /// 판정에 쓴다(exit 0 → Succeeded, 비-0 → Failed). 다른 이벤트는 `None`.
        exit_code: Option<i32>,
    },
    // ─── Plugin lifecycle ───
    /// Plugin spawn 성공 후 hello 까지 완료.
    PluginLoaded {
        plugin_id: String,
        version: String,
    },
    /// Plugin 활성화 상태 변경 (enable=true / disable=false).
    PluginEnableToggled {
        plugin_id: String,
        enabled: bool,
    },
    /// Plugin process 가 종료됨. `reason` 은 LifecycleReason 의 serde rename
    /// (snake_case) — "user" / "ipc" / "crash".
    PluginUnloaded {
        plugin_id: String,
        reason: String,
    },
    /// Plugin spawn 실패 또는 runtime error.
    PluginError {
        plugin_id: String,
        error_kind: String,
        message: String,
    },
    /// Plugin install / remove / grant / revoke 완료. `change_kind` 는
    /// "installed" / "removed" / "permission_granted" / "permission_revoked".
    PluginRegistryChanged {
        plugin_id: String,
        change_kind: String,
        detail: serde_json::Value,
    },
    /// Plugin 의 surface_kind 가 hello 처리 직후 registry 에 등록됨.
    PluginSurfaceKindRegistered {
        plugin_id: String,
        kind: String,
        rendering: String,
    },
    /// Plugin manifest 의 `[[contributes.window]]` 항목이 hello 시점에 등록됨.
    /// 1.0 schema-only — host event `plugin.window_declared` 로 가시화.
    PluginWindowDeclared {
        plugin_id: String,
        window_id: String,
    },
}
