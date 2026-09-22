//! Test mock adapter — `src/ports/` 의 7 external trait 의 deterministic mock.
//!
//! Core 의 단위 test 시 외부 자원 없이 도메인 로직 검증. 모든 in-memory.

// 이 mock adapter 모듈은 `#[cfg(test)]` 로 게이트되어(`adapters/mod.rs`) 비-test 빌드에는
// 컴파일되지 않는다. test 빌드에선 mock 들이 전부 사용되므로 dead_code allow 가 불필요하다.

pub mod fake_clock;
pub mod mem_fs;
pub mod mock_clipboard;
pub mod mock_ipc_server;
pub mod mock_process;
pub mod mock_waker_factory;
pub mod tmp_home;
