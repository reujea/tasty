//! plugin 팝업 선언 저장소. 실행 중 선언(`popup.declare`)과 매니페스트 선언을 한 조회로 묶는다.
//!
//! 매니페스트 값은 표에 복사하지 않고 조회할 때 바꾼다. 패키지 교체·비활성과 표를 따로 맞추지 않기 위해서다.

use tasty_plugin_manifest::{PopupContribute, PopupRendering, PopupScopeDecl, PopupTrigger};
use tasty_plugin_protocol::popup_decl::{
    PopupAnchor, PopupDecl, PopupOutsideClick, PopupScopeKind,
};

use super::PluginManager;

impl PluginManager {
    /// 실행 중인 plugin의 팝업 선언을 등록한다. 같은 id가 있으면 바꾼다.
    ///
    /// 이미 열린 인스턴스는 연 때의 선언을 유지하고, 새 선언은 다음 열기부터 쓴다.
    /// 실행 중 선언은 egui-mesh로 그리므로 매니페스트 `api_version`이 host와 같아야 한다.
    pub fn declare_popup(
        &mut self,
        plugin_id: &str,
        popup_id: &str,
        decl: PopupDecl,
    ) -> Result<(), String> {
        if !self.processes.contains_key(plugin_id) {
            return Err(format!("plugin '{plugin_id}' is not running"));
        }
        let pkg = self
            .packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .ok_or_else(|| format!("plugin '{plugin_id}' is not installed"))?;
        if pkg.manifest.api_version != tasty_plugin_manifest::HOST_API_VERSION {
            return Err(format!(
                "plugin '{plugin_id}' has api_version '{}' incompatible with host '{}'",
                pkg.manifest.api_version,
                tasty_plugin_manifest::HOST_API_VERSION
            ));
        }
        tasty_plugin_manifest::validate_popup_decl(popup_id, &decl)?;
        self.popup_decls
            .insert((plugin_id.to_string(), popup_id.to_string()), decl);
        Ok(())
    }

    /// 팝업 선언. 실행 중 선언이 있으면 그것을, 없으면 활성 plugin의 매니페스트 선언을 바꾼 값을 쓴다.
    pub fn popup_decl(&self, plugin_id: &str, popup_id: &str) -> Option<PopupDecl> {
        if let Some(decl) = self
            .popup_decls
            .get(&(plugin_id.to_string(), popup_id.to_string()))
        {
            return Some(*decl);
        }
        self.manifest_popup(plugin_id, popup_id)
            .map(PopupContribute::to_decl)
    }

    /// 선언된 팝업의 (plugin id, popup id). 실행 중 선언과 활성 plugin의 매니페스트 선언을 합친다.
    pub fn declared_popup_keys(&self) -> Vec<(String, String)> {
        let mut keys: Vec<(String, String)> = self
            .plugin_popup_contributes()
            .into_iter()
            .map(|e| (e.plugin_id, e.contribute.id))
            .collect();
        for key in self.popup_decls.keys() {
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    }

    pub(super) fn forget_popup_decls(&mut self, plugin_id: &str) {
        self.popup_decls.retain(|(p, _), _| p != plugin_id);
    }

    fn manifest_popup(&self, plugin_id: &str, popup_id: &str) -> Option<&PopupContribute> {
        if self.config.is_disabled(plugin_id) {
            return None;
        }
        self.packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)?
            .manifest
            .contributes
            .popup
            .iter()
            .find(|p| p.id == popup_id)
    }

    /// 팝업을 열 때 인스턴스에 담을 선언. 실행 중 선언이 매니페스트 선언보다 먼저다.
    /// egui-mesh 매니페스트 선언은 host와 plugin의 api_version도 같아야 한다.
    pub(super) fn resolve_open_popup_contribute(
        &self,
        plugin_id: &str,
        popup_id: &str,
    ) -> Option<PopupContribute> {
        if let Some(decl) = self
            .popup_decls
            .get(&(plugin_id.to_string(), popup_id.to_string()))
        {
            return instance_contribute(plugin_id, popup_id, decl);
        }
        let pkg = self.packages.iter().find(|p| p.manifest.id == plugin_id)?;
        let contribute = pkg
            .manifest
            .contributes
            .popup
            .iter()
            .find(|p| p.id == popup_id)
            .cloned()?;
        if contribute.rendering == PopupRendering::EguiMesh
            && pkg.manifest.api_version != tasty_plugin_manifest::HOST_API_VERSION
        {
            tracing::warn!(
                "popup.open: egui-mesh popup '{plugin_id}/{popup_id}' has api_version '{}' \
                 incompatible with host '{}'; ignoring",
                pkg.manifest.api_version,
                tasty_plugin_manifest::HOST_API_VERSION
            );
            return None;
        }
        Some(contribute)
    }
}

/// 실행 중 선언을 지금 plugin 팝업 렌더러가 읽는 인스턴스 값으로 옮긴다.
///
/// 렌더러는 창·surface 범위와 가운데·포인터 기준만 그린다. 나머지 범위 종류는 열지 않는다.
/// `scope_top`·`at_point`·`column`은 범위 가운데에 놓는다. 크기는 정수 논리 픽셀로 반올림한다.
// TODO(popup): 인스턴스가 PopupHost로 옮겨지면 선언을 그대로 담고 이 변환을 없앤다.
fn instance_contribute(
    plugin_id: &str,
    popup_id: &str,
    decl: &PopupDecl,
) -> Option<PopupContribute> {
    let scope = match decl.scope_kind {
        PopupScopeKind::Window => PopupScopeDecl::Window,
        PopupScopeKind::Surface => PopupScopeDecl::Surface,
        other => {
            tracing::warn!(
                "popup.open: '{plugin_id}/{popup_id}' has scope '{other:?}', which plugin popups \
                 cannot use yet; not opened"
            );
            return None;
        }
    };
    let anchor = match decl.anchor {
        PopupAnchor::Cursor => tasty_plugin_manifest::PopupAnchor::Cursor,
        PopupAnchor::ScopeCenter
        | PopupAnchor::ScopeTop
        | PopupAnchor::AtPoint
        | PopupAnchor::Column => tasty_plugin_manifest::PopupAnchor::ScreenCenter,
    };
    let px = |v: f32| v.round().max(1.0) as u32;
    Some(PopupContribute {
        id: popup_id.to_string(),
        trigger: PopupTrigger::Ipc,
        size_hint: Some(tasty_plugin_manifest::PopupSizeHint {
            width: px(decl.size.width.value()),
            height: px(decl.size.height.value()),
        }),
        anchor,
        scope,
        dismiss_on_outside_click: decl.outside_click == PopupOutsideClick::Close,
        rendering: PopupRendering::EguiMesh,
    })
}
