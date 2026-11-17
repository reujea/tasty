//! debug IPC 가 보는 host 팝업 인스턴스. 같은 선언의 인스턴스를 구분해 시험할 수 있게 한다.

use super::{PopupId, PopupManager, PopupScope, scope_kind};
use serde_json::json;

impl PopupManager {
    /// `id` 의 열린 인스턴스를 아래에서 위 순서로 돌려준다. 범위 대상은 창 범위면 null 이다.
    pub(crate) fn debug_instances(&self, id: PopupId) -> Vec<serde_json::Value> {
        self.instances_of(id)
            .map(|p| {
                let target = match p.scope {
                    PopupScope::Window => None,
                    PopupScope::Workspace(t)
                    | PopupScope::Pane(t)
                    | PopupScope::Tab(t)
                    | PopupScope::Surface(t) => Some(t),
                };
                let r = p.popup_rect();
                json!({
                    "instance": p.instance.0,
                    "scope_kind": scope_kind(&p.scope),
                    "scope_target": target,
                    "z_seq": p.z_seq,
                    "rect": { "x": r.min.x, "y": r.min.y, "w": r.width(), "h": r.height() },
                    "focused": p.focused,
                    "passive": p.passive,
                    "parent": p.parent.map(|parent| parent.0),
                })
            })
            .collect()
    }
}
