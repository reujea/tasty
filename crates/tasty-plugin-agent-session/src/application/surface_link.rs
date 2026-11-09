//! **세션 ↔ surface 잇기** — plugin 이 양쪽을 안다.
//!
//! # 왜 plugin 이 드는가
//!
//! supervisor 는 화면을 모르고(`Transport::Stream` 은 셀 그리드를 안 잡는다),
//! tasty 는 supervisor 세션을 모른다. 그 둘을 아는 자리는 **이 plugin 뿐**이다.
//!
//! - supervisor 에 `surface_id` 칸을 더하지 **않는다** — 그러면 그쪽 도메인이
//!   tasty 개념을 끌어안고, 화면 없는 다른 클라이언트에게 무의미한 칸이 생긴다.
//! - tasty 본체도 고치지 **않는다** — `surface.meta` 는 이미 있는 표면이다.
//!
//! # 선례
//!
//! `tasty-plugin-agent-stream` 이 같은 수단으로 surface → claude 세션 id 를
//! 푼다(`resolve.rs`). 거기서 배운 두 가지를 그대로 따른다:
//!
//! - **meta 가 없으면 거부한다.** 모르면서 등록을 받아주면 호출자는 붙었다고
//!   믿은 채 아무 것도 못 받는다.
//! - **`surface.locate` 는 없는 대상에 `{exists:false}` 로 답하지 않고 요청을
//!   거절한다**(그쪽 실측 2026-09-05). 그래서 생존 판정은 오류 유무로 읽는다.

use serde_json::{Value, json};

use crate::ports::host::HostCall;

/// surface meta 에 심는 키 — **이 plugin 의 소유**다.
///
/// claude plugin 의 `claude-session-id` 와 **다른 키**다. 그쪽은 claude CLI 가
/// 발급한 좌표이고 이쪽은 supervisor 의 세션 id 다 — 같은 surface 가 둘 다
/// 가질 수 있으므로 키를 합치면 한쪽이 다른 쪽을 덮는다. 이 plugin 이 남기는 대화 id 표지
/// (`hookless::CONVERSATION_META_KEY`)와도 같은 이유로 다른 키다(B107).
pub const SESSION_META_KEY: &str = "agent-session-id";

/// 잇기가 실패하는 사유 — **부재와 고장을 가른다**.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkError {
    /// 그 surface 에 세션이 안 묶여 있다. **결함이 아니다** — 아직 안 이은 것이다.
    NotLinked { surface_id: u32 },
    /// 호스트 호출 자체가 실패했다.
    HostCall(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::NotLinked { surface_id } => write!(
                f,
                "{}",
                crate::domain::i18n::t1("sup.err.surface_not_linked", &surface_id.to_string())
            ),
            LinkError::HostCall(m) => {
                write!(f, "{}", crate::domain::i18n::t1("sup.err.host_call", m))
            }
        }
    }
}

/// surface 에 세션 id 를 심는다 — **잇기**.
///
/// 덮어쓰기를 막지 않는다. 한 surface 가 다른 세션으로 갈아타는 것은 정상이고
/// (세션을 끝내고 새로 띄우는 경우), 막으면 그때 `unlink` 를 강요하게 된다.
pub fn link<H: HostCall>(host: &H, surface_id: u32, session_id: &str) -> Result<(), LinkError> {
    host.call(
        "surface.meta.set",
        json!({ "surface_id": surface_id, "key": SESSION_META_KEY, "value": session_id }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// surface 에서 짝을 **끊는다**.
///
/// # tasty 의 `terminal.release` 와 같은 자리
///
/// 그쪽은 *"프로세스는 살리고 관리만 뗀다"* 를 말할 유일한 경로였다 —
/// 없으면 관리를 그만두는 길이 kill 뿐이라 그 탭의 **사용자 작업이 함께
/// 파괴된다**. 여기서도 같다: 짝만 끊고 **세션도 surface 도 안 건드린다**.
///
/// ⚠️ **`agent_session.delete` 와 다르다.** 그쪽은 레코드를 지워 되돌릴 수
/// 없고, 이쪽은 **표시만 뗀다** — 다시 `link` 하면 그대로 돌아온다.
///
/// 안 이어진 surface 에 불러도 **성공이다**. 끊는 것의 목적은 *"이후 안
/// 이어져 있게 하는 것"* 이고 그 상태는 이미 달성돼 있다 — `delete` 가
/// 멱등을 거부한 것과 반대 판단이며, 사유가 다르다: 그쪽은 **오타난 id 가
/// 성공으로 보이는 것**을 막아야 했고, 이쪽 인자는 id 가 아니라 surface 라
/// 오타면 그 surface 자체가 없다.
pub fn unlink<H: HostCall>(host: &H, surface_id: u32) -> Result<(), LinkError> {
    host.call(
        "surface.meta.unset",
        json!({ "surface_id": surface_id, "key": SESSION_META_KEY }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// surface 에서 세션 id 를 읽는다 — **surface → 세션**.
///
/// 빈 문자열은 **없는 것으로 친다**. 호스트가 지운 meta 를 빈 값으로 답할 수
/// 있고, 그것을 세션 id 로 들고 가면 `not_found` 가 엉뚱한 자리에서 난다.
/// 상태줄 값(`usage::to_snapshot`)을 담는 meta 칸.
///
/// 스냅샷이 아니라 meta 에 두는 이유: 호스트는 plugin 스냅샷을 탭을 만들거나 복원할 때 한 번 캐시하고
/// 다시 묻지 않는다(`surface.snapshot` 을 보내는 자리가 없다). 상태줄 값은 그 뒤에 생기므로 스냅샷으로는
/// 영영 저장되지 않는다. meta 는 호스트가 레이아웃과 함께 저장하고 재시작 때 새 id 로 되심는다.
pub const USAGE_META_KEY: &str = "agent-session-usage";

/// 상태줄 값을 meta 에 쓴다 — 실패해도 화면은 그대로라 로그만 남긴다.
pub fn save_usage<H: HostCall>(host: &H, surface_id: u32, usage: &Value) {
    if let Err(e) = host.call(
        "surface.meta.set",
        json!({ "surface_id": surface_id, "key": USAGE_META_KEY, "value": usage.to_string() }),
    ) {
        tracing::debug!("[agent_session] surface={surface_id}: 상태줄 값을 못 남겼다: {e}");
    }
}

/// meta 에 남긴 상태줄 값 — 없거나 JSON 이 아니면 `None`.
pub fn load_usage<H: HostCall>(host: &H, surface_id: u32) -> Option<Value> {
    let resp = host
        .call(
            "surface.meta.get",
            json!({ "surface_id": surface_id, "key": USAGE_META_KEY }),
        )
        .ok()?;
    serde_json::from_str(resp.get("value")?.as_str()?).ok()
}

pub fn session_of_surface<H: HostCall>(host: &H, surface_id: u32) -> Result<String, LinkError> {
    let resp = host
        .call(
            "surface.meta.get",
            json!({ "surface_id": surface_id, "key": SESSION_META_KEY }),
        )
        .map_err(LinkError::HostCall)?;
    resp.get("value")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or(LinkError::NotLinked { surface_id })
}

/// 열려 있는 surface 전부에서 이 plugin 의 meta 를 걷어 **세션 → surface** 를 만든다.
///
/// # 왜 역방향을 목록으로 푸나
///
/// 호스트에 *"이 meta 값을 가진 surface 를 찾아라"* 는 표면이 없다. 그래서
/// 목록을 훑는다 — surface 수는 수십 규모이고 조회는 20 ms 대다.
///
/// **못 읽은 surface 를 세서 함께 낸다.** 조용히 빼면 짝이 있는데 없다고
/// 답하게 되고, 호출자는 그것을 *"안 이어져 있다"* 로 읽는다.
pub fn surfaces_by_session<H: HostCall>(
    host: &H,
) -> Result<(Vec<(u32, String)>, usize), LinkError> {
    let resp = host
        .call("surface.list", json!({}))
        .map_err(LinkError::HostCall)?;
    // **응답은 배열 그 자체다** — `{"surfaces":[...]}` 가 아니다(실측 2026-09-21).
    // 감싼 모양도 받아 주는 이유: 호스트가 나중에 봉투를 씌워도 여기서 깨지지
    // 않게. 둘 다 아니면 오류다 — 빈 목록으로 흘리면 "안 이어져 있다" 로 읽힌다.
    let arr = resp
        .as_array()
        .or_else(|| resp.get("surfaces").and_then(Value::as_array))
        .ok_or_else(|| {
            LinkError::HostCall(crate::domain::i18n::t1(
                "sup.err.surface_list_not_array",
                &resp.to_string(),
            ))
        })?;

    let mut pairs = Vec::new();
    let mut unreadable = 0usize;
    for s in arr {
        // 키는 `id` 다 — `surface_id` 가 아니다(실측). 가짜 호스트로는
        // 안 잡히는 자리였다: 우리가 지은 픽스처가 우리 가정을 그대로 썼다.
        let Some(sid) = s
            .get("id")
            .or_else(|| s.get("surface_id"))
            .and_then(Value::as_u64)
        else {
            unreadable += 1;
            continue;
        };
        let sid = u32::try_from(sid).unwrap_or(u32::MAX);
        // 안 이어진 surface 는 **오류가 아니다** — 대부분이 그렇다.
        if let Ok(session) = session_of_surface(host, sid) {
            pairs.push((sid, session));
        }
    }
    Ok((pairs, unreadable))
}

/// 같은 surface 에 이어진 **다른 세션들** — tasty `terminal.parent` 가
/// 답하던 물음의 이쪽 판본.
///
/// # 왜 "부모" 가 아니라 "형제" 인가
///
/// tasty 는 *"이 자식을 누가 띄웠나"* 를 묻는다. 우리 세션에는 **그 칸이
/// 없다** — id·kind·state·좌표·cwd·agent 이름뿐이고 계층이 없다.
///
/// 칸을 새로 만들지 않은 것은 의도다: 세션 도메인에 tasty 개념을 들이면
/// **화면 없는 다른 클라이언트에게 무의미한 칸**이 생기고, 그것을 채우는
/// 곳이 없으면 죽은 칸이 된다(`Cell.attrs` 가 그렇게 생겼다).
///
/// 대신 **이미 있는 짝 정보로 답할 수 있는 물음**으로 바꿨다 — *"이 세션과
/// 같은 자리(surface)를 쓰는 다른 세션은 무엇인가"*. 한 탭에서 세션을
/// 갈아 끼웠을 때 그 이력이 여기 보인다.
///
/// ⚠️ **빈 결과가 정상이다.** 대부분의 세션은 형제가 없다 — 오류로 두면
/// 호출자가 그것을 결함으로 읽는다.
/// 이 세션이 **여러 surface 에 걸쳐 있을 때** 나머지 surface 들.
///
/// **"같은 surface 의 다른 세션" 이 아니다.** `surface.meta` 는 키 하나에
/// 값 하나라 surface→세션은 **1:1 로 강제**된다(실측 2026-09-21: 같은 surface
/// 에 두 번째 `link` 를 걸면 첫 번째가 **덮어쓰인다**). 그 방향으로 형제를
/// 정의하면 **구조적으로 항상 빈 목록**이고, 그것은 "형제가 없다" 와 구분되지
/// 않는다. 표현 가능한 방향은 반대쪽 — 세션 하나가 surface 여럿을 가지는 것이다.
pub fn siblings_of_session<H: HostCall>(
    host: &H,
    session_id: &str,
) -> Result<Vec<(u32, String)>, LinkError> {
    let (pairs, _unreadable) = surfaces_by_session(host)?;
    Ok(pairs.into_iter().filter(|(_, s)| s == session_id).collect())
}

/// 회수된 세션을 **화면에 드러낸다** — tasty 의 attention 하이라이트를 쓴다.
///
/// # 왜 필요한가
///
/// 회수는 조용하다. 세션이 재워져도 탭은 그대로라 사용자는 **재워진 줄
/// 모른다** — 그래서 어느 탭이 자원을 놓았는지, 다음 입력이 느릴지를
/// 알 수 없다. 이 plugin 의 존재 이유가 그 자리였는데 P0·P1 은 **묻는
/// 사람에게만** 답했다(CLI 를 쳐야 보인다).
///
/// # 왜 `needs_input` 인가
///
/// tasty 의 attention 종류는 `completion` 과 `needs_input` 둘뿐이다.
/// 회수는 *"끝났다"* 가 아니라 *"깨우려면 입력이 필요하다"* 에 가깝다 —
/// 다음 `tell` 이 세션을 되살리기 때문이다.
///
/// **없는 종류를 새로 만들지 않았다.** tasty 본체를 고치지 않는다는 제약이
/// 있고, 둘 중 뜻이 가까운 쪽을 고르는 편이 낫다.
pub fn mark_reclaimed<H: HostCall>(host: &H, surface_id: u32) -> Result<(), LinkError> {
    host.call(
        "surface.completion",
        json!({ "surface_id": surface_id, "kind": "needs_input" }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// 깨어난 세션의 표시를 **거둔다**.
///
/// 거두지 않으면 하이라이트가 남아 *"아직 재워져 있다"* 고 거짓말한다 —
/// 표시를 켜는 것보다 **끄는 것을 빠뜨리기가 더 쉽고**, 그 실패는 조용하다.
pub fn clear_reclaimed<H: HostCall>(host: &H, surface_id: u32) -> Result<(), LinkError> {
    host.call(
        "surface.attention.clear",
        json!({ "surface_id": surface_id }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// 턴이 끝났음을 **화면에 드러낸다** — claude plugin 이 Stop 훅에서 하는 것과 같은 `completion`.
///
/// 대화 탭은 다른 워크스페이스에 있는 일이 많다. 답이 끝난 것을 탭 테두리 · 제목 · 워크스페이스
/// 배지로 알리지 않으면 사용자가 탭마다 들어가 봐야 한다. 거두는 것은 호스트가 그 탭으로 갈 때 한다.
pub fn mark_done<H: HostCall>(host: &H, surface_id: u32) -> Result<(), LinkError> {
    host.call(
        "surface.completion",
        json!({ "surface_id": surface_id, "kind": "completion" }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// 이 surface 에 hook 이벤트를 발사한다 — 키는 매니페스트 `[[contributes.hook_events]]` 에 선언한 것만 받는다.
pub fn fire_hook<H: HostCall>(host: &H, surface_id: u32, event: &str) -> Result<(), LinkError> {
    host.call(
        "surface.fire_hook",
        json!({ "surface_id": surface_id, "event": event }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// 텔레메트리 한 점 — `surface_id` 태그를 단다(claude plugin 과 같은 모양).
pub fn record_metric<H: HostCall>(
    host: &H,
    surface_id: u32,
    metric: &str,
    value: f64,
) -> Result<(), LinkError> {
    host.call(
        "telemetry.record",
        json!({ "metric": metric, "value": value, "tags": { "surface_id": surface_id.to_string() } }),
    )
    .map(|_| ())
    .map_err(LinkError::HostCall)
}

/// 회수 표시(`needs_input`)일 때만 거둔다 — 깨어 있는 세션의 **턴 완료** 표시는 남긴다.
///
/// sync 가 깨어 있는 세션의 표시를 종류를 가리지 않고 지우면, 턴이 끝나 켠 `completion` 이
/// 30초 안에 사라져 사용자가 보기도 전에 없어진다. 이 plugin 이 켜는 종류는 둘이라 자기 것만 거둔다.
pub fn clear_reclaimed_mark<H: HostCall>(host: &H, surface_id: u32) -> Result<bool, LinkError> {
    if attention_of(host, surface_id)?.as_deref() != Some("needs_input") {
        return Ok(false);
    }
    clear_reclaimed(host, surface_id).map(|()| true)
}

/// 지금 이 surface 에 걸린 attention 종류(`null` 이면 없음).
pub fn attention_of<H: HostCall>(host: &H, surface_id: u32) -> Result<Option<String>, LinkError> {
    let r = host
        .call("surface.attention.get", json!({ "surface_id": surface_id }))
        .map_err(LinkError::HostCall)?;
    Ok(r.get("kind").and_then(Value::as_str).map(str::to_string))
}

/// surface 하나가 화면 어디에 있는지 — 탭 이름·pane·워크스페이스.
///
/// 숫자만 든 `surface_id` 는 사용자에게 아무것도 말하지 않는다. 세션
/// 목록이 **화면 지도와 같은 말을 하게** 하는 것이 이 구조체의 목적이다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SurfacePlace {
    pub tab_name: Option<String>,
    pub pane_id: Option<u32>,
    pub workspace_id: Option<u32>,
    pub workspace_name: Option<String>,
}

/// 모든 surface 의 자리를 한 번에 읽는다 — pane 마다 `tab.list` 를 돈다.
///
/// # 왜 한 번에 모으나
///
/// surface 마다 따로 물으면 pane 수만큼 같은 호출이 반복된다. 목록을
/// 그리는 쪽은 보통 **전부**를 필요로 하므로 한 번 모아 나눠 쓴다.
///
/// 못 읽은 pane 은 **건너뛴다** — 자리를 모르는 것이 목록 전체를 막을
/// 이유는 아니다. 그 surface 는 `SurfacePlace::default()` 로 남는다.
pub fn places_by_surface<H: HostCall>(
    host: &H,
) -> Result<std::collections::BTreeMap<u32, SurfacePlace>, LinkError> {
    let panes = host
        .call("pane.list", json!({}))
        .map_err(LinkError::HostCall)?;
    let panes = panes.as_array().cloned().unwrap_or_default();

    let mut out = std::collections::BTreeMap::new();
    for p in panes {
        let Some(pane_id) = p.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let pane_id = u32::try_from(pane_id).unwrap_or(u32::MAX);
        let ws_id = p
            .get("workspace_id")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let ws_name = p
            .get("workspace_name")
            .and_then(Value::as_str)
            .map(str::to_string);

        // pane 하나를 못 읽어도 나머지는 그린다.
        let Ok(tabs) = host.call("tab.list", json!({ "pane_id": pane_id })) else {
            continue;
        };
        let Some(arr) = tabs.get("tabs").and_then(Value::as_array) else {
            continue;
        };
        for t in arr {
            let Some(sid) = t.get("surface_id").and_then(Value::as_u64) else {
                continue;
            };
            out.insert(
                u32::try_from(sid).unwrap_or(u32::MAX),
                SurfacePlace {
                    tab_name: t.get("name").and_then(Value::as_str).map(str::to_string),
                    pane_id: Some(pane_id),
                    workspace_id: ws_id,
                    workspace_name: ws_name.clone(),
                },
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests_support {
    //! 가짜 호스트 — `surface_link` 와 `main` 의 핸들러 시험이 **함께 쓴다**.
    //!
    //! 종전에는 `mod tests` 안에 숨어 있어 다른 모듈이 못 썼다. 핸들러를
    //! 데몬 없이 재려면 호스트도 가짜여야 해서 여기로 올렸다.

    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    use crate::ports::host::HostCall;

    /// 워커로 옮길 수 있는 가짜 호스트 — 부른 것과 되던진 것을 복제본끼리 함께 쓰는 기록에 남긴다.
    ///
    /// [`FakeHost`] 는 `RefCell` 이라 스레드로 못 넘긴다. plugin 의 껍데기 메서드는 워커를 띄울 수 있어
    /// `SendHost` 를 받으므로 그 시험은 이것을 쓴다. 모든 호출에 `null` 로 답한다 — 설정은 기본값으로,
    /// 표면 조회는 "없음" 으로 읽힌다.
    #[derive(Clone, Default)]
    pub struct SharedHost {
        calls: Arc<Mutex<Vec<(String, Value)>>>,
        invoked: Arc<Mutex<Vec<(String, Value)>>>,
    }

    impl SharedHost {
        /// 부른 순서대로 `(메서드, 인자)`.
        pub fn calls(&self) -> Vec<(String, Value)> {
            self.calls.lock().unwrap().clone()
        }

        /// 이 surface 에 보낸 `webview.post_message` 의 `data` 들.
        pub fn posted_to(&self, sid: u32) -> Vec<Value> {
            self.calls()
                .into_iter()
                .filter(|(m, p)| m == "webview.post_message" && p["surface_id"] == sid)
                .map(|(_, p)| p["data"].clone())
                .collect()
        }

        /// 되던진(`self_invoke`) 순서대로 `(메서드, 인자)`.
        pub fn invoked(&self) -> Vec<(String, Value)> {
            self.invoked.lock().unwrap().clone()
        }
    }

    impl HostCall for SharedHost {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap()
                .push((method.to_string(), params));
            Ok(Value::Null)
        }

        fn self_invoke(&self, method: &str, params: Value) -> Result<(), String> {
            self.invoked
                .lock()
                .unwrap()
                .push((method.to_string(), params));
            Ok(())
        }
    }

    pub struct FakeHost {
        meta: RefCell<std::collections::HashMap<u32, String>>,
        /// 실물과 같게 — surface 당 종류 하나, 없으면 `null`.
        attention: RefCell<std::collections::HashMap<u32, String>>,
        panes: Value,
        tabs: Value,
        /// 쓰기만 거절한다 — 잇기 실패 경로를 만든다.
        refuse_writes: bool,
        /// **문서를 몇 번 다시 실었나** — 이 surface 는 갱신 수단이
        /// `webview.set_url` 하나뿐이라(부분 갱신·JS 없음) 한 번 실을
        /// 때마다 흰 바탕이 스친다. 횟수가 곧 **깜빡임 횟수**다.
        set_url_calls: RefCell<usize>,
        surfaces: Vec<Value>,
        fail: Option<String>,
    }

    impl FakeHost {
        pub fn new() -> Self {
            Self {
                meta: RefCell::new(std::collections::HashMap::new()),
                attention: RefCell::new(std::collections::HashMap::new()),
                panes: Value::Array(vec![]),
                tabs: json!({ "tabs": [] }),
                refuse_writes: false,
                set_url_calls: RefCell::new(0),
                surfaces: vec![],
                fail: None,
            }
        }
        pub fn with_surfaces(mut self, v: Vec<Value>) -> Self {
            self.surfaces = v;
            self
        }
        pub fn with_layout(mut self, panes: Value, tabs: Value) -> Self {
            self.panes = panes;
            self.tabs = tabs;
            self
        }
        /// 문서 재적재 횟수 — 깜빡임을 세는 자리.
        pub fn set_url_calls(&self) -> usize {
            *self.set_url_calls.borrow()
        }

        pub fn failing(m: &str) -> Self {
            let mut s = Self::new();
            s.fail = Some(m.to_string());
            s
        }
    }

    impl HostCall for FakeHost {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            if let Some(m) = &self.fail {
                return Err(m.clone());
            }
            let sid = params
                .get("surface_id")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            if method == "webview.set_url" {
                *self.set_url_calls.borrow_mut() += 1;
            }
            match method {
                "surface.meta.set" if self.refuse_writes => Err("쓰기 거절(시험)".to_string()),
                "surface.meta.set" => {
                    let v = params.get("value").and_then(Value::as_str).unwrap_or("");
                    self.meta.borrow_mut().insert(sid, v.to_string());
                    Ok(json!({}))
                }
                "surface.meta.get" => Ok(json!({
                    "value": self.meta.borrow().get(&sid).cloned().unwrap_or_default()
                })),
                "surface.meta.unset" => {
                    self.meta.borrow_mut().remove(&sid);
                    Ok(json!({}))
                }
                // **실물 그대로** — 배열 자체를 낸다(2026-09-21 실측).
                "surface.list" => Ok(Value::Array(self.surfaces.clone())),
                // 실측 모양 그대로 — pane 은 배열, tab 은 `{pane_id, tabs:[…]}`.
                "pane.list" => Ok(self.panes.clone()),
                "tab.list" => Ok(self.tabs.clone()),
                // 실측 문면 그대로 — `completion` 이 켜고 `attention.clear` 가 끈다.
                "surface.completion" => {
                    let kind = params
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("completion");
                    self.attention.borrow_mut().insert(sid, kind.to_string());
                    Ok(json!({ "ok": true, "surface_id": sid }))
                }
                "surface.attention.clear" => {
                    self.attention.borrow_mut().remove(&sid);
                    Ok(json!({ "ok": true, "surface_id": sid }))
                }
                "surface.attention.get" => Ok(json!({
                    "surface_id": sid,
                    // 없으면 **`null`** 이다(실측) — 빈 문자열이 아니다.
                    "kind": self.attention.borrow().get(&sid).cloned()
                        .map_or(Value::Null, Value::String),
                })),
                other => Err(format!("모르는 메서드: {other}")),
            }
        }
    }

    impl FakeHost {
        /// surface 하나가 세션에 이어진 흔한 상태.
        pub fn linked(surface_id: u32, session: &str) -> Self {
            let h = Self::new().with_surfaces(vec![json!({ "id": surface_id })]);
            super::link(&h, surface_id, session).expect("link");
            h
        }

        /// 읽기는 되는데 **쓰기가 거절되는** 호스트 — 잇기 실패를 만든다.
        pub fn write_only_failure() -> Self {
            let mut h = Self::new().with_surfaces(vec![json!({ "id": 1 })]);
            h.refuse_writes = true;
            h
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::FakeHost;
    use super::*;

    /// 이으면 양방향으로 찾을 수 있다.
    #[test]
    fn a_linked_surface_resolves_both_ways() {
        let h = FakeHost::new().with_surfaces(vec![json!({ "id": 7 })]);
        link(&h, 7, "sess-abc").expect("link");

        assert_eq!(session_of_surface(&h, 7).expect("정방향"), "sess-abc");
        let (pairs, bad) = surfaces_by_session(&h).expect("역방향");
        assert_eq!(pairs, vec![(7u32, "sess-abc".to_string())]);
        assert_eq!(bad, 0);
    }

    /// 회수를 **화면에 켜고 거둔다** — 그리고 `get` 이 그것을 되읽는다.
    ///
    /// 표시는 켜는 것보다 **거두는 것을 빠뜨리기 쉽고**, 안 거두면
    /// 하이라이트가 남아 *"아직 재워져 있다"* 고 거짓말한다.
    #[test]
    fn a_reclaimed_surface_is_marked_and_the_mark_can_be_taken_back() {
        let h = FakeHost::new().with_surfaces(vec![json!({ "id": 1 })]);

        // 처음엔 없다 — 이 전제가 깨지면 아래가 공허해진다.
        assert_eq!(
            attention_of(&h, 1).expect("조회"),
            None,
            "처음부터 표시가 있었다"
        );

        mark_reclaimed(&h, 1).expect("표시");
        assert_eq!(
            attention_of(&h, 1).expect("조회").as_deref(),
            Some("needs_input"),
            "회수를 표시했는데 화면이 모른다"
        );

        clear_reclaimed(&h, 1).expect("거두기");
        assert_eq!(
            attention_of(&h, 1).expect("조회"),
            None,
            "표시가 남았다 — 깨어난 세션이 재워진 것으로 보인다"
        );
    }

    /// 표시 종류는 **`needs_input`** 이다 — `completion` 이 아니다.
    ///
    /// 회수는 *"끝났다"* 가 아니라 *"깨우려면 입력이 필요하다"* 다. 둘을
    /// 뒤바꾸면 사용자가 **끝난 작업으로 읽고 탭을 닫는다**.
    #[test]
    fn a_reclaimed_surface_asks_for_input_rather_than_reporting_completion() {
        let h = FakeHost::new().with_surfaces(vec![json!({ "id": 7 })]);
        mark_reclaimed(&h, 7).expect("표시");
        let kind = attention_of(&h, 7).expect("조회");
        assert_eq!(kind.as_deref(), Some("needs_input"));
        assert_ne!(kind.as_deref(), Some("completion"), "끝난 것으로 보인다");
    }

    /// 호스트가 거절하면 **오류로 올린다** — 조용히 성공으로 흘리지 않는다.
    ///
    /// 실제로 `notification` 권한이 없어 거절당했고(실측), 그때 사유가
    /// 응답에 실려 있어 한 번에 갈렸다. 삼켰으면 *"표시가 왜 안 되지"* 로
    /// 한참 헤맸을 자리다.
    #[test]
    fn a_host_that_refuses_to_mark_is_reported_not_swallowed() {
        let h = FakeHost::failing("permission_denied: missing 'notification'");
        match mark_reclaimed(&h, 1) {
            Err(LinkError::HostCall(m)) => {
                assert!(m.contains("notification"), "사유가 사라졌다: {m}");
            }
            other => panic!("거절이 오류로 안 올라왔다: {other:?}"),
        }
    }

    /// surface 가 **화면 어디에 있는지** 이름으로 답한다.
    ///
    /// 숫자만 든 `surface_id` 는 사용자에게 아무것도 말하지 않는다 —
    /// 세션 목록이 화면 지도와 같은 말을 하게 하는 것이 목적이다.
    #[test]
    fn a_surface_is_placed_by_tab_pane_and_workspace() {
        let h = FakeHost::new().with_layout(
            json!([{ "id": 3, "workspace_id": 9, "workspace_name": "작업판" }]),
            json!({ "pane_id": 3, "tabs": [{ "id": 1, "name": "리뷰어", "surface_id": 42 }] }),
        );
        let places = places_by_surface(&h).expect("조회");

        let p = places.get(&42).expect("surface 42 의 자리가 없다");
        assert_eq!(p.tab_name.as_deref(), Some("리뷰어"));
        assert_eq!(p.pane_id, Some(3));
        assert_eq!(p.workspace_id, Some(9));
        assert_eq!(p.workspace_name.as_deref(), Some("작업판"));
    }

    /// pane 이 하나도 없으면 **빈 지도**다 — 오류가 아니다.
    ///
    /// 자리를 모르는 것이 목록 전체를 막을 이유는 아니다(호출부는
    /// `unwrap_or_default` 로 숫자만 낸다).
    #[test]
    fn an_empty_layout_yields_an_empty_map_rather_than_an_error() {
        let h = FakeHost::new();
        assert!(places_by_surface(&h).expect("조회").is_empty());
    }

    /// 호스트가 통째로 거절하면 **오류로 올린다**.
    #[test]
    fn a_failing_host_is_reported_when_placing_surfaces() {
        let h = FakeHost::failing("pane.list 거절");
        assert!(places_by_surface(&h).is_err(), "거절이 빈 지도로 흘렀다");
    }

    /// 자동 sync 주기는 **회수 임계보다 훨씬 촘촘**하되 호출 비용이 묻힐
    /// 만큼은 길어야 한다.
    ///
    /// 실측: sync 한 번이 **~0.10초**이고 **이어진 수와 무관**하다(1건과
    /// 6건이 같았다 — 대부분이 왕복 비용이다). 회수 기본 임계는 600초.
    #[test]
    fn the_auto_sync_period_sits_between_the_call_cost_and_the_idle_threshold() {
        let period = crate::AUTO_SYNC_EVERY.as_secs();
        assert!(
            period >= 10,
            "너무 잦다 — 호출 비용(~0.1초)이 안 묻힌다: {period}초"
        );
        assert!(
            period <= 120,
            "너무 뜸하다 — 회수되고 한참 뒤에야 화면에 뜬다: {period}초"
        );
    }

    /// **안 이어진 것과 고장난 것을 가른다.**
    ///
    /// 뭉뚱그리면 사용자가 *"버그다"* 와 *"먼저 이어라"* 중 무엇을 할지 모른다.
    #[test]
    fn not_linked_is_not_a_host_failure() {
        let h = FakeHost::new();
        assert_eq!(
            session_of_surface(&h, 9),
            Err(LinkError::NotLinked { surface_id: 9 })
        );

        let broken = FakeHost::failing("소켓 끊김");
        match session_of_surface(&broken, 9) {
            Err(LinkError::HostCall(_)) => {}
            other => panic!("호스트 고장을 미연결로 읽었다: {other:?}"),
        }
    }

    /// 빈 meta 는 **없는 것**이다 — 세션 id 로 들고 가면 엉뚱한 곳에서 터진다.
    #[test]
    fn an_empty_meta_value_counts_as_unlinked() {
        let h = FakeHost::new();
        link(&h, 3, "").expect("빈 값 set");
        assert_eq!(
            session_of_surface(&h, 3),
            Err(LinkError::NotLinked { surface_id: 3 })
        );
    }

    /// 안 이어진 surface 는 역방향 목록에서 **조용히 빠진다** — 그것이 정상이다.
    /// 다만 **읽을 수 없던 항목은 센다**.
    #[test]
    fn unlinked_surfaces_are_skipped_but_unreadable_ones_are_counted() {
        let h = FakeHost::new().with_surfaces(vec![
            json!({ "id": 1 }),
            json!({ "no_id": true }),
            json!({ "id": 2 }),
        ]);
        link(&h, 2, "sess-b").expect("link");

        let (pairs, bad) = surfaces_by_session(&h).expect("목록");
        assert_eq!(
            pairs,
            vec![(2u32, "sess-b".to_string())],
            "안 이어진 1 이 섞였다"
        );
        assert_eq!(bad, 1, "읽을 수 없던 항목을 안 셌다");
    }

    /// **끊으면 안 이어진 상태로 돌아간다** — 그리고 다시 이을 수 있다.
    #[test]
    fn unlinking_returns_to_the_unlinked_state_and_can_be_redone() {
        let h = FakeHost::new();
        link(&h, 4, "sess-x").expect("link");
        assert!(session_of_surface(&h, 4).is_ok(), "이어지지 않았다");

        unlink(&h, 4).expect("unlink");
        assert_eq!(
            session_of_surface(&h, 4),
            Err(LinkError::NotLinked { surface_id: 4 })
        );

        // 표시만 뗀 것이므로 다시 이으면 돌아온다.
        link(&h, 4, "sess-x").expect("re-link");
        assert_eq!(session_of_surface(&h, 4).expect("재연결"), "sess-x");
    }

    /// 안 이어진 surface 를 끊어도 **성공이다**.
    ///
    /// `delete` 가 멱등을 거부한 것과 반대 판단이고 사유가 다르다 — 그쪽은
    /// 오타난 세션 id 가 성공으로 보이는 것을 막아야 했지만, 이쪽 인자는
    /// surface 라 오타면 그 surface 자체가 없다.
    #[test]
    fn unlinking_something_never_linked_is_still_success() {
        let h = FakeHost::new();
        assert!(
            unlink(&h, 99).is_ok(),
            "끊을 것이 없다고 실패하면 호출자가 먼저 조회해야 한다"
        );
    }

    /// **형제가 없는 것이 정상이다** — 오류가 아니라 빈 목록이다.
    #[test]
    fn a_session_spanning_several_surfaces_lists_them_all() {
        let h = FakeHost::new().with_surfaces(vec![
            json!({ "id": 1 }),
            json!({ "id": 2 }),
            json!({ "id": 3 }),
        ]);
        link(&h, 1, "wide").expect("link 1");
        link(&h, 2, "wide").expect("link 2");
        link(&h, 3, "other").expect("link 3");

        // **양성 대조** — 비지 않은 답을 한 번은 받아야 한다. 이게 없으면
        // 상수 `[]` 를 내는 함수도 통과한다(2026-09-21 실제로 그랬다).
        let mut got = siblings_of_session(&h, "wide").expect("조회");
        got.sort();
        assert_eq!(got, vec![(1, "wide".to_string()), (2, "wide".to_string())]);

        // 하나뿐인 세션은 자기 surface 하나만.
        assert_eq!(
            siblings_of_session(&h, "other").expect("조회"),
            vec![(3, "other".to_string())]
        );
        // 없는 세션은 빈 목록이지 오류가 아니다.
        assert_eq!(siblings_of_session(&h, "없는세션").expect("조회"), vec![]);
    }

    #[test]
    fn a_surface_holds_one_session_so_relinking_overwrites() {
        // 실물이 그렇다(2026-09-21). 이 성질이 "같은 surface 의 다른 세션" 이라는
        // 형제 정의를 **구조적으로 불가능**하게 만든다.
        let h = FakeHost::new().with_surfaces(vec![json!({ "id": 1 })]);
        link(&h, 1, "first").expect("link");
        link(&h, 1, "second").expect("relink");
        assert_eq!(session_of_surface(&h, 1).expect("조회"), "second");
    }

    /// 없는 세션을 물어도 **빈 목록**이다 — 우리가 세션 존재를 판정하지
    /// 않는다. 그것은 supervisor 의 일이고, 여기서 하면 답이 둘이 된다.
    #[test]
    fn asking_about_an_unknown_session_is_empty_not_an_error() {
        let h = FakeHost::new().with_surfaces(vec![json!({ "id": 1 })]);
        assert_eq!(siblings_of_session(&h, "없는세션").expect("조회"), vec![]);
    }

    /// 덮어쓰기를 막지 않는다 — 한 surface 가 다른 세션으로 갈아타는 것은 정상이다.
    #[test]
    fn relinking_replaces_the_previous_session() {
        let h = FakeHost::new();
        link(&h, 5, "old").expect("1차");
        link(&h, 5, "new").expect("2차");
        assert_eq!(session_of_surface(&h, 5).expect("조회"), "new");
    }
}
