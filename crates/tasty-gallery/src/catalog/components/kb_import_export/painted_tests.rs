//! 내보내기 실패 표본의 잉크와 버튼 바탕을 그려진 도형으로 확인한다(본체 notices_tests와 같은 방식).

use tasty_type_appearance::theme::Theme;

use super::remaining_values::unknown_export_failure;

/// 그려진 글자 하나 — 문자열, 위치, 실제로 칠해진 색.
struct PaintedText {
    text: String,
    rect: egui::Rect,
    color: egui::Color32,
}

/// 한 프레임에 그려진 것.
struct Painted {
    shapes: Vec<egui::Shape>,
    texts: Vec<PaintedText>,
    /// 텍스처로 칠한 글리프(위치, tint).
    glyphs: Vec<(egui::Rect, egui::Color32)>,
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// `draw`를 두 번 그려(글꼴·글리프 텍스처가 자리 잡게) 둘째 프레임의 도형을 모은다.
fn paint(mut draw: impl FnMut(&mut egui::Ui)) -> Painted {
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 글리프가 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 480.0),
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
    fn text(&self, needle: &str) -> &PaintedText {
        self.texts
            .iter()
            .find(|t| t.text.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} was not painted"))
    }

    /// `inner`를 품는 가장 작은 채움 사각형의 채움색.
    fn fill_under(&self, inner: egui::Rect) -> egui::Color32 {
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

    fn rect_filled_with(&self, fill: egui::Color32) -> egui::Rect {
        self.shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.brush.is_none() && r.fill == fill => Some(r.rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no rect filled with {fill:?}"))
    }
}

/// 미분류 내보내기 실패 표본: tint 안 글리프는 accent-danger-raised, 제목 글자는 strong, 본문과 OS 줄은
/// text-on-active, Try again 채움은 surface-on-raised, ghost 라벨은 raised 바탕 잉크다.
/// Mocha는 panel·raised ghost 잉크가 같은 값이라 바탕은 Latte가 가른다.
#[test]
fn unknown_export_failure_sample_paints_the_app_inks_on_the_raised_card() {
    let latte = crate::host_shell::latte_theme();
    assert_ne!(
        latte.button_ghost_raised_fg(),
        latte.button_ghost_fg(),
        "Latte must separate the raised ghost ink from the panel one"
    );
    let themes: [Theme; 2] = [tasty_themes::mocha_fallback(), latte];
    for th in themes {
        assert_ne!(th.text_on_active(), th.text_secondary());
        assert_ne!(th.accent_danger_raised(), th.accent_danger_strong());
        let p = paint(|ui| unknown_export_failure(ui, &th));

        let title = p.text(crate::i18n::t(
            "settings.keybindings.ie_export_failure_title",
        ));
        assert_eq!(title.color, th.accent_danger_strong().to_egui(), "title");
        let tint = p.rect_filled_with(
            th.accent_danger()
                .to_egui()
                .gamma_multiply(th.tint_fill_alpha()),
        );
        let glyphs: Vec<egui::Color32> = p
            .glyphs
            .iter()
            .filter(|(r, _)| tint.contains_rect(*r))
            .map(|(_, c)| *c)
            .collect();
        assert_eq!(
            glyphs,
            vec![th.accent_danger_raised().to_egui()],
            "title glyph"
        );
        assert_eq!(
            p.text(" Nothing was written.").color,
            th.text_on_active().to_egui(),
            "body"
        );
        assert_eq!(
            p.text("os error 28").color,
            th.text_on_active().to_egui(),
            "OS line"
        );

        let retry = p.text(crate::i18n::t("settings.keybindings.ie_export_retry"));
        assert!(
            retry.rect.top() >= tint.bottom(),
            "Try again below the tint"
        );
        assert_eq!(
            p.fill_under(retry.rect),
            th.surface_on_raised().to_egui(),
            "Try again fill"
        );
        let other = p.text(crate::i18n::t(
            "settings.keybindings.ie_export_choose_another",
        ));
        assert!(other.rect.top() >= tint.bottom(), "ghost below the tint");
        assert_eq!(
            other.color,
            th.button_ghost_raised_fg().to_egui(),
            "ghost label"
        );
    }
}
