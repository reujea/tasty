use super::*;
use crate::application::session_surface::{finish_msg, stopped_msg};
use crate::ports::supervisor::fake::FakeSupervisor;
use crate::test_support::send;

/// 스피너 키프레임 블록 — 프레임은 여기에만 있다(선언이 여러 줄로 접힌다).
fn spin_keyframes(h: &str) -> String {
    let at = h
        .find("@keyframes spinframes")
        .expect("스피너 키프레임이 없다 — 진행 표시가 안 움직인다");
    let rest = &h[at..];
    let end = rest.find("}}").expect("키프레임 블록이 안 닫혔다") + 2;
    rest[..end].to_string()
}

/// **폼 제출 뒤에도 일괄 삭제가 읽힌다**(실사용 결함 · 2026-10-01).
///
/// 세션 목록은 시작 폼이라 제출하면 주소에 `?a=…&agent-start=1` 이
/// 남는다. `query_of` 는 **쿼리를 먼저 읽고 fragment 를 버리므로**
/// (시작 단추를 살리려는 의도된 규칙), `location.hash` 만 바꾸면
/// 삭제 표식이 **영영 안 보인다** — 눌러도 아무 일이 없었다.
///
/// 보내는 쪽이 `?` 를 지운 뒤 fragment 를 싣는다. 아래는 그 결과로
/// 호스트가 실제로 받는 URL 이다(WKWebView 실측 — `#` 는 `%23` 로 온다).
#[test]
fn a_bulk_delete_survives_a_leftover_query() {
    // 고치기 전: `?` 가 남아 삭제가 안 보였다.
    let before = "about:blank?a=cc&agent-start=1%23s=aaa&s=bbb&agent-del=1&ok=1";
    assert!(
        parse_delete_many(before).is_none(),
        "이 모양이 읽히면 이 시험의 전제가 바뀐 것이다 — 보내는 쪽을 되돌려도 된다"
    );
    // 고친 뒤: `?` 를 지우고 보내므로 읽힌다.
    let after = "about:blank%23s=aaa&s=bbb&agent-del=1&ok=1";
    let (ids, confirmed) = parse_delete_many(after).expect("일괄 삭제가 안 읽힌다");
    assert_eq!(ids, vec!["aaa", "bbb"]);
    assert!(confirmed);
}

/// **입력칸이 여러 줄을 담는다** — `input` 은 줄바꿈을 못 담는다.
///
/// Enter=전송 / Shift+Enter=줄바꿈. textarea 는 **기본이 줄바꿈**이라
/// 전송을 명시로 걸지 않으면 칸만 늘어나고 아무것도 안 보내진다.
#[test]
fn the_composer_accepts_multiple_lines() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(
        h.contains(r##"<textarea name="t" id="t""##),
        "입력칸이 textarea 가 아니다 — 줄바꿈을 못 담는다"
    );
    // Enter 로 보내는 배선 — 없으면 칸만 늘어난다.
    let script = h.split("<script").nth(1).expect("스크립트가 없다");
    assert!(
        script.contains("e.key!=='Enter'||e.shiftKey"),
        "Enter 전송·Shift+Enter 줄바꿈 배선이 없다"
    );
    // IME 조합 중 Enter 는 **확정**이다 — 가로채면 한글 입력이 깨진다.
    assert!(
        script.contains("isComposing"),
        "IME 조합 중 Enter 를 안 비킨다 — 한글 입력이 깨진다"
    );
    // 높이가 안 늘면 두 번째 줄이 안 보인다.
    assert!(script.contains("function autosize()"), "자동 높이가 없다");
}

/// **입력칸 높이를 맞춰도 로그가 바닥에서 밀리지 않는다** — `#log` 는 남은 높이를 차지하므로 칸을
/// `auto` 로 접는 순간 로그의 스크롤이 깎인다. 접기 전에 바닥까지의 거리를 재고 높이를 정한 뒤 그
/// 거리로 되돌려야 한다(순서가 바뀌면 깎인 값을 재게 된다).
#[test]
fn autosize_keeps_the_log_distance_from_its_end() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let html = render(&c, false);
    let start = html.find("function autosize()").expect("자동 높이가 없다");
    let body = &html[start..start + html[start..].find("\n}").expect("함수 끝")];
    let measure = body.find("log.scrollHeight - log.scrollTop - log.clientHeight");
    let collapse = body.find("t.style.height='auto'");
    let restore = body.find("log.scrollTop = log.scrollHeight - log.clientHeight - end");
    assert!(
        matches!((measure, collapse, restore), (Some(m), Some(c), Some(r)) if m < c && c < r),
        "autosize 가 바닥까지의 거리를 접기 전에 재고 높이를 정한 뒤 되돌려야 한다: {body}"
    );
}

/// **세션 id 가 통째로 보인다** — 오류 문면과 맞춰 보려면 전체가 필요하다.
///
/// 실측(2026-10-02): *"No conversation found with session ID:
/// e827f3ec-cd50-4700-a827-147bb044d223"* 처럼 오류는 **전체 id** 로
/// 온다. 화면이 앞 8자만 보이면 사용자가 CLI 를 따로 띄워 맞춰야 한다.
#[test]
fn the_status_line_carries_the_whole_session_id() {
    let full = "e827f3ec-cd50-4700-a827-147bb044d223";
    let c = Convo {
        session_id: Some(full.into()),
        ..Default::default()
    };
    let line = crate::adapters::driving::statusline::render_html(&c, false);
    assert!(line.contains(full), "세션 id 가 잘렸다: {line}");
}

/// 에이전트 쪽 세션 id 는 supervisor id 와 **다를 때만** 따로 싣는다 — 이어받은 세션은
/// 사용자가 넣은 값이 이것이라, 안 보이면 이어받았는지 새로 띄웠는지 못 가른다.
#[test]
fn the_status_line_shows_the_agent_session_id_when_it_differs() {
    let mut c = Convo {
        session_id: Some("f2e706b0-eaec".into()),
        agent_session_id: Some("6d1d8650-fea4".into()),
        ..Default::default()
    };
    let line = crate::adapters::driving::statusline::render_html(&c, false);
    assert!(
        line.contains("6d1d8650-fea4"),
        "에이전트 세션 id 가 빠졌다: {line}"
    );
    assert!(
        line.contains("f2e706b0-eaec"),
        "이 탭의 세션 id 가 빠졌다: {line}"
    );
    c.agent_session_id = c.session_id.clone();
    let line = crate::adapters::driving::statusline::render_html(&c, false);
    assert_eq!(
        line.matches("f2e706b0-eaec").count(),
        1,
        "같은 id 를 두 번 실었다: {line}"
    );
}

/// **TUI 요소가 화면에 실린다** — 입력창 세로줄·캐럿·응답 마커.
///
/// 벤치마킹(2026-10-02 · 5개 저장소 렌더 코드 실측)으로 고른 규격이다:
/// 좌측 세로줄 `┃`(opencode·crush 등 다수파) · `> ` 캐럿(5개 중 4개) ·
/// `⏺` 응답 마커(기준점을 Claude Code 로 잡음).
///
/// ⚠️ CSS 만 고치고 마크업이 빠지면 **조용히 사라진다** — 둘 다 본다.
#[test]
fn the_chat_is_drawn_like_a_terminal() {
    let c = Convo {
        session_id: Some("s1".into()),
        turns: vec![Turn {
            sent: "hi".into(),
            got: "hello".into(),
            error: None,
            pending: false,
            at: None,
        }],
        ..Default::default()
    };
    let h = render(&c, false);
    // 입력창 — 세로줄과 캐럿이 **마크업에** 있어야 한다.
    assert!(h.contains(r##"class="bar""##), "입력창 세로줄이 없다");
    assert!(h.contains("&#x2503;"), "세로줄 글리프(┃)가 없다");
    assert!(h.contains(r##"class="caret""##), "입력 캐럿이 없다");
    // 응답 마커 — CSS content 로 건다(본문 텍스트를 오염시키지 않는다).
    //
    // **그 한 줄을 짚는다.** `.got::before` 와 `\23FA` 는 `.empty`·
    // `.pending .got` 에도 있어, 문서 전체에서 찾으면 **이 규칙을
    // 지워도 통과한다**(변이로 밟았다 — 이 저장소가 여러 번 겪은 형태).
    let got_rule = h
        .lines()
        .find(|l| l.trim_start().starts_with(".got::before"))
        .expect("응답 마커 규칙이 없다 — 답에 ⏺ 가 안 붙는다");
    assert!(
        got_rule.contains(r##"\23FA"##),
        "응답 마커가 ⏺ 가 아니다: {got_rule}"
    );
    // 스피너도 같은 축 — 애니메이션 줄을 직접 본다.
    //
    // ⚠️ **종전엔 찾아 놓고 `let _` 로 버렸다** — 줄이 있는지만 보고
    // **무엇인지는 안 봤다**. 이름만 바꿔도 통과하는 자리였다.
    let kf = spin_keyframes(&h);
    assert_eq!(
        kf.matches("content:").count(),
        4,
        "스피너가 4프레임이 아니다(Claude Code 실물): {kf}"
    );
    // 턴 블록의 좌측 경계 — 다수파 방식.
    assert!(h.contains("border-left"), "턴 좌측 경계가 없다");
}

/// **스크립트 안에서 같은 이름을 두 번 선언하지 않는다.**
///
/// 실사용 결함(2026-10-02 · 0.1.38): 전체선택을 넣으며 `var sel={}`
/// 를 추가했는데, 위쪽에 **`var sel=$('a')`(agent 선택 요소)** 가
/// 이미 있었다. 같은 IIFE 라 덮어썼고 `var` 는 함수 스코프라 선언이
/// 끌어올려져, 제출 시점에 `sel.value` 가 `undefined` 였다:
///
/// ```text
/// session.spawn failed: undefined (unknown_agent)
/// ```
///
/// ⚠️ 빌드도 테스트도 안 잡는다 — **JS 는 컴파일되지 않는다**.
#[test]
fn the_script_declares_each_name_once() {
    let a = Attachable {
        id: "aaaaaaaa-1111".into(),
        agent: "cc".into(),
        state: "stopped".into(),
        reclaimed: false,
        cwd: Some("/w".into()),
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);
    let script = h.split("<script").nth(1).expect("스크립트가 없다");
    // **IIFE 최상위 선언만 센다.** 함수 안의 `var t`·`var k` 는
    // 각자 스코프라 겹쳐도 무해하다 — 전부 세면 오탐이 10건 넘는다.
    // 최상위는 **들여쓰기가 없는** 줄로 가른다(이 스크립트의 관행).
    let mut names: Vec<&str> = Vec::new();
    for line in script.lines() {
        // 들여쓴 줄 = 블록 안 → 건너뛴다.
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let t = line;
        let Some(rest) = t.strip_prefix("var ") else {
            continue;
        };
        // `var a=1, b=2;` 를 쉼표로 가른다.
        for part in rest.split(',') {
            let n = part.trim().split(['=', ';', ' ']).next().unwrap_or("");
            if !n.is_empty() {
                names.push(n);
            }
        }
    }
    let mut seen = std::collections::BTreeMap::<&str, usize>::new();
    for n in &names {
        *seen.entry(n).or_default() += 1;
    }
    let dup: Vec<_> = seen.iter().filter(|(_, c)| **c > 1).collect();
    assert!(
        dup.is_empty(),
        "같은 이름을 두 번 선언했다 — 뒤의 것이 앞을 덮는다: {dup:?}"
    );
    // 분모 — 하나도 못 찾았으면 이 시험이 공허하다.
    assert!(
        names.len() >= 4,
        "최상위 변수 선언을 {}개만 봤다",
        names.len()
    );
}

/// **목록도 세션 id 를 통째로 보여 준다** — 상태줄과 같은 축.
///
/// 오류 문면이 전체 id 로 오므로 앞 8자만 보이면 대조하려고 CLI 를
/// 따로 띄워야 한다(사용자 요청 2026-10-02).
#[test]
fn the_list_shows_the_whole_session_id() {
    let full = "e827f3ec-cd50-4700-a827-147bb044d223";
    let a = Attachable {
        id: full.into(),
        agent: "cc".into(),
        state: "stopped".into(),
        reclaimed: false,
        cwd: None,
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);
    // 표의 id 칸에 통째로 있어야 한다(링크 href 말고 **본문**에).
    assert!(h.contains(&format!(">{full}<")), "목록의 세션 id 가 잘렸다");
}

/// **누를 수 있는 것은 단추로 보인다.**
///
/// 종전에는 `<a>` 를 색만 바꿔 썼더니 글자처럼 보여 누를 수 있는
/// 줄 몰랐다(사용자 보고 2026-10-02). 테두리·배경·여백이 다 있어야
/// 단추로 읽힌다 — 색만 바꾸면 여전히 링크다.
#[test]
fn the_actions_look_like_buttons() {
    let a = Attachable {
        id: "aaaaaaaa-1111".into(),
        agent: "cc".into(),
        state: "stopped".into(),
        reclaimed: false,
        cwd: None,
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);
    let css = h.split("<style").nth(1).expect("CSS 가 없다");
    // ⚠️ 규칙이 **여러 줄**이라 첫 줄만 보면 안 된다 — 닫는 `}` 까지
    // 본다(변이로 밟았다: `border:` 가 둘째 줄에 있어 빨개졌다).
    // **`td .btn{` 을 먼저 잡으면 안 된다** — 그쪽은 색만 바꾸는
    // 보정 규칙이라 테두리가 없다(변이로 밟았다). 공백 뒤가 아니라
    // **줄이 그것으로 시작하는** 자리를 찾는다.
    let start = css
        .find("\n .btn{")
        .map(|i| i + 1)
        .expect("단추 규칙이 없다");
    let end = start + css[start..].find('}').expect("단추 규칙이 안 닫혔다");
    let btn = &css[start..end];
    for prop in ["border:", "background:", "padding:"] {
        assert!(
            btn.contains(prop),
            "단추에 `{prop}` 가 없다 — 글자처럼 보인다: {btn}"
        );
    }
    // 조작들이 **실제로 그 클래스를 쓴다** — 규칙만 있고 안 붙이면
    // 아무것도 안 바뀐다.
    for (what, needle) in [
        ("새로고침", r##"class="btn refresh""##),
        ("붙기", r##"class="btn" href="#s="##),
    ] {
        assert!(h.contains(needle), "{what}이 단추가 아니다");
    }
}

/// 입력칸 위에서는 **텍스트 커서**가 나온다 — 어디를 눌러야 쓸 수
/// 있는지가 모양으로 드러난다.
#[test]
fn input_areas_show_a_text_cursor() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    let css = h.split("<style").nth(1).expect("CSS 가 없다");
    // **그 규칙을 짚는다.** `cursor:text` 는 시작 폼 CSS 에도 있어,
    // 문서 전체에서 찾으면 대화창 쪽을 지워도 통과한다(변이로 밟았다).
    // ⚠️ 규칙이 **여러 줄**이라 `.lines()` 로는 못 본다(`.btn` 과 같은
    // 축 — 그때도 첫 줄만 보다 빨개졌다). 닫는 `}` 까지 읽는다.
    let start = css
        .find("\n textarea{")
        .map(|i| i + 1)
        .expect("입력칸 규칙이 없다");
    let end = start + css[start..].find('}').expect("입력칸 규칙이 안 닫혔다");
    let rule = &css[start..end];
    assert!(
        rule.contains("cursor:text"),
        "입력칸에 텍스트 커서가 없다: {rule}"
    );
}

/// **물린 세션은 보이되 붙기 링크가 없다.**
///
/// 빼 버리면 *"다른 탭에 있다"* 와 *"없다"* 가 같은 화면이 된다
/// (사용자 보고 2026-10-02). 보여주되 못 고르게 한다.
#[test]
fn a_session_in_another_tab_shows_where_it_is_instead_of_a_link() {
    let taken = Attachable {
        id: "taken-1".into(),
        agent: "cc".into(),
        state: "running".into(),
        reclaimed: false,
        cwd: None,
        taken_by: Some(7),
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let free = Attachable {
        id: "free-2".into(),
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
        ..taken.clone()
    };
    let h = render_start(&[], &[taken, free], &[], "", None);

    // 물린 것: 어디 있는지 적고 붙기 링크는 없다.
    assert!(h.contains(r##"class="inuse""##), "어느 탭인지 안 적는다");
    assert!(
        !h.contains(r##"href="#s=taken-1"##),
        "물린 세션에 붙기 링크가 있다 — 누르면 앞 탭의 짝이 끊긴다"
    );
    // **양성 대조** — 빈 것은 여전히 고를 수 있어야 한다.
    // 없으면 *"링크를 아예 안 낸다"* 도 통과한다.
    assert!(
        h.contains(r##"href="#s=free-2"##),
        "빈 세션까지 못 고르게 됐다"
    );
    // 둘 다 목록에는 있다.
    assert!(
        h.contains("taken-1") && h.contains("free-2"),
        "세션이 빠졌다"
    );
}

/// 새로고침 단추가 **화면에 실린다** — 파서만 있고 단추가 없으면
/// 사용자는 부를 길이 없다.
#[test]
fn the_refresh_button_is_on_the_list() {
    let a = Attachable {
        id: "aaaaaaaa-1111".into(),
        agent: "cc".into(),
        state: "stopped".into(),
        reclaimed: false,
        cwd: Some("/w".into()),
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);
    assert!(h.contains(r##"id="refresh""##), "새로고침 단추가 없다");
    // 그 단추가 **그 표식으로** 보내야 한다.
    assert!(
        h.contains(&format!("#{REFRESH_MARK}=1")),
        "새로고침 단추가 다른 곳으로 보낸다"
    );
}

/// **새로고침은 다른 표식을 쓴다** — 읽기가 파괴로 새면 안 된다.
///
/// 표식이 겹치면 목록을 다시 받으려다 **세션이 지워진다**.
#[test]
fn refresh_never_shares_a_mark_with_a_destructive_action() {
    assert_ne!(REFRESH_MARK, DELETE_MARK, "새로고침과 삭제가 같은 표식이다");

    // 새로고침 URL 이 파괴 갈래로 읽히면 안 된다.
    let rf = format!("about:blank?{REFRESH_MARK}=1");
    assert!(parse_delete_many(&rf).is_none(), "새로고침을 삭제로 읽었다");

    // 반대도 — 파괴 URL 을 새로고침으로 읽으면 그 조작이 **조용히 사라진다**.
    let del = format!("about:blank?s=aaa&{DELETE_MARK}=1&ok=1");
    assert!(!parse_refresh(&del), "삭제를 새로고침으로 읽었다");

    // 양성 대조 — 없으면 *"항상 false"* 도 통과한다.
    assert!(parse_refresh(&rf), "새로고침을 못 읽는다");
}

/// **여러 세션을 한 번에 지운다** — `s=` 가 여러 개 온다.
#[test]
fn several_sessions_delete_in_one_go() {
    let url = format!("about:blank?s=aaa&s=bbb&s=ccc&{DELETE_MARK}=1&ok=1");
    let (ids, confirmed) = parse_delete_many(&url).expect("골랐어야 한다");
    assert_eq!(ids, vec!["aaa", "bbb", "ccc"], "세 건을 다 안 모았다");
    assert!(confirmed, "ok=1 인데 확인이 아니라고 했다");
}

/// `ok=1` 이 없으면 **묻는 단계** — 다중도 단건과 같은 규칙이다.
#[test]
fn a_multi_delete_without_ok_is_still_the_asking_step() {
    let url = format!("about:blank?s=aaa&s=bbb&{DELETE_MARK}=1");
    let (ids, confirmed) = parse_delete_many(&url).expect("골랐어야 한다");
    assert_eq!(ids.len(), 2);
    assert!(
        !confirmed,
        "확인 없이 지우려 한다 — 되돌릴 수 없는 조작이다"
    );
}

/// 빈 값 하나가 **나머지를 버리게 하면 안 된다**.
///
/// 사용자는 무엇이 지워졌는지 모르게 된다.
#[test]
fn one_blank_id_does_not_throw_away_the_rest() {
    let url = format!("about:blank?s=aaa&s=&s=bbb&{DELETE_MARK}=1&ok=1");
    let (ids, _) = parse_delete_many(&url).expect("골랐어야 한다");
    assert_eq!(ids, vec!["aaa", "bbb"], "빈 값 때문에 나머지를 잃었다");
}

/// 같은 id 가 두 번 오면 **한 번만** — 두 번째는 `not_found` 라
/// 성공한 삭제가 실패로 보고된다.
#[test]
fn a_repeated_id_is_deleted_once() {
    let url = format!("about:blank?s=aaa&s=aaa&{DELETE_MARK}=1&ok=1");
    let (ids, _) = parse_delete_many(&url).expect("골랐어야 한다");
    assert_eq!(ids, vec!["aaa"], "중복을 안 접었다");
}

/// 전부 비면 **고르지 않는다** — 빈 삭제는 조작이 아니다.
#[test]
fn all_blank_selects_nothing() {
    let url = format!("about:blank?s=&s=%20&{DELETE_MARK}=1&ok=1");
    assert!(
        parse_delete_many(&url).is_none(),
        "빈 것만 왔는데 지우려 한다"
    );
}

/// 표식이 없으면 **이 surface 의 것이 아니다**(단건과 같은 축).
#[test]
fn a_multi_delete_needs_the_delete_mark() {
    assert!(parse_delete_many("about:blank?s=aaa&s=bbb&ok=1").is_none());
}

/// 단건도 **같은 경로로** 돈다 — 갈래를 둘로 두지 않는다.
#[test]
fn a_single_delete_goes_through_the_same_path() {
    let url = format!("about:blank?s=aaa&{DELETE_MARK}=1&ok=1");
    let (ids, confirmed) = parse_delete_many(&url).expect("골랐어야 한다");
    assert_eq!(ids, vec!["aaa"]);
    assert!(confirmed);
}

/// 폼이 보낸 fragment 에서 **원문 그대로** 꺼낸다.
#[test]
fn the_form_payload_comes_back_as_typed() {
    let url = "about:blank?t=%EC%95%88%EB%85%95%20%ED%95%98%EC%84%B8%EC%9A%94&agent-send=7";
    assert_eq!(parse_input(url).as_deref(), Some("안녕 하세요"));
}

/// 이 surface 가 낸 것이 **아니면 무시한다**.
///
/// 다른 링크 클릭까지 입력으로 읽으면 사용자가 **안 보낸 말**이 에이전트에게 간다.
#[test]
fn a_navigation_we_did_not_emit_is_ignored() {
    assert_eq!(parse_input("https://example.com/page#section"), None);
    // `?t=` 가 있어도 `n=` 이 없으면 우리 폼이 아니다.
    // 표식이 없으면 남의 `?t=` 다.
    assert_eq!(parse_input("https://example.com/s?t=검색어"), None);

    // **실측 회귀** — fragment 로 보내던 시절 브라우저가 `#` 를 `%23` 로
    // 인코딩해 값에 넣었다. 그 모양은 표식 필드가 없으므로 이제 거절된다.
    assert_eq!(
        parse_input("about:blank?t=%EC%95%88%EB%85%95%23agent-send-0"),
        None,
        "표식이 값에 섞인 옛 모양을 입력으로 받았다"
    );
}

/// **글에 `#` 이 든 입력도 간다** — 마크다운 제목·해시태그. 글 속 `#` 은 `%23` 으로 인코딩돼
/// fragment 안에 들어오는데, 그것을 구분자로 잘라 읽으면 표식이 잘려 입력이 조용히 버려졌다
/// (실측 2026-10-21: 한 탭에서 `%23` 이 든 전송 5건이 모두 무시됐다).
#[test]
fn a_message_with_a_hash_sign_is_sent() {
    let url = "about:blank#agent-send=7&t=%23%23%20%EC%A0%9C%EB%AA%A9%0A-%20a";
    assert_eq!(parse_input(url).as_deref(), Some("## 제목\n- a"));
    // 단추 링크처럼 fragment 자체가 `%23` 으로 오는 모양도 그대로 읽는다.
    let url = "about:blank%23agent-send=8&t=%23tag";
    assert_eq!(parse_input(url).as_deref(), Some("#tag"));
}

/// 빈 입력은 보내지 않는다 — 공백만 쳐도 턴이 돌면 안 된다.
#[test]
fn blank_input_is_not_a_message() {
    assert_eq!(parse_input("x?t=%20%20&agent-send=1"), None);
    assert_eq!(parse_input("x?t=&agent-send=1"), None);
}

/// `+` 는 공백이다(폼 인코딩).
#[test]
fn plus_decodes_to_a_space() {
    assert_eq!(parse_input("x?t=a+b&agent-send=1").as_deref(), Some("a b"));
}

/// **nonce 가 매번 올라간다** — 같은 말을 두 번 보낼 수 있어야 한다.
///
/// fragment 가 같으면 WebView 가 navigation 을 **아예 시도하지 않아**
/// 두 번째 입력이 사라진다.
#[test]
fn the_nonce_advances_so_the_same_text_can_be_sent_twice() {
    let sup = FakeSupervisor::new();
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };

    send(&sup, &mut c, "같은 말");
    let first = c.nonce;
    send(&sup, &mut c, "같은 말");

    assert_ne!(first, c.nonce, "nonce 가 안 올라가 두 번째가 사라진다");
    assert_eq!(c.turns.len(), 2, "두 턴이 다 남아야 한다");
}

/// 실패를 **조용히 삼키지 않는다** — 답이 안 온 것과 실패한 것은 다르다.
#[test]
fn a_failed_turn_keeps_its_reason() {
    let sup = FakeSupervisor::failing("데몬 없음");
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };

    send(&sup, &mut c, "안녕");

    let t = c.turns.last().expect("턴이 남아야 한다");
    assert!(
        t.error.as_deref().is_some_and(|e| e.contains("데몬 없음")),
        "사유가 사라졌다"
    );
    let h = render(&c, false);
    assert!(h.contains("데몬 없음"), "화면에 사유가 안 보인다");
}

/// 세션이 안 이어진 탭에서 보내면 **그렇다고 답한다**.
#[test]
fn sending_without_a_session_says_so() {
    let sup = FakeSupervisor::new();
    let mut c = Convo::default();

    send(&sup, &mut c, "안녕");

    assert!(
        c.turns[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("이어진 세션이 없다"))
    );
    assert!(sup.methods().is_empty(), "세션도 없는데 데몬을 불렀다");
}

/// 사용자 입력이 **문서를 깨지 못한다**.
#[test]
fn user_text_cannot_break_the_document() {
    let c = Convo {
        session_id: Some("s1".into()),
        turns: vec![Turn {
            sent: "<script>alert(1)</script>".into(),
            got: "<b>bold</b>".into(),
            error: None,
            pending: false,
            at: None,
        }],
        nonce: 0,
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(!h.contains("<script>alert"), "태그가 그대로 들어갔다");
    assert!(
        h.contains(r"\u003cscript>alert(1)"),
        "상태 JSON 이 `<` 를 안 바꿨다"
    );
}

/// **브라우저가 실제로 만든 URL** 을 파서가 읽나 (0.1.16 회귀).
///
/// # 왜 이 시험이 없어서 뚫렸나
///
/// 종전 시험은 전부 URL 을 **손으로 적었다**(`?t=hi&agent-send=1`).
/// 그런데 표식을 `<input name>` 으로 쓰면 브라우저가 그 이름을
/// 퍼센트 인코딩한다 — 이름이 `agent-send=` 였으므로 `=` 가 `%3D` 가
/// 되어 **`agent-send%3D=0`** 이 왔다. 손으로 적은 모양과 달라
/// 판정이 어긋났고, 입력이 통째로 무시돼 **흰 화면**이 났다.
///
/// 아래 URL 은 실측 로그에서 그대로 옮긴 것이다.
/// **다시 손으로 예쁘게 적지 않는다** — 그러면 이 시험이 원래
/// 막으려던 것을 도로 못 보게 된다.
#[test]
fn the_parser_reads_the_url_a_browser_actually_submits() {
    // 실측(2026-09-29 · plugins-logs): 보내기 · 시작 둘 다.
    assert_eq!(
        parse_input("about:blank?t=hi&agent-send%3D=0%23").as_deref(),
        Some("hi"),
        "브라우저가 보낸 모양을 못 읽는다 — 입력이 사라진다(흰 화면)"
    );
    assert_eq!(
        parse_start("about:blank?a=cc&c=&agent-start=1%23").map(|f| f.agent),
        Some("cc".to_string()),
        "시작 URL 을 못 읽는다"
    );

    // 인코딩 안 된 형태도 계속 받는다 — 링크(`<a href>`)는 우리가 쓴
    // 문자열 그대로 온다. 둘 다 서야 한다.
    assert_eq!(parse_input("x?t=hi&agent-send=1").as_deref(), Some("hi"));

    // **남의 폼은 여전히 무시한다.** 표식을 느슨하게 보다가 이것이
    // 열리면 사용자가 안 보낸 말이 에이전트에게 간다.
    assert_eq!(parse_input("x?t=hi&other=1"), None, "남의 폼을 읽었다");
    assert_eq!(
        parse_input("x?t=hi&agent-sendx=1"),
        None,
        "접두사만 같은 필드를 표식으로 읽었다"
    );
}

///  **보낸 말이 즉시 보이고 답 자리에 진행 표시가 뜬다**(0.1.22).
///
/// # 왜 페이지가 그리나
///
/// `send` 가 plugin 쪽에서 **동기 블로킹**이라, 답이 올 때까지 화면을
/// 다시 실을 수 없다(0.1.17 실측: 두 `push` 가 3ms 차이로 함께 났고
/// 제출과는 3.7초 떨어져 있었다). 그 사이를 **페이지가 스스로** 메운다.
///
/// ⚠️ 스크립트는 시험이 실행하지 못한다. 그래서 **걸이와 재료**가
/// 있는지를 값으로 단언한다 — 없으면 스크립트가 잡을 것이 없다.
#[test]
fn the_chat_can_show_a_pending_turn_before_the_answer_arrives() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);

    // 스크립트가 붙일 자리 + 붙일 모양.
    assert!(h.contains(r#"id="log""#), "대화 걸이가 없다");
    // 진행 중 턴은 plugin 이 판정한 뒤 스크립트로 붙인다(`send_queue`) — 그 모양을 본다.
    let s = crate::domain::send_queue::settle_msg(Some(1), &[], Some("x"));
    let page = include_str!("../../../../assets/app.js");
    assert!(
        page.contains("'turn' + (t.pending ? ' pending' : '')"),
        "진행 중 턴의 class 가 없다"
    );
    assert!(
        page.contains(r#"<span class="spin"></span>"#),
        "진행 표시가 없다"
    );
    // 문구는 **번역**돼야 한다 — 박아 두면 영어 로케일에서 한국어가 뜬다.
    assert_eq!(
        s["thinking"],
        crate::domain::i18n::t("sup.chat.thinking") + "…",
        "진행 문구가 없다"
    );
    assert!(
        h.contains(r#"id="queue""#),
        "대기 칸이 없다 — 친 말을 올릴 자리가 없다"
    );
    // CSS 가 없으면 스피너가 안 움직인다(정지한 글리프는 오류처럼 보인다).
    assert!(
        h.contains("@keyframes spinframes"),
        "진행 표시가 안 움직인다"
    );
    // **Claude Code 실물 4프레임**(바이너리 실측 · 2.1.285):
    // 가운데 글자만 `| / — \` 로 돌고 양옆 `·` 는 고정이다.
    // **스피너 `content` 그 줄에서** 센다 — 문서 전체에서 찾으면
    // 가드 자신의 문자열에 걸려 통과한다(변이로 밟았다).
    let frames = spin_keyframes(&h);
    for g in ["\\B7|", "/\\B7", "\\2014", "\\\\\\B7"] {
        assert!(frames.contains(g), "진행 프레임 {g} 가 없다: {frames}");
    }
    // **프레임을 폭으로 밀지 않는다.** `·`·`—` 는 이 폰트에서 1ch 가
    // 아니라, 한 줄로 이어 `translateX` 로 밀면 창이 글리프와 어긋나
    // 점 하나만 보였다(실측 2026-10-12). 프레임은 `content` 로 갈아 낀다.
    // 주석에도 그 낱말이 있으므로 **규칙 줄**에서만 본다.
    let spin_rules: String = h
        .lines()
        .filter(|l| l.trim_start().starts_with(".spin"))
        .collect();
    assert!(
        !frames.contains("transform") && !spin_rules.contains("transform"),
        "스피너가 다시 폭 이동으로 돌아갔다 — 글리프가 어긋나 안 보인다"
    );

    //  **배선을 본다** — 함수가 있어도 **안 부르면** 아무 일도 없다.
    // 처음에 재료만 세다가 `optimistic(v);` 를 지우는 변이가 통과했다
    // (2026-09-30). 이 저장소가 여러 번 밟은 형태다.
    assert!(h.contains("function stage("), "그리는 함수가 없다");
    let called = h
        .lines()
        .any(|l| l.contains("stage(v, n)") && !l.trim_start().starts_with("function"));
    assert!(called, "그리는 함수를 아무도 안 부른다 — 화면은 그대로다");
}

///  **보낸 말은 이스케이프 없이 넣는다** — `textContent` 가 맡는다.
///
/// 스크립트가 `innerHTML` 로 사용자 텍스트를 넣으면 `<script>` 가 든
/// 입력이 그대로 실행된다. 이 가드는 그 실수를 잡는다.
/// 중지를 보낸 뒤 실패로 끝난 턴은 **멈춘 턴**이다 — 실패 사유를 안 남기고 따로 센다.
/// 중지 없이 실패하면 그대로 실패다(아래 짝). 표시는 턴 하나에만 쓰이고 거둬진다.
#[test]
fn a_turn_failed_after_stop_is_stopped_not_failed() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let a = begin_turn(&mut c, "slow");
    c.stop_requested = true;
    finish_turn(
        &mut c,
        a,
        Err("턴이 실패로 끝났다: error_during_execution".into()),
    );
    assert!(c.stopped.contains(&a), "멈춘 턴으로 안 셌다");
    assert!(c.turns[a].error.is_none(), "멈춘 턴에 실패 사유가 남았다");
    assert!(!c.stop_requested, "중지 표시를 안 거뒀다");
    // 문서에는 앱 코드가 함께 실려 마크업 낱말로는 가를 수 없다 — 그릴 값을 본다.
    let st = crate::adapters::driving::client::log_state(&c);
    assert_eq!(st["turns"][a]["stopped"], true, "멈춘 턴을 따로 안 그린다");
    assert!(st["turns"][a]["error"].is_null(), "멈춘 턴을 실패로 그린다");
    assert!(
        crate::adapters::driving::statusline::render_html(&c, false)
            .contains(&crate::domain::i18n::t("sup.status.last_stopped"))
    );
    assert_eq!(stopped_msg(a)["cls"], "got stopped");

    // 짝 — 중지 없이 실패하면 실패다.
    let b = begin_turn(&mut c, "x");
    finish_turn(&mut c, b, Err("소켓 없음".into()));
    assert_eq!(c.turns[b].error.as_deref(), Some("소켓 없음"));
    assert!(!c.stopped.contains(&b));
    assert!(
        crate::adapters::driving::statusline::render_html(&c, false)
            .contains(&crate::domain::i18n::t("sup.status.last_err"))
    );

    // 중지를 보냈는데 턴이 먼저 성공으로 끝나면 답이 남고 표시는 거둬진다.
    let d = begin_turn(&mut c, "y");
    c.stop_requested = true;
    finish_turn(&mut c, d, Ok("다 했다".into()));
    assert_eq!(c.turns[d].got, "다 했다");
    assert!(!c.stopped.contains(&d) && !c.stop_requested);
}

/// 이전 대화 줄 — 보관이 없으면 숨고, 남은 수를 싣고, 다 펼치면 링크만 숨는다.
/// 링크의 `hidden` 은 CSS 가 이기지 못하게 `[hidden]` 규칙을 둔다(그 규칙 없이 (0) 이 남았다).
#[test]
fn the_older_row_counts_what_is_left_and_hides_when_done() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    assert_eq!(
        crate::adapters::driving::client::log_state(&c)["older"]["archived"],
        0
    );
    c.archived = 3;
    c.older = (0..2)
        .map(|i| Turn {
            sent: format!("old {i}"),
            got: String::new(),
            pending: false,
            at: None,
            error: None,
        })
        .collect();
    let st = crate::adapters::driving::client::log_state(&c);
    assert_eq!(st["older"]["archived"], 3);
    assert_eq!(st["older"]["items"].as_array().map(Vec::len), Some(2));
    // 숨김 · 남은 수 · 다음 번호는 앱이 그 값으로 낸다(jsdom 검사).
    let app = include_str!("../../../../assets/app.js");
    assert!(app.contains("hidden=${o.archived === 0}"));
    assert!(app.contains("remaining = Math.max(0, o.archived - o.items.length)"));
    assert!(app.contains("hidden=${remaining === 0}"));
    assert!(render(&c, false).contains(".older .more[hidden]{display:none}"));
}

/// 중지는 단추 링크(`#agent-stop=<n>`)로 온다 — 보내기·새로고침과 갈린다.
#[test]
fn a_stop_request_is_told_apart_from_sending() {
    assert_ne!(STOP_MARK, FRAG_MARK, "중지와 보내기가 같은 표식이다");
    assert_ne!(STOP_MARK, REFRESH_MARK, "중지와 새로고침이 같은 표식이다");
    assert!(parse_stop("about:blank%23agent-stop=3"));
    assert!(parse_stop("about:blank?agent-stop=1"));
    assert!(!parse_stop("about:blank?t=stop&agent-send=4"));
    assert!(!parse_stop("about:blank?agent-stopx=1"));
    assert_eq!(
        parse_input("about:blank%23agent-stop=3"),
        None,
        "중지가 말로 나간다"
    );
}

/// 중지 단추는 **늘 문서에 있고** 도는 턴이 있을 때만 보인다. 판정은 그릴 때의 값이
/// 아니라 누르는 순간의 문서다 — 보낸 턴은 스크립트로 붙어 문서를 다시 싣지 않는다
/// (실측 2026-10-16: 그릴 때 값으로 정했더니 보낸 직후 단추가 없었다).
#[test]
fn the_stop_button_follows_the_document_not_the_render() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    for busy in [false, true] {
        let h = render(&c, busy);
        assert!(h.contains(r#"id="stop""#), "busy={busy}: 중지 단추가 없다");
        assert!(h.contains("'agent-stop='+sn"), "중지가 표식을 안 낸다");
        assert!(
            h.contains("e.key!=='Escape'||!running()"),
            "Esc 가 도는 턴을 안 본다"
        );
        assert!(
            h.contains("location.hash='agent-stop='+sn; t.focus();"),
            "멈춘 뒤 입력칸으로 안 돌아간다"
        );
        assert!(!h.contains("var busy"), "그릴 때의 값으로 판정한다");
    }
    let h = render(&c, false);
    // 보이기는 두 신호(보낸 턴 · 다른 곳에서 시작된 턴)를 CSS 가 본다.
    assert!(
        h.contains("button.stop{color:#e06c75;display:none}"),
        "늘 보인다"
    );
    assert!(
        h.contains("body:has(.turn.pending) button.stop"),
        "보낸 턴에 안 보인다"
    );
    assert!(
        h.contains("body:has(#act:not([hidden])) button.stop"),
        "바깥 턴에 안 보인다"
    );
    // 그 두 신호를 실제로 만드는 쪽과 이름이 같아야 한다.
    assert!(
        include_str!("../../../../assets/app.js")
            .contains("'turn' + (t.pending ? ' pending' : '')")
    );
    assert_eq!(
        crate::application::activity::show_msg(true)["type"],
        "activity.show"
    );
}

/// 대화의 줄 높이는 터미널의 Claude Code 에 맞춘 1.3 이다 — 1.6 이면 같은 답이 1/3 성기다.
/// 입력칸의 최대 높이(`em`)도 같은 값으로 10 줄을 잰다.
#[test]
fn chat_text_is_set_as_tight_as_the_terminal() {
    let c = Convo {
        session_id: Some("s".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(
        h.contains("body{font:13px/1.3 "),
        "대화 줄 높이가 터미널보다 넓다"
    );
    assert!(
        h.contains("line-height:1.3;max-height:13em;"),
        "입력칸 줄 높이와 10 줄 상한이 어긋난다"
    );
}

#[test]
fn the_pending_turn_puts_user_text_through_text_content() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(
        h.contains("d.textContent=v"),
        "친 말을 textContent 로 안 넣는다 — 입력이 HTML 로 실행될 수 있다"
    );
    let s = crate::adapters::driving::messages::script_html();
    let app = include_str!("../../../../assets/app.js");
    assert!(
        app.contains("${auto ? L.auto : t.sent}</div>") && s.contains("d.textContent=v"),
        "보낸 말·쌓인 말을 텍스트로 안 넣는다"
    );
    // 사용자 값이 HTML 로 해석되는 경로가 없어야 한다.
    assert!(
        !s.contains("innerHTML=m.sent") && !s.contains("innerHTML=v") && !app.contains("innerHTML"),
        "사용자 값이 innerHTML 로 들어간다"
    );
}

/// 표식 이름에 `=` 를 넣지 않는다 — 폼 필드 이름은 브라우저가
/// **퍼센트 인코딩**해서 보낸다(`agent-send=` → `agent-send%3D=`).
/// 0.1.15 가 그것으로 입력을 통째로 잃었다(흰 화면).
#[test]
fn the_send_mark_survives_the_browser() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    assert!(
        render(&c, true).contains(r#"name="agent-send""#),
        "표식 필드가 없다"
    );
    assert!(
        !render(&c, true).contains(r#"name="agent-send=""#),
        "표식 이름에 `=` 가 들어 있다 — 브라우저가 %3D 로 보낸다"
    );
}

///  **답은 그 자리만 고친다 — 문서를 다시 싣지 않는다**(0.1.24).
///
/// 재적재는 (1) 깜빡이고 (2) **입력 중이던 텍스트를 지우고** (3) 스크롤을
/// 되돌린다. 실사용에서 셋 다 보고됐다.
#[test]
fn the_finish_script_touches_only_that_turn() {
    let ok = finish_msg(2, &Ok("반가워".into()));
    assert_eq!(
        ok,
        serde_json::json!({ "type": "turn.settle", "at": 2, "cls": "got", "text": "반가워" })
    );
    let page = include_str!("../../../../assets/app.js");
    // 자리를 index 로 찾는다 — 마지막 턴을 고치면 답이 엉뚱한 질문에 붙는다.
    assert!(
        page.contains("t = log.turns[m.at];"),
        "자리를 index 로 안 찾는다"
    );
    //  **문서를 다시 싣는 흔적이 없어야 한다.**
    assert!(!page.contains("document.write"), "문서를 다시 쓴다");
    // 텍스트는 textContent 로 — 답에 든 태그가 실행되면 안 된다.
    assert!(
        page.contains("g.textContent = props.text;"),
        "답을 textContent 로 안 넣는다"
    );
    // 대기 표시를 지운다.
    assert!(
        page.contains("t.pending = false;"),
        "대기 class 를 안 지운다"
    );
}

///  **실패도 그 자리에 보인다** — 조용히 삼키면 답이 안 온 것과 같아진다.
#[test]
fn a_failed_turn_shows_its_reason_in_place() {
    let err = finish_msg(0, &Err("소켓 없음".into()));
    assert_eq!(err["cls"], "err", "실패인데 오류 class 가 아니다");
    assert_eq!(err["text"], "소켓 없음", "사유가 없다");
}

///  **답에 든 따옴표·태그는 데이터로 간다** — 호스트가 JSON 으로 싣고 처리기가 글로 넣는다.
#[test]
fn an_answer_with_quotes_or_tags_stays_safe() {
    for nasty in [
        "it's \"quoted\"",
        "</script><script>alert(1)</script>",
        "line1\nline2",
    ] {
        assert_eq!(finish_msg(1, &Ok(nasty.into()))["text"], nasty);
    }
}

///  **턴을 열면 대기, 답이 오면 그 자리가 채워진다**(0.1.23).
///
/// 워커 경로는 스레드라 시험이 순서를 못 본다. 그 대신 **두 함수가
/// 맞물리는지**를 여기서 본다 — 배선이 빠지면 화면이 영영 대기다.
/// 상한을 넘긴 턴은 **열린 채** 문구만 바뀐다 — 닫힌 턴은 표시하지 않는다.
#[test]
fn an_overdue_turn_stays_open_with_its_own_label() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let a = begin_turn(&mut c, "a");
    let b = begin_turn(&mut c, "b");
    finish_turn(&mut c, b, Ok("done".into()));
    assert!(!mark_overdue(&mut c, b), "닫힌 턴을 기다리게 했다");
    assert!(mark_overdue(&mut c, a));
    assert!(!mark_overdue(&mut c, a), "같은 자리를 두 번 올렸다");
    assert!(c.turns[a].pending, "상한을 넘겼다고 닫았다");
    let h = render(&c, true);
    assert!(
        h.contains(&escape(&crate::domain::i18n::t("sup.chat.overdue"))),
        "다시 그리면 대기 문구가 사라진다"
    );
    assert_eq!(settle_overdue(&mut c, Ok("late".into())), Some(a));
    assert_eq!(c.turns[a].got, "late");
    assert!(c.overdue.is_empty());
    assert_eq!(settle_overdue(&mut c, Ok("x".into())), None);
}

#[test]
fn a_turn_opens_pending_and_closes_when_the_answer_lands() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };

    let a = begin_turn(&mut c, "첫 말");
    let b = begin_turn(&mut c, "둘째 말");
    //  **연달아 열 수 있다** — 답을 기다리는 동안에도.
    assert_eq!(c.turns.len(), 2, "두 번째를 못 열었다");
    assert!(
        c.turns[a].pending && c.turns[b].pending,
        "열자마자 대기가 아니다"
    );
    assert_ne!(a, b, "두 턴이 같은 자리를 쓴다 — 답이 덮어쓴다");

    //  **도착 순서가 보낸 순서와 달라도** 각자 자리에 붙는다.
    finish_turn(&mut c, b, Ok("둘째 답".into()));
    assert!(!c.turns[b].pending, "닫히지 않았다");
    assert!(c.turns[a].pending, "안 온 답까지 닫혔다");
    assert_eq!(c.turns[b].got, "둘째 답");
    assert_eq!(c.turns[a].got, "", "엉뚱한 턴에 답이 붙었다");

    finish_turn(&mut c, a, Err("소켓 없음".into()));
    assert_eq!(c.turns[a].error.as_deref(), Some("소켓 없음"));
    assert!(!c.turns[a].pending);
}

/// 대기 중인 턴은 **문서를 다시 실어도** 진행 표시가 남는다.
///
///  스크립트가 그린 것은 재적재에 사라진다. 서버가 같은 모양을
/// 내야 이어진다 — 안 그러면 답을 기다리는 동안 표시가 깜빡 사라진다.
#[test]
fn a_pending_turn_keeps_its_indicator_across_a_reload() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    begin_turn(&mut c, "안녕");
    //  **낱말이 아니라 속성 자리를 본다.** 스크립트가 같은 낱말을
    // 늘 담고 있어, 문자열만 세면 어느 쪽이든 통과한다(이 시험을
    // 처음 그렇게 적었다가 헛통과했다 — `disabled` 때와 같은 함정).
    let st = crate::adapters::driving::client::log_state(&c);
    assert_eq!(
        st["turns"][0]["pending"], true,
        "다시 실으니 대기 표시가 사라졌다"
    );
    assert_eq!(st["turns"][0]["sent"], "안녕", "보낸 말이 사라졌다");
    let app = include_str!("../../../../assets/app.js");
    assert!(
        app.contains(r#"if (t.pending) {"#) && app.contains(r#"<span class="spin"></span>"#),
        "진행 표시가 없다"
    );
    assert!(
        render(&c, false).contains(r#""pending":true"#),
        "문서 상태에 대기가 안 실렸다"
    );

    // 답이 오면 그 자리가 **평범한 턴**이 된다.
    finish_turn(&mut c, 0, Ok("반가워".into()));
    let h2 = render(&c, false);
    assert!(
        !h2.contains(r#"class="turn pending">"#),
        "답이 왔는데 대기 표시가 남았다"
    );
    assert!(h2.contains("반가워"), "답이 안 보인다");
}

///  **입력칸은 답을 기다리는 동안에도 열려 있다**(0.1.23 에 뒤집혔다).
///
/// # 왜 뒤집혔나
///
/// 종전에는 *"겹쳐 보내면 줄을 선다"* 며 막았다. 그 전제는
/// `send_input` 이 plugin 루프에서 **동기로** 돌던 것이었다 — 그때는
/// 두 번째 메시지를 받을 수단 자체가 없었다.
///
/// 0.1.23 이 전송을 워커로 옮겨 루프가 안 막힌다. 데몬도 턴 중
/// 주입을 허용한다(`inject_while_streaming: true` — claude 는 stdin 이
/// 열려 있다). ⇒ 막을 이유가 양쪽 다 사라졌다.
///
/// ⚠️ **`busy` 인자는 남겨 둔다** — 상태줄이 그것으로 *"기다리는 중"* 을
/// 말한다. 입력 잠금과 진행 표시는 다른 축이다.
#[test]
fn the_input_stays_open_while_an_answer_is_pending() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    for busy in [true, false] {
        let h = render(&c, busy);
        //  `disabled` 낱말만 세면 안 된다 — 종전 스크립트가 그것을
        // 늘 담고 있어 헛통과했다(그때 기록). **속성 자리**를 본다.
        assert!(
            !h.contains(r#"placeholder="메시지를 입력한다" disabled"#),
            "busy={busy} 인데 입력이 막혀 있다 — 채팅처럼 연달아 못 쓴다"
        );
        // 스크립트도 막으면 안 된다.
        assert!(
            !h.contains("t.disabled=true"),
            "busy={busy}: 스크립트가 입력을 잠근다"
        );
    }
}

/// 나머지 축은 그대로다 — 상태줄이 기다림을 말한다.
#[test]
fn the_status_line_still_says_when_it_waits() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    // 도는 중 표시는 **상태줄**이 맡는다(0.1.17). 종전에는 폼 위
    // 별도 div 였는데, 상태줄이 같은 것을 말하므로 한 곳으로 모았다.
    assert!(
        crate::adapters::driving::statusline::render_html(&c, true)
            .contains(r#"<span class="wait">"#),
        "도는 중 표시가 없다"
    );
    assert!(
        !render(&c, false).contains(r#"<span class="wait">"#),
        "한가한데 기다린다고 한다"
    );
}

/// 상태줄에 사용량 칸이 있다 — 본 값이 없어도 자리는 둔다(나중에 그 칸만 갈아 끼운다).
#[test]
fn the_status_line_carries_the_usage_slot() {
    let mut c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    assert!(
        crate::adapters::driving::statusline::render_html(&c, false).contains(r#"class="lines""#),
        "상태줄이 없다"
    );
    c.usage.model = Some("claude-haiku-4-5-20251001".into());
    assert!(render(&c, false).contains("claude-haiku-4-5-20251001"));
}

/// **상태줄은 대화가 없어도 나온다** — 세션 id 를 늘 보여야
/// 사용자가 *"이 탭이 무엇에 붙었나"* 를 물을 자리가 생긴다.
#[test]
fn the_status_line_shows_what_this_tab_is_attached_to() {
    let c = Convo {
        session_id: Some("abcdef0123456789".into()),
        known_agents: vec!["cc".into()],
        ..Default::default()
    };
    let doc = render(&c, false);
    assert!(
        doc.contains(r#"<div id="statusbar"></div>"#),
        "상태줄 자리가 없다"
    );
    let h = crate::adapters::driving::statusline::render_html(&c, false);
    assert!(h.contains(r#"class="status""#), "상태줄이 없다");
    // 세션 id 는 **통째로** 싣는다(2026-10-02 에 뒤집혔다).
    //
    // 종전 규칙은 *"앞 8자만 — 대시보드와 같은 폭"* 이었다. 그런데
    // 오류 문면이 전체 id 로 오므로(실측: *"No conversation found
    // with session ID: e827f3ec-…"*), 8자만 보이면 사용자가 둘을
    // 맞춰 보려고 CLI 를 따로 띄워야 했다. 규칙을 지우지 않고 뒤집어
    // 적는다 — 뒤집은 사실 자체가 기록이다.
    assert!(h.contains("abcdef0123456789"), "세션 id 가 잘렸다");
    assert!(doc.contains("cc"), "agent 이름이 없다");
    // 대화가 비면 `idle` 이다 — 성공/실패 어느 쪽도 아니다.
    assert!(h.contains("대기"), "빈 대화인데 대기 표시가 없다");
}

/// 마지막 턴이 **실패면 그렇게 말한다** — 오류가 대화에 묻히면
/// 사용자는 답이 안 온 것과 실패를 구분 못 한다.
#[test]
fn the_status_line_tells_a_failed_turn_from_a_good_one() {
    let ok = Convo {
        session_id: Some("s1".into()),
        turns: vec![Turn {
            sent: "hi".into(),
            got: "yo".into(),
            error: None,
            pending: false,
            at: None,
        }],
        ..Default::default()
    };
    let bad = Convo {
        session_id: Some("s1".into()),
        turns: vec![Turn {
            sent: "hi".into(),
            got: String::new(),
            error: Some("소켓 없음".into()),
            pending: false,
            at: None,
        }],
        ..Default::default()
    };
    assert!(
        crate::adapters::driving::statusline::render_html(&ok, false)
            .contains(r#"<span class="ok">"#),
        "정상인데 표시가 없다"
    );
    assert!(
        crate::adapters::driving::statusline::render_html(&bad, false)
            .contains(r#"<span class="bad">"#),
        "실패인데 정상으로 보인다"
    );
    // 둘이 **같은 표시를 내면** 이 줄은 아무것도 안 말한다.
    assert_ne!(
        render(&ok, false).contains(r#"<span class="bad">"#),
        crate::adapters::driving::statusline::render_html(&bad, false)
            .contains(r#"<span class="bad">"#),
        "정상과 실패가 같은 표시다"
    );
}

/// 보낸 턴의 시각이 문서 상태에 실리고, 스냅샷을 거쳐 복원해도 남는다(재기동 뒤에도 같은 시각).
#[test]
fn a_sent_turn_keeps_its_time_through_render_snapshot_and_restore() {
    use crate::domain::convo::Convo;
    use crate::domain::convo::begin_turn;
    use crate::domain::convo::snapshot_with_turns;
    use crate::domain::convo::turns_from_snapshot;
    let mut c = Convo {
        session_id: Some("s".into()),
        ..Default::default()
    };
    let i = begin_turn(&mut c, "안녕");
    let at = c.turns[i].at.expect("보낸 턴에 시각이 없다");
    assert!(
        render(&c, false).contains(&format!(r#""at":{at}"#)),
        "문서 상태에 턴 시각이 안 실렸다"
    );
    let back = turns_from_snapshot(&snapshot_with_turns(Some("s"), &c.turns));
    assert_eq!(back[0].at, Some(at), "복원하면 시각이 사라진다");
}

/// ⌘↑ · ⌘↓ 는 글 전체의 처음 · 끝이다 — 노트북에는 Home/End 키가 없다. ⌥ 가 붙으면 textarea 에 맡긴다.
#[test]
fn command_up_and_down_move_to_the_start_and_end_of_the_text() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(h.contains("var doc=e.metaKey&&!e.ctrlKey&&(k==='ArrowUp'||k==='ArrowDown');"));
    assert!(h.contains("if(doc)pos=k==='ArrowUp'?0:v.length;"));
    assert!(h.contains("if(doc&&e.altKey)return;"));
}
