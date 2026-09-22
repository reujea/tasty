//! Plugin CLI fallback + client mode runner.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::Result;

use super::Commands;
use super::contract::Envelope;
use super::dispatch::{ClientCtx, Dispatch};
use super::dynamic;
use super::format::format_output;
use super::hook_failure;
use super::request::command_to_request;
use crate::out::outln;
use tasty_ipc::client::IpcConnection;

/// IPC connect 상한. 목적지가 항상 `127.0.0.1` 이라 미리스닝 포트는 즉시 RST 로
/// 거부되므로 평시엔 이 값에 닿지 않는다 — 로컬 방화벽 DROP 처럼 RST 가 돌아오지
/// 않는 상황에서 OS 기본 타임아웃(수십 초~분)까지 매달리는 것을 막는 보험이다.
/// hook 은 에이전트 턴 경계에서 **동기** 실행되므로, 여기서 블록되면 상태 push 가
/// 늦는 데 그치지 않고 턴 자체가 멈춘다. read/write 쪽 선례는
/// `remote_browse.rs` 의 `PROBE_TIMEOUT`.
const IPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// 연결 실패 — **두 개의 문구를 함께 나른다.**
///
/// 사용자에게 보이는 stderr 는 번역문이어야 하고, `hook-failures.log` 에 남는 진단은
/// 로케일 무관 영어여야 한다(`hook_failure` 모듈 참고). 포트 파일 쪽은 `PortFileError` 의
/// `Display` 가 이미 영어 원본을 들고 있었지만 이 경로에는 대응물이 없어, 번역문이
/// 그대로 로그에 실렸다. 그래서 여기서 원본을 만든다.
pub(crate) struct ConnectFailure {
    port: u16,
    source: std::io::Error,
}

impl ConnectFailure {
    /// 사용자 표시용 — 현재 로케일.
    fn localized(&self) -> String {
        tasty_i18n::t_fmt2(
            "cli.request.connect_failed",
            &self.port.to_string(),
            &self.source.to_string(),
        )
    }

    /// 진단용 — 로케일 무관 영어. `lang/en.toml` 의 같은 키와 문자 단위로 같아야 하며
    /// 아래 테스트가 그것을 강제한다(포트 파일 쪽 선례와 같은 형태).
    pub(crate) fn diagnostic(&self) -> crate::hook_failure::DiagnosticEnglish {
        crate::hook_failure::DiagnosticEnglish::new_unchecked(format!(
            "Could not connect to tasty instance on port {}: {}. Is tasty running?",
            self.port, self.source
        ))
    }
}

impl From<ConnectFailure> for anyhow::Error {
    fn from(e: ConnectFailure) -> Self {
        anyhow::anyhow!("{}", e.localized())
    }
}

/// loopback IPC 포트에 상한을 걸고 연결한다. 실패 메시지는 `cli.request.connect_failed`
/// 한 키를 모든 연결 지점(plugin audit-follow / debug stream-echo / attach)과 공유한다.
fn connect_ipc(port: u16) -> std::result::Result<TcpStream, ConnectFailure> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, IPC_CONNECT_TIMEOUT)
        .map_err(|source| ConnectFailure { port, source })
}

pub fn try_run_plugin_cli() -> Option<Result<()>> {
    let plugins_root = tasty_host_plugin::plugin_root()?;
    let entries = dynamic::discover_plugin_clis(&plugins_root);
    if entries.is_empty() {
        return None;
    }
    // 사용자가 입력한 첫 인자가 plugin command 이름인지 확인. plugin 명령이 맞다면
    // clap 에러도 자체 출력으로 처리한다 (정적 CLI의 "unrecognized subcommand"가
    // 대신 뜨면 안 됨).
    let first_arg = std::env::args().nth(1);
    let is_plugin_cmd = first_arg
        .as_deref()
        .map(|name| entries.iter().any(|e| e.cli.name == name))
        .unwrap_or(false);
    let augmented = dynamic::build_augmented_cli(&entries);
    let matches = match augmented.try_get_matches() {
        Ok(m) => m,
        Err(err) => {
            if is_plugin_cmd {
                crate::help::format_parse_error(err);
                unreachable!();
            }
            return None;
        }
    };
    // 루트 `--port-file` 플래그는 augmented(Cli 기반)에 그대로 포함됨. 추출해 dynamic 경로로 전달.
    let port_file = matches.get_one::<String>("port_file").cloned();
    // 루트 `--response-timeout-ms` 도 같다. 단발 요청만 싣고, 폴링·자동 대기는 거절한다.
    let envelope = Envelope {
        response_timeout_ms: matches.get_one::<u64>("response_timeout_ms").copied(),
    };
    let (top_name, _) = matches.subcommand()?;
    if !entries.iter().any(|e| e.cli.name == top_name) {
        return None;
    }
    let (mut request, polling, auto_wait) = match dynamic::matches_to_request(&entries, &matches) {
        Ok(r) => r,
        Err(e) => return Some(Err(e)),
    };
    if polling.is_some() || auto_wait.is_some() {
        envelope.refuse_if_set();
    }
    envelope.apply(&mut request);
    // stdout 파이프 조기 종료(EPIPE)는 조용한 종료 코드 0 — ADR-0101.
    Some(crate::out::quiet_if_stdout_closed(
        match (polling, auto_wait) {
            (Some(p), _) => run_dynamic_client_polling(request, p, port_file.as_deref()),
            (None, Some(aw)) => {
                run_dynamic_client_with_auto_wait(request, aw, port_file.as_deref())
            }
            (None, None) => run_dynamic_client(request, port_file.as_deref()),
        },
    ))
}

/// plugin CLI 단발 요청. **agent hook 이 타는 유일한 경로**다(static CLI 는 plugin
/// 명령이 아니다) — 그래서 실패 기록([`crate::hook_failure`])을 여기에만 건다.
///
/// 그 "유일" 을 지키는 것은 이 파일이 아니라 **매니페스트**다. hook 서브커맨드가
/// `polling` 이나 `auto_wait` 를 선언하면 그 명령은 아래 두 디스패치
/// ([`run_dynamic_client_polling`] · [`run_dynamic_client_with_auto_wait`])로 새고,
/// 거기에는 기록이 없어 그 hook 의 실패는 아무 데도 안 남는다 — 빌드도 테스트도
/// 초록인 채로. 그 전제를 재는 자리는
/// `tests/hook_commands_stay_on_the_recording_path.rs` 다.
///
/// **전달이 실패로 끝나는 갈래를 하나도 빼지 않고 기록한다**: 포트 파일 부재
/// (=tasty 미실행, 실사용에서 가장 흔한 원인) / connect 실패 / 연결 준비 실패 /
/// JSON-RPC 에러, 그리고 응답은 왔지만 호스트 호출이 조용히 실패한 갈래. 셸 래퍼가
/// exit code 를 버리므로, 기록하지 않으면 그중 무엇이 일어났는지 사후에 알 방법이 없다.
///
/// 기록하지 **않는** 갈래는 하나다 — 응답을 받아 놓고 stdout 쓰기가 실패하는 것
/// (`outln!`). 그것은 전달 실패가 아니라 출력 실패이고, 파이프 조기 종료는 ADR-0101 이
/// 조용한 종료 코드 0 으로 접는다.
fn run_dynamic_client(
    mut request: tasty_ipc::protocol::JsonRpcRequest,
    port_file: Option<&str>,
) -> Result<()> {
    // CLI 가 문구를 만드는 갈래는 모두 **로그에는 영어, stderr 에는 번역문**을 낸다.
    // `record` 가 `DiagnosticEnglish` 만 받으므로 그 분리는 타입이 지킨다.
    let port = match crate::port_file::read_port_diagnosed(port_file) {
        Ok(p) => p,
        Err(e) => {
            // `PortFileError` 의 `Display` 가 영어 원본이다. 사용자에게 낼 번역문은
            // `read_port` 와 같은 지점(`port_file::localize`)이 만든다.
            hook_failure::record(
                &request.method,
                &request.params,
                None, // 호스트에 닿지도 못했다 — JSON-RPC 코드가 없다
                &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
            );
            return Err(anyhow::anyhow!("{}", crate::port_file::localize(&e)));
        }
    };
    let stream = match connect_ipc(port) {
        Ok(s) => s,
        Err(e) => {
            hook_failure::record(&request.method, &request.params, None, &e.diagnostic());
            return Err(e.into());
        }
    };
    let mut conn = match IpcConnection::new(stream) {
        Ok(c) => c,
        Err(e) => {
            // 연결은 됐는데 CLI 쪽 준비(스트림 복제)가 실패한 갈래. 요청은 한 바이트도
            // 안 나갔으므로 **전달 실패**이고, 앞의 두 갈래와 같은 이유로 흔적이 여기
            // 말고는 없다.
            //
            // **기록할 곳이 없어서 안 하는 것이 아니다.** 기록은 IPC 가 아니라
            // `tasty_home()` 아래 파일이라 상대에 닿는 것과 무관하다 — 닿지도 못한
            // 앞의 두 갈래가 이미 그렇게 남긴다.
            //
            // 다만 이 갈래의 흔한 원인(fd 고갈)은 로그 파일을 여는 것도 같이 막는다.
            // `record` 는 best-effort 라 그때는 조용히 아무것도 안 남는다 — 그래도
            // 거는 이유는 원인이 그것 하나가 아니고, 안 걸면 남을 확률이 0 이기 때문이다.
            //
            // 문구는 `io::Error` 의 `Display` 다. Rust 는 `setlocale` 을 부르지 않으므로
            // libc 가 로케일과 무관한 영어를 돌려준다 — `new_unchecked` 의 보증을
            // 호출자가 지는 자리이고, 그 근거가 이것이다.
            hook_failure::record(
                &request.method,
                &request.params,
                None, // JSON-RPC 응답을 받은 적이 없다 — 코드가 없다
                &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
            );
            return Err(e);
        }
    };
    // 새 계약을 쓰는 요청이면 상대의 선언을 먼저 묻는다. 여기서 끝나는 갈래는 어느 것이든
    // 이 요청이 안 나간 **전달 실패**라 앞 갈래들과 같은 이유로 기록한다. 문구의 출처는
    // 갈래마다 다르다:
    //
    // - 선언이 없다(`UnsupportedCapability`) — `Display` 가 CLI 가 쥔 **영어 원본**이다.
    // - 확인 요청(`system.info`)의 전송·EOF 실패 — `io::Error` 와 `IpcConnection::send` 의
    //   고정 영어 문구다.
    // - 확인이 요청의 응답 대기 상한 안에 안 끝났다 — CLI 가 만든 `-32067` 문장이다(본 요청이
    //   큐에서 만료됐을 때 호스트가 쓰는 영어 문장과 같은 함수로 만든다). 이 갈래만 `code` 칸에
    //   `-32067` 을 싣는다 — 이 요청에 대한 답이고, 서버가 이 요청을 큐에서 만료시킨 아래 `conn.send`
    //   갈래가 싣는 값과 같다(`contract::failure_code`).
    // - 확인 요청이 JSON-RPC 오류로 끝났다 — 문구는 **답한 서버가 만든 문장**이라 CLI 에 영어
    //   원본이 없다. 아래 `conn.send` 실패 갈래와 같은 처지이고 같은 방식으로 적는다 —
    //   `new_unchecked` 의 보증이 이 갈래에서는 서지 않는다. 다만 그 코드는 이 요청이 아니라
    //   확인 요청의 것이라 `code` 칸에 싣지 않는다.
    if let Err(e) = super::contract::ensure(&mut conn, &mut request) {
        hook_failure::record(
            &request.method,
            &request.params,
            // 이 요청은 안 나갔다 — 확인 만료(이 요청의 -32067)만 코드가 있다
            super::contract::failure_code(&e),
            &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
        );
        super::contract::exit_on_failure(e);
    }
    match conn.send(&request) {
        Ok(value) => {
            // **성공 응답도 실패를 담을 수 있다.** 최선노력 host 호출을 가진 훅 핸들러는
            // 조용히 실패한 호출 수를 `host_call_failures` 로 싣는다
            // (docs/dev-guide/error-handling.md "최선노력의 대가는 치르되 값으로 노출한다").
            // 그 수는 응답에 있으므로 여기 stdout 으로도 나가지만, 실사용에서 이 CLI 는
            // 훅 명령 안에서 돌고 그 명령은 출력을 버린다 — 이 파일이 유일한 흔적이 되는
            // 이유가 앞의 네 갈래와 같다.
            if let Some(failures) = value
                .get("host_call_failures")
                .and_then(serde_json::Value::as_u64)
                && failures > 0
            {
                hook_failure::record(
                    &request.method,
                    &request.params,
                    None, // 호스트는 답했다 — JSON-RPC 에러가 아니다
                    &hook_failure::DiagnosticEnglish::new_unchecked(format!(
                        "response ok but {failures} host call(s) failed silently"
                    )),
                );
            }
            outln!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            )?;
            Ok(())
        }
        Err(e) => {
            // **이 갈래만 문구를 CLI 가 만들지 않는다.** 앞의 둘은 CLI 가 영어 원본을
            // 쥐고 있어 진단과 표시를 갈라 놓을 수 있지만, 여기 `message` 는 답한 쪽이
            // 만들어 보낸 것이라 CLI 에 영어 원본이 없다 — 그리고 plugin 이 답하면 그
            // 문구는 앱 언어를 탄다(`claude.hook`·`codex.hook`). 그래서 로케일 무관성은
            // 산문이 아니라 **코드 필드**가 진다(`code=`). 코드는 프로토콜 값이라
            // 안 흔들리고, 이제 산문을 파싱하지 않아도 꺼낼 수 있다.
            let code = e
                .downcast_ref::<tasty_ipc::client::JsonRpcCallError>()
                .map(|err| err.code);
            hook_failure::record(
                &request.method,
                &request.params,
                code,
                &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
            );
            crate::rpc_error::exit_with(&e);
        }
    }
}

/// auto-wait chain / polling 의 각 응답을 **line-delimited(compact 한 줄)** JSON 으로
/// 직렬화한다. pretty(여러 줄)로 내면 한 프로세스 stdout 에 두 응답을 합칠 때 물리적
/// 라인 경계가 응답 경계와 어긋나 "마지막 line 만 파싱" 계약이 깨진다 — 그래서 compact
/// 고정. serde_json compact 는 중첩 값에도 개행을 넣지 않으므로 emit 당 정확히 1 줄.
fn line_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// 요청 params 에서 `request::CLI_WARNINGS_PARAMS_KEY` 예약 키를 떼어낸다 — 서버는
/// 이 키를 모르므로 전송 전에 제거하고, 값은 응답 출력 시 병합할 수 있게 반환한다.
fn take_cli_warnings(request: &mut tasty_ipc::protocol::JsonRpcRequest) -> Vec<String> {
    let Some(obj) = request.params.as_object_mut() else {
        return Vec::new();
    };
    obj.remove(super::request::CLI_WARNINGS_PARAMS_KEY)
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// client-local 경고를 응답 JSON 의 top-level `warnings` 필드로 병합한다. 응답이
/// object 가 아니거나 경고가 없으면 아무것도 하지 않는다.
fn merge_cli_warnings(value: &mut serde_json::Value, warnings: Vec<String>) {
    if warnings.is_empty() {
        return;
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "warnings".to_string(),
            serde_json::Value::Array(
                warnings
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
}

/// `claude spawn` / `claude tell` / `codex spawn` / `codex tell` 같이 manifest 가
/// `auto_wait` 를 선언한 명령. 1 차 IPC 응답을 line-delimited JSON 으로 출력한 뒤,
/// `--no-wait` 가 아니면 wait IPC 를 chain 호출해 terminal_states 도달까지 block —
/// wait 응답도 두 번째 JSON line 으로 출력. caller 는 마지막 line 만 파싱하면
/// wait 결과를 확보할 수 있다.
fn run_dynamic_client_with_auto_wait(
    request: tasty_ipc::protocol::JsonRpcRequest,
    aw: super::dynamic::AutoWaitPlan,
    port_file: Option<&str>,
) -> Result<()> {
    let port = crate::port_file::read_port(port_file)?;

    // ── 1) 1 차 IPC (spawn / tell) 호출 + 응답 출력.
    let first_value = {
        let stream = connect_ipc(port)?;
        let mut conn = IpcConnection::new(stream)?;
        match conn.send(&request) {
            Ok(value) => value,
            Err(e) => {
                crate::rpc_error::exit_with(&e);
            }
        }
    };
    outln!("{}", line_json(&first_value))?;

    // ── 2) --no-wait 이면 여기서 종료.
    if aw.skipped {
        return Ok(());
    }

    // ── 3) wait params 구성 + 4) wait IPC chain. polling sense 그대로 재사용.
    let wait_params = build_wait_params(&aw, &first_value);
    let wait_req = tasty_ipc::protocol::JsonRpcRequest {
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".into(),
        method: aw.method.clone(),
        params: serde_json::Value::Object(wait_params),
        id: Some(serde_json::Value::from(2)),
        session_token: request.session_token.clone(),
    };
    run_dynamic_client_polling(wait_req, aw.polling, port_file)
}

/// auto-wait chain 의 wait IPC 요청 params 를 빌드. 1 차 응답 → 1 차 요청 →
/// timeout 키 순으로 채우고 마지막에 `surface` ↔ `surface_id` 양방향 alias 를 보강.
///
/// 우선순위:
/// 1. `map_from_response` — 1 차 응답에서 값을 꺼내 wait params 키로 복사.
/// 2. `map_from_request` — 1 차 요청 params 에서 fallback (응답 매핑이 이미 채운
///    키는 건드리지 않음 — response 가 우선).
/// 3. timeout — 1 차 요청 params 의 `aw.timeout_field` 값을 wait params 의
///    `aw.polling.timeout_field` 키 (없으면 `"timeout"`) 로 복사.
/// 4. surface alias — 두 키 중 하나만 채워졌을 때 다른 키도 같은 값으로 보강.
///    wait IPC handler 가 `surface` 또는 `surface_id` 둘 중 어느 키를 기대하든
///    manifest 작성자가 알 수 없으므로 호환 안전망.
pub(crate) fn build_wait_params(
    aw: &super::dynamic::AutoWaitPlan,
    first_value: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let mut wait_params = serde_json::Map::new();
    for (resp_key, target_key) in &aw.map_from_response {
        if let Some(v) = first_value.get(resp_key) {
            wait_params.insert(target_key.clone(), v.clone());
        }
    }
    for (req_key, target_key) in &aw.map_from_request {
        if !wait_params.contains_key(target_key)
            && let Some(v) = aw.request_params.get(req_key)
        {
            wait_params.insert(target_key.clone(), v.clone());
        }
    }
    let wait_timeout_key = aw
        .polling
        .timeout_field
        .clone()
        .unwrap_or_else(|| "timeout".into());
    if let Some(t) = aw.request_params.get(&aw.timeout_field) {
        wait_params.insert(wait_timeout_key, t.clone());
    }
    if let Some(v) = wait_params.get("surface").cloned() {
        wait_params.entry("surface_id".to_string()).or_insert(v);
    }
    if let Some(v) = wait_params.get("surface_id").cloned() {
        wait_params.entry("surface".to_string()).or_insert(v);
    }
    wait_params
}

/// `tasty claude wait` 같이 manifest 가 polling 을 선언한 명령. 호스트에
/// 반복 IPC 호출 + state 확인 + terminal_states 도달 또는 timeout 까지 block.
/// timeout 도달 시 마지막 응답을 그대로 출력 (caller 가 state 보고 판단).
fn run_dynamic_client_polling(
    request: tasty_ipc::protocol::JsonRpcRequest,
    polling: tasty_plugin_manifest::PollingDecl,
    port_file: Option<&str>,
) -> Result<()> {
    use std::time::{Duration, Instant};

    let port = crate::port_file::read_port(port_file)?;
    let interval = Duration::from_millis(polling.interval_ms);
    // timeout_field 가 manifest 에 선언되어 있으면 request.params 에서 그 값 (초)
    // 을 deadline 으로 사용. 없으면 무한 대기.
    let deadline = polling.timeout_field.as_ref().and_then(|field| {
        request
            .params
            .get(field)
            .and_then(|v| v.as_u64())
            .map(|secs| Instant::now() + Duration::from_secs(secs))
    });

    // 첫 None 할당은 loop body 의 `last_response = Some(value);` 가 항상 덮어쓰므로
    // dead store 이지만, deadline 분기서 읽으려면 mutable 변수 선언이 필요. suppress.
    #[allow(unused_assignments)]
    let mut last_response: Option<serde_json::Value> = None;
    loop {
        let stream = connect_ipc(port)?;
        let mut conn = IpcConnection::new(stream)?;
        match conn.send(&request) {
            Ok(value) => {
                let reached = value
                    .get(&polling.state_field)
                    .and_then(|v| v.as_str())
                    .map(|s| polling.terminal_states.iter().any(|t| t == s))
                    .unwrap_or(false);
                if reached {
                    outln!("{}", line_json(&value))?;
                    return Ok(());
                }
                last_response = Some(value);
            }
            Err(e) => {
                // IPC 자체 에러는 polling 의미 없음 — 그대로 종료.
                crate::rpc_error::exit_with(&e);
            }
        }
        if let Some(d) = deadline
            && Instant::now() >= d
        {
            // timeout — 마지막 응답을 그대로 출력. terminal 아님을 caller 가 판단.
            if let Some(v) = last_response {
                outln!("{}", line_json(&v))?;
            }
            return Ok(());
        }
        std::thread::sleep(interval);
    }
}

/// Run the CLI client: connect to a running tasty instance and execute the command.
///
/// stdout 이 파이프 조기 종료(EPIPE)로 닫히면 조용히 `Ok(())` — 종료 코드 0(ADR-0101).
/// 출력 경로 전체가 [`crate::out`] 을 거치므로 `StdoutClosed` 가 여기까지 `?` 로 올라온다.
pub fn run_client(command: Commands, port_file: Option<&str>) -> Result<()> {
    run_client_with(command, port_file, Envelope::default())
}

/// [`run_client`] 에 루트 플래그가 정한 봉투 값을 더한 판. 진입점(`boot`)이 부른다.
pub fn run_client_with(
    command: Commands,
    port_file: Option<&str>,
    envelope: Envelope,
) -> Result<()> {
    crate::out::quiet_if_stdout_closed(run_client_inner(command, port_file, envelope))
}

fn run_client_inner(command: Commands, port_file: Option<&str>, envelope: Envelope) -> Result<()> {
    // 갈래는 `dispatch` 가 정한다 — 클라이언트 주도 실행이면 그쪽으로 넘기고,
    // 아니면 아래 단발 JSON-RPC 경로를 탄다. 새 로컬 명령을 추가할 때 이 함수를
    // 고칠 필요는 없다(`dispatch::classify` 만 손댄다).
    if let Dispatch::ClientDriven(cmd) = command.dispatch()? {
        // 클라이언트 주도 명령은 요청을 여럿 보내거나 IPC 를 안 탄다 — 봉투 상한을 실을
        // 요청 하나가 없다. 받으면 조용히 버리지 않고 거절한다.
        envelope.refuse_if_set();
        return cmd.run(&ClientCtx { port_file });
    }

    let port = crate::port_file::read_port(port_file)?;
    let stream = connect_ipc(port)?;

    let mut conn = IpcConnection::new(stream)?;

    let mut request = command_to_request(&command);
    envelope.apply(&mut request);
    let cli_warnings = take_cli_warnings(&mut request);
    // 새 계약을 쓰는 요청은 상대가 그것을 선언했는지 **보내기 전에** 묻는다 — 모르는
    // 서버는 그 필드를 조용히 버리고 성공으로 답한다(`contract` 모듈).
    if let Err(e) = super::contract::ensure(&mut conn, &mut request) {
        super::contract::exit_on_failure(e);
    }
    let result = conn.send(&request);

    match result {
        Ok(mut value) => {
            merge_cli_warnings(&mut value, cli_warnings);
            format_output(&command, &value)?;
        }
        Err(e) => {
            crate::rpc_error::exit_with(&e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic::AutoWaitPlan;
    use serde_json::{Map, Value, json};
    use std::collections::HashMap;
    use tasty_plugin_manifest::PollingDecl;

    fn sample_plan(
        map_from_response: HashMap<String, String>,
        map_from_request: HashMap<String, String>,
        request_params: Map<String, Value>,
    ) -> AutoWaitPlan {
        AutoWaitPlan {
            method: "x.wait".into(),
            polling: PollingDecl {
                state_field: "state".into(),
                terminal_states: vec!["idle".into()],
                interval_ms: 100,
                timeout_field: Some("timeout".into()),
            },
            map_from_response,
            map_from_request,
            timeout_field: "timeout".into(),
            request_params,
            skipped: false,
        }
    }

    #[test]
    fn line_json_is_single_physical_line() {
        // 프레이밍 계약: auto-wait chain / polling 의 각 emit 은 정확히 1 물리 라인이어야
        // "마지막 line 파싱"이 성립한다. 중첩 객체/배열이 들어와도 compact 직렬화는
        // 개행을 넣지 않으므로 두 응답(spawn + wait)을 이어붙여도 라인 경계 = 응답 경계.
        let spawn_resp = json!({
            "child_index": 3,
            "child_surface_id": 42,
            "parent_surface_id": 11,
            "nested": { "pane_id": 7, "list": [1, 2, 3] }
        });
        let wait_resp = json!({ "state": "idle", "meta": { "a": [true, false] } });
        let spawn_line = line_json(&spawn_resp);
        let wait_line = line_json(&wait_resp);
        assert!(!spawn_line.contains('\n'), "spawn emit must be single-line");
        assert!(!wait_line.contains('\n'), "wait emit must be single-line");
        // 두 emit 을 개행으로 이어붙인 stdout(경로 A) 은 정확히 2 물리 라인이고,
        // 마지막 라인은 그 자체로 유효한 wait JSON 이다.
        let combined = format!("{spawn_line}\n{wait_line}");
        let lines: Vec<&str> = combined.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed_last: Value = serde_json::from_str(lines[1]).expect("last line valid JSON");
        assert_eq!(parsed_last.get("state"), Some(&Value::from("idle")));
        let parsed_first: Value = serde_json::from_str(lines[0]).expect("first line valid JSON");
        assert_eq!(parsed_first.get("child_index"), Some(&Value::from(3)));
    }

    #[test]
    fn auto_wait_maps_from_response() {
        // 1 차 응답 키가 map_from_response 매핑에 따라 wait params 로 복사된다.
        let mut mfr = HashMap::new();
        mfr.insert("child_index".into(), "child".into());
        mfr.insert("parent_surface_id".into(), "surface".into());
        let plan = sample_plan(mfr, HashMap::new(), Map::new());
        let resp = json!({ "child_index": 3, "parent_surface_id": 11 });
        let p = build_wait_params(&plan, &resp);
        assert_eq!(p.get("child"), Some(&Value::from(3)));
        // surface ↔ surface_id alias 가 양쪽 다 채워짐.
        assert_eq!(p.get("surface"), Some(&Value::from(11)));
        assert_eq!(p.get("surface_id"), Some(&Value::from(11)));
    }

    #[test]
    fn auto_wait_maps_from_request_fallback() {
        // 응답에 키가 없을 때 1 차 요청 params 에서 채워온다.
        let mut mfreq = HashMap::new();
        mfreq.insert("surface".into(), "surface".into());
        let mut params = Map::new();
        params.insert("surface".into(), Value::from(42_u32));
        let plan = sample_plan(HashMap::new(), mfreq, params);
        let resp = json!({});
        let p = build_wait_params(&plan, &resp);
        assert_eq!(p.get("surface"), Some(&Value::from(42_u32)));
        assert_eq!(p.get("surface_id"), Some(&Value::from(42_u32)));
    }

    #[test]
    fn auto_wait_both_mappings_response_wins() {
        // response 와 request 둘 다 동일 target_key 로 매핑되어 있으면 response 우선.
        let mut mfr = HashMap::new();
        mfr.insert("surface".into(), "surface".into());
        let mut mfreq = HashMap::new();
        mfreq.insert("surface".into(), "surface".into());
        let mut params = Map::new();
        params.insert("surface".into(), Value::from(1_u32));
        let plan = sample_plan(mfr, mfreq, params);
        let resp = json!({ "surface": 9_u32 });
        let p = build_wait_params(&plan, &resp);
        assert_eq!(
            p.get("surface"),
            Some(&Value::from(9_u32)),
            "response value should win over request fallback"
        );
    }

    #[test]
    fn auto_wait_surface_id_aliased_from_surface() {
        // 응답 매핑이 "surface" 키만 채워도 wait IPC handler 가 surface_id 키를 보면
        // 받아서 동작하도록 양방향 alias.
        let mut mfr = HashMap::new();
        mfr.insert("parent".into(), "surface".into());
        let plan = sample_plan(mfr, HashMap::new(), Map::new());
        let resp = json!({ "parent": 7_u32 });
        let p = build_wait_params(&plan, &resp);
        assert_eq!(p.get("surface"), Some(&Value::from(7_u32)));
        assert_eq!(p.get("surface_id"), Some(&Value::from(7_u32)));
    }

    #[test]
    fn auto_wait_surface_aliased_from_surface_id() {
        // 반대 방향: surface_id 만 채워졌어도 surface 키도 동일 값으로 채운다.
        let mut mfr = HashMap::new();
        mfr.insert("sid".into(), "surface_id".into());
        let plan = sample_plan(mfr, HashMap::new(), Map::new());
        let resp = json!({ "sid": 5_u32 });
        let p = build_wait_params(&plan, &resp);
        assert_eq!(p.get("surface"), Some(&Value::from(5_u32)));
        assert_eq!(p.get("surface_id"), Some(&Value::from(5_u32)));
    }

    #[test]
    fn auto_wait_timeout_field_copied() {
        // 1 차 요청의 timeout 값이 wait params 의 polling.timeout_field 키로 복사.
        let mut params = Map::new();
        params.insert("timeout".into(), Value::from(30_u32));
        let plan = sample_plan(HashMap::new(), HashMap::new(), params);
        let p = build_wait_params(&plan, &json!({}));
        assert_eq!(p.get("timeout"), Some(&Value::from(30_u32)));
    }

    #[test]
    fn auto_wait_timeout_renamed_via_polling_timeout_field() {
        // polling.timeout_field 가 "deadline" 이면 wait params 의 키도 "deadline".
        // (manifest 작성자가 wait handler 의 키 이름을 다른 이름으로 둘 수 있음.)
        let mut params = Map::new();
        params.insert("timeout".into(), Value::from(45_u32));
        let mut plan = sample_plan(HashMap::new(), HashMap::new(), params);
        plan.polling.timeout_field = Some("deadline".into());
        let p = build_wait_params(&plan, &json!({}));
        assert_eq!(p.get("deadline"), Some(&Value::from(45_u32)));
        // 원 키는 채우지 않음.
        assert!(p.get("timeout").is_none());
    }

    #[test]
    fn auto_wait_timeout_absent_no_copy() {
        // 1 차 요청에 timeout 키가 없으면 wait params 에도 timeout 키가 들어가지 않음
        // (= 무한 대기).
        let plan = sample_plan(HashMap::new(), HashMap::new(), Map::new());
        let p = build_wait_params(&plan, &json!({}));
        assert!(p.get("timeout").is_none());
    }
}

#[cfg(test)]
mod language_split_tests {
    use super::*;

    fn failure(port: u16) -> ConnectFailure {
        ConnectFailure {
            port,
            source: std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "Connection refused",
            ),
        }
    }

    /// en 로케일에서는 번역을 거친 문구가 코드에 박은 영어 진단문과 **같아야** 한다.
    ///
    /// 포트 파일 쪽(`port_file.rs`)과 같은 형태의 강제다. 두 값이 갈리면 같은 실패가
    /// 경로에 따라 다른 영어 문장으로 나오고, `hook-failures.log` 를 알려진 패턴과
    /// 대조하는 쪽이 그 차이에 걸린다.
    #[test]
    fn english_lang_value_matches_the_diagnostic_rendering() {
        tasty_i18n::init("en");
        let f = failure(59999);
        assert_eq!(f.localized(), f.diagnostic().as_str());
    }

    /// 진단문은 **i18n 을 거치지 않는다** — 프로세스 로케일이 무엇이든 같은 문자열이다.
    ///
    /// `tasty_i18n::init` 은 프로세스당 1 회 `OnceLock` 이라 테스트 안에서 로케일을
    /// 바꿔 가며 확인할 수 없다. 대신 검증하는 것은 그보다 강한 성질이다:
    /// `diagnostic()` 의 값이 **로케일과 무관하게 고정**이라는 것. 실제로 두 출력이
    /// 갈리는지는 위 en 파리티 테스트(번역 경로 == 영어 경로)와 `lang/ko.toml` 의
    /// 값이 다르다는 사실이 함께 보장한다.
    #[test]
    fn the_diagnostic_rendering_is_locale_independent() {
        let f = failure(59999);
        assert_eq!(
            f.diagnostic().as_str(),
            "Could not connect to tasty instance on port 59999: Connection refused. \
             Is tasty running?"
        );
        assert!(f.diagnostic().as_str().is_ascii());
    }
}
