//! **닫은 탭 되살리기**(Ctrl+Shift+T · R3)와 **재시작 복원**(N26)은
//! 호스트 쪽 경로가 다르다 — `ClosedItem` vs 레이아웃 파일.
//!
//! 그런데 **우리 쪽 진입점은 같다**: 호스트가 둘 다
//! `(def.snapshot)` 으로 담고 `(def.restore)` 로 되돌린다
//! (`snapshot_fn_for` · `rebuild_surface` 의 `ClosedPanel::Generic` 갈래).
//!
//! ⇒ 스냅샷이 세션을 실으면 **두 경로가 함께 산다**. 그 불변식을
//! 여기서 고정한다 — 한쪽만 고치면 다른 쪽이 조용히 빠진다.

use super::*;

/// **대화가 스냅샷을 타고 돌아온다**(R4).
///
/// # 왜 데몬에 안 맡기나
///
/// 실측(2026-09-25): `session.output` 은 **답만** 들고 질문이 없다
/// (두 턴 뒤 `["2","6"]` — 내가 보낸 `1+1`·`3+3` 이 없다). 게다가
/// `output_tail` 은 **메모리 맵**이라 데몬 재기동에 사라지고 200줄
/// 상한이 있다. ⇒ 대화 재구성이 불가능하다.
///
/// **좌표와 다른 축이다** — 좌표는 *"어느 세션인가"*(데몬이 안다)
/// 이고 턴은 *"이 탭이 무엇을 보여 주고 있었나"*(우리만 안다)다.
#[test]
fn the_conversation_rides_the_snapshot() {
    let turns = vec![
        Turn {
            sent: "안녕".into(),
            got: "반가워".into(),
            error: None,
            pending: false,
            at: None,
        },
        Turn {
            sent: "1+1".into(),
            got: String::new(),
            error: Some("데몬 없음".into()),
            pending: false,
            at: None,
        },
    ];
    let snap = snapshot_with_turns(Some("s-1"), &turns);

    let back = turns_from_snapshot(&snap);
    assert_eq!(back.len(), 2, "턴이 사라졌다");
    assert_eq!(back[0].sent, "안녕");
    assert_eq!(back[0].got, "반가워");
    // **실패도 남는다** — 답이 안 온 것과 실패한 것은 다르다.
    assert_eq!(back[1].error.as_deref(), Some("데몬 없음"));
    // 세션은 그대로 실린다(N26 계약).
    assert_eq!(snap["session_id"], "s-1");
}

/// 턴이 없으면 **칸을 안 만든다** — 빈 배열도 안 싣는다.
#[test]
fn an_empty_conversation_adds_no_field() {
    let snap = snapshot_with_turns(Some("s-1"), &[]);
    assert!(snap.get("turns").is_none(), "빈 대화가 칸을 만들었다");
    assert!(turns_from_snapshot(&snap).is_empty());
}

/// 모양이 틀린 스냅샷은 **빈 대화**로 읽는다 — 화면이 비지 않게.
#[test]
fn a_malformed_snapshot_yields_no_turns_not_a_crash() {
    for bad in [
        serde_json::json!({}),
        serde_json::json!({ "turns": "문자열" }),
        serde_json::json!({ "turns": [ { "sent": 1 } ] }),
    ] {
        assert!(
            turns_from_snapshot(&bad).is_empty(),
            "{bad} 에서 턴이 나왔다"
        );
    }
}

/// 되살리기와 재시작이 **같은 값**을 받는다.
#[test]
fn reopen_and_restart_carry_the_same_session() {
    let snap = snapshot_of(Some("sess-keep"));
    // 호스트는 이 값을 `ClosedPanel::Generic.snapshot` 에도,
    // 레이아웃 파일에도 **그대로** 넣는다. 우리가 갈라 담지 않는다.
    assert_eq!(snap["session_id"], "sess-keep");

    // 복원 쪽이 읽는 키와 같아야 한다(`open_session_surface` 의 `field`).
    let back = snap.get("session_id").and_then(serde_json::Value::as_str);
    assert_eq!(back, Some("sess-keep"), "되살리기가 세션을 못 읽는다");
}

/// **세션이 없으면 빈 스냅샷** — 되살린 탭이 시작 폼을 그린다.
///
/// 그 경우 `ClosedPanel::Generic` 은 여전히 만들어진다(kind 가 있으므로).
/// 빈 객체를 내는 것과 `None` 을 내는 것은 다르다 — `None` 이면 호스트가
/// **그 탭을 되살릴 수 없는 것**으로 보고 버린다.
#[test]
fn a_tab_with_no_session_is_still_restorable() {
    let snap = snapshot_of(None);
    assert!(snap.is_object(), "빈 스냅샷이 객체가 아니면 탭이 버려진다");
    assert!(snap.get("session_id").is_none());
}
/// WebKit 이 주는 **실물 모양**을 읽는다 — `%23`(2026-09-28 실측).
///
/// base 가 `about:blank` 라 WebKit 이 fragment 를 합칠 때 인코딩한다:
/// `about:blank%23c=%2Ftmp&agent-cwd=1`. `#` 로만 가르면 이 모양을
/// 못 읽고 단추가 **조용히 아무 일도 안 한다**.
/// **폼 제출 URL 끝의 `%23` 는 종결자다** — 구분자가 아니다(2026-09-28).
///
/// 폼이 `action="#"` 라 제출하면 `?a=…&agent-start=1%23` 로 온다.
/// fragment 를 먼저 보면 그 뒤(빈 문자열)를 질의로 읽어 **시작 단추가
/// 죽는다** — 실측으로 흰 화면을 밟았다.
#[test]
fn a_submitted_form_url_is_read_even_with_a_trailing_fragment() {
    let submit = "about:blank?a=cc&c=%2FUsers%2Fme%2Fdev&agent-start=1%23";
    assert_eq!(
        parse_start(submit),
        Some(StartForm {
            agent: "cc".into(),
            cwd: Some("/Users/me/dev".into()),
            ..Default::default()
        }),
        "제출이 안 읽힌다 — 시작 단추가 죽고 화면이 하얘진다"
    );
    // 날것 `#` 꼬리도 같다.
    assert!(parse_start("x?a=cc&agent-start=1#").is_some());
}

#[test]
fn the_encoded_fragment_webkit_actually_sends_is_read() {
    let real = "about:blank%23c=%2FUsers%2Fme%2Fdev%20x&agent-cwd=1";
    assert_eq!(
        parse_cwd_pick(real),
        Some("/Users/me/dev x".to_string()),
        "WebKit 실물 모양(%23)을 못 읽는다 — 단추가 조용히 죽는다"
    );
    // 날것 `#` 도 받는다(다른 백엔드·미래 대비).
    assert_eq!(
        parse_cwd_pick("about:blank#c=%2Ftmp&agent-cwd=1"),
        Some("/tmp".to_string())
    );
    // 종전 `?` 모양도 계속 읽는다 — 폼 제출이 그 모양이다.
    assert_eq!(
        parse_cwd_pick("x?c=%2Ftmp&agent-cwd=1"),
        Some("/tmp".to_string())
    );
}

/// **단추는 fragment 로 간다** — `?` 로 되돌아가면 화면이 깜빡인다.
#[test]
fn the_buttons_use_fragments_so_the_page_is_not_reloaded() {
    let h = render_start(&[], &[], &[], "", None);
    assert!(h.contains(r##"href="#"##), "단추가 fragment 를 안 쓴다");
    assert!(
        !h.contains(r##"<a href="?"##),
        "단추가 쿼리로 간다 — WebKit 이 문서를 버려 페이지가 깜빡인다"
    );
}
