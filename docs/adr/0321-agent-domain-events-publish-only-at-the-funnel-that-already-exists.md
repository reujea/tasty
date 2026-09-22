# ADR-0321: agent 사건은 **이미 있는 깔때기**에서만 발화한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: events, event-bus, agent, task, barrier, plugin-protocol, catalog
- **Group**: event-feed

## Context

협업 primitive 6 종(task DAG · barrier · semaphore · lease · reducer · rate-limit)의 상태
변화가 Event Bus 에 **한 건도** 안 실렸다. 실측:

- 사건의 예약 네임스페이스는 **19 개**였고 그 목록에 `agent` 가 없었다
  (`tasty_plugin_manifest` 의 `is_reserved_event_namespace`).
- 같은 이름이 **IPC 메서드 prefix 예약 목록**(`RESERVED_IPC_PREFIXES`)에는 처음부터
  있었다. 두 목록이 어긋나 있던 것이다.

그래서 에이전트가 제일 구독하고 싶어 하는 사실 — "그 task 가 끝났다" — 을 받을 길이
없었고, 그 자리를 훅에서 셸 명령을 거쳐 로그 파일을 tail 하는 우회로가 메우고 있었다.
그 로그에는 커서가 없어 **붙기 전에 지나간 줄을 못 읽고 그 사실도 알 수 없다.**

## Decision

**`agent` 를 예약 사건 네임스페이스에 더하고, 종결 전이만 발화한다.** 발화점은 새로
심지 않고 **이미 있는 단일 깔때기**를 쓴다.

- **`agent.task_finished`** — task 가 종결 상태(`succeeded`/`failed`/`cancelled`/`skipped`)에
  들어간 직후. 발화 자리는 `TaskWakerHub::fire` 안이다. 그 함수는 `agent.task_await` 가
  깨어나는 자리이고, 종결 전이의 **세 진입 경로**(`Core` wrapper · 러너 스레드의 set_state
  클로저 · hook wait timeout)가 전부 그것을 지난다. 여기서 같이 내보내면 **피드가 덮는
  범위가 곧 `task_await` 가 덮는 범위**가 된다 — 어긋나면 `task_await` 가 먼저 멈추고,
  그쪽 실패는 훨씬 시끄럽다.
- **`agent.barrier_closed`** — barrier 가 요구 수를 채워 닫힌 직후. `BarrierState::Closed`
  를 쓰는 자리가 `BarrierStore::signal` **하나**이고 그 함수의 호출자도 하나다.
- **비종결 전이는 안 싣는다.** `waiting`/`ready`/`running` 으로 들어가는 전이에는 위와
  같은 깔때기가 없다. 발화점을 손으로 심으면 어느 경로 하나가 빠지고, **소비자는 자기가
  무엇을 못 받았는지 알 수 없다** — 조용한 구멍이다.
- **lease 만료는 사건이 아니다.** 만료는 전이가 아니라 **읽을 때 평가되는 술어**다
  (`Lease::is_expired` 를 `list` 가 `now_ms` 와 견주고 그때 evict 한다). 이것을 사건으로
  내면 발화 시점이 "누가 언제 조회했나" 에 달린다. 그런 계약은 소비자가 쓸 수 없다.
- **payload 에 실패 사유·결과·명령 출력을 안 싣는다.** 그 문자열은 task 가 돌린 명령의
  출력을 그대로 담을 수 있고 피드는 구독 권한만 있으면 받는다. 필요한 소비자는 `task_id`
  로 `agent.task_get` 을 부른다 — 그쪽에는 호출자 권한이 걸린다.
- **대상 workspace 는 `meta.scope` 가 아니라 payload 의 `workspace_id` 로 낸다.**
  envelope 의 scope 축은 `system`/`surface` 둘뿐인데 task 는 workspace 에 매인다. 새
  변형을 더하지 않은 이유는 **`workspace.*` 계열이 이미 같은 방식**이기 때문이다
  (`workspace.created` 등이 scope=system + payload 의 `workspace_id`). 새 변형을 더하면
  wire enum 이 바뀌어 번들 plugin 전부가 따라 움직이는데, 그렇게 해도 기존 workspace
  사건들은 그대로 `system` 이라 축이 두 가지로 갈린다.
- **등급은 Experimental 로 시작한다.** 카탈로그는 plugin 이 의존하는 공개 API 라
  Stable 로 내면 키·필수 필드가 major 까지 묶인다. 이 두 키는 아직 소비자가 없다.

발화 자체는 **큐를 한 번 거친다.** 종결 전이는 러너 스레드에서도 일어나는데 Event Bus 는
`PluginManager` 가 소유해 메인 스레드에서 fan-out 되기 때문이다. 발화점은 사실만 적고
프레임 루프가 꺼내 내보낸다 — `memory.changed` 가 이미 같은 모양이다.

## Consequences

- **얻은 것**: 구독 plugin 이 `agent.*` 로 task 종결을 받는다. 두 목록(IPC prefix ·
  사건 네임스페이스)이 같은 이름을 들게 됐다.
- **얻은 것**: 발화 범위가 **파생**이라 따로 유지할 명부가 없다. 종결 진입 경로가
  하나 더 생기면 그것도 깔때기를 지나야 `task_await` 가 동작하므로, 사건도 자동으로
  따라온다.
- **잃은 것**: 비종결 전이를 보고 싶은 소비자는 아직 폴링해야 한다. 그 자리를 채우려면
  먼저 비종결 쪽에도 깔때기를 만들어야 하고, 그것은 이 결정의 범위 밖이다.
- **잃은 것**: payload 가 얇아서 무엇이 왜 실패했는지는 두 번째 호출로 물어야 한다.
  나중에 실어야 하면 **옵션 필드 추가**라 기존 소비자를 안 깨뜨린다(반대 방향은 major 다).
- **운영 비용 / 유지 부담**: 예약 네임스페이스가 하나 늘어 plugin 이 `agent.*` 를 자기
  것으로 **발화** 선언할 수 없다. 구독은 그대로 열려 있다. 번들 plugin 아홉의 매니페스트를
  실측해 그 이름을 쓰는 것은 **0 건**이었다.
- **운영 비용**: wire payload 타입이 `tasty-plugin-protocol` 에 들어가므로 번들 plugin
  아홉의 버전이 함께 움직인다.

## Alternatives Considered

- **A: 8 상태 전부 발화** — 되돌릴 수 없는 쪽으로 크게 여는 선택이다. 카탈로그가 공개
  API 라 키를 뺐다가 다시 넣을 수 없고, 비종결 쪽은 깔때기가 없어 **처음부터 구멍 난 채**
  발행된다. 구멍 난 피드는 없는 피드보다 나쁘다 — 소비자가 그것을 믿는다.
- **B: 발화점을 각 전이 자리에 직접 심는다** — 지금 종결 진입 경로가 셋이고 앞으로
  늘어난다. 심은 자리와 실제 경로가 어긋나는 것이 기본 동작이고, 어긋나도 아무것도
  빨개지지 않는다. 깔때기에 얹으면 그 어긋남이 `task_await` 의 실패로 먼저 드러난다.
- **C: `EventScope` 에 `Workspace` 를 더한다** — wire enum 변경이라 번들 plugin 전부가
  따라 움직이는데, 기존 `workspace.*` 사건들은 그대로 `system` 이라 같은 뜻을 두 가지
  방식으로 내는 상태가 남는다. 그것을 없애려면 기존 사건의 scope 를 바꿔야 하고 그건
  Stable 키의 계약 변경이다.
- **D: lease 만료도 함께 발화** — 티켓의 완결 조건에 있었으나 **실측이 그 전제를
  뒤집었다.** 만료 전이가 일어나는 자리가 코드에 없다. 발화하려면 시계를 도는 sweeper 를
  새로 만들어야 하고, 그것은 사건 하나가 아니라 새 동작이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 비종결 전이에도 단일 깔때기가 생긴다(`TaskWakerHub::fire` 처럼 모든 진입 경로가
  지나는 자리). 그러면 A 의 반대 논거가 사라진다.
- lease 만료가 술어가 아니라 전이가 된다 — `Lease::is_expired` 를 읽는 자리가 아니라
  상태를 쓰는 자리가 생긴다. 그러면 D 가 가능해진다.
- `EventScope` 가 이미 다른 이유로 바뀐다. 그러면 C 의 비용이 이 결정에만 걸리지 않는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 이 두 키를 실제로 구독하는 plugin·에이전트가 생기고 payload 가 얇아 두 번째 호출을
  강제한다는 보고가 쌓인다. 재는 법: 소비자에게 `agent.task_get` 을 몇 번 부르는지
  묻는다 — 레포 안에는 그 호출이 피드 때문인지 아닌지를 가를 좌변이 없다.
- Experimental → Stable 승격 판단. 재는 법: 이 키로 분기하는 외부 소비자가 있는지는
  이 레포가 알 수 없다. 사람이 확인한다.

## References

- [`docs/reference/event-catalog.md`](../reference/event-catalog.md) — 사건 카탈로그(공개 API 의 SoT)
- [`docs/features/agent-collaboration/index.md`](../features/agent-collaboration/index.md) — 협업 primitive
- [`docs/dev-guide/agent-runner.md`](../dev-guide/agent-runner.md) — task DAG executor
- 코드 근거(결정이 실현된 현재 위치): `TaskWakerHub::fire` · `Core::barrier_signal` ·
  `AgentEventQueue` · `App::dispatch_pending_agent_events` ·
  `tasty_plugin_protocol::events::payloads::{AgentTaskFinished, AgentBarrierClosed}`
