//! 별도 창(설정·플러그인·프리셋)의 사건 시점 노드와 입력 몫. 노드는 하나의 함수(`window_key_node`)가 정한다. 관리자
//! 포커스 인스턴스와 직전 패스 뒤 egui 포커스 기록(W1, `window_keys`)에 아직 그리지 않은 누름을 더한 값이다. 누름·키
//! 사건이 egui 입력에 들어오기 전에 그 노드로 입력을 끊고(`PopupManager::cut_key_frame`), 누름은 그 자리에서 포커스를
//! 옮긴다(`press_window_at`). 키가 실린 몫의 결속은 그 노드다(`bind_window_key`). 그리기 입구
//! (`window_widget_takes_keys`)는 그 몫의 결속을 읽는다. 계약: `docs/architecture/input-layer.md`의 "별도 창 입력 패스".

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

/// 별도 창 키 사건 하나의 단계 4 수신 결과(`PopupManager::settle_window_key`). 메인 창의 `KeyReceiver` 와 같이 egui
/// 수신자(팝업·창 위젯 내용), 창 자체 처리 후보, 버림을 가른다. 단계 6 편집 판정(`window_input::window_edit_target`),
/// 몫 경계(`cut_window_segment`), 단계 8 적재와 결속(`bind_window_key`·`apply_window_escape`), 단계 11 창 자체
/// 처리가 이 한 값을 읽는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowReceiver {
    /// 팝업이 받는다(egui 수신자, 그 팝업 몫).
    Popup(PopupInstanceId),
    /// 창 범위 노드의 내용인 창 위젯이 받는다(egui 수신자, 창 위젯 몫). 창 자체 처리도 그 키를 본다.
    Widget,
    /// 창 자체 처리의 후보(engine level)가 받는다. egui 에는 싣지 않는다.
    Own,
    /// 아무도 받지 않는다. egui 에도 창 자체 처리에도 가지 않는다.
    Nobody,
}

impl WindowReceiver {
    /// egui 에 실을 몫의 노드. egui 수신자가 없으면 None 이다.
    pub(crate) fn egui_node(self) -> Option<WindowNode> {
        match self {
            Self::Popup(iid) => Some(WindowNode::Popup(iid)),
            Self::Widget => Some(WindowNode::Widget),
            Self::Own | Self::Nobody => None,
        }
    }

    /// 단계 11 창 자체 처리가 이 키를 보는지(창 노드가 받았다).
    pub(crate) fn reaches_window(self) -> bool {
        matches!(self, Self::Widget | Self::Own)
    }
}

/// 별도 창 키 사건 하나의 경로 값(단계 4 입력).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowKey {
    /// 경로 키. `own` 은 그 키가 창 자체 처리의 바인딩에 맞는지다(`key_nodes::separate_window_receiver`).
    Path {
        key: crate::adapters::ui::input::key_route::RouteKey,
        own: bool,
    },
    /// 지금 경로로 남는 사건(뗌·수식키 단독 누름·IME). 사건 시점 노드가 받는다.
    Current,
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
    /// 이번 `run` 의 결속 없는 몫 키 주인: `run` 시작 때 노드의 인스턴스(`begin_window_run`).
    run_owner: Option<PopupInstanceId>,
    /// 마지막 키 사건의 단계 4 수신 결과(`settle_window_key`). 그 사건의 결속(`bind_window_key`·
    /// `apply_window_escape`)이 꺼내 쓴다.
    key_receiver: Option<WindowReceiver>,
    /// 마지막 키 사건 경로의 level 집합(`settle_window_key`). 단계 11 더블탭 동작 거르기가 읽는다.
    key_levels: Option<crate::adapters::ui::input::key_level::PathLevels>,
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
    /// 지금 사건 시점의 별도 창 노드. 출발 값은 아직 그리지 않은 누름이 있으면 그 누름이 정한 노드이고, 없으면 기록으로
    /// 정한 W1 값(`window_recorded_owner`)이다. 덮인 노드는 노드가 될 수 없다(`window_uncovered`). 끊기·결속·단계 4 와
    /// 관리자 포커스 표시(`show_window_focus`)가 이 한 값을 읽는다.
    pub(crate) fn window_key_node(&self, ctx: &egui::Context) -> WindowNode {
        let start = match self.window.press {
            Some((node, _)) => node.popup(),
            None => self.window_recorded_owner(ctx, self.window.record),
        };
        self.window_uncovered(start)
            .map_or(WindowNode::Widget, WindowNode::Popup)
    }

    /// 덮음 규칙을 적용하는 한 자리: 출발 값 `start`(None 이면 창 위젯)보다 위에 창을 덮는 팝업이 있으면 그 가운데
    /// 맨 위, 없으면 `start` 다. 사건 시점 노드(`window_key_node`)와 W1(`window_key_owner`)이 이 함수로 덮음을 적용한다.
    pub(crate) fn window_uncovered(
        &self,
        start: Option<PopupInstanceId>,
    ) -> Option<PopupInstanceId> {
        self.window_cover_above(start).or(start)
    }

    /// 노드 `node`(None 이면 창 위젯)보다 위(z)에서 창을 덮는 보이는 팝업 가운데 맨 위. 별도 창의 팝업은 모두 Window
    /// 범위라 덮는 팝업은 창 전체를 덮는다. 덮음 적용(`window_uncovered`)만 이 규칙을 읽는다.
    fn window_cover_above(&self, node: Option<PopupInstanceId>) -> Option<PopupInstanceId> {
        let above = node.and_then(|iid| self.instance(iid)).map(|p| p.z_seq);
        self.covering_nodes()
            .filter(|(_, _, z)| above.is_none_or(|a| *z > a))
            .max_by_key(|(_, _, z)| *z)
            .map(|(iid, _, _)| iid)
    }

    /// egui 에 실을 누름·키(`incoming`)가 egui 입력 큐 `events` 끝에 들어오기 전에 부른다. 그 사건이 묶일 노드
    /// (`node`: 키면 단계 4 결과의 egui 수신자 몫 `WindowReceiver::egui_node`, 누름이면 사건 시점 노드)가 지금 몫의
    /// 노드와 다르거나 그 사건이 몫을 나눠야 하면 끊는다(메인 창 `MainViewState::cut_key_frame` 과 같은 조건 함수). 끊은
    /// 수를 돌려준다. 호출자는 egui 입력도 그 자리에서 그 수만큼 끊는다.
    pub(crate) fn cut_window_segment(
        &mut self,
        events: &[egui::Event],
        incoming: Incoming,
        node: WindowNode,
    ) -> usize {
        let focus = node;
        let mut cuts = 0;
        loop {
            let node = *self.window.open_node();
            let popup = node.unwrap_or(focus).popup().map(PopupNode::Host);
            let changed = if node.is_some_and(|node| node != focus) {
                super::frame_keys::NodeChange::Focus
            } else {
                super::frame_keys::NodeChange::Same
            };
            if !self.cut_key_frame(events, incoming, popup, changed) {
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

    /// 관리자 포커스 표시를 사건 시점 노드(`window_key_node`)에 맞춘다. 노드가 팝업이면 그 팝업이 포커스 위치다(덮는
    /// 팝업이 덮인 포커스를 대신한 경우가 여기서 바뀐다). 노드가 창 위젯이면 관리자 포커스를 그대로 둔다(기록이 팝업 밖인
    /// W1). 키·IME·누름 사건 입구 맨 앞(`show_window_key_node`)과 `run` 앞에 부른다.
    fn show_window_focus(&mut self, node: WindowNode) {
        if let WindowNode::Popup(iid) = node
            && self.host_key_owner().map(|p| p.instance) != Some(iid)
        {
            self.set_instance_focused(iid, true);
        }
    }

    /// 누름을 사건 시점에 적용한다. `pos` 의 레이어가 열린 인스턴스(등록 자식 포함)나 그 덮음 막이면 그 인스턴스로,
    /// 아니면 창 위젯으로 포커스를 옮긴다(`focus_on_press`, 메인 창 포인터 대상 `popup_pointer_target` 이 덮는 사각형을
    /// 보는 것과 같다). 그 누름을 그리기 전까지 노드는 그 값이고, 그 누름을 그리는 패스는 포커스를 다시 정하지 않는다
    /// (`window_press_decided`).
    pub(crate) fn press_window_at(&mut self, ctx: &egui::Context, pos: egui::Pos2) {
        let hit = ctx.layer_id_at(pos).and_then(|layer| {
            self.popups
                .iter()
                .filter(|p| p.open)
                .find(|p| {
                    let area = super::draw::area_id_of(p.id, p.instance);
                    area == layer.id
                        || area.with(super::draw::COVER_BLOCK) == layer.id
                        || super::child_popup_ids::child_layer_ids(ctx, p.instance)
                            .contains(&layer.id)
                })
                .map(|p| p.instance)
        });
        self.focus_on_press(hit.map(PopupNode::Host));
        let node = hit.map_or(WindowNode::Widget, WindowNode::Popup);
        self.window.press = Some((node, self.window.after.len()));
    }

    /// 지금 그리는 몫의 누름이 사건 입구에서 포커스를 정했는지(`press_window_at`). 그리기는 그 누름의 포커스를 다시
    /// 정하지 않는다.
    pub(crate) fn window_press_decided(&self) -> bool {
        matches!(self.window.press, Some((_, 0)))
    }

    /// 키·IME·누름 사건 입구 맨 앞: 관리자 포커스 표시를 사건 시점 노드에 맞춘다(`show_window_focus`).
    pub(crate) fn show_window_key_node(&mut self, ctx: &egui::Context) {
        let node = self.window_key_node(ctx);
        self.show_window_focus(node);
    }

    /// egui 에 실은 키 사건의 몫을 단계 4 결과(`settle_window_key`)의 egui 수신자 몫으로 묶는다: 팝업이 받으면 그 팝업,
    /// 창 위젯이 받으면 창 위젯이다. egui 수신자가 없는 키는 싣지 않으므로 여기 오지 않는다. 몫 경계도 같은 값으로
    /// 끊는다(`cut_window_segment`). 결과가 없으면(시험 대역) 지금 몫의 노드다.
    pub(crate) fn bind_window_key(&mut self, ctx: &egui::Context) {
        let node = match self
            .window
            .key_receiver
            .take()
            .and_then(WindowReceiver::egui_node)
        {
            Some(node) => node,
            None => match *self.window.open_node() {
                Some(node) => node,
                None => self.window_key_node(ctx),
            },
        };
        self.bind_window_key_node(node.popup().map(PopupNode::Host));
    }

    /// 단계 4(별도 창): 키 사건 하나의 수신 결과를 사건 시점 노드(`window_key_node`)에서 정한다. 경로 키는 메인 창과
    /// 같은 `route_key`(`key_nodes::separate_window_receiver`)가 정하고, 지금 경로로 남는 사건은 사건 시점 노드가 받는다.
    /// 끊기(`cut_window_segment`) 앞에 부르고, 끊기·편집 판정·적재·결속·창 자체 처리가 이 값을 함께 읽는다.
    pub(crate) fn settle_window_key(
        &mut self,
        ctx: &egui::Context,
        key: WindowKey,
    ) -> WindowReceiver {
        use crate::adapters::ui::input::key_route::KeyReceiver;
        let node = self.window_key_node(ctx);
        self.window.key_levels = Some(
            crate::adapters::ui::input::key_nodes::separate_window_levels(
                node.popup().and_then(|iid| self.instance(iid)),
            ),
        );
        let receiver = match key {
            WindowKey::Current => match node {
                WindowNode::Popup(iid) => WindowReceiver::Popup(iid),
                WindowNode::Widget => WindowReceiver::Widget,
            },
            WindowKey::Path { key, own } => {
                let popup = node.popup().and_then(|iid| self.instance(iid));
                let attached =
                    crate::adapters::ui::input::key_nodes::separate_window_attached(self);
                match crate::adapters::ui::input::key_nodes::separate_window_receiver(
                    popup, attached, key, own,
                ) {
                    KeyReceiver::Popup(iid) => WindowReceiver::Popup(iid),
                    KeyReceiver::Window => WindowReceiver::Widget,
                    KeyReceiver::Shortcut { .. } => WindowReceiver::Own,
                    _ => WindowReceiver::Nobody,
                }
            }
        };
        self.window.key_receiver = Some(receiver);
        receiver
    }

    /// 마지막 키 사건 경로의 level 집합(단계 4, `settle_window_key`). 키 사건이 아직 없으면 None 이다.
    pub(crate) fn window_key_levels(
        &self,
    ) -> Option<crate::adapters::ui::input::key_level::PathLevels> {
        self.window.key_levels
    }

    /// 단계 8·11(별도 창): egui 에 실은 bare Esc 누름 하나를 그 수신 결과(`settle_window_key`)로 묶고 적용한다.
    /// 수신자가 팝업이면 그 누름을 `pending`(egui 입력)에서 빼 그 팝업에 넘기고(메인 창과 같은 넘김 기록), 메인 창과 같은
    /// 규칙(`PopupState::receive_escape`)으로 포커스를 푼다. 창 위젯이면 창 위젯 몫으로 묶어 egui 입력에 둔다(창 위젯이
    /// 받는다). egui 수신자가 없는 Esc(창 자체 처리 후보·버림)는 싣지 않으므로 여기 오지 않는다. 시험 대역이 실었으면
    /// `pending` 에서 뺀다. `escape_held` 는 직전 패스 끝의 Escape 눌림 상태다.
    pub(crate) fn apply_window_escape(
        &mut self,
        pending: &mut Vec<egui::Event>,
        escape_held: bool,
    ) {
        let receiver = self
            .window
            .key_receiver
            .take()
            .unwrap_or(WindowReceiver::Widget);
        let iid = match receiver {
            WindowReceiver::Popup(iid) => iid,
            WindowReceiver::Widget => {
                self.bind_window_key_node(None);
                return;
            }
            WindowReceiver::Own | WindowReceiver::Nobody => {
                if let Some(at) = pending.iter().rposition(super::body_keys::is_escape_press) {
                    pending.remove(at);
                }
                return;
            }
        };
        self.bind_window_key_node(Some(PopupNode::Host(iid)));
        if self.instance_mut(iid).is_none_or(|p| p.receive_escape()) {
            self.set_instance_focused(iid, false);
        }
        if let Some(at) = pending.iter().rposition(super::body_keys::is_escape_press) {
            let event = pending.remove(at);
            self.hand_escape_to_bound_owner(escape_held, vec![(at, event)]);
        }
    }
}

#[cfg(test)]
impl PopupManager {
    /// 한 `RawInput` 시험 대역(별도 창 `render`): 지금 egui 포커스를 직전 패스 기록으로 두고, `events` 의 수식키 없는
    /// Esc 누름마다 사건 입구의 단계 11(`apply_window_escape`)을 밟고(그 누름은 입력에서 빠져 넘김 기록이 된다) 이번
    /// `run` 의 결속 없는 몫 키 주인을 W1 값(`owner`)으로 둔다.
    pub(crate) fn raw_pass_keys(
        &mut self,
        ctx: &egui::Context,
        owner: Option<PopupInstanceId>,
        events: &mut Vec<egui::Event>,
    ) {
        // 창은 직전 패스를 닫을 때(`close_window_segment`) 포커스를 기록한다. 대역은 패스를 닫지 않으므로 여기서 읽는다.
        self.window.record = WindowFocusRecord::read(ctx);
        let held = ctx.input(|i| i.key_down(egui::Key::Escape));
        let mut loaded = Vec::with_capacity(events.len());
        for event in std::mem::take(events) {
            let bare = matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none()
            );
            loaded.push(event);
            if bare {
                self.settle_window_key(
                    ctx,
                    WindowKey::Path {
                        key: crate::adapters::ui::input::key_route::RouteKey {
                            key: Some(egui::Key::Escape),
                            modifiers: egui::Modifiers::NONE,
                            super_held: false,
                        },
                        own: false,
                    },
                );
                self.apply_window_escape(&mut loaded, held);
            }
        }
        *events = loaded;
        self.window.run_owner = owner;
    }
}

/// 수식키 Esc 시험(`file_chooser_popup_tests::entered_pass`)이 비-macOS 전용이라 이 대역도 같은 조건이다.
#[cfg(all(test, not(target_os = "macos")))]
impl PopupManager {
    /// 시험 대역: 이번 `run` 의 결속 없는 몫 키 주인을 `owner` 로 둔다.
    pub(crate) fn set_window_run_owner(&mut self, owner: Option<PopupInstanceId>) {
        self.window.run_owner = owner;
    }
}

impl PopupManager {
    /// 별도 창 `run` 전에 결속 없는 몫의 키 주인을 지금 노드로 정한다. egui 가 같은 `run` 안에서 패스를 다시 돌려도 같은
    /// 값을 쓴다(`GpuState::run_window_pass`).
    pub(crate) fn begin_window_run(&mut self, ctx: &egui::Context) {
        let node = self.window_key_node(ctx);
        self.show_window_focus(node);
        self.window.run_owner = node.popup();
    }

    /// 이번 `run` 의 결속 없는 몫 키 주인.
    pub(crate) fn window_run_owner(&self) -> Option<PopupInstanceId> {
        self.window.run_owner
    }

    /// 이번 패스 키 주인을 정한다. 그 몫의 결속이 있으면 그것을, 없으면 `run` 전 노드를 쓴다
    /// (`decide_window_frame_keys`). 관리자를 그리는 함수가 팝업을 그리기 전에 패스마다 부른다. 창 위젯이 이 패스 키를
    /// 받으면(키 주인 없음) 참이다.
    pub(crate) fn window_widget_takes_keys(&mut self, ctx: &egui::Context) -> bool {
        !self
            .decide_window_frame_keys(ctx, self.window.run_owner)
            .held()
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
