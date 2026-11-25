//! Attach 탭 표본 Meta의 inactive name 칩을 그려진 도형으로 확인한다.

use tasty_type_appearance::theme::Theme;

use super::draw_attach;

/// 그려진 글자 하나 — 문자열, 위치, 실제로 칠해진 색.
struct PaintedText {
    text: String,
    rect: egui::Rect,
    color: egui::Color32,
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// `draw`를 두 번 그려 둘째 프레임의 도형과 글자를 모은다.
fn paint(mut draw: impl FnMut(&mut egui::Ui)) -> (Vec<egui::Shape>, Vec<PaintedText>) {
    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 1600.0),
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
    (shapes, texts)
}

/// `fill`로 채운, `inner`를 품는 가장 작은 사각형.
fn rect_filled_around(
    shapes: &[egui::Shape],
    fill: egui::Color32,
    inner: egui::Rect,
) -> egui::Rect {
    shapes
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

/// inactive name 칩은 text-muted를 이름과 스와치로 보이고, 그 스와치는 표본의 inactive 행 이름 잉크와 같다.
#[test]
fn attach_meta_names_the_inactive_name_ink_text_muted() {
    let themes: [Theme; 2] = [
        tasty_themes::mocha_fallback(),
        crate::host_shell::latte_theme(),
    ];
    for th in themes {
        assert_ne!(th.text_muted(), th.text_disabled());
        let (shapes, texts) = paint(|ui| draw_attach(ui, &th));
        let use_ = texts
            .iter()
            .find(|t| t.text.contains("— inactive name"))
            .expect("inactive name chip");
        let chip = rect_filled_around(&shapes, th.surface_raised().to_egui(), use_.rect);
        assert!(
            texts
                .iter()
                .any(|t| t.text == "text-muted" && chip.contains_rect(t.rect)),
            "the inactive name chip is named text-muted"
        );
        let swatch = shapes
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
            .expect("swatch in the inactive name chip");
        assert_eq!(swatch, th.text_muted().to_egui(), "chip swatch");
        let name = texts
            .iter()
            .find(|t| t.text == "legacy-attach")
            .expect("inactive attach row name");
        assert_eq!(name.color, swatch, "the row name reads the chip's ink");
    }
}
