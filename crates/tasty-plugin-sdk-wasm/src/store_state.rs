//! wasmtime Store 에 보관하는 host 측 상태.
//!
//! WASI Preview 2 의 `WasiCtx` + 본 POC 의 `HostBridge` 를 함께 보관.
//! linker 의 closure 가 `store.data_mut()` 로 접근.

use std::sync::Arc;

use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

use crate::bridge::HostBridge;

pub struct HostState {
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub bridge: Arc<dyn HostBridge + Send + Sync>,
}

impl HostState {
    pub fn new(bridge: Arc<dyn HostBridge + Send + Sync>) -> Self {
        // 표준 입출력을 상속하고 디렉터리는 preopen하지 않는다.
        // 그 밖의 WASI 기능은 이 버전의 WasiCtx 기본 설정을 사용한다.
        let wasi = WasiCtx::builder().inherit_stdio().build();
        Self {
            wasi,
            table: ResourceTable::new(),
            bridge,
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}
