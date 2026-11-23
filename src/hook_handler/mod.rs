//! 내부 훅과 웹훅이 공유하는 핸들러 정의 registry와 실행 부품.
//! 발화한 훅의 바인딩 실행, 셸 명령 공유 실행기, IpcSequence 대기열은 hook_runtime이 소유한다.
//! host·plugin·사용자 설정을 병합하고 트리거 출처와 셸 실행 제한을 확인한다.

pub mod config;
pub mod env;
pub mod exec;
pub mod outcome;
pub mod registry;
// 텍스트 편집기는 설정 창(gui)에서만 쓴다.
#[cfg(feature = "gui")]
pub mod sequence_text;
pub mod types;
mod user_file;

pub use config::UserHookHandlerActionDecl;
pub use env::{HookShellEnv, build_env};
pub use exec::{SequenceOrigin, SubstitutionContext, dispatch_command, execute_sequence};
pub use registry::{
    HostHookHandlerPort, UserHookHandlerUpsertDecl, global, install_default_sources,
};
pub use types::{
    HookHandler, HookHandlerAction, HookHandlerId, HookHandlerOwner, HookSource, IpcCall,
    TriggerSource, validate_binding,
};
