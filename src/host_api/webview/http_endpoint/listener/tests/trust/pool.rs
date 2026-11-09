//! Real published profile roots and TLS transport. The ACK actor is synthetic, not GTK.
use super::super::super::trust::pool::{ProbeId, ProbePool};
use super::*;
#[cfg(target_os = "linux")]
use crate::host_api::webview::{
    http_context::{
        preparation::{MAX_CLAIM_WAIT, Preparation},
        profile::NativeProfileLease,
    },
    http_endpoint::runtime::tests::HANG_LIMIT,
};
#[cfg(target_os = "linux")]
async fn profile(home: &std::path::Path) -> NativeProfileLease {
    let (receipt, work) = Preparation::new(
        home.to_path_buf(),
        8,
        MAX_CLAIM_WAIT,
        Arc::new(|| true),
        Arc::new(|| {}),
    );
    let worker = tokio::task::spawn_blocking(move || work.run());
    let profile = tokio::time::timeout(HANG_LIMIT, async {
        loop {
            // The worker publishes before it ends, so read its end before the receipt.
            let ended = worker.is_finished();
            if let Some(result) = receipt.poll() {
                return result.unwrap_or_else(|error| panic!("real prepared profile: {error}"));
            }
            assert!(!ended, "profile work: lost, it ended without a result");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("profile work: hung, not finished within the limit");
    let profile = profile.into_native();
    worker.await.unwrap();
    profile
}
#[cfg(target_os = "linux")]
fn roots(home: &std::path::Path) -> usize {
    std::fs::read_dir(home.join("native-http-profiles"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .count()
}
async fn next<T>(pool: &mut ProbePool<T>, native: &NativeTrust) -> ProbeId {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(id) = pool.poll(native, || true) {
                return id;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap()
}
async fn complete<T>(pool: &mut ProbePool<T>, id: ProbeId) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !pool.complete(id) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fixed_private_profile_survives_36_tls_requests_and_12_fresh_documents_with_two_actual_roots()
 {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::Builder::new()
        .prefix("tasty-trust-pool-")
        .tempdir()
        .unwrap();
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    // Separate actual target and private probe profiles. Published roots survive lease Drop,
    // so counting directories catches linear allocation rather than just live Arc counts.
    let target = profile(home.path()).await;
    let probe_profile = profile(home.path()).await;
    let probe_root = probe_profile.root().to_owned();
    let mut pool = ProbePool::new(probe_profile);
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut prior_leaf = None;
    let mut prior_id = None;
    assert_eq!(roots(home.path()), 2);
    for _document in 0..12 {
        let (endpoint, native) = start(
            Duration::from_secs(3),
            budget.clone(),
            Arc::new(AtomicBool::new(true)),
            calls.clone(),
        )
        .await;
        let ready = endpoint
            .prepare_https("https://remote.example:8443/doc")
            .unwrap()
            .await
            .unwrap_or_else(|_| panic!("pin"));
        if let Some(old) = prior_leaf {
            assert_ne!(ready.leaf(), old);
        }
        prior_leaf = Some(ready.leaf());
        for _resource in 0..3 {
            let (socket, client) = hello(&endpoint, "remote.example").await;
            let id = next(&mut pool, &native).await;
            assert!(pool.poll(&native, || true).is_none());
            assert!(!pool.complete(id));
            if let Some(old) = prior_id {
                assert!(pool.request(old).is_none());
                assert!(!pool.reject(old));
                assert!(!pool.complete(old));
                let observation = observation(pool.request(id).unwrap());
                assert!(!pool.ack(old, observation));
                assert!(pool.request(id).is_some());
            }
            let request = pool.request(id).unwrap();
            probe(request, false).await;
            let observed = observation(request);
            assert!(pool.ack(id, observed));
            assert!(pool.request(id).is_none());
            assert!(pool.poll(&native, || true).is_none());
            let response = finish(socket, client).await;
            assert!(response.ends_with(b"REMOTE body"));
            complete(&mut pool, id).await;
            assert_eq!(pool.resource().root(), probe_root);
            assert_eq!(roots(home.path()), 2);
            prior_id = Some(id);
        }
        endpoint.join().await.unwrap();
        assert_eq!(budget.active_trust(), 0);
        assert_eq!(budget.active_probes(), 0);
    }
    tracing::info!(
        "TRUST_POOL tls_connections=36 fresh_documents=12 published_profile_roots={} original_root_cap=8 target_profiles=1 probe_profiles=1",
        roots(home.path())
    );
    assert_eq!(calls.load(Ordering::Acquire), 36);
    assert_eq!(roots(home.path()), 2);
    assert!(target.root().is_dir());
    pool.retire();
    drop(pool);
    drop(target);
    // No WebKit object exists here; all profile/transport tasks are joined before TempDir cleanup.
    assert_eq!(roots(home.path()), 2);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_native_owner_replacement_cancels_pool_ack_and_old_ticket_cannot_release_new_probe()
{
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        calls.clone(),
    )
    .await;
    let (mut socket, _) = hello(&endpoint, "remote.example").await;
    let view = std::rc::Rc::new(std::cell::Cell::new(true));
    let mut pool = ProbePool::new("one fixed manager");
    let id = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let view = view.clone();
            if let Some(id) = pool.poll(&native, move || view.get()) {
                return id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    probe(pool.request(id).unwrap(), false).await;
    let observed = observation(pool.request(id).unwrap());
    view.set(false);
    assert!(pool.request(id).is_none());
    assert!(!pool.ack(id, observed));
    closed(&mut socket).await;
    complete(&mut pool, id).await;
    let (mut socket, _) = hello(&endpoint, "remote.example").await;
    let fresh = next(&mut pool, &native).await;
    assert!(pool.request(id).is_none());
    assert!(!pool.complete(id));
    assert!(!pool.reject(id));
    assert!(pool.request(fresh).is_some());
    assert!(pool.reject(fresh));
    closed(&mut socket).await;
    complete(&mut pool, fresh).await;
    assert_eq!(calls.load(Ordering::Acquire), 0);
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_trust(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pool_cancellation_and_drop_keep_other_inbox_work_owned_until_actual_drop() {
    let budget = EndpointBudget::new();
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    )
    .await;
    let (mut first, _) = hello(&endpoint, "remote.example").await;
    let mut pool = ProbePool::new(());
    let id = next(&mut pool, &native).await;
    let (mut second, _) = hello(&endpoint, "remote.example").await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while budget.active_trust() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(pool.poll(&native, || true).is_none());
    pool.retire();
    closed(&mut first).await;
    assert!(pool.request(id).is_none());
    complete(&mut pool, id).await;
    let id = next(&mut pool, &native).await;
    drop(pool);
    closed(&mut second).await;
    assert!(native.poll().is_none());
    // Tickets are unique even across a replaced pool object and cannot settle its new job.
    let (mut third, _) = hello(&endpoint, "remote.example").await;
    let mut replacement = ProbePool::new(());
    let fresh = next(&mut replacement, &native).await;
    assert!(replacement.request(id).is_none());
    assert!(!replacement.reject(id));
    assert!(!replacement.complete(id));
    assert!(replacement.reject(fresh));
    closed(&mut third).await;
    complete(&mut replacement, fresh).await;
    endpoint.join().await.unwrap();
    assert_eq!(budget.active_trust(), 0);
    assert_eq!(budget.active_probes(), 0);
}
