//! Linux 설정창의 앱 제목 줄과 창 가장자리 입력. OS 장식을 끈 창이라 이동·크기 조절·닫기를 앱이 맡는다.
//! 컴포지터가 서버 장식을 고르면(Wayland) 제목 줄과 가장자리 입력을 모두 OS에 맡긴다.

use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::window::{ResizeDirection, Window};

use crate::adapters::ui::titlebar::{self, TitlebarAction, TitlebarProps};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::platform::window_chrome::{RESIZE_EDGE_MARGIN, resize_direction_at};

/// 설정창 한 개의 제목 줄·가장자리 상태.
pub(crate) struct SettingsChrome {
    /// 마지막 포인터 위치(물리). 창 밖이면 없다.
    cursor: Option<PhysicalPosition<f64>>,
    /// 직전 패스에 그린 닫기 버튼 영역(물리)과 그때의 창 너비. 버튼은 오른쪽에 붙으므로 너비가 바뀌면 그만큼 옮겨 본다.
    close: Option<(PhysicalRect, u32)>,
    /// 직전 패스에서 가장자리 밴드와 겹친 egui 위젯(클릭·드래그를 받는 것)의 영역(물리). 누르는 순간의 포인터가
    /// 이 안이면 리사이즈가 양보해 스크롤바 같은 위젯이 누름을 받는다. 제목 줄의 빈 드래그 영역은 넣지 않는다.
    edge_widgets: Vec<PhysicalRect>,
    /// 창 크기·배율·장식이 바뀐 뒤 아직 새 패스가 영역을 다시 적지 않았다. 그동안은 리사이즈를 시작하지 않는다.
    areas_stale: bool,
    /// OS 테두리 리사이즈가 가져간 누름의 버튼. 그 뗌도 egui에 넣지 않는다.
    resize_buttons: Vec<MouseButton>,
    /// 마지막으로 그릴 때 컴포지터가 서버 장식을 골랐는지(`Window::is_decorated`). X11은 요청값이라 늘 거짓이다.
    drawn_server_decorated: bool,
    /// 마지막 누름이 저널 보류 뒤 재생된 사건이었다. 그 누름으로는 OS 창 이동을 시작하지 않는다.
    replayed_press: bool,
    /// 저널이 멈춘 동안 닫기 버튼 안에서 시작한 누름. 버튼 안에서 떼야 닫는다.
    halted_press: bool,
}

/// 살아 있는 창 사건 하나를 볼 때의 창 상태.
pub(crate) struct LiveInput {
    pub(crate) size: PhysicalSize<u32>,
    /// 편집 키가 사건을 보류 중이다(`GpuState::edit_keys_busy`).
    pub(crate) edit_keys_busy: bool,
    /// 저널이 보류했다가 재생한 사건이다.
    pub(crate) replayed: bool,
}

impl SettingsChrome {
    pub(crate) fn new(window: &Window) -> Self {
        Self {
            cursor: None,
            close: None,
            edge_widgets: Vec::new(),
            areas_stale: true,
            resize_buttons: Vec::new(),
            drawn_server_decorated: window.is_decorated(),
            replayed_press: false,
            halted_press: false,
        }
    }

    /// 장식 협상 결과가 마지막으로 그린 상태와 다른지. 장식 모드만 바뀐 configure는 Resized 없이 다시 그리기만
    /// 요청하므로 다시 그리기 사건마다 부르고, 참이면 호출자가 화면을 다시 그린다.
    pub(crate) fn decorations_changed(&mut self, window: &Window) -> bool {
        let changed = window.is_decorated() != self.drawn_server_decorated;
        if changed {
            self.areas_stale = true;
        }
        changed
    }

    /// 편집 키 판정과 egui 앞에서 창 사건을 본다. 테두리 리사이즈가 가져간 누름과 그 짝 뗌이면 참이고, 그 사건은
    /// egui에 넣지 않는다. OS 리사이즈는 버튼이 눌린 동안에만 시작할 수 있으므로 편집 키가 사건을 보류 중일 때와
    /// 저널이 재생한 사건으로는 시작하지 않는다. 편집 키 보류에는 Wayland 붙여넣기 응답을 기다리며 포인터 사건이
    /// 줄을 서지 않는 때도 들어 있다. 그 짧은 동안의 가장자리 누름도 리사이즈 대신 egui로 간다.
    pub(crate) fn take_event(
        &mut self,
        event: &WindowEvent,
        window: &Window,
        input: LiveInput,
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
            WindowEvent::Resized(_) => {
                self.areas_stale = true;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // 저장한 포인터 좌표는 옛 배율의 물리 좌표라 다음 이동까지 쓰지 않는다.
                self.cursor = None;
                self.areas_stale = true;
                false
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let can_start = !input.edit_keys_busy
                    && !input.replayed
                    && *state == ElementState::Pressed
                    // 장식 협상은 다시 그리기 전에도 바뀔 수 있어 누르는 순간의 값을 읽는다.
                    && !window.is_decorated()
                    && !size_is_locked(window);
                self.owns_button(*state, *button, can_start, input.size, |dir| {
                    if let Err(e) = window.drag_resize_window(dir) {
                        tracing::warn!("settings window resize drag failed: {e}");
                    }
                })
            }
            _ => false,
        }
    }

    /// 버튼 사건이 테두리 리사이즈 것인지. `can_start`인 좌버튼 누름이 가장자리면 `start`로 리사이즈를 시작하고
    /// 그 버튼을 적는다. 적은 버튼의 뗌도 리사이즈 것이다(`os_resize_owns`).
    fn owns_button(
        &mut self,
        state: ElementState,
        button: MouseButton,
        can_start: bool,
        size: PhysicalSize<u32>,
        start: impl FnOnce(ResizeDirection),
    ) -> bool {
        let dir = (can_start && button == MouseButton::Left)
            .then(|| self.edge_direction(size))
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

    /// 지금 포인터 위치의 테두리 리사이즈 방향. 영역을 다시 적기 전이거나 닫기 버튼·위젯 위면 없다.
    fn edge_direction(&self, size: PhysicalSize<u32>) -> Option<ResizeDirection> {
        if self.areas_stale || self.drawn_server_decorated {
            return None;
        }
        edge_direction(self.cursor, &self.yield_rects(size.width), size)
    }

    /// 리사이즈가 양보할 영역: 닫기 버튼과 가장자리 위젯.
    fn yield_rects(&self, width: u32) -> Vec<Option<PhysicalRect>> {
        std::iter::once(self.close_rect(width))
            .chain(self.edge_widgets.iter().copied().map(Some))
            .collect()
    }

    /// 좌버튼 누름의 출처를 적는다. 저널이나 편집 키가 재생한 누름이면 그 누름이 시작한 제목 줄 드래그로 OS 이동을
    /// 시작하지 않는다. 다른 버튼의 누름은 egui의 드래그 후보를 바꾸지 않으므로 적지 않는다.
    pub(crate) fn note_left_press(&mut self, replayed: bool) {
        self.replayed_press = replayed;
    }

    /// 지금 창 너비에 맞춘 닫기 버튼 영역.
    fn close_rect(&self, width: u32) -> Option<PhysicalRect> {
        self.close.map(|(rect, basis)| PhysicalRect {
            x: PhysicalPx(rect.x.value() + width as f32 - basis as f32),
            ..rect
        })
    }

    /// 설정 패널보다 먼저 제목 줄을 그리고 이동 요청을 창에 보낸다. 닫기 버튼을 눌렀으면 참이다.
    /// 컴포지터가 서버 장식을 골랐으면 그리지 않는다.
    pub(crate) fn draw_titlebar(
        &mut self,
        ctx: &egui::Context,
        window: &Window,
        size: PhysicalSize<u32>,
    ) -> bool {
        let server_decorated = window.is_decorated();
        if server_decorated != self.drawn_server_decorated {
            fit_window_height(window, server_decorated);
        }
        self.drawn_server_decorated = server_decorated;
        if self.drawn_server_decorated {
            self.close = None;
            return false;
        }
        let th = crate::theme::theme();
        let props = settings_props(&th, window.has_focus(), window.is_maximized());
        let result = titlebar::draw_titlebar_view(ctx, &props);
        self.close = result.close_rect.map(|rect| {
            (
                titlebar::physical_panel_rect(rect, ctx.pixels_per_point()),
                size.width,
            )
        });
        let routed = route_actions(&result.actions);
        // 재생된 누름이면 버튼이 이미 떨어졌을 수 있어 OS 이동을 시작하지 않는다.
        if routed.drag && !self.replayed_press {
            // 창 이동은 마우스를 누른 상태에서 시작해야 한다. 실패하면 로그를 남긴다.
            if let Err(e) = window.drag_window() {
                tracing::warn!("settings titlebar drag_window failed: {e}");
            }
        }
        routed.close
    }

    /// 패스 끝에 포인터 아래 위젯을 적고 가장자리 리사이즈 커서를 덮는다. egui 위젯이 그 패스에 정한 커서보다
    /// 나중이어야 한다.
    pub(crate) fn finish_pass(
        &mut self,
        ctx: &egui::Context,
        window: &Window,
        size: PhysicalSize<u32>,
    ) {
        let drag_area = egui::Id::new(titlebar::DRAG_AREA_ID);
        let ppp = ctx.pixels_per_point();
        let screen = ctx.screen_rect();
        let band = PhysicalPx(RESIZE_EDGE_MARGIN as f32)
            .to_logical(ppp)
            .value();
        let inner = screen.shrink(band);
        self.edge_widgets = ctx.viewport(|vp| {
            vp.this_pass
                .widgets
                .layers()
                .flat_map(|(_, widgets)| widgets.iter())
                .filter(|w| {
                    w.id != drag_area
                        && w.enabled
                        && (w.sense.senses_click() || w.sense.senses_drag())
                        && w.interact_rect.intersects(screen)
                        && !inner.contains_rect(w.interact_rect)
                })
                .map(|w| titlebar::physical_panel_rect(w.interact_rect, ppp))
                .collect()
        });
        self.areas_stale = false;
        if !size_is_locked(window)
            && let Some(dir) = self.edge_direction(size)
        {
            ctx.set_cursor_icon(titlebar::resize_cursor(dir));
        }
    }

    /// 저널이 멈춘 동안의 창 사건. 그 동안 설정창은 다시 그리지 않으므로 직전 패스의 닫기 버튼 영역에서 좌버튼을
    /// 눌렀다가 그 안에서 뗄 때만 참이고, 호출자가 설정 모달을 닫는다(초안 폐기). 다른 사건은 버린다.
    pub(crate) fn halted_close_click(&mut self, event: &WindowEvent, width: u32) -> bool {
        let on_close =
            |c: &Self| !c.drawn_server_decorated && cursor_in(c.cursor, c.close_rect(width));
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(*position);
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.halted_press = false;
                false
            }
            // 다시 그리지 않으므로 보이는 버튼과 저장한 영역이 어긋날 수 있다. 진행 중인 누름을 취소한다.
            WindowEvent::Resized(_) => {
                self.halted_press = false;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.halted_press = false;
                self.close = None;
                self.cursor = None;
                false
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    self.halted_press = on_close(self);
                    false
                }
                ElementState::Released => std::mem::take(&mut self.halted_press) && on_close(self),
            },
            _ => false,
        }
    }
}

/// 설정창 제목 줄의 화면 입력: 닫기 버튼 하나, 키보드 포커스 표시, 주 버튼 드래그만 창 이동.
fn settings_props(th: &crate::theme::Theme, active: bool, maximized: bool) -> TitlebarProps<'_> {
    TitlebarProps {
        theme: th,
        active,
        height: th.titlebar_height.value(),
        left_inset: 0.0,
        controls: Some(titlebar::close_only_controls()),
        maximized,
        keyboard_focus: true,
        primary_drag_only: true,
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

/// 테두리 리사이즈 방향. 창 밖 좌표이거나 양보할 영역(`yield_to`) 위면 없다.
fn edge_direction(
    cursor: Option<PhysicalPosition<f64>>,
    yield_to: &[Option<PhysicalRect>],
    size: PhysicalSize<u32>,
) -> Option<ResizeDirection> {
    let pos = cursor?;
    let (w, h) = (f64::from(size.width), f64::from(size.height));
    let inside = (0.0..w).contains(&pos.x) && (0.0..h).contains(&pos.y);
    if !inside || yield_to.iter().any(|rect| cursor_in(cursor, *rect)) {
        return None;
    }
    resize_direction_at(pos.x, pos.y, w, h, RESIZE_EDGE_MARGIN)
}

/// 장식 모드가 정해지거나 바뀌면 최소 크기를 그 모드의 값(`settings_window_size`)으로 바꾸고 현재 높이를
/// 제목 줄만큼 늘이거나 줄인다. 서버 장식이면 앱 제목 줄이 없다.
pub(crate) fn fit_window_height(window: &Window, server_decorated: bool) {
    use crate::app::modal::settings::settings_window_size;
    let min = settings_window_size(!server_decorated);
    let height = min.height;
    window.set_min_inner_size(Some(min));
    let current: LogicalSize<f32> = window.inner_size().to_logical(window.scale_factor());
    let delta = height - settings_window_size(server_decorated).height;
    let target = LogicalSize::new(current.width, (current.height + delta).max(height));
    if window.request_inner_size(target).is_none() {
        tracing::debug!("settings window height {height} is applied when the compositor answers");
    }
}

/// 제목 줄 동작 가운데 설정창이 받는 것.
#[derive(Debug, Default, PartialEq, Eq)]
struct Routed {
    drag: bool,
    /// 메인 창 닫기가 아니라 설정 모달 닫기다.
    close: bool,
}

/// 제목 줄 동작을 이동과 닫기로 나눈다. 설정창 제목 줄은 닫기 버튼만 두므로 빈 영역 더블클릭의 최대화 토글과
/// 최소화는 받지 않는다.
fn route_actions(actions: &[TitlebarAction]) -> Routed {
    let mut routed = Routed::default();
    for action in actions {
        match action {
            TitlebarAction::StartDrag => routed.drag = true,
            TitlebarAction::Close => routed.close = true,
            TitlebarAction::ToggleMaximize | TitlebarAction::Minimize => {}
        }
    }
    routed
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

    /// Given 앱이 제목 줄을 그리는 설정창 Then 크기는 제목 줄만큼 높은 1100 × 736이고, When 제목 줄이 없으면(서버
    /// 장식, macOS·Windows) Then 1100 × 700이다.
    #[test]
    fn the_window_grows_by_the_titlebar_only_when_the_app_draws_it() {
        use crate::app::modal::settings::settings_window_size;
        assert_eq!(settings_window_size(true), LogicalSize::new(1100.0, 736.0));
        assert_eq!(settings_window_size(false), LogicalSize::new(1100.0, 700.0));
    }

    /// Given 제목 줄의 닫기 버튼을 눌렀을 때, When 동작을 나누면, Then 이동 없이 닫기만 남는다.
    /// 빈 영역 드래그는 이동이고, 더블클릭 최대화 토글과 최소화는 아무것도 하지 않는다.
    #[test]
    fn the_close_button_only_closes_the_settings_modal() {
        assert_eq!(
            route_actions(&[TitlebarAction::Close]),
            Routed {
                drag: false,
                close: true
            }
        );
        assert_eq!(
            route_actions(&[TitlebarAction::StartDrag]),
            Routed {
                drag: true,
                close: false
            }
        );
        assert_eq!(
            route_actions(&[TitlebarAction::ToggleMaximize, TitlebarAction::Minimize]),
            Routed::default()
        );
    }

    /// Given 포인터가 창 가장자리에 있을 때 Then 리사이즈 방향이 있고, When 그 자리가 닫기 버튼이나 포인터 아래
    /// 위젯(스크롤바) 영역이면 Then 없다. 가운데·창 밖·포인터 없음도 없다.
    #[test]
    fn the_edge_yields_to_the_close_button_and_to_widgets() {
        assert_eq!(
            edge_direction(at(1099.0, 300.0), &[close_rect()], SIZE),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            edge_direction(at(1099.0, 1.0), &[None], SIZE),
            Some(ResizeDirection::NorthEast)
        );
        assert_eq!(edge_direction(at(1099.0, 1.0), &[close_rect()], SIZE), None);
        assert_eq!(edge_direction(at(500.0, 300.0), &[None], SIZE), None);
        assert_eq!(
            edge_direction(at(1100.0, 300.0), &[None], SIZE),
            None,
            "outside"
        );
        assert_eq!(
            edge_direction(at(-1.0, 300.0), &[None], SIZE),
            None,
            "outside"
        );
        assert_eq!(edge_direction(None, &[None], SIZE), None);
        let scrollbar = Some(PhysicalRect {
            x: PhysicalPx(1090.0),
            y: PhysicalPx(80.0),
            width: PhysicalPx(10.0),
            height: PhysicalPx(500.0),
        });
        assert_eq!(
            edge_direction(at(1099.0, 300.0), &[close_rect(), scrollbar], SIZE),
            None,
            "a widget under the pointer keeps the press"
        );
    }

    fn chrome(server_decorated: bool) -> SettingsChrome {
        SettingsChrome {
            cursor: None,
            close: close_rect().map(|r| (r, SIZE.width)),
            edge_widgets: Vec::new(),
            areas_stale: false,
            resize_buttons: Vec::new(),
            drawn_server_decorated: server_decorated,
            replayed_press: false,
            halted_press: false,
        }
    }

    fn moved(x: f64, y: f64) -> WindowEvent {
        WindowEvent::CursorMoved {
            device_id: winit::event::DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        }
    }

    fn left(state: ElementState) -> WindowEvent {
        WindowEvent::MouseInput {
            device_id: winit::event::DeviceId::dummy(),
            state,
            button: MouseButton::Left,
        }
    }

    const PRESS: (ElementState, MouseButton) = (ElementState::Pressed, MouseButton::Left);
    const RELEASE: (ElementState, MouseButton) = (ElementState::Released, MouseButton::Left);

    fn run(
        c: &mut SettingsChrome,
        (s, b): (ElementState, MouseButton),
        can_start: bool,
        size: PhysicalSize<u32>,
    ) -> (bool, Option<ResizeDirection>) {
        let mut started = None;
        let owned = c.owns_button(s, b, can_start, size, |d| started = Some(d));
        (owned, started)
    }

    /// Given 포인터가 오른쪽 가장자리일 때, When 좌버튼을 누르면 Then 리사이즈를 시작하고 그 누름과 짝 뗌을 가져간다.
    /// When 시작할 수 없는 누름(편집 키 보류·저널 재생·장식·크기 고정, `can_start` 거짓)이거나, 닫기 버튼 위이거나,
    /// 서버 장식으로 그렸거나, 크기·배율이 바뀌고 아직 새 패스가 없으면 Then 시작하지 않고 누름·뗌 모두 egui로 간다.
    #[test]
    fn an_edge_press_starts_a_resize_unless_something_else_owns_it() {
        let mut c = chrome(false);
        c.cursor = at(1099.0, 300.0);
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (true, Some(ResizeDirection::East))
        );
        assert_eq!(run(&mut c, RELEASE, true, SIZE), (true, None));

        assert_eq!(
            run(&mut c, PRESS, false, SIZE),
            (false, None),
            "cannot start"
        );
        assert_eq!(run(&mut c, RELEASE, true, SIZE), (false, None));

        c.cursor = at(1080.0, 1.0);
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (false, None),
            "on the close button"
        );

        let mut c = chrome(true);
        c.cursor = at(1099.0, 300.0);
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (false, None),
            "server decorations"
        );

        let mut c = chrome(false);
        c.cursor = at(1099.0, 300.0);
        c.areas_stale = true;
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (false, None),
            "areas not redrawn"
        );
    }

    /// Given 창이 넓어졌지만 아직 다시 그리지 않았을 때, Then 닫기 버튼 영역은 오른쪽 변을 따라 옮겨 본다.
    #[test]
    fn the_close_button_follows_the_right_edge() {
        let c = chrome(false);
        let wide = c.close_rect(1300).expect("close rect");
        assert_eq!(wide.x, PhysicalPx(1270.0));
    }

    /// 제목 줄 하나를 egui 패스 여러 번에 걸쳐 그리고 보고된 동작을 모은다.
    fn bar_actions(passes: Vec<Vec<egui::Event>>, primary_drag_only: bool) -> Vec<TitlebarAction> {
        crate::i18n::init("en");
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        for events in passes {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 736.0),
                )),
                events,
                ..Default::default()
            };
            drop(ctx.run(input, |ctx| {
                let mut props = settings_props(&th, true, false);
                props.primary_drag_only = primary_drag_only;
                actions.extend(titlebar::draw_titlebar_view(ctx, &props).actions);
            }));
        }
        actions
    }

    /// 빈 제목 줄을 `button`으로 눌러 오른쪽으로 끄는 패스들.
    fn drag_with(button: egui::PointerButton) -> Vec<Vec<egui::Event>> {
        let at = |x: f32| egui::pos2(x, 18.0);
        let press = |x: f32, pressed: bool| egui::Event::PointerButton {
            pos: at(x),
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![
            vec![egui::Event::PointerMoved(at(500.0))],
            vec![press(500.0, true)],
            vec![egui::Event::PointerMoved(at(540.0))],
            vec![egui::Event::PointerMoved(at(580.0))],
            vec![press(580.0, false)],
        ]
    }

    /// Given 설정창 제목 줄의 빈 영역, When 왼쪽 버튼으로 끌면 Then 창 이동이고, When 오른쪽 버튼으로 끌면 Then
    /// 아무것도 하지 않는다. 메인 창 설정(`primary_drag_only` 거짓)은 오른쪽 드래그도 이동으로 보고한다.
    #[test]
    fn only_a_primary_drag_moves_the_settings_window() {
        use egui::PointerButton::{Primary, Secondary};
        assert_eq!(
            bar_actions(drag_with(Primary), true),
            vec![TitlebarAction::StartDrag]
        );
        assert_eq!(bar_actions(drag_with(Secondary), true), Vec::new());
        assert_eq!(
            bar_actions(drag_with(Secondary), false),
            vec![TitlebarAction::StartDrag],
            "the main window keeps any button"
        );
    }

    /// Given 저널이 멈췄을 때, When 닫기 버튼 안에서 눌렀다 그 안에서 떼면 Then 설정창을 닫는다. 밖으로 나가서 떼거나,
    /// 밖에서 누르거나, 창을 벗어나 누름이 취소되거나, 서버 장식 창(앱 제목 줄이 없다)이면 닫지 않는다.
    #[test]
    fn a_halted_click_closes_only_on_the_close_button() {
        use ElementState::{Pressed, Released};
        let left_window = WindowEvent::CursorLeft {
            device_id: winit::event::DeviceId::dummy(),
        };
        let resized = WindowEvent::Resized(PhysicalSize::new(1200, 700));
        let cases: [(&str, bool, Vec<WindowEvent>, bool); 6] = [
            (
                "press and release on close",
                false,
                vec![moved(1080.0, 10.0), left(Pressed), left(Released)],
                true,
            ),
            (
                "released outside",
                false,
                vec![
                    moved(1080.0, 10.0),
                    left(Pressed),
                    moved(500.0, 10.0),
                    left(Released),
                ],
                false,
            ),
            (
                "pressed outside",
                false,
                vec![
                    moved(500.0, 10.0),
                    left(Pressed),
                    moved(1080.0, 10.0),
                    left(Released),
                ],
                false,
            ),
            (
                "left the window",
                false,
                vec![
                    moved(1080.0, 10.0),
                    left(Pressed),
                    left_window,
                    moved(1080.0, 10.0),
                    left(Released),
                ],
                false,
            ),
            (
                "window resized",
                false,
                vec![moved(1080.0, 10.0), left(Pressed), resized, left(Released)],
                false,
            ),
            (
                "server decorated",
                true,
                vec![moved(1080.0, 10.0), left(Pressed), left(Released)],
                false,
            ),
        ];
        for (name, server_decorated, events, closes) in cases {
            let mut c = chrome(server_decorated);
            let got: Vec<bool> = events
                .iter()
                .map(|e| c.halted_close_click(e, SIZE.width))
                .collect();
            let (last, before) = got.split_last().expect("events");
            assert!(before.iter().all(|b| !b), "{name}: closed early");
            assert_eq!(*last, closes, "{name}");
        }
    }
}
