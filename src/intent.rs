// reason: from_*는 타입 변환 대신 요청 출처를 붙이는 빌더이므로 self를 받는다.
#![allow(clippy::wrong_self_convention)]

//! 호스트 내부 명령 큐.
//!
//! RequestContext::dispatch_intent로 넣은 명령을 App::dispatch_pending_intents가 꺼내
//! 각 도메인 핸들러에 전달한다. 호출자는 실행 결과를 기다리지 않는다.
//! 설계는 docs/design/flows/action-dispatch.md를 따른다.

#[cfg(all(test, feature = "gui"))]
mod apply_error_tests;
mod failure_feedback;
// 헤드리스용 큐 처리를 GUI 조합의 시험에서도 검증한다.
#[cfg(any(not(feature = "gui"), test))]
pub(crate) mod headless;
pub mod popup;
pub mod preset;
pub mod preset_capture;
pub mod rename;
#[cfg(all(debug_assertions, feature = "gui"))]
pub mod watch;

use crate::model::SplitDirection;
use crate::model::popup_kind::{PopupId, PopupScope};

pub use preset::ClonedPreset;
// 도메인도 origin을 사용하므로 정의를 core에 두고 여기서는 재수출한다.
#[cfg(any(feature = "gui", test))]
pub use crate::core::origin::UserSource;
pub use crate::core::origin::{AgentSource, IntentOrigin};

pub(crate) use failure_feedback::{FailureContext, report_apply_error};

/// 메인 루프가 처리할 때까지 RequestContext::pending_intents에 보관하는 명령.
#[derive(Debug, Clone)]
pub struct DispatchedIntent {
    /// Monotonic admission time; not serialized or replayed with journal commands.
    pub admitted: std::time::Instant,
    pub body: Intent,
    pub origin: IntentOrigin,
    /// 현재 제품 코드에서는 None이며 debug intent 로그만 이 값을 읽는다.
    /// Event Bus의 trace_id와는 별개다. IPC 요청 번호는
    /// [`tasty_ipc::server::RequestSeq`]를 사용한다.
    pub trace_id: Option<String>,
    /// 구조 변경의 최종 결과를 받을 요청의 run([`crate::request_runs`]). journal 완료나 적용 실패가
    /// 그 run을 끝낸다. 결과를 돌려받지 않는 명령은 None이다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "headless refuses the requests that wait for a final result"
        )
    )]
    pub completion: Option<u64>,
}

/// 호스트 내부 명령. UI·도메인 명령과 사용자 단축키용 명령을 같은 큐에 담는다.
// 이유: 단축키·메뉴 전용 variant는 헤드리스에서 생성하지 않지만 큐 처리에서 열거한다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui raises the user-shortcut intents, so headless never builds them"
    )
)]
/// Transient UI work; it is never serialized into a journal operation.
#[derive(Clone, Debug)]
pub enum CreateFollowup {
    Prompt { kind: String },
}

/// 크기 동기화 실패 배너의 동작. 대상은 mirror 워크스페이스 ID다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachSizeSyncAction {
    /// 실패한 요청을 다시 보낸다. 자동 재시도 없이 판정한다.
    Retry { workspace_id: u32 },
    /// 실패 목록을 비운다. 이후 새 실패는 다시 배너에 오른다.
    Dismiss { workspace_id: u32 },
}

/// 저장된 원격 행의 메뉴·팝업 동작.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedRowAction {
    /// 연결하지 않은 행을 사용자 요청으로 연결한다. 팝업의 Connect·Reconnect와 같다.
    Connect,
    /// 행을 이 Tasty에서 없앤다. 원격 workspace와 그 프로세스는 남는다.
    CloseLocally,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: 명령마다 Box를 할당하는 비용을 피한다
pub enum Intent {
    #[cfg(feature = "gui")]
    MouseCaptureHint {
        target: crate::runtime::surface_binding::SurfaceBinding,
        foreground_generation: u64,
        view: std::sync::Weak<()>,
    },
    #[cfg(feature = "gui")]
    RemoteBrowser(crate::app::remote_browser::BrowserRequest),
    /// mirror 크기 동기화 실패 배너의 버튼. App이 그 mirror 세션에 적용한다.
    #[cfg(feature = "gui")]
    AttachSizeSync(AttachSizeSyncAction),
    /// mirror 워크스페이스의 로컬 연결만 끊는다. 서버 워크스페이스와 프로세스는 유지한다.
    #[cfg(feature = "gui")]
    DisconnectMirror { workspace_id: u32 },
    /// 저장된 원격 행의 메뉴·팝업 동작. App이 그 행의 연결 상태에 적용한다.
    #[cfg(feature = "gui")]
    SavedRow {
        workspace_id: u32,
        action: SavedRowAction,
    },
    #[cfg(feature = "gui")]
    PatchSettings(crate::app::engine_action::SettingsPatch),
    /// 일반 명령. 일반 dispatch만 실행한다.
    #[cfg(feature = "gui")]
    Engine(crate::app::engine_action::EngineAction),
    /// 프레임 단계 효과. 그리는 창의 drain은 도달한 단계까지 적용하고, 일반 dispatch는 프레임 밖에서
    /// 만든 효과를 적용한다.
    #[cfg(feature = "gui")]
    Frame(crate::app::frame_effect::FrameEffect),
    #[cfg(feature = "gui")]
    ForwardMirror {
        op: tasty_ipc::stream::StructuralOp,
        close_focus_candidates: Vec<u32>,
    },
    #[cfg(feature = "gui")]
    RespondApproval {
        request_id: tasty_approval::ApprovalId,
        choice: String,
        comment: Option<String>,
    },
    #[cfg(feature = "gui")]
    Ui(UiIntent),
    /// 도메인 명령도 같은 큐에 넣고 AppServices::apply로 전달한다.
    Domain(crate::app::command::DomainIntent),

    /// 사용자 요청일 때만 적용 후 포커스를 옮긴다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    ApplyPreset {
        kind: tasty_presets::PresetKind,
        name: String,
        /// Workspace 프리셋의 소속 카테고리. None이면 normal이며 Tab/Pane에서는 무시한다.
        category: Option<crate::model::WorkspaceCategoryId>,
    },
    /// explicit_name을 우선 사용하고, 없으면 base_name으로 중복되지 않는 이름을 만든다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    CapturePreset {
        kind: tasty_presets::PresetKind,
        source: u32,
        presentation: crate::model::StructurePresentationSnapshot,
    },

    /// 포커스된 surface를 분할한다. 사용자 단축키용이며 IPC는 ID를 지정한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    SplitSurface { direction: SplitDirection },
    /// Terminal은 호스트 내장 종류이며 나머지는 등록된 kind를 사용한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    ConvertSurface {
        surface_id: u32,
        target: ConvertTarget,
    },

    /// 포커스된 pane에 탭을 추가한다. kind가 None이면 terminal을 사용한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    NewTab {
        kind: Option<String>,
        params: serde_json::Value,
    },

    /// Explicit pane selected by a native menu; completion belongs to the originating View.
    #[cfg(feature = "gui")]
    NewTabWithFollowup {
        pane_id: u32,
        followup: CreateFollowup,
    },

    /// 포커스된 pane을 분할하는 사용자 단축키 명령.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    SplitPane { direction: SplitDirection },

    /// Practice setup completion belongs to this original tutorial attempt.
    #[cfg(feature = "gui")]
    PrepareTutorial { ticket: std::sync::Weak<()> },

    /// kind가 None이면 terminal을 사용한다. 사용자 요청일 때만 새 워크스페이스를 활성화한다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    NewWorkspace {
        kind: Option<String>,
        params: serde_json::Value,
        /// 새 워크스페이스의 소속 카테고리. None이면 normal이다.
        category: Option<crate::model::WorkspaceCategoryId>,
    },

    /// 최근 닫은 항목을 포커스된 pane에 복원한다. 필요한 워크스페이스는 먼저 만든다.
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    RestoreClosedItem,

    /// 사용자가 이름 변경 팝업에 직접 입력한 이름. 적용 뒤 user_direct host 이벤트를 낸다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "View input constructs this intent; the shared adapter resolves it while headless IPC uses journal requests"
        )
    )]
    DirectRename(rename::DirectRename),
    #[cfg(feature = "gui")]
    CommitDivider(crate::state::layout_preview::DividerCommit),
}

/// 팝업과 테마 등 화면 상태를 바꾸는 명령.
// 이유: 팝업 입력은 GUI에서 발생하며 헤드리스에서는 해당 variant를 만들지 않는다.
#[cfg_attr(
    not(feature = "gui"),
    expect(dead_code, reason = "only user input in the gui raises a popup intent")
)]
#[derive(Debug, Clone)]
pub enum UiIntent {
    SetCategoryCollapsed {
        id: u32,
        collapsed: bool,
    },
    ToggleCategoryCollapsed {
        id: u32,
    },
    ToggleAllCategoriesCollapsed,
    OpenPopup {
        id: PopupId,
        mode: OpenPopupMode,
    },
    ClosePopup {
        id: PopupId,
    },
    /// 호스트가 확인한 사용자 입력으로 열린 passive 팝업에 포커스를 준다. 사용자 출처일 때만 동작한다.
    FocusPassivePopup {
        id: PopupId,
    },
    TogglePopup {
        id: PopupId,
        mode: OpenPopupMode,
    },
    /// 사용자가 고른 surface 에 새로고침 요청 신호를 보낸다. 대상과 kind 는 키를 누른 시점에 고정한다.
    /// 처리 시점에 같은 세대·kind 가 아니면 버린다(닫기·변환 뒤 다른 대상에 보내지 않는다).
    #[cfg(feature = "gui")]
    RequestSurfaceRefresh {
        target: crate::runtime::surface_binding::SurfaceBinding,
        kind: String,
        trigger: RefreshTrigger,
    },
}

/// 새로고침 요청을 만든 사용자 입력 경로. 이벤트 payload 의 `trigger` 로 나간다.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTrigger {
    Shortcut,
    Palette,
}

#[cfg(feature = "gui")]
impl From<UiIntent> for Intent {
    fn from(ui: UiIntent) -> Self {
        Intent::Ui(ui)
    }
}

#[cfg(feature = "gui")]
impl UiIntent {
    #[cfg(feature = "gui")]
    pub fn from_user_shortcut(self, id: &'static str) -> DispatchedIntent {
        Intent::Ui(self).from_user_shortcut(id)
    }

    #[cfg(feature = "gui")]
    pub fn from_user_menu(self, id: &'static str) -> DispatchedIntent {
        Intent::Ui(self).from_user_menu(id)
    }

    #[cfg(feature = "gui")]
    pub fn from_user_context_menu(self) -> DispatchedIntent {
        Intent::Ui(self).from_user_context_menu()
    }

    #[cfg(feature = "gui")]
    pub fn from_agent_ipc(self) -> DispatchedIntent {
        Intent::Ui(self).from_agent_ipc()
    }

    #[cfg(feature = "gui")]
    pub fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        Intent::Ui(self).from_agent_plugin(plugin_id)
    }

    // reason: 요청 출처별 빌더를 제공하지만 현재 CLI 호출부는 없다.
    #[allow(dead_code)]
    pub fn from_agent_cli(self) -> DispatchedIntent {
        Intent::Ui(self).from_agent_cli()
    }

    // reason: origin을 이어받는 빌더이며 현재 호출부는 없다.
    #[allow(dead_code)]
    pub fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        Intent::Ui(self).cascaded_from(parent)
    }
}

// 시스템 요청용 빌더는 DomainIntent에만 둔다.
impl crate::app::command::DomainIntent {
    #[cfg(feature = "gui")]
    pub(crate) fn from_user_shortcut(self, id: &'static str) -> DispatchedIntent {
        Intent::Domain(self).from_user_shortcut(id)
    }

    #[cfg(feature = "gui")]
    pub(crate) fn from_user_menu(self, id: &'static str) -> DispatchedIntent {
        Intent::Domain(self).from_user_menu(id)
    }

    #[cfg(feature = "gui")]
    pub(crate) fn from_user_context_menu(self) -> DispatchedIntent {
        Intent::Domain(self).from_user_context_menu()
    }

    pub(crate) fn from_agent_ipc(self) -> DispatchedIntent {
        Intent::Domain(self).from_agent_ipc()
    }

    // reason: 요청 출처별 빌더를 제공하지만 현재 플러그인 호출부는 없다.
    #[allow(dead_code)]
    pub(crate) fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        Intent::Domain(self).from_agent_plugin(plugin_id)
    }

    // reason: 요청 출처별 빌더를 제공하지만 현재 CLI 호출부는 없다.
    #[allow(dead_code)]
    pub(crate) fn from_agent_cli(self) -> DispatchedIntent {
        Intent::Domain(self).from_agent_cli()
    }

    pub(crate) fn from_system(self) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: Intent::Domain(self),
            origin: IntentOrigin::System,
            trace_id: None,
            completion: None,
        }
    }

    // reason: origin을 이어받는 빌더이며 현재 호출부는 없다.
    #[allow(dead_code)]
    pub(crate) fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        Intent::Domain(self).cascaded_from(parent)
    }
}

/// Surface 변환 대상. Terminal 외에는 surface_registry에 등록된 kind를 사용한다.
// 이유: 변환을 요청하는 팝업과 메뉴는 GUI에만 있다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui converts a surface through an intent"
    )
)]
#[derive(Debug, Clone)]
pub enum ConvertTarget {
    Terminal,
    Kind {
        /// 시작 cwd. None이면 기존 surface에서 구한다.
        cwd: Option<std::path::PathBuf>,
        kind: String,
        params: serde_json::Value,
    },
}

/// 팝업 위치와 포커스 정책.
// 이유: 팝업 열기는 GUI 입력에서만 요청한다.
#[cfg_attr(
    not(feature = "gui"),
    expect(dead_code, reason = "only user input in the gui opens a popup")
)]
#[derive(Debug, Clone)]
pub enum OpenPopupMode {
    /// 위치를 지정하지 않고 포커스도 옮기지 않는다.
    Default,
    /// 화면 중앙에 열고 포커스를 옮긴다.
    CenteredFocused,
    /// 지정한 범위의 중앙에 연다.
    WithScope(PopupScope),
    /// 지정한 범위의 상단에 연다.
    AtTopOfScope(PopupScope),
    /// 컨텍스트 메뉴에서 지정한 위치에 연다.
    #[cfg(feature = "gui")]
    AtFocused(egui::Pos2),
}

impl Intent {
    #[cfg(any(feature = "gui", test))]
    pub fn from_user_shortcut(self, id: &'static str) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::Shortcut(id),
            },
            trace_id: None,
            completion: None,
        }
    }

    #[cfg(feature = "gui")]
    pub fn from_user_menu(self, id: &'static str) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::Menu(id),
            },
            trace_id: None,
            completion: None,
        }
    }

    #[cfg(feature = "gui")]
    pub fn from_user_context_menu(self) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::ContextMenu,
            },
            trace_id: None,
            completion: None,
        }
    }

    pub fn from_agent_ipc(self) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Ipc,
            },
            trace_id: None,
            completion: None,
        }
    }

    pub fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Plugin(plugin_id.into()),
            },
            trace_id: None,
            completion: None,
        }
    }

    // reason: 요청 출처별 빌더를 제공하지만 현재 CLI 호출부는 없다.
    #[allow(dead_code)]
    pub fn from_agent_cli(self) -> DispatchedIntent {
        DispatchedIntent {
            admitted: std::time::Instant::now(),
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Cli,
            },
            trace_id: None,
            completion: None,
        }
    }

    /// 이전 명령의 origin과 trace_id를 그대로 이어받는다.
    // reason: 제품 코드에는 호출부가 없고 시험에서만 사용한다.
    #[allow(dead_code)]
    pub fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        DispatchedIntent {
            admitted: parent.admitted,
            body: self,
            origin: parent.origin.clone(),
            trace_id: parent.trace_id.clone(),
            // 결과는 원 요청 하나에만 기록한다.
            completion: None,
        }
    }
}

impl DispatchedIntent {
    /// 이 명령의 최종 결과를 `run`에 기록하게 한다.
    #[cfg(feature = "gui")]
    pub(crate) fn with_completion(mut self, run: Option<u64>) -> Self {
        self.completion = run;
        self
    }

    /// 로그를 연결할 trace_id를 지정한다. IPC 요청 번호와는 별개다.
    // reason: 제품 코드에는 호출부가 없고 시험에서만 사용한다.
    #[allow(dead_code)]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RequestContext;

    fn make_state() -> RequestContext {
        crate::state::tests::test_state().0
    }

    #[test]
    fn dispatch_intent_pushes_to_queue() {
        let mut state = make_state();
        state.dispatch_intent(Intent::RestoreClosedItem.from_user_shortcut("test"));
        assert_eq!(state.pending_intents.len(), 1);
    }

    #[test]
    fn take_pending_intents_clears_queue() {
        let mut state = make_state();
        state.dispatch_intent(Intent::RestoreClosedItem.from_user_shortcut("a"));
        state.dispatch_intent(Intent::RestoreClosedItem.from_user_shortcut("b"));
        let drained = state.take_pending_intents();
        assert_eq!(drained.len(), 2);
        assert!(state.pending_intents.is_empty());
    }

    #[test]
    fn origin_is_user_is_agent() {
        let user = Intent::RestoreClosedItem.from_user_shortcut("x");
        let agent = Intent::RestoreClosedItem.from_agent_ipc();
        assert!(user.origin.is_user());
        assert!(!user.origin.is_agent());
        assert!(agent.origin.is_agent());
        assert!(!agent.origin.is_user());
    }

    #[test]
    fn cascade_preserves_origin_and_trace_id() {
        let parent = Intent::RestoreClosedItem
            .from_user_shortcut("approve")
            .with_trace_id("t1");
        let child = Intent::RestoreClosedItem.cascaded_from(&parent);
        assert!(matches!(
            child.origin,
            IntentOrigin::User {
                source: UserSource::Shortcut("approve")
            }
        ));
        assert_eq!(child.trace_id.as_deref(), Some("t1"));
    }
}
