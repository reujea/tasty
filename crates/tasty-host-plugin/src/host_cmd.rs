//! `register_remote_kind`의 create/restore 클로저가 PluginManager에게 보내는 명령.
//!
//! 새 RemoteSurface가 만들어지면 해당 surface의 `Arc<Mutex>` 핸들 묶음을 manager에
//! 전달하여 manager가 plugin과의 메시지 흐름에서 이 surface를 추적할 수 있도록 한다.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// `RemoteSurface`의 내부 상태에 manager가 외부에서 접근하기 위한 핸들.
#[derive(Clone)]
pub struct SurfaceHandles {
    pub display_name: Arc<Mutex<String>>,
    /// plugin 이 `SurfaceResult.snapshot` 으로 piggyback 한 영속화용 데이터.
    /// 매 응답마다 manager 가 최신값으로 갱신하며, `SavedLayout::capture` 시
    /// `registry.get(kind).snapshot(surface)` 가 이 값을 읽어 disk 에 저장.
    pub snapshot_cache: Arc<Mutex<Option<Value>>>,
}

/// registry create/restore closure가 manager에게 보내는 명령.
pub enum HostCmd {
    RemoteSurfaceCreated {
        surface_id: u32,
        plugin_id: String,
        kind: String,
        /// 호스트가 carry 한 시작 cwd. Surface cwd invariant —
        /// `docs/design/policies/cwd.md#surface-cwd-invariant`.
        cwd: Option<PathBuf>,
        params: Value,
        handles: SurfaceHandles,
    },
    RemoteSurfaceRestored {
        surface_id: u32,
        plugin_id: String,
        kind: String,
        data: Value,
        handles: SurfaceHandles,
    },
}
