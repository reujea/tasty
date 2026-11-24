//! Surface 포커스 표현. 바탕이 Tasty 것인 kind는 바탕을 바꾸고, html과 플러그인처럼 바탕을
//! 호스트가 보장할 수 없는 kind는 leaf 위쪽의 띠로 표시한다.
//! 값은 focus 상태만 받는다. hard 점유 판정은 호출자가 먼저 끝내고 넘긴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::{LogicalPx, PhysicalPx};

/// 바탕 kind의 바탕 색. 포커스를 모르면 호출자는 `false`를 넘긴다.
#[inline]
pub fn surface_bed(theme: &Theme, focused: bool) -> egui::Color32 {
    if focused {
        theme.surface_bed_focused().to_egui()
    } else {
        theme.surface_bed_unfocused().to_egui()
    }
}

/// 띠 행의 경계(물리 px). 띠 아래 경계는 정수 물리 px라 그 아래에 놓는 내용(페이지·프레임)의 위 끝이
/// 플랫폼별 반올림과 상관없이 같은 줄에 온다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceFocusEdgeRows {
    /// 띠 행의 아래 경계이자 내용의 위 끝. leaf 아래 끝을 넘지 않는다.
    pub bottom: f32,
    /// 링이 있을 때 켜진 띠가 쓰는 아래 행의 위 경계.
    pub lit_top: f32,
}

/// `top`·`leaf_bottom`은 leaf의 위·아래 끝(물리 px), `ppp`는 논리 px당 물리 px다.
/// host의 내용 예약과 [`SurfaceFocusEdge::paint`]가 같은 경계를 쓰도록 이 함수 하나로 정한다.
pub fn surface_focus_edge_rows(
    theme: &Theme,
    top: f32,
    leaf_bottom: f32,
    ppp: f32,
) -> SurfaceFocusEdgeRows {
    let width = theme.surface_focus_edge_width().to_physical(ppp).value();
    let bottom = (top + width).round().clamp(top, leaf_bottom.max(top));
    let lower = theme.border_width.to_physical(ppp).value().round().max(1.0);
    SurfaceFocusEdgeRows {
        bottom,
        lit_top: (bottom - lower).max(top),
    }
}

/// 논리 좌표 leaf에서 [`surface_focus_edge_rows`]를 구한다. 경계는 물리 px로 정하고 논리 px로 돌려준다.
fn logical_rows(theme: &Theme, leaf: egui::Rect, ppp: f32) -> (f32, f32) {
    let physical = |y: f32| LogicalPx(y).to_physical(ppp).value();
    let logical = |y: f32| PhysicalPx(y).to_logical(ppp).value();
    let rows = surface_focus_edge_rows(theme, physical(leaf.top()), physical(leaf.bottom()), ppp);
    (logical(rows.bottom), logical(rows.lit_top))
}

/// 띠 kind leaf에서 내용(주소 띠·페이지)이 시작하는 논리 y. 띠 행 바로 아래다.
pub fn surface_focus_content_top(theme: &Theme, leaf: egui::Rect, ppp: f32) -> f32 {
    logical_rows(theme, leaf, ppp.max(f32::EPSILON)).0
}

/// 띠 kind의 포커스 띠. 상태와 무관하게 같은 행을 차지하므로 포커스가 바뀌어도 내용이 움직이지 않는다.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceFocusEdge {
    pub focused: bool,
    /// leaf를 두르는 링(입력 대기, 점유, 완료). 링 우선순위는 호출자가 정한다.
    pub ring: Option<egui::Stroke>,
    /// leaf의 모서리 반경. 본체 leaf는 0이다.
    pub corner_radius: LogicalPx,
}

/// 칠하는 한 겹. 순서가 곧 쌓임 순서다.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Layer {
    Row {
        rect: egui::Rect,
        color: egui::Color32,
        round_top: bool,
    },
    Ring,
}

impl SurfaceFocusEdge {
    /// 켜진 띠는 링 위에 놓이고, 링이 있으면 아래 행만 써서 링의 위 행을 남긴다.
    /// 행 경계는 [`surface_focus_edge_rows`]로 `ppp`의 물리 px에 맞춘다.
    fn layers(&self, theme: &Theme, leaf: egui::Rect, ppp: f32) -> Vec<Layer> {
        let ppp = ppp.max(f32::EPSILON);
        let (bottom, lit_top) = logical_rows(theme, leaf, ppp);
        let full = egui::Rect::from_min_max(leaf.min, egui::pos2(leaf.right(), bottom));
        let mut layers = vec![Layer::Row {
            rect: full,
            color: theme.surface_focus_edge_unfocused().to_egui(),
            round_top: true,
        }];
        if self.ring.is_some() {
            layers.push(Layer::Ring);
        }
        if self.focused {
            let lit = theme.surface_focus_edge_focused().to_egui();
            layers.push(if self.ring.is_some() {
                let lower = egui::Rect::from_min_max(
                    egui::pos2(leaf.left(), lit_top),
                    egui::pos2(leaf.right(), bottom),
                );
                Layer::Row {
                    rect: lower,
                    color: lit,
                    round_top: false,
                }
            } else {
                Layer::Row {
                    rect: full,
                    color: lit,
                    round_top: true,
                }
            });
        }
        layers
    }

    /// `leaf` 전체를 기준으로 띠와 링을 칠한다. 내용은 띠 행 아래에 놓는 것이 호출자의 일이다.
    pub fn paint(&self, painter: &egui::Painter, theme: &Theme, leaf: egui::Rect) {
        let radius = self.corner_radius.value().round().clamp(0.0, 255.0) as u8;
        for layer in self.layers(theme, leaf, painter.ctx().pixels_per_point()) {
            match layer {
                Layer::Row {
                    rect,
                    color,
                    round_top,
                } => {
                    let corners = if round_top {
                        egui::CornerRadius {
                            nw: radius,
                            ne: radius,
                            sw: 0,
                            se: 0,
                        }
                    } else {
                        egui::CornerRadius::ZERO
                    };
                    painter.rect_filled(rect, corners, color);
                }
                Layer::Ring => {
                    if let Some(ring) = self.ring {
                        painter.rect_stroke(leaf, radius, ring, egui::StrokeKind::Inside);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors(tasty_themes::mocha_fallback_colors(), false)
    }

    fn leaf() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 50.0))
    }

    fn edge(focused: bool, ring: bool) -> SurfaceFocusEdge {
        SurfaceFocusEdge {
            focused,
            ring: ring.then(|| egui::Stroke::new(theme().border_width.value(), egui::Color32::RED)),
            corner_radius: LogicalPx(0.0),
        }
    }

    fn row(layer: Layer) -> (egui::Rect, egui::Color32) {
        match layer {
            Layer::Row { rect, color, .. } => (rect, color),
            Layer::Ring => panic!("expected a row"),
        }
    }

    #[test]
    fn the_unfocused_edge_is_one_full_row_pair() {
        let th = theme();
        let layers = edge(false, false).layers(&th, leaf(), 1.0);
        assert_eq!(layers.len(), 1);
        let (rect, color) = row(layers[0]);
        assert_eq!(rect.height(), th.surface_focus_edge_width().value());
        assert_eq!(rect.width(), leaf().width());
        assert_eq!(rect.min, leaf().min);
        assert_eq!(color, th.surface_focus_edge_unfocused().to_egui());
    }

    #[test]
    fn a_ring_paints_over_the_unfocused_edge() {
        let layers = edge(false, true).layers(&theme(), leaf(), 1.0);
        assert_eq!(layers.len(), 2);
        assert!(matches!(layers[0], Layer::Row { .. }));
        assert_eq!(layers[1], Layer::Ring);
    }

    #[test]
    fn the_lit_edge_takes_both_rows_without_a_ring() {
        let th = theme();
        let layers = edge(true, false).layers(&th, leaf(), 1.0);
        let (rect, color) = row(layers[layers.len() - 1]);
        assert_eq!(rect.height(), th.surface_focus_edge_width().value());
        assert_eq!(rect.min, leaf().min);
        assert_eq!(color, th.surface_focus_edge_focused().to_egui());
    }

    #[test]
    fn the_lit_edge_sits_over_the_ring_on_the_lower_row_only() {
        let th = theme();
        let layers = edge(true, true).layers(&th, leaf(), 1.0);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[1], Layer::Ring);
        let (rect, color) = row(layers[2]);
        let row_h = th.border_width.value();
        assert_eq!(rect.height(), row_h);
        assert_eq!(rect.top(), leaf().top() + row_h);
        assert_eq!(color, th.surface_focus_edge_focused().to_egui());
    }

    /// 배율이 소수여도 띠 아래 경계와 켜진 행 경계는 정수 물리 px이고, leaf 아래를 넘지 않는다.
    #[test]
    fn the_rows_land_on_whole_physical_pixels() {
        let th = theme();
        let rows = surface_focus_edge_rows(&th, 100.75, 400.0, 1.25);
        assert_eq!(rows.bottom, 103.0);
        assert_eq!(rows.lit_top, 102.0);
        let rows = surface_focus_edge_rows(&th, 0.0, 400.0, 2.0);
        assert_eq!((rows.bottom, rows.lit_top), (4.0, 2.0));
        let short = surface_focus_edge_rows(&th, 10.0, 11.0, 1.0);
        assert_eq!(short.bottom, 11.0);
        assert_eq!(short.lit_top, 10.0);
        let layers = edge(true, true).layers(&th, leaf(), 1.25);
        let (rect, _) = row(layers[2]);
        assert_eq!(rect.bottom() * 1.25, (leaf().top() * 1.25 + 2.5).round());
        assert_eq!(surface_focus_content_top(&th, leaf(), 1.25), rect.bottom());
    }

    #[test]
    fn the_bed_reads_the_pair() {
        let th = theme();
        assert_eq!(surface_bed(&th, true), th.surface_bed_focused().to_egui());
        assert_eq!(
            surface_bed(&th, false),
            th.surface_bed_unfocused().to_egui()
        );
    }
}
