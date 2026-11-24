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
    /// 직전 패스에 그린 닫기 버튼 영역.
    close: Option<CloseArea>,
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
    /// 장식 전환으로 요청한 크기를 창이 그 자리에서 적용했다(Wayland). `Resized`가 오지 않으므로 패스 뒤에
    /// 호출자가 GPU 표면을 이 크기로 맞춘다.
    applied_size: Option<PhysicalSize<u32>>,
    /// 그 자리에서 적용한 창 크기. 그 크기로 그린 패스가 끝날 때까지 영역을 무효로 둔다.
    awaited_size: Option<PhysicalSize<u32>>,
}

/// 그린 닫기 버튼 영역(egui 논리 좌표)과 그때의 창 너비·점당 픽셀. 버튼은 오른쪽에 붙으므로 너비가 바뀌면 그만큼
/// 옮겨 보고, 배율이 바뀌면 새 배율로 물리 좌표를 다시 계산한다.
#[derive(Debug, Clone, Copy)]
struct CloseArea {
    rect: egui::Rect,
    /// 그릴 때의 창 너비(논리).
    basis: f32,
    /// egui 점당 픽셀. 본체는 egui zoom_factor가 1이라 창 배율(`Window::scale_factor`)과 같고, 저널이 멈춘 동안
    /// 배율이 바뀌면 새 배율로 바꾼다.
    ppp: f32,
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
            applied_size: None,
            awaited_size: None,
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
                // 다음 패스는 이 사건의 크기로 그리므로 그 자리 적용 크기를 더 기다리지 않는다.
                self.awaited_size = None;
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
        self.close.map(|area| {
            let now = PhysicalPx(width as f32).to_logical(area.ppp).value();
            let rect = area.rect.translate(egui::vec2(now - area.basis, 0.0));
            titlebar::physical_panel_rect(rect, area.ppp)
        })
    }

    /// 창 크기 요청이 그 자리에서 적용됐으면 적어 둔다. 그 크기에 맞춰 영역도 다시 적어야 한다.
    fn note_applied_size(&mut self, applied: Option<PhysicalSize<u32>>) {
        if let Some(size) = applied {
            self.applied_size = Some(size);
            self.awaited_size = Some(size);
            self.areas_stale = true;
        }
    }

    /// 장식 전환으로 그 자리에서 적용된 창 크기. 한 번만 돌려준다.
    pub(crate) fn take_applied_size(&mut self) -> Option<PhysicalSize<u32>> {
        self.applied_size.take()
    }

    /// 저널이 멈춘 동안 창 배율이 바뀌었다. 다시 그리지 않으므로 직전 닫기 버튼 영역을 새 배율로 옮겨 계속 받는다.
    fn rescale_close(&mut self, new_scale: f64) {
        if let Some(area) = &mut self.close {
            area.ppp = new_scale as f32;
        }
    }

    /// 설정 패널보다 먼저 제목 줄을 그리고 이동 요청을 창에 보낸다. 닫기 버튼을 눌렀으면 참이다.
    /// 컴포지터가 서버 장식을 골랐으면 그리지 않는다. `takes_keys`가 거짓이면(이번 패스의 키 주인이 팝업) 닫기
    /// 버튼은 키 입력을 보지 않는다.
    pub(crate) fn draw_titlebar(
        &mut self,
        ctx: &egui::Context,
        window: &Window,
        size: PhysicalSize<u32>,
        takes_keys: bool,
    ) -> bool {
        let server_decorated = window.is_decorated();
        let step = decoration_step(self.drawn_server_decorated, server_decorated);
        if let Some(server) = step.fit {
            self.note_applied_size(fit_window_height(window, server));
        }
        self.drawn_server_decorated = server_decorated;
        if !step.draw {
            self.close = None;
            return false;
        }
        let th = crate::theme::theme();
        let props = settings_props(&th, window.has_focus(), window.is_maximized());
        let result = draw_bar(ctx, props, takes_keys);
        let ppp = ctx.pixels_per_point();
        self.close = result.close_rect.map(|rect| CloseArea {
            rect,
            basis: PhysicalPx(size.width as f32).to_logical(ppp).value(),
            ppp,
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
        self.record_pass(ctx, size);
        if !size_is_locked(window)
            && let Some(dir) = self.edge_direction(size)
        {
            ctx.set_cursor_icon(titlebar::resize_cursor(dir));
        }
    }

    /// 이번 패스에서 가장자리 밴드와 겹친 위젯을 적는다. `size`는 이 패스를 그린 크기다. 그 자리에서 적용한 크기를
    /// 기다리는 중이면 옛 크기로 그린 패스의 영역이므로 무효를 유지한다.
    fn record_pass(&mut self, ctx: &egui::Context, size: PhysicalSize<u32>) {
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
        if self.awaited_size.is_some_and(|awaited| awaited != size) {
            return;
        }
        self.awaited_size = None;
        self.areas_stale = false;
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
            // 저장한 포인터 좌표는 옛 배율의 값이라 다음 이동까지 쓰지 않는다.
            WindowEvent::ScaleFactorChanged {
                scale_factor: new_scale,
                ..
            } => {
                self.halted_press = false;
                self.rescale_close(*new_scale);
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
        popup_owns_keys: false,
    }
}

/// 제목 줄을 그린다. 팝업이 키 주인인 패스(`takes_keys` 거짓)에서는 본문처럼 키 입력을 가린 채 그려 닫기 버튼이
/// Tab·Space·Enter를 읽지 않는다.
fn draw_bar(
    ctx: &egui::Context,
    mut props: TitlebarProps,
    takes_keys: bool,
) -> titlebar::TitlebarDrawResult {
    props.popup_owns_keys = !takes_keys;
    crate::adapters::ui::popup::body_keys::with_body_keys(ctx, takes_keys, || {
        titlebar::draw_titlebar_view(ctx, &props)
    })
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

/// 장식 협상 결과 하나로 할 일.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DecorationStep {
    /// 창 높이를 맞출 장식(서버 장식이면 참). 창 크기를 맞춰 둔 장식과 같으면 없다.
    pub(crate) fit: Option<bool>,
    /// 앱 제목 줄을 그린다. 서버 장식이면 OS가 제목 줄을 그리므로 그리지 않는다.
    pub(crate) draw: bool,
}

/// 창 크기를 `sized_for_server` 장식(서버 장식이면 참)에 맞춰 두었을 때 지금 협상 결과 `server_decorated`로 할 일.
/// 창은 앱 제목 줄 크기(`sized_for_server` 거짓)로 만들고, 그 뒤로는 마지막으로 그린 장식에 맞춰 둔 상태다.
pub(crate) fn decoration_step(sized_for_server: bool, server_decorated: bool) -> DecorationStep {
    DecorationStep {
        fit: (sized_for_server != server_decorated).then_some(server_decorated),
        draw: !server_decorated,
    }
}

/// 장식 모드가 정해지거나 바뀌면 최소 크기를 그 모드의 값(`settings_window_size`)으로 바꾸고 현재 높이를
/// 제목 줄만큼 늘이거나 줄인다. 서버 장식이면 앱 제목 줄이 없다. 창이 요청을 그 자리에서 적용하면(Wayland) 그
/// 크기를 돌려준다. 이때는 `Resized`가 오지 않는다. X11은 서버 응답 뒤 `Resized`로 온다.
pub(crate) fn fit_window_height(
    window: &Window,
    server_decorated: bool,
) -> Option<PhysicalSize<u32>> {
    use crate::app::modal::settings::settings_window_size;
    let min = settings_window_size(!server_decorated);
    let height = min.height;
    window.set_min_inner_size(Some(min));
    let current: LogicalSize<f32> = window.inner_size().to_logical(window.scale_factor());
    let delta = height - settings_window_size(server_decorated).height;
    let target = LogicalSize::new(current.width, (current.height + delta).max(height));
    let applied = window.request_inner_size(target);
    if applied.is_none() {
        tracing::debug!("settings window height {height} is applied when the compositor answers");
    }
    applied
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
            close: Some(CloseArea {
                rect: egui::Rect::from_min_size(egui::pos2(1070.0, 0.0), egui::vec2(30.0, 30.0)),
                basis: SIZE.width as f32,
                ppp: 1.0,
            }),
            edge_widgets: Vec::new(),
            areas_stale: false,
            resize_buttons: Vec::new(),
            drawn_server_decorated: server_decorated,
            replayed_press: false,
            halted_press: false,
            applied_size: None,
            awaited_size: None,
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
        c.edge_widgets = vec![PhysicalRect {
            x: PhysicalPx(1090.0),
            y: PhysicalPx(80.0),
            width: PhysicalPx(10.0),
            height: PhysicalPx(500.0),
        }];
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (false, None),
            "a scrollbar on the edge keeps the press"
        );
        assert_eq!(run(&mut c, RELEASE, true, SIZE), (false, None));
        c.edge_widgets.clear();
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
        bar_actions_with_keys(passes, primary_drag_only, &[])
    }

    /// `bar_actions`와 같고, `popup_keys`에 든 패스 번호에서는 팝업이 키 주인이다.
    fn bar_actions_with_keys(
        passes: Vec<Vec<egui::Event>>,
        primary_drag_only: bool,
        popup_keys: &[usize],
    ) -> Vec<TitlebarAction> {
        crate::i18n::init("en");
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        for (pass, events) in passes.into_iter().enumerate() {
            let takes_keys = !popup_keys.contains(&pass);
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
                actions.extend(draw_bar(ctx, props, takes_keys).actions);
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

    fn pointer(pos: (f32, f32), button: egui::PointerButton, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: egui::pos2(pos.0, pos.1),
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Given 본문에서 왼쪽 버튼을 누른 채 제목 줄로 온 포인터, When 제목 줄에서 오른쪽 버튼을 새로 눌러 끌면 Then
    /// 드래그를 시작한 누름이 오른쪽이라 창 이동이 아니다.
    #[test]
    fn a_drag_started_by_a_right_press_does_not_move_even_with_left_held() {
        use egui::PointerButton::{Primary, Secondary};
        let moved_to = |x: f32, y: f32| egui::Event::PointerMoved(egui::pos2(x, y));
        let passes = vec![
            vec![moved_to(500.0, 300.0)],
            vec![pointer((500.0, 300.0), Primary, true)],
            vec![moved_to(500.0, 18.0)],
            vec![pointer((500.0, 18.0), Secondary, true)],
            vec![moved_to(540.0, 18.0)],
            vec![moved_to(580.0, 18.0)],
            vec![pointer((580.0, 18.0), Secondary, false)],
            vec![pointer((580.0, 18.0), Primary, false)],
        ];
        assert_eq!(bar_actions(passes, true), Vec::new());
    }

    /// Given 제목 줄에서 오른쪽 버튼을 누른 포인터, When 다음 패스에 본문에서 왼쪽을 누르고 둘 다 누른 채 움직이면
    /// Then 드래그 후보를 만든 제목 줄 누름이 오른쪽이라 창 이동이 아니다.
    #[test]
    fn a_left_press_outside_the_bar_does_not_turn_a_right_drag_into_a_move() {
        use egui::PointerButton::{Primary, Secondary};
        let moved_to = |x: f32, y: f32| egui::Event::PointerMoved(egui::pos2(x, y));
        // 제목 줄(높이 36) 아래 끝에서 눌러 클릭 한계 안(5)만 움직여 본문으로 나간다. 그보다 크게 움직이면 그 이동이
        // 오른쪽 드래그를 먼저 시작한다.
        let passes = vec![
            vec![moved_to(500.0, 33.0)],
            vec![pointer((500.0, 33.0), Secondary, true)],
            vec![moved_to(500.0, 38.0)],
            vec![pointer((500.0, 38.0), Primary, true)],
            vec![moved_to(540.0, 38.0)],
            vec![moved_to(580.0, 38.0)],
            vec![pointer((580.0, 38.0), Primary, false)],
            vec![pointer((580.0, 38.0), Secondary, false)],
        ];
        assert_eq!(bar_actions(passes, true), Vec::new());
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

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Given 설정창 제목 줄, When Tab으로 닫기 버튼에 포커스를 주고 Space를 누르면 Then 닫는다. When 팝업이 그 패스의
    /// 키 주인이면 Then 같은 Tab·Space로도 닫기 버튼은 포커스를 받지 않고 눌리지 않는다.
    #[test]
    fn the_close_button_takes_no_keys_while_a_popup_owns_them() {
        use egui::Key::{Space, Tab};
        let passes = || {
            vec![
                Vec::new(),
                vec![key(Tab, true), key(Tab, false)],
                vec![key(Space, true), key(Space, false)],
            ]
        };
        assert_eq!(
            bar_actions_with_keys(passes(), true, &[]),
            vec![TitlebarAction::Close]
        );
        assert_eq!(
            bar_actions_with_keys(passes(), true, &[1, 2]),
            Vec::new(),
            "a popup owns the keys"
        );
    }

    /// Given 팝업이 키 주인인 패스에서 Tab을 눌렀을 때, When 팝업이 닫혀 다음 패스에 Space·Enter가 설정창에 오면
    /// Then 닫기 버튼은 그 Tab으로 포커스를 받지 않았으므로 닫지 않는다.
    #[test]
    fn a_tab_under_a_popup_leaves_no_focus_for_a_later_space() {
        use egui::Key::{Enter, Space, Tab};
        for press in [Space, Enter] {
            let passes = vec![
                Vec::new(),
                vec![key(Tab, true), key(Tab, false)],
                vec![key(press, true), key(press, false)],
            ];
            assert_eq!(
                bar_actions_with_keys(passes, true, &[1]),
                Vec::new(),
                "{press:?}"
            );
        }
    }

    /// Given 앱 제목 줄 크기로 만든 창, When 협상이 서버 장식이면 Then 높이를 서버 장식에 맞추고 제목 줄을 그리지
    /// 않는다. 이미 맞춰 둔 장식과 같으면 높이는 그대로이고, 앱 장식으로 돌아오면 다시 늘이고 그린다.
    #[test]
    fn the_decoration_decides_the_height_and_the_titlebar() {
        let step = |fit, draw| DecorationStep { fit, draw };
        assert_eq!(decoration_step(false, false), step(None, true));
        assert_eq!(
            decoration_step(false, true),
            step(Some(true), false),
            "a new window under server decorations"
        );
        assert_eq!(decoration_step(true, true), step(None, false));
        assert_eq!(decoration_step(true, false), step(Some(false), true));
    }

    /// 빈 egui 패스 하나를 돌리고 그 패스의 위젯을 적는다.
    fn record_empty_pass(c: &mut SettingsChrome, size: PhysicalSize<u32>) {
        let ctx = egui::Context::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| c.record_pass(ctx, size)));
    }

    /// Given 장식 전환으로 창이 1100 × 700으로 그 자리에서 줄었을 때, When 같은 패스(옛 1100 × 736으로 그림)가 끝나면
    /// Then 가장자리 누름은 아직 리사이즈를 시작하지 않고, When 새 크기로 그린 패스가 끝나면 Then 시작한다.
    #[test]
    fn edge_areas_wait_for_a_pass_at_the_applied_size() {
        let old = PhysicalSize::new(1100, 736);
        let mut c = chrome(false);
        c.cursor = at(1099.0, 300.0);
        c.note_applied_size(Some(SIZE));
        record_empty_pass(&mut c, old);
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (false, None),
            "areas of the old size"
        );
        assert_eq!(run(&mut c, RELEASE, true, SIZE), (false, None));
        record_empty_pass(&mut c, SIZE);
        assert_eq!(
            run(&mut c, PRESS, true, SIZE),
            (true, Some(ResizeDirection::East))
        );
    }

    /// Given 장식 전환으로 요청한 크기를 창이 그 자리에서 적용했을 때 Then 호출자는 그 크기를 한 번 받아 GPU 표면을
    /// 맞추고, 새 패스가 영역을 다시 적기 전에는 리사이즈를 시작하지 않는다. When 적용되지 않았으면(X11) Then 받을
    /// 크기가 없다(`Resized`가 온다).
    #[test]
    fn a_size_applied_on_the_spot_reaches_the_gpu_once() {
        let mut c = chrome(false);
        c.note_applied_size(None);
        assert_eq!(c.take_applied_size(), None);
        assert!(!c.areas_stale);
        let applied = PhysicalSize::new(1100, 736);
        c.note_applied_size(Some(applied));
        assert!(c.areas_stale);
        assert_eq!(c.take_applied_size(), Some(applied));
        assert_eq!(c.take_applied_size(), None);
    }

    /// Given 저널이 멈춘 창, When 배율이 1에서 2로 바뀌고 창이 2200으로 넓어지면 Then 닫기 버튼 영역도 두 배 자리로
    /// 옮겨 그 안의 클릭으로 닫고, 옛 자리의 클릭으로는 닫지 않는다.
    #[test]
    fn a_halted_scale_change_keeps_the_close_button() {
        use ElementState::{Pressed, Released};
        let click = |c: &mut SettingsChrome, x: f64, y: f64| {
            [moved(x, y), left(Pressed), left(Released)]
                .iter()
                .map(|e| c.halted_close_click(e, 2200))
                .last()
                .expect("events")
        };
        let mut c = chrome(false);
        c.rescale_close(2.0);
        assert!(click(&mut c, 2170.0, 30.0), "the button at the new scale");
        let mut c = chrome(false);
        c.rescale_close(2.0);
        assert!(!click(&mut c, 1080.0, 10.0), "the old place");
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
