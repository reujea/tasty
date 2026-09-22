// 본 모듈의 `from_*` 메서드는 `From` trait 변환이 아니라 *intent 의 dispatch
// source 부착* 의미 (예: `intent.from_user_shortcut(id)` = "이 intent 는 사용자
// 단축키로 발화되었다고 표시"). 따라서 `self` 를 받는 것이 의도된 형태.
#![allow(clippy::wrong_self_convention)]

//! Host-internal action dispatch (Intent 큐).
//!
//! 설계: `docs/design/flows/action-dispatch.md`.
//!
//! 발화자는 `AppState::dispatch_intent`로 `DispatchedIntent`를 push만 한다.
//! 메인 루프의 `App::dispatch_pending_intents`가 drain 하여 도메인별 핸들러
//! (`intent::popup`, `intent::preset`, ...)로 분기한다. fire-and-forget.
//!
//! 본 모듈의 `from_*` 메서드는 `From` trait 변환이 아니라 *intent 의 dispatch
//! source 부착* 의미. `self` 를 받는 것이 의도된 형태이며
//! `clippy::wrong_self_convention` 은 모듈 단위로 허용한다.

#[cfg(all(test, feature = "gui"))]
mod apply_error_tests;
pub mod closed_item;
pub(crate) mod headless;
pub mod pane;
pub mod popup;
pub mod preset;
pub mod preset_capture;
pub mod surface;
pub mod tab;
// 발화 로그 — 부르는 자리가 GUI 메인 루프의 intent drain 뿐이다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub mod watch;
pub mod workspace;

use crate::model::SplitDirection;
use crate::model::popup_kind::{PopupId, PopupScope};

pub use preset::ClonedPreset;
// 발화 주체는 도메인 실행(`core::structural_exec` 등)도 읽으므로 정의는 `core` 에 있다 —
// 도메인이 이 GUI 큐 모듈을 거꾸로 부르지 않게 하려는 것이다. 기존 경로를 잇는다.
#[cfg(any(feature = "gui", test))]
pub use crate::core::origin::UserSource;
pub use crate::core::origin::{AgentSource, IntentOrigin};

/// `Core::apply` 가 반환한 에러를 도메인 핸들러가 공통 처리한다. mirror(원격 attach
/// client) 워크스페이스에서 구조 변경을 시도해 거부된 경우
/// ([`crate::core::MirrorStructuralBlocked`]) 사용자에게 차단 toast 를 띄우고,
/// 그 외 에러는 `warn` 로그를 남긴다. `label` 은 로그용 컨텍스트(예: "SplitSurface").
///
/// **toast 는 사용자 origin 에서만 난다.** 에이전트 origin 의 차단은 사용자 발화와 같은
/// 조건이어도 `warn` 로그로만 남긴다 — 에이전트 행동의 부수효과가 사용자 시각 상태에
/// 닿지 않게 하는 것이다(identity 원칙 1, `docs/design/systems/toast.md` "트리거 정책").
/// forward 로 큐잉된 에이전트 op 는 원격 실패 회신도 toast 가 아니라 로그로 가도록
/// 여기서 표시한다([`crate::core::mark_last_forward_agent_origin`]).
pub fn report_apply_error(
    state: &mut crate::state::AppState,
    engine: &mut crate::core::CoreState,
    origin: &IntentOrigin,
    label: &str,
    err: &anyhow::Error,
) {
    crate::core::mark_last_forward_agent_origin(engine, err, origin);
    if let Some(blocked) = err.downcast_ref::<crate::core::MirrorStructuralBlocked>() {
        // forward 로 큐잉된 op 는 원격 실행 결과가 UX 를 결정한다 — 여기서 차단 toast 를
        // 띄우지 않는다(성공 무음, 실패 시 App drain 이 forward 실패 toast — 에이전트 op 는
        // 로그). forward 대상이 아닌 op(mirror↔local 경계를 넘는 move-surface, anchor 를
        // 못 찾은 op)만 차단 신호를 낸다.
        if blocked.forwarded {
            return;
        }
        if !origin.is_user() {
            tracing::warn!("{label} blocked on a mirror workspace (agent origin, no toast): {err}");
            return;
        }
        #[cfg(feature = "gui")]
        state.toasts.push(
            crate::i18n::t("attach.toast.mirror_structural_blocked"),
            crate::model::toast_kind::ToastKind::Warning,
            crate::model::toast_kind::ToastScope::Window,
        );
    } else {
        tracing::warn!("{label} failed: {err}");
    }
}

/// 발화된 Intent. 메인 루프 drain 까지 `AppState::pending_intents` 에 머문다.
#[derive(Debug, Clone)]
pub struct DispatchedIntent {
    pub body: Intent,
    pub origin: IntentOrigin,
    /// 비어 있는 자리다 — **아무도 값을 발급하지 않는다.** 모든 생성자가 `None` 을 넣고, 값을
    /// 넣는 두 helper([`DispatchedIntent::with_trace_id`] · [`Intent::cascaded_from`])는 비-테스트
    /// 호출처가 없다. 새로 발급하는 bridge 도 없다. 읽는 자리는 debug 빌드의 intent watch 로그
    /// 하나뿐이고 거기서도 늘 `None` 이다.
    ///
    /// 이름이 같지만 **Event Bus envelope 의 `trace_id` 와 무관하고**, IPC 요청을 가리키는 값도
    /// 아니다 — IPC 요청 하나를 가리키는 값은 호스트가 발급하는
    /// [`tasty_ipc::server::RequestSeq`] 다(ADR-0436). 칸을 지우지 않고 남긴 이유도 그 ADR 에 있다.
    pub trace_id: Option<String>,
}

/// 호스트 내부 명령. flat enum — variant 가 늘어나도 nested 하지 않는다.
///
/// **분류축** (D.3.I — `intent-ui-vs-domain.md`):
/// - `Ui(UiIntent)`: 사용자 시각 상태 변경 (popup open/close/toggle). headless
///   빌드에서는 컴파일 타임에 사라진다 (Phase E).
/// - 그 외 variant: Domain Intent — 영속 도메인 mutate. headless 빌드에서도 동작.
///
/// release 빌드에서 *시스템/Core/Domain handler 가 자동으로 `Ui` variant 를
/// 발화* 하는 것은 금지된다 (`docs/design/systems/popup.md` "Popup 발화 정책").
/// debug 빌드의 `debug.popup.*` IPC 만 예외.
// 이유: `Ui`·`Domain` 을 뺀 variant 를 만드는 자리(단축키·메뉴·우클릭)가 GUI 뿐이라 headless 에서
// 만들어지지 않는다. 열거와 그 match 는 headless 의 intent drain 도 컴파일한다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "only the gui raises the user-shortcut intents, so headless never builds them"
    )
)]
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // reason: hot intent queue 에 Box 화 시 alloc 비용 큼
pub enum Intent {
    /// UI Intent (popup 발화). 별 enum `UiIntent` 로 분리되어 분류축이 명시된다.
    Ui(UiIntent),
    /// Domain Intent (영속 도메인 mutate). `crate::core::intent::DomainIntent` 를
    /// 래핑하므로 같은 큐 (`pending_intents`) 로 발화 가능. dispatch_one_intent
    /// 에서 `core.apply` 경로로 분기된다.
    ///
    /// 마이그레이션 진행: 현재 도메인 variant (ApplyPreset / SavePreset / ...)
    /// 가 점진적으로 `DomainIntent` 안으로 흡수될 예정.
    Domain(crate::core::intent::DomainIntent),

    // ---- Preset 도메인 ----
    /// Preset 적용. focus 정책은 origin 으로 자동 분기 (User=true, Agent=false).
    ApplyPreset {
        kind: tasty_presets::PresetKind,
        name: String,
        /// Workspace preset 적용 시 소속시킬 카테고리. `None` 이면 normal(기본).
        /// 카테고리 헤더 우클릭 메뉴의 "프리셋으로부터 워크스페이스 생성" 이 그
        /// 카테고리 id 를 실어 보낸다. Tab/Pane preset 에는 의미 없음(무시).
        category: Option<crate::model::WorkspaceCategoryId>,
    },
    /// Preset 저장. `explicit_name` 우선, 없으면 `base_name` 으로 `store.unique_name`.
    /// User origin (우클릭) 은 보통 explicit_name=None + overwrite=false,
    /// Agent origin (IPC) 은 explicit_name=Some + overwrite 명시.
    SavePreset {
        base_name: String,
        explicit_name: Option<String>,
        overwrite: bool,
        preset: ClonedPreset,
    },

    // ---- Surface 도메인 ----
    /// focused surface 를 split. focused 의존이므로 사용자 단축키 전용 (CLI/IPC 미노출).
    SplitSurface { direction: SplitDirection },
    /// Surface 의 kind 변환. Terminal 은 host 내장, 그 외는 plugin 등록 kind.
    ConvertSurface {
        surface_id: u32,
        target: ConvertTarget,
    },

    // ---- Tab 도메인 ----
    /// 새 탭 추가. `kind` None 이면 "terminal" fallback.
    /// focused pane 에 추가 (사용자 동작). ID 명시 경로는 IPC handler 가 직접 처리.
    NewTab {
        kind: Option<String>,
        params: serde_json::Value,
    },

    // ---- Pane 도메인 ----
    /// focused pane 을 split. 사용자 단축키 전용 (focused 의존).
    /// S3=B: ratio / focus 변경 API 는 Intent 미마이그레이션.
    SplitPane { direction: SplitDirection },

    // ---- Workspace 도메인 ----
    /// 새 워크스페이스 생성. `kind` None 이면 "terminal" fallback + active 전환
    /// (사용자 동작 경로). 명시 kind 지정 시 background 경로 (active 전환 없음).
    /// IPC `workspace.create` 는 sync return contract 가 필요하므로 직접 호출 유지.
    /// W1=B: ActivateWorkspace 는 focus 독립성 원칙으로 Intent 미마이그레이션.
    NewWorkspace {
        kind: Option<String>,
        params: serde_json::Value,
        /// 생성 시점 카테고리 소속. `None` 이면 normal(기본). 레일 카테고리 팝업의
        /// "Add workspace" 가 해당 카테고리 id 를 실어 보낸다.
        category: Option<crate::model::WorkspaceCategoryId>,
    },

    // ---- Closed items 도메인 ----
    /// closed_items 스택 top 복원. focused pane 의존 (사용자 단축키 전용).
    /// handler 가 focused pane / workspace 비어있음 사전처리 후 DomainIntent 발화.
    RestoreClosedItem,
}

/// UI Intent — 사용자 시각 상태 변경. release 표면에서는 사용자 행동 (단축키 /
/// 마우스 / 메뉴) 에서만 발화되며, 자동 발화는 금지된다 (`popup-system.md`,
/// `toast-system.md`, `debug-ipc.md` 의 자매 정책 — `intent-ui-vs-domain.md` 2절).
///
/// Phase E 의 headless 빌드 (`--no-default-features` 또는 `feature = "gui"` off)
/// 에서는 본 enum 자체가 컴파일 타임에 사라질 예정 — 그때 `Intent::Ui` variant
/// 도 `#[cfg(feature = "gui")]` 가드된다. 본 commit 에서는 분류축 표시 + builder
/// 도입만, cfg 가드는 후속.
// 이유: popup 을 여닫는 발화가 사용자 입력(GUI)뿐이라 headless 에서 variant 가 만들어지지 않는다.
#[cfg_attr(
    not(feature = "gui"),
    expect(dead_code, reason = "only user input in the gui raises a popup intent")
)]
#[derive(Debug, Clone)]
pub enum UiIntent {
    /// popup 열기.
    OpenPopup { id: PopupId, mode: OpenPopupMode },
    /// popup 닫기.
    ClosePopup { id: PopupId },
    /// popup toggle (열려있으면 닫고, 닫혀있으면 열기).
    TogglePopup { id: PopupId, mode: OpenPopupMode },
    /// Theme 색상 또는 host UI zoom 배율이 바뀌었다. 모든 윈도우 (main + modal)
    /// 의 GpuState 가 전역 `Theme` 인스턴스를 재빌드 후 egui ctx 에 reapply
    /// 해야 한다. dispatcher 가 fan-out 처리.
    AppearanceChanged,
}

impl From<UiIntent> for Intent {
    fn from(ui: UiIntent) -> Self {
        Intent::Ui(ui)
    }
}

/// `UiIntent` 발화 ergonomics — `Intent` 의 builder 들을 그대로 갖춰 호출처가
/// `UiIntent::OpenPopup{...}.from_user_shortcut(...)` 형태로 발화할 수 있게 한다.
///
/// origin 분기 builder set — agent plugin / cli / cascade 발화 경로가 wiring
/// 전이라 일부 메서드 dead. 외부 호출처 추가 시 일관 set 이 필요하므로 보존.
impl UiIntent {
    #[cfg(feature = "gui")]
    pub fn from_user_shortcut(self, id: &'static str) -> DispatchedIntent {
        Intent::Ui(self).from_user_shortcut(id)
    }

    #[cfg(any(feature = "gui", test))]
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

    /// agent plugin 발화 — `file_picker.trigger`(ADR-0058)가 실사용처.
    #[cfg(feature = "gui")]
    pub fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        Intent::Ui(self).from_agent_plugin(plugin_id)
    }

    /// agent CLI 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub fn from_agent_cli(self) -> DispatchedIntent {
        Intent::Ui(self).from_agent_cli()
    }

    /// cascade 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        Intent::Ui(self).cascaded_from(parent)
    }
}

/// `DomainIntent` 발화 ergonomics — `UiIntent` 와 동일 패턴. 단 `from_system()`
/// 은 *Domain 한정* 으로 본 impl 에만 존재한다 — UI Intent 의 자동 발화 차단.
///
/// origin 분기 builder set — context_menu / agent_plugin / agent_cli / cascade
/// 발화 경로가 wiring 전이라 일부 메서드 dead. 외부 호출처 추가 시 일관 set 이
/// 필요하므로 보존.
impl crate::core::intent::DomainIntent {
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

    /// agent plugin 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub(crate) fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        Intent::Domain(self).from_agent_plugin(plugin_id)
    }

    /// agent CLI 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub(crate) fn from_agent_cli(self) -> DispatchedIntent {
        Intent::Domain(self).from_agent_cli()
    }

    /// 시스템 내부 cascade 발화 — PTY escape sequence 가 trigger 한 자동 cascade
    /// 등. UI Intent 의 system 발화는 type-level 로 차단되므로 본 method 는
    /// `DomainIntent` 에만 존재한다.
    pub(crate) fn from_system(self) -> DispatchedIntent {
        DispatchedIntent {
            body: Intent::Domain(self),
            origin: IntentOrigin::System,
            trace_id: None,
        }
    }

    /// cascade 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub(crate) fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        Intent::Domain(self).cascaded_from(parent)
    }
}

/// Surface 변환 타깃. Terminal 은 host 내장 special case, 나머지는 surface_registry
/// 의 kind 로 통합. plugin 이 등록한 kind 도 모두 이 경로로 처리한다.
///
/// `Kind` 의 `cwd` 는 호출자가 명시 또는 None (handler 가 source surface 에서 resolve).
// 이유: surface 변환을 발화하는 자리(변환 입력 popup·메뉴)가 GUI 뿐이다.
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
        /// 변환 대상의 시작 cwd. None 이면 intent handler 가 source surface 로부터 resolve.
        cwd: Option<std::path::PathBuf>,
        kind: String,
        params: serde_json::Value,
    },
}

/// popup open 위치/포커스 정책.
// 이유: `UiIntent` 와 같다 — popup 을 여는 발화가 GUI 뿐이다.
#[cfg_attr(
    not(feature = "gui"),
    expect(dead_code, reason = "only user input in the gui opens a popup")
)]
#[derive(Debug, Clone)]
pub enum OpenPopupMode {
    /// 위치 자유, focus 없음.
    Default,
    /// 화면 중앙 + (user origin 이면) focus.
    CenteredFocused,
    /// 특정 scope rect 기준 센터링.
    WithScope(PopupScope),
    /// scope 상단 정렬.
    AtTopOfScope(PopupScope),
    /// 지정 위치 (context menu). egui::Pos2 — gui-only.
    #[cfg(feature = "gui")]
    AtFocused(egui::Pos2),
}

/// 발화 ergonomics. `UiIntent::OpenPopup { ... }.from_user_shortcut("id")` 또는
/// 도메인 variant 에서 `Intent::ApplyPreset { ... }.from_user_menu("id")` 형태.
///
/// origin 분기 builder set — agent plugin / cli / cascade 발화 경로가 wiring
/// 전이라 일부 메서드 dead. 외부 호출처 추가 시 일관 set 이 필요하므로 보존.
impl Intent {
    #[cfg(any(feature = "gui", test))]
    pub fn from_user_shortcut(self, id: &'static str) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::Shortcut(id),
            },
            trace_id: None,
        }
    }

    #[cfg(any(feature = "gui", test))]
    pub fn from_user_menu(self, id: &'static str) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::Menu(id),
            },
            trace_id: None,
        }
    }

    #[cfg(feature = "gui")]
    pub fn from_user_context_menu(self) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::User {
                source: UserSource::ContextMenu,
            },
            trace_id: None,
        }
    }

    pub fn from_agent_ipc(self) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Ipc,
            },
            trace_id: None,
        }
    }

    /// agent plugin 발화 — `file_picker.trigger`(ADR-0058)가 실사용처.
    pub fn from_agent_plugin(self, plugin_id: impl Into<String>) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Plugin(plugin_id.into()),
            },
            trace_id: None,
        }
    }

    /// agent CLI 발화 경로 wiring 전 — 실사용처 없음.
    #[allow(dead_code)]
    pub fn from_agent_cli(self) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: IntentOrigin::Agent {
                source: AgentSource::Cli,
            },
            trace_id: None,
        }
    }

    /// cascade: 직전 Intent 의 origin 을 명시적으로 전파. `trace_id` 도 그대로.
    /// 비-테스트 호출처 없음 — cfg(test) 에서만 직접 호출됨.
    #[allow(dead_code)]
    pub fn cascaded_from(self, parent: &DispatchedIntent) -> DispatchedIntent {
        DispatchedIntent {
            body: self,
            origin: parent.origin.clone(),
            trace_id: parent.trace_id.clone(),
        }
    }
}

impl DispatchedIntent {
    /// `trace_id` 를 명시 지정한다. 비-테스트 호출처가 없다 — 이 값을 발급하는 IPC 핸들러는
    /// 없고, IPC 요청의 호스트 번호는 이 칸이 아니라 [`tasty_ipc::server::RequestSeq`] 다
    /// (ADR-0436).
    #[allow(dead_code)]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn make_state() -> AppState {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = crate::core::CoreState::new(80, 24, waker).unwrap();
        let preset_store = std::sync::Arc::new(std::sync::Mutex::new(
            tasty_presets::PresetStore::load_default(),
        ));
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::testing::InMemoryStorage::new(),
            ));
        AppState::new(&mut engine, preset_store, memory)
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
    fn appearance_changed_intent_pushes_to_queue() {
        let mut state = make_state();
        state.dispatch_intent(
            UiIntent::AppearanceChanged.from_user_menu("settings.appearance.changed"),
        );
        assert_eq!(state.pending_intents.len(), 1);
        let drained = state.take_pending_intents();
        assert!(matches!(
            drained[0].body,
            Intent::Ui(UiIntent::AppearanceChanged)
        ));
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
