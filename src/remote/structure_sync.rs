//! forward 외의 구조 변경을 attach holder에 보내고, 찾지 못한 forward 대상의 오류를 정한다.

use crate::core::attach::AttachClientId;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_access::EngineRef;
use tasty_ipc::stream::StructuralOp;
use tasty_ipc::stream::{StreamFrame, StreamTag};
use tasty_ipc::stream_hub::PushResult;

/// wire 오류 설명이며 client가 원문을 표시한다. 서버에서 UI 번역을 적용하지 않는다.
fn workspace_not_found_reason() -> String {
    "workspace not found".to_string()
}

/// client가 이 engine의 실제 workspace를 점유하면 공통 surface 오류를 만든다.
/// anchor가 살아 있는지는 호출자가 모든 engine에서 먼저 확인해야 한다.
fn unresolved_anchor_reason(
    engine: &EngineRef<'_>,
    client_id: AttachClientId,
    op: &StructuralOp,
) -> Option<String> {
    let ws = engine.live.occupancy.workspace_held_by(client_id)?;
    engine.find_workspace_index_for_id(ws)?;
    Some(crate::core::request_target::unowned_target_message(
        crate::core::request_target::ResourceId {
            kind: crate::core::request_target::Kind::Surface,
            id: u64::from(op.anchor_surface_id()),
        },
        &format!("structural_op.{}", op.wire_kind()),
    ))
}

/// 모든 engine에서 anchor를 먼저 찾는다. 하나라도 살아 있으면 surface가 사라졌다고 안내하지 않는다.
/// 어디에도 없고 점유한 workspace가 남아 있으면 surface 오류, 그 외에는 workspace 오류다.
pub(crate) fn unresolved_forward_reason<'a>(
    engines: impl IntoIterator<Item = EngineRef<'a>>,
    client_id: AttachClientId,
    op: &StructuralOp,
) -> String {
    let engines: Vec<EngineRef<'a>> = engines.into_iter().collect();
    let anchor = op.anchor_surface_id();
    if engines
        .iter()
        .any(|e| e.find_workspace_index_for_surface(anchor).is_some())
    {
        return workspace_not_found_reason();
    }
    engines
        .into_iter()
        .find_map(|e| unresolved_anchor_reason(&e, client_id, op))
        .unwrap_or_else(workspace_not_found_reason)
}

impl EngineMut<'_> {
    pub(crate) fn flush_committed_workspace_taps(
        &mut self,
        workspace: u32,
        holder: u32,
        hub: &tasty_ipc::stream_hub::StreamHub,
    ) {
        if !self.live.occupancy.workspace_attachment_ready(workspace) {
            return;
        }
        let targets: Vec<_> = self
            .remote
            .pending_workspace_taps
            .iter()
            .filter(|(_, value)| value.0 == workspace)
            .map(|(id, value)| (*id, value.1))
            .collect();
        for (surface, generation) in targets {
            self.remote.pending_workspace_taps.remove(&surface);
            if self.live.occupancy.workspace_holder(workspace) == Some(holder)
                && self
                    .runtime
                    .terminals
                    .matches_generation(surface, generation)
                && self
                    .find_workspace_index_for_surface(surface)
                    .and_then(|(index, _)| self.workspace_at(index))
                    .is_some_and(|value| value.id == workspace)
            {
                self.tap_surface_for_stream(surface, holder, hub);
            }
        }
    }

    /// 변경 표시가 있는 workspace의 전체 트리를 holder에 보낸다. 사라진 workspace는 강제 분리한다.
    /// forward 실행은 자체 delta를 보내고 표시를 지워 중복 통지를 피한다.
    /// 같은 holder에 마지막으로 보낸 트리와 같으면 delta를 생략한다.
    /// 송신 큐가 가득 차면 변경 표시를 다시 쌓고, 사라진 연결에는 재시도하지 않는다.
    pub(crate) fn push_structure_changes(&mut self) {
        for ws_id in self.remote.take_structure_changed() {
            let Some(holder) = self.live.occupancy.workspace_holder(ws_id) else {
                self.remote.forget_structure_sent(ws_id);
                continue;
            };
            if !self.live.occupancy.workspace_attachment_ready(ws_id) {
                self.remote.mark_structure_changed(ws_id);
                continue;
            }
            let Some(idx) = self.find_workspace_index_for_id(ws_id) else {
                self.remote.forget_structure_sent(ws_id);
                self.detach_missing_workspace(ws_id);
                continue;
            };
            let Some(hub) = self.remote.notifier() else {
                continue;
            };
            let class = self.classify_attach_surfaces(
                self.workspace_at(idx).expect("workspace index is valid").id,
            );
            let (tree, surfaces) = self.build_workspace_tree_surfaces(idx, &class);
            let snapshot = serde_json::to_vec(&(&tree, &surfaces)).unwrap_or_default();
            // holder가 이미 같은 트리를 받았다면 다시 보내지 않고 받은 것으로 보아 새 surface의 tap만 연다.
            let pushed = if self.remote.structure_already_sent(ws_id, holder, &snapshot) {
                PushResult::Sent
            } else {
                let delta = tasty_ipc::stream::StreamControl::StructuralDelta {
                    workspace_id: ws_id,
                    tree,
                    surfaces,
                };
                let frame = StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&delta).unwrap_or_default(),
                );
                let pushed = hub.push(holder, frame);
                if pushed == PushResult::Sent {
                    self.remote.record_structure_sent(ws_id, holder, snapshot);
                }
                pushed
            };
            match pushed {
                PushResult::Sent => self.flush_committed_workspace_taps(ws_id, holder, &hub),
                PushResult::Dropped => self.remote.mark_structure_changed(ws_id),
                PushResult::Unknown | PushResult::Disconnected => {
                    self.remote
                        .pending_workspace_taps
                        .retain(|_, value| value.0 != ws_id);
                    tracing::debug!(
                        "structure change: holder {holder} of workspace {ws_id} is gone"
                    );
                }
            }
        }
    }
}
