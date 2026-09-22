//! attach 런타임 결선 (attach/detach 단계 4).
//!
//! 단계 3 의 [`OccupancyRegistry`](crate::core::attach) 는 순수 lock 테이블이고,
//! 단계 2 의 mirror(`new_detached`/`feed_bytes`/output tap/input sink)는 터미널
//! 메커니즘이다. 이 모듈은 둘을 **실제 바이트 파이프로 결선**한다:
//!
//! - [`CoreState::attach_surface_for_stream`]: stream client 의 attach 요청을
//!   처리 — lock 획득 → 초기 화면 bulk 스냅샷 push → 출력 tap 을 forwarder 스레드로
//!   client 에 흘림(서버 PTY 출력 → tap → StreamHub → client mirror).
//! - [`CoreState::feed_attached_input`]: client 입력 Data 프레임을 점유 surface 의
//!   PTY 로 전달(holder 검증 후, 서버 로컬 입력 차단 우회).
//!
//! 메인루프(gui `event_handler` / headless `boot`)의 `StreamReady` arm 이
//! `pump_inbound` 가 분류한 [`PumpOutcome`](tasty_ipc::stream_hub::PumpOutcome)
//! 를 받아 이 메서드들을 호출한다.
//!
//! 범위: surface 단위(터미널 1개). workspace 단위는 단계 6.

use std::collections::HashMap;
use std::thread;

use crate::core::Core;
use crate::core::CoreState;
use crate::core::attach::{AttachClientId, AttachError};
use crate::model::{AttachSurfaceClass, SurfaceId, WorkspaceId};
use tasty_ipc::stream::{StreamControl, StreamFrame, StreamTag, StructuralOp, encode_mux};
use tasty_ipc::stream_hub::{PushResult, StreamHub};

impl CoreState {
    /// stream client 의 attach 요청 처리(`stream.open` 의 `target`). 성공 시 그 client
    /// 가 surface 를 배타 점유하고, 서버는 현재 화면을 1 회 bulk 스냅샷으로 push 한 뒤
    /// 이후 PTY 출력을 forwarder 스레드로 계속 흘린다. 거부/실패 시 `attach_error`
    /// Control + Detach 로 연결을 닫는다.
    ///
    /// `hub` 는 메인루프가 보유한 StreamHub(= client sink 등록처). forwarder 스레드는
    /// 그 clone 을 들고 client 끊김(push Unknown/Disconnected) 시 자동 종료한다.
    pub fn attach_surface_for_stream(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
        hub: &StreamHub,
    ) {
        // 터미널(또는 deferred) 이 아니면 mesh-mirror 후보인지 확인한다(`docs/dev-guide/
        // attach-behavior.md` "mesh mirror 채널" 절 — 단일 surface attach 도 화이트리스트된
        // mesh surface 를 받아들이도록 확장).
        // PTY 스냅샷/tap 이 없는 별도 경로(`attach_mesh_surface_for_stream`)로 분기.
        if !self.terminals.contains(surface_id) && !self.is_surface_deferred(surface_id) {
            if let Some((kind, plugin_id)) = self.find_mesh_surface_info(surface_id)
                && crate::core::surface_registry::egui_mesh::is_egui_mesh_allowed(&kind, &plugin_id)
            {
                self.attach_mesh_surface_for_stream(surface_id, client_id, hub);
                return;
            }
            reject_attach(hub, client_id, "not_found", None);
            return;
        }

        // 배타 lock 획득(동시 attach 거부).
        match self.attach.acquire(surface_id, client_id) {
            Ok(_) => {}
            Err(AttachError::AlreadyAttached { holder }) => {
                reject_attach(hub, client_id, "already_attached", Some(holder));
                return;
            }
            Err(_) => {
                reject_attach(hub, client_id, "lock_error", None);
                return;
            }
        }

        // deferred 면 여기서 PTY spawn.
        self.ensure_surface_initialized(surface_id);

        let Some(terminal) = self.terminals.get_mut(surface_id) else {
            // 점유는 됐으나 터미널이 없다(spawn 실패) → lock 환원 + 에러.
            let _ = self.attach.release(surface_id, client_id); // best-effort release — 실패 무시
            reject_attach(hub, client_id, "spawn_failed", None);
            return;
        };

        let cols = terminal.cols();
        let rows = terminal.rows();
        // 현재 화면 bulk 스냅샷 → tap 등록(메인루프 단일소유라 그 사이 ingest 없음 →
        // 누락/중복 없음). 이후 tap 바이트가 delta.
        let snapshot = terminal.snapshot_as_vt();
        let tap_rx = terminal.add_output_tap();
        let resize_rx = terminal.add_resize_tap();

        // attach 성공 통지(client 가 cols/rows 로 mirror 생성) + 초기 스냅샷.
        let attached = serde_json::json!({
            "event": "attached",
            "surface_id": surface_id,
            "cols": cols,
            "rows": rows,
        });
        let attached_frame = StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&attached).unwrap_or_default(),
        );
        let _ = hub.push(client_id, attached_frame); // best-effort 통지 — PushResult(Result 아님) 무시: client 끊김 시 forwarder 가 정리.
        let _ = hub.push(client_id, StreamFrame::new(StreamTag::Data, snapshot)); // best-effort 스냅샷 push — client 끊김 시 무시.

        // forwarder: 서버 PTY 출력 tap → client. client 끊김 시 자동 종료(다음 출력
        // 때 terminal 의 tap 도 prune 됨, design 단계 4 §8-R4).
        let hub2 = hub.clone();
        thread::spawn(move || {
            for chunk in tap_rx {
                match hub2.push(client_id, StreamFrame::new(StreamTag::Data, chunk)) {
                    PushResult::Unknown | PushResult::Disconnected => break,
                    _ => {}
                }
            }
        });

        // resize forwarder: 원격 grid 변경 → client mirror 크기 갱신(Control frame).
        // client 끊김 시 자동 종료(bare surface_id — 이 연결은 단일 surface).
        let hub3 = hub.clone();
        thread::spawn(move || {
            for (cols, rows) in resize_rx {
                let msg = StreamControl::Resize {
                    surface_id,
                    cols,
                    rows,
                };
                let frame = StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&msg).unwrap_or_default(),
                );
                match hub3.push(client_id, frame) {
                    PushResult::Unknown | PushResult::Disconnected => break,
                    _ => {}
                }
            }
        });

        tracing::debug!("attach: surface {surface_id} -> client {client_id}");
    }

    /// mesh surface(egui-mesh 화이트리스트 plugin) 단일 attach. 터미널 경로와 달리
    /// PTY 가 없어 bulk 스냅샷/출력 tap/resize tap 이 존재하지 않는다 — lock 만 획득하고
    /// `attached` 통지를 보낸다. 실제 구독 시작은 client 가 뒤이어 보내는
    /// `StreamControl::MeshContext` 요청이 트리거한다(`docs/dev-guide/attach-behavior.md`
    /// "구독 = MeshContext" 절: 구독 요청 자체가 capability negotiation). mesh 바이트
    /// forward 는 `PluginManager` 접근권이 있는
    /// App 계층(`src/boot/headless_plugins.rs`)의 몫이라 여기서는 다루지 않는다.
    fn attach_mesh_surface_for_stream(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
        hub: &StreamHub,
    ) {
        match self.attach.acquire(surface_id, client_id) {
            Ok(_) => {}
            Err(AttachError::AlreadyAttached { holder }) => {
                reject_attach(hub, client_id, "already_attached", Some(holder));
                return;
            }
            Err(_) => {
                reject_attach(hub, client_id, "lock_error", None);
                return;
            }
        }
        let attached = serde_json::json!({
            "event": "attached",
            "surface_id": surface_id,
            "kind": "mesh",
        });
        let attached_frame = StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&attached).unwrap_or_default(),
        );
        let _ = hub.push(client_id, attached_frame); // best-effort 통지 — client 끊김 시 무해.
        tracing::debug!("attach: mesh surface {surface_id} -> client {client_id}");
    }

    /// mesh 계열 apply_attached_mesh_* 공용 holder 검증. mesh surface(비-터미널)는
    /// 단일 surface attach(`self.attach.holder`)로도, workspace 단위 attach(`class.
    /// mesh_candidates` 를 통해 `acquire_workspace` 의 `members` 로 편입되지만
    /// `surface_locks` 에는 terminal 만 기록되므로 `workspace_holder_of` 로만 조회됨)
    /// 로도 점유될 수 있다 — 둘 중 하나라도 이 client 면 점유로 인정한다. 이 체크가
    /// `self.attach.holder` 만 봤을 때는 workspace 단위로 attach 한 client 의 모든
    /// MeshContext/MeshInput/MeshFullResendRequest 가 항상 `not_attached` 로 거부됐다.
    fn mesh_holder_matches(&self, surface_id: SurfaceId, client_id: AttachClientId) -> bool {
        self.attach.holder(surface_id) == Some(client_id)
            || self.attach.workspace_holder_of(surface_id) == Some(client_id)
    }

    /// attach client 의 mesh 구독/geometry·theme·focus 갱신 요청을 반영
    /// (`StreamControl::MeshContext`). holder 검증: 이 client 가 이 surface 를 실제로
    /// 점유 중이어야 한다(단일 surface attach 또는 workspace 단위 attach). 반환: 반영
    /// 성공 여부(false = holder 불일치/미점유 → 호출자는 `MeshError` 회신).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_attached_mesh_context(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
        width_px: u32,
        height_px: u32,
        pixels_per_point: f32,
        theme: Option<tasty_plugin_protocol::protocol::ThemeWire>,
        focused: bool,
    ) -> bool {
        if !self.mesh_holder_matches(surface_id, client_id) {
            return false;
        }
        self.mesh_mirror.upsert(
            surface_id,
            client_id,
            width_px,
            height_px,
            pixels_per_point,
            theme,
            focused,
        );
        true
    }

    /// 명시 full-texture-resend 요청(`StreamControl::MeshFullResendRequest`) 반영.
    /// holder 검증은 [`Self::apply_attached_mesh_context`]와 동일. 반환: 성공 여부.
    pub fn apply_attached_mesh_full_resend(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
    ) -> bool {
        if !self.mesh_holder_matches(surface_id, client_id) {
            return false;
        }
        self.mesh_mirror.request_full_resend(surface_id)
    }

    /// attach mesh mirror 클라이언트가 mesh pane 위에서 캡처한 입력
    /// (`StreamControl::MeshInput`, `docs/dev-guide/attach-behavior.md` "MeshInput 누적"
    /// 절) 반영. holder 검증은
    /// [`Self::apply_attached_mesh_context`]와 동일. 반환: 성공 여부.
    pub fn apply_attached_mesh_input(
        &mut self,
        surface_id: SurfaceId,
        client_id: AttachClientId,
        input: tasty_plugin_protocol::protocol::RawInputWire,
    ) -> bool {
        if !self.mesh_holder_matches(surface_id, client_id) {
            return false;
        }
        self.mesh_mirror.push_input(surface_id, input)
    }

    /// client 입력 Data 프레임을 그 client 가 점유한 surface 의 PTY 로 전달한다.
    /// 서버 로컬 입력 차단(`apply_send_to_surface` 의 is_hard_occupied 거부)을 우회하는
    /// 유일한 정규 경로 — holder 검증을 거치므로 점유자만 입력할 수 있다.
    /// 반환: 라우팅 성공 여부(false = 점유 surface 없음 = 비-attach client).
    pub fn feed_attached_input(&mut self, client_id: AttachClientId, bytes: &[u8]) -> bool {
        let Some(surface_id) = self.attach.surface_held_by(client_id) else {
            return false;
        };
        if let Some(terminal) = self.terminals.get_mut(surface_id) {
            terminal.send_bytes(bytes);
            true
        } else {
            false
        }
    }

    /// workspace 단위 attach 요청 처리(단계 6, D1/D3/D4). 그 workspace 의 모든 터미널
    /// surface 를 배타 점유하고 각각 단계 4 와 동일하게 초기 스냅샷 + 출력 forwarder 를
    /// 건다. 단, 한 연결에 N 개 터미널이 실리므로 모든 Data 는 surface-prefixed
    /// (`encode_mux`)다. 비-터미널은 mirror 없이 placeholder 로만 디스크립터에 실린다.
    pub fn attach_workspace_for_stream(
        &mut self,
        workspace_id: u32,
        client_id: AttachClientId,
        hub: &StreamHub,
    ) {
        let Some(idx) = self.find_workspace_index_for_id(workspace_id) else {
            reject_attach(hub, client_id, "workspace_not_found", None);
            return;
        };
        let class = self.workspaces[idx].classify_attach_surfaces();
        // 점유(occupancy) 는 mirror 가능 여부와 무관하게 workspace 의 *모든* surface 를
        // 대상으로 한다(기존 동작과 동일 — mesh 후보가 화이트리스트에 없어 placeholder
        // 로 떨어지더라도 여전히 이 workspace 의 멤버이므로 lock 범위에 포함).
        let members: Vec<SurfaceId> = class
            .terminals
            .iter()
            .chain(class.non_terminals.iter())
            .chain(class.explorers.iter().map(|(sid, _)| sid))
            .chain(class.mesh_candidates.iter().map(|(sid, _, _)| sid))
            .chain(class.content_candidates.iter().map(|(sid, _, _, _)| sid))
            .copied()
            .collect();

        match self
            .attach
            .acquire_workspace(workspace_id, &class.terminals, &members, client_id)
        {
            Ok(_) => {}
            Err(AttachError::AlreadyAttached { holder }) => {
                reject_attach(hub, client_id, "already_attached", Some(holder));
                return;
            }
            Err(_) => {
                reject_attach(hub, client_id, "lock_error", None);
                return;
            }
        }

        // deferred 터미널은 여기서 PTY spawn(크기·스냅샷 확정).
        for &sid in &class.terminals {
            self.ensure_surface_initialized(sid);
        }

        // 트리 디스크립터(client mirror 트리 재구성 + per-surface role/cols/rows).
        let descriptor = self.build_workspace_descriptor(idx, workspace_id, &class);
        let descriptor_frame = StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&descriptor).unwrap_or_default(),
        );
        let _ = hub.push(client_id, descriptor_frame); // best-effort 디스크립터 push — PushResult(Result 아님) 무시: client 끊김 시 forwarder 가 정리.

        // 각 터미널: 초기 스냅샷(mux) + 출력/resize forwarder(mux). client 끊김 시 자동 종료.
        for &sid in &class.terminals {
            self.tap_surface_for_stream(sid, client_id, hub);
        }

        let (mesh_whitelisted, _mesh_rejected) = mesh_mirror_candidates(&class);
        let (content_whitelisted, content_rejected) = content_mirror_candidates(&class);
        tracing::debug!(
            "attach: workspace {workspace_id} -> client {client_id} ({} terminals, {} mesh, {} content, {} placeholders)",
            class.terminals.len(),
            mesh_whitelisted.len(),
            content_whitelisted.len(),
            class.non_terminals.len()
                + (class.mesh_candidates.len() - mesh_whitelisted.len())
                + content_rejected.len(),
        );
    }

    /// 한 라이브 터미널 surface 를 workspace-mode stream client 로 tap 한다: 초기 화면
    /// bulk 스냅샷(mux) 을 1회 push 한 뒤 출력·resize forwarder 스레드를 건다. 핸드셰이크
    /// (`attach_workspace_for_stream`)와 3단계 역반영(원격에 새로 생긴 surface 를
    /// on-the-fly 로 tap)이 공유한다.
    ///
    /// snapshot → tap 은 인접 동기 호출이라 그 사이 ingest 가 없어 누락/중복이 없다
    /// (메인루프 단일소유). client 끊김(push Unknown/Disconnected) 또는 원격 터미널
    /// drop(close 로 `terminals.remove` → tap sender drop → 채널 EOF) 시 forwarder 는
    /// 자연 종료한다.
    pub(crate) fn tap_surface_for_stream(
        &mut self,
        sid: SurfaceId,
        client_id: AttachClientId,
        hub: &StreamHub,
    ) {
        let Some(terminal) = self.terminals.get_mut(sid) else {
            return;
        };
        let snapshot = terminal.snapshot_as_vt();
        let tap_rx = terminal.add_output_tap();
        let resize_rx = terminal.add_resize_tap();
        let snapshot_frame = StreamFrame::new(StreamTag::Data, encode_mux(sid, &snapshot));
        let _ = hub.push(client_id, snapshot_frame); // best-effort 초기 mux 스냅샷 push — PushResult 무시: client 끊김 시 forwarder 가 정리.
        let hub2 = hub.clone();
        thread::spawn(move || {
            for chunk in tap_rx {
                match hub2.push(
                    client_id,
                    StreamFrame::new(StreamTag::Data, encode_mux(sid, &chunk)),
                ) {
                    PushResult::Unknown | PushResult::Disconnected => break,
                    _ => {}
                }
            }
        });

        // resize forwarder: 원격 grid 변경 → client mirror 크기 갱신. workspace 모드라
        // Control payload 에 remote surface_id(sid)를 실어 client 가 remote→local
        // 매핑으로 해당 mirror 만 갱신하게 한다.
        let hub3 = hub.clone();
        thread::spawn(move || {
            for (cols, rows) in resize_rx {
                let msg = StreamControl::Resize {
                    surface_id: sid,
                    cols,
                    rows,
                };
                let frame = StreamFrame::new(
                    StreamTag::Control,
                    serde_json::to_vec(&msg).unwrap_or_default(),
                );
                match hub3.push(client_id, frame) {
                    PushResult::Unknown | PushResult::Disconnected => break,
                    _ => {}
                }
            }
        });
    }

    /// attach 점유 중인 workspace 에 새로 생긴 멤버 surface 를 편입한다(occupancy
    /// 등록 + 즉시 스트림 tap). create-tab/split-pane/split-surface/adopt-terminal
    /// 이 공통으로 호출하는데, 이 함수들은 **로컬 생성 경로**(`tasty claude spawn`
    /// 등)와 **forward-op 경로**(`execute_forwarded_structural_op` 가 부르는
    /// `structural_exec::split`/`structural_exec::create_tab`) 양쪽에서 재사용된다. 로컬 경로는 여기서
    /// 즉시 tap 되는 게 맞지만, forward-op 경로는 호출측(`event_handler.rs`/
    /// `boot.rs`)이 `StructuralDelta` 전송 **후** 별도로 직접 tap 하므로 여기서
    /// 또 tap 하면 이중 tap 이 된다 — `attach.is_auto_tap_suppressed()`
    /// 가 forward-op 실행 구간을 표시해 이 경로만 스킵시킨다.
    ///
    /// workspace 가 점유돼 있지 않으면 `add_workspace_member` 가 no-op(false)이라
    /// 그대로 반환. notifier(StreamHub)가 미주입(테스트/headless)이어도 occupancy
    /// 등록까지는 되고 tap 만 스킵된다.
    pub(crate) fn tap_new_workspace_member(
        &mut self,
        workspace_id: WorkspaceId,
        surface_id: SurfaceId,
        is_terminal: bool,
    ) {
        if !self
            .attach
            .add_workspace_member(workspace_id, surface_id, is_terminal)
        {
            return;
        }
        // forward-op 실행 중(`execute_forwarded_structural_op`)이면 호출측이
        // `StructuralDelta` 전송 후 정확한 순서로 직접 tap 한다 — 여기서 또 tap 하면
        // 이중 tap(문자 중복 echo)이 된다.
        if self.attach.is_auto_tap_suppressed() {
            return;
        }
        // 로컬 경로로 생긴 멤버다 — holder 의 mirror 에는 아직 그 자리가 없다. tap 보다
        // **먼저** 구조를 보내야 client 가 매핑을 만든 뒤 스냅샷을 받는다(forward 경로의
        // "delta → tap" 순서와 같다, ADR-0481).
        self.attach.mark_structure_changed(workspace_id);
        self.push_structure_changes();
        if !is_terminal {
            return;
        }
        let Some(holder) = self.attach.workspace_holder(workspace_id) else {
            return;
        };
        let Some(hub) = self.attach.notifier() else {
            return;
        };
        self.tap_surface_for_stream(surface_id, holder, &hub);
    }

    /// workspace mode client 의 입력(surface-prefixed)을 지정 remote surface 의 PTY 로.
    /// holder 가 그 workspace 를 점유 중일 때만 통과(타 workspace surface 주입 차단).
    pub fn feed_attached_workspace_input(
        &mut self,
        client_id: AttachClientId,
        remote_surface_id: u32,
        bytes: &[u8],
    ) -> bool {
        let Some(ws) = self.attach.workspace_of_surface(remote_surface_id) else {
            return false;
        };
        if self.attach.workspace_holder(ws) != Some(client_id) {
            return false;
        }
        if let Some(terminal) = self.terminals.get_mut(remote_surface_id) {
            terminal.send_bytes(bytes);
            true
        } else {
            false
        }
    }

    /// client-driven mirror geometry(ADR-0045): mirror client 가 보낸
    /// [`StreamControl::ClientResize`](tasty_ipc::stream::StreamControl) 를 지정
    /// remote surface 의 **실제 PTY** 에 적용한다. `feed_attached_workspace_input`
    /// 과 동형으로 holder 를 검증해(그 workspace 를 점유한 client 만) 타 workspace 의
    /// grid 를 구동하지 못하게 막는다.
    ///
    /// 반환: 적용 시도 여부(`false` = anchor workspace 미발견/holder 아님/surface
    /// 없음). 실제 grid 변화 판정은 `Terminal::resize` 내부(동일값이면 no-op)이며,
    /// 변화가 있으면 기존 resize tap 이 server→client `Resize` echo 를 자동
    /// fan-out 한다 — 여기서 추가로 push 하지 않는다(echo 경로 재사용).
    pub fn apply_attached_workspace_resize(
        &mut self,
        client_id: AttachClientId,
        remote_surface_id: u32,
        cols: usize,
        rows: usize,
    ) -> bool {
        let Some(ws) = self.attach.workspace_of_surface(remote_surface_id) else {
            return false;
        };
        if self.attach.workspace_holder(ws) != Some(client_id) {
            return false;
        }
        if let Some(terminal) = self.terminals.get_mut(remote_surface_id) {
            terminal.resize(cols, rows);
            true
        } else {
            false
        }
    }

    /// mirror client 가 보낸 attention **해제 edge**(`StreamControl::ClientAttentionClear`)를
    /// 적용한다 — `apply_attached_workspace_resize` 와 같은 holder 검증 형태다.
    ///
    /// 해제 판정 자체는 미러 인스턴스의 사용자 행동(실 렌더 포커스 / 알림 읽음)이고,
    /// 이 함수는 그 판정을 surface 를 소유한 쪽에 적용할 뿐이다. 인가는 기존 모델
    /// 그대로 "attach 하드 점유 = 그 워크스페이스의 주체"(ADR-0040) — 요청한 client
    /// 가 anchor surface 워크스페이스의 holder 가 아니면 무시한다.
    ///
    /// 반환: 실제로 적용했는지(호출자가 소유 engine 을 찾을 때 쓴다).
    pub fn apply_attached_attention_clear(
        &mut self,
        client_id: AttachClientId,
        remote_surface_id: u32,
    ) -> bool {
        let Some(ws) = self.attach.workspace_of_surface(remote_surface_id) else {
            return false;
        };
        if self.attach.workspace_holder(ws) != Some(client_id) {
            return false;
        }
        // 서버의 surface 는 mirror 가 아니므로 `clear_attention` 이 다시 forward
        // 큐에 넣지 않는다(에코 없음). 반환값(제거 여부)은 여기서 소비하지 않는다 —
        // 레코드가 이미 없었어도 "이 engine 이 그 surface 의 주인" 이라는 사실은
        // 변하지 않으므로 호출자에게는 적용 성공으로 알린다.
        self.clear_attention(remote_surface_id);
        true
    }

    /// 1Hz busy-poll tick 마다 호출 — `busy_activity_forwards`(순수 diff, hub 비의존)가
    /// 계산한 변화분을 실제로 attach client 에 push 한다. gui(`app/busy.rs`, 매 window/
    /// parked engine)와 headless(`boot.rs`, 유일한 engine) 양쪽이 같은 1Hz 캐던스로 호출
    /// 하는 공통 진입점 — resize forwarder(`attach_surface_for_stream`)와 달리 busy 는
    /// `Terminal` 자체 tap 이 아니라 OS 폴링 기반이라 전용 스레드 대신 기존 busy tick
    /// 타이머에 편승한다.
    pub fn forward_busy_activity(&mut self, hub: &StreamHub) {
        for (client_id, surface_id, busy) in self.busy_activity_forwards() {
            let msg = StreamControl::Activity { surface_id, busy };
            let frame = StreamFrame::new(
                StreamTag::Control,
                serde_json::to_vec(&msg).unwrap_or_default(),
            );
            let _ = hub.push(client_id, frame); // best-effort activity 통지 — client 끊김 시 무해, 다음 tick 이 재수렴.
        }
    }

    /// 점유 surface 의 attention 변화분을 attach client 에 push 한다
    /// (`forward_busy_activity` 동형 — 같은 1Hz tick 에서 나란히 호출된다).
    ///
    /// attention 의 진실 원천은 **surface 를 소유한 인스턴스**다: producer 4 종(완료
    /// IPC/CLI, Claude 플러그인 훅, OSC 133 명령 완료, toast)이 전부 PTY 가 있는 쪽에서
    /// 돌고, 특히 `NeedsInput` 은 서버 훅에서만 나오므로 mirror 가 스스로 계산할
    /// 방법이 없다. 이 push 가 mirror attention 의 유일한 소스다.
    pub fn forward_attention(&mut self, hub: &StreamHub) {
        for (client_id, surface_id, kind) in self.attention_forwards() {
            let msg = StreamControl::Attention {
                surface_id,
                kind: kind.map(|k| k.to_wire()),
            };
            let frame = StreamFrame::new(
                StreamTag::Control,
                serde_json::to_vec(&msg).unwrap_or_default(),
            );
            let _ = hub.push(client_id, frame); // best-effort attention 통지 — client 끊김 시 무해, 다음 tick 이 재수렴.
        }
    }

    /// 점유 surface 의 cwd 변화분을 attach client 에 push 한다(`forward_busy_activity` 동형
    /// — 같은 1Hz tick 에서 나란히 호출된다).
    ///
    /// mirror terminal 은 로컬 PTY 가 없어 원격 셸이 OSC 7 을 방출할 때만 cwd 를 알 수
    /// 있다. 서버는 PTY 를 소유하므로 OSC 7 이 없어도 OS 조회로 안다 — 아는 쪽이 모르는
    /// 쪽으로 보내는 채널이다(ADR-0267).
    pub fn forward_surface_cwd(&mut self, hub: &StreamHub) {
        for (client_id, surface_id, cwd) in self.surface_cwd_forwards() {
            let msg = StreamControl::Cwd { surface_id, cwd };
            let frame = StreamFrame::new(
                StreamTag::Control,
                serde_json::to_vec(&msg).unwrap_or_default(),
            );
            let _ = hub.push(client_id, frame); // best-effort cwd 통지 — client 끊김 시 무해, 다음 tick 이 재수렴.
        }
    }

    /// workspace attach 디스크립터: 트리(분할 비율 포함) + per-surface role/cols/rows/kind.
    fn build_workspace_descriptor(
        &self,
        idx: usize,
        workspace_id: u32,
        class: &AttachSurfaceClass,
    ) -> serde_json::Value {
        let (tree, surfaces) = self.build_workspace_tree_surfaces(idx, class);
        serde_json::json!({
            "event": "attached_workspace",
            "workspace_id": workspace_id,
            "name": self.workspaces[idx].name,
            "tree": tree,
            "surfaces": surfaces,
        })
    }

    /// `(tree, surfaces)` 페이로드 — 핸드셰이크 디스크립터(`build_workspace_descriptor`)와
    /// 3단계 역반영 delta([`StreamControl::StructuralDelta`])가 공유한다. tree 는
    /// `to_attach_tree_json`(분할 방향/비율 포함), surfaces 는 per-surface
    /// role/cols/rows/kind.
    pub(crate) fn build_workspace_tree_surfaces(
        &self,
        idx: usize,
        class: &AttachSurfaceClass,
    ) -> (serde_json::Value, Vec<serde_json::Value>) {
        let ws = &self.workspaces[idx];
        // sid → kind (비-터미널 placeholder 라벨용) / display_name(mesh 탭 제목용 —
        // 서버는 이미 `Surface::display_name()` 을 들고 있는데 mesh 디스크립터가
        // 이를 안 실어보내 client 가 kind 문자열("markdown")로 대체하던 버그).
        let mut kinds: HashMap<u32, &'static str> = HashMap::new();
        let mut display_names: HashMap<u32, String> = HashMap::new();
        for pane_id in ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pane_id) {
                for tab in &pane.tabs {
                    tab.for_each_surface(&mut |s| {
                        if let Some(id) = s.surface_id() {
                            kinds.insert(id, s.kind());
                            display_names.insert(id, s.display_name());
                        }
                    });
                }
            }
        }
        // mesh 후보를 bundled 화이트리스트로 재검증. 통과 못한 후보는
        // non_terminals 와 동일하게 placeholder 로 내려간다.
        let (mesh_whitelisted, mesh_rejected) = mesh_mirror_candidates(class);
        // content 후보(ADR-0255)도 같은 두 단 — 통과 못한 후보(html 등 다른 webview
        // kind)는 placeholder 로 내려간다.
        let (content_whitelisted, content_rejected) = content_mirror_candidates(class);

        let mut surfaces = Vec::new();
        for &sid in &class.terminals {
            let (cols, rows) = self
                .terminals
                .get(sid)
                .map(|t| (t.cols(), t.rows()))
                .unwrap_or((80, 24));
            surfaces.push(serde_json::json!({
                "remote_id": sid,
                "role": "terminal",
                "cols": cols,
                "rows": rows,
            }));
        }
        for (sid, kind, plugin_id) in &mesh_whitelisted {
            let display_name = display_names
                .get(sid)
                .cloned()
                .unwrap_or_else(|| kind.to_string());
            surfaces.push(serde_json::json!({
                "remote_id": sid,
                "role": "mesh",
                "kind": kind,
                "plugin_id": plugin_id,
                "display_name": display_name,
            }));
        }
        // ADR-0059 — explorer 는 placeholder 가 아니라 전용 role. root(활성
        // 탭의 현재 디렉토리)만 실어보낸다(cwd 는 별도 필드로 보내지 않고 client 가
        // `ExplorerPanel::new(id, root)` 로 cwd == root 단순화 — ADR 이 위임한 좁은
        // 구현 디테일).
        for (sid, root) in &class.explorers {
            surfaces.push(serde_json::json!({
                "remote_id": sid,
                "role": "explorer",
                "root": root.to_string_lossy(),
            }));
        }
        // ADR-0255 — markdown 은 placeholder 가 아니라 전용 role. 내용은 여기 싣지
        // 않고(트리 디스크립터가 문서 크기만큼 부풀지 않게) client 가
        // `markdown_content_request` 로 따로 가져온다. `file` 은 **표시·제목 전용의
        // opaque 문자열**이다 — client 는 이 값으로 자기 로컬 파일을 열지 않는다
        // (두 인스턴스의 파일시스템이 다르다).
        for (sid, _kind, file) in &content_whitelisted {
            let display_name = display_names
                .get(sid)
                .cloned()
                .unwrap_or_else(|| kinds.get(sid).copied().unwrap_or("markdown").to_string());
            surfaces.push(serde_json::json!({
                "remote_id": sid,
                "role": "markdown",
                "file": file.as_ref().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                "display_name": display_name,
            }));
        }
        for &sid in class
            .non_terminals
            .iter()
            .chain(mesh_rejected.iter())
            .chain(content_rejected.iter())
        {
            surfaces.push(serde_json::json!({
                "remote_id": sid,
                "role": "placeholder",
                "kind": kinds.get(&sid).copied().unwrap_or("unknown"),
            }));
        }
        (ws.to_attach_tree_json(), surfaces)
    }
}

/// mirror client 가 forward 한 구조 op 를 이 인스턴스(원격 = authoritative)에서 실행한
/// 결과로, 성공 시 client 에 역반영할 delta 를 담는다(3단계).
#[derive(Debug)]
pub(crate) struct ForwardedDelta {
    /// client 에 push 할 `StreamControl::StructuralDelta`(원격 ws 전체 트리+surfaces).
    pub delta: tasty_ipc::stream::StreamControl,
    /// 이 op 로 **새로 생긴** 터미널 surface 들. 호출자가 delta push **직후**
    /// [`CoreState::tap_surface_for_stream`] 로 tap 을 건다(스냅샷이 client 매핑 생성
    /// 뒤에 도착하도록 delta 다음 순서를 보장).
    pub added_terminals: Vec<SurfaceId>,
    /// `ConvertSurface` 가 실제로 kind 를 교체한(`replaced=true`) 경우의 대상
    /// surface_id. egui-mesh(markdown 등) 로 제자리 변환 시 같은 surface_id 에 stale
    /// frame 이 남는 문제를 막으려면 `PluginManager::drop_egui_mesh_frame` 호출이
    /// 필요한데, plugin manager 는 이 함수가 받는 상태(`Core`/`AppState`/`CoreState`)
    /// 어디에도 없고 두 빌드 모두 그것을 소유하는 쪽이 이 함수의 호출자다
    /// (`app/event_handler.rs` · `boot/headless_stream.rs`). 그래서 실행 결과를 **값으로**
    /// 돌려주고 호출자가 투영한다 — 로컬(비-forward) 변환 경로의 동일 처리
    /// (`app/dispatch_domain.rs` 의 `SurfaceConverted` 분기)와 짝을 맞춘다. 이 모양을
    /// 유지하는 근거와 대안은
    /// `docs/adr/0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md`.
    pub converted_surface: Option<SurfaceId>,
}

/// mirror client 가 forward 한 구조 op 를 이 인스턴스(원격 = authoritative)에서
/// 실행한다. anchor **원격 surface id** 로 pane/tab/workspace 를 resolve 한 뒤 도메인 실행
/// 함수(`core::structural_exec` 의 split / create_tab / close_tab / move_tab / close_pane /
/// close_surface)를 부른다 — IPC 핸들러가 부르는 것과 **같은 함수**라 검증·cascade(PTY
/// spawn·host event·cleanup)·실패 문구가 두 진입점에서 같다. 서버측 워크스페이스는 mirror
/// 가 아니므로 `Core::apply` 의 mirror 가드에 걸리지 않고 실제로 실행된다.
/// `ConvertSurface`/`RestoreClosedItem`/`MoveSurface` 는 대응하는 도메인 실행 함수가 없어
/// (generic `surface.convert`/`surface.move` IPC 가 없고 복원은 IPC 로 노출된 적이 없다)
/// `Core::apply(DomainIntent)` 를 직접 호출하고, 그 cascade(PTY cleanup / mesh stale-frame
/// 방지)도 이 함수 안에서(또는 `converted_surface` 를 통해 호출자가) 직접 재현한다.
///
/// IPC 진입점의 요청 게이트(호출자 자기 대상 거절 · 하드 점유 거절 · 요청당 한 번의 권한
/// 판정)는 여기서 돌지 않는다 — 이 경로의 게이트는 호출자의 holder 검증이다.
///
/// 반환:
/// - 성공: `Ok(Some(delta))` — anchor 워크스페이스의 실행 **전/후** `all_surface_ids`
///   diff 로 added(신규 터미널)를 계산하고, 실행 후 트리+surfaces 를 담은
///   [`StreamControl::StructuralDelta`] 를 만들어 반환한다(실행 결과 파싱이 아니라
///   트리 diff 라 close cascade·move 도 균일 커버). 워크스페이스가 통째로 사라진 극단
///   케이스는 `Ok(None)`.
/// - 실패: `Err(reason)` — 사유 문자열 하나다. 도메인 실행 함수를 타는 op 는 그 실패
///   문구(예: 원격 미등록 plugin kind → "unknown surface kind")를 [`forward_result`] 가
///   그대로 옮기고 — IPC 진입점이 같은 실패에 싣는 JSON-RPC 에러 메시지와 byte 단위로 같다 —,
///   나머지 op(convert / restore / move-surface)는 이 함수가 사유를 만든다.
///
/// 호출자(메인루프)가 [`StreamControl::StructuralResult`] 로 회신한 **뒤** delta 를 push
/// 하고, 그 다음 added_terminals 를 tap 한다(순서: result → delta → snapshot).
///
/// `origin` 은 client 가 그 op 를 누구의 요청으로 보냈는가다(wire 에 칸이 없으면 이미
/// `User` 로 풀려 온다). close 계열만 읽는다 — `User` 면 서버 복원 스택에 남기고,
/// `Agent` 면 남기지 않는다(`docs/identity.md` 원칙 1, ADR-0480).
pub(crate) fn execute_forwarded_structural_op(
    core: &mut crate::core::Core,
    state: &mut dyn crate::core::cascade_window::CascadeWindow,
    engine: &mut CoreState,
    op: &StructuralOp,
    origin: tasty_ipc::stream::ForwardOrigin,
) -> Result<Option<ForwardedDelta>, String> {
    use crate::core::structural_exec::{self as exec, SplitLevel, SplitRequest};
    use serde_json::json;
    use std::collections::HashSet;

    // anchor 워크스페이스를 실행 **전** 확보(close 로 anchor surface 가 사라져도 ws id
    // 로 재조회 가능하게). before-set 은 delta 의 added 계산 기준.
    let ws_id = engine
        .find_workspace_index_for_surface(op.anchor_surface_id())
        .map(|(idx, _)| engine.workspaces[idx].id);
    let before: HashSet<SurfaceId> = ws_id
        .and_then(|id| engine.find_workspace_index_for_id(id))
        .map(|idx| {
            engine.workspaces[idx]
                .all_surface_ids()
                .into_iter()
                .collect()
        })
        .unwrap_or_default();

    // `ConvertSurface` 성공(kind 실제 교체) 시에만 채워진다 — 호출자가
    // `PluginManager::drop_egui_mesh_frame` 을 트리거하는 신호(위 `ForwardedDelta` 문서
    // 참조).
    let mut converted_surface: Option<SurfaceId> = None;
    // 복원 스택에 남기는가 — 원격 **사용자**의 손 조작일 때만이다. 원격 에이전트의
    // close 를 남기면 서버 앞 사용자의 Ctrl+Shift+T 가 에이전트가 닫은 것을 되살린다.
    let restorable = origin == tasty_ipc::stream::ForwardOrigin::User;

    let outcome: Result<(), String> = match op {
        StructuralOp::SplitSurface {
            surface_id,
            direction,
            surface_kind,
            params,
        } => {
            let p = structural_params(
                params,
                json!({
                    "level": "surface",
                    "direction": direction.as_ipc_str(),
                    "target_surface": surface_id,
                    "type": surface_kind,
                }),
            );
            // 원격이 보낸 묶음에 `target_pane` 이 실려 있으면 IPC 와 같은 규칙으로 읽는다 —
            // 잘못 온 값은 그 문구로, 있으면 대상 둘 지정으로 거절된다.
            let target_pane = crate::core::param_bag::read_u32(&p, "target_pane")?;
            let req = SplitRequest {
                level: SplitLevel::Surface,
                direction: split_direction(*direction),
                target_surface: Some(*surface_id),
                target_pane,
                params: &p,
            };
            // 이 실행은 새 터미널을 만들면 내부에서 `tap_new_workspace_member` 를
            // 호출한다 — 그 즉시-tap 을 억제한다(이 함수 끝의 added_terminals 루프가
            // delta 전송 후 정확한 순서로 직접 tap 한다). 결과를 바인딩만 하고 구간을
            // 닫은 뒤에 판정하므로 사이에 `?` 로 새는 경로가 없다.
            engine.attach.set_auto_tap_suppressed(true);
            let result = exec::split(core, state, engine, req);
            engine.attach.set_auto_tap_suppressed(false);
            forward_result(result)
        }
        StructuralOp::SplitPane {
            anchor_surface_id,
            direction,
            surface_kind,
            params,
        } => {
            // pane-level split 은 target_surface 로도 pane 을 resolve 한다(`exec::split`).
            let p = structural_params(
                params,
                json!({
                    "level": "pane",
                    "direction": direction.as_ipc_str(),
                    "target_surface": anchor_surface_id,
                    "type": surface_kind,
                }),
            );
            let target_pane = crate::core::param_bag::read_u32(&p, "target_pane")?;
            let req = SplitRequest {
                level: SplitLevel::Pane,
                direction: split_direction(*direction),
                target_surface: Some(*anchor_surface_id),
                target_pane,
                params: &p,
            };
            engine.attach.set_auto_tap_suppressed(true);
            let result = exec::split(core, state, engine, req);
            engine.attach.set_auto_tap_suppressed(false);
            forward_result(result)
        }
        StructuralOp::NewTab {
            anchor_surface_id,
            surface_kind,
            params,
        } => {
            let pane_id = engine
                .find_pane_for_surface(*anchor_surface_id)
                .ok_or_else(|| format!("anchor surface {anchor_surface_id} not found"))?;
            let p = structural_params(params, json!({ "pane_id": pane_id, "type": surface_kind }));
            engine.attach.set_auto_tap_suppressed(true);
            // 원격 사용자의 손 조작이면 선택하고, 원격 에이전트면 서버 앞 사용자의 탭을
            // 바꾸지 않는다(ADR-0502) — 복원 스택을 가르는 `restorable` 과 같은 축이다.
            let activate = origin == tasty_ipc::stream::ForwardOrigin::User;
            let result = exec::create_tab(core, state, engine, pane_id, &p, activate);
            engine.attach.set_auto_tap_suppressed(false);
            forward_result(result)
        }
        StructuralOp::CloseSurface { surface_id } => {
            // holder 자신이 보낸 close 라 IPC 진입점의 하드 점유 검사를 지나지 않고 도메인
            // 실행을 직접 부른다 — 면제를 params 플래그로 두면 아무 에이전트나 같은 키를 실어
            // 우회한다. save_snapshot 은 op 의 origin 이 정한다: 원격 **사용자**의 손
            // 조작이면 되돌릴 수 있어야 하고(ADR-0264 결정 4), 원격 에이전트의 close 면
            // 사용자 복원 스택에 닿지 않는다(ADR-0480).
            forward_result(exec::close_surface(
                core,
                state,
                engine,
                *surface_id,
                restorable,
            ))
        }
        StructuralOp::CloseTab { anchor_surface_id } => {
            let tab_id = engine
                .find_tab_for_surface(*anchor_surface_id)
                .ok_or_else(|| format!("anchor surface {anchor_surface_id} tab not found"))?;
            // 원격 **사용자**가 손으로 닫은 것은 되돌릴 수 있어야 한다(ADR-0264 결정 4).
            // `CloseSurface` 는 holder 진입점이 save_snapshot 인자로 갈라 들어가지만
            // `apply_close_tab` 에는 그 축이 없으므로, 여기서 핸들러가 트리를 건드리기
            // **전에** 직접 캡처한다. 원격 에이전트의 close 는 캡처하지 않는다(ADR-0480).
            if let Some(item) = restorable
                .then(|| engine.find_pane_for_tab(tab_id))
                .flatten()
                .and_then(|pane_id| {
                    let idx = engine
                        .find_pane_by_id(pane_id)?
                        .tabs
                        .iter()
                        .position(|t| t.id == tab_id)?;
                    Some((pane_id, idx))
                })
                .and_then(|(pane_id, idx)| engine.capture_closed_tab(pane_id, idx))
            {
                engine.push_closed_item(item);
            }
            forward_result(exec::close_tab(core, state, engine, tab_id))
        }
        StructuralOp::ClosePane { anchor_surface_id } => {
            let pane_id = engine
                .find_pane_for_surface(*anchor_surface_id)
                .ok_or_else(|| format!("anchor surface {anchor_surface_id} pane not found"))?;
            // 위 `CloseTab` 과 같은 근거. pane 캡처는 `close_pane` 이 트리를 재배치하기
            // 전이어야 split context 가 남는다(`capture_closed_pane` 의 doc).
            if let Some(item) = restorable
                .then(|| engine.capture_closed_pane(pane_id))
                .flatten()
            {
                engine.push_closed_item(item);
            }
            forward_result(exec::close_pane(core, state, engine, pane_id))
        }
        StructuralOp::MoveTab {
            anchor_surface_id,
            from_index,
            to_index,
        } => {
            let pane_id = engine
                .find_pane_for_surface(*anchor_surface_id)
                .ok_or_else(|| format!("anchor surface {anchor_surface_id} pane not found"))?;
            forward_result(exec::move_tab(
                core,
                engine,
                pane_id,
                *from_index,
                *to_index,
            ))
        }
        StructuralOp::ConvertSurface {
            surface_id,
            surface_kind,
            params,
            cwd,
        } => {
            // generic `surface.convert` IPC 가 없어 대응하는 도메인 실행 함수가 없다 —
            // `image::handle_open` 과 동일한 형태(`Core::apply` 직접 호출 +
            // `SurfaceConverted{replaced}` 로 성공 판정)를 여기 직접 재현한다.
            //
            // cwd carry (`ConvertSurfaceTarget::{Terminal,Kind}.cwd`) 우선순위:
            // **op 의 `cwd` > 서버 자체 resolve**. 전자는 client 가 source surface
            // 에서 해석한 값(mirror 터미널은 detached 라 원격 셸의 OSC 7 이 있을 때만
            // 채워진다), 후자는 `handle_git_query_request` 와 같은 근거로 서버가
            // **실제 원격 PTY** 기준(`Terminal::get_cwd` — OSC 7 캐시 우선, 없으면
            // Linux `/proc`·macOS `proc_pidinfo` 폴백)으로 직접 판정한다. 이 순서라
            // 원격 셸이 OSC 7 을 방출하지 않는 경우까지 커버된다.
            //
            // 서버 resolve 는 로컬 convert 와 같은 헬퍼를 써서 `inherit_cwd` 설정
            // 게이트를 그대로 적용한다 — 실행 주체가 원격 인스턴스이므로 그 인스턴스의
            // 설정 의미론을 따르는 쪽이 로컬 실행과 대칭이다.
            // (`docs/architecture/invariants/surface-cwd.md` §3)
            use crate::core::intent::ConvertSurfaceTarget;
            let carried_cwd = cwd
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| state.resolve_inherit_cwd_from_surface(engine, *surface_id));
            let target = if surface_kind == "terminal" {
                ConvertSurfaceTarget::Terminal { cwd: carried_cwd }
            } else {
                ConvertSurfaceTarget::Kind {
                    cwd: carried_cwd,
                    kind: surface_kind.clone(),
                    params: params.clone(),
                }
            };
            let intent = crate::core::intent::DomainIntent::ConvertSurface {
                surface_id: *surface_id,
                target,
            };
            match core.apply(engine, intent) {
                Ok(events) => {
                    let replaced = matches!(
                        events.into_iter().next(),
                        Some(crate::core::intent::CoreEvent::SurfaceConverted {
                            replaced: true,
                            ..
                        })
                    );
                    if replaced {
                        converted_surface = Some(*surface_id);
                        Ok(())
                    } else {
                        Err(format!("surface {surface_id} not found"))
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
        StructuralOp::RestoreClosedItem { anchor_surface_id } => {
            // 복원은 IPC/CLI 로 노출된 적이 없어 대응하는 도메인 실행 함수가 없다 — `ConvertSurface`
            // /`MoveSurface` 와 같은 형태로 `Core::apply` 를 직접 부른다.
            let pane_id = engine
                .find_pane_for_surface(*anchor_surface_id)
                .ok_or_else(|| format!("anchor surface {anchor_surface_id} pane not found"))?;
            let ws_id = engine
                .find_workspace_index_for_pane(pane_id)
                .map(|idx| engine.workspaces[idx].id)
                .ok_or_else(|| format!("pane {pane_id} workspace not found"))?;
            let intent = crate::core::intent::DomainIntent::RestoreClosedItem {
                target_pane_id: Some(pane_id),
                // 그 워크스페이스에서 닫힌 항목만 후보다(ADR-0264 결정 3) — 서버 앞에
                // 앉은 사용자의 히스토리를 가져가지 않고, 복원 결과가 anchor 워크스페이스
                // 안에 떨어져 아래 before/after diff 가 그것을 delta 로 싣는다.
                scope: crate::core::intent::RestoreScope::Workspace(ws_id),
            };
            match core.apply(engine, intent) {
                Ok(events) => {
                    let restored = matches!(
                        events.into_iter().next(),
                        Some(crate::core::intent::CoreEvent::ClosedItemRestored {
                            restored: true,
                            ..
                        })
                    );
                    if restored {
                        // ★ 로컬 경로의 cascade(`cascade_closed_item_restored`)를 여기서
                        // 재현하지 않는다. 그 함수의 두 갈래는 `AppState::active_workspace`
                        // 와 `focused_pane` 을 바꾸는데, forward 경로에서 그것을 부르면
                        // **원격 사용자의 조작이 이 기계 앞에 앉은 사용자의 화면을
                        // 움직인다** — 루트 CLAUDE.md 원칙 1·3 과
                        // `docs/design/policies/focus.md` 가 금지하는 형태다. client 쪽
                        // focus 는 `PendingOpFocus::NewResource` 가 delta 적용 시점에
                        // client-only 로 보정하므로 서버 상태를 만질 이유가 없다.
                        Ok(())
                    } else {
                        // "복원할 것 없음" 은 실패가 아니다 — client 가 일반 forward 실패
                        // 문구 대신 전용 안내를 쓰도록 sentinel 을 그대로 실패 사유로 쓴다.
                        Err(tasty_ipc::stream::STRUCTURAL_REASON_RESTORE_EMPTY.to_string())
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
        StructuralOp::MoveSurface {
            source_surface_id,
            target_surface_id,
        } => {
            // generic `surface.move` IPC 도 없다(현재 유일한 발행 경로는 우클릭
            // "잘라내기 → 여기로 이동" GUI, IPC/CLI 미노출) — Convert 와 동일하게
            // `Core::apply` 를 직접 호출한다. `build_mirror_forward_op` 가 이미
            // source/target 이 같은 mirror workspace 에 속할 때만 이 op 를 만들어
            // 보내므로, 여기(서버·authoritative 워크스페이스)서는 두 id 모두 같은
            // 워크스페이스 안의 실제 surface 로 resolve 된다.
            let intent = crate::core::intent::DomainIntent::MoveSurface {
                source_surface_id: *source_surface_id,
                target_surface_id: *target_surface_id,
            };
            match core.apply(engine, intent) {
                Ok(events) => {
                    let ev = events.into_iter().next();
                    if !matches!(
                        ev,
                        Some(crate::core::intent::CoreEvent::MoveSurfaceApplied { .. })
                    ) {
                        return Err("Core::apply returned no MoveSurfaceApplied event".to_string());
                    }
                    // B(target) 의 PTY kill + tab/pane/workspace cascade. 필드 매핑은
                    // `SurfaceCloseCascade::from_move_surface_applied` 한 자리가 소유하고,
                    // 로컬 dispatcher 도 같은 것을 쓴다. IPC 는 agent 경로라
                    // is_user_close=false(`close_surface_via_intent` 와 동일 근거).
                    match ev.and_then(|ev| {
                        crate::core::structural_cascade::SurfaceCloseCascade::from_move_surface_applied(
                            ev, false,
                        )
                    }) {
                        Some(c) => {
                            crate::core::structural_cascade::cascade_surface_closed(
                                core, state, engine, c,
                            );
                            Ok(())
                        }
                        None => Err(format!(
                            "move failed: source={source_surface_id} target={target_surface_id}"
                        )),
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
    };

    outcome?;

    // 이 op 가 anchor 워크스페이스에 낸 구조 변경은 아래 delta(또는 강제 detach)가 전부
    // 싣는다 — 실행 중 닫힌 멤버가 남긴 "forward 아닌 변경" 표시를 지워 같은 트리가 두 번
    // 나가지 않게 한다(ADR-0481).
    if let Some(ws_id) = ws_id {
        engine.attach.clear_structure_changed(ws_id);
    }

    // 성공 — 실행 후 트리 스냅샷으로 delta 구성. anchor 를 못 찾았거나(방어) ws 가 통째로
    // 사라졌으면(극단) delta 없음.
    let Some(ws_id) = ws_id else {
        return Ok(None);
    };
    let Some(idx_after) = engine.find_workspace_index_for_id(ws_id) else {
        // workspace 자체가 cascade 로 purge 됐다(예: mirror 의 마지막 surface 를
        // 닫아 "Case 4: last pane in workspace" 가 워크스페이스를 통째로 지운 경우,
        // `core::mod::close_case_workspace`). mirror 로 반영할 트리가 더 없으므로
        // delta 로 되살리려 하지 않고, 이미 있는 강제 detach 통지 경로를 태워 holder
        // 를 정상적으로 끊는다 — `force_detach_workspace` 가 holder 에게 Control
        // "force_detached" + `Detach` 프레임을 push 하고 `workspace_locks`/
        // `surface_locks`/`surface_to_workspace` 를 함께 정리한다(purge 된 workspace
        // 를 가리키는 stale lock 방지).
        //
        // 순서 주의: 이 push 는 호출측(`event_handler.rs`/`boot.rs`)이 아직 보내지
        // 않은 `StructuralResult{ok:true}` 보다 **먼저** 소켓에 올라간다 — 이 함수가
        // 반환된 뒤에야 호출측이 회신을 push 하기 때문이다. "result → delta" 로
        // 문서화된 통상 순서와 다르지만, `StreamTag::Control` 의 "force_detached"
        // 문자열을 읽는 즉시 reader(`attach_client.rs`)가 disconnected=true 로
        // 세션을 끊고 그 뒤 프레임(뒤늦게 도착할 StructuralResult 포함)을 읽지
        // 않으므로 무해하다 — forward 는 holder 본인만 보낼 수 있어(위 호출측의
        // holder 검증) 자기 op 의 ack 를 못 받는 대신 더 명확한 강제분리 신호를
        // 즉시 받는 셈이다.
        engine.attach.force_detach_workspace(ws_id);
        return Ok(None);
    };
    let class = engine.workspaces[idx_after].classify_attach_surfaces();
    // added = after − before 중 터미널만(비-터미널 placeholder 는 tap 불필요).
    let added_terminals: Vec<SurfaceId> = class
        .terminals
        .iter()
        .copied()
        .filter(|sid| !before.contains(sid))
        .collect();
    // 점유 상속(ADR-0040 "workspace 전체가 remote" 불변식): forward 로 원격에 새로 생긴
    // 터미널을 이 workspace 의 hard 점유에 편입한다. 등록하지 않으면 새 surface 가
    // 비점유로 남아 (1) host 창 sweep 이 자기 grid 로 되돌리고(레터박스) (2) is_hard_occupied
    // 미표시 (3) `feed_attached_workspace_input`/`apply_attached_workspace_resize` 의
    // holder 검증(surface_to_workspace 기반)이 실패해 client 입력·resize 가 거부된다.
    for sid in &added_terminals {
        engine.attach.add_workspace_member(ws_id, *sid, true);
    }
    let (tree, surfaces) = engine.build_workspace_tree_surfaces(idx_after, &class);
    let delta = tasty_ipc::stream::StreamControl::StructuralDelta {
        workspace_id: ws_id,
        tree,
        surfaces,
    };
    Ok(Some(ForwardedDelta {
        delta,
        added_terminals,
        converted_surface,
    }))
}

/// 도메인 실행 결과를 forward 회신의 결과로 줄인다 — 성공 값은 버리고(회신은 성공 여부와
/// 사유 문자열 하나다) 실패는 그 문구를 그대로 쓴다. IPC 진입점이 같은 실패를 JSON-RPC
/// 에러 메시지로 싣는 문구와 byte 단위로 같다(`handler::structural_failure_response`).
///
/// mirror 로 다시 forward 된 차단(`MirrorStructuralBlocked { forwarded: true }`)은 실패가
/// 아니다 — IPC 진입점은 그것을 `{forwarded: true}` 성공으로 답하고, 이 경로도 그 답을
/// 성공으로 읽어 왔다. 이 워크스페이스 자체가 또 다른 인스턴스의 mirror 인 연쇄 attach
/// 에서만 난다.
fn forward_result<T>(
    result: Result<T, crate::core::structural_exec::StructuralFailure>,
) -> Result<(), String> {
    use crate::core::structural_exec::StructuralFailure;
    match result {
        Ok(_) => Ok(()),
        Err(StructuralFailure::Rejected(msg)) => Err(msg),
        Err(StructuralFailure::MissingEvent(msg)) => Err(msg.to_string()),
        Err(StructuralFailure::Apply(e)) => {
            if e.downcast_ref::<crate::core::MirrorStructuralBlocked>()
                .is_some_and(|blocked| blocked.forwarded)
            {
                Ok(())
            } else {
                Err(e.to_string())
            }
        }
    }
}

/// op 의 split 축을 도메인 방향으로. IPC 진입점이 `direction` 문자열을 읽는 규칙
/// (`"horizontal"` 만 가로, 나머지는 세로)과 같은 답을 낸다 — 예전에는 이 값을
/// `as_ipc_str()` 로 문자열로 만든 뒤 그 규칙으로 되읽었다.
fn split_direction(axis: tasty_ipc::stream::SplitAxis) -> crate::model::SplitDirection {
    match axis {
        tasty_ipc::stream::SplitAxis::Horizontal => crate::model::SplitDirection::Horizontal,
        tasty_ipc::stream::SplitAxis::Vertical => crate::model::SplitDirection::Vertical,
    }
}

/// forward 된 op 의 kind params(있으면)에 제어 키(level/direction/target_surface/type/
/// pane_id)를 덮어 얹는다. `base` 가 객체가 아니면 빈 객체에서 시작한다. 결과는 도메인
/// 실행이 IPC 요청의 params 와 같은 자리로 읽는 파라미터 묶음이고, 그대로 새 surface 의
/// `surface_params` 가 된다 — 그래서 제어 키도 IPC 요청이 싣던 모양 그대로 남긴다.
fn structural_params(base: &serde_json::Value, control: serde_json::Value) -> serde_json::Value {
    let mut obj = base.as_object().cloned().unwrap_or_default();
    if let Some(ctrl) = control.as_object() {
        for (k, v) in ctrl {
            obj.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(obj)
}

/// (screenshot→remote-clipboard) mirror client 가 워크스페이스 attach 채널로
/// 보낸 캡처 업로드의 commit 을 처리한다. `client_id` 가 이 engine 이 호스팅하는
/// 어떤 workspace 든 점유(holder)하고 있어야 신뢰한다(구조 op forward 와 동일한
/// "attach 점유 = 권한" 원칙 — 별도 캡슐화된 권한 레이어 없음). 누적 바이트를
/// `~/.tasty/screenshots/<file_name>` 에 쓰고 이 인스턴스(= mirror 관점의 "원격")의
/// 클립보드에 그 경로를 기록한다. 결과는 `capture_result` 커스텀 이벤트로 회신
/// (best-effort, `StreamControl` enum 은 건드리지 않고 그 enum 이 인식 못 하는
/// "event" 값을 같은 `StreamTag::Control` 채널에 실어 보낸다 — stream_hub.rs 의
/// 미지 payload 무시 특성을 그대로 이용).
pub(crate) fn finalize_capture_upload(
    engine: &mut CoreState,
    core: &Core,
    hub: &StreamHub,
    client_id: u32,
    upload_id: u64,
    file_name: &str,
) {
    let bytes = engine.capture_uploads.take(client_id, upload_id);
    let is_holder = engine.attach.client_holds_workspace(client_id);
    let result = match (is_holder, bytes) {
        (false, _) => Err("client does not hold a workspace attach".to_string()),
        (true, None) => Err("no uploaded bytes for this upload_id".to_string()),
        (true, Some(bytes)) => save_capture_and_set_clipboard(core, file_name, &bytes),
    };
    let payload = match &result {
        Ok(path) => serde_json::json!({
            "event": "capture_result",
            "upload_id": upload_id,
            "ok": true,
            "path": path,
        }),
        Err(reason) => serde_json::json!({
            "event": "capture_result",
            "upload_id": upload_id,
            "ok": false,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // best-effort 회신 — client 끊김 시 무해.
}

/// `file_name` 의 basename 만 취해(경로 조작 방지) `~/.tasty/screenshots/` 밑에
/// 저장하고, 그 절대경로 문자열을 로컬 클립보드에 기록한다. 파일 저장 자체는 일반
/// [`save_bulk_file`] 로 위임하고, 클립보드 기록(스크린샷 특화 정책)만 여기 남긴다.
fn save_capture_and_set_clipboard(
    core: &Core,
    file_name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let dir = crate::paths::tasty_home()
        .map(|h| h.join("screenshots"))
        .ok_or_else(|| "no tasty home directory".to_string())?;
    let path_str = save_bulk_file(&dir, file_name, "screenshot.png", bytes)?;
    core.clipboard_arc()
        .write_text(&path_str)
        .map_err(|e| e.to_string())?;
    Ok(path_str)
}

/// 일반 파일 저장 원자: `file_name` 의 basename 만 취해(경로 조작 방지) `dir` 밑에
/// 쓰고 그 절대경로 문자열을 돌려준다. 저장 위치(`dir`)는 **인자**로 받아 하드코딩을
/// 피한다 — 호출자가 설정값(`remote_transfer.dir`)을 주입하는 훅이다. 캡처의
/// 클립보드 기록 같은 소비자-특화 후처리는 이 함수 밖에서 한다(bulk 는 경로만 회신).
/// basename 이 비면 `fallback_name` 을 쓴다.
fn save_bulk_file(
    dir: &std::path::Path,
    file_name: &str,
    fallback_name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let safe_name = std::path::Path::new(file_name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(safe_name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// bulk 전송 파일의 기본 저장 폴더 `~/.tasty/transfers/`(홈 미확인 시 `None`).
/// [`resolve_bulk_transfer_dir`] 가 설정값이 비었을 때 도출하는 폴백이다.
pub(crate) fn default_bulk_transfer_dir() -> Option<std::path::PathBuf> {
    crate::paths::tasty_home().map(|h| h.join("transfers"))
}

/// 원격 전송 저장 폴더 결정: 설정된 `remote_transfer.dir` 가 비어있지 않으면 그
/// 경로, 비었으면 기본 폴더(`~/.tasty/transfers/`). begin 용량 판정·commit 저장이
/// 공유한다(같은 폴더 기준이어야 사용량 계산과 저장 위치가 일치).
pub(crate) fn resolve_bulk_transfer_dir(
    settings: &tasty_settings::Settings,
) -> Option<std::path::PathBuf> {
    let configured = settings.remote_transfer.dir.trim();
    if configured.is_empty() {
        default_bulk_transfer_dir()
    } else {
        Some(std::path::PathBuf::from(configured))
    }
}

/// 지정 폴더의 사용량(바이트) — 1-depth 파일 크기 단순 합산. 폴더가 없으면 0
/// (첫 전송 전). 하위 디렉토리·심볼릭 링크는 세지 않는다(재귀/캐시는 후속 과제).
pub(crate) fn dir_used_bytes(dir: &std::path::Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// 용량 판정 술어: 저장 폴더 사용량 + 유입 파일 크기가 상한을 넘는지. 경계
/// `used + incoming == max_bytes` 는 허용, `> max_bytes` 는 거부(`>` 비교).
/// `saturating_add` 로 u64 오버플로 시에도 거부 쪽으로 안전하게 수렴한다.
fn exceeds_capacity(used: u64, incoming: u64, max_bytes: u64) -> bool {
    used.saturating_add(incoming) > max_bytes
}

/// begin 단계 용량 사전판정 + 등록. `resolve_bulk_transfer_dir` 사용량 +
/// `total_size` 가 `remote_transfer.max_mb` 상한을 넘으면 전송을 **등록하지 않고**
/// 즉시 `BulkResult{ok:false, reason:"capacity exceeded"}` 를 회신한다(청크가 한
/// 바이트도 수신·저장되지 않음 — 전송 실패 팝업의 입력). 경계: `used + total_size ==
/// max` 는 허용, `> max` 는 거부(`>` 비교). 통과 시 registry 에 begin 등록한다.
pub(crate) fn begin_bulk_transfer(
    engine: &mut CoreState,
    hub: &StreamHub,
    client_id: u32,
    transfer_id: u64,
    filename: String,
    total_size: u64,
) {
    let dir = resolve_bulk_transfer_dir(&engine.settings);
    let max_bytes = engine
        .settings
        .remote_transfer
        .max_mb
        .saturating_mul(1024 * 1024);
    let used = dir.as_deref().map(dir_used_bytes).unwrap_or(0);
    if exceeds_capacity(used, total_size, max_bytes) {
        tracing::warn!(
            "bulk transfer: capacity exceeded (used={used}, incoming={total_size}, max={max_bytes}) — rejecting begin"
        );
        let reply = StreamControl::BulkResult {
            transfer_id,
            ok: false,
            path: None,
            reason: Some("capacity exceeded".to_string()),
        };
        let frame = StreamFrame::new(
            StreamTag::Control,
            serde_json::to_vec(&reply).unwrap_or_default(),
        );
        let _ = hub.push(client_id, frame); // best-effort 거부 회신 — client 끊김 시 무해.
        return;
    }
    engine
        .bulk_transfers
        .begin(client_id, transfer_id, filename, total_size);
}

/// bulk 전송 commit 처리: 인가 검증 → 누적 바이트 저장 → 원격 경로 회신
/// (ADR-0054). `finalize_capture_upload` 와 동형의 일반화 버전이다.
///
/// **인가(조사 §6/E)**: 전용 bulk 연결은 workspace holder 가 아니므로
/// (`client_holds_workspace(bulk_client)==false`) capture 의 holder 검증을 그대로 쓸 수
/// 없다. 대신 이 연결이 핸드셰이크에서 결속한 `bulk_workspace` 에 **활성 holder 가
/// 존재하는가**(누군가 그 워크스페이스를 attach 중인가)를 검증한다. 같은 SSH 경계·
/// 같은 터널을 통과했다는 사실이 이미 SSH 위임 인가의 증거이며(ADR-0054 decision#5),
/// holder 존재 확인은 "이 워크스페이스가 실제 attach 중"이라는 타겟 유효성 검증이다.
///
/// `dir` 은 저장 폴더(호출자가 `resolve_bulk_transfer_dir` 로 주입, `None` 이면 홈 미확인). 클립보드 기록 같은
/// 소비자-특화 후처리는 하지 않는다 — bulk 는 경로만 `BulkResult` 로 회신한다.
pub(crate) fn finalize_bulk_transfer(
    engine: &mut CoreState,
    hub: &StreamHub,
    client_id: u32,
    transfer_id: u64,
    bulk_workspace: WorkspaceId,
    dir: Option<std::path::PathBuf>,
) {
    let authorized = engine.attach.workspace_holder(bulk_workspace).is_some();
    // 미인가여도 누적분을 꺼내 메모리를 비운다(대용량 partial 잔존 방지).
    let taken = engine.bulk_transfers.take(client_id, transfer_id);
    let result = if !authorized {
        Err("bound workspace has no active holder".to_string())
    } else {
        match (taken, dir) {
            (None, _) => Err("no uploaded bytes for this transfer_id".to_string()),
            (Some(_), None) => Err("no tasty home directory".to_string()),
            (Some((filename, bytes)), Some(d)) => {
                save_bulk_file(&d, &filename, "bulk-file", &bytes)
            }
        }
    };
    let reply = match result {
        Ok(path) => StreamControl::BulkResult {
            transfer_id,
            ok: true,
            path: Some(path),
            reason: None,
        },
        Err(reason) => StreamControl::BulkResult {
            transfer_id,
            ok: false,
            path: None,
            reason: Some(reason),
        },
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&reply).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // best-effort 회신 — client 끊김 시 무해.
}

/// file picker — mirror client 가 attach 채널로 보낸 `list_dir_request`
/// 하나를 처리한다. `client_id` 가 이 engine 이 호스팅하는 어떤 workspace 든
/// 점유(holder)해야 신뢰한다(구조 op forward/캡처 업로드와 동일한 "attach 점유 =
/// 권한" 원칙 — 로컬 plugin IPC 의 `FsRead` 권한 게이트와는 다른 신뢰
/// 모델, 하이브리드 설계 근거는 신규 ADR 참고). 대상 디렉토리를 공유
/// `read_dir_entries`(`crate::core::fs_list`)로 읽어 wire entries 로 변환해
/// 회신한다(best-effort — `StreamControl` enum 은 그대로 두고 그 enum 이 인식
/// 못 하는 "event" 값을 같은 `StreamTag::Control` 채널에 실어 보낸다, capture_result
/// 와 동일 패턴).
pub(crate) fn handle_list_dir_request(
    engine: &mut CoreState,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    dir: &str,
) {
    let is_holder = engine.attach.client_holds_workspace(client_id);
    let result = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        list_dir_for_request(dir)
    };
    let payload = match result {
        Ok((resolved_dir, entries)) => {
            let (wire_entries, truncated) = list_dir_entries_wire_capped(&entries);
            serde_json::json!({
                "event": "list_dir_result",
                "request_id": request_id,
                "ok": true,
                "dir": resolved_dir,
                "entries": wire_entries,
                "truncated": truncated,
            })
        }
        Err(reason) => serde_json::json!({
            "event": "list_dir_result",
            "request_id": request_id,
            "ok": false,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // best-effort 회신 — client 끊김 시 무해.
}

/// `dir` 이 빈 문자열이면 원격(=이 인스턴스) 홈 디렉토리를 루트로, 아니면 그
/// 경로를 그대로 읽는다. 이름순 정렬(디렉토리 우선)까지 마쳐 반환.
fn list_dir_for_request(
    dir: &str,
) -> Result<(String, Vec<crate::core::fs_list::DirEntryInfo>), String> {
    let path = if dir.trim().is_empty() {
        directories::BaseDirs::new()
            .map(|d| d.home_dir().to_path_buf())
            .ok_or_else(|| "no home directory".to_string())?
    } else {
        std::path::PathBuf::from(dir)
    };
    let mut entries = crate::core::fs_list::read_dir_entries(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission denied".to_string()
        } else {
            e.to_string()
        }
    })?;
    crate::core::fs_list::sort_entries(
        &mut entries,
        tasty_model::SortColumn::Name,
        tasty_model::SortDir::Asc,
    );
    Ok((path.to_string_lossy().to_string(), entries))
}

/// `list_dir_result` 프레임 하나의 entries 배열에 허용하는 직렬화 바이트 예산.
/// attach 채널의 프레임 하드 상한(`tasty_ipc::stream::MAX_FRAME_LEN`, 1MiB)보다
/// 충분히 작게 잡아 `event`/`request_id`/`dir` 등 envelope 오버헤드 + serde_json
/// 이스케이프 팽창분을 흡수한다. 이 상한 없이 대형 디렉토리(수천 개 엔트리)를 그대로
/// 실으면 `write_frame` 이 `MAX_FRAME_LEN` 초과로 에러를 반환하고, 그 attach 세션의
/// write thread 전체가 종료돼(다른 forward/tap 도 동반) mirror 연결 자체가 끊긴다 —
/// 이 상한은 그 회귀를 막는 안전장치다.
const LIST_DIR_ENTRIES_BYTE_BUDGET: usize = 700 * 1024;

/// 엔트리를 wire JSON 으로 변환하되 [`LIST_DIR_ENTRIES_BYTE_BUDGET`] 을 넘기 전에
/// 멈춘다. 두 번째 반환값은 잘렸는지 여부 — client 는 이를 toast 로 사용자에게
/// 알린다(`attach_client.rs` `MirrorEvent::ListDirResult` 처리).
fn list_dir_entries_wire_capped(
    entries: &[crate::core::fs_list::DirEntryInfo],
) -> (Vec<serde_json::Value>, bool) {
    list_dir_entries_wire_capped_with_budget(entries, LIST_DIR_ENTRIES_BYTE_BUDGET)
}

/// 테스트 용이성을 위해 예산을 파라미터로 뺀 실제 구현.
fn list_dir_entries_wire_capped_with_budget(
    entries: &[crate::core::fs_list::DirEntryInfo],
    mut budget: usize,
) -> (Vec<serde_json::Value>, bool) {
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let wire = list_dir_entry_wire(e);
        let approx_len = serde_json::to_vec(&wire).map(|b| b.len()).unwrap_or(0);
        if approx_len > budget {
            return (out, true);
        }
        budget -= approx_len;
        out.push(wire);
    }
    (out, false)
}

/// `DirEntryInfo` 한 줄 → wire JSON. `modified: Option<SystemTime>` 은 JSON 에 그대로
/// 실을 수 없어 unix epoch 초(`modified_unix`)로 변환한다 — 사람이 읽는 포맷팅은
/// 클라이언트의 view 렌더 직전에서만 한다(포맷 변환을 wire 조립 지점과 분리).
fn list_dir_entry_wire(e: &crate::core::fs_list::DirEntryInfo) -> serde_json::Value {
    let modified_unix = e
        .modified
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    serde_json::json!({
        "name": e.name,
        "is_dir": e.is_dir,
        "size": e.size,
        "modified_unix": modified_unix,
        "ext": e.ext,
    })
}

/// git-viewer(ADR-0056) — mirror client 가 attach 채널로 보낸 `git_query_request`
/// 하나를 처리한다(status/log/worktrees snapshot, 또는 단일 파일 diff).
/// `client_id` 가 이 engine 이 호스팅하는 어떤 workspace 든 점유(holder)해야
/// 신뢰한다(`handle_list_dir_request`/`finalize_capture_upload` 와 동일한 "attach
/// 점유 = 권한" 원칙). `worktree_path` 가 없으면 `surface_id` 의 **실제 원격
/// PTY**(`Terminal::get_cwd` — OSC 7 캐시 우선, 없으면 `/proc`(Linux)·`proc_pidinfo`
/// (macOS) 폴백)로 cwd 를 직접 resolve 한다 — mirror 클라이언트의 OSC 7 재생에
/// 의존하지 않으므로, ADR-0056 Context 절에서 설명한 "원격 셸이 OSC 7 을 방출하지
/// 않는 경우"도 커버한다(client 가 forward 하는 `cwd` 문자열을 신뢰하는 대신 서버가
/// 직접 판정).
/// 조회는 `tasty-git-core`(plugin 과 공유하는 순수 로직 crate)로 수행하고,
/// `git_query_result` 이벤트로 회신한다(`StreamControl` enum 밖의 raw JSON "event"
/// 태그, list_dir/capture 와 동일 패턴).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_git_query_request(
    engine: &mut CoreState,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    surface_id: u32,
    kind: tasty_ipc::stream_hub::GitQueryKind,
    worktree_path: Option<String>,
    diff_path: Option<String>,
) {
    use tasty_ipc::stream_hub::GitQueryKind;

    let is_holder = engine.attach.client_holds_workspace(client_id);
    let result: Result<serde_json::Value, String> = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        match kind {
            GitQueryKind::Snapshot => {
                git_query_snapshot(engine, surface_id, worktree_path.as_deref())
            }
            GitQueryKind::Diff => match diff_path.as_deref() {
                Some(p) if !p.trim().is_empty() => {
                    git_query_diff(engine, surface_id, worktree_path.as_deref(), p)
                }
                _ => Err("missing diff_path for kind=diff".to_string()),
            },
        }
    };
    let kind_wire = kind.as_wire_str();
    let payload = match result {
        Ok(mut data) => {
            let obj = data
                .as_object_mut()
                .expect("git query helpers return objects");
            obj.insert("event".to_string(), serde_json::json!("git_query_result"));
            obj.insert("request_id".to_string(), serde_json::json!(request_id));
            obj.insert("ok".to_string(), serde_json::json!(true));
            obj.insert("kind".to_string(), serde_json::json!(kind_wire));
            data
        }
        Err(reason) => serde_json::json!({
            "event": "git_query_result",
            "request_id": request_id,
            "ok": false,
            "kind": kind_wire,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // best-effort 회신 — client 끊김 시 무해.
}

/// `worktree_path` 가 있으면 그 경로를 그대로(이전 응답이 돌려준 opaque 서버 경로
/// echo), 없으면 `surface_id` 의 실제 원격 cwd 를 discover 시작점으로 쓴다.
fn resolve_git_query_target(
    engine: &CoreState,
    surface_id: u32,
    worktree_path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    if let Some(p) = worktree_path {
        if p.trim().is_empty() {
            return Err("empty worktree_path".to_string());
        }
        return Ok(std::path::PathBuf::from(p));
    }
    engine
        .terminals
        .get(surface_id)
        .and_then(|t| t.get_cwd())
        .ok_or_else(|| "remote surface has no known cwd".to_string())
}

/// git-viewer(원격) status/log/worktrees snapshot 한 건을 수집해 wire JSON(예산 캡
/// 적용, envelope 필드 제외)으로 반환한다.
fn git_query_snapshot(
    engine: &CoreState,
    surface_id: u32,
    worktree_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    let target = resolve_git_query_target(engine, surface_id, worktree_path)?;
    let repo = tasty_git_core::discover_repo(&target)
        .ok_or_else(|| "no git repository found".to_string())?;
    let current_wd = repo
        .workdir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo.path().to_path_buf());
    let worktrees =
        tasty_git_core::collect_worktrees(&repo, &current_wd).map_err(|e| e.to_string())?;
    // `is_current` 는 방금 discover 한 repo 자신을 가리키므로, 그 항목의 branch/oid 를
    // 그대로 활성 정보로 재사용한다(별도 head_info 호출/공개 불필요).
    let active = worktrees.iter().find(|w| w.is_current);
    let branch = active.and_then(|w| w.branch.clone());
    let oid = active.and_then(|w| w.oid.clone());

    let status = tasty_git_core::collect_status(&repo).map_err(|e| e.to_string())?;
    let log = tasty_git_core::collect_log(&repo, GIT_QUERY_LOG_LIMIT).map_err(|e| e.to_string())?;

    let worktrees_wire: Vec<serde_json::Value> =
        worktrees.iter().map(worktree_entry_wire).collect();
    let mut budget = GIT_QUERY_BYTE_BUDGET.saturating_sub(wire_values_len(&worktrees_wire));
    let (status_wire, truncated_status) =
        cap_wire_values(budget, status.iter().map(status_entry_wire).collect());
    budget = budget.saturating_sub(wire_values_len(&status_wire));
    let (log_wire, truncated_log) =
        cap_wire_values(budget, log.iter().map(log_entry_wire).collect());

    Ok(serde_json::json!({
        "active_worktree_path": display_path_lossy(&current_wd),
        "branch": branch,
        "oid": oid,
        "worktrees": worktrees_wire,
        "status_entries": status_wire,
        "truncated_status": truncated_status,
        "log_entries": log_wire,
        "truncated_log": truncated_log,
    }))
}

/// git-viewer(원격) 단일 파일 diff 한 건을 수집해 wire JSON(예산 캡 적용, envelope
/// 필드 제외)으로 반환한다.
fn git_query_diff(
    engine: &CoreState,
    surface_id: u32,
    worktree_path: Option<&str>,
    diff_path: &str,
) -> Result<serde_json::Value, String> {
    let target = resolve_git_query_target(engine, surface_id, worktree_path)?;
    let repo = tasty_git_core::discover_repo(&target)
        .ok_or_else(|| "no git repository found".to_string())?;
    let diff = tasty_git_core::collect_diff(&repo, diff_path).map_err(|e| e.to_string())?;
    let (hunks_wire, truncated) = cap_diff_hunks(GIT_QUERY_BYTE_BUDGET, &diff.hunks);
    Ok(serde_json::json!({
        "file_path": diff.file_path,
        "hunks": hunks_wire,
        "truncated_diff": truncated,
    }))
}

fn display_path_lossy(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn worktree_entry_wire(w: &tasty_git_core::WorktreeEntry) -> serde_json::Value {
    serde_json::json!({
        "name": w.name,
        "path": display_path_lossy(&w.path),
        "branch": w.branch,
        "oid": w.oid,
        "is_main": w.is_main,
        "is_current": w.is_current,
        "locked": w.locked,
        "lock_reason": w.lock_reason,
        "is_valid": w.is_valid,
    })
}

fn file_status_wire(s: tasty_git_core::FileStatus) -> &'static str {
    use tasty_git_core::FileStatus;
    match s {
        FileStatus::Modified => "modified",
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Renamed => "renamed",
        FileStatus::Untracked => "untracked",
        FileStatus::Conflicted => "conflicted",
    }
}

fn status_entry_wire(e: &tasty_git_core::StatusEntry) -> serde_json::Value {
    serde_json::json!({ "status": file_status_wire(e.status), "path": e.path })
}

fn log_entry_wire(e: &tasty_git_core::LogEntry) -> serde_json::Value {
    serde_json::json!({
        "oid_short": e.oid_short,
        "summary": e.summary,
        "author": e.author,
        "time": e.time,
        "refs": e.refs,
    })
}

fn diff_line_wire(l: &tasty_git_core::DiffLine) -> serde_json::Value {
    use tasty_git_core::DiffLineKind;
    let kind = match l.kind {
        DiffLineKind::Context => "context",
        DiffLineKind::Addition => "addition",
        DiffLineKind::Deletion => "deletion",
    };
    serde_json::json!({
        "kind": kind,
        "content": l.content,
        "old_lineno": l.old_lineno,
        "new_lineno": l.new_lineno,
    })
}

fn diff_hunk_wire(h: &tasty_git_core::DiffHunk) -> serde_json::Value {
    serde_json::json!({
        "header": h.header,
        "lines": h.lines.iter().map(diff_line_wire).collect::<Vec<_>>(),
    })
}

/// [`LIST_DIR_ENTRIES_BYTE_BUDGET`] 과 동일 근거 — attach 프레임 하드 상한
/// (`MAX_FRAME_LEN`, 1MiB)보다 충분히 작게 잡아 envelope 오버헤드를 흡수한다. 대형
/// 저장소(수천 개 status entry, 긴 log, 큰 diff)를 이 상한 없이 그대로 실으면
/// write thread 가 죽어 mirror 연결 자체가 끊긴다.
const GIT_QUERY_BYTE_BUDGET: usize = 700 * 1024;

/// `collect_log` 조회 상한 — plugin 로컬 `LOG_LIMIT`(main.rs)과 동일 값.
const GIT_QUERY_LOG_LIMIT: usize = 200;

fn wire_values_len(items: &[serde_json::Value]) -> usize {
    items
        .iter()
        .map(|v| serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0))
        .sum()
}

/// 이미 조립된 wire value 목록을 예산 안에서 자른다(status/log 공용). 두 번째
/// 반환값은 잘렸는지 여부.
fn cap_wire_values(
    mut budget: usize,
    items: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, bool) {
    let mut out = Vec::with_capacity(items.len());
    for v in items {
        let len = serde_json::to_vec(&v).map(|b| b.len()).unwrap_or(0);
        if len > budget {
            return (out, true);
        }
        budget -= len;
        out.push(v);
    }
    (out, false)
}

/// diff hunk 단위로 예산 캡(hunk 내부 line 단위 부분 절단은 하지 않음 — 한 hunk 를
/// 통째로 포함하거나 제외).
fn cap_diff_hunks(
    mut budget: usize,
    hunks: &[tasty_git_core::DiffHunk],
) -> (Vec<serde_json::Value>, bool) {
    let mut out = Vec::with_capacity(hunks.len());
    for h in hunks {
        let wire = diff_hunk_wire(h);
        let len = serde_json::to_vec(&wire).map(|b| b.len()).unwrap_or(0);
        if len > budget {
            return (out, true);
        }
        budget -= len;
        out.push(wire);
    }
    (out, false)
}

/// markdown mirror(ADR-0255) — mirror client 가 attach 채널로 보낸
/// `markdown_content_request` 하나를 처리한다. `client_id` 가 이 engine 이 호스팅하는
/// 어떤 workspace 든 점유(holder)해야 신뢰한다(`handle_list_dir_request`/
/// `handle_git_query_request` 와 동일한 "attach 점유 = 권한" 원칙 — 이 채널이 나르는
/// 것은 SSH 로 붙은 사용자가 이미 읽을 수 있는 그 호스트의 파일 원문이다).
///
/// **파일은 이 host 가 직접 읽는다** — 서버측 markdown plugin 에 되묻지 않는다.
/// 이 핸들러는 동기 경로이고 plugin 왕복은 비동기라 여기서 기다릴 수 없다
/// (`handle_git_query_request` 가 `tasty-git-core` 를 host 에서 직접 부르는 것과 같은
/// 형태). 그 결과로, 서버측 plugin 이 대용량 확인 대기 중이라 서버 화면에는 아무것도
/// 안 띄운 파일이라도 여기서는 예산 안에서 읽어 보낸다 — 그 게이트는 그리는 쪽의
/// 물음이지 읽기 권한의 경계가 아니다(ADR-0255 항목 4).
///
/// 회신은 `markdown_content_result` 이벤트(`StreamControl` enum 밖의 raw JSON "event"
/// 태그, list_dir/git_query 와 동일 패턴).
pub(crate) fn handle_markdown_content_request(
    engine: &mut CoreState,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    surface_id: u32,
) {
    let is_holder = engine.attach.client_holds_workspace(client_id);
    let result = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        markdown_content_for_request(engine, surface_id)
    };
    let payload = match result {
        Ok((file, source, truncated)) => serde_json::json!({
            "event": "markdown_content_result",
            "request_id": request_id,
            "surface_id": surface_id,
            "ok": true,
            "file": file,
            "source": source,
            "truncated": truncated,
        }),
        Err(reason) => serde_json::json!({
            "event": "markdown_content_result",
            "request_id": request_id,
            "surface_id": surface_id,
            "ok": false,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // best-effort 회신 — client 끊김 시 무해.
}

/// markdown 원문 채널의 변경 신호(ADR-0255 항목 5) — 서버가 알리기만 하고 client 는 다시
/// 받지 않는다.
#[cfg(feature = "gui")]
const MARKDOWN_CHANGED_EVENT: &str = "markdown_changed";

/// 문서가 다시 그려졌다는 사실을 attach client 들에 알린다. 호출처는 `webview.set_url` 핸들러
/// 하나다 — markdown plugin 의 재렌더(파일 감시·`markdown.reload`·테마 변경·최초 생성)는 전부
/// 그 IPC 로 host 에 도달하므로 host 가 plugin 을 고치지 않고 신호원을 갖는다. 그래서 신호는
/// 실제 파일 변경의 **상위 집합**이고, client 쪽 대가는 "눌러 보니 같은 내용" 뿐이다.
///
/// 수신자는 [`crate::core::attach::OccupancyRegistry::workspace_holders`] — 원문 요청을
/// 인가하는 집합과 같다. 점유가 없거나 notifier 가 주입되지 않았으면 아무것도 안 한다(다음
/// attach 의 핸드셰이크가 최신 상태를 싣고 오므로 쌓아 둘 이유가 없다). 화이트리스트 밖
/// kind(html 등)는 원문 채널을 안 타므로 신호도 없다. 반환은 실제로 큐에 실린 client 수.
///
/// `webview.set_url` 이 gui 전용이라 이 함수도 gui 전용이다 — 헤드리스 서버에서는 markdown
/// plugin 의 재렌더가 host 에 닿지 않아 신호원이 없다.
#[cfg(feature = "gui")]
pub(crate) fn notify_markdown_changed(
    attach: &crate::core::attach::OccupancyRegistry,
    kind: &str,
    plugin_id: &str,
    surface_id: SurfaceId,
) -> usize {
    if !is_attach_content_allowed(kind, plugin_id) {
        return 0;
    }
    let Some(hub) = attach.notifier() else {
        return 0;
    };
    let holders = attach.workspace_holders();
    if holders.is_empty() {
        return 0;
    }
    let payload = serde_json::json!({
        "event": MARKDOWN_CHANGED_EVENT,
        "surface_id": surface_id,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let mut sent = 0;
    for client_id in holders {
        match hub.push(
            client_id,
            StreamFrame::new(StreamTag::Control, bytes.clone()),
        ) {
            PushResult::Sent => sent += 1,
            // 신호는 색만 바꾼다 — 놓치면 다음 신호나 다음 attach 가 채운다.
            other => tracing::debug!(
                "attach: markdown_changed surface={surface_id} client={client_id} not queued: {other:?}"
            ),
        }
    }
    sent
}

/// `surface_id`(원격 id)를 content mirror 대상으로 확인하고 그 파일의 원문을 예산 안에서
/// 읽는다. 반환은 `(file, source, truncated)`.
///
/// **파일 없이 열린 markdown surface 는 에러가 아니다** — 서버에서도 빈 문서가 보이므로
/// 그 상태의 충실한 mirror 는 빈 `file`/`source` 다. `ok:false` 로 답하면 mirror 가
/// 서버에 없는 에러를 만들어낸다(ADR-0255 항목 2).
fn markdown_content_for_request(
    engine: &CoreState,
    surface_id: u32,
) -> Result<(String, String, bool), String> {
    let surface = engine
        .find_surface_by_id(surface_id)
        .ok_or_else(|| "unknown surface".to_string())?;
    let (kind, plugin_id, file) = surface
        .attach_content_info()
        .ok_or_else(|| "surface does not carry content".to_string())?;
    if !is_attach_content_allowed(kind, plugin_id) {
        return Err(format!("surface kind '{kind}' is not content-mirrored"));
    }
    let Some(path) = file else {
        return Ok((String::new(), String::new(), false));
    };
    // 에러 구분 수준은 `list_dir_for_request` 와 같다 — PermissionDenied 만 갈라 적고
    // 나머지는 io 에러 문자열 그대로(없는 파일이면 그 사유가 그대로 나간다).
    let bytes = std::fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission denied".to_string()
        } else {
            e.to_string()
        }
    })?;
    let (source, truncated) = markdown_source_wire_capped(&bytes);
    Ok((path.to_string_lossy().to_string(), source, truncated))
}

/// `markdown_content_result` 프레임 하나의 `source` 에 허용하는 **직렬화** 바이트 예산.
/// [`LIST_DIR_ENTRIES_BYTE_BUDGET`]·[`GIT_QUERY_BYTE_BUDGET`] 과 **같은 근거이자 같은
/// 재는 대상** — attach 프레임 하드 상한(`tasty_ipc::stream::MAX_FRAME_LEN`, 1MiB)보다
/// 충분히 작게 잡아 envelope 오버헤드 + serde_json 이스케이프 팽창분을 흡수한다. 없으면
/// 큰 문서 하나가 `write_frame` 을 상한 초과로 실패시키고 그 세션의 write thread 가
/// 통째로 죽어 mirror 연결 자체가 끊긴다.
///
/// **원문 바이트가 아니라 JSON 문자열이 된 뒤의 바이트로 잰다.** 이스케이프는 최대
/// 6 배(제어문자 → `\u00XX`)까지 부푼다 — 원문을 그대로 재면 이스케이프가 많은 문서가
/// 예산을 통과한 뒤 프레임 상한을 넘어, 막으려던 바로 그 연결 끊김을 낸다. 두 형제
/// 예산이 `serde_json::to_vec` 으로 재는 것도 같은 이유다.
///
/// markdown plugin 의 대용량 게이트(`LARGE_FILE_LIMIT_BYTES`, 1MiB — plugin in-process
/// 사용자 확인)와는 **다른 층**이다. 값이 그보다 작은 것은 우연이 아니다: 그 게이트를
/// 건드릴 만큼 큰 파일은 직렬화하면 더 커지므로 이 예산에도 반드시 걸려 항상
/// `truncated: true` 로 도착한다.
const MARKDOWN_CONTENT_BYTE_BUDGET: usize = 700 * 1024;

/// 원문을 [`MARKDOWN_CONTENT_BYTE_BUDGET`] 안으로 자른다. 두 번째 반환값은 잘렸는지 여부.
fn markdown_source_wire_capped(bytes: &[u8]) -> (String, bool) {
    markdown_source_wire_capped_with_budget(bytes, MARKDOWN_CONTENT_BYTE_BUDGET)
}

/// serde_json 이 이 `char` 를 JSON 문자열 안에 실을 때 차지하는 바이트 수. 두 자리
/// 이스케이프(`\"` `\\` `\n` `\r` `\t` `\b` `\f`), 그 밖의 제어문자는 `\u00XX` 6 바이트,
/// 나머지는 UTF-8 그대로다. 이 표가 serde_json 과 어긋나면 예산이 조용히 빗나가므로
/// `escaped_char_len_matches_serde_json` 이 전수 대조한다.
fn json_escaped_char_len(ch: char) -> usize {
    match ch {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

/// 테스트 용이성을 위해 예산을 파라미터로 뺀 실제 구현.
///
/// 예산은 **JSON 문자열로 직렬화된 뒤의 길이**(감싸는 따옴표 두 개 포함)로 잰다 — 상한을
/// 지켜야 하는 것은 프레임이고, 프레임에 실리는 것은 이스케이프된 형태이기 때문이다.
/// 그래서 앞에서부터 `char` 단위로 이스케이프 비용을 누적하며 예산에 닿는 자리에서 멈춘다.
///
/// `char` 단위로 걷는 덕에 자르는 자리는 항상 **UTF-8 문자 경계**다 — 이어지는 바이트
/// 한가운데서 자르면 그 문자가 통째로 U+FFFD 하나로 바뀌어, 잘린 자리에 있지도 않던
/// 글자가 생긴다. 경계에서 멈추면 그 문자는 아예 안 실린다(있던 글자가 빠지는 쪽이 없던
/// 글자가 생기는 쪽보다 낫고, 어느 쪽이든 `truncated` 가 알린다).
fn markdown_source_wire_capped_with_budget(bytes: &[u8], budget: usize) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    // 감싸는 따옴표 두 개도 프레임에 실린다.
    let mut used: usize = 2;
    for (i, ch) in text.char_indices() {
        let cost = json_escaped_char_len(ch);
        if used + cost > budget {
            return (text[..i].to_string(), true);
        }
        used += cost;
    }
    (text.into_owned(), false)
}

#[cfg(test)]
mod markdown_content_tests {
    use super::{json_escaped_char_len, markdown_source_wire_capped_with_budget};

    /// 실제로 프레임에 실리는 길이 — 이 모듈의 기대값은 전부 이 함수로 잰다.
    fn serialized_len(s: &str) -> usize {
        serde_json::to_vec(&serde_json::Value::String(s.to_string()))
            .expect("string always serializes")
            .len()
    }

    #[test]
    fn a_document_within_budget_is_not_truncated() {
        let (source, truncated) = markdown_source_wire_capped_with_budget(b"# hi\n", 64);
        assert_eq!(source, "# hi\n");
        assert!(!truncated);
    }

    /// 예산이 다국어 문자 한가운데 떨어져도 U+FFFD 를 만들지 않는다 — 그 문자를
    /// 통째로 뺀다.
    #[test]
    fn truncation_backs_up_to_a_char_boundary() {
        // "가" 는 3 바이트(EA B0 80). 따옴표 2 + 3 = 5 라 예산 7 은 한 글자만 담는다.
        let bytes = "가나".as_bytes();
        assert_eq!(bytes.len(), 6);
        let (source, truncated) = markdown_source_wire_capped_with_budget(bytes, 7);
        assert_eq!(source, "가", "잘린 자리에 U+FFFD 가 생기면 안 된다");
        assert!(truncated);
    }

    #[test]
    fn truncation_at_an_exact_boundary_keeps_everything_before_it() {
        let bytes = "가나".as_bytes();
        // 5 = 따옴표 2 + "가" 3. 여기서 딱 떨어지고 "나" 는 안 들어간다.
        let (source, truncated) = markdown_source_wire_capped_with_budget(bytes, 5);
        assert_eq!(source, "가");
        assert!(truncated);
        assert_eq!(serialized_len(&source), 5);
    }

    /// ★ 예산은 **원문**이 아니라 **직렬화된** 바이트로 잰다. 원문으로 재면 이 문서는
    /// 통과한 뒤 프레임에서 2 배로 부풀어, 예산이 막으려던 상한 초과가 그대로 난다.
    #[test]
    fn budget_counts_escape_expansion_not_raw_bytes() {
        // 따옴표는 `\"` 로 2 배가 된다. 원문 400 바이트, 직렬화 802 바이트.
        let raw = "\"".repeat(400);
        assert_eq!(serialized_len(&raw), 802);

        let (source, truncated) = markdown_source_wire_capped_with_budget(raw.as_bytes(), 500);
        assert!(
            truncated,
            "원문(400) 은 예산(500) 안이지만 직렬화(802) 는 넘는다 — 잘려야 한다"
        );
        assert_eq!(source.len(), 249, "따옴표 249 개 = 2 + 249*2 = 500");
        assert_eq!(serialized_len(&source), 500);
    }

    /// 제어문자는 `\u00XX` 로 6 배가 된다 — 최악 팽창률도 예산 안에 들어와야 한다.
    #[test]
    fn control_characters_are_counted_at_their_six_byte_cost() {
        let raw = "\u{01}".repeat(100);
        let (source, truncated) = markdown_source_wire_capped_with_budget(raw.as_bytes(), 200);
        assert!(truncated);
        assert_eq!(source.chars().count(), 33, "2 + 33*6 = 200");
        assert_eq!(serialized_len(&source), 200);
    }

    /// 이스케이프 비용표가 serde_json 과 어긋나면 예산이 조용히 빗나간다 — BMP 전수 대조.
    #[test]
    fn escaped_char_len_matches_serde_json() {
        for cp in 0u32..=0xFFFF {
            let Some(ch) = char::from_u32(cp) else {
                continue; // surrogate
            };
            let s = ch.to_string();
            // 감싸는 따옴표 두 개를 뺀 나머지가 그 char 의 비용이다.
            let expected = serialized_len(&s) - 2;
            assert_eq!(
                json_escaped_char_len(ch),
                expected,
                "U+{cp:04X} 의 이스케이프 길이가 serde_json 과 다르다"
            );
        }
    }

    /// 비-UTF8 바이트는 lossy 로 실린다(`fs_list` 의 경로 처리와 같은 수준) —
    /// 읽기 자체가 실패로 바뀌지는 않는다.
    #[test]
    fn invalid_utf8_is_carried_lossily_without_failing() {
        let (source, truncated) = markdown_source_wire_capped_with_budget(&[0xff, 0xfe], 64);
        assert!(!source.is_empty());
        assert!(!truncated);
    }
}

#[cfg(all(test, feature = "gui"))]
mod markdown_changed_tests {
    use super::notify_markdown_changed;
    use crate::core::attach::OccupancyRegistry;
    use tasty_ipc::stream::StreamTag;
    use tasty_ipc::stream_hub::StreamHub;

    fn changed_surface_id(frame: &tasty_ipc::stream::StreamFrame) -> Option<u64> {
        assert_eq!(frame.tag, StreamTag::Control);
        let v: serde_json::Value = serde_json::from_slice(&frame.payload).ok()?;
        (v.get("event")?.as_str()? == "markdown_changed").then_some(())?;
        v.get("surface_id")?.as_u64()
    }

    /// 수신자는 그 surface 를 담은 워크스페이스의 holder 가 아니라 **아무** 워크스페이스든
    /// 점유한 client 전부다 — 원문 요청의 인가 집합과 같아야 한다.
    #[test]
    fn every_workspace_holder_receives_the_signal_once() {
        let hub = StreamHub::new();
        let a = hub.alloc_id();
        let b = hub.alloc_id();
        let bystander = hub.alloc_id();
        let rx_a = hub.register(a);
        let rx_b = hub.register(b);
        let rx_bystander = hub.register(bystander);
        let mut reg = OccupancyRegistry::new();
        reg.set_notifier(hub);
        reg.acquire_workspace(100, &[10], &[10, 11], a).unwrap();
        reg.acquire_workspace(200, &[20], &[20], b).unwrap();
        reg.acquire_workspace(300, &[30], &[30], a).unwrap();

        assert_eq!(
            notify_markdown_changed(&reg, "markdown", "com.tasty.markdown", 11),
            2
        );
        assert_eq!(changed_surface_id(&rx_a.try_recv().unwrap()), Some(11));
        assert!(
            rx_a.try_recv().is_err(),
            "두 워크스페이스를 점유해도 신호는 한 번"
        );
        assert_eq!(changed_surface_id(&rx_b.try_recv().unwrap()), Some(11));
        assert!(
            rx_bystander.try_recv().is_err(),
            "점유하지 않은 client 에는 가지 않는다"
        );
    }

    #[test]
    fn no_holder_or_non_content_kind_is_a_no_op() {
        let hub = StreamHub::new();
        let a = hub.alloc_id();
        let rx = hub.register(a);
        let mut reg = OccupancyRegistry::new();
        reg.set_notifier(hub);
        assert_eq!(
            notify_markdown_changed(&reg, "markdown", "com.tasty.markdown", 11),
            0
        );
        assert!(rx.try_recv().is_err());

        reg.acquire_workspace(100, &[10], &[10, 11], a).unwrap();
        assert_eq!(
            notify_markdown_changed(&reg, "html", "com.tasty.html", 11),
            0
        );
        assert_eq!(
            notify_markdown_changed(&reg, "markdown", "com.thirdparty.markdown", 11),
            0
        );
        assert!(rx.try_recv().is_err());
    }
}

#[cfg(test)]
mod git_query_tests {
    use super::*;

    #[test]
    fn cap_wire_values_stops_before_exceeding_budget() {
        let items: Vec<serde_json::Value> = (0..20)
            .map(|i| serde_json::json!({ "path": format!("file_{i:03}") }))
            .collect();
        let one_len = serde_json::to_vec(&items[0]).unwrap().len();
        let budget = one_len * 3;
        let (out, truncated) = cap_wire_values(budget, items);
        assert_eq!(out.len(), 3, "expected exactly 3 entries to fit: {out:?}");
        assert!(truncated);
    }

    #[test]
    fn cap_wire_values_empty_input_never_truncated() {
        let (out, truncated) = cap_wire_values(0, Vec::new());
        assert!(out.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn worktree_entry_wire_roundtrips_fields() {
        let w = tasty_git_core::WorktreeEntry {
            name: "main".to_string(),
            path: std::path::PathBuf::from("/repo"),
            branch: Some("main".to_string()),
            oid: Some("abc1234".to_string()),
            is_main: true,
            is_current: true,
            locked: false,
            lock_reason: None,
            is_valid: true,
        };
        let wire = worktree_entry_wire(&w);
        assert_eq!(wire["name"], "main");
        assert_eq!(wire["path"], "/repo");
        assert_eq!(wire["is_current"], true);
    }

    fn test_engine() -> crate::core::CoreState {
        let term_waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::core::CoreState::new(80, 24, term_waker).expect("engine")
    }

    #[test]
    fn resolve_git_query_target_prefers_worktree_path_over_surface_cwd() {
        let engine = test_engine();
        let resolved = resolve_git_query_target(&engine, 999, Some("/explicit/path")).unwrap();
        assert_eq!(resolved, std::path::PathBuf::from("/explicit/path"));
    }

    #[test]
    fn resolve_git_query_target_errors_without_cwd_or_worktree_path() {
        let engine = test_engine();
        let err = resolve_git_query_target(&engine, 999, None).unwrap_err();
        assert!(err.contains("no known cwd"));
    }
}

#[cfg(test)]
mod list_dir_entries_wire_capped_tests {
    use super::list_dir_entries_wire_capped_with_budget;
    use crate::core::fs_list::DirEntryInfo;

    fn entry(name: &str) -> DirEntryInfo {
        DirEntryInfo {
            #[cfg(feature = "gui")]
            path: name.into(),
            name: name.to_string(),
            is_dir: false,
            size: 0,
            modified: None,
            ext: String::new(),
        }
    }

    #[test]
    fn fits_within_budget_untruncated() {
        let entries: Vec<_> = (0..10).map(|i| entry(&format!("f{i}"))).collect();
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&entries, 10_000);
        assert_eq!(wire.len(), 10);
        assert!(!truncated);
    }

    #[test]
    fn stops_before_exceeding_budget() {
        // 각 엔트리의 직렬화 크기를 먼저 재서, 정확히 3개만 들어가는 예산을 계산한다
        // (하드코드된 바이트 수에 의존하면 wire 포맷이 바뀔 때 이 test 가 깨진다).
        let entries: Vec<_> = (0..20).map(|i| entry(&format!("file_{i:03}"))).collect();
        let one_entry_len = serde_json::to_vec(&super::list_dir_entry_wire(&entries[0]))
            .unwrap()
            .len();
        let budget = one_entry_len * 3;
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&entries, budget);
        assert_eq!(wire.len(), 3, "expected exactly 3 entries to fit: {wire:?}");
        assert!(truncated);
    }

    #[test]
    fn empty_input_is_never_truncated() {
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&[], 0);
        assert!(wire.is_empty());
        assert!(!truncated);
    }
}

/// attach 거부/실패 통지: `attach_error` Control + Detach 로 연결 종료 유도.
/// 어떤 engine 도 대상 surface 를 소유하지 않을 때 메인루프(gui)도 호출한다.
pub(crate) fn reject_attach(
    hub: &StreamHub,
    client_id: AttachClientId,
    reason: &str,
    holder: Option<AttachClientId>,
) {
    let msg = serde_json::json!({
        "event": "attach_error",
        "reason": reason,
        "holder": holder,
    });
    let error_frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&msg).unwrap_or_default(),
    );
    let _ = hub.push(client_id, error_frame); // best-effort attach_error 통지 — PushResult(Result 아님) 무시: client 끊겼으면 무해.
    let _ = hub.push(client_id, StreamFrame::new(StreamTag::Detach, Vec::new())); // best-effort detach 신호 — 무시.
}

/// `AttachSurfaceClass::mesh_candidates`(raw, `tasty-model` 이 화이트리스트를
/// 모른 채 `Surface::attach_mesh_info()` 만으로 모은 후보)를 bundled 화이트리스트
/// (`is_egui_mesh_allowed`)로 재검증한다. `tasty-model`은 `is_egui_mesh_allowed`(앱
/// 계층, `src/`)를 참조할 수 없어(crate 의존 방향) 이 최종 판정은 여기(앱 계층)의
/// 책임이다.
///
/// 반환: `(whitelisted, rejected)`. `rejected`는 `class.non_terminals`와 동일하게
/// placeholder로 취급해야 한다 — 호출자는 이 둘을 합쳐 최종 placeholder 목록을 얻는다.
pub(crate) fn mesh_mirror_candidates(
    class: &AttachSurfaceClass,
) -> (Vec<(SurfaceId, &str, &str)>, Vec<SurfaceId>) {
    let mut whitelisted = Vec::new();
    let mut rejected = Vec::new();
    for (sid, kind, plugin_id) in &class.mesh_candidates {
        if crate::core::surface_registry::egui_mesh::is_egui_mesh_allowed(kind, plugin_id) {
            whitelisted.push((*sid, kind.as_str(), plugin_id.as_str()));
        } else {
            rejected.push(*sid);
        }
    }
    (whitelisted, rejected)
}

/// content mirror(ADR-0255) 를 실제로 타는 `(kind, plugin_id)` 조합.
///
/// **markdown 하나로 좁힌다.** `Surface::attach_content_info()` 는 `RemoteSurface`
/// 전체에 붙으므로 이 게이트가 없으면 같은 webview kind 인 html surface 까지 새 role
/// 로 나가는데, html 의 URL 은 파일 경로라는 보장이 없어 "그 경로의 원문" 이라는 이
/// 채널의 의미가 성립하지 않는다(ADR-0255 항목 1).
pub(crate) fn is_attach_content_allowed(kind: &str, plugin_id: &str) -> bool {
    matches!((kind, plugin_id), ("markdown", "com.tasty.markdown"))
}

/// [`content_mirror_candidates`] 의 반환 — `(화이트리스트를 통과한 후보, placeholder 로
/// 떨어진 id)`. 앞쪽 원소는 `(surface_id, kind, file)`.
type ContentMirrorSplit<'a> = (
    Vec<(SurfaceId, &'a str, Option<&'a std::path::Path>)>,
    Vec<SurfaceId>,
);

/// `AttachSurfaceClass::content_candidates`(raw, `tasty-model` 이 화이트리스트를 모른
/// 채 `Surface::attach_content_info()` 만으로 모은 후보)를
/// [`is_attach_content_allowed`] 로 재검증한다. [`mesh_mirror_candidates`] 와 완전히
/// 동형이며, 같은 이유(crate 의존 방향)로 최종 판정이 여기(앱 계층)에 있다.
///
/// 반환: `(whitelisted, rejected)`. `rejected` 는 `class.non_terminals` 와 동일하게
/// placeholder 로 취급해야 한다.
pub(crate) fn content_mirror_candidates(class: &AttachSurfaceClass) -> ContentMirrorSplit<'_> {
    let mut whitelisted = Vec::new();
    let mut rejected = Vec::new();
    for (sid, kind, plugin_id, file) in &class.content_candidates {
        if is_attach_content_allowed(kind, plugin_id) {
            whitelisted.push((*sid, kind.as_str(), file.as_deref()));
        } else {
            rejected.push(*sid);
        }
    }
    (whitelisted, rejected)
}

#[cfg(test)]
mod content_mirror_candidate_tests {
    //! ADR-0255 화이트리스트가 markdown 만 통과시키는지. html(같은 webview kind)과
    //! 서드파티 markdown 사칭(`kind` 는 같고 `plugin_id` 가 다른 조합)은 rejected
    //! (= placeholder 유지)로 떨어져야 한다.
    use super::content_mirror_candidates;
    use crate::model::AttachSurfaceClass;
    use std::path::PathBuf;

    #[test]
    fn content_mirror_candidates_filters_by_whitelist() {
        let class = AttachSurfaceClass {
            content_candidates: vec![
                (
                    10,
                    "markdown".into(),
                    "com.tasty.markdown".into(),
                    Some(PathBuf::from("/proj/README.md")),
                ),
                (11, "html".into(), "com.tasty.html".into(), None),
                (
                    12,
                    "markdown".into(),
                    "com.thirdparty.markdown".into(),
                    Some(PathBuf::from("/x.md")),
                ),
            ],
            ..Default::default()
        };
        let (whitelisted, rejected) = content_mirror_candidates(&class);
        assert_eq!(
            whitelisted,
            vec![(
                10,
                "markdown",
                Some(std::path::Path::new("/proj/README.md"))
            )]
        );
        assert_eq!(rejected, vec![11, 12]);
    }

    /// 파일 없이 열린 markdown surface 도 후보다 — 빈 문서를 빈 문서로 mirror 한다.
    #[test]
    fn a_markdown_surface_without_a_file_is_still_a_candidate() {
        let class = AttachSurfaceClass {
            content_candidates: vec![(7, "markdown".into(), "com.tasty.markdown".into(), None)],
            ..Default::default()
        };
        let (whitelisted, rejected) = content_mirror_candidates(&class);
        assert_eq!(whitelisted, vec![(7, "markdown", None)]);
        assert!(rejected.is_empty());
    }
}

#[cfg(test)]
mod mesh_mirror_candidate_tests {
    //! bundled 화이트리스트에 있는 mesh 후보만
    //! whitelisted 로, 나머지(미등록 kind/plugin_id 조합)는 rejected(=placeholder 유지)로
    //! 분류되는지. markdown 은 Stage B(webview 전환)로 화이트리스트에서 빠졌으므로
    //! 여기선 image/mesh_demo 만 whitelisted 로 남고, markdown 은 3rd-party 미등록
    //! 조합과 동일하게 rejected 로 떨어져야 한다(회귀 방지).
    use super::mesh_mirror_candidates;
    use crate::model::AttachSurfaceClass;

    #[test]
    fn mesh_mirror_candidates_filters_by_whitelist() {
        let class = AttachSurfaceClass {
            terminals: vec![1],
            non_terminals: vec![2],
            explorers: vec![],
            mesh_candidates: vec![
                (10, "image".to_string(), "com.tasty.image".to_string()),
                (
                    11,
                    "mesh_demo".to_string(),
                    "com.tasty.mesh-demo".to_string(),
                ),
                // 화이트리스트 밖(가상의 3rd-party 조합) — rejected 로 떨어져야 한다.
                (12, "widget".to_string(), "com.example.widget".to_string()),
                // markdown 은 Stage B 이후 egui-mesh 화이트리스트 밖 — rejected.
                (13, "markdown".to_string(), "com.tasty.markdown".to_string()),
            ],
            content_candidates: vec![],
        };
        let (whitelisted, rejected) = mesh_mirror_candidates(&class);
        let mut whitelisted_ids: Vec<u32> = whitelisted.iter().map(|(sid, _, _)| *sid).collect();
        whitelisted_ids.sort_unstable();
        let mut rejected_ids = rejected;
        rejected_ids.sort_unstable();
        assert_eq!(whitelisted_ids, vec![10, 11]);
        assert_eq!(rejected_ids, vec![12, 13]);
    }
}

#[cfg(test)]
mod mesh_descriptor_display_name_tests {
    //! mesh 디스크립터(`build_workspace_tree_surfaces`)가 mesh 후보의 실제
    //! `Surface::display_name()`(예: image 파일명)을 `display_name` 필드로
    //! 실어보내는지. 이전엔 이 필드 자체가 없어 client 가 kind 문자열("image")로
    //! 대체 표시했다. markdown 은 Stage B(webview 전환)로 egui-mesh 화이트리스트에서
    //! 빠져 더 이상 이 mesh 디스크립터 경로를 타지 않으므로(ADR-0255 의 전용
    //! `role:"markdown"` 으로 분류) 여기 fixture 로 쓰지 않는다.
    use crate::core::egui_mesh_surface::EguiMeshSurface;

    fn engine_with_mesh_surface(
        kind: &'static str,
        plugin_id: &str,
        display_name: &str,
    ) -> (crate::core::CoreState, usize) {
        let term_waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine = crate::core::CoreState::new(80, 24, term_waker).unwrap();
        let surface: Box<dyn crate::model::Surface> = Box::new(EguiMeshSurface::new(
            100,
            kind,
            plugin_id.to_string(),
            display_name.to_string(),
            Some("/docs/README.md".to_string()),
        ));
        let pane = crate::model::Pane::new_with_surface(1, 1, display_name.to_string(), surface);
        let ws = crate::model::Workspace::new_with_pane(1, "ws".to_string(), pane);
        engine.workspaces.push(ws);
        let idx = engine.workspaces.len() - 1;
        (engine, idx)
    }

    #[test]
    fn image_mesh_descriptor_carries_real_display_name() {
        let (engine, idx) = engine_with_mesh_surface("image", "com.tasty.image", "screenshot.png");
        let class = engine.workspaces[idx].classify_attach_surfaces();
        let (_tree, surfaces) = engine.build_workspace_tree_surfaces(idx, &class);
        let mesh = surfaces
            .iter()
            .find(|s| s.get("role").and_then(|v| v.as_str()) == Some("mesh"))
            .expect("mesh surface descriptor 가 있어야 한다");
        assert_eq!(
            mesh.get("display_name").and_then(|v| v.as_str()),
            Some("screenshot.png"),
            "mesh 디스크립터의 display_name 은 실제 파일명이어야 한다 — kind 문자열(\"image\")로 대체되면 안 됨"
        );
    }

    /// mesh_demo 도 같은 `EguiMeshSurface`/화이트리스트 경로를 타므로 동일하게
    /// display_name 이 전달돼야 한다(image 전용 수정이 아님을 보장 — 회귀 방지).
    #[test]
    fn image_and_mesh_demo_mesh_descriptors_also_carry_display_name() {
        for (kind, plugin_id, name) in [
            ("image", "com.tasty.image", "screenshot.png"),
            ("mesh_demo", "com.tasty.mesh-demo", "Demo"),
        ] {
            let (engine, idx) = engine_with_mesh_surface(kind, plugin_id, name);
            let class = engine.workspaces[idx].classify_attach_surfaces();
            let (_tree, surfaces) = engine.build_workspace_tree_surfaces(idx, &class);
            let mesh = surfaces
                .iter()
                .find(|s| s.get("role").and_then(|v| v.as_str()) == Some("mesh"))
                .unwrap_or_else(|| panic!("{kind} mesh surface descriptor 가 있어야 한다"));
            assert_eq!(
                mesh.get("display_name").and_then(|v| v.as_str()),
                Some(name),
                "{kind} mesh 디스크립터도 display_name 을 실어야 한다"
            );
        }
    }
}

#[cfg(test)]
mod forward_exec_tests {
    //! forward 된 구조 op 실행(2단계). 서버(원격 authoritative)측 워크스페이스는
    //! mirror 가 아니므로 `execute_forwarded_structural_op` 이 IPC 핸들러와 같은 도메인 실행
    //! 함수(`core::structural_exec`)로 **실제로** split/new-tab 을 수행한다(로컬 PTY = 원격의 정당한 PTY). 원격에 없는
    //! kind 는 `Err(reason)` 으로 실패 회신된다.
    use super::execute_forwarded_structural_op;
    use crate::state::AppState;
    use tasty_ipc::stream::{ForwardOrigin, SplitAxis, StructuralOp};
    use tasty_terminal::Terminal;

    fn make_core_state() -> (
        crate::core::Core,
        AppState,
        crate::core::CoreState,
        tempfile::TempDir,
    ) {
        use std::sync::{Arc, Mutex};
        use tasty_memory::MemoryStorage;
        use tasty_themes::{ThemeStorage, ThemeStore};

        use crate::adapters::test::{
            fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
            mock_process::MockProcessSpawner, tmp_home::TmpHome,
        };
        use crate::core::builder::CoreBuilder;
        use crate::ports::notification_sound::NoopPlayer;

        let term_waker: tasty_terminal::Waker = Arc::new(|| {});
        let mut engine = crate::core::CoreState::new(80, 24, term_waker).unwrap();
        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());
        let state = AppState::new(&mut engine, preset_store.clone(), memory.clone());
        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(TmpHome::new(home_tmp.path().to_path_buf())))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core build");
        (core, state, engine, home_tmp)
    }

    /// 기본 워크스페이스 0 의 단일 surface 에 detached 터미널을 붙이고 그 surface_id 반환.
    fn seed(engine: &mut crate::core::CoreState) -> u32 {
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine.terminals.insert(a, Terminal::new_detached(80, 24));
        a
    }

    /// 성공한 op 의 delta 에서 surfaces 배열의 remote_id 집합을 뽑는다(테스트 헬퍼).
    fn delta_surface_ids(fd: &super::ForwardedDelta) -> std::collections::HashSet<u32> {
        let tasty_ipc::stream::StreamControl::StructuralDelta { surfaces, .. } = &fd.delta else {
            panic!("expected StructuralDelta");
        };
        surfaces
            .iter()
            .filter_map(|s| {
                s.get("remote_id")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
            })
            .collect()
    }

    /// forward 된 SplitSurface 는 (비-mirror) 서버 워크스페이스에서 실제로 실행되어
    /// 새 터미널이 insert 된다(=원격이 새 PTY spawn). Ok + delta(신규 surface 포함).
    #[test]
    fn forward_split_surface_executes_and_spawns() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let before = engine.terminals.iter().count();
        let op = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        let fd = r
            .expect("expected Ok")
            .expect("split 성공은 delta 를 동반해야 한다");
        assert_eq!(
            engine.terminals.iter().count(),
            before + 1,
            "forward split 은 서버에서 새 터미널을 spawn 해야 한다"
        );
        // added_terminals 는 신규 surface 하나, delta.surfaces 에는 anchor+신규 모두 포함.
        assert_eq!(fd.added_terminals.len(), 1, "added 는 신규 터미널 1개");
        let ids = delta_surface_ids(&fd);
        assert!(ids.contains(&a), "delta 에 anchor surface 가 있어야 한다");
        assert!(
            ids.contains(&fd.added_terminals[0]),
            "delta.surfaces 에 신규 surface 가 있어야 한다"
        );
    }

    /// 사용자(`ForwardOrigin::User`)가 forward 한 `CloseSurface` 는 서버의 복원 스택에 항목을
    /// 남긴다.
    ///
    /// forward 된 close 의 `save_snapshot` 은 op 의 origin 이 정한다(User=true · Agent=false,
    /// ADR-0480) — 이 시험은 User 쪽이다. 이 앞단이 없으면 복원 forward 를 아무리 정확히 붙여도 원격에
    /// 꺼낼 항목이 존재하지 않는다.
    #[test]
    fn a_forwarded_close_surface_leaves_a_restorable_snapshot() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        // 같은 탭에 surface 를 하나 더 만든다 — 워크스페이스의 마지막 surface 를 닫는
        // 것은 workspace 통째 cascade 라 갈래가 다르다.
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::SplitSurface {
                surface_id: a,
                direction: SplitAxis::Horizontal,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];

        let before = engine.closed_items.len();
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::CloseSurface { surface_id: b },
            ForwardOrigin::User,
        )
        .expect("forwarded close must succeed");
        assert_eq!(
            engine.closed_items.len(),
            before + 1,
            "forward 된 surface close 는 서버 복원 스택에 정확히 하나 남겨야 한다"
        );
    }

    /// forward 된 `CloseTab` 도 같다. `apply_close_tab` 에는 save_snapshot 축이 없어
    /// `execute_forwarded_structural_op` 이 핸들러 호출 **전** 직접 캡처한다.
    #[test]
    fn a_forwarded_close_tab_leaves_a_restorable_snapshot() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::NewTab {
                anchor_surface_id: a,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("new tab ok")
        .expect("new tab delta");
        let b = fd.added_terminals[0];

        let before = engine.closed_items.len();
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::CloseTab {
                anchor_surface_id: b,
            },
            ForwardOrigin::User,
        )
        .expect("forwarded close must succeed");
        assert_eq!(
            engine.closed_items.len(),
            before + 1,
            "forward 된 tab close 는 서버 복원 스택에 정확히 하나 남겨야 한다"
        );
        assert!(
            matches!(
                engine.closed_items.list().next(),
                Some(crate::model::ClosedItem::Tab(_))
            ),
            "탭 단위로 캡처돼야 한다"
        );
    }

    /// `(탭 수, 활성 탭)` — 선택이 바뀌었는지 보는 좌변.
    fn tabs_and_selection(engine: &crate::core::CoreState, surface_id: u32) -> (usize, usize) {
        let pane_id = engine.find_pane_for_surface(surface_id).expect("pane");
        let pane = engine.find_pane_by_id(pane_id).expect("pane");
        (pane.tabs.len(), pane.active_tab)
    }

    fn forward_empty_new_tab(
        core: &mut crate::core::Core,
        state: &mut AppState,
        engine: &mut crate::core::CoreState,
        anchor: u32,
        origin: ForwardOrigin,
    ) {
        execute_forwarded_structural_op(
            core,
            state,
            engine,
            &StructuralOp::NewTab {
                anchor_surface_id: anchor,
                surface_kind: "empty".to_string(),
                params: serde_json::json!({}),
            },
            origin,
        )
        .expect("forwarded new tab must succeed");
    }

    /// 원격 **에이전트**가 연 비터미널 탭은 서버 앞 사용자의 선택을 안 바꾼다(ADR-0502).
    #[test]
    fn a_forwarded_agent_new_tab_keeps_the_selected_tab() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        forward_empty_new_tab(&mut core, &mut state, &mut engine, a, ForwardOrigin::Agent);
        assert_eq!(tabs_and_selection(&engine, a), (2, 0));
    }

    /// 원격 **사용자**가 손으로 연 비터미널 탭은 종전대로 선택된다.
    #[test]
    fn a_forwarded_user_new_tab_selects_it() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        forward_empty_new_tab(&mut core, &mut state, &mut engine, a, ForwardOrigin::User);
        assert_eq!(tabs_and_selection(&engine, a), (2, 1));
    }

    /// IPC `tab.create`(CLI `tasty new tab --type …`)는 에이전트 경로다 — 비터미널 탭을
    /// 만들어도 사용자가 보던 탭이 그대로다. 응답의 `active_tab` 도 그 선택을 그대로 싣는다.
    #[test]
    fn ipc_tab_create_of_a_non_terminal_keeps_the_selected_tab() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let pane_id = engine.find_pane_for_surface(a).expect("pane");
        let req = ipc_request(
            "tab.create",
            serde_json::json!({ "pane_id": pane_id, "type": "empty" }),
        );
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );
        let result = resp.result.expect("tab.create must succeed");
        assert_eq!(result["tab_count"], 2);
        assert_eq!(result["active_tab"], 0);
        assert_eq!(tabs_and_selection(&engine, a), (2, 0));
    }

    /// forward 된 `ClosePane` 은 **트리 재배치 전** 캡처라야 split context 가 남는다 —
    /// 제거 후엔 부모 Split 노드 자체가 사라져 복구할 수 없다.
    #[test]
    fn a_forwarded_close_pane_captures_the_split_context() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::SplitPane {
                anchor_surface_id: a,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("split pane ok")
        .expect("split pane delta");
        let b = fd.added_terminals[0];

        let before = engine.closed_items.len();
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::ClosePane {
                anchor_surface_id: b,
            },
            ForwardOrigin::User,
        )
        .expect("forwarded close must succeed");
        assert_eq!(
            engine.closed_items.len(),
            before + 1,
            "forward 된 pane close 는 서버 복원 스택에 정확히 하나 남겨야 한다"
        );
        assert!(
            matches!(
                engine.closed_items.list().next(),
                Some(crate::model::ClosedItem::Pane { .. })
            ),
            "pane 단위로(split context 를 실어) 캡처돼야 한다 — 재배치 전 캡처의 증거"
        );
    }

    /// 원격 **에이전트**가 보낸 close 는 서버 복원 스택에 아무것도 남기지 않는다 — 세
    /// close 모두. 복원 스택은 사용자 상태라 에이전트 행동의 부수효과가 닿으면 안 되고
    /// (`docs/identity.md` 원칙 1), 남기면 서버 앞 사용자의 Ctrl+Shift+T 가 에이전트가
    /// 닫은 것을 자기 워크스페이스에 되살린다(ADR-0480). 같은 모양의 op 를 `User` 로
    /// 보내면 남는다는 것은 위 세 시험이 본다 — 둘을 가르는 것이 origin 하나뿐이다.
    #[test]
    fn a_forwarded_agent_close_leaves_no_snapshot() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let creates = [
            StructuralOp::SplitSurface {
                surface_id: a,
                direction: SplitAxis::Horizontal,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            StructuralOp::NewTab {
                anchor_surface_id: a,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            StructuralOp::SplitPane {
                anchor_surface_id: a,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
        ];
        let mut added = Vec::new();
        for op in &creates {
            let fd = execute_forwarded_structural_op(
                &mut core,
                &mut state,
                &mut engine,
                op,
                ForwardOrigin::User,
            )
            .expect("create ok")
            .expect("create delta");
            added.push(fd.added_terminals[0]);
        }
        let closes = [
            StructuralOp::CloseSurface {
                surface_id: added[0],
            },
            StructuralOp::CloseTab {
                anchor_surface_id: added[1],
            },
            StructuralOp::ClosePane {
                anchor_surface_id: added[2],
            },
        ];
        let before = engine.closed_items.len();
        for op in &closes {
            execute_forwarded_structural_op(
                &mut core,
                &mut state,
                &mut engine,
                op,
                ForwardOrigin::Agent,
            )
            .unwrap_or_else(|e| panic!("agent close {op:?} must succeed: {e}"));
            assert!(
                engine.find_workspace_index_for_surface(a).is_some(),
                "시험 전제: anchor 가 살아 있어 워크스페이스 통째 close 갈래가 아니어야 한다"
            );
        }
        for sid in &added {
            assert!(
                engine.find_workspace_index_for_surface(*sid).is_none(),
                "시험 전제: 에이전트 close 가 실제로 닫았어야 한다 — surface {sid}"
            );
        }
        assert_eq!(
            engine.closed_items.len(),
            before,
            "원격 에이전트의 close 는 서버 복원 스택에 아무것도 남기지 않아야 한다"
        );
    }

    /// 반대 축의 회귀 방지: **일반 IPC** close 는 여전히 스냅샷을 안 남긴다. 면제는
    /// 데이터(params)가 아니라 **진입 경로**가 정한다는 규율이 살아 있는지 본다.
    #[test]
    fn a_plain_ipc_close_still_leaves_no_snapshot() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::SplitSurface {
                surface_id: a,
                direction: SplitAxis::Horizontal,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];

        let before = engine.closed_items.len();
        let resp = crate::adapters::ipc::handler::surface::handle_surface_close(
            &mut core,
            &mut state,
            &mut engine,
            serde_json::Value::Null,
            &serde_json::json!({ "surface_id": b }),
        );
        assert!(resp.error.is_none(), "일반 IPC close 자체는 성공해야 한다");
        assert_eq!(
            engine.closed_items.len(),
            before,
            "에이전트 경로의 close 는 사용자 되돌리기 스택을 건드리지 않는다"
        );
    }

    /// forward 된 복원은 원격에서 실제로 탭을 되살리고, 그 터미널이 delta 의 added 에
    /// 잡혀 tap 대상이 된다 — 그래야 client mirror 에 나타나고 원격 PTY 출력이 흐른다.
    #[test]
    fn a_forwarded_restore_recreates_the_tab_and_lands_in_the_delta() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        // 탭을 하나 더 만들고 forward close 로 닫아 서버 스택에 항목을 만든다.
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::NewTab {
                anchor_surface_id: a,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("new tab ok")
        .expect("new tab delta");
        let b = fd.added_terminals[0];
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::CloseTab {
                anchor_surface_id: b,
            },
            ForwardOrigin::User,
        )
        .expect("close ok");

        let before = engine.workspaces[0].all_surface_ids().len();
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::RestoreClosedItem {
                anchor_surface_id: a,
            },
            ForwardOrigin::User,
        )
        .expect("restore must succeed")
        .expect("delta must be produced");
        assert_eq!(
            engine.workspaces[0].all_surface_ids().len(),
            before + 1,
            "복원은 anchor 워크스페이스 안에 surface 를 되살려야 한다"
        );
        assert_eq!(
            fd.added_terminals.len(),
            1,
            "복원된 터미널이 tap 대상에 잡혀야 한다 — 안 그러면 mirror 에 화면이 안 온다"
        );
    }

    /// 스택이 비면 **실패가 아니라** 전용 sentinel 로 회신한다 — client 가 일반 forward
    /// 실패 문구("적용하지 못했습니다") 대신 안내 toast 를 쓴다.
    #[test]
    fn a_forwarded_restore_with_an_empty_stack_reports_the_sentinel_reason() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let err = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::RestoreClosedItem {
                anchor_surface_id: a,
            },
            ForwardOrigin::User,
        )
        .expect_err("빈 스택은 Err(reason) 이어야 한다");
        assert_eq!(err, tasty_ipc::stream::STRUCTURAL_REASON_RESTORE_EMPTY);
    }

    /// forward 된 복원은 이 기계 앞에 앉은 사용자의 활성 워크스페이스·focused pane 을
    /// **움직이지 않는다**(루트 CLAUDE.md 원칙 1·3). 로컬 경로의 cascade 를 그대로
    /// 재현하면 깨지는 불변식이라 회귀 방지로 고정한다.
    #[test]
    fn a_forwarded_restore_does_not_move_the_local_users_focus() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::SplitPane {
                anchor_surface_id: a,
                direction: SplitAxis::Vertical,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("split pane ok")
        .expect("split pane delta");
        let b = fd.added_terminals[0];
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::ClosePane {
                anchor_surface_id: b,
            },
            ForwardOrigin::User,
        )
        .expect("close pane ok");

        let ws_before = state.active_workspace;
        let focus_before: Vec<(u32, u32)> = engine
            .workspaces
            .iter()
            .map(|w| (w.id, w.focused_pane))
            .collect();
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::RestoreClosedItem {
                anchor_surface_id: a,
            },
            ForwardOrigin::User,
        )
        .expect("restore must succeed");
        assert_eq!(
            state.active_workspace, ws_before,
            "원격의 복원이 로컬 사용자의 활성 워크스페이스를 바꾸면 안 된다"
        );
        let focus_after: Vec<(u32, u32)> = engine
            .workspaces
            .iter()
            .map(|w| (w.id, w.focused_pane))
            .collect();
        assert_eq!(
            focus_before, focus_after,
            "원격의 복원이 로컬 사용자의 focused pane 을 바꾸면 안 된다"
        );
    }

    /// 다른 워크스페이스에서 닫힌 항목은 **후보가 아니다** — 스택 top 이 그것이어도
    /// anchor 워크스페이스 스코프는 자기 것만 본다(ADR-0264 결정 3).
    #[test]
    fn a_forwarded_restore_never_takes_another_workspaces_item() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        // anchor 워크스페이스(0)에 복원할 항목을 하나 만든다.
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::NewTab {
                anchor_surface_id: a,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("new tab ok")
        .expect("new tab delta");
        let b = fd.added_terminals[0];
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::CloseTab {
                anchor_surface_id: b,
            },
            ForwardOrigin::User,
        )
        .expect("close ok");

        // 그 뒤 **다른** 워크스페이스에서 탭이 닫혀 스택 top 을 차지한다.
        let other_ws = core
            .create_default_workspace(&mut engine)
            .expect("second workspace");
        let other_sid = engine.workspaces[other_ws].all_surface_ids()[0];
        engine
            .terminals
            .insert(other_sid, Terminal::new_detached(80, 24));
        let other_pane = engine.workspaces[other_ws]
            .pane_layout()
            .first_pane()
            .unwrap()
            .id;
        let other_tab_idx = 0;
        let snapshot = engine
            .capture_closed_tab(other_pane, other_tab_idx)
            .expect("other workspace tab snapshot");
        engine.push_closed_item(snapshot);

        let restored_ws0 = engine.workspaces[0].all_surface_ids().len();
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::RestoreClosedItem {
                anchor_surface_id: a,
            },
            ForwardOrigin::User,
        )
        .expect("restore must succeed");
        assert_eq!(
            engine.workspaces[0].all_surface_ids().len(),
            restored_ws0 + 1,
            "anchor 워크스페이스의 항목이 복원돼야 한다"
        );
        assert_eq!(
            engine.closed_items.len(),
            1,
            "다른 워크스페이스 항목은 스택에 그대로 남아야 한다"
        );
    }

    /// forward 된 split 으로 원격에 새로 생긴 surface 는 점유된 workspace 의 hard 점유를
    /// **상속**한다(ADR-0040 "workspace 전체가 remote"). 등록이 빠지면 새 surface 가
    /// 비점유로 남아 (host 창 sweep 이 grid 를 되돌리는) 레터박스·(holder 검증 실패로)
    /// client 입력 거부를 유발한다. surface_locks(is_hard_occupied) + surface_to_workspace
    /// (입력 라우팅) 양쪽에 등록돼야 한다.
    #[test]
    fn forward_split_inherits_workspace_occupancy() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 42;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");
        let op = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("expected Ok")
        .expect("split delta");
        let new_sid = fd.added_terminals[0];
        assert!(
            engine.attach.is_hard_occupied(new_sid),
            "새 surface 는 hard 점유(surface_locks)를 상속해야 한다 — resize skip/readonly"
        );
        assert_eq!(
            engine.attach.workspace_of_surface(new_sid),
            Some(ws_id),
            "새 surface 는 점유 workspace 멤버로 등록돼야 한다 — 입력 라우팅 holder 검증"
        );
        assert_eq!(
            engine.attach.workspace_holder_of(new_sid),
            Some(client_id),
            "새 surface 의 holder 는 workspace holder 와 동일해야 한다"
        );
    }

    /// 회귀 가드(이중 tap 으로 attach client 화면에 타이핑 글자가
    /// 중복 렌더링되던 버그). 이전 코드는 `execute_forwarded_structural_op` 내부에서
    /// (재사용 핸들러가 호출하는 `apply_split_pane`/`apply_split_surface`/
    /// `apply_create_tab` 를 통해) 즉시 tap 하고, 호출측(`event_handler.rs`/`boot.rs`)이
    /// `StructuralDelta` 전송 후 `added_terminals` 를 **또** tap 해 같은 surface 에
    /// tap 이 2개 등록됐다(`Terminal::fan_out_to_taps` 는 등록된 모든 tap 에 매 PTY
    /// 청크를 전부 내보내므로 tap 2개 = client 도착 2번). 이전 테스트들(`set_notifier`
    /// 미주입)은 `tap_new_workspace_member` 의 `notifier()` 가드에 걸려 tap 호출 자체가
    /// 스킵돼 이 회귀를 잡지 못했다 — 이 테스트는 실제 `StreamHub` 를 주입해 그 가드를
    /// 통과시킨다.
    #[test]
    fn forward_split_surface_taps_exactly_once_with_real_stream_hub() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 7;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");
        let hub = tasty_ipc::stream_hub::StreamHub::new();
        let _rx = hub.register(client_id);
        engine.attach.set_notifier(hub.clone());

        let op = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("expected Ok")
        .expect("split delta");
        let new_sid = fd.added_terminals[0];

        assert_eq!(
            engine.terminals.get(new_sid).unwrap().output_tap_count(),
            0,
            "execute_forwarded_structural_op 자체는 tap 하면 안 된다 — 순서 보장은 \
             호출측이 StructuralDelta 전송 후 tap 하는 것에 있다(초기 스냅샷이 client \
             매핑 생성 전에 도착해 드롭되는 것도 방지)"
        );

        // 호출측(event_handler.rs/boot.rs)이 delta 전송 후 하는 것과 동일한 후속 tap.
        engine.tap_surface_for_stream(new_sid, client_id, &hub);
        assert_eq!(
            engine.terminals.get(new_sid).unwrap().output_tap_count(),
            1,
            "호출측 tap 이후엔 정확히 1개만 등록돼야 한다 — 2개면 이중 tap 회귀"
        );
    }

    /// 점유된 워크스페이스에 `a` 와 forward split 으로 만든 `b` 를 두고, holder(7)의
    /// 실제 `StreamHub` 수신단을 돌려준다. 이 시점까지 holder 에게 나간 프레임은 없다.
    fn attached_pair(
        core: &mut crate::core::Core,
        state: &mut AppState,
        engine: &mut crate::core::CoreState,
    ) -> (u32, u32, u32, tasty_ipc::stream_hub::SinkReceiver) {
        let a = seed(engine);
        let b = execute_forwarded_structural_op(
            core,
            state,
            engine,
            &StructuralOp::SplitSurface {
                surface_id: a,
                direction: SplitAxis::Horizontal,
                surface_kind: "terminal".to_string(),
                params: serde_json::json!({}),
            },
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta")
        .added_terminals[0];
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a, b], &[a, b], 7)
            .expect("workspace 점유 획득");
        let hub = tasty_ipc::stream_hub::StreamHub::new();
        let rx = hub.register(7);
        engine.attach.set_notifier(hub);
        (a, b, ws_id, rx)
    }

    /// holder 가 받은 Control 프레임을 전부 꺼낸다(다른 태그는 버린다).
    fn drain_control(rx: &tasty_ipc::stream_hub::SinkReceiver) -> Vec<serde_json::Value> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .filter(|f| f.tag == tasty_ipc::stream::StreamTag::Control)
            .map(|f| serde_json::from_slice(&f.payload).expect("control json"))
            .collect()
    }

    fn surface_ids_of(delta: &serde_json::Value) -> Vec<u64> {
        delta["surfaces"]
            .as_array()
            .expect("surfaces")
            .iter()
            .filter_map(|s| s["remote_id"].as_u64())
            .collect()
    }

    /// 서버에서 PTY 가 끝나 닫힌 멤버는 forward 가 아닌 원인의 구조 변경이다 — holder 가
    /// 닫힌 뒤의 트리를 `StructuralDelta` 로 정확히 한 번 받는다(ADR-0481). 이 경로가 없던
    /// 동안 mirror 는 서버에서 이미 사라진 탭을 다음 forward 까지 계속 보였다.
    #[test]
    fn a_pty_exit_in_an_attached_workspace_reaches_the_holder_as_a_delta() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (a, b, ws_id, rx) = attached_pair(&mut core, &mut state, &mut engine);
        crate::app::process_exit::handle(&mut core, &mut state, &mut engine, b);
        let msgs = drain_control(&rx);
        assert_eq!(msgs.len(), 1, "delta 는 정확히 한 번: {msgs:?}");
        assert_eq!(msgs[0]["event"], "structural_delta");
        assert_eq!(msgs[0]["workspace_id"], ws_id);
        assert_eq!(
            surface_ids_of(&msgs[0]),
            vec![u64::from(a)],
            "닫힌 b 가 빠진 트리"
        );
        assert_eq!(
            engine.attach.workspace_holder(ws_id),
            Some(7),
            "점유는 그대로"
        );
    }

    /// forward 된 close 는 자기 delta 를 호출측이 보낸다 — 그 실행 중 닫힌 멤버가 남긴
    /// 표시가 남아 있으면 같은 트리가 한 번 더 나간다. 실행이 표시를 지우는지 본다.
    #[test]
    fn a_forwarded_close_leaves_no_structure_change_behind() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (_a, b, _ws_id, rx) = attached_pair(&mut core, &mut state, &mut engine);
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &StructuralOp::CloseSurface { surface_id: b },
            ForwardOrigin::User,
        )
        .expect("close ok")
        .expect("close delta");
        engine.push_structure_changes();
        assert!(
            drain_control(&rx).is_empty(),
            "forward close 의 트리는 호출측이 보내는 delta 하나뿐이어야 한다"
        );
    }

    /// 점유 워크스페이스의 마지막 멤버가 끝나면 워크스페이스가 통째로 사라진다 — 보낼 트리가
    /// 없으므로 forward 경로의 같은 상황처럼 holder 를 강제 detach 하고 lock 을 정리한다.
    #[test]
    fn the_last_member_exiting_force_detaches_the_holder() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (a, b, ws_id, rx) = attached_pair(&mut core, &mut state, &mut engine);
        crate::app::process_exit::handle(&mut core, &mut state, &mut engine, b);
        drain_control(&rx);
        crate::app::process_exit::handle(&mut core, &mut state, &mut engine, a);
        assert!(
            engine.find_workspace_index_for_id(ws_id).is_none(),
            "시험 전제: 워크스페이스가 purge 됐다"
        );
        let msgs = drain_control(&rx);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert_eq!(msgs[0]["event"], "force_detached");
        assert_eq!(
            engine.attach.workspace_holder(ws_id),
            None,
            "stale lock 없음"
        );
    }

    /// 로컬 경로로 점유 워크스페이스에 생긴 멤버는 delta 가 tap(스냅샷)보다 **먼저** 나가야
    /// client 가 매핑을 만든 뒤 스냅샷을 받는다 — forward 경로의 "delta → tap" 과 같은 순서.
    #[test]
    fn a_local_member_reaches_the_holder_as_a_delta_before_its_snapshot() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (a, _b, _ws_id, rx) = attached_pair(&mut core, &mut state, &mut engine);
        let pane_id = engine.find_pane_for_surface(a).expect("pane");
        crate::core::structural_exec::create_tab(
            &mut core,
            &mut state,
            &mut engine,
            pane_id,
            &serde_json::json!({ "pane_id": pane_id }),
            false,
        )
        .expect("local create_tab");
        let first = rx.try_recv().expect("holder 에게 무언가 나가야 한다");
        assert_eq!(
            first.tag,
            tasty_ipc::stream::StreamTag::Control,
            "첫 프레임은 delta"
        );
        let delta: serde_json::Value = serde_json::from_slice(&first.payload).unwrap();
        assert_eq!(delta["event"], "structural_delta");
        assert_eq!(surface_ids_of(&delta).len(), 3, "a · b · 새 탭");
    }

    /// `engine` 에 점유와 무관한 워크스페이스(`ws_id`)를 하나 더 두고 그 안에 `surface_id` 를
    /// 살려 둔다 — anchor 가 "다른 곳에 살아 있는" 상황의 fixture.
    fn push_unrelated_workspace(engine: &mut crate::core::CoreState, ws_id: u32, surface_id: u32) {
        let surface: Box<dyn crate::model::Surface> =
            Box::new(crate::core::egui_mesh_surface::EguiMeshSurface::new(
                surface_id,
                "image",
                "com.tasty.image".to_string(),
                "elsewhere".to_string(),
                None,
            ));
        let pane =
            crate::model::Pane::new_with_surface(ws_id, ws_id, "elsewhere".to_string(), surface);
        engine
            .workspaces
            .push(crate::model::Workspace::new_with_pane(
                ws_id,
                "elsewhere".to_string(),
                pane,
            ));
    }

    /// anchor 가 점유 워크스페이스 밖이라도 **살아 있으면** "no live surface" 는 거짓이다 —
    /// 사유는 종전 문구로 남는다. 정말 없는 id 에만 IPC 와 같은 문구가 붙는다(ADR-0482).
    #[test]
    fn an_anchor_alive_in_another_workspace_is_not_called_gone() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (_a, _b, _ws_id, _rx) = attached_pair(&mut core, &mut state, &mut engine);
        push_unrelated_workspace(&mut engine, 900, 901);
        let alive = StructuralOp::CloseSurface { surface_id: 901 };
        let gone = StructuralOp::CloseSurface { surface_id: 555 };
        let reason =
            |op| crate::core::attach_structure_sync::unresolved_forward_reason([&engine], 7, op);
        assert_eq!(reason(&alive), "workspace not found");
        assert!(
            reason(&gone).starts_with("no live surface 555 "),
            "{}",
            reason(&gone)
        );
    }

    /// GUI 는 engine 을 여럿 가진다. 점유한 engine 은 다른 engine 의 surface 를 모르므로,
    /// anchor 가 거기 살아 있으면 판정은 모든 engine 을 본 뒤에 내려져야 한다.
    #[test]
    fn an_anchor_alive_in_another_engine_is_not_called_gone() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let (_a, _b, _ws_id, _rx) = attached_pair(&mut core, &mut state, &mut engine);
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut other = crate::core::CoreState::new(80, 24, waker).expect("engine");
        push_unrelated_workspace(&mut other, 900, 901);
        let op = StructuralOp::CloseSurface { surface_id: 901 };
        let reason = |engines: Vec<&crate::core::CoreState>| {
            crate::core::attach_structure_sync::unresolved_forward_reason(engines, 7, &op)
        };
        assert_eq!(reason(vec![&engine, &other]), "workspace not found");
        assert_eq!(reason(vec![&other, &engine]), "workspace not found");
        assert!(reason(vec![&engine]).starts_with("no live surface 901 "));
    }

    /// forward 된 NewTab 도 실제 실행(pane 은 anchor surface 로 resolve). Ok + delta + 터미널 +1.
    #[test]
    fn forward_new_tab_executes() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let before = engine.terminals.iter().count();
        let op = StructuralOp::NewTab {
            anchor_surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        let fd = r.expect("expected Ok").expect("new-tab 성공은 delta 동반");
        assert_eq!(engine.terminals.iter().count(), before + 1);
        assert_eq!(fd.added_terminals.len(), 1);
    }

    /// forward 된 CloseTab 은 cascade 로 surface 를 제거한다 — delta 에 그 surface 가
    /// 빠지고(=client 가 removed 도출), added 는 비어 있다.
    #[test]
    fn forward_close_tab_removes_from_delta() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        // 먼저 new-tab 으로 두 번째 탭(surface) 을 만든다.
        let mk = StructuralOp::NewTab {
            anchor_surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let added = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &mk,
            ForwardOrigin::User,
        )
        .expect("new-tab Ok")
        .expect("new-tab delta");
        let new_sid = added.added_terminals[0];
        // 새 surface 가 속한 탭을 닫는다.
        let close = StructuralOp::CloseTab {
            anchor_surface_id: new_sid,
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &close,
            ForwardOrigin::User,
        )
        .expect("close Ok")
        .expect("close delta");
        assert!(fd.added_terminals.is_empty(), "close 는 added 없음");
        let ids = delta_surface_ids(&fd);
        assert!(
            !ids.contains(&new_sid),
            "닫힌 surface 는 delta 에서 빠져야 한다"
        );
        assert!(ids.contains(&a), "남은 surface 는 delta 에 유지");
    }

    /// 원격에 등록되지 않은 kind(예: plugin markdown 부재)는 Err(reason) — client 가
    /// 실패 toast 를 띄우고 어느 쪽도 구조를 바꾸지 않는다.
    #[test]
    fn forward_unknown_kind_fails() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let before = engine.terminals.iter().count();
        let op = StructuralOp::NewTab {
            anchor_surface_id: a,
            surface_kind: "definitely-not-registered".to_string(),
            params: serde_json::json!({}),
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        assert!(r.is_err(), "unknown kind must fail");
        assert!(
            r.unwrap_err().contains("unknown surface kind"),
            "reason 이 미등록 kind 를 가리켜야 한다"
        );
        assert_eq!(
            engine.terminals.iter().count(),
            before,
            "실패한 forward 는 새 터미널을 만들지 않는다"
        );
    }

    /// 두 진입점의 실패 문구를 재는 입력 표. 원격이 보내는 파라미터 묶음이 IPC 요청과
    /// 같은 자리로 읽힌다는 점을 겨냥한다: 원격 묶음에 `target_pane` 이 실려 오는 경우
    /// (값이 잘못됐거나, 있어서 대상이 둘이 되는 경우), 원격 기준 cwd 가 서버에 없는 경우,
    /// 서버에 없는 kind.
    ///
    /// 칸: (이름, 기대 문구, forward op 을 만드는 함수, IPC 핸들러, IPC params 를 만드는
    /// 함수). 기대 문구는 forward 가 핸들러를 재사용하던 `331baf491` 의 문구를 리터럴로
    /// 옮긴 것이다 — 도메인 함수가 계산하는 값이 아니다.
    struct FailureCase(
        &'static str,
        &'static str,
        Box<dyn Fn(u32) -> StructuralOp>,
        fn(
            &mut crate::core::Core,
            &mut dyn crate::adapters::ipc::window_port::IpcWindow,
            &mut crate::core::CoreState,
            serde_json::Value,
            &serde_json::Value,
        ) -> tasty_ipc::protocol::JsonRpcResponse,
        Box<dyn Fn(u32, u32) -> serde_json::Value>,
    );

    fn failure_cases() -> Vec<FailureCase> {
        use crate::adapters::ipc::handler::{pane, tab};
        use serde_json::json;

        let missing_dir = "/definitely/not/a/tasty/test/dir";
        vec![
            FailureCase(
                "new tab of an unknown kind",
                "unknown surface kind: definitely-not-registered",
                Box::new(|a| StructuralOp::NewTab {
                    anchor_surface_id: a,
                    surface_kind: "definitely-not-registered".to_string(),
                    params: json!({}),
                }),
                tab::handle_tab_create,
                Box::new(|_, pane| json!({ "pane_id": pane, "type": "definitely-not-registered" })),
            ),
            FailureCase(
                "new tab with a cwd the server does not have",
                "cwd does not exist: /definitely/not/a/tasty/test/dir",
                Box::new(move |a| StructuralOp::NewTab {
                    anchor_surface_id: a,
                    surface_kind: "terminal".to_string(),
                    params: json!({ "cwd": missing_dir }),
                }),
                tab::handle_tab_create,
                Box::new(
                    move |_, pane| json!({ "pane_id": pane, "type": "terminal", "cwd": missing_dir }),
                ),
            ),
            FailureCase(
                "surface split whose bag also names a pane",
                "Cannot specify both 'target_surface' and 'target_pane'. Use one.",
                Box::new(|a| StructuralOp::SplitSurface {
                    surface_id: a,
                    direction: SplitAxis::Vertical,
                    surface_kind: "terminal".to_string(),
                    params: json!({ "target_pane": 1 }),
                }),
                pane::handle_split,
                Box::new(
                    |a, _| json!({ "level": "surface", "target_surface": a, "target_pane": 1, "type": "terminal" }),
                ),
            ),
            FailureCase(
                "pane split whose bag carries a malformed pane id",
                "'target_pane' was given as \"not-a-number\" — it must be a whole number that fits in 32 \
                 bits and is not negative. Refusing rather than coercing it: a truncated id names a \
                 different, possibly real, target, and a dropped value is indistinguishable from the \
                 parameter being absent",
                Box::new(|a| StructuralOp::SplitPane {
                    anchor_surface_id: a,
                    direction: SplitAxis::Horizontal,
                    surface_kind: "terminal".to_string(),
                    params: json!({ "target_pane": "not-a-number" }),
                }),
                pane::handle_split,
                Box::new(
                    |a, _| json!({ "level": "pane", "target_surface": a, "target_pane": "not-a-number", "type": "terminal" }),
                ),
            ),
            FailureCase(
                "surface split with a cwd the server does not have",
                "cwd does not exist: /definitely/not/a/tasty/test/dir",
                Box::new(move |a| StructuralOp::SplitSurface {
                    surface_id: a,
                    direction: SplitAxis::Horizontal,
                    surface_kind: "terminal".to_string(),
                    params: json!({ "cwd": missing_dir }),
                }),
                pane::handle_split,
                Box::new(
                    move |a, _| json!({ "level": "surface", "target_surface": a, "type": "terminal", "cwd": missing_dir }),
                ),
            ),
        ]
    }

    /// 한 입력을 두 진입점에 넣고 (IPC 에러 메시지, forward 회신 사유) 를 돌려준다.
    fn fail_both_ways(case: &FailureCase) -> (String, String) {
        let FailureCase(name, _, op, ipc, ipc_params) = case;
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let pane_id = engine.find_pane_for_surface(a).expect("seed pane");

        let resp = ipc(
            &mut core,
            &mut state,
            &mut engine,
            serde_json::json!(1),
            &ipc_params(a, pane_id),
        );
        let ipc_msg = resp
            .error
            .unwrap_or_else(|| panic!("{name}: IPC 가 성공했다 — 실패 입력이 아니다"))
            .message;

        let forwarded = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op(a),
            ForwardOrigin::User,
        );
        let Err(forward_msg) = forwarded else {
            panic!("{name}: forward 가 성공했다 — IPC 는 `{ipc_msg}` 로 실패했다");
        };
        (ipc_msg, forward_msg)
    }

    /// **진입점 사이의 갈림**을 잰다. forward 실행과 IPC 진입점은 같은 도메인 실행을
    /// 부르므로, 같은 입력의 실패 문구가 byte 단위로 같아야 한다 — forward 회신의 사유
    /// 문자열 = IPC 에러 메시지.
    ///
    /// 이 시험은 **기준 문구를 모른다.** 도메인 함수의 문구를 바꾸면 두 진입점이 함께
    /// 바뀌므로 여기서는 초록이다. 옛 문구의 보존은 아래
    /// `failure_reasons_keep_the_base_literals` 가 잰다.
    #[test]
    fn forward_and_ipc_fail_with_the_same_reason_for_the_same_input() {
        for case in failure_cases() {
            let (ipc_msg, forward_msg) = fail_both_ways(&case);
            assert_eq!(
                forward_msg, ipc_msg,
                "{}: 두 진입점의 실패 문구가 갈렸다",
                case.0
            );
        }
    }

    /// **기준 문구의 고정**을 잰다. forward 회신 사유는 client toast 에 그대로 나가고 IPC
    /// 에러 메시지는 에이전트가 읽으므로, 실행을 도메인 함수로 옮긴 뒤에도 문구는
    /// `331baf491`(forward 가 핸들러를 재사용하던 때)과 byte 단위로 같아야 한다
    /// (ADR-0395). 기대값은 그 커밋의 리터럴을 옮겨 적은 것이라, 문구를 바꾸는 변경은
    /// 여기서 빨개진다 — 의도한 변경이면 이 표와 ADR 을 함께 고친다.
    #[test]
    fn failure_reasons_keep_the_base_literals() {
        for case in failure_cases() {
            let (ipc_msg, forward_msg) = fail_both_ways(&case);
            assert_eq!(ipc_msg, case.1, "{}: IPC 실패 문구가 기준과 다르다", case.0);
            assert_eq!(
                forward_msg, case.1,
                "{}: forward 회신 사유가 기준과 다르다",
                case.0
            );
        }
    }

    /// anchor surface 가 서버 트리에 없으면 Err(회신) — client 매핑이 stale 한 경우.
    #[test]
    fn forward_missing_anchor_fails() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        seed(&mut engine);
        let op = StructuralOp::ClosePane {
            anchor_surface_id: 999_999,
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        assert!(r.is_err(), "missing anchor must fail");
    }

    // ─── hard-occupied dispatch 가드 회귀 방지 ──────────────
    //
    // `hard_occupied_structural_guard`(`adapters/ipc/handler.rs`)는 일반 IPC
    // method-string dispatch 에만 걸려 있고, 이 파일의 `execute_forwarded_structural_op`
    // 가 도메인 실행 함수를 직접 호출하는 forward 실행 경로는 우회한다. 아래 테스트들은
    // 그 분기가 실제로 지켜지는지 — (a) holder 의 forward 는 hard-occupied 워크스페이스
    // 에서도 여전히 성공하고, (b) 비-holder 의 일반 IPC 호출은 거부되는지 — 를 같은
    // fixture 로 함께 확인한다.

    use crate::adapters::ipc::handler::handle_with_caller;
    use tasty_ipc::caller::CallerContext;
    use tasty_ipc::protocol::JsonRpcRequest;

    fn ipc_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Some(serde_json::json!(1)),
            session_token: None,
        }
    }

    /// (holder 회귀 방지) NewTab forward 는 워크스페이스가 hard-occupied 여도 여전히
    /// 성공해야 한다 — attach 연결 자체가 구조 변경 권한을 증명한다는 모델
    /// (`docs/features/remote-attach/index.md`)이 이번 가드로 깨지면 안 된다.
    #[test]
    fn forward_new_tab_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");
        let before = engine.terminals.iter().count();
        let op = StructuralOp::NewTab {
            anchor_surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward NewTab 은 hard-occupied 상태에서도 성공해야 한다"
        );
        assert_eq!(engine.terminals.iter().count(), before + 1);
    }

    /// (holder 회귀 방지) SplitPane forward 도 동일하게 hard-occupied 에서 성공해야 한다.
    #[test]
    fn forward_split_pane_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");
        let panes_before = engine.workspaces[0].pane_layout().all_pane_ids().len();
        let op = StructuralOp::SplitPane {
            anchor_surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward SplitPane 은 hard-occupied 상태에서도 성공해야 한다"
        );
        assert_eq!(
            engine.workspaces[0].pane_layout().all_pane_ids().len(),
            panes_before + 1
        );
    }

    /// (holder 회귀 방지) ClosePane forward 는 hard-occupied 워크스페이스의 pane 이
    /// 2개 이상일 때 여전히 성공해야 한다.
    #[test]
    fn forward_close_pane_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        // 두 번째 pane 을 먼저 만든다(점유 전 — 아직 로컬 자유 상태).
        let split = StructuralOp::SplitPane {
            anchor_surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &split,
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];
        let all: Vec<u32> = engine.workspaces[0].all_surface_ids();
        engine
            .attach
            .acquire_workspace(ws_id, &all, &all, 7)
            .expect("workspace 점유 획득");
        let panes_before = engine.workspaces[0].pane_layout().all_pane_ids().len();
        let close = StructuralOp::ClosePane {
            anchor_surface_id: b,
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &close,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward ClosePane 은 hard-occupied 상태에서도 성공해야 한다"
        );
        assert_eq!(
            engine.workspaces[0].pane_layout().all_pane_ids().len(),
            panes_before - 1
        );
    }

    /// (holder 회귀 방지) MoveTab forward 는 hard-occupied 워크스페이스에서도 성공해야
    /// 한다.
    #[test]
    fn forward_move_tab_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        // 같은 pane 에 두 번째 tab 을 만든다(점유 전).
        let new_tab = StructuralOp::NewTab {
            anchor_surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &new_tab,
            ForwardOrigin::User,
        )
        .expect("new-tab ok")
        .expect("new-tab delta");
        let all: Vec<u32> = engine.workspaces[0].all_surface_ids();
        engine
            .attach
            .acquire_workspace(ws_id, &all, &all, 7)
            .expect("workspace 점유 획득");
        let mv = StructuralOp::MoveTab {
            anchor_surface_id: a,
            from_index: 0,
            to_index: 1,
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &mv,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward MoveTab 은 hard-occupied 상태에서도 성공해야 한다"
        );
    }

    /// (holder 회귀 방지) CloseSurface forward 는 같은 tab 안에 형제 surface 가 있을 때
    /// hard-occupied 워크스페이스에서도 성공해야 한다.
    #[test]
    fn forward_close_surface_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        // 같은 tab 안에 형제 surface 를 만든다(점유 전).
        let split_surface = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &split_surface,
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];
        let all: Vec<u32> = engine.workspaces[0].all_surface_ids();
        engine
            .attach
            .acquire_workspace(ws_id, &all, &all, 7)
            .expect("workspace 점유 획득");
        let before = engine.terminals.iter().count();
        let close = StructuralOp::CloseSurface { surface_id: b };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &close,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward CloseSurface 는 hard-occupied 상태에서도 성공해야 한다"
        );
        assert_eq!(engine.terminals.iter().count(), before - 1);
    }

    /// 회귀 가드(mirror desync): workspace 의 마지막 surface 를 forward CloseSurface 로
    /// 닫으면 `close_case_workspace`("Case 4: last pane in workspace")가 워크스페이스를
    /// 통째로 purge 한다 — `execute_forwarded_structural_op` 이 실행 후 `ws_id` 로
    /// 재조회하면 실패하는 지점. 이전에는 그냥 `Ok(None)`(delta 없음)을 반환해 mirror
    /// client 화면이 갱신되지 않았다(재attach 전까지 존재하지 않는 workspace 를 계속
    /// 보여줌). 이제는 그 대신 `force_detach_workspace` 를 태워 holder 에게 강제분리
    /// 통지(Control "force_detached" + `Detach`)를 push 하고 `OccupancyRegistry` 의
    /// stale lock 도 정리해야 한다.
    #[test]
    fn forward_close_last_surface_force_detaches_holder() {
        use std::time::Duration;
        use tasty_ipc::stream::StreamTag;
        use tasty_ipc::stream_hub::StreamHub;

        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine); // 단일 surface, 형제 없음
        let ws_id = engine.workspaces[0].id;

        let hub = StreamHub::new();
        let holder = hub.alloc_id();
        let rx = hub.register(holder);
        engine.attach.set_notifier(hub);
        let all: Vec<u32> = engine.workspaces[0].all_surface_ids();
        engine
            .attach
            .acquire_workspace(ws_id, &all, &all, holder)
            .expect("workspace 점유 획득");

        let close = StructuralOp::CloseSurface { surface_id: a };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &close,
            ForwardOrigin::User,
        );

        assert!(
            r.is_ok(),
            "마지막 surface close forward 는 에러가 아니어야 한다"
        );
        assert!(
            engine.find_workspace_index_for_id(ws_id).is_none(),
            "workspace 는 purge 되어야 한다"
        );

        // recv_timeout — 회귀 시(force_detach_workspace 호출 누락) push 가 아예
        // 없어 `recv()` 가 무기한 block 되므로, 테스트가 실패 대신 CI 를 통째로
        // 멈추게 만든다. 동기 in-process 호출이라 프레임은 즉시 도착해야 한다.
        let f1 = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("force_detached Control 프레임이 와야 한다");
        assert_eq!(f1.tag, StreamTag::Control);
        assert!(String::from_utf8_lossy(&f1.payload).contains("force_detached"));
        let f2 = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Detach 프레임이 와야 한다");
        assert_eq!(f2.tag, StreamTag::Detach);

        assert_eq!(engine.attach.workspace_holder(ws_id), None);
    }

    /// forward 된 ConvertSurface 는 (비-mirror) 서버 워크스페이스에서 실제로
    /// `Core::apply(DomainIntent::ConvertSurface)` 를 실행해 kind 를 교체한다. Ok +
    /// delta.converted_surface 에 대상 surface_id 가 실린다(호출자가 egui-mesh
    /// stale-frame 방지를 트리거하는 신호 — `ForwardedDelta` 문서 참조).
    #[test]
    fn forward_convert_surface_executes_and_converts() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let op = StructuralOp::ConvertSurface {
            surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
            cwd: None,
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("convert ok")
        .expect("convert delta");
        assert_eq!(
            fd.converted_surface,
            Some(a),
            "converted_surface 는 실제로 교체된 surface_id 를 실어야 한다"
        );
        assert!(
            engine.terminals.get(a).is_some(),
            "terminal 로의 convert 는 새 Terminal 을 insert 해야 한다"
        );
    }

    /// (holder 회귀 방지) ConvertSurface forward 는 hard-occupied 워크스페이스에서도
    /// 성공해야 한다(`execute_forwarded_structural_op` 는 IPC method-string 가드를
    /// 우회하는 직접 함수 호출 경로).
    #[test]
    fn forward_convert_surface_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");
        let op = StructuralOp::ConvertSurface {
            surface_id: a,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
            cwd: None,
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward ConvertSurface 는 hard-occupied 상태에서도 성공해야 한다"
        );
    }

    /// 변환 결과 surface 의 explorer root 를 돌려주는 테스트 헬퍼.
    fn explorer_root(engine: &crate::core::CoreState, surface_id: u32) -> std::path::PathBuf {
        engine
            .find_surface_by_id(surface_id)
            .expect("converted surface")
            .as_any()
            .downcast_ref::<tasty_model::ExplorerPanel>()
            .expect("explorer 로 변환됐어야 한다")
            .current_root()
            .to_path_buf()
    }

    /// op 에 실려 온 cwd(=client 가 source surface 에서 해석한 값)가 변환 결과에
    /// 적용된다 — 이게 없으면 explorer root 가 상대경로 폴백으로 떨어진다.
    #[test]
    fn forward_convert_surface_applies_wire_cwd() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let dir = tempfile::tempdir().expect("test tempdir");
        let op = StructuralOp::ConvertSurface {
            surface_id: a,
            surface_kind: "explorer".to_string(),
            params: serde_json::json!({}),
            cwd: Some(dir.path().to_string_lossy().into_owned()),
        };
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("convert ok")
        .expect("convert delta");
        assert_eq!(explorer_root(&engine, a), dir.path());
    }

    /// cwd 를 싣지 않은 op(구버전 client / OSC 7 미방출 원격 셸)는 서버가 대상
    /// surface 의 실제 cwd 를 직접 resolve 해 채운다.
    #[test]
    fn forward_convert_surface_resolves_cwd_from_target_surface() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let dir = tempfile::tempdir().expect("test tempdir");
        engine
            .terminals
            .get_mut(a)
            .expect("seeded terminal")
            .set_cached_cwd(dir.path().to_path_buf());
        let op = StructuralOp::ConvertSurface {
            surface_id: a,
            surface_kind: "explorer".to_string(),
            params: serde_json::json!({}),
            cwd: None,
        };
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("convert ok")
        .expect("convert delta");
        assert_eq!(explorer_root(&engine, a), dir.path());
    }

    /// 서버측 resolve 는 원격 인스턴스의 `inherit_cwd` 설정 게이트를 따른다(로컬
    /// convert 와 동일 의미론) — 꺼져 있으면 cwd 를 채우지 않는다.
    #[test]
    fn forward_convert_surface_respects_inherit_cwd_gate() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let dir = tempfile::tempdir().expect("test tempdir");
        engine
            .terminals
            .get_mut(a)
            .expect("seeded terminal")
            .set_cached_cwd(dir.path().to_path_buf());
        engine.settings.general.inherit_cwd = false;
        let op = StructuralOp::ConvertSurface {
            surface_id: a,
            surface_kind: "explorer".to_string(),
            params: serde_json::json!({}),
            cwd: None,
        };
        execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &op,
            ForwardOrigin::User,
        )
        .expect("convert ok")
        .expect("convert delta");
        assert_ne!(explorer_root(&engine, a), dir.path());
    }

    /// forward 된 MoveSurface 는 서버에서 실제로 A(source) 를 B(target) 자리로
    /// 이동시키고, B 의 PTY 를 `cascade_surface_closed` 로 cleanup 한다 — 이 cascade 를
    /// 빼먹으면 B 의 PTY 가 리크된다(CloseSurface 의 `close_surface_via_intent` 와 동일
    /// 근거).
    #[test]
    fn forward_move_surface_executes_and_cleans_up_target() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let split_surface = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &split_surface,
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];
        let before = engine.terminals.iter().count();

        let mv = StructuralOp::MoveSurface {
            source_surface_id: a,
            target_surface_id: b,
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &mv,
            ForwardOrigin::User,
        )
        .expect("move ok")
        .expect("move delta");
        assert!(
            fd.converted_surface.is_none(),
            "move 는 convert 가 아니다 — mesh stale-frame 처리 대상 아님"
        );
        assert_eq!(
            engine.terminals.iter().count(),
            before - 1,
            "B(target) 의 PTY 는 cascade 로 정리돼야 한다(안 하면 리크)"
        );
        assert!(
            engine.terminals.get(a).is_some(),
            "A(source) 는 살아있어야 한다(PTY 보존 — R1)"
        );
    }

    /// (holder 회귀 방지) MoveSurface forward 는 hard-occupied 워크스페이스에서도
    /// 성공해야 한다.
    #[test]
    fn forward_move_surface_succeeds_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let split_surface = StructuralOp::SplitSurface {
            surface_id: a,
            direction: SplitAxis::Horizontal,
            surface_kind: "terminal".to_string(),
            params: serde_json::json!({}),
        };
        let fd = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &split_surface,
            ForwardOrigin::User,
        )
        .expect("split ok")
        .expect("split delta");
        let b = fd.added_terminals[0];
        let all: Vec<u32> = engine.workspaces[0].all_surface_ids();
        engine
            .attach
            .acquire_workspace(ws_id, &all, &all, 7)
            .expect("workspace 점유 획득");
        let mv = StructuralOp::MoveSurface {
            source_surface_id: a,
            target_surface_id: b,
        };
        let r = execute_forwarded_structural_op(
            &mut core,
            &mut state,
            &mut engine,
            &mv,
            ForwardOrigin::User,
        );
        assert!(
            r.is_ok(),
            "holder 의 forward MoveSurface 는 hard-occupied 상태에서도 성공해야 한다"
        );
    }

    /// 비-holder 경로(`docs/dev-guide/attach-behavior.md` "서버(피점유)측 비-holder
    /// 구조 변경 차단" 절): hard-occupied 워크스페이스에 일반 IPC(`split`/
    /// `tab.create`)로 직접 호출하면 거부되고 트리가 그대로 유지돼야 한다.
    #[test]
    fn dispatch_denies_structural_create_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let pane_id = engine.workspaces[0].pane_layout().all_pane_ids()[0];
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");

        let terminals_before = engine.terminals.iter().count();
        let panes_before = engine.workspaces[0].pane_layout().all_pane_ids().len();

        for (method, params) in [
            ("tab.create", serde_json::json!({ "pane_id": pane_id })),
            (
                "split",
                serde_json::json!({
                    "level": "surface",
                    "target_surface": a,
                    "direction": "horizontal",
                }),
            ),
        ] {
            let req = ipc_request(method, params);
            let resp = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &req,
                &CallerContext::Local,
            );
            let err = resp.error.unwrap_or_else(|| {
                panic!("{method}: hard-occupied 워크스페이스에 대한 비-holder 요청은 거부돼야 한다")
            });
            assert!(
                err.message.to_lowercase().contains("occupied"),
                "{method}: 에러 메시지에 점유 안내가 있어야 한다 (got: {})",
                err.message
            );
        }

        assert_eq!(
            engine.terminals.iter().count(),
            terminals_before,
            "거부된 요청은 새 터미널을 만들면 안 된다"
        );
        assert_eq!(
            engine.workspaces[0].pane_layout().all_pane_ids().len(),
            panes_before,
            "거부된 요청은 새 pane 을 만들면 안 된다"
        );
    }

    /// 비-holder 경로(`docs/dev-guide/attach-behavior.md` "서버(피점유)측 비-holder
    /// 구조 변경 차단" 절 — 극단 케이스는 이 가드로 도달 불가능해짐): hard-occupied
    /// 워크스페이스에 일반 IPC(`pane.close`/`tab.close`/`tab.move`/`surface.close`)로
    /// 직접 호출하면 거부되고 트리가 그대로 유지돼야 한다.
    #[test]
    fn dispatch_denies_structural_close_move_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let pane_id = engine.workspaces[0].pane_layout().all_pane_ids()[0];
        let tab_id = engine.workspaces[0]
            .pane_layout()
            .find_pane(pane_id)
            .expect("pane exists")
            .tabs[0]
            .id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");

        let terminals_before = engine.terminals.iter().count();
        let panes_before = engine.workspaces[0].pane_layout().all_pane_ids().len();
        let tabs_before = engine.workspaces[0]
            .pane_layout()
            .find_pane(pane_id)
            .expect("pane exists")
            .tabs
            .len();

        for (method, params) in [
            ("pane.close", serde_json::json!({ "pane_id": pane_id })),
            ("tab.close", serde_json::json!({ "tab_id": tab_id })),
            (
                "tab.move",
                serde_json::json!({ "pane_id": pane_id, "from_index": 0, "to_index": 0 }),
            ),
            ("surface.close", serde_json::json!({ "surface_id": a })),
        ] {
            let req = ipc_request(method, params);
            let resp = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &req,
                &CallerContext::Local,
            );
            let err = resp.error.unwrap_or_else(|| {
                panic!("{method}: hard-occupied 워크스페이스에 대한 비-holder 요청은 거부돼야 한다")
            });
            assert!(
                err.message.to_lowercase().contains("occupied"),
                "{method}: 에러 메시지에 점유 안내가 있어야 한다 (got: {})",
                err.message
            );
        }

        assert_eq!(engine.terminals.iter().count(), terminals_before);
        assert_eq!(
            engine.workspaces[0].pane_layout().all_pane_ids().len(),
            panes_before,
            "거부된 요청은 pane 을 닫으면 안 된다"
        );
        assert_eq!(
            engine.workspaces[0]
                .pane_layout()
                .find_pane(pane_id)
                .expect("pane exists")
                .tabs
                .len(),
            tabs_before,
            "거부된 요청은 tab 을 닫거나 이동하면 안 된다"
        );
    }

    /// 비-holder 경로(ADR-0060): `terminal.spawn`(`tasty claude/codex spawn` 이 호출하는
    /// IPC)도 hard-occupied 워크스페이스에 대해서는 거부되고 tab/surface 가 전혀
    /// 생성되지 않아야 한다 — spawn 이 성공 응답을 준 직후 `tap_new_workspace_member`
    /// 로 새 surface 가 hard lock 을 상속받아, spawn 을 호출한 쪽조차 자기 결과물에
    /// 입력을 못 넣게 되는 부작용을 애초에 막는다.
    #[test]
    fn dispatch_denies_terminal_spawn_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");

        let terminals_before = engine.terminals.iter().count();
        let pane_id = engine.workspaces[0].pane_layout().all_pane_ids()[0];
        let tabs_before = engine.workspaces[0]
            .pane_layout()
            .find_pane(pane_id)
            .expect("pane exists")
            .tabs
            .len();

        let req = ipc_request(
            "terminal.spawn",
            serde_json::json!({ "parent": a, "workspace": ws_id.to_string() }),
        );
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );
        let err = resp.error.unwrap_or_else(|| {
            panic!("hard-occupied 워크스페이스로의 terminal.spawn 은 거부돼야 한다")
        });
        assert!(
            err.message.to_lowercase().contains("occupied"),
            "에러 메시지에 점유 안내가 있어야 한다 (got: {})",
            err.message
        );

        assert_eq!(
            engine.terminals.iter().count(),
            terminals_before,
            "거부된 spawn 은 새 터미널을 만들면 안 된다"
        );
        assert_eq!(
            engine.workspaces[0]
                .pane_layout()
                .find_pane(pane_id)
                .expect("pane exists")
                .tabs
                .len(),
            tabs_before,
            "거부된 spawn 은 새 tab 을 만들면 안 된다"
        );
    }

    /// 점유가 없는 workspace 로의 `terminal.spawn` 은 가드 추가 후에도 기존처럼
    /// 정상 동작해야 한다(회귀 방지).
    #[test]
    fn dispatch_allows_terminal_spawn_when_not_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let terminals_before = engine.terminals.iter().count();

        let req = ipc_request(
            "terminal.spawn",
            serde_json::json!({ "parent": a, "workspace": ws_id.to_string() }),
        );
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );
        assert!(
            resp.error.is_none(),
            "점유되지 않은 workspace 의 terminal.spawn 은 허용돼야 한다: {:?}",
            resp.error
        );
        assert_eq!(
            engine.terminals.iter().count(),
            terminals_before + 1,
            "정상 spawn 은 새 터미널을 만들어야 한다"
        );
    }

    // ─── mirror(원격 attach client) 워크스페이스 spawn 가드 ──────────────
    //
    // ADR-0086. mirror 워크스페이스 안의 구조 변경은 원격으로 forward 되는 것이
    // 정상 설계지만, `terminal.spawn` 만은 그 응답에서 생성된 surface 를 **동기로**
    // 꺼내 써야 해서 forward 위에 얹힐 수 없다. 막지 않으면 로컬은 에러를 돌려주는데
    // 원격에는 탭이 남는 고아가 생긴다.

    /// mirror 워크스페이스로의 `terminal.spawn` 은 거부되고, 원격으로 나갈 구조 op 가
    /// 큐에 쌓이지 않아야 한다(고아 원격 탭 방지 — 이 테스트의 핵심).
    #[test]
    fn dispatch_denies_terminal_spawn_into_mirror_workspace() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine.workspaces[0].mirror = true;

        let terminals_before = engine.terminals.iter().count();

        let req = ipc_request(
            "terminal.spawn",
            serde_json::json!({ "parent": a, "workspace": ws_id.to_string() }),
        );
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );

        let err = resp
            .error
            .expect("mirror 워크스페이스 spawn 은 거부돼야 한다");
        assert!(
            err.message.to_lowercase().contains("mirror"),
            "에러 메시지에 mirror 사유가 있어야 한다 (got: {})",
            err.message
        );
        assert!(
            !err.message.contains("surface_id"),
            "내부 필드 이름이 노출되면 안 된다 (got: {})",
            err.message
        );
        assert!(
            engine.pending_structural_forward.is_empty(),
            "거부된 spawn 은 원격 NewTab 을 forward 하면 안 된다"
        );
        assert_eq!(engine.terminals.iter().count(), terminals_before);
    }

    /// (설계 보존) 같은 mirror 워크스페이스라도 `tab.create` 는 **여전히 forward**
    /// 돼야 한다 — 가드가 mirror 구조 변경 전체를 막아버리면 attach 의 핵심 기능이
    /// 회귀한다. 로컬 트리는 그대로고 원격 큐에 `NewTab` 이 쌓인다.
    #[test]
    fn dispatch_still_forwards_tab_create_in_mirror_workspace() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        seed(&mut engine);
        let pane_id = engine.workspaces[0].pane_layout().all_pane_ids()[0];
        engine.workspaces[0].mirror = true;
        let terminals_before = engine.terminals.iter().count();

        let req = ipc_request("tab.create", serde_json::json!({ "pane_id": pane_id }));
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );

        assert!(
            resp.error.is_none(),
            "mirror 구조 변경은 forward success 로 회신돼야 한다: {:?}",
            resp.error
        );
        assert!(
            matches!(
                engine.pending_structural_forward.first().map(|p| &p.op),
                Some(StructuralOp::NewTab { .. })
            ),
            "mirror 의 tab.create 는 원격 NewTab 으로 큐잉돼야 한다 (got: {:?})",
            engine.pending_structural_forward.first().map(|p| &p.op)
        );
        assert_eq!(
            engine.terminals.iter().count(),
            terminals_before,
            "forward 된 구조 변경은 로컬 트리를 바꾸지 않는다"
        );
    }

    /// `--pane` 오버라이드가 `workspace` 파라미터와 **다른** 워크스페이스를 가리키는
    /// 경로도 막힌다. 대상 판정을 최종 pane 기준으로 옮기기 전에는 `workspace` 만
    /// 봤기 때문에, 무해한 워크스페이스를 `workspace` 로 주고 차단 대상 pane 을
    /// `pane` 으로 주면 mirror 도 hard-occupied(ADR-0060)도 그대로 새어 나갔다.
    #[test]
    fn dispatch_denies_terminal_spawn_when_pane_override_targets_blocked_workspace() {
        for blocked in ["mirror", "hard-occupied"] {
            let (mut core, mut state, mut engine, _home) = make_core_state();
            let a = seed(&mut engine);
            let blocked_ws_id = engine.workspaces[0].id;
            let blocked_pane = engine.workspaces[0].pane_layout().all_pane_ids()[0];

            // 우회에 쓸 무해한 두 번째 워크스페이스.
            let create = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &ipc_request("workspace.create", serde_json::json!({ "name": "clean" })),
                &CallerContext::Local,
            );
            assert!(create.error.is_none(), "테스트 준비: {:?}", create.error);
            let clean_ws_id = engine
                .workspaces
                .iter()
                .find(|w| w.id != blocked_ws_id)
                .expect("두 번째 워크스페이스")
                .id;

            match blocked {
                "mirror" => engine.workspaces[0].mirror = true,
                _ => {
                    engine
                        .attach
                        .acquire_workspace(blocked_ws_id, &[a], &[a], 7)
                        .expect("workspace 점유 획득");
                }
            }

            let terminals_before = engine.terminals.iter().count();
            let req = ipc_request(
                "terminal.spawn",
                serde_json::json!({
                    "parent": a,
                    "workspace": clean_ws_id.to_string(),
                    "pane": blocked_pane,
                }),
            );
            let resp = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &req,
                &CallerContext::Local,
            );

            let err = resp.error.unwrap_or_else(|| {
                panic!("{blocked}: pane 오버라이드로 차단 대상 워크스페이스에 spawn 되면 안 된다")
            });
            let msg = err.message.to_lowercase();
            assert!(
                msg.contains("mirror") || msg.contains("occupied"),
                "{blocked}: 에러 메시지에 사유가 있어야 한다 (got: {})",
                err.message
            );
            assert_eq!(
                engine.terminals.iter().count(),
                terminals_before,
                "{blocked}: 거부된 spawn 은 새 터미널을 만들면 안 된다"
            );
            assert!(
                engine.pending_structural_forward.is_empty(),
                "{blocked}: 거부된 spawn 은 원격으로 아무것도 보내지 않는다"
            );
        }
    }

    /// 점유가 없는 워크스페이스에서는 가드가 오탐하지 않고 정상 통과해야 한다
    /// (false-positive 방지 — 가드가 hard-occupied 가 아닌 workspace 까지 막으면
    /// 일반 사용자의 로컬 작업 자체가 회귀한다).
    #[test]
    fn dispatch_allows_tab_create_when_not_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        seed(&mut engine);
        let pane_id = engine.workspaces[0].pane_layout().all_pane_ids()[0];
        let req = ipc_request("tab.create", serde_json::json!({ "pane_id": pane_id }));
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );
        assert!(
            resp.error.is_none(),
            "점유되지 않은 workspace 의 tab.create 는 허용돼야 한다: {:?}",
            resp.error
        );
    }

    /// hard-occupied 워크스페이스에 대한 비-holder 의 `markdown.navigate`/`image.open`
    /// (=ConvertSurface 진입점) IPC 도 다른 6종과 동일하게 거부돼야 한다(guard 확장
    /// 회귀 방지). 가드가 method-string 매치 먼저 걸리므로 `path` 가 실존하지 않아도
    /// (핸들러 본문의 존재 검증까지 못 감) 점유 에러로 막혀야 한다.
    #[test]
    fn dispatch_denies_convert_entrypoints_when_hard_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], 7)
            .expect("workspace 점유 획득");

        for (method, params) in [
            (
                "markdown.navigate",
                serde_json::json!({ "surface_id": a, "path": "/tmp/does-not-matter.md" }),
            ),
            (
                "image.open",
                serde_json::json!({ "surface_id": a, "path": "/tmp/does-not-matter.png" }),
            ),
        ] {
            let req = ipc_request(method, params);
            let resp = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &req,
                &CallerContext::Local,
            );
            let err = resp.error.unwrap_or_else(|| {
                panic!("{method}: hard-occupied 워크스페이스에 대한 비-holder 요청은 거부돼야 한다")
            });
            assert!(
                err.message.to_lowercase().contains("occupied"),
                "{method}: 에러 메시지에 점유 안내가 있어야 한다 (got: {})",
                err.message
            );
        }
    }

    /// 점유가 없으면 가드가 오탐하지 않아야 한다(`markdown.navigate` false-positive
    /// 방지). path 는 실존하지 않아도 되는데, 가드가 먼저 통과시키면 그 다음
    /// `markdown::handle_navigate` 의 자체 존재 검증(`invalid_params`)에 걸려 에러가
    /// 나긴 하지만, 그 에러 메시지에는 "occupied" 가 없어야 한다(가드 통과 확인).
    #[test]
    fn dispatch_allows_markdown_navigate_when_not_occupied() {
        let (mut core, mut state, mut engine, _home) = make_core_state();
        let a = seed(&mut engine);
        let req = ipc_request(
            "markdown.navigate",
            serde_json::json!({ "surface_id": a, "path": "/tmp/does-not-matter.md" }),
        );
        let resp = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &req,
            &CallerContext::Local,
        );
        if let Some(err) = resp.error {
            assert!(
                !err.message.to_lowercase().contains("occupied"),
                "점유되지 않은 workspace 는 가드에 걸리면 안 된다: {}",
                err.message
            );
        }
    }
}

#[cfg(test)]
mod bulk_capacity_tests {
    //! 저장 폴더 사용량 합산 + 용량 경계 판정.
    use super::{dir_used_bytes, exceeds_capacity};

    #[test]
    fn dir_used_bytes_sums_top_level_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("b.bin"), vec![0u8; 250]).unwrap();
        // 하위 디렉토리는 세지 않는다(1-depth 합산).
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("c.bin"), vec![0u8; 999]).unwrap();
        assert_eq!(dir_used_bytes(dir.path()), 350);
    }

    #[test]
    fn dir_used_bytes_missing_dir_is_zero() {
        let missing = std::path::Path::new("/nonexistent/tasty/transfers/xyz");
        assert_eq!(dir_used_bytes(missing), 0);
    }

    #[test]
    fn capacity_boundary_equal_is_allowed() {
        // used + incoming == max → 허용(거부 false).
        assert!(!exceeds_capacity(400, 100, 500));
        assert!(!exceeds_capacity(0, 500, 500));
        assert!(!exceeds_capacity(500, 0, 500));
    }

    #[test]
    fn capacity_boundary_over_is_rejected() {
        // used + incoming > max → 거부(true).
        assert!(exceeds_capacity(400, 101, 500));
        assert!(exceeds_capacity(0, 501, 500));
    }

    #[test]
    fn capacity_saturates_on_overflow() {
        // used + incoming 이 u64 를 넘겨도 saturating_add 로 MAX 에 고정되어
        // 유한한 상한을 확실히 초과(거부)한다.
        assert!(exceeds_capacity(u64::MAX, 1, 500));
        assert!(exceeds_capacity(u64::MAX - 1, 100, 1_000_000));
    }
}
