//! 탐색기에 포커스가 있을 때 선택한 항목에 하는 키 동작(이름 변경·휴지통·열기·선택 해제)과 목록 이동 키.
//! 이름 변경·휴지통은 컨텍스트 메뉴와 같은 함수를 불러 원격 mirror 차단과 확인 절차를 공유한다.

use std::path::PathBuf;

use tasty_key_match::matches_any_binding;
use winit::keyboard::{Key, ModifiersState};

use super::copy_paste::ExplorerAction;
use crate::adapters::ui::surface::explorer::view::cursor::Step;
use crate::runtime::engine_read::EngineRead;
use crate::view::main::MainView;
use crate::view::ui::View as _;

/// 이동 키와 Shift 로 늘리는 키. `EXPLORER_LIST_BINDING_FIELDS` 와 같은 순서다.
pub(super) fn list_step(
    kb: &crate::settings::KeybindingSettings,
    key: &Key,
    mods: ModifiersState,
) -> Option<(Step, bool)> {
    let pairs = [
        (Step::Up, &kb.explorer_cursor_up, &kb.explorer_extend_up),
        (
            Step::Down,
            &kb.explorer_cursor_down,
            &kb.explorer_extend_down,
        ),
        (
            Step::Left,
            &kb.explorer_cursor_left,
            &kb.explorer_extend_left,
        ),
        (
            Step::Right,
            &kb.explorer_cursor_right,
            &kb.explorer_extend_right,
        ),
        (
            Step::Home,
            &kb.explorer_cursor_home,
            &kb.explorer_extend_home,
        ),
        (Step::End, &kb.explorer_cursor_end, &kb.explorer_extend_end),
        (
            Step::PageUp,
            &kb.explorer_cursor_page_up,
            &kb.explorer_extend_page_up,
        ),
        (
            Step::PageDown,
            &kb.explorer_cursor_page_down,
            &kb.explorer_extend_page_down,
        ),
    ];
    pairs.into_iter().find_map(|(step, plain, extend)| {
        if matches_any_binding(extend, key, mods) {
            Some((step, true))
        } else {
            matches_any_binding(plain, key, mods).then_some((step, false))
        }
    })
}

/// egui 가 무언가를 끄는 중인지. 그동안 Esc 는 끌기 취소에 쓰이므로 선택 해제가 받지 않는다.
pub(crate) fn egui_dragging(ctx: &egui::Context) -> bool {
    egui::DragAndDrop::has_any_payload(ctx) || ctx.dragged_id().is_some()
}

/// 포커스된 탐색기가 이 키로 원격 주소 확인을 버리는지: 수식키 없는 Esc 이고, 주소창을 편집 중이 아니며(편집 중이면
/// PathField 가 받는다) 주소 확인을 기다린다. 단계 4 의 후보(`key_nodes::matching_levels`)와 실행부
/// (`MainView::cancel_explorer_address_check`)가 같이 쓴다.
pub(crate) fn explorer_cancels_address_check(
    state: &crate::state::MainViewState,
    engine: &EngineRead<'_>,
    key: &Key,
    mods: ModifiersState,
) -> bool {
    *key == Key::Named(winit::keyboard::NamedKey::Escape)
        && mods.is_empty()
        && super::focused_explorer_surface_id(state, engine)
            .and_then(|sid| state.explorer_views.get(sid))
            .is_some_and(|v| !v.addr_editing && v.address_waiting().is_some())
}

/// 포커스된 탐색기가 선택 해제 바인딩을 쓰는지: 글자 입력 중이 아니고, 키 사건 시점에 끄는 중이 아니며
/// (`MainViewState::key_event_dragging`), 영역 선택 중이거나 고른 항목이 있다. 단계 4 의 후보와 실행부
/// (`MainView::explorer_key_clear_selection`)가 같이 쓴다.
pub(crate) fn explorer_takes_clear_selection(
    state: &crate::state::MainViewState,
    engine: &EngineRead<'_>,
) -> bool {
    !state.key_event_dragging
        && !super::focused_explorer_typing(state, engine)
        && super::focused_explorer_surface_id(state, engine)
            .and_then(|sid| state.explorer_views.get(sid))
            .is_some_and(|v| v.marquee.active() || !v.selected.is_empty())
}

impl MainView {
    /// 포커스된 탐색기가 원격 주소 확인을 기다리는 중이면 `Esc` 가 그 확인을 버린다
    /// ([`explorer_cancels_address_check`]). 주소창은 현재 폴더로 돌아간다.
    pub(super) fn cancel_explorer_address_check(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        if !explorer_cancels_address_check(&self.state, engine, key, mods) {
            return false;
        }
        let Some(view) = super::focused_explorer_surface_id(&self.state, engine)
            .and_then(|sid| self.state.explorer_views.get_mut(sid))
        else {
            return false;
        };
        view.cancel_address_probe();
        self.mark_dirty();
        true
    }

    /// 포커스된 탐색기의 현재 항목을 옮긴다. 앞단(`handle_explorer_shortcut`)이 capability 와 글자
    /// 입력 중인지를 이미 확인했다. 영역 선택 중에는 키를 소비만 한다.
    pub(super) fn handle_explorer_list_key(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let kb = &engine.settings.keybindings;
        if matches_any_binding(&kb.explorer_open, key, mods) {
            return self.explorer_key_open(engine);
        }
        if matches_any_binding(&kb.explorer_clear_selection, key, mods) {
            return self.explorer_key_clear_selection(engine);
        }
        let Some((step, extend)) = list_step(kb, key, mods) else {
            return false;
        };
        let Some(sid) = super::focused_explorer_surface_id(&self.state, engine) else {
            return false;
        };
        if let Some(view) = self.state.explorer_views.get_mut(sid)
            && !view.marquee.active()
        {
            view.move_cursor(step, extend);
            self.mark_dirty();
        }
        true
    }

    /// 고른 항목을 연다(규칙은 `open_selected_actions`). 열 항목이 없으면 키를 소비하지 않는다.
    fn explorer_key_open(&mut self, engine: &EngineRead<'_>) -> bool {
        let Some(sid) = super::focused_explorer_surface_id(&self.state, engine) else {
            return false;
        };
        let Some(view) = self.state.explorer_views.get(sid) else {
            return false;
        };
        if view.marquee.active() {
            return true;
        }
        let actions = view.open_selected_actions();
        if actions.is_empty() {
            return false;
        }
        for action in actions {
            crate::adapters::ui::egui_panels::apply_explorer_action(
                &mut self.state,
                engine,
                sid,
                action,
            );
        }
        self.mark_dirty();
        true
    }

    /// 선택을 모두 푼다. 현재 항목은 그대로 두어 다음 이동 키가 그 자리에서 시작한다. 쓰지 않는 상태
    /// ([`explorer_takes_clear_selection`])면 키를 소비하지 않고, 영역 선택 중에는 소비만 한다.
    fn explorer_key_clear_selection(&mut self, engine: &EngineRead<'_>) -> bool {
        if !explorer_takes_clear_selection(&self.state, engine) {
            return false;
        }
        let Some(view) = super::focused_explorer_surface_id(&self.state, engine)
            .and_then(|sid| self.state.explorer_views.get_mut(sid))
        else {
            return false;
        };
        if view.marquee.active() {
            return true;
        }
        view.clear_selection();
        self.mark_dirty();
        true
    }

    /// 선택이 없으면 아무것도 하지 않는다. 이름 변경은 하나만 골랐을 때만 연다.
    pub(super) fn run_explorer_item_key(
        &mut self,
        engine: &EngineRead<'_>,
        sid: u32,
        action: ExplorerAction,
    ) {
        let mut paths: Vec<PathBuf> = self
            .state
            .explorer_views
            .get(sid)
            .map(|v| v.selected.owned().collect())
            .unwrap_or_default();
        paths.sort();
        match action {
            ExplorerAction::Rename if paths.len() == 1 => {
                if let Some(binding) =
                    crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
                {
                    self.explorer_menu_rename(engine, sid, &paths, binding);
                }
            }
            ExplorerAction::Trash if !paths.is_empty() => {
                self.explorer_key_trash(engine, sid, &paths);
            }
            _ => {}
        }
    }

    /// 휴지통으로 옮긴다. 가역적이라 확인을 묻지 않는 것은 메뉴와 같다.
    fn explorer_key_trash(&mut self, engine: &EngineRead<'_>, sid: u32, paths: &[PathBuf]) {
        // (docs/features/remote-attach/index.md) 원격 쓰기를 알리지 않은 서버의 mirror explorer 는 파일을 바꾸지 않는다.
        if self.state.refuse_remote_write(engine, sid) {
            return;
        }
        self.state.request_explorer_file(
            engine,
            sid,
            crate::app::explorer_files::Operation::Trash(paths.to_vec()),
            crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::Shortcut("explorer_trash"),
            },
        );
    }
}
