//! 메인 창의 편집 키(복사·잘라내기·붙여넣기) 판정 입구. host egui 입력칸이 키보드를 가진 동안
//! 바인딩 판정을 egui-winit과 단축키 디스패치보다 먼저 한 번 한다. 보류한 키의 결과는 창의 GPU 상태가
//! 정하고, 넘길 키는 App이 이 창의 평소 이벤트 경로로 재생한다. 계약: `docs/design/policies/key-mapping.md`.

use winit::event::WindowEvent;

use super::MainView;
use crate::adapters::ui::input::edit_key::{EditBindings, EditKey};
use crate::gfx::gpu::edit_keys::{EditKeyOwner, EguiKey};
use crate::runtime::engine_read::EngineRead;
use crate::view::ui::View;

impl MainView {
    /// 편집 키 판정의 결과를 이벤트 경로에 넘긴다. 판정이 키를 가져갔으면 `None`(다시 그리기만
    /// 요청한다), 아니면 이 키가 넘어가는 편집(한 번 판정 표시)이다.
    pub(super) fn edit_key_gate(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
    ) -> Option<Option<EditKey>> {
        match self.judge_edit_key(engine, event) {
            EguiKey::Taken => {
                self.mark_dirty();
                None
            }
            EguiKey::Pass(edit) => Some(Some(edit)),
            EguiKey::Free => Some(None),
        }
    }

    /// 이벤트 하나의 편집 키 판정. App이 재생하는 키는 이미 판정한 편집으로 넘긴다.
    fn judge_edit_key(&mut self, engine: &EngineRead<'_>, event: &WindowEvent) -> EguiKey {
        if let Some(edit) = self.edit_replay.take() {
            return EguiKey::Pass(edit);
        }
        if let Some(judged) = self.judge_plugin_popup_edit_key(engine, event) {
            return judged;
        }
        if let Some(judged) = self.judge_plugin_surface_edit_key(engine, event) {
            return judged;
        }
        let keyboard = matches!(event, WindowEvent::KeyboardInput { .. });
        let target = keyboard && self.edit_key_target(engine);
        let explorer = keyboard
            && !target
            && crate::shortcuts::focused_explorer_surface_id(&self.state, engine).is_some();
        let entry = edit_key_entry(
            target,
            explorer,
            self.base.gpu.egui_input_pending(),
            self.base.gpu.edit_keys_busy(),
        );
        let bindings = || Some(EditBindings::from_settings(&engine.settings.keybindings));
        let window = &self.base.winit;
        let gpu = &mut self.base.gpu;
        match entry {
            EditKeyEntry::Free => EguiKey::Free,
            EditKeyEntry::Judge => {
                gpu.set_edit_bindings(if target { bindings() } else { None });
                gpu.judge_egui_key(window, event, EditKeyOwner::Window)
            }
            EditKeyEntry::Unsettled => {
                gpu.set_edit_bindings(bindings());
                gpu.judge_unsettled_egui_key(window, event)
            }
        }
    }

    /// 키를 host egui 입력칸이 받는지. 오버레이(팝업·모달·주소 띠·무대)가 열렸거나, egui로 그리는
    /// 탐색기 surface의 입력칸이 포커스를 가졌을 때다. 탐색기 파일 목록의 편집 키는 기존 디스패치가 맡는다.
    fn edit_key_target(&self, engine: &EngineRead<'_>) -> bool {
        if self.state.window_keys_to_overlay() {
            return true;
        }
        self.state
            .focused_surface_type(engine)
            .kind_capability(engine, |d| d.consumes_egui_input)
            && crate::shortcuts::focused_explorer_typing(&self.state, engine)
    }
}

/// 편집 키 판정 입구가 키 하나를 보내는 곳.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKeyEntry {
    /// 판정하지 않는다. 기존 경로(탐색기 파일 액션 등)가 받는다.
    Free,
    /// 공통 판정. 대상이 아니면 바인딩 없이 판정해 보류 중인 키 뒤에 줄만 세운다.
    Judge,
    /// 탐색기 입력칸 여부를 아직 모른다. 직전 렌더의 포커스 값은 그 뒤 egui 입력(입력칸 클릭 등)이
    /// 바꿀 수 있으므로, 그 입력이 처리된 뒤 이 키를 새로 판정하게 GPU 판정에 맡긴다.
    Unsettled,
}

/// `target`: 키를 host egui 입력칸이 받는다(직전 렌더 기준). `explorer`: 대상이 아닌 탐색기 surface가
/// 포커스를 가졌다. `pending`: egui가 아직 처리하지 않은 입력이 있다. `busy`: 보류한 키가 있다.
pub(super) fn edit_key_entry(
    target: bool,
    explorer: bool,
    pending: bool,
    busy: bool,
) -> EditKeyEntry {
    if target {
        EditKeyEntry::Judge
    } else if explorer && pending {
        EditKeyEntry::Unsettled
    } else if busy {
        EditKeyEntry::Judge
    } else {
        EditKeyEntry::Free
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 탐색기 목록에 포커스가 있던 직후 미처리 입력(입력칸 클릭)이 있으면 GPU 판정에 가고, 없으면 기존
    /// 경로로 간다. 입력칸이 포커스를 가졌으면 늘 공통 판정이다.
    #[test]
    fn an_explorer_key_after_unprocessed_input_reaches_the_judgment() {
        assert_eq!(
            edit_key_entry(false, true, true, false),
            EditKeyEntry::Unsettled
        );
        assert_eq!(
            edit_key_entry(false, true, false, false),
            EditKeyEntry::Free
        );
        assert_eq!(
            edit_key_entry(true, false, false, false),
            EditKeyEntry::Judge
        );
        assert_eq!(
            edit_key_entry(false, false, true, false),
            EditKeyEntry::Free
        );
        assert_eq!(
            edit_key_entry(false, false, false, true),
            EditKeyEntry::Judge
        );
    }
}
