//! 터미널 링크 클릭 수식키의 판정과 수식키 도움말 안내가 같은 조합을 쓰게 한다.
//! 설정값 `alt`는 물리 Alt 키(macOS Option)를 뜻한다. 바인딩 토큰 `alt`는 macOS에서 Command라서
//! 토큰으로 옮길 때 OS별 변환이 필요하다. 판정도 이 조합을 [`token_axes`](crate::token_axes) 규칙으로
//! 대조하므로 안내가 실제 클릭 키와 다른 줄에 붙지 않는다.

use tasty_settings::LinkModifier;
use tasty_settings::keybindings::parse::Combo;
use winit::keyboard::ModifiersState;

use crate::token_axes_for;

/// 링크 클릭을 일으키는 수식키를 바인딩 토큰 조합으로 돌려준다. `None`은 수식키가 필요 없어 조합이 없다.
pub fn link_click_combo(modifier: LinkModifier) -> Option<Combo> {
    link_click_combo_for(modifier, cfg!(target_os = "macos"))
}

/// [`link_click_combo`]의 OS를 인자로 받는 판. 시험이 다른 OS의 매핑을 확인할 때 쓴다.
pub fn link_click_combo_for(modifier: LinkModifier, macos: bool) -> Option<Combo> {
    match modifier {
        LinkModifier::Ctrl => Some(Combo {
            ctrl: true,
            ..Combo::default()
        }),
        // 물리 Alt 키. macOS에서는 Option이고 토큰 `option` 축에 있다.
        LinkModifier::Alt if macos => Some(Combo {
            option: true,
            ..Combo::default()
        }),
        LinkModifier::Alt => Some(Combo {
            alt: true,
            ..Combo::default()
        }),
        LinkModifier::None => None,
    }
}

/// 지금 누른 수식키가 링크 클릭 조건을 만족하는지 돌려준다. Shift는 보지 않는다. `None`은 항상 참이다.
pub fn link_click_modifiers_held(modifier: LinkModifier, mods: ModifiersState) -> bool {
    link_click_modifiers_held_for(modifier, mods, cfg!(target_os = "macos"))
}

/// [`link_click_modifiers_held`]의 OS를 인자로 받는 판.
pub(crate) fn link_click_modifiers_held_for(
    modifier: LinkModifier,
    mods: ModifiersState,
    macos: bool,
) -> bool {
    let Some(want) = link_click_combo_for(modifier, macos) else {
        return true;
    };
    let (alt, option) = token_axes_for(mods, macos);
    mods.control_key() == want.ctrl && alt == want.alt && option == want.option
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 공용 변환을 쓰기 전의 클릭 판정(`LinkModifier::matches(control, alt, super)`)을 그대로 옮긴 기준.
    fn previous_rule(modifier: LinkModifier, mods: ModifiersState) -> bool {
        let (c, a, s) = (mods.control_key(), mods.alt_key(), mods.super_key());
        match modifier {
            LinkModifier::Ctrl => c && !a && !s,
            LinkModifier::Alt => a && !c && !s,
            LinkModifier::None => true,
        }
    }

    /// Ctrl·Alt·Super·Shift 16가지 상태 전부.
    fn every_state() -> Vec<ModifiersState> {
        let axes = [
            ModifiersState::CONTROL,
            ModifiersState::ALT,
            ModifiersState::SUPER,
            ModifiersState::SHIFT,
        ];
        (0u8..16)
            .map(|bits| {
                axes.iter()
                    .enumerate()
                    .filter(|(i, _)| bits & (1 << i) != 0)
                    .fold(ModifiersState::empty(), |acc, (_, m)| acc | *m)
            })
            .collect()
    }

    /// OS(macOS·그 밖) × 설정 저장값(ctrl·alt·none) × 수식키 16상태에서 클릭 판정이 공용 변환 전과 같다.
    #[test]
    fn held_check_matches_previous_rule_for_every_os_and_setting() {
        let mut checked = 0;
        for macos in [true, false] {
            for stored in ["ctrl", "alt", "none"] {
                let modifier = LinkModifier::parse(stored);
                for mods in every_state() {
                    assert_eq!(
                        link_click_modifiers_held_for(modifier, mods, macos),
                        previous_rule(modifier, mods),
                        "{stored} {mods:?} macos={macos}"
                    );
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 2 * 3 * 16);
    }

    /// macOS에서 설정값 alt는 물리 Option(winit alt)으로 클릭하고, 안내 조합은 토큰 option이다.
    #[test]
    fn macos_alt_link_is_the_option_token() {
        let combo = link_click_combo_for(LinkModifier::Alt, true).expect("조합");
        assert_eq!(
            combo,
            Combo {
                option: true,
                ..Combo::default()
            }
        );
        assert!(link_click_modifiers_held_for(
            LinkModifier::Alt,
            ModifiersState::ALT,
            true
        ));
        assert!(!link_click_modifiers_held_for(
            LinkModifier::Alt,
            ModifiersState::SUPER,
            true
        ));
    }

    /// macOS 밖에서는 설정값 alt와 토큰 alt가 같은 물리 Alt다. ctrl은 모든 OS에서 토큰 ctrl이다.
    #[test]
    fn other_os_alt_link_is_the_alt_token() {
        assert_eq!(
            link_click_combo_for(LinkModifier::Alt, false),
            Some(Combo {
                alt: true,
                ..Combo::default()
            })
        );
        let ctrl = Some(Combo {
            ctrl: true,
            ..Combo::default()
        });
        assert_eq!(link_click_combo_for(LinkModifier::Ctrl, true), ctrl);
        assert_eq!(link_click_combo_for(LinkModifier::Ctrl, false), ctrl);
    }

    #[test]
    fn none_has_no_combo() {
        assert_eq!(link_click_combo_for(LinkModifier::None, true), None);
        assert_eq!(link_click_combo_for(LinkModifier::None, false), None);
    }
}
