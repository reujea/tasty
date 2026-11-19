//! X11 XIM can commit a composition and replay its ending physical key afterward.
//! Remember ownership at the raw press, before XFilterEvent clears the preedit.

use std::collections::HashMap;
use winit::event::{ElementState, Ime, RawKeyEvent, WindowEvent};
use winit::keyboard::{Key, NamedKey, PhysicalKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputOwner {
    List(u32),
    Find(u32),
    Rename { surface: u32, popup: u64 },
}

impl InputOwner {
    fn surface(self) -> u32 {
        match self {
            Self::List(surface) | Self::Find(surface) => surface,
            Self::Rename { surface, .. } => surface,
        }
    }

    fn consumes(
        self,
        bindings: &crate::settings::KeybindingSettings,
        key: &Key,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
        match self {
            Self::Find(_) => *key == Key::Named(NamedKey::Escape),
            Self::Rename { .. } => *key == Key::Named(NamedKey::Enter),
            Self::List(_) => {
                crate::shortcuts::explorer_composition_list_key(bindings, key, modifiers)
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct ExplorerImeKeys {
    composing: Option<u32>,
    rename_composing: Option<InputOwner>,
    presses: HashMap<PhysicalKey, InputOwner>,
}

impl ExplorerImeKeys {
    fn ime(&mut self, event: &Ime, surface: Option<u32>) {
        match event {
            Ime::Preedit(text, _) => self.composing = surface.filter(|_| !text.is_empty()),
            Ime::Commit(_) | Ime::Disabled => self.composing = None,
            Ime::Enabled => {}
        }
    }

    fn press(&mut self, key: PhysicalKey, input: Option<InputOwner>) {
        // Every new physical press replaces the prior ownership, even if XIM never
        // forwarded that prior key. A later genuine key must not inherit it.
        self.presses.remove(&key);
        if let Some(owner) = input.filter(|owner| match owner {
            InputOwner::Rename { .. } => self.rename_composing == Some(*owner),
            _ => self.composing == Some(owner.surface()),
        }) {
            self.presses.insert(key, owner);
        }
    }

    fn rename_ime(&mut self, event: &Ime, owner: Option<InputOwner>) {
        match event {
            Ime::Preedit(text, _) => {
                self.rename_composing = owner.filter(|_| !text.is_empty());
            }
            Ime::Commit(_) => self.rename_composing = None,
            Ime::Disabled => self.reset_rename(),
            Ime::Enabled => {}
        }
    }

    fn reset_rename(&mut self) {
        self.rename_composing = None;
        self.presses
            .retain(|_, owner| !matches!(owner, InputOwner::Rename { .. }));
    }

    fn take(&mut self, key: PhysicalKey, surface: Option<u32>) -> Option<InputOwner> {
        self.presses
            .remove(&key)
            .filter(|owner| Some(owner.surface()) == surface)
    }
}

fn rename_input_owner(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<InputOwner> {
    if state.fullscreen_stage_active()
        || state.has_settings_open_request()
        || state.has_plugin_popup()
        || state.popups.keyboard_focused_ids() != ["rename"]
    {
        return None;
    }
    let rename = state
        .popups
        .instances_of(crate::adapters::ui::rename_popup::RENAME_POPUP_ID)
        .find(|p| p.focused)
        .and_then(|p| crate::adapters::ui::rename_popup::payload_of(&p.payload))?;
    if rename.waiting.is_some() {
        return None;
    }
    let crate::state::RenameTarget::ExplorerEntry {
        surface_id,
        binding,
        ..
    } = &rename.target
    else {
        return None;
    };
    if !binding.current_in(engine) {
        return None;
    }
    // Reopening or raising the popup changes its sequence; old input cannot own a new dialog.
    let (popup, _) = state.popups.open_geometry("rename")?;
    Some(InputOwner::Rename {
        surface: *surface_id,
        popup,
    })
}

impl crate::App {
    pub(crate) fn observe_explorer_device_key(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: winit::event::DeviceEvent,
    ) {
        use winit::platform::x11::ActiveEventLoopExtX11;
        if !event_loop.is_x11() || self.state.shutdown.is_some() {
            return;
        }
        if let winit::event::DeviceEvent::Key(key) = event
            && let Some(id) = self.view.focused_view_id
            && let Some(main) = self
                .view
                .views
                .get_mut(&id)
                .and_then(|view| view.as_main_mut())
            && let Some(engine) = self.engines.window_read(id)
        {
            main.observe_explorer_raw_key(&engine, &key);
        }
    }
}

impl crate::view::main::MainView {
    fn ime_explorer_surface(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) -> Option<u32> {
        if self.state.window_keys_to_overlay() {
            return None;
        }
        crate::adapters::ui::input::shortcuts::focused_explorer_surface_id(&self.state, engine)
    }

    pub(crate) fn observe_explorer_raw_key(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        event: &RawKeyEvent,
    ) {
        if event.state == ElementState::Pressed {
            let input = rename_input_owner(&self.state, engine).or_else(|| {
                self.ime_explorer_surface(engine).and_then(|sid| {
                    let view = self.state.explorer_views.get(sid)?;
                    if !view.text_input_active() {
                        Some(InputOwner::List(sid))
                    } else if !view.addr_editing
                        && view.create.is_none()
                        && view.find.as_ref().is_some_and(|find| find.field_focused)
                    {
                        Some(InputOwner::Find(sid))
                    } else {
                        None
                    }
                })
            });
            self.explorer_ime_keys.press(event.physical_key, input);
        }
    }

    pub(crate) fn consume_explorer_ime_key(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        event: &WindowEvent,
    ) -> bool {
        match event {
            WindowEvent::Ime(ime) => {
                let surface = crate::adapters::ui::input::shortcuts::focused_explorer_surface_id(
                    &self.state,
                    engine,
                );
                self.explorer_ime_keys.ime(ime, surface);
                let rename = rename_input_owner(&self.state, engine);
                self.explorer_ime_keys.rename_ime(ime, rename);
            }
            WindowEvent::Focused(_) => self.explorer_ime_keys = ExplorerImeKeys::default(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let rename = rename_input_owner(&self.state, engine);
                let surface = rename
                    .map(InputOwner::surface)
                    .or_else(|| self.ime_explorer_surface(engine));
                if let Some(owner) = self.explorer_ime_keys.take(event.physical_key, surface)
                    && match owner {
                        InputOwner::Rename { .. } => Some(owner) == rename,
                        _ => true,
                    }
                    && let Some(view) = self.state.explorer_views.get(owner.surface())
                    && match owner {
                        InputOwner::Rename { .. } => true,
                        InputOwner::List(_) => !view.text_input_active(),
                        InputOwner::Find(_) => {
                            !view.addr_editing && view.create.is_none() && view.find.is_some()
                        }
                    }
                {
                    let key = crate::shortcuts::shortcut_lookup_key(
                        &event.logical_key,
                        &event.physical_key,
                        self.base.state.modifiers,
                    );
                    return owner.consumes(
                        &engine.settings.keybindings,
                        &key,
                        self.base.state.modifiers,
                    );
                }
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
#[path = "explorer_ime_rename_tests.rs"]
mod rename_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn same_press_remains_owned_across_preedit_end_and_commit() {
        for code in [
            KeyCode::ArrowDown,
            KeyCode::Delete,
            KeyCode::Enter,
            KeyCode::Escape,
        ] {
            let mut keys = ExplorerImeKeys::default();
            let key = PhysicalKey::Code(code);
            keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
            keys.press(key, Some(InputOwner::List(7)));
            keys.ime(&Ime::Preedit(String::new(), None), Some(7));
            keys.ime(&Ime::Commit("간".into()), Some(7));
            assert_eq!(keys.take(key, Some(7)), Some(InputOwner::List(7)));
            keys.press(key, Some(InputOwner::List(7)));
            assert!(
                keys.take(key, Some(7)).is_none(),
                "next genuine key must work"
            );
        }
    }

    #[test]
    fn unforwarded_press_does_not_swallow_a_later_press() {
        let mut keys = ExplorerImeKeys::default();
        let key = PhysicalKey::Code(KeyCode::ArrowDown);
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, Some(InputOwner::List(7)));
        keys.ime(&Ime::Commit("간".into()), Some(7));
        keys.press(key, Some(InputOwner::List(7)));
        assert!(keys.take(key, Some(7)).is_none());
    }

    #[test]
    fn popup_typing_other_surfaces_and_enabled_mode_do_not_claim_list_keys() {
        let key = PhysicalKey::Code(KeyCode::Delete);
        let mut keys = ExplorerImeKeys::default();
        keys.ime(&Ime::Enabled, Some(7));
        keys.press(key, Some(InputOwner::List(7)));
        assert!(keys.take(key, Some(7)).is_none());
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, None);
        assert!(keys.take(key, Some(7)).is_none());
        keys.press(key, Some(InputOwner::List(8)));
        assert!(keys.take(key, Some(8)).is_none());
        keys.press(key, Some(InputOwner::List(7)));
        assert!(keys.take(key, Some(8)).is_none());
    }

    #[test]
    fn composition_guard_matches_custom_list_bindings_and_leaves_find_and_select_all() {
        use winit::keyboard::{Key, ModifiersState, NamedKey};
        let bindings = crate::settings::KeybindingSettings {
            explorer_cursor_down: vec!["ctrl+j".into()],
            explorer_trash: vec!["ctrl+d".into()],
            ..Default::default()
        };
        let matches =
            |key, mods| crate::shortcuts::explorer_composition_list_key(&bindings, &key, mods);
        assert!(matches(Key::Character("j".into()), ModifiersState::CONTROL));
        assert!(matches(Key::Character("d".into()), ModifiersState::CONTROL));
        assert!(!matches(
            Key::Named(NamedKey::ArrowDown),
            ModifiersState::empty()
        ));
        assert!(!matches(
            Key::Character("f".into()),
            ModifiersState::CONTROL
        ));
        assert!(!matches(
            Key::Character("a".into()),
            ModifiersState::CONTROL
        ));
    }

    #[test]
    fn find_escape_keeps_its_owner_through_commit_but_not_a_new_press() {
        let mut keys = ExplorerImeKeys::default();
        let escape = PhysicalKey::Code(KeyCode::Escape);
        let find = InputOwner::Find(7);
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(escape, Some(find));
        keys.ime(&Ime::Preedit(String::new(), None), Some(7));
        keys.ime(&Ime::Commit("간".into()), Some(7));
        assert_eq!(keys.take(escape, Some(7)), Some(find));
        keys.press(escape, Some(find));
        assert!(keys.take(escape, Some(7)).is_none());
    }

    #[test]
    fn find_escape_does_not_apply_list_bindings_to_editor_keys() {
        use winit::keyboard::ModifiersState;
        let bindings = crate::settings::KeybindingSettings {
            explorer_cursor_down: vec!["j".into()],
            explorer_clear_selection: vec!["ctrl+g".into()],
            ..Default::default()
        };
        let find = InputOwner::Find(7);
        let modifiers = ModifiersState::empty();
        assert!(find.consumes(&bindings, &Key::Named(NamedKey::Escape), modifiers));
        assert!(!find.consumes(&bindings, &Key::Character("j".into()), modifiers));
        assert!(!find.consumes(&bindings, &Key::Named(NamedKey::ArrowDown), modifiers));
        assert!(InputOwner::List(7).consumes(&bindings, &Key::Character("j".into()), modifiers));
    }
}
