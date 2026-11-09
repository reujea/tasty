//! plugin 의 상태 — surface 별 대화와 그 대화를 둘러싼 대기열 · 활동 · 워커 결과를 든다.
//!
//! 호스트 콜백(`Plugin` 구현)은 조립 루트(main.rs)에 있고, 여기 메서드를 부른다.

use crate::adapters::driven::rate_meter;
use crate::adapters::driven::skills;
use crate::adapters::driven::store::agents;
use crate::adapters::driven::store::roles;
use crate::adapters::driven::store::turn_archive;
use crate::application::activity;
use crate::application::app_error::AppError;
use crate::application::clear;
use crate::application::copy_from;
use crate::application::history;
use crate::application::request;
use crate::application::request::session_for;
use crate::application::session_resolve;
use crate::application::session_surface;
use crate::application::stream;
use crate::application::surface_link;
use crate::application::sync_compact;
use crate::application::tell;
use crate::application::view_state;
use crate::domain::i18n;
use crate::domain::rate_block;
use crate::domain::send_queue;
use crate::domain::usage;
use crate::ports::host::SendHost;
use crate::ports::supervisor;
use serde_json::Value;
use serde_json::json;

/// 띄우고 이은 세션과, 그 세션에 아직 안 보낸 첫 턴들.
pub(crate) struct Started {
    pub(crate) id: String,
    /// 자기 자리를 알려 주는 첫 턴(`session_brief`).
    pub(crate) brief: String,
    /// 시작 요청에 실려 온 첫 메시지.
    pub(crate) prompt: Option<String>,
    /// 데몬이 띄우며 남긴 경고(protocol 15 `session.warnings` 의 `detail`) — 탭에 보인다.
    pub(crate) warnings: Vec<String>,
}

/// 호스트(`H`)와 데몬(`S`)은 포트로만 본다 — 실물(SDK 핸들 · `LocalSupervisor`)은 조립 루트가 정해 넣고,
/// 시험은 가짜 호스트와 임시 표의 데몬을 넣는다.
pub(crate) struct SupervisorPlugin<H, S> {
    /// `on_start` 에서 받아 둔다 — `create_surface` 는 host 를 안 준다(SDK 계약).
    pub(crate) host: Option<H>,
    /// `agent_session` surface 별 대화 기록.
    pub(crate) convos: crate::domain::convo::Convos,
    /// `file_picker.trigger` 로 보낸 요청 id → 그 요청을 낸 surface.
    ///
    /// ⚠️ 답(`file_picker.result`)에는 **surface 가 안 실려 온다** —
    /// `request_id` 뿐이다. 이 표가 없으면 어느 탭의 폴더를 바꿀지 모른다.
    pub(crate) pending_pick: std::collections::HashMap<u64, u32>,
    /// 탭이 보내지 않은 턴까지 포함한 "에이전트가 일하는 중" 표시.
    pub(crate) activity: activity::Activity,
    /// 현행화 설정 4칸의 짧은 수명 캐시 — 흐름 이벤트마다 도는 자리가 호스트에 다시 묻지 않게.
    pub(crate) knobs: sync_compact::KnobCache,
    /// 이 탭이 안 보낸 턴의 답을 대화에 붙이는 세션 구독.
    pub(crate) streams: stream::Streams,
    /// 대화 탭의 턴을 기다리는 `agent_session.tell` 들(`tell` 모듈).
    pub(crate) tells: tell::Tells,
    /// 이번 잠에 회수 표시를 이미 켠 탭(`ipc::handle_sync`) — 다시 켜지 않는다.
    pub(crate) reclaim_marked: std::collections::BTreeSet<u32>,
    /// 탭의 대화 비우기를 기다리는 `agent_session.clear` 들(`clear` 모듈).
    pub(crate) clears: clear::Waiting,
    /// agent 프로필별 429 차단 — 막힌 프로필의 자동 전송을 함께 멈춘다(`rate_block`).
    pub(crate) blocks: rate_block::Blocks,
    /// agent 프로필별 최근 1분 사용 — 별도 스레드가 잰다(`rate_meter`).
    pub(crate) meters: rate_meter::Meters,
    /// 세션 쪽 포트 — 조립 루트가 한 번 만들어 넣고, 모듈과 워커에는 복사본을 넘긴다.
    pub(crate) sup: S,
}

impl<H, S> SupervisorPlugin<H, S> {
    /// 빈 상태 — 호스트는 `on_start` 가 넣는다.
    pub(crate) fn new(sup: S) -> Self {
        Self {
            host: None,
            convos: Default::default(),
            pending_pick: Default::default(),
            activity: Default::default(),
            knobs: Default::default(),
            streams: Default::default(),
            tells: Default::default(),
            reclaim_marked: Default::default(),
            clears: Default::default(),
            blocks: Default::default(),
            meters: Default::default(),
            sup,
        }
    }
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 한 턴을 보낸다 — 화면에 보일 것만 여기서 만들고 **왕복은 워커가** 한다.
    ///
    /// **함수로 뽑은 이유**(0.1.51): 시작 폼의 *첫 메시지* 도 같은 길로
    /// 보내야 한다. 블록을 복사하면 두 벌이 생기고, 한쪽만 고쳐지는 날
    /// **그쪽 경로에서만 답이 안 오는** 형태로 나타난다 — 이 저장소가
    /// 여러 번 밟은 자리다.
    ///
    /// `staged` 는 페이지가 대기 칸에 올려 둔 임시 항목 번호다 — 보낸 말을 그 자리에서
    /// 빼고 대화 끝에 진행 중 턴으로 옮긴다(`send_queue::settle_script`).
    pub(crate) fn send_turn(
        &mut self,
        host: &impl SendHost,
        sid: u32,
        text: &str,
        staged: Option<u64>,
    ) {
        let text = text.to_string();
        let Some(session) = self.convos.get(&sid).and_then(|c| c.session_id.clone()) else {
            return;
        };
        let at = match self.convos.get_mut(&sid) {
            Some(c) => crate::domain::convo::begin_turn(c, &text),
            None => return,
        };
        // 대기열에 있던 `tell` 은 이 턴에 실렸다 — 이 턴의 답으로 답한다.
        self.tells.bind(sid, at);
        if let Some(c) = self.convos.get(&sid) {
            session_surface::patch(
                host,
                sid,
                &send_queue::settle_msg(staged, &c.queued, Some(&text)),
            );
            self.activity.refresh_status(host, sid, c);
        }
        //  **여기서 다시 싣지 않는다**(0.1.24). 보낸 말은 위 스크립트가 그 자리만
        // 그린다 — 문서를 갈아끼우면 (1) 깜빡이고 (2) 사용자가 그 사이 친 다음
        // 메시지가 **지워진다**(실사용 보고 2026-09-30).
        //
        // 모델(`Convo.turns`)에는 `begin_turn` 이 이미 넣었으므로, 탭을
        // 닫았다 열거나 재시작해도 그 턴은 스냅샷으로 되살아난다.

        let worker_host = host.clone();
        let sup = self.sup;
        if std::thread::Builder::new()
            .name(format!("agent-session-send-{sid}"))
            .spawn(move || {
                // 상한을 넘긴 것은 실패가 아니다 — 턴은 데몬에서 계속 돌 수 있다.
                // 그 답은 구독(`stream`)이 받아 이 자리에 붙인다.
                // `raw` 는 이 턴을 기다리는 `tell` 에 그대로 돌려준다 — 탭 없는 길과 모양이 같다.
                let (ok, overdue, body, raw) = match sup.send_input(&session, &text) {
                    Ok(v) => (true, false, crate::domain::convo::answer_of(&v), v),
                    Err(e @ supervisor::ClientError::TimedOut(_)) => {
                        (false, true, e.to_string(), Value::Null)
                    }
                    Err(e) => (false, false, e.to_string(), Value::Null),
                };
                // 되던지기가 실패하면 plugin 이 내려가는 중이다 — 조용히 끝낸다.
                #[expect(clippy::let_underscore_must_use, reason = "A failed self-invoke means the plugin is shutting down; nobody is left to tell.")]
                let _ = worker_host.self_invoke(
                    "agent_session.turn_done",
                    json!({ "surface_id": sid, "at": at, "ok": ok, "overdue": overdue, "text": body, "raw": raw }),
                );
            })
            .is_err()
        {
            //  워커를 못 띄우면 **그 턴이 영영 대기로 남는다**. 조용히
            // 두지 않고 그 자리에 사유를 적는다.
            if let Some(c) = self.convos.get_mut(&sid) {
                crate::domain::convo::finish_turn(c, at, Err(crate::domain::i18n::t("sup.err.worker_spawn")));
            }
            if let Some(c) = self.convos.get(&sid) {
                session_surface::push(host, &self.sup, sid, c, false);
            }
        }
    }

    /// 이 표면을 되살릴 스냅샷 — 이어진 세션 id 하나다.
    ///
    /// **`surface.meta` 만으로는 재시작을 못 넘는다.** 호스트가 복원 직후
    /// live 아닌 Surface scope 를 전부 지우고(`purge_dead_surfaces`)
    /// `surface_id` 는 매 실행 재발급되므로, 옛 id 에 달린 짝은 사라진다.
    /// 레이아웃 스냅샷은 호스트가 파일로 영속하므로 그 길로 나른다.
    pub(crate) fn snapshot_for(&self, surface_id: u32) -> Value {
        // 캐시가 아니라 **지금 실물을 묻는다** — 잇기는 언제든 바뀐다
        // (`link-add`). 캐시한 값을 저장하면 마지막 잇기가 안 실린다.
        let linked = self
            .host
            .as_ref()
            .and_then(|h| surface_link::session_of_surface(h, surface_id).ok());
        // 대화도 함께 싣는다(R4) — 데몬의 `output` 은 **답만** 들고 질문이
        // 없어 화면을 재구성하지 못한다(실측). 이 탭이 무엇을 보여 주고
        // 있었는지는 우리만 안다.
        let turns = self
            .convos
            .get(&surface_id)
            .map(|c| c.turns.as_slice())
            .unwrap_or(&[]);
        let queued = self
            .convos
            .get(&surface_id)
            .map(|c| c.queued.as_slice())
            .unwrap_or(&[]);
        send_queue::with_queued(
            crate::domain::convo::snapshot_with_turns(linked.as_deref(), turns),
            queued,
        )
    }

    /// `agent_session.tell` 을 보내고 **답은 나중에** 한다 — 대화 탭이면 그 탭의 대기열로, 아니면 워커로.
    ///
    /// 여기서 난 오류도 `reply` 로 보낸다 — 답을 미뤘으므로 돌려주는 값은 버려진다.
    pub(crate) fn tell_later(&mut self, params: &Value, reply: tell::Reply) {
        let Some(hh) = self.host.clone() else {
            reply(Err(AppError::new(i18n::t("sup.err.worker_spawn"))));
            return;
        };
        let target = session_for(&hh, params)
            .and_then(|(sid, session)| Ok((sid, session, request::tell_text(params)?)));
        let (sid, session, text) = match target {
            Ok(t) => t,
            Err(e) => {
                reply(Err(e));
                return;
            }
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            tell::direct(hh, self.sup, reply, sid, session, params.clone());
            return;
        };
        let busy = self.activity.turn_busy(c);
        // 물음(권한·AskUserQuestion)을 기다리는 탭이면 이 말은 대기열로 들어갈 뿐 물음을 풀지 못한다.
        // 답을 기다리게 두면 보낸 쪽은 상한까지 서 있다가 "도는 중" 만 받는다 — 곧바로 그렇게 답한다.
        if tell::waits_on_question(c) {
            send_queue::push(c, &text);
            reply(Ok(tell::queued_behind_question(sid, &session)));
            session_surface::patch(&hh, sid, &send_queue::settle_msg(None, &c.queued, None));
            return;
        }
        self.tells.park(sid, session, reply, tell::BUDGET);
        match send_queue::take_or_queue(c, &text, busy) {
            Some(batch) => self.send_turn(&hh, sid, &batch, None),
            None => {
                // 터미널 입력과 같은 로그 — 갇힌 tell 을 로그로 찾을 수 있게 한다(B37).
                tracing::info!(
                    "[agent_session] surface={sid}: 턴이 도는 중이라 tell 을 쌓았다 ({}건 대기)",
                    c.queued.len()
                );
                session_surface::patch(&hh, sid, &send_queue::settle_msg(None, &c.queued, None))
            }
        }
    }

    /// 쌓인 말이 있는 탭 중 **세션이 한가해진 것**에 한 메시지로 합쳐 보낸다.
    ///
    /// 턴의 끝을 아는 자리가 셋이라(이 탭의 답 · 구독 · 활동 폴링) 셋 다 부른다.
    /// 이 탭의 답이 와도 데몬이 아직 턴을 돈다고 하면 다음 폴링까지 기다린다 —
    /// 데몬이 그 세션을 놓기 전에 보내면 앞 턴과 겹친다.
    pub(crate) fn flush_queues(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        // 설정을 짧은 캐시로 다시 읽는다 — 트래픽 한도가 이 자리에서 조절기에 걸린다(`KnobCache::get`).
        // 쌓인 말을 보내기 전이라, 그 전송이 세션을 다시 띄우면 그 자식은 지금 설정으로 뜬다.
        let knobs = self.knobs.get(&host, std::time::Instant::now());
        // 상태줄의 두 시계 — 휴식 시계는 탭 조작 · 명령으로도 밀리는데 그 자리는 상태줄을 안 그린다.
        self.note_clocks(&host, &knobs);
        // 메모리 절약 — 열린 문서가 쓰던 글을 보낼지 정한다(`view_state`).
        self.note_view_saver(&host, knobs.memory_saver);
        let ready: Vec<(u32, String)> = self
            .convos
            .iter_mut()
            .filter(|(_, c)| !c.queued.is_empty())
            .filter_map(|(&sid, c)| {
                if self.activity.turn_busy(c) {
                    return None;
                }
                send_queue::take_batch(c).map(|t| (sid, t))
            })
            .collect();
        for (sid, text) in ready {
            tracing::info!("[agent_session] surface={sid}: 쌓인 말을 합쳐 보낸다");
            self.send_turn(&host, sid, &text, None);
        }
        // 턴 끝 · 구독 · 폴링이 모두 여기를 지난다 — 현행화의 다음 단계도 여기서 판정한다.
        self.apply_meters();
        self.run_pending_syncs();
        self.run_pending_retries();
    }

    /// `restoring` 이면 `params` 는 이 plugin 이 저장한 스냅샷이다 — 그 안의
    /// `session_id` 는 데몬에 묻지 않고 잇는다(`session_resolve::link_saved`).
    pub(crate) fn open_session_surface(
        &mut self,
        surface_id: u32,
        params: &Value,
        restoring: bool,
    ) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let h = &host;
        // 대화 탭 복제(`copy_of`) — 원본 탭의 설정을 `agent` 갈래가 읽는 칸으로 바꿔 아래를 그대로 지난다.
        // 못 읽으면 시작 화면에 사유를 싣는다(원본이 그새 닫혔거나 세션이 끊긴 경우).
        let mut copy_error = None;
        let copied;
        let params = match copy_from::source_of(params).filter(|_| !restoring) {
            Some(src) => match copy_from::read_source(&h, &self.sup, src) {
                Ok(c) => {
                    copied = copy_from::apply_to(params, &c);
                    &copied
                }
                Err(why) => {
                    copy_error = Some(why);
                    params
                }
            },
            None => params,
        };
        // 폼 값은 `params.params` 아래에 중첩돼 온다(SDK 계약 — html·markdown
        // plugin 과 같다). 평평한 자리도 함께 본다.
        let field = |k: &str| -> Option<String> {
            params
                .pointer(&format!("/params/{k}"))
                .or_else(|| params.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|v| !v.trim().is_empty())
        };

        // 세션을 정하는 순서는 **셋**이다:
        // 1. `session_id` 로 지목    — 이미 있는 것에 붙는다
        // 2. `agent` 로 새로 띄운다  — 그리고 이 탭에 잇는다
        // 3. 둘 다 없으면 이 surface 에 이미 이어진 것
        // **meta 는 평평한 칸에서 안 찾는다**(2026-10-06 정정).
        //
        // 호스트가 레이아웃·preset 을 담을 때 이 surface 의 `surface_meta` 를
        // 함께 싣는데, **전용 칸**(`__surface_meta`)에 둔다 — plugin 데이터와
        // 섞으면 복원 때 어느 키가 meta 에서 왔는지 가릴 수 없다.
        //
        // 그리고 복원 시점에 호스트가 그것을 **새 surface id 로 되심는다**
        // (id 는 재시작하면 바뀐다 — 실측 2 -> 4). 그러니 우리는 평소대로
        // 3번 갈래(`session_of_surface`)로 물으면 된다 — 그쪽이 `surface.
        // meta.get` 을 부른다.
        // 1번 갈래의 실패 사유 — 폼에 남겨 왜 안 이어졌는지 보이게 한다.
        let mut start_error = copy_error;
        // 2번 갈래로 띄웠으면 첫 턴들 — 대화를 만든 뒤 워커로 보낸다.
        let mut started: Option<Started> = None;
        let session = match field("session_id") {
            Some(saved) if restoring => match session_resolve::link_saved(&h, surface_id, &saved) {
                Ok(id) => Some(id),
                Err(why) => {
                    start_error = Some(why);
                    None
                }
            },
            Some(input) => match self.attach_by_input(&h, surface_id, &input) {
                Ok(id) => Some(id),
                Err(why) => {
                    start_error = Some(why);
                    None
                }
            },
            None => match field("agent") {
                Some(agent) => self
                    .start_session_for(&h, &self.sup, surface_id, &agent, params)
                    // 못 띄운 사유를 시작 화면에 싣는다 — 버리면 빈 폼만 보여 왜 안 됐는지 모른다.
                    .map_err(|why| start_error = Some(why))
                    .ok()
                    .map(|s| {
                        let id = s.id.clone();
                        started = Some(s);
                        id
                    }),
                None => surface_link::session_of_surface(&h, surface_id).ok(),
            },
        };

        // 세션이 없으면 시작 폼을 그린다 — 단추로 놓을 agent 를 지금 도는
        // 세션들에서 모은다(데몬이 소켓으로 설정을 안 내주기 때문이다).
        let (known_agents, attachable) = if session.is_none() {
            session_resolve::form_choices(&h, &self.sup)
        } else {
            (Vec::new(), Vec::new())
        };

        // 시작 화면에만 프로필 명부가 필요하다 — 폴더가 비어도 기본 프로필 하나는 온다(`agents::load`).
        let profiles = if session.is_none() {
            self.sup
                .agent_list()
                .unwrap_or_else(|_| vec![agents::AgentProfile::default_claude()])
        } else {
            Vec::new()
        };

        // **대화를 되싣는다**(R4). 복원이면 `params` 가 곧 우리가 저장한
        // 스냅샷이다 — 신규 생성이면 `turns` 칸이 없어 빈 대화가 된다.
        //
        // ⚠️ 여기서 안 읽으면 저장은 되는데 **화면에 안 나온다** — 저장과
        // 가시성은 다른 축이다(spec 완료조건 2번).
        let turns = crate::domain::convo::turns_from_snapshot(params);
        // 디스크로 옮겼던 턴 수 — 파일이 정본이다(스냅샷에 수를 따로 적으면 둘이 갈린다).
        let archived = session
            .as_deref()
            .zip(turn_archive::data_dir())
            .and_then(|(s, d)| turn_archive::archive_path(&d, s))
            .map_or(0, |p| turn_archive::count(&p));
        // 다른 곳에서 쓰던 대화면 이전 턴을 대화 기록에서 싣는다(턴 한도만큼만 메모리에).
        let (turns, archived) = history::for_tab(
            &h,
            &self.sup,
            session.as_deref(),
            turns,
            archived,
            restoring,
        );
        // 복원인데 스냅샷이 비었으면 뒤에서 읽는다 — 복원 응답을 늦추지 않는다(`history` 주석).
        if let Some(s) = session
            .as_deref()
            .filter(|_| restoring && turns.is_empty() && archived == 0)
        {
            history::load_in_background(
                host.clone(),
                self.sup,
                surface_id,
                s.to_string(),
                turn_archive::read_limits(&h),
            );
        }
        // 복원된 탭이면 빠진 답을 기록 꼬리로 맞춘다 — 도는 턴 중에 plugin 이 교체됐을 수 있다. 스냅샷이 비었어도
        // 켠다: 도는 턴은 스냅샷에 안 남아, 교체 순간 그 턴뿐이던 탭은 빈 채로 온다(실측). 읽는 때는 활동 폴링이
        // 그 세션을 한가하다고 볼 때다(`catch_up` 모듈).
        let catch_up_on_finish = restoring && session.is_some();
        // 쌓여 있던 말도 되살린다 — 세션이 한가하면 다음 폴링이 보낸다.
        let queued = send_queue::restore(session.as_deref(), params);
        // 마지막으로 본 상태줄 값 — meta 에 남겨 둔 것(`surface_link::USAGE_META_KEY`). 다음 원문이 오면 덮는다.
        let usage = session
            .as_ref()
            .and_then(|_| surface_link::load_usage(&h, surface_id))
            .map(|v| usage::from_snapshot(&v))
            .unwrap_or_default();
        // 시작 화면의 역할 칸 · 관리 절 — 세션이 이어진 탭은 안 쓴다.
        let form_roles = if session.is_none() {
            roles::load(roles::default_file().as_deref())
        } else {
            Vec::new()
        };
        let convo = crate::domain::convo::Convo {
            // 메모리 절약은 지금 설정으로 시작한다 — 꺼짐으로 그린 첫 문서에 켜짐 메시지(`view.save`)가 처리기보다
            // 먼저 닿으면 사라져, 그 문서는 쓰던 글을 끝내 안 보낸다(VM 실측 2026-11-04).
            view: view_state::ViewState {
                draft: view_state::restore(session.as_deref()),
                save: self.knobs.get(&h, std::time::Instant::now()).memory_saver,
            },
            surface_id,
            usage,
            queued,
            session_id: session,
            known_agents,
            attachable,
            roles: form_roles,
            profiles,
            turns,
            archived,
            start_error,
            catch_up_on_finish,
            start_warning: started
                .as_ref()
                .filter(|s| !s.warnings.is_empty())
                .map(|s| s.warnings.join("\n")),
            ..Default::default()
        };
        session_surface::push(&h, &self.sup, surface_id, &convo, false);
        self.convos.insert(surface_id, convo);
        if let Some(s) = started {
            self.begin_started(&host, surface_id, s.brief, s.prompt);
        }
    }

    /// 탭 만들기의 `session_id` 칸 — 데몬이 아는 세션으로 풀고 **이 탭에 잇는다**.
    ///
    /// 잇지 않으면 이 탭만 그 세션을 알고, CLI(`tell`·`status`)와 재시작 복원은
    /// surface meta 로 찾으므로 그 탭을 세션 없는 탭으로 본다(실측 2026-10-12).
    pub(crate) fn attach_by_input(
        &self,
        host: &impl crate::ports::host::HostCall,
        surface_id: u32,
        input: &str,
    ) -> Result<String, String> {
        let listed = self.sup.list().map(|(rows, _)| rows);
        session_resolve::attach(host, listed, surface_id, input)
    }

    /// 세션을 지운 뒤 목록을 **새로 받는다**.
    ///
    /// `Convo.attachable` 은 폼을 그릴 때 담아 둔 값이다. 지우고 그대로
    /// 다시 그리면 **지운 세션이 목록에 남는다** — 사용자는 삭제가 안 된
    /// 줄 안다.
    pub(crate) fn refresh_attachable(&mut self, surface_id: u32) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let (known, attachable) = session_resolve::form_choices(&host, &self.sup);
        if let Some(c) = self.convos.get_mut(&surface_id) {
            c.known_agents = known;
            c.attachable = attachable;
            c.tab_id = Some(surface_id);
        }
    }

    /// 탭이 닫혔다 — 기다리던 호출에 답하고 그 탭의 세션을 재운다(`destroy_surface` 의 몸통).
    pub(crate) fn close_surface(&mut self, surface_id: u32) {
        self.tells.drop_surface(surface_id);
        skills::forget(surface_id);
        self.clears.drop_surface(surface_id);
        // 이 탭이 쥐고 있던 세션 — 없으면(대시보드 등) 할 일이 없다.
        let Some(id) = self
            .convos
            .get(&surface_id)
            .and_then(|c| c.session_id.clone())
        else {
            self.convos.remove(&surface_id);
            return;
        };
        // 이 탭이 디스크로 옮겨 둔 대화도 함께 거둔다 — 탭이 닫히면 메모리의 대화도 사라진다.
        turn_archive::remove_for(&id);
        send_queue::remove_for(&id);
        view_state::remove_for(&id);
        match self.sup.stop(&id) {
            Ok(()) => tracing::info!("[agent_session] 탭 닫힘 — 세션 {id} 를 재웠다"),
            Err(e) => tracing::warn!("[agent_session] 탭 닫힘 — 세션 {id} 를 못 재웠다: {e}"),
        }
        self.convos.remove(&surface_id);
    }
}
