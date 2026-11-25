//! vi 복사 모드의 키 판정(`vi_key_step`).

use super::*;
use crate::adapters::ui::popup::PopupInstanceId;

const VI: u32 = 7;
const OTHER: u32 = 8;

/// Given vi 복사 모드인 터미널, When 키가 그 터미널에서 끝나면(경로 키의 수신자가 그 surface 이거나 단축키
/// 단계의 끝이 그 surface, 지금 경로 키는 오버레이 없이 그 터미널이 포커스), Then vi 복사 모드가 처리한다.
#[test]
fn vi_copy_mode_takes_keys_on_its_terminal() {
    for receiver in [
        Some(KeyReceiver::Surface(VI)),
        Some(KeyReceiver::Shortcut {
            end: KeyEnd::Surface(VI),
        }),
    ] {
        assert_eq!(
            vi_key_step(VI, true, receiver, false, Some(VI)),
            ViKeyStep::Handle,
            "{receiver:?}"
        );
    }
    assert_eq!(
        vi_key_step(VI, true, None, false, Some(VI)),
        ViKeyStep::Handle
    );
}

/// Given vi 복사 모드인 터미널, When 다른 노드(팝업, 다른 surface, 오버레이가 열린 지금 경로)가 키를 받으면,
/// Then vi 복사 모드는 그 키를 받지 않는다.
#[test]
fn vi_copy_mode_takes_no_key_while_another_node_has_focus() {
    for receiver in [
        Some(KeyReceiver::Popup(PopupInstanceId(1))),
        Some(KeyReceiver::Surface(OTHER)),
        Some(KeyReceiver::EguiSurface(OTHER)),
        Some(KeyReceiver::Shortcut { end: KeyEnd::Drop }),
    ] {
        assert_eq!(
            vi_key_step(VI, true, receiver, false, Some(VI)),
            ViKeyStep::Pass,
            "{receiver:?}"
        );
    }
    assert_eq!(
        vi_key_step(VI, true, None, true, Some(VI)),
        ViKeyStep::Pass,
        "오버레이가 열린 지금 경로 키"
    );
    assert_eq!(
        vi_key_step(VI, true, None, false, Some(OTHER)),
        ViKeyStep::Pass,
        "다른 surface 가 포커스"
    );
}

/// Given vi 복사 모드의 대상 터미널이 보이지 않게 됐을 때, When 다른 노드가 받는 키가 오더라도, Then 그 키에서
/// vi 복사 모드를 끝낸다(지금 그대로 그 키는 소비한다).
#[test]
fn vi_copy_mode_ends_on_the_next_key_once_its_terminal_is_hidden() {
    assert_eq!(
        vi_key_step(
            VI,
            false,
            Some(KeyReceiver::Popup(PopupInstanceId(1))),
            true,
            Some(OTHER)
        ),
        ViKeyStep::End
    );
    assert_eq!(
        vi_key_step(VI, false, Some(KeyReceiver::Surface(VI)), false, Some(VI)),
        ViKeyStep::End
    );
}
