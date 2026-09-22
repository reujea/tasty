//! 호스트와의 양방향 NDJSON 채널.
//!
//! Plugin 부팅 흐름:
//! 1. [`Connection::connect`] — 호스트 포트에 TCP connect (인증 전 상태).
//! 2. [`Connection::authenticate`] — [`AuthMessage`] 송신 후 호스트의 `AuthAck`
//!    응답을 5초 안에 받으면 본 루프 진입. 거부 시 [`PluginError::HandshakeRejected`],
//!    무응답 시 [`PluginError::HandshakeTimeout`].
//! 3. [`Connection::send_event`]로 호스트에 알림 송신 (Hello, Log, IpcCall 등).
//! 4. [`Connection::try_recv`]로 호스트의 요청을 한 줄씩 수신.
//!
//! 편의 메서드 [`Connection::connect_and_authenticate`]는 1+2를 합친 것이며
//! 기존 호출자 호환을 위해 남아 있다.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use tasty_plugin_protocol::{
    AuthMessage, PluginEvent, PluginRequest, PluginResponse, protocol::AuthAckEnvelope,
};

use crate::env::PluginEnv;
use crate::error::{PluginError, Result};

/// 호스트가 AuthAck를 보낼 때까지 기다리는 최대 시간.
/// 호스트 측 `AUTH_READ_TIMEOUT`(5s)과 동일하게 맞춰 둔다.
pub(crate) const AUTH_ACK_TIMEOUT: Duration = Duration::from_secs(5);
/// 정상 메시지 루프에서의 read timeout. `try_recv` 패턴을 흉내내기 위해 짧게.
const RUN_READ_TIMEOUT: Duration = Duration::from_millis(50);

pub struct Connection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

/// 호스트가 plugin에 보낼 수 있는 메시지.
#[derive(Debug, Clone)]
pub enum HostMessage {
    Request(PluginRequest),
}

impl Connection {
    /// 호스트 포트에 TCP connect만 한다. AuthMessage는 아직 송신하지 않는다.
    /// 후속으로 [`Connection::authenticate`]를 반드시 호출해야 핸드셰이크가 완료된다.
    pub fn connect(env: &PluginEnv) -> Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", env.host_port)).map_err(|source| {
            PluginError::Connect {
                port: env.host_port,
                source,
            }
        })?;
        // 메시지마다 작은 쓰기가 오가는 요청/응답 채널이라 Nagle 을 끈다. 켜 두면 한
        // 메시지의 뒤 조각이 앞 조각의 ACK 를 기다리고, 받는 쪽은 ACK 를 최대 수십 ms
        // 미루므로 왕복마다 그만큼이 붙는다. 호스트도 자기 끝에서 같은 설정을 한다
        // (`docs/dev-guide/plugin-development.md` "전송 지연"). 실패해도 채널은 동작하므로
        // 기록만 한다.
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("plugin: TCP_NODELAY on the host channel failed: {e}");
        }
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }

    /// AuthMessage를 송신하고 호스트의 AuthAck 한 줄을 [`AUTH_ACK_TIMEOUT`] 안에
    /// 수신한다. ok=false면 [`PluginError::HandshakeRejected`], 시간 안에 안 오면
    /// [`PluginError::HandshakeTimeout`].
    ///
    /// 성공하면 read timeout을 짧은 값(50ms)으로 복구해 try_recv 패턴이 동작한다.
    pub fn authenticate(&mut self, env: &PluginEnv) -> Result<()> {
        let auth = AuthMessage {
            plugin_id: env.plugin_id.clone(),
            token: env.token.clone(),
        };
        let line = serde_json::to_string(&auth)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;

        // AuthAck 한 줄 수신. 호스트가 silent drop했던 과거 버그를 막기 위함.
        self.writer.set_read_timeout(Some(AUTH_ACK_TIMEOUT))?;
        // reader 쪽도 같은 stream이므로 read_timeout이 공유된다.
        let mut ack_line = String::new();
        let read_result = self.reader.read_line(&mut ack_line);
        // 정상 루프 timeout으로 복구.
        self.writer.set_read_timeout(Some(RUN_READ_TIMEOUT))?;
        match read_result {
            Ok(0) => Err(PluginError::HandshakeTimeout),
            Ok(_) => {
                let trim = ack_line.trim();
                if trim.is_empty() {
                    return Err(PluginError::HandshakeTimeout);
                }
                let env_msg: AuthAckEnvelope = serde_json::from_str(trim)?;
                if env_msg.auth_ack.ok {
                    Ok(())
                } else {
                    Err(PluginError::HandshakeRejected {
                        reason: env_msg.auth_ack.reason,
                    })
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Err(PluginError::HandshakeTimeout)
            }
            Err(e) => Err(PluginError::Io(e)),
        }
    }

    /// [`connect`](Self::connect) + [`authenticate`](Self::authenticate) 조합.
    pub fn connect_and_authenticate(env: &PluginEnv) -> Result<Self> {
        let mut conn = Self::connect(env)?;
        conn.authenticate(env)?;
        Ok(conn)
    }

    /// 내부 stream/reader 분리. runtime이 Arc<Mutex<TcpStream>>로 감싸기 위해 사용.
    pub fn into_parts(self) -> (TcpStream, BufReader<TcpStream>) {
        (self.writer, self.reader)
    }

    /// `{"event": <PluginEvent>}` 형태로 호스트에 알림 송신.
    pub fn send_event(&mut self, event: &PluginEvent) -> Result<()> {
        let payload = serde_json::json!({ "event": event });
        let line = serde_json::to_string(&payload)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        Ok(())
    }

    /// 호스트의 요청에 대한 응답. id는 원 요청의 id를 그대로 echo.
    pub fn send_response(&mut self, response: &PluginResponse) -> Result<()> {
        let line = serde_json::to_string(response)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        Ok(())
    }

    /// (테스트용) 짧은 AuthAck 타임아웃으로 authenticate 수행. handshake 거부/timeout
    /// 분기를 빠르게 검증하기 위해 사용.
    #[cfg(test)]
    fn authenticate_with_timeout(&mut self, env: &PluginEnv, timeout: Duration) -> Result<()> {
        let auth = AuthMessage {
            plugin_id: env.plugin_id.clone(),
            token: env.token.clone(),
        };
        let line = serde_json::to_string(&auth)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        self.writer.set_read_timeout(Some(timeout))?;
        let mut ack_line = String::new();
        let read_result = self.reader.read_line(&mut ack_line);
        self.writer.set_read_timeout(Some(RUN_READ_TIMEOUT))?;
        match read_result {
            Ok(0) => Err(PluginError::HandshakeTimeout),
            Ok(_) => {
                let trim = ack_line.trim();
                if trim.is_empty() {
                    return Err(PluginError::HandshakeTimeout);
                }
                let env_msg: AuthAckEnvelope = serde_json::from_str(trim)?;
                if env_msg.auth_ack.ok {
                    Ok(())
                } else {
                    Err(PluginError::HandshakeRejected {
                        reason: env_msg.auth_ack.reason,
                    })
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Err(PluginError::HandshakeTimeout)
            }
            Err(e) => Err(PluginError::Io(e)),
        }
    }

    /// 호스트로부터 한 줄 수신. timeout이면 `Ok(None)`.
    pub fn try_recv(&mut self) -> Result<Option<HostMessage>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Err(PluginError::HostClosed),
            Ok(_) => {
                let trim = line.trim();
                if trim.is_empty() {
                    return Ok(None);
                }
                let req: PluginRequest = serde_json::from_str(trim)?;
                Ok(Some(HostMessage::Request(req)))
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(None)
            }
            Err(e) => Err(PluginError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Barrier};
    use std::thread;

    /// 가짜 호스트 동작 결정.
    enum FakeHostBehavior {
        AckOk,
        AckReject(&'static str),
        DropWithoutAck,
    }

    /// 임의 포트에 listen하고 첫 connection의 AuthMessage 한 줄을 읽은 뒤
    /// behavior에 따라 AuthAck를 보낸다. 그 후 잠시 대기.
    fn spawn_fake_host(behavior: FakeHostBehavior) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let cloned = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(cloned);
            let mut line = String::new();
            reader.read_line(&mut line).expect("fake host: read auth");
            match behavior {
                FakeHostBehavior::AckOk => {
                    writeln!(stream, "{{\"auth_ack\":{{\"ok\":true}}}}")
                        .expect("fake host: write ack");
                    stream.flush().expect("fake host: flush ack");
                    // 메시지 루프 빠져나가지 않도록 잠시 대기.
                    thread::sleep(Duration::from_millis(200));
                }
                FakeHostBehavior::AckReject(reason) => {
                    writeln!(
                        stream,
                        "{{\"auth_ack\":{{\"ok\":false,\"reason\":\"{reason}\"}}}}"
                    )
                    .expect("fake host: write reject");
                    stream.flush().expect("fake host: flush reject");
                    thread::sleep(Duration::from_millis(50));
                }
                FakeHostBehavior::DropWithoutAck => {
                    // 아무것도 보내지 않고 잠시 보관 (SDK가 read_timeout으로 끊을 때까지).
                    thread::sleep(Duration::from_millis(400));
                }
            }
        });
        (port, handle)
    }

    fn env_for(port: u16) -> PluginEnv {
        PluginEnv {
            plugin_id: "com.test.plugin".into(),
            host_port: port,
            token: "test-token".into(),
            host_api_version: "1".into(),
            plugin_dir: None,
            data_dir: None,
            config_path: None,
            log_path: None,
            locale: "en".into(),
            locale_font: None,
            handle_endpoint: None,
        }
    }

    #[test]
    fn authenticate_success_when_host_acks_ok() {
        let (port, handle) = spawn_fake_host(FakeHostBehavior::AckOk);
        let env = env_for(port);
        let mut conn = Connection::connect(&env).expect("connect");
        conn.authenticate(&env).expect("auth ok");
        drop(conn);
        handle.join().expect("fake host thread");
    }

    #[test]
    fn authenticate_returns_handshake_rejected_when_host_acks_false() {
        let (port, handle) = spawn_fake_host(FakeHostBehavior::AckReject("token mismatch"));
        let env = env_for(port);
        let mut conn = Connection::connect(&env).expect("connect");
        let err = conn.authenticate(&env).expect_err("should be rejected");
        match err {
            PluginError::HandshakeRejected { reason } => {
                assert_eq!(reason.as_deref(), Some("token mismatch"));
            }
            other => panic!("expected HandshakeRejected, got {other:?}"),
        }
        handle.join().expect("fake host thread");
    }

    #[test]
    fn authenticate_times_out_when_host_silent() {
        let (port, handle) = spawn_fake_host(FakeHostBehavior::DropWithoutAck);
        let env = env_for(port);
        let mut conn = Connection::connect(&env).expect("connect");
        // 빠른 타임아웃으로 검증 (실제 코드는 5s).
        let err = conn
            .authenticate_with_timeout(&env, Duration::from_millis(150))
            .expect_err("should time out");
        assert!(
            matches!(err, PluginError::HandshakeTimeout),
            "expected HandshakeTimeout, got {err:?}"
        );
        handle.join().expect("fake host thread");
    }

    /// 호스트 채널은 Nagle 이 꺼져 있어야 한다. 켜져 있으면 응답 줄의 개행 조각이
    /// 호스트의 지연 ACK 를 기다려 plugin 호출마다 약 40 ms 가 붙는다.
    #[test]
    fn connect_disables_nagle_on_the_host_channel() {
        let (port, handle) = spawn_fake_host(FakeHostBehavior::AckOk);
        let env = env_for(port);
        let conn = Connection::connect_and_authenticate(&env).expect("ok");
        let (writer, reader) = conn.into_parts();
        assert!(writer.nodelay().unwrap(), "writer must have TCP_NODELAY");
        assert!(
            reader.get_ref().nodelay().unwrap(),
            "reader shares the socket"
        );
        drop((writer, reader));
        handle.join().expect("fake host thread");
    }

    #[test]
    fn connect_and_authenticate_composes_both_steps() {
        let (port, handle) = spawn_fake_host(FakeHostBehavior::AckOk);
        let env = env_for(port);
        let conn = Connection::connect_and_authenticate(&env).expect("ok");
        drop(conn);
        handle.join().expect("fake host thread");
    }

    /// 핸드셰이크 동시성 — 두 plugin이 거의 동시에 connect해도 각각 정확한 ack를 받는다.
    /// 호스트 listener의 stream 매칭이 token으로 격리되므로 SDK 측에서도 race가 없어야 한다.
    #[test]
    fn handshake_is_robust_to_concurrent_connects() {
        let barrier = Arc::new(Barrier::new(2));
        let (port1, h1) = spawn_fake_host(FakeHostBehavior::AckOk);
        let (port2, h2) = spawn_fake_host(FakeHostBehavior::AckReject("nope"));

        let b1 = barrier.clone();
        let t1 = thread::spawn(move || {
            let env = env_for(port1);
            b1.wait();
            let mut c = Connection::connect(&env).unwrap();
            c.authenticate(&env)
        });
        let b2 = barrier.clone();
        let t2 = thread::spawn(move || {
            let env = env_for(port2);
            b2.wait();
            let mut c = Connection::connect(&env).unwrap();
            c.authenticate(&env)
        });
        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();
        assert!(r1.is_ok(), "concurrent ok-host should succeed: {r1:?}");
        assert!(
            matches!(r2, Err(PluginError::HandshakeRejected { .. })),
            "concurrent reject-host should be rejected: {r2:?}"
        );
        h1.join().expect("fake host1 thread");
        h2.join().expect("fake host2 thread");
    }

    /// 다른 stream으로 connect → host가 stream을 종료하면 timeout(0 bytes) 분기로 떨어진다.
    #[test]
    fn handshake_returns_timeout_when_host_closes_immediately() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            // 입력 다 안 읽고 즉시 종료.
            drop(stream);
        });
        let env = env_for(port);
        let mut conn = Connection::connect(&env).expect("connect");
        let err = conn
            .authenticate_with_timeout(&env, Duration::from_millis(500))
            .expect_err("should not succeed");
        assert!(
            matches!(err, PluginError::HandshakeTimeout | PluginError::Io(_)),
            "expected timeout or io error after host close, got {err:?}"
        );
        handle.join().expect("fake host thread");
    }

    /// 호스트가 깨진 envelope를 보내면 JSON 에러로 떨어진다 (Handshake* 가 아님).
    /// 이건 정상 호스트에선 없지만, 프로토콜 회귀 방지용.
    #[test]
    fn handshake_rejects_unparseable_ack() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let cloned = stream.try_clone().unwrap();
            let mut reader = BufReader::new(cloned);
            let mut line = String::new();
            reader.read_line(&mut line).expect("fake host: read auth");
            writeln!(stream, "not-json garbage").expect("fake host: write garbage");
            stream.flush().expect("fake host: flush garbage");
            thread::sleep(Duration::from_millis(50));
        });
        let env = env_for(port);
        let mut conn = Connection::connect(&env).expect("connect");
        let err = conn.authenticate(&env).expect_err("should fail to parse");
        assert!(
            matches!(err, PluginError::Json(_)),
            "expected Json error, got {err:?}"
        );
        handle.join().expect("fake host thread");
    }
}
