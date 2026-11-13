//! agent task 회차 표지 계약. 호스트가 agent task 지시 끝에 회차 표지를 붙이고 provider 플러그인이
//! 프롬프트에서 같은 앞부분으로 토큰을 찾는다.

/// 회차 표지의 앞부분. 표지는 `[tasty-task-attempt:<token>]`이다. 호스트의 작업 실행기
/// (`tasty-agent`)는 플러그인 계약 크레이트에 의존하지 않아 같은 값을 따로 두고, 루트 패키지의
/// 시험이 두 값을 대조한다.
pub const ATTEMPT_MARKER_PREFIX: &str = "[tasty-task-attempt:";
