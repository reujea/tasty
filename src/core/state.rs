use crate::runtime::engine_access::{EngineMut, EngineRef};
#[cfg(test)]
use tasty_terminal::Waker;

use crate::settings::Settings;
pub(crate) use message::SurfaceMessage;

pub struct ShellConfig {
    pub shell: String,
    pub args: Vec<String>,
    /// 셸 초기화에 필요한 추가 환경변수. bash의 rcfile 설정은 args로 전달한다.
    pub envs: Vec<(String, String)>,
}

impl ShellConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        let shell = settings.general.runtime_shell();
        Self {
            args: settings.general.shell_args_for(&shell),
            envs: settings.general.shell_envs_for(&shell),
            shell,
        }
    }

    pub fn shell_ref(&self) -> Option<&str> {
        if self.shell.is_empty() {
            None
        } else {
            Some(&self.shell)
        }
    }

    pub fn args_ref(&self) -> Vec<&str> {
        self.args.iter().map(|s| s.as_str()).collect()
    }

    pub fn envs_ref(&self) -> Vec<(&str, &str)> {
        self.envs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

/// 붙여넣기 시점의 mirror 대상을 고정하고 백그라운드 업로드 뒤 그 surface에 원격 경로를 입력한다.
#[cfg(feature = "gui")]
#[derive(Clone, Debug)]
pub(crate) struct PendingImageUpload {
    /// Original View captured when the user enqueues the paste, preserved through retry.
    pub(crate) origin_view: std::sync::Weak<()>,
    /// attach 세션을 찾을 로컬 mirror workspace ID.
    pub(crate) mirror_ws_id: u32,
    /// 붙여넣기 시점에 정한 로컬 mirror surface ID.
    pub(crate) surface_id: u32,
    /// 붙여넣기 시점의 bracketed paste 설정.
    pub(crate) bracketed: bool,
    pub(crate) file_name: String,
    pub(crate) png_bytes: Vec<u8>,
}

/// MeshContext의 내용. 로컬 surface ID는 이 요청을 담는 큐의 키다.
#[derive(Debug, Clone)]
#[cfg(feature = "gui")]
pub(crate) struct AttachMeshContextForward {
    pub(crate) width_px: u32,
    pub(crate) height_px: u32,
    pub(crate) pixels_per_point: f32,
    pub(crate) theme: Option<tasty_plugin_protocol::protocol::ThemeWire>,
    pub(crate) focused: bool,
}

impl EngineMut<'_> {
    pub fn is_typing(&self, surface_id: u32) -> bool {
        if let Some(last) = self.live.last_key_input.get(&surface_id) {
            last.elapsed().as_secs_f64() < 5.0
        } else {
            false
        }
    }
}

impl EngineRef<'_> {
    /// 없는 키에만 기본값을 넣고 하나라도 넣으면 true다. params가 객체가 아니면 변경하지 않는다.
    /// @settings.explorer_view_mode와 전달된 @home을 해석하고 알 수 없는 @ 토큰은 경고 후 건너뛴다.
    pub(crate) fn apply_kind_default_params(
        &self,
        def: &crate::runtime::surface_registry::SurfaceKindDef,
        params: &mut serde_json::Value,
        home: Option<&std::path::Path>,
    ) -> bool {
        if def.default_params.is_empty() {
            return false;
        }
        let Some(obj) = params.as_object_mut() else {
            return false;
        };
        let mut injected = false;
        for (key, token) in &def.default_params {
            if obj.contains_key(key.as_str()) {
                continue;
            }
            let Some(val) = self.resolve_default_param_token(token, home) else {
                continue;
            };
            obj.insert(key.clone(), serde_json::Value::String(val));
            injected = true;
        }
        injected
    }

    fn resolve_default_param_token(
        &self,
        token: &str,
        home: Option<&std::path::Path>,
    ) -> Option<String> {
        match token {
            "@settings.explorer_view_mode" => {
                Some(self.runtime.settings.general.explorer_view_mode.clone())
            }
            "@home" => home.map(|p| p.to_string_lossy().to_string()),
            t if t.starts_with('@') => {
                tracing::warn!("unknown default_param policy token: {t}");
                None
            }
            literal => Some(literal.to_string()),
        }
    }
}

pub(crate) mod attention;
pub mod child_liveness;
mod command_cycle;
mod finders;
mod message;
mod shell_integration_hint;
mod soft_occupancy;
mod surface_cwd;
mod surface_foreground;

pub(crate) use attention::AttentionKind;
pub(crate) use surface_cwd::RemoteCwd;
pub(crate) use surface_cwd::SurfaceCwd;
pub(crate) use surface_foreground::MirrorForegrounds;
#[cfg(feature = "gui")]
pub use tasty_core::SurfaceDisplayPath;

impl EngineMut<'_> {
    #[cfg(feature = "gui")]
    pub fn resync_terminal_palettes(&mut self) {
        self.runtime.terminals.resync_palettes();
    }

    pub fn refresh_tab_display_name(&mut self, surface: u32) {
        if !self.core.has_surface(surface) {
            return;
        }
        let cwd = self.runtime.terminals.cwd(surface);
        let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
        let name = cwd.as_deref().and_then(|cwd| {
            if Some(cwd) == home.as_deref() {
                Some("~".into())
            } else if cwd == std::path::Path::new("/") {
                Some("/".into())
            } else {
                cwd.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            }
        });
        self.live
            .surface_titles
            .entry(surface)
            .or_default()
            .cwd_name = name;
    }

    #[cfg(feature = "gui")]
    pub fn refresh_tab_osc_title(&mut self, surface: u32) {
        if !self.core.has_surface(surface) {
            return;
        }
        self.live
            .surface_titles
            .entry(surface)
            .or_default()
            .osc_title = self
            .runtime
            .terminals
            .get(surface)
            .and_then(|terminal| terminal.current_title());
    }
}

#[cfg(test)]
mod runtime_counter_tests {
    use crate::runtime::counters::RuntimeCounters;

    #[test]
    fn two_engines_do_not_hand_out_the_same_hook_id() {
        use tasty_hooks::{HookBinding, HookEvent, HookManager};
        let ids = RuntimeCounters::new();
        let mut a = HookManager::with_counter(ids.hook_counter());
        let mut b = HookManager::with_counter(ids.hook_counter());
        let ia = a.add_hook(
            1,
            HookEvent::CommandCompleted(None),
            HookBinding::InlineShell("echo a".into()),
            false,
        );
        let ib = b.add_hook(
            1,
            HookEvent::CommandCompleted(None),
            HookBinding::InlineShell("echo b".into()),
            false,
        );
        assert_ne!(ia, ib, "공유 발급기를 쓰는 두 engine의 hook ID가 겹쳤다");
    }

    #[test]
    fn two_engines_do_not_hand_out_the_same_global_hook_id() {
        use crate::hook_runtime::global::{GlobalHookManager, HookCondition};
        let ids = RuntimeCounters::new();
        let mut a = GlobalHookManager::with_counter(ids.global_hook_counter());
        let mut b = GlobalHookManager::with_counter(ids.global_hook_counter());
        let ia = a.add(
            HookCondition::Interval(std::time::Duration::from_secs(60)),
            "echo a".into(),
            None,
        );
        let ib = b.add(
            HookCondition::Interval(std::time::Duration::from_secs(60)),
            "echo b".into(),
            None,
        );
        assert_ne!(
            ia, ib,
            "공유 발급기를 쓰는 두 engine의 global hook ID가 겹쳤다"
        );
    }
}

#[cfg(test)]
mod default_params_tests {

    fn engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    #[test]
    fn explorer_defaults_without_home_inject_view_mode_only() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let def = e.runtime.surface_registry.get("explorer").unwrap();
        let mut params = serde_json::json!({});
        let injected = e
            .as_ref()
            .apply_kind_default_params(&def, &mut params, None);
        assert!(injected);
        assert_eq!(
            params["view_mode"],
            e.runtime.settings.general.explorer_view_mode
        );
        assert!(
            params.get("path").is_none(),
            "@home must not resolve when home=None"
        );
    }

    #[test]
    fn explorer_defaults_with_home_inject_path() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let def = e.runtime.surface_registry.get("explorer").unwrap();
        let mut params = serde_json::json!({});
        let home = std::path::PathBuf::from("/home/tester");
        e.as_ref()
            .apply_kind_default_params(&def, &mut params, Some(&home));
        assert_eq!(
            params["view_mode"],
            e.runtime.settings.general.explorer_view_mode
        );
        assert_eq!(params["path"], "/home/tester");
    }

    #[test]
    fn explicit_params_preserved() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let def = e.runtime.surface_registry.get("explorer").unwrap();
        let home = std::path::PathBuf::from("/home/tester");
        let mut params = serde_json::json!({"view_mode": "list", "path": "/explicit"});
        e.as_ref()
            .apply_kind_default_params(&def, &mut params, Some(&home));
        assert_eq!(params["view_mode"], "list");
        assert_eq!(params["path"], "/explicit");
    }

    #[test]
    fn kind_without_defaults_is_noop() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let def = e.runtime.surface_registry.get("terminal").unwrap();
        let mut params = serde_json::json!({});
        assert!(
            !e.as_ref()
                .apply_kind_default_params(&def, &mut params, None)
        );
    }
}

/// 세션 초기화는 실행을 시작하지 않으며 전달된 runner owner를 유지한다.
#[cfg(test)]
mod engine_creation_failure_tests {
    use super::*;

    fn bogus_shell_settings() -> Settings {
        let mut s = Settings::default();
        s.general.shell = "/nonexistent/definitely/not/a/real/shell-xyzzy".to_string();
        // 설정값을 받는 것만으로 셸을 실행하지 않아야 한다.
        s.general.restore_layout = false;
        s
    }

    fn registry() -> std::sync::Arc<tasty_task_runtime::RunnerRegistry> {
        std::sync::Arc::new(tasty_task_runtime::RunnerRegistry::new())
    }

    fn in_memory() -> std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> {
        std::sync::Arc::new(std::sync::Mutex::new(
            tasty_memory::MemoryStore::open_in_memory().expect("in-memory store"),
        ))
    }

    #[test]
    fn engine_initialization_does_not_execute_the_configured_shell() {
        let owner = crate::runtime::engine_session::EngineSession::new_with_ids_and_settings(
            crate::runtime::engine_session::EngineSessionSpec {
                cols: 80,
                rows: 24,
                waker: std::sync::Arc::new(|| {}),
                shared_ids: None,
                layout_slot: None,
                memory: tasty_memory::writer::MemoryAccess::inline(in_memory()),
                runner_registry: registry(),
            },
            bogus_shell_settings(),
        )
        .expect("session initialization precedes shell execution");
        assert!(owner.core_state.workspaces().is_empty());
        assert_eq!(owner.runtime.terminals.iter().count(), 0);
    }

    #[test]
    fn the_engine_task_scope_holds_the_runner_registry_it_was_built_with() {
        let waker: Waker = std::sync::Arc::new(|| {});
        let mut settings = Settings::default();
        settings.general.restore_layout = false;
        let shared = registry();
        let mut engine_session =
            crate::runtime::engine_session::EngineSession::new_with_ids_and_settings(
                crate::runtime::engine_session::EngineSessionSpec {
                    cols: 80,
                    rows: 24,
                    waker,
                    shared_ids: None,
                    layout_slot: None,
                    memory: tasty_memory::writer::MemoryAccess::inline(in_memory()),
                    runner_registry: std::sync::Arc::clone(&shared),
                },
                settings,
            )
            .expect("default settings must produce an engine");
        let engine = engine_session.borrow_mut();
        assert!(std::sync::Arc::ptr_eq(
            engine.task_scope.runner_registry(),
            &shared
        ));
    }
}

pub(crate) use finders::mirror_workspace_index_for_structural;
