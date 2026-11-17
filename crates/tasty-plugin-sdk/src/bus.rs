//! Plugin 측 Event Bus 클라이언트.
//!
//! plugin은 [`BusHandle`]로 호스트에 subscribe/unsubscribe/publish를 알린다. fan-out된
//! 이벤트는 호스트가 보낸 `event.dispatch` request로 도착해 [`Plugin::on_event`]가
//! 호출된다.
//!
//! 구독·발행할 이벤트는 매니페스트 `event_subscribe`·`event_publish`에 선언한다.
//! 선언이 덮지 않는 구독·발행은 호스트가 거절하고 고칠 방법을 호스트 로그에 남긴다.

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
    /// 매니페스트 `event_subscribe` 선언이 패턴을 덮지 않으면 envelope는 도착하지 않고
    /// 호스트 로그에 진단만 남는다 — plugin은 별도 에러를 받지 않는다.
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

    /// 이벤트를 호스트에 보낸다. 새 이벤트의 메타데이터는 publish_fresh로 만들 수 있다.
    /// 매니페스트 `event_publish` 선언이 키를 덮지 않으면 호스트가 거절하고 호스트 로그에 진단을 남긴다.
    pub fn publish(&self, envelope: EventEnvelope) -> Result<(), PluginError> {
        self.send(PluginEvent::EventPublish { envelope })
    }

    /// SDK에서 trace_id를 만들고 hop을 0으로 설정해 이벤트를 보낸다.
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

/// 현재 시각의 마이크로초 값과 프로세스 내부 카운터로 trace_id를 만든다.
/// 여러 프로세스 사이의 유일성까지 보장하지는 않는다.
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
