//! Linux 단일 실행의 D-Bus 경로. 실행 중인 release Tasty가 세션 버스에 이름을 등록하고
//! `org.freedesktop.Application.Activate`를 받는다. 두 번째 프로세스는 실행기가 준 증거를
//! `platform_data`에 실어 이 메서드를 부른다.
//!
//! 이름은 데이터 홈마다 하나다. 기본 홈(`~/.tasty`)은 `io.github.zilhak.tasty`, 다른 홈은 정규화한
//! 경로의 해시를 붙인 `io.github.zilhak.tasty.h<16진>`이다. 기본 홈인지는 `TASTY_HOME` 설정 여부가
//! 아니라 정규화한 실제 경로로 판정한다. 상대 경로·심볼릭 링크·기본 홈을 명시한 경우가 같은 홈을
//! 가리킬 수 있기 때문이다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use super::evidence::LaunchEvidence;

pub(crate) const BASE_NAME: &str = "io.github.zilhak.tasty";
pub(crate) const OBJECT_PATH: &str = "/io/github/zilhak/tasty";
const APPLICATION_INTERFACE: &str = "org.freedesktop.Application";

/// release 기본 홈. debug 빌드에서도 같은 규칙으로 계산한다(이름 규칙 시험용).
fn default_release_home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".tasty"))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// FNV-1a 64. 버전이 다른 Tasty끼리도 같은 이름을 계산하도록 고정된 해시를 쓴다.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 정규화한 홈과 정규화한 기본 홈으로 버스 이름을 정한다.
pub(crate) fn bus_name_for(home: &Path, default_home: Option<&Path>) -> String {
    if default_home.is_some_and(|d| d == home) {
        return BASE_NAME.to_string();
    }
    use std::os::unix::ffi::OsStrExt;
    // 이름의 각 요소는 숫자로 시작할 수 없으므로 접두 문자 h를 붙인다.
    format!("{BASE_NAME}.h{:016x}", fnv1a64(home.as_os_str().as_bytes()))
}

pub(crate) fn bus_name(home: &Path) -> String {
    let default_home = default_release_home().map(|d| canonical(&d));
    bus_name_for(&canonical(home), default_home.as_deref())
}

/// `platform_data`에서 문자열 값만 꺼낸다.
fn string_entries(
    platform_data: &HashMap<String, zbus::OwnedVariant>,
) -> Vec<(&str, Option<String>)> {
    platform_data
        .iter()
        .map(|(key, value)| {
            let text = match &**value {
                zbus::Variant::Str(s) => Some(s.to_string()),
                _ => None,
            };
            (key.as_str(), text)
        })
        .collect()
}

type ActivationSink = Box<dyn Fn(LaunchEvidence) + Send + Sync>;

struct ApplicationService {
    sink: ActivationSink,
}

#[zbus::interface(name = "org.freedesktop.Application")]
impl ApplicationService {
    /// 증거 키가 없어도 이벤트는 보낸다. 받는 쪽이 증거가 없으면 아무것도 바꾸지 않는다.
    fn activate(&self, platform_data: HashMap<String, zbus::OwnedVariant>) {
        let evidence = LaunchEvidence::from_platform_data(string_entries(&platform_data));
        tracing::info!("D-Bus Activate received (evidence: {:?})", evidence.kinds());
        (self.sink)(evidence);
    }

    fn open(
        &self,
        _uris: Vec<String>,
        _platform_data: HashMap<String, zbus::OwnedVariant>,
    ) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported(
            "Tasty does not open files through D-Bus activation".into(),
        ))
    }

    fn activate_action(
        &self,
        _action_name: String,
        _parameter: Vec<zbus::OwnedVariant>,
        _platform_data: HashMap<String, zbus::OwnedVariant>,
    ) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported(
            "Tasty has no D-Bus application actions".into(),
        ))
    }
}

/// 서비스 연결을 프로세스 수명 동안 붙잡는다. 놓으면 이름도 풀린다.
static SERVICE: OnceLock<zbus::Connection> = OnceLock::new();

/// 이름을 등록하고 `Activate`를 `sink`로 넘긴다. 실패는 로그만 남긴다(단일 실행 활성화만 못 한다).
pub(crate) fn start_service(home: &Path, sink: ActivationSink) {
    let name = bus_name(home);
    match register(&name, sink) {
        Ok(connection) => {
            let fresh = SERVICE.set(connection).is_ok();
            tracing::info!("single instance: D-Bus name {name} registered (first: {fresh})");
        }
        Err(e) => tracing::warn!("single instance: D-Bus name {name} not registered: {e}"),
    }
}

/// `block_on`이 끝난 뒤에는 zbus의 helper 스레드가 살아 있는 연결의 메서드 호출을 처리한다.
fn register(name: &str, sink: ActivationSink) -> zbus::Result<zbus::Connection> {
    zbus::block_on(
        zbus::connection::Builder::session()
            .name(name)
            .serve_at(OBJECT_PATH, ApplicationService { sink })
            .build(),
    )
}

/// 두 번째 프로세스의 세션 버스 연결. 메서드 응답 기한을 둔다.
pub(crate) fn connect(method_timeout: Duration) -> zbus::Result<zbus::Connection> {
    zbus::block_on(
        zbus::connection::Builder::session()
            .method_timeout(method_timeout)
            .build(),
    )
}

pub(crate) fn name_has_owner(connection: &zbus::Connection, name: &str) -> zbus::Result<bool> {
    let bus_name = zbus::names::BusName::try_from(name)?;
    zbus::block_on(async {
        let dbus = zbus::fdo::DBusProxy::new(connection).await?;
        Ok(dbus.name_has_owner(bus_name).await?)
    })
}

pub(crate) fn activate(
    connection: &zbus::Connection,
    name: &str,
    evidence: &LaunchEvidence,
) -> zbus::Result<()> {
    let data = evidence.platform_data();
    let platform_data: HashMap<&str, zbus::Variant<'_>> = data
        .iter()
        .map(|(key, value)| (*key, zbus::Variant::from(value.as_str())))
        .collect();
    zbus::block_on(connection.call_method(
        Some(name),
        OBJECT_PATH,
        Some(APPLICATION_INTERFACE),
        "Activate",
        &(platform_data,),
    ))?;
    Ok(())
}

/// 데스크톱 알림. 메시지 상자를 띄우지 못했을 때 쓴다.
pub(crate) fn notify(summary: &str, body: &str) -> zbus::Result<()> {
    let connection = connect(Duration::from_secs(3))?;
    let hints: HashMap<&str, zbus::Variant<'_>> = HashMap::new();
    zbus::block_on(connection.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "Tasty",
            0u32,
            "",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_home_uses_the_base_name_and_others_get_a_hashed_suffix() {
        let default = Path::new("/home/u/.tasty");
        assert_eq!(bus_name_for(default, Some(default)), BASE_NAME);
        let other = bus_name_for(Path::new("/tmp/h1"), Some(default));
        assert!(other.starts_with("io.github.zilhak.tasty.h"), "{other}");
        assert_eq!(other.len(), BASE_NAME.len() + 2 + 16);
        assert_ne!(other, bus_name_for(Path::new("/tmp/h2"), Some(default)));
        assert_eq!(other, bus_name_for(Path::new("/tmp/h1"), None));
        // 버스 이름 문법을 만족한다.
        assert!(zbus::names::WellKnownName::try_from(other.as_str()).is_ok());
    }

    #[test]
    fn the_same_home_through_a_symlink_maps_to_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(bus_name(&real), bus_name(&link));
        assert_eq!(bus_name(&real), bus_name(&real.join("..").join("real")));
    }

    #[test]
    fn the_hash_is_fixed_across_builds() {
        // FNV-1a 64의 알려진 값.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn platform_data_strings_are_read_and_other_types_ignored() {
        let mut data = HashMap::new();
        data.insert(
            "desktop-startup-id".to_string(),
            zbus::OwnedVariant::try_from(zbus::Variant::from("id_TIME9")).unwrap(),
        );
        data.insert(
            "activation-token".to_string(),
            zbus::OwnedVariant::from(7u32),
        );
        let evidence = LaunchEvidence::from_platform_data(string_entries(&data));
        assert_eq!(evidence.x11_startup_id.as_deref(), Some("id_TIME9"));
        assert!(evidence.wayland_token.is_none());
    }
}
