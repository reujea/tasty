//! 바탕 kind 페이지에 바탕 포커스 상태를 알리는 계약. 페이지는 이 속성이 정의됐는지만 보고 두 상태의
//! 값을 자기 CSS에서 고른다. 계약과 이유는 `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

/// 바탕 포커스 상태인 동안 문서 `:root`에 빈 값으로 정의되는 CSS 사용자 정의 속성.
/// 상태가 아니면 정의되지 않는다. 페이지는 `var(--tasty-bed-focused) <값>`으로 focus 값을 만들고
/// 정의되지 않았을 때의 대체 값으로 unfocus 값을 둔다. 호스트 코드는 아래 스타일시트만 쓰므로
/// 이 이름은 시험이 스타일시트와 페이지 쪽 사본을 대조할 때만 읽는다.
#[cfg(test)]
pub(crate) const BED_FOCUSED_PROPERTY: &str = "--tasty-bed-focused";

/// 상태를 만드는 고정 스타일시트. 호스트는 이 문자열 말고는 페이지에 넣지 않는다.
pub(crate) const BED_FOCUSED_SHEET: &str = ":root{--tasty-bed-focused: ;}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_defines_only_the_property_with_an_empty_value() {
        assert_eq!(
            BED_FOCUSED_SHEET,
            format!(":root{{{BED_FOCUSED_PROPERTY}: ;}}")
        );
    }

    /// 번들 markdown 페이지가 같은 이름을 읽는다. 플러그인은 호스트 크레이트에 의존하지 않아 사본을 둔다.
    #[test]
    fn the_bundled_markdown_page_reads_the_same_property() {
        let page = include_str!("../../../crates/tasty-plugin-markdown/src/render/bed_focus.rs");
        let decl = format!("const BED_FOCUSED_PROPERTY: &str = \"{BED_FOCUSED_PROPERTY}\";");
        assert!(page.contains(&decl), "{decl}");
    }
}
