//! 라우터 진입점이 쥔 창 — 입구 **본문**이 창 상태를 이름으로 못 부르게 봉인한다.
//!
//! `handle_checked_request` 는 창의 소유자 자리라 `AppState` 를 받는다(ADR-0471 Decision 5).
//! 그러나 그 아래 입구 본문(`route_checked_request` · `dispatch_routed`)이 창에서 하는 일은
//! 셋뿐이다 — 엔진 라우터에 좁은 포트를 넘기고, 요청의 intent 출구를 창 큐로 옮기고, 창 상태
//! 자체가 대상인 창·debug 라우터에 창을 넘기는 것. 이 타입은 그 셋만 메서드로 열고 필드를 이
//! 모듈 밖에 숨긴다. 그래서 입구 본문에 popup 을 열거나 `active_workspace` 를 옮기는 코드가
//! 들어오면 컴파일이 깨진다(원칙 2.1 ① · 2.3).
//!
//! `AppState` 를 **돌려주는** 접근자는 두지 않는다 — 그것은 ADR-0471 이 기각한 "포트를 우회하는
//! 문" 이 된다. 창 전체가 필요한 라우터 호출은 메서드 안에서 끝난다.
//!
//! debug 라우터로 가는 문은 debug 빌드에만 있어야 하므로 별도 파일(`entry_window_debug.rs`)에
//! 둔다. 그 파일은 필드에 닿아야 해서 이 모듈의 **자식**이어야 하고, 그래서 cfg 를 이 파일의
//! 선언이 아니라 그 파일 머리의 `#![cfg(debug_assertions)]` 로 건다 — 핸들러 파일 안에
//! debug 전용 item 을 두지 않는다는 격리 규칙(`docs/dev-guide/debug-ipc.md`, 가드
//! `src/source_guards/debug_handler_isolation.rs`)이 이 형태를 파일 단위 격리로 인정한다.

#[path = "entry_window_debug.rs"]
mod debug;

use crate::ipc::window_port::IpcWindow;
use crate::state::AppState;

#[cfg(feature = "gui")]
use crate::ipc::caller::CallerContext;
#[cfg(feature = "gui")]
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

/// 라우터 진입점이 요청 하나 동안 쥐는 창.
pub(crate) struct EntryWindow<'a> {
    state: &'a mut AppState,
}

impl<'a> EntryWindow<'a> {
    pub(crate) fn new(state: &'a mut AppState) -> Self {
        Self { state }
    }

    /// 엔진 라우터·출구 이동이 보는 좁은 창.
    pub(crate) fn port(&mut self) -> &mut dyn IpcWindow {
        &mut *self.state
    }

    /// 창 상태 자체가 대상인 gui 핸들러의 유일한 문. 누가 부를 수 있는지는 팔마다 핸들러가
    /// 판정하고, 그 명부는 `window_router_caller_tests.rs` 가 대조하지만 그 대조도 닿지 않는
    /// 자리가 있다(그 모듈 doc 의 "한계") — 새 팔은 호출자 판정을 직접 확인하라.
    #[cfg(feature = "gui")]
    pub(crate) fn route_window(
        &mut self,
        engine: &mut crate::core::CoreState,
        caller: &CallerContext,
        request: &JsonRpcRequest,
        id: serde_json::Value,
    ) -> Option<JsonRpcResponse> {
        super::route_window_handler(self.state, engine, caller, request, id)
    }
}
