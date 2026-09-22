//! Event Bus 1.0 — 호스트 ↔ plugin 간 브로드캐스트 이벤트 라우터.
//!
//! 책임:
//! - 매니페스트의 `event_subscribe`/`event_publish` 패턴을 권한 게이트로 보유
//! - plugin 또는 호스트가 발화한 [`EventEnvelope`]를 구독 패턴에 매칭되는 모든 대상에 fan-out
//! - 호스트 본문은 `publish()`로 직접 발화, plugin은 [`PluginEvent::EventPublish`] 경로로 위임
//! - hop count(`MAX_HOP=16`) 초과 envelope는 폐기하고 경고 로그. plugin 이 적은 hop 은
//!   믿지 않고, 응답 전인 dispatch 가 있으면 재발화 하한으로 올린다(ADR-0406)
//! - 호스트 listener와 plugin listener를 통합된 [`Subscriber`] 인터페이스로 다룬다
//! - 지나간 envelope 를 [`EVENT_RING_CAPACITY`] 개와 [`EVENT_RING_BYTES_LIMIT`] 바이트 중
//!   먼저 닿는 쪽까지 들고 있다 — 구독자가 없던 동안의 사건을 나중에 붙은 소비자가
//!   위치로 읽을 수 있게
//!
//! 패턴 매칭은 매니페스트 검증과 같은 형식을 사용한다:
//! - `surface.created` — 정확 일치
//! - `surface.*` — namespace 와일드카드 (마지막 세그먼트만 `*`)
//! - 매뉴얼 파싱이라 의존성 없음
//!
//! 권한 모델:
//! - plugin의 `event_subscribe` 패턴과 subscribe 요청 패턴이 매칭되어야 등록 허용
//! - plugin의 `event_publish` 패턴과 발화 envelope key가 매칭되어야 publish 허용
//! - 호스트 publish는 권한 검사 없이 항상 통과 (origin = Host)

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use tasty_plugin_protocol::{
    EventDispatchParams, EventEnvelope, EventOrigin, MAX_HOP, METHOD_EVENT_DISPATCH, PluginRequest,
};

/// 패턴 매칭 헬퍼. 검증된 패턴은 정확 key 또는 `<segs>.*` 형태로 정규화돼 있다고 가정.
pub(crate) fn pattern_matches(pattern: &str, key: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(".*") {
        if let Some(rest) = key.strip_prefix(prefix) {
            // 와일드카드는 `<prefix>.<segment>` 형태에 일치. `prefix`와 정확히 같은 키는 거부.
            rest.starts_with('.') && rest.len() > 1
        } else {
            false
        }
    } else {
        pattern == key
    }
}

struct PluginSubscription {
    plugin_id: String,
    sub_id: u64,
    pattern: String,
}

/// EventBus 내부 상태. 락 하나로 모든 구독 테이블을 보호.
struct Inner {
    plugin_subs: Vec<PluginSubscription>,
    /// 매니페스트의 `event_subscribe` 패턴 (plugin_id → 패턴 목록).
    plugin_subscribe_perms: HashMap<String, Vec<String>>,
    /// 매니페스트의 `event_publish` 패턴 (plugin_id → 패턴 목록).
    plugin_publish_perms: HashMap<String, Vec<String>>,
    /// 최근 발화된 envelope 를 위치와 함께 들고 있는 링. **debug 와 release 가 같은
    /// 자료구조를 쓴다** — 예전에는 이 자리가 `#[cfg(debug_assertions)]` 라 release
    /// 에서 버스가 지나간 것을 아무것도 안 들고 있었고, 그래서 두 빌드의 동작이
    /// 갈렸다. `debug.event_bus.trace` 도 이 링을 읽는다.
    ring: VecDeque<RingSlot>,
    /// 링에 든 envelope 들의 직렬화 바이트 합. [`RingSlot::bytes`] 의 합과 늘 같다.
    ring_bytes: usize,
    /// 링의 바이트 상한. 기본은 [`EVENT_RING_BYTES_LIMIT`] 이고 시험만 바꾼다.
    ring_bytes_limit: usize,
    /// 다음 발화가 받을 위치. 링에서 밀려나도 **되돌아가지 않는다** — 그래서 소비자가
    /// 든 위치가 보존 밖인지 아직 안 온 것인지가 값으로 갈린다.
    next_offset: u64,
    /// plugin 별로 **보냈지만 아직 응답이 안 온** `event.dispatch` 의 (request id, hop).
    /// 그 plugin 이 이 목록이 비지 않은 동안 publish 하면 그것은 받은 사건에 대한
    /// 반응(재발화)이고, hop 에 하한이 걸린다 — [`Inner::relay_floor`]. 근거는 ADR-0406.
    inflight_dispatches: HashMap<String, VecDeque<(u64, u8)>>,
}

impl Inner {
    /// `plugin_id` 가 지금 publish 하면 가져야 할 hop 의 하한. 응답을 기다리는
    /// dispatch 가 없으면 `None` — 그 publish 는 반응이 아니라 새 발화다.
    fn relay_floor(&self, plugin_id: &str) -> Option<u8> {
        self.inflight_dispatches
            .get(plugin_id)
            .and_then(|q| q.iter().map(|(_, hop)| *hop).max())
            .map(|hop| hop.saturating_add(1))
    }
}

/// 한 plugin 에 대해 응답을 기다리는 dispatch 기록의 상한. 넘으면 가장 오래된 것부터
/// 버린다.
///
/// 응답하지 않는 plugin 에 기록이 끝없이 쌓이지 않게 하는 값이고, **링 용량에서
/// 파생한다** — 그보다 많이 밀린 dispatch 의 사건은 링에서도 이미 밀려났다. 버린
/// 기록은 하한을 낮출 수만 있다(최댓값에서 빠진다). 그래서 이 상한이 루프를 여는
/// 방향으로 작동하려면 plugin 이 1024 건을 응답 없이 쌓아야 한다.
const MAX_INFLIGHT_DISPATCHES: usize = EVENT_RING_CAPACITY;

/// 링에 보존하는 사건 수.
///
/// **값의 단위는 개수다** — 소비자가 말하는 단위(`max`)도 개수라 두 축이 같은 단위를
/// 쓴다. 개수만으로는 메모리가 묶이지 않아 바이트 상한([`EVENT_RING_BYTES_LIMIT`])이
/// 함께 걸린다. 그 값을 재려고 발화마다 envelope 을 한 번 직렬화 길이로 센다(버퍼 없이).
///
/// **이 값은 여기 한 곳에만 있다.** 예전에 `audit` 이 보존 기간을 자기 상수로 들고
/// 있다가 부팅 경로와 **720 배** 어긋난 적이 있다(`src/adapters/ipc/audit.rs` 머리말).
/// 링을 읽는 모든 경로는 이 상수를 본다.
pub const EVENT_RING_CAPACITY: usize = 1024;

/// 링이 들고 있는 envelope 들의 **직렬화 바이트** 합의 상한. [`EVENT_RING_CAPACITY`] 와
/// 함께 걸리고, 둘 중 먼저 닿는 쪽이 가장 오래된 사건을 밀어낸다.
///
/// 개수 상한만으로는 링이 쥐는 메모리가 `1024 × 사건 크기` 이고 사건 크기에는 상한이
/// 없다 — plugin 하나가 1 MB 사건 1100 건을 발행하자 호스트 RSS 가 약 1 GB 늘었다.
/// 값은 **파생이 아니다.** plugin 채널 큐 하나의 바이트 상한(`QUEUE_BYTES_LIMIT`)과 같게
/// 둬, 빠른 발행자 하나가 링을 통해 호스트에 붙잡아 둘 수 있는 양이 채널 큐 하나가
/// 붙잡는 양을 넘지 않게 했다. 근거·대안은 ADR-0456.
///
/// **가장 새 사건 하나는 크기와 무관하게 남긴다** — 상한보다 큰 사건을 받자마자 버리면
/// 그 사건은 위치만 받고 아무도 못 읽는다. 그래서 실제 상한은 `상한 + 사건 한 건` 이다.
/// 밀려난 사건은 개수로 밀려난 것과 같은 신호(`truncated` · `skipped`)로 소비자에게
/// 드러난다 — 새 신호를 만들지 않는다.
pub const EVENT_RING_BYTES_LIMIT: usize = 16 * 1024 * 1024;

/// 링 한 칸 — envelope 과 그것이 받은 위치, 그리고 그 envelope 의 직렬화 바이트.
#[derive(Debug, Clone)]
struct RingSlot {
    offset: u64,
    envelope: EventEnvelope,
    bytes: usize,
}

/// envelope 을 JSON 으로 직렬화했을 때의 바이트 수. 버퍼를 만들지 않고 센다.
///
/// 링이 재는 단위가 이것인 이유는 plugin 채널 장부(ADR-0360)와 같은 단위 — 소켓에 실리는
/// 줄의 바이트 — 를 쓰기 위해서다. 메모리 안의 `serde_json::Value` 크기와 같지는 않지만
/// 그것에 비례한다.
fn serialized_len(envelope: &EventEnvelope) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(buf.len());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    if let Err(e) = serde_json::to_writer(&mut counter, envelope) {
        // Counter 는 실패하지 않으므로 여기 오는 것은 직렬화 자체의 실패다. 그때까지 센
        // 값을 쓴다 — 0 으로 두면 그 사건이 바이트 상한을 통째로 피한다.
        tracing::warn!(key = %envelope.key, "event ring could not size an envelope: {e}");
    }
    counter.0
}

/// 한 번의 [`EventBus::fetch`] 가 돌려주는 것.
#[derive(Debug, Clone)]
pub struct EventFetch {
    /// 요청한 위치부터의 envelope 들. 각 항목에 그 위치가 붙어 있다.
    pub events: Vec<(u64, EventEnvelope)>,
    /// 다음에 이어 붙을 위치. 빈 답이어도 이 값은 온다.
    pub next_offset: u64,
    /// 이 호스트 세대의 표지. 재시작하면 위치가 0 부터 다시 매겨지므로, 소비자가
    /// 옛 위치를 들고 와도 **이 값이 다르면 그것이 옛 세대임을 안다.**
    pub epoch: u64,
    /// 요청한 위치가 보존 밖이었나. `true` 면 [`Self::skipped`] 가 몇 개를 건너뛰었는지
    /// 말한다 — **조용히 처음부터 주지 않는다.**
    pub truncated: bool,
    /// 보존 밖이라 못 준 사건 수.
    pub skipped: u64,
    /// 요청한 위치가 **아직 발화되지 않은 자리**였나 — 링의 끝([`Self::stream_end`])
    /// 보다 뒤다. 이 세대에 그 위치는 없다. 흔한 원인은 재시작 전 세대의 위치를 들고
    /// 온 것이다. 그래도 답의 나머지(`events`·`next_offset`)는 이 필드가 없던 때와
    /// 같다 — 이 필드를 모르는 소비자는 예전처럼 기다리고, 아는 소비자는 **조용히
    /// 기다리지 않는다**. 근거는 ADR-0405.
    pub ahead_of_stream: bool,
    /// 다음 발화가 받을 위치 — 지금 링의 끝. 모든 답에 실린다.
    pub stream_end: u64,
}

#[derive(Clone)]
pub struct EventBus {
    inner: Arc<Mutex<Inner>>,
    /// 이 버스가 선 순간의 벽시계를 나노초 단위로 적은 값. 소비자가 위치의 **세대**를
    /// 가리는 데 쓴다 — 필요한 성질은 순서가 아니라 재시작마다 달라지는 것이다. 버스는
    /// 프로세스마다 하나(`PluginManager` 가 하나)라 두 세대는 서로 다른 프로세스이고, 그
    /// 사이의 시간은 시계 해상도보다 훨씬 길다. ★ 단위가 나노초일 뿐 해상도는 OS 가
    /// 정한다(macOS 는 µs, Windows 는 100 ns) — 한 프로세스 안에서 버스 둘을 잇달아
    /// 세우면 같은 값이 나올 수 있다. 그런 경로는 시험에만 있다. 시계가 1970 이전이면
    /// 벽시계 대신 프로세스마다 다른 값을 쓴다([`epoch_from_clock`]).
    epoch: u64,
    /// 발화가 있을 때마다 깨운다. long-poll 이 이것을 기다린다 — 없으면 대기하는
    /// 쪽이 짧은 잠을 반복해야 하고, 그러면 응답 지연의 바닥이 그 잠 길이가 된다.
    published: Arc<Condvar>,
    /// poison 을 이미 보고했는가. poison 은 sticky 라 fan-out 마다 같은 로그가
    /// 나오는 것을 막는다.
    poison_reported: Arc<AtomicBool>,
}

/// fan-out 결과. 호스트의 plugin 송신 루프가 후처리한다.
/// 락 안에서 `Vec<PluginRequest>`를 만들지 않고 (plugin_id, request_id, EventDispatchParams) 페어만 모아준다.
#[derive(Debug)]
pub struct PluginDispatch {
    pub plugin_id: String,
    pub sub_id: u64,
    pub envelope: EventEnvelope,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Poison 된 버스 상태를 복구한다.
    ///
    /// `Inner` 는 구독 목록과 권한 맵뿐이고 임계구역은 `insert`/`remove`/`retain`/`push`
    /// 밖에 하지 않는다 — 콜백도, 외부 호출도 없다. 그래서 패닉이 나도 맵의 불변식은
    /// 성립하고 데이터는 그대로 쓸 수 있다. 반면 이 버스는 `PluginManager` 가 소유해
    /// **메인 스레드**에서 fan-out 되므로, 여기서 패닉하면 모든 창의 터미널 세션이
    /// 함께 죽는다 — 사망 범위가 비교가 안 된다
    /// ([`error-handling.md`](../../../docs/dev-guide/error-handling.md) "락 poison").
    fn lock_recovering(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            self.report_poison();
            poisoned.into_inner()
        })
    }

    /// 같은 poison 을 두 자리에서 복구하므로 보고도 한 자리에 둔다 — 첫 번째만
    /// 찍는다(이후는 같은 사실의 반복이라 로그를 덮는다).
    fn report_poison(&self) {
        if !self.poison_reported.swap(true, Ordering::Relaxed) {
            tracing::error!(
                "event bus mutex poisoned — a thread panicked while holding it. Recovering \
                 (subscription and permission maps keep their invariants); later occurrences \
                 are not logged."
            );
        }
    }

    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                plugin_subs: Vec::new(),
                plugin_subscribe_perms: HashMap::new(),
                plugin_publish_perms: HashMap::new(),
                ring: VecDeque::with_capacity(EVENT_RING_CAPACITY),
                ring_bytes: 0,
                ring_bytes_limit: EVENT_RING_BYTES_LIMIT,
                next_offset: 0,
                inflight_dispatches: HashMap::new(),
            })),
            published: Arc::new(Condvar::new()),
            poison_reported: Arc::new(AtomicBool::new(false)),
            epoch: epoch_from_clock(
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH),
            ),
        }
    }

    /// 이 호스트 세대의 표지. [`EventFetch::epoch`] 와 같은 값이다.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// plugin이 호스트에 등록될 때 매니페스트의 권한을 적재한다. 비활성화/언인스톨 시 `clear_plugin`으로 정리.
    pub fn set_plugin_permissions(
        &self,
        plugin_id: &str,
        subscribe_patterns: Vec<String>,
        publish_patterns: Vec<String>,
    ) {
        let mut inner = self.lock_recovering();
        inner
            .plugin_subscribe_perms
            .insert(plugin_id.to_string(), subscribe_patterns);
        inner
            .plugin_publish_perms
            .insert(plugin_id.to_string(), publish_patterns);
    }

    /// plugin이 종료되거나 비활성화되면 권한 + 구독 모두 제거.
    pub fn clear_plugin(&self, plugin_id: &str) {
        let mut inner = self.lock_recovering();
        inner.plugin_subscribe_perms.remove(plugin_id);
        inner.plugin_publish_perms.remove(plugin_id);
        inner.plugin_subs.retain(|s| s.plugin_id != plugin_id);
        // 재시작한 plugin 은 옛 프로세스가 받던 dispatch 에 응답하지 않는다.
        inner.inflight_dispatches.remove(plugin_id);
    }

    /// `event.dispatch` 를 `plugin_id` 에게 보냈다 — 응답이 올 때까지 그 plugin 의
    /// publish 는 이 사건에 대한 반응으로 친다([`Self::publish_from_plugin`]).
    /// 송신에 실패한 dispatch 는 부르지 않는다: 받지 않은 사건에 반응할 수는 없다.
    pub fn note_dispatch_sent(&self, plugin_id: &str, request_id: u64, hop: u8) {
        let mut inner = self.lock_recovering();
        let q = inner
            .inflight_dispatches
            .entry(plugin_id.to_string())
            .or_default();
        if q.len() == MAX_INFLIGHT_DISPATCHES {
            q.pop_front();
        }
        q.push_back((request_id, hop));
    }

    /// `plugin_id` 가 `request_id` 에 응답했다. 그것이 이 버스가 기록한 dispatch 였으면
    /// `true` — 호출자는 그 응답을 다른 pending 요청과 견주지 않는다.
    pub fn note_dispatch_answered(&self, plugin_id: &str, request_id: u64) -> bool {
        let mut inner = self.lock_recovering();
        let Some(q) = inner.inflight_dispatches.get_mut(plugin_id) else {
            return false;
        };
        let Some(pos) = q.iter().position(|(id, _)| *id == request_id) else {
            return false;
        };
        q.remove(pos);
        if q.is_empty() {
            inner.inflight_dispatches.remove(plugin_id);
        }
        true
    }

    /// `plugin_id` 의 publish 에 재발화 hop 하한을 건다 — 응답을 기다리는 dispatch 가
    /// 있으면 `hop = max(보낸 값, 그 dispatch 들의 hop 최댓값 + 1)`.
    ///
    /// **hop 은 plugin 이 적어 보내는 값이라 그대로 믿으면 루프 차단이 안 된다.** 두
    /// plugin 이 서로의 사건에 hop 0 · 새 trace 로 반응하면 `MAX_HOP` 에 영영 안 닿는다
    /// (SDK 의 `publish_fresh` 가 바로 그 모양이다). 반응인지는 plugin 이 아니라 **호스트가
    /// 본 순서**로 판정한다: SDK 는 `on_event` 를 마친 뒤에 dispatch 에 응답하므로 그
    /// 안에서 한 publish 는 응답보다 먼저 도착한다. 근거·한계·대안은 ADR-0406.
    ///
    /// 호출 시점이 판정 시점이다 — publish 가 도착한 순간에 불러야 한다. hook 을 거쳐
    /// 나중에 fan-out 되는 publish 는 그 사이에 응답이 와 하한이 사라질 수 있다.
    pub fn apply_relay_floor(&self, plugin_id: &str, envelope: &mut EventEnvelope) {
        let floor = self.lock_recovering().relay_floor(plugin_id);
        if let Some(floor) = floor
            && envelope.meta.hop < floor
        {
            envelope.meta.hop = floor;
        }
    }

    /// plugin이 `event.subscribe` IPC로 등록한 구독. 매니페스트 권한과 매칭되지 않으면 `Err`.
    /// 같은 `(plugin_id, sub_id)` 페어로 다시 호출되면 마지막 호출이 이긴다.
    pub fn subscribe_plugin(
        &self,
        plugin_id: &str,
        sub_id: u64,
        pattern: String,
    ) -> Result<(), EventBusError> {
        let mut inner = self.lock_recovering();
        let allowed = inner
            .plugin_subscribe_perms
            .get(plugin_id)
            .map(|patterns| patterns.iter().any(|p| pattern_covers(p, &pattern)))
            .unwrap_or(false);
        if !allowed {
            return Err(EventBusError::SubscribeDenied {
                plugin_id: plugin_id.to_string(),
                pattern,
            });
        }
        // 중복 sub_id면 교체.
        inner
            .plugin_subs
            .retain(|s| !(s.plugin_id == plugin_id && s.sub_id == sub_id));
        inner.plugin_subs.push(PluginSubscription {
            plugin_id: plugin_id.to_string(),
            sub_id,
            pattern,
        });
        Ok(())
    }

    pub fn unsubscribe_plugin(&self, plugin_id: &str, sub_id: u64) {
        let mut inner = self.lock_recovering();
        inner
            .plugin_subs
            .retain(|s| !(s.plugin_id == plugin_id && s.sub_id == sub_id));
    }

    /// 호스트 본문이 새 envelope를 발화. 호스트는 모든 namespace에 publish 가능.
    /// 매칭되는 구독자에게 fan-out하고, plugin 측 dispatch 큐를 반환한다.
    pub fn publish_from_host(&self, envelope: EventEnvelope) -> Vec<PluginDispatch> {
        self.fan_out(envelope, None)
    }

    /// owner unicast — envelope를 정확히 한 plugin에만 전달한다. 일반 fan-out과 달리
    /// 호스트/다른 plugin 구독자는 무시. `command.invoked`처럼 의도적으로 owner만 받아야 하는
    /// 이벤트에 사용한다. 구독 권한도 검사하지 않는다 (호스트가 명시적으로 보내는 메시지).
    /// 반환값은 송신용 [`PluginDispatch`] (sub_id=0 sentinel).
    pub fn unicast_to_plugin(&self, plugin_id: &str, envelope: EventEnvelope) -> PluginDispatch {
        PluginDispatch {
            plugin_id: plugin_id.to_string(),
            sub_id: 0,
            envelope,
        }
    }

    /// plugin이 발화한 envelope. publish 권한 매칭 + hop count 검사 후 fan-out.
    ///
    /// hop 은 먼저 [`Self::apply_relay_floor`] 로 올린 뒤 `MAX_HOP` 과 견준다 — 그래서
    /// 서로의 사건에 반응하는 plugin 루프는 plugin 이 hop 을 뭐라고 적든 `MAX_HOP` 번
    /// 안에 끊긴다.
    pub fn publish_from_plugin(
        &self,
        plugin_id: &str,
        mut envelope: EventEnvelope,
    ) -> Result<Vec<PluginDispatch>, EventBusError> {
        self.apply_relay_floor(plugin_id, &mut envelope);
        if envelope.meta.hop > MAX_HOP {
            return Err(EventBusError::HopExceeded {
                key: envelope.key,
                hop: envelope.meta.hop,
            });
        }
        // origin이 자기 자신을 가리키는지 확인 (Plugin { plugin_id } 일치).
        let origin_matches = match &envelope.meta.origin {
            EventOrigin::Plugin { plugin_id: pid } => pid == plugin_id,
            EventOrigin::Host => false,
        };
        if !origin_matches {
            return Err(EventBusError::OriginMismatch {
                plugin_id: plugin_id.to_string(),
                envelope_origin: envelope.meta.origin.clone(),
            });
        }
        // publish 권한 매칭.
        let allowed = {
            let inner = self.lock_recovering();
            inner
                .plugin_publish_perms
                .get(plugin_id)
                .map(|patterns| patterns.iter().any(|p| pattern_matches(p, &envelope.key)))
                .unwrap_or(false)
        };
        if !allowed {
            return Err(EventBusError::PublishDenied {
                plugin_id: plugin_id.to_string(),
                key: envelope.key,
            });
        }
        Ok(self.fan_out(envelope, Some(plugin_id)))
    }

    /// 실제 fan-out. 호스트 구독자에게는 그대로 보내고, plugin 구독자는 dispatch 페어로 모아 반환.
    /// 자기 자신이 발화한 이벤트는 자기 plugin 구독자에게 보내지 않는다 (loop 1차 방지).
    fn fan_out(
        &self,
        envelope: EventEnvelope,
        publisher_plugin_id: Option<&str>,
    ) -> Vec<PluginDispatch> {
        // 크기는 락 밖에서 잰다 — 직렬화 한 번이 다른 발화와 fetch 를 막지 않게.
        let bytes = serialized_len(&envelope);
        let mut inner = self.lock_recovering();
        // 링에 위치와 함께 적는다. 앞을 `drain` 하지 않고 `VecDeque` 의 `pop_front` 를
        // 쓴다 — `Vec` 앞을 잘라내면 append 마다 뒤 전체를 memmove 한다.
        // 개수와 바이트 중 먼저 닿는 쪽으로 밀어낸다. 링이 비면 멈추므로 새 사건 하나는
        // 크기와 무관하게 들어간다(`EVENT_RING_BYTES_LIMIT` 문서).
        while inner.ring.len() >= EVENT_RING_CAPACITY
            || (!inner.ring.is_empty()
                && inner.ring_bytes.saturating_add(bytes) > inner.ring_bytes_limit)
        {
            match inner.ring.pop_front() {
                Some(evicted) => inner.ring_bytes -= evicted.bytes,
                None => break,
            }
        }
        let offset = inner.next_offset;
        inner.next_offset = offset.saturating_add(1);
        inner.ring_bytes += bytes;
        inner.ring.push_back(RingSlot {
            offset,
            envelope: envelope.clone(),
            bytes,
        });
        // 기다리는 long-poll 을 깨운다. 락은 아래 fan-out 이 끝나고 풀리므로 깨어난
        // 쪽은 그때 이어 받는다.
        self.published.notify_all();
        // plugin 구독자.
        let mut dispatches: Vec<PluginDispatch> = Vec::new();
        for sub in &inner.plugin_subs {
            if Some(sub.plugin_id.as_str()) == publisher_plugin_id {
                continue;
            }
            if pattern_matches(&sub.pattern, &envelope.key) {
                dispatches.push(PluginDispatch {
                    plugin_id: sub.plugin_id.clone(),
                    sub_id: sub.sub_id,
                    envelope: envelope.clone(),
                });
            }
        }
        dispatches
    }

    /// 호스트가 plugin에 보낼 `event.dispatch` request param을 만든다. 송신은 호출 측이 담당.
    pub fn build_dispatch_request(dispatch: &PluginDispatch) -> PluginRequest {
        let params = EventDispatchParams {
            sub_id: dispatch.sub_id,
            envelope: dispatch.envelope.clone(),
        };
        PluginRequest::new(
            METHOD_EVENT_DISPATCH,
            serde_json::to_value(&params).unwrap_or(serde_json::Value::Null),
            0, // id 는 호출 측이 채워 넣는다.
        )
    }

    /// debug 한정 — 주어진 key에 매칭되는 plugin 구독을 모아 반환.
    /// `(plugin_id, sub_id, 매니페스트의 구독 패턴)`.
    #[cfg(debug_assertions)]
    pub fn debug_list_subscribers(&self, key: &str) -> Vec<(String, u64, String)> {
        let inner = self.lock_recovering();
        inner
            .plugin_subs
            .iter()
            .filter(|s| pattern_matches(&s.pattern, key))
            .map(|s| (s.plugin_id.clone(), s.sub_id, s.pattern.clone()))
            .collect()
    }

    /// debug 한정 — 링에서 `trace_id`가 일치하는 envelope들을 발화 순서로 반환.
    /// **release 의 소비자가 읽는 것과 같은 링이다** — 둘로 두면 debug 에서 보이는
    /// 것과 release 가 내주는 것이 갈린다.
    #[cfg(debug_assertions)]
    pub fn debug_trace(&self, trace_id: &str) -> Vec<EventEnvelope> {
        let inner = self.lock_recovering();
        inner
            .ring
            .iter()
            .filter(|s| s.envelope.meta.trace_id == trace_id)
            .map(|s| s.envelope.clone())
            .collect()
    }

    /// `offset` 부터 최대 `max` 개를, `filter` 가 있으면 그것에 맞는 것만 돌려준다.
    ///
    /// **서버는 소비자별 상태를 들지 않는다** — 커서는 소비자가 들고 매번 가져온다.
    /// 그래서 같은 인자로 두 번 불러도 같은 답이 오고, 느린 소비자가 호스트 쪽에
    /// 아무것도 쌓지 않는다.
    ///
    /// 요청한 위치가 링에서 이미 밀려났으면 **조용히 처음부터 주지 않는다.**
    /// `truncated` 를 세우고 `skipped` 에 몇 개를 건너뛰었는지 싣는다.
    ///
    /// `filter` 는 구독 패턴과 **같은 문법**이다 — 정확 키 또는 `<ns>.*`. 새 문법을
    /// 만들지 않는다.
    pub fn fetch(&self, offset: u64, max: usize, filter: Option<&str>) -> EventFetch {
        let inner = self.lock_recovering();
        Self::fetch_locked(&inner, self.epoch, offset, max, filter)
    }

    /// [`Self::fetch`] 와 같되, 줄 것이 없으면 최대 `wait` 동안 기다린다.
    ///
    /// **이 함수는 부르는 스레드를 막는다.** 호출자는 워커 스레드에서 불러야 한다 —
    /// 프레임 루프에서 부르면 창이 그만큼 멈춘다(`agent.task_await` 가 같은 이유로
    /// 워커로 나간다).
    ///
    /// 기다리는 것은 **새 발화**이지 필터에 맞는 발화가 아니다. 맞지 않는 사건이
    /// 오면 한 번 더 보고 그래도 없으면 남은 시간만큼 다시 기다린다 — 그래서 시끄러운
    /// 버스에서도 깨어난 횟수가 답의 크기를 안 바꾼다.
    ///
    /// 끝보다 뒤인 위치([`EventFetch::ahead_of_stream`])도 **즉답하지 않고 기다린다.**
    /// 즉답으로 바꾸면 그 필드를 모르는 옛 소비자가 같은 위치로 대기 없이 되묻는
    /// 루프가 된다. 표지는 기다린 뒤의 답에도 실린다 — 즉시 알고 싶은 소비자는
    /// `wait` 를 0 으로 한 번 묻는다(`tasty events follow` 의 첫 요청이 그렇다).
    pub fn fetch_blocking(
        &self,
        offset: u64,
        max: usize,
        filter: Option<&str>,
        wait: std::time::Duration,
    ) -> EventFetch {
        let deadline = std::time::Instant::now() + wait;
        let mut inner = self.lock_recovering();
        loop {
            let got = Self::fetch_locked(&inner, self.epoch, offset, max, filter);
            if !got.events.is_empty() || got.truncated {
                return got;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return got;
            }
            let (guard, _timeout) = self
                .published
                .wait_timeout(inner, remaining)
                .unwrap_or_else(|poisoned| {
                    self.report_poison();
                    poisoned.into_inner()
                });
            inner = guard;
        }
    }

    fn fetch_locked(
        inner: &Inner,
        epoch: u64,
        offset: u64,
        max: usize,
        filter: Option<&str>,
    ) -> EventFetch {
        let base = inner
            .ring
            .front()
            .map(|s| s.offset)
            .unwrap_or(inner.next_offset);
        let (start, truncated, skipped) = if offset < base {
            (base, true, base - offset)
        } else {
            (offset, false, 0)
        };
        let events: Vec<(u64, EventEnvelope)> = inner
            .ring
            .iter()
            .filter(|s| s.offset >= start)
            .filter(|s| filter.is_none_or(|p| pattern_matches(p, &s.envelope.key)))
            .take(max)
            .map(|s| (s.offset, s.envelope.clone()))
            .collect();
        // 다음 위치는 **어디까지 봤는가**로 정한다. `max` 에 걸려 멈췄으면 마지막으로
        // 준 것의 다음이고, 링을 끝까지 훑었으면 필터가 거른 칸까지 다 본 것이므로
        // 링의 끝이다. 뒤쪽을 마지막 일치 자리로 되돌리면 필터에 안 맞는 구간을
        // 소비자가 매번 다시 묻는다.
        let exhausted = events.len() < max;
        let next_offset = if exhausted {
            inner.next_offset.max(start)
        } else {
            events
                .last()
                .map(|(o, _)| o.saturating_add(1))
                .unwrap_or(start)
        };
        EventFetch {
            events,
            next_offset,
            epoch,
            truncated,
            skipped,
            // 끝과 **같은** 위치는 정상이다 — 다 읽은 소비자가 다음 사건을 기다리는
            // 자리다. 끝보다 **뒤**만 이 세대에 없는 위치다.
            ahead_of_stream: offset > inner.next_offset,
            stream_end: inner.next_offset,
        }
    }

    /// 테스트 전용 — 링의 바이트 상한을 바꾼다. 기본값(16 MiB)을 시험에서 채우지 않으려고.
    #[cfg(test)]
    pub(crate) fn set_ring_bytes_limit_for_test(&self, limit: usize) {
        self.lock_recovering().ring_bytes_limit = limit;
    }

    /// 테스트 전용 — 링이 지금 든 바이트 합과 칸 수.
    #[cfg(test)]
    pub(crate) fn ring_usage_for_test(&self) -> (usize, usize) {
        let inner = self.lock_recovering();
        (inner.ring_bytes, inner.ring.len())
    }

    /// 테스트 전용 — 락을 든 채 패닉하는 스레드를 띄워 버스를 poison 시킨다.
    ///
    /// 프로덕션 임계구역에는 패닉 지점이 없어(순수 자료구조 조작) 바깥에서 poison 을
    /// 만들 방법이 없다. 그래서 poison 이후에도 버스가 동작하는지 검증하려면 이런
    /// 주입 지점이 필요하다.
    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let held = Arc::clone(&self.inner);
        let joined = std::thread::spawn(move || {
            let _guard = held.lock().expect("fresh mutex");
            panic!("a thread dies while holding the event bus");
        })
        .join();
        assert!(joined.is_err(), "그 스레드는 패닉했어야 한다");
        assert!(self.inner.lock().is_err(), "버스가 poison 됐어야 한다");
    }
}

/// 버스 세대 표지를 시계 읽기 결과에서 만든다.
///
/// 시계가 UNIX_EPOCH 보다 앞이면(RTC 가 깨진 기계) 벽시계로는 값을 못 만든다. 그때 고정값을
/// 쓰면 모든 세대가 같은 값이 되어 소비자가 재시작을 영영 모른다 — 소비자는 표지가 **다른가**
/// 만 본다. 그래서 그 갈래는 프로세스마다 다른 값을 쓴다: `RandomState` 의 키는 OS 난수로
/// 시드되므로 프로세스가 다르면 다르고, 거기에 pid 와 시계가 가리킨 음의 거리를 섞는다.
pub(crate) fn epoch_from_clock(
    since_unix: Result<std::time::Duration, std::time::SystemTimeError>,
) -> u64 {
    match since_unix {
        Ok(d) => d.as_nanos() as u64,
        Err(before) => {
            use std::hash::{BuildHasher, Hash, Hasher};
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            std::process::id().hash(&mut h);
            before.duration().as_nanos().hash(&mut h);
            h.finish()
        }
    }
}

/// 권한 패턴이 요청 패턴을 "포함"하는지. 매니페스트 검증된 패턴만 받는다.
///
/// - 권한이 `foo.*`: 같은 namespace 안의 모든 정확 키 또는 wildcard 허용
/// - 권한이 정확 키: 같은 정확 키만 허용
fn pattern_covers(allowed: &str, requested: &str) -> bool {
    if allowed == requested {
        return true;
    }
    if let Some(prefix) = allowed.strip_suffix(".*") {
        // 요청도 같은 namespace 하위라면 OK.
        if let Some(req_prefix) = requested.strip_suffix(".*") {
            // foo.* covers foo.* and foo.bar.* (sub-namespace) — 1.0은 한 depth만 고려.
            req_prefix == prefix || req_prefix.starts_with(&format!("{prefix}."))
        } else {
            // 정확 키가 권한 namespace 안에 있는지.
            requested
                .strip_prefix(prefix)
                .map(|rest| rest.starts_with('.') && rest.len() > 1)
                .unwrap_or(false)
        }
    } else {
        false
    }
}

#[derive(Debug)]
pub enum EventBusError {
    SubscribeDenied {
        plugin_id: String,
        pattern: String,
    },
    PublishDenied {
        plugin_id: String,
        key: String,
    },
    OriginMismatch {
        plugin_id: String,
        envelope_origin: EventOrigin,
    },
    HopExceeded {
        key: String,
        hop: u8,
    },
}

impl std::fmt::Display for EventBusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SubscribeDenied { plugin_id, pattern } => write!(
                f,
                "plugin '{plugin_id}' has no manifest event_subscribe permission for '{pattern}'"
            ),
            Self::PublishDenied { plugin_id, key } => write!(
                f,
                "plugin '{plugin_id}' has no manifest event_publish permission for key '{key}'"
            ),
            Self::OriginMismatch {
                plugin_id,
                envelope_origin,
            } => write!(
                f,
                "plugin '{plugin_id}' publish envelope origin mismatch: {envelope_origin:?}"
            ),
            Self::HopExceeded { key, hop } => {
                write!(f, "event '{key}' hop count {hop} exceeds MAX_HOP")
            }
        }
    }
}

impl std::error::Error for EventBusError {}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "event_bus_relay_tests.rs"]
mod relay_tests;
