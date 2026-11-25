//! `debug.inject_egui_key`의 요청 읽기. 포커스된 메인 창 입구와 `window_id` 입구가 같이 쓴다.
//! 사용자 입력 재현이므로 release에는 두지 않는다.

use crate::view::main::debug_input::DebugInput;

/// 요청의 `key`(기본 `Escape`)·`pressed`(기본 누름)·`modifiers`로 만든 키 주입 하나. 넣는 일은 그 창의 주입 입구가
/// 한다(`App::enter_debug_input`). `modifiers`를 주면 그 창의 유지 수식 키를 바꾸고, 생략하면 유지 상태를 그대로
/// 싣는다.
pub(super) fn key_input(params: &serde_json::Value) -> Result<DebugInput, String> {
    let key = params
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("Escape");
    let pressed = params
        .get("pressed")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let modifiers = super::window_required::read_egui_modifiers(params)?;
    Ok(DebugInput::Key {
        name: key.to_string(),
        pressed,
        modifiers,
    })
}

/// 시험용: 요청의 키 하나를 그 창에 바로 넣고(`push_egui_key`) 응답 본문을 만든다. 매핑 불가 키는
/// `injected: false`이고, 넣었으면 실제로 넣은 것을 `pushed`에 싣는다.
#[cfg(test)]
fn egui_key_body(
    sink: &mut dyn crate::view::main::debug_input::EguiKeySink,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::view::main::debug_input::{DebugEntered, push_egui_key};
    let DebugInput::Key {
        name,
        pressed,
        modifiers,
    } = key_input(params)?
    else {
        unreachable!("key_input makes a key");
    };
    Ok(DebugEntered::Key(push_egui_key(sink, &name, pressed, modifiers)).body())
}

#[cfg(test)]
mod tests {
    use super::egui_key_body;
    use crate::app::ipc::window_required::read_egui_modifiers;
    use crate::view::main::debug_input::EguiKeySink;
    use serde_json::json;

    /// 창 하나의 egui 입력 상태를 대신한다. 클립보드를 읽은 횟수를 센다.
    #[derive(Default)]
    struct FakeWindow {
        held: egui::Modifiers,
        clipboard: Option<String>,
        reads: usize,
        events: Vec<egui::Event>,
    }

    impl EguiKeySink for FakeWindow {
        fn held_modifiers(&self) -> egui::Modifiers {
            self.held
        }
        fn hold_modifiers(&mut self, modifiers: egui::Modifiers) {
            self.held = modifiers;
        }
        fn clipboard_text(&mut self) -> Option<String> {
            self.reads += 1;
            self.clipboard.clone()
        }
        fn push_events(&mut self, events: Vec<egui::Event>) {
            self.events.extend(events);
        }
    }

    fn command() -> egui::Modifiers {
        read_egui_modifiers(&json!({ "modifiers": ["command"] }))
            .unwrap()
            .unwrap()
    }

    fn only_key_modifiers(w: &FakeWindow) -> egui::Modifiers {
        match w.events[..] {
            [egui::Event::Key { modifiers, .. }] => modifiers,
            ref other => panic!("expected one key event, got {other:?}"),
        }
    }

    #[test]
    fn given_modifiers_are_held_and_carried_by_the_key() {
        let mut w = FakeWindow::default();
        let body = egui_key_body(&mut w, &json!({ "key": "A", "modifiers": ["command"] })).unwrap();
        assert_eq!(body, json!({ "injected": true, "pushed": "key" }));
        assert_eq!(only_key_modifiers(&w), command());
        assert_eq!(w.held, command());
    }

    #[test]
    fn omitted_modifiers_use_what_the_window_holds() {
        let mut w = FakeWindow {
            held: command(),
            ..Default::default()
        };
        egui_key_body(&mut w, &json!({ "key": "A" })).unwrap();
        assert_eq!(only_key_modifiers(&w), command());
    }

    #[test]
    fn an_empty_modifier_list_releases_every_held_key() {
        let mut w = FakeWindow {
            held: command(),
            ..Default::default()
        };
        egui_key_body(&mut w, &json!({ "key": "A", "modifiers": [] })).unwrap();
        assert_eq!(only_key_modifiers(&w), egui::Modifiers::NONE);
        assert_eq!(w.held, egui::Modifiers::NONE);
    }

    #[test]
    fn modifiers_held_in_one_window_do_not_reach_another() {
        let mut a = FakeWindow::default();
        let mut b = FakeWindow::default();
        egui_key_body(&mut a, &json!({ "key": "A", "modifiers": ["command"] })).unwrap();
        egui_key_body(&mut b, &json!({ "key": "A" })).unwrap();
        assert_eq!(only_key_modifiers(&b), egui::Modifiers::NONE);
    }

    #[test]
    fn a_held_command_turns_c_x_v_into_edits() {
        let mut w = FakeWindow {
            clipboard: Some("kept\r\nvalue".into()),
            ..Default::default()
        };
        let copy = egui_key_body(&mut w, &json!({ "key": "C", "modifiers": ["command"] })).unwrap();
        let cut = egui_key_body(&mut w, &json!({ "key": "X" })).unwrap();
        assert_eq!(w.reads, 0, "copy and cut must not read the clipboard");
        let paste = egui_key_body(&mut w, &json!({ "key": "V" })).unwrap();
        assert_eq!(copy["pushed"], "copy");
        assert_eq!(cut["pushed"], "cut");
        assert_eq!(paste["pushed"], "paste");
        assert_eq!(w.reads, 1);
        assert_eq!(
            w.events,
            vec![
                egui::Event::Copy,
                egui::Event::Cut,
                egui::Event::Paste("kept\nvalue".into())
            ]
        );
    }

    #[test]
    fn a_paste_with_an_empty_clipboard_pushes_nothing() {
        let mut w = FakeWindow::default();
        let body = egui_key_body(&mut w, &json!({ "key": "V", "modifiers": ["command"] })).unwrap();
        assert_eq!(body, json!({ "injected": true, "pushed": "none" }));
        assert!(w.events.is_empty());
    }

    #[test]
    fn an_unknown_key_changes_nothing() {
        let mut w = FakeWindow::default();
        let body = egui_key_body(
            &mut w,
            &json!({ "key": "NoSuchKey", "modifiers": ["command"] }),
        )
        .unwrap();
        assert_eq!(body, json!({ "injected": false }));
        assert!(w.events.is_empty());
        assert_eq!(w.held, egui::Modifiers::NONE);
    }

    #[test]
    fn an_unknown_modifier_is_rejected() {
        let mut w = FakeWindow::default();
        assert!(egui_key_body(&mut w, &json!({ "key": "A", "modifiers": ["meta"] })).is_err());
        assert!(w.events.is_empty());
    }

    /// egui-winit은 macOS에서 Cmd를, 그 밖에서는 Ctrl을 command로 본다.
    #[test]
    fn command_names_the_platform_command_key() {
        let m = command();
        assert!(m.command);
        if cfg!(target_os = "macos") {
            assert!(m.mac_cmd && !m.ctrl);
        } else {
            assert!(m.ctrl && !m.mac_cmd);
        }
    }

    #[test]
    fn ctrl_is_command_off_macos() {
        let m = read_egui_modifiers(&json!({ "modifiers": ["ctrl"] }))
            .unwrap()
            .unwrap();
        assert!(m.ctrl);
        assert_eq!(m.command, !cfg!(target_os = "macos"));
    }
}
