//! 그래프 뷰 하나의 이동·배율·선택과 레이아웃 캐시.
//! 레이아웃 캐시는 식별자·간선·방향·치수를 비교하며 노드 상태만 바뀌면 좌표를 유지한다.
//! 조회 주기와 데이터 소스는 호출자가 가진다. 이 상태는 시간을 모른다.

use std::hash::{Hash as _, Hasher as _};
use std::rc::Rc;

use tasty_dag_layout::{GraphLayout, LayoutConfig, Orientation, layout_dag};

use super::model::{GraphData, GraphEdgeData as _, GraphNodeData as _};

/// 줌 범위와 단위. 길이가 아니라 배율이라 `LogicalPx` 대상이 아니다.
pub const ZOOM_MIN: f32 = 0.2;
pub const ZOOM_MAX: f32 = 1.5;
pub const ZOOM_STEP: f32 = 0.1;
/// `fit` 의 상한 — 작은 그래프를 확대하지 않는다.
pub const ZOOM_FIT_MAX: f32 = 1.0;

/// 노드 LOD 3단계. 박스 크기는 tier 와 무관하게 고정이고 **내용만** 바뀐다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lod {
    /// 이름 + 상태 라벨 + duration.
    Full,
    /// 글리프 + 이름.
    Compact,
    /// 상태 채움만.
    Block,
}

impl Lod {
    pub fn of(zoom: f32) -> Self {
        if zoom >= 0.7 {
            Lod::Full
        } else if zoom >= 0.4 {
            Lod::Compact
        } else {
            Lod::Block
        }
    }
}

/// 방향 이름. 캐시 키와 자동 맞춤 키에 들어간다.
pub fn orientation_key(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::LeftRight => "left_right",
        Orientation::TopDown => "top_down",
    }
}

/// 캐시된 좌표를 읽으며 뷰 상태도 바꿀 수 있도록 Rc로 공유한다.
struct CachedLayout {
    key: u64,
    layout: Rc<GraphLayout>,
}

/// 그래프 뷰 하나의 휘발성 상태. surface와 팝업이 같은 상태를 쓴다.
pub struct GraphView {
    pub zoom: f32,
    /// 그래프 원점의 화면 오프셋(캔버스 좌상단 기준, logical px).
    pub offset: egui::Vec2,
    /// 선택된 노드 식별자.
    pub selected: Option<String>,
    /// 자동 맞춤을 적용한 그래프·방향·뷰포트 조합.
    fit_key: Option<String>,
    layout: Option<CachedLayout>,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: egui::Vec2::ZERO,
            selected: None,
            fit_key: None,
            layout: None,
        }
    }
}

impl GraphView {
    /// 현재 데이터에 대한 레이아웃. 그래프 **모양**이 그대로면 캐시를 돌려준다.
    pub fn layout<G: GraphData>(
        &mut self,
        graph: Option<&G>,
        cfg: &LayoutConfig,
    ) -> Rc<GraphLayout> {
        let key = shape_key(graph, cfg);
        if self.layout.as_ref().is_none_or(|c| c.key != key) {
            let layout = Rc::new(compute_layout(graph, cfg));
            self.layout = Some(CachedLayout { key, layout });
        }
        Rc::clone(
            &self
                .layout
                .as_ref()
                .expect("layout cache filled above")
                .layout,
        )
    }

    /// 같은 그래프·방향·32px 단위 뷰포트에서는 자동 맞춤을 한 번만 수행한다.
    pub fn take_fit(
        &mut self,
        graph_key: &str,
        orientation: Orientation,
        viewport: egui::Vec2,
    ) -> bool {
        let key = format!(
            "{graph_key}:{}:{}:{}",
            orientation_key(orientation),
            (viewport.x / 32.0).round() as i32,
            (viewport.y / 32.0).round() as i32
        );
        if self.fit_key.as_deref() == Some(key.as_str()) {
            return false;
        }
        self.fit_key = Some(key);
        true
    }

    /// 줌을 한 단계 바꾸되 `anchor`(캔버스 로컬 좌표) 아래의 그래프 점을 고정한다.
    pub fn zoom_by(&mut self, steps: f32, anchor: egui::Vec2) {
        let next = (self.zoom + steps * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX);
        // 부동소수 누적으로 88.00001% 같은 값이 되지 않게 10% 격자에 맞춘다.
        let next = (next * 10.0).round() / 10.0;
        if (next - self.zoom).abs() < f32::EPSILON {
            return;
        }
        self.offset = anchor - (anchor - self.offset) / self.zoom * next;
        self.zoom = next;
    }

    /// 그래프 전체가 들어오도록 줌/오프셋을 맞춘다.
    pub fn fit(&mut self, graph_size: egui::Vec2, viewport: egui::Vec2, padding: f32) {
        if graph_size.x <= 0.0 || graph_size.y <= 0.0 {
            return;
        }
        let usable = viewport - egui::vec2(padding * 2.0, padding * 2.0);
        let k = (usable.x / graph_size.x)
            .min(usable.y / graph_size.y)
            .clamp(ZOOM_MIN, ZOOM_FIT_MAX);
        self.zoom = k;
        self.offset = egui::vec2(
            ((viewport.x - graph_size.x * k) / 2.0).max(padding),
            ((viewport.y - graph_size.y * k) / 2.0).max(padding),
        );
    }
}

fn compute_layout<G: GraphData>(graph: Option<&G>, cfg: &LayoutConfig) -> GraphLayout {
    let Some(graph) = graph else {
        return GraphLayout::default();
    };
    let ids: Vec<String> = graph
        .nodes()
        .iter()
        .map(|n| n.node_id().to_string())
        .collect();
    let edges: Vec<(usize, usize)> = graph.edges().iter().map(|e| e.ends()).collect();
    layout_dag(&ids, &edges, cfg)
}

/// 상태를 제외한 레이아웃 캐시 키.
pub fn shape_key<G: GraphData>(graph: Option<&G>, cfg: &LayoutConfig) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    orientation_key(cfg.orientation).hash(&mut h);
    for px in [
        cfg.node_size.0,
        cfg.node_size.1,
        cfg.layer_gap,
        cfg.sibling_gap,
        cfg.component_gap,
    ] {
        px.value().to_bits().hash(&mut h);
    }
    if let Some(graph) = graph {
        graph.graph_key().hash(&mut h);
        for n in graph.nodes() {
            n.node_id().hash(&mut h);
        }
        for e in graph.edges() {
            let (from, to) = e.ends();
            from.hash(&mut h);
            to.hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_tiers_follow_zoom_thresholds() {
        assert_eq!(Lod::of(1.0), Lod::Full);
        assert_eq!(Lod::of(0.7), Lod::Full);
        assert_eq!(Lod::of(0.69), Lod::Compact);
        assert_eq!(Lod::of(0.4), Lod::Compact);
        assert_eq!(Lod::of(0.39), Lod::Block);
    }

    #[test]
    fn zoom_snaps_to_ten_percent_steps_and_clamps() {
        let mut v = GraphView::default();
        v.zoom_by(-1.0, egui::Vec2::ZERO);
        assert!((v.zoom - 0.9).abs() < 1e-6, "{}", v.zoom);
        for _ in 0..50 {
            v.zoom_by(-1.0, egui::Vec2::ZERO);
        }
        assert!((v.zoom - ZOOM_MIN).abs() < 1e-6, "{}", v.zoom);
        for _ in 0..50 {
            v.zoom_by(1.0, egui::Vec2::ZERO);
        }
        assert!((v.zoom - ZOOM_MAX).abs() < 1e-6, "{}", v.zoom);
    }

    #[test]
    fn zoom_keeps_the_anchor_point_fixed() {
        let mut v = GraphView {
            offset: egui::vec2(10.0, 20.0),
            ..GraphView::default()
        };
        let anchor = egui::vec2(100.0, 50.0);
        let before = (anchor - v.offset) / v.zoom;
        v.zoom_by(2.0, anchor);
        let after = (anchor - v.offset) / v.zoom;
        assert!((before - after).length() < 1e-3, "{before:?} vs {after:?}");
    }

    #[test]
    fn fit_never_magnifies_beyond_one_hundred_percent() {
        let mut v = GraphView::default();
        v.fit(egui::vec2(50.0, 20.0), egui::vec2(800.0, 600.0), 16.0);
        assert!(v.zoom <= ZOOM_FIT_MAX + 1e-6, "{}", v.zoom);
    }

    #[test]
    fn fit_runs_once_per_dag_direction_and_viewport_bucket() {
        let mut v = GraphView::default();
        let vp = egui::vec2(800.0, 600.0);
        let (lr, td) = (Orientation::LeftRight, Orientation::TopDown);
        assert!(v.take_fit("d:a", lr, vp));
        assert!(!v.take_fit("d:a", lr, vp));
        assert!(!v.take_fit("d:a", lr, vp + egui::vec2(1.0, 0.0)));
        assert!(v.take_fit("d:a", td, vp));
        assert!(v.take_fit("d:b", td, vp));
        assert!(v.take_fit("d:b", td, vp + egui::vec2(200.0, 0.0)));
    }
}
