//! 별도 창(설정·플러그인·프리셋)의 입력 패스. 메인 창과 같은 순서 함수(`gfx::gpu::input_pass::run_input_pass`)로 egui
//! 패스를 열고 닫는다(`GpuState::run_window_pass`). 사건마다 단계 3~5(`settle_window_input`)가 egui 입력에 넣기 전에 사건
//! 시점 노드로 입력을 끊고 누름을 적용한다. 몫은 관리자가 가진다(`PopupManager::cut_window_segment`). 계약:
//! `docs/architecture/input-layer.md`의 "별도 창 입력 패스".

use crate::adapters::ui::fullscreen::StageId;
use crate::adapters::ui::popup::PopupManager;
use crate::adapters::ui::popup::escape_step::WindowPassKeys;
use crate::adapters::ui::popup::frame_keys::{Incoming, InputCuts};
use crate::gfx::gpu::input_pass::InputPassState;
use crate::view::main::input_segment::EguiPressSink;

/// 팝업 관리자를 가진 별도 창 상태.
pub(crate) trait WindowPopups {
    fn window_popups(&mut self) -> &mut PopupManager;
}

/// 별도 창 패스 하나의 상태. `keys` 는 `run` 전에 정한 그 패스 키 값이다(`GpuState::run_window_pass`).
pub(crate) struct WindowPass<'a, T> {
    pub(crate) state: &'a mut T,
    pub(crate) keys: WindowPassKeys,
}

impl<'a, T: WindowPopups> WindowPass<'a, T> {
    pub(crate) fn new(state: &'a mut T) -> Self {
        Self {
            state,
            keys: WindowPassKeys::default(),
        }
    }

    /// `run` 전에 이번 패스 키 값을 정한다. 결속이 없는 몫의 키 주인은 지금 노드이고, 표지는 이번 몫 것이다.
    pub(crate) fn begin_run(&mut self, ctx: &egui::Context, marks: Vec<tasty_key_match::KeyMods>) {
        self.keys = WindowPassKeys {
            owner: self.state.window_popups().window_key_node(ctx).popup(),
            marks,
        };
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

/// 별도 창 사건이 egui 입력 큐에 들어오는 종류. 누름·키·IME 가 아니면 끊지 않는다.
pub(crate) fn incoming_of(event: &winit::event::WindowEvent) -> Option<Incoming> {
    use winit::event::{ElementState, WindowEvent};
    match event {
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            ..
        } => Some(Incoming::Press),
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

#[cfg(test)]
#[path = "window_input_tests.rs"]
mod tests;
