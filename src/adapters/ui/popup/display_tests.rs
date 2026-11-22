//! 좁은 범위 팝업의 접기·펼침 전이를 관리자 층에서 확인한다. L(surface 1)과 R(surface 2)을 나란히 둔다.

use super::super::*;
use super::PopupDisplay;
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::layout_context::SurfaceOwner;
use crate::intent::OpenRequest;

const POP: PopupId = "display_test";
const CHILD: PopupId = "display_test_child";
const L: u32 = 1;
const R: u32 = 2;

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
}

/// R의 폭이 `r_width`인 배치. L이 나머지를 쓴다.
fn layout(r_width: f32) -> LayoutContext {
    let split = 800.0 - r_width;
    let l = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(split, 600.0));
    let r = egui::Rect::from_min_max(egui::pos2(split, 0.0), egui::pos2(800.0, 600.0));
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![(10, l), (20, r)],
        surface_rects: vec![(L, l), (R, r)],
        active_tabs: vec![],
        active_tab_ids: vec![(10, 100), (20, 200)],
        workspace_rect: Some(screen()),
        surface_owners: vec![
            SurfaceOwner {
                surface: L,
                tab: 100,
                pane: 10,
            },
            SurfaceOwner {
                surface: R,
                tab: 200,
                pane: 20,
            },
        ],
        tab_content_rects: vec![(10, l), (20, r)],
    }
}

fn popup(id: PopupId, size: egui::Vec2, covers: bool, headless: bool) -> PopupState {
    let mut p = PopupState::new(id, "t", size, PopupScope::Window)
        .with_instances(PopupInstances::PerScopeTarget);
    p.arrange.fit = PopupFit::Collapse;
    p.look = p.look.covering(covers);
    p.declared_look = p.look;
    p.headless = headless;
    p
}

fn manager(covers: bool, headless: bool) -> PopupManager {
    let mut mgr = PopupManager::new();
    mgr.register(popup(POP, egui::vec2(300.0, 200.0), covers, headless));
    mgr.register(popup(CHILD, egui::vec2(260.0, 160.0), true, false));
    mgr
}

fn open_in_r(mgr: &mut PopupManager) -> PopupInstanceId {
    mgr.open_request(
        &OpenRequest::new(POP, PopupScope::Surface(R)).focused(),
        true,
    )
    .unwrap()
}

/// 한 프레임 그린다. 그린 콘텐츠의 id를 돌려준다.
fn frame(
    mgr: &mut PopupManager,
    ctx: &egui::Context,
    r_width: f32,
    events: Vec<egui::Event>,
) -> (Vec<PopupId>, PopupDrawResult) {
    let layout = layout(r_width);
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let mut drawn = Vec::new();
    let mut result = None;
    drop(ctx.run(raw, |ctx| {
        result = Some(mgr.draw(ctx, &mut |cx, _| drawn.push(cx.id), Some(&layout), &[]));
    }));
    (drawn, result.unwrap())
}

fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(pos), button(true), button(false)]
}

fn display(mgr: &PopupManager, iid: PopupInstanceId) -> PopupDisplay {
    mgr.instance(iid).unwrap().display
}

/// 접힌 팝업의 보기 버튼을 누르고 한 프레임 더 돌려 펼친다.
fn press_show_button(mgr: &mut PopupManager, ctx: &egui::Context, iid: PopupInstanceId, w: f32) {
    let at = mgr
        .instance(iid)
        .unwrap()
        .collapsed_frame()
        .unwrap()
        .rect
        .center();
    // egui는 직전 프레임의 hover로 누름 대상을 정하므로 포인터를 먼저 옮긴다.
    frame(mgr, ctx, w, vec![egui::Event::PointerMoved(at)]);
    frame(mgr, ctx, w, click(at));
    frame(mgr, ctx, w, vec![]);
}

/// Given R이 팝업보다 좁을 때, When 그리면, Then 접혀 보기 버튼 프레임이 R 안에 있고 콘텐츠는 그리지 않는다.
/// When R을 넓히면, Then 다시 범위 안에 그대로 그린다.
#[test]
fn a_popup_wider_than_its_surface_collapses_and_returns_when_it_fits() {
    let mut mgr = manager(true, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    let (drawn, result) = frame(&mut mgr, &ctx, 200.0, vec![]);
    let frame_rect = mgr.instance(iid).unwrap().collapsed_frame().unwrap().rect;
    assert!(layout(200.0).surface_rects[1].1.contains_rect(frame_rect));
    assert!(drawn.is_empty(), "접힌 팝업은 콘텐츠를 그리지 않는다");
    assert_eq!(
        result.hit_rects.iter().map(|o| o.rect).collect::<Vec<_>>(),
        [frame_rect]
    );
    let (drawn, _) = frame(&mut mgr, &ctx, 500.0, vec![]);
    assert_eq!(display(&mgr, iid), PopupDisplay::Fits);
    assert_eq!(drawn, [POP]);
}

/// Given 접힌 팝업, When 보기 버튼을 누르면, Then 창 위로 펼쳐 배치 범위가 Window가 되고 창 전체를 덮으며
/// 포커스를 갖는다. 논리 범위는 R 그대로다. When R을 넓혀도 펼친 채다.
#[test]
fn the_show_button_expands_the_popup_over_the_window() {
    let mut mgr = manager(false, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, iid, 200.0);
    let p = mgr.instance(iid).unwrap();
    assert!(p.expanded() && mgr.is_instance_focused(iid));
    assert_eq!(p.placement_scope(), PopupScope::Window);
    assert_eq!(p.scope, PopupScope::Surface(R));
    let (drawn, result) = frame(&mut mgr, &ctx, 500.0, vec![]);
    assert!(mgr.instance(iid).unwrap().expanded());
    assert_eq!(drawn, [POP]);
    assert_eq!(
        result.covers.iter().map(|c| c.rect).collect::<Vec<_>>(),
        [screen()],
        "덮지 않는 팝업도 펼치면 창을 덮는다"
    );
    let rect = mgr.instance(iid).unwrap().popup_rect();
    assert_eq!(rect.center(), screen().center());
}

/// Given 펼친 팝업, When Esc(다시 접기)를 하면, Then 닫히지 않고 포커스·인스턴스가 그대로이며 범위 판정으로
/// 돌아간다(좁으면 접힘, 넓으면 그대로).
#[test]
fn escape_folds_an_expanded_popup_back_without_closing_it() {
    for (width, collapsed) in [(200.0, true), (500.0, false)] {
        let mut mgr = manager(true, false);
        let iid = open_in_r(&mut mgr);
        let ctx = egui::Context::default();
        frame(&mut mgr, &ctx, 200.0, vec![]);
        press_show_button(&mut mgr, &ctx, iid, 200.0);
        assert!(mgr.refold(iid));
        let (_, result) = frame(&mut mgr, &ctx, width, vec![]);
        assert!(result.closed.is_empty());
        assert!(mgr.take_closed_queue().is_empty());
        let p = mgr.instance(iid).unwrap();
        assert!(p.open && mgr.is_instance_focused(iid));
        assert_eq!(p.collapsed(), collapsed, "{width}");
        assert!(!mgr.refold(iid), "펼치지 않은 팝업은 다시 접을 것이 없다");
    }
}

/// Given 펼친 팝업, When 제목줄의 접기 버튼을 누르면, Then 다시 접힌다.
#[test]
fn the_shrink_button_folds_the_popup_back() {
    let mut mgr = manager(true, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, iid, 200.0);
    frame(&mut mgr, &ctx, 200.0, vec![]);
    let shrink = mgr.instance(iid).unwrap().collapse_btn_rect().unwrap();
    let close = mgr.instance(iid).unwrap().close_btn_rect();
    assert!(shrink.max.x < close.min.x, "접기 버튼은 닫기 왼쪽이다");
    frame(&mut mgr, &ctx, 200.0, click(shrink.center()));
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert!(mgr.instance(iid).unwrap().collapsed());
}

/// Given 키 주인인 접힌 팝업, When Enter를 누르면, Then 펼친다.
#[test]
fn enter_on_the_focused_show_button_expands() {
    let mut mgr = manager(true, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    frame(
        &mut mgr,
        &ctx,
        200.0,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(mgr.instance(iid).unwrap().expanded());
}

/// Given 덮는 팝업이 R에서 접혀 있을 때, When R 범위 안·프레임 밖을 누르면, Then 그 팝업 것이다(덮음 유지).
/// When L을 누르면, Then 팝업은 그대로다.
#[test]
fn a_collapsed_covering_popup_still_covers_its_scope() {
    let mut mgr = manager(true, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    let (_, result) = frame(&mut mgr, &ctx, 200.0, vec![]);
    assert_eq!(
        result.covers.iter().map(|c| c.rect).collect::<Vec<_>>(),
        [layout(200.0).surface_rects[1].1]
    );
    let (_, result) = frame(&mut mgr, &ctx, 200.0, click(egui::pos2(100.0, 300.0)));
    assert!(!result.hovered);
    assert!(mgr.instance(iid).unwrap().collapsed());
}

/// Given 덮는 팝업이 R에서 접혀 있을 때, When R에 불투명 모드 페이지가 보이면, Then 펼친 갈래처럼 scrim을
/// 칠하지 않는다. 페이지가 없으면 칠한다.
#[test]
fn a_collapsed_popup_skips_its_scrim_over_an_opaque_page_like_a_drawn_one() {
    let mut mgr = manager(true, false);
    open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    let r = layout(200.0).surface_rects[1].1;
    let (_, result) = frame(&mut mgr, &ctx, 200.0, vec![]);
    assert_eq!(result.overlay_targets.len(), 1);
    assert_eq!(result.overlay_targets[0].scrim, Some(r));
    tasty_egui_theme::overlay_edge::set_opaque_pages(&ctx, vec![r.shrink(10.0)]);
    let (_, result) = frame(&mut mgr, &ctx, 200.0, vec![]);
    assert_eq!(result.overlay_targets.len(), 1);
    assert_eq!(result.overlay_targets[0].scrim, None);
}

/// Given 아이콘 프레임보다 좁은 R에 접힌 팝업, When 그리면, Then 포인터 판정에 넘기는 상자는 R로 자른 버튼
/// 프레임이다. 그리지 않는 R 밖 부분은 L의 포인터를 막지 않는다.
#[test]
fn the_pointer_box_of_a_collapsed_popup_is_clipped_to_its_scope() {
    let mut mgr = manager(false, false);
    open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    let r = layout(20.0).surface_rects[1].1;
    let (_, result) = frame(&mut mgr, &ctx, 20.0, vec![]);
    assert_eq!(result.hit_rects.len(), 1);
    let hit = result.hit_rects[0].rect;
    assert!(r.contains_rect(hit), "{hit:?} in {r:?}");
    assert!(hit.width() < 38.0);
}

/// Given 자기 제목줄이 없는 팝업, When 펼치면, Then 접기 버튼만 담은 host 제목줄을 받아 그만큼 커진다.
#[test]
fn a_headless_popup_gets_a_host_bar_when_expanded() {
    let mut mgr = manager(true, true);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, iid, 200.0);
    let p = mgr.instance(iid).unwrap();
    assert!(p.has_title_bar());
    assert_eq!(p.collapse_btn_rect(), Some(p.close_btn_rect()));
    assert_eq!(p.size.y, 200.0 + title_bar_height().value());
}

/// Given 펼친 부모가 연 자식, Then 자식도 펼친다. When 자식에서 Esc를 하면, Then 묶음의 뿌리(부모)부터 접힌다.
#[test]
fn a_child_of_an_expanded_parent_expands_and_folds_with_it() {
    let mut mgr = manager(true, false);
    let parent = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, parent, 200.0);
    let child = mgr
        .open_request(
            &OpenRequest {
                parent: Some(parent),
                ..OpenRequest::new(CHILD, PopupScope::Surface(R)).focused()
            },
            true,
        )
        .unwrap();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert!(mgr.instance(child).unwrap().expanded());
    assert!(mgr.refold(child));
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert!(mgr.instance(parent).unwrap().collapsed());
    assert!(mgr.instance(child).unwrap().collapsed());
    // 그리지 않는 자식은 키를 갖지 않는다. 키 주인은 보이는 부모 보기 버튼이다.
    assert!(!mgr.is_instance_focused(child));
    assert!(mgr.is_instance_focused(parent));
    assert_eq!(
        mgr.focused_dismissal_target().map(|(_, iid, _)| iid),
        Some(parent)
    );
}

/// Given 범위에 들어가는 부모가 연 큰 자식, Then 자식만 접힌다. When 자식을 펼쳤다 Esc를 하면, Then 자식만
/// 다시 접히고 부모는 그대로다(N1).
#[test]
fn a_child_too_big_for_a_fitting_parent_folds_alone() {
    let mut mgr = PopupManager::new();
    mgr.register(popup(POP, egui::vec2(150.0, 100.0), true, false));
    mgr.register(popup(CHILD, egui::vec2(300.0, 200.0), true, false));
    let parent = open_in_r(&mut mgr);
    let child = mgr
        .open_request(
            &OpenRequest {
                parent: Some(parent),
                ..OpenRequest::new(CHILD, PopupScope::Surface(R)).focused()
            },
            true,
        )
        .unwrap();
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert_eq!(display(&mgr, parent), PopupDisplay::Fits);
    assert!(mgr.instance(child).unwrap().collapsed());
    press_show_button(&mut mgr, &ctx, child, 200.0);
    assert!(mgr.instance(child).unwrap().expanded());
    assert_eq!(display(&mgr, parent), PopupDisplay::Fits);
    assert!(mgr.refold(child));
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert!(mgr.instance(child).unwrap().collapsed());
    assert_eq!(display(&mgr, parent), PopupDisplay::Fits);
}

/// Given 범위에 맞춰 줄이는 팝업(검색창)과 창 범위 팝업, Then 좁아도 접지 않는다.
#[test]
fn shrinking_and_window_popups_never_collapse() {
    let mut mgr = manager(true, false);
    mgr.popups
        .iter_mut()
        .for_each(|p| p.arrange.fit = PopupFit::Shrink);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert_eq!(display(&mgr, iid), PopupDisplay::Fits);

    let mut mgr = manager(true, false);
    let iid = mgr
        .open_request(&OpenRequest::new(POP, PopupScope::Window).focused(), true)
        .unwrap();
    let small = egui::Context::default();
    frame(&mut mgr, &small, 200.0, vec![]);
    assert_eq!(display(&mgr, iid), PopupDisplay::Fits);
}

/// Given 펼친 팝업, When 닫고 다시 열면, Then 처음부터 판정한다(펼친 상태가 남지 않는다).
#[test]
fn reopening_starts_from_the_fit_check() {
    let mut mgr = manager(true, false);
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, iid, 200.0);
    mgr.close(POP);
    let again = open_in_r(&mut mgr);
    frame(&mut mgr, &ctx, 200.0, vec![]);
    assert!(mgr.instance(again).unwrap().collapsed());
}

/// Given 창보다 큰 팝업, When 펼치면, Then 프레임은 창 안에 들어가 접기·닫기 버튼이 창 안이고, 내용은 필요 크기를
/// 유지해 넘친 만큼만 잘린다(사용자 결정 2026-11-19).
#[test]
fn an_expanded_popup_larger_than_the_window_fits_its_frame_and_keeps_its_content_size() {
    let mut mgr = PopupManager::new();
    let needed = egui::vec2(1000.0, 700.0);
    mgr.register(popup(POP, needed, true, false));
    let iid = open_in_r(&mut mgr);
    let ctx = egui::Context::default();
    frame(&mut mgr, &ctx, 200.0, vec![]);
    press_show_button(&mut mgr, &ctx, iid, 200.0);
    frame(&mut mgr, &ctx, 200.0, vec![]);
    let p = mgr.instance(iid).unwrap();
    let rect = p.popup_rect();
    assert!(screen().contains_rect(rect), "{rect:?}");
    assert!(screen().contains_rect(p.close_btn_rect()));
    assert!(screen().contains_rect(p.collapse_btn_rect().unwrap()));
    assert_eq!(p.needed_size(), needed);
    assert_eq!(rect.size() + p.expanded_overflow(), needed);
    assert!(p.expanded_overflow().x > 0.0 || p.expanded_overflow().y > 0.0);
}
