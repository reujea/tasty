//! 메인 루프 매 tick 에서 호출되는 `PluginManager::pump` + `drain_host_cmds`.
//!
//! - `pump`: plugin 알림 처리, healthcheck/PING, 호스트→plugin 핸드셰이크, surface 등록, restart.
//! - `drain_host_cmds`: registry/file_format/popup closure 가 큐잉한 `HostCmd` 일괄 처리.

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
    PluginManager, PluginTick, RemoteSurfaceEntry,
};

/// 한 tick 의 plugin→호스트 이벤트 수집 결과.
///
/// `pump` 이 `collect_plugin_events` 로 채운 뒤 `apply_collected_events` 로
/// 소비한다. 각 `Vec` 은 원본 pump 의 누산기와 1:1 대응하며, 채워지는 순서·
/// 조건·처리 순서를 그대로 보존한다.
#[derive(Default)]
struct CollectedPluginEvents {
    /// `(채널 키 = 설치 매니페스트 id, hello 가 주장한 id, hello 가 보고한 버전)`.
    /// 앞의 둘을 **함께** 실어야 `log_hello_and_check_drift` 가 한 자리에서 대조한다 —
    /// 주장한 id 만 실으면 그 값이 어긋났을 때 매니페스트 조회가 `None` 이 되어
    /// 버전 대조까지 같이 조용해진다.
    hello_log: Vec<(String, String, String)>,
    to_register: Vec<String>,
    new_calls: Vec<PendingPluginCall>,
    new_event_publishes: Vec<(String, tasty_plugin_protocol::EventEnvelope)>,
    new_event_subscribes: Vec<(String, u64, String)>,
    new_event_unsubscribes: Vec<(String, u64)>,
    // egui-mesh paint_frame 알림 (A1-S3): (surface_id, frame 메타).
    new_paint_frames: Vec<(u32, super::EguiMeshFrame)>,
    // egui-mesh popup paint_frame 알림 (A2): (instance_id, frame 메타).
    new_popup_paint_frames: Vec<(u64, super::EguiMeshFrame)>,
    // egui-mesh banner paint_frame 알림 (A3): (instance_id, frame 메타).
    new_banner_paint_frames: Vec<(u64, super::EguiMeshFrame)>,
    // SurfaceInvalidated 알림 (단계 06): idle 상태에서 plugin 이 파일 변경 등을 알린
    // surface_id. `App::event_handler` 가 pump 후 `take_invalidated_surfaces` 로 드레인.
    new_invalidated: Vec<u32>,
    // PopupInvalidated 알림(`docs/dev-guide/egui-mesh-channel.md` "popup·banner 대응"):
    // idle 상태에서 plugin 이 self-repaint(egui
    // viewport_output) 를 요청한 popup instance_id. `App::event_handler` 가 pump 후
    // `take_invalidated_popups` 로 드레인.
    new_invalidated_popups: Vec<u64>,
    // BannerInvalidated 알림: 위 popup 칸의 banner 대응. `App::event_handler` 가 pump 후
    // `take_invalidated_banners` 로 드레인.
    new_invalidated_banners: Vec<u64>,
    // plugin 이 폐기한 shared buffer (성장 재생성 등): (plugin_id, buffer_id).
    // host 매핑을 해제하지 않으면 구세대 버퍼가 plugin 수명 내내 남는다.
    released_buffers: Vec<(String, SharedBufferId)>,
    // 프로세스가 죽으면(reader 스레드 종료 → event_tx drop) event_rx 가 Disconnected
    // 가 된다. 60초 healthcheck 보다 먼저 감지해, 죽은 plugin 의 egui-mesh frame 을
    // 즉시 비워 stale mesh 가 계속 합성되는 것을 막는다 (research-a1 §9-7 crash 격리).
    disconnected: Vec<String>,
}

/// hello 가 주장한 정체와 설치 매니페스트가 어긋난 축.
/// [`classify_hello_drift`] 가 내고 [`PluginManager::warn_hello_drift`] 가 찍는다.
#[derive(Debug, PartialEq, Eq)]
enum HelloDrift {
    /// hello 의 `plugin_id` 가 채널 키(= 설치 매니페스트 id)와 다르다.
    Id,
    /// 바이너리가 보고한 버전이 매니페스트 버전과 다르다 — 매니페스트 쪽 값을 함께 낸다.
    Version { manifest: String },
}

/// hello 의 두 축을 매니페스트와 대조해 **어긋난 것만** 낸다 — 판정만 하고 아무것도
/// 찍지 않는다. 뿌리는 일과 판정하는 일을 한 함수에 두지 않으므로, 이 판정은
/// `tracing` 캡처 없이 그대로 시험된다.
///
/// `manifest_version` 이 `None` 이면 **채널 키로 매니페스트를 못 찾은 것**이고 그때는
/// 아무 판정도 하지 않는다 — 대조할 좌변이 없다. 호출자가 조회를 주장한 id 가 아니라
/// 채널 키로 해야 하는 이유가 여기 있다([`PluginManager::warn_hello_drift`] 참조).
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

/// 재시작 판정 한 건. 이유가 둘(ping 무응답 · namespace 만료 누적)이고 로그 문구와
/// 이벤트 문구가 원래 서로 달라, 둘을 같이 들고 다닌다.
struct RestartCause {
    plugin_id: String,
    error_kind: &'static str,
    warn: String,
    message: String,
}

impl PluginManager {
    /// 매 tick 호출. plugin 이벤트 처리 + 헬스체크 + 비응답 재시작.
    ///
    /// `now` 는 호출자(메인 루프)의 프레임 기준시각이다 — 주기 작업은 매니저가
    /// 소유한 [`TimerHub`](tasty_timer::TimerHub) 가 판정하므로 내부에서
    /// `Instant::now()` 를 부르지 않는다(테스트가 시간을 주입할 수 있다).
    ///
    /// 반환: 본 tick 에서 *처음 hello 받은 plugin* 의 `(plugin_id, version)`
    /// 리스트. 호출자 (App) 가 `finalize_plugin_hello` 로 surface_kind registry
    /// 등록 + CoreEvent (PluginLoaded / PluginSurfaceKindRegistered) 발화를
    /// 처리한다. 비어있으면 finalize 안 호출.
    pub fn pump(&mut self, now: Instant) -> Vec<(String, String)> {
        // 0. 연결 대기가 끝난 plugin 을 거둔다 — 실패면 기동 실패로 처리한다(`manager::connect`).
        self.settle_connections();

        // 1. plugin → 호스트 이벤트 수집 후 일괄 처리 (수집 순서·부수효과 보존).
        let collected = self.collect_plugin_events();
        let hello_pairs = self.apply_collected_events(collected);

        // 2. 새로 만들어진 RemoteSurface 등록 + plugin에 surface.create/restore 송신.
        self.drain_host_cmds();

        // 4. plugin → 호스트 응답 처리 (display_name/snapshot 동기화).
        self.drain_plugin_responses();

        // 4a. deadline 을 넘긴 pending 요청 정리 — hook 은 fail-open, namespace
        // 호출은 caller 에 오류 회신.
        self.sweep_expired_requests(now);

        // 5. 시간축 — due 한 주기 작업만 실행한다(위 이벤트 drain 은 프레임축).
        for key in self.timers.drain_due(now) {
            match key {
                PluginTick::Ping => {
                    self.send_periodic_ping();
                    // 헬스체크는 ping 과 같은 tick 에서 본다 — 상세·검출 상한은
                    // `PluginTick::Ping` 의 doc-comment 참조.
                    self.restart_unresponsive_plugins();
                }
                PluginTick::Rss => self.sample_plugin_rss(),
                PluginTick::AutoReload => self.poll_auto_reload(),
                PluginTick::Retire => self.poll_retiring(),
            }
        }

        hello_pairs
    }

    /// `SurfaceInvalidated`(단계 06) 누적을 드레인한다. `App::event_handler` 가
    /// `pump()` 직후 호출해, idle 상태에서 파일이 바뀐 egui-mesh surface(markdown 등)의
    /// View 를 dirty 표시하는 데 쓴다.
    pub fn take_invalidated_surfaces(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.invalidated_surfaces)
    }

    /// `PopupInvalidated` 누적을 드레인한다. `App::event_handler` 가
    /// `pump()` 직후 호출해, self-repaint 를 요청한 egui-mesh popup instance 에
    /// 무입력 재-forward 를 예약한다(ADR-0056 `plugin_mesh_popup_pending_repaint`
    /// 재사용, `mark_invalidated_popups_dirty` 참조).
    pub fn take_invalidated_popups(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.invalidated_popups)
    }

    /// `BannerInvalidated` 누적을 드레인한다 — 위 popup 판의 banner 대응
    /// (`AppState::plugin_mesh_banner_pending_repaint`, `mark_invalidated_banners_dirty`).
    pub fn take_invalidated_banners(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.invalidated_banners)
    }

    /// 살아있는 plugin 프로세스 전부의 RSS 를 sysinfo 로 sampling 해
    /// `pending_rss_samples` 에 누적한다. 주기 판정은 `PluginTick::Rss` 가 한다.
    /// 검출/영속/알림은 이 크레이트가 모르는 `tasty-telemetry`/host 책임이라
    /// 여기선 원시 값만 모은다 (`take_rss_samples` 로 드레인).
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

    /// plugin→호스트 이벤트를 `processes` 순회로 수집. self 를 읽기만 하며
    /// (부수효과는 `apply_collected_events` 에서), 각 프로세스의 큐를 순서대로
    /// 비운다 — 수집 순서를 원본 그대로 보존한다.
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

    /// 단일 `PluginEvent` 를 종류별 누산기로 분류. 부수효과 없이 `out` 에만
    /// push 하며(Log 만 즉시 로깅 — 원본 동일), 누산기 mutation 을 원본 arm 과
    /// 1:1 로 유지한다.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: PluginEvent 종류별
    // 12-arm 평면 dispatch — 각 arm 은 얕은 단일 push/log 이며 중첩이 없다(Log
    // arm 내부의 level 3-way match 만 예외). 이벤트 종류 수는 프로토콜이 정의한
    // 열거형 variant 개수의 필연이라, arm 을 별 함수로 쪼개도 이 함수가 인지해야
    // 하는 분기 수 자체는 줄지 않는다 — complexity-gate.md 의 "평면 match
    // 디스패치(arm 많으나 중첩 얕음)" 전형적 정당 초과 케이스.
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
                // idle 상태(입력 무)에서도 plugin 이 파일 변경 등을 알렸다 — 다음 tick 에서
                // 해당 View 의 forward 게이트를 무장해 입력 없이 재-forward 되게 한다.
                out.new_invalidated.push(surface_id);
            }
            PluginEvent::PopupInvalidated { instance_id } => {
                // popup 대응 — egui viewport_output self-repaint 등, 무입력 상태에서
                // plugin 이 재-forward 를 요청했다.
                out.new_invalidated_popups.push(instance_id);
            }
            PluginEvent::BannerInvalidated { instance_id } => {
                // banner 대응 — 위와 같은 이유, 같은 처리.
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
                // A1-S3 수신 라우팅: 최근 mesh frame 메타를 저장. 렌더 prepare(A1-S5)가
                // buffer lookup + 디코드 출발점으로 읽는다. redraw 는 수신 스레드가
                // 매 라인마다 waker 를 깨우므로 별도 트리거 불필요.
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
                // A2 popup 수신 라우팅: 최근 popup mesh frame 메타를 저장.
                // host 합성기(popup_mesh_render)가 instance_id 로 lookup 한다.
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
                    },
                ));
            }
            PluginEvent::BannerPaintFrame {
                instance_id,
                buffer_id,
                generation,
                frame_seq,
                full_textures,
            } => {
                // A3 banner 수신 라우팅: 최근 banner mesh frame 메타를 저장.
                // host 합성기(render_egui_mesh_banners)가 instance_id 로 lookup 한다.
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
                    },
                ));
            }
            PluginEvent::NotifyHost { .. } => {
                // 단계 06에서 처리
            }
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
                out.new_calls.push(PendingPluginCall {
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

    /// 수집된 이벤트의 부수효과를 확정한다.
    /// 반환: 본 tick 에서 처음 hello 받은 plugin 의 `(plugin_id, version)`.
    ///
    /// **plugin IPC 호출의 큐잉만 hello 등록 뒤로 뺐다.** 나머지 항목(frame /
    /// invalidate / buffer / event bus)은 서로 독립인 맵을 건드려 순서가 관측되지
    /// 않지만, 호출은 권한 셋을 함께 실어 나르므로 등록보다 앞서면 hello 와 같은
    /// 배치로 온 첫 호출이 빈 권한으로 굳는다 — 아래 `restamp_permissions` 주석.
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
        // **권한은 등록 뒤에 붙인다.** `classify_event` 는 IpcCall 을 볼 때
        // `plugin_permissions` 를 그 자리에서 읽는데, plugin 의 hello 와 그 plugin 의
        // 첫 IpcCall 이 **같은 배치**로 수집되면 그 시점엔 아직 아무것도 등록돼 있지
        // 않아 빈 권한 셋이 박힌다 — 첫 호출만 `permission_denied` 로 떨어지고 다음
        // 호출부터 멀쩡해지는 형태다. 등록(`register_new_hellos`)이 끝난 뒤 다시
        // 붙이면 두 이벤트가 한 배치로 오든 나뉘어 오든 결과가 같다.
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

    /// 수집 시점에 비어 있었을 수 있는 권한 셋을 **현재** 등록 상태로 다시 붙인다.
    ///
    /// 등록된 plugin 이면 그 권한으로 덮고, 아직 모르는 plugin 이면 수집 시점 값을
    /// 그대로 둔다(모르는 것을 허용으로 바꾸지 않는다). 같은 pump 안이라 그 사이
    /// 권한이 취소될 수 없으므로 멱등이다.
    fn restamp_permissions(&self, calls: &mut [PendingPluginCall]) {
        for call in calls.iter_mut() {
            if let Some(perms) = self.plugin_permissions.get(&call.plugin_id) {
                call.permissions = perms.clone();
            }
        }
    }

    /// 죽은(disconnected) plugin 이 남긴 egui-mesh/popup/banner frame 메타를 즉시
    /// 비운다 — 60초 healthcheck 를 기다리지 않고 surface 를 blank 로 전환해 stale
    /// mesh 합성을 막는다 (research-a1 §9-7).
    fn clear_dead_plugin_frames(&mut self, disconnected: &[String]) {
        for dead in disconnected {
            self.egui_mesh_frames.retain(|_, f| &f.plugin_id != dead);
            self.popup_mesh_frames.retain(|_, f| &f.plugin_id != dead);
            self.banner_mesh_frames.retain(|_, f| &f.plugin_id != dead);
        }
    }

    /// hello 수신 로그. 대조는 [`Self::warn_hello_drift`] 가 한다 — 뿌리는 일과
    /// 판정하는 일을 한 함수에 두지 않는다.
    fn log_hello_and_check_drift(&self, hello_log: Vec<(String, String, String)>) {
        for (channel_id, claimed_id, version) in hello_log {
            tracing::info!("plugin hello: {} v{}", claimed_id, version);
            self.warn_hello_drift(&channel_id, &claimed_id, &version);
        }
    }

    /// hello 가 주장한 정체를 설치 매니페스트와 대조하는 **유일한 자리**. 두 축을
    /// 여기서 함께 본다 — id 와 버전.
    ///
    /// - **id**: plugin 이 hello 로 보내는 `plugin_id` 는 손으로 적은 사본이고
    ///   (`const PLUGIN_ID`), 채널 키는 설치된 매니페스트의 `id` 다. 어긋나면 registry 는
    ///   주장한 id 로 채워지고 권한·비활성·도구 키는 매니페스트 id 로 채워져, 둘을 잇는
    ///   조회(`banner.rs` 의 `packages.iter().find(...)?`)가 `None` 을 내고 조용히
    ///   빠져나간다. 빌드 시점 사본 대조는 번들 아홉에만 걸려 있어(doc-guards
    ///   `plugin_manifest_version_parity`) 서드파티·stale 설치본은 런타임에만 드러난다.
    /// - **버전**: dev bundle 은 매니페스트(소스)와 바이너리(target exe)를 독립적으로
    ///   sync 하므로, plugin 을 재빌드하지 않으면 최신 매니페스트와 stale exe 조합이
    ///   조용히 설치된다 — e2e markdown.recent 회귀의 원인.
    ///
    /// **어느 쪽도 막지 않는다 — 소리만 낸다.** 어긋난 id 를 거부할지, 매니페스트를
    /// 이기게 할지, 지금처럼 경고만 할지는 "plugin 이 자기 정체를 주장할 수 있는가" 에
    /// 대한 신뢰 모델 결정이라 이 자리가 정할 것이 아니다. 대조가 존재하는 것과
    /// 막는 것은 다르다(ADR-0190).
    ///
    /// 매니페스트 조회는 **채널 키**로 한다. 주장한 id 로 찾으면 id 가 어긋난 바로 그
    /// 경우에 조회가 `None` 이 되어 버전 대조까지 함께 조용해진다.
    ///
    /// 판정 자체는 [`classify_hello_drift`] 가 진다 — 이 함수에 남은 것은 조회와
    /// 찍기뿐이다.
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
                     manifest id '{channel_id}' — 권한·비활성·도구 키는 매니페스트 id 로 \
                     채워지고 registry 는 주장한 id 로 채워져, 둘을 잇는 조회가 조용히 \
                     비어난다 (plugin 의 `const PLUGIN_ID` 를 매니페스트에 맞춰라)"
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

    /// 주기적 ping — 전 프로세스에 ping 송신. 주기 판정은 `PluginTick::Ping` 이 한다.
    fn send_periodic_ping(&mut self) {
        for proc in self.processes.values() {
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            proc.ping(id);
        }
    }

    /// 헬스체크 — `HEALTHCHECK_TIMEOUT` 무응답 plugin 을 재시작.
    pub(super) fn restart_unresponsive_plugins(&mut self) {
        let mut unresponsive: Vec<RestartCause> = self
            .processes
            .iter()
            .filter_map(|(id, p)| {
                if p.since_last_pong() > HEALTHCHECK_TIMEOUT {
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
        // ping 에는 답하면서 namespace 호출만 연달아 삼키는 plugin. 위 판정은 그것을
        // 원리적으로 못 본다 — 프로세스가 건강하기 때문이다.
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
            // 회수는 기다리지 않고, 새 프로세스는 옛 것이 빠진 뒤에 뜬다 — 겹치면 옛
            // 프로세스가 쥔 포트·파일을 새 것이 못 잡는다(`manager::retire`).
            let retired = match self.processes.remove(&id) {
                Some(proc) => {
                    self.retire_process(&id, proc, true);
                    true
                }
                None => false,
            };
            // ipc namespace 유지 — 재시작 중에 오는 호출은 "없는 메서드" 가 아니라
            // "지금 안 뜬 plugin" 이다(ADR-0173). 정리는 disable · swap 과 같은 한
            // 함수를 거친다 — 따로 적었을 때 여기만 등록 게이트를 안 풀었다.
            self.forget_plugin_runtime(&id, "plugin restarting");
            // egui-mesh: 죽은 plugin 의 buffer 를 가리키는 stale frame 메타 제거 (A1-S3 / A2 / A3).
            self.egui_mesh_frames.retain(|_, f| f.plugin_id != id);
            self.popup_mesh_frames.retain(|_, f| f.plugin_id != id);
            self.banner_mesh_frames.retain(|_, f| f.plugin_id != id);
            // 죽은 plugin 의 banner 인스턴스도 정리 — 다음 spawn 에서 새 인스턴스로 시작.
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

    /// Surface 닫힘 시 plugin surface 정리 — 소유 plugin 에 `surface.destroy` 를
    /// 보내(plugin 측 per-surface 상태 해제: docs/mesh 컨텍스트/캐시) host 측
    /// `RemoteSurfaceEntry`(shm 핸들)와 stale mesh frame 메타를 제거한다.
    /// plugin surface 가 아니면(터미널 등) no-op — 호출측은 kind 를 구분할 필요 없다.
    ///
    /// 소유 plugin 해석은 두 갈래다:
    /// 1. `RemoteSurfaceEntry` 가 있는 surface — entry 의 plugin_id.
    /// 2. egui-mesh surface(markdown 등) — entry 를 만들지 않으므로
    ///    (`send_egui_mesh_surface_create` 참조) manifest `[[surface_kinds]]` 의
    ///    kind 선언으로 owner 를 해석한다.
    ///
    /// 이 통지가 없으면 plugin 프로세스가 surface 상태를 영원히 들고 있어
    /// open/close 반복 시 무한 성장한다 (soak S6 실측: markdown 사이클당 ~30MB).
    /// plugin 이 create 를 받은 적 없는 surface 에 destroy 가 가도 plugin 측
    /// `destroy_surface` 는 맵 remove 뿐이라 무해하다.
    pub fn destroy_remote_surface(&mut self, surface_id: u32, kind: Option<&str>) {
        // 이 surface 의 mesh frame 이 참조하던 shared buffer 매핑도 host 측에서
        // 해제한다 — plugin 은 해제를 알릴 프로토콜 메시지가 없어 여기서 안 지우면
        // plugin 수명 내내 누적된다 (`release_plugin_buffer` 문서 참조).
        let frame = self.egui_mesh_frames.remove(&surface_id);
        if let Some(f) = &frame {
            let (pid, bid) = (f.plugin_id.clone(), f.buffer_id);
            self.release_plugin_buffer(&pid, bid);
        }
        if let Some(entry) = self.surfaces.remove(&surface_id) {
            self.send_surface_request(
                &entry.plugin_id,
                protocol::METHOD_SURFACE_DESTROY,
                json!({ "surface_id": surface_id }),
                PendingRequestKind::Other,
            );
            // entry drop → SurfaceHandles(shm) 해제.
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

    /// manifest `[[surface_kinds]]` 가 `kind` 를 선언한 plugin id. egui-mesh
    /// surface 의 kind→owner 해석용. 없으면(터미널/호스트 빌트인) None.
    fn plugin_id_for_surface_kind(&self, kind: &str) -> Option<String> {
        self.packages
            .iter()
            .find(|p| p.manifest.surface_kinds.iter().any(|sk| sk.kind == kind))
            .map(|p| p.manifest.id.clone())
    }

    pub(super) fn drain_host_cmds(&mut self) {
        loop {
            let cmd = match self.host_cmd_rx.try_recv() {
                Ok(c) => c,
                Err(_) => break,
            };
            match cmd {
                HostCmd::RemoteSurfaceCreated {
                    surface_id,
                    plugin_id,
                    kind,
                    cwd,
                    params,
                    handles,
                } => {
                    self.surfaces.insert(
                        surface_id,
                        RemoteSurfaceEntry {
                            plugin_id: plugin_id.clone(),
                            handles,
                        },
                    );
                    let cwd_str = cwd.as_ref().and_then(|p| p.to_str()).map(str::to_string);
                    self.send_surface_request(
                        &plugin_id,
                        protocol::METHOD_SURFACE_CREATE,
                        json!({
                            "surface_id": surface_id,
                            "kind": kind,
                            "cwd": cwd_str,
                            "params": params,
                        }),
                        PendingRequestKind::SurfaceCreate { surface_id },
                    );
                }
                HostCmd::RemoteSurfaceRestored {
                    surface_id,
                    plugin_id,
                    kind,
                    data,
                    handles,
                } => {
                    self.surfaces.insert(
                        surface_id,
                        RemoteSurfaceEntry {
                            plugin_id: plugin_id.clone(),
                            handles,
                        },
                    );
                    self.send_surface_request(
                        &plugin_id,
                        protocol::METHOD_SURFACE_RESTORE,
                        json!({
                            "surface_id": surface_id,
                            "kind": kind,
                            "data": data,
                        }),
                        PendingRequestKind::SurfaceRestore { surface_id },
                    );
                }
            }
        }
    }
}

// Task 14 회귀 테스트 — disable→재기동 시 `registered_plugins` gate 가 풀려
// 새 프로세스의 hello 가 다시 `to_register` 에 잡히는지 검증.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tasty_terminal::waker_factory::NoopWakerFactory;

    use super::*;
    use crate::protocol::PluginEvent;

    fn mgr() -> PluginManager {
        PluginManager::new(Arc::new(NoopWakerFactory))
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

    /// hello 와 그 plugin 의 첫 IpcCall 이 **같은 배치**로 수집되면 `classify_event`
    /// 시점엔 `plugin_permissions` 가 비어 있어 빈 권한이 박힌다. 등록 뒤 다시 붙이는
    /// 것이 그 자리를 메운다 — 실측된 증상은 헤드리스에서 lazy 로 띄운
    /// `com.tasty.markdown` 의 **첫** host call 만 `permission_denied` 로 떨어지는
    /// 것이었다(다음 호출부터는 정상).
    ///
    /// 이 테스트가 붙박는 것은 **재부착 그 자체**다. 재부착이 `register_new_hellos`
    /// *뒤*에 온다는 순서까지는 보지 않는다(그 축은 `apply_collected_events` 본문의
    /// 배치와 주석이 진다).
    #[test]
    fn a_call_queued_with_an_empty_permission_set_is_restamped_after_registration() {
        let mut mgr = mgr();
        mgr.set_plugin_permissions("com.example.test", HashSet::from([Permission::SurfaceRead]));
        let mut calls = vec![PendingPluginCall {
            plugin_id: "com.example.test".to_string(),
            call_id: 1,
            method: "recent.query".to_string(),
            params: json!({}),
            // 수집 시점의 상태 — 아직 등록 전이라 빈 셋.
            permissions: Arc::new(HashSet::new()),
        }];
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
        let mgr = mgr();
        let mut calls = vec![PendingPluginCall {
            plugin_id: "com.example.unknown".to_string(),
            call_id: 2,
            method: "recent.query".to_string(),
            params: json!({}),
            permissions: Arc::new(HashSet::new()),
        }];
        mgr.restamp_permissions(&mut calls);
        assert!(
            calls[0].permissions.is_empty(),
            "모르는 plugin 은 그대로 비어 있어야 한다: {:?}",
            calls[0].permissions
        );
    }

    /// hello 대조의 세 갈래 — 어긋난 축만 나오는가.
    ///
    /// 이 판정이 없으면 `warn_hello_drift` 를 통째로 지워도 초록이었다.
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

    /// 버전 축은 매니페스트 쪽 값을 함께 낸다 — 경고문이 "무엇과" 어긋났는지 찍는다.
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

    /// 두 축은 독립이다 — 하나가 어긋났다고 다른 하나를 안 보지 않는다.
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

    /// 그 함수 doc 이 스스로 밝힌 함정 — 매니페스트를 못 찾으면 **아무 판정도 안 한다.**
    /// 호출자가 조회를 주장한 id 로 하면 id 가 어긋난 바로 그 경우에 여기로 떨어져
    /// 버전 대조까지 함께 조용해진다. 조회를 채널 키로 하는 이유가 이 갈래다.
    #[test]
    fn a_channel_key_with_no_manifest_reports_nothing_even_when_both_axes_differ() {
        assert_eq!(
            classify_hello_drift("com.example.ghost", "com.example.typo", "1.0.0", None),
            Vec::new()
        );
    }

    /// banner self-repaint 의 host 쪽 두 자리 — pump 누적과 드레인.
    /// `BannerInvalidated` 가 banner 누산기로 가고, `take_invalidated_banners` 가
    /// 한 번만 낸다(두 번째는 비어야 다음 tick 이 헛돌지 않는다).
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

    /// popup 판을 복사해 오면서 누산기만 안 바꾸는 형태의 drift 를 가른다 — banner
    /// 이벤트는 popup 누산기에 닿지 않는다(반대도 같다).
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

    /// `disable()` 이 `config.save()` 로 실 파일을 건드리므로 홈을 격리한다. 직렬화 락은
    /// crate 공용 가드가 잡는다 — 이 모듈만의 락을 따로 두면 `bundle_sig` 쪽 테스트와
    /// 서로의 임시 홈을 지운다(`crate::test_support` 참조).
    use crate::test_support::HomeEnvGuard;

    /// 회귀 재현: hello 로 한 번 등록된 plugin 이 disable 을 거친 뒤 재기동
    /// (새 프로세스의 새 hello) 하면, gate 가 풀려 다시 `to_register` 에 잡혀야
    /// `finalize_plugin_hello` → `hook_event_registry.register()` 가 재실행된다.
    /// 고치기 전에는 `disable()` 이 `registered_plugins` 를 지우지 않아 두 번째
    /// hello 가 여기서 조용히 무시됐다 (crates/tasty-host-plugin/src/manager.rs:263).
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

    /// 회귀 재현(swap 경로) — `auto_reload_one`/`upgrade_builtins --restart-running`
    /// 이 쓰는 `swap_shutdown_internal` 도 `disable()` 과 동일한 gate 미해제 버그를
    /// 갖고 있었다. `swap_shutdown_internal` 은 `config.save()` 를 호출하지 않으므로
    /// (disable() 과 달리 `config.disabled.ids` 를 안 건드림) TASTY_HOME 격리가
    /// 필요 없다.
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
