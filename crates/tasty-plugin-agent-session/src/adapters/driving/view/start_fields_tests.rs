use super::{StartForm, parse_start, render_start};

fn prof(name: &str) -> crate::adapters::driven::store::agents::AgentProfile {
    crate::adapters::driven::store::agents::AgentProfile {
        name: name.into(),
        kind: "claude".into(),
        cwd: None,
        env: Default::default(),
        ..Default::default()
    }
}

/// **폼이 세션당 값 둘을 받는다**(모델·이름 · 0.1.56).
///
/// 매니페스트는 `preset_fields` 로 이 셋을 처음부터 선언했고 데몬도
/// 받는데, **이 폼만 안 냈다**. 선언·배선이 있어도 입력칸이 없으면
/// 사용자에게는 **없는 기능**이다.
#[test]
fn the_start_form_offers_model_and_name() {
    let h = render_start(&[], &[], &[prof("cc")], "", None);
    for (id, what) in [
        (r##"<input id="m""##, "모델"),
        (r##"<input id="n""##, "세션 이름"),
    ] {
        assert!(h.contains(id), "{what} 칸이 없다 — 값을 줄 자리가 없다");
    }
}

/// **접은 칸이 눌러 보이는가** — 마크업만으로는 안 보인다.
///
/// 0.1.51 이 `<details>` 를 넣고 **스타일을 안 붙였다**. 어두운 바탕에
/// 브라우저 기본 `summary` 는 거의 안 보여, 사용자에게는 *"필드가 없는
/// 화면"* 이었다 — 그때 시험은 `input id="m"` 을 찾아 **통과하고
/// 있었다**. *"마크업이 있다"* 와 *"보인다"* 는 다른 질문이다.
#[test]
fn the_folded_fields_look_clickable() {
    let h = render_start(&[], &[], &[prof("cc")], "", None);
    assert!(h.contains("<details"), "접는 블록이 없다");
    // ⚠️ `contains("details.more>summary")` 로는 못 가른다 — 아래
    // `::before` 규칙이 같은 접두를 가져 **그 줄만 지워도 통과했다**
    // (변이로 밟았다). **규칙 자체**가 있는지 본다.
    assert!(
        h.contains("details.more>summary{"),
        "`summary` 스타일 규칙이 없다 — 어두운 바탕에서 안 보인다"
    );
    assert!(
        h.contains("details.more>summary::before"),
        "접힘 표시가 없다 — 누를 수 있는지 모른다"
    );
    assert!(
        h.contains("details.more[open]>summary::before"),
        "펼침 표시가 없다 — 접힘과 같은 모양이 된다"
    );
    assert!(h.contains("cursor:pointer"), "손 모양 커서가 없다");
}

/// **첫 메시지 칸은 없다**(0.1.56 · 사용자 지정).
///
/// 대화창에서 바로 치면 되고, 시작 폼에 두면 같은 일을 두 자리에서
/// 하게 된다. 양성 대조이기도 하다 — 위 시험이 *"칸이 있다"* 만
/// 보므로, 없어야 할 것이 생겨도 거기선 안 걸린다.
#[test]
fn there_is_no_first_message_field() {
    let h = render_start(&[], &[], &[prof("cc")], "", None);
    assert!(
        !h.contains(r##"<input id="p""##),
        "첫 메시지 칸이 남아 있다"
    );
}

/// **그 칸이 실제로 읽힌다** — 그려 놓고 안 읽으면 같은 일이다.
#[test]
fn what_the_form_sends_is_read_back() {
    let url = "about:blank?a=cc&c=%2Ftmp&m=sonnet&n=%EC%9D%BC&ad=f395ed9e&r=builder&r=bdd-reviewer&agent-start=1";
    assert_eq!(
        parse_start(url),
        Some(StartForm {
            agent: "cc".into(),
            cwd: Some("/tmp".into()),
            model: Some("sonnet".into()),
            name: Some("일".into()),
            adopt: Some("f395ed9e".into()),
            permission_mode: None,
            roles: vec!["builder".into(), "bdd-reviewer".into()],
        })
    );
}

///  **제출 스크립트가 그 칸들을 실제로 싣는가** — 손으로 적은 URL 로는
/// 안 갈린다.
///
/// 위 시험은 **파서**만 본다. 그래서 `m=`·`n=` 이 파서에는 있고 제출에는
/// 없는 상태가 **0.1.51~56 내내 통과**했다(모델·이름 칸이 죽어 있었다).
/// 같은 함정을 0.1.16 에서 이미 밟았다 — *"시험 13건이 URL 을 손으로
/// 적어 브라우저가 만드는 문자열을 한 번도 안 봤다"*.
///
/// ⇒ **렌더된 스크립트에서** 그 칸을 싣는 코드를 찾는다.
#[test]
fn the_submit_script_carries_every_folded_field() {
    let html = render_start(&[], &[], &[], "", None);
    // 제출 핸들러가 읽는 id 목록 — 하나라도 빠지면 그 칸은 죽는다.
    for id in ["m", "n", "ad"] {
        assert!(
            html.contains(&format!("['{id}','{id}']")),
            "제출이 `{id}` 칸을 안 싣는다 — 그리기만 하고 전송은 안 된다"
        );
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "`{id}` 입력칸 자체가 없다"
        );
    }
    // 분모 — 셋을 실제로 셌다.
    assert!(html.contains("extra.length"), "추가 칸을 도는 루프가 없다");
}

/// **양성 대조** — 안 준 칸은 `None` 이다.
///
/// 이것이 없으면 위 시험은 파서가 **아무 값이나 채워도** 통과하고,
/// 그러면 빈 칸이 빈 문자열로 가서 데몬이 그것을 값으로 받는다.
#[test]
fn fields_left_empty_do_not_become_values() {
    let form =
        parse_start("about:blank?a=cc&m=&n=&agent-start=1").expect("agent 만 있어도 읽혀야 한다");
    assert_eq!(form.agent, "cc");
    for (got, what) in [
        (&form.cwd, "cwd"),
        (&form.model, "model"),
        (&form.name, "name"),
    ] {
        assert!(got.is_none(), "빈 {what} 가 값이 됐다: {got:?}");
    }
}
