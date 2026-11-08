//! 호스트 프로세스가 SIGTERM으로 끝나면 `spawn_bound_to_host`로 띄운 자식도 끝나는지 확인한다.
//! 이 시험 바이너리를 호스트 역할로 다시 실행하고, 그 호스트에 SIGTERM을 보낸다.
#![cfg(target_os = "linux")]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// 호스트 역할로 실행할 때 자식 PID를 적을 파일 경로.
const HOST_ENV: &str = "TASTY_REAPER_BOUND_HOST";

fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !tasty_test_support::proc_stat_reads_finished(&stat),
        Err(_) => false,
    }
}

fn wait_gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn act_as_host(pid_file: &std::ffi::OsStr) {
    // 짧게 사는 스레드에서 실행해도 그 스레드의 종료로 자식이 끝나지 않아야 한다.
    let mut child = std::thread::spawn(|| {
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        tasty_reaper::spawn_bound_to_host(cmd).expect("spawn sleep")
    })
    .join()
    .expect("spawn thread");
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(pid_file, child.id().to_string()).expect("report the bound child");
    // 보통은 여기까지 오기 전에 SIGTERM을 받는다.
    std::thread::sleep(Duration::from_secs(60));
    child.kill().expect("stop the bound child");
    child.wait().expect("reap the bound child");
}

#[test]
fn a_bound_child_ends_with_its_host() {
    if let Some(pid_file) = std::env::var_os(HOST_ENV) {
        act_as_host(&pid_file);
        return;
    }
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    let pid_file = std::env::temp_dir().join(format!(
        "tasty-reaper-bound-{}-{call}.pid",
        std::process::id()
    ));
    if pid_file.exists() {
        std::fs::remove_file(&pid_file).expect("clear a stale pid file");
    }
    // 호스트의 stdout·stderr 는 파일로 모은다. 부모 시험의 출력에 호스트의 줄이 섞이지 않게 한다.
    let host_log = pid_file.with_extension("out");
    let out = std::fs::File::create(&host_log).expect("host log");
    let mut host = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "a_bound_child_ends_with_its_host"])
        .env(HOST_ENV, &pid_file)
        .stdout(out.try_clone().expect("host log handle"))
        .stderr(out)
        .spawn()
        .expect("host process");
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid: u32 = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = text.trim().parse()
        {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "host never reported its child\nhost output:\n{}",
            std::fs::read_to_string(&host_log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    std::fs::remove_file(&pid_file).expect("remove the pid file");
    assert!(
        alive(pid),
        "the bound child outlives the short spawning thread"
    );
    let status = Command::new("kill")
        .args(["-TERM", &host.id().to_string()])
        .status()
        .expect("kill");
    assert!(status.success());
    host.wait().expect("host exit");
    std::fs::remove_file(&host_log).expect("remove the host log");
    let gone = wait_gone(pid);
    if !gone {
        // 시험이 실패해도 고아를 남기지 않는다.
        Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()
            .expect("stop the leftover child");
    }
    assert!(gone, "bound child {pid} survived its host");
}
