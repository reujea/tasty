//! Actual GIO async tasks; run only on an isolated display, never the user's GTK context.
use super::*;
use crate::webview::entry_response::tests::{entry, id, store};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

fn drain_until(context: &glib::MainContext, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    // 원래 순서(읽기 → 기한 검사 → 대기 중인 GLib 일 처리 → 1ms) 그대로, 기한 뒤 읽기는 처리 없이 본다.
    let finished = tasty_utils::test_wait::holds_by_every(end, Duration::ZERO, |last| {
        if !last && std::mem::replace(&mut started, true) {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        done()
    });
    assert!(finished, "native task did not finish");
}
type ReadResults = Rc<RefCell<Vec<Result<Vec<u8>, glib::Error>>>>;
fn read(stream: &DeferredStream, cancel: &gio::Cancellable) -> ReadResults {
    let results = Rc::new(RefCell::new(Vec::new()));
    let out = results.clone();
    stream.read_bytes_async(8192, glib::Priority::DEFAULT, Some(cancel), move |result| {
        out.borrow_mut()
            .push(result.map(|bytes| bytes.as_ref().to_vec()));
    });
    results
}
fn body(bytes: &[u8]) -> Arc<NativeBodyResponse> {
    Arc::new(
        NativeBodyResponse::new(
            id(),
            8,
            "text/css",
            "file:///server/style.css",
            Arc::new(bytes.to_vec()),
        )
        .unwrap(),
    )
}
fn unread_streams_are_bounded_and_close_revokes_them(source: &EntryResponseSource) {
    // Admission is bounded even before GIO starts its first read. Close revokes these waits too.
    let unread = PendingReads::default();
    let mut held = Vec::new();
    let mut targets = Vec::new();
    for _ in 0..256 {
        let (wait, target) = ResponseWait::pair();
        held.push(DeferredStream::new(source.clone(), wait, &unread).unwrap());
        targets.push(target);
    }
    let (wait, rejected) = ResponseWait::pair();
    assert!(DeferredStream::new(source.clone(), wait, &unread).is_none());
    assert!(!rejected.live());
    unread.close();
    assert!(targets.iter().all(|target| !target.live()));
    drop(held);
}
#[test]
#[ignore = "requires an isolated GTK display and serialized native execution"]
fn gio_abort_close_late_completion_and_exact_body() {
    gtk::init().unwrap();
    let context = glib::MainContext::default();
    let _owner = context.acquire().unwrap();
    let backing = store(entry());
    let source = EntryResponseSource::new(&backing);
    unread_streams_are_bounded_and_close_revokes_them(&source);
    let registry = PendingReads::default();
    let (wait_a, target_a) = ResponseWait::pair();
    let (wait_b, target_b) = ResponseWait::pair();
    let a = DeferredStream::new(source.clone(), wait_a, &registry).unwrap();
    let b = DeferredStream::new(source.clone(), wait_b, &registry).unwrap();
    let ca = gio::Cancellable::new();
    let cb = gio::Cancellable::new();
    let ra = read(&a, &ca);
    let rb = read(&b, &cb);
    while context.pending() {
        context.iteration(false);
    }
    assert!(ra.borrow().is_empty() && rb.borrow().is_empty());
    assert!(target_a.live() && target_b.live());
    ca.cancel();
    drain_until(&context, || !ra.borrow().is_empty());
    assert!(
        ra.borrow()[0]
            .as_ref()
            .unwrap_err()
            .matches(gio::IOErrorEnum::Cancelled)
    );
    assert!(!target_a.live());
    assert!(target_b.live() && rb.borrow().is_empty());
    target_a.finish(Ok(body(b"late bytes never reach A")));
    registry.close();
    drain_until(&context, || !rb.borrow().is_empty());
    assert!(
        rb.borrow()[0]
            .as_ref()
            .unwrap_err()
            .matches(gio::IOErrorEnum::Cancelled)
    );
    assert!(!target_b.live());
    target_b.finish(Ok(body(b"late bytes never reach B")));
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(ra.borrow().len(), 1);
    assert_eq!(rb.borrow().len(), 1);
    assert!(!pending_reads::callbacks_pending());

    let registry = PendingReads::default();
    let (wait, target) = ResponseWait::pair();
    let stream = DeferredStream::new(source.clone(), wait, &registry).unwrap();
    let response = URISchemeResponse::new(&stream, -1);
    stream.set_response(&response);
    let cancel = gio::Cancellable::new();
    let received = read(&stream, &cancel);
    while context.pending() {
        context.iteration(false);
    }
    assert!(received.borrow().is_empty());
    let bytes = b"body{background:green}";
    target.finish(Ok(body(bytes)));
    drain_until(&context, || !received.borrow().is_empty());
    assert_eq!(received.borrow()[0].as_ref().unwrap(), bytes);
    let eof = read(&stream, &cancel);
    drain_until(&context, || !eof.borrow().is_empty());
    assert!(eof.borrow()[0].as_ref().unwrap().is_empty());

    let (wait, target) = ResponseWait::pair();
    let stream = DeferredStream::new(source.clone(), wait, &registry).unwrap();
    let response = URISchemeResponse::new(&stream, -1);
    stream.set_response(&response);
    let stale = read(&stream, &cancel);
    target.finish(Ok(body(b"stale")));
    backing.lock().unwrap().retire_native_view();
    drain_until(&context, || !stale.borrow().is_empty());
    assert!(stale.borrow()[0].is_err());
    assert!(!pending_reads::callbacks_pending());
    registry.close();
    browser::exercise(&context);
}

mod browser;
