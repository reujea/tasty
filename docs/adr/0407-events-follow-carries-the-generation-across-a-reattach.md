# ADR-0407: `events follow` 는 재부착을 넘어 세대를 들고 간다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: events, cli, cursor, epoch, reconnect, compatibility, adr-0323, adr-0405
- **Group**: event-feed

## Context

`tasty events follow` 는 답에 실린 `epoch` 을 보고 세대가 바뀌면 stderr 에
`cli.events.epoch_changed` 를 찍고 위치를 0 으로 되돌렸다. 그런데 그 비교는 **한 연결 안에서만**
일어났다 — 세대는 첫 답이 정했고, 연결이 끊기면 명령이 끝났다. 재시작은 연결도 끊으므로,
재시작 뒤에 이 비교가 참이 되는 실행 경로가 없었다.

실측(격리 headless 인스턴스): 재시작 전 세대에서 위치 2404 까지 읽은 소비자가 재시작 뒤
`--offset 2404` 로 다시 붙자, 새 세대의 `agent.task_finished` 를 stdout 0 줄 · stderr 0 줄로
놓쳤다. 새 연결의 첫 답이 새 세대를 "처음 본 세대" 로 정했기 때문이다.

[ADR-0405](0405-a-position-past-the-end-of-the-feed-is-marked-not-waited-on-silently.md) 가 끝보다
뒤인 위치에 `ahead_of_stream` 표지를 달았다. 그것으로 이 명령은 옛 세대를 들고 있지 않아도
**끝보다 뒤인** 옛 위치를 안다. 끝보다 **안쪽**인 옛 위치는 세대 비교만이 가른다.

## Decision

**세대를 연결 밖으로 꺼낸다 — 재부착 인자로 받고, 끊길 때 찍고, 선택적으로 스스로 다시 붙는다.**

- `--epoch <세대>` 를 받는다(선택). 주면 첫 답이 정하는 대신 그 값이 알려진 세대가 된다.
  첫 답의 세대가 다르면 `cli.events.epoch_changed` 를 찍고 0 부터 잇는다. 안 주면 예전과
  같다(첫 답이 정한다).
- 답의 `ahead_of_stream` 이 참이면 `cli.events.ahead_of_stream` 을 stderr 에 찍고 0 부터 잇는다.
  그 답의 사건은 찍지 않는다. 필드가 없는 답(표지를 모르는 호스트)은 거짓으로 읽는다.
- **연결마다 첫 요청은 `wait_ms` 0** 이다. 위 두 알림이 `wait_ms`(기본 30 초)만큼 늦지 않게 한다.
  답은 같은 위치에서 같은 사건을 주므로 사건 흐름은 바뀌지 않는다.
- 연결이 끊기면(호스트가 답한 오류가 아닌 전송 실패) 기본은 **예전처럼 끝나되**, 끝나기 전에
  다시 붙을 인자(`--offset <위치> --epoch <세대>`)를 `cli.events.connection_lost` 로 stderr 에
  찍는다. 그 줄을 그대로 붙이면 재시작 여부를 다음 부착이 가른다.
- `--reconnect` 를 주면 끝나지 않고 1 초마다 포트 파일을 다시 읽어 붙는다. 위치와 세대를 들고
  가므로 재시작이면 `epoch_changed` 로 이어진다.
- stdout 은 사건 줄만이다 — 모든 알림은 stderr 다.

## Consequences

- **얻은 것**: 재시작 뒤 옛 위치로 재부착해도 조용하지 않다. `--epoch` 을 넘기면 위치가 끝보다
  안쪽이어도 가르고, 안 넘겨도 끝보다 뒤면 ADR-0405 의 표지로 가른다.
- **얻은 것**: `--reconnect` 하나로 재시작을 넘는 감시 루프를 셸 재시도 없이 만든다.
- **잃은 것**: `--epoch` 없이, 새 세대가 이미 옛 위치를 넘긴 뒤 붙으면 여전히 가를 수 없다 —
  그 위치는 새 세대에서도 실재한다. 끊길 때 찍는 재부착 줄이 `--epoch` 을 담는 이유다.
- **잃은 것**: 끝나는 동작이 기본으로 남아, 재부착 인자를 버리는 스크립트는 예전과 같다.
- **호환**: 기존 인자·stdout 형식·종료 조건은 바뀌지 않는다. 새로 생기는 것은 stderr 줄과 두
  선택 인자뿐이다. 첫 요청을 즉답으로 바꾼 것은 사건을 더 빨리 줄 뿐 다른 사건을 주지 않는다.
  **예외 하나 — 끝보다 뒤인 위치는 알린 뒤 처음부터 준다.** 새 인자 없이도 그렇다. 예전에는 그
  번호에 스트림이 닿을 때까지 조용히 기다렸으므로, 같은 세대에서 끝보다 뒤인 `--offset` 을 준
  스크립트는 이제 링에 남은 과거 사건(최대 `EVENT_RING_CAPACITY` 건)을 stdout 으로 받는다. 호스트가
  준 `next_offset` 은 끝을 넘지 않으므로 그런 값은 재시작 전 세대의 위치이거나 손으로 만든 값뿐이고,
  앞쪽이 이 결정이 고치려는 경우다.

## Alternatives Considered

- **A: 끊기면 항상 다시 붙는다** — 가장 편하다. 안 고른 이유는 호환이다. 끊기면 끝나는 것에
  기대어 감독 프로세스가 재기동하는 스크립트가 있을 수 있고, 그 스크립트는 명령이 영영 안
  끝나 감독이 멈춘다. 그래서 선택 인자로 둔다.
- **B: 세대를 파일에 적어 두고 다음 실행이 읽는다** — 인자 없이도 이어진다. 안 고른 이유는
  서버가 소비자 상태를 안 들기로 한 [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md)
  의 모양을 CLI 가 로컬 상태로 되살리는 것이고, 한 머신의 여러 소비자가 그 파일을 두고 부딪힌다.
  커서를 소비자가 드는 모양에 맞춰 세대도 소비자가 든다.
- **C: 호스트 쪽에서 해결한다(옛 세대 위치를 오류로)** — ADR-0405 의 대안 A 와 같은 이유로
  기각한다. 그리고 끝보다 안쪽인 옛 위치는 호스트도 가를 수 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 사건 링이 디스크에 남아 재시작을 넘는다. 세대가 바뀌지 않으면 이 재부착 규칙의 절반이
  쓸모없어진다.
- `events.fetch` 가 요청에 세대를 받아 서버가 직접 가르게 된다. 그러면 CLI 의 비교는 그
  인자를 넘기는 것으로 줄어든다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 재부착 줄을 버리는 스크립트가 얼마나 되는가. 레포 밖이라 못 잰다. 재는 법: 사용자 보고로
  "재시작 뒤 사건을 놓쳤다" 가 다시 오는지 본다.

## References

- [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) — 커서를 소비자가 드는 피드
- [ADR-0405](0405-a-position-past-the-end-of-the-feed-is-marked-not-waited-on-silently.md) — 끝보다 뒤인 위치의 표지
- 코드 근거(결정이 실현된 현재 위치): `tasty-cli` 의 `events::run_follow` · `events::Cursor::absorb` ·
  `events::Cursor::reattach_args` · `EventsCommands::Follow` 의 `epoch` · `reconnect` 인자
