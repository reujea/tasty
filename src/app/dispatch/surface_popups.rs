//! surface 이벤트로 여는 플러그인 이벤트 팝업을 연다. surface 범위는 대상 surface 의 소유를 확인한 뒤
//! 그 surface 에 연다. 플러그인이 자기 surface 가 아닌 곳에 팝업을 띄우지 못하게 하는 경계다
//! (docs/design/systems/popup.md). 창 범위는 그 surface 가 있는 창에만 연다.

use crate::app::App;
use tasty_host_plugin::manager::PendingSurfacePopup;

impl App {
    pub(crate) fn dispatch_pending_surface_popups(&mut self) {
        let Some(pending) = self
            .plugin_manager
            .as_mut()
            .map(|m| m.take_pending_surface_popups())
        else {
            return;
        };
        for req in pending {
            if req.scope == tasty_plugin_manifest::PopupScopeDecl::Window {
                self.open_event_window_popup(req);
                continue;
            }
            let owner = self.plugin_surface_owner_of_any_kind(req.surface_id);
            if let Err(reason) = authorize(&req, owner.as_deref()) {
                tracing::warn!(
                    "popup '{}/{}' refused: {reason}",
                    req.plugin_id,
                    req.popup_id
                );
                continue;
            }
            let Some(mgr) = self.plugin_manager.as_mut() else {
                return;
            };
            if let Some(instance_id) =
                mgr.open_popup_instance(&req.plugin_id, &req.popup_id, req.context)
            {
                mgr.bind_popup_instance_surface(instance_id, req.surface_id);
                if let Some(wid) = self.find_main_with_surface(req.surface_id) {
                    self.focus_plugin_opened_popup(wid, &req.plugin_id, instance_id);
                }
            }
        }
    }

    /// plugin 이 스스로 연 팝업(surface 이벤트·`popup.show`)을 그 팝업을 그리는 창의 포커스 노드로 할지 정한다.
    /// 여는 시점에 그 창의 엔진 포커스 surface 가 그 plugin 의 surface 면 사용자 출처다
    /// (`popup_scope::opened_by_user`).
    pub(super) fn focus_plugin_opened_popup(
        &mut self,
        wid: winit::window::WindowId,
        plugin_id: &str,
        instance_id: u64,
    ) {
        let focused = self
            .engines()
            .window_pair(wid)
            .and_then(|(main, engine)| main.state.focused_surface_id(&engine));
        let owner = focused.and_then(|sid| self.plugin_surface_owner_of_any_kind(sid));
        let opener = crate::plugin_bridge::popup_scope::PopupOpener::Plugin {
            plugin_id,
            focused_surface_owner: owner.as_deref(),
        };
        let Some(mgr) = self.plugin_manager.as_ref() else {
            return;
        };
        if let Some(main) = self.view.views.get_mut(&wid).and_then(|v| v.as_main_mut()) {
            crate::plugin_bridge::popup_scope::focus_opened_popup(
                &mut main.state,
                mgr,
                instance_id,
                opener,
            );
        }
    }

    /// 창 범위 이벤트 팝업을 대상 surface 가 있는 창에 연다. 그 surface 가 창에 없으면 열지 않는다.
    /// 창 범위는 surface 를 지목하지 않으므로 소유·발행자를 대조하지 않는다.
    fn open_event_window_popup(&mut self, req: PendingSurfacePopup) {
        let Some(wid) = self.find_main_with_surface(req.surface_id) else {
            tracing::warn!(
                "popup '{}/{}' not opened: surface {} is not in a window",
                req.plugin_id,
                req.popup_id,
                req.surface_id
            );
            return;
        };
        let Some(mgr) = self.plugin_manager.as_mut() else {
            return;
        };
        let was_open =
            crate::plugin_bridge::popup_scope::is_open(mgr, &req.plugin_id, &req.popup_id);
        let Some(instance_id) = mgr.open_popup_instance(&req.plugin_id, &req.popup_id, req.context)
        else {
            return;
        };
        if let Some(main) = self.view.views.get_mut(&wid).and_then(|v| v.as_main_mut()) {
            crate::plugin_bridge::popup_scope::bind_window_popup(
                &mut main.state,
                mgr,
                instance_id,
                was_open,
            );
        }
        self.focus_plugin_opened_popup(wid, &req.plugin_id, instance_id);
    }

    /// 창에 있는 플러그인 surface 의 소유 플러그인. webview(RemoteSurface)와 egui-mesh 를 모두 본다.
    /// parked engine 은 화면이 없어 팝업을 그릴 곳이 없으므로 보지 않는다.
    pub(super) fn plugin_surface_owner_of_any_kind(&self, surface_id: u32) -> Option<String> {
        self.engines().windows().find_map(|(_, e)| {
            let surface = e.find_surface_by_id(surface_id)?;
            let any = surface.as_any();
            if let Some(rs) =
                any.downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            {
                return Some(rs.plugin_id.clone());
            }
            any.downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
                .map(|ms| ms.plugin_id.clone())
        })
    }
}

/// 대상은 팝업 소유 플러그인의 surface 여야 하고, 플러그인이 발행한 이벤트라면
/// 발행자도 팝업 소유 플러그인이어야 한다. 호스트 발행 이벤트는 발행자 검사를 생략한다.
pub(crate) fn authorize(req: &PendingSurfacePopup, owner: Option<&str>) -> Result<(), String> {
    if let Some(publisher) = req.publisher.as_deref()
        && publisher != req.plugin_id
    {
        return Err(format!(
            "event from plugin '{publisher}' cannot place another plugin's popup on surface {}",
            req.surface_id
        ));
    }
    match owner {
        None => Err(format!(
            "surface {} is not a live plugin surface",
            req.surface_id
        )),
        Some(owner) if owner != req.plugin_id => Err(format!(
            "surface {} belongs to plugin '{owner}'",
            req.surface_id
        )),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(publisher: Option<&str>) -> PendingSurfacePopup {
        PendingSurfacePopup {
            plugin_id: "com.tasty.markdown".to_string(),
            popup_id: "large-file-confirm".to_string(),
            scope: tasty_plugin_manifest::PopupScopeDecl::Surface,
            context: serde_json::Value::Null,
            publisher: publisher.map(str::to_string),
            surface_id: 7,
        }
    }

    #[test]
    fn own_surface_from_own_event_is_allowed() {
        assert!(authorize(&req(Some("com.tasty.markdown")), Some("com.tasty.markdown")).is_ok());
        assert!(authorize(&req(None), Some("com.tasty.markdown")).is_ok());
    }

    #[test]
    fn another_plugins_surface_is_refused() {
        let err = authorize(&req(Some("com.tasty.markdown")), Some("com.tasty.html"))
            .expect_err("foreign surface");
        assert!(err.contains("belongs to plugin 'com.tasty.html'"), "{err}");
    }

    #[test]
    fn a_host_surface_or_a_missing_one_is_refused() {
        let err = authorize(&req(Some("com.tasty.markdown")), None).expect_err("no owner");
        assert!(err.contains("not a live plugin surface"), "{err}");
    }

    #[test]
    fn another_plugins_event_cannot_place_the_popup() {
        let err = authorize(&req(Some("com.tasty.html")), Some("com.tasty.markdown"))
            .expect_err("foreign publisher");
        assert!(err.contains("event from plugin 'com.tasty.html'"), "{err}");
    }
}
