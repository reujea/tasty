//! `CoreBuilder` — DI 패턴. production / test 별 adapter 주입.
//!
//! Production wiring 은 `src/boot/wiring.rs` 의 `build_production_core()`.

use std::sync::{Arc, Mutex, OnceLock};

use tasty_memory::MemoryStorage;
use tasty_presets::{PresetStorage, PresetStore};
use tasty_settings::SettingsStorage;
use tasty_themes::ThemeStorage;

use super::Core;
use crate::ports::clipboard::ClipboardSystem;
use crate::ports::clock::Clock;
use crate::ports::fs::FileSystem;
use crate::ports::home::HomeDirectory;
use crate::ports::notification_sound::NotificationSoundPlayer;
use crate::ports::process::ProcessSpawner;

/// Builder for `Core`. 모든 port 가 주입돼야 `build()` 가 성공.
///
/// `preset_store` 는 구체 Arc 로 받고, trait `presets` port 는 빌더가 내부에서
/// *같은 allocation* 으로 coerce — 호출처가 별 두 인자 주입할 필요 없음 +
/// 두 필드의 owner 일관성 자동 보장.
pub(crate) struct CoreBuilder {
    fs: Option<Arc<dyn FileSystem>>,
    clock: Option<Arc<dyn Clock>>,
    clipboard: Option<Arc<dyn ClipboardSystem>>,
    process: Option<Arc<dyn ProcessSpawner>>,
    home: Option<Arc<dyn HomeDirectory>>,
    sound_player: Option<Arc<dyn NotificationSoundPlayer>>,
    memory: Option<Arc<Mutex<dyn MemoryStorage>>>,
    themes: Option<Arc<dyn ThemeStorage>>,
    preset_store: Option<Arc<Mutex<PresetStore>>>,
    settings_storage: Option<Arc<dyn SettingsStorage>>,
    /// port 가 아니다 — 주입 대상 10 port 와 달리 이것이 없어도 `build` 가 성공한다.
    /// 스토어가 열릴 때 그 안에서 태어난 핸들이라, 스토어를 못 여는 조립에서는
    /// 짝이 없는 것이 정상이다.
    db_latency: Option<Arc<tasty_memory::DbLatencyStats>>,
    /// `db_latency` 와 같은 성격이다 — port 가 아니고, 스토어를 못 여는 조립에서는
    /// 없는 것이 정상이다.
    memory_pragmas: Option<tasty_memory::pragma::AppliedPragmas>,
    /// `memory_pragmas` 와 같은 성격이다. 대체 저장소가 아니면 없는 것이 정상이다.
    memory_init_fallback: Option<tasty_memory::InitFallback>,
}

impl CoreBuilder {
    pub(crate) fn new() -> Self {
        Self {
            fs: None,
            clock: None,
            clipboard: None,
            process: None,
            home: None,
            sound_player: None,
            memory: None,
            themes: None,
            preset_store: None,
            settings_storage: None,
            db_latency: None,
            memory_pragmas: None,
            memory_init_fallback: None,
        }
    }

    pub(crate) fn with_fs(mut self, fs: Arc<dyn FileSystem>) -> Self {
        self.fs = Some(fs);
        self
    }
    pub(crate) fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }
    pub(crate) fn with_clipboard(mut self, clipboard: Arc<dyn ClipboardSystem>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }
    pub(crate) fn with_process(mut self, process: Arc<dyn ProcessSpawner>) -> Self {
        self.process = Some(process);
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
    /// 스토어가 자기 안에서 재는 DB 지연 게이지를 `Core` 에 붙인다. 안 부르면
    /// `Core` 는 아무도 안 올리는 게이지를 들고, 진단은 관측 0 으로 답한다.
    pub(crate) fn with_db_latency(mut self, stats: Arc<tasty_memory::DbLatencyStats>) -> Self {
        self.db_latency = Some(stats);
        self
    }
    /// 스토어가 열릴 때 되읽은 연결 pragma 결과를 `Core` 에 붙인다. 안 부르면 진단은
    /// `null` 로 답한다.
    pub(crate) fn with_memory_pragmas(
        mut self,
        pragmas: tasty_memory::pragma::AppliedPragmas,
    ) -> Self {
        self.memory_pragmas = Some(pragmas);
        self
    }
    /// 스토어가 `memory.db` 초기화 실패의 대체면 그 까닭을 `Core` 에 붙인다. `None` 이면
    /// durable 저장소다(ADR-0485).
    pub(crate) fn with_memory_init_fallback(
        mut self,
        fallback: Option<tasty_memory::InitFallback>,
    ) -> Self {
        self.memory_init_fallback = fallback;
        self
    }
    pub(crate) fn with_settings_storage(mut self, settings: Arc<dyn SettingsStorage>) -> Self {
        self.settings_storage = Some(settings);
        self
    }

    /// 모든 10 port 가 주입됐는지 확인 후 Core 생성.
    pub(crate) fn build(self) -> anyhow::Result<Core> {
        let preset_store = self
            .preset_store
            .ok_or_else(|| anyhow::anyhow!("PresetStore missing"))?;
        // `presets` (trait Arc) 는 `preset_store` 와 *같은 allocation* — coerce 로 type 만 변경.
        let presets: Arc<Mutex<dyn PresetStorage>> = preset_store.clone();
        Ok(Core {
            fs: self
                .fs
                .ok_or_else(|| anyhow::anyhow!("FileSystem missing"))?,
            clock: self.clock.ok_or_else(|| anyhow::anyhow!("Clock missing"))?,
            clipboard: self
                .clipboard
                .ok_or_else(|| anyhow::anyhow!("ClipboardSystem missing"))?,
            process: self
                .process
                .ok_or_else(|| anyhow::anyhow!("ProcessSpawner missing"))?,
            home: self
                .home
                .ok_or_else(|| anyhow::anyhow!("HomeDirectory missing"))?,
            sound_player: self
                .sound_player
                .ok_or_else(|| anyhow::anyhow!("NotificationSoundPlayer missing"))?,
            memory: self
                .memory
                .ok_or_else(|| anyhow::anyhow!("MemoryStorage missing"))?,
            themes: self
                .themes
                .ok_or_else(|| anyhow::anyhow!("ThemeStorage missing"))?,
            presets,
            settings_storage: self
                .settings_storage
                .ok_or_else(|| anyhow::anyhow!("SettingsStorage missing"))?,
            preset_store,
            host_ipc_injector: Arc::new(OnceLock::new()),
            runner_registry: Arc::new(crate::core::agent::runner_thread::RunnerRegistry::new()),
            hook_task_waits: Arc::new(crate::core::agent::hook_wait::HookTaskWaits::new()),
            // 주입 대상이 아니다 — 외부 자원이 아니라 이 프로세스의 누계라서
            // production/test 가 다른 구현을 받을 이유가 없다.
            pressure: tasty_telemetry::PressureStats::default(),
            // 주입 대상이 아니다 — `pressure` 와 같은 이유(이 프로세스의 누계).
            gate: tasty_telemetry::GateStats::default(),
            plugin_wait: std::sync::Arc::new(tasty_telemetry::PluginWaitStats::default()),
            slow_requests: std::sync::Arc::new(tasty_telemetry::SlowRequestLog::default()),
            db_latency: self.db_latency.unwrap_or_default(),
            memory_pragmas: self.memory_pragmas,
            memory_init_fallback: self.memory_init_fallback,
            // 주입 대상이 아니다 — IPC 서버가 `Core` 뒤에 뜨므로 여기서 낳고
            // 부팅이 그 핸들을 서버에 건넨다(`Hub::start_ipc`).
            connections: std::sync::Arc::new(tasty_telemetry::ConnectionStats::default()),
            // 주입 대상이 아니다 — `pressure` 와 같은 이유(이 프로세스의 누계).
            dispatch: std::sync::Arc::new(tasty_ipc::dispatch::DispatchStats::default()),
        })
    }
}

impl Default for CoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}
