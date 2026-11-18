//! 상태 술어가 여닫는 팝업. 술어 목록은 `frame::OPEN_WHEN` 에 있다.

use super::{PopupId, PopupManager, PopupPayload, PopupScope};

/// 상태 술어가 정한 열기: 범위와, 선언 대신 정하는 덮음 값.
#[derive(Debug, Clone, PartialEq)]
pub struct DrivenOpen {
    pub scope: PopupScope,
    pub covers_scope: Option<bool>,
}

impl DrivenOpen {
    /// 선언대로 덮는 열기.
    #[cfg(test)]
    pub fn at(scope: PopupScope) -> Self {
        Self {
            scope,
            covers_scope: None,
        }
    }
}

impl PopupManager {
    /// 상태 술어가 정한 값(`want`)을 따른다. 값이 직전과 같으면 아무것도 하지 않아, 다른 경로로 닫힌
    /// 팝업을 술어가 바뀌기 전까지 다시 열지 않는다. 범위가 생기거나 바뀌면 포커스 없이 연다(정책대로 옮긴다).
    /// 덮음이 바뀌면 다른 모양의 팝업이므로 열린 인스턴스를 닫고 새로 연다. 값이 없어지면 닫는다.
    /// `payload`는 열 때의 인스턴스 데이터다.
    pub fn follow_open_when(
        &mut self,
        id: PopupId,
        want: Option<DrivenOpen>,
        payload: impl FnOnce(&DrivenOpen) -> PopupPayload,
    ) {
        let at = self.driven.iter().position(|(d, _)| *d == id);
        let shown = at.map(|i| self.driven[i].1.clone());
        if shown == want {
            return;
        }
        if let Some(i) = at {
            self.driven.remove(i);
        }
        let Some(open) = want else {
            self.close(id);
            return;
        };
        if shown.is_some_and(|s| s.covers_scope != open.covers_scope) {
            self.close(id);
        }
        let mut request = crate::intent::OpenRequest::new(id, open.scope.clone())
            .centered()
            .with_payload(payload(&open));
        request.covers_scope = open.covers_scope;
        self.open_request(&request, false);
        self.driven.push((id, open));
    }
}
