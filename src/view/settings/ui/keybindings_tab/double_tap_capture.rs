//! 녹화 중 감지한 수식키 더블탭을 칸 종류에 따라 받거나 거절한다. 받는 칸의 집합은 메인 창
//! 실행 경로와 같은 `KeybindingSettings::DOUBLE_TAP_FIELDS`다. 거절하면 값은 바꾸지 않고 녹화를
//! 이어 가며 안내를 표시한다. 이미 저장된 실행 불가 더블탭은 지우지 않고 경고로 보여 준다.

use crate::i18n::{t, t_fmt};
use crate::settings::{KeybindingSettings, Settings};

use super::{FieldKind, KeyCapture, RecordingSlot};

/// 거절한 녹화 슬롯을 기억하는 egui 임시 데이터 키.
fn rejected_id() -> egui::Id {
    egui::Id::new("settings.keybindings.double_tap_rejected")
}

/// 더블탭 감지 결과의 처리.
#[derive(Debug, PartialEq)]
pub(super) enum DoubleTapDecision {
    /// 더블탭이 없었다.
    None,
    /// 이 칸에 더블탭을 저장한다.
    Accept(String),
    /// 이 칸에서는 더블탭이 실행되지 않으므로 값을 바꾸지 않는다.
    Reject,
}

/// 녹화 슬롯이 더블탭을 받는가. 일반·범위 한정 칸은 실행 집합에 있을 때만 받는다.
/// 스크립트(`script:`)·plugin 명령·빠른 전환 슬롯은 더블탭을 실행하지 않는다.
pub(super) fn decide(
    slot: Option<&RecordingSlot>,
    double_tap: Option<String>,
) -> DoubleTapDecision {
    let (Some(slot), Some(dt)) = (slot, double_tap) else {
        return DoubleTapDecision::None;
    };
    if slot.field_kind == FieldKind::Combo && KeybindingSettings::accepts_double_tap(&slot.field_id)
    {
        DoubleTapDecision::Accept(dt)
    } else {
        DoubleTapDecision::Reject
    }
}

fn slot_key(slot: &RecordingSlot) -> String {
    format!("{:?}:{}:{}", slot.field_kind, slot.field_id, slot.idx)
}

/// 이번 프레임의 캡처 값을 정한다. 더블탭이 winit 조합보다 앞선다.
pub(super) fn take_capture(
    ui: &egui::Ui,
    recording_field: &Option<RecordingSlot>,
    captured_double_tap: &mut Option<String>,
    captured_winit_combo: &mut Option<KeyCapture>,
) -> KeyCapture {
    let Some(slot) = recording_field.as_ref() else {
        ui.data_mut(|d| d.remove::<String>(rejected_id()));
        captured_double_tap.take();
        return KeyCapture::None;
    };
    match decide(Some(slot), captured_double_tap.take()) {
        DoubleTapDecision::Accept(dt) => KeyCapture::Combo(dt),
        DoubleTapDecision::Reject => {
            ui.data_mut(|d| d.insert_temp(rejected_id(), slot_key(slot)));
            // 더블탭 끝의 수식키 누름이 같은 프레임의 조합으로 들어오지 않게 함께 버린다.
            captured_winit_combo.take();
            KeyCapture::None
        }
        DoubleTapDecision::None => captured_winit_combo.take().unwrap_or(KeyCapture::None),
    }
}

/// 거절 안내와 실행되지 않는 저장 값을 경고 문구로 그린다.
pub(super) fn draw_notices(
    ui: &mut egui::Ui,
    th: &crate::theme::Theme,
    settings: &Settings,
    recording_field: &Option<RecordingSlot>,
) {
    let rejected: Option<String> = ui.data(|d| d.get_temp(rejected_id()));
    let still_on_rejected_slot = match (recording_field, rejected) {
        (Some(slot), Some(key)) => slot_key(slot) == key,
        _ => false,
    };
    if !still_on_rejected_slot {
        ui.data_mut(|d| d.remove::<String>(rejected_id()));
    }
    let mut lines = Vec::new();
    if still_on_rejected_slot {
        lines.push(t("settings.keybindings.double_tap_rejected").to_string());
    }
    let unusable = unusable_labels(settings);
    if !unusable.is_empty() {
        lines.push(t_fmt(
            "settings.keybindings.double_tap_unusable",
            &unusable.join(", "),
        ));
    }
    for line in lines {
        tasty_ui_widgets::vspace(ui, th.spacing_xs);
        ui.label(
            egui::RichText::new(line)
                .small()
                .color(th.accent_warning_raised()),
        );
    }
}

/// 실행되지 않는 칸에 저장된 더블탭의 표시 이름(`라벨 값`). 일반·범위 한정 칸, 스크립트,
/// 번호 전환 슬롯·다음·이전을 본다. plugin 명령은 행 자체가 읽을 수 없는 키로 표시한다.
fn unusable_labels(settings: &Settings) -> Vec<String> {
    let kb = &settings.keybindings;
    let label_of = |field: &str| {
        KeybindingSettings::binding_fields()
            .find(|(id, _)| *id == field)
            .map(|(_, key)| t(key).trim_end_matches(':').to_string())
            .unwrap_or_else(|| field.to_string())
    };
    let fields = kb
        .unusable_double_taps()
        .into_iter()
        .map(|(field, value)| format!("{} {value}", label_of(field)));
    let scripts = kb
        .script_bindings
        .iter()
        .filter(|b| KeybindingSettings::is_double_tap_spelling(&b.combo))
        .map(|b| {
            let name = settings
                .scripts
                .get(&b.script_id)
                .map(|e| e.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| b.script_id.clone());
            format!("{name} {}", b.combo)
        });
    fields
        .chain(scripts)
        .chain(super::quick_switch::unusable_double_taps(kb))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::settings::ui::keybindings_tab::BareTarget;

    fn slot(field_id: &str, field_kind: FieldKind) -> RecordingSlot {
        RecordingSlot {
            field_id: field_id.to_string(),
            idx: 0,
            field_kind,
        }
    }

    fn dt() -> Option<String> {
        Some("shift+shift".to_string())
    }

    #[test]
    fn a_field_that_runs_double_taps_records_it() {
        assert_eq!(
            decide(Some(&slot("new_tab", FieldKind::Combo)), dt()),
            DoubleTapDecision::Accept("shift+shift".into())
        );
        assert_eq!(
            decide(Some(&slot("toggle_settings", FieldKind::Combo)), dt()),
            DoubleTapDecision::Accept("shift+shift".into())
        );
    }

    #[test]
    fn fields_that_do_not_run_double_taps_reject_it() {
        for s in [
            slot("toggle_command_palette", FieldKind::Combo),
            slot("code_area_apply", FieldKind::Combo),
            slot("script:abc", FieldKind::Combo),
            slot("plugin:com.example:cmd", FieldKind::Combo),
            slot("", FieldKind::BareKey(BareTarget::TabSlot(0))),
            slot("", FieldKind::IndividualSlot(BareTarget::TabSlot(0))),
        ] {
            assert_eq!(decide(Some(&s), dt()), DoubleTapDecision::Reject, "{s:?}");
        }
    }

    #[test]
    fn recording_and_execution_use_the_same_field_set() {
        for (field, _) in KeybindingSettings::binding_fields() {
            let accepted = matches!(
                decide(Some(&slot(field, FieldKind::Combo)), dt()),
                DoubleTapDecision::Accept(_)
            );
            assert_eq!(
                accepted,
                KeybindingSettings::DOUBLE_TAP_FIELDS.contains(field),
                "{field}"
            );
        }
    }

    #[test]
    fn nothing_happens_without_a_double_tap_or_a_recording() {
        assert_eq!(decide(None, dt()), DoubleTapDecision::None);
        assert_eq!(
            decide(Some(&slot("new_tab", FieldKind::Combo)), None),
            DoubleTapDecision::None
        );
    }

    #[test]
    fn stored_unusable_double_taps_are_listed_with_scripts() {
        let mut settings = Settings::default();
        settings.keybindings.toggle_command_palette = vec!["shift+shift".into()];
        settings.keybindings.new_tab = vec!["ctrl+ctrl".into()];
        settings
            .keybindings
            .script_bindings
            .push(crate::settings::ScriptBinding {
                script_id: "s1".into(),
                combo: "alt+alt".into(),
            });
        settings.keybindings.set_tab_slot_key(1, "shift+shift");
        let labels = unusable_labels(&settings);
        assert_eq!(labels.len(), 3, "{labels:?}");
        assert!(labels[0].ends_with(" shift+shift"), "{labels:?}");
        assert_eq!(labels[1], "s1 alt+alt");
        assert!(labels[2].ends_with(" shift+shift"), "{labels:?}");
    }
}
