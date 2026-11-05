#![forbid(unsafe_code)]

//! 색·테마 타입과 UI 치수를 정의한다.
//! 저장소 내부 의존성은 다른 tasty-type-* 크레이트로 제한하며 순환을 허용하지 않는다.
//! 테마 파일 입출력과 설정 관리는 상위 크레이트에서 처리한다.
//!
//! HexColor는 직렬화용 straight RGBA이고 GpuRgba/GpuRgb는 GPU 배열과 같은 메모리 표현이다.
//! 테마 색은 파일이나 tasty-themes의 기본 팔레트에서 정의해 정상 변환 함수를 사용한다.
//! dangerously_force_*는 외부 픽셀·복원 데이터·테스트 값에만 사용한다.
//! 자세한 허용 범위는 docs/design/systems/theme.md#색-생성-정책을 따른다.

// 이유: 테스트에서 사용하지 않는 반환값을 버리는 것은 허용한다.
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

pub mod color;
pub mod motion;
pub mod theme;
pub mod toast_kind;

/// 탐색기 영역 선택의 color-mix 수기 접근자 (`impl Theme`).
mod explorer_marquee;

/// 승인 위험 선택지 채움의 color-mix 수기 접근자 (`impl Theme`).
mod approval_danger;

/// 경고 알림 배지의 테마별 대비를 위한 수기 접근자 (`impl Theme`).
mod notification_ink;
mod state_ink;

/// DAG 표시 색의 color-mix 수기 접근자 (`impl Theme`).
mod dag_mix;

/// 값 없는 검색 active 글자색의 도출 (`impl ThemeColors`).
mod search_ink;

/// `tasty-design-tokens` 생성기가 산출하는 semantic 색 접근자 (`impl Theme`).
/// DO NOT EDIT — `cargo run -p tasty-design-tokens --bin generate` 로 재생성.
mod semantic_color_generated;

/// `tasty-design-tokens` 생성기가 산출하는 component 접근자 (`impl Theme`).
/// DO NOT EDIT — `cargo run -p tasty-design-tokens --bin generate` 로 재생성.
mod generated_component;

/// `tasty-design-tokens` 생성기가 산출하는 em 자간 접근자 (`impl Theme`).
/// DO NOT EDIT — `cargo run -p tasty-design-tokens --bin generate` 로 재생성.
mod generated_tracking;

/// 그림자 선택 정책의 소스 검사. CI의 패키지 lib 검사에 포함한다.
#[cfg(test)]
mod shadow_policy_guard;
#[cfg(test)]
mod zoom_coverage_guard;
#[cfg(test)]
mod zoom_exempt_fields_guard;
