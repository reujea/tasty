//! Core에 저장소와 외부 기능 구현을 주입한다. 실제 앱 조립은 boot::wiring에서 맡는다.

use std::sync::{Arc, Mutex, OnceLock};

#[cfg(test)]
use tasty_memory::MemoryStorage;
use tasty_presets::PresetStore;
use tasty_settings::SettingsStorage;
use tasty_themes::ThemeStorage;

use super::AppServices;
use crate::ports::clipboard::ClipboardSystem;
use crate::ports::clock::Clock;
use crate::ports::home::HomeDirectory;
use crate::ports::notification_sound::NotificationSoundPlayer;

/// 필수 port가 모두 있어야 build가 성공한다.
pub(crate) struct AppServicesBuilder {
    clock: Option<Arc<dyn Clock>>,
    clipboard: Option<Arc<dyn ClipboardSystem>>,
    home: Option<Arc<dyn HomeDirectory>>,
    sound_player: Option<Arc<dyn NotificationSoundPlayer>>,
    #[cfg(test)]
    memory: Option<Arc<Mutex<dyn MemoryStorage>>>,
    themes: Option<Arc<dyn ThemeStorage>>,
    preset_store: Option<Arc<Mutex<PresetStore>>>,
    settings_storage: Option<Arc<dyn SettingsStorage>>,
    /// 저장소 진단 정보는 선택 사항이며 없어도 build할 수 있다.
    db_latency: Option<Arc<tasty_memory::DbLatencyStats>>,
    memory_pragmas: Option<tasty_memory::pragma::AppliedPragmas>,
    memory_init_fallback: Option<tasty_memory::InitFallback>,
    memory_writer: Option<super::MemoryWriterParts>,
    file_writer: Option<crate::app::file_writer::FileWriterParts>,
}

impl AppServicesBuilder {
    pub(crate) fn new() -> Self {
        Self {
            clock: None,
            clipboard: None,
            home: None,
            sound_player: None,
            #[cfg(test)]
            memory: None,
            themes: None,
            preset_store: None,
            settings_storage: None,
            db_latency: None,
            memory_pragmas: None,
            memory_init_fallback: None,
            memory_writer: None,
            file_writer: None,
        }
    }

    pub(crate) fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }
    pub(crate) fn with_clipboard(mut self, clipboard: Arc<dyn ClipboardSystem>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }
    pub(crate) fn with_home(mut self, home: Arc<dyn HomeDirectory>) -> Self {
        self.home = Some(home);
        self
    }
    pub(crate) fn with_sound_player(
        mut self,
        sound_player: Arc<dyn NotificationSoundPlayer>,
    ) -> Self {
        self.sound_player = Some(sound_player);
        self
    }
    /// 시험 전용. writer를 함께 주지 않으면 이 저장소에서 바로 실행하는 구성이다.
    #[cfg(test)]
    pub(crate) fn with_memory(mut self, memory: Arc<Mutex<dyn MemoryStorage>>) -> Self {
        self.memory = Some(memory);
        self
    }
    pub(crate) fn with_themes(mut self, themes: Arc<dyn ThemeStorage>) -> Self {
        self.themes = Some(themes);
        self
    }
    pub(crate) fn with_preset_store(mut self, preset_store: Arc<Mutex<PresetStore>>) -> Self {
        self.preset_store = Some(preset_store);
        self
    }
    /// 저장소의 실제 지연 계측기를 연결한다. 생략하면 새 빈 계측기를 쓴다.
    pub(crate) fn with_db_latency(mut self, stats: Arc<tasty_memory::DbLatencyStats>) -> Self {
        self.db_latency = Some(stats);
        self
    }
    /// 저장소를 열 때 읽은 pragma 결과. 생략하면 진단은 null이다.
    pub(crate) fn with_memory_pragmas(
        mut self,
        pragmas: tasty_memory::pragma::AppliedPragmas,
    ) -> Self {
        self.memory_pragmas = Some(pragmas);
        self
    }
    /// 저장소 초기화 실패 후 대체 저장소를 쓴 이유. 생략했다고 실제 저장소의 내구성을 검증한 것은 아니다.
    pub(crate) fn with_memory_init_fallback(
        mut self,
        fallback: Option<tasty_memory::InitFallback>,
    ) -> Self {
        self.memory_init_fallback = fallback;
        self
    }
    /// memory.db 전담 writer. 생략하면 writer 없는 구성이다.
    /// `changes`·`wake`는 writer를 만들 때 넘긴 변경 버퍼·깨우기와 같아야 한다.
    pub(crate) fn with_memory_writer(
        mut self,
        writer: super::HostMemoryWriter,
        changes: Arc<tasty_memory::writer::ChangeFeed>,
        wake: Arc<tasty_utils::batch_writer::WakeLatch>,
    ) -> Self {
        self.memory_writer = Some(super::MemoryWriterParts {
            writer,
            changes,
            wake,
        });
        self
    }
    /// 사용자 설정 파일 전담 writer. 생략하면 설정 저장을 호출 스레드에서 바로 하는 구성이다.
    /// writer는 `with_settings_storage`와 같은 저장소로 만들어야 한다.
    pub(crate) fn with_file_writer(
        mut self,
        writer: crate::app::file_writer::FileWriterParts,
    ) -> Self {
        self.file_writer = Some(writer);
        self
    }
    pub(crate) fn with_settings_storage(mut self, settings: Arc<dyn SettingsStorage>) -> Self {
        self.settings_storage = Some(settings);
        self
    }

    pub(crate) fn build(self) -> anyhow::Result<AppServices> {
        let preset_store = self
            .preset_store
            .ok_or_else(|| anyhow::anyhow!("PresetStore missing"))?;
        // 제품 구성은 writer만 준다. 시험 구성은 writer 대신, 또는 writer와 같은 저장소를 직접 확인하려고
        // 공유 잠금을 준다.
        #[cfg(test)]
        let memory = self.memory;
        #[cfg(test)]
        if memory.is_none() && self.memory_writer.is_none() {
            anyhow::bail!("MemoryStorage missing");
        }
        let memory_access = match &self.memory_writer {
            Some(parts) => tasty_memory::writer::MemoryAccess::writer(
                parts.writer.handle(),
                parts.changes.clone(),
            ),
            #[cfg(test)]
            None => {
                tasty_memory::writer::MemoryAccess::inline(memory.clone().expect("checked above"))
            }
            #[cfg(not(test))]
            None => anyhow::bail!("memory writer missing"),
        };
        let host_ipc_injector = Arc::new(OnceLock::new());
        let tasks = tasty_task_runtime::TaskService::new(
            memory_access.clone(),
            Arc::clone(&host_ipc_injector),
            Arc::new(crate::app::task_completion::AppCompletionResolver),
        )
        .with_store_fallback(
            self.memory_init_fallback
                .as_ref()
                .map(|f| f.cause.to_string()),
        );
        let settings_storage = self
            .settings_storage
            .ok_or_else(|| anyhow::anyhow!("SettingsStorage missing"))?;
        let home = self
            .home
            .ok_or_else(|| anyhow::anyhow!("HomeDirectory missing"))?;
        let registry_config = home
            .tasty_config()
            .map(|path| path.join("file-handlers.toml"));
        let registries =
            crate::runtime::registries::RuntimeRegistries::new(registry_config.as_deref());
        if let Some(writer) = self.file_writer.as_ref() {
            crate::app::file_consumers::attach(&registries, writer.writer.handle());
        }
        let preset_files = crate::app::preset_commands::PresetCommands::new(
            preset_store.clone(),
            self.file_writer.as_ref().map(|parts| parts.writer.handle()),
        );
        let webhooks = Arc::new(crate::webhook::registry::Registry::new(
            self.file_writer
                .as_ref()
                .map(|parts| Arc::new(parts.writer.handle()) as crate::file_command::Queue),
            crate::webhook::config::config_path(),
        ));
        Ok(AppServices {
            registries,
            #[cfg(feature = "gui")]
            navigation_proofs: Default::default(),
            #[cfg(feature = "gui")]
            explorer_clipboard: Default::default(),
            #[cfg(feature = "gui")]
            file_clipboard: Default::default(),
            profile_detections: tasty_remote::profile_detection::ProfileDetections::with_file_sink(
                crate::app::remote_file_commands::detection_sink(
                    self.file_writer.as_ref().map(|parts| parts.writer.handle()),
                ),
            ),
            approval_store: Arc::new(tasty_approval::ApprovalStore::new()),
            telemetry_seq: Arc::new(tasty_telemetry::TelemetrySeq::new()),
            anomaly_detector: Arc::new(tasty_telemetry::AnomalyDetector::new()),
            attach_heartbeat: Default::default(),
            reconnect_window_secs: Arc::new(std::sync::atomic::AtomicU64::new(
                tasty_settings::DEFAULT_RECONNECT_TIMEOUT_SECS,
            )),
            clock: self.clock.ok_or_else(|| anyhow::anyhow!("Clock missing"))?,
            clipboard: self
                .clipboard
                .ok_or_else(|| anyhow::anyhow!("ClipboardSystem missing"))?,
            home,
            sound_player: self
                .sound_player
                .ok_or_else(|| anyhow::anyhow!("NotificationSoundPlayer missing"))?,
            #[cfg(test)]
            memory,
            themes: self
                .themes
                .ok_or_else(|| anyhow::anyhow!("ThemeStorage missing"))?,
            settings_saves: crate::app::file_writer::SettingsSaves::new(
                self.file_writer.as_ref().map(|w| w.writer.handle()),
                settings_storage.clone(),
            ),
            file_writer: self.file_writer,
            file_drain_reported: false,
            preset_store,
            preset_files,
            host_ipc_injector,
            journal_metrics: None,
            ports: Default::default(),
            webhooks,
            tasks,
            pressure: tasty_telemetry::PressureStats::default(),
            gate: tasty_telemetry::GateStats::default(),
            plugin_wait: std::sync::Arc::new(tasty_telemetry::PluginWaitStats::default()),
            slow_requests: std::sync::Arc::new(tasty_telemetry::SlowRequestLog::default()),
            db_latency: self.db_latency.unwrap_or_default(),
            memory_pragmas: self.memory_pragmas,
            memory_init_fallback: self.memory_init_fallback,
            memory_writer: self.memory_writer,
            memory_access,
            memory_replies: Default::default(),
            telemetry_follow_ups: Default::default(),
            memory_writer_end_reported: false,
            #[cfg(any(feature = "gui", test))]
            state_writer: None,
            // IPC 서버는 AppServices 이후에 시작하므로 여기서 만든 계측기를 부팅 과정에서 서버에도 넘긴다.
            connections: std::sync::Arc::new(tasty_telemetry::ConnectionStats::default()),
            dispatch: std::sync::Arc::new(tasty_ipc::dispatch::DispatchStats::default()),
        })
    }
}

impl Default for AppServicesBuilder {
    fn default() -> Self {
        Self::new()
    }
}
