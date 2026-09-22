//! Host-side per-surface state for `ExplorerPanel` rendering (T11).
//!
//! `ExplorerPanel` (model) 은 식별 + 내비게이션(내부 탭/히스토리/뷰모드)만 보유하고,
//! 디렉토리 엔트리 캐시·선택 집합·사이드바 트리 펼침 같은 무거운 GUI 상태는 본 뷰
//! 스토어에 둔다 (markdown/image 뷰 스토어와 동형). surface id 로 keying.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tasty_model::{ExplorerPanel, SortColumn, SortDir, SurfaceId};

pub(crate) use crate::core::fs_list::{DirEntryInfo, human_size};
use crate::core::fs_list::{read_dir_entries, sort_entries};
use crate::i18n::t;

/// 디렉토리 로드 결과 상태 (content 중앙 상태 텍스트로 표현 — design §3.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Ok,
    /// (ADR-0059) 원격 mirror 응답 대기 중 — 로컬은 동기 IO 라 이 상태를 거치지 않는다.
    Loading,
    /// 권한 거부 (`PermissionDenied`).
    NoPermission,
    /// 그 외 IO 에러 (메시지).
    Error(String),
}

/// (ADR-0059) 원격 mirror 디렉토리 응답 소프트 타임아웃 — File Picker
/// (`file_picker.rs::LIST_DIR_SOFT_TIMEOUT`)와 동일 값. 상수 자체를 공유하진 않는다
/// (두 모듈이 서로를 참조할 근거가 없는 독립 소비자 — 값의 우연한 일치일 뿐).
const LIST_DIR_SOFT_TIMEOUT: Duration = Duration::from_secs(8);

/// 경로 하나의 원격 list_dir 요청 생애주기(ADR-0059 Decision 4 — `ExplorerView` 가
/// 자체 소유하는 경로별 pending 상태, host 범용 레지스트리 없음).
#[derive(Clone)]
enum RemoteLoadState {
    Loading {
        request_id: u64,
        sent_at: Instant,
    },
    /// 서버가 보낸 원본(Name/Asc) 엔트리 — main list/tree 소비처가 각자 필요한 형태로
    /// (재정렬/디렉토리만 필터) 파생한다.
    Loaded(Vec<DirEntryInfo>),
    Error(String),
}

/// `ExplorerView` 가 이번 프레임에 새로 만든 원격 list_dir 요청. 렌더 루프 중엔
/// `engine`(`CoreState::pending_list_dir_forward`)을 재차입할 수 없어(egui_panels 의
/// `ws`/`pane`/`tab`/`surface` 가 이미 `engine` 을 배타 차용 중) 여기 임시로 쌓아두고,
/// 호출자가 루프 종료 후 [`ExplorerViewStore::drain_outbox`] 로 옮긴다.
pub(crate) struct ExplorerListRequest {
    pub(crate) local_ws_id: u32,
    pub(crate) request_id: u64,
    pub(crate) dir: PathBuf,
}

pub struct ExplorerView {
    /// 정렬된 현재 디렉토리 엔트리.
    pub entries: Vec<DirEntryInfo>,
    /// `entries` 가 어떤 디렉토리/정렬 기준으로 로드됐는지 (변화 감지용).
    loaded: Option<(PathBuf, SortColumn, SortDir)>,
    /// 로드 결과 상태.
    pub state: LoadState,
    /// 선택된 엔트리 경로 집합.
    pub selected: HashSet<PathBuf>,
    /// 마지막으로 클릭(앵커)된 엔트리 — shift 범위 선택 기준.
    pub anchor: Option<PathBuf>,
    /// 사이드바 디렉토리 트리에서 펼쳐진 디렉토리.
    pub expanded: HashSet<PathBuf>,
    /// 사이드바 트리 펼침으로 읽은 하위 디렉토리 캐시.
    pub tree_children: HashMap<PathBuf, Vec<DirEntryInfo>>,
    /// 강제 새로고침 요청 플래그 (F5 / refresh 버튼).
    reload_requested: bool,
    /// 주소창(PathField) 편집 버퍼. 비편집 시 `sync()` 가 활성 탭 cwd 로 재동기화한다.
    pub addr_buffer: String,
    /// 주소창 편집(=트리거 포커스) 여부. PathField 가 매 프레임 갱신.
    pub addr_editing: bool,
    /// 주소창 후보 드롭다운의 keyboard-active 행(필터된 가시 목록 기준).
    pub addr_active: Option<usize>,
    /// (ADR-0059) 이 surface 가 원격 mirror 인가 — `Some(local_ws_id)` 면 원격, `None`
    /// 이면 로컬. `sync()`가 매 호출마다 최신값으로 갱신한다.
    mirror_ws_id: Option<u32>,
    /// (ADR-0059) 경로별 원격 요청 상태 — 로컬 surface 는 항상 비어 있다.
    remote_state: HashMap<PathBuf, RemoteLoadState>,
    /// (ADR-0059) 이번 프레임 새로 만든 원격 요청 — 렌더 루프 종료 후 drain.
    outbox: Vec<ExplorerListRequest>,
}

impl ExplorerView {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            loaded: None,
            state: LoadState::Ok,
            selected: HashSet::new(),
            anchor: None,
            expanded: HashSet::new(),
            tree_children: HashMap::new(),
            reload_requested: false,
            addr_buffer: String::new(),
            addr_editing: false,
            addr_active: None,
            mirror_ws_id: None,
            remote_state: HashMap::new(),
            outbox: Vec::new(),
        }
    }

    /// 주소창 편집을 취소한다. 내부 탭 전환/nav 로 cwd 가 바뀔 때 호출해 편집 버퍼가
    /// 다른 탭/경로로 새는 것을 막는다 — 다음 `sync()` 가 새 cwd 로 재동기화한다.
    pub fn cancel_addr_edit(&mut self) {
        self.addr_editing = false;
        self.addr_active = None;
    }

    /// 다음 렌더에서 현재 디렉토리를 다시 읽도록 표시.
    pub fn request_reload(&mut self) {
        self.reload_requested = true;
    }

    /// 현재 디렉토리의 모든 엔트리를 선택. 앵커는 마지막 엔트리로 둔다.
    pub fn select_all(&mut self) {
        self.selected = self.entries.iter().map(|e| e.path.clone()).collect();
        self.anchor = self.entries.last().map(|e| e.path.clone());
    }

    /// 선택된 경로를 (정렬·개행 결합) 텍스트로. 선택이 없으면 None.
    /// "경로 복사"(copy_path) 클립보드 페이로드.
    pub fn selected_paths_text(&self) -> Option<String> {
        if self.selected.is_empty() {
            return None;
        }
        let mut paths: Vec<String> = self
            .selected
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        paths.sort();
        Some(paths.join("\n"))
    }

    /// 활성 탭 기준으로 엔트리 캐시를 동기화. 로컬은 디렉토리/정렬이 바뀌었거나
    /// 새로고침이 요청됐으면 디스크에서 다시 읽는다. `mirror_ws_id` 가 `Some` 이면
    /// (ADR-0059) 원격 mirror surface — 동기 IO 대신 `list_dir_request` 를 큐잉하고
    /// 경로별 pending 상태로 진행 상황을 추적한다. 디렉토리가 바뀌면 선택을 초기화한다.
    pub fn sync(&mut self, panel: &ExplorerPanel, mirror_ws_id: Option<u32>) {
        let tab = panel.active_tab();
        // 주소창: 비편집 시 항상 활성 탭 cwd 로 재동기화(탭 전환/nav 로 cwd 가 바뀌면 버퍼도
        // 따라간다). 편집 중이면 사용자 입력을 보존한다. 엔트리 reload 여부와 독립이므로
        // 아래 early-return 보다 앞에 둔다.
        if !self.addr_editing {
            let cwd = tab.root.display().to_string();
            if self.addr_buffer != cwd {
                self.addr_buffer = cwd;
            }
            self.addr_active = None;
        }

        self.mirror_ws_id = mirror_ws_id;
        if let Some(local_ws_id) = mirror_ws_id {
            self.sync_remote(&tab.root, tab.sort_column, tab.sort_dir, local_ws_id);
            return;
        }

        let key = (tab.root.clone(), tab.sort_column, tab.sort_dir);
        let dir_changed = self
            .loaded
            .as_ref()
            .map(|(d, _, _)| d != &tab.root)
            .unwrap_or(true);
        let need = self.reload_requested || self.loaded.as_ref() != Some(&key);
        if !need {
            return;
        }
        self.reload_requested = false;
        if dir_changed {
            self.selected.clear();
            self.anchor = None;
        }
        match read_dir_entries(&tab.root) {
            Ok(mut entries) => {
                sort_entries(&mut entries, tab.sort_column, tab.sort_dir);
                self.entries = entries;
                self.state = LoadState::Ok;
            }
            Err(e) => {
                self.entries.clear();
                self.state = if e.kind() == std::io::ErrorKind::PermissionDenied {
                    LoadState::NoPermission
                } else {
                    LoadState::Error(e.to_string())
                };
            }
        }
        // 트리 펼침 캐시도 새로고침 시 무효화.
        if dir_changed || self.tree_children.is_empty() {
            // (펼침 자체는 유지 — 다음 트리 렌더에서 lazy 재로드)
            self.tree_children.clear();
        }
        self.loaded = Some(key);
    }

    /// (ADR-0059) 원격 mirror 경로: `dir` 의 원격 상태를 확인해 필요하면 새
    /// `list_dir_request` 를 큐잉(`outbox`)하고, 이미 있는 상태(Loading/Loaded/Error)를
    /// `entries`/`state` 에 반영한다. 응답 자체(`Loaded`/`Error` 전이)는
    /// [`Self::apply_remote_list_dir_result`] 가 담당 — 여기서는 절대 동기 IO 를 하지
    /// 않는다.
    fn sync_remote(
        &mut self,
        dir: &Path,
        sort_column: SortColumn,
        sort_dir: SortDir,
        local_ws_id: u32,
    ) {
        let key = (dir.to_path_buf(), sort_column, sort_dir);
        let dir_changed = self
            .loaded
            .as_ref()
            .map(|(d, _, _)| d.as_path() != dir)
            .unwrap_or(true);
        let refresh = self.reload_requested;
        self.reload_requested = false;

        // soft timeout — Loading 상태에서 응답이 오래 안 오면 연결 끊김으로 간주
        // (File Picker 의 `LIST_DIR_SOFT_TIMEOUT` 과 동일 값/근거).
        if let Some(RemoteLoadState::Loading { sent_at, .. }) = self.remote_state.get(dir)
            && sent_at.elapsed() > LIST_DIR_SOFT_TIMEOUT
        {
            self.remote_state.insert(
                dir.to_path_buf(),
                RemoteLoadState::Error(t("explorer.state.error_conn_timeout").to_string()),
            );
        }

        let need_request = match self.remote_state.get(dir) {
            None => true,
            Some(RemoteLoadState::Loading { .. }) => false,
            // Loaded/Error — 사용자가 명시적으로 새로고침을 요청했을 때만 재조회.
            Some(_) => refresh,
        };
        if need_request {
            let request_id = crate::core::next_list_dir_request_id();
            self.remote_state.insert(
                dir.to_path_buf(),
                RemoteLoadState::Loading {
                    request_id,
                    sent_at: Instant::now(),
                },
            );
            self.outbox.push(ExplorerListRequest {
                local_ws_id,
                request_id,
                dir: dir.to_path_buf(),
            });
        }

        if dir_changed {
            self.selected.clear();
            self.anchor = None;
            self.tree_children.clear();
        }

        self.state = match self.remote_state.get(dir) {
            Some(RemoteLoadState::Loading { .. }) | None => LoadState::Loading,
            Some(RemoteLoadState::Loaded(raw)) => {
                let mut entries = raw.clone();
                sort_entries(&mut entries, sort_column, sort_dir);
                self.entries = entries;
                LoadState::Ok
            }
            Some(RemoteLoadState::Error(msg)) => {
                self.entries.clear();
                if msg == "permission denied" {
                    LoadState::NoPermission
                } else {
                    LoadState::Error(msg.clone())
                }
            }
        };
        self.loaded = Some(key);
    }

    /// 이 view 안에서 `request_id` 로 대기 중인 경로를 찾는다. host 가 응답 라우팅
    /// 전 이걸로 "이 view 가 실제로 이 요청을 기다리는가"를 판정해, stale/불일치
    /// 응답은 여기서 `None` 을 돌려받아 조용히 무시한다(ADR-0059 Decision 6).
    fn find_pending_dir(&self, request_id: u64) -> Option<PathBuf> {
        self.remote_state
            .iter()
            .find_map(|(dir, state)| match state {
                RemoteLoadState::Loading {
                    request_id: rid, ..
                } if *rid == request_id => Some(dir.clone()),
                _ => None,
            })
    }

    /// (ADR-0059) `MirrorEvent::ListDirResult` 도착 시 App 레이어가 호출. 이 view 가
    /// `request_id` 를 실제로 기다리던 경로에 한해 반영 — 응답이 그 경로의 현재
    /// 활성 root 와 같으면 `entries`/`state` 도 함께 갱신, 아니면(트리 펼침 요청)
    /// `tree_children` 캐시만 채운다. 두 소비처(메인 목록/좌측 트리)가 같은 경로
    /// 캐시(`remote_state`)를 공유하므로 한 번의 응답으로 둘 다 최신화될 수 있다.
    pub(crate) fn apply_remote_list_dir_result(
        &mut self,
        request_id: u64,
        panel: &ExplorerPanel,
        result: Result<Vec<DirEntryInfo>, String>,
    ) -> bool {
        let Some(dir) = self.find_pending_dir(request_id) else {
            return false;
        };
        let current_root = panel.current_root();
        let is_current = dir == current_root;
        match result {
            Ok(entries) => {
                let mut tree_children: Vec<DirEntryInfo> =
                    entries.iter().filter(|e| e.is_dir).cloned().collect();
                tree_children.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.tree_children.insert(dir.clone(), tree_children);
                if is_current {
                    let tab = panel.active_tab();
                    let mut sorted = entries.clone();
                    sort_entries(&mut sorted, tab.sort_column, tab.sort_dir);
                    self.entries = sorted;
                    self.state = LoadState::Ok;
                }
                self.remote_state
                    .insert(dir, RemoteLoadState::Loaded(entries));
            }
            Err(reason) => {
                if is_current {
                    self.entries.clear();
                    self.state = if reason == "permission denied" {
                        LoadState::NoPermission
                    } else {
                        LoadState::Error(reason.clone())
                    };
                }
                self.tree_children.insert(dir.clone(), Vec::new());
                self.remote_state
                    .insert(dir, RemoteLoadState::Error(reason));
            }
        }
        true
    }

    /// 이번 프레임 새로 만든 원격 list_dir 요청을 모두 꺼낸다(렌더 루프 종료 후
    /// 호출자가 `CoreState::pending_list_dir_forward` 로 옮긴다).
    pub(crate) fn drain_outbox(&mut self) -> Vec<ExplorerListRequest> {
        std::mem::take(&mut self.outbox)
    }

    /// 사이드바 트리에서 `dir` 의 하위 디렉토리를 (캐시에 없으면) 읽어 반환.
    /// `mirror_ws_id` 가 `Some` 이면(ADR-0059) 동기 IO 대신 `list_dir_request` 를
    /// 큐잉하고, 응답이 올 때까지 빈 슬라이스를 반환한다(다음 프레임들에서 자동 채움).
    pub fn tree_children_of(&mut self, dir: &Path, mirror_ws_id: Option<u32>) -> &[DirEntryInfo] {
        if let Some(local_ws_id) = mirror_ws_id {
            if !self.tree_children.contains_key(dir) {
                if self.remote_state.get(dir).is_none() {
                    let request_id = crate::core::next_list_dir_request_id();
                    self.remote_state.insert(
                        dir.to_path_buf(),
                        RemoteLoadState::Loading {
                            request_id,
                            sent_at: Instant::now(),
                        },
                    );
                    self.outbox.push(ExplorerListRequest {
                        local_ws_id,
                        request_id,
                        dir: dir.to_path_buf(),
                    });
                }
                // placeholder — 응답 도착 전엔 빈 슬라이스, 매 프레임 재요청 방지.
                self.tree_children.entry(dir.to_path_buf()).or_default();
            }
            return self
                .tree_children
                .get(dir)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
        }
        if !self.tree_children.contains_key(dir) {
            let children = read_dir_entries(dir)
                .map(|mut v| {
                    v.retain(|e| e.is_dir);
                    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                    v
                })
                .unwrap_or_default();
            self.tree_children.insert(dir.to_path_buf(), children);
        }
        &self.tree_children[dir]
    }

    /// 단일 선택으로 설정.
    pub fn select_only(&mut self, path: &Path) {
        self.selected.clear();
        self.selected.insert(path.to_path_buf());
        self.anchor = Some(path.to_path_buf());
    }

    /// 토글 선택 (ctrl-click).
    pub fn toggle_select(&mut self, path: &Path) {
        if !self.selected.remove(path) {
            self.selected.insert(path.to_path_buf());
        }
        self.anchor = Some(path.to_path_buf());
    }
}

impl Default for ExplorerView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct ExplorerViewStore {
    views: HashMap<SurfaceId, ExplorerView>,
}

impl ExplorerViewStore {
    /// surface 의 뷰를 가져오고 (없으면 생성) 활성 탭 기준으로 동기화. `mirror_ws_id`
    /// 가 `Some` 이면(ADR-0059) 이 surface 가 속한 mirror workspace id — view 는 동기
    /// 로컬 IO 대신 원격 `list_dir_request` 를 큐잉한다.
    pub fn get_or_init(
        &mut self,
        panel: &ExplorerPanel,
        mirror_ws_id: Option<u32>,
    ) -> &mut ExplorerView {
        let view = self.views.entry(panel.id).or_default();
        view.sync(panel, mirror_ws_id);
        view
    }

    pub fn get(&self, sid: SurfaceId) -> Option<&ExplorerView> {
        self.views.get(&sid)
    }

    pub fn get_mut(&mut self, sid: SurfaceId) -> Option<&mut ExplorerView> {
        self.views.get_mut(&sid)
    }

    /// (ADR-0059) `request_id` 로 대기 중인 view 를 찾아 응답을 반영한다. 어느 view도
    /// 이 `request_id` 를 기다리지 않았으면(stale) `false`.
    pub(crate) fn apply_remote_list_dir_result(
        &mut self,
        surface_id: SurfaceId,
        request_id: u64,
        panel: &ExplorerPanel,
        result: Result<Vec<DirEntryInfo>, String>,
    ) -> bool {
        self.views
            .get_mut(&surface_id)
            .map(|v| v.apply_remote_list_dir_result(request_id, panel, result))
            .unwrap_or(false)
    }

    /// 모든 view 에서 이번 프레임 새로 만든 원격 list_dir 요청을 `(surface_id, request)`
    /// 쌍으로 drain 한다 — 렌더 루프(engine 재차입 불가) 종료 후 호출자가
    /// `CoreState::pending_list_dir_forward` 로 옮긴다.
    pub(crate) fn drain_outbox(&mut self) -> Vec<(SurfaceId, ExplorerListRequest)> {
        let mut out = Vec::new();
        for (&sid, view) in self.views.iter_mut() {
            for req in view.drain_outbox() {
                out.push((sid, req));
            }
        }
        out
    }

    pub fn drop_view(&mut self, sid: SurfaceId) {
        self.views.remove(&sid);
    }

    /// 지금 들고 있는 view 수 — `system.gpu_stats` 가 창마다 싣는다. view 는 surface 가 처음
    /// 렌더될 때 생기고 [`Self::drop_view`] 로 사라지므로, surface 를 닫은 뒤에도 줄지 않으면 그것이
    /// 누수다(메모리 soak 의 기준선 복귀 판정).
    pub(crate) fn view_count(&self) -> usize {
        self.views.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(true, 999), "—");
        assert_eq!(human_size(false, 512), "512 B");
        assert_eq!(human_size(false, 4096), "4.0 KB");
    }

    #[test]
    fn sort_dirs_first() {
        let mut v = vec![
            DirEntryInfo {
                path: "/z".into(),
                name: "z".into(),
                is_dir: false,
                size: 1,
                modified: None,
                ext: String::new(),
            },
            DirEntryInfo {
                path: "/a".into(),
                name: "a".into(),
                is_dir: true,
                size: 0,
                modified: None,
                ext: String::new(),
            },
        ];
        sort_entries(&mut v, SortColumn::Name, SortDir::Asc);
        assert!(v[0].is_dir);
    }

    /// 비편집 시 sync 가 주소창 버퍼를 활성 탭 cwd 로 맞춘다.
    #[test]
    fn sync_addr_buffer_tracks_cwd_when_not_editing() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);
        assert_eq!(view.addr_buffer, "/tmp/alpha");
        assert!(!view.addr_editing);
    }

    /// 편집 중이면 sync 가 cwd 로 덮어쓰지 않고 사용자 입력을 보존한다.
    #[test]
    fn sync_preserves_addr_buffer_while_editing() {
        let mut panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);
        // 편집 진입 후 타이핑.
        view.addr_editing = true;
        view.addr_buffer = "/tmp/typed".to_string();
        // cwd 가 바뀌어도(다른 탭/nav) 편집 중이면 버퍼 유지.
        panel
            .active_tab_mut()
            .navigate_to(PathBuf::from("/tmp/beta"));
        view.sync(&panel, None);
        assert_eq!(view.addr_buffer, "/tmp/typed");
    }

    /// cancel_addr_edit 후 sync 가 새 cwd 로 재동기화(내부 탭 전환/nav 누수 방지).
    #[test]
    fn cancel_addr_edit_resyncs_to_new_cwd() {
        let mut panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);
        view.addr_editing = true;
        view.addr_buffer = "/tmp/typed".to_string();
        panel
            .active_tab_mut()
            .navigate_to(PathBuf::from("/tmp/beta"));
        // 탭 전환/nav 적용 시 편집 취소 → 다음 sync 가 새 cwd 로 맞춘다.
        view.cancel_addr_edit();
        assert!(!view.addr_editing);
        view.sync(&panel, None);
        assert_eq!(view.addr_buffer, "/tmp/beta");
    }

    /// (ADR-0059) `mirror_ws_id` 가 `Some` 이면 동기 IO 대신 `list_dir_request` 를
    /// outbox 에 큐잉하고 `LoadState::Loading` 으로 전이한다.
    #[test]
    fn sync_remote_queues_request_and_sets_loading() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let mut view = ExplorerView::new();
        view.sync(&panel, Some(7));
        assert_eq!(view.state, LoadState::Loading);
        let drained = view.drain_outbox();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].local_ws_id, 7);
        assert_eq!(drained[0].dir, PathBuf::from("/remote/project"));
        // outbox 는 drain 후 비어야 다음 프레임에 중복 재요청하지 않는다.
        assert!(view.drain_outbox().is_empty());
    }

    /// (ADR-0059 Decision 6) 이 view 가 실제로 기다리던 request_id 가 아니면(stale)
    /// 조용히 무시 — `entries`/`state` 를 바꾸지 않고 `false` 를 반환한다.
    #[test]
    fn apply_remote_list_dir_result_ignores_stale_request_id() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let mut view = ExplorerView::new();
        view.sync(&panel, Some(7));
        let real_request_id = match view.remote_state.get(panel.current_root()) {
            Some(RemoteLoadState::Loading { request_id, .. }) => *request_id,
            _ => panic!("expected pending Loading state"),
        };
        let stale_request_id = real_request_id.wrapping_add(1);
        let applied = view.apply_remote_list_dir_result(stale_request_id, &panel, Ok(Vec::new()));
        assert!(!applied);
        assert_eq!(view.state, LoadState::Loading);
        assert!(view.entries.is_empty());
    }

    /// 실제로 기다리던 request_id 로 응답이 오면 현재 root 의 `entries`/`state` 를
    /// 갱신하고 `tree_children` 캐시도 함께 채운다(메인 목록·트리가 같은 응답을 공유).
    #[test]
    fn apply_remote_list_dir_result_updates_entries_on_matching_request_id() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let mut view = ExplorerView::new();
        view.sync(&panel, Some(7));
        let request_id = match view.remote_state.get(panel.current_root()) {
            Some(RemoteLoadState::Loading { request_id, .. }) => *request_id,
            _ => panic!("expected pending Loading state"),
        };
        let entries = vec![DirEntryInfo {
            path: "/remote/project/a".into(),
            name: "a".into(),
            is_dir: true,
            size: 0,
            modified: None,
            ext: String::new(),
        }];
        let applied = view.apply_remote_list_dir_result(request_id, &panel, Ok(entries.clone()));
        assert!(applied);
        assert_eq!(view.state, LoadState::Ok);
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].name, "a");
        assert_eq!(
            view.tree_children
                .get(panel.current_root())
                .map(|v| v.len()),
            Some(1)
        );
    }

    /// 실패 응답(`"permission denied"`)은 `LoadState::NoPermission` 으로, 그 외 사유는
    /// `LoadState::Error` 로 반영한다(File Picker 와 동일한 문자열 비교 판정).
    #[test]
    fn apply_remote_list_dir_result_maps_permission_denied_reason() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let mut view = ExplorerView::new();
        view.sync(&panel, Some(7));
        let request_id = match view.remote_state.get(panel.current_root()) {
            Some(RemoteLoadState::Loading { request_id, .. }) => *request_id,
            _ => panic!("expected pending Loading state"),
        };
        let applied = view.apply_remote_list_dir_result(
            request_id,
            &panel,
            Err("permission denied".to_string()),
        );
        assert!(applied);
        assert_eq!(view.state, LoadState::NoPermission);
    }
}
