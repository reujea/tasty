//! 파일 식별을 작업 스레드에서 수행하고 요청 origin과 함께 이벤트 루프로 돌려보낸다.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use winit::event_loop::EventLoopProxy;

use crate::AppEvent;
use crate::file::format::{DetectDepth, FileFormatRegistry, FileTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentifyRequestId(pub u64);

impl std::fmt::Display for IdentifyRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct IdentifyWorker {
    registry: Arc<FileFormatRegistry>,
    proxy: EventLoopProxy<AppEvent>,
    next_id: AtomicU64,
}

impl IdentifyWorker {
    pub(crate) fn new(registry: Arc<FileFormatRegistry>, proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            registry,
            proxy,
            next_id: AtomicU64::new(1),
        }
    }

    /// 요청마다 스레드를 만든다. 결과 순서나 동시 스레드 수 제한·취소 기능은 없다.
    /// origin이 사라졌는지 확인하고 결과를 버리는 일은 GUI 적용 경로가 맡는다.
    pub fn spawn(
        &self,
        target: FileTarget,
        depth: DetectDepth,
        origin_surface_id: Option<u32>,
        policy: crate::file::dispatch::FileDispatchPolicy,
    ) -> IdentifyRequestId {
        let crate::file::dispatch::FileDispatchPolicy {
            dispatch_origin,
            ignore_size_limit,
            run,
            result_place,
        } = policy;
        let id = IdentifyRequestId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let registry = self.registry.clone();
        let proxy = self.proxy.clone();
        let target_for_thread = target.clone();
        std::thread::spawn(move || {
            let detector = registry.identify(&target_for_thread, depth);
            let done = AppEvent::IdentifyDone {
                request_id: id,
                target: target_for_thread,
                detector,
                origin_surface_id,
                dispatch_origin,
                ignore_size_limit,
                run,
                result_place,
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "The receiving session or event loop may have already ended."
            )]
            let _ = proxy.send_event(done); // event loop 종료 시에만 실패 — 무시.
        });
        id
    }
}

impl crate::core::identify_port::IdentifySpawner for IdentifyWorker {
    fn spawn_identify(
        &self,
        target: FileTarget,
        depth: DetectDepth,
        origin_surface_id: Option<u32>,
        policy: crate::file::dispatch::FileDispatchPolicy,
    ) {
        let _id = self.spawn(target, depth, origin_surface_id, policy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 독립 카운터의 증가만 검사한다. 실제 worker의 ID 발급 호출이나 overflow는 검사하지 않는다.
    #[test]
    fn request_ids_are_monotonic() {
        let counter = AtomicU64::new(1);
        let a = counter.fetch_add(1, Ordering::Relaxed);
        let b = counter.fetch_add(1, Ordering::Relaxed);
        let c = counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(c, 3);
    }

    #[test]
    fn request_id_display() {
        let id = IdentifyRequestId(42);
        assert_eq!(format!("{id}"), "42");
    }
}
