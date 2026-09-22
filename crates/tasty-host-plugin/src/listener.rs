//! 호스트 측 plugin TCP listener.
//!
//! 호스트가 부팅 시 `127.0.0.1:0` 으로 한 번만 bind, plugin이 spawn 후
//! 이 포트로 connect. 첫 줄로 `AuthMessage` (token 포함) 보내야 인증.
//!
//! plugin마다 token이 다르므로 listener는 token → 채널 맵을 들고 있다가
//! 매칭되는 spawn 측에 stream을 전달.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use crate::protocol::{AuthAck, AuthAckEnvelope, AuthMessage};

const AUTH_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct HostListener {
    addr: SocketAddr,
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<TcpStream>>>>,
    _accept_thread: std::thread::JoinHandle<()>,
}

/// 대기 중인 handshake 채널 맵의 poison 을 보고했는가(첫 1 회만).
///
/// 임계구역은 `HashMap` insert/remove 뿐이라 패닉이 나도 불변식이 성립한다 — 복구가
/// 맞다. 조용히 버리면 등록이 안 된 채 `recv_timeout` 만 흘러 **"plugin 이 왜 안 뜨는지"
/// 가 timeout 으로만 보이고**, 수락 쪽에서 버리면 이미 연결한 plugin 이 거절된다.
static PENDING_POISONED: AtomicBool = AtomicBool::new(false);
const PENDING_WHAT: &str = "plugin handshake pending map";

impl HostListener {
    pub fn bind() -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let pending: Arc<Mutex<HashMap<String, mpsc::Sender<TcpStream>>>> = Arc::default();
        let pending_clone = pending.clone();
        let accept_thread = std::thread::Builder::new()
            .name("plugin-listener".to_string())
            .spawn(move || {
                for incoming in listener.incoming() {
                    match incoming {
                        Ok(stream) => handle_incoming(stream, &pending_clone),
                        Err(e) => {
                            tracing::warn!("plugin listener accept error: {e}");
                        }
                    }
                }
            })?;
        Ok(Self {
            addr,
            pending,
            _accept_thread: accept_thread,
        })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// plugin spawn 직전에 호출. 해당 토큰의 connection을 받기 위한 채널을 등록.
    /// `timeout` 내 connection이 안 오면 `None`.
    pub fn expect_connection(&self, token: &str, timeout: Duration) -> Option<TcpStream> {
        let (tx, rx) = mpsc::channel();
        tasty_utils::poison::recover_mutex(self.pending.lock(), PENDING_WHAT, &PENDING_POISONED)
            .insert(token.to_string(), tx);
        match rx.recv_timeout(timeout) {
            Ok(stream) => Some(stream),
            Err(_) => {
                tasty_utils::poison::recover_mutex(
                    self.pending.lock(),
                    PENDING_WHAT,
                    &PENDING_POISONED,
                )
                .remove(token);
                None
            }
        }
    }
}

fn handle_incoming(
    stream: TcpStream,
    pending: &Arc<Mutex<HashMap<String, mpsc::Sender<TcpStream>>>>,
) {
    // 요청/응답마다 작은 쓰기가 오가는 채널이라 Nagle 을 끈다 — 켜 두면 한 메시지의
    // 뒤 조각이 앞 조각의 ACK 를 기다리고, plugin 쪽은 ACK 를 수십 ms
    // 미뤄서 hop 마다 그만큼이 붙는다. plugin SDK 도 자기 끝에서 같은 설정을 한다
    // (`docs/dev-guide/plugin-development.md` "전송 지연"). 실패해도 채널은 동작하므로
    // 기록만 한다.
    if let Err(e) = stream.set_nodelay(true) {
        tracing::warn!("plugin listener: TCP_NODELAY failed: {e}");
    }
    let Some(auth) = read_auth_tcp(&stream) else {
        return;
    };
    let tx_opt =
        tasty_utils::poison::recover_mutex(pending.lock(), PENDING_WHAT, &PENDING_POISONED)
            .remove(&auth.token);
    match tx_opt {
        Some(tx) => accept_handshake_tcp(stream, auth, tx),
        None => reject_handshake_tcp(&stream, &auth),
    }
}

/// 인증 라인을 읽어 파싱 — 실패하면 warn 후 `None`(caller 는 그대로 drop).
/// 성공하면 read_timeout 을 해제한 상태로 반환(핸드셰이크 이후 정상 read 재개 대비).
fn read_auth_tcp(stream: &TcpStream) -> Option<AuthMessage> {
    if let Err(e) = stream.set_read_timeout(Some(AUTH_READ_TIMEOUT)) {
        tracing::warn!("plugin listener: set_read_timeout failed: {e}");
    }
    let auth = read_auth_message_tcp(stream)?;
    if let Err(e) = stream.set_read_timeout(None) {
        tracing::warn!("plugin listener: clearing read_timeout failed: {e}");
    }
    Some(auth)
}

/// stream 을 clone 해 한 줄 읽고 `AuthMessage` 로 파싱. 실패 사유는 내부에서 warn.
fn read_auth_message_tcp(stream: &TcpStream) -> Option<AuthMessage> {
    let line = read_auth_line_tcp(stream)?;
    match serde_json::from_str(line.trim()) {
        Ok(a) => Some(a),
        Err(e) => {
            tracing::warn!("plugin listener: invalid auth message: {e}");
            None
        }
    }
}

/// stream 을 clone 해 인증 라인 한 줄을 읽는다.
fn read_auth_line_tcp(stream: &TcpStream) -> Option<String> {
    let cloned = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("plugin listener: stream clone failed: {e}");
            return None;
        }
    };
    let mut reader = BufReader::new(cloned);
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line) {
        tracing::warn!("plugin listener: auth read failed: {e}");
        return None;
    }
    Some(line)
}

/// 토큰 매칭 성공 — auth_ack(true) 송신 후 `TcpStream` 을 대기 중인 spawn 측에 handoff.
fn accept_handshake_tcp(stream: TcpStream, auth: AuthMessage, tx: mpsc::Sender<TcpStream>) {
    if !send_auth_ack_ok(&stream, &auth) {
        return;
    }
    tracing::info!("plugin '{}' authenticated", auth.plugin_id);
    if let Err(e) = tx.send(stream) {
        tracing::warn!("plugin '{}' stream handoff failed: {e}", auth.plugin_id);
    }
}

/// 성공 auth_ack 송신. 실패하면 warn 후 false(caller 는 그대로 drop).
fn send_auth_ack_ok(stream: &TcpStream, auth: &AuthMessage) -> bool {
    if let Err(e) = send_auth_ack(stream, true, None) {
        tracing::warn!(
            "plugin '{}' auth_ack send failed: {e} — dropping",
            auth.plugin_id
        );
        return false;
    }
    true
}

/// 토큰 매칭 실패(unknown/expired) — 거부 ack 송신 후 drop.
/// SDK가 즉시 HandshakeRejected로 실패하도록 명시적 거부 ack 를 보낸다.
fn reject_handshake_tcp(stream: &TcpStream, auth: &AuthMessage) {
    tracing::warn!(
        "plugin auth with unknown/expired token (plugin_id={})",
        auth.plugin_id
    );
    if let Err(e) = send_auth_ack(stream, false, Some("token mismatch")) {
        tracing::debug!("plugin auth_ack(false) send failed: {e}");
    }
}

fn send_auth_ack(stream: &TcpStream, ok: bool, reason: Option<&str>) -> std::io::Result<()> {
    let env = AuthAckEnvelope {
        auth_ack: AuthAck {
            ok,
            reason: reason.map(|s| s.to_string()),
        },
    };
    let line = serde_json::to_string(&env)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut w = stream;
    tasty_plugin_protocol::write_line(&mut w, &line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;

    #[test]
    fn listener_binds_on_random_port() {
        let l = HostListener::bind().unwrap();
        assert_ne!(l.port(), 0);
    }

    #[test]
    fn auth_flow_matches_token() {
        let listener = HostListener::bind().unwrap();
        let port = listener.port();
        let token = "test-token-123".to_string();

        // expect_connection in this thread (with timeout)
        std::thread::scope(|s| {
            let token_clone = token.clone();
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                let auth = AuthMessage {
                    plugin_id: "com.test.plugin".into(),
                    token: token_clone,
                };
                let line = serde_json::to_string(&auth).unwrap() + "\n";
                stream.write_all(line.as_bytes()).unwrap();
                stream.flush().unwrap();
                // keep-alive to avoid premature close
                std::thread::sleep(Duration::from_millis(200));
            });

            let stream = listener.expect_connection(&token, Duration::from_secs(2));
            assert!(stream.is_some(), "expected connection to be received");
        });
    }

    /// 넘겨받은 plugin 채널은 Nagle 이 꺼져 있어야 한다. 켜져 있으면 요청 줄의
    /// 개행 조각이 plugin 의 지연 ACK 를 기다려 hop 마다 약 40 ms 가 붙는다.
    #[test]
    fn handed_off_stream_has_nodelay() {
        let listener = HostListener::bind().unwrap();
        let port = listener.port();
        let token = "nodelay-token".to_string();
        std::thread::scope(|s| {
            let token_clone = token.clone();
            s.spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                let auth = AuthMessage {
                    plugin_id: "com.test.plugin".into(),
                    token: token_clone,
                };
                let line = serde_json::to_string(&auth).unwrap() + "\n";
                stream.write_all(line.as_bytes()).unwrap();
                stream.flush().unwrap();
                std::thread::sleep(Duration::from_millis(200));
            });
            let stream = listener
                .expect_connection(&token, Duration::from_secs(2))
                .expect("connection handed off");
            assert!(
                stream.nodelay().unwrap(),
                "plugin channel must disable Nagle"
            );
        });
    }

    /// 대기 맵이 poison 돼도 handshake 가 성사된다.
    ///
    /// 조용히 버리는 구현이면 `expect_connection` 의 등록이 사라져 수락 쪽이 채널을
    /// 못 찾고, 호출자에게는 원인 없는 timeout 으로만 보인다.
    #[test]
    fn poisoned_pending_map_still_completes_the_handshake() {
        let listener = HostListener::bind().expect("bind");
        let port = listener.port();
        let token = "poisoned-token".to_string();

        let held = Arc::clone(&listener.pending);
        // join 결과를 버리지 않고 Err 를 단언한다 — 이 스레드가 언젠가 패닉을 멈추면
        // 아무것도 poison 되지 않은 채 아래 단언이 전부 공허하게 통과한다.
        std::thread::spawn(move || {
            let _guard = held.lock().expect("fresh lock");
            panic!("poison the pending map on purpose");
        })
        .join()
        .expect_err("패닉한 스레드는 Err 로 join 된다");
        assert!(listener.pending.is_poisoned(), "전제: 락이 poison 이다");

        std::thread::scope(|s| {
            let token_clone = token.clone();
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
                let auth = AuthMessage {
                    plugin_id: "com.test.plugin".into(),
                    token: token_clone,
                };
                let line = serde_json::to_string(&auth).expect("serialize") + "\n";
                stream.write_all(line.as_bytes()).expect("write");
                stream.flush().expect("flush");
                std::thread::sleep(Duration::from_millis(200));
            });

            let stream = listener.expect_connection(&token, Duration::from_secs(2));
            assert!(stream.is_some(), "poison 이후에도 연결이 전달돼야 한다");
        });
    }

    #[test]
    fn auth_flow_rejects_unknown_token() {
        let listener = HostListener::bind().unwrap();
        let port = listener.port();

        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                let auth = AuthMessage {
                    plugin_id: "com.test.plugin".into(),
                    token: "unknown-token".into(),
                };
                let line = serde_json::to_string(&auth).unwrap() + "\n";
                stream.write_all(line.as_bytes()).expect("test auth write");
                stream.flush().expect("test auth flush");
            });

            let stream = listener.expect_connection("expected-token", Duration::from_millis(800));
            assert!(stream.is_none(), "expected no connection (token mismatch)");
        });
    }
}
