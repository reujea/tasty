//! 명령 팔레트의 플러그인 명령을 처리한다. surface 대상은 없다.
//! action이 있으면 호스트가 실행하고 command.invoked도 알린다.
//! surface_id가 필수인 command.invoke IPC는 사용하지 않는다.

use winit::window::WindowId;

use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::plugin;

impl App {
    pub(crate) fn dispatch_pending_palette_plugin_commands(&mut self) {
        // 창 상태의 빌림을 끝낸 뒤 플러그인 매니저를 사용한다.
        let mut drained: Vec<(WindowId, String, String)> = Vec::new();
        for (wid, w) in self.view.views.iter_mut() {
            if let Some(main) = w.as_main_mut() {
                for (plugin_id, command_id) in main.state.pending_plugin_command_invokes.drain(..) {
                    drained.push((*wid, plugin_id, command_id));
                }
            }
        }
        if drained.is_empty() {
            return;
        }

        for (wid, plugin_id, command_id) in drained {
            let action = self
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.command_registry.find(&plugin_id, &command_id))
                .and_then(|e| e.action.clone());

            if let Some(action) = action {
                // 팔레트 행은 실행할 수 없으면 실행하지 않는다. 큐에 남은 요청도 같은 판정으로 거른다.
                let blocked = engines_mut!(self)
                    .window_pair(wid)
                    .is_some_and(|(main, engine)| {
                        crate::adapters::ui::tools_menu::tool_action_blocked(
                            &main.state,
                            &engine.read(),
                            &action,
                        )
                    });
                if blocked {
                    tracing::warn!(
                        "plugin command '{plugin_id}/{command_id}' not run: its popup needs a surface target and no surface is focused"
                    );
                    continue;
                }
                if let Some(mgr) = self.plugin_manager.as_mut() {
                    crate::plugin_bridge::key_dispatch::emit_command_invoked(
                        mgr,
                        &plugin_id,
                        &command_id,
                        None,
                    );
                }
                let item = plugin::tool_registry::ToolItem {
                    source: plugin::tool_registry::ToolSource::Plugin {
                        plugin_id: plugin_id.clone(),
                        tool_id: command_id.clone(),
                    },
                    key: format!("{plugin_id}/{command_id}"),
                    label_i18n_key: String::new(),
                    icon: None,
                    action,
                    order_hint: 0,
                };
                if let Some((main, engine)) = engines_mut!(self).window_pair(wid)
                    && let Err(blocked) = crate::adapters::ui::tools_menu::invoke_tool(
                        &mut main.state,
                        &engine.read(),
                        &item,
                    )
                {
                    tracing::warn!(
                        "plugin command '{}' not run: {}",
                        item.key,
                        blocked.message()
                    );
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
