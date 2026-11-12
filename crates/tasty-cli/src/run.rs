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

/// loopback에서도 방화벽 DROP 등으로 연결이 지연될 수 있어 대기 상한을 둔다.
/// 에이전트 훅은 동기로 실행되므로 연결 지연이 턴을 막을 수 있다.
const IPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// 사용자 stderr용 번역과 hook-failures.log용 영어 진단을 나눠 만든다.
pub(crate) struct ConnectFailure {
    port: u16,
    source: std::io::Error,
    /// 포트 파일 옆 인스턴스 파일이 그 포트의 인스턴스가 끝났다고 말한다.
    exited: bool,
}

impl ConnectFailure {
    /// 사용자 표시용 — 현재 로케일.
    fn localized(&self) -> String {
        let key = if self.exited {
            "cli.request.connect_failed_instance_exited"
        } else {
            "cli.request.connect_failed"
        };
        tasty_i18n::t_fmt2(key, &self.port.to_string(), &self.source.to_string())
    }

    /// 영어 카탈로그와 정확히 같은 진단문을 반환한다.
    pub(crate) fn diagnostic(&self) -> crate::hook_failure::DiagnosticEnglish {
        let hint = if self.exited {
            "The tasty instance that wrote the port file has exited, so the port file is stale."
        } else {
            "Is tasty running?"
        };
        crate::hook_failure::DiagnosticEnglish::new_unchecked(format!(
            "Could not connect to tasty instance on port {}: {}. {hint}",
            self.port, self.source
        ))
    }

    /// 연결 시도는 그대로 두고, 실패한 뒤에만 포트 파일이 끝난 인스턴스의 것인지 확인한다.
    fn judged(mut self, port_file: Option<&str>) -> Self {
        self.exited = crate::port_file::exited_owner(port_file, self.port).is_some();
        self
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
    connect_ipc_within(port, IPC_CONNECT_TIMEOUT)
}

/// 호출자의 남은 대기 시간이 연결 상한보다 짧으면 그 시간까지만 연결을 기다린다.
fn connect_ipc_within(port: u16, left: Duration) -> std::result::Result<TcpStream, ConnectFailure> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let bound = left.min(IPC_CONNECT_TIMEOUT).max(Duration::from_millis(1));
    TcpStream::connect_timeout(&addr, bound).map_err(|source| ConnectFailure {
        port,
        source,
        exited: false,
    })
}

/// 포트 파일에서 읽은 포트에 연결한다. 실패 안내는 그 포트 파일의 인스턴스가 끝났는지 구분한다.
fn connect_ipc_from(port: u16, port_file: Option<&str>) -> Result<TcpStream, ConnectFailure> {
    connect_ipc(port).map_err(|e| e.judged(port_file))
}

/// 도움말·버전 출력은 오류가 아니라 clap이 렌더링하는 정상 출력이다.
fn is_help_output(kind: clap::error::ErrorKind) -> bool {
    use clap::error::ErrorKind;
    matches!(
        kind,
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            | ErrorKind::DisplayVersion
    )
}

/// 정적 트리가 해석하지 못한 argv를 플러그인 명령을 포함한 트리로 다시 해석한다.
/// 도움말 요청이면 그 트리의 도움말을 출력하고 종료한다. 플러그인 명령이 아니면 `None`.
pub fn try_run_plugin_cli(args: &[String]) -> Option<Result<()>> {
    let plugins_root = tasty_host_plugin::plugin_root()?;
    let dynamic::DiscoveredPlugins {
        clis: entries,
        caller_contexts,
    } = dynamic::discover_plugins(&plugins_root);
    if entries.is_empty() {
        return None;
    }
    let augmented = dynamic::build_augmented_cli(&entries);
    // 루트 옵션과 `help` 접두를 건너뛴 첫 명령이 plugin 이름이면 clap 오류도 자체 출력으로
    // 처리한다. 정적 CLI의 "unrecognized subcommand"가 대신 뜨면 안 된다.
    let is_plugin_cmd = crate::help_route::first_command_name(&augmented, args)
        .is_some_and(|name| entries.iter().any(|e| e.cli.name == name));
    let matches = match augmented.try_get_matches_from(args) {
        Ok(m) => m,
        Err(err) => {
            if is_plugin_cmd || is_help_output(err.kind()) {
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
    let (mut request, polling, auto_wait) =
        match dynamic::matches_to_request(&entries, &caller_contexts, &matches, &mut |query| {
            placement_is_remote(query, port_file.as_deref())
        }) {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
    let child_table = match dynamic::wants_child_table(&entries, &request) {
        Ok(wanted) => wanted,
        Err(e) => return Some(Err(e)),
    };
    if polling.is_some() || auto_wait.is_some() {
        envelope.refuse_if_set();
    }
    envelope.apply(&mut request);
    // stdout 파이프 조기 종료(EPIPE)는 조용한 종료 코드 0 — docs/dev-guide/cli-structure.md#stdout-출력-outrs.
    Some(crate::out::quiet_if_stdout_closed(
        match (polling, auto_wait) {
            (Some(p), _) => run_dynamic_client_polling(request, p, port_file.as_deref()),
            (None, Some(aw)) => {
                run_dynamic_client_with_auto_wait(request, aw, port_file.as_deref())
            }
            (None, None) => run_dynamic_client(request, port_file.as_deref(), child_table),
        },
    ))
}

/// 실행 위치가 attach mirror 인지 host 에 묻는다. 묻지 못하면(구 host·연결 실패·대상 없음) 이 머신의
/// 대상으로 보고 기존 경로 검사를 한다. 원래 요청의 전송·오류 처리는 이 결과와 관계없이 그대로다.
fn placement_is_remote(
    query: &serde_json::Map<String, serde_json::Value>,
    port_file: Option<&str>,
) -> bool {
    let probe = || -> Result<bool> {
        let port = crate::port_file::read_port_diagnosed(port_file)?;
        let mut conn = IpcConnection::new(connect_ipc(port)?)?;
        let request = tasty_ipc::protocol::JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".into(),
            method: "attach.agent_target".into(),
            params: serde_json::Value::Object(query.clone()),
            id: Some(serde_json::Value::from(1)),
            session_token: None,
        };
        let value = conn.send(&request)?;
        Ok(value.get("remote").and_then(serde_json::Value::as_bool) == Some(true))
    };
    probe().unwrap_or_else(|e| {
        tracing::debug!("attach placement probe failed; treating the target as local: {e}");
        false
    })
}

/// 플러그인 CLI 단발 요청. 훅 전달 실패는 로컬 파일에 기록한다.
/// 훅 매니페스트가 polling/auto_wait를 선언하면 이 경로를 우회하므로
/// tests/hook_commands_stay_on_the_recording_path.rs가 그 선언을 막는다.
/// 포트 조회·연결·준비·JSON-RPC 오류와 성공 응답의 host_call_failures를 기록한다.
/// 응답 뒤 stdout 쓰기 실패는 전달 실패가 아니므로 여기에 기록하지 않는다.
fn run_dynamic_client(
    mut request: tasty_ipc::protocol::JsonRpcRequest,
    port_file: Option<&str>,
    child_table: bool,
) -> Result<()> {
    // CLI 가 문구를 만드는 갈래는 모두 **로그에는 영어, stderr 에는 번역문**을 낸다.
    // `record` 가 `DiagnosticEnglish` 만 받으므로 그 분리는 타입이 지킨다.
    let port = match crate::port_file::read_port_diagnosed(port_file) {
        Ok(p) => p,
        Err(e) => {
            hook_failure::record(
                &request.method,
                &request.params,
                None, // 호스트에 닿지도 못했다 — JSON-RPC 코드가 없다
                &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
            );
            return Err(anyhow::anyhow!("{}", crate::port_file::localize(&e)));
        }
    };
    let stream = match connect_ipc_from(port, port_file) {
        Ok(s) => s,
        Err(e) => {
            hook_failure::record(&request.method, &request.params, None, &e.diagnostic());
            return Err(e.into());
        }
    };
    let mut conn = match IpcConnection::new(stream) {
        Ok(c) => c,
        Err(e) => {
            // 요청을 보내기 전의 준비 실패도 기록한다. fd 고갈이면 로그 파일 열기도
            // 실패할 수 있어 기록은 best-effort다. 문구는 io::Error에서 얻는다.
            hook_failure::record(
                &request.method,
                &request.params,
                None, // JSON-RPC 응답을 받은 적이 없다 — 코드가 없다
                &hook_failure::DiagnosticEnglish::new_unchecked(e.to_string()),
            );
            return Err(e);
        }
    };
    // capability 확인에 실패하면 원래 요청은 보내지 않는다. 확인 만료의 -32067만
    // 원래 요청의 오류 코드로 기록한다. 확인 요청이 받은 다른 오류는 서버 문구를
    // 보존하되 원래 요청의 JSON-RPC 코드로 기록하지 않는다.
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
            // 성공 응답에도 최선노력 host 호출 실패가 담길 수 있다. 훅의 stdout이
            // 버려져도 사유를 볼 수 있도록 host_call_failures를 파일에 기록한다.
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
            if child_table && crate::format::child_table::write_table(&value)? {
                return Ok(());
            }
            outln!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            )?;
            Ok(())
        }
        Err(e) => {
            // 외부 응답 문구는 번역됐을 수 있으므로 고정 JSON-RPC 코드도 기록한다.
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

/// auto-wait/polling 응답 하나를 한 줄 JSON으로 만든다. 마지막 줄을 읽는 호출자의 형식을 유지한다.
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

    let first_value = {
        let stream = connect_ipc_from(port, port_file)?;
        let mut conn = IpcConnection::new(stream)?;
        match conn.send(&request) {
            Ok(value) => value,
            Err(e) => {
                crate::rpc_error::exit_with(&e);
            }
        }
    };
    outln!("{}", line_json(&first_value))?;

    if aw.skipped {
        return Ok(());
    }

    let wait_params = build_wait_params(&aw, &first_value);
    let wait_req = tasty_ipc::protocol::JsonRpcRequest {
        caller_agent_id: request.caller_agent_id.clone(),
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
        let stream = connect_ipc_from(port, port_file)?;
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
/// stdout 이 파이프 조기 종료(EPIPE)로 닫히면 조용히 `Ok(())` — 종료 코드 0(docs/dev-guide/cli-structure.md#stdout-출력-outrs).
/// 출력 경로 전체가 [`crate::out`] 을 거치므로 `StdoutClosed` 가 여기까지 `?` 로 올라온다.
pub fn run_client(command: Commands, port_file: Option<&str>) -> Result<()> {
    run_client_with(command, port_file, Envelope::default())
}

/// CLI 루트 플래그의 요청 옵션을 받아 실행한다. boot 진입점이 사용한다.
pub fn run_client_with(
    command: Commands,
    port_file: Option<&str>,
    envelope: Envelope,
) -> Result<()> {
    crate::out::quiet_if_stdout_closed(run_client_inner(command, port_file, envelope))
}

fn run_client_inner(command: Commands, port_file: Option<&str>, envelope: Envelope) -> Result<()> {
    if let Dispatch::ClientDriven(cmd) = command.dispatch()? {
        // 클라이언트 주도 명령은 요청을 여럿 보내거나 IPC 를 안 탄다 — 봉투 상한을 실을
        // 요청 하나가 없다. 받으면 조용히 버리지 않고 거절한다.
        envelope.refuse_if_set();
        return cmd.run(&ClientCtx { port_file });
    }

    // 인자 오류를 인스턴스 미실행 오류보다 먼저 알리도록 연결 전에 요청을 만든다.
    // 서버의 capability 확인만 연결 뒤에 수행한다.
    // 결과를 기다리는 명령은 루프라 첫 요청의 응답 시한을 받지 않는다(contract 모듈).
    if crate::plugin_enable_wait::waits(&command) {
        envelope.refuse_if_set();
    }
    let mut request = command_to_request(&command);
    envelope.apply(&mut request);
    let cli_warnings = take_cli_warnings(&mut request);

    // 포트 저장·삭제는 실행 중인 Tasty가 없으면 파일을 직접 고친다. 저장한 포트가 막혀 Tasty가
    // 뜨지 못할 때 이 명령으로 풀 수 있어야 하기 때문이다.
    let offline = offline_webhook_port(&command);
    let port = match (crate::port_file::read_port_diagnosed(port_file), offline) {
        (Ok(port), _) => port,
        (Err(tasty_ipc::port_file::PortFileError::NotFound { .. }), Some(change)) => {
            return save_webhook_port_offline(change);
        }
        (Err(e), _) => return Err(anyhow::anyhow!("{}", crate::port_file::localize(&e))),
    };
    let stream = match (connect_ipc_from(port, port_file), offline) {
        (Ok(stream), _) => stream,
        // 포트 파일이 남아 있어도 연결이 거절되면 그 포트에 인스턴스가 없다.
        (Err(e), Some(change)) if e.source.kind() == std::io::ErrorKind::ConnectionRefused => {
            return save_webhook_port_offline(change);
        }
        (Err(e), _) => return Err(e.into()),
    };

    let mut conn = IpcConnection::new(stream)?;

    // 새 계약을 쓰는 요청은 상대가 그것을 선언했는지 **보내기 전에** 묻는다 — 모르는
    // 서버는 그 필드를 조용히 버리고 성공으로 답한다(`contract` 모듈).
    if let Err(e) = super::contract::ensure(&mut conn, &mut request) {
        super::contract::exit_on_failure(e);
    }
    let result = conn.send(&request);

    match result {
        Ok(mut value) => {
            let failure = crate::plugin_enable_wait::settle(
                &command,
                &request,
                conn,
                |left| IpcConnection::new(connect_ipc_within(port, left)?),
                &mut value,
            );
            merge_cli_warnings(&mut value, cli_warnings);
            format_output(&command, &value)?;
            if let Some(failure) = failure {
                crate::out::errln!("{failure}");
                std::process::exit(1);
            }
        }
        // JSON-RPC 오류가 아니면 그 포트의 상대가 Tasty가 아닐 수 있다. 파일은 고치지 않고 직접 고치는 방법을 알린다.
        Err(e)
            if offline.is_some()
                && e.downcast_ref::<tasty_ipc::client::JsonRpcCallError>()
                    .is_none() =>
        {
            crate::out::errln!("{}", crate::rpc_error::render(&e));
            crate::out::errln!("{}", not_tasty_hint(port_file, port));
            std::process::exit(1);
        }
        Err(e) => {
            if let Some(warning) = crate::install_warning::unverified_install_warning(&command, &e)
            {
                crate::out::errln!("{}", crate::rpc_error::render(&e));
                crate::out::errln!("{warning}");
                std::process::exit(1);
            }
            crate::rpc_error::exit_with(&e);
        }
    }

    Ok(())
}

/// 포트 파일의 포트에서 Tasty가 아닌 상대가 답했을 때의 안내.
fn not_tasty_hint(port_file: Option<&str>, port: u16) -> String {
    let port_file = port_file
        .map(std::path::PathBuf::from)
        .or_else(tasty_ipc::port_file::port_file_path)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let webhooks_file = tasty_settings::webhook_port_file::path()
        .display()
        .to_string();
    tasty_i18n::t_args(
        "cli.webhook.port_not_tasty",
        &[&port_file, &port.to_string(), &webhooks_file],
    )
}

/// 인스턴스 없이 처리할 수 있는 웹훅 포트 변경. `port`가 Some이면 저장, None이면 삭제다.
#[derive(Clone, Copy)]
struct OfflinePortChange {
    port: Option<u16>,
    /// 이전 이름 `webhook config`로 불렀다.
    deprecated_name: bool,
}

fn offline_webhook_port(command: &Commands) -> Option<OfflinePortChange> {
    use crate::commands::WebhookCommands as W;
    let Commands::Webhook { command } = command else {
        return None;
    };
    let (port, deprecated_name) = match command {
        W::Port {
            port: Some(port), ..
        } => (Some(*port), false),
        W::Port {
            port: None,
            unset: true,
        } => (None, false),
        // 이전 이름은 범위 검사가 없다. 0은 실행 중인 Tasty의 범위 오류와 같은 경로로 보낸다.
        W::Config {
            port: Some(port @ 1..),
        } => (Some(*port), true),
        _ => return None,
    };
    Some(OfflinePortChange {
        port,
        deprecated_name,
    })
}

/// 실행 중인 Tasty가 없을 때 webhooks.toml을 직접 고치고, 그렇게 했다는 사실을 함께 출력한다.
fn save_webhook_port_offline(change: OfflinePortChange) -> Result<()> {
    let path = tasty_settings::webhook_port_file::path();
    crate::local::port::save_webhook_port_at(&path, change.port).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.webhook.port_file_write_failed",
                &path.display().to_string(),
                &e.to_string()
            )
        )
    })?;
    let file = path.display().to_string();
    let mut value = serde_json::json!({
        "saved_port": change.port,
        "applies_from": "next_start",
        "instance_running": false,
        "file": file,
        "message": tasty_i18n::t_fmt("cli.webhook.port_saved_offline", &file),
    });
    if change.deprecated_name {
        merge_cli_warnings(
            &mut value,
            vec![super::request::WEBHOOK_CONFIG_DEPRECATED.to_string()],
        );
    }
    outln!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    )?;
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
        let mut mfr = HashMap::new();
        mfr.insert("child_index".into(), "child".into());
        mfr.insert("parent_surface_id".into(), "surface".into());
        let plan = sample_plan(mfr, HashMap::new(), Map::new());
        let resp = json!({ "child_index": 3, "parent_surface_id": 11 });
        let p = build_wait_params(&plan, &resp);
        assert_eq!(p.get("child"), Some(&Value::from(3)));
        assert_eq!(p.get("surface"), Some(&Value::from(11)));
        assert_eq!(p.get("surface_id"), Some(&Value::from(11)));
    }

    #[test]
    fn auto_wait_maps_from_request_fallback() {
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
        let mut params = Map::new();
        params.insert("timeout".into(), Value::from(30_u32));
        let plan = sample_plan(HashMap::new(), HashMap::new(), params);
        let p = build_wait_params(&plan, &json!({}));
        assert_eq!(p.get("timeout"), Some(&Value::from(30_u32)));
    }

    #[test]
    fn auto_wait_timeout_renamed_via_polling_timeout_field() {
        let mut params = Map::new();
        params.insert("timeout".into(), Value::from(45_u32));
        let mut plan = sample_plan(HashMap::new(), HashMap::new(), params);
        plan.polling.timeout_field = Some("deadline".into());
        let p = build_wait_params(&plan, &json!({}));
        assert_eq!(p.get("deadline"), Some(&Value::from(45_u32)));
        assert!(p.get("timeout").is_none());
    }

    #[test]
    fn auto_wait_timeout_absent_no_copy() {
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
            exited: false,
        }
    }

    #[test]
    fn english_lang_value_matches_the_diagnostic_rendering() {
        tasty_i18n::init("en");
        let f = failure(59999);
        assert_eq!(f.localized(), f.diagnostic().as_str());
        let exited = ConnectFailure {
            exited: true,
            ..failure(59999)
        };
        assert_eq!(exited.localized(), exited.diagnostic().as_str());
        assert!(!exited.localized().contains("Is tasty running?"));
    }

    /// 연결 실패 뒤 포트 파일 옆 인스턴스 기록이 끝난 프로세스를 가리키면 그 사실을 안내한다.
    /// 인스턴스 파일이 없으면 기존 안내를 쓴다.
    #[test]
    fn a_failed_connection_names_a_stale_port_file() {
        use tasty_ipc::instance_file::{self, InstanceRecord};
        let home = tempfile::tempdir().unwrap();
        let port_file = home.path().join("tasty.port");
        std::fs::write(&port_file, "59999").unwrap();
        let port_file = port_file.to_str().unwrap();
        assert!(!failure(59999).judged(Some(port_file)).exited);
        let pid = std::process::id();
        let stale = InstanceRecord {
            pid,
            start_time: instance_file::process_start_time(pid)
                .unwrap()
                .wrapping_add(1),
            port: Some(59999),
        };
        instance_file::write(home.path(), &stale).unwrap();
        assert!(failure(59999).judged(Some(port_file)).exited);
    }

    /// OnceLock으로 로케일을 재설정할 수 없어 진단문이 i18n 없이 고정되는지 확인한다.
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
