//! remote/debug attach의 공용 세션 처리. mirror-dump는 화면을 재구성해 출력하고,
//! raw bridge는 stdin과 stdout을 연결한다. Ctrl+\로 detach하며 완전한 raw TTY 설정은 하지 않는다.
//! --send 입력은 Loss 복구를 위한 재attach에서 반복하지 않는다.
//! 핸드셰이크 뒤 attached/attached_workspace 또는 attach_error를 받고, Data로 snapshot과 delta를 받는다.
//! force-detach는 force_detached Control과 Detach로 전달된다.
//! ClientLossNotify를 선언하고 Loss를 받으면 다시 attach해 snapshot을 받는다.
//! 구 서버는 이 선언을 무시하므로 손실 통지 없이 기존 방식으로 동작한다.

mod stdin;
#[cfg(test)]
use stdin::{StdinEofLatch, StdinSlot, route_stdin_chunk, route_stdin_eof};
use stdin::{install_sender, stdin_router};

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tasty_ipc::stream::{self, STREAM_PROTO, StreamControl, StreamFrame, StreamTag};
use tasty_terminal::Terminal;

use crate::out::outln;
use crate::ssh::{self, Backoff, PortMode, SshTarget, SshTunnel};
use tasty_ipc::client::StreamConnection;

/// attach 종료 사유. 호출자가 백오프 재연결 여부를 정한다.
pub(crate) enum AttachExit {
    /// 정상 종료(mirror-dump 1회성 완료, raw 의 사용자 detach/EOF, force-detach).
    Completed,
    /// 연결이 예기치 않게 끊김(터널/서버 단절) — 재연결 대상.
    Disconnected,
}

/// 연결 종료 사유. Desynced는 이 모듈에서 재attach하므로 공개 AttachExit에는 포함하지 않는다.
enum SessionEnd {
    Exit(AttachExit),
    /// 서버가 이 연결의 프레임을 버렸다고 알렸고(`StreamControl::Loss`), 옛 연결을 놓았다.
    /// 값은 그 연결에서 통지된 프레임 수의 합.
    Desynced(u64),
}

/// dump의 손실 복구 횟수 상한. 초과하면 수집을 마치고 stderr로 손실을 알린다.
/// raw bridge는 사용자가 종료할 수 있어 이 횟수 제한을 적용하지 않는다.
const DUMP_RESYNC_LIMIT: u32 = 3;

/// 긴 dump 중 서버의 heartbeat timeout으로 점유가 풀리지 않게 Ping을 보낸다.
/// 첫 Ping은 한 주기 뒤에 보내며 raw bridge와 같은 주기를 쓴다.
struct DumpHeartbeat {
    every: Duration,
    next: Instant,
    /// Ping 쓰기가 실패했다 — 연결이 끊겼다는 뜻이라 수집 루프는 `Disconnected` 로 끝낸다.
    broken: bool,
}

impl DumpHeartbeat {
    fn start(now: Instant) -> Self {
        let every = dump_heartbeat_interval();
        Self {
            every,
            next: now + every,
            broken: false,
        }
    }

    /// 수집 루프 머리: 주기가 됐으면 Ping 을 보내고, 다음 대기 시간(창 끝과 다음 Ping 중
    /// 이른 쪽)을 돌려준다. 창이 끝났거나 Ping 을 못 썼으면 `None` — 루프를 끝낸다.
    fn next_wait(&mut self, writer: &mut TcpStream, deadline: Instant) -> Option<Duration> {
        let (ping, wait) = self.plan(dump_now(), deadline)?;
        if ping && let Err(e) = stream::write_frame(writer, StreamTag::Ping, &[]) {
            tracing::warn!("attach: mirror-dump heartbeat was not written: {e}");
            self.broken = true;
            return None;
        }
        Some(wait)
    }

    /// 시각 `now` 의 할 일 — 창이 끝났으면 `None`, 아니면 (Ping 을 보낼지, 다음 대기 시간).
    /// 늦게 깨어나도 Ping 은 한 번만 보내고 다음 주기를 `now` 에서 다시 센다.
    fn plan(&mut self, now: Instant, deadline: Instant) -> Option<(bool, Duration)> {
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            return None;
        }
        let ping = now >= self.next;
        if ping {
            self.next = now + self.every;
        }
        Some((
            ping,
            remaining.min(self.next.saturating_duration_since(now)),
        ))
    }
}

/// 수집 루프가 `wait` 동안 다음 프레임을 기다린다. 시험의 가짜 시계는 실제 경과 대신
/// 루프가 넘긴 이 대기만큼 간다 — 루프가 계산과 다른 시간을 기다리면 Ping 수가 달라진다.
fn dump_recv<T>(rx: &mpsc::Receiver<T>, wait: Duration) -> Result<T, mpsc::RecvTimeoutError> {
    #[cfg(test)]
    dump_heartbeat_tests::advance_fake(wait);
    rx.recv_timeout(wait)
}

/// dump 수집 창과 heartbeat 이 읽는 시계. 시험은 스레드별 가짜 시계를 끼워 스케줄러
/// 지연과 무관하게 Ping 수를 잰다.
fn dump_now() -> Instant {
    #[cfg(test)]
    if let Some(now) = dump_heartbeat_tests::fake_now() {
        return now;
    }
    Instant::now()
}

/// dump heartbeat 주기. 시험의 수집 루프는 주기마다 실제로 `recv_timeout` 을 기다리므로 주기를 줄인다.
fn dump_heartbeat_interval() -> Duration {
    #[cfg(test)]
    {
        Duration::from_millis(50)
    }
    #[cfg(not(test))]
    {
        stream::HEARTBEAT_INTERVAL
    }
}

/// 손실 통지를 받겠다고 선언한다. 선언이 없으면 서버는 그 연결에서 종전대로 조용히
/// 버린다. 보내지 못해도 attach 는 이어간다 — 그 연결은 종전 동작이 될 뿐이다.
fn declare_loss_notify(conn: &mut StreamConnection) {
    let declare = match serde_json::to_vec(&StreamControl::ClientLossNotify {}) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("attach: loss-notify declaration did not serialize: {e}");
            return;
        }
    };
    if let Err(e) = conn.send(StreamTag::Control, &declare) {
        tracing::warn!(
            "attach: loss-notify declaration was not sent; this session will not hear of gaps: {e}"
        );
    }
}

/// mid-session Control 프레임이 이 client 에게 무슨 뜻인가.
enum ControlSignal {
    ForceDetached,
    Loss(u64),
    Other,
}

fn classify_control(payload: &[u8]) -> ControlSignal {
    if let Ok(StreamControl::Loss { frames }) = serde_json::from_slice(payload) {
        return ControlSignal::Loss(frames);
    }
    if String::from_utf8_lossy(payload).contains("force_detached") {
        return ControlSignal::ForceDetached;
    }
    ControlSignal::Other
}

/// loopback 또는 SSH 터널의 로컬 포트에 attach한다. Loss면 다시 연결한다.
/// send는 첫 연결에만 적용해 원격 명령이 중복 실행되지 않게 한다.
pub(crate) fn run_attach_on_port(
    port: u16,
    surface: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    raw: bool,
) -> Result<AttachExit> {
    let mut send = send;
    let mut resyncs = 0u32;
    loop {
        let resync_allowed = raw || resyncs < DUMP_RESYNC_LIMIT;
        match attach_surface_once(port, surface, dump_after, send, raw, resync_allowed)? {
            SessionEnd::Exit(exit) => return Ok(exit),
            SessionEnd::Desynced(frames) => {
                resyncs += 1;
                send = None;
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_count("cli.attach.resyncing", frames, &[&frames.to_string()])
                );
            }
        }
    }
}

/// [`run_attach_on_port`] 의 연결 한 번.
fn attach_surface_once(
    port: u16,
    surface: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    raw: bool,
    resync_allowed: bool,
) -> Result<SessionEnd> {
    let sock = TcpStream::connect(("127.0.0.1", port)).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.request.connect_failed",
                &port.to_string(),
                &e.to_string()
            )
        )
    })?;
    let (mut conn, client_id) = StreamConnection::open_attach(sock, STREAM_PROTO, surface)?;

    // 핸드셰이크 ack 다음의 attach 결과 Control 프레임.
    let first = conn.recv()?;
    if first.tag != StreamTag::Control {
        bail!(
            "{}",
            tasty_i18n::t_fmt(
                "cli.attach.unexpected_first_frame",
                &format!("{:?}", first.tag)
            )
        );
    }
    let ctrl: serde_json::Value = serde_json::from_slice(&first.payload)?;
    match ctrl.get("event").and_then(|v| v.as_str()) {
        Some("attached") => {}
        Some("attach_error") => {
            let reason = ctrl
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            bail!("{}", tasty_i18n::t_fmt("cli.attach.rejected", reason));
        }
        other => bail!(
            "{}",
            tasty_i18n::t_fmt("cli.attach.unexpected_control_event", &format!("{other:?}"))
        ),
    }
    let cols = ctrl.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as usize;
    let rows = ctrl.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
    declare_loss_notify(&mut conn);
    crate::out::errln!(
        "{}",
        tasty_i18n::t_args(
            "cli.attach.attached_surface",
            &[
                &surface.to_string(),
                &client_id.to_string(),
                &cols.to_string(),
                &rows.to_string()
            ]
        )
    );

    if raw {
        run_raw_bridge(conn, send)
    } else {
        run_mirror_dump(conn, cols, rows, dump_after, send, resync_allowed)
    }
}

/// 원격 포트를 찾아 SSH 터널로 attach한다. 끊기면 터널과 연결을 다시 만들며
/// --no-reconnect로 재연결을 끌 수 있다. 원격 세션은 서버에 남는다.
#[allow(clippy::too_many_arguments)]
pub fn run_attach_ssh(
    target: SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
    surface: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    raw: bool,
    reconnect: bool,
) -> Result<()> {
    let ssh = ssh::resolve_ssh_path();
    let dest = target.destination.clone();
    let mode = PortMode::parse(port_mode)?;
    // 자동 검증 한정 host key accept-new. 변수가 없으면 host key 옵션을 넘기지 않고 사용자 ssh 설정을 따른다.
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);

    let mut backoff = Backoff::new();
    loop {
        // ① 원격 포트 발견.
        let remote_port = match ssh::discover_remote_port(
            &ssh,
            &target,
            remote_tasty,
            mode,
            verify,
            debug,
            port_file,
        ) {
            Ok(p) => p,
            Err(e) if reconnect => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.attach.port_discovery_failed_retry", &e.to_string())
                );
                backoff.sleep();
                continue;
            }
            Err(e) => return Err(e),
        };

        // ② ssh -L 터널 (Drop 시 자식 ssh 자동 kill — 원격 데몬은 생존).
        let tunnel = match SshTunnel::establish(&ssh, &target, remote_port, verify) {
            Ok(t) => t,
            Err(e) if reconnect => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.attach.tunnel_failed_retry", &e.to_string())
                );
                backoff.sleep();
                continue;
            }
            Err(e) => return Err(e),
        };
        crate::out::errln!(
            "{}",
            tasty_i18n::t_args(
                "cli.attach.tunnel_established",
                &[
                    &tunnel.local_port.to_string(),
                    &dest,
                    &remote_port.to_string()
                ]
            )
        );
        backoff.reset();

        // ③ 단계 4 attach (터널 localport 로).
        match run_attach_on_port(tunnel.local_port, surface, dump_after, send, raw)? {
            AttachExit::Completed => return Ok(()),
            AttachExit::Disconnected if reconnect => {
                crate::out::errln!("{}", tasty_i18n::t("cli.attach.disconnected_reconnect"));
                drop(tunnel); // 자식 ssh kill 후 재수립.
                backoff.sleep();
                continue;
            }
            AttachExit::Disconnected => return Ok(()),
        }
    }
}

/// workspace attach(로컬/SSH 공용). 핸드셰이크 → `attached_workspace` 디스크립터
/// 파싱 → 터미널마다 mirror 생성 + 비-터미널 placeholder 기록 → demux-dump.
///
/// 손실 통지로 끝난 연결은 [`run_attach_on_port`] 와 같은 규칙으로 다시 붙는다.
pub(crate) fn run_attach_workspace_on_port(
    port: u16,
    workspace: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    send_to: Option<u32>,
) -> Result<AttachExit> {
    let mut send = send;
    let mut resyncs = 0u32;
    loop {
        let resync_allowed = resyncs < DUMP_RESYNC_LIMIT;
        match attach_workspace_once(port, workspace, dump_after, send, send_to, resync_allowed)? {
            SessionEnd::Exit(exit) => return Ok(exit),
            SessionEnd::Desynced(frames) => {
                resyncs += 1;
                send = None;
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_count("cli.attach.resyncing", frames, &[&frames.to_string()])
                );
            }
        }
    }
}

/// [`run_attach_workspace_on_port`] 의 연결 한 번.
fn attach_workspace_once(
    port: u16,
    workspace: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    send_to: Option<u32>,
    resync_allowed: bool,
) -> Result<SessionEnd> {
    let sock = TcpStream::connect(("127.0.0.1", port)).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.request.connect_failed",
                &port.to_string(),
                &e.to_string()
            )
        )
    })?;
    let (mut conn, client_id) =
        StreamConnection::open_attach_workspace(sock, STREAM_PROTO, workspace)?;

    let first = conn.recv()?;
    if first.tag != StreamTag::Control {
        bail!(
            "{}",
            tasty_i18n::t_fmt(
                "cli.attach.unexpected_first_frame",
                &format!("{:?}", first.tag)
            )
        );
    }
    let ctrl: serde_json::Value = serde_json::from_slice(&first.payload)?;
    match ctrl.get("event").and_then(|v| v.as_str()) {
        Some("attached_workspace") => {}
        Some("attach_error") => {
            let reason = ctrl
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            bail!(
                "{}",
                tasty_i18n::t_fmt("cli.attach.workspace_rejected", reason)
            );
        }
        other => bail!(
            "{}",
            tasty_i18n::t_fmt("cli.attach.unexpected_control_event", &format!("{other:?}"))
        ),
    }

    // surfaces 디스크립터 → 터미널 mirror + 비-터미널 placeholder. 트리 순서 보존.
    let surfaces = ctrl
        .get("surfaces")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut mirrors: Vec<(u32, Terminal)> = Vec::new();
    let mut placeholders: Vec<(u32, String)> = Vec::new();
    for s in &surfaces {
        let remote_id = s.get("remote_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        match s.get("role").and_then(|v| v.as_str()) {
            Some("terminal") => {
                let cols = s.get("cols").and_then(|v| v.as_u64()).unwrap_or(80) as usize;
                let rows = s.get("rows").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
                mirrors.push((remote_id, Terminal::new_detached(cols, rows)));
            }
            _ => {
                let kind = s
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                placeholders.push((remote_id, kind));
            }
        }
    }
    crate::out::errln!(
        "{}",
        tasty_i18n::t_args(
            "cli.attach.attached_workspace",
            &[
                &workspace.to_string(),
                &client_id.to_string(),
                &mirrors.len().to_string(),
                &placeholders.len().to_string()
            ]
        )
    );

    declare_loss_notify(&mut conn);
    run_workspace_mirror_dump(
        conn,
        mirrors,
        placeholders,
        dump_after,
        send,
        send_to,
        resync_allowed,
    )
}

/// workspace 단위 원격 attach. surface attach와 포트 발견·터널·백오프를 공유한다.
#[allow(clippy::too_many_arguments)]
pub fn run_attach_workspace_ssh(
    target: SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
    workspace: u32,
    dump_after: Option<u64>,
    send: Option<&str>,
    send_to: Option<u32>,
    reconnect: bool,
) -> Result<()> {
    let ssh = ssh::resolve_ssh_path();
    let dest = target.destination.clone();
    let mode = PortMode::parse(port_mode)?;
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);

    let mut backoff = Backoff::new();
    loop {
        let remote_port = match ssh::discover_remote_port(
            &ssh,
            &target,
            remote_tasty,
            mode,
            verify,
            debug,
            port_file,
        ) {
            Ok(p) => p,
            Err(e) if reconnect => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.attach.port_discovery_failed_retry", &e.to_string())
                );
                backoff.sleep();
                continue;
            }
            Err(e) => return Err(e),
        };

        let tunnel = match SshTunnel::establish(&ssh, &target, remote_port, verify) {
            Ok(t) => t,
            Err(e) if reconnect => {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.attach.tunnel_failed_retry", &e.to_string())
                );
                backoff.sleep();
                continue;
            }
            Err(e) => return Err(e),
        };
        crate::out::errln!(
            "{}",
            tasty_i18n::t_args(
                "cli.attach.tunnel_established",
                &[
                    &tunnel.local_port.to_string(),
                    &dest,
                    &remote_port.to_string()
                ]
            )
        );
        backoff.reset();

        match run_attach_workspace_on_port(tunnel.local_port, workspace, dump_after, send, send_to)?
        {
            AttachExit::Completed => return Ok(()),
            AttachExit::Disconnected if reconnect => {
                crate::out::errln!("{}", tasty_i18n::t("cli.attach.disconnected_reconnect"));
                drop(tunnel);
                backoff.sleep();
                continue;
            }
            AttachExit::Disconnected => return Ok(()),
        }
    }
}

fn send_detach_notice(writer: &mut TcpStream) {
    if let Err(error) = stream::write_frame(writer, StreamTag::Detach, &[]) {
        tracing::debug!(%error, "detach notice could not reach the closing server");
    }
}

fn join_attach_reader(reader: thread::JoinHandle<()>) {
    if reader.join().is_err() {
        tracing::warn!("attach reader thread panicked during cleanup");
    }
}

/// workspace demux-dump: surface-prefixed Data 를 demux 해 각 mirror 에 feed,
/// deadline 후 surface 별 화면을 섹션으로 stdout 출력. 검증 핵심(GUI 없이 N grid 확인).
///
/// 손실 통지를 받으면 `resync_allowed` 일 때 수집을 멈추고 옛 연결을 놓아
/// [`SessionEnd::Desynced`] 를 돌려준다(화면은 찍지 않는다 — 다시 붙어 새로 받는다).
/// 아니면 수집을 이어가고, 끝에 stderr 로 공백이 있다고 알린다.
fn run_workspace_mirror_dump(
    mut conn: StreamConnection,
    mut mirrors: Vec<(u32, Terminal)>,
    placeholders: Vec<(u32, String)>,
    dump_after: Option<u64>,
    send: Option<&str>,
    send_to: Option<u32>,
    resync_allowed: bool,
) -> Result<SessionEnd> {
    let collect_ms = dump_after.unwrap_or(500);

    // 초기 입력 1 회(지정 surface 로 surface-prefixed).
    if let Some(s) = send {
        match send_to {
            Some(sid) => conn.send(
                StreamTag::Data,
                &stream::encode_mux(sid, &decode_escapes(s)),
            )?,
            None => {
                crate::out::errln!("{}", tasty_i18n::t("cli.attach.send_requires_send_to"))
            }
        }
    }

    let mut writer = conn.try_clone_writer()?;
    let (tx, rx) = mpsc::channel::<StreamFrame>();
    let reader = thread::spawn(move || {
        while let Ok(frame) = conn.recv() {
            let stop = frame.tag == StreamTag::Detach;
            if tx.send(frame).is_err() || stop {
                break;
            }
        }
    });

    let deadline = dump_now() + Duration::from_millis(collect_ms);
    let mut forced = false;
    let mut disconnected = false;
    let mut desynced = false;
    let mut lost = 0u64;
    let mut heartbeat = DumpHeartbeat::start(dump_now());
    while let Some(wait) = heartbeat.next_wait(&mut writer, deadline) {
        match dump_recv(&rx, wait) {
            Ok(frame) => match frame.tag {
                StreamTag::Data => {
                    if let Some((sid, payload)) = stream::decode_mux(&frame.payload)
                        && let Some((_, m)) = mirrors.iter_mut().find(|(id, _)| *id == sid)
                    {
                        m.feed_bytes(payload);
                    }
                }
                StreamTag::Control => match classify_control(&frame.payload) {
                    ControlSignal::ForceDetached => {
                        forced = true;
                        break;
                    }
                    ControlSignal::Loss(frames) => {
                        lost += frames;
                        if resync_allowed {
                            desynced = true;
                            break;
                        }
                    }
                    ControlSignal::Other => {}
                },
                StreamTag::Detach => {
                    forced = true;
                    break;
                }
                // 서버 heartbeat — read 자체가 이미 소켓 read timeout 을 리셋하므로 별도
                // 처리 불필요. 반대 방향(client 발)은 위 `heartbeat` 가 보낸다.
                StreamTag::Ping => {}
                // CLI mirror-dump 는 terminal grid 재구성만 한다 — mesh 바이트를
                // 디코드/렌더할 GPU 파이프라인이 없으므로 무시(attach mesh mirror
                // 는 GUI client 전용, `docs/dev-guide/attach-behavior.md` "mesh
                // mirror 채널" 절).
                StreamTag::MeshData => {}
            },
            // deadline 이나 heartbeat 주기 — 루프 머리가 어느 쪽인지 가른다.
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                disconnected = true;
                break;
            }
        }
    }

    disconnected |= heartbeat.broken;
    if desynced {
        return Ok(SessionEnd::Desynced(release_for_resync(
            writer, reader, lost,
        )));
    }

    // 각 surface 화면을 섹션 헤더와 함께 stdout 으로 — 검증 grep 용.
    for (sid, m) in &mirrors {
        outln!("=== surface {sid} ===")?;
        outln!("{}", m.screen_text(true))?;
    }
    for (sid, kind) in &placeholders {
        outln!("=== surface {sid} (placeholder: {kind}) ===")?;
    }

    if !forced && !disconnected {
        send_detach_notice(&mut writer);
    } else if forced {
        crate::out::errln!("{}", tasty_i18n::t("cli.attach.force_detached"));
    }
    join_attach_reader(reader);
    report_unrecovered_loss(lost);
    Ok(SessionEnd::Exit(if disconnected {
        AttachExit::Disconnected
    } else {
        AttachExit::Completed
    }))
}

/// 수집 시간이 끝나면 화면을 출력하고 Completed를 반환한다.
/// 그 전에 reader가 끊기면 Disconnected로 재연결을 요청한다. Loss는 workspace dump와 동일하게 처리한다.
fn run_mirror_dump(
    mut conn: StreamConnection,
    cols: usize,
    rows: usize,
    dump_after: Option<u64>,
    send: Option<&str>,
    resync_allowed: bool,
) -> Result<SessionEnd> {
    let collect_ms = dump_after.unwrap_or(500);

    // 초기 입력 1 회(비대화형 검증용).
    if let Some(s) = send {
        conn.send(StreamTag::Data, &decode_escapes(s))?;
    }

    // reader thread → channel; 메인은 deadline 까지 수집해 mirror 에 feed.
    // (소켓 read timeout 은 프레임 중간에 잘릴 수 있어 thread+channel 로 분리.)
    let mut writer = conn.try_clone_writer()?;
    let (tx, rx) = mpsc::channel::<StreamFrame>();
    let reader = thread::spawn(move || {
        while let Ok(frame) = conn.recv() {
            let stop = frame.tag == StreamTag::Detach;
            if tx.send(frame).is_err() || stop {
                break;
            }
        }
    });

    let mut mirror = Terminal::new_detached(cols, rows);
    let deadline = dump_now() + Duration::from_millis(collect_ms);
    let mut forced = false;
    let mut disconnected = false;
    let mut desynced = false;
    let mut lost = 0u64;
    let mut heartbeat = DumpHeartbeat::start(dump_now());
    while let Some(wait) = heartbeat.next_wait(&mut writer, deadline) {
        match dump_recv(&rx, wait) {
            Ok(frame) => match frame.tag {
                StreamTag::Data => {
                    mirror.feed_bytes(&frame.payload);
                }
                StreamTag::Control => match classify_control(&frame.payload) {
                    ControlSignal::ForceDetached => {
                        forced = true;
                        break;
                    }
                    ControlSignal::Loss(frames) => {
                        lost += frames;
                        if resync_allowed {
                            desynced = true;
                            break;
                        }
                    }
                    ControlSignal::Other => {}
                },
                StreamTag::Detach => {
                    forced = true;
                    break;
                }
                // 서버 heartbeat — read 자체가 이미 소켓 read timeout 을 리셋하므로 별도
                // 처리 불필요. 반대 방향(client 발)은 위 `heartbeat` 가 보낸다.
                StreamTag::Ping => {}
                // 위와 동일 사유 — CLI dump 는 mesh 를 소비하지 않는다.
                StreamTag::MeshData => {}
            },
            // deadline 이나 heartbeat 주기 — 루프 머리가 어느 쪽인지 가른다.
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // reader 스레드 종료 = 소켓 끊김(터널/서버 단절) → 재연결 대상.
                disconnected = true;
                break;
            }
        }
    }

    disconnected |= heartbeat.broken;
    if desynced {
        return Ok(SessionEnd::Desynced(release_for_resync(
            writer, reader, lost,
        )));
    }

    // mirror 화면을 stdout 으로 — 검증 핵심(GUI 없이 grid 확인).
    outln!("{}", mirror.screen_text(true))?;

    // 정상 종료 시 detach 통지(force-detach/단절이면 서버가 이미 끊음).
    if !forced && !disconnected {
        send_detach_notice(&mut writer);
    } else if forced {
        crate::out::errln!("{}", tasty_i18n::t("cli.attach.force_detached"));
    }
    join_attach_reader(reader);
    report_unrecovered_loss(lost);
    Ok(SessionEnd::Exit(if disconnected {
        AttachExit::Disconnected
    } else {
        AttachExit::Completed
    }))
}

/// 손실로 끝내는 dump 의 옛 연결을 놓는다 — `Detach` 를 보내고 서버가 소켓을 닫을
/// 때까지(reader 스레드가 EOF 로 끝날 때까지) 기다린다. 서버는 점유 해제를 inbound 에
/// 넣은 **뒤에** 소켓을 닫으므로, 이 함수가 돌아온 뒤 여는 새 연결의 attach 요청은 그
/// 해제 뒤에 처리된다 — 점유가 옛 연결에 남아 재attach 가 거절되는 경합이 없다.
fn release_for_resync(mut writer: TcpStream, reader: thread::JoinHandle<()>, lost: u64) -> u64 {
    if let Err(e) = stream::write_frame(&mut writer, StreamTag::Detach, &[]) {
        // 이미 끊기는 중이면 서버가 곧 소켓을 닫는다 — 기다림은 그대로 유효하다.
        tracing::warn!("attach: detach before re-attach was not written: {e}");
    }
    if reader.join().is_err() {
        tracing::warn!(
            "attach: reader thread panicked while waiting for the old connection to close"
        );
    }
    lost
}

/// 재attach 한도를 다 쓰고도 손실이 남았으면 stderr 로 알린다 — 위에 찍힌 화면에 공백이
/// 있을 수 있다. stdout 은 건드리지 않는다(검증 스크립트가 그 형식을 grep 한다).
fn report_unrecovered_loss(lost: u64) {
    if lost > 0 {
        crate::out::errln!(
            "{}",
            tasty_i18n::t_count("cli.attach.desync_unrecovered", lost, &[&lost.to_string()])
        );
    }
}

/// stdin과 서버 reader가 보내는 이벤트. 메인 루프는 이 채널에서 기다려 서버 단절도 처리한다.
enum RawEvent {
    Stdin(Vec<u8>),
    StdinEof,
    Server(StreamFrame),
    /// server reader 의 `conn.recv()` 가 `Err` — 조용한 끊김(재연결 대상).
    ServerRecvErr,
}

/// 소켓 writer의 poison은 로그를 남기고 세션을 끝낸다. 복구해서 이어 쓰지 않는다.
/// 이전 쓰기가 프레임 중간에서 끝났을 수 있어 이후 프레임도 잘못 해석될 수 있다.
fn note_writer_poisoned(during: &str) {
    tracing::error!(
        "attach: writer lock poisoned while {during} — a thread panicked while holding it; \
         ending this attach session rather than writing onto a half-written frame"
    );
}

/// stdin→서버, 서버→stdout을 연결한다. Ctrl+\(0x1c)로 detach한다.
/// 서버 reader와 공용 stdin 리더의 이벤트를 채널에서 받아 단절 사유를 반환한다.
/// 세션마다 stdin sender만 교체하며, 전환 중 입력은 유실될 수 있다.
fn run_raw_bridge(conn: StreamConnection, send: Option<&str>) -> Result<SessionEnd> {
    // 입력/Detach/heartbeat 송신용 단일 writer(여러 스레드가 공유 — 프레임 인터리브 방지).
    let writer = Arc::new(Mutex::new(conn.try_clone_writer()?));
    if let Some(s) = send {
        // 아직 이 writer 를 공유하는 스레드가 없다(heartbeat/stdin 라우팅은 아래에서
        // 시작한다) — poison 이 발생할 수 있는 다른 홀더가 존재하지 않는 지점이다.
        let mut w = writer
            .lock()
            .expect("attach writer before any other thread exists");
        stream::write_frame(&mut *w, StreamTag::Data, &decode_escapes(s))?;
        drop(w);
    }

    // heartbeat thread: 이 세션은 raw 브리지라 stdin 이 조용하면(사용자가 그냥 보기만
    // 하는 동안) 오래 idle 할 수 있다 — 주기적으로 Ping 을 보내 서버측 read timeout 을
    // 갱신한다(반대 방향은 서버 write thread 의 동일 로직이 처리). 별도 종료 신호 없이
    // 프로세스 종료(main 함수 반환)에 맡긴다 — CLI 프로세스라 스레드 정리를 기다릴
    // 이유가 없다.
    {
        let writer = writer.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(stream::HEARTBEAT_INTERVAL);
                let sent = match writer.lock() {
                    Ok(mut w) => stream::write_frame(&mut *w, StreamTag::Ping, &[]).is_ok(),
                    Err(_) => {
                        note_writer_poisoned("sending a heartbeat");
                        false
                    }
                };
                if !sent {
                    break;
                }
            }
        });
    }

    let (tx, rx) = mpsc::channel::<RawEvent>();

    // stdin 라우팅: 프로세스 전역 리더 스레드(있으면 재사용, 없으면 최초 1 회
    // 스폰)의 슬롯에 이 세션의 sender 를 설치한다. 새 스레드는 스폰하지 않는다.
    let (slot, eof_latch) = stdin_router();
    install_sender(slot, eof_latch, tx.clone());

    // server reader thread: 서버 출력 → 채널. `conn.recv()` 가 `Err` 면 명시적으로
    // `ServerRecvErr` 를 보낸다(채널 drop 감지에 기대지 않음 — tx clone 이 stdin
    // 라우팅 슬롯에도 남아있어 drop 만으로는 신호가 안 된다).
    {
        let mut conn = conn;
        let tx = tx.clone();
        thread::spawn(move || {
            loop {
                match conn.recv() {
                    Ok(frame) => {
                        let is_detach = frame.tag == StreamTag::Detach;
                        if tx.send(RawEvent::Server(frame)).is_err() || is_detach {
                            break;
                        }
                    }
                    Err(_) => {
                        #[expect(
                            clippy::let_underscore_must_use,
                            reason = "The receiving session or event loop may have already ended."
                        )]
                        // 채널 receiver 가 이미 drop 된 정상 종료 케이스(메인 루프가
                        // stdin EOF/detach 등 다른 사유로 먼저 return) — 송신 실패 무시.
                        let _ = tx.send(RawEvent::ServerRecvErr);
                        break;
                    }
                }
            }
        });
    }
    drop(tx); // 남은 건 stdin 슬롯과 server 스레드가 쥔 clone 뿐 — 원본은 더 필요 없음.

    // main: 채널 이벤트를 기다린다(더 이상 stdin syscall 에 직접 블록하지 않음).
    // `Stdout` 은 쓸 때마다 잠근다 — 이전 코드가 프레임마다 `lock()` 하던 것과 같다.
    // `StdoutLock` 을 넘기면 루프가 도는 내내 잠금을 붙들어 다른 스레드가 막힌다.
    let mut out = std::io::stdout();
    raw_bridge_session(rx, writer, &mut out)
}

/// 세션이 끝나면 mirror의 SGR을 지운다. 첫 화면이 원본의 현재 펜으로 끝나고 PTY 출력도 펜을 남겨서,
/// 그대로 두면 사용자 셸이 그 속성으로 이어진다. 재동기화는 새 첫 화면이 곧 이어지므로 건드리지 않는다.
fn raw_bridge_session(
    rx: mpsc::Receiver<RawEvent>,
    writer: Arc<Mutex<TcpStream>>,
    out: &mut impl Write,
) -> Result<SessionEnd> {
    let end = raw_bridge_main_loop(rx, writer, out)?;
    if matches!(end, SessionEnd::Exit(_))
        && let Err(error) = out.write_all(b"\x1b[0m").and_then(|()| out.flush())
        && error.kind() != std::io::ErrorKind::BrokenPipe
    {
        tracing::warn!(%error, "SGR could not be reset after the raw bridge ended");
    }
    Ok(end)
}

/// 실제 stdin·소켓을 읽는 스레드와 분리한 이벤트 처리. 시험은 RawEvent를 직접 보낸다.
fn raw_bridge_main_loop(
    rx: mpsc::Receiver<RawEvent>,
    writer: Arc<Mutex<TcpStream>>,
    out: &mut impl Write,
) -> Result<SessionEnd> {
    use AttachExit::{Completed, Disconnected};
    let done = |exit| Ok(SessionEnd::Exit(exit));
    loop {
        match rx.recv() {
            Ok(RawEvent::Stdin(data)) => {
                if let Some(pos) = data.iter().position(|&b| b == 0x1c) {
                    // Ctrl+\ 이전 바이트만 보내고 detach.
                    if pos > 0 {
                        match writer.lock() {
                            Ok(mut w) => {
                                if let Err(error) =
                                    stream::write_frame(&mut *w, StreamTag::Data, &data[..pos])
                                {
                                    tracing::warn!(%error, "input could not be flushed before detach");
                                }
                            }
                            Err(_) => note_writer_poisoned("flushing input before detach"),
                        }
                    }
                    match writer.lock() {
                        Ok(mut w) => {
                            if let Err(error) = stream::write_frame(&mut *w, StreamTag::Detach, &[])
                            {
                                tracing::debug!(%error, "detach notice could not reach the closing server");
                            }
                        }
                        Err(_) => note_writer_poisoned("sending the detach notice"),
                    }
                    return done(Completed);
                }
                let write_ok = match writer.lock() {
                    Ok(mut w) => stream::write_frame(&mut *w, StreamTag::Data, &data).is_ok(),
                    Err(_) => {
                        note_writer_poisoned("forwarding stdin");
                        false
                    }
                };
                if !write_ok {
                    return done(Completed);
                }
            }
            Ok(RawEvent::StdinEof) => return done(Completed),
            Ok(RawEvent::Server(frame)) => match frame.tag {
                StreamTag::Data => {
                    if let Err(error) = out.write_all(&frame.payload).and_then(|()| out.flush()) {
                        if error.kind() == std::io::ErrorKind::BrokenPipe {
                            return done(Completed);
                        }
                        return Err(error.into());
                    }
                }
                StreamTag::Detach => return done(Completed),
                StreamTag::Control => match classify_control(&frame.payload) {
                    ControlSignal::ForceDetached => {
                        crate::out::errln!("\r\n{}", tasty_i18n::t("cli.attach.force_detached"));
                        return done(Completed);
                    }
                    // 화면이 이어지지 않는다 — 옛 연결을 놓고 다시 붙는다. 새 snapshot 이
                    // 화면을 처음부터 다시 그린다(docs/dev-guide/attach-behavior.md#밀어내기-실패와-누적-손실).
                    ControlSignal::Loss(frames) => {
                        return Ok(release_raw_for_resync(&rx, &writer, frames));
                    }
                    ControlSignal::Other => {}
                },
                // heartbeat — read 자체가 이미 소켓 read timeout 을 리셋하므로 별도
                // 처리 불필요.
                StreamTag::Ping => {}
                // raw 브리지는 순수 PTY passthrough — mesh 는 소비 대상이 아니다.
                StreamTag::MeshData => {}
            },
            Ok(RawEvent::ServerRecvErr) => return done(Disconnected),
            // 두 송신 스레드가 모두 죽어야만 발생 — 사실상 도달 불가(stdin 스레드는
            // 항상 종료 전 StdinEof 를 보내고, 있는대로 서버 스레드도 ServerRecvErr
            // 를 보낸다).
            Err(_) => return done(Completed),
        }
    }
}

/// Detach를 보내고 서버의 Detach 또는 EOF를 기다린다. 일반 입력은 이 동안 버린다.
/// HEARTBEAT_TIMEOUT이 지나면 다시 attach하므로 서버의 점유 해제와 경합할 수 있다.
/// stdin EOF와 Ctrl+\는 버리지 않고 세션을 끝낸다. 삼키면 다음 세션이 종료 신호를 받지 못한다.
fn release_raw_for_resync(
    rx: &mpsc::Receiver<RawEvent>,
    writer: &Arc<Mutex<TcpStream>>,
    frames: u64,
) -> SessionEnd {
    match writer.lock() {
        Ok(mut w) => {
            if let Err(e) = stream::write_frame(&mut *w, StreamTag::Detach, &[]) {
                tracing::warn!("attach: detach before re-attach was not written: {e}");
            }
        }
        Err(_) => note_writer_poisoned("sending the detach before a re-attach"),
    }
    let deadline = Instant::now() + stream::HEARTBEAT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(RawEvent::ServerRecvErr) => break,
            Ok(RawEvent::Server(f)) if f.tag == StreamTag::Detach => break,
            Ok(RawEvent::StdinEof) => return SessionEnd::Exit(AttachExit::Completed),
            Ok(RawEvent::Stdin(data)) if data.contains(&0x1c) => {
                return SessionEnd::Exit(AttachExit::Completed);
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    "attach: the old connection did not close before the heartbeat timeout; re-attaching anyway"
                );
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    SessionEnd::Desynced(frames)
}

/// 입력 문자열의 escape 를 raw 바이트로 디코딩: `\r \n \t \0 \\ \xNN`.
/// (요청-응답 경로의 `unescape` 와 달리 `\xNN` 제어바이트를 지원 — raw 입력 주입용.)
fn decode_escapes(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                }
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                }
                b'0' => {
                    out.push(0);
                    i += 2;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'x' if i + 3 < bytes.len() => {
                    let hi = (bytes[i + 2] as char).to_digit(16);
                    let lo = (bytes[i + 3] as char).to_digit(16);
                    if let (Some(h), Some(l)) = (hi, lo) {
                        out.push((h * 16 + l) as u8);
                        i += 4;
                    } else {
                        out.push(b'\\');
                        i += 1;
                    }
                }
                _ => {
                    out.push(b'\\');
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::decode_escapes;

    #[test]
    fn decode_basic_escapes() {
        assert_eq!(decode_escapes("echo hi\\r"), b"echo hi\r".to_vec());
        assert_eq!(decode_escapes("a\\tb\\n"), b"a\tb\n".to_vec());
    }

    #[test]
    fn decode_hex_control_bytes() {
        // \x1b = ESC, \x1c = Ctrl+\
        assert_eq!(decode_escapes("\\x1b[A"), vec![0x1b, b'[', b'A']);
        assert_eq!(decode_escapes("x\\x1cy"), vec![b'x', 0x1c, b'y']);
    }

    #[test]
    fn decode_passthrough_unknown() {
        assert_eq!(decode_escapes("a\\qb"), b"a\\qb".to_vec());
        assert_eq!(decode_escapes("plain"), b"plain".to_vec());
    }
}

/// 실제 stdin·소켓 없이 RawEvent로 종료 및 재연결 판단을 검사한다.
#[cfg(test)]
// 이유: 테스트는 let _ = 사유 주석 정책의 대상이 아니며 제품 lint는 유지한다.
#[allow(clippy::let_underscore_must_use)]
mod raw_bridge_tests {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;

    use tasty_ipc::stream::{StreamFrame, StreamTag};

    use super::{
        AttachExit, RawEvent, SessionEnd, StdinEofLatch, StdinSlot, install_sender,
        raw_bridge_main_loop, raw_bridge_session, route_stdin_chunk, route_stdin_eof,
    };

    /// writer.lock() 이 실제로 잠글 대상이 필요할 뿐 내용은 검사하지 않으므로,
    /// loopback 소켓 한쪽을 열어 반대쪽에서 계속 읽어 버림으로써 send-buffer 가
    /// 차 write 가 막히는 일이 없게 한다.
    fn dummy_writer() -> Arc<Mutex<TcpStream>> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let client = TcpStream::connect(addr).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = server_side.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
        });
        Arc::new(Mutex::new(client))
    }

    struct FailedOutput {
        kind: std::io::ErrorKind,
        fail_flush: bool,
    }

    impl std::io::Write for FailedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(std::io::Error::from(self.kind))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from(self.kind))
        }
    }

    /// Whichever way the session ends, the outer terminal does not keep the mirrored pen.
    #[test]
    fn ending_a_session_resets_sgr_after_the_last_output() {
        let ends = [
            RawEvent::Stdin(vec![0x1c]),
            RawEvent::StdinEof,
            RawEvent::Server(StreamFrame::new(StreamTag::Detach, Vec::new())),
            RawEvent::ServerRecvErr,
        ];
        for end in ends {
            let (tx, rx) = mpsc::channel();
            tx.send(RawEvent::Server(StreamFrame::new(
                StreamTag::Data,
                b"\x1b[41;4mX".to_vec(),
            )))
            .unwrap();
            tx.send(end).unwrap();
            let mut out = Vec::new();
            let result = raw_bridge_session(rx, dummy_writer(), &mut out).unwrap();
            assert!(matches!(result, SessionEnd::Exit(_)));
            assert_eq!(out, b"\x1b[41;4mX\x1b[0m");
        }
    }

    /// A resync redraws through the next snapshot, so it writes nothing of its own.
    #[test]
    fn a_resync_leaves_the_output_alone() {
        let (tx, rx) = mpsc::channel();
        let loss =
            serde_json::to_vec(&tasty_ipc::stream::StreamControl::Loss { frames: 1 }).unwrap();
        tx.send(RawEvent::Server(StreamFrame::new(StreamTag::Control, loss)))
            .unwrap();
        tx.send(RawEvent::ServerRecvErr).unwrap();
        let mut out = Vec::new();
        let result = raw_bridge_session(rx, dummy_writer(), &mut out).unwrap();
        assert!(matches!(result, SessionEnd::Desynced(1)));
        assert!(out.is_empty());
    }

    #[test]
    fn a_closed_stdout_at_the_end_of_a_session_is_not_an_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(RawEvent::StdinEof).unwrap();
        let mut out = FailedOutput {
            kind: std::io::ErrorKind::BrokenPipe,
            fail_flush: false,
        };
        let result = raw_bridge_session(rx, dummy_writer(), &mut out).unwrap();
        assert!(matches!(result, SessionEnd::Exit(AttachExit::Completed)));
    }

    #[test]
    fn closed_stdout_finishes_attach_but_other_output_errors_are_returned() {
        for fail_flush in [false, true] {
            for kind in [
                std::io::ErrorKind::BrokenPipe,
                std::io::ErrorKind::PermissionDenied,
            ] {
                let (tx, rx) = mpsc::channel();
                tx.send(RawEvent::Server(StreamFrame::new(
                    StreamTag::Data,
                    b"hello".to_vec(),
                )))
                .unwrap();
                drop(tx);
                let result = raw_bridge_main_loop(
                    rx,
                    dummy_writer(),
                    &mut FailedOutput { kind, fail_flush },
                );
                if kind == std::io::ErrorKind::BrokenPipe {
                    assert!(matches!(
                        result.unwrap(),
                        SessionEnd::Exit(AttachExit::Completed)
                    ));
                } else {
                    assert_eq!(
                        result
                            .err()
                            .expect("non-BrokenPipe errors must be returned")
                            .downcast_ref::<std::io::Error>()
                            .unwrap()
                            .kind(),
                        kind
                    );
                }
            }
        }
    }

    #[test]
    fn server_recv_err_reports_disconnected() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        tx.send(RawEvent::ServerRecvErr).unwrap();
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Disconnected)));
    }

    #[test]
    fn server_detach_frame_reports_completed() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        tx.send(RawEvent::Server(StreamFrame::new(
            StreamTag::Detach,
            vec![],
        )))
        .unwrap();
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Completed)));
    }

    /// (docs/dev-guide/attach-behavior.md#밀어내기-실패와-누적-손실) 손실 통지를 받은 raw 브리지는 옛 연결에 `Detach` 를 쓰고, 서버가
    /// 소켓을 닫았다는 신호를 기다린 뒤 재attach 를 요청한다. 그 사이의 stdin 은 원격에
    /// 안 간다.
    #[test]
    fn a_loss_notice_detaches_the_old_connection_and_asks_for_a_reattach() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let client = TcpStream::connect(listener.local_addr().expect("addr")).expect("connect");
        let (mut server_side, _) = listener.accept().expect("accept");
        let writer = Arc::new(Mutex::new(client));

        let (tx, rx) = mpsc::channel::<RawEvent>();
        let loss = serde_json::to_vec(&tasty_ipc::stream::StreamControl::Loss { frames: 4 })
            .expect("serialize");
        tx.send(RawEvent::Server(StreamFrame::new(StreamTag::Control, loss)))
            .unwrap();
        tx.send(RawEvent::Stdin(b"typed while switching".to_vec()))
            .unwrap();
        tx.send(RawEvent::ServerRecvErr).unwrap();
        drop(tx);

        let end = raw_bridge_main_loop(rx, writer.clone(), &mut Vec::<u8>::new()).unwrap();
        assert!(
            matches!(end, SessionEnd::Desynced(4)),
            "재attach 를 요청해야 한다"
        );

        drop(writer);
        let first = tasty_ipc::stream::read_frame(&mut server_side).expect("a frame");
        assert_eq!(
            first.tag,
            StreamTag::Detach,
            "옛 연결에 Detach 가 먼저 가야 한다"
        );
        assert!(
            tasty_ipc::stream::read_frame(&mut server_side).is_err(),
            "전환 중 stdin 이 옛 연결로 새어 나갔다"
        );
    }

    /// 재attach 창(옛 연결을 놓고 소켓이 닫히길 기다리는 사이)에 stdin 이 닫히면 세션을
    /// 끝낸다. 그 창에서는 슬롯이 아직 이 세션의 sender 를 들고 있어 EOF 전달이 성공하고
    /// latch 가 안 서므로, 여기서 삼키면 다음 세션은 EOF 를 영영 못 받는다.
    #[test]
    fn a_stdin_eof_during_the_resync_window_ends_the_session() {
        let slot: StdinSlot = Arc::new(Mutex::new(None));
        let latch: StdinEofLatch = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<RawEvent>();
        install_sender(&slot, &latch, tx.clone());

        let loss = serde_json::to_vec(&tasty_ipc::stream::StreamControl::Loss { frames: 2 })
            .expect("serialize");
        tx.send(RawEvent::Server(StreamFrame::new(StreamTag::Control, loss)))
            .unwrap();
        route_stdin_eof(&slot, &latch);
        assert!(
            !latch.load(Ordering::Acquire),
            "전제: 창 안의 EOF 는 옛 sender 로 전달돼 latch 에 안 남는다"
        );
        tx.send(RawEvent::ServerRecvErr).unwrap();
        drop(tx);

        let end = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(
            matches!(end, SessionEnd::Exit(AttachExit::Completed)),
            "창 안의 stdin EOF 가 재attach 로 바뀌었다"
        );
    }

    /// 재attach 창에서 누른 detach 키(`Ctrl+\`)도 재attach 가 아니라 종료다.
    #[test]
    fn a_detach_key_during_the_resync_window_ends_the_session() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        let loss = serde_json::to_vec(&tasty_ipc::stream::StreamControl::Loss { frames: 2 })
            .expect("serialize");
        tx.send(RawEvent::Server(StreamFrame::new(StreamTag::Control, loss)))
            .unwrap();
        tx.send(RawEvent::Stdin(b"ab\x1c".to_vec())).unwrap();
        tx.send(RawEvent::ServerRecvErr).unwrap();
        drop(tx);

        let end = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(
            matches!(end, SessionEnd::Exit(AttachExit::Completed)),
            "창 안의 detach 키가 재attach 로 바뀌었다"
        );
    }

    /// 손실 통지와 강제 detach 와 나머지가 서로 섞이지 않는다.
    #[test]
    fn control_frames_are_told_apart() {
        let loss = serde_json::to_vec(&tasty_ipc::stream::StreamControl::Loss { frames: 9 })
            .expect("serialize");
        assert!(matches!(
            super::classify_control(&loss),
            super::ControlSignal::Loss(9)
        ));
        assert!(matches!(
            super::classify_control(br#"{"event":"force_detached"}"#),
            super::ControlSignal::ForceDetached
        ));
        assert!(matches!(
            super::classify_control(br#"{"event":"resize","surface_id":1,"cols":2,"rows":3}"#),
            super::ControlSignal::Other
        ));
    }

    #[test]
    fn server_force_detached_control_reports_completed() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        let payload = br#"{"event":"force_detached"}"#.to_vec();
        tx.send(RawEvent::Server(StreamFrame::new(
            StreamTag::Control,
            payload,
        )))
        .unwrap();
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Completed)));
    }

    #[test]
    fn stdin_eof_reports_completed() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        tx.send(RawEvent::StdinEof).unwrap();
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Completed)));
    }

    #[test]
    fn stdin_detach_key_reports_completed() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        tx.send(RawEvent::Stdin(vec![b'a', 0x1c, b'b'])).unwrap(); // Ctrl+\ 포함
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Completed)));
    }

    /// 서버 프레임(Data)이 먼저 여러 번 오고, 그 다음 단절되는 순서도 정상 처리되는지
    /// — 재연결 전까지 정상 출력이 이어지다 끊김만 감지하는 실사용 패턴.
    #[test]
    fn data_frames_then_server_recv_err_reports_disconnected() {
        let (tx, rx) = mpsc::channel::<RawEvent>();
        tx.send(RawEvent::Server(StreamFrame::new(
            StreamTag::Data,
            b"hello".to_vec(),
        )))
        .unwrap();
        tx.send(RawEvent::Server(StreamFrame::new(StreamTag::Ping, vec![])))
            .unwrap();
        tx.send(RawEvent::ServerRecvErr).unwrap();
        drop(tx);

        let exit = raw_bridge_main_loop(rx, dummy_writer(), &mut Vec::<u8>::new()).unwrap();
        assert!(matches!(exit, SessionEnd::Exit(AttachExit::Disconnected)));
    }

    // --- 단일 영속 stdin 리더의 슬롯 라우팅 회귀 테스트 ---
    // 실제 stdin 을 못 쓰므로 `route_stdin_chunk`/`route_stdin_eof`/`install_sender`
    // 를 직접 호출해 슬롯 교체·ABA 경쟁·EOF latch 를 검증한다.

    /// 슬롯이 비어있는 동안 들어온 입력은 조용히 버려지고, 이후 sender 설치 후
    /// 들어온 입력은 정상 전달된다 — 세션 전환 사이의 유실 창이 "그 청크만 버림"
    /// 으로 국한되고 리더 자체는 계속 살아있음을 보여준다.
    #[test]
    fn route_stdin_chunk_drops_while_slot_empty_then_delivers_after_install() {
        let slot: StdinSlot = Arc::new(Mutex::new(None));

        route_stdin_chunk(&slot, b"lost during gap"); // slot 이 None — 조용히 버려짐.

        let (tx, rx) = mpsc::channel::<RawEvent>();
        *slot.lock().unwrap() = Some(tx);
        route_stdin_chunk(&slot, b"delivered after reconnect");

        match rx.recv().unwrap() {
            RawEvent::Stdin(data) => assert_eq!(data, b"delivered after reconnect"),
            _ => panic!("expected Stdin event"),
        }
    }

    /// ABA 회귀: 죽은(rx 가 이미 drop 된) sender 로의 송신 실패를 처리하는 동안
    /// `route_stdin_chunk` 가 `slot` 을 절대 쓰지 않으므로, 그 사이 이미 설치된
    /// 새 sender 가 지워지지 않는다.
    #[test]
    fn stale_send_failure_does_not_clobber_freshly_installed_sender() {
        let slot: StdinSlot = Arc::new(Mutex::new(None));
        let (stale_tx, stale_rx) = mpsc::channel::<RawEvent>();
        *slot.lock().unwrap() = Some(stale_tx);
        drop(stale_rx); // 이전 세션 종료 — 이 sender 로의 send 는 이제 Err.

        // 죽은 sender 로의 송신 실패 — slot 을 건드리지 않아야 한다(위 불변식).
        route_stdin_chunk(&slot, b"lost - no live receiver");

        let (fresh_tx, fresh_rx) = mpsc::channel::<RawEvent>();
        *slot.lock().unwrap() = Some(fresh_tx); // 새 세션이 install_sender 로 교체했다고 가정.

        route_stdin_chunk(&slot, b"should reach fresh session");
        match fresh_rx.recv().unwrap() {
            RawEvent::Stdin(data) => assert_eq!(data, b"should reach fresh session"),
            _ => panic!("expected Stdin event"),
        }
    }

    /// stdin EOF 가 슬롯이 `None` 인 동안(세션 전환 사이) 발생해도 latch 에
    /// 기억해뒀다가, 이후 `install_sender` 로 새 sender 가 설치되는 시점에 즉시
    /// `StdinEof` 로 전달된다.
    #[test]
    fn stdin_eof_during_gap_is_latched_and_delivered_on_next_install() {
        let slot: StdinSlot = Arc::new(Mutex::new(None));
        let eof_latch: StdinEofLatch = Arc::new(AtomicBool::new(false));

        route_stdin_eof(&slot, &eof_latch); // slot 이 None 인 동안 EOF 발생.
        assert!(eof_latch.load(Ordering::Acquire));

        let (tx, rx) = mpsc::channel::<RawEvent>();
        install_sender(&slot, &eof_latch, tx); // 새 세션 설치 — latch 된 EOF 를 즉시 전달해야 함.

        assert!(matches!(rx.recv().unwrap(), RawEvent::StdinEof));
        assert!(!eof_latch.load(Ordering::Acquire)); // 전달 후 latch 는 내려간다.
    }

    /// 회귀 방지: `StdinSlot` 이 poison 된 뒤에도 `route_stdin_chunk` 가 패닉하지
    /// 않고 계속 진행해야 한다 — poison 되면 이 함수가 도는 상시 리더 스레드가
    /// 영구 사망해 이후 모든 재연결 세션이 stdin 을 못 받게 되기 때문.
    #[test]
    fn route_stdin_chunk_survives_poisoned_slot() {
        let slot: StdinSlot = Arc::new(Mutex::new(None));
        let poisoned = slot.clone();
        let _ = std::thread::spawn(move || {
            let _g = poisoned.lock().unwrap();
            panic!("simulate poison");
        })
        .join(); // Err 무시 — poison 을 의도적으로 남긴다.

        // 패닉하지 않고 정상 진행되면 통과.
        route_stdin_chunk(&slot, b"after poison");
    }
}

/// 서버 시한보다 긴 dump에서 Ping으로 연결을 유지하는지 확인한다.
/// 수집 창과 Ping 주기는 가짜 시계로 잰다 — 실제 시계로 재면 부하로 스레드가 주기보다
/// 오래 못 돌 때 Ping 이 늦어 시험이 실패한다(제품의 주기 5 s·시한 30 s 와 달리 시험 크기에서는
/// 여유가 훨씬 작다). 서버의 read timeout 은 Detach 가 오지 않을 때 멈추지 않게 하는 상한이다.
#[cfg(test)]
mod dump_heartbeat_tests {
    use std::cell::Cell;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    use tasty_ipc::client::StreamConnection;
    use tasty_ipc::stream::{self, STREAM_PROTO, StreamTag};

    use super::{
        AttachExit, DumpHeartbeat, SessionEnd, dump_heartbeat_interval, run_mirror_dump,
        run_workspace_mirror_dump,
    };

    thread_local! {
        /// 설정된 스레드의 가짜 현재 시각. 읽어도 가지 않고 `dump_recv` 의 대기만큼만 간다.
        static FAKE_CLOCK: Cell<Option<Instant>> = const { Cell::new(None) };
    }

    pub(super) fn fake_now() -> Option<Instant> {
        FAKE_CLOCK.with(Cell::get)
    }

    pub(super) fn advance_fake(wait: Duration) {
        FAKE_CLOCK.with(|c| c.set(c.get().map(|now| now + wait)));
    }

    /// `f` 를 가짜 시계로 돌린다. 시험 스레드에만 걸리고 reader 스레드는 시계를 읽지 않는다.
    fn with_fake_clock<R>(f: impl FnOnce() -> R) -> R {
        FAKE_CLOCK.with(|c| c.set(Some(Instant::now())));
        let out = f();
        FAKE_CLOCK.with(|c| c.set(None));
        out
    }

    /// 가짜 서버가 dump 동안 본 것.
    struct Seen {
        pings: usize,
        timed_out: bool,
        detached: bool,
    }

    fn fake_server(listener: TcpListener) -> thread::JoinHandle<Seen> {
        thread::spawn(move || {
            let (sock, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(sock.try_clone().expect("clone"));
            let mut writer = sock;
            let mut line = String::new();
            reader.read_line(&mut line).expect("handshake line");
            let ack = serde_json::json!({ "ok": true, "client_id": 7 });
            stream::write_frame(
                &mut writer,
                StreamTag::Control,
                &serde_json::to_vec(&ack).expect("ack json"),
            )
            .expect("ack");
            reader
                .get_ref()
                .set_read_timeout(Some(Duration::from_secs(30)))
                .expect("read timeout");
            let mut seen = Seen {
                pings: 0,
                timed_out: false,
                detached: false,
            };
            loop {
                match stream::read_frame(&mut reader) {
                    Ok(f) if f.tag == StreamTag::Ping => seen.pings += 1,
                    Ok(f) if f.tag == StreamTag::Detach => {
                        seen.detached = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        seen.timed_out = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
            seen
        })
    }

    /// 수집 창의 주기 수 — 서버 시한(주기 × 6)의 두 배.
    const LONG_DUMP_PERIODS: u32 = 12;

    fn long_dump_ms() -> u64 {
        (dump_heartbeat_interval() * LONG_DUMP_PERIODS).as_millis() as u64
    }

    fn connect() -> (StreamConnection, thread::JoinHandle<Seen>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = fake_server(listener);
        let sock = TcpStream::connect(addr).expect("connect");
        let (conn, _) = StreamConnection::open(sock, STREAM_PROTO).expect("open");
        (conn, server)
    }

    fn assert_kept_alive(end: SessionEnd, server: thread::JoinHandle<Seen>) {
        assert!(matches!(end, SessionEnd::Exit(AttachExit::Completed)));
        let seen = server.join().expect("server thread");
        assert!(!seen.timed_out, "dump 가 끝났는데 Detach 가 오지 않았다");
        assert!(seen.detached, "dump 가 끝나면 Detach 로 놓아야 한다");
        // 가짜 시계는 루프가 기다린 만큼 간다. 첫 Ping 은 한 주기 뒤이고 창 끝에서는 보내지
        // 않으므로, 창 안의 주기 경계마다 한 번씩 창의 주기 수보다 하나 적게 나간다.
        assert_eq!(
            seen.pings,
            (LONG_DUMP_PERIODS - 1) as usize,
            "창 동안 주기마다 Ping 을 보내야 한다"
        );
    }

    #[test]
    fn a_surface_dump_longer_than_the_server_timeout_keeps_the_connection_alive() {
        let (conn, server) = connect();
        let end = with_fake_clock(|| {
            run_mirror_dump(conn, 80, 24, Some(long_dump_ms()), None, false).expect("dump")
        });
        assert_kept_alive(end, server);
    }

    #[test]
    fn a_workspace_dump_longer_than_the_server_timeout_keeps_the_connection_alive() {
        let (conn, server) = connect();
        let end = with_fake_clock(|| {
            run_workspace_mirror_dump(
                conn,
                Vec::new(),
                Vec::new(),
                Some(long_dump_ms()),
                None,
                None,
                false,
            )
            .expect("dump")
        });
        assert_kept_alive(end, server);
    }

    fn heartbeat_at(t0: Instant) -> (DumpHeartbeat, Duration) {
        let every = dump_heartbeat_interval();
        let hb = DumpHeartbeat {
            every,
            next: t0 + every,
            broken: false,
        };
        (hb, every)
    }

    #[test]
    fn the_first_ping_waits_one_period_and_the_wait_ends_at_it() {
        let t0 = Instant::now();
        let (mut hb, every) = heartbeat_at(t0);
        let deadline = t0 + every * 10;
        assert_eq!(hb.plan(t0, deadline), Some((false, every)));
        let quarter = every / 4;
        assert_eq!(
            hb.plan(t0 + quarter, deadline),
            Some((false, every - quarter))
        );
        assert_eq!(hb.plan(t0 + every, deadline), Some((true, every)));
        assert_eq!(hb.plan(t0 + every, deadline), Some((false, every)));
    }

    #[test]
    fn a_late_wakeup_sends_one_ping_and_counts_the_next_period_from_then() {
        let t0 = Instant::now();
        let (mut hb, every) = heartbeat_at(t0);
        let deadline = t0 + every * 10;
        let late = t0 + every * 3 + every / 2;
        assert_eq!(hb.plan(late, deadline), Some((true, every)));
        assert_eq!(hb.plan(late, deadline), Some((false, every)));
        assert_eq!(hb.plan(late + every, deadline), Some((true, every)));
    }

    #[test]
    fn the_wait_stops_at_the_window_end_and_the_window_end_finishes() {
        let t0 = Instant::now();
        let (mut hb, every) = heartbeat_at(t0);
        let deadline = t0 + every / 2;
        assert_eq!(hb.plan(t0, deadline), Some((false, every / 2)));
        assert_eq!(hb.plan(deadline, deadline), None);
        assert_eq!(hb.plan(deadline + every, deadline), None);
    }
}
