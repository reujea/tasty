//! 매니페스트의 `min_tasty_version`과 실행 중인 Tasty 본체 버전의 비교.
//!
//! 판정은 [`check_min_tasty_version_source`]가 매니페스트 원문에서 이 키만 꺼내서 한다.
//! 타입 역직렬화·선언 검사보다 먼저 하므로, 이 판이 모르는 계약을 쓰는 매니페스트도
//! "더 새 Tasty가 필요하다"는 사유로 거절된다. 플러그인을 받아들이는 자리는
//! [`Manifest::load_for_host`]·[`Manifest::admit_host_tasty`]를 쓴다.
//! 설치본 버전 읽기처럼 읽기만 하는 경로는 [`Manifest::load`]를 그대로 쓴다.

use std::path::Path;
use std::sync::OnceLock;

use super::types::Manifest;

/// 본체 버전이 매니페스트 요구보다 낮을 때 받아들이는 자리가 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionShortfall {
    /// 거절한다.
    Refuse,
    /// 두 버전을 담은 경고를 남기고 받아들인다.
    Warn,
}

/// 실행 중인 Tasty 본체 버전과 부족 처리. 본체가 시작할 때 정한다.
/// 이 크레이트의 `CARGO_PKG_VERSION`은 본체 버전이 아니어서 호출자가 넘긴다.
static HOST: OnceLock<(semver::Version, VersionShortfall)> = OnceLock::new();

/// 본체 버전과 부족 처리를 정한다. prerelease·build 꼬리는 떼고 숫자 셋만 쓴다.
/// 같은 값으로 다시 부르면 그대로 성공하고, 다른 값이면 오류다.
pub fn set_host_tasty_version(version: &str, shortfall: VersionShortfall) -> anyhow::Result<()> {
    let parsed = semver::Version::parse(version)
        .map_err(|e| anyhow::anyhow!("invalid host Tasty version '{version}': {e}"))?;
    let value = (
        semver::Version::new(parsed.major, parsed.minor, parsed.patch),
        shortfall,
    );
    let stored = HOST.get_or_init(|| value.clone());
    if *stored != value {
        anyhow::bail!(
            "host Tasty version is already set to {} ({:?})",
            stored.0,
            stored.1
        );
    }
    Ok(())
}

/// 정해진 본체 버전. 본체가 아닌 소비자(라이브러리 시험 등)에서는 `None`이다.
pub fn host_tasty_version() -> Option<&'static semver::Version> {
    HOST.get().map(|(version, _)| version)
}

/// `MAJOR.MINOR.PATCH`(10진 숫자 셋, prerelease·build 없음)만 받는다.
pub(crate) fn parse_release_version(value: &str) -> Option<semver::Version> {
    let version = semver::Version::parse(value).ok()?;
    (version.pre.is_empty() && version.build.is_empty()).then_some(version)
}

/// 매니페스트가 요구한 Tasty 버전보다 실행 중인 본체가 낮다.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("requires Tasty {required} or newer (this is Tasty {current})")]
pub struct TastyVersionTooOld {
    pub required: String,
    pub current: String,
}

fn format_error(value: &str) -> anyhow::Error {
    anyhow::anyhow!("invalid min_tasty_version '{value}': expected MAJOR.MINOR.PATCH")
}

/// 매니페스트 원문 `source`의 최상위 `min_tasty_version`을 `host`와 비교한다.
/// 키가 없거나 원문이 TOML이 아니면 통과한다(그 오류는 뒤의 load가 알린다).
/// 형식이 틀리면 형식 오류, 본체가 낮으면 [`TastyVersionTooOld`]다.
pub fn check_min_tasty_version_source(source: &str, host: &semver::Version) -> anyhow::Result<()> {
    let Ok(table) = source.parse::<toml::Table>() else {
        return Ok(());
    };
    let Some(value) = table.get("min_tasty_version") else {
        return Ok(());
    };
    let Some(text) = value.as_str() else {
        return Err(format_error(&value.to_string()));
    };
    let required = parse_release_version(text).ok_or_else(|| format_error(text))?;
    if (host.major, host.minor, host.patch) < (required.major, required.minor, required.patch) {
        return Err(TastyVersionTooOld {
            required: text.to_string(),
            current: format!("{}.{}.{}", host.major, host.minor, host.patch),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// 이 스레드에서 매니페스트 원문을 읽은 횟수. 한 번 읽기 계약을 시험이 잰다.
    pub(crate) static SOURCE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 매니페스트 원문 읽기. 판정과 역직렬화가 같은 내용을 쓰는지 시험이 셀 수 있게 한곳에 둔다.
pub(crate) fn read_source(path: &Path) -> std::io::Result<String> {
    #[cfg(test)]
    SOURCE_READS.with(|n| n.set(n.get() + 1));
    std::fs::read_to_string(path)
}

impl Manifest {
    /// `min_tasty_version`의 형식만 본다. [`Manifest::validate`]가 부른다.
    pub(crate) fn validate_min_tasty_version(&self) -> anyhow::Result<()> {
        match &self.min_tasty_version {
            Some(value) if parse_release_version(value).is_none() => Err(format_error(value)),
            _ => Ok(()),
        }
    }

    /// 플러그인을 받아들이는 자리의 첫 단계. `dir`의 매니페스트 원문으로 최소 버전을 판정하고,
    /// 정해진 부족 처리를 적용한다. 본체 버전이 정해지지 않았거나 파일을 읽지 못하면 통과한다
    /// (읽기 오류는 뒤의 load가 알린다).
    pub fn admit_host_tasty(dir: &Path) -> anyhow::Result<()> {
        match read_source(&dir.join("tasty-plugin.toml")) {
            Ok(source) => Self::admit_host_tasty_source(&source, dir),
            Err(_) => Ok(()),
        }
    }

    /// 공식 번들을 복사하기 전의 판정. [`Manifest::admit_host_tasty`]에 더해
    /// TOML로 읽히지 않는 매니페스트도 거절한다(복사한 뒤에야 load가 알리면 늦다).
    pub fn admit_bundle_host_tasty(dir: &Path) -> anyhow::Result<()> {
        let path = dir.join("tasty-plugin.toml");
        let Ok(source) = read_source(&path) else {
            return Ok(());
        };
        source
            .parse::<toml::Table>()
            .map_err(|e| anyhow::anyhow!("invalid manifest at {}: {e}", path.display()))?;
        Self::admit_host_tasty_source(&source, dir)
    }

    /// 원문 `source`로 최소 버전을 판정하고 정해진 부족 처리를 적용한다. `dir`은 경고에만 쓴다.
    fn admit_host_tasty_source(source: &str, dir: &Path) -> anyhow::Result<()> {
        let Some((host, shortfall)) = HOST.get() else {
            return Ok(());
        };
        match check_min_tasty_version_source(source, host) {
            Err(error)
                if *shortfall == VersionShortfall::Warn && error.is::<TastyVersionTooOld>() =>
            {
                tracing::warn!("plugin at {} loaded anyway: {error}", dir.display());
                Ok(())
            }
            other => other,
        }
    }

    /// 매니페스트를 한 번 읽어, 그 원문으로 [`Manifest::admit_host_tasty`]와 같은 판정을 한 뒤
    /// 같은 원문을 [`Manifest::load`]와 같게 역직렬화·검사한다.
    pub fn load_for_host(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("tasty-plugin.toml");
        let source = read_source(&path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {}", path.display(), e))?;
        Self::admit_host_tasty_source(&source, dir)?;
        let manifest = Self::parse_source(&source, &path)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// `path`에서 읽은 원문을 역직렬화만 한다. [`Manifest::read`]와 같은 오류 문구를 쓴다.
    pub(crate) fn parse_source(source: &str, path: &Path) -> anyhow::Result<Self> {
        toml::from_str(source)
            .map_err(|e| anyhow::anyhow!("invalid manifest at {}: {}", path.display(), e))
    }
}
