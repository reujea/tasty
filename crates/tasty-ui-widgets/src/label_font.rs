//! `font-label`: 번역 카탈로그의 낱말을 그리는 작은 라벨의 글꼴. 값은 문자열이 아니라 UI 언어로 정한다.
//!
//! ja와 zh에서는 고정폭 글꼴에 없는 한자가 많아 고정폭 체인으로 그리면 한 낱말 안에서 글꼴이
//! 바뀐다. 라틴 글자의 고정폭을 지키려면 D2Coding이 체인 맨 앞에 있어야 하므로 글꼴 순서로는 풀 수
//! 없다. 경로·ID·버전·숫자 같은 값은 이 글꼴을 읽지 않고 모든 언어에서 고정폭으로 그린다.
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
        egui::FontFamily::Proportional
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
        for code in ["ja", "ja-JP", "zh", "zh-Hans", "zh-CN", "ZH_cn", "JA"] {
            assert_eq!(
                label_font_family_for(code),
                egui::FontFamily::Proportional,
                "{code:?}"
            );
        }
    }
}
