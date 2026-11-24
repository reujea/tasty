//! 창의 포커스 위치(팝업 노드 하나)와 passive 전이. 포커스 위치는 창마다 하나다(`PopupManager::focus`).

use super::{PopupId, PopupInstanceId, PopupManager, PopupScope, PopupState};

/// 팝업 노드 하나. 창의 포커스 위치와 창이 입력 시점에 묶은 키 주인이 이 값이다. host 팝업은 인스턴스,
/// plugin 팝업은 plugin 매니저의 instance id 다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupNode {
    Host(PopupInstanceId),
    /// plugin 팝업 instance id.
    Plugin(u64),
}

impl PopupManager {
    /// 이 창의 포커스 위치가 팝업이면 그 노드. 유효한지(열림·보임·경로)는 보지 않는다. 창의 키 경로는 입력마다
    /// 정규화한 값(`MainViewState::normalize_focus`)을 쓴다.
    pub(crate) fn focus(&self) -> Option<PopupNode> {
        self.focus
    }

    /// 포커스 위치를 바꾼다. 포커스 위치를 바꾸는 모든 경로가 이 함수나 아래 함수들을 거친다.
    pub(crate) fn set_focus(&mut self, node: Option<PopupNode>) {
        self.focus = node;
    }

    /// 포커스 위치가 이 노드면 비운다(포커스 위치는 팝업이 아니게 된다).
    pub(crate) fn unfocus_node(&mut self, node: PopupNode) {
        if self.focus == Some(node) {
            self.focus = None;
        }
    }

    /// 이 인스턴스가 창의 포커스 위치인지. 열림·보임은 보지 않는다(포커스 표시용).
    pub(crate) fn is_instance_focused(&self, iid: PopupInstanceId) -> bool {
        self.focus == Some(PopupNode::Host(iid))
    }

    /// 포커스 위치가 열려 있고 범위가 보이는 host 팝업인지.
    pub fn has_focused(&self) -> bool {
        self.focused_host()
            .is_some_and(|p| p.open && p.scope_visible)
    }

    /// 이 선언의 인스턴스가 포커스 위치인지.
    #[cfg(test)]
    pub fn is_focused(&self, id: PopupId) -> bool {
        self.focused_host()
            .is_some_and(|p| p.id == id && p.open && p.scope_visible)
    }

    fn focused_host(&self) -> Option<&PopupState> {
        match self.focus? {
            PopupNode::Host(iid) => self.instance(iid),
            PopupNode::Plugin(_) => None,
        }
    }

    /// Escape 대상인 host 키 주인과 바깥 클릭 시 닫기 여부를 반환한다.
    pub fn focused_dismissal_target(&self) -> Option<(PopupId, PopupInstanceId, bool)> {
        self.host_key_owner()
            .map(|p| (p.id, p.instance, p.close_on_outside_click))
    }

    /// 키를 받을 수 있는 host 인스턴스: 포커스 위치이고 열려 있으며 범위가 보인다. 부모 버튼이 대표하는(그리지
    /// 않는) 자식은 키를 받지 않으므로 빠진다. 창의 키 주인(`MainViewState::popup_key_owner`)의 host 쪽, 다시 접기
    /// 대상, 일반 Esc 대상, 본문에 키를 주는 인스턴스가 모두 이 값이다. 새로 연 팝업은 그리기 전에도 보이는 것으로
    /// 친다.
    pub(crate) fn host_key_owner(&self) -> Option<&super::PopupState> {
        self.focused_host()
            .filter(|p| p.open && p.scope_visible)
            .filter(|p| p.display != super::display::PopupDisplay::Collapsed(None))
    }

    /// 이 선언의 포커스를 바꾼다. 켤 때는 맨 위 인스턴스가 포커스 위치가 되고, 끌 때는 포커스 위치가 이 선언의
    /// 인스턴스면 비운다(검색창↔터미널 포커스 토글용).
    pub fn set_focused(&mut self, id: PopupId, focused: bool) {
        if focused {
            if let Some(iid) = self.topmost_of(id).map(|p| p.instance) {
                self.give_focus(iid);
            }
            return;
        }
        if self.focused_host().is_some_and(|p| p.id == id) {
            self.focus = None;
        }
    }

    /// 인스턴스 하나를 포커스 위치로 하거나, 포커스 위치가 그 인스턴스면 비운다.
    pub fn set_instance_focused(&mut self, iid: PopupInstanceId, focused: bool) {
        if focused {
            self.give_focus(iid);
        } else {
            self.unfocus_node(PopupNode::Host(iid));
        }
    }

    /// 열린 인스턴스 하나를 포커스 위치로 한다(열기·펼침·passive 승격). 포커스 위치는 창에 하나라 다른 팝업은
    /// 저절로 포커스를 잃는다. 포커스를 받지 않는 인스턴스면 포커스 위치를 바꾸지 않는다.
    pub(super) fn give_focus(&mut self, iid: PopupInstanceId) {
        let Some(p) = self
            .popups
            .iter_mut()
            .find(|p| p.open && p.instance == iid)
            .filter(|p| p.arrange.takes_focus)
        else {
            return;
        };
        p.passive = false;
        self.focus = Some(PopupNode::Host(iid));
    }

    /// 누름이 정한 포커스 위치. `target`은 누른 좌표의 포인터 대상인 팝업 노드(상자 또는 덮는 범위)이고, 팝업이
    /// 아니면 `None`이다. 팝업이면 그 팝업이 포커스 위치가 되고, 아니면 포커스 위치는 팝업이 아니게 된다(누른
    /// surface·창 크롬). 포커스를 받지 않는 host 팝업(`keyboard = None`)을 누르면 바꾸지 않는다.
    pub(crate) fn focus_on_press(&mut self, target: Option<PopupNode>) {
        match target {
            Some(PopupNode::Host(iid)) => self.give_focus(iid),
            Some(node @ PopupNode::Plugin(_)) => self.focus = Some(node),
            None => self.focus = None,
        }
    }

    /// 닫힌 인스턴스가 포커스 위치였으면 열린 부모(host 부모, 아니면 plugin 부모)로, 부모가 없으면 비운다.
    pub(super) fn focus_after_close(
        &mut self,
        closed: PopupInstanceId,
        (parent, plugin_parent): (Option<PopupInstanceId>, Option<u64>),
    ) {
        if self.focus != Some(PopupNode::Host(closed)) {
            return;
        }
        self.focus = match parent.filter(|p| self.instance(*p).is_some_and(|p| p.open)) {
            Some(parent) => Some(PopupNode::Host(parent)),
            None => plugin_parent.map(PopupNode::Plugin),
        };
    }

    /// 호스트가 확인한 사용자 입력으로 passive popup을 포커스 위치로 하고 맨 앞으로 올린다. 열린 passive popup이
    /// 보일 때만 바뀐다.
    pub fn focus_passive(&mut self, id: PopupId) -> bool {
        self.focus_passive_where(|p| p.id == id)
    }

    /// `scope` 에 열린 `id` 의 passive 인스턴스를 포커스 위치로 한다. 다른 범위의 같은 팝업은 건드리지 않는다.
    pub fn focus_passive_in(&mut self, id: PopupId, scope: &PopupScope) -> bool {
        self.focus_passive_where(|p| p.id == id && p.scope == *scope)
    }

    fn focus_passive_where(&mut self, target: impl Fn(&PopupState) -> bool) -> bool {
        let Some(iid) = self
            .popups
            .iter()
            .rev()
            .find(|p| target(p) && p.open && p.scope_visible && p.passive)
            .map(|p| p.instance)
        else {
            return false;
        };
        self.give_focus(iid);
        self.bring_to_front(iid);
        true
    }

    /// 이 순번의 열린 host 인스턴스.
    pub(crate) fn open_with_z(&self, z: u64) -> Option<PopupInstanceId> {
        self.popups
            .iter()
            .find(|p| p.open && p.z_seq == z)
            .map(|p| p.instance)
    }

    /// 범위를 덮는 보이는 host 인스턴스와 그 배치 범위·순번. 포커스를 받지 않는 인스턴스는 포커스 노드가 될 수
    /// 없으므로 빠진다. 부모 버튼이 대표하는(그리지 않는) 자식도 빠진다. 그 자식이 덮는 노드로 남으면 포커스를
    /// 보기 버튼으로 넘긴 뒤 정규화가 포커스를 숨은 자식에게 되돌려 키를 받는 팝업이 없어진다.
    pub(crate) fn covering_nodes(
        &self,
    ) -> impl Iterator<Item = (PopupInstanceId, PopupScope, u64)> + '_ {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.covers() && p.arrange.takes_focus)
            .filter(|p| p.display != super::display::PopupDisplay::Collapsed(None))
            .map(|p| (p.instance, p.placement_scope(), p.z_seq))
    }

    /// 범위를 덮지 않는 보이는 host 인스턴스와 그 배치 범위·순번·Esc 선언. 범위 노드에 붙는 팝업의 원천이다
    /// (`key_nodes::window_key_path`). 부모 버튼이 대표하는(그리지 않는) 자식은 빠진다.
    pub(crate) fn attached_nodes(
        &self,
    ) -> impl Iterator<Item = (PopupInstanceId, PopupScope, u64, super::PopupEscape)> + '_ {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && !p.covers())
            .filter(|p| p.display != super::display::PopupDisplay::Collapsed(None))
            .map(|p| (p.instance, p.placement_scope(), p.z_seq, p.escape_decl()))
    }

    /// 이 인스턴스가 지금 화면에 보이는 passive popup인지.
    pub fn instance_is_visible_passive(&self, iid: PopupInstanceId) -> bool {
        self.instance(iid)
            .is_some_and(|p| p.scope_visible && p.passive)
    }

    /// 지금 화면에 보이는 passive popup인지. 범위 밖에 숨은 popup은 올릴 수 없다.
    #[cfg(test)]
    pub fn is_visible_passive(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.scope_visible && p.passive)
    }

    /// 포커스 위치인 열린 popup ID. Linux 탐색기 IME가 rename 입력의 소유를 판정할 때 쓴다.
    #[cfg(target_os = "linux")]
    pub fn keyboard_focused_ids(&self) -> Vec<PopupId> {
        self.focused_host()
            .filter(|p| p.open && p.scope_visible)
            .map(|p| p.id)
            .into_iter()
            .collect()
    }
}
