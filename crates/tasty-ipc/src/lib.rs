#![forbid(unsafe_code)]

//! 호스트와 CLI·에이전트·플러그인이 공유하는 IPC 전송 형식과 호출자 모델.
//! 도메인 핸들러와 실제 서버 adapter는 호스트 본체가 구현한다.

pub mod admission;
pub mod agent_child;
pub mod alias;
pub mod caller;
pub mod capability;
pub mod client;
pub mod dispatch;
pub mod explorer_op;
pub mod file_download;
pub mod host_call;
pub mod host_port;
pub mod html_http;
pub mod html_transfer;
pub mod instance;
pub mod ipc_namespace;
pub mod mesh_stream;
pub mod method_meta;
pub mod output_cursor;
pub mod port_file;
pub mod progress;
pub mod protocol;
pub mod server;
pub mod session;
pub mod stream;
pub mod stream_hub;
pub mod terminal_read;

pub use host_port::{AuditCallerMarker, AuditDecision, IpcHostFacade, SessionResolution};
