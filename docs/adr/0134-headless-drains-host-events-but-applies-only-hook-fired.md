# ADR-0134: headless 는 host event 큐를 비우되 비-bus 소비자가 있는 종류만 적용한다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: headless, host-event, plugin-event-bus, agent-runner, hooks, queue, agent-surface
- **Group**: architecture

## Context

[ADR-0111](0111-headless-drains-the-intent-queue.md) 이 Intent 큐에서 닫은 것과 같은
구멍이 **host event 큐**(`AppState::pending_host_events`)에 하나 더 있었다. 넣는 쪽은
headless 에서 실행되는데 — `src/boot.rs` 의 idle-timeout 훅 발화,
`src/adapters/ipc/handler/hooks.rs` 의 `surface.fire_hook` — 빼 가는 유일한 프로덕션
소비자 `App::dispatch_pending_host_events`(`src/app/dispatch/host_events.rs`)가
`#[cfg(feature = "gui")]` 이라 headless 에는 없었다.

큐 적재보다 나쁜 것이 기능 쪽이었다. 그 drain 의 소비자는 세 갈래다.

1. **plugin event bus** — 모든 종류를 `emit_host_event` 로 내보낸다.
2. **`resolve_hook_fired_task_waits`** — `HookFired` 만 받는다. push 완료 전략
   (`core::agent::runner_host::dispatch_push_strategy`)이 대상 surface 에 1회성 훅을
   걸고 그 `hook_id` 를 `hook_task_waits` 에 등록한 뒤 task 를 `AwaitExternal` 로
   두는데, 그 task 를 마감하는 것이 이 소비자다.
3. **OSC 제목 재투영** — `SurfaceFocused` 만 받는다. headless 에는 그 종류의 발화점이
   없다.

즉 headless 에서는 훅이 발화해도 그것을 기다리던 agent task 가 깨어나지 않았다. 무응답도
아니었다 — deadline 이 지나면 `runner_thread::expire_overdue_hook_waits` 가 그 task 를
Failed 로 마감한다. **훅이 성공을 알렸는데 결과가 실패로 뒤집힌다.** `hook.set` /
`surface.fire_hook` 은 IPC·CLI 양면으로 노출된 에이전트 표면이고, `surface.fire_hook`
핸들러의 주석은 이 조합(`--event command-completed:1` 로 push 전략 대기 task 를
시뮬레이션)을 명시적 용법으로 적고 있었다.

## Decision

headless 전용 drain `intent::headless::drain_pending_host_events` 를 두고, Intent 큐
drain 과 같은 세 진입점(`src/boot.rs` · `src/boot/headless_dispatch.rs` ·
`src/boot/headless_plugins.rs`)에 배선한다. 이 drain 은 큐를 **통째로 비우되**
`HookFired` 만 `Core::resolve_hook_task_wait` 로 적용하고 나머지 종류는 버린다. 즉
소비자가 plugin event bus 뿐인 종류는 headless 에서 발화하지 않는다.

## Consequences

- **얻은 것**: push 완료 전략으로 대기하던 agent task 가 headless 에서도 훅 발화로
  정상 마감된다(exit code 에 따라 Succeeded / Failed). 큐 무한 적재도 함께 닫힌다 —
  비-`HookFired` 종류도 큐에서 제거되기 때문이다.
- **잃은 것**: headless 에서 plugin event bus 로 나가는 host event 가 없다. 다만 이
  큐는 headless 에서 애초에 아무도 비우지 않아 bus 로 나간 적이 **없으므로** 이 결정으로
  새로 잃는 것은 없다. 번들 plugin 중 이 이벤트들을 구독하는 것은 0건이다(구독 선언은
  `tasty-plugin.toml` 의 `event_subscribe` 하나뿐이고 `theme.changed` 를 본다).
  서드파티 plugin 은 구독할 수 있으므로 이 0 은 **번들 범위의 0** 이다.
- **운영 비용 / 유지 부담**: 배선 가드가 세 진입점의 소스를 `include_str!` 로 읽어 호출
  존재를 확인한다 — 호출부가 빠지는 회귀(가장 그럴듯한 형태)를 잡기 위함이며, 진입점이
  늘면 그 목록도 함께 늘려야 한다.

### 자동 채널

이 결정을 고정하는 테스트는 `src/intent/headless.rs` 의 단위 테스트 세 개다 — 훅
발화가 대기 task 를 마감하는가, host event 큐가 유계인가, 세 진입점이 drain 을
호출하는가. 셋 다 `--lib --bins` 조합에 들어 있어 **기본 조합 CI 잡이 본다**.

그 전에는 **이 경로를 덮는 채널이 하나도 없었다.** 통합 테스트 0건이었고
(`tests/` 에 agent task 를 다루는 파일이 없다), 단위 테스트는
`core::agent::task.rs` 의 `hook_wait_tests` 가 `resolve_hook_task_wait` 를 **직접
호출**해 검증할 뿐 "훅 발화가 그 함수에 도달하는가" 는 아무도 안 봤다. 그 간극이
정확히 이 결함이 살던 자리다.

**데몬 e2e 는 여전히 없다.** 실제 `dispatch_push_strategy` 를 태우려면 완료 전략을
선언하는 plugin 이 필요한데 레포에 테스트용 plugin 이 없다 — 여기서는 등록을
`hook_task_waits.register` 로 직접 흉내 내고 발화만 실제 IPC 경로로 냈다. 따라서
"dispatch 가 등록하는 hook_id 와 발화가 싣는 hook_id 가 같다" 는 축은 이 채널이
보지 않는다. 그 축을 덮으려면 테스트 plugin 하네스를 새로 만들어야 하고, 그것은 이
결정의 범위 밖이다.

## Alternatives Considered

- **A: `App::dispatch_pending_host_events` 를 headless 로 끌어온다** — 안 골랐다. 그
  함수는 `self.view.views` 순회, lua autofire 컨텍스트, `PluginManager` 대여를 전제로
  하고 그 소비처가 headless 에 없다. 소비자 없는 배선을 위해 gui 계층을 headless 로
  끌어오는 비용이 얻는 것보다 크다.
- **B: `HookFired` 만 enqueue 하지 않도록 발화 쪽을 막는다** — 안 골랐다. 발화 지점은
  이미 두 곳(idle-timeout · `surface.fire_hook`)이고 늘어난다. 막는 쪽으로 가면 기능이
  죽은 채로 남고 큐 적재만 사라진다 — 문제의 절반만 푼다.
- **C: 큐 길이를 계측/상한으로 관리한다** — 안 골랐다. 적재는 증상이고 원인은 소비자
  부재다. 원인을 닫으면 증상이 따라 닫힌다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 번들 plugin 이 host event(`hook.fired` / `notification.created` / surface·tab·pane·
  workspace 계열)를 구독하기 시작한다 — 그러면 "버려도 잃는 것이 없다" 가 깨진다.
  **판정 방법**: 번들 매니페스트의 구독 선언을 훑는다 —
  `git grep -n event_subscribe -- 'crates/**/tasty-plugin.toml'`. 이 ADR 작성
  시점의 값은 `tasty-plugin-markdown` 한 줄(`["theme.changed"]`)뿐이고, 그 한 줄이
  스캔이 실제로 무언가를 잡는다는 양성 대조 역할도 한다 — 결과가 0건으로 바뀌면
  스캔이 고장난 것이지 구독이 사라진 것이 아니다.
- headless 에서 서드파티 plugin 의 host event 구독을 지원 대상으로 선언한다.
- `HookFired` 외의 종류에 비-bus 소비자가 생긴다(예: engine 상태를 갱신하는 cascade).
- headless 에 `SurfaceFocused` 발화점이 생긴다 — OSC 제목 재투영이 소비자로 살아난다.

## References

- [ADR-0111](0111-headless-drains-the-intent-queue.md) — 같은 형태를 Intent 큐에서 닫은 결정
- [action-dispatch](../design/flows/action-dispatch.md) — gui / headless dispatch 흐름
- [agent-runner](../dev-guide/agent-runner.md) — push 완료 전략과 task DAG
