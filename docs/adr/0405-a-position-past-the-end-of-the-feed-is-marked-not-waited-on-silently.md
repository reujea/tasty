# ADR-0405: 피드 끝보다 뒤인 위치는 조용히 기다리지 않고 표지를 단다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: events, event-bus, cursor, offsets, epoch, compatibility, adr-0322, adr-0323
- **Group**: event-feed

## Context

[ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) 는 위치가
되돌아가지 않으므로 "소비자가 든 위치가 **보존 밖인지 아직 안 온 것인지가 값으로
갈린다**" 고 적었다. 그런데 답에 실은 것은 앞쪽뿐이었다 — 보존 밖이면 `truncated` ·
`skipped` 를 주고, 끝보다 **뒤**인 위치에는 아무 표지 없이 빈 답을 줬다.

실측(격리 headless 인스턴스, 링 끝 ≈9): `events.fetch {offset: 100000, wait_ms}` 가
기다린 뒤 `events [] · next_offset 100000 · truncated false · skipped 0` 으로 답했다.
그 위치를 든 소비자는 스트림이 그 번호에 닿을 때까지 **모든 사건을 조용히 놓친다.**

그런 위치가 생기는 실제 경로는 재시작이다. 재시작하면 위치가 0 부터 다시 매겨지므로
옛 세대의 위치는 새 링의 끝보다 뒤에 있다. 실측으로 새 세대에서 옛 위치 2404 로 다시 붙은
`tasty events follow` 가 새 세대의 `agent.task_finished` 를 0 줄 · stderr 없이 놓쳤다.
`epoch` 은 답에 실려 있지만, 그것으로 가르려면 소비자가 옛 epoch 을 따로 들고 있어야 한다.

같은 물음에 터미널 출력 읽기는 이미 답을 냈다 — `OutputBuffer` 가 끝보다 뒤인 커서를
`AheadOfStream` 으로 가르고, IPC 는 그것을 `cursor_ahead_of_stream` 오류로 거절한다
([ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md)).

## Decision

**끝보다 뒤인 위치에는 답에 표지를 싣는다. 예전 필드는 이름도 값도 바꾸지 않는다.**

- 답에 두 필드를 더한다.
  - `ahead_of_stream: bool` — 요청한 위치가 링의 끝보다 **뒤**였다. 끝과 **같은** 위치는
    다 읽은 소비자가 다음 사건을 기다리는 정상 자리라 표지를 안 단다.
  - `stream_end: u64` — 다음 발화가 받을 위치. 모든 답에 실린다.
- `events` · `next_offset` · `epoch` · `truncated` · `skipped` 는 이 결정 전과 같은 값이다.
  끝보다 뒤인 위치에 대한 `next_offset` 도 요청한 위치 그대로다.
- **즉답으로 바꾸지 않는다.** `wait_ms` 를 준 요청은 끝보다 뒤인 위치여도 예전처럼
  기다린 뒤 답한다. 표지는 그 답에도 실린다. 즉시 알고 싶은 소비자는 `wait_ms` 0 으로 한 번
  묻는다.
- 오류로 거절하지 않는다(아래 대안 A).

## Consequences

- **얻은 것**: 표지를 읽는 소비자는 옛 세대의 위치를 **옛 epoch 을 들고 있지 않아도**
  안다.
- **얻은 것**: `stream_end` 로 소비자가 지금 끝이 어디인지 따로 묻지 않고 안다.
- **잃은 것**: 표지를 **모르는** 소비자는 예전처럼 조용히 기다린다. 이 결정은 그 소비자를
  고치지 않는다 — 고치는 길(대안 A·B)이 그 소비자를 깨거나 바쁜 루프로 만든다.
- **잃은 것**: 끝보다 **안쪽**인 옛 세대 위치(새 세대가 이미 그 번호를 넘긴 경우)는 이
  표지로 안 잡힌다. 그 위치는 새 세대에서도 실재하는 자리라 값으로는 가를 수 없고,
  `epoch` 비교만이 가른다.

## Alternatives Considered

- **A: 오류로 거절한다(`AheadOfStream` 과 같은 모양)** — 터미널 출력 읽기와 모양이 맞는다.
  안 고른 이유는 호환이다. 이 메서드는 이미 끝보다 뒤인 위치에 성공으로 답해 왔고, 그 답을
  받아 기다리던 소비자는 오류를 받는 순간 멈추거나 재시도 루프에 들어간다. 출력 읽기는
  처음부터 거절로 출발해 그 부담이 없었다.
- **B: 즉답한다(대기 없이 표지만 싣는다)** — 즉시 알 수 있다. 안 고른 이유는 표지를 모르는
  옛 소비자가 `next_offset`(=요청 위치)으로 곧바로 다시 묻고, 호스트가 또 즉답하는
  **대기 없는 루프**가 된다는 것이다. 느린 실패(조용한 대기)를 빠른 실패(호스트를 두드리는
  루프)로 바꾸는 것이라 더 나쁘다.
- **C: `next_offset` 을 `stream_end` 로 되돌린다** — 표지를 모르는 소비자도 끝에서부터는
  받는다. 안 고른 이유는 기존 필드의 값을 바꾸는 것이고, 새 세대의 0 부터 끝까지는 여전히
  조용히 놓친다는 것이다. 되돌릴 자리를 0 으로 하면 그 구간은 받지만, 같은 세대에서 위치를
  잘못 든 소비자에게 중복을 조용히 준다.
- **D: 아무것도 안 한다(`epoch` 비교로 충분하다)** — 옛 epoch 을 들고 있는 소비자만 가른다.
  CLI `follow` 는 연결마다 epoch 을 새로 배우므로 재부착 때 그 값을 못 든다. 실측에서 신호 0
  이었던 경로가 바로 그것이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `events.fetch` 에 버전 협상(요청이 자기가 아는 필드를 밝히는 인자)이 생긴다. 그러면
  표지를 아는 소비자에게만 즉답(대안 B)을 줄 수 있다.
- 사건 링이 디스크에 남아 재시작을 넘는다. 그러면 옛 세대 위치가 끝보다 뒤라는 전제가
  무너진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 표지를 모르는 외부 소비자가 얼마나 남았는가. 레포가 못 잰다 — 소비자는 레포 밖이다.
  재는 법: 끝보다 뒤인 위치로 오는 요청 수를 세는 것인데, 그러면 서버가 소비자별 관측을
  들게 되어 [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md)
  과 부딪힌다. 그래서 안 잰다.

## References

- [ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) — 위치가 되돌아가지 않는 링
- [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) — 위치로 읽는 피드
- [ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md) — 같은 물음의 출력 읽기 쪽 답
- [ADR-0407](0407-events-follow-carries-the-generation-across-a-reattach.md) — 이 표지를 읽는 CLI `follow` 의 재부착 규칙
- 코드 근거(결정이 실현된 현재 위치): `EventFetch::ahead_of_stream` · `EventFetch::stream_end` ·
  `EventBus::fetch_blocking` 의 대기 규칙 · `events.fetch` 핸들러의 wire 조립
