//! 프레임의 실제 도형과 팝업 상태에서 scrim 영역·클립과 작은 범위의 처리를 검사한다.

use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::{PopupScope, defs, frame::draw_popup_layer};
use crate::state::MainViewState;

/// 범위 대상이 사라진 팝업은 그리기 전에 닫히므로 엔진에 실제로 있는 surface를 쓴다.
#[derive(Clone, Copy)]
struct Ids {
    surface: u32,
    /// 팝업이 경계에 닿도록 좁게 만든 테스트 영역. 디자인 토큰의 치수를 복제하지 않는다.
    narrow: u32,
}

fn screen() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0))
}

fn surface_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(400.0, 360.0))
}

fn narrow_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(601.0, 61.0), egui::vec2(213.0, 247.0))
}

fn layout(ids: Ids) -> LayoutContext {
    layout_of(vec![
        (ids.surface, surface_rect()),
        (ids.narrow, narrow_rect()),
    ])
}

fn layout_of(surface_rects: Vec<(u32, egui::Rect)>) -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 0,
        pane_rects: vec![],
        surface_rects,
        active_tabs: vec![],
        active_tab_ids: vec![],
        workspace_rect: None,
        surface_owners: Vec::new(),
        tab_content_rects: Vec::new(),
    }
}

fn prepared() -> (
    MainViewState,
    crate::runtime::engine_session::EngineSession,
    Ids,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    let model = crate::state::tests::test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "workspace".into(),
            category: 0,
            index: 0,
            pane: 1,
        },
        E::TabCreated {
            id: 1,
            pane: 1,
            index: 0,
            name: "first".into(),
            surface: SurfaceSpec {
                id: 1,
                kind: "terminal".into(),
                data: None,
            },
        },
        E::TabCreated {
            id: 2,
            pane: 1,
            index: 1,
            name: "second".into(),
            surface: SurfaceSpec {
                id: 2,
                kind: "terminal".into(),
                data: None,
            },
        },
    ]);
    let (mut state, engine_session) = crate::state::tests::test_state_from_model(model);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    let (surface, narrow) = (1, 2);
    (state, engine_session, Ids { surface, narrow })
}

/// 한 프레임을 그려 나온 도형을 그대로 돌려준다.
fn painted_shapes(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    ids: Ids,
) -> Vec<egui::epaint::ClippedShape> {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    };
    ctx.run(raw, |ctx| {
        draw_popup_layer(ctx, state, engine, &layout(ids));
    })
    .shapes
}

/// 한 프레임을 그려 scrim 색으로 칠해진 사각형들의 rect 를 모은다.
fn scrim_rects(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    ids: Ids,
) -> Vec<egui::Rect> {
    let scrim = egui::Color32::from(crate::theme::theme().scrim());
    let mut found = Vec::new();
    for clipped in painted_shapes(state, engine, ids) {
        if let egui::Shape::Rect(r) = &clipped.shape
            && r.fill == scrim
        {
            found.push(r.rect);
        }
    }
    found
}

/// surface 범위 popup 의 scrim 은 그 칸 하나를 덮는다 — 화면 전체가 아니다.
#[test]
fn a_surface_scoped_popup_dims_only_its_own_surface() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.surface), true);
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![surface_rect()]);
}

/// 창 범위 popup 은 종전대로 창 전체를 덮는다.
#[test]
fn a_window_scoped_popup_still_dims_the_whole_window() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_centered_focused("command_palette", PopupScope::Window);
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![screen()]);
}

/// 부모·자식이 같은 범위를 쓰면 scrim을 중복으로 그리지 않는다.
#[test]
fn a_parent_and_its_child_picker_share_one_scrim() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.surface), true);
    state.popups.open_centered_focused(
        super::file_picker::FILE_PICKER_POPUP_ID,
        PopupScope::Surface(ids.surface),
    );
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![surface_rect()]);
}

/// scrim이 필요한 팝업 둘도 같은 범위에서는 한 번만 그린다.
#[test]
fn two_scrim_popups_in_one_scope_still_paint_one_scrim() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.surface), true);
    state
        .popups
        .open_centered_focused("port_scanner", PopupScope::Surface(ids.surface));
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![surface_rect()]);
}

/// 창 전체 scrim이 있으면 내부 surface scrim을 덧그리지 않는다.
#[test]
fn a_window_scrim_absorbs_the_surface_scrim_under_it() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.surface), true);
    state
        .popups
        .open_centered_focused("command_palette", PopupScope::Window);
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![screen()]);
}

/// scrim뿐 아니라 그림자도 소속 영역 안에 있어야 한다. 도형과 clip의 교집합을 검사한다.
#[test]
fn nothing_a_surface_scoped_popup_paints_lands_outside_its_surface() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.narrow), true);
    // 경계 자신은 안쪽이다 — 반올림 한 칸까지만 봐준다.
    let bound = narrow_rect().expand(0.5);
    let shapes = painted_shapes(&mut state, &engine.read(), ids);
    assert!(
        state.popups.is_open("convert_surface"),
        "the popup must be drawn for the bound check to mean anything"
    );
    for clipped in shapes {
        let painted = clipped
            .clip_rect
            .intersect(clipped.shape.visual_bounding_rect());
        if !painted.is_positive() {
            continue;
        }
        assert!(
            bound.contains_rect(painted),
            "surface 범위 popup 이 칸 밖을 칠했다: {painted:?} ⊄ {:?}",
            narrow_rect()
        );
    }
}

/// 같은 egui Context 로 한 프레임을 그린다. 실제 창처럼 프레임 사이 상태가 이어진다.
fn frame_with(
    ctx: &egui::Context,
    raw: egui::RawInput,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    layout: &LayoutContext,
) {
    drop(ctx.run(raw, |ctx| draw_popup_layer(ctx, state, engine, layout)));
}

fn screen_input(size: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    }
}

/// Windows 의 장식 없는 창은 최소화 중 작업 표시줄 아이콘 크기(160x28)를 보고한다. 그 프레임에도
/// 팝업은 열린 채 같은 자리·크기에 남고, 복원한 뒤에도 그대로다.
#[test]
fn a_minimized_frame_keeps_the_open_popup_where_it_was() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state.popups.open("convert_surface", PopupScope::Window);
    let ctx = egui::Context::default();
    let layout = layout(ids);
    frame_with(
        &ctx,
        screen_input(egui::vec2(1280.0, 720.0)),
        &mut state,
        &engine.read(),
        &layout,
    );
    let before = state.popups.visible_rect("convert_surface");
    assert!(before.is_some());

    let mut minimized = screen_input(egui::vec2(160.0, 28.0));
    crate::gfx::gpu::keep_screen_while_minimized(&mut minimized, true);
    frame_with(&ctx, minimized, &mut state, &engine.read(), &layout);
    assert!(state.popups.is_open("convert_surface"));
    assert_eq!(state.popups.visible_rect("convert_surface"), before);

    frame_with(
        &ctx,
        screen_input(egui::vec2(1280.0, 720.0)),
        &mut state,
        &engine.read(),
        &layout,
    );
    assert_eq!(state.popups.visible_rect("convert_surface"), before);
}

/// 범위가 제목줄과 여백보다 낮으면 팝업은 범위 안으로 줄고 콘텐츠 자리는 빈 사각형이다.
/// 팝업은 열린 채 범위를 유지하며, 범위가 다시 넓어지면 원래 크기로 그린다.
#[test]
fn a_scope_lower_than_the_title_bar_leaves_an_empty_content_area() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_request(&super::convert::open_request(ids.surface), true);
    let ctx = egui::Context::default();
    let wide = layout(ids);
    frame_with(
        &ctx,
        screen_input(screen().size()),
        &mut state,
        &engine.read(),
        &wide,
    );
    let before = state.popups.visible_rect("convert_surface").expect("open");

    let low = egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(300.0, 40.0));
    let narrow = layout_of(vec![(ids.surface, low)]);
    frame_with(
        &ctx,
        screen_input(screen().size()),
        &mut state,
        &engine.read(),
        &narrow,
    );
    let popup = state
        .popups
        .popups
        .iter()
        .find(|p| p.id == "convert_surface")
        .expect("registered");
    assert!(popup.open);
    assert_eq!(popup.scope, PopupScope::Surface(ids.surface));
    assert!(low.contains_rect(popup.popup_rect()));
    let content = popup.content_rect();
    assert!(content.width() >= 0.0 && content.height() == 0.0);

    frame_with(
        &ctx,
        screen_input(screen().size()),
        &mut state,
        &engine.read(),
        &wide,
    );
    assert_eq!(
        state
            .popups
            .visible_rect("convert_surface")
            .map(|r| r.size()),
        Some(before.size())
    );
}

/// Given sizer 가 없는 팝업(port_scanner)이 창보다 커서 줄어 그려진 뒤,
/// When 창이 다시 넓어지면,
/// Then 선언 크기로 다시 그린다. 줄인 크기가 필요 크기를 덮지 않는다.
#[test]
fn a_popup_without_a_sizer_grows_back_when_its_scope_widens() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    assert!(
        defs::find("port_scanner").is_some_and(|d| d.sizer.is_none()),
        "this scene needs a popup without a sizer"
    );
    state
        .popups
        .open_centered_focused("port_scanner", PopupScope::Window);
    let ctx = egui::Context::default();
    let layout = layout(ids);
    frame_with(
        &ctx,
        screen_input(screen().size()),
        &mut state,
        &engine.read(),
        &layout,
    );
    let declared = state
        .popups
        .visible_rect("port_scanner")
        .expect("open")
        .size();

    let small = declared * 0.5;
    frame_with(
        &ctx,
        screen_input(small),
        &mut state,
        &engine.read(),
        &layout,
    );
    let shrunk = state
        .popups
        .visible_rect("port_scanner")
        .expect("open")
        .size();
    assert!(
        shrunk.x <= small.x && shrunk.y <= small.y,
        "the drawn size must fit the small window: {shrunk:?} > {small:?}"
    );
    let needed = state
        .popups
        .get_mut("port_scanner")
        .expect("registered")
        .needed_size();
    assert_eq!(
        needed, declared,
        "shrinking to the window must not lower the needed size"
    );

    frame_with(
        &ctx,
        screen_input(screen().size()),
        &mut state,
        &engine.read(),
        &layout,
    );
    assert_eq!(
        state.popups.visible_rect("port_scanner").map(|r| r.size()),
        Some(declared)
    );
}

/// 사이드바(x < 40)·타이틀바(y < 30)·상태바(y > 676)를 뺀 workspace 콘텐츠 영역(탭 바와 pane).
fn workspace_area() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(40.0, 30.0), egui::pos2(900.0, 676.0))
}

/// 탭 바(30 ≤ y < 60) 아래 활성 탭의 콘텐츠 영역.
fn tab_content() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(40.0, 60.0), egui::pos2(900.0, 676.0))
}

/// pane 하나(id 1, 탭 바 포함)가 workspace 1 을 채운 레이아웃. `prepared()` 의 엔진과 ID가 같다.
fn workspace_layout(ids: Ids) -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: 1,
        pane_rects: vec![(1, workspace_area())],
        surface_rects: vec![(ids.surface, tab_content())],
        active_tabs: vec![(1, 0)],
        active_tab_ids: vec![(1, 1)],
        workspace_rect: Some(workspace_area()),
        surface_owners: Vec::new(),
        tab_content_rects: vec![(1, tab_content())],
    }
}

/// 한 프레임을 그려 scrim 사각형과 그 팝업의 사각형을 돌려준다.
fn scrim_and_rect(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    layout: &LayoutContext,
    id: &'static str,
) -> (Vec<egui::Rect>, egui::Rect) {
    let scrim = egui::Color32::from(crate::theme::theme().scrim());
    let ctx = egui::Context::default();
    let shapes = ctx
        .run(screen_input(screen().size()), |ctx| {
            draw_popup_layer(ctx, state, engine, layout)
        })
        .shapes;
    let rects = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) if r.fill == scrim => Some(r.rect),
            _ => None,
        })
        .collect();
    (rects, state.popups.visible_rect(id).expect("drawn"))
}

/// Given Workspace 범위의 덮는 팝업일 때,
/// Then scrim·배치·clamp 사각형은 workspace 콘텐츠 영역이고 사이드바·상태바와 겹치지 않는다.
#[test]
fn a_workspace_scoped_popup_stays_inside_the_workspace_area() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_centered_focused("port_scanner", PopupScope::Workspace(1));
    let (scrims, rect) = scrim_and_rect(
        &mut state,
        &engine.read(),
        &workspace_layout(ids),
        "port_scanner",
    );
    assert_eq!(scrims, vec![workspace_area()]);
    assert!(workspace_area().contains_rect(rect), "{rect:?}");
    assert_eq!(rect.center(), workspace_area().center());
}

/// Given Tab 범위의 덮는 팝업일 때,
/// Then scrim·배치·clamp 사각형은 탭 바 아래 콘텐츠 영역이다.
#[test]
fn a_tab_scoped_popup_stays_below_the_tab_bar() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state
        .popups
        .open_centered_focused("port_scanner", PopupScope::Tab(1));
    let (scrims, rect) = scrim_and_rect(
        &mut state,
        &engine.read(),
        &workspace_layout(ids),
        "port_scanner",
    );
    assert_eq!(scrims, vec![tab_content()]);
    assert!(tab_content().contains_rect(rect), "{rect:?}");
    assert_eq!(rect.center(), tab_content().center());
}

/// Given 탭 바 위의 점일 때, Then Pane 범위 사각형에는 들고 Tab 범위 사각형에는 들지 않는다.
#[test]
fn a_pane_scope_holds_the_tab_bar_and_a_tab_scope_does_not() {
    let (_, _, ids) = prepared();
    let layout = workspace_layout(ids);
    let on_tab_bar = egui::pos2(450.0, 45.0);
    let pane = super::PopupManager::scope_rect(&PopupScope::Pane(1), Some(&layout));
    let tab = super::PopupManager::scope_rect(&PopupScope::Tab(1), Some(&layout));
    assert!(pane.is_some_and(|r| r.contains(on_tab_bar)));
    assert!(tab.is_some_and(|r| !r.contains(on_tab_bar)));
}

/// Given 저장된 원격 행의 연결 팝업(Workspace 범위, inset 없음)일 때,
/// Then clamp 경계는 바꾸기 전 특례가 돌려주던 `workspace_rect` 그대로다.
#[test]
fn the_remote_row_bounds_are_the_workspace_rect_as_before() {
    let (_, _, ids) = prepared();
    let layout = workspace_layout(ids);
    let inset = crate::theme::theme().spacing_sm.value();
    let scope = PopupScope::Workspace(1);
    assert_eq!(
        super::PopupManager::scope_rect(&scope, Some(&layout)),
        layout.workspace_rect
    );
    assert_eq!(
        super::PopupManager::scope_bounds(&scope, Some(&layout), screen(), inset),
        workspace_area()
    );
}

/// Given 다른 workspace 의 범위일 때, Then 이 창의 workspace 영역을 빌려 쓰지 않는다.
#[test]
fn an_inactive_workspace_scope_has_no_rect() {
    let (_, _, ids) = prepared();
    let layout = workspace_layout(ids);
    assert_eq!(
        super::PopupManager::scope_rect(&PopupScope::Workspace(2), Some(&layout)),
        None
    );
}

/// Given 그리기 루프일 때, Then 범위 사각형·clamp 계산에 원격 행 특례가 없다.
#[test]
fn the_draw_loop_has_no_remote_row_special_case() {
    let source = include_str!("draw.rs");
    assert!(!source.contains("is_remote_row"));
    assert!(!source.contains("popup_scope_rect"));
}
