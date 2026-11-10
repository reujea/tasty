//! 의존 관계를 색과 파선 패턴으로 함께 구분한다. 화살촉은 의존하는 작업을 가리킨다.
//! 꺾인선·모서리·파선·화살촉은 본체 캔버스와 같은 공통 그래프 뷰 함수로 그린다.

use tasty_dag_layout::Orientation;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    EdgeInk, graph_orthogonalize, graph_paint_arrow, graph_paint_edge_line, graph_round_corners,
};

use super::Rel;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 엣지 견본 한 줄의 그리기 값. 선택 상태가 바꾸는 것은 굵기와 선 잉크뿐이다.
pub(super) struct EdgeLook {
    /// 견본 이름의 색. 관계의 색이다.
    pub color: egui::Color32,
    /// 선과 화살표의 색.
    pub ink: egui::Color32,
    pub dash: Option<&'static [f32]>,
    pub width: f32,
}

/// 디자인 `EdgeSpecimen` — 시안 뷰박스 120×56 안의 elbow 한 줄 + 이름 + 라벨.
///
/// `node_box` 와 같은 이유로 한 번의 `allocate_exact_size` 로 자리를 잡는다.
pub(super) fn specimen(
    ui: &mut egui::Ui,
    theme: &Theme,
    stroke: EdgeLook,
    key: &str,
    label: Option<&str>,
) {
    let gap = theme.spacing_sm.value();
    let caption = theme.font_size_caption.value();
    let font = egui::FontId::proportional(caption);
    let line_h = super::node::row_height(ui, &font);
    let lines = if label.is_some() { 2.0 } else { 1.0 };
    // 시안 120×56: 카드 폭에서 레이어 간격과 여백을 뺀 값 / 카드 높이 + 8.
    let w =
        theme.dag_node_width().value() - theme.dag_layer_gap().value() - theme.spacing_lg.value();
    let h = theme.dag_node_height().value() + gap;
    let (outer, _) = ui.allocate_exact_size(
        egui::vec2(w, h + gap + line_h * lines + gap),
        egui::Sense::hover(),
    );
    let rect = egui::Rect::from_min_size(outer.min, egui::vec2(w, h));
    let inset = theme.spacing_md.value();
    let s = egui::pos2(rect.min.x + inset, rect.min.y + gap);
    let t = egui::pos2(rect.max.x - inset, rect.max.y - gap);
    let color = stroke.ink;
    let ortho = graph_orthogonalize(&[s, t], Orientation::TopDown);
    let path = graph_round_corners(&ortho, theme.dag_edge_corner_radius().value());
    let ink = EdgeInk {
        color,
        width: stroke.width,
        dash: stroke.dash,
    };
    graph_paint_edge_line(ui.painter(), &path, &ink, 1.0);
    graph_paint_arrow(
        ui.painter(),
        theme.dag_edge_arrow_size().value(),
        color,
        &ortho,
    );
    let cx = outer.center().x;
    ui.painter().text(
        egui::pos2(cx, rect.max.y + gap),
        egui::Align2::CENTER_TOP,
        key,
        egui::FontId::monospace(caption),
        stroke.color,
    );
    if let Some(label) = label {
        ui.painter().text(
            egui::pos2(cx, rect.max.y + gap + line_h),
            egui::Align2::CENTER_TOP,
            label,
            font,
            theme.text_muted().to_egui(),
        );
    }
}

fn edge_specimen(ui: &mut egui::Ui, theme: &Theme, rel: Rel) {
    let stroke = EdgeLook {
        color: rel.color(theme).to_egui(),
        ink: rel.color(theme).to_egui(),
        dash: rel.dash(),
        width: theme.dag_edge_width().value(),
    };
    specimen(ui, theme, stroke, rel.key(), Some(rel.label()));
}

/// `edges` 섹션 Spec — 관계 5 종.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for rel in Rel::ALL {
            edge_specimen(ui, theme, rel);
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("routing", "orthogonal, 4px elbow"),
            ("width", "1px"),
            ("fallback dash", "6 3"),
            ("reduce dash", "2 3"),
            ("binding dash", "8 2 2 2"),
            ("transition dash", "10 4"),
            ("same pair", "binding replaces depends_on"),
            (
                "detail label",
                "binding \u{2192} \u{201c}binds input\u{201d} \u{b7} transition \u{2192} \u{201c}transition\u{201d}",
            ),
            ("arrow", "8px triangle at target"),
            ("selected node", "its edges take the accent"),
        ],
        &[
            TokenChip::new(
                "dag-edge-depends",
                "depends_on",
                theme.dag_edge_depends().to_egui(),
            ),
            TokenChip::new(
                "dag-edge-fallback",
                "fallback",
                theme.dag_edge_fallback().to_egui(),
            ),
            TokenChip::new(
                "dag-edge-reduce",
                "reduce",
                theme.dag_edge_reduce().to_egui(),
            ),
            TokenChip::new(
                "dag-edge-binding",
                "binding \u{2192} accent-data",
                theme.dag_edge_binding().to_egui(),
            ),
            TokenChip::new(
                "dag-edge-transition",
                "transition \u{2192} accent-route",
                theme.dag_edge_transition().to_egui(),
            ),
            TokenChip::new("dag-edge-dim", "dead path", theme.dag_edge_dim().to_egui()),
            TokenChip::new(
                "dag-edge-highlight",
                "selected",
                theme.dag_edge_highlight().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "When a pair has both depends_on and binding, only the binding is drawn \u{2014} a \
         binding already implies the order. A one_of binding draws one binding edge per source.",
    );
    spec::do_(
        ui,
        theme,
        "Dim the dead path: an edge leaving a failed/cancelled task, or entering a skipped one, \
         takes dag-edge-dim while its node recedes. The failure stays loud; everything it stranded recedes.",
    );
}
