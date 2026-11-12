//! 원격 포트 발견의 첫 단계인 tasty port를 처리한다. IPC 연결 없이 포트 파일을 읽는다.
//! Windows GUI release의 비-PTY PowerShell/cmd에서는 stdout을 받지 못할 수 있어
//! auto 발견은 file-unix/file-windows fallback도 사용한다(crate::ssh::discover_remote_port).

use anyhow::Result;

use crate::out::outln;

/// tasty.port의 포트를 출력한다. 기본 루트는 release ~/.tasty/, debug ~/.tasty-debug/다.
/// --port-file로 별도 파일을 지정할 수 있다. 포트 파일 옆 인스턴스 파일이 그 포트의 인스턴스가
/// 끝났다고 말하면 포트를 출력하지 않고 tasty_ssh::PORT_COMMAND_EXITED_INSTANCE_CODE로 끝낸다.
pub fn run_port(port_file: Option<&str>) -> Result<()> {
    let port = crate::port_file::read_port(port_file)?;
    if let Some(record) = crate::port_file::exited_owner(port_file, port) {
        crate::out::errln!(
            "{}",
            crate::port_file::exited_message(port_file, port, record.pid)
        );
        std::process::exit(tasty_ssh::PORT_COMMAND_EXITED_INSTANCE_CODE);
    }
    outln!("{port}")?;
    Ok(())
}

/// Synchronous standalone participant. Returns after the shared webhooks.toml guard drops;
/// caller-side output/publication must follow this result rather than run under the lock.
pub fn save_webhook_port_at(path: &std::path::Path, port: Option<u16>) -> std::io::Result<()> {
    match port {
        Some(port) => tasty_settings::webhook_port_file::set_port(path, port),
        None => tasty_settings::webhook_port_file::clear_port(path),
    }
}

#[cfg(test)]
mod webhook_file_tests {
    use super::*;

    /// Owned cross-process file fixture; no IPC, listener or real user port is used.
    #[test]
    fn file_participant_child() {
        if std::env::var_os("TASTY_WEBHOOK_FILE_CHILD").as_deref()
            != Some(std::ffi::OsStr::new("port"))
        {
            return;
        }
        let path = std::path::PathBuf::from(std::env::var_os("TASTY_WEBHOOK_FILE_PATH").unwrap());
        let ready = std::path::PathBuf::from(std::env::var_os("TASTY_WEBHOOK_FILE_READY").unwrap());
        std::fs::write(ready, b"ready").unwrap();
        for n in 0..16 {
            assert!(
                save_webhook_port_at(&path, Some(40000 + n)).is_ok(),
                "offline port mutation failed"
            );
        }
    }
    /// The final try_lock fails when a child spawned by another test in this binary inherits the
    /// lock file description between fork and exec. This test therefore reruns itself alone in a
    /// child process with one test thread; the parent only checks that the child passed.
    #[test]
    fn actual_offline_adapter_preserves_webhook_and_releases_lock_before_return() {
        const ISOLATED: &str = "TASTY_CLI_WEBHOOK_LOCK_ISOLATED";
        if std::env::var_os(ISOLATED).is_none() {
            let name = "local::port::webhook_file_tests::actual_offline_adapter_preserves_webhook_and_releases_lock_before_return";
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--test-threads=1"])
                .env(ISOLATED, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            // "1 passed" also rejects a misspelled name, which would filter out every test.
            assert!(
                output.status.success() && stdout.contains("test result: ok. 1 passed"),
                "isolated run failed: {}\nstdout:\n{stdout}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("webhooks.toml");
        std::fs::write(
            &path,
            "format=2\n[[webhook]]\nid='fixture'\n[webhook.auth]\ntoken='isolated-cli-fixture'\n",
        )
        .unwrap();
        save_webhook_port_at(&path, Some(41000)).unwrap();
        let table = tasty_settings::webhook_port_file::read_table(&path).unwrap();
        assert_eq!(table["port"].as_integer(), Some(41000));
        assert!(table["webhook"][0]["auth"]["token"].as_str() == Some("isolated-cli-fixture"));
        save_webhook_port_at(&path, None).unwrap();
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("webhooks.toml.lock"))
            .unwrap();
        assert!(lock_file.try_lock().is_ok());
    }
    #[test]
    fn actual_offline_adapter_does_not_replace_malformed_original_or_expose_its_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("webhooks.toml");
        let original = "token='isolated-cli-fixture'\nbroken=[";
        std::fs::write(&path, original).unwrap();
        let error = save_webhook_port_at(&path, Some(41000)).unwrap_err();
        assert!(!error.to_string().contains("isolated-cli-fixture"));
        assert!(save_webhook_port_at(&path, None).is_err());
        assert!(std::fs::read_to_string(&path).unwrap() == original);
    }
}
