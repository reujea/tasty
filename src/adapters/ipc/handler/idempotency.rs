//! 같은 변경 요청의 재시도가 **중복 실행인지 재조회인지** 갈리게 하는 보존소.
//!
//! ## 이 모듈이 없을 때 무엇이 구별되지 않았나
//!
//! 응답만 유실된 요청을 호출자가 다시 보내면, 받는 쪽에는 그것이 새 요청인지 재시도인지
//! 말해 주는 것이 **하나도 없었다.** `id` 는 응답 대응용이라 연결마다 다시 매겨지고,
//! params 가 같다는 사실은 "같은 일을 두 번 하려는 것" 과 구별되지 않는다. 그래서
//! 재시도는 그대로 두 번째 효과를 남겼고, 호출자는 그것을 관측할 수단도 없었다.
//!
//! 구별을 만들 수 있는 것은 **호출자뿐**이다 — 자기가 보내는 두 요청이 같은 것인지는
//! 자기만 안다. [`tasty_ipc::protocol::JsonRpcRequest::idempotency_key`] 가 그 선언이고,
//! 이 모듈은 그 선언을 받아 **처음 본 키만 실행**한다.
//!
//! ## 무엇에 대해 도는가 — `Mutate` 는 상한이고, 배선이 그것을 더 좁힌다
//!
//! 키가 뜻을 가질 **수 있는** 메서드는 [`MethodEffect::Mutate`] 로 분류된 것뿐이다.
//! 나머지 둘은 정의상 재전달이 안전하고(읽기는 흔적을 안 남기고, 멱등은 같은 끝 상태로
//! 수렴한다), 보존소에 넣으면 오히려 **조회가 낡은 답을 받는다.** 그 분류가 이미 표에
//! 있으므로 여기서 목록을 다시 만들지 않는다 — 두 목록이면 갈리고, 갈렸을 때 조용하다.
//!
//! ★ **그 상한과 실제로 도는 범위는 경로마다 따로 배선된다.** 호스트가 키를 실은 요청을
//! 끝내는 경로는 넷이고, 이 모듈을 부르는 자리가 경로마다 있다:
//!
//! - **engine 라우터** — [`super::route_checked_request`] 가 [`begin`]/[`finish`] 를 부른다.
//!   [`super::handle_checked_request`] 를 지나는 모든 요청이다.
//! - **App 층** — 창 생성·화면 캡처·plugin 설치·원격 attach 처럼 `App` 이 직접 끝내는
//!   메서드. GUI 의 app_methods step 과 헤드리스의 App 층 가로채기가 각각 첫 줄에서
//!   [`run_app_layer`] 를 부른다(ADR-0421). 답을 **나중에** 보내는 메서드가 있어서 진행
//!   중 상태가 여기서 생긴다 — 아래 "동시에 같은 키가 둘 오면".
//! - **GUI debug step** — app_methods step **뒤**의 두 step(`debug_methods` ·
//!   `window_required`, 사용자 입력 재현이라 release 에 없다). 두 step 을 한 함수로 묶어 그
//!   첫 줄에서 [`run_app_layer`] 를 부른다. 헤드리스에는 이 step 이 따로 없다 — 그 빌드의
//!   debug 이름은 App 층 가로채기 안에서 끝난다(ADR-0566).
//! - **plugin namespace forward** — plugin 이 점유한 prefix 아래의 이름. 두 무리가 있다.
//!   표가 아는 호스트 메서드인데 prefix 를 번들 plugin 이 점유한 것(`image.open` 등 — plugin
//!   이 받아 호스트로 되부른다)은 [`forward_keeping_the_key`] 가 보존소를 지나게 한다.
//!   표가 모르는 plugin 고유 이름은 호스트가 뜻을 모르므로 개입하지 않고, 이름 표가 계약
//!   **밖**이라고 선언한다(ADR-0361).
//!   forward 가 실행 전에 거절한 응답(plugin 이 안 떠 있어 검증에서 멈춘 것 등)도 relay 가
//!   결말로 기록하므로, 키 수명 안의 재시도는 plugin 이 되살아난 뒤에도 그 거절을 재생으로
//!   받는다 — 이 경로만의 정책이 아니라 engine 라우터 · App 층도 응답 종류를 가리지 않고 기록한다.
//!
//! 그래서 **`Mutate` 중 보존소를 안 지나는 것은 plugin 고유 이름뿐이다.** 호스트가 아는
//! `Mutate` 이름은 어느 경로로 끝나든 지난다 — 그 경로가 받기 시작한 판은 이름 표의
//! `KeyContract::Kept { since }` 가 말한다. `Read` · `Idempotent` 이름은 위 상한 밖이라 어느
//! 경로에서도 보존소에 안 들어간다(`KeyContract::Unneeded`).
//!
//! ## 동시에 같은 키가 둘 오면
//!
//! 수렴한다. 그 수렴을 누가 만드는가는 층마다 다르다.
//!
//! - **engine 라우터**에서는 이 모듈이 만드는 것이 아니다 — 실행이
//!   [`super::handle_checked_request`] 한 자리로 모이고 거기서 직렬로 돈다. 둘째 요청이
//!   보존소를 볼 때 첫째는 이미 끝나 있다.
//! - **App 층**에서는 이 모듈이 만든다. 창 생성의 답은 winit 핸들러가, 원격 attach 의 답은
//!   워커가 나중에 보내므로 그 사이에 같은 키가 올 수 있다. 그때 둘째는 첫 실행에
//!   **합류**하고(두 번째 실행 없음), 첫 결말이 나오면 그것을 재생 표지와 함께 받는다.
//!   진행 중을 뜻하는 값([`Decision::InFlight`])은 이 층이 열 때만 생긴다.
//!
//! ## 보장의 경계 — 없는 것을 약속하지 않는다
//!
//! 보존소는 프로세스 메모리에 있고 상한이 셋이다(보존 시간 · 항목 수 · 총 바이트).
//! 경계 밖에서 일어나는 일은 둘이고 **답이 다르다**:
//!
//! - 항목이 밀려났으면 그 키는 **처음 보는 키와 구별되지 않는다** → 다시 실행된다.
//!   구별하려면 밀려난 키를 영원히 기억해야 하고, 그것은 상한이 있다는 말과 모순이다.
//! - 답만 버렸으면(너무 커서) 키는 남는다 → [`ERR_IDEMPOTENT_RESULT_DISCARDED`] 로
//!   **"실행은 됐고 답이 없다"** 를 말한다. 이쪽은 재전송이 답이 아니다.
//!
//! 호스트가 재시작하면 전부 사라진다. 그 경계를 호출자가 읽을 수 있게 [`declaration`]
//! 이 값으로 내놓고 `system.info` 가 싣는다.

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

use tasty_ipc::caller::CallerContext;
use tasty_ipc::method_meta::{MethodEffect, method_meta};
use tasty_ipc::protocol::{
    ERR_IDEMPOTENCY_KEY_CONFLICT, ERR_IDEMPOTENT_RESULT_DISCARDED,
    ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN, JsonRpcRequest, JsonRpcResponse,
};
use tasty_ipc::server::{IpcCommand, send_response};

/// 보관한 답이 유효한 시간.
///
/// 값의 근거는 이 보존소가 막으려는 사건의 수명이다 — 응답을 놓친 호출자가 다시 거는
/// 간격(연결 재수립 + 재시도 백오프)이고, 사람이 손으로 다시 거는 경우까지 덮으려면
/// 분 단위여야 한다. 더 길게 잡을 근거는 측정된 적이 없고, 늘리면 항목 수 상한이
/// 먼저 걸려 **시간이 아니라 용량이 경계를 정하게 된다**(그쪽은 호출자가 예측할 수
/// 없다).
pub(crate) const RETENTION: Duration = Duration::from_secs(600);

/// 동시에 기억하는 키의 수.
pub(crate) const CAPACITY: usize = 256;

/// 답 하나를 보관할 최대 바이트. 넘으면 키만 남기고 답을 버린다.
///
/// `Mutate` 중에는 터미널 출력을 통째로 내는 것이 있다(`surface.read_since_scan_mark`).
/// 그런 답까지 항목마다 들고 있으면 상한이 항목 수로는 안 선다.
pub(crate) const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024;

/// 보관한 답 전체의 합 상한. 항목 수 상한과 **둘 다** 건다 — 한쪽만으로는 작은 답
/// 256 개와 큰 답 256 개가 같은 상한 아래 놓인다.
pub(crate) const TOTAL_BUDGET_BYTES: usize = 4 * 1024 * 1024;

/// 키 문자열의 최대 바이트. 키는 호출자가 고르므로 상한이 없으면 보존소의 크기를
/// 호출자가 정하게 된다.
pub(crate) const MAX_KEY_BYTES: usize = 256;

/// 보관 중인 한 요청의 결말.
#[derive(Debug)]
enum Stored {
    /// 답을 그대로 들고 있다.
    Response(Box<JsonRpcResponse>),
    /// 실행은 됐지만 답이 상한을 넘어 버렸다.
    Discarded,
    /// 실행이 시작됐고 답이 아직 안 왔다. App 층의 지연 응답 메서드에서만 생긴다 —
    /// [`run_app_layer`] 참조.
    InFlight {
        /// 이 실행을 연 쪽의 표. 늦게 온 결말이 **자기 자리**에만 기록되게 한다.
        ticket: u64,
        /// 이 실행에 합류한 재시도. 결말이 나면 그것을 재생으로 받는다.
        waiters: Vec<Waiter>,
    },
}

/// 진행 중인 실행에 합류한 재시도 하나 — 답할 `id` 와 답할 통로.
#[derive(Debug)]
struct Waiter {
    id: serde_json::Value,
    reply: mpsc::SyncSender<JsonRpcResponse>,
}

/// 결말을 기록한 뒤 합류자에게 나눠 줄 것.
#[derive(Debug, Default)]
struct Handoff {
    waiters: Vec<Waiter>,
    /// 답을 보관했는가(거짓이면 상한을 넘어 버렸다).
    kept: bool,
}

#[derive(Debug)]
struct Entry {
    /// 키를 고른 주체. [`caller_scope`] 참조.
    scope: String,
    key: String,
    /// `(메서드, params)` 의 다이제스트. 같은 키에 다른 요청이 붙는 것을 잡는다.
    ///
    /// 원문을 안 들고 다이제스트로 견주는 이유는 params 가 요청 줄 상한까지 커질 수
    /// 있어서다. 다이제스트가 충돌하면 다른 요청이 앞선 답을 받지만, 그러려면 호출자가
    /// **이미 키를 잘못 재사용한 상태**여야 하고(맞게 쓰면 params 도 같다) 그 위에
    /// 64 비트 충돌이 겹쳐야 한다. 그 조합에 원문 보관 비용을 쓰지 않는다.
    digest: u64,
    stored_at: Instant,
    outcome: Stored,
    /// 이 항목이 [`TOTAL_BUDGET_BYTES`] 에서 차지하는 몫.
    bytes: usize,
}

/// 키 하나에 대한 판정.
#[derive(Debug)]
pub(crate) enum Decision {
    /// 처음 보는 키다. 실행하고 이 다이제스트로 기록한다.
    Execute(u64),
    /// 같은 키·같은 요청의 답이 있다. **실행하지 않는다.**
    Replay(Box<JsonRpcResponse>),
    /// 같은 키인데 요청이 다르다.
    Conflict,
    /// 실행은 됐고 답은 버려졌다.
    Discarded,
    /// 같은 키·같은 요청의 실행이 **아직 끝나지 않았다.**
    InFlight,
}

/// 키를 실은 요청을 보존소가 **어떻게 판정했는가** 의 프로세스 누계. [`Decision`] 한
/// 갈래에 칸 하나다.
///
/// 재시도가 실제로 얼마나 오는지(RF16 의 "재시도" 축)를 이 값이 답한다. `executed` 를 함께
/// 두는 이유는 모수다 — 재생 수만으로는 그것이 키 실은 실행 열 건 중 하나인지 만 건 중
/// 하나인지 모른다.
///
/// 레이블(메서드 · 주체)은 없다. 키는 호출자가 고르는 문자열이고 주체는 agent id 라, 그것을
/// 레이블로 달면 칸 수를 호출자가 정한다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RetryCounts {
    /// 처음 보는 키 — 실행했다([`Decision::Execute`]).
    pub(crate) executed: u64,
    /// 같은 키·같은 요청 — 실행하지 않고 보관된 답을 냈다([`Decision::Replay`]).
    pub(crate) replayed: u64,
    /// 같은 키·다른 요청 — 아무것도 실행하지 않았다([`Decision::Conflict`]).
    pub(crate) conflicted: u64,
    /// 실행은 됐고 답은 버려졌다 — 실행하지 않았다([`Decision::Discarded`]).
    pub(crate) discarded: u64,
    /// 같은 키·같은 요청이 진행 중이었다 — App 층에서는 합류했고, engine 라우터에서는
    /// (도달하지 않지만) 결과 불명으로 답했다([`Decision::InFlight`]).
    pub(crate) in_flight: u64,
}

/// 키를 받는 보존소. 프로세스에 하나다.
#[derive(Debug, Default)]
pub(crate) struct Store {
    /// 삽입 순서. 밀어내는 쪽은 항상 앞이다.
    entries: VecDeque<Entry>,
    total_bytes: usize,
    /// 다음에 줄 진행 중 표.
    next_ticket: u64,
    /// 판정 누계. [`Store::decide`] 가 갈래마다 센다 — 판정이 그 함수 하나에서 나오므로
    /// 층(engine 라우터 · App 층 · GUI debug 묶음 · forward)마다 세는 자리를 따로 두지 않는다.
    /// 단 한 요청이 여러 층의 판정을 지나는 갈래가 있어(App 층이나 GUI debug 묶음이 이름을
    /// 안 맡아 다음 층으로 넘긴다), 그 갈래는 [`Store::abandon_unhandled`] 가 앞 층의 셈을
    /// 되돌린다.
    counts: RetryCounts,
}

impl Store {
    pub(crate) const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            total_bytes: 0,
            next_ticket: 0,
            counts: RetryCounts {
                executed: 0,
                replayed: 0,
                conflicted: 0,
                discarded: 0,
                in_flight: 0,
            },
        }
    }

    /// 이 키로 무엇을 할지 정한다. 만료된 항목은 이 자리에서 걷힌다.
    pub(crate) fn decide(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> Decision {
        self.purge_expired(now);
        let digest = digest(method, params);
        let decision = match self
            .entries
            .iter()
            .find(|e| e.scope == scope && e.key == key)
        {
            None => Decision::Execute(digest),
            Some(e) if e.digest != digest => Decision::Conflict,
            Some(e) => match &e.outcome {
                Stored::Response(r) => Decision::Replay(r.clone()),
                Stored::Discarded => Decision::Discarded,
                Stored::InFlight { .. } => Decision::InFlight,
            },
        };
        let slot = match decision {
            Decision::Execute(_) => &mut self.counts.executed,
            Decision::Replay(_) => &mut self.counts.replayed,
            Decision::Conflict => &mut self.counts.conflicted,
            Decision::Discarded => &mut self.counts.discarded,
            Decision::InFlight => &mut self.counts.in_flight,
        };
        *slot = slot.saturating_add(1);
        decision
    }

    /// 판정 누계의 사본.
    pub(crate) fn counts(&self) -> RetryCounts {
        self.counts
    }

    /// 실행을 연다 — 결말이 올 때까지 이 키는 [`Decision::InFlight`] 로 답한다.
    fn open(&mut self, now: Instant, scope: &str, key: &str, digest: u64) -> u64 {
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        self.remove(scope, key);
        self.entries.push_back(Entry {
            scope: scope.to_string(),
            key: key.to_string(),
            digest,
            stored_at: now,
            outcome: Stored::InFlight {
                ticket,
                waiters: Vec::new(),
            },
            bytes: 0,
        });
        self.enforce_bounds();
        ticket
    }

    /// 진행 중인 실행에 재시도를 합류시킨다. 그 자리가 이미 진행 중이 아니면 거짓이다.
    fn join(&mut self, scope: &str, key: &str, waiter: Waiter) -> Result<(), Waiter> {
        match self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.key == key)
            .map(|e| &mut e.outcome)
        {
            Some(Stored::InFlight { waiters, .. }) => {
                waiters.push(waiter);
                Ok(())
            }
            _ => Err(waiter),
        }
    }

    /// 연 실행을 결말 없이 닫는다. 그 자리가 **이 표의** 진행 중일 때만 걷는다 — 이미
    /// 밀려나 다른 실행이 그 키를 쥐었으면 그쪽을 건드리지 않는다. 합류자의 통로는
    /// 함께 버려진다(첫 요청이 답을 못 받는 것과 같은 결말이다).
    fn abandon(&mut self, scope: &str, key: &str, ticket: u64) {
        if let Some(i) = self.entries.iter().position(|e| {
            e.scope == scope
                && e.key == key
                && matches!(e.outcome, Stored::InFlight { ticket: t, .. } if t == ticket)
        }) && let Some(e) = self.entries.remove(i)
        {
            self.total_bytes -= e.bytes;
        }
    }

    /// 이 층이 그 이름을 **안 맡은** 실행을 닫는다 — [`Store::abandon`] 에 더해, 연 판정이
    /// 올린 `executed` 를 되돌린다.
    ///
    /// 그 요청은 실행되지 않았다. 다음 층으로 가서 거기서 다시 판정되고(다음 층도 보존소를
    /// 지나면 처음 보는 키로 한 번 더 센다), 보존소를 안 지나는 이름(plugin 고유 이름)이면
    /// 아무 데서도 실행을 세지 않는 것이 맞다. 되돌리지 않으면 한 요청이 두 번 세지거나,
    /// 계약 밖 호출이 모수에 섞인다(ADR-0422).
    fn abandon_unhandled(&mut self, scope: &str, key: &str, ticket: u64) {
        self.abandon(scope, key, ticket);
        self.counts.executed = self.counts.executed.saturating_sub(1);
    }

    /// 라우팅에 들어간 뒤 정해진 답을 그 키로 기록한다.
    ///
    /// "handler 의 답" 이 아니라 그렇게 적는 이유는 갈래가 하나 더 있기 때문이다 —
    /// 라우터가 이름을 못 찾은 답(`-32601`/`-32017`)도 이 자리로 온다. 그것도 이
    /// `(주체, 키, 요청)` 에 대한 **종결된 답**이라 보관 대상이 맞다.
    ///
    /// 게이트(권한·cap·rate)가 낸 거절은 여기 안 온다 — 그것들은 라우팅 **전에** 끝나므로
    /// 보관할 결말이 없고, 보관하면 권한이 생긴 뒤의 재시도까지 옛 거절을 받는다.
    pub(crate) fn record(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        digest: u64,
        response: &JsonRpcResponse,
    ) {
        self.settle(now, scope, key, digest, None, response);
    }

    /// [`Store::record`] 의 본체. `ticket` 은 [`Store::open`] 이 준 표다.
    ///
    /// 표가 있는데 그 자리가 **다른 표의** 진행 중이면 기록하지 않는다 — 이 실행은
    /// 상한에 밀려났고 그 키는 이미 다른 실행이 쥐었다. 그쪽의 결말이 그 자리를 채운다.
    fn settle(
        &mut self,
        now: Instant,
        scope: &str,
        key: &str,
        digest: u64,
        ticket: Option<u64>,
        response: &JsonRpcResponse,
    ) -> Handoff {
        let mut handoff = Handoff::default();
        let slot = self
            .entries
            .iter_mut()
            .find(|e| e.scope == scope && e.key == key);
        if let (Some(mine), Some(Entry { outcome, .. })) = (ticket, slot) {
            match outcome {
                Stored::InFlight { ticket: t, waiters } if *t == mine => {
                    handoff.waiters = std::mem::take(waiters);
                }
                Stored::InFlight { .. } => return handoff,
                _ => {}
            }
        }
        // 같은 키의 옛 항목은 남기지 않는다 — 앞의 것이 남아 있으면 `decide` 가
        // 둘 중 어느 것을 볼지가 삽입 순서에 달린다.
        self.remove(scope, key);
        let size = serde_json::to_string(response)
            .map(|s| s.len())
            .unwrap_or(0);
        let (outcome, bytes) = if size > MAX_STORED_RESPONSE_BYTES {
            (Stored::Discarded, 0)
        } else {
            (Stored::Response(Box::new(response.clone())), size)
        };
        handoff.kept = matches!(outcome, Stored::Response(_));
        self.entries.push_back(Entry {
            scope: scope.to_string(),
            key: key.to_string(),
            digest,
            stored_at: now,
            outcome,
            bytes,
        });
        self.total_bytes += bytes;
        self.enforce_bounds();
        handoff
    }

    fn remove(&mut self, scope: &str, key: &str) {
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.scope == scope && e.key == key)
        {
            if let Some(e) = self.entries.remove(i) {
                self.total_bytes -= e.bytes;
            }
        }
    }

    fn purge_expired(&mut self, now: Instant) {
        // 앞에서부터 삽입 순이므로 만료도 앞에서부터다.
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.stored_at) < RETENTION {
                break;
            }
            self.pop_front();
        }
    }

    fn enforce_bounds(&mut self) {
        while self.entries.len() > CAPACITY || self.total_bytes > TOTAL_BUDGET_BYTES {
            if self.pop_front().is_none() {
                break;
            }
        }
    }

    fn pop_front(&mut self) -> Option<Entry> {
        let e = self.entries.pop_front()?;
        self.total_bytes -= e.bytes;
        Some(e)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// `(메서드, params)` 의 프로세스 안 다이제스트.
///
/// 값은 프로세스 밖으로 안 나가므로 해시 함수의 안정성이 계약이 아니다 — 같은 실행
/// 안에서 같은 입력이 같은 값을 내면 된다.
fn digest(method: &str, params: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    method.hash(&mut h);
    // `serde_json::Value` 는 항상 직렬화된다(NaN 을 담을 수 없다). 그래도 실패를
    // 무시하지 않는 이유는 그때 모든 params 가 같은 값으로 접혀 **서로 다른 요청이
    // 같은 키로 수렴**하기 때문이다 — 길이를 함께 섞어 그 접힘을 막는다.
    match serde_json::to_string(params) {
        Ok(s) => {
            s.hash(&mut h);
        }
        Err(_) => {
            // 직렬화가 안 되는 값은 원문으로 못 견주므로 구조를 그대로 센다.
            format!("{params:?}").hash(&mut h);
        }
    }
    h.finish()
}

/// 키를 고른 **주체**. 보존소의 자리는 `(주체, 키)` 로 정해진다.
///
/// 키는 호출자가 고르는 임의의 문자열이라, 주체를 빼면 서로 모르는 두 호출자가 같은
/// 문자열을 골랐을 때 한쪽이 **다른 쪽의 답**을 받는다. 그 자리는 조용하다 — 답이
/// 성공이고 모양도 맞다.
///
/// 연결 단위로 가르지 않는 이유는 이 계약의 목적 자체다. 재시도는 응답을 놓친 뒤에
/// 오므로 **다른 연결**로 온다 — 연결로 가르면 키가 한 번도 안 맞는다.
///
/// `Local` 이 하나로 묶이는 것은 그것이 한 주체이기 때문이다(사용자). CLI 를 두 번
/// 부른 것과 네트워크로 붙은 것이 같은 주체라는 판정은 이 레포가 이미 하고 있다 —
/// `Local` 은 권한 검사를 통째로 건너뛴다.
fn caller_scope(caller: &CallerContext) -> String {
    match caller {
        CallerContext::Local => "local".to_string(),
        CallerContext::Plugin { plugin_id, .. } => format!("plugin:{plugin_id}"),
        CallerContext::Agent { agent_id, .. } => format!("agent:{agent_id}"),
    }
}

static STORE: Mutex<Store> = Mutex::new(Store::new());
static POISON_REPORTED: AtomicBool = AtomicBool::new(false);
const WHAT: &str = "the IPC idempotency store";

fn store() -> MutexGuard<'static, Store> {
    lock(&STORE)
}

/// 보존소 하나를 잠근다. 프로세스 보존소는 [`STORE`] 하나지만, 판정 함수들은 그것을
/// 인자로 받는다 — 층을 차례로 지나는 흐름의 누계를 시험이 **정확한 값**으로 재려면
/// 다른 시험과 안 섞이는 보존소가 필요하다(전역 보존소는 병렬 시험이 함께 올린다).
fn lock(store: &'static Mutex<Store>) -> MutexGuard<'static, Store> {
    tasty_utils::poison::recover_mutex(store.lock(), WHAT, &POISON_REPORTED)
}

/// 실행을 기다리는 키. [`begin`] 이 주고 [`finish`] 가 받는다.
#[derive(Debug)]
pub(crate) struct Pending {
    scope: String,
    key: String,
    digest: u64,
}

/// 봉투의 멱등 키가 **모양으로** 유효한가 — 길이 밖 키(빈 문자열 · 상한 초과)는
/// `-32602` 다. 메서드도 보존소도 안 본다.
///
/// 부르는 자리는 진입 게이트([`super::check_request`] · `check_without_engine`)다 — 요청이
/// App 층 · plugin namespace forward · engine 라우터 중 어디로 가든 그 앞이다. 이 검사가
/// [`begin`] 안에 있던 때에는 보존소가 닿는 범위(engine 라우터)만 물려받아, 같은 봉투가
/// 목적지에 따라 유효하기도 무효하기도 했다(ADR-0420).
pub(crate) fn check_envelope(
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<(), JsonRpcResponse> {
    let Some(key) = request.idempotency_key.as_deref() else {
        return Ok(());
    };
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(JsonRpcResponse::invalid_params(
            id.clone(),
            format!(
                "idempotency_key must be 1..={MAX_KEY_BYTES} bytes (got {})",
                key.len()
            ),
        ));
    }
    Ok(())
}

/// 라우팅 **전에** 이 요청을 실행해도 되는지 정한다.
///
/// `Ok(None)` 은 "키가 없거나 이 메서드에 뜻이 없다" 이고, 그때 동작은 이 모듈이
/// 생기기 전과 한 글자도 다르지 않다.
///
/// 키의 모양은 여기서 다시 안 본다 — 요청은 진입 게이트의 [`check_envelope`] 를 이미
/// 지났다. 판정을 두 자리에 두면 한쪽만 고쳐진다.
pub(crate) fn begin(
    now: Instant,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<Option<Pending>, JsonRpcResponse> {
    begin_in(&STORE, now, caller, request, id)
}

fn begin_in(
    store: &'static Mutex<Store>,
    now: Instant,
    caller: &CallerContext,
    request: &JsonRpcRequest,
    id: &serde_json::Value,
) -> Result<Option<Pending>, JsonRpcResponse> {
    let Some(key) = request.idempotency_key.as_deref() else {
        return Ok(None);
    };
    // 표가 이 이름을 모르면 라우팅도 못 한다 — 그 답은 라우터가 낸다.
    if method_meta(&request.method).map(|m| m.effect) != Some(MethodEffect::Mutate) {
        return Ok(None);
    }
    let scope = caller_scope(caller);
    match lock(store).decide(now, &scope, key, &request.method, &request.params) {
        Decision::Execute(digest) => Ok(Some(Pending {
            scope,
            key: key.to_string(),
            digest,
        })),
        Decision::Replay(stored) => Err(stored.replayed_for(id.clone())),
        Decision::Conflict => Err(conflict_error(key, id)),
        Decision::Discarded => Err(discarded_error(key, id)),
        // engine 라우터에서는 도달하지 않는다 — 진행 중은 App 층의 지연 응답만 열고, 같은
        // `(주체, 키, 요청)` 은 같은 메서드라 같은 층으로 간다. 라우터는 동기라 기다릴 수
        // 없으므로, 닿는다면 결과 불명으로 답한다(재전송하지 말고 조회하라는 뜻이다).
        Decision::InFlight => Err(still_running_error(key, id)),
    }
}

fn conflict_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_IDEMPOTENCY_KEY_CONFLICT,
        format!(
            "idempotency_key '{key}' was already used for a different request; \
             use a new key for a new request"
        ),
    )
}

fn discarded_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_IDEMPOTENT_RESULT_DISCARDED,
        format!(
            "the request for idempotency_key '{key}' did run, but its response exceeded \
             {MAX_STORED_RESPONSE_BYTES} bytes and was not kept: do not resend, query instead"
        ),
    )
}

fn still_running_error(key: &str, id: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse::error(
        id.clone(),
        ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN,
        format!(
            "the request for idempotency_key '{key}' is still running and its outcome is not \
             known yet: do not resend, query instead"
        ),
    )
}

/// 실행이 끝났다. [`begin`] 이 키를 줬을 때만 기록한다.
pub(crate) fn finish(now: Instant, pending: Option<Pending>, response: &JsonRpcResponse) {
    finish_in(&STORE, now, pending, response);
}

fn finish_in(
    store: &'static Mutex<Store>,
    now: Instant,
    pending: Option<Pending>,
    response: &JsonRpcResponse,
) {
    if let Some(p) = pending {
        lock(store).record(now, &p.scope, &p.key, p.digest, response);
    }
}

/// App 층 메서드가 보존소를 지나게 한다 — engine 라우터의 [`begin`]/[`finish`] 와 같은
/// 판정을, **응답을 나중에 보내는** 메서드에도 걸리게 하는 자리.
///
/// ## 왜 [`begin`]/[`finish`] 를 그대로 못 쓰나
///
/// App 층 메서드는 답을 반환하지 않고 **통로에 보낸다**. 창 생성은 winit 핸들러가
/// 나중에 완료 채널로, 원격 attach 는 워커 스레드가 SSH 수립 뒤에 보낸다. 그래서 결말을
/// 기록할 자리가 "handler 가 돌아온 뒤" 가 아니라 "답이 통로에 들어온 뒤" 이고, 그
/// 사이에 같은 키의 재시도가 올 수 있다 — engine 라우터에서는 실행이 직렬이라 닿지
/// 않던 **진행 중** 상태가 여기서는 실재한다.
///
/// ## 어떻게 도는가
///
/// 1. 키가 없거나 `Mutate` 가 아니면 개입하지 않는다(`None`) — 호출자는 원래 본문을 돈다.
/// 2. 판정이 재생·충돌·버려짐이면 그 답을 원래 통로로 보내고 `answered` 를 돌려준다.
/// 3. 같은 키·같은 요청이 **진행 중**이면 그 실행에 합류한다. 답은 첫 실행의 결말이
///    나올 때 재생 표지와 함께 나간다 — 두 번째 실행은 없다. 이것이 동시에 온 같은 키를
///    **한 실행으로 수렴시키는** 자리다.
/// 4. 처음 보는 키면 진행 중을 열고, 통로를 relay 로 바꾸고 **키를 뗀** 사본으로
///    `dispatch` 를 부른다(떼지 않으면 `dispatch` 가 같은 함수로 돌아와 자기에게 합류한다).
///    `handled` 가 거짓이면(이 층이 그 이름을 안 맡았다) 연 자리를 닫고 그 판정의 셈을
///    되돌린다 — 요청은 다음 층으로 가고 거기서 다시 판정된다. 참이면 relay 가 답을 기다렸다가 기록하고, 합류자에게
///    나눠 주고, 원래 통로로 넘긴다.
///
/// relay 스레드는 본문을 부르기 **전에** 선다. 못 세우면 연 자리를 닫고 키를 뗀 요청을
/// 원래 통로로 부른다 — 보장만 잃고 동작은 키가 없을 때와 같다([`Relay::run`]).
///
/// 결말 없이 통로가 닫히면(답을 안 보내고 버린 경로) 연 자리를 닫는다. 합류자의 통로도
/// 함께 버려져 첫 요청과 같은 결말(응답 없이 연결 종료)을 받는다.
///
/// 시각은 `Instant::now()` 다 — relay 스레드는 `Core` 를 들고 있지 않고, 여는 쪽과 닫는
/// 쪽이 같은 원천을 봐야 보존 시간이 한 시계로 잰다.
pub(crate) fn run_app_layer<T>(
    caller: &CallerContext,
    cmd: &IpcCommand,
    answered: T,
    handled: impl FnOnce(&T) -> bool,
    dispatch: impl FnOnce(&IpcCommand) -> T,
) -> Option<T> {
    run_app_layer_in(
        &STORE,
        spawn_relay,
        caller,
        cmd,
        answered,
        handled,
        dispatch,
    )
}

/// plugin namespace forward 가 보존소를 지나게 한다 — 이름 표가 `Kept` 로 선언한 이름만.
///
/// 부르는 쪽은 그 이름을 plugin 이 점유했는지 **먼저** 정하고(점유하지 않았으면 engine
/// 라우터가 보존소를 지난다), 점유했으면 여기서 `forward` 로 넘긴다. 나가는 이름은 두 무리다.
///
/// - **표의 호스트 메서드**(`image.open` · `markdown.navigate` 등 — prefix 를 번들 plugin 이
///   점유했다). 표가 `Kept` 로 선언하므로 호출자는 키를 싣는다. plugin 이 받아 호스트로
///   되부르는데 그 되부름에는 키가 없다 — 여기서 안 걸면 재시도가 두 번째 실행이 된다. 답은
///   plugin 이 **나중에** 보내므로 App 층과 같은 진행 중 상태가 생기고, 그래서 같은 함수
///   ([`run_app_layer`])를 쓴다. `forward` 는 키를 뗀 사본과 relay 통로를 받는다.
/// - **plugin 고유 이름**(표가 모른다 → `Outside`). 호스트는 그 뜻을 모르고, 정확히 한 번은
///   target plugin 의 몫이라고 선언돼 있다(ADR-0361). 개입하지 않고 원래 요청을 그대로 넘긴다.
///
/// **plugin 고유 이름을 계약 밖으로 지키는 판정은 이름 표의 `Kept` 판정 하나뿐이다.**
/// [`run_app_layer`] 의 `Mutate` 판정은 그 이름을 거르지 않는다 — forward 는 prefix 가
/// 점유됐을 때만 일어나고, 그때 `method_meta` 는 plugin 고유 이름을 namespace fallback 으로
/// `Mutate` 로 해소한다. 그래서 `Kept` 판정을 지우면 plugin 고유 이름이 보존소에 들어가 같은
/// 키의 재시도가 재생이 된다 — ADR-0361 이 정한 "정확히 한 번은 target 의 몫" 과 반대다.
/// 그 갈래는 prefix 를 등록한 채 부르는
/// `tests::a_plugin_name_is_forwarded_every_time_with_its_request_untouched` 가 잰다.
pub(crate) fn forward_keeping_the_key(
    caller: &CallerContext,
    cmd: &IpcCommand,
    forward: impl FnOnce(&IpcCommand),
) {
    let method = tasty_ipc::alias::canonicalize(&cmd.request.method);
    let mut forward = Some(forward);
    let engaged = matches!(
        tasty_ipc::method_meta::key_contract(method),
        tasty_ipc::method_meta::KeyContract::Kept { .. }
    ) && run_app_layer(
        caller,
        cmd,
        (),
        |_| true,
        |relayed| {
            if let Some(f) = forward.take() {
                f(relayed);
            }
        },
    )
    .is_some();
    // 개입하지 않았으면(키가 없거나 계약 밖) 원래 요청을 넘긴다. 개입했으면 `forward` 는
    // relay 안에서 불렸거나(처음 보는 키), 재생·충돌·합류라 부르면 안 된다.
    if !engaged && let Some(f) = forward.take() {
        f(cmd);
    }
}

/// relay 스레드를 세우는 함수. 시험은 실패하는 것을 넣어 그 갈래를 잰다.
type Spawn = fn(Box<dyn FnOnce() + Send>) -> std::io::Result<()>;

/// 프로세스의 relay 스레드. `std::thread::spawn` 이 아닌 이유는 그것이 OS 가 스레드를 못
/// 만들 때 **패닉**하고, 부르는 자리가 GUI 이벤트 루프 · 헤드리스 펌프라는 것이다.
fn spawn_relay(work: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("ipc-idempotency-relay".into())
        .spawn(work)
        .map(|_| ())
}

fn run_app_layer_in<T>(
    store: &'static Mutex<Store>,
    spawn: Spawn,
    caller: &CallerContext,
    cmd: &IpcCommand,
    answered: T,
    handled: impl FnOnce(&T) -> bool,
    dispatch: impl FnOnce(&IpcCommand) -> T,
) -> Option<T> {
    let request = &cmd.request;
    let key = request.idempotency_key.as_deref()?;
    let method = tasty_ipc::alias::canonicalize(&request.method);
    if method_meta(method).map(|m| m.effect) != Some(MethodEffect::Mutate) {
        return None;
    }
    let now = Instant::now();
    let id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let scope = caller_scope(caller);
    let mut guard = lock(store);
    let answer = match guard.decide(now, &scope, key, method, &request.params) {
        Decision::Execute(digest) => {
            let ticket = guard.open(now, &scope, key, digest);
            drop(guard);
            let relay = Relay {
                store,
                scope,
                key: key.to_string(),
                digest,
                ticket,
                reply: cmd.response_tx.clone(),
                request_seq: cmd.request_seq(),
            };
            return Some(relay.run(spawn, request, handled, dispatch));
        }
        Decision::InFlight => {
            let waiter = Waiter {
                id,
                reply: cmd.response_tx.clone(),
            };
            match guard.join(&scope, key, waiter) {
                Ok(()) => return Some(answered),
                // `decide` 와 같은 잠금 안이라 닿지 않는다. 닿는다면 합류할 자리가 없으니
                // 결과 불명으로 답한다.
                Err(w) => still_running_error(key, &w.id),
            }
        }
        Decision::Replay(stored) => stored.replayed_for(id),
        Decision::Conflict => conflict_error(key, &id),
        Decision::Discarded => discarded_error(key, &id),
    };
    drop(guard);
    send_response(&cmd.response_tx, answer);
    Some(answered)
}

/// App 층에서 연 실행 하나 — 결말을 기다렸다가 기록하고 원래 통로로 넘긴다.
struct Relay {
    store: &'static Mutex<Store>,
    scope: String,
    key: String,
    digest: u64,
    ticket: u64,
    /// 원래 통로.
    reply: mpsc::SyncSender<JsonRpcResponse>,
    /// 원 요청의 호스트 번호. 키를 뗀 사본도 같은 요청이라 이 번호를 그대로 든다.
    request_seq: tasty_ipc::server::RequestSeq,
}

impl Relay {
    /// relay 스레드를 **먼저** 세우고 본문을 부른다.
    ///
    /// 순서가 이런 것은 스레드를 못 세운 갈래 때문이다. 본문을 부른 뒤에 세우다 실패하면
    /// 답이 이미 relay 통로로 가 있거나(동기 메서드) 나중에 그리로 온다(창 생성 · 원격
    /// attach) — 어느 쪽이든 원래 통로로 넘겨 줄 쪽이 없다. 먼저 세우면 실패했을 때 아직
    /// 아무것도 안 불렀으므로, 연 자리를 닫고 키를 뗀 요청을 **원래 통로로** 부르면 된다.
    /// 잃는 것은 이 요청의 보장뿐이고 동작은 키가 없을 때와 같다.
    ///
    /// 이 층이 이름을 안 맡으면 스레드는 본문이 relay 통로를 놓는 즉시(통로 끊김) 끝난다.
    fn run<T>(
        self,
        spawn: Spawn,
        request: &JsonRpcRequest,
        handled: impl FnOnce(&T) -> bool,
        dispatch: impl FnOnce(&IpcCommand) -> T,
    ) -> T {
        let (store, ticket, request_seq) = (self.store, self.ticket, self.request_seq);
        let (scope, key) = (self.scope.clone(), self.key.clone());
        let reply = self.reply.clone();
        let stripped = JsonRpcRequest {
            idempotency_key: None,
            ..request.clone()
        };
        // 칸이 하나인 것은 원래 통로와 같다 — 요청 하나에 답은 하나다.
        let (tx, rx) = mpsc::sync_channel::<JsonRpcResponse>(1);
        if let Err(e) = spawn(Box::new(move || self.settle_when_answered(&rx))) {
            tracing::warn!(
                method = %request.method,
                error = %e,
                "could not start the idempotency relay thread; running the request \
                 without the key guarantee"
            );
            // 본문을 부르기 **전에** 닫는다 — 그 사이에 온 재시도가 결말이 안 날 자리에
            // 합류하지 않게.
            lock(store).abandon(&scope, &key, ticket);
            let out = dispatch(&IpcCommand::continuing(stripped, reply, request_seq));
            if !handled(&out) {
                lock(store).abandon_unhandled(&scope, &key, ticket);
            }
            return out;
        }
        let relayed = IpcCommand::continuing(stripped, tx, request_seq);
        let out = dispatch(&relayed);
        drop(relayed);
        if !handled(&out) {
            lock(store).abandon_unhandled(&scope, &key, ticket);
        }
        out
    }

    fn settle_when_answered(self, rx: &mpsc::Receiver<JsonRpcResponse>) {
        let Ok(response) = rx.recv() else {
            lock(self.store).abandon(&self.scope, &self.key, self.ticket);
            return;
        };
        let handoff = lock(self.store).settle(
            Instant::now(),
            &self.scope,
            &self.key,
            self.digest,
            Some(self.ticket),
            &response,
        );
        for w in handoff.waiters {
            let answer = if handoff.kept {
                response.replayed_for(w.id)
            } else {
                discarded_error(&self.key, &w.id)
            };
            send_response(&w.reply, answer);
        }
        send_response(&self.reply, response);
    }
}

/// 프로세스 보존소의 판정 누계 — 값을 읽는 자리. 요청 압력 응답(`system.pressure`)의
/// `keyed_requests` 덩어리가 이것을 싣는다.
pub(crate) fn retry_counts() -> RetryCounts {
    store().counts()
}

/// `system.info` 가 싣는 보장 범위. 상한을 **리터럴로 다시 적지 않는다** — 위 상수를
/// 그대로 내보내야 선언과 동작이 갈리지 않는다.
pub(crate) fn declaration() -> serde_json::Value {
    serde_json::json!({
        "retention_ms": RETENTION.as_millis() as u64,
        "capacity": CAPACITY,
        "max_response_bytes": MAX_STORED_RESPONSE_BYTES,
        "max_key_bytes": MAX_KEY_BYTES,
        // 보존소가 프로세스 메모리에 있다는 사실. 호출자가 crash 뒤의 보장을
        // 추정하지 않게 값으로 말한다.
        "survives_restart": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resp(id: i32, body: &str) -> JsonRpcResponse {
        JsonRpcResponse::success(json!(id), json!({ "body": body }))
    }

    /// 같은 키·같은 요청은 **한 번만 실행**되고 둘째는 보관된 답을 받는다.
    #[test]
    fn the_same_key_and_request_is_executed_once_and_then_replayed() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"}))
        else {
            panic!("처음 보는 키는 실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));

        match s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"})) {
            Decision::Replay(r) => assert_eq!(r.result.as_ref().unwrap()["body"], "first"),
            other => panic!("재조회여야 한다: {other:?}"),
        }
    }

    /// 재조회로 나가는 답은 **이번 요청의 `id`** 를 달고, 재조회임을 표지로 말한다.
    #[test]
    fn a_replay_carries_the_new_id_and_says_it_is_a_replay() {
        let stored = resp(1, "first");
        let again = stored.replayed_for(json!(99));
        assert_eq!(
            again.id,
            json!(99),
            "앞선 요청의 id 로 답하면 대응이 깨진다"
        );
        assert!(again.idempotent_replay);
        assert!(
            !stored.idempotent_replay,
            "처음 실행된 답에 재조회 표지가 붙으면 두 사건이 구별되지 않는다"
        );
    }

    /// 같은 키에 다른 요청이 붙으면 **아무것도 실행하지 않는다.**
    #[test]
    fn the_same_key_with_a_different_request_is_a_conflict() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "a"}))
        else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));

        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "b"})),
            Decision::Conflict
        ));
        assert!(
            matches!(
                s.decide(t0, "local", "k", "tab.create", &json!({"name": "a"})),
                Decision::Conflict
            ),
            "메서드가 다른 것도 다른 요청이다"
        );
    }

    /// 키는 **주체마다 따로 산다.** 서로 모르는 두 호출자가 같은 문자열을 골랐을 때
    /// 한쪽이 다른 쪽의 답을 받으면, 그 자리는 성공 응답이라 아무 신호도 안 난다.
    #[test]
    fn two_callers_that_picked_the_same_string_do_not_share_an_answer() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let params = json!({ "name": "same" });
        let Decision::Execute(d) = s.decide(t0, "agent:a", "k", "workspace.create", &params) else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "agent:a", "k", d, &resp(1, "answer-for-a"));

        assert!(
            matches!(
                s.decide(t0, "agent:b", "k", "workspace.create", &params),
                Decision::Execute(_)
            ),
            "다른 주체가 같은 키로 남의 답을 받았다"
        );
        // 같은 주체는 그대로 재조회다 — 위 단정이 scope 를 통째로 무력화한 것이 아니다.
        assert!(matches!(
            s.decide(t0, "agent:a", "k", "workspace.create", &params),
            Decision::Replay(_)
        ));
    }

    /// 주체 문자열이 세 변종을 **갈라** 낸다. 하나로 접히면 위 시험이 통과하면서도
    /// plugin 과 agent 가 한 칸을 나눠 쓰게 된다.
    #[test]
    fn the_three_caller_kinds_get_three_scopes() {
        use std::collections::HashSet;
        use std::sync::Arc;
        let scopes: HashSet<String> = [
            caller_scope(&CallerContext::Local),
            caller_scope(&CallerContext::Plugin {
                plugin_id: "p".into(),
                permissions: Arc::new(Default::default()),
            }),
            caller_scope(&CallerContext::Agent {
                agent_id: "p".into(),
                permissions: Arc::new(Default::default()),
            }),
        ]
        .into_iter()
        .collect();
        assert_eq!(scopes.len(), 3, "주체가 접혔다: {scopes:?}");
    }

    /// 다이제스트는 **JSON 오브젝트의 키 순서에 안 흔들린다.** 흔들리면 같은 요청의
    /// 재시도가 충돌로 거절된다 — 이 축의 실패는 조용하지 않고 호출자를 막는다.
    #[test]
    fn the_digest_does_not_move_with_object_key_order() {
        let a: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
        assert_eq!(digest("m", &a), digest("m", &b));
        assert_ne!(digest("m", &a), digest("m", &json!({"a": 1, "b": 3})));
    }

    /// 보존 시간이 지나면 항목이 걷히고, 그 키는 **처음 보는 키와 같아진다.**
    #[test]
    fn an_expired_key_is_indistinguishable_from_a_fresh_one() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) = s.decide(t0, "local", "k", "workspace.create", &json!({}))
        else {
            panic!("실행이어야 한다");
        };
        s.record(t0, "local", "k", d, &resp(1, "first"));
        assert!(matches!(
            s.decide(
                t0 + RETENTION - Duration::from_millis(1),
                "local",
                "k",
                "workspace.create",
                &json!({})
            ),
            Decision::Replay(_)
        ));
        assert!(matches!(
            s.decide(t0 + RETENTION, "local", "k", "workspace.create", &json!({})),
            Decision::Execute(_)
        ));
    }

    /// 항목 수 상한을 넘기면 가장 오래된 것부터 밀려난다.
    #[test]
    fn the_oldest_key_is_pushed_out_past_the_capacity() {
        let mut s = Store::new();
        let t0 = Instant::now();
        for i in 0..=CAPACITY {
            let key = format!("k{i}");
            let Decision::Execute(d) =
                s.decide(t0, "local", &key, "workspace.create", &json!({ "i": i }))
            else {
                panic!("실행이어야 한다");
            };
            s.record(t0, "local", &key, d, &resp(1, "x"));
        }
        assert_eq!(s.len(), CAPACITY);
        assert!(
            matches!(
                s.decide(t0, "local", "k0", "workspace.create", &json!({"i": 0})),
                Decision::Execute(_)
            ),
            "밀려난 키는 처음 보는 키와 구별되지 않는다"
        );
        assert!(matches!(
            s.decide(
                t0,
                "local",
                &format!("k{CAPACITY}"),
                "workspace.create",
                &json!({"i": CAPACITY})
            ),
            Decision::Replay(_)
        ));
    }

    /// 상한을 넘는 답은 **버리되 키는 남긴다** — 그래야 재전송이 두 번째 효과를
    /// 남기지 않고, 대신 "실행은 됐다" 를 말할 수 있다.
    #[test]
    fn an_oversized_response_keeps_the_key_and_drops_the_answer() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let Decision::Execute(d) =
            s.decide(t0, "local", "k", "surface.read_since_scan_mark", &json!({}))
        else {
            panic!("실행이어야 한다");
        };
        let big = JsonRpcResponse::success(json!(1), json!("x".repeat(MAX_STORED_RESPONSE_BYTES)));
        s.record(t0, "local", "k", d, &big);
        assert!(matches!(
            s.decide(t0, "local", "k", "surface.read_since_scan_mark", &json!({})),
            Decision::Discarded
        ));
    }

    /// 총 바이트 상한이 항목 수 상한과 **독립으로** 건다.
    #[test]
    fn the_byte_budget_evicts_before_the_capacity_does() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let payload = "y".repeat(MAX_STORED_RESPONSE_BYTES - 128);
        let n = TOTAL_BUDGET_BYTES / MAX_STORED_RESPONSE_BYTES + 2;
        assert!(
            n < CAPACITY,
            "이 시험은 항목 수 상한 아래에서 돌아야 뜻이 있다"
        );
        for i in 0..n {
            let key = format!("k{i}");
            let Decision::Execute(d) =
                s.decide(t0, "local", &key, "workspace.create", &json!({ "i": i }))
            else {
                panic!("실행이어야 한다");
            };
            s.record(
                t0,
                "local",
                &key,
                d,
                &JsonRpcResponse::success(json!(1), json!(payload)),
            );
        }
        assert!(s.len() < n, "바이트 예산이 아무것도 안 밀어냈다");
        assert!(s.total_bytes <= TOTAL_BUDGET_BYTES);
    }

    /// 키가 없으면 이 모듈은 아무 일도 안 한다.
    #[test]
    fn a_request_without_a_key_is_untouched() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "workspace.create".into(),
            params: json!({}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: None,
        };
        assert!(
            begin(Instant::now(), &CallerContext::Local, &req, &json!(1))
                .unwrap()
                .is_none()
        );
    }

    /// 읽기·멱등 메서드는 키를 실어도 보존소에 안 들어간다 — 들어가면 조회가 낡은
    /// 답을 받는다.
    #[test]
    fn a_key_on_a_non_mutating_method_does_nothing() {
        for method in ["workspace.list", "workspace.update"] {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: method.into(),
                params: json!({}),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: Some("shared-key".into()),
            };
            assert!(
                begin(Instant::now(), &CallerContext::Local, &req, &json!(1))
                    .unwrap()
                    .is_none(),
                "{method} 가 보존소에 들어갔다"
            );
        }
    }

    /// 빈 키와 너무 긴 키는 **실행 전에** 거절된다. 경계 안의 두 끝은 통과한다 — 그래야
    /// 거절이 "길이" 때문인지 "키가 있어서" 인지 갈린다.
    #[test]
    fn a_key_outside_the_length_bound_is_rejected_before_anything_runs() {
        fn keyed(key: String) -> JsonRpcRequest {
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "workspace.create".into(),
                params: json!({}),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: Some(key),
            }
        }
        for key in ["".to_string(), "k".repeat(MAX_KEY_BYTES + 1)] {
            let err =
                check_envelope(&keyed(key), &json!(1)).expect_err("길이 밖 키는 거절이어야 한다");
            assert_eq!(err.error.expect("에러").code, -32602);
        }
        for key in ["k".to_string(), "k".repeat(MAX_KEY_BYTES)] {
            assert!(check_envelope(&keyed(key), &json!(1)).is_ok());
        }
    }

    /// 봉투 검사는 **목적지와 무관하다** — engine 라우터에 안 닿는 App 층 메서드와
    /// plugin namespace 이름도 같은 판정을 받는다. 진입 게이트([`super::super::check_request`])
    /// 가 그 자리이므로 거기서 잰다.
    ///
    /// 통제군이 같은 시험 안에 있다: 같은 메서드에 경계 안의 키는 게이트를 통과한다.
    ///
    /// 게이트는 둘이다. 창도 parked engine 도 없는 GUI 구간의 Local 호출은
    /// `check_without_engine` 을 지나고, 그 구간에서 부를 수 있는 대표 메서드가 App 층의
    /// `window.create` 다 — 그 갈래도 같은 판정을 받아야 한다(gui 조합에만 있다).
    #[test]
    fn the_envelope_is_judged_at_the_gate_whatever_the_destination() {
        use crate::ipc::handler::check_request;
        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = crate::ipc::handler::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        // App 층(창 생성) · namespace forward 모양의 이름 · engine 라우터.
        for method in ["window.create", "someplugin.do_thing", "workspace.create"] {
            for (key, ok) in [
                (String::new(), false),
                ("k".repeat(MAX_KEY_BYTES + 1), false),
                ("fine".to_string(), true),
            ] {
                let req = JsonRpcRequest {
                    jsonrpc: "2.0".into(),
                    method: method.into(),
                    params: json!({}),
                    id: Some(json!(7)),
                    session_token: None,
                    response_timeout_ms: None,
                    idempotency_key: Some(key.clone()),
                };
                let r = check_request(
                    &mut core,
                    &mut state,
                    &mut engine,
                    &req,
                    &CallerContext::Local,
                );
                #[cfg(feature = "gui")]
                let without_engine = Some((
                    "check_without_engine",
                    crate::ipc::handler::check_without_engine(&req, &CallerContext::Local)
                        .map(|_| ()),
                ));
                #[cfg(not(feature = "gui"))]
                let without_engine = None;
                let gates = std::iter::once(("check_request", r.map(|_| ()))).chain(without_engine);
                for (gate, r) in gates {
                    match (r, ok) {
                        (Ok(()), true) => {}
                        (Err(e), false) => {
                            assert_eq!(e.error.expect("에러").code, -32602, "{gate} {method}")
                        }
                        (Ok(()), false) => {
                            panic!("{gate} {method}: 길이 {} 키가 게이트를 지났다", key.len())
                        }
                        (Err(e), true) => panic!("{gate} {method}: 경계 안 키가 거절됐다: {e:?}"),
                    }
                }
            }
        }
    }

    /// 표지의 **부재**는 "계약 밖" 을 뜻하지 않는다 — 계약이 걸려서 **실행을 막은** 답
    /// (`-32063`)도 에러로 만들어져 표지가 `false` 다. [`JsonRpcRequest::idempotency_key`]
    /// 의 서술이 그 구별에 기대므로 값으로 고정한다.
    ///
    /// 통제군이 같은 시험 안에 있다: 같은 키·같은 요청의 재생은 표지가 `true` 다. 그래야
    /// `false` 가 "계약이 안 걸렸다" 인지 "이 갈래의 성질" 인지 갈린다.
    #[test]
    fn a_blocked_retry_carries_no_replay_marker_though_the_contract_did_engage() {
        fn req(name: &str) -> JsonRpcRequest {
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "workspace.create".into(),
                params: json!({ "name": name }),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                // 보존소는 프로세스 전역이라 다른 시험과 안 겹치는 키를 쓴다.
                idempotency_key: Some("a-blocked-retry-probe".into()),
            }
        }
        let now = Instant::now();
        let first = req("probe-a");
        let pending =
            begin(now, &CallerContext::Local, &first, &json!(1)).expect("처음 보는 키는 실행이다");
        finish(now, pending, &resp(1, "ok"));

        let replay =
            begin(now, &CallerContext::Local, &first, &json!(2)).expect_err("재생은 답을 낸다");
        assert!(replay.idempotent_replay, "재생에는 표지가 붙어야 한다");

        let conflict = begin(now, &CallerContext::Local, &req("probe-b"), &json!(3))
            .expect_err("같은 키·다른 요청은 거절이다");
        assert_eq!(
            conflict.error.as_ref().expect("에러").code,
            ERR_IDEMPOTENCY_KEY_CONFLICT
        );
        assert!(
            !conflict.idempotent_replay,
            "이 시험이 고정하는 것은 이 부재다 — 여기에 표지가 붙으면 문서의 단서가 불필요해진다"
        );
    }

    /// 라우터가 실제로 보존소를 **지나는가.** 위 시험들은 보존소만 재고, 그 자리가
    /// 배선돼 있다는 것은 재지 않는다 — 진짜 handler 를 두 번 불러 **부수효과의 수**로
    /// 잰다.
    ///
    /// 통제군이 같은 시험 안에 있다: 키 없는 두 번은 워크스페이스를 둘 만든다. 그래야
    /// "하나만 생겼다" 가 보존소 덕인지 handler 가 원래 한 번만 만드는 것인지 갈린다.
    #[test]
    fn the_router_consults_the_store_before_the_handler_runs() {
        use crate::ipc::caller::CallerContext;
        use crate::ipc::handler::handle_with_caller;

        fn create(name: &str, key: Option<&str>) -> JsonRpcRequest {
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                method: "workspace.create".into(),
                params: json!({ "name": name }),
                id: Some(json!(1)),
                session_token: None,
                response_timeout_ms: None,
                idempotency_key: key.map(str::to_string),
            }
        }

        let _home = crate::test_support::TastyHomeGuard::new();
        let mut core = crate::ipc::handler::cli_entry_tests::test_core();
        let (mut state, mut engine) = crate::state::tests::test_state();
        let base = engine.workspaces.len();

        // 통제군 — 키가 없으면 두 번 불린 만큼 두 개가 생긴다.
        for _ in 0..2 {
            let r = handle_with_caller(
                &mut core,
                &mut state,
                &mut engine,
                &create("unkeyed", None),
                &CallerContext::Local,
            );
            assert!(r.error.is_none(), "준비: {:?}", r.error);
        }
        assert_eq!(
            engine.workspaces.len(),
            base + 2,
            "통제군이 성립하지 않는다"
        );

        // 실험군 — 같은 키로 두 번.
        let key = "wiring-probe-a";
        let first = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &create("keyed", Some(key)),
            &CallerContext::Local,
        );
        assert!(first.error.is_none(), "{:?}", first.error);
        assert!(!first.idempotent_replay);
        let after_first = engine.workspaces.len();

        let second = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &create("keyed", Some(key)),
            &CallerContext::Local,
        );
        assert_eq!(
            engine.workspaces.len(),
            after_first,
            "같은 키의 재시도가 두 번째 워크스페이스를 만들었다"
        );
        assert!(second.idempotent_replay, "재조회 표지가 없다");
        assert_eq!(second.result, first.result, "재조회가 다른 답을 냈다");
        assert_eq!(second.id, json!(1));

        // 같은 키에 다른 입력 — 아무것도 안 만들고 충돌로 답한다.
        let conflict = handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &create("something-else", Some(key)),
            &CallerContext::Local,
        );
        assert_eq!(
            conflict.error.expect("충돌이어야 한다").code,
            ERR_IDEMPOTENCY_KEY_CONFLICT
        );
        assert_eq!(
            engine.workspaces.len(),
            after_first,
            "충돌인데 워크스페이스가 생겼다"
        );
    }

    // ── App 층(`run_app_layer`) ──
    //
    // 보존소는 프로세스 전역이라 시험마다 다른 키를 쓴다. relay 는 스레드에서 결말을
    // 기록하므로 답은 원래 통로에서 기한을 두고 받는다.

    const WAIT: Duration = Duration::from_secs(5);

    /// `Mutate` 인 App 층 이름 하나(창 생성). params 로 요청을 가른다.
    fn app_cmd(key: &str, id: i64, name: &str) -> (IpcCommand, mpsc::Receiver<JsonRpcResponse>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "window.create".into(),
            params: json!({ "name": name }),
            id: Some(json!(id)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some(key.into()),
        };
        (IpcCommand::new(req, tx), rx)
    }

    /// 동기로 답하는 App 층 메서드 흉내 — 부른 횟수를 세고 그 자리에서 답한다.
    fn answer_now(runs: &std::cell::Cell<u32>) -> impl FnOnce(&IpcCommand) -> bool + '_ {
        move |c: &IpcCommand| {
            runs.set(runs.get() + 1);
            assert!(
                c.request.idempotency_key.is_none(),
                "dispatch 는 키를 뗀 사본을 받아야 한다 — 안 떼면 자기에게 합류한다"
            );
            send_response(
                &c.response_tx,
                JsonRpcResponse::success(c.request.id.clone().unwrap(), json!({"run": runs.get()})),
            );
            true
        }
    }

    /// App 층 메서드도 같은 키의 재시도를 **한 번만 실행**하고 재생으로 답한다.
    ///
    /// 통제군이 같은 시험 안에 있다: 키 없이 두 번 부르면 보존소가 개입하지 않고(`None`)
    /// 호출자의 원래 본문이 두 번 돈다.
    #[test]
    fn an_app_layer_call_runs_once_per_key_and_the_retry_is_a_replay() {
        let runs = std::cell::Cell::new(0);
        let (first, rx1) = app_cmd("app-once-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::Local,
            &first,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(out, Some(true));
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        assert!(!r1.idempotent_replay);

        let (retry, rx2) = app_cmd("app-once-probe", 2, "a");
        let out = run_app_layer(
            &CallerContext::Local,
            &retry,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(
            out,
            Some(false),
            "재생은 dispatch 없이 `answered` 를 돌려준다"
        );
        let r2 = rx2.recv_timeout(WAIT).expect("재생 답");
        assert_eq!(runs.get(), 1, "같은 키의 재시도가 두 번째 실행을 냈다");
        assert!(r2.idempotent_replay, "재생 표지가 없다");
        assert_eq!(r2.id, json!(2), "재생은 이번 요청의 id 로 답한다");
        assert_eq!(r2.result, r1.result);

        // 통제군 — 키가 없으면 개입하지 않는다.
        let (tx, _rx) = mpsc::sync_channel(1);
        let mut req = first.request.clone();
        req.idempotency_key = None;
        let unkeyed = IpcCommand::new(req, tx);
        assert!(
            run_app_layer(
                &CallerContext::Local,
                &unkeyed,
                false,
                |h| *h,
                answer_now(&runs)
            )
            .is_none()
        );
    }

    /// 키를 뗀 사본은 원 요청의 호스트 번호를 그대로 든다 — 새 번호를 받으면 요청 하나가
    /// 번호 둘로 갈려, 그 안에서 plugin 으로 넘긴 대기를 원 요청으로 되짚을 수 없다.
    #[test]
    fn the_stripped_copy_keeps_the_request_seq_of_the_original() {
        let seen = std::cell::Cell::new(None);
        let (first, _rx) = app_cmd("app-seq-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::Local,
            &first,
            false,
            |h| *h,
            |c: &IpcCommand| {
                seen.set(Some(c.request_seq()));
                send_response(
                    &c.response_tx,
                    JsonRpcResponse::success(json!(1), json!({})),
                );
                true
            },
        );
        assert_eq!(out, Some(true));
        assert_eq!(seen.get(), Some(first.request_seq()));

        // relay 스레드를 못 세운 갈래도 같은 사본을 원래 통로로 실행한다 — 번호도 같아야 한다.
        let (second, _rx) = app_cmd("app-seq-probe-no-relay", 1, "a");
        let out = run_app_layer_in(
            isolated_store(),
            no_threads,
            &CallerContext::Local,
            &second,
            false,
            |h| *h,
            |c: &IpcCommand| {
                seen.set(Some(c.request_seq()));
                true
            },
        );
        assert_eq!(out, Some(true));
        assert_eq!(seen.get(), Some(second.request_seq()));
    }

    /// 답이 **나중에** 오는 메서드(창 생성)에서 첫 실행이 끝나기 전에 같은 키가 오면
    /// 두 번째 실행 없이 합류하고, 첫 결말을 재생으로 받는다. 이것이 동시 동일 키의
    /// 수렴이다 — engine 라우터에서는 실행이 직렬이라 닿지 않던 상태다.
    #[test]
    fn a_retry_that_arrives_while_the_first_is_running_joins_it() {
        let runs = std::cell::Cell::new(0);
        let parked: std::cell::RefCell<Option<IpcCommand>> = std::cell::RefCell::new(None);
        let (first, rx1) = app_cmd("app-join-probe", 1, "a");
        let out = run_app_layer(
            &CallerContext::Local,
            &first,
            false,
            |h| *h,
            |c| {
                runs.set(runs.get() + 1);
                // 완료 채널처럼 통로를 들고 나간다 — 답은 나중에.
                *parked.borrow_mut() =
                    Some(IpcCommand::new(c.request.clone(), c.response_tx.clone()));
                true
            },
        );
        assert_eq!(out, Some(true));

        let (retry, rx2) = app_cmd("app-join-probe", 2, "a");
        let out = run_app_layer(
            &CallerContext::Local,
            &retry,
            false,
            |h| *h,
            |_| panic!("진행 중인 키가 두 번째 실행을 냈다"),
        );
        assert_eq!(out, Some(false));
        assert!(
            rx2.try_recv().is_err(),
            "합류자는 첫 실행이 끝나기 전에 답을 받으면 안 된다"
        );

        // 진행 중인데 요청이 다르면 충돌이다 — 합류가 아니다.
        let (other, rx3) = app_cmd("app-join-probe", 3, "b");
        run_app_layer(
            &CallerContext::Local,
            &other,
            false,
            |h| *h,
            |_| -> bool { panic!("충돌인데 실행됐다") },
        );
        assert_eq!(
            rx3.recv_timeout(WAIT)
                .expect("충돌 답")
                .error
                .expect("에러")
                .code,
            ERR_IDEMPOTENCY_KEY_CONFLICT
        );

        let c = parked.borrow_mut().take().expect("통로를 들고 있어야 한다");
        send_response(
            &c.response_tx,
            JsonRpcResponse::success(json!(1), json!({"window_id": 7})),
        );
        let r1 = rx1.recv_timeout(WAIT).expect("첫 요청의 답");
        let r2 = rx2.recv_timeout(WAIT).expect("합류자의 답");
        assert_eq!(runs.get(), 1);
        assert!(!r1.idempotent_replay);
        assert!(r2.idempotent_replay, "합류자의 답은 재생이다");
        assert_eq!(r2.id, json!(2));
        assert_eq!(r2.result, r1.result);
    }

    /// 이 층이 그 이름을 **안 맡으면** 연 자리를 닫는다 — 다음 층(engine 라우터)이 같은
    /// 키를 처음 보는 키로 판정해야 한다. 닫지 않으면 라우터가 진행 중을 보고 결과
    /// 불명으로 답한다.
    #[test]
    fn a_layer_that_does_not_handle_the_name_leaves_no_trace() {
        let (cmd, _rx) = app_cmd("app-unhandled-probe", 1, "a");
        let out = run_app_layer(&CallerContext::Local, &cmd, false, |h| *h, |_| false);
        assert_eq!(out, Some(false));
        let pending = begin(
            Instant::now(),
            &CallerContext::Local,
            &cmd.request,
            &json!(1),
        )
        .expect("다음 층은 처음 보는 키로 실행해야 한다");
        assert!(pending.is_some());
    }

    /// 답을 안 보내고 통로를 버린 실행은 기록을 안 남긴다 — 다음 재시도는 실행된다. 합류자는
    /// 첫 요청과 같은 결말(통로 끊김)을 받는다.
    #[test]
    fn a_run_that_drops_its_reply_is_forgotten_with_its_joiners() {
        let parked: std::cell::RefCell<Option<IpcCommand>> = std::cell::RefCell::new(None);
        let (first, rx1) = app_cmd("app-dropped-probe", 1, "a");
        run_app_layer(
            &CallerContext::Local,
            &first,
            false,
            |h| *h,
            |c| {
                *parked.borrow_mut() =
                    Some(IpcCommand::new(c.request.clone(), c.response_tx.clone()));
                true
            },
        );
        let (retry, rx2) = app_cmd("app-dropped-probe", 2, "a");
        run_app_layer(
            &CallerContext::Local,
            &retry,
            false,
            |h| *h,
            |_| -> bool { panic!("합류해야 한다") },
        );
        // 원래 통로의 송신 쪽은 명령 자신도 들고 있다 — 서버에서는 처리가 끝나면 버려진다.
        drop((first, retry));
        drop(parked.borrow_mut().take());
        assert!(matches!(
            rx1.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(matches!(
            rx2.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));

        let runs = std::cell::Cell::new(0);
        let (again, rx3) = app_cmd("app-dropped-probe", 3, "a");
        run_app_layer(
            &CallerContext::Local,
            &again,
            false,
            |h| *h,
            answer_now(&runs),
        );
        assert_eq!(runs.get(), 1, "결말이 없던 키는 다시 실행돼야 한다");
        assert!(!rx3.recv_timeout(WAIT).expect("답").idempotent_replay);
    }

    /// 진행 중인 실행이 상한에 밀려나 같은 키를 다른 실행이 쥐었으면, 늦게 온 첫 결말은
    /// **남의 자리**를 덮지 않는다.
    #[test]
    fn a_late_outcome_does_not_overwrite_a_newer_run_of_the_same_key() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let old = s.open(t0, "local", "k", 1);
        s.abandon("local", "k", old); // 밀려남을 흉내 낸다
        let newer = s.open(t0, "local", "k", 1);
        let h = s.settle(t0, "local", "k", 1, Some(old), &resp(1, "late"));
        assert!(h.waiters.is_empty());
        assert!(
            matches!(
                s.entries.front().map(|e| &e.outcome),
                Some(Stored::InFlight { ticket, .. }) if *ticket == newer
            ),
            "늦은 결말이 새 실행의 자리를 덮었다"
        );
        let h = s.settle(t0, "local", "k", 1, Some(newer), &resp(2, "mine"));
        assert!(h.kept);
    }

    /// 판정 갈래마다 칸 하나가 한 번씩 오른다 — 다섯 갈래를 한 번씩 밟고 누계를 본다.
    #[test]
    fn every_decision_is_counted_once_in_its_own_slot() {
        let mut s = Store::new();
        let t0 = Instant::now();
        let p = json!({"name": "a"});
        let Decision::Execute(d) = s.decide(t0, "local", "k", "workspace.create", &p) else {
            panic!("처음 보는 키");
        };
        s.record(t0, "local", "k", d, &resp(1, "ok"));
        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &p),
            Decision::Replay(_)
        ));
        assert!(matches!(
            s.decide(t0, "local", "k", "workspace.create", &json!({"name": "b"})),
            Decision::Conflict
        ));
        let Decision::Execute(big) = s.decide(t0, "local", "big", "workspace.create", &p) else {
            panic!("처음 보는 키");
        };
        let huge =
            JsonRpcResponse::success(json!(1), json!("x".repeat(MAX_STORED_RESPONSE_BYTES + 1)));
        s.record(t0, "local", "big", big, &huge);
        assert!(matches!(
            s.decide(t0, "local", "big", "workspace.create", &p),
            Decision::Discarded
        ));
        let Decision::Execute(run) = s.decide(t0, "local", "run", "window.create", &p) else {
            panic!("처음 보는 키");
        };
        s.open(t0, "local", "run", run);
        assert!(matches!(
            s.decide(t0, "local", "run", "window.create", &p),
            Decision::InFlight
        ));

        assert_eq!(
            s.counts(),
            RetryCounts {
                executed: 3,
                replayed: 1,
                conflicted: 1,
                discarded: 1,
                in_flight: 1,
            }
        );
    }

    /// 프로세스 보존소의 누계가 **실제 판정 자리**에서 오르는가 — engine 라우터의 입구를
    /// 지나 재생을 한 번 만들고 읽는다. 보존소는 전역이고 시험이 병렬로 돌아 다른 시험도
    /// 같은 칸을 올리므로, 정확한 값이 아니라 **적어도 이만큼 올랐다** 를 본다.
    #[test]
    fn the_process_counts_move_where_the_router_decides() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "workspace.create".into(),
            params: json!({"name": "count-probe"}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some("retry-count-probe".into()),
        };
        let before = retry_counts();
        let now = Instant::now();
        let pending = begin(now, &CallerContext::Local, &req, &json!(1)).expect("처음 보는 키");
        finish(now, pending, &resp(1, "ok"));
        begin(now, &CallerContext::Local, &req, &json!(2)).expect_err("재생");
        let after = retry_counts();
        assert!(after.executed > before.executed, "{before:?} → {after:?}");
        assert!(after.replayed > before.replayed, "{before:?} → {after:?}");
    }

    /// 다른 시험과 안 섞이는 보존소 — 누계를 정확한 값으로 잰다.
    fn isolated_store() -> &'static Mutex<Store> {
        Box::leak(Box::new(Mutex::new(Store::new())))
    }

    fn keyed(method: &str, key: &str) -> (IpcCommand, mpsc::Receiver<JsonRpcResponse>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params: json!({}),
            id: Some(json!(1)),
            session_token: None,
            response_timeout_ms: None,
            idempotency_key: Some(key.into()),
        };
        (IpcCommand::new(req, tx), rx)
    }

    /// App 층을 지나 engine 라우터에서 실행되는 요청은 `executed` 에 **정확히 한 번** 든다.
    /// 실제 흐름과 같은 순서로 두 층을 지난다 — App 층이 이름을 안 맡고(NotHandled),
    /// engine 라우터가 `begin` → `finish` 로 실행한다.
    ///
    /// 통제군이 같은 시험 안에 있다: 같은 키의 재시도는 `replayed` 로 들고 `executed` 는
    /// 그대로다 — 그래야 1 이 "한 번 셌다" 이지 "아무 갈래나 올랐다" 가 아니다. 재시도는
    /// engine 라우터까지 안 간다: engine 이 기록한 답을 App 층이 먼저 보고 재생한다(두 층의
    /// 다이제스트가 같다).
    #[test]
    fn a_request_that_crosses_both_layers_is_executed_once() {
        let store = isolated_store();
        let (cmd, rx) = keyed("workspace.create", "cross-layer");
        let out = run_app_layer_in(
            store,
            spawn_relay,
            &CallerContext::Local,
            &cmd,
            true,
            |h| *h,
            |_| false,
        );
        assert_eq!(out, Some(false), "App 층은 그 이름을 안 맡는다");
        let pending = begin_in(
            store,
            Instant::now(),
            &CallerContext::Local,
            &cmd.request,
            &json!(1),
        )
        .expect("처음 보는 키");
        assert!(pending.is_some());
        finish_in(store, Instant::now(), pending, &resp(1, "ok"));
        assert_eq!(
            lock(store).counts(),
            RetryCounts {
                executed: 1,
                ..RetryCounts::default()
            }
        );
        let out = run_app_layer_in(
            store,
            spawn_relay,
            &CallerContext::Local,
            &cmd,
            true,
            |h| *h,
            |_| panic!("재생 갈래는 본문을 안 부른다"),
        );
        assert_eq!(out, Some(true), "App 층이 재생으로 답한다");
        assert!(rx.try_recv().expect("재생 답").idempotent_replay);
        let after_retry = lock(store).counts();
        assert_eq!(after_retry.executed, 1, "{after_retry:?}");
        assert_eq!(after_retry.replayed, 1, "{after_retry:?}");
    }

    /// App 층이 안 맡은 호출은 거기서 `executed` 에 안 든다 — 다음 경로가 다시 판정한다.
    /// GUI debug step 의 `Mutate`(그 step 이 다시 [`run_app_layer`] 로 판정한다)와 plugin
    /// namespace 고유 이름(표가 모른다 — 아무 데서도 안 센다).
    #[test]
    fn a_call_outside_the_contract_is_not_counted_as_executed() {
        let store = isolated_store();
        let mut names = vec!["someplugin.do_thing"];
        if cfg!(debug_assertions) {
            names.push("debug.lua.eval");
        }
        for name in names {
            let (cmd, _rx) = keyed(name, "outside");
            run_app_layer_in(
                store,
                spawn_relay,
                &CallerContext::Local,
                &cmd,
                true,
                |h| *h,
                |_| false,
            );
            assert_eq!(lock(store).counts(), RetryCounts::default(), "{name}");
        }
    }

    // ── plugin namespace forward(`forward_keeping_the_key`) ──

    /// forward 흉내 — 넘겨받은 명령을 붙들어 둔다. plugin 은 답을 **나중에** 보내므로 여기서
    /// 답하지 않는다.
    fn hold_forward(held: &std::cell::RefCell<Vec<IpcCommand>>) -> impl FnOnce(&IpcCommand) + '_ {
        move |c: &IpcCommand| {
            held.borrow_mut().push(IpcCommand::continuing(
                c.request.clone(),
                c.response_tx.clone(),
                c.request_seq(),
            ));
        }
    }

    /// prefix 를 plugin 이 점유한 **표의** `Mutate`(`image.open`)는 forward 로 나가도 같은 키의
    /// 재시도가 두 번째 forward 를 안 낸다 — 첫 forward 의 답(plugin 이 나중에 보낸 것)을 재생으로
    /// 받는다. 표가 그 이름을 `Kept` 로 선언하므로 호출자가 믿는 계약이다(ADR-0566).
    #[test]
    fn a_forwarded_host_method_runs_once_per_key_and_the_retry_is_a_replay() {
        let held = std::cell::RefCell::new(Vec::new());
        let (first, rx1) = keyed("image.open", "forward-once-probe");
        forward_keeping_the_key(&CallerContext::Local, &first, hold_forward(&held));
        assert_eq!(held.borrow().len(), 1, "첫 요청이 forward 되지 않았다");
        assert!(
            held.borrow()[0].request.idempotency_key.is_none(),
            "forward 는 키를 뗀 사본을 받아야 한다"
        );
        // plugin 이 나중에 답한다 — relay 통로로.
        let answer = JsonRpcResponse::success(json!(1), json!({"surface_id": 7}));
        send_response(&held.borrow()[0].response_tx, answer);
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        assert!(!r1.idempotent_replay);

        let (retry, rx2) = keyed("image.open", "forward-once-probe");
        forward_keeping_the_key(&CallerContext::Local, &retry, hold_forward(&held));
        assert_eq!(
            held.borrow().len(),
            1,
            "같은 키의 재시도가 plugin 으로 두 번째 forward 됐다"
        );
        let r2 = rx2.recv_timeout(WAIT).expect("재생 답");
        assert!(r2.idempotent_replay, "재생 표지가 없다");
        assert_eq!(r2.result, r1.result);
    }

    /// 위 시험의 짝 — 재시도가 plugin 의 답 **전에** 오면 진행 중인 첫 forward 에 합류하고, 두
    /// 번째 forward 를 안 낸다. forward 자리가 `run_app_layer` 에 넘기는 `handled` 판정이 자리를
    /// 열어 둔 채 두는지를 잰다 — 그 판정이 `false` 면 forward 직후 자리가 닫혀 재시도가 plugin
    /// 으로 한 번 더 나가고, 답을 늦게 받는 위 시험은 그 갈래를 못 본다(ADR-0566).
    #[test]
    fn a_retry_before_the_plugin_answers_joins_the_running_forward() {
        let held = std::cell::RefCell::new(Vec::new());
        let (first, rx1) = keyed("image.open", "forward-join-probe");
        forward_keeping_the_key(&CallerContext::Local, &first, hold_forward(&held));
        let (retry, rx2) = keyed("image.open", "forward-join-probe");
        forward_keeping_the_key(&CallerContext::Local, &retry, hold_forward(&held));
        assert_eq!(
            held.borrow().len(),
            1,
            "답 전에 온 같은 키의 재시도가 plugin 으로 두 번째 forward 됐다"
        );
        let answer = JsonRpcResponse::success(json!(1), json!({"surface_id": 9}));
        send_response(&held.borrow()[0].response_tx, answer);
        let r1 = rx1.recv_timeout(WAIT).expect("첫 답");
        let r2 = rx2.recv_timeout(WAIT).expect("합류한 재시도의 답");
        assert!(r2.idempotent_replay, "합류한 재시도에 재생 표지가 없다");
        assert_eq!(r2.result, r1.result);
    }

    /// 통제군 — plugin **고유** 이름(표가 모른다 → `Outside`)은 보존소가 개입하지 않는다. 같은
    /// 키로 두 번 오면 두 번 forward 되고, plugin 은 원래 요청을 그대로 받는다(ADR-0361).
    ///
    /// prefix 를 **등록한 채** 부른다 — 운영에서 forward 는 prefix 가 점유됐을 때만 일어나므로
    /// 그것이 이 함수가 plugin 고유 이름을 받는 유일한 상태다. 그 상태에서 `method_meta` 는 그
    /// 이름을 namespace fallback 으로 `Mutate` 로 해소하므로 [`run_app_layer`] 의 `Mutate`
    /// 판정은 그 이름을 거르지 **않는다** — 거르는 것은 `forward_keeping_the_key` 의 `Kept`
    /// 판정뿐이고, 이 시험이 그 판정을 잰다. 등록하지 않으면 해소가 `None` 이라 `Mutate`
    /// 판정이 먼저 빼 주고, 그 판정이 없어도 이 시험이 통과한다.
    #[test]
    fn a_plugin_name_is_forwarded_every_time_with_its_request_untouched() {
        const OWNER: &str = "com.test.idempotency-forward-probe";
        const PREFIX: &str = "zzzforwardprobe";
        let table = crate::namespace_table_for_tests::installed_test_table();
        table
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .register(OWNER, PREFIX)
            .expect("test prefix must be free");
        let method = format!("{PREFIX}.run");
        assert_eq!(
            tasty_ipc::method_meta::method_meta(&method).map(|m| m.effect),
            Some(MethodEffect::Mutate),
            "등록된 prefix 아래 이름이 `Mutate` 로 해소되지 않으면 이 시험은 `Kept` 판정을 안 잰다"
        );
        let held = std::cell::RefCell::new(Vec::new());
        for _ in 0..2 {
            let (cmd, _rx) = keyed(&method, "forward-outside-probe");
            forward_keeping_the_key(&CallerContext::Local, &cmd, hold_forward(&held));
        }
        table
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .unregister_plugin(OWNER);
        let held = held.borrow();
        assert_eq!(held.len(), 2, "계약 밖 이름의 두 번째 forward 가 걸러졌다");
        for c in held.iter() {
            assert_eq!(
                c.request.idempotency_key.as_deref(),
                Some("forward-outside-probe"),
                "개입하지 않는 갈래는 원래 요청을 넘긴다"
            );
        }
    }

    fn no_threads(_: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
        Err(std::io::Error::other("no threads left"))
    }

    /// relay 스레드를 못 세우면 보장만 잃고 동작은 키가 없을 때와 같다 — 본문이 한 번 돌고
    /// 답은 **원래 통로로**, 재생 표지 없이 간다. 보존소에는 자국이 안 남아 같은 키의
    /// 재시도는 다시 실행된다.
    ///
    /// 이 층이 안 맡는 이름이면 그 판정은 세지 않는다 — 다음 층이 다시 판정한다.
    #[test]
    fn a_relay_that_cannot_start_runs_the_request_without_the_guarantee() {
        let store = isolated_store();
        let runs = std::cell::Cell::new(0);
        for expected_runs in [1, 2] {
            let (cmd, rx) = app_cmd("no-relay", 1, "a");
            let out = run_app_layer_in(
                store,
                no_threads,
                &CallerContext::Local,
                &cmd,
                false,
                |h| *h,
                answer_now(&runs),
            );
            assert_eq!(out, Some(true));
            assert_eq!(runs.get(), expected_runs, "재시도가 다시 실행돼야 한다");
            let answer = rx.try_recv().expect("원래 통로로 답이 와야 한다");
            assert!(!answer.idempotent_replay);
            assert_eq!(lock(store).len(), 0);
        }
        assert_eq!(lock(store).counts().executed, 2);

        let (cmd, _rx) = keyed("workspace.create", "no-relay-unhandled");
        let out = run_app_layer_in(
            store,
            no_threads,
            &CallerContext::Local,
            &cmd,
            true,
            |h| *h,
            |_| false,
        );
        assert_eq!(out, Some(false));
        assert_eq!(lock(store).counts().executed, 2, "안 맡은 판정은 되돌린다");
    }

    /// 선언은 상수에서 **유도**된다. 리터럴로 다시 적으면 동작과 갈린다.
    #[test]
    fn the_declaration_is_derived_from_the_bounds_it_describes() {
        let d = declaration();
        assert_eq!(d["retention_ms"], RETENTION.as_millis() as u64);
        assert_eq!(d["capacity"], CAPACITY);
        assert_eq!(d["max_response_bytes"], MAX_STORED_RESPONSE_BYTES);
        assert_eq!(d["max_key_bytes"], MAX_KEY_BYTES);
        assert_eq!(d["survives_restart"], false);
    }
}
