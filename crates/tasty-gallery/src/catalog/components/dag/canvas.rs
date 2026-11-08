//! 격자 바탕, 그래프, 조작 도구를 겹쳐 그리는 DAG 예제.
//! 열릴 때 그래프를 화면에 맞추며 선택만 지원한다. 팬·줌 제스처는 본체에서 처리한다.

use tasty_dag_layout::{GraphLayout, Orientation};
use tasty_type_appearance::theme::Theme;

use super::node::{self, Lod, NodeVis};
use super::{Graph, Rel};
use super::{chrome, edges};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 줌 하한 / auto-fit 상한 (`Z_MIN` / fit cap).
const ZOOM_MIN: f32 = 0.2;
const ZOOM_FIT_MAX: f32 = 1.0;

/// 캔버스가 그래프 층에 적용하는 변환.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub origin: egui::Vec2,
    pub zoom: f32,
}

impl Transform {
    fn point(&self, x: f32, y: f32, rect: egui::Rect) -> egui::Pos2 {
        rect.min + self.origin + egui::vec2(x, y) * self.zoom
    }
}

/// 노드와 엣지 꺾임점을 모두 감싸는 경계. 원점이 0이 아닐 수 있어
/// GraphLayout의 폭·높이만으로는 중앙 위치를 구할 수 없다.
pub fn graph_bounds(layout: &GraphLayout, theme: &Theme) -> egui::Rect {
    let (nw, nh) = (
        theme.dag_node_width().value(),
        theme.dag_node_height().value(),
    );
    let mut b: Option<egui::Rect> = None;
    let mut grow = |r: egui::Rect| {
        b = Some(match b {
            Some(cur) => cur.union(r),
            None => r,
        });
    };
    for n in &layout.nodes {
        grow(egui::Rect::from_min_size(
            egui::pos2(n.x.value(), n.y.value()),
            egui::vec2(nw, nh),
        ));
    }
    for e in &layout.edges {
        for (x, y) in &e.points {
            grow(egui::Rect::from_min_size(
                egui::pos2(x.value(), y.value()),
                egui::Vec2::ZERO,
            ));
        }
    }
    b.unwrap_or(egui::Rect::ZERO)
}

/// 시안의 `fit()` — 여백 16 을 빼고 맞춘 뒤 100% 로 자른다.
pub fn fit(rect: egui::Rect, layout: &GraphLayout, theme: &Theme) -> Transform {
    let pad = theme.dag_canvas_padding().value();
    let b = graph_bounds(layout, theme);
    let (gw, gh) = (b.width(), b.height());
    if gw <= 0.0 || gh <= 0.0 {
        return Transform {
            origin: egui::vec2(pad, pad),
            zoom: ZOOM_FIT_MAX,
        };
    }
    let zoom = ((rect.width() - pad * 2.0) / gw)
        .min((rect.height() - pad * 2.0) / gh)
        .clamp(ZOOM_MIN, ZOOM_FIT_MAX);
    Transform {
        origin: egui::vec2(
            (rect.width() - gw * zoom) / 2.0 - b.min.x * zoom,
            ((rect.height() - gh * zoom) / 2.0).max(pad) - b.min.y * zoom,
        ),
        zoom,
    }
}

/// 점 격자 바탕 — `radial-gradient` 1px dot / 16px gap.
fn paint_dot_grid(painter: &egui::Painter, theme: &Theme, rect: egui::Rect) {
    let gap = theme.dag_canvas_dot_gap().value();
    let r = theme.dag_canvas_dot_size().value();
    let color = theme.dag_canvas_dot().to_egui();
    let mut y = rect.min.y + gap;
    while y < rect.max.y {
        let mut x = rect.min.x + gap;
        while x < rect.max.x {
            painter.circle_filled(egui::pos2(x, y), r, color);
            x += gap;
        }
        y += gap;
    }
}

fn node_rect(
    t: &Transform,
    rect: egui::Rect,
    p: &tasty_dag_layout::NodePosition,
    theme: &Theme,
) -> egui::Rect {
    egui::Rect::from_min_size(
        t.point(p.x.value(), p.y.value(), rect),
        egui::vec2(
            theme.dag_node_width().value() * t.zoom,
            theme.dag_node_height().value() * t.zoom,
        ),
    )
}

/// 캔버스 한 장. `selected` 는 호출자가 소유한 선택 상태다.
// 이유: 본체 canvas::draw_canvas와 같은 인자 구성을 유지해 비교하기 쉽게 한다.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    graph: &Graph,
    dir: Orientation,
    selected: &mut Option<String>,
    minimap: bool,
    // 팝업 상세에서는 뒤로 가기 바에 조작 도구가 있으므로 false로 둔다.
    zoom_cluster: bool,
    // 탭 surface는 포커스 바탕을 넘긴다. 워크스페이스 팝업은 surface가 아니라 None이다.
    bed: Option<egui::Color32>,
) {
    let layout = super::layout(graph, theme, dir);
    let t = fit(rect, &layout, theme);
    let lod = Lod::of(t.zoom);
    let top_down = dir == Orientation::TopDown;

    ui.painter().rect_filled(
        rect,
        0.0,
        bed.unwrap_or_else(|| theme.dag_canvas_bg().to_egui()),
    );
    paint_dot_grid(ui.painter(), theme, rect);

    let resp = ui.interact(
        rect,
        ui.id().with(("dag_canvas", &graph.id)),
        egui::Sense::click(),
    );
    let pointer = resp.hover_pos();

    let mut layer = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layer_id(ui.layer_id()),
    );
    layer.set_clip_rect(rect);

    let rels: Vec<(usize, usize, Rel)> = graph.edges();
    let edge_w = theme.dag_edge_width().value();
    let arrow = theme.dag_edge_arrow_size().value();
    let radius = theme.dag_edge_corner_radius().value() * t.zoom;
    for route in &layout.edges {
        let rel = rels
            .iter()
            .find(|(f, to, _)| *f == route.from && *to == route.to)
            .map(|(_, _, r)| *r)
            .unwrap_or(Rel::DependsOn);
        let hot = selected
            .as_deref()
            .is_some_and(|s| s == graph.nodes[route.from].id || s == graph.nodes[route.to].id);
        let color = if hot {
            theme.dag_edge_highlight()
        } else if graph.edge_is_dim(route.from, route.to) {
            theme.dag_edge_dim()
        } else {
            rel.color(theme)
        }
        .to_egui();
        let raw: Vec<egui::Pos2> = route
            .points
            .iter()
            .map(|(x, y)| t.point(x.value(), y.value(), rect))
            .collect();
        let path = edges::round_corners(&edges::orthogonalize(&raw, top_down), radius);
        edges::paint_path(layer.painter(), &path, color, edge_w, rel.dash());
        if let Some(tip) = path.last() {
            edges::paint_arrow(
                layer.painter(),
                *tip,
                top_down,
                arrow * t.zoom,
                theme.border_width.value(),
                color,
            );
        }
    }

    let mut hit: Option<String> = None;
    for (i, pos) in layout.nodes.iter().enumerate() {
        let r = node_rect(&t, rect, pos, theme);
        let n = &graph.nodes[i];
        let hovered = pointer.is_some_and(|p| r.contains(p));
        if hovered {
            hit = Some(n.id.clone());
        }
        node::paint_card(
            &mut layer,
            theme,
            r,
            n,
            NodeVis {
                lod,
                zoom: t.zoom,
                selected: selected.as_deref() == Some(n.id.as_str()),
                hovered,
                dimmed: n.status.is_dim(),
            },
        );
    }
    if resp.clicked() {
        *selected = hit;
    }

    if let Some(cycle) = &graph.cycle {
        chrome::paint_cycle_banner(&mut layer, theme, rect, cycle);
    }
    chrome::paint_canvas_chrome(
        &mut layer,
        theme,
        rect,
        graph,
        &layout,
        t,
        minimap,
        zoom_cluster,
        lod,
    );
}

/// Tight 예제 영역에 지정한 높이로 캔버스를 그린다.
pub fn stage(
    ui: &mut egui::Ui,
    theme: &Theme,
    graph: &Graph,
    dir: Orientation,
    height: f32,
    minimap: bool,
) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let key = ui.id().with(("dag_sel", &graph.id));
        let mut sel: Option<String> = ui.data(|d| d.get_temp(key)).unwrap_or(None);
        paint(ui, theme, rect, graph, dir, &mut sel, minimap, true, None);
        ui.data_mut(|d| d.insert_temp(key, sel));
    });
}

/// 캔버스 무대의 표준 높이 — 시안 460.
fn tall(theme: &Theme) -> f32 {
    theme.dag_popup_height().value()
}

/// `canvas` 섹션 Spec — 읽기 전용 관찰 캔버스.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::build_dag();
    stage(ui, theme, &graph, Orientation::TopDown, tall(theme), true);
    spec::meta(
        ui,
        theme,
        &[
            ("node box", "168 × 48 (fixed at every zoom)"),
            ("layer gap", "32 along flow · 24 across"),
            ("zoom", "20% – 150%, 10% steps"),
            ("fit", "auto on open, capped at 100%"),
            ("background", "dot grid · 16px"),
            ("poll", "0.5s — colour only, never re-layout"),
        ],
        &[
            TokenChip::new(
                "dag-canvas-bg",
                "canvas bed",
                theme.dag_canvas_bg().to_egui(),
            ),
            TokenChip::new(
                "dag-canvas-dot",
                "grid mark",
                theme.dag_canvas_dot().to_egui(),
            ),
            TokenChip::without_color("dag-layer-gap", "32 flow depth"),
            TokenChip::without_color("dag-sibling-gap", "24 across"),
            TokenChip::without_color("dag-edge-corner-radius", "elbow"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Position stability is the contract that makes a 0.5s poll survivable: layout is computed \
         from ids + dependency edges only — never from status, duration or counts — so a task \
         turning running → succeeded repaints one card and moves nothing.",
    );
    spec::dont(
        ui,
        theme,
        "Never add a connect handle, a port dot, or a drag-to-move affordance. Anything that looks \
         editable promises an edit the host cannot honour — DAGs are built by agents over CLI/IPC.",
    );
}

/// LOD Spec 안에 들어가는 55 노드 무대.
pub fn dense_stage(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::dense_dag();
    stage(
        ui,
        theme,
        &graph,
        Orientation::TopDown,
        theme.measure_sm.value(),
        true,
    );
}

/// 사이클 Spec 안에 들어가는 무대 — 배너가 떠도 그래프는 계속 그린다.
pub fn cycle_stage(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::cycle_dag();
    stage(
        ui,
        theme,
        &graph,
        Orientation::TopDown,
        theme.measure_sm.value(),
        false,
    );
}
