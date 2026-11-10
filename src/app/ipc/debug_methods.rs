//! debug 빌드의 App 단위 조작·조회 요청을 처리한다.

use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc as host_ipc;
use crate::ipc::handler::debug_plugin;
use crate::ipc::server::{IpcCommand, send_response};

impl App {
    pub(crate) fn ipc_step_debug(&mut self, cmd: &IpcCommand) -> IpcStep {
        // 설정 창은 별도 winit 창이라 열기·탭 전환·닫기를 App에서 처리한다.
        #[cfg(feature = "gui")]
        match cmd.request.method.as_str() {
            "debug.settings.open" => return self.ipc_handle_debug_settings_open(cmd),
            "debug.settings.close" => return self.ipc_handle_debug_settings_close(cmd),
            _ => {}
        }
        // 창을 지정한 egui 입력 주입은 설정 같은 보조 창까지 가리킬 수 있어 App에서 처리한다.
        #[cfg(feature = "gui")]
        if cmd.request.params.get("window_id").is_some() {
            use super::debug_egui_window::EguiInjection;
            let kind = match cmd.request.method.as_str() {
                "debug.inject_egui_text" => Some(EguiInjection::Text),
                "debug.inject_egui_key" => Some(EguiInjection::Key),
                "debug.inject_egui_mouse" => Some(EguiInjection::Pointer),
                _ => None,
            };
            if let Some(kind) = kind {
                return self.ipc_handle_debug_egui_to_window(cmd, kind);
            }
        }
        // 보조 창까지 나열하려면 App 의 창 목록이 필요하다.
        #[cfg(feature = "gui")]
        if cmd.request.method == "debug.window.list" {
            return self.ipc_handle_debug_window_list(cmd);
        }
        // 토스트 스택은 창마다 따로 있어 보조 창까지 볼 수 있는 App에서 처리한다.
        #[cfg(feature = "gui")]
        if cmd.request.method == "debug.toast" {
            return self.ipc_handle_debug_toast(cmd);
        }
        // 다운로드 대기 상태는 App 의 attach 세션과 함께 움직인다.
        #[cfg(feature = "gui")]
        if cmd.request.method.starts_with("debug.remote_download.") {
            return self.ipc_handle_debug_remote_download(cmd);
        }
        // 설정에 한정하지 않고 현재 활성 모달을 닫는다.
        #[cfg(feature = "gui")]
        if cmd.request.method == "debug.modal.close_request" {
            return self.ipc_handle_debug_modal_close_request(cmd);
        }
        // 임의 Lua 실행은 debug 전용이며 헤드리스와 같은 함수를 사용한다.
        if cmd.request.method == "debug.lua.eval" {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response =
                crate::app::services::lua_eval(self.lua_engine.as_ref(), id, &cmd.request.params);
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        if cmd.request.method.starts_with("debug.event_bus.") {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response = debug_plugin::handle_event_bus(
                self.plugin_manager.as_mut(),
                &cmd.request.method,
                &cmd.request.params,
                id,
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        if cmd.request.method == "debug.extension.invoke_hook" {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            debug_plugin::handle_extension_invoke_hook(
                self.plugin_manager.as_mut(),
                &cmd.request.params,
                id,
                cmd.response_tx.clone(),
            );
            return IpcStep::Handled;
        }
        if cmd.request.method.starts_with("debug.popup.") {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response = match cmd.request.method.as_str() {
                "debug.popup.list" => {
                    host_ipc::handler::popup::handle_list(self.plugin_manager.as_ref(), id)
                }
                "debug.popup.open" => host_ipc::handler::popup::handle_open(
                    self.plugin_manager.as_mut(),
                    id,
                    &cmd.request.params,
                ),
                // 공통 닫기 큐를 거쳐 자식 파일 피커도 정리한다.
                "debug.popup.close" => {
                    match params::read_int::<u64>(&cmd.request.params, "instance_id") {
                        Err(msg) => host_ipc::protocol::JsonRpcResponse::invalid_params(id, &msg),
                        Ok(None) => host_ipc::protocol::JsonRpcResponse::invalid_params(
                            id,
                            "Missing required 'instance_id' parameter",
                        ),
                        Ok(Some(instance_id)) if self.plugin_manager.is_none() => {
                            host_ipc::protocol::JsonRpcResponse::error(
                                id,
                                -32002,
                                format!("plugin manager not initialized (instance {instance_id})"),
                            )
                        }
                        Ok(Some(instance_id)) => {
                            self.enqueue_plugin_popup_close(
                                instance_id,
                                tasty_plugin_protocol::PopupCloseReason::PluginRequest,
                            );
                            host_ipc::protocol::JsonRpcResponse::success(
                                id,
                                serde_json::json!({ "closed": instance_id }),
                            )
                        }
                    }
                }
                other => host_ipc::protocol::JsonRpcResponse::method_not_found(id, other),
            };
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // 배너는 매니저와 소유 창 상태를 함께 갱신한다.
        #[cfg(feature = "gui")]
        if cmd.request.method.starts_with("debug.plugin_banner.") {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let params = &cmd.request.params;
            let response = match cmd.request.method.as_str() {
                "debug.plugin_banner.open" => {
                    // 범위 밖 값을 잘라 다른 surface ID로 바꾸지 않는다.
                    let sid = match params::read_u32(params, "surface_id") {
                        Ok(v) => v,
                        Err(msg) => {
                            send_response(
                                &cmd.response_tx,
                                host_ipc::protocol::JsonRpcResponse::invalid_params(id, &msg),
                            );
                            return IpcStep::Handled;
                        }
                    };
                    match (params.get("banner_id").and_then(|v| v.as_str()), sid) {
                        (Some(bid), Some(sid)) => {
                            let bid = bid.to_string();
                            // debug 요청은 소유자 검사를 생략하되 실제 소유 플러그인으로 연다.
                            match self.open_plugin_banner(None, &bid, sid) {
                                Ok(iid) => host_ipc::protocol::JsonRpcResponse::success(
                                    id,
                                    serde_json::json!({ "instance_id": iid }),
                                ),
                                Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32602, e),
                            }
                        }
                        _ => host_ipc::protocol::JsonRpcResponse::invalid_params(
                            id,
                            "Missing 'banner_id' or 'surface_id'",
                        ),
                    }
                }
                "debug.plugin_banner.close" => match params::read_int::<u64>(params, "instance_id")
                {
                    Err(msg) => host_ipc::protocol::JsonRpcResponse::invalid_params(id, &msg),
                    Ok(Some(iid)) => {
                        let closed = self.close_plugin_banner(
                            iid,
                            tasty_plugin_protocol::BannerCloseReason::PluginRequest,
                        );
                        host_ipc::protocol::JsonRpcResponse::success(
                            id,
                            serde_json::json!({ "closed": closed }),
                        )
                    }
                    Ok(None) => host_ipc::protocol::JsonRpcResponse::invalid_params(
                        id,
                        "Missing 'instance_id'",
                    ),
                },
                other => host_ipc::protocol::JsonRpcResponse::method_not_found(id, other),
            };
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // 전체화면 무대는 창별 상태라 모든 창을 볼 수 있는 App에서 처리한다.
        #[cfg(feature = "gui")]
        if cmd.request.method.starts_with("debug.fullscreen.") {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response = self.ipc_debug_fullscreen(&cmd.request.method, &cmd.request.params, id);
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        IpcStep::NotHandled
    }
}

/// 전체화면 무대는 MainView만 대상으로 한다. ID가 없으면 MainView 하나일 때만 선택한다.
#[cfg(feature = "gui")]
impl App {
    fn ipc_debug_fullscreen(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        id: serde_json::Value,
    ) -> host_ipc::protocol::JsonRpcResponse {
        match method {
            "debug.fullscreen.list" => crate::app::services::fullscreen_list(id),
            "debug.fullscreen.open" => self.debug_fullscreen_open(params, id),
            "debug.fullscreen.close" => self.debug_fullscreen_close(params, id),
            "debug.fullscreen.state" => self.debug_fullscreen_state(params, id),
            other => host_ipc::protocol::JsonRpcResponse::method_not_found(id, other),
        }
    }

    fn debug_fullscreen_open(
        &mut self,
        params: &serde_json::Value,
        id: serde_json::Value,
    ) -> host_ipc::protocol::JsonRpcResponse {
        let Some(stage_id) = params.get("stage_id").and_then(|v| v.as_str()) else {
            return host_ipc::protocol::JsonRpcResponse::invalid_params(id, "Missing 'stage_id'");
        };
        // 잘못된 무대 ID를 열기 성공처럼 처리하지 않는다.
        if crate::fullscreen_stages::find(stage_id).is_none() {
            let known: Vec<&str> = crate::fullscreen_stages::all_metas()
                .iter()
                .map(|m| m.id)
                .collect();
            return host_ipc::protocol::JsonRpcResponse::invalid_params(
                id,
                format!(
                    "Unknown stage id '{stage_id}'. Known: {}. Use 'debug.fullscreen.list'.",
                    known.join(", ")
                ),
            );
        }
        let target = match self.pick_debug_window(params) {
            Ok(t) => t,
            Err((code, msg)) => {
                return host_ipc::protocol::JsonRpcResponse::error(id, code, msg);
            }
        };
        let Some(main) = self
            .view
            .views
            .get_mut(&target)
            .and_then(|w| w.as_main_mut())
        else {
            return host_ipc::protocol::JsonRpcResponse::error(
                id,
                -32602,
                "Target window is not a main view",
            );
        };
        let previous = main.state.fullscreen_stage_id();
        if !main.state.open_fullscreen_stage(stage_id) {
            return host_ipc::protocol::JsonRpcResponse::error(
                id,
                -32603,
                format!("Failed to open stage '{stage_id}'"),
            );
        }
        let opened = main.state.fullscreen_stage_id();
        // 일반 라우터를 거치지 않아 무대와 OS 전체화면 동기화에 필요한 repaint를 직접 요청한다.
        main.base.state.dirty = true;
        main.base.winit.request_redraw();
        host_ipc::protocol::JsonRpcResponse::success(
            id,
            serde_json::json!({
                "window_id": u64::from(target),
                "stage_id": opened,
                "previous_stage_id": previous,
                "replaced": previous.is_some_and(|p| Some(p) != opened),
            }),
        )
    }

    /// OS 창 관리자 없이도 활성 모달 닫기를 재현하도록 close_active_modal을 호출한다.
    /// SettingsView의 CloseRequested 이벤트 자체를 주입하는 경로는 아니다.
    #[cfg(feature = "gui")]
    fn ipc_handle_debug_modal_close_request(&mut self, cmd: &IpcCommand) -> IpcStep {
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let was_open = self.view.active_modal_id().is_some();
        if was_open {
            self.close_active_modal();
        }
        send_response(
            &cmd.response_tx,
            host_ipc::protocol::JsonRpcResponse::success(
                response_id,
                serde_json::json!({"closed": was_open}),
            ),
        );
        IpcStep::Handled
    }

    fn debug_fullscreen_close(
        &mut self,
        params: &serde_json::Value,
        id: serde_json::Value,
    ) -> host_ipc::protocol::JsonRpcResponse {
        let target = match self.pick_debug_window(params) {
            Ok(t) => t,
            Err((code, msg)) => {
                return host_ipc::protocol::JsonRpcResponse::error(id, code, msg);
            }
        };
        let Some(main) = self
            .view
            .views
            .get_mut(&target)
            .and_then(|w| w.as_main_mut())
        else {
            return host_ipc::protocol::JsonRpcResponse::error(
                id,
                -32602,
                "Target window is not a main view",
            );
        };
        let previous = main.state.fullscreen_stage_id();
        let closed = main.state.close_fullscreen_stage();
        main.base.state.dirty = true;
        main.base.winit.request_redraw();
        host_ipc::protocol::JsonRpcResponse::success(
            id,
            serde_json::json!({
                "window_id": u64::from(target),
                "closed": closed,
                "stage_id": previous,
            }),
        )
    }

    fn debug_fullscreen_state(
        &mut self,
        params: &serde_json::Value,
        id: serde_json::Value,
    ) -> host_ipc::protocol::JsonRpcResponse {
        let target = match self.pick_debug_window(params) {
            Ok(t) => t,
            Err((code, msg)) => {
                return host_ipc::protocol::JsonRpcResponse::error(id, code, msg);
            }
        };
        let Some(main) = self.view.views.get(&target).and_then(|w| w.as_main()) else {
            return host_ipc::protocol::JsonRpcResponse::error(
                id,
                -32602,
                "Target window is not a main view",
            );
        };
        let report = main.fullscreen_window_report();
        host_ipc::protocol::JsonRpcResponse::success(
            id,
            serde_json::json!({
                "window_id": u64::from(target),
                "stage_id": main.state.fullscreen_stage_id(),
                "stage_active": report.stage_active,
                "os_fullscreen": report.os_fullscreen,
                "maximized": report.maximized,
                "inner_size": { "width": report.inner_size.0, "height": report.inner_size.1 },
                "monitor": report.monitor.map(|m| serde_json::json!({
                    "name": m.name,
                    "position": { "x": m.position.0, "y": m.position.1 },
                    "size": { "width": m.size.0, "height": m.size.1 },
                    "scale_factor": m.scale_factor,
                })),
            }),
        )
    }

    /// MainView ID를 해석한다. 캡처와 달리 모달·preset은 대상이 아니다.
    /// ID가 없을 때 포커스를 쓰지 않고 MainView가 하나인 경우만 선택한다.
    fn pick_debug_window(
        &self,
        params: &serde_json::Value,
    ) -> Result<winit::window::WindowId, (i32, String)> {
        let requested =
            params::read_int::<u64>(params, "window_id").map_err(|msg| (-32602, msg))?;
        let mains: Vec<_> = self
            .view
            .views
            .iter()
            .filter(|(_, w)| w.as_main().is_some())
            .map(|(id, _)| *id)
            .collect();
        match requested {
            Some(wid) => mains
                .iter()
                .copied()
                .find(|w| u64::from(*w) == wid)
                .ok_or_else(|| (-32602, format!("Window id {wid} not found"))),
            None if mains.len() == 1 => Ok(mains[0]),
            None if mains.is_empty() => Err((-32000, "No main window open".to_string())),
            None => Err((
                -32000,
                "Multiple windows open; specify 'window_id' (focus-independent). Use 'window.list' to enumerate.".to_string(),
            )),
        }
    }
}

impl App {
    /// ui.state 응답의 모달 필드를 ViewRegistry의 활성 모달로 채운다.
    /// 응답을 만든 창·parked 상태와 관계없이 전역 원본 하나를 보고한다.
    pub(super) fn project_active_modal(
        &self,
        method: &str,
        mut response: host_ipc::protocol::JsonRpcResponse,
    ) -> host_ipc::protocol::JsonRpcResponse {
        if method == "ui.state" {
            fill_active_modal(self.view.active_modal(), &mut response);
        }
        response
    }
}

fn fill_active_modal(
    active: Option<crate::view::ActiveModal>,
    response: &mut host_ipc::protocol::JsonRpcResponse,
) {
    let Some(serde_json::Value::Object(fields)) = response.result.as_mut() else {
        return;
    };
    fields.insert("modal_open".into(), serde_json::json!(active.is_some()));
    fields.insert(
        "active_modal_id".into(),
        serde_json::json!(active.map(|m| u64::from(m.id))),
    );
    // 창 ID만으로는 설정 모달과 다른 모달을 구분할 수 없다.
    fields.insert(
        "active_modal_kind".into(),
        serde_json::json!(active.map(|m| m.kind.as_str())),
    );
}

#[cfg(test)]
mod tests {
    use super::fill_active_modal;
    use crate::ipc::protocol::JsonRpcResponse;
    use crate::view::{ActiveModal, ModalKind};
    use winit::window::WindowId;

    fn ui_state_without_modal() -> JsonRpcResponse {
        JsonRpcResponse::success(
            serde_json::json!(1),
            serde_json::json!({
                "modal_open": false,
                "active_modal_id": null,
                "active_modal_kind": null,
                "workspace_count": 1,
            }),
        )
    }

    #[test]
    fn an_open_modal_fills_all_three_fields() {
        let mut resp = ui_state_without_modal();
        let active = ActiveModal {
            id: WindowId::from(7u64),
            kind: ModalKind::Plugins,
        };
        fill_active_modal(Some(active), &mut resp);
        let result = resp.result.expect("성공 응답");
        assert_eq!(result["modal_open"], true);
        assert_eq!(result["active_modal_id"], 7);
        assert_eq!(result["active_modal_kind"], "plugins");
        assert_eq!(result["workspace_count"], 1);
    }

    #[test]
    fn no_modal_reports_closed_and_null() {
        let mut resp = ui_state_without_modal();
        fill_active_modal(None, &mut resp);
        let result = resp.result.expect("성공 응답");
        assert_eq!(result["modal_open"], false);
        assert!(result["active_modal_id"].is_null());
        assert!(result["active_modal_kind"].is_null());
    }
}
