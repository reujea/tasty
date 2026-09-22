//! `DomainIntent` — `Core::apply` 의 입력. 영속 도메인 mutate 요청.
//!
//! 분류축 (intent-ui-vs-domain.md): 모든 Intent 는 *UI Intent* (시각 상태
//! 변경, `crate::intent::UiIntent`) 또는 *Domain Intent* (도메인 mutate, 본
//! 타입) 중 하나다. `DomainIntent` 는 headless 빌드에서도 그대로 실행된다.
//!
//! 현재 큐 구조:
//! - `AppState.pending_intents`: 통합 Intent 큐. UI Intent (`Intent::Ui`) 와
//!   Domain Intent (`Intent::Domain(DomainIntent)`) 가 같은 큐 위에서 처리됨.
//!   `App::dispatch_pending_intents` 가 매 frame drain — UI 항목은 popup handler
//!   분기, Domain 항목은 별 batch 로 모아 `dispatch_domain_intent` (core.apply +
//!   handle_core_event cascade) 일괄 처리.

use std::path::PathBuf;

use serde_json::Value;
use tasty_settings::Settings;

/// `DomainIntent::ConvertSurface` 의 target. variant 별로 새 surface 생성
/// 경로가 다름 (terminal: PTY spawn, kind: SurfaceKindRegistry 경유 — markdown
/// / image 등 모든 host/plugin kind 통합).
#[derive(Debug, Clone)]
pub(crate) enum ConvertSurfaceTarget {
    Terminal {
        cwd: Option<PathBuf>,
    },
    /// `cwd` 는 호출자가 source surface 로부터 resolve 한 carry cwd. Surface cwd
    /// invariant — 호출자는 None 으로 임의 고정하지 않는다 (변환 시 cwd 손실 금지).
    /// 자세한 규칙은 `docs/architecture/invariants/surface-cwd.md`.
    Kind {
        cwd: Option<PathBuf>,
        kind: String,
        params: Value,
    },
}

/// `DomainIntent::SendToSurface` 의 payload. 호출자가 *어느 메서드 호출* 할지
/// 결정해 전달 — Core 는 받아서 그대로 dispatch.
/// - `Bytes`: 변환 완료된 raw bytes (control sequences 포함). `send_bytes` 호출.
/// - `Text`: raw text. `send_key` 호출 (escape 처리 없음 — UTF-8 그대로).
#[derive(Debug, Clone)]
pub(crate) enum SendPayload {
    Bytes(Vec<u8>),
    Text(String),
}

/// [`DomainIntent::RestoreClosedItem`] 이 복원 스택에서 무엇을 꺼낼지 정하는 스코프.
///
/// 스택은 한 인스턴스 안에서 두 사용자에게 공유된다 — 그 기계 앞에 앉은 사용자와,
/// 워크스페이스를 원격에서 점유한 mirror 사용자. 결정·대안은
/// `docs/adr/0264-mirror-restore-closed-item-runs-on-the-remote.md` 결정 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreScope {
    /// 이 인스턴스 앞의 사용자가 누른 복원. 전역 LIFO — 스코프를 걸지 않는다.
    /// forward 된 close 는 이 인스턴스 자신의 트리에서 탭을 없애므로, 그 변경도
    /// 여기 앉은 사용자의 undo 대상이어야 한다.
    Local,
    /// mirror client 가 forward 한 복원. 그 워크스페이스에서 닫힌 항목만 후보다 —
    /// 복원 결과가 anchor 워크스페이스 안에 떨어져야 forward 실행의 트리 diff 가
    /// 그것을 delta 로 실어 보낸다.
    Workspace(u32),
}

/// 도메인 변경 요청. Core 만이 자기 메서드로 적용한다.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: hot intent queue 에 Box 화 시 alloc 비용 큼
pub(crate) enum DomainIntent {
    // ─── Settings ───
    /// Settings 전체 교체. cascade — Theme apply / Scrollback limit / clipboard
    /// max / notification coalesce 가 Core 내부에서 자동 발동.
    UpdateSettings(Settings),

    // ─── Workspace lifecycle ───
    /// 새 workspace 를 생성. focused 의존 없음 — `cwd` 는 호출자가 미리
    /// 결정해 payload 로 넘긴다 (terminal kind 에서 사용). `kind="empty"` 는
    /// 거부. `name` 이 None 이면 자동 ("Workspace N"). cascade 가 host event
    /// (WorkspaceRenamed) 발화 + (User origin 이면) active 전환.
    CreateWorkspace {
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
        /// 생성 시점 카테고리 소속. `None` 이면 normal(기본).
        category: Option<crate::model::WorkspaceCategoryId>,
    },
    /// 기존 workspace 의 메타 (name/subtitle/description) 부분 갱신. None
    /// 필드는 변경 없음. cascade 가 host event (WorkspaceRenamed) 발화.
    UpdateWorkspaceMeta {
        workspace_id: u32,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
    },
    /// workspace 순서 이동 (from_index → to_index). out-of-range 또는
    /// from==to 면 no-op. cascade 가 발화 source 의 active_workspace 를
    /// *사용자가 보던 동일 ws 가 계속 active 유지* 되도록 보정.
    MoveWorkspace { from_index: usize, to_index: usize },

    // ─── Tab lifecycle ───
    /// 특정 pane 에 새 tab 생성. focused pane 의존 없음 — 호출자가 pane_id
    /// 미리 결정. `cwd` 는 terminal kind 에서만 사용 (호출자가 inherit 결정).
    CreateTab {
        pane_id: u32,
        cwd: Option<PathBuf>,
        kind: String,
        /// 명시 탭 이름. `Some` 이면 생성 시점부터 `explicit_name` 으로 고정되어
        /// `display_name()` 에서 최우선으로 쓰인다 (cwd/OSC title 로 덮이지 않음).
        name: Option<String>,
        surface_params: Value,
        /// 새 탭을 그 pane 의 활성 탭으로 세우는가. 사용자가 직접 연 탭이면 `true`,
        /// 에이전트(IPC/CLI)가 만든 탭이면 `false` — 에이전트 행동이 사용자가 보던 탭을
        /// 바꾸면 안 된다([ADR-0502](../../docs/adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md)).
        /// terminal kind 는 이 값과 무관하게 background 로 붙는다 — 사용자의 새 터미널
        /// 탭은 이 인텐트가 아니라 `AppState::add_tab` 이 연다.
        activate: bool,
    },
    /// tab_id 로 tab close. *모든* workspace 의 pane 순회 (포커스 독립).
    /// cleanup (markdown_views / image_views / surface_meta / memory purge) 은
    /// cascade 에서 처리 — Core 가 AppState 데이터 모름.
    CloseTab { tab_id: u32 },
    /// pane 안 tab 순서 이동 (from_index → to_index). out-of-range 면 no-op.
    /// pane_id 로 *모든* workspace 순회 — focused 의존 없음.
    MoveTab {
        pane_id: u32,
        from_index: usize,
        to_index: usize,
    },
    /// headless PTY(`pty_registry`, ADR-0050 · features/headless-pty 참고)를 실제
    /// Surface 로 **승격(adopt)** 한다(`pty.attach_surface`). `CreateTab` 처럼 새
    /// tab_id/surface_id 를 발급받아
    /// Tab/Pane 트리에 marker 를 꽂되, **새 Terminal 을 spawn 하지 않는다** — 이미
    /// headless(`PTY_ID_BASE` 이상) id 로 `TerminalStore` 에 존재하는 Terminal 을
    /// 새 surface_id 로 re-key 하고 `pty_registry` 에서 제거한다. `RespawnTerminal`
    /// 이 같은 id 위에서 Terminal 을 교체하는 것과 반대 방향(id 이전) 연산이다.
    /// 성공 시 `CoreEvent::TabCreated` 를 발행 — cascade 는 `CreateTab` 과 동형.
    AdoptTerminal { pane_id: u32, pty_id: u32 },

    // ─── Pane lifecycle ───
    /// 특정 pane 을 split. focused 의존 없음 — 호출자가 target_pane_id 결정.
    /// cwd 는 terminal kind 에서만 사용 (호출자가 inherit 결정).
    /// cascade 가 host event (PaneSplit) 발화 + (User origin 이면) focused_pane
    /// 을 new_pane_id 로 변경.
    SplitPane {
        target_pane_id: u32,
        direction: crate::model::SplitDirection,
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
    },
    /// 특정 surface 를 split (같은 tab 안에서). focused 의존 없음 — 호출자가
    /// target_surface_id 결정. cascade 가 (User origin 이면) tab 의
    /// focused_surface 를 new_surface_id 로 변경.
    SplitSurface {
        target_surface_id: u32,
        direction: crate::model::SplitDirection,
        cwd: Option<PathBuf>,
        kind: String,
        surface_params: Value,
    },
    /// pane_id 로 pane 제거. workspace 안 invariant (focused_pane 보존) 는
    /// Core::apply 가 직접 처리 (workspace 안 *닫힌 곳* 의 자연 이동 — 원칙 1
    /// 위반 아님). cleanup_surface (markdown/image/memory) 는 cascade.
    ClosePane { pane_id: u32 },
    /// surface 제거. cascading close — surface→tab→pane→workspace 단계까지
    /// 자동 cascade. `save_snapshot=true` 면 각 단계에서 ClosedItem snapshot
    /// 푸시 (Ctrl+Shift+T 복원). cleanup_surface / workspace scope memory purge /
    /// active_workspace 보정 / auto-recreate empty workspace 는 cascade + handler.
    CloseSurface {
        surface_id: u32,
        save_snapshot: bool,
    },
    /// 기존 surface 를 다른 kind 로 변환. tab 의 split 안 leaf 만 교체 / sole
    /// surface tab 전체 교체. terminal 변환은 호출자가 cwd 미리 결정.
    ConvertSurface {
        surface_id: u32,
        target: ConvertSurfaceTarget,
    },
    /// 살아있는 surface 를 트리에서 떼어 다른 위치로 **이동(replace)** 한다 (T9).
    /// `source` 를 그 자리에서 떼어내(형제 끌어올림 / sole 이면 tab/pane/workspace
    /// cascade) `target` 위치의 leaf 를 대체하고, `target` 의 옛 surface 는 닫는다
    /// (PTY kill, closed-item 히스토리 미기록). source 의 Terminal/scrollback 은
    /// surface_id 불변이라 `TerminalStore` 가 자동으로 따라온다 — **이동 경로는
    /// source 에 대해 store/cleanup 을 절대 호출하지 않는다(PTY 보존).**
    MoveSurface {
        source_surface_id: u32,
        target_surface_id: u32,
    },

    // ─── Terminal send ───
    /// terminal surface 에 입력 전송. payload 의 종류에 따라 send_bytes 또는
    /// send_key 호출. ensure_surface_initialized 도 Core 가 처리.
    SendToSurface {
        surface_id: u32,
        payload: SendPayload,
    },
    /// 특정 surface 의 PTY 를 새 terminal 로 교체 (respawn). cwd 가 주어지면
    /// 새 PTY 의 working_dir 로 사용. plugin `claude.respawn` 의 진입점.
    RespawnTerminal {
        surface_id: u32,
        cwd: Option<PathBuf>,
    },

    // ─── Notifications ───
    /// 알림 push. ws_id 가 라우팅 키 — 해당 workspace 가 속한 main window 의
    /// notifications store 에 add (coalesce 자동) + host event enqueue.
    /// `source` 는 host event 의 source 태그 ("host" / "telemetry.cap" 등).
    PushNotification {
        ws_id: u32,
        surface_id: u32,
        title: String,
        body: String,
        source: String,
    },
    /// 특정 알림 읽음 처리. cascade 가 알림을 보유한 main/parked engine 의
    /// `notifications.mark_read(id)` 호출.
    #[cfg(feature = "gui")]
    MarkNotificationRead { id: u64 },
    /// 모든 알림 읽음 처리. cascade 가 main/parked 모두의
    /// `notifications.mark_all_read()` 호출.
    #[cfg(feature = "gui")]
    MarkAllNotificationsRead,

    // ─── Surface lifecycle ───
    /// Terminal 이 OSC 7 등으로 cwd 변경을 알림. cascade 가
    /// `refresh_tab_display_name` + `mark_layout_dirty` 수행.
    #[cfg(feature = "gui")]
    SurfaceCwdChanged { surface_id: u32 },

    // ─── Terminal control ───
    /// 특정 surface 의 read mark 설정. cascade 가 main/parked 의 engine
    /// 순회 후 terminal.set_mark() 호출. surface_id 가 None 이면 focused.
    SetTerminalMark { surface_id: u32 },

    // ─── Surface completion (attention producer) ───
    /// "이 surface 에 attention 이 필요하다" 신호. attention 을 발동하는 producer
    /// 중 하나(release 정식 IPC/CLI, Claude 플러그인 훅). cascade 가 surface 보유
    /// engine 의 `raise_attention(surface_id, kind)` + redraw. surface_id 필수
    /// (포커스 독립 — 불가침 원칙 1). `kind` 기본값은 IPC 핸들러가 `Completion`
    /// 으로 채운다(하위 호환 — 기존 CLI/OSC 133/toast 는 kind 를 모른다).
    SurfaceCompletion {
        surface_id: u32,
        kind: super::AttentionKind,
    },

    // ─── Surface attention 해제 (clear producer) ───
    /// "이 surface 의 attention 을 지운다" 신호. `SurfaceCompletion` 의 역방향으로,
    /// 실 렌더 포커스(`gpu.rs`)·알림 읽음에 이은 세 번째 clear producer 다 — 그 둘은
    /// 모두 GUI 로컬 사건이라 headless 인스턴스에는 clear producer 가 하나도 없었다.
    /// cascade 가 surface 보유 engine 의 `clear_attention(surface_id)` + redraw.
    /// surface_id 필수(포커스 독립 — 불가침 원칙 1).
    ///
    /// `kind` 는 **선택적 필터**다. `None` 이면 현재 kind 와 무관하게 지우고,
    /// `Some(k)` 면 현재 기록된 kind 가 `k` 일 때만 지운다 — 지운 뒤 다른 producer 가
    /// 더 급한 kind(`NeedsInput`)로 다시 발동한 것을 뒤늦게 도착한 해제가 덮어쓰는
    /// 것을 호출자가 막을 수 있게 한다.
    SurfaceAttentionClear {
        surface_id: u32,
        kind: Option<super::AttentionKind>,
    },

    // ─── Closed items ───
    /// closed_items 에서 `scope` 가 정하는 가장 최근 항목을 pop 해 복원.
    /// `target_pane_id` 는 *호출자가 결정한* attach 대상 (focused pane).
    /// Workspace 복원 시에는 사용 안 함.
    /// `target_pane_id == None` 이면 (engine.workspaces 비어있는 상태에서
    /// Surface/Tab 을 복원 요청한 경우) 복원은 Workspace 인 경우만 가능.
    /// 이 경우 caller 가 사전에 ensure_workspace_exists 처리하는 것을 권장.
    RestoreClosedItem {
        target_pane_id: Option<u32>,
        scope: RestoreScope,
    },

    // ─── Tab name ───
    /// Terminal 의 OSC 0/2 title 변경 등으로 tab 표시명을 갱신. surface_id 가
    /// 속한 tab 을 모든 workspace 에서 찾아 `osc_title` 필드 set. explicit_name
    /// 은 *건드리지 않음* — 사용자가 직접 이름 지은 tab 의 이름은 OSC title 에
    /// 의해 덮어쓰여지지 않는다 (display_name 우선순위: explicit_name >
    /// osc_title > cached_display_name > name).
    #[cfg(feature = "gui")]
    UpdateTabName { surface_id: u32, name: String },

    // ─── Layout persistence ───
    /// 현재 layout 을 ~/.tasty/layouts/NN.json 슬롯 파일에 저장.
    /// - `active_workspace`: 호출자가 결정한 active workspace 인덱스 (AppState 가
    ///   들고 있는 정보이므로 Intent 발화 시 동봉).
    /// - `force=true`: shutdown 경로용. debounce 무시 + `restore_surface_content`
    ///   설정이 켜져 있으면 layout_dirty 가 false 여도 저장.
    /// - `force=false`: main loop tick 경로. debounce 통과 시에만 저장.
    ///
    /// settings.restore_layout=false 면 skip. cascade 없음. host event 없음.
    #[cfg(feature = "gui")]
    SaveLayoutNow {
        active_workspace: usize,
        force: bool,
    },

    /// `engine.pending_layout_restore` 를 take 해 live engine 으로 복원.
    /// 호출 전에 *호출자* 가 wait-for-plugin loop 를 끝내 둬야 함 — Intent 본문
    /// 안에서 plugin manager 를 못 만진다 (Core 의존 없음).
    /// pending_layout_restore 가 None 이면 no-op (`restored=false`).
    #[cfg(feature = "gui")]
    ApplyPendingLayoutRestore,

    // ─── File dispatch ───
    /// 파일 dispatch 진입점. mouse ctrl+click / drag&drop / IPC `file_handler.dispatch`
    /// 가 발화. apply 분기에서 `engine.identify_worker.spawn(target, depth)` 호출 —
    /// Cheap/Deep 모두 worker thread 경유 (통일된 경로). 결과는
    /// `AppEvent::IdentifyDone` 으로 main thread 도착 후 `event_handler` 가
    /// `file::dispatch::apply_identify_result` 를 직접 호출. worker 미주입 시 drop
    /// + warn.
    ///
    /// gui 빌드에만 있다 — 적용할 identify worker 도, 이 intent 를 만드는 자리도 gui 에만
    /// 있다. headless 에서 에이전트 경로(`file_handler.dispatch`)는 `-32017` 로 거절된다
    /// (docs/adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md).
    #[cfg(feature = "gui")]
    DispatchFile {
        target: crate::file::format::FileTarget,
        depth: crate::file::format::DetectDepth,
        /// OpenSurface action 실행 시 이 surface 가 속한 *Pane* 에 새 tab 으로 추가.
        /// None 이면 focused pane 의 새 탭 (기존 동작).
        origin_surface_id: Option<u32>,
        /// 누가 열었는가. 비동기 식별 왕복이 `IntentOrigin` 을 못 나르므로 값으로 싣는다
        /// — 계약은 [`crate::core::origin::FileDispatchOrigin`].
        dispatch_origin: crate::core::origin::FileDispatchOrigin,
        /// true 면 대용량 markdown 확인 게이트를 건너뛰고 즉시 연다(에이전트/IPC
        /// 강제 열기). 기본 false — 게이트(팝업) 적용.
        ignore_size_limit: bool,
    },
}

/// `Core::apply` 의 결과 — `handle_core_event` 가 소비해 도메인 cascade(설정 적용,
/// 워크스페이스/탭 생성, 알림 등)를 구동한다.
/// (관찰: observer/remote-attach 는 각자 별도 메커니즘으로 이미 완성돼 CoreEvent 를
/// 쓸 계획이 없고, replay 는 기능 자체가 미착수.)
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: event queue 의 Box 화는 alloc/clone 비용 큼
pub(crate) enum CoreEvent {
    // ─── Settings ───
    /// Settings 가 갱신됨. 새 값 동봉.
    SettingsUpdated(Settings),

    // ─── Workspace lifecycle ───
    /// 새 workspace 생성 완료. cascade 가 host event (WorkspaceRenamed —
    /// name/subtitle/description 이 설정된 경우) 발화 + (User origin 이면)
    /// active 전환. `surface_id` 는 focused tab 의 surface.
    WorkspaceCreated {
        id: u32,
        index: usize,
        surface_id: Option<u32>,
        renamed_name: Option<String>,
        renamed_subtitle: Option<String>,
        renamed_description: Option<String>,
    },
    /// workspace 메타 갱신 완료. cascade 가 host event (WorkspaceRenamed) 발화.
    WorkspaceMetaUpdated {
        workspace_id: u32,
        index: usize,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
    },
    /// workspace 가 이동됨. cascade 가 발화 source 의 active_workspace 보정.
    /// `moved=false` 면 no-op (out-of-range / from==to).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    WorkspaceMoved {
        from_index: usize,
        to_index: usize,
        moved: bool,
    },

    // ─── Tab lifecycle ───
    /// 새 tab 생성 완료. cascade 추가 처리 없음 (main.mark_dirty 만).
    TabCreated {
        pane_id: u32,
        tab_id: u32,
        surface_id: u32,
        tab_count: usize,
        active_tab: usize,
    },
    /// tab close 완료. `cleanup_targets` 는 닫힌 tab 안의 (surface_id,
    /// persist_id) — cascade 가 각각에 `AppState::cleanup_surface` 호출.
    /// `pane_id` 는 닫힌 tab 이 속해 있던 pane (host event 발화용). 못 찾은
    /// 경우 `None`.
    TabClosed {
        tab_id: u32,
        pane_id: Option<u32>,
        closed: bool,
        cleanup_targets: Vec<(u32, Option<String>)>,
    },
    /// tab 이동 완료. `moved=false` 면 no-op (pane 없음 / from==to / out-of-range).
    TabMoved { moved: bool },

    // ─── Pane lifecycle ───
    /// pane split 완료. cascade 가 host event (PaneSplit) 발화 + (User origin
    /// 이면) focused_pane 변경.
    PaneSplit {
        workspace_index: usize,
        original_pane_id: u32,
        new_pane_id: u32,
        new_surface_id: u32,
        direction: crate::model::SplitDirection,
    },
    /// surface split 완료. cascade 가 (User origin 이면) tab 의 focused_surface
    /// 를 new_surface_id 로 변경.
    SurfaceSplit {
        workspace_index: usize,
        pane_id: u32,
        new_surface_id: u32,
    },
    /// pane close 완료. cleanup_targets 는 닫힌 pane 안의 (surface_id,
    /// persist_id) — cascade 가 cleanup_surface 호출.
    PaneClosed {
        pane_id: u32,
        closed: bool,
        cleanup_targets: Vec<(u32, Option<String>)>,
    },
    /// surface close 완료 (cascading). `cascade_level` 은 어디까지 닫혔는지
    /// (Surface/Tab/Pane/Workspace). `workspace_purged` 는 Case 4 (workspace
    /// 자체 닫힘) 시 cascade 가 memory scope purge 할 workspace_id.
    /// `workspaces_now_empty` 가 true 면 caller 가 auto-recreate.
    /// `closed_tab_ids` / `closed_pane_ids` 는 cascade 가 host event
    /// (`tab.closed` / `pane.closed`) 를 발화할 때 사용 — Surface level 은 비어
    /// 있고, Tab level 은 닫힌 tab 1 개, Pane level 은 tab + pane, Workspace
    /// level 은 workspace 안 모든 tab + pane.
    SurfaceClosed {
        surface_id: u32,
        closed: bool,
        cascade_level: CascadeLevel,
        cleanup_targets: Vec<(u32, Option<String>)>,
        closed_tab_ids: Vec<u32>,
        closed_pane_ids: Vec<u32>,
        /// 마지막 surface 가 닫혀 workspace 째 사라졌다면 그 **(인덱스, id)**.
        ///
        /// 둘을 한 필드로 묶어 "id 는 실렸는데 인덱스는 빠졌다" 를 타입이 막는다 —
        /// 인덱스가 빠지면 활성 포인터 보정이 조용히 건너뛰어져 사용자 화면이 밀린다.
        /// id 는 memory scope purge 와 `workspace.closed` host event 에, 인덱스는
        /// 활성 포인터 보정에 쓴다. Core 는 `AppState::active_workspace` 를 모르므로
        /// 보정 자체는 cascade 몫이고(`AppState::fix_workspace_pointers_after_removal`),
        /// cascade 시점엔 workspace 가 이미 사라져 위치를 알 수 없어 여기 싣는다.
        workspace_purged: Option<(usize, u32)>,
        workspaces_now_empty: bool,
    },
    /// surface 변환 완료. `replaced=false` 면 surface 못 찾음 또는 변환 실패이고, 그때
    /// `failure` 가 실패를 낸 자리의 사유 문구다(예: 미등록 kind → registry 의
    /// `unknown surface kind: <kind>`). 성공이면 `None` 이다. forward 된 convert 는 이 문구를
    /// 원격 사유로 그대로 회신한다(`attach_runtime::execute_forwarded_structural_op`).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    SurfaceConverted {
        surface_id: u32,
        replaced: bool,
        failure: Option<String>,
    },
    /// surface 이동(replace) 완료 (T9). `moved=false` 면 self-ref / source 무효 /
    /// target 못 찾음 (no-op, 슬롯만 비움).
    ///
    /// `b_cleanup` 은 닫히는 target(B) 의 `(surface_id, scrollback_persist_id)` —
    /// cascade 가 `cleanup_surface`(PTY kill) + `surface.closed` host event 발화.
    /// 나머지 필드는 **source(A) 의 옛 자리** 가 sole 이라 구조적으로 닫힌
    /// tab/pane/workspace 의 cascade 정보 (split 안 이동이면 `Surface` level + 빈
    /// vec). A 의 surface 자체는 **cleanup 대상이 아니다**(이동이므로 살아있음).
    /// 의미상 `SurfaceClosed` 와 동일 cascade 를 재사용한다.
    MoveSurfaceApplied {
        moved: bool,
        b_cleanup: Option<(u32, Option<String>)>,
        cascade_level: CascadeLevel,
        closed_tab_ids: Vec<u32>,
        closed_pane_ids: Vec<u32>,
        /// `SurfaceClosed::workspace_purged` 와 같은 의미 — A 의 옛 자리가 workspace
        /// 째 사라진 경우 그 **(인덱스, id)**.
        workspace_purged: Option<(usize, u32)>,
        workspaces_now_empty: bool,
    },
    /// terminal send 완료. `sent=false` 면 surface 가 terminal 이 아니거나, 없거나,
    /// hard-occupied(attach 로 점유) — `hard_occupied` 로 그 중 어느 쪽인지
    /// 구분한다(Gate4 판단필요: 이 둘을 같은 "not found" 메시지로 뭉뚱그리면
    /// attach 로 점유된 — `list`류엔 여전히 보이는 — surface 를 존재하지 않는다고
    /// 오인하게 된다).
    SurfaceSent { sent: bool, hard_occupied: bool },
    /// terminal respawn 완료. `error` 가 Some 이면 spawn 실패 또는 surface
    /// 가 terminal 이 아님 — handler 가 invalid_params 반환.
    TerminalRespawned {
        surface_id: u32,
        error: Option<String>,
    },

    // ─── Notifications ───
    /// 알림 push 요청. cascade 가 라우팅 + store.add + host event enqueue.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    NotificationPushRequested {
        ws_id: u32,
        surface_id: u32,
        title: String,
        body: String,
        source: String,
    },
    /// 특정 알림 읽음 처리 요청.
    #[cfg(feature = "gui")]
    NotificationReadRequested { id: u64 },
    /// 모든 알림 읽음 처리 요청.
    #[cfg(feature = "gui")]
    AllNotificationsReadRequested,

    // ─── Surface lifecycle ───
    /// Surface 의 cwd 변경 알림. cascade 가 tab display name / layout dirty 갱신.
    #[cfg(feature = "gui")]
    SurfaceCwdChanged { surface_id: u32 },

    // ─── Terminal control ───
    /// Terminal read mark 설정 요청. cascade 가 surface 보유 engine 에 적용.
    TerminalMarkSet { surface_id: u32 },

    // ─── Surface completion (attention producer) ───
    /// Surface attention 신호 요청. cascade 가 surface 보유 engine 의
    /// `raise_attention(surface_id, kind)` + redraw.
    SurfaceCompletionRequested {
        surface_id: u32,
        kind: super::AttentionKind,
    },

    // ─── Surface attention 해제 (clear producer) ───
    /// Surface attention 해제 요청. cascade 가 surface 보유 engine 의
    /// `clear_attention(surface_id)` + redraw. `kind` 가 `Some` 이면 현재 기록된
    /// kind 가 일치할 때만 지운다(선택적 필터).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    SurfaceAttentionClearRequested {
        surface_id: u32,
        kind: Option<super::AttentionKind>,
    },

    // ─── Closed items ───
    /// closed_items pop + 복원 완료. cascade 가 (Workspace kind 인 경우)
    /// active_workspace 를 새 인덱스로 옮긴다.
    /// - `restored=false`: closed_items 가 비었거나 rebuild 실패.
    /// - `kind`: 어떤 종류가 복원되었는지 + cascade 가 알아야 할 인덱스.
    ClosedItemRestored { restored: bool, kind: RestoredKind },

    // ─── Terminal cascade — PTY emit 발화 ───
    /// PTY child process exit. cascade 가 hook 발화 + ProcessExited host event
    /// enqueue + closed_items snapshot 분류 + 후속 `DomainIntent::CloseSurface
    /// { save_snapshot: true }` 발행.
    TerminalProcessExited { surface_id: u32 },

    /// OSC 0/2 로 받은 terminal window title. cascade 가 SurfaceTitleChanged
    /// host event 발화 (plugin 호환) + 후속 `DomainIntent::UpdateTabName` 발행.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalTitleChanged { surface_id: u32, title: String },

    /// OSC 9 / OSC 99 / OSC 777 알림. cascade 가 settings.notification gate
    /// 적용 + `DomainIntent::PushNotification` 발행 + hook 발화.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalNotification {
        surface_id: u32,
        title: String,
        body: String,
    },

    /// Bell (\\a) 수신. cascade 가 settings.notification gate 적용 +
    /// `DomainIntent::PushNotification { title: t("notification.bell_title") }` 발행 + hook 발화.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalBellRing { surface_id: u32 },

    /// PTY 출력이 완성된 한 라인을 이뤘다. cascade 가 등록된
    /// `OutputMatch` 훅과 이 라인 텍스트를 비교해 발화한다. `has_output_match_hook`
    /// 로 게이트돼 있어 이 surface 에 `OutputMatch` 훅이 없으면 애초에 발행되지
    /// 않는다.
    TerminalOutputMatch { surface_id: u32, text: String },

    /// OSC 7 cwd 변경. gui cascade 는 후속 `DomainIntent::SurfaceCwdChanged` 를 발행하고,
    /// headless 는 PTY drain(`src/boot.rs`)이 `intent::headless::apply_terminal_cwd_changed`
    /// 로 같은 engine 갱신을 직접 한다.
    TerminalCwdChanged { surface_id: u32 },

    /// OSC 133 D phase — 셸 통합이 명령 완료 + exit code 를 보고했다. cascade
    /// 가 exit code 무관하게 항상 surface attention(kind=Completion) 을 발동하고
    /// (자동 경로), 동시에 `HookEvent::CommandCompleted(exit_code)` 로 훅도 발화한다
    /// (커스터마이즈 경로 — 두 경로는 상호 배타적이지 않다. 상세:
    /// `docs/features/surface-highlight/index.md`).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalCommandCompleted {
        surface_id: u32,
        exit_code: Option<i32>,
    },

    /// 이 surface 가 출력을 내고 있는데도 일정 시간 `PromptBoundary` 를 한 번도
    /// 못 받았다 — OSC 133 셸 통합 미설치로 추정. cascade 가 안내 배너를
    /// 1 회 띄운다(자동 조치 없음).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalShellIntegrationHint { surface_id: u32 },

    /// OSC 52 clipboard set. cascade 가 `toast.copied_osc52` 토스트만 발행한다.
    /// 시스템 clipboard 쓰기는 Core::process_pty_output 이 self.clipboard 로 직접 처리한다.
    /// `surface_id` 는 토스트를 Surface 스코프로 띄우기 위함 (호스트가 stamp 한 실제 sid).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "이 이벤트는 headless 에서도 발화하지만 그 빌드의 drain 에 cascade 가 \
                      없어(`intent::headless::handle_core_event` 의 마지막 갈래) 칸을 읽는 \
                      자리가 없다. 칸을 빼면 발화점이 정보를 잃으므로 남긴다 — 배선이 \
                      생기면 이 기대가 깨져 그 자리를 가리킨다"
        )
    )]
    TerminalClipboardSet { surface_id: u32 },

    /// `DomainIntent::UpdateTabName` 적용 결과. cascade 가 mark_dirty 만.
    /// `osc_title` 은 레이아웃 영속 대상 아님 — mark_layout_dirty 호출 안 함.
    #[cfg(any(feature = "gui", test))]
    TabNameUpdated {
        /// `apply_update_tab_name` 의 explicit_name 보존 분기 여부 — production
        /// cascade 는 mark_dirty 만 하고 참조하지 않는다. 테스트 전용 관측 계약
        /// (`non_focused_surface_title_does_not_change_tab_name` 등이 assert).
        #[allow(dead_code)]
        skipped_explicit: bool,
    },

    // ─── Layout persistence ───
    /// `SaveLayoutNow` 결과 알림 — 저장/skip(설정 off 또는 dirty 아님 + force=false)
    /// 여부와 무관하게 cascade 없음.
    #[cfg(any(feature = "gui", test))]
    LayoutSaved,

    /// `ApplyPendingLayoutRestore` 결과. `restored=true` 면 caller 가
    /// `active_workspace` 로 `state.switch_workspace` 수행. `restored=false` 면
    /// pending 없거나 schema 미스매치. cascade 없음 — caller 가 events 직접 검사.
    #[cfg(feature = "gui")]
    LayoutRestored {
        restored: bool,
        active_workspace: Option<usize>,
    },

    // ─── Plugin lifecycle ───
    /// Plugin process 가 spawn 되어 hello 까지 완료. cascade 가
    /// PendingHostEvent::PluginLoaded enqueue + plugin event_bus broadcast.
    #[cfg(feature = "gui")]
    PluginLoaded { plugin_id: String, version: String },

    /// Plugin 활성화 상태 변경 (enable/disable). PluginManager.config 변경
    /// 직후 발화. `enabled=false` 인 경우 cascade 가 `PluginUnloaded` 도 함께
    /// 발화 (옛 lifecycle.rs:317 의 was_running 분기를 cascade 가 흡수).
    PluginEnableToggled { plugin_id: String, enabled: bool },

    /// Plugin process 가 graceful shutdown 또는 abnormal terminate.
    /// `reason` 은 `LifecycleReason::{User, Ipc, Crash}` 3종. 본 substep 에서는
    /// (결정 §7.2) 항상 `User` — caller context 추적 단순화.
    PluginUnloaded {
        plugin_id: String,
        reason: tasty_plugin_protocol::events::LifecycleReason,
    },

    /// Plugin 실패 — spawn/runtime/pump 등 모든 error 통합. cascade(`cascade_plugin_error`)
    /// 는 완전히 구현돼 host event + event_bus broadcast 까지 연결돼 있으나, 이 variant
    /// 를 실제로 construct 하는 producer 가 아직 없다(범위 밖이라 미착수. plugin
    /// spawn/runtime/pump 실패 지점에서 이 event 를 발화하도록 배선하는 게 다음
    /// 단계).
    #[allow(dead_code)]
    PluginError {
        plugin_id: String,
        error_kind: String,
        message: String,
    },

    /// Plugin 의 surface_kind 가 registry 에 등록됨. hello 처리 시 매 kind 마다
    /// 발화. cascade 가 PendingHostEvent 로 라우팅 (외부 가시성). `rendering` 은
    /// "remote" / "host" / "webview" 중 하나.
    #[cfg(feature = "gui")]
    PluginSurfaceKindRegistered {
        plugin_id: String,
        kind: String,
        rendering: String,
    },

    /// Plugin install / remove / grant / revoke 완료. 정적 상태 변경 (config /
    /// packages / permissions). cascade 가 host event 라우팅.
    #[cfg(feature = "gui")]
    PluginRegistryChanged {
        plugin_id: String,
        change: PluginRegistryChange,
    },

    /// Plugin manifest 의 `[[contributes.window]]` 항목이 hello 시점에 등록됨.
    /// 1.0 에서는 *stub 통지* — 실 spawn handler 는 별도 영역. cascade 가 host
    /// event (`plugin.window_declared`) 만 발화.
    #[cfg(feature = "gui")]
    PluginWindowDeclared {
        plugin_id: String,
        window_id: String,
    },
}

/// `CoreEvent::PluginRegistryChanged` 의 변경 종류.
#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) enum PluginRegistryChange {
    Installed { version: String },
    Removed,
    PermissionGranted { permission: String },
    PermissionRevoked { permission: String },
}

/// `CoreEvent::ClosedItemRestored` 의 복원 결과 분류.
#[derive(Debug, Clone)]
pub(crate) enum RestoredKind {
    /// 비어있는 스택 또는 rebuild 실패.
    Nothing,
    /// Workspace 복원 — cascade 가 `state.active_workspace = new_ws_index` 적용.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "닫은 항목 복원은 headless 에서도 돌아 이 값을 싣지만, 그 값을 쓰는 \
                      것은 활성 워크스페이스 보정과 focus 이동이라 GUI 에만 있다"
        )
    )]
    Workspace { new_ws_index: usize },
    /// Surface 또는 Tab 이 기존 pane 의 tab 으로 attach 됨.
    /// cascade 가 별도 mutate 없이 mark_dirty 만 발화 (Core::apply 가 이미
    /// mark_layout_dirty 처리).
    TabIntoPane,
    /// Pane 이 현재 활성 워크스페이스의 split 트리에 재삽입됨. cascade 가
    /// 복원된 pane 으로 focus 를 옮긴다 — Workspace 복원이 `active_workspace`
    /// 를 옮기는 것과 같은 취지(사용자가 복원 결과를 바로 보게 한다).
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "닫은 항목 복원은 headless 에서도 돌아 이 값을 싣지만, 그 값을 쓰는 \
                      것은 활성 워크스페이스 보정과 focus 이동이라 GUI 에만 있다"
        )
    )]
    PaneIntoWorkspace { pane_id: u32 },
}

/// `CoreEvent::SurfaceClosed` 의 cascade 깊이 정보.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CascadeLevel {
    Surface,
    Tab,
    Pane,
    Workspace,
}

/// `Core::process_pty_output` 의 반환. PTY drain 의 부수효과를 *데이터로* 표현
/// — 호출자 (event_handler) 가 cascade dispatch + state queue 분배.
///
/// `events` 는 cascade dispatcher 가 처리할 CoreEvent.
#[derive(Debug, Default)]
pub(crate) struct ProcessPtyOutcome {
    pub events: Vec<CoreEvent>,
}
