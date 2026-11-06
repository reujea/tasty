//! ClientCommand 실행 처리. 단발 JSON-RPC와의 구분은 crate::dispatch가 담당한다.

pub mod attach;
pub mod debug;
pub mod file_dispatch;
pub mod hook_dispatch;
pub mod passkey;
pub mod port;
pub mod remote_check;
pub mod remote_gui_attach;
pub mod remote_new_workspace;
pub mod remote_profile;
pub mod remote_workspaces;
mod run_wait;

use anyhow::Result;

use crate::commands::passkey::PasskeyCommands;
use crate::commands::remote_profile::RemoteProfileCommands;
use crate::dispatch::{ClientCommand, ClientCtx};

/// `tasty port` — 포트 파일을 읽어 출력한다. 로컬 전용(IPC 미경유):
/// auto 원격 포트 발견 체인(`ssh host tasty port`)의 첫 단계 역할을 한다.
/// 셸 독립성은 체인 전체(subcommand → file-unix → file-windows)가 주는 것이지
/// 이 단계 하나가 주는 것이 아니다 — subcommand 단계는 Windows GUI release
/// 셸에서 조용히 실패한다.
pub struct Port;
impl ClientCommand for Port {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        crate::local::port::run_port(ctx.port_file)
    }
}

/// `tasty agent report usage` — report 기록 방법을 출력한다(통신 없음).
pub struct ReportUsage;
impl ClientCommand for ReportUsage {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        crate::out::outln!("{}", tasty_i18n::t("cli.agent.report_usage"))?;
        Ok(())
    }
}

/// `tasty tool ssh <profile>` — 저장된 ssh 프로필로 대화형 ssh 접속(로컬, IPC 미경유).
/// remote-profiles.toml 은 client 로컬 파일이라 직접 resolve → 시스템 ssh spawn.
pub struct ToolSsh<'a> {
    pub profile: &'a str,
    pub command: &'a [String],
}
impl ClientCommand for ToolSsh<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        let profiles = tasty_remote_profiles::RemoteProfiles::load();
        let passkeys = tasty_remote_profiles::Passkeys::load();
        let Some(p) = profiles.get(self.profile) else {
            anyhow::bail!(
                "{}",
                tasty_i18n::t_fmt("cli.dispatch.ssh_profile_not_found", self.profile)
            );
        };
        if p.as_ssh().is_none() {
            anyhow::bail!(
                "{}",
                tasty_i18n::t_fmt2(
                    "cli.dispatch.ssh_profile_not_ssh_kind",
                    self.profile,
                    &p.kind
                )
            );
        }
        let target = crate::ssh::SshTarget::from_remote_profile(p, &passkeys)?;
        crate::ssh::run_ssh_connect(&target, self.command)
    }
}

/// `tasty tool remote-profile ...` — 프로필 CRUD (ssh + tasty-attach), 로컬 (IPC 미경유).
pub struct ToolRemoteProfile<'a> {
    pub command: &'a RemoteProfileCommands,
}
impl ClientCommand for ToolRemoteProfile<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        crate::local::remote_profile::run(self.command)
    }
}

/// `tasty tool passkey ...` — passkeys.toml 로컬 파일 (IPC 미경유).
pub struct ToolPasskey<'a> {
    pub command: &'a PasskeyCommands,
}
impl ClientCommand for ToolPasskey<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        crate::local::passkey::run(self.command)
    }
}

/// `tasty tool attach ...` — tasty-attach 프로필로 attach 실행 또는 `--list` 목록만.
///
/// `--list` 면 tasty-attach kind 목록만 출력하고 종료. name 이 있으면 그 프로필을
/// resolve(docs/features/remote-profiles/index.md#데이터-모델 ref/inline)해 기존 attach 엔진을 재사용한다.
pub struct ToolAttach<'a> {
    pub name: &'a Option<String>,
    pub surface: Option<u32>,
    pub workspace: Option<u32>,
    pub send: &'a Option<String>,
    pub send_to: Option<u32>,
    pub dump_after: Option<u64>,
    pub raw: bool,
    pub no_reconnect: bool,
    pub list: bool,
}
impl ClientCommand for ToolAttach<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        if self.list {
            return crate::local::remote_profile::run(&RemoteProfileCommands::List {
                json: false,
                kind: Some("tasty-attach".to_string()),
            });
        }
        let Some(name) = self.name.as_deref() else {
            anyhow::bail!(
                "{}",
                tasty_i18n::t("cli.dispatch.tool_attach_target_required")
            );
        };
        if self.surface.is_some() && self.workspace.is_some() {
            anyhow::bail!(
                "{}",
                tasty_i18n::t("cli.dispatch.surface_workspace_exclusive")
            );
        }
        let profiles = tasty_remote_profiles::RemoteProfiles::load();
        let passkeys = tasty_remote_profiles::Passkeys::load();
        let Some(p) = profiles.get(name) else {
            anyhow::bail!(
                "{}",
                tasty_i18n::t_fmt("cli.dispatch.attach_profile_not_found", name)
            );
        };
        let (target, rt, pm, pf) = crate::ssh::resolve_attach_target(p, &profiles, &passkeys)?;
        if let Some(ws) = self.workspace {
            if self.raw {
                anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.raw_workspace_exclusive"));
            }
            return crate::local::attach::run_attach_workspace_ssh(
                target,
                &rt,
                &pm,
                pf.as_deref(),
                ws,
                self.dump_after,
                self.send.as_deref(),
                self.send_to,
                !self.no_reconnect,
            );
        }
        let Some(surface) = self.surface else {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.attach_target_required"));
        };
        crate::local::attach::run_attach_ssh(
            target,
            &rt,
            &pm,
            pf.as_deref(),
            surface,
            self.dump_after,
            self.send.as_deref(),
            self.raw,
            !self.no_reconnect,
        )
    }
}

/// `tasty remote attach` — SSH 터널 + raw 스트림 attach, 백오프 재연결.
pub struct RemoteAttach<'a> {
    pub surface: Option<u32>,
    pub workspace: Option<u32>,
    pub dump_after: Option<u64>,
    pub send: &'a Option<String>,
    pub send_to: Option<u32>,
    pub raw: bool,
    pub ssh: &'a Option<String>,
    pub profile: &'a Option<String>,
    pub remote_tasty: &'a str,
    pub remote_port_mode: &'a str,
    pub no_reconnect: bool,
}
impl ClientCommand for RemoteAttach<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        if self.surface.is_some() && self.workspace.is_some() {
            anyhow::bail!(
                "{}",
                tasty_i18n::t("cli.dispatch.surface_workspace_exclusive")
            );
        }
        if self.ssh.is_some() && self.profile.is_some() {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.ssh_profile_exclusive"));
        }
        let (target, rt, pm, pf) = resolve_attach_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            self.remote_tasty,
            self.remote_port_mode,
            tasty_i18n::t("cli.dispatch.remote_attach_target_required"),
        )?;
        if let Some(ws) = self.workspace {
            if self.raw {
                anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.raw_workspace_exclusive"));
            }
            return crate::local::attach::run_attach_workspace_ssh(
                target,
                &rt,
                &pm,
                pf.as_deref(),
                ws,
                self.dump_after,
                self.send.as_deref(),
                self.send_to,
                !self.no_reconnect,
            );
        }
        let Some(surface) = self.surface else {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.attach_target_required"));
        };
        crate::local::attach::run_attach_ssh(
            target,
            &rt,
            &pm,
            pf.as_deref(),
            surface,
            self.dump_after,
            self.send.as_deref(),
            self.raw,
            !self.no_reconnect,
        )
    }
}

/// 원격 포트를 찾고 SSH 터널로 system.info 응답을 확인한다. 포트 파일만으로 생존을 판단하지 않는다.
pub struct RemoteCheck<'a> {
    pub ssh: &'a Option<String>,
    pub profile: &'a Option<String>,
    pub remote_tasty: &'a str,
    pub remote_port_mode: &'a str,
}
impl ClientCommand for RemoteCheck<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        if self.ssh.is_some() && self.profile.is_some() {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.ssh_profile_exclusive"));
        }
        let (target, rt, pm, pf) = resolve_attach_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            self.remote_tasty,
            self.remote_port_mode,
            tasty_i18n::t("cli.dispatch.remote_check_target_required"),
        )?;
        crate::local::remote_check::run_remote_check(target, &rt, &pm, pf.as_deref())
    }
}

/// `tasty remote workspaces` — 원격 tasty 의 워크스페이스 목록 조회(browse).
/// 순수 조회라 로컬 사용자 상태(focus)에 닿지 않는다(docs/design/policies/focus.md).
pub struct RemoteWorkspaces<'a> {
    pub ssh: &'a Option<String>,
    pub profile: &'a Option<String>,
    pub remote_tasty: &'a str,
    pub remote_port_mode: &'a str,
    pub json: bool,
}
impl ClientCommand for RemoteWorkspaces<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        if self.ssh.is_some() && self.profile.is_some() {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.ssh_profile_exclusive"));
        }
        let (target, rt, pm, pf) = crate::remote_browse::resolve_connection_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            self.remote_tasty,
            self.remote_port_mode,
        )?;
        crate::local::remote_workspaces::run_remote_workspaces(
            target,
            &rt,
            &pm,
            pf.as_deref(),
            self.json,
        )
    }
}

/// `tasty remote new-workspace` — 원격 tasty 에 워크스페이스 생성(원격 mutate).
/// 로컬 사용자 상태에 닿지 않는다(docs/design/policies/focus.md).
pub struct RemoteNewWorkspace<'a> {
    pub ssh: &'a Option<String>,
    pub profile: &'a Option<String>,
    pub remote_tasty: &'a str,
    pub remote_port_mode: &'a str,
    pub name: &'a Option<String>,
    pub cwd: &'a Option<String>,
    pub json: bool,
}
impl ClientCommand for RemoteNewWorkspace<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        if self.ssh.is_some() && self.profile.is_some() {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.ssh_profile_exclusive"));
        }
        let (target, rt, pm, pf) = crate::remote_browse::resolve_connection_spec(
            self.profile.as_deref(),
            self.ssh.as_deref(),
            self.remote_tasty,
            self.remote_port_mode,
        )?;
        crate::local::remote_new_workspace::run_remote_new_workspace(
            target,
            &rt,
            &pm,
            pf.as_deref(),
            self.name.as_deref(),
            self.cwd.as_deref(),
            self.json,
        )
    }
}

/// plugin logs is local-only — read the log file directly.
pub struct PluginLogs<'a> {
    pub id: &'a str,
    pub follow: bool,
}
impl ClientCommand for PluginLogs<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        crate::plugin::run_plugin_logs(self.id, self.follow)
    }
}

/// plugin doctor is local-only — read manifest from disk, no IPC needed.
pub struct PluginDoctor<'a> {
    pub id: &'a str,
}
impl ClientCommand for PluginDoctor<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        crate::plugin::run_plugin_doctor(self.id)
    }
}

/// plugin audit-follow is a polling loop over plugin.audit_follow IPC.
pub struct PluginAuditFollow<'a> {
    pub caller_kind: &'a Option<String>,
    pub caller_id: &'a Option<String>,
    pub method_prefix: &'a Option<String>,
    pub decision: &'a Option<String>,
    pub batch: u64,
    pub interval_ms: u64,
}
impl ClientCommand for PluginAuditFollow<'_> {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        crate::plugin::run_audit_follow(
            self.caller_kind.as_deref(),
            self.caller_id.as_deref(),
            self.method_prefix.as_deref(),
            self.decision.as_deref(),
            self.batch,
            self.interval_ms,
            ctx.port_file,
        )
    }
}

/// events follow is a long-poll loop over events.fetch IPC. The cursor lives
/// here, not on the host, so a restart of this process resumes from whatever
/// offset it was given.
pub struct EventsFollow<'a> {
    pub offset: u64,
    pub filter: &'a Option<String>,
    pub batch: u64,
    pub wait_ms: u64,
    pub epoch: Option<u64>,
    pub reconnect: bool,
}
impl ClientCommand for EventsFollow<'_> {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        crate::events::run_follow(
            crate::events::FollowArgs {
                offset: self.offset,
                filter: self.filter.as_deref(),
                batch: self.batch,
                wait_ms: self.wait_ms,
                epoch: self.epoch,
                reconnect: self.reconnect,
            },
            ctx.port_file,
        )
    }
}

/// debug stream-echo uses a raw framed streaming channel, not the JSON-RPC
/// request-response path (debug builds only).
#[cfg(debug_assertions)]
pub struct DebugStreamEcho<'a> {
    pub payload: &'a str,
    pub count: u32,
}
#[cfg(debug_assertions)]
impl ClientCommand for DebugStreamEcho<'_> {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        crate::local::debug::run_stream_echo(self.payload, self.count, ctx.port_file)
    }
}

/// debug sim emits raw VTE to its own stdout from inside the current surface —
/// no host instance / IPC involved (debug builds only).
#[cfg(debug_assertions)]
pub struct DebugSim<'a> {
    pub cmd: &'a Option<tasty_tui_simulator::Commands>,
}
#[cfg(debug_assertions)]
impl ClientCommand for DebugSim<'_> {
    fn run(self: Box<Self>, _ctx: &ClientCtx<'_>) -> Result<()> {
        tasty_tui_simulator::run(self.cmd);
        Ok(())
    }
}

/// `tasty debug attach <id>` — 로컬 loopback raw 스트림 (debug 빌드 전용).
#[cfg(debug_assertions)]
pub struct DebugAttach<'a> {
    pub surface: Option<u32>,
    pub workspace: Option<u32>,
    pub dump_after: Option<u64>,
    pub send: &'a Option<String>,
    pub send_to: Option<u32>,
    pub raw: bool,
}
#[cfg(debug_assertions)]
impl ClientCommand for DebugAttach<'_> {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        if self.surface.is_some() && self.workspace.is_some() {
            anyhow::bail!(
                "{}",
                tasty_i18n::t("cli.dispatch.surface_workspace_exclusive")
            );
        }
        if let Some(ws) = self.workspace {
            if self.raw {
                anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.raw_workspace_exclusive"));
            }
            return crate::local::debug::attach::run_attach_workspace(
                ws,
                self.dump_after,
                self.send.as_deref(),
                self.send_to,
                ctx.port_file,
            );
        }
        let Some(surface) = self.surface else {
            anyhow::bail!("{}", tasty_i18n::t("cli.dispatch.attach_target_required"));
        };
        crate::local::debug::attach::run_attach(
            surface,
            self.dump_after,
            self.send.as_deref(),
            self.raw,
            ctx.port_file,
        )
    }
}

/// 원격 접속 스펙 결정: 저장 프로필(비활성 거부) → `SshTarget`(+remote_tasty/
/// port_mode 대체), 없으면 1회성 `--ssh`. 둘 다 없으면 `missing` 문구로 거부한다.
///
/// `attach` 와 `check` 가 같은 규칙을 쓰되 대상 미지정 문구만 다르다.
fn resolve_attach_spec(
    profile: Option<&str>,
    ssh: Option<&str>,
    remote_tasty: &str,
    remote_port_mode: &str,
    missing: &str,
) -> Result<(crate::ssh::SshTarget, String, String, Option<String>)> {
    match profile {
        Some(name) => {
            let profiles = tasty_remote_profiles::RemoteProfiles::load();
            let passkeys = tasty_remote_profiles::Passkeys::load();
            let Some(p) = profiles.get(name) else {
                anyhow::bail!(
                    "{}",
                    tasty_i18n::t_fmt("cli.remote_profile.not_found_list_hint", name)
                );
            };
            crate::ssh::resolve_attach_target(p, &profiles, &passkeys)
        }
        None => match ssh {
            Some(dest) => Ok((
                crate::ssh::SshTarget::parse(dest),
                remote_tasty.to_string(),
                remote_port_mode.to_string(),
                None,
            )),
            None => anyhow::bail!("{missing}"),
        },
    }
}
