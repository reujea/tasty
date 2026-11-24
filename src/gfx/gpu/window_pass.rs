//! 별도 창(설정·플러그인·프리셋)의 egui 패스 주체. 메인 창 패스(`MainPass`)와 같이 끊은 자리로 입력을 나눠 꺼내고,
//! `run` 전에 그 몫의 노드를 창 상태에 넘긴다(`view::window_input::WindowPass`).

use winit::window::Window;

use super::GpuState;
use super::input_pass::{InputPassHost, run_input_pass};
use crate::adapters::ui::popup::frame_keys::InputCuts;
use crate::view::window_input::{WindowPass, WindowPopups};

impl GpuState {
    /// 별도 창의 egui 패스 하나. 패스 하나는 입력 몫 하나다(`run_main_pass` 와 같은 순서 함수). 끊은 자리가 남았으면
    /// 다음 패스를 요청한다.
    pub(crate) fn run_window_pass<'a, T: WindowPopups>(
        &mut self,
        window: &Window,
        state: &'a mut T,
        ui: impl FnMut(&egui::Context, &mut WindowPass<'a, T>),
    ) -> egui::FullOutput {
        let mut pass = WindowPass::new(state);
        run_input_pass(&mut WindowHost { gpu: self, window }, &mut pass, |_| {}, ui)
    }
}

/// 별도 창 패스의 주체.
struct WindowHost<'a> {
    gpu: &'a mut GpuState,
    window: &'a Window,
}

impl<'s, T: WindowPopups> InputPassHost<WindowPass<'s, T>> for WindowHost<'_> {
    fn escape_held(&self) -> bool {
        self.gpu.egui_escape_held()
    }

    fn held_pastes(&mut self) -> &mut super::text_selection::HeldPastes {
        &mut self.gpu.held_pastes
    }

    fn take_input(&mut self) -> egui::RawInput {
        self.gpu.take_pass_input(self.window, true)
    }

    fn egui_ctx(&self) -> &egui::Context {
        &self.gpu.egui_ctx
    }

    fn input_dropped(&mut self) {
        self.gpu.edit_keys.input_dropped();
    }

    fn focus_tag(&self) -> u64 {
        self.gpu.focus_tag()
    }

    fn run(
        &mut self,
        raw_input: egui::RawInput,
        state: &mut WindowPass<'s, T>,
        mut ui: impl FnMut(&egui::Context, &mut WindowPass<'s, T>),
    ) -> egui::FullOutput {
        state.begin_run(&self.gpu.egui_ctx);
        self.gpu.egui_ctx.run(raw_input, |ctx| ui(ctx, state))
    }

    fn cuts_and_raw_left(&mut self) -> (&mut InputCuts, usize) {
        let raw_left = self.gpu.egui_state.egui_input().events.len();
        (&mut self.gpu.egui_input_cuts, raw_left)
    }
}
