//! 플러그인 붙여넣기 읽기 시험.

use super::*;

/// 요청마다 tag 가 다르고, 팝업 tag·호스트 위젯 tag(포커스 없음의 0 포함)와 겹치지 않는다.
#[test]
fn every_read_gets_its_own_tag_apart_from_host_and_popup_tags() {
    let mut reads = PluginPasteReads::default();
    let (a, b) = (reads.next_tag(), reads.next_tag());
    assert_ne!(a, b);
    for tag in [a, b] {
        assert_ne!(tag, 0);
        assert_ne!(tag, crate::gfx::gpu::text_selection::plugin_popup_tag(1));
        assert_ne!(tag, egui::Id::new("field").value());
    }
}

/// 기다리던 읽기의 결과는 App 에 가고 나머지(호스트 입력칸·`HeldPastes` 몫)에 섞이지 않는다. 실패와 빈 글자는
/// 글자 없음이다.
#[test]
fn the_awaited_result_goes_to_the_plugin_and_not_to_host_pastes() {
    let mut reads = PluginPasteReads::default();
    let tag = reads.next_tag();
    reads.started(tag);
    assert!(reads.wants_failures() && reads.busy());
    let rest = reads.sort(
        vec![(7, Some("host".into())), (tag, Some("hi".into()))],
        false,
    );
    assert_eq!(rest, vec![(7, Some("host".into()))]);
    assert_eq!(
        reads.take_arrived(),
        vec![(tag, PluginPasteEnd::Text("hi".into()))]
    );
    assert!(!reads.busy() && !reads.wants_failures());

    for text in [None, Some(String::new())] {
        let tag = reads.next_tag();
        reads.started(tag);
        assert!(reads.sort(vec![(tag, text)], false).is_empty());
        assert_eq!(reads.take_arrived(), vec![(tag, PluginPasteEnd::Nothing)]);
    }
}

/// 결과 없이 읽기가 더 진행되지 않으면 다른 읽기가 대신한 것이다. 진행 중이면 계속 기다린다.
#[test]
fn a_read_that_stops_without_a_result_was_replaced() {
    let mut reads = PluginPasteReads::default();
    let tag = reads.next_tag();
    reads.started(tag);
    assert!(reads.sort(Vec::new(), true).is_empty());
    assert!(reads.take_arrived().is_empty());
    reads.sort(Vec::new(), false);
    assert_eq!(reads.take_arrived(), vec![(tag, PluginPasteEnd::Replaced)]);
}

/// 끝낸(취소·시간 초과) 읽기의 늦은 결과는 버려진다. 호스트 입력칸·`HeldPastes`·다음 요청 어디에도 가지 않는다.
#[test]
fn a_late_result_of_an_ended_read_is_dropped() {
    let mut reads = PluginPasteReads::default();
    let tag = reads.next_tag();
    reads.started(tag);
    assert!(reads.cancel(tag), "진행 중인 읽기는 호출자가 취소한다");
    assert!(!reads.busy());
    let next = reads.next_tag();
    reads.started(next);
    let rest = reads.sort(vec![(tag, Some("late".into()))], true);
    assert!(rest.is_empty(), "늦은 결과가 다른 입력칸으로 샜다");
    assert!(
        reads.take_arrived().is_empty(),
        "늦은 결과가 다음 요청에 갔다"
    );
    assert!(!reads.cancel(tag), "이미 끝낸 읽기");
}

/// 넘기기 전에 끝낸 읽기의 결과는 App 에 가지 않는다.
#[test]
fn cancelling_drops_an_arrived_result_not_yet_taken() {
    let mut reads = PluginPasteReads::default();
    let tag = reads.next_tag();
    reads.started(tag);
    reads.sort(vec![(tag, Some("hi".into()))], false);
    assert!(!reads.cancel(tag));
    assert!(reads.take_arrived().is_empty());
}

/// 연결은 `GpuState` 없이 시험할 수 없어 소스로 고정한다. 끝난 읽기는 호스트 입력칸(`edit_keys_text_arrived`)과
/// `HeldPastes`보다 먼저 플러그인 기다림이 가려 가고, 기다리는 동안 실패도 받으며, 폴링이 이어진다. 기다리는 키 시점
/// 읽기는 읽는 중인 선택을 취소하지 않고 기다림으로 돌려준다. 팝업은 회신을 약속한 것만 기다린다.
#[test]
fn the_window_sorts_plugin_reads_before_host_pastes() {
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let src = squash(include_str!("text_selection.rs"));
    let failures = src
        .find(&squash(
            "if self.edit_keys.awaits_text() || self.plugin_reads.wants_failures() { selection.poll_with_failures()",
        ))
        .expect("기다리는 동안 실패를 받지 않는다");
    let sorted = src
        .find(&squash(
            "let finished = self.plugin_reads.sort(finished, selection.waiting());",
        ))
        .expect("플러그인 읽기를 가리지 않는다");
    let host = src
        .find(&squash("if !self.edit_keys_text_arrived(text.clone())"))
        .expect("호스트 입력칸 전달");
    assert!(failures < sorted && sorted < host, "순서가 바뀌었다");
    assert!(
        src.contains(&squash(
            "None if wait => { self.plugin_reads.started(tag); PluginClip::Pending(tag) }"
        )),
        "읽는 중인 선택을 기다리지 않는다"
    );
    assert!(
        src.contains(&squash(
            "|| self.edit_keys.awaits_text() || self.plugin_reads.busy()"
        )),
        "기다리는 동안 폴링이 멈춘다"
    );
    let popup = squash(include_str!("../../view/main/plugin_popup_keys.rs"));
    assert!(
        popup.contains(&squash(
            "let replies = self.state.plugin_popup_replies.contains(&instance);"
        )) && popup.contains(&squash("clip = self.base.gpu.plugin_paste(replies);")),
        "팝업이 회신 약속과 무관하게 기다린다"
    );
    let render = squash(include_str!("../../plugin_bridge/popup_render.rs"));
    assert!(
        render.contains(&squash(
            ".filter(|(snap, _)| mgr.supports_edit_outcome(&snap.plugin_id))"
        )),
        "렌더가 팝업의 회신 약속을 기록하지 않는다"
    );
}
