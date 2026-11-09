//! 놓을 대상 표시. 칠 순서는 부모, 틴트, 행 fill, 잉크, 링이다. 틴트가 잉크 위에 오면 화면의
//! 글자가 토큰 값이 아니게 되므로 틴트는 행보다 먼저 칠한다.

use tasty_type_appearance::theme::Theme;

fn tint_shape(theme: &Theme, rect: egui::Rect, radius: f32) -> egui::Shape {
    egui::Shape::rect_filled(rect, radius, theme.explorer_drop_target_bg().to_egui())
}

/// 놓을 대상의 틴트. 행을 그리기 전에 부른다.
pub fn paint_drop_tint(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, radius: f32) {
    painter.add(tint_shape(theme, rect, radius));
}

/// 놓을 대상의 안쪽 링. 행을 그린 뒤에 부른다. 배치는 바꾸지 않는다.
pub fn paint_drop_ring(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, radius: f32) {
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.explorer_drop_target_border().to_egui(),
        ),
        egui::StrokeKind::Inside,
    );
}

/// 틴트 자리. 놓을 대상은 행을 그린 뒤에야 정해지므로 행보다 먼저 자리를 잡아 둔다.
/// 틴트를 칠하지 않으면 자리는 빈 채로 남는다.
#[derive(Clone, Copy, Debug)]
pub struct DropTintSlot {
    layer: egui::LayerId,
    shape: egui::layers::ShapeIdx,
}

impl DropTintSlot {
    pub fn reserve(painter: &egui::Painter) -> Self {
        Self {
            layer: painter.layer_id(),
            shape: painter.add(egui::Shape::Noop),
        }
    }

    /// 잡아 둔 자리에 틴트를 칠한다. 잡은 프레임 안에서만 부른다.
    pub fn fill(self, ctx: &egui::Context, theme: &Theme, rect: egui::Rect, radius: f32) {
        ctx.layer_painter(self.layer)
            .with_clip_rect(rect)
            .set(self.shape, tint_shape(theme, rect, radius));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 대상을 행 뒤에 정해도 틴트는 행 잉크 아래, 링은 위에 놓인다.
    #[test]
    fn the_tint_lands_under_the_row_and_the_ring_over_it() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 20.0));
        let ink = theme.text_primary().to_egui();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let slot = DropTintSlot::reserve(ui.painter());
                ui.painter().line_segment(
                    [rect.left_center(), rect.right_center()],
                    egui::Stroke::new(theme.border_width.value(), ink),
                );
                slot.fill(ctx, &theme, rect, 0.0);
                paint_drop_ring(ui.painter(), &theme, rect, 0.0);
            });
        });
        let at = |want: &dyn Fn(&egui::Shape) -> bool| {
            out.shapes
                .iter()
                .position(|s| want(&s.shape))
                .expect("shape painted")
        };
        let tint = theme.explorer_drop_target_bg().to_egui();
        let border = theme.explorer_drop_target_border().to_egui();
        let tint_at = at(&|s| matches!(s, egui::Shape::Rect(r) if r.fill == tint));
        let ink_at =
            at(&|s| matches!(s, egui::Shape::LineSegment { stroke, .. } if stroke.color == ink));
        let ring_at = at(&|s| matches!(s, egui::Shape::Rect(r) if r.stroke.color == border));
        assert!(tint_at < ink_at, "the tint is under the ink");
        assert!(ink_at < ring_at, "the ring is over the ink");
    }

    #[test]
    fn an_unfilled_slot_paints_nothing() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                DropTintSlot::reserve(ui.painter());
            });
        });
        let tint = theme.explorer_drop_target_bg().to_egui();
        assert!(
            !out.shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == tint))
        );
    }
}
