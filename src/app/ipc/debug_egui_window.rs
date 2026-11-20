//! `debug.inject_egui_*`의 `window_id` 지정 — 지목한 창의 egui 입력 큐에 이벤트를 넣는다.
//! 설정처럼 별도 winit 창으로 뜨는 화면의 입력은 포커스된 메인 창 경로로 닿지 않는다.
//! 사용자 입력 재현이므로 release에는 두지 않는다.

use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};
use crate::view::main::debug_input::{ScrollUnit, push_egui_pointer, push_egui_text};
use crate::view::ui::View;

/// 창을 지정해 넣을 egui 입력 종류. 메서드 이름 분기는 라우터(`debug_methods.rs`)가 맡는다.
pub(super) enum EguiInjection {
    Text,
    Key,
    Pointer,
}

impl App {
    pub(super) fn ipc_handle_debug_egui_to_window(
        &mut self,
        cmd: &IpcCommand,
        kind: EguiInjection,
    ) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let params = &cmd.request.params;
        let result = match kind {
            EguiInjection::Text => self.debug_egui_text_to_window(params),
            EguiInjection::Key => self.debug_egui_key_to_window(params),
            EguiInjection::Pointer => self.debug_egui_mouse_to_window(params),
        };
        let response = match result {
            Ok(body) => JsonRpcResponse::success(id, body),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    fn debug_egui_text_to_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let Some(text) = params.get("text").and_then(|v| v.as_str()) else {
            return Err((-32602, "missing or non-string 'text'".to_string()));
        };
        let (wid, view) = self.debug_egui_target(params)?;
        let injected = push_egui_text(&mut view.base_mut().gpu, text);
        if injected {
            view.mark_dirty();
        }
        Ok(serde_json::json!({ "injected": injected, "window_id": wid }))
    }

    /// 처리는 focused window 경로와 같고(`debug_egui_key.rs`), 응답에 `window_id`를 더한다.
    /// 유지 수식 키는 창마다 따로다.
    fn debug_egui_key_to_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let (wid, view) = self.debug_egui_target(params)?;
        let mut body = super::debug_egui_key::egui_key_body(&mut view.base_mut().gpu, params)
            .map_err(|m| (-32602, m))?;
        if body["injected"] == true {
            view.mark_dirty();
        }
        body["window_id"] = serde_json::json!(wid);
        Ok(body)
    }

    /// 좌표는 그 창 전체에 대한 정규화 좌표(fx, fy)만 받는다. 보조 창에는 surface가 없으므로
    /// `surface_id`와 함께 주면 거절한다. 단위·버튼·수식 키는 focused window 경로와 같다.
    fn debug_egui_mouse_to_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        if params.get("surface_id").is_some_and(|v| !v.is_null()) {
            return Err((
                -32602,
                "'surface_id' cannot be combined with 'window_id'; give fx/fy relative to the window"
                    .to_string(),
            ));
        }
        let unit_name = params
            .get("unit")
            .and_then(|v| v.as_str())
            .unwrap_or("point");
        let Some(unit) = ScrollUnit::from_name(unit_name) else {
            return Err((
                -32602,
                "unknown scroll unit: expected \"line\", \"point\" or \"page\"".to_string(),
            ));
        };
        let (fx, fy, action) =
            super::window_required::read_pointer_params(params, unit).map_err(|m| (-32602, m))?;
        let modifiers =
            super::window_required::read_egui_modifiers(params).map_err(|m| (-32602, m))?;
        let (wid, view) = self.debug_egui_target(params)?;
        // 메인 창이면 focused window 경로처럼 이전 메뉴 포획 결과를 지운다.
        if let Some(main) = view.as_main_mut() {
            main.debug_captured_menu = None;
        }
        push_egui_pointer(&mut view.base_mut().gpu, fx, fy, action, modifiers);
        view.mark_dirty();
        Ok(serde_json::json!({ "injected": true, "window_id": wid }))
    }

    /// `window_id` 가 메인 창이면 그 창과 ID. egui 주입 라우터가 메인 창을 ID 생략 경로와 같은 진입으로 보낸다.
    pub(super) fn debug_main_window_by_id(
        &self,
        params: &serde_json::Value,
    ) -> Option<(winit::window::WindowId, u64)> {
        let wid = params::read_int::<u64>(params, "window_id").ok()??;
        self.view
            .views
            .iter()
            .find(|(w, view)| u64::from(**w) == wid && view.as_main().is_some())
            .map(|(w, _)| (*w, wid))
    }

    /// 창은 ID로만 고른다. 포커스된 창으로 대신하지 않는다.
    pub(super) fn debug_egui_target(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<(u64, &mut dyn View), (i32, String)> {
        let Some(wid) =
            params::read_int::<u64>(params, "window_id").map_err(|msg| (-32602, msg))?
        else {
            return Err((-32602, "'window_id' must be a window id".to_string()));
        };
        self.view
            .views
            .iter_mut()
            .find(|(w, _)| u64::from(**w) == wid)
            .map(|(_, view)| (wid, view.as_mut()))
            .ok_or_else(|| (-32602, format!("Window id {wid} not found")))
    }
}
