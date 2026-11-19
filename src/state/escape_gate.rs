//! 창의 Esc 처리. 창은 그리기보다 먼저 이 판정을 돌린다. 판정 전에 입력 시점의 키 주인을 묶으므로
//! (`bind_window_key_owner`) 이 Esc 를 실은 그리기의 키 주인은 여기서 정한 대상과 같다.
//!
//! 창이 처리한 Esc 는 언제나 egui 입력에서 뺀다. egui 는 패스 시작에서 입력에 남은 Esc 로 위젯 키 포커스를 풀므로,
//! 남기면 팝업 층 아래 화면(탐색기 찾기 입력란 등)이 포커스를 잃는다. 키 주인이 그 Esc 를 읽어야 하면(host 본문의
//! 하위 화면 되돌아가기, plugin 렌더의 닫기·다시 접기) 뺀 원래 사건을 그 키 주인을 그리는 동안만 되넣는다
//! (`body_keys::with_handed_escape`).

use crate::adapters::ui::popup::occlusion::PopupKeyOwner;

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
    /// 창이 받은 Esc 를 판정한다. 처리했으면 `pending`(이번 egui 입력)에서 Esc 사건을 빼고, 키 주인이 읽을 몫이면 그
    /// 원래 사건을 원래 자리와 함께 넘겨 둔다. `escape_held` 는 직전 egui 패스 끝의 Escape 눌림 상태다(키 주인에게
    /// 되넣을 때 egui 와 같은 반복 판정의 출발점). 처리했으면 참이다.
    pub(crate) fn window_escape(
        &mut self,
        pending: &mut Vec<egui::Event>,
        escape_held: bool,
    ) -> bool {
        let EscapeGate::Consumed { hand_to_owner } = self.escape_gate() else {
            return false;
        };
        let mut taken = Vec::new();
        let mut kept = 0;
        pending.retain(|e| {
            let escape = crate::adapters::ui::popup::body_keys::is_escape(e);
            if escape {
                taken.push((kept, e.clone()));
            } else {
                kept += 1;
            }
            !escape
        });
        if hand_to_owner {
            self.popups.hand_escape_to_bound_owner(escape_held, taken);
        }
        true
    }

    /// Escape로 설정 요청·알림을 처리하고 팝업 포커스를 해제한다.
    /// 바깥 클릭으로 닫을 수 있는 팝업만 닫는다.
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
        // 키 주인(host·plugin)이 창 위로 펼친 팝업이면 다시 접는다. 닫지 않고 포커스와 상태를 그대로 둔다.
        // 아래 알림 목록이나 host 팝업보다 먼저 판정해 그쪽을 접거나 닫지 않는다. plugin 키 주인은 그 렌더가
        // 이 Esc 로 접으므로 그 렌더에 넘긴다.
        let plugin_owns_keys = matches!(self.popup_key_owner(), Some(PopupKeyOwner::Plugin(_)));
        if self.refold_key_owner() {
            return EscapeGate::Consumed {
                hand_to_owner: plugin_owns_keys,
            };
        }
        if self.popups.is_open("notifications") {
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
        // plugin 팝업이 키 주인이면 그 렌더가 Esc 를 받는다(선언이 `Close`면 닫는다). 아래의 포커스된 host 팝업은
        // 건드리지 않는다.
        if matches!(
            self.popup_key_owner(),
            Some(crate::adapters::ui::popup::occlusion::PopupKeyOwner::Plugin(_))
        ) {
            return EscapeGate::Consumed {
                hand_to_owner: true,
            };
        }
        // 설정 요청과 알림을 먼저 처리한 뒤, 포커스된 팝업 하나의 포커스를 해제한다.
        // 바깥 클릭과 같이 close_on_outside_click인 팝업만 닫는다.
        // 순서 검증: crates/tasty-doc-guards/tests/escape_dismisses_the_focused_popup.rs.
        if let Some((_, instance, closes)) = self.popups.focused_dismissal_target() {
            self.popups.set_instance_focused(instance, false);
            if closes {
                self.dispatch_intent(
                    crate::intent::UiIntent::CloseInstance {
                        instance,
                        reason: crate::adapters::ui::popup::PopupCloseReason::Escape,
                    }
                    .from_user_shortcut("escape_dismiss_focused_popup"),
                );
            }
            // 포커스만 풀었으면 입력 시점의 키 주인(위에서 묶은 이 인스턴스) 본문이 팝업 층에서 이 Esc 를 받는다
            // (하위 화면이면 목록으로 돌아간다). 닫았으면 받을 본문이 없다.
            return EscapeGate::Consumed {
                hand_to_owner: !closes,
            };
        }
        EscapeGate::Pass
    }
}
