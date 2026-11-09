//! 대화 탭의 **"에이전트가 일하는 중"** 표시.
//!
//! # 왜 따로 있나 — 탭이 보낸 턴만으로는 모자란다
//!
//! 대화 탭의 진행 표시(`.turn.pending`)는 **이 탭이 보낸 말의 답을 기다리는
//! 동안**에만 돈다. 그런데 에이전트는 그 밖에서도 일한다: 백그라운드 작업을
//! 걸어 두고 답을 끝낸 뒤, 작업 완료 알림을 받아 **스스로 다음 턴을 돈다**
//! (실측 2026-10-12 — claude 가 "완료 알림이 오면 이어서 하겠다" 고 답한 뒤
//! 배포 검증·문서 갱신 턴을 혼자 돌았고, 탭에는 아무 표시도 없었다).
//! CLI(`supervisor tell`)나 다른 소비자가 보낸 턴도 같다.
//!
//! 그 사실을 아는 곳은 데몬뿐이다 — `protocol` 6 부터 `session.list` 의 행마다
//! `turn_active` 가 `status` 와 같은 값으로 실린다(실측 2026-10-13, `dd9e139`: 7 초 턴
//! 동안 0.3 초 간격 21 번 모두 `true`, 끝나자 `false`. 판 5 는 행에 늘 `false` 를
//! 실었다). 그래서 **데몬에 주기적으로 묻는다**.
//!
//! ⚠️ 판 6 에서 데몬이 `true` 를 내는 것은 **누군가 `send_input` 으로 보낸 턴**뿐이다.
//! agent 가 스스로 이어 가는 턴 · `broadcast` 로 보낸 턴 · 새 세션의 첫 프롬프트는
//! `false` 다. 판 7 부터는 데몬이 세션의 출력을 늘 읽어 그 턴들도 `true` 다(실측
//! 2026-10-13, `24b4e3c`: 백그라운드 작업 뒤 스스로 이어 간 0.6 초 턴을 0.8 초 간격 폴링이
//! 한 번 `true` 로 잡았다). 이 자리는 판을 가리지 않는다 — 고칠 것이 없었다.
//! 구독(`session.subscribe`)은 세션마다 연결 하나를 붙잡아 plugin 루프를
//! 막으므로(`client::session_watch` 주석) 이 자리에 못 쓴다.
//!
//! # 비용
//!
//! 묻는 것은 **워커**다 — 왕복(~0.10초)이 plugin 루프를 막지 않는다. 묻는
//! 호출은 이어진 세션 수와 무관하게 한 번이다(`list` 한 번에 전수가 온다).
//! 이어진 대화 탭이 없으면 데몬을 아예 안 부른다([`Activity::watching`]).
//!
//! 화면은 **문서를 다시 싣지 않고** 메시지(`webview.post_message`)로만 고친다 — 재적재는 깜빡이고
//! 입력 중이던 글을 지운다(`session_surface::patch` 주석).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use crate::application::session_surface;
use crate::domain::convo::Convos;
use crate::ports::host::HostCall;
use crate::ports::host::SendHost;
use crate::ports::supervisor::Supervisor;

/// 데몬에 묻는 주기. 사람이 "멈췄나?" 를 느끼기 전에 표시가 바뀔 만큼 촘촘하되,
/// 왕복 비용(~0.10초)이 묻힐 만큼은 길게.
const POLL_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// 워커가 되던지는 메서드 — plugin 만 부른다(`self_invoke`).
pub const METHOD: &str = "agent_session.activity";

/// 탭별 표시 상태와 워커에게 줄 "볼 것이 있다" 신호.
#[derive(Default)]
pub struct Activity {
    /// 표시가 **켜져 있는** 대화 탭.
    shown: BTreeSet<u32>,
    /// 상태줄이 **바쁨으로 그려져 있는** 대화 탭.
    busy_shown: BTreeSet<u32>,
    /// 위 두 표시를 마지막으로 그린 문서의 세대(`session_surface::doc_generation`) — 다르면
    /// 문서가 다시 실려 표시가 처음 모양(진행 표시 숨김)으로 돌아간 것이다.
    drawn_on: BTreeMap<u32, u64>,
    /// 호스트에 마지막으로 알린 탭별 실행 상태(`surface.set_busy`) — 탭 점 · 사이드바
    /// 워크스페이스 점의 근거다. 바뀔 때만 보낸다. plugin 이 다시 뜨면 비어 있으므로
    /// 첫 주기에 모든 탭을 한 번씩 알린다.
    host_busy: BTreeMap<u32, bool>,
    /// 호스트에 `surface.set_busy` 가 없다 — 한 번 알았으면 더 부르지 않는다.
    host_lacks_busy: bool,
    /// 마지막으로 데몬이 "턴을 돈다" 고 한 세션. 못 물었으면 비운다.
    active: BTreeSet<String>,
    /// 행이 `busy` 라고 한 세션 — 턴 · 기다리는 물음 · 턴 밖 작업(백그라운드 Bash · Monitor ·
    /// 하위 에이전트) 중 하나라도 있다. `active` 를 포함한다.
    working: BTreeSet<String>,
    /// 이어진 대화 탭이 하나라도 있는가 — 없으면 워커가 데몬을 안 부른다.
    watching: Arc<AtomicBool>,
}

impl Activity {
    /// 주기 워커를 띄운다. 실패해도 plugin 은 돈다 — 표시만 안 나온다.
    pub fn spawn_poller<S: Supervisor + Send + 'static>(&self, host: impl SendHost, sup: S) {
        let watching = Arc::clone(&self.watching);
        if std::thread::Builder::new()
            .name("agent-session-activity".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(POLL_EVERY);
                    // 볼 탭이 없으면 데몬을 안 부르고 **플래그만 갱신하러** 간다.
                    // 탭이 새로 열린 것은 plugin 쪽만 안다.
                    let params = if watching.load(Ordering::Relaxed) {
                        poll(&sup)
                    } else {
                        json!({})
                    };
                    if host.self_invoke(METHOD, params).is_err() {
                        // 되던질 수 없으면 plugin 이 내려가는 중이다 — 끝낸다.
                        break;
                    }
                }
            })
            .is_err()
        {
            tracing::warn!("활동 표시 워커를 못 띄웠다 — 에이전트 진행 표시가 안 나온다");
        }
    }

    /// 워커가 가져온 값을 탭에 반영한다.
    ///
    /// 표시는 **바뀔 때만** 그린다. 문서가 통째로 다시 실리면(`push`) 표시가 처음
    /// 모양으로 돌아가므로, 문서 세대가 바뀐 탭은 바뀐 것으로 보고 다시 그린다. 한때
    /// 켜야 하는 탭을 매 주기 다시 켰는데, 그 메시지가 바쁜 탭 수 × 주기마다 쌓였다
    /// (실측 2026-10-26: 10분에 569 회).
    pub fn apply<H: HostCall>(&mut self, host: &H, convos: &Convos, params: &Value) {
        self.watching.store(
            convos.values().any(|c| c.session_id.is_some()),
            Ordering::Relaxed,
        );
        let field = |k: &str| {
            params
                .pointer(&format!("/params/{k}"))
                .or_else(|| params.get(k))
        };
        // 이번 주기에 데몬을 안 물었다(볼 탭이 없었다) — 바꿀 근거가 없다. 호스트에
        // 알린 값만 맞춘다 — 세션이 풀린 탭이 켜진 채 남지 않게.
        let Some(ok) = field("ok").and_then(Value::as_bool) else {
            self.sync_host_busy(host, convos);
            return;
        };
        // 데몬을 못 물었으면(`ok=false`) **일한다고 말하지 않는다** — 모르는
        // 것을 "진행 중" 으로 단정하면 영영 도는 표시가 남는다.
        let ids = |k: &str| -> BTreeSet<String> {
            if !ok {
                return BTreeSet::new();
            }
            field(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        self.active = ids("active");
        self.working = ids("working").union(&self.active).cloned().collect();
        // 닫힌 탭은 잊는다 — 안 지우면 다음에 같은 id 가 오면 끄기를 건너뛴다.
        self.shown.retain(|sid| convos.contains_key(sid));
        self.busy_shown.retain(|sid| convos.contains_key(sid));
        self.drawn_on.retain(|sid, _| convos.contains_key(sid));
        for (&sid, convo) in convos {
            let generation = session_surface::doc_generation(sid);
            let fresh = self.drawn_on.insert(sid, generation) != Some(generation);
            // 새 문서의 진행 표시는 숨김으로 시작한다.
            let was = self.shown.contains(&sid) && !fresh;
            let want = wants_indicator(convo, &self.working);
            if want != was {
                session_surface::patch(host, sid, &show_msg(want));
            }
            if want {
                self.shown.insert(sid);
            } else {
                self.shown.remove(&sid);
            }
            // 상태줄 — 새 문서는 실을 때의 값으로 그려져 있어 한 번 맞춘다.
            let busy = self.is_busy(convo);
            if fresh || busy != self.busy_shown.contains(&sid) {
                session_surface::patch(host, sid, &status_msg(convo, busy));
            }
            if busy {
                self.busy_shown.insert(sid);
            } else {
                self.busy_shown.remove(&sid);
            }
        }
        self.sync_host_busy(host, convos);
    }

    /// 탭별 실행 상태를 호스트에 알린다 — 값이 바뀐 탭에만, 닫힌 탭은 잊는다.
    ///
    /// 대화 탭에는 PTY 가 없어 호스트가 스스로 판정하지 못한다(`docs/design/policies/busy-indicator.md`
    /// § (P) plugin surface). 알리지 않으면
    /// 턴을 도는 동안에도 탭 점과 사이드바 워크스페이스 점이 꺼져 있다. 실패한 탭은
    /// 기록하지 않아 다음 주기에 다시 보낸다.
    fn sync_host_busy<H: HostCall>(&mut self, host: &H, convos: &Convos) {
        if self.host_lacks_busy {
            return;
        }
        self.host_busy.retain(|sid, _| convos.contains_key(sid));
        for (&sid, convo) in convos {
            let busy = self.is_busy(convo);
            if self.host_busy.get(&sid) == Some(&busy) {
                continue;
            }
            match host.call(
                "surface.set_busy",
                json!({ "surface_id": sid, "busy": busy }),
            ) {
                Ok(_) => {
                    self.host_busy.insert(sid, busy);
                }
                Err(e) if crate::ports::host::host_lacks_method(&e) => {
                    tracing::info!("호스트에 surface.set_busy 가 없다 — 탭 실행 표시 없이 돈다");
                    self.host_lacks_busy = true;
                    return;
                }
                Err(e) => tracing::warn!("agent_session surface {sid}: 실행 상태를 못 알렸다: {e}"),
            }
        }
    }

    /// 데몬이 이 탭 세션의 턴을 돈다고 했나(`turn_active`) — 탭이 기다리는 턴과 별개다.
    pub fn daemon_active(&self, convo: &crate::domain::convo::Convo) -> bool {
        convo
            .session_id
            .as_deref()
            .is_some_and(|id| self.active.contains(id))
    }

    /// 이 탭의 세션이 **일하는 중인가** — 넓은 판정. 턴(이 탭이 기다리는 턴 · 맥락 턴 · 비우기 ·
    /// 압축 · 남이 보낸 턴)에 더해 기다리는 물음과 턴 밖 작업(백그라운드 Bash · Monitor · 하위
    /// 에이전트)도 센다.
    ///
    /// "지금 자식을 내리면 잃는 것이 있는가" 와 같은 질문이라 탭 표시 · 상태줄 · 정지 전 질의 ·
    /// 유휴 현행화 · 회수 · 작업 폴더 변경이 이 값 하나를 쓴다. 입력을 보낼지만 정하는 자리는
    /// [`Self::turn_busy`] 를 쓴다 — 끝나지 않는 작업(개발 서버 등)이 대화를 막지 않게.
    pub fn is_busy(&self, convo: &crate::domain::convo::Convo) -> bool {
        self.turn_busy(convo)
            || convo
                .session_id
                .as_deref()
                .is_some_and(|id| self.working.contains(id))
    }

    /// 턴이 도는가 — 좁은 판정. 새 입력을 보내면 그 턴 뒤에 쌓여야 하는가만 본다.
    pub fn turn_busy(&self, convo: &crate::domain::convo::Convo) -> bool {
        convo.turns.iter().any(|t| t.pending)
            || convo.briefing
            || convo.clearing
            || convo.sync.compacting()
            || convo
                .session_id
                .as_deref()
                .is_some_and(|id| self.active.contains(id))
    }

    /// 상태줄을 지금 값으로 다시 그린다 — 턴 수가 바뀌는 자리(보냄·답·이어진 턴)가 부른다.
    pub fn refresh_status<H: HostCall>(
        &self,
        host: &H,
        sid: u32,
        convo: &crate::domain::convo::Convo,
    ) {
        session_surface::patch(host, sid, &status_msg(convo, self.is_busy(convo)));
    }
}

/// 상태줄을 **서버가 그린 그대로** 갈아 끼우는 스크립트.
///
/// 스크립트 쪽에서 칸을 하나씩 고치면 상태줄의 규칙(무엇을 어떤 색으로)이
/// 두 벌이 된다. `status_line` 하나가 문서 첫 적재와 이 갱신을 같이 그린다.
fn status_msg(convo: &crate::domain::convo::Convo, busy: bool) -> Value {
    serde_json::json!({ "type": "status.set", "status": crate::adapters::driving::view::status_line(convo, busy) })
}

/// 표시를 켤 것인가 — 세션이 턴을 돌고 있고, **이 탭이 기다리는 턴이 없을 때**.
///
/// 이 탭이 보낸 턴을 기다리는 중이면 그 턴 자리에 이미 진행 표시가 돈다.
/// 둘을 함께 켜면 같은 사실을 두 번 말한다.
fn wants_indicator(convo: &crate::domain::convo::Convo, working: &BTreeSet<String>) -> bool {
    convo
        .session_id
        .as_deref()
        .is_some_and(|id| working.contains(id))
        && !convo.turns.iter().any(|t| t.pending)
}

/// 데몬에 한 번 묻는다 — 턴을 돌고 있는 세션 id 와 물음의 성패.
///
/// `turn_active` 가 없는 행(그 필드를 안 내는 낡은 데몬)은 **안 도는 것으로**
/// 친다 — 위 `ok=false` 와 같은 이유다.
fn poll(sup: &impl Supervisor) -> Value {
    match sup.list() {
        // 행도 함께 싣는다 — 상태줄이 같은 주기의 목록으로 맞춰진다(`session_info`).
        Ok((rows, _)) => json!({
            "ok": true,
            "active": active_ids(&rows),
            "working": working_ids(&rows),
            "rows": crate::adapters::driven::session_info::rows_json(&rows),
        }),
        Err(_) => json!({ "ok": false }),
    }
}

fn active_ids(rows: &[crate::ports::supervisor::SessionRow]) -> Vec<&str> {
    rows.iter()
        .filter(|r| r.raw().get("turn_active").and_then(Value::as_bool) == Some(true))
        .map(|r| r.id.as_str())
        .collect()
}

/// 행이 `busy` 라고 한 세션 — 그 칸이 없는 행은 `turn_active` 만 본다.
fn working_ids(rows: &[crate::ports::supervisor::SessionRow]) -> Vec<&str> {
    rows.iter()
        .filter(|r| r.raw().get("busy").and_then(Value::as_bool) == Some(true))
        .map(|r| r.id.as_str())
        .collect()
}

/// 턴이 도는데 소식이 없다 — claude plugin 의 `claude-error-stalled` 와 같은 키.
pub const STALLED_EVENT: &str = "claude-error-stalled";

/// 이만큼 소식이 없으면 멈춘 것으로 본다 — claude plugin 이 오류 없이 판정할 때 쓰는 값(120초)과 같다.
/// 대화 탭에는 턴 도중의 오류 신호가 없어 그쪽의 짧은 기준(오류 뒤 30초)은 쓸 자리가 없다.
const STALL_QUIET_SECS: u64 = 120;

/// 같은 탭에 다시 알리기까지 — claude plugin 과 같은 300초. 턴을 넘어서도 지킨다.
const STALL_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

/// 이번 주기에 `claude-error-stalled` 를 낼 것인가 — 내면 그 사실을 탭에 적는다.
///
/// `quiet` 은 데몬 행의 `turn_quiet_secs` 다. 턴이 도는 동안만 실리고, 턴 시작과 마지막 이벤트 중
/// 늦은 쪽부터 지난 초다(데몬 consumer-contract 판 11). **판정이 아니라 길이다** — 긴 도구 실행도
/// 소식이 없을 수 있어, 이 hook 은 "멈췄을 수 있다" 이지 "멈췄다" 가 아니다. 칸이 없으면 턴이 안
/// 도는 것이므로 다음 턴을 위해 표시를 거둔다. 한 턴에 한 번, 같은 탭에는 300초에 한 번까지다.
pub(crate) fn stall_event(
    convo: &mut crate::domain::convo::Convo,
    quiet: Option<u64>,
    now: std::time::Instant,
) -> bool {
    let Some(q) = quiet else {
        convo.stall_fired = false;
        return false;
    };
    let claude = convo.agent_kind.as_deref().is_none_or(|k| k == "claude");
    let cooled = convo
        .last_stall
        .is_none_or(|at| now.duration_since(at) >= STALL_COOLDOWN);
    if !claude || convo.stall_fired || q < STALL_QUIET_SECS || !cooled {
        return false;
    }
    convo.stall_fired = true;
    convo.last_stall = Some(now);
    true
}

/// 표시 줄의 마크업 — 대화 로그와 상태줄 사이에 **꺼진 채로** 놓인다.
///
/// 로그(`#log`) 밖에 두는 이유: 로그 끝에 두면 새 턴이 그 **뒤에** 붙어
/// 표시가 대화 중간에 끼고, 긴 대화에서는 스크롤에 묻힌다.
/// 글리프는 대화 턴의 진행 표시(`.spin`)를 그대로 쓴다 — 같은 사실은 같은 모양.
pub fn row_html() -> String {
    format!(
        r#"<style>.act{{padding:0 16px 8px;color:#7d8694}}.act[hidden]{{display:none}}.sent.auto{{color:#5a6070;font-style:italic}}.sent.auto::before{{content:"\21BB  ";color:#5a6070}}</style>
<div id="act" class="act" hidden><span class="spin"></span><span class="think">{}…</span></div>"#,
        crate::adapters::driving::html::escape(&crate::domain::i18n::t("sup.chat.working"))
    )
}

/// 표시를 켜거나 끄는 스크립트. 요소가 없으면(시작 폼 등) 아무것도 안 한다.
pub(crate) fn show_msg(show: bool) -> Value {
    serde_json::json!({ "type": "activity.show", "show": show })
}

/// 탭별로 무엇을 보냈는지 기록하는 시험용 호스트.
impl Activity {
    /// 세션 목록(`activity` 폴러의 `rows`)에서 각 탭 세션의 에이전트 쪽 id 를 옮겨 적는다.
    /// 바뀐 탭만 상태줄을 다시 그린다 — 같은 값으로 매 주기 그리면 깜빡인다.
    pub fn note_agent_sessions<H: HostCall>(&self, host: &H, convos: &mut Convos, params: &Value) {
        let Some(rows) = params
            .pointer("/params/rows")
            .or_else(|| params.get("rows"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for (&sid, convo) in convos.iter_mut() {
            // 옛 판이 대화 id 로 덮은 잇기를 먼저 되돌린다 — 그래야 아래에서 행을 찾는다(B107).
            crate::application::hookless::repair_link(host, sid, convo, rows);
            let Some(session) = convo.session_id.as_deref() else {
                continue;
            };
            let Some(row) = rows
                .iter()
                .find(|r| r.get("id").and_then(Value::as_str) == Some(session))
            else {
                continue;
            };
            let agent = row
                .get("agent_session_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            // 종류는 그리지 않는다 — hook 판정에만 쓰므로 다시 그릴 근거가 아니다.
            convo.agent_kind = row.get("kind").and_then(Value::as_str).map(str::to_string);
            // 데몬이 회수한 세션 — 현행화를 열지 않는다(`sync_compact::SyncState::reclaimed`). 사용자 턴으로
            // 되살아나면 행이 `stopped` 를 벗어나 다시 열린다.
            convo.sync.reclaimed = row.get("reclaimed").and_then(Value::as_bool) == Some(true)
                && row.get("state").and_then(Value::as_str) == Some("stopped");
            convo.sync.row_context = row.get("context_tokens").and_then(Value::as_u64);
            // 세션이 기다리는 물음(protocol 13) — 새로 오면 카드 · attention · hook(`prompts`).
            crate::application::prompts::note(host, sid, convo, row);
            // hook 없는 신호 — 세션 종료 알림 · 대화 id 표지(`hookless`).
            crate::application::hookless::note_row(host, sid, convo, row);
            let quiet = row.get("turn_quiet_secs").and_then(Value::as_u64);
            if stall_event(convo, quiet, std::time::Instant::now())
                && let Err(e) =
                    crate::application::surface_link::fire_hook(host, sid, STALLED_EVENT)
            {
                tracing::warn!(
                    "[agent_session] surface={sid}: hook '{STALLED_EVENT}' 발사 실패: {e}"
                );
            }
            let name = crate::adapters::driven::session_info::name_of_row(row);
            let process = crate::adapters::driven::session_info::proc_of_row(row);
            // 경과는 시각이 흘러 바뀌므로 값이 아니라 **그린 모양**으로 견준다.
            let shown = crate::adapters::driving::statusline::proc_key(
                process.as_ref(),
                crate::adapters::driven::store::budget::now(),
            );
            let agent_name = row.get("agent").and_then(Value::as_str).map(str::to_string);
            let cwd = row.get("cwd").and_then(Value::as_str).map(str::to_string);
            if agent != convo.agent_session_id
                || name != convo.listed_name
                || shown != convo.process_shown
                || agent_name != convo.agent_name
                || cwd != convo.listed_cwd
            {
                convo.agent_session_id = agent;
                convo.listed_name = name;
                convo.agent_name = agent_name;
                convo.listed_cwd = cwd;
                convo.process = process;
                convo.process_shown = shown;
                self.refresh_status(host, sid, convo);
            }
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct RecordingHost(std::cell::RefCell<std::collections::HashMap<u32, Vec<String>>>);

#[cfg(test)]
impl HostCall for RecordingHost {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        if method == "webview.post_message" {
            let sid = params["surface_id"].as_u64().unwrap_or_default() as u32;
            let s = params["data"].to_string();
            self.0.borrow_mut().entry(sid).or_default().push(s);
        }
        Ok(Value::Null)
    }
}

/// 호스트가 이 plugin 을 내리기 전에 묻는 바쁨(`agent_session.lifecycle_busy`,
/// `docs/dev-guide/plugin-development.md` § CLI + IPC namespace).
///
/// 탭의 진행 표시와 **같은 판정**(`Activity::is_busy`)을 쓴다 — 화면이 "일하는 중" 이라고 보이는 탭이 곧
/// 내리면 답을 잃는 탭이다. 바쁜 탭의 surface id 를 싣는다.
pub fn busy_report(
    activity: &Activity,
    convos: &crate::domain::convo::Convos,
) -> serde_json::Value {
    let mut busy: Vec<u32> = convos
        .iter()
        .filter(|(_, c)| activity.is_busy(c))
        .map(|(sid, _)| *sid)
        .collect();
    busy.sort_unstable();
    let reason = (!busy.is_empty()).then(|| {
        crate::domain::i18n::t1(
            "sup.busy.turns_running",
            &busy
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )
    });
    serde_json::json!({ "busy": !busy.is_empty(), "surfaces": busy, "reason": reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convo::Convo;
    use crate::domain::convo::Turn;
    use std::collections::HashMap;

    fn convo(session: &str, pending: bool) -> Convo {
        Convo {
            session_id: Some(session.to_string()),
            turns: vec![Turn {
                sent: "hi".into(),
                got: String::new(),
                error: None,
                pending,
                at: None,
            }],
            ..Default::default()
        }
    }

    /// 진행 표시 줄에 보낸 스크립트만.
    fn sent(host: &RecordingHost, sid: u32) -> Vec<String> {
        all_sent(host, sid)
            .into_iter()
            .filter(|s| s.contains(r#""type":"activity.show""#))
            .collect()
    }

    /// 상태줄에 보낸 스크립트만.
    fn status_sent(host: &RecordingHost, sid: u32) -> Vec<String> {
        all_sent(host, sid)
            .into_iter()
            .filter(|s| s.contains(r#""type":"status.set""#))
            .collect()
    }

    fn all_sent(host: &RecordingHost, sid: u32) -> Vec<String> {
        host.0.borrow().get(&sid).cloned().unwrap_or_default()
    }

    /// 상태줄은 바쁜 동안 "응답 대기" 로 매 주기 다시 그려지고, 쉬면 **한 번** 되돌아간다.
    /// 호스트가 묻는 바쁨 — 도는 턴 · 비우기 중인 탭만 잡고 그 surface 를 싣는다.
    #[test]
    fn the_busy_report_names_the_tabs_with_work_in_flight() {
        let mut convos = crate::domain::convo::Convos::default();
        let mut idle = crate::domain::convo::Convo {
            session_id: Some("a".into()),
            ..Default::default()
        };
        convos.insert(3, std::mem::take(&mut idle));
        let r = busy_report(&Activity::default(), &convos);
        assert_eq!(r["busy"], false);
        assert!(r["reason"].is_null());
        let mut running = crate::domain::convo::Convo {
            session_id: Some("b".into()),
            ..Default::default()
        };
        crate::domain::convo::begin_turn(&mut running, "hi");
        convos.insert(9, running);
        convos.insert(
            5,
            crate::domain::convo::Convo {
                session_id: Some("c".into()),
                clearing: true,
                ..Default::default()
            },
        );
        let r = busy_report(&Activity::default(), &convos);
        assert_eq!(r["busy"], true);
        assert_eq!(r["surfaces"], serde_json::json!([5, 9]));
        assert!(r["reason"].as_str().unwrap_or_default().contains("5, 9"));
    }

    /// `surface.set_busy` 호출만 모으는 호스트 — `fail` 이 켜져 있으면 거절한다.
    #[derive(Default)]
    struct BusyHost {
        calls: std::cell::RefCell<Vec<(u64, bool)>>,
        fail: std::cell::Cell<bool>,
        /// 메서드가 없는 호스트처럼 거절한다.
        missing: std::cell::Cell<bool>,
    }

    impl HostCall for BusyHost {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            if method != "surface.set_busy" {
                return Ok(Value::Null);
            }
            if self.missing.get() {
                self.calls.borrow_mut().push((0, false));
                return Err("permission_denied: unknown ipc method: surface.set_busy".into());
            }
            if self.fail.get() {
                return Err("down".into());
            }
            self.calls.borrow_mut().push((
                params["surface_id"].as_u64().unwrap(),
                params["busy"].as_bool().unwrap(),
            ));
            Ok(Value::Null)
        }
    }

    /// 호스트에 `surface.set_busy` 가 없으면 한 번 거절당한 뒤 다시 부르지 않는다 — 주기마다
    /// 부르면 거절과 경고가 끝없이 쌓인다.
    #[test]
    fn a_host_without_set_busy_is_asked_once() {
        let host = BusyHost::default();
        host.missing.set(true);
        let mut act = Activity::default();
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        convos.insert(7008, convo("s2", false));
        let busy = json!({ "ok": true, "active": ["s1", "s2"] });
        let idle = json!({ "ok": true, "active": [] });
        for _ in 0..3 {
            act.apply(&host, &convos, &busy);
            act.apply(&host, &convos, &idle);
        }
        assert_eq!(host.calls.borrow().len(), 1, "{:?}", host.calls.borrow());
    }

    /// 탭 점 · 사이드바 점의 근거는 바뀔 때만 호스트에 간다. 실패하면 다음 주기에 다시
    /// 보내고, 데몬을 안 물은 주기에도 꺼지며, 닫힌 탭은 잊어 같은 id 가 다시 오면 또 알린다.
    #[test]
    fn the_host_hears_the_busy_state_only_when_it_flips() {
        let host = BusyHost::default();
        let mut act = Activity::default();
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let busy = json!({ "ok": true, "active": ["s1"] });
        let idle = json!({ "ok": true, "active": [] });

        act.apply(&host, &convos, &busy);
        act.apply(&host, &convos, &busy);
        assert_eq!(
            *host.calls.borrow(),
            vec![(7007, true)],
            "같은 값은 다시 안 보낸다"
        );

        host.fail.set(true);
        act.apply(&host, &convos, &idle);
        host.fail.set(false);
        act.apply(&host, &convos, &idle);
        assert_eq!(
            *host.calls.borrow(),
            vec![(7007, true), (7007, false)],
            "거절된 값은 다음 주기에 다시 보낸다"
        );

        // 세션이 풀려 데몬을 안 물은 주기 — 탭이 기다리던 턴만으로 바빴다가 끝난다.
        let mut local = Convo::default();
        crate::domain::convo::begin_turn(&mut local, "hi");
        convos.insert(7007, local);
        act.apply(&host, &convos, &json!({}));
        convos.insert(7007, Convo::default());
        act.apply(&host, &convos, &json!({}));
        assert_eq!(host.calls.borrow()[2..], [(7007, true), (7007, false)]);

        convos.clear();
        act.apply(&host, &convos, &idle);
        convos.insert(7007, convo("s1", false));
        act.apply(&host, &convos, &idle);
        assert_eq!(
            host.calls.borrow().last(),
            Some(&(7007, false)),
            "닫혔다 다시 온 탭은 처음처럼 알린다"
        );
        assert_eq!(host.calls.borrow().len(), 5);
    }

    /// 이 시험만 surface 7171 을 쓴다 — 문서를 다시 실어(`push`) 전역 세대를 올리므로, 같은 id 를
    /// 쓰는 시험이 병렬로 돌면 그쪽이 "다시 실렸다" 로 읽는다.
    #[test]
    fn the_status_line_follows_the_daemon_and_settles_once() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7171, convo("s1", false));
        let host = RecordingHost::default();
        let mut act = Activity::default();
        let waiting = crate::domain::i18n::t("sup.status.waiting");

        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        let busy = status_sent(&host, 7171);
        assert_eq!(busy.len(), 1, "바쁜 동안 같은 상태줄을 다시 그렸다");
        assert!(busy[0].contains(&waiting), "바쁜데 대기로 안 그렸다");

        act.apply(&host, &convos, &json!({ "ok": true, "active": [] }));
        act.apply(&host, &convos, &json!({ "ok": true, "active": [] }));
        let all = status_sent(&host, 7171);
        assert_eq!(all.len(), 2, "쉬게 될 때 한 번만 그린다");
        assert!(!all[1].contains(&waiting), "쉬는데 여전히 대기로 그렸다");

        // 문서가 다시 실리면(바쁜 동안) 표시가 처음 모양이 되므로 다음 주기에 다시 그린다.
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        crate::application::session_surface::push(
            &host,
            &crate::adapters::driven::local::LocalSupervisor::new(),
            7171,
            &convos[&7171],
            false,
        );
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        assert_eq!(
            status_sent(&host, 7171).len(),
            4,
            "다시 실린 문서를 안 고쳤다"
        );
        assert_eq!(
            sent(&host, 7171)
                .iter()
                .filter(|s| s.contains(r#""show":true"#))
                .count(),
            3,
            "켜기는 바빠질 때 둘 · 다시 실린 문서에 하나"
        );
    }

    /// 상태줄의 턴 수는 지금 대화의 길이다 — 첫 적재 때 값에 머물지 않는다.
    #[test]
    fn a_status_refresh_carries_the_current_turn_count() {
        let mut c = convo("s1", false);
        c.turns.push(c.turns[0].clone());
        let host = RecordingHost::default();
        Activity::default().refresh_status(&host, 7007, &c);
        let s = &status_sent(&host, 7007)[0];
        let m: Value = serde_json::from_str(s).expect("메시지");
        assert_eq!(m["type"], "status.set");
        let html = crate::adapters::driving::statusline::html_of(&m["status"]);
        assert!(
            html.contains("<b>2</b>"),
            "턴 수가 2 로 안 그려졌다: {html}"
        );
    }

    /// 데몬이 턴을 돈다고 하면 켜고, 멈추면 **한 번** 끈다 — 쉬는 동안은 조용하다.
    #[test]
    fn shows_while_the_daemon_reports_a_turn_and_hides_once_after() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = RecordingHost::default();
        let mut act = Activity::default();

        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        assert_eq!(sent(&host, 7007), vec![show_msg(true).to_string()]);
        assert!(
            act.watching.load(Ordering::Relaxed),
            "이어진 탭이 있는데 워커가 쉰다"
        );

        act.apply(&host, &convos, &json!({ "ok": true, "active": [] }));
        act.apply(&host, &convos, &json!({ "ok": true, "active": [] }));
        assert_eq!(
            sent(&host, 7007),
            vec![show_msg(true).to_string(), show_msg(false).to_string()]
        );
    }

    /// 이 탭이 기다리는 턴이 있으면 켜지 않는다 — 그 턴 자리에 이미 돈다.
    #[test]
    fn stays_off_while_this_tab_awaits_its_own_turn() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", true));
        let host = RecordingHost::default();
        let mut act = Activity::default();
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        assert!(sent(&host, 7007).is_empty());
    }

    /// 데몬을 못 물었으면 켜져 있던 표시를 끈다 — "진행 중" 을 단정하지 않는다.
    #[test]
    fn an_unreachable_daemon_turns_the_indicator_off() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = RecordingHost::default();
        let mut act = Activity::default();
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        act.apply(&host, &convos, &json!({ "ok": false }));
        assert_eq!(
            sent(&host, 7007),
            vec![show_msg(true).to_string(), show_msg(false).to_string()]
        );
    }

    /// 데몬을 안 물은 주기(볼 탭이 없었다)는 아무것도 안 바꾼다.
    #[test]
    fn a_skipped_poll_changes_nothing() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = RecordingHost::default();
        let mut act = Activity::default();
        act.apply(&host, &convos, &json!({ "ok": true, "active": ["s1"] }));
        act.apply(&host, &convos, &json!({}));
        assert_eq!(sent(&host, 7007), vec![show_msg(true).to_string()]);
    }

    /// 이어진 세션이 없으면 워커가 데몬을 안 부르게 한다.
    #[test]
    fn no_linked_tab_stops_the_daemon_calls() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, Convo::default());
        let host = RecordingHost::default();
        let mut act = Activity::default();
        act.watching.store(true, Ordering::Relaxed);
        act.apply(&host, &convos, &json!({}));
        assert!(!act.watching.load(Ordering::Relaxed));
    }

    /// 목록 행의 이름 · 프로세스 칸이 상태줄에 실린다 — 칸이 빠지면 지운다(옛 pid 를 안 남긴다).
    #[test]
    fn row_name_and_process_reach_the_status_line() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = RecordingHost::default();
        let act = Activity::default();
        let live =
            json!({"rows":[{"id":"s1","name":"named","pid":4242,"rss_kb":2048,"started_unix":0}]});
        act.note_agent_sessions(&host, &mut convos, &live);
        let s = status_sent(&host, 7007).pop().expect("상태줄을 안 그렸다");
        let m: Value = serde_json::from_str(&s).expect("메시지");
        let s = crate::adapters::driving::statusline::html_of(&m["status"]);
        assert!(
            s.contains(">(named)<") && s.contains(">4242<") && s.contains(">2MB<"),
            "{s}"
        );
        act.note_agent_sessions(&host, &mut convos, &json!({"rows":[{"id":"s1"}]}));
        let s = status_sent(&host, 7007)
            .pop()
            .expect("칸이 빠졌는데 안 다시 그렸다");
        assert!(!s.contains("4242") && !s.contains(">named<"), "{s}");
    }

    /// 대화 id 로 덮인 잇기(옛 판, B107)는 첫 목록에서 기록 id 로 되돌아가고, 같은 주기에 그 행을 찾는다.
    #[test]
    fn an_overwritten_link_is_restored_by_the_poll() {
        #[derive(Default)]
        struct Meta(std::cell::RefCell<Option<String>>);
        impl HostCall for Meta {
            fn call(&self, method: &str, p: Value) -> Result<Value, String> {
                if p["key"] != crate::application::surface_link::SESSION_META_KEY {
                    return Ok(Value::Null);
                }
                match method {
                    "surface.meta.set" => {
                        *self.0.borrow_mut() = p["value"].as_str().map(str::to_string);
                        Ok(Value::Null)
                    }
                    "surface.meta.get" => Ok(json!({ "value": *self.0.borrow() })),
                    _ => Ok(Value::Null),
                }
            }
        }
        let host = Meta(std::cell::RefCell::new(Some("conv-9".into())));
        let mut convos: Convos = HashMap::new();
        convos.insert(451, convo("conv-9", false));
        let rows = json!({"rows":[{"id":"rec-1","agent_session_id":"conv-9"}]});
        Activity::default().note_agent_sessions(&host, &mut convos, &rows);
        assert_eq!(
            host.0.borrow().as_deref(),
            Some("rec-1"),
            "잇기를 안 되돌렸다"
        );
        let c = &convos[&451];
        assert_eq!(c.session_id.as_deref(), Some("rec-1"));
        assert_eq!(
            c.agent_session_id.as_deref(),
            Some("conv-9"),
            "되돌린 주기에 행을 못 찾았다"
        );
        assert_eq!(
            host.0.borrow().as_deref(),
            Some("rec-1"),
            "행 반영이 잇기를 다시 덮었다"
        );
    }

    /// 소식 없는 턴은 120초에 한 번 알린다 — 같은 턴에 다시 안 알리고, 턴이 끝나면 거두고, 300초
    /// 안에는 다음 턴이어도 안 알린다. claude 가 아닌 세션은 알리지 않는다.
    #[test]
    fn a_quiet_turn_raises_the_stall_hook_once() {
        let t0 = std::time::Instant::now();
        let at = |s: u64| t0 + std::time::Duration::from_secs(s);
        let mut c = convo("s1", false);
        assert!(!stall_event(&mut c, Some(119), at(0)), "기준 전에 알렸다");
        assert!(stall_event(&mut c, Some(120), at(1)));
        assert!(
            !stall_event(&mut c, Some(500), at(2)),
            "같은 턴에 또 알렸다"
        );
        assert!(
            !stall_event(&mut c, Some(900), at(400)),
            "냉각이 지났어도 같은 턴은 다시 안 알린다"
        );
        assert!(!stall_event(&mut c, None, at(3)), "턴이 끝났는데 알렸다");
        assert!(!c.stall_fired, "턴이 끝났는데 표시를 안 거뒀다");
        assert!(
            !stall_event(&mut c, Some(200), at(100)),
            "냉각 안에 다음 턴을 알렸다"
        );
        stall_event(&mut c, None, at(101));
        assert!(
            stall_event(&mut c, Some(200), at(302)),
            "냉각이 지났는데 안 알렸다"
        );
        let mut p = convo("s1", false);
        p.agent_kind = Some("pi".into());
        assert!(!stall_event(&mut p, Some(999), at(0)));
    }

    /// 목록 주기가 행의 물음(`needs_input`)을 탭에 옮긴다 — 카드 · 표시 · hook 은 `prompts` 가 맡는다.
    #[test]
    fn the_list_poll_carries_a_pending_prompt_to_the_tab() {
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = RecordingHost::default();
        let rows = json!({"rows":[{"id":"s1","kind":"claude","needs_input":
            {"request_id":"r-1","kind":"permission","tool":"Bash","input":{}}}]});
        Activity::default().note_agent_sessions(&host, &mut convos, &rows);
        assert_eq!(
            convos[&7007]
                .needs_input
                .as_ref()
                .map(|p| p.request_id.as_str()),
            Some("r-1")
        );
        assert!(
            all_sent(&host, 7007)
                .iter()
                .any(|s| s.contains(r#""type":"ask.set""#)),
            "카드를 안 그렸다"
        );
    }

    /// 목록 주기가 소식 없는 턴을 hook 으로 낸다 — 행의 `turn_quiet_secs` 를 본다.
    #[test]
    fn the_list_poll_fires_the_stall_hook() {
        #[derive(Default)]
        struct Hooks(std::cell::RefCell<Vec<String>>);
        impl HostCall for Hooks {
            fn call(&self, method: &str, params: Value) -> Result<Value, String> {
                if method == "surface.fire_hook" {
                    self.0
                        .borrow_mut()
                        .push(params["event"].as_str().unwrap_or_default().into());
                }
                Ok(Value::Null)
            }
        }
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        let host = Hooks::default();
        let rows = |q: u64| json!({"rows":[{"id":"s1","kind":"claude","turn_quiet_secs":q}]});
        Activity::default().note_agent_sessions(&host, &mut convos, &rows(30));
        assert!(host.0.borrow().is_empty());
        Activity::default().note_agent_sessions(&host, &mut convos, &rows(130));
        assert_eq!(*host.0.borrow(), vec![STALLED_EVENT.to_string()]);
    }

    /// `turn_active` 를 안 내는 행은 도는 것으로 안 친다.
    #[test]
    fn only_rows_that_say_turn_active_count() {
        use crate::ports::supervisor::SessionRow;
        let rows = vec![
            SessionRow::from_raw_for_test(json!({"id":"a","state":"running","turn_active":true})),
            SessionRow::from_raw_for_test(json!({"id":"b","state":"running","turn_active":false})),
            SessionRow::from_raw_for_test(json!({"id":"c","state":"running"})),
        ];
        assert_eq!(active_ids(&rows), vec!["a"]);
    }

    /// 데몬이 거절하면 `ok=false` 로 낸다 — 빈 목록(`ok=true`)과 갈린다.
    #[test]
    fn a_failed_list_is_not_an_empty_list() {
        let sup = crate::ports::supervisor::fake::FakeSupervisor::failing("down");
        assert_eq!(poll(&sup), json!({ "ok": false }));
    }

    /// 표시 줄은 꺼진 채로 그려지고, 메시지 처리기가 찾는 id 를 갖는다.
    #[test]
    fn the_row_starts_hidden_and_carries_the_scripted_id() {
        let h = row_html();
        assert!(h.contains(r#"id="act""#) && h.contains(" hidden>"));
        assert_eq!(
            show_msg(true),
            json!({ "type": "activity.show", "show": true })
        );
        assert!(
            crate::adapters::driving::messages::script_html()
                .contains("var a=byId('act');if(a)a.hidden=!m.show;")
        );
    }

    /// 턴이 끝나도 턴 밖 작업(백그라운드 Bash · Monitor · 하위 에이전트)이 남은 세션은 넓은 판정으로
    /// 바쁘다 — 탭 점 · 정지 전 질의가 그것을 본다. 입력 판정(`turn_busy`)은 막지 않는다.
    #[test]
    fn background_work_is_busy_for_the_broad_judgement_only() {
        let host = BusyHost::default();
        let mut act = Activity::default();
        let mut convos: Convos = HashMap::new();
        convos.insert(7007, convo("s1", false));
        act.apply(
            &host,
            &convos,
            &json!({ "ok": true, "active": [], "working": ["s1"] }),
        );
        let c = &convos[&7007];
        assert!(act.is_busy(c), "턴 밖 작업이 남은 세션은 바쁘다");
        assert!(!act.turn_busy(c), "입력은 막지 않는다");
        assert_eq!(*host.calls.borrow(), vec![(7007, true)]);
        assert_eq!(busy_report(&act, &convos)["surfaces"], json!([7007]));
        act.apply(
            &host,
            &convos,
            &json!({ "ok": true, "active": [], "working": [] }),
        );
        assert!(!act.is_busy(&convos[&7007]));
    }

    /// 폴러는 행의 `busy` 를 `working` 으로, `turn_active` 를 `active` 로 옮긴다.
    #[test]
    fn the_poll_carries_busy_rows_as_working() {
        let rows = vec![
            crate::ports::supervisor::SessionRow::from_raw_for_test(
                json!({"id":"a","state":"running","turn_active":false,"busy":true}),
            ),
            crate::ports::supervisor::SessionRow::from_raw_for_test(
                json!({"id":"b","state":"running","turn_active":true,"busy":true}),
            ),
        ];
        assert_eq!(working_ids(&rows), vec!["a", "b"]);
        assert_eq!(active_ids(&rows), vec!["b"]);
    }
}
