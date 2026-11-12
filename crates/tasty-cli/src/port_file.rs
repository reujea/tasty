//! PortFileError를 현재 로케일의 CLI 안내로 바꾼다. 모든 포트 조회가 같은 문구를 사용한다.

use anyhow::Result;
use tasty_i18n::{t, t_fmt};
use tasty_ipc::port_file::{self as pf, PortFileError};

/// 포트 파일에서 IPC 포트를 읽는다. 실패 문구는 `general.language` 를 따른다.
///
/// 반환은 단일 메시지 `anyhow::Error` 다 — `context` 로 원인을 체인하면 최상위
/// 출력이 `Caused by:` 블록까지 붙어 기존 한 줄 출력과 달라진다.
pub fn read_port(port_file: Option<&str>) -> Result<u16> {
    read_port_diagnosed(port_file).map_err(|e| anyhow::anyhow!("{}", localize(&e)))
}

/// 로그에 쓸 번역 전 오류를 반환한다. PortFileError의 Display는 영어 카탈로그와 일치해야 한다.
pub fn read_port_diagnosed(port_file: Option<&str>) -> std::result::Result<u16, PortFileError> {
    pf::read_port_file_from(port_file)
}

/// 포트 파일 옆 인스턴스 파일이 그 포트를 쓴 인스턴스가 끝났다고 말하면 그 기록. 판별할 수
/// 없으면(인스턴스 파일 없음 등) None이며 호출자는 기존 동작을 유지한다.
pub fn exited_owner(
    port_file: Option<&str>,
    port: u16,
) -> Option<tasty_ipc::instance_file::InstanceRecord> {
    let path = pf::resolve_port_file_path(port_file).ok()?;
    match pf::port_owner(&path, port) {
        tasty_ipc::instance_file::PortOwner::Exited(record) => Some(record),
        _ => None,
    }
}

/// 끝난 인스턴스가 남긴 포트 파일의 안내.
pub fn exited_message(port_file: Option<&str>, port: u16, pid: u32) -> String {
    let path = pf::resolve_port_file_path(port_file)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    tasty_i18n::t_args(
        "cli.port_file.instance_exited",
        &[&path, &port.to_string(), &pid.to_string()],
    )
}

/// 실패 조건 → 현재 로케일 문장. en 값은 [`PortFileError`] 의 기본 렌더링과
/// 문자 단위로 같아야 한다(아래 테스트가 강제).
pub(crate) fn localize(err: &PortFileError) -> String {
    match err {
        PortFileError::HomeUnresolved => t("cli.port_file.home_unresolved").to_string(),
        PortFileError::NotFound { path } => {
            t_fmt("cli.port_file.no_instance", &path.display().to_string())
        }
        PortFileError::Invalid => t("cli.port_file.invalid_contents").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_lang_values_match_the_crate_default_rendering() {
        tasty_i18n::init("en");

        let path = std::path::PathBuf::from("/home/u/.tasty/tasty.port");
        assert_eq!(
            localize(&PortFileError::NotFound { path: path.clone() }),
            PortFileError::NotFound { path }.to_string()
        );
        assert_eq!(
            localize(&PortFileError::HomeUnresolved),
            PortFileError::HomeUnresolved.to_string()
        );
        assert_eq!(
            localize(&PortFileError::Invalid),
            PortFileError::Invalid.to_string()
        );
    }

    /// 포트 파일과 같은 포트를 적은 인스턴스 기록의 PID가 다른 프로세스면 끝난 인스턴스다.
    /// 인스턴스 파일이 없으면 판별하지 않는다.
    #[test]
    fn a_port_file_next_to_a_stale_instance_record_is_reported_as_exited() {
        use tasty_ipc::instance_file::{self, InstanceRecord};
        let home = tempfile::tempdir().unwrap();
        let port_file = home.path().join("tasty.port");
        std::fs::write(&port_file, "4321").unwrap();
        let port_file = port_file.to_str().unwrap();
        assert_eq!(exited_owner(Some(port_file), 4321), None);
        let pid = std::process::id();
        let start = instance_file::process_start_time(pid).unwrap();
        let stale = InstanceRecord {
            pid,
            start_time: start.wrapping_add(1),
            port: Some(4321),
        };
        instance_file::write(home.path(), &stale).unwrap();
        assert_eq!(exited_owner(Some(port_file), 4321), Some(stale));
        let live = InstanceRecord {
            start_time: start,
            ..stale
        };
        instance_file::write(home.path(), &live).unwrap();
        assert_eq!(exited_owner(Some(port_file), 4321), None);
    }

    /// 격리 홈의 경로도 안내에 그대로 포함한다.
    #[test]
    fn the_searched_path_is_substituted_into_the_message() {
        tasty_i18n::init("en");

        let msg = localize(&PortFileError::NotFound {
            path: std::path::PathBuf::from("/isolated/home/tasty.port"),
        });
        assert!(msg.contains("/isolated/home/tasty.port"), "{msg}");
        assert!(!msg.contains("{}"), "치환 자리가 남았다: {msg}");
    }
}
