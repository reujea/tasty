//! Engine-scoped remote session resources. They are not local journal facts.
#[cfg(feature = "gui")]
mod explorer;

use crate::core::attach::AttachClientId;
use crate::core::state::AttentionKind;
use crate::core::state::RemoteCwd;
#[cfg(feature = "gui")]
use crate::core::state::{AttachMeshContextForward, PendingImageUpload};
use tasty_ipc::stream::{StreamFrame, StreamTag};

/// mirror workspace 가 붙은 원격 Tasty. 탐색기 진행 표시와 쓰기를 거절하는 이유에 이름을 쓴다.
#[cfg(feature = "gui")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExplorerRemoteHost {
    pub(crate) label: String,
    /// 연결돼 있지만 그 서버가 원격 쓰기를 알리지 않았다(옛 Tasty).
    pub(crate) needs_update: bool,
}

/// mirror workspace가 어디에 붙어 있는지. 상태바·핸들러 picker·링크 툴팁의 원격 위치 표시가 읽는다.
#[cfg(feature = "gui")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MirrorLocation {
    /// 호스트 글자(`ClientSessionState::host_text`). 비면 화면이 `Remote` 낱말을 쓴다.
    pub(crate) host: String,
    /// 연결이 끊겨 다시 연결하는 중이다. 화면은 원격 색을 빼고 상태어를 붙인다.
    pub(crate) reconnecting: bool,
}

/// 원격으로 보낸 캡처를 붙여넣을 수 있는 동안의 기록.
#[cfg(feature = "gui")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MirrorCapturePaste {
    /// 캡처를 시작할 때의 로컬 클립보드 지문. 달라지면 사용자가 그 뒤에 새로 복사한 것이다.
    pub(crate) clipboard_before: u64,
    /// 원격 저장 경로. 원격이 회신하기 전에는 없다.
    pub(crate) remote_path: Option<String>,
}

#[cfg(feature = "gui")]
impl MirrorCapturePaste {
    /// 캡처 뒤 로컬 클립보드가 그대로면 붙여넣을 원격 경로다.
    pub(crate) fn path_for(&self, clipboard_now: u64) -> Option<&str> {
        (self.clipboard_before == clipboard_now)
            .then_some(self.remote_path.as_deref())
            .flatten()
    }
}

/// 원격 explorer 칸의 쓰기를 거절하는 이유. 옛 Tasty 면 업데이트를 안내하고, 연결 정보가 없으면
/// (연결이 끊기는 중) 원격 탐색기에서 지원하지 않는다는 문구를 쓴다.
#[cfg(feature = "gui")]
pub(crate) fn explorer_write_refusal(host: Option<&ExplorerRemoteHost>) -> String {
    match host {
        Some(host) if host.needs_update => {
            crate::i18n::t_fmt("explorer.remote.write_needs_update", &host.label)
        }
        _ => crate::i18n::t("explorer.state.remote_write_unsupported").to_string(),
    }
}

pub(crate) struct RemoteState {
    #[cfg(feature = "gui")]
    pub(crate) html_entry_hosts: std::collections::HashSet<u32>,
    pub(crate) html_http: crate::remote::html_http_host::HttpHost,
    pub(crate) html_transfers: crate::remote::html_transfer::HtmlTransfers,
    pub(crate) attach_subscriptions:
        std::collections::HashMap<(u32, u32), super::subscription::Subscription>,
    /// 저장된 원격 행의 연결 토큰. 행을 닫거나 사용자가 끊으면 바꿔 진행 중인 연결을 낡게 만든다.
    pub(crate) row_tokens: std::collections::HashMap<u32, std::sync::Arc<()>>,
    /// 저장된 원격 행의 연결 상태 이름. App이 매 회차 채우고 `workspace.list`가 읽는다.
    pub(crate) row_states: std::collections::HashMap<u32, &'static str>,
    pub(crate) presentation: crate::model::StructurePresentationSnapshot,
    notifier: Option<tasty_ipc::stream_hub::StreamHub>,
    structure_changed: std::collections::BTreeSet<u32>,
    pub(crate) pending_workspace_taps:
        std::collections::HashMap<u32, (u32, tasty_terminal::ResourceGeneration)>,
    pub(crate) pending_structure_replies: std::collections::BTreeMap<u64, u32>,
    /// 서버의 mesh 구독 상태. 실제 전송은 PluginManager를 가진 GUI·헤드리스 계층이 맡는다.
    pub(crate) mesh_mirror: crate::remote::mesh_mirror::MeshMirrorRegistry,
    /// client가 조립한 mesh frame을 로컬 surface ID로 보관한다. 서버 구독 상태와는 별개다.
    #[cfg(feature = "gui")]
    pub(crate) attach_mesh_frames: crate::remote::mesh_frames::AttachMeshFrameStore,
    /// IPC가 요청한 GUI attach 대기열(포트, 원격 workspace, 결과를 받을 run).
    /// 처리 후 사용자의 선택을 옮기지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) pending_gui_attach: Vec<(u16, u32, u64)>,
    /// 캡처 시점에 정한 mirror workspace. None이면 로컬 클립보드에 기록한다.
    /// 캡처 도중 포커스가 바뀌어도 업로드 대상은 바뀌지 않는다.
    #[cfg(feature = "gui")]
    pub(crate) pending_screenshot_captures: Vec<(Option<u32>, std::sync::Weak<()>)>,
    /// mirror 이미지 붙여넣기 요청. App이 업로드하고 저장 경로를 미리 정한 surface로 보낸다.
    #[cfg(feature = "gui")]
    pub(crate) pending_image_uploads: Vec<PendingImageUpload>,
    /// mirror workspace별 마지막 스크린샷 단축키 결과. 붙여넣기는 로컬 클립보드를 읽으므로
    /// 원격 클립보드에만 올라간 캡처를 여기서 찾아 원격 경로를 입력한다.
    #[cfg(feature = "gui")]
    pub(crate) mirror_capture_pastes: std::collections::HashMap<u32, MirrorCapturePaste>,
    /// 로컬 터미널 이미지 붙여넣기의 저장 요청. App 이 worker 에 넘기고 완료 때 경로를 입력한다.
    #[cfg(feature = "gui")]
    pub(crate) pending_image_saves: Vec<crate::app::image_jobs::paste::PendingImageSave>,
    /// GUI·헤드리스 attach 서버가 공유하는 캡처 업로드 버퍼.
    pub(crate) capture_uploads: crate::remote::capture_upload::CaptureUploadRegistry,
    /// 전용 bulk 연결의 (client_id, transfer_id)별 메타데이터·바이트 버퍼.
    pub(crate) bulk_transfers: crate::remote::bulk_transfer::BulkTransferRegistry,
    /// 원격 실행 요청과 사용자 선택 보정 태그. anchor는 로컬 ID이며 전송 직전에 원격 ID로 바꾼다.
    /// 로컬 mirror ID별 최신 resize 목표. 로컬에 먼저 적용하지 않고 서버 echo를 기다린다.
    #[cfg(feature = "gui")]
    pub(crate) pending_resize_forward: std::collections::HashMap<u32, (usize, usize)>,
    /// mirror에서 실제 attention을 지웠을 때의 로컬 ID를 모아 서버에 해제를 요청한다.
    pub(crate) pending_attention_clear_forward: std::collections::HashSet<u32>,
    /// 원격 디렉터리 조회 요청만 담는다. 응답은 MirrorEvent로 따로 들어온다.
    #[cfg(feature = "gui")]
    pub(crate) pending_list_dir_forward: Vec<crate::core::PendingListDirForward>,
    /// 서버가 원격 explorer 파일 작업을 받는 mirror workspace(로컬 ID). App 이 연결 상태에서 맞춘다.
    #[cfg(feature = "gui")]
    pub(crate) explorer_writable: std::collections::HashSet<u32>,
    /// mirror workspace(로컬 ID)마다 붙은 원격 Tasty. App 이 연결 상태에서 맞춘다.
    #[cfg(feature = "gui")]
    pub(crate) explorer_hosts: std::collections::HashMap<u32, ExplorerRemoteHost>,
    /// mirror workspace(로컬 ID)마다 원격 위치 표시 값. App 이 연결 상태에서 맞춘다.
    #[cfg(feature = "gui")]
    pub(crate) mirror_locations: std::collections::HashMap<u32, MirrorLocation>,
    /// 마지막 서버 descriptor와 연결. 클라이언트의 탐색 경로와 분리한다.
    #[cfg(feature = "gui")]
    pub(crate) explorer_sources: explorer::MirrorExplorers,
    /// 원격 Git 조회 요청. 응답은 MirrorEvent로 따로 들어온다.
    #[cfg(feature = "gui")]
    pub(crate) pending_git_query_forward: Vec<crate::core::PendingGitQueryForward>,
    /// 원격 markdown 원문 조회 요청. 응답은 MirrorEvent로 따로 들어온다.
    #[cfg(feature = "gui")]
    pub(crate) pending_markdown_content_forward: Vec<crate::core::PendingMarkdownContentForward>,
    /// texture 복구가 필요한 로컬 surface ID. 전송 때 원격 ID로 바꾼다.
    #[cfg(feature = "gui")]
    pub(crate) pending_mesh_full_resend_forward: std::collections::HashSet<u32>,
    /// 로컬 mesh surface별 최신 geometry·theme·focus. 같은 surface의 변경은 합친다.
    #[cfg(feature = "gui")]
    pub(crate) pending_mesh_context_forward:
        std::collections::HashMap<u32, AttachMeshContextForward>,
    /// 로컬 mesh surface별 누적 입력. App이 원격으로 보낸다.
    #[cfg(feature = "gui")]
    pub(crate) pending_mesh_input_forward: std::collections::HashMap<
        u32,
        (
            std::time::Instant,
            tasty_plugin_protocol::protocol::RawInputWire,
        ),
    >,
    /// 원격에서 받은 busy 상태. 로컬 폴링이 집합을 교체하므로 별도로 보관한다.
    pub(crate) mirror_busy_surfaces: std::collections::HashSet<u32>,
    /// 원격 surface의 cwd. 두 호스트의 파일시스템이 달라 로컬 Path로 해석하지 않는다.
    pub(crate) mirror_surface_cwd: std::collections::HashMap<u32, RemoteCwd>,
    /// 원격 surface의 전경 프로세스 이름과 세대. 로컬 폴링이 덮어쓰지 않도록 별도로 보관한다.
    pub(crate) mirror_foreground: crate::core::state::MirrorForegrounds,
    /// busy 전송 후보를 마지막으로 만든 (holder, 값). 송신 성공 기록은 아니다.
    /// holder도 비교해야 같은 값으로 점유자가 바뀌어도 새 client에 초기 상태를 보낸다.
    pub(crate) last_forwarded_busy:
        std::collections::HashMap<u32, (crate::core::attach::AttachClientId, bool)>,
    /// attention 전송 후보의 (holder, kind). None은 해제이며 송신 성공과는 별개다.
    pub(crate) last_forwarded_attention: std::collections::HashMap<
        u32,
        (crate::core::attach::AttachClientId, Option<AttentionKind>),
    >,
    /// cwd 전송 후보의 (holder, 값). 같은 값이어도 holder가 바뀌면 새 후보를 만든다.
    pub(crate) last_forwarded_cwd:
        std::collections::HashMap<u32, (crate::core::attach::AttachClientId, Option<String>)>,
    /// 전경 이름 전송 후보의 (holder, 값). 같은 값이어도 holder가 바뀌면 새 후보를 만든다.
    pub(crate) last_forwarded_foreground:
        std::collections::HashMap<u32, (crate::core::attach::AttachClientId, Option<String>)>,
    /// workspace별로 holder에 보낸 마지막 트리·surface 직렬화 값. 구조 변경 표시는 다른
    /// workspace의 확정에도 찍히므로, 같은 holder에 같은 트리를 다시 보내지 않는 데 쓴다.
    last_structure_sent: std::collections::HashMap<u32, (AttachClientId, Vec<u8>)>,
}
impl RemoteState {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(feature = "gui")]
            html_entry_hosts: Default::default(),
            html_transfers: Default::default(),
            html_http: Default::default(),
            attach_subscriptions: Default::default(),
            row_tokens: Default::default(),
            row_states: Default::default(),
            presentation: Default::default(),
            notifier: None,
            structure_changed: Default::default(),
            pending_workspace_taps: Default::default(),
            pending_structure_replies: Default::default(),
            mesh_mirror: crate::remote::mesh_mirror::MeshMirrorRegistry::default(),
            #[cfg(feature = "gui")]
            attach_mesh_frames: crate::remote::mesh_frames::AttachMeshFrameStore::default(),
            #[cfg(feature = "gui")]
            pending_gui_attach: Vec::new(),
            #[cfg(feature = "gui")]
            pending_screenshot_captures: Vec::new(),
            #[cfg(feature = "gui")]
            pending_image_uploads: Vec::new(),
            #[cfg(feature = "gui")]
            pending_image_saves: Vec::new(),
            #[cfg(feature = "gui")]
            mirror_capture_pastes: std::collections::HashMap::new(),
            capture_uploads: crate::remote::capture_upload::CaptureUploadRegistry::new(),
            bulk_transfers: crate::remote::bulk_transfer::BulkTransferRegistry::new(),
            #[cfg(feature = "gui")]
            pending_resize_forward: std::collections::HashMap::new(),
            pending_attention_clear_forward: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            pending_list_dir_forward: Vec::new(),
            #[cfg(feature = "gui")]
            explorer_writable: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            explorer_hosts: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            mirror_locations: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            explorer_sources: Default::default(),
            #[cfg(feature = "gui")]
            pending_git_query_forward: Vec::new(),
            #[cfg(feature = "gui")]
            pending_markdown_content_forward: Vec::new(),
            #[cfg(feature = "gui")]
            pending_mesh_full_resend_forward: std::collections::HashSet::new(),
            #[cfg(feature = "gui")]
            pending_mesh_context_forward: std::collections::HashMap::new(),
            #[cfg(feature = "gui")]
            pending_mesh_input_forward: std::collections::HashMap::new(),
            mirror_busy_surfaces: std::collections::HashSet::new(),
            mirror_surface_cwd: std::collections::HashMap::new(),
            mirror_foreground: Default::default(),
            last_forwarded_busy: std::collections::HashMap::new(),
            last_forwarded_attention: std::collections::HashMap::new(),
            last_forwarded_cwd: std::collections::HashMap::new(),
            last_forwarded_foreground: std::collections::HashMap::new(),
            last_structure_sent: std::collections::HashMap::new(),
        }
    }
}

impl RemoteState {
    pub(crate) fn set_notifier(&mut self, hub: tasty_ipc::stream_hub::StreamHub) {
        self.notifier = Some(hub);
    }
    pub(crate) fn notifier(&self) -> Option<tasty_ipc::stream_hub::StreamHub> {
        self.notifier.clone()
    }
    pub(crate) fn structure_reply_pending(&self, workspace: u32) -> bool {
        self.pending_structure_replies
            .values()
            .any(|value| *value == workspace)
    }
    pub(crate) fn mark_structure_changed(&mut self, id: u32) {
        self.structure_changed.insert(id);
    }
    pub(crate) fn clear_structure_changed(&mut self, id: u32) {
        self.structure_changed.remove(&id);
    }
    /// `snapshot`은 `(tree, surfaces)`의 직렬화 값이다. 초기 attach와 두 delta 경로가 같은 형식을 쓴다.
    pub(crate) fn structure_already_sent(
        &self,
        workspace: u32,
        holder: AttachClientId,
        snapshot: &[u8],
    ) -> bool {
        self.last_structure_sent
            .get(&workspace)
            .is_some_and(|(sent_to, sent)| *sent_to == holder && sent == snapshot)
    }
    pub(crate) fn record_structure_sent(
        &mut self,
        workspace: u32,
        holder: AttachClientId,
        snapshot: Vec<u8>,
    ) {
        self.last_structure_sent
            .insert(workspace, (holder, snapshot));
    }
    pub(crate) fn forget_structure_sent(&mut self, workspace: u32) {
        self.last_structure_sent.remove(&workspace);
    }
    pub(crate) fn forget_client_structures(&mut self, holder: AttachClientId) {
        self.last_structure_sent
            .retain(|_, (sent_to, _)| *sent_to != holder);
    }
    pub(crate) fn take_structure_changed(&mut self) -> Vec<u32> {
        let (held, ready): (Vec<_>, Vec<_>) = std::mem::take(&mut self.structure_changed)
            .into_iter()
            .partition(|id| self.structure_reply_pending(*id));
        self.structure_changed.extend(held);
        ready
    }
    /// Control 사유와 Detach를 차례로 push한다. 허브가 없거나 송신에 실패해도 점유 해제는 되돌리지 않는다.
    pub(crate) fn notify_detached(&self, holder: AttachClientId, reason: &str) {
        let Some(hub) = &self.notifier else {
            return;
        };
        let msg = serde_json::json!({ "event": "force_detached", "reason": reason });
        let payload = serde_json::to_vec(&msg).unwrap_or_default();
        let _ = hub.push(holder, StreamFrame::new(StreamTag::Control, payload)); // 손실·연결 종료는 허브에 맡기며 여기서 재시도하지 않는다.
        let _ = hub.push(holder, StreamFrame::new(StreamTag::Detach, Vec::new())); // 송신 실패에도 점유 해제는 유지한다.
    }
}

#[cfg(feature = "gui")]
impl RemoteState {
    /// This runs on the App thread before exposing a replacement connection. Queued local-ID
    /// requests still refer to the retired mapping and must not be resolved through the new one.
    pub(crate) fn discard_connection_requests(
        &mut self,
        mapping: &std::collections::HashMap<u32, u32>,
        workspace: u32,
    ) {
        let ids: std::collections::HashSet<u32> = mapping.values().copied().collect();
        self.pending_resize_forward
            .retain(|id, _| !ids.contains(id));
        self.pending_mesh_context_forward
            .retain(|id, _| !ids.contains(id));
        self.pending_mesh_input_forward
            .retain(|id, _| !ids.contains(id));
        self.pending_mesh_full_resend_forward
            .retain(|id| !ids.contains(id));
        self.pending_attention_clear_forward
            .retain(|id| !ids.contains(id));
        self.pending_list_dir_forward
            .retain(|request| request.local_ws_id != workspace);
        self.pending_git_query_forward
            .retain(|request| !ids.contains(&request.local_surface_id));
        self.pending_markdown_content_forward
            .retain(|request| !ids.contains(&request.local_surface_id));
        self.pending_image_uploads
            .retain(|request| request.mirror_ws_id != workspace);
        self.pending_screenshot_captures
            .retain(|(target, _)| *target != Some(workspace));
    }
}

impl RemoteState {
    /// Dispose only ephemeral observations and queued sends for a removed local identity.
    /// Connection ownership and other surfaces in the same session remain intact.
    pub(crate) fn forget_surface_observations(&mut self, id: u32) {
        self.attach_subscriptions
            .retain(|(surface, _), _| *surface != id);
        self.mesh_mirror.remove(id);
        self.last_forwarded_busy.remove(&id);
        self.last_forwarded_attention.remove(&id);
        self.last_forwarded_cwd.remove(&id);
        self.last_forwarded_foreground.remove(&id);
        self.pending_attention_clear_forward.remove(&id);
        #[cfg(feature = "gui")]
        {
            self.explorer_sources.remove(id);
            self.pending_resize_forward.remove(&id);
            self.pending_mesh_context_forward.remove(&id);
            self.pending_mesh_input_forward.remove(&id);
            self.pending_mesh_full_resend_forward.remove(&id);
            self.pending_git_query_forward
                .retain(|request| request.local_surface_id != id);
            self.pending_markdown_content_forward
                .retain(|request| request.local_surface_id != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteState;

    #[test]
    fn a_sent_structure_is_skipped_only_for_the_same_holder_and_value() {
        let mut remote = RemoteState::new();
        assert!(!remote.structure_already_sent(7, 1, b"tree-a"));
        remote.record_structure_sent(7, 1, b"tree-a".to_vec());
        assert!(remote.structure_already_sent(7, 1, b"tree-a"));
        assert!(!remote.structure_already_sent(7, 1, b"tree-b"));
        assert!(!remote.structure_already_sent(7, 2, b"tree-a"));
        assert!(!remote.structure_already_sent(8, 1, b"tree-a"));
        remote.record_structure_sent(8, 2, b"tree-b".to_vec());
        remote.forget_client_structures(1);
        assert!(!remote.structure_already_sent(7, 1, b"tree-a"));
        assert!(remote.structure_already_sent(8, 2, b"tree-b"));
        remote.forget_structure_sent(8);
        assert!(!remote.structure_already_sent(8, 2, b"tree-b"));
    }
}
