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

impl App {
    /// 렌더·IPC·debug 닫기를 같은 큐로 보내 자식 피커도 정리한다.
    /// 어느 창이 자식을 갖는지 몰라 모든 상태에 넣고 instance_id로 중복을 제거한다.
    pub(crate) fn enqueue_plugin_popup_close(
        &mut self,
        instance_id: u64,
        reason: tasty_plugin_protocol::PopupCloseReason,
    ) {
        let mut queued = false;
        for (s, _engine) in self.engines_mut().sessions() {
            s.plugin_popup_closes.push((instance_id, reason));
            queued = true;
        }
        // 상태가 하나도 없으면 큐를 비울 곳도 없어 매니저를 직접 닫는다.
        if !queued && let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.close_popup_instance(instance_id, reason);
        }
    }

    pub(crate) fn dispatch_plugin_popup_events(&mut self) {
        let mut drained_closes: Vec<(u64, tasty_plugin_protocol::PopupCloseReason)> = Vec::new();
        let mut drained_banner_closes: Vec<(u64, tasty_plugin_protocol::BannerCloseReason)> =
            Vec::new();
        let mut drained_focus_bumps: Vec<u64> = Vec::new();
        for (s, _engine) in self.engines_mut().sessions() {
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
        let Some(mgr) = self.plugin_manager.as_mut() else {
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
}
