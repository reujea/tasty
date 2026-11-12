//! 메인 루프가 멈춰 ping을 보내지 못한 시간을 무응답으로 세지 않는지 확인한다.
//! 정지는 pump에 넘기는 now를 건너뛰어 만들고, 수신 시각은 과거로 옮겨 흉내 낸다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tasty_terminal::waker_factory::NoopWakerFactory;
use tasty_timer::Precision;

use super::{PING_INTERVAL, PluginManager, PluginTick};
use crate::process::PluginProcess;

const ID: &str = "com.example.healthy";
const S: fn(u64) -> Duration = Duration::from_secs;

/// Ping 격자를 `base`에 맞춘 매니저와 프로세스 하나.
fn mgr_at(base: Instant) -> PluginManager {
    let mut m = PluginManager::new(Arc::new(NoopWakerFactory));
    m.timers
        .every(PluginTick::Ping, PING_INTERVAL, Precision::Strict, base);
    m.processes
        .insert(ID.into(), PluginProcess::stub_for_test(ID));
    m
}

/// 플러그인이 `at`에 응답한 것처럼 마지막 수신 시각을 둔다.
fn answered_at(m: &PluginManager, at: Instant) {
    m.processes[ID].backdate_pong_for_test(Instant::now() - at);
}

fn alive(m: &PluginManager) -> bool {
    m.processes.contains_key(ID)
}

#[test]
fn the_first_ping_tick_after_a_stall_does_not_restart() {
    let base = Instant::now() - S(400);
    let mut m = mgr_at(base);
    m.pump(base + S(15));
    answered_at(&m, base + S(16));

    // 210초 동안 pump를 부르지 않았다.
    m.pump(base + S(15 + 210));
    assert!(
        alive(&m),
        "정지 뒤 첫 Ping tick에서 정상 플러그인을 재시작했다"
    );
    assert_eq!(m.retiring_count(), 0);

    // 복구 뒤 응답하면 이후 tick에서도 재시작하지 않는다.
    answered_at(&m, base + S(226));
    for t in [240, 255, 270, 285, 300] {
        m.pump(base + S(t));
        answered_at(&m, base + S(t + 1));
        assert!(alive(&m), "복구 뒤 응답하는 플러그인을 {t}초에 재시작했다");
    }
}

#[test]
fn a_real_hang_after_a_stall_restarts_sixty_seconds_after_the_resume_ping() {
    let base = Instant::now() - S(400);
    let mut m = mgr_at(base);
    m.pump(base + S(15));
    answered_at(&m, base + S(16));

    m.pump(base + S(225)); // 복구 ping. 답하지 않는다.
    for t in [240, 255, 270] {
        m.pump(base + S(t));
        assert!(alive(&m), "복구 ping 뒤 {}초에 재시작했다", t - 225);
    }
    m.pump(base + S(285));
    assert!(!alive(&m), "복구 ping 뒤 60초가 지났는데 재시작하지 않았다");
}

#[test]
fn a_hang_without_a_stall_is_caught_within_seventy_five_seconds() {
    let base = Instant::now() - S(200);
    let mut m = mgr_at(base);
    m.pump(base + S(15));
    answered_at(&m, base + S(16)); // 이 뒤로 응답이 멈춘다.

    for t in [30, 45, 60, 75] {
        m.pump(base + S(t));
        assert!(
            alive(&m),
            "첫 미응답 ping(30초) 뒤 {}초에 재시작했다",
            t - 30
        );
    }
    m.pump(base + S(90));
    assert!(
        !alive(&m),
        "응답이 멈춘 뒤 74초가 지났는데 재시작하지 않았다(상한 75초)"
    );
}

#[test]
fn a_new_process_is_not_judged_before_its_first_ping() {
    let base = Instant::now() - S(200);
    let mut m = mgr_at(base);
    answered_at(&m, base - S(100));
    m.restart_unresponsive_plugins(base + S(14));
    assert!(alive(&m), "첫 ping 전의 프로세스를 무응답으로 판정했다");
}

/// 루프가 격자보다 늦게 깨어나고 그 흔들림이 tick마다 달라도 검출이 한 tick 밀리지 않는다.
#[test]
fn wake_jitter_does_not_push_the_restart_a_tick_later() {
    let base = Instant::now() - S(200);
    let ms = Duration::from_millis;
    let mut m = mgr_at(base);
    m.pump(base + S(15) + ms(40));
    answered_at(&m, base + S(16));

    // 첫 미응답 ping은 늦게, 판정 tick은 덜 늦게 깨어난다.
    for (t, j) in [(30, 50), (45, 5), (60, 30), (75, 1)] {
        m.pump(base + S(t) + ms(j));
        assert!(
            alive(&m),
            "첫 미응답 ping(30초) 뒤 {}초에 재시작했다",
            t - 30
        );
    }
    m.pump(base + S(90) + ms(10));
    assert!(
        !alive(&m),
        "깨어난 시각의 흔들림 때문에 60초 판정이 한 tick 밀렸다"
    );
}
