//! 원격 workspace.create를 호출한다. 로컬 상태를 바꾸지 않으며 원격 생성은 Agent 요청으로 처리된다.
//! 블로킹 I/O이므로 CLI 또는 호스트 워커에서 호출한다.

use anyhow::{Context, Result};

use crate::browse::{probe_method, resolve_endpoint};
use tasty_ssh::SshTarget;

/// 원격 생성 응답에서 추린 workspace 식별 정보.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreatedRemoteWorkspace {
    pub id: u32,
    pub name: String,
    pub index: u32,
    /// 생성과 함께 만들어진 첫 surface id (원격 기준).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_id: Option<u32>,
}

/// 접속된 포트로 `workspace.create` 1회. 터널 수명은 호출자가 관리한다 — attach 를
/// 이어 붙이는 호스트 경로는 같은 터널을 mirror 세션에 그대로 실어 살려야 한다.
///
/// name/cwd는 생략할 수 있다. cwd 유효성은 원격이 검사하며 로컬 파일시스템으로 미리 판단하지 않는다.
pub fn create_via_port(
    port: u16,
    name: Option<&str>,
    cwd: Option<&str>,
) -> Result<CreatedRemoteWorkspace> {
    let mut params = serde_json::Map::new();
    if let Some(n) = name {
        params.insert("name".to_string(), serde_json::Value::from(n));
    }
    if let Some(c) = cwd {
        params.insert("cwd".to_string(), serde_json::Value::from(c));
    }
    let resp = probe_method(port, "workspace.create", serde_json::Value::Object(params))
        .context("원격 workspace.create 실패")?;
    let id = resp
        .get("id")
        .and_then(|v| v.as_u64())
        .context("원격 workspace.create 응답에 id 가 없습니다")? as u32;
    Ok(CreatedRemoteWorkspace {
        id,
        name: resp
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        index: resp.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        surface_id: resp
            .get("surface_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    })
}

/// 접속 후 workspace를 만들고 터널을 닫는 CLI 내부용 경로.
/// attach를 이어갈 호출자는 resolve_endpoint와 create_via_port로 터널 수명을 직접 관리한다.
#[doc(hidden)]
pub fn create(
    target: &SshTarget,
    remote_tasty: &str,
    port_mode: &str,
    port_file: Option<&str>,
    name: Option<&str>,
    cwd: Option<&str>,
) -> Result<CreatedRemoteWorkspace> {
    let (tunnel, port) = resolve_endpoint(target, remote_tasty, port_mode, port_file)?;
    create_via_port(port, name, cwd)
        .map_err(|e| crate::browse::explain_tunnel_failure(tunnel.as_ref(), e))
}
