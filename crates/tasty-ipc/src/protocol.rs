use serde::{Deserialize, Serialize};

// 전송 계층이 직접 내는 오류 코드의 자리 — `-32060..-32069`.
//
// 이 구역이 도메인 코드(`-32000..-32059`)와 갈리는 이유는 **호출자가 다음에 할 일이
// 다르기 때문**이다. 도메인 코드는 요청이 handler 까지 갔고 거기서 거절됐다는 뜻이라
// 인자·주체·상태를 본다. 이 구역은 요청이 **handler 에 닿지도 못했다**는 뜻이라 볼
// 곳이 요청의 모양이나 서버의 수용 여력이다. 두 사실을 같은 구역에 섞으면 재시도
// 정책을 고를 수 없다 — 앞의 것은 고쳐야 다시 되고, 뒤의 것은 그대로 다시 해도 된다.

/// 요청 한 줄이 서버의 줄 상한(`MAX_REQUEST_LINE_BYTES`)을 넘었다. 넘긴 줄의 나머지가
/// 소켓에 남아 있으므로 그 연결은 이 응답 **뒤에 닫힌다** — 계속 읽으면 한 줄이 여러
/// 요청으로 쪼개져 들어가 상한이 다시 없는 것이 된다. 상한 값은 응답 문구에 실린다.
pub const ERR_REQUEST_LINE_TOO_LONG: i32 = -32060;

/// 요청이 실은 응답 대기 상한이 만료됐다. **그 요청이 실행됐는지 안 됐는지는 이 답으로 알 수
/// 없다** — 호스트는 응답 통로를 놓았을 뿐이고 요청 자체는 메인 스레드에서 계속 실행될 수
/// 있다. 그래서 이 코드는 실패가 아니라 **결과 불명**을 뜻한다.
///
/// 이 코드가 오면 요청은 **이미 시작됐다.** 큐에서 기다리다 만료된 요청은 실행되지 않고
/// [`ERR_EXPIRED_BEFORE_RUN`] 으로 답한다. 시작된 요청을 끊는 수단은 없다 — 만료는 취소가
/// 아니다(ADR-0411).
///
/// 이 구분이 코드 하나를 따로 쓸 만한 이유: 호출자가 다음에 할 일이 그 값에 달렸다.
/// 부수효과가 남는 메서드(`MethodEffect::Mutate`)를 그냥 재전송하면 **두 번째 효과**가
/// 남는다. 기존 코드 중에 이 사실을 뜻하는 것이 없었다 — `-32603`(내부 오류)도
/// `-32000`(서버 사정)도 "안 됐다" 로 읽힌다.
pub const ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN: i32 = -32061;

/// 서버가 동시 연결 상한에 닿아 이 연결을 받지 않았다. **요청이 실행되지 않았다** —
/// 줄을 읽기도 전에 끝났으므로 `-32061` 과 달리 결과가 불명이지 않다. 그래서 그대로
/// 다시 걸어도 되고, 그것이 호출자가 할 일이다(자리가 나면 붙는다).
///
/// 이 답은 **최선 노력**이다. 거절이 일어나는 자리가 accept 스레드라 거기서 막히는
/// 쓰기를 할 수 없고(그 스레드가 멈추면 자리가 나도 아무도 못 붙는다), 그래서 서버는
/// 한 번만 시도하고 안 되면 그냥 닫는다. client 는 이 줄 대신 EOF 를 볼 수 있다.
///
/// ★ **이 줄을 오류 한 줄로 읽는 인구는 request-response client 뿐이다.** 거절은 accept
/// 직후, 즉 스트림 업그레이드 판별 **전**에 일어나므로 attach 같은 스트림 client 도 같은
/// 바이트를 받는데, 그쪽의 첫 읽기는 줄이 아니라 프레임이다 — 첫 바이트 `{`(123)가
/// [`crate::stream::StreamTag`] 에 없어 `unknown stream tag` 로 끝난다. 그쪽에서 이 코드는
/// EOF 를 **대체하지 못하고** 다른 실패로 바꿀 뿐이다. 프레임으로 싸서 보내려면 accept
/// 스레드가 상대의 종류를 알아야 하는데 그건 줄을 읽어야 알 수 있고, 줄을 읽지 않는 것이
/// 이 거절의 값이다.
pub const ERR_CONNECTION_LIMIT_REACHED: i32 = -32062;

/// 같은 멱등 키([`JsonRpcRequest::idempotency_key`])로 **다른 요청**이 왔다.
///
/// 키는 호출자가 "이것은 아까 그 요청이다" 를 말하는 수단이므로, 같은 키에 다른
/// 메서드·다른 params 가 붙었다면 둘 중 하나는 호출자의 착오다. 어느 쪽인지는 여기서
/// 알 수 없고, **아무것도 실행하지 않는 것**만이 두 뜻 모두에서 안전하다 — 실행하면
/// 키가 가리키던 앞선 결과를 덮거나 두 번째 효과를 남긴다.
///
/// 이 코드가 오면 호출자가 고칠 것은 인자가 아니라 **키의 재사용**이다. 그래서
/// `-32602`(invalid params)와 갈라 둔다.
pub const ERR_IDEMPOTENCY_KEY_CONFLICT: i32 = -32063;

/// 그 멱등 키의 요청은 **실행됐지만 그 답을 더는 갖고 있지 않다**.
///
/// 보존소는 항목마다 크기 상한이 있고(그 값은 호스트가 정해 `system.info` 의
/// `idempotency` 로 선언한다), 그 상한을 넘는 답은 보관하지 않는다. 그때 키만
/// 남기고 답을 버리는 이유는 **재전송이 두 번째 효과를 남기는 것을 막기 위해서**다 —
/// 항목까지 지우면 다음 요청이 처음 보는 키가 되어 그대로 다시 실행된다.
///
/// 그래서 이 답은 실패도 미실행도 아니다. [`ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN`] 과
/// 같은 계열의 **결과 불명**이되 사실이 하나 더 있다 — 실행된 것은 **확실하다**.
/// 호출자가 할 일은 재전송이 아니라 조회다.
pub const ERR_IDEMPOTENT_RESULT_DISCARDED: i32 = -32064;

/// 호스트의 명령 큐가 차서 이 요청을 **큐에 넣지 않았다**. **요청이 실행되지 않았다** —
/// 판정은 큐에 넣기 전에 일어나므로 `-32061` 과 달리 결과가 불명이지 않다.
///
/// "찼다" 의 좌변은 명령 수가 아니라 큐에 든 요청 **바이트의 합**이다(정의와 상한은
/// `crate::admission`). 한 건의 크기는 [`ERR_REQUEST_LINE_TOO_LONG`] 이 따로 자르므로, 이
/// 코드가 뜻하는 것은 "이 요청이 크다" 가 아니라 "지금 호스트가 밀려 있다" 다. 그래서
/// 고칠 것이 없고, 호출자가 할 일은 **잠시 뒤 그대로 다시 거는 것**이다. 연결은 닫히지
/// 않는다 — 줄은 끝까지 읽혔으므로 다음 요청이 같은 연결로 와도 된다.
pub const ERR_COMMAND_QUEUE_FULL: i32 = -32065;

/// 연결이 열린 뒤 첫 요청 줄이 정해진 시간 안에 오지 않아 호스트가 연결을 닫는다.
/// **요청이 실행되지 않았다** — 줄을 받지 못했으므로 실행할 것이 없다.
///
/// 이 한도는 **첫 줄에만** 걸린다. 한 번이라도 요청을 보낸 연결은 요청 사이에 얼마나
/// 쉬어도 닫히지 않는다(오래 붙어 있는 client 의 호환). 이 답은 최선 노력이다 — 쓰기에도
/// 시간 상한이 있어 client 는 이 줄 대신 EOF 를 볼 수 있다. 근거는 ADR-0392.
pub const ERR_FIRST_LINE_IDLE: i32 = -32066;

/// 호출자가 실은 응답 대기 상한이 **요청이 큐에서 기다리는 동안** 지났다. **요청이 실행되지
/// 않았다** — 호스트는 실행 직전에 기한을 보고, 지났으면 실행하지 않는다. 그래서 `-32061` 과
/// 달리 결과가 불명이지 않고, 그대로 다시 보내도 두 번째 효과가 남지 않는다.
///
/// 두 코드는 **같은 상한의 만료**에서 갈린다 — 가르는 것은 만료 순간 요청이 시작됐는가다.
/// 시작 전이면 이 코드, 시작 뒤면 `-32061` 이다. 그 판정은 명령마다 한 번, 비교-교환 하나로
/// 한다(`crate::server::CommandLifecycle`). 연결은 닫히지 않는다. 근거는 ADR-0411.
pub const ERR_EXPIRED_BEFORE_RUN: i32 = -32067;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
    /// 자식 agent (claude.spawn 등으로 호스트가 띄운 프로세스) 가
    /// 호스트에 IPC 호출 시 자기 신원을 증명하는 token. 64-char lowercase hex.
    /// 호스트는 [`crate::ipc::session::SessionStore`] 로 resolve 해 `CallerContext::Agent`
    /// 를 만든다. 토큰이 invalid/expired/revoked 면 `permission_denied` 로 거부 —
    /// Local 로 fallback 하지 않는다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// 이 요청의 응답을 호출자가 기다릴 최대 시간(밀리초). 서버는 그 시간이 지나면
    /// 응답 통로를 놓고 [`ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN`] 으로 답한다.
    ///
    /// **없으면 상한이 없다.** 그것이 이 필드 이전의 동작이고, 기본값이 상한이 되면
    /// 사람의 결재를 기다리는 호출(`approval.await`)이 잘린다 — 사람에게는 상한을 둘 수
    /// 없다. `Some(0)` 도 같은 뜻으로 읽는다(무한). 레포의 기존 관례가 그렇다:
    /// `approval.await` 와 `agent.task_await` 의 `timeout_ms` 파라미터가 0 을 무한으로
    /// 읽는다. **이 필드는 그 관례를 봉투 수준으로 올린 것**이고, 그래서 메서드마다
    /// 다시 정하지 않아도 모든 메서드에 같은 뜻으로 붙는다.
    ///
    /// 구 서버는 이 키를 **조용히 무시한다** — 이 구조체에 `deny_unknown_fields` 가 없다.
    /// 그래서 새 client 가 보내도 깨지지 않고, 대신 상한이 안 걸린다. 서버가 이것을
    /// 읽는지 확인하려면 `system.info` 의 capability 목록에서 `ipc.response-timeout` 을
    /// 본다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_timeout_ms: Option<u64>,
    /// 이 요청이 **아까 그 요청과 같은 것**임을 호출자가 선언하는 이름.
    ///
    /// 응답만 유실된 요청을 다시 보낼 때, 이 키가 없으면 받는 쪽은 그것이 새 요청인지
    /// 재시도인지 구별할 수단이 없다 — `id` 는 응답 대응용이라 연결마다 다시 매겨지고,
    /// params 가 같다는 것도 "같은 일을 두 번 하려는 것" 과 구별되지 않는다. 그래서
    /// 구별을 만드는 것은 **호출자뿐**이고 이 필드가 그 자리다.
    ///
    /// 키가 뜻을 가질 **수 있는** 것은 [`crate::method_meta::MethodEffect::Mutate`] 로
    /// 분류된 메서드뿐이다. 나머지 둘은 정의상 재전달이 안전하므로(읽기는 흔적을 안
    /// 남기고, 멱등은 같은 끝 상태로 수렴한다) 보존소에 넣을 이유가 없다 — 넣으면 조회가
    /// 낡은 답을 받는다. 그 메서드들에서 키는 **봉투 검사만 받고 아무 일도 안 한다.**
    ///
    /// **봉투 검사는 목적지와 무관하다.** 길이 밖 키(빈 문자열, 상한 초과)는 요청이 App 층 ·
    /// plugin namespace forward · engine 라우터 중 어디로 가든 `-32602` 다. 검사 자리가 그
    /// 셋보다 앞인 진입 게이트(`check_request`)이기 때문이다 — ADR-0420.
    ///
    /// ★ **`Mutate` 는 상한이지 보장이 아니다.** 호스트의 보존소는 호스트가 아는 이름이
    /// 끝나는 경로마다 배선돼 있다 — engine 라우터, App 층(창 · plugin 설치 · 스크린샷 ·
    /// 원격 attach 처럼 `App` 이 끝내는 메서드, ADR-0421), 그리고 그 뒤의 GUI debug step 과
    /// plugin namespace 로 forward 되는 **표의** 이름(`image.open` 등, ADR-0566). 계약
    /// **밖**은 표가 모르는 plugin 고유 이름뿐이다 — 키를 실어도 호스트가 보존소를 안
    /// 거친다(ADR-0361).
    ///
    /// 그 차이는 **보내기 전에** 안다 — 메서드마다 이름 표가 선언한다
    /// ([`crate::method_meta::key_contract`]: 보존소가 받는다 · 원래 안전하다 · 계약 밖).
    /// 그래서 응답의 [`JsonRpcResponse::idempotent_replay`] 는 "이 답이 재생인가" 하나만
    /// 답한다. 표지의 부재를 "계약 밖" 으로 읽을 필요가 없다 — 보존소가 받는 메서드에서 표지
    /// 없는 성공은 **이번에 실행한 것**이고, [`ERR_IDEMPOTENCY_KEY_CONFLICT`](같은 키·다른
    /// 요청 — 실행을 막았다)와 [`ERR_IDEMPOTENT_RESULT_DISCARDED`](실행은 됐고 답을 버림)는
    /// 계약이 개입한 답이다. 보존 범위 밖으로 밀려난 키는 처음 보는 키와 구별되지 않아 다시
    /// 실행되고 표지 없이 성공을 낸다 — 그 경계가 아래 선언이다(ADR-0423).
    ///
    /// 보장의 범위는 호스트가 `system.info` 의 `idempotency` 로 선언한다(보존 시간 ·
    /// 항목 수 · 재시작 생존 여부). 그 범위를 벗어난 키는 처음 보는 키와 구별되지
    /// 않으므로 **다시 실행된다** — 그 경계를 값으로 내놓는 것이 선언의 목적이다.
    ///
    /// 구 서버는 이 키를 **조용히 무시한다**(이 구조체에 `deny_unknown_fields` 가 없다).
    /// 그래서 보내기만 해서는 계약이 걸렸는지 알 수 없고, client 는 **부수효과가 나기
    /// 전에** `system.info` 의 capability 목록에서 `ipc.idempotency-key` 를 봐야 한다
    /// ([`crate::client::IpcConnection::send_idempotent`] 가 그것을 한다).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
    /// 이 답은 **지금 실행해서 나온 것이 아니라 보관돼 있던 것**이다.
    ///
    /// [`JsonRpcRequest::idempotency_key`] 를 실은 재시도가 받는 표지다. 이것이 없으면
    /// 호출자는 같은 답을 두 번 받고도 그것이 **중복 실행의 결과**인지 **앞선 실행의
    /// 재조회**인지 구별할 수 없다 — 두 경우의 부수효과가 정반대인데 바이트는 같다.
    ///
    /// 거짓일 때는 wire 에 안 나간다. 그래서 이 필드는 **추가**이고, 모르는 키를
    /// 무시하는 구 client 와 이 키가 없는 구 서버 양쪽에서 종전 그대로 읽힌다.
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent_replay: bool,
}

/// `skip_serializing_if` 전용 — 거짓이면 키 자체를 빼서 종전 바이트를 유지한다.
fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
            idempotent_replay: false,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
            id,
            idempotent_replay: false,
        }
    }

    pub fn method_not_found(id: serde_json::Value, method: &str) -> Self {
        Self::error(id, -32601, format!("Method not found: {}", method))
    }

    /// 외부(CLI / 네트워크 IPC) 호출자가 dispatch 끝까지 못 닿은 이름에 대한 답.
    ///
    /// `-32601`("그런 메서드 없다")은 호출자를 **이름을 의심하는 쪽**으로 보낸다 —
    /// 오타를 고치거나 표를 다시 읽는다. 그 방향에 고칠 것이 없는 경우가 셋이고, 셋 다
    /// **이름은 맞다**. 무엇이 다른지가 호출자가 다음에 할 일을 가른다:
    ///
    /// | 사실 | 코드 | 호출자가 다음에 할 일 |
    /// |------|------|----------------------|
    /// | 부를 수 있는 주체가 다르다 | `-32016` | 호출 주체를 본다 |
    /// | 이 플랫폼에서 안 된다 | `-32015` | 플랫폼을 본다 |
    /// | 이 바이너리에 안 들어 있다 | `-32017` | 조합(헤드리스/release)을 본다 |
    /// | 이름이 틀렸다 | `-32601` | 이름을 고친다 |
    ///
    /// 앞의 둘은 [ADR-0163](../../../docs/adr/0163-a-registered-name-answers-who-not-whether.md)
    /// 과 [ADR-0154](../../../docs/adr/0154-a-platform-gated-dispatch-arm-answers-why-not-what.md)
    /// 가 결정했고, 셋째가 이 함수의 마지막 갈래다.
    ///
    /// ## 셋째 갈래의 술어가 왜 [`is_registered_name`] 인가
    ///
    /// [`method_meta`] 로 물으면 안 된다. 그 함수는 **런타임 등록 plugin prefix** 까지
    /// 해소하므로 설치된 plugin 의 이름과 그 아래 오타까지 `Some` 을 준다 — 그것으로
    /// 갈래를 타면 plugin 으로 갈 호출이 host 의 답을 받는다(실측 근거는
    /// [`is_registered_name`] 에 있다).
    ///
    /// [`is_registered_name`]: crate::method_meta::is_registered_name
    /// [`method_meta`]: crate::method_meta::method_meta
    pub fn unrouted_for_external_caller(id: serde_json::Value, method: &str) -> Self {
        match crate::method_meta::method_meta(method) {
            Some(m) if m.plugin_only => Self::error(
                id,
                -32016,
                format!(
                    "method '{method}' is plugin-only: only the plugin host-call path \
                     dispatches it, so CLI and network IPC callers have no entry point"
                ),
            ),
            _ if crate::method_meta::is_registered_name(method) => Self::error(
                id,
                -32017,
                format!(
                    "method '{method}' is registered but this binary has no dispatch \
                     arm for it: it is gated out of this build combination \
                     (headless / release)"
                ),
            ),
            _ => Self::method_not_found(id, method),
        }
    }

    pub fn invalid_params(id: serde_json::Value, msg: impl Into<String>) -> Self {
        Self::error(id, -32602, msg)
    }

    pub fn internal_error(id: serde_json::Value, msg: impl Into<String>) -> Self {
        Self::error(id, -32603, msg)
    }

    /// `error` 와 동일하되 `error.data` 도 함께 싣는다 — 호출자가 에러 메시지
    /// 문자열 파싱 없이 구조화된 부가정보(예: 참조 중인 task id 목록)를 받을 때.
    pub fn error_with_data(
        id: serde_json::Value,
        code: i32,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: Some(data),
            }),
            id,
            idempotent_replay: false,
        }
    }

    /// 보관돼 있던 답을 **이번 요청의 `id` 로** 다시 낸다.
    ///
    /// `id` 를 갈아 끼우는 것이 이 함수의 전부가 아니다 — 갈아 끼우지 **않으면** 답이
    /// 앞선 요청의 `id` 를 달고 나가고, 그러면 한 연결에서 여러 요청을 띄워 둔 client 가
    /// 이 답을 자기 어느 요청에도 못 붙인다(JSON-RPC 의 대응은 `id` 하나로만 선다).
    /// 그래서 보존소는 응답을 통째로 들고 있되 `id` 만은 **답할 때** 정한다.
    pub fn replayed_for(&self, id: serde_json::Value) -> Self {
        Self {
            id,
            idempotent_replay: true,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "workspace.list".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, "workspace.list");
        assert_eq!(parsed.jsonrpc, "2.0");
        assert!(parsed.session_token.is_none());
    }

    #[test]
    fn request_session_token_roundtrip() {
        // 토큰이 None 이면 wire 에 안 나가야 한다(공간/노이즈 절감).
        let req_none = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "x.y".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let json_none = serde_json::to_string(&req_none).unwrap();
        assert!(!json_none.contains("session_token"));

        // Some(_) 면 직렬화 + 역직렬화 보존.
        let token = "a".repeat(64);
        let req_some = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "x.y".into(),
            params: serde_json::json!({}),
            id: Some(serde_json::json!(1)),
            session_token: Some(token.clone()),
            response_timeout_ms: None,
            idempotency_key: None,
        };
        let json_some = serde_json::to_string(&req_some).unwrap();
        assert!(json_some.contains("session_token"));
        let parsed: JsonRpcRequest = serde_json::from_str(&json_some).unwrap();
        assert_eq!(parsed.session_token.as_deref(), Some(token.as_str()));
    }

    /// 표에 `plugin_only` 로 등재된 이름은 "없다" 가 아니라 "부를 수 있는 주체가
    /// 다르다" 로 답한다. 두 답은 호출자가 **다음에 할 일**이 다르다 — `-32601` 은
    /// 이름을 고치게 하고, `-32016` 은 호출 주체를 보게 한다.
    #[test]
    fn a_plugin_only_method_is_not_answered_as_missing() {
        let resp =
            JsonRpcResponse::unrouted_for_external_caller(serde_json::json!(1), "banner.open");
        let err = resp.error.expect("에러여야 한다");
        assert_eq!(err.code, -32016, "plugin-only 인데 -32601 로 답했다");
        assert!(
            err.message.contains("plugin-only"),
            "사유가 메시지에 없다: {}",
            err.message
        );
    }

    /// 등재되지 않은 이름은 그대로 `-32601` 이다 — 이 갈래가 무너지면 오타가
    /// "주체가 다르다" 로 보고돼 호출자가 영영 못 고친다.
    #[test]
    fn an_unregistered_name_is_still_method_not_found() {
        let resp = JsonRpcResponse::unrouted_for_external_caller(
            serde_json::json!(1),
            "no.such.method.exists",
        );
        assert_eq!(resp.error.expect("에러여야 한다").code, -32601);
    }

    /// 등재됐지만 `plugin_only` 가 아닌 이름이 여기까지 왔다면 **이 조합에 arm 이 없는
    /// 것**이고, 그렇게 답한다.
    ///
    /// 이 자리는 원래 `-32601` 이었다. 그 근거는 "안 닿는 것은 다른 이유(플랫폼·조합
    /// 게이트)이고 답도 그 층이 낸다" 였는데, 실행해 보면 **그 층은 답하지 않는다** —
    /// 조합 게이트는 `match` 팔을 통째로 없애므로 호출은 `_` 로 떨어져 바로 여기 온다.
    /// 그래서 오타와 구분이 안 됐다(실측 2026-09-05: 헤드리스에서 `window.creat` 와
    /// `window.create` 의 응답이 바이트 단위로 같았다).
    #[test]
    fn a_registered_name_with_no_arm_here_says_so() {
        let resp =
            JsonRpcResponse::unrouted_for_external_caller(serde_json::json!(1), "system.info");
        let err = resp.error.expect("에러여야 한다");
        assert_eq!(err.code, -32017, "등재된 이름인데 -32601 로 답했다");
        assert!(
            err.message.contains("no dispatch arm"),
            "사유가 메시지에 없다: {}",
            err.message
        );
    }

    /// 오타와 등재된 이름이 **다른 코드**로 갈린다.
    ///
    /// 이 축의 결함이 정확히 이 구분의 부재였다. 한 이름만 보면 어느 쪽이 틀렸는지 알 수
    /// 없으므로 **짝으로** 본다 — 한 글자만 다른 두 이름을 같은 함수에 넣는다.
    #[test]
    fn a_typo_and_a_registered_name_do_not_get_the_same_answer() {
        let real =
            JsonRpcResponse::unrouted_for_external_caller(serde_json::json!(1), "workspace.create");
        let typo =
            JsonRpcResponse::unrouted_for_external_caller(serde_json::json!(1), "workspace.creat");
        let real = real.error.expect("에러여야 한다");
        let typo = typo.error.expect("에러여야 한다");
        assert_eq!(real.code, -32017);
        assert_eq!(typo.code, -32601);
        assert_ne!(
            real.code, typo.code,
            "오타와 등재된 이름이 같은 답을 받는다 — 호출자가 무엇을 고쳐야 할지 모른다"
        );
    }

    #[test]
    fn response_success() {
        let resp = JsonRpcResponse::success(serde_json::json!(1), serde_json::json!({"ok": true}));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.result.is_some());
    }

    #[test]
    fn response_error() {
        let resp = JsonRpcResponse::error(serde_json::json!(1), -32601, "Method not found");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
    }

    #[test]
    fn response_method_not_found() {
        let resp = JsonRpcResponse::method_not_found(serde_json::json!(1), "foo.bar");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
        assert!(resp.error.as_ref().unwrap().message.contains("foo.bar"));
    }

    #[test]
    fn response_roundtrip() {
        let resp = JsonRpcResponse::success(serde_json::json!(42), serde_json::json!({"count": 5}));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, serde_json::json!(42));
        assert_eq!(parsed.result.unwrap()["count"], 5);
    }
}
