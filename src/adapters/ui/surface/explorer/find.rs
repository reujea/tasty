//! Find 바 — 지금 폴더를 이름으로 거르고, Subfolders 를 켜면 하위 폴더까지 찾는다.
//! 바는 툴바 아래에 붙고, 열어 둔 폴더를 떠나면(뒤로·위로·주소창·폴더 열기) 닫힌다.
//! 하위 폴더 검색은 로컬 읽기 worker 가 하고 결과는 들어오는 대로 목록을 채운다. 원격 explorer 는 거르기만 한다.

use std::path::{Path, PathBuf};

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    ExplorerFindBar, ExplorerFindEvents, ExplorerFindLabels, ExplorerFindStatus, explorer_find_bar,
    explorer_match_job,
};

use super::DirEntryInfo;
use super::view::ExplorerView;
use crate::app::local_reads::{SEARCH_MAX_HITS, SearchEvent, SearchQuery};
use std::cmp::Ordering;

use crate::core::fs_list::find_query::FindQuery;
use crate::core::fs_list::{compare_entries, sort_entries};
use crate::i18n::{t, t_count, t_fmt, t_fmt2};
use tasty_model::{SortColumn, SortDir};

/// 열려 있는 Find 바. 연 폴더를 함께 둔다.
pub(crate) struct FindState {
    root: PathBuf,
    query: String,
    matcher: FindQuery,
    show_hidden: bool,
    deep: bool,
    /// 다음 프레임에 입력에 포커스를 준다.
    focus: bool,
    /// 입력 칸이 키를 받고 있다. 그동안 타입어헤드와 목록 단축키를 멈춘다.
    pub(crate) field_focused: bool,
    search: Option<Search>,
    filtered: Option<Filtered>,
}

/// 이름으로 거른 결과의 목록 번호. 만든 때의 검색어와 목록(세대·주소·길이)을 함께 두어,
/// 둘 다 그대로일 때만 쓴다. 그래서 프레임마다 목록 전체를 다시 거르지 않는다.
struct Filtered {
    query: String,
    list: ListKey,
    hits: Vec<usize>,
}

type ListKey = (u64, usize, usize);

struct Search {
    receipt: Option<SearchQuery>,
    hits: Vec<DirEntryInfo>,
    skipped: Vec<PathBuf>,
    outcome: Outcome,
}

#[derive(Clone, PartialEq)]
enum Outcome {
    Running,
    Done,
    Stopped,
    /// 결과가 `SEARCH_MAX_HITS` 에 닿아 멈췄다. 찾은 결과는 남긴다.
    Capped,
    Failed(String),
}

/// 목록 대신 보일 Find 상태 화면.
pub(super) enum FindScreen {
    /// 거르기에 맞는 이름이 없다. 바는 "0 of N" 을 그대로 보인다.
    NoFilterMatches {
        query: String,
    },
    /// 하위 폴더까지 찾았지만 맞는 항목이 없다.
    NoSearchMatches {
        folder: String,
    },
    Failed(String),
}

impl ExplorerView {
    /// Find 바를 열고 입력에 포커스를 준다. 이미 열려 있으면 포커스만 준다.
    pub(crate) fn open_find(&mut self) {
        let Some(root) = self.shown_dir().map(Path::to_path_buf) else {
            return;
        };
        match &mut self.find {
            Some(find) => find.focus = true,
            None => {
                self.find = Some(FindState {
                    root,
                    query: String::new(),
                    matcher: FindQuery::default(),
                    show_hidden: self.hidden.show,
                    deep: false,
                    focus: true,
                    field_focused: false,
                    search: None,
                    filtered: None,
                })
            }
        }
    }

    /// 툴바 Find 토글. 열려 있으면 닫는다.
    pub(crate) fn toggle_find(&mut self) {
        if self.find.is_some() {
            self.find = None;
        } else {
            self.open_find();
        }
    }

    /// 지금 폴더를 떠났으면 Find 바를 닫는다. 새로고침이면 하위 폴더 검색을 다시 한다.
    pub(super) fn sync_find(&mut self, root: &Path, reload: bool) {
        if self.find.as_ref().is_some_and(|f| f.root != root) {
            self.find = None;
        }
        if reload && let Some(find) = &mut self.find {
            find.restart();
        }
    }

    /// 목록에 보일 항목의 사본. 목록을 그리는 쪽과 선택 명령만 쓴다.
    /// 매 프레임 부르는 상태줄·바·상태 화면은 `shown` 과 `shown_count` 로 복사 없이 본다.
    pub(crate) fn shown_entries(&self) -> Vec<DirEntryInfo> {
        self.shown().cloned().collect()
    }

    /// 목록에 보일 항목. 거르는 중이면 맞는 항목만, 하위 폴더 검색이면 그 결과다.
    pub(crate) fn shown(&self) -> Box<dyn Iterator<Item = &DirEntryInfo> + '_> {
        if let Some(hits) = self.filtered() {
            return Box::new(hits.iter().map(|&i| &self.entries[i]));
        }
        let (list, query): (&[DirEntryInfo], Option<&FindQuery>) = match &self.find {
            Some(FindState {
                search: Some(search),
                query,
                ..
            }) if !query.is_empty() => (&search.hits, None),
            Some(find) if !find.deep => (&self.entries, Some(&find.matcher)),
            _ => (&self.entries, None),
        };
        Box::new(
            list.iter()
                .filter(move |e| query.is_none_or(|q| q.matches(e))),
        )
    }

    /// 고른 항목을 목록에 보이는 순서로. 하위 폴더 검색 중이면 그 결과에서 고른다.
    /// 선택은 보이는 항목에만 남으므로 고른 항목은 모두 여기 있다.
    pub(crate) fn selected_shown(&self) -> impl Iterator<Item = &DirEntryInfo> + '_ {
        self.shown().filter(|e| self.selected.contains(&e.path))
    }

    /// 거르는 중이고 지금 검색어·목록으로 만든 거르기 결과가 있으면 그 목록 번호.
    pub(super) fn filtered(&self) -> Option<&[usize]> {
        let find = self.find.as_ref()?;
        let filtered = find.filtered.as_ref()?;
        (self.shown_slice().is_none()
            && filtered.query == find.query
            && filtered.list == self.list_key())
        .then_some(filtered.hits.as_slice())
    }

    fn list_key(&self) -> ListKey {
        let addr = self.entries.as_ptr() as usize;
        (self.entries_gen, addr, self.entries.len())
    }

    /// 거르는 중이면 지금 검색어와 목록으로 거른 결과를 맞춰 둔다. 둘 다 그대로면 다시 거르지
    /// 않는다. 맞춰 두지 못한 프레임에는 `shown` 이 그 자리에서 거른다.
    pub(super) fn refresh_filter(&mut self) {
        let key = self.list_key();
        let filtering = self.shown_slice().is_none();
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if !filtering {
            find.filtered = None;
            return;
        }
        if find
            .filtered
            .as_ref()
            .is_some_and(|f| f.query == find.query && f.list == key)
        {
            return;
        }
        let hits = (self.entries.iter().enumerate())
            .filter(|(_, e)| find.matcher.matches(e))
            .map(|(i, _)| i)
            .collect();
        find.filtered = Some(Filtered {
            query: find.query.clone(),
            list: key,
            hits,
        });
    }

    /// 보일 항목 수. 거르지 않으면 이름을 보지 않고 센다.
    pub(crate) fn shown_count(&self) -> usize {
        match self.shown_slice() {
            Some(list) => list.len(),
            None => self
                .filtered()
                .map_or_else(|| self.shown().count(), <[usize]>::len),
        }
    }

    /// 보이는 목록 번호로 항목을 빌린다. 필터 캐시가 있으면 원본 번호로 바로 조회한다.
    pub(super) fn shown_at(&self, index: usize) -> Option<&DirEntryInfo> {
        if let Some(list) = self.shown_slice() {
            return list.get(index);
        }
        if let Some(hits) = self.filtered() {
            return hits.get(index).and_then(|&i| self.entries.get(i));
        }
        self.shown().nth(index)
    }

    /// 표시 순서의 이름을 빌린다. 목록·필터 캐시는 인덱스로 조회한다.
    pub(super) fn shown_names(&self) -> impl ExactSizeIterator<Item = &str> + Clone {
        (0..self.shown_count()).map(|i| self.shown_at(i).expect("shown index").name.as_str())
    }

    /// 보일 항목 가운데 `range` 자리의 사본. 목록은 보이는 행만 복제해 그린다.
    /// 이름으로 거르지 않거나 거른 결과가 맞춰져 있으면 목록을 훑지 않는다.
    pub(crate) fn shown_range(&self, range: std::ops::Range<usize>) -> Vec<DirEntryInfo> {
        match self.shown_slice() {
            Some(list) => {
                let end = range.end.min(list.len());
                list[range.start.min(end)..end].to_vec()
            }
            None => self.filtered_range(range.clone()).unwrap_or_else(|| {
                (self.shown().skip(range.start).take(range.len()))
                    .cloned()
                    .collect()
            }),
        }
    }

    /// `shown_range` 의 거르는 중 경로. 거른 결과가 있으면 그 번호로 바로 고른다.
    fn filtered_range(&self, range: std::ops::Range<usize>) -> Option<Vec<DirEntryInfo>> {
        let hits = self.filtered()?;
        let end = range.end.min(hits.len());
        Some(
            hits[range.start.min(end)..end]
                .iter()
                .map(|&i| self.entries[i].clone())
                .collect(),
        )
    }

    /// 이름으로 거르지 않고 그대로 보이는 목록. 거르는 중이면 `None`.
    pub(crate) fn shown_slice(&self) -> Option<&[DirEntryInfo]> {
        match &self.find {
            Some(FindState {
                search: Some(search),
                query,
                ..
            }) if !query.is_empty() => Some(&search.hits),
            Some(find) if !find.deep && !find.query.is_empty() => None,
            _ => Some(&self.entries),
        }
    }

    /// 하위 폴더 검색 결과. 검색 중이 아니면 비어 있다.
    pub(super) fn search_hits(&self) -> &[DirEntryInfo] {
        self.find
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .map_or(&[], |s| s.hits.as_slice())
    }

    fn search_running_for_perf(&self) -> bool {
        self.find
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .is_some_and(|s| s.outcome == Outcome::Running)
    }

    /// 이름과 확장자 조건을 모두 포함한 필터 캐시 키.
    pub(super) fn find_query_key(&self) -> &str {
        self.find.as_ref().map_or("", |f| f.query.as_str())
    }

    /// 이름에서 칠할 검색어. 거르거나 찾는 중이 아니면 빈 문자열이다.
    pub(super) fn find_query(&self) -> &str {
        self.find.as_ref().map_or("", |f| f.matcher.text())
    }

    /// 하위 폴더 검색 결과를 보이는 중이면 시작 폴더.
    pub(super) fn search_root(&self) -> Option<&Path> {
        self.find
            .as_ref()
            .filter(|f| f.search.is_some())
            .map(|f| f.root.as_path())
    }

    /// 상태줄 글자.
    pub(super) fn status_text(&self) -> String {
        let sel = self.selected.len();
        if let Some(root) = self.search_root()
            && sel == 1
            && let Some(path) = self.selected.iter().next()
        {
            return relative_path(root, path);
        }
        if sel > 0 {
            return t_fmt("explorer.status.selected", &sel.to_string());
        }
        match &self.find {
            Some(FindState {
                search: Some(search),
                ..
            }) if search.outcome == Outcome::Capped => capped_text("explorer.find.capped_hint"),
            Some(find) if find.search.is_some() => {
                t_fmt("explorer.find.found", &self.shown_count().to_string())
            }
            Some(find) if !find.query.is_empty() => t_count(
                "explorer.find.status",
                self.entries.len() as u64,
                &[
                    &self.shown_count().to_string(),
                    &self.entries.len().to_string(),
                ],
            ),
            _ => {
                let n = self.entries.len();
                self.hidden_status(t_count(
                    "explorer.status.items",
                    n as u64,
                    &[&n.to_string()],
                ))
            }
        }
    }

    /// 목록 대신 보일 상태 화면이 있으면.
    pub(super) fn find_screen(&self) -> Option<FindScreen> {
        let find = self.find.as_ref()?;
        if find.query.is_empty() {
            return None;
        }
        let folder = find.root.file_name().map_or_else(
            || find.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        match &find.search {
            Some(search) => match &search.outcome {
                Outcome::Failed(msg) => Some(FindScreen::Failed(msg.clone())),
                Outcome::Done if search.hits.is_empty() => {
                    Some(FindScreen::NoSearchMatches { folder })
                }
                _ => None,
            },
            None if !self.entries.is_empty() && self.shown().next().is_none() => {
                Some(FindScreen::NoFilterMatches {
                    query: find.query.clone(),
                })
            }
            None => None,
        }
    }

    /// 하위 폴더 검색 결과를 받는다. 받은 것이 있으면 true.
    pub(super) fn poll_find(&mut self, owner: &mut crate::app::local_reads::ReadRequests) -> bool {
        let sort = self.loaded.as_ref().map(|(_, col, dir)| (*col, *dir));
        let Some(search) = self.find.as_mut().and_then(|f| f.search.as_mut()) else {
            return false;
        };
        let Some(receipt) = search.receipt.as_mut() else {
            return false;
        };
        let events = receipt.poll(owner);
        if events.is_empty() {
            return false;
        }
        self.path_index.borrow_mut().clear();
        for event in events {
            match event {
                SearchEvent::Hits(batch) => match sort {
                    Some((col, dir)) => insert_sorted(&mut search.hits, batch, col, dir),
                    None => search.hits.extend(batch),
                },
                SearchEvent::Skipped(dir) => search.skipped.push(dir),
                SearchEvent::Failed(msg) => search.outcome = Outcome::Failed(msg),
                SearchEvent::Done { stopped, capped } => {
                    search.outcome = if stopped {
                        Outcome::Stopped
                    } else if capped {
                        Outcome::Capped
                    } else {
                        Outcome::Done
                    };
                }
            }
        }
        if search.outcome != Outcome::Running {
            search.receipt = None;
        }
        true
    }
}

impl ExplorerView {
    /// 하위 폴더 검색 결과를 보이는 중이고 파일 작업이 바꾼 경로가 시작 폴더 안에 있으면 처음부터 다시 찾는다.
    /// 결과는 하위 폴더의 항목이라 지금 폴더 목록을 다시 읽는 조건으로는 잡히지 않는다. 다시 찾았으면 true.
    pub(super) fn refind_if_changed_under(
        &mut self,
        changed: &[PathBuf],
        removed: &[PathBuf],
    ) -> bool {
        let Some(find) = self.find.as_mut().filter(|f| f.search.is_some()) else {
            return false;
        };
        let under = changed
            .iter()
            .chain(removed)
            .any(|p| p.starts_with(&find.root));
        if under {
            find.restart();
        }
        under
    }

    /// 숨김 정책이 바뀌면 기존 영수증을 버려 취소하고, 그 채널의 늦은 결과도 차단한다.
    pub(super) fn refind_for_hidden(&mut self, show: bool) {
        if let Some(find) = &mut self.find {
            find.show_hidden = show;
            find.restart();
        }
    }

    /// 시험: Find 바를 열고 하위 폴더 검색을 시작한다.
    #[cfg(test)]
    pub(crate) fn search_subfolders(&mut self, query: &str) {
        self.open_find();
        let find = self.find.as_mut().expect("a loaded folder opens the bar");
        find.query = query.into();
        find.deep = true;
        find.restart();
    }

    /// 시험: 하위 폴더 검색이 아직 진행 중인가.
    #[cfg(test)]
    pub(crate) fn search_running(&self) -> bool {
        self.find
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .is_some_and(|s| s.outcome == Outcome::Running)
    }
}

#[cfg(test)]
impl FindState {
    /// 시험: 입력한 글자.
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// 시험: 글자를 바꾼다(바 입력과 같다).
    pub(crate) fn set_query(&mut self, query: &str) {
        self.query = query.into();
        self.matcher = FindQuery::parse(&self.query);
    }

    /// 시험: 하위 폴더 검색이 끝나 더 받을 결과가 없다.
    pub(crate) fn search_finished(&self) -> bool {
        self.search.as_ref().is_some_and(|s| s.receipt.is_none())
    }
}

impl FindState {
    pub(super) fn has_pending(&self) -> bool {
        self.search
            .as_ref()
            .is_some_and(|search| search.receipt.is_some())
    }

    pub(super) fn leave_tab(&mut self) {
        self.focus = false;
        self.field_focused = false;
    }

    /// 하위 폴더 검색이 켜져 있고 검색어가 있으면 처음부터 다시 찾는다. 아니면 검색을 버린다.
    fn restart(&mut self) {
        self.matcher = FindQuery::parse(&self.query);
        self.search = (self.deep && !self.query.is_empty()).then(|| Search {
            receipt: Some(crate::app::local_reads::search(
                self.root.clone(),
                self.query.clone(),
                self.show_hidden,
            )),
            hits: Vec::new(),
            skipped: Vec::new(),
            outcome: Outcome::Running,
        });
    }
}

/// 상한 문구. 번역문은 이름 붙은 `{n}` 자리로 상한(세 자리 쉼표)을 받는다.
fn capped_text(key: &str) -> String {
    t(key).replace(
        "{n}",
        &crate::core::fs_list::group_digits(SEARCH_MAX_HITS as u64),
    )
}

/// 정렬한 결과에 새 묶음을 끼워 넣는다. 묶음만 정렬하고 자리는 이분 탐색으로 찾아,
/// 결과가 많아도 받을 때마다 전체를 다시 정렬하지 않는다. 같은 순서의 항목은 먼저 온 것이 앞이다.
fn insert_sorted(
    hits: &mut Vec<DirEntryInfo>,
    mut batch: Vec<DirEntryInfo>,
    col: SortColumn,
    dir: SortDir,
) {
    sort_entries(&mut batch, col, dir);
    let at: Vec<usize> = batch
        .iter()
        .map(|b| hits.partition_point(|h| compare_entries(h, b, col, dir) != Ordering::Greater))
        .collect();
    let old = std::mem::replace(hits, Vec::with_capacity(hits.len() + batch.len()));
    let mut old = old.into_iter();
    let mut moved = 0;
    for (pos, entry) in at.into_iter().zip(batch) {
        hits.extend(old.by_ref().take(pos - moved));
        moved = pos;
        hits.push(entry);
    }
    hits.extend(old);
}

/// 시작 폴더 기준 상대 경로. 화면에 보이는 로컬 경로라 OS 구분자를 그대로 쓴다.
fn relative_path(start: &Path, path: &Path) -> String {
    path.strip_prefix(start)
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
}

/// 하위 폴더 검색 결과가 든 폴더. 시작 폴더 자신은 ".".
pub(super) fn hit_folder(root: &Path, hit: &Path) -> String {
    crate::app::local_reads::relative_folder(root, hit)
}

/// 이름 글자 배치. 검색어와 맞는 부분을 accent-primary-raised 로 칠한다. 검색어가 없으면 한 색이다.
/// 선택 행은 일치 구간도 선택 색이다.
pub(super) fn name_job(
    theme: &Theme,
    name: &str,
    query: &str,
    font: egui::FontId,
    color: egui::Color32,
    line_height: Option<f32>,
    selected: bool,
) -> egui::text::LayoutJob {
    let mut job = explorer_match_job(theme, name, query, font, color, selected);
    for section in &mut job.sections {
        section.format.line_height = line_height;
    }
    job
}

impl ExplorerView {
    /// List · Grid 의 하위 폴더 검색 결과는 든 폴더를 툴팁으로 보인다.
    pub(super) fn hit_tooltip(&self, entry: &DirEntryInfo, resp: egui::Response) -> egui::Response {
        match self.search_root() {
            Some(root) => resp.on_hover_text(
                super::display_text::nfc(&hit_folder(root, &entry.path)).into_owned(),
            ),
            None => resp,
        }
    }
}

/// 툴바 아래 Find 바를 그린다. 원격 explorer 는 Subfolders 를 숨긴다.
pub(super) fn bar(ui: &mut egui::Ui, theme: &Theme, view: &mut ExplorerView, remote: bool) {
    view.refresh_filter();
    let shown = view.shown_count();
    let total = view.entries.len();
    tracing::trace!(target: "tasty::explorer::perf", tab = view.tab_salt(),
        entries = total, shown, selected = view.selected.len(), thumbnails = view.thumbs.cached(),
        searching = view.search_running_for_perf(), find_open = view.find.is_some(), "explorer_state");
    // 입력 칸의 egui 상태(포커스·커서)를 내부 탭마다 따로 둔다.
    let salt = ("explorer_find", view.tab_salt());
    let Some(find) = view.find.as_mut() else {
        return;
    };
    let folder = find.root.file_name().map_or_else(
        || find.root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let placeholder = if find.deep {
        t_fmt("explorer.find.search_placeholder", &folder)
    } else {
        t("explorer.find.filter_placeholder").to_string()
    };
    let (text, skipped, tooltip) = status_parts(find, shown, total);
    let status = match &find.search {
        Some(search) if search.outcome == Outcome::Running => ExplorerFindStatus::Searching {
            text: &text,
            stop: t("explorer.find.stop"),
        },
        Some(search) if !search.skipped.is_empty() => ExplorerFindStatus::Skipped {
            text: &text,
            skipped: &skipped,
            tooltip: &tooltip,
        },
        _ => ExplorerFindStatus::Text(&text),
    };
    let focus = std::mem::take(&mut find.focus);
    let mut deep = find.deep;
    let events = ui
        .push_id(salt, |ui| {
            explorer_find_bar(
                ui,
                theme,
                ExplorerFindBar {
                    query: &mut find.query,
                    subfolders: (!remote).then_some(&mut deep),
                    status,
                    labels: ExplorerFindLabels {
                        placeholder: &placeholder,
                        subfolders: t("explorer.find.subfolders"),
                        close: t("explorer.find.close"),
                    },
                    focus,
                },
            )
        })
        .inner;
    view.apply_find_events(events, deep);
    view.refresh_filter();
}

impl ExplorerView {
    /// Find 바에서 일어난 일을 반영한다. `deep` 은 바가 돌려준 Subfolders 값이다.
    pub(super) fn apply_find_events(&mut self, events: ExplorerFindEvents, deep: bool) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        find.field_focused = events.field_focused;
        if events.close || (events.escape && find.query.is_empty()) {
            self.find = None;
            return;
        }
        if events.escape {
            // 첫 Esc 는 글자만 지우고 입력을 계속 쓸 수 있게 둔다.
            find.query.clear();
            find.matcher = FindQuery::default();
            find.focus = true;
            find.search = None;
            return;
        }
        if events.stop
            && let Some(receipt) = find.search.as_ref().and_then(|s| s.receipt.as_ref())
        {
            receipt.stop();
        }
        if events.subfolders_toggled || events.changed {
            find.deep = deep;
            find.restart();
            self.refresh_filter();
            self.retain_shown_selection();
        }
    }

    /// 실제 보이는 결과에서 선택을 남긴다. 확장자 조건으로 빠진 폴더에도 명령이 닿지 않는다.
    fn retain_shown_selection(&mut self) {
        if self.selected.is_empty() && self.anchor.is_none() {
            return;
        }
        let shown: std::collections::HashSet<PathBuf> = self
            .shown()
            .filter(|e| self.selected.contains(&e.path) || self.anchor.as_ref() == Some(&e.path))
            .map(|e| e.path.clone())
            .collect();
        self.selected.retain(|p| shown.contains(p));
        if self.anchor.as_ref().is_some_and(|p| !shown.contains(p)) {
            self.anchor = None;
        }
    }
}

/// (상태 글자, 건너뛴 폴더 글자, 건너뛴 폴더 목록 툴팁).
fn status_parts(find: &FindState, shown: usize, total: usize) -> (String, String, String) {
    let Some(search) = &find.search else {
        let text = if find.query.is_empty() {
            String::new()
        } else {
            t_fmt2(
                "explorer.find.count",
                &shown.to_string(),
                &total.to_string(),
            )
        };
        return (text, String::new(), String::new());
    };
    let n = search.hits.len().to_string();
    let text = match search.outcome {
        Outcome::Running => t_fmt("explorer.find.searching", &n),
        Outcome::Done => t_fmt("explorer.find.found", &n),
        Outcome::Stopped => t_fmt("explorer.find.stopped", &n),
        Outcome::Capped => capped_text("explorer.find.capped"),
        Outcome::Failed(_) => "—".to_string(),
    };
    let n = search.skipped.len();
    let skipped = t_count("explorer.find.skipped", n as u64, &[&n.to_string()]);
    let tooltip = search
        .skipped
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    (text, skipped, tooltip)
}

#[cfg(test)]
mod tests;
