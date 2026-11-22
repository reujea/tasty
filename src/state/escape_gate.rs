//! 창의 Esc 처리. 창은 그리기보다 먼저 이 판정을 돌린다. 판정 전에 입력 시점의 키 주인을 묶으므로
//! (`bind_window_key_owner`) 이 Esc 를 실은 그리기의 키 주인은 여기서 정한 대상과 같다.
//!
//! 창이 처리한 Esc 누름은 언제나 egui 입력에서 뺀다(뗌은 남겨 egui 의 눌린 키 상태를 푼다). egui 는 패스 시작에서 입력에 남은 Esc 로 위젯 키 포커스를 풀므로,
//! 남기면 팝업 층 아래 화면(탐색기 찾기 입력란 등)이 포커스를 잃는다. 키 주인이 그 Esc 를 읽어야 하면(host 본문의
//! 하위 화면 되돌아가기, plugin 렌더의 닫기·다시 접기) 뺀 원래 사건을 그 키 주인을 그리는 동안만 되넣는다
//! (`body_keys::hand_escape`·`withdraw_escape`).

/// 창 Esc 처리 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeGate {
    /// 창이 처리하지 않았다. 다음 단계와 egui 입력으로 간다.
    Pass,
    /// 창이 처리했다. Esc 는 egui 입력에서 뺀다. `hand_to_owner`이면 묶인 키 주인(host 본문·plugin 렌더)이 뺀 원래
    /// 사건을 읽는다.
    Consumed { hand_to_owner: bool },
}

impl super::MainViewState {
    /// 창이 받은 Esc 누름을 판정한다. 처리했으면 `pending`(이번 egui 입력)에서 그 누름 하나를 빼고, 키 주인이 읽을
    /// 몫이면 그 원래 사건을 원래 자리와 함께 넘겨 둔다. 고르는 누름은 이번 사건이 실은 범위(`loaded_from` 부터 끝까지)의
    /// 마지막 Esc 누름이다. 적재가 없었으면(`None` 이거나 범위에 Esc 누름이 없음) 큐를 바꾸지 않고 넘기지도 않는다. Esc
    /// 뗌은 빼지 않는다. egui 입력에 남아 눌린 키 상태를 푼다(넘긴 누름을 되넣을 때도 그 뗌이 뒤에 온다). `escape_held` 는
    /// 직전 egui 패스 끝의 Escape 눌림 상태다(키 주인에게 되넣을 때 egui 와 같은 반복 판정의 출발점). 처리했으면 참이다.
    pub(crate) fn window_escape(
        &mut self,
        pending: &mut Vec<egui::Event>,
        loaded_from: Option<usize>,
        escape_held: bool,
    ) -> bool {
        let EscapeGate::Consumed { hand_to_owner } = self.escape_gate() else {
            return false;
        };
        let press = loaded_from.and_then(|from| {
            let from = from.min(pending.len());
            pending[from..]
                .iter()
                .rposition(crate::adapters::ui::popup::body_keys::is_escape_press)
                .map(|at| from + at)
        });
        if let Some(at) = press {
            let event = pending.remove(at);
            if hand_to_owner {
                self.popups
                    .hand_escape_to_bound_owner(escape_held, vec![(at, event)]);
            }
        }
        true
    }

    /// Escape로 설정 요청·알림을 처리하고, host 키 주인은 선언 `escape` 대로 포커스를 풀어 넘긴다(Ignore 는 넘기지
    /// 않는다). 닫기는 팝업 층 중앙 단계가 정한다.
    fn escape_gate(&mut self) -> EscapeGate {
        if self.settings_open_requested {
            // 아직 처리하지 않은 설정 창 열기 요청을 취소한다.
            self.settings_open_requested = false;
            return EscapeGate::Consumed {
                hand_to_owner: false,
            };
        }
        // 이 Esc 를 실은 그리기의 키 주인을 지금 정한다. 아래 갈래가 포커스를 풀거나 닫아도 그 프레임에 새로 키
        // 주인이 된 다른 팝업은 같은 Esc 를 읽지 않는다.
        let live = self.live_key_owner();
        if let Some(live) = live {
            self.popups.bind_input_key_owner(live);
        }
        // 이번 프레임에 앞서 묶인 키 주인이 있으면 이 Esc 도 그 주인 것이다. 창이 이미 그 주인의 포커스를 풀었거나
        // 닫았으면(그리기 전 두 번째 Esc) 더 처리하지 않는다. 다른 범위의 팝업을 풀거나 닫지 않는다. 묶인 주인에는
        // 첫 Esc 만 넘어간다.
        let bound = self.popups.bound_input_key_owner();
        if bound.is_some() && bound != live {
            return EscapeGate::Consumed {
                hand_to_owner: false,
            };
        }
        // 키 주인도 알림 목록도 없으면 창이 처리할 것이 없다.
        if live.is_none() && !self.popups.is_open("notifications") {
            return EscapeGate::Pass;
        }
        // 알림 목록은 포커스 노드가 다른 팝업이 아닐 때만 Esc 를 받는다. 창을 덮는 피커처럼 다른 팝업이 포커스 노드면
        // Esc 는 그 팝업에서 시작한다.
        if self.notifications_take_escape(live) {
            self.dispatch_intent(
                crate::intent::UiIntent::ClosePopup {
                    id: "notifications",
                }
                .from_user_shortcut("escape_close_notifications"),
            );
            return EscapeGate::Consumed {
                hand_to_owner: false,
            };
        }
        // plugin 팝업이 키 주인이면 그 렌더가 Esc 를 받아 선언과 표시 상태로 처리한다(펼쳤으면 다시 접기). 아래의
        // 포커스된 host 팝업과 알림 목록은 건드리지 않는다(plugin 키 주인이면 알림 목록은 위에서 Esc 를 받지 않는다).
        if matches!(
            self.popup_key_owner(),
            Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(_))
        ) {
            return EscapeGate::Consumed {
                hand_to_owner: true,
            };
        }
        // 설정 요청과 알림을 먼저 처리한 뒤, host 키 주인의 Esc 를 선언대로 처리한다. Close·Unfocus 는 지금 인스턴스
        // 포커스를 풀고 그 Esc 를 넘긴다. 입력 시점의 키 주인(위에서 묶은 이 인스턴스) 본문이 팝업 층에서 이 Esc 를
        // 받고, 본문이 쓰지 않았으면 Close 는 그 패스 중앙 단계(`popup::escape_step`)가 닫는다. Ignore 는 넘기지 않는다.
        // 순서 검증: crates/tasty-doc-guards/tests/escape_dismisses_the_focused_popup.rs.
        if let Some((instance, escape)) = self.popups.host_key_owner_escape() {
            use crate::adapters::ui::popup::PopupEscape;
            // 펼친 host 팝업은 포커스를 풀지 않고 넘긴다. 본문이 쓰지 않았으면 중앙 단계가 이 Esc 에 묶인 인스턴스를
            // 다시 접는다(닫지 않는다). 선언 `escape` 와 무관하다.
            if self.popups.instance(instance).is_some_and(|p| p.expanded()) {
                return EscapeGate::Consumed {
                    hand_to_owner: true,
                };
            }
            if escape == PopupEscape::Ignore {
                return EscapeGate::Consumed {
                    hand_to_owner: false,
                };
            }
            self.popups.set_instance_focused(instance, false);
            return EscapeGate::Consumed {
                hand_to_owner: true,
            };
        }
        EscapeGate::Pass
    }

    /// 열린 알림 목록이 이 Esc 로 닫히는지. 포커스 노드가 팝업이 아니거나 알림 목록 자신일 때다.
    fn notifications_take_escape(
        &self,
        live: Option<crate::adapters::ui::popup::focus::PopupNode>,
    ) -> bool {
        use crate::adapters::ui::popup::focus::PopupNode;
        self.popups.is_open("notifications")
            && match live {
                None => true,
                Some(PopupNode::Host(iid)) => self
                    .popups
                    .instance(iid)
                    .is_some_and(|p| p.id == "notifications"),
                Some(PopupNode::Plugin(_)) => false,
            }
    }
}
