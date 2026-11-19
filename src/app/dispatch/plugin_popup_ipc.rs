//! plugin이 실행 중에 자기 팝업을 선언하고 여는 IPC(`popup.declare`, `popup.show`).
//!
//! 게이트와 권한은 진입부에서 끝났다. 여기서는 선언 검사, 대상 종류, 대상의 존재와 surface 소유를 확인한다.
//! 대상은 요청이 정한다. host가 대신 고르는 기본 대상은 없고, 대상이 없거나 사라졌으면 열지 않는다.
//! `popup.show`는 인스턴스를 만든 뒤 `popup.open`을 보내고 응답한다. plugin이 이 호출을
//! 요청 처리 스레드에서 했다면 `popup.open`은 응답을 받은 뒤 처리된다.

use tasty_host_plugin::manager::{PendingPluginCall, PendingSurfacePopup};
use tasty_plugin_manifest::PopupScopeDecl;
use tasty_plugin_protocol::popup_decl::PopupDecl;
use tasty_plugin_protocol::{
    PopupDeclareParams, PopupShowParams, PopupShowResult, PopupShowTarget,
};

use crate::app::App;
use crate::plugin::PluginManager;

/// 형식이 틀렸거나, 선언이 없거나, 대상 종류가 선언과 다르거나, 다른 plugin의 surface다.
pub(crate) const POPUP_INVALID_PARAMS: i32 = -32602;
/// 대상이 없다: 없는 창·surface id, 포커스된 창이 없음.
pub(crate) const POPUP_TARGET_NOT_FOUND: i32 = -32004;

type CallError = (i32, String);

fn invalid(msg: impl Into<String>) -> CallError {
    (POPUP_INVALID_PARAMS, msg.into())
}

fn not_found(msg: impl Into<String>) -> CallError {
    (POPUP_TARGET_NOT_FOUND, msg.into())
}

/// 선언과 맞춰 본 대상. 대상의 존재는 아직 확인하지 않았다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowTarget {
    Window(u64),
    FocusedWindow,
    Surface(u32),
}

impl App {
    pub(super) fn handle_ipc_popup_declare(&mut self, call: &PendingPluginCall) {
        let outcome = match serde_json::from_value::<PopupDeclareParams>(call.params.clone()) {
            Err(e) => Err(invalid(format!("popup.declare: invalid params: {e}"))),
            Ok(p) => match self.plugin_manager.as_mut() {
                None => Err(invalid("popup.declare: plugin manager unavailable")),
                Some(mgr) => mgr
                    .declare_popup(&call.plugin_id, &p.popup_id, p.decl.to_decl())
                    .map_err(|e| invalid(format!("popup.declare: {e}"))),
            },
        };
        if outcome.is_ok() {
            // 도구·명령이 surface 대상 없이 이 팝업을 열지 않도록 목록을 다시 만든다.
            self.refresh_tool_registry();
        }
        self.reply_popup_call(call, outcome.map(|()| serde_json::json!({})));
    }

    pub(super) fn handle_ipc_popup_show(&mut self, call: &PendingPluginCall) {
        let outcome = self.show_plugin_popup(call).map(|instance_id| {
            serde_json::to_value(PopupShowResult { instance_id })
                .unwrap_or_else(|_| serde_json::json!({ "instance_id": instance_id }))
        });
        self.reply_popup_call(call, outcome);
    }

    fn show_plugin_popup(&mut self, call: &PendingPluginCall) -> Result<u64, CallError> {
        let p = serde_json::from_value::<PopupShowParams>(call.params.clone())
            .map_err(|e| invalid(format!("popup.show: invalid params: {e}")))?;
        let decl = self
            .plugin_manager
            .as_ref()
            .ok_or_else(|| invalid("popup.show: plugin manager unavailable"))?
            .popup_decl(&call.plugin_id, &p.popup_id);
        let target = match_target(&call.plugin_id, &p.popup_id, decl, p.target)?;
        // 창 범위 대상은 요청을 받은 지금 고른 창이다. 인스턴스는 그 창에만 결속한다.
        let (surface, window) = match target {
            ShowTarget::Surface(sid) => {
                self.check_surface_target(&call.plugin_id, &p.popup_id, sid)?;
                (Some(sid), None)
            }
            ShowTarget::Window(wid) => {
                let Some(window) = self
                    .view
                    .views
                    .iter()
                    .find(|(w, v)| u64::from(**w) == wid && v.as_main().is_some())
                    .map(|(w, _)| *w)
                else {
                    return Err(not_found(format!("popup.show: window {wid} not found")));
                };
                (None, Some(window))
            }
            ShowTarget::FocusedWindow => {
                let Some(window) = self.focused_window().and(self.view.focused_view_id) else {
                    return Err(not_found("popup.show: no window is focused"));
                };
                (None, Some(window))
            }
        };
        let mgr = self
            .plugin_manager
            .as_mut()
            .ok_or_else(|| invalid("popup.show: plugin manager unavailable"))?;
        let scope = if surface.is_some() {
            PopupScopeDecl::Surface
        } else {
            PopupScopeDecl::Window
        };
        check_open_scope(mgr, &call.plugin_id, &p.popup_id, scope)?;
        let was_open =
            crate::plugin_bridge::popup_scope::is_open(mgr, &call.plugin_id, &p.popup_id);
        let instance_id = mgr
            .open_popup_instance(&call.plugin_id, &p.popup_id, p.context)
            .ok_or_else(|| {
                invalid(format!(
                    "popup.show: popup '{}/{}' could not be opened",
                    call.plugin_id, p.popup_id
                ))
            })?;
        if let Some(sid) = surface {
            mgr.bind_popup_instance_surface(instance_id, sid);
        }
        if let Some(main) = window
            .and_then(|w| self.view.views.get_mut(&w))
            .and_then(|v| v.as_main_mut())
            && let Some(mgr) = self.plugin_manager.as_ref()
        {
            crate::plugin_bridge::popup_scope::bind_window_popup(
                &mut main.state,
                mgr,
                instance_id,
                was_open,
            );
        }
        let drawn_in = window.or_else(|| surface.and_then(|sid| self.find_main_with_surface(sid)));
        if let Some(wid) = drawn_in {
            self.focus_plugin_opened_popup(wid, &call.plugin_id, instance_id);
        }
        Ok(instance_id)
    }

    /// 창에 있는 surface여야 하고, 그 surface의 주인이 요청 plugin이어야 한다.
    fn check_surface_target(
        &self,
        plugin_id: &str,
        popup_id: &str,
        surface_id: u32,
    ) -> Result<(), CallError> {
        if self.find_main_with_surface(surface_id).is_none() {
            return Err(not_found(format!(
                "popup.show: surface {surface_id} not found"
            )));
        }
        let owner = self.plugin_surface_owner_of_any_kind(surface_id);
        let req = PendingSurfacePopup {
            plugin_id: plugin_id.to_string(),
            popup_id: popup_id.to_string(),
            scope: PopupScopeDecl::Surface,
            context: serde_json::Value::Null,
            publisher: None,
            surface_id,
        };
        super::surface_popups::authorize(&req, owner.as_deref())
            .map_err(|e| invalid(format!("popup.show: {e}")))
    }

    fn reply_popup_call(
        &mut self,
        call: &PendingPluginCall,
        outcome: Result<serde_json::Value, CallError>,
    ) {
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        match outcome {
            Ok(v) => mgr.send_plugin_call_result(call, Some(v), None, None),
            Err((code, msg)) => mgr.send_plugin_call_result(call, None, Some(msg), Some(code)),
        }
    }
}

/// 선언과 대상을 맞춰 본다. 대상 종류는 선언의 범위 종류와 같아야 하고 창 범위로 낮추지 않는다.
///
/// 지금 여는 대상은 창·포커스 창·surface다. workspace·pane·tab 대상은 plugin 팝업이 소유 화면의
/// 팝업 목록으로 옮겨진 뒤 연다. 창 범위 팝업은 대상 창에만 그려진다.
fn match_target(
    plugin_id: &str,
    popup_id: &str,
    decl: Option<PopupDecl>,
    target: PopupShowTarget,
) -> Result<ShowTarget, CallError> {
    let decl = decl.ok_or_else(|| {
        invalid(format!(
            "popup.show: popup '{plugin_id}/{popup_id}' is not declared"
        ))
    })?;
    if target.scope_kind() != decl.scope_kind {
        return Err(invalid(format!(
            "popup.show: popup '{plugin_id}/{popup_id}' is declared with scope '{:?}' but the target is '{:?}'",
            decl.scope_kind,
            target.scope_kind()
        )));
    }
    match target {
        PopupShowTarget::Window { id } => Ok(ShowTarget::Window(id)),
        PopupShowTarget::FocusedWindow => Ok(ShowTarget::FocusedWindow),
        PopupShowTarget::Surface { id } => Ok(ShowTarget::Surface(id)),
        other => Err(invalid(format!(
            "popup.show: '{:?}' targets are not supported yet",
            other.scope_kind()
        ))),
    }
}

/// 같은 팝업이 이미 열려 있으면 그 인스턴스를 돌려주므로, 열린 범위가 이번 대상의 범위와 같아야 한다.
/// 열린 인스턴스는 열 때의 선언을 유지한다. 범위가 다르면 바꾸지 않고 거절한다.
fn check_open_scope(
    mgr: &PluginManager,
    plugin_id: &str,
    popup_id: &str,
    scope: PopupScopeDecl,
) -> Result<(), CallError> {
    let open = mgr
        .popup_instances()
        .find(|(_, inst)| inst.plugin_id == plugin_id && inst.popup_id == popup_id);
    match open {
        Some((_, inst)) if inst.contribute.scope != scope => Err(invalid(format!(
            "popup.show: popup '{plugin_id}/{popup_id}' is open with a different scope; close it first"
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "plugin_popup_ipc_tests.rs"]
mod tests;
