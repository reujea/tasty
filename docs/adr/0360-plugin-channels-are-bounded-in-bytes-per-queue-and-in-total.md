# ADR-0360: plugin 채널은 큐마다와 합계로 바이트에 묶인다 — 빈 큐는 한 건을 늘 받는다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: plugin, host-plugin, resource-bounds, backpressure, observability, adr-0315, adr-0339
- **Group**: ipc-transport

## Context

[ADR-0315](0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md) 가
호스트와 plugin 프로세스 사이의 세 채널(요청 · 응답 · 이벤트)을 유한하게 만들었다. 그 상한은
메시지 **개수**(각 1024)였고, 그것이 메모리 상한이라는 논증은 "메시지 하나가 수 KB 라도 곱이
수십 MB" 였다. 그 논증에는 기둥이 둘 있었고 어느 쪽도 강제되지 않았다.

- **메시지 크기에 상한이 없다.** plugin 소켓에는 줄 길이 상한이 없고, `ipc.result` 는 호스트가
  plugin 에 돌려주는 결과(터미널 출력 읽기 같은)를 그대로 싣는다. 수 KB 는 관측이 아니라 가정이다.
- **plugin 수가 고정이 아니다.** 번들은 아홉이지만 서드파티 plugin 은 몇이든 설치된다. 채널 수가
  plugin 수에 비례하므로 개수 상한의 곱도 비례한다.

그리고 개수 상한은 **plugin 하나 안**에서만 걸린다. 여럿이 한꺼번에 밀릴 때 호스트가 쥐는 양은
어디에서도 안 묶인다.

제약은 ADR-0315 와 같다. **wire 를 바꾸지 않는다**(바꾸면 번들 plugin 전부가 버전 bump 를 끌고
온다). 그리고 **호스트 main thread 가 서지 않는다** — 요청 송신은 pump 안에서 일어난다.

## Decision

**두 바이트 상한을 더한다 — 큐 하나의 누적(`QUEUE_BYTES_LIMIT` = 16 MiB)과, 이 프로세스의
모든 plugin · 모든 채널을 더한 합계(`TOTAL_BYTES_LIMIT` = 64 MiB). 포화의 답은 ADR-0315 의
방향 규칙을 그대로 따른다** — 요청은 거절(개수 포화와 같이 `dropped_requests` 로 plugin 에게
알린다, [ADR-0339](0339-the-host-tells-a-plugin-what-saturation-dropped.md)), 응답·이벤트는
reader 스레드가 기다린다.

**상한은 누적에 걸리고, 빈 큐는 한 건을 늘 받는다.** 상한보다 큰 한 건을 영영 못 들이면
기다리는 방향이 교착한다 — 그 메시지 뒤로 소켓 읽기가 서는데 큐에는 비울 것이 없다. 그래서 실제
상한은 `상한 + 큐마다 한 건` 이고, 그 한 건의 크기는 이 결정이 묶지 않는다(아래 재검토 조건).

**재는 것은 소켓에 나가는(들어온) 줄의 바이트다.** 추정하지 않는다: 들어오는 쪽은 reader 가 이미
든 줄의 길이를 쓰고, 나가는 쪽은 **직렬화를 송신 자리(main thread)로 옮겨** 그 결과를 채널에
싣는다. 넣기 전에 크기를 알아야 판정할 수 있기 때문이고, 두 번 직렬화하지 않으려고 결과를 그대로
싣는다(writer 스레드는 줄을 쓰기만 한다).

**합계의 축은 프로세스다.** 장부(`ChannelLedger`) 하나를 프로세스가 공유한다
(`ChannelLedger::process_wide`) — 창마다 매니저를 새로 세울 수 있어도 메모리는 하나다. 판정은 채널
밖의 장부 하나가 락 하나로 한다: 채널마다 원자값을 두면 두 큐가 동시에 합계의 마지막 자리를
가져가는 것을 막을 수 없다. **큐의 몫은 받는 쪽이 사라질 때 통째로 빠진다** — plugin 이 재시작·
비활성될 때 큐에 남은 메시지는 채널과 함께 버려지는데, 장부에서 그 몫을 안 빼면 합계가 영구히
부풀어 다른 plugin 의 자리를 먹는다.

두 수는 **파생이 아니다.** 관측된 분포가 없다. 고른 근거는 관계뿐이다 — 합계가 큐 하나보다 크고
큐 하나 × 번들 plugin 의 채널 수(16 MiB × 27)보다 작아야 둘이 서로 다른 것을 잰다: plugin 하나가
밀리면 큐 상한이, 여럿이 밀리면 합계 상한이 먼저 걸린다. 그 관계는 컴파일 시점 단정으로 고정했다.

**계측을 같이 둔다.** 장부는 지금 쌓인 바이트 · 최댓값 · 방향별 거절 수(큐/합계) · 기다린 수와
누계 시간 · 열린 큐 하나하나를 낸다(`PluginManager::channel_bytes`). 이 값이 두 수가 틀렸는지를 재는
유일한 채널이다. **IPC/CLI 로 내보내는 자리는 아직 없다** — 노출 자리(`system.pressure`)는 다른
작업이 그 응답 모양을 고치고 있어 이 결정에서 건드리지 않았다(아래 재검토 조건).

### 2026-09-21 보강 — 제어 요청은 합계 판정을 면제한다

위 결정은 요청을 한 갈래로 봤다. 그러면 합계가 **다른** plugin 들로 찬 동안 건강한 plugin 의
큐가 비어 있지 않은 순간에 그 plugin 의 ping 과 shutdown 도 `OverBytes(Total)` 로 거절된다. ping
거절이 healthcheck 시한(60 초)을 넘게 이어지면 그 plugin 은 **남의 포화 때문에** 무응답으로
재시작되고, shutdown 이 거절되면 graceful 없이 kill 된다. 포화가 다른 plugin 과 종료 진행을 막는
형태이고, 합계 상한이 격리를 해친다는 아래 "★ 잃은 것" 이 제어까지 번지는 자리다.

**제어(ping · shutdown)는 합계 판정을 면제하고 큐 상한만 본다**(`Admission::Control`). 개수
상한도 그대로다. 면제된 한 건도 합계에 **센다** — 장부는 실제로 쥔 양을 적는다. 큐 상한을 면제하지
않는 이유는 그 큐를 채운 것이 그 plugin 자신이기 때문이다(그 plugin 이 안 읽고 있으면 ping 이
막혀 재시작되는 것이 healthcheck 의 뜻이다). 제어 한 건은 수십 바이트라 면제가 합계를 의미 있게
넘기지 않는다.

대안: ⒜ 제어도 같은 규칙이라고 적기만 한다 — 위 교착성 비용(남의 포화로 재시작·강제 kill)을
그대로 둔다. ⒝ 제어 전용 채널을 둔다 — wire 는 안 바뀌지만 writer 가 두 큐를 순서 있게 섞어야
하고, 그 순서(shutdown 이 앞선 요청을 추월하는가)가 새 계약이 된다. 판정 갈래 하나로 같은 효과를
얻으므로 고르지 않았다.

## Consequences

- **얻은 것**: 호스트가 plugin 채널에 쥐는 메모리에 **처음으로 바이트 상한**이 생겼다. 그전에는
  개수 곱에 메시지 크기를 곱한 값이 상한이었고 메시지 크기는 무한이었다.
- **얻은 것**: 합계가 plugin 을 가로지른다. 여럿이 한꺼번에 밀려도 상한이 선다.
- **얻은 것**: 재시작·비활성 때 큐가 들고 있던 몫이 장부에서 빠진다는 것이 시험으로 고정됐다.
- **★ 잃은 것 — 합계 상한은 한 plugin 의 포화가 다른 plugin 을 막을 수 있게 한다.** 합계가 찬
  동안 다른 plugin 의 요청도 거절되고(`request ... total byte budget`), 다른 plugin 의 reader 도
  기다린다. 격리의 반대 방향이다. 그래서 큐 상한을 합계보다 작게 둬, plugin 하나가 혼자서는 합계를
  못 채우게 했다(16 MiB < 64 MiB). 넷 이상이 동시에 큐 상한까지 차야 합계가 걸린다.
- **잃은 것**: 요청 직렬화가 writer 스레드에서 main thread 로 옮겨왔다. 일의 양은 같다(한 번
  직렬화한다). 큰 요청을 보내는 프레임이 그만큼 길어진다.
- **잃은 것**: 큐마다 한 건의 예외 때문에 상한이 엄밀하지 않다. 한 건의 크기가 무한이면 실제
  상한도 무한이다 — 그 축은 줄 길이 상한의 몫이고 이 결정의 몫이 아니다.
- **운영 비용**: 모든 송수신이 장부 락을 한 번씩 잡는다. 임계구역은 정수 덧뺄셈과 맵 조회다.

## Alternatives Considered

- **개수 상한만 조인다** — 메시지 크기가 무한인 한 개수는 메모리를 묶지 못한다. 조이는 만큼 작은
  메시지의 정상 버스트만 잘린다.
- **plugin 소켓에 줄 길이 상한을 건다** — 올바른 보완이지만 **다른 축**이다. 한 줄의 크기를 묶을
  뿐 쌓이는 양은 안 묶는다. 그리고 넘는 줄을 어떻게 처리할지(연결 종료? 건너뛰기?)가 plugin 쪽
  계약이라 wire 협상이 필요하다. 그래서 분리했다.
- **합계 상한 없이 큐 상한만** — plugin 수에 비례해 상한이 자란다. Context 의 두 번째 기둥이 그대로
  남는다.
- **합계 상한만** — plugin 하나가 합계를 혼자 채워 모두를 막는다. 큐 상한이 그 격리를 맡는다.
- **크기를 추정한다(직렬화 없이)** — `serde_json::Value` 의 크기를 재귀로 세면 직렬화와 거의 같은
  일을 하면서 결과는 근사다. 정확한 값을 한 번 만들어 그대로 쓰는 편이 싸다.
- **나가는 쪽은 writer 가 꺼낸 뒤 센다** — 그러면 큐에 **쌓이는** 양은 못 잰다. 상한이 묶으려는
  것이 바로 그것이다.
- **빈 큐 예외 없이 엄밀하게** — 기다리는 방향이 상한보다 큰 한 건에서 교착한다. 거절 방향은
  교착하지 않지만, 방향마다 규칙이 다르면 같은 장부 위에서 두 불변식을 지켜야 한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 두 기본 상한의 관계(합계 > 큐 하나, 합계 < 큐 하나 × 27)가 깨지면
  `the_two_default_limits_measure_different_things` 가 **컴파일 시점에** 막는다.
- 큐 상한·합계 상한·빈 큐 예외 중 하나가 빠지면 `process::channel_bytes::tests` 의
  `a_queue_refuses_past_its_bytes_but_an_empty_queue_takes_one` ·
  `the_total_is_shared_across_queues` 가 잡는다(각 변이를 실제로 붙여 죽는 것을 확인했다).
- 받는 쪽이 사라질 때 몫이 안 빠지면 `dropping_the_receiver_returns_its_bytes_to_the_total` 이,
  닫은 뒤 늦은 release 가 두 번 빼면 `a_release_after_close_does_not_subtract_twice` 가 잡는다.
- 기다리는 쪽이 받는 쪽이 사라진 뒤에도 자면 `a_waiter_is_released_when_the_receiver_goes_away`
  가 **멈추지 않고 실패한다**(판정을 다른 스레드에서 timeout 으로 받는다).
- 바이트로 버린 요청이 plugin 에게 안 알려지면
  `a_request_over_the_byte_budget_is_dropped_and_reported_like_a_full_queue` 가 잡는다.
- 장부의 바이트가 소켓 바이트와 어긋나면 `the_ledger_counts_the_wire_bytes_of_a_queued_request`
  가 잡는다.
- 제어 요청의 면제는 세 방향으로 고정돼 있다. 각 변이를 실제로 붙여 죽는 것을 확인했다.
  - 제어 요청이 다시 합계 판정을 받게 되면 `control_requests_pass_a_total_filled_by_another_plugin`
    이 잡는다.
  - 면제된 제어 요청이 합계에 안 세이게 되면 같은 시험이 잡는다. 들어간 줄들의 wire 바이트 합과
    합계 증가분을 견준다.
  - 제어 요청이 큐 상한까지 면제받게 되면 `control_requests_still_obey_the_queue_byte_limit` 가
    잡는다.
- plugin 소켓에 줄 길이 상한이 생기면 — 빈 큐 예외의 크기가 묶이므로 "실제 상한은 무한" 이라는
  Consequences 문장이 틀리게 된다. 고쳐 적는다.
- `system.pressure`(또는 다른 IPC/CLI 표면)가 `PluginManager::channel_bytes` 를 읽게 되면 — 위
  "노출 자리가 없다" 는 문장이 낡는다. 그때 원리적으로 안 붙던 아래 첫 항목이 사람의 로그 대조
  없이 재진다. **그리고 두 상한을 그 분포로 다시 정한다** — 지금 값은 계측과 같은 커밋에서
  관계만으로 정했다(측정 → 상한의 순서가 뒤집혔다). 노출이 생긴 날이 재보정의 첫 날이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **정상 사용이 두 상한에 닿는가.** 재는 법: 호스트 로그에서 `over its byte budget` · `total byte
  budget` 을 찾는다(거절마다 호출부가 한 줄 남긴다). 기다리는 방향은 로그가 안 남으므로
  `PluginManager::channel_bytes` 의 `waits` · `peak_total_bytes` 를 본다 — 지금은 그 값을 읽는
  IPC 가 없어 디버거나 시험 하네스로만 읽힌다(위 채널 항목).
- **합계 상한이 격리를 해치는가.** 재는 법: plugin 넷을 각자 큐 상한까지 채운 상태에서 다섯째
  plugin 의 요청 거절률과 namespace 만료율을 본다.

## References

- 관련 ADR: [ADR-0315](0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md)
  — 방향별 포화의 답. 이 ADR 은 그 답을 바꾸지 않고 상한의 축을 하나 더한다.
- 관련 ADR: [ADR-0339](0339-the-host-tells-a-plugin-what-saturation-dropped.md) — 버린 수를 plugin
  에게 알리는 경로. 바이트로 버린 것도 같은 경로를 탄다.
- 관련 ADR: [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) —
  IPC 소켓 수신 쪽에는 줄 바이트 상한이 이미 있다. plugin 소켓에는 아직 없다.
- 관련 dev-guide: [plugin-development](../dev-guide/plugin-development.md) "큐 포화 통지".
- 관련 ADR: [ADR-0456](0456-the-event-ring-evicts-by-count-or-bytes-whichever-comes-first.md) — 이
  장부 밖에 있던 사건 링에 같은 단위의 바이트 상한을 준다.
- **코드 근거 (결정이 실현된 현재 위치)**: `tasty-host-plugin` 의 `process::channel_bytes` 모듈 —
  `QUEUE_BYTES_LIMIT` · `TOTAL_BYTES_LIMIT` · `ChannelLedger` · `ChannelLedger::process_wide` ·
  `QueueMeter` · `metered_channel` · `MeteredReceiver`, 그리고 `PluginProcess::try_send_request` ·
  `RequestSendError::OverBytes` · `PluginManager::channel_bytes`.
