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
        let value = crate::adapters::ui::input::key_class::KeyEventValue {
            logical: &ke.logical_key,
            physical: ke.physical_key,
            pressed: true,
            mods,
        };
        self.dispatch_plugin_shortcut_key(id, &shortcut_key, mods, PluginKeyGate::Winit(&value))
    }

    /// winit과 native webview 입력이 같은 차단 조건·우선순위를 사용한다. 창의 판정·실행은
    /// `plugin_shortcut_in_window` 가 한다.
    pub(crate) fn dispatch_plugin_shortcut_key(
        &mut self,
        id: winit::window::WindowId,
        shortcut_key: &winit::keyboard::Key,
        mods: winit::keyboard::ModifiersState,
        gate: PluginKeyGate<'_>,
    ) -> bool {
        if self.view.is_modal_active() {
            return false;
        }
        let Some((main, engine)) = engines_mut!(self).window_pair(id) else {
            return false;
        };
        let webviews = &main.webviews;
        plugin_shortcut_in_window(
            &mut main.state,
            &engine,
            self.plugin_manager.as_mut(),
            |sid| webviews.contains_key(&sid),
            shortcut_key,
            mods,
            gate,
        )
    }
}

/// plugin 명령 게이트가 볼 키 분류. winit 키는 사건 값으로 단계 4와 같이 분류하고, native WebView 가 넘긴 키는 그 입구가
/// 한 번 정한 분류(`key_nodes::forwarded_key_class`)와 그 키를 받은 surface 를 받는다.
#[derive(Clone, Copy)]
pub(crate) enum PluginKeyGate<'a> {
    Winit(&'a crate::adapters::ui::input::key_class::KeyEventValue<'a>),
    Forwarded(crate::adapters::ui::input::key_nodes::ForwardedKey, u32),
}

impl PluginKeyGate<'_> {
    /// native WebView 가 넘긴 키를 받은 surface. winit 키는 None 이다.
    fn received_by(&self) -> Option<u32> {
        match self {
            Self::Winit(_) => None,
            Self::Forwarded(_, sid) => Some(*sid),
        }
    }
}

/// 한 창에서 plugin 명령 단축키를 판정하고 실행한다. `has_webview` 는 surface 에 native WebView 가 있는지다. 처리했으면
/// true 이고, 호출자는 일반 창 키 처리를 생략한다. App 의 단축키 처리와 제품 상태 시험이 함께 쓴다.
pub(crate) fn plugin_shortcut_in_window(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_access::EngineMut<'_>,
    plugin_manager: Option<&mut plugin::PluginManager>,
    has_webview: impl Fn(u32) -> bool,
    shortcut_key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
    gate: PluginKeyGate<'_>,
) -> bool {
    // 일반 창 키 처리보다 먼저 실행되므로 popup·overlay·전체화면 무대의 키를 여기서 보호한다. 키는 창의
    // 포커스 노드에서 시작하므로 그 값을 먼저 정규화한다. winit 키는 포커스 노드에서 시작한
    // 경로가 출발 노드에서 그 키를 끝내면 닫는다. native WebView 가 넘긴 키는 받은 surface 에서 시작한 경로가
    // 덮였으면 닫는다. 지금 경로로 남는 사건은 어느 입구든 지금 게이트다.
    state.normalize_focus(engine);
    let received_by = gate.received_by();
    let gate_open = match gate {
        PluginKeyGate::Winit(value) => crate::adapters::ui::input::key_nodes::plugin_key_gate_open(
            state,
            &engine.read(),
            value,
        ),
        PluginKeyGate::Forwarded(class, sid) => {
            !crate::adapters::ui::input::key_nodes::received_key_blocked(
                state,
                &engine.read(),
                class,
                sid,
            )
        }
    };
    if !gate_open {
        return false;
    }
    // native WebView 가 넘긴 키는 그 키를 받은 surface 가 입력 시점 대상이고 키 경로는 그 surface 에서 시작한다.
    // winit 키는 지금의 대상이고 경로는 포커스 노드에서 시작한다.
    let path = match received_by {
        Some(_) => Some(crate::adapters::ui::input::key_level::PathLevels::all()),
        None => crate::adapters::ui::input::key_path::focus_path_levels(state, &engine.read()),
    };
    let (focused, target) = match received_by {
        Some(sid) => (
            crate::plugin_bridge::key_dispatch::plugin_surface(&engine.as_ref(), sid),
            state
                .command_target_for_surface(&engine.read(), sid)
                .unwrap_or(crate::state::command_target::CommandTarget {
                    surface: Some(
                        crate::state::attach_pending::deferred::CommandSurfaceTarget::Surface(sid),
                    ),
                    ..Default::default()
                }),
        ),
        None => (
            crate::plugin_bridge::key_dispatch::focused_plugin_surface(state, &engine.as_ref()),
            state.command_target(&engine.read()),
        ),
    };
    // 포커스 WebView의 편집 바인딩은 plugin 명령보다 먼저다. native 자식이 키를 받을 때와 같은 순서다.
    if shortcuts::webview_edit_for_surface_key(
        state,
        has_webview,
        &engine.read(),
        shortcut_key,
        mods,
    )
    .is_some()
    {
        return false;
    }
    let host_kb = engine.runtime.settings.keybindings.clone();

    let matched = {
        let Some(mgr) = plugin_manager.as_deref() else {
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
                    path,
                )
                .map(|cmd_id| (plugin_id.clone(), cmd_id, Some(*surface_id)))
            }
            None => crate::plugin_bridge::key_dispatch::match_global_shortcut(
                mgr,
                shortcut_key,
                mods,
                &host_kb,
                path,
            )
            .map(|(plugin_id, cmd_id)| (plugin_id, cmd_id, None)),
        }
    };
    let Some((plugin_id, cmd_id, surface_id)) = matched else {
        return false;
    };

    let action = plugin_manager
        .as_deref()
        .and_then(|mgr| mgr.command_registry.find(&plugin_id, &cmd_id))
        .and_then(|e| e.action.clone());

    if let Some(action) = action {
        return run_shortcut_action(
            state,
            &engine.read(),
            plugin_manager,
            (plugin_id, cmd_id, surface_id),
            action,
            &target,
        );
    } else if let Some(mgr) = plugin_manager {
        crate::plugin_bridge::key_dispatch::dispatch_plugin_command(
            mgr, &plugin_id, &cmd_id, surface_id,
        );
    }
    true
}

/// 명령 단축키의 action 갈래. 대상은 키를 입력한 시점의 것을 호출자가 넘긴다. action과 command.invoke를 함께 실행하면
/// 같은 명령이 두 번 처리되므로 호스트 action만 실행하고 command.invoked 알림은 실행할 때 보낸다.
/// 실행했거나 생성 대기 초안에 걸어 두었으면 키를 소비해 true, 실행할 수 없으면 키를 다음 처리자에 넘겨 false.
pub(crate) fn run_shortcut_action(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: Option<&mut tasty_host_plugin::PluginManager>,
    (plugin_id, cmd_id, surface_id): (String, String, Option<u32>),
    action: crate::plugin::manifest::ToolAction,
    target: &crate::state::command_target::CommandTarget,
) -> bool {
    let item = crate::adapters::ui::tools_menu::command_item(&plugin_id, &cmd_id, action);
    let invoked = crate::state::attach_pending::deferred::InvokedNotice {
        plugin_id,
        command_id: cmd_id,
        surface_id,
    };
    if let Err(blocked) = crate::adapters::ui::tools_menu::run_tool(
        state,
        engine,
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
        blocked.log_not_run("plugin command", &item.key);
        return false;
    }
    true
}

pub(crate) fn draft_bytes(draft: &Draft) -> usize {
    crate::app::file_writer::command::value_bytes(&draft.iter().collect::<Vec<_>>())
}

#[cfg(test)]
mod action_tests {
    use super::run_shortcut_action;
    use crate::plugin::manifest::ToolAction;

    const SURFACE_POPUP: &str = "com.example.viewer/confirm";

    fn open() -> ToolAction {
        ToolAction::OpenPopup {
            popup_id: SURFACE_POPUP.into(),
        }
    }

    fn matched() -> (String, String, Option<u32>) {
        ("com.example.viewer".into(), "confirm".into(), None)
    }

    /// 초안 선택 중 global 로 매칭된 명령 단축키는 초안에 걸어 두고 키를 소비한다.
    #[test]
    fn a_global_shortcut_on_a_pending_draft_is_held_and_consumes_the_key() {
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        let engine = session.borrow_mut();
        let read = engine.read();
        let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
        let pending = state.attach_pending.entries[0].id;
        let target = state.command_target(&read);
        assert!(run_shortcut_action(
            &mut state,
            &read,
            None,
            matched(),
            open(),
            &target
        ));
        assert_eq!(state.attach_pending.deferred.waiting_for(pending), 1);
        assert!(state.pending_popup_opens.is_empty());
    }

    /// 호출자가 넘긴 입력 시점 대상으로 실행한다. 지금 초안이 선택돼 있어도 넘겨받은 surface 가 대상이다.
    #[test]
    fn a_shortcut_runs_on_the_target_its_caller_bound() {
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        let engine = session.borrow_mut();
        let read = engine.read();
        let anchor = state.focused_surface_id(&read).expect("focused surface");
        let target = state.command_target(&read);
        let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
        let pending = state.attach_pending.entries[0].id;
        assert!(run_shortcut_action(
            &mut state,
            &read,
            None,
            matched(),
            open(),
            &target
        ));
        assert_eq!(state.attach_pending.deferred.waiting_for(pending), 0);
        assert_eq!(state.pending_popup_opens.len(), 1);
        assert_eq!(state.pending_popup_opens[0].target_surface, Some(anchor));
    }

    /// 초안이 선택돼 있으면 포커스 plugin surface 가 없다. 초안 뒤에 남은 기존 plugin surface 의 surface 범위
    /// 명령을 고르지 않는다. native WebView 가 넘긴 키는 받은 surface 로 고른다.
    #[test]
    fn a_draft_hides_the_previous_plugin_surface_from_shortcut_scope() {
        use crate::plugin_bridge::key_dispatch::{focused_plugin_surface, plugin_surface};
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        let anchor = state
            .focused_surface_id(&session.read())
            .expect("focused surface");
        let plugin = "com.example.viewer";
        session.runtime.surfaces.insert(
            anchor,
            Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
                anchor,
                "terminal",
                plugin.into(),
                "remote".into(),
            )),
        );
        let engine = session.borrow_mut();
        let expected = Some((plugin.to_string(), anchor));
        assert_eq!(focused_plugin_surface(&state, &engine.as_ref()), expected);
        let read = engine.read();
        let _draft = crate::state::tests::select_pending_draft(&mut state, &read);
        assert_eq!(focused_plugin_surface(&state, &engine.as_ref()), None);
        assert_eq!(plugin_surface(&engine.as_ref(), anchor), expected);
    }

    /// 대상 surface 도 초안도 없으면 surface 팝업 명령은 실행하지 않고 키를 다음 처리자에 넘긴다.
    #[test]
    fn a_blocked_shortcut_runs_nothing_and_passes_the_key_on() {
        let (mut state, mut session) =
            crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![]));
        state
            .tool_registry
            .set_surface_popups([SURFACE_POPUP.to_string()].into_iter().collect());
        let engine = session.borrow_mut();
        let read = engine.read();
        assert_eq!(state.command_surface_target(&read), None);
        let target = state.command_target(&read);
        assert!(!run_shortcut_action(
            &mut state,
            &read,
            None,
            matched(),
            open(),
            &target
        ));
        assert!(state.pending_popup_opens.is_empty());
    }
}
