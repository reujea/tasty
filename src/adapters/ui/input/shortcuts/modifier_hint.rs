//! 누른 수식키를 포함하는 조합별 단축키와 역할 안내를 만든다. UI 상태는 바꾸지 않는다.
//! ctrl/alt/option/shift 네 축을 사용한다. alt는 macOS에서 Command, option은 macOS에서 Option, 다른 OS에서 Win·Super에 대응한다.
//! 조합 크기순으로, 같은 크기는 Ctrl→Alt→Option→Shift 순으로 정렬한다.
//!
//! 설정 필드와 script_bindings, 전달받은 PluginBindingInput을 읽는다.
//! 더블탭·수식키 없는 키·수식키 단독 입력은 제외한다.
//! 탭/workspace/카테고리/링크 역할은 현재 설정을 사용한다. 링크 역할은 클릭을 일으키는 물리 키의 조합에 붙는다. 카테고리는 folders가 켜졌을 때만 표시한다.
//! 마우스 캡처 우회는 Shift를 포함한 입력에 적용되지만 안내는 Shift 단독 섹션에 한 번만 표시한다.
//! 빈 조합도 유지해 바인딩 없음을 보여준다(docs/design/policies/key-mapping.md).
//!
//! 현재 modifier_hint_overlay는 플러그인 바인딩에 빈 목록을 전달한다.
//! 모델은 입력받은 플러그인 항목을 모두 포함하며 소유자·포커스 선택은 호출자 책임이다.

use tasty_settings::{KeybindingSettings, LinkModifier};

use tasty_key_match::parse_binding;

pub use tasty_settings::keybindings::parse::{Combo, combos_containing_all};

/// 헤더에 수식키가 있으므로 행에는 나머지 키만 표시한다. 원본 binding은 유지한다.
/// 표기 순서에 영향을 받지 않도록 파싱하며 실패하면 원문을 반환한다.
pub fn binding_leaf(binding: &str) -> &str {
    parse_binding(binding).map(|p| p.key).unwrap_or(binding)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintRow {
    pub source: HintRowSource,
    /// 이 행의 원본 바인딩 문자열(예: `"ctrl+shift+t"`). 오버레이(`modifier_hint_overlay`)가 키캡으로 분해해 그린다.
    pub binding: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintRowSource {
    /// 고정 호스트 액션 — 라벨 i18n 키(`settings.keybindings.*_label`).
    Host { label_key: &'static str },
    /// 사용자 스크립트 — `script_id`. 오버레이가 `ScriptRegistry` 로 이름을 해석한다.
    Script { script_id: String },
    /// Plugin command — `plugin_id` + command title i18n 키. 오버레이가 "plugin_id: title" 로 표기.
    Plugin {
        plugin_id: String,
        title_i18n_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintRole {
    /// Shift 단독 조합 — TUI 마우스 캡처 임시 우회.
    MouseCaptureBypass,
    /// `tab_switch_modifier` 단독 — 탭 전환 + 숫자 오버레이.
    TabSwitch,
    /// `workspace_switch_modifier` 단독 — 워크스페이스 전환 + 숫자 오버레이.
    WorkspaceSwitch,
    /// `category_switch_modifier`(기본 `ctrl+shift`) — 카테고리 전환 + 헤더 숫자 오버레이(folders on).
    CategorySwitch,
    /// `link_click_modifier` 단독 — modifier+클릭 링크 열기.
    LinkClick,
}

impl HintRole {
    pub fn desc_key(&self) -> &'static str {
        match self {
            HintRole::MouseCaptureBypass => "modifier_hint.role.mouse_capture_bypass",
            HintRole::TabSwitch => "modifier_hint.role.tab_switch",
            HintRole::WorkspaceSwitch => "modifier_hint.role.workspace_switch",
            HintRole::CategorySwitch => "modifier_hint.role.category_switch",
            HintRole::LinkClick => "modifier_hint.role.link_click",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintSection {
    pub combo: Combo,
    /// 이 조합에 매핑된 바인딩 항목(필드 순서 → 스크립트 → plugin 순).
    pub rows: Vec<HintRow>,
    pub roles: Vec<HintRole>,
}

impl HintSection {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.roles.is_empty()
    }
}

/// 호출자가 등록표·override·포커스를 반영한 플러그인 바인딩 입력.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginBindingInput {
    pub plugin_id: String,
    pub title_i18n_key: String,
    /// 실제 매칭에 쓰는 키 목록(effective). 빈 벡터면 미할당.
    pub bindings: Vec<String>,
}

/// held의 수식키를 모두 포함하는 조합을 반환한다. 설정의 액션·스크립트·전환 역할과
/// 전달된 플러그인 입력을 포함하며 빈 섹션도 유지한다.
/// 예외로 macOS 밖에서 option(Win·Super)을 누른 동안 채워진 섹션이 하나도 없으면 빈 목록을 돌려준다
/// ([`silent_option_hold`]).
pub fn build_hint_sections(
    held: Combo,
    kb: &KeybindingSettings,
    link_click_modifier: &str,
    categories_enabled: bool,
    plugin_bindings: &[PluginBindingInput],
) -> Vec<HintSection> {
    build_hint_sections_for(
        held,
        kb,
        link_click_modifier,
        categories_enabled,
        plugin_bindings,
        cfg!(target_os = "macos"),
    )
}

/// [`build_hint_sections`]의 OS를 인자로 받는 판. 시험이 macOS 매핑을 다른 OS에서 확인할 때 쓴다.
fn build_hint_sections_for(
    held: Combo,
    kb: &KeybindingSettings,
    link_click_modifier: &str,
    categories_enabled: bool,
    plugin_bindings: &[PluginBindingInput],
    macos: bool,
) -> Vec<HintSection> {
    let mut sections: Vec<HintSection> = combos_containing_all(held)
        .into_iter()
        .map(|combo| HintSection {
            combo,
            rows: Vec::new(),
            roles: Vec::new(),
        })
        .collect();

    let push_row = |combo: Combo, row: HintRow, sections: &mut Vec<HintSection>| {
        if let Some(sec) = sections.iter_mut().find(|s| s.combo == combo) {
            sec.rows.push(row);
        }
    };

    for (field_id, label_key) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        let Some(bindings) = kb.get_bindings(field_id) else {
            continue;
        };
        for b in bindings {
            let Some(parsed) = parse_binding(b) else {
                continue;
            };
            let combo = parsed.combo();
            if !combo.contains_all(held) {
                continue;
            }
            push_row(
                combo,
                HintRow {
                    source: HintRowSource::Host { label_key },
                    binding: b.clone(),
                },
                &mut sections,
            );
        }
    }

    for sb in &kb.script_bindings {
        let Some(parsed) = parse_binding(&sb.combo) else {
            continue;
        };
        let combo = parsed.combo();
        if !combo.contains_all(held) {
            continue;
        }
        push_row(
            combo,
            HintRow {
                source: HintRowSource::Script {
                    script_id: sb.script_id.clone(),
                },
                binding: sb.combo.clone(),
            },
            &mut sections,
        );
    }

    for pb in plugin_bindings {
        for b in &pb.bindings {
            let Some(parsed) = parse_binding(b) else {
                continue;
            };
            let combo = parsed.combo();
            if !combo.contains_all(held) {
                continue;
            }
            push_row(
                combo,
                HintRow {
                    source: HintRowSource::Plugin {
                        plugin_id: pb.plugin_id.clone(),
                        title_i18n_key: pb.title_i18n_key.clone(),
                    },
                    binding: b.clone(),
                },
                &mut sections,
            );
        }
    }

    let tab_combo = Combo::parse_modifiers(&kb.tab_switch_modifier);
    let ws_combo = Combo::parse_modifiers(&kb.workspace_switch_modifier);
    let cat_combo = Combo::parse_modifiers(&kb.category_switch_modifier);
    // 클릭 판정(src/view/main/mouse.rs)과 같은 변환으로 실제 클릭 키의 토큰 조합을 구한다. macOS의 alt는 Option 줄이다.
    let link_combo =
        tasty_key_match::link_click_combo_for(LinkModifier::parse(link_click_modifier), macos);
    for sec in &mut sections {
        if sec.combo.shift && !sec.combo.ctrl && !sec.combo.alt && !sec.combo.option {
            sec.roles.push(HintRole::MouseCaptureBypass);
        }
        if Some(sec.combo) == tab_combo {
            sec.roles.push(HintRole::TabSwitch);
        }
        if Some(sec.combo) == ws_combo {
            sec.roles.push(HintRole::WorkspaceSwitch);
        }
        if categories_enabled && Some(sec.combo) == cat_combo {
            sec.roles.push(HintRole::CategorySwitch);
        }
        if Some(sec.combo) == link_combo {
            sec.roles.push(HintRole::LinkClick);
        }
    }

    if silent_option_hold(held, &sections, macos) {
        return Vec::new();
    }
    sections
}

/// macOS 밖에서 Win·Super(option)를 누른 동안 option 이 든 바인딩·역할이 하나도 없으면 도움말을 띄우지 않는다.
/// 타일링 WM 조작이나 Super+드래그 창 이동처럼 이 키를 오래 누르는 사용이 흔하다.
/// held 에 option 이 있으면 모든 섹션이 option 조합이므로, 모두 비었다는 것은 option 바인딩이 없다는 뜻이다.
/// macOS 의 Option 과 Alt·Ctrl·Shift 는 빈 조합도 보인다.
fn silent_option_hold(held: Combo, sections: &[HintSection], macos: bool) -> bool {
    !macos && held.option && sections.iter().all(HintSection::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(combos: &[Combo]) -> Vec<String> {
        combos.iter().map(Combo::name).collect()
    }

    fn section_names(sections: &[HintSection]) -> Vec<String> {
        sections.iter().map(|s| s.combo.name()).collect()
    }

    fn ctrl() -> Combo {
        Combo {
            ctrl: true,
            ..Default::default()
        }
    }
    fn alt() -> Combo {
        Combo {
            alt: true,
            ..Default::default()
        }
    }
    fn shift() -> Combo {
        Combo {
            shift: true,
            ..Default::default()
        }
    }

    fn kb() -> KeybindingSettings {
        KeybindingSettings::preset_tasty()
    }

    #[test]
    fn binding_leaf_strips_all_modifiers() {
        assert_eq!(binding_leaf("ctrl+k"), "k");
        assert_eq!(binding_leaf("alt+t"), "t");
        assert_eq!(binding_leaf("ctrl+shift+t"), "t");
        assert_eq!(binding_leaf("shift+ctrl+t"), "t");
        assert_eq!(binding_leaf("ctrl+,"), ",");
        assert_eq!(binding_leaf("ctrl"), "ctrl");
        assert_eq!(binding_leaf("f11"), "f11");
    }

    #[test]
    fn contains_all_is_subset_check() {
        let ctrl_shift = Combo {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert!(ctrl_shift.contains_all(ctrl()));
        assert!(ctrl_shift.contains_all(shift()));
        assert!(ctrl_shift.contains_all(ctrl_shift));
        assert!(!ctrl().contains_all(ctrl_shift));
        assert!(!ctrl().contains_all(alt()));
    }

    #[test]
    fn multi_axis_hold_narrows_to_superset_combos() {
        let ctrl_shift = Combo {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let names = names(&combos_containing_all(ctrl_shift));
        assert!(
            names
                .iter()
                .all(|n| n.contains("ctrl") && n.contains("shift"))
        );
        assert!(!names.iter().any(|n| n == "ctrl"));
        assert!(!names.iter().any(|n| n == "ctrl+alt"));
        assert_eq!(names.first().map(String::as_str), Some("ctrl+shift"));
    }

    /// option 축은 모든 OS 에 있다(macOS Option · Windows Win · Linux Super).
    #[test]
    fn combos_for_alt_are_sorted_by_size_then_priority() {
        let combos = combos_containing_all(alt());
        assert_eq!(
            names(&combos),
            [
                "alt",
                "ctrl+alt",
                "alt+option",
                "alt+shift",
                "ctrl+alt+option",
                "ctrl+alt+shift",
                "alt+option+shift",
                "ctrl+alt+option+shift",
            ]
        );
    }

    #[test]
    fn combos_for_ctrl_start_with_single_ctrl_then_size_two() {
        let combos = combos_containing_all(ctrl());
        assert_eq!(combos.first().map(Combo::name), Some("ctrl".to_string()));
        assert!(combos.iter().all(|c| c.ctrl));
    }

    #[test]
    fn option_held_alone_lists_every_option_combo() {
        let option = Combo {
            option: true,
            ..Default::default()
        };
        let combos = combos_containing_all(option);
        assert_eq!(combos.len(), 8);
        assert_eq!(combos.first().map(Combo::name), Some("option".to_string()));
    }

    #[test]
    fn restore_closed_grouped_into_ctrl_shift() {
        let sections = build_hint_sections(ctrl(), &kb(), "ctrl", false, &[]);
        let ctrl_shift = sections
            .iter()
            .find(|s| s.combo.name() == "ctrl+shift")
            .expect("ctrl+shift 섹션 존재해야 함");
        assert!(ctrl_shift.rows.iter().any(|r| {
            r.binding == "ctrl+shift+t"
                && matches!(
                    r.source,
                    HintRowSource::Host {
                        label_key: "settings.keybindings.restore_closed_label"
                    }
                )
        }));
    }

    #[test]
    fn alt_section_has_workspace_bindings_and_switch_role() {
        let sections = build_hint_sections(alt(), &kb(), "ctrl", false, &[]);
        let alt = sections
            .iter()
            .find(|s| s.combo.name() == "alt")
            .expect("alt 섹션 존재");
        assert!(alt.rows.iter().any(|r| r.binding == "alt+n"));
        assert!(alt.rows.iter().any(|r| r.binding == "alt+t"));
        assert!(alt.roles.contains(&HintRole::WorkspaceSwitch));
    }

    #[test]
    fn category_combo_section_has_category_switch_role_when_folders_on() {
        let on = build_hint_sections(ctrl(), &kb(), "ctrl", true, &[]);
        let cat = on
            .iter()
            .find(|s| s.combo.name() == "ctrl+shift")
            .expect("ctrl+shift 섹션 존재");
        assert!(cat.roles.contains(&HintRole::CategorySwitch));
        let off = build_hint_sections(ctrl(), &kb(), "ctrl", false, &[]);
        assert!(
            off.iter()
                .all(|s| !s.roles.contains(&HintRole::CategorySwitch))
        );
    }

    #[test]
    fn parse_modifiers_handles_single_and_combo_tokens() {
        assert_eq!(Combo::parse_modifiers("ctrl"), Some(ctrl()));
        assert_eq!(Combo::parse_modifiers("alt"), Some(alt()));
        assert_eq!(
            Combo::parse_modifiers("ctrl+shift"),
            Some(Combo {
                ctrl: true,
                shift: true,
                ..Default::default()
            })
        );
        assert_eq!(
            Combo::parse_modifiers("shift+ctrl"),
            Some(Combo {
                ctrl: true,
                shift: true,
                ..Default::default()
            })
        );
        assert_eq!(Combo::parse_modifiers("none"), None);
        assert_eq!(Combo::parse_modifiers(""), None);
        assert_eq!(Combo::parse_modifiers("ctrl+x"), None);
    }

    // 개별 지정 값은 수식키 조합으로 해석하지 않아야 한다.
    #[test]
    fn parse_modifiers_rejects_individual_switch_sentinel() {
        assert_eq!(
            Combo::parse_modifiers(tasty_settings::KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER),
            None
        );
    }

    #[test]
    fn ctrl_single_section_has_tab_switch_role() {
        let sections = build_hint_sections(ctrl(), &kb(), "ctrl", false, &[]);
        let ctrl = sections
            .iter()
            .find(|s| s.combo.name() == "ctrl")
            .expect("ctrl 섹션 존재");
        assert!(ctrl.roles.contains(&HintRole::TabSwitch));
        assert!(ctrl.roles.contains(&HintRole::LinkClick));
    }

    #[test]
    fn only_shift_alone_section_gets_mouse_capture_bypass() {
        let sections = build_hint_sections(shift(), &kb(), "ctrl", false, &[]);
        for sec in &sections {
            let shift_alone =
                sec.combo.shift && !sec.combo.ctrl && !sec.combo.alt && !sec.combo.option;
            if shift_alone {
                assert!(
                    sec.roles.contains(&HintRole::MouseCaptureBypass),
                    "shift 단독 섹션 {} 에 우회 역할 누락",
                    sec.combo.name()
                );
            } else {
                assert!(
                    !sec.roles.contains(&HintRole::MouseCaptureBypass),
                    "shift 단독 아닌 섹션 {} 에 우회 역할이 잘못 붙음",
                    sec.combo.name()
                );
            }
        }
    }

    fn sections_with_link_role(sections: &[HintSection]) -> Vec<String> {
        sections
            .iter()
            .filter(|s| s.roles.contains(&HintRole::LinkClick))
            .map(|s| s.combo.name())
            .collect()
    }

    /// macOS에서 링크 클릭 alt는 물리 Option이므로 안내는 Command(토큰 alt) 줄이 아니라 Option 줄에 붙는다.
    #[test]
    fn macos_alt_link_role_sits_on_the_option_row() {
        let on_option = build_hint_sections_for(option(), &kb(), "alt", false, &[], true);
        assert_eq!(sections_with_link_role(&on_option), ["option"]);
        let on_alt = build_hint_sections_for(alt(), &kb(), "alt", false, &[], true);
        assert_eq!(sections_with_link_role(&on_alt), Vec::<String>::new());
    }

    /// macOS 밖에서는 링크 클릭 alt가 토큰 alt와 같은 물리 Alt다. ctrl은 모든 OS에서 Ctrl 줄이다.
    #[test]
    fn other_os_alt_link_role_sits_on_the_alt_row() {
        let on_alt = build_hint_sections_for(alt(), &kb(), "alt", false, &[], false);
        assert_eq!(sections_with_link_role(&on_alt), ["alt"]);
        for macos in [true, false] {
            let on_ctrl = build_hint_sections_for(ctrl(), &kb(), "ctrl", false, &[], macos);
            assert_eq!(sections_with_link_role(&on_ctrl), ["ctrl"], "macos={macos}");
        }
    }

    #[test]
    fn link_none_produces_no_link_role() {
        let sections = build_hint_sections(ctrl(), &kb(), "none", false, &[]);
        assert!(
            sections
                .iter()
                .all(|s| !s.roles.contains(&HintRole::LinkClick))
        );
    }

    #[test]
    fn double_tap_and_no_modifier_bindings_excluded() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.new_tab = vec!["shift+shift".into(), "f11".into(), "ctrl".into()];
        let shift_sections = build_hint_sections(shift(), &kb, "ctrl", false, &[]);
        assert!(shift_sections.iter().all(|s| {
            s.rows
                .iter()
                .all(|r| r.binding != "shift+shift" && r.binding != "f11" && r.binding != "ctrl")
        }));
        let ctrl_sections = build_hint_sections(ctrl(), &kb, "ctrl", false, &[]);
        assert!(ctrl_sections.iter().all(|s| {
            s.rows
                .iter()
                .all(|r| r.binding != "f11" && r.binding != "ctrl")
        }));
    }

    #[test]
    fn rebound_switch_modifiers_follow_settings() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = "alt".into();
        kb.workspace_switch_modifier = "ctrl".into();

        let alt_sections = build_hint_sections(alt(), &kb, "none", false, &[]);
        let alt = alt_sections
            .iter()
            .find(|s| s.combo.name() == "alt")
            .unwrap();
        assert!(alt.roles.contains(&HintRole::TabSwitch));
        assert!(!alt.roles.contains(&HintRole::WorkspaceSwitch));

        let ctrl_sections = build_hint_sections(ctrl(), &kb, "none", false, &[]);
        let ctrl = ctrl_sections
            .iter()
            .find(|s| s.combo.name() == "ctrl")
            .unwrap();
        assert!(ctrl.roles.contains(&HintRole::WorkspaceSwitch));
        assert!(!ctrl.roles.contains(&HintRole::TabSwitch));
    }

    #[test]
    fn plugin_bindings_included_with_source() {
        let plugins = [PluginBindingInput {
            plugin_id: "git-helper".into(),
            title_i18n_key: "git_helper.stage_hunk".into(),
            bindings: vec!["ctrl+alt+g".into()],
        }];
        let sections = build_hint_sections(ctrl(), &kb(), "ctrl", false, &plugins);
        let ctrl_alt = sections
            .iter()
            .find(|s| s.combo.name() == "ctrl+alt")
            .expect("ctrl+alt 섹션 존재");
        assert!(ctrl_alt.rows.iter().any(|r| {
            r.binding == "ctrl+alt+g"
                && matches!(
                    &r.source,
                    HintRowSource::Plugin { plugin_id, title_i18n_key }
                        if plugin_id == "git-helper" && title_i18n_key == "git_helper.stage_hunk"
                )
        }));
    }

    #[test]
    fn empty_sections_are_retained() {
        let mut kb = KeybindingSettings::preset_tasty();
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            kb.clear_field(field_id);
        }
        kb.script_bindings.clear();
        kb.tab_switch_modifier = "ctrl".into();
        kb.workspace_switch_modifier = "ctrl".into();
        let sections = build_hint_sections(alt(), &kb, "ctrl", false, &[]);
        assert!(section_names(&sections).iter().any(|n| n == "alt"));
        let alt_sec = sections
            .iter()
            .find(|s| s.combo == alt())
            .expect("alt 섹션 존재");
        assert!(alt_sec.is_empty());
    }

    fn option() -> Combo {
        Combo {
            option: true,
            ..Combo::default()
        }
    }

    /// 수식키 바인딩·전환 역할을 모두 비운 설정 — 빈 섹션만 남는다.
    fn unbound_kb() -> KeybindingSettings {
        let mut kb = KeybindingSettings::preset_tasty();
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            kb.clear_field(field_id);
        }
        kb.script_bindings.clear();
        kb.tab_switch_modifier = "ctrl".into();
        kb.workspace_switch_modifier = "ctrl".into();
        kb
    }

    /// macOS 밖에서 option 바인딩이 없으면 Win·Super 를 눌러도 도움말이 없다. macOS Option 은 빈 조합도 보인다.
    #[test]
    fn unbound_option_hold_is_silent_only_off_macos() {
        let kb = unbound_kb();
        let raw: Vec<HintSection> = combos_containing_all(option())
            .into_iter()
            .map(|combo| HintSection {
                combo,
                rows: Vec::new(),
                roles: Vec::new(),
            })
            .collect();
        assert!(silent_option_hold(option(), &raw, false));
        assert!(!silent_option_hold(option(), &raw, true));
        let ctrl_option = Combo {
            ctrl: true,
            ..option()
        };
        assert!(silent_option_hold(ctrl_option, &raw, false));
        // Alt 는 비어 있어도 그대로 보인다.
        assert!(!silent_option_hold(alt(), &raw, false));

        let sections = build_hint_sections(option(), &kb, "ctrl", false, &[]);
        assert_eq!(sections.is_empty(), !cfg!(target_os = "macos"));
        assert!(!build_hint_sections(alt(), &kb, "ctrl", false, &[]).is_empty());
    }

    /// option 바인딩이 하나라도 있으면 Win·Super 도 Alt 처럼 빈 조합까지 보인다.
    #[test]
    fn bound_option_hold_lists_every_option_combo() {
        let mut kb = unbound_kb();
        kb.new_tab = vec!["option+k".to_string()];
        let sections = build_hint_sections(option(), &kb, "ctrl", false, &[]);
        assert_eq!(sections.len(), combos_containing_all(option()).len());
        assert!(
            sections
                .iter()
                .any(|s| s.combo == option() && !s.is_empty())
        );
        assert!(!silent_option_hold(option(), &sections, false));

        // 전환 역할만 option 이어도 바인딩이 있는 것으로 본다.
        let mut kb = unbound_kb();
        kb.workspace_switch_modifier = "option".into();
        let sections = build_hint_sections(option(), &kb, "ctrl", false, &[]);
        assert!(!sections.is_empty());
    }

    #[test]
    fn mixed_hold_keeps_filled_and_empty_sections() {
        let mut kb = KeybindingSettings::preset_tasty();
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            kb.clear_field(field_id);
        }
        kb.script_bindings.clear();
        kb.new_tab = vec!["ctrl+k".to_string()];
        kb.tab_switch_modifier = "shift".into();
        kb.workspace_switch_modifier = "shift".into();
        let sections = build_hint_sections(ctrl(), &kb, "none", false, &[]);
        let ctrl_sec = sections
            .iter()
            .find(|s| s.combo == ctrl())
            .expect("ctrl 섹션 존재");
        assert!(!ctrl_sec.is_empty(), "Ctrl 섹션은 비지 않아야 함");
        assert!(
            sections.iter().any(|s| s.combo != ctrl() && s.is_empty()),
            "빈 상위조합 섹션이 유지되어야 함"
        );
    }
}
