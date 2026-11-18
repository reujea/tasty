//! Copy/Paste 단축키.

use crate::runtime::engine_read::EngineRead;
use winit::keyboard::{Key, ModifiersState};

use crate::view::main::MainView;
use crate::view::main::selection::should_copy_via_focused_selection;
use crate::view::ui::View as _;
use tasty_key_match::matches_any_binding;

/// 단축키와 명령 팔레트가 공유하는 탐색기 파일 액션.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    SelectAll,
    CopyPath,
    CopyFiles,
    CutFiles,
    PasteFiles,
    Rename,
    Trash,
}

impl MainView {
    pub(super) fn handle_copy_shortcut(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let bindings = engine.settings.keybindings.copy.clone();
        if !matches_any_binding(&bindings, key, mods) {
            return false;
        }
        self.run_copy(engine)
    }

    /// 처리하지 못하면 false로 반환해 탐색기 파일 복사 등 다음 경로로 넘긴다. 대상은 키를 누른 때의 포커스다.
    pub(crate) fn run_copy(&mut self, engine: &EngineRead<'_>) -> bool {
        let sid = self.state.focused_surface_id(engine);
        self.run_copy_on(engine, sid)
    }

    /// `sid` 를 대상으로 복사한다. 처리하지 못하면 false 다. 포커스를 다시 읽지 않는다.
    pub(crate) fn run_copy_on(&mut self, engine: &EngineRead<'_>, sid: Option<u32>) -> bool {
        // 붙여넣기 직후 짧은 시간 동안은 오타 방지를 위해 복사·SIGINT 없이 안내만 한다.
        if let Some(t) = self.last_terminal_paste_at
            && t.elapsed() < crate::view::main::PASTE_CTRL_C_COOLDOWN
        {
            let scope = crate::adapters::ui::ToastScope::Surface(sid.unwrap_or(0));
            self.state
                .toasts
                .push_info(crate::i18n::t("toast.ctrl_c_ignored_after_paste"), scope);
            self.mark_dirty();
            return true;
        }
        // 키보드 복사는 대상 surface와 선택 영역의 surface가 같을 때만 한다.
        // 다른 surface의 이전 선택을 복사해 현재 탐색기의 입력을 가로채지 않게 한다.
        let sel_surface_id = self.text_selection.as_ref().map(|s| s.surface_id);
        let selection_targets_focus = should_copy_via_focused_selection(sel_surface_id, sid);
        if selection_targets_focus && self.copy_selection_to_clipboard(engine) {
            self.mark_dirty();
            return true;
        }
        let Some(sid) = sid else {
            return false;
        };
        let st = self.state.surface_type_of(engine, sid);
        // 선택 위젯은 플러그인 egui Context에 있으므로 Copy 이벤트를 해당 surface에 보낸다.
        if st.kind_capability(engine, |d| d.egui_copy) && egui_mesh_surface(engine, sid) {
            self.egui_mesh_push_copy(sid);
            self.mark_dirty();
            return true;
        }
        false
    }

    /// copy_path capability가 있는 포커스 대상의 파일 액션을 처리한다.
    /// 파일 작업은 context menu와 같은 함수를 호출해 충돌·잘못된 붙여넣기 처리를 공유한다.
    pub(super) fn handle_explorer_shortcut(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
        edit_judged: bool,
    ) -> bool {
        let kb = &engine.settings.keybindings;
        let target = self.state.command_target(engine);
        for (combos, id) in [
            (&kb.select_all, "select_all"),
            (&kb.copy_path, "copy_path"),
            (&kb.explorer_rename, "explorer_rename"),
            (&kb.explorer_trash, "explorer_trash"),
        ] {
            if matches_any_binding(combos, key, mods)
                && super::surface_actions::held_for_draft(&mut self.state, engine, &target, id)
            {
                return true;
            }
        }
        if !self
            .state
            .focused_surface_type(engine)
            .kind_capability(engine, |d| d.copy_path)
        {
            return false;
        }
        if self.cancel_explorer_address_check(engine, key, mods) {
            return true;
        }
        if super::focused_explorer_typing(&self.state, engine) {
            return false;
        }
        // 편집 판정을 마친 키는 파일 복사·잘라내기·붙여넣기로 다시 고르지 않는다.
        let edit = |combos: &[String]| !edit_judged && matches_any_binding(combos, key, mods);
        let action = if matches_any_binding(&kb.select_all, key, mods) {
            ExplorerAction::SelectAll
        } else if matches_any_binding(&kb.copy_path, key, mods) {
            ExplorerAction::CopyPath
        } else if edit(&kb.copy) {
            ExplorerAction::CopyFiles
        } else if edit(&kb.cut) {
            ExplorerAction::CutFiles
        } else if edit(&kb.paste) {
            ExplorerAction::PasteFiles
        } else if matches_any_binding(&kb.explorer_rename, key, mods) {
            ExplorerAction::Rename
        } else if matches_any_binding(&kb.explorer_trash, key, mods) {
            ExplorerAction::Trash
        } else {
            return self.handle_explorer_list_key(engine, key, mods);
        };
        let sid = self.state.focused_surface_id(engine);
        self.run_explorer_action_on(engine, action, sid)
    }

    /// `sid` 의 탐색기에 파일 액션을 실행한다. 팔레트는 키보드 앞단 검사를 거치지 않으므로 여기서도
    /// capability를 확인한다. 포커스를 다시 읽지 않는다.
    pub(crate) fn run_explorer_action_on(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        action: ExplorerAction,
        sid: Option<u32>,
    ) -> bool {
        let Some(sid) = sid else {
            return false;
        };
        if !self
            .state
            .surface_type_of(engine, sid)
            .kind_capability(engine, |d| d.copy_path)
        {
            return false;
        }
        let is_cut_files = action == ExplorerAction::CutFiles;
        let Some(panel) = engine
            .find_surface_by_id(sid)
            .and_then(|surface| surface.explorer())
        else {
            return true;
        };
        let cwd = panel.current_root().to_path_buf();
        let sid = panel.id;
        if matches!(action, ExplorerAction::Rename | ExplorerAction::Trash) {
            self.run_explorer_item_key(engine, sid, action);
        } else if action == ExplorerAction::SelectAll {
            if let Some(view) = self.state.explorer_views.get_mut(sid) {
                view.select_all();
            }
        } else if action == ExplorerAction::CopyPath {
            if let Some(text) = self
                .state
                .explorer_views
                .get(sid)
                .and_then(|v| v.selected_paths_text())
            {
                self.copy_text(
                    &text,
                    false,
                    "toast.copied_path",
                    Vec::new(),
                    crate::adapters::ui::ToastScope::Surface(sid),
                );
            }
        } else if action == ExplorerAction::CopyFiles || is_cut_files {
            let paths: Vec<std::path::PathBuf> = self
                .state
                .explorer_views
                .get(sid)
                .map(|v| v.selected.owned().collect())
                .unwrap_or_default();
            if !paths.is_empty() {
                self.explorer_menu_set_clipboard(engine, sid, &paths, is_cut_files);
            }
        } else if action == ExplorerAction::PasteFiles {
            self.explorer_menu_paste(
                engine,
                sid,
                &[],
                &cwd,
                false,
                crate::intent::IntentOrigin::User {
                    source: crate::intent::UserSource::Shortcut("paste"),
                },
            );
        }
        self.mark_dirty();
        true
    }

    pub(super) fn handle_paste_shortcut(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let bindings = engine.settings.keybindings.paste.clone();
        if !matches_any_binding(&bindings, key, mods) {
            return false;
        }
        let sid = self.state.focused_surface_id(engine);
        self.run_paste_on(engine, sid)
    }

    /// `sid` 에 붙여 넣는다. 포커스를 다시 읽지 않는다.
    pub(crate) fn run_paste_on(&mut self, engine: &EngineRead<'_>, sid: Option<u32>) -> bool {
        // 키보드와 팔레트 붙여넣기 모두 사용자 입력으로 기록한다.
        if let Some(sid) = sid
            && let Some(target) =
                crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        {
            self.state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::RecordTyping {
                        target,
                        at: std::time::Instant::now(),
                    },
                )
                .from_user_shortcut("paste"),
            );
        }
        let st = sid.map_or(crate::state::FocusedSurfaceType::None, |sid| {
            self.state.surface_type_of(engine, sid)
        });
        // egui_paste는 플러그인이 처리하므로 터미널 입력으로 넘기지 않고, 대상 egui-mesh
        // surface에 Paste 이벤트를 보낸다. 클립보드에서 필요한 형식은 플러그인이 직접 읽는다.
        if st.kind_capability(engine, |d| d.egui_paste) {
            if let Some(sid) = sid.filter(|sid| egui_mesh_surface(engine, *sid)) {
                self.egui_mesh_push_paste(sid);
                self.mark_dirty();
            }
            return true;
        }
        self.paste_to_terminal_on(engine, sid);
        self.mark_dirty();
        true
    }
}

/// `sid` 가 egui-mesh surface 다. 다른 종류에는 편집 이벤트를 보내지 않는다.
fn egui_mesh_surface(engine: &EngineRead<'_>, sid: u32) -> bool {
    engine
        .find_surface_by_id(sid)
        .is_some_and(|surface| surface.mesh().is_some())
}
