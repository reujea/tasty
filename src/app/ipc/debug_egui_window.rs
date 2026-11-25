//! `debug.inject_egui_*`의 `window_id` 지정 — 지목한 창의 egui 입력 큐에 이벤트를 넣는다.
//! 설정처럼 별도 winit 창으로 뜨는 화면의 입력은 포커스된 메인 창 경로로 닿지 않는다.
//! 사용자 입력 재현이므로 release에는 두지 않는다.

use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};
use crate::view::main::debug_input::{DebugInput, ScrollUnit};
use crate::view::ui::View;

/// `window_id` 를 지정한 포인터 주입에 `surface_id` 가 함께 왔다. 창 종류와 관계없이 `-32602` 로 거절한다.
pub(super) fn reject_surface_with_window(cmd: &IpcCommand) -> IpcStep {
    let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
    send_response(
        &cmd.response_tx,
        JsonRpcResponse::error(id, -32602, SURFACE_WITH_WINDOW),
    );
    IpcStep::Handled
}

/// `reject_surface_with_window` 의 오류 문구.
pub(super) const SURFACE_WITH_WINDOW: &str =
    "'surface_id' cannot be combined with 'window_id'; give fx/fy relative to the window";

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
        self.debug_input_to_window(params, DebugInput::Text(text.to_string()))
    }

    /// 지목한 창의 debug 주입 입구(`enter_debug_input`)로 넣고 응답 본문에 `window_id` 를 더한다.
    fn debug_input_to_window(
        &mut self,
        params: &serde_json::Value,
        input: DebugInput,
    ) -> Result<serde_json::Value, (i32, String)> {
        let (wid, window) = self.debug_egui_window(params)?;
        let entered = self
            .enter_debug_input(window, input)
            .ok_or_else(|| (-32602, format!("Window id {wid} not found")))?;
        let mut body = entered.body();
        body["window_id"] = serde_json::json!(wid);
        Ok(body)
    }

    /// `window_id` 가 가리키는 창의 ID. 포커스된 창으로 대신하지 않는다.
    pub(super) fn debug_egui_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<(u64, winit::window::WindowId), (i32, String)> {
        let (wid, _) = self.debug_egui_target(params)?;
        self.view
            .views
            .keys()
            .copied()
            .find(|w| u64::from(*w) == wid)
            .map(|w| (wid, w))
            .ok_or_else(|| (-32602, format!("Window id {wid} not found")))
    }

    /// 요청 읽기는 focused window 경로와 같고(`debug_egui_key.rs`), 응답에 `window_id`를 더한다. 유지 수식 키는
    /// 창마다 따로다. 팝업 관리자를 가진 창은 winit 키와 같은 사건 입구(보류 줄 → 끊기 → 적재 → 결속·bare Esc 적용)를
    /// 지난다(`window_input::enter_window_debug`).
    fn debug_egui_key_to_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let input = super::debug_egui_key::key_input(params).map_err(|m| (-32602, m))?;
        self.debug_input_to_window(params, input)
    }

    /// 좌표는 그 창 전체에 대한 정규화 좌표(fx, fy)만 받는다(`surface_id` 를 함께 준 요청은 라우터가 거절한다,
    /// `reject_surface_with_window`). 단위·버튼·수식 키는 focused window 경로와 같다. 메인 창은 라우터가 ID 생략
    /// 경로로 보내므로 여기에는 보조 창만 온다.
    fn debug_egui_mouse_to_window(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
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
        let input = DebugInput::Pointer {
            fx,
            fy,
            surface_id: None,
            action,
            modifiers,
        };
        self.debug_input_to_window(params, input)
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
