//! `agent_session` surface — **탭 안에서 에이전트와 대화한다**.
//!
//! # 쉘 탭과 무엇이 같고 다른가
//!
//! 같은 것: 탭 하나가 살아 있는 자식 하나를 보고, 입력을 넣으면 답이 온다.
//! 다른 것: 자식이 쉘이 아니라 **stream-json 을 내는 에이전트 CLI** 다 —
//! TTY 가 없어 화면 격자도 커서도 키 입력도 없다(`ps -o tty` 가 `??`).
//! 그래서 80x24 를 흉내내지 않고 **턴 기록**을 쌓아 보여 준다.
//!
//! # 입력이 어떻게 들어오나 — navigation 을 신호로 쓴다
//!
//! webview 는 plugin 에게 직접 이벤트를 못 보낸다. 대신 페이지가 **navigation 을
//! 시도**하면 호스트가 `on_webview_navigation_attempt` 로 알려 준다
//! (`{surface_id, url}`). markdown plugin 이 링크 클릭을 받는 것과 같은 수단이다.
//!
//! 그래서 입력 폼은 `#agent-send:<percent-encoded>:<nonce>` 로 이동한다.
//!
//! nonce 가 필요한 이유: **같은 fragment 를 다시 대입하면 WebView 가 navigation 을
//! 아예 시도하지 않는다** — 같은 말을 두 번 보내면 두 번째가 사라진다
//! (markdown 이 같은 자리에 같은 주석을 달아 두었다).

use crate::domain::convo::Convo;
use crate::ports::host::HostCall;

/// surface 별 문서 세대 — [`push`] 가 문서를 통째로 갈아끼울 때마다 오른다.
///
/// 문서를 다시 실으면 메시지로 고쳐 둔 것(진행 표시 · 바쁨 상태줄)이 처음 모양으로
/// 돌아간다. 고친 쪽(`activity`)은 그 사실을 이 값으로 안다 — 같은 세대면 이미 그린 것을
/// 다시 보내지 않는다. 실을 자리가 열다섯 곳이라 그 자리마다 알리는 대신 여기 하나에 둔다.
static DOC_GENERATION: std::sync::Mutex<Option<std::collections::HashMap<u32, u64>>> =
    std::sync::Mutex::new(None);

/// 이 surface 문서의 세대 — 한 번도 안 실었으면 0.
///
/// 잠금이 poison 돼도(다른 스레드가 쥔 채 죽었다) 값을 되찾아 쓴다 — 표는 정수 몇 개라 반쯤 쓴
/// 상태가 없고, 세대를 잃으면 재적재를 못 알아채 표시가 처음 모양으로 남는다.
pub fn doc_generation(surface_id: u32) -> u64 {
    let g = DOC_GENERATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    g.as_ref()
        .and_then(|m| m.get(&surface_id))
        .copied()
        .unwrap_or(0)
}

fn bump_doc_generation(surface_id: u32) {
    let mut g = DOC_GENERATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *g.get_or_insert_with(Default::default)
        .entry(surface_id)
        .or_insert(0) += 1;
}

/// 이 surface 의 지금 대화를 webview 에 싣는다.
pub fn push<H: HostCall, S: crate::ports::supervisor::Supervisor>(
    host: &H,
    sup: &S,
    surface_id: u32,
    convo: &Convo,
    busy: bool,
) {
    bump_doc_generation(surface_id);
    if let Some(session) = convo.session_id.as_deref() {
        crate::adapters::driven::skills::on_document(
            sup,
            surface_id,
            session,
            convo.agent_kind.as_deref(),
        );
    }
    // 우클릭 처리기는 모든 화면(시작 · 대화)에 붙인다. 문서 끝에 둬 첫 `<script>` 를 재는 시험과 겹치지 않는다.
    let doc = crate::adapters::driving::view::render(convo, busy)
        + &crate::adapters::driving::context_menu::script();
    if let Err(e) = host.call(
        "webview.set_url",
        serde_json::json!({ "surface_id": surface_id, "url": doc }),
    ) {
        tracing::warn!("agent_session surface {surface_id}: webview.set_url 실패: {e}");
    }
}

///  **문서를 다시 싣지 않고 대화만 고친다**(0.1.24).
///
/// # 왜 `push` 로 안 되나
///
/// `push` 는 `webview.set_url` 로 문서를 통째로 갈아끼운다. 답이 올 때마다
/// 부르면 (1) 흰 바탕이 스치고 (2) **사용자가 입력 중이던 텍스트가 사라지고**
/// (3) 스크롤이 위로 튄다. 실사용에서 셋 다 보고됐다(2026-09-30).
///
/// 실측(Swift 프로브): `about:blank` 문서에도 스크립트가 닿고, DOM 을 고쳐도
/// **입력칸 값이 살아남는다**.
///
/// ⚠️ **`push` 를 없애지 않는다.** 첫 적재와 구조가 통째로 바뀌는 전환
/// (시작 폼 → 대화창)은 여전히 문서를 새로 실어야 한다. 이 함수는 **이미
/// 대화창이 떠 있을 때의 증분**만 맡는다.
pub fn patch<H: HostCall>(host: &H, surface_id: u32, msg: &serde_json::Value) {
    if let Err(e) = host.call(
        "webview.post_message",
        serde_json::json!({ "surface_id": surface_id, "data": msg }),
    ) {
        // 실패를 삼키지 않는다 — 화면이 안 바뀌는 이유가 로그에만 남아도 갈린다.
        tracing::warn!("agent_session surface {surface_id}: webview.post_message 실패: {e}");
    }
}

/// 그 턴의 답이 도착했을 때 화면에 반영할 **스크립트**를 만든다.
///
/// 대기 표시를 지우고 답을 그 자리에 넣는다 — `finish_turn` 이 모델에 한 것을
/// DOM 에도 그대로 한다.
///
///  **자리를 index 로 찾는다.** 마지막 턴을 고치면 여러 개를 연속으로
/// 보냈을 때 **엉뚱한 질문에 답이 붙는다**.
pub fn finish_msg(at: usize, got: &Result<String, String>) -> serde_json::Value {
    let (cls, body) = match got {
        Ok(t) => ("got", t.as_str()),
        Err(e) => ("err", e.as_str()),
    };
    settle_turn_msg(at, cls, body)
}

/// 사용자가 멈춘 턴을 그 자리에 닫는다 — 실패 색이 아니라 흐린 "중지했다".
pub fn stopped_msg(at: usize) -> serde_json::Value {
    settle_turn_msg(
        at,
        "got stopped",
        &crate::domain::i18n::t("sup.chat.stopped"),
    )
}

/// 턴 하나를 닫는 스크립트 — 답 · 실패 · 중지가 같은 자리 찾기를 쓴다.
fn settle_turn_msg(at: usize, cls: &str, body: &str) -> serde_json::Value {
    serde_json::json!({ "type": "turn.settle", "at": at, "cls": cls, "text": body })
}

/// 상한을 넘긴 턴의 대기 문구를 바꾸는 스크립트 — 점 셋은 그대로 돈다.
pub fn overdue_msg(at: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "turn.overdue",
        "at": at,
        "text": crate::domain::i18n::t("sup.chat.overdue") + "…",
    })
}
