//! 이 시험 바이너리가 쓰는 plugin namespace 소유 표.
//!
//! 소유 표는 **프로세스당 1 회 설치**된다(`tasty_ipc::method_meta::install_namespace_table`,
//! 운영에서는 `PluginManager` 가 넘긴다). 이 바이너리에는 그 매니저가 없으므로 여기서 자기
//! 표를 한 번 설치하고, prefix 가 등록된 상태를 재야 하는 시험들이 그 핸들을 **함께** 쓴다.
//! 시험마다 따로 설치하면 두 번째부터 `false` 라 넣은 prefix 가 해소에 안 쓰인다.
//!
//! 표 하나를 바이너리 전체가 나눠 쓰므로 시험마다 **서로 다른 plugin id 와 prefix** 를 쓰고,
//! 끝나면 자기 plugin id 로 지운다.

use std::sync::{Arc, OnceLock, RwLock};

use tasty_ipc::ipc_namespace::IpcNamespaceRegistry;

pub(crate) fn installed_test_table() -> &'static Arc<RwLock<IpcNamespaceRegistry>> {
    static TABLE: OnceLock<Arc<RwLock<IpcNamespaceRegistry>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let table = Arc::new(RwLock::new(IpcNamespaceRegistry::new()));
        assert!(
            tasty_ipc::method_meta::install_namespace_table(Arc::clone(&table)),
            "다른 곳이 먼저 표를 설치했다 — 그러면 이 테스트가 넣는 prefix 는 해소에 안 쓰인다"
        );
        table
    })
}
