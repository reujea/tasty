//! IPC 진입 게이트의 판정 누계 — 몇 건을 판정했고, 그중 몇 건을 **어느 게이트가** 돌려보냈나.
//!
//! 게이트는 셋이고 차례대로 본다(본체 `handler/checked.rs` 의 `check_request`): 권한 → 텔레메트리
//! cap → rate limit. 앞 게이트가 돌려보낸 요청은 뒤 게이트를 안 지나므로 한 요청은 **많아야 한 칸**에
//! 세진다. 그래서 `judged - (permission_denied + cap_blocked + throttled)` 는 세 게이트를 **모두
//! 지난** 요청 수와 정확히 같다 — 그 뒤의 봉투 검사(멱등 키 길이)에서 돌려보낸 요청도 그 안에 든다.
//! 멱등 키 봉투 거절은 정책이 아니라 모양이 틀린 인자라 이 누계가 세지 않는다.
//!
//! 거절 셋을 한 칸으로 합치지 않는 이유는 처방이 달라서다 — 권한 거절은 권한을 청해야 하고(격상
//! 승인), cap 차단은 운영자가 cap 을 풀어야 하고, 스로틀은 기다리면 풀린다. 합치면 "왜 안 먹었나"
//! 가 다시 안 보인다. 다만 권한 칸은 권한이 모자란 거절만 담지 않는다 — 권한 게이트는 권한 셋을 가진
//! 호출자(plugin · agent)가 부른 없는 메서드 이름과 그 호출자에게 열리지 않은 메서드도 같은 `-32001`
//! 로 돌려보내고, 그 둘은 어떤 권한을 청해도 안 풀린다. Local 호출자는 이 게이트에서 돌려보내지지
//! 않으므로 그 호출은 없는 메서드여도 이 칸에 안 든다([`GateRefusal::Permission`]).
//!
//! 모든 값은 **이 프로세스가 뜬 뒤의 누계**다. 내려가지 않고, 창 단위로 비워지지 않으며, 재시작하면
//! 0 에서 다시 센다. 영속되는 `RateLimit.throttled_count`(`tasty-agent`)와 이 점이 다르다 — 그쪽은
//! 버킷 하나의 수명(재설정 전까지) 동안 재시작을 넘어 쌓이고, 게이트 밖의 직접 `try_consume` 거절도
//! 센다. 근거와 대안은 ADR-0548.

use std::sync::atomic::{AtomicU64, Ordering};

/// 요청을 돌려보낸 게이트. 판정 순서와 같은 순서로 적는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRefusal {
    /// 권한 게이트 — 호출자의 권한 셋이 그 메서드를 허락하지 않았다(`-32001`). 이 칸은 게이트의
    /// 거절과 1:1 이고 wire 코드와는 아니다 — 게이트의 거절은 모두 `-32001` 이지만, 게이트 앞의 봉투
    /// 토큰 거절과 게이트를 지난 뒤 핸들러가 내는 `-32001` 은 여기 안 든다. 세 갈래가 함께 든다:
    /// 권한이 모자란 것(권한을 청하면 풀린다), 권한 셋을 가진 호출자(plugin · agent)가 부른 없는
    /// 메서드 이름과 그 호출자에게 열리지 않은 메서드. 뒤의 둘은 어떤 권한으로도 안 풀리고 부르는
    /// 메서드를 바꿔야 한다. Local 호출자는 이 게이트에서 돌려보내지지 않는다.
    Permission,
    /// 텔레메트리 cap 게이트 — 걸린 cap 이 호출자를 막고 있다(`-32007`).
    Cap,
    /// rate limit 게이트 — 호출자의 `ipc_calls` 버킷이 비었다(`-32010`).
    Throttle,
}

/// 프로세스 수명 동안 누적되는 게이트 판정 계수. 모든 갱신이 `Relaxed` 인 이유와 스냅샷이 원자적이
/// 아닌 것은 [`crate::PressureStats`] 와 같다 — 한 스냅샷에서 `judged` 가 거절 합보다 먼저 읽혀
/// 잠깐 작게 보일 수 있다.
#[derive(Debug, Default)]
pub struct GateStats {
    /// 게이트에 들어온 요청 수 — 호출자 종류를 가리지 않고(Local 도 센다), 외부 IPC 와 plugin
    /// host-call 을 다 센다. host-call 은 IPC 큐를 안 지나므로 큐 계측과 모수가 다르다.
    judged: AtomicU64,
    permission_denied: AtomicU64,
    cap_blocked: AtomicU64,
    throttled: AtomicU64,
}

impl GateStats {
    /// 요청 하나가 게이트에 들어왔다. 판정 결과와 무관하게 한 번 부른다.
    pub fn record_judged(&self) {
        self.judged.fetch_add(1, Ordering::Relaxed);
    }

    /// 요청 하나를 `by` 가 돌려보냈다.
    pub fn record_refusal(&self, by: GateRefusal) {
        let slot = match by {
            GateRefusal::Permission => &self.permission_denied,
            GateRefusal::Cap => &self.cap_blocked,
            GateRefusal::Throttle => &self.throttled,
        };
        slot.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> GateSnapshot {
        GateSnapshot {
            judged: self.judged.load(Ordering::Relaxed),
            permission_denied: self.permission_denied.load(Ordering::Relaxed),
            cap_blocked: self.cap_blocked.load(Ordering::Relaxed),
            throttled: self.throttled.load(Ordering::Relaxed),
        }
    }
}

/// [`GateStats`] 의 한 시점 읽기.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct GateSnapshot {
    pub judged: u64,
    pub permission_denied: u64,
    pub cap_blocked: u64,
    pub throttled: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_lands_in_its_own_slot() {
        let g = GateStats::default();
        for _ in 0..4 {
            g.record_judged();
        }
        g.record_refusal(GateRefusal::Permission);
        g.record_refusal(GateRefusal::Cap);
        g.record_refusal(GateRefusal::Throttle);
        g.record_refusal(GateRefusal::Throttle);
        assert_eq!(
            g.snapshot(),
            GateSnapshot {
                judged: 4,
                permission_denied: 1,
                cap_blocked: 1,
                throttled: 2,
            }
        );
    }
}
