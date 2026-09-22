# ADR-0323: 피드는 **위치로 읽고**, 서버는 소비자 상태를 안 든다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: events, ipc, cli, cursor, long-poll, method-effect, identity, adr-0322
- **Group**: event-feed

## Context

링(ADR-0322)이 생겼지만 **버스 밖에서 읽을 길이 없었다.** plugin 은 구독으로 받고,
CLI·외부 에이전트에게는 일반 구독 수단이 없다. 기다릴 수 있는 것은 메서드마다 따로 만든
장수 호출 다섯(`agent.task_await` · `agent.barrier_await` · `approval.await` · `pty.wait` ·
`surface.send_wait_idle`)뿐이다.

그래서 에이전트는 `list tree` 류를 **폴링**하거나, 훅 → 셸 명령 → 알림 로그 → `tail -F`
라는 우회로를 쓴다. 그 폴링의 비용은 이미 장부에 찍혀 있다 — allow audit 이 초당 14 건 ·
18 시간 371,936 행으로 `memory.db` 최대 유입원이 된 원인이 폴링형 에이전트 워크로드였고
([ADR-0085](0085-ipc-log-retention-bounded.md)), 그때의 처방은 **기록을 끈 것**이었다.
폴링 자체는 남았다.

읽는 표면이 하나 생기므로 **불가침 원칙 1**(사용자 행동 ↔ 에이전트 행동 분리)의 경계를
여기서 함께 박아야 한다. 사건 기록은 "무슨 일이 있었나" 를 한 자리에 모으는 구조라,
그 자리가 사용자 상태의 복원 경로나 사용자 입력의 저장소로 번지기 쉽다.

## Decision

**`events.fetch {offset, max, filter, wait_ms}` 하나를 낸다. 커서는 소비자가 든다.**

- **서버는 소비자별 상태를 안 든다.** 소비자가 위치를 들고 매 호출에 가져온다. 그래서
  소비자가 사라져도 서버에 남는 것이 없고, 느린 소비자가 호스트 쪽에 아무것도 쌓지
  않는다. 이 모양의 선례가 `plugin.audit_follow` 다.
- **`MethodEffect` 는 `Read` 다.** [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md)
  의 축은 "두 번 전달되면 관측 가능한 차이가 남는가" 이고, 이 호출은 **서버 쪽 커서를
  전진시키지 않으므로** 같은 인자로 두 번 부르면 같은 답이 온다. 같은 물음에
  `surface.read_since_scan_mark` 가 `Mutate` 로 답한 것과 갈리는 자리가 정확히 그
  차이다([ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md)) —
  그쪽은 읽으면서 마크가 움직인다.
- **`local_only` 다.** 권한 게이트가 아니라 **모양** 때문이다. `wait_ms` 가 스레드를
  잡으므로 plugin SDK 의 단일 워커가 막힌다 — `agent.task_await` 와 `approval.await` 가
  같은 이유로 local 이다. plugin 에게는 이미 구독이 있다.
- **그래서 부를 수 있는 것은 로컬 호출자뿐이다.** `local_only` 는 plugin 만이 아니라
  **세션 토큰을 제시한 `Agent` 호출자도 거절한다**(`caller.rs` 의
  `agent_cannot_call_local_only_method` 가 그 갈래를 고정한다). 원격 에이전트가 이
  피드를 쓰는 길은 로컬 소켓의 CLI 를 경유하는 것이고, 그것은 이 결정의 판단 기준
  ("SSH 로 이미 가능한가")과 같은 자리다 — 토큰을 늘려서 여는 것이 아니다.
- **권한 게이트는 면제다.** CLI caller 는 Local 이고 판단 기준은 "SSH 로 이미 가능한가"
  다([ADR-0277](0277-ipc-admission-and-observation-run-once.md) 이 개정하지 않고 남긴 "Local 예외" 와 같은 방향). plugin
  쪽 매니페스트 `event_subscribe` 게이트는 그대로다.
- **필터는 구독과 같은 문법이다.** `pattern_matches()` 를 그대로 쓴다 — 정확 일치와
  `<ns>.*` 와일드카드. 조회용 문법을 따로 만들면 같은 물음에 답이 둘이 된다.
- **대기는 조건 변수로 한다.** 폴링 간격으로 깎으면 응답 지연의 하한이 그 간격이 된다.
  필터에 안 걸리는 사건으로 깨면 **남은 시간만큼 계속 기다린다** — 안 그러면 관심 없는
  사건이 잦을수록 빈 답이 늘어난다.
- **CLI 는 `fetch` 와 그 루프(`follow`) 둘을 낸다.** `follow` 는 한 줄에 한 사건씩 JSON 을
  stdout 에 찍어 셸의 `while read` 가 먹는다. 건너뛴 수와 세대 교체 통지는 **stderr** 로
  낸다 — stdout 에 섞으면 그 루프가 사건이 아닌 줄을 파싱하게 된다.
- **장수 호출 다섯을 이 결정으로 걷어내지 않는다.** 대체는 별도 결정이다.

### 불가침 원칙 1 의 경계 셋

이 표면이 넘지 않는 선을 값으로 박는다. [`docs/identity.md`](../identity.md) 의 원칙 1 이
상위다.

1. **위치로 읽는 것은 조회다.** 포커스 · 선택 · 스크롤 · 커서 — 어떤 사용자 상태도 이
   호출로 움직이지 않는다. `Read` 라는 분류가 그 사실의 한 자리다.
2. **기록에서 상태를 복원하는 경로를 만들지 않는다.** 스크롤 위치를 offset 으로 쓰거나,
   피드를 되감아 레이아웃을 되살리는 길을 내지 않는다. 그 순간 사건 기록이 사용자 상태의
   두 번째 원천이 되고, 에이전트의 조회가 사용자 상태를 되돌리는 손잡이가 된다.
3. **피드에 사용자 입력 자체를 싣지 않는다.** 키 · 마우스는 payload 에 안 들어간다.
   싣는 순간 원칙 1 이 `#[cfg(debug_assertions)]` 로 격리해 둔 것(입력 주입)과 같은
   범주가 된다 — 입력을 기록하는 쪽과 재생하는 쪽은 한 걸음 차이다.

## Consequences

- **얻은 것**: 외부 에이전트가 CLI 를 경유해 폴링 없이 사건을 받는다. 끊겼다 붙어도
  위치로 이어진다.
- **얻은 것**: 장수 호출을 메서드마다 새로 만드는 압력이 줄었다. 새 사건은 키 하나를
  더하는 일이고 새 blocking 메서드를 만드는 일이 아니다.
- **잃은 것**: 지연이 pull 의 왕복만큼 있다. 지연이 중요한 소비자에게는 push 가 맞고,
  그것은 `stream.rs` 의 프레임 프로토콜 위에 나중에 얹는다.
- **잃은 것**: 대기 중인 `fetch` 하나가 스레드 하나를 잡는다. `local_only` 인 이유가
  그것이고, 상한(60초)이 그 스레드가 영영 안 돌아오는 것을 막는다.
- **운영 비용**: 소비자가 위치를 들므로, 위치를 안 들고 매번 0 부터 읽는 잘못 쓴
  소비자는 같은 사건을 되풀이해 받는다. 그것은 서버에 아무것도 쌓지 않지만 그 소비자의
  중복은 서버가 못 막는다.

## Alternatives Considered

- **A: push(`events.subscribe` + `StreamTag` 프레임)** — 지연이 낮지만 느린 소비자에
  대한 정책(버리나 미나)이 필요하고, 끊긴 사이를 메우려면 결국 위치가 필요하다.
  에이전트의 사건 소비는 완전성이 중요한 쪽이라 pull 이 먼저다.
- **B: 서버가 소비자별 커서를 든다** — 소비자가 사라져도 상태가 남고, 누가 살아 있는지
  서버가 판정해야 한다. `plugin.audit_follow` 가 이미 반대 모양으로 옳게 푼다.
- **C: `MethodEffect::Idempotent`** — "읽기는 아니다" 를 말하려는 유혹이 있지만 축이
  다르다. 이 호출은 서버 상태를 아예 안 건드린다.
- **D: plugin 에게도 연다** — SDK 의 단일 워커가 `wait_ms` 동안 막힌다. plugin 에게는
  구독이 이미 있어 얻는 것도 없다.
- **E: 폴링 간격으로 대기 구현** — 응답 지연의 하한이 간격이 되고, 간격을 줄이면 이
  결정이 없애려던 폴링이 서버 안으로 들어온다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- push 형이 결정된다. 그러면 A 로 가고, 이 조회 계약은 남는다 — 둘은 배타가 아니다.
- 장수 호출 다섯 중 하나를 걷어내는 결정이 선다. 그 판정은 이 표면이 그것을 대체할 수
  있는지를 묻고, 그때 `local_only` 의 범위가 함께 움직인다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 소비자가 위치를 옳게 드는지. **레포가 못 잰다** — 소비자는 레포 밖이다. 재는 법:
  같은 caller 가 계속 `offset: 0` 으로 묻는지 서버 쪽에서 세는 것인데, 그러면 서버가
  소비자별 상태를 드는 것이라 이 결정과 부딪힌다. 그래서 안 잰다.
- 대기 중 스레드 수. 재는 법: `wait_ms` 를 준 동시 `fetch` 의 수를 세야 하는데 그 좌변이
  지금 없다. 상한 60초가 그것이 무한히 쌓이는 것만 막는다.

## References

- [ADR-0322](0322-the-event-ring-keeps-positions-and-says-what-it-dropped.md) — 이 호출이 읽는 링
- [ADR-0321](0321-agent-domain-events-publish-only-at-the-funnel-that-already-exists.md) — 이 피드의 첫 사건들
- [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md) · [ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md) — `MethodEffect` 의 축
- [`docs/identity.md`](../identity.md) — 불가침 원칙 1
- [`docs/reference/api.md`](../reference/api.md) — 표면 기술
- 코드 근거(결정이 실현된 현재 위치): `events.fetch` 핸들러 · `METHOD_TABLE` 의
  `("events.fetch", local_only(Read))` · `EventBus::fetch_blocking` · `tasty events follow`
