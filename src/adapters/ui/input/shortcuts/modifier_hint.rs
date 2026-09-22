//! Modifier-hint 오버레이 **조합 콘텐츠 모델** — 순수 로직, UI 비의존.
//!
//! "누른 modifier `M` → 정렬된 `(조합, 항목 목록)` 리스트" 를 만드는 읽기 전용
//! 모델이다. 렌더링/디자인(modifier-hint-03) 과 분리돼 있으며 기존 동작을 바꾸지
//! 않는다 — 오직 표시할 데이터를 계산한다.
//!
//! ## 조합 공간·정렬
//! - 축: `ctrl` / `alt` / `option` / `shift`. **`option` 은 macOS 전용**(비-macOS 컴파일에선
//!   축 자체가 빠진다). `"alt"` 토큰은 macOS 에서 물리 Command(⌘), 그 외에서 Alt 에
//!   매핑되는데(위치 기반 추상화), 이는 [`super::binding`] 의 파싱 규칙과 동일하므로
//!   여기서는 파서가 채워준 `ParsedBinding` 축을 그대로 쓴다.
//! - 정렬: ① 조합 크기 오름차순 → ② 같은 크기 내 우선순위 `Ctrl < Alt(Cmd) < Option < Shift`.
//!   예) Alt 홀드(macOS) → `alt, ctrl+alt, alt+option, alt+shift, ctrl+alt+option,
//!   ctrl+alt+shift, alt+option+shift, ctrl+alt+option+shift`.
//!
//! ## 열거 소스
//! - `KeybindingSettings` 고정 필드 전체([`KeybindingSettings::GENERAL_BINDING_FIELDS`] 재사용)
//!   + `script_bindings`.
//! - Plugin command 의 `EffectiveBinding`([`PluginBindingInput`] 로 주입) — 디자인 확정상 **전량
//!   포함**한다. focus 스코핑(전체 등록 plugin vs focused RemoteSurface 만)은 modifier-hint-03
//!   wiring 의 결정사항이라 이 모델은 받은 것을 모두 노출한다 (open).
//! - 더블탭(`shift+shift` 등)·무 modifier(`f11`)·모디파이어 단독(`ctrl`) 바인딩은
//!   [`parse_binding`] 이 `None` 을 돌려주어 자연히 제외된다.
//!
//! ## 특수 역할 (단축키 목록 외 설명 행) — **설정 현재값 기준**(기본값 가정 금지)
//! - Shift **단독** 조합: TUI 마우스 캡처 임시 우회(Shift+드래그=로컬 선택 등).
//!   실 동작은 `shift_key()` 만 검사해 Ctrl+Shift 등에도 우회가 걸리지만, 안내 행은
//!   Shift 단독 섹션에만 붙여 조합마다 중복 표시되지 않게 한다.
//! - `tab_switch_modifier` 조합: 탭 전환 + 숫자 오버레이.
//! - `workspace_switch_modifier` 조합: 워크스페이스 전환 + 숫자 오버레이.
//! - `category_switch_modifier` 조합(기본 `ctrl+shift`): 카테고리 전환 + 헤더 숫자 오버레이(folders on).
//! - `link_click_modifier`(`general`) 단독 조합: modifier+클릭 링크 열기. `"none"` 이면 역할 없음.
//!
//! 빈 조합(바인딩·역할 모두 없음)도 섹션을 **유지**한다 — 오버레이가 ChordHead 아래에
//! "바인딩 없음" 플레이스홀더 한 줄을 그린다(2026-07-06 결정, ADR-0038). 이전엔 빈 섹션을
//! 생략했으나, 미할당 조합을 홀드하면 패널이 아예 안 떠 "반응 없음"으로 읽히는 문제로 반전.
//!
//! NOTE: 오버레이(`super::super::modifier_hint_overlay`)가 이 모델을 소비한다. plugin 단축키는
//! 아직 배선되지 않았다 — `PluginManager` 가 `App` 소유라 draw 경로에 미도달이고, 오버레이는
//! `plugin_bindings` 에 빈 목록을 넘긴다. 배선할 때 registry entry 를 [`PluginBindingInput`] 으로
//! 바꾸는 해석(`crate::plugin::command_registry::effective_binding`)을 그 자리에 둔다.

use tasty_settings::KeybindingSettings;

use tasty_key_match::parse_binding;

// 조합 타입과 열거는 `KeybindingSettings` 가 저장하는 축 modifier 값의 해석 규칙이라
// 그 값을 소유한 크레이트에 있다(`docs/adr/0256-the-binding-parser-lives-with-the-setting-it-parses.md`).
// 이 모듈은 그것으로 hint 섹션을 조립한다.
pub use tasty_settings::keybindings::parse::{Combo, all_modifier_combos, combos_containing_all};

/// 행 바인딩에서 leaf 키 토큰만 반환 — 섹션 헤더가 이미 modifier 를 보여주므로 중복 제거.
/// canonical full binding 은 [`HintRow::binding`] 에 유지하고 **표시만** leaf 로 도출한다.
///
/// 섹션 combo == 행 modifier 집합이 항상 성립하므로([`build_hint_sections`] 의 push 규칙:
/// 행은 자신의 조합과 정확히 같은 섹션에만 들어간다), "modifier 전부 제거" 는 "섹션 prefix
/// strip" 과 증명적으로 동치다. 문자열 strip 대신 파서를 쓰는 이유는 표기 순서에 견고하기
/// 때문 — `"shift+ctrl+t"` 도(헤더가 `Ctrl+Shift` 라도) leaf 는 항상 `"t"`.
///
/// parse 실패(비정상 경로 — 섹션에 존재하는 행은 구조적으로 여기 오지 않는다) 시 원본
/// 바인딩을 그대로 fallback 하여 빈 렌더를 원천 차단한다.
pub fn binding_leaf(binding: &str) -> &str {
    parse_binding(binding).map(|p| p.key).unwrap_or(binding)
}

/// 조합 섹션 안의 한 항목(바인딩된 액션/스크립트/plugin command).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintRow {
    pub source: HintRowSource,
    /// 이 행의 원본 바인딩 문자열(예: `"ctrl+shift+t"`). 오버레이(`modifier_hint_overlay`)가 키캡으로 분해해 그린다.
    pub binding: String,
}

/// 항목의 출처 — 라벨 해석 방식을 결정한다(모델은 키만 반환, 문자열 해석은 오버레이).
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

/// 조합에 붙는 특수 역할(바인딩이 아닌 설명 행).
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
    /// 역할 설명 i18n 키. 오버레이가 `t()` 로 해석한다.
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

/// 한 조합 섹션 — 조합 + 바인딩 항목들 + 특수 역할 설명들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintSection {
    pub combo: Combo,
    /// 이 조합에 매핑된 바인딩 항목(필드 순서 → 스크립트 → plugin 순).
    pub rows: Vec<HintRow>,
    /// 이 조합에 해당하는 특수 역할.
    pub roles: Vec<HintRole>,
}

impl HintSection {
    /// 바인딩·역할이 모두 없는 조합인가 — 오버레이가 이때 "바인딩 없음" 플레이스홀더를
    /// 그린다(빈 섹션은 더 이상 생략되지 않는다, ADR-0038).
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.roles.is_empty()
    }
}

/// Plugin command 하나의 표시용 입력(effective 바인딩 해석 결과).
///
/// `crate::plugin::command_registry::EffectiveBinding` 을 재사용해 실제 매칭 키로 환원한 값을 담는다. registry 순회·
/// override 소스·focus 스코핑은 오버레이 wiring 이 담당하고, 이 모델은 완성된 입력을 받는다
/// (순수 함수 테스트 가능성 유지).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginBindingInput {
    pub plugin_id: String,
    pub title_i18n_key: String,
    /// 실제 매칭에 쓰는 키 목록(effective). 빈 벡터면 미할당.
    pub bindings: Vec<String>,
}

/// 홀드 조합 `held` 에 대한 정렬된 조합 콘텐츠를 만든다.
///
/// - `held`: 사용자가 누르고 있는 modifier **조합**(4축). 이 조합을 포함하는 조합만 노출된다.
/// - `kb`: 고정 필드 + `script_bindings` + tab/workspace switch modifier 소스.
/// - `link_click_modifier`: `general.link_click_modifier`(`"ctrl"`|`"alt"`|`"none"`).
/// - `plugin_bindings`: 표시할 plugin command 입력(전량).
///
/// 반환은 정렬된 섹션 목록이며 **빈 섹션(바인딩·역할 모두 없음)은 생략**된다.
pub fn build_hint_sections(
    held: Combo,
    kb: &KeybindingSettings,
    link_click_modifier: &str,
    categories_enabled: bool,
    plugin_bindings: &[PluginBindingInput],
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

    // 1. 고정 호스트 액션 필드 (GENERAL_BINDING_FIELDS 를 SoT 로 재사용).
    //    toggle_sidebar/collapse 는 라벨 키가 없어 이 목록에서 제외돼 있으므로 자연히 빠진다.
    for (field_id, label_key) in KeybindingSettings::GENERAL_BINDING_FIELDS {
        let Some(bindings) = kb.get_bindings(field_id) else {
            continue;
        };
        for b in bindings {
            let Some(parsed) = parse_binding(b) else {
                continue; // 더블탭·무 modifier·modifier 단독 → 제외
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

    // 2. 사용자 스크립트 동적 바인딩.
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

    // 3. Plugin command (전량 노출 — focus 스코핑은 오버레이 wiring 의 결정으로 남은 미결).
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

    // 4. 특수 역할 주입 (설정 현재값 기준). 세 축 각각의 독립 조합을 파싱해 매칭한다
    //    (과거 `Combo{shift:true,..ws}` 카테고리 파생 하드코딩 제거 — 카테고리도 1급 축).
    let tab_combo = Combo::parse_modifiers(&kb.tab_switch_modifier);
    let ws_combo = Combo::parse_modifiers(&kb.workspace_switch_modifier);
    let cat_combo = Combo::parse_modifiers(&kb.category_switch_modifier);
    let link_combo = Combo::parse_modifiers(link_click_modifier); // "none" → None
    for sec in &mut sections {
        // Shift **단독** 조합에만 → 마우스 캡처 우회 안내.
        // (실 동작은 `shift_key()` 만 검사해 Ctrl+Shift 등에도 우회가 걸리지만, 안내 행은
        //  Shift 단독 섹션에만 붙여 조합마다 중복 표시되지 않게 한다.)
        if sec.combo.shift && !sec.combo.ctrl && !sec.combo.alt && !sec.combo.option {
            sec.roles.push(HintRole::MouseCaptureBypass);
        }
        if Some(sec.combo) == tab_combo {
            sec.roles.push(HintRole::TabSwitch);
        }
        if Some(sec.combo) == ws_combo {
            sec.roles.push(HintRole::WorkspaceSwitch);
        }
        // 카테고리 축 조합(folders 기능 on 전제, 디자인 E).
        if categories_enabled && Some(sec.combo) == cat_combo {
            sec.roles.push(HintRole::CategorySwitch);
        }
        if Some(sec.combo) == link_combo {
            sec.roles.push(HintRole::LinkClick);
        }
    }

    // 5. 빈 섹션도 유지한다 — 오버레이(modifier-hint-03)가 빈 섹션에 "바인딩 없음"
    //    플레이스홀더를 그려 "이 조합은 정말 미할당" 임을 명시한다(2026-07-06 결정,
    //    ADR-0038). 이전(2026-07-02)엔 여기서 `retain` 으로 빈 섹션을 생략했다.
    sections
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

    /// 단일 축 홀드 조합 헬퍼(가독성).
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

    /// 테스트용 기본 키바인딩(preset_tasty): new_workspace="alt+n", new_tab="alt+t",
    /// restore_closed="ctrl+shift+t", tab_switch="ctrl", workspace_switch="alt".
    fn kb() -> KeybindingSettings {
        KeybindingSettings::preset_tasty()
    }

    #[test]
    fn binding_leaf_strips_all_modifiers() {
        // 단일 modifier → leaf 키.
        assert_eq!(binding_leaf("ctrl+k"), "k");
        assert_eq!(binding_leaf("alt+t"), "t");
        // 다축 조합 → leaf 키.
        assert_eq!(binding_leaf("ctrl+shift+t"), "t");
        // 표기 순서 무관(파서 사용) — 헤더가 Ctrl+Shift 라도 leaf 는 항상 t.
        assert_eq!(binding_leaf("shift+ctrl+t"), "t");
        // 구분자와 충돌하는 키(`,`)도 leaf 로 보존.
        assert_eq!(binding_leaf("ctrl+,"), ",");
        // parse 실패(modifier 단독/무 modifier) → 원본 fallback.
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
        // 상위 조합은 하위 축 셋을 포함.
        assert!(ctrl_shift.contains_all(ctrl()));
        assert!(ctrl_shift.contains_all(shift()));
        assert!(ctrl_shift.contains_all(ctrl_shift));
        // 하위(단일)는 상위(다축)를 포함하지 않음.
        assert!(!ctrl().contains_all(ctrl_shift));
        // 겹치지 않는 축은 포함 안 함.
        assert!(!ctrl().contains_all(alt()));
    }

    #[test]
    fn multi_axis_hold_narrows_to_superset_combos() {
        // Ctrl+Shift 홀드 → ctrl+shift 를 포함하는 조합만(ctrl 단독·ctrl+alt 는 제외).
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
        // 첫 원소는 홀드 조합 자신(가장 작은 크기).
        assert_eq!(names.first().map(String::as_str), Some("ctrl+shift"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn combos_for_alt_are_sorted_by_size_then_priority_macos() {
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

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn combos_for_alt_are_sorted_by_size_then_priority_non_macos() {
        // 비-macOS: option 축 없음 → 4개.
        let combos = combos_containing_all(alt());
        assert_eq!(
            names(&combos),
            ["alt", "ctrl+alt", "alt+shift", "ctrl+alt+shift"]
        );
    }

    #[test]
    fn combos_for_ctrl_start_with_single_ctrl_then_size_two() {
        let combos = combos_containing_all(ctrl());
        // 크기 1 이 먼저, 그 다음 크기 2 (ctrl+alt 가 ctrl 단독보다 뒤).
        assert_eq!(combos.first().map(Combo::name), Some("ctrl".to_string()));
        assert!(combos.iter().all(|c| c.ctrl));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_never_generates_option_combos() {
        for held in [ctrl(), alt(), shift()] {
            for c in combos_containing_all(held) {
                assert!(!c.option, "option 축이 비-macOS 에서 생성됨: {}", c.name());
            }
        }
        // Option 홀드 자체도 비-macOS 에선 아무 조합도 없어야 한다.
        let option = Combo {
            option: true,
            ..Default::default()
        };
        assert!(combos_containing_all(option).is_empty());
    }

    #[test]
    fn restore_closed_grouped_into_ctrl_shift() {
        // 기본 restore_closed = "ctrl+shift+t" → Ctrl 홀드 시 ctrl+shift 섹션에 분류.
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
        // Alt 홀드 → "alt" 단독 섹션에 new_workspace/new_tab + WorkspaceSwitch 역할.
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
        // 기본 카테고리 modifier = ctrl+shift → Ctrl 홀드 + folders on 시 ctrl+shift 섹션에
        // CategorySwitch 역할(과거 workspace축+Shift 파생 제거, 독립 축 매칭).
        let on = build_hint_sections(ctrl(), &kb(), "ctrl", true, &[]);
        let cat = on
            .iter()
            .find(|s| s.combo.name() == "ctrl+shift")
            .expect("ctrl+shift 섹션 존재");
        assert!(cat.roles.contains(&HintRole::CategorySwitch));
        // folders off → 역할 없음.
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
        // 표기 순서 무관.
        assert_eq!(
            Combo::parse_modifiers("shift+ctrl"),
            Some(Combo {
                ctrl: true,
                shift: true,
                ..Default::default()
            })
        );
        // 알 수 없는 토큰·"none"·빈 문자열 → None.
        assert_eq!(Combo::parse_modifiers("none"), None);
        assert_eq!(Combo::parse_modifiers(""), None);
        assert_eq!(Combo::parse_modifiers("ctrl+x"), None);
    }

    /// S-9: "개별 지정" sentinel(`INDIVIDUAL_SWITCH_MODIFIER`)은 4축 토큰(`ctrl`/`shift`/
    /// `alt`/`option`) 어느 것과도 안 맞아 파싱 실패한다 — 파서 수정 없이 sentinel 을
    /// 도입할 수 있다는 설계 전제를 회귀로 고정한다.
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
        // link_click_modifier="ctrl" 이므로 LinkClick 역할도 같은 섹션에.
        assert!(ctrl.roles.contains(&HintRole::LinkClick));
    }

    #[test]
    fn only_shift_alone_section_gets_mouse_capture_bypass() {
        // Shift 홀드 → Shift 단독 섹션에만 우회 역할, Ctrl+Shift 등 다축 섹션엔 없음.
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
        // 더블탭·무 modifier·modifier 단독 → 어느 섹션에도 안 들어가야 한다.
        kb.new_tab = vec!["shift+shift".into(), "f11".into(), "ctrl".into()];
        // Shift 홀드 섹션에 shift+shift 가 새지 않는지 확인.
        let shift_sections = build_hint_sections(shift(), &kb, "ctrl", false, &[]);
        assert!(shift_sections.iter().all(|s| {
            s.rows
                .iter()
                .all(|r| r.binding != "shift+shift" && r.binding != "f11" && r.binding != "ctrl")
        }));
        // Ctrl 홀드에서도 "ctrl" 단독/"f11" 이 새지 않음.
        let ctrl_sections = build_hint_sections(ctrl(), &kb, "ctrl", false, &[]);
        assert!(ctrl_sections.iter().all(|s| {
            s.rows
                .iter()
                .all(|r| r.binding != "f11" && r.binding != "ctrl")
        }));
    }

    #[test]
    fn rebound_switch_modifiers_follow_settings() {
        // tab=alt / ws=ctrl 로 재바인딩 → 역할도 반대 섹션으로.
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
        // ADR-0038: 바인딩·역할이 하나도 안 걸리는 조합도 섹션이 유지된다(오버레이가
        // 플레이스홀더를 그린다). 이전엔 여기서 생략됐다.
        let mut kb = KeybindingSettings::preset_tasty();
        // 모든 고정 필드를 비워 역할만 남긴다.
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            kb.clear_field(field_id);
        }
        kb.script_bindings.clear();
        // Alt 홀드, switch/link 모두 alt 아닌 값 → alt 단독 섹션에 아무것도 안 붙는다.
        kb.tab_switch_modifier = "ctrl".into();
        kb.workspace_switch_modifier = "ctrl".into();
        let sections = build_hint_sections(alt(), &kb, "ctrl", false, &[]);
        // alt 단독 섹션은 이제 유지되어야 한다(생략 안 함).
        assert!(section_names(&sections).iter().any(|n| n == "alt"));
        // 그 alt 섹션은 바인딩·역할이 없어 빈 섹션(is_empty)이어야 한다.
        let alt_sec = sections
            .iter()
            .find(|s| s.combo == alt())
            .expect("alt 섹션 존재");
        assert!(alt_sec.is_empty());
    }

    #[test]
    fn mixed_hold_keeps_filled_and_empty_sections() {
        // ADR-0038: Ctrl 홀드 시 채워진 섹션과 빈(플레이스홀더) 섹션이 한 리스트에 공존.
        let mut kb = KeybindingSettings::preset_tasty();
        for (field_id, _) in KeybindingSettings::GENERAL_BINDING_FIELDS {
            kb.clear_field(field_id);
        }
        kb.script_bindings.clear();
        // Ctrl 단독에만 바인딩 하나 남긴다. switch modifier 는 Ctrl 계열 아님(역할 배제).
        kb.new_tab = vec!["ctrl+k".to_string()];
        kb.tab_switch_modifier = "shift".into();
        kb.workspace_switch_modifier = "shift".into();
        let sections = build_hint_sections(ctrl(), &kb, "none", false, &[]);
        // Ctrl 섹션은 바인딩이 있어 채워짐.
        let ctrl_sec = sections
            .iter()
            .find(|s| s.combo == ctrl())
            .expect("ctrl 섹션 존재");
        assert!(!ctrl_sec.is_empty(), "Ctrl 섹션은 비지 않아야 함");
        // 상위집합(Ctrl+Alt 등) 중 최소 하나는 빈 섹션으로 유지된다.
        assert!(
            sections.iter().any(|s| s.combo != ctrl() && s.is_empty()),
            "빈 상위조합 섹션이 유지되어야 함"
        );
    }
}
