//! Incoming drag orders without a compositor: the commands sent to offers and the window events.
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

const OPEN: Gate = Gate {
    inbound: true,
    known_surface: true,
    outbound_active: false,
};
const LIST: &[u8] = b"file:///a%20b\r\nfile:///c\r\n";

fn paths() -> Vec<PathBuf> {
    vec![PathBuf::from("/a b"), PathBuf::from("/c")]
}

fn offer(inbound: &mut Inbound, id: u32, mimes: &[&str]) {
    inbound.data_offer(id);
    for mime in mimes {
        inbound.offer_mime(id, (*mime).to_string());
    }
}

fn enter(inbound: &mut Inbound, offer: u32, surface: u32, gate: Gate, now: Instant) {
    inbound.enter(
        Enter {
            serial: 9,
            surface,
            at: (10.0, 20.0),
            offer: Some(offer),
        },
        gate,
        now,
    );
}

/// Enters a file list on surface 5 and hands over a pipe that has the whole list.
fn hovering(now: Instant) -> (Inbound, Pipe) {
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &["text/plain", URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, now);
    inbound.take_commands();
    let pipe = Pipe::default();
    pipe.push(Step::Data(LIST));
    pipe.push(Step::Eof);
    inbound.attach(1, Ok(Box::new(pipe.clone())));
    inbound.pump(now);
    (inbound, pipe)
}

fn rejected(offer: u32) -> Vec<Command> {
    vec![
        Command::Accept {
            offer,
            serial: 9,
            uri_list: false,
        },
        Command::SetActions { offer, copy: false },
    ]
}

#[test]
fn a_file_list_from_another_client_is_taken_for_copy() {
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &["text/plain", URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, Instant::now());
    assert_eq!(
        inbound.take_commands(),
        [
            Command::Accept {
                offer: 1,
                serial: 9,
                uri_list: true
            },
            Command::SetActions {
                offer: 1,
                copy: true
            },
            Command::Receive { offer: 1 },
        ]
    );
    assert!(inbound.take_events().is_empty());
}

#[test]
fn other_drags_are_refused_and_without_the_standing_device_left_alone() {
    let now = Instant::now();
    for (mimes, outbound_active, known_surface) in [
        (&[URI_LIST][..], true, true),
        (&["text/plain"][..], false, true),
        (&[URI_LIST][..], false, false),
    ] {
        let mut inbound = Inbound::default();
        offer(&mut inbound, 1, mimes);
        let gate = Gate {
            inbound: true,
            known_surface,
            outbound_active,
        };
        enter(&mut inbound, 1, 5, gate, now);
        assert_eq!(inbound.take_commands(), rejected(1));
    }
    // Outbound-only device: only an active outbound drag answers the offer.
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &[URI_LIST]);
    let quiet = Gate {
        inbound: false,
        ..OPEN
    };
    enter(&mut inbound, 1, 5, quiet, now);
    assert!(inbound.take_commands().is_empty());
    inbound.leave();
    assert_eq!(inbound.take_commands(), [Command::Destroy { offer: 1 }]);
    offer(&mut inbound, 2, &[URI_LIST]);
    let outbound = Gate {
        outbound_active: true,
        ..quiet
    };
    enter(&mut inbound, 2, 5, outbound, now);
    assert_eq!(inbound.take_commands(), rejected(2));
}

#[test]
fn hover_starts_once_the_list_is_read_and_motion_follows() {
    let now = Instant::now();
    let (mut inbound, _) = hovering(now);
    assert_eq!(
        inbound.take_events(),
        [(
            5,
            InboundEvent::Hover {
                paths: paths(),
                at: (10.0, 20.0)
            }
        )]
    );
    inbound.motion((30.5, 40.0));
    assert_eq!(
        inbound.take_events(),
        [(5, InboundEvent::Motion { at: (30.5, 40.0) })]
    );
    inbound.leave();
    assert_eq!(inbound.take_events(), [(5, InboundEvent::Leave)]);
    assert_eq!(inbound.take_commands(), [Command::Destroy { offer: 1 }]);
    assert!(!inbound.busy());
}

#[test]
fn a_drop_before_the_list_is_read_waits_and_keeps_its_position() {
    let now = Instant::now();
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &[URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, now);
    inbound.take_commands();
    let pipe = Pipe::default();
    pipe.push(Step::Data(b"file:///a%20b\r\n"));
    pipe.push(Step::Block);
    inbound.attach(1, Ok(Box::new(pipe.clone())));
    inbound.offer_action(1, Action::Copy);
    inbound.motion((1.0, 2.0));
    inbound.drop_here(now);
    inbound.motion((99.0, 99.0));
    inbound.leave();
    assert!(inbound.take_events().is_empty());
    assert!(inbound.take_commands().is_empty());
    assert!(inbound.busy());
    pipe.push(Step::Data(b"file:///c\r\n"));
    pipe.push(Step::Eof);
    inbound.pump(now);
    assert_eq!(
        inbound.take_events(),
        [(
            5,
            InboundEvent::Drop {
                paths: paths(),
                at: (1.0, 2.0)
            }
        )]
    );
    assert_eq!(
        inbound.take_commands(),
        [Command::Finish { offer: 1 }, Command::Destroy { offer: 1 }]
    );
    assert!(!inbound.busy());
}

#[test]
fn only_a_final_copy_is_finished() {
    for action in [None, Some(Action::None), Some(Action::Other)] {
        let now = Instant::now();
        let (mut inbound, _) = hovering(now);
        inbound.take_events();
        if let Some(action) = action {
            inbound.offer_action(1, action);
        }
        inbound.drop_here(now);
        assert_eq!(inbound.take_events(), [(5, InboundEvent::Leave)]);
        assert_eq!(inbound.take_commands(), [Command::Destroy { offer: 1 }]);
    }
}

#[test]
fn a_failed_read_refuses_the_drag_and_never_finishes() {
    let now = Instant::now();
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &[URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, now);
    inbound.take_commands();
    let pipe = Pipe::default();
    pipe.push(Step::Fail);
    inbound.attach(1, Ok(Box::new(pipe)));
    inbound.pump(now);
    assert_eq!(inbound.take_commands(), rejected(1));
    inbound.offer_action(1, Action::Copy);
    inbound.drop_here(now);
    assert!(inbound.take_events().is_empty());
    assert_eq!(inbound.take_commands(), [Command::Destroy { offer: 1 }]);

    // A list that never ends is dropped at the read limit.
    let mut inbound = Inbound::default();
    offer(&mut inbound, 2, &[URI_LIST]);
    enter(&mut inbound, 2, 5, OPEN, now);
    inbound.take_commands();
    inbound.attach(2, Ok(Box::new(Pipe::default())));
    inbound.pump(now + READ_LIMIT - Duration::from_millis(1));
    assert!(inbound.take_commands().is_empty());
    inbound.pump(now + READ_LIMIT);
    assert_eq!(
        inbound.take_commands(),
        [
            Command::Accept {
                offer: 2,
                serial: 9,
                uri_list: false
            },
            Command::SetActions {
                offer: 2,
                copy: false
            }
        ]
    );
}

#[test]
fn an_oversized_list_is_refused() {
    let now = Instant::now();
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &[URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, now);
    inbound.take_commands();
    let pipe = Pipe::default();
    static CHUNK: [u8; 32 * 1024] = [b'a'; 32 * 1024];
    for _ in 0..=(crate::file::drag::payload::MAX_BYTES / CHUNK.len()) {
        pipe.push(Step::Data(&CHUNK));
    }
    inbound.attach(1, Ok(Box::new(pipe)));
    inbound.pump(now);
    assert_eq!(inbound.take_commands(), rejected(1));
}

#[test]
fn a_selection_during_a_drag_destroys_only_its_own_offer() {
    let now = Instant::now();
    let (mut inbound, _) = hovering(now);
    inbound.take_events();
    offer(&mut inbound, 7, &["text/plain"]);
    inbound.selection(Some(7));
    inbound.selection(Some(1));
    inbound.selection(None);
    assert_eq!(inbound.take_commands(), [Command::Destroy { offer: 7 }]);
    inbound.offer_action(1, Action::Copy);
    inbound.drop_here(now);
    assert_eq!(
        inbound.take_events(),
        [(
            5,
            InboundEvent::Drop {
                paths: paths(),
                at: (10.0, 20.0)
            }
        )]
    );
    assert_eq!(
        inbound.take_commands(),
        [Command::Finish { offer: 1 }, Command::Destroy { offer: 1 }]
    );
}

#[test]
fn a_late_read_after_leave_or_reenter_is_discarded() {
    let now = Instant::now();
    let mut inbound = Inbound::default();
    offer(&mut inbound, 1, &[URI_LIST]);
    enter(&mut inbound, 1, 5, OPEN, now);
    inbound.leave();
    offer(&mut inbound, 2, &[URI_LIST]);
    enter(&mut inbound, 2, 6, OPEN, now);
    inbound.take_commands();
    let stale = Pipe::default();
    stale.push(Step::Data(b"file:///stale\r\n"));
    stale.push(Step::Eof);
    inbound.attach(1, Ok(Box::new(stale)));
    inbound.pump(now);
    assert!(inbound.take_events().is_empty());
    let fresh = Pipe::default();
    fresh.push(Step::Data(LIST));
    fresh.push(Step::Eof);
    inbound.attach(2, Ok(Box::new(fresh)));
    inbound.pump(now);
    // The second window's surface receives the hover, not the first.
    assert_eq!(
        inbound.take_events(),
        [(
            6,
            InboundEvent::Hover {
                paths: paths(),
                at: (10.0, 20.0)
            }
        )]
    );
}

#[test]
fn moving_between_windows_leaves_the_first_and_hovers_the_second() {
    let now = Instant::now();
    let (mut inbound, _) = hovering(now);
    inbound.take_events();
    inbound.leave();
    offer(&mut inbound, 2, &[URI_LIST]);
    enter(&mut inbound, 2, 6, OPEN, now);
    let pipe = Pipe::default();
    pipe.push(Step::Data(LIST));
    pipe.push(Step::Eof);
    inbound.attach(2, Ok(Box::new(pipe)));
    inbound.pump(now);
    assert_eq!(
        inbound.take_events(),
        [
            (5, InboundEvent::Leave),
            (
                6,
                InboundEvent::Hover {
                    paths: paths(),
                    at: (10.0, 20.0)
                }
            )
        ]
    );
}

#[test]
fn takes_and_can_finish_are_the_protocol_conditions() {
    let uri = [URI_LIST.to_string()];
    assert!(takes(OPEN, &uri));
    assert!(!takes(OPEN, &["text/plain".to_string()]));
    assert!(!takes(
        Gate {
            inbound: false,
            ..OPEN
        },
        &uri
    ));
    assert!(can_finish(true, Some(Action::Copy)));
    assert!(!can_finish(false, Some(Action::Copy)));
    assert!(!can_finish(true, None));
    assert!(!can_finish(true, Some(Action::None)));
}
