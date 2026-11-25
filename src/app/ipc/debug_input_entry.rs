//! debug egui 입력 주입(키·포인터·글자·붙여넣기)의 App 입구. 맨 앞은 winit 사건의 창 입력(`route_input_steps`)과 같은
//! 두 줄 검사다. 줄에 선 주입은 winit 사건과 도착한 순서대로 서고, 줄이 풀리면 이 입구를 다시 지난다.
//! 사용자 입력 재현이므로 release에는 두지 않는다.

use winit::window::WindowId;

use crate::app::App;
use crate::view::main::debug_input::{DebugEntered, DebugInput};
use crate::view::ui::View;

impl App {
    /// `id` 창의 debug 주입 입구. winit 사건의 창 입력(`route_input_steps`)과 같은 검사를 같은 순서로 한다: plugin 편집
    /// 보류 줄(`queue_behind_plugin_edit`, 붙여넣기 대기 중의 누름은 `interrupt_awaits`로 대기를 끝낸다), 그다음 창의
    /// 보류·정착 줄(`hold_window_input`). 줄에 서면 `Held` 다. 줄이 풀리면 재생(`replay_edit_steps`, 별도 창은
    /// `replay_edit_keys`)이 이 입구로 다시 보낸다. 지나면 메인 창은 `MainView::enter_debug_input`, 별도 창은
    /// `window_input::enter_window_debug`(그 창의 앞 검사부터)로 넣는다. 창이 없으면 `None`.
    pub(crate) fn enter_debug_input(
        &mut self,
        id: WindowId,
        input: DebugInput,
    ) -> Option<DebugEntered> {
        let main = self.view.views.get(&id)?.as_main().is_some();
        if self.queue_behind_plugin_edit(id, &input, std::time::Instant::now()) {
            return Some(DebugEntered::Held);
        }
        if !main {
            let view = self.view.views.get_mut(&id)?;
            return Some(crate::view::window_input::enter_window_debug(
                view.as_mut(),
                input,
            ));
        }
        if self.hold_window_input(id, &input) {
            if let Some(view) = self.view.views.get_mut(&id) {
                view.mark_dirty();
            }
            return Some(DebugEntered::Held);
        }
        let (w, engine) = self.engines_mut().window_pair(id)?;
        let entered = w.enter_debug_input(&engine.read(), input);
        if entered.changed() {
            w.mark_dirty();
        }
        Some(entered)
    }
}
