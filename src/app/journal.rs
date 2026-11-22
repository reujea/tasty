//! Application side of the data-home worker. Engine bindings live on their EngineSession.
#[cfg(feature = "gui")]
mod boot_recovery;
mod capture;
pub(crate) mod commands;
mod creation;
#[cfg(feature = "gui")]
pub(crate) mod forward;
mod id_reservations;
pub(crate) mod notices;
mod process_release;
mod publication;
mod resource_cleanup;
mod restorations;
#[cfg(feature = "gui")]
mod retirement;
mod save_failures;
mod unowned;
#[cfg(feature = "gui")]
mod view_writes;

use crate::runtime::engine_session::{EngineId, EngineSession};
use crate::runtime::journal_product::{
    Completion, EngineSelection, JournalWorker, Request, ResultValue, Work,
};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use capture::PresetCaptureCompletion;
#[cfg(feature = "gui")]
pub(crate) use capture::PresetCaptureNotice;
pub(crate) use capture::{PendingPresetSave, PresetCaptureOutput, PresetCaptureReply};
pub(crate) use creation::{ActivationOutcome, ActivationReceipt};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tasty_core::projection;

/// halt 거절 응답에 붙이는 사유의 최대 문자 수.
const HALT_CAUSE_EXCERPT: usize = 512;

struct Opening {
    selection: EngineSelection,
    ticket: Option<u64>,
    projected: bool,
    surface_floor: crate::runtime::journal_product::SurfaceFloor,
    select_available_slot: bool,
}

pub(crate) struct JournalApplication {
    worker: JournalWorker,
    /// Starts claimed terminal candidates so a slow child start does not block the App.
    spawner: crate::runtime::effect_runner::TerminalSpawner,
    commands: commands::Commands,
    captures: std::collections::BTreeMap<u64, capture::PendingCapture>,
    preset_captures: capture::PresetCaptures,
    capture_requests: HashMap<EngineId, bool>,
    capture_rejections: HashMap<EngineId, capture::CaptureRejection>,
    /// 주기 capture의 연속 실패와 상한 거절 알림 여부.
    save_failures: save_failures::FailureStreaks<EngineId>,
    execution_id_requests: HashMap<u64, EngineId>,
    replacements: Vec<(EngineId, tasty_core::Replacement)>,
    changed_engines: std::collections::HashSet<EngineId>,
    completion_views: HashMap<EngineId, crate::runtime::journal_product::CompletionView>,
    /// 엔진의 workspace 를 보여 주는 창마다의 터미널 영역. split 의 탐색기 하한 판정에만 쓰며 journal 에
    /// 저장하지 않는다.
    split_geometries: HashMap<EngineId, Vec<commands::split_floor::ShownWorkspace>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    opening: HashMap<EngineId, Opening>,
    creations: HashMap<(EngineId, u64), creation::Creation>,
    #[cfg(feature = "gui")]
    forwards: std::collections::BTreeMap<u64, forward::Forward>,
    resource_cleanups: std::collections::BTreeMap<u64, resource_cleanup::Cleanup>,
    restorations: restorations::Restorations,
    restored_views: HashMap<EngineId, crate::model::RestoredPresentation>,
    /// Why an engine's first workspace ended unpublished, for the View that cannot open it.
    #[cfg(feature = "gui")]
    first_workspace_failures: HashMap<EngineId, String>,
    /// 결속 전에 거절된 opening의 사유. 대기하던 호출자가 [`Self::poll_restore_bootstrap`]이나
    /// [`Self::take_refused_opening`]으로 받는다.
    refused_openings: HashMap<EngineId, String>,
    #[cfg(feature = "gui")]
    view_writes: view_writes::ViewWrites,
    #[cfg(feature = "gui")]
    retirements: retirement::Retirements,
    known_slots: std::collections::BTreeMap<u32, bool>,
    /// 기록을 읽지 못한 슬롯. 원본 위에 이어 쓰지 않도록 이번 실행에서는 고르지 않는다.
    damaged_slots: std::collections::BTreeSet<u32>,
    started: bool,
    /// 저널을 열지 못한 이유. 부팅 오류 화면이 문구를 고를 때 읽는다.
    #[cfg(feature = "gui")]
    startup_failure: Option<crate::runtime::journal_product::StartupFailure>,
    runtime_epoch: Option<u64>,
    next_ticket: u64,
    command_submissions: unowned::CommandSubmissions,
    halted: Option<String>,
    /// 실행 중 halt를 알림으로 넘겼는지. 같은 halt를 다시 알리지 않는다.
    halt_reported: bool,
    /// 이 저널의 저장소 파일. halt 안내가 원본 위치로 보인다.
    database: std::path::PathBuf,
    /// 사용자에게 알릴 복원·저장 사실. App이 꺼내 알림과 안내로 옮긴다.
    notices: Vec<notices::RecoveryNotice>,
    /// 원본 저널을 열지 못해 이번 실행에만 쓰는 임시 저장소. worker가 먼저 멈추도록 마지막 필드다.
    #[cfg(feature = "gui")]
    ephemeral_home: Option<tempfile::TempDir>,
    #[cfg(feature = "gui")]
    boot_release: Option<crate::runtime::resource_retirement::EngineRelease>,
}

/// 원본 저널을 연 실패의 원인을 파일에서 확인할 수 있는 만큼만 가린다. 읽을 권한이 없으면
/// 읽기 거절, 읽은 머리가 SQLite 저장소가 아니면 손상이다. 그 밖(머리는 맞지만 열리지 않음,
/// 쓰기 거절, 파일 없음 등)은 손상으로 단정하지 않고 열지 못함으로 알린다.
#[cfg(feature = "gui")]
fn journal_read_cause(database: &std::path::Path) -> notices::NoticeCause {
    const HEADER: &[u8] = b"SQLite format 3\0";
    let mut file = match std::fs::File::open(database) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return notices::NoticeCause::ReadDenied;
        }
        Err(_) => return notices::NoticeCause::OpenFailed,
    };
    let mut head = Vec::with_capacity(HEADER.len());
    match std::io::Read::read_to_end(
        &mut std::io::Read::take(&mut file, HEADER.len() as u64),
        &mut head,
    ) {
        Ok(_) if !head.is_empty() && head != HEADER => notices::NoticeCause::Corrupt,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            notices::NoticeCause::ReadDenied
        }
        _ => notices::NoticeCause::OpenFailed,
    }
}

/// Longer than each stage shutdown waits for: a settling creation's child input and replaced
/// owner receipts and a closing retirement's receipts each have a 5 second deadline. So it ends
/// only a wait that no longer progresses. It also bounds the GUI shutdown's final View write and
/// slot retirements, which have no stage deadline of their own.
const SHUTDOWN_JOURNAL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);

impl JournalApplication {
    pub(crate) fn metrics_handle(&self) -> Arc<crate::runtime::journal_product::JournalMetrics> {
        self.worker.metrics_handle()
    }

    /// 엔진의 workspace 를 보여 주는 창들의 터미널 영역을 기록한다. 기록이 없는 workspace 의 split 에는
    /// 하한이 없다.
    #[cfg(feature = "gui")]
    pub(crate) fn update_split_geometry(
        &mut self,
        id: EngineId,
        views: Vec<commands::split_floor::ShownWorkspace>,
    ) {
        self.split_geometries.insert(id, views);
    }

    /// 창이 닫힌 엔진의 기록을 지운다.
    #[cfg(feature = "gui")]
    pub(crate) fn retain_split_geometries(&mut self, shown: impl Fn(EngineId) -> bool) {
        self.split_geometries.retain(|id, _| shown(*id));
    }

    pub(crate) fn update_completion_view(
        &mut self,
        id: EngineId,
        core: &crate::core::CoreState,
        presentation: &dyn crate::model::StructurePresentation,
    ) {
        let mut focused_panes = std::collections::BTreeMap::new();
        let mut selected_tabs = std::collections::BTreeMap::new();
        let mut selected_surfaces = std::collections::BTreeMap::new();
        for workspace in &core.workspaces() {
            if let Some(pane) = presentation.pane_id(workspace) {
                focused_panes.insert(workspace.id, pane);
            }
            for pane in workspace.pane_layout().all_pane_ids() {
                if let Some(pane) = workspace.pane_layout().find_pane(pane) {
                    if let Some(tab) = pane.tabs.get(presentation.tab_index(pane)) {
                        selected_tabs.insert(pane.id, tab.id);
                    }
                    for tab in &pane.tabs {
                        if let Some(surface) = presentation.surface_id(tab) {
                            selected_surfaces.insert(tab.id, surface);
                        }
                    }
                }
            }
        }
        self.completion_views.insert(
            id,
            crate::runtime::journal_product::CompletionView {
                focused_panes,
                mirror_count: core.mirror_workspaces().len(),
                selected_tabs,
                selected_surfaces,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn new(wake: Arc<dyn Fn() + Send + Sync>) -> anyhow::Result<Self> {
        Self::new_with_writer_lock(wake, None)
    }

    /// `writer_lock`은 부팅 첫머리에서 이 홈의 저널에 대해 선점한 잠금이다(GUI·헤드리스 부팅). 없으면
    /// worker가 저장소를 연 뒤 잠근다(선점을 판정하지 못한 부팅·테스트).
    pub(crate) fn new_with_writer_lock(
        wake: Arc<dyn Fn() + Send + Sync>,
        writer_lock: Option<tasty_event_store::WriterLock>,
    ) -> anyhow::Result<Self> {
        let home = tasty_utils::path::tasty_home()
            .ok_or_else(|| anyhow::anyhow!("data home unavailable for structure journal"))?;
        Self::open_at(home, wake, writer_lock)
    }

    fn open_at(
        home: std::path::PathBuf,
        wake: Arc<dyn Fn() + Send + Sync>,
        writer_lock: Option<tasty_event_store::WriterLock>,
    ) -> anyhow::Result<Self> {
        let database = crate::runtime::journal_product::journal_database_path(&home);
        let worker = JournalWorker::spawn_with(home, wake.clone(), writer_lock)
            .map_err(anyhow::Error::msg)?;
        let spawner = crate::runtime::effect_runner::TerminalSpawner::spawn(wake.clone())?;
        Ok(Self {
            worker,
            spawner,
            commands: Default::default(),
            captures: Default::default(),
            preset_captures: Default::default(),
            capture_requests: Default::default(),
            capture_rejections: Default::default(),
            save_failures: Default::default(),
            execution_id_requests: Default::default(),
            changed_engines: Default::default(),
            replacements: Vec::new(),
            completion_views: Default::default(),
            split_geometries: Default::default(),
            wake,
            opening: HashMap::new(),
            creations: HashMap::new(),
            resource_cleanups: Default::default(),
            #[cfg(feature = "gui")]
            forwards: Default::default(),
            restorations: Default::default(),
            restored_views: Default::default(),
            #[cfg(feature = "gui")]
            first_workspace_failures: Default::default(),
            refused_openings: Default::default(),
            #[cfg(feature = "gui")]
            view_writes: Default::default(),
            #[cfg(feature = "gui")]
            retirements: Default::default(),
            known_slots: Default::default(),
            damaged_slots: Default::default(),
            started: false,
            #[cfg(feature = "gui")]
            startup_failure: None,
            runtime_epoch: None,
            next_ticket: 1,
            command_submissions: Default::default(),
            halted: None,
            halt_reported: false,
            database,
            notices: Vec::new(),
            #[cfg(feature = "gui")]
            ephemeral_home: None,
            #[cfg(feature = "gui")]
            boot_release: None,
        })
    }

    /// 데이터 홈의 저널을 열지 못한 첫 엔진을 이번 실행에만 쓰는 빈 임시 저장소로 다시 연다.
    /// 원본(`structure/journal.db`와 부속 파일)에는 쓰지 않는다. 원인을 가리려고 머리만 읽는다. 임시 저장소는
    /// 종료할 때 지우므로 다음 실행은 원본을 다시 읽는다. 원인과 원본 위치는 오류 알림과
    /// 비포커스 안내로 `engine`의 창에 알린다.
    #[cfg(all(test, unix, feature = "gui"))]
    pub(crate) fn fall_back_to_ephemeral(
        &mut self,
        engine: EngineId,
        failure: &str,
    ) -> anyhow::Result<()> {
        self.replace_with_ephemeral(engine, failure, false)
    }

    #[cfg(feature = "gui")]
    fn replace_with_ephemeral(
        &mut self,
        engine: EngineId,
        failure: &str,
        after_binding: bool,
    ) -> anyhow::Result<()> {
        let original = tasty_utils::path::tasty_home()
            .ok_or_else(|| anyhow::anyhow!("data home unavailable for structure journal"))?;
        let database = crate::runtime::journal_product::journal_database_path(&original);
        let temporary = tempfile::Builder::new()
            .prefix("tasty-ephemeral-journal-")
            .tempdir()?;
        let mut fresh = Self::open_at(temporary.path().to_path_buf(), self.wake.clone(), None)?;
        fresh.ephemeral_home = Some(temporary);
        tracing::error!(
            path = %database.display(),
            "structure journal could not be opened; this run uses a temporary journal: {failure}"
        );
        fresh.notices.push(notices::RecoveryNotice::critical(
            Some(engine),
            notices::NoticePopup {
                variant: notices::NoticeVariant::Restore,
                cause: if after_binding {
                    notices::NoticeCause::SaveStopped
                } else {
                    journal_read_cause(&database)
                },
                path: database.display().to_string(),
            },
            &format!(
                "{}\n{failure}",
                crate::i18n::t("recovery.ephemeral_session")
            ),
        ));
        *self = fresh;
        Ok(())
    }

    /// 이번 실행이 임시 저장소를 쓰는지. 임시 저장소도 열지 못하면 더 물러날 곳이 없다.
    #[cfg(feature = "gui")]
    pub(crate) fn is_ephemeral(&self) -> bool {
        self.ephemeral_home.is_some()
    }

    pub(crate) fn begin_engine(
        &mut self,
        session: &EngineSession,
        selection: EngineSelection,
    ) -> Result<(), String> {
        if self.opening.contains_key(&session.id) {
            return Ok(());
        }
        if let EngineSelection::ImportedSlot { source } = &selection
            && session.persistence.slot != Some(source.destination_slot)
        {
            return Err("import destination must be claimed by the opening engine".into());
        }
        let surface_floor = crate::runtime::journal_product::SurfaceFloor::of(session);
        self.restorations.register(session.id);
        self.opening.insert(
            session.id,
            Opening {
                selection,
                ticket: None,
                projected: false,
                surface_floor,
                select_available_slot: false,
            },
        );
        Ok(())
    }

    /// The caller holds newly opening engines behind its startup read/render barrier.
    pub(crate) fn poll_bootstrap(
        &mut self,
        sessions: &mut [&mut EngineSession],
        plugins: Option<&mut crate::plugin::PluginManager>,
    ) -> Result<(), String> {
        if let Some(reason) = &self.halted {
            return Err(reason.clone());
        }
        let result = self.poll_initial(sessions, plugins);
        #[cfg(feature = "gui")]
        for (stream, failures, cause) in self.view_writes.take_alerts() {
            if let Some(session) = sessions.iter().find(|session| {
                session
                    .journal_binding
                    .as_ref()
                    .is_some_and(|binding| binding.stream == stream)
            }) {
                self.notices.push(save_failures::view_failing_notice(
                    session, failures, &cause,
                ));
            }
        }
        #[cfg(feature = "gui")]
        for session in sessions {
            if session
                .journal_binding
                .as_ref()
                .is_some_and(|binding| self.view_writes.failed_for(&binding.stream))
            {
                session.persistence.dirty.mark_dirty();
            }
        }
        if let Err(reason) = &result {
            self.halted = Some(reason.clone());
            self.fail_pending_commands(reason);
        }
        result
    }

    /// 쌓인 복원·저장 알림을 꺼낸다.
    pub(crate) fn take_notices(&mut self) -> Vec<notices::RecoveryNotice> {
        if let Some(reason) = self.halted.as_deref()
            && !std::mem::replace(&mut self.halt_reported, true)
        {
            self.notices.push(notices::RecoveryNotice::critical(
                None,
                notices::NoticePopup {
                    variant: notices::NoticeVariant::Halt,
                    cause: notices::NoticeCause::SaveStopped,
                    path: self.database.display().to_string(),
                },
                reason,
            ));
        }
        std::mem::take(&mut self.notices)
    }

    /// 아직 받을 곳이 없어 전달하지 못한 알림을 다시 넣는다.
    #[cfg(not(feature = "gui"))]
    pub(crate) fn defer_notices(&mut self, notices: Vec<notices::RecoveryNotice>) {
        let mut later = notices;
        later.append(&mut self.notices);
        self.notices = later;
    }

    /// 복원하지 못한 surface를 경고 알림으로 옮긴다. 그 surface는 빈 상태로 다시 시작한다.
    fn report_payload_failures(&mut self) {
        for failure in self.restorations.take_failures() {
            let surface = failure.surface.to_string();
            self.notices.push(notices::RecoveryNotice::warning(
                Some(failure.engine),
                crate::i18n::t("recovery.surface_payload_title").to_owned(),
                crate::i18n::t_args(
                    "recovery.surface_payload_body",
                    &[&failure.workspace, &surface, &failure.cause],
                ),
            ));
        }
    }

    /// 적재하지 못한 범위를 경고로 남긴다. 손상된 stream마다, 건너뛴 snapshot은 한 번.
    fn report_load_damage(&mut self, damage: &crate::runtime::journal::LoadReport) {
        for (stream, cause) in &damage.damaged {
            let name = stream
                .strip_prefix("structure:slot-")
                .unwrap_or(stream.as_str());
            self.notices.push(notices::RecoveryNotice::warning(
                None,
                crate::i18n::t("recovery.damaged_stream_title").to_owned(),
                crate::i18n::t_args("recovery.damaged_stream_body", &[name, cause]),
            ));
        }
        if let Some((_, cause)) = damage.skipped_snapshots.first() {
            self.notices.push(notices::RecoveryNotice::warning(
                None,
                crate::i18n::t("recovery.snapshot_fallback_title").to_owned(),
                crate::i18n::t_args("recovery.snapshot_fallback_body", &[cause]),
            ));
        }
    }

    fn report_binding_issue(
        &mut self,
        engine: EngineId,
        issue: &crate::runtime::journal_product::BindingIssue,
    ) {
        use crate::runtime::journal_product::BindingIssue;
        let notice = match issue {
            BindingIssue::ViewUnreadable {
                slot,
                preserved,
                cause,
            } => notices::RecoveryNotice::warning(
                Some(engine),
                crate::i18n::t("recovery.view_unreadable_title").to_owned(),
                crate::i18n::t_args(
                    "recovery.view_unreadable_body",
                    &[&slot.to_string(), preserved, cause],
                ),
            ),
            BindingIssue::LegacyUnparsable { path, cause } => notices::RecoveryNotice::warning(
                Some(engine),
                crate::i18n::t("recovery.legacy_unparsable_title").to_owned(),
                crate::i18n::t_args("recovery.legacy_unparsable_body", &[path, cause]),
            ),
            BindingIssue::LegacyUnreadable {
                path,
                permission_denied,
                cause,
            } => notices::RecoveryNotice::critical(
                Some(engine),
                notices::NoticePopup {
                    variant: notices::NoticeVariant::Restore,
                    cause: if *permission_denied {
                        notices::NoticeCause::ReadDenied
                    } else {
                        notices::NoticeCause::Corrupt
                    },
                    path: path.clone(),
                },
                &format!(
                    "{}\n{cause}",
                    crate::i18n::t("recovery.legacy_not_reimported")
                ),
            ),
            BindingIssue::MissingScrollback { count, path } => notices::RecoveryNotice::warning(
                Some(engine),
                crate::i18n::t("recovery.missing_scrollback_title").to_owned(),
                crate::i18n::t_args(
                    "recovery.missing_scrollback_body",
                    &[&count.to_string(), path],
                ),
            ),
        };
        self.notices.push(notice);
    }

    #[cfg(any(feature = "gui", all(test, unix)))]
    pub(crate) fn damaged_layout_slots(&self) -> impl Iterator<Item = u32> + '_ {
        self.damaged_slots.iter().copied()
    }

    #[cfg(feature = "gui")]
    pub(crate) fn halt_reason(&self) -> Option<&str> {
        self.halted.as_deref()
    }

    #[cfg(feature = "gui")]
    pub(crate) fn input_generation(
        &self,
        engine: EngineId,
        surface: u32,
    ) -> Option<Option<tasty_terminal::ResourceGeneration>> {
        self.creations
            .iter()
            .filter(|((id, _), _)| *id == engine)
            .find_map(|(_, creation)| creation.input_generation(surface))
    }

    pub(crate) fn pauses_observation(&self) -> bool {
        self.commands
            .has_closing(|operation| self.cleanup_awaits_receipts(operation))
            || self.cleanup_pauses_observation()
            || self
                .creations
                .values()
                .any(creation::Creation::pauses_observation)
    }

    /// Shutdown waits for publication, for the settling of published creations (their child CR
    /// and replaced owner receipts) and for retirements that only await their receipts, so a
    /// command replies once and records its outcome. Each stage has its own deadline, and the
    /// shutdown deadline bounds the whole wait.
    pub(crate) fn shutdown_waits_for_publication(&self) -> bool {
        self.pauses_observation()
            || self
                .creations
                .values()
                .any(creation::Creation::awaits_settlement)
            || self.cleanup_awaits_any_receipts()
    }

    /// Shutdown waits for publication within one deadline fixed at its first wait; a later poll
    /// never extends it. Past it, execution halts: unfinished operations keep their records for
    /// restart recovery. Headless shutdown then moves their owners to the halted release; GUI
    /// shutdown drops them with the App. Nothing is reported as done.
    pub(crate) fn shutdown_publication_pending(
        &mut self,
        deadline: &mut Option<std::time::Instant>,
        now: std::time::Instant,
    ) -> bool {
        let waiting = self.shutdown_waits_for_publication();
        self.shutdown_waits_within(waiting, deadline, now)
    }

    /// The final View write and slot retirements of a GUI shutdown share the publication
    /// deadline. Past it, the halt leaves the last stored View and keeps the unretired slots.
    #[cfg(feature = "gui")]
    pub(crate) fn shutdown_persistence_pending(
        &mut self,
        deadline: &mut Option<std::time::Instant>,
        now: std::time::Instant,
    ) -> bool {
        let waiting = self.has_pending_view_writes() || self.has_pending_retirements();
        self.shutdown_waits_within(waiting, deadline, now)
    }

    fn shutdown_waits_within(
        &mut self,
        waiting: bool,
        deadline: &mut Option<std::time::Instant>,
        now: std::time::Instant,
    ) -> bool {
        if !waiting || self.is_halted() {
            return false;
        }
        if now < *deadline.get_or_insert(now + SHUTDOWN_JOURNAL_DEADLINE) {
            return true;
        }
        let reason = "shutdown stopped waiting for unfinished journal work; its outcome is left to restart recovery";
        tracing::warn!("{reason}");
        self.halted = Some(reason.into());
        // 종료가 정한 halt다. 사용자가 이미 종료를 골랐으므로 알리지 않는다.
        self.halt_reported = true;
        self.fail_pending_commands(reason);
        false
    }

    pub(crate) fn cleanup_poll_deadline(&self) -> Option<std::time::Instant> {
        let deadline = self
            .resource_cleanup_deadline()
            .into_iter()
            .chain(
                self.creations
                    .values()
                    .filter_map(creation::Creation::cleanup_deadline),
            )
            .min();
        #[cfg(feature = "gui")]
        if self
            .boot_release
            .as_ref()
            .is_some_and(|release| release.needs_detached_poll())
        {
            let next = std::time::Instant::now() + std::time::Duration::from_millis(100);
            return Some(deadline.map_or(next, |existing| existing.min(next)));
        }
        deadline
    }

    #[cfg(not(feature = "gui"))]
    pub(crate) fn runtime_epoch(&self) -> Option<u64> {
        self.runtime_epoch
    }
    pub(crate) fn is_halted(&self) -> bool {
        self.halted.is_some()
    }

    pub(crate) fn reject_halted_request(
        &self,
        request: &crate::ipc::protocol::JsonRpcRequest,
    ) -> Option<crate::ipc::protocol::JsonRpcResponse> {
        if !self.is_halted() || request.method == "system.shutdown" {
            return None;
        }
        // 재시작 말고는 회복할 수 없으므로 호출자가 원인과 함께 알 수 있게 사유를 붙인다.
        let reason = self.halted.as_deref().unwrap_or_default();
        Some(crate::ipc::protocol::JsonRpcResponse::error(
            request.id.clone().unwrap_or(serde_json::Value::Null),
            -32000,
            format!(
                "structure publication is halted; no live projection is available; restart Tasty to recover (cause: {})",
                unowned::excerpt(reason, HALT_CAUSE_EXCERPT)
            ),
        ))
    }

    /// Stage selected startup resources while leaving inactive terminals and missing kinds pending.
    pub(crate) fn poll_restore_bootstrap(&mut self, session: &EngineSession) -> Result<(), String> {
        let id = session.id;
        if let Some(reason) = self.refused_openings.remove(&id) {
            return Err(reason);
        }
        #[cfg(feature = "gui")]
        if self.retirements.pending.contains_key(&id) {
            return Ok(());
        }
        if self.opening.contains_key(&id)
            || session.journal_binding.is_none()
            || self.has_creation(id)
            || self.restorations.has_pending(id)
        {
            return Ok(());
        }
        let selected = crate::runtime::surface_restorer::initial_terminal_selection(
            &session.core_state,
            self.restored_views.get(&id),
        );
        let next = self
            .restorations
            .ready(id)
            .iter()
            .find(|item| {
                if item.input.kind == "terminal" {
                    selected.contains(&item.surface_id)
                } else {
                    session
                        .runtime
                        .surface_registry
                        .get_live(&item.input.kind)
                        .is_some()
                }
            })
            .map(|item| item.surface_id);
        if let Some(surface_id) = next {
            self.activate_restored_surface(session, surface_id)?;
        } else {
            self.restorations.finish_bootstrap(id);
        }
        Ok(())
    }

    #[cfg(feature = "gui")]
    pub(crate) fn take_restored_presentation(
        &mut self,
        id: EngineId,
    ) -> Option<crate::model::RestoredPresentation> {
        self.restored_views.remove(&id)
    }

    /// 결속 전에 거절된 opening의 사유를 꺼낸다.
    #[cfg(feature = "gui")]
    pub(crate) fn take_refused_opening(&mut self, id: EngineId) -> Option<String> {
        self.refused_openings.remove(&id)
    }

    /// The reason an engine became ready without a workspace, when its first creation failed.
    #[cfg(feature = "gui")]
    pub(crate) fn take_first_workspace_failure(&mut self, id: EngineId) -> Option<String> {
        self.first_workspace_failures.remove(&id)
    }

    /// Selection is supplied by the application/View. Loading a payload alone grants no activation.
    pub(crate) fn wake_application(&self) {
        (self.wake)();
    }
    pub(crate) fn request_restore_receipt(
        &mut self,
        session: &EngineSession,
        surface: u32,
        activation: Option<u64>,
    ) -> Result<Option<ActivationReceipt>, String> {
        if self.is_halted() {
            return Err("resource publication is halted".into());
        }
        if let Some(reason) =
            crate::runtime::surface_restorer::recovery_blocked_reason(session, surface)
        {
            return Err(reason.to_owned());
        }
        if let Some(receipt) = self
            .creations
            .iter()
            .filter(|((engine, _), _)| *engine == session.id)
            .find_map(|(_, creation)| creation.join_restore(surface, activation))
        {
            return Ok(Some(receipt));
        }
        if self.has_creation(session.id) {
            return Ok(None);
        }
        if session
            .core_state
            .find_surface_by_id(surface)
            .and_then(|value| value.activation_generation)
            != activation
        {
            return Err("activation target changed".into());
        }
        if self.activate_restored_surface(session, surface)? {
            return Ok(self
                .creations
                .iter()
                .filter(|((engine, _), _)| *engine == session.id)
                .find_map(|(_, creation)| creation.join_restore(surface, activation)));
        }
        let reading = self.restorations.is_reading(session.id, surface);
        if reading {
            Ok(None)
        } else {
            Err("selected resource has no pending restore input".into())
        }
    }

    pub(crate) fn activate_restored_surface(
        &mut self,
        session: &EngineSession,
        surface_id: u32,
    ) -> Result<bool, String> {
        #[cfg(feature = "gui")]
        if self.retirements.pending.contains_key(&session.id) {
            return Ok(false);
        }
        if self.is_halted() || self.has_creation(session.id) {
            return Ok(false);
        }
        let items = self.restorations.ready(session.id);
        let Some(index) = items.iter().position(|item| item.surface_id == surface_id) else {
            return Ok(false);
        };
        if items[index].input.kind != "terminal"
            && session
                .runtime
                .surface_registry
                .get_live(&items[index].input.kind)
                .is_none()
        {
            return Ok(false);
        }
        let request = self.restorations.take_ready(session.id, index);
        let ticket = self.next_ticket;
        self.next_ticket = ticket
            .checked_add(1)
            .ok_or("journal ticket range exhausted")?;
        let creation = creation::Creation::restore(ticket, session, &self.worker, request)?;
        self.creations
            .insert((session.id, creation.ticket), creation);
        (self.wake)();
        Ok(true)
    }

    pub(super) fn has_creation(&self, id: EngineId) -> bool {
        self.creations.keys().any(|(engine, _)| *engine == id)
    }
    #[cfg(feature = "gui")]
    pub(super) fn has_forward(&self, id: EngineId) -> bool {
        self.forwards.values().any(|forward| forward.engine == id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn has_pending_engine_effects(&self, id: EngineId) -> bool {
        self.commands.has_remote_request(id)
            || self.has_forward(id)
            || self.has_pending_capture(id)
            || self.has_pending_preset_capture(id)
            || self.has_creation(id)
            || self.has_resource_cleanup(id)
            || self.commands.has_resource_request(id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn has_pending_view_for(&self, stream: &str) -> bool {
        self.view_writes.pending_for(stream)
    }

    /// 저널 worker가 다른 프로세스가 같은 홈을 쓰고 있어 시작하지 못했다.
    #[cfg(feature = "gui")]
    pub(crate) fn home_in_use(&self) -> bool {
        self.startup_failure
            .as_ref()
            .is_some_and(crate::runtime::journal_product::StartupFailure::is_home_in_use)
    }

    pub(crate) fn is_ready(&self, id: EngineId) -> bool {
        self.started
            && self.halted.is_none()
            && !self.opening.contains_key(&id)
            && !self.has_creation(id)
            && !self.has_resource_cleanup(id)
            && self.restorations.is_ready(id)
    }

    fn submit_restore_reads(&mut self) -> Result<(), String> {
        self.restorations
            .submit(&self.worker, &mut self.next_ticket)
    }

    #[cfg(feature = "gui")]
    pub(crate) fn queue_view(
        &mut self,
        view: crate::runtime::journal_product::view_record::StoredView,
    ) {
        if !self.is_halted() {
            if let Err(error) = self.view_writes.queue(view) {
                self.halted = Some(error);
                return;
            }
            (self.wake)();
        }
    }
    #[cfg(feature = "gui")]
    pub(crate) fn latest_view_sequence(&self) -> u64 {
        self.view_writes.sequence()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn has_pending_view_writes(&self) -> bool {
        !self.is_halted()
            && (self.has_pending_preset_captures()
                || !self.capture_requests.is_empty()
                || !self.captures.is_empty()
                || self.view_writes.pending())
    }
    #[cfg(feature = "gui")]
    fn submit_view_writes(&mut self) -> Result<(), String> {
        self.view_writes.submit(&self.worker, &mut self.next_ticket)
    }

    fn submit_openings(&mut self) -> Result<(), String> {
        for opening in self
            .opening
            .values_mut()
            .filter(|opening| opening.ticket.is_none())
        {
            let ticket = self.next_ticket;
            self.next_ticket = ticket
                .checked_add(1)
                .ok_or("journal ticket range exhausted")?;
            self.worker
                .submit(Request {
                    ticket,
                    work: Work::OpenEngine {
                        selection: opening.selection.clone(),
                        normal_category_name: "normal".into(),
                        surface_floor: opening.surface_floor.clone(),
                    },
                })
                .map_err(|error| format!("engine bootstrap submission: {error:?}"))?;
            opening.ticket = Some(ticket);
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod bulk_restore_tests;
#[cfg(all(test, unix, feature = "gui"))]
mod content_off_tests;
#[cfg(all(test, unix))]
mod damaged_stream_tests;
#[cfg(all(test, unix))]
mod payload_failure_tests;
#[cfg(test)]
mod stall_budget;
#[cfg(test)]
mod test_shell;
#[cfg(all(test, unix))]
mod tests;
#[cfg(all(test, unix))]
mod unowned_tests;

#[cfg(all(test, unix, feature = "gui"))]
mod ephemeral_fallback_tests;
#[cfg(all(test, unix))]
mod legacy_import_failure_tests;
#[cfg(all(test, unix, feature = "gui"))]
mod view_failure_tests;
#[cfg(all(test, unix, feature = "gui"))]
mod view_tests;
