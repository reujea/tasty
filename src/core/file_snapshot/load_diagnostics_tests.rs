use crate::file::{format::FileFormatRegistry, handler::FileHandlerRegistry};
use crate::hook_handler::registry::HookHandlerRegistry;
use crate::runtime::registries::file_rules::FileRules;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Other tests hit the load warning callsite with no subscriber, which can cache "never" for the
/// whole process (docs/dev-guide/unit-test-isolation.md). Callers must run alone through
/// `rerun_alone`.
fn warnings(run: impl FnOnce()) -> String {
    assert!(
        crate::test_support::is_rerun_alone(),
        "load warning checks must run through rerun_alone"
    );
    let buffer = Buffer(Arc::new(Mutex::new(Vec::new())));
    let sink = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .without_time()
        .with_writer(move || sink.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        run();
    });
    String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap()
}
fn load(home: &std::path::Path) {
    HookHandlerRegistry::new().install_user_config(&home.join("hook-handlers.toml"));
    FileRules::load(
        &home.join("file-handlers.toml"),
        &FileFormatRegistry::new(),
        &FileHandlerRegistry::new(),
    );
}
fn assert_both(logs: &str, cause: &str) {
    assert_eq!(
        logs.matches("user file load failed; protecting original")
            .count(),
        2,
        "{logs}"
    );
    for file in ["hook-handlers.toml", "file-handlers.toml"] {
        assert!(logs.contains(file), "{logs}");
    }
    assert_eq!(logs.matches(cause).count(), 2, "{logs}");
}
#[test]
fn initial_registry_parse_diagnostics_identify_files_without_parser_excerpts() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "initial_registry_parse_diagnostics_identify_files_without_parser_excerpts",
    ) {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let hook = "[[handler]]\nid='user/test'\npriority='HOOK_PRIVATE_SENTINEL_082'\n";
    let file = "[[handler]]\nid='host/test'\ndisabled='FILE_PRIVATE_SENTINEL_082'\n";
    std::fs::write(home.path().join("hook-handlers.toml"), hook).unwrap();
    std::fs::write(home.path().join("file-handlers.toml"), file).unwrap();
    let logs = warnings(|| load(home.path()));
    assert_both(&logs, "file parse failed");
    assert!(!logs.contains("PRIVATE_SENTINEL"), "{logs}");
    assert!(!logs.contains("priority="));
    assert!(!logs.contains("disabled="));
    assert!(
        !logs.contains(&home.path().display().to_string()),
        "filename only"
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("hook-handlers.toml")).unwrap(),
        hook
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join("file-handlers.toml")).unwrap(),
        file
    );
}
#[test]
fn initial_registry_read_errors_name_files_and_preserve_unreadable_originals() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "initial_registry_read_errors_name_files_and_preserve_unreadable_originals",
    ) {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    for file in ["hook-handlers.toml", "file-handlers.toml"] {
        let path = home.path().join(file);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("original"), "READ_PRIVATE_SENTINEL_082").unwrap();
    }
    let logs = warnings(|| load(home.path()));
    assert_both(&logs, "file could not be read");
    assert!(!logs.contains("PRIVATE_SENTINEL"));
    for file in ["hook-handlers.toml", "file-handlers.toml"] {
        assert_eq!(
            std::fs::read_to_string(home.path().join(file).join("original")).unwrap(),
            "READ_PRIVATE_SENTINEL_082"
        );
    }
}
#[test]
fn missing_registry_files_use_defaults_without_false_load_warnings() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "missing_registry_files_use_defaults_without_false_load_warnings",
    ) {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    assert!(warnings(|| load(home.path())).is_empty());
    assert!(!home.path().join("hook-handlers.toml").exists());
    assert!(!home.path().join("file-handlers.toml").exists());
}
