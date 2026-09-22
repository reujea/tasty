//! 클라이언트 측 IPC 연결 — 실행 중인 tasty 인스턴스에 붙는 쪽.
//!
//! 서버 측([`crate::server`] / [`crate::session`])과 wire 프레이밍
//! ([`crate::protocol`] / [`crate::stream`])이 이 크레이트에 있으므로,
//! 그 짝인 클라이언트 연결도 같은 곳에 둔다. CLI 와 본체 GUI 가 함께 쓴다.
//!
//! - [`IpcConnection`] — BufReader 를 유지하는 JSON-RPC request-response 연결
//! - [`StreamConnection`] — attach/bulk 스트림 연결

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::Result;

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// 호스트가 JSON-RPC 오류로 답했다 — **코드를 데이터로 들고 있는** 실패.
///
/// 이전에는 `anyhow::bail!("Error ({code}): {message}")` 로 곧장 문자열이 됐고, 그래서
/// 호출자가 코드를 쓰려면 자기가 만든 문장을 자기가 다시 파싱해야 했다. `message` 는
/// 답한 쪽(호스트 또는 plugin)이 만든 산문이라 **로케일을 탈 수 있는** 반면 `code` 는
/// 프로토콜 값이라 안 탄다 — 그 둘을 갈라 두는 것이 이 타입의 전부다.
///
/// `Display` 는 종전 문자열 그대로다. 기존 호출자의 출력은 한 글자도 바뀌지 않는다.
///
/// `data` 는 응답의 `error.data` 를 **원형 그대로** 든다(`reason` · `storage_failure` 처럼
/// 호출자가 분기할 구조화 필드). `Display` 에는 싣지 않는다 — 그 문자열을 첫 줄로 파싱하는
/// 쪽이 있고, `data` 를 보여 줄지는 출력하는 쪽이 정한다(CLI 는 둘째 줄, ADR-0512).
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

/// 상대가 그 계약을 선언하지 않았다 — **요청은 아직 안 나갔다.**
///
/// 이 타입이 [`JsonRpcCallError`] 와 갈라져 있는 이유가 그 한 줄이다. 저쪽은 호스트가
/// 답한 실패라 "무엇이 일어났는지" 가 이미 정해졌고, 이쪽은 **아무것도 일어나지
/// 않았음**을 뜻한다. 부수효과가 남는 메서드에서는 그 차이가 재시도 판단을 통째로
/// 가른다 — 이 오류를 받은 호출자는 그대로 다시 걸 수 있다(다만 같은 서버라면 또 같은
/// 답이다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedCapability {
    pub name: String,
    pub required: u32,
    /// 서버가 그 이름을 **다른 판으로** 선언했으면 그 값. 아예 없으면 `None`.
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

/// 계약 확인 요청(`system.info`)이 호출자가 준 시간 안에 안 끝났다 — **그 뒤에 보낼 요청은
/// 아직 안 나갔다.**
///
/// 확인 요청은 부수효과가 없으므로 클라이언트가 먼저 물러나도 안전하다. 그래서 이 판정은 서버를
/// 믿지 않고 소켓 읽기 기한으로도 건다 — 봉투 상한을 모르는 구 서버에서도 같은 시간에 끝난다.
/// 이 오류 뒤의 연결은 다시 쓰지 않는다(늦은 답이 소켓에 남을 수 있다).
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

/// 그 메서드는 멱등 키 계약 **밖**이다 — **요청은 아직 안 나갔다.**
///
/// [`UnsupportedCapability`] 와 같은 성질의 거절이다. 저쪽은 "서버가 키를 못 읽는다",
/// 이쪽은 "서버가 키를 읽어도 **이 메서드에서는** 안 지킨다" 다. 어느 쪽이든 키를 실어
/// 보내면 호출자는 계약이 걸린 줄 알고 재시도하고, 그 재시도는 두 번째 실행이 된다.
/// 판정은 [`crate::method_meta::key_contract`] 가 한다 — plugin namespace forward(ADR-0361)와
/// GUI debug step 이 끝내는 `Mutate`(ADR-0423)가 이 값을 낸다.
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

/// 키를 실어 보내기 **전에** 그 메서드의 선언을 읽어, 상대에게 요구할
/// `ipc.idempotency-key` 판을 정한다. 계약 밖이면 거절이다. 연결이 필요 없다.
///
/// - `Kept { since }` — 보존소가 그 판부터 이 메서드를 받는다. App 층 메서드는 판 2 부터라,
///   판 1 서버에 보내면 키가 조용히 무시된다.
/// - `Unneeded` — 재전달이 원래 안전하다. 서버가 필드를 읽기만 하면 된다(최소 판).
/// - `Outside` — [`KeyOutsideContract`].
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

/// `system.info` 응답에서 capability 이름 → 판 을 뽑는다.
///
/// 키가 아예 없는 구 서버는 **빈 map** 이다. 그것이 "아무것도 선언하지 않았다" 의 옳은
/// 표현이고, 그래서 이 함수의 실패 갈래는 없다 — 모양이 어긋난 항목은 조용히 빠진다.
/// 어긋난 항목을 오류로 올리면 서버가 나중에 더한 모양 하나가 **기존에 되던 조회까지**
/// 깨뜨린다(이 목록의 요점이 추가는 안전하다는 것이었다).
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
    /// 읽기 타임아웃은 걸지 않는다 — 프로토콜에 **의도적으로 무한 대기하는 호출**이
    /// 있기 때문이다(`tasty agent task-await --timeout-ms 0`, 사용자 응답을 기다리는
    /// approval 등). 한 값으로 자르면 그 호출들이 정상 동작 중에 끊긴다. 응답 없는
    /// 상대를 감지하는 몫은 [`IpcConnection::send`] 의 EOF 판정이 진다.
    ///
    /// 기다림에 상한을 두려는 호출자는 **소켓이 아니라 요청 봉투에 싣는다**
    /// ([`crate::protocol::JsonRpcRequest::response_timeout_ms`]). 서버가 그 시간에 응답
    /// 통로를 놓고 `-32061`(결과 불명)로 답하므로, 봉투마다 정해지는 그 상한은
    /// 무한 대기하는 호출을 같이 자르지 않는다 — 소켓 한 값으로는 그 구분이 안 된다는
    /// 것이 위 문단의 이유였다. 이 연결은 그 필드를 스스로 채우지 않는다 — 싣는 것은 요청을
    /// 만드는 호출자다(CLI 는 루트 플래그 `--response-timeout-ms` 로 단발 요청에만 싣는다).
    /// 안 실은 요청에는 EOF 판정이 여전히 유일한 감지 수단이다.
    pub fn new(stream: TcpStream) -> Result<Self> {
        // Nagle 해제 — 요청 한 줄을 보내고 응답을 기다리는 순수 request-response 라
        // 지연시켜 합칠 뒷 데이터가 애초에 없다. Nagle 이 켜져 있으면 요청 줄이 두
        // 세그먼트로 쪼개지는 순간(`writeln!` 은 본문과 개행을 나눠 쓴다) 뒷조각이
        // 상대의 delayed ACK(~40ms)까지 붙잡혀 **모든 CLI 명령**에 그만큼이 얹힌다.
        // 실패해도 연결 자체는 유효하므로 에러로 올리지 않는다.
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

    /// Send a JSON-RPC request and read the response.
    pub fn send(&mut self, request: &JsonRpcRequest) -> Result<serde_json::Value> {
        let json = serde_json::to_string(request)?;
        // 개행까지 한 버퍼에 담아 **한 번의 write** 로 보낸다 — `writeln!` 은 본문과
        // 개행을 각각 write 해 세그먼트를 쪼갠다(위 `TCP_NODELAY` 주석 참고).
        let mut line = json.into_bytes();
        line.push(b'\n');
        self.writer.write_all(&line)?;
        self.writer.flush()?;

        loop {
            let mut line = String::new();
            // EOF(`read_line` 이 0 을 반환)를 반드시 먼저 걸러낸다. EOF 는 이후로
            // 영원히 0 을 반환하므로, 빈 줄 skip 으로 흘려보내면 이 루프가 코어
            // 하나를 100% 로 태우며 무한 스핀한다(커널 블록이 아니라 유저스페이스
            // 스핀이라 겉보기 hang 과 구분도 어렵다). 호스트가 종료 중이거나
            // 크래시/SIGKILL 로 죽으면 실제로 밟는 경로다.
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

            let response: JsonRpcResponse = serde_json::from_str(trimmed)?;

            if let Some(error) = response.error {
                return Err(JsonRpcCallError {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                }
                .into());
            }

            return Ok(response.result.unwrap_or(serde_json::Value::Null));
        }
    }

    /// 서버가 선언한 기능 목록. 연결마다 **한 번만** 묻는다.
    ///
    /// `session_token` 은 부를 요청의 것을 그대로 쓴다 — 토큰이 있는 호출자가 토큰 없이
    /// 물으면 그 조회는 `Local` 로 판정돼, 실제로 요청을 보낼 주체와 **다른 주체의**
    /// 답을 보게 된다.
    pub fn capabilities(&mut self, session_token: Option<&str>) -> Result<&BTreeMap<String, u32>> {
        self.capabilities_within(session_token, None)
    }

    /// [`IpcConnection::capabilities`] 와 같되, 아직 안 물었으면 그 확인을 `bound` 안에 끝낸다.
    ///
    /// 상한을 실은 요청의 호출자는 "이 시간 뒤에는 돌아온다" 를 전제로 한다. 확인 요청도 호스트
    /// 메인 스레드의 큐를 지나므로, 그것만 상한 없이 기다리면 굳은 호스트에서 그 전제가 확인
    /// 단계에서 깨진다. 그래서 확인 요청은 두 겹으로 자른다:
    ///
    /// - 봉투에 같은 상한을 **밀리초 올림**으로 싣는다 — 그 이름을 아는 서버는 상한에서 확인
    ///   요청을 큐에서 물리고 답한다(나중에 실행되지 않는다). 올림인 이유: 내림이면 1 ms 밑으로
    ///   남은 상한(`--response-timeout-ms 1` 은 확인을 시작하는 순간 늘 그렇다)이 0 이 되어 확인
    ///   요청을 보내지도 못한다. 올려도 합계는 안 넘는다 — 기다림을 끊는 것은 아래 소켓 기한이다.
    /// - 소켓 읽기 기한을 같은 시간으로 건다 — 봉투를 모르는 **구 서버**는 필드를 조용히 버리고
    ///   무한정 기다리게 하므로, 그때도 같은 시간에 끝나려면 이쪽이 필요하다.
    ///
    /// 어느 쪽으로 끝나든 [`CapabilityProbeExpired`] 다 — 확인 요청의 결과가 무엇이었든 그 뒤에
    /// 보낼 요청은 안 나갔다. 확인이 끝나면 읽기 기한을 원래대로(없음) 돌린다. `bound` 가 0 이면
    /// 확인 요청을 보내지 않고 곧바로 그 오류다.
    pub fn capabilities_within(
        &mut self,
        session_token: Option<&str>,
        bound: Option<Duration>,
    ) -> Result<&BTreeMap<String, u32>> {
        if self.capabilities.is_none() {
            let mut probe = JsonRpcRequest {
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

    /// 읽기 기한을 걸고 보낸다. 기한 만료와, 상대가 봉투 상한으로 답한 만료(`-32061` ·
    /// `-32067`)는 [`CapabilityProbeExpired`] 로 모은다.
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
        // 기한 안에 답이 왔다. 뒤의 요청은 이 연결의 원래 규약(읽기 기한 없음)으로 기다린다.
        self.reader.get_ref().set_read_timeout(None)?;
        sent
    }

    /// [`IpcConnection::require_capability`] 와 같되, 확인 요청을 `bound` 안에 끝낸다
    /// ([`IpcConnection::capabilities_within`]).
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

    /// 그 계약을 상대가 아는지 **부수효과가 나기 전에** 판정한다.
    ///
    /// 판이 요구값 **이상**이면 통과다. 기능 목록의 판은 올라가는 방향으로만 움직이고
    /// 뜻이 바뀔 때 올라가며(그 규약은 [`crate::capability`] 에 있다), 높은 판은 낮은 판의
    /// 약속을 다 지킨다 — `ipc.idempotency-key` 판 2 는 판 1 의 engine 라우터에 App 층을
    /// 더한 것이다. 그래서 더 높은 판은 "이 이름을 더 잘 안다" 는 뜻이다.
    pub fn require_capability(
        &mut self,
        name: &str,
        min_version: u32,
        session_token: Option<&str>,
    ) -> Result<()> {
        self.require_capability_within(name, min_version, session_token, None)
    }

    /// 멱등 키를 실어 보낸다 — 단, **보내기 전에** 상대가 그 계약을 아는지 묻는다.
    ///
    /// 그냥 필드만 실으면 구 서버는 키를 조용히 버리고 요청을 그대로 실행한다. 그때
    /// 호출자는 계약이 걸린 줄 알고 재시도하므로 **두 번째 효과**가 남는다 — 키를 실은
    /// 목적과 정확히 반대다. 그래서 이 함수의 값은 키를 싣는 것이 아니라 **못 싣는
    /// 상대에게 안 보내는 것**이다.
    ///
    /// 확인 자체는 `system.info` 라 부수효과가 없고 연결마다 한 번이다.
    ///
    /// **확인은 둘이다.** 먼저 그 메서드의 선언을 읽는다([`crate::method_meta::key_contract`]).
    /// 계약 밖이면(plugin namespace forward 등) 연결을 쓰지도 않고 [`KeyOutsideContract`] 로
    /// 끝난다. 그다음 선언이 요구하는 판을 서버가 선언하는지 capability 로 묻는다 — engine
    /// 라우터 메서드는 판 1, App 층 메서드(창 생성 · plugin 설치 등)는 판 2 다. 판이 모자라면
    /// [`UnsupportedCapability`] 로 끝나고 요청은 안 나간다.
    ///
    /// 그래서 이 함수를 통과한 요청은 **보내기 전에** 셋 중 하나로 정해져 있다: 보존소가 받는다
    /// (`Kept`), 원래 안전하다(`Unneeded`). 계약 밖은 여기까지 안 온다. 응답의
    /// [`JsonRpcResponse::idempotent_replay`] 는 "이 답이 재생인가" 하나만 답하면 되고, 표지가
    /// 없는 것을 "계약 밖" 으로 읽을 일이 없다 — 계약 안인지는 이미 선언으로 안다. `Kept`
    /// 메서드에서 표지 없는 성공은 **이번에 실행한 것**이고, `-32063`·`-32064` 는 계약이
    /// 개입한(실행을 막거나 실행을 확인한) 답이다. 선언된 보존 범위를 벗어난 키는 처음 보는
    /// 키와 구별되지 않으므로 다시 실행된다 — 그 경계는 `system.info` 의 `idempotency` 가
    /// 값으로 준다.
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

/// 확인 요청의 봉투에 실을 밀리초 — **올림**이다(1 ms 밑이어도 남은 시간이 있으면 1).
/// 남은 시간이 0 이면 `None` — 실을 것도 기다릴 것도 없다. 합계를 지키는 것은 봉투가 아니라 같은
/// 시간으로 거는 소켓 읽기 기한이다([`IpcConnection::capabilities_within`]).
fn ceil_millis(left: Duration) -> Option<u64> {
    if left.is_zero() {
        return None;
    }
    let ms = left.as_nanos().div_ceil(1_000_000);
    Some(u64::try_from(ms).unwrap_or(u64::MAX))
}

/// 남은 시간을 **본 요청** 봉투의 밀리초로 옮긴다 — **내림**이다. 본 요청은 서버가 봉투 값으로 끊으므로
/// 올림이면 두 요청의 합이 상한을 넘을 수 있다. 확인 요청의 봉투는 올림이다(`ceil_millis`).
/// 0 이 되면 `None` — 봉투 규약상 `0` 은 "상한 없음" 이라 실을 수 없고, 남은 시간이 없다는 뜻이다.
pub fn whole_millis(left: Duration) -> Option<u64> {
    u64::try_from(left.as_millis()).ok().filter(|&ms| ms > 0)
}

/// [`crate::protocol::JsonRpcRequest::idempotency_key`] 를 서버가 읽는다는 선언의 이름.
pub const IDEMPOTENCY_CAPABILITY: &str = "ipc.idempotency-key";
/// 이 client 가 요구하는 **최소** 판 — 서버가 필드를 읽는다는 것까지. 메서드마다 더 높은 판을
/// 요구할 수 있다(`KeyContract::Kept` 의 `since`).
pub const IDEMPOTENCY_CAPABILITY_VERSION: u32 = crate::method_meta::KEY_KEPT_BY_ROUTER;

#[cfg(test)]
mod tests {
    use super::*;

    /// capability 키가 아예 없는 **구 서버**는 빈 선언이다. "모른다" 를 "안다" 로 읽으면
    /// 그 서버에 키를 실어 보내게 되고, 그것이 이 축의 결함이다.
    #[test]
    fn an_old_server_without_the_key_declares_nothing() {
        let info = serde_json::json!({ "version": "0.6.0", "scope": "engine" });
        assert!(parse_capabilities(&info).is_empty());
    }

    /// plugin namespace 이름은 client 프로세스에서 표가 모르는 이름이다 — 계약 밖으로 읽혀
    /// **보내기 전에** 거절된다. 호스트 메서드는 선언이 요구하는 판을 들고 다음 단계로 간다.
    #[test]
    fn a_key_is_refused_before_sending_for_a_name_the_host_table_does_not_own() {
        let refused = required_key_version("markdown.recent").unwrap_err();
        let typed = refused
            .downcast_ref::<KeyOutsideContract>()
            .expect("타입으로 갈라져야 호출자가 '안 나갔다' 를 안다");
        assert_eq!(typed.method, "markdown.recent");
        assert!(refused.to_string().contains("nothing was sent"));

        assert_eq!(
            required_key_version("workspace.create").expect("engine 라우터 메서드"),
            crate::method_meta::KEY_KEPT_BY_ROUTER
        );
        assert_eq!(
            required_key_version("window.create").expect("App 층 메서드"),
            crate::method_meta::KEY_KEPT_BY_APP_LAYER
        );
        assert_eq!(
            required_key_version("workspace.list").expect("읽기"),
            IDEMPOTENCY_CAPABILITY_VERSION
        );
    }

    /// App 층 메서드에 키를 실으려면 상대가 **판 2** 를 선언해야 한다. 판 1 서버는 그 층에서
    /// 키를 무시하므로(재시도가 두 번째 실행이 된다) 보내기 전에 거절하고, 요청은 안 나간다.
    /// 같은 서버에 engine 라우터 메서드는 통과한다 — 판 1 서버가 이미 받던 것을 막지 않는다.
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
        // 판 1 만 선언한 서버라고 미리 알고 있다(연결마다 한 번 묻는 `system.info` 의 결과).
        conn.capabilities = Some(
            [(IDEMPOTENCY_CAPABILITY.to_string(), 1u32)]
                .into_iter()
                .collect(),
        );

        let req = JsonRpcRequest {
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

    /// 거절은 **연결을 한 바이트도 쓰지 않는다** — 상대가 무엇이든 요청이 안 나간다.
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

    /// 오류 응답의 `error.data` 는 원형 그대로 호출자에게 간다 — 버리면 IPC 가 싣는 실패
    /// 분류(`storage_failure` · `reason`)를 CLI 가 볼 길이 없다(ADR-0512). `Display` 는 종전 한
    /// 줄 그대로다.
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

    /// 남은 시간은 내림으로 싣고, 1 ms 도 안 남았으면 실을 수 없다 — `0` 은 "상한 없음" 이다.
    #[test]
    fn what_is_left_rounds_down_and_nothing_left_is_none() {
        use std::time::Duration;
        assert_eq!(whole_millis(Duration::from_micros(299_900)), Some(299));
        assert_eq!(whole_millis(Duration::from_micros(999)), None);
        assert_eq!(whole_millis(Duration::ZERO), None);
    }

    /// 확인 요청의 봉투는 올림이다 — 1 ms 밑으로 남아도 1 이고(0 은 "상한 없음" 이라 실을 수 없다),
    /// 남은 것이 없을 때만 싣지 못한다.
    #[test]
    fn the_check_bound_rounds_up_and_only_nothing_left_is_none() {
        use std::time::Duration;
        assert_eq!(ceil_millis(Duration::from_nanos(1)), Some(1));
        assert_eq!(ceil_millis(Duration::from_micros(999)), Some(1));
        assert_eq!(ceil_millis(Duration::from_micros(1_001)), Some(2));
        assert_eq!(ceil_millis(Duration::from_millis(300)), Some(300));
        assert_eq!(ceil_millis(Duration::ZERO), None);
    }

    /// 모양이 어긋난 항목은 조용히 빠지고 나머지는 산다 — 한 항목이 전체 조회를
    /// 깨뜨리면 "추가는 안전하다" 가 거짓이 된다.
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

    /// 더 높은 판은 통과, 낮은 판과 부재는 거절 — 그리고 **부재와 낮은 판이 다른
    /// 값으로 보고된다**(호출자가 서버를 올릴지 요구를 내릴지 가른다).
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
        // 어느 쪽이든 "아직 안 보냈다" 를 말한다 — 재시도 판단이 거기 달렸다.
        assert!(missing.to_string().contains("nothing was sent"));
        assert!(older.to_string().contains("nothing was sent"));
    }
}
