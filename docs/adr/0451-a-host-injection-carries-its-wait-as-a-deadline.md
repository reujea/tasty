# ADR-0451: 호스트 주입도 제 대기 상한을 기한으로 싣는다 — ADR-0411 의 호스트 주입 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, host-injection, timeout, cancellation, reliability, terminal, adr-0411, adr-0391
- **Group**: ipc-transport

## Context

[ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md) 은 소켓 요청에
기한(큐 진입 + 봉투의 응답 대기 상한)을 싣고, 큐에서 기한이 지난 명령은 실행하지 않게 했다. 그러면서
**호스트 주입(`HostIpcInjector`)은 바꾸지 않는다** 고 적었다 — 주입기는 봉투에 상한을 싣지 않고, 제
시간 상한에서 물러날 때 명령을 큐에 남기며, 그 명령은 나중에 실행된다. 안 바꾼 이유는 "주입 호출자가
`InjectError::Timeout` 을 결과 불명으로 읽도록 짜여 있고 그 갈래를 가르는 것은 범위 밖" 이었다.

그 조항이 실행에서 결함을 냈다. `terminal.tell` 은 본문을 쓴 뒤 제출 `\r` 을 별도 스레드에서 주입하고,
그 호출부(`src/adapters/ipc/handler/terminal.rs` 의 `send_body_then_submit`)는 실패해도 **다시 걸지
않는다** — 늦은 `\r` 은 사용자가 그 사이 친 입력 뒤에 붙어 엉뚱한 줄을 제출하기 때문이다. 그런데 주입이
상한(5 s)에서 물러나도 `\r` 은 큐에 남아 있었다. 격리 gui 인스턴스에서 `terminal.tell` 직후 메인
스레드를 7 s 세우자, 5 s 에 `submit \r re-injection failed … host_dispatch timeout after 5s` 가 남고
7 s 뒤 그 `\r` 이 실제로 제출됐다(화면에 명령의 출력이 찍혔다). 호출부가 막으려던 바로 그 일이 "실패"
로 기록된 채 일어났다.

주입기의 호출자는 셋이다(ADR-0391 의 호출자 표와 같다).

| 호출자 | 상한 | 포기 뒤 늦은 실행 |
|---|---|---|
| tell/spawn 의 `\r` 재주입(`terminal.rs`) | 5 s | 해롭다 — 위 결함 |
| agent runner 의 `dispatch_plugin`(`src/core/agent/runner_host.rs`) | 5 s | 해롭다 — task 는 이미 실패로 보고됐고, 그 뒤의 효과는 아무도 모른다. 재시도하면 두 번째 효과다 |
| `execute_sequence`(`src/hook_handler/exec.rs`)를 지나는 모든 훅 스텝 — 웹훅 · surface 훅(notification · bell · output-match · command-completed · process-exit) · idle 훅 · 수동 발화 | 10 s | 필요하다 — 웹훅은 밖에서 이미 ACK 된 사건이고, surface 훅은 다시 오지 않는 사건이다(그 알림·벨·출력·종료는 한 번 일어나고 끝난다). `agent.task_set_result` 스텝이 버려지면 그 task 는 끝나지 않는다 |

## Decision

**주입은 제 응답 대기 상한을 명령의 기한으로 싣는다 — 소켓 요청과 같은 자리와 같은 판정으로.
포기해도 실행돼야 하는 훅 스텝(`execute_sequence` 를 지나는 전부)만 기한 없이 넣는다.**

- `HostIpcInjector::dispatch(method, params, timeout)` 은 `timeout` 을 봉투의 `response_timeout_ms` 에
  싣는다(1 ms 밑이면 1 — `0` 은 봉투 규약상 "기한 없음" 이다). 기한 판정은 ADR-0411 그대로다: 꺼내는
  쪽이 실행 직전에 보고, 기다리는 쪽은 상한에서 상태 칸을 `WITHDRAWN` 으로 옮기려 한다. 한 번의
  비교-교환이 둘 중 하나만 이기게 한다.
- 상한에서 명령이 아직 큐에 있었으면 **새 갈래 `InjectError::Expired`** 로 돌아온다 — 실행되지 않았고
  앞으로도 안 된다(`nothing_ran()` 이 참). 꺼낸 쪽이 먼저 기한 경과를 봐 `-32067` 을 답으로 보낸 경우도
  같은 갈래다. 이미 시작됐으면 종전대로 `InjectError::Timeout`(결과 불명)이다.
- `HostIpcInjector::dispatch_even_if_abandoned` 는 종전 동작이다 — 기한을 싣지 않고, 상한에서
  물러나도 명령은 큐에 남아 나중에 실행된다. `execute_sequence` 를 지나는 모든 훅 스텝(웹훅 · surface 훅 —
  notification · bell · output-match · command-completed · process-exit · idle 훅 · 수동 발화)이 이것을 쓴다.
- 문구: `Expired` 의 `Display` 는 `host_dispatch timeout after <상한> while still queued (nothing ran)`
  이다. 앞부분이 `Timeout` 의 문구와 같아 그 문구로 로그를 찾던 사람은 계속 찾는다. `Timeout` ·
  `Refused` · `Rpc` · `Disconnected` 의 문구는 안 바뀐다.

**개정하는 것**: ADR-0411 Decision 의 "호스트 주입(`HostIpcInjector`)은 바꾸지 않는다" 조항. **개정하지
않는 것**: 기한의 정의(큐 진입 + 상한), 판정 자리(꺼낸 직후 · 게이트 앞)와 상태 칸, `-32067` 의 코드와
문장, "만료는 취소가 아니다" 는 계약, capability `ipc.response-timeout.not-run` 의 이름과 판(1),
ADR-0391 의 주입 깊이 상한과 거절 갈래.

**경합 — 막을 수 없는 창이 둘 있다.** 기한은 실행 **시작 전**의 명령만 멈춘다.

- **시작과 포기가 맞닿는 창.** 꺼낸 쪽이 기한 직전에 `STARTED` 로 옮기면, 호출자는 그 직후 상한에
  닿아 `Timeout` 을 받고 명령은 그대로 실행된다. 이 창의 폭은 기한 판정과 실행 시작 사이(비교-교환
  하나)와 호출자의 타이머가 큐 진입보다 늦게 시작하는 폭(명령을 만든 뒤 송신 · waker 호출까지)이다.
  둘 다 마이크로초 단위지만 0 이 아니다. `Timeout` 이 "결과 불명" 을 말하므로 이 경우의 보고는 거짓이
  아니다.
- **시작 뒤의 만료.** 실행이 시작된 뒤 상한이 지나면(plugin 으로 넘어간 요청 · 굳은 handler) 끊을
  수단이 없다 — ADR-0411 의 계약 그대로다. `\r` 재주입의 `surface.send` 는 시작하면 곧 끝나므로 이
  창이 실제로 열리는 호출자는 runner 의 plugin 호출이다.

## Consequences

- **얻은 것**: 호출자가 포기한 `\r` 은 나중에 제출되지 않는다. 격리 gui 인스턴스에서 같은 재현(tell
  직후 메인 스레드 7 s 정지, 11 s 뒤 화면 읽기)이 바뀌기 전 바이너리로는 "5 s 에 실패 로그 → 정지가
  풀린 뒤 `\r` 제출(화면에 명령 출력)" 이었고, 바뀐 바이너리로는 "5 s 에 `… while still queued (nothing
  ran)` 로그 → 화면에 입력 줄만 남고 출력 없음" 이었다. `queue_dispatch.expired_before_run` 이 1 올랐다.
  정지 없는 tell 은 그대로 제출됐다.
- **얻은 것**: runner 의 task 결과가 "실패" 이면 그 plugin 호출이 뒤에 몰래 실행되지 않는다(시작 전
  만료의 경우). 문구가 `nothing ran` 을 실어 agent 가 그대로 다시 걸어도 되는 경우와 결과 불명을
  가를 수 있다.
- **얻은 것**: 주입이 남긴 만료 명령은 꺼내질 때 비용 없이 버려진다 — 소켓 경로와 같다. 메인 스레드가
  풀린 뒤 포기된 주입 명령이 몰아서 실행되지 않는다.
- **잃은 것 / 호환**: runner 의 task 결과 문구가 시작 전 만료에서 `host_dispatch timeout after 5s` 에서
  `host_dispatch timeout after 5s while still queued (nothing ran)` 로 바뀐다. 앞부분이 같다. 전에는
  이 경우 plugin 호출이 뒤에 실행됐고 이제는 안 된다 — 그 늦은 실행에 기대던 호출자는 이제 효과를
  못 얻는다. 기대던 것이 결함이었다고 판단했다(보고는 "실패" 였다).
- **잃은 것**: 주입 명령은 `expired_before_run` 누계에도 든다 — 그 값이 소켓 요청만 세지 않는다.
- **운영 비용**: 새 주입 호출자는 둘 중 하나를 골라야 한다. 기본 이름(`dispatch`)이 기한을 싣는
  쪽이라 고르지 않으면 안전한 쪽이다.

## Alternatives Considered

- **A: 주입기는 그대로 두고 `terminal.rs` 만 고친다**(재주입 스레드가 제출 직전에 "아직 유효한가" 를
  따로 본다) — 안 골랐다. 늦은 실행은 주입기의 성질이고, runner 도 같은 형태다. 호출부마다 막으면
  새 호출자가 또 빠진다. 또 호출부는 명령이 큐에서 언제 꺼내지는지 모른다 — 그것을 아는 자리는 꺼내는
  쪽뿐이고, ADR-0411 의 상태 칸이 이미 거기 있다.
- **B: 모든 주입에 기한을 싣는다(훅 스텝 포함)** — 가장 단순하다. 안 골랐다. 훅 스텝이 반영하는
  사건은 다시 오지 않는다 — 웹훅은 밖에서 이미 ACK 됐고, surface 훅의 알림·벨·출력 일치·명령 완료·프로세스
  종료는 한 번 일어나고 끝난다. 그래서 메인 스레드가 10 s 넘게 서 있던 동안 일어난 그 사건들의 효과가
  통째로 사라진다. 지금 동작(늦게라도 반영)을 바꿀 근거가 없다 — 호환이 가장 많이 남는 쪽을 골랐다.
- **C: 기한을 호출자의 상한과 다른 값으로 둔다**(예: 상한의 두 배) — 안 골랐다. 호출자가 포기한
  시점과 명령이 무효가 되는 시점이 갈리면, 그 사이에 실행된 명령이 다시 "포기 뒤의 실행" 이 된다.
- **D: 기본 `dispatch` 는 그대로 두고 기한을 싣는 새 메서드를 만든다** — 호출부 문구가 안 바뀐다.
  안 골랐다. 기한이 없는 쪽이 이름을 가지면 새 호출자가 결함을 기본값으로 받는다. 또 runner 호출부
  (`src/core/**`)를 고쳐야 한다.

## Reconsideration Triggers

**채널이 붙는 것**

- 기한을 실은 주입이 큐에서 포기된 뒤 실행되면 `tasty-ipc` `host_call.rs` 의 시험
  `an_injection_abandoned_in_the_queue_is_expired_and_not_run_later` 가 빨개진다.
- 훅 스텝이 기한을 싣기 시작할 때. 좌변: `src/hook_handler/exec.rs` 의 `execute_sequence` 가
  `dispatch_even_if_abandoned` 를 부르는가(`git grep -n dispatch_even_if_abandoned src/hook_handler`).
  이 호출을 지키는 시험은 없다 — 스텝 상한이 10 s 라 시험으로 재면 한 건에 10 s 가 든다. 기한 없는
  주입의 동작 자체는 `host_call.rs` 의 `an_injection_even_if_abandoned_carries_no_deadline_and_runs_later`
  가 본다.

**원리적으로 안 붙는 것**

- 훅 스텝의 늦은 실행이 사고를 내는 보고. 재는 법: `<출처> IpcSequence step … failed: host_dispatch
  timeout` 로그(출처는 `webhook` · `surface hook` · `hook_handler.dispatch`,
  [ADR-0515](0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md))와 같은 스텝의 실행 흔적을 시각으로 대조한다.
- runner task 가 "nothing ran" 을 받았는데 plugin 에 효과가 남은 보고 — 위 경합 창이 실제로 열리는지.
  재는 법: 그 task 의 결과 시각과 plugin 쪽 요청 로그의 수신 시각을 대조한다.

## References

- 개정 대상: [ADR-0411](0411-a-request-whose-deadline-passed-in-the-queue-is-answered-as-not-run.md)
  (호스트 주입은 바꾸지 않는다 — 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 관련: [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) (주입 호출자
  표와 거절 갈래) · [ADR-0412](0412-in-flight-counts-a-started-request-while-its-caller-still-waits.md)
  (주입기가 상태 칸 사본을 드는 이유)
- 코드 근거(결정이 실현된 현재 위치): `tasty-ipc` 의 `host_call::HostIpcInjector::dispatch` ·
  `HostIpcInjector::dispatch_even_if_abandoned` · `InjectError::Expired`, 본체의
  `hook_handler::exec::execute_sequence`
