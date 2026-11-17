//! 플러그인 명령 단축키의 설정·조회·키 입력 처리.

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::plugin::registry_state::shortcut_override_display;
use crate::{plugin, settings_ui, shortcuts};

pub(crate) type Draft =
    std::collections::BTreeMap<(String, String), Option<plugin::registry_state::ShortcutOverride>>;

fn emit_shortcut_changed_events(
    mgr: &mut plugin::PluginManager,
    emit_queue: Vec<(String, String, Option<String>, Option<String>)>,
) {
    for (plugin_id, command_id, shortcut, prev_shortcut) in emit_queue {
        use tasty_plugin_protocol::EventScope;
        use tasty_plugin_protocol::events::payloads::CommandShortcutChanged;
        let payload = CommandShortcutChanged {
            plugin_id,
            command_id,
            shortcut,
            prev_shortcut,
        };
        mgr.emit_host_event("command.shortcut_changed", &payload, EventScope::System);
    }
}

impl App {
    pub(crate) fn apply_plugin_shortcut_draft(
        &mut self,
        draft: std::collections::BTreeMap<
            (String, String),
            Option<plugin::registry_state::ShortcutOverride>,
        >,
    ) {
        self.queue_plugin_shortcut_draft(draft, false);
    }

    pub(crate) fn queue_plugin_shortcut_draft(&mut self, draft: Draft, retry: bool) {
        if draft.is_empty() {
            return;
        }
        let draft = match self.plugin_files.coalesce_shortcuts(draft) {
            Ok(()) => return,
            Err(draft) => draft,
        };
        let (pending, held) = match self.plugin_files.reserve_shortcuts(draft) {
            Ok(reserved) => reserved,
            Err(error) => {
                tracing::warn!(error=%error.message,"plugin shortcuts not accepted");
                self.toast_settings_result(error.message, crate::adapters::ui::ToastKind::Error);
                return;
            }
        };
        let values = pending.clone();
        let start =
            move |app: &mut App| {
                if app
                    .plugin_files
                    .shortcuts
                    .as_ref()
                    .is_some_and(|pending| std::rc::Rc::ptr_eq(pending, &values))
                {
                    app.plugin_files.clear_shortcut_slot();
                }
                let draft = std::mem::take(&mut *values.borrow_mut());
                let mut events = Vec::new();
                if let Some(mgr) = app.plugin_manager.as_ref() {
                    for ((id, command), value) in &draft {
                        let before =
                            shortcut_override_display(mgr.config.shortcut_override(id, command));
                        let after = shortcut_override_display(value.as_ref());
                        if before != after {
                            events.push((id.clone(), command.clone(), after, before));
                        }
                    }
                }
                let change = tasty_host_plugin::manager::file_config::ConfigChange::Shortcuts(
                    draft
                        .iter()
                        .map(|((id, cmd), value)| (id.clone(), cmd.clone(), value.clone()))
                        .collect(),
                );
                let context_bytes = draft_bytes(&draft)
                    .saturating_add(crate::app::file_writer::command::value_bytes(&events));
                app.start_plugin_config_retained(change, context_bytes, move |app, result| {
                    match result {
                        Ok(_) => {
                            app.plugin_files.settle_shortcuts(draft, true);
                            if let Some(mgr) = app.plugin_manager.as_mut() {
                                emit_shortcut_changed_events(mgr, events);
                            }
                        }
                        Err(error) => {
                            app.plugin_files.settle_shortcuts(draft, false);
                            tracing::warn!(error=%error.message,"plugin shortcuts were not saved");
                            app.toast_settings_result(
                                error.message,
                                crate::adapters::ui::ToastKind::Error,
                            );
                        }
                    }
                });
            };
        let admitted = self
            .plugin_files
            .enqueue_shortcuts(held.clone(), start, retry);
        match admitted {
            Ok(()) => {
                self.plugin_files.mark_shortcut_slot(pending, held);
                (self.services.reply_wake())();
            }
            Err(error) => {
                self.plugin_files
                    .keep_unqueued_shortcuts(std::mem::take(&mut *pending.borrow_mut()), held);
                tracing::warn!(error=%error.message,"plugin shortcuts not accepted");
                self.toast_settings_result(error.message, crate::adapters::ui::ToastKind::Error);
            }
        }
    }

    pub(crate) fn snapshot_plugin_shortcuts(&self) -> settings_ui::PluginShortcutSnapshot {
        let Some(mgr) = self.plugin_manager.as_ref() else {
            return settings_ui::PluginShortcutSnapshot::default();
        };
        let name_for: std::collections::HashMap<&str, &str> = mgr
            .packages()
            .iter()
            .map(|p| (p.manifest.id.as_str(), p.manifest.name.as_str()))
            .collect();

        let rows: Vec<settings_ui::PluginShortcutRow> = mgr
            .command_registry
            .iter_all()
            .map(|e| settings_ui::PluginShortcutRow {
                plugin_id: e.plugin_id.clone(),
                plugin_name: name_for
                    .get(e.plugin_id.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| e.plugin_id.clone()),
                command_id: e.command_id.clone(),
                title_i18n_key: e.title_i18n_key.clone(),
                binding_mode: e.binding_mode.clone(),
                manifest_default: e.manifest_default.clone(),
                current_override: mgr
                    .config
                    .shortcut_override(&e.plugin_id, &e.command_id)
                    .cloned(),
            })
            .collect();
        settings_ui::PluginShortcutSnapshot { rows }
    }

    /// 가져오기·내보내기는 등록 여부와 무관하게 저장된 override 전체를 사용한다.
    pub(crate) fn plugin_bundle_context(&self) -> settings_ui::PluginBundleContext {
        let Some(mgr) = self.plugin_manager.as_ref() else {
            return settings_ui::PluginBundleContext::default();
        };
        settings_ui::PluginBundleContext {
            overrides: mgr.config.shortcut_overrides().clone(),
            installed_plugin_ids: mgr
                .packages()
                .iter()
                .map(|p| p.manifest.id.clone())
                .collect(),
            plugin_names: mgr
                .packages()
                .iter()
                .map(|p| (p.manifest.id.clone(), p.manifest.name.clone()))
                .collect(),
        }
    }

    /// 포커스된 플러그인이 있으면 그 플러그인의 모든 scope를, 없으면 전체 Global 명령을 찾는다.
    /// 처리했으면 호출자는 일반 창 키 처리를 생략해 같은 키가 두 번 실행되지 않게 한다.
    pub(crate) fn try_plugin_shortcut(
        &mut self,
        id: winit::window::WindowId,
        ke: &winit::event::KeyEvent,
    ) -> bool {
        use winit::event::ElementState;
        if ke.state != ElementState::Pressed {
            return false;
        }
        let Some(main) = self.view.views.get_mut(&id).and_then(|w| w.as_main_mut()) else {
            return false;
        };
        let mods = main.base.state.modifiers;
        let shortcut_key = shortcuts::shortcut_lookup_key(&ke.logical_key, &ke.physical_key, mods);
        self.dispatch_plugin_shortcut_key(id, &shortcut_key, mods)
    }

    /// winit과 native webview 입력이 같은 차단 조건·우선순위를 사용한다.
    pub(crate) fn dispatch_plugin_shortcut_key(
        &mut self,
        id: winit::window::WindowId,
        shortcut_key: &winit::keyboard::Key,
        mods: winit::keyboard::ModifiersState,
    ) -> bool {
        if self.view.is_modal_active() {
            return false;
        }
        let Some((main, engine)) = engines_mut!(self).window_pair(id) else {
            return false;
        };
        // 일반 창 키 처리보다 먼저 실행되므로 popup·overlay·전체화면 무대의 키를 여기서 보호한다.
        if main.state.keyboard_overlay_open() || main.state.fullscreen_stage_active() {
            return false;
        }
        let focused = crate::plugin_bridge::key_dispatch::focused_plugin_surface(
            &main.state,
            &engine.as_ref(),
        );
        // 포커스 WebView의 편집 바인딩은 plugin 명령보다 먼저다. native 자식이 키를 받을 때와 같은 순서다.
        if main
            .webview_edit_for_key(&engine.read(), shortcut_key, mods)
            .is_some()
        {
            return false;
        }
        let host_kb = engine.runtime.settings.keybindings.clone();

        let matched = {
            let Some(mgr) = self.plugin_manager.as_ref() else {
                return false;
            };
            match &focused {
                Some((plugin_id, surface_id)) => {
                    crate::plugin_bridge::key_dispatch::match_plugin_shortcut(
                        mgr,
                        plugin_id,
                        shortcut_key,
                        mods,
                        &host_kb,
                    )
                    .map(|cmd_id| (plugin_id.clone(), cmd_id, Some(*surface_id)))
                }
                None => crate::plugin_bridge::key_dispatch::match_global_shortcut(
                    mgr,
                    shortcut_key,
                    mods,
                    &host_kb,
                )
                .map(|(plugin_id, cmd_id)| (plugin_id, cmd_id, None)),
            }
        };
        let Some((plugin_id, cmd_id, surface_id)) = matched else {
            return false;
        };

        let action = self
            .plugin_manager
            .as_ref()
            .and_then(|mgr| mgr.command_registry.find(&plugin_id, &cmd_id))
            .and_then(|e| e.action.clone());

        if let Some(action) = action {
            // action과 command.invoke를 함께 실행하면 같은 명령이 두 번 처리된다.
            // 호스트 action만 실행하되 command.invoked 알림은 보낸다. 대상은 키를 누른 지금 정한다.
            let item = plugin::tool_registry::ToolItem {
                source: plugin::tool_registry::ToolSource::Plugin {
                    plugin_id: plugin_id.clone(),
                    tool_id: cmd_id.clone(),
                },
                key: format!("{plugin_id}/{cmd_id}"),
                label_i18n_key: String::new(),
                icon: None,
                action,
                order_hint: 0,
            };
            let invoked = crate::state::attach_pending::deferred::InvokedNotice {
                plugin_id,
                command_id: cmd_id,
                surface_id,
            };
            let target = main.state.command_surface_target(&engine.read());
            let mgr = self.plugin_manager.as_mut();
            // 실행할 수 없는 동작이면 실행도 알림도 하지 않고 키를 다음 처리자에 넘긴다.
            if let Err(blocked) = crate::adapters::ui::tools_menu::run_tool(
                &mut main.state,
                &engine.read(),
                &item,
                target,
                Some(invoked),
                |n| {
                    if let Some(mgr) = mgr {
                        crate::plugin_bridge::key_dispatch::emit_command_invoked(
                            mgr,
                            &n.plugin_id,
                            &n.command_id,
                            n.surface_id,
                        );
                    }
                },
            ) {
                tracing::warn!(
                    "plugin command '{}' not run: {}",
                    item.key,
                    blocked.message()
                );
                return false;
            }
        } else if let Some(mgr) = self.plugin_manager.as_mut() {
            crate::plugin_bridge::key_dispatch::dispatch_plugin_command(
                mgr, &plugin_id, &cmd_id, surface_id,
            );
        }
        true
    }
}

pub(crate) fn draft_bytes(draft: &Draft) -> usize {
    crate::app::file_writer::command::value_bytes(&draft.iter().collect::<Vec<_>>())
}
