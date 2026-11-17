use super::*;
use winit::keyboard::{KeyCode, ModifiersState};

const RENAME: InputOwner = InputOwner::Rename {
    surface: 7,
    popup: 11,
};

fn update(keys: &mut ExplorerImeKeys, event: Ime, owner: Option<InputOwner>) {
    keys.ime(&event, Some(7));
    keys.rename_ime(&event, owner);
}
fn composing() -> ExplorerImeKeys {
    let mut keys = ExplorerImeKeys::default();
    update(
        &mut keys,
        Ime::Preedit("간".into(), Some((3, 3))),
        Some(RENAME),
    );
    keys
}
fn end(keys: &mut ExplorerImeKeys) {
    for event in [
        Ime::Preedit(String::new(), None),
        Ime::Commit("간".into()),
        Ime::Preedit(String::new(), None),
    ] {
        update(keys, event, Some(RENAME));
    }
}

#[test]
fn rename_raw_enter_is_owned_through_commit_but_next_genuine_press_is_not() {
    for code in [KeyCode::Enter, KeyCode::NumpadEnter] {
        let key = PhysicalKey::Code(code);
        let mut keys = composing();
        keys.press(key, Some(RENAME));
        // XIM can deliver a native release before clear/Commit and replay the press later.
        // Only native presses replace ownership; releasing is not evidence of replay delivery.
        end(&mut keys);
        assert_eq!(keys.take(key, Some(7)), Some(RENAME));
        keys.press(key, Some(RENAME));
        assert_eq!(keys.take(key, Some(7)), None);
    }
}

#[test]
fn rename_unforwarded_end_never_claims_distinct_genuine_press_even_in_same_batch() {
    let key = PhysicalKey::Code(KeyCode::Enter);
    for captured_ending_press in [false, true] {
        for repeated in [0, 1, 3] {
            let mut keys = composing();
            if captured_ending_press {
                keys.press(key, Some(RENAME));
            }
            end(&mut keys);
            for _ in 0..repeated {
                end(&mut keys);
            }
            // No ending KeyboardInput was forwarded. No batch boundary or delay is needed:
            // the distinct native press itself replaces ownership before its own replay.
            keys.press(key, Some(RENAME));
            assert_eq!(keys.take(key, Some(7)), None);
        }
    }
}

#[test]
fn rename_requires_its_own_nonempty_preedit_not_enabled_mode_or_background_composition() {
    let key = PhysicalKey::Code(KeyCode::Enter);
    for event in [
        Ime::Enabled,
        Ime::Preedit(String::new(), None),
        Ime::Commit("간".into()),
    ] {
        let mut keys = ExplorerImeKeys::default();
        update(&mut keys, event, Some(RENAME));
        keys.press(key, Some(RENAME));
        assert_eq!(keys.take(key, Some(7)), None);
    }
    let mut keys = ExplorerImeKeys::default();
    update(&mut keys, Ime::Preedit("간".into(), Some((3, 3))), None);
    keys.press(key, Some(RENAME));
    assert_eq!(
        keys.take(key, Some(7)),
        None,
        "opening Rename must not inherit background preedit"
    );
    let mut keys = composing();
    keys.press(
        key,
        Some(InputOwner::Rename {
            surface: 7,
            popup: 12,
        }),
    );
    assert_eq!(
        keys.take(key, Some(7)),
        None,
        "reopened popup needs its own composition"
    );
    keys.press(
        key,
        Some(InputOwner::Rename {
            surface: 8,
            popup: 11,
        }),
    );
    assert_eq!(keys.take(key, Some(8)), None);
}

#[test]
fn rename_reset_disabled_and_window_focus_reset_discard_late_ownership() {
    let key = PhysicalKey::Code(KeyCode::Enter);
    for reset in 0..3 {
        let mut keys = composing();
        keys.press(key, Some(RENAME));
        match reset {
            0 => keys.reset_rename(),
            1 => update(&mut keys, Ime::Disabled, Some(RENAME)),
            _ => keys = ExplorerImeKeys::default(),
        }
        end(&mut keys);
        assert_eq!(keys.take(key, Some(7)), None);
        keys.press(key, Some(RENAME));
        assert_eq!(keys.take(key, Some(7)), None);
    }
}

#[test]
fn rename_only_consumes_its_enter_and_keeps_other_editor_and_popup_keys() {
    let bindings = crate::settings::KeybindingSettings::default();
    let mods = ModifiersState::empty();
    assert!(RENAME.consumes(&bindings, &Key::Named(NamedKey::Enter), mods));
    for key in [
        NamedKey::Escape,
        NamedKey::Delete,
        NamedKey::ArrowDown,
        NamedKey::Tab,
    ] {
        assert!(!RENAME.consumes(&bindings, &Key::Named(key), mods));
    }
    assert!(!RENAME.consumes(&bindings, &Key::Character("a".into()), mods));
}

#[test]
fn rename_owner_requires_current_focused_explorer_entry_popup_and_respects_other_popups() {
    use crate::adapters::ui::popup::PopupState;
    use crate::runtime::surface_binding::SurfaceBinding;
    use crate::state::RenameTarget;
    let (mut state, engine) = crate::state::tests::test_state();
    let read = engine.read();
    let sid = read.workspace_at(0).unwrap().all_surface_ids()[0];
    // Target admission uses a real current binding; no filesystem or surface factory runs here.
    let entry = || {
        crate::adapters::ui::rename_popup::RenamePayload::new(
            RenameTarget::ExplorerEntry {
                surface_id: sid,
                path: "owned-fixture.txt".into(),
                binding: SurfaceBinding::capture(&read, sid).unwrap(),
            },
            String::new(),
        )
    };
    crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        entry(),
        crate::model::popup_kind::PopupScope::Window,
        None,
    );
    let owner =
        rename_input_owner(&state, &read).expect("focused Explorer Rename must participate");
    assert_eq!(owner.surface(), sid);
    state.popups.set_focused("rename", false);
    assert_eq!(rename_input_owner(&state, &read), None);
    state.popups.set_focused("rename", true);
    state.plugin_popup_open = true;
    assert_eq!(rename_input_owner(&state, &read), None);
    state.plugin_popup_open = false;
    state.popups.register(PopupState::new(
        "other",
        "",
        egui::Vec2::ZERO,
        crate::model::popup_kind::PopupScope::Window,
    ));
    state
        .popups
        .open_centered_focused("other", crate::model::popup_kind::PopupScope::Window);
    assert_eq!(rename_input_owner(&state, &read), None);
    state.popups.close("other");
    state.popups.close("rename");
    crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        entry(),
        crate::model::popup_kind::PopupScope::Window,
        None,
    );
    assert_ne!(
        rename_input_owner(&state, &read),
        Some(owner),
        "reopening changes ownership"
    );
    for rename in crate::adapters::ui::rename_popup::instances_mut(&mut state) {
        rename.target = RenameTarget::TabName { tab_id: 1 };
    }
    assert_eq!(
        rename_input_owner(&state, &read),
        None,
        "other rename targets are outside this fix"
    );
}
