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
    state.dialogs.convert_popup = Some(ids.surface);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.surface));
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
        .open_with_scope("command_palette", PopupScope::Window);
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![screen()]);
}

/// 부모·자식이 같은 범위를 쓰면 scrim을 중복으로 그리지 않는다.
#[test]
fn a_parent_and_its_child_picker_share_one_scrim() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state.dialogs.convert_popup = Some(ids.surface);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.surface));
    state.popups.open_with_scope(
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
    state.dialogs.convert_popup = Some(ids.surface);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.surface));
    state
        .popups
        .open_with_scope("port_scanner", PopupScope::Surface(ids.surface));
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![surface_rect()]);
}

/// 창 전체 scrim이 있으면 내부 surface scrim을 덧그리지 않는다.
#[test]
fn a_window_scrim_absorbs_the_surface_scrim_under_it() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state.dialogs.convert_popup = Some(ids.surface);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.surface));
    state
        .popups
        .open_with_scope("command_palette", PopupScope::Window);
    let rects = scrim_rects(&mut state, &engine.read(), ids);
    assert_eq!(rects, vec![screen()]);
}

/// scrim뿐 아니라 그림자도 소속 영역 안에 있어야 한다. 도형과 clip의 교집합을 검사한다.
#[test]
fn nothing_a_surface_scoped_popup_paints_lands_outside_its_surface() {
    let (mut state, mut engine_session, ids) = prepared();
    let engine = engine_session.borrow_mut();
    state.dialogs.convert_popup = Some(ids.narrow);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.narrow));
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
    state.popups.open("convert_surface");
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
    state.dialogs.convert_popup = Some(ids.surface);
    state
        .popups
        .open_with_scope("convert_surface", PopupScope::Surface(ids.surface));
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
