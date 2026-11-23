//! Production file APIs, with two owned processes sharing the host/CLI file lock.
use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);
fn items(id: &str) -> Vec<PersistedWebhook> {
    vec![PersistedWebhook {
        id: id.into(),
        methods: vec!["POST".into()],
        handler: None,
        calls: vec![],
        limit: PersistedLimit::Count { remaining: 7 },
        auth: Some(WebhookAuth {
            location: super::super::auth::AuthLocation::BearerHeader,
            token: "isolated-fixture-credential".into(),
        }),
    }]
}

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
            self.0.wait().unwrap();
        }
    }
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + WAIT;
    assert!(
        tasty_utils::test_wait::holds_by(until, condition),
        "owned file participant did not finish"
    );
}

#[test]
fn file_participant_child() {
    let Some(role) = std::env::var_os("TASTY_WEBHOOK_FILE_CHILD") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("TASTY_WEBHOOK_FILE_PATH").unwrap());
    let ready = PathBuf::from(std::env::var_os("TASTY_WEBHOOK_FILE_READY").unwrap());
    std::fs::write(ready, b"ready").unwrap();
    for n in 0..16 {
        if role == "host" {
            assert!(
                write_to(&path, &items(&format!("host-{n}"))).is_ok(),
                "host file mutation failed"
            );
        } else {
            assert!(
                tasty_settings::webhook_port_file::set_port(&path, 40000 + n).is_ok(),
                "CLI port participant failed"
            );
        }
    }
}

fn participant_driver(role: &str) -> (PathBuf, &'static str) {
    // An independently built CLI test driver exercises the actual offline adapter without
    // introducing a host -> CLI source dependency. Ordinary unit runs use the shared port API.
    let cli_driver = (role == "port")
        .then(|| std::env::var_os("TASTY_TEST_WEBHOOK_CLI_DRIVER"))
        .flatten();
    match cli_driver {
        Some(executable) => {
            tracing::info!("webhook_file_cli_driver=true");
            (
                PathBuf::from(executable),
                "local::port::webhook_file_tests::file_participant_child",
            )
        }
        None => (
            std::env::current_exe().unwrap(),
            "webhook::persist::backend_tests::file_participant_child",
        ),
    }
}

fn spawn_participant(role: &str, path: &Path, directory: &Path) -> Owned {
    let ready = directory.join(role);
    let (executable, test) = participant_driver(role);
    let child = Owned(
        Command::new(executable)
            .args(["--exact", test, "--nocapture"])
            .env("TASTY_WEBHOOK_FILE_CHILD", role)
            .env("TASTY_WEBHOOK_FILE_PATH", path)
            .env("TASTY_WEBHOOK_FILE_READY", &ready)
            .env("TASTY_HOME", directory.join("home"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    tracing::info!(role, pid = child.0.id(), "webhook_file_owned_child");
    wait_for(|| ready.exists());
    child
}

fn assert_participants_blocked(children: &mut [Owned], path: &Path) {
    std::thread::sleep(Duration::from_millis(100));
    for child in children {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "production participant bypassed held file lock"
        );
    }
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .contains("port=39999")
    );
}

fn reap_participants(children: Vec<Owned>) {
    for mut child in children {
        wait_for(|| child.0.try_wait().unwrap().is_some());
        assert!(
            child.0.try_wait().unwrap().unwrap().success(),
            "owned participant failed"
        );
        tracing::info!(pid = child.0.id(), "webhook_file_owned_child reaped=true");
    }
}

#[test]
fn host_and_cli_processes_read_merge_replace_under_the_same_lock() {
    let subscriber = tracing_subscriber::fmt()
        .with_test_writer()
        .without_time()
        .with_ansi(false)
        .finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, "format=2\nport=39999\n[listener]\nkeep=true\n").unwrap();
    let lock = tasty_utils::file_replace::lock(&path).unwrap();
    let mut children = Vec::new();
    for role in ["host", "port"] {
        children.push(spawn_participant(role, &path, dir.path()));
    }
    assert_participants_blocked(&mut children, &path);
    drop(lock);
    reap_participants(children);
    let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert_eq!(table["port"].as_integer(), Some(40015));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    assert!(table["webhook"][0]["id"].as_str() == Some("host-15"));
    assert!(table["webhook"][0]["auth"]["token"].as_str() == Some("isolated-fixture-credential"));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("webhooks.toml.lock"))
        .unwrap();
    assert!(
        lock.try_lock().is_ok(),
        "lock must drop before caller-side external actions"
    );
}

#[test]
fn actual_write_preserves_foreign_listener_port_and_auth() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, "format=2\nport=40123\n[listener]\nkeep=true\n").unwrap();
    assert!(write_to(&path, &items("fixture")).is_ok());
    let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert_eq!(table["port"].as_integer(), Some(40123));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    assert!(table["webhook"][0]["auth"]["token"].as_str() == Some("isolated-fixture-credential"));
    assert!(write_to(&path, &[]).is_ok());
    let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert!(!table.contains_key("webhook"));
    assert_eq!(table["port"].as_integer(), Some(40123));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
}

#[test]
fn owned_request_reads_latest_foreign_sections_when_executed() {
    fn owned<T: Send + 'static>(_: &T) {}
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("webhooks.toml");
    std::fs::write(&path, "format=2\nport=40000\n").unwrap();
    let request = WriteRequest::new(path.clone(), items("queued"));
    owned(&request);
    std::fs::write(&path, "format=2\nport=42000\n[listener]\nkeep=true\n").unwrap();
    let execute_borrowed = |request: &WriteRequest| request.execute();
    assert!(execute_borrowed(&request).is_ok());
    let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
    assert_eq!(table["port"].as_integer(), Some(42000));
    assert_eq!(table["listener"]["keep"].as_bool(), Some(true));
    assert!(table["webhook"][0]["id"].as_str() == Some("queued"));
}
