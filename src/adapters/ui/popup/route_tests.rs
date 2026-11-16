//! 범위 경로 계산을 검사한다. 장면은 좌우 두 pane(L·R)에 surface 가 하나씩 있는 실제 엔진이다.

use super::{KeyTarget, ScopePath, keyboard_target, path_at, path_of_focus};
use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::layout_context::SurfaceOwner;
use crate::adapters::ui::popup::PopupScope;
use crate::state::RequestContext;

pub(super) const WORKSPACE: u32 = 1;
pub(super) const PANE_L: u32 = 10;
pub(super) const PANE_R: u32 = 20;
/// 이 장면에서 tab 과 surface 는 같은 ID 를 쓴다.
pub(super) const SURFACE_L: u32 = 101;
pub(super) const SURFACE_R: u32 = 201;

/// 탭 바 높이. 디자인 토큰을 복제하지 않는 시험용 값이다.
const TAB_BAR: f32 = 30.0;

/// 사이드바 자리를 비우지 않고 아래 20px 만 상태바로 남긴 workspace 영역.
fn workspace_rect() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 680.0))
}

fn pane_rect(left: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(left, 0.0), egui::pos2(left + 450.0, 680.0))
}

pub(super) fn surface_rect_l() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, TAB_BAR), egui::pos2(450.0, 680.0))
}

pub(super) fn surface_rect_r() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(450.0, TAB_BAR), egui::pos2(900.0, 680.0))
}

/// L(0..450)·R(450..900) 두 surface 의 레이아웃.
pub(super) fn layout() -> LayoutContext {
    LayoutContext {
        active_workspace: 0,
        active_workspace_id: WORKSPACE,
        pane_rects: vec![(PANE_L, pane_rect(0.0)), (PANE_R, pane_rect(450.0))],
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

#[test]
fn a_point_outside_the_workspace_is_on_the_window_only() {
    // 상태바 자리.
    let path = path_at(&layout(), egui::pos2(300.0, 690.0));
    assert_eq!(all_scopes_of(&path), [PopupScope::Window]);
    assert!(!path.contains(&PopupScope::Workspace(WORKSPACE)));
}

#[test]
fn a_point_on_the_tab_bar_reaches_the_pane_but_no_surface() {
    let path = path_at(&layout(), egui::pos2(600.0, TAB_BAR / 2.0));
    assert_eq!(
        all_scopes_of(&path),
        [
            PopupScope::Window,
            PopupScope::Workspace(WORKSPACE),
            PopupScope::Pane(PANE_R),
        ]
    );
}

#[test]
fn a_window_without_a_layout_puts_every_point_on_the_window_only() {
    let path = path_at(&LayoutContext::window_only(), egui::pos2(10.0, 10.0));
    assert_eq!(path, ScopePath::default());
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
    let (state, mut session) = scene(PANE_L);
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
}

#[test]
fn the_key_target_wraps_the_current_gate() {
    let (mut state, mut session) = scene(PANE_L);
    let engine = session.borrow_mut();
    assert_eq!(
        keyboard_target(&state, &engine.read()),
        Some(KeyTarget::Surface(SURFACE_L))
    );
    state.plugin_popup_open = true;
    assert_eq!(
        keyboard_target(&state, &engine.read()),
        Some(KeyTarget::Overlay)
    );
}

#[test]
fn the_fullscreen_stage_takes_the_keys() {
    let (mut state, mut session) = scene(PANE_L);
    let engine = session.borrow_mut();
    assert!(state.open_fullscreen_stage("blank"));
    assert_eq!(
        keyboard_target(&state, &engine.read()),
        Some(KeyTarget::Overlay)
    );
}
