//! Double-tap modifier (Shift+Shift / Ctrl+Ctrl / Alt+Alt) 단축키 처리.

use crate::intent::{Intent, UiIntent};
use crate::model::{PhysicalRect, SplitDirection};
use crate::runtime::engine_read::EngineRead;
use crate::view::main::MainView;

use super::{focused_workspace_category, send_app_event};

impl MainView {
    /// 마지막 workspace가 닫혔으면 창을 닫고, 남아 있으면 레이아웃을 다시 계산한다.
    fn finish_after_possible_close(
        &mut self,
        engine: &EngineRead<'_>,
        terminal_rect: PhysicalRect,
        cell_w: f32,
        cell_h: f32,
    ) {
        if engine.workspaces().is_empty() {
            self.request_close();
        } else {
            let scale_factor = self.base.gpu.scale_factor();
            self.state
                .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
        }
    }

    pub(crate) fn handle_double_tap_shortcut(
        &mut self,
        engine: &EngineRead<'_>,
        dt: crate::double_tap::DoubleTapKey,
    ) -> bool {
        let kb = engine.settings.keybindings.clone();
        let dt_str = dt.binding_str();

        let has_dt = |bindings: &[String]| bindings.iter().any(|b| b == dt_str);
        if has_dt(&kb.toggle_settings) {
            send_app_event(&self.proxy, crate::AppEvent::OpenSettings);
            return true;
        }
        if has_dt(&kb.toggle_notifications) {
            self.state.dispatch_intent(
                UiIntent::TogglePopup(crate::intent::OpenRequest::new(
                    "notifications",
                    crate::model::popup_kind::PopupScope::Window,
                ))
                .from_user_shortcut("toggle_notifications_double_tap"),
            );
            return true;
        }

        // 받는 칸의 집합은 설정 녹화와 같은 `DOUBLE_TAP_FIELDS` 하나다. 위의 두 동작은 먼저 처리했다.
        // 매칭뿐 아니라 실제 실행한 경우에만 입력을 소비한다.
        for action in crate::settings::KeybindingSettings::DOUBLE_TAP_FIELDS
            .iter()
            .filter(|f| !matches!(**f, "toggle_settings" | "toggle_notifications"))
        {
            if has_dt(kb.get_bindings(action).unwrap_or(&[]))
                && self.run_double_tap_action(engine, action)
            {
                return true;
            }
        }

        false
    }

    /// 매칭된 액션을 실행하고 처리 여부를 반환한다.
    fn run_double_tap_action(&mut self, engine: &EngineRead<'_>, action: &str) -> bool {
        if self.run_double_tap_layout_action(engine, action)
            || self.run_double_tap_focus_action(engine, action)
            || self.run_double_tap_open_action(engine, action)
            || self.run_double_tap_app_action(action)
        {
            return true;
        }
        tracing::warn!("double-tap: registered action '{action}' has no execution arm");
        false
    }

    fn run_double_tap_layout_action(&mut self, engine: &EngineRead<'_>, action: &str) -> bool {
        let terminal_rect = self.compute_terminal_rect();
        let cell_w = self.base.gpu.cell_width();
        let cell_h = self.base.gpu.cell_height();
        let scale_factor = self.base.gpu.scale_factor();

        match action {
            "new_workspace" => {
                let category = focused_workspace_category(&self.state, engine);
                let cwd_source = self.state.command_target(engine).cwd_source();
                self.state.dispatch_intent(
                    Intent::NewWorkspace {
                        kind: None,
                        params: serde_json::Value::Null,
                        category,
                        cwd_source,
                    }
                    .from_user_shortcut("new_workspace"),
                );
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "close_workspace" => {
                self.state.close_active_workspace(engine);
                self.finish_after_possible_close(engine, terminal_rect, cell_w, cell_h);
            }
            "new_tab" => {
                let pane_id = self.state.focused_pane_id(engine);
                let cwd = self.state.resolve_inherit_cwd(engine);
                let cwd_source = self.state.focused_surface_id(engine);
                self.state.dispatch_intent(
                    crate::app::command::DomainIntent::CreateTab {
                        pane_id,
                        cwd,
                        cwd_source,
                        kind: "terminal".into(),
                        name: None,
                        surface_params: serde_json::json!({}),
                        activate: true,
                    }
                    .from_user_shortcut("new_tab"),
                );
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "close_pane" => {
                let target = self.state.command_target(engine);
                super::close_pane_action(&mut self.state, engine, &target);
                self.finish_after_possible_close(engine, terminal_rect, cell_w, cell_h);
            }
            "split_pane_vertical" => {
                let target = self.state.command_target(engine);
                if let Some(intent) =
                    super::structure::split_pane(&target, SplitDirection::Vertical)
                {
                    self.state
                        .dispatch_intent(intent.from_user_shortcut("split_pane_vertical"));
                }
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_pane_horizontal" => {
                let target = self.state.command_target(engine);
                if let Some(intent) =
                    super::structure::split_pane(&target, SplitDirection::Horizontal)
                {
                    self.state
                        .dispatch_intent(intent.from_user_shortcut("split_pane_horizontal"));
                }
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_surface_vertical" => {
                let target = self.state.command_target(engine);
                if let Some(intent) =
                    super::structure::split_surface(&target, SplitDirection::Vertical)
                {
                    self.state
                        .dispatch_intent(intent.from_user_shortcut("split_surface_vertical"));
                }
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "split_surface_horizontal" => {
                let target = self.state.command_target(engine);
                if let Some(intent) =
                    super::structure::split_surface(&target, SplitDirection::Horizontal)
                {
                    self.state
                        .dispatch_intent(intent.from_user_shortcut("split_surface_horizontal"));
                }
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            "close_surface" => {
                let closed = self.state.close_active_surface(engine);
                if !closed && !self.state.close_active_pane(engine) {
                    self.state.close_active_workspace(engine);
                }
                self.finish_after_possible_close(engine, terminal_rect, cell_w, cell_h);
            }
            "close_active" => {
                let target = self.state.command_target(engine);
                super::close_active_action(&mut self.state, engine, &target);
                self.finish_after_possible_close(engine, terminal_rect, cell_w, cell_h);
            }
            "restore_closed" => {
                let target = self.state.command_target(engine);
                self.state.dispatch_intent(
                    super::structure::restore_closed(&target).from_user_shortcut("restore_closed"),
                );
                self.state
                    .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
            }
            _ => return false,
        }
        true
    }

    fn run_double_tap_focus_action(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        action: &str,
    ) -> bool {
        match action {
            "focus_pane_next" => {
                self.state.move_pane_focus_forward(engine);
            }
            "focus_pane_prev" => {
                self.state.move_pane_focus_backward(engine);
            }
            "focus_surface_next" => {
                self.state.move_surface_focus_forward(engine);
            }
            "focus_surface_prev" => {
                self.state.move_surface_focus_backward(engine);
            }
            "next_tab" => {
                self.state.next_tab_in_pane(engine);
            }
            "prev_tab" => {
                self.state.prev_tab_in_pane(engine);
            }
            _ => return false,
        }
        true
    }

    fn run_double_tap_open_action(&mut self, engine: &EngineRead<'_>, action: &str) -> bool {
        let target = self.state.command_target(engine);
        match action {
            "open_markdown" => {
                self.state
                    .enqueue_convert_input_popup(&engine.as_ref(), "markdown", None);
            }
            "open_explorer" => {
                let target = self.state.command_target(engine);
                Self::open_explorer_tab(&mut self.state, &target);
            }
            "convert_surface" => {
                if super::surface_actions::held_for_draft(
                    &mut self.state,
                    engine,
                    &target,
                    "convert_surface",
                ) {
                    return true;
                }
                if let Some(sid) = self.state.focused_surface_id(engine) {
                    super::surface_actions::open_convert_popup(
                        &mut self.state,
                        sid,
                        "convert_surface_double_tap",
                    );
                }
            }
            "convert_to_markdown" => {
                if super::surface_actions::held_for_draft(
                    &mut self.state,
                    engine,
                    &target,
                    "convert_to_markdown",
                ) {
                    return true;
                }
                if let Some(sid) = self.state.focused_surface_id(engine) {
                    self.state
                        .enqueue_convert_input_popup(&engine.as_ref(), "markdown", Some(sid));
                }
            }
            "convert_to_explorer" => {
                if super::surface_actions::held_for_draft(
                    &mut self.state,
                    engine,
                    &target,
                    "convert_to_explorer",
                ) {
                    return true;
                }
                if let Some(sid) = self.state.focused_surface_id(engine) {
                    super::surface_actions::convert_to_explorer(
                        &mut self.state,
                        sid,
                        "convert_to_explorer_double_tap",
                    );
                }
            }
            _ => return false,
        }
        true
    }

    fn run_double_tap_app_action(&mut self, action: &str) -> bool {
        match action {
            "quit" => {
                send_app_event(&self.proxy, crate::AppEvent::QuitRequested);
            }
            "quit_immediate" => {
                send_app_event(&self.proxy, crate::AppEvent::Shutdown);
            }
            "quit_minimize" => {
                send_app_event(&self.proxy, crate::AppEvent::Minimize);
            }
            _ => return false,
        }
        true
    }
}
