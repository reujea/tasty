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
        let Some((host, shortfall)) = HOST.get() else {
            return Ok(());
        };
        let Ok(source) = std::fs::read_to_string(dir.join("tasty-plugin.toml")) else {
            return Ok(());
        };
        match check_min_tasty_version_source(&source, host) {
            Err(error)
                if *shortfall == VersionShortfall::Warn && error.is::<TastyVersionTooOld>() =>
            {
                tracing::warn!("plugin at {} loaded anyway: {error}", dir.display());
                Ok(())
            }
            other => other,
        }
    }

    /// [`Manifest::admit_host_tasty`]를 통과한 뒤 [`Manifest::load`]한다.
    pub fn load_for_host(dir: &Path) -> anyhow::Result<Self> {
        Self::admit_host_tasty(dir)?;
        Self::load(dir)
    }
}
