//! Plugin CLI 명령 중 호스트 IPC를 거치지 않는 local-only 처리.

use std::io::{Read, Seek, SeekFrom};
use std::net::TcpStream;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Map, Value, json};

use tasty_ipc::client::IpcConnection;
use tasty_plugin_manifest::Manifest;

use crate::out::{out, outln};

fn log_dir() -> Result<PathBuf> {
    tasty_utils::path::tasty_home()
        .map(|d| d.join("plugins-logs"))
        .ok_or_else(|| anyhow::anyhow!("{}", tasty_i18n::t("cli.plugin.home_unresolved")))
}

/// Polls `plugin.audit_follow` every `interval_ms` and prints new records as
/// they arrive. Runs until Ctrl-C (no graceful shutdown — process exit).
pub fn run_audit_follow(
    caller_kind: Option<&str>,
    caller_id: Option<&str>,
    method_prefix: Option<&str>,
    decision: Option<&str>,
    batch: u64,
    interval_ms: u64,
    port_file: Option<&str>,
) -> Result<()> {
    let port = crate::port_file::read_port(port_file)?;
    let stream = TcpStream::connect(format!("127.0.0.1:{}", port)).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.request.connect_failed",
                &port.to_string(),
                &e.to_string()
            )
        )
    })?;
    let mut conn = IpcConnection::new(stream)?;

    let mut base = Map::new();
    if let Some(v) = caller_kind {
        base.insert("caller_kind".into(), json!(v));
    }
    if let Some(v) = caller_id {
        base.insert("caller_id".into(), json!(v));
    }
    if let Some(v) = method_prefix {
        base.insert("method_prefix".into(), json!(v));
    }
    if let Some(v) = decision {
        base.insert("decision".into(), json!(v));
    }
    base.insert("limit".into(), json!(batch));

    let session_token = std::env::var("TASTY_SESSION_TOKEN").ok();
    let mut next_id: i64 = 1;
    let mut after_ts: Option<u64> = None;
    let mut after_seq: Option<u64> = None;
    loop {
        let mut params = base.clone();
        if let Some(t) = after_ts {
            params.insert("after_ts_ms".into(), json!(t));
        }
        if let Some(s) = after_seq {
            params.insert("after_seq".into(), json!(s));
        }
        let req = tasty_ipc::protocol::JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: "plugin.audit_follow".to_string(),
            params: Value::Object(params),
            id: Some(json!(next_id)),
            session_token: session_token.clone(),
        };
        next_id += 1;
        // 호스트 오류는 `main` 까지 올라가 std 가 찍는다 — `data` 는 둘째 줄로 싣는다(ADR-0512).
        let resp = conn.send(&req).map_err(crate::rpc_error::with_data_line)?;
        if let Some(ts) = resp.get("next_after_ts_ms").and_then(|v| v.as_u64()) {
            after_ts = Some(ts);
        }
        if let Some(seq) = resp.get("next_after_seq").and_then(|v| v.as_u64()) {
            after_seq = Some(seq);
        }
        if let Some(records) = resp.get("records").and_then(|v| v.as_array()) {
            for rec in records {
                outln!("{}", serde_json::to_string(rec).unwrap_or_default())?;
            }
            // 배치 경계 flush(종전 best-effort flush 자리). 파이프 생존 프로브가 아니다 —
            // 레코드는 `outln!` 로 개행과 함께 이미 write 됐으므로 여기서 버퍼는 비어 있고,
            // 빈 버퍼 flush 는 write(2) 를 내지 않아 EPIPE 를 감지하지 못한다. 읽는 쪽이
            // 닫혔을 때 이 무한 루프를 실제로 빠져나가는 지점은 **다음 레코드의 `outln!`**
            // (`StdoutClosed` → 종료 코드 0, ADR-0101)이고, 레코드가 더 없으면
            // `tail -f | head -1` 처럼 계속 대기한다(종전과 같다).
            crate::out::flush()?;
        }
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
    }
}

/// `tasty plugin doctor <id>` — Plugin manifest 의 contributes.detector / handler 를
/// 점검해 현재 호스트가 이해하지 못하는 rule kind (= `DetectorRuleDecl::Unknown`) 를
/// 표시한다. 호스트가 실행 중이지 않아도 작동하는 local-only 명령.
pub fn run_plugin_doctor(plugin_id: &str) -> Result<()> {
    let root = tasty_host_plugin::plugin_root()
        .ok_or_else(|| anyhow::anyhow!("{}", tasty_i18n::t("cli.plugin.root_unresolved")))?;
    let plugin_dir = root.join(plugin_id);
    if !plugin_dir.join("tasty-plugin.toml").exists() {
        anyhow::bail!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.plugin.not_installed",
                plugin_id,
                &plugin_dir.join("tasty-plugin.toml").display().to_string()
            )
        );
    }
    // F.B.13-3: host file 도메인 검증 (validate_bin_extras) 은 본 바이너리 잔존.
    // CLI tasty-cli 단독 빌드 가능을 위해 schema 검증 (Manifest::load 내장) 까지만
    // 수행. install/remove 경로의 daemon IPC handler 가 bin extras 를 추가 검증.
    let manifest = Manifest::load(&plugin_dir).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            tasty_i18n::t_fmt2("cli.plugin.manifest_load_failed", plugin_id, &e.to_string())
        )
    })?;

    outln!(
        "{}",
        tasty_i18n::t_fmt("cli.plugin.doctor_header", &manifest.id)
    )?;
    outln!("  name:             {}", manifest.name)?;
    outln!("  version:          {}", manifest.version)?;
    outln!("  manifest_version: {}", manifest.manifest_version)?;
    outln!("  api_version:      {}", manifest.api_version)?;

    // F.B.2: contributes.detector/handler 가 opaque Value 로 전환되어 본 CLI 표시는
    // Value 의 필드를 직접 읽어 요약한다. concrete schema 검증은 manifest::load 내
    // bin glue 에서 수행하므로 여기 도달했다는 것은 schema 가 valid 함을 의미.
    let detectors = &manifest.contributes.detector;
    outln!()?;
    outln!(
        "{}",
        tasty_i18n::t_fmt("cli.plugin.doctor_detectors", &detectors.len().to_string())
    )?;
    let mut total_unsupported = 0_usize;
    for v in detectors {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
        let rules: Vec<serde_json::Value> = v
            .get("rule")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        let total = rules.len();
        let unsupported: Vec<&serde_json::Value> = rules
            .iter()
            .filter(|r| {
                let kind = r.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                !matches!(
                    kind,
                    "extension"
                        | "path_glob"
                        | "mime"
                        | "magic"
                        | "is_directory"
                        | "structure_check"
                )
            })
            .collect();
        let ok = total - unsupported.len();
        total_unsupported += unsupported.len();
        outln!(
            "{}",
            tasty_i18n::t_args(
                "cli.plugin.doctor_detector_row",
                &[id, &ok.to_string(), &unsupported.len().to_string()]
            )
        )?;
        for rule in &unsupported {
            let kind_name = rule.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
            outln!(
                "{}",
                tasty_i18n::t_fmt("cli.plugin.doctor_rule_unsupported", kind_name)
            )?;
        }
    }

    let handlers = &manifest.contributes.handler;
    outln!()?;
    outln!(
        "{}",
        tasty_i18n::t_fmt("cli.plugin.doctor_handlers", &handlers.len().to_string())
    )?;
    for v in handlers {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
        let detector_id = v.get("detector").and_then(|x| x.as_str()).unwrap_or("?");
        let action_summary = v
            .get("action")
            .and_then(|a| a.as_object())
            .and_then(|obj| {
                let kind = obj.get("kind").and_then(|k| k.as_str())?;
                match kind {
                    "open_surface" => Some(format!(
                        "surface \"{}\"",
                        obj.get("surface_kind")
                            .and_then(|x| x.as_str())
                            .unwrap_or("?")
                    )),
                    "ipc" => Some(format!(
                        "ipc \"{}\"",
                        obj.get("method").and_then(|x| x.as_str()).unwrap_or("?")
                    )),
                    other => Some(tasty_i18n::t_fmt("cli.plugin.doctor_action_unknown", other)),
                }
            })
            .unwrap_or_else(|| tasty_i18n::t("cli.plugin.doctor_action_none").to_string());
        outln!(
            "  - {} → detector \"{}\" → {}",
            id,
            detector_id,
            action_summary
        )?;
    }

    if total_unsupported > 0 {
        outln!()?;
        outln!(
            "{}",
            tasty_i18n::t_fmt(
                "cli.plugin.doctor_unsupported_summary",
                &total_unsupported.to_string()
            )
        )?;
    }
    Ok(())
}

pub fn run_plugin_logs(plugin_id: &str, follow: bool) -> Result<()> {
    let path = log_dir()?.join(format!("{plugin_id}.log"));
    if !path.exists() {
        anyhow::bail!(
            "{}",
            tasty_i18n::t_fmt2(
                "cli.plugin.no_log_file",
                plugin_id,
                &path.display().to_string()
            )
        );
    }
    if !follow {
        let s = std::fs::read_to_string(&path)?;
        out!("{s}")?;
        return Ok(());
    }
    let mut file = std::fs::File::open(&path)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    out!("{buf}")?;
    let mut pos = file.metadata()?.len();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        file.seek(SeekFrom::Start(pos))?;
        let mut chunk = String::new();
        let n = file.read_to_string(&mut chunk)? as u64;
        if n > 0 {
            out!("{chunk}")?;
            pos += n;
        }
    }
}
