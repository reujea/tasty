//! 파일 형식 식별 시스템.
//!
//! `FileFormatRegistry` 가 모든 detector 를 보관하고 `identify` 가 매칭되는
//! `DetectorId` 를 반환한다. 매칭 실패는 `None` (= unknown) — 등록된 별도
//! "$unknown" detector 는 없다.

pub mod config;
pub(crate) mod evaluator;
pub mod info;
pub(crate) mod lua_eval;
pub mod registry;
pub(crate) mod structure_eval;
pub mod types;

pub use config::{DetectorDecl, DetectorDeclError, DetectorRuleDecl};
pub use info::DetectorInfo;
pub use registry::{ContributionSnapshot, DetectorSnapshot, FileFormatRegistry};
pub use types::{
    DetectDepth, DetectorId, DetectorRule, DetectorRuleKind, FileFormatDetector, FileTarget,
    RuleOrigin, is_valid_detector_id, looks_like_url,
};

/// 호스트가 기본 제공하는 detector 선언 묶음.
///
/// 이 파일은 크레이트 안에 있고 `include_str!` 로 바이너리에 박힌다. 예전에는 본체와
/// 시험 여섯 자리가 각자 상대 경로로 같은 파일을 박았는데, 경로가 크레이트 밖을
/// 가리키게 되면 그 자리마다 조용히 깨진다. 값을 여기 한 번만 두고 소비처가 이름으로
/// 부른다.
pub const HOST_DEFAULTS_TOML: &str = include_str!("defaults/default-file-format.toml");
