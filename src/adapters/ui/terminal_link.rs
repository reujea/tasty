//! 터미널 링크 — 검출과 타입은 `tasty-terminal-link` 크레이트에 있고, 여기에는 **링크를
//! 여는 일**만 남는다.
//!
//! 나눈 기준은 부수효과다. 검출은 화면 텍스트에서 범위를 계산하는 순수 계산이라 헤드리스
//! 에서도 성립하지만, 여는 것은 기본 브라우저를 부르는 OS 동작이고 그 의존(`webbrowser`)은
//! 본체의 `gui` feature 뒤에 있다. 크레이트로 함께 내리면 헤드리스 빌드가 그 의존을
//! 떠안는다.
//!
//! 호출부는 종전대로 `terminal_link::` 하나로 둘 다 부른다 — 아래 재수출이 잇는다.

pub use tasty_terminal_link::*;

/// URI를 기본 브라우저/연결 프로그램으로 연다. 크로스 플랫폼.
/// 성공하면 true, 실패하면 `tracing::warn!`을 남기고 false.
pub fn open_uri(uri: &str) -> bool {
    #[cfg(debug_assertions)]
    if crate::platform::debug_os_open::intercepted("open_uri", uri) {
        return true;
    }
    match webbrowser::open(uri) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("failed to open link {:?}: {}", uri, e);
            false
        }
    }
}
