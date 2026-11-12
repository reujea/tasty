//! Selection orders without a compositor: which offers are kept, read and destroyed, what reaches
//! a paste target and what a receiver of our copy is written.
use super::*;
use std::{cell::RefCell, collections::VecDeque, io, rc::Rc};

enum Step {
    Data(&'static [u8]),
    Block,
    Eof,
    Fail,
}

/// Read end whose next results the test appends after handing it over.
#[derive(Clone, Default)]
struct Pipe(Rc<RefCell<VecDeque<Step>>>);
impl Pipe {
    fn push(&self, step: Step) {
        self.0.borrow_mut().push_back(step);
    }
}
impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.borrow_mut().pop_front() {
            Some(Step::Data(bytes)) => {
                buf[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
            Some(Step::Eof) => Ok(0),
            Some(Step::Fail) => Err(io::Error::other("broken pipe")),
            Some(Step::Block) | None => Err(io::ErrorKind::WouldBlock.into()),
        }
    }
}

/// Write end that takes at most `room` bytes per pump, then blocks.
#[derive(Clone, Default)]
struct Sink {
    taken: Rc<RefCell<Vec<u8>>>,
    room: Rc<RefCell<usize>>,
}
impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut room = self.room.borrow_mut();
        if *room == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = buf.len().min(*room);
        *room -= n;
        self.taken.borrow_mut().extend_from_slice(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const A: Target = Target { client: 1, tag: 7 };
const B: Target = Target { client: 2, tag: 9 };

fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|m| (*m).to_string()).collect()
}

fn offered(selection: &mut Selection, offer: u32, list: &[&str]) {
    selection.offered(Some((offer, mimes(list))));
}

#[test]
fn only_the_latest_selection_offer_is_kept() {
    let mut s = Selection::default();
    offered(&mut s, 3, &["text/plain"]);
    assert!(s.take_commands().is_empty());
    offered(&mut s, 4, &["text/plain"]);
    assert_eq!(s.take_commands(), [Command::Destroy { offer: 3 }]);
    // The same offer announced again is not destroyed under itself.
    offered(&mut s, 4, &["text/plain"]);
    assert!(s.take_commands().is_empty());
    s.offered(None);
    assert_eq!(s.take_commands(), [Command::Destroy { offer: 4 }]);
    assert_eq!(s.request(A, Want::Text, Instant::now()), Paste::Empty);
}

#[test]
fn a_paste_reads_the_preferred_text_mime() {
    assert_eq!(
        text_mime(&mimes(&[
            "text/plain",
            "UTF8_STRING",
            "text/plain;charset=utf-8"
        ])),
        Some("text/plain;charset=utf-8")
    );
    assert_eq!(
        text_mime(&mimes(&["text/plain", "UTF8_STRING"])),
        Some("UTF8_STRING")
    );
    assert_eq!(text_mime(&mimes(&["text/plain"])), Some("text/plain"));
    assert_eq!(text_mime(&mimes(&["image/png", "text/html"])), None);

    let mut s = Selection::default();
    offered(&mut s, 3, &["image/png"]);
    assert_eq!(s.request(A, Want::Text, Instant::now()), Paste::Empty);
    assert!(s.take_commands().is_empty());
    offered(&mut s, 5, &["text/plain", "UTF8_STRING"]);
    s.take_commands();
    assert_eq!(s.request(A, Want::Text, Instant::now()), Paste::Pending);
    assert_eq!(
        s.take_commands(),
        [Command::Receive {
            offer: 5,
            mime: "UTF8_STRING"
        }]
    );
}

#[test]
fn a_read_paste_reaches_only_the_handle_that_asked() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain;charset=utf-8"]);
    assert_eq!(s.request(A, Want::Text, now), Paste::Pending);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Data("한글\r\nline".as_bytes()));
    pipe.push(Step::Block);
    s.pump(now);
    assert!(s.busy());
    assert!(s.take_results(A.client).is_empty());
    pipe.push(Step::Data(b" end"));
    pipe.push(Step::Eof);
    s.pump(now);
    assert!(!s.busy());
    assert!(s.take_results(B.client).is_empty());
    assert_eq!(
        s.take_results(A.client),
        [(A.tag, "한글\nline end".to_string())]
    );
    assert!(s.take_results(A.client).is_empty());
}

#[test]
fn a_paste_that_does_not_arrive_in_time_is_dropped() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    s.request(A, Want::Text, now);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Data(b"partial"));
    s.pump(now + Duration::from_secs(1));
    assert!(s.busy());
    s.pump(now + READ_LIMIT);
    assert!(!s.busy());
    // Data that comes after the limit is never delivered.
    pipe.push(Step::Data(b" rest"));
    pipe.push(Step::Eof);
    s.pump(now + READ_LIMIT);
    assert!(s.take_results(A.client).is_empty());
}

#[test]
fn text_already_waiting_when_the_limit_passes_is_not_delivered() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    s.request(A, Want::Text, now);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    // The source wrote everything and closed, but the first pump after that is past the limit.
    pipe.push(Step::Data(b"late"));
    pipe.push(Step::Eof);
    s.pump(now + READ_LIMIT);
    assert!(!s.busy());
    assert!(s.take_results(A.client).is_empty());
}

#[test]
fn a_receive_that_never_got_a_pipe_also_times_out() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    s.request(A, Want::Text, now);
    s.pump(now + READ_LIMIT);
    assert!(!s.busy());
}

#[test]
fn late_results_are_dropped_when_the_target_changed() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    // A newer request replaces the waiting read: the first pipe's text is never delivered.
    s.request(A, Want::Text, now);
    let first = Pipe::default();
    s.attach(5, Ok(Box::new(first.clone())));
    s.request(B, Want::Text, now);
    first.push(Step::Data(b"old"));
    first.push(Step::Eof);
    s.pump(now);
    assert!(s.take_results(A.client).is_empty());
    let second = Pipe::default();
    s.attach(5, Ok(Box::new(second.clone())));
    second.push(Step::Data(b"new"));
    second.push(Step::Eof);
    s.pump(now);
    assert_eq!(s.take_results(B.client), [(B.tag, "new".to_string())]);

    // A selection change while reading drops the read.
    s.request(A, Want::Text, now);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    offered(&mut s, 6, &["text/plain"]);
    pipe.push(Step::Data(b"stale"));
    pipe.push(Step::Eof);
    s.pump(now);
    assert!(s.take_results(A.client).is_empty());
    assert!(!s.busy());

    // A cancelled read and a forgotten handle deliver nothing.
    s.request(A, Want::Text, now);
    s.cancel(B.client);
    assert!(s.busy());
    s.cancel(A.client);
    assert!(!s.busy());
    s.request(B, Want::Text, now);
    s.forget(B.client);
    assert!(!s.busy());
}

#[test]
fn a_failed_or_oversized_read_is_dropped() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    s.request(A, Want::Text, now);
    s.attach(5, Err(io::Error::other("no pipe")));
    assert!(!s.busy());

    s.request(A, Want::Text, now);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Fail);
    s.pump(now);
    assert!(!s.busy());

    // An empty selection pastes nothing.
    s.request(A, Want::Text, now);
    let pipe = Pipe::default();
    s.attach(5, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Eof);
    s.pump(now);
    assert!(s.take_results(A.client).is_empty());

    struct Endless;
    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(b'a');
            Ok(buf.len())
        }
    }
    s.request(A, Want::Text, now);
    s.attach(5, Ok(Box::new(Endless)));
    s.pump(now);
    assert!(!s.busy());
    assert!(s.take_results(A.client).is_empty());
}

#[test]
fn our_own_copy_pastes_at_once_until_another_client_takes_the_selection() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);
    assert_eq!(s.own(40, Arc::from("mine\r\n")), None);
    assert_eq!(
        s.request(A, Want::Text, now),
        Paste::Ready("mine\n".to_string())
    );
    assert!(s.take_commands().is_empty());
    assert_eq!(s.own(41, Arc::from("newer")), Some(40));
    assert!(!s.cancelled(40));
    assert_eq!(
        s.request(A, Want::Text, now),
        Paste::Ready("newer".to_string())
    );
    assert!(s.cancelled(41));
    assert_eq!(s.request(A, Want::Text, now), Paste::Pending);
}

#[test]
fn a_receiver_of_our_copy_gets_the_text_for_each_text_mime() {
    let now = Instant::now();
    for mime in TEXT_MIMES {
        let mut s = Selection::default();
        s.own(40, Arc::from("copied 한글"));
        let sink = Sink::default();
        *sink.room.borrow_mut() = 4;
        s.send(40, mime, Box::new(sink.clone()), now);
        s.pump(now);
        assert_eq!(*sink.taken.borrow(), b"copi");
        assert!(s.busy());
        *sink.room.borrow_mut() = usize::MAX;
        s.pump(now);
        assert_eq!(*sink.taken.borrow(), "copied 한글".as_bytes());
        assert!(!s.busy(), "{mime}");
    }
}

#[test]
fn unknown_mimes_stale_sources_and_slow_receivers_get_nothing_more() {
    let now = Instant::now();
    let mut s = Selection::default();
    s.own(40, Arc::from("text"));
    let sink = Sink::default();
    *sink.room.borrow_mut() = usize::MAX;
    s.send(40, "image/png", Box::new(sink.clone()), now);
    s.send(39, "text/plain", Box::new(sink.clone()), now);
    s.pump(now);
    assert!(sink.taken.borrow().is_empty());
    assert!(!s.busy());

    // A transfer already started keeps its text when the source is replaced.
    let slow = Sink::default();
    *slow.room.borrow_mut() = 2;
    s.send(40, "text/plain", Box::new(slow.clone()), now);
    s.pump(now);
    s.own(41, Arc::from("other"));
    *slow.room.borrow_mut() = usize::MAX;
    s.pump(now);
    assert_eq!(*slow.taken.borrow(), b"text");

    let stuck = Sink::default();
    s.send(41, "text/plain", Box::new(stuck.clone()), now);
    s.pump(now + Duration::from_secs(29));
    assert!(s.busy());
    s.pump(now + WRITE_LIMIT);
    assert!(!s.busy());

    for _ in 0..MAX_SENDS + 2 {
        s.send(41, "text/plain", Box::new(Sink::default()), now);
    }
    assert_eq!(s.sending.len(), MAX_SENDS);
}

#[test]
fn a_terminal_paste_takes_a_png_only_when_the_offer_has_no_text() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 3, &["image/png", "text/plain"]);
    s.take_commands();
    assert_eq!(s.request(A, Want::TextOrPng, now), Paste::Pending);
    assert_eq!(
        s.take_commands(),
        [Command::Receive {
            offer: 3,
            mime: "text/plain"
        }]
    );
    offered(&mut s, 4, &["image/png"]);
    s.take_commands();
    // The text read was cut short by the new selection.
    assert!(matches!(
        s.take_received(A.client).as_slice(),
        [(7, Received::Failed(_))]
    ));
    // egui text fields never read an image.
    assert_eq!(s.request(A, Want::Text, now), Paste::Empty);
    assert_eq!(s.request(A, Want::TextOrPng, now), Paste::Pending);
    assert_eq!(
        s.take_commands(),
        [Command::Receive {
            offer: 4,
            mime: "image/png"
        }]
    );
    let pipe = Pipe::default();
    s.attach(4, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Data(b"\x89PNG\r\n"));
    pipe.push(Step::Eof);
    s.pump(now);
    // The image bytes are kept as sent: no text conversion.
    assert_eq!(
        s.take_received(A.client),
        [(A.tag, Received::Png(b"\x89PNG\r\n".to_vec()))]
    );
}

#[test]
fn a_failed_read_reports_to_its_target_but_a_replaced_one_does_not() {
    let now = Instant::now();
    let mut s = Selection::default();
    offered(&mut s, 5, &["text/plain"]);

    // Timeout.
    s.request(A, Want::TextOrPng, now);
    assert!(s.waiting(A.client));
    assert!(!s.waiting(B.client));
    s.pump(now + READ_LIMIT);
    assert!(!s.waiting(A.client));
    assert!(matches!(
        s.take_received(A.client).as_slice(),
        [(7, Received::Failed(_))]
    ));

    // Pipe error and a selection change while reading.
    s.request(A, Want::TextOrPng, now);
    s.attach(5, Err(io::Error::other("no pipe")));
    s.request(A, Want::TextOrPng, now);
    offered(&mut s, 6, &["text/plain"]);
    let received = s.take_received(A.client);
    assert_eq!(received.len(), 2);
    assert!(
        received
            .iter()
            .all(|(_, r)| matches!(r, Received::Failed(_)))
    );

    // A newer request from another handle replaces the read silently.
    s.request(A, Want::TextOrPng, now);
    s.request(B, Want::Text, now);
    assert!(!s.waiting(A.client));
    assert!(s.take_received(A.client).is_empty());

    // An empty selection is a finished read; text fields still paste nothing.
    s.take_commands();
    s.request(A, Want::TextOrPng, now);
    let pipe = Pipe::default();
    s.attach(6, Ok(Box::new(pipe.clone())));
    pipe.push(Step::Eof);
    s.pump(now);
    assert_eq!(
        s.take_received(A.client),
        [(A.tag, Received::Text(String::new()))]
    );
}
