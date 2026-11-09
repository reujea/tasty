//! surface별 탐색기 표시 상태. 모델의 탐색 이력과 별도로 목록 캐시·선택·트리 펼침을 내부 탭별로 보관한다.

pub(crate) mod cursor;
pub(crate) mod drag;
pub(crate) mod drag_scroll;
mod entry_snapshot;
pub(crate) mod hidden;
pub(crate) mod keys;
pub(crate) mod list_layout;
pub(crate) mod marquee;
pub(crate) mod ops;
mod path_index;
pub(crate) mod poll;
#[cfg(test)]
mod remote_cache_tests;
mod remote_reload;
mod selection;
mod tab_state;

pub(crate) use entry_snapshot::EntrySnapshot;
pub use selection::SelectedPaths;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tasty_model::{ExplorerPanel, ExplorerTabId, SortColumn, SortDir, SurfaceId};

use super::type_ahead::TypeAhead;

use crate::app::local_reads::StampedListing;
use crate::core::fs_list::sort_entries;
pub(crate) use crate::core::fs_list::{DirEntryInfo, human_size};
use crate::i18n::t;

/// 디렉토리 로드 결과 상태 (content 중앙 상태 텍스트로 표현 — design §3.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Ok,
    /// 로컬 worker 또는 원격 mirror 응답을 기다린다.
    Loading,
    /// 권한 거부 (`PermissionDenied`).
    NoPermission,
    /// 그 외 IO 에러 (메시지).
    Error(String),
}

/// 원격 디렉터리 응답의 UI 대기 제한. 로컬 worker 수명과는 별개다.
pub(super) const LIST_DIR_SOFT_TIMEOUT: Duration = Duration::from_secs(8);

/// 경로 하나의 원격 list_dir 요청 생애주기(docs/features/remote-attach/index.md — `ExplorerView` 가
/// 자체 소유하는 경로별 pending 상태, host 범용 레지스트리 없음).
#[derive(Clone)]
enum RemoteLoadState {
    Loading {
        request_id: u64,
        sent_at: Instant,
    },
    /// 서버가 보낸 원본(Name/Asc) 엔트리 — main list/tree 소비처가 각자 필요한 형태로
    /// (재정렬/디렉토리만 필터) 파생한다.
    Loaded {
        entries: Vec<DirEntryInfo>,
        /// 새 응답은 한 번 적용한다. 이후 같은 정렬의 프레임은 목록을 재사용한다.
        applied: bool,
    },
    Error(String),
}

/// 렌더 중 만든 원격 조회 요청. engine을 다시 빌릴 수 없어 루프 종료 뒤 outbox를 옮긴다.
pub(crate) struct ExplorerListRequest {
    pub(crate) local_ws_id: u32,
    pub(crate) request_id: u64,
    pub(crate) dir: PathBuf,
}

pub struct ExplorerView {
    /// 정렬된 현재 디렉토리 엔트리. 새 목록으로 바꿀 때는 `set_entries` 를 쓴다.
    pub entries: EntrySnapshot,
    /// `entries` 를 바꿀 때마다 오르는 세대. Find 거르기 결과가 이 목록으로 만든 것인지 가린다.
    pub(super) entries_gen: u64,
    pub(super) path_index: std::cell::RefCell<path_index::PathIndex>,
    /// `entries` 가 어떤 디렉토리/정렬 기준으로 로드됐는지 (변화 감지용).
    pub(super) loaded: Option<(PathBuf, SortColumn, SortDir)>,
    local_query: Option<crate::app::local_reads::Query<StampedListing>>,
    tree_queries: HashMap<PathBuf, crate::app::local_reads::Query<StampedListing>>,
    /// 로드 결과 상태.
    pub state: LoadState,
    /// 선택된 엔트리 경로 집합.
    pub selected: SelectedPaths,
    selection_identity: std::sync::Arc<()>,
    /// 범위 선택의 기준 엔트리. 단일 클릭·토글 클릭이 옮기고 Shift 클릭은 그대로 둔다.
    pub anchor: Option<PathBuf>,
    /// 사이드바 디렉토리 트리에서 펼쳐진 디렉토리.
    pub expanded: HashSet<PathBuf>,
    /// 사이드바 트리 펼침으로 읽은 하위 디렉토리 캐시.
    pub tree_children: HashMap<PathBuf, Vec<DirEntryInfo>>,
    /// 강제 새로고침 요청 플래그 (F5 / refresh 버튼).
    reload_requested: bool,
    /// 사용자 새로고침(F5·툴바)이다. 같은 폴더를 보이고 있으면 목록과 트리를 둔 채 다시 읽는다.
    refresh_in_place: bool,
    /// 현재 폴더가 없어 읽지 못했을 때 남아 있는 가장 가까운 상위 폴더. 읽기 오류 화면의 "상위 폴더로" 가 쓴다.
    existing_ancestor: Option<PathBuf>,
    /// 원격 mirror의 홈 폴더. 연결마다 한 번 조회하며 아직 모르면 None이다. 주소창의 `~` 가 쓴다.
    pub(crate) remote_home: Option<PathBuf>,
    /// 주소창(PathField) 편집 버퍼. 비편집 시 `sync()` 가 활성 탭 cwd 로 재동기화한다.
    pub addr_buffer: String,
    /// 주소창 편집(=트리거 포커스) 여부. PathField 가 매 프레임 갱신.
    pub addr_editing: bool,
    /// 주소창 후보 드롭다운의 keyboard-active 행(필터된 가시 목록 기준).
    pub addr_active: Option<usize>,
    /// 주소창 포커스 단축키를 눌렀다. 다음에 주소창을 그릴 때 포커스를 주고 소비한다.
    pub(crate) focus_address: bool,
    /// 파일인지 확인하려고 부모 폴더 목록을 기다리는 원격 주소 입력.
    pub(super) addr_probe: Option<super::address::AddressProbe>,
    /// (docs/features/remote-attach/index.md) 이 surface 가 원격 mirror 인가 — `Some(local_ws_id)` 면 원격, `None`
    /// 이면 로컬. `sync()`가 매 호출마다 최신값으로 갱신한다.
    pub(super) mirror_ws_id: Option<u32>,
    /// (docs/features/remote-attach/index.md) 경로별 원격 요청 상태 — 로컬 surface 는 항상 비어 있다.
    remote_state: HashMap<PathBuf, RemoteLoadState>,
    /// 이 칸의 파일을 바꿀 수 있는가. 로컬은 늘 참이고, 원격은 그 서버가 원격 쓰기를 알린 동안 참이다.
    pub(crate) writes_allowed: bool,
    /// 원격 칸이 붙은 Tasty. 진행 표시의 호스트와 쓰기를 거절하는 이유에 쓴다. 로컬은 None 이다.
    pub(crate) remote_host: Option<crate::remote::state::ExplorerRemoteHost>,
    /// 원격 폴더가 있는 디스크의 서버 쪽 식별값. 끌어 놓기의 기본 동작(이동·복사)을 정한다.
    remote_volumes: HashMap<PathBuf, u64>,
    /// (docs/features/remote-attach/index.md) 이번 프레임 새로 만든 원격 요청 — 렌더 루프 종료 후 drain.
    pub(super) outbox: Vec<ExplorerListRequest>,
    /// 영숫자로 항목을 선택하는 타입어헤드의 입력 버퍼. 동작 규칙은 `type_ahead`
    /// 모듈에 있다.
    pub type_ahead: TypeAhead,
    /// 이번 프레임에 화면에 보이도록 스크롤할 항목의 경로. 사용한 뒤에는 비운다.
    /// 남겨두면 매 프레임 다시 스크롤해서 사용자가 휠로 다른 곳을 볼 때 끌려간다.
    pub scroll_to: Option<PathBuf>,
    /// 목록 오른쪽 미리보기 패널.
    pub preview: super::preview::PreviewPane,
    /// Grid 썸네일 캐시.
    pub thumbs: super::thumbs::Thumbs,
    /// 목록 맨 위에서 이름을 받고 있는 새 항목.
    pub(crate) create: Option<super::create::CreateEdit>,
    /// 이름 입력을 열기 전에 기다리는 대상 폴더의 쓰기 가능 확인과, 쓸 수 없다고 확인되어 알릴 거절.
    pub(super) create_check: Option<super::create::WriteCheck>,
    pub(super) create_refused: bool,
    /// 방금 만든 항목이나 들어 있는 폴더에서 보일 항목. 다시 읽은 목록에 나타나면 그 자리로 스크롤한다.
    pub(super) reveal: Option<PathBuf>,
    /// 나타났을 때 그 항목을 고르기도 하는가(들어 있는 폴더에서 보기).
    pub(super) reveal_select: bool,
    /// 고를 항목(`reveal_after_load`)이 기다리는 폴더와 그 폴더에 한 번 도착했는지.
    pub(super) reveal_in: Option<(PathBuf, bool)>,
    /// 지금 보는 로컬 폴더에 쓸 수 있는가. 확인한 폴더와 함께 둔다.
    writable: Option<(PathBuf, bool)>,
    writable_query: Option<(PathBuf, crate::app::local_reads::Query<bool>)>,
    /// 열려 있는 Find 바.
    pub(crate) find: Option<super::find::FindState>,
    /// 이 칸이 요청한 파일 작업의 진행·대기열·결과 표시.
    pub(crate) ops: ops::OpsState,
    /// 다른 프로그램이 바꾼 폴더를 찾는 주기 확인.
    poll: poll::ExternalPoll,
    /// 키보드로 움직이는 현재 항목.
    pub(crate) cursor: cursor::CursorState,
    /// 빈 곳에서 끄는 영역 선택.
    pub(crate) marquee: marquee::MarqueeState,
    /// 이번 프레임 목록 영역(미리보기 패널 제외). 상태 화면이면 배치가 없다.
    pub(crate) list_rect: Option<egui::Rect>,
    /// 파일 영역이 칠할 포커스 값. 호스트가 프레임마다 채운다. 입력 판단은 `ExplorerInput.focused`다.
    pub(crate) bed_focused: bool,
    /// 이번 프레임에 그린 목록의 칸 배치.
    pub(crate) list_layout: Option<list_layout::ListLayout>,
    /// 숨김 파일 표시 여부와 끈 동안 뺀 항목.
    pub(crate) hidden: hidden::HiddenFiles,
    /// 위 탭별 필드가 지금 담고 있는 내부 탭. 처음 동기화하기 전에는 None 이다.
    tab: Option<ExplorerTabId>,
    /// 활성이 아닌 내부 탭의 상태(`tab_state`).
    parked: HashMap<ExplorerTabId, tab_state::TabSession>,
}

/// 사이드바 트리에 두는 하위 폴더 목록. 이름 오름차순이다.
fn tree_dirs(dirs: impl Iterator<Item = DirEntryInfo>) -> Vec<DirEntryInfo> {
    let mut dirs: Vec<DirEntryInfo> = dirs.collect();
    sort_entries(&mut dirs, SortColumn::Name, SortDir::Asc);
    dirs
}

impl ExplorerView {
    pub(super) fn poll_local_reads(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) -> bool {
        // 확인 결과가 건 다시 읽기를 아래에서 같은 차례에 worker 로 넘긴다.
        self.poll_external_result(owner);
        let mut changed = self.poll_tab_reads(owner);
        changed |= self.preview.poll(owner);
        changed |= self.thumbs.poll(owner);
        self.poll_parked_reads(owner);
        changed
    }

    /// 지금 탭 몫의 목록·쓰기 확인·검색·트리 읽기 결과를 거둔다. 보관한 탭에도 같은 것을 쓴다.
    fn poll_tab_reads(&mut self, owner: &mut crate::app::local_reads::ReadRequests) -> bool {
        let mut changed = false;
        if let Some(result) = self
            .local_query
            .as_mut()
            .and_then(|query| query.poll(owner))
        {
            self.local_query = None;
            if self.mirror_ws_id.is_none() {
                match result {
                    Ok(StampedListing {
                        stamp,
                        mut entries,
                        entries_key,
                    }) => {
                        if let Some((root, col, dir)) = &self.loaded {
                            sort_entries(&mut entries, *col, *dir);
                            self.poll.note_read(root, stamp);
                            self.poll.note_entries(root, entries_key);
                            // 트리가 이 폴더의 하위 목록을 캐시해 두었으면 같은 결과로 바꾼다.
                            if let Some(children) = self.tree_children.get_mut(root) {
                                *children = tree_dirs(entries.iter().filter(|e| e.is_dir).cloned());
                            }
                        }
                        self.set_entries(entries);
                        self.state = LoadState::Ok;
                        self.retain_listed_selection();
                    }
                    Err(error) => {
                        self.set_entries(Vec::new());
                        self.existing_ancestor = error
                            .get_ref()
                            .and_then(|e| {
                                e.downcast_ref::<crate::app::local_reads::MissingFolder>()
                            })
                            .and_then(|m| m.existing_ancestor.clone());
                        self.state = if error.kind() == std::io::ErrorKind::PermissionDenied {
                            LoadState::NoPermission
                        } else {
                            LoadState::Error(error.to_string())
                        };
                    }
                }
                changed = true;
            }
        }
        if let Some((dir, query)) = &mut self.writable_query
            && let Some(result) = query.poll(owner)
        {
            // 확인하지 못했으면 쓸 수 있다고 보고 실제 쓰기의 오류로 알린다.
            self.writable = Some((dir.clone(), result.unwrap_or(true)));
            self.writable_query = None;
            // 목록이 아직 오지 않았으면 그 결과가 다시 그리게 한다.
            changed |= self.local_query.is_none();
        }
        changed |= self.poll_find(owner);
        changed |= self.poll_create_check(owner);
        let ready: Vec<_> = self
            .tree_queries
            .iter_mut()
            .filter_map(|(path, query)| query.poll(owner).map(|result| (path.clone(), result)))
            .collect();
        for (path, result) in ready {
            self.tree_queries.remove(&path);
            match result {
                Ok(StampedListing { stamp, entries, .. }) => {
                    self.poll.note_read(&path, stamp);
                    let children = tree_dirs(entries.into_iter().filter(|entry| entry.is_dir));
                    self.tree_children.insert(path, children);
                }
                Err(error) => {
                    tracing::debug!(%error, path=%path.display(), "Explorer tree read failed");
                    self.tree_children.insert(path, Vec::new());
                }
            }
            changed = true;
        }
        changed
    }
    /// 목록을 바꾸고 세대를 올린다. 숨김 파일을 끈 동안에는 숨김 항목을 따로 둔다.
    pub(super) fn set_entries(&mut self, entries: Vec<DirEntryInfo>) {
        self.entries = self.split_hidden(entries).into();
        self.entries_gen += 1;
    }

    pub fn new() -> Self {
        Self {
            entries: Default::default(),
            entries_gen: 0,
            path_index: path_index::empty(),
            loaded: None,
            local_query: None,
            tree_queries: HashMap::new(),
            state: LoadState::Ok,
            selected: SelectedPaths::default(),
            selection_identity: std::sync::Arc::new(()),
            anchor: None,
            expanded: HashSet::new(),
            tree_children: HashMap::new(),
            reload_requested: false,
            refresh_in_place: false,
            existing_ancestor: None,
            remote_home: None,
            addr_buffer: String::new(),
            addr_editing: false,
            addr_active: None,
            focus_address: false,
            addr_probe: None,
            mirror_ws_id: None,
            remote_state: HashMap::new(),
            writes_allowed: true,
            remote_host: None,
            remote_volumes: HashMap::new(),
            outbox: Vec::new(),
            type_ahead: TypeAhead::default(),
            scroll_to: None,
            preview: Default::default(),
            thumbs: Default::default(),
            create: None,
            create_check: None,
            create_refused: false,
            reveal: None,
            reveal_select: false,
            reveal_in: None,
            writable: None,
            writable_query: None,
            find: None,
            ops: ops::OpsState::default(),
            poll: Default::default(),
            cursor: Default::default(),
            marquee: Default::default(),
            list_rect: None,
            bed_focused: false,
            list_layout: None,
            hidden: Default::default(),
            tab: None,
            parked: HashMap::new(),
        }
    }

    /// 원격 mirror 탐색기인가. 원격은 파일 내용을 읽지 않는다.
    pub(crate) fn is_remote(&self) -> bool {
        self.mirror_ws_id.is_some()
    }

    /// 타입어헤드 입력을 비운다. 내부 탭을 바꾸거나 추가·닫을 때처럼 목록이 통째로
    /// 바뀌는 자리에서 호출한다. 이전 탭에서 입력하던 접두사가 새 목록으로 이어지면
    /// 안 되기 때문이다. 폴더나 정렬이 바뀌는 경우는 `sync()`가 알아서 처리한다.
    pub fn reset_type_ahead(&mut self) {
        self.type_ahead.reset();
        self.scroll_to = None;
    }

    /// 주소창 편집을 취소한다. 내부 탭 전환/nav 로 cwd 가 바뀔 때 호출해 편집 버퍼가
    /// 다른 탭/경로로 새는 것을 막는다 — 다음 `sync()` 가 새 cwd 로 재동기화한다.
    pub fn cancel_addr_edit(&mut self) {
        self.addr_editing = false;
        self.addr_active = None;
    }

    /// 지금 폴더 목록이나 하위 폴더 검색 결과에 있는 그 경로의 항목.
    pub(crate) fn listed_entry(&self, path: &Path) -> Option<&DirEntryInfo> {
        self.entries
            .iter()
            .chain(self.search_hits())
            .find(|e| e.path == path)
    }

    /// 목록(하위 폴더 검색 결과 포함)에서 대상이 없는 링크인가. 열기 대신 원인을 알리는 데 쓴다.
    pub(crate) fn is_broken_link(&self, path: &Path) -> bool {
        self.listed_entry(path)
            .is_some_and(|e| e.link == crate::core::fs_list::EntryLink::Broken)
    }

    pub(crate) fn selection_identity(&self) -> std::sync::Weak<()> {
        std::sync::Arc::downgrade(&self.selection_identity)
    }
    pub(crate) fn matches_selection(&self, identity: &std::sync::Weak<()>) -> bool {
        self.selection_identity().ptr_eq(identity)
    }
    /// 선택을 통째로 바꾼다(영역 선택). 새 선택이므로 선택 식별자도 바꾼다.
    pub(crate) fn replace_selection(&mut self, selected: SelectedPaths, anchor: Option<PathBuf>) {
        self.selection_identity = std::sync::Arc::new(());
        self.selected = selected;
        self.anchor = anchor;
    }

    /// 하나만 골랐을 때 그 항목의 목록 번호. 타입어헤드가 그다음부터 찾는다.
    pub(crate) fn single_selected_index(&self) -> Option<usize> {
        if self.selected.len() != 1 {
            return None;
        }
        let sel = self.selected.iter().next()?;
        self.shown_position(sel)
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection_identity = std::sync::Arc::new(());
        self.selected.clear();
        self.anchor = None;
    }

    /// 읽기 오류 화면의 "상위 폴더로" 가 할 일. 바로 위 폴더도 사라졌으면 남아 있는 가장 가까운 상위 폴더로 간다.
    /// 확인한 상위 폴더가 없으면(원격, 다른 오류) 한 단계 위로 간다.
    pub(crate) fn go_up_action(&self, root: &Path) -> super::ExplorerAction {
        match &self.existing_ancestor {
            Some(ancestor) if Some(ancestor.as_path()) != root.parent() => {
                super::ExplorerAction::Navigate(ancestor.clone())
            }
            _ => super::ExplorerAction::GoUp,
        }
    }

    /// 다시 읽은 목록에 없는 경로를 선택에서 뺀다. 다른 곳에서 지운 항목을 상태줄이 세지 않게 한다.
    /// 남은 선택은 같은 선택이므로 선택 식별자는 바꾸지 않는다.
    fn retain_listed_selection(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let listed: HashSet<PathBuf> = self
            .entries
            .iter()
            .chain(self.search_hits())
            .map(|e| e.path.clone())
            .collect();
        self.selected.retain(|p| listed.contains(p));
        if self.anchor.as_ref().is_some_and(|a| !listed.contains(a)) {
            self.anchor = None;
        }
    }

    /// 주소창·이름 입력·Find 입력이 키보드를 쓰고 있다. 그동안 타입어헤드와 목록 단축키를 멈춘다.
    pub(crate) fn text_input_active(&self) -> bool {
        self.addr_editing
            || self.create.is_some()
            || self.find.as_ref().is_some_and(|f| f.field_focused)
    }

    /// 지금 보이는 목록의 폴더.
    pub(crate) fn shown_dir(&self) -> Option<&Path> {
        self.loaded.as_ref().map(|(dir, _, _)| dir.as_path())
    }

    /// 로컬 explorer 가 지금 폴더에 쓸 수 있는가. 아직 확인하지 못했으면 true 다.
    pub(crate) fn can_write_here(&self) -> bool {
        match (&self.writable, self.shown_dir()) {
            (Some((dir, writable)), Some(shown)) if dir == shown => *writable,
            _ => true,
        }
    }

    /// 만든 항목을 고른다. 사용자가 그 폴더를 떠났거나 요청한 뒤 선택을 바꿨으면(`selection` 은 요청
    /// 때의 선택 식별자) 아무것도 바꾸지 않는다.
    pub(crate) fn reveal_created_if_unchanged(
        &mut self,
        path: &Path,
        selection: Option<&std::sync::Weak<()>>,
    ) {
        if selection.is_some_and(|old| self.matches_selection(old)) {
            self.reveal_created(path);
        }
    }

    /// 만든 항목을 고른다. 사용자가 그 폴더를 떠났으면 아무것도 바꾸지 않는다.
    pub(crate) fn reveal_created(&mut self, path: &Path) {
        if self.shown_dir() != path.parent() {
            return;
        }
        self.select_only(path);
        self.reveal = Some(path.to_path_buf());
        self.reveal_in = None;
        self.reveal_select = false;
    }

    /// 다시 읽은 목록에 나타나기를 기다리는 보일 항목.
    #[cfg(test)]
    pub(crate) fn pending_reveal(&self) -> Option<&Path> {
        self.reveal.as_deref()
    }

    /// 만든 항목이 목록에 나타났으면 이번 프레임의 스크롤 대상으로 꺼낸다. 없으면 키보드로 옮긴
    /// 현재 항목이 화면 밖일 때 그 항목이다.
    pub(crate) fn take_reveal(&mut self) -> Option<PathBuf> {
        let listed = self
            .reveal
            .as_ref()
            .is_some_and(|p| self.entries.iter().any(|e| &e.path == p));
        if !listed {
            return self.cursor.take_scroll();
        }
        let path = self.reveal.take()?;
        self.reveal_in = None;
        if std::mem::take(&mut self.reveal_select) {
            self.select_only(&path);
        }
        Some(path)
    }

    /// 다음 렌더에서 현재 디렉토리를 다시 읽도록 표시.
    pub fn request_reload(&mut self) {
        self.reload_requested = true;
    }

    /// 사용자 새로고침(F5·툴바·다시 시도). 같은 폴더의 목록을 보이고 있으면 외부 변경 확인처럼
    /// 보이던 목록과 펼친 트리를 둔 채 다시 읽어 스크롤을 지킨다. 그 밖에는 [`Self::request_reload`] 와 같다.
    pub fn request_refresh(&mut self) {
        self.reload_requested = true;
        self.refresh_in_place = true;
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
    /// (docs/features/remote-attach/index.md) 원격 mirror surface — 동기 IO 대신 `list_dir_request` 를 큐잉하고
    /// 경로별 pending 상태로 진행 상황을 추적한다. 디렉토리가 바뀌면 선택을 초기화한다.
    pub fn sync(&mut self, panel: &ExplorerPanel, mirror_ws_id: Option<u32>) {
        // 내부 탭 전환은 폴더 변경이 아니다. 그 탭의 상태를 먼저 꺼낸 뒤 아래에서 폴더를 비교한다.
        self.switch_tab(panel);
        self.preview.adopt(&panel.preview);
        self.adopt_hidden(panel.show_hidden);
        let tab = panel.active_tab();
        self.settle_reveal(&tab.root);
        // 편집 중이거나 원격 확인을 기다리는 중에는 입력을 유지하고, 아니면 주소를 현재 cwd로 맞춘다.
        // 목록 갱신과는 별개다.
        if !self.addr_editing && self.addr_probe.is_none() {
            let cwd = tab.root.display().to_string();
            if self.addr_buffer != cwd {
                self.addr_buffer = cwd;
            }
            self.addr_active = None;
        }

        self.sync_find(&tab.root, false);
        self.mirror_ws_id = mirror_ws_id;
        if let Some(local_ws_id) = mirror_ws_id {
            self.local_query = None;
            self.tree_queries.clear();
            self.refresh_in_place = false;
            self.sync_remote(&tab.root, tab.sort_column, tab.sort_dir, local_ws_id);
            return;
        }

        let key = (tab.root.clone(), tab.sort_column, tab.sort_dir);
        let dir_changed = self
            .loaded
            .as_ref()
            .map(|(d, _, _)| d != &tab.root)
            .unwrap_or(true);
        let explicit = self.reload_requested;
        let same_list = self.loaded.as_ref() == Some(&key);
        let need = explicit || !same_list;
        let in_place = std::mem::take(&mut self.refresh_in_place)
            && explicit
            && same_list
            && self.state == LoadState::Ok;
        if !need {
            return;
        }
        self.reload_requested = false;
        self.sync_find(&tab.root, true);
        if in_place {
            self.refresh_keeping_list(&tab.root);
            return;
        }
        // 목록이 바뀌는 것이 확실한 지점이다. 정렬만 바뀌어도 인덱스가 가리키는 항목이
        // 달라지므로, 폴더 변경(`dir_changed`)보다 넓은 이 조건에서 버퍼를 비운다.
        self.reset_type_ahead();
        if dir_changed {
            self.clear_selection();
        }
        self.existing_ancestor = None;
        self.local_query = Some(crate::app::local_reads::stamped_directory(tab.root.clone()));
        self.writable_query = Some((
            tab.root.clone(),
            crate::app::local_reads::writable(tab.root.clone()),
        ));
        self.set_entries(Vec::new());
        self.state = LoadState::Loading;
        // 새로고침은 같은 폴더여도 펼친 트리를 다시 읽는다. 트리만 바뀐 경우를 놓치지 않기 위해서다.
        if dir_changed || explicit || self.tree_children.is_empty() {
            self.tree_children.clear();
            self.tree_queries.clear();
        }
        self.loaded = Some(key);
    }

    /// 보이던 목록·선택·트리 캐시를 둔 채 현재 폴더와 하위 목록을 읽어 둔 트리 폴더를 다시 읽는다.
    /// 결과가 오면 외부 변경 확인의 다시 읽기와 같은 경로로 바꾼다. 트리 캐시를 먼저 비우면 사이드바
    /// 높이가 줄어 트리 스크롤이 맨 위로 돌아가고, 목록을 비우면 목록 스크롤이 맨 위로 간다.
    fn refresh_keeping_list(&mut self, root: &Path) {
        self.reset_type_ahead();
        self.local_query = Some(crate::app::local_reads::stamped_directory(
            root.to_path_buf(),
        ));
        self.writable_query = Some((
            root.to_path_buf(),
            crate::app::local_reads::writable(root.to_path_buf()),
        ));
        // 트리의 현재 폴더는 목록 결과가 함께 바꾼다.
        let stale: Vec<PathBuf> = self
            .tree_children
            .keys()
            .filter(|dir| dir.as_path() != root && !self.tree_queries.contains_key(*dir))
            .cloned()
            .collect();
        for dir in stale {
            let query = crate::app::local_reads::stamped_directory(dir.clone());
            self.tree_queries.insert(dir, query);
        }
    }

    /// 경로별 원격 요청을 만들거나 저장된 결과를 표시한다. 실제 응답 반영은 apply_remote_list_dir_result에서 한다.
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
        if !refresh
            && self.loaded.as_ref() == Some(&key)
            && self.state == LoadState::Ok
            && matches!(
                self.remote_state.get(dir),
                Some(RemoteLoadState::Loaded { applied: true, .. })
            )
        {
            return;
        }

        // 응답 대기가 제한 시간을 넘으면 조회 오류로 표시한다.
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
            self.clear_selection();
            self.tree_children.clear();
        } else if refresh {
            // 펼친 트리도 다시 받는다. 응답을 기다리는 경로는 그대로 두어 중복 요청을 만들지 않는다.
            self.tree_children.clear();
            self.remote_state
                .retain(|_, state| matches!(state, RemoteLoadState::Loading { .. }));
        }

        self.state = match self.remote_state.get(dir) {
            Some(RemoteLoadState::Loading { .. }) | None => LoadState::Loading,
            Some(RemoteLoadState::Loaded { entries: raw, .. }) => {
                let mut entries = raw.clone();
                sort_entries(&mut entries, sort_column, sort_dir);
                self.set_entries(entries);
                LoadState::Ok
            }
            Some(RemoteLoadState::Error(msg)) => {
                // `msg` 가 `remote_state` 를 빌리고 있어 `set_entries` 대신 두 필드를 직접 바꾼다.
                self.entries = Default::default();
                self.hidden.forget();
                self.entries_gen += 1;
                if msg == "permission denied" {
                    LoadState::NoPermission
                } else {
                    LoadState::Error(msg.clone())
                }
            }
        };
        if let Some(RemoteLoadState::Loaded { applied, .. }) = self.remote_state.get_mut(dir) {
            *applied = true;
        }
        self.loaded = Some(key);
    }

    /// 현재 기다리는 요청 ID의 경로만 반환한다. 오래되거나 다른 요청이면 None이다.
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

    /// 다른 경로로 받은 원격 폴더 목록(주소창의 파일 확인)을 그 폴더의 캐시로 둔다. 그 폴더로 옮겼을 때
    /// 같은 목록을 다시 요청하지 않는다. 그 폴더를 기다리는 요청이 있으면 그 응답은 낡은 것으로 버려진다.
    pub(super) fn cache_remote_listing(&mut self, dir: PathBuf, entries: Vec<DirEntryInfo>) {
        self.tree_children.insert(
            dir.clone(),
            tree_dirs(entries.iter().filter(|e| e.is_dir).cloned()),
        );
        self.remote_state.insert(
            dir,
            RemoteLoadState::Loaded {
                entries,
                applied: false,
            },
        );
    }

    /// 기다리는 요청의 응답을 경로 캐시에 반영한다. 현재 디렉터리이면 본문도 갱신하며
    /// 트리와 본문이 같은 경로를 보고 있으면 같은 응답을 함께 사용한다.
    pub(crate) fn apply_remote_list_dir_result(
        &mut self,
        request_id: u64,
        panel: &ExplorerPanel,
        result: Result<Vec<DirEntryInfo>, String>,
    ) -> bool {
        self.switch_tab(panel);
        if self.answer_address_probe(request_id, &result) {
            return true;
        }
        let Some(dir) = self.find_pending_dir(request_id) else {
            let mut result = Some(result);
            let mut applied = false;
            self.for_each_parked(|view| {
                if let Some(dir) = view.find_pending_dir(request_id) {
                    view.apply_remote_tab_result(dir, result.take().expect("unique request"));
                    applied = true;
                }
            });
            return applied;
        };
        self.apply_remote_tab_result(dir, result);
        true
    }

    fn apply_remote_tab_result(&mut self, dir: PathBuf, result: Result<Vec<DirEntryInfo>, String>) {
        let is_current = self.loaded_dir() == Some(dir.as_path());
        match result {
            Ok(entries) => {
                let mut tree_children: Vec<DirEntryInfo> =
                    entries.iter().filter(|e| e.is_dir).cloned().collect();
                tree_children.sort_by_key(|a| a.name.to_lowercase());
                self.tree_children.insert(dir.clone(), tree_children);
                if is_current {
                    let (_, col, order) = self.loaded.as_ref().expect("current listing");
                    let mut sorted = entries.clone();
                    sort_entries(&mut sorted, *col, *order);
                    self.set_entries(sorted);
                    self.state = LoadState::Ok;
                    self.retain_listed_selection();
                }
                self.remote_state.insert(
                    dir,
                    RemoteLoadState::Loaded {
                        entries,
                        applied: is_current,
                    },
                );
            }
            Err(reason) => {
                if is_current {
                    self.set_entries(Vec::new());
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
    }

    /// 이번 프레임 새로 만든 원격 list_dir 요청을 모두 꺼낸다(렌더 루프 종료 후
    /// 호출자가 `CoreState::pending_list_dir_forward` 로 옮긴다).
    pub(crate) fn drain_outbox(&mut self) -> Vec<ExplorerListRequest> {
        std::mem::take(&mut self.outbox)
            .into_iter()
            .filter(|request| {
                self.pending_dir_any_tab(request.request_id).is_some()
                    || self
                        .addr_probe
                        .as_ref()
                        .is_some_and(|probe| probe.owns_request(request.request_id))
            })
            .collect()
    }

    /// 사이드바 트리에서 `dir` 의 하위 디렉토리를 (캐시에 없으면) 읽어 반환한다.
    /// 숨김 파일을 끈 동안에는 숨김 폴더를 뺀다. 캐시에는 모두 남겨 토글만으로 다시 보인다.
    /// `mirror_ws_id` 가 `Some` 이면(docs/features/remote-attach/index.md) 동기 IO 대신 `list_dir_request` 를
    /// 큐잉하고, 응답이 올 때까지 빈 목록을 반환한다(다음 프레임들에서 자동 채움).
    pub fn tree_children_of(
        &mut self,
        dir: &Path,
        mirror_ws_id: Option<u32>,
    ) -> Vec<&DirEntryInfo> {
        self.request_tree_children(dir, mirror_ws_id);
        self.tree_shown(dir)
    }

    /// 캐시에 없는 `dir` 의 하위 디렉토리 읽기를 요청하고 빈 목록을 자리로 둔다.
    fn request_tree_children(&mut self, dir: &Path, mirror_ws_id: Option<u32>) {
        if let Some(local_ws_id) = mirror_ws_id {
            if !self.tree_children.contains_key(dir) {
                if !self.remote_state.contains_key(dir) {
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
            return;
        }
        if !self.tree_children.contains_key(dir) && self.tree_queries.len() < 32 {
            self.tree_queries
                .entry(dir.to_owned())
                .or_insert_with(|| crate::app::local_reads::stamped_directory(dir.to_owned()));
            self.tree_children.insert(dir.to_owned(), Vec::new());
        }
    }

    /// 목록을 읽어 보여 주고 있는 폴더.
    pub(crate) fn loaded_dir(&self) -> Option<&Path> {
        self.loaded.as_ref().map(|(dir, _, _)| dir.as_path())
    }

    /// 단일 선택으로 설정.
    pub fn select_only(&mut self, path: &Path) {
        self.selection_identity = std::sync::Arc::new(());
        self.selected.clear();
        self.selected.insert(path.to_path_buf());
        self.anchor = Some(path.to_path_buf());
    }

    /// 토글 선택 (ctrl-click).
    pub fn toggle_select(&mut self, path: &Path) {
        self.selection_identity = std::sync::Arc::new(());
        if !self.selected.remove(path) {
            self.selected.insert(path.to_path_buf());
        }
        self.anchor = Some(path.to_path_buf());
    }

    /// 클릭 한 번의 선택 갱신. `toggle` 은 Ctrl/Cmd, `range` 는 Shift 다.
    /// Shift 는 앵커부터 대상까지 목록 순서의 범위를 고르고 앵커는 옮기지 않는다.
    /// Ctrl/Cmd 를 함께 누르면 기존 선택에 그 범위를 더한다. 앵커가 없거나 현재 목록에
    /// 없으면 Shift 를 뺀 클릭과 같다.
    pub fn click_select(&mut self, path: &Path, toggle: bool, range: bool) {
        if range && let Some(span) = self.anchor_span(path) {
            self.selection_identity = std::sync::Arc::new(());
            if !toggle {
                self.selected.clear();
            }
            self.selected.extend(span);
        } else if toggle {
            self.toggle_select(path);
        } else {
            self.select_only(path);
        }
    }

    fn anchor_span(&self, path: &Path) -> Option<Vec<PathBuf>> {
        let anchor = self.anchor.as_deref()?;
        let shown = self.shown_entries();
        let a = shown.iter().position(|e| e.path == anchor)?;
        let b = shown.iter().position(|e| e.path == path)?;
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        Some(shown[lo..=hi].iter().map(|e| e.path.clone()).collect())
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
    /// mirror workspace별 원격 홈과 응답을 기다리는 홈 조회 요청. `address::remote`가 관리한다.
    pub(super) remote_homes: HashMap<u32, PathBuf>,
    pub(super) home_probes: HashMap<u64, u32>,
    /// 서버가 원격 쓰기를 받는 mirror workspace. App 이 연결 상태에서 맞춘다.
    remote_writable: std::collections::HashSet<u32>,
    /// mirror workspace 마다 붙은 원격 Tasty. App 이 연결 상태에서 맞춘다.
    remote_hosts: HashMap<u32, crate::remote::state::ExplorerRemoteHost>,
}

impl ExplorerViewStore {
    pub(crate) fn pending_remote_times(&self) -> Vec<(u32, Instant)> {
        self.views
            .iter()
            .flat_map(|(surface, view)| {
                view.remote_state
                    .values()
                    .filter_map(move |state| match state {
                        RemoteLoadState::Loading { sent_at, .. } => Some((*surface, *sent_at)),
                        _ => None,
                    })
                    .chain(
                        view.addr_probe
                            .as_ref()
                            .and_then(|probe| probe.pending_since())
                            .map(|since| (*surface, since)),
                    )
            })
            .collect()
    }
    pub(crate) fn poll_local_reads(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) -> bool {
        let mut changed = false;
        for view in self.views.values_mut() {
            changed |= view.poll_local_reads(owner);
        }
        changed
    }
    pub(crate) fn cached_surfaces(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.views.keys().copied()
    }

    /// surface 의 뷰를 가져오고 (없으면 생성) 활성 탭 기준으로 동기화. `mirror_ws_id`
    /// 가 `Some` 이면(docs/features/remote-attach/index.md) 이 surface 가 속한 mirror workspace id — view 는 동기
    /// 로컬 IO 대신 원격 `list_dir_request` 를 큐잉한다.
    pub fn get_or_init(
        &mut self,
        panel: &ExplorerPanel,
        mirror_ws_id: Option<u32>,
    ) -> &mut ExplorerView {
        let home = mirror_ws_id.and_then(|ws| self.remote_home(ws).map(Path::to_path_buf));
        let writable = mirror_ws_id.is_none_or(|ws| self.remote_writable.contains(&ws));
        let host = mirror_ws_id.and_then(|ws| self.remote_hosts.get(&ws).cloned());
        let view = self.views.entry(panel.id).or_default();
        view.sync(panel, mirror_ws_id);
        view.remote_home = home;
        view.writes_allowed = writable;
        view.remote_host = host;
        view
    }

    pub fn get(&self, sid: SurfaceId) -> Option<&ExplorerView> {
        self.views.get(&sid)
    }

    pub fn get_mut(&mut self, sid: SurfaceId) -> Option<&mut ExplorerView> {
        self.views.get_mut(&sid)
    }

    /// (docs/features/remote-attach/index.md) `request_id` 로 대기 중인 view 를 찾아 응답을 반영한다. 어느 view도
    /// 이 `request_id` 를 기다리지 않았으면(stale) `false`.
    pub(crate) fn apply_remote_list_dir_result(
        &mut self,
        surface_id: SurfaceId,
        request_id: u64,
        panel: &ExplorerPanel,
        result: Result<(Vec<DirEntryInfo>, Option<u64>), String>,
    ) -> bool {
        let Some(view) = self.views.get_mut(&surface_id) else {
            return false;
        };
        let result = result.map(|(entries, volume)| {
            if let Some(dir) = view.pending_dir_any_tab(request_id) {
                view.note_remote_volume(dir, volume);
            }
            entries
        });
        view.apply_remote_list_dir_result(request_id, panel, result)
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

    /// 로컬 파일 작업이 바꾼 폴더를 보고 있는 View 를 다시 읽게 한다. 다시 읽은 View 가 있으면 true.
    /// `changed` 는 항목이 생기거나 사라진 폴더, `removed` 는 원래 자리에서 사라진 경로다.
    /// 사라진 경로 안을 보던 View 는 다시 읽어 읽기 오류 화면을 보인다. 다른 경로로 옮기지 않는다.
    /// 하위 폴더 검색 결과를 보이는 View 는 바뀐 경로가 시작 폴더 안이면 다시 찾는다.
    pub(crate) fn invalidate_local(&mut self, changed: &[PathBuf], removed: &[PathBuf]) -> bool {
        let hit = |dir: &Path| {
            changed.iter().any(|c| c == dir) || removed.iter().any(|r| dir.starts_with(r))
        };
        let mut any = false;
        for view in self.views.values_mut() {
            if view.mirror_ws_id.is_some() {
                continue;
            }
            // 활성이 아닌 내부 탭도 표시해 두어야 돌아왔을 때 낡은 목록에 머물지 않는다.
            view.for_each_tab(|view| {
                if view.loaded.as_ref().is_some_and(|(dir, _, _)| hit(dir)) {
                    view.request_reload();
                    any = true;
                    return;
                }
                any |= view.refind_if_changed_under(changed, removed);
                let stale: Vec<PathBuf> = view
                    .tree_children
                    .keys()
                    .filter(|dir| hit(dir))
                    .cloned()
                    .collect();
                for dir in &stale {
                    view.tree_children.remove(dir);
                    view.tree_queries.remove(dir);
                }
                any |= !stale.is_empty();
            });
        }
        any
    }

    /// 현재 뷰 개수. system.gpu_stats에서 닫힌 surface의 상태 정리를 확인할 때 쓴다.
    pub(crate) fn view_count(&self) -> usize {
        self.views.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_with(names: &[&str]) -> ExplorerView {
        let mut v = ExplorerView::new();
        v.entries = names
            .iter()
            .map(|n| DirEntryInfo {
                path: PathBuf::from(format!("/d/{n}")),
                name: (*n).into(),
                is_dir: false,
                size: 0,
                modified: None,
                ext: String::new(),
                ..Default::default()
            })
            .collect();
        v
    }

    fn selected_names(v: &ExplorerView) -> Vec<String> {
        let mut out: Vec<String> = v
            .selected
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    fn p(n: &str) -> PathBuf {
        PathBuf::from(format!("/d/{n}"))
    }

    #[test]
    fn shift_click_selects_the_span_from_the_anchor() {
        let mut v = view_with(&["a", "b", "c", "d", "e"]);
        v.click_select(&p("b"), false, false);
        v.click_select(&p("d"), false, true);
        assert_eq!(selected_names(&v), ["b", "c", "d"]);
        assert_eq!(v.anchor, Some(p("b")));
        // 앵커가 남아 있어 다른 쪽으로 다시 Shift 클릭하면 범위를 바꾼다.
        v.click_select(&p("a"), false, true);
        assert_eq!(selected_names(&v), ["a", "b"]);
    }

    #[test]
    fn ctrl_shift_click_adds_the_span_to_the_selection() {
        let mut v = view_with(&["a", "b", "c", "d", "e"]);
        v.click_select(&p("a"), false, false);
        v.click_select(&p("c"), true, false);
        v.click_select(&p("e"), true, true);
        assert_eq!(selected_names(&v), ["a", "c", "d", "e"]);
        assert_eq!(v.anchor, Some(p("c")));
    }

    #[test]
    fn shift_click_without_a_usable_anchor_is_a_plain_click() {
        let mut v = view_with(&["a", "b", "c"]);
        v.click_select(&p("b"), false, true);
        assert_eq!(selected_names(&v), ["b"]);
        assert_eq!(v.anchor, Some(p("b")));
        // 앵커가 목록에서 사라진 경우(다시 읽은 목록에 없음).
        v.anchor = Some(p("gone"));
        v.click_select(&p("c"), true, true);
        assert_eq!(selected_names(&v), ["b", "c"]);
        assert_eq!(v.anchor, Some(p("c")));
    }

    #[test]
    fn plain_and_toggle_clicks_move_the_anchor() {
        let mut v = view_with(&["a", "b", "c", "d"]);
        v.click_select(&p("a"), false, false);
        v.click_select(&p("c"), true, false);
        v.click_select(&p("d"), false, true);
        assert_eq!(selected_names(&v), ["c", "d"]);
    }

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
                ..Default::default()
            },
            DirEntryInfo {
                path: "/a".into(),
                name: "a".into(),
                is_dir: true,
                size: 0,
                modified: None,
                ext: String::new(),
                ..Default::default()
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
        view.addr_editing = true;
        view.addr_buffer = "/tmp/typed".to_string();
        panel
            .active_tab_mut()
            .navigate_to(PathBuf::from("/tmp/beta"));
        view.sync(&panel, None);
        assert_eq!(view.addr_buffer, "/tmp/typed");
    }

    /// 목록을 다시 읽어 오면 타입어헤드 버퍼가 비워지는지 확인한다. 새 폴더에서 예전
    /// 접두사가 이어지면 사용자가 입력하지 않은 글자로 검색하게 된다.
    ///
    /// 버퍼는 비공개라 값을 직접 볼 수 없어, 다음 입력이 무엇으로 검색되는지로 확인한다.
    /// 비워지지 않았다면 "ab"로 찾아 `None`이 되고, 비워졌다면 "b"로 찾아 `bravo`가 된다.
    #[test]
    fn reloading_the_listing_clears_the_type_ahead_buffer() {
        let names = vec!["alpha".to_string(), "bravo".to_string()];
        let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);

        let now = std::time::Instant::now();
        assert_eq!(view.type_ahead.feed('a', now, &names, None), Some(0));

        // 정렬만 바뀌어도 인덱스가 가리키는 항목이 달라지므로 같은 자리에서 비워야 한다.
        let mut panel = panel;
        panel.active_tab_mut().sort_dir = SortDir::Desc;
        view.sync(&panel, None);

        assert_eq!(view.type_ahead.feed('b', now, &names, None), Some(1));
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
        view.cancel_addr_edit();
        assert!(!view.addr_editing);
        view.sync(&panel, None);
        assert_eq!(view.addr_buffer, "/tmp/beta");
    }

    /// (docs/features/remote-attach/index.md) `mirror_ws_id` 가 `Some` 이면 동기 IO 대신 `list_dir_request` 를
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
        assert!(view.drain_outbox().is_empty());
    }

    /// (docs/features/remote-attach/index.md) 이 view 가 실제로 기다리던 request_id 가 아니면(stale)
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
            ..Default::default()
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

    #[test]
    fn only_a_listed_broken_link_is_reported_as_broken() {
        let mut view = ExplorerView::new();
        let entry = |name: &str, link| DirEntryInfo {
            path: PathBuf::from("/d").join(name),
            name: name.into(),
            is_dir: false,
            size: 0,
            modified: None,
            ext: String::new(),
            link,
            ..Default::default()
        };
        view.entries = vec![
            entry("gone", crate::core::fs_list::EntryLink::Broken),
            entry("ok", crate::core::fs_list::EntryLink::Valid),
        ]
        .into();
        assert!(view.is_broken_link(Path::new("/d/gone")));
        assert!(!view.is_broken_link(Path::new("/d/ok")));
        assert!(!view.is_broken_link(Path::new("/d/other")));
    }

    #[test]
    fn an_explicit_reload_of_the_same_folder_rereads_the_tree() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/tmp/alpha"));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);
        view.tree_children
            .insert(PathBuf::from("/tmp/alpha"), Vec::new());

        view.sync(&panel, None);
        assert!(
            !view.tree_children.is_empty(),
            "an unchanged sync keeps the tree"
        );

        view.request_reload();
        view.sync(&panel, None);
        assert!(view.tree_children.is_empty());
    }

    #[test]
    fn a_remote_refresh_rereads_the_tree_but_keeps_requests_in_flight() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let mut view = ExplorerView::new();
        view.sync(&panel, Some(7));
        let id = match view.remote_state.get(panel.current_root()) {
            Some(RemoteLoadState::Loading { request_id, .. }) => *request_id,
            _ => panic!("expected a loading request"),
        };
        assert!(view.apply_remote_list_dir_result(id, &panel, Ok(Vec::new())));
        view.remote_state.insert(
            "/remote/project/sub".into(),
            RemoteLoadState::Loaded {
                entries: Vec::new(),
                applied: false,
            },
        );
        view.tree_children
            .insert("/remote/project/sub".into(), Vec::new());
        view.remote_state.insert(
            "/remote/project/pending".into(),
            RemoteLoadState::Loading {
                request_id: 99,
                sent_at: Instant::now(),
            },
        );
        drop(view.drain_outbox());

        view.request_reload();
        view.sync(&panel, Some(7));
        assert!(view.tree_children.is_empty());
        assert!(
            !view
                .remote_state
                .contains_key(Path::new("/remote/project/sub"))
        );
        assert!(
            view.remote_state
                .contains_key(Path::new("/remote/project/pending"))
        );
        assert_eq!(
            view.drain_outbox().len(),
            1,
            "only the current folder is re-requested"
        );
    }

    #[test]
    fn a_reconnected_mirror_rereads_folders_the_old_connection_left_behind() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/remote/project"));
        let other = ExplorerPanel::new(2, PathBuf::from("/remote/other"));
        let mut store = ExplorerViewStore::default();
        drop(store.get_or_init(&other, Some(8)).drain_outbox());
        let view = store.get_or_init(&panel, Some(7));
        drop(view.drain_outbox());
        // 옛 연결에서 시간 초과로 오류가 된 지금 폴더와, 응답이 오지 않을 옛 요청.
        view.remote_state.insert(
            "/remote/project".into(),
            RemoteLoadState::Error("timed out".into()),
        );
        view.remote_state.insert(
            "/remote/project/pending".into(),
            RemoteLoadState::Loading {
                request_id: 99,
                sent_at: Instant::now(),
            },
        );
        let view = store.get_or_init(&panel, Some(7));
        assert!(matches!(view.state, LoadState::Error(_)));
        assert!(view.drain_outbox().is_empty());

        assert!(store.reload_mirror(7));
        let view = store.get_or_init(&panel, Some(7));
        assert_eq!(view.state, LoadState::Loading);
        assert_eq!(view.drain_outbox().len(), 1, "the folder is asked again");
        assert!(
            !view
                .remote_state
                .contains_key(Path::new("/remote/project/pending")),
            "a request sent on the old connection is forgotten"
        );
        let untouched = store.get_or_init(&other, Some(8));
        assert!(
            untouched.drain_outbox().is_empty(),
            "another mirror workspace is left alone"
        );
    }

    #[test]
    fn converting_away_and_back_under_the_same_id_shows_the_new_panels_preview() {
        use crate::model::ExplorerPreview;
        use tasty_type_geometry::length::LogicalPx;
        let mut store = ExplorerViewStore::default();
        let mut saved = ExplorerPanel::new(5, PathBuf::from("/w"));
        saved.preview = ExplorerPreview {
            open: true,
            width: Some(LogicalPx(428.0)),
        };
        assert!(store.get_or_init(&saved, None).preview.open);

        // terminal 로 바꾸는 동안 같은 id 의 view 는 남는다. explorer 로 돌아오면 새 panel 은 기본값이다.
        let fresh = ExplorerPanel::new(5, PathBuf::from("/w"));
        let view = store.get_or_init(&fresh, None);
        assert!(!view.preview.open, "the view follows the new panel");
        assert_eq!(view.preview.take_change(), None);
        // 그 뒤 사용자가 켜면 새 panel 의 폭(기본)으로 남긴다.
        view.preview.toggle();
        assert_eq!(
            view.preview.take_change(),
            Some(ExplorerPreview {
                open: true,
                width: None,
            })
        );
    }

    #[test]
    fn a_local_file_action_reloads_every_local_view_of_the_changed_folders() {
        let mut store = ExplorerViewStore::default();
        let mut sync = |sid, root: &str, mirror| {
            let panel = ExplorerPanel::new(sid, PathBuf::from(root));
            store.get_or_init(&panel, mirror);
        };
        sync(1, "/w/dest", None);
        sync(2, "/w/gone/inner", None);
        sync(3, "/w/elsewhere", None);
        sync(4, "/w/dest", Some(7));
        let tree_view = store.get_mut(3).unwrap();
        tree_view.tree_children.insert("/w/dest".into(), Vec::new());
        tree_view
            .tree_children
            .insert("/w/untouched".into(), Vec::new());
        let before = store.get(4).unwrap().reload_requested;

        assert!(store.invalidate_local(&["/w/dest".into()], &["/w/gone".into()]));
        assert!(store.get(1).unwrap().reload_requested, "same folder");
        assert!(
            store.get(2).unwrap().reload_requested,
            "inside a removed folder"
        );
        let tree_view = store.get(3).unwrap();
        assert!(!tree_view.reload_requested);
        assert!(!tree_view.tree_children.contains_key(Path::new("/w/dest")));
        assert!(
            tree_view
                .tree_children
                .contains_key(Path::new("/w/untouched"))
        );
        assert_eq!(
            store.get(4).unwrap().reload_requested,
            before,
            "a mirror view shows remote files"
        );
        assert!(!store.invalidate_local(&["/w/none".into()], &[]));
    }

    #[test]
    fn go_up_from_a_vanished_folder_skips_vanished_parents() {
        use super::super::ExplorerAction as A;
        let root = Path::new("/w/a/b/c");
        let mut view = ExplorerView::new();
        assert!(
            matches!(view.go_up_action(root), A::GoUp),
            "unknown ancestor"
        );
        view.existing_ancestor = Some("/w/a/b".into());
        assert!(
            matches!(view.go_up_action(root), A::GoUp),
            "the parent exists"
        );
        view.existing_ancestor = Some("/w".into());
        assert!(matches!(view.go_up_action(root), A::Navigate(p) if p == Path::new("/w")));

        // 새로 읽기 시작하면 이전 오류의 상위 폴더를 쓰지 않는다.
        let panel = ExplorerPanel::new(1, root.to_path_buf());
        view.sync(&panel, None);
        assert!(matches!(view.go_up_action(root), A::GoUp));
    }

    #[test]
    fn a_reread_listing_drops_selected_paths_that_are_gone() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["keep", "gone"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let panel = ExplorerPanel::new(1, dir.path().into());
        let mut view = ExplorerView::new();
        let mut owner = crate::app::local_reads::LocalReads::default();
        let mut load = |view: &mut ExplorerView| {
            view.sync(&panel, None);
            owner.drive(|requests| view.poll_local_reads(requests));
        };
        load(&mut view);
        view.select_only(&dir.path().join("keep"));
        view.toggle_select(&dir.path().join("gone"));
        let identity = view.selection_identity();

        std::fs::remove_file(dir.path().join("gone")).unwrap();
        view.request_reload();
        load(&mut view);

        assert_eq!(
            view.selected,
            HashSet::from([dir.path().join("keep")]),
            "the status line counts only listed items"
        );
        assert_eq!(view.anchor, None, "the anchor was the removed item");
        assert!(
            view.matches_selection(&identity),
            "the rest is the same selection"
        );
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_remote_reply_drops_selected_paths_that_are_gone() {
        let panel = ExplorerPanel::new(1, PathBuf::from("/d"));
        let entries = |names: &[&str]| view_with(names).entries;
        let mut view = ExplorerView::new();
        let reply = |view: &mut ExplorerView, names: &[&str]| {
            view.sync(&panel, Some(7));
            let id = view.drain_outbox()[0].request_id;
            assert!(view.apply_remote_list_dir_result(id, &panel, Ok(entries(names).into_vec())));
        };
        reply(&mut view, &["a", "b"]);
        view.select_only(Path::new("/d/a"));
        view.toggle_select(Path::new("/d/b"));

        view.request_reload();
        reply(&mut view, &["a"]);
        assert_eq!(view.selected, HashSet::from([PathBuf::from("/d/a")]));
    }
}
