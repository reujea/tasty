use super::{MarkPairing, pair_window_marks};

fn escape_press() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

const BARE: tasty_key_match::KeyMods = tasty_key_match::KeyMods {
    shift: false,
    ctrl: false,
    alt: false,
    command: false,
    super_: false,
};

/// 표지는 그 패스 `raw` 입력이 있을 때만 대조한다. 다시 부른 패스(빈 `raw`)는 대조하지 않고, 처리된 `events` 가
/// 비었다는 것만으로는 대조를 건너뛰지 않는다.
#[test]
fn window_escape_marks_pair_only_when_the_pass_has_raw_events() {
    // (가) 다시 부른 패스: raw·events 모두 빔.
    assert_eq!(pair_window_marks(&[BARE], &[], &[]), MarkPairing::Skipped);
    // (나) 누름 하나씩: 그 누름에 bare 표지.
    assert_eq!(
        pair_window_marks(&[BARE], &[escape_press()], &[escape_press()]),
        MarkPairing::Paired(vec![Some(BARE)])
    );
    // (다) egui 가 처리 전에 하나를 지웠다: 불일치.
    assert_eq!(
        pair_window_marks(
            &[BARE, BARE],
            &[escape_press(), escape_press()],
            &[escape_press()]
        ),
        MarkPairing::Mismatch {
            marks: 2,
            raw: 2,
            events: 1
        }
    );
    // (라) raw 에 누름이 있는데 처리된 events 가 비었다: 대조 생략이 아니라 불일치다.
    assert_eq!(
        pair_window_marks(&[BARE], &[escape_press()], &[]),
        MarkPairing::Mismatch {
            marks: 1,
            raw: 1,
            events: 0
        }
    );
}
