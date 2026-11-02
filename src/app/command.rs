//! AppServices::apply의 변경 요청과 후속 처리용 결과 이벤트.
//! GUI 전용 요청은 cfg로 구분하며, 실제 상태 갱신과 App 후속 처리는 각 적용 경로가 나눠 맡는다.

use std::path::PathBuf;

use serde_json::Value;
use tasty_settings::Settings;

/// Terminal은 새 PTY를 만들고 Kind는 등록된 surface 생성기를 사용한다.
#[derive(Debug, Clone)]
pub(crate) enum ConvertSurfaceTarget {
    Terminal {
        cwd: Option<PathBuf>,
    },
    /// 변환 때 유지할 cwd. 호출자가 원래 surface와 cwd 상속 규칙에 따라 정한다.
    Kind {
        cwd: Option<PathBuf>,
        kind: String,
        params: Value,
    },
}

/// Bytes는 send_bytes, Text는 UTF-8 바이트를 쓰는 send_key에 전달한다. 둘 다 비동기 쓰기 큐를 사용한다.
#[derive(Debug, Clone)]
pub(crate) enum SendPayload {
    Bytes(Vec<u8>),
    Text(String),
    /// Live-only delayed user input, bound to the resource which accepted the operation.
    #[cfg(feature = "gui")]
    Bound {
        generation: tasty_terminal::ResourceGeneration,
        bytes: Vec<u8>,
    },
}

/// 로컬 복원은 전체 목록에서, mirror 복원은 점유 workspace 범위에서 항목을 고른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreScope {
    /// 원격 사용자 요청으로 닫힌 항목도 포함한 인스턴스 전체의 최신 항목.
    Local,
}

/// 알림을 저장소에 넣을 때 앞 항목과 합칠지 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationMerge {
    /// 같은 workspace·surface·level이 설정 간격 안에 이어지면 합친다. 터미널 OSC·벨처럼 한 출처의
    /// 연속 출력을 묶는다.
    Coalesce,
    /// 서로 다른 호스트 사건이다. 간격 안에 이어져도 제목·본문·미확인 수를 각각 남긴다.
    Separate,
}

impl NotificationMerge {
    /// 새 항목을 만들었으면 그 ID를, 앞 항목에 합쳤으면 `None`을 돌려준다.
    pub(crate) fn add(
        self,
        store: &mut crate::notification::NotificationStore,
        ws_id: u32,
        surface_id: u32,
        title: String,
        body: String,
        level: crate::notification::NotificationLevel,
    ) -> Option<u64> {
        match self {
            Self::Coalesce => store.add_with_level(ws_id, surface_id, title, body, level),
            Self::Separate => Some(store.add_separate(ws_id, surface_id, title, body, level)),
        }
    }
}

/// GUI 알림 저장 경로(`App::cascade_notification_pushed`)가 받는 알림 한 건.
#[cfg(feature = "gui")]
pub(crate) struct NotificationPush {
    pub level: crate::notification::NotificationLevel,
    pub ws_id: u32,
    pub surface_id: u32,
    pub title: String,
    pub body: String,
    pub source: String,
    pub merge: NotificationMerge,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: 큐 항목마다 Box를 추가 할당하지 않도록 값을 직접 보관한다.
pub(crate) enum DomainIntent {
    ApplyPreset {
        kind: tasty_presets::PresetKind,
        name: String,
        target_pane_id: Option<u32>,
        category: Option<u32>,
    },
    /// 새 설정을 후속 처리 이벤트로 넘긴다. AppServices::apply 자체가 테마·스크롤백 등을 갱신하지는 않는다.
    // reason: 전체 설정 교체는 설정 창 같은 GUI 경로만 만든다. 헤드리스 요청은 ChangeSettings를 쓴다.
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    UpdateSettings(Settings),
    /// 에이전트·plugin의 좁은 설정 변경. 실행 시점의 최신 설정에 적용하고 결과를 `done`으로 돌려준다.
    ChangeSettings {
        change: crate::app::settings_change::SettingsChange,
        done: crate::app::settings_change::SettingsDone,
    },

    /// cwd와 대상을 호출자가 지정한다. empty 종류는 거절하고 이름이 없으면 자동 이름을 쓴다.
    CreateWorkspace {
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
        /// 카테고리가 없거나 찾지 못하면 기본 분류에 둔다.
        category: Option<crate::model::WorkspaceCategoryId>,
    },
    /// Source identity is fixed at admission; the destination is an order coordinate.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    MoveWorkspace {
        workspace_id: u32,
        to_index: usize,
    },

    /// 없는 workspace나 카테고리면 오류다. workspace 순서와 사용자 선택은 바꾸지 않는다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    SetWorkspaceCategory {
        workspace_id: u32,
        category: crate::model::WorkspaceCategoryId,
    },

    /// 이름을 검증해 목록 끝에 추가한다.
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    CreateCategory {
        name: String,
    },
    /// normal은 이름을 바꿀 수 없다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    RenameCategory {
        id: crate::model::WorkspaceCategoryId,
        name: String,
    },
    /// normal은 지울 수 없다. 안의 workspace는 normal로 옮긴다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    DeleteCategory {
        id: crate::model::WorkspaceCategoryId,
    },
    CreateTab {
        pane_id: u32,
        cwd: Option<PathBuf>,
        kind: String,
        /// 명시 이름은 자동 제목보다 우선한다.
        name: Option<String>,
        surface_params: Value,
        /// 비터미널 탭의 선택 여부. 에이전트 생성은 false여야 사용자 선택을 유지한다.
        /// terminal은 이 값과 무관하게 배경 탭으로 만든다.
        activate: bool,
    },
    /// 트리를 닫고 자원 정리 대상은 후속 처리에 넘긴다.
    #[cfg_attr(
        all(not(feature = "gui"), not(debug_assertions)),
        expect(
            dead_code,
            reason = "GUI and debug navigation construct this request; release headless workspace.close uses direct journal admission"
        )
    )]
    CloseWorkspace {
        workspace_id: u32,
    },
    RetireExitedSurface {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    CloseTab {
        tab_id: u32,
    },
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    MoveTab {
        pane_id: u32,
        tab_id: u32,
        to_index: usize,
    },
    /// pane을 분할한다. 사용자 요청의 새 pane 선택은 App 후속 처리에서 맡는다.
    SplitPane {
        target_pane_id: u32,
        direction: crate::model::SplitDirection,
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
    },
    /// 같은 탭 안에서 분할한다. 사용자 요청의 새 surface 선택은 App 후속 처리에서 맡는다.
    SplitSurface {
        target_surface_id: u32,
        direction: crate::model::SplitDirection,
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
    },
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    ClosePane {
        pane_id: u32,
    },
    /// 빈 상위 tab·pane·workspace까지 닫을 수 있다. save_snapshot은 복원 기록 저장 여부다.
    /// 자원·메모리 정리, 활성 workspace 보정과 빈 창 보충은 호출자의 후속 처리다.
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    CloseSurface {
        surface_id: u32,
        presentation: Option<Box<crate::model::StructurePresentationSnapshot>>,
    },
    /// split 탭의 leaf 또는 단일 surface 탭을 다른 종류로 바꾼다.
    ConvertSurface {
        surface_id: u32,
        target: ConvertSurfaceTarget,
    },
    /// source의 Terminal·scrollback·ID는 유지하며 target 위치로 옮긴다.
    /// 덮어쓴 target은 후속 처리로 정리하고 닫기 복원 기록에 남기지 않는다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "GUI input constructs this explicit target; headless IPC resolves its wire request directly in the journal"
        )
    )]
    MoveSurface {
        source_surface_id: u32,
        target_surface_id: u32,
    },
    /// source 탭(ID·이름·surface·Terminal·scrollback 유지)을 target 탭 자리로 옮긴다.
    /// 덮어쓴 target 탭은 후속 처리로 정리하고 닫기 복원 기록에 남기지 않는다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(dead_code, reason = "only the gui-only tab context menu issues it")
    )]
    ReplaceTabWithTab {
        source_tab_id: u32,
        target_tab_id: u32,
    },
    /// source 페인(ID·탭·surface·Terminal·scrollback 유지)을 target 페인 자리로 옮긴다.
    /// 덮어쓴 target 페인은 후속 처리로 정리하고 닫기 복원 기록에 남기지 않는다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(dead_code, reason = "only the gui-only tab context menu issues it")
    )]
    ReplacePaneWithPane {
        source_pane_id: u32,
        target_pane_id: u32,
    },

    SendToSurface {
        surface_id: u32,
        payload: SendPayload,
    },

    /// workspace ID로 알림을 라우팅한다. source는 생성 주체를 구별하는 태그다.
    PushNotification {
        level: crate::notification::NotificationLevel,
        ws_id: u32,
        surface_id: u32,
        title: String,
        body: String,
        source: String,
        merge: NotificationMerge,
    },
    #[cfg(feature = "gui")]
    MarkNotificationRead {
        id: u64,
    },
    #[cfg(feature = "gui")]
    MarkAllNotificationsRead {
        ids: Vec<u64>,
    },

    #[cfg(feature = "gui")]
    SurfaceCwdChanged {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    SetTerminalMark {
        surface_id: u32,
    },

    /// 지정 surface에 attention을 요청한다. 기본 kind는 IPC 핸들러에서 정한다.
    SurfaceCompletion {
        surface_id: u32,
        kind: crate::core::AttentionKind,
    },

    /// kind가 None이면 현재 attention을 지우고 Some이면 같은 종류만 지운다.
    /// 뒤늦은 완료 해제가 더 최근의 입력 대기 표시를 지우지 않도록 종류를 지정할 수 있다.
    SurfaceAttentionClear {
        surface_id: u32,
        kind: Option<crate::core::AttentionKind>,
    },

    /// scope에 맞는 최신 항목을 꺼내 복원한다. workspace 자체를 복원할 때는 target_pane_id를 쓰지 않는다.
    /// 그 외에는 대상 pane이 필요하며, 꺼낸 뒤 대상이 없다고 실패해도 항목을 복원 목록에 돌려놓지 않는다.
    RestoreClosedItem {
        target_pane_id: Option<u32>,
        scope: RestoreScope,
    },

    /// 탭에서 선택된 surface의 OSC 제목만 반영하고 사용자의 명시 이름은 유지한다.
    #[cfg(feature = "gui")]
    UpdateTabName {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
        name: String,
    },

    /// GUI 파일 식별 worker에 요청한다. 결과는 App 이벤트로 받으며 worker가 없으면 로그만 남긴다.
    #[cfg(feature = "gui")]
    DispatchFile {
        target: crate::file::format::FileTarget,
        depth: crate::file::format::DetectDepth,
        /// 지정 surface의 pane에 새 탭을 만든다. None이면 사용자 선택 pane을 쓴다.
        origin_surface_id: Option<u32>,
        /// 비동기 식별 후에도 호출 주체를 구분하기 위한 값.
        dispatch_origin: crate::core::origin::FileDispatchOrigin,
        /// 대용량 markdown 확인을 건너뛸지 여부. 기본 false다.
        ignore_size_limit: bool,
        /// 최종 결과를 받을 요청의 run([`crate::request_runs`]). IPC 요청만 가진다.
        run: Option<u64>,
    },
}

/// 적용 결과와 후속 처리 요청. App 또는 헤드리스 호출자가 지원하는 이벤트를 처리한다.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: 큐 항목마다 Box를 추가 할당하지 않도록 값을 직접 보관한다.
pub(crate) enum CoreEvent {
    /// `done`이 있으면 적용·저장 결과를 원 요청에 돌려준다.
    SettingsUpdated {
        settings: Settings,
        done: Option<crate::app::settings_change::SettingsDone>,
    },

    /// sent는 터미널 입력 함수를 호출했는지이며 PTY 쓰기 완료를 뜻하지 않는다.
    /// 거절 사유 중 attach 점유는 hard_occupied로 구별한다.
    SurfaceSent {
        sent: bool,
        hard_occupied: bool,
    },
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "some shared event fields are read only by GUI dispatch and remain unused in headless builds"
        )
    )]
    NotificationPushRequested {
        level: crate::notification::NotificationLevel,
        ws_id: u32,
        surface_id: u32,
        title: String,
        body: String,
        source: String,
        merge: NotificationMerge,
    },
    #[cfg(feature = "gui")]
    NotificationReadRequested {
        id: u64,
    },
    #[cfg(feature = "gui")]
    AllNotificationsReadRequested {
        ids: Vec<u64>,
    },

    #[cfg(feature = "gui")]
    SurfaceCwdChanged {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    TerminalMarkSet {
        surface_id: u32,
    },

    SurfaceCompletionRequested {
        surface_id: u32,
        kind: crate::core::AttentionKind,
    },

    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "some shared event fields are read only by GUI dispatch and remain unused in headless builds"
        )
    )]
    SurfaceAttentionClearRequested {
        surface_id: u32,
        kind: Option<crate::core::AttentionKind>,
    },

    /// 자식 프로세스 종료. GUI는 hook·알림과 닫기 요청을 이어 처리한다.
    TerminalProcessExited {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    /// OSC 제목 변경. GUI는 host 이벤트와 탭 제목 갱신 요청으로 처리한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "some shared event fields are read only by GUI dispatch and remain unused in headless builds"
        )
    )]
    TerminalTitleChanged {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
        title: String,
    },

    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "some shared event fields are read only by GUI dispatch and remain unused in headless builds"
        )
    )]
    TerminalNotification {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
        title: String,
        body: String,
    },

    TerminalBellRing {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    /// 완성된 출력 줄. 해당 surface에 OutputMatch hook이 있을 때만 만들어진다.
    TerminalOutputMatch {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
        text: String,
    },

    /// cwd 변경. GUI는 후속 요청으로, 헤드리스는 PTY 처리 경로에서 직접 반영한다.
    TerminalCwdChanged {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    /// OSC 133의 명령 완료 보고. GUI는 종료 코드와 무관하게 완료 attention을 올리고 hook에도 코드를 전달한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "some shared event fields are read only by GUI dispatch and remain unused in headless builds"
        )
    )]
    TerminalCommandCompleted {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
        exit_code: Option<i32>,
    },

    /// 출력 이후 일정 시간 동안 OSC 133 경계가 없다는 안내 요청. 셸 통합 미설치를 확정한 것은 아니다.
    /// 배너로만 소비되므로 GUI 조합에만 있다.
    #[cfg(feature = "gui")]
    TerminalShellIntegrationHint {
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    /// Completed clipboard write, bound to the originating terminal generation.
    TerminalClipboardSet {
        #[cfg_attr(
            not(feature = "gui"),
            expect(
                dead_code,
                reason = "headless produces the outcome; only GUI displays clipboard feedback"
            )
        )]
        result: Result<(), crate::clipboard::error::ClipboardError>,
        surface_id: u32,
        generation: tasty_terminal::ResourceGeneration,
    },

    /// 탭 표시를 다시 그리기 위한 결과. OSC 제목은 레이아웃 저장 대상이 아니다.
    #[cfg(feature = "gui")]
    TabNameUpdated {
        /// 명시 이름 때문에 건너뛴 경우를 검사에서 구별한다. 제품 후속 처리는 이 값을 읽지 않는다.
        #[allow(dead_code)]
        skipped_explicit: bool,
    },

    #[cfg(feature = "gui")]
    PluginLoaded {
        plugin_id: String,
        version: String,
    },

    PluginEnableToggled {
        plugin_id: String,
        enabled: bool,
    },

    /// plugin 종료 사유. 현재 이 이벤트를 만드는 plugin IPC 경로는 User를 넣는다.
    PluginUnloaded {
        plugin_id: String,
        reason: tasty_plugin_protocol::events::LifecycleReason,
    },

    /// 실패 후속 처리는 구현돼 있지만 이 CoreEvent를 생성하는 제품 경로는 아직 없다.
    #[allow(dead_code)]
    PluginError {
        plugin_id: String,
        error_kind: String,
        message: String,
    },

    /// hello에서 등록한 surface 종류와 rendering 값.
    #[cfg(feature = "gui")]
    PluginSurfaceKindRegistered {
        plugin_id: String,
        kind: String,
        rendering: String,
    },

    #[cfg(feature = "gui")]
    PluginRegistryChanged {
        plugin_id: String,
        change: PluginRegistryChange,
    },

    /// window 기여 선언 등록 통지이며 실제 창 생성 완료를 뜻하지 않는다.
    #[cfg(feature = "gui")]
    PluginWindowDeclared {
        plugin_id: String,
        window_id: String,
    },
}

#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) enum PluginRegistryChange {
    Installed { version: String },
    Removed,
    PermissionGranted { permission: String },
    PermissionRevoked { permission: String },
}

/// PTY 출력 처리 뒤 호출자가 이어 처리할 이벤트 목록.
#[derive(Debug, Default)]
pub(crate) struct ProcessPtyOutcome {
    pub events: Vec<CoreEvent>,
}

impl CoreEvent {
    /// Keep resource identity through the application queue, not just the raw event drain.
    pub(crate) fn terminal_binding(&self) -> Option<(u32, tasty_terminal::ResourceGeneration)> {
        match self {
            #[cfg(feature = "gui")]
            Self::SurfaceCwdChanged {
                surface_id,
                generation,
            } => Some((*surface_id, *generation)),
            Self::TerminalProcessExited {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalTitleChanged {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalNotification {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalBellRing {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalOutputMatch {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalCwdChanged {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalCommandCompleted {
                surface_id,
                generation,
                ..
            }
            | Self::TerminalClipboardSet {
                surface_id,
                generation,
                ..
            } => Some((*surface_id, *generation)),
            #[cfg(feature = "gui")]
            Self::TerminalShellIntegrationHint {
                surface_id,
                generation,
            } => Some((*surface_id, *generation)),
            _ => None,
        }
    }
}
