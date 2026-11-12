//! SSH 터널 너머의 tasty가 이 프로세스 자신인지 판정한다.
//!
//! 호스트명이나 LAN IP로 자기 머신을 지정하면 요청 포트는 터널의 로컬 포트라서 자기 IPC 포트와
//! 비교해도 같지 않다. 그래서 터널을 연 뒤 상대의 `system.info`가 보고한 `instance_id`를 이
//! 프로세스의 값과 비교한다. 신뢰 경계는 SSH이며 별도 권한 검사를 더하지 않는다.

use crate::outbound::AttemptToken;

/// 상대가 이 프로세스 자신이라서 attach를 거절했다. 호출자는 downcast해 자기 포트 거절과
/// 같은 오류로 응답하거나 거절 기록에 남긴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThisInstance {
    /// 터널의 로컬 포트.
    pub port: u16,
}

impl std::fmt::Display for ThisInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "attach target reached over SSH is this instance itself; attaching to itself is refused"
        )
    }
}

impl std::error::Error for ThisInstance {}

/// SSH 터널로 연결한 엔드포인트면 상대의 `instance_id`를 확인한다. loopback 직결(터널 없음)은
/// 포트 비교가 맡으므로 묻지 않는다. 상대가 `instance_id`를 보고하지 않으면(이전 버전) 판정하지
/// 않고 통과시킨다. `system.info` 요청 자체가 실패하면 그 오류를 돌려준다.
pub fn refuse_this_instance(
    tunneled: bool,
    port: u16,
    attempt: Option<&AttemptToken>,
) -> anyhow::Result<()> {
    if !tunneled {
        return Ok(());
    }
    let info =
        crate::browse::probe_method_bound(port, "system.info", serde_json::json!({}), attempt)?;
    if reports_this_instance(&info) {
        return Err(ThisInstance { port }.into());
    }
    Ok(())
}

/// 엔드포인트의 터널이 주는 리스너 없음 판정 핸들. 터널이 없는 엔드포인트와 시험은 `()`나
/// 판정 핸들을 넘긴다.
pub trait TunnelVerdict {
    fn listener_verdict(&self) -> Option<tasty_ssh::ListenerVerdict>;
}

impl TunnelVerdict for tasty_ssh::SshTunnel {
    fn listener_verdict(&self) -> Option<tasty_ssh::ListenerVerdict> {
        Some(tasty_ssh::SshTunnel::listener_verdict(self))
    }
}

impl TunnelVerdict for () {
    fn listener_verdict(&self) -> Option<tasty_ssh::ListenerVerdict> {
        None
    }
}

impl TunnelVerdict for tasty_ssh::ListenerVerdict {
    fn listener_verdict(&self) -> Option<tasty_ssh::ListenerVerdict> {
        Some(self.clone())
    }
}

/// 터널 너머 상대가 자기 자신이면 거절한다. 이 검사가 터널 뒤 첫 요청이므로, 원격 포트에 리스너가
/// 없어 요청이 끊기면 그 판정을 오류에 붙인다.
pub fn refuse_this_instance_over<T: TunnelVerdict>(
    tunnel: Option<&T>,
    port: u16,
    attempt: Option<&AttemptToken>,
) -> anyhow::Result<()> {
    refuse_this_instance(tunnel.is_some(), port, attempt).map_err(|error| {
        match tunnel.and_then(T::listener_verdict) {
            Some(verdict) => verdict.explain_failure(error),
            None => error,
        }
    })
}

fn reports_this_instance(info: &serde_json::Value) -> bool {
    info.get("instance_id").and_then(serde_json::Value::as_str)
        == Some(tasty_ipc::instance::instance_id())
}

/// 요청 한 줄을 받고 응답 없이 닫는 가짜 서버. 원격 포트에 리스너가 없을 때 ssh가 로컬 연결을
/// 닫는 모양이다.
#[cfg(test)]
pub(crate) fn close_after_request() -> (u16, std::thread::JoinHandle<()>) {
    use std::io::BufRead;
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        std::io::BufReader::new(&stream)
            .read_line(&mut line)
            .expect("read");
    });
    (port, handle)
}

/// 요청 한 줄을 읽고 주어진 결과 한 줄을 돌려주는 가짜 서버. 받은 요청 줄을 돌려준다.
#[cfg(test)]
pub(crate) fn serve_once(result: serde_json::Value) -> (u16, std::thread::JoinHandle<String>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).expect("read");
        let reply = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result });
        writeln!(&stream, "{reply}").expect("write");
        line
    });
    (port, handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_reporting_this_process_id_is_refused() {
        let (port, server) =
            serve_once(serde_json::json!({ "instance_id": tasty_ipc::instance::instance_id() }));
        let error = refuse_this_instance(true, port, None).expect_err("same instance");
        assert_eq!(
            error.downcast_ref::<ThisInstance>(),
            Some(&ThisInstance { port })
        );
        assert!(server.join().expect("server").contains("\"system.info\""));
    }

    #[test]
    fn another_instance_or_an_older_peer_passes() {
        for result in [
            serde_json::json!({ "instance_id": "another-instance" }),
            serde_json::json!({ "version": "0.1.0" }),
        ] {
            let (port, server) = serve_once(result);
            refuse_this_instance(true, port, None).expect("not this instance");
            server.join().expect("server");
        }
    }

    #[test]
    fn a_direct_loopback_endpoint_is_not_asked() {
        // 아무도 듣지 않는 포트라도 터널이 아니면 연결하지 않는다.
        let port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .expect("free port")
            .port();
        refuse_this_instance(false, port, None).expect("not asked");
    }

    /// 판정 요청에는 신원이 필요 없으므로, 부모 Tasty의 토큰을 상속한 GUI에서도 `system.info`에
    /// `session_token`을 싣지 않는다. 실으면 원격이 모르는 토큰이라 거절해 기존 워크스페이스
    /// attach까지 막힌다. 이 시험만 `TASTY_SESSION_TOKEN`을 둔 자식 프로세스로 다시 실행한다.
    #[test]
    fn the_self_check_does_not_carry_the_inherited_session_token() {
        const CHILD: &str = "TASTY_TEST_INHERITED_TOKEN_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // 자식의 결과 줄이 부모 시험의 출력에 섞이지 않게 캡처하고, 실패할 때만 보여 준다.
            let output = std::process::Command::new(std::env::current_exe().expect("exe"))
                .args([
                    "--exact",
                    "self_instance::tests::the_self_check_does_not_carry_the_inherited_session_token",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .env("TASTY_SESSION_TOKEN", "ab".repeat(32))
                .output()
                .expect("child");
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
            std::env::var("TASTY_SESSION_TOKEN").expect("inherited"),
            "ab".repeat(32)
        );
        let (port, server) = serve_once(serde_json::json!({ "instance_id": "another-instance" }));
        refuse_this_instance(true, port, None).expect("not this instance");
        let request: serde_json::Value =
            serde_json::from_str(server.join().expect("server").trim()).expect("json");
        assert_eq!(request["method"], "system.info");
        assert!(
            request.get("session_token").is_none(),
            "self check carried a session token: {request}"
        );
    }
}
