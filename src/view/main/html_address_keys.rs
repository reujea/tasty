//! HTML 주소 입력(host egui)이 키보드 포커스를 가진 동안 사용자 편집·주소창 바인딩을 적용한다.
//! egui-winit은 자체 기본 키로 Copy/Cut/Paste를 만들고 `KeybindingSettings`를 읽지 않는다.
//! 바인딩한 키가 만든 egui 이벤트를 고른 동작 하나로 바꿔 한 키가 한 번만 실행되게 한다. 계약: `docs/surfaces/html/index.md`.

use winit::event::{ElementState, WindowEvent};

use super::MainView;
use crate::runtime::engine_read::EngineRead;
use crate::view::ui::View as _;
use crate::webview::WebViewEditAction;

/// 주소 입력에서 사용자 바인딩이 가리키는 동작.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddressKeyAction {
    Edit(WebViewEditAction),
    FocusAddress,
    Refresh,
}

/// 주소 입력이 포커스를 가진 동안 한 키가 실행할 동작 하나. 여러 바인딩이 겹치면 편집, 주소창 포커스,
/// 공통 새로고침 순으로 하나만 고른다. 이벤트 바꾸기와 새로고침 키 처리가 같은 판정을 쓴다.
pub(super) fn address_key_action(
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
) -> Option<AddressKeyAction> {
    let matches = |field: &str| {
        crate::shortcuts::matches_any_binding(kb.get_bindings(field).unwrap_or(&[]), key, mods)
    };
    WebViewEditAction::ALL
        .into_iter()
        .find(|a| matches(a.field_id()))
        .map(AddressKeyAction::Edit)
        .or_else(|| matches("explorer_focus_address").then_some(AddressKeyAction::FocusAddress))
        .or_else(|| {
            crate::shortcuts::matches_any_binding(&kb.refresh, key, mods)
                .then_some(AddressKeyAction::Refresh)
        })
}

impl MainView {
    /// egui에 키를 넘긴 뒤 부른다. `from`은 넘기기 전 egui 이벤트 수다.
    pub(super) fn remap_html_address_keys(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
        from: usize,
    ) {
        let Some(sid) = self.state.html_address.focused_surface() else {
            return;
        };
        let WindowEvent::KeyboardInput { event: key, .. } = event else {
            return;
        };
        if key.state != ElementState::Pressed {
            return;
        }
        let mods = self.base.state.modifiers;
        let lookup =
            crate::shortcuts::shortcut_lookup_key(&key.logical_key, &key.physical_key, mods);
        // 새로고침은 키 처리 단계(`try_consume_address_refresh_key`)가 실행하고 egui 효과는 그대로 둔다.
        let action = match address_key_action(&engine.settings.keybindings, &lookup, mods) {
            None | Some(AddressKeyAction::Refresh) => return,
            Some(action) => action,
        };
        if action == AddressKeyAction::FocusAddress {
            self.state.html_address.request_focus(sid);
        }
        let gpu = &mut self.base.gpu;
        // The key's own paste (a native Wayland read in flight) loses to the binding as well.
        gpu.cancel_pending_paste();
        let clipboard = (action == AddressKeyAction::Edit(WebViewEditAction::Paste))
            .then(|| gpu.egui_clipboard_text())
            .flatten();
        remap_events(gpu.egui_input_events_mut(), from, action, clipboard);
        self.mark_dirty();
    }
}

/// `events[from..]`은 이번 키 하나가 만든 egui 이벤트다. 사용자 바인딩이 이기면 그 키의 원래 효과
/// (글자 입력, egui 기본 Copy/Cut/Paste, 삭제·이동 같은 Key 처리)를 모두 버리고 고른 동작 하나만 넣는다.
fn remap_events(
    events: &mut Vec<egui::Event>,
    from: usize,
    action: AddressKeyAction,
    clipboard: Option<String>,
) {
    events.truncate(from.min(events.len()));
    let AddressKeyAction::Edit(edit) = action else {
        return;
    };
    // TextEdit가 전체 선택·실행 취소·다시 실행으로 처리하는 키 이벤트.
    let command_key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    match edit {
        WebViewEditAction::Copy => events.push(egui::Event::Copy),
        WebViewEditAction::Cut => events.push(egui::Event::Cut),
        WebViewEditAction::Paste => {
            if let Some(text) = clipboard.filter(|t| !t.is_empty()) {
                events.push(egui::Event::Paste(text));
            }
        }
        WebViewEditAction::SelectAll => {
            events.push(command_key(egui::Key::A, egui::Modifiers::COMMAND));
        }
        WebViewEditAction::Undo => events.push(command_key(egui::Key::Z, egui::Modifiers::COMMAND)),
        WebViewEditAction::Redo => events.push(command_key(
            egui::Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn run(
        tail: Vec<egui::Event>,
        action: AddressKeyAction,
        clip: Option<&str>,
    ) -> Vec<egui::Event> {
        let mut ev = vec![egui::Event::PointerGone];
        ev.extend(tail);
        remap_events(&mut ev, 1, action, clip.map(str::to_string));
        assert_eq!(ev[0], egui::Event::PointerGone, "이전 이벤트는 그대로 둔다");
        ev.split_off(1)
    }

    /// 새로고침과 겹친 편집·주소창 포커스 키는 그쪽 하나만 실행하고, 겹치지 않은 F5는 새로고침이다.
    #[test]
    fn an_overlapping_key_resolves_to_edit_or_focus_before_refresh() {
        use winit::keyboard::{Key, ModifiersState, NamedKey};
        let f5 = Key::Named(NamedKey::F5);
        let none = ModifiersState::empty();
        let mut kb = crate::settings::KeybindingSettings {
            refresh: vec!["f5".into()],
            explorer_refresh: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            address_key_action(&kb, &f5, none),
            Some(AddressKeyAction::Refresh)
        );
        kb.copy = vec!["f5".into()];
        assert_eq!(
            address_key_action(&kb, &f5, none),
            Some(AddressKeyAction::Edit(WebViewEditAction::Copy))
        );
        kb.copy.clear();
        kb.explorer_focus_address = vec!["f5".into()];
        assert_eq!(
            address_key_action(&kb, &f5, none),
            Some(AddressKeyAction::FocusAddress)
        );
        assert_eq!(
            address_key_action(&kb, &Key::Character("q".into()), none),
            None
        );
    }

    /// egui 기본 키(Ctrl+C)에 같은 copy 바인딩이면 Copy 한 번만 남는다.
    #[test]
    fn a_bound_default_key_copies_once() {
        let out = run(
            vec![
                egui::Event::Copy,
                key(egui::Key::C, egui::Modifiers::COMMAND),
            ],
            AddressKeyAction::Edit(WebViewEditAction::Copy),
            None,
        );
        assert_eq!(out, vec![egui::Event::Copy]);
    }

    /// 기본 키가 아닌 바인딩(Alt+C)은 Copy만 남고 그 키의 글자 `c`는 입력하지 않는다.
    #[test]
    fn a_custom_binding_copies_and_types_nothing() {
        let out = run(
            vec![
                key(egui::Key::C, egui::Modifiers::ALT),
                egui::Event::Text("c".into()),
            ],
            AddressKeyAction::Edit(WebViewEditAction::Copy),
            None,
        );
        assert_eq!(out, vec![egui::Event::Copy]);
    }

    /// egui 기본 잘라내기 키(Ctrl+X)를 paste에 묶으면 Cut 없이 Paste만 한 번 한다.
    #[test]
    fn a_default_cut_key_bound_to_paste_only_pastes() {
        let out = run(
            vec![
                egui::Event::Cut,
                key(egui::Key::X, egui::Modifiers::COMMAND),
            ],
            AddressKeyAction::Edit(WebViewEditAction::Paste),
            Some("xyz"),
        );
        assert_eq!(out, vec![egui::Event::Paste("xyz".into())]);
        let out = run(
            vec![key(egui::Key::X, egui::Modifiers::COMMAND)],
            AddressKeyAction::Edit(WebViewEditAction::Paste),
            None,
        );
        assert!(
            out.is_empty(),
            "빈 클립보드는 아무것도 하지 않는다: {out:?}"
        );
    }

    /// egui 기본 붙여넣기 키(Ctrl+V)를 select_all에 묶으면 붙여넣지 않고 전체 선택만 한다.
    #[test]
    fn a_default_paste_key_bound_to_select_all_only_selects() {
        let out = run(
            vec![
                egui::Event::Paste("clip".into()),
                key(egui::Key::V, egui::Modifiers::COMMAND),
            ],
            AddressKeyAction::Edit(WebViewEditAction::SelectAll),
            Some("clip"),
        );
        assert_eq!(out, vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    }

    /// 편집 키(Ctrl+Backspace)를 copy에 묶으면 단어 삭제 Key를 버리고 복사만 한다.
    #[test]
    fn an_editing_key_bound_to_copy_does_not_delete() {
        let out = run(
            vec![key(egui::Key::Backspace, egui::Modifiers::COMMAND)],
            AddressKeyAction::Edit(WebViewEditAction::Copy),
            None,
        );
        assert_eq!(out, vec![egui::Event::Copy]);
    }

    /// 실행 취소 바인딩(Alt+U)은 글자 대신 TextEdit 실행 취소 키 하나가 되고, 다시 실행 바인딩에
    /// egui 기본 실행 취소 키(Ctrl+Z)를 주면 실행 취소 없이 다시 실행만 한다.
    #[test]
    fn undo_and_redo_bindings_become_one_text_edit_key() {
        let out = run(
            vec![
                key(egui::Key::U, egui::Modifiers::ALT),
                egui::Event::Text("u".into()),
            ],
            AddressKeyAction::Edit(WebViewEditAction::Undo),
            None,
        );
        assert_eq!(out, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        let out = run(
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
            AddressKeyAction::Edit(WebViewEditAction::Redo),
            None,
        );
        assert_eq!(
            out,
            vec![key(
                egui::Key::Z,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
            )]
        );
    }

    /// 주소창 이동 키는 그 키의 원래 효과를 남기지 않는다(전체 선택은 띠가 한다).
    #[test]
    fn the_focus_key_drops_the_key_effects() {
        let out = run(
            vec![
                egui::Event::Copy,
                key(egui::Key::L, egui::Modifiers::ALT),
                egui::Event::Text("l".into()),
            ],
            AddressKeyAction::FocusAddress,
            Some("x"),
        );
        assert!(out.is_empty(), "{out:?}");
    }
}
