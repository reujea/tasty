//! 시험이 프로세스 종료를 확인하는 판정. 한 번 true가 되면 false로 돌아가지 않는다.

/// 끝났거나 회수를 기다리거나 회수되는 중이다. `/proc`가 없는 macOS는 `kill(pid, 0)`으로 보므로
/// 좀비를 남았다고 본다.
pub(crate) fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => tasty_test_support::proc_stat_reads_finished(&stat),
        Err(_) => !tasty_agent::platform::process_alive::is_alive(pid),
    }
}

#[test]
fn a_running_process_is_not_gone_and_a_reaped_one_is() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep");
    assert!(!gone(child.id()));
    child.kill().expect("kill sleep");
    child.wait().expect("reap sleep");
    assert!(gone(child.id()));
}
