# ADR-0328: 응답 대기의 상한은 호출자가 싣고, 만료는 "실행 여부 불명" 이다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, transport, timeout, wire, idempotency, reliability
- **Group**: ipc-transport

## Context

`TcpIpcServer::dispatch_and_await` 는 요청을 메인 스레드로 보낸 뒤 `resp_rx.recv()` 로
**상한 없이** 기다렸다. 기다리는 주체는 GUI 스레드가 아니라 연결 스레드라 굳은 핸들러가
화면을 멈추지는 않는다. 대신 `ConnectionSlot` 하나를 그동안 쥔다.

**영구히 쥐는 경우는 생각보다 좁다.** 응답 통로(`response_tx`)가 **버려지면** 기다림은
즉시 `Err(Disconnected)` 로 끝나고 그 갈래는 이미 처리돼 있었다. 남는 것은 그 통로를
**든 채 끝나지 않는 실행** 하나다.

그리고 그 모양이 바로 **정당한 대기**의 모양이다. 두 자리가 일부러 그렇게 한다.

| 메서드 | 지금의 대기 상한 | 자리 |
|---|---|---|
| `approval.await` | `timeout_ms` 가 0/없음이고 record 에도 없으면 **무한** | `handler/approval/read.rs` |
| `agent.task_await` | 기본 600 초, 명시적 `timeout_ms: 0` 이면 **무한** | `handler/agent/task.rs` |

`approval.await` 는 **사람의 결재를 기다린다.** 사람에게는 상한이 없다. 두 handler 모두
`response_tx` 를 워커 스레드로 옮겨 나중에 회신하므로, 전송 계층에서 보면 "굳은 핸들러" 와
구분되지 않는다.

즉 (문제는 "얼마로 할까" 가 아니라) **"상한을 둬도 되는 요청인가" 를 전송 계층이 알 수
없다는 것**이다.

## Decision

**상한을 요청이 싣고 온다.** `JsonRpcRequest` 에 `response_timeout_ms: Option<u64>` 를
더하고, 서버는 그 값이 있고 0 이 아니면 `recv_timeout` 으로, 아니면 종전대로 `recv` 로
기다린다. **없으면 상한이 없다** — 그것이 이 필드 이전의 동작이고, 기본값이 상한이 되면
위 표의 두 자리가 잘린다.

`Some(0)` 을 무한으로 읽는 것은 **레포에 이미 있는 관례**다. `approval.await` 와
`agent.task_await` 의 `timeout_ms` 파라미터가 그렇게 읽는다. 이 결정은 그 관례를 **봉투
수준으로 올린 것**이고, 그래서 메서드마다 다시 정하지 않아도 모든 메서드에 같은 뜻으로
붙는다. 기다리는 쪽이 얼마나 기다릴지 아는 것은 호출자다.

**만료는 실패가 아니라 결과 불명이다.** `ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN`(-32061)로
답한다. 호스트는 응답 통로를 놓았을 뿐이고 **그 요청은 메인 스레드에서 계속 실행될 수
있다.** 호스트는 실행 여부를 알지만 호출자는 모르고, 호출자가 다음에 할 일이 그 값에
달렸다 — 부수효과가 남는 메서드(`MethodEffect::Mutate`)를 그냥 재전송하면 **두 번째 효과**가
남는다. 기존 코드 중에 이 사실을 뜻하는 것이 없었다: `-32603`(내부 오류)도
`-32000`(서버 사정)도 "안 됐다" 로 읽힌다. 그래서 새 코드 하나를 쓴다.

**만료는 연결을 닫지 않는다.** 뒤늦게 완료된 응답은 수신자가 이미 사라져
`send_response` 에서 조용히 실패하므로, 이 소켓에 끼어들어 다음 요청의 답으로 읽힐 일이
없다. 닫을 이유가 없으면 안 닫는다.

**구 서버는 이 필드를 조용히 무시한다.** `JsonRpcRequest` 에 `deny_unknown_fields` 가
없고, 같은 구조체의 `session_token` 이 이미 같은 방식으로 나중에 붙었다. 그래서 client 는
보내기 전에 물을 자리가 필요하고, `system.info` 의 capability 목록에
`ipc.response-timeout`(판 1)을 더한다. **거절하는 기계는 필요 없다** — client 가 이름을
못 보면 상한이 안 걸린다는 것만 알면 되고, 그 판단은 보내기 전에 끝난다.

## Consequences

- **얻은 것**: 호출자가 **자기 노출을 스스로 자를 수 있다.** 그 전에는 굳은 핸들러에 걸린
  호출을 client 쪽에서 끝낼 방법이 소켓을 닫는 것뿐이었고, 그래도 서버 스레드는 안 풀렸다.
- **얻은 것**: 만료가 **재전송 가능 여부를 말한다.** "결과 불명" 은 `Mutate` 앞에서 상태를
  먼저 읽게 한다.
- **★ 잃은 것 / 정직하게 적는 한계**: **오늘의 호출자는 아무도 이 필드를 안 보낸다.**
  그래서 이 결정만으로는 연결 자리를 쥔 대기가 하나도 줄지 않는다. 이것은 결함이 아니라
  선택의 값이다 — "오늘 효과가 있는" 대안은 전부 위 표의 두 자리를 자른다. CLI·plugin
  client 가 이 필드를 싣는 것은 각자의 후속 결정이고, 그때 `agent task-await --timeout-ms 0`
  같은 **일부러 무한인 호출**은 안 싣는 쪽으로 남아야 한다.
  (구현 확정 보강 — CLI 쪽 후속 결정:
  [ADR-0366](0366-the-cli-bounds-a-single-request-wait-with-a-root-flag.md) 이 루트 플래그
  `--response-timeout-ms` 로 단발 요청에만 싣는다. 기본값은 여전히 "안 싣는다" 라 일부러
  무한인 호출은 플래그를 안 준 채로 그대로다. plugin client 쪽은 아직 아무도 안 싣는다.)
- **운영 비용**: 봉투에 필드가 하나 늘어 **리터럴 19 자리**가 함께 움직였다. 컴파일러가
  전수 강제하므로 조용한 누락은 없다(실측: 다섯 번의 컴파일 회차로 전부 드러났다).
- **운영 비용**: 이름이 겹친다. 레포에는 이미 agent runner 쪽에 `deadline_ms` 가 50 자리
  넘게 있다(`DispatchHandle` 계열). 그래서 봉투 필드를 `deadline_ms` 로 부르지 않고
  `response_timeout_ms` 로 부른다 — grep 이 갈리고, 재는 대상(응답 대기)이 이름에 있다.

### 취소 가능 범위 (보강, 2026-09-21)

만료는 **취소가 아니다.** 시작 전에 만료된 요청은 실행되지 않고 `-32067` 로 답한다 — 그 갈래는
[ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md) 이 이
결정의 "만료는 결과 불명" 을 시작 전/후로 갈라 개정했다. **시작 뒤에는 끊지 않는다** — 메인 스레드의
동기 handler 를 선점할 수단이 없고, 응답을 워커 스레드로 넘긴 요청도 만료로 멈추지 않는다. 만료가
하는 일은 응답 통로를 놓는 것뿐이다. 그 밖의 취소 수단(취소 메서드 · 연결 끊김을 취소로 읽기)은 없다.

## Alternatives Considered

- **블랭킷 기본 상한** — 오늘 효과가 있는 유일한 안이지만 `approval.await` 를 자른다.
  사람의 결재에 상한을 두는 것은 이 결정이 할 일이 아니다.
- **메서드별 상한(표의 새 칸)** — `METHOD_TABLE` 에 그 칸이 지금 없다. 칸을 더하면 284
  항목이 함께 움직이고, 그 값은 **호출마다 다른 것**(같은 `approval.await` 라도 사람이
  볼 때와 자동 승인일 때가 다르다)이라 메서드에 매는 것 자체가 틀린 좌변이다.
- **대기형 메서드만 예외 목록** — 목록이 둘이 되고 갈리면 조용하다. 새 대기형 메서드가
  목록에 안 들어가면 블랭킷 상한이 그것을 자르는데, 그 실패는 런타임에만 보인다.
- **★ 소켓 생존을 보고 끊는다** — 기다리는 동안 peer 가 사라졌는지 `peek` 으로 보고,
  사라졌으면 기다림을 버린다. wire 를 안 바꾸고 **오늘 효과가 있으며** 정당한 대기를 안
  자르는 안이다(기다리는 client 는 살아 있다). 안 고른 이유는 범위다 — 같은 소켓을 읽는
  reader 와 옵션을 공유하므로 read timeout 을 걸었다 푸는 조작이 필요하고, half-close 한
  client 를 죽은 것으로 볼 위험이 있다. **이것은 다음 결정의 후보로 남긴다.**

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 상한이 걸리는 것과 만료가 "결과 불명" 으로 답하는 것은
  `admission_tests::a_wait_past_the_callers_bound_answers_unknown_outcome` 이 잡는다.
  실제 소켓 위에서 재고, 상한을 안 읽게 만들면 5 초 뒤 **실패**한다(hang 이 아니다 —
  완료 신호 자체에 상한을 걸어 뒀다).
- **없으면 상한이 없다**는 것은 `admission_tests::a_request_without_a_bound_is_still_waited_on`
  이 잡는다. 기본값이 상한이 되는 순간 그 시험이 죽는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **호출자가 이 필드를 쓰기 시작했는가.** 쓰는 client 가 하나도 없으면 이 결정은 표면만
  있고 효과가 없다. 재는 법: 호스트 로그에서 `IPC response wait of` 를 찾는다 — 만료가
  날 때마다 `warn` 한 줄이 남는다. 한 줄도 없으면 아무도 안 싣는 것이다.
- **만료 뒤 재전송이 두 번째 효과를 남겼는가.** 이 코드는 호출자에게 **읽고 나서 보내라**
  고 말할 뿐 강제하지 않는다. 재는 법: `Mutate` 메서드의 중복 실행 보고를 만료 로그와
  같은 시각대에서 대조한다. 멱등 키가 생기면 그때 이 자리가 바뀐다.

## References

- 부분 개정: [0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md) (만료의 뜻 개정 — 시작 전 만료는 "실행 안 됨" `-32067`)

- 관련 ADR: [ADR-0311](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md)
  — 만료를 fail-open 이 아니라 오류로 끝낸다는 원칙. 이 결정이 그 원칙을 RPC 봉투 쪽에
  적용하되, **무엇이 불명인지**를 코드로 말하는 자리를 더한다.
- 관련 ADR: [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md)
  — 재전송이 무엇을 남기는지를 메서드가 선언한다. "결과 불명" 이 값을 내는 이유가 거기 있다.
- 관련 ADR: [ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md)
  — 같은 회차의 쓰기 상한. 만료 응답도 그 상한 아래에서 쓰인다.
- 관련 ADR: [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md)
  — capability 이름으로 선언하는 형태.
- **코드 근거 (결정이 실현된 현재 위치)**: `JsonRpcRequest::response_timeout_ms` ·
  `ERR_RESPONSE_TIMEOUT_OUTCOME_UNKNOWN` (`crates/tasty-ipc/src/protocol.rs`) ·
  `TcpIpcServer::dispatch_and_await` · `TcpIpcServer::answer_wait_expired`
  (`src/adapters/production/tcp_ipc_server.rs`) · `CAPABILITIES` 의
  `ipc.response-timeout` (`crates/tasty-ipc/src/capability.rs`).
