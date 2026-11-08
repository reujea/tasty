//! Surface 포커스 표현. 바탕이 Tasty 것인 kind는 바탕을 바꾸고, html과 플러그인처럼 바탕을
//! 호스트가 보장할 수 없는 kind는 leaf 위쪽의 띠로 표시한다.
//! 값은 focus 상태만 받는다. hard 점유 판정은 호출자가 먼저 끝내고 넘긴다.

use tasty_type_appearance::theme::Theme;

/// 바탕 kind의 바탕 색. 포커스를 모르면 호출자는 `false`를 넘긴다.
#[inline]
pub fn surface_bed(theme: &Theme, focused: bool) -> egui::Color32 {
    if focused {
        theme.surface_bed_focused().to_egui()
    } else {
        theme.surface_bed_unfocused().to_egui()
    }
}

/// 띠 kind의 포커스 띠. 상태와 무관하게 같은 행을 차지하므로 포커스가 바뀌어도 내용이 움직이지 않는다.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceFocusEdge {
    pub focused: bool,
    /// leaf를 두르는 링(입력 대기, 점유, 완료). 링 우선순위는 호출자가 정한다.
    pub ring: Option<egui::Stroke>,
    /// leaf의 모서리 반경. 본체 leaf는 0이다.
    pub corner_radius: f32,
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
    fn layers(&self, theme: &Theme, leaf: egui::Rect) -> Vec<Layer> {
        let height = theme.surface_focus_edge_width().value();
        let row_h = theme.border_width.value();
        let full = egui::Rect::from_min_size(leaf.min, egui::vec2(leaf.width(), height));
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
                let lower = egui::Rect::from_min_size(
                    egui::pos2(leaf.left(), leaf.top() + row_h),
                    egui::vec2(leaf.width(), row_h),
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
        let radius = self.corner_radius.round().clamp(0.0, 255.0) as u8;
        for layer in self.layers(theme, leaf) {
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
            corner_radius: 0.0,
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
        let layers = edge(false, false).layers(&th, leaf());
        assert_eq!(layers.len(), 1);
        let (rect, color) = row(layers[0]);
        assert_eq!(rect.height(), th.surface_focus_edge_width().value());
        assert_eq!(rect.width(), leaf().width());
        assert_eq!(rect.min, leaf().min);
        assert_eq!(color, th.surface_focus_edge_unfocused().to_egui());
    }

    #[test]
    fn a_ring_paints_over_the_unfocused_edge() {
        let layers = edge(false, true).layers(&theme(), leaf());
        assert_eq!(layers.len(), 2);
        assert!(matches!(layers[0], Layer::Row { .. }));
        assert_eq!(layers[1], Layer::Ring);
    }

    #[test]
    fn the_lit_edge_takes_both_rows_without_a_ring() {
        let th = theme();
        let layers = edge(true, false).layers(&th, leaf());
        let (rect, color) = row(layers[layers.len() - 1]);
        assert_eq!(rect.height(), th.surface_focus_edge_width().value());
        assert_eq!(rect.min, leaf().min);
        assert_eq!(color, th.surface_focus_edge_focused().to_egui());
    }

    #[test]
    fn the_lit_edge_sits_over_the_ring_on_the_lower_row_only() {
        let th = theme();
        let layers = edge(true, true).layers(&th, leaf());
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[1], Layer::Ring);
        let (rect, color) = row(layers[2]);
        let row_h = th.border_width.value();
        assert_eq!(rect.height(), row_h);
        assert_eq!(rect.top(), leaf().top() + row_h);
        assert_eq!(color, th.surface_focus_edge_focused().to_egui());
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
