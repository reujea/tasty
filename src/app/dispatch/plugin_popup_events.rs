//! 플러그인 popup·banner의 닫기 요청과 포커스 변경을 전달한다.

use crate::app::App;
use crate::state::{FilePickerResult, MainViewState};

/// 부모 popup이 닫히면 자식 피커를 취소한다. 데이터를 지우지 않고 결과를 채워
/// 기존 결과 처리 경로가 요청한 플러그인에 취소를 알릴 수 있게 한다.
pub(crate) fn cancel_child_file_picker(
    state: &mut MainViewState,
    closed: &[(u64, tasty_plugin_protocol::PopupCloseReason)],
) {
    use crate::adapters::ui::popup::file_picker::{FILE_PICKER_POPUP_ID, instances};
    let mut children = Vec::new();
    for entry in state.popups.entries_of_mut(FILE_PICKER_POPUP_ID) {
        let (open, iid) = (entry.open, entry.instance);
        let Some(data) = instances::picker_of_mut(&mut entry.payload) else {
            continue;
        };
        let Some(owner) = data.requester.as_ref().and_then(|r| r.owner_popup_instance) else {
            continue;
        };
        if !open || !closed.iter().any(|(closed, _)| *closed == owner) {
            continue;
        }
        if data.result.is_none() {
            data.result = Some(FilePickerResult::Cancelled);
            // 취소 정리로 부모가 사라진 경우는 늦은 결과 경고를 내지 않는다.
            // 사용자가 이미 확정한 결과의 부모 정보는 남겨 경고로 확인할 수 있게 한다.
            if let Some(req) = data.requester.as_mut() {
                req.owner_popup_instance = None;
            }
        }
        children.push(iid);
    }
    // A hidden child never runs its draw callback. Close the shell here; the close
    // hook hands its settled result to the existing exactly-once drain.
    for iid in children {
        // intent-exempt: parent-close lifecycle.
        state.popups.close_instance(
            iid,
            crate::adapters::ui::popup::PopupCloseReason::ParentClosed,
        );
    }
}

/// 닫기 요청을 모든 상태의 닫기 큐에 넣는다. 어느 창이 자식 피커를 갖는지 몰라 모든 상태에 넣고
/// 비울 때 instance_id로 중복을 제거한다. 상태가 하나도 없으면 비울 곳이 없어 매니저를 직접 닫는다.
pub(crate) fn enqueue_popup_closes<'a>(
    sessions: impl Iterator<Item = &'a mut MainViewState>,
    mgr: Option<&mut crate::plugin::PluginManager>,
    closes: &[(u64, tasty_plugin_protocol::PopupCloseReason)],
) {
    let mut queued = false;
    for s in sessions {
        s.plugin_popup_closes.extend_from_slice(closes);
        queued = true;
    }
    if !queued && let Some(mgr) = mgr {
        for (instance_id, reason) in closes {
            mgr.close_popup_instance(*instance_id, *reason);
        }
    }
}

/// 버리는 창 state 하나를 그 자리에서 정리한다. 팝업은 범위 노드인 창의 자식이라 창과 함께 닫힌다.
/// 결속한 창 범위 plugin 팝업을 사유 `HostShutdown`으로 닫기 큐에 넣고, 큐 처리를 이 state 하나에 바로 적용한다
/// (자식 피커 취소와 매니저 닫기). 남은 창의 큐로 미루면 같은 회차의 다시 열기가 곧 닫힐 인스턴스를 받는다.
/// 이 state는 다시 그려지지 않으므로 닫힌 피커의 결과를 닫기 훅 대신 여기서 꺼내 요청한 plugin에 보낸다.
/// 본체 호출자의 결과는 이 state와 함께 버려지므로 보내지 않는다.
pub(crate) fn close_retired_window(
    retired: &mut MainViewState,
    mut mgr: Option<&mut crate::plugin::PluginManager>,
) {
    let closes = std::mem::take(&mut retired.plugin_window_popups)
        .into_iter()
        // plugin에 보내는 사유는 릴리스된 plugin이 아는 네 값(OutsideClick·Escape·PluginRequest·
        // HostShutdown) 안에서 고른다. 모르는 값을 받은 plugin은 popup.closed를 거절해 인스턴스 정리를 놓친다.
        .map(|id| (id, tasty_plugin_protocol::PopupCloseReason::HostShutdown));
    retired.plugin_popup_closes.extend(closes);
    drain_plugin_popup_events(std::iter::once(&mut *retired), mgr.as_deref_mut());
    for closed in retired.popups.take_closed_queue() {
        crate::adapters::ui::popup::file_picker::instances::finish(retired, &closed);
    }
    for (outcome, requester) in crate::app::dispatch::file_picker::take_outcomes(retired) {
        if let Some(req) = requester {
            crate::adapters::ipc::handler::file_picker::deliver_result(
                mgr.as_deref_mut(),
                &req,
                &outcome,
            );
        }
    }
}

/// 상태마다 쌓인 popup·banner 닫기와 포커스 변경을 매니저에 전달한다.
pub(crate) fn drain_plugin_popup_events<'a>(
    sessions: impl Iterator<Item = &'a mut MainViewState>,
    mgr: Option<&mut crate::plugin::PluginManager>,
) {
    let mut drained_closes: Vec<(u64, tasty_plugin_protocol::PopupCloseReason)> = Vec::new();
    let mut drained_banner_closes: Vec<(u64, tasty_plugin_protocol::BannerCloseReason)> =
        Vec::new();
    let mut drained_focus_bumps: Vec<u64> = Vec::new();
    for s in sessions {
        let closes = std::mem::take(&mut s.plugin_popup_closes);
        cancel_child_file_picker(s, &closes);
        drained_closes.extend(closes);
        drained_banner_closes.append(&mut s.plugin_banner_closes);
        drained_focus_bumps.append(&mut s.plugin_popup_focus_bumps);
    }
    if drained_closes.is_empty()
        && drained_banner_closes.is_empty()
        && drained_focus_bumps.is_empty()
    {
        return;
    }
    let Some(mgr) = mgr else {
        return;
    };
    // 같은 인스턴스는 첫 닫기 사유만 처리한다.
    let mut seen = std::collections::HashSet::new();
    for (instance_id, reason) in drained_closes {
        if seen.insert(instance_id) {
            mgr.close_popup_instance(instance_id, reason);
        }
    }
    let mut seen_banner = std::collections::HashSet::new();
    for (instance_id, reason) in drained_banner_closes {
        if seen_banner.insert(instance_id) {
            mgr.close_banner_instance(instance_id, reason);
        }
    }
    for instance_id in drained_focus_bumps {
        mgr.touch_popup_instance_z(instance_id);
    }
}

impl App {
    /// 렌더·IPC·debug 닫기를 같은 큐로 보내 자식 피커도 정리한다.
    pub(crate) fn enqueue_plugin_popup_close(
        &mut self,
        instance_id: u64,
        reason: tasty_plugin_protocol::PopupCloseReason,
    ) {
        enqueue_popup_closes(
            crate::app::window_access::engines_mut!(self)
                .sessions()
                .map(|(s, _)| s),
            self.plugin_manager.as_mut(),
            &[(instance_id, reason)],
        );
    }

    /// 버리는 창의 plugin 팝업과 자식 피커를 그 자리에서 닫는다. 창은 이미 창 목록에서 빠져 있다.
    pub(crate) fn close_retired_window_popups(&mut self, retired: &mut MainViewState) {
        close_retired_window(retired, self.plugin_manager.as_mut());
    }

    pub(crate) fn dispatch_plugin_popup_events(&mut self) {
        drain_plugin_popup_events(
            crate::app::window_access::engines_mut!(self)
                .sessions()
                .map(|(s, _)| s),
            self.plugin_manager.as_mut(),
        );
    }
}
