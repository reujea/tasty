//! Viewer-mirror leaves (`mirror_viewer`) are exposed only after viewer negotiation on the current registration.
use crate::{plugin_bridge::remote_surface::RemoteSurface, runtime::engine_access::EngineRef};
impl EngineRef<'_> {
    pub(super) fn project_html_entry_roles(
        &self,
        workspace: u32,
        surfaces: &mut [serde_json::Value],
    ) {
        let Some(holder) = self.live.occupancy.workspace_holder(workspace) else {
            return;
        };
        if self
            .remote
            .notifier()
            .is_none_or(|hub| !hub.html_viewer_enabled(holder))
        {
            return;
        }
        for descriptor in surfaces {
            if descriptor["role"] != "placeholder" {
                continue;
            }
            let Some(id) = descriptor["remote_id"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
            else {
                continue;
            };
            let Some(surface) = self
                .runtime
                .surfaces
                .get(&id)
                .and_then(|s| s.as_any().downcast_ref::<RemoteSurface>())
            else {
                continue;
            };
            if !announces_viewer(&surface.capabilities) {
                continue;
            }
            let (source, generation) = surface.html_source_info();
            *descriptor = serde_json::json!({"remote_id":id,"role":super::mirror_role::VIEWER,"kind":surface.kind_static,"plugin_id":surface.plugin_id,"display_name":crate::model::Surface::display_name(surface),"source":source,"source_generation":generation});
        }
    }
}

/// viewer로 알릴 kind. client의 viewer는 문서 출처(`address_band`일 때만 기록)와 서버의 HTTP·하위 파일
/// 제공(`remote_http_serve`)을 받아야 보이므로, 셋을 모두 선언한 kind만 알린다.
fn announces_viewer(capabilities: &crate::runtime::surface_registry::KindCapabilities) -> bool {
    capabilities.mirror_viewer && capabilities.remote_http_serve && capabilities.address_band
}

#[cfg(test)]
mod tests {
    use super::announces_viewer;
    use crate::runtime::surface_registry::KindCapabilities;

    /// `mirror_viewer`만 선언하면 서버가 출처·HTTP를 줄 수 없어 viewer로 알리지 않는다.
    #[test]
    fn only_a_kind_that_can_serve_its_document_is_announced_as_a_viewer() {
        let full = KindCapabilities {
            mirror_viewer: true,
            remote_http_serve: true,
            address_band: true,
            ..KindCapabilities::default()
        };
        assert!(announces_viewer(&full));
        assert!(announces_viewer(&KindCapabilities::undeclared(
            "html",
            "com.tasty.html"
        )));
        for partial in [
            KindCapabilities {
                remote_http_serve: false,
                address_band: false,
                ..full
            },
            KindCapabilities {
                remote_http_serve: false,
                ..full
            },
            KindCapabilities {
                address_band: false,
                ..full
            },
        ] {
            assert!(!announces_viewer(&partial), "{partial:?}");
        }
    }
}
