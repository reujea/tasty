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

/// Given 다른 OSC를 C1 CSI가 끝내고 그 CSI를 ESC가 끊으며 OSC 52 쓰기가 시작된다, When 본문이 오는 중이다,
/// Then OSC가 끝났으므로 그 ESC는 새 머리이고 쓰기를 접수한다.
#[test]
fn a_write_after_an_osc_ended_by_a_c1_csi_is_received() {
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]0;t\x9b1;\x1b]52;c;aGk=", at(0));
    assert_eq!(waits(&progress), vec![at(0)]);
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

fn repeat(chunk: &[u8], times: usize) -> Vec<u8> {
    chunk.repeat(times)
}

/// Given 머리 한도(64바이트)보다 긴 앞 시퀀스가 dispatch 없이 열려 있다, When 그 뒤 ESC나 C1로 OSC 52 쓰기가
/// 시작해 본문이 온다, Then 본문을 받는 중으로 접수한다. ESC·C1이 dispatch 없이도 기준점이기 때문이다.
#[test]
fn a_write_after_a_long_open_sequence_is_received() {
    const WRITE: &[u8] = b"\x1b]52;c;aGk=";
    let cases: [(&str, Vec<u8>); 9] = [
        (
            "끝나지 않은 CSI",
            [b"\x1b[".as_slice(), &[b'1'; 62]].concat(),
        ),
        (
            "파라미터가 긴 CSI",
            [b"\x1b[".to_vec(), repeat(b"1;", 40)].concat(),
        ),
        ("CSI 무시", [b"\x1b[:".as_slice(), &[b'1'; 70]].concat()),
        ("SOS", [b"\x1bX".as_slice(), &[b'a'; 70]].concat()),
        ("PM", [b"\x1b^".as_slice(), &[b'a'; 70]].concat()),
        (
            "DCS 파라미터",
            [b"\x1bP".to_vec(), repeat(b"1;", 40)].concat(),
        ),
        ("DCS 무시", [b"\x1bP:".as_slice(), &[b'1'; 70]].concat()),
        ("C1 SOS", [b"\x98".as_slice(), &[b'a'; 70]].concat()),
        ("C1 CSI", [b"\x9b".as_slice(), &[b'1'; 70]].concat()),
    ];
    for (name, before) in cases {
        let input = [before.as_slice(), WRITE].concat();
        assert!(oracle_receiving(&input), "기준 {name}");
        assert!(tracker_receiving(&input), "{name}");
    }
    // OSC 문자열 밖이면 C1 OSC 도입자 0x9d도 기준점이다.
    let input = [b"\x98".as_slice(), &[b'a'; 70], b"\x9d52;c;aGk="].concat();
    assert!(oracle_receiving(&input));
    assert!(tracker_receiving(&input), "SOS 뒤 0x9d");
    // 긴 OSC 문자열 안 미완성 UTF-8이 삼킨 ESC는 기준점이 아니다.
    let input = [b"\x1b]0;".as_slice(), &[b'a'; 70], b"\xe4", WRITE].concat();
    assert!(!oracle_receiving(&input));
    assert!(!tracker_receiving(&input), "글자에 먹힌 ESC");
    // 남는 한계: OSC 52 자신의 선택 인자가 한도를 넘으면 진행 표시를 놓친다.
    let input = [b"\x1b]52;".as_slice(), &[b'c'; 70], b";aGk="].concat();
    assert!(oracle_receiving(&input));
    assert!(!tracker_receiving(&input), "선택 인자가 긴 OSC 52");
}

/// Given ground·OSC 문자열 밖의 상태에서 리드 바이트나 0x9d가 머리 한도 근처에 온다, When 그 뒤 OSC 52 쓰기가
/// 시작한다, Then 실제 파서대로 판정한다. 그런 상태의 리드 바이트는 글자를 시작하지 않아 바로 뒤 ESC가
/// 기준점이고, 0x9d는 앞 시퀀스를 끝내고 새 OSC를 연다.
#[test]
fn lead_bytes_and_0x9d_outside_osc_strings_near_the_head_limit() {
    const WRITE: &[u8] = b"\x1b]52;c;aGk=";
    for n in 56..=64 {
        for (name, intro) in [("CSI", b"\x1b[".as_slice()), ("SOS", b"\x1bX")] {
            let input = [intro, &vec![b'1'; n], b"\xc2", WRITE].concat();
            assert!(oracle_receiving(&input), "기준 {name} {n}");
            assert!(tracker_receiving(&input), "{name} 리드 뒤 ESC {n}");
        }
        let input = [b"\x1b[".as_slice(), &vec![b'1'; n], b"\x9d52;c;aGk="].concat();
        assert!(oracle_receiving(&input), "기준 0x9d {n}");
        assert!(tracker_receiving(&input), "CSI를 끝낸 0x9d {n}");
    }

    // CSI 안 0xe4는 글자를 시작하지 않는다. 뒤의 제목 OSC 안 0x9d는 아무것도 하지 않으므로 쓰기가 아니다.
    let input = [
        b"\x1b[".as_slice(),
        &[b'1'; 61],
        b"\xe4\x1b]0;t\x9d52;c;aGk=",
    ]
    .concat();
    assert!(!oracle_receiving(&input));
    let mut progress = CopyProgress::default();
    progress.feed(SID, &input, at(0));
    assert!(waits(&progress).is_empty());
    progress.break_stream(SID);
    assert!(progress.take_failures().is_empty(), "없는 복사의 실패");
}

/// Given OSC 52 머리의 `5`·`2`·`;` 사이에 OSC 문자열이 무시하는 바이트가 끼어 있다, When 나뉘어 도착한다,
/// Then 무시된 바이트 없이 온 쓰기처럼 본문 첫 바이트에서 접수한다.
#[test]
fn ignored_bytes_inside_the_52_parameter_do_not_hide_a_write() {
    for ignored in [0x00u8, 0x9d, 0xa0, 0xff] {
        let mut progress = CopyProgress::default();
        progress.feed(SID, b"\x1b]5", at(0));
        progress.feed(SID, &[ignored], at(1));
        progress.feed(SID, b"2;c;", at(2));
        assert!(waits(&progress).is_empty(), "{ignored:02x}");
        progress.feed(SID, b"aGk=", at(3));
        assert_eq!(waits(&progress), vec![at(3)], "{ignored:02x}");
        let input = [b"\x1b]5".as_slice(), &[ignored], b"2;c;aGk="].concat();
        assert!(oracle_receiving(&input), "기준 {ignored:02x}");
    }
}

/// Given 쓰기 본문이 미완성 UTF-8로 끝난다, When 실제 종결이 온다, Then 접수는 실제 바이트가 정한 데이터를
/// 따른다. C2 리드 뒤 0x9c는 UTF-8로 쓴 ST라 데이터 `?`의 질의가 된다.
#[test]
fn an_unfinished_utf8_tail_is_judged_by_real_terminators() {
    let cases: [(&[u8], bool); 6] = [
        (b"\x1b]52;c;?\xc2", false),
        (b"\x1b]52;c;\xc2", false),
        (b"\x1b]52;c;a\xc2", true),
        // C2가 아닌 리드는 어떻게 끝나도 데이터에 글자 하나가 들어간다.
        (b"\x1b]52;c;\xe4", true),
        (b"\x1b]52;c;?\xe4\xb8", true),
        (b"\x1b]52;c;?\xc2\x07", true),
    ];
    for (input, expected) in cases {
        assert_eq!(oracle_receiving(input), expected, "기준 {input:02x?}");
        assert_eq!(tracker_receiving(input), expected, "{input:02x?}");
    }

    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;?\xc2", at(0));
    assert!(waits(&progress).is_empty());
    progress.feed(SID, b"\x9c", at(1));
    assert!(waits(&progress).is_empty(), "UTF-8 ST로 끝난 질의");
    assert!(progress.take_failures().is_empty());

    for terminator in [b"\x07".as_slice(), b"\x1b\\", b"\x9c", b"\xc2\x9c"] {
        let mut progress = CopyProgress::default();
        progress.feed(SID, b"\x1b]52;c;aG", at(0));
        progress.feed(SID, &[b"k=".as_slice(), terminator].concat(), at(1));
        assert_eq!(waits(&progress), vec![at(0)], "{terminator:02x?}");
        assert!(progress.settle_write(SID));
    }
}

/// Given 쓰기가 머리 한도(64바이트) 근처에서 여러 조각으로 나뉘어 온다, When 본문 첫 바이트가 도착한다,
/// Then 그 조각에서 접수하고 종결 뒤 결과 대기로 넘긴다.
#[test]
fn a_write_split_across_the_head_limit_is_received() {
    let before = [b"\x1b[".as_slice(), &[b'1'; 58]].concat();
    let mut progress = CopyProgress::default();
    progress.feed(SID, &[before.as_slice(), b"\x1b]5"].concat(), at(0));
    progress.feed(SID, b"2;c;", at(1));
    assert!(waits(&progress).is_empty());
    progress.feed(SID, b"aG", at(2));
    assert_eq!(waits(&progress), vec![at(2)]);
    progress.feed(SID, b"k=\x07", at(3));
    assert_eq!(waits(&progress), vec![at(2)]);
    assert!(progress.settle_write(SID));

    // 데이터가 한도를 넘어 길어져도 접수는 그대로다.
    let mut progress = CopyProgress::default();
    progress.feed(SID, b"\x1b]52;c;", at(0));
    for step in 1..=10 {
        progress.feed(SID, &[b'Q'; 10], at(step));
    }
    progress.feed(SID, b"\x07", at(11));
    assert_eq!(waits(&progress), vec![at(1)]);
}

/// 기준(oracle): 입력 뒤에 실제 종결 하나(BEL, ESC \\, 0x9c, UTF-8로 쓴 ST `C2 9C`)를 OSC가 끝날 때까지(최대
/// 네 번) 이어 붙여 vtparse 0.6.2에 먹인다. 네 종결 모두에서 붙인 바이트의 첫 OSC dispatch가 OSC 52 쓰기면(첫
/// 파라미터가 `52`이고 셋째 파라미터부터의 데이터가 비지 않았고 질의 `?`가 아니다) 입력 끝에 열린 시퀀스가 쓰기
/// 본문을 받는 중이다. 곧 실제 바이트가 어떻게 이 OSC를 끝내도 쓰기다. 종결이 입력 끝의 미완성 UTF-8에 먹히면
/// OSC가 끝날 때까지 같은 종결을 더 붙인다. 그래서 C2 리드는 0x9c와 함께 UTF-8 ST가 되어 데이터를 바꾸지 않고
/// 끝날 수 있다. 다른 미완성 리드는 어느 실제 종결로도 글자(잘못된 시퀀스면 대체 문자)가 데이터에 남아 쓰기
/// 시도로 센다. 그런 데이터는 termwiz가 base64로 받지 않아 `Ended::Invalid`(실패)로 끝난다.
fn oracle_receiving(input: &[u8]) -> bool {
    #[derive(Default)]
    struct Oscs(Vec<Vec<Vec<u8>>>);
    impl vtparse::VTActor for Oscs {
        fn print(&mut self, _: char) {}
        fn execute_c0_or_c1(&mut self, _: u8) {}
        fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
        fn dcs_put(&mut self, _: u8) {}
        fn dcs_unhook(&mut self) {}
        fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {}
        fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {}
        fn osc_dispatch(&mut self, params: &[&[u8]]) {
            self.0.push(params.iter().map(|p| p.to_vec()).collect());
        }
        fn apc_dispatch(&mut self, _: Vec<u8>) {}
    }
    let ends_as_write = |terminator: &[u8]| {
        let mut parser = vtparse::VTParser::new();
        let mut oscs = Oscs::default();
        parser.parse(input, &mut oscs);
        let before = oscs.0.len();
        for _ in 0..4 {
            parser.parse(terminator, &mut oscs);
            if oscs.0.len() > before {
                break;
            }
        }
        let Some(params) = oscs.0.get(before) else {
            return false;
        };
        if params.len() < 3 || params[0] != b"52" {
            return false;
        }
        let data = params[2..].join(&b';');
        !data.is_empty() && data != b"?"
    };
    [b"\x07".as_slice(), b"\x1b\\", b"\x9c", b"\xc2\x9c"]
        .into_iter()
        .all(ends_as_write)
}

fn tracker_receiving(input: &[u8]) -> bool {
    let mut progress = CopyProgress::default();
    progress.feed(SID, input, at(0));
    progress
        .surfaces
        .get(&SID)
        .is_some_and(|scan| scan.receiving.is_some())
}

/// 경계 바이트와 짧은 조각. `52;`·`c;`처럼 묶은 것은 짧은 길이로도 쓰기의 머리가 생기게 하려는 것이다.
/// `?`·`\xc2`·UTF-8 ST·ESC \\는 입력 안의 실제 종결과 미완성 UTF-8을 만든다.
const TOKENS: [&[u8]; 27] = [
    b"\x1b",
    b"\x1b]",
    b"]",
    b"\x9b",
    b"\x9d",
    b"\x9c",
    b"\x07",
    b"\\",
    b"52;",
    b"c;",
    b"0;",
    b"1;",
    b"\xe4",
    b"a",
    b"?",
    b"\xc2",
    b"\xc2\x9c",
    b"\x1b\\",
    b"\x1bP",
    b"\x1bX",
    b"5",
    b"2",
    b";",
    b"\x00",
    b"\xa0",
    b"\xff",
    b"\xe4\xb8",
];

fn mismatches(inputs: impl Iterator<Item = Vec<u8>>) -> Vec<String> {
    inputs
        .filter(|input| oracle_receiving(input) != tracker_receiving(input))
        .take(8)
        .map(|input| {
            format!(
                "{input:02x?} 기준={} 추적기={}",
                oracle_receiving(&input),
                tracker_receiving(&input)
            )
        })
        .collect()
}

/// 지적된 경계 입력과 기준이 정한 값. 차등 비교에도 같은 입력을 넣는다.
const NAMED: [(&[u8], bool); 13] = [
    (b"\x1b]\x9b1;\x1b]52;c;aGk=", true),
    (b"\x1b]0;t\x9b\x9d0;u\xe4\x1b]52;c;aGk=", false),
    // OSC 문자열 안의 0x9d는 vtparse에서 같은 상태로 가는 전이라 아무것도 하지 않는다. 새 OSC가 아니다.
    (b"\x1b]0;t\x9d52;c;aGk=", false),
    (b"\x1b]0;t\x9b1;\x1b]52;c;aGk=", true),
    (b"\x1b]0;t\xe4\x1b]52;c;aGk=", false),
    // 리드마다 첫 연속 바이트의 범위가 다르다. 제목 OSC 안에서 범위 밖이면 글자가 끝나 다음 C2가 새 글자를
    // 시작하고 ESC를 삼킨다. 범위 안이면 C2가 잘못된 바이트로 소비되고 ESC가 제목을 끝내며 쓰기를 연다.
    (b"\x1b]0;t\xe0\x80\xc2\x1b]52;c;aGk=", false),
    (b"\x1b]0;t\xe0\xa0\xc2\x1b]52;c;aGk=", true),
    (b"\x1b]0;t\xed\xa0\xc2\x1b]52;c;aGk=", false),
    (b"\x1b]0;t\xed\x80\xc2\x1b]52;c;aGk=", true),
    (b"\x1b]0;t\xf0\x80\xc2\x1b]52;c;aGk=", false),
    (b"\x1b]0;t\xf0\x90\xc2\x1b]52;c;aGk=", true),
    (b"\x1b]0;t\xf4\x90\xc2\x1b]52;c;aGk=", false),
    (b"\x1b]0;t\xf4\x80\xc2\x1b]52;c;aGk=", true),
];

/// Given 경계 바이트로 만든 입력, When 추적기와 vtparse 기준(`oracle_receiving`)이 입력 끝에 쓰기 본문을
/// 받는 중인지 판정한다, Then 모든 입력에서 같다. 토큰 4개까지는 전수, 5~12개는 고정 시드 무작위다.
#[test]
fn receiving_matches_vtparse_on_boundary_inputs() {
    for (input, expected) in NAMED {
        assert_eq!(oracle_receiving(input), expected, "기준 {input:02x?}");
    }
    let named = NAMED.iter().map(|(input, _)| input.to_vec());
    assert_eq!(mismatches(named), Vec::<String>::new());

    let mut exhaustive = Vec::new();
    let mut frontier = vec![Vec::<u8>::new()];
    for _ in 0..4 {
        frontier = frontier
            .iter()
            .flat_map(|prefix| {
                TOKENS.iter().map(move |token| {
                    let mut next = prefix.clone();
                    next.extend_from_slice(token);
                    next
                })
            })
            .collect();
        exhaustive.extend(frontier.iter().cloned());
    }
    assert_eq!(mismatches(exhaustive.into_iter()), Vec::<String>::new());

    let mut state: u64 = 0x5eed_c0de;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let random = (0..400_000).map(|_| {
        let len = 5 + (next() % 8) as usize;
        (0..len)
            .flat_map(|_| {
                TOKENS[(next() % TOKENS.len() as u64) as usize]
                    .iter()
                    .copied()
            })
            .collect::<Vec<u8>>()
    });
    assert_eq!(mismatches(random), Vec::<String>::new());

    // 머리 한도를 넘는 앞 시퀀스 뒤. 긴 조각은 앞 시퀀스 안에만 둔다(OSC 52 자신의 선택 인자가 한도를 넘는
    // 경우는 문서의 남은 한계라 비교하지 않는다).
    let intros: [(&[u8], &[u8]); 9] = [
        (b"\x1b[", b""),
        (b"\x1b[:", b""),
        (b"\x1bX", b""),
        (b"\x1bP", b""),
        (b"\x1bP:", b""),
        (b"\x1b]0;", b""),
        (b"\x1b]0;", b"\xe4"),
        (b"\x9b", b""),
        (b"\x98", b""),
    ];
    // 한도 앞뒤 길이. 앞 시퀀스가 한도에 닿기 직전·직후에 기준점이 오는 경우를 함께 본다.
    let long_prefixes: Vec<Vec<u8>> = intros
        .iter()
        .flat_map(|(intro, tail)| {
            [56, 58, 59, 60, 61, 62, 63, 66, 70].map(|n| [*intro, &vec![b'1'; n], *tail].concat())
        })
        .collect();
    // 토큰 셋으로 쓰기가 생기도록 쓰기 머리 조각을 더한 작은 알파벳.
    let alphabet: [&[u8]; 16] = [
        b"\x1b",
        b"\x9d",
        b"\x9c",
        b"\x07",
        b"\xc2",
        b"\xe4",
        b"\xe4\xb8",
        b"a",
        b"?",
        b"52;",
        b"c;",
        b";",
        b"\x00",
        b"\x1b]52;c;",
        b"\x9d52;c;",
        b"\x1b]0;t",
    ];
    let after_long = long_prefixes.iter().flat_map(|prefix| {
        let mut inputs = vec![prefix.clone()];
        for _ in 0..3 {
            inputs = inputs
                .iter()
                .flat_map(|input| {
                    alphabet
                        .iter()
                        .map(move |token| [input.as_slice(), token].concat())
                })
                .collect();
        }
        inputs
    });
    assert_eq!(mismatches(after_long), Vec::<String>::new());
}
