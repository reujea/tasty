//! html·플러그인 surface의 포커스 띠. 띠 kind leaf의 위쪽 행을 `tasty_ui_widgets::SurfaceFocusEdge`로 칠한다.
//! 계약은 `docs/design/policies/focus.md`의 "Surface 포커스 표현" 절에 있다.
//!
//! 띠 행은 이 모듈만 칠한다. attention 링과 점유 링은 띠 leaf에서 행 아래만 칠하고, 행 안의 링은 위젯이
//! 비포커스 띠 → 링 → 켜진 띠 순서로 다시 칠한다. 그래서 링 층이 어디 있든 행 안 순서가 같다.
//! 띠 층은 `Order::Middle`이라 Foreground인 탭 바·host·plugin 팝업 셸보다 늘 아래다.

use crate::app::plugin_display::PluginDisplay;
use crate::core::AttentionKind;
use crate::model::PhysicalRect;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::state::surface_focus::FocusEdgeLeaf;

/// 이번 프레임의 띠 leaf를 모아 `state.surface_focus_edges`에 둔다. 탭 바가 탭과 높이를 고친 뒤,
/// chrome 패널·점유 링과 같은 pane 배치(`pane_rects`)로 부른다.
pub(crate) fn collect(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    pane_rects: &[(u32, PhysicalRect)],
    scale_factor: f32,
) {
    let theme = crate::theme::theme();
    let focused = state.focused_surface_id(engine);
    let mut out = Vec::new();
    if let Some(ws) = engine.workspace_at(state.active_workspace_index(engine)) {
        for &(pane_id, pane_rect) in pane_rects {
            let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                continue;
            };
            let Some(tab) = pane.tabs.get(state.navigation.tab_index(pane)) else {
                continue;
            };
            let content = state.tab_content_rect(pane_rect);
            for r in state.tab_surface_regions(engine, tab, content, scale_factor) {
                if !crate::state::surface_focus::reserves_edge(engine, r.id) {
                    continue;
                }
                let is_focused = focused == Some(r.id);
                let below = crate::state::surface_focus::below_edge(&theme, r.rect, scale_factor);
                out.push(FocusEdgeLeaf {
                    surface_id: r.id,
                    leaf: crate::adapters::ui::to_egui_rect(r.rect, scale_factor),
                    below: crate::adapters::ui::to_egui_rect(below, scale_factor),
                    rows: crate::state::surface_focus::edge_rows(&theme, r.rect, scale_factor),
                    focused: is_focused,
                    ring: row_ring(engine, &theme, r.id, is_focused),
                });
            }
        }
    }
    state.surface_focus_edges = out;
}

/// 띠 행 안에 보일 링. 지금 화면에서 위에 오는 점유 링(Foreground)을 attention 링보다 먼저 본다.
/// 포커스된 leaf의 완료 링은 행에 두지 않는다(완료는 포커스로 풀린다는 정본 규칙). 행 밖 링은 그대로다.
fn row_ring(
    engine: &EngineRead<'_>,
    theme: &tasty_type_appearance::theme::Theme,
    surface_id: u32,
    focused: bool,
) -> Option<egui::Stroke> {
    use crate::adapters::ui::surface_rings::{attention_ring, occupancy_mark};
    if let Some(mark) = occupancy_mark(engine, surface_id).filter(|m| m.edge) {
        return Some(tasty_ui_widgets::occupancy_edge_stroke(theme, mark.hard));
    }
    attention_ring(engine, surface_id)
        .filter(|kind| !(focused && *kind == AttentionKind::Completion))
        .map(|kind| tasty_ui_widgets::attention_edge_stroke(theme, kind.into()))
}

/// 띠 leaf의 링을 칠할 영역. 띠 leaf면 띠 행 아래, 아니면 `None`(leaf 전체)이다.
pub(crate) fn ring_clip(state: &MainViewState, surface_id: u32) -> Option<egui::Rect> {
    state
        .surface_focus_edges
        .iter()
        .find(|e| e.surface_id == surface_id)
        .map(|e| e.below)
}

/// 띠 kind webview chrome이 시작하는 논리 y. 띠 행 아래 끝이며 native 페이지의 위 끝과 같은 물리 경계다.
/// 띠 leaf가 아니면 `None`이다.
pub(crate) fn chrome_top(state: &MainViewState, surface_id: u32) -> Option<f32> {
    ring_clip(state, surface_id).map(|below| below.top())
}

/// GPU가 이번 프레임에 합성할 plugin 팝업 본문 하나. 본문이 있고 primitive가 남은 팝업만 든다.
/// `clips`는 팝업 안 논리 px(`ppp` 기준)의 primitive 잘림 영역이다.
#[derive(Clone, Debug, Default)]
pub(crate) struct ComposedPopup {
    pub(crate) ppp: f32,
    pub(crate) clips: Vec<egui::Rect>,
}

/// 띠를 칠한다. plugin 팝업을 그린 뒤 부른다. host 팝업이 plugin 팝업 위일 때는 plugin 본문이 egui보다
/// 먼저 합성되므로, 이번 프레임에 실제로 합성될 본문이 칠하는 영역을 띠 행에서 뺀다.
pub(crate) fn paint(
    ctx: &egui::Context,
    state: &MainViewState,
    plugin_manager: Option<PluginDisplay<'_>>,
    composed: &std::collections::HashMap<u64, ComposedPopup>,
    host_popup_on_top: bool,
    scale_factor: f32,
) {
    if state.surface_focus_edges.is_empty() {
        return;
    }
    let theme = crate::theme::theme();
    let bodies = match plugin_manager {
        Some(mgr) if host_popup_on_top => composited_bodies(
            &state.plugin_mesh_popup_regions,
            composed,
            |iid| mgr.popup_mesh_frame(iid).is_some(),
            scale_factor,
        ),
        _ => Vec::new(),
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("surface_focus_edges"),
    ));
    for edge in &state.surface_focus_edges {
        let row = edge.leaf.with_max_y(edge.below.top());
        let widget = tasty_ui_widgets::SurfaceFocusEdge {
            focused: edge.focused,
            ring: edge.ring,
            corner_radius: tasty_type_geometry::length::LogicalPx(0.0),
        };
        for piece in edge_pieces(row, host_popup_on_top, &bodies) {
            widget.paint_rows(&painter.with_clip_rect(piece), &theme, edge.leaf, edge.rows);
        }
    }
}

/// 띠 행에서 칠할 조각. host 팝업이 위일 때만 먼저 합성된 plugin 본문 영역을 뺀다. 아니면 plugin 본문이
/// egui 뒤에 합성되어 띠를 덮으므로 행을 그대로 칠한다.
fn edge_pieces(row: egui::Rect, host_popup_on_top: bool, bodies: &[egui::Rect]) -> Vec<egui::Rect> {
    if host_popup_on_top {
        subtract(row, bodies)
    } else {
        vec![row]
    }
}

/// 합성될 plugin 팝업 본문이 칠하는 영역(host 논리 px). 프레임이 있고 GPU가 본문을 가진 팝업에서
/// primitive 잘림 영역을 팝업 영역 안으로 자른 것만 든다. 합성 경로와 같은 평행이동·자르기다.
fn composited_bodies(
    regions: &[(u64, PhysicalRect)],
    composed: &std::collections::HashMap<u64, ComposedPopup>,
    has_frame: impl Fn(u64) -> bool,
    scale_factor: f32,
) -> Vec<egui::Rect> {
    let mut out = Vec::new();
    for (iid, region) in regions {
        let Some(body) = composed.get(iid).filter(|_| has_frame(*iid)) else {
            continue;
        };
        if body.ppp <= 0.0 {
            continue;
        }
        let bounds = crate::adapters::ui::to_egui_rect(*region, body.ppp);
        for clip in &body.clips {
            let local = clip.translate(bounds.min.to_vec2()).intersect(bounds);
            if !local.is_positive() {
                continue;
            }
            let logical = tasty_type_geometry::rect::LogicalRect {
                x: tasty_type_geometry::length::LogicalPx(local.min.x),
                y: tasty_type_geometry::length::LogicalPx(local.min.y),
                width: tasty_type_geometry::length::LogicalPx(local.width()),
                height: tasty_type_geometry::length::LogicalPx(local.height()),
            };
            out.push(crate::adapters::ui::to_egui_rect(
                logical.to_physical(body.ppp),
                scale_factor,
            ));
        }
    }
    out
}

/// `rect`에서 `holes`를 뺀 나머지를 겹치지 않는 사각형으로 나눈다.
fn subtract(rect: egui::Rect, holes: &[egui::Rect]) -> Vec<egui::Rect> {
    let mut pieces = vec![rect];
    for hole in holes {
        let mut next = Vec::new();
        for p in pieces {
            let cut = p.intersect(*hole);
            if !cut.is_positive() {
                next.push(p);
                continue;
            }
            let parts = [
                egui::Rect::from_min_max(p.min, egui::pos2(p.max.x, cut.min.y)),
                egui::Rect::from_min_max(egui::pos2(p.min.x, cut.max.y), p.max),
                egui::Rect::from_min_max(
                    egui::pos2(p.min.x, cut.min.y),
                    egui::pos2(cut.min.x, cut.max.y),
                ),
                egui::Rect::from_min_max(
                    egui::pos2(cut.max.x, cut.min.y),
                    egui::pos2(p.max.x, cut.max.y),
                ),
            ];
            next.extend(parts.into_iter().filter(|r| r.is_positive()));
        }
        pieces = next;
    }
    pieces
}

#[cfg(test)]
mod scene_tests;

#[cfg(test)]
mod tests {
    use super::{ComposedPopup, composited_bodies, edge_pieces, subtract};
    use crate::model::{PhysicalPx, PhysicalRect};

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1))
    }

    fn area(rects: &[egui::Rect]) -> f32 {
        rects.iter().map(|r| r.area()).sum()
    }

    #[test]
    fn a_body_over_the_row_leaves_the_sides() {
        let row = r(0.0, 0.0, 100.0, 2.0);
        let pieces = subtract(row, &[r(40.0, -10.0, 60.0, 50.0)]);
        assert_eq!(area(&pieces), 160.0);
        assert!(pieces.iter().all(|p| p.right() <= 40.0 || p.left() >= 60.0));
    }

    #[test]
    fn a_body_that_misses_the_row_keeps_it_whole() {
        let row = r(0.0, 0.0, 100.0, 2.0);
        assert_eq!(subtract(row, &[r(0.0, 5.0, 100.0, 9.0)]), vec![row]);
        assert_eq!(subtract(row, &[]), vec![row]);
    }

    #[test]
    fn a_body_covering_the_lower_row_keeps_the_upper_one() {
        let row = r(0.0, 0.0, 100.0, 2.0);
        let pieces = subtract(row, &[r(-5.0, 1.0, 105.0, 30.0)]);
        assert_eq!(pieces, vec![r(0.0, 0.0, 100.0, 1.0)]);
    }

    /// host 팝업이 plugin 팝업 위면 먼저 합성된 본문 자리를 띠 행에서 비운다. 아래면 본문이 띠 뒤에
    /// 합성되어 덮으므로 행을 그대로 칠한다.
    #[test]
    fn the_row_opens_only_under_a_host_popup_on_top() {
        let row = r(0.0, 0.0, 100.0, 2.0);
        let body = [r(40.0, -10.0, 60.0, 50.0)];
        let under = edge_pieces(row, true, &body);
        assert_eq!(area(&under), 160.0);
        assert!(under.iter().all(|p| p.right() <= 40.0 || p.left() >= 60.0));
        assert_eq!(edge_pieces(row, false, &body), vec![row]);
    }

    /// 비우는 자리는 팝업 영역 전체가 아니라 합성된 primitive의 잘림 영역이다. 팝업 영역으로 자르고
    /// 팝업 배율에서 host 배율로 옮긴다. 본문이 없거나 프레임이 없는 팝업은 비우지 않는다.
    #[test]
    fn the_hole_is_the_composited_clip_inside_the_popup() {
        let region = PhysicalRect {
            x: PhysicalPx(100.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(200.0),
            height: PhysicalPx(100.0),
        };
        let regions = [(7_u64, region), (8_u64, region)];
        let composed = std::collections::HashMap::from([(
            7_u64,
            ComposedPopup {
                ppp: 2.0,
                clips: vec![r(0.0, 0.0, 20.0, 10.0), r(90.0, 40.0, 500.0, 500.0)],
            },
        )]);
        let bodies = composited_bodies(&regions, &composed, |_| true, 1.0);
        assert_eq!(
            bodies,
            vec![r(100.0, 0.0, 140.0, 20.0), r(280.0, 80.0, 300.0, 100.0)],
            "clips in physical px at host scale 1"
        );
        assert!(
            composited_bodies(&regions, &composed, |_| false, 1.0).is_empty(),
            "no frame"
        );
        let none = std::collections::HashMap::new();
        assert!(
            composited_bodies(&regions, &none, |_| true, 1.0).is_empty(),
            "no body"
        );
    }
}
