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
        let proc = match self.processes.get(plugin_id) {
            Some(p) => p,
            None => return,
        };
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let req = PluginRequest::new(method, params, id);
        // 실패를 조용히 삼키면 popup 이 안 열리는 사유가 어디에도 안 남는다. 무제한
        // 채널일 때는 여기서 실패한다는 것이 곧 "plugin 이 죽었다" 였지만, 유한해진
        // 뒤로는 **일시적 포화**도 같은 갈래로 떨어진다 — 둘을 가르는 문장이
        // `RequestSendError` 의 `Display` 에 있고, 그것이 로그에 닿아야 ADR-0315 의
        // 재검토 조건("정상 사용이 용량에 닿는가")을 잴 수 있다.
        match proc.try_send_request(req) {
            Ok(()) => {
                self.pending_requests
                    .insert(id, PendingRequest::now(plugin_id, kind));
            }
            Err(e) => {
                tracing::warn!("plugin '{plugin_id}' {method} send failed: {e}");
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

    /// 열린 popup 인스턴스에 소속 surface 를 바인딩한다. 이미 바인딩돼 있으면 그대로 둔다 —
    /// 단일 인스턴스 가드로 기존 인스턴스가 재사용될 때 plugin 이 받은 open context 의
    /// 대상과 host 가 그리는 범위가 갈리지 않게 하려는 것이다.
    pub fn bind_popup_instance_surface(&mut self, instance_id: u64, surface_id: u32) {
        if let Some(inst) = self.popup_instances.get_mut(&instance_id)
            && inst.scope_surface.is_none()
        {
            inst.scope_surface = Some(surface_id);
        }
    }

    /// 단일 인스턴스 가드: 같은 (plugin_id, popup_id) 인스턴스가 이미 열려 있으면 그 id.
    /// 사용자 조작(popup open)이 중복 인스턴스를 쌓지 않게 하는 host 소유 정책
    /// (identity 원칙 1 — 셸 생명주기는 host 소유).
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

    /// 매니페스트에서 contribute 를 찾고 egui-mesh api_version 게이트까지 통과해야 `Some`.
    /// egui-mesh popup 은 epaint 와이어가 host·plugin 동일 컴파일을 강제하므로
    /// api_version 일치를 게이트한다(surface egui-mesh 등록 정책 미러, ADR-0028).
    fn resolve_open_popup_contribute(
        &self,
        plugin_id: &str,
        popup_id: &str,
    ) -> Option<tasty_plugin_manifest::PopupContribute> {
        let pkg = self.packages.iter().find(|p| p.manifest.id == plugin_id)?;
        let contribute = pkg
            .manifest
            .contributes
            .popup
            .iter()
            .find(|p| p.id == popup_id)
            .cloned()?;
        if contribute.rendering == tasty_plugin_manifest::PopupRendering::EguiMesh
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
        // egui-mesh popup 이면 합성기가 참조하던 frame 메타를 함께 정리 (stale buffer 방지).
        // 그 frame 의 shared buffer 매핑도 해제 — 안 지우면 plugin 수명 내내 host 에
        // 누적된다 (`release_plugin_buffer` 문서 참조).
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

    /// plugin 프로세스 없이 popup 인스턴스를 하나 세운다 — 호스트 크레이트의 시험이 popup 렌더
    /// 루프(입력 forward 와 그 자리의 사용자 활성화 기록)를 실제로 돌리려고 쓴다. 정식 경로
    /// [`Self::open_popup_instance`] 는 실행 중인 plugin 이 있어야 인스턴스를 만든다.
    ///
    /// `test-support` feature 뒤에만 있다. 루트 크레이트는 그 feature 를 dev-dependency 로만
    /// 켜므로 정규(lib/bin/release) 빌드에는 이 함수가 없다.
    #[cfg(feature = "test-support")]
    pub fn insert_popup_instance_for_test(&mut self, instance_id: u64, instance: PopupInstance) {
        self.popup_instances.insert(instance_id, instance);
    }

    /// 현재 활성 popup 인스턴스 목록. PopupManager 렌더 / debug IPC가 사용.
    pub fn popup_instances(&self) -> impl Iterator<Item = (u64, &PopupInstance)> {
        self.popup_instances.iter().map(|(k, v)| (*k, v))
    }

    /// popup 콘텐츠 영역 클릭 시 z-order 순번을 갱신해 맨 앞으로 가져온다
    /// (`docs/design/systems/popup.md` 규칙 7 "클릭된 것이 앞"). 인스턴스가 이미
    /// 닫혔으면 조용히 무시.
    pub fn touch_popup_instance_z(&mut self, instance_id: u64) {
        if let Some(inst) = self.popup_instances.get_mut(&instance_id) {
            inst.z_seq = super::next_popup_z_seq();
        }
    }

    /// 단계 G: 사용자 단축키 매칭으로 plugin command를 trigger. 응답은
    /// `SurfaceResult` 형태로 받아 tree/display_name을 갱신할 수 있다.
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
            PendingRequestKind::CommandInvoke { surface_id },
        );
    }
}
