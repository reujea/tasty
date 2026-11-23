//! Linux 설정창의 앱 제목 줄과 창 가장자리 입력. OS 장식을 끈 창이라 이동·크기 조절·닫기를 앱이 맡는다.
//! 컴포지터가 서버 장식을 고르면(Wayland) 제목 줄과 가장자리 입력을 모두 OS에 맡긴다.

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::window::{ResizeDirection, Window};

use crate::adapters::ui::titlebar::{self, TitlebarAction, TitlebarProps};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::platform::window_chrome::{RESIZE_EDGE_MARGIN, resize_direction_at};

/// 설정창 한 개의 제목 줄·가장자리 상태.
pub(crate) struct SettingsChrome {
    /// 마지막 포인터 위치(물리). 창 밖이면 없다.
    cursor: Option<PhysicalPosition<f64>>,
    /// 직전 패스에 그린 닫기 버튼 영역(물리). 창 크기·배율이 바뀌면 다음 패스가 다시 적을 때까지 없다.
    close_rect: Option<PhysicalRect>,
    /// OS 테두리 리사이즈가 가져간 누름의 버튼. 그 뗌도 egui에 넣지 않는다.
    resize_buttons: Vec<MouseButton>,
    /// 컴포지터가 서버 장식을 골랐는지(`Window::is_decorated`). X11은 요청값이라 늘 거짓이다.
    server_decorated: bool,
}

/// 제목 줄 동작 가운데 창에 요청할 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowRequest {
    Drag,
    ToggleMaximize,
    Minimize,
}

impl SettingsChrome {
    pub(crate) fn new(window: &Window) -> Self {
        Self {
            cursor: None,
            close_rect: None,
            resize_buttons: Vec::new(),
            server_decorated: window.is_decorated(),
        }
    }

    /// 장식 협상 결과를 다시 읽는다. 장식 모드만 바뀐 configure는 Resized 없이 다시 그리기만 요청하므로
    /// 다시 그리기 사건마다 부른다. 바뀌었으면 참이고 호출자가 화면을 다시 그린다.
    pub(crate) fn sync_decorations(&mut self, window: &Window) -> bool {
        let now = window.is_decorated();
        let changed = now != self.server_decorated;
        self.server_decorated = now;
        if changed {
            self.close_rect = None;
        }
        changed
    }

    /// 편집 키 판정과 egui 앞에서 살아 있는 창 사건을 본다. 테두리 리사이즈가 가져간 누름과 그 짝 뗌이면 참이고,
    /// 그 사건은 egui에 넣지 않는다. 편집 키가 사건을 보류하는 동안의 누름은 나중에 재생되어 OS가 요구하는
    /// 실제 누름 시점과 어긋나므로 리사이즈를 시작하지 않는다.
    pub(crate) fn take_live_event(
        &mut self,
        event: &WindowEvent,
        window: &Window,
        size: PhysicalSize<u32>,
        edit_keys_busy: bool,
    ) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(*position);
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                false
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.close_rect = None;
                false
            }
            WindowEvent::MouseInput { state, button, .. } => self.owns_button(
                *state,
                *button,
                edit_keys_busy,
                size_is_locked(window),
                size,
                |dir| {
                    if let Err(e) = window.drag_resize_window(dir) {
                        tracing::warn!("settings window resize drag failed: {e}");
                    }
                },
            ),
            _ => false,
        }
    }

    /// 버튼 사건이 테두리 리사이즈 것인지. 좌버튼 누름이 가장자리면 `start`로 리사이즈를 시작하고 그 버튼을 적는다.
    /// 적은 버튼의 뗌도 리사이즈 것이다(`os_resize_owns`).
    fn owns_button(
        &mut self,
        state: ElementState,
        button: MouseButton,
        edit_keys_busy: bool,
        locked: bool,
        size: PhysicalSize<u32>,
        start: impl FnOnce(ResizeDirection),
    ) -> bool {
        let dir =
            (state == ElementState::Pressed && button == MouseButton::Left && !edit_keys_busy)
                .then(|| self.edge_direction(size, locked))
                .flatten();
        crate::view::os_resize_press::os_resize_owns(
            &mut self.resize_buttons,
            state,
            button,
            false,
            || match dir {
                Some(dir) => {
                    start(dir);
                    true
                }
                None => false,
            },
        )
    }

    /// 지금 포인터 위치의 테두리 리사이즈 방향. 서버 장식·최대화·전체화면이거나 닫기 버튼 위면 없다.
    fn edge_direction(&self, size: PhysicalSize<u32>, locked: bool) -> Option<ResizeDirection> {
        edge_direction(
            self.cursor,
            self.close_rect,
            self.server_decorated || locked,
            size,
        )
    }

    /// 설정 패널보다 먼저 제목 줄을 그리고 이동·최대화 요청을 창에 보낸다. 닫기 버튼을 눌렀으면 참이다.
    pub(crate) fn draw_titlebar(&mut self, ctx: &egui::Context, window: &Window) -> bool {
        if self.server_decorated {
            self.close_rect = None;
            return false;
        }
        let th = crate::theme::theme();
        let props = TitlebarProps {
            theme: &th,
            active: window.has_focus(),
            height: th.titlebar_height.value(),
            left_inset: 0.0,
            controls: Some(titlebar::close_only_controls()),
            maximized: window.is_maximized(),
        };
        let result = titlebar::draw_titlebar_view(ctx, &props);
        self.close_rect = result
            .close_rect
            .map(|rect| titlebar::physical_panel_rect(rect, ctx.pixels_per_point()));
        let (requests, close) = route_actions(&result.actions);
        for request in requests {
            match request {
                WindowRequest::Drag => {
                    // 창 이동은 마우스를 누른 상태에서 시작해야 한다. 실패하면 로그를 남긴다.
                    if let Err(e) = window.drag_window() {
                        tracing::warn!("settings titlebar drag_window failed: {e}");
                    }
                }
                WindowRequest::ToggleMaximize => window.set_maximized(!window.is_maximized()),
                WindowRequest::Minimize => window.set_minimized(true),
            }
        }
        close
    }

    /// 패스 끝에 가장자리 리사이즈 커서를 덮는다. egui 위젯이 그 패스에 정한 커서보다 나중이어야 한다.
    pub(crate) fn apply_resize_cursor(&self, ctx: &egui::Context, window: &Window) {
        let size = window.inner_size();
        if let Some(dir) = self.edge_direction(size, size_is_locked(window)) {
            ctx.set_cursor_icon(titlebar::resize_cursor(dir));
        }
    }

    /// 저널이 멈춘 동안의 창 사건. 그 동안 설정창은 다시 그리지 않으므로 직전 패스의 닫기 버튼 영역에서
    /// 좌버튼을 누를 때만 참이고, 호출자가 설정 모달을 닫는다(초안 폐기). 다른 사건은 버린다.
    pub(crate) fn halted_close_press(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(*position);
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                false
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => !self.server_decorated && cursor_in(self.cursor, self.close_rect),
            _ => false,
        }
    }
}

fn size_is_locked(window: &Window) -> bool {
    crate::view::main::window_size_is_locked(window)
}

fn cursor_in(cursor: Option<PhysicalPosition<f64>>, rect: Option<PhysicalRect>) -> bool {
    match (cursor, rect) {
        (Some(pos), Some(rect)) => {
            rect.contains(PhysicalPx(pos.x as f32), PhysicalPx(pos.y as f32))
        }
        _ => false,
    }
}

/// 테두리 리사이즈 방향. 크기를 바꿀 수 없거나(`blocked`) 닫기 버튼 위면 없다.
fn edge_direction(
    cursor: Option<PhysicalPosition<f64>>,
    close_rect: Option<PhysicalRect>,
    blocked: bool,
    size: PhysicalSize<u32>,
) -> Option<ResizeDirection> {
    if blocked || cursor_in(cursor, close_rect) {
        return None;
    }
    let pos = cursor?;
    resize_direction_at(
        pos.x,
        pos.y,
        f64::from(size.width),
        f64::from(size.height),
        RESIZE_EDGE_MARGIN,
    )
}

/// 제목 줄 동작을 창 요청과 닫기 여부로 나눈다. 닫기는 메인 창 닫기가 아니라 설정 모달 닫기다.
fn route_actions(actions: &[TitlebarAction]) -> (Vec<WindowRequest>, bool) {
    let mut requests = Vec::new();
    let mut close = false;
    for action in actions {
        match action {
            TitlebarAction::StartDrag => requests.push(WindowRequest::Drag),
            TitlebarAction::ToggleMaximize => requests.push(WindowRequest::ToggleMaximize),
            TitlebarAction::Minimize => requests.push(WindowRequest::Minimize),
            TitlebarAction::Close => close = true,
        }
    }
    (requests, close)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: PhysicalSize<u32> = PhysicalSize::new(1100, 700);

    fn close_rect() -> Option<PhysicalRect> {
        Some(PhysicalRect {
            x: PhysicalPx(1070.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(30.0),
            height: PhysicalPx(30.0),
        })
    }

    fn at(x: f64, y: f64) -> Option<PhysicalPosition<f64>> {
        Some(PhysicalPosition::new(x, y))
    }

    /// Given 제목 줄의 닫기 버튼을 눌렀을 때, When 동작을 나누면, Then 창 요청 없이 닫기만 남는다.
    /// 이동·최대화·최소화는 창 요청으로 가고 닫기가 아니다.
    #[test]
    fn the_close_button_only_closes_the_settings_modal() {
        assert_eq!(route_actions(&[TitlebarAction::Close]), (vec![], true));
        assert_eq!(
            route_actions(&[
                TitlebarAction::StartDrag,
                TitlebarAction::ToggleMaximize,
                TitlebarAction::Minimize,
            ]),
            (
                vec![
                    WindowRequest::Drag,
                    WindowRequest::ToggleMaximize,
                    WindowRequest::Minimize
                ],
                false
            )
        );
    }

    /// Given 포인터가 창 가장자리에 있을 때 Then 리사이즈 방향이 있고, When 그 자리가 닫기 버튼 영역이거나
    /// 크기를 바꿀 수 없으면(서버 장식·최대화) Then 없다. 가운데나 창 밖이면 없다.
    #[test]
    fn the_edge_yields_to_the_close_button_and_to_a_locked_window() {
        assert_eq!(
            edge_direction(at(1099.0, 300.0), close_rect(), false, SIZE),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            edge_direction(at(1099.0, 1.0), None, false, SIZE),
            Some(ResizeDirection::NorthEast)
        );
        assert_eq!(
            edge_direction(at(1099.0, 1.0), close_rect(), false, SIZE),
            None
        );
        assert_eq!(
            edge_direction(at(1099.0, 300.0), close_rect(), true, SIZE),
            None
        );
        assert_eq!(edge_direction(at(500.0, 300.0), None, false, SIZE), None);
        assert_eq!(edge_direction(None, None, false, SIZE), None);
    }

    fn chrome(server_decorated: bool) -> SettingsChrome {
        SettingsChrome {
            cursor: None,
            close_rect: close_rect(),
            resize_buttons: Vec::new(),
            server_decorated,
        }
    }

    fn moved(x: f64, y: f64) -> WindowEvent {
        WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        }
    }

    fn left_press() -> WindowEvent {
        WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state: ElementState::Pressed,
            button: MouseButton::Left,
        }
    }

    fn release() -> (ElementState, MouseButton) {
        (ElementState::Released, MouseButton::Left)
    }

    /// Given 포인터가 오른쪽 가장자리일 때, When 좌버튼을 누르면 Then 리사이즈를 시작하고 그 누름과 짝 뗌을 가져간다.
    /// When 편집 키가 사건을 보류 중이거나, 서버 장식이거나, 닫기 버튼 위면 Then 시작하지 않고 누름·뗌 모두 egui로 간다.
    #[test]
    fn an_edge_press_starts_a_resize_unless_something_else_owns_it() {
        let press = (ElementState::Pressed, MouseButton::Left);
        let run = |c: &mut SettingsChrome, (s, b): (ElementState, MouseButton), busy: bool| {
            let mut started = None;
            let owned = c.owns_button(s, b, busy, false, SIZE, |d| started = Some(d));
            (owned, started)
        };

        let mut c = chrome(false);
        c.cursor = at(1099.0, 300.0);
        assert_eq!(
            run(&mut c, press, false),
            (true, Some(ResizeDirection::East))
        );
        assert_eq!(run(&mut c, release(), false), (true, None));

        assert_eq!(
            run(&mut c, press, true),
            (false, None),
            "held behind an edit key"
        );
        assert_eq!(run(&mut c, release(), false), (false, None));

        c.cursor = at(1080.0, 1.0);
        assert_eq!(
            run(&mut c, press, false),
            (false, None),
            "on the close button"
        );

        let mut c = chrome(true);
        c.cursor = at(1099.0, 300.0);
        assert_eq!(
            run(&mut c, press, false),
            (false, None),
            "server decorations"
        );
    }

    /// Given 저널이 멈췄을 때, When 닫기 버튼 영역에서 좌버튼을 누르면 Then 설정창을 닫는다. 영역 밖의 누름이나
    /// 서버 장식 창(앱 제목 줄이 없다)의 누름은 닫지 않는다.
    #[test]
    fn a_halted_press_closes_only_on_the_close_button() {
        let mut c = chrome(false);
        assert!(!c.halted_close_press(&moved(1080.0, 10.0)));
        assert!(c.halted_close_press(&left_press()));
        assert!(!c.halted_close_press(&moved(500.0, 10.0)));
        assert!(!c.halted_close_press(&left_press()));

        let mut c = chrome(true);
        assert!(!c.halted_close_press(&moved(1080.0, 10.0)));
        assert!(!c.halted_close_press(&left_press()));
    }
}
