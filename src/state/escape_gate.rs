//! 창의 Esc 처리(단계 11). 창은 그리기보다 먼저 사건 시점 수신자(단계 4 `route_escape`)의 결과를 적용한다. egui
//! 수신자는 단계 8(`window_feeds_key`)이 그 몫에 묶었으므로 이 Esc 를 실은 그리기의 키 주인은 그 수신자와 같다.
//!
//! 창이 처리한 Esc 누름은 언제나 egui 입력에서 뺀다(뗌은 남겨 egui 의 눌린 키 상태를 푼다). egui 는 패스 시작에서 입력에 남은 Esc 로 위젯 키 포커스를 풀므로,
//! 남기면 팝업 층 아래 화면(탐색기 찾기 입력란 등)이 포커스를 잃는다. 키 주인이 그 Esc 를 읽어야 하면(host 본문의
//! 하위 화면 되돌아가기, plugin 렌더의 닫기·다시 접기) 뺀 원래 사건을 그 키 주인을 그리는 동안만 되넣는다
//! (`body_keys::hand_escape`·`withdraw_escape`).

/// 창 Esc 처리 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeOutcome {
    /// 창이 처리하지 않았다. 다음 단계와 egui 입력으로 간다.
    Pass,
    /// 창이 처리했다. Esc 는 egui 입력에서 뺀다. `hand_to_owner`이면 묶인 키 주인(host 본문·plugin 렌더)이 뺀 원래
    /// 사건을 읽는다.
    Consumed { hand_to_owner: bool },
}

impl super::MainViewState {
    /// 단계 11 입구: 창이 받은 키 사건(`value`)이 수식키 없는 Esc 누름이면 사건 시점 수신자(`decision`)의 결과를
    /// 적용한다(`apply_escape_decision`). 대상인지는 단계 4 와 같은 판정이다(`key_class::is_bare_escape_press`, 논리 키가 없으면
    /// 물리 키). `fed` 는 단계 8 적재 결과다. 처리했으면 참이다.
    pub(crate) fn apply_escape_key(
        &mut self,
        value: &crate::adapters::ui::input::key_class::KeyEventValue<'_>,
        decision: crate::adapters::ui::input::key_nodes::KeyDecision,
        fed: bool,
        pending: &mut Vec<egui::Event>,
        escape_held: bool,
    ) -> bool {
        crate::adapters::ui::input::key_class::is_bare_escape_press(value)
            && self.apply_escape_decision(decision, fed, pending, escape_held)
    }

    /// 단계 11 맨 앞: 단계 4 수신자가 설정 열기 요청 래치 노드면 그 누름을 여기서 끝낸다(참). 무대·더블탭·단축키·vi·끝
    /// 목적지로 가지 않는다. bare Esc 면 적용 함수([`Self::apply_escape_key`])가 래치를 취소한다. 인자는 그 함수와 같다.
    pub(crate) fn end_key_at_settings_latch(
        &mut self,
        value: &crate::adapters::ui::input::key_class::KeyEventValue<'_>,
        decision: crate::adapters::ui::input::key_nodes::KeyDecision,
        fed: bool,
        pending: &mut Vec<egui::Event>,
        escape_held: bool,
    ) -> bool {
        if !crate::adapters::ui::input::key_nodes::settings_latch_receives(decision) {
            return false;
        }
        self.apply_escape_key(value, decision, fed, pending, escape_held);
        true
    }

    /// 단계 11: 창이 받은 bare Esc 누름 하나에 사건 시점 수신자(`decision`, 단계 4)의 결과를 적용한다. 처리했으면
    /// `pending`(egui 입력)에서 그 누름을 빼고, 키 주인이 읽을 몫이면 그 원래 사건을 원래 자리와 함께 넘겨 둔다. 뺄 누름은
    /// 이번 사건이 egui 에 실은 누름(큐의 마지막 Esc 누름)이고, 실렸는지(`fed`)는 단계 8 적재 결과다.
    /// egui 수신자의 결속은 단계 8(`window_feeds_key`)이 이미 했다. Esc 뗌은 빼지 않는다. `escape_held` 는 직전 egui 패스
    /// 끝의 Escape 눌림 상태다(키 주인에게 되넣을 때 egui 와 같은 반복 판정의 출발점). 처리했으면 참이고, 아니면 단축키·끝
    /// 목적지 단계로 간다.
    pub(crate) fn apply_escape_decision(
        &mut self,
        decision: crate::adapters::ui::input::key_nodes::KeyDecision,
        fed: bool,
        pending: &mut Vec<egui::Event>,
        escape_held: bool,
    ) -> bool {
        let EscapeOutcome::Consumed { hand_to_owner } = self.escape_outcome(decision) else {
            return false;
        };
        let press = fed
            .then(|| {
                pending
                    .iter()
                    .rposition(crate::adapters::ui::popup::body_keys::is_escape_press)
            })
            .flatten();
        if let Some(at) = press {
            let event = pending.remove(at);
            if hand_to_owner {
                self.popups
                    .hand_escape_to_bound_owner(escape_held, vec![(at, event)]);
            }
        }
        true
    }

    /// Escape 결과. 사건 시점 수신자가 래치 노드면 설정 요청을 취소하고, 사건 시점 수신자가
    /// host 팝업이면 선언 `escape` 대로 포커스를 풀어 넘긴다(펼친 팝업은 풀지 않고 넘긴다). Ignore 팝업은 수신자가 되지
    /// 않는다(`route_escape` 가 부모 경로로 올린다). plugin 팝업이면 그 렌더가 받도록 넘긴다. 그 밖의 수신자는 창이
    /// 처리하지 않는다. 닫기는 팝업 층 중앙 단계가 정한다.
    fn escape_outcome(
        &mut self,
        decision: crate::adapters::ui::input::key_nodes::KeyDecision,
    ) -> EscapeOutcome {
        use crate::adapters::ui::input::key_nodes::KeyDecision;
        use crate::adapters::ui::input::key_route::KeyReceiver;
        // 래치 노드(단계 4 수신자)가 받은 Esc 는 아직 처리하지 않은 설정 창 열기 요청을 취소한다.
        if crate::adapters::ui::input::key_nodes::settings_latch_receives(decision) {
            self.settings_open_requested = false;
            return EscapeOutcome::Consumed {
                hand_to_owner: false,
            };
        }
        let KeyDecision::Path(routed) = decision else {
            return EscapeOutcome::Pass;
        };
        match routed.receiver {
            // plugin 팝업 렌더가 Esc 를 받아 선언과 표시 상태로 처리한다(펼쳤으면 다시 접기).
            KeyReceiver::PluginPopup(_) => EscapeOutcome::Consumed {
                hand_to_owner: true,
            },
            // Close·Unfocus 는 지금 인스턴스 포커스를 풀고 그 Esc 를 넘긴다. 입력 시점의 키 주인(단계 8 이 묶은 이
            // 인스턴스) 본문이 팝업 층에서 이 Esc 를 받고, 본문이 쓰지 않았으면 Close 는 그 패스 중앙 단계
            // (`popup::escape_step`)가 닫는다. 펼친 host 팝업은 포커스를 풀지 않고 넘긴다. 본문이 쓰지 않았으면 중앙
            // 단계가 다시 접는다(닫지 않는다). 순서 검증: crates/tasty-doc-guards/tests/escape_dismisses_the_focused_popup.rs.
            KeyReceiver::Popup(instance) => {
                if !self.popups.instance(instance).is_some_and(|p| p.expanded()) {
                    self.popups.set_instance_focused(instance, false);
                }
                EscapeOutcome::Consumed {
                    hand_to_owner: true,
                }
            }
            _ => EscapeOutcome::Pass,
        }
    }
}
