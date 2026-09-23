use std::path::PathBuf;

use super::SurfaceId;
use super::surface_trait::Surface;
use super::terminal_surface::{Deferred, DeferredPlugin, DeferredSpawn};

/// 빈 surface placeholder. 일반적으로는 convert 버튼만 보여주는 비활성 패널이지만,
/// `deferred`가 Some일 때는 layout 복원 직후 아직 실제화되지 않은 자리를 차지하는
/// placeholder다. 실제화 대상은 두 가지다([`Deferred`]):
/// - `Deferred::Terminal`: PTY가 아직 spawn되지 않은 터미널 자리.
/// - `Deferred::Plugin`: plugin kind가 아직 `SurfaceKindRegistry`에 없는(부팅 창)
///   non-terminal surface 자리. reify가 kind 등록을 기다렸다가 복원한다.
pub struct EmptySurface {
    pub id: SurfaceId,
    /// 실제화 대기 파라미터. Some이면 이 surface는 reify 시 실제 surface로 교체될
    /// 자리표시자다. terminal과 plugin이 동시에 될 수는 없어 enum 하나로 담는다.
    pub deferred: Option<Deferred>,
    /// 연속 실제화 실패 횟수. reify 가 매 프레임(~60fps) 재시도하므로, 복원된
    /// layout 의 shell 바이너리가 영구히 없거나 plugin kind 가 영영 안 오는 등 영구
    /// 실패 시 폭주를 막기 위해 실패할 때마다 +1 하고, 상한에 도달하면 재시도를
    /// 멈춘다 (transient 실패는 상한 전에 성공해 0 으로 의미를 잃는다).
    pub spawn_attempts: u32,
    /// 호스트가 carry 한 시작 cwd. fresh empty 면 None — Surface cwd invariant
    /// (`docs/design/policies/cwd.md#surface-cwd-invariant`) 에 따라 다음 변환 시 후보로 사용.
    pub cwd: Option<PathBuf>,
}

impl EmptySurface {
    pub fn new(id: SurfaceId) -> Self {
        Self {
            id,
            deferred: None,
            spawn_attempts: 0,
            cwd: None,
        }
    }

    /// Deferred PTY spawn 파라미터를 가진 terminal placeholder를 생성.
    pub fn new_deferred(id: SurfaceId, spawn: DeferredSpawn) -> Self {
        Self {
            id,
            deferred: Some(Deferred::Terminal(spawn)),
            spawn_attempts: 0,
            cwd: None,
        }
    }

    /// plugin kind registry 등록을 기다리는 non-terminal placeholder를 생성.
    pub fn new_deferred_plugin(id: SurfaceId, plugin: DeferredPlugin) -> Self {
        Self {
            id,
            deferred: Some(Deferred::Plugin(plugin)),
            spawn_attempts: 0,
            cwd: None,
        }
    }

    /// 호스트가 carry 한 cwd 를 부여 (builder).
    pub fn with_cwd(mut self, cwd: Option<PathBuf>) -> Self {
        self.cwd = cwd;
        self
    }

    /// 실제화 대기(terminal 이든 plugin 이든) 상태인지 여부.
    pub fn is_deferred(&self) -> bool {
        self.deferred.is_some()
    }

    /// terminal 자리표시자면 그 spawn 파라미터. plugin/비-deferred 면 None.
    pub fn deferred_spawn(&self) -> Option<&DeferredSpawn> {
        match &self.deferred {
            Some(Deferred::Terminal(spawn)) => Some(spawn),
            _ => None,
        }
    }

    /// terminal 자리표시자의 spawn 파라미터를 가변 참조로. plugin/비-deferred 면 None.
    pub fn deferred_spawn_mut(&mut self) -> Option<&mut DeferredSpawn> {
        match &mut self.deferred {
            Some(Deferred::Terminal(spawn)) => Some(spawn),
            _ => None,
        }
    }

    /// plugin 자리표시자면 그 kind/snapshot. terminal/비-deferred 면 None.
    pub fn deferred_plugin(&self) -> Option<&DeferredPlugin> {
        match &self.deferred {
            Some(Deferred::Plugin(p)) => Some(p),
            _ => None,
        }
    }
}

impl Surface for EmptySurface {
    crate::impl_surface_any!();

    fn kind(&self) -> &'static str {
        "empty"
    }
    fn type_name(&self) -> &'static str {
        "Empty"
    }
    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    fn source_cwd(&self) -> Option<std::path::PathBuf> {
        self.cwd.clone()
    }

    fn to_tree_json(&self) -> serde_json::Value {
        match &self.deferred {
            Some(Deferred::Terminal(spawn)) => {
                // Deferred terminal placeholder — 외부에 Terminal로 보이고 pty_ready: false.
                serde_json::json!({
                    "type": "Terminal",
                    "kind": "terminal",
                    "id": self.id,
                    "cols": spawn.cols,
                    "rows": spawn.rows,
                    "pty_ready": false,
                })
            }
            Some(Deferred::Plugin(p)) => {
                // Deferred plugin placeholder — 원래 kind 로 나오되 아직 못 쓰는
                // 상태(`ready: false`)임을 응답에서 읽히게 한다. 에이전트가 "있다"
                // 로만 세고 못 쓰는 것을 모르면 그 자체가 결함(원칙 2)이라, kind 를
                // 그대로 노출하면서 ready 플래그로 구분한다.
                serde_json::json!({
                    "type": "Pending",
                    "kind": p.kind,
                    "id": self.id,
                    "ready": false,
                    "pending_reason": "plugin_not_loaded",
                })
            }
            None => serde_json::json!({
                "type": "Empty",
                "kind": "empty",
                "id": self.id,
            }),
        }
    }
}
