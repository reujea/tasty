use super::*;

fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// `draw`를 두 번 그린 뒤(글꼴·글리프 텍스처가 자리 잡게) 경고 배지 채움 안에 칠해진 글리프의 tint를 모은다.
fn badge_glyph_inks(th: &Theme, mut draw: impl FnMut(&mut egui::Ui)) -> Vec<egui::Color32> {
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 글리프가 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
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
    let badge_fill = th
        .accent_warning()
        .to_egui()
        .gamma_multiply(th.tint_fill_alpha());
    let badges: Vec<egui::Rect> = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_none() && r.fill == badge_fill => Some(r.rect),
            _ => None,
        })
        .collect();
    assert_eq!(badges.len(), 1, "one warning badge is painted");
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_some() && badges[0].contains_rect(r.rect) => {
                Some(r.fill)
            }
            _ => None,
        })
        .collect()
}

/// 경고 배지 글리프는 base 행에서 accent-warning-glyph, raised 필터 메뉴에서 accent-warning-raised다.
/// Mocha는 두 값이 같아 Latte에서 갈리는지 먼저 본다.
#[test]
fn warn_badge_glyph_ink_follows_its_host() {
    let latte = latte();
    assert_ne!(
        latte.accent_warning_glyph(),
        latte.accent_warning_raised(),
        "Latte must separate the two glyph inks"
    );
    for th in [tasty_themes::mocha_fallback(), latte] {
        assert_eq!(
            warn_badge_glyph_ink(&th, false),
            th.accent_warning_glyph(),
            "base row glyph"
        );
        assert_eq!(
            warn_badge_glyph_ink(&th, true),
            th.accent_warning_raised(),
            "raised menu glyph"
        );
    }
}

/// 실제로 그린 목록 행 칩과 프로토콜 필터 메뉴의 배지 글리프 색. 호출부가 넘기는 바탕과 그리기가 그 잉크를
/// 쓰는지 함께 본다.
#[test]
fn painted_warn_badge_glyphs_read_their_host_ink() {
    for th in [tasty_themes::mocha_fallback(), latte()] {
        let row = badge_glyph_inks(&th, |ui| {
            remote_row_title(
                ui,
                &th,
                "legacy-box",
                None,
                false,
                &[RemoteRowChip::Warn {
                    text: "passkey missing",
                    tooltip: "hint",
                }],
                400.0,
            );
        });
        assert_eq!(row, vec![th.accent_warning_glyph().to_egui()], "base row");

        let labels = ProtocolFilterLabels {
            title: "Filter by protocol",
            select_all: "Select all",
            deselect_all: "Deselect all",
            reset: "Reset",
            apply: "Apply",
            unknown: "unknown",
            unknown_hint: "hint",
        };
        let items = [
            ProtocolFilterItem {
                name: "ssh",
                unknown: false,
            },
            ProtocolFilterItem {
                name: "smb",
                unknown: true,
            },
        ];
        let mut draft = HashSet::new();
        let menu = badge_glyph_inks(&th, |ui| {
            draw_protocol_filter_body(ui, &th, &items, &labels, &mut draft);
        });
        assert_eq!(
            menu,
            vec![th.accent_warning_raised().to_egui()],
            "raised filter menu"
        );
    }
}
