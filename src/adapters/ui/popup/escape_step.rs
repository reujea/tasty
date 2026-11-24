//! 팝업 층 Esc 의 프레임 시점 단계(중앙 단계). 창은 사건 시점에 수식키 없는 Esc 만 받고 host 키 주인의 포커스를
//! 풀어 그 Esc 를 넘긴다(`state::escape_gate`). 본문을 그린 뒤 이 단계가 선언 `escape` 와 본문 결과로 마무리한다.
//! 본문이 그 Esc 를 썼으면 닫지 않는다. 썼다는 것은 셋 가운데 하나다: (a) 본문이 키 통로로 Esc 를 소비했다, (b) 본문
//! 결과가 `PopupAction::None` 이 아니다, (c) 본문을 그리기 전에 이 인스턴스의 자식 팝업이 열려 있었다(egui 가 그 자식을
//! 닫는다). 쓰지 않았고 선언이 Close 면 사유 Escape 로 닫는다. Unfocus 는 사건 시점 해제로 끝이다. 펼친 host 팝업은
//! 창이 포커스를 풀지 않고 넘기며, 쓰지 않았으면 선언과 무관하게 그 Esc 에 묶인 인스턴스를 다시 접는다(닫지 않는다).
//!
//! 별도 창도 같다. 그 창의 사건 입구가 bare Esc 의 수신 노드를 사건 시점에 정해 넘기고
//! (`PopupManager::apply_window_escape`), 관리자를 그리는 함수가 같은 넘김 기록을 꺼낸다.

use super::keys::{BasicKey, ConsumedKey};
use super::{PopupAction, PopupCloseReason, PopupEscape};

/// 본문이 이 패스 Esc 를 썼는지((a)(b)(c)).
fn content_used_escape(consumed: &[ConsumedKey], action: &PopupAction, child_open: bool) -> bool {
    consumed.contains(&ConsumedKey::Basic(BasicKey::Escape))
        || !matches!(action, PopupAction::None)
        || child_open
}

/// 이 패스 Esc 를 받은 키 주인의 마무리. 본문이 쓰지 않았고 선언이 Close 면 닫는 결과를 돌려준다.
pub(super) fn central_close(
    escape: PopupEscape,
    consumed: &[ConsumedKey],
    action: &PopupAction,
    child_open: bool,
) -> Option<PopupAction> {
    (escape == PopupEscape::Close && !content_used_escape(consumed, action, child_open))
        .then_some(PopupAction::CloseBy(PopupCloseReason::Escape))
}

/// 이 패스 Esc 를 받은 펼친 키 주인의 마무리. 본문이 쓰지 않았으면 다시 접는다(닫지 않는다).
pub(super) fn central_refold(
    consumed: &[ConsumedKey],
    action: &PopupAction,
    child_open: bool,
) -> bool {
    !content_used_escape(consumed, action, child_open)
}
