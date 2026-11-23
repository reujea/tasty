use super::*;
use gtk::prelude::*;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[path = "tests/cache.rs"]
mod cache;
#[path = "tests/direct.rs"]
mod direct;
#[path = "tests/lifecycle.rs"]
mod lifecycle;
#[path = "tests/navigation.rs"]
mod navigation;
#[path = "tests/retained.rs"]
mod retained;

#[test]
fn proxy_requires_owned_authenticated_loopback_socks() {
    assert!(authority::proxy("socks5://u:p@127.0.0.1:1234"));
    for value in [
        "http://u:p@127.0.0.1:1234",
        "socks5://127.0.0.1:1234",
        "socks5://u:p@example.test:1234",
        "socks5://u:p@127.0.0.1:0",
    ] {
        assert!(!authority::proxy(value));
    }
}
#[test]
fn authority_keeps_port_and_rejects_non_web_credentials() {
    let a = authority::https("https://LOCALHOST/path").unwrap();
    assert_eq!(a.host, "localhost");
    assert_eq!(a.port, 443);
    assert_eq!(
        authority::https("https://localhost:444/").unwrap().port,
        444
    );
    assert_eq!(authority::https("https://[::1]:8443/").unwrap().host, "::1");
    assert!(authority::https("https://u:p@localhost/").is_none());
    assert!(authority::https("http://localhost/").is_none());
}
#[test]
fn selected_clear_excludes_cookie_and_unrelated_storage() {
    use webkit2gtk::WebsiteDataTypes;
    let types = store::clear_types();
    assert!(!types.contains(WebsiteDataTypes::COOKIES));
    assert!(!types.contains(WebsiteDataTypes::LOCAL_STORAGE));
    assert!(types.contains(WebsiteDataTypes::SERVICE_WORKER_REGISTRATIONS));
    assert!(types.contains(WebsiteDataTypes::DOM_CACHE));
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut started = false;
    // 원래 순서(읽기 → 기한 검사 → 대기 중인 GLib 일 처리 → 5ms) 그대로, 기한 뒤 읽기는 처리 없이 본다.
    let held = tasty_utils::test_wait::holds_by_every(deadline, Duration::ZERO, |last| {
        if !last && std::mem::replace(&mut started, true) {
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        condition()
    });
    assert!(held, "native fixture deadline");
}
fn config(current: Arc<AtomicBool>) -> ContextConfig {
    let expected = std::env::var("DE_EXPECTED_LEAF").expect("owned fixture leaf");
    let expected: [u8; 32] = (0..32)
        .map(|i| u8::from_str_radix(&expected[i * 2..i * 2 + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let case = std::env::var("DE_CASE").unwrap();
    let other = std::env::var("DE_OTHER_LEAF").unwrap_or_else(|_| "00".repeat(32));
    let other: [u8; 32] = (0..32)
        .map(|i| u8::from_str_radix(&other[i * 2..i * 2 + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    ContextConfig {
        proxy_uri: std::env::var("DE_PROXY_URI").unwrap(),
        current: Arc::new(move || current.load(Ordering::Acquire)),
        expected_leaf: Arc::new(move |host, port| {
            if case == "bound" && port == fixture_port("HTTPS") {
                // These held probes never receive TLS bytes. Distinct expected
                // leaves isolate pending capacity from duplicate-leaf refusal.
                use sha2::Digest;
                return Some(sha2::Sha256::digest(host.as_bytes()).into());
            }
            if host == "localhost"
                && port == fixture_port("OTHER")
                && matches!(case.as_str(), "port" | "history")
            {
                return Some(other);
            }
            if (host == "localhost" || case == "bound")
                && port == fixture_port("HTTPS")
                && case != "wrong_host"
            {
                Some(expected)
            } else {
                None
            }
        }),
    }
}
// Only the fixture creates a worker here; production receives an already
// prepared profile from the host's bounded worker before entering GTK.
fn data_store() -> DataStore {
    let home = std::path::PathBuf::from(
        std::env::var_os("DE_PROFILE_HOME").expect("owned private profile home"),
    );
    let (receipt, work) = super::super::preparation::Preparation::new(
        home,
        32,
        Duration::from_secs(5),
        Arc::new(|| true),
        Arc::new(|| {}),
    );
    let worker = std::thread::spawn(move || work.run());
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut result = None;
    assert!(
        tasty_utils::test_wait::holds_by(deadline, || {
            result = receipt.poll();
            result.is_some()
        }),
        "owned profile preparation deadline"
    );
    let profile = result
        .unwrap()
        .unwrap_or_else(|error| panic!("owned profile preparation: {error}"));
    worker.join().expect("owned profile fixture worker");
    DataStore::new(profile).unwrap_or_else(|error| panic!("native store: {error:?}"))
}
fn fixture_port(kind: &str) -> u16 {
    std::env::var(format!("DE_{kind}_PORT"))
        .expect("owned logical destination port")
        .parse()
        .expect("owned logical port")
}
fn fixture_url(kind: &str, path: &str) -> String {
    let scheme = if kind == "HTTP" { "http" } else { "https" };
    format!("{scheme}://localhost:{}{path}", fixture_port(kind))
}

fn prepared(store: &DataStore, live: &Arc<AtomicBool>) -> PreparedView {
    let result = Rc::new(RefCell::new(None));
    let out = result.clone();
    store.prepare(
        config(live.clone()),
        Box::new(move |r| *out.borrow_mut() = Some(r)),
    );
    assert!(
        result.borrow().is_none(),
        "clear must complete asynchronously before exposing a view"
    );
    wait(|| result.borrow().is_some());
    let prepared = result
        .borrow_mut()
        .take()
        .unwrap()
        .unwrap_or_else(|e| panic!("prepare: {e:?}"));
    if let Some(settings) = WebViewExt::settings(prepared.webview()) {
        WebSettingsExt::set_enable_page_cache(&settings, false);
    }
    tracing::info!(
        "NATIVE_CONTEXT generation=1 context={:p} manager={:p} current={}",
        prepared.0.context.as_ptr(),
        store.0.manager.as_ptr(),
        prepared.0.live()
    );
    prepared
}
fn window(view: &PreparedView) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.add(view.webview());
    window.show_all();
    window
}
fn destroy(window: &gtk::Window) {
    // SAFETY: owned fixture widget, on its GTK thread, destroyed exactly once.
    unsafe {
        window.destroy();
    }
}
#[test]
#[ignore = "requires the owned isolated native fixture"]
fn native_suite() {
    if let Err(error) = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .without_time()
        .with_ansi(false)
        .try_init()
    {
        tracing::debug!(%error, "native fixture tracing already configured");
    }
    gtk::init().unwrap();
    let case = std::env::var("DE_CASE").unwrap();
    match case.as_str() {
        "lifecycle" => lifecycle::run(),
        "retained_lifecycle" => retained::lifecycle(),
        "retained_https" => retained::https(),
        "retained_held" => retained::held(false),
        "retained_held_cancel" => retained::held(true),
        "retained_epoch" => retained::epoch(),
        "retained_post" | "retained_no_clear" => retained::post_cookie(&case),
        "cache" | "cache_private" => cache::run(),
        "direct_paths"
        | "direct_paths_header_on"
        | "direct_paths_meta_on"
        | "direct_paths_control" => direct::run(&case),
        _ => navigation::run(&case),
    }
    tracing::info!("NATIVE_CASE_PASS {case}");
}

#[test]
fn manager_allowances_are_finite_and_failed_reservations_do_not_consume_trust() {
    let history = Rc::new(RefCell::new(allowance::History::default()));
    let permit = allowance::History::reserve(&history, [1; 32]).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(matches!(
        allowance::History::reserve(&history, [1; 32]),
        Err(ContextError::Tls)
    ));
    drop(permit);
    assert_eq!(history.borrow().used(), 0);
    for n in 0..allowance::LIMIT {
        let mut leaf = [0; 32];
        leaf[..8].copy_from_slice(&(n as u64).to_le_bytes());
        let mut permit =
            allowance::History::reserve(&history, leaf).unwrap_or_else(|e| panic!("{e:?}"));
        permit.commit();
    }
    assert_eq!(history.borrow().used(), allowance::LIMIT);
    assert!(matches!(
        allowance::History::reserve(&history, [255; 32]),
        Err(ContextError::Capacity)
    ));
}

#[test]
fn non_utf8_real_profile_fails_closed_before_native_construction_and_retains_ownership() {
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, PermissionsExt},
    };
    let home = tempfile::tempdir().unwrap();
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let parent = home
        .path()
        .join(std::ffi::OsStr::from_bytes(b"private-\xff"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&parent)
        .unwrap();
    let profile = PreparedProfile::create(&parent, 1).unwrap();
    let root = profile.root().to_owned();
    assert!(matches!(
        DataStore::new(profile),
        Err(ContextError::Storage)
    ));
    assert!(
        root.is_dir(),
        "failed construction must retain the exact owned root"
    );
}
