//! 별도 창(설정·플러그인·프리셋)의 입력 패스. 메인 창과 같은 순서 함수(`gfx::gpu::input_pass::run_input_pass`)로 egui
//! 패스를 열고 닫는다(`GpuState::run_window_pass`). 사건마다 단계 3~5(`settle_window_input`)가 egui 입력에 넣기 전에 사건
//! 시점 노드로 입력을 끊고 누름을 적용한다. 몫은 관리자가 가진다(`PopupManager::cut_window_segment`). 계약:
//! `docs/architecture/input-layer.md`의 "별도 창 입력 패스".

use crate::adapters::ui::fullscreen::StageId;
use crate::adapters::ui::popup::PopupManager;
use crate::adapters::ui::popup::frame_keys::{Incoming, InputCuts};
use crate::adapters::ui::popup::window_segments::{WindowKey, WindowReceiver};
use crate::gfx::gpu::input_pass::InputPassState;
use crate::view::main::input_segment::EguiPressSink;

/// 팝업 관리자를 가진 별도 창 상태.
pub(crate) trait WindowPopups {
    fn window_popups(&mut self) -> &mut PopupManager;
}

/// 별도 창 패스 하나의 상태.
pub(crate) struct WindowPass<'a, T> {
    pub(crate) state: &'a mut T,
}

impl<'a, T: WindowPopups> WindowPass<'a, T> {
    pub(crate) fn new(state: &'a mut T) -> Self {
        Self { state }
    }

    /// `run` 전에 결속이 없는 몫의 키 주인을 지금 노드로 정한다(`PopupManager::begin_window_run`).
    pub(crate) fn begin_run(&mut self, ctx: &egui::Context) {
        self.state.window_popups().begin_window_run(ctx);
    }
}

/// 별도 창에는 Find 입력칸·몫 필터·전체화면 무대가 없다. 몫은 관리자가 열고 닫는다.
impl<T: WindowPopups> InputPassState for WindowPass<'_, T> {
    fn begin_key_pass(&mut self, escape_held: bool) {
        self.state.window_popups().begin_key_pass(escape_held);
    }

    fn find_fields(&self) -> &[u64] {
        &[]
    }

    fn drop_input_without_its_node(
        &mut self,
        _ctx: &egui::Context,
        _events: &mut Vec<egui::Event>,
    ) -> bool {
        false
    }

    fn find_paste_open(&self) -> bool {
        false
    }

    fn fullscreen_stage_id(&self) -> Option<StageId> {
        None
    }

    fn close_input_pass(&mut self, ctx: &egui::Context) {
        let popups = self.state.window_popups();
        if popups.close_key_pass() {
            crate::adapters::ui::popup::body_keys::release_escape(ctx);
        }
        popups.close_window_segment(ctx);
    }

    fn seal_after_drawing_transition(
        &mut self,
        _drawn: Option<StageId>,
        _cuts: &mut InputCuts,
        _raw_left: usize,
    ) {
    }
}

/// 별도 창 사건이 egui 입력 큐에 들어오는 종류. 누름·키·IME 가 아니면 끊지 않는다. 수식키 없는 Esc 누름은 메인 창과
/// 같은 판정(`key_class::is_bare_escape_press`)으로 가른다. `mods` 는 사건 시점 창의 winit 수식키다.
pub(crate) fn incoming_of(
    event: &winit::event::WindowEvent,
    mods: winit::keyboard::ModifiersState,
) -> Option<Incoming> {
    use crate::adapters::ui::input::key_class::{KeyEventValue, is_bare_escape_press};
    use winit::event::{ElementState, WindowEvent};
    match event {
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            ..
        } => Some(Incoming::Press),
        WindowEvent::KeyboardInput { event: key, .. }
            if is_bare_escape_press(&KeyEventValue {
                logical: &key.logical_key,
                physical: key.physical_key,
                pressed: key.state == ElementState::Pressed,
                mods,
            }) =>
        {
            Some(Incoming::Escape)
        }
        WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) => Some(Incoming::Key),
        _ => None,
    }
}

/// 별도 창 사건이 단계 6 편집 판정을 받는지. 관리자가 없는 종료 확인 창(`routed` 거짓)은 모든 사건이, Esc 도 편집
/// 판정 보류(`EditKeyState::judge`)를 거친다. 팝업 관리자를 가진 창은 메인 창 `edit_target::host_edit_target` 과 같이
/// 단계 4 결과(`receiver`)가 egui 수신자(팝업·창 위젯)인 키만 대상이고, 수식키 없는 Esc 누름은 편집 바인딩과 겹쳐도
/// 수신자의 Esc 처리로 간다(메인 창 `Routed::escape`). 키가 아닌 사건(`receiver` None)은 대상이다.
pub(crate) fn window_edit_target(
    incoming: Option<Incoming>,
    receiver: Option<WindowReceiver>,
    routed: bool,
) -> bool {
    !routed
        || (incoming != Some(Incoming::Escape) && receiver.is_none_or(|r| r.egui_node().is_some()))
}

/// 별도 창 키 사건의 단계 4 입력 값. 경로 키는 메인 창과 같은 분류(`key_class::classify_key`)이고, `own` 은 창 자체
/// 처리가 그 키를 바인딩으로 쓰는지다. 뗌·수식키 단독 누름·IME 는 지금 경로다. 키·IME 가 아니면 `None`.
pub(crate) fn window_key_of(
    event: &winit::event::WindowEvent,
    mods: winit::keyboard::ModifiersState,
    own: impl FnOnce(&winit::event::KeyEvent) -> bool,
) -> Option<WindowKey> {
    use crate::adapters::ui::input::key_class::{KeyClass, KeyEventValue, classify_key};
    use winit::event::{ElementState, WindowEvent};
    match event {
        WindowEvent::KeyboardInput { event: key, .. } => Some(
            match classify_key(&KeyEventValue {
                logical: &key.logical_key,
                physical: key.physical_key,
                pressed: key.state == ElementState::Pressed,
                mods,
            }) {
                KeyClass::Path(route) => WindowKey::Path {
                    key: route,
                    own: own(key),
                },
                KeyClass::Current(_) => WindowKey::Current,
            },
        ),
        WindowEvent::Ime(_) => Some(WindowKey::Current),
        _ => None,
    }
}

/// 단계 3~5: 키 사건(`key`)이면 먼저 사건 시점 노드에서 수신 결과를 정한다(단계 4, `PopupManager::settle_window_key`).
/// 그 사건을 egui 에 실으면(egui 수신자가 있는 키, 또는 누름·키가 아닌 IME) 큐에 넣기 전에 그 사건이 묶일 노드로
/// 끊고(`cut_window_segment`), 누름이면 그 자리에서 포커스를 옮긴다. 누름 좌표는 큐에 남은 마지막 포인터 이동, 없으면
/// egui 가 아는 마지막 좌표다. 키 사건의 수신 결과를 돌려준다(키가 아니면 None). 호출자는 그 결과의 egui 수신자가
/// 없으면 사건을 egui 에 싣지 않는다.
pub(crate) fn settle_window_input(
    sink: &mut impl EguiPressSink,
    ctx: &egui::Context,
    popups: &mut PopupManager,
    incoming: Incoming,
    key: Option<WindowKey>,
) -> Option<WindowReceiver> {
    popups.show_window_key_node(ctx);
    let receiver = key.map(|key| popups.settle_window_key(ctx, key));
    let node = match receiver {
        Some(receiver) => receiver.egui_node(),
        None => Some(popups.window_key_node(ctx)),
    };
    let Some(node) = node else {
        return receiver;
    };
    let cuts = popups.cut_window_segment(sink.queued_events(), incoming, node);
    for _ in 0..cuts {
        sink.cut_queue_here();
    }
    if incoming == Incoming::Press {
        let queued = sink.queued_events().iter().rev().find_map(|e| match e {
            egui::Event::PointerMoved(pos) => Some(*pos),
            _ => None,
        });
        if let Some(pos) = queued.or_else(|| ctx.input(|i| i.pointer.latest_pos())) {
            popups.press_window_at(ctx, pos);
        }
    }
    receiver
}

/// egui 에 실은 사건(`incoming`)의 몫을 묶는다. 키·IME 면 그 사건 시점 노드로 묶는다. bare Esc 면 그 몫을 끝내고(뒤
/// 경계, `PopupManager::note_escape_loaded`) 사건 시점 수신 노드로 묶어 결과를 적용한다(단계 11,
/// `PopupManager::apply_window_escape`). `pending` 은 그 창 egui 입력 큐, `escape_held` 는 직전 패스 끝의 Escape 눌림
/// 상태다.
pub(crate) fn bind_window_input(
    ctx: &egui::Context,
    popups: &mut PopupManager,
    incoming: Incoming,
    pending: &mut Vec<egui::Event>,
    escape_held: bool,
) {
    match incoming {
        Incoming::Escape => {
            popups.note_escape_loaded();
            popups.apply_window_escape(pending, escape_held);
        }
        Incoming::Key => popups.bind_window_key(ctx),
        Incoming::Press => {}
    }
}

/// 별도 창 debug 키 주입 하나. 보류 검사(`enter_window_debug`)를 지난 뒤 winit 키와 같은 사건 입구를 지난다: 실을
/// 사건을 정하고(`debug_input::egui_key_load`) → 끊기(`settle_window_input`) → egui 큐 적재 → 결속과 bare Esc 적용
/// (`bind_window_input`). 실을 사건이 없으면(빈 클립보드의 붙여넣기) 끊지도 결속하지도 않는다. bare Esc 인지는 메인 창
/// debug 주입과 같이 주입이 쥔 egui 수식키와 창이 지금 누른 것으로 보는 실물 Win·Super 로 정한다
/// (`RouteKey::bare_escape`).
#[cfg(debug_assertions)]
pub(crate) fn inject_window_key<S>(
    ctx: &egui::Context,
    popups: &mut PopupManager,
    sink: &mut S,
    key: egui::Key,
    pressed: bool,
    modifiers: Option<egui::Modifiers>,
) -> crate::view::main::debug_input::PushedKey
where
    S: EguiPressSink + crate::view::main::debug_input::EguiKeySink,
{
    let route = crate::adapters::ui::input::key_route::RouteKey {
        key: Some(key),
        modifiers: modifiers.unwrap_or_else(|| sink.held_modifiers()),
        super_held: sink.super_held(),
    };
    let bare_escape = pressed && route.bare_escape();
    let incoming = if bare_escape {
        Incoming::Escape
    } else {
        Incoming::Key
    };
    let window_key = if pressed {
        WindowKey::Path {
            key: route,
            own: false,
        }
    } else {
        WindowKey::Current
    };
    let (events, pushed) =
        crate::view::main::debug_input::egui_key_load(sink, key, pressed, modifiers);
    if events.is_empty() {
        return pushed;
    }
    let receiver = settle_window_input(sink, ctx, popups, incoming, Some(window_key));
    if receiver.is_some_and(|r| r.egui_node().is_none()) {
        return pushed;
    }
    sink.push_events(events);
    let held = sink.escape_held();
    bind_window_input(ctx, popups, incoming, sink.queued_events(), held);
    pushed
}

/// 별도 창 debug 주입 하나의 입구. 맨 앞은 winit 사건과 같은 보류 줄 검사다(`GpuState::hold_debug_input`). 줄이 열려
/// 있으면 그 줄 끝에 서고, 줄이 풀리면 `replay_edit_keys` 가 이 입구로 다시 보낸다. 지나면 키는 팝업 관리자를 가진
/// 창이면 [`inject_window_key`], 아니면 egui 큐에 바로 넣는다. 포인터·글자·붙여넣기는 egui 큐에 바로 넣는다.
#[cfg(debug_assertions)]
pub(crate) fn enter_window_debug<V: crate::view::ui::View + ?Sized>(
    view: &mut V,
    input: crate::view::main::debug_input::DebugInput,
) -> crate::view::main::debug_input::DebugEntered {
    use crate::view::main::debug_input::{
        DebugEntered, DebugInput, push_egui_key, push_egui_pointer, push_egui_text,
    };
    let base = view.base_mut();
    let Some(input) = base.gpu.hold_debug_input(&base.winit, input) else {
        view.mark_dirty();
        return DebugEntered::Held;
    };
    let entered = match input {
        DebugInput::Key {
            key,
            pressed,
            modifiers,
        } => DebugEntered::Key(match view.window_input() {
            Some((gpu, popups)) => {
                let ctx = gpu.egui_ctx.clone();
                inject_window_key(&ctx, popups, gpu, key, pressed, modifiers)
            }
            None => push_egui_key(&mut view.base_mut().gpu, key, pressed, modifiers),
        }),
        DebugInput::Pointer {
            fx,
            fy,
            action,
            modifiers,
            ..
        } => {
            push_egui_pointer(&mut view.base_mut().gpu, fx, fy, action, modifiers);
            DebugEntered::Done(true)
        }
        DebugInput::Text(text) => {
            DebugEntered::Done(push_egui_text(&mut view.base_mut().gpu, &text))
        }
        DebugInput::Clipboard(event) => {
            view.base_mut().gpu.debug_push_egui_events(vec![event]);
            DebugEntered::Done(true)
        }
    };
    if entered.changed() {
        view.mark_dirty();
    }
    entered
}

#[cfg(test)]
#[path = "window_input_tests.rs"]
pub(crate) mod tests;
