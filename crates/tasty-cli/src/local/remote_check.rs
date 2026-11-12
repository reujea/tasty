//! 포트 발견 → SSH 터널 → system.info 응답으로 원격 인스턴스의 생존을 확인한다.
//! stale 포트 파일만으로 성공하지 않는다. attach와 포트 발견·터널 구현을 공유한다.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tasty_i18n::{t, t_args};
use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

use crate::out::outln;

use crate::ssh::{self, PortMode, SshTarget, SshTunnel};

/// 프로브의 읽기·쓰기 대기 상한. 응답 없는 연결에서 계속 기다리지 않게 한다.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 성공하면 원격 포트와 가능한 버전 정보를 출력한다. 실패 단계는 오류로 반환하며 재연결하지 않는다.
pub fn run_remote_check(
    target: SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
) -> Result<()> {
    let ssh = ssh::resolve_ssh_path();
    let dest = target.destination.clone();
    let mode = PortMode::parse(port_mode)?;
    // 검증 환경에서만 accept-new를 사용한다. 일반 실행의 SSH 호스트 키 정책은 유지한다.
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);

    let remote_port =
        ssh::discover_remote_port(&ssh, &target, remote_tasty, mode, verify, debug, port_file)
            .with_context(|| t_args("cli.remote_check.not_found", &[dest.as_str(), port_mode]))?;

    let tunnel = SshTunnel::establish(&ssh, &target, remote_port, verify).with_context(|| {
        t_args(
            "cli.remote_check.tunnel_failed",
            &[&remote_port.to_string(), dest.as_str()],
        )
    })?;

    let info = probe_system_info(tunnel.local_port).map_err(|e| {
        // 리스너가 없으면 터널이 연결된 것처럼 읽히는 문장을 쓰지 않는다.
        let key = if tunnel.remote_listener_absent(&e) {
            "cli.remote_check.listener_absent"
        } else {
            "cli.remote_check.no_ipc_response"
        };
        e.context(t_args(key, &[&remote_port.to_string(), dest.as_str()]))
    })?;

    let version = info.get("version").and_then(|v| v.as_str());
    let ws_count = info.get("workspace_count").and_then(|v| v.as_u64());
    let port_str = remote_port.to_string();
    match (version, ws_count) {
        (Some(v), Some(n)) => outln!(
            "{}",
            tasty_i18n::t_count(
                "cli.remote_check.alive_full",
                n,
                &[dest.as_str(), &port_str, v, &n.to_string()],
            )
        )?,
        (Some(v), None) => outln!(
            "{}",
            t_args(
                "cli.remote_check.alive_version",
                &[dest.as_str(), &port_str, v],
            )
        )?,
        _ => outln!(
            "{}",
            t_args("cli.remote_check.alive_basic", &[dest.as_str(), &port_str])
        )?,
    }
    Ok(())
}

/// system.info를 한 번 호출한다. 읽기·쓰기 timeout을 설정하고 EOF·빈 응답을 오류로 반환한다.
fn probe_system_info(port: u16) -> Result<serde_json::Value> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .with_context(|| t_args("cli.remote_check.connect_failed", &[&port.to_string()]))?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT))?;

    let request = JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        method: "system.info".to_string(),
        params: serde_json::json!({}),
        id: Some(serde_json::json!(1)),
        // 세션 토큰은 이 프로세스를 띄운 로컬 Tasty가 발급한 것이라 원격 Tasty는 모른다.
        // 원격 요청에 실으면 `session_token unknown`으로 거절되므로 싣지 않는다.
        session_token: None,
    };

    let mut writer = stream.try_clone()?;
    let json = serde_json::to_string(&request)?;
    writeln!(writer, "{json}").context(t("cli.remote_check.send_failed"))?;
    writer.flush().context(t("cli.remote_check.flush_failed"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .context(t("cli.remote_check.read_failed"))?;
    if n == 0 || line.trim().is_empty() {
        return Err(ssh::connection_closed_error(t("cli.remote_check.eof")));
    }

    let response: JsonRpcResponse =
        serde_json::from_str(line.trim()).context(t("cli.remote_check.parse_failed"))?;
    if let Some(err) = response.error {
        bail!(
            "{}",
            t_args(
                "cli.remote_check.error_response",
                &[&err.code.to_string(), err.message.as_str()],
            )
        );
    }
    Ok(response.result.unwrap_or(serde_json::Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn probe_alive_parses_system_info() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            // 요청을 개행까지 완전 소비해야 한다. 부분 read 후 close 하면 미소비
            // 데이터 때문에 FIN 대신 RST 가 나가 클라이언트 read 가 간헐 실패한다.
            let mut request = String::new();
            BufReader::new(sock.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let resp =
                br#"{"jsonrpc":"2.0","result":{"version":"9.9.9","workspace_count":3},"id":1}"#;
            sock.write_all(resp).unwrap();
            sock.write_all(b"\n").unwrap();
        });
        let info = probe_system_info(port).expect("alive");
        assert_eq!(info.get("version").and_then(|v| v.as_str()), Some("9.9.9"));
        assert_eq!(
            info.get("workspace_count").and_then(|v| v.as_u64()),
            Some(3)
        );
        h.join().unwrap();
    }

    #[test]
    fn probe_stale_port_eof_is_error_not_hang() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (sock, _) = listener.accept().unwrap();
            drop(sock); // 즉시 닫음 → client 는 EOF.
        });
        let r = probe_system_info(port);
        assert!(r.is_err(), "EOF 는 dead(에러)여야 한다: {r:?}");
        // 리스너 없음 판정이 끊긴 연결로 알아보는 형태다.
        assert!(ssh::is_dropped_connection(&r.unwrap_err()));
        h.join().unwrap();
    }

    /// 닫은 포트를 다른 프로세스가 먼저 사용할 수 있어 서로 다른 포트로 최대 세 번 확인한다.
    #[test]
    fn probe_connection_refused_is_error() {
        let mut answered = Vec::new();
        for _ in 0..3 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            match probe_system_info(port) {
                Err(_) => return,
                Ok(v) => answered.push((port, v)),
            }
        }
        panic!("연결 거부는 dead(에러)여야 한다 — 세 포트가 전부 응답했다: {answered:?}");
    }

    /// 부모 Tasty의 토큰을 상속한 프로세스에서도 원격 요청에 `session_token`을 싣지 않는다.
    /// 환경변수를 바꾸는 대신 이 시험만 `TASTY_SESSION_TOKEN`을 둔 자식 프로세스로 다시 실행한다.
    #[test]
    fn a_remote_request_does_not_carry_the_inherited_session_token() {
        const CHILD: &str = "TASTY_TEST_INHERITED_TOKEN_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // 자식의 결과 줄이 부모 시험의 출력에 섞이지 않게 캡처하고, 실패할 때만 보여 준다.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "local::remote_check::tests::a_remote_request_does_not_carry_the_inherited_session_token", "--nocapture", "--test-threads=1"])
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
        probe_system_info(port).expect("probe");
        let request: serde_json::Value = serde_json::from_str(h.join().unwrap().trim()).unwrap();
        assert!(
            request.get("session_token").is_none(),
            "remote request carried a session token: {request}"
        );
    }
}
