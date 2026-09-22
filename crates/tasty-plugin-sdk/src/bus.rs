//! Plugin 측 Event Bus 클라이언트.
//!
//! plugin은 [`BusHandle`]로 호스트에 subscribe/unsubscribe/publish를 알린다. fan-out된
//! 이벤트는 호스트가 보낸 `event.dispatch` request로 도착해 [`Plugin::on_event`]가
//! 호출된다.
//!
//! 발화 권한과 구독 권한은 매니페스트의 `event_subscribe`/`event_publish` 패턴으로
//! 호스트가 결정한다 — plugin은 클라이언트 측에서 별도 권한 검사를 하지 않는다.

use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tasty_plugin_protocol::{EventEnvelope, EventMeta, EventOrigin, EventScope, PluginEvent};

use crate::error::PluginError;

/// plugin이 publish/subscribe할 수 있는 핸들. `Clone` 가능 — 여러 스레드에서 공유 안전.
#[derive(Clone)]
pub struct BusHandle {
    writer: Arc<Mutex<TcpStream>>,
    next_sub_id: Arc<AtomicU64>,
    /// plugin이 자기 envelope를 만들 때 origin에 채울 plugin id.
    plugin_id: String,
}

impl BusHandle {
    pub(crate) fn new(writer: Arc<Mutex<TcpStream>>, plugin_id: String) -> Self {
        Self {
            writer,
            next_sub_id: Arc::new(AtomicU64::new(1)),
            plugin_id,
        }
    }

    /// 패턴에 매칭되는 이벤트 구독. 반환된 `sub_id`로 [`Self::unsubscribe`]한다.
    /// 호스트가 매니페스트 권한과 매칭되지 않는다고 판단하면 envelope는 도착하지 않고
    /// 호스트 측에 경고 로그만 남는다 — plugin은 별도 에러를 받지 않는다.
    pub fn subscribe(&self, pattern: impl Into<String>) -> Result<u64, PluginError> {
        let sub_id = self.next_sub_id.fetch_add(1, Ordering::Relaxed);
        let event = PluginEvent::EventSubscribe {
            sub_id,
            pattern: pattern.into(),
        };
        self.send(event)?;
        Ok(sub_id)
    }

    pub fn unsubscribe(&self, sub_id: u64) -> Result<(), PluginError> {
        self.send(PluginEvent::EventUnsubscribe { sub_id })
    }

    /// 자기 namespace의 이벤트를 발화. `trace_id`/`hop`은 fresh 발화 가정으로
    /// 호출 측이 채우지 않아도 되도록 [`Self::publish_fresh`]가 합리적인 기본값을 만든다.
    pub fn publish(&self, envelope: EventEnvelope) -> Result<(), PluginError> {
        self.send(PluginEvent::EventPublish { envelope })
    }

    /// 권장 헬퍼: 호스트에서 새 trace_id를 발급한 신선한 envelope로 발화.
    /// payload는 임의 JSON, scope는 `System` 또는 `Surface` 중 택1.
    pub fn publish_fresh(
        &self,
        key: impl Into<String>,
        payload: serde_json::Value,
        scope: EventScope,
    ) -> Result<(), PluginError> {
        let envelope = EventEnvelope {
            key: key.into(),
            payload,
            meta: EventMeta {
                trace_id: fresh_trace_id(),
                hop: 0,
                origin: EventOrigin::Plugin {
                    plugin_id: self.plugin_id.clone(),
                },
                scope,
            },
        };
        self.publish(envelope)
    }

    fn send(&self, event: PluginEvent) -> Result<(), PluginError> {
        let payload = serde_json::json!({ "event": event });
        let line = serde_json::to_string(&payload)?;
        let mut w = self
            .writer
            .lock()
            .map_err(|_| PluginError::LockPoisoned("bus writer"))?;
        tasty_plugin_protocol::write_line(&mut *w, &line)?;
        Ok(())
    }
}

/// 단순한 단조-증가 trace id 생성기. 매니페스트가 짧은 hop chain만 다루므로 충돌 가능성
/// 무시. 호스트 측 envelope 분석에서 plugin prefix가 식별자 역할을 한다.
fn fresh_trace_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    format!("p{t:x}-{n:x}")
}
