//! 이 프로세스가 소유한 인스턴스 파일 `tasty.instance`의 수명. 잠금을 얻은 직후 쓰고, IPC 서버가 열리면
//! 포트를 넣어 다시 쓰고, 정상 종료할 때 지운다. 파일 형식과 남은 파일 판별은 CLI와 함께 쓰도록
//! `tasty_ipc::instance_file`에 있다.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) use tasty_ipc::instance_file::{
    InstanceRecord, InstanceState, path, process_start_time, read_state, write,
};

/// 이 프로세스가 쓴 기록. 포트를 넣어 다시 쓰고 종료할 때 지우는 데 쓴다.
static OWNED: Mutex<Option<(PathBuf, InstanceRecord)>> = Mutex::new(None);

fn with_owned(f: impl FnOnce(&mut Option<(PathBuf, InstanceRecord)>)) {
    match OWNED.lock() {
        Ok(mut owned) => f(&mut owned),
        Err(e) => tracing::warn!("instance file state poisoned: {e}"),
    }
}

/// 잠금을 얻은 직후 포트 없는 기록을 쓴다. 실패는 로그만 남긴다. 두 번째 프로세스는 부팅 중으로 보고 기다린다.
pub(crate) fn publish_started(home: &Path) {
    let pid = std::process::id();
    let Some(start_time) = process_start_time(pid) else {
        tracing::warn!("instance file: cannot read this process start time; not publishing");
        return;
    };
    let record = InstanceRecord {
        pid,
        start_time,
        port: None,
    };
    if let Err(e) = write(home, &record) {
        tracing::warn!("instance file: write {} failed: {e}", path(home).display());
        return;
    }
    with_owned(|owned| *owned = Some((home.to_path_buf(), record)));
}

/// IPC 서버가 열린 뒤 포트를 넣어 다시 쓴다. 먼저 쓴 기록이 없으면 아무것도 하지 않는다.
pub(crate) fn publish_port(port: u16) {
    with_owned(|owned| {
        let Some((home, record)) = owned.as_mut() else {
            return;
        };
        record.port = Some(port);
        if let Err(e) = write(home, record) {
            tracing::warn!("instance file: port update failed: {e}");
        }
    });
}

/// 정상 종료할 때 저널 잠금을 놓기 전에 지운다.
pub(crate) fn remove_owned() {
    with_owned(|owned| {
        if let Some((home, _)) = owned.take()
            && let Err(e) = std::fs::remove_file(path(&home))
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("instance file: remove failed: {e}");
        }
    });
}
