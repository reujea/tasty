//! `mdedit.*` IPC(= `tasty mdedit …` CLI). 대상은 파일 경로다 — 편집기가 열려 있지 않아도
//! 되고 포커스를 보지 않는다(docs/surfaces/mdedit/index.md#ipc-namespace-는-mdedit). 쓰기 연산은 모두 같은 길을 지난다:
//! `--if-match` 확인 → 가드(A4) → 비밀값(A5) → 직전 내용 스냅샷(E5) → 원자적 저장(E2) →
//! 그 파일을 연 편집기에 에이전트 출처로 알림(A3·A6).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tasty_plugin_sdk::IpcMethodError;

use crate::core::guard::{self, DocConfig, ViolationReason};
use crate::core::leak::{self, SecretsMode};
use crate::core::ops::{self, OpError, SectionOp, TaskTo};
use crate::core::outline;
use crate::core::review::Origin;
use crate::core::snapshot::Reason;
use crate::core::textio::{self, Eol};
use crate::session::{Ctx, Session};
use crate::{MdEditPlugin, RELOAD_METHOD, canonical};

/// `--if-match` 불일치(오래된 내용 위에 쓰려 했다).
pub const CODE_STALE: i32 = -32040;
/// 가드 섹션 쓰기 거부.
pub const CODE_GUARDED: i32 = -32041;
/// 비밀값 차단.
pub const CODE_SECRET: i32 = -32042;
/// 헤딩·체크박스 지정 실패(0 개 또는 2 개 이상).
pub const CODE_TARGET: i32 = -32043;

fn err(code: i32, msg: impl Into<String>) -> IpcMethodError {
    IpcMethodError::with_code(msg, code)
}

fn str_param<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

fn need<'a>(p: &'a Value, k: &str) -> Result<&'a str, IpcMethodError> {
    str_param(p, k).ok_or_else(|| IpcMethodError::invalid_params(&format!("missing '{k}'")))
}

fn file_of(p: &Value) -> Result<PathBuf, IpcMethodError> {
    let f = need(p, "file")?;
    let path = Path::new(f);
    if !path.is_absolute() {
        return Err(IpcMethodError::invalid_params(
            "'file' must be an absolute path (the CLI resolves --file against your cwd)",
        ));
    }
    Ok(canonical(path))
}

fn op_err(e: OpError) -> IpcMethodError {
    match e {
        OpError::InvalidArgument(m) => IpcMethodError::invalid_params(&m),
        other => err(CODE_TARGET, other.to_string()),
    }
}

fn read_raw(path: &Path) -> Result<String, IpcMethodError> {
    std::fs::read_to_string(path)
        .map_err(|e| IpcMethodError::new(format!("{}: {e}", path.display())))
}

/// 로컬 `HH:MM`.
fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

impl MdEditPlugin {
    pub(crate) fn dispatch_ipc(
        &mut self,
        method: &str,
        p: &Value,
    ) -> Result<Value, IpcMethodError> {
        match method {
            RELOAD_METHOD => self.reload(p),
            "mdedit.read" => self.read(p),
            "mdedit.section" => self.section(p),
            "mdedit.task" => self.task(p),
            "mdedit.agent_note" => self.agent_note(p),
            "mdedit.snapshot" => self.snapshot(p),
            "mdedit.status" => self.status(p),
            // 호스트가 plugin 을 내리기 전에 묻는다(docs/dev-guide/plugin-development.md#cli--ipc-namespace) — 저장 안 한 편집이 있으면 바쁘다.
            "mdedit.lifecycle_busy" => {
                let dirty: Vec<String> = self
                    .sessions
                    .values()
                    .filter(|s| s.dirty())
                    .map(Session::file_name)
                    .collect();
                Ok(json!({
                    "busy": !dirty.is_empty(),
                    "reason": if dirty.is_empty() { String::new() } else { format!("unsaved edits: {}", dirty.join(", ")) },
                }))
            }
            other => Err(IpcMethodError::not_found(other)),
        }
    }

    /// 감시 스레드의 변경 통지.
    fn reload(&mut self, p: &Value) -> Result<Value, IpcMethodError> {
        let sid = p
            .get("surface")
            .and_then(Value::as_u64)
            .ok_or_else(|| IpcMethodError::invalid_params("missing 'surface'"))?
            as u32;
        let keep = self.retention();
        let ctx = Ctx {
            store: &self.store,
            keep,
        };
        let c = match self.sessions.get_mut(&sid) {
            Some(s) => s.on_disk_change(Origin::External, &ctx),
            None => return Ok(json!({ "ok": false })),
        };
        self.present(sid, c, None);
        Ok(json!({ "ok": true, "surface_id": sid }))
    }

    fn read(&self, p: &Value) -> Result<Value, IpcMethodError> {
        let path = file_of(p)?;
        let raw = read_raw(&path)?;
        let d = textio::decode(&raw);
        let mut out = json!({
            "file": path.to_string_lossy(),
            "hash": textio::content_hash(&raw),
            "eol": match d.eol { Eol::Lf => "lf", Eol::Crlf => "crlf", Eol::Mixed => "mixed" },
        });
        if p.get("outline").and_then(Value::as_bool).unwrap_or(false) {
            out["outline"] = Value::Array(
                outline::headings(&d.text)
                    .into_iter()
                    .map(|h| json!({ "level": h.level, "line": h.line + 1, "text": h.text }))
                    .collect(),
            );
        } else {
            out["text"] = Value::String(d.text);
        }
        Ok(out)
    }

    fn section(&mut self, p: &Value) -> Result<Value, IpcMethodError> {
        let op = need(p, "op")?;
        let heading = need(p, "heading")?.to_string();
        if op == "get" {
            let path = file_of(p)?;
            let raw = read_raw(&path)?;
            let d = textio::decode(&raw);
            let (sec, text) = ops::section_get(&d.text, &heading).map_err(op_err)?;
            return Ok(json!({
                "file": path.to_string_lossy(),
                "hash": textio::content_hash(&raw),
                "line": sec.heading.line + 1,
                "text": text,
            }));
        }
        let kind = SectionOp::parse(op).ok_or_else(|| {
            IpcMethodError::invalid_params(
                "op must be one of get|append|prepend|insert-before|replace",
            )
        })?;
        let body = str_param(p, "text").unwrap_or_default().to_string();
        if body.is_empty() && kind != SectionOp::Replace {
            return Err(IpcMethodError::invalid_params("missing 'text'"));
        }
        self.write_op(p, |text| {
            ops::section_edit(text, &heading, kind, &body).map(|e| e.apply(text))
        })
    }

    fn task(&mut self, p: &Value) -> Result<Value, IpcMethodError> {
        let action = str_param(p, "action").unwrap_or("toggle");
        if action != "toggle" {
            return Err(IpcMethodError::invalid_params("action must be 'toggle'"));
        }
        let heading = need(p, "heading")?.to_string();
        let needle = need(p, "match")?.to_string();
        let to = match str_param(p, "to") {
            None => TaskTo::Toggle,
            Some(t) => TaskTo::parse(t).ok_or_else(|| {
                IpcMethodError::invalid_params("to must be one of toggle|checked|unchecked")
            })?,
        };
        self.write_op(p, |text| {
            ops::task_toggle(text, &heading, &needle, to).map(|e| e.apply(text))
        })
    }

    fn agent_note(&mut self, p: &Value) -> Result<Value, IpcMethodError> {
        let heading = need(p, "heading")?.to_string();
        let title = need(p, "title")?;
        let body = str_param(p, "text").unwrap_or_default();
        let time = if p.get("no_time").and_then(Value::as_bool).unwrap_or(false) {
            None
        } else {
            Some(str_param(p, "time").map_or_else(now_hhmm, str::to_string))
        };
        let block = ops::agent_note_block(time.as_deref(), title, body).map_err(op_err)?;
        self.write_op(p, |text| {
            ops::agent_note(text, &heading, &block).map(|e| e.apply(text))
        })
    }

    fn snapshot(&mut self, p: &Value) -> Result<Value, IpcMethodError> {
        let path = file_of(p)?;
        match str_param(p, "action").unwrap_or("list") {
            "list" => {
                let list = self
                    .store
                    .list(&path)
                    .map_err(|e| IpcMethodError::new(e.to_string()))?;
                Ok(json!({ "file": path.to_string_lossy(), "snapshots": list }))
            }
            "restore" => {
                let id = need(p, "id")?;
                let raw = self
                    .store
                    .read(&path, id)
                    .map_err(|e| IpcMethodError::invalid_params(&e.to_string()))?;
                let restored = textio::decode(&raw).text;
                self.write_op_with(p, Reason::Restore, true, |_| Ok(restored.clone()))
            }
            other => Err(IpcMethodError::invalid_params(&format!(
                "action must be list|restore, got '{other}'"
            ))),
        }
    }

    fn status(&self, p: &Value) -> Result<Value, IpcMethodError> {
        let path = file_of(p)?;
        let raw = std::fs::read_to_string(&path).ok();
        let open: Vec<Value> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.path == path)
            .map(|(id, s)| {
                json!({
                    "surface_id": id,
                    "dirty": s.dirty(),
                    "conflict": s.conflict.is_some(),
                    "review_hunks": s.review_hunks().len(),
                })
            })
            .collect();
        let cfg = DocConfig::load(&path);
        Ok(json!({
            "file": path.to_string_lossy(),
            "exists": raw.is_some(),
            "hash": raw.as_deref().map(textio::content_hash),
            "open": open,
            "guards": cfg.guards.len(),
            "sidecar_warnings": cfg.warnings,
        }))
    }

    fn write_op(
        &mut self,
        p: &Value,
        f: impl FnOnce(&str) -> Result<String, OpError>,
    ) -> Result<Value, IpcMethodError> {
        self.write_op_with(p, Reason::Agent, false, |t| f(t).map_err(op_err))
    }

    /// 쓰기 연산의 공통 길. `restoring` 이면 가드가 "그 텍스트로 되돌리는" 쓰기를 허용한다.
    fn write_op_with(
        &mut self,
        p: &Value,
        reason: Reason,
        restoring: bool,
        f: impl FnOnce(&str) -> Result<String, IpcMethodError>,
    ) -> Result<Value, IpcMethodError> {
        let path = file_of(p)?;
        let raw = read_raw(&path)?;
        let hash = textio::content_hash(&raw);
        if let Some(want) = str_param(p, "if_match")
            && !want.is_empty()
            && want != hash
        {
            return Err(err(
                CODE_STALE,
                format!(
                    "file changed since you read it (if-match {want}, now {hash}) — read again"
                ),
            ));
        }
        let d = textio::decode(&raw);
        let next = f(&d.text)?;
        if next == d.text {
            return Ok(json!({ "file": path.to_string_lossy(), "changed": false, "hash": hash }));
        }
        let cfg = DocConfig::load(&path);
        if let Some(v) =
            guard::check(&d.text, &next, &cfg, restoring.then_some(next.as_str())).first()
        {
            let what = match v.reason {
                ViolationReason::HeadingRemoved => "removes or renames",
                ViolationReason::Modified => "modifies",
            };
            return Err(err(
                CODE_GUARDED,
                format!(
                    "write {what} guarded section '{}' (line {}) — see the sidecar",
                    v.heading, v.line
                ),
            ));
        }
        let leaks = leak::scan_added(&d.text, &next, &cfg.secrets);
        if let Some(l) = leaks.first()
            && cfg.secrets.mode == SecretsMode::Block
        {
            return Err(err(
                CODE_SECRET,
                format!(
                    "line {} looks like a plaintext secret ({}) — write a label like [pw:P1] instead",
                    l.line, l.kind
                ),
            ));
        }
        let keep = self.retention();
        if let Err(e) = self.store.save(&path, &raw, reason, keep) {
            tracing::warn!("mdedit: snapshot before agent write failed: {e}");
        }
        let eol = if d.eol == Eol::Mixed { Eol::Lf } else { d.eol };
        let out = textio::encode(&next, eol, d.bom);
        textio::save_atomic(&path, out.as_bytes())
            .map_err(|e| IpcMethodError::new(format!("{}: {e}", path.display())))?;
        self.notify_agent_write(&path, &d.text, &next);
        Ok(json!({
            "file": path.to_string_lossy(),
            "changed": true,
            "hash_before": hash,
            "hash": textio::content_hash(&out),
            "warnings": leaks.iter().map(|l| format!("line {} looks like a secret ({})", l.line, l.kind)).collect::<Vec<_>>(),
        }))
    }

    /// 같은 파일을 연 편집기에 에이전트 쓰기를 알린다 — 출처가 확실하므로 감시 스레드를
    /// 기다리지 않고 바로 반영한다(감시 스레드는 뒤에 같은 내용을 보고 아무것도 안 한다).
    fn notify_agent_write(&mut self, path: &Path, before: &str, after: &str) {
        let keep = self.retention();
        let ids: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.path == path)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let ctx = Ctx {
                store: &self.store,
                keep,
            };
            let c = match self.sessions.get_mut(&id) {
                Some(s) => {
                    s.agent.record(before, after);
                    s.on_disk_change(Origin::Agent, &ctx)
                }
                None => continue,
            };
            self.present(id, c, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_distinct() {
        let mut v = vec![CODE_STALE, CODE_GUARDED, CODE_SECRET, CODE_TARGET];
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 4);
    }
}
