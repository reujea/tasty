//! 공유 훅 핸들러 레지스트리 (webhook/hook 트리거 공유).
//!
//! 파일 핸들러(`crates/tasty-file-handler/src/`)를 정본 템플릿으로 미러링한다. S1b 로 3출처 병합
//! (host embedded TOML + plugin manifest + user config) + patch semantics + user
//! config 영속화(`~/.tasty/hook-handlers.toml`)까지 정식화됐다. `source` 바인딩
//! 게이트와 IpcSequence 실행 코어는 S1a 부터.
//!
//! 핵심 불변식은 [`types`] 모듈 문서 참조 — 데이터/흐름 분리(`IpcCall.method`
//! 고정) + 셸 웹훅 거부(`ShellCommand` 는 webhook 바인딩 불가).

pub mod config;
pub mod env;
pub mod exec;
pub mod registry;
pub mod trigger;
pub mod types;

// 편의 re-export — 현재 소비되는 심볼만 노출한다. 나머지(config 의 `HookHandlerDecl` /
// host·plugin action decl, `HookHandlerRegistry` / `RegistryError` 등)는 소비 시점에
// 추가한다. 전체 경로(`registry::` / `types::` / `exec::` / `config::`)로는 항상 접근 가능.
pub use config::UserHookHandlerActionDecl;
pub use env::{HookShellEnv, build_env};
pub use exec::{
    SequenceNotQueued, SequenceOrigin, SubstitutionContext, enqueue_sequence, execute_sequence,
    spawn_shell,
};
pub use registry::{
    HostHookHandlerPort, UserHookHandlerUpsertDecl, global, install_default_sources,
    user_config_path,
};
pub use types::{
    HookHandler, HookHandlerAction, HookHandlerId, HookHandlerOwner, HookSource, IpcCall,
    TriggerSource, validate_binding,
};
