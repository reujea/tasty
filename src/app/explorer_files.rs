//! Bounded local Explorer actions. Views queue fixed inputs; App owns execution and joins.
pub(crate) mod history;
pub(crate) mod remote;
mod ui_sync;
pub(crate) use super::file_jobs::job;
use super::file_jobs::named;
#[cfg(test)]
pub(crate) use ui_sync::mark_rename_done;
pub(crate) use ui_sync::{forget_press_request, open_conflict};
use ui_sync::{push_result, show_running};

use history::{Recorded, UndoOf};
use job::leftover::Leftover;
use job::{OpKind, Report, Shared, UndoStep};

use crate::explorer_ui::view::ops::Refused;
use crate::runtime::surface_binding::SurfaceBinding;
use crate::view::ui::View;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;

const MAX_PENDING_PER_VIEW: usize = 8;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) enum Operation {
    Paste {
        paths: Vec<PathBuf>,
        destination: PathBuf,
        cut: bool,
    },
    Trash(Vec<PathBuf>),
    Rename {
        path: PathBuf,
        name: String,
    },
    /// OS 기본 앱(폴더는 파일 관리자)으로 연다. 여러 항목이면 하나씩 연다.
    Open(Vec<PathBuf>),
    /// 담은 폴더를 OS 파일 관리자로 열고 그 항목을 고른다.
    Reveal(PathBuf),
    /// 새 폴더나 빈 파일. 같은 이름이 있으면 덮어쓰지 않고 실패한다.
    Create {
        dir: PathBuf,
        name: String,
        folder: bool,
    },
    /// Properties 권한 편집. 한 항목에만 적용하고 폴더 안으로 내려가지 않는다. 로컬 칸에서만 쓴다.
    SetPermissions {
        path: PathBuf,
        change: super::file_jobs::permissions::PermChange,
    },
    /// 끝난 복사·이동을 되돌린다.
    Undo(Vec<UndoStep>),
    /// 원본이 남은 이동 항목의 원본만 다시 지운다. `dest` 는 처음 이동의 목적지 폴더다.
    RemoveLeftovers {
        dest: Option<PathBuf>,
        leftovers: Vec<Leftover>,
    },
}

/// worker 가 돌려주는 결과. 복사·이동·휴지통·되돌리기는 항목별 결과를 남긴다.
#[derive(Debug)]
pub(crate) enum Done {
    Report(Report),
    Simple(Result<(), String>),
    /// 이름 변경·새 항목. 성공해 무엇이 바뀌었으면 이력에 쌓을 되돌리기 단계를 담는다.
    Step(Result<Option<UndoStep>, String>),
    /// 원격 작업의 결과를 받기 전에 연결이 끊겼다. 어디까지 했는지 모른다.
    Lost(remote::Lost),
}
impl Done {
    fn success(&self) -> bool {
        match self {
            Self::Report(r) => r.failed.is_empty() && r.skipped.is_empty() && !r.cancelled,
            Self::Simple(r) => r.is_ok(),
            Self::Step(r) => r.is_ok(),
            Self::Lost(_) => false,
        }
    }
    /// 실패를 한 줄로 요약한다. 실패가 없으면 None.
    fn error_summary(&self) -> Option<String> {
        match self {
            Self::Report(r) => r.failed.first().map(|f| {
                format!(
                    "{} succeeded, {} failed: {}: {:?}",
                    r.done,
                    r.failed.len(),
                    f.path.display(),
                    f.reason
                )
            }),
            Self::Simple(r) => r.as_ref().err().cloned(),
            Self::Step(r) => r.as_ref().err().cloned(),
            Self::Lost(_) => {
                Some(crate::i18n::t("explorer.state.remote_result_unknown").to_string())
            }
        }
    }
}
impl Operation {
    /// 진행·결과를 항목 단위로 보이는 작업의 종류. 이름 변경·열기·새 항목은 None.
    pub(crate) fn kind(&self) -> Option<OpKind> {
        match self {
            Self::Paste { cut: false, .. } => Some(OpKind::Copy),
            Self::Paste { cut: true, .. } => Some(OpKind::Move),
            Self::RemoveLeftovers { .. } => Some(OpKind::RemoveOriginals),
            Self::Trash(_) => Some(OpKind::Trash),
            Self::Undo(_) => Some(OpKind::Undo),
            Self::Rename { .. }
            | Self::Open(_)
            | Self::Reveal(_)
            | Self::Create { .. }
            | Self::SetPermissions { .. } => None,
        }
    }
    /// 진행 표시에 쓰는 목적지 폴더와 항목 수.
    fn summary(&self) -> (Option<PathBuf>, usize) {
        match self {
            Self::Paste {
                paths, destination, ..
            } => (Some(destination.clone()), paths.len()),
            Self::Trash(paths) => (None, paths.len()),
            Self::Undo(steps) => (None, steps.len()),
            Self::RemoveLeftovers { dest, leftovers } => (dest.clone(), leftovers.len()),
            Self::Rename { .. }
            | Self::Open(_)
            | Self::Reveal(_)
            | Self::Create { .. }
            | Self::SetPermissions { .. } => (None, 1),
        }
    }
    /// 되돌리기라면 되돌리는 작업의 종류. 옮긴 항목이 하나라도 있으면 이동이다. 이력의 단계를
    /// 되돌릴 때는 그 단계의 원래 작업([`history::Entry::undo_of`])이 이것보다 앞선다.
    fn undo_of(&self) -> Option<UndoOf> {
        let Self::Undo(steps) = self else {
            return None;
        };
        Some(
            if steps.iter().any(|s| matches!(s, UndoStep::Moved { .. })) {
                UndoOf::Move
            } else {
                UndoOf::Copy
            },
        )
    }
    fn bytes(&self) -> usize {
        let paths = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|p| p.as_os_str().len() + std::mem::size_of::<PathBuf>())
                .sum::<usize>()
        };
        match self {
            Self::Paste {
                paths: items,
                destination,
                ..
            } => paths(items) + destination.as_os_str().len(),
            Self::Trash(items) => paths(items),
            Self::RemoveLeftovers { dest, leftovers } => {
                dest.as_ref().map_or(0, |d| d.as_os_str().len())
                    + leftovers
                        .iter()
                        .map(|l| {
                            l.source.as_os_str().len()
                                + l.copy.as_os_str().len()
                                + std::mem::size_of::<Leftover>()
                        })
                        .sum::<usize>()
            }
            Self::Rename { path, name } => path.as_os_str().len() + name.len(),
            Self::Open(paths) => paths.iter().map(|p| p.as_os_str().len()).sum(),
            Self::Reveal(path) | Self::SetPermissions { path, .. } => path.as_os_str().len(),
            Self::Create { dir, name, .. } => dir.as_os_str().len() + name.len(),
            Self::Undo(steps) => steps
                .iter()
                .map(|s| match s {
                    UndoStep::Created(p, _) | UndoStep::Made(p, _) | UndoStep::Replaced(p) => {
                        p.as_os_str().len()
                    }
                    UndoStep::Moved { from, to, .. } => from.as_os_str().len() + to.as_os_str().len(),
                } + std::mem::size_of::<UndoStep>())
                .sum(),
        }
    }
    /// 작업이 바꿀 수 있는 경로. 결과와 관계없이 이 경로를 보는 목록을 다시 읽는다.
    /// 앞은 항목이 생기거나 사라질 수 있는 폴더, 뒤는 원래 자리에서 사라질 수 있는 경로다.
    fn affected(&self) -> Affected {
        let parents = |paths: &[PathBuf]| -> Vec<PathBuf> {
            paths
                .iter()
                .filter_map(|p| p.parent().map(PathBuf::from))
                .collect()
        };
        match self {
            Self::Paste {
                paths,
                destination,
                cut,
            } => {
                let mut changed = vec![destination.clone()];
                if *cut {
                    changed.extend(parents(paths));
                }
                let removed = if *cut { paths.clone() } else { Vec::new() };
                Affected { changed, removed }
            }
            Self::Trash(paths) => Affected {
                changed: parents(paths),
                removed: paths.clone(),
            },
            Self::RemoveLeftovers { leftovers, .. } => {
                let sources: Vec<PathBuf> = leftovers.iter().map(|l| l.source.clone()).collect();
                Affected {
                    changed: parents(&sources),
                    removed: sources,
                }
            }
            Self::Rename { path, .. } => Affected {
                changed: parents(std::slice::from_ref(path)),
                removed: vec![path.clone()],
            },
            Self::Open(_) | Self::Reveal(_) => Affected::default(),
            // 목록의 항목 집합은 그대로지만 같은 폴더를 보는 칸이 바뀐 metadata 를 다시 읽는다.
            Self::SetPermissions { path, .. } => Affected {
                changed: parents(std::slice::from_ref(path)),
                removed: Vec::new(),
            },
            Self::Create { dir, .. } => Affected {
                changed: vec![dir.clone()],
                removed: Vec::new(),
            },
            Self::Undo(steps) => {
                let mut changed = Vec::new();
                let mut removed = Vec::new();
                for step in steps {
                    match step {
                        UndoStep::Created(p, _) | UndoStep::Made(p, _) | UndoStep::Replaced(p) => {
                            changed.extend(parents(std::slice::from_ref(p)));
                            removed.push(p.clone());
                        }
                        UndoStep::Moved { from, to, .. } => {
                            changed.extend(parents(&[from.clone(), to.clone()]));
                            removed.push(to.clone());
                        }
                    }
                }
                Affected { changed, removed }
            }
        }
    }
    fn run(self, shared: &Shared) -> Done {
        match self {
            Self::Paste {
                paths,
                destination,
                cut,
            } => Done::Report(job::run_transfer(shared, &paths, &destination, cut)),
            Self::Trash(paths) => Done::Report(job::run_trash(shared, &paths)),
            Self::Undo(steps) => Done::Report(job::run_undo(shared, &steps)),
            Self::RemoveLeftovers { dest, leftovers } => Done::Report(
                job::leftover::run_remove_leftovers(shared, dest, &leftovers),
            ),
            Self::Rename { path, name } => Done::Step(named::rename(&path, &name)),
            Self::Open(paths) => Done::Simple(open_each(&paths)),
            Self::Reveal(path) => {
                Done::Simple(crate::platform::reveal::reveal_path(&path).map_err(|e| e.to_string()))
            }
            Self::Create { dir, name, folder } => Done::Step(named::create(&dir, &name, folder)),
            Self::SetPermissions { path, change } => {
                Done::Simple(super::file_jobs::permissions::apply(&path, change))
            }
        }
    }
}

/// 하나를 못 열어도 나머지는 연다. 실패한 항목은 경로와 이유를 줄마다 모아 돌려준다.
fn open_each(paths: &[PathBuf]) -> Result<(), String> {
    let failed: Vec<String> = paths
        .iter()
        .filter_map(|path| {
            crate::platform::reveal::open_path(path)
                .err()
                .map(|e| format!("{}: {e}", path.display()))
        })
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

#[derive(Debug, Default, PartialEq)]
struct Affected {
    changed: Vec<PathBuf>,
    removed: Vec<PathBuf>,
}

struct Target {
    origin: crate::intent::IntentOrigin,
    surface: u32,
    binding: SurfaceBinding,
    view: Weak<()>,
    selection: Option<Weak<()>>,
    tab: Option<tasty_model::ExplorerTabId>,
    clipboard: Option<Arc<AtomicBool>>,
    reload: bool,
    clear_selection: bool,
    /// 만든 항목. 성공했고 요청한 explorer 가 아직 그 폴더를 보고 있으면 이 항목을 고른다.
    created: Option<PathBuf>,
    /// 결과를 카드 하나로 모을 누름 번호와 이 요청의 번호.
    press: Option<(u64, u64)>,
    /// 대기열에서 받은 요청 번호. 시작한 뒤에도 남아 결과를 기다리는 화면이 이 요청을 찾는다.
    request: u64,
}
struct Request {
    /// 대기열에서 이 요청을 가리키는 번호.
    id: u64,
    target: Target,
    operation: Operation,
    recorded: Option<Recorded>,
}
#[derive(Default)]
pub(crate) struct Requests(VecDeque<Request>, u64);
impl Requests {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
    /// 아직 시작하지 않은 요청을 뺀다.
    pub(crate) fn remove(&mut self, id: u64) {
        self.0.retain(|r| r.id != id);
    }
    /// 기다리는 되돌리기 작업들의 단계.
    #[cfg(test)]
    pub(crate) fn queued_steps(&self) -> Vec<Vec<UndoStep>> {
        self.0
            .iter()
            .filter_map(|r| match &r.operation {
                Operation::Undo(steps) => Some(steps.clone()),
                _ => None,
            })
            .collect()
    }
    /// `surface` 가 요청해 기다리는 작업 중 끝나면 이력에 반영할 것이 있는가.
    pub(crate) fn has_history_for(&self, surface: u32) -> bool {
        self.0
            .iter()
            .any(|r| r.target.surface == surface && r.recorded.is_some())
    }
    /// `surface` 가 요청해 기다리는 작업들. 진행 표시가 없는 이름 변경·열기는 뺀다.
    fn queued_for(&self, surface: u32) -> Vec<crate::explorer_ui::view::ops::Queued> {
        self.0
            .iter()
            .filter(|r| r.target.surface == surface)
            .filter_map(|r| {
                let kind = r.operation.kind()?;
                let (dest, count) = r.operation.summary();
                Some(crate::explorer_ui::view::ops::Queued {
                    id: r.id,
                    kind,
                    count,
                    dest,
                })
            })
            .collect()
    }
}

impl crate::state::MainViewState {
    /// This entry is only used by native user menus/shortcuts and the user rename popup.
    /// 잘라내기 붙여넣기는 explorer 클립보드에서 온 것으로 보고 성공하면 클립보드를 비운다.
    pub(crate) fn request_explorer_file(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        operation: Operation,
        origin: crate::intent::IntentOrigin,
    ) {
        self.enqueue_explorer_file(engine, surface, operation, origin, true, &mut None);
    }
    /// 클립보드와 무관한 사용자 작업(드래그·다시 시도·되돌리기)을 요청한다.
    pub(crate) fn request_explorer_file_direct(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        operation: Operation,
        origin: crate::intent::IntentOrigin,
    ) {
        self.enqueue_explorer_file(engine, surface, operation, origin, false, &mut None);
    }
    /// [`Self::request_explorer_file_direct`] 와 같고, 받은 요청의 번호를 돌려준다.
    pub(crate) fn request_explorer_file_numbered(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        operation: Operation,
        origin: crate::intent::IntentOrigin,
    ) -> Option<u64> {
        self.enqueue_explorer_file(engine, surface, operation, origin, false, &mut None)
    }
    /// 한 번의 누름으로 넣은 요청들의 결과를 그 칸에서 카드 하나로 모은다.
    pub(crate) fn group_explorer_requests(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        requests: Vec<u64>,
    ) {
        let Some(view) = self.explorer_views.get_mut(surface) else {
            return;
        };
        let lifetime = std::time::Duration::from_millis(engine.settings.overlay.toast_duration_ms);
        let press = view.ops.expect_press(requests.clone(), lifetime);
        for request in self.explorer_file_requests.0.iter_mut() {
            if requests.contains(&request.id) {
                request.target.press = Some((press, request.id));
            }
        }
    }
    /// 이력의 단계를 되돌리거나 다시 실행하는 요청. 요청하지 못했으면 단계를 돌려준다.
    pub(crate) fn request_explorer_history(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        operation: Operation,
        origin: crate::intent::IntentOrigin,
        recorded: Recorded,
    ) -> Option<Recorded> {
        let mut recorded = Some(recorded);
        self.enqueue_explorer_file(engine, surface, operation, origin, false, &mut recorded);
        recorded
    }
    /// 요청을 대기열에 넣고 번호를 돌려준다. 받으면 `recorded` 의 이력 단계를 요청으로 가져가고,
    /// 받지 않으면 그대로 둔다.
    fn enqueue_explorer_file(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        operation: Operation,
        origin: crate::intent::IntentOrigin,
        from_clipboard: bool,
        recorded: &mut Option<Recorded>,
    ) -> Option<u64> {
        let remote = engine.mirror_workspace_of(surface).is_some();
        if !matches!(origin, crate::intent::IntentOrigin::User { .. })
            || engine.blocks_explorer_write(surface)
            || (remote
                && matches!(
                    operation,
                    Operation::Open(_) | Operation::Reveal(_) | Operation::SetPermissions { .. }
                ))
        {
            return None;
        }
        let binding = SurfaceBinding::capture(engine, surface)?;
        // 사용자 조작으로 들어온 요청이므로 받지 않을 때도 그 칸에 이유를 보인다.
        let refused = if self.explorer_file_requests.0.len() >= MAX_PENDING_PER_VIEW {
            Some(Refused::QueueFull)
        } else if operation.bytes() > MAX_REQUEST_BYTES {
            Some(Refused::TooLarge)
        } else {
            None
        };
        if let Some(refused) = refused {
            tracing::warn!(
                "Explorer file request capacity exhausted; no filesystem operation started"
            );
            let lifetime =
                std::time::Duration::from_millis(engine.settings.overlay.toast_duration_ms);
            match self.explorer_views.get_mut(surface) {
                Some(view) => view
                    .ops
                    .push_refused(refused, std::time::Instant::now() + lifetime),
                None => self.toasts.push(
                    refused.text(),
                    crate::adapters::ui::ToastKind::Warning,
                    crate::adapters::ui::ToastScope::Surface(surface),
                ),
            }
            return None;
        }
        let clipboard = match &operation {
            Operation::Paste { cut: true, .. } if from_clipboard => self
                .explorer_clipboard
                .get()
                .map(|c| Arc::clone(&c.identity)),
            _ => None,
        };
        let requests = &mut self.explorer_file_requests;
        requests.1 += 1;
        let id = requests.1;
        let target = Target {
            request: id,
            surface,
            binding,
            origin,
            view: self.webview_identity.clone(),
            clipboard,
            selection: self
                .explorer_views
                .get(surface)
                .map(|v| v.selection_identity()),
            tab: self
                .explorer_views
                .get(surface)
                .and_then(|view| view.tab_id()),
            clear_selection: matches!(operation, Operation::Trash(_) | Operation::Rename { .. }),
            reload: !matches!(operation, Operation::Open(_) | Operation::Reveal(_)),
            created: match &operation {
                Operation::Create { dir, name, .. } => Some(dir.join(name)),
                _ => None,
            },
            press: None,
        };
        // 새 복사·이동·이름 변경·새 항목은 된 항목이 있으면 이력에 쌓는다. 원격 작업은 이력에 넣지 않는다.
        let recorded = recorded
            .take()
            .or_else(|| history::Source::of(&operation).map(Recorded::New))
            .filter(|_| !remote);
        self.explorer_file_requests.0.push_back(Request {
            id,
            target,
            operation,
            recorded,
        });
        Some(id)
    }
}

struct Job {
    window: winit::window::WindowId,
    engine: crate::runtime::engine_session::EngineId,
    target: Target,
    affected: Affected,
    shared: Arc<Shared>,
    worker: JoinHandle<Done>,
    undo_of: Option<UndoOf>,
    recorded: Option<Recorded>,
    /// 마지막으로 다시 그린 진행 상태(집계 단계, 파일·항목 수, 바이트, 파일명, 질문 여부). 바뀔 때만 다시 그린다.
    seen: ui_sync::ProgressSeen,
}
/// One running file operation serializes collision checks/moves. Pending requests remain bounded
/// in their originating Views and are discarded if that View closes before admission.
#[derive(Default)]
pub(crate) struct ExplorerFiles {
    job: Option<Job>,
    /// 결과를 기다리는 원격 작업. mirror workspace 마다 하나다.
    remote: Vec<remote::RemoteJob>,
    /// 원격 작업 번호. 연결과 함께 쓰므로 프로세스 안에서 구별되면 된다.
    next_op: u64,
    stopping: bool,
}
impl ExplorerFiles {
    pub(crate) fn has_pending(&self) -> bool {
        self.job.is_some()
    }
    pub(crate) fn begin_shutdown(&mut self) {
        self.stopping = true;
    }
    /// 끝난 작업을 거둔다. 아직 실행 중이거나 작업이 없으면 None 이다.
    fn take_finished(&mut self) -> Option<Finished> {
        if !self.job.as_ref().is_some_and(|j| j.worker.is_finished()) {
            return None;
        }
        let Job {
            window,
            engine,
            target,
            affected,
            worker,
            shared,
            undo_of,
            recorded,
            ..
        } = self.job.take().expect("finished job exists");
        Some(Finished {
            window,
            engine,
            remote: None,
            target,
            affected,
            shared,
            undo_of,
            recorded,
            result: join(worker),
        })
    }
    pub(crate) fn poll_shutdown(&mut self) -> usize {
        self.begin_shutdown();
        // 종료 중에는 남은 항목을 멈춘다. 답을 기다리던 작업도 풀리고, 만들던 사본은 임시 폴더째 지워진다.
        if let Some(job) = &self.job {
            job.shared.cancel();
        }
        if self.job.as_ref().is_some_and(|j| j.worker.is_finished()) {
            let job = self.job.take().expect("finished job exists");
            report_result(&join(job.worker));
        }
        usize::from(self.job.is_some())
    }
}
impl Drop for ExplorerFiles {
    fn drop(&mut self) {
        if self.poll_shutdown() != 0 {
            tracing::warn!(
                "Explorer file worker remains unjoined at App drop; OS file work was not cancelled"
            );
        }
    }
}
struct Finished {
    window: winit::window::WindowId,
    engine: crate::runtime::engine_session::EngineId,
    /// 원격 작업이면 그 mirror workspace.
    remote: Option<u32>,
    target: Target,
    affected: Affected,
    shared: Arc<Shared>,
    undo_of: Option<UndoOf>,
    recorded: Option<Recorded>,
    result: Done,
}
impl Finished {
    /// 이 작업이 바꿀 수 있는 경로를 보는 explorer 를 다시 읽게 한다. 다시 읽은 View 가 있으면 true.
    /// 원격 작업은 같은 mirror workspace 의 explorer 만 본다.
    fn reload_views(
        &self,
        store: &mut crate::adapters::ui::surface::explorer::view::ExplorerViewStore,
    ) -> bool {
        let Affected { changed, removed } = &self.affected;
        if changed.is_empty() && removed.is_empty() {
            return false;
        }
        match self.remote {
            Some(ws) => store.invalidate_remote(ws, changed, removed),
            None => store.invalidate_local(changed, removed),
        }
    }
}
fn join(worker: JoinHandle<Done>) -> Done {
    worker
        .join()
        .unwrap_or_else(|_| Done::Simple(Err("Explorer file worker panicked".into())))
}
fn report_result(result: &Done) {
    if let Some(error) = result.error_summary() {
        tracing::warn!(%error, "Explorer file operation failed");
    }
}
impl Target {
    fn matches_view(&self, view: &crate::view::state::ViewState) -> bool {
        view.matches_identity(&self.view)
    }
    fn view_is_current(&self, view: &crate::view::main::MainView) -> bool {
        self.matches_view(&view.base.state)
    }
    fn clipboard_is_current(
        &self,
        slot: &crate::state::explorer_clipboard_slot::ExplorerClipboardSlot,
    ) -> bool {
        self.clipboard.as_ref().is_none_or(|old| {
            slot.get()
                .is_some_and(|current| Arc::ptr_eq(old, &current.identity))
        })
    }
    /// 잘라낸 목록을 모두 옮겼으면 앱 공용 목록과 OS 클립보드에서 지운다. 창이나 대상 surface 가
    /// 그 사이 닫혀도 해야 하므로 View 와 무관하게 앱 상태로 처리한다.
    fn settle_cut(
        &self,
        slot: &crate::state::explorer_clipboard_slot::ExplorerClipboardSlot,
        os: &crate::clipboard::files::Handle,
        success: bool,
    ) {
        if success
            && self.clipboard.is_some()
            && self.clipboard_is_current(slot)
            && let Some(clip) = slot.take()
        {
            crate::state::explorer_os_clipboard::cut_consumed(os, &clip);
        }
    }
    /// 시험용: 앱이 하는 순서대로 잘라내기를 정리한 뒤 View 에 반영한다.
    #[cfg(test)]
    fn finish(self, state: &mut crate::state::MainViewState, success: bool) {
        self.settle_cut(&state.explorer_clipboard, &state.file_clipboard, success);
        self.apply(state, success);
    }
    fn apply(self, state: &mut crate::state::MainViewState, success: bool) {
        if self.reload
            && let Some(view) = state.explorer_views.get_mut(self.surface)
        {
            let active = view.tab_id();
            view.for_each_tab(|view| {
                if view.tab_id() != self.tab {
                    return;
                }
                if success
                    && self.clear_selection
                    && self
                        .selection
                        .as_ref()
                        .is_some_and(|old| view.matches_selection(old))
                {
                    view.clear_selection();
                }
                if success
                    && active == self.tab
                    && let Some(path) = &self.created
                {
                    view.reveal_created_if_unchanged(path, self.selection.as_ref());
                }
                view.request_reload();
            });
        }
    }
}
impl super::App {
    pub(crate) fn poll_explorer_files(&mut self) {
        if tracing::enabled!(target: "tasty::explorer::perf", tracing::Level::TRACE) {
            let queued: usize = self
                .view
                .views
                .values()
                .filter_map(|v| v.as_main())
                .map(|v| v.state.explorer_file_requests.0.len())
                .sum();
            tracing::trace!(target: "tasty::explorer::perf", queued,
                running = self.explorer_files.job.is_some(), "file_queue");
        }
        self.sync_remote_write_support();
        if let Some(finished) = self.explorer_files.take_finished() {
            self.finish_explorer_file(finished);
        }
        for finished in self.explorer_files.poll_remote(&mut self.remote.sessions) {
            self.finish_explorer_file(finished);
        }
        if !self.explorer_files.stopping {
            self.start_explorer_file();
        }
        self.sync_explorer_ops();
    }
    fn finish_explorer_file(&mut self, finished: Finished) {
        // 같은 폴더를 보는 다른 explorer 도 다시 읽는다. 실패·부분 성공도 실제 상태를 다시 읽어야 하고,
        // 요청한 surface 가 그 사이 닫혔어도 다른 윈도우의 목록은 낡은 채로 남으면 안 된다.
        // 원격 작업은 같은 mirror workspace 의 explorer 만, 로컬 작업은 로컬 explorer 만 다시 읽는다.
        for view in self.view.views.values_mut() {
            if let Some(view) = view.as_main_mut()
                && finished.reload_views(&mut view.state.explorer_views)
            {
                view.mark_dirty();
            }
        }
        let Finished {
            window,
            engine,
            target,
            result,
            shared,
            undo_of,
            recorded,
            ..
        } = finished;
        self.clear_explorer_running(&shared);
        let success = result.success();
        let failure = result.error_summary();
        if success && let Some(clipboard) = &target.clipboard {
            clipboard.store(true, Ordering::Release);
        }
        target.settle_cut(
            &self.services.explorer_clipboard,
            &self.services.file_clipboard,
            success,
        );
        report_result(&result);
        if success {
            if let Some(path) = &target.created {
                ui_sync::close_created(&mut self.view, target.surface, path);
            }
            ui_sync::close_renamed(&mut self.view, target.request);
        }
        // 권한 편집 상자는 칸·바인딩이 바뀌었어도 결과를 알아야 다시 편집할 수 있다. 상자가 실패를
        // 직접 보이면 같은 실패를 토스트로 또 알리지 않는다.
        let shown_in_popup = ui_sync::settle_permissions(
            &mut self.view,
            window,
            target.request,
            if success {
                Ok(())
            } else {
                Err(failure.clone().unwrap_or_default())
            },
        );
        if self.engines.of_window(window) != Some(engine) {
            return;
        }
        let Some(session) = self.engines.get(engine) else {
            return;
        };
        if !target.binding.current(&session) {
            return;
        }
        let Some(view) = self
            .view
            .views
            .get_mut(&window)
            .and_then(|v| v.as_main_mut())
        else {
            return;
        };
        if !target.view_is_current(view) {
            return;
        }
        match result {
            Done::Report(report) => {
                if let Some(recorded) = recorded
                    && let Some(v) = view.state.explorer_views.get_mut(target.surface)
                {
                    v.ops.history.record(recorded, &report);
                }
                let lifetime = std::time::Duration::from_millis(
                    session.read().settings.overlay.toast_duration_ms,
                );
                push_result(
                    &mut view.state,
                    target.surface,
                    (report, target.press),
                    undo_of,
                    lifetime,
                );
            }
            Done::Step(step) => {
                if let Some(recorded) = recorded
                    && let Some(v) = view.state.explorer_views.get_mut(target.surface)
                {
                    v.ops.history.record_step(recorded, step.ok().flatten());
                }
                ui_sync::push_failure(&mut view.state, target.surface, target.press, failure);
            }
            Done::Simple(_) => {
                let failure = failure.filter(|_| !shown_in_popup);
                ui_sync::push_failure(&mut view.state, target.surface, target.press, failure);
            }
            Done::Lost(lost) => {
                ui_sync::push_failure(&mut view.state, target.surface, target.press, None);
                if let Some(v) = view.state.explorer_views.get_mut(target.surface) {
                    v.ops.push_lost(lost.title(), lost.body());
                }
            }
        }
        target.apply(&mut view.state, success);
        view.mark_dirty();
    }
    /// 기다리는 요청을 시작한다. 로컬 작업은 앱 전체에서 하나, 원격 작업은 mirror workspace 마다 하나씩
    /// 실행하고 서로 기다리지 않는다. 시작할 수 없게 된 요청(칸·클립보드가 바뀜, 원격 쓰기를 잃음)은 버린다.
    fn start_explorer_file(&mut self) {
        let mut local_free = self.explorer_files.job.is_none();
        for (&window, view) in &mut self.view.views {
            let Some(view) = view.as_main_mut() else {
                continue;
            };
            let Some(engine) = self.engines.of_window(window) else {
                continue;
            };
            let Some(session) = self.engines.get(engine) else {
                continue;
            };
            let mut i = 0;
            let mut lost_requests = ui_sync::LostRequests::default();
            while let Some(request) = view.state.explorer_file_requests.0.get(i) {
                let target = &request.target;
                let read = session.read();
                let remote = read.mirror_workspace_of(target.surface);
                // mirror 칸이 원격 쓰기를 잃었다면 연결이 끊긴 것이다.
                let lost = read.blocks_explorer_write(target.surface);
                let stale = !matches!(target.origin, crate::intent::IntentOrigin::User { .. })
                    || !target.view_is_current(view)
                    || !target.binding.current(&session)
                    || lost
                    || target
                        .clipboard
                        .as_ref()
                        .is_some_and(|c| c.load(Ordering::Acquire));
                let startable = match remote {
                    None => local_free,
                    Some(ws) => !self.explorer_files.remote_busy(ws),
                };
                drop(read);
                if !stale && !startable {
                    i += 1;
                    continue;
                }
                let Request {
                    target,
                    operation,
                    recorded,
                    ..
                } = view
                    .state
                    .explorer_file_requests
                    .0
                    .remove(i)
                    .expect("request at index exists");
                if stale {
                    if let Some((_, request)) = target.press {
                        ui_sync::forget_press_request(&mut view.state, target.surface, request);
                    }
                    if lost {
                        lost_requests.add(target.surface);
                    }
                    continue;
                }
                let wake = self.view.proxy.clone();
                let shared = Arc::new(match remote {
                    Some(_) => Shared::interactive(|| {}),
                    None => ui_sync::interactive_shared(wake.clone()),
                });
                show_running(
                    &mut view.state,
                    target.surface,
                    &operation,
                    &shared,
                    recorded.is_some(),
                );
                let started = match remote {
                    Some(workspace) => self.explorer_files.start_remote(
                        &self.remote.sessions,
                        remote::Start {
                            window,
                            engine,
                            workspace,
                            target,
                            operation,
                        },
                        shared,
                    ),
                    None => {
                        local_free = false;
                        spawn_local(
                            &mut self.explorer_files,
                            wake,
                            (window, engine),
                            Request {
                                id: 0,
                                target,
                                operation,
                                recorded,
                            },
                            shared,
                        )
                    }
                };
                if let Err(target) = started {
                    if let Some(v) = view.state.explorer_views.get_mut(target.surface) {
                        v.ops.running = None;
                        v.ops.quiet_step = None;
                    }
                    if let Some((_, request)) = target.press {
                        ui_sync::forget_press_request(&mut view.state, target.surface, request);
                    }
                    // 원격 연결로 보내지 못했으면 서버는 아무것도 하지 않았다.
                    if remote.is_some() {
                        ui_sync::push_failure(
                            &mut view.state,
                            target.surface,
                            None,
                            Some(crate::i18n::t("explorer.state.remote_send_failed").to_string()),
                        );
                    }
                }
            }
            lost_requests.notify(&mut view.state);
        }
    }
}

/// 로컬 작업 스레드를 띄운다. 띄우지 못하면 요청의 대상을 돌려준다.
fn spawn_local(
    files: &mut ExplorerFiles,
    wake: winit::event_loop::EventLoopProxy<crate::AppEvent>,
    (window, engine): (
        winit::window::WindowId,
        crate::runtime::engine_session::EngineId,
    ),
    request: Request,
    shared: Arc<Shared>,
) -> Result<(), Target> {
    let Request {
        target,
        operation,
        recorded,
        ..
    } = request;
    let affected = operation.affected();
    let undo_of = match &recorded {
        Some(Recorded::Undo(entry)) => Some(entry.undo_of()),
        _ => operation.undo_of(),
    };
    let worker_shared = Arc::clone(&shared);
    match std::thread::Builder::new()
        .name("explorer-files".into())
        .spawn(move || {
            let result = operation.run(&worker_shared);
            if wake.send_event(crate::AppEvent::TimerTick).is_err() {
                tracing::debug!("Explorer file completion after event loop closed");
            }
            result
        }) {
        Ok(worker) => {
            files.job = Some(Job {
                window,
                engine,
                target,
                affected,
                shared,
                worker,
                undo_of,
                recorded,
                seen: Default::default(),
            });
            Ok(())
        }
        Err(error) => {
            tracing::warn!(%error, "Explorer file worker spawn failed");
            Err(target)
        }
    }
}

#[cfg(test)]
mod search_result_tests;
#[cfg(test)]
mod tests;
