//! 요청이 상대에게 **요구하는 계약**을 보내기 전에 확인한다.
//!
//! 봉투의 새 필드와 메서드의 새 인자는 그것을 모르는 서버에서 **조용히 버려진다** — 봉투에도
//! 인자 객체에도 모르는 키 거절이 없다. 버려진 요청은 성공으로 돌아오므로 호출자는 계약이
//! 걸린 줄 안다. 그래서 그런 필드·인자를 실은 요청은 `system.info` 의 capability 목록에서
//! 그 이름을 먼저 묻고, 없으면 **요청을 내보내지 않는다.**
//!
//! 무엇을 요구하는지는 명령이 아니라 **요청 자체**에서 판정한다. 같은 판정을 정적 CLI 와
//! plugin CLI 가 함께 쓰려면 둘이 공유하는 것이 요청뿐이다.
//!
//! 근거: `docs/adr/0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md`.

use std::time::{Duration, Instant};

use anyhow::Result;
use tasty_ipc::capability::{RESPONSE_TIMEOUT, RESPONSE_TIMEOUT_VERSION};
use tasty_ipc::client::{
    CapabilityProbeExpired, IpcConnection, JsonRpcCallError, UnsupportedCapability, whole_millis,
};
use tasty_ipc::output_cursor;
use tasty_ipc::protocol::JsonRpcRequest;

/// 단발 요청 봉투에 CLI 가 싣는 값. 루트 플래그에서 온다.
///
/// 봉투 상한은 **요청 하나의 응답 대기**를 자른다. 그래서 요청 하나로 끝나는 명령(정적
/// CLI 의 단발 RPC · plugin CLI 의 단발 요청)만 이것을 싣는다. 루프를 도는 명령(사건
/// 따라가기 · 감사 따라가기 · plugin 의 폴링과 자동 대기)과 스트림·SSH 경유 명령은 싣지
/// 않고, 플래그를 받으면 **거절한다** — 조용히 무시하면 이 모듈이 막으려는 결함을 CLI 가
/// 스스로 만든다. 근거:
/// `docs/adr/0366-the-cli-bounds-a-single-request-wait-with-a-root-flag.md`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    /// `--response-timeout-ms`. `Some(0)` 은 봉투 규약상 "상한 없음" 이라 없는 것과 같다.
    pub response_timeout_ms: Option<u64>,
}

impl Envelope {
    /// 봉투에 싣는다. 0 은 안 싣는다 — 실으면 구 서버에 계약 확인만 붙고 뜻은 같다.
    pub(crate) fn apply(&self, request: &mut JsonRpcRequest) {
        request.response_timeout_ms = self.response_timeout_ms.filter(|&ms| ms > 0);
    }

    /// 이 봉투가 싣는 것이 있는가. 싣는 것이 있는데 그것을 못 싣는 명령이면 거절한다.
    pub(crate) fn is_set(&self) -> bool {
        self.response_timeout_ms.is_some_and(|ms| ms > 0)
    }

    /// 봉투를 못 싣는 명령에 플래그가 왔으면 사용 오류로 끝낸다(종료 코드 2 — clap 의
    /// 인자 오류와 같은 값이다). 통신은 시작 전이다.
    ///
    /// 서브커맨드가 없는 호출(GUI 기동 · augmented help)도 실을 요청이 없으므로 진입점
    /// 라우팅이 같은 자리에서 이것을 부른다 — 그래서 `pub` 이다.
    pub fn refuse_if_set(&self) {
        if self.is_set() {
            crate::out::errln!("{}", tasty_i18n::t("cli.contract.bound_not_applicable"));
            std::process::exit(2);
        }
    }
}

/// 이 요청이 상대에게 요구하는 계약 — `(이름, 최소 판)`.
///
/// 비어 있으면 묻지 않는다. 새 계약을 하나도 안 쓰는 요청은 구 서버에서도 뜻이 같으므로,
/// 물으면 기존 호출에 왕복 하나를 얹을 뿐이다.
pub(crate) fn required(request: &JsonRpcRequest) -> Vec<(&'static str, u32)> {
    let mut out = Vec::new();
    // `Some(0)` 은 봉투 규약상 "상한 없음" 이라 계약을 안 쓴다.
    if request.response_timeout_ms.is_some_and(|ms| ms > 0) {
        out.push((RESPONSE_TIMEOUT, RESPONSE_TIMEOUT_VERSION));
    }
    if output_cursor::requested_by(&request.method, &request.params) {
        out.push((output_cursor::CAPABILITY, output_cursor::VERSION));
    }
    out
}

/// 요구하는 계약을 상대가 선언했는지 확인한다. 하나라도 없으면 [`UnsupportedCapability`]
/// 를 담은 오류로 끝나고, 그때 **요청은 아직 안 나갔다.**
///
/// 확인 자체(`system.info`)가 실패하면 그 오류가 그대로 온다 — 그것은 거절이 아니라 연결의
/// 실패다.
///
/// **요청이 응답 대기 상한을 실었으면 확인도 그 상한 안에 들어간다** — 두 요청이 상한 하나를
/// 나눠 쓴다. 확인 요청은 확인을 시작하는 순간 남은 상한을 기한으로 받고
/// (`IpcConnection::capabilities_within` — 소켓 기한은 그 시간 그대로, 봉투는 밀리초 올림이다),
/// 끝나면 요청의 봉투를 **남은 시간**으로 줄여 싣는다. 그래서 굳은 호스트에서도 CLI 는 상한
/// 뒤에 돌아온다(ADR-0366 이 약속한 것). 확인이 상한 안에 안 끝났거나 남은 시간이 1 ms 도
/// 안 되면 요청은 안 나가고, 그 답은 요청이 큐에서 만료됐을 때와 같은 `-32067`(실행 안 됨)
/// 이다 — 문구도 같다. 근거:
/// `docs/adr/0452-the-cli-capability-check-spends-the-same-response-bound.md`.
pub(crate) fn ensure(conn: &mut IpcConnection, request: &mut JsonRpcRequest) -> Result<()> {
    let needed = required(request);
    if needed.is_empty() {
        return Ok(());
    }
    let bound = request
        .response_timeout_ms
        .filter(|&ms| ms > 0)
        .map(Duration::from_millis);
    let started = Instant::now();
    let left = |bound: Duration| bound.saturating_sub(started.elapsed());
    for (name, version) in needed {
        conn.require_capability_within(
            name,
            version,
            request.session_token.as_deref(),
            bound.map(left),
        )
        .map_err(
            |e| match (bound, e.downcast_ref::<CapabilityProbeExpired>()) {
                (Some(bound), Some(_)) => not_run(bound),
                _ => e,
            },
        )?;
    }
    if let Some(bound) = bound {
        match whole_millis(left(bound)) {
            Some(ms) => request.response_timeout_ms = Some(ms),
            None => return Err(not_run(bound)),
        }
    }
    Ok(())
}

/// [`ensure`] 실패를 기록할 때 **이 요청의** JSON-RPC 코드 칸에 실을 값.
///
/// 확인이 상한 안에 안 끝난 갈래([`not_run`])는 이 요청에 대한 답이다 — 서버가 이 요청을 큐에서
/// 만료시켰을 때와 같은 `-32067` 이므로 같은 코드를 싣는다(같은 사실에는 같은 답, ADR-0452). 그
/// 밖의 실패는 싣지 않는다: 선언 없음 · 전송 실패에는 코드가 없고, 확인 요청 자체가 받은 다른
/// JSON-RPC 오류는 이 요청이 아니라 **확인 요청의** 코드다.
pub(crate) fn failure_code(e: &anyhow::Error) -> Option<i32> {
    e.downcast_ref::<JsonRpcCallError>()
        .map(|rpc| rpc.code)
        .filter(|&code| code == tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN)
}

/// 상한 안에 요청을 못 내보냈다 — 그 요청이 큐에서 만료됐을 때의 답과 **같은 코드·문구**다.
/// 요청은 안 나갔으므로 "실행 안 됨 — 그대로 다시 보내도 된다" 가 참이다.
fn not_run(bound: Duration) -> anyhow::Error {
    let answer = tasty_ipc::server::expired_before_run_response(serde_json::Value::Null, bound);
    let err = answer
        .error
        .expect("expired_before_run_response 는 늘 오류 응답이다");
    JsonRpcCallError {
        code: err.code,
        message: err.message,
        data: err.data,
    }
    .into()
}

/// 거절을 stderr 에 낼 **한 줄 JSON**. 사람이 읽는 문장은 `message` 에 싣고, 호출자가
/// 분기할 값(`kind` · `capability` · `required` · `found` · `sent`)은 따로 싣는다 — 문장을
/// 파싱하게 두면 그 판정이 로케일에 묶인다.
pub(crate) fn refusal_line(refusal: &UnsupportedCapability) -> String {
    let message = match refusal.found {
        Some(found) => tasty_i18n::t_args(
            "cli.contract.unsupported_older",
            &[
                &refusal.name,
                &found.to_string(),
                &refusal.required.to_string(),
            ],
        ),
        None => tasty_i18n::t_args(
            "cli.contract.unsupported_missing",
            &[&refusal.name, &refusal.required.to_string()],
        ),
    };
    serde_json::json!({
        "error": {
            "kind": "unsupported_capability",
            "capability": refusal.name,
            "required": refusal.required,
            "found": refusal.found,
            "sent": false,
            "message": message,
        }
    })
    .to_string()
}

/// [`ensure`] 의 오류를 사용자에게 내고 종료한다. 거절이면 구조화된 한 줄, 그 밖의 실패
/// (확인 요청이 안 닿음 등)는 다른 전송 실패와 같은 모양이다. 종료 코드는 둘 다 1 이다.
pub(crate) fn exit_on_failure(e: anyhow::Error) -> ! {
    match e.downcast_ref::<UnsupportedCapability>() {
        Some(refusal) => crate::out::errln!("{}", refusal_line(refusal)),
        None => crate::rpc_error::exit_with(&e),
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(method: &str, params: serde_json::Value, timeout: Option<u64>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: timeout,
            idempotency_key: None,
        }
    }

    #[test]
    fn a_request_that_uses_no_new_contract_asks_nothing() {
        let r = req(
            "surface.read_since_mark",
            json!({ "surface_id": 1, "strip_ansi": true }),
            None,
        );
        assert!(required(&r).is_empty());
        // `Some(0)` 은 상한 없음 — 계약을 안 쓴다.
        let r = req("workspace.list", json!({}), Some(0));
        assert!(required(&r).is_empty());
    }

    #[test]
    fn a_positive_bound_requires_the_response_timeout_contract() {
        let r = req("workspace.list", json!({}), Some(500));
        assert_eq!(
            required(&r),
            vec![(RESPONSE_TIMEOUT, RESPONSE_TIMEOUT_VERSION)]
        );
    }

    #[test]
    fn a_position_argument_requires_the_output_cursor_contract() {
        let r = req(
            "surface.read_since_mark",
            json!({ "surface_id": 1, "max_bytes": 64 }),
            Some(10),
        );
        assert_eq!(
            required(&r),
            vec![
                (RESPONSE_TIMEOUT, RESPONSE_TIMEOUT_VERSION),
                (output_cursor::CAPABILITY, output_cursor::VERSION),
            ]
        );
    }

    /// 0 은 "상한 없음" 이라 봉투에 안 싣고, 그래서 구 서버에 계약 확인도 안 붙는다.
    #[test]
    fn a_zero_bound_is_not_carried() {
        let mut r = req("workspace.list", json!({}), None);
        let zero = Envelope {
            response_timeout_ms: Some(0),
        };
        zero.apply(&mut r);
        assert_eq!(r.response_timeout_ms, None);
        assert!(!zero.is_set());
        let bound = Envelope {
            response_timeout_ms: Some(250),
        };
        bound.apply(&mut r);
        assert_eq!(r.response_timeout_ms, Some(250));
        assert!(bound.is_set());
        assert!(!Envelope::default().is_set());
    }

    /// 루트 플래그는 서브커맨드 **앞**에 온다(`--port-file` 과 같은 자리).
    #[test]
    fn the_root_flag_parses_before_the_subcommand() {
        use clap::Parser;
        let cli = crate::Cli::try_parse_from([
            "tasty",
            "--response-timeout-ms",
            "500",
            "list",
            "workspaces",
        ])
        .expect("파싱");
        assert_eq!(cli.response_timeout_ms, Some(500));
    }

    fn since_mark(args: &[&str]) -> JsonRpcRequest {
        use clap::Parser;
        let mut argv = vec!["tasty", "read", "since-mark", "--surface", "3"];
        argv.extend_from_slice(args);
        let cli = crate::Cli::try_parse_from(argv).expect("파싱");
        crate::request::command_to_request(&cli.command.expect("명령"))
    }

    /// 위치 인자를 안 준 호출은 **예전과 같은 요청**이다 — 인자 키가 아예 없고, 그래서
    /// 구 서버에 계약 확인도 안 붙는다.
    #[test]
    fn a_plain_since_mark_sends_the_same_params_as_before_and_asks_nothing() {
        let r = since_mark(&["--strip-ansi"]);
        assert_eq!(r.method, "surface.read_since_mark");
        assert_eq!(r.params, json!({ "surface_id": 3, "strip_ansi": true }));
        assert!(required(&r).is_empty());
    }

    #[test]
    fn position_arguments_are_carried_and_require_the_contract() {
        let r = since_mark(&["--cursor", "120", "--stream", "s1", "--max-bytes", "64"]);
        assert_eq!(r.params["cursor"], 120);
        assert_eq!(r.params["stream"], "s1");
        assert_eq!(r.params["max_bytes"], 64);
        assert_eq!(
            required(&r),
            vec![(output_cursor::CAPABILITY, output_cursor::VERSION)]
        );
    }

    /// `--cursor` 만 주면 통신 전에 clap 이 막는다 — 서버가 `cursor_without_stream` 으로
    /// 거절할 조합이다. `--stream` 단독은 서버가 받는 형태라 허용한다.
    #[test]
    fn a_cursor_without_a_stream_is_refused_before_anything_is_sent() {
        use clap::Parser;
        assert!(
            crate::Cli::try_parse_from(["tasty", "read", "since-mark", "--cursor", "1"]).is_err()
        );
        let r = since_mark(&["--stream", "s1"]);
        assert_eq!(r.params["stream"], "s1");
        assert!(r.params.get("cursor").is_none());
    }

    /// 가짜 호스트 — 받은 줄을 돌려주고, `answer` 가 정한 대로 답한다(`None` 이면 답하지 않고
    /// 소켓을 연 채 둔다 — 메인 스레드가 선 호스트이거나, 봉투 상한을 모르는 구 서버다).
    fn fake_host(
        answers: Vec<Option<(std::time::Duration, String)>>,
    ) -> (
        std::net::SocketAddr,
        std::sync::mpsc::Receiver<String>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut writer = stream.try_clone().expect("clone");
            let mut reader = std::io::BufReader::new(stream);
            for answer in answers {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if seen_tx.send(line).is_err() {
                    return;
                }
                match answer {
                    Some((delay, body)) => {
                        std::thread::sleep(delay);
                        if writer.write_all(format!("{body}\n").as_bytes()).is_err() {
                            return;
                        }
                    }
                    // 답하지 않는다 — 상대가 연결을 닫을 때까지 붙들고 있는다.
                    None => {
                        let mut rest = String::new();
                        let _ = reader.read_line(&mut rest); // 상대가 닫으면 0 — 끝낸다
                        return;
                    }
                }
            }
        });
        (addr, seen_rx, h)
    }

    fn capabilities_answer() -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "result": { "capabilities": [ { "name": RESPONSE_TIMEOUT, "version": RESPONSE_TIMEOUT_VERSION } ] }
        })
        .to_string()
    }

    /// `ensure` 를 다른 스레드에서 돌리고 5 s 까지만 기다린다 — 결함이 되살아나면 시험이 매달리지
    /// 않고 빨개진다.
    fn ensure_with_deadline(
        addr: std::net::SocketAddr,
        mut request: JsonRpcRequest,
    ) -> (Result<JsonRpcRequest>, std::time::Duration) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stream = std::net::TcpStream::connect(addr).expect("connect");
            let mut conn = IpcConnection::new(stream).expect("conn");
            let started = Instant::now();
            let r = ensure(&mut conn, &mut request).map(|()| request);
            // 받는 쪽이 5 s 에 떠났으면 그 결과는 버린다 — 시험은 이미 실패로 끝났다.
            let _ = tx.send((r, started.elapsed(), conn));
        });
        let (r, took, conn) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("ensure did not return within 5 s — the bound did not cover the check");
        drop(conn);
        (r, took)
    }

    fn expect_not_run(r: Result<JsonRpcRequest>, bound_ms: u64) {
        let e = r.expect_err("the request must not go out");
        let rpc = e
            .downcast_ref::<JsonRpcCallError>()
            .unwrap_or_else(|| panic!("expected the not-run answer, got {e}"));
        assert_eq!(rpc.code, tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN);
        // 본 요청이 큐에서 만료됐을 때와 같은 문장이다 — 사용자가 준 상한 그대로를 싣는다.
        let same = tasty_ipc::server::expired_before_run_response(
            serde_json::Value::Null,
            Duration::from_millis(bound_ms),
        );
        assert_eq!(rpc.message, same.error.expect("error").message);
    }

    /// 굳은 호스트(또는 봉투 상한을 모르는 구 서버)가 확인 요청에 답하지 않아도 CLI 는 상한 뒤에
    /// 돌아온다. 확인 요청을 상한 없이 기다려 무한정 매달리던 결함의 회귀 시험이다.
    #[test]
    fn a_bound_covers_the_capability_check_when_the_host_does_not_answer() {
        let (addr, seen, h) = fake_host(vec![None]);
        let (r, took) = ensure_with_deadline(addr, req("workspace.list", json!({}), Some(200)));
        expect_not_run(r, 200);
        assert!(took >= Duration::from_millis(190), "{took:?}");
        assert!(took < Duration::from_millis(1500), "{took:?}");
        let probe: serde_json::Value =
            serde_json::from_str(&seen.recv().expect("probe line")).expect("json");
        assert_eq!(probe["method"], "system.info");
        let carried = probe["response_timeout_ms"]
            .as_u64()
            .expect("the probe carries the bound");
        assert!((1..=200).contains(&carried), "{carried}");
        h.join().unwrap();
    }

    /// 상한 1 ms 에서도 확인 요청은 **나간다** — 봉투는 올림이라 1 을 싣는다. 내림이면 확인을
    /// 시작하는 순간 남은 시간(1 ms 미만)이 0 이 되어 아무것도 안 보내고 끝났다. 본 요청은 남은 시간을
    /// 내림으로 싣고 합계가 상한을 넘지 않아야 하므로, 확인 왕복 뒤에는 늘 1 ms 미만이 남아 안 나간다 —
    /// 답은 "실행 안 됨"(ADR-0452).
    #[test]
    fn a_one_millisecond_bound_still_sends_the_check() {
        let (addr, seen, h) = fake_host(vec![Some((Duration::ZERO, capabilities_answer()))]);
        let (r, _) = ensure_with_deadline(addr, req("workspace.list", json!({}), Some(1)));
        let probe: serde_json::Value =
            serde_json::from_str(&seen.recv().expect("the check was sent")).expect("json");
        assert_eq!(probe["method"], "system.info");
        assert_eq!(probe["response_timeout_ms"], 1);
        expect_not_run(r, 1);
        h.join().unwrap();
    }

    /// 봉투 상한을 아는 서버는 확인 요청을 상한에서 물리고 `-32067` 로 답한다 — CLI 는 그 답도
    /// 사용자 상한의 "실행 안 됨" 으로 옮긴다(확인 요청의 문장이 아니라 본 요청의 문장이다).
    #[test]
    fn a_server_that_expires_the_check_is_reported_as_the_request_not_run() {
        let answer = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "error": { "code": tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN, "message": "probe expired" }
        })
        .to_string();
        let (addr, _seen, h) = fake_host(vec![Some((Duration::from_millis(20), answer))]);
        let (r, _) = ensure_with_deadline(addr, req("workspace.list", json!({}), Some(300)));
        expect_not_run(r, 300);
        h.join().unwrap();
    }

    /// 확인이 상한 안에 끝나면 요청은 **남은 시간**을 싣고 나간다 — 두 요청이 상한 하나를 나눠
    /// 쓴다. 그리고 확인이 건 읽기 기한은 풀려 있어야 한다(본 요청은 서버가 상한으로 끊는다).
    #[test]
    fn the_request_carries_what_is_left_after_the_check() {
        let (addr, _seen, h) = fake_host(vec![Some((
            Duration::from_millis(120),
            capabilities_answer(),
        ))]);
        let (r, _) = ensure_with_deadline(addr, req("workspace.list", json!({}), Some(500)));
        let request = r.expect("declared — the check passes");
        let left = request.response_timeout_ms.expect("still bounded");
        assert!(left > 0 && left <= 380, "{left}");
        h.join().unwrap();
    }

    /// 확인 뒤의 연결은 읽기 기한이 없다 — 상한보다 늦게 오는 본 요청의 답도 받는다.
    #[test]
    fn the_check_leaves_no_read_timeout_on_the_connection() {
        let late = json!({ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }).to_string();
        let (addr, _seen, h) = fake_host(vec![
            Some((Duration::from_millis(0), capabilities_answer())),
            Some((Duration::from_millis(400), late)),
        ]);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stream = std::net::TcpStream::connect(addr).expect("connect");
            let mut conn = IpcConnection::new(stream).expect("conn");
            let mut r = req("workspace.list", json!({}), Some(150));
            ensure(&mut conn, &mut r).expect("declared");
            // 받는 쪽이 5 s 에 떠났으면 그 결과는 버린다 — 시험은 이미 실패로 끝났다.
            let _ = tx.send(conn.send(&r).map_err(|e| e.to_string()));
        });
        let answer = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("send returned");
        assert_eq!(answer, Ok(json!({ "ok": true })));
        h.join().unwrap();
    }

    /// 상한이 없으면 확인 요청도 종전 그대로다 — 봉투에 아무것도 안 싣는다.
    #[test]
    fn without_a_bound_the_check_is_sent_as_before() {
        let (addr, seen, h) = fake_host(vec![Some((
            Duration::from_millis(0),
            capabilities_answer(),
        ))]);
        let mut r = req(
            "surface.read_since_mark",
            json!({ "surface_id": 1, "max_bytes": 64 }),
            None,
        );
        let stream = std::net::TcpStream::connect(addr).expect("connect");
        let mut conn = IpcConnection::new(stream).expect("conn");
        // output-cursor 는 선언 안 됐으니 거절이지만, 확인 요청은 나갔다.
        assert!(ensure(&mut conn, &mut r).is_err());
        let probe: serde_json::Value =
            serde_json::from_str(&seen.recv().expect("probe")).expect("json");
        assert!(probe.get("response_timeout_ms").is_none(), "{probe}");
        assert_eq!(r.response_timeout_ms, None);
        drop(conn);
        h.join().unwrap();
    }

    /// 기록의 코드 칸 — 확인 만료는 이 요청의 `-32067` 이고, 확인 요청 자신의 다른 오류 코드와 선언
    /// 없음은 싣지 않는다.
    #[test]
    fn only_the_not_run_answer_fills_the_recorded_code() {
        assert_eq!(
            failure_code(&not_run(Duration::from_millis(300))),
            Some(tasty_ipc::protocol::ERR_EXPIRED_BEFORE_RUN)
        );
        let probe_error: anyhow::Error = JsonRpcCallError {
            code: -32001,
            message: "permission denied".into(),
            data: None,
        }
        .into();
        assert_eq!(failure_code(&probe_error), None);
        let refusal: anyhow::Error = UnsupportedCapability {
            name: RESPONSE_TIMEOUT.into(),
            required: 1,
            found: None,
        }
        .into();
        assert_eq!(failure_code(&refusal), None);
    }

    /// 거절 줄은 **한 줄 JSON** 이고, 호출자가 분기할 값을 문장과 따로 싣는다. 특히
    /// `sent:false` — 호스트가 답한 실패와 이 거절을 가르는 것이 그 값이다.
    #[test]
    fn the_refusal_is_one_json_line_that_says_nothing_was_sent() {
        let missing = UnsupportedCapability {
            name: "ipc.output-cursor".into(),
            required: 1,
            found: None,
        };
        let line = refusal_line(&missing);
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        let e = &v["error"];
        assert_eq!(e["kind"], "unsupported_capability");
        assert_eq!(e["capability"], "ipc.output-cursor");
        assert_eq!(e["required"], 1);
        assert!(e["found"].is_null(), "없는 것과 낮은 판은 다른 값이다");
        assert_eq!(e["sent"], false);
        assert!(e["message"].is_string());

        let older = UnsupportedCapability {
            name: "x".into(),
            required: 2,
            found: Some(1),
        };
        let v: serde_json::Value = serde_json::from_str(&refusal_line(&older)).expect("JSON");
        assert_eq!(v["error"]["found"], 1);
    }
}
