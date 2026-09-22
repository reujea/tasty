//! IPC 엔진 핸들러가 **창 쪽에서 읽거나 갱신해야 하는 것** — 좁은 포트와 intent 출구.
//!
//! 엔진 핸들러는 창 상태(`AppState`)를 인자로 받지 않는다. 창에 닿아야 하는 일은 두 갈래다.
//!
//! - **창에 묻거나 창을 갱신하는 연산** — 대상 생략 시의 기본 워크스페이스, cwd 상속,
//!   워크스페이스 닫기·이동·생성 뒤의 포인터 보정, 호스트 이벤트 큐, preset 적용, 승인 팝업.
//!   그 연산만 [`IpcWindow`] 로 선언하고 창 쪽(`state::ipc_window`)이 구현한다. 구조 실행이
//!   부르는 창 연산은 이미 도메인 포트 [`CascadeWindow`] 에 있어 그것을 물려받는다.
//! - **UI intent 발화** — 핸들러는 intent 를 창 큐(`AppState::pending_intents`)에 직접 넣지 않고
//!   요청 하나의 [`IntentOutbox`] 에 넣는다. 진입점(`check_request` 의 게이트 · 라우터의
//!   `dispatch_routed` · `record_plugin_rss_samples`)이 요청이 끝날 때 그 출구를 창 큐 끝으로
//!   한 번에 옮긴다([`IpcWindow::enqueue_intents`]). 요청 하나 안의 적재 순서는 출구에 넣은
//!   순서 그대로이고, 게이트가 낸 것이 핸들러가 낸 것보다 먼저다
//!   (`handler/intent_order_tests.rs` 가 고정한다).
//!
//! 창 상태를 **실제로 조작하는** GUI·debug 전용 핸들러(popup · 배너 · 도구 메뉴 · 파일 선택기
//! · debug 주입 · `ui.state`)는 이 포트의 대상이 아니다 — 그것들은 라우터 진입점
//! (`handle_checked_request`)이 쥔 `AppState` 를 `EntryWindow` 를 거쳐 창 핸들러 라우터(`route_window_handler`)로
//! 그대로 받는다. 근거와 경계는
//! [ADR-0471](../../../docs/adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md).

use std::path::PathBuf;

use crate::core::CoreState;
use crate::core::cascade_window::CascadeWindow;
use crate::intent::DispatchedIntent;

/// 엔진 핸들러가 쓰는 창 연산. 메서드마다 `AppState` 의 같은 일로 한 줄 위임된다
/// (`state::ipc_window`) — 이 trait 은 핸들러가 무엇에 닿는지를 시그니처로 말하게 할 뿐
/// 동작을 바꾸지 않는다.
pub(crate) trait IpcWindow: CascadeWindow {
    /// 이 창에서 로컬 사용자가 보고 있는 워크스페이스의 index.
    ///
    /// 대상을 생략한 요청의 기본 워크스페이스 · 응답의 "활성" 표시 · 기록의 기본 귀속이
    /// 이 값을 읽는다. 포커스 독립성(원칙 3)과의 경계에 있는 읽기다 — 호출 자리마다 이 값을
    /// 남길지(명시 수단이 있거나 좌변이 없는 자리) 요청이 댄 대상에서 끌어낼지를
    /// [ADR-0533](../../../docs/adr/0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md)
    /// 이 자리별로 정했다(`approval.request` 는 `surface_id` 가 있으면 그 워크스페이스로 간다).
    fn active_workspace_index(&self) -> usize;

    /// 새 워크스페이스의 cwd 상속 원본 — 설정(`inherit_cwd`)과 이 창의 포커스 surface 를 본다.
    fn resolve_inherit_cwd(&self, engine: &CoreState) -> Option<PathBuf>;

    /// 워크스페이스 생성 뒤 창 쪽 cascade(호스트 이벤트 · 활성 전환 판정).
    fn cascade_workspace_created(
        &mut self,
        engine: &mut CoreState,
        origin: &crate::intent::IntentOrigin,
        window_id: u64,
        created: crate::app::dispatch_domain::WorkspaceCreatedCascade,
    );

    /// 워크스페이스 메타 갱신 뒤 창 쪽 cascade(이름이 바뀌었으면 호스트 이벤트).
    fn cascade_workspace_meta_updated(
        &mut self,
        workspace_id: u32,
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
    );

    /// 워크스페이스 하나를 닫고 창 쪽 자원과 활성 포인터를 정리한다. 닫았으면 `true`.
    fn close_workspace_at(
        &mut self,
        engine: &mut CoreState,
        ws_idx: usize,
        origin: crate::state::WorkspaceCloseOrigin,
    ) -> bool;

    /// 워크스페이스 순서 이동 뒤 활성 포인터를 따라 옮긴다.
    fn fix_workspace_pointers_after_move(&mut self, from: usize, to: usize);

    /// 호스트 이벤트(plugin event bus · hook 대기 task)를 이 창의 큐에 넣는다.
    fn push_host_event(&mut self, event: crate::state::PendingHostEvent);

    /// 이 창이 기억하는 최근 파일(최신순).
    fn recent_files(&self, kind: &str) -> Vec<String>;

    /// preset 을 이 창에 적용한다. 워크스페이스 preset 은 이 창의 워크스페이스 목록에 붙는다.
    fn apply_preset(
        &mut self,
        core: &crate::core::Core,
        engine: &mut CoreState,
        target: crate::intent::preset::PresetApplyTarget,
        options: crate::state::preset_apply::ApplyOptions,
    ) -> Result<crate::intent::preset::ApplyOutcome, crate::intent::preset::PresetMutationError>;

    /// 발행된 capability 승인 요청을 이 창의 승인 팝업 큐에 넣는다.
    #[cfg(feature = "gui")]
    fn enqueue_approval_popup(
        &mut self,
        engine: &mut CoreState,
        record: &tasty_approval::ApprovalRecord,
    );

    /// `plugin_id` 가 소유한 popup 인스턴스 `instance_id` 가 이 창에 열려 있고 사용자의
    /// 확정형 입력을 받았는가. plugin 이 자기 popup 안의 사용자 조작으로 host 를 부를 때
    /// 그 호출을 사용자 행동으로 칠지 정한다(ADR-0526).
    #[cfg(feature = "gui")]
    fn plugin_popup_user_activated(&self, plugin_id: &str, instance_id: u64) -> bool;

    /// 요청 하나가 모은 intent 를 이 창의 큐 끝에 순서대로 옮긴다. 진입점만 부른다.
    fn enqueue_intents(&mut self, intents: IntentOutbox);
}

/// 요청 하나가 발화한 UI intent 의 출구. 핸들러는 창 큐가 아니라 여기에 넣는다.
///
/// 출구는 진입점이 만들고 요청이 끝날 때 [`IpcWindow::enqueue_intents`] 로 비운다. 핸들러가
/// 출구에만 닿으면 "이 핸들러는 창 상태를 안 읽고 intent 만 낸다" 를 시그니처가 말한다.
#[derive(Default, Debug)]
pub(crate) struct IntentOutbox(Vec<DispatchedIntent>);

impl IntentOutbox {
    /// intent 하나를 출구 끝에 넣는다.
    pub(crate) fn push(&mut self, intent: DispatchedIntent) {
        self.0.push(intent);
    }

    /// 넣은 순서 그대로 꺼낸다.
    pub(crate) fn into_vec(self) -> Vec<DispatchedIntent> {
        self.0
    }

    /// 넣은 것이 없는가.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
