//! 파일 작업 — 형식 식별 / 핸들러 / 드래그 / 디스패치.

// 파일 열기 dispatch — 부르는 자리가 전부 GUI 다. 링크 해석·대상 판정은 시험이 headless 에서도 부른다.
#[cfg(any(feature = "gui", test))]
pub mod dispatch;
#[cfg(feature = "gui")]
pub mod drag;
#[cfg(feature = "gui")]
pub mod identify_worker;

/// 파일 형식 식별 — 실체는 `tasty-file-format` 크레이트에 있다(호스트 결합이 0 이라
/// leaf 로 올렸다). 본체 코드가 `crate::file::format::…` 로 계속 부르도록 이름만 잇는다.
pub use tasty_file_format as format;

/// 파일 핸들러 정책·레지스트리 — 실체는 `tasty-file-handler` 크레이트에 있다(호스트·GUI
/// 결합이 0 이라 도메인-IO 로 내렸다). 본체 코드가 `crate::file::handler::…` 로 계속
/// 부르도록 이름만 잇는다.
pub use tasty_file_handler as handler;
