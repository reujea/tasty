//! Non-journal execution requests carry targets observed by a View, never a mutable Engine borrow.
//! 여기의 일반 명령은 일반 dispatch만 실행한다. 프레임 단계 효과는 `super::frame_effect`에 있다.
use crate::runtime::engine_access::EngineMut;
use crate::runtime::surface_binding::SurfaceBinding;

impl SurfaceBinding {
    #[cfg(feature = "gui")]
    pub(crate) fn take_mouse_capture_hint(
        &self,
        engine: &mut EngineMut<'_>,
        foreground_generation: u64,
    ) -> Option<u32> {
        if !self.current(&engine.as_ref())
            || engine.foreground_generation(self.surface_id()) != foreground_generation
            || !engine.runtime.settings.general.mouse_capture_hint
            || engine
                .foreground_name(self.surface_id())
                .is_some_and(|name| {
                    engine
                        .runtime
                        .settings
                        .general
                        .mouse_capture_banner_disabled_for(name)
                })
        {
            return None;
        }
        engine
            .runtime
            .terminals
            .get_mut(self.surface_id())?
            .take_mouse_capture_hint()
            .then_some(self.surface_id())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ZoomChange {
    In,
    Out,
    Reset,
}

#[derive(Clone, Debug)]
pub(crate) enum SettingsPatch {
    /// webview kind 설정 소유 plugin의 zoom. 저장값이 없을 때와 초기화할 때 default_zoom을 쓴다.
    PluginZoom {
        plugin: String,
        default_zoom: f64,
        change: ZoomChange,
    },
    FontSize {
        kind: Option<String>,
        change: ZoomChange,
    },
    ExplorerMode(String),
    SuppressMouseHint(String),
    DisableMouseCapture(String),
}
impl SettingsPatch {
    pub(crate) fn apply(&self, settings: &mut crate::settings::Settings) {
        match self {
            Self::PluginZoom {
                plugin,
                default_zoom,
                change,
            } => {
                use crate::settings::PluginSettingValue;
                use tasty_plugin_manifest::{WEBVIEW_ZOOM_MAX, WEBVIEW_ZOOM_MIN};
                let current = match settings.plugin_setting(plugin, "zoom") {
                    Some(PluginSettingValue::Number(value)) => *value,
                    _ => *default_zoom,
                };
                let value = match change {
                    ZoomChange::In => (current + 10.0).min(WEBVIEW_ZOOM_MAX),
                    ZoomChange::Out => (current - 10.0).max(WEBVIEW_ZOOM_MIN),
                    ZoomChange::Reset => *default_zoom,
                };
                settings.set_plugin_setting(plugin, "zoom", PluginSettingValue::Number(value));
            }
            Self::FontSize { kind, change } => {
                let appearance = &mut settings.appearance;
                let current = match kind {
                    Some(kind) => appearance.effective_font_for_kind(kind).font_size,
                    None => appearance.effective_terminal_font().font_size,
                };
                let size = match change {
                    ZoomChange::In => Some((current + 1.0).min(72.0)),
                    ZoomChange::Out => Some((current - 1.0).max(6.0)),
                    ZoomChange::Reset => None,
                };
                let font = match kind {
                    Some(kind) => appearance
                        .plugin_font_overrides
                        .entry(kind.clone())
                        .or_default(),
                    None => &mut appearance.terminal_font,
                };
                font.font_size = size;
            }
            Self::ExplorerMode(mode) => settings.general.explorer_view_mode = mode.clone(),
            Self::SuppressMouseHint(name) => {
                add_app_pattern(&mut settings.general.mouse_capture_banner_blacklist, name)
            }
            Self::DisableMouseCapture(name) => {
                add_app_pattern(&mut settings.general.mouse_capture_blacklist, name)
            }
        }
    }
}

/// 매칭은 trim·ASCII 소문자화한 패턴으로 하고 빈 패턴을 건너뛴다(`tasty_settings` `matches_blacklist`).
/// 빈 이름은 아무 프로그램에도 맞지 않고, 같은 패턴을 다시 넣으면 결과는 같고 목록만 늘어난다.
/// 두 경우 모두 목록을 바꾸지 않는다.
fn add_app_pattern(list: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if name.is_empty()
        || list
            .iter()
            .any(|existing| existing.trim().eq_ignore_ascii_case(name))
    {
        return;
    }
    list.push(name.to_string());
}

/// 프레임 단계와 무관한 일반 명령. 프레임 drain은 이것을 실행하지 않고 일반 dispatch에 넘긴다.
#[derive(Clone, Debug)]
pub(crate) enum EngineAction {
    #[cfg(feature = "gui")]
    PasteImage {
        target: SurfaceBinding,
        view: std::sync::Weak<()>,
        bracketed: bool,
        file_name: String,
        png_bytes: Vec<u8>,
    },
    #[cfg(feature = "gui")]
    ImageUpload {
        target: SurfaceBinding,
        request: crate::core::PendingImageUpload,
    },
    #[cfg(feature = "gui")]
    Screenshot {
        target: Option<SurfaceBinding>,
        mirror_workspace: Option<u32>,
        view: std::sync::Weak<()>,
    },
    #[cfg(feature = "gui")]
    Explorer(super::requests::explorer::BoundExplorerCommand),
    /// 탐색기 미리보기 패널의 토글·폭을 model 에 남겨 레이아웃 스냅샷에 싣는다.
    #[cfg(feature = "gui")]
    ExplorerPreview {
        target: SurfaceBinding,
        preview: crate::model::ExplorerPreview,
    },
    /// 탐색기의 숨김 파일 표시 여부를 model 에 남겨 레이아웃 스냅샷에 싣는다.
    #[cfg(feature = "gui")]
    ExplorerHidden { target: SurfaceBinding, show: bool },
    #[cfg(feature = "gui")]
    DagSelection {
        target: SurfaceBinding,
        dag_id: Option<String>,
        direction: crate::model::DagDirection,
    },
    RecordTyping {
        target: SurfaceBinding,
        at: std::time::Instant,
    },
    ExplorerCwd {
        target: SurfaceBinding,
        folder: std::path::PathBuf,
    },
    #[cfg(feature = "gui")]
    RemoveExplorerFavorite { path: std::path::PathBuf },
    #[cfg(feature = "gui")]
    AddExplorerFavorite {
        path: std::path::PathBuf,
        label: String,
    },
    #[cfg(feature = "gui")]
    TogglePortFavorite {
        address: std::net::IpAddr,
        port: u16,
        label: String,
    },
    #[cfg(feature = "gui")]
    DetachSurface { surface: u32, grant: u64 },
    DetachWorkspace {
        workspace: u32,
        holder: u32,
        grant: u64,
    },
    #[cfg(feature = "gui")]
    ListDirectory {
        request: crate::core::PendingListDirForward,
        projection: std::sync::Weak<()>,
        target: Option<SurfaceBinding>,
    },
}
impl EngineAction {
    pub(crate) fn apply(&self, engine: &mut EngineMut<'_>) {
        match self {
            #[cfg(feature = "gui")]
            Self::Explorer(request) => {
                super::explorer_action::apply_bound_explorer_command(engine, request);
            }
            #[cfg(feature = "gui")]
            Self::ExplorerPreview { .. } => self.apply_explorer_preview(engine),
            #[cfg(feature = "gui")]
            Self::ExplorerHidden { target, show } => {
                crate::app::explorer_hidden::apply(engine, target, *show)
            }
            #[cfg(feature = "gui")]
            Self::DagSelection { .. } => self.apply_dag_selection(engine),
            #[cfg(feature = "gui")]
            Self::Screenshot {
                target,
                mirror_workspace,
                view,
            } => {
                if target
                    .as_ref()
                    .is_none_or(|target| target.current(&engine.as_ref()))
                {
                    engine
                        .remote
                        .pending_screenshot_captures
                        .push((*mirror_workspace, view.clone()));
                }
            }
            #[cfg(feature = "gui")]
            Self::ImageUpload { target, request } => {
                if target.current(&engine.as_ref()) {
                    engine.remote.pending_image_uploads.push(request.clone());
                }
            }
            #[cfg(feature = "gui")]
            Self::PasteImage { .. } => self.apply_paste_image(engine),
            #[cfg(feature = "gui")]
            Self::AddExplorerFavorite { path, label } => {
                engine
                    .runtime
                    .explorer_favorites
                    .change(|favorites| favorites.add(path.clone(), label.clone()));
            }
            #[cfg(feature = "gui")]
            Self::TogglePortFavorite {
                address,
                port,
                label,
            } => {
                // 사본이 아직 다른 윈도우의 변경을 받지 못했을 수 있으므로 원본 기준으로 뒤집는다.
                engine.runtime.port_favorites.change(|favorites| {
                    if favorites.contains(*address, *port) {
                        favorites.remove(*address, *port);
                    } else {
                        favorites.add(*address, *port, label.clone());
                    }
                });
            }
            #[cfg(feature = "gui")]
            Self::DetachSurface { surface, grant } => {
                if engine
                    .live
                    .occupancy
                    .occupancy_of(*surface)
                    .is_some_and(|lock| lock.granted_seq == *grant)
                {
                    engine.release_occupancy(*surface);
                }
            }
            Self::DetachWorkspace {
                workspace,
                holder,
                grant,
            } => {
                if engine
                    .live
                    .occupancy
                    .workspaces_snapshot()
                    .into_iter()
                    .any(|(id, lock)| {
                        id == *workspace && lock.holder == *holder && lock.granted_seq == *grant
                    })
                {
                    engine.force_detach_workspace(*workspace);
                }
            }
            Self::ExplorerCwd { .. } => self.apply_explorer_cwd(engine),
            #[cfg(feature = "gui")]
            Self::RemoveExplorerFavorite { path } => {
                engine
                    .runtime
                    .explorer_favorites
                    .change(|favorites| favorites.remove(path));
            }
            Self::RecordTyping { target, at } if target.current(&engine.as_ref()) => {
                engine.live.last_key_input.insert(target.surface_id(), *at);
            }
            Self::RecordTyping { .. } => {}
            #[cfg(feature = "gui")]
            Self::ListDirectory {
                request,
                projection,
                target,
            } => {
                if engine.matches_mirror_projection(request.local_ws_id, projection)
                    && target
                        .as_ref()
                        .is_none_or(|target| target.current(&engine.as_ref()))
                {
                    engine.remote.pending_list_dir_forward.push(request.clone());
                }
            }
        }
    }

    #[cfg(feature = "gui")]
    fn apply_paste_image(&self, engine: &mut EngineMut<'_>) {
        let Self::PasteImage {
            target,
            view,
            bracketed,
            file_name,
            png_bytes,
        } = self
        else {
            unreachable!("variant-specific action dispatch")
        };
        if !target.current(&engine.as_ref()) {
            return;
        }
        if let Some(workspace) = target.mirror_workspace() {
            engine
                .remote
                .pending_image_uploads
                .push(crate::core::PendingImageUpload {
                    origin_view: view.clone(),
                    mirror_ws_id: workspace,
                    surface_id: target.surface_id(),
                    bracketed: *bracketed,
                    file_name: file_name.clone(),
                    png_bytes: png_bytes.clone(),
                });
        } else {
            // 파일 저장은 App worker 가 하고, 완료 때 이 대상을 다시 확인해 경로를 입력한다.
            engine.remote.pending_image_saves.push(
                crate::app::image_jobs::paste::PendingImageSave {
                    target: target.clone(),
                    origin_view: view.clone(),
                    bracketed: *bracketed,
                    file_name: file_name.clone(),
                    png_bytes: png_bytes.clone(),
                },
            );
        }
    }
    #[cfg(feature = "gui")]
    fn apply_explorer_preview(&self, engine: &mut EngineMut<'_>) {
        let Self::ExplorerPreview { target, preview } = self else {
            unreachable!("variant-specific action dispatch")
        };
        if target.current(&engine.as_ref())
            && let Some(panel) = engine
                .runtime
                .surfaces
                .get_mut(&target.surface_id())
                .and_then(|surface| {
                    surface
                        .as_any_mut()
                        .downcast_mut::<crate::model::ExplorerPanel>()
                })
            && panel.preview != *preview
        {
            panel.preview = *preview;
            engine.mark_layout_dirty();
        }
    }
    #[cfg(feature = "gui")]
    fn apply_dag_selection(&self, engine: &mut EngineMut<'_>) {
        let Self::DagSelection {
            target,
            dag_id,
            direction,
        } = self
        else {
            unreachable!("variant-specific action dispatch")
        };
        if target.current(&engine.as_ref())
            && let Some(dag) = engine
                .runtime
                .surfaces
                .get_mut(&target.surface_id())
                .and_then(|surface| {
                    surface
                        .as_any_mut()
                        .downcast_mut::<crate::model::DagGraphSurface>()
                })
        {
            dag.dag_id = dag_id.clone();
            dag.direction = *direction;
            engine.mark_layout_dirty();
        }
    }

    fn apply_explorer_cwd(&self, engine: &mut EngineMut<'_>) {
        let Self::ExplorerCwd { target, folder } = self else {
            unreachable!("variant-specific action dispatch")
        };
        if !target.current(&engine.as_ref()) {
            return;
        }
        if let Some(explorer) = engine
            .runtime
            .surfaces
            .get_mut(&target.surface_id())
            .and_then(|surface| {
                surface
                    .as_any_mut()
                    .downcast_mut::<crate::model::ExplorerPanel>()
            })
        {
            explorer.active_tab_mut().set_cwd(folder.clone());
            engine.mark_layout_dirty();
        }
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::EngineAction;

    #[test]
    fn an_empty_app_name_is_not_added_to_either_blacklist() {
        use super::SettingsPatch;
        let mut settings = crate::settings::Settings::default();
        let banner_before = settings.general.mouse_capture_banner_blacklist.clone();
        let capture_before = settings.general.mouse_capture_blacklist.clone();
        SettingsPatch::SuppressMouseHint(String::new()).apply(&mut settings);
        SettingsPatch::SuppressMouseHint(" \t".into()).apply(&mut settings);
        SettingsPatch::DisableMouseCapture(String::new()).apply(&mut settings);
        SettingsPatch::DisableMouseCapture("  ".into()).apply(&mut settings);
        assert_eq!(
            settings.general.mouse_capture_banner_blacklist,
            banner_before
        );
        assert_eq!(settings.general.mouse_capture_blacklist, capture_before);
    }

    #[test]
    fn the_same_app_pattern_is_added_once_and_trimmed() {
        use super::SettingsPatch;
        let mut settings = crate::settings::Settings::default();
        settings.general.mouse_capture_banner_blacklist.clear();
        settings.general.mouse_capture_blacklist.clear();
        for name in ["vim", "vim", " vim ", "VIM"] {
            SettingsPatch::SuppressMouseHint(name.into()).apply(&mut settings);
            SettingsPatch::DisableMouseCapture(name.into()).apply(&mut settings);
        }
        assert_eq!(
            settings.general.mouse_capture_banner_blacklist,
            vec!["vim".to_string()]
        );
        assert_eq!(
            settings.general.mouse_capture_blacklist,
            vec!["vim".to_string()]
        );
    }

    /// 설정 화면에서 넣은 공백·대문자 항목도 같은 패턴으로 본다(매칭이 trim·소문자화하므로).
    #[test]
    fn a_stored_pattern_with_spaces_counts_as_the_same_name() {
        use super::SettingsPatch;
        let mut settings = crate::settings::Settings::default();
        settings.general.mouse_capture_blacklist = vec![" HTOP ".to_string()];
        SettingsPatch::DisableMouseCapture("htop".into()).apply(&mut settings);
        assert_eq!(
            settings.general.mouse_capture_blacklist,
            vec![" HTOP ".to_string()]
        );
    }

    fn engine_with(
        registries: &crate::runtime::registries::RuntimeRegistries,
    ) -> crate::runtime::engine_session::EngineSession {
        let memory: std::sync::Arc<std::sync::Mutex<dyn tasty_memory::MemoryStorage>> =
            std::sync::Arc::new(std::sync::Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().expect("in-memory store"),
            ));
        crate::runtime::engine_session::EngineSession::for_journal(
            crate::runtime::engine_session::EngineSessionSpec {
                cols: 80,
                rows: 24,
                waker: std::sync::Arc::new(|| {}),
                shared_ids: None,
                layout_slot: None,
                memory: tasty_memory::writer::MemoryAccess::inline(memory),
                runner_registry: std::sync::Arc::new(tasty_task_runtime::RunnerRegistry::new()),
            },
            crate::settings::Settings::default(),
            registries.clone(),
        )
        .expect("engine")
    }

    fn favorites_writer(
        registries: &crate::runtime::registries::RuntimeRegistries,
    ) -> crate::app::file_writer::FileWriterParts {
        let parts = crate::app::file_writer::start(
            std::sync::Arc::new(tasty_settings::FileSettingsStorage),
            tasty_utils::batch_writer::Limits::default(),
            std::sync::Arc::new(tasty_utils::batch_writer::SystemClock),
            std::sync::Arc::new(|| {}),
        )
        .unwrap();
        crate::app::file_consumers::attach(registries, parts.writer.handle());
        parts
    }

    fn wait_favorites(registries: &crate::runtime::registries::RuntimeRegistries) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // poll 은 끝난 저장 영수증만 거두므로 기한 뒤 관측에서도 거둔 뒤 읽는다.
        assert!(
            tasty_utils::test_wait::holds_by_every(deadline, std::time::Duration::ZERO, |_| {
                assert!(registries.explorer_favorites.poll().is_empty());
                assert!(registries.port_favorites.poll().is_empty());
                [
                    registries.explorer_favorites.save_status(),
                    registries.port_favorites.save_status(),
                ]
                .into_iter()
                .all(|(visible, committed, pending)| visible == committed && pending == 0)
            }),
            "favorites not saved"
        );
    }

    fn stop_favorites_writer(writer: &mut crate::app::file_writer::FileWriterParts) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // writer 의 poll_shutdown 은 끝난 worker 를 join 할 뿐 새 작업을 만들지 않는다.
        assert!(
            tasty_utils::test_wait::holds_by(deadline, || writer.writer.poll_shutdown() == 0),
            "favorites writer not drained"
        );
    }

    /// 윈도우마다 engine 이 있어도 즐겨찾기 추가가 서로를 지우지 않고, 다른 윈도우의 사본도 맞춰진다.
    #[test]
    fn favorites_added_in_two_windows_both_survive_a_restart() {
        let _home = crate::test_support::IsolatedHome::new();
        // Engine test owners push nested thread-local homes; retain the registry's actual file.
        let explorer_path =
            crate::core::explorer_favorites::ExplorerFavorites::config_path().unwrap();
        let registries = crate::runtime::registries::RuntimeRegistries::new(None);
        let mut writer = favorites_writer(&registries);
        let mut first = engine_with(&registries);
        let mut second = engine_with(&registries);
        let add = |session: &mut crate::runtime::engine_session::EngineSession, name: &str| {
            EngineAction::AddExplorerFavorite {
                path: crate::test_support::abs_path(name),
                label: String::new(),
            }
            .apply(&mut session.borrow_mut());
        };
        add(&mut first, "w/alpha");
        add(&mut second, "w/beta");
        let labels = |favorites: &crate::core::explorer_favorites::ExplorerFavorites| {
            favorites
                .items
                .iter()
                .map(|f| f.label.clone())
                .collect::<Vec<_>>()
        };

        // 재시작: 새 프로세스 원본은 파일에서 읽는다.
        wait_favorites(&registries);
        let restarted = crate::core::shared_list::SharedList::<
            crate::core::explorer_favorites::ExplorerFavorites,
        >::load_at(explorer_path.clone());
        assert_eq!(labels(&restarted.copy().1), ["alpha", "beta"]);

        // 첫 윈도우의 사본은 다음 그리기 전에 다른 윈도우의 추가를 받는다.
        assert_eq!(labels(&first.runtime.explorer_favorites), ["alpha"]);
        assert!(first.runtime.sync_shared_lists());
        assert_eq!(labels(&first.runtime.explorer_favorites), ["alpha", "beta"]);
        assert!(!first.runtime.sync_shared_lists(), "already current");

        EngineAction::RemoveExplorerFavorite {
            path: crate::test_support::abs_path("w/alpha"),
        }
        .apply(&mut second.borrow_mut());
        wait_favorites(&registries);
        assert_eq!(
            labels(
                &crate::core::shared_list::SharedList::<
                    crate::core::explorer_favorites::ExplorerFavorites,
                >::load_at(explorer_path)
                .copy()
                .1
            ),
            ["beta"]
        );
        stop_favorites_writer(&mut writer);
    }

    /// 포트 즐겨찾기도 윈도우마다 다른 항목을 바꿔도 서로 지우지 않는다. 다른 윈도우의 변경을 아직 받지 못한
    /// 사본에서 같은 항목을 누르면 원본 기준으로 해제된다.
    #[test]
    fn port_favorites_toggled_in_two_windows_both_survive_a_restart() {
        let _home = crate::test_support::IsolatedHome::new();
        // Engine test owners push nested thread-local homes; retain the registry's actual file.
        let port_path = crate::core::port_favorites::PortFavorites::config_path().unwrap();
        let registries = crate::runtime::registries::RuntimeRegistries::new(None);
        let mut writer = favorites_writer(&registries);
        let mut first = engine_with(&registries);
        let mut second = engine_with(&registries);
        let localhost: std::net::IpAddr = "127.0.0.1".parse().expect("address");
        let toggle = |session: &mut crate::runtime::engine_session::EngineSession, port: u16| {
            EngineAction::TogglePortFavorite {
                address: localhost,
                port,
                label: format!("p{port}"),
            }
            .apply(&mut session.borrow_mut());
        };
        let ports = |favorites: &crate::core::port_favorites::PortFavorites| {
            favorites.items.iter().map(|f| f.port).collect::<Vec<_>>()
        };
        toggle(&mut first, 47811);
        toggle(&mut second, 47812);
        wait_favorites(&registries);
        let restarted = crate::core::shared_list::SharedList::<
            crate::core::port_favorites::PortFavorites,
        >::load_at(port_path.clone());
        assert_eq!(ports(&restarted.copy().1), [47811, 47812]);

        assert_eq!(ports(&first.runtime.port_favorites), [47811]);
        toggle(&mut first, 47812);
        wait_favorites(&registries);
        assert_eq!(
            ports(&crate::core::shared_list::SharedList::<crate::core::port_favorites::PortFavorites>::load_at(port_path).copy().1),
            [47811]
        );
        assert_eq!(ports(&first.runtime.port_favorites), [47811]);
        assert!(second.runtime.sync_shared_lists());
        assert_eq!(ports(&second.runtime.port_favorites), [47811]);
        stop_favorites_writer(&mut writer);
    }

    #[test]
    fn plugin_zoom_starts_from_and_resets_to_the_plugin_default() {
        use super::{SettingsPatch, ZoomChange};
        use crate::settings::PluginSettingValue;
        let zoom = |settings: &crate::settings::Settings| match settings
            .plugin_setting("com.example.view", "zoom")
        {
            Some(PluginSettingValue::Number(n)) => *n,
            other => panic!("zoom not stored: {other:?}"),
        };
        let patch = |change| SettingsPatch::PluginZoom {
            plugin: "com.example.view".into(),
            default_zoom: 150.0,
            change,
        };
        let mut settings = crate::settings::Settings::default();
        patch(ZoomChange::In).apply(&mut settings);
        assert_eq!(zoom(&settings), 160.0);
        patch(ZoomChange::Reset).apply(&mut settings);
        assert_eq!(zoom(&settings), 150.0);
        for _ in 0..40 {
            patch(ZoomChange::In).apply(&mut settings);
        }
        assert_eq!(zoom(&settings), tasty_plugin_manifest::WEBVIEW_ZOOM_MAX);
    }
}
