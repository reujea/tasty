//! 정의의 그리기 함수를 쓰지 않는 창(View)의 팝업 그리기. 설정 창과 파일 선택 창처럼
//! `MainViewState`가 없는 창은 자기 `PopupManager`에 정의를 등록하고 콘텐츠를 직접 그린다.

use super::{ClosedPopup, PopupAction, PopupCx, PopupId, PopupManager};
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
    /// 창의 팝업을 그린다. `content`의 세 번째 인자는 그 인스턴스가 이번 패스 키 주인(창이 넣은 W1 값, 넣지
    /// 않았으면 포커스 인스턴스)인지이고, 참을 돌려주면 그리기가 끝난 뒤 그 인스턴스를 닫는다.
    /// 이 프레임에 닫힌 팝업(셸에서 닫힘, 콘텐츠가 닫음, 중앙 단계가 선언대로 닫음, 그 밖의 닫기 대기열)을 사유와
    /// 함께 돌려준다. 모든 닫기는 닫기 큐를 지나므로 그 큐 하나를 꺼낸다.
    pub fn draw_view_local(
        &mut self,
        ctx: &egui::Context,
        content: &mut dyn FnMut(&mut PopupCx<'_>, &mut egui::Ui, bool) -> bool,
    ) -> ViewClosed {
        let key_owner = self.pass_key_host();
        let mut done = Vec::new();
        let result = self.draw(
            ctx,
            &mut |cx, ui| {
                if content(cx, ui, Some(cx.instance) == key_owner) {
                    // 그 패스에 남은 Esc 는 끝낸 콘텐츠가 쓴 것이다. 중앙 단계가 Escape 로 다시 닫지 않는다.
                    cx.keys.consume(super::keys::BasicKey::Escape);
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
        // 중앙 단계가 선언대로 닫은 인스턴스.
        for (iid, action) in result.actions {
            match action {
                PopupAction::Close => {
                    self.close_instance(iid, tasty_plugin_protocol::PopupCloseReason::Content)
                }
                PopupAction::CloseBy(reason) => self.close_instance(iid, reason),
                PopupAction::ReleaseFocus | PopupAction::None => {}
            }
        }
        ViewClosed(self.take_closed_queue())
    }
}

/// 창이 그린 프레임에 닫힌 팝업. 닫기 사유를 버리지 않는다.
#[derive(Debug)]
pub struct ViewClosed(Vec<ClosedPopup>);

impl ViewClosed {
    /// 선언 `id`의 인스턴스가 이 프레임에 닫혔는지.
    pub fn contains(&self, id: &PopupId) -> bool {
        self.0.iter().any(|c| c.id == *id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 선언 `id`의 인스턴스가 닫힌 사유들(닫힌 순서).
    #[cfg(test)]
    pub fn reasons_of(&self, id: &PopupId) -> Vec<super::PopupCloseReason> {
        self.0
            .iter()
            .filter(|c| c.id == *id)
            .map(|c| c.reason)
            .collect()
    }
}
