//! 갤러리 표본을 egui로 그려 그려진 도형(글자 색·위치·줄바꿈 폭, 채움 사각형, 글리프 tint)을 모으는
//! 시험 도우미. 표본 시험이 같은 색 해석을 쓰게 한 곳에 둔다.

/// 그려진 글자 하나 — 문자열, 위치, 실제로 칠해진 색.
pub(crate) struct PaintedText {
    pub(crate) text: String,
    pub(crate) rect: egui::Rect,
    pub(crate) color: egui::Color32,
    /// 줄바꿈 폭 — 배치가 준 최대 폭이다.
    pub(crate) wrap_width: f32,
}

/// 한 프레임에 그려진 것.
pub(crate) struct Painted {
    pub(crate) shapes: Vec<egui::Shape>,
    pub(crate) texts: Vec<PaintedText>,
    /// 텍스처로 칠한 글리프(위치, tint).
    pub(crate) glyphs: Vec<(egui::Rect, egui::Color32)>,
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// `draw`를 두 번 그려(글꼴·글리프 텍스처가 자리 잡게) 둘째 프레임의 도형을 모은다.
pub(crate) fn paint(mut draw: impl FnMut(&mut egui::Ui)) -> Painted {
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 글리프가 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui));
        });
        shapes.clear();
        for clipped in out.shapes {
            flatten(&clipped.shape, &mut shapes);
        }
    }
    let texts = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) => {
                let section = t.galley.job.sections.first()?.format.color;
                let color =
                    t.override_text_color
                        .unwrap_or(if section == egui::Color32::PLACEHOLDER {
                            t.fallback_color
                        } else {
                            section
                        });
                Some(PaintedText {
                    text: t.galley.text().to_string(),
                    rect: t.visual_bounding_rect(),
                    color,
                    wrap_width: t.galley.job.wrap.max_width,
                })
            }
            _ => None,
        })
        .collect();
    let glyphs = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_some() => Some((r.rect, r.fill)),
            _ => None,
        })
        .collect();
    Painted {
        shapes,
        texts,
        glyphs,
    }
}

impl Painted {
    pub(crate) fn text(&self, needle: &str) -> &PaintedText {
        self.texts
            .iter()
            .find(|t| t.text.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} was not painted"))
    }

    /// `inner`를 품는 가장 작은 채움 사각형의 채움색.
    pub(crate) fn fill_under(&self, inner: egui::Rect) -> egui::Color32 {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.brush.is_none()
                        && r.fill != egui::Color32::TRANSPARENT
                        && r.rect.contains_rect(inner) =>
                {
                    Some((r.rect.area(), r.fill))
                }
                _ => None,
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, fill)| fill)
            .unwrap_or_else(|| panic!("nothing filled under {inner:?}"))
    }

    pub(crate) fn texts_with(&self, needle: &str) -> Vec<&PaintedText> {
        let found: Vec<&PaintedText> = self
            .texts
            .iter()
            .filter(|t| t.text.contains(needle))
            .collect();
        assert!(!found.is_empty(), "{needle:?} was not painted");
        found
    }

    /// `inner`를 품는, `fill`로 채운 가장 작은 사각형.
    pub(crate) fn rect_filled_around(&self, fill: egui::Color32, inner: egui::Rect) -> egui::Rect {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.brush.is_none() && r.fill == fill && r.rect.contains_rect(inner) =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap_or_else(|| panic!("no rect filled with {fill:?} around {inner:?}"))
    }

    /// `area` 안에 칠한 글리프의 tint.
    pub(crate) fn glyphs_in(&self, area: egui::Rect) -> Vec<egui::Color32> {
        self.glyphs
            .iter()
            .filter(|(r, _)| area.contains_rect(*r))
            .map(|(_, c)| *c)
            .collect()
    }

    pub(crate) fn rect_filled_with(&self, fill: egui::Color32) -> egui::Rect {
        self.shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.brush.is_none() && r.fill == fill => Some(r.rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no rect filled with {fill:?}"))
    }
    /// 용도 글자에 `use_needle`이 든 토큰 칩(`bed`로 채운 사각형)의 사각형과 스와치 채움색.
    pub(crate) fn token_chip(
        &self,
        bed: egui::Color32,
        use_needle: &str,
    ) -> (egui::Rect, egui::Color32) {
        let use_ = self.text(use_needle);
        let chip = self.rect_filled_around(bed, use_.rect);
        let swatch = self
            .shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.brush.is_none()
                        && r.rect != chip
                        && chip.contains_rect(r.rect)
                        && r.fill != egui::Color32::TRANSPARENT =>
                {
                    Some(r.fill)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no swatch in the {use_needle:?} chip"));
        (chip, swatch)
    }

    /// `area` 안에 `text`와 똑같은 글자가 그려졌는가.
    pub(crate) fn has_text_in(&self, text: &str, area: egui::Rect) -> bool {
        self.texts
            .iter()
            .any(|t| t.text == text && area.contains_rect(t.rect))
    }
}

/// 도우미의 색 비교를 고정한다. 표본 시험은 제품 잉크가 바뀌면 실패하지만, 도우미가 색을 보지 않게
/// 느슨해지면 그대로 통과할 수 있어 도우미만 따로 잰다.
mod helper_tests {
    use super::{Painted, paint};

    /// 표본에 칠할 네 색(패널, 칩 바탕, 스와치, 글자 뒤 강조). 색 값은 상관없고 서로 달라야 한다.
    struct Inks {
        panel: egui::Color32,
        bed: egui::Color32,
        swatch: egui::Color32,
        highlight: egui::Color32,
    }

    fn inks() -> Inks {
        let th = tasty_themes::mocha_fallback();
        let inks = Inks {
            panel: th.bg_panel().to_egui(),
            bed: th.surface_raised().to_egui(),
            swatch: th.accent_warning_raised().to_egui(),
            highlight: th.accent_primary().to_egui(),
        };
        let all = [inks.panel, inks.bed, inks.swatch, inks.highlight];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "the sample inks must differ");
            }
        }
        inks
    }
    const USE: &str = "— chip use";

    fn at(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    /// 패널 안 칩 하나: 칩 바탕 → 투명 사각형 → 스와치 → 글자 뒤 강조 → 용도 글자 순서로 칠한다.
    /// 강조 사각형은 칩보다 작게 글자를 품고, 투명 사각형은 스와치보다 먼저 칩 안에 있다.
    fn chip_sample(k: &Inks) -> (Painted, egui::Rect) {
        let chip = at(40.0, 40.0, 400.0, 60.0);
        let p = paint(|ui| {
            let painter = ui.painter();
            painter.rect_filled(at(20.0, 20.0, 600.0, 200.0), 0.0, k.panel);
            painter.rect_filled(chip, 0.0, k.bed);
            painter.rect_filled(at(48.0, 60.0, 12.0, 12.0), 0.0, egui::Color32::TRANSPARENT);
            painter.rect_filled(at(64.0, 60.0, 12.0, 12.0), 0.0, k.swatch);
            painter.rect_filled(at(90.0, 45.0, 340.0, 50.0), 0.0, k.highlight);
            painter.text(
                egui::pos2(100.0, 55.0),
                egui::Align2::LEFT_TOP,
                USE,
                egui::TextStyle::Body.resolve(ui.style()),
                egui::Color32::WHITE,
            );
        });
        (p, chip)
    }

    /// 같은 글자를 품은 사각형이 여럿이어도 고른 색으로 채운 것만 돌려준다.
    #[test]
    fn rect_filled_around_matches_the_fill_colour() {
        let k = inks();
        let (p, chip) = chip_sample(&k);
        let text = p.text(USE).rect;
        assert_eq!(p.rect_filled_around(k.bed, text), chip);
        assert_eq!(
            p.rect_filled_around(k.panel, text),
            at(20.0, 20.0, 600.0, 200.0)
        );
        assert_eq!(
            p.rect_filled_around(k.highlight, text),
            at(90.0, 45.0, 340.0, 50.0)
        );
    }

    /// 칩은 바탕 색으로 찾고, 스와치는 칩 안의 투명하지 않은 첫 사각형이다.
    #[test]
    fn token_chip_finds_the_bed_and_the_opaque_swatch() {
        let k = inks();
        let (p, chip) = chip_sample(&k);
        assert_eq!(p.token_chip(k.bed, USE), (chip, k.swatch));
    }
}
