# ADR-0498: surface 훅의 IpcSequence 는 호스트 명령 큐를 비우는 스레드 밖에서 실행한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: hooks, hook-handler, ipc, host-injection, main-thread, concurrency, adr-0451

## Context

surface 훅(notification · bell · output-match · command-completed · process-exit · idle 훅)에
`IpcSequence` 핸들러를 묶으면, 훅이 발화한 자리 — GUI 는 `App` 의 cascade(메인 스레드), headless 는
데몬 루프 — 에서 `hook_handler::trigger::execute_binding` 이 `hook_handler::exec::execute_sequence` 를
**동기로** 불렀다. 그 함수는 스텝마다 `HostIpcInjector::dispatch_even_if_abandoned` 로 명령을 호스트
명령 큐에 넣고 답을 최대 10 s 기다린다. 그런데 그 큐를 비우는 스레드가 바로 기다리는 스레드다. 그래서
스텝마다 10 s 가 꽉 차서 물러났고, 명령은 기한 없이 큐에 남아 스레드가 풀린 뒤 실행됐다
([ADR-0451](0451-a-host-injection-carries-its-wait-as-a-deadline.md) 이 이 경로에 기한을 안 싣는다).

재어진 비용(격리 debug 인스턴스, 2026-09-23, `system.info` 두 스텝짜리 시퀀스):

- GUI(Xvfb), `notification` 훅을 OSC 9 로 발화 — 직후 `tasty list info` 한 번이 **19.57 s**(평시 0.09 s).
  그동안 화면과 모든 IPC 응답이 섰다.
- headless, `output-match` 훅 — 같은 자리 **19.51 s**.
- 두 스텝 모두 로그에 `failed: host_dispatch timeout after 10s` 로 남았다. 실제로는 뒤에 실행됐는데
  실패로 기록됐다.

같은 `execute_sequence` 를 부르는 다른 두 자리 — 웹훅 리스너와 `hook_handler.dispatch` 수동 발화 —
는 이미 제 스레드에서 부르고 있어 이 문제가 없다.

## Decision

**surface 훅의 `IpcSequence` 는 전용 실행기 스레드 하나에 넘기고, 발화한 스레드는 스텝의 답을 기다리지
않고 바로 돌아온다.**

- 실행기는 `hook_handler::exec::enqueue_sequence` 다. 처음 부를 때 `hook-sequence` 스레드를 하나 띄우고,
  넘겨받은 시퀀스를 **받은 순서대로 하나씩** `execute_sequence` 로 실행한다. 한 시퀀스 안의 스텝은 앞
  스텝의 답을 받은 뒤 다음 스텝이 들어가고, 먼저 발화한 훅의 시퀀스가 먼저 끝난다.
- 스텝의 실행·기록은 그대로 `execute_sequence` 다 — 기한 없는 주입(ADR-0451), 실패는 `error!`,
  반환값 없음, 한 스텝이 실패해도 다음 스텝 진행.
- 실행기에 쌓일 수 있는 시퀀스 수(`PENDING_SEQUENCE_LIMIT`)는 호스트 큐의 주입 명령 수 상한
  (`tasty_ipc::admission::INJECTED_DEPTH_LIMIT`, 현재 256,
  [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md))에 **묶는다** — 코드가
  그 상수를 그대로 쓰므로 그것이 바뀌면 이 값도 따라 바뀐다. 근거: 한 시퀀스는 스텝이 하나 이상이므로,
  이 값이 호스트 상한 이상이면 호스트 큐가 받아 줬을 폭주를 여기서 먼저 거절하지 않는다. 넘치면 그 시퀀스는 실행하지 않고 `hook IpcSequence '<id>' not run — …` 를 `error!` 로
  남긴다. 호스트 큐의 입장 거절과 같은 성질이라 다시 걸지 않는다.
- 실행기 스레드를 못 띄웠으면 시퀀스는 실행하지 않고 `error!` 를 남긴다. 발화한 자리에서 대신 실행하지
  않는다 — 그러면 이 결정이 없애려던 정지가 되돌아온다.
- `HookFired` host event 는 종전처럼 발화한 자리에서 바로 enqueue 한다 — 스텝의 결과를 기다려 본 적이
  없다(예전에도 스텝은 스레드가 풀린 뒤에야 실행됐다).
- 웹훅 리스너와 `hook_handler.dispatch` 는 바꾸지 않는다 — 이미 발화 스레드가 큐를 비우는 스레드가 아니다.

## Consequences

- **얻은 것**: 같은 재현에서 발화 직후 `list info` 가 GUI 0.095 s · headless 0.089 s 였다(전 19.57 s ·
  19.51 s). 두 스텝이 실행기가 시퀀스를 받은 뒤 GUI 39 ms · headless 1 ms 안에 `ok` 로 기록됐다 — 예전에는 둘 다 실패로 기록되고 20 s 뒤에
  실행됐다.
- **얻은 것**: 스텝이 진짜로 순차가 됐다. 예전에는 둘째 스텝이 첫 스텝의 답을 못 받은 채(10 s 초과)
  들어갔다.
- **잃은 것**: 훅이 발화한 순간과 스텝이 호스트 큐에 들어가는 순간 사이에 스레드 한 번을 건너는 틈이
  생긴다. 그 사이 같은 스레드에서 이어진 처리(같은 tick 의 다른 cascade)가 스텝보다 먼저 돈다. 예전에도
  스텝은 스레드가 풀린 뒤에 실행됐으므로 "발화한 tick 안에 스텝이 실행된다" 는 보장은 원래 없었다.
- **잃은 것**: 한 시퀀스가 느리면(스텝 하나가 10 s 까지) 뒤에 온 훅의 시퀀스가 그만큼 기다린다. 발화
  순서를 지키려는 대가다. 예전에는 같은 기다림이 메인 스레드 위에 있었다.
- **잃은 것**: 버리는 경로가 새로 생겼다. 예전에는 시퀀스를 버리는 경로가 없었다 — 메인 스레드가 서서 사건
  처리 자체가 느려졌을 뿐, 스텝은 늦게라도 전부 실행됐다. 이제 대기 시퀀스가 상한(`INJECTED_DEPTH_LIMIT`,
  현재 256)에 이르면 넘친 시퀀스는 실행하지 않고 `error!` 로그만 남긴다. 늦게라도 반영하는 쪽을 고른
  ADR-0451 과 결이 반대지만, 넘칠 때 기다리게 하면 메인 스레드 정지가 되돌아오고 상한을 없애면 대안 D 의
  무한 적체가 된다.
- **운영 비용**: 프로세스마다 스레드 하나(처음 쓸 때 뜬다), 대기 시퀀스 최대 `INJECTED_DEPTH_LIMIT`(현재 256) 건의 메모리.

## Alternatives Considered

- **A: 시퀀스마다 스레드를 띄운다** — `hook_handler.dispatch` 의 모양이다. 안 고른 이유는 순서다. 같은
  surface 에서 연달아 발화한 두 훅의 스텝이 서로 끼어든다. 예전에는 발화 순서대로 큐에 들어갔다.
- **B: 스텝을 기다리지 않고 큐에 넣기만 한다** — 스레드가 필요 없다. 안 고른 이유는 기록이다. 스텝의
  결과를 받을 자리가 없어져 실패가 로그에서 사라지고, 둘째 스텝이 첫 스텝의 효과 전에 실행될 수 있다.
- **C: 대기 상한을 줄인다** — 정지가 짧아질 뿐 없어지지 않고, 스텝은 여전히 늘 시간 초과로 기록된다.
- **D: 대기 시퀀스 수에 상한을 두지 않는다** — 출력 매치 같은 폭주가 메모리를 제한 없이 쌓는다.
  예전에는 메인 스레드가 서서 사건 처리 자체가 느려지는 것이 (나쁜) 제동이었다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 발화한 스레드가 다시 스텝을 기다리면
  `hook_handler::trigger::tests::an_ipc_sequence_hook_returns_before_its_steps_are_answered_and_runs_them_in_order`
  가 잡는다(`enqueue_sequence` 안에서 `execute_sequence` 를 동기로 한 번 부르는 변이로 확인했다 —
  20.0 s 대기로 실패). 같은 시험이 한 시퀀스 **안**의 순서 — 둘째 스텝이 첫 스텝의 답 전에 들어오지
  않는다 — 도 잡는다(`execute_sequence` 의 스텝 대기를 `STEP_TIMEOUT / 10_000` 으로 줄이는 변이로
  확인했다 — `the second step must wait for the first step's answer` 로 실패, rc=101).
- 시퀀스 **사이**의 순서가 깨지면 — 먼저 발화한 훅의 시퀀스가 끝나기 전에 뒤 훅의 스텝이 들어가면 —
  `hook_handler::trigger::tests::ipc_sequence_hooks_fired_back_to_back_do_not_interleave_their_steps`
  가 잡는다(`sequence_worker` 의 루프 본문을 작업마다 `std::thread::spawn` 으로 바꾸는 변이, 곧 대안 A 로
  확인했다 — 둘째 시퀀스의 스텝이 먼저 들어와 실패, rc=101). 이 성질이 필요 없어지면 대안 A 를 다시 볼 수
  있다.
- `execute_sequence` 가 스텝의 결과를 기다리지 않게 바뀌면 — 순서를 지키려고 한 스레드에 모은 이유가
  사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 메인 스레드가 정말 안 서는가. 재는 법: 격리 debug GUI 인스턴스에서 `system.info` 두 스텝짜리 핸들러를
  `notification` 훅에 묶고 `printf '\033]9;probe\007'` 로 발화한 직후 `tasty list info` 왕복을 연속으로
  재서 평시와 견준다.
- 대기 상한(`INJECTED_DEPTH_LIMIT`, 현재 256)에 닿는 사용이 있는가. 재는 법: 호스트 로그에서 `sequences are already waiting` 줄을 센다.

## References

- 관련 문서: [`docs/features/hooks/index.md`](../features/hooks/index.md) "바인딩" 절 — 이 결정의 현재 운영 상태
- 관련 ADR: [ADR-0451](0451-a-host-injection-carries-its-wait-as-a-deadline.md) — 훅 스텝이 기한 없이 들어가는 이유(이 결정은 그 대기를 어느 스레드가 하는지만 옮긴다)
- 부분 개정: [0515](0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md) (수동 발화 조항 개정 — `hook_handler.dispatch` 도 이 실행기에 줄 선다)
- 인용 문구 개정: Decision 의 `hook IpcSequence '<id>' not run — …` 는 [0515](0515-a-manually-dispatched-hook-sequence-joins-the-surface-hook-worker.md) 이후 머리말이 출처(`webhook` · `surface hook` · `hook_handler.dispatch`)로 바뀌었다
- 관련 ADR: [ADR-0457](0457-a-single-plugin-shutdown-is-reaped-off-the-main-thread.md) — 메인 스레드 위의 대기를 옮긴 같은 모양의 결정
- **코드 근거 (결정이 실현된 현재 위치)**: `hook_handler::exec` 의 `enqueue_sequence` · `sequence_worker` ·
  `PENDING_SEQUENCE_LIMIT`, `hook_handler::trigger` 의 `execute_ipc_sequence_handler`.
