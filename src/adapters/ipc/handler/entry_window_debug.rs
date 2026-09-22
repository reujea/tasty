//! [`EntryWindow`](super::EntryWindow) 의 debug 라우터 문. 파일 전체가
//! `#![cfg(debug_assertions)]` 라 release 바이너리에는 이 문 자체가 없다.
#![cfg(debug_assertions)]

use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

impl super::EntryWindow<'_> {
    /// debug 핸들러(주입 · popup 강제 조작 · `ui.state`)에 창을 넘긴다.
    pub(crate) fn route_debug(
        &mut self,
        engine: &mut crate::core::CoreState,
        request: &JsonRpcRequest,
        id: serde_json::Value,
    ) -> Option<JsonRpcResponse> {
        super::super::route_debug_handler(self.state, engine, request, id)
    }
}
