//! 인스턴스의 키보드 포커스와 passive 전이.

use super::{PopupInstanceId, PopupManager};

impl PopupManager {
    /// 열린 인스턴스 하나에 키보드 포커스를 준다. 포커스를 주는 모든 경로(열기·팝업 안 클릭·passive 승격)가
    /// 이 함수를 거친다. 같은 선언의 다른 인스턴스는 숨은 범위에 있어도 포커스를 푼다. 숨은 형제가 포커스를
    /// 가진 채 범위가 다시 보이면 두 인스턴스가 함께 키를 읽기 때문이다. `exclusive`이면 보이는 다른 선언의
    /// 포커스도 푼다(팝업 안을 누른 것과 같은 전이).
    pub(super) fn give_focus(&mut self, iid: PopupInstanceId, exclusive: bool) {
        let Some(id) = self.instance(iid).map(|p| p.id) else {
            return;
        };
        for p in &mut self.popups {
            if p.open && p.instance == iid {
                p.focused = true;
                p.passive = false;
            } else if p.id == id || (exclusive && p.scope_visible) {
                p.focused = false;
            }
        }
    }
}
