//! 인스턴스의 키보드 포커스와 passive 전이.

use super::route::ScopePath;
use super::{PopupId, PopupInstanceId, PopupManager, PopupScope, PopupState};

impl PopupManager {
    /// Check if any popup currently has keyboard focus.
    pub fn has_focused(&self) -> bool {
        self.popups
            .iter()
            .any(|p| p.open && p.scope_visible && p.focused)
    }

    /// 이 선언의 인스턴스 하나라도 키보드 포커스를 가졌는지.
    #[cfg(test)]
    pub fn is_focused(&self, id: PopupId) -> bool {
        self.popups
            .iter()
            .any(|p| p.id == id && p.open && p.scope_visible && p.focused)
    }

    /// Escape 대상인 포커스된 인스턴스 하나와 바깥 클릭 시 닫기 여부를 반환한다. 포커스된 인스턴스가
    /// 여럿이면 맨 위 것이다. Escape에는 좌표가 없으므로 바깥 클릭처럼 여러 팝업을 한꺼번에 닫지 않는다.
    pub fn focused_dismissal_target(&self) -> Option<(PopupId, PopupInstanceId, bool)> {
        self.host_key_owner()
            .map(|p| (p.id, p.instance, p.close_on_outside_click))
    }

    /// 키를 받을 수 있는 맨 위 host 인스턴스: 열려 있고 범위가 보이며 포커스를 가졌다. 포커스 없는(passive 포함)
    /// 인스턴스와 부모 버튼이 대표하는(그리지 않는) 자식은 키를 받지 않으므로 빠진다. 창의 키 주인
    /// (`MainViewState::popup_key_owner`)의 host 후보, 다시 접기 대상, 일반 Esc 대상, 본문에 키를 주는 인스턴스가
    /// 모두 이 값이다. 새로 연 팝업은 그리기 전에도 보이는 것으로 친다.
    pub(crate) fn host_key_owner(&self) -> Option<&super::PopupState> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.focused)
            .filter(|p| p.display != super::display::PopupDisplay::Collapsed(None))
            .max_by_key(|p| p.z_seq)
    }

    /// [`Self::host_key_owner`]의 순번.
    pub(crate) fn host_key_owner_z(&self) -> Option<u64> {
        self.host_key_owner().map(|p| p.z_seq)
    }

    /// 이 선언의 키보드 포커스 플래그를 바꾼다. 켤 때는 맨 위 인스턴스에만, 끌 때는 모든 인스턴스에서 끈다.
    /// 다른 선언의 포커스는 건드리지 않는다 (검색창↔터미널 포커스 토글용).
    pub fn set_focused(&mut self, id: PopupId, focused: bool) {
        if focused {
            if let Some(iid) = self.topmost_of(id).map(|p| p.instance) {
                self.set_instance_focused(iid, true);
            }
            return;
        }
        for p in self.popups.iter_mut().filter(|p| p.id == id) {
            p.focused = false;
        }
    }

    /// 인스턴스 하나의 키보드 포커스 플래그를 바꾼다. 켤 때는 다른 팝업의 포커스를 바꾸지 않는다.
    pub fn set_instance_focused(&mut self, iid: PopupInstanceId, focused: bool) {
        if focused {
            self.give_focus(iid, None);
        } else if let Some(p) = self.instance_mut(iid) {
            p.focused = false;
        }
    }

    /// 열린 인스턴스 하나에 키보드 포커스를 준다. 포커스를 주는 모든 경로(열기·팝업 안 클릭·passive 승격)가
    /// 이 함수를 거친다. `path`를 주면 그 경로 위에 놓인 보이는 다른 팝업의 포커스를 푼다(팝업 안을 누른 것과
    /// 같은 전이). 경로 밖 범위의 팝업은 같은 선언의 인스턴스여도 건드리지 않는다. 키는 포커스를 가진 맨 위
    /// 인스턴스 하나(키 주인)가 받는다. 포커스를 받지 않는 인스턴스에는 주지 않고 다른 팝업의 포커스도 바꾸지
    /// 않는다.
    pub(super) fn give_focus(&mut self, iid: PopupInstanceId, path: Option<&ScopePath>) {
        if !self.instance(iid).is_some_and(|p| p.arrange.takes_focus) {
            return;
        }
        for p in &mut self.popups {
            if p.open && p.instance == iid {
                p.focused = true;
                p.passive = false;
            } else if path
                .is_some_and(|path| p.scope_visible && path.contains(&p.placement_scope()))
            {
                p.focused = false;
            }
        }
    }

    /// 호스트가 확인한 사용자 입력으로 passive popup에 포커스를 준다. 팝업 안을 누른 것과 같은 전이다:
    /// 그 팝업 범위 경로(`path_of`가 계산) 위의 보이는 다른 팝업 포커스를 풀고 맨 앞으로 올린다. 열린 passive
    /// popup이 보일 때만 바뀐다.
    pub fn focus_passive(
        &mut self,
        id: PopupId,
        path_of: &dyn Fn(&PopupScope) -> ScopePath,
    ) -> bool {
        self.focus_passive_where(|p| p.id == id, path_of)
    }

    /// `scope` 에 열린 `id` 의 passive 인스턴스에 포커스를 준다. 다른 범위의 같은 팝업은 건드리지 않는다.
    pub fn focus_passive_in(
        &mut self,
        id: PopupId,
        scope: &PopupScope,
        path_of: &dyn Fn(&PopupScope) -> ScopePath,
    ) -> bool {
        self.focus_passive_where(|p| p.id == id && p.scope == *scope, path_of)
    }

    fn focus_passive_where(
        &mut self,
        target: impl Fn(&PopupState) -> bool,
        path_of: &dyn Fn(&PopupScope) -> ScopePath,
    ) -> bool {
        let Some((iid, path)) = self
            .popups
            .iter()
            .rev()
            .find(|p| target(p) && p.open && p.scope_visible && p.passive)
            .map(|p| (p.instance, path_of(&p.scope)))
        else {
            return false;
        };
        self.give_focus(iid, Some(&path));
        self.bring_to_front(iid);
        true
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

    /// 키보드 포커스를 가진 열린 popup ID. Linux 탐색기 IME가 rename 입력의 소유를 판정할 때 쓴다.
    #[cfg(target_os = "linux")]
    pub fn keyboard_focused_ids(&self) -> Vec<PopupId> {
        self.popups
            .iter()
            .filter(|p| p.open && p.scope_visible && p.focused)
            .map(|p| p.id)
            .collect()
    }
}
