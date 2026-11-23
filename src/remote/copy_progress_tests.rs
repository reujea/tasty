use super::*;
use std::time::Duration;

const SID: u32 = 7;

fn at(ms: u64) -> Instant {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *BASE.get_or_init(Instant::now) + Duration::from_millis(ms)
}

fn waits(progress: &CopyProgress) -> Vec<Instant> {
    let mut times: Vec<_> = progress.waits().map(|(_, since)| since).collect();
    times.sort();
    times
}

/// Given 쓰기 본문이 여러 묶음으로 온다, When 첫 데이터 바이트가 도착한다, Then 그때 접수되고
/// 끝 바이트가 와도 client 기록 결과가 올 때까지 남는다.
#[test]
fn a_split_write_is_admitted_at_its_first_data_byte_and_waits_for_the_result() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"text\x1b]5", at(0));
    assert!(
        waits(&progress).is_empty(),
        "도입자만으로는 접수하지 않는다"
    );
    progress.feed(SID, b"2;c;", at(1));
    assert!(waits(&progress).is_empty(), "데이터 전");
    progress.feed(SID, b"aGVs", at(2));
    assert_eq!(waits(&progress), vec![at(2)]);
    progress.feed(SID, b"bG8=\x07more", at(3));
    assert_eq!(waits(&progress), vec![at(2)], "결과 대기");
    assert!(progress.settle_write(SID));
    assert!(waits(&progress).is_empty());
    assert!(!progress.settle_write(SID), "결과는 한 번만 닫는다");
}

/// Given C1 도입자, UTF-8로 쓴 C1, ST 종료, When 쓰기가 끝난다, Then termwiz와 같이 쓰기로 본다.
#[test]
fn c1_and_utf8_c1_introducers_and_st_end_a_write_like_termwiz() {
    for bytes in [
        b"\x9d52;c;aGk=\x07".as_slice(),
        b"\xc2\x9d52;c;aGk=\x07".as_slice(),
        b"\x1b]52;c;aGk=\x1b\\".as_slice(),
    ] {
        let mut progress = CopyProgress::default();
        progress.feed(SID, bytes, at(0));
        assert_eq!(waits(&progress).len(), 1, "{bytes:?}");
        assert!(progress.settle_write(SID));
    }
}

/// Given 질의·지우기·다른 OSC, When 끝난다, Then 복사로 접수하지 않는다. 빈 본문 쓰기는 쓰기다.
#[test]
fn queries_clears_and_other_oscs_are_not_copies() {
    let mut progress = CopyProgress::default();
    for bytes in [
        b"\x1b]52;c;?\x07".as_slice(),
        b"\x1b]52;c\x07".as_slice(),
        b"\x1b]0;title\x07".as_slice(),
        b"\x1b]9;note\x07".as_slice(),
        "가나\x1b[31m".as_bytes(),
    ] {
        progress.feed(SID, bytes, at(0));
        assert!(waits(&progress).is_empty(), "{bytes:?}");
        assert!(progress.take_failures().is_empty(), "{bytes:?}");
    }
    progress.feed(SID, b"\x1b]52;c;\x07", at(1));
    assert_eq!(waits(&progress), vec![at(1)], "빈 문자열 쓰기");
}

/// Given termwiz가 받지 않는 본문(잘못된 base64), When 끝난다, Then 결과 대기 없이 바로 실패다.
#[test]
fn a_write_termwiz_rejects_fails_at_once() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;!!", at(0));
    assert_eq!(waits(&progress).len(), 1);
    progress.feed(SID, b"!!\x07", at(1));
    assert_eq!(progress.take_failures(), vec![SID]);
    assert!(waits(&progress).is_empty());
}

/// Given 실패할 쓰기 A와 성공할 쓰기 B가 한 묶음에 끝난다, When B의 기록 결과가 온다, Then A는 끝날 때
/// 실패로 빠졌으므로 그 결과가 B를 닫는다.
#[test]
fn a_failed_write_does_not_take_the_result_of_the_next_one() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;!!\x07\x1b]52;c;QQ==\x07", at(0));
    assert_eq!(progress.take_failures(), vec![SID]);
    assert_eq!(waits(&progress).len(), 1);
    assert!(progress.settle_write(SID));
    assert!(waits(&progress).is_empty());
}

/// Given vtparse가 무시하는 NUL이 든 질의, When 끝난다, Then 쓰기로 접수하지 않고 실패도 아니다.
#[test]
fn ignored_control_bytes_do_not_turn_a_query_into_a_write() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;\x00", at(0));
    assert!(waits(&progress).is_empty(), "NUL은 데이터가 아니다");
    progress.feed(SID, b"?\x07", at(1));
    assert!(waits(&progress).is_empty());
    assert!(progress.take_failures().is_empty());
    progress.feed(SID, b"\x1b]5\x002;c;Q\x00Q==\x07", at(2));
    assert_eq!(waits(&progress), vec![at(2)], "52 사이의 NUL");
}

/// Given CAN·ESC로 중단된 쓰기, When 끝난다, Then termwiz처럼 쌓인 본문으로 판정한다.
#[test]
fn an_aborted_write_is_judged_by_what_it_carried() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;aGk=\x18", at(0));
    assert_eq!(waits(&progress).len(), 1, "CAN: 받은 본문이 유효한 쓰기");
    assert!(progress.settle_write(SID));
    // ESC로 끝난 뒤 바로 다음 쓰기가 시작돼도 놓치지 않는다.
    progress.feed(SID, b"\x1b]52;c;aGk=\x1b]52;c;aGk", at(1));
    assert_eq!(waits(&progress), vec![at(1), at(1)]);
}

/// Given 다른 OSC·CSI를 ESC가 끊고 OSC 52 쓰기가 시작된다, When 본문이 아직 오는 중이다, Then 접수한다.
#[test]
fn a_write_started_by_the_esc_that_ends_another_sequence_is_received() {
    for lead in [&b"\x1b]0;title"[..], b"\x1b[1;2"] {
        let mut progress = CopyProgress::default();
        let mut bytes = lead.to_vec();
        bytes.extend_from_slice(b"\x1b]52;c;aGk");
        progress.feed(SID, &bytes, at(0));
        assert_eq!(waits(&progress), vec![at(0)], "{lead:?}");
    }
}

/// Given 다른 OSC의 UTF-8 첫 바이트 뒤에 ESC가 온다, When 파서가 그 ESC를 잘못된 UTF-8로 삼키고 OSC 52처럼
/// 보이는 바이트가 그 OSC의 본문으로 온다, Then 끝나기 전에도 접수하지 않아 끊겨도 실패가 없고, 끝나도
/// 진행 표시·실패가 남지 않는다.
#[test]
fn an_esc_swallowed_inside_another_osc_leaves_no_copy() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]0;t\xe4\x1b]52;c;aGk=", at(0));
    assert!(waits(&progress).is_empty(), "끝나기 전");
    progress.break_stream(SID);
    assert!(progress.take_failures().is_empty(), "끊김");
    progress.feed(SID, b"\x1b]0;t\xe4\x1b]52;c;aGk=\x07", at(1));
    assert!(waits(&progress).is_empty());
    assert!(progress.take_failures().is_empty());
    assert!(progress.surfaces().is_empty());
}

/// Given 한 묶음에서 쓰기 A가 끝나고 B가 시작된다, When A의 결과가 온다, Then A만 닫힌다.
#[test]
fn consecutive_writes_settle_in_byte_order() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg", at(0));
    progress.feed(SID, b"==", at(5));
    assert_eq!(waits(&progress).len(), 2);
    assert!(progress.settle_write(SID));
    assert_eq!(waits(&progress).len(), 1, "B는 아직 받는 중");
    progress.feed(SID, b"\x07", at(6));
    assert!(progress.settle_write(SID));
    assert!(waits(&progress).is_empty());
}

/// Given 결과 대기·받는 중인 쓰기, When 결과 없이 처리가 끝나거나 스트림이 끊긴다, Then 한 건마다 한 번
/// 실패로 알린다. 스트림이 끊기면 받는 중인 건만 실패하고 결과 대기 건은 사건 처리로 정산한다.
#[test]
fn unanswered_and_broken_writes_fail_once() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;QQ==\x07", at(0));
    progress.fail_unanswered(SID);
    assert_eq!(progress.take_failures(), vec![SID]);
    progress.fail_unanswered(SID);
    assert!(progress.take_failures().is_empty());

    progress.feed(SID, b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg", at(1));
    progress.break_stream(SID);
    assert_eq!(progress.take_failures(), vec![SID], "받는 중인 건");
    assert_eq!(waits(&progress), vec![at(1)], "결과 대기 건은 남는다");
    // 끊긴 뒤 남은 본문은 옛 시퀀스로 이어 보지 않는다.
    progress.feed(SID, b"==\x07", at(2));
    assert!(progress.take_failures().is_empty());
    assert!(progress.settle_write(SID));
    assert!(waits(&progress).is_empty());

    progress.feed(SID, b"\x1b]52;c;Qg", at(3));
    progress.forget(SID);
    assert!(waits(&progress).is_empty());
    assert!(
        progress.take_failures().is_empty(),
        "사라진 surface는 알리지 않는다"
    );
}
