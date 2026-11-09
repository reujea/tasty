//! 대화 탭이 낸 navigation 주소 — 단추 · 폼이 싣는 표식과 그 값을 읽는다.
//!
//! 화면이 링크와 폼에 표식을 싣고, 호스트가 navigation 시도로 그 주소를 알려 주면 여기서 읽는다.
//! 표식이 서로 달라야 하는 이유는 각 상수에 적혀 있다.

use crate::domain::convo::PickField;

/// 이 surface 가 낸 navigation 임을 가르는 fragment 표식.
///
/// 이것이 없으면 **다른 페이지의 `?t=` 까지** 입력으로 읽어, 사용자가 안 보낸
/// 말이 에이전트에게 간다.
pub(crate) const FRAG_MARK: &str = "agent-send";

/// 세션을 **시작**하는 폼의 표식. 보내기(`FRAG_MARK`)와 갈라야 한다 —
/// 같은 표식이면 시작 요청이 그대로 첫 메시지로 나간다.
pub(crate) const START_MARK: &str = "agent-start";

/// 기존 세션에 **붙는** 요청의 표식. 시작·보내기와 갈라야 한다.
pub(crate) const ATTACH_MARK: &str = "agent-attach";

/// 폴더 단추 — 값만 싣는다. 시작(`START_MARK`)과 **반드시 달라야 한다**:
/// 같으면 단추 한 번에 첫 프로필로 세션이 떠 버린다.
pub(crate) const PICK_MARK: &str = "agent-cwd";

/// 폴더 찾기 단추 — host 피커를 연다. 값을 안 싣는다(고르는 것은 피커다). `pf=` 는 고른 폴더를
/// 넣을 칸이다([`PickField`]).
pub(crate) const BROWSE_MARK: &str = "agent-browse";

/// 세션 **삭제** 표식 — 되돌릴 수 없으므로 확인 단계를 거친다.
///
/// 붙기(`ATTACH_MARK`)와 **반드시 달라야 한다**: 같으면 붙으려다
/// 지운다. 두 링크가 같은 표에 나란히 있어 오조작이 쉬운 자리다.
pub(crate) const DELETE_MARK: &str = "agent-del";

/// 목록 새로고침 — 삭제(`DELETE_MARK`)와 **반드시 달라야 한다**:
/// 같으면 새로 받으려다 세션을 지운다.
pub(crate) const REFRESH_MARK: &str = "agent-refresh";

/// 도는 턴 **중지** — 중지 단추와 Esc 가 낸다. 보내기(`FRAG_MARK`)와 **반드시 달라야
/// 한다**: 같으면 멈추려다 빈 말을 보낸다.
pub(crate) const STOP_MARK: &str = "agent-stop";

/// 보관된 이전 대화를 펼친다 — 한 번 누를 때마다 [`crate::adapters::driven::store::turn_archive::OLDER_PAGE`] 개씩.
pub(crate) const OLDER_MARK: &str = "agent-older";

/// navigation URL 에서 보낼 텍스트를 꺼낸다.
///
/// 이 surface 가 낸 것이 아니면 `None` — 다른 링크 클릭까지 입력으로 읽으면
/// 사용자가 안 보낸 말이 에이전트에게 간다.
/// 시작 폼이 낸 값 — `(agent, cwd)`.
///
/// 세션이 없는 탭은 대화창 대신 이 폼을 그린다. tasty 의 팝업(`convert_input_popup`)
/// 은 plugin 이 egui-mesh 로 **직접 그려야** 해서(markdown 은 그 렌더에만 570줄)
/// 탭 본문에서 받는다 — 사용자가 보는 결과는 같고 비용이 훨씬 작다.
/// URL 에서 **질의 부분**을 꺼낸다 — `?` 와 fragment(`#`) 둘 다 받는다.
///
/// # 왜 둘 다인가
///
/// 폼 제출은 `?a=b` 로 오고, 단추 링크는 **fragment** 로 온다
/// (2026-09-28). fragment 를 쓰는 이유는 **화면이 안 깜빡이기** 때문
/// 이다 — `?` 로 가면 WebKit 이 문서를 버리고 새로 로드해(그 로드는
/// `about:blank?…` 라 실패한다) 페이지가 한 번 죽었다 살아난다.
/// fragment 는 **같은 문서 안 이동**이라 DOM 이 그대로 산다(실측:
/// `didFinish` 1회 · 클릭 후 DOM 생존).
///
/// **`#` 는 `%23` 으로 온다.** base 가 `about:blank` 라 WebKit 이
/// 합칠 때 인코딩한다(실측: `about:blank%23c=…`). 그래서 둘 다 본다.
///
/// **입력 폼(`?t=`)은 fragment 로 못 옮긴다** — 종전에 시도했다가
/// 되돌린 자리다. 브라우저가 제출 시 `#` 를 **쿼리 값 안에** `%23` 로
/// 넣어 사용자 텍스트에 섞였다(`"안녕#agent-send-0"`). 그것은 폼
/// 제출의 동작이고, 아래 단추 링크(`<a href>`)와는 다른 경로다.
pub(crate) fn query_of(url: &str) -> Option<&str> {
    // **쿼리가 먼저다.** `?` 가 있으면 그것이 값이고, fragment 는 꼬리다.
    //
    // 폼 제출은 `?a=…&agent-start=1%23` 로 온다 — 끝의 `%23` 는
    // `action="#"` 가 남긴 **종결자**이지 구분자가 아니다. fragment 를
    // 먼저 보면 그 뒤(빈 문자열)를 질의로 읽어 **시작 단추가 죽는다**
    // (실측 2026-09-28: 흰 화면).
    if let Some(q) = url.split('?').nth(1) {
        // 꼬리 fragment 를 떼어 낸다 — 날것과 인코딩된 것 둘 다.
        let q = q.split('#').next().unwrap_or(q);
        let q = q.split("%23").next().unwrap_or(q);
        return Some(q);
    }
    // `?` 가 없으면 단추 링크다 — fragment 자체가 질의다. 구분자는 `#` 과 `%23` 중 **먼저 나온
    // 것**이다. `%23` 을 늘 먼저 찾으면, 글 속 `#`(마크다운 제목 등)이 인코딩된 `%23` 을 구분자로
    // 잘라 표식이 빠지고 입력이 조용히 버려졌다(실측 2026-10-21).
    let raw = url.find('#').map(|i| (i, 1));
    let enc = url.find("%23").map(|i| (i, 3));
    let (i, len) = match (raw, enc) {
        (Some(a), Some(b)) => a.min(b),
        (a, b) => a.or(b)?,
    };
    Some(&url[i + len..])
}

/// **이 표식이 이 URL 에 있나** — 다섯 갈래가 **같은 판정**을 쓴다.
///
/// # 왜 함수인가
///
/// 종전에는 갈래마다 `fields.iter().any(|kv| kv.starts_with(MARK))` 를
/// 적었다. 한 곳만 고치면 나머지 넷이 낡고, 그 낡음은 **그 갈래를 쓸
/// 때만** 드러난다(0.1.16 에서 실제로 밟았다 — 아래).
///
/// #  이름이 인코딩된 형태도 받는다
///
/// 표식을 `<input name>` 으로 쓰면 **브라우저가 이름을 퍼센트
/// 인코딩한다**. 종전 이름은 `agent-send=` 였고, 그 `=` 가 `%3D` 가 되어
/// `agent-send%3D=0` 으로 왔다 — `starts_with("agent-send=")` 가 어긋나
/// **입력이 통째로 무시됐다**(흰 화면).
///
/// 이름에서 `=` 를 뺐지만 그것만으로는 부족하다. 다음에 다른 문자가
/// 인코딩될 수 있으므로 **구분자를 값 기준으로** 본다: 표식 뒤에 오는
/// 것은 `=`(정상) 또는 `%3D`(인코딩됨) 뿐이다.
pub(crate) fn has_mark(fields: &[&str], mark: &str) -> bool {
    fields.iter().any(|kv| {
        kv.strip_prefix(mark).is_some_and(|rest| {
            rest.starts_with('=') || rest.starts_with("%3D") || rest.starts_with("%3d")
        })
    })
}

/// 삭제 요청 — `(세션 id, 확인했나)`.
///
/// # 왜 두 값인가
///
/// 되돌릴 수 없는 조작이라 **한 번 묻는다**. 첫 클릭은 `confirmed=false`
/// 로 와서 그 행을 *"지울까? [지운다] [취소]"* 로 바꾸고, [지운다] 가
/// `confirmed=true` 로 온다.
///
/// 확인을 **URL 로** 나른다 — JS 의 `confirm()` 을 쓰지 않는다. 그쪽은
/// 이 webview 에서 모달이 뜨는지 보장이 없고, 뜨더라도 취소를 누른 것과
/// 아무 일도 안 일어난 것을 플러그인이 구분할 수 없다.
/// 여러 세션을 한 번에 지운다 — `s=` 가 **여러 개** 올 수 있다.
///
/// 단건(`parse_delete`)과 같은 표식·같은 확인 규칙을 쓴다. 그래야
/// *"어떤 것은 묻고 어떤 것은 안 묻는"* 갈래가 안 생긴다.
///
/// ⚠️ **빈 id 는 걸러내되 그것 때문에 전체를 버리지 않는다.** URL 이
/// `s=a&s=&s=b` 로 와도 a·b 는 지워져야 한다 — 하나가 비었다고 나머지를
/// 안 지우면 사용자는 **무엇이 지워졌는지 모른다**.
/// **목록을 다시 받아 달라는 요청인가.**
///
/// 읽기라서 확인 단계가 없다 — 되돌릴 것이 없다(삭제·정리와 다른 축).
pub fn parse_refresh(url: &str) -> bool {
    let Some(q) = query_of(url) else {
        return false;
    };
    has_mark(&q.split('&').collect::<Vec<_>>(), REFRESH_MARK)
}

/// **이전 대화를 더 펼쳐 달라는 요청인가.**
pub fn parse_older(url: &str) -> bool {
    let Some(q) = query_of(url) else {
        return false;
    };
    has_mark(&q.split('&').collect::<Vec<_>>(), OLDER_MARK)
}

/// **도는 턴을 멈춰 달라는 요청인가.**
///
/// 확인 단계가 없다 — 세션은 남고 다음 말을 받는다(`agent_session.abort` 와 같은 축).
/// 도는 턴이 없을 때 와도 해가 없다.
pub fn parse_stop(url: &str) -> bool {
    let Some(q) = query_of(url) else {
        return false;
    };
    has_mark(&q.split('&').collect::<Vec<_>>(), STOP_MARK)
}

pub fn parse_delete_many(url: &str) -> Option<(Vec<String>, bool)> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, DELETE_MARK) {
        return None;
    }
    let mut ids: Vec<String> = Vec::new();
    for kv in &fields {
        let Some(raw) = kv.strip_prefix("s=") else {
            continue;
        };
        let id = percent_decode(raw);
        if id.trim().is_empty() {
            continue;
        }
        // 같은 id 가 두 번 오면 한 번만 — 두 번째는 `not_found` 가 되어
        // **성공한 삭제가 실패로 보고**된다.
        if !ids.iter().any(|x| x == &id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return None;
    }
    // `ok=1` 이 확인이다. 없으면 **묻는 단계**다.
    let confirmed = fields.contains(&"ok=1");
    Some((ids, confirmed))
}

pub fn parse_attach(url: &str) -> Option<String> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, ATTACH_MARK) {
        return None;
    }
    let id = fields.iter().find_map(|kv| kv.strip_prefix("s="))?;
    let id = percent_decode(id);
    (!id.trim().is_empty()).then_some(id)
}

/// 폴더 찾기 단추를 눌렀나 — 고른 값을 넣을 칸을 돌려준다(`pf=`, 없으면 연결 탭의 작업 폴더).
pub fn parse_browse(url: &str) -> Option<PickField> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, BROWSE_MARK) {
        return None;
    }
    Some(
        fields
            .iter()
            .find_map(|kv| kv.strip_prefix("pf="))
            .map(PickField::of_param)
            .unwrap_or_default(),
    )
}

/// 폴더 단추 — 값만 싣고 **시작하지는 않는다**.
///
/// 표식을 따로 두는 이유: 이것이 없으면 `?c=…` 는 어느 갈래에도 안 걸려
/// 조용히 무시되고, webview 는 이미 폼에서 **떠나 있다**(빈 화면). 값을
/// 받아 폼을 다시 그리는 갈래가 반드시 있어야 한다.
pub fn parse_cwd_pick(url: &str) -> Option<String> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, PICK_MARK) {
        return None;
    }
    fields
        .iter()
        .find_map(|kv| kv.strip_prefix("c="))
        .map(percent_decode)
        .filter(|v| !v.trim().is_empty())
}

/// 시작 폼이 보낸 것.
///
/// **튜플이 아니라 구조체다.** 칸이 넷이 되면 전부 `Option<String>` 이라
/// 순서를 바꿔 넣어도 컴파일된다 — `SpawnOpts` 가 이미 같은 이유로 구조체다.
/// 그 자리를 여기서도 없앤다.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StartForm {
    /// 띄울 agent 프로필. 이것만 필수다.
    pub agent: String,
    /// 작업 디렉토리. 비우면 agent 설정을 따른다.
    pub cwd: Option<String>,
    /// 모델 이름. 비우면 agent 기본값.
    pub model: Option<String>,
    /// 세션 이름 — 목록에서 알아보기 위한 것.
    pub name: Option<String>,
    /// **이어받을 좌표** — supervisor 가 모르는, agent 프로필이 쓰던 id.
    ///
    /// 데몬의 `spawn --adopt` 로 간다. `session_id`(이미 아는 세션에
    /// 붙기)와 **다른 축이다**: 저쪽은 우리 레코드가 있고, 이쪽은
    /// **레코드가 없는 대화**를 끌어온다.
    pub adopt: Option<String>,
    /// 기동 권한 모드(protocol 13) — 비우면 agent 프로필의 모드.
    pub permission_mode: Option<String>,
    /// 고른 역할 폴더 이름 — 체크 칸 `r` 이 여럿 온다.
    pub roles: Vec<String>,
}

pub fn parse_start(url: &str) -> Option<StartForm> {
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    if !has_mark(&fields, START_MARK) {
        return None;
    }
    let get = |k: &str| -> Option<String> {
        fields
            .iter()
            .find_map(|kv| kv.strip_prefix(k))
            .map(percent_decode)
            .filter(|v| !v.trim().is_empty())
    };
    // agent 가 없으면 띄울 수 없다 — 빈 요청은 무시한다.
    let agent = get("a=")?;
    Some(StartForm {
        agent,
        cwd: get("c="),
        model: get("m="),
        name: get("n="),
        adopt: get("ad="),
        permission_mode: get("pm="),
        roles: fields
            .iter()
            .filter_map(|kv| kv.strip_prefix("r="))
            .map(percent_decode)
            .filter(|v| !v.trim().is_empty())
            .collect(),
    })
}

pub fn parse_input(url: &str) -> Option<String> {
    // 제출하면 `about:blank?t=<encoded>&agent-send=<nonce>` 가 된다.
    //
    // 표식을 **필드 이름**으로 찾는다. 종전에는 fragment 로 보냈는데, 브라우저가
    // 그것을 쿼리 값 안에 `%23` 로 인코딩해 넣어 **사용자 텍스트에 섞였다**
    // (실측: `?t=%EC%95%88%EB%85%95%23agent-send-0` → "안녕#agent-send-0").
    let q = query_of(url)?;
    let fields: Vec<&str> = q.split('&').collect();
    // 우리 폼이 아니면 무시한다 — 남의 `?t=` 까지 읽으면 사용자가 안 보낸
    // 말이 에이전트에게 간다.
    if !has_mark(&fields, FRAG_MARK) {
        return None;
    }
    let raw = fields.iter().find_map(|kv| kv.strip_prefix("t="))?;
    let text = percent_decode(raw);
    (!text.trim().is_empty()).then_some(text)
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        // `+` 는 폼 인코딩의 공백이다.
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 비우기 단추의 표식 — 보내기 · 중지와 **반드시 달라야 한다**: 같으면 비우려다 말을
/// 보내거나 턴을 멈춘다.
pub(crate) const CLEAR_MARK: &str = "agent-clear";

/// 비우기 단추를 확정해 눌렀나.
pub fn parse_clear_click(url: &str) -> bool {
    let Some(q) = query_of(url) else {
        return false;
    };
    has_mark(&q.split('&').collect::<Vec<_>>(), CLEAR_MARK)
}

#[cfg(test)]
mod clear_mark_tests {
    use super::*;

    #[test]
    fn the_click_is_read_from_its_own_mark_only() {
        assert!(parse_clear_click("about:blank#agent-clear=1"));
        assert!(parse_clear_click("about:blank%23agent-clear=3"));
        assert!(!parse_clear_click("about:blank#agent-stop=1"));
        assert!(!parse_clear_click("about:blank#agent-send=1&t=%2Fclear"));
    }
}
