//! The end of one attach transport: a reason that only rises and a single settle point.
//!
//! Reader, writer and the App releasing the transport may all observe the end. Exactly one
//! of them settles it, through one compare-and-swap. The reason seen by that swap is final:
//! a later raise has no effect. This value never closes a socket; whoever owns the socket
//! closes it.
use std::sync::atomic::{AtomicU8, Ordering};

const SETTLED: u8 = 0x80;
const REASON: u8 = 0x7f;

/// Why a transport ended. Ordered so that a forced detach outranks a plain loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndReason {
    Lost = 1,
    Forced = 2,
}

/// Raised reason plus a settled bit in one atomic byte.
#[derive(Debug, Default)]
pub struct TransportEnd(AtomicU8);

impl TransportEnd {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raises the reason. A lower reason never replaces a higher one, and a settled value
    /// keeps its reason because the settled bit makes it larger than any reason.
    pub fn raise(&self, reason: EndReason) {
        self.0.fetch_max(reason as u8, Ordering::AcqRel);
    }

    /// Settles an unsettled value that has a reason. Returns true for exactly one caller per
    /// transport; an open value or an already settled one returns false.
    pub fn settle(&self) -> bool {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                (v & SETTLED == 0 && v & REASON != 0).then_some(v | SETTLED)
            })
            .is_ok()
    }

    /// The final reason, only once settled.
    pub fn settled(&self) -> Option<EndReason> {
        let v = self.0.load(Ordering::Acquire);
        if v & SETTLED == 0 {
            return None;
        }
        reason_of(v & REASON)
    }

    /// Whether any reason was raised, settled or not. For I/O decisions only.
    pub fn is_ending(&self) -> bool {
        self.0.load(Ordering::Acquire) & REASON != 0
    }
}

fn reason_of(bits: u8) -> Option<EndReason> {
    match bits {
        1 => Some(EndReason::Lost),
        2 => Some(EndReason::Forced),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn an_open_end_does_not_settle() {
        let end = TransportEnd::new();
        assert!(!end.is_ending());
        assert!(!end.settle());
        assert_eq!(end.settled(), None);
    }

    #[test]
    fn a_raised_reason_only_rises_before_settling() {
        let end = TransportEnd::new();
        end.raise(EndReason::Forced);
        end.raise(EndReason::Lost);
        assert!(end.is_ending());
        assert_eq!(end.settled(), None, "unsettled until someone settles");
        assert!(end.settle());
        assert_eq!(end.settled(), Some(EndReason::Forced));
    }

    #[test]
    fn a_lost_end_rises_to_forced_before_settling() {
        let end = TransportEnd::new();
        end.raise(EndReason::Lost);
        end.raise(EndReason::Forced);
        assert!(end.settle());
        assert_eq!(end.settled(), Some(EndReason::Forced));
    }

    #[test]
    fn a_settled_reason_ignores_a_later_force() {
        let end = TransportEnd::new();
        end.raise(EndReason::Lost);
        assert!(end.settle());
        end.raise(EndReason::Forced);
        assert_eq!(end.settled(), Some(EndReason::Lost));
    }

    #[test]
    fn a_second_settle_returns_false() {
        let end = TransportEnd::new();
        end.raise(EndReason::Lost);
        assert!(end.settle());
        assert!(!end.settle());
        end.raise(EndReason::Forced);
        assert!(!end.settle());
        assert_eq!(end.settled(), Some(EndReason::Lost));
    }

    #[test]
    fn racing_settles_have_one_winner_and_fix_the_reason() {
        for _ in 0..200 {
            let end = Arc::new(TransportEnd::new());
            end.raise(EndReason::Lost);
            let threads = 8;
            let start = Arc::new(Barrier::new(threads));
            let handles: Vec<_> = (0..threads)
                .map(|i| {
                    let end = Arc::clone(&end);
                    let start = Arc::clone(&start);
                    std::thread::spawn(move || {
                        start.wait();
                        if i % 2 == 0 {
                            end.raise(EndReason::Forced);
                        }
                        end.settle()
                    })
                })
                .collect();
            let winners = handles
                .into_iter()
                .map(|h| h.join().expect("racer"))
                .filter(|won| *won)
                .count();
            assert_eq!(winners, 1, "exactly one settle succeeds");
            let settled = end.settled().expect("settled after the race");
            end.raise(EndReason::Forced);
            assert_eq!(end.settled(), Some(settled), "reason fixed after settle");
        }
    }
}
