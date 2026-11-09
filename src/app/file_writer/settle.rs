//! 시험이 file writer의 진행을 기다리는 방법.
//!
//! 판정을 시간에서 떼어 낸다. worker는 큐를 접수 순서대로 비우므로, 지금 넣은 flush가 끝나면 그
//! 앞의 요청은 모두 결과를 냈다. 그 뒤에 결과가 없으면 느린 것이 아니라 잃은 것이라 바로 실패한다.

use std::time::Duration;

use tasty_utils::batch_writer::{FlushPoint, Handle};

use super::FileExecutor;

/// worker가 멈춘 시험을 끝내기 위한 벽시계 상한. 이 시간으로 기능을 판정하지 않는다.
///
/// worker의 잠금 대기와 일하는 중인 구간은 시간 없이 구별할 수 없고, 잠든 스레드를 읽을 수 있는 OS도
/// Linux뿐이다. 값은 `StallBudget::CEILING`과 같고, Windows CI에서 관측한 lib 전체 실행 시간의
/// 최대(92초)보다 길다.
pub(crate) const HANG_LIMIT: Duration = Duration::from_secs(300);

/// 이 호출 전에 접수한 요청이 모두 끝나기를 기다린다. 요청이 성공했는지는 묻지 않는다.
pub(crate) fn settle(queue: &Handle<FileExecutor>) {
    // 큐를 닫은 worker는 이미 남은 요청을 Lost로 끝냈으므로 기다릴 것이 없다.
    let Ok(fence) = queue.flush(FlushPoint::START) else {
        return;
    };
    assert!(
        fence.wait_timeout(HANG_LIMIT).is_some(),
        "file writer did not finish its queue within {HANG_LIMIT:?}: {:?}",
        queue.stats()
    );
}
