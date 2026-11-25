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

/// 단계 11 맨 앞의 키 사건 하나: 설정 열기 요청 래치 끝(`latch`, 누름만 본다)과 더블탭 검출기 공급. 메인 창과 별도 창
/// (`judged_event::window_tap_front`)이 이 한 자리로 검출기를 공급하고, 단계 4 수신 노드와 관계없이 모든 키 사건이
/// 온다. 검출한 더블탭의 동작은 그 사건의 경로 level 로 거른다. 뗌이거나 래치 노드가 받은 누름이면 참이고 그 사건은
/// 뒤 단계로 가지 않는다. 래치 노드가 받은 누름은 검출기에 가지 않고 진행 중인 탭을 버린다: 래치 안의 탭이 래치
/// 밖에서 더블탭이 되지 않는다. 래치 노드는 메인 창에만 있다. `at` 은 그 사건이 창에 도착한 시각이다.
pub(crate) fn key_ends_at_latch(
    detector: &mut crate::double_tap::DoubleTapDetector,
    key: &winit::keyboard::Key,
    pressed: bool,
    at: std::time::Instant,
    latch: impl FnOnce() -> bool,
) -> bool {
    if pressed && latch() {
        detector.reset();
        return true;
    }
    detector.on_key_event(key, pressed, at);
    !pressed
}
