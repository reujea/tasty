//! OS 창 테두리 리사이즈가 가져간 버튼 사건의 기록. 장식 없는 창(메인·Linux 설정창)이 함께 쓴다.

#[cfg(not(target_os = "macos"))]
use winit::event::ElementState;
use winit::event::MouseButton;

/// OS 테두리 리사이즈가 가져간 버튼 사건이면 참이다. 누름은 `begin`(테두리 리사이즈를 시작했는지)으로 정하고 그
/// 버튼을 적는다. 뗌은 적힌 버튼이면 꺼내고 참이다. OS 가 리사이즈 중 뗌을 삼켜 오지 않았으면 같은 버튼의 다음
/// 누름이 그 기록을 지워, 그 누름의 뗌을 잘못 삼키지 않는다. 메뉴를 닫는 클릭으로 삼킨 사건(`swallowed`)은
/// 리사이즈를 시작하지 않고 거짓이지만, 같은 버튼의 기록은 똑같이 지운다.
#[cfg(not(target_os = "macos"))]
pub(crate) fn os_resize_owns(
    owned: &mut Vec<MouseButton>,
    button_state: ElementState,
    button: MouseButton,
    swallowed: bool,
    begin: impl FnOnce() -> bool,
) -> bool {
    match button_state {
        ElementState::Pressed => {
            take_blocked_button(owned, button);
            let began = !swallowed && begin();
            if began {
                owned.push(button);
            }
            began
        }
        ElementState::Released => take_blocked_button(owned, button) && !swallowed,
    }
}

/// 위에서 가져간 press 의 버튼이면 목록에서 빼고 참이다. 그 release 도 같은 곳이 받는다.
pub(crate) fn take_blocked_button(blocked: &mut Vec<MouseButton>, button: MouseButton) -> bool {
    let Some(i) = blocked.iter().position(|b| *b == button) else {
        return false;
    };
    blocked.swap_remove(i);
    true
}

/// OS 테두리 리사이즈가 가져간 누름의 뗌을 함께 가져가는지 확인한다.
#[cfg(all(test, not(target_os = "macos")))]
mod os_resize_button_tests {
    use super::os_resize_owns;
    use winit::event::{ElementState, MouseButton};

    /// Given 테두리에서 왼쪽을 눌러 리사이즈를 시작했을 때, When 왼쪽을 떼면, Then 그 뗌도 리사이즈 것이라
    /// 아래(egui·팝업·surface)로 가지 않는다. When 테두리가 아닌 곳의 누름·뗌이면, Then 아래로 간다.
    #[test]
    fn the_release_of_a_border_press_stays_with_the_resize() {
        let mut owned = Vec::new();
        let left = MouseButton::Left;
        assert!(os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            false,
            || true
        ));
        assert!(os_resize_owns(
            &mut owned,
            ElementState::Released,
            left,
            false,
            || { unreachable!("뗌은 리사이즈를 시작하지 않는다") }
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            false,
            || false
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Released,
            left,
            false,
            || false
        ));
    }

    /// Given OS 가 리사이즈 중 뗌을 삼켜 기록이 남았을 때, When 같은 버튼의 누름·뗌을 메뉴 닫기 클릭으로 삼킨 뒤 앱이
    /// 누름을 보지 못한 그 버튼의 뗌이 오면, Then 그 뗌은 옛 리사이즈 것이 아니다. 삼킨 사건 자체도 리사이즈 것이
    /// 아니다(창 경로와 debug 주입 경로가 같은 함수를 쓴다).
    #[test]
    fn a_menu_dismiss_click_clears_a_lost_resize_release() {
        let mut owned = Vec::new();
        let left = MouseButton::Left;
        assert!(os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            false,
            || true
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            true,
            || unreachable!("삼킨 누름은 리사이즈를 시작하지 않는다")
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Released,
            left,
            true,
            || false
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Released,
            left,
            false,
            || false
        ));
    }

    /// Given OS 가 리사이즈 중 뗌을 삼켜 기록이 남았을 때, When 같은 버튼을 테두리 밖에서 다시 누르면, Then 기록이
    /// 지워져 그 누름의 뗌은 아래로 간다.
    #[test]
    fn a_lost_release_does_not_swallow_the_next_one() {
        let mut owned = Vec::new();
        let left = MouseButton::Left;
        assert!(os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            false,
            || true
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Pressed,
            left,
            false,
            || false
        ));
        assert!(!os_resize_owns(
            &mut owned,
            ElementState::Released,
            left,
            false,
            || false
        ));
    }
}

/// 위에서 가져간 press 의 버튼 목록이 그 release 를 한 번만 가져가는지 확인한다.
#[cfg(test)]
mod blocked_button_tests {
    use super::take_blocked_button;
    use winit::event::MouseButton;

    /// Given 위(팝업·egui)가 왼쪽과 오른쪽 press를 가져갔을 때, When 오른쪽을 떼면, Then 그 release만 위가
    /// 받고 왼쪽은 남는다. When 가져가지 않은 가운데를 떼면, Then 아래로 간다.
    #[test]
    fn a_release_follows_the_press_its_button_was_taken_by() {
        let mut blocked = vec![MouseButton::Left, MouseButton::Right];
        assert!(take_blocked_button(&mut blocked, MouseButton::Right));
        assert_eq!(blocked, [MouseButton::Left]);
        assert!(!take_blocked_button(&mut blocked, MouseButton::Middle));
        assert!(take_blocked_button(&mut blocked, MouseButton::Left));
        assert!(blocked.is_empty());
    }
}
