//! 메인 창이 가진 키 사건 단계(계약 E0). 순서는 `crate::view::key_event::run_key_event`가 정하고, 여기는
//! 단계마다 메인 창이 부르는 함수만 둔다. 단계 번호와 범위는 `docs/architecture/input-layer.md`.

use winit::event::WindowEvent;

use super::{MainView, modal_blocks_input};
use crate::adapters::ui::input::edit_key::EditKey;
use crate::runtime::engine_read::EngineRead;
use crate::view::ViewAction;
use crate::view::key_event::KeyEventSteps;

/// 메인 창의 사건 하나. 메뉴 닫기 클릭과 OS 테두리 리사이즈 판정은 egui 적재 앞에서 정해 단계 11이 같은 값을 쓴다.
pub(super) struct MainKeySteps<'a, 'e> {
    view: &'a mut MainView,
    engine: &'a EngineRead<'e>,
    modal_active: bool,
    menu_dismiss_swallow: bool,
    os_resize_press: bool,
}

impl<'a, 'e> MainKeySteps<'a, 'e> {
    pub(super) fn new(
        view: &'a mut MainView,
        engine: &'a EngineRead<'e>,
        modal_active: bool,
    ) -> Self {
        Self {
            view,
            engine,
            modal_active,
            menu_dismiss_swallow: false,
            os_resize_press: false,
        }
    }
}

impl KeyEventSteps<WindowEvent> for MainKeySteps<'_, '_> {
    fn ends_before_edit_judgment(&mut self, event: &WindowEvent) -> bool {
        if self
            .view
            .outbound_drag_event(self.engine, event, self.modal_active)
        {
            return true;
        }
        // If a modal is active, block all input events before they reach egui.
        // Only allow non-input events (resize, redraw, scale factor, focus) through.
        modal_blocks_input(self.modal_active, event)
    }

    fn settle_key_node(&mut self, event: &WindowEvent) {
        self.view.normalize_focus_on_key(self.engine, event);
        self.view.decide_key_receiver(self.engine, event);
        // 끊었으면 이 사건은 새 몫의 첫 사건이다. 끊은 몫의 결속은 새 몫의 판정에 쓰지 않으므로 새 몫에서 다시 정한다.
        if self.view.cut_input_segment(self.engine, event) > 0 {
            self.view.decide_key_receiver(self.engine, event);
        }
    }

    // 편집 바인딩 판정은 egui-winit과 단축키 디스패치보다 먼저 한 번 한다.
    fn judge_edit_key(&mut self, event: &WindowEvent) -> Option<Option<EditKey>> {
        self.view.edit_key_gate(self.engine, event)
    }

    fn intercepts_after_edit_judgment(&mut self, event: &WindowEvent) -> bool {
        #[cfg(target_os = "linux")]
        if self.view.consume_explorer_ime_key(self.engine, event) {
            return true;
        }
        // 팝업·host 위젯의 키 입력만 egui에 전달하고 나머지는 중앙 디스패처로 보낸다.
        self.view.route_tutorial_input(event)
    }

    fn feed_egui(&mut self, event: &WindowEvent, judged: Option<EditKey>) -> Option<(bool, bool)> {
        let is_keyboard_event = matches!(
            event,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
        );
        let is_modifiers_event = matches!(event, WindowEvent::ModifiersChanged(_));
        let is_redraw_event = matches!(event, WindowEvent::RedrawRequested);

        // 메뉴를 닫는 클릭은 누름부터 뗌까지 egui와 Tasty 모두에 전달하지 않는다.
        // 두 경로가 같은 판단을 쓰도록 egui 전달 전에 한 번 판정한다.
        self.menu_dismiss_swallow = self.view.begin_menu_dismiss_swallow(event);
        self.os_resize_press = self
            .view
            .os_resize_owns_event(event, self.menu_dismiss_swallow);

        Some(if is_redraw_event {
            // RedrawRequested를 egui에 넣으면 다시 그리기를 반복 요청하므로 제외한다.
            (false, false)
        } else if is_keyboard_event {
            self.view
                .feed_egui_keyboard(self.engine, event, judged.is_some())
        } else if is_modifiers_event {
            let view = &mut *self.view;
            let (_, repaint) = view.base.gpu.handle_egui_event(&view.base.winit, event);
            (false, repaint)
        } else if self.menu_dismiss_swallow || self.os_resize_press {
            (false, true)
        } else {
            self.view.feed_egui_pointer_or_other(event)
        })
    }

    fn run_receiver(
        &mut self,
        event: WindowEvent,
        judged: Option<EditKey>,
        egui: (bool, bool),
    ) -> ViewAction {
        self.view.after_egui_feed(
            self.engine,
            event,
            judged,
            egui,
            self.menu_dismiss_swallow,
            self.os_resize_press,
        )
    }
}
