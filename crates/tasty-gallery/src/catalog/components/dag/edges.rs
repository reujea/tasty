//! 의존 관계를 색과 파선 패턴으로 함께 구분한다. 화살촉은 의존하는 작업을 가리킨다.

use tasty_type_appearance::theme::Theme;

use super::Rel;
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 코너 하나를 이차 베지어로 몇 조각 내어 근사할지. egui 에는 SVG `Q` 가 없다.
const CORNER_STEPS: usize = 6;

/// 엔진이 남긴 대각선 구간을 흐름 방향에 맞춘 직교 경로로 바꾼다.
pub fn orthogonalize(points: &[egui::Pos2], top_down: bool) -> Vec<egui::Pos2> {
    let mut out: Vec<egui::Pos2> = Vec::with_capacity(points.len() * 2);
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if out.last() != Some(&a) {
            out.push(a);
        }
        let aligned = (a.x - b.x).abs() < 1.0 || (a.y - b.y).abs() < 1.0;
        if !aligned {
            if top_down {
                let m = (a.y + b.y) / 2.0;
                out.push(egui::pos2(a.x, m));
                out.push(egui::pos2(b.x, m));
            } else {
                let m = (a.x + b.x) / 2.0;
                out.push(egui::pos2(m, a.y));
                out.push(egui::pos2(m, b.y));
            }
        }
        out.push(b);
    }
    out
}

/// 직교 폴리라인의 각 꺾임을 반경 `r` 로 둥글린다 (디자인 `Q` 코너).
pub fn round_corners(points: &[egui::Pos2], r: f32) -> Vec<egui::Pos2> {
    if points.len() < 3 || r <= 0.0 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for i in 1..points.len() - 1 {
        let (prev, cur, next) = (points[i - 1], points[i], points[i + 1]);
        let in_len = (cur - prev).length();
        let out_len = (next - cur).length();
        let rr = r.min(in_len / 2.0).min(out_len / 2.0);
        if rr <= 0.5 {
            out.push(cur);
            continue;
        }
        let a = cur - (cur - prev).normalized() * rr;
        let b = cur + (next - cur).normalized() * rr;
        out.push(a);
        for s in 1..CORNER_STEPS {
            let t = s as f32 / CORNER_STEPS as f32;
            let u = 1.0 - t;
            out.push(egui::pos2(
                u * u * a.x + 2.0 * u * t * cur.x + t * t * b.x,
                u * u * a.y + 2.0 * u * t * cur.y + t * t * b.y,
            ));
        }
        out.push(b);
    }
    out.push(points[points.len() - 1]);
    out
}

/// 디자인 `elbow(s, t, td, r)` — 두 점을 잇는 둥근 직교 경로.
pub fn elbow(s: egui::Pos2, t: egui::Pos2, top_down: bool, r: f32) -> Vec<egui::Pos2> {
    round_corners(&orthogonalize(&[s, t], top_down), r)
}

/// 폴리라인 한 줄. `dash` 가 있으면 SVG `stroke-dasharray` 처럼 선·틈 길이를 번갈아 끊어 그린다.
pub fn paint_path(
    painter: &egui::Painter,
    points: &[egui::Pos2],
    color: egui::Color32,
    width: f32,
    dash: Option<&[f32]>,
) {
    if points.len() < 2 {
        return;
    }
    let stroke = egui::Stroke::new(width, color);
    match dash {
        None => painter.add(egui::Shape::line(points.to_vec(), stroke)),
        Some(pattern) => {
            let on: Vec<f32> = pattern.iter().step_by(2).copied().collect();
            let off: Vec<f32> = pattern.iter().skip(1).step_by(2).copied().collect();
            painter.add(egui::Shape::Vec(egui::Shape::dashed_line_with_offset(
                points, stroke, &on, &off, 0.0,
            )))
        }
    };
}

/// 도착점의 삼각 화살촉. `size` = `--tasty-dag-edge-arrow-size`(8).
///
/// 디자인 폴리곤은 밑변 8 · 높이 7 이다 — 높이는 밑변에서 1px(보더 폭) 뺀 값으로
/// 잡아 카드 변에 정확히 닿게 한다.
pub fn paint_arrow(
    painter: &egui::Painter,
    tip: egui::Pos2,
    top_down: bool,
    size: f32,
    border_width: f32,
    color: egui::Color32,
) {
    let half = size / 2.0;
    let len = size - border_width;
    let pts = if top_down {
        vec![
            egui::pos2(tip.x - half, tip.y - len),
            egui::pos2(tip.x + half, tip.y - len),
            tip,
        ]
    } else {
        vec![
            egui::pos2(tip.x - len, tip.y - half),
            egui::pos2(tip.x - len, tip.y + half),
            tip,
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

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
    let path = elbow(s, t, true, theme.dag_edge_corner_radius().value());
    paint_path(ui.painter(), &path, color, stroke.width, stroke.dash);
    paint_arrow(
        ui.painter(),
        t,
        true,
        theme.dag_edge_arrow_size().value(),
        theme.border_width.value(),
        color,
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
         drops to 0.4 along with the node. The failure stays loud; everything it stranded recedes.",
    );
}
