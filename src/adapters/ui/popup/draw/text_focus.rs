//! 키 주인이 아닌 팝업의 egui 텍스트 포커스를 놓는다(G7). 관리자가 모든 인스턴스를 그린 뒤 한 번 본다.
//! 포커스 위젯이 텍스트 편집 위젯이고 그 레이어가 키를 받지 않는(`takes_keys` 거짓) 인스턴스의 영역이나 그 등록 자식
//! 레이어면 포커스를 놓는다. 선언 `keyboard`가 `None`인 인스턴스(복원 안내처럼 안쪽 포커스로 키를 읽는다), 텍스트가 아닌
//! 위젯(버튼), 등록되지 않은 바깥 위젯은 대상이 아니다. 메인 창과 별도 창 모두 누름을 사건 시점에 적용해 키 주인
//! 판정보다 먼저 반영하므로, 누른 팝업은 그 패스의 키 주인이고 누름으로 얻은 포커스는 놓을 대상이 아니다.

use super::super::{PopupInstanceId, PopupKeyboard};

/// 이번 패스 본문 하나의 키 값.
pub(super) struct BodyKeys {
    /// 본문 영역의 egui Area id(Foreground 레이어 id).
    pub(super) area: egui::Id,
    pub(super) instance: PopupInstanceId,
    /// 본문 문 값. 참이면 그 본문이 키를 받는다.
    pub(super) takes_keys: bool,
    pub(super) keyboard: PopupKeyboard,
}

/// 패스 끝 egui 포커스가 키를 받지 않는 본문의 텍스트 위젯이면 놓는다.
pub(super) fn release_non_owner_text_focus(ctx: &egui::Context, bodies: &[BodyKeys]) {
    let Some(focused) = ctx.memory(|m| m.focused()) else {
        return;
    };
    if egui::TextEdit::load_state(ctx, focused).is_none() {
        return;
    }
    // 레이어만 읽는다. `read_response` 는 응답을 다시 만들며 그 패스의 상호작용 상태를 건드린다(누름 패스에 부르면
    // 누른 입력칸이 다음 패스 글자를 받지 못했다).
    let Some(layer) = ctx.viewport(|v| {
        v.this_pass
            .widgets
            .get(focused)
            .or_else(|| v.prev_pass.widgets.get(focused))
            .map(|w| w.layer_id)
    }) else {
        return;
    };
    let body = bodies.iter().find(|b| {
        b.area == layer.id
            || super::super::child_popup_ids::child_layer_ids(ctx, b.instance).contains(&layer.id)
    });
    if body.is_some_and(|b| !b.takes_keys && b.keyboard != PopupKeyboard::None) {
        ctx.memory_mut(|m| m.surrender_focus(focused));
        ctx.request_repaint();
    }
}
