//! CLI와 GUI가 공유하는 JSON-RPC 연결과 attach/bulk 스트림 연결.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// JSON-RPC 오류의 코드·문구·구조화된 data. 문구는 서버 로케일에 따라 다를 수 있다.
/// Display는 기존 한 줄 형식을 유지하며 data는 출력하지 않는다. 추가 정보의 표시는 호출자가 정한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonRpcCallError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error ({}): {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcCallError {}

/// 필요한 기능을 서버가 선언하지 않아 원래 요청을 보내지 않았다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedCapability {
    pub name: String,
    pub required: u32,
    /// 서버가 선언한 버전. 기능 자체가 없으면 None이다.
    pub found: Option<u32>,
}

impl std::fmt::Display for UnsupportedCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.found {
            Some(v) => write!(
                f,
                "the tasty instance declares '{}' at version {v}, but version {} is required; \
                 nothing was sent",
                self.name, self.required
            ),
            None => write!(
                f,
                "the tasty instance does not declare '{}' (required version {}); nothing was sent",
                self.name, self.required
            ),
        }
    }
}

impl std::error::Error for UnsupportedCapability {}

/// capability 확인의 읽기 기한이 만료됐다. 원래 요청은 보내지 않았다.
/// 늦은 응답이 남을 수 있으므로 이 연결을 재사용하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityProbeExpired;

impl std::fmt::Display for CapabilityProbeExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the capability check did not finish within the response timeout; nothing was sent"
        )
    }
}

impl std::error::Error for CapabilityProbeExpired {}

/// 해당 메서드는 호스트 멱등 키 계약 밖이므로 요청을 보내지 않았다.
/// 호스트가 아는 Mutate 이름은 Kept, Read/Idempotent는 Unneeded다.
/// 표에 없는 플러그인 고유 이름에는 키 기반 재실행 방지를 보장하지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyOutsideContract {
    pub method: String,
}

impl std::fmt::Display for KeyOutsideContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' is outside the idempotency-key contract — the host does not replay it for a \
             key, so a retry runs it again; nothing was sent",
            self.method
        )
    }
}

impl std::error::Error for KeyOutsideContract {}

/// 메서드가 요구하는 멱등 키 기능 버전을 연결 없이 확인한다.
/// Kept는 since 버전, Unneeded는 최소 버전을 요구하며 Outside는 거절한다.
fn required_key_version(method: &str) -> Result<u32> {
    use crate::method_meta::KeyContract;
    match crate::method_meta::key_contract(method) {
        KeyContract::Kept { since } => Ok(since),
        KeyContract::Unneeded => Ok(IDEMPOTENCY_CAPABILITY_VERSION),
        KeyContract::Outside => Err(KeyOutsideContract {
            method: method.to_string(),
        }
        .into()),
    }
}

/// system.info에서 기능 이름과 버전을 읽는다. 목록이 없으면 빈 맵이다.
/// 형식이 다른 항목은 무시해 나머지 지원 기능을 읽을 수 있게 한다.
pub fn parse_capabilities(info: &serde_json::Value) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    let Some(items) = info.get("capabilities").and_then(|v| v.as_array()) else {
        return out;
    };
    for item in items {
        if let (Some(name), Some(version)) = (
            item.get("name").and_then(|v| v.as_str()),
            item.get("version").and_then(|v| v.as_u64()),
        ) {
            out.insert(name.to_string(), version as u32);
        }
    }
    out
}

pub mod stream;

pub use stream::StreamConnection;

/// A reusable IPC connection that keeps a single BufReader across multiple requests.
pub struct IpcConnection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    /// 한 번 물어 둔 서버의 선언. 연결 수명 동안 안 바뀐다 — 그 값을 정하는 것은
    /// 상대 프로세스의 빌드이고, 연결이 살아 있는 한 그 프로세스도 그대로다.
    capabilities: Option<BTreeMap<String, u32>>,
}

impl IpcConnection {
    /// 무기한 대기가 필요한 메서드도 있어 기본 소켓 읽기 timeout은 두지 않는다.
    /// 요청별 상한은 호출자가 response_timeout_ms에 적는다. 서버는 시작 전 만료를
    /// -32067, 시작 후 결과 불명을 -32061로 답한다. 이 생성자는 그 필드를 채우지 않는다.
    /// EOF는 오류로 처리하지만 응답 없는 열린 연결의 무기한 대기를 감지하지는 않는다.
    pub fn new(stream: TcpStream) -> Result<Self> {
        // 작은 요청의 분할 쓰기가 delayed ACK를 기다리지 않도록 Nagle을 끈다.
        // 설정 실패는 경고만 남기고 연결을 유지한다.
        if let Err(e) = stream.set_nodelay(true) {
            tracing::warn!("IPC 연결: TCP_NODELAY 설정 실패(지연 증가 가능): {e}");
        }
        let writer = stream.try_clone()?;
        let reader = BufReader::new(stream);
        Ok(Self {
            writer,
            reader,
            capabilities: None,
        })
    }

    /// 응답을 읽는 소켓 핸들에 걸린 읽기 기한. 요청별 상한이 끝난 뒤 기한이 남지 않았는지 확인할 때 쓴다.
    /// Windows 에서는 소켓 사본이 다른 핸들에 건 기한을 돌려주지 않으므로 이 핸들에서 직접 읽는다.
    pub fn read_timeout(&self) -> std::io::Result<Option<std::time::Duration>> {
        self.reader.get_ref().read_timeout()
    }

    /// Send a JSON-RPC request and read the response.
    pub fn send(&mut self, request: &JsonRpcRequest) -> Result<serde_json::Value> {
        self.write_request(request)?;

        loop {
            let mut line = String::new();
            // EOF를 빈 줄로 무시하면 read_line이 계속 0을 반환하며 반복한다.
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                anyhow::bail!(
                    "tasty instance closed the connection without responding \
                     (host may be shutting down)"
                );
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            return Self::parse_response(trimmed);
        }
    }

    fn write_request(&mut self, request: &JsonRpcRequest) -> Result<()> {
        let json = serde_json::to_string(request)?;
        // 본문과 개행을 한 버퍼로 묶어 write_all을 한 번 호출한다.
        let mut line = json.into_bytes();
        line.push(b'\n');
        self.writer.write_all(&line)?;
        self.writer.flush()?;
        Ok(())
    }

    fn parse_response(line: &str) -> Result<serde_json::Value> {
        let response: JsonRpcResponse = serde_json::from_str(line)?;
        if let Some(error) = response.error {
            return Err(JsonRpcCallError {
                code: error.code,
                message: error.message,
                data: error.data,
            }
            .into());
        }
        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    /// 응답을 `bound`까지만 읽는다. 후속 조회처럼 호출자가 자기 대기 상한을 지켜야 할 때 쓴다.
    /// 상한은 요청을 쓴 뒤부터 응답 한 줄을 다 읽을 때까지의 전체 시간이다. 응답이 조각으로 이어
    /// 와도 읽을 때마다 남은 시간을 다시 계산한다. 연결과 요청 쓰기는 상한에 들지 않는다.
    /// 요청의 `response_timeout_ms`(서버가 지키는 최초 명령의 응답 시한)는 바꾸지 않는다.
    /// 상한 안에 응답 줄을 다 읽지 못하면 `Ok(None)`이다. 그 응답이 나중에 도착할 수 있으므로
    /// 이 연결을 다시 쓰지 않는다. 그 밖의 결과는 [`Self::send`]와 같고 읽기 timeout을 없앤다.
    pub fn send_reading_within(
        &mut self,
        request: &JsonRpcRequest,
        bound: Duration,
    ) -> Result<Option<serde_json::Value>> {
        self.write_request(request)?;
        let deadline = Instant::now() + bound;
        let mut line = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.reader.get_ref().set_read_timeout(Some(left))?;
            let available = match self.reader.fill_buf() {
                Ok(available) => available,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            };
            if available.is_empty() {
                anyhow::bail!(
                    "tasty instance closed the connection without responding \
                     (host may be shutting down)"
                );
            }
            let Some(end) = available.iter().position(|&b| b == b'\n') else {
                let taken = available.len();
                line.extend_from_slice(available);
                self.reader.consume(taken);
                continue;
            };
            line.extend_from_slice(&available[..end]);
            self.reader.consume(end + 1);
            let text = String::from_utf8(std::mem::take(&mut line))?;
            if text.trim().is_empty() {
                continue;
            }
            self.reader.get_ref().set_read_timeout(None)?;
            return Self::parse_response(text.trim()).map(Some);
        }
    }

    /// 연결당 한 번 기능을 조회한다. 실제 요청과 같은 session_token으로 호출자를 유지한다.
    pub fn capabilities(&mut self, session_token: Option<&str>) -> Result<&BTreeMap<String, u32>> {
        self.capabilities_within(session_token, None)
    }

    /// 아직 기능을 조회하지 않았다면 확인 요청의 response_timeout_ms와 소켓 읽기 timeout을 설정한다.
    /// 밀리초는 올림하며 bound=0이면 보내지 않는다. 쓰기와 여러 읽기를 합친 전체 소요 시간의 상한은 아니다.
    /// 만료는 CapabilityProbeExpired이고 연결을 재사용하지 않는다. 만료 이외의 결과에서는 읽기 timeout을 None으로 돌린다.
    pub fn capabilities_within(
        &mut self,
        session_token: Option<&str>,
        bound: Option<Duration>,
    ) -> Result<&BTreeMap<String, u32>> {
        if self.capabilities.is_none() {
            let mut probe = JsonRpcRequest {
                caller_agent_id: None,
                jsonrpc: "2.0".to_string(),
                method: "system.info".to_string(),
                params: serde_json::Value::Null,
                id: Some(serde_json::Value::from(0)),
                session_token: session_token.map(str::to_string),
                response_timeout_ms: None,
                idempotency_key: None,
            };
            let info = match bound {
                None => self.send(&probe)?,
                Some(bound) => {
                    let Some(ms) = ceil_millis(bound) else {
                        return Err(CapabilityProbeExpired.into());
                    };
                    probe.response_timeout_ms = Some(ms);
                    self.send_within(&probe, bound)?
                }
            };
            self.capabilities = Some(parse_capabilities(&info));
        }
        Ok(self.capabilities.as_ref().expect("직전 분기가 채웠다"))
    }

    /// 각 소켓 읽기에 timeout을 설정한다. 쓰기나 부분 입력·빈 줄에 따른 반복 읽기 전체를 제한하지는 않는다.
    /// 읽기 timeout과 서버의 -32061/-32067 응답은 CapabilityProbeExpired로 변환한다.
    fn send_within(
        &mut self,
        request: &JsonRpcRequest,
        bound: Duration,
    ) -> Result<serde_json::Value> {
        self.reader.get_ref().set_read_timeout(Some(bound))?;
        let sent = self.send(request);
        let expired = match &sent {
            Ok(_) => false,
            Err(e) => {
                e.downcast_ref::<std::io::Error>().is_some_and(|io| {
                    matches!(
                        io.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    )
                }) || e.downcast_ref::<JsonRpcCallError>().is_some_and(|rpc| {
                    rpc.code == crate::protocol::ERR_EXPIRED_BEFORE_RUN
                        || rpc.code == crate::protocol::ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN
                })
            }
        };
        if expired {
            return Err(CapabilityProbeExpired.into());
        }
        // 성공과 만료 이외의 오류에서는 다음 요청의 읽기를 무기한으로 돌린다.
        self.reader.get_ref().set_read_timeout(None)?;
        sent
    }

    /// require_capability와 같되 capabilities_within의 요청·읽기 timeout 설정을 사용한다.
    pub fn require_capability_within(
        &mut self,
        name: &str,
        min_version: u32,
        session_token: Option<&str>,
        bound: Option<Duration>,
    ) -> Result<()> {
        let found = self
            .capabilities_within(session_token, bound)?
            .get(name)
            .copied();
        match found {
            Some(v) if v >= min_version => Ok(()),
            other => Err(UnsupportedCapability {
                name: name.to_string(),
                required: min_version,
                found: other,
            }
            .into()),
        }
    }

    /// 서버가 요구한 최소 기능 버전 이상을 지원하는지 전송 전에 확인한다.
    pub fn require_capability(
        &mut self,
        name: &str,
        min_version: u32,
        session_token: Option<&str>,
    ) -> Result<()> {
        self.require_capability_within(name, min_version, session_token, None)
    }

    /// 메서드의 키 계약과 서버의 지원 버전을 확인한 뒤 멱등 키를 붙여 보낸다.
    /// Outside 또는 지원 버전 부족이면 원래 요청은 보내지 않는다.
    /// Kept 요청의 재사용 응답에는 idempotent_replay가 붙고, Unneeded에는 붙지 않는다.
    /// 보존 시간·개수 등 system.info의 idempotency 범위 밖에서는 같은 키도 다시 실행될 수 있다.
    pub fn send_idempotent(
        &mut self,
        request: &JsonRpcRequest,
        key: &str,
    ) -> Result<serde_json::Value> {
        let version = required_key_version(&request.method)?;
        self.require_capability(
            IDEMPOTENCY_CAPABILITY,
            version,
            request.session_token.as_deref(),
        )?;
        let keyed = JsonRpcRequest {
            idempotency_key: Some(key.to_string()),
            ..request.clone()
        };
        self.send(&keyed)
    }
}

/// 확인 요청의 밀리초는 올림한다. 남은 시간이 0이면 None이다. 소켓 읽기 기한도 함께 적용한다.
fn ceil_millis(left: Duration) -> Option<u64> {
    if left.is_zero() {
        return None;
    }
    let ms = left.as_nanos().div_ceil(1_000_000);
    Some(u64::try_from(ms).unwrap_or(u64::MAX))
}

/// 본 요청의 밀리초는 내림한다. 1ms 미만이면 None이며, 무한 대기를 뜻하는 0을 보내지 않는다.
pub fn whole_millis(left: Duration) -> Option<u64> {
    u64::try_from(left.as_millis()).ok().filter(|&ms| ms > 0)
}

/// [`crate::protocol::JsonRpcRequest::idempotency_key`] 를 서버가 읽는다는 선언의 이름.
pub const IDEMPOTENCY_CAPABILITY: &str = "ipc.idempotency-key";
/// 서버가 멱등 키를 읽는 최소 버전. 메서드의 Kept.since에 따라 더 높은 버전이 필요할 수 있다.
pub const IDEMPOTENCY_CAPABILITY_VERSION: u32 = crate::method_meta::KEY_KEPT_BY_ROUTER;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_server_without_the_key_declares_nothing() {
        let info = serde_json::json!({ "version": "0.6.0", "scope": "engine" });
        assert!(parse_capabilities(&info).is_empty());
    }

    #[test]
    fn a_key_is_refused_before_sending_for_a_name_the_host_table_does_not_own() {
        let refused = required_key_version("markdown.recent").unwrap_err();
        let typed = refused
            .downcast_ref::<KeyOutsideContract>()
            .expect("타입으로 갈라져야 호출자가 '안 나갔다' 를 안다");
        assert_eq!(typed.method, "markdown.recent");
        assert!(refused.to_string().contains("nothing was sent"));

        assert_eq!(
            required_key_version("workspace.create").expect("구조 journal 메서드"),
            crate::method_meta::KEY_KEPT_IN_STRUCTURE_JOURNAL
        );
        assert_eq!(
            required_key_version("window.create").expect("App 층 메서드"),
            crate::method_meta::KEY_KEPT_BY_APP_LAYER
        );
        assert_eq!(
            required_key_version("image.open").expect("forward 로 나가는 표 이름"),
            crate::method_meta::KEY_KEPT_ON_EVERY_HOST_PATH
        );
        assert_eq!(
            required_key_version("workspace.list").expect("읽기"),
            IDEMPOTENCY_CAPABILITY_VERSION
        );
    }

    #[test]
    fn a_bounded_read_gives_up_on_a_silent_server_and_keeps_a_prompt_answer() {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut conn = IpcConnection::new(TcpStream::connect(addr).unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(server.try_clone().unwrap());
        let mut writer = server;
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "run.result".to_string(),
            params: serde_json::json!({ "run": 1 }),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };

        let answer = std::thread::spawn(move || {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                writer,
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"done":true}}}}"#
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            (reader, writer)
        });
        let first = conn
            .send_reading_within(&req, std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(first, Some(serde_json::json!({ "done": true })));

        let started = std::time::Instant::now();
        let second = conn
            .send_reading_within(&req, std::time::Duration::from_millis(200))
            .unwrap();
        assert_eq!(second, None);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        drop(answer.join().unwrap());
    }

    /// 한 줄 응답이 70ms마다 1바이트씩 오면 읽기마다 상한 안에 무언가 오므로, 상한은 읽기마다가
    /// 아니라 전체에 걸려야 한다.
    #[test]
    fn a_bounded_read_stops_a_trickling_answer_at_the_total_bound() {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut conn = IpcConnection::new(TcpStream::connect(addr).unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let trickle = std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(server.try_clone().unwrap());
            let mut writer = server;
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let started = std::time::Instant::now();
            let answer = br#"{"jsonrpc":"2.0","id":1,"result":{"done":true}}"#;
            for byte in answer {
                if started.elapsed() > std::time::Duration::from_millis(400)
                    || writer.write_all(&[*byte]).is_err()
                {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(70));
            }
        });
        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "run.result".to_string(),
            params: serde_json::json!({ "run": 1 }),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let started = std::time::Instant::now();
        let answer = conn
            .send_reading_within(&req, std::time::Duration::from_millis(100))
            .unwrap();
        let waited = started.elapsed();
        assert_eq!(answer, None);
        assert!(
            waited < std::time::Duration::from_millis(300),
            "waited {waited:?}"
        );
        drop(conn);
        trickle.join().unwrap();
    }

    #[test]
    fn an_app_layer_key_needs_the_version_that_keeps_it() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut conn = IpcConnection::new(stream).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        conn.capabilities = Some(
            [(IDEMPOTENCY_CAPABILITY.to_string(), 1u32)]
                .into_iter()
                .collect(),
        );

        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "window.create".to_string(),
            params: serde_json::json!({}),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let err = conn.send_idempotent(&req, "k-1").unwrap_err();
        let typed = err
            .downcast_ref::<UnsupportedCapability>()
            .expect("판이 모자라면 capability 거절이다");
        assert_eq!(typed.required, crate::method_meta::KEY_KEPT_BY_APP_LAYER);
        assert_eq!(typed.found, Some(1));

        drop(conn);
        let mut got = Vec::new();
        server.read_to_end(&mut got).unwrap();
        assert!(
            got.is_empty(),
            "거절됐는데 무언가 나갔다: {}",
            String::from_utf8_lossy(&got)
        );
    }

    #[test]
    fn a_refused_key_never_touches_the_connection() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(addr).unwrap();
        // 거절이 사라져 요청이 나가면 서버가 답하지 않으므로 client 가 영영 선다 — 시험이
        // 멈추지 않고 실패하도록 읽기에 시한을 둔다.
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut conn = IpcConnection::new(stream).unwrap();
        let (mut server, _) = listener.accept().unwrap();

        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "markdown.recent".to_string(),
            params: serde_json::json!({}),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let err = conn.send_idempotent(&req, "k-1").unwrap_err();
        assert!(err.downcast_ref::<KeyOutsideContract>().is_some(), "{err}");

        drop(conn);
        let mut got = Vec::new();
        server.read_to_end(&mut got).unwrap();
        assert!(
            got.is_empty(),
            "거절됐는데 무언가 나갔다: {}",
            String::from_utf8_lossy(&got)
        );
    }

    #[test]
    fn an_error_answer_keeps_its_data() {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut conn = IpcConnection::new(stream).unwrap();
        let (server, _) = listener.accept().unwrap();
        let answer = std::thread::spawn(move || {
            let mut reader = BufReader::new(server.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut server = server;
            writeln!(
                server,
                "{}",
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": {
                        "code": -32603,
                        "message": "memory db error: disk I/O error",
                        "data": { "storage_failure": "io" }
                    }
                })
            )
            .unwrap();
        });

        let req = JsonRpcRequest {
            caller_agent_id: None,
            jsonrpc: "2.0".to_string(),
            method: "memory.put".to_string(),
            params: serde_json::json!({}),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let err = conn.send(&req).unwrap_err();
        answer.join().unwrap();
        let rpc = err
            .downcast_ref::<JsonRpcCallError>()
            .expect("호스트가 답한 오류는 타입으로 온다");
        assert_eq!(rpc.code, -32603);
        assert_eq!(
            rpc.data,
            Some(serde_json::json!({ "storage_failure": "io" }))
        );
        assert_eq!(
            err.to_string(),
            "Error (-32603): memory db error: disk I/O error"
        );
    }

    #[test]
    fn what_is_left_rounds_down_and_nothing_left_is_none() {
        use std::time::Duration;
        assert_eq!(whole_millis(Duration::from_micros(299_900)), Some(299));
        assert_eq!(whole_millis(Duration::from_micros(999)), None);
        assert_eq!(whole_millis(Duration::ZERO), None);
    }

    #[test]
    fn the_check_bound_rounds_up_and_only_nothing_left_is_none() {
        use std::time::Duration;
        assert_eq!(ceil_millis(Duration::from_nanos(1)), Some(1));
        assert_eq!(ceil_millis(Duration::from_micros(999)), Some(1));
        assert_eq!(ceil_millis(Duration::from_micros(1_001)), Some(2));
        assert_eq!(ceil_millis(Duration::from_millis(300)), Some(300));
        assert_eq!(ceil_millis(Duration::ZERO), None);
    }

    #[test]
    fn a_malformed_entry_does_not_take_the_others_down() {
        let info = serde_json::json!({
            "capabilities": [
                { "name": "ipc.capabilities", "version": 1 },
                { "name": "no.version" },
                { "version": 2 },
                "not an object",
                { "name": "ipc.idempotency-key", "version": 3 },
            ]
        });
        let caps = parse_capabilities(&info);
        assert_eq!(caps.get("ipc.capabilities"), Some(&1));
        assert_eq!(caps.get("ipc.idempotency-key"), Some(&3));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn a_higher_version_satisfies_and_the_two_failures_are_told_apart() {
        let caps: BTreeMap<String, u32> = [("ipc.idempotency-key".to_string(), 2u32)]
            .into_iter()
            .collect();
        assert!(
            caps.get("ipc.idempotency-key")
                .copied()
                .is_some_and(|v| v >= 1)
        );

        let missing = UnsupportedCapability {
            name: "x".into(),
            required: 1,
            found: None,
        };
        let older = UnsupportedCapability {
            name: "x".into(),
            required: 2,
            found: Some(1),
        };
        assert_ne!(missing, older);
        assert!(missing.to_string().contains("does not declare"));
        assert!(older.to_string().contains("version 1"));
        assert!(missing.to_string().contains("nothing was sent"));
        assert!(older.to_string().contains("nothing was sent"));
    }
}
