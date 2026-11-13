//! 바탕 kind 페이지에 바탕 포커스 상태를 알리는 스타일시트. 속성 이름은 페이지와 함께 쓰는 계약이라
//! `tasty_plugin_protocol::bed_focus`에 있다. 계약과 이유는 `docs/design/systems/webview.md`의
//! "바탕 포커스 상태" 절에 있다.

use tasty_plugin_protocol::bed_focus::BED_FOCUSED_PROPERTY;

/// 상태를 만드는 고정 스타일시트. 계약 속성을 빈 값으로 정의하는 것뿐이다. 호스트는 이 문자열 말고는
/// 페이지에 넣지 않는다.
pub(crate) fn bed_focused_sheet() -> String {
    format!(":root{{{BED_FOCUSED_PROPERTY}: ;}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_defines_only_the_property_with_an_empty_value() {
        assert_eq!(bed_focused_sheet(), ":root{--tasty-bed-focused: ;}");
    }
}
