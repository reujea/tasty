//! Explorer 메뉴와 단축키가 공유하는 판정. MainView 없이 시험할 수 있게 상태 쪽에 둔다.

use crate::runtime::engine_read::EngineRead;
use crate::runtime::surface_binding::SurfaceBinding;

/// 메뉴를 연 뒤 고른 항목을 실행해도 되는가.
/// 파일시스템을 바꾸지 않는 항목(`read_only`)은 같은 explorer surface 이기만 하면 된다.
/// 쓰기 항목은 mirror projection 세대까지 같아야 한다.
pub(crate) fn explorer_menu_admits(
    binding: Option<&SurfaceBinding>,
    engine: &EngineRead<'_>,
    read_only: bool,
) -> bool {
    binding.is_some_and(|binding| {
        if read_only {
            binding.same_surface_in(engine)
        } else {
            binding.current_in(engine)
        }
    })
}

/// 바인딩이 있는 모든 explorer 메뉴 행에 첫 바인딩을 표시한다. 비우면 표시도 지운다.
pub(crate) fn attach_shortcut_hints(
    items: &mut [crate::platform::native_menu::MenuItem],
    settings: &crate::settings::Settings,
) {
    for item in items {
        let field = match item.id {
            1 => "copy_path",
            10 => "copy",
            11 => "cut",
            12 => "paste",
            70 => "explorer_properties",
            110 => "explorer_undo",
            111 => "explorer_redo",
            40 => "explorer_rename",
            30 => "explorer_trash",
            80 => "explorer_new_folder",
            81 => "explorer_new_file",
            _ => continue,
        };
        item.shortcut = menu_shortcut(settings, field);
    }
}

fn menu_shortcut(
    settings: &crate::settings::Settings,
    field: &str,
) -> Option<crate::platform::native_menu::MenuShortcut> {
    let binding = settings.keybindings.get_bindings(field)?.first()?;
    Some(crate::platform::native_menu::MenuShortcut {
        binding: binding.clone(),
        display: menu_display(binding, &settings.general),
    })
}

/// 메뉴 단축키 열의 표시. 키 이름은 설정 화면과 같고, 앞 지우기 키만 메뉴 관례대로 `Del` 로 줄인다.
fn menu_display(binding: &str, general: &crate::settings::GeneralSettings) -> String {
    let mut parts = tasty_settings::KeybindingSettings::format_display_parts(binding, general);
    if let Some(key) = parts.last_mut()
        && key.eq_ignore_ascii_case("delete")
    {
        *key = "Del".to_string();
    }
    parts.join("+")
}

impl super::MainViewState {
    /// 쓸 수 없는 원격 explorer 칸이면 이유를 그 칸에 알리고 true 를 돌려준다. 메뉴의 꺼진 행 툴팁과
    /// 같은 문구다(Windows 메뉴에는 툴팁이 없어 단축키 경로가 이 토스트로 알린다).
    pub(crate) fn refuse_remote_write(&mut self, engine: &EngineRead<'_>, surface_id: u32) -> bool {
        let Some(reason) = engine.explorer_write_refusal(surface_id) else {
            return false;
        };
        self.toasts.push(
            reason,
            crate::adapters::ui::ToastKind::Info,
            crate::adapters::ui::ToastScope::Surface(surface_id),
        );
        true
    }

    /// 클립보드의 경로를 `destination` 에 붙여넣도록 요청한다.
    /// 원격 쓰기를 받지 않는 mirror 와 다른 컴퓨터의 클립보드는 거절한다.
    pub(crate) fn explorer_paste(
        &mut self,
        engine: &EngineRead<'_>,
        surface_id: u32,
        destination: std::path::PathBuf,
        origin: crate::intent::IntentOrigin,
    ) {
        // 표시된 경로는 원격 호스트의 경로다(docs/features/remote-attach/index.md). 원격 쓰기는 그 서버가 알린 연결에서만 한다.
        if self.refuse_remote_write(engine, surface_id) {
            return;
        }
        let Some(target) = super::ExplorerPathSource::of_surface(engine, surface_id) else {
            return;
        };
        if !self.explorer_take_os_clipboard(target) {
            return;
        }
        let Some(clip) = self.explorer_clipboard.get() else {
            return;
        };
        // 다른 컴퓨터의 경로를 같은 문자열의 이 컴퓨터 파일로 읽지 않는다.
        if !clip.pastes_into(engine, surface_id) {
            let key = super::explorer_os_clipboard::source_reason(&clip);
            self.toasts.push(
                crate::i18n::t(key).to_string(),
                crate::adapters::ui::ToastKind::Info,
                crate::adapters::ui::ToastScope::Window,
            );
            return;
        }
        self.request_explorer_file(
            engine,
            surface_id,
            crate::app::explorer_files::Operation::Paste {
                paths: clip.paths,
                destination,
                cut: clip.cut,
            },
            origin,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ExplorerClipboard, ExplorerPathSource};

    fn clipboard(source: ExplorerPathSource) -> ExplorerClipboard {
        ExplorerClipboard {
            identity: Default::default(),
            paths: vec![crate::test_support::abs_path("remote/proj/a.txt")],
            cut: false,
            source,
            os_seen: Some(crate::clipboard::files::OsClip::Other),
        }
    }

    fn user() -> crate::intent::IntentOrigin {
        crate::intent::IntentOrigin::User {
            source: crate::intent::UserSource::ContextMenu,
        }
    }

    #[test]
    fn a_clipboard_copied_from_a_remote_explorer_is_not_pasted_locally() {
        let (mut state, session, sid, root) = crate::state::tests::explorer_fixture(false);
        state
            .explorer_clipboard
            .set(Some(clipboard(ExplorerPathSource::Remote { workspace: 9 })));
        state.explorer_paste(&session.read(), sid, root.clone(), user());
        assert_eq!(state.explorer_file_requests.len(), 0);
        assert_eq!(
            state.toasts.messages(),
            vec![crate::i18n::t("explorer.state.remote_paste_unsupported")]
        );

        state
            .explorer_clipboard
            .set(Some(clipboard(ExplorerPathSource::Local)));
        state.explorer_paste(&session.read(), sid, root, user());
        assert_eq!(
            state.explorer_file_requests.len(),
            1,
            "a local clipboard is pasted"
        );
    }

    #[test]
    fn a_menu_choice_runs_only_on_the_surface_generation_it_was_opened_on() {
        // 자원 세대를 가진 surface 로 교체를 흉내 낸다. 판정 규칙은 kind 와 무관하다.
        let (_state, mut session) = crate::state::tests::test_state();
        let sid = session.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let binding = SurfaceBinding::capture(&session.read(), sid);
        assert!(explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            false
        ));
        assert!(explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            true
        ));
        assert!(!explorer_menu_admits(None, &session.read(), true));

        // 같은 id 에 다른 자원이 들어오면 세대가 달라진다.
        session.borrow_mut().runtime.terminals.insert(
            sid,
            tasty_terminal::Terminal::new_detached(80, 24),
            None,
        );
        assert!(!explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            false
        ));
        assert!(!explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            true
        ));
    }

    #[test]
    fn read_only_choices_survive_a_rebuilt_mirror_projection_but_writes_do_not() {
        let (_state, mut session, sid, _root) = crate::state::tests::explorer_fixture(true);
        let binding = SurfaceBinding::capture(&session.read(), sid);
        assert!(explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            false
        ));

        // 원격 트리를 다시 받으면 projection 토큰만 새로 만든다. surface 는 그대로다.
        let pane = crate::model::Pane::new_with_surface(
            sid,
            sid,
            "Explorer".into(),
            crate::model::SurfaceDescriptor::new(sid, "explorer"),
        );
        let mut workspace = crate::model::Workspace::new_with_pane(sid, "Explorer".into(), pane);
        workspace.mirror = true;
        assert!(
            session
                .core_state
                .replace_mirror_workspace(workspace)
                .is_ok()
        );
        assert!(explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            true
        ));
        assert!(!explorer_menu_admits(
            binding.as_ref(),
            &session.read(),
            false
        ));
    }

    /// 새 폴더·새 파일 행도 이름 변경·휴지통처럼 첫 바인딩을 보인다. 바인딩이 없는 행은 비운다.
    #[test]
    fn the_new_folder_and_new_file_rows_show_their_first_binding() {
        use crate::platform::native_menu::MenuItem;
        let mut settings = crate::settings::Settings::default();
        let mut items = vec![
            MenuItem::new(80, "New folder"),
            MenuItem::new(81, "New file"),
            MenuItem::new(50, "Add to favorites"),
        ];
        attach_shortcut_hints(&mut items, &settings);
        let shown: Vec<_> = items
            .iter()
            .map(|i| i.shortcut.as_ref().map(|s| s.binding.as_str()))
            .collect();
        assert_eq!(shown, vec![Some("f7"), Some("shift+f4"), None]);
        assert!(items[0].shortcut.as_ref().unwrap().display.ends_with("F7"));

        settings.keybindings.explorer_new_folder.clear();
        let mut items = vec![MenuItem::new(80, "New folder")];
        attach_shortcut_hints(&mut items, &settings);
        assert!(items[0].shortcut.is_none());
    }
    #[test]
    fn all_bound_rows_follow_custom_bindings_and_clearing() {
        use crate::platform::native_menu::MenuItem;
        let mut settings = crate::settings::Settings::default();
        let fields = [
            (1, "copy_path"),
            (10, "copy"),
            (11, "cut"),
            (12, "paste"),
            (30, "explorer_trash"),
            (40, "explorer_rename"),
            (70, "explorer_properties"),
            (80, "explorer_new_folder"),
            (81, "explorer_new_file"),
            (110, "explorer_undo"),
            (111, "explorer_redo"),
        ];
        for (id, field) in fields {
            let mut items = [MenuItem::disabled(id, "Action")];
            assert!(settings.keybindings.set_field(field, "ctrl+f9"));
            assert!(settings.keybindings.add_binding(field, "alt+f10".into()));
            attach_shortcut_hints(&mut items, &settings);
            let hint = items[0].shortcut.as_ref().unwrap();
            assert_eq!(hint.binding, "ctrl+f9", "{field}");
            assert!(hint.display.ends_with("F9"), "{field}");
            assert!(settings.keybindings.clear_field(field));
            attach_shortcut_hints(&mut items, &settings);
            assert!(items[0].shortcut.is_none(), "{field}");
        }
    }
}
