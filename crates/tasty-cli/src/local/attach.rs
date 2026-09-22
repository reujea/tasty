//! `tasty attach <surface>` — surface 단위 attach client (attach/detach 단계 4).
//!
//! 두 모드:
//! - **mirror-dump**(기본, `--dump-after`): attach 후 일정 시간 출력을 수집해 mirror
//!   Terminal 로 grid 를 재구성하고 `screen_text` 를 stdout 으로 출력한다. GUI 없이
//!   로컬 loopback e2e 를 자동 검증하는 핵심 경로(초기 스냅샷 + 출력 delta 확인).
//! - **raw 브리지**(`--raw`): stdin↔stdout passthrough. detach 전용 키 `Ctrl+\`
//!   (decisions #8). 완전한 raw TTY 모드는 단계 4 옵션(여기선 기본 passthrough).
//!
//! `--send` 로 attach 직후 1 회 비대화형 입력을 보낼 수 있다(escape 디코딩) —
//! raw TTY 없이 입력 라우팅을 검증하기 위함.
//!
//! 핸드셰이크(`stream.open{target}`) 직후 서버는 attach 결과를 Control 프레임으로
//! 통지한다(`attached{cols,rows}` 또는 `attach_error{reason}`). 그 다음 Data 프레임이
//! 초기 스냅샷 + 이후 출력 delta. force-detach 는 `Control{force_detached}`+`Detach`.
//!
//! 세 루프 모두 attach 직후 손실 통지를 받겠다고 선언한다(`ClientLossNotify`). 서버가
//! `Loss` 를 보내면 그 연결의 화면은 이어지지 않으므로 옛 연결을 놓고 **다시 attach** 해
//! 새 snapshot 을 받는다 — 서버가 snapshot 을 만드는 자리는 attach 하나뿐이다
//! (`docs/adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md`).
//! 구 서버는 선언을 모르는 변종으로 무시하고 `Loss` 도 안 보내므로 종전 동작 그대로다.

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

/// 한 attach 세션이 끝난 사유(백오프 재연결 판단용 — 단계 5).
pub(crate) enum AttachExit {
    /// 정상 종료(mirror-dump 1회성 완료, raw 의 사용자 detach/EOF, force-detach).
    Completed,
    /// 연결이 예기치 않게 끊김(터널/서버 단절) — 재연결 대상.
    Disconnected,
}

/// 한 연결이 끝난 사유 — [`AttachExit`] 에 **재attach** 하나를 더한다. 재attach 는 이
/// 파일 안에서 끝나는 일이라(`run_attach_on_port` · `run_attach_workspace_on_port` 가 다시
/// 붙는다) 호출자에게 보이는 [`AttachExit`] 에는 없다.
enum SessionEnd {
    Exit(AttachExit),
    /// 서버가 이 연결의 프레임을 버렸다고 알렸고(`StreamControl::Loss`), 옛 연결을 놓았다.
    /// 값은 그 연결에서 통지된 프레임 수의 합.
    Desynced(u64),
}

/// mirror-dump 한 번의 실행에서 손실로 다시 attach 하는 최대 횟수. 넘으면 수집을 이어가
/// 결과를 찍고 stderr 로 공백이 있다고 알린다(ADR-0400). raw 브리지에는 걸지 않는다 —
/// 대화형이라 사용자가 `Ctrl+\` 로 끝낼 수 있다.
const DUMP_RESYNC_LIMIT: u32 = 3;

/// mirror-dump 수집 루프가 보내는 client 발 heartbeat. 서버는 attach 소켓에
/// [`stream::HEARTBEAT_TIMEOUT`] read timeout 을 걸어, 그동안 client 프레임이 하나도 안 오면
/// 끊긴 연결로 보고 점유를 푼다. dump 는 `--send` 한 번 뒤로 아무것도 안 보내므로 이것이
/// 없으면 그 시한보다 긴 `--dump-after` 가 오류 없이 도중에 풀린다 — 그래서 raw 브리지와
/// 같은 주기로 `Ping` 을 보낸다. 첫 Ping 은 시작 후 한 주기 뒤라 그보다 짧은 dump 의
/// 송신 프레임은 종전과 같다(`docs/adr/0529-a-mirror-dump-longer-than-the-heartbeat-timeout-sends-heartbeats.md`).
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
        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            return None;
        }
        if now >= self.next {
            if let Err(e) = stream::write_frame(writer, StreamTag::Ping, &[]) {
                tracing::warn!("attach: mirror-dump heartbeat was not written: {e}");
                self.broken = true;
                return None;
            }
            self.next = now + self.every;
        }
        Some(remaining.min(self.next.saturating_duration_since(now)))
    }
}

/// dump heartbeat 주기. 시험은 서버 시한을 줄인 가짜 서버로 재므로 주기도 같은 비율로 줄인다.
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

/// 단일 attach 세션: `127.0.0.1:port` 접속 → 핸드셰이크 → mirror/raw.
/// 로컬(loopback)과 SSH(터널 localport) 양쪽이 공유한다 — SSH 경로는 이 함수에
/// **터널의 localport** 를 넘기기만 한다(O7: `--port` 공개 플래그 불필요).
///
/// 손실 통지로 끝난 연결은 여기서 다시 붙는다(ADR-0400). `send` 입력은 첫 attach 에서만
/// 보낸다 — 재attach 가 입력을 되풀이하면 원격에서 명령이 두 번 돈다.
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
                    tasty_i18n::t_fmt("cli.attach.resyncing", &frames.to_string())
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

/// `tasty attach --ssh user@host <surface>` (1회성 SSH 터널 attach — 단계 5).
///
/// ① 원격 포트 발견(auto fallback 체인) → ② `ssh -L` 터널 수립 → ③ 터널 localport 로
/// 단계 4 attach. SSH 끊김 시 백오프 재연결(decisions 7, `--no-reconnect` 로 off).
/// 세션은 서버에 상주하므로 재연결은 터널 재수립 + 재attach 만 하면 복구된다.
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
    // 자동 검증(Claude Bash) 한정 host key accept-new. 평상시는 기본 strict 유지(보안).
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
                    tasty_i18n::t_fmt("cli.attach.resyncing", &frames.to_string())
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

/// `tasty attach --ssh user@host --workspace <id>` (1회성 SSH 터널 workspace attach).
/// 단계 5 의 터널/포트발견/백오프를 그대로 재사용 — surface 단위와 동일한 SSH 경로.
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

    let deadline = Instant::now() + Duration::from_millis(collect_ms);
    let mut forced = false;
    let mut disconnected = false;
    let mut desynced = false;
    let mut lost = 0u64;
    let mut heartbeat = DumpHeartbeat::start(Instant::now());
    while let Some(wait) = heartbeat.next_wait(&mut writer, deadline) {
        match rx.recv_timeout(wait) {
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
        let _ = stream::write_frame(&mut writer, StreamTag::Detach, &[]); // best-effort detach 통지 — 무시
    } else if forced {
        crate::out::errln!("{}", tasty_i18n::t("cli.attach.force_detached"));
    }
    let _ = reader.join(); // reader 스레드 join 실패(패닉) 무시 — 종료 경로
    report_unrecovered_loss(lost);
    Ok(SessionEnd::Exit(if disconnected {
        AttachExit::Disconnected
    } else {
        AttachExit::Completed
    }))
}

/// mirror-dump 모드: 출력을 수집해 mirror grid 재구성 → stdout 출력.
/// 1회성이라 항상 `Completed` 를 반환하지만, deadline 전에 reader 가 끊기면
/// `Disconnected`(터널/서버 단절)로 보고해 SSH 재연결이 가능하게 한다.
///
/// 손실 통지는 [`run_workspace_mirror_dump`] 와 같은 규칙이다.
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
    let deadline = Instant::now() + Duration::from_millis(collect_ms);
    let mut forced = false;
    let mut disconnected = false;
    let mut desynced = false;
    let mut lost = 0u64;
    let mut heartbeat = DumpHeartbeat::start(Instant::now());
    while let Some(wait) = heartbeat.next_wait(&mut writer, deadline) {
        match rx.recv_timeout(wait) {
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
        let _ = stream::write_frame(&mut writer, StreamTag::Detach, &[]); // best-effort detach 통지 — 무시
    } else if forced {
        crate::out::errln!("{}", tasty_i18n::t("cli.attach.force_detached"));
    }
    let _ = reader.join(); // reader 스레드 join 실패(패닉) 무시 — 종료 경로
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
            tasty_i18n::t_fmt("cli.attach.desync_unrecovered", &lost.to_string())
        );
    }
}

/// raw 브리지 내부 이벤트 — stdin 스레드와 server reader 스레드가 하나의 채널에
/// merge 해 보낸다. main 은 이 채널에만 블록하므로 더 이상 stdin syscall 에 직접
/// 갇히지 않고, 서버 쪽 단절을 즉시 감지해 `AttachExit::Disconnected` 를 반환할 수
/// 있다(mirror-dump 의 `rx.recv_timeout` 패턴과 동일한 아이디어 — 여기선 deadline
/// 이 없으므로 `recv()`).
enum RawEvent {
    Stdin(Vec<u8>),
    StdinEof,
    Server(StreamFrame),
    /// server reader 의 `conn.recv()` 가 `Err` — 조용한 끊김(재연결 대상).
    ServerRecvErr,
}

/// raw 브리지의 stdin 라우팅 슬롯 — 현재 활성 세션의 sender. **쓰기는
/// [`install_sender`] 만 수행한다**(`docs/dev-guide/attach-behavior.md` "SSH 터널"
/// "stdin 라우팅(단일 영속 리더 + 슬롯 교체)" 절 불변식) — 리더 스레드는 읽기만
/// 해서 ABA 경쟁을 피한다: 죽은 sender 로의 송신이 실패했다고 리더가 슬롯을
/// 되돌리면, 그 사이 이미 설치된 새 세션의 sender 를 지워버릴 수 있다.
type StdinSlot = Arc<Mutex<Option<mpsc::Sender<RawEvent>>>>;

/// stdin 슬롯 poison 을 보고했는가(첫 1 회만 — poison 은 sticky 라 이후 모든 청크가
/// 같은 경로를 탄다).
static STDIN_SLOT_POISON_REPORTED: AtomicBool = AtomicBool::new(false);

/// stdin 슬롯 락을 poison 이어도 잡는다. 세 호출부(청크 라우팅 · EOF 라우팅 ·
/// sender 설치)의 근거는 각 함수 doc 에 있고, 공통점은 **여기서 패닉하면 프로세스가
/// 영구 사망**한다는 것이다. 복구를 택하더라도 관측은 남긴다
/// (`docs/dev-guide/error-handling.md` "어느 선택을 하든 로그를 남긴다").
///
/// `tasty_utils::poison` 헬퍼를 쓰지 않는 이유는 이 파일의 형제 지점인 writer 락이
/// **복구가 오답**이라 반대 선택을 하기 때문이다 — 한 파일에 두 형태를 섞으면 다음
/// 사람이 "여기도 헬퍼면 되겠네" 로 잘못 통일하기 쉽다.
fn lock_stdin_slot(slot: &StdinSlot) -> std::sync::MutexGuard<'_, Option<mpsc::Sender<RawEvent>>> {
    slot.lock().unwrap_or_else(|p| {
        if !STDIN_SLOT_POISON_REPORTED.swap(true, Ordering::Relaxed) {
            tracing::error!(
                "attach: stdin slot lock poisoned — a thread panicked while holding it; \
                 recovering (the slot holds a plain `Option<Sender>`), later occurrences \
                 are not logged"
            );
        }
        p.into_inner()
    })
}

/// 슬롯이 비어있는(세션 전환 중) 동안 발생한 진짜 stdin EOF/에러를 기억해두는
/// latch. 리더 스레드가 세우고, [`install_sender`] 가 다음 세션 설치 시 확인해
/// 즉시 `RawEvent::StdinEof` 를 전달한다 — 서버 단절→재연결 전환과 진짜 stdin
/// EOF 가 겹치는 경쟁 대응(그렇지 않으면 EOF 통지가 영영 유실돼, 다음 세션이
/// 이미 닫힌 stdin 을 무한정 기다리게 될 수 있다).
type StdinEofLatch = Arc<AtomicBool>;

/// 프로세스 생애주기 동안 단 하나만 존재하는 stdin 리더 스레드를 시작한다
/// (`docs/dev-guide/attach-behavior.md` "SSH 터널" "stdin 라우팅(단일 영속 리더 +
/// 슬롯 교체)" 절) — `run_raw_bridge` 가 재연결마다 새 스레드를 스폰하던 기존
/// 구조를 대체한다. stdin 을 읽는 스레드가 항상 정확히 1 개이므로, 좀비 스레드가
/// 새 스레드와 전역 `std::io::Stdin`(내부 `Mutex<BufReader<..>>`)을 두고 경쟁해
/// 재연결 직후 입력을 훔쳐가는 문제가 구조적으로 사라진다. 반환된 슬롯에 각 raw
/// 세션이 [`install_sender`] 로 자신의 sender 를 설치해 라우팅 대상을 바꾼다.
///
/// 별도 종료 신호 없이 프로세스 종료에 정리를 맡긴다 — 기존 heartbeat 스레드와
/// 동일한 전제(528행 근방 주석 참고, OS 가 프로세스 종료 시 blocking syscall 여부와
/// 무관하게 모든 스레드를 회수한다)라 코드베이스 관행과 일치한다.
fn spawn_stdin_reader() -> (StdinSlot, StdinEofLatch) {
    let slot: StdinSlot = Arc::new(Mutex::new(None));
    let eof_latch: StdinEofLatch = Arc::new(AtomicBool::new(false));
    {
        let slot = slot.clone();
        let eof_latch = eof_latch.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut stdin = std::io::stdin();
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) => {
                        route_stdin_eof(&slot, &eof_latch);
                        break; // 진짜 EOF — 다시 읽어도 항상 0 이므로 더 읽지 않는다.
                    }
                    Ok(n) => route_stdin_chunk(&slot, &buf[..n]),
                    Err(_) => {
                        route_stdin_eof(&slot, &eof_latch);
                        break;
                    }
                }
            }
        });
    }
    (slot, eof_latch)
}

/// 프로세스 전역 stdin 라우팅 슬롯 — 최초 호출 시 1 회 [`spawn_stdin_reader`] 로
/// 초기화된다. 이후 모든 `run_raw_bridge` 호출(=재연결마다)은 이미 떠 있는 같은
/// 리더 스레드의 슬롯에 자신의 sender 를 설치할 뿐, 새 스레드를 스폰하지 않는다.
fn stdin_router() -> &'static (StdinSlot, StdinEofLatch) {
    static ROUTER: OnceLock<(StdinSlot, StdinEofLatch)> = OnceLock::new();
    ROUTER.get_or_init(spawn_stdin_reader)
}

/// stdin 에서 읽은 바이트를 현재 슬롯의 sender 로 라우팅한다. 슬롯이 비어있거나
/// (세션 전환 사이) 이미 죽은 채널이면 이 청크만 조용히 버리고 계속 읽는다 —
/// 오늘의 "송신 실패 시 스레드 종료" 와 달리, 리더가 항상 1 개만 존재해야 한다는
/// 불변식을 지키기 위해 이 함수는 스레드를 종료시키지 않는다.
///
/// **불변식**: 이 함수는 `slot` 을 절대 쓰지 않는다(읽기만) — 위 [`StdinSlot`]
/// 문서의 ABA 경쟁 방지 규칙 참고.
fn route_stdin_chunk(slot: &StdinSlot, data: &[u8]) {
    // poison 이어도 계속 진행 — 감싼 `Option<Sender>` 은 항상 유효한 값이라 poison
    // 후에도 안전하게 읽을 수 있다(tearing 불가). 이 상시 리더 스레드는 `OnceLock`
    // 초기화로 프로세스 생애주기에 1번만 도므로, 여기서 패닉하면 재시작 없이 영구
    // 사망해 이후 모든 재연결 세션이 stdin 을 못 받는다.
    let sender = lock_stdin_slot(slot).clone();
    if let Some(tx) = sender {
        let _ = tx.send(RawEvent::Stdin(data.to_vec())); // 세션 전환 중 송신 실패는 버림 — 위 불변식 참고
    }
}

/// stdin EOF/에러 통지를 현재 슬롯의 sender 로 라우팅한다. 활성 세션이 있어
/// 전달에 성공하면 그걸로 끝. 슬롯이 비어있거나 전달에 실패하면(세션 전환 사이)
/// [`StdinEofLatch`] 에 기억해뒀다가, 다음 세션이 [`install_sender`] 로 sender 를
/// 설치하는 시점에 즉시 전달되게 한다. 위 [`route_stdin_chunk`] 와 동일한 이유로
/// `slot` 은 쓰지 않는다.
fn route_stdin_eof(slot: &StdinSlot, eof_latch: &StdinEofLatch) {
    // route_stdin_chunk 와 동일한 이유로 poison 을 무시하고 계속 진행한다 — 이
    // 함수도 같은 상시 리더 스레드에서 돌므로 여기서 패닉하면 마찬가지로 영구 사망.
    let sender = lock_stdin_slot(slot).clone();
    let delivered = match sender {
        Some(tx) => tx.send(RawEvent::StdinEof).is_ok(),
        None => false,
    };
    if !delivered {
        eof_latch.store(true, Ordering::Release);
    }
}

/// 새 raw 세션이 시작될 때 자신의 sender 를 슬롯에 설치한다 — `slot` 에 대한
/// 유일한 쓰기 지점(위 [`StdinSlot`] 불변식). 설치 직전까지 [`StdinEofLatch`] 가
/// 세워져 있었다면(슬롯이 비어있는 동안 진짜 stdin EOF 가 발생한 경우) 새로
/// 설치한 sender 로 즉시 `RawEvent::StdinEof` 를 전달하고 latch 를 내린다.
fn install_sender(slot: &StdinSlot, eof_latch: &StdinEofLatch, tx: mpsc::Sender<RawEvent>) {
    // poison 이어도 대입은 안전(값 자체가 tearing 불가) — 이 함수는 메인 스레드
    // (재연결 루프)에서 매 세션마다 호출되므로, 여기서 패닉하면 백오프 재연결
    // 루프조차 못 돌고 `tasty attach --raw --ssh` 프로세스 자체가 종료된다.
    *lock_stdin_slot(slot) = Some(tx.clone());
    if eof_latch.swap(false, Ordering::AcqRel) {
        let _ = tx.send(RawEvent::StdinEof); // best-effort — 세션이 이미 끝났으면 무시.
    }
}

/// writer 락 poison 처리 — **복구하지 않는다.**
///
/// 임계구역이 소켓에 프레임을 쓰므로, 락을 든 채 죽은 스레드는 프레임을 절반만
/// 남겼을 수 있다. 그 위에 이어 쓰면 스트림 프레이밍이 깨져 상대가 쓰레기를 읽는다 —
/// 데이터를 신뢰할 수 없는 자리라 복구가 오답이다
/// (`docs/dev-guide/error-handling.md` "락 poison").
///
/// 대신 **조용히** 접지도 않는다. 지금까지 poison 은 "쓰기 실패" 와 구분 없이 세션
/// 종료로 흘러가, 사용자에게는 attach 가 이유 없이 끊긴 것으로 보였다.
fn note_writer_poisoned(during: &str) {
    tracing::error!(
        "attach: writer lock poisoned while {during} — a thread panicked while holding it; \
         ending this attach session rather than writing onto a half-written frame"
    );
}

/// raw 브리지 모드: stdin→서버 입력, 서버 출력→stdout. detach 키 `Ctrl+\`(0x1c).
/// 단계 4 옵션(완전 raw TTY 설정은 추후) — 기본 passthrough.
///
/// server 출력은 별도 스레드로 읽어 하나의 `mpsc` 채널에 merge 한다(mirror-dump
/// 와 동일 패턴). main 은 채널 `recv()` 에만 블록하므로 서버 단절을
/// `RawEvent::ServerRecvErr` 로 즉시 감지해 `AttachExit::Disconnected` 를 정상
/// 반환할 수 있다 — `process::exit` 는 쓰지 않는다.
///
/// stdin 은 이 함수가 직접 스레드를 스폰하지 않는다 — 프로세스
/// 생애주기 동안 [`stdin_router`] 가 1 회만 스폰한 리더 스레드가 있고, 이 함수는
/// 매 호출(=매 재연결 세션)마다 [`install_sender`] 로 자신의 sender 를 그 리더의
/// 라우팅 슬롯에 설치할 뿐이다. 재연결마다 새 stdin 스레드를 스폰하던 예전
/// 구조는 이전 스레드가 종료 신호를 받을 방법이 없어 blocking read 에 갇힌 채
/// 좀비로 남았고, 좀비와 새 스레드가 전역 stdin Mutex 를 두고 경쟁해 재연결
/// 직후 입력이 비결정적으로 유실될 수 있었다 — 리더가 항상 정확히 1 개인
/// 지금은 이 경쟁 자체가 구조적으로 불가능하다. 남는 유실 창은 세션 전환의 아주
/// 짧은 순간(이전 세션이 끝나 슬롯이 비거나 죽은 채널을 가리키는 동안 들어온
/// 입력)뿐이다. 상세 서술은 `docs/dev-guide/attach-behavior.md` "SSH 터널" 절 참고.
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
    raw_bridge_main_loop(rx, writer, &mut out)
}

/// `run_raw_bridge` 의 이벤트 디스패치 루프 — stdin/server 스레드가 실제 OS 자원에
/// 블록하는 부분과 분리해뒀다. 채널 로직만 이 함수가 담당하므로, 유닛 테스트가
/// 실제 stdin/소켓 없이 `RawEvent` 를 직접 채널에 흘려 종료 사유 판정을 검증할 수
/// 있다(아래 `raw_bridge_tests`).
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
                                let _ = stream::write_frame(&mut *w, StreamTag::Data, &data[..pos]); // 종료 경로 best-effort 송신 — 무시
                            }
                            Err(_) => note_writer_poisoned("flushing input before detach"),
                        }
                    }
                    match writer.lock() {
                        Ok(mut w) => {
                            let _ = stream::write_frame(&mut *w, StreamTag::Detach, &[]); // best-effort detach 통지 — 무시
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
                    let _ = out.write_all(&frame.payload); // best-effort 미러 — 무시
                    let _ = out.flush(); // best-effort flush — 무시
                }
                StreamTag::Detach => return done(Completed),
                StreamTag::Control => match classify_control(&frame.payload) {
                    ControlSignal::ForceDetached => {
                        crate::out::errln!("\r\n{}", tasty_i18n::t("cli.attach.force_detached"));
                        return done(Completed);
                    }
                    // 화면이 이어지지 않는다 — 옛 연결을 놓고 다시 붙는다. 새 snapshot 이
                    // 화면을 처음부터 다시 그린다(ADR-0400).
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

/// raw 브리지의 옛 연결을 놓는다 — `Detach` 를 쓰고, 서버가 소켓을 닫았다는 신호
/// (`ServerRecvErr` 또는 서버의 `Detach`)를 기다린다. 기다리는 동안 들어온 stdin 은
/// 버린다 — 세션 전환 사이의 짧은 창이고, 그 창의 입력은 재연결 때와 같이 원격에 닿지
/// 않는다(`run_raw_bridge` doc). 신호가 끝내 안 오면 `HEARTBEAT_TIMEOUT` 에서 포기하고
/// 다시 붙는다 — 그 경우 새 attach 는 서버 점유 해제와 경합할 수 있다.
///
/// 단 **끝내라는 신호는 버리지 않는다** — stdin EOF 와 detach 키(`Ctrl+\`)는 이
/// 창에서도 정상 루프와 같이 세션을 끝낸다(재attach 하지 않는다). 이 창에서는 슬롯이
/// 아직 이 세션의 sender 를 들고 있어 EOF 가 latch 에 안 남으므로, 여기서 삼키면 다음
/// 세션이 받을 EOF 가 없다. 옛 연결에는 이미 `Detach` 를 썼으므로 더 보낼 것도 없다.
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

/// `raw_bridge_main_loop` 채널 로직 단위 테스트 — 실제 stdin/소켓 대신 `RawEvent`
/// 를 채널에 직접 흘려 종료 사유별 `AttachExit` 판정을 검증한다. 이 항목이 고치는
/// 결함은 "서버 쪽 단절을 감지해도 raw 브리지가 `AttachExit::Disconnected` 를 반환하지
/// 못하고(과거엔 `process::exit` 로 프로세스 자체가 죽어 반환 지점에 도달 못함)
/// 백오프 재연결이 발동하지 않는" 것이었다 — 아래 `server_recv_err_reports_disconnected`
/// 가 바로 그 회귀를 잡는다.
#[cfg(test)]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
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
        raw_bridge_main_loop, route_stdin_chunk, route_stdin_eof,
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

    /// (ADR-0400) 손실 통지를 받은 raw 브리지는 옛 연결에 `Detach` 를 쓰고, 서버가
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

/// mirror-dump 가 서버 heartbeat 시한보다 오래 붙어 있을 때 client 발 `Ping` 을 보내는가.
/// 가짜 서버가 실제 서버처럼 read timeout 을 걸고(시험용으로 줄인 주기의 네 배 —
/// `HEARTBEAT_TIMEOUT` 과 `HEARTBEAT_INTERVAL` 의 비율 그대로), 그 시한을 여러 번 넘기는
/// dump 동안 한 번도 시한에 안 걸리고 `Detach` 까지 받는지 잰다. Ping 이 없으면 서버 쪽
/// read 가 시한에 걸려 실제 서버가 점유를 푸는 바로 그 자리에서 이 시험이 실패한다.
#[cfg(test)]
mod dump_heartbeat_tests {
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use tasty_ipc::client::StreamConnection;
    use tasty_ipc::stream::{self, STREAM_PROTO, StreamTag};

    use super::{
        AttachExit, SessionEnd, dump_heartbeat_interval, run_mirror_dump, run_workspace_mirror_dump,
    };

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
                .set_read_timeout(Some(dump_heartbeat_interval() * 4))
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

    /// 서버 시한(주기 × 4)의 세 배.
    fn long_dump_ms() -> u64 {
        (dump_heartbeat_interval() * 12).as_millis() as u64
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
        assert!(
            !seen.timed_out,
            "서버 read 가 heartbeat 시한에 걸렸다 — dump 가 Ping 을 안 보냈다"
        );
        assert!(seen.detached, "dump 가 끝나면 Detach 로 놓아야 한다");
        assert!(
            seen.pings >= 3,
            "Ping {} 회 — 주기마다 보내야 한다",
            seen.pings
        );
    }

    #[test]
    fn a_surface_dump_longer_than_the_server_timeout_keeps_the_connection_alive() {
        let (conn, server) = connect();
        let end = run_mirror_dump(conn, 80, 24, Some(long_dump_ms()), None, false).expect("dump");
        assert_kept_alive(end, server);
    }

    #[test]
    fn a_workspace_dump_longer_than_the_server_timeout_keeps_the_connection_alive() {
        let (conn, server) = connect();
        let end = run_workspace_mirror_dump(
            conn,
            Vec::new(),
            Vec::new(),
            Some(long_dump_ms()),
            None,
            None,
            false,
        )
        .expect("dump");
        assert_kept_alive(end, server);
    }
}
