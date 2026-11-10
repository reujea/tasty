//! 소켓을 닫지 않고 Ping을 멈춘 뒤 heartbeat TTL로 점유가 회수되는지 확인한다(docs/dev-guide/attach-behavior.md).
//! raw TcpStream을 사용하며 각 시험은 별도 workspace의 surface를 점유한다.
//! 서버는 이 시험과 같은 feature의 바이너리로 실행되므로 GUI·헤드리스 각각의 실행 결과를 구별해야 한다.
//!
//! GUI self-attach는 dispatcher 완료 기록과 connector 진입 수로 확인하고 RTT는 진단으로만 쓴다.
//! 헤드리스의 attach.into_gui 미지원 거절은 별도 시험이며 GUI 거절의 증거가 아니다.
//! 같은 처리 배치의 disconnect·재attach 순서는 core::attach의 합성 시험과 소스 가드에서 따로 확인한다.

mod attach_common;
mod common;

use std::time::{Duration, Instant};

use attach_common::open_surface_attach;
use common::TastyInstance;
use serde_json::json;

// 제품의 heartbeat 제한을 참조하고 회수 폴링에는 추가 여유를 둔다.
const HEARTBEAT_TIMEOUT_HINT: Duration = tasty_ipc::stream::HEARTBEAT_TIMEOUT;
const RELEASE_POLL_TIMEOUT: Duration = Duration::from_secs(55);

fn is_attached(instance: &TastyInstance, surface_id: u64) -> bool {
    let surfaces = instance.call("surface.list", json!({}));
    surfaces
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"].as_u64() == Some(surface_id))
        .and_then(|s| s["attached"].as_bool())
        .unwrap_or(false)
}

#[test]
fn silent_disconnect_releases_occupancy_via_heartbeat_ttl() {
    let server = common::shared();
    let sid = server.create_workspace("silent-disconnect").surface_id;

    assert!(!is_attached(server, sid), "surface must start unattached");

    let (stale_conn, ctrl) = open_surface_attach(server.port(), sid);
    assert_eq!(
        ctrl["event"].as_str(),
        Some("attached"),
        "attach should succeed: {ctrl:?}"
    );
    assert!(
        is_attached(server, sid),
        "surface must show attached after a successful attach"
    );

    let (_rejected_conn, reject_ctrl) = open_surface_attach(server.port(), sid);
    assert_eq!(
        reject_ctrl["event"].as_str(),
        Some("attach_error"),
        "second attach must be rejected while the first holds the lock: {reject_ctrl:?}"
    );
    drop(_rejected_conn); // EOF 로 정리 — 거부된 연결이라 점유와 무관.

    // FIN을 보내지 않고 유지해 EOF 대신 읽기 타임아웃에 따른 회수를 검증한다.
    let held_since = Instant::now();

    let deadline = Instant::now() + RELEASE_POLL_TIMEOUT;
    let mut released = false;
    while Instant::now() < deadline {
        if !is_attached(server, sid) {
            released = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        released,
        "occupancy lock was not released within {:?} of a silent disconnect \
         (heartbeat TTL hint: {:?})",
        RELEASE_POLL_TIMEOUT, HEARTBEAT_TIMEOUT_HINT
    );
    assert!(
        held_since.elapsed() >= Duration::from_secs(5),
        "sanity: release should not be instantaneous (would indicate EOF, not TTL)"
    );

    let (_reattached_conn, reattach_ctrl) = open_surface_attach(server.port(), sid);
    assert_eq!(
        reattach_ctrl["event"].as_str(),
        Some("attached"),
        "re-attach after TTL release should succeed: {reattach_ctrl:?}"
    );
    assert!(is_attached(server, sid));

    drop(stale_conn); // 정리 — 이미 서버측에서 release 됐으므로 이제 닫아도 무해.
}

/// 회수까지 걸린 시간과 폴링 횟수. 제한 안에 회수되지 않으면 None.
struct FreeWait {
    elapsed: Duration,
    polls: usize,
}

const FREE_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 대기 예산이 단언의 문턱보다 커야 느린 회수도 시간값으로 비교할 수 있다. 폴링 자체가 늦었는지 판단할 자료로 횟수도 남긴다.
fn wait_until_free(server: &TastyInstance, surface_id: u64, within: Duration) -> Option<FreeWait> {
    let t0 = Instant::now();
    let mut polls = 0usize;
    while t0.elapsed() < within {
        polls += 1;
        if !is_attached(server, surface_id) {
            return Some(FreeWait {
                elapsed: t0.elapsed(),
                polls,
            });
        }
        std::thread::sleep(FREE_POLL_INTERVAL);
    }
    None
}

/// 프로토콜 불일치 연결을 유지한 채 점유가 없는지 본다. 먼저 닫으면 EOF 정리가 점유를 회수해, 거절됐어야 할 연결이 점유를 얻은 오류를 놓칠 수 있다.
#[test]
fn proto_mismatch_never_takes_occupancy_and_leaves_the_workspace_attachable() {
    let server = common::shared();
    let ws = server.create_workspace("proto-mismatch-no-occupancy");
    assert!(!is_attached(server, ws.surface_id), "시작은 비점유");

    let _hung_peer = attach_common::raw_open_workspace_proto(server.port(), ws.id, 999);

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        assert!(
            !is_attached(server, ws.surface_id),
            "proto 가 맞지 않는 client 는 점유를 잡으면 안 된다 — 그 점유를 쓸 수 없다"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let outcome = attach_common::try_open_workspace_attach(server.port(), ws.id);
    assert_eq!(
        outcome, "attached_workspace",
        "실패한 attach 가 정상 attach 를 막으면 안 된다"
    );
    // 헬퍼가 연결을 닫으므로 이후 점유값은 EOF 정리와 경합한다. 여기서는 attach 성공 결과만 확인한다.
}

/// 디스크립터를 읽기 전에 소켓을 닫고 TTL을 기다리지 않고 점유가 회수되는지 확인한다.
#[test]
fn closing_before_the_descriptor_releases_occupancy_promptly() {
    let server = common::shared();
    let ws = server.create_workspace("close-before-descriptor");

    drop(attach_common::raw_open_workspace_no_read(
        server.port(),
        ws.id,
    ));

    let budget = HEARTBEAT_TIMEOUT_HINT + Duration::from_secs(5);
    let freed = wait_until_free(server, ws.surface_id, budget);
    let freed = freed.expect("EOF 이후 점유가 회수되지 않았다");
    let expected_polls = budget.as_millis() / FREE_POLL_INTERVAL.as_millis();
    assert!(
        freed.elapsed < HEARTBEAT_TIMEOUT_HINT,
        "EOF 뒤 회수가 TTL {HEARTBEAT_TIMEOUT_HINT:?}보다 늦었다(경과 {:?}, 폴 {}회, 예산상 최대 {expected_polls}회). 폴링 지연과 서버의 회수 지연을 함께 확인한다. 낮은 폴 수만으로 원인을 단정하지 않는다.",
        freed.elapsed,
        freed.polls
    );

    let outcome = attach_common::try_open_workspace_attach(server.port(), ws.id);
    assert_eq!(
        outcome, "attached_workspace",
        "재attach 가 곧바로 성공해야 한다"
    );
}

/// 스트림은 SSH·loopback을 신뢰 경계로 쓰며 handshake의 session_token을 무시한다. 잘못된 토큰으로도 attach가 성립하는지 확인한다.
#[test]
fn the_stream_channel_ignores_session_token_so_auth_cannot_strand_occupancy() {
    let server = common::shared();
    let ws = server.create_workspace("stream-ignores-token");

    let outcome =
        attach_common::try_open_workspace_attach_with_token(server.port(), ws.id, "bogus-token");
    assert_eq!(
        outcome, "attached_workspace",
        "스트림 채널은 session_token 을 보지 않는다 — 토큰 기반 거절 경로가 없다"
    );
}

/// GUI dispatcher 완료 기록의 source 값. `AttachSource::label`과 같아야 한다.
#[cfg(debug_assertions)]
const INTO_GUI_SOURCE: &str = "attach.into_gui";

/// 유일한 사용처인 remote.attach 거절 시험이 gui 조합에만 있다.
#[cfg(all(feature = "gui", debug_assertions))]
const ENDPOINT_SOURCE: &str = "remote.attach/auto-attach";

#[cfg(debug_assertions)]
fn dispatch_completion(
    server: &TastyInstance,
    workspace: u64,
    source: &str,
) -> Option<serde_json::Value> {
    let parse = |line: &str| {
        line.split_once("attach_dispatch_completed ")
            .and_then(|(_, record)| serde_json::from_str::<serde_json::Value>(record).ok())
    };
    let line = server.find_stderr(|line| {
        parse(line).is_some_and(|record| {
            record["port"] == server.port()
                && record["workspace"] == workspace
                && record["source"] == source
        })
    })?;
    parse(&line)
}

/// 늦게 처리되는 일도 관측하도록 요청 몇 개로 이벤트 루프를 돌린다.
fn settle(server: &TastyInstance) {
    for _ in 0..10 {
        server.call("ui.state", json!({}));
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 자기 포트 거절이 처리 시점의 인자 오류로 오는지 확인한다. 헤드리스는 이 판정 전에 거절한다.
#[cfg(feature = "gui")]
fn assert_own_port_refusal(reply: &serde_json::Value) {
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    let message = reply["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("own IPC port"), "{reply}");
}

/// 자기 포트를 가리키는 attach.into_gui는 큐에 넣기 전에 오류로 응답한다.
/// GUI dispatcher와 connector에는 도달하지 않는다. 헤드리스는 아래 미지원 거절 시험이 맡는다.
#[cfg(feature = "gui")]
#[test]
fn attach_into_gui_to_the_own_port_is_refused_before_queueing() {
    let server = common::shared();
    let ws = server.create_workspace("self-attach-refused");
    let reply = server.call_raw(
        "attach.into_gui",
        json!({ "port": server.port(), "workspace": ws.id }),
    );
    assert_own_port_refusal(&reply);
    settle(server);
    #[cfg(debug_assertions)]
    assert_eq!(dispatch_completion(server, ws.id, INTO_GUI_SOURCE), None);
    assert!(!is_attached(server, ws.surface_id));
    let outcome = attach_common::try_open_workspace_attach(server.port(), ws.id);
    assert_eq!(outcome, "attached_workspace");
}

/// 다른 포트에서 자기 IPC 포트로 바이트를 그대로 넘기는 중계. 사용자가 연 `ssh -L` 터널을 대신한다.
#[cfg(feature = "gui")]
fn relay_to(target: u16) -> u16 {
    use std::net::{TcpListener, TcpStream};
    let listener = TcpListener::bind("127.0.0.1:0").expect("relay listener");
    let port = listener.local_addr().expect("relay addr").port();
    std::thread::spawn(move || {
        for inbound in listener.incoming() {
            let Ok(inbound) = inbound else { return };
            let Ok(outbound) = TcpStream::connect(("127.0.0.1", target)) else {
                return;
            };
            let (mut a_read, mut b_write) = (
                inbound.try_clone().expect("clone inbound"),
                outbound.try_clone().expect("clone outbound"),
            );
            let (mut b_read, mut a_write) = (outbound, inbound);
            std::thread::spawn(move || std::io::copy(&mut a_read, &mut b_write));
            std::thread::spawn(move || std::io::copy(&mut b_read, &mut a_write));
        }
    });
    port
}

/// 다른 포트를 거쳐 자기 자신에 닿는 attach.into_gui는 상대의 instance_id로 판정해 오류로 응답한다.
/// 헤드리스는 자기 포트만 비교하고 큐를 처리하지 않으므로 GUI에서만 확인한다.
#[cfg(feature = "gui")]
#[test]
fn attach_into_gui_through_a_tunnel_to_itself_is_refused_before_queueing() {
    let server = common::shared();
    let ws = server.create_workspace("tunnel-self-attach-refused");
    let reply = server.call_raw(
        "attach.into_gui",
        json!({ "port": relay_to(server.port()), "workspace": ws.id }),
    );
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    let message = reply["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("reaches this instance itself"), "{reply}");
    settle(server);
    #[cfg(debug_assertions)]
    assert_eq!(dispatch_completion(server, ws.id, INTO_GUI_SOURCE), None);
    assert!(!is_attached(server, ws.surface_id));
}

/// IPC remote.attach가 loopback 주소로 자기 포트를 가리키면 연결 시도를 시작하기 전에 오류로 응답한다.
#[cfg(feature = "gui")]
#[test]
fn remote_attach_to_the_own_port_is_refused_before_any_attempt() {
    let server = common::shared();
    let ws = server.create_workspace("remote-attach-self-refused");
    let reply = server.call_raw(
        "remote.attach",
        json!({
            "ssh": format!("127.0.0.1:{}", server.port()),
            "remote_workspace": ws.id,
        }),
    );
    assert_own_port_refusal(&reply);
    settle(server);
    #[cfg(debug_assertions)]
    assert_eq!(dispatch_completion(server, ws.id, ENDPOINT_SOURCE), None);
    assert!(!is_attached(server, ws.surface_id));
}

/// system.info는 프로세스마다 다른 instance_id를 준다. SSH 너머의 상대가 자기 자신인지
/// 판정하는 근거라서, 같은 서버에서는 같고 이 시험 프로세스의 값과는 다르다.
#[test]
fn system_info_reports_a_per_process_instance_id() {
    let server = common::shared();
    let first = server.call("system.info", json!({}));
    let id = first["instance_id"]
        .as_str()
        .expect("system.info carries instance_id");
    assert_eq!(id.len(), 32, "{first}");
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()), "{first}");
    assert_ne!(id, tasty_ipc::instance::instance_id());
    assert_eq!(server.call("system.info", json!({}))["instance_id"], id);
}

/// 자기 포트로 새 워크스페이스 attach를 요청하면 원격(=자기)에 워크스페이스를 만들기 전에 거절한다.
#[cfg(feature = "gui")]
#[test]
fn remote_attach_new_workspace_to_the_own_port_creates_nothing() {
    let server = common::shared();
    let name = format!("self-create-probe-{}", std::process::id());
    let reply = server.call_raw(
        "remote.attach",
        json!({
            "ssh": format!("127.0.0.1:{}", server.port()),
            "new_workspace": true,
            "name": name,
        }),
    );
    settle(server);
    let created = server
        .call("workspace.list", json!({}))
        .as_array()
        .expect("workspace.list returns a list")
        .iter()
        .filter(|ws| ws["name"] == name.as_str())
        .count();
    assert_eq!(
        created, 0,
        "a workspace was created on this instance; reply={reply}"
    );
    assert_own_port_refusal(&reply);
}

/// 헤드리스에는 GUI attach 큐를 처리할 쪽이 없으므로 자기 포트든 다른 포트든 접수하지 않고
/// 이름을 실은 미지원 오류로 거절한다. 거절 뒤에도 점유를 잡지 않는다.
#[cfg(not(feature = "gui"))]
#[test]
fn a_headless_daemon_refuses_gui_attach_without_taking_occupancy() {
    let server = common::shared();
    let ws = server.create_workspace("headless-gui-attach-refused");
    for port in [server.port(), server.port().wrapping_add(1)] {
        let reply = server.call_raw(
            "attach.into_gui",
            json!({ "port": port, "workspace": ws.id }),
        );
        assert!(
            reply.get("result").is_none(),
            "처리하지 않을 into_gui 를 접수했다(port {port}): {reply}"
        );
        assert_eq!(reply["error"]["code"], -32017, "port {port}: {reply}");
        let message = reply["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("attach.into_gui"), "{reply}");
    }
    settle(server);
    #[cfg(debug_assertions)]
    assert_eq!(dispatch_completion(server, ws.id, INTO_GUI_SOURCE), None);
    assert!(!is_attached(server, ws.surface_id));
    let outcome = attach_common::try_open_workspace_attach(server.port(), ws.id);
    assert_eq!(outcome, "attached_workspace");
}

/// heartbeat를 보내는 연결은 제품 TTL을 넘겨도 프레임을 읽고 점유를 유지해야 한다.
#[test]
fn a_heartbeating_client_outlives_the_silence_ttl() {
    let server = common::shared();
    let ws = server.create_workspace("heartbeat-outlives-ttl");
    let mut stream = attach_common::open_workspace_attach(server.port(), ws.id);

    std::thread::sleep(tasty_ipc::stream::HEARTBEAT_TIMEOUT + Duration::from_secs(3));

    let read = attach_common::read_frame_result(&mut stream);
    assert!(
        read.is_ok(),
        "heartbeat 연결에서 TTL 뒤 프레임 읽기에 실패했다: {:?}",
        read.err()
    );

    let outcome = attach_common::try_open_workspace_attach(server.port(), ws.id);
    assert_eq!(
        outcome, "attach_error:already_attached",
        "연결이 살아 있으면 그 workspace 는 여전히 점유 중이어야 한다"
    );
}
