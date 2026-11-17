//! App-owned bounded local reads. Views keep receipts; discarded receipts cannot update a new View.
use crate::core::fs_list::DirEntryInfo;
use crate::i18n::t;
use crate::state::branch::HeadState;
use crate::state::{FpLoadState, MainViewState};
use crate::view::ui::View;
use std::{
    io,
    path::PathBuf,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

mod file_info;
pub(crate) use file_info::{
    FolderCount, ItemFacts, ItemKind, LinkTarget, MAX_DECODE_ALLOC, MAX_IMAGE_SIDE,
    PREVIEW_MAX_BYTES, PreviewData, PropertiesFacts, TooLarge, decodable_image_ext,
};
mod dir_stamp;
pub(crate) use dir_stamp::{DirStamp, StampCheck, StampedListing};
mod search;
pub(crate) use search::{SEARCH_MAX_HITS, SearchEvent, SearchQuery, relative_folder, search};

const MAX_RUNNING: usize = 4;
const MAX_SCRIPT_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) struct Query<T> {
    request: Option<Request>,
    receiver: mpsc::Receiver<io::Result<T>>,
}
impl<T> Query<T> {
    fn new(make: impl FnOnce(mpsc::SyncSender<io::Result<T>>) -> Request) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        Self {
            request: Some(make(sender)),
            receiver,
        }
    }
    pub(crate) fn poll(&mut self, requests: &mut ReadRequests) -> Option<io::Result<T>> {
        if let Some(request) = self.request.take()
            && let Err(request) = requests.push(request)
        {
            self.request = Some(request);
        }
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(io::Error::other("local read worker disconnected")))
            }
        }
    }
}

/// A bounded value outbox, not a borrowed worker or execution capability.
pub(crate) struct ReadRequests {
    pending: Vec<Request>,
    available: usize,
}
impl ReadRequests {
    fn push(&mut self, request: Request) -> Result<(), Request> {
        if self.pending.len() >= self.available {
            return Err(request);
        }
        self.pending.push(request);
        Ok(())
    }
}

enum Request {
    Directory(PathBuf, mpsc::SyncSender<io::Result<Vec<DirEntryInfo>>>),
    StampedDirectory(PathBuf, mpsc::SyncSender<io::Result<StampedListing>>),
    DirStamps(
        Vec<PathBuf>,
        Option<PathBuf>,
        mpsc::SyncSender<io::Result<StampCheck>>,
    ),
    Git(PathBuf, mpsc::SyncSender<io::Result<Option<HeadState>>>),
    Script(PathBuf, mpsc::SyncSender<io::Result<ScriptSource>>),
    Preview(PathBuf, u32, mpsc::SyncSender<io::Result<PreviewData>>),
    Thumbnail(PathBuf, mpsc::SyncSender<io::Result<egui::ColorImage>>),
    Properties(
        PropertiesRead,
        mpsc::SyncSender<io::Result<PropertiesFacts>>,
    ),
    Writable(PathBuf, mpsc::SyncSender<io::Result<bool>>),
    /// 하위 폴더 검색. 오래 걸리므로 결과를 나눠 보내며 그때마다 화면을 깨운다.
    Search(search::SearchSpec, mpsc::SyncSender<SearchEvent>),
    #[cfg(test)]
    Blocked(mpsc::Receiver<()>, mpsc::SyncSender<io::Result<()>>),
}
impl Request {
    fn run(self, wake: &dyn Fn()) {
        let delivered = match self {
            Self::Search(spec, sender) => {
                search::run(spec, sender, wake);
                true
            }
            #[cfg(test)]
            Self::Blocked(wait, sender) => {
                sender.send(wait.recv().map_err(io::Error::other)).is_ok()
            }
            Self::Directory(path, sender) => sender
                .send(crate::core::fs_list::read_dir_entries(&path).map_err(|e| missing(&path, e)))
                .is_ok(),
            Self::StampedDirectory(path, sender) => sender
                .send(dir_stamp::read_stamped(&path, |dir| {
                    crate::core::fs_list::read_dir_entries(dir).map_err(|e| missing(dir, e))
                }))
                .is_ok(),
            Self::DirStamps(dirs, entries_of, sender) => sender
                .send(Ok(dir_stamp::read_check(dirs, entries_of, |dir| {
                    crate::core::fs_list::read_dir_entries(dir)
                })))
                .is_ok(),
            Self::Git(path, sender) => sender.send(Ok(git_branch(&path))).is_ok(),
            Self::Preview(path, fit_width, sender) => sender
                .send(file_info::read_preview(&path, fit_width))
                .is_ok(),
            Self::Thumbnail(path, sender) => sender.send(file_info::read_thumbnail(&path)).is_ok(),
            Self::Properties(read, sender) => {
                file_info::read_properties(&read.paths, &read.count, &read.cancel, |facts| {
                    sender.send(facts).is_ok()
                })
            }
            Self::Writable(path, sender) => sender.send(Ok(can_write(&path))).is_ok(),
            Self::Script(path, sender) => {
                use std::io::Read;
                let result = std::fs::File::open(&path).and_then(|file| {
                    let mut text = String::new();
                    file.take(MAX_SCRIPT_BYTES + 1).read_to_string(&mut text)?;
                    if text.len() as u64 > MAX_SCRIPT_BYTES {
                        return Err(io::Error::other("script exceeds 8 MiB"));
                    }
                    Ok(ScriptSource {
                        hash: tasty_settings::hash_bytes(text.as_bytes()),
                        source: text,
                    })
                });
                sender.send(result).is_ok()
            }
        };
        if !delivered {
            tracing::debug!("discarded local read result for a closed request");
        }
    }
}
/// 폴더가 없어 읽지 못했을 때 남아 있는 가장 가까운 상위 폴더. 오류 문구는 OS 문구 그대로다.
#[derive(Debug)]
pub(crate) struct MissingFolder {
    message: String,
    pub(crate) existing_ancestor: Option<PathBuf>,
}
impl std::fmt::Display for MissingFolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for MissingFolder {}

/// 경로가 없거나 폴더가 아니면 worker 에서 상위 폴더를 확인해 오류에 붙인다. UI 스레드는 파일시스템을 읽지 않는다.
fn missing(path: &std::path::Path, error: io::Error) -> io::Error {
    let kind = error.kind();
    if !matches!(kind, io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) {
        return error;
    }
    let existing_ancestor = path
        .ancestors()
        .skip(1)
        .find(|a| a.is_dir())
        .map(PathBuf::from);
    io::Error::new(
        kind,
        MissingFolder {
            message: error.to_string(),
            existing_ancestor,
        },
    )
}
pub(crate) fn directory(path: PathBuf) -> Query<Vec<DirEntryInfo>> {
    Query::new(|sender| Request::Directory(path, sender))
}
/// 탐색기 목록. 읽기 직전의 폴더 표지를 함께 돌려준다.
pub(crate) fn stamped_directory(path: PathBuf) -> Query<StampedListing> {
    Query::new(|sender| Request::StampedDirectory(path, sender))
}
/// 폴더 표지를 읽는다. `entries_of` 가 있으면 그 폴더의 항목 서명도 읽는다(파일 정보 확인).
pub(crate) fn dir_stamps(dirs: Vec<PathBuf>, entries_of: Option<PathBuf>) -> Query<StampCheck> {
    Query::new(|sender| Request::DirStamps(dirs, entries_of, sender))
}
pub(crate) type DirStamps = Vec<(PathBuf, Option<DirStamp>)>;
#[cfg(test)]
impl Query<StampCheck> {
    /// worker 에 넘기기 전의 확인 대상 폴더.
    pub(crate) fn dirs_for_test(&self) -> Vec<PathBuf> {
        match &self.request {
            Some(Request::DirStamps(dirs, _, _)) => dirs.clone(),
            _ => panic!("not a pending folder check"),
        }
    }
}
pub(crate) fn writable(path: PathBuf) -> Query<bool> {
    Query::new(|sender| Request::Writable(path, sender))
}

/// 현재 사용자가 폴더 안에 항목을 만들 수 있는가. Unix 는 실제 권한을 OS 에 묻는다.
#[cfg(unix)]
fn can_write(path: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c is a valid nul-terminated path for the duration of the call.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}
/// Windows 폴더의 읽기 전용 속성은 쓰기 권한을 뜻하지 않고 ACL 판정은 비싸다.
/// 쓸 수 있다고 보고 거부는 만들 때의 오류로 알린다.
#[cfg(not(unix))]
fn can_write(_path: &std::path::Path) -> bool {
    true
}
pub(crate) fn git(path: PathBuf) -> Query<Option<HeadState>> {
    Query::new(|sender| Request::Git(path, sender))
}
pub(crate) fn script(path: PathBuf) -> Query<ScriptSource> {
    Query::new(|sender| Request::Script(path, sender))
}
/// `fit_width` 는 그림을 줄여 맞출 최대 표시 폭(물리 px)이다.
pub(crate) fn preview(path: PathBuf, fit_width: u32) -> Query<PreviewData> {
    Query::new(|sender| Request::Preview(path, fit_width, sender))
}
pub(crate) fn thumbnail(path: PathBuf) -> Query<egui::ColorImage> {
    Query::new(|sender| Request::Thumbnail(path, sender))
}
/// 정보를 먼저 돌려준 뒤 같은 worker 에서 폴더 크기를 `count` 에 센다. `cancel` 을 켜면 멈춘다.
pub(crate) fn properties(
    paths: Vec<PathBuf>,
    count: Arc<FolderCount>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Query<PropertiesFacts> {
    Query::new(|sender| {
        Request::Properties(
            PropertiesRead {
                paths,
                count,
                cancel,
            },
            sender,
        )
    })
}

struct PropertiesRead {
    paths: Vec<PathBuf>,
    count: Arc<FolderCount>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) struct ScriptSource {
    pub(crate) source: String,
    pub(crate) hash: String,
}
pub(crate) struct PendingScript {
    pub(crate) script_id: String,
    pub(crate) name: String,
    pub(crate) stored_hash: String,
    pub(crate) query: Query<ScriptSource>,
}

pub(crate) struct LocalReads {
    jobs: Vec<JoinHandle<()>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stopping: bool,
}
impl Default for LocalReads {
    fn default() -> Self {
        Self::new(Arc::new(|| {}))
    }
}
impl LocalReads {
    pub(crate) fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            jobs: Vec::new(),
            wake,
            stopping: false,
        }
    }
    fn requests(&self) -> ReadRequests {
        ReadRequests {
            pending: Vec::new(),
            available: if self.stopping {
                0
            } else {
                MAX_RUNNING.saturating_sub(self.jobs.len())
            },
        }
    }
    fn run_requests(&mut self, requests: ReadRequests) {
        tracing::trace!(target: "tasty::explorer::perf", running = self.jobs.len(),
            admitted = requests.pending.len(), capacity = MAX_RUNNING, "read_queue");
        for request in requests.pending {
            if self.submit(request).is_err() {
                tracing::warn!("local read admission closed before execution");
            }
        }
    }
    fn submit(&mut self, request: Request) -> Result<(), Request> {
        if self.stopping || self.jobs.len() >= MAX_RUNNING {
            return Err(request);
        }
        let wake = self.wake.clone();
        match std::thread::Builder::new()
            .name("local-read".into())
            .spawn(move || {
                request.run(&*wake);
                wake();
            }) {
            Ok(job) => self.jobs.push(job),
            Err(error) => tracing::warn!(%error, "local read worker could not start"),
        }
        Ok(())
    }
    /// 시험에서 View 의 읽기 요청을 실제 worker 로 돌린다. `poll` 이 true 를 돌려주면 멈춘다.
    #[cfg(test)]
    pub(crate) fn drive(&mut self, mut poll: impl FnMut(&mut ReadRequests) -> bool) {
        loop {
            let mut requests = self.requests();
            let done = poll(&mut requests);
            self.run_requests(requests);
            if done {
                return;
            }
            std::thread::yield_now();
        }
    }
    pub(crate) fn reap(&mut self) {
        let mut i = 0;
        while i < self.jobs.len() {
            if self.jobs[i].is_finished() {
                if self.jobs.swap_remove(i).join().is_err() {
                    tracing::warn!("local read worker panicked");
                }
            } else {
                i += 1;
            }
        }
    }
    pub(crate) fn poll_shutdown(&mut self) -> usize {
        self.stopping = true;
        self.reap();
        self.jobs.len()
    }
    /// 시험에서 넘긴 읽기가 모두 끝날 때까지 기다린다. 결과는 영수증 채널에 남아 아직 받지 않은 상태다.
    #[cfg(test)]
    pub(crate) fn wait_idle(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            self.reap();
            if self.jobs.is_empty() {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "local reads did not finish"
            );
            std::thread::yield_now();
        }
    }
    #[cfg(test)]
    pub(crate) fn finish<T>(&mut self, query: &mut Query<T>) -> io::Result<T> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            self.reap();
            let mut requests = self.requests();
            let result = query.poll(&mut requests);
            self.run_requests(requests);
            if let Some(result) = result {
                return result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "local query did not complete"
            );
            std::thread::yield_now();
        }
    }
}
impl Drop for LocalReads {
    fn drop(&mut self) {
        let remaining = self.poll_shutdown();
        if remaining != 0 {
            tracing::warn!(remaining, "local read workers remain unjoined at App drop");
        }
    }
}

pub(crate) fn git_branch(cwd: &std::path::Path) -> Option<HeadState> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let dot_git = d.join(".git");
        if let Ok(content) = std::fs::read_to_string(dot_git.join("HEAD")) {
            return crate::state::branch::parse_head(&content);
        }
        if let Ok(content) = std::fs::read_to_string(&dot_git)
            && let Some(gitdir) = crate::state::branch::resolve_gitdir_file(d, &content)
            && let Ok(head) = std::fs::read_to_string(gitdir.join("HEAD"))
        {
            return crate::state::branch::parse_head(&head);
        }
        dir = d.parent();
    }
    None
}

impl super::App {
    pub(crate) fn poll_local_reads(&mut self) {
        self.local_reads.reap();
        let mut requests = self.local_reads.requests();
        for view in self.view.views.values_mut() {
            view.poll_local_reads(&mut requests);
        }
        self.local_reads.run_requests(requests);
    }
}

impl crate::view::MainView {
    pub(crate) fn poll_local_read_results(
        &mut self,
        owner: &mut crate::app::local_reads::ReadRequests,
    ) {
        let mut changed = self.state.tutorial.poll_progress();
        changed |= self.state.explorer_views.poll_local_reads(owner);
        changed |= self.state.report_explorer_create_refusals();
        changed |= self.state.poll_branch_read(owner);
        changed |= poll_file_picker_reads(&mut self.state, owner);
        changed |= crate::adapters::ui::popup::explorer_properties::poll(&mut self.state, owner);
        if self.state.dialogs.pending_script_confirm.is_none()
            && let Some(result) = self
                .state
                .script_reads
                .front_mut()
                .and_then(|request| request.query.poll(owner))
        {
            let request = self.state.script_reads.pop_front().expect("script receipt");
            match result {
                Ok(read) if read.hash == request.stored_hash => {
                    if self
                        .proxy
                        .send_event(crate::AppEvent::RunLuaScript {
                            source: read.source,
                            name: request.name,
                            window: self.base.winit.id(),
                        })
                        .is_err()
                    {
                        tracing::warn!("script execution request after shutdown");
                    }
                }
                Ok(read) => {
                    self.state.dialogs.pending_script_confirm =
                        Some(crate::state::PendingScriptConfirm {
                            script_id: request.script_id,
                            name: request.name,
                            source: read.source,
                            new_hash: read.hash,
                            result: None,
                            content_height: None,
                        });
                    self.state.dispatch_intent(
                        crate::intent::UiIntent::OpenPopup(
                            crate::intent::OpenRequest::new(
                                "script_changed_confirm",
                                crate::model::popup_kind::PopupScope::Window,
                            )
                            .centered()
                            .focused(),
                        )
                        .from_user_menu("script_tofu_gate"),
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "script read failed");
                    let text = crate::hooks::lua_runs::shortcut_failure_text(
                        &request.name,
                        &crate::hooks::lua_runs::ShortcutFailure::ReadFailed(&error.to_string()),
                    );
                    self.state.toasts.push(
                        text,
                        crate::adapters::ui::ToastKind::Error,
                        crate::adapters::ui::ToastScope::Window,
                    );
                }
            }
            changed = true;
        }
        if changed {
            self.mark_dirty();
        }
    }
}

fn poll_file_picker_reads(
    state: &mut MainViewState,
    owner: &mut crate::app::local_reads::ReadRequests,
) -> bool {
    let Some(d) = state.dialogs.file_picker.as_mut() else {
        return false;
    };
    let Some(result) = d.local_query.as_mut().and_then(|query| query.poll(owner)) else {
        return false;
    };
    d.local_query = None;
    match result {
        Ok(mut entries) => {
            crate::core::fs_list::sort_entries(
                &mut entries,
                tasty_model::SortColumn::Name,
                tasty_model::SortDir::Asc,
            );
            d.load = if entries.is_empty() {
                FpLoadState::Empty
            } else {
                FpLoadState::Loaded
            };
            d.entries = entries;
        }
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::PermissionDenied {
                t("filepicker.error_perm.reason_permission").to_string()
            } else {
                e.to_string()
            };
            d.entries.clear();
            d.load = FpLoadState::ErrorPerm(msg);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaced_directory_receipt_cannot_publish_its_old_result() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(a.join("old"), b"old").unwrap();
        std::fs::write(b.join("new"), b"new").unwrap();
        let mut owner = LocalReads::default();
        let mut query = directory(a);
        let mut requests = owner.requests();
        let _result = query.poll(&mut requests);
        owner.run_requests(requests);
        query = directory(b);
        let result = owner.finish(&mut query).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "new");
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
    }
    #[test]
    fn script_reads_accept_the_budget_and_reject_one_extra_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.sh");
        let mut owner = LocalReads::default();
        for size in [MAX_SCRIPT_BYTES - 1, MAX_SCRIPT_BYTES, MAX_SCRIPT_BYTES + 1] {
            std::fs::write(&path, vec![b'x'; size as usize]).unwrap();
            let result = owner.finish(&mut script(path.clone()));
            if size <= MAX_SCRIPT_BYTES {
                assert_eq!(result.unwrap().source.len() as u64, size);
            } else {
                assert_eq!(result.err().unwrap().to_string(), "script exceeds 8 MiB");
            }
        }
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
    }
    #[test]
    fn slow_reads_do_not_exceed_the_worker_limit() {
        let mut owner = LocalReads::default();
        let mut releases = Vec::new();
        let mut queries = Vec::new();
        for _ in 0..MAX_RUNNING + 1 {
            let (release, wait) = mpsc::channel();
            let mut query = Query::new(|reply| Request::Blocked(wait, reply));
            let mut requests = owner.requests();
            assert!(query.poll(&mut requests).is_none());
            owner.run_requests(requests);
            releases.push(release);
            queries.push(query);
        }
        assert_eq!(owner.jobs.len(), MAX_RUNNING);
        assert!(queries.last().unwrap().request.is_some());
        for release in releases {
            release.send(()).unwrap();
        }
        for query in &mut queries {
            owner.finish(query).unwrap();
        }
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_missing_folder_names_its_nearest_existing_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone").join("inner");
        let text = std::fs::read_dir(&gone).unwrap_err().to_string();
        let mut owner = LocalReads::default();
        let error = owner.finish(&mut directory(gone.clone())).err().unwrap();
        while owner.poll_shutdown() != 0 {
            std::thread::yield_now();
        }
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(error.to_string(), text, "the OS message is kept");
        let found = error
            .get_ref()
            .and_then(|e| e.downcast_ref::<MissingFolder>())
            .and_then(|m| m.existing_ancestor.clone());
        assert_eq!(found.as_deref(), Some(dir.path()));

        let other = missing(&gone, io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(other.get_ref().is_none(), "other errors pass through");
    }
}
