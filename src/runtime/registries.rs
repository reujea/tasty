//! Process-owned registry instances. Engines and PluginManager receive the same shared handles.
pub(crate) mod file_rules;
use std::{path::Path, sync::Arc};
#[derive(Clone)]
pub(crate) struct RuntimeRegistries {
    pub(crate) child_terminals: Arc<super::child_terminal::Hub>,
    pub(crate) file_rules: Option<Arc<file_rules::FileRules>>,
    pub(crate) surface_registry: Arc<super::surface_registry::SurfaceKindRegistry>,
    pub(crate) file_format: Arc<crate::file::format::FileFormatRegistry>,
    pub(crate) file_handler: Arc<crate::file::handler::FileHandlerRegistry>,
    pub(crate) plugin_hook_events: Arc<crate::core::hook_event_registry::PluginHookEventRegistry>,
    #[cfg(feature = "gui")]
    pub(crate) explorer_favorites: Arc<
        crate::core::shared_list::SharedList<crate::core::explorer_favorites::ExplorerFavorites>,
    >,
    #[cfg(feature = "gui")]
    pub(crate) port_favorites:
        Arc<crate::core::shared_list::SharedList<crate::core::port_favorites::PortFavorites>>,
    /// 서버가 강제로 끊어 자동으로 다시 붙지 않는 저장된 원격 행의 연결 대상.
    #[cfg(feature = "gui")]
    pub(crate) remote_force_detached: Arc<
        crate::core::shared_list::SharedList<crate::core::remote_force_detached::ForceDetachedRows>,
    >,
}
impl RuntimeRegistries {
    pub(crate) fn new(user_config: Option<&Path>) -> Self {
        let surface_registry = super::surface_registry::SurfaceKindRegistry::new();
        super::surface_registry::register_builtin_kinds(&surface_registry);
        let file_format = Arc::new(crate::file::format::FileFormatRegistry::new());
        file_format.install_host_defaults(crate::file::format::HOST_DEFAULTS_TOML);
        let file_handler = Arc::new(crate::file::handler::FileHandlerRegistry::new());
        file_handler.install_host_defaults(crate::file::handler::HOST_DEFAULTS_TOML);
        let file_rules = user_config.map(|path| {
            Arc::new(file_rules::FileRules::load(
                path,
                &file_format,
                &file_handler,
            ))
        });
        file_handler.attach_detector_info(file_format.clone());
        Self {
            child_terminals: super::child_terminal::Hub::new(
                tasty_utils::path::tasty_home().map(|p| p.join("child-terminals.json")),
            ),
            file_rules,
            surface_registry: Arc::new(surface_registry),
            file_format,
            file_handler,
            plugin_hook_events: Arc::new(
                crate::core::hook_event_registry::PluginHookEventRegistry::new(),
            ),
            #[cfg(feature = "gui")]
            explorer_favorites: Arc::new(crate::core::shared_list::SharedList::load()),
            #[cfg(feature = "gui")]
            port_favorites: Arc::new(crate::core::shared_list::SharedList::load()),
            #[cfg(feature = "gui")]
            remote_force_detached: Arc::new(crate::core::shared_list::SharedList::load()),
        }
    }
}
