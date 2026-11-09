//! `com.tasty.` id 이름 공간은 Tasty 공식 플러그인 전용이다.
//! 공식 id 목록은 번들 목록([`crate::builtin`]의 `BUILTINS`)과 따로 둔다. 번들에서 빠지고
//! 내려받아 설치하는 공식 플러그인도 이 목록으로 판정한다.
//! 이 판정은 id만 본다. 내용이 공식인지는 서명·신뢰 판정([`crate::trust_verdict`])이 정한다.
//! 규칙: `docs/principles/plugins.md`의 "인증과 설치 경고" 절.

use std::path::Path;

/// 공식 플러그인만 쓰는 id 접두.
pub const RESERVED_ID_PREFIX: &str = "com.tasty.";

/// Tasty가 아는 공식 플러그인 id. 새 공식 플러그인은 여기에 한 줄을 넣어야 설치·등록된다.
pub const OFFICIAL_PLUGIN_IDS: &[&str] = &[
    "com.tasty.agent-session",
    "com.tasty.agent-stream",
    "com.tasty.claude",
    "com.tasty.clipboard-viewer",
    "com.tasty.codex",
    "com.tasty.git-viewer",
    "com.tasty.html",
    "com.tasty.image",
    "com.tasty.markdown",
    "com.tasty.mesh-demo",
];

/// 공식 목록에 없는 `com.tasty.*` id. 서명 상태와 관계없이 설치·등록하지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedIdError {
    pub id: String,
}

impl std::fmt::Display for ReservedIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plugin id '{}' uses the reserved {RESERVED_ID_PREFIX} prefix but is not an official Tasty plugin",
            self.id
        )
    }
}

impl std::error::Error for ReservedIdError {}

/// `com.tasty.`로 시작하는데 공식 목록에 없으면 거부한다. 다른 id는 모두 통과한다.
pub fn admit_plugin_id(id: &str) -> Result<(), ReservedIdError> {
    if id.starts_with(RESERVED_ID_PREFIX) && !OFFICIAL_PLUGIN_IDS.contains(&id) {
        return Err(ReservedIdError { id: id.to_string() });
    }
    Ok(())
}

/// 번들 폴더 `dir`의 매니페스트 id가 폴더가 가리키는 공식 id `expected`와 다르면 사유를 돌려준다.
/// 매니페스트를 읽지 못하면 `None`이다. 그 경우는 버전 비교·서명 검사가 다룬다.
pub(crate) fn bundle_identity_mismatch(expected: &str, dir: &Path) -> Option<String> {
    let manifest = tasty_plugin_manifest::Manifest::load(dir).ok()?;
    (manifest.id != expected).then(|| {
        let reason = format!(
            "bundle folder '{expected}' declares plugin id '{}'",
            manifest.id
        );
        tracing::warn!("builtin bundle rejected: {reason}");
        reason
    })
}

/// 번들에서 공식 id `id`의 폴더를 고른다. 폴더가 없으면 `Ok(None)`, 매니페스트가 다른 id를
/// 선언하면 복사·권한 기록 전에 실패(`Err`)로 돌려준다.
pub(crate) fn bundle_source(bundle: &Path, id: &str) -> Result<Option<std::path::PathBuf>, String> {
    let src = bundle.join(id);
    if !src.is_dir() {
        tracing::debug!(
            "builtin plugin '{id}' not in bundle ({}), skipping",
            src.display()
        );
        return Ok(None);
    }
    match bundle_identity_mismatch(id, &src) {
        Some(reason) => Err(reason),
        None => Ok(Some(src)),
    }
}

#[cfg(test)]
#[path = "official_tests.rs"]
mod tests;
