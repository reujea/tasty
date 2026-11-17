//! 원 surface binding과 plugin process에 고정된 회수 요청 및 ACK 추적.

use super::super::{
    PendingRequest, PendingRequestKind, PluginManager, RemotePublication, RemoteSurfaceEntry,
};
use crate::host_cmd::HostCmd;
use crate::protocol;
use serde_json::json;

impl PluginManager {
    /// Creation and retirement share one FIFO, including an unpumped previous Created command.
    pub fn enqueue_bound_remote_retirement(
        &self,
        surface_id: u32,
        binding: crate::host_cmd::SurfaceBinding,
    ) -> Result<(), String> {
        self.host_cmd_tx
            .send(HostCmd::RemoteSurfaceRetired {
                surface_id,
                binding,
                completion: None,
            })
            .map_err(|error| error.to_string())
    }

    pub fn enqueue_observed_remote_retirement(
        &self,
        surface_id: u32,
        binding: crate::host_cmd::SurfaceBinding,
    ) -> Result<crate::host_cmd::RemoteRetirementReceipt, String> {
        let (receipt, completion) =
            crate::host_cmd::RemoteRetirementReceipt::pending(surface_id, binding.clone());
        self.host_cmd_tx
            .send(HostCmd::RemoteSurfaceRetired {
                surface_id,
                binding,
                completion: Some(completion),
            })
            .map_err(|error| error.to_string())?;
        Ok(receipt)
    }

    pub fn enqueue_observed_mesh_retirement(
        &mut self,
        surface_id: u32,
        binding: &crate::host_cmd::MeshBinding,
    ) -> Result<crate::host_cmd::RemoteRetirementReceipt, String> {
        use crate::host_cmd::{MeshPublication, RemoteRetirementReceipt};
        let mut publication = binding
            .publication
            .lock()
            .map_err(|_| "mesh binding poisoned")?;
        if let MeshPublication::Retiring(receipt) = &*publication {
            return Ok(receipt.clone());
        }
        let mut receipts = Vec::new();
        match &*publication {
            MeshPublication::NeverSent => {
                let (receipt, completion) =
                    RemoteRetirementReceipt::pending(surface_id, binding.binding());
                completion.finish(Ok(()));
                receipts.push(receipt);
            }
            MeshPublication::Sent(generations) => {
                for generation in generations {
                    let (receipt, completion) =
                        RemoteRetirementReceipt::pending(surface_id, binding.binding());
                    let crate::host_cmd::MeshBootstrap {
                        plugin,
                        process,
                        request: bootstrap,
                    } = generation;
                    if let Some(owner) = self
                        .processes
                        .get(plugin)
                        .filter(|owner| owner.reply_binding().ptr_eq(process))
                    {
                        let id = self
                            .next_request_id
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let request = protocol::PluginRequest::new(
                            protocol::METHOD_SURFACE_DESTROY,
                            json!({"surface_id":surface_id}),
                            id,
                        );
                        match owner.try_send_request(request) {
                        Ok(()) => {self.pending_requests.insert(id,PendingRequest::now(plugin,PendingRequestKind::SurfaceRetire {completion,process_binding:process.clone()}));},
                        Err(error) => completion.finish(Err(format!("mesh bootstrap {bootstrap} destruction was not acknowledged: {error}"))),
                    }
                    } else if let Some(completion) =
                        self.settle_on_retired_generation(plugin, process, completion)
                    {
                        completion.finish(Err(format!(
                            "mesh bootstrap {bootstrap} original process is unavailable"
                        )));
                    }
                    receipts.push(receipt);
                }
            }
            MeshPublication::Retiring(_) => unreachable!(),
        }
        let receipt = RemoteRetirementReceipt::group(surface_id, binding.binding(), receipts);
        *publication = MeshPublication::Retiring(receipt.clone());
        Ok(receipt)
    }

    pub(super) fn destroy_observed_remote_surface(
        &mut self,
        surface_id: u32,
        binding: crate::host_cmd::SurfaceBinding,
        completion: crate::host_cmd::RemoteRetirementCompletion,
    ) {
        let Some(entry) = self.surfaces.get(&surface_id) else {
            completion.finish(Err(
                "original remote registration is absent; destruction is unconfirmed".into(),
            ));
            return;
        };
        if !binding.matches(&entry.handles) {
            completion.finish(Err(
                "remote registration changed before destruction acknowledgement".into(),
            ));
            return;
        }
        let process_binding = match &entry.publication {
            RemotePublication::NeverSent => None,
            RemotePublication::Sent(binding) => Some(binding.clone()),
        };
        let plugin_id = entry.plugin_id.clone();
        if let Some(frame) = self.egui_mesh_frames.remove(&surface_id) {
            self.release_plugin_buffer(&frame.plugin_id, frame.buffer_id);
        }
        self.fresh_frame_surfaces.remove(&surface_id);
        self.surfaces.remove(&surface_id);
        let Some(process_binding) = process_binding else {
            completion.finish(Ok(()));
            return;
        };
        let Some(process) = self
            .processes
            .get(&plugin_id)
            .filter(|process| process.reply_binding().ptr_eq(&process_binding))
        else {
            if let Some(completion) =
                self.settle_on_retired_generation(&plugin_id, &process_binding, completion)
            {
                completion.finish(Err(
                    "original plugin process is unavailable for destruction".into(),
                ));
            }
            return;
        };
        let id = self
            .next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let request = protocol::PluginRequest::new(
            protocol::METHOD_SURFACE_DESTROY,
            json!({"surface_id":surface_id}),
            id,
        );
        match process.try_send_request(request) {
            Ok(()) => {
                self.pending_requests.insert(
                    id,
                    PendingRequest::now(
                        &plugin_id,
                        PendingRequestKind::SurfaceRetire {
                            completion,
                            process_binding,
                        },
                    ),
                );
            }
            Err(error) => completion.finish(Err(format!(
                "remote destruction request was not acknowledged: {error}"
            ))),
        }
    }

    /// Retire only the instance captured before replacement, never a newer same-ID registration.
    pub(super) fn destroy_bound_remote_surface(
        &mut self,
        surface_id: u32,
        binding: &crate::host_cmd::SurfaceBinding,
    ) -> bool {
        let Some(entry) = self.surfaces.get(&surface_id) else {
            return true;
        };
        if !binding.matches(&entry.handles) {
            return false;
        }
        self.destroy_remote_surface(surface_id, None);
        true
    }

    /// 게시한 remote·webview surface를 소유한 plugin. 아직 게시 전이거나 닫힌 surface면 `None`이다.
    pub fn remote_surface_owner(&self, surface_id: u32) -> Option<&str> {
        self.surfaces
            .get(&surface_id)
            .map(|entry| entry.plugin_id.as_str())
    }

    /// surface 종료를 플러그인에 알리고 호스트의 프레임·상태를 지운다.
    /// 소유자 정보가 없는 surface는 남아 있는 프레임 정보와 매니페스트 종류로 확인한다.
    pub fn destroy_remote_surface(&mut self, surface_id: u32, kind: Option<&str>) {
        // 이 surface 의 mesh frame 이 참조하던 shared buffer 매핑도 host 측에서
        // 해제한다 — plugin 은 해제를 알릴 프로토콜 메시지가 없어 여기서 안 지우면
        // plugin 수명 내내 누적된다 (`release_plugin_buffer` 문서 참조).
        let frame = self.egui_mesh_frames.remove(&surface_id);
        self.fresh_frame_surfaces.remove(&surface_id);
        if let Some(f) = &frame {
            let (pid, bid) = (f.plugin_id.clone(), f.buffer_id);
            self.release_plugin_buffer(&pid, bid);
        }
        if let Some(entry) = self.surfaces.remove(&surface_id) {
            self.destroy_published_remote_surface(surface_id, entry);
            return;
        }
        // egui-mesh surface: 수신했던 mesh frame 의 plugin_id 가 1순위 owner 소스다 —
        // cascade 시점엔 surface 가 이미 layout 에서 제거돼 kind 가 None 으로 올 수
        // 있기 때문 (`cascade_surface_closed` 의 surface_kind 폴백 주석 참조).
        // frame 을 한 번도 못 받은 surface(paint 전 즉시 close)만 kind 선언으로 폴백.
        let owner = frame
            .map(|f| f.plugin_id)
            .or_else(|| kind.and_then(|k| self.plugin_id_for_surface_kind(k)));
        if let Some(pid) = owner {
            tracing::debug!("surface.destroy → plugin '{pid}' (surface {surface_id})");
            self.send_surface_request(
                &pid,
                protocol::METHOD_SURFACE_DESTROY,
                json!({ "surface_id": surface_id }),
                PendingRequestKind::Other,
            );
        } else {
            tracing::debug!(
                "surface.destroy skipped (surface {surface_id}, kind {kind:?} — owner 미해석)"
            );
        }
    }

    // The caller has removed frame mappings and this exact entry before sending destruction.
    fn destroy_published_remote_surface(&mut self, surface_id: u32, entry: RemoteSurfaceEntry) {
        match &entry.publication {
            RemotePublication::NeverSent => return,
            RemotePublication::Sent(binding)
                if self
                    .processes
                    .get(&entry.plugin_id)
                    .is_some_and(|process| process.reply_binding().ptr_eq(binding)) => {}
            RemotePublication::Sent(binding)
                if self.is_retired_generation(&entry.plugin_id, binding) =>
            {
                tracing::debug!(surface_id, plugin = %entry.plugin_id,
                        "original remote process was retired with its surface instance");
                return;
            }
            RemotePublication::Sent(_) => {
                tracing::warn!(surface_id, plugin = %entry.plugin_id,
                        "original remote process is unavailable; destruction remains unconfirmed");
                return;
            }
        }
        self.send_surface_request(
            &entry.plugin_id,
            protocol::METHOD_SURFACE_DESTROY,
            json!({ "surface_id": surface_id }),
            PendingRequestKind::Other,
        );
        // entry drop → SurfaceHandles(shm) 해제.
    }

    /// manifest `[[surface_kinds]]` 가 `kind` 를 선언한 plugin id. egui-mesh
    /// surface 의 kind→owner 해석용. 없으면(터미널/호스트 빌트인) None.
    fn plugin_id_for_surface_kind(&self, kind: &str) -> Option<String> {
        self.packages
            .iter()
            .find(|p| p.manifest.surface_kinds.iter().any(|sk| sk.kind == kind))
            .map(|p| p.manifest.id.clone())
    }
}
