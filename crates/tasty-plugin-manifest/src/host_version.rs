//! 매니페스트의 `min_tasty_version`과 실행 중인 Tasty 본체 버전의 비교.
//!
//! 형식은 [`Manifest::validate`]가 늘 검사한다. 비교는 플러그인을 받아들이는 자리
//! (탐색·설치·CLI 명령 적재)가 [`Manifest::require_host_tasty`]로 따로 한다.
//! 번들 버전 읽기처럼 매니페스트를 읽기만 하는 경로는 비교하지 않는다.

use std::sync::OnceLock;

use super::types::Manifest;

/// 실행 중인 Tasty 본체 버전. 본체가 시작할 때 한 번 정한다.
/// 이 크레이트의 `CARGO_PKG_VERSION`은 본체 버전이 아니어서 호출자가 넘긴다.
static HOST_TASTY_VERSION: OnceLock<semver::Version> = OnceLock::new();

/// 본체 버전을 정한다. 두 번째 호출과 `MAJOR.MINOR.PATCH`가 아닌 값은 오류다.
pub fn set_host_tasty_version(version: &str) -> anyhow::Result<()> {
    let parsed = parse_release_version(version)
        .ok_or_else(|| anyhow::anyhow!("invalid host Tasty version '{version}'"))?;
    HOST_TASTY_VERSION
        .set(parsed)
        .map_err(|_| anyhow::anyhow!("host Tasty version is already set"))
}

/// 정해진 본체 버전. 본체가 아닌 소비자(라이브러리 시험 등)에서는 `None`이다.
pub fn host_tasty_version() -> Option<&'static semver::Version> {
    HOST_TASTY_VERSION.get()
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

impl Manifest {
    /// `min_tasty_version`의 형식만 본다. [`Manifest::validate`]가 부른다.
    pub(crate) fn validate_min_tasty_version(&self) -> anyhow::Result<()> {
        if let Some(value) = &self.min_tasty_version
            && parse_release_version(value).is_none()
        {
            anyhow::bail!("invalid min_tasty_version '{value}': expected MAJOR.MINOR.PATCH");
        }
        Ok(())
    }

    /// `host`가 `min_tasty_version`보다 낮으면 [`TastyVersionTooOld`]로 거절한다.
    /// 필드가 없으면 통과하고, 형식이 틀린 값은 형식 오류로 거절한다.
    pub fn check_min_tasty_version(&self, host: &semver::Version) -> anyhow::Result<()> {
        let Some(value) = &self.min_tasty_version else {
            return Ok(());
        };
        let Some(required) = parse_release_version(value) else {
            anyhow::bail!("invalid min_tasty_version '{value}': expected MAJOR.MINOR.PATCH");
        };
        let host_triple = (host.major, host.minor, host.patch);
        if host_triple < (required.major, required.minor, required.patch) {
            return Err(TastyVersionTooOld {
                required: value.clone(),
                current: format!("{}.{}.{}", host.major, host.minor, host.patch),
            }
            .into());
        }
        Ok(())
    }

    /// 실행 중인 본체 버전으로 [`Manifest::check_min_tasty_version`]을 한다.
    /// 본체 버전이 정해지지 않은 소비자에서는 비교하지 않는다.
    pub fn require_host_tasty(&self) -> anyhow::Result<()> {
        if let Some(host) = host_tasty_version() {
            self.check_min_tasty_version(host)?;
        }
        Ok(())
    }
}
