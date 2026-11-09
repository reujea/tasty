//! IPC 응답을 **나중에** 보내기 — 오래 걸리는 처리기가 plugin 루프를 막지 않게 한다.
//!
//! # 왜 있나
//!
//! [`Plugin::handle_ipc_method`](crate::Plugin::handle_ipc_method) 는 값을 돌려줘야
//! 끝나고, 그동안 worker 는 다음 요청을 못 받는다. 호스트의 생존 확인(`ping`)도 같은
//! 줄에 서므로 처리기가 60 초를 넘기면 plugin 이 응답 없음으로 재시작된다(실측
//! 2026-10-13: 80 초 걸리는 `supervisor.tell` 이 60 초째에 재시작을 불렀다).
//!
//! 처리기가 [`IpcMethodCtx::defer`](crate::plugin::IpcMethodCtx::defer) 로 답할 권리를
//! 받아 다른 스레드로 넘기면, worker 는 그 요청에 답하지 않고 다음으로 간다. 답은 그
//! 스레드가 [`DeferredReply::send`] 로 같은 요청 id 에 보낸다. 호스트는 응답을 id 로
//! 짝지으므로 순서가 바뀌어도 된다.
//!
//! # 상한
//!
//! 호스트는 namespace 호출 하나를 무한정 기다리지 않는다(`NAMESPACE_CALL_TIMEOUT`,
//! 지금 150 초). 그것을 넘기면 호출자가 만료 오류를 받고, 연달아 만료되면 plugin 이
//! 재시작된다. 미룬 처리기는 그 상한 **안에서** 스스로 답해야 한다.

use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tasty_plugin_protocol::PluginResponse;

use crate::plugin::IpcMethodError;

/// worker 가 지금 처리 중인 호스트 요청 — 처리기가 미루면 `Deferred` 로 바뀐다.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotState {
    /// 답할 호스트 요청이 없다(자기 호출 · 다른 종류의 요청).
    #[default]
    Idle,
    /// 이 id 의 `ipc.invoke` 를 처리 중이다.
    Open(u64),
    /// 처리기가 답을 가져갔다 — worker 는 이 요청에 답하지 않는다.
    Deferred,
}

/// worker 와 처리기가 함께 보는 칸. `HostHandle` 의 clone 끼리 같은 칸을 본다.
pub(crate) type ReplySlot = Arc<Mutex<SlotState>>;

/// 미룬 요청 하나에 답할 권리. 한 번만 쓸 수 있고(`send` 가 값을 소비한다), 다른
/// 스레드로 옮길 수 있다.
///
/// 답하지 않고 버리면 **오류로 답한다** — 그러지 않으면 호출자가 상한까지 기다리고,
/// 그 만료가 쌓이면 plugin 이 재시작된다.
pub struct DeferredReply {
    id: u64,
    writer: Arc<Mutex<TcpStream>>,
    sent: bool,
}

impl std::fmt::Debug for DeferredReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeferredReply")
            .field("id", &self.id)
            .field("sent", &self.sent)
            .finish()
    }
}

impl DeferredReply {
    pub(crate) fn new(id: u64, writer: Arc<Mutex<TcpStream>>) -> Self {
        Self {
            id,
            writer,
            sent: false,
        }
    }

    /// 답할 호스트 요청의 id.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 답을 보낸다. 연결이 끊겨 못 보냈으면 `false` — plugin 이 내려가는 중이다.
    pub fn send(mut self, result: Result<Value, IpcMethodError>) -> bool {
        self.sent = true;
        write(&self.writer, self.id, result)
    }
}

impl Drop for DeferredReply {
    fn drop(&mut self) {
        if !self.sent {
            tracing::warn!("deferred reply {} dropped without an answer", self.id);
            write(
                &self.writer,
                self.id,
                Err(IpcMethodError::new(
                    "the plugin dropped this call without answering",
                )),
            );
        }
    }
}

fn write(writer: &Arc<Mutex<TcpStream>>, id: u64, result: Result<Value, IpcMethodError>) -> bool {
    let resp = match result {
        Ok(v) => PluginResponse {
            id,
            result: Some(v),
            error: None,
            error_code: None,
        },
        Err(e) => PluginResponse {
            id,
            result: None,
            error: Some(e.message),
            error_code: Some(e.code),
        },
    };
    let Ok(line) = serde_json::to_string(&resp) else {
        tracing::warn!("deferred reply {id}: could not encode the response");
        return false;
    };
    // poison 이면 다른 스레드가 쓰다 죽었다 — 반쯤 쓰인 줄 위에 잇지 않고 포기한다.
    let Ok(mut w) = writer.lock() else {
        tracing::warn!("deferred reply {id}: writer lock poisoned");
        return false;
    };
    match writeln!(*w, "{line}").and_then(|()| w.flush()) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("deferred reply {id}: send failed: {e}");
            false
        }
    }
}

/// 칸 잠금이 poison 이면 안의 값을 되찾아 쓴다 — 칸은 상태 값 하나라 반쯤 쓰인 것이 없다.
/// 넘겨 버리면 그 요청의 칸이 안 열리고 안 닫혀, 처리기가 미룬 답이 나가지 않거나 worker 가
/// 답을 두 번 쓴다.
const SLOT_WHAT: &str = "deferred reply slot";
static SLOT_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn lock_slot(slot: &ReplySlot) -> std::sync::MutexGuard<'_, SlotState> {
    tasty_utils::poison::recover_mutex(slot.lock(), SLOT_WHAT, &SLOT_POISONED)
}

/// worker 쪽: 요청을 처리하기 전에 칸을 연다.
pub(crate) fn open(slot: &ReplySlot, id: u64) {
    *lock_slot(slot) = SlotState::Open(id);
}

/// worker 쪽: 처리가 끝나면 칸을 닫고, 처리기가 미뤘는지 돌려준다.
pub(crate) fn close(slot: &ReplySlot) -> bool {
    std::mem::take(&mut *lock_slot(slot)) == SlotState::Deferred
}

/// 처리기 쪽: 열린 요청이 있으면 미루고 답할 권리를 가져간다.
pub(crate) fn take(slot: &ReplySlot, writer: &Arc<Mutex<TcpStream>>) -> Option<DeferredReply> {
    let mut s = lock_slot(slot);
    let SlotState::Open(id) = *s else {
        return None;
    };
    *s = SlotState::Deferred;
    Some(DeferredReply::new(id, writer.clone()))
}
