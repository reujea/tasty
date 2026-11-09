use super::*;

/// 시작 화면은 세 탭이다 — 연결(선택 · 옵션 · 세션 목록) · 에이전트 관리(폼 · 표) · 역할 관리(폼 ·
/// 가져오기 · 표). 평소에는 연결 탭이 열리고, 관리 결과가 있으면 그 결과의 탭이 열린다.
#[test]
fn the_start_form_is_split_into_three_tabs() {
    let plain = render(&Convo::default(), false);
    let connect = plain.find(r#"id="tab-connect""#).expect("연결 탭이 없다");
    let agents = plain
        .find(r#"id="tab-agents""#)
        .expect("에이전트 관리 탭이 없다");
    let roles = plain
        .find(r#"id="tab-roles""#)
        .expect("역할 관리 탭이 없다");
    assert!(connect < agents && agents < roles);
    assert!(
        plain[connect..agents].contains(r#"name="a""#),
        "에이전트 선택이 연결 탭 밖이다"
    );
    assert!(
        plain[connect..agents].contains(r#"id="pm""#),
        "옵션이 연결 탭 밖이다"
    );
    assert!(
        plain[agents..roles].contains(r#"id="pform""#),
        "에이전트 폼이 에이전트 관리 탭 밖이다"
    );
    assert!(
        plain[roles..].contains(r#"id="rform""#),
        "역할 폼이 역할 관리 탭 밖이다"
    );
    assert!(
        !plain[agents..roles].contains(r#"name="rn""#),
        "역할 칸이 에이전트 관리 탭에 남았다"
    );
    assert!(plain.contains(r#"class="pane on" id="tab-connect""#));
    assert!(plain.contains(r#"class="pane" id="tab-agents""#));
    assert!(
        plain.contains(r#"class="seg" role="tablist""#),
        "세그먼트 컨트롤이 아니다"
    );
    assert!(
        plain.contains("sessionStorage"),
        "고른 탭을 기억하지 않는다"
    );

    let note = |tab| crate::adapters::driving::manage::Note {
        text: "saved".into(),
        ok: true,
        tab,
    };
    let after = render(
        &Convo {
            manage_note: Some(note(crate::adapters::driving::manage::Tab::Agents)),
            ..Default::default()
        },
        false,
    );
    assert!(after.contains(r#"class="pane" id="tab-connect""#));
    assert!(after.contains(r#"class="pane on" id="tab-agents""#));
    assert!(after.contains("var forced='agents';"));
    let role = render(
        &Convo {
            manage_note: Some(note(crate::adapters::driving::manage::Tab::Roles)),
            ..Default::default()
        },
        false,
    );
    assert!(role.contains(r#"class="pane on" id="tab-roles""#));
    assert!(role.contains("var forced='roles';"));
    assert!(plain.contains("var forced=null;"));
}

/// 폭이 좁으면 표 상자가 가로로 스크롤한다 — 행을 카드로 접지 않는다. 행이 많으면 세로로도
/// 스크롤한다(쪽 나눔이 없다).
#[test]
fn the_session_table_scrolls_instead_of_folding_or_paging() {
    let h = render_start(&[], &many(25, "stopped"), &[], "", None);
    let css = &h[..h.find("</style>").expect("CSS 가 없다")];
    let wrap = css
        .split(" .tbl-wrap{")
        .nth(1)
        .and_then(|r| r.split('}').next())
        .expect("표 상자 규칙이 없다");
    assert!(
        wrap.contains("overflow:auto") && wrap.contains("max-height"),
        "{wrap}"
    );
    assert!(h.contains(r#"<div class="tbl-wrap"><table class="tbl" id="rows" style="min-width:"#));
    assert!(
        !css.contains("@media (max-width:760px)"),
        "좁은 폭에서 카드로 접는다"
    );
    for gone in [r#"id="pager""#, r#"id="prev""#, "PER=", "page_of"] {
        assert!(!h.contains(gone), "쪽 나눔이 남았다: {gone}");
    }
    let td = css
        .split(" .tbl th,.tbl td{")
        .nth(1)
        .and_then(|r| r.split('}').next())
        .expect("칸 규칙이 없다");
    for want in [
        "white-space:nowrap",
        "text-overflow:ellipsis",
        "overflow:hidden",
    ] {
        assert!(td.contains(want), "칸이 한 줄이 아니다: {want}");
    }
}

/// 표는 머리글 줄이 있고 열마다 이름이 있다. 조작 칸은 맨 오른쪽이고 마우스를 올리거나 포커스가
/// 들어올 때 보인다 — 키보드로도 닿게 `visibility` 로 숨기지 않는다.
#[test]
fn the_session_table_has_a_header_and_hover_revealed_actions() {
    let h = render_start(&[], &many(2, "stopped"), &[], "", None);
    let head = h
        .split("<thead>")
        .nth(1)
        .and_then(|r| r.split("</thead>").next())
        .expect("머리글이 없다");
    for col in [
        "상태",
        "에이전트",
        "폴더",
        "변경",
        "세션 ID",
        "에이전트 세션",
        "탭",
    ] {
        assert!(
            head.contains(&format!(">{col}</th>")),
            "머리글에 {col} 이 없다"
        );
    }
    assert!(
        head.contains(r#"id="allpick""#),
        "전체 선택이 머리글에 없다"
    );
    assert_eq!(head.matches("<th").count(), 9, "{head}");
    assert!(
        head.ends_with(r#"<th class="acts" scope="col"></th></tr>"#),
        "조작 칸이 맨 오른쪽이 아니다"
    );
    let css = &h[..h.find("</style>").expect("CSS")];
    assert!(css.contains(".tbl td.acts>*{opacity:0}"));
    assert!(
        css.contains(".tbl tbody tr:focus-within td.acts>*"),
        "키보드 포커스로 안 보인다"
    );
    assert!(
        !css.contains("visibility:hidden"),
        "숨긴 단추는 탭 키가 못 닿는다"
    );
}

/// 세션이 없으면 **시작 폼**이다 — 빈 대화창이 아니다.
///
/// 빈 대화창을 주면 입력해도 "세션이 없다" 만 나와, 사용자가 무엇을
/// 해야 할지 모른다.
#[test]
fn a_tab_with_no_session_shows_the_start_form() {
    let h = render(&Convo::default(), false);
    assert!(h.contains("에이전트 세션 시작"), "시작 폼이 아니다");
    assert!(h.contains(r#"name="a""#), "agent 칸이 없다");
    assert!(!h.contains("메시지를 입력한다"), "대화 입력창이 섞였다");
}

/// 입력 줄은 보내기 · 중지 · 비우기뿐이다 — 현행화 단추 · 체크 옵션 · Alt 로 바뀌는 보내기 단추가 없다.
#[test]
fn the_input_row_only_sends_and_has_no_sync_button_or_loop_mode() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(
        h.contains(r#"<button type="submit" id="send""#),
        "보내기 단추에 id 가 없다"
    );
    assert!(!h.contains("syncbtn"), "현행화 단추가 남았다");
    assert!(!h.contains(r#"id="gate""#), "체크 옵션이 남았다");
    assert!(!h.contains("loopMode"), "Alt 로 바뀌는 보내기가 남았다");
    assert!(
        h.contains("location.hash='agent-send='+n+'&t='+encodeURIComponent(v);"),
        "보내기 주소가 글 하나만 싣지 않는다"
    );
}

/// 세션이 있으면 **대화창**이다.
#[test]
fn a_tab_with_a_session_shows_the_chat() {
    let c = Convo {
        session_id: Some("s1".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    assert!(h.contains("메시지를 입력한다"), "대화 입력창이 없다");
    assert!(!h.contains("에이전트 세션 시작"), "시작 폼이 섞였다");
}

/// 시작 폼이 낸 값을 읽는다 — agent 와 cwd.
#[test]
fn the_start_form_carries_agent_and_cwd() {
    let url = "about:blank?a=cc&c=%2Ftmp&agent-start=1";
    assert_eq!(
        parse_start(url),
        Some(StartForm {
            agent: "cc".into(),
            cwd: Some("/tmp".into()),
            ..Default::default()
        })
    );
}

/// cwd 는 비워도 된다 — agent 설정을 따른다.
#[test]
fn the_start_form_works_without_a_directory() {
    assert_eq!(
        parse_start("x?a=cc&agent-start=1"),
        Some(StartForm {
            agent: "cc".into(),
            ..Default::default()
        })
    );
}

/// agent 가 없으면 **시작하지 않는다** — 띄울 대상이 없다.
#[test]
fn a_start_without_an_agent_is_ignored() {
    assert_eq!(parse_start("x?c=%2Ftmp&agent-start=1"), None);
    assert_eq!(parse_start("x?a=&agent-start=1"), None);
}

/// **시작과 보내기를 가른다.** 같은 표식이면 시작 요청이 그대로 첫
/// 메시지로 나간다.
#[test]
fn a_start_is_not_read_as_a_message() {
    let start = "x?a=cc&agent-start=1";
    assert!(parse_start(start).is_some());
    assert_eq!(parse_input(start), None, "시작 요청을 메시지로 읽었다");

    let send = "x?t=%EC%95%88%EB%85%95&agent-send=1";
    assert!(parse_input(send).is_some());
    assert_eq!(parse_start(send), None, "메시지를 시작 요청으로 읽었다");
}

fn att(id: &str, state: &str, reclaimed: bool) -> Attachable {
    Attachable {
        id: id.into(),
        state: state.into(),
        agent: "cc".into(),
        reclaimed,
        cwd: None,
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    }
}

fn prof(
    name: &str,
    kind: &str,
    env: &[(&str, &str)],
) -> crate::adapters::driven::store::agents::AgentProfile {
    crate::adapters::driven::store::agents::AgentProfile {
        name: name.into(),
        kind: kind.into(),
        cwd: None,
        env: env
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        ..Default::default()
    }
}

/// 프로필은 **고르는 것**이다 — select 에 전부 들어가고 kind 로 묶인다.
#[test]
fn profiles_are_selectable_and_grouped_by_kind() {
    let h = render_start(
        &[],
        &[],
        &[prof("cc", "claude", &[]), prof("pi-dev", "pi", &[])],
        "",
        None,
    );
    assert!(h.contains(r##"<select id="a" name="a""##), "select 가 없다");
    assert!(
        h.contains(r##"<optgroup label="claude">"##),
        "claude 묶음이 없다"
    );
    assert!(h.contains(r##"<optgroup label="pi">"##), "pi 묶음이 없다");
    assert!(h.contains(r##"<option value="cc""##), "cc 항목이 없다");
    assert!(
        h.contains(r##"<option value="pi-dev""##),
        "pi-dev 항목이 없다"
    );
    // 명부가 있으면 직접 입력칸은 **없다** — 둘이 같이 뜨면 어느 쪽이
    // 쓰이는지 알 수 없다.
    assert!(
        !h.contains(r##"<input id="a""##),
        "select 와 입력칸이 같이 떴다"
    );
}

/// 명부에 **없는 kind 는 안 만든다** — 빈 optgroup 이 생기면 고를 것이
/// 없는 묶음이 화면에 남는다.
#[test]
fn only_kinds_present_in_the_roster_get_a_group() {
    let h = render_start(&[], &[], &[prof("cc", "claude", &[])], "", None);
    assert!(h.contains(r##"<optgroup label="claude">"##));
    assert!(!h.contains(r##"label="pi""##), "없는 kind 묶음이 생겼다");
}

/// **시작 spinner 의 걸이가 있다**(0.1.18).
///
/// 종전에는 시작을 누르면 응답까지 **흰 화면**이었다(폼 제출이 곧
/// navigation 이고, 그 처리는 동기라 그 사이 아무것도 안 그려진다).
/// 이제 스크립트가 덮개를 씌운다 — 마크업이 없으면 씌울 것이 없다.
#[test]
fn the_start_form_can_cover_itself_while_it_waits() {
    let h = render_start(&[], &[], &[prof("cc", "claude", &[])], "", None);
    assert!(h.contains(r##"id="starting""##), "덮개가 없다");
    assert!(h.contains(r##"id="starting-msg""##), "문구 자리가 없다");
    assert!(h.contains("class=\"spin\""), "회전자가 없다");
    // 기본은 **숨김** — `on` 이 붙어야 보인다. 처음부터 보이면 폼을
    // 아예 못 쓴다.
    assert!(h.contains("#starting{"), "덮개 스타일이 없다");
    assert!(h.contains("display:none"), "덮개가 처음부터 보인다");
}

/// **목록 도구가 다 나온다** — 검색·페이저·삭제(0.1.18).
///
/// 이 셋은 스크립트가 움직이지만, **마크업이 없으면 스크립트가 잡을
/// 것이 없다**. 스크립트는 시험이 실행하지 못하므로 최소한 그 걸이가
/// 있는지는 값으로 단언한다.
/// 세션을 N 건 만든다 — 페이징·선택을 재려면 **10건을 넘겨야** 한다.
fn many(n: usize, state: &str) -> Vec<Attachable> {
    (0..n)
        .map(|i| Attachable {
            id: format!("sess-{i:04}"),
            agent: "cc".into(),
            state: state.into(),
            reclaimed: false,
            cwd: Some(format!("/w/p{i}")),
            taken_by: None,
            agent_session_id: None,
            agent_session_name: None,
            remaining: Default::default(),
        })
        .collect()
}

/// **행이 전수 실린다** — 페이징은 JS 가 숨기는 것이지 서버가 자르는
/// 것이 아니다.
///
/// 서버가 10건만 실으면 2쪽으로 넘겨도 **빈 화면**이 된다. 종전
/// 가드는 `id="pager"` 가 있는지만 봐서 이것을 못 잡았다.
#[test]
fn every_row_is_in_the_document_even_past_the_first_page() {
    let a = many(25, "stopped");
    let h = render_start(&[], &a, &[], "", None);
    let rows = h.matches("<tr data-k=").count();
    assert_eq!(rows, 25, "행이 25건이 아니다 — 서버가 잘랐으면 2쪽이 빈다");
    // 끝 쪽에만 있는 것도 실려야 한다.
    assert!(h.contains("sess-0024"), "마지막 세션이 문서에 없다");
}

/// **고를 수 있는 것은 `stopped` 뿐이다** — `running` 은 체크박스가
/// 아예 없다(단건 삭제 링크를 안 내는 것과 같은 규칙).
#[test]
fn only_stopped_sessions_can_be_picked() {
    let stopped = render_start(&[], &many(3, "stopped"), &[], "", None);
    assert_eq!(
        stopped.matches(r##"class="pick""##).count(),
        3,
        "stopped 인데 고를 수 없다"
    );
    let running = render_start(&[], &many(3, "running"), &[], "", None);
    assert_eq!(
        running.matches(r##"class="pick""##).count(),
        0,
        "도는 세션을 고를 수 있다 — 데몬이 거부해 일부만 지워진다"
    );
}

/// 일괄 삭제 바가 **있고**, 단건과 **같은 표식**을 쓴다.
#[test]
fn the_bulk_delete_bar_uses_the_same_mark_as_the_single_one() {
    let h = render_start(&[], &many(12, "stopped"), &[], "", None);
    for (needle, what) in [
        (r##"id="bulk""##, "일괄 바"),
        (r##"id="delsel""##, "고른 것 지우기"),
        (r##"id="picked""##, "고른 개수"),
    ] {
        assert!(h.contains(needle), "{what}이 없다");
    }
    // **일괄이 보내는 줄 자체를 본다.** 종전에는 문서에
    // `&agent-del=1&ok=1` 이 있는지만 봤는데, **단건 확인 링크가
    // 같은 문자열을 낸다** — 일괄 쪽 표식을 바꿔도 통과했다(변이로
    // 밟았다). 보내는 그 한 줄을 짚어야 갈린다.
    // 보내기 **직전에 `?` 를 지운다** — 안 지우면 호스트가 쿼리를
    // 먼저 읽어 삭제 표식을 못 본다(실사용 결함 ·
    // `a_bulk_delete_survives_a_leftover_query` 가 그 축이다).
    let script = h.split("<script").nth(1).expect("스크립트가 없다");
    assert!(
        script.contains("history.replaceState(null,'','about:blank')"),
        "보내기 전에 `?` 를 안 지운다 — 눌러도 아무 일이 없다"
    );
    let send_line = h
        .lines()
        .find(|l| l.contains("location.hash=qs+"))
        .expect("일괄 삭제가 보내는 줄이 없다");
    assert!(
        send_line.contains(&format!("&{DELETE_MARK}=1")),
        "일괄 삭제가 단건과 다른 표식을 쓴다: {send_line}"
    );
    assert!(
        send_line.contains("ok=1"),
        "일괄 삭제가 확인 없이 나간다: {send_line}"
    );
}

/// **스크립트는 요소보다 뒤에 온다** — 앞에 오면 아무것도 안 붙는다.
///
/// 실측(WKWebView): 스크립트가 먼저 돌면 `getElementById` 가 전부
/// `null` 이라 **직접 바인딩이 하나도 안 붙는다**. 검색·페이저·일괄
/// 삭제가 그래서 조용히 죽어 있었다 — 눌러도 아무 일이 없었다.
/// 위임(`document.addEventListener`)만 살아남아 행 클릭은 됐고,
/// 그 차이가 *"단건은 되는데 일괄만 안 된다"* 로 보였다.
#[test]
fn the_script_comes_after_everything_it_binds_to() {
    let h = render_start(&[], &many(3, "stopped"), &[], "", None);
    let script_at = h.find("<script").expect("스크립트가 없다");
    for id in [
        "id=\"q\"",
        "id=\"rows\"",
        "id=\"allpick\"",
        "id=\"delsel\"",
        "id=\"bulk\"",
    ] {
        let at = h.find(id).unwrap_or_else(|| panic!("{id} 가 없다"));
        assert!(
            at < script_at,
            "{id} 가 스크립트보다 뒤에 있다 — 그 바인딩은 안 붙는다"
        );
    }
}

/// **일괄 단추를 단건 핸들러가 집어가면 안 된다**(실사용 결함).
///
/// 일괄 단추는 `class="del"` 을 쓰는데(모양을 맞추려고), 단건 삭제
/// 핸들러가 `closest('a.del')` 로 **문서 전체의 클릭**을 받는다.
/// 그래서 "고른 것 지우기" 를 눌러도 단건 쪽이 먼저 `preventDefault`
/// 하고 그 자리를 확인 UI 로 바꿔 **아무것도 안 지워졌다**.
///
/// 종전 가드는 마크업과 보내는 줄만 봐서 이것을 못 잡았다 — 둘 다
/// 멀쩡했고 **핸들러끼리 부딪힌 것**이라, 사용자가 쓰기 전까지
/// 조용했다.
#[test]
fn the_single_delete_handler_lets_the_bulk_button_through() {
    let h = render_start(&[], &many(3, "stopped"), &[], "", None);
    let single = h.split("a.del").nth(1).expect("단건 삭제 핸들러가 없다");
    let head: String = single.chars().take(300).collect();
    assert!(
        head.contains("delsel"),
        "단건 핸들러가 일괄 단추를 비켜주지 않는다 — 눌러도 안 지워진다"
    );
}

/// **행을 눌러도 골라진다** — 체크박스만 누르게 하면 과녁이 작다.
#[test]
fn clicking_the_row_picks_it() {
    let h = render_start(&[], &many(3, "stopped"), &[], "", None);
    // **스크립트 쪽을 짚는다.** `#rows tr` 는 CSS 에도 있어
    // 그것만 보면 JS 를 지워도 통과한다(변이로 밟았다).
    let script = h.split("<script").nth(1).expect("스크립트가 없다");
    assert!(
        script.contains("t.closest('#rows tr')"),
        "행 클릭을 안 받는다 — 체크박스만 과녁이 된다"
    );
    // 링크·단추를 누른 것은 **선택이 아니다** — 안 비키면 "붙기" 를
    // 누를 때마다 선택이 토글된다.
    assert!(
        h.contains("t.closest('a')") && h.contains("t.closest('button')"),
        "행 안의 링크·단추를 안 비킨다 — 누를 때마다 선택이 바뀐다"
    );
}

/// **고른 행은 색이 다르다** — 체크박스만으로는 눈에 안 들어온다.
#[test]
fn a_picked_row_is_marked() {
    let h = render_start(&[], &many(3, "stopped"), &[], "", None);
    // 색은 **CSS 에**, 켜는 것은 **스크립트에** 있어야 한다 — 한쪽만
    // 있으면 아무 표시도 안 난다. 두 자리를 따로 짚는다.
    let css = h.split("<style").nth(1).expect("CSS 가 없다");
    // ⚠️ **이 창(4000자)은 약점이다**(2026-10-06 실측). 규칙을 위쪽에
    // 더하면 찾던 것이 창 밖으로 밀려 **멀쩡한데 빨간불**이 난다 —
    // 실제로 `details.more` 를 넣었다가 그랬고, 규칙을 아래로 옮겨
    // 풀었다. 고치려면 `</style>` 까지 전부 보는 쪽이 맞다.
    let css_head = &css[..css.find("</style>").expect("CSS 가 안 닫혔다")];
    assert!(
        css_head.contains("#rows tr.on{"),
        "고른 행에 줄 색이 없다 — 켜도 안 보인다"
    );
    let script = h.split("<script").nth(1).expect("스크립트가 없다");
    assert!(
        script.contains("classList.toggle('on'"),
        "고른 행에 표시를 안 켠다 — 색이 있어도 안 붙는다"
    );
}

/// 고른 것은 **집합이 쥔다** — 페이징이 숨긴 행의 선택이 날아가면
/// 사용자는 *"고른 것 중 보이는 것만"* 지우게 된다.
#[test]
fn the_selection_is_held_outside_the_dom() {
    let h = render_start(&[], &many(25, "stopped"), &[], "", None);
    assert!(h.contains("var picks={}"), "선택을 담는 집합이 없다");
    // 보내는 쪽은 그 집합에서 뽑아야 한다 — DOM 질의면 숨은 행을 놓친다.
    assert!(
        h.contains("function picked()"),
        "고른 것을 집합에서 안 뽑는다"
    );
    assert!(
        !h.contains("querySelectorAll('.pick:checked')"),
        "DOM 에서 고른 것을 뽑는다 — 페이징이 숨긴 행을 놓친다"
    );
}

#[test]
fn the_session_list_has_search_paging_and_delete() {
    let a = Attachable {
        id: "aaaaaaaa-1111".into(),
        agent: "cc".into(),
        state: "stopped".into(),
        reclaimed: false,
        cwd: Some("/w/proj".into()),
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);

    for (needle, what) in [
        (r##"id="q""##, "검색칸"),
        (r##"id="rows""##, "표 걸이"),
        (r##"<thead>"##, "머리글"),
        (r##"id="count""##, "건수"),
        (r##"id="nomatch""##, "없음 문구"),
    ] {
        assert!(h.contains(needle), "{what}이 없다");
    }
    // 검색 키 — agent·폴더·id 를 **소문자로** 모은다. 없으면 필터가
    // 아무것도 못 거른다(=전부 보인다 = 검색이 없는 것과 같다).
    assert!(
        h.contains(r##"data-k="cc /w/proj aaaaaaaa-1111""##),
        "검색 키가 없거나 모양이 다르다"
    );
    // 삭제 링크 — 표식이 붙기와 **달라야** 한다.
    assert!(h.contains(DELETE_MARK), "삭제 링크가 없다");
    assert_ne!(DELETE_MARK, ATTACH_MARK, "삭제와 붙기가 같은 표식이다");
}

/// **도는 세션에는 삭제 링크를 안 낸다.**
///
/// 데몬도 거부하지만(`invalid_state`), 누를 수 있게 두면 사용자는
/// 눌러 보고 실패한다 — 그 실패가 화면에 안 나타나면 *"안 지워졌다"*
/// 로만 남는다.
#[test]
fn a_running_session_offers_no_delete_link() {
    let mk = |state: &str| Attachable {
        id: "bbbbbbbb-2222".into(),
        agent: "cc".into(),
        state: state.into(),
        reclaimed: false,
        cwd: None,
        taken_by: None,
        agent_session_id: None,
        agent_session_name: None,
        remaining: Default::default(),
    };
    let running = render_start(&[], &[mk("running")], &[], "", None);
    let stopped = render_start(&[], &[mk("stopped")], &[], "", None);

    // ⚠️ **클래스가 아니라 `href` 로 본다.** 종전에는
    // `class="del" href=…` 를 통째로 찾았는데, 단추 모양을 입히며
    // `class="btn danger del"` 이 되자 **멀쩡한 코드가 빨개졌다**
    // (2026-10-02). 보이는 모양은 바뀌어도 **어디로 보내는가**는
    // 그대로다 — 그쪽이 이 시험이 지키려는 것이다.
    let del_link = format!(r##"href="#s=bbbbbbbb-2222&amp;{DELETE_MARK}"##);
    assert!(!running.contains(&del_link), "도는 세션에 삭제 링크가 있다");
    // **양성 대조** — 없는 것만 보면 링크가 아예 안 나가도 통과한다.
    assert!(
        stopped.contains(&del_link),
        "재운 세션에도 삭제 링크가 없다"
    );
}

/// **고른 폴더가 프로필 기본값에 안 덮인다**(2026-09-30 실사용 결함).
///
/// # 무슨 일이 있었나
///
/// 스크립트가 로드 직후 `fillCwd()` 를 불러 입력칸을 프로필의
/// `data-cwd` 로 채웠다. 그런데 폴더 피커로 고른 뒤 다시 그릴 때는
/// **서버가 이미 그 값을 실어 준다** — 그것을 덮어써 고른 폴더가
/// 사라졌다. 피커는 제대로 돌았는데(로그로 확인) 화면만 안 바뀌어
/// *"선택이 입력 안 된다"* 로 보였다.
///
/// 서버가 실어 준 값이 **정본**이다. 비었을 때만 채운다.
///
/// **시작이 실패하면 사유가 화면에 남는다**(2026-09-30 실사용 결함).
///
/// # 왜 이것이 spinner 를 푸나
///
/// 실패하면 폼을 다시 그리는데, 그 HTML 이 **직전과 바이트까지 같으면**
/// 호스트가 재적재를 건너뛴다(`resync_webview_urls` 의 URL 비교).
/// 그러면 스크립트가 씌운 spinner 가 그대로 남아 **영영 도는 것처럼**
/// 보인다 — 실사용에서 `Resource temporarily unavailable` 로 spawn 이
/// 실패했는데 화면은 계속 "Starting the session..." 이었다.
///
/// 사유를 실으면 문서가 달라져 재적재가 일어나고, 사용자도 **왜**
/// 안 됐는지 안다 — 두 문제가 한 값으로 풀린다.
#[test]
fn a_failed_start_leaves_its_reason_on_the_form() {
    let ok = render_start(&[], &[], &[prof("cc", "claude", &[])], "", None);
    let bad = render_start(
        &[],
        &[],
        &[prof("cc", "claude", &[])],
        "",
        Some("소켓 응답 없음"),
    );

    assert!(bad.contains("소켓 응답 없음"), "사유가 화면에 없다");
    // **낱말이 아니라 속성 자리를 본다** — CSS 규칙이 그 이름을 늘
    // 담고 있어, 문자열만 세면 어느 쪽이든 통과한다.
    assert!(
        bad.contains(r#"<div class="start-err">"#),
        "사유 자리가 없다"
    );
    assert!(
        !ok.contains(r#"<div class="start-err">"#),
        "실패가 아닌데 오류 칸이 있다"
    );

    // **문서가 달라야 재적재가 일어난다.** 같으면 spinner 가 남는다.
    assert_ne!(
        ok, bad,
        "실패해도 HTML 이 같다 — 호스트가 재적재를 건너뛰어 spinner 가 안 걷힌다"
    );
}

#[test]
fn a_picked_folder_is_not_overwritten_by_the_profile_default() {
    let mut a = prof("cc", "claude", &[]);
    a.cwd = Some("/profile/default".into());
    // 피커로 고른 값을 서버가 실어 준 상태.
    let h = render_start(&[], &[], &[a], "/picked/by/user", None);

    assert!(
        h.contains(r##"value="/picked/by/user""##),
        "고른 폴더가 칸에 안 실렸다"
    );
    // **채우기는 조건부여야 한다.** 무조건 부르면 프로필 값이 이긴다.
    assert!(
        h.contains("!cwd.value"),
        "로드 직후 무조건 채운다 — 고른 폴더가 프로필 기본값에 덮인다"
    );
}

/// **프로필의 폴더가 화면에 실린다** — 고르면 칸이 채워지게.
///
/// # 이 축은 옮겨졌다(0.1.18)
///
/// 종전에는 폴더 찾기 아래에 프로필들의 폴더를 **단추로** 늘어놓고,
/// 누르면 `?c=…&agent-cwd=1` 로 폼을 다시 그렸다(서버 왕복 = 문서
/// 재적재). 지금은 각 `option` 이 자기 폴더를 싣고 **스크립트가**
/// 칸을 채운다 — 왕복이 없다.
///
/// 축 자체는 같다: *"프로필이 아는 폴더를 사용자가 다시 치지
/// 않는다"*. 단추 목록만 지우고 이 시험을 없애면 그 축이 통째로 빈다.
#[test]
fn each_profile_carries_its_folder_into_the_form() {
    let mut a = prof("cc", "claude", &[]);
    a.cwd = Some("/w/one".into());
    let mut b = prof("cc2", "claude", &[]);
    b.cwd = None;
    let h = render_start(&[], &[], &[a, b], "", None);

    assert!(
        h.contains(r##"data-cwd="/w/one""##),
        "프로필의 폴더가 option 에 안 실렸다 — 고를 방법이 사라진다"
    );
    // 폴더가 없는 프로필은 **빈 값**이다. 없는 것을 빠뜨리면 스크립트가
    // 앞 프로필의 값을 그대로 남겨 **엉뚱한 폴더로 뜬다**.
    assert!(
        h.contains(r##"data-cwd=""##),
        "폴더 없는 프로필에 빈 data-cwd 가 없다"
    );
    // 단추 목록은 **없어야 한다** — 같은 것을 두 군데서 주면
    // 어느 쪽이 정본인지 흐려진다(이것이 이번에 지운 이유다).
    assert!(
        !h.contains("known-cwds"),
        "폴더 단추 목록이 남아 있다 — 채움과 단추가 둘 다 있다"
    );
}

/// 경로에 `&`·공백이 있어도 **그 경로 그대로** option 에 실린다.
///
/// HTML 속성이라 `escape` 만 하면 된다(종전에는 URL 이라
/// `query_encode` 가 필요했다 — 층이 달라졌다).
#[test]
fn odd_paths_survive_the_attribute() {
    let mut a = prof("cc", "claude", &[]);
    a.cwd = Some("/w/a b&c".into());
    let h = render_start(&[], &[], &[a], "", None);
    assert!(
        h.contains(r##"data-cwd="/w/a b&amp;c""##),
        "이상한 경로가 속성에서 깨졌다: {}",
        &h[..200.min(h.len())]
    );
}

/// 고른 폴더가 **칸에 되실린다** — 안 그러면 누른 것이 사라져 보인다.
#[test]
fn the_picked_folder_comes_back_in_the_field() {
    let h = render_start(&[], &[], &[prof("cc", "claude", &[])], "/w/picked", None);
    assert!(
        h.contains(r##"<input id="c" name="c" autocomplete="off" value="/w/picked""##),
        "고른 폴더가 칸에 안 실렸다"
    );
}

/// 찾기 단추는 **네 갈래 중 자기 것만** 켠다. 다섯 표식이 한 URL 위에
/// 있으니 서로 섞이면 엉뚱한 일이 난다(시작·붙기·보내기·고르기·찾기).
#[test]
fn browsing_is_its_own_branch() {
    let url = "x?agent-browse=1";
    assert_eq!(parse_browse(url), Some(PickField::Start));
    // 관리 탭의 폴더 칸 — 어느 칸에 넣을지가 함께 온다. 모르는 칸은 연결 탭의 칸이다.
    assert_eq!(
        parse_browse("x?agent-browse=1&pf=pw"),
        Some(PickField::AgentCwd)
    );
    assert_eq!(
        parse_browse("x?agent-browse=1&pf=rp"),
        Some(PickField::RolePath)
    );
    assert_eq!(
        parse_browse("x?agent-browse=1&pf=rs"),
        Some(PickField::RoleScan)
    );
    assert_eq!(
        parse_browse("x?agent-browse=1&pf=zz"),
        Some(PickField::Start)
    );
    assert_eq!(parse_start(url), None, "찾기를 시작으로 읽었다");
    assert_eq!(parse_cwd_pick(url), None, "찾기를 고르기로 읽었다");
    assert_eq!(parse_input(url), None, "찾기를 메시지로 읽었다");
    assert_eq!(parse_attach(url), None, "찾기를 붙기로 읽었다");
}

/// 거꾸로도 갈린다 — 다른 갈래를 찾기로 읽으면 피커가 헛되이 열린다.
#[test]
fn other_branches_are_not_browsing() {
    for url in [
        "x?a=cc&agent-start=1",
        "x?c=%2Ftmp&agent-cwd=1",
        "x?t=hi&agent-send=1",
    ] {
        assert_eq!(parse_browse(url), None, "{url} 을 찾기로 읽었다");
    }
}

/// **무엇을 눌러야 끝나는지 화면에 적는다.**
///
/// 피커를 열어 놓고 확정하는 법을 모르면 그 자리에서 막힌다 —
/// 파일을 고르려 해도 안 골리기 때문이다(폴더 모드).
///
/// 종전 문구는 *"그 폴더의 파일을 하나 고르면 된다"* 였다. 그때는
/// 디렉토리 모드가 없어 **파일의 부모**를 썼기 때문인데, 0.1.9 에서
/// 폴더를 직접 고르게 되면서 거짓이 됐다.
///
/// **가드가 옛 문구를 붙들고 있었다** — 화면을 고쳐도 이 시험이
/// 빨간불이 되어 되돌리게 만든다. 문구를 바꿀 때 가드도 같이 본다.
#[test]
fn the_browse_button_says_how_to_finish() {
    let h = render_start(&[], &[], &[], "", None);
    assert!(h.contains("이 폴더 선택"), "확정하는 법 안내가 없다");
    // 옛 우회 문구가 되살아나면 red — 그것은 더 이상 사실이 아니다.
    assert!(
        !h.contains("파일을 하나 고르면"),
        "폴더 모드인데 *파일을 고르라* 고 안내한다"
    );
}

/// 찾기 단추는 **명부가 비어도** 있다 — 단추가 하나도 없을 때
/// 폴더를 정할 유일한 길이다.
#[test]
fn the_browse_button_is_there_even_without_a_roster() {
    let h = render_start(&[], &[], &[], "", None);
    assert!(h.contains("선택…"), "명부가 비니 찾기 단추가 사라졌다");
    assert!(h.contains(BROWSE_MARK), "찾기 표식이 없다");
}

/// 폴더 단추는 **시작 요청이 아니다** — 눌러도 세션이 안 뜬다.
#[test]
fn a_folder_pick_is_not_a_start_request() {
    let url = "x?c=%2Fw%2Fone&agent-cwd=1";
    assert_eq!(parse_cwd_pick(url), Some("/w/one".into()));
    assert_eq!(parse_start(url), None, "폴더 고르기를 시작으로 읽었다");
    assert_eq!(parse_input(url), None, "폴더 고르기를 메시지로 읽었다");
    assert_eq!(parse_attach(url), None, "폴더 고르기를 붙기로 읽었다");
}

/// 거꾸로도 갈린다 — 시작 요청을 폴더 고르기로 읽으면 폼만 다시 그리고
/// **세션이 안 뜬다**.
#[test]
fn a_start_request_is_not_a_folder_pick() {
    assert_eq!(parse_cwd_pick("x?a=cc&c=%2Ftmp&agent-start=1"), None);
    assert_eq!(parse_cwd_pick("x?c=%2Ftmp"), None, "표식 없이 읽혔다");
    assert_eq!(parse_cwd_pick("x?c=&agent-cwd=1"), None, "빈 폴더가 읽혔다");
}

/// **env 를 함께 적는다** — 이름만으로는 `cc` 와 `cc-sdk` 가 무엇이
/// 다른지 알 수 없다. 그 둘을 가르는 것이 env 다.
#[test]
fn each_profile_shows_the_env_it_carries() {
    let h = render_start(
        &[],
        &[],
        &[prof("cc", "claude", &[("CLAUDE_CONFIG_DIR", "/p/dev")])],
        "",
        None,
    );
    assert!(h.contains("CLAUDE_CONFIG_DIR"), "env 이름이 없다");
    assert!(
        h.contains("/p/dev"),
        "env 값이 없다 — 어느 프로필인지 못 가른다"
    );
}

/// 명부를 못 읽으면 **직접 입력으로 돌아간다** — 화면이 비지 않는다.
#[test]
fn without_a_roster_the_form_still_works() {
    let h = render_start(&["cc".to_string()], &[], &[], "", None);
    assert!(h.contains(r#"name="a""#), "직접 입력칸이 사라졌다");
    assert!(h.contains(">cc</a>"), "도는 세션에서 모은 단추도 없다");
}

/// 입력칸이 쉘과 같은 편집 키를 받는다 — Home/End 는 줄 처음·끝, PageUp/PageDown 은 대화 스크롤.
/// 빠지면 macOS 에서 그 키의 기능 문자가 깨진 글자로 들어간다(실측 2026-10-22).
#[test]
fn the_input_takes_the_shell_editing_keys() {
    let c = Convo {
        session_id: Some("s".into()),
        ..Default::default()
    };
    let h = render(&c, false);
    for want in [
        "k==='Home'",
        "k!=='End'",
        "k==='PageUp'",
        "setSelectionRange",
        "lg.scrollTop+=",
    ] {
        assert!(h.contains(want), "입력칸 키 처리에 {want} 가 없다");
    }
}

/// 입력 히스토리 — 보낸 말을 오래된 순으로, 이어진 같은 말은 하나로, 빈 말은 빼고, 대기 칸까지.
#[test]
fn the_input_history_is_what_this_tab_sent() {
    let turn = |sent: &str| Turn {
        sent: sent.into(),
        got: String::new(),
        error: None,
        pending: false,
        at: None,
    };
    let c = Convo {
        turns: vec![
            turn("ls"),
            turn("ls"),
            turn(""),
            turn("</script>x"),
            turn("ls"),
        ],
        queued: vec!["다음 말".into()],
        ..Default::default()
    };
    let h = input_history(&c);
    // `<` 는 하나도 남지 않는다 — 전부 유니코드 이스케이프로 바뀐다.
    assert!(!h.contains('<') && h.contains("u003c/script>x"), "{h}");
    let v: Vec<String> = serde_json::from_str(&h).expect("JSON");
    assert_eq!(v, vec!["ls", "</script>x", "ls", "다음 말"]);
    let many = Convo {
        turns: (0..HISTORY_MAX + 5).map(|i| turn(&i.to_string())).collect(),
        ..Default::default()
    };
    let v: Vec<String> = serde_json::from_str(&input_history(&many)).expect("JSON");
    assert_eq!(v.len(), HISTORY_MAX);
    assert_eq!(v[0], "5", "오래된 것부터 버려야 한다");
    // 대화창 문서에 실린다.
    assert!(
        render(
            &Convo {
                session_id: Some("s".into()),
                ..c
            },
            false
        )
        .contains(r#"var hist = ["ls""#)
    );
}

/// 접힌 칸은 **전부** 제출에 실린다 — 칸만 더하고 제출 목록에 빠뜨리면 파서에만 있고 전송은 안 된다
/// (모델 · 이름이 그랬고, 권한 모드도 처음에 그랬다).
#[test]
fn every_folded_start_field_is_submitted() {
    let h = render_start(&[], &[], &[], "", None);
    let extra = h
        .split("var extra=[")
        .nth(1)
        .and_then(|s| s.split("];").next())
        .expect("제출 목록");
    for id in ["m", "n", "ad", "pm"] {
        assert!(h.contains(&format!(r#"name="{id}""#)), "{id} 칸이 없다");
        assert!(
            extra.contains(&format!("['{id}','{id}']")),
            "{id} 칸이 제출에 안 실린다"
        );
    }
    assert_eq!(
        parse_start("x?a=cc&pm=plan&agent-start=1")
            .and_then(|f| f.permission_mode)
            .as_deref(),
        Some("plan")
    );
}

/// 행의 칸 순서 — 상태 · 프로필 · 폴더 · 남은 일 · supervisor id · 에이전트 세션 id(이름) · 붙기/삭제(한 칸).
#[test]
fn a_row_shows_the_agent_session_and_one_action_cell() {
    let a = Attachable {
        id: "sup-1111".into(),
        state: "stopped".into(),
        agent: "dev".into(),
        reclaimed: true,
        cwd: Some("/w/proj".into()),
        taken_by: None,
        agent_session_id: Some("conv-2222".into()),
        agent_session_name: Some("ideabank-33".into()),
        remaining: Default::default(),
    };
    let h = render_start(&[], std::slice::from_ref(&a), &[], "", None);
    let row = h
        .split("<tr data-k=")
        .nth(1)
        .and_then(|r| r.split("</tr>").next())
        .expect("행이 없다");
    let at = |needle: &str| {
        row.find(needle)
            .unwrap_or_else(|| panic!("{needle} 이 없다: {row}"))
    };
    assert!(at("dev") < at("/w/proj") && at("/w/proj") < at("sup-1111"));
    assert!(at("sup-1111") < at("conv-2222") && at("conv-2222") < at("(ideabank-33)"));
    assert_eq!(row.matches("<td").count(), 9, "칸 수가 다르다: {row}");
    let acts = &row[at(r#"<td class="acts">"#)..];
    assert!(
        acts.contains("agent-attach=1") && acts.contains("agent-del=1"),
        "붙기·삭제가 한 칸에 없다: {acts}"
    );
    // 검색은 에이전트 세션 id 와 이름으로도 걸린다.
    assert!(
        row.starts_with(r#""dev /w/proj sup-1111 conv-2222 ideabank-33""#),
        "{row}"
    );
    // 이름을 모르면 id 만, id 도 없으면 `-`.
    let bare = Attachable {
        agent_session_name: None,
        remaining: Default::default(),
        ..a.clone()
    };
    let h = render_start(&[], std::slice::from_ref(&bare), &[], "", None);
    assert!(h.contains("conv-2222") && !h.contains("class=\"aname\""));
}

/// 기존 세션이 **목록으로** 보인다 — id 를 손으로 옮겨 적지 않는다.
#[test]
fn existing_sessions_are_listed_to_attach() {
    let h = render_start(&[], &[att("aaaaaaaa-1", "running", false)], &[], "", None);
    assert!(h.contains("세션 목록"), "목록 절이 없다");
    assert!(h.contains("aaaaaaaa"), "세션 id 가 없다");
    assert!(h.contains("이 탭에서 열기"), "여는 길이 없다");
}

/// 폴더를 함께 보여 준다 — 같은 agent 로 띄운 세션이 여럿이면 **이것이
/// 유일하게 가르는 값**이다(id 8자리는 안 외워진다).
#[test]
fn the_list_shows_which_folder_each_session_runs_in() {
    let mut a = att("aaaaaaaa-1", "running", false);
    a.cwd = Some("/Users/me/project".into());
    let h = render_start(&[], &[a], &[], "", None);
    assert!(h.contains("/Users/me/project"), "폴더가 안 보인다");
}

/// **폴더 없음과 "agent 설정을 따른다" 는 다르다.**
///
/// 그쪽은 오버라이드가 있을 때만 `cwd` 를 싣는다(키 자체가 없다). 빈 칸으로
/// 두면 *"폴더가 없다"* 로 읽혀, 사용자가 어디서 도는지 모른다고 여긴다.
#[test]
fn a_session_without_an_override_says_it_follows_the_agent() {
    let h = render_start(&[], &[att("bbbbbbbb-2", "running", false)], &[], "", None);
    // **가르는 낱말로 단언한다.** `"agent 설정"` 만 세면 위쪽 입력칸의
    // placeholder("비우면 agent 설정을 따른다")가 늘 담고 있어 **변이가
    // 통과한다**(실제로 그랬다). 그 칸에만 붙는 class 를 본다.
    assert!(
        h.contains(r#"<span class="inherit">"#),
        "설정을 따른다는 표시가 없다 — 빈 칸이면 '폴더가 없다' 로 읽힌다"
    );
}

/// 목록도 **세 상태를 가른다** — 도는 중 / 재워짐 / 죽음.
#[test]
fn the_list_tells_the_three_states_apart() {
    let h = render_start(
        &[],
        &[
            att("a1", "running", false),
            att("b2", "stopped", true),
            att("c3", "failed", false),
            att("d4", "stopped", false),
            att("e5", "weird_state", false),
        ],
        &[],
        "",
        None,
    );
    // 상태는 색만으로 가르지 않는다 — 글이 늘 함께 있고, 데몬 값은 화면 언어로 옮긴다.
    assert!(h.contains(r#"<span class="dot run"></span>실행 중"#));
    assert!(h.contains(r#"<span class="dot slept"></span>재워짐"#));
    assert!(
        h.contains(r#"<span class="dot dead"></span>실패"#),
        "죽은 것의 사유가 사라졌다"
    );
    assert!(
        h.contains(r#"<span class="dot dead"></span>중지됨"#),
        "stopped 가 번역되지 않았다"
    );
    assert!(!h.contains(">stopped<"), "영어 상태가 남았다");
    assert!(
        h.contains("weird_state"),
        "모르는 상태를 지웠다 — 데몬 값을 그대로 보여야 한다"
    );
}

/// 목록 한 줄에 **남은 일**이 보인다 — 결정 대기 · 응답 대기 · 미커밋 · 마지막 활동.
#[test]
fn the_list_shows_what_is_left_per_session() {
    let mut busy = att("a1", "stopped", false);
    busy.remaining = crate::adapters::driven::remaining::Remaining {
        questions: Some(2),
        asking: true,
        dirty: Some(3),
        idle_secs: Some(7200),
    };
    let h = render_start(&[], &[busy, att("b2", "stopped", false)], &[], "", None);
    for want in ["결정 2", "응답 대기", "미커밋 3", "2시간 전"] {
        assert!(h.contains(want), "{want} 가 목록에 없다");
    }
    assert_eq!(
        h.matches(r#"<td class="rem">"#).count(),
        2,
        "모르는 행도 칸은 낸다(`-`) — 칸이 빠지면 열이 어긋난다"
    );
}

/// 붙을 것이 없으면 **그렇다고 적는다** — 빈 표를 주지 않는다.
#[test]
fn an_empty_list_says_so() {
    let h = render_start(&[], &[], &[], "", None);
    assert!(h.contains("열 수 있는 세션이 없습니다"));
}

/// 붙기 요청을 읽는다.
#[test]
fn an_attach_request_carries_the_session_id() {
    assert_eq!(
        parse_attach("about:blank?s=abc-123&agent-attach=1").as_deref(),
        Some("abc-123")
    );
}

/// 붙기·시작·보내기가 **서로 안 섞인다**.
///
/// 표식이 같으면 붙기가 새 세션을 띄우거나 첫 메시지로 나간다.
#[test]
fn attach_start_and_send_do_not_overlap() {
    let attach = "x?s=abc&agent-attach=1";
    let start = "x?a=cc&agent-start=1";
    let send = "x?t=hi&agent-send=1";

    assert!(parse_attach(attach).is_some());
    assert_eq!(parse_start(attach), None, "붙기를 시작으로 읽었다");
    assert_eq!(parse_input(attach), None, "붙기를 메시지로 읽었다");

    assert_eq!(parse_attach(start), None, "시작을 붙기로 읽었다");
    assert_eq!(parse_attach(send), None, "메시지를 붙기로 읽었다");
}

/// 아는 agent 는 단추로 놓는다 — 흔한 경우를 덜어 준다.
#[test]
fn known_agents_become_shortcuts() {
    let h = render_start(
        &["cc".to_string(), "pi-dev".to_string()],
        &[],
        &[],
        "",
        None,
    );
    assert!(h.contains(">cc</a>"), "단추가 없다");
    assert!(h.contains(">pi-dev</a>"));
}

/// 역할 표는 연결 탭에서 **시작 폼 옆**에 있다 — 폼과 같은 2단 상자 안, 폼 다음 칸이다. 표의 스크립트는
/// 표 뒤에 오고, 연결 스크립트가 문서의 첫 `<script>` 로 남는다.
#[test]
fn the_role_table_sits_beside_the_start_form() {
    let roles = [crate::adapters::driven::store::roles::Role {
        name: "builder".into(),
        path: "/w/builder".into(),
    }];
    let h = start_form::render_start_with(
        &Default::default(),
        &[],
        &[],
        &[],
        "",
        None,
        &start_form::Extras {
            roles: &roles,
            ..Default::default()
        },
    );
    let connect = h.find(r#"id="tab-connect""#).expect("연결 탭이 없다");
    let agents = h
        .find(r#"id="tab-agents""#)
        .expect("에이전트 관리 탭이 없다");
    let split = h.find(r#"class="connect-split""#).expect("2단 상자가 없다");
    let form = h.find(r#"id="sform""#).expect("시작 폼이 없다");
    let table = h.find(r#"id="rpick""#).expect("역할 표가 없다");
    assert!(connect < split && split < form && form < table && table < agents);
    assert!(
        !h[form..table].contains(r#"name="r""#),
        "폼 안에 옛 역할 체크 칸이 남았다"
    );
    let role_script = h
        .find("getElementById('rpick')")
        .expect("역할 표 스크립트가 없다");
    assert!(table < role_script && role_script < agents);
    let first_script = h.find("<script").expect("스크립트가 없다");
    assert!(
        first_script
            < h[..role_script]
                .rfind("<script")
                .expect("역할 스크립트 머리"),
        "역할 스크립트가 문서의 첫 스크립트다 — 연결 스크립트가 첫째여야 한다"
    );
}
