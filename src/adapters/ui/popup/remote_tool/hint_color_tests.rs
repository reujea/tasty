//! 원격 모달 입력 행의 플레이스홀더가 본문색이 아니라 placeholder 토큰으로 그려지는지 본다.
//! 본체 전역 스타일은 `override_text_color`가 본문색(`text_primary()`)이라 색 없는 hint는 본문색이 된다.
//! 그래서 시험도 `apply_theme_to_egui`를 먼저 적용한다. 빠지면 결함이 재현되지 않는다.

use egui::{Color32, Pos2, RawInput, Rect, pos2, vec2};

use super::text_row;
use crate::theme;

fn mocha() -> theme::Theme {
    theme::Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// `value`가 든 입력 행을 그리고 `needle` 글자를 칠한 색을 모은다.
fn glyph_colors(th: &theme::Theme, value: &str, placeholder: &str, needle: &str) -> Vec<Color32> {
    let ctx = egui::Context::default();
    tasty_egui_theme::apply_theme_to_egui(th, &ctx);
    let mut out = None;
    // 측정이 끝난 두 번째 프레임을 읽는다.
    for _ in 0..2 {
        out = Some(ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 200.0))),
                focused: true,
                ..Default::default()
            },
            |c| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(pos2(10.0, 10.0))
                    .fade_in(false)
                    .show(c, |ui| {
                        let mut v = value.to_string();
                        text_row(ui, th, "Name", &mut v, placeholder, false);
                    });
            },
        ));
    }
    let out = out.expect("두 프레임을 돌렸다");
    let mut colors = Vec::new();
    for s in &out.shapes {
        let egui::Shape::Text(t) = &s.shape else {
            continue;
        };
        if t.galley.job.text != needle {
            continue;
        }
        for sec in &t.galley.job.sections {
            let c = if sec.format.color == Color32::PLACEHOLDER {
                t.fallback_color
            } else {
                sec.format.color
            };
            colors.push(t.override_text_color.unwrap_or(c));
        }
    }
    colors
}

#[test]
fn an_empty_text_row_paints_its_hint_with_the_placeholder_token() {
    let th = mocha();
    let placeholder: Color32 = th.input_placeholder().to_egui();
    let body: Color32 = th.text_primary().to_egui();
    assert_ne!(
        placeholder, body,
        "두 색이 같으면 이 시험은 아무것도 가르지 못한다"
    );
    let colors = glyph_colors(&th, "", "gb10", "gb10");
    assert!(!colors.is_empty(), "hint 글자를 찾지 못했다");
    assert!(
        colors.iter().all(|c| *c == placeholder),
        "hint가 placeholder 토큰이 아니다 — 값처럼 읽힌다: {colors:?}"
    );
}

#[test]
fn a_filled_text_row_keeps_the_body_color_for_its_value() {
    let th = mocha();
    let body: Color32 = th.text_primary().to_egui();
    let colors = glyph_colors(&th, "gx10", "gb10", "gx10");
    assert!(!colors.is_empty(), "값 글자를 찾지 못했다");
    assert!(
        colors.iter().all(|c| *c == body),
        "입력값이 본문색이 아니다: {colors:?}"
    );
}
