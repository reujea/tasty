//! DTCG 디자인 토큰을 저장하고 Rust 상수와 Theme 접근자를 생성한다.
//! primitive → semantic → component, 총 1530 토큰을 `dtcg/tasty.tokens.json`에 보관한다.
//! 생성 결과는 커밋하며 `tests/freshness.rs`가 현재 생성기 출력과 비교한다.
//!
//! # Tier 규율 (컴파일 타임 강제)
//!
//! `generated::primitive`는 `pub(crate)`이므로 외부 크레이트에서 직접 읽을 수 없다.
//!
//! # zoom 우회 금지 (필수)
//!
//! 생성된 치수 상수는 SIZING 초기값과 대조 시험에 쓴다. 위젯은 Theme 필드나 접근자로
//! 치수를 읽어야 `with_colors_and_zoom()`의 배율 적용·반올림·제외 정책을 따른다.
//! 배율 제외 항목은 tasty-type-appearance의 검사 목록에서 관리한다. 여기에는 hairline,
//! 탭바, 상태바, CSD 타이틀바, 렌더 콘텐츠 폰트가 포함된다.
//!
//! # 색 토큰
//!
//! 색 상수는 생성하지 않는다. Theme 접근자가 런타임 테마의 색을 반환하며
//! `tests/color_drift.rs`는 DTCG와 내장 테마의 대응 값을 비교한다.
//!
//! # vendor 갱신
//!
//! 이 크레이트의 README 절차에 따라 원격 파일을 받고 생성기를 실행한다.
//! freshness는 CI의 check-headless 잡에서 실행하며 기본 조합 잡의 `--lib --bins`에는
//! 포함되지 않는다. 커밋 전에 `cargo test -p tasty-design-tokens`를 직접 실행한다.

pub mod dtcg;
pub mod generated;

/// vendor 된 DTCG 토큰 파일 원문. 파서/생성기/테스트가 공유하는 단일 입력.
pub const DTCG_JSON: &str = include_str!("../dtcg/tasty.tokens.json");
