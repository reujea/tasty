//! descriptor가 알린 원격 문서를 로컬 plugin kind로 만든다. kind 이름이 아니라 그 kind가 선언한
//! 능력(`mirror_content`·`mirror_viewer`)으로 고르고, 알림은 그 surface를 소유한 plugin에 보낸다.

use crate::model::{DeferredPlugin, EmptySurface, Surface};
use crate::remote::mirror_role;
use crate::runtime::surface_registry::{KindCapabilities, SurfaceKindRegistry};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DocumentMirror {
    /// 서버가 파일 원문을 주고 로컬 kind가 `remote.file`로 연다.
    Content,
    /// 서버 문서를 로컬 kind가 `remote.html_entry` viewer로 연다.
    Viewer,
}

impl DocumentMirror {
    pub(super) fn of(descriptor: &Value) -> Option<Self> {
        match descriptor.get("role").and_then(Value::as_str) {
            Some(mirror_role::CONTENT) => Some(Self::Content),
            Some(mirror_role::VIEWER) => Some(Self::Viewer),
            _ => None,
        }
    }

    fn declared_by(self, capabilities: &KindCapabilities) -> bool {
        match self {
            Self::Content => capabilities.mirror_content,
            Self::Viewer => capabilities.mirror_viewer && super::html::viewer_available(),
        }
    }

    /// 지금 만들 수 있는 로컬 kind. descriptor가 알린 kind가 그 능력을 선언해야 한다.
    /// kind를 알리지 않는 이전 빌드의 서버면 그 능력을 선언한 kind 중 이름 순으로 첫 kind를 쓴다.
    pub(super) fn live_kind(
        self,
        registry: &SurfaceKindRegistry,
        descriptor: &Value,
    ) -> Option<&'static str> {
        match descriptor.get("kind").and_then(Value::as_str) {
            Some(kind) => registry
                .get_live(kind)
                .filter(|def| self.declared_by(&def.capabilities))
                .map(|def| def.kind),
            None => registry.first_live_kind_with(|caps| self.declared_by(caps)),
        }
    }

    fn params(self, descriptor: &Value, kind: &str) -> Value {
        let display_name = |fallback: &str| {
            descriptor
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(fallback)
                .to_owned()
        };
        match self {
            Self::Content => serde_json::json!({
                "display_name": display_name(kind),
                "remote": {
                    "file": descriptor.get("file").and_then(Value::as_str).unwrap_or_default(),
                },
            }),
            Self::Viewer => serde_json::json!({
                "display_name": display_name("HTML"),
                "remote": {"html_entry": true, "source": descriptor.get("source")},
            }),
        }
    }

    /// 로컬 kind로 surface를 만든다. kind가 아직 등록되지 않았으면 등록될 때 만들도록 미뤄 둔다.
    pub(super) fn materialize(
        self,
        descriptor: &Value,
        local: u32,
        registry: &SurfaceKindRegistry,
    ) -> Option<Box<dyn Surface>> {
        if let Some(kind) = self.live_kind(registry, descriptor) {
            let definition = registry.get_live(kind)?;
            let params = self.params(descriptor, kind);
            return match (definition.create)(local, None, &params)
                .and_then(|prepared| prepared.publish())
            {
                Ok(surface) => Some(surface),
                Err(error) => {
                    tracing::warn!(surface = local, kind, %error, "attach mirror document creation failed");
                    None
                }
            };
        }
        let kind = descriptor.get("kind").and_then(Value::as_str)?;
        (!registry.contains(kind)).then(|| {
            Box::new(EmptySurface::new_deferred_plugin(
                local,
                DeferredPlugin {
                    kind: kind.to_owned(),
                    snapshot: self.params(descriptor, kind),
                },
            )) as Box<dyn Surface>
        })
    }
}

/// 미뤄 둔 문서의 kind가 이제 어느 문서 mirror로든 만들 수 있는지.
pub(super) fn deferred_available(registry: &SurfaceKindRegistry, kind: &str) -> bool {
    registry.get_live(kind).is_some_and(|def| {
        DocumentMirror::Content.declared_by(&def.capabilities)
            || DocumentMirror::Viewer.declared_by(&def.capabilities)
    })
}

/// mirror 삭제는 일반 lifecycle 큐를 거치지 않아 사라진 문서를 소유 plugin에 직접 알린다.
pub(super) fn destroy(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    ids: impl IntoIterator<Item = u32>,
) {
    if let Some(manager) = plugin_manager {
        for id in ids {
            manager.destroy_remote_surface(id, None);
        }
    }
}

/// `surface_id`(로컬 ID)를 소유한 plugin에만 보낸다. 게시 전이거나 닫힌 surface면 보내지 않는다.
pub(super) fn emit_to_owner(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    surface_id: u32,
    event: &str,
    payload: &Value,
) {
    let Some(manager) = plugin_manager.as_mut() else {
        return;
    };
    let Some(owner) = manager.remote_surface_owner(surface_id).map(str::to_owned) else {
        tracing::debug!(
            surface_id,
            event,
            "attach mirror document has no published owner"
        );
        return;
    };
    manager.emit_host_event_to_plugin(
        &owner,
        event,
        payload,
        tasty_plugin_protocol::EventScope::System,
    );
}
