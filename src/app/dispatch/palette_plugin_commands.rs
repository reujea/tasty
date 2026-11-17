//! 명령 팔레트의 플러그인 명령을 처리한다. 대상은 팔레트에서 명령을 고른 시점에 정한 값이다.
//! action이 있으면 호스트가 실행하고 command.invoked도 알린다. 대상이 생성 대기 초안이면 걸어 둔다.
//! surface_id가 필수인 command.invoke IPC는 사용하지 않는다.

use winit::window::WindowId;

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::state::attach_pending::deferred::{CommandSurfaceTarget, InvokedNotice};

impl App {
    pub(crate) fn dispatch_pending_palette_plugin_commands(&mut self) {
        // 창 상태의 빌림을 끝낸 뒤 플러그인 매니저를 사용한다.
        type Queued = (WindowId, String, String, Option<CommandSurfaceTarget>);
        let mut drained: Vec<Queued> = Vec::new();
        for (wid, w) in self.view.views.iter_mut() {
            if let Some(main) = w.as_main_mut() {
                for (plugin_id, command_id, target) in
                    main.state.pending_plugin_command_invokes.drain(..)
                {
                    drained.push((*wid, plugin_id, command_id, target));
                }
            }
        }
        if drained.is_empty() {
            return;
        }

        for (wid, plugin_id, command_id, target) in drained {
            let action = self
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.command_registry.find(&plugin_id, &command_id))
                .and_then(|e| e.action.clone());

            if let Some(action) = action {
                let item =
                    crate::adapters::ui::tools_menu::command_item(&plugin_id, &command_id, action);
                let mgr = self.plugin_manager.as_mut();
                if let Some((main, engine)) = engines_mut!(self).window_pair(wid) {
                    run_palette_command(&mut main.state, &engine.read(), &item, target, |n| {
                        if let Some(mgr) = mgr {
                            crate::plugin_bridge::key_dispatch::emit_command_invoked(
                                mgr,
                                &n.plugin_id,
                                &n.command_id,
                                n.surface_id,
                            );
                        }
                    });
                }
            } else if let Some(mgr) = self.plugin_manager.as_mut() {
                crate::plugin_bridge::key_dispatch::dispatch_plugin_command(
                    mgr,
                    &plugin_id,
                    &command_id,
                    None,
                );
            }
        }
    }
}

/// 팔레트 큐의 plugin 명령 하나를 고른 시점의 대상으로 실행한다. 바로 실행할 surface 가 그 사이
/// 사라졌으면 실행하지도 알리지도 않고 버린다. 실행할 수 없으면 알리지도 실행하지도 않는다.
pub(crate) fn run_palette_command(
    state: &mut crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    item: &crate::plugin::tool_registry::ToolItem,
    target: Option<CommandSurfaceTarget>,
    notify: impl FnOnce(&InvokedNotice),
) {
    if let Some(CommandSurfaceTarget::Surface(sid)) = target
        && !engine.has_surface(sid)
    {
        tracing::info!(
            "plugin command '{}' dropped: its surface {sid} is gone",
            item.key
        );
        return;
    }
    let crate::plugin::tool_registry::ToolSource::Plugin { plugin_id, tool_id } = &item.source;
    let invoked = InvokedNotice {
        plugin_id: plugin_id.clone(),
        command_id: tool_id.clone(),
        surface_id: None,
    };
    if let Err(blocked) = crate::adapters::ui::tools_menu::run_tool(
        state,
        engine,
        item,
        target,
        Some(invoked),
        notify,
    ) {
        tracing::warn!(
            "plugin command '{}' not run: {}",
            item.key,
            blocked.message()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::run_palette_command;
    use crate::state::attach_pending::deferred::CommandSurfaceTarget;

    fn item() -> crate::plugin::tool_registry::ToolItem {
        crate::adapters::ui::tools_menu::command_item(
            "com.example.viewer",
            "confirm",
            crate::plugin::manifest::ToolAction::OpenPopup {
                popup_id: "com.example.viewer/confirm".into(),
            },
        )
    }

    /// 큐에 담긴 대상으로 실행한다. 그 사이 포커스가 옮겨져도 고른 시점의 surface 다.
    #[test]
    fn a_queued_command_runs_on_its_queued_surface_and_notifies_once() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        let engine = session.borrow_mut();
        let read = engine.read();
        assert!(state.focus_surface_by_id(&read, 101));
        let mut notified = Vec::new();
        run_palette_command(
            &mut state,
            &read,
            &item(),
            Some(CommandSurfaceTarget::Surface(201)),
            |n| notified.push(n.command_id.clone()),
        );
        assert_eq!(notified, vec!["confirm".to_string()]);
        assert_eq!(state.pending_popup_opens.len(), 1);
        assert_eq!(state.pending_popup_opens[0].target_surface, Some(201));
    }

    /// 고른 surface 가 실행 전에 사라졌으면 열지도 알리지도 않는다.
    #[test]
    fn a_queued_command_whose_surface_is_gone_is_dropped() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        let engine = session.borrow_mut();
        let read = engine.read();
        let mut notified = 0;
        run_palette_command(
            &mut state,
            &read,
            &item(),
            Some(CommandSurfaceTarget::Surface(9_999)),
            |_| notified += 1,
        );
        assert_eq!(notified, 0);
        assert!(state.pending_popup_opens.is_empty());
    }
}
