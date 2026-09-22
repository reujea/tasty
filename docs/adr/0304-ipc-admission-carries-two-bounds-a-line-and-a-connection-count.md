# ADR-0304: IPC 수신에 상한을 둘 둔다 — 요청 한 줄의 바이트와 동시 연결 수

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, resource-bounds, transport, reliability
- **Group**: ipc-transport

## Context

일반 request-response 경로의 수신에 자원 상한이 하나도 없었다. 두 자리다.

- `read_line` 은 개행을 만날 때까지 `String` 을 키운다. 개행을 끝내 보내지 않는 peer
  **하나**가 호스트 메모리를 끝까지 먹을 수 있었다.
- accept 루프는 연결마다 스레드를 낸다. 세는 것이 없어서, 요청을 한 건도 안 보내는
  연결을 여는 것만으로 스레드와 스택이 무한히 늘 수 있었다.

같은 소켓의 **스트림 업그레이드** 경로에는 대응하는 상한이 이미 둘 있다 — 프레임 길이
접두사를 자르는 `stream::MAX_FRAME_LEN` 과 `TcpIpcServer::arm_stream_read_timeout` 의
read timeout. 즉 이 결정은 없던 방어를 새로 발명하는 것이 아니라 **한쪽에만 있던 것을
반대쪽으로 넓히는 것**이고, 그래서 설계 위험이 낮다.

제약이 하나 있다. 이 회차는 wire 를 바꾸지 않는다. 상한 초과를 **응답으로** 알리는 것은
새 오류 계약이라 호환 협상과 함께 정해야 한다.

## Decision

두 상한을 둔다.

**요청 한 줄의 바이트**를 `MAX_REQUEST_LINE_BYTES`(8 MiB)로 자른다. 이 수는 고르지 않고
**파생한다** — 호스트가 받아들이는 가장 큰 요청 payload 는 저장소 항목 하나이고 그 상한은
`MemoryConfig::entry_max_bytes` 다. 그 값이 한 줄에 실릴 때의 팽창은 두 갈래다:
`value_b64` 는 base64 라 4/3 배, `value` 문자열은 JSON escape 가 최악에 바이트당
`\u00XX` 6 자라 6 배. 그래서 **기본 설정에서** 정상 요청은 6 MiB + 봉투 아래에 있고
8 MiB 가 그 위의 여유다.

**파생의 좌변이 상수가 아니라 설정값이라는 것이 이 결정의 약한 자리다.** `entry_max_bytes`
는 부팅이 `~/.tasty/config.toml` 의 `[memory] entry_max_mb`(기본 1)를 환산해 넣는 값이다
(`src/boot.rs`). 즉 파생은 **기본값에서만** 성립하고, 사용자가 그 설정을 올리면 저장소는
큰 값을 정상으로 받아들이는데 줄 상한은 안 따라 올라간다. 두 수를 잇는 배선은 지금 없다 —
잇는 것은 동작 변경이라 이 회차의 범위 밖이고, 아래 Consequences 에 그 결과를 적는다.

`tasty_memory::MAX_VALUE_BYTES` 를 파생 원천으로 읽지 않는다. 같은 1 MiB 를 적고 있지만
**집행 경로에 없는 상수**라(부팅도 `MemoryConfig::default()` 도 그것을 안 읽는다) 그것을
좌변으로 삼으면 설정 기본값이 움직여도 판정이 안 움직인다.

**동시 연결 수**를 `MAX_CONCURRENT_CONNECTIONS`(256)로 자른다. 자리는 스레드를 띄우기
전에 잡고 `ConnectionSlot` 의 `Drop` 이 돌려준다 — 회수를 연결 처리의 제어흐름(조기 return
이 넷)에 맡기지 않는다. 이 수는 파생이 아니라 **정상 인구의 자리수 여유**다: 번들 plugin 은
아홉이고 각자 host-call 연결을 들며, attach/mesh/bulk 스트림은 워크스페이스당 한 자리
수이고, CLI 는 요청마다 열고 닫는 단명 연결이다.

두 상한 모두 **초과 시 연결을 닫고 응답하지 않는다.** 줄 상한 쪽에는 닫아야 하는 이유가
하나 더 있다 — 초과한 줄의 나머지가 소켓에 남아 있어서, 잘린 JSON 에 parse error 를
돌려주고 계속 읽는 기존 갈래(`send_parse_error` 는 연결을 유지한다)를 쓰면 **한 줄이 여러
요청으로 쪼개져 들어가고 상한이 다시 없는 것이 된다.**

포화 로그는 **들어가는 순간 한 번만** `warn` 이고 그 뒤로는 `debug` 다. 거절은 상대가 이미
상한만큼 쥐고 있을 때만 나는데, 그 상대가 재시도 루프를 돌면 거절마다 한 줄이 나가 로그를
채운다.

## Consequences

- **얻은 것**: 두 자리에 상한이 **생겼다.** 그전에는 어느 쪽도 유한하지 않았다. 정상
  인구가 상한에 닿으면 `warn` 한 줄이 남으므로, 두 수가 틀렸다는 것도 값으로 드러난다.
- **잃은 것**: 두 수의 **곱은 여전히 크다**(256 × 8 MiB). 이 결정은 유한하게 만들 뿐
  조이지 않는다. 조이려면 명령 큐 자체를 bounded 로 바꾸고 포화를 응답으로 알려야 하는데,
  그것이 wire 가 걸리는 다음 결정이다. 지금 `mpsc::channel()` 무제한 큐는 **그대로다.**
- **잃은 것**: 상한 초과가 **조용하다.** client 는 닫힌 소켓만 본다 — 자기 줄이 길어서인지
  네트워크가 끊겨서인지 구분할 수 없다. 호스트 쪽에는 `warn` 이 남는다.
- **잃은 것 (★ 설정 하나로 합법적인 요청이 끊긴다)**: 줄 상한은 `entry_max_mb` 를 따라
  가지 않는다. `entry_max_mb = 2` 면 저장소는 2 MiB 값을 정상으로 받는데, 그 값이 최악으로
  escape 된 `memory.set` 한 줄은 약 12 MiB 라 8 MiB 를 넘는다 — `read_line_capped` 가
  `TooLong` 을 내고 **응답 없이 연결이 닫힌다.** base64 갈래만 쳐도 `entry_max_mb = 7`
  부터 같은 일이 난다. 사용자는 합법적인 설정을 했는데 실패가 오류가 아니라 소켓 종료로
  온다. 고치려면 줄 상한을 설정에서 파생시키거나(동작 변경) 초과를 응답으로 알려야 하는데
  (wire 변경) 둘 다 이 회차 밖이다.
- **운영 비용**: 두 수가 각각 다른 것에 매여 있다. 줄 상한은 `entry_max_mb` 에, 연결
  상한은 번들 plugin 수와 스트림 사용 형태에. 그 둘이 움직이면 여기도 움직여야 한다.
  앞의 것은 **기본값이 움직일 때만** 시험이 잡고, 뒤의 것은 안 잡는다(아래 재검토 트리거).

## Alternatives Considered

- **메서드별 상한** — 어떤 메서드인지 알려면 줄을 먼저 파싱해야 하고, 파싱하려면 줄을 먼저
  다 읽어야 한다. 상한을 정하려는 그 읽기가 상한 없이 일어나므로 순환이다.
- **초과를 응답으로 알린다** — wire 가 걸린다. 새 오류 코드의 의미를 구/신 client 가 같게
  읽는지는 호환 협상에서 정할 일이라 여기서 앞당기지 않았다.
- **세대별 연결 풀 / 유휴 연결 회수** — 상한이 아니라 정책이다. 상한이 없는 상태에서
  회수 정책부터 만들면 여전히 유한하지 않다. 순서가 반대다.
- **줄 상한을 `MAX_FRAME_LEN`(1 MiB)과 같게** — 대칭은 예뻐지지만 `entry_max_bytes` 가
  1 MiB 라 **지금 통과하는 저장 요청이 거절된다.** 대칭보다 행동 보존이 앞선다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `MemorySettings::default().entry_max_mb` 가 올라 줄 상한의 파생을 넘어서면
  `admission_tests::the_cap_clears_the_largest_payload_the_store_accepts` 가 실패한다.
  그 시험의 좌변이 곧 부팅이 설치하는 기본 cap 이다.
- 상한 판정 자체(개행으로 끝난 줄과 잘린 줄의 구분, 거절이 계수를 밀어 올리지 않는 것)는
  같은 `admission_tests` 가 잡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **정상 사용이 연결 상한에 닿는가.** 256 은 실행 인스턴스에서 세어 보정한 값이 아니다.
  재는 법: 호스트 로그에서 `IPC connection refused` 를 찾는다 — 포화로 들어간 순간마다
  `warn` 한 줄이 남는다. 그 줄이 정상 사용 중에 나오면 이 수가 틀린 것이다.
- **사용자가 `entry_max_mb` 를 올려 놓았는가.** 설정에서 온 값은 컴파일 시점에 안 보여서
  **어떤 시험도 못 잡는다.** 재는 법: 호스트 로그에서 `exceeded ... bytes — closing the
  connection` 을 찾고, 같이 그 인스턴스의 `[memory] entry_max_mb` 를 읽는다. 그 값이 2 이상
  이면 이 조합이 원인이다.
- **두 상한의 곱이 문제가 되는가.** 재는 법: 상한만큼의 연결이 각자 상한 가까운 줄을 보낼
  때 호스트 RSS 를 관측한다. 이 ADR 은 그 곱을 재지 않았다.

## References

- 관련 dev-guide: [attach-behavior](../dev-guide/attach-behavior.md) — 스트림 경로가 이미
  들고 있던 두 상한(프레임 길이·read timeout)의 맥락.
- 관련 ADR: [ADR-0004](0004-ipc-transport-tcp.md) — 이 소켓이 TCP loopback 인 이유.
- 부분 개정: [0327](0327-the-transport-answers-instead-of-going-silent-and-its-writes-are-bounded.md) (무응답 종료 조항 개정)
- **코드 근거 (결정이 실현된 현재 위치)**: `TcpIpcServer::read_line_capped` ·
  `TcpIpcServer::MAX_REQUEST_LINE_BYTES` · `ConnectionSlot` ·
  `MAX_CONCURRENT_CONNECTIONS` (`src/adapters/production/tcp_ipc_server.rs`).
