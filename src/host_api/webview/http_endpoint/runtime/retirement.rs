//! Retired setup threads keep the global owner permit through actual runtime/job destruction.
use super::RuntimeError;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
};
static OWNERS: AtomicUsize = AtomicUsize::new(0);
static RETIRED: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());
static POISONED: AtomicBool = AtomicBool::new(false);
pub(super) struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        OWNERS.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) fn retain(thread: JoinHandle<()>) {
    match RETIRED.lock() {
        Ok(mut retired) => retired.push(thread),
        Err(_) => {
            POISONED.store(true, Ordering::Release);
            tracing::error!("native runtime retirement poisoned; new runtimes refused");
            drop(thread);
        }
    }
}
pub(super) fn reap() -> Result<(), RuntimeError> {
    if POISONED.load(Ordering::Acquire) {
        return Err(RuntimeError::Worker(None));
    }
    let Ok(mut retired) = RETIRED.lock() else {
        POISONED.store(true, Ordering::Release);
        tracing::error!("native runtime retirement poisoned; new runtimes refused");
        return Err(RuntimeError::Worker(None));
    };
    let mut index = 0;
    while index < retired.len() {
        if retired[index].is_finished() {
            let thread = retired.swap_remove(index);
            if thread.join().is_err() {
                tracing::warn!("native runtime setup thread panicked");
            }
        } else {
            index += 1;
        }
    }
    Ok(())
}
pub(super) fn reserve() -> Result<Permit, RuntimeError> {
    reap()?;
    OWNERS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 8).then_some(n + 1)
        })
        .map_err(|_| RuntimeError::Capacity)?;
    Ok(Permit)
}
#[cfg(test)]
pub(super) fn owners() -> usize {
    OWNERS.load(Ordering::Acquire)
}
