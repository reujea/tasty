# ADR-0157: 끊긴 holder 는 재attach 를 막지 못한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: attach, occupancy, stream, ordering, headless, adr-0040, adr-0052
- **Group**: remote-attach

## Context

attach stream 의 inbound 는 한 배치(`PumpOutcome`)로 실려 오고, 그 배치의 적용 순서는
gui(`App::apply_stream_outcome`)와 headless(`boot::headless_stream::apply`)가 각각
**선언된 호출 순서**로 갖는다. 두 곳 모두 순서가 같다 — attach 결선이 먼저, 연결 종료
정리가 마지막.

그 순서에는 이유가 있다. 끊긴 client 가 죽기 직전에 보낸 입력 프레임은 그 client 의
점유가 살아 있는 동안 적용돼야 하고(`feed_attached_input` 은 holder 를 검증한다),
그러려면 lock 해제가 입력 적용보다 뒤여야 한다.

문제는 그 순서가 **제3자에게도 적용된다**는 것이다. 한 배치에 "C1 끊김" 과 "C2 의
attach" 가 함께 실리면, C2 의 `acquire_workspace` 는 같은 배치의 끝에서 사라질 것이
확정된 C1 의 lock 을 보고 `already_attached` 를 돌려준다. 서버는 이미 답을 알면서
틀린 답을 준다.

**측정.**

- 원인은 확률이 아니라 정적으로 선언된 순서다. 부하는 두 이벤트가 한 배치에 실릴
  확률만 바꾼다.
- 기존 회귀(`tests/attach_silent_disconnect.rs::closing_before_the_descriptor_releases_occupancy_promptly`)
  는 이 결함을 보지 못한다 — 결함이 살아 있는 상태에서 헤드리스로 25 회 실행해 이 테스트는
  25 회 통과했고, 결함을 되살린 뮤테이션에서도 3 회 전부 `13 passed` 였다.
- 그 테스트가 이 결함에 무력한 이유는 관측점이다. `wait_until_free` 는 `surface.list` 의
  `attached`(=`surface_locks`)만 보는데, attach 가 아직 적용되지 않은 시점의 "false" 와
  해제된 뒤의 "false" 가 같은 값이다(R8 의 형태). 그래서 재attach 요청이 C1 의 결선보다
  먼저 나갈 수 있고, 그 경합은 서버 부하에 따라만 갈린다.

**사용자 비용.** 이 거절은 client 쪽에서 *영구* 충돌로 분류된다 —
`app::auto_attach` 의 `on_reconnect_attempt_failed` 는 `already_attached` 를 지수
백오프 대신 **최대 간격 고정**으로 처리한다. 즉 끊기자마자 곧바로 다시 붙는 정상
재연결이 가장 오래 기다리게 된다.

## Decision

순서를 바꾸지 않는다. 대신 **상태 전이를 거부**한다.

배치 **머리**에서 두 pump 가 `OccupancyRegistry::mark_clients_disconnected` 로 "이
client 들은 이미 끊겼다" 는 사실만 먼저 알린다(해제는 여전히 배치 마지막). acquire 계열
(`acquire` · `acquire_workspace`)은 자기를 막고 선 holder 가 그 표시를 갖고 있으면 그
자리에서 점유를 회수하고 진행한다. 표시는 배치 끝의 `release_all_for_client` 가 lock 과
함께 지운다 — 그래서 표시는 한 배치를 넘어 살지 않고, 재사용된 client_id 가 잘못 죽지
않는다.

회수는 **경쟁이 실제로 있을 때만** 한다. 경쟁이 없으면 lock 은 배치 끝까지 남아 그
client 의 잔여 입력이 정상 처리된다 — 끊김 정리를 마지막에 두는 원래 계약의 목적이
그대로 지켜진다. 경쟁이 있으면 그 잔여 입력은 어차피 주인이 바뀐 자원으로 가므로
버리는 것이 맞다.

**규칙의 소유자는 하나다.** 판정은 전부 `OccupancyRegistry` 안에 있고, gui 와 headless 는
같은 함수를 부르기만 한다.

## Consequences

- **얻은 것**: "안 깨진다" 가 확률이 아니라 불변식이 됐다 — 같은 배치에 실렸는지만
  보므로 부하와 무관하다. 정상 재연결이 최대 백오프로 밀리는 경로가 닫혔다.
- **얻은 것**: 이 규칙은 합성 픽스처로 결정적으로 재현·검증된다(`src/core/attach.rs` 의
  `a_dead_holder_does_not_block_a_workspace_reattach_in_the_same_batch` 외 4 건).
  실제 서버·소켓·sleep 이 판정에 들어가지 않는다.
- **잃은 것**: 경쟁이 있는 경우 끊긴 client 의 잔여 입력 프레임을 버린다. 자원의 주인이
  바뀌었으므로 의도한 동작이지만, 이전에는 (거절 대신) 그 입력이 적용됐다.
- **어느 쪽으로 틀리는가**: 이 판정은 **좁게** 틀린다. 표시가 없으면 예전 행동
  (거절)으로 떨어질 뿐이고, 살아 있는 holder 를 쫓아내는 방향으로는 틀리지 않는다 —
  표시의 출처가 pump 가 보고한 `disconnected` 목록 하나뿐이기 때문이다. 이 비대칭이
  대안 C 를 기각한 이유다.
- **운영 비용**: `OccupancyRegistry` 에 `HashSet` 필드 하나. 한 배치 안에서만 값을
  가지므로 누적되지 않는다.

## Alternatives Considered

- **A: 끊김 정리를 배치 머리로 옮긴다** — 순서를 통째로 뒤집는 형태. 기각. 끊긴
  client 의 잔여 입력 프레임이 **경쟁이 없을 때도** 전부 버려진다. 입력 라우팅이
  holder 를 검증하므로(`feed_attached_input`) lock 이 먼저 사라지면 그 입력은 갈 곳이
  없다. 지금 계약이 보호하던 것을 그대로 깨뜨린다.
- **B: 거절 사유를 client 가 재시도로 처리한다**(`already_attached` 를 일시 실패로
  분류) — 기각. 서버가 틀린 답을 준다는 사실은 그대로 두고 client 에게 보정을
  떠넘긴다. 게다가 *진짜* 영구 충돌과 구분이 사라져 백오프가 폭주 쪽으로 틀린다.
- **C: 레지스트리가 `StreamHub` 에 "이 client 가 아직 붙어 있나" 를 직접 묻는다** —
  기각. `notifier` 로 이미 hub 를 들고 있어 배선이 0 이 되고 늦게 도착한 EOF 까지
  덮는다는 장점이 있지만, **틀리는 방향이 반대**다. sink 등록/해제 시점이 acquire 보다
  늦거나 이르면 *살아 있는* holder 를 죽은 것으로 보고 남의 세션을 조용히 빼앗는다.
  좁게 틀리는 A/현 결정과 달리 이쪽은 넓게 틀리고, 그 오류는 초록으로 지나간다.
- **D: 기존 e2e 회귀를 부하 조건에서 반복 실행해 잡는다** — 기각. 확률을 불변식 자리에
  쓰는 것이다. 같은 파일의 `self_attach_is_rejected_before_it_can_take_occupancy` 가 그
  형태이고(IPC 왕복에 벽시계 상한 2s), 인위 부하 5 회 중 1 회 왕복 10.6s 로 깨졌다.

## Reconsideration Triggers

- 배치 적용 순서를 바꾸는 변경이 들어올 때. 특히 끊김 *정리* 를 앞으로 옮기려는
  변경은 대안 A 를 다시 여는 것이므로 이 ADR 을 먼저 읽어야 한다.
- `mark_clients_disconnected` 를 부르는 **세 번째** 이벤트 루프가 생길 때. 지금 배선
  가드(`both_pumps_mark_disconnects_before_applying_attach_requests`)는 두 자리를
  **이름으로** 못 박고 있어 새 자리를 자동으로 보지 못한다.
- `StreamHub` 가 sink 등록·해제 시점을 acquire 대비 확정적으로 보장하게 되면 대안 C 가
  다시 열린다 — 그때는 배선 두 자리가 사라진다.
- `auto_attach` 가 `already_attached` 를 영구 충돌로 분류하는 것을 그만두면 이 결함의
  사용자 비용 서술이 낡는다(결정 자체는 유지된다 — 서버가 틀린 답을 준다는 사실은
  client 의 해석과 무관하다).

## References

- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 통합 점유 레지스트리(hard/soft)
- [ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md) — silent disconnect 의 TTL 해제 경로
- `src/core/attach.rs` — `mark_clients_disconnected` · `evict_if_dead` 와 합성 회귀
- `src/boot/headless_stream.rs` · `src/app/event_handler.rs` — 두 pump 의 배선
- [attach-behavior](../dev-guide/attach-behavior.md) — release 경로
