//! 별도 창(설정·플러그인·프리셋)의 입력 패스. 메인 창과 같은 순서 함수(`gfx::gpu::input_pass::run_input_pass`)로 egui
//! 패스를 열고 닫는다(`GpuState::run_window_pass`). 사건마다 단계 3~5(`settle_window_input`)가 egui 입력에 넣기 전에 사건
//! 시점 노드로 입력을 끊고 누름을 적용한다. 몫은 관리자가 가진다(`PopupManager::cut_window_segment`). 계약:
//! `docs/architecture/input-layer.md`의 "별도 창 입력 패스".

use crate::adapters::ui::fullscreen::StageId;
use crate::adapters::ui::popup::PopupManager;
use crate::adapters::ui::popup::frame_keys::{Incoming, InputCuts};
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
/// 같은 판정(`key_class::is_bare_escape_press`)으로 가른다. `mods` 는 사건 시점 창의 winit 수식키, `egui_mods` 는 그 창
/// egui-winit 의 수식키다.
pub(crate) fn incoming_of(
    event: &winit::event::WindowEvent,
    mods: winit::keyboard::ModifiersState,
    egui_mods: egui::Modifiers,
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
                egui_mods,
            }) =>
        {
            Some(Incoming::Escape)
        }
        WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) => Some(Incoming::Key),
        _ => None,
    }
}

/// 단계 3~5: 사건(`incoming`)을 egui 입력 큐에 넣기 전에 사건 시점 노드로 끊고, 누름이면 그 자리에서 포커스를 옮긴다.
/// 누름 좌표는 큐에 남은 마지막 포인터 이동, 없으면 egui 가 아는 마지막 좌표다.
pub(crate) fn settle_window_input(
    sink: &mut impl EguiPressSink,
    ctx: &egui::Context,
    popups: &mut PopupManager,
    incoming: Incoming,
) {
    let cuts = popups.cut_window_segment(ctx, sink.queued_events(), incoming);
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
            popups.apply_window_escape(ctx, pending, escape_held);
        }
        Incoming::Key => popups.bind_window_key(ctx),
        Incoming::Press => {}
    }
}

/// 별도 창 debug 키 주입 하나. winit 키와 같은 사건 입구를 지난다: 끊기(`settle_window_input`) → egui 큐 적재
/// (`debug_input::push_egui_key`) → 결속과 bare Esc 적용(`bind_window_input`). bare Esc 인지는 메인 창 debug 주입과 같이
/// 주입이 쥔 egui 수식키와 창이 지금 누른 것으로 보는 실물 Win·Super 로 정한다(`RouteKey::bare_escape`). 매핑 불가 키면
/// 끊지도 넣지도 않고 `None`.
#[cfg(debug_assertions)]
pub(crate) fn inject_window_key<S>(
    ctx: &egui::Context,
    popups: &mut PopupManager,
    sink: &mut S,
    key_name: &str,
    pressed: bool,
    modifiers: Option<egui::Modifiers>,
) -> Option<crate::view::main::debug_input::PushedKey>
where
    S: EguiPressSink + crate::view::main::debug_input::EguiKeySink,
{
    let key = egui::Key::from_name(key_name)?;
    let bare_escape = pressed
        && crate::adapters::ui::input::key_route::RouteKey {
            key,
            modifiers: modifiers.unwrap_or_else(|| sink.held_modifiers()),
            super_held: sink.super_held(),
        }
        .bare_escape();
    let incoming = if bare_escape {
        Incoming::Escape
    } else {
        Incoming::Key
    };
    settle_window_input(sink, ctx, popups, incoming);
    let pushed = crate::view::main::debug_input::push_egui_key(sink, key_name, pressed, modifiers)?;
    let held = sink.escape_held();
    bind_window_input(ctx, popups, incoming, sink.queued_events(), held);
    Some(pushed)
}

#[cfg(test)]
#[path = "window_input_tests.rs"]
pub(crate) mod tests;
