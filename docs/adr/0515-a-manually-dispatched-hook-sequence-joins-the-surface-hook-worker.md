# ADR-0515: 수동 발화한 훅 시퀀스는 surface 훅과 같은 실행기에 줄 선다 — ADR-0498 의 수동 발화 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: hooks, hook-handler, ipc, concurrency, logging, adr-0498
- **Group**: webhook-hooks

## Context

[ADR-0498](0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md) 은 surface 훅의
`IpcSequence` 를 전용 `hook-sequence` 스레드 하나에 넘겨 **받은 순서대로 하나씩** 실행하게 했고, 대안 A
("시퀀스마다 스레드를 띄운다")를 순서 때문에 기각했다. 그러면서 `hook_handler.dispatch` 수동 발화는
"이미 발화 스레드가 큐를 비우는 스레드가 아니다" 라는 이유로 바꾸지 않았다. 그래서 수동 발화는 대안 A
의 모양 그대로 남았다 — 발화마다 `hook-dispatch` 스레드를 새로 띄워 `execute_sequence` 를 불렀다.

결과는 둘이었다(2026-09-23 코드 기준).

- **끼어듦**: 잇달아 수동 발화한 두 시퀀스, 또는 수동 발화 시퀀스와 surface 훅 시퀀스의 스텝이 서로
  끼어들어 호스트 큐에 들어갔다. ADR-0498 이 surface 훅 사이에서 막은 것과 같은 형태다. 시험으로
  재현했다 — 수동 발화를 스레드로 되돌리는 변이에서 둘째 시퀀스의 첫 스텝이 첫 시퀀스의 첫 스텝 답
  전에 들어왔다.
- **로그 머리말**: 세 출처(웹훅 · surface 훅 · 수동 발화)가 같은 `execute_sequence` 를 부르는데, 그
  스텝 로그가 출처와 무관하게 `webhook IpcSequence step …` 을 찍었다. 수동 발화나 surface 훅의 실패가
  웹훅 실패로 읽혔다.

## Decision

**`hook_handler.dispatch` 의 `IpcSequence` 는 surface 훅과 같은 실행기(`hook_handler::exec::enqueue_sequence`)
에 넘긴다. 실행기에 넘기지 못했으면 호출자에게 "실행하지 않았다" 오류로 답한다. 스텝 로그는 실제 출처를
머리말로 단다.**

- 두 출처의 시퀀스는 실행기가 **넘겨받은 순서대로 하나씩** 실행된다. 한 시퀀스의 스텝이 모두 끝나야 다음
  시퀀스의 첫 스텝이 들어간다 — 수동 발화끼리도, 수동 발화와 surface 훅 사이도 마찬가지다.
- 응답은 종전처럼 `{"accepted": true, "id", "action": "ipc_sequence", "steps"}` 이다. 실행 결과는 싣지
  않는다(단방향 불변식).
- 실행기에 넘기지 못한 경우 — 대기 시퀀스가 `PENDING_SEQUENCE_LIMIT`(= `INJECTED_DEPTH_LIMIT`, 현재
  256)에 이름, 실행기 스레드를 못 띄움, 실행기 스레드가 멈춤 — 는 `-32603` 과
  `hook handler '<id>' not run — <사유>` 로 답하고 같은 사유를 `error!` 로 남긴다. surface 훅은 답할
  호출자가 없으므로 `error!` 만 남긴다(ADR-0498 그대로). 그래서 `enqueue_sequence` 는 사유를 `Result` 로
  돌려주고 로그는 호출자가 남긴다.
- 스텝 로그 머리말은 `webhook` · `surface hook` · `hook_handler.dispatch` 중 하나다
  (`hook_handler::exec::SequenceOrigin`). 문구의 나머지(`IpcSequence step {i} ({method}) ok` ·
  `… failed: …` · `… not run — the host did not queue it: …`)는 그대로다. 실행기의 시퀀스 단위 줄
  (`<출처> IpcSequence '<id>' running`(debug) · `<출처> IpcSequence '<id>' not run — …`(error))도 머리말이
  `hook` 에서 같은 출처로 바뀐다 — ADR-0498 Decision 이 인용한 `hook IpcSequence '<id>' not run — …` 문구의 개정이다.

**개정하지 않는 것** — ADR-0498 의 나머지 조항은 모두 유효하다: surface 훅을 실행기에 넘기는 것, 실행기가
하나이고 순서대로 도는 것, 대기 상한과 넘칠 때 버리는 것, 스레드를 못 띄우면 발화한 자리에서 대신
실행하지 않는 것, `HookFired` host event 를 발화한 자리에서 바로 내는 것. **웹훅 리스너도 바꾸지 않는다**
— 요청마다 제 스레드에서 `execute_sequence` 를 부르며, 웹훅끼리 · 웹훅과 실행기 사이의 순서는 이 결정이
정하지 않는다.

## Consequences

- **얻은 것**: 에이전트가 `tasty hook-handler dispatch` 를 잇달아 부르면 발화한 순서대로 시퀀스가 끝난다.
  surface 훅과 같은 대상을 만지는 시퀀스가 스텝 단위로 섞이지 않는다.
- **얻은 것**: 넘기지 못한 수동 발화가 "accepted" 로 답하지 않는다. 종전 구조에서는 스레드 생성 실패만
  오류였고 적체는 개념이 없었다.
- **얻은 것**: 실패한 스텝의 로그만으로 어느 경로의 시퀀스였는지 갈린다.
- **잃은 것**: 수동 발화가 앞선 시퀀스(느린 surface 훅 시퀀스 포함 — 스텝 하나가 10 s 까지)를 기다린다.
  종전에는 즉시 제 스레드에서 시작했다. 응답은 기다리지 않고 바로 오므로 호출자가 막히지는 않는다.
- **잃은 것**: 수동 발화가 대기 상한을 surface 훅과 나눠 쓴다. 수동 발화를 폭주시키면 surface 훅
  시퀀스가 넘쳐 버려질 수 있고, 그 반대도 된다.
- **호환**: 스텝 로그 문구의 머리말이 바뀌었다 — `webhook IpcSequence step` 으로 로그를 찾던 사람은
  surface 훅 · 수동 발화 스텝을 더 이상 그 문자열로 못 찾는다(원래 그 문자열이 틀렸던 것이다).
  시퀀스 단위의 `hook IpcSequence '<id>' running` · `… not run — …` 줄도 `<출처> IpcSequence` 로 바뀌어,
  `hook_handler.dispatch` 출처의 줄은 더 이상 `hook IpcSequence '` 로 안 찾힌다(`webhook …` ·
  `surface hook …` 줄은 출처 낱말이 `hook` 으로 끝나 부분 문자열로 여전히 걸린다).
- **운영 비용**: 없음 — 스레드는 줄었다(발화마다 하나 → 공유 하나).

## Alternatives Considered

- **A: 수동 발화를 그대로 두고 끼어듦을 계약으로 문서화한다** — 안 고른 이유는 ADR-0498 이 같은 형태를
  surface 훅 사이에서 결함으로 판정했기 때문이다. 같은 실행 코어를 부르는 두 출처가 순서 계약만 다르면,
  에이전트가 surface 훅으로 옮기거나 수동 발화로 시험할 때 동작이 달라진다.
- **B: 수동 발화 전용 실행기를 따로 둔다** — 수동 발화끼리는 순서가 서지만 surface 훅과는 여전히 끼어든다.
  스레드와 상한도 두 벌이 된다.
- **C: 넘기지 못해도 "accepted" 로 답하고 로그만 남긴다** — surface 훅과 모양이 같아지지만, 수동 발화에는
  답할 호출자가 있다. 실행되지 않은 것을 받아들였다고 답하면 호출자가 그 결과를 영영 기다린다.
- **D: 로그 머리말을 문자열 인자로 넘긴다** — 출처가 셋으로 닫혀 있으니 열거형이 오타를 막는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 수동 발화가 다시 시퀀스마다 스레드를 띄우면
  `adapters::ipc::handler::hook_handler::tests::dispatched_sequences_do_not_interleave_with_each_other_or_with_surface_hooks`
  가 잡는다(`start_dispatched_sequence` 본문을 `std::thread::spawn` + `execute_sequence` 로 바꾸는 변이로
  확인했다 — 둘째 스텝이 첫 스텝 답 전에 들어와 실패, rc=101, 8 회 반복 8 회 실패).
- 스텝 로그가 출처를 잃으면 `hook_handler::exec::tests::step_logs_name_the_origin_that_fired_the_sequence`
  가 잡는다(`SequenceOrigin::SurfaceHook` 의 머리말을 `webhook` 으로 바꾸는 변이로 확인했다 — rc=101).
- ADR-0498 의 실행기가 없어지거나 순서를 보장하지 않게 바뀌면 — 이 결정의 전제가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 수동 발화가 느린 surface 훅 시퀀스 뒤에서 기다리는 것이 실제 자동화를 방해하는가. 재는 법: 호스트
  debug 로그에서 `hook_handler.dispatch IpcSequence '<id>' running` 줄의 시각과 그 dispatch 요청 시각의
  차를 본다.
- 두 출처가 대기 상한을 다투는가. 재는 법: 호스트 로그에서 `sequences are already waiting` 줄을 출처
  머리말별로 센다.

## References

- 관련 문서: [`docs/features/hooks/index.md`](../features/hooks/index.md) "바인딩" 절 · "핸들러 레지스트리" 절 — 이 결정의 현재 운영 상태
- 개정 대상: [ADR-0498](0498-a-surface-hook-sequence-runs-off-the-thread-that-drains-the-queue.md) (웹훅 리스너와 `hook_handler.dispatch` 는 바꾸지 않는다 — 수동 발화 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- **코드 근거 (결정이 실현된 현재 위치)**: `hook_handler::exec` 의 `enqueue_sequence` · `SequenceOrigin` ·
  `SequenceNotQueued`, `adapters::ipc::handler::hook_handler` 의 `handle_dispatch` · `start_dispatched_sequence`.
