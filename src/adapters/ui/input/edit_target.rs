//! 편집 키(복사·잘라내기·붙여넣기)의 대상. 편집 바인딩 키와 클립보드 조합은 경로 키이고(`key_class::classify_key`),
//! 그 대상은 단계 4 수신자가 정한다. 단계 6 의 host egui 편집 판정, plugin 팝업 편집 요청, plugin surface 편집
//! 요청과 단계 8 의 plugin 붙여넣기 대상이 같은 수신자를 읽는다. 지금 경로로 남는 사건(뗌, 수식키 단독 누름)은
//! 호출자의 지금 판정(오버레이 게이트, 키 주인 plugin 팝업)을 쓴다. 수식키 없는 Esc 는 편집 바인딩과 겹쳐도 수신자의
//! Esc 처리로 가므로 어느 편집 대상도 아니다(`Routed::escape`). 계약: `docs/design/policies/key-mapping.md`.

use super::key_nodes::{KeyDecision, path_receiver};
use super::key_route::{KeyReceiver, Routed};

/// 편집 대상을 정하는 수신자. bare Esc 의 수신자는 편집 대상이 아니므로 버림으로 읽는다.
fn edit_receiver(decision: KeyDecision) -> Option<KeyReceiver> {
    match decision {
        KeyDecision::Path(Routed { escape: true, .. }) => Some(KeyReceiver::Drop),
        _ => path_receiver(decision),
    }
}

/// 단계 6 host egui 편집 판정의 대상인지. 경로 키는 egui 수신자(팝업·주소 띠·안내·튜토리얼·무대)면 대상이고, egui
/// 탐색기 surface 면 `typing`(입력칸 포커스)일 때만이다. 경로를 올라간 키(단축키·터미널)는 대상이 아니다. 지금
/// 경로로 남는 사건은 `current` 다.
pub(crate) fn host_edit_target(
    decision: KeyDecision,
    typing: impl FnOnce() -> bool,
    current: impl FnOnce() -> bool,
) -> bool {
    match edit_receiver(decision) {
        Some(
            KeyReceiver::Popup(_)
            | KeyReceiver::PluginPopup(_)
            | KeyReceiver::Notice(_)
            | KeyReceiver::Tutorial
            | KeyReceiver::HtmlAddress(_)
            | KeyReceiver::Stage
            | KeyReceiver::Window,
        ) => true,
        Some(KeyReceiver::EguiSurface(_)) => typing(),
        Some(
            KeyReceiver::Shortcut { .. }
            | KeyReceiver::Surface(_)
            | KeyReceiver::Drop
            | KeyReceiver::SettingsRequest,
        ) => false,
        None => current(),
    }
}

/// plugin 팝업 편집 요청과 plugin 붙여넣기의 대상 instance. 경로 키는 수신자가 그 plugin 팝업일 때만이다. 지금
/// 경로로 남는 사건은 `key_owner`(키 주인 plugin 팝업)다.
pub(crate) fn plugin_popup_edit_target(
    decision: KeyDecision,
    key_owner: Option<u64>,
) -> Option<u64> {
    match edit_receiver(decision) {
        Some(KeyReceiver::PluginPopup(instance)) => Some(instance),
        Some(_) => None,
        None => key_owner,
    }
}

/// plugin surface 편집 요청의 대상 surface. 경로 키는 수신자가 경로 끝 surface(`Surface`, 후보가 모두 거절하면 끝
/// surface 로 가는 단축키 단계)일 때 그 surface 다. egui 노드·버림(덮는 팝업이 쓰지 않은 키)·래치·bare Esc 면 없다.
/// 지금 경로로 남는 사건은 `current`(편집 키가 지금 닿는 surface, `key_nodes::edit_key_surface`)다.
pub(crate) fn plugin_surface_edit_target(
    decision: KeyDecision,
    current: impl FnOnce() -> Option<u32>,
) -> Option<u32> {
    use super::key_route::KeyEnd;
    match edit_receiver(decision) {
        Some(
            KeyReceiver::Surface(sid)
            | KeyReceiver::Shortcut {
                end: KeyEnd::Surface(sid),
            },
        ) => Some(sid),
        Some(_) => None,
        None => current(),
    }
}

#[cfg(test)]
#[path = "edit_target_tests.rs"]
mod tests;
