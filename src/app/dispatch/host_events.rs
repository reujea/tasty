//! 대기 중인 호스트 이벤트를 프로토콜 payload로 바꿔 전달한다.

mod misc;
mod pane;
mod surface;
mod tab;
mod workspace;

#[cfg(test)]
mod created_window_tests;

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::runtime::engine_access::{EngineMut, EngineRef};
use crate::state::PendingHostEvent;

pub(crate) fn take_current_host_events(engine: &mut EngineMut<'_>) -> Vec<PendingHostEvent> {
    let mut events = engine.take_pending_host_events();
    events.retain(|event| match event {
        PendingHostEvent::SurfaceTitleChanged {
            surface_id,
            generation,
            ..
        } => engine
            .runtime
            .terminals
            .matches_generation(*surface_id, *generation),
        PendingHostEvent::SurfaceRefreshRequested { target, kind, .. } => {
            crate::core::host_event::refresh_target_current(&engine.as_ref(), target, kind)
        }
        _ => true,
    });
    events
}

impl App {
    pub(crate) fn dispatch_pending_host_events(&mut self) {
        let mut drained = std::mem::take(&mut self.state.pending_host_events);
        for (state, mut engine) in engines_mut!(self).sessions() {
            observe_window_focus(state, &mut engine);
        }
        // Includes parked, pending and retiring owners; delivery never depends on a live View.
        for session in self.engines.all_sessions_mut() {
            let mut engine = session.borrow_mut();
            let events = take_current_host_events(&mut engine);
            resolve_hook_fired_task_waits(&self.services, &engine.as_ref(), &events);
            drained.extend(events);
        }
        if drained.is_empty() {
            return;
        }
        let scripts = self.autofire_scripts();
        macro_rules! af {
            () => {
                crate::hooks::lua::AutofireCtx {
                    scripts: &scripts,
                    guard: &mut self.lua_autofire,
                    runs: &mut self.lua_runs,
                }
            };
        }
        let lua_send = crate::app::dispatch::lua_commands::lua_send!(self);
        let lua = lua_send.as_ref();
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        for ev in drained {
            match ev {
                PendingHostEvent::SurfaceFocused {
                    surface_id,
                    prev_surface_id,
                } => surface::emit_focused(mgr, surface_id, prev_surface_id),
                PendingHostEvent::SurfaceTitleChanged {
                    surface_id, title, ..
                } => surface::emit_title_changed(mgr, surface_id, title),
                PendingHostEvent::SurfaceCreated {
                    surface_id,
                    kind,
                    tab_id,
                    pane_id,
                    workspace_id,
                    created_by_plugin,
                } => surface::emit_created(
                    mgr,
                    lua,
                    af!(),
                    surface_id,
                    kind,
                    tab_id,
                    pane_id,
                    workspace_id,
                    created_by_plugin,
                ),
                PendingHostEvent::WorkspaceActivated {
                    workspace_id,
                    prev_workspace_id,
                } => workspace::emit_activated(mgr, workspace_id, prev_workspace_id),
                PendingHostEvent::WorkspaceRenamed {
                    workspace_id,
                    name,
                    subtitle,
                    description,
                    user_direct,
                } => workspace::emit_renamed(
                    mgr,
                    lua,
                    af!(),
                    workspace::RenameEvent {
                        workspace_id,
                        name,
                        subtitle,
                        description,
                        user_direct,
                    },
                ),
                PendingHostEvent::WorkspaceCreated { workspace_id, name } => {
                    // 쌓을 때는 창을 모르므로 발행 시점의 소유 창을 찾는다.
                    let windows = self.engines.windows_in(self.view.views.keys().copied());
                    let window = workspace::created_window(
                        windows.map(|(wid, e)| (wid, e.core)),
                        workspace_id,
                    );
                    workspace::emit_created(mgr, lua, af!(), workspace_id, window, name)
                }
                PendingHostEvent::WorkspaceClosed { workspace_id } => {
                    workspace::emit_closed(mgr, lua, af!(), workspace_id)
                }
                PendingHostEvent::TabFocused {
                    tab_id,
                    pane_id,
                    prev_tab_id,
                } => tab::emit_focused(mgr, tab_id, pane_id, prev_tab_id),
                PendingHostEvent::TabRenamed {
                    tab_id,
                    title,
                    user_direct,
                } => tab::emit_renamed(mgr, lua, af!(), tab_id, title, user_direct),
                PendingHostEvent::TabCreated {
                    tab_id,
                    pane_id,
                    workspace_id,
                    kind,
                } => tab::emit_created(mgr, lua, af!(), tab_id, pane_id, workspace_id, kind),
                PendingHostEvent::TabClosed { tab_id, pane_id } => {
                    tab::emit_closed(mgr, lua, af!(), tab_id, pane_id)
                }
                PendingHostEvent::TabMoved {
                    tab_id,
                    from_pane,
                    to_pane,
                } => tab::emit_moved(mgr, tab_id, from_pane, to_pane),
                PendingHostEvent::PaneCreated {
                    pane_id,
                    workspace_id,
                } => pane::emit_created(mgr, lua, af!(), pane_id, workspace_id),
                PendingHostEvent::PaneClosed { pane_id } => {
                    pane::emit_closed(mgr, lua, af!(), pane_id)
                }
                PendingHostEvent::PaneSplit {
                    original_pane,
                    new_pane,
                    direction,
                } => pane::emit_split(mgr, original_pane, new_pane, direction),
                PendingHostEvent::ProcessExited { surface_id } => {
                    misc::emit_process_exited(mgr, surface_id)
                }
                PendingHostEvent::NotificationCreated {
                    id,
                    level,
                    title,
                    body,
                    source,
                } => misc::emit_notification_created(mgr, id, title, body, source, level),
                PendingHostEvent::HookFired {
                    hook_id,
                    event_kind,
                    surface_id,
                    exit_code: _,
                } => misc::emit_hook_fired(mgr, hook_id, event_kind, surface_id),
                PendingHostEvent::PluginLoaded { plugin_id, version } => {
                    misc::emit_plugin_loaded(mgr, plugin_id, version)
                }
                PendingHostEvent::PluginEnableToggled { plugin_id, enabled } => {
                    misc::emit_plugin_enable_toggled(mgr, plugin_id, enabled)
                }
                PendingHostEvent::PluginUnloaded { plugin_id, reason } => {
                    misc::emit_plugin_unloaded(mgr, plugin_id, reason)
                }
                PendingHostEvent::PluginError {
                    plugin_id,
                    error_kind,
                    message,
                } => misc::emit_plugin_error(mgr, plugin_id, error_kind, message),
                PendingHostEvent::PluginRegistryChanged {
                    plugin_id,
                    change_kind,
                    detail,
                } => misc::emit_plugin_registry_changed(mgr, plugin_id, change_kind, detail),
                PendingHostEvent::PluginSurfaceKindRegistered {
                    plugin_id,
                    kind,
                    rendering,
                } => misc::emit_plugin_surface_kind_registered(mgr, plugin_id, kind, rendering),
                PendingHostEvent::PluginWindowDeclared {
                    plugin_id,
                    window_id,
                } => misc::emit_plugin_window_declared(mgr, plugin_id, window_id),
                PendingHostEvent::SurfaceRefreshRequested {
                    target,
                    kind,
                    trigger,
                } => surface::emit_refresh_requested(mgr, target.surface_id(), kind, trigger),
            }
        }
    }
}

/// 창 하나의 포커스 변화를 감지해 plugin 이벤트 큐에 넣는다. 탭의 OSC 제목은 선택이 바뀔 때만 새 선택 surface의
/// 것으로 다시 읽는다. 키 포커스 진입(`surface.focused`)은 제목을 다시 읽지 않는다. 배경 탭의 포커스 변경은 여기서
/// 감지하지 않아 닫기·이동 경로가 직접 반영한다.
pub(crate) fn observe_window_focus(
    state: &mut crate::state::MainViewState,
    engine: &mut EngineMut<'_>,
) {
    if let Some(selected) = state.detect_selection_change(engine.core) {
        engine.refresh_tab_osc_title(selected);
    }
    let mut observed = state.detect_focus_change();
    observed.extend(state.detect_workspace_activation(engine.core));
    observed.extend(state.detect_tab_focus_change(engine.core));
    engine.runtime.pending_host_events.extend(observed);
}

/// 이벤트를 다른 창의 이벤트와 합치기 전에 해당 engine으로 대기 작업을 완료한다.
/// 다른 engine을 넘기면 대기자를 깨울 waker hub가 달라진다.
fn resolve_hook_fired_task_waits(
    core: &crate::app::services::AppServices,
    engine: &EngineRef<'_>,
    events: &[PendingHostEvent],
) {
    for ev in events {
        if let PendingHostEvent::HookFired {
            hook_id, exit_code, ..
        } = ev
        {
            core.tasks.resolve_hook_task_wait(
                engine.task_scope,
                *hook_id,
                *exit_code,
                core.now_unix_millis() as u64,
            );
        }
    }
}

#[cfg(test)]
mod terminal_generation_tests {
    use super::*;

    #[test]
    fn the_real_host_queue_drops_only_the_old_connection_title() {
        let (_state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let surface = engine
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let old = engine.runtime.terminals.generation(surface).unwrap();
        drop(engine.take_pending_host_events());
        engine.enqueue_host_event(PendingHostEvent::SurfaceTitleChanged {
            surface_id: surface,
            generation: old,
            title: "stale-title".into(),
        });
        engine.runtime.terminals.replace(
            surface,
            tasty_terminal::Terminal::new_detached(80, 24),
            None,
        );
        let current = engine.runtime.terminals.generation(surface).unwrap();
        engine.enqueue_host_event(PendingHostEvent::SurfaceTitleChanged {
            surface_id: surface,
            generation: current,
            title: "current-title".into(),
        });
        engine.enqueue_host_event(PendingHostEvent::SurfaceFocused {
            surface_id: surface,
            prev_surface_id: None,
        });
        let events = take_current_host_events(&mut engine);
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], PendingHostEvent::SurfaceTitleChanged { title, .. } if title == "current-title")
        );
        assert!(matches!(events[1], PendingHostEvent::SurfaceFocused { .. }));
        assert!(engine.take_pending_host_events().is_empty());
    }
}
