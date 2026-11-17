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

    /// 닫은 surface·popup·banner의 호스트 공유 매핑을 해제한다.
    /// plugin의 매핑 해제는 별도로 통지되지 않으며 양쪽 매핑의 수명은 독립적이다.
    pub(super) fn release_plugin_buffer(
        &mut self,
        plugin_id: &str,
        buffer_id: super::SharedBufferId,
    ) {
        if let Some(bufs) = self.plugin_buffers.get_mut(plugin_id) {
            bufs.remove(&buffer_id);
        }
    }

    /// surface의 최근 프레임 메타데이터. 아직 받지 않았거나 정리했으면 None이다.
    pub fn egui_mesh_frame(&self, surface_id: u32) -> Option<&super::EguiMeshFrame> {
        self.egui_mesh_frames.get(&surface_id)
    }

    /// 캐시된 egui-mesh frame 을 버린다 — 다음 `forward_egui_mesh_context` 에서
    /// `has_frame == false` 가 되어 재-bootstrap(`surface.create` 재발송)을 유발한다.
    /// markdown 제자리 이동(`markdown.navigate`)처럼 같은 surface_id 를 새 콘텐츠 params 로 다시 열 때
    /// stale frame 이 남지 않게 호출한다.
    pub fn drop_egui_mesh_frame(&mut self, surface_id: u32) {
        self.egui_mesh_frames.remove(&surface_id);
        self.fresh_frame_surfaces.remove(&surface_id);
    }

    /// popup의 최근 프레임 메타데이터. 아직 받지 않았거나 닫혔으면 None이다.
    pub fn popup_mesh_frame(&self, instance_id: u64) -> Option<&super::EguiMeshFrame> {
        self.popup_mesh_frames.get(&instance_id)
    }

    /// banner의 최근 프레임 메타데이터. 아직 받지 않았거나 닫혔으면 None이다.
    pub fn banner_mesh_frame(&self, instance_id: u64) -> Option<&super::EguiMeshFrame> {
        self.banner_mesh_frames.get(&instance_id)
    }

    /// 공유 메모리를 만들고 보조 채널로 핸들을 전달한다.
    /// 호출자가 메인 채널 응답을 보내며 SDK는 call_id로 양쪽 결과를 연결한다.
    /// 핸들 전송에 실패하면 매핑을 등록하지 않고 오류를 반환한다.
    #[cfg(unix)]
    pub fn create_shared_buffer_for(
        &mut self,
        plugin_id: &str,
        call_id: u64,
        size: u64,
    ) -> Result<SharedBufferCreateResult, String> {
        // TODO(버퍼 상한): plugin별 상한이 필요해지면 매니페스트 선언으로 받는다.
        const MAX_BYTES: u64 = 1 << 30; // 1 GiB. 모든 plugin 공통 상한.
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
        // 호스트가 렌더링에 사용할 매핑을 보관한다. plugin 매핑은 별도 수명을 가진다.
        // 전송용 payload를 버려도 호스트 매핑은 유지된다.
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
        // TODO(버퍼 상한): plugin별 상한이 필요해지면 매니페스트 선언으로 받는다.
        const MAX_BYTES: u64 = 1 << 30; // 1 GiB. 모든 plugin 공통 상한.
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
        // 호스트 매핑을 보관한다. 상대 프로세스에 복제한 핸들은 payload Drop으로 닫히지 않는다.
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
