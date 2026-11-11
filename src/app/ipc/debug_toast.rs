//! `debug.toast` — 범위와 대상을 지정해 토스트를 띄운다.
//! 토스트는 사용자 행동의 결과에만 뜨므로 release에는 두지 않는다. 격리 인스턴스에서 위치를 실측하는 진입점이다.

use crate::adapters::ipc::handler::params;
use crate::adapters::ui::{ToastKind, ToastScope};
use crate::app::App;
use crate::app::auxiliary_toasts::auxiliary_toasts;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};
use crate::view::ui::View as _;

/// 요청한 범위. 메인 창이 아닌 창(Settings·Preset·Plugins)은 창 범위만 그린다.
enum RequestedScope {
    Window,
    Workspace(u32),
    Pane(u32),
    Surface(u32),
}

struct ToastRequest {
    scope: RequestedScope,
    message: String,
    kind: ToastKind,
    hint: Vec<String>,
}

impl App {
    pub(super) fn ipc_handle_debug_toast(&mut self, cmd: &IpcCommand) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let response = match self.debug_toast(&cmd.request.params) {
            Ok(body) => JsonRpcResponse::success(id, body),
            Err((code, msg)) => JsonRpcResponse::error(id, code, msg),
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    fn debug_toast(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let request = parse_request(params)?;
        let target = self.pick_toast_window(params)?;
        let window_id = u64::from(target);

        // 메인 창은 대상 ID를 그 창의 engine에서 확인한 뒤 ToastScope로 바꾼다.
        if let Some((_, engine)) = self.engines().window_pair(target) {
            let scope = resolve_main_scope(&request.scope, engine.core)?;
            let Some(main) = self
                .view
                .views
                .get_mut(&target)
                .and_then(|w| w.as_main_mut())
            else {
                return Err((
                    -32603,
                    "Main window disappeared while resolving".to_string(),
                ));
            };
            main.state
                .toasts
                .push_with_hint(request.message, request.kind, request.hint, scope);
            main.mark_dirty();
            return Ok(serde_json::json!({ "window_id": window_id, "view": "main" }));
        }

        if !matches!(request.scope, RequestedScope::Window) {
            return Err((
                -32602,
                "Only scope 'window' is available in a window that is not a main window"
                    .to_string(),
            ));
        }
        let Some(view) = self.view.views.get_mut(&target) else {
            return Err((-32602, format!("Window id {window_id} not found")));
        };
        let (kind_name, toasts) = auxiliary_toasts(view.as_any_mut())
            .ok_or_else(|| (-32602, format!("Window id {window_id} has no toast stack")))?;
        toasts.push_with_hint(
            request.message,
            request.kind,
            request.hint,
            ToastScope::Window,
        );
        view.mark_dirty();
        Ok(serde_json::json!({ "window_id": window_id, "view": kind_name }))
    }

    /// `window_id`가 있으면 그 창(메인·보조 모두), 없으면 메인 창이 하나일 때만 그 창이다.
    /// 포커스된 창으로 대신 고르지 않는다.
    fn pick_toast_window(
        &self,
        params: &serde_json::Value,
    ) -> Result<winit::window::WindowId, (i32, String)> {
        let requested =
            params::read_int::<u64>(params, "window_id").map_err(|msg| (-32602, msg))?;
        if let Some(wid) = requested {
            return self
                .view
                .views
                .keys()
                .copied()
                .find(|w| u64::from(*w) == wid)
                .ok_or_else(|| (-32602, format!("Window id {wid} not found")));
        }
        let mains: Vec<_> = self
            .view
            .views
            .iter()
            .filter(|(_, w)| w.as_main().is_some())
            .map(|(id, _)| *id)
            .collect();
        match mains.as_slice() {
            [only] => Ok(*only),
            [] => Err((-32000, "No main window open".to_string())),
            _ => Err((
                -32000,
                "Multiple windows open; specify 'window_id' (focus-independent). Use 'window.list' to enumerate.".to_string(),
            )),
        }
    }
}

fn parse_request(params: &serde_json::Value) -> Result<ToastRequest, (i32, String)> {
    let Some(message) = params.get("message").and_then(|v| v.as_str()) else {
        return Err((-32602, "Missing required 'message' parameter".to_string()));
    };
    let Some(scope_name) = params.get("scope").and_then(|v| v.as_str()) else {
        return Err((-32602, "Missing required 'scope' parameter".to_string()));
    };
    let target = params::read_int::<u32>(params, "target_id").map_err(|msg| (-32602, msg))?;
    let need_target =
        |name: &str| target.ok_or_else(|| (-32602, format!("scope '{name}' needs 'target_id'")));
    let scope = match scope_name {
        "window" => RequestedScope::Window,
        "workspace" => RequestedScope::Workspace(need_target("workspace")?),
        "pane" => RequestedScope::Pane(need_target("pane")?),
        "surface" => RequestedScope::Surface(need_target("surface")?),
        other => {
            return Err((
                -32602,
                format!("Unknown scope '{other}'. Known: window, workspace, pane, surface"),
            ));
        }
    };
    let kind = match params
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("info")
    {
        "info" => ToastKind::Info,
        "success" => ToastKind::Success,
        "warning" => ToastKind::Warning,
        "error" => ToastKind::Error,
        other => {
            return Err((
                -32602,
                format!("Unknown kind '{other}'. Known: info, success, warning, error"),
            ));
        }
    };
    let hint = match params.get("hint") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .map(|p| {
                p.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| (-32602, "'hint' must be an array of strings".to_string()))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err((-32602, "'hint' must be an array of strings".to_string())),
    };
    Ok(ToastRequest {
        scope,
        message: message.to_string(),
        kind,
        hint,
    })
}

/// 워크스페이스는 ID로 받아 표시 순번으로 바꾼다. ToastScope::Workspace가 순번을 쓰기 때문이다.
fn resolve_main_scope(
    scope: &RequestedScope,
    core: &crate::core::CoreState,
) -> Result<ToastScope, (i32, String)> {
    match *scope {
        RequestedScope::Window => Ok(ToastScope::Window),
        RequestedScope::Workspace(ws_id) => core
            .workspaces()
            .iter()
            .position(|ws| ws.id == ws_id)
            .map(ToastScope::Workspace)
            .ok_or_else(|| {
                (
                    -32602,
                    format!("Workspace id {ws_id} not found in this window"),
                )
            }),
        RequestedScope::Pane(pane_id) => core
            .find_pane_by_id(pane_id)
            .map(|_| ToastScope::Pane(pane_id))
            .ok_or_else(|| {
                (
                    -32602,
                    format!("Pane id {pane_id} not found in this window"),
                )
            }),
        RequestedScope::Surface(surface_id) => {
            if core.has_surface(surface_id) {
                Ok(ToastScope::Surface(surface_id))
            } else {
                Err((
                    -32602,
                    format!("Surface id {surface_id} not found in this window"),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(params: serde_json::Value) -> String {
        match parse_request(&params) {
            Ok(_) => panic!("{params} should be rejected"),
            Err((code, msg)) => {
                assert_eq!(code, -32602, "{msg}");
                msg
            }
        }
    }

    #[test]
    fn window_scope_needs_no_target_and_defaults_to_info() {
        let req = parse_request(&serde_json::json!({ "message": "Saved", "scope": "window" }))
            .expect("window scope parses");
        assert!(matches!(req.scope, RequestedScope::Window));
        assert_eq!(req.kind, ToastKind::Info);
        assert!(req.hint.is_empty());
    }

    #[test]
    fn non_window_scopes_need_a_target() {
        for scope in ["workspace", "pane", "surface"] {
            let msg = err(serde_json::json!({ "message": "m", "scope": scope }));
            assert!(msg.contains("target_id"), "{scope}: {msg}");
        }
        let req =
            parse_request(&serde_json::json!({ "message": "m", "scope": "pane", "target_id": 7 }))
                .expect("pane with target parses");
        assert!(matches!(req.scope, RequestedScope::Pane(7)));
    }

    #[test]
    fn unknown_names_are_refused_not_defaulted() {
        assert!(
            err(serde_json::json!({ "message": "m", "scope": "tab" })).contains("Unknown scope")
        );
        assert!(
            err(serde_json::json!({ "message": "m", "scope": "window", "kind": "fatal" }))
                .contains("Unknown kind")
        );
        assert!(err(serde_json::json!({ "scope": "window" })).contains("message"));
        assert!(
            err(serde_json::json!({ "message": "m", "scope": "window", "hint": [1] }))
                .contains("hint")
        );
    }
}
