//! 선언이 정하는 배치·포커스 규칙을 시험용 정의로 확인한다. 이 규칙을 쓰는 host 팝업이 없어도 동작은 같다.

use super::*;
use crate::intent::OpenRequest;

const NORMAL: PopupId = "arrange_test_normal";
const QUIET: PopupId = "arrange_test_quiet";
const GRIP: PopupId = "arrange_test_grip";
const NOTICE: PopupId = "arrange_test_notice";
const TOP: PopupId = "arrange_test_top";

fn decl(edit: impl FnOnce(&mut PopupDecl)) -> PopupDecl {
    let mut d = defs::host_decl(popup_size(egui::vec2(200.0, 100.0)));
    edit(&mut d);
    d
}

fn manager() -> PopupManager {
    let mut mgr = PopupManager::new();
    let decls = [
        (NORMAL, decl(|_| {})),
        (QUIET, decl(|d| d.keyboard = PopupKeyboard::None)),
        (
            GRIP,
            decl(|d| {
                d.resizable = true;
                d.resize_handle = PopupResizeHandle::Grip {
                    size: LogicalPx(16.0),
                };
            }),
        ),
        (
            NOTICE,
            decl(|d| {
                d.keyboard = PopupKeyboard::None;
                d.anchor = PopupAnchor::Column;
                d.instances = PopupInstances::Multi;
            }),
        ),
        (TOP, decl(|d| d.anchor = PopupAnchor::ScopeTop)),
    ];
    for (id, d) in decls {
        mgr.register(PopupState::declared(
            id,
            "test",
            &d,
            None,
            PopupScope::Window,
        ));
    }
    mgr
}

fn open_at(mgr: &mut PopupManager, id: PopupId, pos: egui::Pos2) -> PopupInstanceId {
    mgr.open_request(
        &OpenRequest::new(id, PopupScope::Window).at(pos).focused(),
        true,
    )
    .expect("opens")
}

enum Pointer {
    Press(egui::Pos2),
    MoveTo(egui::Pos2),
    Release(egui::Pos2),
    Idle,
}

fn frame(ctx: &egui::Context, mgr: &mut PopupManager, pointer: Pointer) -> PopupDrawResult {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    match pointer {
        Pointer::Press(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, true));
        }
        Pointer::MoveTo(pos) => raw.events.push(egui::Event::PointerMoved(pos)),
        Pointer::Release(pos) => {
            raw.events.push(egui::Event::PointerMoved(pos));
            raw.events.push(button(pos, false));
        }
        Pointer::Idle => {}
    }
    let mut result = None;
    drop(ctx.run(raw, |ctx| {
        result = Some(mgr.draw(ctx, &mut |_, _| {}, None, &[]));
    }));
    result.expect("drawn")
}

/// Given 포커스를 가진 보통 팝업, When 키를 받지 않는 팝업을 사용자가 포커스를 주며 열고 그 안을 누르면,
/// Then 키를 받지 않는 팝업은 포커스를 받지 않고 보통 팝업의 포커스도 그대로다. 나중에 열었어도 보통 팝업
/// 아래에 그리고, Esc 주인은 보통 팝업이다.
#[test]
fn a_no_focus_popup_does_not_take_focus_on_click_or_raise() {
    let mut mgr = manager();
    let normal = open_at(&mut mgr, NORMAL, egui::pos2(100.0, 100.0));
    let quiet = open_at(&mut mgr, QUIET, egui::pos2(500.0, 500.0));
    assert!(mgr.is_focused(NORMAL));
    assert!(!mgr.is_focused(QUIET));

    let ctx = egui::Context::default();
    frame(&ctx, &mut mgr, Pointer::Idle);
    let drawn = frame(&ctx, &mut mgr, Pointer::Press(egui::pos2(600.0, 560.0)));

    assert!(drawn.hovered, "the press landed on the quiet popup");
    assert!(mgr.is_focused(NORMAL));
    assert!(!mgr.is_focused(QUIET));
    let order: Vec<egui::Rect> = drawn.hit_rects.iter().map(|o| o.rect).collect();
    assert_eq!(
        order,
        [
            mgr.instance(quiet).unwrap().popup_rect(),
            mgr.instance(normal).unwrap().popup_rect()
        ],
        "the quiet popup is drawn below"
    );
    assert_eq!(mgr.topmost_visible_open(None).map(|t| t.1), Some(normal));
    assert!(!mgr.focus_passive(QUIET));
}

/// Given 오른쪽 아래 그립으로만 크기를 바꾸는 팝업, When 왼쪽 변을 끌면, Then 크기가 그대로다.
/// When 그립을 끌면 Then 오른쪽·아래 변이 포인터를 따라온다.
#[test]
fn grip_only_resize_ignores_the_other_edges() {
    let mut mgr = manager();
    let iid = open_at(&mut mgr, GRIP, egui::pos2(100.0, 100.0));
    let ctx = egui::Context::default();
    frame(&ctx, &mut mgr, Pointer::Idle);
    let before = mgr.instance(iid).unwrap().popup_rect();

    frame(&ctx, &mut mgr, Pointer::Press(egui::pos2(101.0, 150.0)));
    frame(&ctx, &mut mgr, Pointer::MoveTo(egui::pos2(60.0, 150.0)));
    frame(&ctx, &mut mgr, Pointer::Release(egui::pos2(60.0, 150.0)));
    assert_eq!(mgr.instance(iid).unwrap().popup_rect(), before);

    let grip = before.max - egui::vec2(4.0, 4.0);
    frame(&ctx, &mut mgr, Pointer::Press(grip));
    frame(
        &ctx,
        &mut mgr,
        Pointer::MoveTo(grip + egui::vec2(40.0, 30.0)),
    );
    frame(
        &ctx,
        &mut mgr,
        Pointer::Release(grip + egui::vec2(40.0, 30.0)),
    );
    let after = mgr.instance(iid).unwrap().popup_rect();
    assert_eq!(after.min, before.min);
    assert_eq!(after.max, grip + egui::vec2(40.0, 30.0));
}

/// Given 안내 열 자리, When 안내 세 장을 차례로 열면, Then 연 순서대로 위에서부터 간격을 두고 쌓고 폭은
/// 열의 폭이다. 차지한 사각형의 합을 돌려준다.
#[test]
fn notice_column_stacks_instances_in_open_order() {
    let mut mgr = manager();
    mgr.set_notice_column(Some(NoticeColumn {
        rect: egui::Rect::from_min_size(egui::pos2(1000.0, 40.0), egui::vec2(320.0, 1.0)),
        gap: LogicalPx(8.0),
    }));
    let heights = [80.0, 50.0, 60.0];
    let notices: Vec<PopupInstanceId> = heights
        .iter()
        .map(|&h| {
            let iid = mgr
                .open_request(&OpenRequest::new(NOTICE, PopupScope::Window), false)
                .expect("opens");
            mgr.instance_mut(iid).unwrap().size.y = h;
            iid
        })
        .collect();

    let drawn = frame(&egui::Context::default(), &mut mgr, Pointer::Idle);

    let tops: Vec<f32> = notices
        .iter()
        .map(|&iid| mgr.instance(iid).unwrap().pos.y)
        .collect();
    assert_eq!(tops, [40.0, 128.0, 186.0]);
    assert!(
        notices
            .iter()
            .all(|&iid| mgr.instance(iid).unwrap().size.x == 320.0)
    );
    assert_eq!(
        drawn.column_rect,
        Some(egui::Rect::from_min_max(
            egui::pos2(1000.0, 40.0),
            egui::pos2(1320.0, 246.0)
        ))
    );
}

/// Given 범위 위쪽에 붙는 팝업, When 열려 그려지면, Then 그 사각형을 피할 띠로 알려 준다. When 닫히면 Then 띠가 없다.
#[test]
fn an_anchored_top_popup_reserves_its_band() {
    let mut mgr = manager();
    assert!(mgr.reserved_top_bands().is_empty());
    let iid = open_at(&mut mgr, TOP, egui::pos2(300.0, 20.0));
    frame(&egui::Context::default(), &mut mgr, Pointer::Idle);
    assert_eq!(
        mgr.reserved_top_bands(),
        [mgr.instance(iid).unwrap().popup_rect()]
    );
    mgr.close(TOP);
    assert!(mgr.reserved_top_bands().is_empty());
}

/// Given 제목줄로 옮길 수 있는 팝업, When 끌어 옮긴 뒤 놓으면, Then 놓은 프레임의 결과에만 그 인스턴스와
/// 옮긴 사각형이 한 번 들어 있다.
#[test]
fn user_geometry_is_reported_once_on_release() {
    let mut mgr = manager();
    let iid = open_at(&mut mgr, NORMAL, egui::pos2(100.0, 100.0));
    let ctx = egui::Context::default();
    assert!(frame(&ctx, &mut mgr, Pointer::Idle).released.is_empty());

    let grab = egui::pos2(150.0, 105.0);
    let to = egui::pos2(250.0, 205.0);
    assert!(
        frame(&ctx, &mut mgr, Pointer::Press(grab))
            .released
            .is_empty()
    );
    assert!(
        frame(&ctx, &mut mgr, Pointer::MoveTo(to))
            .released
            .is_empty()
    );
    let released = frame(&ctx, &mut mgr, Pointer::Release(to)).released;
    let moved = mgr.instance(iid).unwrap().popup_rect();
    assert_eq!(moved.min, egui::pos2(200.0, 200.0));
    assert_eq!(released, [(iid, moved)]);
    assert!(frame(&ctx, &mut mgr, Pointer::Idle).released.is_empty());
}

/// 즉시 적용 판정(`intent::popup::gives_user_focus`)은 실제 열기와 같은 포커스 술어(`request_takes_focus`)를 읽는다.
/// Given 보통 선언, 키를 받지 않는 선언, 등록되지 않은 선언, When 사용자가 포커스를 주며 여는 요청을 대기열에 두면,
/// Then 보통 선언만 포커스를 주는 열기로 세고, 키를 받지 않는 선언은 실제로 열어도 포커스를 받지 않는다.
#[test]
fn the_early_apply_judgment_reads_the_focus_predicate_of_the_open() {
    let (mut state, _engine) = crate::state::tests::test_state();
    state.popups = manager();
    let request = |id| {
        OpenRequest::new(id, PopupScope::Window)
            .centered()
            .focused()
    };
    let intent = |id| crate::intent::UiIntent::OpenPopup(request(id)).from_user_shortcut("test");
    let gives = |state: &crate::state::RequestContext, id| {
        crate::intent::popup::gives_user_focus(state, &intent(id))
    };
    assert!(gives(&state, NORMAL));
    assert!(
        !gives(&state, QUIET),
        "키를 받지 않는 선언은 포커스를 주지 않는다"
    );
    assert!(
        !gives(&state, "arrange_test_missing"),
        "등록되지 않은 선언은 열리지 않는다"
    );
    assert!(!state.popups.request_takes_focus(&request(NORMAL), false));
    state
        .popups
        .open_request(&request(QUIET), true)
        .expect("opens");
    assert!(!state.popups.is_focused(QUIET));
    assert!(
        state
            .popups
            .open_request(&request("arrange_test_missing"), true)
            .is_none()
    );
}
