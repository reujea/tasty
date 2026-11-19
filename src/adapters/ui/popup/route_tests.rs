//! 범위 경로 계산을 검사한다. 장면은 좌우 두 pane(L·R)에 surface 가 하나씩 있는 실제 엔진이다.

use super::{ScopePath, path_at, path_of_focus};
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::layout_context::SurfaceOwner;
use crate::adapters::ui::popup::PopupScope;
use crate::model::{LogicalPx, LogicalRect};
use crate::state::RequestContext;

pub(super) const WORKSPACE: u32 = 1;
pub(super) const PANE_L: u32 = 10;
pub(super) const PANE_R: u32 = 20;
/// 이 장면에서 tab 과 surface 는 같은 ID 를 쓴다.
pub(super) const SURFACE_L: u32 = 101;
pub(super) const SURFACE_R: u32 = 201;

/// 탭 바 높이. 디자인 토큰을 복제하지 않는 시험용 값이다.
const TAB_BAR: LogicalPx = LogicalPx(30.0);
/// 창 너비와 pane 하나의 너비, 작업 영역 높이(아래 20px 은 상태바다).
const WIDTH: LogicalPx = LogicalPx(900.0);
const HALF: LogicalPx = LogicalPx(450.0);
const WORK_H: LogicalPx = LogicalPx(680.0);

/// 시험의 길이는 길이 타입으로 계산하고 egui 를 부를 때만 값을 꺼낸다.
pub(super) fn rect(x: LogicalPx, y: LogicalPx, width: LogicalPx, height: LogicalPx) -> LogicalRect {
    LogicalRect {
        x,
        y,
        width,
        height,
    }
}

pub(super) fn to_egui(r: LogicalRect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(r.x.value(), r.y.value()),
        egui::vec2(r.width.value(), r.height.value()),
    )
}

pub(super) fn point(x: LogicalPx, y: LogicalPx) -> egui::Pos2 {
    egui::pos2(x.value(), y.value())
}

/// 사이드바 자리를 비우지 않고 아래 20px 만 상태바로 남긴 workspace 영역.
fn workspace_rect() -> egui::Rect {
    to_egui(rect(LogicalPx(0.0), LogicalPx(0.0), WIDTH, WORK_H))
}

fn pane_rect(left: LogicalPx) -> egui::Rect {
    to_egui(rect(left, LogicalPx(0.0), HALF, WORK_H))
}

pub(super) fn surface_rect_l() -> egui::Rect {
    to_egui(rect(LogicalPx(0.0), TAB_BAR, HALF, WORK_H - TAB_BAR))
}

pub(super) fn surface_rect_r() -> egui::Rect {
    to_egui(rect(HALF, TAB_BAR, HALF, WORK_H - TAB_BAR))
}

/// L(0..450)·R(450..900) 두 surface 의 레이아웃.
pub(super) fn layout() -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: WORKSPACE,
        pane_rects: vec![
            (PANE_L, pane_rect(LogicalPx(0.0))),
            (PANE_R, pane_rect(HALF)),
        ],
        surface_rects: vec![(SURFACE_L, surface_rect_l()), (SURFACE_R, surface_rect_r())],
        active_tabs: vec![(PANE_L, 0), (PANE_R, 0)],
        active_tab_ids: vec![(PANE_L, SURFACE_L), (PANE_R, SURFACE_R)],
        workspace_rect: Some(workspace_rect()),
        surface_owners: vec![
            SurfaceOwner {
                surface: SURFACE_L,
                tab: SURFACE_L,
                pane: PANE_L,
            },
            SurfaceOwner {
                surface: SURFACE_R,
                tab: SURFACE_R,
                pane: PANE_R,
            },
        ],
        // 각 pane 에 surface 가 하나라 탭 콘텐츠 영역이 surface 와 같다.
        tab_content_rects: vec![(PANE_L, surface_rect_l()), (PANE_R, surface_rect_r())],
    }
}

/// 좌우 두 pane 의 실제 엔진. 포커스는 `focused_pane` 이다.
pub(super) fn scene(
    focused_pane: u32,
) -> (
    RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::DomainEvent as E;
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: WORKSPACE,
            name: "workspace".into(),
            category: 0,
            index: 0,
            pane: PANE_L,
        },
        E::PaneSplit {
            target: PANE_L,
            pane: PANE_R,
            split: tasty_core::SplitSpec {
                direction: crate::model::SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
    ];
    for (id, pane) in [(SURFACE_L, PANE_L), (SURFACE_R, PANE_R)] {
        events.push(E::TabCreated {
            id,
            pane,
            index: 0,
            name: "terminal".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "terminal".into(),
                data: None,
            },
        });
    }
    let model = crate::state::tests::test_model(events);
    let (mut state, session) = crate::state::tests::test_state_from_model(model);
    state
        .navigation
        .select_pane(session.core_state.workspace_at(0).unwrap(), focused_pane);
    (state, session)
}

fn all_scopes_of(path: &ScopePath) -> Vec<PopupScope> {
    [
        PopupScope::Window,
        PopupScope::Workspace(WORKSPACE),
        PopupScope::Pane(PANE_L),
        PopupScope::Pane(PANE_R),
        PopupScope::Tab(SURFACE_L),
        PopupScope::Tab(SURFACE_R),
        PopupScope::Surface(SURFACE_L),
        PopupScope::Surface(SURFACE_R),
    ]
    .into_iter()
    .filter(|s| path.contains(s))
    .collect()
}

#[test]
fn a_point_in_the_left_surface_is_on_the_left_path_only() {
    let path = path_at(&layout(), surface_rect_l().center());
    assert_eq!(
        all_scopes_of(&path),
        [
            PopupScope::Window,
            PopupScope::Workspace(WORKSPACE),
            PopupScope::Pane(PANE_L),
            PopupScope::Tab(SURFACE_L),
            PopupScope::Surface(SURFACE_L),
        ]
    );
    assert!(!path.contains(&PopupScope::Surface(SURFACE_R)));
}

/// 맞닿은 경계 점은 포인터의 surface 판정처럼 오른쪽 surface 하나에만 든다.
#[test]
fn a_point_on_a_shared_edge_belongs_to_the_right_side_only() {
    let path = path_at(&layout(), point(HALF, LogicalPx(100.0)));
    assert_eq!(
        all_scopes_of(&path),
        [
            PopupScope::Window,
            PopupScope::Workspace(WORKSPACE),
            PopupScope::Pane(PANE_R),
            PopupScope::Tab(SURFACE_R),
            PopupScope::Surface(SURFACE_R),
        ]
    );
}

/// 1px 간격으로 나눈 두 pane 사이의 분할선은 어느 pane·surface 에도 들지 않는다.
#[test]
fn a_point_on_a_one_pixel_divider_is_on_no_pane() {
    let mut gap = layout();
    let divider = HALF - LogicalPx(1.0);
    gap.pane_rects[0].1 = to_egui(rect(LogicalPx(0.0), LogicalPx(0.0), divider, WORK_H));
    gap.surface_rects[0].1 = to_egui(rect(LogicalPx(0.0), TAB_BAR, divider, WORK_H - TAB_BAR));
    let path = path_at(&gap, point(divider, LogicalPx(100.0)));
    assert_eq!(
        all_scopes_of(&path),
        [PopupScope::Window, PopupScope::Workspace(WORKSPACE)]
    );
}

/// 작업 영역의 아래 끝 줄은 상태바의 첫 줄이라 Window 만 담는다.
#[test]
fn the_bottom_edge_of_the_work_area_is_outside_it() {
    let path = path_at(&layout(), point(LogicalPx(100.0), WORK_H));
    assert_eq!(all_scopes_of(&path), [PopupScope::Window]);
}

#[test]
fn a_point_on_the_status_bar_is_on_the_window_only() {
    // 작업 영역(0..680) 아래 상태바 자리.
    let path = path_at(&layout(), point(LogicalPx(300.0), WORK_H + LogicalPx(10.0)));
    assert_eq!(all_scopes_of(&path), [PopupScope::Window]);
    assert!(!path.contains(&PopupScope::Workspace(WORKSPACE)));
}

#[test]
fn a_point_on_the_sidebar_is_on_the_window_only() {
    // 사이드바(0..180)를 둔 창. 작업 영역은 그 오른쪽이다.
    let mut with_sidebar = layout();
    let sidebar = LogicalPx(180.0);
    with_sidebar.workspace_rect = Some(to_egui(rect(
        sidebar,
        LogicalPx(0.0),
        WIDTH - sidebar,
        WORK_H,
    )));
    with_sidebar.pane_rects.retain(|(id, _)| *id == PANE_R);
    with_sidebar
        .surface_rects
        .retain(|(id, _)| *id == SURFACE_R);
    let path = path_at(&with_sidebar, point(sidebar / 2.0, LogicalPx(300.0)));
    assert_eq!(all_scopes_of(&path), [PopupScope::Window]);
    assert!(
        path_at(&with_sidebar, surface_rect_r().center())
            .contains(&PopupScope::Workspace(WORKSPACE))
    );
}

#[test]
fn without_a_workspace_area_no_point_is_on_the_workspace() {
    // 작업 영역이 없으면 surface 위 좌표도 Workspace 를 담지 않는다.
    let mut no_area = layout();
    no_area.workspace_rect = None;
    let path = path_at(&no_area, surface_rect_l().center());
    assert!(path.contains(&PopupScope::Surface(SURFACE_L)));
    assert!(!path.contains(&PopupScope::Workspace(WORKSPACE)));
}

#[test]
fn a_point_on_the_tab_bar_reaches_the_pane_but_no_surface() {
    let path = path_at(&layout(), point(LogicalPx(600.0), TAB_BAR / 2.0));
    assert_eq!(
        all_scopes_of(&path),
        [
            PopupScope::Window,
            PopupScope::Workspace(WORKSPACE),
            PopupScope::Pane(PANE_R),
        ]
    );
}

/// 한 pane 의 탭 하나를 surface 둘로 1px 간격을 두고 나눈 레이아웃.
fn one_tab_split_in_two() -> LayoutContext {
    const TAB: u32 = 301;
    let divider = HALF - LogicalPx(1.0);
    let body = WORK_H - TAB_BAR;
    let mut split = layout();
    split.pane_rects = vec![(
        PANE_L,
        to_egui(rect(LogicalPx(0.0), LogicalPx(0.0), WIDTH, WORK_H)),
    )];
    split.surface_rects = vec![
        (
            SURFACE_L,
            to_egui(rect(LogicalPx(0.0), TAB_BAR, divider, body)),
        ),
        (SURFACE_R, to_egui(rect(HALF, TAB_BAR, HALF, body))),
    ];
    split.tab_content_rects = vec![(PANE_L, to_egui(rect(LogicalPx(0.0), TAB_BAR, WIDTH, body)))];
    split.active_tabs = vec![(PANE_L, 0)];
    split.active_tab_ids = vec![(PANE_L, TAB)];
    split.surface_owners = [SURFACE_L, SURFACE_R]
        .into_iter()
        .map(|surface| SurfaceOwner {
            surface,
            tab: TAB,
            pane: PANE_L,
        })
        .collect();
    split
}

/// 같은 탭 surface 사이의 분할선은 탭 콘텐츠라 Tab 까지 담고 어떤 Surface 도 담지 않는다.
#[test]
fn a_point_on_a_divider_inside_a_tab_stays_on_the_tab() {
    let split = one_tab_split_in_two();
    let path = path_at(&split, point(HALF - LogicalPx(0.5), LogicalPx(100.0)));
    assert!(path.contains(&PopupScope::Tab(301)));
    assert!(path.contains(&PopupScope::Pane(PANE_L)));
    assert!(path.contains(&PopupScope::Workspace(WORKSPACE)));
    assert!(!path.contains(&PopupScope::Surface(SURFACE_L)));
    assert!(!path.contains(&PopupScope::Surface(SURFACE_R)));
    // 같은 pane 의 탭 바는 여전히 Tab 을 담지 않는다.
    let bar = path_at(&split, point(LogicalPx(100.0), TAB_BAR / 2.0));
    assert!(bar.contains(&PopupScope::Pane(PANE_L)));
    assert!(!bar.contains(&PopupScope::Tab(301)));
}

#[test]
fn a_window_without_a_layout_puts_every_point_on_the_window_only() {
    let path = path_at(
        &LayoutContext::window_only(),
        point(LogicalPx(10.0), LogicalPx(10.0)),
    );
    assert_eq!(path, ScopePath::WINDOW);
    assert!(path.contains(&PopupScope::Window));
}

#[test]
fn the_focus_path_is_the_same_from_the_engine_and_the_core_state() {
    for (pane, surface) in [(PANE_L, SURFACE_L), (PANE_R, SURFACE_R)] {
        let (state, mut session) = scene(pane);
        let from_core = path_of_focus(&state, &session.core_state);
        let engine = session.borrow_mut();
        let from_engine = path_of_focus(&state, &engine.read());
        assert_eq!(from_core, from_engine);
        assert!(from_core.contains(&PopupScope::Surface(surface)));
        assert!(from_core.contains(&PopupScope::Tab(surface)));
        assert!(from_core.contains(&PopupScope::Pane(pane)));
        assert!(from_core.contains(&PopupScope::Workspace(WORKSPACE)));
        // 포커스 경로와 같은 surface 위의 좌표 경로는 같은 노드를 담는다.
        let rect = if surface == SURFACE_L {
            surface_rect_l()
        } else {
            surface_rect_r()
        };
        assert_eq!(path_at(&layout(), rect.center()), from_core);
    }
}

#[test]
fn the_layout_built_from_the_engine_records_who_owns_each_surface() {
    use crate::model::{PhysicalPx, PhysicalRect};
    let (mut state, mut session) = scene(PANE_L);
    // 탭 바가 있어야 콘텐츠 영역과 pane 사각형이 구별된다.
    state.tab_bar_height = PhysicalPx(30.0);
    let engine = session.borrow_mut();
    let terminal = PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(900.0),
        height: PhysicalPx(680.0),
    };
    let built = crate::adapters::ui::layout_context::build_layout_context(
        &state,
        &engine.read(),
        &[],
        terminal,
        1.0,
    );
    let mut owners = built.surface_owners.clone();
    owners.sort_by_key(|o| o.surface);
    assert_eq!(owners, layout().surface_owners);
    // 탭 콘텐츠 영역은 pane 마다 하나이고 그 pane 의 surface 를 담는다.
    for owner in &owners {
        let (_, content) = built
            .tab_content_rects
            .iter()
            .find(|(pane, _)| *pane == owner.pane)
            .expect("pane 마다 탭 콘텐츠 영역이 있다");
        let (_, surface) = built
            .surface_rects
            .iter()
            .find(|(id, _)| *id == owner.surface)
            .unwrap();
        assert!(content.contains_rect(*surface));
        // 탭 바는 콘텐츠 영역에 들지 않는다. surface 하나인 탭이라 둘의 위쪽 끝이 같다.
        assert_eq!(content.top(), surface.top());
        // 두 pane 이 좌우로 놓여 pane 위쪽 끝이 작업 영역 위쪽 끝이다.
        assert!(content.top() > built.workspace_rect.unwrap().top());
    }
}

/// 창의 포커스 노드는 엔진 포커스 surface 다. 열려 있기만 한 plugin 팝업은 바꾸지 않고, 포커스 노드가 된 plugin
/// 팝업(사용자가 열거나 누름)이면 그 팝업이다. 그 팝업이 닫히면 정규화가 surface 로 되돌린다.
#[test]
fn the_focus_node_is_the_focused_surface_until_a_popup_takes_it() {
    use crate::adapters::ui::popup::focus::PopupNode;
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(PANE_L);
    let engine = session.borrow_mut();
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    state.plugin_popup_open = true;
    state
        .plugin_popup_bound_scopes
        .insert(9, ("p".to_string(), Some(PopupScope::Surface(SURFACE_R))));
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L)),
        "열려 있기만 한 plugin 팝업은 포커스 노드가 아니다"
    );
    state.popups.set_focus(Some(PopupNode::Plugin(9)));
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(9))
    );
    state.plugin_popup_bound_scopes.clear();
    state.normalize_focus(&engine.read());
    assert_eq!(state.popups.focus(), None);
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
}

#[test]
fn the_fullscreen_stage_takes_the_keys() {
    use crate::state::focus_node::FocusNode;
    let (mut state, mut session) = scene(PANE_L);
    let engine = session.borrow_mut();
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(state.focus_node(&engine.read()), FocusNode::Stage);
}

#[path = "route_repro_tests.rs"]
mod repro;

#[path = "route_pending_tests.rs"]
mod pending;

#[cfg(feature = "gui")]
#[path = "frame_key_owner_tests.rs"]
mod frame_keys;
