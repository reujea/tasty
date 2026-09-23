//! GPU shared buffer 매핑 관리. plugin 별 buffer id 발급 + dirty rect drain.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use tasty_plugin_protocol::HandleChannelMessage;
use tasty_plugin_protocol::{SharedBufferCreateResult, SharedBufferId};
use tasty_shm::PeerPid;
use tasty_shm::SharedMemory;

use super::PluginManager;

impl PluginManager {
    pub fn take_plugin_dirty_rects(
        &self,
        plugin_id: &str,
    ) -> HashMap<SharedBufferId, Option<tasty_plugin_protocol::PixelRect>> {
        self.processes
            .get(plugin_id)
            .map(|p| p.take_dirty_rects())
            .unwrap_or_default()
    }

    /// (plugin_id, buffer_id)에 해당하는 호스트 측 매핑된 [`SharedMemory`] 참조.
    /// 캔버스 렌더 시 atomic generation + user data를 읽기 위해 사용.
    pub fn plugin_buffer(
        &self,
        plugin_id: &str,
        buffer_id: SharedBufferId,
    ) -> Option<&SharedMemory> {
        self.plugin_buffers.get(plugin_id)?.get(&buffer_id)
    }

    /// surface/popup/banner 닫힘 시 그 인스턴스가 쓰던 shared buffer 매핑을 host 측
    /// 에서 해제한다. plugin 은 자기 매핑을 drop 하지만 host 에 알릴 프로토콜
    /// 메시지가 없어, 이 호출이 없으면 `plugin_buffers` 의 `SharedMemory` 매핑이
    /// plugin 수명 내내 누적된다 (soak s6 실측: markdown open/close 당 호스트
    /// RSS ~1.1MB — Rust heap 밖(mmap)이라 dhat 에도 안 잡히는 유형).
    /// plugin 측 매핑은 독립 뷰라 이 해제와 무관하게 유효하다.
    pub(super) fn release_plugin_buffer(
        &mut self,
        plugin_id: &str,
        buffer_id: super::SharedBufferId,
    ) {
        if let Some(bufs) = self.plugin_buffers.get_mut(plugin_id) {
            bufs.remove(&buffer_id);
        }
    }

    /// egui-mesh surface 의 최근 paint_frame 메타 조회 (A1-S3 수신 라우팅 골격).
    ///
    /// 렌더 prepare(A1-S5)가 호출해 `buffer_id` 로 [`PluginManager::plugin_buffer`] 를
    /// lookup → footer Acquire-load → `mesh_wire::decode_paint` 로 mesh 를 복원한다.
    /// plugin 이 아직 frame 을 보내지 않았거나 죽어서 정리됐으면 `None`.
    pub fn egui_mesh_frame(&self, surface_id: u32) -> Option<&super::EguiMeshFrame> {
        self.egui_mesh_frames.get(&surface_id)
    }

    /// 캐시된 egui-mesh frame 을 버린다 — 다음 `forward_egui_mesh_context` 에서
    /// `has_frame == false` 가 되어 재-bootstrap(`surface.create` 재발송)을 유발한다.
    /// markdown 제자리 이동(`markdown.navigate`)처럼 같은 surface_id 를 새 콘텐츠 params 로 다시 열 때
    /// stale frame 이 남지 않게 호출한다.
    pub fn drop_egui_mesh_frame(&mut self, surface_id: u32) {
        self.egui_mesh_frames.remove(&surface_id);
    }

    /// egui-mesh popup 인스턴스의 최근 paint_frame 메타 조회 (A2). 호스트 popup
    /// 합성기가 `instance_id` 로 lookup → `buffer_id` 로 [`PluginManager::plugin_buffer`]
    /// → footer Acquire-load → `decode_paint` 로 mesh 를 복원한다. plugin 이 아직 frame 을
    /// 보내지 않았거나 popup 이 닫혀 정리됐으면 `None`.
    pub fn popup_mesh_frame(&self, instance_id: u64) -> Option<&super::EguiMeshFrame> {
        self.popup_mesh_frames.get(&instance_id)
    }

    /// egui-mesh banner 인스턴스의 최근 paint_frame 메타 조회 (A3). [`popup_mesh_frame`]
    /// 의 banner 대응 — 호스트 banner 합성기가 `instance_id` 로 lookup 한다. plugin 이 아직
    /// frame 을 보내지 않았거나 banner 가 닫혀 정리됐으면 `None`.
    ///
    /// [`popup_mesh_frame`]: Self::popup_mesh_frame
    pub fn banner_mesh_frame(&self, instance_id: u64) -> Option<&super::EguiMeshFrame> {
        self.banner_mesh_frames.get(&instance_id)
    }

    /// `host.shared_buffer.create` 처리. 새 공유 메모리 영역을 만들어
    /// 메인 채널 결과(`SharedBufferCreateResult`)와 보조 채널 핸들(`HandleAttach`)을
    /// 양쪽 모두 전송한다.
    ///
    /// - main 채널 응답은 caller(`App::process_plugin_ipc_calls`)가
    ///   `send_ipc_result`로 회신.
    /// - 보조 채널 핸들은 본 메서드가 직접 송신 (plugin SDK는 같은 call_id로
    ///   매칭되는 `HandleAttach`를 기다린다).
    ///
    /// 핸들 전송이 실패하면 SharedMemory를 등록하지 않고 에러를 반환한다 — plugin은
    /// `PluginError::HostCallTimeout` 등으로 인식한다.
    #[cfg(unix)]
    pub fn create_shared_buffer_for(
        &mut self,
        plugin_id: &str,
        call_id: u64,
        size: u64,
    ) -> Result<SharedBufferCreateResult, String> {
        // TODO(권한모델): manifest 권한 도입 후 plugin별 permissions.max_shared_buffer_bytes 로 대체.
        const MAX_BYTES: u64 = 1 << 30; // 1 GiB. manifest 권한 도입 전 임시 상한.
        if size == 0 {
            return Err("shared_buffer.create: size must be > 0".into());
        }
        if size > MAX_BYTES {
            return Err(format!(
                "shared_buffer.create: size {size} exceeds host cap {MAX_BYTES}"
            ));
        }
        let proc = self
            .processes
            .get(plugin_id)
            .ok_or_else(|| format!("plugin '{plugin_id}' is not running"))?;
        // 보조 채널이 없으면 핸들 전송이 불가능. 즉시 거절.
        if self.handle_listener.is_none() {
            return Err("shared_buffer.create: host handle channel not available".into());
        }

        let (mem, sendable) = tasty_shm::create(size as usize)
            .map_err(|e| format!("shared_buffer.create: tasty_shm::create failed: {e}"))?;
        // Unix는 peer pid를 무시하지만 의도 명시를 위해 child pid를 넘긴다.
        let peer = match proc.child_pid() {
            Some(pid) => PeerPid::Other(pid),
            None => PeerPid::Same,
        };
        let payload = tasty_shm::prepare_send(sendable, peer)
            .map_err(|e| format!("shared_buffer.create: prepare_send failed: {e}"))?;

        let id = SharedBufferId(self.next_buffer_id.fetch_add(1, Ordering::Relaxed));
        let actual_size = mem.len() as u64;
        let msg = HandleChannelMessage::HandleAttach {
            request_id: call_id,
            id,
            size: actual_size,
            // Unix는 fd가 ancillary data로 동행 — handle 필드는 사용하지 않는다.
            handle: None,
        };
        let raw_fd = payload.raw_fd();
        let send_result = proc.with_handle_stream(|stream| stream.send_handle(&msg, raw_fd));
        match send_result {
            Some(Ok(())) => {}
            Some(Err(e)) => {
                return Err(format!(
                    "shared_buffer.create: handle channel send failed: {e}"
                ));
            }
            None => {
                return Err("shared_buffer.create: plugin handle channel not connected".into());
            }
        }
        // SharedMemory를 매니저가 보관 — Drop이 일어나면 OS region이 회수되므로
        // plugin이 매핑을 잡고 있는 한 살아있어야 한다. payload는 method 끝에서
        // Drop되며 송신 fd가 닫힌다(매핑 fd는 mem 안에 별도로 보존).
        self.plugin_buffers
            .entry(plugin_id.to_string())
            .or_default()
            .insert(id, mem);
        drop(payload);
        Ok(SharedBufferCreateResult {
            id,
            size: actual_size,
        })
    }

    /// Windows: Unix 판과 동형이되 핸들 전달 방식만 다르다. `tasty_shm::prepare_send`
    /// 의 `DuplicateHandle` 이 plugin 프로세스(child pid) 핸들 테이블에 파일 매핑 핸들을
    /// 복제해 넣고, 그 결과 HANDLE u64 를 [`HandleAttach`] 의 `handle` 필드에 in-band 로
    /// 실어 보낸다(ancillary data 없음). 매핑(`SharedMemory`)은 매니저가 보관한다.
    ///
    /// [`HandleAttach`]: HandleChannelMessage::HandleAttach
    #[cfg(windows)]
    pub fn create_shared_buffer_for(
        &mut self,
        plugin_id: &str,
        call_id: u64,
        size: u64,
    ) -> Result<SharedBufferCreateResult, String> {
        // TODO(권한모델): manifest 권한 도입 후 plugin별 permissions.max_shared_buffer_bytes 로 대체.
        const MAX_BYTES: u64 = 1 << 30; // 1 GiB. manifest 권한 도입 전 임시 상한.
        if size == 0 {
            return Err("shared_buffer.create: size must be > 0".into());
        }
        if size > MAX_BYTES {
            return Err(format!(
                "shared_buffer.create: size {size} exceeds host cap {MAX_BYTES}"
            ));
        }
        let proc = self
            .processes
            .get(plugin_id)
            .ok_or_else(|| format!("plugin '{plugin_id}' is not running"))?;
        if self.handle_listener.is_none() {
            return Err("shared_buffer.create: host handle channel not available".into());
        }
        // Windows 는 DuplicateHandle 대상 프로세스를 pid 로 특정해야 한다. child pid 가
        // 없으면(shutdown 등) 핸들을 복제할 수 없다.
        let child_pid = proc
            .child_pid()
            .ok_or_else(|| "shared_buffer.create: plugin child pid unavailable".to_string())?;

        let (mem, sendable) = tasty_shm::create(size as usize)
            .map_err(|e| format!("shared_buffer.create: tasty_shm::create failed: {e}"))?;
        // 파일 매핑 핸들을 plugin 프로세스 핸들 테이블에 복제한다.
        let payload = tasty_shm::prepare_send(sendable, PeerPid::Other(child_pid))
            .map_err(|e| format!("shared_buffer.create: prepare_send failed: {e}"))?;
        let dup_handle = payload.serialized_handle();

        let id = SharedBufferId(self.next_buffer_id.fetch_add(1, Ordering::Relaxed));
        let actual_size = mem.len() as u64;
        let msg = HandleChannelMessage::HandleAttach {
            request_id: call_id,
            id,
            size: actual_size,
            // send_handle 이 인자 handle 로 덮어쓰므로 여기선 None 이어도 무방.
            handle: None,
        };
        let send_result = proc.with_handle_stream(|stream| stream.send_handle(&msg, dup_handle));
        match send_result {
            Some(Ok(())) => {}
            Some(Err(e)) => {
                return Err(format!(
                    "shared_buffer.create: handle channel send failed: {e}"
                ));
            }
            None => {
                return Err("shared_buffer.create: plugin handle channel not connected".into());
            }
        }
        // 우리 매핑은 매니저가 보관 — plugin 이 매핑을 잡고 있는 한 살아있어야 한다.
        // payload(peer 테이블의 복제 핸들)는 Drop 이 no-op 라 그대로 떨궈도 된다.
        self.plugin_buffers
            .entry(plugin_id.to_string())
            .or_default()
            .insert(id, mem);
        drop(payload);
        Ok(SharedBufferCreateResult {
            id,
            size: actual_size,
        })
    }
}
