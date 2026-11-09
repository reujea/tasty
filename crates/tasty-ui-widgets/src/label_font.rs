//! `font-label`: 번역 카탈로그의 낱말을 그리는 작은 라벨의 글꼴. 값은 문자열이 아니라 UI 언어로 정한다.
//!
//! ja와 zh에서는 고정폭 글꼴에 없는 한자가 많아 고정폭 체인으로 그리면 한 낱말 안에서 글꼴이
//! 바뀐다. 라틴 글자의 고정폭을 지키려면 D2Coding이 체인 맨 앞에 있어야 하므로 글꼴 순서로는 풀 수
//! 없다. 경로·ID·버전·숫자 같은 값은 이 글꼴을 읽지 않고 모든 언어에서 고정폭으로 그린다.
//!
//! UI sans 라벨의 글자는 Proportional 체인의 것이고 행 높이와 baseline은 같은 크기 mono의 값이다.
//! 언어를 바꿔도 레이아웃이 움직이지 않게 하는 규칙이며(RULES 8절) 본문 UI sans는 움직이지 않는다.
//! 값은 epaint가 family를 풀 때 mono에서 읽는다(`vendor/epaint/PATCHES.md`).
//! 체인 전체에 없는 글자는 같은 mono로 그린다. 시스템 CJK 글꼴에 없는 ー·상태 글리프가 고정폭 라벨
//! 시절처럼 나오게 하는 마지막 대체이며 앞선 글꼴의 글자를 빼앗지 않는다.
//!
//! fontFamily 토큰은 생성기가 접근자를 만들지 않으므로 여기서 egui family로 옮긴다.

use std::sync::OnceLock;

/// `font-label`이 `font-ui`를 읽는 언어. 디자인 정본 `vendor/tokens/base.css`의 `:lang()` 규칙과
/// 같은 집합이며 `label_font_languages_match_the_design_source`가 대조한다.
const LABEL_UI_SANS_LANGUAGES: &[&str] = &["ja", "zh"];

/// 언어 코드의 `font-label` 값. CSS `:lang()`처럼 첫 부분을 대소문자 없이 비교한다.
pub fn label_font_family_for(language: &str) -> egui::FontFamily {
    let primary = language.split(['-', '_']).next().unwrap_or_default();
    if LABEL_UI_SANS_LANGUAGES
        .iter()
        .any(|listed| primary.eq_ignore_ascii_case(listed))
    {
        egui::FontFamily::Name(egui::epaint::text::MONO_LINE_SANS_FAMILY.into())
    } else {
        egui::FontFamily::Monospace
    }
}

/// 현재 UI 언어의 `font-label`. 호스트가 넘긴 `TASTY_LOCALE`을 읽는다. 플러그인은 `tasty_i18n::init`을
/// 부르지 않아 `current_language()`가 항상 `en`이기 때문이다. 언어는 재시작해야 바뀌므로 프로세스마다
/// 한 번만 읽는다.
pub fn label_font_family() -> egui::FontFamily {
    static FAMILY: OnceLock<egui::FontFamily> = OnceLock::new();
    FAMILY
        .get_or_init(|| label_font_family_for(&std::env::var("TASTY_LOCALE").unwrap_or_default()))
        .clone()
}

/// `size` 크기의 `font-label` 글꼴.
pub fn label_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, label_font_family())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_code_picks_the_family_as_the_css_rule_does() {
        for code in ["en", "ko", "ko-KR", "fr", ""] {
            assert_eq!(
                label_font_family_for(code),
                egui::FontFamily::Monospace,
                "{code:?}"
            );
        }
        let ui_sans = egui::FontFamily::Name(egui::epaint::text::MONO_LINE_SANS_FAMILY.into());
        for code in ["ja", "ja-JP", "zh", "zh-Hans", "zh-CN", "ZH_cn", "JA"] {
            assert_eq!(label_font_family_for(code), ui_sans, "{code:?}");
        }
    }

    const LABEL_SIZES: [f32; 4] = [10.0, 11.0, 12.0, 13.0];

    fn with_app_fonts<R>(pixels_per_point: f32, read: impl FnOnce(&egui::Context) -> R) -> R {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(pixels_per_point);
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "d2coding".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(
                tasty_font::D2CODING_REGULAR_TTF,
            )),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "d2coding".to_owned());
        ctx.set_fonts(fonts);
        let mut out = None;
        let mut read = Some(read);
        let _full_output = ctx.run(egui::RawInput::default(), |ctx| {
            if let Some(read) = read.take() {
                out = Some(read(ctx));
            }
        });
        out.expect("the frame ran once")
    }

    /// 측정 기준 1번. 이 글꼴 구성에서 sans와 mono의 행 높이가 실제로 달라야 시험이 뜻을 가진다.
    #[test]
    fn a_ui_sans_label_has_the_row_height_of_mono_at_the_same_size() {
        for pixels_per_point in [1.0, 1.5, 2.0] {
            with_app_fonts(pixels_per_point, |ctx| {
                for size in LABEL_SIZES {
                    let [label, mono, body] = [
                        label_font_family_for("ja"),
                        egui::FontFamily::Monospace,
                        egui::FontFamily::Proportional,
                    ]
                    .map(|family| ctx.fonts(|f| f.row_height(&egui::FontId::new(size, family))));
                    assert!(
                        (label - mono).abs() < 0.001,
                        "{size}px at {pixels_per_point}: label {label}, mono {mono}"
                    );
                    assert!(
                        (body - mono).abs() >= 0.001,
                        "{size}px at {pixels_per_point}: body {body} must keep its own height"
                    );
                }
            });
        }
    }

    /// 라틴과 대체 글꼴의 글자가 mono와 같은 baseline에 놓인다. 글리프 pos.y가 baseline이다.
    #[test]
    fn a_ui_sans_label_sits_on_the_mono_baseline() {
        fn baseline(ctx: &egui::Context, size: f32, family: egui::FontFamily, text: &str) -> f32 {
            let galley = ctx.fonts(|f| {
                f.layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::new(size, family),
                    egui::Color32::WHITE,
                )
            });
            let y = galley.rows[0].glyphs[0].pos.y;
            assert!(galley.rows[0].glyphs.iter().all(|g| g.pos.y == y), "{text}");
            y
        }
        with_app_fonts(1.0, |ctx| {
            for size in LABEL_SIZES {
                let mono = baseline(ctx, size, egui::FontFamily::Monospace, "H");
                // 그림 문자는 Ubuntu에 없어 번들 대체 글꼴까지 지난다. 시스템 CJK 글꼴이 없어도 잰다.
                // ー와 상태 글리프는 마지막 대체인 D2Coding까지 지난다.
                for text in [
                    "H",
                    "PROJECTS",
                    "\u{1f600}",
                    "H \u{1f600}",
                    "H\u{30fc}\u{25e6}\u{2298}\u{25d2}",
                ] {
                    let label = baseline(ctx, size, label_font_family_for("ja"), text);
                    assert_eq!(label, mono, "{size}px {text}");
                }
            }
        });
    }

    /// 라벨 글꼴이 글자 폭은 UI sans의 것을 그대로 쓴다.
    #[test]
    fn a_ui_sans_label_keeps_the_sans_advance() {
        with_app_fonts(1.0, |ctx| {
            let width = |family| {
                ctx.fonts(|f| {
                    f.layout_no_wrap(
                        "PROJECTS".to_owned(),
                        egui::FontId::new(LABEL_SIZES[0], family),
                        egui::Color32::WHITE,
                    )
                })
                .size()
                .x
            };
            assert_eq!(
                width(label_font_family_for("ja")),
                width(egui::FontFamily::Proportional)
            );
            assert_ne!(
                width(label_font_family_for("ja")),
                width(egui::FontFamily::Monospace)
            );
        });
    }

    fn advance(ctx: &egui::Context, family: egui::FontFamily, c: char) -> f32 {
        let font = egui::FontId::new(LABEL_SIZES[1], family);
        ctx.fonts(|f| f.glyph_width(&font, c))
    }

    /// 시스템 CJK 글꼴에 없을 수 있는 네 글자. `with_app_fonts`는 시스템 글꼴을 넣지 않으므로
    /// 본문 체인에는 없고 mono에는 있다. 본문이 D2Coding을 얻지 않는지도 함께 잰다.
    #[test]
    fn a_ui_sans_label_draws_in_mono_what_its_chain_lacks_and_the_body_stays_as_it_was() {
        with_app_fonts(1.0, |ctx| {
            let has = |family: egui::FontFamily, c| {
                let font = egui::FontId::new(LABEL_SIZES[1], family);
                ctx.fonts(|f| f.has_glyph(&font, c))
            };
            for c in ['\u{30fc}', '\u{25e6}', '\u{2298}', '\u{25d2}'] {
                assert!(has(label_font_family_for("ja"), c), "label {c}");
                assert!(has(egui::FontFamily::Monospace, c), "mono {c}");
                assert!(!has(egui::FontFamily::Proportional, c), "body {c}");
                assert_eq!(
                    advance(ctx, label_font_family_for("ja"), c),
                    advance(ctx, egui::FontFamily::Monospace, c),
                    "{c}"
                );
            }
        });
    }

    /// 마지막 대체는 체인 뒤에만 있다. ◑은 본문 체인의 emoji-icon-font와 D2Coding 양쪽에 있어
    /// 어느 글꼴이 그리는지 폭으로 갈린다.
    #[test]
    fn the_mono_fallback_stays_behind_the_whole_chain() {
        with_app_fonts(1.0, |ctx| {
            let c = '\u{25d1}';
            let body = advance(ctx, egui::FontFamily::Proportional, c);
            assert_ne!(body, advance(ctx, egui::FontFamily::Monospace, c));
            assert_eq!(advance(ctx, label_font_family_for("ja"), c), body);
        });
    }

    /// 다른 언어의 라벨은 Monospace family 그대로라 체인이 달라지지 않는다.
    #[test]
    fn other_languages_do_not_take_the_ui_sans_chain() {
        with_app_fonts(1.0, |ctx| {
            for code in ["en", "ko"] {
                let c = '\u{25d1}';
                assert_eq!(
                    advance(ctx, label_font_family_for(code), c),
                    advance(ctx, egui::FontFamily::Monospace, c),
                    "{code}"
                );
            }
        });
    }
}
