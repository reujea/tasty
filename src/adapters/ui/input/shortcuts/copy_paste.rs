//! Copy/Paste 단축키.

use winit::keyboard::{Key, ModifiersState};

use crate::view::main::MainView;
use crate::view::main::selection::should_copy_via_focused_selection;
use crate::view::ui::View as _;
use tasty_key_match::matches_any_binding;

/// explorer 서피스의 파일 액션. 줌과 같은 이유로 판별과 실행을 가른다 —
/// 단발 키는 키로, 명령 팔레트는 `action_id` 로 이 값을 정하고 실행부는 하나다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    SelectAll,
    CopyPath,
    CopyFiles,
    CutFiles,
    PasteFiles,
}

impl MainView {
    pub(super) fn handle_copy_shortcut(&mut self, key: &Key, mods: ModifiersState) -> bool {
        let bindings = self.core_state.settings.keybindings.copy.clone();
        if !matches_any_binding(&bindings, key, mods) {
            return false;
        }
        self.run_copy()
    }

    /// 복사 실행 — 키 경로와 명령 팔레트가 공유한다. 어느 것도 처리하지 못하면 `false`
    /// 이고, 그때 키 경로는 뒤의 explorer 파일 복사로 흘려보낸다.
    pub(crate) fn run_copy(&mut self) -> bool {
        // Paste cooldown: Ctrl+V 직후 짧은 시간 안에 들어온 Ctrl+C는 사용자의
        // 오타(옆 키 누름)로 간주하고 통째로 무시한다. SIGINT도, 클립보드 복사도
        // 일어나지 않으며 toast로만 알린다.
        if let Some(t) = self.last_terminal_paste_at
            && t.elapsed() < crate::view::main::PASTE_CTRL_C_COOLDOWN
        {
            let scope = crate::adapters::ui::ToastScope::Surface(
                self.state.focused_surface_id(&self.core_state).unwrap_or(0),
            );
            self.state
                .toasts
                .push_info(crate::i18n::t("toast.ctrl_c_ignored_after_paste"), scope);
            self.mark_dirty();
            return true;
        }
        // 터미널 우클릭 메뉴의 "copy"(surface 무관 전역 selection 관례,
        // `copy_selection_to_clipboard` 자체엔 포커스 체크 없음)와 달리, 키보드 Ctrl+C는
        // 여기서 포커스와 selection 의 surface 가 일치할 때만 그 함수를 부른다 — 안 그러면
        // 다른 surface(예: 드래그 선택했던 터미널)로 포커스를 옮긴 뒤 Ctrl+C 를 눌렀을 때
        // stale selection 이 조용히 복사되고 이 surface 자신의 copy 처리(예: Explorer 의
        // `handle_explorer_shortcut`)로 흘러가지 못한다.
        let sel_surface_id = self.text_selection.as_ref().map(|s| s.surface_id);
        let focused = self.state.focused_surface_id(&self.core_state);
        let selection_targets_focus = should_copy_via_focused_selection(sel_surface_id, focused);
        if selection_targets_focus && self.copy_selection_to_clipboard() {
            self.mark_dirty();
            return true;
        }
        let st = self.state.focused_surface_type(&self.core_state);
        // egui_copy capability 를 가진 kind(예: markdown)는 선택 텍스트를 plugin 자신의
        // egui Context 가 복사하도록 Copy wire 이벤트를 그 surface 에 forward 한다(kind
        // 하드코딩 없음). host 자신의 top-level egui_ctx 는 이 plugin 의 위젯을 갖고
        // 있지 않으므로 대상이 될 수 없다 — 반드시 focused_egui_mesh_surface_id() 로
        // 찾은 실제 plugin surface 로 보낸다.
        if st.kind_capability(&self.core_state, |d| d.egui_copy)
            && let Some(sid) = self.focused_egui_mesh_surface_id()
        {
            self.egui_mesh_push_copy(sid);
            self.mark_dirty();
            return true;
        }
        false
    }

    /// select-all / copy-path(선택 항목 경로 복사) / 파일 복사·잘라내기·붙여넣기 단축키.
    /// clipboard 차용이 필요하므로 keybinding free-fn(=clipboard 미접근) 이 아니라
    /// MainView 메서드로 둔다. 포커스가 copy_path capability 를 가진 kind(예: explorer)가
    /// 아니면 false 를 반환해 일반 단축키 경로로 흘려보낸다(kind 하드코딩 없음).
    ///
    /// 파일 복사(`kb.copy`)/잘라내기(`kb.cut`)/붙여넣기(`kb.paste`) 는 우클릭 컨텍스트
    /// 메뉴(`explorer_menu_set_clipboard`/`explorer_menu_paste`, redraw.rs)와 동일 로직을
    /// 재사용한다 — fs 동작(충돌 시 (copy) 접미사, 자기 자신/하위 붙여넣기 거부 등)이
    /// 키보드/마우스 경로에서 갈라지지 않도록.
    pub(super) fn handle_explorer_shortcut(&mut self, key: &Key, mods: ModifiersState) -> bool {
        if !self
            .state
            .focused_surface_type(&self.core_state)
            .kind_capability(&self.core_state, |d| d.copy_path)
        {
            return false;
        }
        let kb = &self.core_state.settings.keybindings;
        let action = if matches_any_binding(&kb.select_all, key, mods) {
            ExplorerAction::SelectAll
        } else if matches_any_binding(&kb.copy_path, key, mods) {
            ExplorerAction::CopyPath
        } else if matches_any_binding(&kb.copy, key, mods) {
            ExplorerAction::CopyFiles
        } else if matches_any_binding(&kb.cut, key, mods) {
            ExplorerAction::CutFiles
        } else if matches_any_binding(&kb.paste, key, mods) {
            ExplorerAction::PasteFiles
        } else {
            return false;
        };
        self.run_explorer_action(action)
    }

    /// explorer 파일 액션 실행 — 키 경로와 명령 팔레트가 공유한다.
    ///
    /// 포커스가 `copy_path` capability 를 가진 kind 가 아니면 `false` 를 돌려 다음 경로로
    /// 흘려보내는 것까지 키 경로와 같다(그 판정을 여기서 다시 한다 — 팔레트는 키 경로의
    /// 앞선 게이트를 안 거치고 들어온다).
    pub(crate) fn run_explorer_action(&mut self, action: ExplorerAction) -> bool {
        if !self
            .state
            .focused_surface_type(&self.core_state)
            .kind_capability(&self.core_state, |d| d.copy_path)
        {
            return false;
        }
        let is_cut_files = action == ExplorerAction::CutFiles;
        let Some(sid) = super::focused_explorer_surface_id(&self.state, &self.core_state) else {
            return true;
        };
        if action == ExplorerAction::SelectAll {
            if let Some(view) = self.state.explorer_views.get_mut(sid) {
                view.select_all();
            }
        } else if action == ExplorerAction::CopyPath {
            if let Some(text) = self
                .state
                .explorer_views
                .get(sid)
                .and_then(|v| v.selected_paths_text())
                && let Some(cb) = self.clipboard.as_mut()
            {
                cb.set_text(&text);
                self.state.toasts.push_info(
                    crate::i18n::t("toast.copied_path"),
                    crate::adapters::ui::ToastScope::Surface(sid),
                );
            }
        } else if action == ExplorerAction::CopyFiles || is_cut_files {
            let paths: Vec<std::path::PathBuf> = self
                .state
                .explorer_views
                .get(sid)
                .map(|v| v.selected.iter().cloned().collect())
                .unwrap_or_default();
            if !paths.is_empty() {
                self.explorer_menu_set_clipboard(sid, &paths, is_cut_files);
            }
        } else if action == ExplorerAction::PasteFiles
            && let Some(cwd) = super::focused_explorer_cwd(&self.state, &self.core_state)
        {
            self.explorer_menu_paste(sid, &[], &cwd, false);
        }
        self.mark_dirty();
        true
    }

    pub(super) fn handle_paste_shortcut(&mut self, key: &Key, mods: ModifiersState) -> bool {
        let bindings = self.core_state.settings.keybindings.paste.clone();
        if !matches_any_binding(&bindings, key, mods) {
            return false;
        }
        self.run_paste()
    }

    /// 붙여넣기 실행 — 키 경로와 명령 팔레트가 공유한다.
    pub(crate) fn run_paste(&mut self) -> bool {
        // 붙여넣기는 사용자 입력이다 — `surface.is_typing` 이 사람 있음을 말하게 기록한다.
        // 키 경로는 수식키 키다운이 이미 기록하지만 명령 팔레트 경로는 키가 surface 에
        // 닿지 않아, 여기가 두 경로가 만나는 유일한 자리다(docs/adr/0560-paste-is-user-input-and-is-recorded-where-both-paste-paths-meet.md).
        if let Some(sid) = self.state.focused_surface_id(&self.core_state) {
            self.core_state.record_typing(sid);
        }
        let st = self.state.focused_surface_type(&self.core_state);
        // egui_paste capability 를 가진 kind(예: image)의 paste 는 plugin 이 자기
        // egui-mesh 입력 / `image.paste` IPC 로 처리한다 — host 는 terminal paste 로
        // 흘리지 않고 소비만 한다(kind 하드코딩 없음).
        if st.kind_capability(&self.core_state, |d| d.egui_paste) {
            return true;
        }
        self.paste_to_terminal();
        self.mark_dirty();
        true
    }
}
