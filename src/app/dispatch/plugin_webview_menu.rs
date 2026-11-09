//! plugin 페이지의 우클릭을 host 네이티브 메뉴로 연다(`webview.context_menu`).
//! 페이지는 OS 메뉴를 막고 우클릭 위치를 소유 plugin 에 넘기며, plugin 이 이 메서드로
//! 그 surface 의 메뉴를 요청한다. 메뉴 항목과 선택 뒤 처리는 `src/view/main/webview_menu.rs` 에 있다.

use serde_json::json;
use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;

/// 고른 항목을 소유 plugin 에만 보내는 이벤트. `surface_id`·`item`·`copied`·`x`·`y`(요청한 페이지 좌표)를 싣는다.
pub(crate) const SELECTED_EVENT: &str = "webview.context_menu_selected";

/// 검사를 통과한 메뉴 요청. 좌표는 webview 왼쪽 위 기준 논리 픽셀이다.
#[derive(Debug, PartialEq)]
pub(crate) struct MenuRequest {
    pub(crate) surface_id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl App {
    /// 권한은 진입부 게이트에서 검사한다. 소유자가 아니거나 화면에 드러난 webview 가 아니면 거절한다.
    pub(super) fn handle_ipc_webview_context_menu(&mut self, call: &PendingPluginCall) {
        let (result, error, code) = match self.open_webview_context_menu(call) {
            Ok(()) => (Some(json!({ "opened": true })), None, None),
            Err(message) => (None, Some(message), Some(-32602)),
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_plugin_call_result(call, result, error, code);
        }
    }

    fn open_webview_context_menu(&mut self, call: &PendingPluginCall) -> Result<(), String> {
        let req = parse_request(&call.params)?;
        let mut found = None;
        for (wid, engine) in self.engines().windows() {
            if let Some(surface) = engine.find_surface_by_id(req.surface_id) {
                found = surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                    .map(|rs| (wid, rs.plugin_id.clone()));
                break;
            }
        }
        let Some((wid, owner)) = found else {
            return Err(format!(
                "webview.context_menu: surface {} is not a live plugin surface",
                req.surface_id
            ));
        };
        if owner != call.plugin_id {
            return Err(format!(
                "webview.context_menu: plugin '{}' does not own surface {} (owner '{owner}')",
                call.plugin_id, req.surface_id
            ));
        }
        let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut()) else {
            return Err("webview.context_menu: the surface has no window".to_string());
        };
        let Some(rect) = main
            .state
            .native_content_rects
            .iter()
            .find(|(sid, _)| *sid == req.surface_id)
            .map(|(_, r)| *r)
        else {
            return Err(format!(
                "webview.context_menu: surface {} is not a visible webview",
                req.surface_id
            ));
        };
        let sf = main.base.gpu.scale_factor();
        let (x, y) = window_point(rect, sf, req.x, req.y);
        main.state.dialogs.pending_native_menu =
            Some(crate::state::PendingNativeMenu::WebviewSurface {
                surface_id: req.surface_id,
                plugin_id: owner,
                page_x: req.x,
                page_y: req.y,
                x,
                y,
            });
        main.base.mark_dirty();
        Ok(())
    }
}

/// webview 안의 점(논리 픽셀)을 창의 논리 좌표로 옮긴다. 페이지 밖 좌표는 webview 가장자리로 붙인다.
fn window_point(rect: crate::model::PhysicalRect, sf: f32, x: f32, y: f32) -> (f32, f32) {
    let r = rect.to_logical(sf);
    let x = x.clamp(0.0, r.width.value());
    let y = y.clamp(0.0, r.height.value());
    (r.x.value() + x, r.y.value() + y)
}

pub(crate) fn parse_request(params: &serde_json::Value) -> Result<MenuRequest, String> {
    let surface_id = crate::adapters::ipc::handler::params::read_u32(params, "surface_id")
        .map_err(|m| format!("webview.context_menu: {m}"))?
        .ok_or("webview.context_menu: missing 'surface_id'")?;
    let coord = |key: &str| {
        params
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v as f32)
            .ok_or(format!("webview.context_menu: missing or invalid '{key}'"))
    };
    let (x, y) = (coord("x")?, coord("y")?);
    Ok(MenuRequest { surface_id, x, y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PhysicalPx, PhysicalRect};

    #[test]
    fn a_request_carries_the_surface_and_the_point() {
        let req = parse_request(&json!({ "surface_id": 7, "x": 10.5, "y": 20 })).expect("valid");
        assert_eq!(
            req,
            MenuRequest {
                surface_id: 7,
                x: 10.5,
                y: 20.0,
            }
        );
    }

    #[test]
    fn a_bad_request_is_refused() {
        for params in [
            json!({ "x": 0, "y": 0 }),
            json!({ "surface_id": 7, "y": 0 }),
            json!({ "surface_id": 7, "x": "1", "y": 0 }),
        ] {
            assert!(parse_request(&params).is_err(), "{params}");
        }
    }

    #[test]
    fn a_page_point_moves_by_the_webview_origin_in_logical_pixels() {
        let rect = PhysicalRect {
            x: PhysicalPx(200.0),
            y: PhysicalPx(100.0),
            width: PhysicalPx(400.0),
            height: PhysicalPx(300.0),
        };
        assert_eq!(window_point(rect, 2.0, 10.0, 5.0), (110.0, 55.0));
        // 페이지 밖 좌표는 webview 안으로 붙인다.
        assert_eq!(window_point(rect, 2.0, 999.0, -4.0), (300.0, 50.0));
    }
}
