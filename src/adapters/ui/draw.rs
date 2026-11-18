//! UI 전체 진입점 — sidebar 를 그리고 남은 terminal 영역 PhysicalRect 를 반환.

use crate::intent::Intent;
use crate::model::PhysicalRect;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use tasty_type_geometry::length::{LogicalPx, PhysicalPx};

use super::sidebar;

/// Render the egui UI and return the remaining terminal area rect (in physical pixels).
pub fn draw_ui(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    scale_factor: f32,
    plugin_alert: usize,
) -> PhysicalRect {
    let sidebar_width = state.sidebar_width.value();

    if !state.sidebar_visible {
    } else if state.sidebar_collapsed {
        let r = sidebar::draw_collapsed_sidebar(ctx, state, engine, sidebar_width, plugin_alert);

        if r.expand_clicked {
            state.sidebar_collapsed = false;
        }
        if r.plugins_clicked {
            state.plugins_open = true;
        }
        if r.settings_clicked {
            state.settings_open_requested = true;
        }
        if let Some(btn_rect) = r.tools_rect {
            sidebar::tools::open_tools_menu(ctx, state, engine, btn_rect);
        }
        if let Some(i) = r.switch_ws {
            state.switch_workspace(engine, i);
        }
        if r.add_ws {
            state.dispatch_intent(
                Intent::NewWorkspace {
                    kind: None,
                    params: serde_json::Value::Null,
                    category: None,
                    cwd_source: state.focused_surface_id(engine),
                }
                .from_user_menu("sidebar_add_workspace"),
            );
        }
    } else {
        let r = sidebar::draw_full_sidebar(ctx, state, engine, sidebar_width, plugin_alert);

        if r.collapse_clicked {
            state.sidebar_collapsed = true;
        }
        if r.plugins_clicked {
            state.plugins_open = true;
        }
        if r.settings_clicked {
            state.settings_open_requested = true;
        }
        if let Some(btn_rect) = r.tools_rect {
            sidebar::tools::open_tools_menu(ctx, state, engine, btn_rect);
        }
    }

    let screen_rect = ctx.screen_rect();
    let top_inset = super::titlebar::top_inset(scale_factor);
    // egui의 논리 좌표를 to_physical에서 한 번만 변환한다.
    let terminal_x = LogicalPx(sidebar_width).to_physical(scale_factor);
    let terminal_y = top_inset;
    let terminal_width = LogicalPx(screen_rect.width() - sidebar_width).to_physical(scale_factor);
    let terminal_height = LogicalPx(screen_rect.height()).to_physical(scale_factor) - top_inset;

    PhysicalRect {
        x: terminal_x,
        y: terminal_y,
        width: PhysicalPx(terminal_width.value().max(1.0)),
        height: PhysicalPx(terminal_height.value().max(1.0)),
    }
}
