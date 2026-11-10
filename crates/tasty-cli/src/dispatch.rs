//! 단발 JSON-RPC와 클라이언트가 직접 처리하는 명령을 구분한다.
//! 로컬 파일·프로세스, 스트림·폴링·SSH 명령은 ClientDriven으로 실행한다.
//! 새 실행 경로는 classify에 등록한다.

use anyhow::Result;

use crate::Commands;
use crate::commands::{
    AgentCommands, EventsCommands, PluginCommands, RemoteCommands, ReportCommands, ToolCommands,
};

/// 클라이언트 실행에 전달할 공통 옵션.
pub struct ClientCtx<'a> {
    pub port_file: Option<&'a str>,
}

/// 클라이언트가 주도하는 실행 하나.
///
/// `run` 이 `Box<Self>` 를 받는 것은 [`Dispatch::ClientDriven`] 이 트레잇 객체를
/// 담기 때문이다 — 소유권을 그대로 넘겨 내부 필드를 move 로 쓸 수 있다.
pub trait ClientCommand {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()>;
}

/// clap이 파싱한 명령을 빌려 실행하므로 값을 복제하지 않는다.
pub enum Dispatch<'a> {
    /// `request/` 가 만든 단발 JSON-RPC — 보내고 응답을 출력하면 끝.
    Rpc,
    /// 클라이언트가 주도하는 실행 — 로컬 파일/프로세스, raw 스트림, 폴링 루프,
    /// SSH 터널 경유 조회. 공통점은 "단발 RPC 가 아니다" 하나다.
    ClientDriven(Box<dyn ClientCommand + 'a>),
}

impl Commands {
    /// 이 명령이 어느 갈래인지 판정한다. 인자 조합 검증도 여기서 끝낸다 —
    /// 검증 실패는 통신을 시작하기 전에 나야 한다.
    pub fn dispatch(&self) -> Result<Dispatch<'_>> {
        match classify(self)? {
            Some(cmd) => Ok(Dispatch::ClientDriven(cmd)),
            None => Ok(Dispatch::Rpc),
        }
    }
}

/// 클라이언트 주도 명령이면 그 실행 단위를, 아니면 `None`(= 단발 RPC)을 돌려준다.
///
/// arm 순서가 의미를 갖는다 — 앞선 arm 이 이긴다.
#[allow(clippy::too_many_lines)] // 평면 라우팅 표 — 쪼개면 "어디로 가는가" 를 한눈에 못 본다
fn classify(command: &Commands) -> Result<Option<Box<dyn ClientCommand + '_>>> {
    let driven: Box<dyn ClientCommand + '_> = match command {
        // ── 로컬 파일/프로세스 (통신 없음) ──────────────────────────────────
        Commands::Port => Box::new(crate::local::Port),
        Commands::Tool {
            command: ToolCommands::Ssh { profile, command },
        } => Box::new(crate::local::ToolSsh { profile, command }),
        Commands::Tool {
            command: ToolCommands::RemoteProfile { command },
        } => Box::new(crate::local::ToolRemoteProfile { command }),
        Commands::Tool {
            command: ToolCommands::Passkey { command },
        } => Box::new(crate::local::ToolPasskey { command }),
        Commands::Plugin {
            command: PluginCommands::Doctor { id },
        } => Box::new(crate::local::PluginDoctor { id }),
        Commands::Agent {
            command:
                AgentCommands::Report {
                    command: ReportCommands::Usage,
                },
        } => Box::new(crate::local::ReportUsage),

        // ── raw 스트림 / 터널 경유 조회 ─────────────────────────────────────
        Commands::Tool {
            command:
                ToolCommands::Attach {
                    name,
                    surface,
                    workspace,
                    send,
                    send_to,
                    dump_after,
                    raw,
                    no_reconnect,
                    list,
                },
        } => Box::new(crate::local::ToolAttach {
            name,
            surface: *surface,
            workspace: *workspace,
            send,
            send_to: *send_to,
            dump_after: *dump_after,
            raw: *raw,
            no_reconnect: *no_reconnect,
            list: *list,
        }),
        // into-gui는 단발 RPC로 처리하고 일반 attach만 스트림을 연다.
        Commands::Remote {
            command:
                RemoteCommands::Attach {
                    surface,
                    workspace,
                    dump_after,
                    send,
                    send_to,
                    raw,
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                    no_reconnect,
                    into_gui: false,
                    target_port: _,
                    new_workspace: _,
                    name: _,
                    cwd: _,
                },
        } => Box::new(crate::local::RemoteAttach {
            surface: *surface,
            workspace: *workspace,
            dump_after: *dump_after,
            send,
            send_to: *send_to,
            raw: *raw,
            ssh,
            profile,
            remote_tasty,
            remote_port_mode,
            no_reconnect: *no_reconnect,
        }),
        Commands::Remote { command: remote }
            if remote.gui_opens_tunnel() || matches!(remote, RemoteCommands::Connect { .. }) =>
        {
            Box::new(crate::local::remote_gui_attach::RemoteGuiAttach { command })
        }
        Commands::Remote {
            command:
                RemoteCommands::Check {
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                },
        } => Box::new(crate::local::RemoteCheck {
            ssh,
            profile,
            remote_tasty,
            remote_port_mode,
        }),
        Commands::Remote {
            command:
                RemoteCommands::Workspaces {
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                    json,
                },
        } => Box::new(crate::local::RemoteWorkspaces {
            ssh,
            profile,
            remote_tasty,
            remote_port_mode,
            json: *json,
        }),
        Commands::Remote {
            command:
                RemoteCommands::NewWorkspace {
                    ssh,
                    profile,
                    remote_tasty,
                    remote_port_mode,
                    name,
                    cwd,
                    json,
                },
        } => Box::new(crate::local::RemoteNewWorkspace {
            ssh,
            profile,
            remote_tasty,
            remote_port_mode,
            name,
            cwd,
            json: *json,
        }),

        // ── 폴링/스트리밍 루프 (반복 IPC) ───────────────────────────────────
        Commands::HookHandler {
            command: crate::commands::HookHandlerCommands::Dispatch { no_wait: false, .. },
        } => Box::new(crate::local::hook_dispatch::HookDispatch { command }),
        Commands::FileHandler {
            command: crate::commands::FileHandlerCommands::Dispatch { no_wait: false, .. },
        } => Box::new(crate::local::file_dispatch::FileDispatch { command }),
        Commands::Plugin {
            command: PluginCommands::Logs { id, follow },
        } => Box::new(crate::local::PluginLogs {
            id,
            follow: *follow,
        }),
        Commands::Plugin {
            command:
                PluginCommands::AuditFollow {
                    caller_kind,
                    caller_id,
                    method_prefix,
                    decision,
                    batch,
                    interval_ms,
                },
        } => Box::new(crate::local::PluginAuditFollow {
            caller_kind,
            caller_id,
            method_prefix,
            decision,
            batch: *batch,
            interval_ms: *interval_ms,
        }),
        Commands::Events {
            command:
                EventsCommands::Follow {
                    offset,
                    filter,
                    batch,
                    wait_ms,
                    epoch,
                    reconnect,
                },
        } => Box::new(crate::local::EventsFollow {
            offset: *offset,
            filter,
            batch: *batch,
            wait_ms: *wait_ms,
            epoch: *epoch,
            reconnect: *reconnect,
        }),

        // ── debug 빌드 전용 ────────────────────────────────────────────────
        #[cfg(debug_assertions)]
        Commands::Debug {
            command: crate::commands::DebugCommands::StreamEcho { payload, count },
        } => Box::new(crate::local::DebugStreamEcho {
            payload,
            count: *count,
        }),
        #[cfg(debug_assertions)]
        Commands::Debug {
            command: crate::commands::DebugCommands::Sim { cmd },
        } => Box::new(crate::local::DebugSim { cmd }),
        // debug의 force-detach는 단발 RPC(debug.attach.*)로 처리한다.
        #[cfg(debug_assertions)]
        Commands::Debug {
            command:
                crate::commands::DebugCommands::Attach {
                    surface,
                    workspace,
                    dump_after,
                    send,
                    send_to,
                    raw,
                    force_detach: false,
                },
        } => Box::new(crate::local::DebugAttach {
            surface: *surface,
            workspace: *workspace,
            dump_after: *dump_after,
            send,
            send_to: *send_to,
            raw: *raw,
        }),

        _ => return Ok(None),
    };
    Ok(Some(driven))
}
