//! 창 사건 하나가 지나는 단계 순서(계약 E0). 메인 창과 자체 키 처리를 가진 별도 창이 이 함수 하나로 사건을
//! 넘긴다. 창은 가진 단계만 구현하고 순서는 여기서만 정한다. 단계 번호와 창별 범위는
//! `docs/architecture/input-layer.md`의 "키 사건 하나의 순서"에 있다.

use super::ViewAction;
use crate::adapters::ui::input::edit_key::EditKey;

/// 창 하나가 가진 단계. 갖지 않은 단계는 기본 구현(아무것도 하지 않고 지나감)을 쓴다.
pub(crate) trait KeyEventSteps<E> {
    /// 단계 0·1(끌어 내보내기, modal 차단). 참이면 사건이 여기서 끝난다.
    fn ends_before_edit_judgment(&mut self, _event: &E) -> bool {
        false
    }

    /// 단계 3·5(포커스 정규화, 입력 몫 끊기).
    fn settle_key_node(&mut self, _event: &E) {}

    /// 단계 6(편집 판정). `None`이면 판정이 사건을 가져갔다. 값은 판정이 넘긴 편집이다.
    fn judge_edit_key(&mut self, event: &E) -> Option<Option<EditKey>>;

    /// 단계 7(explorer IME·튜토리얼 가로채기). 참이면 사건이 여기서 끝난다.
    fn intercepts_after_edit_judgment(&mut self, _event: &E) -> bool {
        false
    }

    /// 단계 8(egui 적재). 값은 (소비, 다시 그리기)다. `None`이면 적재 앞의 판정이 사건을 가져갔다.
    fn feed_egui(&mut self, event: &E, judged: Option<EditKey>) -> Option<(bool, bool)>;

    /// 단계 11(수신자 실행). 창 자체 처리와 단축키·목적지가 여기 있다.
    fn run_receiver(&mut self, event: E, judged: Option<EditKey>, egui: (bool, bool))
    -> ViewAction;
}

/// 사건 하나를 단계 순서대로 넘긴다. 앞 단계가 끝낸 사건은 뒤 단계에 가지 않는다.
pub(crate) fn run_key_event<E>(steps: &mut impl KeyEventSteps<E>, event: E) -> ViewAction {
    if steps.ends_before_edit_judgment(&event) {
        return ViewAction::None;
    }
    steps.settle_key_node(&event);
    let Some(judged) = steps.judge_edit_key(&event) else {
        return ViewAction::None;
    };
    if steps.intercepts_after_edit_judgment(&event) {
        return ViewAction::None;
    }
    let Some(egui) = steps.feed_egui(&event, judged) else {
        return ViewAction::None;
    };
    steps.run_receiver(event, judged, egui)
}
