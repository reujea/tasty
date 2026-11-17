//! 상태 술어가 여닫는 팝업. 술어 목록은 `frame::OPEN_WHEN` 에 있다.

use super::{PopupId, PopupManager, PopupScope};

impl PopupManager {
    /// 상태 술어가 정한 범위(`want`)를 따른다. 값이 직전과 같으면 아무것도 하지 않아, 다른 경로로 닫힌
    /// 팝업을 술어가 바뀌기 전까지 다시 열지 않는다. 범위가 생기거나 바뀌면 포커스 없이 연다(정책대로 옮긴다).
    /// 범위가 없어지면 닫는다.
    pub fn follow_open_when(&mut self, id: PopupId, want: Option<PopupScope>) {
        let at = self.driven.iter().position(|(d, _)| *d == id);
        let shown = at.map(|i| self.driven[i].1.clone());
        if shown == want {
            return;
        }
        if let Some(i) = at {
            self.driven.remove(i);
        }
        let Some(scope) = want else {
            self.close(id);
            return;
        };
        self.open_request(
            &crate::intent::OpenRequest::new(id, scope.clone()).centered(),
            false,
        );
        self.driven.push((id, scope));
    }
}
