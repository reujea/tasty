//! 호스트와 plugin의 이벤트를 구독 패턴에 맞춰 전달하고 최근 이벤트를 보관한다.
//! plugin 구독·발행은 매니페스트 선언(`event_subscribe`·`event_publish`)이 덮는지 계약을 검사하며 호스트 발행은 이 검사를 생략한다.
//! 정확한 키 또는 namespace.* 패턴을 사용한다.
//!
//! 응답을 기다리는 dispatch가 있으면 해당 plugin의 발행 hop에 하한을 적용한다.
//! 상세: docs/reference/event-catalog.md#재발행과-응답.

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
    declared_subscribe: HashMap<String, Vec<String>>,
    /// 매니페스트의 `event_publish` 패턴 (plugin_id → 패턴 목록).
    declared_publish: HashMap<String, Vec<String>>,
    /// debug 조회와 release fetch가 함께 읽는 최근 이벤트 기록.
    ring: VecDeque<RingSlot>,
    /// 링에 든 envelope 들의 직렬화 바이트 합. [`RingSlot::bytes`] 의 합과 늘 같다.
    ring_bytes: usize,
    /// 링의 바이트 상한. 기본은 [`EVENT_RING_BYTES_LIMIT`] 이고 시험만 바꾼다.
    ring_bytes_limit: usize,
    /// 다음 이벤트의 위치. 오래된 기록이 제거되어도 위치를 재사용하지 않는다.
    next_offset: u64,
    /// 보냈지만 응답을 받지 못한 dispatch. 이 목록으로 plugin별 hop 하한을 정한다.
    inflight_dispatches: HashMap<String, VecDeque<(u64, u8)>>,
}

impl Inner {
    /// 미응답 dispatch가 있으면 hop 하한을 반환한다. 없으면 새 발행으로 취급한다.
    fn relay_floor(&self, plugin_id: &str) -> Option<u8> {
        self.inflight_dispatches
            .get(plugin_id)
            .and_then(|q| q.iter().map(|(_, hop)| *hop).max())
            .map(|hop| hop.saturating_add(1))
    }
}

/// plugin별 미응답 dispatch 기록 상한. 넘으면 오래된 기록부터 버린다.
/// 버린 기록의 hop은 하한 계산에서 빠지므로 이 제한이 루프 검출 범위에도 영향을 준다.
const MAX_INFLIGHT_DISPATCHES: usize = EVENT_RING_CAPACITY;

/// 보관할 이벤트 수의 상한. 메모리 증가를 줄이기 위해 바이트 상한도 함께 적용한다.
pub const EVENT_RING_CAPACITY: usize = 1024;

/// 보관한 이벤트의 JSON 직렬화 크기 합계 상한. 실제 메모리 크기와는 다르다.
/// 개수와 바이트 상한에 맞춰 오래된 이벤트부터 제거한다.
/// 가장 최근 이벤트 하나는 상한보다 커도 보관하며 유실은 truncated·skipped로 알린다.
pub const EVENT_RING_BYTES_LIMIT: usize = 16 * 1024 * 1024;

/// 링 한 칸 — envelope 과 그것이 받은 위치, 그리고 그 envelope 의 직렬화 바이트.
#[derive(Debug, Clone)]
struct RingSlot {
    offset: u64,
    envelope: EventEnvelope,
    bytes: usize,
}

/// 버퍼를 만들지 않고 이벤트의 JSON 직렬화 크기를 센다. 실제 메모리 사용량은 아니다.
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
        // 직렬화 실패 시에도 지금까지 센 크기를 사용해 0으로 처리하지 않는다.
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
    /// 버스의 세대 표지. 소비자는 이전 세대의 offset과 구분하는 데 사용한다.
    pub epoch: u64,
    /// 요청한 위치가 보존 밖이었나. `true` 면 [`Self::skipped`] 가 몇 개를 건너뛰었는지
    /// 말한다 — **조용히 처음부터 주지 않는다.**
    pub truncated: bool,
    /// 보존 밖이라 못 준 사건 수.
    pub skipped: u64,
    /// 요청 위치가 현재 stream_end보다 뒤에 있는지 여부.
    /// 이전 호스트의 offset을 사용했거나 아직 없는 위치를 요청했을 수 있다.
    pub ahead_of_stream: bool,
    /// 다음 발화가 받을 위치 — 지금 링의 끝. 모든 답에 실린다.
    pub stream_end: u64,
}

#[derive(Clone)]
pub struct EventBus {
    inner: Arc<Mutex<Inner>>,
    /// 생성 시각에서 만든 세대 표지. 시계가 UNIX_EPOCH 이전이면 난수 기반 값을 사용한다.
    /// 시계 해상도나 되돌림에 따라 같은 값이 나올 수 있어 고유성을 보장하지는 않는다.
    epoch: u64,
    /// 새 이벤트를 보관하면 long-poll 대기자를 깨운다.
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
    /// poison을 기록한 뒤 저장된 상태로 계속 동작한다.
    /// 호스트 전체의 종료를 피하기 위한 정책이며 중간 갱신을 되돌리는 처리는 하지 않는다.
    /// 관련 정책: docs/dev-guide/error-handling.md.
    fn lock_recovering(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            self.report_poison();
            poisoned.into_inner()
        })
    }

    /// poison은 첫 발견 때만 기록한다.
    fn report_poison(&self) {
        if !self.poison_reported.swap(true, Ordering::Relaxed) {
            tracing::error!(
                "event bus mutex poisoned — a thread panicked while holding it. Recovering \
                 using the stored state; later occurrences are not logged."
            );
        }
    }

    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                plugin_subs: Vec::new(),
                declared_subscribe: HashMap::new(),
                declared_publish: HashMap::new(),
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

    /// plugin이 호스트에 등록될 때 매니페스트의 이벤트 선언을 적재한다. 비활성화/언인스톨 시 `clear_plugin`으로 정리.
    pub fn set_plugin_declarations(
        &self,
        plugin_id: &str,
        subscribe_patterns: Vec<String>,
        publish_patterns: Vec<String>,
    ) {
        let mut inner = self.lock_recovering();
        inner
            .declared_subscribe
            .insert(plugin_id.to_string(), subscribe_patterns);
        inner
            .declared_publish
            .insert(plugin_id.to_string(), publish_patterns);
    }

    /// plugin이 종료되거나 비활성화되면 선언 + 구독 모두 제거.
    pub fn clear_plugin(&self, plugin_id: &str) {
        let mut inner = self.lock_recovering();
        inner.declared_subscribe.remove(plugin_id);
        inner.declared_publish.remove(plugin_id);
        inner.plugin_subs.retain(|s| s.plugin_id != plugin_id);
        // 재시작한 plugin 은 옛 프로세스가 받던 dispatch 에 응답하지 않는다.
        inner.inflight_dispatches.remove(plugin_id);
    }

    /// 성공적으로 보낸 dispatch를 기록해 응답 전 발행에 hop 하한을 적용한다.
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

    /// hop을 max(받은 값, 미응답 dispatch의 최대 hop + 1)로 보정한다.
    /// hook 처리 중 응답이 도착할 수 있으므로 발행을 받은 시점에 적용해야 한다.
    /// 범위와 한계: docs/reference/event-catalog.md#재발행과-응답.
    pub fn apply_relay_floor(&self, plugin_id: &str, envelope: &mut EventEnvelope) {
        let floor = self.lock_recovering().relay_floor(plugin_id);
        if let Some(floor) = floor
            && envelope.meta.hop < floor
        {
            envelope.meta.hop = floor;
        }
    }

    /// plugin이 `event.subscribe` IPC로 등록한 구독. 매니페스트 `event_subscribe` 선언이 덮지 않으면 `Err`.
    /// 같은 `(plugin_id, sub_id)` 페어로 다시 호출되면 마지막 호출이 이긴다.
    pub fn subscribe_plugin(
        &self,
        plugin_id: &str,
        sub_id: u64,
        pattern: String,
    ) -> Result<(), EventBusError> {
        let mut inner = self.lock_recovering();
        let declared = inner.declared_subscribe.get(plugin_id);
        let covered =
            declared.is_some_and(|patterns| patterns.iter().any(|p| pattern_covers(p, &pattern)));
        if !covered {
            return Err(EventBusError::UndeclaredSubscribe {
                plugin_id: plugin_id.to_string(),
                pattern,
                declared: declared.cloned().unwrap_or_default(),
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
    /// 이벤트에 사용한다. 구독 선언도 검사하지 않는다 (호스트가 명시적으로 보내는 메시지).
    /// 반환값은 송신용 [`PluginDispatch`] (sub_id=0 sentinel).
    pub fn unicast_to_plugin(&self, plugin_id: &str, envelope: EventEnvelope) -> PluginDispatch {
        PluginDispatch {
            plugin_id: plugin_id.to_string(),
            sub_id: 0,
            envelope,
        }
    }

    /// plugin 발행이 `event_publish` 선언 안에 있는지 확인하고 hop 하한을 적용한 뒤 MAX_HOP을 검사한다.
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
        // event_publish 선언 대조. 선언 목록은 거절 진단에만 복제한다.
        let undeclared = {
            let inner = self.lock_recovering();
            let declared = inner.declared_publish.get(plugin_id);
            let covered = declared
                .is_some_and(|patterns| patterns.iter().any(|p| pattern_matches(p, &envelope.key)));
            (!covered).then(|| declared.cloned().unwrap_or_default())
        };
        if let Some(declared) = undeclared {
            return Err(EventBusError::UndeclaredPublish {
                plugin_id: plugin_id.to_string(),
                key: envelope.key,
                declared,
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
        // 개수·바이트 상한에 맞춰 오래된 이벤트를 제거한다. 새 이벤트 하나는 크기와 무관하게 남긴다.
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

    /// debug에서 같은 링의 trace_id 일치 항목을 발행 순서로 조회한다.
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

    /// offset부터 필터에 맞는 이벤트를 최대 max개 반환한다.
    /// 소비자가 offset을 보관하며, 그 사이 이벤트 추가·제거가 있으면 같은 요청의 결과도 달라질 수 있다.
    /// 이미 제거된 위치는 truncated·skipped로 알린다. 필터 문법은 구독 패턴과 같다.
    pub fn fetch(&self, offset: u64, max: usize, filter: Option<&str>) -> EventFetch {
        let inner = self.lock_recovering();
        Self::fetch_locked(&inner, self.epoch, offset, max, filter)
    }

    /// 보낼 이벤트가 없으면 wait 동안 기다린다. 호출 스레드를 막으므로 워커에서 사용한다.
    /// 필터 밖의 이벤트로 깨어나도 남은 시간 동안 다시 기다린다.
    /// stream_end 뒤의 위치도 기다려 오래된 클라이언트의 반복 요청을 막는다.
    /// 즉시 위치 상태를 확인하려면 wait=0으로 호출한다.
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
        // max에 도달하면 마지막 반환 항목 다음 위치, 끝까지 조회했으면 필터에서
        // 제외한 항목까지 건너뛴 링의 끝을 다음 위치로 반환한다.
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

    /// 테스트에서 락을 가진 스레드를 패닉시켜 poison 이후 동작을 확인한다.
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

/// 시계에서 세대 표지를 만든다. UNIX_EPOCH 이전이면 난수·PID·시계값을 섞는다.
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

/// 선언 패턴이 요청 패턴을 "덮는지". 매니페스트 검증된 패턴만 받는다.
///
/// - 선언이 `foo.*`: 같은 namespace 안의 모든 정확 키 또는 wildcard 를 덮는다
/// - 선언이 정확 키: 같은 정확 키만 덮는다
fn pattern_covers(declared: &str, requested: &str) -> bool {
    if declared == requested {
        return true;
    }
    if let Some(prefix) = declared.strip_suffix(".*") {
        // 요청도 같은 namespace 하위라면 OK.
        if let Some(req_prefix) = requested.strip_suffix(".*") {
            // 같은 namespace와 그 하위 namespace를 모두 덮는다.
            req_prefix == prefix || req_prefix.starts_with(&format!("{prefix}."))
        } else {
            // 정확 키가 선언 namespace 안에 있는지.
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
    UndeclaredSubscribe {
        plugin_id: String,
        pattern: String,
        /// 매니페스트 `event_subscribe` 에 선언된 패턴. 진단 메시지에 그대로 싣는다.
        declared: Vec<String>,
    },
    UndeclaredPublish {
        plugin_id: String,
        key: String,
        /// 매니페스트 `event_publish` 에 선언된 패턴. 진단 메시지에 그대로 싣는다.
        declared: Vec<String>,
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
            Self::UndeclaredSubscribe {
                plugin_id,
                pattern,
                declared,
            } => write!(
                f,
                "plugin '{plugin_id}' subscribed to '{pattern}', which its manifest does not \
                 declare. event_subscribe in its tasty-plugin.toml lists {}. Add '{pattern}' \
                 or a covering '<namespace>.*' pattern to event_subscribe.",
                declared_list(declared)
            ),
            Self::UndeclaredPublish {
                plugin_id,
                key,
                declared,
            } => {
                let namespace = key.split('.').next().unwrap_or("");
                if tasty_plugin_manifest::validators::is_reserved_event_namespace(namespace) {
                    write!(
                        f,
                        "plugin '{plugin_id}' published '{key}', but the '{namespace}' namespace \
                         is reserved for the host. A plugin cannot publish there, and adding it \
                         to event_publish in tasty-plugin.toml does not help because the \
                         manifest is refused. Publish under the plugin's own namespace instead."
                    )
                } else {
                    write!(
                        f,
                        "plugin '{plugin_id}' published '{key}', which its manifest does not \
                         declare. event_publish in its tasty-plugin.toml lists {}. Add '{key}' \
                         or a covering '<namespace>.*' pattern to event_publish.",
                        declared_list(declared)
                    )
                }
            }
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

/// 진단 메시지에 싣는 선언 목록. 비어 있으면 그렇다고 적는다.
fn declared_list(declared: &[String]) -> String {
    if declared.is_empty() {
        "no patterns".to_string()
    } else {
        declared
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "event_bus_relay_tests.rs"]
mod relay_tests;
