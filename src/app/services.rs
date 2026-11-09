//! Process-wide application services and execution coordination. Engine structure has another owner.
use crate::app::approval_records::{self, ApprovalTransition};
use crate::app::command::{CoreEvent, DomainIntent, ProcessPtyOutcome};
use crate::core::*;
use crate::ports::{
    clipboard::ClipboardSystem, clock::Clock, home::HomeDirectory,
    notification_sound::NotificationSoundPlayer,
};
use crate::runtime::engine_access::{EngineMut, EngineRef};
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(test)]
use tasty_memory::MemoryStorage;
use tasty_presets::PresetStore;
use tasty_themes::ThemeStorage;
pub(crate) mod builder;
pub(crate) mod file;
pub(crate) mod impl_attach;
pub(crate) mod impl_clipboard;
#[cfg(feature = "gui")]
pub(crate) mod impl_mirror;
pub(crate) mod impl_pty;
pub(crate) mod impl_tab;
pub(crate) mod ipc_facade;
mod live_intent;
pub(crate) mod session;
mod shutdown_observation;
/// 프로세스가 공유하는 port와 저장소 핸들. 창별 데이터는 CoreState에 있다.
#[allow(dead_code)] // 이유: 일부 port는 아직 읽지 않지만 AppServicesBuilder가 같은 port 묶음을 주입하는 인터페이스를 유지한다.
pub(crate) struct AppServices {
    pub(crate) registries: crate::runtime::registries::RuntimeRegistries,
    /// Process-wide services. Engine and View owners carry neither copies nor lookup authority.
    #[cfg(feature = "gui")]
    pub(crate) navigation_proofs: Arc<crate::app::html_runtime::NavigationProofs>,
    /// 모든 창이 함께 쓰는 Explorer 복사·잘라내기 목록. 창의 View 상태를 만들 때 같은 슬롯을 넘긴다.
    #[cfg(feature = "gui")]
    pub(crate) explorer_clipboard: crate::state::explorer_clipboard_slot::ExplorerClipboardSlot,
    /// OS 파일 클립보드. 창마다 같은 핸들을 넘기고, 창이 닫힌 뒤 끝난 잘라내기도 이것으로 정리한다.
    #[cfg(feature = "gui")]
    pub(crate) file_clipboard: crate::clipboard::files::Handle,
    pub(crate) profile_detections: tasty_remote::profile_detection::ProfileDetections,
    pub(crate) approval_store: Arc<tasty_approval::ApprovalStore>,
    pub(crate) telemetry_seq: Arc<tasty_telemetry::TelemetrySeq>,
    pub(crate) anomaly_detector: Arc<tasty_telemetry::AnomalyDetector>,
    /// 이 프로세스가 맺는 새 attach 연결(서버·클라이언트·bulk)의 무응답 제한.
    pub(crate) attach_heartbeat: tasty_ipc::stream::SharedHeartbeatTimeout,
    /// 저장된 원격 행의 재연결 시간(초). 설정을 적용할 때 바뀐다.
    pub(crate) reconnect_window_secs: Arc<std::sync::atomic::AtomicU64>,
    clock: Arc<dyn Clock>,
    clipboard: Arc<dyn ClipboardSystem>,
    home: Arc<dyn HomeDirectory>,
    sound_player: Arc<dyn NotificationSoundPlayer>,

    /// 시험 구성이 저장소를 직접 준비·확인하는 공유 잠금. 제품 구성에는 없고 writer가 저장소를 소유한다.
    #[cfg(test)]
    memory: Option<Arc<Mutex<dyn MemoryStorage>>>,
    themes: Arc<dyn ThemeStorage>,

    /// 뷰에도 같은 Arc를 넘겨 preset 디스크 캐시를 공유한다.
    pub(crate) preset_store: Arc<Mutex<PresetStore>>,
    pub(crate) preset_files: super::preset_commands::PresetCommands,

    /// IPC 서버 시작 뒤 주입한다. runner 등 다른 스레드가 host IPC를 호출할 때 쓴다.
    pub(crate) host_ipc_injector: Arc<OnceLock<tasty_ipc::host_call::HostIpcInjector>>,

    /// Journal worker가 갱신하는 캐시만 공유한다. worker 없는 시험 구성에는 없다.
    journal_metrics: Option<Arc<crate::runtime::journal_product::JournalMetrics>>,

    /// 이 인스턴스가 bind한 IPC·웹훅 포트. 포트가 필요한 곳은 여기서 읽는다.
    pub(crate) ports: Arc<crate::runtime_ports::RuntimePorts>,
    pub(crate) webhooks: Arc<crate::webhook::registry::Registry>,

    /// 러너·완료 대기·훅-작업 연결을 가진 작업 실행 서비스.
    pub(crate) tasks: tasty_task_runtime::TaskService,

    /// GUI·헤드리스 큐와 핸들러가 같은 요청 압력 계측을 사용한다.
    pressure: tasty_telemetry::PressureStats,

    gate: tasty_telemetry::GateStats,

    /// Core를 참조할 수 없는 plugin 통신 크레이트에도 같은 계측 Arc를 넘긴다.
    plugin_wait: Arc<tasty_telemetry::PluginWaitStats>,

    /// 호스트와 plugin 요청의 느린 처리 기록을 공유한다.
    slow_requests: Arc<tasty_telemetry::SlowRequestLog>,

    /// 진단 조회가 MemoryStorage mutex를 잡지 않도록 저장소의 계측 Arc를 별도로 공유한다.
    /// 주입되지 않은 기본 계측기는 관측값이 비어 있다.
    db_latency: Arc<tasty_memory::DbLatencyStats>,

    /// AppServices 뒤에 시작하는 IPC 서버에 넘길 연결 계측기.
    connections: Arc<tasty_telemetry::ConnectionStats>,

    dispatch: Arc<tasty_ipc::dispatch::DispatchStats>,

    /// 저장소를 열 때 읽은 pragma 결과 사본. 진단이 저장소 mutex를 잡지 않게 한다. 없으면 None이다.
    memory_pragmas: Option<tasty_memory::pragma::AppliedPragmas>,

    /// 초기화 실패 뒤 대체 저장소를 쓴 사유. None만으로 실제 쓰기 내구성을 검증할 수는 없다.
    memory_init_fallback: Option<tasty_memory::InitFallback>,

    /// memory.db 전담 writer. 제품 구성에서는 저장소를 소유한다(docs/design/systems/storage-contract.md의
    /// "저장 실행기" 절). writer 없는 시험 구성에는 없다.
    memory_writer: Option<MemoryWriterParts>,
    /// memory.db 요청 핸들. writer가 있으면 그 큐로, 없으면(시험 구성) 호출 스레드에서 바로 실행한다.
    memory_access: tasty_memory::writer::MemoryAccess,
    /// writer 결과를 기다려 응답할 요청.
    memory_replies: crate::ipc::handler::deferred::MemoryReplies,
    /// commit된 telemetry 기록 뒤 창 문맥이 있어야 실행할 cap 액션·RSS 탐지. 메인 루프가 회차마다 비운다.
    telemetry_follow_ups: crate::ipc::handler::telemetry_follow_up::FollowUps,
    /// 예상하지 못한 writer 종료를 한 번만 알린다.
    memory_writer_end_reported: bool,
    /// state.db 전담 writer. GUI 부팅이 DB를 연 뒤 설치한다. 헤드리스와 DB를 열지 못한 실행에는 없다.
    #[cfg(any(feature = "gui", test))]
    state_writer: Option<crate::db::writer::StateWriterParts>,
    /// 사용자 설정 파일 전담 writer. writer 없는 시험 구성에서는 설정 저장을 호출 스레드에서 바로 한다.
    file_writer: Option<crate::app::file_writer::FileWriterParts>,
    /// 설정 저장의 표시·확정 revision과 결과를 기다리는 요청.
    settings_saves: crate::app::file_writer::SettingsSaves,
    /// 종료 때 확정하지 못한 설정을 한 번만 알린다.
    file_drain_reported: bool,
}

/// 호스트의 memory.db writer.
pub(crate) type HostMemoryWriter =
    tasty_memory::writer::MemoryWriter<tasty_memory::writer::WriterStore>;

/// writer와 함께 만든 변경 버퍼와 깨우기.
pub(crate) struct MemoryWriterParts {
    pub(crate) writer: HostMemoryWriter,
    pub(crate) changes: Arc<tasty_memory::writer::ChangeFeed>,
    pub(crate) wake: Arc<tasty_utils::batch_writer::WakeLatch>,
}

/// GUI·헤드리스의 plugin 매니저에 함께 주입할 계측 핸들.
#[derive(Clone)]
pub(crate) struct PluginGauges {
    pub(crate) plugin_wait: Arc<tasty_telemetry::PluginWaitStats>,
    pub(crate) slow_requests: Arc<tasty_telemetry::SlowRequestLog>,
}

impl AppServices {
    pub(crate) fn now_instant(&self) -> std::time::Instant {
        self.clock.now_instant()
    }

    pub(crate) fn telemetry_follow_ups(
        &self,
    ) -> &crate::ipc::handler::telemetry_follow_up::FollowUps {
        &self.telemetry_follow_ups
    }

    pub(crate) fn pressure(&self) -> &tasty_telemetry::PressureStats {
        &self.pressure
    }

    pub(crate) fn gate(&self) -> &tasty_telemetry::GateStats {
        &self.gate
    }

    pub(crate) fn plugin_wait(&self) -> &Arc<tasty_telemetry::PluginWaitStats> {
        &self.plugin_wait
    }

    pub(crate) fn slow_requests(&self) -> &Arc<tasty_telemetry::SlowRequestLog> {
        &self.slow_requests
    }

    pub(crate) fn plugin_gauges(&self) -> PluginGauges {
        PluginGauges {
            plugin_wait: self.plugin_wait.clone(),
            slow_requests: self.slow_requests.clone(),
        }
    }

    pub(crate) fn db_latency(&self) -> &Arc<tasty_memory::DbLatencyStats> {
        &self.db_latency
    }

    pub(crate) fn memory_pragmas(&self) -> Option<&tasty_memory::pragma::AppliedPragmas> {
        self.memory_pragmas.as_ref()
    }

    pub(crate) fn memory_init_fallback(&self) -> Option<&tasty_memory::InitFallback> {
        self.memory_init_fallback.as_ref()
    }

    pub(crate) fn memory_access(&self) -> &tasty_memory::writer::MemoryAccess {
        &self.memory_access
    }

    pub(crate) fn memory_replies(&self) -> &crate::ipc::handler::deferred::MemoryReplies {
        &self.memory_replies
    }

    /// 메모리에 적용한 설정의 저장을 file writer에 넣는다. 기다리지 않는다. 결과는 메인 루프가
    /// [`Self::take_settled_settings`]로 거두어 `done`과 사용자 출처의 토스트로 알린다.
    pub(crate) fn submit_settings_save(
        &mut self,
        settings: tasty_settings::Settings,
        origin: Option<crate::intent::IntentOrigin>,
        done: Option<crate::app::settings_change::SettingsDone>,
    ) {
        self.settings_saves.submit(settings, origin, done);
    }

    /// New windows use the latest applied snapshot instead of reloading an older queued file.
    #[cfg(any(feature = "gui", test))]
    pub(crate) fn latest_settings(&self) -> Option<&tasty_settings::Settings> {
        self.settings_saves.latest()
    }

    /// Nonconsuming settings persistence metadata, also available without a GUI.
    pub(crate) fn settings_save_status(&self) -> crate::app::file_writer::SettingsSaveSnapshot {
        self.settings_saves.snapshot()
    }

    /// 끝난 설정 저장을 꺼낸다. 깨우기 확인은 [`Self::poll_file_writer`]가 한다.
    pub(crate) fn take_settled_settings(&mut self) -> Vec<crate::app::file_writer::SettledSave> {
        self.settings_saves.reap()
    }

    pub(crate) fn start_webhooks(
        &self,
        reservation: Option<crate::webhook::Reservation>,
        injector: tasty_ipc::host_call::HostIpcInjector,
    ) -> crate::webhook::WebhookInitReport {
        crate::webhook::start(
            reservation,
            injector,
            self.ports.clone(),
            self.webhooks.clone(),
        )
    }

    /// 메인 루프 회차마다 결과를 거두기 전에 부른다.
    pub(crate) fn poll_file_writer(&mut self) {
        if let Some(parts) = self.file_writer.as_mut() {
            parts.poll();
        }
        crate::app::file_consumers::poll(&self.registries);
        self.preset_files.poll();
        self.webhooks.poll();
    }

    /// 확정하지 못한 최신 설정을 한 번 더 넣은 뒤 접수를 멈추고 마지막 flush 뒤 worker를 끝내게 한다.
    /// 남은 worker 수를 돌려준다. 기다리지 않는다. 설정 저장 producer가 멈춘 뒤 부른다.
    pub(crate) fn poll_file_writer_shutdown(&mut self) -> usize {
        crate::app::file_consumers::before_shutdown(&self.registries);
        self.preset_files.poll();
        self.settings_saves.collect_completed();
        self.settings_saves.retry_latest_before_shutdown();
        let webhook_waiting = self.webhooks.poll_shutdown();
        // Both producers must finish admission before the shared writer fence.
        let child_admission_drained = self.registries.child_terminals.admission_drained();
        let remaining = if webhook_waiting != 0 || !child_admission_drained {
            1
        } else {
            self.file_writer
                .as_mut()
                .map_or(0, |parts| parts.writer.poll_shutdown())
        };
        self.settings_saves.collect_completed();
        self.preset_files.poll();
        if remaining == 0 {
            crate::app::file_consumers::after_shutdown(&self.registries);
        }
        if remaining == 0
            && self.settings_saves.pending() == 0
            && !std::mem::replace(&mut self.file_drain_reported, true)
            && let Some((visible, committed)) = self.settings_saves.unsaved()
        {
            tracing::warn!(
                visible,
                committed,
                "settings applied in this run were not all saved"
            );
        }
        remaining
    }

    /// Reuse the existing writer for owned consumer commands; never creates a worker.
    pub(crate) fn file_command_queue(
        &self,
    ) -> Option<tasty_utils::batch_writer::Handle<crate::app::file_writer::FileExecutor>> {
        self.file_writer.as_ref().map(|parts| parts.writer.handle())
    }

    /// file writer 계측. writer가 없으면 None이다.
    pub(crate) fn file_writer_stats(
        &self,
    ) -> Option<(
        tasty_utils::batch_writer::WriterStatsSnapshot,
        crate::app::file_writer::FileWriteCounts,
    )> {
        self.file_command_queue().map(|queue| {
            let writes = self.file_writer.as_ref().expect("queue has a writer");
            (queue.stats(), writes.stats.snapshot())
        })
    }

    /// 지연 응답의 결과가 메인 루프 안에서 생겼을 때 다음 회차가 응답을 거두도록 깨운다.
    /// writer가 없는 시험 구성은 호출자가 직접 거둔다.
    pub(crate) fn reply_wake(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        match &self.memory_writer {
            Some(parts) => parts.wake.notifier(),
            None => std::sync::Arc::new(|| {}),
        }
    }

    /// 원 호출자를 붙여 writer 결과를 기다리게 한다.
    pub(crate) fn defer_reply(
        &self,
        deferred: crate::ipc::handler::deferred::Deferred,
        to: crate::ipc::handler::ReplyTo,
    ) {
        self.memory_replies.push(deferred, to);
    }

    /// 끝난 writer 결과로 응답한다. IPC 응답은 보내고 plugin 응답은 `plugins`로 보낸다. 메인 루프가
    /// `poll_memory_writer` 뒤에 부른다. plugin 매니저가 없으면 plugin 응답은 버린다.
    pub(crate) fn poll_memory_replies(&self, plugins: Option<&mut crate::plugin::PluginManager>) {
        // writer 결과를 거두는 스레드다. 이 스레드가 writer 결과를 기다리면 debug 빌드에서 멈춘다.
        tasty_memory::writer::mark_result_reaping_thread();
        let replies = crate::ipc::handler::deferred::MemoryReplies::poll(self);
        if replies.is_empty() {
            return;
        }
        let Some(manager) = plugins else {
            tracing::warn!(
                replies = replies.len(),
                "plugin replies finished without a plugin manager"
            );
            return;
        };
        for reply in replies {
            if !manager.send_bound_ipc_result(
                &reply.plugin_id,
                &reply.binding,
                reply.call_id,
                reply.response,
            ) {
                tracing::debug!(
                    plugin = %reply.plugin_id,
                    call = reply.call_id,
                    "plugin process was replaced before its memory reply"
                );
            }
        }
    }

    /// commit된 regular 변경을 commit 순서대로 꺼낸다. 저장소 잠금을 기다리지 않는다.
    pub(crate) fn take_memory_changes(&self) -> Vec<tasty_memory::MemoryChange> {
        self.memory_access.changes().take()
    }

    /// writer 계측. writer가 없는 구성이면 None이다.
    pub(crate) fn memory_writer_stats(
        &self,
    ) -> Option<tasty_utils::batch_writer::WriterStatsSnapshot> {
        self.memory_writer.as_ref().map(|w| w.writer.stats())
    }

    /// 메인 루프가 회차마다 writer 결과를 거두기 전에 부른다. 깨우기를 확인해 그 뒤의 묶음이 다시
    /// 깨우게 하고, 종료를 요청하지 않았는데 worker가 끝났으면 한 번 알린다.
    pub(crate) fn poll_memory_writer(&mut self) {
        if let Some(parts) = &self.memory_writer {
            parts.wake.acknowledge();
        }
        if !self.memory_writer_end_reported
            && self
                .memory_writer
                .as_ref()
                .is_some_and(|w| w.writer.worker_ended_unexpectedly())
        {
            self.memory_writer_end_reported = true;
            tracing::error!("memory writer ended without a shutdown request");
        }
    }

    /// 접수를 멈추고 마지막 flush 뒤 worker를 끝내도록 하고 남은 worker 수를 돌려준다.
    /// 다른 producer를 모두 멈춘 뒤 부른다. 기다리지 않는다.
    pub(crate) fn poll_memory_writer_shutdown(&mut self) -> usize {
        self.memory_writer
            .as_mut()
            .map_or(0, |parts| parts.writer.poll_shutdown())
    }

    /// GUI 부팅이 state.db writer를 시작한 뒤 넘긴다.
    #[cfg(feature = "gui")]
    pub(crate) fn install_state_writer(&mut self, parts: crate::db::writer::StateWriterParts) {
        self.state_writer = Some(parts);
    }

    /// 메인 루프 회차마다 부른다. 깨우기를 확인하고 최근 파일 저장 결과를 거둔다.
    #[cfg(feature = "gui")]
    pub(crate) fn poll_state_writer(&mut self) {
        if let Some(parts) = self.state_writer.as_mut() {
            parts.poll();
        }
    }

    /// 접수를 멈추고 마지막 flush 뒤 worker를 끝내도록 하고 남은 worker 수를 돌려준다. 기다리지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) fn poll_state_writer_shutdown(&mut self) -> usize {
        self.state_writer
            .as_mut()
            .map_or(0, crate::db::writer::StateWriterParts::poll_shutdown)
    }

    /// Cached semantic recent-list dirty period; no receipt or storage access.
    pub(crate) fn state_recent_status(&self) -> Option<crate::recent_files::RecentSaveSnapshot> {
        #[cfg(any(feature = "gui", test))]
        {
            self.state_writer
                .as_ref()
                .map(|parts| parts.recent.save_snapshot())
        }
        #[cfg(not(any(feature = "gui", test)))]
        {
            None
        }
    }

    /// state.db writer 계측. writer가 없으면 None이다.
    pub(crate) fn state_writer_stats(
        &self,
    ) -> Option<tasty_utils::batch_writer::WriterStatsSnapshot> {
        #[cfg(any(feature = "gui", test))]
        {
            self.state_writer.as_ref().map(|w| w.writer.stats())
        }
        #[cfg(not(any(feature = "gui", test)))]
        {
            None
        }
    }

    /// state.db를 열 때 확인한 pragma 사본. 연결에 닿지 않는다.
    pub(crate) fn state_pragmas(&self) -> Option<&tasty_memory::pragma::AppliedPragmas> {
        #[cfg(any(feature = "gui", test))]
        {
            self.state_writer.as_ref().map(|w| &w.pragmas)
        }
        #[cfg(not(any(feature = "gui", test)))]
        {
            None
        }
    }

    /// state.db 쓰기 commit 계측. writer가 없으면 None이다.
    pub(crate) fn state_db_latency(&self) -> Option<tasty_memory::DbLatencySnapshot> {
        #[cfg(any(feature = "gui", test))]
        {
            self.state_writer.as_ref().map(|w| w.latency.snapshot())
        }
        #[cfg(not(any(feature = "gui", test)))]
        {
            None
        }
    }

    pub(crate) fn connections(&self) -> &Arc<tasty_telemetry::ConnectionStats> {
        &self.connections
    }

    pub(crate) fn dispatch(&self) -> &Arc<tasty_ipc::dispatch::DispatchStats> {
        &self.dispatch
    }

    /// 감사·보관 기간 계산은 벽시계를 사용하므로 monotonic 시각과 별도로 제공한다.
    pub(crate) fn now_unix_millis(&self) -> i64 {
        self.clock.now_unix_millis()
    }

    pub(crate) fn send_surface_message(
        &mut self,
        engine: &mut EngineMut<'_>,
        from: u32,
        to: u32,
        content: String,
    ) -> u32 {
        engine.send_message(from, to, content)
    }

    pub(crate) fn read_surface_messages(
        &mut self,
        engine: &mut EngineMut<'_>,
        sid: u32,
        from: Option<u32>,
        peek: bool,
    ) -> Vec<state::SurfaceMessage> {
        engine.read_messages(sid, from, peek)
    }

    pub(crate) fn clear_surface_messages(&mut self, engine: &mut EngineMut<'_>, sid: u32) {
        engine.clear_messages(sid);
    }

    pub(crate) fn observer_register(
        &mut self,
        engine: &mut EngineMut<'_>,
        spec: crate::output_observer::ObserverSpec,
    ) -> Result<u64, crate::output_observer::ObserverError> {
        let id = engine
            .observer_router
            .register(spec, self.memory_access.clone())?;
        engine.sync_output_event_gates();
        Ok(id)
    }

    pub(crate) fn observer_unregister(
        &mut self,
        engine: &mut EngineMut<'_>,
        observer_id: u64,
    ) -> Result<(), crate::output_observer::ObserverError> {
        engine.observer_router.unregister(observer_id)?;
        engine.sync_output_event_gates();
        Ok(())
    }

    pub(crate) fn observer_list(
        &self,
        engine: &EngineRef<'_>,
    ) -> Vec<crate::output_observer::ObserverInfo> {
        engine.observer_router.list()
    }

    pub(crate) fn observer_info(
        &self,
        engine: &EngineRef<'_>,
        observer_id: u64,
    ) -> Option<crate::output_observer::ObserverInfo> {
        engine.observer_router.info(observer_id)
    }

    /// hook을 등록한 직후 출력 이벤트 허용 상태도 맞춘다.
    /// parser 스레드가 이미 출력 처리 중이므로 다음 process_surface까지 미루면 중간 출력 이벤트를 놓칠 수 있다.
    pub(crate) fn register_surface_hook(
        &mut self,
        engine: &mut EngineMut<'_>,
        surface_id: u32,
        event: tasty_hooks::HookEvent,
        binding: tasty_hooks::HookBinding,
        once: bool,
    ) -> u64 {
        let id = engine
            .hooks
            .add_surface_hook(surface_id, event, binding, once);
        engine.sync_output_event_gates();
        id
    }

    pub(crate) fn unregister_surface_hook(
        &mut self,
        engine: &mut EngineMut<'_>,
        hook_id: u64,
    ) -> bool {
        let removed = engine.hooks.remove_surface_hook(hook_id);
        engine.sync_output_event_gates();
        removed
    }

    pub(crate) fn register_global_hook(
        &mut self,
        engine: &mut EngineMut<'_>,
        condition: crate::global_hooks::HookCondition,
        command: String,
        label: Option<String>,
    ) -> u32 {
        engine.hooks.add_global_hook(condition, command, label)
    }

    pub(crate) fn unregister_global_hook(
        &mut self,
        engine: &mut EngineMut<'_>,
        hook_id: u32,
    ) -> bool {
        engine.hooks.remove_global_hook(hook_id)
    }

    /// 발화한 훅 바인딩의 실행기. IPC 주입기가 아직 없으면 IpcSequence handler를 건너뛴다.
    pub(crate) fn hook_executor(&self) -> crate::hook_runtime::HookExecutor {
        crate::hook_runtime::HookExecutor::new(self.host_ipc_injector.get().cloned())
    }

    /// 승인 요청을 등록하고 그 기록을 제출한다. 호출자는 기록을 다시 제출하지 않는다.
    pub(crate) fn request_approval(
        &mut self,
        req: tasty_approval::ApprovalRequest,
    ) -> Result<ApprovalTransition, tasty_approval::ApprovalError> {
        let change = self.approval_store.request(req)?;
        Ok(approval_records::submit(self.memory_access(), change))
    }

    /// 응답을 적용하고 그 기록을 제출한다. 권한 상승 요청을 승인하면 전이 직후, 기록 제출 전에
    /// 그 agent 세션에 권한을 부여한다. IPC와 GUI 팝업 응답이 모두 이 경로를 쓴다.
    pub(crate) fn respond_approval(
        &mut self,
        req_id: &tasty_approval::ApprovalId,
        choice: String,
        by: tasty_approval::Responder,
        comment: Option<String>,
    ) -> Result<ApprovalTransition, tasty_approval::ApprovalError> {
        let change = self
            .approval_store
            .respond(req_id, choice.clone(), by, comment)?;
        crate::app::approval_grant::apply_elevation_grant_if_any(self, &change.record, &choice);
        Ok(approval_records::submit(self.memory_access(), change))
    }

    /// 요청을 취소하고 그 기록을 제출한다.
    pub(crate) fn cancel_approval(
        &mut self,
        req_id: &tasty_approval::ApprovalId,
    ) -> Result<ApprovalTransition, tasty_approval::ApprovalError> {
        let change = self.approval_store.cancel(req_id)?;
        Ok(approval_records::submit(self.memory_access(), change))
    }

    /// 캡처 worker와 원격 클립보드 처리에도 같은 구현을 주입한다.
    pub(crate) fn clipboard_arc(&self) -> Arc<dyn ClipboardSystem> {
        self.clipboard.clone()
    }

    /// 시험이 클립보드 포트를 바꾼다 — 헤드리스 App 은 서비스 빌더를 거치지 않고 만든다.
    #[cfg(all(test, not(feature = "gui")))]
    pub(crate) fn replace_clipboard_for_test(&mut self, clipboard: Arc<dyn ClipboardSystem>) {
        self.clipboard = clipboard;
    }

    /// App 소유 journal의 캐시를 연결한다. 저장소나 executor를 공유하지 않는다.
    pub(crate) fn with_journal_metrics(
        mut self,
        metrics: Arc<crate::runtime::journal_product::JournalMetrics>,
    ) -> Self {
        self.journal_metrics = Some(metrics);
        self
    }

    pub(crate) fn journal_metrics_snapshot(
        &self,
    ) -> Option<crate::runtime::journal_product::JournalMetricsSnapshot> {
        self.journal_metrics
            .as_ref()
            .map(|metrics| metrics.snapshot())
    }

    /// IPC 서버 시작 뒤 한 번 주입한다. 두 번째 요청은 경고만 남긴다.
    pub(crate) fn set_host_ipc_injector(&self, injector: tasty_ipc::host_call::HostIpcInjector) {
        if self.host_ipc_injector.set(injector).is_err() {
            tracing::warn!("host_ipc_injector already initialized");
        }
    }

    /// IPC 서버 bind 뒤 한 번 기록한다. IPC 서버는 loopback의 OS 지정 포트를 쓴다.
    pub(crate) fn record_ipc_port(&self, port: u16) {
        self.ports.record_ipc(crate::runtime_ports::BoundPort {
            addr: std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            source: crate::runtime_ports::PortSource::Dynamic,
        });
    }

    /// 자기 자신을 향한 attach 요청을 거절할 때 쓴다. IPC 서버가 없으면 None이다.
    #[cfg(feature = "gui")]
    pub(crate) fn own_ipc_port(&self) -> Option<u16> {
        self.ports.ipc_port()
    }

    pub(crate) fn host_ipc_injector_arc(
        &self,
    ) -> Arc<OnceLock<tasty_ipc::host_call::HostIpcInjector>> {
        self.host_ipc_injector.clone()
    }

    /// 시험 전용. 저장소 잠금을 잡고 콜백을 실행한다. 콜백이 남긴 변경은 writer가 옮긴 변경 뒤에
    /// 붙인다.
    #[cfg(test)]
    pub(crate) fn with_memory<R>(
        &self,
        f: impl FnOnce(&mut dyn tasty_memory::MemoryStorage) -> R,
    ) -> R {
        let memory = self
            .memory
            .as_ref()
            .expect("this test core was built without a shared store");
        let mut guard = crate::poison::recover_mutex(memory.lock(), MEMORY_WHAT, &MEMORY_POISONED);
        let result = f(&mut *guard);
        self.memory_access
            .changes()
            .extend_for_tests(guard.take_pending_changes());
        result
    }

    #[cfg(feature = "gui")]
    pub(crate) fn sound_player(&self) -> &Arc<dyn NotificationSoundPlayer> {
        &self.sound_player
    }
}

pub(crate) mod barrier;
pub(crate) mod lease;
pub(crate) mod ratelimit;
pub(crate) mod semaphore;

mod surface;
#[cfg(feature = "gui")]
pub(crate) use surface::RemoteConnParams;
#[cfg(feature = "gui")]
pub(crate) use surface::no_application_state;
pub(crate) use surface::{clipboard_set_text, spawn_remote_workspaces};
#[cfg(debug_assertions)]
mod surface_debug;
#[cfg(debug_assertions)]
pub(crate) use surface_debug::{fullscreen_list, lua_eval};
