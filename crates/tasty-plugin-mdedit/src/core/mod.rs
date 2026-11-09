//! 편집기와 IPC 가 함께 쓰는 순수 로직 — 호스트·웹뷰를 모른다. 단위 시험이 여기 모인다.

pub mod blocks;
pub mod diff;
pub mod guard;
pub mod leak;
pub mod merge;
pub mod ops;
pub mod outline;
pub mod review;
pub mod snapshot;
pub mod textio;
pub mod units;
