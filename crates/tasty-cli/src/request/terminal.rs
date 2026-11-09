//! terminal.*는 자식 터미널 surface를, pty.*는 surface 없는 PTY를 ID로 조작한다.
//! cwd 정규화는 normalize_cwd_or_exit를 공유한다.

use super::{normalize_cwd_or_exit, resolve_surface_id};

pub(super) fn terminal_command_to_method_params(
    command: &crate::commands::TerminalCommands,
) -> (&'static str, serde_json::Value) {
    use crate::commands::TerminalCommands as T;
    use serde_json::{Map, Value};

    // key 를 Some 일 때만 넣는 헬퍼 — host 는 생략된 optional 을 single_parent 등으로
    // 폴백하므로 null 을 넣지 않는다.
    fn put_u32(map: &mut Map<String, Value>, key: &str, v: Option<u32>) {
        if let Some(x) = v {
            map.insert(key.into(), Value::from(x));
        }
    }
    fn put_str(map: &mut Map<String, Value>, key: &str, v: &Option<String>) {
        if let Some(x) = v {
            map.insert(key.into(), Value::from(x.clone()));
        }
    }
    // External 부모를 주면 caller TASTY_SURFACE_ID 를 숫자 부모로 채우지 않는다. workspace 는
    // 창이 여럿일 때 host 가 대상 창을 고르는 데 쓴다.
    fn put_parent(
        map: &mut Map<String, Value>,
        surface: Option<u32>,
        external: &Option<String>,
        workspace: &Option<String>,
    ) {
        match external {
            Some(key) => {
                map.insert("parent_external".into(), Value::from(key.clone()));
                put_str(map, "workspace", workspace);
            }
            None => put_u32(map, "surface", resolve_surface_id(surface)),
        }
    }

    match command {
        T::Spawn {
            surface,
            parent_external,
            workspace,
            pane,
            cwd,
            command,
            role,
            nickname,
        } => {
            let mut m = Map::new();
            // parent = --parent-external, --surface 또는 caller TASTY_SURFACE_ID. 모두 없으면 대상 불명.
            if let Some(key) = parent_external {
                m.insert("parent_external".into(), Value::from(key.clone()));
            } else {
                let Some(parent) = resolve_surface_id(*surface) else {
                    crate::out::errln!("{}", tasty_i18n::t("cli.terminal.spawn_no_parent"));
                    std::process::exit(1);
                };
                m.insert("parent".into(), Value::from(parent));
            }
            m.insert("workspace".into(), Value::from(workspace.clone()));
            put_u32(&mut m, "pane", *pane);
            if let Some(c) = normalize_cwd_or_exit(cwd.as_deref()) {
                m.insert("cwd".into(), Value::from(c));
            }
            m.insert("command".into(), Value::from(command.clone()));
            put_str(&mut m, "role", role);
            put_str(&mut m, "nickname", nickname);
            ("terminal.spawn", Value::Object(m))
        }
        T::Tell { text, surface } => {
            let mut m = Map::new();
            let Some(target) = resolve_surface_id(*surface) else {
                crate::out::errln!("{}", tasty_i18n::t("cli.terminal.tell_no_target"));
                std::process::exit(1);
            };
            m.insert("surface".into(), Value::from(target));
            m.insert("text".into(), Value::from(text.clone()));
            ("terminal.tell", Value::Object(m))
        }
        T::Children {
            surface,
            parent_external,
            workspace,
            format: _,
        } => {
            let mut m = Map::new();
            put_parent(&mut m, *surface, parent_external, workspace);
            ("terminal.children", Value::Object(m))
        }
        T::Parent { surface } => ("terminal.parent", serde_json::json!({ "surface": surface })),
        T::State { surface } => ("terminal.state", serde_json::json!({ "surface": surface })),
        T::Kill {
            surface,
            parent_external,
            workspace,
            child,
        } => {
            let mut m = Map::new();
            put_parent(&mut m, *surface, parent_external, workspace);
            m.insert("child".into(), Value::from(*child));
            ("terminal.kill", Value::Object(m))
        }
        T::Respawn {
            surface,
            parent_external,
            workspace,
            child,
            cwd,
            command,
            role,
            nickname,
        } => {
            let mut m = Map::new();
            put_parent(&mut m, *surface, parent_external, workspace);
            m.insert("child".into(), Value::from(*child));
            if let Some(c) = normalize_cwd_or_exit(cwd.as_deref()) {
                m.insert("cwd".into(), Value::from(c));
            }
            put_str(&mut m, "command", command);
            put_str(&mut m, "role", role);
            put_str(&mut m, "nickname", nickname);
            ("terminal.respawn", Value::Object(m))
        }
        T::Broadcast {
            text,
            surface,
            parent_external,
            workspace,
            role,
        } => {
            let mut m = Map::new();
            put_parent(&mut m, *surface, parent_external, workspace);
            m.insert("text".into(), Value::from(text.clone()));
            put_str(&mut m, "role", role);
            ("terminal.broadcast", Value::Object(m))
        }
        T::SetState { surface, state } => (
            "terminal.set_state",
            serde_json::json!({ "surface": surface, "state": state }),
        ),
        T::Adopt {
            surface,
            target,
            cwd,
            role,
            nickname,
        } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
            m.insert("target".into(), Value::from(*target));
            if let Some(c) = normalize_cwd_or_exit(cwd.as_deref()) {
                m.insert("cwd".into(), Value::from(c));
            }
            put_str(&mut m, "role", role);
            put_str(&mut m, "nickname", nickname);
            ("terminal.adopt", Value::Object(m))
        }
        T::Release {
            surface,
            parent_external,
            workspace,
            child,
        } => {
            let mut m = Map::new();
            put_parent(&mut m, *surface, parent_external, workspace);
            m.insert("child".into(), Value::from(*child));
            ("terminal.release", Value::Object(m))
        }
    }
}

/// `pty.*` headless PTY primitive (docs/features/headless-pty/index.md#내부-동작-headless-valid). `terminal.*`(자식 터미널 surface) 와
/// 별개 네임스페이스 — pty id 로만 조작하고 Surface 를 만들지 않는다.
pub(super) fn pty_command_to_method_params(
    command: &crate::commands::PtyCommands,
) -> (&'static str, serde_json::Value) {
    use crate::commands::PtyCommands as P;
    match command {
        P::Spawn {
            cwd,
            idle_timeout,
            command,
        } => {
            let mut m = serde_json::Map::new();
            if let Some(c) = normalize_cwd_or_exit(cwd.as_deref()) {
                m.insert("cwd".into(), serde_json::Value::from(c));
            }
            if let Some(secs) = idle_timeout {
                m.insert("idle_timeout_secs".into(), serde_json::Value::from(*secs));
            }
            m.insert("command".into(), serde_json::json!(command));
            ("pty.spawn", serde_json::Value::Object(m))
        }
        P::Write { id, text } => ("pty.write", serde_json::json!({ "id": id, "text": text })),
        P::Read {
            id,
            lines,
            show_dim,
        } => (
            "pty.read",
            serde_json::json!({ "id": id, "lines": lines, "show_dim": show_dim }),
        ),
        P::State { id } => ("pty.wait", serde_json::json!({ "id": id })),
        P::Kill { id } => ("pty.kill", serde_json::json!({ "id": id })),
        P::List => ("pty.list", serde_json::json!({})),
        P::AttachSurface { pty_id, pane_id } => (
            "pty.attach_surface",
            serde_json::json!({ "id": pty_id, "pane_id": pane_id }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::Commands;

    fn request(args: &[&str]) -> tasty_ipc::protocol::JsonRpcRequest {
        use clap::Parser;
        let command: Commands = crate::Cli::try_parse_from(args)
            .expect("parse")
            .command
            .expect("subcommand");
        super::super::command_to_request(&command)
    }

    #[test]
    fn an_external_parent_replaces_the_numeric_parent_on_the_wire() {
        let spawn = request(&[
            "tasty",
            "terminal",
            "spawn",
            "--parent-external",
            "codex:t1",
            "--workspace",
            "w",
            "--command",
            "ls",
        ]);
        assert_eq!(spawn.method, "terminal.spawn");
        assert_eq!(spawn.params["parent_external"], "codex:t1");
        assert!(spawn.params.get("parent").is_none());

        for (sub, extra) in [
            ("children", &[][..]),
            ("kill", &["--child", "1"][..]),
            ("release", &["--child", "1"][..]),
            ("respawn", &["--child", "1"][..]),
            ("broadcast", &["hi"][..]),
        ] {
            let mut args = vec![
                "tasty",
                "terminal",
                sub,
                "--parent-external",
                "codex:t1",
                "--workspace",
                "w",
            ];
            args.extend_from_slice(extra);
            let req = request(&args);
            assert_eq!(req.params["parent_external"], "codex:t1", "{sub}");
            assert_eq!(req.params["workspace"], "w", "{sub}");
            assert!(req.params.get("surface").is_none(), "{sub}");
        }
    }

    #[test]
    fn children_format_stays_in_the_cli_and_accepts_only_table_and_json() {
        use clap::Parser;
        for args in [
            &["tasty", "terminal", "children"][..],
            &["tasty", "terminal", "children", "--format", "json"][..],
        ] {
            assert!(request(args).params.get("format").is_none(), "{args:?}");
        }
        let dot = crate::Cli::try_parse_from(["tasty", "terminal", "children", "--format", "dot"]);
        assert!(dot.is_err());
    }

    #[test]
    fn an_external_parent_conflicts_with_surface_and_workspace_needs_it() {
        use clap::Parser;
        let both = crate::Cli::try_parse_from([
            "tasty",
            "terminal",
            "children",
            "--surface",
            "7",
            "--parent-external",
            "codex:t1",
        ]);
        assert!(both.is_err());
        let workspace_only =
            crate::Cli::try_parse_from(["tasty", "terminal", "children", "--workspace", "w"]);
        assert!(workspace_only.is_err());
    }
}
