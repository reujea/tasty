//! Host adapters for committed structure, live observations, and legacy layout import.
//! Pure decisions and canonical models live in tasty-core; execution owners live in runtime.

pub(crate) mod attach;
pub(crate) mod child_parent;
pub(crate) mod command_index;
#[cfg(feature = "gui")]
pub(crate) mod explorer_favorites;
pub(crate) mod file_kind;
pub(crate) mod file_snapshot;
pub(crate) mod fs_list;
pub(crate) mod hook_event_registry;
pub(crate) mod host_event;
#[cfg(feature = "gui")]
pub(crate) mod identify_port;
pub(crate) mod layout_persistence;
pub(crate) mod origin;
pub(crate) mod param_bag;
#[cfg(feature = "gui")]
pub(crate) mod port_favorites;
#[cfg(feature = "gui")]
pub(crate) mod shared_list;
pub(crate) mod state;

pub(crate) mod request_target;

pub(crate) use state::AttentionKind;
#[cfg(feature = "gui")]
pub(crate) use state::{AttachMeshContextForward, PendingImageUpload};
pub(crate) use tasty_core::CoreState;

#[cfg(feature = "gui")]
static NEXT_LIST_DIR_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "gui")]
pub(crate) fn next_list_dir_request_id() -> u64 {
    NEXT_LIST_DIR_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "gui")]
static NEXT_GIT_QUERY_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "gui")]
pub(crate) fn next_git_query_request_id() -> u64 {
    NEXT_GIT_QUERY_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// markdown 요청 ID는 1부터 시작한다. 0은 대기 요청 취소 신호지만 카운터 overflow는 별도로 막지 않는다.
#[cfg(feature = "gui")]
static NEXT_MARKDOWN_CONTENT_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "gui")]
pub(crate) fn next_markdown_content_request_id() -> u64 {
    NEXT_MARKDOWN_CONTENT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 파일 피커 요청의 ID. 플러그인 요청이면 file_picker.trigger 왕복의 request_id로도 쓴다.
/// popup 내부에서 디렉터리를 조회하는 list_dir 요청 ID와 별개다.
#[cfg(feature = "gui")]
static NEXT_FILE_PICKER_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "gui")]
pub(crate) fn next_file_picker_request_id() -> u64 {
    NEXT_FILE_PICKER_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// memory port와 같은 poison 보고 플래그를 공유해 같은 락의 오류를 여러 번 기록하지 않는다.
pub(crate) use tasty_memory::{
    STORE_LOCK_POISONED as MEMORY_POISONED, STORE_LOCK_WHAT as MEMORY_WHAT,
};

/// preset_store와 presets가 같은 Arc를 공유하므로 poison 보고 플래그도 하나를 쓴다.
pub(crate) const PRESET_STORE_WHAT: &str = "preset store";
pub(crate) static PRESET_STORE_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// App이 attach 채널로 보낼 디렉터리 조회 요청.
#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) struct PendingListDirForward {
    pub(crate) local_ws_id: u32,
    pub(crate) request_id: u64,
    pub(crate) dir: String,
    /// None이면 파일 선택기, Some이면 해당 explorer surface로 응답을 전달한다.
    pub(crate) consumer: Option<u32>,
}

/// App이 attach 채널로 보낼 Git 조회 요청.
#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) struct PendingGitQueryForward {
    pub(crate) admitted: std::time::Instant,
    /// 대기 표시를 걸 popup instance. 요청한 plugin의 instance일 때만 쓴다.
    pub(crate) consumer: Option<u64>,
    /// 결과 이벤트를 받을 plugin. plugin이 아닌 호출자면 결과를 보낼 곳이 없다.
    pub(crate) requester: Option<String>,
    /// 원격 ID로 바꿀 로컬 mirror surface. worktree_path가 없으면 서버가 해당 터미널 cwd를 조회한다.
    pub(crate) local_surface_id: u32,
    pub(crate) request_id: u64,
    pub(crate) kind: tasty_ipc::stream_hub::GitQueryKind,
    /// 서버 응답에서 받은 경로. client의 로컬 경로로 해석하지 않는다.
    pub(crate) worktree_path: Option<String>,
    /// Diff 요청에만 사용한다.
    pub(crate) diff_path: Option<String>,
}

/// markdown 원문 조회 요청. plugin이 surface별 대기를 추적하고 응답에도 surface ID가 포함된다.
#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) struct PendingMarkdownContentForward {
    pub(crate) user_wait: bool,
    pub(crate) admitted: std::time::Instant,
    /// 원문을 기다리는 로컬 mirror surface ID.
    pub(crate) local_surface_id: u32,
    pub(crate) request_id: u64,
    /// 에이전트 요청의 잘린 응답은 사용자 toast로 알리지 않는다.
    pub(crate) agent_origin: bool,
}

pub(crate) mod live;
