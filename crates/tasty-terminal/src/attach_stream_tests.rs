//! Ordered stream contracts; raw tap tests do not exercise this receiver's loss state.
use super::*;
use std::sync::Barrier;
use std::time::{Duration, Instant};

#[test]
fn ordered_output_and_resize_drain_before_one_loss_and_disconnect() {
    let mut terminal = Terminal::new_detached(80, 24);
    let mut subscription = terminal.snapshot_and_stream();
    assert_eq!((subscription.cols, subscription.rows), (80, 24));
    terminal.feed_bytes(b"first");
    terminal.resize(90, 30);
    for _ in 2..ATTACH_STREAM_MAX_EVENTS {
        terminal.feed_bytes(b"x");
    }
    terminal.feed_bytes(b"overflow");
    terminal.feed_bytes(b"must not resume");
    terminal.resize(100, 40);
    assert!(
        matches!(subscription.events.try_recv(), Ok(AttachEvent::Output(bytes)) if bytes == b"first")
    );
    assert!(matches!(
        subscription.events.try_recv(),
        Ok(AttachEvent::Resize { cols: 90, rows: 30 })
    ));
    for _ in 2..ATTACH_STREAM_MAX_EVENTS {
        assert!(
            matches!(subscription.events.try_recv(), Ok(AttachEvent::Output(bytes)) if bytes == b"x")
        );
    }
    assert!(matches!(
        subscription.events.try_recv(),
        Ok(AttachEvent::Loss)
    ));
    for _ in 0..2 {
        assert!(matches!(
            subscription.events.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }
    assert_eq!(subscription.events.state.bytes.load(Ordering::Acquire), 0);
}

#[test]
fn byte_limit_preserves_exact_prefix_and_receiver_drop_releases_subscription() {
    let mut terminal = Terminal::new_detached(80, 24);
    let mut subscription = terminal.snapshot_and_stream();
    let prefix = vec![b'x'; ATTACH_STREAM_MAX_BYTES - EVENT_CHARGE];
    // Use the actual producer under its parser lock without parsing a megabyte of glyphs.
    {
        let mut state = terminal.lock_state();
        state.fan_out_attach_output(&prefix);
        state.fan_out_attach_resize(81, 24);
        assert!(state.attach_streams.is_empty());
    }
    assert!(
        matches!(subscription.events.try_recv(), Ok(AttachEvent::Output(bytes)) if bytes == prefix)
    );
    assert!(matches!(
        subscription.events.try_recv(),
        Ok(AttachEvent::Loss)
    ));
    assert!(matches!(
        subscription.events.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    let fresh = terminal.snapshot_and_stream();
    drop(fresh);
    terminal.feed_bytes(b"still live");
    assert!(terminal.lock_state().attach_streams.is_empty());
    assert!(terminal.screen_text(false).contains("still live"));
}

#[test]
fn ordered_snapshot_subscription_accounts_for_concurrent_parser_ingest() {
    // Same parser-lock ingestion pattern as snapshot_and_tap's existing test. Keep the
    // entire workload below this channel's count bound so Loss cannot hide a cut error.
    const CHUNKS: usize = 128;
    const CHUNK: &[u8] = b"xxxxxxxx";
    for _ in 0..128 {
        let mut terminal = Terminal::new_detached(80, 40);
        let state = Arc::clone(&terminal.state);
        let fed = Arc::new(AtomicUsize::new(0));
        let worker_fed = Arc::clone(&fed);
        let feeder = std::thread::spawn(move || {
            for _ in 0..CHUNKS {
                state.lock().expect("parser state").ingest(CHUNK);
                worker_fed.fetch_add(1, Ordering::Release);
                std::thread::yield_now();
            }
        });
        while fed.load(Ordering::Acquire) < CHUNKS / 4 {
            std::thread::yield_now();
        }
        let mut subscription = terminal.snapshot_and_stream();
        feeder.join().expect("feeder");
        let mut observed = subscription
            .snapshot
            .iter()
            .filter(|&&byte| byte == b'x')
            .count();
        loop {
            match subscription.events.try_recv() {
                Ok(AttachEvent::Output(bytes)) => {
                    assert_eq!(bytes, CHUNK);
                    observed += bytes.len();
                }
                Err(mpsc::TryRecvError::Empty) => break,
                other => panic!("under-budget subscription lost its cut: {other:?}"),
            }
        }
        assert_eq!(observed, CHUNKS * CHUNK.len());
    }
}

#[test]
fn empty_poll_racing_final_enqueue_and_overflow_never_overtakes_prefix() {
    // Stress the real receiver with a producer racing its Empty poll. No test hook forces
    // the two internal receives: this supplements, rather than proves, every interleaving.
    let oversized = Arc::new(vec![0u8; ATTACH_STREAM_MAX_BYTES]);
    for _ in 0..1024 {
        let (sender, receiver) = mpsc::sync_channel(ATTACH_STREAM_MAX_EVENTS);
        let state = Arc::new(QueueState::default());
        let tap = AttachStreamTap {
            sender,
            state: state.clone(),
        };
        let mut events = AttachEventReceiver {
            receiver,
            state,
            ended: false,
        };
        assert!(matches!(events.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let start = Arc::new(Barrier::new(2));
        let worker_start = start.clone();
        let oversized = oversized.clone();
        let producer = std::thread::spawn(move || {
            worker_start.wait();
            assert!(tap.send(Some(b"retained"), 0, 0));
            // Even if the consumer drains concurrently, this event exceeds the byte cap.
            assert!(!tap.send(Some(&oversized), 0, 0));
        });
        start.wait();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut prefix = Vec::new();
        // 기한 뒤에도 쌓인 이벤트를 비울 때까지 한 번 더 읽는다. 경합을 겨냥한 yield 간격은 그대로 둔다.
        let mut past = false;
        loop {
            match events.try_recv() {
                Ok(AttachEvent::Output(bytes)) => prefix.extend(bytes),
                Ok(AttachEvent::Loss) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(!past, "producer stalled");
                    past = Instant::now() >= deadline;
                    if !past {
                        std::thread::yield_now();
                    }
                }
                other => panic!("unexpected event before loss: {other:?}"),
            }
        }
        producer.join().expect("producer");
        assert_eq!(
            prefix, b"retained",
            "Loss overtook the final accepted output"
        );
        assert!(matches!(
            events.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }
}

/// 같은 크기 요청에는 grid가 바뀌지 않아도 현재 크기의 Resize가 출력 순서 안에 남는다.
/// 일반 resize는 같은 크기에서 아무것도 남기지 않는다.
#[test]
fn same_size_attach_request_confirms_the_current_size_in_order() {
    let mut terminal = Terminal::new_detached(80, 24);
    let mut subscription = terminal.snapshot_and_stream();
    assert!(!terminal.resize(80, 24));
    assert!(matches!(
        subscription.events.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    terminal.feed_bytes(b"before");
    assert!(!terminal.resize_or_confirm_attach(80, 24));
    terminal.feed_bytes(b"after");
    assert!(
        matches!(subscription.events.try_recv(), Ok(AttachEvent::Output(bytes)) if bytes == b"before")
    );
    assert!(matches!(
        subscription.events.try_recv(),
        Ok(AttachEvent::Resize { cols: 80, rows: 24 })
    ));
    assert!(
        matches!(subscription.events.try_recv(), Ok(AttachEvent::Output(bytes)) if bytes == b"after")
    );
    assert!(terminal.resize_or_confirm_attach(100, 30));
    assert!(matches!(
        subscription.events.try_recv(),
        Ok(AttachEvent::Resize {
            cols: 100,
            rows: 30
        })
    ));
    assert!(matches!(
        subscription.events.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

/// mirror 는 snapshot 과 이후 출력으로 보이는 화면을 맞춘다. 출력 링·scrollback 은 원본과 다르다
/// — attach 이전 원문이 없고 합성 snapshot 바이트가 앞에 있다. mirror 읽기를 서버 원본으로 보내는 근거다.
#[test]
fn a_mirror_matches_the_visible_screen_but_not_the_output_ring_or_scrollback() {
    let mut source = Terminal::new_detached(20, 3);
    source.feed_bytes(b"old0\r\nold1\r\nold2\r\nold3\r\nold4");
    let mut subscription = source.snapshot_and_stream();
    source.feed_bytes(b"\r\nnew");
    let mut mirror = Terminal::new_detached(subscription.cols, subscription.rows);
    mirror.feed_bytes(&subscription.snapshot);
    while let Ok(AttachEvent::Output(bytes)) = subscription.events.try_recv() {
        mirror.feed_bytes(&bytes);
    }
    assert_eq!(source.screen_text(true), mirror.screen_text(true));
    assert_ne!(
        source.screen_text_lines(6, true),
        mirror.screen_text_lines(6, true)
    );
    let whole = |t: &mut Terminal| {
        t.read_output(&crate::OutputReadRequest {
            from: crate::OutputCursor::At(0),
            max_bytes: crate::OUTPUT_RETENTION_MAX_BYTES,
            strip_ansi: false,
            expect_stream: None,
        })
        .expect("readable")
    };
    let (src, mir) = (whole(&mut source), whole(&mut mirror));
    assert!(src.text.starts_with("old0"));
    assert!(
        mir.text.starts_with("\x1b[0m\x1b[2J"),
        "mirror ring starts with the synthetic snapshot"
    );
    assert_ne!(src.stream, mir.stream);
}

/// Given 같은 터미널, When 바이트가 attach 스트림에 넘어가기 전·넘어가는 동안·수신자가 닫힌 뒤
/// 사건을 만든다, Then 그 바이트를 받은 스트림이 있을 때만 사건에 attach 표지가 붙는다.
#[test]
fn only_events_from_bytes_an_effects_owner_stream_carried_are_marked() {
    let bell = |terminal: &mut Terminal| {
        terminal.feed_bytes(b"\x07");
        let events = terminal.try_take_events().expect("unlocked");
        assert_eq!(events.len(), 1);
        events[0].attach_streamed
    };
    let mut terminal = Terminal::new_detached(80, 24);
    assert!(!bell(&mut terminal), "구독 전");
    let subscription = terminal.snapshot_and_stream();
    assert!(!bell(&mut terminal), "효과를 맡지 않은 구독");
    subscription.events.set_effects_owner(true);
    assert!(terminal.has_effects_owner_stream());
    assert!(bell(&mut terminal), "효과를 맡은 구독");
    // 큐가 넘쳐 바이트를 잃어도 처리권은 subscriber에 남는다.
    let filler = vec![b'x'; ATTACH_STREAM_MAX_BYTES];
    terminal.feed_bytes(&filler);
    assert!(bell(&mut terminal), "넘친 큐");
    subscription.events.set_effects_owner(false);
    assert!(!bell(&mut terminal), "처리권을 거둔 뒤");
    subscription.events.set_effects_owner(true);
    drop(subscription);
    assert!(!bell(&mut terminal), "수신자가 닫힌 뒤");
    assert!(!terminal.has_effects_owner_stream());
}

/// Given 효과를 맡은 구독이 OSC 52 쓰기의 앞부분만 받았다, When 수신자가 닫힌 뒤 끝 바이트(BEL 또는 다음
/// 시퀀스를 시작하는 ESC)와 새 쓰기가 한 청크로 온다, Then 앞부분을 넘긴 쓰기는 client 몫(표지)으로
/// 남고 그 뒤의 새 쓰기만 서버 몫이다.
#[test]
fn a_sequence_begun_for_an_effects_owner_stays_its_effect_after_the_receiver_closes() {
    let marks = |terminal: &mut Terminal| -> Vec<bool> {
        terminal
            .try_take_events()
            .expect("unlocked")
            .iter()
            .filter(|event| matches!(event.kind, crate::TerminalEventKind::ClipboardSet(_)))
            .map(|event| event.attach_streamed)
            .collect()
    };
    let mut terminal = Terminal::new_detached(80, 24);
    let subscription = terminal.snapshot_and_stream();
    subscription.events.set_effects_owner(true);
    terminal.feed_bytes(b"\x1b]52;c;aGk");
    assert!(marks(&mut terminal).is_empty());
    drop(subscription);
    terminal.feed_bytes(b"=\x07\x1b]52;c;Ymll\x07");
    assert_eq!(marks(&mut terminal), vec![true, false]);
    terminal.feed_bytes(b"\x1b]52;c;Ymll\x07");
    assert_eq!(marks(&mut terminal), vec![false], "끝난 뒤에는 서버 몫이다");

    // 앞부분을 넘긴 쓰기를 ESC가 끝내며 새 쓰기를 시작하면 새 쓰기는 서버 몫이다.
    let subscription = terminal.snapshot_and_stream();
    subscription.events.set_effects_owner(true);
    terminal.feed_bytes(b"\x1b]52;c;aGk");
    assert!(marks(&mut terminal).is_empty());
    drop(subscription);
    terminal.feed_bytes(b"=\x1b]52;c;Ymll\x07");
    assert_eq!(marks(&mut terminal), vec![true, false], "ESC 경계");
}

/// Given 끝나지 않은 OSC 52 쓰기, When mirror 스트림이 끊긴다, Then 다음 바이트의 ESC가 옛 시퀀스를
/// 끝내 클립보드 쓰기를 만들지 않는다. 끊기지 않았으면 같은 바이트가 쓰기를 끝낸다.
#[test]
fn a_broken_stream_drops_the_unfinished_sequence() {
    let writes = |broken: bool| {
        let mut terminal = Terminal::new_detached(80, 24);
        terminal.feed_bytes(b"\x1b]52;c;aGk=");
        if broken {
            terminal.break_stream();
        }
        terminal.feed_bytes(b"\x1b\\");
        terminal
            .try_take_events()
            .expect("unlocked")
            .iter()
            .filter(|event| matches!(event.kind, crate::TerminalEventKind::ClipboardSet(_)))
            .count()
    };
    assert_eq!(writes(false), 1);
    assert_eq!(writes(true), 0);
}

/// Given 효과를 맡은 구독이 CSI·SOS·PM·APC·DCS 시퀀스의 앞부분만 받았다, When 수신자가 닫힌 뒤 ESC가
/// 그 시퀀스를 끝내며 OSC 52 쓰기를 시작한다, Then 새 쓰기는 서버 몫(표지 없음)이다. CSI·DCS 머리·SOS·PM은
/// ESC가 dispatch 없이 취소하고, APC·DCS 본문은 그 ESC에서 ApcEnd·Unhook 콜백으로 끝난다.
#[test]
fn an_owner_sequence_cancelled_by_esc_leaves_the_next_sequence_to_the_server() {
    let cases: [(&str, &[u8]); 7] = [
        ("CSI", b"\x1b[1;"),
        ("CSI 중간 바이트", b"\x1b[1 "),
        ("SOS", b"\x1bXabc"),
        ("PM", b"\x1b^abc"),
        ("APC", b"\x1b_abc"),
        ("DCS 머리", b"\x1bP1;"),
        ("DCS 본문", b"\x1bPqabc"),
    ];
    let marks = |head: &[u8]| -> Vec<bool> {
        let mut terminal = Terminal::new_detached(80, 24);
        let subscription = terminal.snapshot_and_stream();
        subscription.events.set_effects_owner(true);
        terminal.feed_bytes(head);
        drop(subscription);
        terminal.feed_bytes(b"\x1b]52;c;Ymll\x07");
        terminal
            .try_take_events()
            .expect("unlocked")
            .iter()
            .filter(|event| matches!(event.kind, crate::TerminalEventKind::ClipboardSet(_)))
            .map(|event| event.attach_streamed)
            .collect()
    };
    let wrong: Vec<(&str, Vec<bool>)> = cases
        .into_iter()
        .map(|(kind, head)| (kind, marks(head)))
        .filter(|(_, marks)| *marks != [false])
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// attach 표지를 함께 보는 사용자 쪽 효과 사건.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    ClipboardSet,
    ClipboardQuery,
    Title,
}

/// 효과를 맡은 구독에 `head`를 넘기고 수신자를 닫은 뒤 `tail`을 넣는다. OSC 52 쓰기·읽기와 제목 사건을
/// 종류와 attach 표지로 순서대로 돌려준다.
fn effect_marks_after_owner_leaves(head: &[u8], tail: &[u8]) -> Vec<(Effect, bool)> {
    let mut terminal = Terminal::new_detached(80, 24);
    let subscription = terminal.snapshot_and_stream();
    subscription.events.set_effects_owner(true);
    terminal.feed_bytes(head);
    drop(subscription);
    terminal.feed_bytes(tail);
    terminal
        .try_take_events()
        .expect("unlocked")
        .iter()
        .filter_map(|event| {
            let effect = match event.kind {
                crate::TerminalEventKind::ClipboardSet(_) => Effect::ClipboardSet,
                crate::TerminalEventKind::ClipboardQuery => Effect::ClipboardQuery,
                crate::TerminalEventKind::TitleChanged(_) => Effect::Title,
                _ => return None,
            };
            Some((effect, event.attach_streamed))
        })
        .collect()
}

/// Given 효과를 맡은 구독이 8비트 C1 도입자(또는 UTF-8로 인코딩한 C1 도입자)로 연 CSI·DCS·SOS·PM·APC의
/// 앞부분만 받았다, When 수신자가 닫힌 뒤 ESC가 그 시퀀스를 취소하며 OSC 52 쓰기나 읽기를 시작한다,
/// Then 새 OSC 52는 서버 몫이다.
#[test]
fn an_owner_sequence_opened_by_a_c1_introducer_ends_at_the_esc_that_cancels_it() {
    let heads: [(&str, &[u8]); 10] = [
        ("CSI", b"\x9b1;"),
        ("DCS", b"\x901;"),
        ("SOS", b"\x98abc"),
        ("PM", b"\x9eabc"),
        ("APC", b"\x9fabc"),
        ("UTF-8 CSI", b"\xc2\x9b1;"),
        ("UTF-8 DCS", b"\xc2\x901;"),
        ("UTF-8 SOS", b"\xc2\x98abc"),
        ("UTF-8 PM", b"\xc2\x9eabc"),
        ("UTF-8 APC", b"\xc2\x9fabc"),
    ];
    let tails: [(&[u8], Effect); 2] = [
        (b"\x1b]52;c;Ymll\x07", Effect::ClipboardSet),
        (b"\x1b]52;c;?\x07", Effect::ClipboardQuery),
    ];
    let mut wrong = Vec::new();
    for (kind, head) in heads {
        for (tail, effect) in tails {
            let marks = effect_marks_after_owner_leaves(head, tail);
            if marks != [(effect, false)] {
                wrong.push((kind, effect, marks));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// Given ESC와 7비트 도입자 사이에 NUL·DEL이 있는 CSI(vtparse는 그동안 Escape에 머문다), When 소유자가
/// 그 앞부분만 받고 떠난 뒤 ESC가 CSI를 취소하며 OSC 52 쓰기를 시작한다, Then 새 쓰기는 서버 몫이다.
/// 소유자가 첫 ESC만 받고 떠나 NUL·DEL과 도입자가 뒤에 와도 같다.
#[test]
fn a_control_byte_between_esc_and_the_introducer_keeps_the_cancel_boundary() {
    let cases: [(&str, &[u8], &[u8]); 4] = [
        ("NUL", b"\x1b\x00[1;", b"\x1b]52;c;Ymll\x07"),
        ("DEL", b"\x1b\x7f[1;", b"\x1b]52;c;Ymll\x07"),
        ("ESC 뒤 NUL", b"\x1b", b"\x00[1;\x1b]52;c;Ymll\x07"),
        ("ESC 뒤 DEL", b"\x1b", b"\x7f[1;\x1b]52;c;Ymll\x07"),
    ];
    let wrong: Vec<_> = cases
        .into_iter()
        .map(|(kind, head, tail)| (kind, effect_marks_after_owner_leaves(head, tail)))
        .filter(|(_, marks)| *marks != [(Effect::ClipboardSet, false)])
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// Given 소유자가 C1 OSC로 CSI를 끊고 연 제목 OSC를 불완전한 UTF-8에서 받고 떠났다, When ESC와 BEL이 온다,
/// Then vtparse가 ESC를 대체 문자로 읽어 OSC가 BEL에서 끝나므로 제목은 소유자 몫이다.
#[test]
fn an_esc_read_as_part_of_a_utf8_character_does_not_end_the_owner_osc() {
    assert_eq!(
        effect_marks_after_owner_leaves(b"\x1b[1;\x9d2;title\xc2", b"\x1b\x07"),
        vec![(Effect::Title, true)]
    );
}
