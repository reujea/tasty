//! 팝업의 egui 레이어·Area ID는 인스턴스로 정해진다. 순서가 바뀌거나 이웃이 닫혀도 남은 인스턴스의
//! ID가 그대로이고, 화면 순서는 매니저의 z 순서를 따른다.

use super::super::{PopupInstanceId, PopupInstances, PopupManager, PopupScope, PopupState};

const ID: &str = "area_id_test";

fn frame(ctx: &egui::Context, mgr: &mut PopupManager) -> Vec<egui::LayerId> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let mut layers = Vec::new();
    drop(ctx.run(raw, |ctx| {
        layers = mgr.draw(ctx, &mut |_, _| {}, None, &[]).layers;
    }));
    layers
}

fn top_layer_at(ctx: &egui::Context, mgr: &PopupManager, iid: PopupInstanceId) -> egui::LayerId {
    let at = mgr.instance(iid).unwrap().content_rect().center();
    ctx.layer_id_at(at).expect("a layer under the popup")
}

/// Given 같은 선언 A·B가 겹쳐 보일 때, When A를 맨 앞으로 올리면, Then 두 레이어 ID는 그대로이고 A가 위다.
/// When A를 닫으면, Then B의 레이어 ID는 그대로다.
#[test]
fn reordering_or_closing_keeps_the_other_instance_area_id() {
    let mut mgr = PopupManager::new();
    mgr.register(
        PopupState::new(ID, "test", egui::vec2(200.0, 100.0), PopupScope::Window)
            .with_instances(PopupInstances::PerScopeTarget),
    );
    let a = mgr.open(ID, PopupScope::Surface(1)).unwrap();
    mgr.open(ID, PopupScope::Surface(2)).unwrap();
    let ctx = egui::Context::default();
    let first = frame(&ctx, &mut mgr);
    let (layer_a, layer_b) = (first[0], first[1]);
    assert_ne!(layer_a, layer_b, "each instance has its own layer");
    assert_eq!(top_layer_at(&ctx, &mgr, a), layer_b, "B was opened last");

    mgr.bring_to_front(a);
    assert_eq!(frame(&ctx, &mut mgr), [layer_b, layer_a]);
    assert_eq!(top_layer_at(&ctx, &mgr, a), layer_a, "A is drawn on top");

    mgr.close_instance(a, super::super::PopupCloseReason::Command);
    assert_eq!(frame(&ctx, &mut mgr), [layer_b]);
}
