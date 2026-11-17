//! 인스턴스가 여럿일 수 있는 팝업의 헤더 이동 손잡이. 뷰가 그린 헤더 영역을 인스턴스별로 보고하고
//! hit-test가 그 인스턴스의 값을 읽는다. 선언에 인스턴스가 하나뿐인 팝업은 `report_header_drag_rect`를 쓴다.

use super::PopupInstanceId;

fn rect_id(instance: PopupInstanceId) -> egui::Id {
    egui::Id::new("popup.instance_header_drag_rect").with(instance.0)
}

/// 뷰가 그린 헤더 영역을 인스턴스 ID로 보고한다. 매 프레임 보고해야 한다.
pub fn report(ctx: &egui::Context, instance: PopupInstanceId, rect: egui::Rect) {
    ctx.memory_mut(|m| m.data.insert_temp(rect_id(instance), rect));
}

/// 인스턴스가 보고한 헤더 영역. 아직 보고 전이면 None.
pub(crate) fn reported(ctx: &egui::Context, instance: PopupInstanceId) -> Option<egui::Rect> {
    ctx.memory(|m| m.data.get_temp(rect_id(instance)))
}

/// 인스턴스가 보고한 헤더 영역, 없으면 선언 id로 보고한 헤더 영역.
pub(super) fn reported_for(
    ctx: &egui::Context,
    instance: PopupInstanceId,
    id: super::PopupId,
) -> Option<egui::Rect> {
    reported(ctx, instance).or_else(|| super::reported_header_drag_rect(ctx, id))
}

#[cfg(test)]
mod tests {
    use super::super::{PopupInstances, PopupManager, PopupScope, PopupState};
    use super::*;

    /// Given 같은 선언의 피커 둘이 다른 위치에 헤더를 보고했을 때, Then 각 인스턴스의 이동 손잡이는 자기 헤더다.
    #[test]
    fn each_instance_drags_by_its_own_reported_header() {
        let id = "header_drag_test";
        let mut mgr = PopupManager::new();
        mgr.register(
            PopupState::new(id, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
                .with_instances(PopupInstances::PerScopeTarget),
        );
        let a = mgr.open(id, PopupScope::Surface(1)).unwrap();
        let b = mgr.open(id, PopupScope::Surface(2)).unwrap();
        let ctx = egui::Context::default();
        let rect_a = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 20.0));
        let rect_b = egui::Rect::from_min_size(egui::pos2(500.0, 300.0), egui::vec2(100.0, 20.0));
        report(&ctx, a, rect_a);
        report(&ctx, b, rect_b);
        let handle = |iid| mgr.instance(iid).unwrap().effective_drag_handle_rect(&ctx);
        assert_eq!(handle(a), Some(rect_a));
        assert_eq!(handle(b), Some(rect_b));
    }
}
