//! 창의 입력 순서를 GPU 없이 밟는 시험 도우미. 누름은 debug 주입과 같은 함수(`input_segment::press_focus`)로 지금 몫을
//! 끊고 포커스 노드를 옮긴다. 패스 입력은 egui 패스와 같은 순서로 꺼낸다(`PopupManager::begin_key_pass` 뒤 큐를 꺼내
//! 앞 끊은 자리에서 나눔, `InputCuts::split_pass`). 키 사건의 게이트 판정(`window_feeds_egui`·`window_escape`)은 각
//! 시험이 창과 같은 함수로 하고 egui 로 보낼 사건만 `push` 한다.

use crate::adapters::ui::popup::frame_keys::InputCuts;
use crate::state::RequestContext;
use crate::view::main::input_segment::{EguiPressSink, press_focus};

/// 창 하나의 egui 입력 큐와 끊은 자리.
#[derive(Default)]
pub(crate) struct WindowInput {
    queue: Vec<egui::Event>,
    cuts: InputCuts,
}

impl EguiPressSink for WindowInput {
    fn queued_events(&mut self) -> &mut Vec<egui::Event> {
        &mut self.queue
    }

    fn cut_queue_here(&mut self) {
        self.cuts.cut_at(self.queue.len());
    }
}

impl WindowInput {
    /// 창이 egui 로 보낸 사건 하나를 큐에 싣는다.
    pub(crate) fn push(&mut self, event: egui::Event) {
        self.queue.push(event);
    }

    /// 창이 처리할 Esc 를 고를 큐(`MainViewState::window_escape` 의 `pending`).
    pub(crate) fn queue_mut(&mut self) -> &mut Vec<egui::Event> {
        &mut self.queue
    }

    /// 주입한 왼쪽 버튼 누름. 포커스 처리 뒤 포인터 이동과 누름 사건을 싣는다(`push_egui_pointer_at` 순서).
    pub(crate) fn press(
        &mut self,
        state: &mut RequestContext,
        core: &crate::core::CoreState,
        pos: egui::Pos2,
    ) {
        press_focus(state, core, self, pos, egui::PointerButton::Primary);
        self.queue.push(egui::Event::PointerMoved(pos));
        self.queue.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
    }

    /// 다음 egui 패스의 입력.
    pub(crate) fn take_pass(
        &mut self,
        state: &mut RequestContext,
        ctx: &egui::Context,
    ) -> Vec<egui::Event> {
        state
            .popups
            .begin_key_pass(ctx.input(|i| i.key_down(egui::Key::Escape)));
        let mut events = std::mem::take(&mut self.queue);
        self.cuts.split_pass(&mut events, &mut self.queue);
        events
    }

    /// 아직 꺼내지 않은 사건이 없는지.
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
