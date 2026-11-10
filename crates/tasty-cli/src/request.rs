mod agent;
mod approval;
mod clipboard;
mod debug;
mod memory;
mod output;
mod plugin_cmd;
mod presets;
mod remote;
mod settings;
mod telemetry;
mod terminal;

use agent::agent_command_to_method_params;
use approval::approval_command_to_method_params;
use clipboard::clipboard_command_to_method_params;
#[cfg(debug_assertions)]
use debug::debug_command_to_method_params;
use memory::memory_command_to_method_params;
use output::output_command_to_method_params;
use plugin_cmd::plugin_command_to_method_params;
use presets::{
    completion_strategy_command_to_method_params, file_handler_command_to_method_params,
    hook_handler_command_to_method_params, preset_command_to_method_params,
};
use settings::settings_command_to_method_params;
use telemetry::telemetry_command_to_method_params;
use terminal::{pty_command_to_method_params, terminal_command_to_method_params};

use super::{
    CloseCommands, Commands, EventsCommands, ListCommands, MoveCommands, NewCommands,
    NotificationCommands, ReadCommands, RemoteCommands, SendCommands, SessionCommands, SetCommands,
    SurfaceAttentionCommands, SurfaceCommands, SurfaceMetaCommands, ToolCommands, UnsetCommands,
    WorkspaceCategoryCommands,
};
use tasty_ipc::protocol::JsonRpcRequest;

/// 서버로 보내지 않는 client-local 경고를 params 에 임시로 실어 나르는 예약 키.
/// `run_client`(`run.rs`) 가 요청 전송 직전에 떼어내 실제 RPC params 에서 제거하고,
/// 응답 출력 시 top-level `warnings` 필드로 병합한다.
pub(crate) const CLI_WARNINGS_PARAMS_KEY: &str = "__cli_warnings";

/// 이전 이름 `webhook config`를 쓰면 응답의 `warnings`에 싣는 안내(API 규칙의 deprecation 절차).
pub(crate) const WEBHOOK_CONFIG_DEPRECATED: &str =
    "deprecated: tasty webhook config, use tasty webhook port";

/// `tasty remote ...` → JsonRpcRequest 매핑. `--into-gui` 가 아닌 attach 는
/// run_client 에서 raw 스트림으로 선처리되므로, 여기 도달하는 remote attach 는
/// `--into-gui`(원격 GUI mirror 위임)뿐이다.
fn remote_command_to_method_params(command: &RemoteCommands) -> (&'static str, serde_json::Value) {
    match command {
        attach if attach.gui_opens_tunnel() => ("remote.attach", remote::attach_params(attach)),
        RemoteCommands::Attach {
            workspace,
            target_port,
            into_gui,
            ..
        } => {
            debug_assert!(
                *into_gui,
                "non-into_gui remote attach is dispatched before request mapping"
            );
            (
                "attach.into_gui",
                serde_json::json!({
                    "port": target_port,
                    "workspace": workspace,
                }),
            )
        }
        RemoteCommands::Check { .. } => {
            debug_assert!(false, "remote check is dispatched before request mapping");
            ("remote.check.noop", serde_json::json!({}))
        }
        RemoteCommands::Workspaces { .. } => {
            debug_assert!(
                false,
                "remote workspaces is dispatched before request mapping"
            );
            ("remote.workspaces.noop", serde_json::json!({}))
        }
        RemoteCommands::Refusals => ("remote.refusals", serde_json::json!({})),
        RemoteCommands::Disconnect { workspace } => (
            "remote.disconnect",
            serde_json::json!({ "workspace_id": workspace }),
        ),
        RemoteCommands::NewWorkspace { .. } => {
            debug_assert!(
                false,
                "remote new-workspace is dispatched before request mapping"
            );
            ("remote.new_workspace.noop", serde_json::json!({}))
        }
    }
}

/// Resolve a target string for split/other commands.
/// - "this" → numeric surface ID from TASTY_SURFACE_ID env var
/// - numeric string → passed through as-is
/// - other string → passed through as-is (server resolves as nickname)
fn resolve_target(target: &str) -> String {
    if target == "this" {
        std::env::var("TASTY_SURFACE_ID").unwrap_or_else(|_| target.to_string())
    } else {
        target.to_string()
    }
}

/// Interpret C-style escape sequences: \r \n \t \\ \0
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('r') => out.push('\r'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Get surface_id: explicit value > TASTY_SURFACE_ID env var.
pub(super) fn resolve_surface_id(explicit: Option<u32>) -> Option<u32> {
    explicit.or_else(|| std::env::var("TASTY_SURFACE_ID").ok()?.parse().ok())
}

/// 명시한 cwd를 절대 경로로 바꾸고 디렉터리인지 확인한다. 실패하면 exit 1로 끝낸다.
/// 생략한 cwd는 호스트가 결정한다.
fn normalize_cwd_or_exit(raw: Option<&str>) -> Option<String> {
    let value = raw?;
    if value.is_empty() {
        return None;
    }
    match super::cwd_resolve::normalize_cwd_arg(value) {
        Ok(absolute) => Some(absolute),
        Err(e) => {
            crate::out::errln!(
                "{}",
                tasty_i18n::t_fmt("cli.request.cwd_invalid", &e.to_string())
            );
            std::process::exit(1);
        }
    }
}

/// 상대 `--file`의 기준으로 호출자 폴더를 돌려준다. 호출자 폴더는 CLI만 안다. 대상이 mirror이면
/// file은 원격 경로이므로 여기서 절대 경로로 바꾸지 않고 호스트가 대상을 보고 정한다.
/// 폴더를 읽지 못하면 기준 없이 보낸다. 호스트는 상대 경로에서 cwd를 얻지 않는다.
fn relative_file_base(file: Option<&str>) -> Option<String> {
    let file = file.filter(|file| !file.is_empty())?;
    if std::path::Path::new(file).is_absolute() {
        return None;
    }
    let base = std::env::current_dir().ok()?;
    Some(base.to_string_lossy().into_owned())
}

/// JSON 을 받는 인자(`flag`)의 값을 파싱한다. 실패 문구는 인자 이름과 파서의 원인을 담는다.
fn parse_json_arg(flag: &str, raw: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(raw)
        .map_err(|e| tasty_i18n::t_fmt2("cli.request.json_arg_invalid", flag, &e.to_string()))
}

/// [`parse_json_arg`] 가 실패하면 원인을 stderr 에 내고 종료 코드 1 로 끝낸다 — 요청 매핑은
/// `Result` 를 돌려주지 못한다([`normalize_cwd_or_exit`] 와 같은 선례).
fn parse_json_arg_or_exit(flag: &str, raw: &str) -> serde_json::Value {
    parse_json_arg(flag, raw).unwrap_or_else(|msg| {
        crate::out::errln!("{msg}");
        std::process::exit(1);
    })
}

pub fn command_to_request(command: &Commands) -> JsonRpcRequest {
    let (method, params) = match command {
        Commands::New { command } => new_command_to_method_params(command),
        Commands::Close { command } => close_command_to_method_params(command),
        Commands::List { command } => list_command_to_method_params(command),
        Commands::Set { command } => set_command_to_method_params(command),
        Commands::Move { command } => move_command_to_method_params(command),
        #[cfg(debug_assertions)]
        Commands::Debug { command } => debug_command_to_method_params(command),
        Commands::Split {
            level,
            target_surface,
            target_pane,
            direction,
            r#type,
            meta,
            cwd,
            file,
            path,
            url,
        } => {
            let meta_value = meta
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

            let ts = target_surface
                .as_ref()
                .map(|s| serde_json::Value::String(resolve_target(s)));
            let tp = target_pane.map(|p| serde_json::Value::Number(p.into()));
            let resolved_cwd = normalize_cwd_or_exit(cwd.as_deref());

            (
                "split",
                serde_json::json!({
                    "level": level,
                    "target_surface": ts,
                    "target_pane": tp,
                    "direction": direction,
                    "type": r#type,
                    "meta": meta_value,
                    "cwd": resolved_cwd,
                    "file": file,
                    "file_base": relative_file_base(file.as_deref()),
                    "path": path,
                    "url": url,
                }),
            )
        }
        Commands::Send { command } => send_command_to_method_params(command),
        // 일반 attach는 ClientDriven이 처리한다. 여기서는 into-gui만 변환한다.
        Commands::Remote { command } => remote_command_to_method_params(command),
        Commands::Read { command } => read_command_to_method_params(command),
        Commands::Notify {
            body,
            title,
            level,
            surface,
            workspace,
        } => (
            "notification.create",
            serde_json::json!({
                "title": title,
                "body": body,
                "level": level,
                "surface_id": if workspace.is_some() { *surface } else { resolve_surface_id(*surface) },
                "workspace_id": workspace,
            }),
        ),
        Commands::Notification { command } => match command {
            NotificationCommands::Counts { workspace } => (
                "notification.counts",
                serde_json::json!({"workspace_id": workspace}),
            ),
            NotificationCommands::Read { id, all, workspace } => (
                "notification.read",
                serde_json::json!({"notification_id": id, "all": all, "workspace_id": workspace}),
            ),
        },
        Commands::Unset { command } => unset_command_to_method_params(command),
        Commands::SurfaceMeta { command } => surface_meta_command_to_method_params(command),
        Commands::Surface { command } => match command {
            SurfaceCommands::Completion { surface, kind } => (
                "surface.completion",
                serde_json::json!({ "surface_id": surface, "kind": kind }),
            ),
            SurfaceCommands::CursorPosition { surface } => (
                "surface.cursor_position",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::MouseTracking { surface } => (
                "surface.mouse_tracking",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::ForegroundProcess { surface } => (
                "surface.foreground_process",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::Locate { surface } => (
                "surface.locate",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::HtmlScript { surface } => (
                "surface.html_script",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::HtmlScriptAllow {
                surface,
                url,
                fingerprint,
            } => {
                let mut params = serde_json::json!({ "surface_id": surface });
                if let Some(url) = url {
                    params["url"] = serde_json::json!(url);
                }
                if let Some(fingerprint) = fingerprint {
                    params["fingerprint"] = serde_json::json!(fingerprint);
                }
                ("surface.html_script_allow", params)
            }
            SurfaceCommands::HtmlRemoteAllow {
                surface,
                url,
                fingerprint,
            } => {
                let mut params = serde_json::json!({ "surface_id": surface });
                if let Some(url) = url {
                    params["url"] = serde_json::json!(url);
                }
                if let Some(fingerprint) = fingerprint {
                    params["fingerprint"] = serde_json::json!(fingerprint);
                }
                ("surface.html_remote_allow", params)
            }
            SurfaceCommands::HtmlNav { surface } => (
                "surface.html_nav",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::HtmlBack { surface } => (
                "surface.html_back",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::HtmlForward { surface } => (
                "surface.html_forward",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::HtmlReload { surface } => (
                "surface.html_reload",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::RespawnTerminal { surface } => (
                "surface.respawn_terminal",
                serde_json::json!({ "surface_id": surface }),
            ),
            SurfaceCommands::FireHook { surface, event } => (
                "surface.fire_hook",
                serde_json::json!({ "surface_id": surface, "event": event }),
            ),
            SurfaceCommands::Attention { command } => match command {
                SurfaceAttentionCommands::Get { surface } => (
                    "surface.attention.get",
                    serde_json::json!({ "surface_id": surface }),
                ),
                SurfaceAttentionCommands::Clear { surface, kind } => (
                    "surface.attention.clear",
                    serde_json::json!({ "surface_id": surface, "kind": kind }),
                ),
            },
        },
        Commands::IsTyping { surface } => (
            "surface.is_typing",
            serde_json::json!({ "surface_id": resolve_surface_id(*surface) }),
        ),
        Commands::Wake { surface } => (
            "surface.wake",
            serde_json::json!({ "surface_id": resolve_surface_id(*surface) }),
        ),
        Commands::Session { command } => match command {
            SessionCommands::Issue {
                agent_id,
                permissions,
                ttl_ms,
            } => (
                "session.issue",
                serde_json::json!({
                    "agent_id": agent_id,
                    "permissions": permissions,
                    "ttl_ms": ttl_ms,
                }),
            ),
            SessionCommands::Revoke { token } => {
                ("session.revoke", serde_json::json!({ "token": token }))
            }
            SessionCommands::List => ("session.list", serde_json::json!({})),
        },
        Commands::Tool { command } => tool_command_to_method_params(command),
        Commands::Plugin { command } => plugin_command_to_method_params(command),
        Commands::Memory { command } => memory_command_to_method_params(command),
        Commands::Settings { command } => settings_command_to_method_params(command),
        Commands::Output { command } => output_command_to_method_params(command),
        Commands::Approval { command } => approval_command_to_method_params(command),
        Commands::Events { command } => match command {
            EventsCommands::Fetch {
                offset,
                max,
                filter,
                wait_ms,
            } => {
                let mut p = serde_json::Map::new();
                p.insert("offset".into(), serde_json::json!(offset));
                if let Some(m) = max {
                    p.insert("max".into(), serde_json::json!(m));
                }
                if let Some(f) = filter {
                    p.insert("filter".into(), serde_json::json!(f));
                }
                if let Some(w) = wait_ms {
                    p.insert("wait_ms".into(), serde_json::json!(w));
                }
                ("events.fetch", serde_json::Value::Object(p))
            }
            // `follow` 는 루프라 요청 하나로 안 접힌다 — `dispatch.rs` 가 가져간다.
            EventsCommands::Follow { .. } => ("events.fetch", serde_json::json!({})),
        },
        Commands::Telemetry { command } => telemetry_command_to_method_params(command),
        Commands::Agent { command } => agent_command_to_method_params(command),
        Commands::FileHandler { command } => file_handler_command_to_method_params(command),
        Commands::Clipboard { command } => clipboard_command_to_method_params(command),
        Commands::HookHandler { command } => hook_handler_command_to_method_params(command),
        Commands::CompletionStrategy { command } => {
            completion_strategy_command_to_method_params(command)
        }
        Commands::Preset { command } => preset_command_to_method_params(command),
        Commands::WorkspaceCategory { command } => {
            workspace_category_command_to_method_params(command)
        }
        Commands::Webhook { command } => webhook_command_to_method_params(command),
        Commands::Terminal { command } => terminal_command_to_method_params(command),
        Commands::Pty { command } => pty_command_to_method_params(command),
        // `tasty port` 는 run.rs 에서 IPC 전에 로컬 처리됨 — 여기 도달하지 않음.
        Commands::Port => ("port.noop", serde_json::json!({})),
        // focus 독립 캡처. surface/window 를 ID 로 직접 지정 (focused 의존 없음).
        Commands::Screenshot {
            path,
            surface,
            window,
        } => (
            "ui.screenshot",
            serde_json::json!({
                "path": path,
                "surface_id": surface,
                "window_id": window,
            }),
        ),
    };

    JsonRpcRequest {
        caller_agent_id: tasty_ipc::protocol::caller_agent_id_from_env(),
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: Some(serde_json::json!(1)),
        // 자식 agent 가 띄운 CLI 가 env 로 받은 session token 을
        // envelope 에 자동 첨부. 호스트는 이를 검증해 CallerContext::Agent 로 분기.
        session_token: std::env::var("TASTY_SESSION_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
    }
}

fn new_command_to_method_params(command: &NewCommands) -> (&'static str, serde_json::Value) {
    match command {
        NewCommands::Window => ("window.create", serde_json::json!({})),
        NewCommands::Workspace {
            name,
            cwd,
            r#type,
            file,
            path,
            url,
            ssh_profile,
            ssh,
            remote_workspace,
            category,
            surface,
        } => {
            let resolved_cwd = normalize_cwd_or_exit(cwd.as_deref());
            let mut params = serde_json::json!({
                "name": name.as_deref().unwrap_or(""),
                "cwd": resolved_cwd,
                "type": r#type,
                "file": file,
                "file_base": relative_file_base(file.as_deref()),
                "path": path,
                "url": url,
                "attach_profile": ssh_profile,
                "attach_ssh": ssh,
                "attach_remote_workspace": remote_workspace,
                "category": category,
            });
            // 명시한 surface_id로 창을 찾고, 찾지 못하면 거절한다.
            // 생략 시 기존 창 선택을 유지하기 위해 TASTY_SURFACE_ID로 채우지 않는다.
            if let Some(sid) = surface {
                params["surface_id"] = serde_json::json!(sid);
            }
            ("workspace.create", params)
        }
        NewCommands::Tab {
            pane,
            r#type,
            cwd,
            file,
            path,
            url,
        } => {
            let resolved_cwd = normalize_cwd_or_exit(cwd.as_deref());
            (
                "tab.create",
                serde_json::json!({
                    "pane_id": pane,
                    "type": r#type,
                    "cwd": resolved_cwd,
                    "file": file,
                    "file_base": relative_file_base(file.as_deref()),
                    "path": path,
                    "url": url,
                }),
            )
        }
    }
}

fn close_command_to_method_params(command: &CloseCommands) -> (&'static str, serde_json::Value) {
    let caller = resolve_surface_id(None); // TASTY_SURFACE_ID
    match command {
        CloseCommands::Tab { tab } => (
            "tab.close",
            serde_json::json!({ "tab_id": tab, "caller_surface_id": caller }),
        ),
        CloseCommands::Pane { pane } => (
            "pane.close",
            serde_json::json!({ "pane_id": pane, "caller_surface_id": caller }),
        ),
        CloseCommands::Surface { surface } => (
            "surface.close",
            serde_json::json!({ "surface_id": surface, "caller_surface_id": caller }),
        ),
        CloseCommands::Workspace { id } => (
            "workspace.close",
            serde_json::json!({ "id": id, "caller_surface_id": caller }),
        ),
        CloseCommands::Window { id } => ("window.close", serde_json::json!({ "id": id })),
        CloseCommands::CloseSelf => match caller {
            Some(sid) => (
                "surface.close_self",
                serde_json::json!({ "surface_id": sid }),
            ),
            None => {
                crate::out::errln!("{}", tasty_i18n::t("cli.request.close_self_no_surface"));
                std::process::exit(1);
            }
        },
    }
}

fn list_command_to_method_params(command: &ListCommands) -> (&'static str, serde_json::Value) {
    match command {
        ListCommands::Workspaces => ("workspace.list", serde_json::json!({})),
        ListCommands::Windows => ("window.list", serde_json::json!({})),
        ListCommands::Tree => ("tree", serde_json::json!({})),
        ListCommands::Surfaces => ("surface.list", serde_json::json!({})),
        ListCommands::SurfaceKinds => ("surface.kinds", serde_json::json!({})),
        ListCommands::Panes => ("pane.list", serde_json::json!({})),
        ListCommands::Tabs { pane } => ("tab.list", serde_json::json!({ "pane_id": pane })),
        ListCommands::Info => ("system.info", serde_json::json!({})),
        ListCommands::GpuStats => ("system.gpu_stats", serde_json::json!({})),
        ListCommands::Pressure => ("system.pressure", serde_json::json!({})),
        ListCommands::Notifications => ("notification.list", serde_json::json!({})),
        ListCommands::Timers => ("timer.list", serde_json::json!({})),
        // 전체 조회를 현재 surface로 제한하지 않도록 환경변수 대신 명시한 필터만 보낸다.
        ListCommands::Hooks { surface } => {
            ("hook.list", serde_json::json!({ "surface_id": surface }))
        }
        ListCommands::GlobalHooks => ("global_hook.list", serde_json::json!({})),
        ListCommands::Theme => ("theme.query", serde_json::json!({})),
        ListCommands::Recent { kind } => ("recent.query", serde_json::json!({ "kind": kind })),
        ListCommands::Queue { surface } => (
            "message.count",
            serde_json::json!({ "surface_id": resolve_surface_id(*surface) }),
        ),
    }
}

fn send_command_to_method_params(command: &SendCommands) -> (&'static str, serde_json::Value) {
    match command {
        // --wait-idle 은 "타이핑 중인지 보고 아니면 보낸다" 를 한 번의 호출로 판정한다.
        // is-typing → send 두 단계로는 그 사이에 사용자가 타이핑을 시작하는 창이 남는다.
        SendCommands::Text {
            text,
            surface,
            wait_idle,
        } => (
            if *wait_idle {
                "surface.send_wait_idle"
            } else {
                "surface.send"
            },
            serde_json::json!({ "text": unescape(text), "surface_id": resolve_surface_id(*surface) }),
        ),
        SendCommands::Key { key, surface } => (
            "surface.send_key",
            serde_json::json!({ "key": key, "surface_id": resolve_surface_id(*surface) }),
        ),
        SendCommands::Queue { to, content, from } => (
            "message.send",
            serde_json::json!({
                "to_surface_id": to,
                "content": content,
                "from_surface_id": resolve_surface_id(*from),
            }),
        ),
    }
}

fn read_command_to_method_params(command: &ReadCommands) -> (&'static str, serde_json::Value) {
    match command {
        ReadCommands::SinceMark {
            surface,
            strip_ansi,
            cursor,
            stream,
            max_bytes,
        } => {
            use tasty_ipc::output_cursor::{PARAM_CURSOR, PARAM_MAX_BYTES, PARAM_STREAM};
            let mut params = serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "strip_ansi": strip_ansi,
            });
            // 위치 인자를 생략한 요청은 기존 형태를 유지하고 capability 조회도 생략한다.
            if let Some(c) = cursor {
                params[PARAM_CURSOR] = serde_json::json!(c);
            }
            if let Some(s) = stream {
                params[PARAM_STREAM] = serde_json::json!(s);
            }
            if let Some(m) = max_bytes {
                params[PARAM_MAX_BYTES] = serde_json::json!(m);
            }
            // 메서드 이름은 리터럴로 둔다 — `tests/cli_method_table_parity.rs` 가 CLI 소스의
            // 리터럴로 CLI 진입점을 센다. `output_cursor::METHOD` 와 같은 값인지는
            // `contract` 의 시험(위치 인자를 실은 요청이 계약을 요구하는가)이 잡는다.
            ("surface.read_since_mark", params)
        }
        ReadCommands::ParseSinceMark { surface, parsers } => {
            let mut params = serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
            });
            if let Some(ids) = parsers {
                params["parsers"] = serde_json::Value::Array(
                    ids.iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                );
            }
            ("surface.parse_since_mark", params)
        }
        ReadCommands::Queue {
            surface,
            from,
            peek,
            clear,
        } => {
            if *clear {
                (
                    "message.clear",
                    serde_json::json!({ "surface_id": resolve_surface_id(*surface) }),
                )
            } else {
                (
                    "message.read",
                    serde_json::json!({
                        "surface_id": resolve_surface_id(*surface),
                        "from_surface_id": from,
                        "peek": peek,
                    }),
                )
            }
        }
        ReadCommands::Screen {
            surface,
            lines,
            show_dim,
        } => (
            "surface.screen_text",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "lines": lines,
                "show_dim": show_dim,
            }),
        ),
        ReadCommands::Commands {
            surface,
            limit,
            since,
        } => {
            let mut params = serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
            });
            if let Some(n) = limit {
                params["limit"] = serde_json::Value::from(*n);
            }
            if let Some(ts) = since {
                params["since"] = serde_json::Value::from(*ts);
            }
            ("surface.commands", params)
        }
        ReadCommands::LastCommand { surface } => (
            "surface.last_command",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
            }),
        ),
        ReadCommands::CommandAt { surface, index } => (
            "surface.command_at",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "index": index,
            }),
        ),
    }
}

fn set_command_to_method_params(command: &SetCommands) -> (&'static str, serde_json::Value) {
    match command {
        SetCommands::Hook {
            surface,
            event,
            command,
            handler,
            once,
        } => (
            "hook.set",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "event": event,
                "command": command,
                "handler": handler,
                "once": once,
            }),
        ),
        SetCommands::Mark { surface } => (
            "surface.set_mark",
            serde_json::json!({ "surface_id": resolve_surface_id(*surface) }),
        ),
        SetCommands::Workspace {
            id,
            name,
            subtitle,
            description,
            ssh_profile,
            ssh,
            remote_workspace,
            clear_mapping,
            category,
        } => (
            "workspace.update",
            serde_json::json!({
                "id": id,
                "name": name,
                "subtitle": subtitle,
                "description": description,
                "attach_profile": ssh_profile,
                "attach_ssh": ssh,
                "attach_remote_workspace": remote_workspace,
                "attach_clear": clear_mapping,
                "category": category,
            }),
        ),
        SetCommands::Cwd { surface, path } => (
            "surface.set_cwd",
            serde_json::json!({ "surface_id": surface, "cwd": path }),
        ),
        SetCommands::Url {
            surface,
            url,
            label,
        } => (
            "webview.set_url",
            serde_json::json!({ "surface_id": surface, "url": url, "label": label }),
        ),
        SetCommands::GlobalHook {
            condition,
            command,
            label,
        } => (
            "global_hook.set",
            serde_json::json!({
                "condition": condition,
                "command": command,
                "label": label,
            }),
        ),
    }
}

fn workspace_category_command_to_method_params(
    command: &WorkspaceCategoryCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        WorkspaceCategoryCommands::List => ("workspace_category.list", serde_json::json!({})),
        WorkspaceCategoryCommands::Create { name } => (
            "workspace_category.create",
            serde_json::json!({ "name": name }),
        ),
        WorkspaceCategoryCommands::Rename { id, name } => (
            "workspace_category.rename",
            serde_json::json!({ "id": id, "name": name }),
        ),
        WorkspaceCategoryCommands::Delete { id } => {
            ("workspace_category.delete", serde_json::json!({ "id": id }))
        }
        WorkspaceCategoryCommands::Move { id, from, to } => {
            let mut p = serde_json::json!({ "to_index": to });
            if let Some(id) = id {
                p["id"] = serde_json::json!(id);
            }
            if let Some(from) = from {
                p["from_index"] = serde_json::json!(from);
            }
            ("workspace_category.move", p)
        }
    }
}

fn webhook_command_to_method_params(
    command: &crate::commands::WebhookCommands,
) -> (&'static str, serde_json::Value) {
    use crate::commands::WebhookCommands as W;
    match command {
        W::Register {
            methods,
            handler,
            sequence,
            persistent,
            ttl_secs,
            count,
            auth_location,
            auth_key,
            auth_token,
        } => {
            // 파싱 오류를 null로 보내면 서버가 다른 사유로 거절하므로 여기서 알린다.
            let sequence_value = sequence
                .as_deref()
                .map(|s| parse_json_arg_or_exit("--sequence", s));
            let auth_value = auth_location.as_deref().map(|loc| {
                serde_json::json!({
                    "location": loc,
                    "key": auth_key,
                    "token": auth_token,
                })
            });
            (
                "webhook.register",
                serde_json::json!({
                    // 빈 배열은 거절되므로 미지정은 null로 보내 서버 기본값 POST를 사용한다.
                    "methods": (!methods.is_empty()).then_some(methods),
                    "handler": handler,
                    "sequence": sequence_value,
                    "persistent": persistent,
                    "ttl_secs": ttl_secs,
                    "count": count,
                    "auth": auth_value,
                }),
            )
        }
        W::List => ("webhook.list", serde_json::json!({})),
        W::Info { id } => ("webhook.info", serde_json::json!({ "id": id })),
        W::Unregister { id } => ("webhook.unregister", serde_json::json!({ "id": id })),
        W::Sweep => ("webhook.sweep", serde_json::json!({})),
        W::Port { port, unset } => (
            "webhook.config",
            serde_json::json!({ "port": port, "unset_port": unset }),
        ),
        W::AllowExternal { value } => (
            "webhook.config",
            serde_json::json!({ "allow_external": value.as_deref().map(|v| v == "on") }),
        ),
        W::Config { port } => (
            "webhook.config",
            serde_json::json!({
                "port": port,
                CLI_WARNINGS_PARAMS_KEY: [WEBHOOK_CONFIG_DEPRECATED],
            }),
        ),
    }
}

fn move_command_to_method_params(command: &MoveCommands) -> (&'static str, serde_json::Value) {
    match command {
        MoveCommands::Tab { pane, from, to } => (
            "tab.move",
            serde_json::json!({
                "pane_id": pane,
                "from_index": from,
                "to_index": to,
            }),
        ),
        MoveCommands::Workspace { id, from, to } => {
            let mut p = serde_json::json!({ "to_index": to });
            // 둘 중 **있는 쪽만** 싣는다. 없는 키를 null 로 실으면 핸들러의
            // "둘 다 줬다" 거절과 "안 줬다" 거절이 구분되지 않는다.
            if let Some(id) = id {
                p["id"] = serde_json::json!(id);
            }
            if let Some(from) = from {
                p["from_index"] = serde_json::json!(from);
            }
            ("workspace.move", p)
        }
    }
}

fn unset_command_to_method_params(command: &UnsetCommands) -> (&'static str, serde_json::Value) {
    match command {
        UnsetCommands::Hook { hook } => ("hook.unset", serde_json::json!({ "hook_id": hook })),
        UnsetCommands::GlobalHook { hook } => {
            ("global_hook.unset", serde_json::json!({ "hook_id": hook }))
        }
    }
}

fn surface_meta_command_to_method_params(
    command: &SurfaceMetaCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        SurfaceMetaCommands::Set {
            key,
            value,
            surface,
        } => (
            "surface.meta.set",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "key": key,
                "value": value,
            }),
        ),
        SurfaceMetaCommands::Get { key, surface } => (
            "surface.meta.get",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "key": key,
            }),
        ),
        SurfaceMetaCommands::Unset { key, surface } => (
            "surface.meta.unset",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "key": key,
            }),
        ),
        SurfaceMetaCommands::List { surface } => (
            "surface.meta.list",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
            }),
        ),
    }
}

fn tool_command_to_method_params(command: &ToolCommands) -> (&'static str, serde_json::Value) {
    match command {
        // 네 명령은 dispatch::classify가 ClientDriven으로 처리한다.
        // 프로필을 IPC로 조작하는 경로는 별도의 remote.profile.* 메서드다.
        ToolCommands::Ssh { .. } => ("tool.ssh.noop", serde_json::json!({})),
        ToolCommands::RemoteProfile { .. } => ("tool.remote_profile.noop", serde_json::json!({})),
        ToolCommands::Attach { .. } => ("tool.attach.noop", serde_json::json!({})),
        ToolCommands::Passkey { .. } => ("tool.passkey.noop", serde_json::json!({})),
    }
}

#[cfg(test)]
mod caller_agent_id_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 환경변수의 원래 값을 저장하고 패닉 시에도 복원한다. 단순 삭제하면 실행 환경을 잃는다.
    struct SurfaceIdEnvGuard(Option<std::ffi::OsString>);

    impl SurfaceIdEnvGuard {
        const KEY: &'static str = "TASTY_SURFACE_ID";

        fn new() -> Self {
            Self(std::env::var_os(Self::KEY))
        }
        fn set(&self, v: &str) {
            // SAFETY: 이 키를 만지는 시나리오를 단일 #[test] 안에 모아 두었으므로
            // 같은 프로세스에서 동시에 쓰는 다른 테스트가 없다.
            unsafe { std::env::set_var(Self::KEY, v) };
        }
        fn unset(&self) {
            // SAFETY: set 과 동일 — 단일 #[test] 안에 격리해 직렬화된다.
            unsafe { std::env::remove_var(Self::KEY) };
        }
    }

    impl Drop for SurfaceIdEnvGuard {
        fn drop(&mut self) {
            match &self.0 {
                // SAFETY: set 과 동일 — 단일 #[test] 안에 격리해 직렬화된다.
                Some(v) => unsafe { std::env::set_var(Self::KEY, v) },
                // SAFETY: 상동.
                None => unsafe { std::env::remove_var(Self::KEY) },
            }
        }
    }

    #[test]
    fn a_broken_json_argument_is_an_error_that_names_the_flag() {
        tasty_i18n::init("en");
        let err = parse_json_arg("--sequence", "{broken").unwrap_err();
        assert!(
            err.starts_with("Error: --sequence: not valid JSON: "),
            "{err}"
        );
        assert_eq!(
            parse_json_arg("--sequence", r#"[{"method":"x"}]"#).unwrap(),
            serde_json::json!([{ "method": "x" }])
        );
    }

    fn parsed(args: &[&str]) -> Commands {
        use clap::Parser;
        crate::Cli::try_parse_from(args)
            .unwrap_or_else(|e| panic!("{args:?}: {e}"))
            .command
            .expect("subcommand")
    }

    /// 절대 경로는 기준을 싣지 않고, 상대 경로는 호출자 폴더를 함께 싣되 file은 바꾸지 않는다.
    #[test]
    fn only_a_relative_file_carries_the_caller_folder() {
        let full = if cfg!(windows) { r"C:\a.md" } else { "/a.md" };
        for args in [
            &["tasty", "new", "tab", "--pane", "1", "--file"][..],
            &["tasty", "new", "workspace", "--file"][..],
            &["tasty", "split", "--level", "pane", "--file"][..],
        ] {
            let absolute = parsed(&[args, &[full]].concat());
            let params = command_to_request(&absolute).params;
            assert_eq!(params["file"], full);
            assert!(params["file_base"].is_null(), "{args:?}");
            let local = parsed(&[args, &["docs/a.md"]].concat());
            let params = command_to_request(&local).params;
            assert_eq!(params["file"], "docs/a.md");
            assert_eq!(
                params["file_base"],
                std::env::current_dir().unwrap().to_string_lossy().as_ref(),
                "{args:?}"
            );
        }
    }

    #[test]
    fn json_output_flag_does_not_change_the_task_request() {
        for (plain, json) in [
            (
                &[
                    "tasty",
                    "agent",
                    "task-get",
                    "--workspace-id",
                    "1",
                    "--id",
                    "a",
                ][..],
                &[
                    "tasty",
                    "agent",
                    "task-get",
                    "--workspace-id",
                    "1",
                    "--id",
                    "a",
                    "--json",
                ][..],
            ),
            (
                &["tasty", "agent", "task-list", "--workspace-id", "1"][..],
                &[
                    "tasty",
                    "agent",
                    "task-list",
                    "--workspace-id",
                    "1",
                    "--json",
                ][..],
            ),
        ] {
            let plain = command_to_request(&parsed(plain));
            let json = command_to_request(&parsed(json));
            assert_eq!(plain.method, json.method);
            assert_eq!(plain.params, json.params);
        }
    }

    #[test]
    fn report_commands_map_to_their_methods() {
        let req = command_to_request(&parsed(&[
            "tasty",
            "agent",
            "dag-report",
            "d:rep",
            "--task",
            "a",
            "--attempt",
            "2",
        ]));
        assert_eq!(req.method, "agent.dag_report");
        assert_eq!(
            req.params,
            serde_json::json!({"id": "d:rep", "task": "a", "attempt": 2, "include_raw": false})
        );
        let req = command_to_request(&parsed(&[
            "tasty",
            "agent",
            "report",
            "append",
            "--address",
            "1/1/run/t0k/a",
            "checked",
        ]));
        assert_eq!(req.method, "agent.report_append");
        assert_eq!(
            req.params,
            serde_json::json!({"address": "1/1/run/t0k/a", "text": "checked"})
        );
        // 회차만 고를 수는 없다.
        use clap::Parser;
        assert!(
            crate::Cli::try_parse_from(["tasty", "agent", "dag-report", "d:rep", "--attempt", "1"])
                .is_err()
        );
        assert!(matches!(
            parsed(&["tasty", "agent", "report", "usage"]).dispatch(),
            Ok(crate::dispatch::Dispatch::ClientDriven(_))
        ));
    }

    #[test]
    fn notification_cli_levels_targets_and_read_scope() {
        let req = command_to_request(&cmd_from(&[
            "tasty",
            "notify",
            "failure",
            "--level",
            "error",
            "--workspace",
            "12",
        ]));
        assert_eq!(req.method, "notification.create");
        assert_eq!(req.params["level"], "error");
        assert_eq!(req.params["workspace_id"], 12);
        assert!(req.params["surface_id"].is_null());
        let req = command_to_request(&cmd_from(&[
            "tasty",
            "notification",
            "counts",
            "--workspace",
            "12",
        ]));
        assert_eq!(req.method, "notification.counts");
        assert_eq!(req.params["workspace_id"], 12);
        let req = command_to_request(&cmd_from(&["tasty", "notification", "read", "7"]));
        assert_eq!(req.method, "notification.read");
        assert_eq!(req.params["notification_id"], 7);
        assert_eq!(req.params["all"], false);
        let req = command_to_request(&cmd_from(&["tasty", "notification", "read", "--all"]));
        assert_eq!(req.params["all"], true);
    }

    // 환경변수는 프로세스 전체가 공유하므로 모든 변경 시나리오를 한 시험 안에서 직렬화한다.
    #[test]
    fn surface_id_env_scenarios() {
        let cmd = Commands::Notify {
            body: "msg".to_string(),
            title: Some("T".to_string()),
            level: "info".into(),
            surface: None,
            workspace: None,
        };

        let env = SurfaceIdEnvGuard::new();

        env.set("42");
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "notification.create");
        assert_eq!(req.params.get("title").and_then(|v| v.as_str()), Some("T"));
        assert_eq!(req.params.get("body").and_then(|v| v.as_str()), Some("msg"));
        assert_eq!(
            req.params.get("surface_id").and_then(|v| v.as_u64()),
            Some(42)
        );

        let ws_default = command_to_request(&cmd_from(&["tasty", "new", "workspace"]));
        assert_eq!(ws_default.method, "workspace.create");
        assert!(ws_default.params.get("surface_id").is_none());
        let ws_named =
            command_to_request(&cmd_from(&["tasty", "new", "workspace", "--surface", "7"]));
        assert_eq!(
            ws_named.params.get("surface_id").and_then(|v| v.as_u64()),
            Some(7)
        );

        let hooks_no_filter = command_to_request(&cmd_from(&["tasty", "list", "hooks"]));
        assert_eq!(hooks_no_filter.method, "hook.list");
        assert!(
            hooks_no_filter
                .params
                .get("surface_id")
                .is_some_and(|v| v.is_null())
        );
        let hooks_filtered =
            command_to_request(&cmd_from(&["tasty", "list", "hooks", "--surface", "7"]));
        assert_eq!(
            hooks_filtered
                .params
                .get("surface_id")
                .and_then(|v| v.as_u64()),
            Some(7)
        );

        env.unset();
        let req = command_to_request(&cmd);
        assert!(req.params.get("surface_id").is_some_and(|v| v.is_null()));

        let children = cmd_from(&["tasty", "terminal", "children"]);
        let req = command_to_request(&children);
        assert_eq!(req.method, "terminal.children");
        assert!(req.params.get("surface").is_none());

        env.set("not-a-number");
        let req = command_to_request(&cmd);
        assert!(req.params.get("surface_id").is_some_and(|v| v.is_null()));
    }

    /// `agent task-list --state` 는 콤마 다중값을 받는다(`task-purge --states` 와
    /// 동일한 파싱). 단일값은 예전처럼 문자열로, 2개 이상이면 배열로 보낸다 —
    /// 구버전 호스트에 새 CLI 가 붙어도 단일값 필터는 그대로 동작한다.
    #[test]
    fn agent_task_list_state_accepts_comma_separated_values() {
        let single = command_to_request(&cmd_from(&[
            "tasty",
            "agent",
            "task-list",
            "--workspace-id",
            "4",
            "--state",
            "running",
        ]));
        assert_eq!(single.method, "agent.task_list");
        assert_eq!(
            single.params.get("state").and_then(|v| v.as_str()),
            Some("running")
        );

        let multi = command_to_request(&cmd_from(&[
            "tasty",
            "agent",
            "task-list",
            "--workspace-id",
            "4",
            "--state",
            "waiting,ready,running",
        ]));
        assert_eq!(
            multi.params.get("state"),
            Some(&serde_json::json!(["waiting", "ready", "running"]))
        );

        let repeated = command_to_request(&cmd_from(&[
            "tasty",
            "agent",
            "task-list",
            "--workspace-id",
            "4",
            "--state",
            "waiting",
            "--state",
            "ready",
        ]));
        assert_eq!(
            repeated.params.get("state"),
            Some(&serde_json::json!(["waiting", "ready"]))
        );

        let none = command_to_request(&cmd_from(&[
            "tasty",
            "agent",
            "task-list",
            "--workspace-id",
            "4",
        ]));
        assert!(none.params.get("state").is_none());
    }

    fn cmd_from(args: &[&str]) -> Commands {
        use clap::Parser;
        crate::Cli::try_parse_from(args)
            .expect("parse")
            .command
            .expect("subcommand")
    }

    #[test]
    fn screenshot_surface_maps_to_ui_screenshot() {
        let cmd = cmd_from(&[
            "tasty",
            "screenshot",
            "--path",
            "/tmp/s.png",
            "--surface",
            "5",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "ui.screenshot");
        assert_eq!(
            req.params.get("path").and_then(|v| v.as_str()),
            Some("/tmp/s.png")
        );
        assert_eq!(
            req.params.get("surface_id").and_then(|v| v.as_u64()),
            Some(5)
        );
        assert!(req.params.get("window_id").is_some_and(|v| v.is_null()));
    }

    #[test]
    fn screenshot_window_maps_to_ui_screenshot() {
        let cmd = cmd_from(&[
            "tasty",
            "screenshot",
            "--path",
            "/tmp/w.png",
            "--window",
            "2",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "ui.screenshot");
        assert_eq!(
            req.params.get("window_id").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert!(req.params.get("surface_id").is_some_and(|v| v.is_null()));
    }

    #[test]
    fn terminal_spawn_maps_to_terminal_spawn_with_explicit_parent() {
        let cmd = cmd_from(&[
            "tasty",
            "terminal",
            "spawn",
            "--surface",
            "7",
            "--workspace",
            "dev",
            "--command",
            "bash",
            "--role",
            "worker",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.spawn");
        assert_eq!(req.params["parent"].as_u64(), Some(7));
        assert_eq!(req.params["workspace"].as_str(), Some("dev"));
        assert_eq!(req.params["command"].as_str(), Some("bash"));
        assert_eq!(req.params["role"].as_str(), Some("worker"));
    }

    #[test]
    fn terminal_kill_maps_child_index() {
        let cmd = cmd_from(&[
            "tasty",
            "terminal",
            "kill",
            "--surface",
            "7",
            "--child",
            "2",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.kill");
        assert_eq!(req.params["surface"].as_u64(), Some(7));
        assert_eq!(req.params["child"].as_u64(), Some(2));
    }

    #[test]
    fn terminal_adopt_maps_target_and_optional_fields() {
        let cmd = cmd_from(&[
            "tasty",
            "terminal",
            "adopt",
            "--surface",
            "7",
            "--target",
            "42",
            "--nickname",
            "worker",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.adopt");
        assert_eq!(req.params["surface"].as_u64(), Some(7));
        assert_eq!(req.params["target"].as_u64(), Some(42));
        assert_eq!(req.params["nickname"].as_str(), Some("worker"));
        assert!(req.params.get("role").is_none());
    }

    #[test]
    fn terminal_release_maps_child_index() {
        let cmd = cmd_from(&[
            "tasty",
            "terminal",
            "release",
            "--surface",
            "7",
            "--child",
            "2",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.release");
        assert_eq!(req.params["surface"].as_u64(), Some(7));
        assert_eq!(req.params["child"].as_u64(), Some(2));
    }

    #[test]
    fn pty_spawn_maps_command_tokens_as_array() {
        let cmd = cmd_from(&["tasty", "pty", "spawn", "--", "echo", "hi"]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "pty.spawn");
        assert_eq!(
            req.params["command"],
            serde_json::json!(["echo", "hi"]),
            "command tokens must map to a JSON array"
        );
    }

    #[test]
    fn pty_write_wait_kill_map_id() {
        let w = command_to_request(&cmd_from(&[
            "tasty",
            "pty",
            "write",
            "--id",
            "2147483648",
            "hello",
        ]));
        assert_eq!(w.method, "pty.write");
        assert_eq!(w.params["id"].as_u64(), Some(2_147_483_648));
        assert_eq!(w.params["text"].as_str(), Some("hello"));

        let wait = command_to_request(&cmd_from(&["tasty", "pty", "wait", "--id", "42"]));
        assert_eq!(wait.method, "pty.wait");
        assert_eq!(wait.params["id"].as_u64(), Some(42));

        let list = command_to_request(&cmd_from(&["tasty", "pty", "list"]));
        assert_eq!(list.method, "pty.list");
    }

    #[test]
    fn terminal_set_state_maps_surface_and_state() {
        let cmd = cmd_from(&[
            "tasty",
            "terminal",
            "set-state",
            "--surface",
            "9",
            "--state",
            "idle",
        ]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.set_state");
        assert_eq!(req.params["surface"].as_u64(), Some(9));
        assert_eq!(req.params["state"].as_str(), Some("idle"));
    }

    #[test]
    fn terminal_parent_requires_surface() {
        let cmd = cmd_from(&["tasty", "terminal", "parent", "--surface", "5000"]);
        let req = command_to_request(&cmd);
        assert_eq!(req.method, "terminal.parent");
        assert_eq!(req.params["surface"].as_u64(), Some(5000));
    }
}
