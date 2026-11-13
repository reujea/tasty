//! plugin 페이지의 우클릭을 host 네이티브 메뉴로 연다(`webview.context_menu`).
//! 페이지는 OS 메뉴를 막고 우클릭 위치를 소유 plugin 에 넘기며, plugin 이 이 메서드로
//! 그 surface 의 메뉴를 요청한다. 메뉴 항목과 선택 뒤 처리는 `src/view/main/webview_menu.rs` 에 있다.

use serde_json::json;
use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;

/// 고른 항목을 소유 plugin 에만 보내는 이벤트. `surface_id`·`item`·`copied`·`x`·`y`(요청한 페이지 좌표)를 싣는다.
pub(crate) const SELECTED_EVENT: &str = "webview.context_menu_selected";

/// 같은 plugin 의 두 메뉴 요청 사이에 두는 최소 간격. 사람의 우클릭은 이보다 느리고,
/// 루프를 도는 plugin 은 여기서 걸린다.
pub(crate) const MIN_MENU_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// plugin 별 메뉴 요청 조절기. 진행 중 1건과 최소 간격을 함께 본다.
/// 상태는 plugin 단위라 한 plugin 이 여러 surface 로 번갈아 불러도 간격을 피하지 못한다.
#[derive(Default)]
pub(crate) struct WebviewMenuGate {
    /// plugin 별 마지막으로 **받아들인** 요청 시각.
    last_opened: std::collections::HashMap<String, std::time::Instant>,
    /// 메뉴를 띄워 둔 plugin — 닫힐 때까지 그 plugin 의 다음 요청을 받지 않는다.
    in_flight: Option<String>,
}

/// 요청을 거절한 이유. 호출부가 메시지를 만든다.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MenuRefusal {
    /// 이 plugin 의 메뉴가 아직 떠 있다.
    InFlight,
    /// 직전 요청과 너무 가깝다.
    TooSoon,
}

impl WebviewMenuGate {
    /// 요청을 받아들일지 정하고, 받아들이면 그 plugin 을 진행 중으로 표시한다.
    pub(crate) fn admit(
        &mut self,
        plugin_id: &str,
        now: std::time::Instant,
    ) -> Result<(), MenuRefusal> {
        if self.in_flight.as_deref() == Some(plugin_id) {
            return Err(MenuRefusal::InFlight);
        }
        if let Some(last) = self.last_opened.get(plugin_id)
            && now.duration_since(*last) < MIN_MENU_INTERVAL
        {
            return Err(MenuRefusal::TooSoon);
        }
        self.last_opened.insert(plugin_id.to_string(), now);
        self.in_flight = Some(plugin_id.to_string());
        Ok(())
    }

    /// 메뉴가 닫혔다(또는 요청이 버려졌다). 진행 중 표시만 지우고 간격은 유지한다.
    pub(crate) fn release(&mut self, plugin_id: &str) {
        if self.in_flight.as_deref() == Some(plugin_id) {
            self.in_flight = None;
        }
    }

    /// plugin 이 사라지면 그 자리를 비운다.
    pub(crate) fn forget(&mut self, plugin_id: &str) {
        self.last_opened.remove(plugin_id);
        self.release(plugin_id);
    }
}

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
        // 줌은 kind 단위 설정이라 surface kind 로 푼다. 좌표 변환 전에 읽어 둔다.
        let zoom = self.webview_zoom_factor(req.surface_id);
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
        // 배경 창 위에 메뉴를 띄우지 않는다. 페이지가 포커스를 쥐면 winit 은 부모 창에
        // Focused(false) 를 보내므로 webview 자식의 포커스도 함께 본다.
        let webviews = &main.webviews;
        if !crate::view::main::redraw::host_window_has_os_focus(main.base.state.focused, || {
            webviews.values().any(|wv| wv.holds_keyboard_focus())
        }) {
            return Err("webview.context_menu: the window is not focused".to_string());
        }
        // 사용자 우클릭 여부는 host 가 볼 수 없다 — 페이지가 OS 메뉴를 막고 plugin 이 중계하기
        // 때문이다. 대신 plugin 별 진행 중 1건과 최소 간격으로 루프 호출을 막는다.
        match self
            .webview_menu_gate
            .admit(&owner, std::time::Instant::now())
        {
            Ok(()) => {}
            Err(MenuRefusal::InFlight) => {
                return Err(format!(
                    "webview.context_menu: plugin '{owner}' already has a menu open"
                ));
            }
            Err(MenuRefusal::TooSoon) => {
                return Err(format!(
                    "webview.context_menu: plugin '{owner}' asked again within {}ms",
                    MIN_MENU_INTERVAL.as_millis()
                ));
            }
        }
        let sf = main.base.gpu.scale_factor();
        let (x, y) = window_point(rect, sf, zoom, req.x, req.y);
        // 사용자가 같은 프레임에 탭·사이드바에서 우클릭해 둔 요청을 밀어내지 않는다.
        let Some(main) = self.view.views.get_mut(&wid).and_then(|w| w.as_main_mut()) else {
            self.webview_menu_gate.release(&owner);
            return Err("webview.context_menu: the surface has no window".to_string());
        };
        if main.state.dialogs.pending_native_menu.is_some() {
            self.webview_menu_gate.release(&owner);
            return Err(
                "webview.context_menu: another context menu request is already pending".to_string(),
            );
        }
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

    /// surface kind 에 적용 중인 webview 확대 배율(1.0 = 100%). webview 가 아니면 1.0.
    fn webview_zoom_factor(&self, surface_id: u32) -> f32 {
        let Some(engine) = self
            .engines()
            .windows()
            .find_map(|(_, e)| e.find_surface_by_id(surface_id).map(|s| (e, s.kind())))
        else {
            return 1.0;
        };
        let (engine, kind) = engine;
        let Some(source) =
            crate::runtime::surface_registry::webview_kind::webview_settings_source(kind)
        else {
            return 1.0;
        };
        let settings =
            crate::webview::HtmlWebViewSettings::resolve(Some(&source), &engine.runtime.settings);
        zoom_factor(settings.zoom_percent)
    }

    /// 메뉴가 닫혔다. 조절기를 풀어 그 plugin 의 다음 요청을 받고, 고른 것이 있으면 소유
    /// plugin 에만 보낸다. 고르지 않고 닫으면 `payload` 가 없어 발행하지 않는다.
    pub(crate) fn on_webview_menu_closed(
        &mut self,
        plugin_id: &str,
        payload: Option<serde_json::Value>,
    ) {
        self.webview_menu_gate.release(plugin_id);
        if let Some(payload) = payload
            && let Some(mgr) = self.plugin_manager.as_mut()
        {
            mgr.emit_host_event_to_plugin(
                plugin_id,
                SELECTED_EVENT,
                &payload,
                tasty_plugin_protocol::EventScope::System,
            );
        }
    }
}

/// 설정의 백분율을 배율로. 0 이하·비정상 값은 100% 로 본다(0 으로 나누지 않는다).
pub(crate) fn zoom_factor(zoom_percent: f64) -> f32 {
    let f = zoom_percent / 100.0;
    if f.is_finite() && f > 0.0 {
        f as f32
    } else {
        1.0
    }
}

/// webview 안의 점을 창의 논리 좌표로 옮긴다. 페이지 좌표는 CSS px 라 확대 배율만큼
/// 작으므로 먼저 `zoom` 을 곱해 논리 픽셀로 바꾼 뒤 webview 영역으로 자른다.
/// 페이지 밖 좌표는 webview 가장자리로 붙인다.
fn window_point(
    rect: crate::model::PhysicalRect,
    sf: f32,
    zoom: f32,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let r = rect.to_logical(sf);
    let x = (x * zoom).clamp(0.0, r.width.value());
    let y = (y * zoom).clamp(0.0, r.height.value());
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

    fn rect() -> PhysicalRect {
        PhysicalRect {
            x: PhysicalPx(200.0),
            y: PhysicalPx(100.0),
            width: PhysicalPx(400.0),
            height: PhysicalPx(300.0),
        }
    }

    #[test]
    fn a_page_point_moves_by_the_webview_origin_in_logical_pixels() {
        assert_eq!(window_point(rect(), 2.0, 1.0, 10.0, 5.0), (110.0, 55.0));
        // 페이지 밖 좌표는 webview 안으로 붙인다.
        assert_eq!(window_point(rect(), 2.0, 1.0, 999.0, -4.0), (300.0, 50.0));
    }

    /// 확대가 100% 가 아니면 페이지 CSS px 는 논리 px 보다 작다 — 배율을 곱해야
    /// 클릭한 자리에 메뉴가 뜬다. 150% 에서 오른쪽 아래 우클릭이 오른쪽 아래로 가야 한다.
    #[test]
    fn a_zoomed_page_point_scales_before_it_is_clamped() {
        // 영역은 논리 200x150. 150% 에서 페이지의 (120, 90) 은 논리 (180, 135) 다.
        assert_eq!(window_point(rect(), 2.0, 1.5, 120.0, 90.0), (280.0, 185.0));
        // 배율을 무시하면 (220, 140) 으로 왼쪽 위에 치우친다.
        assert_ne!(window_point(rect(), 2.0, 1.5, 120.0, 90.0), (220.0, 140.0));
        // 50% 축소에서는 반대로 원점 쪽으로 온다 — 페이지 (120, 90) 은 논리 (60, 45) 다.
        assert_eq!(window_point(rect(), 2.0, 0.5, 120.0, 90.0), (160.0, 95.0));
        // clamp 는 배율을 곱한 뒤에 건다 — 배율 전에는 영역 안이던 점이 밖으로 나간다.
        // 페이지 (150, 120) 은 배율 없이는 논리 (150, 120) 으로 영역(200x150) 안이지만
        // 150% 에서는 (225, 180) 이라 오른쪽 아래 모서리로 붙는다.
        assert_eq!(window_point(rect(), 2.0, 1.5, 150.0, 120.0), (300.0, 200.0));
        assert_ne!(window_point(rect(), 2.0, 1.5, 150.0, 120.0), (250.0, 170.0));
    }

    #[test]
    fn a_zoom_percent_becomes_a_factor_and_bad_values_fall_back_to_one() {
        assert_eq!(zoom_factor(100.0), 1.0);
        assert_eq!(zoom_factor(150.0), 1.5);
        for bad in [0.0, -50.0, f64::NAN, f64::INFINITY] {
            assert_eq!(zoom_factor(bad), 1.0, "{bad}");
        }
    }

    /// 한 plugin 의 메뉴가 떠 있는 동안 그 plugin 의 다음 요청은 거절한다.
    #[test]
    fn a_plugin_gets_one_menu_at_a_time() {
        let mut gate = WebviewMenuGate::default();
        let t0 = std::time::Instant::now();
        assert_eq!(gate.admit("a", t0), Ok(()));
        assert_eq!(
            gate.admit("a", t0 + MIN_MENU_INTERVAL),
            Err(MenuRefusal::InFlight)
        );
        // 다른 plugin 은 막지 않는다.
        assert_eq!(gate.admit("b", t0), Ok(()));
    }

    /// 닫힌 뒤에도 최소 간격이 지나야 다시 연다 — 루프를 도는 plugin 이 여기서 걸린다.
    #[test]
    fn a_closed_menu_still_waits_out_the_minimum_interval() {
        let mut gate = WebviewMenuGate::default();
        let t0 = std::time::Instant::now();
        assert_eq!(gate.admit("a", t0), Ok(()));
        gate.release("a");
        assert_eq!(
            gate.admit(
                "a",
                t0 + MIN_MENU_INTERVAL - std::time::Duration::from_millis(1)
            ),
            Err(MenuRefusal::TooSoon)
        );
        assert_eq!(gate.admit("a", t0 + MIN_MENU_INTERVAL), Ok(()));
    }

    /// 고르지 않고 닫아도 풀어야 한다. 안 풀면 그 plugin 은 다시는 메뉴를 열지 못한다.
    #[test]
    fn a_dismissed_menu_releases_the_slot() {
        let mut gate = WebviewMenuGate::default();
        let t0 = std::time::Instant::now();
        assert_eq!(gate.admit("a", t0), Ok(()));
        gate.release("a");
        assert_eq!(gate.admit("a", t0 + MIN_MENU_INTERVAL * 2), Ok(()));
    }

    /// plugin 이 내려가면 간격도 진행 중 표시도 남기지 않는다.
    #[test]
    fn an_unloaded_plugin_leaves_no_trace_in_the_gate() {
        let mut gate = WebviewMenuGate::default();
        let t0 = std::time::Instant::now();
        assert_eq!(gate.admit("a", t0), Ok(()));
        gate.forget("a");
        assert_eq!(gate.admit("a", t0), Ok(()));
    }
}
