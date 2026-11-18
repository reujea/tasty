//! 설정된 슬롯·다음·이전 키로 탭, 워크스페이스, 카테고리를 전환한다.
//! 다음·이전 키가 슬롯 키와 겹치면 다음·이전을 먼저 처리한다.
//!
//! 개별 지정은 완성된 키 조합을 사용하므로 문자 키 검사보다 먼저 처리한다.
//! 나머지 설정은 번호 오버레이와 같은 [`switch_target_for`]로 대상을 고른다.
//! 사용자 포커스를 바꾸는 동작이므로 사용자 키 입력에서만 호출하며 release IPC에는 노출하지 않는다.

use crate::runtime::engine_read::EngineRead;
use winit::keyboard::{Key, ModifiersState};

use crate::adapters::ui::switch_overlay::{SwitchTarget, switch_target_for};
use crate::settings::KeybindingSettings;
use crate::view::main::MainView;
use tasty_key_match::matches_binding;

/// 완성된 키 조합이 일치하는 첫 슬롯의 인덱스.
fn find_matching_individual_slot(
    slots: &[String],
    key: &Key,
    mods: ModifiersState,
) -> Option<usize> {
    slots
        .iter()
        .position(|combo| !combo.is_empty() && matches_binding(combo, key, mods))
}

impl MainView {
    #[allow(clippy::too_many_arguments)] // reason: quick-switch dispatch context(정규화된 modifier bool 4개 + 원본 ModifiersState)
    pub(super) fn handle_numeric_switch_shortcuts(
        state: &mut crate::state::MainViewState,
        engine: &EngineRead<'_>,
        kb: &crate::settings::KeybindingSettings,
        key: &Key,
        mods: ModifiersState,
        ctrl: bool,
        shift: bool,
        alt: bool,
        option: bool,
        target: &crate::state::command_target::CommandTarget,
    ) -> bool {
        // 개별 지정은 F5·화살표도 허용하므로 Character 검사보다 먼저 처리한다.
        // 설정 저장 시 대상 간 키 충돌을 검사한다.
        if kb.tab_switch_modifier == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER {
            if matches_binding(kb.tab_next_key(), key, mods) {
                state.step_visible_tab_in(engine, target, true);
                return true;
            }
            if matches_binding(kb.tab_prev_key(), key, mods) {
                state.step_visible_tab_in(engine, target, false);
                return true;
            }
            if let Some(index) = find_matching_individual_slot(&kb.tab_switch_slot_keys, key, mods)
            {
                state.goto_tab_in(engine, target.pane, index);
                return true;
            }
        }
        if kb.workspace_switch_modifier == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER {
            if matches_binding(kb.workspace_next_key(), key, mods) {
                step_target_workspace(state, engine, target, 1);
                return true;
            }
            if matches_binding(kb.workspace_prev_key(), key, mods) {
                step_target_workspace(state, engine, target, -1);
                return true;
            }
            if let Some(local) =
                find_matching_individual_slot(&kb.workspace_switch_slot_keys, key, mods)
            {
                if engine.settings.general.workspace_categories_enabled {
                    if let Some(from) = target.workspace_index(engine) {
                        state.switch_workspace_in_category_of(engine, from, local);
                    }
                } else {
                    state.switch_workspace(engine, local);
                }
                return true;
            }
        }
        if kb.category_switch_modifier == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER
            && engine.settings.general.workspace_categories_enabled
        {
            if matches_binding(kb.category_next_key(), key, mods) {
                step_target_category(state, engine, target, 1);
                return true;
            }
            if matches_binding(kb.category_prev_key(), key, mods) {
                step_target_category(state, engine, target, -1);
                return true;
            }
            if let Some(section) =
                find_matching_individual_slot(&kb.category_switch_slot_keys, key, mods)
            {
                state.switch_to_category(engine, section);
                return true;
            }
        }

        // 슬롯 키는 숫자뿐 아니라 q 같은 문자도 가능하다.
        let Key::Character(c) = key else {
            return false;
        };
        let ch = c.as_str();

        // 개별 지정은 위에서 처리했다. 나머지는 번호 오버레이와 같은 규칙을 사용한다.
        match switch_target_for(kb, ctrl, shift, alt, option) {
            Some(SwitchTarget::Tab) => {
                if ch == kb.tab_next_key() {
                    state.step_visible_tab_in(engine, target, true);
                    return true;
                }
                if ch == kb.tab_prev_key() {
                    state.step_visible_tab_in(engine, target, false);
                    return true;
                }
                if let Some(index) = kb.tab_switch_slot_keys.iter().position(|k| k == ch) {
                    state.goto_tab_in(engine, target.pane, index);
                    return true;
                }
            }
            Some(SwitchTarget::Workspace) => {
                if ch == kb.workspace_next_key() {
                    step_target_workspace(state, engine, target, 1);
                    return true;
                }
                if ch == kb.workspace_prev_key() {
                    step_target_workspace(state, engine, target, -1);
                    return true;
                }
                if let Some(local) = kb.workspace_switch_slot_keys.iter().position(|k| k == ch) {
                    // 카테고리를 사용하면 카테고리 내 인덱스, 아니면 전역 인덱스다.
                    if engine.settings.general.workspace_categories_enabled {
                        if let Some(from) = target.workspace_index(engine) {
                            state.switch_workspace_in_category_of(engine, from, local);
                        }
                    } else {
                        state.switch_workspace(engine, local);
                    }
                    return true;
                }
            }
            Some(SwitchTarget::Category) => {
                if !engine.settings.general.workspace_categories_enabled {
                    return false;
                }
                if ch == kb.category_next_key() {
                    step_target_category(state, engine, target, 1);
                    return true;
                }
                if ch == kb.category_prev_key() {
                    step_target_category(state, engine, target, -1);
                    return true;
                }
                if let Some(section) = kb.category_switch_slot_keys.iter().position(|k| k == ch) {
                    state.switch_to_category(engine, section);
                    return true;
                }
            }
            None => {}
        }

        false
    }
}

/// 명령 대상 workspace 에서 다음·이전 workspace 로 옮긴다. 대상 workspace 가 없으면 하지 않는다.
fn step_target_workspace(
    state: &mut crate::state::MainViewState,
    engine: &EngineRead<'_>,
    target: &crate::state::command_target::CommandTarget,
    delta: isize,
) {
    if let Some(from) = target.workspace_index(engine) {
        state.step_workspace_from(engine, from, delta);
    }
}

/// 명령 대상 workspace 의 카테고리에서 다음·이전 카테고리로 옮긴다. 대상 workspace 가 없으면 하지 않는다.
/// 카테고리 슬롯 키는 섹션 번호로 대상을 정하고 활성 workspace 를 읽지 않으므로 대상이 필요 없다.
fn step_target_category(
    state: &mut crate::state::MainViewState,
    engine: &EngineRead<'_>,
    target: &crate::state::command_target::CommandTarget,
    delta: isize,
) {
    if let Some(from) = target.workspace_index(engine) {
        state.step_category_from(engine, from, delta);
    }
}
