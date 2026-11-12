#![forbid(unsafe_code)]

//! CLI·GUI·로컬 IPC가 공유하는 원격 workspace 조회와 생성.
//! tasty-ssh로 연결한 뒤 JSON-RPC를 보내며 로컬 화면 상태는 변경하지 않는다.

//! # 공개 계약
//!
//! 아래 항목은 호스트·CLI가 공유한다. doc(hidden) 항목은 CLI 구현 내부용이다.
//!
//! | 항목 | 소비자 |
//! |------|--------|
//! | [`browse::RemoteWorkspace`] · [`browse::browse`] · [`browse::browse_via_port`] | 본체 · CLI |
//! | [`browse::resolve_connection_spec`] · [`browse::resolve_endpoint`] | 본체 · CLI |
//! | [`browse::parse_loopback_port`] | 자기 포트 attach를 처리 시점에 거절하는 본체 |
//! | [`self_instance::refuse_this_instance`] · [`self_instance::ThisInstance`] · [`self_instance::refuse_this_instance_over`] · [`self_instance::TunnelVerdict`] | SSH 너머의 자기 자신을 거절하고, 원격 포트의 리스너 없음으로 끊긴 검사에 그 판정을 붙이는 본체 |
//! | [`create::create_via_port`] ([`create::CreatedRemoteWorkspace`] 는 그 반환형) | 본체 · CLI |
//! | [`browse::probe_method`] | 상황별 오류 안내를 만드는 호스트 호출자 |
//! | [`browse::PROBE_TIMEOUT`] | 소비자가 진행 표시·문구를 같은 값에 맞추도록 노출(`docs/dev-guide/attach-behavior.md#ssh-터널-원격-client-공통`) |

pub mod browse;
pub mod create;

pub mod client_session;
/// Persistent connection/session ownership shared by host adapters without App/View dependencies.
pub mod connection;
pub mod html_body;
pub mod html_document;
pub mod html_http_body;
pub mod html_http_bridge;
pub mod html_http_client;
pub mod html_http_native;
pub mod html_http_session;
pub mod html_mirror;
pub mod html_source;
mod input_receipts;
pub mod outbound;
pub mod pending_request;
pub mod transport;

pub mod pending_connection;

/// mirror 크기 요청의 응답 대기·재시도·실패 목록.
pub mod resize_sync;

pub mod refusal;

pub mod self_instance;

pub mod browser;

pub mod profile_detection;
