//! 웹훅 리스너 싱글턴 라우터 — tiny_http 단일 포트 + std::thread accept.
//!
//! 기존 IPC 서버(`tcp_ipc_server.rs`)의 std::thread accept + 요청별 worker 패턴을
//! 미러링한다. 매칭된 요청은 (path) → 웹훅 → IpcSequence 로 라우팅되어
//! `HostIpcInjector` 로 메인루프에 주입된다(별도 waker 불필요 — injector 가 내장).
//!
//! ## 흐름 (단방향 불변식)
//! `요청 파싱 → 매칭 → build_ack 즉시 응답 → (별도) IpcSequence fire-and-forget`.
//! ACK 는 실행 전/무관하게 확정되며 실행 결과에 닿지 않는다.

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::OnceLock;
use std::thread;

use serde_json::Value;

use super::WebhookInitReport;
use super::abuse;
use super::ack::{AckStatus, build_ack};
use super::registry::{self, MatchResult};
use crate::hook_handler::{SequenceOrigin, SubstitutionContext, execute_sequence};
use tasty_ipc::host_call::HostIpcInjector;

/// 리스너 init — runtime 주입 후 tiny_http 를 bind 하고 accept thread 를 띄운다.
///
/// 포트는 **설정값 only**(자동 폴백 bind 없음). bind 실패는 삼키지 않고 경고 —
/// 사용자가 포트/방화벽을 직접 조치한다(다중 bind 가드로 중복 호출 무해). 결과를
/// [`WebhookInitReport`] 로 반환해 caller 가 UI/로그로 노출한다.
pub fn init(injector: HostIpcInjector, bind_addr: &str, port: u16) -> WebhookInitReport {
    registry::set_runtime(injector, bind_addr, Some(port));
    if registry::is_bound() {
        tracing::debug!("webhook listener already bound; skip re-init");
        return WebhookInitReport::Bound;
    }
    let addr = format!("{bind_addr}:{port}");
    match tiny_http::Server::http(addr.as_str()) {
        Ok(server) => on_bind_success(server, &addr),
        Err(e) => on_bind_failed(e.to_string(), &addr, port),
    }
}

/// bind 성공 시 registry 를 bound 로 표시하고 accept 스레드를 띄운다.
fn on_bind_success(server: tiny_http::Server, addr: &str) -> WebhookInitReport {
    registry::mark_bound();
    tracing::info!("webhook listener bound on {addr}");
    spawn_accept_thread(server);
    WebhookInitReport::Bound
}

/// bind 실패를 경고 로그로 남기고(자동 폴백 없음 — 사용자가 직접 조치) 보고서로 변환.
fn on_bind_failed(error: String, addr: &str, port: u16) -> WebhookInitReport {
    tracing::warn!(
        "webhook listener bind {addr} failed: {error} — set a free port and check firewall (no auto-fallback)"
    );
    WebhookInitReport::BindFailed {
        #[cfg(feature = "gui")]
        port,
        #[cfg(feature = "gui")]
        error,
    }
}

/// accept 스레드 스폰. 실패해도 bind 자체는 이미 성공했으므로 경고만 남긴다
/// (재시도 없음 — 다중 bind 가드가 재호출을 무해하게 만들 뿐 자동 복구는 안 함).
fn spawn_accept_thread(server: tiny_http::Server) {
    if let Err(e) = thread::Builder::new()
        .name("webhook-accept".into())
        .spawn(move || accept_loop(server))
    {
        tracing::warn!("webhook accept thread spawn failed: {e}");
    }
}

/// accept 루프 — 요청별 worker thread 로 넘겨 IpcSequence dispatch 가 accept 를
/// 막지 않게 한다(tcp_ipc_server 패턴).
fn accept_loop(server: tiny_http::Server) {
    for request in server.incoming_requests() {
        thread::spawn(move || handle_request(request));
    }
}

/// 한 요청 처리: (남용차단) → 파싱 → 매칭 → ACK 응답 → fire-and-forget 실행.
fn handle_request(request: tiny_http::Request) {
    // 출처 IP(포트 제외 — 스캐너는 IP 를 재사용하며 포트만 바꾼다. 포트를 키에 넣으면
    // 한 발신자가 요청마다 새 키를 만들어 카운터를 우회한다. 대가는 NAT 뒤 공유 —
    // `docs/adr/0196-abuse-thresholds-and-source-key.md`).
    let source = request.remote_addr().map(|a| a.ip().to_string());

    let Some(mut request) = reject_if_abusive(request, source.as_deref()) else {
        return;
    };

    let url = request.url();
    let method = request.method();

    let (path_raw, query_str) = match url.split_once('?') {
        Some((p, q)) => (p, q),
        None => (url.as_str(), ""),
    };
    let path = path_raw.trim_start_matches('/').to_string();
    let query = parse_query(query_str);

    let headers = request.headers();

    // body 상한 초과는 매칭보다 앞에서 끝난다 — 등록 여부·인증 여부와 무관하게
    // 읽지 않기로 한 것이라, 그 판정에 레지스트리를 물을 이유가 없다.
    let (ack, exec, body) = match request.read_json_body(max_body_bytes()) {
        Ok(body) => {
            let (ack, exec) = resolve_ack(&path, &method, &headers, &query, &body);
            (ack, exec, body)
        }
        Err(BodyTooLarge) => (AckStatus::PayloadTooLarge, None, Value::Null),
    };

    // 매칭·인증 실패와 body 상한 초과는 출처 실패로 집계(임계치 초과 시 다음 요청부터 쿨다운 429).
    // 무엇을 세는지는 `abuse::counts_as_failure` 가 정한다 — 무엇이 남용인가는
    // 남용차단의 정책이고, 여기 인라인 조건으로 두면 그 답이 두 곳에 생긴다.
    if abuse::counts_as_failure(ack)
        && let Some(src) = source.as_deref()
    {
        abuse::record_failure(src);
    }

    // 단방향: ACK 를 실행 전/무관하게 즉시 확정·응답.
    request.respond(ack);

    // fire-and-forget 실행 — 실행 결과는 응답 경로에 절대 닿지 않는다.
    if let Some((calls, Some(injector))) = exec {
        let ctx = SubstitutionContext {
            body,
            headers,
            query,
        };
        execute_sequence(SequenceOrigin::Webhook, &injector, &calls, &ctx);
    }
}

/// JSON 입력 body 가 요청당 상한을 넘었다 — 파서의 읽기를 멈췄다는 표시.
struct BodyTooLarge;

/// 요청 하나에서 JSON 처리에 허용하는 body 의 상한(바이트).
///
/// 이 값은 JSON 입력에 적용된다. 동시에 들어오는 요청 수는 제한하지 않으며,
/// 프로세스 전체 메모리 상한이 아니다. 리스너는 요청마다 스레드를 띄운다.
/// 초과 요청의 잔여 body 는 읽지 않고 연결을 닫는다([`Screened::respond`]).
/// ([ADR-0200](../../docs/adr/0200-webhook-body-has-a-per-request-byte-cap.md)).
///
/// 기본 1 MiB. `TASTY_WEBHOOK_MAX_BODY_BYTES` 로 오버라이드한다(0·파싱 실패는 기본값).
fn max_body_bytes() -> usize {
    const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
    static CACHED: OnceLock<usize> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("TASTY_WEBHOOK_MAX_BODY_BYTES")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_BODY_BYTES)
    })
}

/// 남용차단 선검사를 **통과한** 요청.
///
/// 이 타입은 body 를 JSON 입력으로 모으는 파서의 입구를 제한한다.
/// 쿨다운 중인 요청은 [`reject_if_abusive`] 에서 소비되므로 `Screened` 가 만들어지지
/// 않고 `read_json_body` 도 호출되지 않는다. HTTP 라이브러리의 작은 body 사전
/// 버퍼링은 이 타입의 제어 범위 밖이다.
/// 타입은 남용차단 뒤에 파서를 호출하는 순서를 강제한다.
/// ([ADR-0199](../../docs/adr/0199-the-block-is-decided-before-the-body-is-read.md)).
struct Screened(tiny_http::Request);

impl Screened {
    fn url(&self) -> String {
        self.0.url().to_string()
    }

    fn method(&self) -> String {
        self.0.method().to_string().to_ascii_uppercase()
    }

    /// 요청 헤더를 소문자 필드명 맵으로 수집.
    fn headers(&self) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        for h in self.0.headers() {
            headers.insert(
                h.field.as_str().as_str().to_ascii_lowercase(),
                h.value.as_str().to_string(),
            );
        }
        headers
    }

    /// JSON 처리에 허용하는 body 는 `limit` 바이트(초과 판정용으로 1바이트 추가 읽기).
    /// 읽기 실패/비-JSON 바디는
    /// `Value::Null`, 상한 초과는 [`BodyTooLarge`].
    ///
    /// **여기가 body 를 JSON 입력 버퍼로 모으는 자리다**([ADR-0199](../../docs/adr/0199-the-block-is-decided-before-the-body-is-read.md)),
    /// 이 파서의 입력 상한은 여기서 판정한다. 두 갈래를 함께 막는다 —
    /// 선언된 길이(`Content-Length`)가 이미 크면 **이 함수에서 읽지 않고**, 길이 선언이
    /// 없는 `Transfer-Encoding: chunked` 는 상한 + 1 바이트에서 멈춘다. 뒤엣것이 없으면
    /// 상한이 상한이 아니다 — chunked 에는 선언된 길이가 아예 없다
    /// ([ADR-0200](../../docs/adr/0200-webhook-body-has-a-per-request-byte-cap.md)).
    ///
    /// 상한을 넘기면 이 파서는 더 읽지 않는다. 잔여 body 를 HTTP 계층도 읽지 않게
    /// 하는 것은 413 을 보내는 [`Screened::respond`] 의 몫이다.
    fn read_json_body(&mut self, limit: usize) -> Result<Value, BodyTooLarge> {
        if self
            .0
            .body_length()
            .is_some_and(|declared| declared > limit)
        {
            return Err(BodyTooLarge);
        }
        let mut body_bytes = Vec::new();
        // 상한 + 1 — 딱 상한만 읽으면 "정확히 상한" 과 "더 있다" 를 못 가른다.
        let capped = limit.saturating_add(1) as u64;
        if let Err(e) = self.0.as_reader().take(capped).read_to_end(&mut body_bytes) {
            tracing::debug!("webhook body read failed: {e}");
        }
        // UTF-8 변환 실패로 바이트 수를 잃기 전에 초과를 판정한다.
        if body_bytes.len() > limit {
            return Err(BodyTooLarge);
        }
        let body_str = match String::from_utf8(body_bytes) {
            Ok(body) => body,
            Err(e) => {
                tracing::debug!("webhook body UTF-8 decode failed: {e}");
                return Ok(Value::Null);
            }
        };
        Ok(serde_json::from_str::<Value>(&body_str).unwrap_or(Value::Null))
    }

    /// 고정 ACK 로 응답하고 요청을 소비한다.
    ///
    /// 413 은 응답 뒤 연결을 닫는다 — 상한을 넘긴 body 의 나머지를 읽지 않으므로 그
    /// 연결의 스트림은 다음 요청의 시작에 있지 않다. 닫지 않으면 `tiny_http` 의
    /// Content-Length 리더가 Drop 에서 잔여 선언 길이를 끝까지 읽는다(drain). 닫는
    /// 경로는 레포에 vendoring 한 `tiny_http` 의 패치다
    /// ([ADR-0516](../../docs/adr/0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md)).
    fn respond(self, ack: AckStatus) {
        let response = build_ack(ack);
        let result = if ack == AckStatus::PayloadTooLarge {
            self.0.respond_and_close(response)
        } else {
            self.0.respond(response)
        };
        if let Err(e) = result {
            tracing::debug!("webhook ack respond failed: {e}");
        }
    }
}

/// 남용 차단(쿨다운 중) 출처면 즉시 429 로 응답하고 `None`(요청 소비 완료,
/// 호출자는 더 진행하지 않음)을 반환한다. 아니면 `request` 소유권을 그대로
/// 돌려준다.
///
/// 429 도 413 처럼 body 를 읽지 않고 답하므로 같은 자리에서 연결을 닫는다
/// ([`Screened::respond`] 와 같은 이유 — 읽지 않은 body 를 HTTP 계층이 Drop 에서 끝까지
/// 읽으면 선언 길이에 비례한 할당이 생기고, 그 할당이 실패하면 프로세스가 끝난다).
fn reject_if_abusive(request: tiny_http::Request, source: Option<&str>) -> Option<Screened> {
    if let Some(src) = source
        && abuse::is_source_blocked(src)
    {
        if let Err(e) = request.respond_and_close(build_ack(AckStatus::TooManyRequests)) {
            tracing::debug!("webhook 429 respond failed: {e}");
        }
        return None;
    }
    Some(Screened(request))
}

/// 매칭된 웹훅 실행에 필요한 (호출 시퀀스, injector). injector 는 아직 준비되지
/// 않았을 수 있어(주입 전) `Option`.
type PendingExec = (Vec<crate::hook_handler::IpcCall>, Option<HostIpcInjector>);

/// path/method 매칭 + (설정돼 있으면) 인증 검증까지 수행해 응답할 ACK 상태와,
/// 실행할 게 있으면 그 `(calls, injector)` 를 반환한다.
fn resolve_ack(
    path: &str,
    method: &str,
    headers: &BTreeMap<String, String>,
    query: &BTreeMap<String, String>,
    body: &Value,
) -> (AckStatus, Option<PendingExec>) {
    // 인증 술어는 레지스트리가 lock 안에서 부른다 — 차감이 인증보다 앞서지 않게
    // 하려는 것이다. 검증은 ACK 상태코드 선택에만 관여하고 실행/응답바디에
    // 데이터를 싣지 않는다(단방향 불변식).
    match registry::match_request(path, method, |a| a.verify(headers, query, body)) {
        MatchResult::NotFound => (AckStatus::NotFound, None),
        MatchResult::MethodNotAllowed => (AckStatus::MethodNotAllowed, None),
        MatchResult::Expired => (AckStatus::Gone, None),
        MatchResult::Unauthorized => (AckStatus::Unauthorized, None),
        MatchResult::Matched { calls, injector } => (AckStatus::Received, Some((calls, injector))),
    }
}

/// `key=val&k2=v2` 쿼리 문자열 파싱(MVP — percent-decode 없음).
fn parse_query(q: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(k.to_string(), v.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_basic() {
        let m = parse_query("token=abc&x=1");
        assert_eq!(m.get("token"), Some(&"abc".to_string()));
        assert_eq!(m.get("x"), Some(&"1".to_string()));
    }

    /// 401 은 통을 태우지 않는다 — `registry` 쪽 405 짝
    /// (`method_mismatch_does_not_consume_count`)의 대칭 자리다. 그쪽이 registry 에
    /// 사는 것은 405 판정이 `match_request` 안에서 끝나기 때문이고, 401 판정은
    /// [`resolve_ack`] 에서 나므로 술어도 여기에 둔다.
    ///
    /// `--count N` 이 세는 단위는 **시퀀스를 돌린 횟수**이고 401 은 그것을 0 번
    /// 돌린다. 그리고 통의 키는 토큰이 아니라 **등록**이라, 차감하면 인증을 통과하지
    /// 못한 익명 발신자가 owner 의 예산을 태운다 — `remaining` 이 작으면 등록 자체가
    /// 사라지고 owner 에게 가는 신호는 없다.
    #[test]
    fn unauthorized_does_not_consume_count() {
        use crate::webhook::auth::{AuthLocation, WebhookAuth};
        use crate::webhook::lifetime::{Lifetime, Limit, Persistence};
        use crate::webhook::registry;

        let id = registry::register(
            vec!["POST".to_string()],
            None,
            vec![],
            Lifetime {
                persistence: Persistence::Temporary,
                limit: Limit::CountLimit { remaining: 2 },
            },
            Some(WebhookAuth {
                location: AuthLocation::QueryKey { key: "t".into() },
                token: "right".into(),
            }),
        )
        .id;

        let mut wrong = BTreeMap::new();
        wrong.insert("t".to_string(), "nope".to_string());
        let (ack, exec) = resolve_ack(&id, "POST", &BTreeMap::new(), &wrong, &Value::Null);

        assert_eq!(ack, AckStatus::Unauthorized);
        assert!(exec.is_none(), "401 은 시퀀스를 넘기지 않는다");

        let (entry, _) = registry::info(&id).expect("401 로 등록이 사라지면 안 된다");
        assert_eq!(
            entry.lifetime.limit,
            Limit::CountLimit { remaining: 2 },
            "401 은 시퀀스를 0 번 돌렸으므로 통을 태우지 않는다"
        );
    }

    #[test]
    fn parse_query_empty() {
        assert!(parse_query("").is_empty());
    }
}

#[cfg(test)]
#[path = "listener_body_tests.rs"]
mod body_tests;
