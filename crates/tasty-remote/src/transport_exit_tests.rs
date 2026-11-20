//! 전송 출구마다 누가 끝을 확정하고 언제 깨우는지 잰다.
//!
//! 깨움 콜백은 불리는 순간의 `settled()`를 기록한다. None은 사유만 올린 조기 깨움(큐 거절)이고,
//! Some은 확정 뒤의 종료 깨움이다. 검사는 종료 깨움만 본다. 순서는 시험 이음새(`WriterSeams`,
//! `ReaderGate`)로 만들고 벽시계 경과에 기대지 않는다.

use super::*;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

const WAIT: Duration = Duration::from_secs(10);

/// 깨울 때마다 그 순간의 확정 사유를 기록하고 알린다.
struct Wakes {
    seen: Arc<Mutex<Vec<Option<EndReason>>>>,
    terminal: Receiver<EndReason>,
}

fn recording_wake(end: &Arc<TransportEnd>) -> (Arc<dyn Fn() + Send + Sync>, Wakes) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (tx, terminal) = channel();
    let wake: Arc<dyn Fn() + Send + Sync> = {
        let (end, seen) = (end.clone(), seen.clone());
        let tx = Mutex::new(tx);
        Arc::new(move || {
            let settled = end.settled();
            seen.lock().unwrap().push(settled);
            if let Some(reason) = settled
                && tx.lock().unwrap().send(reason).is_err()
            {
                tracing::debug!("the test stopped listening for wakes");
            }
        })
    };
    (wake, Wakes { seen, terminal })
}

impl Wakes {
    fn terminal(&self) -> Vec<EndReason> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .copied()
            .collect()
    }
}

/// 쓰기 스레드 이음새: 기다림 진입 신호와 상한 문.
fn seams() -> (WriterSeams, Receiver<()>, Arc<AtomicBool>) {
    let (enter_tx, enter_rx) = channel();
    let gate = Arc::new(AtomicBool::new(false));
    (
        WriterSeams {
            on_wait_enter: Some(enter_tx),
            cap_gate: Some(gate.clone()),
        },
        enter_rx,
        gate,
    )
}

/// reader 이음새: 프레임을 받으면 도착을 알리고 놓아줄 때까지 멈춘다.
fn reader_gate() -> (ReaderGate, Receiver<()>, Sender<()>) {
    let (arrived_tx, arrived_rx) = channel();
    let (release_tx, release_rx) = channel();
    (
        ReaderGate {
            arrived: arrived_tx,
            release: release_rx,
        },
        arrived_rx,
        release_tx,
    )
}

fn gated_reader_link(
    end: &Arc<TransportEnd>,
    finished: &Arc<AtomicBool>,
    gate: Option<ReaderGate>,
) -> AttachReaderEnd {
    AttachReaderEnd {
        gate,
        ..reader_link(end, finished)
    }
}

fn seamed_writer_link(
    end: &Arc<TransportEnd>,
    finished: &Arc<AtomicBool>,
    seams: WriterSeams,
) -> AttachWriterEnd {
    AttachWriterEnd {
        seams,
        ..writer_link(end, finished, Duration::ZERO)
    }
}

/// 큐 거절(Q): 바이트 상한을 넘는 프레임 하나로 `ConnectionSender::fail`을 일으킨다. 사유만 올리고
/// 깨운다.
fn reject_the_queue(sender: &crate::connection::ConnectionSender) {
    let oversized = OutFrame {
        tag: StreamTag::Data,
        payload: Vec::with_capacity(tasty_ipc::admission::QUEUED_BYTES_LIMIT + 1),
    };
    assert!(sender.send(oversized).is_err(), "the queue rejects it");
}

const FORCE_NOTICE: &[u8] = br#"{"event":"force_detached","reason":"force_detach_workspace"}"#;

fn wait_finished(handle: &std::thread::JoinHandle<()>, what: &str) {
    let until = std::time::Instant::now() + WAIT;
    while !handle.is_finished() {
        assert!(std::time::Instant::now() < until, "{what} did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// S2: reader 출구마다 확정 뒤에 깨운다. 깨움은 확정한 쪽 하나다.
#[test]
fn every_reader_exit_settles_before_it_wakes() {
    enum Exit {
        Notice,
        Eof,
        Detach,
        Panic,
    }
    for (exit, expected) in [
        (Exit::Notice, EndReason::Forced),
        (Exit::Eof, EndReason::Lost),
        (Exit::Detach, EndReason::Lost),
        (Exit::Panic, EndReason::Lost),
    ] {
        let (_write_half, conn, mut peer) = opened_attach_pair();
        let end = Arc::new(TransportEnd::new());
        let finished = Arc::new(AtomicBool::new(false));
        let (wake, wakes) = recording_wake(&end);
        let (sender, _rx) = crate::connection::channel();
        let decode: fn(&[u8]) -> Option<MirrorEvent> = match exit {
            Exit::Panic => panicking_decode,
            _ => |_| None,
        };
        match exit {
            Exit::Notice => stream::write_frame(&mut peer, StreamTag::Control, FORCE_NOTICE),
            Exit::Eof => peer.shutdown(std::net::Shutdown::Write),
            Exit::Detach => stream::write_frame(&mut peer, StreamTag::Detach, b""),
            Exit::Panic => stream::write_frame(&mut peer, StreamTag::Control, b"{}"),
        }
        .unwrap();
        let reader = spawn_attach_reader_thread(
            conn,
            MirrorOutbox::new(sender.epoch()),
            sender.clone(),
            wake,
            decode,
            reader_link(&end, &finished),
        );
        let joined = reader.join();
        assert_eq!(joined.is_err(), matches!(exit, Exit::Panic));
        assert!(finished.load(Ordering::SeqCst));
        assert_eq!(wakes.terminal(), vec![expected]);
        assert!(
            wakes.seen.lock().unwrap().iter().all(Option::is_some),
            "no wake before the settle"
        );
    }
}

/// S3a: 큐 거절로 쓰기 스레드가 reader를 기다리는 동안 reader가 강제 통지를 처리하면, reader가
/// `Forced`로 확정하고 한 번 깨운다. 상한 문을 연 뒤에도 쓰기 스레드는 확정하지 않고 깨우지 않는다.
#[test]
fn the_reader_settles_a_notice_while_the_writer_waits_after_a_queue_rejection() {
    let (write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (wake, wakes) = recording_wake(&end);
    let (sender, rx) = crate::connection::channel();
    sender.bind_failure(end.clone(), wake.clone());
    let (seams, entered, cap_gate) = seams();
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        seamed_writer_link(&end, &finished, seams),
        wake.clone(),
        " S3a",
    );
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        wake,
        |_| None,
        reader_link(&end, &finished),
    );

    reject_the_queue(&sender);
    entered.recv_timeout(WAIT).expect("the writer waits");
    assert_eq!(end.settled(), None, "the rejection does not settle");
    stream::write_frame(&mut peer, StreamTag::Control, FORCE_NOTICE).unwrap();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(
        wakes.terminal.recv_timeout(WAIT).unwrap(),
        EndReason::Forced
    );
    reader.join().unwrap();

    cap_gate.store(true, Ordering::SeqCst);
    writer.join().unwrap();
    assert_eq!(wakes.terminal(), vec![EndReason::Forced]);
    assert!(
        wakes.seen.lock().unwrap().first() == Some(&None),
        "the rejection woke early without a settle"
    );
    drop(sender);
}

/// S3b: reader가 강제 통지를 받고 판별 전에 멈춘 사이 쓰기 스레드가 상한에 닿으면, 쓰기 스레드가
/// `Lost`로 확정하고 소켓을 닫고 깨운다. 그 뒤 reader가 통지로 사유를 올려도 확정된 사유는 그대로다.
#[test]
fn the_writer_settles_at_the_cap_and_a_later_notice_does_not_change_it() {
    let (write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (wake, wakes) = recording_wake(&end);
    let (sender, rx) = crate::connection::channel();
    sender.bind_failure(end.clone(), wake.clone());
    let (seams, entered, cap_gate) = seams();
    let (gate, arrived, release) = reader_gate();
    stream::write_frame(&mut peer, StreamTag::Control, FORCE_NOTICE).unwrap();
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        wake.clone(),
        |_| None,
        gated_reader_link(&end, &finished, Some(gate)),
    );
    arrived
        .recv_timeout(WAIT)
        .expect("the reader holds the notice");
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        seamed_writer_link(&end, &finished, seams),
        wake,
        " S3b",
    );

    reject_the_queue(&sender);
    entered.recv_timeout(WAIT).expect("the writer waits");
    cap_gate.store(true, Ordering::SeqCst);
    assert_eq!(wakes.terminal.recv_timeout(WAIT).unwrap(), EndReason::Lost);
    writer.join().unwrap();

    drop(release);
    reader.join().unwrap();
    assert_eq!(end.settled(), Some(EndReason::Lost));
    assert_eq!(wakes.terminal(), vec![EndReason::Lost]);
    drop(sender);
}

/// 쓰기 스레드의 출구를 일으키는 방법.
enum WriterExit {
    /// 큐 거절.
    Rejected,
    /// 사유가 오른 뒤 큐가 계속 차 있다.
    FullQueue,
    /// 쓰기 실패(너무 큰 프레임).
    WriteFailed,
    /// Detach를 썼다.
    Detach,
    /// 모든 sender가 사라졌다.
    SendersGone,
    /// 쓰기 스레드 panic.
    Panic,
}

/// S4: 쓰기 스레드의 출구마다 reader가 멈춘 상태에서 쓰기 스레드가 `Lost`로 확정하고 한 번 깨운다.
/// 그 뒤 reader가 끝나도 더 깨우지 않는다.
#[test]
fn every_writer_exit_settles_and_wakes_once_while_the_reader_is_held() {
    for exit in [
        WriterExit::Rejected,
        WriterExit::FullQueue,
        WriterExit::WriteFailed,
        WriterExit::Detach,
        WriterExit::SendersGone,
        WriterExit::Panic,
    ] {
        let (write_half, conn, mut peer) = opened_attach_pair();
        let end = Arc::new(TransportEnd::new());
        let finished = Arc::new(AtomicBool::new(false));
        let (wake, wakes) = recording_wake(&end);
        let (sender, rx) = crate::connection::channel();
        sender.bind_failure(end.clone(), wake.clone());
        let (gate, arrived, release) = reader_gate();
        // 깨우지 않는 Ping으로 reader를 붙잡는다. Data는 놓아준 뒤 일반 데이터 깨움을 낸다.
        stream::write_frame(&mut peer, StreamTag::Ping, b"").unwrap();
        // reader는 다른 채널의 sender를 쥔다. 쓰기 채널의 sender는 시험이 쥔 하나뿐이다(W4).
        let (reader_sender, _reader_rx) = crate::connection::channel();
        let reader = spawn_attach_reader_thread(
            conn,
            MirrorOutbox::new(sender.epoch()),
            reader_sender,
            wake.clone(),
            |_| None,
            gated_reader_link(&end, &finished, Some(gate)),
        );
        arrived.recv_timeout(WAIT).expect("the reader is held");
        match exit {
            WriterExit::Rejected => reject_the_queue(&sender),
            WriterExit::FullQueue => {
                end.raise(EndReason::Lost);
                for _ in 0..200 {
                    if sender
                        .send(OutFrame {
                            tag: StreamTag::Data,
                            payload: b"x".to_vec(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            WriterExit::WriteFailed => sender
                .send(OutFrame {
                    tag: StreamTag::Control,
                    payload: vec![0; stream::MAX_FRAME_LEN as usize + 1],
                })
                .unwrap(),
            WriterExit::Detach => sender
                .send(OutFrame {
                    tag: StreamTag::Detach,
                    payload: Vec::new(),
                })
                .unwrap(),
            WriterExit::SendersGone => {}
            WriterExit::Panic => {
                let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                sender
                    .send_guarded(
                        OutFrame {
                            tag: StreamTag::Data,
                            payload: b"x".to_vec(),
                        },
                        Arc::new(move || {
                            if calls.fetch_add(1, Ordering::SeqCst) > 0 {
                                panic!("the writer's owner check panics on purpose");
                            }
                            true
                        }),
                    )
                    .unwrap();
            }
        }
        let writer = spawn_attach_write_thread(
            write_half,
            rx,
            writer_link(&end, &finished, Duration::ZERO),
            wake.clone(),
            " S4",
        );
        let panics = matches!(exit, WriterExit::Panic);
        let reader_sender = if matches!(exit, WriterExit::SendersGone) {
            drop(sender);
            None
        } else {
            Some(sender)
        };
        assert_eq!(
            wakes.terminal.recv_timeout(WAIT).unwrap(),
            EndReason::Lost,
            "the writer settles while the reader is held"
        );
        wait_finished(&writer, "the writer");
        assert_eq!(writer.join().is_err(), panics);
        drop(release);
        reader.join().unwrap();
        assert_eq!(end.settled(), Some(EndReason::Lost));
        assert_eq!(wakes.terminal(), vec![EndReason::Lost], "one terminal wake");
        drop(reader_sender);
    }
}

/// U7-a (a): 재동기화의 Detach를 쓴 뒤에도 쓰기 스레드는 reader를 기다린다. 그동안 서버가 옛 소켓에
/// 강제 통지를 쓰면 reader가 `Forced`로 확정한다. Detach 직후 소켓을 닫고 `Lost`로 확정하면 실패한다.
#[test]
fn a_notice_after_the_resync_detach_is_still_settled_as_forced() {
    let (write_half, conn, mut peer) = opened_attach_pair();
    let end = Arc::new(TransportEnd::new());
    let finished = Arc::new(AtomicBool::new(false));
    let (wake, wakes) = recording_wake(&end);
    let (sender, rx) = crate::connection::channel();
    sender.bind_failure(end.clone(), wake.clone());
    let (seams, entered, cap_gate) = seams();
    let writer = spawn_attach_write_thread(
        write_half,
        rx,
        seamed_writer_link(&end, &finished, seams),
        wake.clone(),
        " U7a",
    );
    let reader = spawn_attach_reader_thread(
        conn,
        MirrorOutbox::new(sender.epoch()),
        sender.clone(),
        wake,
        |_| None,
        reader_link(&end, &finished),
    );

    sender
        .send(OutFrame {
            tag: StreamTag::Detach,
            payload: Vec::new(),
        })
        .unwrap();
    entered
        .recv_timeout(WAIT)
        .expect("the writer waits after the Detach");
    let detach = stream::read_frame(&mut peer).unwrap();
    assert_eq!(detach.tag, StreamTag::Detach);
    stream::write_frame(&mut peer, StreamTag::Control, FORCE_NOTICE).unwrap();
    assert_eq!(
        wakes.terminal.recv_timeout(WAIT).unwrap(),
        EndReason::Forced
    );
    reader.join().unwrap();
    cap_gate.store(true, Ordering::SeqCst);
    writer.join().unwrap();
    assert_eq!(end.settled(), Some(EndReason::Forced));
    assert_eq!(wakes.terminal(), vec![EndReason::Forced]);
    drop(sender);
}
