//! remote_browse로 원격 workspace를 조회하고 텍스트 또는 JSON으로 출력한다.
//! CLI와 remote.workspaces IPC는 같은 조회 경로를 쓰며 로컬 사용자 상태는 바꾸지 않는다.

use anyhow::{Context, Result};
use tasty_i18n::{t, t_args};

use crate::out::outln;
use crate::remote_browse;
use crate::ssh::SshTarget;

/// 호출자가 profile/ssh 중 하나로 접속 설정을 정한 뒤 호출한다.
pub fn run_remote_workspaces(
    target: SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let dest = target.destination.clone();
    let list = remote_browse::browse(&target, remote_tasty, port_mode, port_file)
        .with_context(|| t_args("cli.remote_workspaces.browse_failed", &[dest.as_str()]))?;

    if json_output {
        // 자동화와 팝업이 읽는 JSON 키는 번역하지 않는다.
        let json = serde_json::to_string_pretty(&list)
            .context(t("cli.remote_workspaces.serialize_failed"))?;
        outln!("{json}")?;
        return Ok(());
    }

    if list.is_empty() {
        outln!("{}", t_args("cli.remote_workspaces.none", &[dest.as_str()]))?;
        return Ok(());
    }

    outln!(
        "{}",
        t_args(
            "cli.remote_workspaces.header",
            &[dest.as_str(), &list.len().to_string()],
        )
    )?;
    for ws in &list {
        let id_str = ws.id.to_string();
        let pane_str = ws.pane_count.to_string();
        let busy_str = ws.busy_count.to_string();
        if let Some(holder) = ws.holder {
            outln!(
                "{}",
                t_args(
                    "cli.remote_workspaces.row_attached",
                    &[&id_str, &ws.name, &pane_str, &busy_str, &holder.to_string()],
                )
            )?;
        } else {
            outln!(
                "{}",
                t_args(
                    "cli.remote_workspaces.row_free",
                    &[&id_str, &ws.name, &pane_str, &busy_str],
                )
            )?;
        }
        if ws.mirror {
            // 원격에서도 mirror인 workspace는 다시 mirror할 수 없다. 이유를 행 아래에 적는다.
            outln!(
                "    {}",
                t_args("remote.row.reason.chain", &[ws.name.as_str()])
            )?;
        }
    }
    Ok(())
}
