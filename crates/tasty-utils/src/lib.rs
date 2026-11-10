#![deny(unsafe_code)]

//! 다른 tasty 크레이트에 의존하지 않는 공용 경로·식별자·프로세스 유틸리티.
//! notify는 완료 로그의 경로와 쓰기를, target은 호스트·plugin이 공유하는 프로토콜 문구를 제공한다.
//! batch_writer는 저장소별 전담 writer가 공유하는 접수·묶음·완료·종료 실행기다.
//! file_replace는 설정 파일의 원자 교체·쓰기 잠금·내용 관측값이다.
//! 테마·설정·DB 등 기능별 경로는 각 크레이트가 path::tasty_home 위에 정의한다.
//! path_resolve는 한 Tasty 프로세스 안에서 같은 입력을 같은 경로로 푸는 규칙이다.

pub mod batch_writer;
pub mod execution;
pub mod file_replace;
pub mod html_address;
pub mod id;
pub mod notify;
pub mod path;
pub mod path_resolve;
pub mod plugin_id;
pub mod plugin_namespaces;
pub mod poison;
pub mod process;
pub mod shell_family;
pub mod target;
/// Full Windows file identity captured from a borrowed live file handle.
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "stable std lacks full Windows file IDs; this module bounds the native query behind a safe borrowed-file API"
)]
pub mod windows_file_identity;
