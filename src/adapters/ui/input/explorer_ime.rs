//! X11 XIM can commit a composition and replay its ending physical key afterward.
//! Remember ownership at the raw press, before XFilterEvent clears the preedit.
//! A window key event pairs with the last raw press of its physical key observed before it
//! arrived, so presses held behind earlier input and replayed later keep their own pairs.

use std::collections::HashMap;
use std::time::Instant;
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
    /// Raw presses per physical key, oldest first: when each was observed and the owner it had
    /// (`None` when no explorer input owned it).
    presses: HashMap<PhysicalKey, Vec<(Instant, Option<InputOwner>)>>,
}

impl ExplorerImeKeys {
    fn ime(&mut self, event: &Ime, surface: Option<u32>) {
        match event {
            Ime::Preedit(text, _) => self.composing = surface.filter(|_| !text.is_empty()),
            Ime::Commit(_) | Ime::Disabled => self.composing = None,
            Ime::Enabled => {}
        }
    }

    /// A raw press observed at `at`. Every new physical press stands for itself, even if XIM never
    /// forwarded the prior one: a later genuine key pairs with its own observation, not an older one.
    fn press(&mut self, key: PhysicalKey, input: Option<InputOwner>, at: Instant) {
        let owner = input.filter(|owner| match owner {
            InputOwner::Rename { .. } => self.rename_composing == Some(*owner),
            _ => self.composing == Some(owner.surface()),
        });
        self.presses.entry(key).or_default().push((at, owner));
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
        for (_, owner) in self.presses.values_mut().flatten() {
            if matches!(owner, Some(InputOwner::Rename { .. })) {
                *owner = None;
            }
        }
    }

    /// The owner of the window press of `key` that arrived at `arrived`: the last raw press of that
    /// key observed by then. That observation and the older ones are used up.
    fn take(
        &mut self,
        key: PhysicalKey,
        arrived: Instant,
        surface: Option<u32>,
    ) -> Option<InputOwner> {
        let observed = self.presses.get_mut(&key)?;
        let paired = observed.iter().rposition(|(at, _)| *at <= arrived)?;
        let (_, owner) = observed.drain(..=paired).last()?;
        if observed.is_empty() {
            self.presses.remove(&key);
        }
        owner.filter(|owner| Some(owner.surface()) == surface)
    }
}

fn rename_input_owner(
    state: &crate::state::MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<InputOwner> {
    if state.fullscreen_stage_active()
        || state.has_settings_open_request()
        || state.plugin_popup_owns_keys()
        || state.popups.keyboard_focused_ids() != ["rename"]
    {
        return None;
    }
    let rename = state
        .popups
        .instances_of(crate::adapters::ui::rename_popup::RENAME_POPUP_ID)
        .find(|p| state.popups.is_instance_focused(p.instance))
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
            self.explorer_ime_keys
                .press(event.physical_key, input, Instant::now());
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
                let arrived = self.base.gpu.event_arrival();
                if let Some(owner) =
                    self.explorer_ime_keys
                        .take(event.physical_key, arrived, surface)
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
            keys.press(key, Some(InputOwner::List(7)), Instant::now());
            keys.ime(&Ime::Preedit(String::new(), None), Some(7));
            keys.ime(&Ime::Commit("간".into()), Some(7));
            assert_eq!(
                keys.take(key, Instant::now(), Some(7)),
                Some(InputOwner::List(7))
            );
            keys.press(key, Some(InputOwner::List(7)), Instant::now());
            assert!(
                keys.take(key, Instant::now(), Some(7)).is_none(),
                "next genuine key must work"
            );
        }
    }

    #[test]
    fn unforwarded_press_does_not_swallow_a_later_press() {
        let mut keys = ExplorerImeKeys::default();
        let key = PhysicalKey::Code(KeyCode::ArrowDown);
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        keys.ime(&Ime::Commit("간".into()), Some(7));
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        assert!(keys.take(key, Instant::now(), Some(7)).is_none());
    }

    /// Given 조합 중 explorer 가 같은 물리 키의 누름 둘을 받았고 두 창 사건이 앞 입력 뒤에 줄을 섰다(첫 사건 도착 뒤 둘째
    /// raw 관찰), When 두 사건을 도착 시각으로 차례로 재생하면, Then 각 사건이 자기 관찰과 짝을 지어 둘 다 그 explorer 가
    /// 가진다.
    #[test]
    fn two_held_presses_of_one_key_each_keep_their_observation() {
        let ms = std::time::Duration::from_millis;
        let mut keys = ExplorerImeKeys::default();
        let key = PhysicalKey::Code(KeyCode::ArrowDown);
        let t0 = Instant::now();
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, Some(InputOwner::List(7)), t0);
        let first = t0 + ms(1);
        keys.press(key, Some(InputOwner::List(7)), t0 + ms(2));
        let second = t0 + ms(3);
        assert_eq!(keys.take(key, first, Some(7)), Some(InputOwner::List(7)));
        assert_eq!(keys.take(key, second, Some(7)), Some(InputOwner::List(7)));
    }

    #[test]
    fn popup_typing_other_surfaces_and_enabled_mode_do_not_claim_list_keys() {
        let key = PhysicalKey::Code(KeyCode::Delete);
        let mut keys = ExplorerImeKeys::default();
        keys.ime(&Ime::Enabled, Some(7));
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        assert!(keys.take(key, Instant::now(), Some(7)).is_none());
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, None, Instant::now());
        assert!(keys.take(key, Instant::now(), Some(7)).is_none());
        keys.press(key, Some(InputOwner::List(8)), Instant::now());
        assert!(keys.take(key, Instant::now(), Some(8)).is_none());
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        assert!(keys.take(key, Instant::now(), Some(8)).is_none());
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
        keys.press(escape, Some(find), Instant::now());
        keys.ime(&Ime::Preedit(String::new(), None), Some(7));
        keys.ime(&Ime::Commit("간".into()), Some(7));
        assert_eq!(keys.take(escape, Instant::now(), Some(7)), Some(find));
        keys.press(escape, Some(find), Instant::now());
        assert!(keys.take(escape, Instant::now(), Some(7)).is_none());
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

    /// X11 관찰은 줄에 서지 않고 누름이 도착한 때 그 explorer 를 기록한다. 그 키가 정착되지 않은 앞 입력 뒤에 섰다가
    /// 재생될 때 그 사이 포커스가 그 explorer 를 떠났으면(앞 패스가 팝업을 열었다) 재생 시점 주인(`surface`)으로
    /// 거르므로 기록이 그 키를 가져가지 않는다.
    #[test]
    fn an_x11_observed_key_held_behind_input_uses_its_replay_owner() {
        let key = PhysicalKey::Code(KeyCode::ArrowDown);
        let mut keys = ExplorerImeKeys::default();
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        assert!(keys.take(key, Instant::now(), None).is_none());
    }

    /// 보류 없이 처리한 같은 키는 도착 때 주인이 재생 시점 주인이라 그 explorer 가 가져간다.
    #[test]
    fn an_x11_observed_key_without_a_hold_uses_its_owner() {
        let key = PhysicalKey::Code(KeyCode::ArrowDown);
        let mut keys = ExplorerImeKeys::default();
        keys.ime(&Ime::Preedit("간".into(), Some((3, 3))), Some(7));
        keys.press(key, Some(InputOwner::List(7)), Instant::now());
        assert_eq!(
            keys.take(key, Instant::now(), Some(7)),
            Some(InputOwner::List(7))
        );
    }
}
