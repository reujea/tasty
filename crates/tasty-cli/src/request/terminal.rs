//! `tasty terminal` · `tasty pty` CLI → JsonRpcRequest 매핑.
//!
//! 두 네임스페이스는 같은 PTY 를 다루지만 대상이 다르다 — `terminal.*` 는 자식 터미널
//! surface 를, `pty.*` 는 surface 없는 headless PTY primitive([ADR-0050](../../../docs/adr/0050-headless-pty-primitive.md))
//! 를 id 로 조작한다. 한 자리에 두는 이유는 둘 다 `normalize_cwd_or_exit` 로 cwd 를
//! 정규화하고, 그 외에는 부모의 어떤 상태도 안 본다는 것이다.

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

    match command {
        T::Spawn {
            surface,
            workspace,
            pane,
            cwd,
            command,
            role,
            nickname,
        } => {
            let mut m = Map::new();
            // parent = --surface 또는 caller TASTY_SURFACE_ID. 둘 다 없으면 대상 불명.
            let Some(parent) = resolve_surface_id(*surface) else {
                crate::out::errln!("{}", tasty_i18n::t("cli.terminal.spawn_no_parent"));
                std::process::exit(1);
            };
            m.insert("parent".into(), Value::from(parent));
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
        T::Children { surface } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
            ("terminal.children", Value::Object(m))
        }
        T::Parent { surface } => ("terminal.parent", serde_json::json!({ "surface": surface })),
        T::State { surface } => ("terminal.state", serde_json::json!({ "surface": surface })),
        T::Kill { surface, child } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
            m.insert("child".into(), Value::from(*child));
            ("terminal.kill", Value::Object(m))
        }
        T::Respawn {
            surface,
            child,
            cwd,
            command,
            role,
            nickname,
        } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
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
            role,
        } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
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
        T::Release { surface, child } => {
            let mut m = Map::new();
            put_u32(&mut m, "surface", resolve_surface_id(*surface));
            m.insert("child".into(), Value::from(*child));
            ("terminal.release", Value::Object(m))
        }
    }
}

/// `pty.*` headless PTY primitive (ADR-0050). `terminal.*`(자식 터미널 surface) 와
/// 별개 네임스페이스 — pty id 로만 조작하고 Surface 를 만들지 않는다.
pub(super) fn pty_command_to_method_params(
    command: &crate::commands::PtyCommands,
) -> (&'static str, serde_json::Value) {
    use crate::commands::PtyCommands as P;
    match command {
        P::Spawn { cwd, command } => {
            let mut m = serde_json::Map::new();
            if let Some(c) = normalize_cwd_or_exit(cwd.as_deref()) {
                m.insert("cwd".into(), serde_json::Value::from(c));
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
        P::Wait { id } => ("pty.wait", serde_json::json!({ "id": id })),
        P::Kill { id } => ("pty.kill", serde_json::json!({ "id": id })),
        P::List => ("pty.list", serde_json::json!({})),
        P::AttachSurface { pty_id, pane_id } => (
            "pty.attach_surface",
            serde_json::json!({ "id": pty_id, "pane_id": pane_id }),
        ),
    }
}
