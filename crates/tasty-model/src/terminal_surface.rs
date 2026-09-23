use super::SurfaceId;
pub use super::surface_layout::{SurfaceLayout, SurfaceRegion};
use super::surface_trait::Surface;

/// Surface 트리의 *terminal kind* placeholder.
///
/// PTY/Terminal/scrollback_persist 데이터는 모두 `CoreState::terminals`
/// (`TerminalStore`) 가 owner. 본 struct 는 *Surface 트리에서 terminal kind 인
/// leaf 라는 사실만 표시* 하는 id-only marker.
pub struct TerminalSurface {
    pub id: SurfaceId,
}

/// Parameters needed to spawn a PTY later (lazy init).
///
/// `EmptySurface { deferred_spawn: Some(..) }` placeholder 의 본체. PTY 가 spawn
/// 되는 시점에 TerminalSurface marker 로 교체된다.
#[derive(Clone)]
pub struct DeferredSpawn {
    pub shell: Option<String>,
    pub shell_args: Vec<String>,
    /// 자식 셸에 추가로 심을 환경변수(docs/features/terminal-output/index.md#명령-인덱싱-osc-133
    /// 참고). `shell_args` 와 마찬가지로 spawn
    /// 시점까지 owned 로 들고 있다가, 실제 PTY spawn 순간 `TerminalConfig` 로 넘긴다.
    pub extra_env: Vec<(String, String)>,
    pub cols: usize,
    pub rows: usize,
    pub waker: tasty_terminal::Waker,
    pub working_dir: Option<std::path::PathBuf>,
    /// PTY spawn 직후 즉시 send_key 로 주입할 명령. 줄바꿈은 호출자가 붙이지 않고
    /// `ensure_initialized` 가 `\r` 를 자동으로 덧붙여 submit 한다. TUI 세션 재개용
    /// (예: `claude -r <uuid>`).
    pub restore_command: Option<String>,
    /// 복원 시 layout.json 의 scrollback_ref 를 그대로 들고 있다가, PTY 가
    /// 실제로 spawn 되는 순간 `TerminalStore::set_scrollback_persist_id` 로 이관된다.
    pub scrollback_persist_id: Option<String>,
}

/// Non-terminal(plugin) surface 를 나중에 실제화하기 위한 파라미터.
///
/// `EmptySurface { deferred: Some(Deferred::Plugin(..)) }` placeholder 의 본체다.
/// layout 복원 시점에 plugin 이 아직 hello 를 안 보내 `SurfaceKindRegistry` 에
/// `kind` 가 없으면(부팅 창) 그 자리를 이 placeholder 가 차지하고, `kind` 가
/// 등록되는 순간 reify 가 `snapshot` 으로 실제 surface 를 복원한다. `?` 전파로
/// 형제 tab/pane 이 함께 유실되던 것을 막는 목적 — 상세 [`crate::empty_surface`].
#[derive(Clone)]
pub struct DeferredPlugin {
    /// surface kind 식별자(예: `"markdown"`). registry 등록을 기다리는 대상.
    pub kind: String,
    /// 해당 kind 의 `restore` 콜백이 받을 JSON. reify 까지 owned 로 보관한다.
    pub snapshot: serde_json::Value,
}

/// `EmptySurface` 가 나중에 실제화될 자리표시자일 때 그 종류를 한 자리에 담는다.
///
/// terminal(PTY lazy spawn)과 plugin(kind registry 대기)의 두 지연이 있고, 한
/// surface 가 **둘 다일 수는 없다** — `Option<DeferredSpawn>` 과
/// `Option<DeferredPlugin>` 을 각각 필드로 두면 "둘 다 Some" 이라는 무의미 상태가
/// 타입에 생기므로, enum 하나로 올려 그 상태를 컴파일러가 배제한다.
#[derive(Clone)]
pub enum Deferred {
    /// PTY 가 아직 안 뜬 터미널 자리.
    Terminal(DeferredSpawn),
    /// plugin kind 가 아직 registry 에 없는 non-terminal surface 자리.
    Plugin(DeferredPlugin),
}

impl Surface for TerminalSurface {
    crate::impl_surface_any!();

    fn kind(&self) -> &'static str {
        "terminal"
    }

    fn type_name(&self) -> &'static str {
        "Terminal"
    }

    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    /// Terminal 의 cwd 는 `engine.terminals.get(id).get_cwd()` 로 store 경유 —
    /// trait 는 None 반환. caller(host 의 `CoreState::surface_cwd`)가 분기 처리. Surface cwd
    /// invariant — `docs/design/policies/cwd.md#surface-cwd-invariant`.
    fn source_cwd(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn to_tree_json(&self) -> serde_json::Value {
        // cols/rows 는 caller 가 engine.terminals.get(id) 로 enrichment.
        serde_json::json!({
            "type": "Terminal",
            "id": self.id,
            "cols": 0,
            "rows": 0,
        })
    }
}
