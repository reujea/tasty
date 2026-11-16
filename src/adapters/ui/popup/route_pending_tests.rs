//! attach 대기 장면의 범위 경로. 대기 중인 새 탭·새 surface 자리는 실제 surface 가 없어도 탭 콘텐츠다.

use std::sync::Arc;

use super::{point, rect};
use crate::adapters::ui::popup::PopupScope;
use crate::adapters::ui::popup::route::{ScopePath, path_at};
use crate::ipc::stream::{SplitAxis, StructuralOp};
use crate::model::{LogicalPx, LogicalRect, PhysicalPx, PhysicalRect};
use crate::state::RequestContext;

fn terminal() -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1000.0),
        height: PhysicalPx(600.0),
    }
}

fn center(r: LogicalRect) -> egui::Pos2 {
    point(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

/// attach 대기를 하나 시작한 mirror 장면에서 anchor surface 와 그 탭·pane, 그리고 레이아웃을 돌려준다.
fn pending_scene(
    op: impl FnOnce(u32) -> StructuralOp,
    check: impl FnOnce(
        &RequestContext,
        u32,
        crate::adapters::ui::LayoutContext,
        &crate::core::CoreState,
    ),
) {
    let (mut state, mut session) = crate::state::tests::test_mirror_state();
    let engine = session.borrow_mut();
    let core = &engine.read();
    let anchor = state.focused_surface_id(core).unwrap();
    let (sender, _queue) = tasty_remote::connection::channel();
    let view = Arc::new(());
    state
        .attach_pending
        .begin(
            core,
            &state.navigation,
            &op(anchor),
            sender.epoch(),
            Arc::downgrade(&view),
            std::time::Instant::now(),
        )
        .unwrap();
    // 실제 창처럼 pane 사각형을 같은 레이아웃 계산에서 받아 넘긴다.
    let pane_rects: Vec<(u32, PhysicalRect)> = state
        .surface_regions(core, terminal(), 1.0)
        .into_iter()
        .map(|(pane, rect, _)| (pane, rect))
        .collect();
    let layout = crate::adapters::ui::layout_context::build_layout_context(
        &state,
        core,
        &pane_rects,
        terminal(),
        1.0,
    );
    check(&state, anchor, layout, core);
}

fn assert_on_tab_without_surface(path: &ScopePath, core: &crate::core::CoreState, anchor: u32) {
    let tab = core.find_tab_for_surface(anchor).unwrap();
    let pane = core.find_pane_for_surface(anchor).unwrap();
    assert!(path.contains(&PopupScope::Tab(tab)), "{path:?}");
    assert!(path.contains(&PopupScope::Pane(pane)), "{path:?}");
    assert!(!path.contains(&PopupScope::Surface(anchor)), "{path:?}");
}

/// 대기 중인 새 탭이 보이면 그 pane 에 실제 surface 가 없지만 콘텐츠는 탭이다.
#[test]
fn the_content_of_a_pending_new_tab_is_on_the_tab() {
    pending_scene(
        |anchor| StructuralOp::NewTab {
            anchor_surface_id: anchor,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: None,
        },
        |_, anchor, layout, core| {
            assert!(
                layout.surface_rects.is_empty(),
                "대기 탭이 보이는 장면이어야 한다"
            );
            let pane = core.find_pane_for_surface(anchor).unwrap();
            let (_, content) = layout
                .tab_content_rects
                .iter()
                .find(|(id, _)| *id == pane)
                .unwrap();
            let content = LogicalRect {
                x: LogicalPx(content.min.x),
                y: LogicalPx(content.min.y),
                width: LogicalPx(content.width()),
                height: LogicalPx(content.height()),
            };
            assert_on_tab_without_surface(&path_at(&layout, center(content)), core, anchor);
        },
    );
}

/// 대기 중인 새 surface 분할에서 anchor 와 대기 자리 사이 분할선과 대기 자리는 탭 콘텐츠다.
#[test]
fn the_divider_and_the_half_of_a_pending_surface_split_are_on_the_tab() {
    pending_scene(
        |anchor| StructuralOp::SplitSurface {
            surface_id: anchor,
            direction: SplitAxis::Vertical,
            surface_kind: "terminal".into(),
            params: serde_json::json!({}),
            inherit_cwd: None,
        },
        |state, anchor, layout, core| {
            let placeholders = state.pending_regions(core, terminal(), 1.0);
            assert_eq!(placeholders.len(), 1);
            let pending = placeholders[0].1.to_logical(1.0);
            let kept = state
                .surface_rect_by_id(core, anchor, terminal(), 1.0)
                .unwrap()
                .to_logical(1.0);
            let kept_right = kept.x + kept.width;
            assert!(pending.x > kept_right, "분할선이 있는 분할이어야 한다");
            let divider = rect(kept_right, kept.y, pending.x - kept_right, kept.height);
            for at in [center(divider), center(pending)] {
                assert_on_tab_without_surface(&path_at(&layout, at), core, anchor);
            }
        },
    );
}
