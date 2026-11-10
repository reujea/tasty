//! 원격 workspace 목록과 attach 점유 정보를 조회해 합친다. 로컬 포커스·선택 상태는 바꾸지 않는다.
//! CLI와 호스트 워커가 공유하며 SSH 연결·소켓 I/O가 블로킹하므로 이벤트 루프에서 직접 호출하지 않는다.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
use tasty_remote_profiles::{Passkeys, RemoteProfiles};

use tasty_ssh::{self as ssh, PortMode, SshTarget, SshTunnel};

/// 개별 소켓 읽기·쓰기 timeout. 연결 수립과 부분 입력을 합친 전체 호출 시간의 상한은 아니다.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 원격 tasty 의 워크스페이스 1개(browse 결과). 필드는 `workspace.list` 응답에서
/// 뽑고, `attached`/`holder` 만 `attach.list` 와 병합해 채운다.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteWorkspace {
    pub id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub pane_count: u32,
    pub busy_count: u32,
    /// attach.list에서 workspace 점유가 관측됐는지. 단일 surface 점유는 포함하지 않는다.
    pub attached: bool,
    /// 점유중이면 lock holder(원격의 `AttachClientId` = u32; 없으면 None).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<u32>,
    /// 원격 쪽에서도 다른 기계의 mirror인 workspace. mirror를 다시 mirror할 수 없어 attach 대상이 아니다.
    pub mirror: bool,
}

/// `--profile` / `--ssh` 입력을 `(SshTarget, remote_tasty, port_mode, port_file)` 로
/// resolve 한다. CLI 선처리와 호스트 IPC 워커가 공유한다(중복 제거).
///
/// - `profile`: 저장된 tasty-attach 프로필 → [`ssh::resolve_attach_target`](비활성 게이트
///   포함). `remote_tasty`/`remote_port_mode` 는 프로필 값으로 대체된다.
/// - `ssh`: 1회성 대상(`user@host`/alias/`127.0.0.1:PORT`). CLI 인자의 `remote_tasty`/
///   `remote_port_mode` 를 그대로 쓴다.
///
/// 둘 다 지정/둘 다 없음은 호출자가 사전에 거부한다(여기서는 profile 우선).
pub fn resolve_connection_spec(
    profile: Option<&str>,
    ssh: Option<&str>,
    remote_tasty: &str,
    remote_port_mode: &str,
) -> Result<(SshTarget, String, String, Option<String>)> {
    match profile {
        Some(name) => {
            let profiles = RemoteProfiles::load();
            let passkeys = Passkeys::load();
            let p = profiles
                .get(name)
                .with_context(|| tasty_i18n::t_fmt("cli.remote_profile.not_found", name))?;
            ssh::resolve_attach_target(p, &profiles, &passkeys)
        }
        None => match ssh {
            Some(dest) => Ok((
                SshTarget::parse(dest),
                remote_tasty.to_string(),
                remote_port_mode.to_string(),
                None,
            )),
            None => bail!("원격 대상이 필요합니다 (--ssh 또는 --profile)."),
        },
    }
}

/// `127.0.0.1:PORT` / `localhost:PORT` / `[::1]:PORT` 면 PORT 를 돌려준다(loopback 직결).
/// 그 외(원격 호스트/alias)는 None → SSH 터널 경로.
pub fn parse_loopback_port(dest: &str) -> Option<u16> {
    let (host, port_str) = if let Some(rest) = dest.strip_prefix("[::1]:") {
        ("::1", rest)
    } else {
        let (h, p) = dest.rsplit_once(':')?;
        (h, p)
    };
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return None;
    }
    port_str.parse::<u16>().ok()
}

/// loopback은 직접 연결할 포트, 원격은 포트 발견 후 SSH 터널을 반환한다.
/// 블로킹하므로 호스트는 워커에서 호출한다. 반환한 SshTunnel을 버리면 SSH 자식도 종료한다.
/// attach를 이어갈 호출자는 세션 수명 동안 터널을 보유해야 한다.
pub fn resolve_endpoint(
    target: &SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
) -> Result<(Option<SshTunnel>, u16)> {
    if let Some(port) = parse_loopback_port(&target.destination) {
        return Ok((None, port));
    }

    let ssh = ssh::resolve_ssh_path();
    let mode = PortMode::parse(port_mode)?;
    // 검증용 TASTY_SSH_VERIFY가 있으면 accept-new, 없으면 기본 host-key 정책을 사용한다.
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);
    let remote_port =
        ssh::discover_remote_port(&ssh, target, remote_tasty, mode, verify, debug, port_file)?;
    let tunnel = SshTunnel::establish(&ssh, target, remote_port, verify)?;
    let local_port = tunnel.local_port;
    Ok((Some(tunnel), local_port))
}

/// 포트에 JSON-RPC 요청 한 번을 보내고 응답 한 줄을 읽는다. EOF·빈 응답은 오류다.
/// 읽기·쓰기 timeout을 설정하지만 연결과 여러 부분 읽기를 합친 절대 기한은 없다.
/// 호스트도 이 함수의 원래 응답을 읽어 상황별 사용자 안내를 만든다.
pub fn probe_method(
    port: u16,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    probe_method_bound(port, method, params, None)
}

pub(crate) fn probe_method_bound(
    port: u16,
    method: &str,
    params: serde_json::Value,
    attempt: Option<&crate::outbound::AttemptToken>,
) -> Result<serde_json::Value> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .with_context(|| format!("터널 localport 127.0.0.1:{port} 연결 실패"))?;
    if let Some(attempt) = attempt {
        attempt.register_socket(&stream)?;
    }
    stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT))?;

    let request = JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: Some(serde_json::json!(1)),
        // 세션 토큰은 이 프로세스를 띄운 로컬 Tasty가 발급한 것이라 원격 Tasty는 모른다.
        // 원격 요청에 실으면 `session_token unknown`으로 거절되므로 싣지 않는다.
        session_token: None,
    };

    let mut writer = stream.try_clone()?;
    let json = serde_json::to_string(&request)?;
    writeln!(writer, "{json}").context("IPC 요청 전송 실패")?;
    writer.flush().context("IPC 요청 flush 실패")?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .context("IPC 응답 read 실패 (타임아웃/연결 종료)")?;
    if n == 0 || line.trim().is_empty() {
        bail!("IPC 응답 없음 (EOF) — 원격 서버가 응답하지 않습니다");
    }

    let response: JsonRpcResponse =
        serde_json::from_str(line.trim()).context("IPC 응답 JSON 파싱 실패")?;
    if let Some(err) = response.error {
        bail!("IPC 에러 응답 (code={}): {}", err.code, err.message);
    }
    Ok(response.result.unwrap_or(serde_json::Value::Null))
}

/// 접속된 포트로 `workspace.list` + `attach.list` 를 조회해 병합한 목록을 만든다.
/// 터널 수명은 호출자가 관리한다(browse 는 조회 후 Drop, attach 는 별개).
pub fn browse_via_port(port: u16) -> Result<Vec<RemoteWorkspace>> {
    browse_via_port_bound(port, None)
}

pub(crate) fn browse_via_port_bound(
    port: u16,
    attempt: Option<&crate::outbound::AttemptToken>,
) -> Result<Vec<RemoteWorkspace>> {
    let ws_list = probe_method_bound(port, "workspace.list", serde_json::json!({}), attempt)
        .context("원격 workspace.list 조회 실패")?;
    // attach.list 는 병합용 부가 정보 — 실패해도 목록 자체는 반환(점유 표시만 생략).
    let attach_list = probe_method_bound(port, "attach.list", serde_json::json!({}), attempt)
        .unwrap_or(serde_json::Value::Null);

    // workspace 점유만 합친다. workspace.list에 멤버 surface ID가 없어 단일 surface 점유는 판정하지 않는다.
    let mut ws_holders: std::collections::HashMap<u32, Option<u32>> =
        std::collections::HashMap::new();
    if let Some(arr) = attach_list.get("workspaces").and_then(|v| v.as_array()) {
        for w in arr {
            if let Some(wid) = w.get("workspace_id").and_then(|v| v.as_u64()) {
                let holder = w.get("holder").and_then(|v| v.as_u64()).map(|h| h as u32);
                ws_holders.insert(wid as u32, holder);
            }
        }
    }

    let arr = ws_list
        .as_array()
        .context("원격 workspace.list 응답이 배열이 아닙니다")?;
    let mut out = Vec::with_capacity(arr.len());
    for ws in arr {
        let id = ws.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let holder_entry = ws_holders.get(&id);
        out.push(RemoteWorkspace {
            id,
            name: ws
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            subtitle: ws
                .get("subtitle")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            description: ws
                .get("description")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            pane_count: ws.get("pane_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            busy_count: ws.get("busy_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            attached: holder_entry.is_some(),
            holder: holder_entry.and_then(|h| *h),
            mirror: ws.get("mirror").and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    Ok(out)
}

/// 전체 browse: 접속 스펙 resolve → 엔드포인트(터널/loopback) → 목록 조회. **블로킹**
/// (SSH) — 워커 스레드/CLI 프로세스에서 호출한다. 터널은 이 함수 반환 시 Drop(단발).
pub fn browse(
    target: &SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
) -> Result<Vec<RemoteWorkspace>> {
    let (_tunnel, port) = resolve_endpoint(target, remote_tasty, port_mode, port_file)?;
    // _tunnel 은 이 스코프 끝에서 Drop → 자식 ssh kill(단발 조회이므로 여기서 정리).
    browse_via_port(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn loopback_ports_parsed() {
        assert_eq!(parse_loopback_port("127.0.0.1:45123"), Some(45123));
        assert_eq!(parse_loopback_port("localhost:8080"), Some(8080));
        assert_eq!(parse_loopback_port("[::1]:9000"), Some(9000));
    }

    #[test]
    fn non_loopback_is_none() {
        assert_eq!(parse_loopback_port("gx10"), None);
        assert_eq!(parse_loopback_port("user@host"), None);
        assert_eq!(parse_loopback_port("192.168.0.10:45123"), None);
        assert_eq!(parse_loopback_port("example.com:22"), None);
    }

    /// workspace.list와 attach.list의 모의 응답을 합쳐 점유자를 표시하는지 확인한다.
    #[test]
    fn browse_via_port_parses_and_merges_attach() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            // 첫 연결: workspace.list 응답.
            {
                let (mut sock, _) = listener.accept().unwrap();
                let mut req = String::new();
                BufReader::new(sock.try_clone().unwrap())
                    .read_line(&mut req)
                    .unwrap();
                let resp = br#"{"jsonrpc":"2.0","result":[{"id":1,"name":"alpha","pane_count":2,"busy_count":0},{"id":2,"name":"beta","pane_count":1,"busy_count":1,"mirror":true}],"id":1}"#;
                sock.write_all(resp).unwrap();
                sock.write_all(b"\n").unwrap();
            }
            // 둘째 연결: attach.list 응답(ws 2 점유중).
            {
                let (mut sock, _) = listener.accept().unwrap();
                let mut req = String::new();
                BufReader::new(sock.try_clone().unwrap())
                    .read_line(&mut req)
                    .unwrap();
                let resp = br#"{"jsonrpc":"2.0","result":{"attached":[],"workspaces":[{"workspace_id":2,"holder":7,"granted_seq":5}]},"id":1}"#;
                sock.write_all(resp).unwrap();
                sock.write_all(b"\n").unwrap();
            }
        });
        let list = browse_via_port(port).expect("browse ok");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, 1);
        assert_eq!(list[0].name, "alpha");
        assert!(!list[0].attached);
        assert_eq!(list[0].holder, None);
        assert_eq!(list[1].id, 2);
        assert!(list[1].attached);
        assert_eq!(list[1].holder, Some(7));
        assert_eq!(list[1].busy_count, 1);
        assert!(!list[0].mirror, "a missing mirror field reads as local");
        assert!(list[1].mirror, "a mirror on the remote is marked");
        h.join().unwrap();
    }

    /// EOF를 오류로 반환하는지 확인한다.
    #[test]
    fn probe_stale_port_eof_is_error_not_hang() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            drop(sock);
        });
        let r = probe_method(port, "workspace.list", serde_json::json!({}));
        assert!(r.is_err(), "EOF 는 에러여야 한다: {r:?}");
        h.join().unwrap();
    }

    /// 닫힌 포트는 반납 뒤 다른 프로세스가 가져갈 수 있어 최대 세 포트를 시도한다.
    /// 이 시험은 모든 순간의 포트 부재를 보장하지는 않는다.
    #[test]
    fn probe_connection_refused_is_error() {
        let mut answered = Vec::new();
        for _ in 0..3 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            match probe_method(port, "workspace.list", serde_json::json!({})) {
                Err(_) => return,
                Ok(v) => answered.push((port, v)),
            }
        }
        panic!("연결 거부는 에러여야 한다 — 세 포트가 전부 응답했다: {answered:?}");
    }

    /// 부모 Tasty의 토큰을 상속한 프로세스에서도 원격 요청에 `session_token`을 싣지 않는다.
    /// 환경변수를 바꾸는 대신 이 시험만 `TASTY_SESSION_TOKEN`을 둔 자식 프로세스로 다시 실행한다.
    #[test]
    fn a_remote_request_does_not_carry_the_inherited_session_token() {
        const CHILD: &str = "TASTY_TEST_INHERITED_TOKEN_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // 자식의 결과 줄이 부모 시험의 출력에 섞이지 않게 캡처하고, 실패할 때만 보여 준다.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "browse::tests::a_remote_request_does_not_carry_the_inherited_session_token",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .env("TASTY_SESSION_TOKEN", "ab".repeat(32))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child run failed: {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert_eq!(
            std::env::var("TASTY_SESSION_TOKEN").unwrap(),
            "ab".repeat(32)
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(sock.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            sock.write_all(br#"{"jsonrpc":"2.0","result":{},"id":1}"#)
                .unwrap();
            sock.write_all(b"\n").unwrap();
            request
        });
        probe_method(port, "system.info", serde_json::json!({})).expect("probe");
        let request: serde_json::Value = serde_json::from_str(h.join().unwrap().trim()).unwrap();
        assert!(
            request.get("session_token").is_none(),
            "remote request carried a session token: {request}"
        );
    }
}
