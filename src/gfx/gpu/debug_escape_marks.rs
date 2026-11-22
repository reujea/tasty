//! debug 키 주입이 별도 창에 넣은 Esc 누름에도 실제 키처럼 사건 시점 수식키 표지를 쌓는다. 실제 키는
//! `handle_egui_event_for_window` 가 egui 입력을 적재하면서 같은 일을 한다(팝업 층 Esc 판정, `escape_step`).

use super::GpuState;

impl GpuState {
    /// egui 입력 큐 `from` 이후에 든 Esc 누름마다 `mods` 표지를 쌓는다. 다음 입력을 꺼낼 때 그 패스 표지가 된다.
    pub fn debug_load_escape_marks(&mut self, from: usize, mods: tasty_key_match::KeyMods) {
        self.loaded_escape_marks
            .extend(crate::adapters::ui::popup::body_keys::escape_marks_in(
                &self.egui_state.egui_input().events,
                from,
                mods,
            ));
    }
}
