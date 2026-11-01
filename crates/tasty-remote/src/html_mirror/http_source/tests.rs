//! Real metadata/Reload/disconnect and native-view callback owners; no native engine fixture.
use super::*;
use crate::connection::channel;
fn metadata(mirror: &mut HtmlMirror, generation: u64, url: &str) {
    let id = mirror.id.unwrap();
    assert!(mirror.receive(HtmlReply::HtmlMetadata {
        id,
        metadata: HtmlMetadata {
            source: HtmlSource::Http { url: url.into() },
            source_generation: generation,
            root: None,
            cwd: Some("/remote-cwd".into()),
            size: 0,
            regular: false,
            file: None
        }
    }));
}
fn mirror() -> HtmlMirror {
    let (sender, _rx) = channel();
    let mut mirror = HtmlMirror::new(42, 7, sender.epoch());
    begin(&mut mirror, 1);
    mirror
}
fn begin(mirror: &mut HtmlMirror, request: u64) {
    let id = TransferId {
        epoch: 1,
        surface_id: 42,
        binding: 7,
        document: mirror.generation,
        request,
        transfer: request,
    };
    assert_eq!(mirror.begin(id).len(), 1);
}
#[test]
fn actual_http_metadata_binds_source_without_protocol_id_allocation_or_client_file() {
    let mut mirror = mirror();
    assert!(mirror.http_source().is_none());
    metadata(&mut mirror, 9, "https://remote.example/page?mode=1#section");
    let source = mirror.http_source().unwrap();
    assert_eq!(source.metadata_id(), mirror.id.unwrap());
    assert_eq!(source.source_generation(), 9);
    assert_eq!(source.entry_url(), "https://remote.example/page?mode=1");
    assert_eq!(
        source.address(),
        "https://remote.example/page?mode=1#section"
    );
    assert!(source.is_current());
    assert!(mirror.body_store().current().is_none());
    // The non-file metadata already ends file receiving; its subsequent failure is ignored.
    // This retains the captured HTTP
    // metadata for the separately negotiated native route. It does not supply any body bytes.
    assert_eq!(mirror.state(), Some(ReceiveState::Failed));
    assert!(!mirror.receive(HtmlReply::HtmlFailure {
        id: mirror.id.unwrap(),
        reason: "HTTP source requires remote proxy".into()
    }));
    assert!(source.is_current());
    assert!(mirror.http_source().is_some());
    assert!(!mirror.receiving());
}
#[test]
fn replacement_view_cancels_old_callbacks_without_retiring_logical_body_source() {
    let mut mirror = mirror();
    metadata(&mut mirror, 9, "http://remote.example/");
    let source = mirror.http_source().unwrap();
    let first = source.new_view().unwrap();
    let callback = first.clone();
    assert!(first.is_current());
    let next = source.new_view().unwrap();
    assert!(!callback.is_current());
    assert!(next.is_current());
    drop(first);
    drop(callback);
    assert!(next.is_current());
    assert!(source.is_current());
    assert_eq!(next.source().source_generation(), 9);
    next.retire();
    assert!(!next.is_current());
    assert!(source.is_current());
    let third = source.new_view().unwrap();
    drop(next);
    assert!(third.is_current());
}
#[test]
fn reload_disconnect_and_mirror_drop_revoke_native_held_source_without_model_reentry() {
    let mut mirror = mirror();
    metadata(&mut mirror, 9, "http://remote.example/");
    let old = mirror.http_source().unwrap();
    let view = old.new_view().unwrap();
    assert!(mirror.user_reload());
    assert!(!old.is_current());
    assert!(!view.is_current());
    assert!(mirror.http_source().is_none());
    begin(&mut mirror, 2);
    metadata(&mut mirror, 10, "http://remote.example/new");
    let next = mirror.http_source().unwrap();
    assert_eq!(next.metadata_id().document, 2);
    assert_eq!(next.source_generation(), 10);
    assert!(next.is_current());
    mirror.disconnect();
    assert!(!next.is_current());
    assert!(next.new_view().is_none());
    let mut mirror = super::tests::mirror();
    metadata(&mut mirror, 1, "https://remote.example/");
    let source = mirror.http_source().unwrap();
    drop(mirror);
    assert!(!source.is_current());
}
#[test]
fn file_and_credential_or_invalid_metadata_never_create_native_http_source() {
    for url in [
        "file:///client/sentinel.html",
        "http://user:secret@remote.example/",
        "not a URL",
    ] {
        let mut mirror = mirror();
        metadata(&mut mirror, 1, url);
        assert!(mirror.http_source().is_none());
    }
    let mut mirror = mirror();
    metadata(&mut mirror, 0, "http://remote.example/");
    assert!(mirror.http_source().is_none());
}

#[test]
fn concurrent_view_replacement_cannot_resurrect_an_old_callback_or_drop_the_latest_owner() {
    let mut mirror = mirror();
    metadata(&mut mirror, 1, "http://remote.example/");
    let source = mirror.http_source().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let source = source.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            source.new_view().unwrap()
        }));
    }
    let mut views: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(views.iter().filter(|view| view.is_current()).count(), 1);
    let newest = views.remove(views.iter().position(|view| view.is_current()).unwrap());
    drop(views);
    assert!(newest.is_current());
    drop(newest);
    assert!(source.is_current());
    let replacement = source.new_view().unwrap();
    assert!(replacement.is_current());
}
// The App keeps a failed HTTP session while its source identity is unchanged. A reconnect builds a
// new mirror with a new binding, so the same address and generation must not look unchanged.
#[test]
fn a_reconnected_mirror_gives_the_same_address_a_new_source_identity() {
    let mut old = mirror();
    metadata(&mut old, 9, "https://remote.example/page");
    let before = old.http_source().unwrap();
    old.disconnect();
    assert!(!before.is_current());
    assert!(old.http_source().is_none());

    let (sender, _rx) = channel();
    let mut new = HtmlMirror::new(42, 8, sender.epoch());
    let stale = TransferId {
        epoch: 1,
        surface_id: 42,
        binding: 7,
        document: new.generation,
        request: 1,
        transfer: 1,
    };
    assert!(new.begin(stale).is_empty());
    let id = TransferId {
        epoch: 2,
        binding: 8,
        ..stale
    };
    assert_eq!(new.begin(id).len(), 1);
    metadata(&mut new, 9, "https://remote.example/page");
    let after = new.http_source().unwrap();
    assert_eq!(after.source_generation(), before.source_generation());
    assert_eq!(after.entry_url(), before.entry_url());
    assert_ne!(after.metadata_id(), before.metadata_id());
}
