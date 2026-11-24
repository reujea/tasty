//! 라벨 옆 물음표와 호버 도움말. 클릭 동작 없이 Tooltip을 조합한다.
//! 아이콘은 painter로 직접 그리며 tasty-icons의 HELP_CIRCLE SVG와는 별도 구현이다.

use tasty_type_appearance::theme::Theme;

use crate::tooltip::{Tooltip, TooltipPlacement, tooltip_hover_delay_elapsed};

/// HelpHint 빌더.
pub struct HelpHint<'a> {
    text: &'a str,
    placement: TooltipPlacement,
    /// 강제 표시(specimen 의 open prop) — hover 없이 버블을 띄운다.
    open: bool,
    /// 버블 `Area` id 출처 — 한 페이지에 여러 개(placement 4종)를 동시에 그릴 때 고유화.
    id: Option<egui::Id>,
}

impl<'a> HelpHint<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            placement: TooltipPlacement::default(),
            open: false,
            id: None,
        }
    }

    /// 버블 배치(앵커 기준).
    pub fn placement(mut self, placement: TooltipPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// 강제 표시(specimen). hover 여부와 무관하게 버블을 그린다.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// 버블 `Area` id 출처를 명시(specimen 에서 여러 버블 동시 표시 시 충돌 방지).
    pub fn id_source(mut self, source: impl std::hash::Hash) -> Self {
        self.id = Some(egui::Id::new(source));
        self
    }

    /// 글리프를 그리고 hover(또는 강제 open) 시 Tooltip 을 조합해 표시한다.
    pub fn show(self, ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
        let size = theme.icon_glyph_size_sm.value();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let hovered = resp.hovered();

        if ui.is_rect_visible(rect) {
            let color = if hovered || self.open {
                theme.text_secondary()
            } else {
                theme.text_muted()
            }
            .to_egui();
            paint_help_glyph(ui.painter(), rect, color);
        }

        let resp = resp.on_hover_cursor(egui::CursorIcon::Help);

        // 강제 open이라도 글리프가 잘려 안 보이면 띄우지 않는다. 버블 Area는 화면 안으로 당겨져
        // 스크롤로 사라진 앵커의 버블들이 창 가장자리에 모여 겹친다.
        let forced = self.open && ui.is_rect_visible(rect);
        let show = forced || tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, hovered);
        if show {
            let tip_id = self.id.unwrap_or_else(|| resp.id.with("tooltip"));
            Tooltip::new(self.text)
                .placement(self.placement)
                .id_source(tip_id)
                .show(ui, theme, rect);
        }
        resp
    }
}

/// design 글리프 path 를 painter 로 그린다 — 바깥 원 + 물음표 훅/꼬리(arc+cubic) + 점.
/// `rect` 는 정사각 앵커(변 = size). viewBox 24 를 `rect` 로 스케일한다.
fn paint_help_glyph(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let size = rect.width();
    let scale = size / 24.0;
    let map = |x: f32, y: f32| rect.min + egui::vec2(x * scale, y * scale);
    let stroke_w = 2.0 * scale;
    let stroke = egui::Stroke::new(stroke_w, color);

    // 바깥 원 — `circle cx=12 cy=12 r=10`.
    painter.circle_stroke(map(12.0, 12.0), 10.0 * scale, stroke);

    // 물음표 훅(arc `M9.09 9 a3 3 0 0 1 5.83 1`) → 꼬리(cubic `c0 2 -3 3 -3 3`).
    // SVG 호의 끝점 표현을 중심 좌표로 변환한 값이다.
    let mut pts: Vec<egui::Pos2> = Vec::new();
    let (cx, cy, r) = (11.92_f32, 9.996_f32, 3.0_f32);
    let start = 199.4_f32.to_radians();
    let sweep = 160.7_f32.to_radians();
    const ARC_SEG: usize = 14;
    for i in 0..=ARC_SEG {
        let a = start + sweep * (i as f32 / ARC_SEG as f32);
        pts.push(map(cx + r * a.cos(), cy + r * a.sin()));
    }
    // cubic bezier: 훅 끝 (14.92,10) → 꼬리 (11.92,13).
    let (p0, c1, c2, p3) = (
        (14.92_f32, 10.0_f32),
        (14.92_f32, 12.0_f32),
        (11.92_f32, 13.0_f32),
        (11.92_f32, 13.0_f32),
    );
    const CUB_SEG: usize = 10;
    for i in 1..=CUB_SEG {
        let t = i as f32 / CUB_SEG as f32;
        let u = 1.0 - t;
        let x =
            u * u * u * p0.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p3.0;
        let y =
            u * u * u * p0.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p3.1;
        pts.push(map(x, y));
    }
    // round cap 근사(spinner 전례) — 양 끝에 반경 stroke/2 원.
    let cap = stroke_w * 0.5;
    if let (Some(&f), Some(&l)) = (pts.first(), pts.last()) {
        painter.circle_filled(f, cap, color);
        painter.circle_filled(l, cap, color);
    }
    painter.add(egui::Shape::line(pts, stroke));

    // 점 — `M12 17 h.01` (round-cap 0-length line = 지름 stroke 의 점).
    painter.circle_filled(map(12.0, 17.0), stroke_w * 0.5, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 강제 open 버블이 그려졌는지(Tooltip 순위 레이어가 생겼는지) 한 프레임으로 확인한다.
    fn forced_bubble_shown(clip: Option<egui::Rect>) -> bool {
        let theme = theme();
        let ctx = egui::Context::default();
        let id = egui::Id::new("forced-hint");
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(clip) = clip {
                    ui.set_clip_rect(clip);
                }
                HelpHint::new("tip")
                    .open(true)
                    .id_source(id)
                    .show(ui, &theme);
            });
        });
        ctx.memory(|m| {
            m.layer_ids()
                .any(|layer| layer.order == egui::Order::Tooltip)
        })
    }

    #[test]
    fn a_forced_bubble_shows_while_its_glyph_is_visible() {
        assert!(forced_bubble_shown(None));
    }

    /// 스크롤로 글리프가 클립 밖에 있으면 화면 가장자리로 당겨진 버블을 그리지 않는다.
    #[test]
    fn a_forced_bubble_hides_while_its_glyph_is_clipped() {
        let away = egui::Rect::from_min_size(egui::pos2(0.0, 5000.0), egui::vec2(10.0, 10.0));
        assert!(!forced_bubble_shown(Some(away)));
    }
}
