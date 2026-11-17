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
            if !surface.capabilities.mirror_viewer {
                continue;
            }
            let (source, generation) = surface.html_source_info();
            *descriptor = serde_json::json!({"remote_id":id,"role":super::mirror_role::VIEWER,"kind":surface.kind_static,"plugin_id":surface.plugin_id,"display_name":crate::model::Surface::display_name(surface),"source":source,"source_generation":generation});
        }
    }
}
