//! 별도 창(설정·플러그인·프리셋)의 사건 시점 노드와 입력 몫. 노드는 하나의 함수(`window_key_node`)가 정한다. 관리자
//! 포커스 인스턴스와 직전 패스 뒤 egui 포커스 기록(W1, `window_keys`)에 아직 그리지 않은 누름을 더한 값이다. 누름·키
//! 사건이 egui 입력에 들어오기 전에 그 노드로 입력을 끊고(`PopupManager::cut_key_frame`), 누름은 그 자리에서 포커스를
//! 옮긴다(`press_window_at`). 키가 실린 몫의 결속은 그 노드다(`bind_window_key`). 그리기 입구
//! (`put_window_pass_keys`)는 그 몫의 결속을 읽는다. 계약: `docs/architecture/input-layer.md`의 "별도 창 입력 패스".

use std::collections::VecDeque;

use super::focus::PopupNode;
use super::frame_keys::Incoming;
use super::window_keys::WindowFocusRecord;
use super::{PopupInstanceId, PopupManager};

/// 별도 창 키 사건의 노드. 팝업 인스턴스가 아니면 창 위젯이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowNode {
    Widget,
    Popup(PopupInstanceId),
}

impl WindowNode {
    /// 팝업 노드면 그 인스턴스.
    pub(crate) fn popup(self) -> Option<PopupInstanceId> {
        match self {
            Self::Widget => None,
            Self::Popup(iid) => Some(iid),
        }
    }
}

/// 별도 창의 몫별 노드와 패스 뒤 기록.
#[derive(Debug, Default)]
pub(crate) struct WindowSegments {
    /// 직전 패스 뒤 egui 포커스 기록.
    record: WindowFocusRecord,
    /// 아직 그리지 않은 누름이 정한 노드와, 그 누름이 실린 몫까지 남은 패스 수(0 이면 지금 그리는 몫).
    press: Option<(WindowNode, usize)>,
    /// 지금 그리는 몫에 처음 든 키 사건 시점의 노드.
    node: Option<WindowNode>,
    /// 끊은 뒤 몫들의 노드. 앞에서부터 그리기 순서다.
    after: VecDeque<Option<WindowNode>>,
}

impl WindowSegments {
    /// 지금 사건을 받는 몫(마지막으로 끊은 뒤 몫, 없으면 지금 그리는 몫)의 노드 칸.
    fn open_node(&mut self) -> &mut Option<WindowNode> {
        match self.after.back_mut() {
            Some(node) => node,
            None => &mut self.node,
        }
    }
}

impl PopupManager {
    /// 지금 사건 시점의 별도 창 노드. 아직 그리지 않은 누름이 있으면 그 누름이 정한 노드이고, 없으면 W1 키 주인
    /// (`window_key_owner`)이다. W1 은 관리자 키 주인이 있고 포커스 기록이 없거나 기록 레이어가 그 인스턴스 안이면 그
    /// 인스턴스다.
    pub(crate) fn window_key_node(&self, ctx: &egui::Context) -> WindowNode {
        if let Some((node, _)) = self.window.press {
            return node;
        }
        self.window_key_owner(ctx, self.window.record)
            .map_or(WindowNode::Widget, WindowNode::Popup)
    }

    /// 누름·키(`incoming`)가 egui 입력 큐 `events` 끝에 들어오기 전에 부른다. 사건 시점 노드가 지금 몫의 노드와
    /// 다르거나 그 사건이 몫을 나눠야 하면 끊는다(메인 창 `MainViewState::cut_key_frame` 과 같은 조건 함수). 끊은 수를
    /// 돌려준다. 호출자는 egui 입력도 그 자리에서 그 수만큼 끊는다.
    pub(crate) fn cut_window_segment(
        &mut self,
        ctx: &egui::Context,
        events: &[egui::Event],
        incoming: Incoming,
    ) -> usize {
        let focus = self.window_key_node(ctx);
        let mut cuts = 0;
        loop {
            let node = *self.window.open_node();
            let popup = node.unwrap_or(focus).popup().map(PopupNode::Host);
            let changed = node.is_some_and(|node| node != focus);
            if !self.cut_key_frame(events, incoming, popup, changed, false) {
                break;
            }
            self.window.open_node().get_or_insert(focus);
            self.window.after.push_back(None);
            cuts += 1;
        }
        if incoming.is_key() {
            self.window.open_node().get_or_insert(focus);
        }
        cuts
    }

    /// 누름을 사건 시점에 적용한다. `pos` 의 레이어가 열린 인스턴스(등록 자식 포함)면 그 인스턴스로, 아니면 창 위젯으로
    /// 포커스를 옮긴다(`focus_on_press`). 그 누름을 그리기 전까지 노드는 그 값이다.
    pub(crate) fn press_window_at(&mut self, ctx: &egui::Context, pos: egui::Pos2) {
        let hit = ctx.layer_id_at(pos).and_then(|layer| {
            self.popups
                .iter()
                .filter(|p| p.open)
                .find(|p| {
                    super::draw::area_id_of(p.id, p.instance) == layer.id
                        || super::child_popup_ids::child_layer_ids(ctx, p.instance)
                            .contains(&layer.id)
                })
                .map(|p| p.instance)
        });
        self.focus_on_press(hit.map(PopupNode::Host));
        let node = hit.map_or(WindowNode::Widget, WindowNode::Popup);
        self.window.press = Some((node, self.window.after.len()));
    }

    /// egui 에 실은 키 사건의 몫을 그 사건 시점 노드로 묶는다. 팝업이면 그 팝업, 창 위젯이면 팝업이 아닌 노드다.
    pub(crate) fn bind_window_key(&mut self, ctx: &egui::Context) {
        let node = match *self.window.open_node() {
            Some(node) => node,
            None => self.window_key_node(ctx),
        };
        self.bind_window_key_node(node.popup().map(PopupNode::Host));
    }

    /// 별도 창 패스 하나를 닫는다. 패스 뒤 egui 포커스를 기록하고 다음 몫의 노드를 꺼낸다. 그 패스가 그린 누름은 지운다.
    pub(crate) fn close_window_segment(&mut self, ctx: &egui::Context) {
        self.window.record = WindowFocusRecord::read(ctx);
        self.window.node = self.window.after.pop_front().flatten();
        self.window.press = match self.window.press {
            Some((node, left)) if left > 0 => Some((node, left - 1)),
            _ => None,
        };
    }
}
