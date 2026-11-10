//! 스트림 읽기 실패의 원인 분류. client와 server가 같은 판정으로 끊김 원인을 로그에 남긴다.

use std::io;
use std::time::Duration;

/// [`super::read_frame`]이 실패한 원인. errno는 OS마다 달라 `io::ErrorKind`로 판정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    /// 소켓 read timeout 만료. macOS·Linux는 `WouldBlock`(EAGAIN), Windows는 `TimedOut`으로 온다.
    /// 프레임 일부를 받는 도중에 만료돼도 같은 kind라 "아무 프레임도 받지 못함"을 뜻하지 않는다.
    HeartbeatTimeout,
    /// 상대가 연결을 닫거나(EOF) 재설정했다.
    PeerClosed,
    /// 모르는 tag나 길이 상한 초과 같은 프레임 형식 오류.
    Protocol,
    /// 위에 속하지 않는 수신 오류.
    Other,
}

impl ReadFailure {
    pub fn of_io(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Self::HeartbeatTimeout,
            io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted => Self::PeerClosed,
            io::ErrorKind::InvalidData => Self::Protocol,
            _ => Self::Other,
        }
    }

    /// `StreamConnection::recv`처럼 `io::Error`를 `anyhow`로 감싼 결과를 분류한다.
    pub fn of_error(error: &anyhow::Error) -> Self {
        error
            .downcast_ref::<io::Error>()
            .map_or(Self::Other, Self::of_io)
    }

    /// 로그에 넣는 원인 문구. `timeout`은 이 소켓에 실제로 건 read timeout이다.
    pub fn describe(self, timeout: Duration) -> String {
        match self {
            Self::HeartbeatTimeout => format!(
                "하트비트 timeout: 수신 대기 timeout({}초) 안에 프레임을 끝까지 받지 못했다",
                timeout.as_secs()
            ),
            Self::PeerClosed => "원격이 연결을 닫았다".to_string(),
            Self::Protocol => "프로토콜 오류: 프레임 형식이 맞지 않는다".to_string(),
            Self::Other => "기타 수신 오류".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_read_error_kind_maps_to_its_cause() {
        let cases = [
            (io::ErrorKind::WouldBlock, ReadFailure::HeartbeatTimeout),
            (io::ErrorKind::TimedOut, ReadFailure::HeartbeatTimeout),
            (io::ErrorKind::UnexpectedEof, ReadFailure::PeerClosed),
            (io::ErrorKind::ConnectionReset, ReadFailure::PeerClosed),
            (io::ErrorKind::ConnectionAborted, ReadFailure::PeerClosed),
            (io::ErrorKind::InvalidData, ReadFailure::Protocol),
            (io::ErrorKind::PermissionDenied, ReadFailure::Other),
        ];
        for (kind, expected) in cases {
            let error = io::Error::from(kind);
            assert_eq!(ReadFailure::of_io(&error), expected, "{kind:?}");
            assert_eq!(
                ReadFailure::of_error(&anyhow::Error::from(io::Error::from(kind))),
                expected,
                "{kind:?} wrapped in anyhow"
            );
        }
    }

    #[test]
    fn a_non_io_error_is_other() {
        assert_eq!(
            ReadFailure::of_error(&anyhow::anyhow!("expected Control ack frame")),
            ReadFailure::Other
        );
    }

    #[test]
    fn the_timeout_cause_states_the_armed_seconds() {
        let text = ReadFailure::HeartbeatTimeout.describe(Duration::from_secs(45));
        assert!(text.contains("45초"), "{text}");
        assert!(!text.contains("아무 프레임"), "{text}");
    }

    #[test]
    fn a_frame_with_an_unknown_tag_is_a_protocol_failure() {
        let bytes = [0xEEu8, 0, 0, 0, 0];
        let error = super::super::read_frame(&mut std::io::Cursor::new(bytes)).unwrap_err();
        assert_eq!(ReadFailure::of_io(&error), ReadFailure::Protocol);
    }

    #[test]
    fn a_stream_that_ends_mid_header_is_a_peer_close() {
        let bytes = [0u8, 0];
        let error = super::super::read_frame(&mut std::io::Cursor::new(bytes)).unwrap_err();
        assert_eq!(ReadFailure::of_io(&error), ReadFailure::PeerClosed);
    }
}
