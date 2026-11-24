//! vi 복사 모드가 키 하나를 받는지. vi 복사 모드는 그 터미널 surface 노드의 내용이라, 키가 그 노드에서 끝날 때만
//! 받는다. 대상 터미널이 보이지 않게 된 정리는 포커스와 무관하게 다음 키에서 한다(그 키는 소비한다).

use crate::adapters::ui::input::key_route::{KeyEnd, KeyReceiver};

/// 단계 11 vi 자리에서 키 하나의 처리.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViKeyStep {
    /// 대상 터미널이 보이지 않는다. vi 복사 모드를 끝내고 그 키를 소비한다.
    End,
    /// vi 복사 모드가 그 키를 처리한다.
    Handle,
    /// vi 복사 모드가 받지 않는다. 다음 단계(끝 목적지)로 간다.
    Pass,
}

/// `vi_sid` 의 vi 복사 모드가 이 키를 어떻게 하는지. `visible` 은 대상 터미널이 보이는지, `receiver` 는 경로 키의 단계 4
/// 수신자(지금 경로로 남는 사건은 `None`), `overlay_open` 은 터미널 게이트(`window_key_holds_terminal`), `focused` 는 지금
/// 포커스 surface 다.
pub(crate) fn vi_key_step(
    vi_sid: u32,
    visible: bool,
    receiver: Option<KeyReceiver>,
    overlay_open: bool,
    focused: Option<u32>,
) -> ViKeyStep {
    if !visible {
        return ViKeyStep::End;
    }
    let on_terminal = match receiver {
        Some(
            KeyReceiver::Surface(sid)
            | KeyReceiver::Shortcut {
                end: KeyEnd::Surface(sid),
            },
        ) => sid == vi_sid,
        Some(_) => false,
        None => !overlay_open && focused == Some(vi_sid),
    };
    if on_terminal {
        ViKeyStep::Handle
    } else {
        ViKeyStep::Pass
    }
}

#[cfg(test)]
#[path = "vi_key_node_tests.rs"]
mod tests;
