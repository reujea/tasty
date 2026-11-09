//! 세션의 **추가 작업 폴더** — Claude Code 의 `/add-dir` 를 대화 탭에서 쓰게 한다.
//!
//! 데몬이 띄우는 claude(`-p --input-format stream-json`)에 `/add-dir` 를 텍스트로 보내면 "isn't available in
//! this environment" 로 답한다(실측 2026-10-21, claude 2.1.304). 폴더는 띄울 때 `--add-dir` 로만 넘어가므로
//! 데몬이 세션별 목록을 들고 자식을 그 목록으로 다시 띄운다(상대 protocol 14 · `session.add_dir`). 이 모듈은
//! 그 메서드를 탭의 `/add-dir <경로>` 와 IPC `agent_session.add_dir` · `agent_session.remove_dir` 로 연다.
//!
//! 탭에서 친 것은 보통 말과 같은 자리를 탄다 — 진행 중 턴을 하나 열고 워커가 데몬에 묻고 그 결과를
//! `agent_session.turn_done` 으로 되던진다. 그래서 진행 표시 · 대기열 · 중복 방지가 말과 같고, 결과가
//! 대화에 한 턴으로 남는다(목록이 바뀌면 데몬이 자식을 다시 띄우므로 몇 초 걸린다).
use serde_json::{Value, json};

use crate::application::app_error::AppError;
use crate::application::request::session_for;
use crate::application::session_surface;
use crate::application::tell::Reply;
use crate::ports::host::SendHost;
use crate::{domain::i18n, domain::send_queue};

/// 탭에서 치는 커맨드.
pub const COMMAND: &str = "/add-dir";

/// 입력이 `/add-dir` 커맨드인가 — 맞으면 경로(또는 왜 못 쓰는지).
///
/// `~/` 는 홈으로 편다. 데몬은 절대 경로만 받으므로 상대 경로는 여기서 막는다 — 탭의 작업 폴더를
/// 기준으로 풀면 세션의 작업 폴더와 달라질 수 있어 추측하지 않는다.
pub fn parse_command(text: &str) -> Option<Result<String, String>> {
    let t = text.trim();
    let rest = t.strip_prefix(COMMAND)?;
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None; // `/add-dirx` 는 이 커맨드가 아니다.
    }
    let path = rest.trim();
    if path.is_empty() {
        return Some(Err(i18n::t("sup.err.add_dir_need_path")));
    }
    let expanded = match path.strip_prefix("~/") {
        Some(tail) => match std::env::var_os("HOME") {
            Some(home) => std::path::Path::new(&home).join(tail).display().to_string(),
            None => path.to_string(),
        },
        None => path.to_string(),
    };
    if !std::path::Path::new(&expanded).is_absolute() {
        return Some(Err(i18n::t1("sup.err.add_dir_absolute", path)));
    }
    Some(Ok(expanded))
}

/// 데몬의 답을 사람이 읽을 한두 줄로 — `add_dirs.{dirs, changed, applied}`.
pub fn summary(remove: bool, path: &str, v: &Value) -> String {
    let d = v.get("add_dirs").unwrap_or(v);
    let changed = d.get("changed").and_then(Value::as_bool).unwrap_or(false);
    let applied = d.get("applied").and_then(Value::as_bool).unwrap_or(false);
    let head = match (remove, changed, applied) {
        (false, true, true) => i18n::t1("sup.dir.added_now", path),
        (false, true, false) => i18n::t1("sup.dir.added_later", path),
        (false, false, _) => i18n::t1("sup.dir.already", path),
        (true, true, true) => i18n::t1("sup.dir.removed_now", path),
        (true, true, false) => i18n::t1("sup.dir.removed_later", path),
        (true, false, _) => i18n::t1("sup.dir.not_there", path),
    };
    let dirs: Vec<&str> = d
        .get("dirs")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let list = if dirs.is_empty() {
        i18n::t("sup.dir.none")
    } else {
        i18n::t1("sup.dir.list", &dirs.join(", "))
    };
    format!("{head}\n{list}")
}

/// 탭 쪽 처리 — 대화 · 대기열 · 활동을 함께 만지므로 plugin 의 메서드로 둔다.
impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 입력칸에 친 `/add-dir …` 면 처리하고 `true` — 보통 말과 같은 자리(진행 중 턴 → 워커 → `turn_done`)를 탄다.
    pub(crate) fn add_dir_typed(&mut self, host: &impl SendHost, sid: u32, url: &str) -> bool {
        let Some(text) = crate::adapters::driving::url::parse_input(url) else {
            return false;
        };
        let Some(parsed) = parse_command(&text) else {
            return false;
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            return true;
        };
        let staged = send_queue::parse_nonce(url);
        let busy = self.activity.is_busy(c);
        let Some(session) = c.session_id.clone() else {
            return true;
        };
        // 턴이 도는 동안은 보내지 않는다 — 데몬도 `invalid_state` 로 거절하지만, 그 전에 이 탭의 대기 칸을
        // 정리하고 사유를 보인다(도는 턴에 끼워 넣지 않는 것은 보통 말과 같다).
        if busy {
            session_surface::patch(host, sid, &send_queue::settle_msg(staged, &c.queued, None));
            session_surface::patch(
                host,
                sid,
                &crate::application::clear::note_msg(Some(&i18n::t("sup.err.add_dir_busy"))),
            );
            return true;
        }
        let at = crate::domain::convo::begin_turn(c, &text);
        session_surface::patch(
            host,
            sid,
            &send_queue::settle_msg(staged, &c.queued, Some(&text)),
        );
        self.activity.refresh_status(host, sid, c);
        let worker_host = host.clone();
        let sup = self.sup;
        let spawned = std::thread::Builder::new()
            .name(format!("agent-session-add-dir-{sid}"))
            .spawn(move || {
                let (ok, body) = match parsed {
                    Err(why) => (false, why),
                    Ok(path) => {
                        match sup.add_dir(&session, &path) {
                            Ok(v) => (true, summary(false, &path, &v)),
                            Err(e) => (false, e.to_string()),
                        }
                    }
                };
                // 되던지기가 실패하면 plugin 이 내려가는 중이다 — 조용히 끝낸다.
                #[expect(clippy::let_underscore_must_use, reason = "A failed self-invoke means the plugin is shutting down; nobody is left to tell.")]
                let _ = worker_host.self_invoke(
                    "agent_session.turn_done",
                    json!({ "surface_id": sid, "at": at, "ok": ok, "text": body }),
                );
            });
        if spawned.is_err()
            && let Some(c) = self.convos.get_mut(&sid)
        {
            crate::domain::convo::finish_turn(c, at, Err(i18n::t("sup.err.worker_spawn")));
        }
        true
    }

    /// `agent_session.add_dir` · `agent_session.remove_dir` — 워커에서 데몬에 묻고 답을 돌려준다.
    pub(crate) fn dir_later(&mut self, params: &Value, reply: Reply, remove: bool) {
        let Some(hh) = self.host.clone() else {
            reply(Err(AppError::new(i18n::t("sup.err.worker_spawn"))));
            return;
        };
        let target = session_for(&hh, params).and_then(|(sid, session)| {
            let raw = params
                .get("path")
                .and_then(Value::as_str)
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| AppError::new(i18n::t("sup.err.add_dir_need_path")))?;
            match parse_command(&format!("{COMMAND} {raw}")) {
                Some(Ok(path)) => Ok((sid, session, path)),
                Some(Err(why)) => Err(AppError::new(why)),
                None => Err(AppError::new(i18n::t("sup.err.add_dir_need_path"))),
            }
        });
        let (sid, session, path) = match target {
            Ok(t) => t,
            Err(e) => {
                reply(Err(e));
                return;
            }
        };
        let sup = self.sup;
        let spawned = std::thread::Builder::new()
            .name(format!("agent-session-dir-{sid}"))
            .spawn(move || {
                let got = if remove {
                    sup.remove_dir(&session, &path)
                } else {
                    sup.add_dir(&session, &path)
                };
                let r = got
                    .map(|v| {
                        let d = v.get("add_dirs").cloned().unwrap_or(Value::Null);
                        json!({
                            "surface_id": sid,
                            "session_id": session,
                            "add_dirs": d,
                            "summary": summary(remove, &path, &v),
                        })
                    })
                    .map_err(AppError::from);
                if !reply(r) {
                    tracing::debug!(
                        "[agent_session] surface={sid}: 폴더 답을 받을 호출이 이미 없다"
                    );
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("[agent_session] 폴더 워커를 못 띄웠다: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_takes_an_absolute_path_only() {
        assert_eq!(parse_command("/add-dir /a/b"), Some(Ok("/a/b".into())));
        assert_eq!(
            parse_command("  /add-dir   /a b  "),
            Some(Ok("/a b".into()))
        );
        assert!(matches!(parse_command("/add-dir"), Some(Err(_))));
        assert!(matches!(parse_command("/add-dir rel/dir"), Some(Err(_))));
        assert_eq!(parse_command("/add-dirx /a"), None);
        assert_eq!(parse_command("add-dir /a"), None);
        assert_eq!(parse_command("hi /add-dir /a"), None);
    }

    #[test]
    fn a_home_path_is_expanded() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let got = parse_command("/add-dir ~/x")
            .and_then(Result::ok)
            .unwrap_or_default();
        assert_eq!(
            got,
            std::path::Path::new(&home).join("x").display().to_string()
        );
    }

    #[test]
    fn the_summary_says_whether_it_applies_now() {
        let now = json!({"add_dirs": {"dirs": ["/a"], "changed": true, "applied": true}});
        let later = json!({"add_dirs": {"dirs": ["/a"], "changed": true, "applied": false}});
        let same = json!({"add_dirs": {"dirs": ["/a"], "changed": false, "applied": true}});
        let a = summary(false, "/a", &now);
        let b = summary(false, "/a", &later);
        let c = summary(false, "/a", &same);
        assert!(
            a != b && b != c && a != c,
            "세 경우가 같은 문구다: {a} | {b} | {c}"
        );
        assert!(a.contains("/a"));
        let none = summary(
            true,
            "/a",
            &json!({"add_dirs": {"dirs": [], "changed": true, "applied": false}}),
        );
        assert!(none.contains(&i18n::t("sup.dir.none")));
    }
}
