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
