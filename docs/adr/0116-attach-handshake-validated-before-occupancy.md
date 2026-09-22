# ADR-0116: attach 점유는 핸드셰이크가 검증된 뒤에만 잡는다 — proto 불일치와 self-attach 는 점유 전에 거절한다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: attach, remote-attach, occupancy, handshake, protocol-version, self-attach, stream, adr-0040, adr-0052
- **Group**: remote-attach

## Context

attach 점유는 **핸드셰이크 params 만 보고** 잡혔다. 서버의 stream 연결 처리
(`tcp_ipc_server.rs::handle_stream_connection`)는 `stream.open` params 에서
`target_workspace` 를 꺼내 곧바로 attach 요청을 메인 루프에 dispatch 하고, 메인 루프가
`attach_workspace_for_stream` → `OccupancyRegistry::acquire_workspace` 로 그 workspace 의
모든 surface 를 배타 점유했다. 이 경로에는 **핸드셰이크가 성립하는지에 대한 검증이 하나도
없었다** — `StreamOpenParams.proto` 는 파싱만 되고 어디서도 비교되지 않았다.

그 결과 **실패가 확정된 client 가 쓸 수도 없는 점유를 가져갔다.** 점유의 회수는 전적으로
사후적이다 — 연결이 닫히면 EOF(`Disconnected` → `release_all_for_client`), 조용히 죽으면
heartbeat TTL 만료([ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md)). 상대가 소켓을 닫아 주지
않는 구버전/hung peer 면 TTL 이 유일한 회수 수단이라 그 사이 정상 attach 가 전부
`already_attached` 로 거절된다. 버전 불일치는 원격 attach 에서 흔한 실패 경로다.

self-attach(자기 IPC 포트를 대상으로 한 GUI mirror attach)는 여기에 **구조적 실패**가 겹친다.
`attach.into_gui` / 사용자 remote-attach 의 핸드셰이크(`attach_client.rs::attach_handshake`)는
GUI **메인 스레드에서 동기 블로킹**으로 돈다. 그런데 그 응답(`attached_workspace` 디스크립터)을
만드는 것도 같은 메인 스레드다 — accept 스레드는 요청을 큐에 넣을 뿐, 점유와 디스크립터는
메인 루프가 적용한다. 자기 자신을 대상으로 하면 메인 스레드가 자기가 만들어야 할 응답을
기다리며 막혀 **반드시 실패**하고(heartbeat Ping 을 먼저 받거나 read timeout), 그 실패가
정리되는 동안 대상 workspace 점유만 남는다. 이 게이트는 release 빌드에만 있었고
(원칙 1 ②: 사용자 입력 재현은 debug 격리), debug 빌드에서는 그대로 열려 있었다.

## Decision

**attach 점유는 핸드셰이크가 검증된 뒤에만 잡는다.** 실패가 그 시점에 이미 확정된 요청은
점유를 잡기 전에 거절한다. 두 갈래로 집행한다.

1. **프로토콜 버전을 attach dispatch 앞에서 검증한다.** `handle_stream_connection` 이
   `validate_stream_proto` 로 `handshake.proto == STREAM_PROTO` 를 확인하고, 다르면
   `dispatch_stream_attach` 로 가지 않고 연결을 정리한다 — 점유는 아예 잡히지 않는다.
   거절은 **프로토콜에 이미 있는 모양**을 쓴다: `StreamAck{ok:false, proto, error}` 는
   client(`StreamConnection::open_with`)가 이미 검사해 그 `error` 문구로 bail 하므로,
   새 wire 형식 없이 실패 사유가 사용자에게 그대로 전달된다. `proto` 필드가 생략된
   핸드셰이크는 serde default 로 `0` 이 되어 함께 거절된다("모르는 버전은 통과" 구멍 없음).

2. **self-attach 는 GUI attach dispatch 층에서 무조건 거절한다.**
   `attach_client.rs::reject_self_attach` 가 요청 포트를 이 인스턴스의 IPC 포트와 비교해
   같으면 거부한다. 기존 release 전용 게이트(`#[cfg(not(debug_assertions))]`)를 **양쪽 빌드로
   올린다.**

   **이 층인 이유**: 요청 포트와 이 인스턴스 자신의 IPC 포트를 **둘 다** 아는 유일한 층이다.
   서버 accept 쪽은 자기에게 온 loopback 연결이 자기 자신인지 구분할 수단이 없고, 구분하려
   들면 `ssh -L` 터널로 도착하는 **정상** 원격 GUI mirror(역시 loopback 연결이다)까지 함께
   막힌다. 그래서 판정은 "연결이 loopback 인가" 가 아니라 "요청 대상 포트가 내 IPC 포트인가"
   여야 하고, 그 정보는 client 측 dispatch 에만 있다.

   **debug 에서도 막는 이유**: 이 경로의 self-attach 는 위 교착으로 **성립할 수 없다** —
   열어 두어도 얻는 기능이 없고 점유 사고만 남는다. 로컬 self-mirror 검증 수단은 그대로
   남는다: `tasty debug attach`(`crates/tasty-cli/src/local/debug/attach.rs`)는 **별도
   프로세스**의 raw attach client 라 이 메인 스레드 교착과 무관하다. 원칙 1 ② 의 판단은
   유지되며, 이 결정은 거기에 "성립 불가 + 점유 사고" 라는 별개 근거를 더해 무조건 거부로
   올린 것이다.

**사후 회수 경로는 그대로 둔다.** EOF 해제와 heartbeat TTL(ADR-0052)은 여전히 필요하다 —
정상 attach 가 성립한 뒤 끊기는 경우를 덮는 것은 그쪽이고, 이 결정은 **애초에 점유를 잡지
말았어야 할 요청**만 앞에서 걸러낸다. 둘은 대체 관계가 아니라 층이 다르다.

## Consequences

- **얻은 것**: 버전이 안 맞는 client 가 남의 workspace 를 20 초씩 붙잡지 못한다. 실패한
  attach 직후 같은 workspace 에 정상 attach 가 즉시 성공한다. 거절 사유가 `StreamAck.error`
  로 client 에 전달돼, 사용자가 "왜 안 되는지" 를 로그가 아니라 실패 메시지로 본다.
  self-attach 는 성립하지 않는 시도로 점유를 흔들지 않는다.
- **잃은 것**: debug 빌드에서 `attach.into_gui` / 사용자 remote-attach 로 자기 자신을 mirror 해
  보는 시도가 막힌다. 그 시도는 원래 교착으로 실패했으므로 실질 기능 손실은 없지만, "일단
  해 보고 실패를 관찰하는" 경로는 사라진다 — 로컬 self-mirror 는 `tasty debug attach` 로 한다.
- **운영 비용 / 유지 부담**: `STREAM_PROTO` 를 올리면 구버전 client 는 이제 **조용히 실패**가
  아니라 명시적 거절을 받는다. 프로토콜 버전을 올릴 때는 그 거절이 의도한 동작인지(하위
  호환을 유지할 것인지) 함께 판단해야 한다 — 지금은 정확히 같은 값만 통과하는 엄격 매칭이다.
- **검증 범위**: proto 게이트는 단위 테스트(`matching_proto_passes_without_pushing_a_rejection` 일치 통과 / `mismatched_proto_is_rejected_with_an_error_ack` 불일치 거절 ack / `missing_proto_field_defaults_to_zero_and_is_rejected` 생략 시 0 거절)와
  실제 인스턴스 대상 e2e(점유 미획득 + 그 peer 가 붙어 있는 동안 정상 attach 성공) 양쪽이
  고정한다. self-attach 게이트는 e2e 가 **메인 루프 응답성**으로 고정한다 — 게이트를 지우면
  IPC 왕복이 초 단위로 늘어 교착이 실측된다. 점유 유무만 폴링하면 실패가 빠를 때 창을 놓친다.

## Alternatives Considered

- **A: 핸드셰이크는 그대로 두고 EOF/TTL 회수만 개선한다**(TTL 단축 등) — 변경이 가장 작다.
  기각: 소켓을 닫지 않는 상대에게는 TTL 이 유일한 수단이라 창이 남고, TTL 을 줄이면 느린
  네트워크의 정상 세션이 끊긴다(ADR-0052 가 그 값을 그렇게 고른 이유). 애초에 점유를 잡지
  않는 쪽이 확실하다.
- **B: proto 검증을 메인 루프(`attach_workspace_for_stream`)에서 한다** — 거절 사유를
  `attach_error` 로 보내 기존 거절 경로와 통일할 수 있다. 기각: 그러려면 `proto` 를 attach
  요청에 실어 메인 루프까지 옮겨야 하는데, 그 시점은 이미 "attach 를 시도할 자격이 있는가"
  가 아니라 "이 workspace 를 점유할 수 있는가" 를 판정하는 층이다. 연결 자체가 성립하지
  않는다는 판정은 연결을 소유한 accept 층에 두는 편이 층 구분에 맞고, 점유 코드에 프로토콜
  버전 인자가 새로 스며들지 않는다.
- **C: self-attach 를 서버 accept 층에서 막는다** — 한 곳에서 모든 진입을 덮는다. 기각:
  위 Decision 2 의 이유로 자기 자신과 `ssh -L` 정상 mirror 를 구분할 수 없다.
- **D: self-attach 를 비동기로 만들어 실제로 동작하게 한다**(핸드셰이크를 워커 스레드로) —
  거절 대신 기능으로 만든다. 기각: 자기 화면을 자기가 mirror 하는 것은 원칙 1 ② 상 사용자
  입력 재현 성격이라 release 표면에 둘 대상이 아니고, debug 검증 수단은 이미 별도 프로세스
  client 로 존재한다. 얻는 것에 비해 attach 수명주기가 복잡해진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **`STREAM_PROTO` 에 하위 호환 범위가 생긴다** — "정확히 같은 값" 대신 "지원 범위" 매칭이
  필요해지면 `validate_stream_proto` 의 비교 규칙을 그 범위로 바꾼다.
- **핸드셰이크에 인증이 도입된다** — 지금 stream 채널은 인증이 없고(신뢰 경계 = SSH +
  loopback, [ADR-0004](0004-ipc-transport-tcp.md)) `session_token` 을 무시한다. 인증이 생기면
  그 실패도 이 게이트와 같은 자리에서(점유 앞에서) 거절해야 한다.
- **self-attach 가 실제로 필요해진다** — 예컨대 attach 핸드셰이크가 메인 스레드를 막지 않게
  바뀌면(위 대안 D) 교착 근거가 사라지고, 원칙 1 ② 의 정책 판단만 남는다.
- **점유를 잡는 진입점이 늘어난다** — 지금은 workspace/surface attach 둘 다 같은
  `dispatch_stream_attach` 를 지나 이 게이트 뒤에 있다. 그 밖에서 점유를 잡는 경로가 생기면
  같은 검증을 그쪽에도 걸어야 한다.

## References

- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 점유 모델(하드 점유 = 배타)
- [ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md) — 조용한 단절의 TTL 회수(사후 회수 층)
- [ADR-0004](0004-ipc-transport-tcp.md) — stream 채널의 신뢰 경계(SSH + loopback, 인증 없음)
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) — 핸드셰이크·점유·release 경로
- [features/remote-attach](../features/remote-attach/index.md) — 점유 획득/해제의 사용자 관점
