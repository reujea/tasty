//! 탭·워크스페이스·카테고리의 빠른 전환 키 설정.
//! 공통 수식키 모드에서는 단일 키를 저장하고 표시·실행 시 수식키와 조합한다.
//! 개별 지정 모드에서는 완성된 조합을 저장하며 공통 숫자 힌트를 표시하지 않는다.
//! 일반 동작 및 다른 전환 슬롯과의 충돌을 확인한다.
//!
//! 공통 모드에서 개별 지정으로 바꾸면 현재 조합을 저장한다. 반대 방향은 해당 종류의
//! 기본 슬롯 키로 복원한다. 공통 수식키끼리 바꾸면 저장된 단일 키는 유지한다.

use crate::i18n::{t, t_fmt};
use crate::settings::{KeybindingSettings, SwitchStep};

use super::{BareTarget, FieldKind, KeyCapture, PendingBinding, RecordingSlot, RowLayout};
use tasty_settings::keybindings::quick_switch_modifier_options;
use tasty_ui_widgets::{
    KbRecordSlot, StackRow, kb_record_slot, select, settings_stack_row, vspace,
};

/// 설정의 SwitchAxis를 화면의 녹화 대상과 연결한다.
pub(super) use tasty_settings::SwitchAxis as QuickSwitchKind;

fn slot_target(kind: QuickSwitchKind, idx: usize) -> BareTarget {
    match kind {
        QuickSwitchKind::Tab => BareTarget::TabSlot(idx),
        QuickSwitchKind::Workspace => BareTarget::WorkspaceSlot(idx),
        QuickSwitchKind::Category => BareTarget::CategorySlot(idx),
    }
}

/// 다음/이전 raw 키 타겟.
fn step_target(kind: QuickSwitchKind, step: SwitchStep) -> BareTarget {
    match (kind, step) {
        (QuickSwitchKind::Tab, SwitchStep::Next) => BareTarget::TabNext,
        (QuickSwitchKind::Tab, SwitchStep::Prev) => BareTarget::TabPrev,
        (QuickSwitchKind::Workspace, SwitchStep::Next) => BareTarget::WorkspaceNext,
        (QuickSwitchKind::Workspace, SwitchStep::Prev) => BareTarget::WorkspacePrev,
        (QuickSwitchKind::Category, SwitchStep::Next) => BareTarget::CategoryNext,
        (QuickSwitchKind::Category, SwitchStep::Prev) => BareTarget::CategoryPrev,
    }
}

/// 이 축의 슬롯 + 다음/이전 타겟 전체(순서: 슬롯 1~N → 다음 → 이전).
fn all_targets(kind: QuickSwitchKind) -> Vec<BareTarget> {
    let mut targets: Vec<BareTarget> = (0..kind.slot_count())
        .map(|i| slot_target(kind, i))
        .collect();
    targets.extend(SwitchStep::ALL.map(|s| step_target(kind, s)));
    targets
}

/// 이 축이 그리는 행의 라벨(수식키 행, 슬롯·다음·이전 행). 서브탭 라벨 열을 잴 때 쓴다.
pub(super) fn row_labels(kind: QuickSwitchKind) -> Vec<String> {
    let mut labels = vec![t(kind.modifier_label_key()).to_string()];
    labels.extend(
        all_targets(kind)
            .into_iter()
            .map(|tg| format!("{}:", bare_display_label(tg))),
    );
    labels
}

/// `target` 이 속한 quick-switch 축.
fn axis_of(target: BareTarget) -> QuickSwitchKind {
    match target {
        BareTarget::TabSlot(_) | BareTarget::TabNext | BareTarget::TabPrev => QuickSwitchKind::Tab,
        BareTarget::WorkspaceSlot(_) | BareTarget::WorkspaceNext | BareTarget::WorkspacePrev => {
            QuickSwitchKind::Workspace
        }
        BareTarget::CategorySlot(_) | BareTarget::CategoryNext | BareTarget::CategoryPrev => {
            QuickSwitchKind::Category
        }
    }
}

// ── BareTarget 데이터 접근 (accessor 경유) ────────────────────────────────

/// `target` 슬롯의 현재 raw 키를 읽는다.
fn bare_key_value(kb: &KeybindingSettings, target: BareTarget) -> String {
    match target {
        BareTarget::TabSlot(i) => kb.tab_slot_key(i).unwrap_or("").to_string(),
        BareTarget::WorkspaceSlot(i) => kb.workspace_slot_key(i).unwrap_or("").to_string(),
        BareTarget::CategorySlot(i) => kb.category_slot_key(i).unwrap_or("").to_string(),
        BareTarget::TabNext => kb.tab_next_key().to_string(),
        BareTarget::TabPrev => kb.tab_prev_key().to_string(),
        BareTarget::WorkspaceNext => kb.workspace_next_key().to_string(),
        BareTarget::WorkspacePrev => kb.workspace_prev_key().to_string(),
        BareTarget::CategoryNext => kb.category_next_key().to_string(),
        BareTarget::CategoryPrev => kb.category_prev_key().to_string(),
    }
}

/// `target` 슬롯에 raw 키를 기록한다. 충돌 팝업 accept 경로(ui.rs)에서도 사용.
pub fn set_bare_target(kb: &mut KeybindingSettings, target: BareTarget, raw_key: &str) {
    match target {
        BareTarget::TabSlot(i) => {
            kb.set_tab_slot_key(i, raw_key);
        }
        BareTarget::WorkspaceSlot(i) => {
            kb.set_workspace_slot_key(i, raw_key);
        }
        BareTarget::CategorySlot(i) => {
            kb.set_category_slot_key(i, raw_key);
        }
        BareTarget::TabNext => kb.set_tab_next_key(raw_key),
        BareTarget::TabPrev => kb.set_tab_prev_key(raw_key),
        BareTarget::WorkspaceNext => kb.set_workspace_next_key(raw_key),
        BareTarget::WorkspacePrev => kb.set_workspace_prev_key(raw_key),
        BareTarget::CategoryNext => kb.set_category_next_key(raw_key),
        BareTarget::CategoryPrev => kb.set_category_prev_key(raw_key),
    }
}

/// `target` 슬롯을 비운다(빈 문자열). 슬롯 간 충돌 accept 시 상대 슬롯 클리어에 사용.
pub fn clear_bare_target(kb: &mut KeybindingSettings, target: BareTarget) {
    set_bare_target(kb, target, "");
}

/// `target` 이 조합에 쓰는 modifier 조합(`"ctrl"`/`"alt"`/`"ctrl+shift"` …).
fn bare_modifier(kb: &KeybindingSettings, target: BareTarget) -> &str {
    match target {
        BareTarget::TabSlot(_) | BareTarget::TabNext | BareTarget::TabPrev => {
            &kb.tab_switch_modifier
        }
        BareTarget::WorkspaceSlot(_) | BareTarget::WorkspaceNext | BareTarget::WorkspacePrev => {
            &kb.workspace_switch_modifier
        }
        BareTarget::CategorySlot(_) | BareTarget::CategoryNext | BareTarget::CategoryPrev => {
            &kb.category_switch_modifier
        }
    }
}

/// modifier + raw 키를 합성한 콤보 문자열. raw 가 비면 빈 문자열(= 미설정).
fn compose(modifier: &str, raw: &str) -> String {
    if raw.is_empty() {
        String::new()
    } else {
        format!("{modifier}+{raw}")
    }
}

/// `target` 의 최종 콤보. 규칙 기반 축이면 `compose(modifier, raw)`, 개별 지정 축이면
/// 슬롯 필드에 이미 저장된 완전 콤보를 그대로 반환한다(compose 하지 않음).
fn bare_combo(kb: &KeybindingSettings, target: BareTarget) -> String {
    if axis_of(target).is_individual(kb) {
        bare_key_value(kb, target)
    } else {
        compose(bare_modifier(kb, target), &bare_key_value(kb, target))
    }
}

/// 팝업/경고에 표시할 `target` 라벨(콜론·공백 정리됨).
fn bare_display_label(target: BareTarget) -> String {
    let raw = match target {
        BareTarget::TabSlot(i) => t_fmt(
            "settings.keybindings.tab_switch_slot_label",
            &(i + 1).to_string(),
        ),
        BareTarget::WorkspaceSlot(i) => t_fmt(
            "settings.keybindings.workspace_switch_slot_label",
            &(i + 1).to_string(),
        ),
        BareTarget::CategorySlot(i) => t_fmt(
            "settings.keybindings.category_switch_slot_label",
            &(i + 1).to_string(),
        ),
        BareTarget::TabNext => t("settings.keybindings.tab_switch_next_label").to_string(),
        BareTarget::TabPrev => t("settings.keybindings.tab_switch_prev_label").to_string(),
        BareTarget::WorkspaceNext => {
            t("settings.keybindings.workspace_switch_next_label").to_string()
        }
        BareTarget::WorkspacePrev => {
            t("settings.keybindings.workspace_switch_prev_label").to_string()
        }
        BareTarget::CategoryNext => {
            t("settings.keybindings.category_switch_next_label").to_string()
        }
        BareTarget::CategoryPrev => {
            t("settings.keybindings.category_switch_prev_label").to_string()
        }
    };
    raw.trim_end_matches(':').trim().to_string()
}

/// 모든 quick-switch bare 타겟 목록(슬롯 간 중복 검사용 — 탭·워크스페이스·카테고리 교차 포함).
fn all_bare_targets() -> Vec<BareTarget> {
    QuickSwitchKind::ALL
        .into_iter()
        .flat_map(all_targets)
        .collect()
}

/// `target` 에 `candidate_combo`(이미 합성 완료된 최종 콤보 — 규칙 기반이든 개별
/// 지정이든 호출측이 [`bare_combo`] 와 동일 규칙으로 만들어 넘긴다)를 넣었을 때 겹치는
/// **다른 슬롯**을 찾는다. 탭/워크스페이스/카테고리 축이 우연히 같은 콤보를 가지면
/// 교차 충돌도 잡힌다.
fn find_slot_conflict(
    kb: &KeybindingSettings,
    target: BareTarget,
    candidate_combo: &str,
) -> Option<BareTarget> {
    if candidate_combo.is_empty() {
        return None;
    }
    all_bare_targets().into_iter().find(|&other| {
        other != target && {
            let oc = bare_combo(kb, other);
            !oc.is_empty() && oc == candidate_combo
        }
    })
}

// ── 렌더 ──────────────────────────────────────────────────────────────────

pub(super) fn draw_quick_switch_section(
    ui: &mut egui::Ui,
    keybindings: &mut KeybindingSettings,
    layout: RowLayout<'_>,
    recording_field: &mut Option<RecordingSlot>,
    pending_binding: &mut Option<PendingBinding>,
    captured: &KeyCapture,
    kind: QuickSwitchKind,
) {
    let RowLayout { general, label_col } = layout;
    // 녹화된 bare 키 소비 — 이 섹션 소속 BareKey 슬롯만.
    consume_capture(keybindings, recording_field, pending_binding, captured);

    let th = crate::theme::theme();
    // 충돌 팝업이 떠 있는 동안은 새 녹화 진입 금지.
    let can_record = pending_binding.is_none();

    // 수식키가 실제로 바뀌었을 때만 슬롯 값을 변환하거나 복원한다.
    let old_modifier = kind.modifier(keybindings).to_string();

    // 수식키 행도 아래 슬롯 행과 같은 라벨 열을 쓴다.
    let frame = StackRow {
        id: ui
            .id()
            .with(("kb_switch_modifier", kind.modifier_field_id())),
        label_col,
        row_h: th.kb_record_height(),
        label: t(kind.modifier_label_key()),
        hint: None,
        align_top: false,
    };
    settings_stack_row(ui, &th, frame, |ui| {
        let modifier = match kind {
            QuickSwitchKind::Tab => &mut keybindings.tab_switch_modifier,
            QuickSwitchKind::Workspace => &mut keybindings.workspace_switch_modifier,
            QuickSwitchKind::Category => &mut keybindings.category_switch_modifier,
        };
        // 플랫폼에서 허용하는 수식키 조합, 설정 파일에만 있는 값, 개별 지정 항목 순이다.
        let names = quick_switch_modifier_options(modifier);
        let labels: Vec<String> = names
            .iter()
            .map(|n| {
                if n == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER {
                    t("settings.keybindings.quick_switch_individual_label").to_string()
                } else {
                    KeybindingSettings::format_display(n, general)
                }
            })
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        // 현재 값은 목록에 항상 있다(목록 밖 값은 덧붙인다). 그래서 0번 옵션을 골라도 선택이 바뀐다.
        let mut selected = names.iter().position(|n| n == modifier).unwrap_or(0);
        if select(
            ui,
            &th,
            kind.modifier_field_id(),
            &mut selected,
            &labels,
            th.field_width_md.value(),
            true,
        ) && let Some(name) = names.get(selected)
        {
            modifier.clone_from(name);
        }
    });

    // modifier 가 실제로 바뀌었으면 슬롯 값을 이관(규칙→개별)하거나 복원(개별→규칙)한다.
    let new_modifier = kind.modifier(keybindings).to_string();
    if new_modifier != old_modifier {
        apply_modifier_transition(keybindings, kind, &old_modifier, &new_modifier);
    }

    super::entries::kb_row_gap(ui, &th);

    let is_individual = kind.is_individual(keybindings);

    // 슬롯 1~N.
    for i in 0..kind.slot_count() {
        slot_row(
            ui,
            keybindings,
            layout,
            recording_field,
            can_record,
            slot_target(kind, i),
            is_individual,
        );
    }
    // 다음/이전 (세 축 모두 존재 — 카테고리도 대칭).
    for tg in SwitchStep::ALL.map(|s| step_target(kind, s)) {
        slot_row(
            ui,
            keybindings,
            layout,
            recording_field,
            can_record,
            tg,
            is_individual,
        );
    }

    // modifier 변경 등으로 현재 합성 콤보가 다른 액션과 충돌하면 조용히 넘기지 않고 경고.
    let conflicts = current_conflicts(keybindings, kind);
    if !conflicts.is_empty() {
        vspace(ui, th.spacing_xs);
        ui.label(
            egui::RichText::new(t_fmt(
                "settings.keybindings.quick_switch_conflict_warning",
                &conflicts.join(", "),
            ))
            .small()
            .color(th.accent_warning_raised()),
        );
    }
}

/// 이 섹션 슬롯들의 현재 합성 콤보가 **일반 액션**과 충돌하는 목록을 라벨로 반환.
fn current_conflicts(kb: &KeybindingSettings, kind: QuickSwitchKind) -> Vec<String> {
    let mut out = Vec::new();
    for tg in all_targets(kind) {
        let combo = bare_combo(kb, tg);
        if combo.is_empty() {
            continue;
        }
        if kb.find_conflict("", &combo).is_some() {
            out.push(bare_display_label(tg));
        }
    }
    out
}

/// 저장 값이나 합성 조합이 더블탭 표기인 전환 슬롯·다음·이전(`라벨 값`). 이 칸은 더블탭을
/// 실행하지 않으므로 설정 파일·가져오기로 들어온 값을 경고로 알린다.
pub(super) fn unusable_double_taps(kb: &KeybindingSettings) -> Vec<String> {
    QuickSwitchKind::ALL
        .into_iter()
        .flat_map(all_targets)
        .filter_map(|tg| {
            let raw = bare_key_value(kb, tg);
            let combo = bare_combo(kb, tg);
            let shown = [&raw, &combo]
                .into_iter()
                .find(|v| KeybindingSettings::is_double_tap_spelling(v))?;
            Some(format!("{} {shown}", bare_display_label(tg)))
        })
        .collect()
}

/// 공통 모드에서 개별 지정으로 바꾸면 현재 조합을 저장한다.
/// 반대로 바꾸면 해당 종류의 슬롯을 기본값으로 복원한다.
/// 공통 수식키끼리 변경할 때는 슬롯의 단일 키를 유지한다.
fn apply_modifier_transition(
    kb: &mut KeybindingSettings,
    kind: QuickSwitchKind,
    old_modifier: &str,
    new_modifier: &str,
) {
    let was_individual = old_modifier == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER;
    let becomes_individual = new_modifier == KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER;
    if !was_individual && becomes_individual {
        for target in all_targets(kind) {
            let migrated = compose(old_modifier, &bare_key_value(kb, target));
            set_bare_target(kb, target, &migrated);
        }
    } else if was_individual && !becomes_individual {
        match kind {
            QuickSwitchKind::Tab => kb.reset_tab_switch_to_defaults(),
            QuickSwitchKind::Workspace => kb.reset_workspace_switch_to_defaults(),
            QuickSwitchKind::Category => kb.reset_category_switch_to_defaults(),
        }
    }
}

/// 슬롯 한 줄: 라벨 + 현재 값 버튼(클릭 시 녹화 진입). `is_individual` 이면
/// [`FieldKind::IndividualSlot`](modifier 포함 자유 콤보), 아니면 [`FieldKind::BareKey`]
/// (modifier 금지 raw 키 하나)로 녹화한다 — 이 축의 modifier 값에 따라 정해진다.
fn slot_row(
    ui: &mut egui::Ui,
    keybindings: &KeybindingSettings,
    layout: RowLayout<'_>,
    recording_field: &mut Option<RecordingSlot>,
    can_record: bool,
    target: BareTarget,
    is_individual: bool,
) {
    let RowLayout { general, label_col } = layout;
    let th = crate::theme::theme();
    let field_kind = if is_individual {
        FieldKind::IndividualSlot(target)
    } else {
        FieldKind::BareKey(target)
    };
    let is_recording = matches!(
        recording_field,
        Some(slot) if slot.field_kind == field_kind
    );

    // 서브탭 공유 폭의 라벨 열(entries.rs 와 같은 칸). 버튼이 라벨 옆에 들어가지 않으면 쌓는다.
    let label = format!("{}:", bare_display_label(target));
    let frame = StackRow {
        id: ui.id().with(("kb_switch_slot", &label)),
        label_col,
        row_h: th.kb_record_height(),
        label: &label,
        hint: None,
        align_top: true,
    };
    settings_stack_row(ui, &th, frame, |ui| {
        let combo = bare_combo(keybindings, target);
        let display = if is_recording {
            let hint_key = if is_individual {
                "settings.keybindings.hint_press_key"
            } else {
                "settings.keybindings.hint_press_bare_key"
            };
            t(hint_key).to_string()
        } else if combo.is_empty() {
            t("settings.keybindings.hint_none").to_string()
        } else {
            KeybindingSettings::format_display(&combo, general)
        };
        let slot = if is_recording {
            KbRecordSlot::Recording(&display)
        } else if combo.is_empty() {
            KbRecordSlot::Empty(&display)
        } else {
            KbRecordSlot::Binding(&display)
        };
        if kb_record_slot(ui, &th, slot, th.kb_record_width(), can_record).clicked() {
            *recording_field = Some(RecordingSlot {
                field_id: String::new(),
                idx: 0,
                field_kind,
            });
        }
    });
    super::entries::kb_row_gap(ui, &th);
}

/// 녹화한 키를 슬롯에 반영하거나 충돌 확인을 요청한다.
/// 공통 모드는 단일 키를, 개별 지정은 완성된 조합을 저장한다.
fn consume_capture(
    keybindings: &mut KeybindingSettings,
    recording_field: &mut Option<RecordingSlot>,
    pending_binding: &mut Option<PendingBinding>,
    captured: &KeyCapture,
) {
    let Some(slot) = recording_field.clone() else {
        return;
    };
    let (target, is_individual) = match slot.field_kind {
        FieldKind::BareKey(target) => (target, false),
        FieldKind::IndividualSlot(target) => (target, true),
        FieldKind::Combo => return,
    };

    match captured {
        KeyCapture::Combo(raw) => {
            // 개별 지정: `raw` 는 이미 완성된 콤보(capture_winit_key_combo 산출) — 그대로
            // 쓴다. 규칙 기반: `raw` 는 modifier 없는 키 하나 — compose 로 합성한다.
            let combo = if is_individual {
                raw.clone()
            } else {
                compose(bare_modifier(keybindings, target), raw)
            };
            if let Some((cf, ci)) = keybindings.find_conflict("", &combo) {
                // 일반 액션과 충돌.
                let label = KeybindingSettings::label_key_for(cf)
                    .map(t)
                    .unwrap_or(cf)
                    .trim_end_matches(':')
                    .trim()
                    .to_string();
                *pending_binding = Some(PendingBinding {
                    target_field: String::new(),
                    target_idx: 0,
                    combo: combo.clone(),
                    conflicting_field: cf.to_string(),
                    conflicting_idx: ci,
                    bare_target: Some(target),
                    bare_raw_key: combo,
                    conflicting_bare: None,
                    conflicting_label: Some(label),
                });
            } else if let Some(other) = find_slot_conflict(keybindings, target, &combo) {
                // 다른 quick-switch 슬롯과 중복.
                *pending_binding = Some(PendingBinding {
                    target_field: String::new(),
                    target_idx: 0,
                    combo: combo.clone(),
                    conflicting_field: String::new(),
                    conflicting_idx: 0,
                    bare_target: Some(target),
                    bare_raw_key: combo,
                    conflicting_bare: Some(other),
                    conflicting_label: Some(bare_display_label(other)),
                });
            } else {
                // `raw` 를 그대로 저장 — 개별 지정이면 `raw == combo`(이미 완전 콤보),
                // 규칙 기반이면 `raw` 는 modifier 없는 키 하나(합성은 표시 시점 몫).
                set_bare_target(keybindings, target, raw);
            }
            *recording_field = None;
        }
        KeyCapture::Clear => {
            // Escape — 슬롯 비우기.
            clear_bare_target(keybindings, target);
            *recording_field = None;
        }
        KeyCapture::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDIVIDUAL: &str = KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER;

    #[test]
    fn stored_double_taps_in_switch_slots_and_steps_are_listed() {
        let mut kb = KeybindingSettings::preset_tasty();
        assert!(unusable_double_taps(&kb).is_empty());
        kb.set_tab_slot_key(0, "shift+shift");
        kb.set_workspace_next_key("ctrl+ctrl");
        kb.category_switch_modifier = INDIVIDUAL.to_string();
        kb.set_category_prev_key("alt+alt");
        let listed = unusable_double_taps(&kb);
        assert_eq!(listed.len(), 3, "{listed:?}");
        assert!(listed[0].ends_with(" shift+shift"), "{listed:?}");
        assert!(listed[1].ends_with(" ctrl+ctrl"), "{listed:?}");
        assert!(listed[2].ends_with(" alt+alt"), "{listed:?}");
    }

    #[test]
    fn a_modifier_and_key_that_compose_a_double_tap_are_listed() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = "shift".to_string();
        kb.set_tab_prev_key("shift");
        let listed = unusable_double_taps(&kb);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert!(listed[0].ends_with(" shift+shift"), "{listed:?}");
    }

    #[test]
    fn bare_combo_rule_based_composes_modifier() {
        let kb = KeybindingSettings::preset_tasty(); // tab modifier = "ctrl"
        assert_eq!(bare_combo(&kb, BareTarget::TabSlot(0)), "ctrl+1");
    }

    #[test]
    fn bare_combo_individual_mode_returns_stored_value_as_is() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        kb.set_tab_slot_key(0, "ctrl+alt+q"); // 이미 완성된 콤보를 그대로 저장.
        // compose 를 거치지 않고 저장값을 그대로 반환해야 한다(이중 합성 금지).
        assert_eq!(bare_combo(&kb, BareTarget::TabSlot(0)), "ctrl+alt+q");
    }

    #[test]
    fn apply_modifier_transition_rule_to_individual_migrates_slots() {
        let mut kb = KeybindingSettings::preset_tasty(); // tab modifier = "ctrl", slot0 = "1"
        apply_modifier_transition(&mut kb, QuickSwitchKind::Tab, "ctrl", INDIVIDUAL);
        // 기존 수식키와 슬롯 키를 조합해 저장한다.
        assert_eq!(kb.tab_slot_key(0), Some("ctrl+1"));
        assert_eq!(kb.tab_next_key(), "ctrl+l");
        assert_eq!(kb.tab_prev_key(), "ctrl+h");
        // 이후 bare_combo 는 이 값을 그대로 반환(이중 합성 없음).
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        assert_eq!(bare_combo(&kb, BareTarget::TabSlot(0)), "ctrl+1");
    }

    #[test]
    fn apply_modifier_transition_individual_to_rule_based_resets_to_defaults() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        kb.set_tab_slot_key(0, "ctrl+alt+q"); // 개별 지정 완전 콤보(역산 불가능한 값).
        kb.set_tab_next_key("shift+f5");
        apply_modifier_transition(&mut kb, QuickSwitchKind::Tab, INDIVIDUAL, "alt");
        // 해당 종류의 기본 슬롯 키로 복원한다.
        assert_eq!(
            kb.tab_switch_slot_keys,
            ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
        );
        assert_eq!(kb.tab_next_key(), "l");
        assert_eq!(kb.tab_prev_key(), "h");
    }

    #[test]
    fn apply_modifier_transition_rule_to_rule_leaves_slots_unchanged() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.set_tab_slot_key(0, "q");
        apply_modifier_transition(&mut kb, QuickSwitchKind::Tab, "ctrl", "alt");
        // raw 값은 그대로 — 표시만 새 modifier 로 재합성(호출측 bare_combo 책임).
        assert_eq!(kb.tab_slot_key(0), Some("q"));
    }

    #[test]
    fn apply_modifier_transition_only_touches_target_axis() {
        let mut kb = KeybindingSettings::preset_tasty();
        apply_modifier_transition(&mut kb, QuickSwitchKind::Tab, "ctrl", INDIVIDUAL);
        // 워크스페이스/카테고리 축은 여전히 규칙 기반 raw 값 그대로.
        assert_eq!(kb.workspace_slot_key(0), Some("1"));
        assert_eq!(kb.category_slot_key(0), Some("1"));
    }

    #[test]
    fn find_slot_conflict_detects_cross_axis_duplicate_in_individual_mode() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        kb.set_tab_slot_key(1, "ctrl+alt+q");
        // 워크스페이스도 개별 지정으로 바꿔 종류가 다른 슬롯 간 충돌을 확인한다.
        kb.workspace_switch_modifier = INDIVIDUAL.to_string();
        kb.set_workspace_slot_key(0, "ctrl+alt+q");
        let conflict = find_slot_conflict(&kb, BareTarget::TabSlot(1), "ctrl+alt+q");
        assert_eq!(conflict, Some(BareTarget::WorkspaceSlot(0)));
    }

    #[test]
    fn find_slot_conflict_empty_combo_is_never_a_conflict() {
        let kb = KeybindingSettings::preset_tasty();
        assert_eq!(find_slot_conflict(&kb, BareTarget::TabSlot(0), ""), None);
    }

    #[test]
    fn consume_capture_individual_slot_stores_full_combo_verbatim() {
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        let mut recording = Some(RecordingSlot {
            field_id: String::new(),
            idx: 0,
            field_kind: FieldKind::IndividualSlot(BareTarget::TabSlot(0)),
        });
        let mut pending = None;
        let captured = KeyCapture::Combo("ctrl+alt+q".to_string());
        consume_capture(&mut kb, &mut recording, &mut pending, &captured);
        assert_eq!(kb.tab_slot_key(0), Some("ctrl+alt+q"));
        assert!(recording.is_none());
        assert!(pending.is_none());
    }

    #[test]
    fn consume_capture_individual_slot_conflicts_with_general_action() {
        // preset_tasty 의 restore_closed = "ctrl+shift+t" — 개별 지정 슬롯에 같은 콤보를
        // 녹화하면 일반 액션 충돌 팝업(PendingBinding)이 뜨고 즉시 저장되지 않아야 한다.
        let mut kb = KeybindingSettings::preset_tasty();
        kb.tab_switch_modifier = INDIVIDUAL.to_string();
        let mut recording = Some(RecordingSlot {
            field_id: String::new(),
            idx: 0,
            field_kind: FieldKind::IndividualSlot(BareTarget::TabSlot(0)),
        });
        let mut pending = None;
        let captured = KeyCapture::Combo("ctrl+shift+t".to_string());
        consume_capture(&mut kb, &mut recording, &mut pending, &captured);
        // 충돌 발견 → 슬롯엔 아직 반영 안 됨, pending 팝업 상태로 전환.
        assert_eq!(kb.tab_slot_key(0), Some("1")); // preset 기본값 그대로.
        assert!(recording.is_none()); // 녹화는 종료(팝업으로 전환).
        let pending = pending.expect("일반 액션 충돌 시 PendingBinding 이 채워져야 함");
        assert_eq!(pending.conflicting_field, "restore_closed");
        assert_eq!(pending.bare_target, Some(BareTarget::TabSlot(0)));
        assert_eq!(pending.bare_raw_key, "ctrl+shift+t");
    }

    #[test]
    fn consume_capture_bare_slot_still_composes_via_modifier() {
        // 회귀 방지: IndividualSlot 분기 추가가 기존 BareKey 경로를 깨지 않아야 한다.
        let mut kb = KeybindingSettings::preset_tasty(); // tab modifier = "ctrl"
        let mut recording = Some(RecordingSlot {
            field_id: String::new(),
            idx: 0,
            field_kind: FieldKind::BareKey(BareTarget::TabSlot(0)),
        });
        let mut pending = None;
        let captured = KeyCapture::Combo("q".to_string());
        consume_capture(&mut kb, &mut recording, &mut pending, &captured);
        // 저장은 raw 그대로("q"), 합성은 표시 시점(bare_combo)의 몫.
        assert_eq!(kb.tab_slot_key(0), Some("q"));
        assert_eq!(bare_combo(&kb, BareTarget::TabSlot(0)), "ctrl+q");
    }

    #[test]
    fn axis_of_maps_every_variant_correctly() {
        assert_eq!(axis_of(BareTarget::TabSlot(0)), QuickSwitchKind::Tab);
        assert_eq!(axis_of(BareTarget::TabNext), QuickSwitchKind::Tab);
        assert_eq!(axis_of(BareTarget::TabPrev), QuickSwitchKind::Tab);
        assert_eq!(
            axis_of(BareTarget::WorkspaceSlot(0)),
            QuickSwitchKind::Workspace
        );
        assert_eq!(
            axis_of(BareTarget::WorkspaceNext),
            QuickSwitchKind::Workspace
        );
        assert_eq!(
            axis_of(BareTarget::WorkspacePrev),
            QuickSwitchKind::Workspace
        );
        assert_eq!(
            axis_of(BareTarget::CategorySlot(0)),
            QuickSwitchKind::Category
        );
        assert_eq!(axis_of(BareTarget::CategoryNext), QuickSwitchKind::Category);
        assert_eq!(axis_of(BareTarget::CategoryPrev), QuickSwitchKind::Category);
    }

    mod select_row {
        use super::*;
        use crate::settings::GeneralSettings;
        use crate::view::settings::ui::select_probe as probe;

        fn draw<'a>(
            kb: &'a mut KeybindingSettings,
            with_drag_flip: bool,
        ) -> impl FnMut(&mut egui::Ui) + 'a {
            move |ui| {
                let th = crate::theme::theme();
                let general = GeneralSettings::default();
                let layout = RowLayout {
                    general: &general,
                    label_col: th.settings_label_width(),
                };
                draw_quick_switch_section(
                    ui,
                    kb,
                    layout,
                    &mut None,
                    &mut None,
                    &KeyCapture::None,
                    QuickSwitchKind::Tab,
                );
                if with_drag_flip {
                    super::super::super::drag_flip::draw(ui, &th, kb, &general, layout.label_col);
                }
            }
        }

        fn shown(name: &str) -> String {
            KeybindingSettings::format_display(name, &GeneralSettings::default())
        }

        /// 수식키 트리거는 드래그 반전 행과 같은 Select 상자(높이 select-height, 폭 field-width-md)다.
        #[test]
        fn the_modifier_trigger_matches_the_drag_flip_select() {
            let th = crate::theme::theme();
            let ctx = egui::Context::default();
            let mut kb = KeybindingSettings::preset_tasty();
            kb.tab_switch_modifier = "alt".into();
            kb.explorer_drag_flip_modifier = "shift".into();
            probe::frame(&ctx, &mut draw(&mut kb, true));
            let out = probe::frame(&ctx, &mut draw(&mut kb, true));
            let modifier = probe::trigger_around(&out, &shown("alt"))
                .expect("the switch modifier is drawn as a Select trigger");
            let flip = probe::trigger_around(&out, &shown("shift"))
                .expect("the drag flip modifier is drawn as a Select trigger");
            assert_eq!(modifier.height(), th.select_height().value());
            assert_eq!(modifier.width(), th.field_width_md.value());
            assert_eq!(modifier.size(), flip.size());
        }

        /// 수식키 행에는 캡션이 없다 — 트리거 아래 첫 글자는 첫 슬롯 행의 라벨이다.
        #[test]
        fn the_modifier_row_has_no_caption() {
            let ctx = egui::Context::default();
            let mut kb = KeybindingSettings::preset_tasty();
            probe::frame(&ctx, &mut draw(&mut kb, false));
            let out = probe::frame(&ctx, &mut draw(&mut kb, false));
            let trigger = probe::trigger_around(&out, &shown("ctrl")).expect("trigger");
            let first_slot = format!("{}:", bare_display_label(BareTarget::TabSlot(0)));
            let slot_label = probe::text_rects(&out, &first_slot)[0];
            let between: Vec<_> =
                crate::view::plugins::ui::text_probe::visible_text_rects(&out, probe::SCREEN)
                    .into_iter()
                    .filter(|(_, r)| r.top() >= trigger.bottom() && r.bottom() <= slot_label.top())
                    .collect();
            assert!(between.is_empty(), "text between the rows: {between:?}");
        }

        /// 설정 파일의 목록 밖 조합은 트리거에 그대로 보이고 그리기만으로 바뀌지 않는다. 첫 옵션을
        /// 고르면 그 값이 되고(규칙 사이 전환이라 슬롯 키는 그대로), 첫 옵션 함정에 걸리지 않는다.
        #[test]
        fn an_unlisted_combo_is_kept_and_the_first_option_replaces_it() {
            let ctx = egui::Context::default();
            let mut kb = KeybindingSettings::preset_tasty();
            kb.tab_switch_modifier = "shift+alt".into();
            kb.set_tab_slot_key(0, "q");
            let first = quick_switch_modifier_options("")[0].clone();
            probe::frame(&ctx, &mut draw(&mut kb, false));
            let out = probe::frame(&ctx, &mut draw(&mut kb, false));
            assert!(probe::trigger_around(&out, &shown("shift+alt")).is_some());
            assert_eq!(kb.tab_switch_modifier, "shift+alt");
            probe::pick(
                &ctx,
                &mut draw(&mut kb, false),
                &shown("shift+alt"),
                &shown(&first),
            );
            assert_eq!(kb.tab_switch_modifier, first);
            assert_eq!(kb.tab_slot_key(0), Some("q"));
        }

        /// 개별 지정으로 고르면 슬롯이 조합으로 이관되고, 규칙으로 돌아오면 기본 슬롯으로 복원된다.
        #[test]
        fn picking_individual_migrates_slots_and_picking_a_rule_restores_them() {
            let ctx = egui::Context::default();
            let mut kb = KeybindingSettings::preset_tasty(); // tab modifier = "ctrl", slot0 = "1"
            let individual = t("settings.keybindings.quick_switch_individual_label");
            probe::pick(&ctx, &mut draw(&mut kb, false), &shown("ctrl"), individual);
            assert_eq!(kb.tab_switch_modifier, INDIVIDUAL);
            assert_eq!(kb.tab_slot_key(0), Some("ctrl+1"));
            kb.set_tab_slot_key(0, "ctrl+alt+q");
            probe::pick(&ctx, &mut draw(&mut kb, false), individual, &shown("alt"));
            assert_eq!(kb.tab_switch_modifier, "alt");
            assert_eq!(kb.tab_slot_key(0), Some("1"));
        }

        /// 같은 값을 다시 고르면 슬롯을 건드리지 않는다.
        #[test]
        fn picking_the_current_value_leaves_slots_alone() {
            let ctx = egui::Context::default();
            let mut kb = KeybindingSettings::preset_tasty();
            kb.tab_switch_modifier = INDIVIDUAL.into();
            kb.set_tab_slot_key(0, "ctrl+alt+q");
            let individual = t("settings.keybindings.quick_switch_individual_label");
            probe::pick(&ctx, &mut draw(&mut kb, false), individual, individual);
            assert_eq!(kb.tab_switch_modifier, INDIVIDUAL);
            assert_eq!(kb.tab_slot_key(0), Some("ctrl+alt+q"));
        }
    }
}
