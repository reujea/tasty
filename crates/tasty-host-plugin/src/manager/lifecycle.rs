//! Plugin 생명주기: 인스턴스 생성, listener bind, discover→spawn, healthcheck restart 후
//! plugin process 정리, enable/disable, 권한 갱신, log path 조회.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;
use std::time::Instant;

use tasty_plugin_protocol::host_port::SurfaceRegistry;

use crate::handle_channel::HandleListener;
use crate::listener::HostListener;
use crate::process::{CHILD_EXIT_POLL_INTERVAL, PluginProcess, ShutdownBatch};
use crate::registry_state::PluginsConfig;
use tasty_ipc::ipc_namespace::IpcNamespaceRegistry;
use tasty_plugin_manifest::{Permission, PluginPackage};

use super::{
    AUTO_RELOAD_POLL_INTERVAL, PING_INTERVAL, PLUGIN_SHUTDOWN_TIMEOUT, PluginManager, PluginTick,
    RESTART_FAILURE_LIMIT, RESTART_FAILURE_WINDOW, RSS_SAMPLE_INTERVAL,
};

/// 완료 전략의 owner는 첫 IPC namespace 접두어이며, 없으면 manifest id를 쓴다.
/// poll 메서드의 접두어와 비교하므로 등록·해제에서 같은 값을 사용해야 한다.
fn completion_strategy_owner_id(pkg: &PluginPackage) -> &str {
    pkg.manifest
        .contributes
        .ipc_namespace
        .first()
        .map(|ns| ns.prefix.as_str())
        .unwrap_or(pkg.manifest.id.as_str())
}

impl PluginManager {
    /// 부팅 시 등록하는 plugin 주기 작업. auto-reload 는 flag 가 켜질 때만 등록된다
    /// ([`PluginManager::set_auto_reload_enabled`]).
    fn initial_timers(now: Instant) -> tasty_timer::TimerHub<PluginTick> {
        let mut hub = tasty_timer::TimerHub::new();
        hub.every(
            PluginTick::Ping,
            PING_INTERVAL,
            tasty_timer::Precision::Strict,
            now,
        );
        // RSS 측정은 ping 주기만큼 늦춰도 되므로 같은 길이의 slack을 허용한다.
        hub.every(
            PluginTick::Rss,
            RSS_SAMPLE_INTERVAL,
            tasty_timer::Precision::Lax {
                slack: PING_INTERVAL,
            },
            now,
        );
        hub
    }

    /// auto-reload를 켤 때만 타이머를 등록한다. 꺼져 있으면 wakeup 시각에 포함하지 않는다.
    pub(super) fn set_auto_reload_enabled(&mut self, enabled: bool, now: Instant) {
        self.auto_reload_enabled = enabled;
        if enabled {
            self.timers.every(
                PluginTick::AutoReload,
                AUTO_RELOAD_POLL_INTERVAL,
                tasty_timer::Precision::Strict,
                now,
            );
        } else {
            self.timers.cancel(PluginTick::AutoReload);
        }
    }

    /// 다음 wakeup 요청 시각. 호스트는 자신의 타이머 시각과 비교해 빠른 쪽을 쓴다.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.timers.next_deadline()
    }

    /// 내부 PluginTick을 표시용 이름으로 바꾼 조회 전용 스냅샷. 호스트의 timer.list에 합친다.
    pub fn timer_snapshot(&self) -> Vec<tasty_timer::TimerSnapshot<&'static str>> {
        self.timers
            .snapshot()
            .into_iter()
            .map(|s| tasty_timer::TimerSnapshot {
                key: match s.key {
                    PluginTick::Ping => "PluginPing",
                    PluginTick::Rss => "PluginRss",
                    PluginTick::AutoReload => "PluginAutoReload",
                    PluginTick::Retire => "PluginRetire",
                },
                interval: s.interval,
                next_due: s.next_due,
                precision: s.precision,
                last_fired: s.last_fired,
            })
            .collect()
    }

    /// file_format/file_handler에 no-op 구현을 쓰는 단위 테스트용 생성자.
    #[cfg(test)]
    pub fn new(waker: tasty_terminal::waker_factory::SharedWakerFactory) -> Self {
        struct StubFormat;
        impl tasty_plugin_protocol::host_port::FileFormatRegistryPort for StubFormat {
            fn install_plugin_detectors(&self, _: &str, _: &[serde_json::Value]) {}
            fn uninstall_plugin(&self, _: &str) {}
        }
        struct StubHandler;
        impl tasty_plugin_protocol::host_port::FileHandlerRegistryPort for StubHandler {
            fn install_plugin_handlers(&self, _: &str, _: &[serde_json::Value]) {}
            fn uninstall_plugin(&self, _: &str) {}
        }
        Self::with_registries(waker, Arc::new(StubFormat), Arc::new(StubHandler))
    }

    /// CoreState 와 같은 Arc 를 공유하기 위한 생성자.
    pub fn with_registries(
        waker: tasty_terminal::waker_factory::SharedWakerFactory,
        file_format: Arc<dyn tasty_plugin_protocol::host_port::FileFormatRegistryPort>,
        file_handler: Arc<dyn tasty_plugin_protocol::host_port::FileHandlerRegistryPort>,
    ) -> Self {
        let log_dir = tasty_utils::path::tasty_home()
            .map(|d| d.join("plugins-logs"))
            .unwrap_or_else(|| PathBuf::from("./plugin-logs"));
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            tracing::warn!("plugin log dir {} create failed: {e}", log_dir.display());
        }
        let (host_cmd_tx, host_cmd_rx) = mpsc::channel();
        // 플러그인 수명 결박 reaper. Windows 는 Job Object 생성을 시도하고, 실패 시
        // 결박 없이 기존 kill 기반 정리로 degrade. 비-Windows 는 무조건 성공(stub).
        let plugin_reaper = crate::reaper::PluginReaper::new().unwrap_or_else(|e| {
            tracing::warn!("plugin reaper init failed — plugin lifetime binding disabled: {e}");
            crate::reaper::PluginReaper::disabled()
        });
        Self {
            packages: Vec::new(),
            rejected: Vec::new(),
            reserved: Vec::new(),
            processes: HashMap::new(),
            config: PluginsConfig::load(),
            waker,
            listener: None,
            handle_listener: None,
            log_dir,
            next_request_id: AtomicU64::new(1),
            timers: Self::initial_timers(Instant::now()),
            spawn_failures: HashMap::new(),
            auto_disabled: std::collections::HashSet::new(),
            start_tracker: Default::default(),
            plugin_binary_mtimes: HashMap::new(),
            plugin_manifest_versions: HashMap::new(),
            auto_reload_enabled: false,
            surface_registry: None,
            registered_plugins: std::collections::HashSet::new(),
            host_cmd_tx,
            host_cmd_rx,
            retirement_control: Default::default(),
            surfaces: HashMap::new(),
            pending_requests: HashMap::new(),
            plugin_wait: None,
            slow_requests: None,
            channel_ledger: crate::process::channel_bytes::ChannelLedger::process_wide(),
            plugin_permissions: HashMap::new(),
            pending_plugin_calls: Vec::new(),
            command_registry: crate::command_registry::PluginCommandRegistry::new(),
            settings_pages: crate::settings_registry::SettingsPageRegistry::new(),
            ipc_namespaces: std::sync::Arc::new(
                std::sync::RwLock::new(IpcNamespaceRegistry::new()),
            ),
            plugin_buffers: HashMap::new(),
            next_buffer_id: AtomicU64::new(1),
            egui_mesh_frames: HashMap::new(),
            popup_mesh_frames: HashMap::new(),
            extensions: crate::extension_registry::ExtensionRegistry::new(),
            inherited_env: std::collections::BTreeMap::new(),
            namespace_owners: std::collections::BTreeMap::new(),
            publishes_shared_tables: false,
            hook_failures: HashMap::new(),
            namespace_expiries: HashMap::new(),
            expired_namespace_calls: HashMap::new(),
            event_bus: crate::event_bus::EventBus::new(),
            event_trace_seq: AtomicU64::new(1),
            popup_instances: HashMap::new(),
            next_popup_instance_id: 1,
            pending_surface_popups: Vec::new(),
            banner_instances: HashMap::new(),
            next_banner_instance_id: 1,
            banner_mesh_frames: HashMap::new(),
            invalidated_surfaces: Vec::new(),
            fresh_frame_surfaces: std::collections::HashSet::new(),
            invalidated_popups: Vec::new(),
            invalidated_banners: Vec::new(),
            copied_texts: std::collections::VecDeque::new(),
            sys: sysinfo::System::new(),
            pending_rss_samples: Vec::new(),
            file_format,
            file_handler,
            hook_handler: None,
            completion_strategy: None,
            i18n_registrar: None,
            plugin_reaper,
            shutdown_batch: None,
            retiring: HashMap::new(),
            reaped_generations: Vec::new(),
            mesh_publications: Default::default(),
            closing_surfaces: Default::default(),
        }
    }

    /// 훅 레지스트리를 주입한다. 이후 enable/disable에서 hook_handler를 등록·해제한다.
    pub fn set_hook_handler_registry(
        &mut self,
        registry: Arc<dyn tasty_plugin_protocol::host_port::HookHandlerRegistryPort>,
    ) {
        self.hook_handler = Some(registry);
    }

    /// 완료 전략 레지스트리를 주입한다. 이후 enable/disable에서 전략을 등록·해제한다.
    pub fn set_completion_strategy_registry(
        &mut self,
        registry: Arc<dyn tasty_plugin_protocol::host_port::CompletionStrategyRegistryPort>,
    ) {
        self.completion_strategy = Some(registry);
    }

    /// i18n namespace 등록 기능을 주입한다.
    pub fn set_i18n_registrar(
        &mut self,
        registrar: Arc<dyn tasty_plugin_protocol::host_port::I18nNamespaceRegistrar>,
    ) {
        self.i18n_registrar = Some(registrar);
    }

    /// plugin에 grant된 권한 set을 갱신. 매니페스트 hello 시점 또는 사용자가
    /// grant/revoke 했을 때 호출. plugin process 재시작 없이 즉시 반영된다.
    pub fn set_plugin_permissions(&mut self, plugin_id: &str, perms: HashSet<Permission>) {
        self.plugin_permissions
            .insert(plugin_id.to_string(), Arc::new(perms));
    }

    /// 호스트 main loop이 라우팅하기 위해 plugin IPC 호출을 모두 가져간다.
    pub fn set_surface_registry(&mut self, registry: Arc<dyn SurfaceRegistry>) {
        self.surface_registry = Some(registry);
    }

    /// 설치 디렉터리를 다시 읽어 패키지와 거부 목록을 함께 갱신한다.
    pub fn refresh_packages(&mut self) {
        let found = crate::discovery::discover_all();
        self.packages = found.trusted;
        self.rejected = found.rejected;
        self.note_reserved(found.reserved);
        self.sync_ipc_namespaces_from_packages();
        self.recompute_shared_tables();
    }

    /// 공식 목록에 없는 `com.tasty.*` 폴더 목록. 등록하지 않은 항목이다.
    pub fn reserved_blocked(&self) -> &[crate::discovery::ReservedIdBlocked] {
        &self.reserved
    }

    /// 탐색할 때마다 같은 폴더를 다시 알리지 않는다. 사라졌다 다시 생긴 폴더는 새로 알린다.
    fn note_reserved(&mut self, found: Vec<crate::discovery::ReservedIdBlocked>) {
        for blocked in &found {
            if !self.reserved.contains(blocked) {
                self.start_tracker.report(super::FailureReport::ReservedId {
                    plugin_id: blocked.id.clone(),
                });
            }
        }
        self.reserved = found;
    }

    /// 지운 폴더의 항목만 뺀다. 같은 id 의 다른 폴더는 남는다.
    pub fn forget_reserved(&mut self, dir: &std::path::Path) {
        self.reserved.retain(|blocked| blocked.dir != dir);
    }

    /// Removal waits on the existing reaper without blocking the event loop or reviving a plugin.
    pub fn poll_file_retirement(&mut self, id: &str) -> bool {
        self.cancel_respawn_after_retire(id);
        self.poll_file_retirement_in_place(id)
    }

    /// Retire a running generation for an accepted file swap; the caller polls its completion.
    pub fn begin_file_swap(&mut self, id: &str, stop_running: bool) -> (bool, bool) {
        let restart = self.take_file_restart(id);
        let swapping = stop_running && self.is_running(id);
        if swapping {
            self.start_tracker.cancelled(id);
            if let Some(process) = self.processes.remove(id) {
                self.retire_process(id, process, false);
            }
            self.forget_plugin_runtime(id, "plugin swap restart");
        }
        (swapping, restart)
    }

    /// Report whether this completion actually started a new process, excluding an existing one.
    pub fn finish_file_swap(&mut self, id: &str) -> anyhow::Result<bool> {
        let already_running = self.is_running(id);
        self.swap_respawn_internal(id)?;
        Ok(!already_running && self.is_running(id))
    }

    /// Receipt-confirmed package changes avoid another filesystem scan on the event loop.
    pub fn replace_committed_package(&mut self, id: &str, package: Option<PluginPackage>) {
        self.packages.retain(|p| p.manifest.id != id);
        self.rejected.retain(|p| p.id != id);
        if let Some(package) = package {
            self.packages.push(package);
        }
        self.packages
            .sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        self.sync_ipc_namespaces_from_packages();
        self.recompute_shared_tables();
    }

    /// 설치 목록으로 namespace 소유자를 갱신한다. 실행 여부는 호출 검증에서 따로 확인한다.
    /// 소유자 조회를 위해 플러그인을 실행할 필요가 없고, 비활성 플러그인도
    /// 없는 메서드와 구분할 수 있다.
    /// [CLI + IPC namespace](../../../../docs/dev-guide/plugin-development.md#cli--ipc-namespace) 참고.
    fn sync_ipc_namespaces_from_packages(&mut self) {
        let fresh = self.freshly_computed_namespaces();
        // 계산을 마친 표를 한 번의 잠금으로 교체한다.
        *self.namespaces_write() = fresh;
    }

    /// 설치 목록 전체로 다시 계산해 제거된 패키지의 namespace가 남지 않게 한다.
    fn freshly_computed_namespaces(&self) -> IpcNamespaceRegistry {
        let mut fresh = IpcNamespaceRegistry::new();
        for package in &self.packages {
            let id = &package.manifest.id;
            for ns in &package.manifest.contributes.ipc_namespace {
                if let Err(e) = fresh.register(id, &ns.prefix) {
                    tracing::warn!("plugin '{id}' ipc namespace registration failed: {e}");
                }
            }
        }
        fresh
    }

    /// 디스크 스캔 없이 설치 목록을 교체하고 namespace 소유자를 갱신한다.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_packages_for_tests(&mut self, packages: Vec<crate::PluginPackage>) {
        self.packages = packages;
        self.sync_ipc_namespaces_from_packages();
        self.recompute_shared_tables();
    }

    /// debug 전용 신선도 검사가 낡은 표를 잡는지 확인하기 위해 갱신을 생략한다.
    #[cfg(all(test, debug_assertions))]
    pub(crate) fn overwrite_packages_without_deriving_for_tests(
        &mut self,
        packages: Vec<crate::PluginPackage>,
    ) {
        self.packages = packages;
    }

    /// 설치 목록을 마지막으로 바꾼 뒤 소유자 표도 갱신했는지 debug 빌드에서 확인한다.
    pub fn debug_assert_namespaces_fresh(&self) {
        #[cfg(debug_assertions)]
        {
            let fresh = self.freshly_computed_namespaces();
            assert!(
                fresh == *self.namespaces_read(),
                "namespace 소유 표가 낡았다 — packages의 마지막 변경 뒤 refresh_packages를 호출해야 한다"
            );
        }
    }

    pub fn discover_and_start(&mut self) {
        // 비어 있지 않고 "0"이 아닌 환경 변수 값은 auto-reload를 켠다.
        let enabled = std::env::var("TASTY_PLUGIN_AUTO_RELOAD")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false);
        self.set_auto_reload_enabled(enabled, Instant::now());
        if self.auto_reload_enabled {
            tracing::info!("plugin auto-reload: enabled (TASTY_PLUGIN_AUTO_RELOAD)");
        }
        self.refresh_packages();
        self.register_all_package_commands();
        self.recompute_extensions();

        let to_start: Vec<String> = self
            .packages
            .iter()
            .filter(|p| !self.config.is_disabled(&p.manifest.id))
            .map(|p| p.manifest.id.clone())
            .collect();
        if to_start.is_empty() {
            tracing::info!(
                "plugin: discovered {} package(s), 0 enabled — skipping listener bind",
                self.packages.len()
            );
            return;
        }
        self.ensure_listener();
        for id in &to_start {
            self.start_enabled_package(id);
        }
        // GUI 부팅 워커에서 연결을 먼저 기다려, 이후 hello 대기 시간을 연결에 쓰지 않는다.
        self.wait_for_connections();
    }

    /// command registry에 모든 발견된 plugin의 commands를 등록.
    /// disabled 여부와 무관 — 설정 UI는 비활성 plugin도 단축키 항목을
    /// 보여줘야 사용자가 미리 키를 잡아둘 수 있다.
    fn register_all_package_commands(&mut self) {
        self.command_registry = crate::command_registry::PluginCommandRegistry::new();
        for pkg in &self.packages {
            self.command_registry.register_plugin(&pkg.manifest);
            // i18n namespace 등록 — 비활성 plugin도 설정 UI에서 command title을
            // 번역해서 보여줘야 하므로 disabled 여부와 무관하게 등록한다.
            self.register_package_i18n(pkg);
        }
    }

    /// 패키지의 언어 디렉터리를 그 id namespace로 등록한다. registrar가 없으면 생략한다.
    fn register_package_i18n(&self, pkg: &PluginPackage) {
        if let Some(reg) = &self.i18n_registrar {
            reg.register(&pkg.manifest.id, &pkg.dir.join(&pkg.manifest.lang_dir));
        }
    }

    /// 설정을 바꾸지 않고 설치·활성 상태인 플러그인 하나만 실행한다. 실행했으면 true.
    /// 요청 처리가 disable을 되돌리거나 관계없는 플러그인까지 실행하지 않도록
    /// 활성 상태를 저장하는 enable 및 전체 기동과 구분한다.
    pub fn start_one_enabled(&mut self, plugin_id: &str) -> bool {
        if self.config.is_disabled(plugin_id) || self.is_auto_disabled(plugin_id) {
            return false;
        }
        if self.processes.contains_key(plugin_id) {
            return false;
        }
        let Some(pkg) = self.packages.iter().find(|p| p.manifest.id == plugin_id) else {
            return false;
        };
        // 전체 기동을 거치지 않아도 이 플러그인 kind의 탭 이름이 정해지기 전에 번역을 준비한다.
        self.register_package_i18n(pkg);
        self.ensure_listener();
        self.start_enabled_package(plugin_id);
        self.processes.contains_key(plugin_id)
    }

    /// 설치된 패키지의 정적 기능을 등록하고 실행한다. 이미 실행 중이면 그대로 둔다.
    fn start_enabled_package(&mut self, id: &str) {
        if self.processes.contains_key(id) {
            return;
        }
        let Some(pkg) = self.packages.iter().find(|p| p.manifest.id == id).cloned() else {
            return;
        };
        // 정적 기능은 프로세스 실행 성공 여부와 관계없이 활성화한다.
        self.file_format
            .install_plugin_detectors(&pkg.manifest.id, &pkg.manifest.contributes.detector);
        self.file_handler
            .install_plugin_handlers(&pkg.manifest.id, &pkg.manifest.contributes.handler);
        if let Some(hh) = &self.hook_handler {
            hh.install_plugin_hook_handlers(
                &pkg.manifest.id,
                &pkg.manifest.contributes.hook_handler,
            );
        }
        if let Some(cs) = &self.completion_strategy {
            cs.install_plugin_completion_strategies(
                completion_strategy_owner_id(&pkg),
                &pkg.manifest.contributes.completion_strategy,
            );
        }
        self.start_plugin_internal(&pkg);
    }

    pub(super) fn ensure_listener(&mut self) {
        self.ensure_tcp_listener();
        self.ensure_handle_listener();
    }

    fn ensure_tcp_listener(&mut self) {
        if self.listener.is_some() {
            return;
        }
        match HostListener::bind() {
            Ok(l) => {
                tracing::info!("plugin host listener on 127.0.0.1:{}", l.port());
                self.listener = Some(l);
            }
            Err(e) => {
                tracing::error!("plugin host listener bind failed: {e}");
            }
        }
    }

    fn ensure_handle_listener(&mut self) {
        if self.handle_listener.is_some() {
            return;
        }
        match HandleListener::bind() {
            Ok(l) => {
                tracing::info!("plugin handle channel listener at {}", l.endpoint());
                self.handle_listener = Some(l);
            }
            Err(e) => {
                // 일반 IPC는 쓸 수 있지만 보조 채널이 필요한 shared buffer 전달은 실패한다.
                tracing::warn!("plugin handle channel listener bind failed: {e}");
            }
        }
    }

    pub(super) fn start_plugin_internal(&mut self, pkg: &PluginPackage) {
        if self.auto_disabled.contains(&pkg.manifest.id) {
            return;
        }
        // 회수를 기다리는 기동은 같은 세대로 나중에 띄운다.
        let generation = self.start_tracker.begin(&pkg.manifest.id);
        // 이전 프로세스의 회수가 끝난 뒤에만 다시 실행한다(manager::retire).
        if self.defer_start_until_retired(&pkg.manifest.id) {
            return;
        }
        let listener = match &self.listener {
            Some(l) => l,
            None => {
                tracing::warn!("plugin '{}' start skipped — no listener", pkg.manifest.id);
                self.start_tracker.failed(
                    &pkg.manifest.id,
                    super::FailureKind::SpawnFailed,
                    "the plugin host listener is not available".to_string(),
                    None,
                    Instant::now(),
                );
                return;
            }
        };
        match PluginProcess::spawn(
            pkg,
            listener,
            self.handle_listener.as_ref(),
            &self.log_dir,
            self.waker.clone(),
            &self.plugin_reaper,
            &self.channel_ledger,
        ) {
            Ok(p) => {
                self.start_tracker.spawned(&pkg.manifest.id, generation);
                self.on_plugin_spawn_success(pkg, p)
            }
            Err(e) => {
                self.on_plugin_spawn_failure(&pkg.manifest.id, super::FailureKind::SpawnFailed, e)
            }
        }
    }

    fn on_plugin_spawn_success(&mut self, pkg: &PluginPackage, p: PluginProcess) {
        tracing::info!("plugin started: {}", p.plugin_id);
        self.processes.insert(pkg.manifest.id.clone(), p);
        // 이전 프로세스가 회수된 뒤 남은 surface 를 새 프로세스에 다시 게시한다.
        self.reattach_orphan_surfaces(&pkg.manifest.id);
        // 연결 성공 전까지는 연속 기동 실패 기록을 유지한다(manager::connect).
        // 실행 직후의 파일 상태를 기록해 같은 변경으로 auto-reload를 반복하지 않게 한다.
        self.capture_plugin_baseline(&pkg.manifest.id);
        // plugin.loaded는 hello 이후 호스트가 알린다. namespace 소유자는 설치 스캔에서 등록한다.
    }

    pub(super) fn on_plugin_spawn_failure(
        &mut self,
        plugin_id: &str,
        kind: super::FailureKind,
        e: anyhow::Error,
    ) {
        tracing::error!("plugin '{}' spawn failed: {}", plugin_id, e);
        {
            use tasty_plugin_protocol::EventScope;
            use tasty_plugin_protocol::events::payloads::PluginError;
            let payload = PluginError {
                plugin_id: plugin_id.to_string(),
                error_kind: "spawn_failed".to_string(),
                message: e.to_string(),
            };
            self.emit_host_event("plugin.error", &payload, EventScope::System);
        }
        let auto_disabled = self.record_spawn_failure(plugin_id);
        self.start_tracker.failed(
            plugin_id,
            kind,
            e.to_string(),
            auto_disabled,
            Instant::now(),
        );
    }

    /// 실패를 세고, 이 실패로 자동 비활성화했으면 누적 실패 수를 돌려준다.
    fn record_spawn_failure(&mut self, plugin_id: &str) -> Option<usize> {
        let now = Instant::now();
        let entry = self
            .spawn_failures
            .entry(plugin_id.to_string())
            .or_default();
        entry.retain(|t| now.duration_since(*t) < RESTART_FAILURE_WINDOW);
        entry.push(now);
        if entry.len() >= RESTART_FAILURE_LIMIT {
            tracing::error!(
                "plugin '{plugin_id}' failed {} times in {}s — auto-disabling until manual re-enable",
                entry.len(),
                RESTART_FAILURE_WINDOW.as_secs()
            );
            let failures = entry.len();
            self.auto_disabled.insert(plugin_id.to_string());
            self.spawn_failures.remove(plugin_id);
            return Some(failures);
        }
        None
    }

    /// 모든 플러그인에 종료를 요청한 뒤 공유 deadline으로 대기하고 자식을 회수한다.
    /// 정상 종료 대기는 겹치지만 kill과 회수까지 포함한 총시간 상한은 아니다.
    /// 프레임을 계속 처리해야 하면 begin_shutdown_all과 poll_shutdown_all을 쓴다.
    /// tasty::shutdown의 S4a는 개별 회수, S4는 전체 완료를 기록한다.
    /// 로그 표지는 docs/architecture/shutdown-sequence.md에 정리돼 있다.
    pub fn shutdown_all(&mut self) {
        self.begin_shutdown_all();
        while !self.poll_shutdown_all() {
            std::thread::sleep(CHILD_EXIT_POLL_INTERVAL);
        }
    }

    /// 모든 플러그인에 종료를 요청하고 기다리지 않고 반환한다. 이미 진행 중이면 그대로 둔다.
    /// 각 요청은 같은 채널의 surface.closed 뒤에 놓인다.
    /// 반환 후 poll_shutdown_all이 true가 될 때까지 호출해야 자식을 회수할 수 있다.
    pub fn begin_shutdown_all(&mut self) {
        if self.shutdown_batch.is_some() {
            return;
        }
        let deadline = Instant::now() + PLUGIN_SHUTDOWN_TIMEOUT;
        let pending: Vec<_> = self
            .processes
            .drain()
            .map(|(_, proc)| proc.begin_shutdown(deadline))
            .collect();
        self.shutdown_batch = Some(ShutdownBatch::new(pending));
    }

    /// 전체 종료와 별도로 진행 중인 회수를 폴링한다. 모두 끝나면 true.
    /// 개별 완료는 S4a, 전체 종료 배치의 완료는 S4로 기록한다.
    pub fn poll_shutdown_all(&mut self) -> bool {
        // 단건 경로로 이미 내려가던 plugin 도 끝날 때까지 본다 — 다시 띄우지 않는다.
        let retired = self.poll_retiring_for_exit();
        let Some(batch) = self.shutdown_batch.as_mut() else {
            return retired;
        };
        for report in batch.poll() {
            tracing::info!(
                target: "tasty::shutdown",
                ms = report.elapsed.as_secs_f64() * 1000.0,
                plugin_id = report.plugin_id,
                reason = report.outcome.as_str(),
                "S4a plugin_shutdown_one (graceful deadline 2s)"
            );
        }
        if !batch.is_done() || !retired {
            return false;
        }
        let ms = batch.elapsed().as_secs_f64() * 1000.0;
        let plugins = batch.total();
        self.shutdown_batch = None;
        self.plugin_buffers.clear();
        tracing::info!(
            target: "tasty::shutdown",
            ms,
            plugins,
            "S4 plugin_shutdown (병렬 대기 합계)"
        );
        true
    }

    /// CLI/IPC용 — plugin 활성화. 활성화 즉시 spawn 시도.
    pub fn enable(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        if !self.packages.iter().any(|p| p.manifest.id == plugin_id) {
            anyhow::bail!("plugin '{plugin_id}' not installed");
        }
        let mut config = self.config.clone();
        config.enable(plugin_id);
        self.config = crate::file_persistence::ConfigUpdate::between(&self.config, &config)?
            .with_enabled_intent(plugin_id, true)
            .execute()?;
        if self.wait_retired(plugin_id) {
            tracing::debug!(plugin_id, "enable took over a pending restart");
        }
        self.enable_committed(plugin_id)
    }

    /// Apply a confirmed configuration without doing file I/O on the lifecycle thread.
    pub fn enable_committed(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        // 설치 목록을 확인하기 전에는 설정·수명주기 상태를 바꾸지 않는다.
        let pkg = self
            .packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("plugin '{plugin_id}' not installed"))?;
        if self.config.is_disabled(plugin_id) {
            anyhow::bail!("plugin configuration is still disabled");
        }
        if self.is_retiring(plugin_id) {
            anyhow::bail!("plugin generation is still retiring");
        }
        self.auto_disabled.remove(plugin_id);
        self.start_tracker.reset_reporting(plugin_id);
        self.recompute_extensions();
        // file_format / file_handler 두 registry 에 plugin 의 contribute 등록.
        // plugin process spawn 과 별개로 정적 contribute 는 즉시 활성화한다.
        self.file_format
            .install_plugin_detectors(plugin_id, &pkg.manifest.contributes.detector);
        self.file_handler
            .install_plugin_handlers(plugin_id, &pkg.manifest.contributes.handler);
        if let Some(hh) = &self.hook_handler {
            hh.install_plugin_hook_handlers(plugin_id, &pkg.manifest.contributes.hook_handler);
        }
        if let Some(cs) = &self.completion_strategy {
            cs.install_plugin_completion_strategies(
                completion_strategy_owner_id(&pkg),
                &pkg.manifest.contributes.completion_strategy,
            );
        }

        if !self.processes.contains_key(plugin_id) {
            self.ensure_listener();
            self.start_plugin_internal(&pkg);
        }
        // plugin.enabled는 호스트의 CoreEvent 처리에서 알린다.
        Ok(())
    }

    /// CLI/IPC용 — plugin 비활성화. 살아있는 process는 graceful shutdown.
    pub fn disable(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        if !self.packages.iter().any(|p| p.manifest.id == plugin_id) {
            anyhow::bail!("plugin '{plugin_id}' not installed");
        }
        let mut config = self.config.clone();
        config.disable(plugin_id);
        self.config = crate::file_persistence::ConfigUpdate::between(&self.config, &config)?
            .with_enabled_intent(plugin_id, false)
            .execute()?;
        self.disable_committed(plugin_id)
    }

    /// Retire only after the configuration receipt succeeds; no file I/O occurs here.
    pub fn disable_committed(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        // enable 과 같은 설치 경계. 실패한 요청은 disabled 흔적도 남기지 않는다.
        let pkg = self
            .packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .ok_or_else(|| anyhow::anyhow!("plugin '{plugin_id}' not installed"))?;
        let cs_owner_id = completion_strategy_owner_id(pkg).to_string();
        if !self.config.is_disabled(plugin_id) {
            anyhow::bail!("plugin configuration is still enabled");
        }
        self.recompute_extensions();
        let was_running = self.processes.contains_key(plugin_id);
        // 결과가 나기 전의 기동은 취소로 끝낸다.
        self.start_tracker.cancelled(plugin_id);
        // 회수는 별도 스레드에서 진행하며 기존 재시작 예약은 취소한다.
        if let Some(proc) = self.processes.remove(plugin_id) {
            self.retire_process(plugin_id, proc, false);
        } else {
            self.cancel_respawn_after_retire(plugin_id);
        }
        // namespace 소유자는 설치 정보이므로 유지한다. 호출 시 실행 중이 아님을 알릴 수 있다.
        // 완료 전략의 owner는 등록할 때와 같은 접두어로 계산해야 해제할 수 있다.
        self.file_format.uninstall_plugin(plugin_id);
        self.file_handler.uninstall_plugin(plugin_id);
        if let Some(hh) = &self.hook_handler {
            hh.uninstall_plugin(plugin_id);
        }
        if let Some(cs) = &self.completion_strategy {
            cs.uninstall_plugin(&cs_owner_id);
        }
        // 상태 알림은 App::plugin_disable에서 처리하며, 실행 중이었는지도 거기서 확인한다.
        let _ = was_running; // 의도적으로 무시 — 상태 알림은 호스트에서 처리한다.
        self.forget_plugin_runtime(plugin_id, "plugin disabled");
        Ok(())
    }

    /// 프로세스를 정리한 뒤 요청·버퍼·권한·설정 페이지 등 실행 상태를 지운다.
    /// namespace 소유권은 설치 정보이므로 유지한다. 정적 기능과 mesh 프레임은 호출자가 정리한다.
    pub(super) fn forget_plugin_runtime(&mut self, plugin_id: &str, reason: &str) {
        self.event_bus.clear_plugin(plugin_id);
        // namespace 호출·hook 은 caller 에 회신하고, 그 plugin 에게 보낸 나머지 요청도
        // 거둔다(`reclaim_requests_sent_to`).
        self.cancel_pending_namespace_calls(plugin_id, reason);
        self.plugin_buffers.remove(plugin_id);
        self.forget_copied_texts(plugin_id);
        self.settings_pages.unregister_plugin(plugin_id);
        // 다음 hello에서 권한과 설정 페이지 등을 다시 등록할 수 있게 한다.
        self.registered_plugins.remove(plugin_id);
    }

    pub fn is_running(&self, plugin_id: &str) -> bool {
        self.processes.contains_key(plugin_id)
    }

    /// 아직 응답이 없는 surface.restore 요청이 있는지 확인한다.
    /// 부팅 대기에서 복원 완료 여부를 확인하는 데 쓴다.
    pub fn has_pending_surface_restores(&self) -> bool {
        self.pending_requests
            .values()
            .any(|p| matches!(p.kind, super::PendingRequestKind::SurfaceRestore { .. }))
    }

    /// 활성 설정을 바꾸지 않고 교체할 프로세스를 종료한다.
    /// 종료 뒤 파일을 덮어쓰고 swap_respawn_internal로 다시 실행한다.
    pub(crate) fn swap_shutdown_internal(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        // 파일을 덮어쓰기 전에 이전 프로세스의 회수가 끝나야 한다.
        // 기존 재시작 예약은 뒤의 swap_respawn_internal로 대신한다.
        if self.wait_retired(plugin_id) {
            tracing::debug!(
                plugin_id,
                "swap took over a pending restart — swap_respawn_internal starts it"
            );
        }
        self.start_tracker.cancelled(plugin_id);
        if let Some(proc) = self.processes.remove(plugin_id) {
            let generation = proc.reply_binding();
            proc.shutdown(PLUGIN_SHUTDOWN_TIMEOUT);
            self.note_reaped_generation(generation);
        }
        // ipc namespace 유지 — swap 중에 오는 호출은 "없는 메서드" 가 아니라
        // "지금 안 뜬 plugin" 이다(docs/dev-guide/plugin-development.md#cli--ipc-namespace).
        self.forget_plugin_runtime(plugin_id, "plugin swap restart");
        Ok(())
    }

    /// 활성 설정을 바꾸지 않고 교체된 바이너리로 다시 실행한다.
    pub(crate) fn swap_respawn_internal(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        self.auto_disabled.remove(plugin_id);
        let pkg = self
            .packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("plugin '{plugin_id}' not in packages"))?;
        if self.processes.contains_key(plugin_id) {
            return Ok(());
        }
        self.ensure_listener();
        self.start_plugin_internal(&pkg);
        if !self.processes.contains_key(plugin_id) {
            anyhow::bail!("plugin '{plugin_id}' respawn failed (spawn error logged)");
        }
        Ok(())
    }

    /// 플러그인 로그 파일 경로.
    pub fn log_path(&self, plugin_id: &str) -> PathBuf {
        self.log_dir.join(format!("{plugin_id}.log"))
    }

    /// auto-reload 한 건을 처리한다. 종료 성공 뒤 기준 파일 상태를 갱신한다.
    /// 재실행에 실패해도 같은 변경으로 교체를 반복하지 않는다.
    pub(super) fn auto_reload_one(&mut self, plugin_id: &str) -> anyhow::Result<()> {
        tracing::info!("auto-reload: {plugin_id} swap start");
        self.swap_shutdown_internal(plugin_id)?;
        let respawn = self.swap_respawn_internal(plugin_id);
        // shutdown 이 성공한 시점에서 baseline 을 갱신해야 다음 polling tick 에서
        // 같은 mtime/version 으로 또 swap 시도하지 않는다 (respawn 결과와 무관).
        self.capture_plugin_baseline(plugin_id);
        respawn?;
        tracing::info!("auto-reload: {plugin_id} swap done");
        Ok(())
    }

    /// auto-reload가 켜져 있으면 실행 중인 플러그인의 mtime·버전 변경을 찾는다.
    /// 파일 정보를 읽지 못하면 mtime 비교만 생략한다.
    pub(super) fn check_for_updates(&self) -> Vec<String> {
        if !self.auto_reload_enabled {
            return Vec::new();
        }
        let mut updated = Vec::new();
        for plugin_id in self.processes.keys() {
            let pkg = match self.packages.iter().find(|p| &p.manifest.id == plugin_id) {
                Some(p) => p,
                None => continue,
            };
            let bin = pkg.entry_command_path();
            let new_mtime = std::fs::metadata(&bin).ok().and_then(|m| m.modified().ok());
            let old_mtime = self.plugin_binary_mtimes.get(plugin_id).copied();
            let binary_changed = matches!((new_mtime, old_mtime), (Some(n), Some(o)) if n != o);

            let new_version = pkg.manifest.version.as_str();
            let version_changed = self
                .plugin_manifest_versions
                .get(plugin_id)
                .is_some_and(|old| old != new_version);

            if binary_changed || version_changed {
                updated.push(plugin_id.clone());
            }
        }
        updated
    }

    /// mtime과 manifest 버전을 기록한다. 파일 정보를 읽지 못해도 버전은 기록한다.
    pub(super) fn capture_plugin_baseline(&mut self, plugin_id: &str) {
        let pkg = match self.packages.iter().find(|p| p.manifest.id == plugin_id) {
            Some(p) => p,
            None => return,
        };
        let bin = pkg.entry_command_path();
        match std::fs::metadata(&bin).and_then(|m| m.modified()) {
            Ok(mtime) => {
                self.plugin_binary_mtimes
                    .insert(plugin_id.to_string(), mtime);
            }
            Err(e) => {
                tracing::debug!(
                    "plugin '{plugin_id}' baseline mtime skip ({}): {e}",
                    bin.display()
                );
                self.plugin_binary_mtimes.remove(plugin_id);
            }
        }
        self.plugin_manifest_versions
            .insert(plugin_id.to_string(), pkg.manifest.version.clone());
    }
}
