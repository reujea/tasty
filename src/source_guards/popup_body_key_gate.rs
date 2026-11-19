//! host 팝업 본문 호출이 키 입력 문(`body_keys::with_body_keys`) 안에 있는지 확인한다. 문 안에 있으면 본문이
//! egui 입력을 직접 읽든(`key_pressed`·`consume_key`·`events`) 위젯 키 포커스로 읽든 키 주인이 아닌 본문은 키를
//! 보지 못한다. 본문마다 키 주인 표시를 확인하는 대신 이 한 곳을 지킨다. 텍스트 위치를 보며 조건 분기의 실제
//! 실행 여부는 판단하지 않는다.

use super::{repo_root, strip_comments};

const HOME: &str = "src/adapters/ui/popup/draw.rs";
const GATE: &str = "with_body_keys(";
const BODY_CALL: &str = "content_fn(&mut";

#[test]
fn every_popup_body_call_goes_through_the_key_gate() {
    let path = repo_root().join(HOME);
    let src = strip_comments(
        &std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
            .replace("\r\n", "\n"),
    );
    let calls: Vec<usize> = src.match_indices(BODY_CALL).map(|(at, _)| at).collect();
    assert!(
        !calls.is_empty(),
        "{HOME} 에 `{BODY_CALL}` 호출이 없다 — 본문 호출 이름이 바뀌었으면 이 검사를 함께 고친다"
    );
    for at in calls {
        let before = &src[..at];
        let gate = before.rfind(GATE).unwrap_or_else(|| {
            panic!("{HOME} 의 본문 호출이 `{GATE}` 밖에 있다. 키 주인이 아닌 본문도 키를 읽는다.")
        });
        // 문을 연 표현식과 본문 호출 사이에 문장 끝(`;`)이 있으면 문 밖이다.
        assert!(
            !before[gate..].contains(';'),
            "{HOME} 의 본문 호출이 `{GATE}` 표현식 안에 있지 않다: {}",
            &before[gate..]
        );
    }
}
