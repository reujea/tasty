//! `EventBus` 단위 테스트.

use crate::event_bus::{EventBus, EventBusError, pattern_matches};
use tasty_plugin_protocol::{EventEnvelope, EventMeta, EventOrigin, EventScope, MAX_HOP};

fn env(key: &str, origin: EventOrigin) -> EventEnvelope {
    EventEnvelope {
        key: key.to_string(),
        payload: serde_json::Value::Null,
        meta: EventMeta {
            trace_id: "t".into(),
            hop: 0,
            origin,
            scope: EventScope::System,
        },
    }
}

#[test]
fn exact_pattern_matches_exact_key() {
    assert!(pattern_matches("surface.created", "surface.created"));
    assert!(!pattern_matches("surface.created", "surface.closed"));
}

#[test]
fn wildcard_matches_same_namespace() {
    assert!(pattern_matches("surface.*", "surface.created"));
    assert!(pattern_matches("surface.*", "surface.lifecycle.changed"));
    assert!(!pattern_matches("surface.*", "tab.created"));
    // wildcard는 자기 자신과 같은 namespace 키와는 매칭되지 않음 (.*은 sub-key 의미).
    assert!(!pattern_matches("surface.*", "surface"));
}

#[test]
fn host_publish_with_no_plugin_subscriber_returns_empty() {
    let bus = EventBus::new();
    let dispatches = bus.publish_from_host(env("surface.created", EventOrigin::Host));
    assert!(dispatches.is_empty());
}

#[test]
fn plugin_publish_requires_publish_permission() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec![], vec!["p1.foo.*".into()]);
    let envelope = env(
        "p1.foo.bar",
        EventOrigin::Plugin {
            plugin_id: "p1".into(),
        },
    );
    let res = bus.publish_from_plugin("p1", envelope);
    assert!(res.is_ok());
}

#[test]
fn plugin_publish_rejected_without_permission() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec![], vec![]);
    let envelope = env(
        "p1.foo.bar",
        EventOrigin::Plugin {
            plugin_id: "p1".into(),
        },
    );
    let err = bus.publish_from_plugin("p1", envelope).unwrap_err();
    assert!(matches!(err, EventBusError::PublishDenied { .. }));
}

#[test]
fn plugin_publish_rejected_for_wrong_origin() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec![], vec!["p1.foo.*".into()]);
    let envelope = env(
        "p1.foo.bar",
        EventOrigin::Plugin {
            plugin_id: "p2".into(),
        },
    );
    let err = bus.publish_from_plugin("p1", envelope).unwrap_err();
    assert!(matches!(err, EventBusError::OriginMismatch { .. }));
}

#[test]
fn plugin_publish_rejected_at_hop_overflow() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec![], vec!["p1.foo.*".into()]);
    let mut envelope = env(
        "p1.foo.bar",
        EventOrigin::Plugin {
            plugin_id: "p1".into(),
        },
    );
    envelope.meta.hop = MAX_HOP + 1;
    let err = bus.publish_from_plugin("p1", envelope).unwrap_err();
    assert!(matches!(err, EventBusError::HopExceeded { .. }));
}

#[test]
fn plugin_subscribe_requires_permission() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec!["surface.*".into()], vec![]);
    assert!(
        bus.subscribe_plugin("p1", 1, "surface.created".into())
            .is_ok()
    );
    assert!(bus.subscribe_plugin("p1", 2, "tab.created".into()).is_err());
    assert!(bus.subscribe_plugin("p1", 3, "surface.*".into()).is_ok());
}

#[test]
fn fan_out_to_plugin_subscribers_excludes_publisher() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec!["evt.*".into()], vec!["evt.*".into()]);
    bus.set_plugin_permissions("p2", vec!["evt.*".into()], vec![]);
    bus.subscribe_plugin("p1", 1, "evt.*".into()).unwrap();
    bus.subscribe_plugin("p2", 1, "evt.*".into()).unwrap();
    let envelope = env(
        "evt.something",
        EventOrigin::Plugin {
            plugin_id: "p1".into(),
        },
    );
    let dispatches = bus.publish_from_plugin("p1", envelope).unwrap();
    // p1은 자기 이벤트를 다시 받지 않고, p2만 받는다.
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].plugin_id, "p2");
}

#[test]
fn clear_plugin_removes_subs_and_perms() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec!["evt.*".into()], vec!["evt.*".into()]);
    bus.subscribe_plugin("p1", 1, "evt.*".into()).unwrap();
    bus.clear_plugin("p1");
    let envelope = env(
        "evt.x",
        EventOrigin::Plugin {
            plugin_id: "p1".into(),
        },
    );
    let res = bus.publish_from_plugin("p1", envelope);
    assert!(matches!(res, Err(EventBusError::PublishDenied { .. })));
}

#[cfg(debug_assertions)]
#[test]
fn debug_list_subscribers_matches_subscribed_plugins() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec!["surface.*".into()], vec![]);
    bus.set_plugin_permissions("p2", vec!["surface.closed".into()], vec![]);
    bus.subscribe_plugin("p1", 1, "surface.*".into()).unwrap();
    bus.subscribe_plugin("p2", 7, "surface.closed".into())
        .unwrap();
    let subs = bus.debug_list_subscribers("surface.closed");
    assert_eq!(subs.len(), 2);
    assert!(subs.iter().any(|(p, _, _)| p == "p1"));
    assert!(subs.iter().any(|(p, sub, _)| p == "p2" && *sub == 7));
    let none = bus.debug_list_subscribers("tab.created");
    assert!(none.is_empty());
}

#[cfg(debug_assertions)]
#[test]
fn debug_trace_returns_recent_envelopes_by_id() {
    let bus = EventBus::new();
    // 3건 발화 — 같은 trace_id 2건 + 다른 1건.
    let mut e1 = env("surface.created", EventOrigin::Host);
    e1.meta.trace_id = "h1".into();
    let mut e2 = env("surface.closed", EventOrigin::Host);
    e2.meta.trace_id = "h1".into();
    let mut e3 = env("tab.created", EventOrigin::Host);
    e3.meta.trace_id = "h2".into();
    bus.publish_from_host(e1);
    bus.publish_from_host(e2);
    bus.publish_from_host(e3);
    let chain = bus.debug_trace("h1");
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].key, "surface.created");
    assert_eq!(chain[1].key, "surface.closed");
    let other = bus.debug_trace("h99");
    assert!(other.is_empty());
}

#[test]
fn unicast_to_plugin_bypasses_subscribers_and_uses_zero_sub_id() {
    // unicast는 fan-out과 별개 경로. 다른 plugin이 구독해도 envelope를 받지 않는다.
    let bus = EventBus::new();
    bus.set_plugin_permissions("p2", vec!["command.*".into()], vec![]);
    bus.subscribe_plugin("p2", 1, "command.*".into()).unwrap();
    let envelope = env("command.invoked", EventOrigin::Host);
    let dispatch = bus.unicast_to_plugin("p1", envelope);
    assert_eq!(dispatch.plugin_id, "p1");
    assert_eq!(dispatch.sub_id, 0);
}

/// 버스가 poison 돼도 구독 등록·정리·fan-out 이 계속 동작한다.
///
/// `.expect()` 이던 시절에는 이 호출들이 전부 패닉했다. 버스는 `PluginManager` 가
/// 소유해 **메인 스레드**에서 fan-out 되므로 그 패닉은 모든 창의 터미널 세션을
/// 함께 죽인다 — `Inner` 가 구독 목록과 권한 맵뿐이라 데이터는 멀쩡한데도 그랬다
/// (`docs/dev-guide/error-handling.md` "락 poison").
#[test]
fn a_poisoned_bus_keeps_serving_subscriptions_and_fan_out() {
    let bus = EventBus::new();
    bus.set_plugin_permissions("p1", vec!["evt.*".into()], vec![]);
    bus.subscribe_plugin("p1", 1, "evt.*".into())
        .expect("fresh bus accepts the subscription");

    bus.poison_for_test();

    // 등록된 구독은 살아 있고 fan-out 도 된다.
    let dispatches = bus.publish_from_host(env("evt.one", EventOrigin::Host));
    assert_eq!(dispatches.len(), 1, "poison 이후에도 fan-out 된다");

    // 새 등록·해제도 된다.
    bus.set_plugin_permissions("p2", vec!["evt.*".into()], vec![]);
    bus.subscribe_plugin("p2", 1, "evt.*".into())
        .expect("poison 이후에도 구독 등록이 된다");
    assert_eq!(
        bus.publish_from_host(env("evt.two", EventOrigin::Host))
            .len(),
        2
    );
    bus.clear_plugin("p1");
    assert_eq!(
        bus.publish_from_host(env("evt.three", EventOrigin::Host))
            .len(),
        1,
        "poison 이후에도 정리가 된다"
    );
}

// ── offset 링 ────────────────────────────────────────────────────────────────

#[test]
fn a_consumer_that_was_not_listening_still_reads_what_it_missed() {
    let bus = EventBus::new();
    // 구독자가 하나도 없는 상태로 발화한다.
    bus.publish_from_host(env("agent.task_finished", EventOrigin::Host));
    bus.publish_from_host(env("agent.barrier_closed", EventOrigin::Host));
    let got = bus.fetch(0, 10, None);
    assert_eq!(got.events.len(), 2, "구독자가 없던 동안의 사건이 없다");
    assert_eq!(got.events[0].0, 0);
    assert_eq!(got.events[1].0, 1);
    assert_eq!(got.next_offset, 2);
    assert!(!got.truncated);
    assert_eq!(got.skipped, 0);
}

#[test]
fn positions_never_repeat_and_never_go_backwards() {
    let bus = EventBus::new();
    for _ in 0..(crate::event_bus::EVENT_RING_CAPACITY + 5) {
        bus.publish_from_host(env("system.startup_complete", EventOrigin::Host));
    }
    let got = bus.fetch(0, 4, None);
    // 앞의 5 개는 밀려났다. 위치는 그 자리를 **되쓰지 않는다**.
    assert!(got.truncated, "보존 밖 요청인데 truncated 가 아니다");
    assert_eq!(got.skipped, 5);
    assert_eq!(got.events[0].0, 5, "밀려난 자리의 위치가 재사용됐다");
}

/// 보존 밖 요청에 **조용히 처음부터 주지 않는다.** 건너뛴 수를 함께 준다 —
/// 그것이 없으면 소비자는 자기가 받은 첫 사건이 진짜 첫 사건인 줄 안다.
#[test]
fn asking_for_a_position_that_scrolled_away_says_how_many_were_skipped() {
    let bus = EventBus::new();
    for _ in 0..(crate::event_bus::EVENT_RING_CAPACITY + 7) {
        bus.publish_from_host(env("system.startup_complete", EventOrigin::Host));
    }
    let got = bus.fetch(2, 3, None);
    assert!(got.truncated);
    assert_eq!(got.skipped, 5, "2 부터 요청했고 보존은 7 부터다");
    assert_eq!(got.events[0].0, 7);
}

/// 재시작한 호스트에 옛 세대의 위치를 들고 오면 그 위치는 새 링의 끝보다 뒤다.
/// 빈 답만 주면 소비자는 스트림이 그 번호에 닿을 때까지 모든 사건을 조용히 놓친다.
#[test]
fn a_position_past_the_end_is_marked_ahead_and_says_where_the_end_is() {
    let bus = EventBus::new();
    bus.publish_from_host(env("tab.created", EventOrigin::Host));
    bus.publish_from_host(env("tab.closed", EventOrigin::Host));
    let got = bus.fetch(2404, 10, None);
    assert!(got.ahead_of_stream, "끝(2) 보다 뒤인 위치다");
    assert_eq!(got.stream_end, 2);
    // 나머지는 이 표지가 없던 때와 같다 — 옛 소비자의 동작을 안 바꾼다.
    assert!(got.events.is_empty());
    assert_eq!(got.next_offset, 2404);
    assert!(!got.truncated);
    assert_eq!(got.skipped, 0);
}

/// 끝과 같은 위치는 다 읽은 소비자가 다음 사건을 기다리는 정상 자리다.
#[test]
fn the_position_right_at_the_end_is_not_ahead() {
    let bus = EventBus::new();
    bus.publish_from_host(env("tab.created", EventOrigin::Host));
    let got = bus.fetch(1, 10, None);
    assert!(!got.ahead_of_stream);
    assert_eq!(got.stream_end, 1);
    let inside = bus.fetch(0, 10, None);
    assert!(!inside.ahead_of_stream);
    assert_eq!(inside.stream_end, 1);
}

/// 앞선 위치도 즉답하지 않는다 — 즉답하면 표지를 모르는 옛 소비자가 대기 없이
/// 되묻는 루프가 된다. 기다린 뒤의 답에도 표지가 실린다.
#[test]
fn a_blocking_fetch_past_the_end_still_waits_and_then_marks_it() {
    let bus = EventBus::new();
    let wait = std::time::Duration::from_millis(80);
    let started = std::time::Instant::now();
    let got = bus.fetch_blocking(50, 10, None, wait);
    assert!(started.elapsed() >= wait, "기다리지 않고 돌아왔다");
    assert!(got.ahead_of_stream);
    assert_eq!(got.stream_end, 0);
}

#[test]
fn a_position_inside_the_ring_is_not_reported_as_truncated() {
    let bus = EventBus::new();
    bus.publish_from_host(env("tab.created", EventOrigin::Host));
    bus.publish_from_host(env("tab.closed", EventOrigin::Host));
    let got = bus.fetch(1, 10, None);
    assert!(!got.truncated);
    assert_eq!(got.skipped, 0);
    assert_eq!(got.events.len(), 1);
    assert_eq!(got.events[0].1.key, "tab.closed");
}

#[test]
fn the_filter_is_the_same_grammar_the_subscriptions_use() {
    let bus = EventBus::new();
    bus.publish_from_host(env("agent.task_finished", EventOrigin::Host));
    bus.publish_from_host(env("tab.created", EventOrigin::Host));
    bus.publish_from_host(env("agent.barrier_closed", EventOrigin::Host));

    let wild = bus.fetch(0, 10, Some("agent.*"));
    assert_eq!(wild.events.len(), 2);
    assert_eq!(wild.next_offset, 3, "필터가 거른 칸까지 다 본 것이다");

    let exact = bus.fetch(0, 10, Some("tab.created"));
    assert_eq!(exact.events.len(), 1);
    assert_eq!(exact.events[0].0, 1);

    let none = bus.fetch(0, 10, Some("nothing.here"));
    assert!(none.events.is_empty());
    assert_eq!(none.next_offset, 3, "빈 답이어도 위치는 전진한다");
}

/// `max` 에 걸려 멈췄으면 다음 위치는 **마지막으로 준 것의 다음**이다. 링의 끝으로
/// 밀면 그 사이 사건을 소비자가 영원히 못 본다.
#[test]
fn stopping_at_max_resumes_right_after_what_it_gave() {
    let bus = EventBus::new();
    for _ in 0..5 {
        bus.publish_from_host(env("tab.created", EventOrigin::Host));
    }
    let first = bus.fetch(0, 2, None);
    assert_eq!(first.events.len(), 2);
    assert_eq!(first.next_offset, 2);
    let second = bus.fetch(first.next_offset, 2, None);
    assert_eq!(second.events[0].0, 2);
}

#[test]
fn an_empty_bus_answers_with_a_position_and_no_events() {
    let bus = EventBus::new();
    let got = bus.fetch(0, 10, None);
    assert!(got.events.is_empty());
    assert_eq!(got.next_offset, 0);
    assert!(!got.truncated);
}

/// 재시작하면 위치가 0 부터 다시 매겨진다. 소비자가 그것을 **알 수 있어야** 한다 —
/// 세대 표지가 없으면 옛 위치가 새 세대의 다른 사건을 가리킨다.
///
/// 두 세대는 서로 다른 프로세스라 시계가 흐른 뒤에 선다. 그래서 둘째 버스는 시계가 첫
/// 표지를 지난 뒤에 세운다 — 바로 잇달아 세우면 해상도가 µs 인 macOS 에서 같은 값이
/// 나온다(CI 실측: 두 값 모두 `…376000`). 그 겹침은 제품에 없는 경로다.
#[test]
fn two_buses_do_not_share_a_generation_marker() {
    let a = EventBus::new();
    let now_nanos = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while now_nanos() <= a.epoch() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    let b = EventBus::new();
    assert_ne!(a.epoch(), b.epoch(), "세대 표지가 같으면 구별이 안 된다");
    assert_eq!(a.fetch(0, 1, None).epoch, a.epoch());
}

/// debug 의 trace 조회와 소비자의 `fetch` 가 **한 링**을 본다. 둘로 두면 debug 에서
/// 보이는 것과 release 가 내주는 것이 갈린다.
#[cfg(debug_assertions)]
#[test]
fn the_trace_lookup_and_the_fetch_read_the_same_ring() {
    let bus = EventBus::new();
    let mut e = env("agent.task_finished", EventOrigin::Host);
    e.meta.trace_id = "same".into();
    bus.publish_from_host(e);
    assert_eq!(bus.debug_trace("same").len(), 1);
    let got = bus.fetch(0, 10, None);
    assert_eq!(got.events.len(), 1);
    assert_eq!(got.events[0].1.meta.trace_id, "same");
}

#[test]
fn a_blocking_fetch_returns_at_once_when_there_is_already_something() {
    let bus = EventBus::new();
    bus.publish_from_host(env("agent.task_finished", EventOrigin::Host));
    let start = std::time::Instant::now();
    let got = bus.fetch_blocking(0, 10, None, std::time::Duration::from_secs(5));
    assert_eq!(got.events.len(), 1);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "줄 것이 있는데 기다렸다: {:?}",
        start.elapsed()
    );
}

#[test]
fn a_blocking_fetch_gives_up_and_answers_with_the_position() {
    let bus = EventBus::new();
    let got = bus.fetch_blocking(0, 10, None, std::time::Duration::from_millis(50));
    assert!(got.events.is_empty());
    assert_eq!(got.next_offset, 0, "빈 답이어도 이어 붙을 위치는 온다");
}

/// 기다리던 쪽이 **발화로 깨어난다.** 짧은 잠을 반복하는 구조였다면 응답 지연의
/// 바닥이 그 잠 길이가 되고, 이 시험은 그 바닥을 넘는 값으로 통과한다.
#[test]
fn a_publish_wakes_the_one_that_was_waiting() {
    use std::sync::Arc;
    let bus = Arc::new(EventBus::new());
    let waiting = Arc::clone(&bus);
    let handle = std::thread::spawn(move || {
        waiting.fetch_blocking(0, 10, None, std::time::Duration::from_secs(5))
    });
    // 대기 등록이 보이도록 잠깐 양보한다.
    std::thread::sleep(std::time::Duration::from_millis(50));
    bus.publish_from_host(env("agent.barrier_closed", EventOrigin::Host));
    let got = handle.join().expect("대기 스레드가 패닉하면 안 된다");
    assert_eq!(got.events.len(), 1);
    assert_eq!(got.events[0].1.key, "agent.barrier_closed");
}

/// 필터에 안 맞는 발화로 깨어나면 **답을 만들지 않고 남은 시간을 마저 기다린다.**
/// 깨어난 횟수가 답의 크기를 바꾸면 시끄러운 버스에서 빈 답이 쏟아진다.
#[test]
fn waking_on_something_the_filter_rejects_keeps_waiting() {
    use std::sync::Arc;
    let bus = Arc::new(EventBus::new());
    let waiting = Arc::clone(&bus);
    let handle = std::thread::spawn(move || {
        waiting.fetch_blocking(0, 10, Some("agent.*"), std::time::Duration::from_secs(5))
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    bus.publish_from_host(env("tab.created", EventOrigin::Host));
    std::thread::sleep(std::time::Duration::from_millis(50));
    bus.publish_from_host(env("agent.task_finished", EventOrigin::Host));
    let got = handle.join().expect("대기 스레드가 패닉하면 안 된다");
    assert_eq!(got.events.len(), 1, "필터 밖 사건으로 답이 났다");
    assert_eq!(got.events[0].1.key, "agent.task_finished");
}

fn sized(n: usize) -> EventEnvelope {
    let mut e = env("loadgen.tick", EventOrigin::Host);
    e.payload = serde_json::Value::String("x".repeat(n));
    e
}

fn wire_len(e: &EventEnvelope) -> usize {
    serde_json::to_vec(e).expect("envelope serializes").len()
}

/// 개수 상한 전에 바이트 상한에 닿으면 가장 오래된 사건부터 밀려나고, 그 사실은
/// 개수로 밀려난 것과 **같은 신호**(`truncated` · `skipped`)로 드러난다.
#[test]
fn the_ring_evicts_by_bytes_and_says_so_like_it_does_by_count() {
    let bus = EventBus::new();
    let one = wire_len(&sized(1000));
    bus.set_ring_bytes_limit_for_test(one * 3);
    for _ in 0..5 {
        bus.publish_from_host(sized(1000));
    }
    let (bytes, slots) = bus.ring_usage_for_test();
    assert_eq!(
        slots, 3,
        "바이트 상한이 사건 셋을 담는데 {slots} 칸이 남았다"
    );
    assert_eq!(bytes, one * 3);
    let got = bus.fetch(0, 10, None);
    assert!(got.truncated, "바이트로 밀려난 자리를 조용히 건너뛰었다");
    assert_eq!(got.skipped, 2);
    assert_eq!(got.events[0].0, 2);
    assert_eq!(got.stream_end, 5, "밀려나도 위치는 되돌아가지 않는다");
}

/// 상한보다 큰 사건도 가장 새 것이면 남는다 — 받자마자 버리면 위치만 받고 아무도
/// 못 읽는다. 다음 사건이 오면 그것이 밀려난다.
#[test]
fn an_event_larger_than_the_limit_is_kept_until_the_next_one() {
    let bus = EventBus::new();
    bus.set_ring_bytes_limit_for_test(100);
    bus.publish_from_host(sized(10));
    bus.publish_from_host(sized(5000));
    assert_eq!(
        bus.ring_usage_for_test().1,
        1,
        "큰 사건 앞의 것은 밀려나야 한다"
    );
    let got = bus.fetch(1, 10, None);
    assert_eq!(got.events.len(), 1, "가장 새 사건이 링에 없다");
    bus.publish_from_host(sized(10));
    let (bytes, slots) = bus.ring_usage_for_test();
    assert_eq!((bytes, slots), (wire_len(&sized(10)), 1));
}

/// 링이 세는 바이트는 소켓에 실리는 직렬화 길이다 — 밀어낸 뒤에도 남은 칸들의 합과 같다.
#[test]
fn the_ring_counts_the_serialized_bytes_of_what_it_holds() {
    let bus = EventBus::new();
    let sizes = [3usize, 700, 40, 1200, 9];
    bus.set_ring_bytes_limit_for_test(wire_len(&sized(1200)) + wire_len(&sized(9)) + 1);
    for n in sizes {
        bus.publish_from_host(sized(n));
    }
    let got = bus.fetch(0, 10, None);
    let held: usize = got.events.iter().map(|(_, e)| wire_len(e)).sum();
    assert_eq!(bus.ring_usage_for_test(), (held, got.events.len()));
    assert_eq!(got.events.len(), 2);
}

/// 개수 상한은 그대로다 — 작은 사건만 오면 바이트가 아니라 개수가 먼저 닿는다.
#[test]
fn the_count_limit_still_applies_to_small_events() {
    let bus = EventBus::new();
    for _ in 0..(crate::event_bus::EVENT_RING_CAPACITY + 3) {
        bus.publish_from_host(sized(1));
    }
    let (bytes, slots) = bus.ring_usage_for_test();
    assert_eq!(slots, crate::event_bus::EVENT_RING_CAPACITY);
    assert_eq!(
        bytes,
        wire_len(&sized(1)) * crate::event_bus::EVENT_RING_CAPACITY
    );
    assert!(bytes < crate::event_bus::EVENT_RING_BYTES_LIMIT);
}

// ── 등급은 구독 조건이 아니다 (ADR-0501) ─────────────────────────────────────

/// 카탈로그가 적은 구독 조건 — 매니페스트 `event_subscribe` 가 덮는가 하나 — 이 코드의
/// 판정과 같다는 것을 고정한다. Experimental 키도 `experimental_events` 유무와 무관하게
/// 받는다. 매니페스트는 실제 파서를 거치고, 권한은 `pump` 가 넘기는 필드 그대로 넘긴다.
#[test]
fn an_experimental_key_reaches_a_subscriber_with_or_without_the_experimental_flag() {
    let manifest = |id: &str, extra: &str| -> tasty_plugin_manifest::Manifest {
        toml::from_str(&format!(
            r#"manifest_version=1
id="{id}"
name="P"
version="0.1.0"
api_version="1"
event_subscribe=["agent.*", "tab.*"]
{extra}
[entry]
type="process"
command="x"
"#
        ))
        .expect("manifest parses")
    };
    let flagged = manifest("com.example.flagged", "experimental_events = true");
    let plain = manifest("com.example.plain", "");
    let bus = EventBus::new();
    for m in [&flagged, &plain] {
        bus.set_plugin_permissions(&m.id, m.event_subscribe.clone(), m.event_publish.clone());
        bus.subscribe_plugin(&m.id, 1, "agent.*".into())
            .expect("agent.* subscribe allowed");
        bus.subscribe_plugin(&m.id, 2, "tab.*".into())
            .expect("tab.* subscribe allowed");
    }
    for key in ["agent.task_finished", "tab.created"] {
        let mut got: Vec<String> = bus
            .publish_from_host(env(key, EventOrigin::Host))
            .into_iter()
            .map(|d| d.plugin_id)
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec!["com.example.flagged", "com.example.plain"],
            "{key} 는 플래그와 무관하게 두 plugin 에 간다"
        );
    }
}
