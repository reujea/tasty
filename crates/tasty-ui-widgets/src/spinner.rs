//! 회전하는 호로 진행 중임을 표시한다. 기본 색은 text_muted다.
//! Theme의 모션 감소 설정이 켜지면 회전 대신 정적인 점 세 개를 그린다.

use tasty_type_appearance::theme::Theme;

/// viewBox 24 기준 stroke 두께(디자인 `stroke=2`). 실제 그릴 때 size 비율로 환산.
const VIEWBOX: f32 = 24.0;
const STROKE_VB: f32 = 2.0;
/// track 저대비 alpha (디자인 `.tasty-spinner__track { opacity: 0.22 }`).
const TRACK_ALPHA: f32 = 0.22;
/// arc 가 도는 각도(디자인 `a r r 0 0 1 r r` = 90° 호).
const ARC_SWEEP: f32 = std::f32::consts::FRAC_PI_2;

/// [`busy_caption`] 이 차지하는 폭. 줄 배치를 미리 정할 때 쓴다.
pub fn busy_caption_width(ui: &egui::Ui, theme: &Theme, word: &str, size: f32) -> f32 {
    let font = egui::FontId::proportional(theme.font_size_caption.value());
    let text = ui.fonts(|f| {
        f.layout_no_wrap(word.to_owned(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    });
    size + theme.spacing_sm.value() + text
}

/// 바쁜 버튼 옆의 진행 표시. Spinner 와 caption 크기의 `text-muted` 문구다.
/// 버튼은 라벨과 폭을 그대로 둔 채 비활성이 되고, 진행은 이 표시가 알린다.
pub fn busy_caption(ui: &mut egui::Ui, theme: &Theme, word: &str, size: f32) {
    busy_caption_in_ink(ui, theme, word, size, theme.text_muted().to_egui());
}

/// `surface-raised` 처럼 `text-muted` 가 등급에 못 미치는 바탕에서 그 바탕의 잉크를 넘긴다.
pub fn busy_caption_in_ink(
    ui: &mut egui::Ui,
    theme: &Theme,
    word: &str,
    size: f32,
    ink: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        Spinner::new().size(size).show(ui, theme);
        ui.add(
            egui::Label::new(
                egui::RichText::new(word)
                    .size(theme.font_size_caption.value())
                    .color(ink),
            )
            .truncate(),
        );
    });
}

/// Spinner 빌더.
pub struct Spinner {
    size: Option<f32>,
    still_arc: bool,
    stroke: f32,
    /// 설정을 무시하는 override. `None` 이면 `theme.reduced_motion` 을 따른다.
    reduced_motion: Option<bool>,
    /// 호출부 지정 색. `None` 이면 `theme.spinner_indicator()`.
    color: Option<egui::Color32>,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            size: None,
            still_arc: false,
            stroke: STROKE_VB,
            reduced_motion: None,
            color: None,
        }
    }

    /// 정사각 변 길이(px). 미지정이면 아이콘 가족 `icon-size-md`(16).
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// 모션 감소 설정을 덮어쓴다. 두 상태를 나란히 보여주는 갤러리 등에만 사용한다.
    /// 제품 화면에서는 사용자 접근성 설정을 따라야 한다.
    pub fn reduced_motion(mut self, reduced_motion: bool) -> Self {
        self.reduced_motion = Some(reduced_motion);
        self
    }

    /// arc/track/dot 의 기본 색을 덮어쓴다(미지정 시 `text-muted`).
    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Stroke in the design's 24-unit view box (rail loading uses 1.72).
    pub fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = stroke;
        self
    }

    /// LoadingOverlay/PendingIndicator use a still arc for reduced motion.
    pub fn still_arc(mut self) -> Self {
        self.still_arc = true;
        self
    }

    /// 그리고 hover 응답을 반환한다.
    pub fn show(self, ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
        let size = self
            .size
            .unwrap_or_else(|| theme.icon_glyph_size_md.value());
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        self.paint_in(ui, theme, rect);
        resp
    }

    /// 자리를 잡지 않고 `rect` 가운데에 그린다. 버튼의 leading 아이콘처럼 자리를 다른 위젯이
    /// 정하는 곳에서 쓴다. 변 길이는 `size` 이며, 지정하지 않으면 `rect` 의 짧은 변이다.
    pub fn paint_in(self, ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
        let size = self.size.unwrap_or_else(|| rect.width().min(rect.height()));
        let rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
        let color = self
            .color
            .unwrap_or_else(|| theme.spinner_indicator().to_egui());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let reduced = self.reduced_motion.unwrap_or(theme.reduced_motion);
        if reduced && !self.still_arc {
            draw_dots(ui, rect, color);
            return;
        }

        let painter = ui.painter();
        let center = rect.center();
        let scale = size / VIEWBOX;
        let stroke_w = self.stroke * scale;
        let radius = (VIEWBOX * 0.5 - self.stroke) * scale;

        let track_color = color.gamma_multiply(TRACK_ALPHA);
        painter.circle_stroke(center, radius, egui::Stroke::new(stroke_w, track_color));

        let t = ui.ctx().input(|i| i.time);
        // Theme의 회전 주기를 초 단위로 변환한다.
        let phase = if reduced {
            0.0
        } else {
            (t / theme.spinner_duration().to_secs_f64()).rem_euclid(1.0) as f32
        };
        let start = -std::f32::consts::FRAC_PI_2 + phase * std::f32::consts::TAU;
        draw_arc(
            ui,
            center,
            radius,
            start,
            start + ARC_SWEEP,
            stroke_w,
            color,
        );

        if !reduced {
            ui.ctx().request_repaint();
        }
    }
}

/// `start_angle`→`end_angle`(라디안, 시계방향 화면 좌표) 사이의 호를 둥근 끝
/// 선분으로 근사해 그린다.
fn draw_arc(
    ui: &mut egui::Ui,
    center: egui::Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    stroke_w: f32,
    color: egui::Color32,
) {
    const SEGMENTS: usize = 24;
    let mut points = Vec::with_capacity(SEGMENTS + 1);
    for i in 0..=SEGMENTS {
        let a = start_angle + (end_angle - start_angle) * (i as f32 / SEGMENTS as f32);
        points.push(egui::pos2(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    let stroke = egui::Stroke::new(stroke_w, color);
    // 둥근 끝(디자인 `stroke-linecap="round"`) 근사 — 양 끝에 반원 캡.
    let cap_r = stroke_w * 0.5;
    if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
        ui.painter().circle_filled(first, cap_r, color);
        ui.painter().circle_filled(last, cap_r, color);
    }
    ui.painter().add(egui::Shape::line(points, stroke));
}

/// reduced-motion fallback — 가로 3-dot (디자인 `radial-gradient` 28% 점 3개).
fn draw_dots(ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32) {
    let painter = ui.painter();
    // 디자인: 점 지름 ≈ 28% of box → 반경 14%.
    let r = rect.width() * 0.14;
    let cy = rect.center().y;
    let xs = [rect.left() + r, rect.center().x, rect.right() - r];
    for x in xs {
        painter.circle_filled(egui::pos2(x, cy), r, color);
    }
}
