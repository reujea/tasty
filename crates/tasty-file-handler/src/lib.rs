//! 파일 핸들러 시스템.
//!
//! `FileHandlerRegistry` 가 detector → handler 매핑을 관리한다. `file_format` 의
//! evaluator / rule kind 를 모르고, `DetectorId` 만 import 한다.

pub mod config;
pub mod recent;
pub mod registry;
pub mod save;
pub mod types;

pub use config::{HandlerDeclError, UserHandlerActionDecl};
pub use registry::{
    FileHandlerRegistry, RejectedUserHandler, UserHandlerRejectReason, UserHandlerUpsertDecl,
};
pub use types::{FileHandler, HandlerAction, HandlerId, HandlerOwner, is_valid_handler_short_name};

/// 호스트가 기본 제공하는 handler 선언 묶음.
///
/// 이 파일은 크레이트 안에 있고 `include_str!` 로 바이너리에 박힌다. 본체와 시험이 각자
/// 상대 경로로 같은 파일을 박으면 경로가 크레이트 밖을 가리키게 된 순간 그 자리마다 조용히
/// 깨진다 — 자매 크레이트 `tasty-file-format` 이 같은 이유로 같은 모양을 쓴다. 값을 여기
/// 한 번만 두고 소비처가 이름으로 부른다.
pub const HOST_DEFAULTS_TOML: &str = include_str!("defaults/default-file-handlers.toml");
