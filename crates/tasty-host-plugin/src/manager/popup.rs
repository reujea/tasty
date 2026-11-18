//! Popup 인스턴스 생명주기: open/event/close, host 발급 instance_id 관리.
//! `send_surface_request` 헬퍼와 `send_command_invoke` 단축키 dispatch 도 포함.

use std::sync::atomic::Ordering;

use serde_json::json;

use crate::protocol::{self, PluginRequest};

use super::{PendingRequest, PendingRequestKind, PluginManager, PopupInstance};

impl PluginManager {
    pub(super) fn send_surface_request(
        &mut self,
        plugin_id: &str,
        method: &str,
        params: serde_json::Value,
        kind: PendingRequestKind,
    ) {
        self.send_owned_surface_request(plugin_id, method, params, kind);
    }

    /// The returned identity exists only after this process accepted the original request.
    pub(super) fn send_owned_surface_request(
        &mut self,
        plugin_id: &str,
        method: &str,
        params: serde_json::Value,
        kind: PendingRequestKind,
    ) -> super::RemotePublication {
        let Some(proc) = self.processes.get(plugin_id) else {
            return super::RemotePublication::NeverSent;
        };
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let req = PluginRequest::new(method, params, id);
        // 큐 포화와 연결 종료를 구분할 수 있도록 RequestSendError도 기록한다.
        match proc.try_send_request(req) {
            Ok(()) => {
                self.pending_requests
                    .insert(id, PendingRequest::now(plugin_id, kind));
                super::RemotePublication::Sent(proc.reply_binding())
            }
            Err(e) => {
                tracing::warn!("plugin '{plugin_id}' {method} send failed: {e}");
                super::RemotePublication::NeverSent
            }
        }
    }

    /// Plugin이 contribute한 popup의 새 인스턴스를 연다. plugin process에
    /// `popup.open` IPC를 보낸다.
    ///
    /// 반환값은 호스트가 발급한 instance_id. 추후 `close_popup_instance`에
    /// 같은 id를 넘겨야 한다.
    ///
    /// plugin이 실행 중이 아니거나 popup contribute가 없으면 `None`.
    pub fn open_popup_instance(
        &mut self,
        plugin_id: &str,
        popup_id: &str,
        context: serde_json::Value,
    ) -> Option<u64> {
        if !self.processes.contains_key(plugin_id) {
            tracing::warn!(
                "popup.open: plugin '{plugin_id}' is not running, dropping popup '{popup_id}'"
            );
            return None;
        }
        if let Some(existing) = self.find_open_popup_instance(plugin_id, popup_id) {
            return Some(existing);
        }
        let contribute = self.resolve_open_popup_contribute(plugin_id, popup_id)?;
        let instance_id = self.next_popup_instance_id;
        self.next_popup_instance_id = self.next_popup_instance_id.wrapping_add(1);
        self.popup_instances.insert(
            instance_id,
            PopupInstance {
                plugin_id: plugin_id.to_string(),
                popup_id: popup_id.to_string(),
                contribute,
                z_seq: super::next_popup_z_seq(),
                scope_surface: None,
            },
        );
        self.send_surface_request(
            plugin_id,
            protocol::METHOD_POPUP_OPEN,
            json!({
                "popup_id": popup_id,
                "instance_id": instance_id,
                "context": context,
            }),
            PendingRequestKind::PopupOpen { instance_id },
        );
        Some(instance_id)
    }

    /// 소유 확인을 기다리는 surface 범위 이벤트 팝업을 모두 꺼낸다.
    pub fn take_pending_surface_popups(&mut self) -> Vec<super::PendingSurfacePopup> {
        std::mem::take(&mut self.pending_surface_popups)
    }

    /// 팝업에 surface를 연결한다. 기존 인스턴스를 재사용할 때는 처음의 대상을 유지한다.
    pub fn bind_popup_instance_surface(&mut self, instance_id: u64, surface_id: u32) {
        if let Some(inst) = self.popup_instances.get_mut(&instance_id)
            && inst.scope_surface.is_none()
        {
            inst.scope_surface = Some(surface_id);
        }
    }

    /// 같은 플러그인·팝업의 열린 인스턴스가 있으면 기존 id를 반환한다.
    fn find_open_popup_instance(&self, plugin_id: &str, popup_id: &str) -> Option<u64> {
        let (existing, _) = self
            .popup_instances
            .iter()
            .find(|(_, inst)| inst.plugin_id == plugin_id && inst.popup_id == popup_id)?;
        tracing::debug!(
            "popup.open: '{plugin_id}/{popup_id}' already open (instance {existing}); dedup"
        );
        Some(*existing)
    }

    /// popup 인스턴스를 닫는다. plugin에 `popup.closed` fire-and-forget IPC를 보내고
    /// 호스트 측 인스턴스도 제거. 닫는 이유는 [`PopupCloseReason`] 그대로 전달.
    pub fn close_popup_instance(
        &mut self,
        instance_id: u64,
        reason: tasty_plugin_protocol::PopupCloseReason,
    ) {
        let Some(inst) = self.popup_instances.remove(&instance_id) else {
            return;
        };
        // 마지막 프레임과 호스트의 버퍼 매핑도 함께 해제한다.
        if let Some(f) = self.popup_mesh_frames.remove(&instance_id) {
            self.release_plugin_buffer(&f.plugin_id, f.buffer_id);
        }
        // plugin process가 살아있을 때만 알린다. 종료 중이면 다음 spawn에서 새 인스턴스 id로 시작.
        if self.processes.contains_key(&inst.plugin_id) {
            self.send_surface_request(
                &inst.plugin_id,
                protocol::METHOD_POPUP_CLOSED,
                json!({
                    "instance_id": instance_id,
                    "reason": reason,
                }),
                PendingRequestKind::Other,
            );
        }
    }

    /// 프로세스 없이 팝업 렌더·입력 전달을 시험하기 위한 인스턴스를 만든다.
    /// test-support 기능을 켰을 때만 제공한다.
    #[cfg(feature = "test-support")]
    pub fn insert_popup_instance_for_test(&mut self, instance_id: u64, instance: PopupInstance) {
        self.popup_instances.insert(instance_id, instance);
    }

    /// 현재 활성 popup 인스턴스 목록. PopupManager 렌더 / debug IPC가 사용.
    pub fn popup_instances(&self) -> impl Iterator<Item = (u64, &PopupInstance)> {
        self.popup_instances.iter().map(|(k, v)| (*k, v))
    }

    /// 열린 팝업을 클릭하면 z-order 순번을 갱신해 앞으로 가져온다.
    pub fn touch_popup_instance_z(&mut self, instance_id: u64) {
        if let Some(inst) = self.popup_instances.get_mut(&instance_id) {
            inst.z_seq = super::next_popup_z_seq();
        }
    }

    /// 단축키에 연결된 플러그인 명령을 실행하고 SurfaceResult 응답으로 화면을 갱신한다.
    pub fn send_command_invoke(&mut self, plugin_id: &str, surface_id: u32, command_id: &str) {
        if !self.processes.contains_key(plugin_id) {
            tracing::warn!(
                "command.invoke: plugin '{}' is not running, dropping command '{}'",
                plugin_id,
                command_id
            );
            return;
        }
        self.send_surface_request(
            plugin_id,
            protocol::METHOD_COMMAND_INVOKE,
            json!({
                "surface_id": surface_id,
                "command_id": command_id,
            }),
            PendingRequestKind::CommandInvoke {
                surface_id,
                binding: self
                    .surfaces
                    .get(&surface_id)
                    .map(|entry| entry.handles.binding()),
            },
        );
    }
}
