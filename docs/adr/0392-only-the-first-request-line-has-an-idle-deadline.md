# ADR-0392: 첫 요청 줄에만 idle 기한을 건다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, resource-bounds, connection, idle, timeout, compatibility, adr-0304, adr-0327
- **Group**: ipc-transport

## Context

[ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) 의 연결 상한(256)이 센
자리는 연결 스레드가 끝나야 돌아온다. [ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md)
은 쓰기 쪽에 시간 상한을 걸어 "응답을 안 읽는 peer" 가 자리를 영구히 쥐는 길을 막았다. 읽기 쪽에는 같은
상한이 없었다. 연결을 열고 **줄을 한 번도 안 보내는** peer 는 `src/adapters/production/tcp_ipc_server.rs`
의 첫 읽기(`read_first_line`)에서 스레드를 영원히 세운다. 그런 peer 가 256 이면 정상 client 가 못 붙는다.
스트림 업그레이드 뒤에는 읽기 상한(`arm_stream_read_timeout`)이 이미 있으므로, 비어 있는 자리는 업그레이드
판별 전의 첫 줄과 요청-응답 연결의 요청 사이, 이렇게 둘이다.

그 소켓의 인구를 세면 다음과 같다(plugin 은 이 서버가 아니라 별도 listener 로 붙는다).

- CLI 한 번 호출 — 연결 직후 요청을 보낸다.
- CLI 의 오래 붙는 명령(`events follow` 의 long-poll · `plugin audit-follow`) — 요청 사이에 쉰다.
- attach · 원격 browse — 연결 직후 `stream.open` 핸드셰이크를 보낸다.
- 제3자 client — 요청 사이에 얼마나 쉴지 알 수 없다.

## Decision

**연결이 열린 뒤 첫 요청 줄이 `FIRST_LINE_IDLE_TIMEOUT` 안에 끝나지 않으면 호스트가
`ERR_FIRST_LINE_IDLE`(`-32066`)로 답하고 연결을 닫는다. 첫 줄 뒤에는 기한이 없다.**

- **기한은 줄 전체에 걸린다.** 읽기 한 번에만 걸면 한 바이트씩 기한보다 조금 빨리 흘리는 peer 가 영원히
  버틴다. `DeadlineReader`(`src/adapters/production/tcp_ipc_server/first_line.rs`)가 소켓에서 새로 읽을
  때마다 **남은 시간**으로 read timeout 을 다시 건다.
- **값은 `stream::HEARTBEAT_TIMEOUT`(20 s)에서 파생한다.** 물음이 같다 — "이 loopback 소켓의 상대가
  얼마나 진척을 안 내면 죽은 것으로 보는가". 같은 소켓의 쓰기 상한과 attach 읽기 상한도 그 값이다.
- 첫 줄을 받으면 read timeout 을 걷는다. 걷지 못하면 요청 사이에 상한이 남으므로 연결을 닫는다. 조용히 다른
  계약으로 도는 것보다 첫 요청에서 드러나는 편이 낫다.
- 거절 응답은 `id` 가 `null` 이고(받은 줄이 없거나 끝나지 않았다) 문구에 기한과 "nothing ran" 을 싣는다.
  쓰기에는 ADR-0327 의 쓰기 상한을 먼저 건다 — 줄을 안 보내는 peer 는 응답도 안 읽을 수 있다. 그래서
  이 답은 최선 노력이고 client 는 EOF 를 볼 수 있다.
- 시험을 위해 debug 빌드에서만 `TASTY_DEBUG_IPC_FIRST_LINE_IDLE_MS` 로 기한을 바꿀 수 있다. release 에는
  그 코드가 없다.

## Consequences

- **얻은 것**: 줄을 안 보내는 연결이 자리를 쥘 수 있는 시간이 무한에서 20 s 로 줄었다. 그 연결은 사유를
  받는다. 기존 client 중 이 기한에 닿는 것은 없다 — 모두 연결 직후 첫 줄을 보낸다.
- **잃은 것**: **첫 줄을 보낸 뒤 쉬는 연결은 여전히 자리를 쥔다.** 요청 하나를 보내고 입을 닫는 peer
  256 개는 이 결정으로 막히지 않는다. 연결 직후 20 s 넘게 생각하다 요청하는 제3자 client 는 끊긴다.
- **운영 비용 / 유지 부담**: 첫 줄 읽기마다 `set_read_timeout` 시스템 호출이 붙는다(보통 한두 번).

## Alternatives Considered

- **A. 요청 사이에도 idle 기한을 건다.** 자리를 쥐는 두 번째 길까지 닫힌다. 그러나 `events follow`
  같은 CLI long-poll 과 제3자 client 가 끊긴다. 기한을 넉넉히 잡아도 "얼마나 쉬는가" 는 client 가
  정하는 값이라 호스트가 고를 수 없다. 과제의 호환 조건(기존 장수 연결을 끊지 않는다)과 충돌한다. 안
  골랐다.
- **B. 읽기 한 번마다 timeout(`set_read_timeout` 한 번).** 구현이 한 줄이지만 흘려 보내는 peer 를 못
  막는다. 안 골랐다.
- **C. 기한 없이 연결 상한만 올린다.** 자리를 쥐는 비용(스레드·스택)이 그대로 늘어난다. 안 골랐다.
- **D. 기한 만료를 무응답으로 닫는다.** EOF 는 네트워크 단절과 구별되지 않아 client 가 처방을 못
  고른다. ADR-0327 이 전송 계층의 침묵을 없앤 방향과 반대다. 안 골랐다.

## Reconsideration Triggers

**채널이 붙는 것**

- 스트림 핸드셰이크 줄이 연결 직후가 아니라 다른 교환 뒤에 오게 바뀌면. 그때 첫 줄 기한이 attach 를
  끊을 수 있다. 재는 법: attach client(`src/app/attach_client.rs`)가 연결 후 `stream.open` 을 보내기 전에
  읽기를 하는지 본다.

**원리적으로 안 붙는 것**

- 연결 자리 포화(`system.pressure` 의 `connections.refused_saturated` 증가)가 **첫 줄을 보낸 뒤 쉬는**
  연결로 일어난다는 관측. 그때 요청 사이 기한(대안 A)을 opt-in 봉투 필드나 capability 로 다시 본다. 재는
  법: 포화 시점에 살아 있는 연결들의 마지막 요청 시각을 본다.
- 연결 직후 20 s 안에 첫 줄을 못 보내는 정상 client 의 보고. 재는 법: `-32066` 을 받은 client 의 종류를
  본다.

## References

- 코드 근거(결정이 실현된 현재 위치): `src/adapters/production/tcp_ipc_server/first_line.rs` 의
  `DeadlineReader` · `src/adapters/production/tcp_ipc_server.rs` 의 `handle_connection` ·
  `refuse_idle_first_line`
- [ADR-0304](0304-ipc-admission-carries-two-bounds-a-line-and-a-connection-count.md) — 연결 상한
- [ADR-0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md) — 쓰기 상한과 코드로 답하기
- [ADR-0391](0391-the-command-queue-admits-by-queued-bytes-and-injected-depth.md) — 같은 회차의 큐 입장 상한
- [`docs/dev-guide/api-conventions.md`](../dev-guide/api-conventions.md) — 전송 계층 오류 코드 표
