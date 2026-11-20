//! 누름·키가 창의 egui 입력 큐에 들어오기 전 입력 몫을 끊는 자리. debug 주입과 GPU 없이 창의 입력 순서를 밟는 시험이
//! 같은 함수를 쓴다. 창 winit 경로는 `MainView::cut_input_segment` 가 같은 `MainViewState::cut_key_frame` 을 부른다.

use crate::adapters::ui::popup::frame_keys::Incoming;

/// 주입한 누름이 거치는 창 하나의 egui 입력 큐. 실제로는 그 창의 `GpuState`이고 시험은 가짜를 쓴다.
pub(crate) trait EguiPressSink {
    /// 다음 패스들에 넣을 egui 입력 큐.
    fn queued_events(&mut self) -> &mut Vec<egui::Event>;
    /// 큐를 지금 자리에서 끊는다(`GpuState::cut_egui_input_here`).
    fn cut_queue_here(&mut self);
}

impl EguiPressSink for crate::gpu::GpuState {
    fn queued_events(&mut self) -> &mut Vec<egui::Event> {
        self.egui_input_events_mut()
    }

    fn cut_queue_here(&mut self) {
        self.cut_egui_input_here();
    }
}

/// 주입한 누름 하나가 포커스 노드를 정한다. 창의 누름과 같은 순서다. 누름을 egui 입력에 넣기 전에 지금 몫을 끊고
/// (`MainViewState::cut_key_frame`, 창은 `MainView::cut_input_segment`), 누름 스냅샷으로 포커스 노드를 옮겨(`take_press_snapshot`) 그 누름의 그리기 몫에
/// 남긴다(`keep_press_snapshot`). 호출자는 이어서 누름 사건을 큐에 넣는다.
pub(crate) fn press_focus(
    state: &mut crate::state::MainViewState,
    core: &crate::core::CoreState,
    sink: &mut impl EguiPressSink,
    pos: egui::Pos2,
    button: egui::PointerButton,
) {
    if state.cut_key_frame(core, sink.queued_events(), Incoming::Press) {
        sink.cut_queue_here();
    }
    let snapshot = state.take_press_snapshot(core, pos, button);
    state.keep_press_snapshot(snapshot);
}

/// 주입한 키·글자 사건 하나가 egui 입력 큐에 들어오기 전에 부른다. 창의 키와 같은 순서다. 지금 몫의 마지막 입력이
/// 누름이면 그 자리에서 끊는다(`MainViewState::cut_key_frame`). 호출자는 이어서 키 사건을 큐에 넣는다.
pub(crate) fn key_focus(
    state: &mut crate::state::MainViewState,
    core: &crate::core::CoreState,
    sink: &mut impl EguiPressSink,
) {
    if state.cut_key_frame(core, sink.queued_events(), Incoming::Key) {
        sink.cut_queue_here();
    }
}
