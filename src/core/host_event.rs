//! 도메인과 창 상태가 쌓고 GUI 메인 루프가 전달하는 이벤트.
//! 도메인이 창·plugin 모듈에 의존하지 않도록 여기서 일반 데이터로 정의한다.
//! 메인 루프가 plugin 프로토콜로 변환한다.

/// surface 종료 통지. 메인 루프가 is_user_close를 LifecycleReason으로 바꾼다.
// 헤드리스에서도 생성 경로는 컴파일되지만 큐를 비우는 쪽은 GUI뿐이다.
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
    /// layout에서 이미 제거돼 kind를 모르면 None이다. 전송할 때 빈 문자열로 바꾼다.
    pub(crate) kind: Option<String>,
    pub(crate) is_user_close: bool,
}

/// plugin 이벤트와 Lua hook으로 전달할 큐 항목. 큐를 비우는 쪽은 GUI뿐이다.
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
        generation: tasty_terminal::ResourceGeneration,
        title: String,
    },
    SurfaceCreated {
        surface_id: u32,
        kind: &'static str,
        tab_id: u32,
        pane_id: u32,
        workspace_id: u32,
        /// plugin이 생성했으면 그 ID, 사용자 생성이면 None이다.
        created_by_plugin: Option<String>,
    },
    WorkspaceActivated {
        workspace_id: u32,
        prev_workspace_id: Option<u32>,
    },
    /// 바뀐 필드만 Some이다. user_direct인 변경만 Lua workspace.change.post도 실행한다.
    /// IPC·CLI 변경은 plugin 이벤트 버스로만 전달한다.
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
    /// user_direct인 이름 변경만 Lua tab.change.post도 실행한다.
    TabRenamed {
        tab_id: u32,
        title: String,
        user_direct: bool,
    },
    /// 프로세스 종료 통지. 전송 payload의 exit_code는 현재 None이다.
    ProcessExited {
        surface_id: u32,
    },
    /// 알림 source는 host 또는 생성한 plugin ID다.
    NotificationCreated {
        id: u64,
        level: crate::notification::NotificationLevel,
        title: String,
        body: String,
        source: String,
    },
    TabCreated {
        tab_id: u32,
        pane_id: u32,
        workspace_id: u32,
        kind: String,
    },
    /// 탭 종료 통지. 전송 reason은 현재 User로 고정된다.
    TabClosed {
        tab_id: u32,
        pane_id: u32,
    },
    TabMoved {
        tab_id: u32,
        from_pane: u32,
        to_pane: u32,
    },
    /// pane 생성 통지. 전송 parent_pane_group은 현재 None이다.
    PaneCreated {
        pane_id: u32,
        workspace_id: u32,
    },
    /// pane 종료 통지. 전송 reason은 현재 User로 고정된다.
    PaneClosed {
        pane_id: u32,
    },
    /// window_id는 쌓지 않는다. 발행할 때 workspace를 가진 창을 조회해 채운다.
    WorkspaceCreated {
        workspace_id: u32,
        name: String,
    },
    /// workspace 종료 통지. 전송 reason은 현재 User로 고정된다.
    WorkspaceClosed {
        workspace_id: u32,
    },
    /// 폴링으로 분할 방향을 알 수 없어 분할한 호출부가 직접 쌓는다.
    PaneSplit {
        original_pane: u32,
        new_pane: u32,
        direction: crate::model::SplitDirection,
    },
    /// 실행한 hook ID와 이벤트를 전달한다. surface_id가 0이면 전역 hook이다.
    HookFired {
        hook_id: u64,
        event_kind: String,
        surface_id: u32,
        /// CommandCompleted에서 관측한 종료 코드. task 완료 판정은 0이면 성공, 그 외에는 실패로 쓴다.
        /// 다른 이벤트는 None이다.
        exit_code: Option<i32>,
    },
    /// 프로세스 시작과 hello를 마친 plugin.
    PluginLoaded {
        plugin_id: String,
        version: String,
    },
    PluginEnableToggled {
        plugin_id: String,
        enabled: bool,
    },
    /// 종료 reason은 user·ipc·crash 문자열이다.
    PluginUnloaded {
        plugin_id: String,
        reason: String,
    },
    PluginError {
        plugin_id: String,
        error_kind: String,
        message: String,
    },
    /// change_kind는 installed·removed 중 하나다.
    PluginRegistryChanged {
        plugin_id: String,
        change_kind: String,
        detail: serde_json::Value,
    },
    PluginSurfaceKindRegistered {
        plugin_id: String,
        kind: String,
        rendering: String,
    },
    /// hello에서 등록한 window 기여 선언. 실제 창을 생성했다는 뜻은 아니다.
    PluginWindowDeclared {
        plugin_id: String,
        window_id: String,
    },
    /// 사용자가 surface 에 보낸 새로고침 요청. 전달 직전에 같은 세대·kind 인지 다시 확인한다.
    #[cfg(feature = "gui")]
    SurfaceRefreshRequested {
        target: crate::runtime::surface_binding::SurfaceBinding,
        kind: String,
        /// `shortcut` 또는 `palette`.
        trigger: &'static str,
    },
}

/// 새로고침 대상이 키를 누른 시점의 세대·kind 그대로인가. 닫기·변환·재생성 뒤에는 거짓이다.
#[cfg(feature = "gui")]
pub(crate) fn refresh_target_current(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    target: &crate::runtime::surface_binding::SurfaceBinding,
    kind: &str,
) -> bool {
    target.current(engine)
        && engine
            .core
            .find_surface_by_id(target.surface_id())
            .is_some_and(|descriptor| descriptor.kind() == kind)
}
