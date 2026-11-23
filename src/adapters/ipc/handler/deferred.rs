//! memory writer의 결과나 설정 변경의 적용 결과를 기다려 응답하는 요청.
//!
//! 핸들러는 요청을 writer에 넣고 결과로 응답을 만드는 continuation을 등록한 뒤 자리표시 응답을
//! 돌려준다. `handle_checked_request`가 자리표시를 [`Handled::Later`]로 바꾸고, 호출 경로가 원
//! 호출자(IPC 연결·plugin 호출)를 붙여 [`MemoryReplies`]에 넘긴다. 메인 루프가 회차마다 결과를
//! 거두면 continuation이 응답을 만들고 멱등 키 기록을 끝낸 뒤 원 호출자에게 보낸다.
//!
//! - 응답 시점은 commit 뒤다. 기존 IPC의 확정 성공·version·CAS 의미가 유지된다.
//! - 호출자의 응답 시한·연결 종료는 이미 접수한 요청을 취소하지 않는다. 응답만 버려진다.
//! - 제출 직후 결과가 이미 있으면 지연하지 않고 그 자리에서 응답한다. writer 없는 시험 구성이
//!   그렇고, 운영 writer에서도 수집 창 없이 실행되는 barrier 요청(Section·Read)은 제출 직후
//!   결과가 나와 있을 수 있다.

mod file;

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::mpsc::SyncSender;

use serde_json::{Value, json};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_memory::MemoryError;
use tasty_memory::writer::{Failure, MemoryCompletion, MemoryTicket};
use tasty_utils::batch_writer::SubmitError;

use crate::app::services::AppServices;

/// writer 결과로 응답을 만드는 continuation.
pub(crate) type Finish = Box<dyn FnOnce(&AppServices, MemoryCompletion) -> JsonRpcResponse + Send>;

/// 설정 변경 결과로 응답을 만드는 continuation. 결과 없이 버려진 요청은 `None`을 받는다.
pub(crate) type SettingsFinish =
    Box<dyn FnOnce(Option<crate::app::settings_change::SettingsOutcome>) -> JsonRpcResponse + Send>;

/// 응답을 미룬 요청이 기다리는 결과.
enum Wait {
    File(Box<dyn file::FileWait>),
    Memory {
        ticket: MemoryTicket,
        finish: Finish,
    },
    Settings {
        wait: crate::app::settings_change::SettingsWait,
        finish: SettingsFinish,
    },
}

/// 나온 결과.
enum Ready {
    File,
    Memory(MemoryCompletion),
    Settings(Option<crate::app::settings_change::SettingsOutcome>),
}

/// 기다리는 결과와 그 결과를 응답으로 바꾸는 continuation.
pub(crate) struct Deferred {
    id: Value,
    wait: Wait,
    idempotency: Option<super::idempotency::Pending>,
}

/// 검사한 요청을 실행한 결과. `Later`는 원 호출자를 붙여 [`MemoryReplies::push`]에 넘긴다.
pub(crate) enum Handled {
    Now(JsonRpcResponse),
    Later(Deferred),
}

impl Handled {
    /// 결과를 기다려 응답을 얻는다. 시험과 결과를 기다려도 되는 호출부만 쓴다.
    #[cfg(test)]
    pub(crate) fn wait(self, core: &AppServices) -> JsonRpcResponse {
        match self {
            Self::Now(response) => response,
            Self::Later(mut deferred) => {
                let ready = match &mut deferred.wait {
                    Wait::Memory { ticket, .. } => Ready::Memory(
                        ticket
                            .wait_timeout(std::time::Duration::from_secs(30))
                            .expect("memory writer result"),
                    ),
                    Wait::File(_) | Wait::Settings { .. } => deferred
                        .try_ready()
                        .expect("settings result must be ready; drain intents before waiting"),
                };
                deferred.complete(core, ready)
            }
        }
    }
}

impl Deferred {
    pub(super) fn with_idempotency(mut self, pending: Option<super::idempotency::Pending>) -> Self {
        self.idempotency = pending;
        self
    }

    fn try_ready(&mut self) -> Option<Ready> {
        use crate::app::settings_change::SettingsPoll;
        match &mut self.wait {
            Wait::File(wait) => wait.try_ready().then_some(Ready::File),
            Wait::Memory { ticket, .. } => ticket.try_take().map(Ready::Memory),
            Wait::Settings { wait, .. } => match wait.poll() {
                SettingsPoll::Pending => None,
                SettingsPoll::Ready(outcome) => Some(Ready::Settings(Some(outcome))),
                SettingsPoll::Dropped => Some(Ready::Settings(None)),
            },
        }
    }

    fn complete(self, core: &AppServices, ready: Ready) -> JsonRpcResponse {
        let response = match (self.wait, ready) {
            (Wait::File(wait), Ready::File) => wait.finish(core),
            (Wait::Memory { finish, .. }, Ready::Memory(completion)) => finish(core, completion),
            (Wait::Settings { finish, .. }, Ready::Settings(outcome)) => finish(outcome),
            _ => unreachable!("a result always matches the wait that produced it"),
        };
        super::idempotency::finish(core.now_instant(), self.idempotency, &response);
        response
    }

    pub(crate) fn id(&self) -> &Value {
        &self.id
    }
}

/// 응답을 받을 원 호출자.
pub(crate) enum ReplyTo {
    Ipc(SyncSender<JsonRpcResponse>),
    /// plugin의 IPC 호출. 응답 때 같은 프로세스인지 binding으로 확인한다.
    Plugin {
        plugin_id: String,
        call_id: u64,
        binding: std::sync::Weak<()>,
    },
}

impl ReplyTo {
    pub(crate) fn plugin(call: &tasty_host_plugin::manager::PendingPluginCall) -> Self {
        Self::Plugin {
            plugin_id: call.plugin_id.clone(),
            call_id: call.call_id,
            binding: call.binding.clone(),
        }
    }
}

/// 끝난 plugin 호출의 응답. plugin 매니저를 가진 루프가 보낸다.
pub(crate) struct PluginReply {
    pub(crate) plugin_id: String,
    pub(crate) call_id: u64,
    pub(crate) binding: std::sync::Weak<()>,
    pub(crate) response: JsonRpcResponse,
}

struct PendingReply {
    deferred: Deferred,
    to: ReplyTo,
}

static SLOT_POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 응답을 기다리는 요청 목록. 개수는 writer 예산이 묶는다. 메인 루프만 쓰지만 `AppServices`가
/// 스레드 사이에서 공유되는 타입이라 잠금 안에 둔다.
#[derive(Default)]
pub(crate) struct MemoryReplies {
    /// 핸들러가 이번 요청에 등록한 continuation. `handle_checked_request`가 바로 꺼낸다.
    slot: Mutex<Option<Deferred>>,
    pending: Mutex<VecDeque<PendingReply>>,
}

impl MemoryReplies {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Deferred>> {
        crate::poison::recover_mutex(self.slot.lock(), "memory reply slot", &SLOT_POISONED)
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, VecDeque<PendingReply>> {
        crate::poison::recover_mutex(self.pending.lock(), "memory replies", &SLOT_POISONED)
    }

    pub(super) fn park(&self, deferred: Deferred) {
        if let Some(previous) = self.slot().replace(deferred) {
            tracing::error!(
                id = %previous.id,
                "a handler deferred twice; the earlier result is answered by nobody"
            );
        }
    }

    pub(super) fn take_parked(&self) -> Option<Deferred> {
        self.slot().take()
    }

    pub(crate) fn push(&self, deferred: Deferred, to: ReplyTo) {
        self.pending().push_back(PendingReply { deferred, to });
    }

    /// 끝난 요청의 응답을 만든다. IPC 응답은 바로 보내고 plugin 응답은 돌려준다.
    pub(crate) fn poll(core: &AppServices) -> Vec<PluginReply> {
        core.memory_access().reap_detached();
        let mut ready = Vec::new();
        {
            let mut pending = core.memory_replies().pending();
            let mut index = 0;
            while index < pending.len() {
                match pending[index].deferred.try_ready() {
                    Some(completion) => {
                        if let Some(entry) = pending.remove(index) {
                            ready.push((entry, completion));
                        }
                    }
                    None => index += 1,
                }
            }
        }
        let mut plugin = Vec::new();
        for (entry, completion) in ready {
            let response = entry.deferred.complete(core, completion);
            match entry.to {
                ReplyTo::Ipc(tx) => tasty_ipc::server::send_response(&tx, response),
                ReplyTo::Plugin {
                    plugin_id,
                    call_id,
                    binding,
                } => plugin.push(PluginReply {
                    plugin_id,
                    call_id,
                    binding,
                    response,
                }),
            }
        }
        plugin
    }
}

/// 예산에 셀 요청 크기. 요청 인자의 JSON 길이로 잡는다.
pub(crate) fn params_bytes(params: &Value) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    if serde_json::to_writer(&mut counter, params).is_err() {
        tracing::debug!("request size could not be measured");
    }
    counter.0
}

/// 자리표시 응답. `handle_checked_request` 밖으로 나가면 호출자는 오류를 받는다.
fn placeholder(id: Value) -> JsonRpcResponse {
    JsonRpcResponse::internal_error(id, "response was deferred outside a reply path")
}

/// 설정 변경 결과의 응답. 메모리 적용과 저장이 모두 끝났을 때만 `ok`로 성공을 만든다.
/// 적용됐지만 저장하지 못한 경우는 `applied:true, saved:false`인 오류다.
pub(crate) fn settings_outcome_response(
    id: Value,
    outcome: Option<crate::app::settings_change::SettingsOutcome>,
    ok: impl FnOnce(Value, bool, &tasty_settings::Settings) -> JsonRpcResponse,
) -> JsonRpcResponse {
    use crate::app::settings_change::SettingsOutcome;
    match outcome {
        Some(SettingsOutcome::Applied {
            changed,
            saved: Ok(()),
            settings,
        }) => ok(id, changed, &settings),
        Some(SettingsOutcome::Applied {
            saved: Err(error), ..
        }) => JsonRpcResponse::error_with_data(
            id,
            -32603,
            format!("settings applied for this run but not saved: {error}"),
            json!({ "applied": true, "saved": false }),
        ),
        Some(SettingsOutcome::Rejected(reason)) => JsonRpcResponse::error_with_data(
            id,
            -32603,
            format!("settings not applied: {reason}"),
            json!({ "applied": false }),
        ),
        Some(SettingsOutcome::Superseded) => JsonRpcResponse::error_with_data(
            id,
            -32603,
            "settings change was replaced by a newer change before it finished",
            json!({ "applied": false, "superseded": true }),
        ),
        None => JsonRpcResponse::error_with_data(
            id,
            -32603,
            "settings change was dropped before it finished",
            json!({ "applied": false }),
        ),
    }
}

/// writer가 접수하지 않은 요청의 응답. 포화는 다시 시도할 수 있는 busy로 분류한다.
pub(crate) fn submit_error_response(id: Value, error: &SubmitError) -> JsonRpcResponse {
    let (failure, writer) = match error {
        SubmitError::Saturated { .. } => ("busy", "saturated"),
        SubmitError::Closed => ("other", "closed"),
    };
    JsonRpcResponse::error_with_data(
        id,
        -32603,
        format!("memory {error}"),
        json!({ "storage_failure": failure, "writer": writer }),
    )
}

/// 요청 자신의 오류도 저장 실패도 아닌 결과(확정 여부 불명, 본문 panic)의 응답.
/// `map_error`는 요청이 받은 `MemoryError`를 응답으로 바꾸는 핸들러의 매핑이다.
pub(crate) fn unsettled_response(
    id: Value,
    failure: &Failure,
    map_error: impl FnOnce(Value, &MemoryError) -> JsonRpcResponse,
) -> JsonRpcResponse {
    match failure {
        Failure::Rejected { error, .. } | Failure::Storage(error) => map_error(id, error),
        Failure::Unknown(Some(error)) => {
            let mut response = map_error(id, error);
            if let Some(e) = response.error.as_mut() {
                let data = e.data.get_or_insert_with(|| json!({}));
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("outcome".into(), json!("unknown"));
                }
            }
            response
        }
        Failure::Unknown(None) => JsonRpcResponse::error_with_data(
            id,
            -32603,
            "memory writer ended before the request finished; it may have been stored",
            json!({ "storage_failure": "other", "outcome": "unknown" }),
        ),
        Failure::Panicked => JsonRpcResponse::error_with_data(
            id,
            -32603,
            "memory request panicked",
            json!({ "storage_failure": "other" }),
        ),
    }
}

impl AppServices {
    /// 저장소 연산을 writer 요청 하나로 넣고 그 결과 뒤에 응답한다. 본문의 오류는 응답의 값이다
    /// (본문이 오류를 돌려줘도 그전의 쓰기는 직접 잠금 때와 같이 남는다). writer의 저장 실패와 결과
    /// 불명은 memory 오류 표대로 답한다.
    pub(crate) fn reply_store<T: Send + 'static>(
        &self,
        id: Value,
        bytes: usize,
        mut body: impl FnMut(&mut dyn tasty_memory::MemoryStorage) -> T + Send + 'static,
        respond: impl FnOnce(&AppServices, Value, T) -> JsonRpcResponse + Send + 'static,
    ) -> JsonRpcResponse {
        self.reply_unit(
            id,
            bytes,
            move |s| Ok(body(s)),
            move |core, id, result| match result {
                Ok(value) => respond(core, id, value),
                Err(error) => super::memory::map_error_ref(id, error),
            },
        )
    }

    /// 로그 정리 틱마다 만료 행 청소를, 주기가 돌아왔으면 로그 보존 정리를 기다리지 않는 writer
    /// 요청으로 넣는다. 별도 Unit도 writer 수집 묶음에서 transaction을 공유해 저장 실패 시 함께
    /// 롤백될 수 있다. TTL 청소는 다음 LogPrune 틱에 다시 제출한다. 로그 정리는 제출 전 성공한
    /// 1시간 claim의 시각을 접수 거절·저장 실패·롤백 뒤에도 되돌리지 않는다.
    pub(crate) fn submit_periodic_prune(&self, now_ms: u64) {
        self.memory_access().submit_detached(
            tasty_memory::writer::MemoryRequest::unit(0, |memory| {
                crate::store::expired_rows::purge(memory);
                Ok(())
            }),
            "expired memory purge",
            None,
        );
        self.submit_log_prune_if_due(now_ms);
    }

    /// 한 시간 간격이 돌아왔으면 관측 로그 보존 정리를 기다리지 않는 writer 요청으로 넣는다.
    /// 기록을 넣은 경로가 함께 부르므로 조회가 없어도 보존 정리가 실행된다.
    pub(crate) fn submit_log_prune_if_due(&self, now_ms: u64) {
        if crate::store::log_retention::claim_prune(now_ms) {
            self.memory_access().submit_detached(
                tasty_memory::writer::MemoryRequest::unit(0, move |memory| {
                    crate::store::log_retention::prune_all(memory, now_ms);
                    Ok(())
                }),
                "log retention prune",
                None,
            );
        }
    }

    /// `handle_checked_request` 밖(app 계층)에서 부른 핸들러의 응답을 보낸다. 핸들러가 writer 결과를
    /// 기다리게 했으면 그 continuation에 원 호출자를 붙이고, 아니면 바로 보낸다.
    pub(crate) fn send_handled(&self, tx: &SyncSender<JsonRpcResponse>, response: JsonRpcResponse) {
        match self.memory_replies().take_parked() {
            Some(deferred) => self.defer_reply(deferred, ReplyTo::Ipc(tx.clone())),
            None => tasty_ipc::server::send_response(tx, response),
        }
    }

    /// IPC 명령 하나를 마친 뒤 부른다. 남은 continuation은 자리표시 응답이 이미 원 호출자에게 갔고
    /// 결과에 답할 곳이 없다는 뜻이다. 진입 경로 밖에서 지연 핸들러를 부르고 [`Self::send_handled`]를
    /// 쓰지 않은 결함이다.
    pub(crate) fn expect_no_parked_reply(&self) {
        if let Some(stale) = self.memory_replies().take_parked() {
            tracing::error!(
                id = %stale.id(),
                "a handler deferred a memory reply outside a reply path; its caller got a placeholder"
            );
            #[cfg(test)]
            panic!(
                "a handler deferred a memory reply outside a reply path: {}",
                stale.id()
            );
        }
    }

    /// 요청을 writer에 넣고 결과가 나오면 `finish`로 응답한다. 결과가 바로 있으면(writer 없는
    /// 시험 구성, 운영 중 이미 끝난 barrier 요청) 지금 응답하고, 아니면 continuation을 등록하고
    /// 자리표시를 돌려준다.
    pub(crate) fn reply_with_memory(
        &self,
        id: Value,
        request: tasty_memory::writer::MemoryRequest,
        finish: impl FnOnce(&AppServices, MemoryCompletion) -> JsonRpcResponse + Send + 'static,
    ) -> JsonRpcResponse {
        // 경로를 적어 라우팅 키 검사가 이름이 같은 `agent` 핸들러로 따라가지 않게 한다.
        let ticket = match tasty_memory::writer::MemoryAccess::submit(self.memory_access(), request)
        {
            Ok(ticket) => ticket,
            Err(error) => return submit_error_response(id, &error),
        };
        self.reply_with_ticket(id, ticket, finish)
    }

    /// 이미 writer에 접수한 요청의 결과로 응답한다. 제출을 App 기능이 맡는 경로가 쓴다.
    pub(crate) fn reply_with_ticket(
        &self,
        id: Value,
        mut ticket: MemoryTicket,
        finish: impl FnOnce(&AppServices, MemoryCompletion) -> JsonRpcResponse + Send + 'static,
    ) -> JsonRpcResponse {
        if let Some(completion) = ticket.try_take() {
            return finish(self, completion);
        }
        self.memory_replies().park(Deferred {
            id: id.clone(),
            wait: Wait::Memory {
                ticket,
                finish: Box::new(finish),
            },
            idempotency: None,
        });
        placeholder(id)
    }

    /// 시험이 park된 응답을 지금 결과로 만든다. 결과가 아직 없으면 실패한다.
    #[cfg(test)]
    pub(crate) fn answer_parked_for_test(&self) -> JsonRpcResponse {
        let mut deferred = self.memory_replies().take_parked().expect("a parked reply");
        let ready = deferred.try_ready().expect("the parked reply has a result");
        deferred.complete(self, ready)
    }

    /// 좁은 설정 변경을 intent로 넣고 그 결과 뒤에 응답한다. `ok`는 적용과 저장이 끝난 성공 응답을
    /// 만든다(인자: 요청 id, 실제로 바뀌었는지, 적용 뒤의 설정).
    pub(crate) fn request_settings_change(
        &self,
        id: Value,
        change: crate::app::settings_change::SettingsChange,
        push: impl FnOnce(crate::intent::DispatchedIntent),
        ok: impl FnOnce(Value, bool, &tasty_settings::Settings) -> JsonRpcResponse + Send + 'static,
    ) -> JsonRpcResponse {
        let (done, wait) = crate::app::settings_change::pair(self.reply_wake());
        push(crate::app::command::DomainIntent::ChangeSettings { change, done }.from_agent_ipc());
        let reply_id = id.clone();
        self.reply_with_settings(id, wait, move |outcome| {
            settings_outcome_response(reply_id, outcome, ok)
        })
    }

    /// 설정 변경 결과가 나오면 `finish`로 응답한다. 핸들러는 `done`을 실은 intent를 넣고 이 함수의
    /// 자리표시 응답을 돌려준다. 원 호출자는 진입 경로가 붙인다.
    pub(crate) fn reply_with_settings(
        &self,
        id: Value,
        wait: crate::app::settings_change::SettingsWait,
        finish: impl FnOnce(Option<crate::app::settings_change::SettingsOutcome>) -> JsonRpcResponse
        + Send
        + 'static,
    ) -> JsonRpcResponse {
        self.memory_replies().park(Deferred {
            id: id.clone(),
            wait: Wait::Settings {
                wait,
                finish: Box::new(finish),
            },
            idempotency: None,
        });
        placeholder(id)
    }

    /// 읽기-판단-쓰기 본문을 writer 안에서 한 번에 실행하고 그 결과로 응답한다. `respond`는 본문의
    /// 값이나 요청이 받은 `MemoryError`(거절·저장 실패)를 받는다. 확정 여부를 모르는 결과와 본문
    /// panic은 `map_error`로 공통 응답을 만든다.
    pub(crate) fn reply_unit<T: Send + 'static>(
        &self,
        id: Value,
        bytes: usize,
        body: impl FnMut(&mut dyn tasty_memory::MemoryStorage) -> tasty_memory::Result<T>
        + Send
        + 'static,
        respond: impl FnOnce(&AppServices, Value, Result<T, &MemoryError>) -> JsonRpcResponse
        + Send
        + 'static,
    ) -> JsonRpcResponse {
        let reply_id = id.clone();
        self.reply_with_memory(
            id,
            tasty_memory::writer::MemoryRequest::unit(bytes, body),
            move |core, completion| match tasty_memory::writer::settle(completion) {
                Ok(output) => match output.into_value::<T>() {
                    Some(value) => respond(core, reply_id, Ok(value)),
                    None => JsonRpcResponse::internal_error(reply_id, "unexpected memory output"),
                },
                Err(Failure::Rejected { error, .. } | Failure::Storage(error)) => {
                    respond(core, reply_id, Err(&error))
                }
                Err(failure) => {
                    unsettled_response(reply_id, &failure, super::memory::map_error_ref)
                }
            },
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use tasty_utils::test_wait::holds_by_every;

    use serde_json::{Value, json};
    use tasty_ipc::caller::CallerContext;
    use tasty_ipc::protocol::{JsonRpcRequest, JsonRpcResponse};
    use tasty_memory::MemoryStorage;

    use super::{Handled, ReplyTo};
    use crate::app::services::AppServices;

    /// 실제 writer 스레드를 거치는 구성. 시험 구성의 기본은 writer 없이 바로 실행한다.
    /// 수집 창을 넓혀 제출 직후에는 결과가 없도록 한다. 수집 창이 없는 barrier 요청은
    /// `run_request` 가 [`WriterGate`]로 writer 를 멈춰 같은 조건을 만든다.
    fn writer_core() -> AppServices {
        writer_core_with(
            Arc::new(Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().expect("store"),
            )),
            usize::MAX,
        )
    }

    pub(crate) fn writer_core_with(
        memory: Arc<Mutex<dyn MemoryStorage>>,
        queue_items: usize,
    ) -> AppServices {
        let changes = Arc::new(tasty_memory::writer::ChangeFeed::default());
        let wake = tasty_utils::batch_writer::WakeLatch::new(Arc::new(|| {}));
        let writer = tasty_memory::writer::spawn(
            tasty_memory::writer::WriterStore::shared(memory.clone()),
            changes.clone(),
            tasty_utils::batch_writer::Limits {
                max_delay: Duration::from_millis(50),
                queue_items: queue_items
                    .min(tasty_utils::batch_writer::Limits::default().queue_items),
                ..Default::default()
            },
            Arc::new(tasty_utils::batch_writer::SystemClock),
            wake.notifier(),
        )
        .expect("memory writer");
        crate::ipc::handler::cli_entry_tests::test_core_builder()
            .with_memory(memory)
            .with_memory_writer(writer, changes, wake)
            .build()
            .expect("test AppServices")
    }

    /// 요청을 처리하는 동안 writer 를 멈춘다. barrier 요청(`Read`·`Section`)은 수집 창 없이 바로
    /// 실행되므로, 멈추지 않으면 writer 스레드가 제출 직후의 `try_take` 보다 먼저 끝내 핸들러가
    /// 결과로 바로 응답할 수 있다(`Handled::Now`). 그 응답도 writer 결과에서 만든 것이지만 시험의
    /// `later` 가 스레드 일정에 따라 달라진다. 멈춘 동안 접수한 요청은 결과가 없으므로 `later` 는
    /// 핸들러가 writer 결과를 기다리게 했는지만 나타낸다.
    struct WriterGate {
        release: mpsc::SyncSender<()>,
        ticket: tasty_memory::writer::MemoryTicket,
    }

    impl WriterGate {
        /// writer 가 없는 구성(요청을 호출 스레드에서 바로 실행)에서는 멈출 것이 없다.
        fn hold(core: &AppServices) -> Option<Self> {
            if !core.memory_access().is_queued() {
                return None;
            }
            let (release, held) = mpsc::sync_channel::<()>(1);
            // 강한 읽기는 앞선 요청이 확정된 뒤 혼자 실행되므로 뒤 요청은 이 읽기가 끝날 때까지 기다린다.
            let gate = tasty_memory::writer::MemoryRequest::read(0, move |_| {
                // 송신 쪽을 버리면(`release`, panic 으로 풀린 시험 포함) Disconnected 로 끝난다.
                if let Err(mpsc::RecvTimeoutError::Timeout) =
                    held.recv_timeout(Duration::from_secs(30))
                {
                    panic!("the test writer gate was never released");
                }
                Ok(())
            });
            // 닫히거나 가득 찬 writer 는 뒤 요청도 접수하지 않아 경합이 없다. 그 거절 응답은 시험이 본다.
            match core.memory_access().submit(gate) {
                Ok(ticket) => Some(Self { release, ticket }),
                Err(_) => None,
            }
        }

        /// writer 를 다시 움직이고 멈춤 구간이 끝날 때까지 기다린다.
        fn release(self) {
            let Self {
                release,
                mut ticket,
            } = self;
            drop(release);
            let completion = ticket
                .wait_timeout(Duration::from_secs(30))
                .expect("the writer gate finished");
            if let Err(failure) = tasty_memory::writer::settle(completion) {
                panic!("the writer gate failed: {failure:?}");
            }
        }
    }

    pub(crate) fn make_request(
        id: u64,
        method: &str,
        params: Value,
        key: Option<&str>,
    ) -> JsonRpcRequest {
        JsonRpcRequest {
            caller_agent_id: None,
            response_timeout_ms: None,
            idempotency_key: key.map(str::to_owned),
            jsonrpc: "2.0".into(),
            id: Some(json!(id)),
            method: method.into(),
            params,
            session_token: None,
        }
    }

    /// 요청을 실행하고 지연됐으면 연결 응답으로 받는다. 지연 여부를 함께 돌려준다.
    pub(crate) fn run_request(
        core: &mut AppServices,
        req: &JsonRpcRequest,
    ) -> (JsonRpcResponse, bool) {
        run_checked(core, req, &CallerContext::local(), true)
    }

    /// `run_request`와 같고 호출자를 정한다. 게이트가 거절한 요청은 그 응답을 바로 돌려준다.
    pub(crate) fn run_request_as(
        core: &mut AppServices,
        req: &JsonRpcRequest,
        caller: &CallerContext,
    ) -> (JsonRpcResponse, bool) {
        run_checked(core, req, caller, false)
    }

    fn run_checked(
        core: &mut AppServices,
        req: &JsonRpcRequest,
        caller: &CallerContext,
        panic_on_reject: bool,
    ) -> (JsonRpcResponse, bool) {
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let checked =
            match crate::ipc::handler::check_request(core, &mut state, &mut engine, req, caller) {
                Ok(checked) => checked,
                Err(r) if panic_on_reject => panic!("request rejected: {r:?}"),
                Err(response) => return (response, false),
            };
        let gate = WriterGate::hold(core);
        let handled =
            crate::ipc::handler::handle_checked_request(core, &mut state, &mut engine, &checked);
        if let Some(gate) = gate {
            gate.release();
        }
        match handled {
            Handled::Now(response) => (response, false),
            Handled::Later(deferred) => {
                let (tx, rx) = mpsc::sync_channel(1);
                core.defer_reply(deferred, ReplyTo::Ipc(tx));
                // poll_memory_replies 는 이미 나온 writer 결과만 응답으로 바꿔 보낸다(새 제출 없음).
                // 그래서 기한 뒤 관측 읽기에서도 거둔 뒤 받는다.
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut response = None;
                assert!(
                    holds_by_every(deadline, Duration::from_millis(1), |_| {
                        core.poll_memory_replies(None);
                        response = rx.try_recv().ok();
                        response.is_some()
                    }),
                    "deferred reply never arrived"
                );
                (response.expect("checked above"), true)
            }
        }
    }

    // writer 경로에서도 응답은 commit 뒤에 오고, CAS·version·오류 코드는 바로 실행할 때와 같다.
    #[test]
    fn memory_requests_answer_after_the_writer_commits() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let cas_put = |id, cas: Option<u64>| {
            let mut params = json!({"scope": "global", "key": "k", "value": "v"});
            if let Some(cas) = cas {
                params["cas"] = json!(cas);
            }
            make_request(id, "memory.put", params, None)
        };
        let mut deferred = 0;
        let (first, later) = run_request(&mut core, &cas_put(1, None));
        deferred += usize::from(later);
        assert_eq!(first.result.expect("put")["version"], json!(1));
        let (stale, later) = run_request(&mut core, &cas_put(2, Some(7)));
        deferred += usize::from(later);
        assert_eq!(stale.error.expect("cas").code, -32005);
        let (got, later) = run_request(
            &mut core,
            &make_request(
                3,
                "memory.get",
                json!({"scope": "global", "key": "k"}),
                None,
            ),
        );
        deferred += usize::from(later);
        assert_eq!(got.result.expect("get")["version"], json!(1));
        assert_eq!(deferred, 3, "every request goes through the writer");
    }

    // 읽기-판단-쓰기 연산(blackboard)도 writer 안에서 한 번에 판정한다. CAS 거절은 기존 코드다.
    #[test]
    fn compound_memory_requests_judge_inside_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let bb = |id, method: &str, extra: Value| {
            let mut params = json!({"workspace_id": 1, "name": "board"});
            if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
                p.extend(e.clone());
            }
            make_request(id, method, params, None)
        };
        let (created, later) = run_request(&mut core, &bb(1, "memory.bb_create", json!({})));
        assert!(later);
        assert!(created.error.is_none(), "{created:?}");
        let (first, _) = run_request(
            &mut core,
            &bb(2, "memory.bb_put", json!({"field": "f", "value": "1"})),
        );
        let version = first.result.expect("bb_put")["version"].clone();
        let (stale, _) = run_request(
            &mut core,
            &bb(
                3,
                "memory.bb_put",
                json!({"field": "f", "value": "2", "cas": 99}),
            ),
        );
        assert_eq!(stale.error.expect("cas").code, -32005);
        let (got, _) = run_request(&mut core, &bb(4, "memory.bb_get", json!({"field": "f"})));
        let got = got.result.expect("bb_get");
        assert_eq!(got["version"], version);
        assert_eq!(got["value"], json!("1"));
    }

    // agent 협업 연산도 판단과 쓰기를 writer 요청 하나로 하고 그 결과 뒤에 응답한다.
    #[test]
    fn agent_coordination_requests_judge_inside_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let call = |id, method: &str, extra: Value| {
            let mut params = json!({"workspace_id": 1, "name": "gate"});
            if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
                p.extend(e.clone());
            }
            make_request(id, method, params, None)
        };
        let (created, later) = run_request(
            &mut core,
            &call(1, "agent.semaphore_create", json!({"permits": 1})),
        );
        assert!(later);
        assert!(created.error.is_none(), "{created:?}");
        let (first, _) = run_request(
            &mut core,
            &call(2, "agent.semaphore_acquire", json!({"holder": "a"})),
        );
        assert_eq!(
            first.result.expect("first acquire")["acquired"],
            json!(true)
        );
        let (second, _) = run_request(
            &mut core,
            &call(3, "agent.semaphore_acquire", json!({"holder": "b"})),
        );
        assert_eq!(
            second.result.expect("second acquire")["acquired"],
            json!(false)
        );
        let (missing, later) = run_request(
            &mut core,
            &call(4, "agent.barrier_signal", json!({"name": "absent"})),
        );
        assert!(later);
        assert!(missing.error.is_some(), "{missing:?}");
    }

    // task 연산은 writer의 호환 구간에서 실행하고 그 결과 뒤에 응답한다. 앞서 접수한 생성을 뒤의
    // 조회·취소가 본다.
    #[test]
    fn task_requests_run_in_writer_order() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let (created, later) = run_request(
            &mut core,
            &make_request(
                1,
                "agent.task_create",
                json!({"workspace_id": 1, "name": "t",
                       "command": {"kind": "run", "workspace_id": 1, "command": ["true"]}}),
                None,
            ),
        );
        assert!(later);
        let task_id = created.result.expect("created")["id"]
            .as_str()
            .expect("task id")
            .to_string();
        let (got, later) = run_request(
            &mut core,
            &make_request(
                2,
                "agent.task_get",
                json!({"workspace_id": 1, "id": task_id}),
                None,
            ),
        );
        assert!(later);
        assert_eq!(got.result.expect("task_get")["id"], json!(task_id));
        let (cancelled, _) = run_request(
            &mut core,
            &make_request(
                3,
                "agent.task_cancel",
                json!({"workspace_id": 1, "id": task_id}),
                None,
            ),
        );
        assert_eq!(
            cancelled.result.expect("task_cancel")["task"]["state"]["kind"],
            json!("cancelled")
        );
        let (missing, _) = run_request(
            &mut core,
            &make_request(
                4,
                "agent.task_get",
                json!({"workspace_id": 1, "id": "absent"}),
                None,
            ),
        );
        assert_eq!(missing.error.expect("missing").code, -32004);
    }

    // 세션 IPC와 명령 기록 조회도 writer 요청의 결과 뒤에 응답한다.
    #[test]
    fn session_and_command_reads_answer_after_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let (issued, later) = run_request(
            &mut core,
            &make_request(1, "session.issue", json!({"agent_id": "w-agent"}), None),
        );
        assert!(later);
        assert!(issued.result.expect("issued")["token"].is_string());
        let (listed, later) =
            run_request(&mut core, &make_request(2, "session.list", json!({}), None));
        assert!(later);
        let sessions = listed.result.expect("listed")["sessions"].clone();
        assert_eq!(sessions[0]["agent_id"], json!("w-agent"));
        let (commands, later) = run_request(
            &mut core,
            &make_request(3, "surface.commands", json!({"surface_id": 7}), None),
        );
        assert!(later);
        assert_eq!(commands.result.expect("commands")["commands"], json!([]));
    }

    /// 진입 경로 밖(app 계층)에서 부른 핸들러의 응답을 [`AppServices::send_handled`]로 보내고 결과를
    /// 기다린다.
    fn answer_outside_the_router(core: &AppServices, response: JsonRpcResponse) -> JsonRpcResponse {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        core.send_handled(&tx, response);
        // 원래 순서는 받기 → 기한 검사 → 거두기 → 쉼이다. 거두기는 검사를 통과한 뒤에만 돌리고, 기한 뒤
        // 관측 읽기에서는 이미 나온 결과만 응답으로 바꾸는 거두기(새 제출 없음) 뒤에 한 번 더 받는다.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut started = false;
        let mut answer = None;
        assert!(
            holds_by_every(deadline, Duration::ZERO, |last| {
                if last || std::mem::replace(&mut started, true) {
                    core.poll_memory_replies(None);
                    if !last {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                answer = rx.try_recv().ok();
                answer.is_some()
            }),
            "no reply"
        );
        answer.expect("checked above")
    }

    // GUI·headless app 계층이 부르는 감사 조회도 writer 결과로 답한다. 자리표시를 보내면 실패한다.
    #[test]
    fn app_layer_audit_handlers_answer_after_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let core = writer_core();
        let registry = crate::runtime::surface_registry::SurfaceKindRegistry::new();
        for method in ["plugin.audit_query", "plugin.audit_summary"] {
            let response = crate::ipc::handler::plugin::dispatch_readonly(
                &core,
                None,
                &registry,
                method,
                json!(1),
                &json!({}),
            )
            .expect("read only method");
            let answer = answer_outside_the_router(&core, response);
            assert!(answer.error.is_none(), "{method}: {answer:?}");
        }
        #[cfg(feature = "gui")]
        {
            let follow = crate::ipc::handler::audit::handle_follow(&core, json!(2), &json!({}));
            let answer = answer_outside_the_router(&core, follow);
            assert!(answer.error.is_none(), "audit_follow: {answer:?}");
            let clear = crate::ipc::handler::audit::handle_clear(&core, json!(3), &json!({}));
            let answer = answer_outside_the_router(&core, clear);
            assert_eq!(answer.result.expect("cleared")["removed"], json!(0));
        }
        core.expect_no_parked_reply();
    }

    // GUI app 계층이 부르는 agent 권한 부여·회수·조회도 writer 결과로 답한다.
    #[cfg(feature = "gui")]
    #[test]
    fn app_layer_agent_permission_handlers_answer_after_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let (issued, later) = run_request(
            &mut core,
            &make_request(1, "session.issue", json!({"agent_id": "app-agent"}), None),
        );
        assert!(later);
        assert!(issued.error.is_none(), "{issued:?}");
        let grant = crate::ipc::handler::session::handle_grant_agent_permission(
            &core,
            json!(2),
            &json!({"agent_id": "app-agent", "permission": "surface.read"}),
        );
        let answer = answer_outside_the_router(&core, grant);
        assert_eq!(answer.result.expect("granted")["added"], json!(true));
        let registry = crate::runtime::surface_registry::SurfaceKindRegistry::new();
        let listed = crate::ipc::handler::plugin::dispatch_readonly(
            &core,
            None,
            &registry,
            "plugin.list_agent_permissions",
            json!(3),
            &json!({"agent_id": "app-agent"}),
        )
        .expect("read only method");
        let answer = answer_outside_the_router(&core, listed);
        let agents = answer.result.expect("listed")["agents"].clone();
        assert_eq!(
            agents[0]["temp_grants"][0]["permission"],
            json!("surface.read")
        );
        let revoke = crate::ipc::handler::session::handle_revoke_agent_permission(
            &core,
            json!(4),
            &json!({"agent_id": "app-agent", "permission": "surface.read"}),
        );
        let answer = answer_outside_the_router(&core, revoke);
        assert_eq!(answer.result.expect("revoked")["removed"], json!(true));
        core.expect_no_parked_reply();
    }

    // 진입 경로 밖에서 지연 핸들러를 부르고 그 응답을 그대로 보내면 명령 뒤 가드가 잡는다.
    #[test]
    #[should_panic(expected = "deferred a memory reply outside a reply path")]
    fn a_reply_parked_outside_a_reply_path_fails_the_guard() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let core = writer_core();
        let registry = crate::runtime::surface_registry::SurfaceKindRegistry::new();
        let placeholder = crate::ipc::handler::plugin::dispatch_readonly(
            &core,
            None,
            &registry,
            "plugin.audit_query",
            json!(1),
            &json!({}),
        );
        assert!(placeholder.is_some());
        core.expect_no_parked_reply();
    }

    // telemetry cap 관리·조회도 writer 요청의 결과 뒤에 응답한다.
    #[test]
    fn telemetry_reads_answer_after_the_writer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let (set, later) = run_request(
            &mut core,
            &make_request(
                1,
                "telemetry.cap.set",
                json!({"agent": "a", "metric": "tokens", "threshold": 5}),
                None,
            ),
        );
        assert!(later);
        let cap_id = set.result.expect("cap set")["id"].clone();
        let (list, later) = run_request(
            &mut core,
            &make_request(2, "telemetry.cap.list", json!({}), None),
        );
        assert!(later);
        let list = list.result.expect("cap list");
        assert_eq!(list["count"], json!(1));
        assert_eq!(list["entries"][0]["id"], cap_id);
        let (summary, later) = run_request(
            &mut core,
            &make_request(3, "telemetry.summary", json!({}), None),
        );
        assert!(later);
        assert!(summary.error.is_none(), "{summary:?}");
    }

    // 멱등 키 기록은 continuation의 응답으로 끝난다. 같은 키의 재시도는 실행하지 않고 같은 답을
    // 받고, 결과를 기다리는 동안의 재시도는 결과 불명 오류를 받는다.
    #[test]
    fn a_deferred_reply_records_its_idempotency_answer() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let params = json!({
            "entries": [{"scope": "global", "key": "idem", "kind": "text", "value": "v"}],
        });
        let import = || make_request(1, "memory.import", params.clone(), Some("deferred-idem"));
        let (first, _) = run_request(&mut core, &import());
        assert_eq!(first.result.clone().expect("import")["applied"], json!(1));
        let (again, later) = run_request(&mut core, &import());
        assert!(!later, "a replay must not reach the writer");
        assert_eq!(first.result, again.result);

        let pending = import_while_pending(&mut core);
        assert_eq!(
            pending.error.expect("in flight").code,
            tasty_ipc::protocol::ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN
        );
    }

    fn put_request(id: u64, key: &str) -> JsonRpcRequest {
        make_request(
            id,
            "memory.put",
            json!({"scope": "global", "key": key, "value": "v"}),
            None,
        )
    }

    fn error_of(response: JsonRpcResponse) -> (i32, String, Option<Value>) {
        let e = response.error.expect("error response");
        (e.code, e.message, e.data)
    }

    // 종료 중 접수 거절: -32603, writer:closed.
    #[test]
    fn a_closed_writer_answers_with_the_closed_error() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core();
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(
            holds_by_every(deadline, Duration::from_millis(1), |_| {
                core.poll_memory_writer_shutdown() == 0
            }),
            "writer did not stop"
        );
        let (response, later) = run_request(&mut core, &put_request(1, "k"));
        assert!(!later);
        let (code, message, data) = error_of(response);
        assert_eq!(code, -32603);
        assert_eq!(message, "memory writer is not accepting requests");
        assert_eq!(
            data,
            Some(json!({"storage_failure": "other", "writer": "closed"}))
        );
    }

    // 큐 포화: -32603, storage_failure:busy(재시도 가능), writer:saturated. 먼저 접수한 요청은 그대로 끝난다.
    #[test]
    fn a_saturated_writer_answers_busy_and_keeps_the_accepted_request() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core_with(
            Arc::new(Mutex::new(
                tasty_memory::MemoryStore::open_in_memory().expect("store"),
            )),
            1,
        );
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let caller = CallerContext::local();
        let first_req = put_request(1, "a");
        let checked = crate::ipc::handler::check_request(
            &mut core,
            &mut state,
            &mut engine,
            &first_req,
            &caller,
        )
        .unwrap_or_else(|r| panic!("request rejected: {r:?}"));
        let Handled::Later(first) = crate::ipc::handler::handle_checked_request(
            &mut core,
            &mut state,
            &mut engine,
            &checked,
        ) else {
            panic!("the first request was not deferred");
        };
        let second_req = put_request(2, "b");
        let checked = crate::ipc::handler::check_request(
            &mut core,
            &mut state,
            &mut engine,
            &second_req,
            &caller,
        )
        .unwrap_or_else(|r| panic!("request rejected: {r:?}"));
        let Handled::Now(refused) = crate::ipc::handler::handle_checked_request(
            &mut core,
            &mut state,
            &mut engine,
            &checked,
        ) else {
            panic!("a request over the budget was accepted");
        };
        let (code, message, data) = error_of(refused);
        assert_eq!(code, -32603);
        assert!(
            message.starts_with("memory writer queue is full ("),
            "{message}"
        );
        assert_eq!(
            data,
            Some(json!({"storage_failure": "busy", "writer": "saturated"}))
        );
        let (tx, rx) = mpsc::sync_channel(1);
        core.defer_reply(first, ReplyTo::Ipc(tx));
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut accepted = None;
        assert!(
            holds_by_every(deadline, Duration::from_millis(1), |_| {
                core.poll_memory_replies(None);
                accepted = rx.try_recv().ok();
                accepted.is_some()
            }),
            "deferred reply never arrived"
        );
        let accepted = accepted.expect("checked above");
        assert_eq!(accepted.result.expect("put")["version"], json!(1));
    }

    // 요청 본문 panic: -32603 'memory request panicked'. 시험 저장소의 미구현 import가 panic한다.
    #[test]
    fn a_panicking_request_answers_with_the_panic_error() {
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = writer_core_with(
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new())),
            usize::MAX,
        );
        let params = json!({
            "entries": [{"scope": "global", "key": "p", "kind": "text", "value": "v"}],
        });
        let (response, later) =
            run_request(&mut core, &make_request(1, "memory.import", params, None));
        assert!(later);
        let (code, message, data) = error_of(response);
        assert_eq!(code, -32603);
        assert_eq!(message, "memory request panicked");
        assert_eq!(data, Some(json!({"storage_failure": "other"})));
    }

    // 결과 불명: 원인 오류가 있으면 기존 문구에 outcome:unknown을, worker 소실이면 고정 문구를 쓴다.
    #[test]
    fn unknown_outcomes_say_the_write_may_have_landed() {
        use tasty_memory::writer::Failure;
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let expected = format!("memory db error: {busy}");
        let cause = Arc::new(tasty_memory::MemoryError::Db(busy));
        let (code, message, data) = error_of(super::unsettled_response(
            json!(1),
            &Failure::Unknown(Some(cause)),
            super::super::memory::map_error_ref,
        ));
        assert_eq!(code, -32603);
        assert_eq!(message, expected);
        assert_eq!(
            data,
            Some(json!({"storage_failure": "busy", "outcome": "unknown"}))
        );

        let (code, message, data) = error_of(super::unsettled_response(
            json!(2),
            &Failure::Unknown(None),
            super::super::memory::map_error_ref,
        ));
        assert_eq!(code, -32603);
        assert_eq!(
            message,
            "memory writer ended before the request finished; it may have been stored"
        );
        assert_eq!(
            data,
            Some(json!({"storage_failure": "other", "outcome": "unknown"}))
        );

        // 저장 실패와 요청 거절은 기존 매핑 그대로다.
        let refused = Arc::new(tasty_memory::MemoryError::NotFound {
            scope: "global".into(),
            key: "k".into(),
        });
        let (code, _, data) = error_of(super::unsettled_response(
            json!(3),
            &Failure::Rejected {
                error: refused,
                index: None,
            },
            super::super::memory::map_error_ref,
        ));
        assert_eq!((code, data), (-32004, None));
    }

    // 세 문서의 오류 표가 서로 같고, 고정 문구가 코드가 만드는 문구와 같다.
    #[test]
    fn the_writer_error_table_matches_in_every_document_and_in_code() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let docs = [
            "docs/design/systems/storage-contract.md",
            "docs/reference/api.md",
            "docs/dev-guide/headless-ipc-surface.md",
        ];
        let tables: Vec<Vec<String>> = docs
            .iter()
            .map(|path| {
                let text = std::fs::read_to_string(root.join(path))
                    .unwrap_or_else(|e| panic!("{path}: {e}"));
                let start = text
                    .find("| 큐 포화(")
                    .unwrap_or_else(|| panic!("{path} has no writer error table"));
                text[start..]
                    .lines()
                    .take_while(|l| l.starts_with('|'))
                    .map(str::to_owned)
                    .collect()
            })
            .collect();
        assert_eq!(tables[0].len(), 6, "{:#?}", tables[0]);
        for (path, table) in docs.iter().zip(&tables).skip(1) {
            assert_eq!(table, &tables[0], "{path} differs from {}", docs[0]);
        }
        let closed =
            super::submit_error_response(json!(1), &tasty_utils::batch_writer::SubmitError::Closed);
        let saturated = super::submit_error_response(
            json!(1),
            &tasty_utils::batch_writer::SubmitError::Saturated { items: 7, bytes: 9 },
        );
        let lost = super::unsettled_response(
            json!(1),
            &tasty_memory::writer::Failure::Unknown(None),
            super::super::memory::map_error_ref,
        );
        let panicked = super::unsettled_response(
            json!(1),
            &tasty_memory::writer::Failure::Panicked,
            super::super::memory::map_error_ref,
        );
        let documented = tables[0].join("\n");
        let documented_data: Vec<Value> = documented
            .split('`')
            .filter(|cell| cell.starts_with('{'))
            .map(|cell| serde_json::from_str(cell).unwrap_or_else(|e| panic!("{cell}: {e}")))
            .collect();
        assert_eq!(documented_data.len(), 4, "{documented_data:?}");
        for response in [closed, lost, panicked] {
            let (_, message, data) = error_of(response);
            assert!(documented.contains(&format!("`{message}`")), "{message}");
            let data = data.expect("data");
            assert!(documented_data.contains(&data), "{data}");
        }
        let (_, message, data) = error_of(saturated);
        assert_eq!(
            message,
            "memory writer queue is full (7 requests, 9 bytes held)"
        );
        assert!(documented_data.contains(&data.expect("data")));
    }

    /// 결과를 거두기 전에 같은 키로 다시 보낸 요청의 응답.
    fn import_while_pending(core: &mut AppServices) -> JsonRpcResponse {
        let params = json!({
            "entries": [{"scope": "global", "key": "idem2", "kind": "text", "value": "v"}],
        });
        let req = make_request(2, "memory.import", params, Some("pending-idem"));
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let caller = CallerContext::local();
        let checked =
            crate::ipc::handler::check_request(core, &mut state, &mut engine, &req, &caller)
                .unwrap_or_else(|r| panic!("request rejected: {r:?}"));
        let first =
            crate::ipc::handler::handle_checked_request(core, &mut state, &mut engine, &checked);
        let Handled::Later(deferred) = first else {
            panic!("writer route answered without deferring");
        };
        let checked =
            crate::ipc::handler::check_request(core, &mut state, &mut engine, &req, &caller)
                .unwrap_or_else(|r| panic!("request rejected: {r:?}"));
        let second =
            crate::ipc::handler::handle_checked_request(core, &mut state, &mut engine, &checked);
        let (tx, rx) = mpsc::sync_channel(1);
        core.defer_reply(deferred, ReplyTo::Ipc(tx));
        // 원래 순서는 받기 → 거두기 → 기한 검사다. 기한 뒤 관측 읽기는 새 제출 없는 거두기 뒤에 받는다.
        let deadline = Instant::now() + Duration::from_secs(30);
        assert!(
            holds_by_every(deadline, Duration::from_millis(1), |last| {
                if last {
                    core.poll_memory_replies(None);
                }
                if rx.try_recv().is_ok() {
                    return true;
                }
                if !last {
                    core.poll_memory_replies(None);
                }
                false
            }),
            "deferred reply never arrived"
        );
        match second {
            Handled::Now(response) => response,
            Handled::Later(_) => panic!("a retry in flight reached the writer"),
        }
    }
}
