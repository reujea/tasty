//! 메인 루프 매 tick 에서 호출되는 `PluginManager::pump` + `drain_host_cmds`.
//!
//! - `pump`: plugin 알림 처리, healthcheck/PING, 호스트→plugin 핸드셰이크, surface 등록, restart.
//! - `drain_host_cmds`: registry/file_format/popup closure 가 큐잉한 `HostCmd` 일괄 처리.

mod healthcheck;
mod retirement;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::json;

use crate::host_cmd::HostCmd;
use crate::protocol::{self, PluginEvent};
use tasty_plugin_manifest::Permission;
use tasty_plugin_protocol::SharedBufferId;

use super::{
    HEALTHCHECK_TIMEOUT, NAMESPACE_EXPIRY_RESTART_LIMIT, PendingPluginCall, PendingRequestKind,
    PluginManager, PluginTick,
};

/// 한 tick에서 모은 플러그인 이벤트. collect_plugin_events가 채우고 apply_collected_events가 처리한다.
#[derive(Default)]
struct CollectedPluginEvents {
    /// (채널의 manifest id, hello의 id, hello의 버전).
    /// hello의 id가 잘못됐어도 매니페스트를 찾을 수 있도록 채널 id를 함께 둔다.
    hello_log: Vec<(String, String, String)>,
    to_register: Vec<String>,
    new_calls: Vec<PendingPluginCall>,
    new_event_publishes: Vec<(String, tasty_plugin_protocol::EventEnvelope)>,
    new_event_subscribes: Vec<(String, u64, String)>,
    new_event_unsubscribes: Vec<(String, u64)>,
    new_paint_frames: Vec<(u32, super::EguiMeshFrame)>,
    new_popup_paint_frames: Vec<(u64, super::EguiMeshFrame)>,
    new_banner_paint_frames: Vec<(u64, super::EguiMeshFrame)>,
    // surface의 무입력 갱신 요청. 호스트가 pump 이후 가져간다.
    new_invalidated: Vec<u32>,
    // popup의 무입력 갱신 요청.
    new_invalidated_popups: Vec<u64>,
    // banner의 무입력 갱신 요청.
    new_invalidated_banners: Vec<u64>,
    // plugin 이 폐기한 shared buffer (성장 재생성 등): (plugin_id, buffer_id).
    // host 매핑을 해제하지 않으면 구세대 버퍼가 plugin 수명 내내 남는다.
    released_buffers: Vec<(String, SharedBufferId)>,
    // 연결 종료를 pump에서 감지해 다음 healthcheck 전에도 마지막 mesh 프레임을 지운다.
    disconnected: Vec<String>,
}

/// hello의 id 또는 버전이 설치 매니페스트와 다른 경우.
#[derive(Debug, PartialEq, Eq)]
enum HelloDrift {
    /// hello 의 `plugin_id` 가 채널 키(= 설치 매니페스트 id)와 다르다.
    Id,
    /// 바이너리가 보고한 버전이 매니페스트 버전과 다르다 — 매니페스트 쪽 값을 함께 낸다.
    Version { manifest: String },
}

/// hello의 id와 버전을 비교한다. 매니페스트를 찾지 못하면 비교를 생략한다.
/// 매니페스트 조회에는 hello가 주장한 id 대신 채널 id를 사용해야 한다.
fn classify_hello_drift(
    channel_id: &str,
    claimed_id: &str,
    version: &str,
    manifest_version: Option<&str>,
) -> Vec<HelloDrift> {
    let Some(manifest_version) = manifest_version else {
        return Vec::new();
    };
    let mut drifts = Vec::new();
    if claimed_id != channel_id {
        drifts.push(HelloDrift::Id);
    }
    if manifest_version != version {
        drifts.push(HelloDrift::Version {
            manifest: manifest_version.to_string(),
        });
    }
    drifts
}

/// 재시작 사유와 로그·이벤트 메시지.
struct RestartCause {
    plugin_id: String,
    error_kind: &'static str,
    warn: String,
    message: String,
}

#[derive(Default)]
pub(super) struct RetirementControl {
    publications: std::collections::VecDeque<(HostCmd, usize)>,
    publication_bytes: usize,
    cancelled: Vec<(u32, crate::host_cmd::SurfaceBinding)>,
    responses: std::collections::VecDeque<(String, std::sync::Weak<()>, protocol::PluginResponse)>,
    bytes: usize,
}

impl PluginManager {
    /// 플러그인 이벤트와 주기 작업을 처리하고 응답이 없는 프로세스를 재시작한다.
    /// TimerHub의 주기 판정에는 호출자가 준 now를 사용한다.
    /// 처음 등록한 hello 목록을 반환하면 호스트가 surface 종류 등록과 상태 알림을 마친다.
    pub fn pump(&mut self, now: Instant) -> Vec<(String, String)> {
        self.restore_retirement_control_responses();
        self.settle_connections();

        let collected = self.collect_plugin_events();
        let hello_pairs = self.apply_collected_events(collected);

        self.drain_host_cmds();

        self.drain_plugin_responses();

        // 만료된 hook은 fail-open으로, namespace 호출은 오류로 회신한다.
        self.sweep_expired_requests(now);

        for key in self.timers.drain_due(now) {
            match key {
                PluginTick::Ping => {
                    // 송신·판정 모두 이번 발화의 격자 시각을 쓴다. 루프가 깨어난 시각의 흔들림이
                    // 60초 비교를 한 tick 밀지 않게 한다.
                    let tick = self.ping_grid_time(now);
                    self.send_periodic_ping(tick);
                    self.restart_unresponsive_plugins(tick);
                }
                PluginTick::Rss => self.sample_plugin_rss(),
                PluginTick::AutoReload => self.poll_auto_reload(),
                PluginTick::Retire => self.poll_retiring(),
            }
        }

        hello_pairs
    }

    /// A halted journal cancels unpublished work; it never starts new plugin execution.
    pub fn poll_retirement_control(&mut self) {
        if let Err(reason) = self.poll_resource_control(true) {
            tracing::error!(%reason, "halted retirement control could not advance");
        }
    }

    /// Advance only required retirements while the App hides a structural publication.
    /// Unrelated publication and ordinary callbacks stay queued until observation resumes.
    pub fn poll_publication_retirements(&mut self) -> Result<(), String> {
        self.poll_resource_control(false)
    }

    fn poll_resource_control(&mut self, halted: bool) -> Result<(), String> {
        if halted {
            for (command, _) in self.retirement_control.publications.drain(..) {
                if let Some((surface, binding)) = publication_binding(&command) {
                    self.retirement_control.cancelled.push((surface, binding));
                }
            }
            self.retirement_control.publication_bytes = 0;
        }
        for _ in 0..64 {
            if !halted
                && (self.retirement_control.publications.len() >= 256
                    || self.retirement_control.publication_bytes
                        >= crate::process::channel_bytes::QUEUE_BYTES_LIMIT)
            {
                return Err(
                    "plugin publication backlog exceeded its retirement-control budget".into(),
                );
            }
            let Ok(command) = self.host_cmd_rx.try_recv() else {
                break;
            };
            match command {
                HostCmd::RemoteSurfaceRetired {
                    surface_id,
                    binding,
                    completion,
                } => {
                    let cancelled = if let Some(index) = self
                        .retirement_control
                        .cancelled
                        .iter()
                        .position(|(id, old)| *id == surface_id && old.same_instance(&binding))
                    {
                        self.retirement_control.cancelled.swap_remove(index);
                        true
                    } else if let Some(index) = self
                        .retirement_control
                        .publications
                        .iter()
                        .position(|(command, _)| {
                            publication_binding(command).is_some_and(|(id, old)| {
                                id == surface_id && old.same_instance(&binding)
                            })
                        })
                    {
                        // This exact owner is being retired before its queued create was sent.
                        // Other publications keep their FIFO order and are never cancelled here.
                        let (_, bytes) = self
                            .retirement_control
                            .publications
                            .remove(index)
                            .expect("located publication");
                        self.retirement_control.publication_bytes -= bytes;
                        true
                    } else {
                        false
                    };
                    if cancelled {
                        if let Some(completion) = completion {
                            completion.finish(Ok(()));
                        }
                    } else if let Some(completion) = completion {
                        self.destroy_observed_remote_surface(surface_id, binding, completion);
                    } else {
                        self.destroy_bound_remote_surface(surface_id, &binding);
                    }
                }
                publication => {
                    if halted {
                        if let Some((surface, binding)) = publication_binding(&publication) {
                            self.retirement_control.cancelled.push((surface, binding));
                        }
                    } else {
                        let bytes = publication_weight(&publication);
                        self.retirement_control.publication_bytes = self
                            .retirement_control
                            .publication_bytes
                            .saturating_add(bytes);
                        self.retirement_control
                            .publications
                            .push_back((publication, bytes));
                    }
                }
            }
        }
        let plugins: Vec<_> = self.processes.keys().cloned().collect();
        let mut remaining = 64usize;
        for plugin in plugins {
            while remaining > 0
                && self.retirement_control.bytes < crate::process::channel_bytes::QUEUE_BYTES_LIMIT
            {
                let Some(process) = self.processes.get(&plugin) else {
                    break;
                };
                let binding = process.reply_binding();
                let response = match process.resp_rx.try_recv() {
                    Ok(response) => response,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let ids: Vec<_> = self
                            .pending_requests
                            .iter()
                            .filter_map(|(id, pending)| {
                                (pending.to == plugin
                                    && matches!(
                                        pending.kind,
                                        PendingRequestKind::SurfaceRetire { .. }
                                    ))
                                .then_some(*id)
                            })
                            .collect();
                        for id in ids {
                            self.settle_removed_retire_ack(id);
                        }
                        break;
                    }
                };
                remaining -= 1;
                if self
                    .pending_requests
                    .get(&response.id)
                    .is_some_and(|pending| {
                        matches!(pending.kind, PendingRequestKind::SurfaceRetire { .. })
                    })
                {
                    self.handle_plugin_response(&plugin, response);
                } else {
                    // Match the channel policy: one already-received oversized response may be
                    // retained, then further reads stop. Ordinary callbacks are never run here.
                    let bytes = serde_json::to_vec(&response)
                        .map_or(crate::process::channel_bytes::QUEUE_BYTES_LIMIT, |bytes| {
                            bytes.len()
                        });
                    self.retirement_control.bytes = self
                        .retirement_control
                        .bytes
                        .saturating_add(bytes)
                        .saturating_add(plugin.len());
                    self.retirement_control.responses.push_back((
                        plugin.clone(),
                        binding,
                        response,
                    ));
                }
            }
        }
        let obsolete: Vec<_> = self
            .pending_requests
            .iter()
            .filter_map(|(id, pending)| match &pending.kind {
                PendingRequestKind::SurfaceRetire {
                    process_binding, ..
                } if self
                    .processes
                    .get(&pending.to)
                    .is_none_or(|process| !process.reply_binding().ptr_eq(process_binding)) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for id in obsolete {
            self.settle_removed_retire_ack(id);
        }
        // 관측이 멈춘 동안 pump 의 Retire 틱이 돌지 않으므로 여기서 끝난 회수를 거둔다.
        self.settle_finished_retirements_in_place();
        if !halted
            && self.retirement_control.bytes >= crate::process::channel_bytes::QUEUE_BYTES_LIMIT
        {
            return Err("plugin response backlog exceeded its retirement-control budget".into());
        }
        Ok(())
    }
    fn restore_retirement_control_responses(&mut self) {
        let responses = std::mem::take(&mut self.retirement_control.responses);
        self.retirement_control.bytes = 0;
        for (plugin, binding, response) in responses {
            if self
                .processes
                .get(&plugin)
                .is_some_and(|process| process.reply_binding().ptr_eq(&binding))
            {
                self.handle_plugin_response(&plugin, response);
            }
        }
    }

    /// surface 갱신 요청을 가져간다. 호스트가 해당 View를 다시 그리는 데 쓴다.
    pub fn take_invalidated_surfaces(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.invalidated_surfaces)
    }

    /// 새 frame을 받은 surface를 가져간다. 호스트가 그 surface가 보이는 창을 다시 그린다.
    pub fn take_fresh_frame_surfaces(&mut self) -> Vec<u32> {
        self.fresh_frame_surfaces.drain().collect()
    }

    /// 팝업 갱신 요청을 가져간다. 호스트가 입력 없이도 다시 forward하도록 예약한다.
    pub fn take_invalidated_popups(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.invalidated_popups)
    }

    /// 배너 갱신 요청을 가져간다.
    pub fn take_invalidated_banners(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.invalidated_banners)
    }

    /// 실행 중인 플러그인의 RSS를 모은다. 이상 탐지·저장·알림은 호스트가 처리한다.
    fn sample_plugin_rss(&mut self) {
        let pids: Vec<(String, sysinfo::Pid)> = self
            .processes
            .iter()
            .filter_map(|(id, proc)| {
                proc.child_pid()
                    .map(|pid| (id.clone(), sysinfo::Pid::from_u32(pid)))
            })
            .collect();
        if pids.is_empty() {
            return;
        }
        let pid_list: Vec<sysinfo::Pid> = pids.iter().map(|(_, pid)| *pid).collect();
        self.sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&pid_list),
            true,
            sysinfo::ProcessRefreshKind::nothing().with_memory(),
        );
        for (plugin_id, pid) in pids {
            if let Some(p) = self.sys.process(pid) {
                self.pending_rss_samples.push((plugin_id, p.memory()));
            }
        }
    }

    /// `sample_plugin_rss` 누적을 드레인한다. `App::about_to_wait` 이 `pump()`
    /// 직후 호출해, `CoreState.anomaly_detector` 에 공급한다.
    pub fn take_rss_samples(&mut self) -> Vec<(String, u64)> {
        std::mem::take(&mut self.pending_rss_samples)
    }

    /// 프로세스별 이벤트 큐를 순서대로 비워 수집한다.
    fn collect_plugin_events(&self) -> CollectedPluginEvents {
        let mut out = CollectedPluginEvents::default();
        for (id, proc) in &self.processes {
            loop {
                match proc.event_rx.try_recv() {
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        out.disconnected.push(id.clone());
                        break;
                    }
                    Ok(ev) => self.classify_event(id, ev, &mut out),
                }
            }
        }
        out
    }

    /// 이벤트를 종류별로 모은다. Log는 여기서 바로 기록한다.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: PluginEvent 종류별
    // 프로토콜 이벤트별 얕은 분기이며, 별도 함수로 나눠도 이벤트 구분은 필요하다.
    fn classify_event(&self, id: &str, ev: PluginEvent, out: &mut CollectedPluginEvents) {
        match ev {
            PluginEvent::Hello { plugin_id, version } => {
                out.hello_log
                    .push((id.to_string(), plugin_id.clone(), version));
                if !self.registered_plugins.contains(&plugin_id) {
                    out.to_register.push(plugin_id);
                }
            }
            PluginEvent::Log { level, message } => match level.as_str() {
                "error" => tracing::error!("[plugin {}] {}", id, message),
                "warn" => tracing::warn!("[plugin {}] {}", id, message),
                _ => tracing::info!("[plugin {}] {}", id, message),
            },
            PluginEvent::SurfaceInvalidated { surface_id } => {
                // 입력 없이도 다음 forward에서 화면을 갱신한다.
                out.new_invalidated.push(surface_id);
            }
            PluginEvent::PopupInvalidated { instance_id } => {
                out.new_invalidated_popups.push(instance_id);
            }
            PluginEvent::BannerInvalidated { instance_id } => {
                out.new_invalidated_banners.push(instance_id);
            }
            PluginEvent::PaintFrame {
                surface_id,
                buffer_id,
                generation,
                frame_seq,
                full_textures,
                byte_len,
                ime_cursor,
            } => {
                // 렌더러가 사용할 최신 프레임 정보. 수신 스레드가 이미 redraw를 깨운다.
                out.new_paint_frames.push((
                    surface_id,
                    super::EguiMeshFrame {
                        plugin_id: id.to_string(),
                        buffer_id,
                        generation,
                        frame_seq,
                        full_textures,
                        byte_len,
                        ime_cursor,
                        banner_body_cut: None,
                    },
                ));
            }
            PluginEvent::PopupPaintFrame {
                instance_id,
                buffer_id,
                generation,
                frame_seq,
                full_textures,
                ime_cursor,
            } => {
                out.new_popup_paint_frames.push((
                    instance_id,
                    super::EguiMeshFrame {
                        plugin_id: id.to_string(),
                        buffer_id,
                        generation,
                        frame_seq,
                        full_textures,
                        // popup 은 attach mesh mirror 스코프 밖이다(mirror 는
                        // surface 전용) — wire 에 byte_len 이 없어 0(구버전과 동일
                        // fallback).
                        byte_len: 0,
                        ime_cursor,
                        banner_body_cut: None,
                    },
                ));
            }
            PluginEvent::BannerPaintFrame {
                instance_id,
                buffer_id,
                generation,
                frame_seq,
                full_textures,
                body_cut,
            } => {
                out.new_banner_paint_frames.push((
                    instance_id,
                    super::EguiMeshFrame {
                        plugin_id: id.to_string(),
                        buffer_id,
                        generation,
                        frame_seq,
                        full_textures,
                        // banner 도 attach mesh mirror 스코프 밖 — 위 popup 과 동일 사유.
                        byte_len: 0,
                        // banner 는 키/IME 를 forward 받지 않아(셸이 포커스를 주지
                        // 않는 non-modal 공지) wire 에 이 칸이 없다.
                        ime_cursor: None,
                        banner_body_cut: body_cut,
                    },
                ));
            }
            PluginEvent::NotifyHost { .. } => {}
            PluginEvent::IpcCall {
                call_id,
                method,
                params,
            } => {
                let perms = self
                    .plugin_permissions
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(HashSet::new()));
                let Some(process) = self.processes.get(id) else {
                    return;
                };
                out.new_calls.push(PendingPluginCall {
                    binding: process.reply_binding(),
                    plugin_id: id.to_string(),
                    call_id,
                    method,
                    params,
                    permissions: perms,
                });
            }
            PluginEvent::EventPublish { envelope } => {
                out.new_event_publishes.push((id.to_string(), envelope));
            }
            PluginEvent::EventSubscribe { sub_id, pattern } => {
                out.new_event_subscribes
                    .push((id.to_string(), sub_id, pattern));
            }
            PluginEvent::EventUnsubscribe { sub_id } => {
                out.new_event_unsubscribes.push((id.to_string(), sub_id));
            }
            PluginEvent::SharedBufferReleased { id: buffer_id } => {
                out.released_buffers.push((id.to_string(), buffer_id));
            }
            PluginEvent::Unknown => {
                // forward-compat fallback — 신버전 plugin 의 미지 이벤트는 무시.
                tracing::debug!("[plugin {}] unknown event kind (ignored)", id);
            }
        }
    }

    /// 수집한 이벤트를 처리하고 처음 등록한 hello 목록을 반환한다.
    /// IPC 호출에는 hello 등록 후의 권한을 붙여야 같은 배치의 첫 호출도 올바르게 검사한다.
    fn apply_collected_events(
        &mut self,
        collected: CollectedPluginEvents,
    ) -> Vec<(String, String)> {
        let CollectedPluginEvents {
            hello_log,
            to_register,
            mut new_calls,
            new_event_publishes,
            new_event_subscribes,
            new_event_unsubscribes,
            new_paint_frames,
            new_popup_paint_frames,
            new_banner_paint_frames,
            new_invalidated,
            new_invalidated_popups,
            new_invalidated_banners,
            released_buffers,
            disconnected,
        } = collected;

        self.clear_dead_plugin_frames(&disconnected);
        for (surface_id, frame) in new_paint_frames {
            self.egui_mesh_frames.insert(surface_id, frame);
            self.fresh_frame_surfaces.insert(surface_id);
        }
        for (instance_id, frame) in new_popup_paint_frames {
            self.popup_mesh_frames.insert(instance_id, frame);
        }
        for (instance_id, frame) in new_banner_paint_frames {
            self.banner_mesh_frames.insert(instance_id, frame);
        }
        if !new_invalidated.is_empty() {
            self.invalidated_surfaces.extend(new_invalidated);
        }
        if !new_invalidated_popups.is_empty() {
            self.invalidated_popups.extend(new_invalidated_popups);
        }
        if !new_invalidated_banners.is_empty() {
            self.invalidated_banners.extend(new_invalidated_banners);
        }
        for (plugin_id, buffer_id) in released_buffers {
            self.release_plugin_buffer(&plugin_id, buffer_id);
        }
        self.log_hello_and_check_drift(hello_log);
        let hello_pairs = self.register_new_hellos(&to_register);
        // 같은 배치의 hello와 첫 IPC 호출을 처리할 때도 등록된 권한을 사용한다.
        self.restamp_permissions(&mut new_calls);
        if !new_calls.is_empty() {
            self.pending_plugin_calls.extend(new_calls);
        }
        self.apply_event_bus_changes(
            new_event_subscribes,
            new_event_unsubscribes,
            new_event_publishes,
        );

        hello_pairs
    }

    /// 현재 등록된 권한으로 바꾼다. 아직 모르는 플러그인은 수집 당시 값을 유지한다.
    fn restamp_permissions(&self, calls: &mut [PendingPluginCall]) {
        for call in calls.iter_mut() {
            if !self.plugin_call_is_current(call) {
                continue;
            }
            if let Some(perms) = self.plugin_permissions.get(&call.plugin_id) {
                call.permissions = perms.clone();
            }
        }
    }

    /// 연결이 종료된 플러그인의 프레임을 제거한다.
    fn clear_dead_plugin_frames(&mut self, disconnected: &[String]) {
        for dead in disconnected {
            self.egui_mesh_frames.retain(|_, f| &f.plugin_id != dead);
            self.popup_mesh_frames.retain(|_, f| &f.plugin_id != dead);
            self.banner_mesh_frames.retain(|_, f| &f.plugin_id != dead);
        }
        self.forget_fresh_frames_without_a_frame();
    }

    /// 프레임을 지운 surface의 새 frame 기록도 지운다. 기록은 프레임이 남은 surface만 가리킨다.
    fn forget_fresh_frames_without_a_frame(&mut self) {
        let frames = &self.egui_mesh_frames;
        self.fresh_frame_surfaces
            .retain(|sid| frames.contains_key(sid));
    }

    /// hello 수신 로그. 대조는 [`Self::warn_hello_drift`] 가 한다 — 뿌리는 일과
    /// 판정하는 일을 한 함수에 두지 않는다.
    fn log_hello_and_check_drift(&self, hello_log: Vec<(String, String, String)>) {
        for (channel_id, claimed_id, version) in hello_log {
            tracing::info!("plugin hello: {} v{}", claimed_id, version);
            self.warn_hello_drift(&channel_id, &claimed_id, &version);
        }
    }

    /// hello의 id·버전을 채널의 매니페스트와 비교해 다르면 경고한다.
    /// 이 함수는 연결을 거절하거나 매니페스트를 변경하지 않는다.
    fn warn_hello_drift(&self, channel_id: &str, claimed_id: &str, version: &str) {
        let manifest_version = self
            .packages
            .iter()
            .find(|p| p.manifest.id == channel_id)
            .map(|p| p.manifest.version.as_str());
        for drift in classify_hello_drift(channel_id, claimed_id, version, manifest_version) {
            match drift {
                HelloDrift::Id => tracing::warn!(
                    "plugin '{channel_id}' identity drift: hello claims id '{claimed_id}' != \
                     manifest id '{channel_id}'; check PLUGIN_ID against the installed manifest"
                ),
                HelloDrift::Version { manifest } => tracing::warn!(
                    "plugin '{channel_id}' version drift: binary v{version} != manifest \
                     v{manifest} — stale build? (dev: `cargo build --workspace` 후 재실행)"
                ),
            }
        }
    }

    /// Event Bus: plugin이 보낸 subscribe/unsubscribe/publish 를 원본 순서대로 처리.
    fn apply_event_bus_changes(
        &mut self,
        new_event_subscribes: Vec<(String, u64, String)>,
        new_event_unsubscribes: Vec<(String, u64)>,
        new_event_publishes: Vec<(String, tasty_plugin_protocol::EventEnvelope)>,
    ) {
        for (plugin_id, sub_id, pattern) in new_event_subscribes {
            if let Err(e) = self
                .event_bus
                .subscribe_plugin(&plugin_id, sub_id, pattern.clone())
            {
                tracing::warn!("plugin '{plugin_id}' event.subscribe rejected: {e}");
            }
        }
        for (plugin_id, sub_id) in new_event_unsubscribes {
            self.event_bus.unsubscribe_plugin(&plugin_id, sub_id);
        }
        for (plugin_id, envelope) in new_event_publishes {
            self.route_plugin_event_publish(&plugin_id, envelope);
        }
    }

    /// hello 를 처음 받은 plugin 의 권한 set / event_bus 패턴 / settings_pages 동기화.
    /// surface_kind registry 등록 + `registered_plugins.insert` 는 호출자
    /// (App::finalize_plugin_hello) 가 처리 — CoreEvent 발화 위치 정렬.
    fn register_new_hellos(&mut self, to_register: &[String]) -> Vec<(String, String)> {
        let mut hello_pairs: Vec<(String, String)> = Vec::new();
        if !to_register.is_empty() {
            for plugin_id in to_register {
                if let Some(pkg) = self.packages.iter().find(|p| &p.manifest.id == plugin_id) {
                    let granted = self.config.granted_permissions(plugin_id);
                    let perms: HashSet<Permission> = pkg
                        .manifest
                        .parsed_permissions()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|p| granted.contains(&p.as_token()))
                        .collect();
                    self.plugin_permissions
                        .insert(plugin_id.clone(), Arc::new(perms));
                    self.event_bus.set_plugin_permissions(
                        plugin_id,
                        pkg.manifest.event_subscribe.clone(),
                        pkg.manifest.event_publish.clone(),
                    );
                    // settings_pages: hello/manifest 수신 시 plugin 의 sub-page 등록.
                    // 동일 plugin 의 중복 register 방지를 위해 먼저 정리한 뒤 register.
                    self.settings_pages.unregister_plugin(plugin_id);
                    self.settings_pages.register(
                        plugin_id.clone(),
                        pkg.manifest.contributes.settings_pages.clone(),
                    );
                    hello_pairs.push((plugin_id.clone(), pkg.manifest.version.clone()));
                }
            }
        }
        hello_pairs
    }

    /// 방금 발화한 Ping 타이머의 격자 시각. 발화 뒤 다음 예정 시각에서 한 주기를 뺀다.
    fn ping_grid_time(&self, now: Instant) -> Instant {
        let next = self
            .timers
            .snapshot()
            .into_iter()
            .find(|s| s.key == PluginTick::Ping);
        match next {
            Some(s) => healthcheck::grid_time(s.next_due, s.interval, now),
            None => now,
        }
    }

    /// 주기적 ping — 전 프로세스에 ping 송신. 주기 판정은 `PluginTick::Ping` 이 한다.
    /// 송신이 실패해도(큐 포화 등) 미응답 송신 시각은 잡는다. 송신 큐가 막힌 것도 행 상태다.
    fn send_periodic_ping(&mut self, now: Instant) {
        for proc in self.processes.values_mut() {
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            proc.ping(id);
            let owed = healthcheck::owed_since_after_ping(
                proc.ping_owed_since(),
                proc.last_pong_at(),
                now,
            );
            proc.set_ping_owed_since(owed);
        }
    }

    /// 헬스체크 — 답하지 않은 ping을 보낸 지 `HEALTHCHECK_TIMEOUT` 이 지난 plugin 을 재시작.
    pub(super) fn restart_unresponsive_plugins(&mut self, now: Instant) {
        let mut unresponsive: Vec<RestartCause> = self
            .processes
            .iter()
            .filter_map(|(id, p)| {
                if healthcheck::is_unresponsive(
                    p.ping_owed_since(),
                    p.last_pong_at(),
                    now,
                    HEALTHCHECK_TIMEOUT,
                ) {
                    Some(RestartCause {
                        plugin_id: id.clone(),
                        error_kind: "unresponsive",
                        warn: format!(
                            "plugin '{}' unresponsive for {}s — restarting",
                            id,
                            HEALTHCHECK_TIMEOUT.as_secs()
                        ),
                        message: format!(
                            "plugin '{}' did not respond to ping for {}s — restarting",
                            id,
                            HEALTHCHECK_TIMEOUT.as_secs()
                        ),
                    })
                } else {
                    None
                }
            })
            .collect();
        // ping 응답 여부와 별도로 namespace 호출의 연속 만료도 확인한다.
        for (id, n) in &self.namespace_expiries {
            if *n >= NAMESPACE_EXPIRY_RESTART_LIMIT
                && self.processes.contains_key(id)
                && !unresponsive.iter().any(|c| &c.plugin_id == id)
            {
                let text =
                    format!("plugin '{id}' let {n} namespace calls expire in a row — restarting");
                unresponsive.push(RestartCause {
                    plugin_id: id.clone(),
                    error_kind: "namespace_unresponsive",
                    warn: text.clone(),
                    message: text,
                });
            }
        }
        for RestartCause {
            plugin_id: id,
            error_kind,
            warn,
            message,
        } in unresponsive
        {
            tracing::warn!("{warn}");
            {
                use tasty_plugin_protocol::EventScope;
                use tasty_plugin_protocol::events::payloads::PluginError;
                let payload = PluginError {
                    plugin_id: id.clone(),
                    error_kind: error_kind.to_string(),
                    message,
                };
                self.emit_host_event("plugin.error", &payload, EventScope::System);
            }
            // 이전 프로세스의 회수가 끝난 뒤 다시 실행해 파일·포트 사용이 겹치지 않게 한다.
            let retired = match self.processes.remove(&id) {
                Some(proc) => {
                    self.retire_process(&id, proc, true);
                    true
                }
                None => false,
            };
            // 설치 정보인 namespace 소유자는 유지하고 실행 상태만 지운다.
            self.forget_plugin_runtime(&id, "plugin restarting");
            self.egui_mesh_frames.retain(|_, f| f.plugin_id != id);
            self.popup_mesh_frames.retain(|_, f| f.plugin_id != id);
            self.banner_mesh_frames.retain(|_, f| f.plugin_id != id);
            self.forget_fresh_frames_without_a_frame();
            self.banner_instances.retain(|_, inst| inst.plugin_id != id);
            if !retired
                && let Some(pkg) = self.packages.iter().find(|p| p.manifest.id == id).cloned()
            {
                self.start_plugin_internal(&pkg);
            }
        }
    }

    /// auto-reload polling. 주기 판정은 `PluginTick::AutoReload` 가 하고, 그 타이머는
    /// flag 가 켜져 있을 때만 등록된다 — flag off 면 여기 도달하지 않는다
    /// (`check_for_updates` 자체도 flag 를 다시 확인해 이중 안전).
    fn poll_auto_reload(&mut self) {
        for plugin_id in self.check_for_updates() {
            if let Err(e) = self.auto_reload_one(&plugin_id) {
                tracing::warn!("auto-reload '{plugin_id}' failed: {e}");
            }
        }
    }

    pub(super) fn drain_host_cmds(&mut self) {
        loop {
            let cmd =
                if let Some((command, bytes)) = self.retirement_control.publications.pop_front() {
                    self.retirement_control.publication_bytes -= bytes;
                    command
                } else {
                    match self.host_cmd_rx.try_recv() {
                        Ok(command) => command,
                        Err(_) => break,
                    }
                };
            match cmd {
                HostCmd::RemoteSurfaceRetired {
                    surface_id,
                    binding,
                    completion,
                } => {
                    if let Some(completion) = completion {
                        self.destroy_observed_remote_surface(surface_id, binding, completion);
                    } else {
                        self.destroy_bound_remote_surface(surface_id, &binding);
                    }
                }
                HostCmd::RemoteSurfaceCreated {
                    surface_id,
                    plugin_id,
                    kind,
                    cwd,
                    params,
                    handles,
                } => {
                    let cwd_str = cwd.as_ref().and_then(|p| p.to_str()).map(str::to_string);
                    let origin = super::reattach::SurfaceOrigin::create(json!({
                        "surface_id": surface_id,
                        "kind": kind,
                        "cwd": cwd_str,
                        "params": params,
                    }));
                    self.publish_remote_surface(surface_id, plugin_id, handles, origin);
                }
                HostCmd::RemoteSurfaceRestored {
                    surface_id,
                    plugin_id,
                    kind,
                    data,
                    handles,
                } => {
                    let origin = super::reattach::SurfaceOrigin::restore(json!({
                        "surface_id": surface_id,
                        "kind": kind,
                        "data": data,
                    }));
                    self.publish_remote_surface(surface_id, plugin_id, handles, origin);
                }
            }
        }
    }
}

fn publication_binding(command: &HostCmd) -> Option<(u32, crate::host_cmd::SurfaceBinding)> {
    match command {
        HostCmd::RemoteSurfaceCreated {
            surface_id,
            handles,
            ..
        }
        | HostCmd::RemoteSurfaceRestored {
            surface_id,
            handles,
            ..
        } => Some((*surface_id, handles.binding())),
        HostCmd::RemoteSurfaceRetired { .. } => None,
    }
}

fn publication_weight(command: &HostCmd) -> usize {
    let (plugin, kind, payload, cwd) = match command {
        HostCmd::RemoteSurfaceCreated {
            plugin_id,
            kind,
            params,
            cwd,
            ..
        } => (
            plugin_id,
            kind,
            params,
            cwd.as_ref().map_or(0, |path| path.as_os_str().len()),
        ),
        HostCmd::RemoteSurfaceRestored {
            plugin_id,
            kind,
            data,
            ..
        } => (plugin_id, kind, data, 0),
        HostCmd::RemoteSurfaceRetired { .. } => return std::mem::size_of::<HostCmd>(),
    };
    std::mem::size_of::<HostCmd>()
        .saturating_add(plugin.len())
        .saturating_add(kind.len())
        .saturating_add(cwd)
        .saturating_add(
            serde_json::to_vec(payload)
                .map_or(crate::process::channel_bytes::QUEUE_BYTES_LIMIT, |bytes| {
                    bytes.len()
                }),
        )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tasty_terminal::waker_factory::NoopWakerFactory;

    use super::*;
    use crate::PluginProcess;
    use crate::protocol::PluginEvent;

    fn mgr() -> PluginManager {
        PluginManager::new(Arc::new(NoopWakerFactory))
    }

    #[test]
    fn queued_retirement_orders_between_unpumped_old_and_new_creation() {
        let mut manager = mgr();
        let plugin = "com.test.document";
        let (process, requests) = crate::process::PluginProcess::stub_with_request_rx(plugin);
        manager.processes.insert(plugin.into(), process);
        let handles = || crate::host_cmd::SurfaceHandles {
            display_name: Arc::default(),
            snapshot_cache: Arc::default(),
        };
        let old = handles();
        let fresh = handles();
        let create = |handles| HostCmd::RemoteSurfaceCreated {
            surface_id: 7,
            plugin_id: plugin.into(),
            kind: "document".into(),
            cwd: None,
            params: json!({}),
            handles,
        };
        manager.host_cmd_tx.send(create(old.clone())).unwrap();
        assert!(!manager.surfaces.contains_key(&7));
        manager
            .enqueue_bound_remote_retirement(7, old.binding())
            .unwrap();
        manager.host_cmd_tx.send(create(fresh.clone())).unwrap();
        manager.drain_host_cmds();
        assert!(fresh.binding().matches(&manager.surfaces[&7].handles));
        let methods: Vec<_> = std::iter::from_fn(|| requests.try_recv().ok())
            .map(|request| request.method)
            .collect();
        assert_eq!(
            methods,
            [
                protocol::METHOD_SURFACE_CREATE,
                protocol::METHOD_SURFACE_DESTROY,
                protocol::METHOD_SURFACE_CREATE
            ]
        );
        manager
            .enqueue_bound_remote_retirement(7, old.binding())
            .unwrap();
        manager.drain_host_cmds();
        assert!(fresh.binding().matches(&manager.surfaces[&7].handles));
        assert!(requests.try_recv().is_err());
    }

    fn hello(plugin_id: &str) -> PluginEvent {
        PluginEvent::Hello {
            plugin_id: plugin_id.to_string(),
            version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn classify_event_hello_queues_to_register_when_not_registered() {
        let mgr = mgr();
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event("com.example.test", hello("com.example.test"), &mut out);
        assert_eq!(out.to_register, vec!["com.example.test".to_string()]);
    }

    /// 같은 배치의 hello 처리 후에는 첫 호출에도 등록된 권한을 붙여야 한다.
    /// 이 시험은 권한 갱신 함수만 검사하며 pump의 호출 순서까지 확인하지는 않는다.
    #[test]
    fn a_call_queued_with_an_empty_permission_set_is_restamped_after_registration() {
        let mut mgr = mgr();
        mgr.set_plugin_permissions("com.example.test", HashSet::from([Permission::SurfaceRead]));
        let (process, _requests) = PluginProcess::stub_with_request_rx("com.example.test");
        let binding = process.reply_binding();
        mgr.processes.insert("com.example.test".into(), process);
        let mut calls = vec![PendingPluginCall {
            binding,
            plugin_id: "com.example.test".to_string(),
            call_id: 1,
            method: "recent.query".to_string(),
            params: json!({}),
            // 수집 시점의 상태 — 아직 등록 전이라 빈 셋.
            permissions: Arc::new(HashSet::new()),
        }];
        assert!(mgr.plugin_call_is_current(&calls[0]));
        mgr.restamp_permissions(&mut calls);
        assert!(
            calls[0].permissions.contains(&Permission::SurfaceRead),
            "등록된 권한이 다시 붙어야 한다: {:?}",
            calls[0].permissions
        );
    }

    /// 반대 방향 — 모르는 plugin 을 허용으로 바꾸지 않는다. 재부착이 "없으면 비운다"
    /// 가 아니라 "있으면 덮는다" 인지 가른다.
    #[test]
    fn an_unregistered_plugins_call_keeps_its_empty_permission_set() {
        let mut mgr = mgr();
        let (process, _requests) = PluginProcess::stub_with_request_rx("com.example.unknown");
        let binding = process.reply_binding();
        mgr.processes.insert("com.example.unknown".into(), process);
        let mut calls = vec![PendingPluginCall {
            binding,
            plugin_id: "com.example.unknown".to_string(),
            call_id: 2,
            method: "recent.query".to_string(),
            params: json!({}),
            permissions: Arc::new(HashSet::new()),
        }];
        assert!(mgr.plugin_call_is_current(&calls[0]));
        mgr.restamp_permissions(&mut calls);
        assert!(
            calls[0].permissions.is_empty(),
            "모르는 plugin 은 그대로 비어 있어야 한다: {:?}",
            calls[0].permissions
        );
    }

    /// id와 버전이 모두 같으면 경고할 차이가 없다.
    #[test]
    fn a_matching_hello_reports_no_drift() {
        assert_eq!(
            classify_hello_drift(
                "com.example.test",
                "com.example.test",
                "1.0.0",
                Some("1.0.0")
            ),
            Vec::new()
        );
    }

    #[test]
    fn a_claimed_id_that_differs_from_the_channel_key_is_id_drift() {
        assert_eq!(
            classify_hello_drift(
                "com.example.test",
                "com.example.typo",
                "1.0.0",
                Some("1.0.0")
            ),
            vec![HelloDrift::Id]
        );
    }

    /// 버전이 다르면 비교 대상인 매니페스트 버전도 반환한다.
    #[test]
    fn a_binary_version_that_differs_from_the_manifest_is_version_drift() {
        assert_eq!(
            classify_hello_drift(
                "com.example.test",
                "com.example.test",
                "1.0.0",
                Some("1.0.1")
            ),
            vec![HelloDrift::Version {
                manifest: "1.0.1".to_string()
            }]
        );
    }

    /// id와 버전의 차이를 각각 확인한다.
    #[test]
    fn both_axes_can_drift_at_once() {
        assert_eq!(
            classify_hello_drift(
                "com.example.test",
                "com.example.typo",
                "1.0.0",
                Some("1.0.1")
            ),
            vec![
                HelloDrift::Id,
                HelloDrift::Version {
                    manifest: "1.0.1".to_string()
                }
            ]
        );
    }

    /// 매니페스트를 찾지 못하면 두 비교 모두 생략한다.
    #[test]
    fn a_channel_key_with_no_manifest_reports_nothing_even_when_both_axes_differ() {
        assert_eq!(
            classify_hello_drift("com.example.ghost", "com.example.typo", "1.0.0", None),
            Vec::new()
        );
    }

    /// banner frame에 실린 본문 잘림은 그 배너의 최신 frame에 남고, 칸이 없는 구버전 알림은 잘림 없음이다.
    #[test]
    fn a_banner_frame_keeps_the_plugin_body_cut_and_an_old_frame_has_none() {
        let cut = tasty_plugin_protocol::BannerBodyCutWire {
            text: "full plugin body".to_owned(),
            body_rect: tasty_plugin_protocol::RectWire {
                x: 4.0,
                y: 2.0,
                width: 120.0,
                height: 45.0,
            },
        };
        let new = PluginEvent::BannerPaintFrame {
            instance_id: 9,
            buffer_id: SharedBufferId(1),
            generation: 1,
            frame_seq: 1,
            full_textures: true,
            body_cut: Some(cut.clone()),
        };
        let mut old = serde_json::to_value(&new).expect("serialize");
        old.as_object_mut()
            .expect("object")
            .remove("body_cut")
            .expect("new wire carries body_cut");
        let old: PluginEvent = serde_json::from_value(old).expect("old wire still parses");

        let mut mgr = mgr();
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event("com.tasty.mesh-demo", new, &mut out);
        mgr.apply_collected_events(out);
        assert_eq!(
            mgr.banner_mesh_frame(9)
                .and_then(|f| f.banner_body_cut.clone()),
            Some(cut)
        );

        let mut out = CollectedPluginEvents::default();
        mgr.classify_event("com.tasty.mesh-demo", old, &mut out);
        mgr.apply_collected_events(out);
        assert!(
            mgr.banner_mesh_frame(9)
                .is_some_and(|f| f.banner_body_cut.is_none())
        );
    }

    /// 배너 갱신 요청은 한 번만 가져갈 수 있다.
    #[test]
    fn banner_invalidated_accumulates_and_drains_once() {
        let mut mgr = mgr();
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event(
            "com.tasty.mesh-demo",
            PluginEvent::BannerInvalidated { instance_id: 7 },
            &mut out,
        );
        assert_eq!(out.new_invalidated_banners, vec![7]);

        mgr.apply_collected_events(out);
        assert_eq!(mgr.take_invalidated_banners(), vec![7]);
        assert!(
            mgr.take_invalidated_banners().is_empty(),
            "드레인은 1회여야 한다"
        );
    }

    /// 새 frame을 받은 surface는 호스트가 다시 그리도록 한 번 넘긴다. 플러그인이 입력 없이 보낸
    /// frame(파일 감시의 reload 등)은 창이 그린 뒤에 도착할 수 있다.
    #[test]
    fn a_fresh_paint_frame_is_handed_over_once_for_a_redraw() {
        let mut mgr = mgr();
        let mut out = CollectedPluginEvents::default();
        for generation in [1, 2] {
            mgr.classify_event(
                "com.tasty.image",
                PluginEvent::PaintFrame {
                    surface_id: 5,
                    buffer_id: SharedBufferId(1),
                    generation,
                    frame_seq: generation,
                    full_textures: false,
                    byte_len: 0,
                    ime_cursor: None,
                },
                &mut out,
            );
        }
        mgr.apply_collected_events(out);
        assert_eq!(mgr.take_fresh_frame_surfaces(), vec![5]);
        assert!(
            mgr.take_fresh_frame_surfaces().is_empty(),
            "드레인은 1회여야 한다"
        );
    }

    fn record_fresh_frames(mgr: &mut PluginManager, plugin: &str, surfaces: &[u32]) {
        let mut out = CollectedPluginEvents::default();
        for &surface_id in surfaces {
            mgr.classify_event(
                plugin,
                PluginEvent::PaintFrame {
                    surface_id,
                    buffer_id: SharedBufferId(u64::from(surface_id)),
                    generation: 1,
                    frame_seq: 1,
                    full_textures: true,
                    byte_len: 0,
                    ime_cursor: None,
                },
                &mut out,
            );
        }
        mgr.apply_collected_events(out);
    }

    /// 프레임을 지운 surface는 소비되지 않은 새 frame 기록에도 남지 않는다.
    #[test]
    fn closing_a_surface_leaves_no_fresh_frame_record() {
        let mut mgr = mgr();
        record_fresh_frames(&mut mgr, "com.tasty.image", &[5, 6]);
        record_fresh_frames(&mut mgr, "com.tasty.git-viewer", &[7]);

        let left = |mgr: &PluginManager| {
            let mut v: Vec<u32> = mgr.fresh_frame_surfaces.iter().copied().collect();
            v.sort_unstable();
            v
        };
        mgr.destroy_remote_surface(5, None);
        assert_eq!(left(&mgr), vec![6, 7], "닫은 surface 5의 기록이 남았다");
        mgr.drop_egui_mesh_frame(6);
        assert_eq!(
            left(&mgr),
            vec![7],
            "프레임을 버린 surface 6의 기록이 남았다"
        );
        mgr.clear_dead_plugin_frames(&["com.tasty.git-viewer".to_string()]);
        assert_eq!(
            left(&mgr),
            Vec::<u32>::new(),
            "끊긴 plugin의 surface 7 기록이 남았다"
        );
    }

    /// 배너와 팝업의 갱신 요청을 따로 모은다.
    #[test]
    fn banner_and_popup_invalidations_land_in_separate_accumulators() {
        let mgr = mgr();
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event(
            "com.tasty.mesh-demo",
            PluginEvent::BannerInvalidated { instance_id: 7 },
            &mut out,
        );
        mgr.classify_event(
            "com.tasty.git-viewer",
            PluginEvent::PopupInvalidated { instance_id: 9 },
            &mut out,
        );
        assert_eq!(out.new_invalidated_banners, vec![7]);
        assert_eq!(out.new_invalidated_popups, vec![9]);
    }

    #[test]
    fn classify_event_hello_skips_to_register_when_already_registered() {
        let mut mgr = mgr();
        mgr.registered_plugins
            .insert("com.example.test".to_string());
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event("com.example.test", hello("com.example.test"), &mut out);
        assert!(out.to_register.is_empty());
    }

    /// disable이 설정을 저장하므로 공용 테스트 가드로 홈을 격리한다.
    use crate::test_support::HomeEnvGuard;

    /// disable 후 새 hello는 권한과 훅을 다시 등록할 수 있어야 한다.
    #[test]
    fn disable_clears_registered_plugins_so_restart_hello_reregisters() {
        let home = HomeEnvGuard::tasty_home();

        let mut mgr = mgr();
        let plugin_id = "com.example.test";
        // disable 은 설치된 패키지에만 적용된다. 이 시험의 대상도 설치 목록에 둔다.
        mgr.set_packages_for_tests(vec![tasty_plugin_manifest::PluginPackage {
            dir: home.path().join("plugin"),
            manifest: toml::from_str(
                r#"
manifest_version = 1
id = "com.example.test"
name = "Test"
version = "1.0.0"
api_version = "1"
[entry]
type = "process"
command = "unused"
"#,
            )
            .expect("fixture manifest"),
        }]);
        // 최초 hello 등록을 시뮬레이션 (finalize_plugin_hello 가 정상 시 하는 일).
        mgr.registered_plugins.insert(plugin_id.to_string());

        mgr.disable(plugin_id).expect("disable should succeed");
        assert!(
            !mgr.registered_plugins.contains(plugin_id),
            "disable() must clear the registered_plugins gate so a restarted \
             process's hello re-registers hook events"
        );

        // 재기동한 새 프로세스가 다시 hello 를 보낸 상황.
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event(plugin_id, hello(plugin_id), &mut out);
        assert_eq!(
            out.to_register,
            vec![plugin_id.to_string()],
            "post-disable hello must be queued for re-registration"
        );
    }

    /// swap으로 종료해도 새 hello를 다시 등록할 수 있어야 한다.
    /// 설정을 저장하지 않으므로 이 시험에는 홈 격리가 필요 없다.
    #[test]
    fn swap_shutdown_internal_clears_registered_plugins_so_restart_hello_reregisters() {
        let mut mgr = mgr();
        let plugin_id = "com.example.test";
        mgr.registered_plugins.insert(plugin_id.to_string());

        mgr.swap_shutdown_internal(plugin_id)
            .expect("swap_shutdown_internal should succeed");
        assert!(
            !mgr.registered_plugins.contains(plugin_id),
            "swap_shutdown_internal() must clear the registered_plugins gate so the \
             respawned process's hello re-registers hook events"
        );

        // swap_respawn_internal 이 띄운 새 프로세스가 다시 hello 를 보낸 상황.
        let mut out = CollectedPluginEvents::default();
        mgr.classify_event(plugin_id, hello(plugin_id), &mut out);
        assert_eq!(
            out.to_register,
            vec![plugin_id.to_string()],
            "post-swap hello must be queued for re-registration"
        );
    }
}
