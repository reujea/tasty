//! `tasty remote attach --into-gui --ssh|--profile`의 `remote.attach` 인자.

use crate::commands::RemoteCommands;

/// 값이 없는 키는 싣지 않는다. 처리기가 `remote_workspace`의 null을 잘못된 값으로 읽는다.
pub(super) fn attach_params(command: &RemoteCommands) -> serde_json::Value {
    let RemoteCommands::Attach {
        workspace,
        ssh,
        profile,
        remote_tasty,
        remote_port_mode,
        new_workspace,
        name,
        cwd,
        ..
    } = command
    else {
        unreachable!("only remote attach opens a tunnel from the GUI");
    };
    let mut params = serde_json::json!({
        "remote_tasty": remote_tasty,
        "remote_port_mode": remote_port_mode,
    });
    let optional = [
        ("ssh", ssh.as_deref().map(serde_json::Value::from)),
        ("profile", profile.as_deref().map(serde_json::Value::from)),
        ("remote_workspace", workspace.map(serde_json::Value::from)),
        ("new_workspace", new_workspace.then_some(true.into())),
        ("name", name.as_deref().map(serde_json::Value::from)),
        ("cwd", cwd.as_deref().map(serde_json::Value::from)),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            params[key] = value;
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;

    use crate::request::command_to_request;
    use crate::{Cli, Commands};

    fn sent(args: &[&str]) -> (String, serde_json::Value) {
        let cli = Cli::try_parse_from(args).unwrap();
        let command: Commands = cli.command.unwrap();
        let request = command_to_request(&command);
        (request.method, request.params)
    }

    #[test]
    fn a_target_without_a_port_asks_the_gui_to_open_the_tunnel() {
        let (method, params) = sent(&[
            "tasty",
            "remote",
            "attach",
            "--into-gui",
            "--workspace",
            "2",
            "--profile",
            "gx10",
        ]);
        assert_eq!(method, "remote.attach");
        assert_eq!(
            params,
            json!({
                "profile": "gx10",
                "remote_workspace": 2,
                "remote_tasty": "tasty",
                "remote_port_mode": "auto",
            })
        );
    }

    #[test]
    fn a_new_workspace_carries_its_name_and_remote_cwd_as_given() {
        let (method, params) = sent(&[
            "tasty",
            "remote",
            "attach",
            "--into-gui",
            "--new-workspace",
            "--ssh",
            "user@host",
            "--name",
            "build",
            "--cwd",
            "relative/on/remote",
        ]);
        assert_eq!(method, "remote.attach");
        assert_eq!(
            params,
            json!({
                "ssh": "user@host",
                "new_workspace": true,
                "name": "build",
                "cwd": "relative/on/remote",
                "remote_tasty": "tasty",
                "remote_port_mode": "auto",
            })
        );
    }

    /// 기존 호출 형태는 그대로다. `--profile`을 함께 줘도 포트가 있으면 터널을 열지 않는다.
    #[test]
    fn a_target_port_keeps_attach_into_gui() {
        for extra in [&[][..], &["--profile", "gx10"][..]] {
            let mut args = vec![
                "tasty",
                "remote",
                "attach",
                "--into-gui",
                "--workspace",
                "2",
                "--target-port",
                "45123",
            ];
            args.extend_from_slice(extra);
            let (method, params) = sent(&args);
            assert_eq!(method, "attach.into_gui");
            assert_eq!(params, json!({ "port": 45123, "workspace": 2 }));
        }
    }

    #[test]
    fn neither_a_target_nor_a_port_stays_on_attach_into_gui() {
        let (method, _) = sent(&[
            "tasty",
            "remote",
            "attach",
            "--into-gui",
            "--workspace",
            "2",
        ]);
        assert_eq!(method, "attach.into_gui");
    }

    /// 연결 끊기 대상은 mirror 의 로컬 workspace id 다. 원격 id 와 섞지 않도록 이름을 붙여 보낸다.
    #[test]
    fn disconnect_sends_the_local_mirror_workspace_id() {
        let (method, params) = sent(&["tasty", "remote", "disconnect", "--workspace", "7"]);
        assert_eq!(method, "remote.disconnect");
        assert_eq!(params, json!({ "workspace_id": 7 }));
    }

    /// Given 저장된 원격 행의 로컬 id, When `tasty remote connect --workspace` 로 부르면,
    /// Then `remote.connect` 에 그 id 를 `workspace_id` 로 보낸다.
    #[test]
    fn connect_sends_the_local_saved_row_id() {
        let (method, params) = sent(&["tasty", "remote", "connect", "--workspace", "7"]);
        assert_eq!(method, "remote.connect");
        assert_eq!(params, json!({ "workspace_id": 7 }));
    }
}
