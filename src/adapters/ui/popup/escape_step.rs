//! 팝업 층 Esc 의 프레임 시점 단계(중앙 단계). 창은 사건 시점에 수식키 없는 Esc 만 받고 host 키 주인의 포커스를
//! 풀어 그 Esc 를 넘긴다(`state::escape_gate`). 본문을 그린 뒤 이 단계가 선언 `escape` 와 본문 결과로 마무리한다.
//! 본문이 그 Esc 를 썼으면 닫지 않는다. 썼다는 것은 셋 가운데 하나다: (a) 본문이 키 통로로 Esc 를 소비했다, (b) 본문
//! 결과가 `PopupAction::None` 이 아니다, (c) 본문을 그리기 전에 이 인스턴스의 자식 팝업이 열려 있었다(egui 가 그 자식을
//! 닫는다). 쓰지 않았고 선언이 Close 면 사유 Escape 로 닫는다. Unfocus 는 사건 시점 해제로 끝이다. 펼친 host 팝업은
//! 창이 포커스를 풀지 않고 넘기며, 쓰지 않았으면 선언과 무관하게 그 Esc 에 묶인 인스턴스를 다시 접는다(닫지 않는다).
//!
//! 별도 창에는 창의 Esc 판정이 없다. 그 창이 egui 에 적재한 Esc 누름마다 사건 시점 수식키를 표지로 남기고(`Gpu`),
//! 공용 패스가 그 몫의 표지를 W1 노드와 함께 관리자를 그리는 함수에 넘긴다(`WindowPassKeys`). 그 패스 사건은 "그 패스
//! 키 주인이 있고 표지 수식키가 없는 Esc 누름이 있다" 이다.

use super::body_keys::{EscapeMarks, is_escape_press};
use super::keys::{BasicKey, ConsumedKey};
use super::{PopupAction, PopupCloseReason, PopupEscape, PopupInstanceId, PopupManager};

/// 별도 창이 이번 egui 패스마다 그 관리자에 넣는 키 값. 공용 패스(`view::window_input::WindowPass`)가 `run` 전에 한 번
/// 정하고 다시 그리는 패스에도 같은 값을 넣는다.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WindowPassKeys {
    /// 결속이 없는 몫의 키 주인: `run` 시작 때 노드(`PopupManager::window_key_node`)의 인스턴스.
    pub(crate) owner: Option<PopupInstanceId>,
    /// 이번 패스 몫이 egui 에 적재한 Esc 누름마다 사건 시점 수식키(`Gpu::take_window_escape_marks`).
    pub(crate) marks: Vec<tasty_key_match::KeyMods>,
}

/// 별도 창 표지와 패스 입력의 짝 맞춤 결과.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MarkPairing {
    /// 이 패스는 raw 입력이 비었다(같은 입력 묶음을 다시 부른 패스). 대조하지 않고 누름에 표지를 붙이지 않는다.
    Skipped,
    /// 표지 수와 raw·처리된 입력의 Esc 누름 수가 같다. 누름마다 표지.
    Paired(EscapeMarks),
    /// 수가 다르다(egui 가 패스 시작에 Esc 를 지운 경우 등). 그 패스 Esc 누름은 모두 표지가 없다.
    Mismatch {
        marks: usize,
        raw: usize,
        events: usize,
    },
}

/// 별도 창 표지(`marks`)를 그 패스 `raw` 입력과 처리된 `events` 의 Esc 누름에 맞춘다.
pub(crate) fn pair_window_marks(
    marks: &[tasty_key_match::KeyMods],
    raw: &[egui::Event],
    events: &[egui::Event],
) -> MarkPairing {
    if raw.is_empty() {
        return MarkPairing::Skipped;
    }
    let raw_presses = raw.iter().filter(|e| is_escape_press(e)).count();
    let presses = events.iter().filter(|e| is_escape_press(e)).count();
    if raw_presses == marks.len() && presses == marks.len() {
        MarkPairing::Paired(marks.iter().copied().map(Some).collect())
    } else {
        MarkPairing::Mismatch {
            marks: marks.len(),
            raw: raw_presses,
            events: presses,
        }
    }
}

/// 별도 창 표지를 이번 패스 몫(`raw`)과 뒤 몫으로 나눈다. 끊은 자리에서 나눈 패스(`split`)면 `raw` 의 Esc 누름 수만큼이
/// 이번 패스 것이고 나머지는 `loaded` 에 남아 뒤 몫이 받는다. 나누지 않은 패스는 모두 이번 패스 것이다.
pub(crate) fn split_pass_marks(
    loaded: &mut Vec<tasty_key_match::KeyMods>,
    raw: &[egui::Event],
    split: bool,
) -> Vec<tasty_key_match::KeyMods> {
    let pass = if split {
        let presses = raw.iter().filter(|e| is_escape_press(e)).count();
        presses.min(loaded.len())
    } else {
        loaded.len()
    };
    let later = loaded.split_off(pass);
    std::mem::replace(loaded, later)
}

/// 본문이 이 패스 Esc 를 썼는지((a)(b)(c)).
fn content_used_escape(consumed: &[ConsumedKey], action: &PopupAction, child_open: bool) -> bool {
    consumed.contains(&ConsumedKey::Basic(BasicKey::Escape))
        || !matches!(action, PopupAction::None)
        || child_open
}

/// 이 패스 Esc 를 받은 키 주인의 마무리. 본문이 쓰지 않았고 선언이 Close 면 닫는 결과를 돌려준다.
pub(super) fn central_close(
    escape: PopupEscape,
    consumed: &[ConsumedKey],
    action: &PopupAction,
    child_open: bool,
) -> Option<PopupAction> {
    (escape == PopupEscape::Close && !content_used_escape(consumed, action, child_open))
        .then_some(PopupAction::CloseBy(PopupCloseReason::Escape))
}

/// 이 패스 Esc 를 받은 펼친 키 주인의 마무리. 본문이 쓰지 않았으면 다시 접는다(닫지 않는다).
pub(super) fn central_refold(
    consumed: &[ConsumedKey],
    action: &PopupAction,
    child_open: bool,
) -> bool {
    !content_used_escape(consumed, action, child_open)
}

/// 이번 패스 Esc 사건과 키 주인 통로에 줄 표지.
pub(super) struct PassEscape {
    /// 이번 패스 키 주인이 받은 Esc 가 있다(메인 창은 넘긴 Esc, 별도 창은 수식키 없는 표지 누름).
    pub(super) event: bool,
    /// 패스 `events` 의 Esc 누름마다 표지.
    pub(super) marks: EscapeMarks,
}

impl PassEscape {
    /// 별도 창이 넣은 표지로 만든다.
    pub(super) fn from_window(marks: EscapeMarks) -> Self {
        let event = marks
            .iter()
            .flatten()
            .any(|m| tasty_key_match::is_window_escape(true, *m));
        Self { event, marks }
    }
}

impl PopupManager {
    /// 창의 Esc 판정이 쓰는 host 키 주인과 그 선언 `escape`.
    pub(crate) fn host_key_owner_escape(&self) -> Option<(PopupInstanceId, PopupEscape)> {
        self.host_key_owner().map(|p| (p.instance, p.escape))
    }

    /// 별도 창이 이번 패스 키 값을 넣는다. 관리자를 그리는 함수가 팝업을 그리기 전에 패스마다 부른다. 그 몫의 결속이
    /// 있으면 그것을, 없으면 W1 노드를 그 패스 키 주인으로 정하고(`decide_window_frame_keys`), 표지를 그 패스 입력에
    /// 맞춰 둔다. 수가 다르면 경고를 남기고 표지를 붙이지 않는다. 창 위젯이 이 패스 키를 받으면(키 주인 없음) 참이다.
    pub(crate) fn put_window_pass_keys(
        &mut self,
        ctx: &egui::Context,
        keys: &WindowPassKeys,
    ) -> bool {
        let frame = self.decide_window_frame_keys(ctx, keys.owner);
        let (pairing, presses) = ctx.input(|i| {
            (
                pair_window_marks(&keys.marks, &i.raw.events, &i.events),
                i.events.iter().filter(|e| is_escape_press(e)).count(),
            )
        });
        let marks = match pairing {
            MarkPairing::Paired(marks) => marks,
            MarkPairing::Skipped => vec![None; presses],
            MarkPairing::Mismatch { marks, raw, events } => {
                tracing::warn!(
                    marks,
                    raw,
                    events,
                    "window Escape marks do not match the pass input; its Escape presses get no mark"
                );
                vec![None; presses]
            }
        };
        self.keys.window_escape = Some(marks);
        !frame.held()
    }

    /// 별도 창이 이번 패스에 넣은 표지. 넣지 않은 관리자(메인 창)는 `None` 이다.
    pub(super) fn take_window_escape(&mut self) -> Option<EscapeMarks> {
        self.keys.window_escape.take()
    }
}

#[cfg(test)]
#[path = "escape_step_tests.rs"]
mod tests;
