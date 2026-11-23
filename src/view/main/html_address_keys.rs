//! HTML 주소 입력(host egui)이 키보드 포커스를 가진 동안 사용자 편집·주소창 바인딩을 적용한다.
//! 복사·잘라내기·붙여넣기는 공통 편집 키 판정(`edit_keys.rs`)이 egui-winit보다 먼저 맡는다. 여기서는
//! 그 밖의 바인딩(전체 선택·실행 취소·다시 실행·주소창 포커스)이 만든 egui 이벤트를 고른 동작 하나로
//! 바꿔 한 키가 한 번만 실행되게 한다. 계약: `docs/surfaces/html/index.md`.

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
/// `clipboard_judged`면 공통 판정이 이 키의 클립보드 편집을 이미 판정해 넘겼으므로(동작할 것이 없었다)
/// 클립보드 편집은 고르지 않고 그 밖의 바인딩을 그대로 판정한다.
pub(super) fn address_key_action(
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
    clipboard_judged: bool,
) -> Option<AddressKeyAction> {
    let matches = |field: &str| {
        crate::shortcuts::matches_any_binding(kb.get_bindings(field).unwrap_or(&[]), key, mods)
    };
    let edit = if clipboard_judged {
        crate::adapters::ui::input::shortcuts::edit_binding_after_clipboard(kb, key, mods)
    } else {
        crate::adapters::ui::input::shortcuts::edit_binding_for_key(kb, key, mods)
    };
    edit.map(AddressKeyAction::Edit)
        .or_else(|| matches("explorer_focus_address").then_some(AddressKeyAction::FocusAddress))
        .or_else(|| {
            crate::shortcuts::matches_any_binding(&kb.refresh, key, mods)
                .then_some(AddressKeyAction::Refresh)
        })
}

/// 주소 띠 노드 키맵 수신 판정: 공통 편집 판정 전후 두 값 가운데 하나라도 동작이 있으면 주소 띠에서 끝나는
/// 키다. 동작은 지금처럼 편집 판정 뒤 `address_key_action`이 정한다.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the window key path calls it from the receiver commit on"
    )
)]
pub(super) fn address_keymap_receives(
    kb: &crate::settings::KeybindingSettings,
    key: &winit::keyboard::Key,
    mods: winit::keyboard::ModifiersState,
) -> bool {
    [false, true]
        .into_iter()
        .any(|judged| address_key_action(kb, key, mods, judged).is_some())
}

impl MainView {
    /// egui에 키를 넘긴 뒤 부른다. `from`은 넘기기 전 egui 이벤트 수다. `clipboard_judged`는
    /// [`address_key_action`]의 같은 인자다.
    pub(super) fn remap_html_address_keys(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
        from: usize,
        clipboard_judged: bool,
    ) {
        let Some(sid) = self.state.html_address.focused_surface() else {
            return;
        };
        // 주소 띠가 포커스 노드일 때만 바꾼다. 팝업에 묶인 키는 건드리지 않는다.
        if self.state.focus_node(engine) != crate::state::focus_node::FocusNode::HtmlAddress(sid) {
            return;
        }
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
        // 클립보드 편집은 공통 판정을 이미 거쳤다. 넘긴 키는 원래 효과를 그대로 둔다.
        let keybindings = &engine.settings.keybindings;
        let action = match address_key_action(keybindings, &lookup, mods, clipboard_judged) {
            None | Some(AddressKeyAction::Refresh) => return,
            Some(AddressKeyAction::Edit(edit)) if is_clipboard_edit(edit) => return,
            Some(action) => action,
        };
        if action == AddressKeyAction::FocusAddress {
            self.state.html_address.request_focus(sid);
        }
        let gpu = &mut self.base.gpu;
        // The key's own paste (a native Wayland read in flight) loses to the binding as well.
        gpu.cancel_pending_paste();
        remap_events(gpu.egui_input_events_mut(), from, action);
        self.mark_dirty();
    }
}

fn is_clipboard_edit(edit: WebViewEditAction) -> bool {
    matches!(
        edit,
        WebViewEditAction::Copy | WebViewEditAction::Cut | WebViewEditAction::Paste
    )
}

/// `events[from..]`은 이번 키 하나가 만든 egui 이벤트다. 사용자 바인딩이 이기면 그 키의 원래 효과
/// (글자 입력, egui 기본 Copy/Cut/Paste, 삭제·이동 같은 Key 처리)를 모두 버리고 고른 동작 하나만 넣는다.
/// 클립보드 편집은 공통 판정이 맡으므로 여기 오지 않는다.
fn remap_events(events: &mut Vec<egui::Event>, from: usize, action: AddressKeyAction) {
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
        WebViewEditAction::Copy | WebViewEditAction::Cut | WebViewEditAction::Paste => {}
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

    /// 주소 띠 키맵 수신 판정: 편집 판정 전후 어느 쪽이든 동작이 있는 키(새로고침, 편집)는 받고, 바인딩이 없는
    /// 키는 받지 않는다.
    #[test]
    fn the_address_keymap_receives_keys_with_an_action_either_side_of_edit_judgment() {
        use winit::keyboard::{Key, ModifiersState, NamedKey};
        let kb = crate::settings::KeybindingSettings::default();
        let f5 = Key::Named(NamedKey::F5);
        assert!(address_keymap_receives(&kb, &f5, ModifiersState::empty()));
        let a = Key::Character("a".into());
        assert!(!address_keymap_receives(&kb, &a, ModifiersState::empty()));
        for judged in [false, true] {
            assert_eq!(
                address_key_action(&kb, &f5, ModifiersState::empty(), judged).is_some(),
                address_keymap_receives(&kb, &f5, ModifiersState::empty())
            );
        }
    }

    fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn run(tail: Vec<egui::Event>, action: AddressKeyAction) -> Vec<egui::Event> {
        let mut ev = vec![egui::Event::PointerGone];
        ev.extend(tail);
        remap_events(&mut ev, 1, action);
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
            address_key_action(&kb, &f5, none, false),
            Some(AddressKeyAction::Refresh)
        );
        kb.copy = vec!["f5".into()];
        assert_eq!(
            address_key_action(&kb, &f5, none, false),
            Some(AddressKeyAction::Edit(WebViewEditAction::Copy))
        );
        kb.copy.clear();
        kb.explorer_focus_address = vec!["f5".into()];
        assert_eq!(
            address_key_action(&kb, &f5, none, false),
            Some(AddressKeyAction::FocusAddress)
        );
        assert_eq!(
            address_key_action(&kb, &Key::Character("q".into()), none, false),
            None
        );
    }

    /// 공통 판정이 넘긴 복사 키(복사할 선택이 없음)가 새로고침에도 묶였으면 새로고침한다. 판정 전
    /// 키는 여전히 편집이 이긴다.
    #[test]
    fn a_passed_copy_key_bound_to_refresh_refreshes() {
        use winit::keyboard::{Key, ModifiersState, NamedKey};
        let f5 = Key::Named(NamedKey::F5);
        let none = ModifiersState::empty();
        let kb = crate::settings::KeybindingSettings {
            refresh: vec!["f5".into()],
            copy: vec!["f5".into()],
            explorer_refresh: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            address_key_action(&kb, &f5, none, true),
            Some(AddressKeyAction::Refresh)
        );
        assert_eq!(
            address_key_action(&kb, &f5, none, false),
            Some(AddressKeyAction::Edit(WebViewEditAction::Copy))
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
        );
        assert_eq!(out, vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
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
        );
        assert_eq!(out, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        let out = run(
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
            AddressKeyAction::Edit(WebViewEditAction::Redo),
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
        );
        assert!(out.is_empty(), "{out:?}");
    }
}
