//! 대화 탭 하나의 상태 — 턴 기록과 그 탭을 둘러싼 진행 · 대기 · 표시 상태, 턴 전이, 저장 모양.
//!
//! 화면(`view`) · 문서 싣기(`session_surface`) · 탭 주소(`url`)는 이것을 읽고 쓰기만 한다. 여기에는 호스트 호출이 없다.

use std::collections::HashMap;

/// 폴더 피커가 고른 값을 넣을 칸 — 연결 탭의 작업 폴더(기본) · 에이전트의 작업 폴더 · 역할 폴더 ·
/// 가져올 상위 폴더.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickField {
    #[default]
    Start,
    AgentCwd,
    RolePath,
    RoleScan,
}

impl PickField {
    /// 폼 칸 이름 — 단추 주소의 `pf=` 값과 같다.
    pub fn param(self) -> &'static str {
        match self {
            PickField::Start => "c",
            PickField::AgentCwd => "pw",
            PickField::RolePath => "rp",
            PickField::RoleScan => "rs",
        }
    }

    pub(crate) fn of_param(v: &str) -> Self {
        [Self::AgentCwd, Self::RolePath, Self::RoleScan]
            .into_iter()
            .find(|f| f.param() == v)
            .unwrap_or_default()
    }
}

/// 시작 폼이 보여 줄 기존 세션 하나.
#[derive(Clone)]
pub struct Attachable {
    pub id: String,
    pub state: String,
    pub agent: String,
    /// 회수(재워짐)인가 — `stopped` 하나로는 *"우리가 재웠다"* 와
    /// *"죽었다"* 가 안 갈린다.
    pub reclaimed: bool,
    /// 이 세션이 도는 폴더. `None` 은 **"agent 설정을 따른다"** 이지
    /// *"폴더가 없다"* 가 아니다 — 그래서 화면에도 그렇게 적는다.
    pub cwd: Option<String>,
    /// **이미 다른 탭이 쓰고 있나** — 그 탭의 surface id(2026-10-02).
    ///
    /// # 왜 목록에서 빼지 않나
    ///
    /// 종전에는 물린 세션을 **목록에서 통째로 뺐다**. surface 는 세션
    /// 하나만 들 수 있어(`surface.meta` 가 1:1) 고르면 앞 탭의 짝이
    /// 조용히 끊기기 때문이다. 그런데 그러면 사용자 쪽에서는 세션이
    /// **사라진 것처럼** 보인다 — *"다른 탭에 있다"* 와 *"없다"* 가
    /// 같은 화면이 된다(사용자 보고).
    ///
    /// ⇒ **보여주되 못 고르게** 한다. 어디 있는지 알면 그 탭으로 가면
    /// 된다. `None` 이면 비어 있어 붙을 수 있다.
    pub taken_by: Option<u32>,
    /// 그 세션이 붙든 에이전트 쪽 세션 id 와 이름 — 이어받은 대화는 supervisor id 와 다르다.
    /// 이름은 데몬 행의 `name`, 없으면 대화 기록의 이름 줄(`session_info::fill_names`).
    pub agent_session_id: Option<String>,
    pub agent_session_name: Option<String>,
    /// 남은 일 — 결정 대기 · 미커밋 · 마지막 활동(`remaining`).
    pub remaining: crate::adapters::driven::remaining::Remaining,
}

/// 한 턴 — 사람이 넣은 것과 에이전트가 답한 것.
#[derive(Clone)]
pub struct Turn {
    pub sent: String,
    pub got: String,
    ///  **아직 답을 기다리는 중**(0.1.23).
    ///
    /// `got` 가 빈 것과 다르다 — 빈 답을 받은 턴도 `got` 가 비어 있다.
    /// 그 둘을 안 가르면 *"답이 없다"* 와 *"아직이다"* 가 같아 보인다.
    pub pending: bool,
    /// 실패면 사유. 성공이면 `None`.
    pub error: Option<String>,
    /// 턴이 열린 시각(유닉스 초) — 화면의 날짜 줄과 `[HH:MM:ss]` 가 쓴다(`turn_time`).
    /// 모르면(시각이 없던 스냅샷 · 기록) `None` 이고 그 턴에는 시각을 안 붙인다.
    pub at: Option<i64>,
}

/// surface 하나의 대화 상태.
#[derive(Default)]
pub struct Convo {
    pub session_id: Option<String>,
    /// plugin 설정이 보낸 제어 턴의 `[system]` 줄(`sys_note`).
    pub sys: Vec<crate::domain::sys_note::Note>,
    /// 그 세션이 붙든 **에이전트 쪽 세션 id**(claude 면 Claude Code 의 세션 id) — 세션 목록이
    /// 주기마다 채운다(`activity::Activity::note_agent_sessions`). supervisor id 와 다를 때만
    /// 상태줄에 따로 보인다. 사용자가 이어받기 칸에 넣은 값이 이것이라, 안 보이면 탭이 그
    /// 대화를 이어받았는지 새로 띄웠는지 화면에서 가를 수 없다.
    pub agent_session_id: Option<String>,
    /// 시작 폼에 단추로 놓을 agent 이름 — 지금 도는 세션들에서 모은다.
    ///
    /// `profiles` 를 읽었으면 이것은 안 쓴다 — 명부가 더 정확하다.
    pub known_agents: Vec<String>,
    /// supervisor 설정의 agent 프로필 전수. 못 읽으면 비고, 그때는 직접 친다.
    pub profiles: Vec<crate::adapters::driven::store::agents::AgentProfile>,
    /// 역할 폴더 명부 — 시작 폼의 역할 칸과 관리 절이 쓴다.
    pub roles: Vec<crate::adapters::driven::store::roles::Role>,
    /// 관리 탭(에이전트 · 역할)의 마지막 결과 — 화면을 다시 그릴 때 그 탭 위에 보인다.
    pub manage_note: Option<crate::adapters::driving::manage::Note>,
    /// 열린 폴더 피커의 답을 넣을 칸 — 찾기 단추가 정한다.
    pub pick_target: PickField,
    /// 관리 탭의 폴더 칸에 고른 값 — 한 번 그리고 비운다(`cwd_pick` 과 달리 연결 폼 칸이 아니다).
    pub field_pick: Option<(PickField, String)>,
    /// 이 탭의 surface id — 세션 표가 "이 탭" 을 가른다. 목록을 받을 때 채운다.
    pub tab_id: Option<u32>,
    /// 폴더 단추로 고른 값 — 폼을 다시 그릴 때 칸에 되싣는다.
    pub cwd_pick: String,
    /// 직전 시작이 **실패한 사유** — 화면에 남긴다.
    ///
    /// # 왜 상태로 두나
    ///
    /// 실패하면 폼을 다시 그리는데, 그 HTML 이 **직전과 바이트까지 같으면**
    /// 호스트가 재적재를 건너뛴다(`resync_webview_urls` 의 URL 비교).
    /// 그러면 스크립트가 씌운 spinner 가 그대로 남아 **영영 도는 것처럼**
    /// 보인다(실사용 보고 2026-09-30).
    ///
    /// 사유를 실으면 문서가 달라져 재적재가 일어나고, 사용자도 **왜**
    /// 안 됐는지 안다 — 두 문제가 한 값으로 풀린다.
    pub start_error: Option<String>,
    /// 붙을 수 있는 기존 세션 — 시작 폼이 목록으로 보여 준다.
    ///
    /// **이미 다른 탭이 든 세션은 빼고 준다**(호출부가 거른다) — surface 는
    /// 세션 하나만 들 수 있어(`surface.meta` 가 1:1), 이미 물린 것을 고르면
    /// 앞 탭의 짝이 조용히 끊긴다.
    pub attachable: Vec<Attachable>,
    pub turns: Vec<Turn>,
    /// 다음 nonce — 같은 입력을 두 번 보낼 수 있게 매번 올린다.
    pub nonce: u64,
    /// `send_input` 이 상한을 넘겨 **답을 구독으로 받기로 한** 턴의 자리(오래된 순).
    ///
    /// 상한을 넘겼다고 턴이 끝난 것이 아니다 — 데몬에서는 계속 돈다. 종전에는 그
    /// 자리를 실패로 닫아, 턴이 끝난 뒤의 답이 그 탭에 끝내 안 붙었다(`stream` 은
    /// 이 탭이 기다리던 턴의 답을 버린다). 저장하지 않는다 — 대기 중인 턴은
    /// 복원되지 않는다(`turns_from_snapshot`).
    pub overdue: Vec<usize>,
    /// 이 탭에서 **중지를 보냈다** — 다음에 실패로 끝나는 턴은 실패가 아니라 사용자가
    /// 멈춘 것이다. 턴이 끝나면(성공이든 실패든) 거둔다.
    pub stop_requested: bool,
    /// 사용자가 멈춘 턴의 자리 — 실패(`error`)와 따로 그린다. 멈춘 턴을 빨간 실패로
    /// 보이면 일부러 한 일이 오류처럼 읽힌다(실측 2026-10-16: 데몬의 실패 원문이
    /// 그대로 찍혔다). 저장하지 않는다 — 복원된 대화에서는 빈 답으로 보인다.
    pub stopped: Vec<usize>,
    /// 디스크로 옮긴 턴 수(`turn_archive`) — "이전 대화" 줄이 보여 준다. 0 이면 그 줄이 없다.
    pub archived: usize,
    /// "하위 에이전트" 줄 — 누르면 읽는다(`subagents`). 대화 기록이 정본이라 저장하지 않는다.
    pub subs: crate::adapters::driven::subagents::Panel,
    /// 한도를 넘겨 **버린** 턴 수(스왑 꺼짐) — 주고받은 수에만 쓴다.
    pub dropped: usize,
    /// 이 탭이 연 턴의 표 — 구독이 본 턴이 이 탭의 것인지 가른다(`own_turns`).
    pub own_turns: crate::domain::own_turns::OwnTurns,
    /// 사용자가 펼친 이전 대화 — 보관 파일에서 읽어 화면에 그린다(자리 index 를 안 받는다).
    /// 문서를 다시 실을 때까지 남는다.
    pub older: Vec<Turn>,
    /// 턴이 도는 동안 이 탭에서 보낸 말 — 턴이 끝나면 한 메시지로 합쳐 보낸다(`send_queue`).
    pub queued: Vec<String>,
    /// 띄운 세션에 자기 자리를 알리는 첫 턴이 도는 중 — 화면에 안 내는 턴이라 `turns` 에
    /// 없지만 세션은 바쁘다. 저장하지 않는다.
    pub briefing: bool,
    /// 대화를 비우는 `/clear` 턴이 도는 중(`clear`) — 맥락 턴처럼 화면에 안 내지만 세션은
    /// 바쁘다. 저장하지 않는다.
    pub clearing: bool,
    /// 복원된 탭 — 다음에 끝나는 턴 뒤에 대화 기록 꼬리를 한 번 더 읽는다(`catch_up`). 저장하지 않는다.
    pub catch_up_on_finish: bool,
    /// 데몬이 세션을 띄우며 남긴 경고(protocol 15) — 대화 위에 보인다. 저장하지 않는다.
    pub start_warning: Option<String>,
    /// 상태줄의 모델 · 컨텍스트 · 비용 · 사용률 — 원문 구독이 채운다(`stream`). 저장하지
    /// 않는다 — 다음 턴의 원문이 다시 채운다.
    pub usage: crate::domain::usage::Usage,
    /// 세션 이름 — 데몬 행의 `name`(있을 때) · 대화 기록의 이름 줄(`session_info`). 저장하지 않는다.
    pub listed_name: Option<String>,
    pub transcript_name: Option<String>,
    /// 데몬 행의 프로세스 칸 — 살아 있는 세션에만 온다. 저장하지 않는다.
    pub process: Option<crate::adapters::driven::session_info::Proc>,
    /// 마지막으로 그린 프로세스 칸 — 같은 값으로 매 주기 다시 그리지 않으려고 둔다.
    pub process_shown: String,
    /// 데몬 행의 agent 종류(`claude`·`pi`) — `claude-*` hook 은 claude 세션에서만 발사한다.
    pub agent_kind: Option<String>,
    /// 상태줄을 접었나 — 탭이 쥔다(문서를 다시 실어도 풀리지 않는다). 저장하지 않는다.
    pub status_folded: bool,
    /// 데몬 행의 agent 프로필 이름 · 작업 폴더 — 상태줄 첫 줄. 저장하지 않는다.
    pub agent_name: Option<String>,
    pub listed_cwd: Option<String>,
    /// 그 agent 프로필이 로그인한 계정 — 구독 워커가 프로필 설정에서 한 번 읽는다(`budget::account_email`).
    /// API 키 프로필 · claude 가 아닌 agent 는 없다. 저장하지 않는다.
    pub account_email: Option<String>,
    /// 이 탭의 surface id — 상태줄에 보이고 누르면 복사한다(`statusline::copy_surface`). 0 이면 모른다.
    pub surface_id: u32,
    /// 그 agent 프로필 전체의 최근 1분 사용(`rate_meter`) — 상태줄 넷째 줄. 저장하지 않는다.
    pub meter: Option<crate::adapters::driven::rate_meter::Meter>,
    /// 그 프로필이 429 로 막힌 지 몇 초째인가(`rate_block`) — 막히지 않았으면 `None`. 저장하지 않는다.
    pub held_secs: Option<u64>,
    /// 시작 화면의 프로필별 모델 — 데몬에서 받은 것(`models`). 없는 프로필은 별칭을 보인다. 저장하지 않는다.
    pub models: std::collections::BTreeMap<String, crate::domain::models::Choices>,
    /// 세션이 기다리는 물음 — 데몬 행의 `needs_input`(protocol 13). 저장하지 않는다.
    pub needs_input: Option<crate::application::prompts::Pending>,
    /// 이 턴에 `claude-error-stalled` 를 이미 냈나 · 마지막으로 낸 때(`activity::stall_event`).
    pub stall_fired: bool,
    pub last_stall: Option<std::time::Instant>,
    /// 현행화 → compact 진행 · 자동 수행 판정(`sync_compact`). 저장하지 않는다.
    pub sync: crate::application::sync_compact::SyncState,
    /// 메모리 절약 — 다시 실을 때 되살릴 화면 상태(`view_state`).
    pub view: crate::application::view_state::ViewState,
    /// 분당 한도(429)로 막힌 턴의 재전송 예약(`retry_429`). 저장하지 않는다.
    pub retry: crate::domain::retry_429::State,
    /// 프롬프트 캐시 재작성 감지(`cache_watch`). 저장하지 않는다.
    pub cache: crate::application::cache_watch::State,
    /// overdue 턴이 남았는데 데몬이 턴 없음이라 한 연속 주기 수(`overdue`). 저장하지 않는다.
    pub overdue_idle_polls: u8,
    /// hook 없이 응답 흐름에서 읽는 신호의 상태(`hookless`). 저장하지 않는다.
    pub hookless: crate::application::hookless::State,
    /// 마지막으로 중지를 누른 때 — 두 번째 누름이면 탭에서 턴을 닫는다(`overdue::force_stop`). 저장하지 않는다.
    pub last_stop_at: Option<std::time::Instant>,
}

impl Convo {
    /// 상태줄에 보일 세션 이름 — 데몬이 낸 것이 이긴다.
    pub fn display_name(&self) -> Option<&str> {
        self.listed_name
            .as_deref()
            .or(self.transcript_name.as_deref())
    }
}

/// surface 별 대화. plugin 이 들고 있다.
pub type Convos = HashMap<u32, Convo>;

/// 턴을 **연다** — 보낸 말을 대화에 붙이고 답을 기다리는 표시를 남긴다.
///
/// # 왜 보내기와 갈랐나(0.1.23)
///
/// `send_input` 은 **모델이 답을 다 낼 때까지 안 돌아온다**
/// (`client.rs` 의 `TURN_TIMEOUT` 주석). 그것을 plugin 루프에서 부르면
/// 그 동안 **아무 요청도 못 받는다** — 두 번째 메시지가 유실되고,
/// 60초를 넘으면 호스트가 plugin 을 죽인다(실측 기록).
///
/// ⇒ 전송은 워커가 맡고, 이 함수는 **화면에 보일 것만** 만든다.
/// 돌려주는 값은 그 턴의 자리(index)다 — 답이 오면 [`finish_turn`] 이
/// 그 자리를 채운다.
///
/// ⚠️ **자리를 index 로 들고 다닌다.** 답이 도착 순서대로 오지 않을 수
/// 있고(여러 개를 연속으로 보내면), 그때 마지막 턴을 채우면 **엉뚱한
/// 질문에 답이 붙는다**.
pub fn begin_turn(convo: &mut Convo, text: &str) -> usize {
    // nonce 를 먼저 올린다 — 실패해도 다음 입력이 막히면 안 된다.
    convo.nonce = convo.nonce.wrapping_add(1);
    convo.own_turns.opened();
    // 사람이(또는 `tell` 이) 보낸 말이면 새 쉼이 시작된다 — 유휴 현행화를 다시 열어 둔다. 현행화 턴
    // 자신은 진행 중(`running`)이라 풀지 않는다.
    if !convo.sync.running() {
        convo.sync.idle_spent = false;
        convo.sync.touch(std::time::Instant::now());
    }
    // 새 말이 나간다 — 429 재전송 예약은 버린다(사람이 직접 다시 보냈을 수 있다). 재전송 자신은
    // 예약을 꺼낸 뒤 여기 오므로 상관없다.
    convo.retry.pending = None;
    // 이 턴을 연 슬래시 명령 — 캐시 재작성의 원인 짐작에 쓴다(`cache_watch`).
    convo.cache.command = crate::application::cache_watch::command_of(text);
    convo.turns.push(Turn {
        sent: text.to_string(),
        got: String::new(),
        error: None,
        pending: true,
        at: Some(crate::adapters::driven::store::budget::now()),
    });
    convo.turns.len() - 1
}

/// 그 턴의 답이 왔다 — 자리를 채운다.
///
/// ⚠️ **자리가 없으면 조용히 버린다.** 탭이 닫혔거나 대화가 새로
/// 그려졌을 수 있다 — 그때 패닉하면 워커가 죽어 다음 답도 못 받는다.
pub fn finish_turn(convo: &mut Convo, at: usize, got: Result<String, String>) {
    convo.own_turns.closed(std::time::Instant::now());
    let stop = std::mem::take(&mut convo.stop_requested);
    let Some(t) = convo.turns.get_mut(at) else {
        return;
    };
    t.pending = false;
    let stopped = match got {
        Ok(text) => {
            t.got = text;
            false
        }
        // 중지를 보낸 뒤의 실패는 실패가 아니다 — 데몬은 끊긴 턴을 실패로 닫는다.
        Err(_) if stop => true,
        Err(e) => {
            t.error = Some(e);
            false
        }
    };
    if stopped && !convo.stopped.contains(&at) {
        convo.stopped.push(at);
    }
    convo.overdue.retain(|&i| i != at);
}

/// `send_input` 이 상한을 넘겼다 — 그 턴을 **열어 둔 채** 답을 구독에 맡긴다.
///
/// 이미 닫힌 자리(다른 길로 답이 온 턴)는 표시하지 않는다.
pub fn mark_overdue(convo: &mut Convo, at: usize) -> bool {
    if !convo.turns.get(at).is_some_and(|t| t.pending) || convo.overdue.contains(&at) {
        return false;
    }
    convo.overdue.push(at);
    true
}

/// 이 탭이 답을 기다리는 턴이 있나 — **overdue 턴은 빼고** 본다.
///
/// overdue 턴은 그 턴의 `started` 때 이미 이 탭의 것으로 열렸다. 그 뒤에 시작하는 턴(task-notification 등)을
/// 이 탭의 것으로 치면 그 끝이 overdue 자리를 엉뚱한 답으로 닫는다(B37, surface 93 실측).
pub fn waits_for_own_turn(convo: &Convo) -> bool {
    convo
        .turns
        .iter()
        .enumerate()
        .any(|(i, t)| t.pending && !convo.overdue.contains(&i))
}

/// 구독이 기다리던 턴의 끝을 봤다 — **가장 오래 기다린** 자리를 그 답으로 닫는다.
///
/// 돌려주는 값은 닫은 자리다. 기다리는 자리가 없으면 `None`.
pub fn settle_overdue(convo: &mut Convo, got: Result<String, String>) -> Option<usize> {
    let at = *convo.overdue.first()?;
    finish_turn(convo, at, got);
    Some(at)
}

/// 데몬 응답에서 답 텍스트를 꺼낸다 — 워커와 동기 경로가 **같은 곳**을 본다.
pub fn answer_of(v: &serde_json::Value) -> String {
    v.pointer("/output/text")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string()
}

/// 재시작을 건너 이 표면을 되살릴 **최소 정보**.
///
/// # 왜 필요한가
///
/// 탭과 세션의 짝은 `surface.meta` 에 있는데, 호스트는 복원 직후
/// **live 가 아닌 Surface scope 를 전부 지운다**(`src/runtime/surface_registry/meta.rs` 의
/// `SurfaceMetaStore::purge_dead_surfaces`). `surface_id` 는 **매 실행 재발급**되므로
/// 옛 id 에 달린 짝은 살아남지 못한다.
///
/// 그래서 짝을 **레이아웃 스냅샷에도** 싣는다 — 이것은 호스트가
/// 레이아웃 파일로 영속하고 복원 때 `restore_surface` 의 `data` 로
/// 돌려준다.
///
/// **대화 내용은 안 싣는다.** 그것은 데몬이 들고 있고
/// (`session.output`), 여기 실으면 레이아웃 파일이 턴마다 커진다.
/// 실어야 할 것은 *"어느 세션이었나"* 하나다.
pub fn snapshot_of(session_id: Option<&str>) -> serde_json::Value {
    match session_id {
        // `open_session_surface` 가 읽는 이름과 **같은 키**여야 한다 —
        // 그쪽이 `session_id` 로 기존 세션에 붙는 갈래를 이미 갖고 있다.
        Some(id) => serde_json::json!({ "session_id": id }),
        // 짝이 없으면 빈 채로 둔다 — 복원이 시작 폼을 그린다.
        None => serde_json::json!({}),
    }
}

/// 스냅샷에 **대화까지** 싣는다(R4 · 2026-09-25).
///
/// # 왜 데몬에 안 맡기나
///
/// 실측: `session.output` 은 **답만** 들고 질문이 없고, 그 꼬리는
/// 메모리 맵이라 데몬 재기동에 사라진다(200줄 상한). 대화를 재구성할
/// 수 없다 — 이 탭이 무엇을 보여 주고 있었는지는 **우리만 안다**.
///
/// **좌표와 다른 축이다.** 좌표(`session_id`)는 *"어느 세션인가"* 라
/// 데몬이 정본이고, 턴은 *"이 탭의 화면"* 이라 plugin 이 정본이다.
///
/// **빈 대화는 칸을 안 만든다** — 레이아웃 파일에 빈 배열이 쌓이는
/// 것을 막는다.
pub fn snapshot_with_turns(session_id: Option<&str>, turns: &[Turn]) -> serde_json::Value {
    let mut v = snapshot_of(session_id);
    if turns.is_empty() {
        return v;
    }
    let rows: Vec<serde_json::Value> = turns
        .iter()
        .map(|t| {
            serde_json::json!({
                "sent": t.sent,
                "got": t.got,
                // 실패를 삼키지 않는다 — 답이 안 온 것과 실패한 것은 다르다.
                "error": t.error,
                "at": t.at,
            })
        })
        .collect();
    if let Some(o) = v.as_object_mut() {
        o.insert("turns".to_string(), serde_json::Value::Array(rows));
    }
    v
}

/// 스냅샷에서 대화를 되읽는다.
///
/// **모양이 틀리면 빈 대화다** — 크래시가 아니라. 레이아웃 파일은
/// 옛 판이 남아 있을 수 있고(턴이 없던 시절), 거기서 죽으면 **탭이
/// 통째로 안 열린다**.
pub fn turns_from_snapshot(snap: &serde_json::Value) -> Vec<Turn> {
    let Some(rows) = snap.get("turns").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|r| {
            Some(Turn {
                // `sent` 가 없으면 그 턴은 못 그린다 — 통째로 버린다.
                sent: r.get("sent")?.as_str()?.to_string(),
                got: r
                    .get("got")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                //  **복원된 턴은 대기 중이 아니다.** 그 워커는 이미
                // 사라졌으므로, 대기로 두면 영영 도는 점 셋이 남는다.
                pending: false,
                at: r.get("at").and_then(serde_json::Value::as_i64),
                error: r
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    /// 이어진 세션은 **스냅샷에 실린다** — 안 실으면 재시작 후 그 탭이
    /// 시작 폼으로 떨어진다.
    #[test]
    fn a_linked_session_is_carried_in_the_snapshot() {
        let v = snapshot_of(Some("abc-123"));
        assert_eq!(v["session_id"], "abc-123");
    }

    /// **키 이름이 `open_session_surface` 와 같아야 한다.** 다르면
    /// 복원이 그 값을 못 읽고 **조용히 시작 폼**을 그린다 — 오류가 없다.
    #[test]
    fn the_key_is_the_one_the_restore_path_reads() {
        let v = snapshot_of(Some("x"));
        let keys: Vec<&str> = v
            .as_object()
            .expect("객체")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["session_id"], "복원이 읽는 키와 다르다");
    }

    /// 짝이 없으면 **빈 객체** — 복원이 시작 폼을 그린다.
    #[test]
    fn without_a_session_the_snapshot_is_empty() {
        let v = snapshot_of(None);
        assert_eq!(v, serde_json::json!({}));
        assert!(v.get("session_id").is_none(), "없는 짝을 실었다");
    }
}
