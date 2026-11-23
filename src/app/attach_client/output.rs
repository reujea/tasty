//! Apply ordered incoming events to windowed or parked mirror engines.

mod apply_event;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod close_history_tests;
#[cfg(test)]
mod close_order_tests;
#[cfg(test)]
mod home_probe_tests;
#[cfg(test)]
mod http_tests;
mod list_dir;
mod remote_entry;
#[cfg(test)]
mod remote_entry_tests;
#[cfg(test)]
mod tests;
use super::agent_origin;
use super::connection::find_parked_with_workspace;
use super::deliver_git_query_result;
use super::navigation::{capture_focused_remote, restore_focus_after_delta, set_focus_to_surface};
use super::projection::{build_mirror_workspace, lease_mirror_ids, mirror_id_needs};
use super::resources::{
    install_mirror_fallbacks, push_markdown_changed, push_markdown_content_result,
};
use super::round_intake::RoundIntake;
use super::survivors::{keep_terminal_resize_state, merge_survivor_mapping};
use super::user_waits;
use crate::app::App;
use crate::app::window_access::engines_mut;
use crate::ipc::stream::StreamTag;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_session::EngineId;
use crate::view::ui::View as _;
use apply_event::apply_one_mirror_event;
use serde_json::Value;
use tasty_remote::client_session::{
    AttachClientSession, MirrorEvent, PendingOpFocus, SessionState,
};

impl App {
    /// 출력 회차 하나: 관측 → 설치 → 이벤트 적용 → 처리.
    ///
    /// 창이 있는 engine, parked engine 순서로 적용 대상을 찾은 뒤 출력 버퍼를 비운다.
    /// 대상이 없으면 버퍼를 유지한다. 고아 판정·정리도 같은 engine 범위를 확인해야 한다.
    pub(crate) fn apply_attach_client_output(&mut self) {
        let intake = self.observe_round();
        self.finish_round(intake);
    }

    /// 관측 뒤의 단계. 같은 회차의 판정은 모두 `intake`와 관측 단계가 쓴 표지·고정 사유를 쓴다.
    pub(crate) fn finish_round(&mut self, intake: RoundIntake) {
        self.poll_pending_mirror_installs(&intake);
        super::op_runs::end_closed_connections();
        if self.remote.sessions.is_empty() {
            return;
        }
        let mut dead: Vec<usize> = Vec::new();
        let mut closed: Vec<usize> = Vec::new();
        let mut unknown_close: Vec<u32> = Vec::new();
        let mut reconnecting: Vec<usize> = Vec::new();
        let mut resyncing: Vec<usize> = Vec::new();
        let mut stopped: Vec<(usize, bool)> = Vec::new();
        for idx in 0..self.remote.sessions.len() {
            let (local_ws, phase, saved_row) = {
                let sess = &self.remote.sessions[idx];
                (
                    sess.state.local_workspace,
                    sess.state.phase,
                    sess.state.saved_row,
                )
            };

            // 막힌 저장 행은 손실 복구로도 다시 붙지 않는다. 이벤트 적용은 복구 안내를 내지 않고, 처리는
            // 재동기화 대신 멈춘다.
            let blocked = saved_row.is_some_and(|row| self.row_blocked(row));
            let host = mirror_output_host(
                self.find_main_with_workspace(local_ws),
                self.engines().parked_with_ids().map(|(id, e)| (id, e.core)),
                local_ws,
            );
            // delta 뒤의 출력도 갱신된 ID 매핑을 써야 하므로 세션을 복제하지 않고 나눠 빌린다.
            match host {
                Some(MirrorOutputHost::Window(wid)) => {
                    let sess = &mut self.remote.sessions[idx];
                    let mut pair = engines_mut!(self).window_pair(wid);
                    let mut mirror_host = pair.as_mut().map(|(m, engine)| {
                        let view = m.base.state.identity();
                        MirrorHost::windowed(&mut m.state, engine)
                            .for_view(view)
                            .row_blocked(blocked)
                    });
                    if let Some(host) = mirror_host.as_mut() {
                        resume_resync_in_window(sess, host);
                    }
                    let applied =
                        apply_pending_mirror_output(sess, mirror_host, &mut self.plugin_manager);
                    if applied && let Some((main, _)) = pair {
                        main.mark_dirty_from(crate::view::RepaintSource::AttachMirror);
                    }
                }
                Some(MirrorOutputHost::Parked(id)) => {
                    let sess = &mut self.remote.sessions[idx];
                    let (state, mut engine) = engines_mut!(self)
                        .parked_session(id)
                        .expect("mirror_output_host가 방금 찾은 parked engine");
                    apply_pending_mirror_output(
                        sess,
                        Some(MirrorHost::parked(state, &mut engine).row_blocked(blocked)),
                        &mut self.plugin_manager,
                    );
                }
                None => {}
            }
            // 같은 묶음에 손실 통지와 EOF가 올 수 있어 이벤트 적용 뒤 재attach 여부를 확인한다.
            let sess = &mut self.remote.sessions[idx];
            let events_deferred = sess.transport.output.has_pending();
            let (disposition, unknown) = route_session_end(sess, phase, events_deferred, blocked);
            if unknown {
                unknown_close.push(sess.state.local_workspace);
            }
            match disposition {
                DisconnectDisposition::Resync => resyncing.push(idx),
                DisconnectDisposition::Reconnect => reconnecting.push(idx),
                DisconnectDisposition::Stop if saved_row.is_some() => stopped.push((idx, false)),
                DisconnectDisposition::StopThenConnect if saved_row.is_some() => {
                    stopped.push((idx, true))
                }
                DisconnectDisposition::Stop | DisconnectDisposition::StopThenConnect => {
                    dead.push(idx)
                }
                DisconnectDisposition::Cleanup => dead.push(idx),
                DisconnectDisposition::ServerClosed => closed.push(idx),
                DisconnectDisposition::None => {}
            }
        }
        for &idx in &resyncing {
            if !self.resync_session(idx) {
                if self.remote.sessions[idx].state.saved_row.is_some() {
                    reconnecting.push(idx);
                } else {
                    dead.push(idx);
                }
            }
        }
        // 서버가 넘긴 알림음은 세션이 정리되기 전에 재생한다.
        let sounds: u32 = self
            .remote
            .sessions
            .iter_mut()
            .map(|sess| std::mem::take(&mut sess.state.notification_sounds))
            .sum();
        for _ in 0..sounds {
            self.services.sound_player().play();
        }
        // 이번 묶음에서 받은 원격 자식 통지·결과를 세션이 정리되기 전에 적용한다.
        self.pump_agent_children();
        self.pump_terminal_reads();
        self.pump_file_downloads();
        self.notify_unknown_closes(unknown_close);
        for &idx in &reconnecting {
            self.reconnect_ended_session(idx);
        }
        let restart: Vec<u32> = stopped
            .iter()
            .filter_map(|&(idx, connect_after)| self.stop_saved_row(idx, connect_after))
            .collect();
        self.end_sessions(&dead, &closed);
        // 멈춤 뒤에 미룬 명시 연결을 지금 시작한다. 세션 인덱스가 바뀐 뒤라 행으로 찾는다.
        for row in restart {
            self.connect_after_stop(row);
        }
    }

    /// 이 mirror workspace의 서버가 터미널 유래 알림을 직접 만들어 그 소리를 넘겨주는지.
    pub(crate) fn mirror_receives_notification_sounds(&self, workspace: u32) -> bool {
        self.remote.sessions.iter().any(|session| {
            session.state.local_workspace == workspace && session.transport.notification_sounds
        })
    }
}

/// 이벤트를 적용한 뒤 세션의 끝을 정한다. `phase`는 이벤트 적용 전에 읽은 값이다.
/// 두 번째 값은 회신 없이 끝난 서버 닫기가 있어 결과를 모른다고 알려야 하는지다.
///
/// 순서: 서버 닫기 성공 회신 → 관측·설치 단계가 지금 전송에 정한 멈춤 → 관측한 끝(처리하지 않은 것만).
/// 정한 멈춤은 ID 예약을 기다리는 이벤트가 남아 있으면 다음 회차로 미룬다. 그 이벤트에 닫기 회신이
/// 있을 수 있기 때문이다. 미룬 동안 명시적 연결이 와 멈춤에 연결 의사를 남겼으면 `StopThenConnect`다.
/// 서버 닫기 성공 회신이 같은 회차에 있으면 그 의사는 세션과 함께 버려진다. `blocked`는 저장 행이 막혀 있다는 뜻이다. 막힌 행의 손실 복구(재동기화)는
/// 멈춤이 되고, 같은 이유로 남은 이벤트가 있으면 지금 전송의 멈춤으로 미룬다.
fn route_session_end(
    sess: &mut AttachClientSession,
    phase: SessionState,
    events_deferred: bool,
    blocked: bool,
) -> (DisconnectDisposition, bool) {
    let disposition =
        if sess.state.workspace_close == tasty_remote::client_session::WorkspaceClose::Closed {
            DisconnectDisposition::ServerClosed
        } else if sess
            .state
            .stop_after_events
            .as_ref()
            .is_some_and(|stop| stop.is_for(&sess.transport.frame_tx.epoch()))
        {
            if events_deferred {
                DisconnectDisposition::None
            } else {
                match sess.state.stop_after_events.take() {
                    Some(stop) if stop.connect_after => DisconnectDisposition::StopThenConnect,
                    _ => DisconnectDisposition::Stop,
                }
            }
        } else {
            // 다른 전송에 정한 멈춤은 지금 전송의 것이 아니다.
            sess.state.stop_after_events = None;
            match classify_observed_end(sess, phase) {
                DisconnectDisposition::Resync if blocked && events_deferred => {
                    sess.state.stop_after_events =
                        Some(tasty_remote::client_session::DeferredStop::new(
                            sess.transport.frame_tx.epoch(),
                        ));
                    DisconnectDisposition::None
                }
                DisconnectDisposition::Resync if blocked => DisconnectDisposition::Stop,
                observed => observed.after_discard(sess, phase, events_deferred),
            }
        };
    // 결과 불명은 Connected 세션의 끝을 처리하는 이 자리에서 알리고 그 요청에 알렸다고 기록한다. 재연결
    // 중인 세션의 멈춤은 여기서 알리지 않는다. 같은 epoch 에서 다시 처리되는 Connected 세션과 사용자 끊기는
    // 기록을 보고 다시 알리지 않는다.
    let unknown = phase == SessionState::Connected
        && matches!(
            disposition,
            DisconnectDisposition::Resync
                | DisconnectDisposition::Reconnect
                | DisconnectDisposition::Stop
                | DisconnectDisposition::StopThenConnect
                | DisconnectDisposition::Cleanup
        )
        && super::workspace_close::take_unknown_close_notice(sess);
    (disposition, unknown)
}

/// 관측 단계가 기록한 끝을 한 번만 분류한다. 지금 전송의 끝이 아니거나 이미 분류한 끝은 None이다.
/// 강제 끊김은 정식 종료라 재연결·재동기화하지 않는다.
fn classify_observed_end(
    sess: &mut AttachClientSession,
    phase: SessionState,
) -> DisconnectDisposition {
    let current = sess.transport.frame_tx.epoch();
    let resync_released = sess.resync_released();
    let saved = sess.state.saved_row.is_some();
    let Some(end) = sess
        .state
        .observed_end
        .as_mut()
        .filter(|end| !end.routed && end.epoch.same(&current))
    else {
        return DisconnectDisposition::None;
    };
    end.routed = true;
    if phase != SessionState::Connected {
        return DisconnectDisposition::None;
    }
    if end.reason == tasty_remote::transport_end::EndReason::Forced {
        return DisconnectDisposition::Stop;
    }
    disconnect_disposition(true, phase, resync_released, saved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MirrorOutputHost {
    Window(winit::window::WindowId),
    Parked(EngineId),
}

/// 창이 있는 engine을 우선하고 없으면 parked engine에서 찾는다. None이면 버퍼를 비우지 않는다.
/// 이 mirror workspace 를 가진 engine. 원격 자식 처리도 출력 적용과 같은 범위를 쓴다.
pub(super) fn mirror_engine_host(app: &App, local_workspace: u32) -> Option<MirrorOutputHost> {
    mirror_output_host(
        app.find_main_with_workspace(local_workspace),
        app.engines().parked_with_ids().map(|(id, e)| (id, e.core)),
        local_workspace,
    )
}

fn mirror_output_host<'a>(
    windowed: Option<winit::window::WindowId>,
    parked: impl IntoIterator<Item = (EngineId, &'a crate::core::CoreState)>,
    local_workspace: u32,
) -> Option<MirrorOutputHost> {
    windowed.map(MirrorOutputHost::Window).or_else(|| {
        find_parked_with_workspace(parked, local_workspace).map(MirrorOutputHost::Parked)
    })
}

/// 출력 적용·정리·고아 판정이 공유하는 parked engine 검색. 여러 항목 모두 확인한다.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisconnectDisposition {
    None,
    Resync,
    Reconnect,
    /// 재연결·재동기화하지 않고 멈춘다. 저장된 원격 행은 끊긴 행으로 남고, 에이전트 mirror는 정리한다.
    /// 서버의 강제 끊기와 관측·설치 단계가 정한 멈춤이 여기로 온다.
    Stop,
    /// 정한 멈춤을 적용한 뒤 미룬 동안 받은 명시적 연결을 잇는다(`App::stop_saved_row`).
    StopThenConnect,
    Cleanup,
    /// 서버가 workspace를 닫았다고 회신했다. 재연결 없이 로컬에서 정리한다.
    ServerClosed,
}

/// 연결별 종료 신호에는 한 번만 반응한다. 저장된 원격 행은 재연결하고 에이전트 mirror는 정리한다.
/// 손실로 Detach를 보낸 세션은 저장 여부와 관계없이 재attach한다.
/// parked에서 Detach를 미룬 동안 별도로 끊기면 일반 끊김으로 처리한다.
fn disconnect_disposition(
    disconnected: bool,
    state: SessionState,
    resync_released: bool,
    saved_row: bool,
) -> DisconnectDisposition {
    if !disconnected || state != SessionState::Connected {
        DisconnectDisposition::None
    } else if resync_released {
        DisconnectDisposition::Resync
    } else if saved_row {
        DisconnectDisposition::Reconnect
    } else {
        DisconnectDisposition::Cleanup
    }
}

/// 창과 parked 상태의 공통 적용 대상. 창이 있어야 의미 있는 toast만 구별한다.
struct MirrorHost<'a, 'engine> {
    state: &'a mut crate::state::MainViewState,
    engine: &'a mut EngineMut<'engine>,
    windowed: bool,
    /// 창의 View. 요청한 View에만 보내는 안내(캡처 회신)가 대조한다.
    view: Option<std::sync::Weak<()>>,
    /// 저장 행이 막혀 손실 복구가 다시 붙지 않는다. 복구 안내를 내지 않는다.
    row_blocked: bool,
}

impl<'a, 'engine> MirrorHost<'a, 'engine> {
    fn focus_surface(&mut self, workspace: u32, surface: Option<u32>) {
        if let Some(surface) = surface
            && let Some(index) = self.engine.find_workspace_index_for_id(workspace)
            && let Some(workspace) = self.engine.workspace_at(index)
        {
            set_focus_to_surface(&mut self.state.navigation, workspace, surface);
        }
    }

    fn windowed(
        state: &'a mut crate::state::MainViewState,
        engine: &'a mut EngineMut<'engine>,
    ) -> Self {
        Self {
            state,
            engine,
            windowed: true,
            view: None,
            row_blocked: false,
        }
    }

    fn for_view(mut self, view: std::sync::Weak<()>) -> Self {
        self.view = Some(view);
        self
    }

    fn row_blocked(mut self, blocked: bool) -> Self {
        self.row_blocked = blocked;
        self
    }

    /// 요청을 만든 View가 지금 이 창의 View 일 때만 안내한다.
    fn toast_for(
        &mut self,
        origin: &std::sync::Weak<()>,
        message: String,
        kind: crate::adapters::ui::ToastKind,
    ) {
        if self.view.as_ref().is_some_and(|view| view.ptr_eq(origin)) {
            self.toast(message, kind);
        } else {
            tracing::debug!("attach mirror: 요청한 View 가 아니라 안내 생략 — {message}");
        }
    }

    fn parked(
        state: &'a mut crate::state::MainViewState,
        engine: &'a mut EngineMut<'engine>,
    ) -> Self {
        Self {
            state,
            engine,
            windowed: false,
            view: None,
            row_blocked: false,
        }
    }

    /// 적용 대상이 준비된 뒤 버퍼를 비우고 순서대로 적용한다.
    fn drain_and_apply(
        &mut self,
        sess: &mut AttachClientSession,
        plugin_manager: &mut Option<crate::plugin::PluginManager>,
    ) -> bool {
        let drained = sess.transport.output.drain();
        let changed = !drained.is_empty();
        if changed {
            apply_mirror_events(sess, self, plugin_manager, drained);
        }
        super::html::synchronize(sess, self.engine) || changed
    }

    fn toast(&mut self, message: String, kind: crate::adapters::ui::ToastKind) {
        if self.windowed {
            self.state
                .toasts
                .push(message, kind, crate::adapters::ui::ToastScope::Window);
        } else {
            tracing::info!("attach mirror: parked engine 이라 toast 생략 — {message}");
        }
    }
}

/// 적용 대상이 없으면 버퍼를 유지해 다음 수신 이벤트나 주기 확인에서 다시 처리한다.
fn apply_pending_mirror_output(
    sess: &mut AttachClientSession,
    host: Option<MirrorHost<'_, '_>>,
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
) -> bool {
    let Some(mut host) = host else {
        return false;
    };
    host.drain_and_apply(sess, plugin_manager)
}

fn apply_mirror_events(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    events: Vec<MirrorEvent>,
) {
    let epoch = sess.transport.frame_tx.epoch();
    let mut remaining = events.into_iter();
    while let Some(ev) = remaining.next() {
        if !epoch.is_active() || !epoch.same(&sess.transport.frame_tx.epoch()) {
            break;
        }
        if let MirrorEvent::StructuralDelta { tree, surfaces, .. } = &ev {
            let admission = mirror_id_needs(tree, surfaces.len(), false).and_then(|needed| {
                host.engine
                    .runtime
                    .ids
                    .ensure(&needed)
                    .map_err(anyhow::Error::new)
            });
            if let Err(error) = admission {
                if matches!(
                    error.downcast_ref::<crate::runtime::id_reservations::ReservationError>(),
                    Some(crate::runtime::id_reservations::ReservationError::Pending)
                ) {
                    let mut tail = vec![ev];
                    tail.extend(remaining);
                    sess.transport.output.restore_front(tail);
                    (host.engine.runtime.waker)();
                } else {
                    tracing::warn!("mirror structural identity reservation failed: {error}");
                    // 사유만 올린다. 쓰기 스레드가 reader를 기다린 뒤 확정한다.
                    sess.transport.frame_tx.retire();
                    sess.transport
                        .end
                        .raise(tasty_remote::transport_end::EndReason::Lost);
                }
                return;
            }
        }
        apply_one_mirror_event(sess, host, plugin_manager, ev);
    }
}

/// 손실 통지마다 스트림 표지를 바꾼다. 이미 재attach 대기 중이면 손실 수를 누적한다.
/// parked 상태에서는 창을 다시 찾을 때까지 Detach를 미룬다.
fn begin_resync(sess: &mut AttachClientSession, host: &mut MirrorHost<'_, '_>, frames: u64) {
    for &local in sess.state.remote_to_local.values() {
        if let Some(t) = host.engine.runtime.terminals.get_mut(local) {
            t.renew_output_stream();
            // 끊긴 뒤의 바이트는 이어지지 않는다. 미완성 시퀀스와 옛 스트림 질의의 늦은 응답을 버린다.
            t.break_stream();
        }
    }
    if let Some(total) = sess.state.resync_pending.as_mut() {
        *total += frames;
        return;
    }
    sess.state.resync_pending = Some(frames);
    if !host.windowed {
        sess.state.resync_awaiting_window = true;
        tracing::warn!(
            "attach mirror: mirror workspace {} — 서버가 이 연결의 프레임 {frames} 장을 버렸다. 창이 없어(parked) 재attach 를 창이 돌아올 때까지 미룬다",
            sess.state.local_workspace
        );
        return;
    }
    tracing::warn!(
        "attach mirror: mirror workspace {} — 서버가 이 연결의 프레임 {frames} 장을 버렸다. 옛 연결을 놓고 재attach 한다",
        sess.state.local_workspace
    );
    release_for_resync(sess, host);
}

fn resume_resync_in_window(sess: &mut AttachClientSession, host: &mut MirrorHost<'_, '_>) {
    if !sess.state.resync_awaiting_window || !host.windowed {
        return;
    }
    sess.state.resync_awaiting_window = false;
    tracing::info!(
        "attach mirror: mirror workspace {} — 창이 돌아왔다. 미뤄 둔 재attach 를 시작한다(손실 {} 장)",
        sess.state.local_workspace,
        sess.state.resync_pending.unwrap_or(0)
    );
    release_for_resync(sess, host);
}

/// Detach 뒤 EOF를 확인하면 재attach한다. 큐 전송 실패도 끊김 처리를 기다린다.
/// 막힌 저장 행은 처리 단계가 재attach 대신 멈추므로 복구 안내를 내지 않는다. 멈춘 행은 끊긴 행 표시로
/// 보인다.
fn release_for_resync(sess: &mut AttachClientSession, host: &mut MirrorHost<'_, '_>) {
    if let Err(e) = sess.send_frame(StreamTag::Detach, Vec::new()) {
        tracing::warn!("attach mirror: 재동기화용 Detach 를 큐에 못 넣었다 — 끊김을 기다린다: {e}");
    }
    if host.row_blocked {
        tracing::info!(
            "attach mirror: mirror workspace {} — 막힌 저장 행이라 재attach 하지 않아 복구 안내를 생략한다",
            sess.state.local_workspace
        );
        return;
    }
    host.toast(
        crate::i18n::t("attach.toast.mirror_desynced").to_string(),
        crate::adapters::ui::ToastKind::Warning,
    );
}

/// 회신이 말한 범위만 알린다. 이미지 게시 여부를 싣지 않은 옛 서버의 성공은 경로만 쓴 성공이다.
fn show_mirror_capture_result(
    host: &mut MirrorHost<'_, '_>,
    origin: &std::sync::Weak<()>,
    result: MirrorCaptureResult,
) {
    use crate::adapters::ui::ToastKind;
    let MirrorCaptureResult {
        ok,
        path,
        reason,
        image,
        image_reason,
    } = result;
    // 경로·사유는 길이를 모른다. 꼬리가 잘려 파일 이름이 사라지지 않도록 가운데를 줄인다.
    let with_detail = |key: &str, detail: Option<String>| {
        let base = crate::i18n::t(key);
        match detail {
            Some(d) if !d.is_empty() => {
                crate::i18n::fit_fragment(&d, |fragment| format!("{base} ({fragment})"))
            }
            _ => base.to_string(),
        }
    };
    let (msg, kind) = match (ok, image) {
        (false, _) => (
            with_detail("attach.toast.mirror_capture_failed", reason),
            ToastKind::Warning,
        ),
        (true, Some(true)) => (
            with_detail("attach.toast.mirror_capture_saved", path),
            ToastKind::Success,
        ),
        (true, Some(false)) => (
            with_detail("attach.toast.mirror_capture_path_only", image_reason),
            ToastKind::Warning,
        ),
        // 경로는 클립보드에 있다. 붙이면 토스트 길이 한도에 걸려 안내가 잘린다.
        (true, None) => (
            crate::i18n::t("attach.toast.mirror_capture_path_only_old_server").to_string(),
            ToastKind::Info,
        ),
    };
    host.toast_for(origin, msg, kind);
}

/// 기다리는 업로드의 회신만 안내한다. 이미 답한 업로드의 중복 회신이나 끊긴 뒤 정리한
/// 업로드의 늦은 회신을 다른 요청의 결과로 보이지 않게 한다.
fn apply_capture_result(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    upload_id: Option<u64>,
    result: MirrorCaptureResult,
) {
    let Some(origin) = upload_id.and_then(|id| sess.state.capture_uploads.remove(&id)) else {
        tracing::debug!(
            ?upload_id,
            ok = result.ok,
            "attach mirror: 기다리지 않는 캡처 회신이라 안내하지 않는다"
        );
        return;
    };
    // 클립보드 쓰기만 실패한 회신에도 저장 경로가 있으며, 그 경로는 붙여넣을 수 있다.
    if let (Some(paste), Some(path)) = (
        host.engine
            .remote
            .mirror_capture_pastes
            .get_mut(&sess.state.local_workspace),
        result.path.as_ref(),
    ) {
        paste.remote_path = Some(path.clone());
    }
    show_mirror_capture_result(host, &origin, result);
}

/// `MirrorEvent::CaptureResult` 중 안내에 쓰는 필드.
struct MirrorCaptureResult {
    ok: bool,
    path: Option<String>,
    reason: Option<String>,
    image: Option<bool>,
    image_reason: Option<String>,
}

/// 원격 회신이 공통으로 싣는 결과 봉투.
struct MirrorResultEnvelope {
    request_id: u64,
    ok: bool,
    truncated: bool,
    reason: Option<String>,
}

/// 요청한 plugin에 결과를 전달한다. 요청 기록이 없으면 오래된 회신으로 보고 무시한다.
fn apply_git_query_result_event(
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    state: &mut crate::state::MainViewState,
    requester: Option<String>,
    envelope: MirrorResultEnvelope,
    kind: String,
    data: Option<Value>,
) {
    let MirrorResultEnvelope {
        request_id,
        ok,
        truncated,
        reason,
    } = envelope;
    let Some(requester) = requester else {
        return;
    };
    let payload = serde_json::json!({
        "request_id": request_id,
        "ok": ok,
        "kind": kind,
        "data": data,
        "truncated": truncated,
        "reason": reason,
    });
    let popups = deliver_git_query_result(plugin_manager, &requester, &payload);
    state.plugin_mesh_popup_pending_repaint.extend(popups);
}

fn markdown_mirror_local(sess: &AttachClientSession, remote_surface_id: u32) -> Option<u32> {
    let &local = sess.state.remote_to_local.get(&remote_surface_id)?;
    sess.state.markdown_locals.contains(&local).then_some(local)
}

/// 원격 ID를 로컬 markdown ID로 바꿔 회신한다. leaf가 사라졌거나 kind가 바뀌었으면 무시한다.
#[allow(clippy::too_many_arguments)] // reason: wire 회신 필드를 풀어 받는다(GitQueryResult 적용과 같은 형태)
fn apply_markdown_content_result_event(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    plugin_manager: &mut Option<crate::plugin::PluginManager>,
    request_id: u64,
    remote_surface_id: u32,
    ok: bool,
    file: Option<String>,
    source: Option<String>,
    truncated: bool,
    reason: Option<String>,
) {
    let agent_origin = sess.state.agent_requests.take_markdown(request_id);
    let Some(local) = markdown_mirror_local(sess, remote_surface_id) else {
        return;
    };
    let payload = serde_json::json!({
        "surface_id": local,
        "request_id": request_id,
        "ok": ok,
        "file": file,
        "source": source,
        "truncated": truncated,
        "reason": reason,
    });
    push_markdown_content_result(plugin_manager, &payload);
    if ok && truncated {
        agent_origin::notify_markdown_truncated(
            |message, kind| host.toast(message, kind),
            local,
            agent_origin,
        );
    }
}

/// 새 구조를 반영하되 살아남은 surface의 로컬 ID·자원과 사용자의 포커스를 보존한다.
/// 순수 로컬 포커스 이동은 서버에 전달하지 않으므로 원격 포커스를 그대로 덮어쓰지 않는다.
/// 사라진 로컬 markdown ID를 반환해 호출자가 플러그인에 destroy를 보낼 수 있게 한다.
fn apply_mirror_structural_delta(
    navigation: &mut crate::state::navigation::NavigationState,
    sess: &mut AttachClientSession,
    engine: &mut EngineMut<'_>,
    workspace_id: u32,
    tree: &Value,
    surfaces: &[Value],
    pending_focus: Option<PendingOpFocus>,
) -> anyhow::Result<tasty_remote::client_session::RetiredDocuments> {
    let ids = lease_mirror_ids(
        &engine.runtime.ids,
        &engine.runtime.waker,
        tree,
        surfaces.len(),
        false,
    )?;

    let old_focused_remote: Option<u32> = engine
        .workspaces()
        .into_iter()
        .find(|w| w.id == sess.state.local_workspace)
        .and_then(|ws| capture_focused_remote(navigation, ws, &sess.state.remote_to_local));

    let mut mapping = merge_survivor_mapping(
        &sess.state.remote_to_local,
        surfaces,
        &ids,
        &sess.transport.frame_tx,
        engine,
    )?;
    let newly_created_remote_ids = std::mem::take(&mut mapping.newly_created_remote_ids);

    super::html::retain(sess, surfaces);
    sess.state.remote_to_local = std::mem::take(&mut mapping.remote_to_local);
    let state = &mut sess.state;
    keep_terminal_resize_state(&mut state.resize_sync, &state.remote_to_local, &mapping);
    let removed = state.replace_document_ids(mapping.markdown_ids(), mapping.html.clone());

    if let Some(pos) = engine
        .workspaces()
        .into_iter()
        .position(|w| w.id == sess.state.local_workspace)
    {
        let name = engine
            .workspace_at(pos)
            .expect("workspace index is valid")
            .name
            .clone();
        let mut ws = build_mirror_workspace(
            &mut sess.state.structure_ids,
            navigation,
            sess.state.local_workspace,
            &name,
            tree,
            &ids,
            &sess.state.remote_to_local,
            &mapping.terminals,
            &mapping.mesh,
            &mapping.explorer,
            &mut mapping.markdown,
        )?;
        install_mirror_fallbacks(&ws, &mut engine.runtime.surfaces);
        ws.mirror = true;

        // 사용자가 새 surface를 만들었다면 옛 포커스 복원보다 새 surface 선택을 우선한다.
        let mut focus_handled = false;
        if matches!(pending_focus, Some(PendingOpFocus::NewResource))
            && newly_created_remote_ids.len() == 1
            && let Some(&new_local) = newly_created_remote_ids
                .first()
                .and_then(|rid| sess.state.remote_to_local.get(rid))
        {
            focus_handled = set_focus_to_surface(navigation, &ws, new_local);
        }
        if !focus_handled {
            let restored = restore_focus_after_delta(
                navigation,
                &mut ws,
                old_focused_remote,
                &sess.state.remote_to_local,
            );
            // 옛 포커스가 사라졌으면 닫기 전에 구한 인접 후보를 시도한다. 후보도 없으면 원격 값을 유지한다.
            if !restored && let Some(PendingOpFocus::Close { candidates }) = &pending_focus {
                for &remote_cand in candidates {
                    if let Some(&local_cand) = sess.state.remote_to_local.get(&remote_cand)
                        && set_focus_to_surface(navigation, &ws, local_cand)
                    {
                        break;
                    }
                }
            }
        }

        engine
            .replace_mirror_workspace(ws)
            .unwrap_or_else(|_| panic!("mirror workspace disappeared"));
    } else {
        tracing::warn!(
            "structural delta: mirror workspace {} (remote {workspace_id}) 를 찾지 못해 갱신을 건너뛴다",
            sess.state.local_workspace
        );
    }
    Ok(removed)
}
