# ADR-0339: 호스트는 포화로 버린 요청 수를 다음 요청에 얹어 plugin 에게 알린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: plugin, host-plugin, wire-protocol, backpressure, resource-bounds, forward-compat, adr-0315
- **Group**: ipc-transport

## Context

[ADR-0315](0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md)
가 호스트↔plugin 세 채널을 유한하게 만들면서 방향마다 답을 갈랐다. 호스트 → plugin
방향은 **거절**이다 — 송신 자리가 거의 전부 호스트 main thread 의 pump 안이라 거기서
기다리면 큐가 찼다는 이유로 프레임이 통째로 선다.

거절은 호스트 쪽 문제를 닫는다. 그러나 **plugin 쪽에는 아무것도 안 남는다.** 버려진
요청은 소켓에 나가지 않으므로 plugin 은 그것이 있었다는 사실 자체를 모른다. 호스트
로그에는 `RequestSendError::Full` 이 남지만 그것은 호스트의 로그이고, 밀리고 있는 쪽은
plugin 이다 — 자기 작업량을 줄일지 판단할 근거가 정작 판단할 수 있는 쪽에 없다.

**통지를 별도 메시지로 만들 수 없다.** 호스트 → plugin 경로는 그 한 큐뿐이고, 통지가
필요해진 이유가 바로 그 큐가 찼기 때문이다. 별도 메시지를 만들면 포화 상황에서 통지
자신이 포화에 막히고, 안 막히도록 예약 슬롯을 두면 "예약분도 차면 무엇을 하는가" 가
한 겹 더 생긴다.

호환 축도 좁다. plugin 경계의 호환 판정은 capability 협상이 아니라
`HOST_API_VERSION` 의 **동등 비교**다(`crates/tasty-plugin-manifest` 의 `validate`).
그래서 갈래가 둘뿐이다 — 새 필드(`#[serde(default)]`)를 얹으면 구 plugin 이 그대로 돌고,
태그 enum 에 새 variant 를 더하면 구 SDK 가 그 줄에서 역직렬화에 실패하므로
`HOST_API_VERSION` 을 올려야 하고 out-of-tree plugin 이 로드 거부된다.

## Decision

**`PluginRequest` 에 `dropped_requests: u64` 를 더하고, 포화로 버린 수를 다음으로
실제 큐에 들어가는 요청에 얹는다.**

- **왜 다음 요청에 얹는가**: 큐에 자리가 났다는 것은 plugin 이 하나라도 소비했다는
  뜻이다. 그래서 **살아서 밀리는 plugin 은 반드시 이 값을 본다.** 전혀 소비하지 않는
  plugin 은 같은 큐로 가는 ping 도 못 받으므로 healthcheck 가 거둔다(75 s 상한) —
  그쪽은 이 통지가 답할 경우가 아니다. 통지가 자기 슬롯을 필요로 하지 않으므로 위
  자기모순이 성립하지 않는다.
- **의미는 델타다.** 실린 값은 "이 요청 직전까지 버린, 아직 안 알린 수" 이고 실린 만큼
  차감한다. load 와 실제 송신 사이에 늘어난 몫은 그 다음 요청이 싣는다 — 누락도 중복도
  없다.
- **추가형으로 간다.** `#[serde(default, skip_serializing_if)]` 라 양방향 호환이고
  `HOST_API_VERSION` 은 안 움직인다. 0 일 때 키를 안 싣는 것은 비용 때문이다 —
  `surface.set_context` 는 프레임마다 나간다.
- **SDK 가 세 가지로 노출한다.** (1) 값이 0 이 아닌 줄을 받으면 `warn` 로그(모든 요청에
  대해 — `ipc.result`·`shutdown` 은 worker 로 안 가므로 분기 전에 센다), (2)
  `HostHandle::dropped_by_host()` 누적값(clone 한 핸들끼리 공유하므로 자체 background
  thread 에서 읽는다), (3) `Plugin::on_host_dropped_requests(dropped)` 기본 구현
  no-op 콜백 — worker 스레드에서 dispatch 직전 1 회.
- **무엇이 버려졌는지는 안 싣는다.** 호스트도 안 들고 있다(버린 요청은 그 자리에서
  소멸한다). 그래서 이 통지의 처방은 재요청이 아니라 **자기 작업량을 줄이는 것**이다.

## Consequences

- **얻은 것**: 밀리는 plugin 이 그 사실을 자기 프로세스 안에서 안다. `error_code`
  선례([ADR-0171](0171-a-host-error-code-survives-the-plugin-boundary.md))와 같은 모양의
  추가형 필드라 구 plugin 과 양방향 호환이고, ADR-0315 의 "거절" 이 조용하지 않게 된다.
- **잃은 것**: `PluginRequest` 가 필드 하나만큼 넓어졌고, 호스트 본문이 그것을 리터럴로
  지으면 매번 `dropped_requests: 0` 을 적어야 한다. 그래서 **생성자
  `PluginRequest::new` 하나로 입구를 좁혔다** — 본문은 그 필드를 모르고, 값을 정하는
  것은 송신 지점(`PluginProcess::try_send_request`) 하나다.
- **정확도의 한계**: 통지가 늦다. 버린 직후가 아니라 **다음 요청이 들어갈 때** 도착하고,
  그때까지 얼마나 걸릴지는 plugin 의 소비 속도가 정한다. 즉시성이 필요한 처방(예:
  호스트가 그 plugin 에 대한 송신을 잠시 멈추는 것)은 이 채널로 못 짓는다.
- **운영 비용 / 유지 부담**: 원자값 하나(`PluginProcess::dropped_requests`), SDK 의
  원자값 둘, 와이어 필드 하나. 번들 plugin 9 개의 version·매니페스트·`Cargo.lock` 이
  이 변경과 같은 커밋에 온다(plugin protocol 폐포 규칙).

## Alternatives Considered

- **A: 새 `PluginRequest` variant / 전용 제어 메시지** — 기각. 그 메시지도 같은 큐를
  써야 하므로 포화에 막힌다. 그리고 태그 enum 의 새 variant 는 구 SDK 에서 역직렬화
  실패라 `HOST_API_VERSION` 을 올려야 하고, 그러면 번들 9 개 매니페스트의
  `api_version` 과 out-of-tree plugin 의 로드 가부까지 같이 움직인다 — 통지 하나의
  대가가 아니다.
- **B: 예약 슬롯(headroom)을 두고 통지는 그 자리를 쓴다** — 기각. `SyncSender` 에
  길이 질의가 없어 자체 계수기를 덧붙여야 하고, 무엇보다 "예약분도 차면" 이 그대로
  남는다. 문제를 한 겹 미룰 뿐이다.
- **C: 버린 요청을 보관했다 자리가 나면 다시 보낸다** — 기각. 그것은 통지가 아니라
  **무제한 큐의 복원**이다. ADR-0315 가 닫은 바로 그 갈래이고, 게다가 순서가 뒤섞인다
  (`set_context` 같은 최신값-우선 요청은 늦게 도착한 옛 프레임이 해롭다).
- **D: 호스트 telemetry 로만 노출한다** — 기각. 판단해야 하는 주체가 plugin 인데
  호스트 쪽 게이지는 plugin 프로세스가 못 읽는다. 호스트 쪽 관측은 ADR-0315 의
  재검토 조건이 이미 다루고, 이 결정은 그것과 배타가 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 호스트 → plugin 방향이 거절에서 대기로 바뀌었을 때. 그러면 버리는 일 자체가 없어져
  이 필드가 의미를 잃는다. 좌변은 `PluginProcess::try_send_request` 가 `try_send` 를
  쓰는가다. (이 좌변에 붙은 판정기는 지금 없다 — 그 자리를 재는 것은
  `tasty-host-plugin` 의 `the_next_delivered_request_carries_what_saturation_dropped`
  와 `a_full_queue_is_refused_instead_of_awaited` 다.)
- plugin 경계의 호환 판정이 `HOST_API_VERSION` 동등 비교에서 협상으로 바뀌었을 때.
  그러면 "추가형이냐 새 variant 냐" 의 대가 구조가 달라져 위 A 의 기각 근거가 없어진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 통지의 **지연**이 실제로 문제가 됐을 때 — 버린 시점과 알린 시점의 간격이 너무 벌어져
  plugin 의 대응이 늦는 경우. 재는 법: plugin 로그의 `host dropped N request(s)` 줄
  시각과, 같은 구간 호스트 `~/.tasty/debug.log` 의 `request queue full` 경고 첫 줄
  시각을 견준다. 간격이 그 plugin 의 한 프레임보다 크게 벌어지면 즉시성이 필요한
  형태(위 Consequences 의 한계)를 다시 본다.
- 값이 실제로 0 이 아닌 경우가 관측됐을 때 — ADR-0315 의 용량 1024 가 정상 사용에서
  닿는다는 뜻이므로 그 ADR 의 재검토 조건과 함께 본다. 재는 법: plugin 로그에서 위
  경고를 찾고, 그 plugin 이 그 시점에 무엇을 하고 있었는지 대조한다.

## References

- [ADR-0315](0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md)
  — 방향마다 포화의 답이 갈린다는 결정. 이 ADR 은 그 "거절" 갈래에 통지를 붙인다.
- [ADR-0171](0171-a-host-error-code-survives-the-plugin-boundary.md) — host → plugin
  와이어에 추가형 필드를 얹은 선례(`ipc.result` 의 `error_code`), 양방향 호환 시험의 모양.
- [ADR-0311](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md) — namespace
  호출의 만료와 반복 만료. 같은 "느린 plugin" 축의 다른 자리(응답이 안 오는 쪽).
- [plugin-development](../dev-guide/plugin-development.md) "생명주기" — 큐 포화와
  healthcheck 의 관계.
- 코드 근거(결정이 실현된 현재 위치): `tasty_plugin_protocol::PluginRequest` 의
  `dropped_requests` · `PluginRequest::new` · `tasty-host-plugin` 의
  `PluginProcess::try_send_request` · `tasty-plugin-sdk` 의
  `HostHandle::dropped_by_host` · `Plugin::on_host_dropped_requests`.
