//! 정의의 그리기 함수를 쓰지 않는 창(View)의 팝업 그리기. 설정 창과 파일 선택 창처럼
//! `MainViewState`가 없는 창은 자기 `PopupManager`에 정의를 등록하고 콘텐츠를 직접 그린다.

use super::{PopupAction, PopupCx, PopupId, PopupManager};
use crate::state::MainViewState;

/// 창이 콘텐츠를 직접 그리는 정의의 `draw_fn`. 정의 경로로 그리는 일이 없으므로 아무것도 그리지 않는다.
pub fn drawn_by_view(
    _ui: &mut egui::Ui,
    _state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _cx: &mut PopupCx<'_>,
) -> PopupAction {
    PopupAction::None
}

impl PopupManager {
    /// 창의 팝업을 그린다. `content`의 세 번째 인자는 그 인스턴스가 Esc 주인(이 프레임을 시작할 때
    /// 열린 것 중 맨 위)인지이고, 참을 돌려주면 그리기가 끝난 뒤 그 인스턴스를 닫는다.
    /// 이 프레임에 닫힌 팝업(셸에서 닫힘, 콘텐츠가 닫음, 그 밖의 닫기 대기열)의 id를 돌려준다.
    pub fn draw_view_local(
        &mut self,
        ctx: &egui::Context,
        content: &mut dyn FnMut(&mut PopupCx<'_>, &mut egui::Ui, bool) -> bool,
    ) -> Vec<PopupId> {
        let escape_owner = self.escape_owner();
        let mut done = Vec::new();
        let result = self.draw(
            ctx,
            &mut |cx, ui| {
                if content(cx, ui, Some(cx.instance) == escape_owner) {
                    done.push(cx.instance);
                }
            },
            None,
            // 창의 자체 관리자다. 플러그인 팝업과 겹치지 않는다.
            &[],
        );
        for iid in done {
            self.close_instance(iid, tasty_plugin_protocol::PopupCloseReason::Command);
        }
        let mut closed = result.closed;
        closed.extend(self.take_closed_ids());
        closed
    }

    /// Esc를 받을 인스턴스. 열린 것 중 z 순서가 가장 위인 것이다.
    pub fn escape_owner(&self) -> Option<super::PopupInstanceId> {
        self.topmost_visible_open(None).map(|(_, iid, _)| iid)
    }
}
