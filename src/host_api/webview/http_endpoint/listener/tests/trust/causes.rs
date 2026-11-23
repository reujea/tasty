//! A listener that cannot open keeps its OS cause in the error value and names its use in a warning.
use super::super::fault::{Fault, inject};
use super::*;
use std::sync::Mutex;

/// A raw value with no meaning on any OS: only its delivery is checked, not what it stands for.
const RAW: i32 = 9999;

fn kind() -> std::io::ErrorKind {
    std::io::Error::from_raw_os_error(RAW).kind()
}

/// Collects DEBUG events from every thread, since the probe runs on a runtime worker. Only a
/// `rerun_alone` child may install it: that process runs this one test, so the process-wide
/// subscriber reaches no other test.
fn global_log() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        crate::test_support::is_rerun_alone(),
        "a process-wide subscriber is installed only in a rerun_alone child"
    );
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = Sink(buffer.clone());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_writer(move || sink.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("the only global subscriber");
    buffer
}

fn text(log: &Mutex<Vec<u8>>) -> String {
    String::from_utf8_lossy(
        &log.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
    .into_owned()
}

/// Sends an HTTPS ClientHello that reaches the trust probe, then checks the original connection
/// ends, its error keeps the cause, one warning names the probe, and the trust permit returns.
async fn probe_fails(fault: Fault, variant: &str, warning: &str) {
    let log = global_log();
    let budget = EndpointBudget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    inject(fault);
    let (endpoint, native) = start(
        Duration::from_secs(3),
        budget.clone(),
        Arc::new(AtomicBool::new(true)),
        calls.clone(),
    )
    .await;
    let (mut socket, _) = hello(&endpoint, "remote.example").await;
    closed(&mut socket).await;
    // The connection task logs its error after it has released the socket.
    let ended = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let ended = text(&log)
                .lines()
                .find(|line| line.contains("native mirror connection ended"))
                .map(str::to_owned);
            if let Some(line) = ended {
                return line;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the connection error was logged");
    assert!(ended.contains(&format!("{variant} {{")), "{ended}");
    assert!(ended.contains(&format!("kind: {:?}", kind())), "{ended}");
    assert!(ended.contains(&format!("os_error: Some({RAW})")), "{ended}");
    assert_eq!(budget.active_trust(), 0);
    assert!(native.poll().is_none());
    assert_eq!(calls.load(Ordering::Acquire), 0);
    endpoint.join().await.unwrap();
    let log = text(&log);
    let warnings: Vec<_> = log
        .lines()
        .filter(|line| line.contains(" WARN ") && line.contains(warning))
        .collect();
    assert_eq!(warnings.len(), 1, "{log}");
    assert!(warnings[0].contains("listener=TrustProbe"), "{log}");
    assert!(
        warnings[0].contains(&format!("os_error=Some({RAW})")),
        "{log}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trust_probe_that_cannot_bind_keeps_its_cause_and_returns_the_trust_permit() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_trust_probe_that_cannot_bind_keeps_its_cause_and_returns_the_trust_permit",
    ) {
        return;
    }
    probe_fails(Fault::ProbeBind(RAW), "Bind", "HTTP endpoint bind failed").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trust_probe_without_an_address_keeps_its_cause_apart_from_bind() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_trust_probe_without_an_address_keeps_its_cause_apart_from_bind",
    ) {
        return;
    }
    probe_fails(
        Fault::ProbeAddr(RAW),
        "LocalAddr",
        "HTTP endpoint listener address unavailable",
    )
    .await;
}

#[tokio::test]
async fn an_endpoint_without_an_address_keeps_its_cause_apart_from_bind_and_returns_its_owner() {
    let budget = EndpointBudget::new();
    inject(Fault::EndpointAddr(RAW));
    let handler: Handler = Arc::new(|request| Box::pin(async move { Ok(reply(request)) }));
    let started = Endpoint::start(
        Handle::current(),
        config("https://remote.example:8443/doc".into(), handler),
        budget.clone(),
    )
    .await;
    assert_eq!(
        started.err(),
        Some(EndpointError::LocalAddr {
            kind: kind(),
            os_error: Some(RAW),
        })
    );
    assert_eq!(budget.active_owners(), 0);
}
