//! SDK 도메인 에러.
//!
//! [`PluginError`]는 SDK 함수에서 발생할 수 있는 모든 실패 모드를 표현한다.
//! Plugin 작성자는 `?`로 흘려 전파하거나, 분기가 필요할 때 variant로 매칭한다.
//!
//! IPC 응답 표면에서는 [`crate::plugin::IpcMethodError`]를 그대로 쓴다. SDK
//! 내부 에러를 IPC 응답으로 흘릴 때는 `From<PluginError> for IpcMethodError`
//! 변환이 자동으로 동작한다.

use std::time::Duration;

/// SDK 함수의 표준 결과 타입.
pub type Result<T, E = PluginError> = std::result::Result<T, E>;

/// SDK 작업 중 발생할 수 있는 에러.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// 필수 환경변수가 비어 있다.
    #[error("missing env var: {0}")]
    EnvMissing(&'static str),

    /// 환경변수 파싱 실패 (예: 포트 번호가 숫자 아님).
    #[error("invalid env var {var}: {message}")]
    EnvParse { var: &'static str, message: String },

    /// 호스트 TCP 포트에 connect 실패.
    #[error("failed to connect to host on port {port}: {source}")]
    Connect {
        port: u16,
        #[source]
        source: std::io::Error,
    },

    /// 호스트가 connection을 닫음.
    ///
    /// [`crate::host::HostHandle::call`] 도 이 값을 돌려준다 — 호스트가 shutdown 을
    /// 요청했거나 연결이 끊겨 SDK 가 결과를 더 받지 않을 때다. 기다려도 오지 않는다.
    #[error("host closed connection")]
    HostClosed,

    /// 호스트가 핸드셰이크를 거부함 (토큰 불일치 등). reason은 호스트가
    /// AuthAck에 담아 보낸 사유.
    #[error("host rejected handshake: {}", reason.as_deref().unwrap_or("(no reason)"))]
    HandshakeRejected { reason: Option<String> },

    /// 호스트가 AuthAck를 보내기 전 타임아웃. 호스트가 죽었거나, 토큰
    /// 매칭 단계에서 stream을 silent drop했을 가능성.
    #[error("host did not send auth_ack within timeout")]
    HandshakeTimeout,

    /// 호스트 호출 응답에 호스트가 명시한 에러.
    ///
    /// `code` 는 호스트가 준 JSON-RPC 코드다. **표시 문구에는 안 들어간다** —
    /// 이 메시지 모양을 읽는 소비자가 이미 있고(예: agent-stream 의 "그런 surface 는
    /// 없다" 판정), 코드를 더하는 것이 그 판정을 깨서는 안 된다. 코드는
    /// [`crate::plugin::IpcMethodError`] 로 변환될 때 쓰인다.
    ///
    /// `None` 은 "호스트가 코드를 안 줬다" 이고, 그때만 server error(-32000)로 떨어진다.
    #[error("host call '{method}' failed: {message}")]
    HostCall {
        method: String,
        message: String,
        code: Option<i32>,
    },

    /// 호스트 호출이 timeout 안에 응답을 받지 못함.
    #[error("host call '{method}' timed out after {timeout:?}")]
    HostCallTimeout { method: String, timeout: Duration },

    /// 일반 IO 에러.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 인코딩/디코딩 실패.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// Mutex가 poison된 상태에서 lock 시도.
    #[error("mutex poisoned: {0}")]
    LockPoisoned(&'static str),

    /// 보조 핸들 채널이 필요한 동작을 호출했으나 채널이 활성화되지 않았다 (host가 endpoint를
    /// 전달하지 않았거나 SDK가 connect에 실패함).
    #[error("plugin handle channel not available — shared buffer features disabled")]
    HandleChannelUnavailable,

    /// `tasty-shm` 영역 매핑 실패.
    #[error("shared memory error: {0}")]
    Shm(String),

    /// [`crate::host::HostHandle::self_invoke`] 호출 시점에 worker 큐가 아직
    /// 준비되지 않았거나(비정상 초기화 순서) 이미 종료됨(plugin shutdown 경합).
    #[error("self-invoke queue unavailable — worker thread not ready or already exited")]
    SelfInvokeUnavailable,
}
