//! IPC 메서드의 몸통 — `handle_ipc_method` 가 이름으로 골라 부른다.
//!
//! 처리기는 plugin 상태를 갖지 않는다. 호스트와 데몬을 인자로 받으므로(`HostCall` ·
//! `Supervisor`) 시험이 가짜를 넣어 잰다.
//!
//! IPC 오류(`IpcMethodError`)로 옮기는 곳은 이 파일과 조립 루트뿐이다. 로직은 [`AppError`] 를
//! 돌려주고, 여기서 데몬 오류의 종류에 맞는 코드를 고른다.

use serde_json::{Value, json};
use tasty_plugin_sdk::IpcMethodError;

use crate::application::app_error::AppError;
use crate::application::request::{require_surface, session_for};
use crate::application::turn_done::BRIEF_DONE;
use crate::ports::host::SendHost;
use crate::ports::supervisor::Supervisor;

/// **전제 미충족** — 데몬 부재·소켓 사망. 재시도해도 같다.
///
/// 그쪽 CLI 의 종료 코드 2 와 같은 축이다. JSON-RPC 구현 정의 범위
/// (-32000..-32099)에서 골랐다.
pub(crate) const CODE_PRECONDITION: i32 = -32001;
/// 프로토콜 불일치 — 프레이밍·JSON 모양이 어긋났다.
pub(crate) const CODE_PROTOCOL: i32 = -32002;
/// **데몬이 거절했다** — 말은 통했고 요청이 안 받아들여졌다.
///
/// ⚠️ `CODE_PROTOCOL` 과 가른다. 저것은 *"말이 안 통한다"* 라 연결을
/// 다시 맺어야 하고, 이것은 **요청을 고쳐야** 한다(없는 세션·없는 agent·
/// 맞지 않는 상태). 한 코드로 합치면 소비자가 재시도 여부를 못 정한다.
pub(crate) const CODE_DAEMON_REFUSED: i32 = -32003;

/// 데몬 오류 `kind` 를 우리 코드로 옮긴다.
///
/// **`reason` 문자열로 분기하지 않는다** — 그것은 사람용이라 바뀐다.
/// supervisor 가 `kind` 를 구조로 주는 이유가 이것이다(그쪽 spec §4).
///
/// **모르는 `kind` 는 거절로 본다** — 결함(`failed`)으로 올리면 없는
/// 회귀를 세고, 전제 미충족으로 내리면 진짜 결함을 놓친다. 그 사이가
/// *"우리가 모르는 거절"* 이다.
/// 데몬 오류를 **한 자리에서** IPC 오류로 옮긴다.
///
/// 종전에는 이 갈래가 네 곳에 흩어져 있었고, `Daemon` 을 더하자 그
/// 넷이 전부 컴파일 오류가 됐다 — 갈래가 늘 때마다 같은 일이 난다.
pub(crate) fn to_ipc_error(e: crate::ports::supervisor::ClientError) -> IpcMethodError {
    // **결함만 경고로 올린다.** 전제 미충족(데몬 없음·CLI 없음)까지
    // 경고로 찍으면 로그가 환경 문제로 덮여 진짜 회귀가 묻힌다.
    if e.is_product_failure() {
        tracing::warn!("supervisor 제품 결함: {e}");
    }
    match e {
        // 전제 미충족을 결함으로 올리지 않는다 — 데몬이 없는 것은
        // 환경이지 버그가 아니며, 섞으면 사용자가 무한 재시도한다.
        crate::ports::supervisor::ClientError::Unavailable(m) => {
            IpcMethodError::with_code(m, CODE_PRECONDITION)
        }
        // 데몬이 묶였거나 답이 늦은 것도 환경이다 — 결함으로 안 올린다.
        e @ crate::ports::supervisor::ClientError::TimedOut(_) => {
            IpcMethodError::with_code(e.to_string(), CODE_PRECONDITION)
        }
        crate::ports::supervisor::ClientError::Daemon { kind, detail } => {
            IpcMethodError::with_code(detail, code_for_daemon_kind(&kind))
        }
    }
}

fn code_for_daemon_kind(kind: &str) -> i32 {
    match kind {
        // 환경 문제 — 재시도해도 같다. 사용자가 고칠 것이 있다.
        "precondition" => CODE_PRECONDITION,
        // 제품 결함 — 회귀로 집계된다.
        "failed" => CODE_PROTOCOL,
        // 요청을 고치면 된다(없는 세션·없는 agent·맞지 않는 상태·잘못된 요청).
        _ => CODE_DAEMON_REFUSED,
    }
}

/// 로직의 오류를 IPC 오류로 옮긴다 — 데몬 오류는 종류에 맞는 코드로([`to_ipc_error`]), 나머지는
/// 일반 서버 오류(-32000)로.
impl From<AppError> for IpcMethodError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::Message(m) => IpcMethodError::new(m),
            AppError::Supervisor(e) => to_ipc_error(e),
        }
    }
}

/// 답을 미루는 메서드 — 미룰 수 있었으면 답 함수를 로직에 넘기고 곧바로 돌아온다. 못 미뤘으면 그 사실로 답한다.
pub(crate) fn answer_later(
    deferred: Option<tasty_plugin_sdk::DeferredReply>,
    start: impl FnOnce(crate::application::tell::Reply),
) -> Result<Value, IpcMethodError> {
    let Some(reply) = deferred else {
        return Err(IpcMethodError::new(crate::domain::i18n::t(
            "sup.err.worker_spawn",
        )));
    };
    start(defer_reply(reply));
    Ok(Value::Null)
}

/// 답을 미루는 메서드 — 턴이 끝나야 답이 있어 worker 를 막지 않는다(`tell` 모듈).
pub(crate) const DEFERRED_METHODS: &[&str] = &[
    "agent_session.tell",
    "agent_session.clear",
    "agent_session.add_dir",
    "agent_session.remove_dir",
];

/// 미룬 답(SDK `DeferredReply`)을 로직이 쓰는 답 함수로 감싼다 — 오류는 보낼 때 IPC 오류로 옮긴다.
pub(crate) fn defer_reply(
    reply: tasty_plugin_sdk::DeferredReply,
) -> crate::application::tell::Reply {
    Box::new(move |r| reply.send(r.map_err(IpcMethodError::from)))
}

/// **어느 세션이 자원을 물고 있고, 어느 것이 재워졌나.**
///
/// # 왜 이 표면인가
///
/// 그쪽 제품의 목적이 유휴 자원 관리인데, 회수는 **화면에 안 보인다**.
/// 사용자는 세션이 재워진 줄 모르고 어느 것이 살아 있는지도 모른다.
///
/// # state 하나로는 셋을 못 가른다
///
/// `stopped` 는 **회수·명시 정지·자식 사망**을 전부 덮는다. 그래서
/// `reclaimed` 를 함께 봐야 *"우리가 재웠다"* 와 *"죽었다"* 가 갈린다.
/// 여기서 그 셋을 **미리 접지 않는다** — 원값을 그대로 내고 분류는
/// 소비자가 한다. 접으면 그쪽이 상태를 늘릴 때 우리가 조용히 틀린다.
///
/// ⚠️ **살아 있는 수를 함께 센다.** 목록만 내면 47건 중 2건만 active 인
/// 상황에서 사람이 그것을 눈으로 세야 한다.
pub(crate) fn handle_list(sup: &impl Supervisor) -> Result<Value, IpcMethodError> {
    match sup.list() {
        Ok((rows, unreadable)) => {
            let live = rows.iter().filter(|r| r.state == "running").count();
            let reclaimed = rows.iter().filter(|r| r.reclaimed).count();
            let sessions: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "state": r.state,
                        "agent_name": r.agent_name,
                        "resumable": r.resumable,
                        "reclaimed": r.reclaimed,
                        // 원본을 함께 낸다 — 그쪽이 필드를 늘리면 우리 칸이
                        // 아니라 **여기서** 보인다. 버리면 늘어난 것을
                        // 아무도 모르고, 그때 이 plugin 을 고칠 계기가 없다.
                        "raw": r.raw(),
                    })
                })
                .collect();
            Ok(json!({
                "store": sup.endpoint(),
                "total": rows.len(),
                "running": live,
                "reclaimed": reclaimed,
                // **못 읽은 행을 숨기지 않는다** — 0 이 아니면 목록이
                // 짧아진 것이고, 조용히 빼면 아무도 모른다.
                "unreadable": unreadable,
                "sessions": sessions,
            }))
        }
        Err(e) => Err(to_ipc_error(e)),
    }
}

/// **세션을 surface 에 잇는다.**
///
/// 이으면 그 탭이 어느 supervisor 세션인지 양방향으로 찾을 수 있다 —
/// 화면에서 세션을 짚고, 세션에서 화면을 짚는다.
pub(crate) fn handle_link(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let surface_id = require_surface(params)?;
    let session = params
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            IpcMethodError::new(crate::domain::i18n::t("sup.err.need_session_id_full"))
        })?;

    // 존재하지 않는 세션에 이으면 조용히 끊긴 짝이 생긴다 — 먼저 확인한다.
    // 앞부분만 적었으면 하나뿐일 때 전체 id 로 푼다(탭 만들기와 같은 판정).
    let (rows, _) = sup.list().map_err(to_ipc_error)?;
    let session = crate::application::session_resolve::resolve(&rows, session)
        .map_err(IpcMethodError::new)?;
    let session = session.as_str();

    crate::application::surface_link::link(host, surface_id, session)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({ "surface_id": surface_id, "session_id": session }))
}

/// **이 surface 는 어느 세션인가.**
pub(crate) fn handle_of_surface(
    host: &impl crate::ports::host::HostCall,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let surface_id = require_surface(params)?;
    match crate::application::surface_link::session_of_surface(host, surface_id) {
        Ok(session) => Ok(json!({ "surface_id": surface_id, "session_id": session })),
        // 안 이어진 것은 **결함이 아니다** — 대부분의 surface 가 그렇다.
        Err(e) => Err(IpcMethodError::new(e.to_string())),
    }
}

/// **짝을 끊는다** — 세션도 surface 도 안 건드린다.
///
/// tasty 의 `terminal.release` 와 같은 자리다: 관리를 그만두는 길이
/// 파괴뿐이면 그 탭의 사용자 작업이 함께 죽는다.
pub(crate) fn handle_unlink(
    host: &impl crate::ports::host::HostCall,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let surface_id = require_surface(params)?;
    crate::application::surface_link::unlink(host, surface_id)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({ "surface_id": surface_id, "unlinked": true }))
}

/// **같은 자리를 쓰는 다른 세션들** — tasty `terminal.parent` 의 이쪽 판본.
///
/// 그쪽은 *"누가 띄웠나"* 를 묻지만 우리 세션에는 계층이 없다. 대신 이미
/// 있는 짝 정보로 답할 수 있는 물음으로 바꿨다 — *"이 세션과 같은
/// surface 를 쓰는 다른 세션"*. 한 탭에서 세션을 갈아 끼운 이력이 보인다.
///
/// 빈 결과가 정상이다 — 대부분의 세션은 형제가 없다.
pub(crate) fn handle_siblings(
    host: &impl crate::ports::host::HostCall,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let session = params
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.err.need_session_id")))?;
    let rows = crate::application::surface_link::siblings_of_session(host, session)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({
        "session_id": session,
        "total": rows.len(),
        "siblings": rows
            .iter()
            .map(|(sid, s)| json!({ "surface_id": sid, "session_id": s }))
            .collect::<Vec<_>>(),
    }))
}

/// **이어진 짝 전부.**
pub(crate) fn handle_links(
    host: &impl crate::ports::host::HostCall,
) -> Result<Value, IpcMethodError> {
    let (pairs, unreadable) = crate::application::surface_link::surfaces_by_session(host)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    // 자리를 못 읽어도 목록은 낸다 — 이름이 없는 것이 목록을 막을 이유는
    // 아니다. 그때는 종전처럼 숫자만 나간다.
    let places = crate::application::surface_link::places_by_surface(host).unwrap_or_default();
    let links: Vec<Value> = pairs
        .iter()
        .map(|(sid, sess)| {
            let p = places.get(sid).cloned().unwrap_or_default();
            json!({
                "surface_id": sid,
                "session_id": sess,
                "tab_name": p.tab_name,
                "pane_id": p.pane_id,
                "workspace_id": p.workspace_id,
                "workspace_name": p.workspace_name,
            })
        })
        .collect();
    Ok(json!({
        "total": links.len(),
        // 못 읽은 항목을 숨기지 않는다 — 0 이 아니면 목록이 짧아진 것이다.
        "unreadable": unreadable,
        "links": links,
    }))
}

// ────────────────────────────────────────────────────────────────────────
// surface 를 **기준으로** 세션에 작용한다 — 이 plugin 의 값이 여기서 난다.
//
// 세션 id 를 손으로 옮겨 적을 필요가 없다: 지금 보고 있는 탭이 어느 세션인지
// 이미 `surface.meta` 가 안다. 아래 셋은 전부 *"이 surface 의 세션"* 에 건다.
// ────────────────────────────────────────────────────────────────────────

/// 이 surface 의 세션이 **지금 어떤 상태인가**.
///
/// 회수(`reclaimed`)는 화면에 안 드러나는 상태다 — 사용자는 세션이 재워진
/// 줄 모른 채 답을 기다린다. 그것을 묻는 자리다.
/// 작업 그래프가 읽는 턴 상태 — 데몬 상태 행에서 정한다.
///
/// | 값 | 조건 | 완료 전략 |
/// |---|---|---|
/// | `needs_input` | 행에 물음이 있다(protocol 13) — 그동안 `turn_active` 는 참이다 | 끝(사람이 답하면 이어진다) |
/// | `running` | `turn_active` | 기다림 |
/// | `exited` | 끝났거나(`finished`) 실패했거나(`failed`) 회수가 아닌 이유로 멈췄다(`stopped`) | 실패 |
/// | `idle` | 그 밖 — 회수돼 쉬는 세션 포함(다음 말에 되살아난다) | 끝 |
///
/// 터미널 claude 의 `tell-wait` 가 hook 이 쓴 `idle`·`needs_input`·`exited` 를 보는 것과 같은 축이다 —
/// 대화 탭은 hook 없이 데몬 행만으로 낸다.
pub(crate) fn turn_state(st: &Value) -> &'static str {
    if st.get("needs_input").is_some_and(|n| !n.is_null()) {
        return "needs_input";
    }
    if st.get("turn_active").and_then(Value::as_bool) == Some(true) {
        return "running";
    }
    let reclaimed = st.get("reclaimed").and_then(Value::as_bool) == Some(true);
    match st.get("state").and_then(Value::as_str) {
        Some("finished" | "failed") => "exited",
        Some("stopped") if !reclaimed => "exited",
        _ => "idle",
    }
}

pub(crate) fn handle_status(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let (surface_id, session) = session_for(host, params)?;
    let st = sup
        .status(&session)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    // 화면 표시도 함께 낸다 — **세션 상태와 표시가 어긋났는지**를 한 번에
    // 본다. 둘을 따로 물으면 어긋남이 눈에 안 띈다.
    let attention = crate::application::surface_link::attention_of(host, surface_id)
        .ok()
        .flatten();
    // 턴의 상태를 **최상위 칸**으로도 낸다 — 작업 그래프의 완료 판정(`supervisor-tell-wait`)은
    // 응답의 최상위 칸만 읽는다([`turn_state`]).
    let turn = turn_state(&st);
    Ok(json!({
        "surface_id": surface_id,
        "session_id": session,
        "status": st,
        "attention": attention,
        "turn": turn,
    }))
}

/// 이 surface 의 세션을 **재운다** — 좌표는 남는다.
///
/// 유휴 임계(기본 600초)를 기다리지 않고 **지금** 자원을 놓게 한다.
/// 회수는 파괴가 아니다: 다음 `tell` 이 자동으로 되살린다.
pub(crate) fn handle_sleep(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let (surface_id, session) = session_for(host, params)?;
    sup.stop(&session)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    // 화면에 드러낸다. 실패해도 재운 것 자체는 성공이므로 **사유만 싣고
    // 계속 간다** — 표시를 못 켰다고 회수를 되돌리면 자원이 다시 물린다.
    let marked = crate::application::surface_link::mark_reclaimed(host, surface_id);
    Ok(json!({
        "surface_id": surface_id,
        "session_id": session,
        "slept": true,
        "marked": marked.is_ok(),
        "mark_error": marked.err().map(|e| e.to_string()),
        "note": crate::domain::i18n::t("sup.note.coordinate_kept"),
    }))
}

/// 이 surface 의 세션에 **턴을 넣고 답을 받는다**(`crate::application::tell::send_now`).
pub(crate) fn handle_tell(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    Ok(crate::application::tell::send_now(host, sup, params)?)
}

/// 이어진 **모든** surface 의 표시를 실제 세션 상태에 맞춘다.
///
/// # 왜 따로 필요한가
///
/// `sleep`·`tell` 은 **이 plugin 을 거친** 변화만 안다. 그런데 회수의
/// 주인공은 **데몬의 유휴 스윕**이다 — 600초가 지나 스스로 재운 것은
/// 아무도 화면에 안 알린다. 그 간극을 메우는 자리다.
///
/// 표시를 켜는 것만큼 **거두는 것**도 한다. 한쪽만 하면 하이라이트가
/// 쌓이기만 하고, 쌓인 표시는 아무것도 뜻하지 않게 된다.
///
/// 회수 표시는 **잠들 때 한 번만** 켠다. `reclaim_marked` 는 이번 잠에 이미 표시를 켠 탭이다 — 매 주기 다시
/// 켜면, 사용자가 그 탭을 읽어 표시를 지워도 30초 안에 다시 떠서 "읽지 않음" 이 사라지지 않았다
/// (실측 2026-10-21: 멈춘 세션의 탭 7개에 `needs_input` 이 주기마다 다시 켜졌다). 깨어나면 지운다.
pub(crate) fn handle_sync(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    reclaim_marked: &mut std::collections::BTreeSet<u32>,
) -> Result<Value, IpcMethodError> {
    let (pairs, unreadable) = crate::application::surface_link::surfaces_by_session(host)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    let mut marked = Vec::new();
    let mut cleared = Vec::new();
    let mut failed = Vec::new();

    for (surface_id, session) in pairs {
        // 상태를 못 물어본 세션은 **건너뛴다** — 모르는 것을 표시로
        // 단정하면 거짓이 화면에 남는다.
        let st = match sup.status(&session) {
            Ok(v) => v,
            Err(e) => {
                failed.push(json!({ "surface_id": surface_id, "reason": e.to_string() }));
                continue;
            }
        };
        let asleep = st
            .get("reclaimed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || st.get("state").and_then(Value::as_str) == Some("stopped");

        // 깨어 있으면 **회수 표시만** 거둔다 — 턴이 끝나 켠 완료 표시는 사용자가 볼 때까지 남는다.
        // 물음을 기다리는 세션(protocol 13 `needs_input`)의 `needs_input` 표시는 회수 표시가 아니다.
        if !asleep {
            reclaim_marked.remove(&surface_id);
        }
        let r = if asleep && reclaim_marked.contains(&surface_id) {
            // 이번 잠에 이미 켰다 — 사용자가 지웠으면 그대로 둔다.
            continue;
        } else if asleep {
            crate::application::surface_link::mark_reclaimed(host, surface_id).map(|()| {
                reclaim_marked.insert(surface_id);
                true
            })
        } else if st.get("needs_input").is_some() {
            Ok(false)
        } else {
            crate::application::surface_link::clear_reclaimed_mark(host, surface_id)
        };
        match r {
            Ok(_) if asleep => marked.push(surface_id),
            Ok(true) => cleared.push(surface_id),
            Ok(false) => {}
            Err(e) => failed.push(json!({ "surface_id": surface_id, "reason": e.to_string() })),
        }
    }

    Ok(json!({
        "marked": marked,
        "cleared": cleared,
        "failed": failed,
        "unreadable_surfaces": unreadable,
    }))
}

/// 이 surface 세션의 **도는 턴만 취소한다** — 세션은 살려 둔다.
///
/// `sleep` 과 다르다: 저쪽은 프로세스를 놓고, 이쪽은 **답을 그만 기다린다**.
/// 긴 답이 잘못 가고 있을 때 쓰는 자리다.
pub(crate) fn handle_abort(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let (surface_id, session) = session_for(host, params)?;
    sup.abort(&session)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({ "surface_id": surface_id, "session_id": session, "aborted": true }))
}

/// 세션 레코드를 **지운다 — 되돌릴 수 없다**(2026-09-28).
///
/// # 왜 단추가 없나
///
/// 좌표가 사라져 **그 대화로 돌아갈 길이 없어진다**. 실수로 누를 수
/// 있는 자리에 두지 않는다 — 부르려면 의도가 분명해야 한다.
///
/// # 왜 `session_for` 를 안 쓰나
///
/// 다른 명령은 **surface 에 물린 세션**을 대상으로 하지만, 지우는 것은
/// 대개 **탭에 없는** 세션이다(탭이 있으면 먼저 닫는다). surface 를
/// 요구하면 정작 정리해야 할 것을 못 지운다.
///
/// 상대는 살아 있는 세션을 거절한다 — 먼저 `agent_session.stop` 한다.
pub(crate) fn handle_delete(
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let id = params
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.err.need_session_id")))?;
    sup.delete(id)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({ "session_id": id, "deleted": true }))
}

/// 세션을 새로 띄우고 **이 surface 에 바로 잇는다**.
///
/// # 왜 한 번에 하나
///
/// `spawn` 뒤에 `link` 를 따로 치게 하면 그 사이에 실패할 자리가 생기고,
/// 그때 세션은 떠 있는데 아무 탭도 그것을 모른다 — **자원만 물린 유령**이다.
///
/// 이미 이어져 있으면 **거절한다**. surface 는 세션 하나만 들 수 있어
/// (`surface.meta` 가 1:1 이다) 덮어쓰면 앞 세션이 조용히 미아가 된다.
pub(crate) fn handle_start(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let surface_id = require_surface(params)?;
    let agent = params
        .get("agent")
        .and_then(Value::as_str)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.err.need_agent")))?;

    if let Ok(existing) = crate::application::surface_link::session_of_surface(host, surface_id) {
        return Err(IpcMethodError::new(crate::domain::i18n::tn(
            "sup.err.surface_has_session",
            &[&surface_id.to_string(), existing.as_str()],
        )));
    }

    let cwd = params.get("cwd").and_then(Value::as_str);
    // 복제(`--copy-of`)는 호출부가 원본 설정을 평평한 칸으로 얹어 둔다(`crate::application::copy_from::apply_to`).
    let add_dirs = crate::application::copy_from::add_dirs_of(params);
    let roles = crate::adapters::driven::store::roles::names_of(params);
    let sess = sup
        .spawn(&crate::ports::supervisor::SpawnOpts {
            agent,
            cwd,
            model: params.get("model").and_then(Value::as_str),
            add_dirs: &add_dirs,
            roles: &roles,
            permission_mode: params.get("permission_mode").and_then(Value::as_str),
            // 이 surface 의 대화 탭이 물음에 답한다(`prompts`) — 데몬이 protocol 13 일 때만 실린다.
            permission_prompts: true,
            ..Default::default()
        })
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    let id = sess.get("id").and_then(Value::as_str).ok_or_else(|| {
        IpcMethodError::new(crate::domain::i18n::t1(
            "sup.err.spawned_without_id",
            &sess.to_string(),
        ))
    })?;

    // 못 이으면 **띄운 것을 되돌린다** — 이어지지 않은 세션은 아무도 못 찾고
    // 자원만 문다.
    if let Err(e) = crate::application::surface_link::link(host, surface_id, id) {
        // 되돌리기의 실패는 삼킨다 — 이미 오류를 내는 길이고, 여기서 또
        // 실패하면 원래 사유(잇지 못했다)가 덜 중요한 것에 가린다.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "A rollback failure would hide the link error that is being returned."
        )]
        let _ = sup.stop(id); // 되돌리기 실패는 삼킨다(위 주석).
        return Err(IpcMethodError::new(crate::domain::i18n::tn(
            "sup.err.spawn_link_rolled_back",
            &[id, &e.to_string()],
        )));
    }
    Ok(json!({ "surface_id": surface_id, "session_id": id, "session": sess }))
}

/// `supervisor start` — `copy_of` 가 오면 원본 탭의 설정을 얹어 띄운다(`copy_from`).
pub(crate) fn handle_start_or_copy(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let Some(src) = crate::application::copy_from::source_of(params) else {
        return handle_start(host, sup, params);
    };
    match crate::application::copy_from::read_source(host, sup, src) {
        Ok(c) => handle_start(
            host,
            sup,
            &crate::application::copy_from::apply_to(params, &c),
        ),
        Err(why) => Err(IpcMethodError::new(why)),
    }
}

/// 프로필 이름 인자 — 비었거나 파일 이름이 될 수 없으면 거절한다.
fn profile_name(params: &Value) -> Result<&str, IpcMethodError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.profile.need_name")))?;
    if !crate::adapters::driven::store::agents::valid_name(name) {
        return Err(IpcMethodError::new(crate::domain::i18n::t1(
            "sup.profile.bad_name",
            name,
        )));
    }
    Ok(name)
}

/// 프로필 폴더 — plugin 데이터 폴더가 없으면 쓰기를 거절한다.
fn profile_dir() -> Result<std::path::PathBuf, IpcMethodError> {
    crate::adapters::driven::store::agents::default_dir()
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.profile.no_dir")))
}

fn profile_json(p: &crate::adapters::driven::store::agents::AgentProfile) -> Value {
    let mut v = p.to_json();
    v["name"] = Value::String(p.name.clone());
    v
}

/// 프로필 명부 — 폴더가 비었으면 기본 프로필 하나가 온다(`builtin: true`).
pub(crate) fn handle_profile_list(sup: &impl Supervisor) -> Result<Value, IpcMethodError> {
    let all = sup.agent_list().map_err(to_ipc_error)?;
    let dir = crate::adapters::driven::store::agents::default_dir();
    let saved = |name: &str| {
        dir.as_ref()
            .is_some_and(|d| d.join(format!("{name}.json")).is_file())
    };
    let profiles: Vec<Value> = all
        .iter()
        .map(|p| {
            let mut v = profile_json(p);
            v["builtin"] = Value::Bool(!saved(&p.name));
            v
        })
        .collect();
    Ok(json!({
        "dir": dir.map(|d| d.display().to_string()),
        "profiles": profiles,
    }))
}

pub(crate) fn handle_profile_show(
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let name = profile_name(params)?;
    sup.agent_list()
        .map_err(to_ipc_error)?
        .iter()
        .find(|p| p.name == name)
        .map(profile_json)
        .ok_or_else(|| {
            IpcMethodError::new(crate::domain::i18n::t1("sup.err.unknown_profile", name))
        })
}

/// 프로필을 통째로 쓴다 — `json` 은 프로필 파일 본문(칸은 [`agents`] 머리글)이다.
pub(crate) fn handle_profile_set(params: &Value) -> Result<Value, IpcMethodError> {
    let name = profile_name(params)?;
    let body = params
        .get("json")
        .and_then(Value::as_str)
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.profile.need_json")))?;
    let v: Value = serde_json::from_str(body).map_err(|e| {
        IpcMethodError::new(crate::domain::i18n::t1(
            "sup.profile.bad_json",
            &e.to_string(),
        ))
    })?;
    let p = crate::adapters::driven::store::agents::AgentProfile::from_json(name, &v)
        .map_err(IpcMethodError::new)?;
    if !crate::adapters::driven::store::agents::supported(&p.kind) {
        return Err(IpcMethodError::new(crate::domain::i18n::t1(
            "sup.err.kind_unsupported",
            &p.kind,
        )));
    }
    if let Some(m) = p.permission_mode.as_deref()
        && !crate::adapters::driven::local::PERMISSION_MODES.contains(&m)
    {
        return Err(IpcMethodError::new(crate::domain::i18n::t1(
            "sup.err.bad_permission_mode",
            m,
        )));
    }
    let path = crate::adapters::driven::store::agents::save(&profile_dir()?, &p)
        .map_err(IpcMethodError::new)?;
    Ok(json!({ "saved": path.display().to_string(), "profile": profile_json(&p) }))
}

pub(crate) fn handle_profile_remove(params: &Value) -> Result<Value, IpcMethodError> {
    let name = profile_name(params)?;
    crate::adapters::driven::store::agents::remove(&profile_dir()?, name)
        .map_err(IpcMethodError::new)?;
    Ok(json!({ "removed": name }))
}

fn roles_file() -> Result<std::path::PathBuf, IpcMethodError> {
    crate::adapters::driven::store::roles::default_file()
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.profile.no_dir")))
}

fn arg<'a>(params: &'a Value, k: &str, need: &'static str) -> Result<&'a str, IpcMethodError> {
    params
        .get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t(need)))
}

/// 역할 폴더 명부 — 이름 · 경로 · 지침(`CLAUDE.md`) 유무.
pub(crate) fn handle_role_list() -> Result<Value, IpcMethodError> {
    let file = roles_file()?;
    let roles: Vec<Value> = crate::adapters::driven::store::roles::load(Some(&file))
        .iter()
        .map(crate::adapters::driven::store::roles::Role::to_json)
        .collect();
    Ok(json!({ "file": file.display().to_string(), "roles": roles }))
}

/// 역할 폴더를 더하거나 바꾼다.
pub(crate) fn handle_role_add(params: &Value) -> Result<Value, IpcMethodError> {
    let name = profile_name(params)?;
    let path = arg(params, "path", "sup.role.need_path")?;
    let r = crate::adapters::driven::store::roles::add(&roles_file()?, name, path)
        .map_err(IpcMethodError::new)?;
    Ok(json!({ "added": r.to_json() }))
}

/// 폴더 아래 `CLAUDE.md` 를 가진 폴더를 모두 역할로 더한다.
pub(crate) fn handle_role_scan(params: &Value) -> Result<Value, IpcMethodError> {
    let path = arg(params, "path", "sup.role.need_path")?;
    let added = crate::adapters::driven::store::roles::scan(&roles_file()?, path)
        .map_err(IpcMethodError::new)?;
    Ok(
        json!({ "added": added.iter().map(crate::adapters::driven::store::roles::Role::to_json).collect::<Vec<_>>() }),
    )
}

pub(crate) fn handle_role_remove(params: &Value) -> Result<Value, IpcMethodError> {
    let name = profile_name(params)?;
    crate::adapters::driven::store::roles::remove(&roles_file()?, name)
        .map_err(IpcMethodError::new)?;
    Ok(json!({ "removed": name }))
}

/// 같은 agent 로 띄운 세션 **전부**에 같은 입력을 보낸다.
///
/// tasty 의 `terminal.broadcast` 는 *부모의 자식들*을 고르지만 우리 세션에는
/// 부모가 없다 — **agent 이름으로 고른다**(같은 프로필로 띄운 것이 한 묶음).
/// `agent` 를 생략하면 살아 있는 전부다.
///
/// **일부 실패가 전체를 취소하지 않는다** — `sent`·`failed` 를 나눠 낸다.
pub(crate) fn handle_broadcast(
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    // **host 를 안 받는다** — 대상이 agent 이름이라 어느 탭에서 불러도
    // 같다. 인터페이스로 묶으면서 **안 쓰는 인자가 드러났다**(종전에는
    // `let _ = host` 로 받아만 두고 있었다).
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| IpcMethodError::new(crate::domain::i18n::t("sup.err.need_text")))?;
    let agent = params.get("agent").and_then(Value::as_str);
    let out = sup
        .broadcast(text, agent)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(out)
}

/// 이 surface 세션의 턴을 **지켜본다** — 이벤트를 모아서 낸다.
///
/// `tell` 은 답만 주고 중간을 안 보여 준다. 다른 곳에서 들어간 턴이 지금
/// 무엇을 하고 있는지는 이 길로만 보인다.
///
/// 이 plugin 의 `tell` 이 넣은 턴도 지켜볼 수 있다 — `tell` 은 답을 미루고 곧바로
/// 루프를 비운다(실측 2026-10-13: 20 초 턴의 `tell` 2 초 뒤 `watch` 가 `started` 부터
/// 이벤트 12 개를 받았다). 반대로 **`watch` 는 루프를 점유한다** — 도는 동안
/// (최대 `seconds`) 이 plugin 의 다른 호출은 줄을 선다(같은 실측: `links` 가 9 초 기다렸다).
///
/// 회수는 이 채널에 **안 온다**(실측) — 그것은 `sync` 몫이다.
pub(crate) fn handle_watch(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let (surface_id, session) = session_for(host, params)?;
    // 한도가 없으면 plugin 호출이 영영 안 돌아온다. 기본은 한 턴이다.
    let max = params
        .get("max_events")
        .and_then(Value::as_u64)
        .map_or(200, |v| v.clamp(1, 2000) as usize);
    let until_finished = params
        .get("until_finished")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // 호스트 ping(15초)보다 **넉넉히 짧게**. 길게 잡으면 그 동안 이 plugin 의
    // 다른 명령이 전부 막히고, 60초를 넘기면 호스트가 죽인다(실측).
    let budget = std::time::Duration::from_secs(
        params
            .get("seconds")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .clamp(1, 10),
    );
    let (events, finished) = sup
        .watch(&session, max, until_finished, budget)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({
        "surface_id": surface_id,
        "session_id": session,
        "count": events.len(),
        // 셋을 **가른다** — 턴이 끝난 것 · 한도에 걸린 것 · 시간이 다 된 것.
        // 뭉뚱그리면 잘린 목록을 완결로 읽는다.
        "finished": finished,
        "hit_limit": events.len() >= max,
        "events": events,
    }))
}

/// 이 surface 세션이 **최근에 무슨 말을 했나** — 구독 전 것도 보인다.
///
/// `watch` 와 다른 축이다: 저쪽은 **지금부터** 흐르는 것을 보고, 이것은
/// **이미 지나간 것**을 당긴다. 탭을 열어 두지 않았어도 답한다.
///
/// 재워진 세션도 답한다 — *"무슨 말을 하다 잠들었나"* 를 볼 수 있다.
pub(crate) fn handle_output(
    host: &impl crate::ports::host::HostCall,
    sup: &impl Supervisor,
    params: &Value,
) -> Result<Value, IpcMethodError> {
    let (surface_id, session) = session_for(host, params)?;
    let lines = params
        .get("lines")
        .and_then(Value::as_u64)
        .map(|v| v.clamp(1, 1000) as usize);
    let tail = sup
        .output(&session, lines)
        .map_err(|e| IpcMethodError::new(e.to_string()))?;
    Ok(json!({ "surface_id": surface_id, "session_id": session, "output": tail }))
}

/// 고른 세션을 지운다 — **하나가 실패해도 나머지를 보낸다**.
///
/// `broadcast` 와 같은 축이다. 중간에 멈추면 사용자는 **어디까지
/// 지워졌는지 모른다** — 되돌릴 수 없는 조작이라 그 모름이 특히 나쁘다.
///
/// 돌려주는 값은 `(성공, 실패)`.
pub(crate) fn delete_sessions(
    sup: &impl crate::ports::supervisor::Supervisor,
    ids: &[String],
) -> (usize, usize) {
    let (mut done, mut failed) = (0usize, 0usize);
    for id in ids {
        match sup.delete(id) {
            Ok(()) => {
                done += 1;
                tracing::info!("[agent_session] 세션 삭제 {id}");
            }
            // 실패를 삼키지 않는다 — 목록에서 사라지지 않은 이유가
            // 로그에만 남아도, 없는 것보다는 갈린다.
            Err(e) => {
                failed += 1;
                tracing::warn!("[agent_session] 세션 {id} 삭제 실패: {e}");
            }
        }
    }
    if ids.len() > 1 {
        tracing::info!("[agent_session] 일괄 삭제 {done}건 성공 · {failed}건 실패");
    }
    (done, failed)
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// IPC 메서드를 이름으로 골라 부른다 — 로직의 오류는 여기서 IPC 오류로 옮긴다.
    ///
    /// 답을 미룬 메서드([`DEFERRED_METHODS`])는 `deferred` 에 답할 권리가 실려 온다.
    pub(crate) fn dispatch_ipc(
        &mut self,
        method: &str,
        params: &Value,
        host: &impl SendHost,
        deferred: Option<tasty_plugin_sdk::DeferredReply>,
    ) -> Result<Value, IpcMethodError> {
        // 데몬 경계를 **한 자리에서** 만든다 — 종전에는 핸들러마다
        // `default_port_file()` 를 다시 불렀다(11군데).
        let sup = self.sup;
        // 세션을 겨눈 명령은 휴식 시계를 민다 — 회수 기한만 늦추고 캐시 시계는 안 건드린다.
        if crate::application::sync_compact::TOUCHING_METHODS.contains(&method)
            && let Ok(sid) = require_surface(params)
            && let Some(c) = self.convos.get_mut(&sid)
        {
            c.sync.touch(std::time::Instant::now());
        }
        match method {
            "agent_session.list" => handle_list(&sup),
            "agent_session.link" => {
                let r = handle_link(host, &sup, params);
                self.reopen_linked(r.is_ok(), params);
                r
            }
            "agent_session.of_surface" => handle_of_surface(host, params),
            "agent_session.unlink" => handle_unlink(host, params),
            "agent_session.siblings" => handle_siblings(host, params),
            "agent_session.links" => handle_links(host),
            // API 트래픽 조절의 한도 · 바이트 · 터널 수.
            "agent_session.traffic" => Ok(crate::adapters::driven::traffic::global().report()),
            "agent_session.status" => handle_status(host, &sup, params),
            "agent_session.sleep" => handle_sleep(host, &sup, params),
            "agent_session.tell" => match deferred {
                Some(reply) => {
                    self.tell_later(params, defer_reply(reply));
                    Ok(Value::Null)
                }
                None => handle_tell(host, &sup, params),
            },
            "agent_session.clear" => answer_later(deferred, |r| self.clear_later(params, r)),
            // 워커가 `/clear` 턴을 끝냈다 — 탭의 대화를 비우고 다시 그린다.
            crate::application::clear::METHOD => {
                let (sid, got) =
                    crate::application::clear::parse_done(params).ok_or_else(|| {
                        IpcMethodError::new(crate::domain::i18n::t("sup.err.need_surface_id"))
                    })?;
                self.clear_done(sid, got);
                Ok(json!({ "surface_id": sid }))
            }
            // 대화 탭의 현행화 → compact(수동). 시작했는지만 답한다 — 끝은 탭의 상태줄이 보인다.
            "agent_session.sync_compact" => Ok(self.sync_by_ipc(params)?),
            // 대화 탭의 체크리스트 게이트를 켜고 끄거나 본다.
            // 워커가 compact 턴을 끝냈다.
            crate::application::sync_compact::METHOD => {
                let (sid, got) =
                    crate::application::sync_compact::parse_done(params).ok_or_else(|| {
                        IpcMethodError::new(crate::domain::i18n::t("sup.err.need_surface_id"))
                    })?;
                self.sync_done(sid, got);
                Ok(json!({ "surface_id": sid }))
            }
            "agent_session.add_dir" => answer_later(deferred, |r| self.dir_later(params, r, false)),
            "agent_session.remove_dir" => {
                answer_later(deferred, |r| self.dir_later(params, r, true))
            }
            "agent_session.lifecycle_busy" => Ok(crate::application::activity::busy_report(
                &self.activity,
                &self.convos,
            )),
            "agent_session.sync" => handle_sync(host, &sup, &mut self.reclaim_marked),
            //  워커가 답을 들고 돌아왔다 — 그 턴의 자리를 채우고 다시 그린다.
            //
            // 이 메서드는 **워커만 부른다**(`self_invoke`). 바깥에 열려
            // 있는 것이 마음에 걸릴 수 있으나, 남이 불러도 하는 일은
            // *"그 자리에 이 텍스트를 넣는다"* 뿐이라 데몬을 건드리지 않는다.
            "agent_session.turn_done" => Ok(self.turn_done(host, params)?),
            // 맥락 워커가 끝났다 — 바쁨을 풀고, 쌓인 첫 메시지를 이 탭의 턴으로 보낸다.
            BRIEF_DONE => Ok(self.brief_done(host, params)?),
            // 워커가 데몬의 `turn_active` 를 들고 돌아왔다(`activity` 모듈).
            crate::application::activity::METHOD => {
                self.activity_polled(host, params);
                Ok(json!({}))
            }
            // 복원된 탭의 이전 턴을 워커가 다 읽었다(`history` 모듈).
            crate::application::history::METHOD => {
                if let Some(sid) =
                    crate::application::history::apply_loaded(&mut self.convos, params)
                    && let Some(c) = self.convos.get(&sid)
                {
                    crate::application::session_surface::push(
                        host,
                        &self.sup,
                        sid,
                        c,
                        self.activity.is_busy(c),
                    );
                }
                Ok(json!({}))
            }
            // 기록 꼬리를 다 읽었다 — 빠진 답을 채웠으면 다시 그린다(`catch_up` 모듈).
            crate::application::catch_up::METHOD => {
                self.caught_up(host, params);
                Ok(json!({}))
            }
            // 세션 구독 워커가 이벤트를 넘겼다(`stream` 모듈).
            crate::application::stream::METHOD => {
                self.after_stream(host, params);
                self.flush_queues();
                Ok(json!({}))
            }
            "agent_session.abort" => handle_abort(host, &sup, params),
            "agent_session.delete" => handle_delete(&sup, params),
            "agent_session.start" => {
                let r = handle_start_or_copy(host, &sup, params);
                self.reopen_linked(r.is_ok(), params);
                r
            }
            "agent_session.broadcast" => handle_broadcast(&sup, params),
            "agent_session.profile_list" => handle_profile_list(&sup),
            "agent_session.profile_show" => handle_profile_show(&sup, params),
            "agent_session.profile_set" => handle_profile_set(params),
            "agent_session.profile_remove" => handle_profile_remove(params),
            "agent_session.role_list" => handle_role_list(),
            "agent_session.role_add" => handle_role_add(params),
            "agent_session.role_scan" => handle_role_scan(params),
            "agent_session.role_remove" => handle_role_remove(params),
            "agent_session.watch" => handle_watch(host, &sup, params),
            "agent_session.output" => handle_output(host, &sup, params),
            // 에이전트 내역(읽기 전용) — 패널과 같은 값을 돌려준다(`crate::adapters::driven::subagents::by_ipc`).
            "agent_session.agent_log" => {
                let sid = require_surface(params)?;
                crate::adapters::driven::subagents::by_ipc(host, &sup, &mut self.convos, sid)
                    .ok_or_else(|| {
                        IpcMethodError::new(crate::domain::i18n::t("sup.err.tab_has_no_session"))
                    })
            }
            // 탭을 다시 그린다 — 자동 sync 와 사용자 호출 둘 다 쓴다.
            "agent_session.repaint" => {
                let sid = require_surface(params)?;
                if let Some(c) = self.convos.get(&sid) {
                    crate::application::session_surface::push(host, &self.sup, sid, c, false);
                }
                Ok(json!({ "surface_id": sid, "repainted": true }))
            }
            // ⚠️ SDK 가 코드를 갖는 생성자를 준다 — `not_found` 는 -32601 이다.
            other => Err(IpcMethodError::not_found(other)),
        }
    }
}

#[cfg(test)]
mod turn_state_tests {
    use super::turn_state;
    use serde_json::json;

    #[test]
    fn the_turn_state_covers_question_run_exit_and_idle() {
        // U-G2
        assert_eq!(
            turn_state(&json!({"turn_active": true, "needs_input": {"request_id": "r"}})),
            "needs_input"
        );
        assert_eq!(
            turn_state(&json!({"turn_active": true, "needs_input": null})),
            "running"
        );
        assert_eq!(
            turn_state(&json!({"state": "stopped", "reclaimed": false})),
            "exited"
        );
        assert_eq!(turn_state(&json!({"state": "failed"})), "exited");
        assert_eq!(turn_state(&json!({"state": "finished"})), "exited");
        assert_eq!(
            turn_state(&json!({"state": "stopped", "reclaimed": true})),
            "idle",
            "회수는 쉬는 것이다"
        );
        assert_eq!(
            turn_state(&json!({"state": "running", "turn_active": false})),
            "idle"
        );
    }

    /// 매니페스트의 완료 전략이 위 값을 끝·실패로 나눈다 — 한쪽만 바뀌면 노드가 안 끝나거나 실패를 성공으로 친다.
    #[test]
    fn the_manifest_strategy_ends_on_question_and_fails_on_exit() {
        let m = include_str!("../../../tasty-plugin.toml");
        let spec = m
            .split("id = \"agent-session-tell-wait\"")
            .nth(1)
            .expect("전략");
        let spec = spec.split("[[contributes.cli]]").next().unwrap_or_default();
        assert!(spec.contains(r#"terminal_states = ["idle", "needs_input"]"#));
        assert!(spec.contains(r#"failure_states = ["exited"]"#));
    }
}

#[cfg(test)]
mod app_error_tests {
    use super::*;

    /// 로직의 오류가 IPC 로 나갈 때 코드가 갈린다 — 데몬 오류는 종류대로, 사유 문장은 일반 서버 오류로.
    /// 문자열로 접어 넘기면 전제 미충족(-32001)과 거절(-32003)이 같은 코드가 되어 소비자가 재시도 여부를 못 정한다.
    #[test]
    fn app_errors_keep_their_daemon_codes_at_the_ipc_edge() {
        let cases = [
            (AppError::new("사유"), -32000, "사유"),
            (
                AppError::from(crate::ports::supervisor::ClientError::Unavailable(
                    "데몬 없음".into(),
                )),
                CODE_PRECONDITION,
                "데몬 없음",
            ),
            (
                AppError::from(crate::ports::supervisor::ClientError::Daemon {
                    kind: "not_found".into(),
                    detail: "없는 세션".into(),
                }),
                CODE_DAEMON_REFUSED,
                "없는 세션",
            ),
            (
                AppError::from(crate::ports::supervisor::ClientError::Daemon {
                    kind: "failed".into(),
                    detail: "결함".into(),
                }),
                CODE_PROTOCOL,
                "결함",
            ),
        ];
        for (app, code, text) in cases {
            let e = IpcMethodError::from(app);
            assert_eq!((e.code, e.message.as_str()), (code, text));
        }
    }
}
