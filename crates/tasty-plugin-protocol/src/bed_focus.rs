//! 바탕 kind 페이지에 바탕 포커스 상태를 알리는 계약. 호스트가 문서 `:root`에 속성을 정의하고 페이지는
//! 그 속성이 정의됐는지만 보고 두 상태의 값을 자기 CSS에서 고른다. 계약과 이유는
//! `docs/design/systems/webview.md`의 "바탕 포커스 상태" 절에 있다.

/// 바탕 포커스 상태인 동안 문서 `:root`에 빈 값으로 정의되는 CSS 사용자 정의 속성.
/// 상태가 아니면 정의되지 않는다. 페이지는 `var(--tasty-bed-focused) <값>`으로 focus 값을 만들고
/// 정의되지 않았을 때의 대체 값으로 unfocus 값을 둔다.
pub const BED_FOCUSED_PROPERTY: &str = "--tasty-bed-focused";
