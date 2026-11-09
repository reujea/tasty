//! 응용 로직이 돌려주는 오류 — 탭 · 워커 · IPC 처리가 함께 쓰는 사유.
//!
//! 로직은 IPC SDK 의 오류 타입을 모른다. 바깥에 답하는 쪽(`ipc.rs`)이 이것을 IPC 오류로 옮기고,
//! 그때 데몬 오류의 종류를 보고 코드를 고른다. 사유를 문자열로 접어 두면 그 종류가 사라져
//! 소비자가 재시도 여부를 못 정하므로 데몬 오류는 원래 값으로 들고 간다.

use crate::ports::supervisor::ClientError;

#[derive(Debug)]
pub enum AppError {
    /// 사람에게 보일 사유 한 줄 — 요청이 모자라거나 맞지 않는 상태다.
    Message(String),
    /// 데몬 경계의 오류 — 종류(전제 미충족 · 거절 · 결함)를 IPC 코드로 옮길 때 본다.
    Supervisor(ClientError),
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(m) => f.write_str(m),
            Self::Supervisor(e) => write!(f, "{e}"),
        }
    }
}

impl From<ClientError> for AppError {
    fn from(e: ClientError) -> Self {
        Self::Supervisor(e)
    }
}
