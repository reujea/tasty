//! 타이틀바 위 좌버튼 누름을 사건 좌표로 가린다. egui 의 소비 판정(`wants_pointer_input`)은 직전 패스의 포인터
//! 위치를 보므로, 포인터를 옮기자마자 누른 타이틀바 버튼이 터미널 클릭(선택·클릭 커서 이동)으로도 처리됐다.
//! 규칙은 `docs/architecture/input-layer.md` 의 '타이틀바 좌클릭 press' 단락에 있다.

use super::MainView;
use crate::model::{PhysicalPx, PhysicalRect};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};

impl MainView {
    /// 이 버튼 사건이 직전 패스에 그린 타이틀바 위의 좌버튼 누름인지. 지금 포인터 좌표(물리)로 본다.
    pub(super) fn left_press_on_titlebar(&self, button: MouseButton, state: ElementState) -> bool {
        press_on_titlebar(
            button,
            state,
            self.cursor_position,
            self.state.titlebar_rect,
            || self.base.winit.inner_size(),
        )
    }
}

/// 직전 패스에 그린 타이틀바 패널 위의 좌버튼 누름인지. 좌표·영역은 물리 좌표이고 경계는 위·왼쪽 포함, 아래·오른쪽
/// 제외다(패널 아래 변은 작업 영역의 첫 줄). 창이 줄어든 뒤 첫 패스 전에는 저장된 영역이 창보다 넓을 수 있어 지금 창
/// 크기도 본다. 창 크기 조회는 X11 에서 서버 왕복이라 패널 안 누름일 때만 부른다.
fn press_on_titlebar(
    button: MouseButton,
    button_state: ElementState,
    pos: Option<PhysicalPosition<f64>>,
    titlebar: Option<PhysicalRect>,
    window_size: impl FnOnce() -> PhysicalSize<u32>,
) -> bool {
    if button != MouseButton::Left || button_state != ElementState::Pressed {
        return false;
    }
    let (Some(pos), Some(titlebar)) = (pos, titlebar) else {
        return false;
    };
    let (x, y) = (PhysicalPx(pos.x as f32), PhysicalPx(pos.y as f32));
    if !titlebar.contains(x, y) {
        return false;
    }
    let size = window_size();
    let window = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(size.width as f32),
        height: PhysicalPx(size.height as f32),
    };
    window.contains(x, y)
}

/// 직전 패스에 그린 타이틀바 위 좌버튼 누름을 사건 좌표로 가리는 판정을 확인한다.
#[cfg(test)]
mod titlebar_press_tests {
    use super::press_on_titlebar;
    use crate::model::{PhysicalPx, PhysicalRect};
    use std::cell::Cell;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::ElementState::{Pressed, Released};
    use winit::event::MouseButton;

    /// 창 1600x1000, 타이틀바는 위 36px.
    const BAR: PhysicalRect = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1600.0),
        height: PhysicalPx(36.0),
    };
    const WINDOW: PhysicalSize<u32> = PhysicalSize::new(1600, 1000);

    fn left_press_at(x: f64, y: f64) -> bool {
        press_on_titlebar(
            MouseButton::Left,
            Pressed,
            Some(PhysicalPosition::new(x, y)),
            Some(BAR),
            || WINDOW,
        )
    }

    /// Given 타이틀바 패널, When 그 안(닫기 버튼 자리 포함)에서 좌버튼을 누르면, Then 위에 남는다.
    /// 위·왼쪽 변은 패널에 든다.
    #[test]
    fn a_left_press_on_the_titlebar_stays_above() {
        assert!(left_press_at(1580.0, 18.0));
        assert!(left_press_at(0.0, 0.0));
        assert!(left_press_at(1599.0, 35.0));
    }

    /// 패널 아래 변은 작업 영역의 첫 줄이고 오른쪽 변은 창 밖이다. 패널 아래는 터미널이다.
    #[test]
    fn the_bottom_and_right_edges_and_below_are_not_the_titlebar() {
        assert!(!left_press_at(100.0, 36.0));
        assert!(!left_press_at(1600.0, 10.0));
        assert!(!left_press_at(100.0, 500.0));
    }

    /// Given 창이 줄어든 뒤 첫 패스 전이라 저장된 패널이 창보다 넓을 때, When 창 밖 좌표면, Then 가리지 않는다.
    #[test]
    fn a_press_outside_the_current_window_is_not_the_titlebar() {
        let narrowed = PhysicalSize::new(800, 1000);
        assert!(!press_on_titlebar(
            MouseButton::Left,
            Pressed,
            Some(PhysicalPosition::new(1200.0, 18.0)),
            Some(BAR),
            || narrowed,
        ));
        assert!(!left_press_at(-1.0, 18.0));
    }

    /// 좌버튼 누름만 본다. 뗌·다른 버튼, 위치나 패널을 모르는 경우는 가리지 않는다.
    #[test]
    fn only_a_known_left_press_on_a_drawn_titlebar_counts() {
        let pos = Some(PhysicalPosition::new(100.0, 18.0));
        for (button, state) in [
            (MouseButton::Left, Released),
            (MouseButton::Right, Pressed),
            (MouseButton::Middle, Pressed),
        ] {
            assert!(!press_on_titlebar(button, state, pos, Some(BAR), || WINDOW));
        }
        assert!(!press_on_titlebar(
            MouseButton::Left,
            Pressed,
            None,
            Some(BAR),
            || WINDOW
        ));
        assert!(!press_on_titlebar(
            MouseButton::Left,
            Pressed,
            pos,
            None,
            || WINDOW
        ));
    }

    /// 창 크기는 패널 안 좌버튼 누름일 때만 조회한다(X11 에서는 서버 왕복이다).
    #[test]
    fn the_window_size_is_read_only_for_a_candidate() {
        let reads = Cell::new(0);
        let size = || {
            reads.set(reads.get() + 1);
            WINDOW
        };
        let below = Some(PhysicalPosition::new(100.0, 500.0));
        assert!(!press_on_titlebar(
            MouseButton::Left,
            Pressed,
            below,
            Some(BAR),
            size
        ));
        assert_eq!(reads.get(), 0);
        let on_bar = Some(PhysicalPosition::new(100.0, 18.0));
        let size = || {
            reads.set(reads.get() + 1);
            WINDOW
        };
        assert!(press_on_titlebar(
            MouseButton::Left,
            Pressed,
            on_bar,
            Some(BAR),
            size
        ));
        assert_eq!(reads.get(), 1);
    }
}
