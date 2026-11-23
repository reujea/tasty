//! 창이 가진 것(클립보드·복사 모드·파일 작업 확인)이 필요한 surface 액션. 대상 surface 는 호출자가 입력
//! 시점에 정해 넘기며 여기서 포커스를 다시 읽지 않는다.

use super::copy_paste::ExplorerAction;
use super::surface_actions::{self, PaletteRun, Received};
use crate::runtime::engine_read::EngineRead;
use crate::state::attach_pending::deferred::CommandSurfaceTarget;
use crate::view::main::MainView;

impl MainView {
    /// `sid` 에서 창 액션을 실행한다. 실행할 상태(선택 항목 등)가 없으면 false 다.
    fn run_view_surface_action(&mut self, engine: &EngineRead<'_>, id: &str, sid: u32) -> bool {
        match id {
            "copy_path" => {
                let Some(text) = self
                    .state
                    .explorer_views
                    .get(sid)
                    .and_then(|v| v.selected_paths_text())
                else {
                    return false;
                };
                self.copy_text(
                    &text,
                    false,
                    "toast.copied_path",
                    Vec::new(),
                    crate::adapters::ui::ToastScope::Surface(sid),
                );
                true
            }
            "explorer_rename" | "explorer_trash" => {
                let rename = id == "explorer_rename";
                let selected = self
                    .state
                    .explorer_views
                    .get(sid)
                    .map_or(0, |v| v.selected.owned().count());
                if selected == 0 || (rename && selected != 1) {
                    return false;
                }
                let action = if rename {
                    ExplorerAction::Rename
                } else {
                    ExplorerAction::Trash
                };
                self.run_explorer_item_key(engine, sid, action);
                true
            }
            "enter_copy_mode" => {
                self.enter_vi_copy_mode_on(engine, sid);
                true
            }
            // 팔레트 대상이 WebView 면 그 페이지의 전체 선택이다.
            "select_all" => {
                self.run_webview_edit_on(sid, crate::webview::WebViewEditAction::SelectAll)
            }
            _ => false,
        }
    }

    /// 생성된 surface 에 걸어 두었던 창 액션을 실행한다.
    pub(crate) fn run_held_view_actions(&mut self, engine: &EngineRead<'_>) {
        for (id, sid) in self.state.attach_pending.deferred.take_view() {
            if !engine.has_surface(sid) {
                tracing::info!("deferred action '{id}' dropped: its surface {sid} is gone");
                continue;
            }
            if !self.run_view_surface_action(engine, id, sid) {
                tracing::info!("deferred action '{id}' did nothing on surface {sid}");
            }
            self.base.state.dirty = true;
        }
    }

    /// 팔레트에서 고른 호스트 명령 중 surface 하나를 대상으로 하는 액션을 Enter 때 정한 대상으로 실행한다.
    /// surface 액션이 아니면 true 이고, 호출자가 액션 ID 로 실행한다.
    pub(crate) fn run_palette_surface_command(
        &mut self,
        engine: &EngineRead<'_>,
        id: &'static str,
        target: Option<CommandSurfaceTarget>,
    ) -> bool {
        match surface_actions::run_palette_action(&mut self.state, engine, id, target) {
            PaletteRun::Dispatch => return true,
            PaletteRun::Done => {}
            PaletteRun::View(id, sid) => {
                if !self.run_view_surface_action(engine, id, sid) {
                    tracing::info!("palette action '{id}' did nothing on surface {sid}");
                }
            }
        }
        self.base.state.dirty = true;
        false
    }

    /// native WebView 가 넘긴 키가 surface 액션이면 키를 받은 `sid` 에서 실행한다. 처리했으면 `Some(true)`,
    /// 그 surface 가 받지 않아 페이지로 넘길 키면 `Some(false)`, surface 액션 키가 아니면 `None` 이다. 그
    /// surface 가 받지 않지만 같은 키가 전역 단축키이기도 하면 전역 단축키만 `target`(키를 받은 surface 의
    /// 대상)으로 처리한 결과다.
    pub(crate) fn run_received_surface_key(
        &mut self,
        engine: &EngineRead<'_>,
        key: &winit::keyboard::Key,
        mods: winit::keyboard::ModifiersState,
        sid: u32,
        target: &crate::state::command_target::CommandTarget,
    ) -> Option<bool> {
        let kb = &engine.settings.keybindings;
        match surface_actions::run_received_key(&mut self.state, engine, kb, key, mods, sid) {
            Received::Unbound => None,
            Received::Pass => Some(false),
            Received::Global => Some(self.run_target_shortcuts(
                engine,
                key,
                mods,
                target,
                false,
                Some(crate::adapters::ui::input::key_level::PathLevels::all()),
            )),
            Received::Done => Some(true),
            Received::View(id) => Some(self.run_view_surface_action(engine, id, sid)),
        }
    }
}
