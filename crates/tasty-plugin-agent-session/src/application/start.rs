//! 대화 탭에서 세션을 **띄우고 잇는** 흐름 — 시작 폼 · 붙기 · 첫 안내 턴.
//!
//! plugin 상태(대화 · 활동)를 함께 만지므로 plugin 의 메서드로 둔다. `main.rs` 가 파일 크기 게이트에
//! 닿아 이 덩어리를 옮겼다.
use serde_json::{Value, json};

use crate::application::plugin::Started;
use crate::application::request::require_surface;
use crate::application::turn_done::BRIEF_DONE;
use crate::ports::host::SendHost;
use crate::ports::supervisor::Supervisor;

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 세션을 새로 띄우고 **이 탭에 잇는다**.
    ///
    /// 못 이으면 띄운 것을 되돌린다 — 이어지지 않은 세션은 아무도 못 찾고
    /// 자원만 문다(`supervisor start` 와 같은 규율).
    /// ⚠️ **데몬을 인자로 받는다.** 안에서 `LocalSupervisor::new()` 를 부르면
    /// 시험이 **진짜 데몬에 세션을 띄운다**(실제로 그랬다 — 가짜를 넣었는데도
    /// 실물 id 가 돌아왔다).
    pub(crate) fn start_session_for(
        &self,
        host: &impl crate::ports::host::HostCall,
        sup: &impl Supervisor,
        surface_id: u32,
        agent: &str,
        params: &Value,
    ) -> Result<Started, String> {
        let field = |k: &str| -> Option<String> {
            params
                .pointer(&format!("/params/{k}"))
                .or_else(|| params.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|v| !v.trim().is_empty())
        };
        let (mut cwd, model, name) = (field("cwd"), field("model"), field("name"));
        let permission_mode = field("permission_mode");
        // **이어받을 좌표** — supervisor 가 모르는, agent 프로필이 쓰던 id.
        //  이것을 주면 데몬이 `--resume` 경로로 띄워 **그 대화가 이어진다**.
        // 안 주면 종전대로 새 대화다.
        let adopt = field("adopt");
        if let Some(input) = adopt.as_deref() {
            // 그 대화를 이미 가진 기록부터 찾는다 — 연결돼 있으면 거절, 끊겼으면 그 기록을
            // 다시 잇는다(`crate::application::session_resolve::adopt_target`).
            let (rows, _) = sup.list().map_err(|e| e.to_string())?;
            match crate::application::session_resolve::adopt_target(&rows, input)? {
                crate::application::session_resolve::Adopt::Reuse { id, cwd: was } => {
                    let cwd = was.or(cwd);
                    return self.tie_session(host, sup, surface_id, id, agent, cwd, params, false);
                }
                crate::application::session_resolve::Adopt::New => {
                    // 작업 폴더를 안 정했으면 그 대화가 시작된 폴더에서 잇는다 — 안 주면 데몬의
                    // 폴더에서 돈다.
                    if cwd.is_none() {
                        cwd = crate::application::session_resolve::adopted_cwd(sup, agent, input);
                    }
                }
            }
        }
        let opts = crate::ports::supervisor::SpawnOpts {
            agent,
            adopt: adopt.as_deref(),
            cwd: cwd.as_deref(),
            model: model.as_deref(),
            name: name.as_deref(),
            // 첫 메시지는 **여기서 안 싣는다** — `spawn` 의 `prompt` 는 답을
            // 기다리지 않아 그 출력이 다음 턴의 응답으로 밀린다(그쪽 실측).
            // 아래에서 `send_input` 으로 보내 답을 제자리에 받는다.
            prompt: None,
            permission_mode: permission_mode.as_deref(),
            // 이 탭이 물음에 답한다(`prompts`) — 데몬이 protocol 13 일 때만 실린다.
            permission_prompts: true,
            // 대화 탭 복제가 원본의 추가 작업 폴더를 싣는다(`copy_from`) — 데몬이 protocol 14 일 때만 실린다.
            add_dirs: &crate::application::copy_from::add_dirs_of(params),
            // 시작 폼의 역할 칸 · CLI `--roles` · 복제가 같은 칸(`roles`)으로 싣는다.
            roles: &crate::adapters::driven::store::roles::names_of(params),
        };
        let sess = match sup.spawn(&opts) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("agent_session surface {surface_id}: 세션을 못 띄웠다: {e}");
                //  사유를 **돌려준다** — 종전에는 로그에만 남아 화면이
                // 조용했고, 그 조용함이 spinner 로 나타났다.
                return Err(e.to_string());
            }
        };
        let Some(id) = sess.get("id").and_then(Value::as_str).map(str::to_string) else {
            return Err(crate::domain::i18n::t("sup.err.spawn_no_id"));
        };
        // 데몬이 이어받을 대화를 쥔 **끊긴 기존 기록을 되살려** 그 id 를 돌려줬으면(protocol 9 ·
        // `reused`, 참일 때만 실린다) 우리가 띄운 것이 아니다 — 못 이어도 멈추지 않는다.
        let reused = sess.get("reused").and_then(Value::as_bool).unwrap_or(false);
        let mut started =
            self.tie_session(host, sup, surface_id, id, agent, cwd, params, !reused)?;
        started.warnings = spawn_warnings(&sess);
        Ok(started)
    }

    /// 세션을 탭에 잇고 첫 맥락 턴을 마련한다. `spawned` 면 못 이었을 때 띄운 것을 되돌린다 —
    /// 다시 잇는 기존 기록은 우리가 띄운 것이 아니라 건드리지 않는다.
    #[allow(clippy::too_many_arguments)] // reason: start_session_for 의 두 갈래가 같은 값을 넘긴다 — 묶음 타입은 이 한 자리만 쓴다
    pub(crate) fn tie_session(
        &self,
        host: &impl crate::ports::host::HostCall,
        sup: &impl Supervisor,
        surface_id: u32,
        id: String,
        agent: &str,
        cwd: Option<String>,
        params: &Value,
        spawned: bool,
    ) -> Result<Started, String> {
        let prompt = params
            .pointer("/params/prompt")
            .or_else(|| params.get("prompt"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|v| !v.trim().is_empty());
        if let Err(e) = crate::application::surface_link::link(host, surface_id, &id) {
            tracing::warn!("agent_session surface {surface_id}: 못 이었다: {e}");
            if spawned {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "A rollback failure would hide the link error that is being returned."
                )]
                let _ = sup.stop(&id); // 되돌리기 실패는 삼킨다 — 원래 사유가 더 중요하다
            }
            return Err(e.to_string());
        }
        //  **자기가 어디에 있는지 알려 준다**(0.1.29).
        //
        // 데몬의 `session.spawn` 에는 `env` 가 **없다**(실측 — 그쪽
        // `Request::Spawn` 은 agent/name/model/prompt/cwd/adopt 뿐이다).
        // 쉘 surface 는 PTY 를 띄울 때 호스트가 `TASTY_SURFACE_ID` 를
        // 꽂아 주지만(`tasty-terminal`), 이 세션은 **데몬이** 띄우므로
        // 그 경로가 없다. 그래서 환경이 아니라 **첫 턴 문면**으로 준다.
        //
        // ⚠️ `spawn` 의 `prompt` 에 싣지 않는다 — 그쪽은 답을 안 기다려
        // 출력이 다음 턴의 응답으로 밀린다(위 주석과 같은 축).
        //
        // ⚠️ **여기서 보내지 않는다.** `send_input` 은 턴이 끝나야 돌아와, plugin 루프에서
        // 부르면 그동안 **다른 모든 탭이 멈추고** 첫 메시지가 길면 호스트가 plugin 을
        // 응답 없음으로 끊는다. 보내는 것은 호출부가 대화를 만든 뒤 워커로 한다
        // (`begin_started`).
        let brief = session_brief(surface_id, &id, agent, cwd.as_deref());
        Ok(Started {
            id,
            brief,
            prompt,
            warnings: Vec::new(),
        })
    }

    /// 띄운 세션의 첫 턴들을 **워커로** 보낸다 — 맥락 먼저, 첫 메시지는 그 뒤.
    ///
    /// 첫 메시지는 대기열에 넣고 맥락이 도는 동안 세션을 바쁨으로 둔다(`Convo::briefing`).
    /// 맥락이 끝나면(`agent_session.brief_done`) 대기열이 그것을 이 탭의 턴으로 보내, 답이
    /// 화면의 제자리에 붙는다. 맥락의 답은 화면에 안 낸다 — 안내가 답을 요구하지 않는다.
    pub(crate) fn begin_started(
        &mut self,
        host: &impl SendHost,
        sid: u32,
        brief: String,
        prompt: Option<String>,
    ) {
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let Some(session) = c.session_id.clone() else {
            return;
        };
        if let Some(p) = prompt {
            crate::domain::send_queue::push(c, &p);
        }
        c.briefing = true;
        // 맥락 턴도 이 탭이 연 턴이다 — 그 답이 "에이전트가 이어서" 로 붙지 않게(`own_turns`).
        c.own_turns.opened();
        let worker_host = host.clone();
        let sup = self.sup;
        let spawned = std::thread::Builder::new()
            .name(format!("agent-session-brief-{sid}"))
            .spawn(move || {
                if let Err(e) = sup.send_input(&session, &brief) {
                    // 실패해도 세션은 산다 — 맥락이 없을 뿐이다.
                    tracing::warn!("agent_session surface {sid}: 맥락 전달 실패: {e}");
                }
                // 되던지기가 실패하면 plugin 이 내려가는 중이다 — 조용히 끝낸다.
                #[expect(clippy::let_underscore_must_use, reason = "A failed self-invoke means the plugin is shutting down; nobody is left to tell.")]
                let _ = worker_host.self_invoke(BRIEF_DONE, json!({ "surface_id": sid }));
            })
            .is_ok();
        if !spawned {
            // 맥락 없이 간다 — 바쁨으로 남기면 첫 메시지가 영영 안 나간다.
            tracing::warn!("agent_session surface {sid}: 맥락 워커를 못 띄웠다");
            c.briefing = false;
            c.own_turns.closed(std::time::Instant::now());
        }
        if let Some(c) = self.convos.get(&sid) {
            crate::application::session_surface::push(host, &self.sup, sid, c, false);
        }
        self.flush_queues();
    }

    /// 시작 폼이 온 경우 — 세션을 띄워 이 탭에 잇고 대화창으로 바꾼다.
    pub(crate) fn start_from_form(
        &mut self,
        host: &impl SendHost,
        sid: u32,
        form: &crate::adapters::driving::url::StartForm,
    ) {
        let h = host;
        let sup = self.sup;
        // `start_session_for` 가 `cwd`·`model`·`name` 을 여기서 읽는다 —
        // 탭 만들기 경로와 **같은 함수**를 쓴다(같은 값을 두 길로 넣으면
        // 한쪽만 고쳐질 자리가 생긴다).
        let params = serde_json::json!({ "params": {
            "cwd": form.cwd,
            "model": form.model,
            "name": form.name,
            "adopt": form.adopt,
            "permission_mode": form.permission_mode,
            "roles": form.roles,
        }});
        match self.start_session_for(&h, &sup, sid, &form.agent, &params) {
            Ok(s) => {
                let convo = crate::domain::convo::Convo {
                    session_id: Some(s.id),
                    start_warning: (!s.warnings.is_empty()).then(|| s.warnings.join("\n")),
                    ..Default::default()
                };
                self.convos.insert(sid, convo);
                self.begin_started(host, sid, s.brief, s.prompt);
            }
            Err(why) => {
                // 못 띄웠으면 **폼을 다시 그린다** — 빈 화면으로 두면 사용자가
                // 무엇이 잘못됐는지 모른 채 기다린다.
                //
                //  **사유를 실어야 화면이 바뀐다.** 안 실으면 HTML 이
                // 직전과 바이트까지 같아 호스트가 재적재를 건너뛰고
                // (`resync_webview_urls` 의 URL 비교), 스크립트가 씌운
                // spinner 가 **영영 남는다**(실사용 보고 2026-09-30).
                let mut convo = self.convos.remove(&sid).unwrap_or_default();
                convo.start_error = Some(why);
                crate::application::session_surface::push(&h, &self.sup, sid, &convo, false);
                self.convos.insert(sid, convo);
            }
        }
    }

    /// 잇기 · 시작 IPC 가 성공했으면(`linked`) 시작 폼에 머문 그 탭을 대화창으로 다시 연다.
    ///
    /// CLI 로 세션을 이은 탭이 아직 시작 폼이면 대화창으로 다시 연다 — 잇기는 surface meta 만 써서
    /// 화면은 폼에 남았고, 그 탭으로 보낸 말은 세션 없는 대기열에 갇혔다(실측 2026-10-21).
    pub(crate) fn reopen_linked(&mut self, linked: bool, params: &Value) {
        if let (true, Ok(sid)) = (linked, require_surface(params))
            && self
                .convos
                .get(&sid)
                .is_some_and(|c| c.session_id.is_none())
        {
            self.open_session_surface(sid, &json!({}), false);
        }
    }

    /// 기존 세션을 이 탭에 잇고 대화창으로 바꾼다.
    ///
    /// **띄우지 않는다** — 이미 있는 것에 붙는 것이라 자원이 새로 들지 않는다.
    pub(crate) fn attach_from_form(&mut self, host: &impl SendHost, sid: u32, session_id: &str) {
        let h = host;
        if let Err(e) = crate::application::surface_link::link(&h, sid, session_id) {
            tracing::warn!("agent_session surface {sid}: 못 이었다: {e}");
            // 폼을 다시 그린다 — 빈 화면으로 두면 무엇이 잘못됐는지 모른다.
            self.open_session_surface(sid, &serde_json::json!({}), false);
            return;
        }
        let convo = crate::domain::convo::Convo {
            session_id: Some(session_id.to_string()),
            ..Default::default()
        };
        crate::application::session_surface::push(&h, &self.sup, sid, &convo, false);
        self.convos.insert(sid, convo);
    }
}

/// 데몬이 띄우며 남긴 경고의 문면 — `warnings[].detail`(protocol 15, 없으면 칸이 생략된다).
///
/// 바깥 claude 가 같은 대화를 쥔 채 이어받은 경우가 그것이다: 데몬은 거절하지 않고 진행하되 한 대화
/// 기록에 두 프로세스가 쓴다는 사실을 알린다. 모르는 `kind` 여도 `detail` 을 그대로 보인다(상대 계약 §7-D).
pub(crate) fn spawn_warnings(sess: &Value) -> Vec<String> {
    sess.get("warnings")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|w| {
                    w.get("detail")
                        .and_then(Value::as_str)
                        .or_else(|| w.get("kind").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 세션이 **자기 자리**를 알게 하는 첫 턴 문면.
///
/// 쉘 surface 는 PTY env 로 받는다(`TASTY_SURFACE_ID` 등 — `tasty-terminal`
/// 이 꽂는다). 이 세션은 **데몬이** 띄우므로 그 경로가 없고, 데몬의
/// `session.spawn` 에는 `env` 가 아예 없다(실측). 그래서 같은 사실을
/// **문면으로** 준다.
///
/// ⚠️ **없는 것을 적지 않는다.** 바이너리 경로·포트처럼 이 플러그인이
/// 확신할 수 없는 값은 빼고, 에이전트가 `tasty` 로 직접 물어보게 한다 —
/// 틀린 경로를 적어 주면 그것을 믿고 쓰다 실패한다.
pub(crate) fn session_brief(
    surface_id: u32,
    session_id: &str,
    agent: &str,
    cwd: Option<&str>,
) -> String {
    let t = crate::domain::i18n::t;
    let t1 = crate::domain::i18n::t1;
    let mut lines = vec![
        t("sup.brief.intro"),
        t1("sup.brief.surface", &surface_id.to_string()),
        t1("sup.brief.session", session_id),
        t1("sup.brief.agent", agent),
    ];
    if let Some(c) = cwd {
        lines.push(t1("sup.brief.cwd", c));
    }
    format!("{}\n\n{}\n", lines.join("\n"), t("sup.brief.outro"))
}

/// 이 말이 탭이 숨겨 보낸 안내([`session_brief`])인가 — 대화 기록을 되읽을 때 거른다.
///
/// 로케일이 바뀌어도 알아보도록 문면이 아니라 세 언어에 공통인 표지를 본다: 첫 줄이 `[tasty] ` 로
/// 시작하고 `agent-session` 을 담는다(`sup.brief.intro`). 안 거르면 숨긴 턴이 이전 대화로 보인다
/// (실측 2026-10-21: 복원 · 이어받기 뒤 첫 턴에 안내문과 그 답이 떴다).
pub(crate) fn is_brief(sent: &str) -> bool {
    let first = sent.lines().next().unwrap_or_default();
    first.starts_with("[tasty] ") && first.contains("agent-session")
}

#[cfg(test)]
mod warning_tests {
    use super::spawn_warnings;
    use serde_json::json;

    #[test]
    fn the_detail_of_each_warning_is_kept() {
        let sess = json!({"id": "s", "warnings": [
            {"kind": "coordinate_held_outside", "pid": 47250, "detail": "두 프로세스가 쓴다"},
            {"kind": "something_new"}
        ]});
        assert_eq!(
            spawn_warnings(&sess),
            vec!["두 프로세스가 쓴다", "something_new"]
        );
        assert!(spawn_warnings(&json!({"id": "s"})).is_empty());
    }
}

#[cfg(test)]
mod brief_marker_tests {
    use super::*;

    /// 세 로케일의 실제 안내문이 모두 표지에 걸리고, 보통 말은 안 걸린다.
    #[test]
    fn every_locale_brief_is_recognized() {
        for body in [
            include_str!("../../lang/en.toml"),
            include_str!("../../lang/ko.toml"),
            include_str!("../../lang/ja.toml"),
        ] {
            let sec = body
                .split("[sup.brief]")
                .nth(1)
                .expect("[sup.brief] 절이 없다");
            let intro = sec
                .lines()
                .find_map(|l| l.strip_prefix("intro = \""))
                .and_then(|l| l.strip_suffix('"'))
                .expect("intro 가 없다");
            assert!(
                is_brief(&format!("{intro}\n- x")),
                "안내문을 못 알아본다: {intro}"
            );
        }
        assert!(is_brief(&session_brief(7, "s", "cc", None)));
        assert!(!is_brief("[tasty] 다른 이야기"));
        assert!(!is_brief("agent-session 탭에 대해 묻는다"));
    }
}
