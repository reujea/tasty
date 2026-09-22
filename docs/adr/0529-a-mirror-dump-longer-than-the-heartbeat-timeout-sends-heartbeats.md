# ADR-0529: 서버 heartbeat 시한보다 긴 mirror-dump 도 client 발 heartbeat 을 보낸다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: attach, stream, heartbeat, cli, debug, mirror-dump, silent-failure, compatibility, adr-0052, adr-0400, adr-0450
- **Group**: remote-attach

## Context

서버는 attach 스트림 소켓에 `HEARTBEAT_TIMEOUT`(20 s) read timeout 을 건다. 그동안 client 에서
아무 프레임도 안 오면 끊긴 연결로 보고 EOF 와 같이 점유를 푼다([ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md)).
GUI client 와 CLI raw 브리지는 `HEARTBEAT_INTERVAL`(5 s) 마다 `Ping` 을 보내 그 시한을 갱신한다.

CLI mirror-dump(`tasty debug attach` · `tasty remote attach` · `tasty tool attach` 의 `--dump-after`
기본 모드 — 세 명령이 `crates/tasty-cli/src/local/attach.rs` 의 `run_mirror_dump` ·
`run_workspace_mirror_dump` 를 공유한다)는 attach 직후 손실 통지 선언과 `--send` 한 번을 보낸 뒤
아무것도 안 보냈다. 기본 창 500 ms 에서는 문제가 없었고, dev-guide 는 이것을 "짧게 끝나는 검증
전용 모드라 20 초를 넘기는 사용은 지원 대상이 아니다" 로 적어 두었다. 그러나 명령은 그 값을
막지 않았다:

- `--dump-after 120000` 을 주면 ≈20 s 뒤 서버가 연결을 끊고 점유를 푼다(`attach.list` 로 확인).
- client 는 그것을 오류로 말하지 않는다. reader 스레드가 EOF 로 끝나 `Disconnected` 로 수집을
  멈추고, 그때까지의 화면을 찍고, 종료 코드는 0 이다. loopback 모드(`tasty debug attach`)는
  재연결하지 않으므로 사용자에게는 "120 초 수집" 과 "20 초 수집 후 조용히 끝남" 이 같아 보인다.

고칠 방향은 둘이었다 — dump 도 heartbeat 을 보내거나, 시한보다 긴 `--dump-after` 를 거절·경고한다.
둘 다 조용한 실패를 없앤다. 조건은 20 s 이하 사용이 무변경인 것이다.

## Decision

**mirror-dump 수집 루프도 `HEARTBEAT_INTERVAL` 마다 client 발 `Ping` 을 보낸다.** 별도 스레드 없이
수집 루프가 대기를 `min(창 끝, 다음 Ping)` 으로 끊고, 루프 머리에서 주기가 됐으면 `Ping` 을 쓴다
(`DumpHeartbeat`). 첫 `Ping` 은 창을 연 지 한 주기(5 s) 뒤다. `Ping` 쓰기가 실패하면 연결이 끊긴
것이므로 서버 EOF 와 같은 `Disconnected` 로 끝낸다(SSH 모드의 재연결 판단도 같다).

`--dump-after` 의 값 범위는 바꾸지 않는다 — 거절도 경고도 없다.

## Consequences

- **얻은 것**:
  - `--dump-after` 가 요청한 창을 끝까지 채운다. 서버가 살아 있는 동안 점유가 창 도중에 풀리지
    않는다.
  - 20 s 이하 사용은 관측 가능한 면에서 무변경이다: 창이 5 s 이하면 wire 로 나가는 프레임이 종전과
    바이트 단위로 같고, 5–20 s 창은 5 바이트 `Ping` 이 더해질 뿐 stdout · stderr · 종료 코드는 같다.
    `Ping` 은 서버가 수신만으로 liveness 로 치는 프레임이라 서버 쪽 처리도 없다.
  - [ADR-0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)
    결정 5 가 전제한 "살아 있는 연결은 `HEARTBEAT_TIMEOUT` 안에 무엇이든 보낸다" 를 dump 도
    지키게 된다.
- **잃은 것**:
  - dump 창에 5 s 마다 5 바이트가 더 나간다.
  - 서버가 멈춘 경우(SIGSTOP 등)의 감지는 그대로다 — client 쪽 read timeout 은 서버의 `Ping` 이
    갱신하므로 이 변경과 무관하게 ≈`HEARTBEAT_TIMEOUT` 에 `Disconnected` 로 끝난다.
- **운영 비용 / 유지 부담**: 두 수집 루프가 같은 `DumpHeartbeat` 를 쓴다. 시험은 주기를 50 ms 로
  줄인 빌드(`#[cfg(test)]`)에서 서버 시한을 같은 비율(주기 × 4)로 건 가짜 서버로 잰다
  (`dump_heartbeat_tests` — surface · workspace 두 루프 각각). 각 루프에서 `Ping` 송신을 빼는
  변이가 그 루프의 시험을 실패시킨다.

## Alternatives Considered

- **시한보다 긴 `--dump-after` 를 거절한다** — 안 골랐다. 조용한 실패는 없어지지만, 긴 창을 쓰는
  기존 절차(dev-guide 의 SIGSTOP 실측은 `--dump-after 30000`, ADR-0450 의 실측은 `18000`)를
  하나 이상 깨뜨리고, 거절 문턱이 서버 상수의 사본이 되어 그 상수가 움직일 때 함께 움직여야 한다.
  heartbeat 은 같은 목적을 값 범위를 줄이지 않고 이룬다.
- **긴 값에 경고만 찍는다** — 안 골랐다. 경고 뒤에도 연결은 여전히 20 s 에 끊기므로 실패가 조용하지
  않을 뿐 여전히 실패다.
- **raw 브리지처럼 별도 heartbeat 스레드를 띄운다** — 안 골랐다. dump 는 writer 를 수집 루프 하나가
  쥐고 있어 루프 안에서 보내면 잠금도 종료 신호도 필요 없다. raw 브리지는 stdin 라우팅이 같은
  writer 를 공유해 스레드와 `Mutex` 가 필요했던 것이다.
- **ADR-0450 이 기각한 "CLI dump 에도 심장박동을 붙인다" 와의 관계** — 그 기각은 **손실 통지 지연**을
  줄이는 수단으로서의 기각이었다(기본 창 500 ms 가 주기 5 s 보다 짧아 통지 지연에 아무 효과가 없다).
  이 ADR 의 목적은 연결 생존이고, 0450 의 결정(빚은 sink 에 자리가 나는 순간 갚는다)은 그대로다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- 서버가 attach 스트림의 read timeout 을 client 선언(capability)에 따라 끄거나 늘리게 된다 — 그러면
  dump 가 heartbeat 대신 선언으로 시한을 벗어날 수 있다. 판정: `src/adapters/production/tcp_ipc_server.rs`
  의 `set_read_timeout` 인자가 `stream::HEARTBEAT_TIMEOUT` 이 아닌 값이 되는 자리.
- mirror-dump 수집 루프가 writer 를 다른 스레드와 공유하게 된다(예: dump 중 대화형 입력) — 그때는
  루프 안 송신이 아니라 raw 브리지 형태의 공유 writer 가 필요하다.

**원리적으로 안 붙는 것**

- 긴 dump 가 `Ping` 을 보내는데도 창 도중에 끊기는 사례가 관측된다. 재는 법: 격리 debug 인스턴스에
  `tasty debug attach <surface> --dump-after 60000` 을 걸고 30 s · 55 s 에 IPC `attach.list` 로
  점유가 남아 있는지 본다.

## References

- 선행 결정: [ADR-0052](0052-attach-heartbeat-ttl-hard-occupancy-release.md) (서버가 TTL 만료로
  점유를 푸는 규칙 — 이 ADR 은 dump 가 그 규칙에 안 걸리게 한다, 규칙 자체는 그대로)
- 선행 결정: [ADR-0450](0450-a-pending-loss-notice-is-queued-the-moment-the-sink-has-room.md)
  (기각한 대안 "CLI dump 에도 심장박동" 과 목적이 다르다 — 위 Alternatives)
- 선행 결정: [ADR-0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)
  (결정 5 의 "살아 있는 연결은 시한 안에 무엇이든 보낸다" 전제)
- 탐색: `git grep -ln 'dump' -- docs/adr/` · `git grep -ln 'HEARTBEAT' -- docs/adr/`
- 운영 문서: [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "연결 생존 확인"
- 코드 근거(현재 위치): `crates/tasty-cli/src/local/attach.rs` 의 `DumpHeartbeat` · `run_mirror_dump` ·
  `run_workspace_mirror_dump` · `dump_heartbeat_tests`
