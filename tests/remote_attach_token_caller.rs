//! 권한 없는 세션 토큰으로 remote.attach·remote.refusals를 호출한다(docs/features/remote-attach/index.md).
//! 메서드 표만 보는 시험은 App 층까지 닿는지 알 수 없어 실제 인스턴스 둘을 loopback으로 잇는다.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use common::TastyInstance;
use serde_json::{Value, json};

fn issue_token(instance: &TastyInstance, agent_id: &str) -> String {
    instance.call("session.issue", json!({ "agent_id": agent_id }))["token"]
        .as_str()
        .expect("session.issue returns a token")
        .to_string()
}

/// 세션 토큰을 봉투에 실어 보내고 전체 응답을 돌려준다.
fn call_as(instance: &TastyInstance, token: &str, method: &str, params: Value) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", instance.port())).expect("connect");
    stream
        .set_read_timeout(Some(common::RESPONSE_WAIT))
        .expect("read timeout");
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
        "session_token": token,
    });
    writeln!(stream, "{request}").expect("write request");
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .expect("read response");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("{method}: {e}: {line:?}"))
}

#[cfg(feature = "gui")]
mod gui {
    use std::time::{Duration, Instant};

    use super::*;

    const CONNECT_WAIT: Duration = Duration::from_secs(30);

    fn loopback(server: &TastyInstance) -> String {
        format!("127.0.0.1:{}", server.port())
    }

    fn mirrors(client: &TastyInstance) -> Vec<Value> {
        client
            .call("workspace.list", json!({}))
            .as_array()
            .expect("workspace.list returns a list")
            .iter()
            .filter(|ws| ws["mirror"] == true)
            .cloned()
            .collect()
    }

    fn wait_for_mirror(client: &TastyInstance, name: &str) {
        let started = Instant::now();
        while started.elapsed() < CONNECT_WAIT {
            if mirrors(client).iter().any(|ws| ws["name"] == name) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("no mirror named {name}: {:?}", mirrors(client));
    }

    fn wait_for_run(client: &TastyInstance, token: &str, run: &Value) -> Value {
        let started = Instant::now();
        loop {
            let reply = call_as(client, token, "run.result", json!({ "run": run }));
            let result = &reply["result"];
            if result["done"] == true {
                return result.clone();
            }
            assert!(started.elapsed() < CONNECT_WAIT, "{reply}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// 활성 workspace와 포커스 surface. 호출 전후가 같아야 한다.
    fn user_position(client: &TastyInstance) -> Vec<Value> {
        let info = client.call("system.info", json!({}));
        let active = info["active_workspace_id"].clone();
        assert!(active.is_u64(), "{info}");
        #[cfg(debug_assertions)]
        let surface = {
            let focused = client.call("debug.focused_surface", json!({}));
            assert!(focused["surface_id"].is_u64(), "{focused}");
            focused["surface_id"].clone()
        };
        #[cfg(not(debug_assertions))]
        let surface = Value::Null;
        vec![active, surface]
    }

    /// 대상이 다른 프로세스여야 하므로 서버만 전용 인스턴스로 띄운다. 격리 단위는 workspace라
    /// 기존 workspace와 새 workspace를 한 서버로 잰다.
    #[test]
    fn a_token_without_permissions_makes_mirrors_and_leaves_focus_alone() {
        let server = TastyInstance::spawn();
        let client = common::shared();
        let token = issue_token(client, "no-permissions");
        let remote = server.create_workspace("token-mirror");
        let before = user_position(client);

        let reply = call_as(
            client,
            &token,
            "remote.attach",
            json!({ "ssh": loopback(&server), "remote_workspace": remote.id }),
        );
        let accepted = &reply["result"];
        assert_eq!(accepted["attaching"], true, "{reply}");
        assert_eq!(accepted["remote_workspace"], remote.id, "{reply}");
        assert!(accepted["attempt"].is_u64(), "{reply}");
        let finished = wait_for_run(client, &token, &accepted["run"]);
        assert_eq!(finished["state"], "succeeded", "{finished}");
        wait_for_mirror(client, "token-mirror");
        assert_eq!(user_position(client), before);

        let reply = call_as(
            client,
            &token,
            "remote.attach",
            json!({ "ssh": loopback(&server), "new_workspace": true, "name": "token-new" }),
        );
        let created = &reply["result"];
        assert_eq!(created["created"], true, "{reply}");
        assert!(created["remote_workspace"].is_u64(), "{reply}");
        assert!(created.get("run").is_none(), "{reply}");
        wait_for_mirror(client, "token-new");
        assert_eq!(user_position(client), before);
    }

    #[test]
    fn an_unreachable_target_fails_through_the_run_or_the_reply() {
        let client = common::shared();
        let token = issue_token(client, "no-permissions");
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("addr").port()
        };
        let target = format!("127.0.0.1:{closed}");

        let reply = call_as(
            client,
            &token,
            "remote.attach",
            json!({ "ssh": target, "remote_workspace": 1 }),
        );
        assert_eq!(reply["result"]["attaching"], true, "{reply}");
        let finished = wait_for_run(client, &token, &reply["result"]["run"]);
        assert_eq!(finished["state"], "failed", "{finished}");

        let created = call_as(
            client,
            &token,
            "remote.attach",
            json!({ "ssh": target, "new_workspace": true, "name": "token-unreachable" }),
        );
        assert_eq!(created["error"]["code"], -32050, "{created}");
        assert!(
            !mirrors(client)
                .iter()
                .any(|ws| ws["name"] == "token-unreachable")
        );
    }

    #[test]
    fn the_refusal_list_answers_a_token_without_permissions() {
        let client = common::shared();
        let token = issue_token(client, "no-permissions");
        let reply = call_as(client, &token, "remote.refusals", json!({}));
        assert!(reply["result"]["refusals"].is_array(), "{reply}");
        assert!(reply["result"]["notices"].is_array(), "{reply}");
    }

    #[test]
    fn the_own_port_is_refused_for_a_token_too() {
        let client = common::shared();
        let token = issue_token(client, "no-permissions");
        let reply = call_as(
            client,
            &token,
            "remote.attach",
            json!({ "ssh": loopback(client), "remote_workspace": 1 }),
        );
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
    }
}

/// 게이트를 통과해도 창이 없는 헤드리스는 두 메서드를 지원하지 않는다.
#[cfg(not(feature = "gui"))]
#[test]
fn a_headless_daemon_still_does_not_support_them_for_a_token() {
    let daemon = common::shared();
    let token = issue_token(daemon, "no-permissions");
    for method in ["remote.attach", "remote.refusals"] {
        let reply = call_as(
            daemon,
            &token,
            method,
            json!({ "ssh": "127.0.0.1:1", "remote_workspace": 1 }),
        );
        assert_eq!(reply["error"]["code"], -32017, "{method}: {reply}");
    }
}
