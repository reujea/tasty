//! 키보드 입력을 전달할 surface 종류. 호스트 이벤트 타입은 core::host_event에 있다.

/// 터미널 입력은 별도 경로로 보내고, 다른 surface는 kind로 구분한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusedSurfaceType {
    None,
    Terminal,
    Kind(String),
}

impl FocusedSurfaceType {
    /// 등록된 kind의 capability를 조회한다. Terminal·None과 미등록 kind는 false다.
    pub fn kind_capability(
        &self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        f: impl Fn(&crate::runtime::kind_catalog::KindMetadata) -> bool,
    ) -> bool {
        match self {
            Self::Kind(k) => engine
                .surface_registry
                .get(k)
                .map(|d| f(&d))
                .unwrap_or(false),
            _ => false,
        }
    }
}
