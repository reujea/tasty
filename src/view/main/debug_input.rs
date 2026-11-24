//! debug 전용 포인터·키·문자 입력 주입. release에는 없다.
//! PTY 보고 문자열을 넣는 debug.inject_mouse와 달리 실제 MainView·egui 입력 경로를 사용한다.

#![cfg(debug_assertions)]

use super::input_segment::{key_focus, press_focus, segment_focus};
use crate::model::PhysicalPx;
use crate::runtime::engine_read::EngineRead;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

use super::MainView;

/// 주입할 포인터 동작 종류.
pub(crate) enum InjectPointer {
    Move,
    Button { button: MouseButton, pressed: bool },
    Scroll { dx: f32, dy: f32, unit: ScrollUnit },
}

/// 휠 단위. 장치별 변환 경로를 각각 확인할 수 있도록 줄·포인트·페이지를 구분한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ScrollUnit {
    /// 줄 수. 데스크톱 마우스 휠 한 칸이 1.0.
    Line,
    /// 포인트. winit 레벨로 넣을 때는 **물리 픽셀**이다(`PixelDelta` 가 물리 px 이고
    /// 수신 측이 scale factor 로 나눈다) — egui 레벨로 넣을 때는 논리 포인트다.
    Point,
    /// 페이지. egui 는 이 단위를 다루지만 egui-winit 이 데스크톱에서 만들지 않으므로
    /// winit 레벨에는 대응하는 델타가 없다.
    Page,
}

impl ScrollUnit {
    /// 단위 이름을 해석한다. 모르는 값은 거절한다.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "line" => Some(Self::Line),
            "point" => Some(Self::Point),
            "page" => Some(Self::Page),
            _ => None,
        }
    }

    /// egui 입력 큐에 넣을 단위.
    fn to_egui(self) -> egui::MouseWheelUnit {
        match self {
            Self::Line => egui::MouseWheelUnit::Line,
            Self::Point => egui::MouseWheelUnit::Point,
            Self::Page => egui::MouseWheelUnit::Page,
        }
    }

    /// winit 핸들러에 넣을 델타. `Page` 는 winit 에 표현이 없어 `None` — 그 경우
    /// 주입은 성공한 척하지 않고 거절된다.
    fn to_winit_delta(self, dx: f32, dy: f32) -> Option<MouseScrollDelta> {
        match self {
            Self::Line => Some(MouseScrollDelta::LineDelta(dx, dy)),
            Self::Point => Some(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                dx as f64, dy as f64,
            ))),
            Self::Page => None,
        }
    }
}

impl MainView {
    /// surface_id 영역의 정규화 좌표 (fx, fy ∈ [0,1]) 에 포인터 동작을 주입한다.
    ///
    /// surface 의 물리 rect 를 찾아 window-global 물리 좌표로 환산한 뒤, 실제
    /// WindowEvent 가 가는 것과 동일한 MainView 핸들러를 호출한다. surface 가 layout
    /// 에 없으면 `false`.
    pub(crate) fn debug_inject_mesh_pointer(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        fx: f32,
        fy: f32,
        action: InjectPointer,
    ) -> bool {
        let terminal_rect = self.compute_terminal_rect();
        let Some(rect) = self.state.surface_rect_by_id(
            engine,
            surface_id,
            terminal_rect,
            self.base.gpu.scale_factor(),
        ) else {
            return false;
        };
        let x = rect.x.value() + fx * rect.width.value();
        let y = rect.y.value() + fy * rect.height.value();
        let pos = PhysicalPosition::new(x as f64, y as f64);
        // cursor_position 은 handle_mouse_input/wheel 의 hit-test 출발점이라 먼저 갱신.
        self.cursor_position = Some(pos);

        match action {
            InjectPointer::Move => self.handle_cursor_moved(engine, pos, false),
            InjectPointer::Button { button, pressed } => {
                // 이전 메뉴 결과를 지운다. 이번 요청은 OS가 표시하기 전에 응답 경로와 포획으로 나눈다.
                self.debug_captured_menu = None;
                let state = if pressed {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                };
                // handle_event를 거치지 않아 메뉴 닫기 입력 판정을 직접 호출한다.
                let swallow = self.take_menu_dismiss_swallow(state, button);
                if !self.begin_os_resize_press(state, button, swallow) {
                    self.handle_mouse_input(engine, state, button, false, swallow);
                }
                self.debug_route_injected_menu();
            }
            InjectPointer::Scroll { dx, dy, unit } => {
                let Some(delta) = unit.to_winit_delta(dx, dy) else {
                    return false;
                };
                self.handle_mouse_wheel(engine, delta, false);
            }
        }
        true
    }

    /// 정규화 좌표를 egui의 다음 프레임 입력에 넣어 plugin 팝업 전달을 검사한다.
    pub(crate) fn debug_inject_egui_pointer(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        fx: f32,
        fy: f32,
        surface_id: Option<u32>,
        action: InjectPointer,
        modifiers: Option<egui::Modifiers>,
    ) -> bool {
        let ppp = self.base.gpu.egui_pixels_per_point().max(f32::EPSILON);
        // surface를 지정했으면 그 영역, 아니면 창 전체에 대한 정규화 좌표다.
        let pos = if let Some(sid) = surface_id {
            let terminal_rect = self.compute_terminal_rect();
            let Some(rect) = self.state.surface_rect_by_id(
                engine,
                sid,
                terminal_rect,
                self.base.gpu.scale_factor(),
            ) else {
                return false;
            };
            let point = PhysicalPx(rect.x.value() + fx * rect.width.value());
            let line = PhysicalPx(rect.y.value() + fy * rect.height.value());
            egui::pos2(point.to_logical(ppp).value(), line.to_logical(ppp).value())
        } else {
            window_egui_pos(&self.base.gpu, fx, fy)
        };
        // 이번 입력이 메뉴를 만들지 않았을 때 이전 결과가 남지 않게 한다.
        self.debug_captured_menu = None;
        // 창의 누름(`press_snapshot_now`)과 같은 함수로 이 누름의 포커스 노드를 지금 정한다. 뒤이어 주입한 키가
        // 다음 egui 프레임을 기다리지 않고 그 포커스 노드를 따른다.
        if let InjectPointer::Button {
            button,
            pressed: true,
        } = &action
        {
            press_focus(
                &mut self.state,
                engine,
                &mut self.base.gpu,
                pos,
                map_egui_button(*button),
            );
        }
        push_egui_pointer_at(&mut self.base.gpu, pos, action, modifiers);
        true
    }

    /// 문자열 전체를 한 egui Text 이벤트로 넣는다. 빈 값·제어문자 등 실입력에서
    /// 전달되지 않는 문자열은 거절한다. Enter·Tab 같은 키 동작은 키 주입을 사용한다.
    /// 넣기 전에 창의 키처럼 입력 몫을 끊는다(`key_focus`).
    pub(crate) fn debug_inject_egui_text(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        text: &str,
    ) -> bool {
        if !text_reaches_egui_as_typed(text) {
            return false;
        }
        key_focus(&mut self.state, engine, &mut self.base.gpu);
        push_egui_text(&mut self.base.gpu, text)
    }
}

/// 창 전체에 대한 정규화 좌표(fx, fy)를 그 창의 egui 논리 좌표로 바꾼다.
fn window_egui_pos(gpu: &crate::gpu::GpuState, fx: f32, fy: f32) -> egui::Pos2 {
    let ppp = gpu.egui_pixels_per_point().max(f32::EPSILON);
    let (w, h) = gpu.surface_config_size();
    let logical_w = PhysicalPx(w as f32).to_logical(ppp).value();
    let logical_h = PhysicalPx(h as f32).to_logical(ppp).value();
    egui::pos2(logical_w * fx, logical_h * fy)
}

/// 창 종류와 관계없이 그 창 전체에 대한 정규화 좌표에 egui 포인터 동작을 넣는다.
/// surface가 없는 설정 같은 보조 창은 이 좌표 기준만 쓴다.
pub(crate) fn push_egui_pointer(
    gpu: &mut crate::gpu::GpuState,
    fx: f32,
    fy: f32,
    action: InjectPointer,
    modifiers: Option<egui::Modifiers>,
) {
    let pos = window_egui_pos(gpu, fx, fy);
    push_egui_pointer_at(gpu, pos, action, modifiers);
}

/// `modifiers`를 주면 egui가 누른 것으로 보는 수식 키를 그 값으로 바꾼 뒤 이벤트를 넣는다.
fn push_egui_pointer_at(
    gpu: &mut crate::gpu::GpuState,
    pos: egui::Pos2,
    action: InjectPointer,
    modifiers: Option<egui::Modifiers>,
) {
    if let Some(m) = modifiers {
        gpu.debug_set_egui_modifiers(m);
    }
    let modifiers = modifiers.unwrap_or_default();
    let events = match action {
        // 클릭 전 hover 를 같은 pos 로 세팅해야 plugin egui 가 위젯 hit-test 를 맞춘다.
        InjectPointer::Move => vec![egui::Event::PointerMoved(pos)],
        InjectPointer::Button { button, pressed } => vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: map_egui_button(button),
                pressed,
                modifiers,
            },
        ],
        InjectPointer::Scroll { dx, dy, unit } => vec![egui::Event::MouseWheel {
            unit: unit.to_egui(),
            delta: egui::vec2(dx, dy),
            modifiers,
        }],
    };
    gpu.debug_push_egui_events(events);
}

/// 키 주입이 쓰는 창 하나의 egui 입력 상태. 실제로는 그 창의 `GpuState`이고 시험은 가짜를 쓴다.
pub(crate) trait EguiKeySink {
    /// egui가 지금 누른 것으로 보는 수식 키.
    fn held_modifiers(&self) -> egui::Modifiers;
    /// 누른 것으로 보는 수식 키를 바꾼다. 다음에 바꿀 때까지 유지된다.
    fn hold_modifiers(&mut self, modifiers: egui::Modifiers);
    /// 실제 붙여넣기 키가 읽는 egui-winit 클립보드의 글자.
    fn clipboard_text(&mut self) -> Option<String>;
    /// 다음 프레임의 egui 입력 큐에 이벤트를 넣는다.
    fn push_events(&mut self, events: Vec<egui::Event>);
    /// 직전 egui 패스 끝에 Escape 가 눌려 있는지(창의 Esc 판정이 받는 값).
    fn escape_held(&self) -> bool {
        false
    }
    /// 창이 지금 실물 Win·Super 를 누른 것으로 보는지(`tasty_key_match::super_held`). egui 수식키는 비-macOS 의
    /// Win·Super 를 담지 않으므로 창의 Esc 판정은 이 값을 따로 읽는다.
    fn super_held(&self) -> bool {
        false
    }
}

impl EguiKeySink for crate::gpu::GpuState {
    fn held_modifiers(&self) -> egui::Modifiers {
        self.debug_egui_modifiers()
    }

    fn hold_modifiers(&mut self, modifiers: egui::Modifiers) {
        self.debug_set_egui_modifiers(modifiers);
    }

    fn clipboard_text(&mut self) -> Option<String> {
        self.debug_clipboard_text()
    }

    fn escape_held(&self) -> bool {
        self.egui_escape_held()
    }

    fn super_held(&self) -> bool {
        tasty_key_match::super_held(&self.egui_ctx)
    }

    fn push_events(&mut self, events: Vec<egui::Event>) {
        self.debug_push_egui_events(events);
    }
}

/// 키 하나를 주입해 egui 입력 큐에 실제로 들어간 것.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PushedKey {
    Key,
    Copy,
    Cut,
    Paste,
    /// 붙여넣기 키였지만 클립보드가 비어 아무것도 넣지 않았다.
    Nothing,
}

impl PushedKey {
    /// IPC 응답 `pushed`에 싣는 이름.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::Nothing => "none",
        }
    }
}

/// 메인 창에 주입한 키 하나. 창의 키와 같은 순서로 지금 입력 몫을 끊은 뒤(`key_focus`) 키를 egui 입력 큐에 넣는다
/// (`push_egui_key`). 포인터(`press_focus`)·글자(`debug_inject_egui_text`) 주입과 진입 순서가 같다. 매핑 불가 키면
/// 끊지도 넣지도 않고 `None`. 넣은 사건이 수식키 없는 Esc 누름이면(`RouteKey::bare_escape`, 수식키는 주입이 쥔 egui
/// 수식키와 창이 지금 누른 것으로 보는 실물 Win·Super `EguiKeySink::super_held`) 실제 키처럼 메인 경로의 Esc 수신자
/// (`key_nodes::bare_escape_decision`)를 정해 그 몫에 묶고(`window_feeds_key`) 단계 11 적용(`apply_escape_decision`)을 거친다.
/// 처리한 누름은 egui 큐에서 빠지고 키 주인 몫으로 넘어간다. egui 전용 입구라 단축키·끝 목적지 단계는 돌지 않는다.
pub(crate) fn inject_key<S: super::input_segment::EguiPressSink + EguiKeySink>(
    state: &mut crate::state::MainViewState,
    core: &crate::runtime::engine_read::EngineRead<'_>,
    sink: &mut S,
    key_name: &str,
    pressed: bool,
    modifiers: Option<egui::Modifiers>,
) -> Option<PushedKey> {
    let key = egui::Key::from_name(key_name)?;
    let bare_escape = pressed
        && crate::adapters::ui::input::key_route::RouteKey {
            key,
            modifiers: modifiers.unwrap_or_else(|| sink.held_modifiers()),
            super_held: sink.super_held(),
        }
        .bare_escape();
    let incoming = if bare_escape {
        crate::adapters::ui::popup::frame_keys::Incoming::Escape
    } else {
        crate::adapters::ui::popup::frame_keys::Incoming::Key
    };
    segment_focus(state, core, sink, incoming);
    let from = sink.queued_events().len();
    let pushed = push_egui_key(sink, key_name, pressed, modifiers)?;
    let loaded = sink.queued_events().get(from..).is_some_and(|e| {
        e.iter()
            .any(crate::adapters::ui::popup::body_keys::is_escape_press)
    });
    // 주입한 bare Esc 는 egui 입력에 실리므로 그 몫을 끝낸다(뒤 경계).
    state.mark_escape_segment(loaded, bare_escape);
    if loaded && bare_escape {
        let egui_surface =
            crate::adapters::ui::input::key_nodes::focused_surface_takes_egui(state, core);
        let decision =
            crate::adapters::ui::input::key_nodes::bare_escape_decision(state, core, egui_surface);
        // 결속은 winit 키의 단계 8 과 같은 판정이다. debug 주입은 수신자와 무관하게 egui 큐에 싣는다.
        let _ = state.window_feeds_key(decision, egui_surface, true);
        let held = sink.escape_held();
        if !state.apply_escape_decision(decision, loaded, sink.queued_events(), held) {
            tracing::debug!(
                "the window did not take the injected Escape; it stays in the egui input"
            );
        }
    }
    Some(pushed)
}

/// 메인 창에 주입한 붙여넣기·복사·잘라내기 사건 하나. 키 주입과 같이 지금 입력 몫을 끊은 뒤(`key_focus`) egui 입력
/// 큐에 넣는다.
pub(crate) fn inject_clipboard<S: super::input_segment::EguiPressSink + EguiKeySink>(
    state: &mut crate::state::MainViewState,
    core: &crate::core::CoreState,
    sink: &mut S,
    event: egui::Event,
) {
    key_focus(state, core, sink);
    sink.push_events(vec![event]);
}

/// 창 종류와 관계없이 그 창의 egui 입력 큐에 키 하나를 실제 키가 egui에 닿는 모양으로 넣는다.
/// `modifiers`를 주면 그 창의 유지 수식 키를 먼저 바꾸고, 이벤트에는 유지 수식 키를 싣는다.
/// 매핑 불가 키면 아무것도 바꾸지 않고 `None`.
pub(crate) fn push_egui_key(
    sink: &mut dyn EguiKeySink,
    key_name: &str,
    pressed: bool,
    modifiers: Option<egui::Modifiers>,
) -> Option<PushedKey> {
    let key = egui::Key::from_name(key_name)?;
    if let Some(m) = modifiers {
        sink.hold_modifiers(m);
    }
    let held = sink.held_modifiers();
    let (events, pushed) = key_events(key, pressed, held, || sink.clipboard_text());
    if !events.is_empty() {
        sink.push_events(events);
    }
    Some(pushed)
}

/// egui-winit 0.31.1 `State::on_keyboard_input`의 변환을 옮겨 적은 것. 눌린 편집 명령은
/// 복사·잘라내기·붙여넣기 이벤트 하나로 바꾸고 같은 눌림의 `Key`는 넣지 않는다. 붙여넣기는
/// 클립보드 글자의 `\r\n`을 `\n`으로 바꾸고, 비어 있으면 아무것도 넣지 않는다.
fn key_events(
    key: egui::Key,
    pressed: bool,
    modifiers: egui::Modifiers,
    read_clipboard: impl FnOnce() -> Option<String>,
) -> (Vec<egui::Event>, PushedKey) {
    if pressed {
        if is_cut_command(modifiers, key) {
            return (vec![egui::Event::Cut], PushedKey::Cut);
        }
        if is_copy_command(modifiers, key) {
            return (vec![egui::Event::Copy], PushedKey::Copy);
        }
        if is_paste_command(modifiers, key) {
            let text = read_clipboard()
                .map(|t| t.replace("\r\n", "\n"))
                .filter(|t| !t.is_empty());
            return match text {
                Some(t) => (vec![egui::Event::Paste(t)], PushedKey::Paste),
                None => (Vec::new(), PushedKey::Nothing),
            };
        }
    }
    let event = egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    };
    (vec![event], PushedKey::Key)
}

/// egui-winit 0.31.1의 동명 판정을 옮겨 적은 것(상류 함수가 비공개다).
fn is_cut_command(modifiers: egui::Modifiers, key: egui::Key) -> bool {
    key == egui::Key::Cut
        || (modifiers.command && key == egui::Key::X)
        || (cfg!(target_os = "windows") && modifiers.shift && key == egui::Key::Delete)
}

/// egui-winit 0.31.1의 동명 판정을 옮겨 적은 것.
fn is_copy_command(modifiers: egui::Modifiers, key: egui::Key) -> bool {
    key == egui::Key::Copy
        || (modifiers.command && key == egui::Key::C)
        || (cfg!(target_os = "windows") && modifiers.ctrl && key == egui::Key::Insert)
}

/// egui-winit 0.31.1의 동명 판정을 옮겨 적은 것.
fn is_paste_command(modifiers: egui::Modifiers, key: egui::Key) -> bool {
    key == egui::Key::Paste
        || (modifiers.command && key == egui::Key::V)
        || (cfg!(target_os = "windows") && modifiers.shift && key == egui::Key::Insert)
}

/// 창 종류와 관계없이 그 창의 egui 입력 큐에 문자열을 한 Text 이벤트로 넣는다.
/// 거절 조건은 [`MainView::debug_inject_egui_text`]와 같다.
pub(crate) fn push_egui_text(gpu: &mut crate::gpu::GpuState, text: &str) -> bool {
    if !text_reaches_egui_as_typed(text) {
        return false;
    }
    gpu.debug_push_egui_events(vec![egui::Event::Text(text.to_string())]);
    true
}

/// egui-winit의 문자 판정을 따르고 TextEdit이 무시하는 빈 값은 제외한다.
/// 상류 함수가 비공개여서 같은 조건을 여기서 검사한다.
fn text_reaches_egui_as_typed(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_printable_char)
}

/// `egui-winit` 의 동명 판정을 옮겨 적은 것.
fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
        || ('\u{100000}'..='\u{10fffd}').contains(&chr);
    !is_in_private_use_area && !chr.is_ascii_control()
}

/// winit 마우스 버튼 → egui 포인터 버튼 (debug 주입용).
fn map_egui_button(button: MouseButton) -> egui::PointerButton {
    match button {
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
        _ => egui::PointerButton::Primary,
    }
}

#[cfg(all(test, feature = "gui"))]
#[path = "debug_input_escape_tests.rs"]
mod escape_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unit_reaches_egui_as_itself() {
        assert_eq!(ScrollUnit::Line.to_egui(), egui::MouseWheelUnit::Line);
        assert_eq!(ScrollUnit::Point.to_egui(), egui::MouseWheelUnit::Point);
        assert_eq!(ScrollUnit::Page.to_egui(), egui::MouseWheelUnit::Page);
    }

    /// winit 레벨은 두 갈래뿐이다 — 줄은 `LineDelta`, 포인트는 `PixelDelta`.
    #[test]
    fn winit_level_maps_line_and_point_to_the_two_winit_deltas() {
        assert!(matches!(
            ScrollUnit::Line.to_winit_delta(1.0, -3.0),
            Some(MouseScrollDelta::LineDelta(1.0, -3.0))
        ));
        let Some(MouseScrollDelta::PixelDelta(p)) = ScrollUnit::Point.to_winit_delta(1.0, -3.0)
        else {
            panic!("point 는 PixelDelta 여야 한다");
        };
        assert_eq!((p.x, p.y), (1.0, -3.0));
    }

    #[test]
    fn winit_level_refuses_page_instead_of_folding_it_into_lines() {
        assert!(ScrollUnit::Page.to_winit_delta(0.0, 1.0).is_none());
    }

    #[test]
    fn unknown_unit_names_are_rejected_not_defaulted() {
        assert_eq!(ScrollUnit::from_name("line"), Some(ScrollUnit::Line));
        assert_eq!(ScrollUnit::from_name("point"), Some(ScrollUnit::Point));
        assert_eq!(ScrollUnit::from_name("page"), Some(ScrollUnit::Page));
        assert_eq!(ScrollUnit::from_name("Line"), None);
        assert_eq!(ScrollUnit::from_name("lines"), None);
        assert_eq!(ScrollUnit::from_name(""), None);
    }

    #[test]
    fn text_that_the_text_edit_drops_is_refused_instead_of_reported_as_injected() {
        assert!(!text_reaches_egui_as_typed(""));
        assert!(!text_reaches_egui_as_typed("\n"));
        assert!(!text_reaches_egui_as_typed("\r"));
    }

    #[test]
    fn characters_the_real_path_never_carries_are_refused() {
        assert!(!text_reaches_egui_as_typed("git\tstatus"));
        assert!(!text_reaches_egui_as_typed("\u{1b}"));
        assert!(!text_reaches_egui_as_typed("\u{e000}"));
    }

    fn command() -> egui::Modifiers {
        egui::Modifiers::COMMAND
    }

    fn unread() -> Option<String> {
        panic!("only a paste command reads the clipboard")
    }

    #[test]
    fn a_pressed_copy_or_cut_command_becomes_the_edit_alone() {
        assert_eq!(
            key_events(egui::Key::C, true, command(), unread),
            (vec![egui::Event::Copy], PushedKey::Copy)
        );
        assert_eq!(
            key_events(egui::Key::X, true, command(), unread),
            (vec![egui::Event::Cut], PushedKey::Cut)
        );
    }

    #[test]
    fn releasing_the_edit_key_is_a_key_event_with_the_modifiers() {
        let (events, pushed) = key_events(egui::Key::C, false, command(), unread);
        assert_eq!(pushed, PushedKey::Key);
        assert!(matches!(
            events[..],
            [egui::Event::Key { key: egui::Key::C, pressed: false, modifiers, .. }] if modifiers.command
        ));
    }

    #[test]
    fn a_paste_command_reads_the_clipboard_and_normalizes_line_ends() {
        assert_eq!(
            key_events(egui::Key::V, true, command(), || Some("a\r\nb".into())),
            (vec![egui::Event::Paste("a\nb".into())], PushedKey::Paste)
        );
    }

    #[test]
    fn an_empty_or_unreadable_clipboard_pushes_nothing() {
        assert_eq!(
            key_events(egui::Key::V, true, command(), || Some(String::new())),
            (Vec::new(), PushedKey::Nothing)
        );
        assert_eq!(
            key_events(egui::Key::V, true, command(), || None),
            (Vec::new(), PushedKey::Nothing)
        );
    }

    #[test]
    fn other_keys_carry_the_modifiers_without_reading_the_clipboard() {
        let (events, _) = key_events(egui::Key::A, true, command(), unread);
        assert!(matches!(
            events[..],
            [egui::Event::Key { key: egui::Key::A, pressed: true, modifiers, .. }] if modifiers.command
        ));
        let (events, pushed) = key_events(egui::Key::C, true, egui::Modifiers::NONE, unread);
        assert_eq!(pushed, PushedKey::Key);
        assert!(matches!(
            events[..],
            [egui::Event::Key {
                key: egui::Key::C,
                modifiers: egui::Modifiers::NONE,
                ..
            }]
        ));
    }

    #[test]
    fn what_a_person_types_passes() {
        assert!(text_reaches_egui_as_typed("theme"));
        assert!(text_reaches_egui_as_typed("새 탭"));
        assert!(text_reaches_egui_as_typed(" "));
        assert!(text_reaches_egui_as_typed("--force"));
    }
}
