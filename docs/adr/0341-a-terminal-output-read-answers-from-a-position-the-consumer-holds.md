# ADR-0341: 터미널 출력 읽기는 **소비자가 든 위치**로 답하고, 못 준 것을 말한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: terminal, output, cursor, retention, ipc, method-effect, identity, adr-0307, adr-0322, adr-0323
- **Group**: event-feed

## Context

원문 출력 버퍼(`OutputBuffer`, 보존 1 MiB)에 읽는 자리를 말하는 수단이 **둘뿐이었고 둘 다
서버의 것**이었다.

- `read_mark` — `surface.set_mark` 이 세우고 `surface.read_since_mark` ·
  `surface.parse_since_mark` 가 읽는다. **surface 당 하나**라 같은 터미널을 보는
  에이전트 셋이 한 창을 공유하고, `set_mark` 하는 쪽이 안 하는 쪽을 민다.
- `scan_mark` — 주기 폴링 소비자 전용([ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md)).
  읽으면 전진하므로 **소비자가 하나라는 전제** 위에 있다. 둘이 부르면 서로의 바이트를
  먹는다.

둘 중 어느 것도 아닌 소비자에게는 자기 자리를 말할 방법이 없었다.

그리고 **잃은 것을 말하는 칸이 없었다.** 마크가 trim 구간에 들면 `None` 이 되고
(`read_mark = None`) 다음 읽기가 `.unwrap_or(0)` 으로 **버퍼 처음부터** 돌아갔다. 응답은
그 사실을 한 글자도 말하지 않는다 — 소비자는 자기가 받은 첫 바이트가 진짜 다음 바이트인
줄 안다. 이 결함은 정본 문서에도 그대로 적혀 있었다
([`docs/features/terminal-output/index.md`](../features/terminal-output/index.md)).

같은 물음 — "순서 있는 기록을 여러 소비자가 각자 속도로 읽는다" — 을 이 레포는 사건 링에서
이미 풀었다: 위치는 소비자가 들고, 서버는 소비자별 상태를 안 들며, 보존 밖 요청에는 건너뛴
수를 함께 답한다([ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) ·
[ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md)).
터미널 원문 쪽만 그 규율 밖에 있었다.

보존 수준은 사용자가 확정했다: **메모리 보존.** 원문 출력을 디스크에 남기지 않고 영속 event
journal 과 통합하지 않는다.

## Decision

**`surface.read_since_mark` 이 위치를 받는다. 위치를 준 호출에 대해 서버는 그 소비자를 위해
아무것도 들지 않는다.**

- **이름을 늘리지 않는다.** `METHOD_TABLE` 의 이름은 0.7.x 동안 **제거 금지**라
  (`tests/api_baseline_0_7.rs`) 새 이름은 영구 비용이다. 같은 자원(한 터미널의 원문)을 같은
  권한 버킷(`terminal.read`)으로 읽는 일이고, 기존 이름이 **더해지는 인자**로 그것을 실어
  나를 수 있다. `cursor` 를 안 주면 예전 그대로 마크에서 읽는다.
- **위치는 절대값이고 되돌아가지 않는다.** 터미널이 낸 원문 바이트 수를 센다. trim 은
  `base` 만 옮기고 마크는 제자리에 둔다. 그래서 **trim 이 지나간 마크도 여전히 견줄 수 있는
  수**이고, 그것이 "조용히 처음부터" 를 "잃은 만큼을 말한다" 로 바꾼다.
- **응답이 좌표를 전부 싣는다** — `retention_start` · `retention_end` · `cursor` ·
  `next_cursor` · `raw_bytes` · `skipped` · `stream`. 마크 형태도 같은 칸을 받는다.
- **`raw_bytes` 와 `text` 의 길이는 다른 수다.** 손실 디코딩이 U+FFFD 를 넣고 `strip_ansi`
  가 바이트를 빼므로 `text` 의 길이는 읽은 구간의 길이가 아니다. 전진은 `next_cursor` 가
  정하고, `text` 길이로 전진한 소비자는 스트림과 어긋난다.
- **`cursor` 에는 `stream` 이 따라야 한다.** surface id 는 닫혔다 열리면 재사용되고
  `surface.respawn_terminal` 은 id 를 그대로 둔 채 터미널을 갈아 끼운다. 표지가 없으면 옛
  위치가 **남의 출력**에 조용히 적용된다. 표지는 버퍼마다 새로 찍고, 호스트 재시작을 건너기
  위한 시계와 같은 나노초를 가르기 위한 계수기를 함께 쓴다.
- **거절 셋에 기계가 읽는 사유를 싣는다** — `error.data.reason` 이
  `cursor_without_stream` · `stream_mismatch` · `cursor_ahead_of_stream` · `no_terminal` 로
  갈린다. 호출자의 다음 동작이 사유마다 다른데 메시지 문자열을 파싱하게 두면 그 판정이 문구에
  묶인다.
- **스트림 끝을 넘은 위치는 빈 답이 아니라 거절이다.** 터미널은 계속 출력을 내므로 그 위치는
  **나중에 유효해진다.** 빈 답으로 돌려주면 소비자는 이어질 것을 기다리고, 실제로 도착하는
  것은 자기가 본 것과 무관한 구간이다.
- **`max_bytes` 는 원문 바이트를 자르고 상한은 보존 크기다.** 보존보다 큰 값은 답을 한
  바이트도 못 늘린다. 값은 `OUTPUT_RETENTION_MAX_BYTES` 한 곳에만 둔다 — IPC 층에 사본을
  두면 보존을 키우는 날 상한만 남아 조용히 어긋난다. 안 주면 보존 전체이므로 **예전 호출의
  `text` 가 그대로 나온다.**
  - *(구현 확정 보강 — 결정 당시 코드가 이미 그렇게 자르던 것을 본문에 적는다)* **하한은 1
    이다 — `0` 은 "한도 없음" 이 아니라 1 바이트로 올린다.** `0` 을 그대로 쓰면 바이트가
    기다리는데도 매 읽기가 빈 답이라 소비자가 영영 못 나아간다(아래 물러서기 규칙이 0 바이트를
    피하는 것과 같은 이유). 한도 없이 읽으려면 인자를 빼면 된다. 음수·정수 아닌 값은 자르지 않고
    인자 오류로 거절한다. 자르는 자리는 버퍼 하나다(`OutputBuffer::read` 의 `clamp`) — IPC
    관문은 받은 값을 그대로 넘긴다.
- **자르는 자리가 문자를 반 토막 내면 뒤로 물러선다** — 최대 3 바이트. 다만 물러서서 0
  바이트가 되면 물러서지 않는다. 바이트가 기다리는데 0 을 돌려주는 읽기는 진전이 없다.
- **`MethodEffect` 는 `Read` 로 둔다.** [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md)
  의 축은 "두 번 전달되면 관측 가능한 차이가 남는가" 이고, 위치를 준 읽기는 서버 커서를
  전진시키지 않는다. `surface.read_since_scan_mark` 가 `Mutate` 인 것과 갈리는 자리가
  정확히 그 차이다.
- **기존 셋은 그대로다.** `surface.set_mark` · `surface.read_since_mark`(인자 없는 형태) ·
  `surface.parse_since_mark` · `surface.read_since_scan_mark` 의 `text` 는 예나 지금이나 같은
  값이다. 더해진 것은 **말해 주는 칸**이다.
- **원문을 디스크에 안 쓴다.** 보존은 메모리 1 MiB 그대로이고 이 결정은 영속 저장을 들이지
  않는다.

### 불가침 원칙 1 의 경계

[`docs/identity.md`](../identity.md) 의 원칙 1 이 상위다. 이 표면이 넘지 않는 선은
[ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) 이 사건
피드에 박은 것과 같다.

1. **위치로 읽는 것은 조회다.** 포커스 · 선택 · 스크롤 · 커서 — 어떤 사용자 상태도 이 호출로
   움직이지 않는다. 위치를 준 형태에는 **포커스 폴백도 없다** — 이름 댄 surface 만 본다.
2. **되감아 화면을 되살리는 길을 내지 않는다.** 이 표면이 주는 것은 원문 바이트뿐이고,
   스크롤 위치를 이 좌표로 쓰거나 읽은 것을 화면에 재생하는 경로를 만들지 않는다.
3. **사용자 입력 자체를 싣지 않는다.** 이 버퍼가 담는 것은 PTY 가 낸 **출력**이다. 사용자가
   친 키가 담기는 것은 셸이 그것을 echo 한 결과이고, 이 결정은 그 앞에 입력 기록을 따로
   들이지 않는다.

## Consequences

- **얻은 것**: 에이전트 여럿이 같은 터미널을 각자 속도로 읽고 서로를 안 민다. 위치를 든
  소비자에게 서버는 아무것도 안 들므로, 소비자가 사라져도 호스트에 남는 것이 없다.
- **얻은 것**: 보존 밖으로 밀려난 출력이 **값으로** 드러난다. 마크 형태도 같이 얻었다 — 이
  결정 전에는 그 갈래가 조용한 재시작이었다.
- **얻은 것**: 같은 숫자 surface 가 다른 스트림을 가리킬 때 옛 위치가 조용히 적용되지 않는다.
- **잃은 것**: 한 이름이 두 모양을 갖는다. `cursor` 의 유무가 무엇을 읽는지를 가르므로,
  이 메서드를 읽는 사람이 두 갈래를 다 알아야 한다.
- **잃은 것**: 위치를 든 소비자가 보존보다 느리면 잃는다. 그것이 메모리 보존의 수준이고,
  잃은 양이 `skipped` 로 나오는 것이 이 결정이 준 전부다.
- **운영 비용**: 위치를 옳게 들지 않는 소비자 — 매번 `retention_start` 부터 읽는 것 — 는 같은
  구간을 되풀이해 받는다. 서버에 쌓이는 것은 없지만 그 중복은 서버가 못 막는다.
- **운영 비용**: CLI 진입점은 `tasty read since-mark --cursor N --stream S [--max-bytes N]` 이다.
  (구현 확정 보강 — CLI 인자 착지 시점: 결정 당시에는 CLI 진입점이 없었고 마크 형태만 냈다.
  아래 재검토 조건 둘째가 그 착지로 발동했고, 재검토 결과 결정은 그대로다 — 인자는 이 한
  이름에 더해졌고 새 이름이 생기지 않았다.) 구 서버가 이 인자를 조용히 버리는 갈래는
  [ADR-0365](0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md)
  가 capability 이름으로 막는다.

## Alternatives Considered

- **A: 새 메서드 이름(`surface.read_output` 류)** — 두 모양이 갈려 읽기 쉽다. 안 고른 이유는
  이름의 비용이다: 0.7.x 동안 제거가 금지라 새 이름은 영구적이고, 같은 자원을 같은 권한으로
  읽는 일에 두 이름을 두면 소비자마다 어느 쪽을 쓸지 고르게 된다. 그리고 그 이름에는 CLI 잎도
  따로 나야 한다.
- **B: 서버가 소비자별 커서를 든다** — 소비자가 사라져도 상태가 남고, 누가 살아 있는지 서버가
  판정해야 한다. `plugin.audit_follow` 와 `events.fetch` 가 이미 반대 모양으로 옳게 푼다.
- **C: `scan_mark` 을 여럿으로 늘린다** — 소비자 수만큼 서버 상태가 늘고, 등록·해제·누수 회수가
  전부 서버 일이 된다. B 와 같은 자리로 간다.
- **D: 보존 밖 위치를 조용히 처음부터 준다** — 지금 동작이고 이 결정이 없애려는 것이다.
  소비자가 받은 첫 바이트를 진짜 다음 바이트로 읽는다.
- **E: 스트림 끝을 넘은 위치를 빈 답으로 준다** — 터미널이 계속 출력을 내므로 그 위치는 나중에
  유효해지고, 그때 오는 것은 소비자가 본 것과 무관한 구간이다. 조용한 오접속이 된다.
- **F: `stream` 을 선택으로 둔다** — 안 주는 소비자가 정확히 이 결정이 막으려는 갈래(재사용된
  surface id) 에 그대로 놓인다. 위치를 아는 소비자는 첫 읽기에서 표지도 함께 받으므로 추가
  비용이 없다.
- **G: 위치를 디스크 저널로 받친다** — 사용자가 이번 범위에서 명시적으로 제외했다. 나중에
  필요해지면 **이 조회 계약 밑을 갈아 끼우는 일**이 되도록 계약을 먼저 고정한다
  ([ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) 의 E 와 같은
  자리).
- **H: 위치의 단위를 줄이나 레코드로 둔다** — 원문은 줄 경계가 없는 바이트 스트림이고, PTY 는
  escape 시퀀스 중간에서도 끊긴다. 바이트가 아닌 단위를 두면 그 단위를 만드는 쪽이 생기고
  그것이 곧 파싱이다. 파싱은 `surface.parse_since_mark` 의 일이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `surface.read_since_scan_mark` 의 호출자가 0 이 된다. 위치를 든 읽기가 그 소비자까지
  덮으면 서버가 드는 커서가 하나 줄고, [ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md)
  의 전제("소비자가 하나")가 필요 없어진다. 좌변: `git grep -n 'read_since_scan_mark'` 의
  호출 자리.
- `tasty read since-mark` 에 위치 인자가 생긴다. 그러면 위 "CLI 진입점이 아직 없다" 가
  사라지고, `docs/dev-guide/api-conventions.md` 의 CLI 표와 함께 움직인다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 소비자가 위치를 옳게 드는지. **레포가 못 잰다** — 소비자는 레포 밖이다. 재는 법: 같은
  caller 가 계속 `retention_start` 부터 묻는지 서버 쪽에서 세는 것인데, 그러면 서버가
  소비자별 상태를 드는 것이라 이 결정과 부딪힌다. 그래서 안 잰다.
- 보존 1 MiB 가 맞는 값인지. 재는 법: 실제 워크로드에서 응답의 `skipped` 가 0 이 아닌 비율을
  본다. 그 값이 지금은 아무 데도 안 남는다.
- 표지가 같은 나노초에 겹치는지. 계수기가 그것을 막지만, **두 터미널이 같은 나노초에 서는
  일 자체를 시험이 못 만든다.** 재는 법 대신 표지의 **모양**(시계 뒤에 매번 달라지는 마디가
  붙어 있는가)을 인파일 시험이 고정한다.

## References

- [ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md) — 스캐너 전용 커서. 이 결정이
  그것을 대체하지 않는다
- [ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) ·
  [ADR-0323](0323-the-feed-is-read-by-position-and-the-server-keeps-no-consumer-state.md) —
  같은 규율을 사건 링에 박은 결정. 위치·보존·`skipped`·세대 표지의 선례
- [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md) — `MethodEffect`
  의 축
- [`docs/identity.md`](../identity.md) — 불가침 원칙 1
- [`docs/features/terminal-output/index.md`](../features/terminal-output/index.md) ·
  [`docs/reference/api.md`](../reference/api.md) — 표면 기술
- 코드 근거(결정이 실현된 현재 위치): `OutputBuffer::read` · `OutputCursor` ·
  `OUTPUT_RETENTION_MAX_BYTES` · `handle_read_since_mark` 의 `OutputReadParams` 와
  `answered` · `CoreState::read_output`
