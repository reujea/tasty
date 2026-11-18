//! 도메인이 GUI worker 타입에 의존하지 않고 비동기 파일 식별을 요청하기 위한 인터페이스.

use crate::core::origin::FileDispatchPolicy;
use crate::file::format::{DetectDepth, FileTarget};

pub(crate) trait IdentifySpawner: Send + Sync {
    /// 식별을 백그라운드에서 시작한다. 구현이 결과를 전달하며 도메인은 요청 ID를 추적하지 않는다.
    fn spawn_identify(
        &self,
        target: FileTarget,
        depth: DetectDepth,
        origin_surface_id: Option<u32>,
        policy: FileDispatchPolicy,
    );
}
