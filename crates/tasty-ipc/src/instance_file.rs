//! 홈의 인스턴스 파일 `tasty.instance`. 잠금을 얻은 인스턴스가 PID·프로세스 시작 시각·IPC 포트를 적는다.
//!
//! 두 번째 프로세스가 실행 중인 인스턴스를 찾을 때 읽는다. 잠금을 가졌다고 파일이 최신인 것은 아니다.
//! 이전 소유자가 비정상 종료하면 파일이 남으므로, 읽는 쪽은 PID가 살아 있고 그 프로세스의 시작
//! 시각이 파일 값과 같은지 확인한다(PID 재사용도 이 검증으로 걸러진다). 쓰기는 임시 파일 + rename이다.
//! 잠금 직후에는 포트가 없고, IPC 서버가 열린 뒤 포트를 넣어 다시 쓴다. Windows는 잠긴 파일을 다른
//! 프로세스가 읽을 수 없으므로 잠금 파일과 따로 둔다.
//!
//! 본체의 부팅 경로와 CLI가 같은 형식과 판별을 쓰도록 이 크레이트에 둔다. 기록을 소유하고 갱신하는
//! 수명 관리는 본체(`boot::single_instance::instance_file`)에 있다.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 홈 안의 인스턴스 파일 이름.
pub const FILE_NAME: &str = "tasty.instance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub pid: u32,
    /// OS가 보고한 프로세스 시작 시각. 단위는 OS마다 다르며 같은 OS 안에서 비교만 한다.
    pub start_time: u64,
    /// IPC 포트. 아직 IPC 서버를 열지 않았으면 없다.
    pub port: Option<u16>,
}

/// 두 번째 프로세스가 본 실행 중 인스턴스의 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceState {
    /// 파일이 있고 PID·시작 시각이 맞으며 포트도 있다.
    Live(InstanceRecord),
    /// 아직 부팅 중이다. 파일이 없거나, 포트가 없거나, 낡은 기록이다(새 소유자가 아직 쓰지 않았다).
    Booting(BootingReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootingReason {
    NoFile,
    NoPort(InstanceRecord),
    /// 기록한 PID가 없거나 시작 시각이 다르다.
    Stale {
        record: InstanceRecord,
        observed_start: Option<u64>,
    },
    Unreadable(String),
}

pub fn path(home: &Path) -> PathBuf {
    home.join(FILE_NAME)
}

/// 읽은 기록과 그 PID의 실제 시작 시각으로 상태를 정한다.
pub fn classify(record: InstanceRecord, observed_start: Option<u64>) -> InstanceState {
    if observed_start != Some(record.start_time) {
        return InstanceState::Booting(BootingReason::Stale {
            record,
            observed_start,
        });
    }
    if record.port.is_none() {
        return InstanceState::Booting(BootingReason::NoPort(record));
    }
    InstanceState::Live(record)
}

pub fn read_state(home: &Path) -> InstanceState {
    let bytes = match std::fs::read(path(home)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return InstanceState::Booting(BootingReason::NoFile);
        }
        Err(e) => return InstanceState::Booting(BootingReason::Unreadable(e.to_string())),
    };
    match serde_json::from_slice::<InstanceRecord>(&bytes) {
        Ok(record) => classify(record, process_start_time(record.pid)),
        Err(e) => InstanceState::Booting(BootingReason::Unreadable(e.to_string())),
    }
}

/// 포트 파일의 포트를 그 홈의 인스턴스 파일과 대조한 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortOwner {
    /// 같은 포트를 적은 기록의 PID·시작 시각이 맞다.
    Live,
    /// 같은 포트를 적은 기록이 있으나 그 PID가 없거나 다른 프로세스다. 포트 파일도 남은 것이다.
    Exited(InstanceRecord),
    /// 판별할 수 없다. 인스턴스 파일이 없거나(옛 버전·다른 위치의 포트 파일), 읽지 못했거나,
    /// 다른 포트를 적었거나, 아직 포트를 적지 않았다.
    Unknown,
}

/// 인스턴스 상태와 포트 파일의 포트로 그 포트의 주인을 정한다. 기록의 포트가 다르면 두 파일이
/// 같은 실행에서 나왔다고 볼 수 없으므로 판별하지 않는다.
pub fn port_owner(state: &InstanceState, port: u16) -> PortOwner {
    match state {
        InstanceState::Live(record) if record.port == Some(port) => PortOwner::Live,
        InstanceState::Booting(BootingReason::Stale { record, .. })
            if record.port == Some(port) =>
        {
            PortOwner::Exited(*record)
        }
        _ => PortOwner::Unknown,
    }
}

/// `home`의 인스턴스 파일을 읽어 [`port_owner`]로 판별한다.
pub fn port_owner_in(home: &Path, port: u16) -> PortOwner {
    port_owner(&read_state(home), port)
}

/// 기록을 임시 파일 + rename으로 쓴다.
pub fn write(home: &Path, record: &InstanceRecord) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(record).map_err(std::io::Error::other)?;
    let mut temporary = tempfile::NamedTempFile::new_in(home)?;
    std::io::Write::write_all(&mut temporary, &bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path(home)).map_err(|e| e.error)?;
    Ok(())
}

/// PID의 프로세스 시작 시각. 프로세스가 없거나 읽을 수 없으면 None.
/// 플랫폼별 조회는 `tasty_agent::platform::process_start`에 있다.
pub fn process_start_time(pid: u32) -> Option<u64> {
    tasty_agent::platform::process_start::start_time(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(port: Option<u16>) -> InstanceRecord {
        InstanceRecord {
            pid: 10,
            start_time: 500,
            port,
        }
    }

    #[test]
    fn a_record_without_a_port_is_still_booting() {
        assert_eq!(
            classify(record(None), Some(500)),
            InstanceState::Booting(BootingReason::NoPort(record(None)))
        );
    }

    #[test]
    fn a_matching_pid_and_start_time_with_a_port_is_live() {
        assert_eq!(
            classify(record(Some(4000)), Some(500)),
            InstanceState::Live(record(Some(4000)))
        );
    }

    #[test]
    fn a_dead_or_reused_pid_is_stale() {
        for observed in [None, Some(501)] {
            assert!(matches!(
                classify(record(Some(4000)), observed),
                InstanceState::Booting(BootingReason::Stale { .. })
            ));
        }
    }

    #[test]
    fn only_a_stale_record_with_the_same_port_marks_the_port_as_exited() {
        let live = classify(record(Some(4000)), Some(500));
        assert_eq!(port_owner(&live, 4000), PortOwner::Live);
        assert_eq!(port_owner(&live, 4001), PortOwner::Unknown);
        for observed in [None, Some(501)] {
            let stale = classify(record(Some(4000)), observed);
            assert_eq!(
                port_owner(&stale, 4000),
                PortOwner::Exited(record(Some(4000)))
            );
            assert_eq!(port_owner(&stale, 4001), PortOwner::Unknown);
        }
        let no_port = classify(record(None), Some(500));
        assert_eq!(port_owner(&no_port, 4000), PortOwner::Unknown);
        assert_eq!(
            port_owner(&InstanceState::Booting(BootingReason::NoFile), 4000),
            PortOwner::Unknown
        );
    }

    /// 살아 있는 PID라도 시작 시각이 다르면(PID 재사용) 끝난 인스턴스로 본다.
    #[test]
    fn a_reused_live_pid_reads_back_as_exited() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(port_owner_in(home.path(), 4000), PortOwner::Unknown);
        let pid = std::process::id();
        let start_time = process_start_time(pid).expect("own start time");
        let reused = InstanceRecord {
            pid,
            start_time: start_time.wrapping_add(1),
            port: Some(4000),
        };
        write(home.path(), &reused).unwrap();
        assert_eq!(port_owner_in(home.path(), 4000), PortOwner::Exited(reused));
        let live = InstanceRecord {
            start_time,
            ..reused
        };
        write(home.path(), &live).unwrap();
        assert_eq!(port_owner_in(home.path(), 4000), PortOwner::Live);
    }

    #[test]
    fn this_process_has_a_start_time_and_its_record_reads_back_live() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            read_state(home.path()),
            InstanceState::Booting(BootingReason::NoFile)
        );
        let pid = std::process::id();
        let start_time = process_start_time(pid).expect("own start time");
        let live = InstanceRecord {
            pid,
            start_time,
            port: Some(1234),
        };
        write(home.path(), &live).unwrap();
        assert_eq!(read_state(home.path()), InstanceState::Live(live));
        std::fs::write(path(home.path()), b"not json").unwrap();
        assert!(matches!(
            read_state(home.path()),
            InstanceState::Booting(BootingReason::Unreadable(_))
        ));
    }
}
