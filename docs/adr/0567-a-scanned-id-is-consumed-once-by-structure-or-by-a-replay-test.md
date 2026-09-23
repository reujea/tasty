# ADR-0567: 훑어 얻은 id 의 한 번 소비는 구조나 재전송 시험으로 지킨다 — 자리를 세는 명부 가드는 두지 않는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: testing, mutation-testing, exactly-once, host-plugin, hooks, guards, adr-0311, adr-0243
- **Group**: guard-design

## Context

컬렉션을 훑어 id 를 모으고, 그 id 마다 한 번만 일어나야 하는 효과(caller 회신 · 계수 지우기 ·
재시작 · 훅 실행)를 내는 자리가 여럿 있다. 이 효과가 **0 번**이면 누락 소비, **두 번**이면
중복 소비다. 두 번째는 대개 조용하다 — 원본에서 id 가 안 빠지면 다음 훑기나 같은 id 의
재전송이 그것을 또 소비한다.

이런 자리는 성질로 둘로 갈린다.

- **구조로 보장되는 자리** — 효과가 같은 `&mut` 호출 안의 `remove` · `take` 가 `Some` 을 돌려줄
  때만 난다(`sweep_expired_requests` · `cancel_pending_namespace_calls` · `HookTaskWaits::sweep_expired`
  등). 제거를 빼면 효과를 낼 값도 없어지므로 "제거는 빠지고 효과는 남는" 형태가 한 줄 변이로
  안 만들어진다.
- **따로 선 한 줄에 기대는 자리** — 제거가 효과와 떨어져 있다. 그 줄 하나가 빠져도 컴파일되고
  효과는 계속 난다.

뒤쪽 가운데 셋이 시험 없이 서 있었다(2026-09-23, 변이로 확인).

1. `tasty-host-plugin` `PluginManager::clear_streak_if_late_namespace_answer` — 만료로 거둔 id 를
   기억해 두고, 그 id 의 늦은 응답이 오면 id 를 빼고 연속 만료 계수를 지운다(ADR-0311 의 결정).
   id 를 빼는 줄을 지워도 `tasty-host-plugin` lib 시험이 전부 초록이었다 — 같은 id 를 두 번
   보내는 시험이 없었다. 그 줄이 빠지면 plugin 이 옛 응답 한 줄을 계속 재전송해 계수를 영구히
   0 으로 눌러 둘 수 있다.
2. 같은 크레이트의 `cancel_pending_namespace_calls` 첫머리 — 재시작이 그 plugin 의 계수와 거둔
   id 목록을 함께 비운다(ADR-0311 "그 경로가 두 자료를 함께 비운다"). 계수를 비우는 줄을 지워도
   초록이었다 — 재시작 **뒤에** 새 프로세스를 두고 판정을 다시 돌리는 시험이 없었다. 그 줄이
   빠지면 새로 뜬 프로세스가 ping tick 마다 옛 계수로 다시 재시작된다.
3. `tasty-hooks` `HookManager::check_and_fire` — once 훅의 제거가 훑기가 **끝난 뒤** 일어난다.
   그래서 한 호출의 `events` 에 그 훅과 맞는 사건이 여럿이면 같은 once 훅이 사건 수만큼
   돌려졌다. 지금 호출자는 전부 사건 하나만 넘기므로 도달하지 않는 결함이었고, 그래서 시험도
   없었다.

## Decision

**구조로 보장되는 자리는 그대로 두고, 따로 선 한 줄에 기대는 자리는 그 자리마다 행동 시험
하나로 고정한다. 시험은 같은 id 를 한 번 더 들이밀거나(재전송) 소비자를 한 번 더 돌리고,
두 번째에 효과가 없다는 것과 첫 번째에 효과가 있었다는 것을 함께 단언한다.** 앞의 단언이 중복
소비를, 뒤의 단언이 누락 소비를 잡는다. 이 결정의 채널은 아래 세 시험이다.

- `tasty-host-plugin` `a_reaped_namespace_id_clears_the_streak_exactly_once` — 첫 늦은 응답이 계수를
  지우고, 같은 id 의 재전송은 안 지우며, 한 id 의 소비가 같은 묶음의 다른 id 를 지우지 않는다.
  목록 길이 상한이 id 를 밀어내면 소비가 아니라 자름이 id 를 지운 것이 되어 시험이 소비를 못
  재므로, 한 번에 상한보다 적게 쌓는다.
- `tasty-host-plugin` `a_plugin_restarted_by_the_expiry_streak_is_restarted_once` — 재시작 뒤 새
  프로세스를 두고 판정을 한 번 더 돌려, 새 프로세스가 남는 것과 옛 거둔 id 가 없는 것을 단언한다.
- `tasty-hooks` `a_once_hook_fires_once_even_when_several_events_match` — 맞는 사건 둘에 once 훅이
  한 번, 지속 훅이 두 번 돌려지고, 다음 호출에는 once 훅이 없다.

**once 훅은 한 번의 `check_and_fire` 안에서도 첫 맞는 사건에서 멈춘다.** 지속 훅은 맞는
사건마다 발사한다. once 의 약속은 "한 번 실행 후 자동 삭제" 이고, 제거 시점이 훑기 뒤라는
구현 사정이 그 약속을 사건 수만큼 늘려서는 안 된다.

**자리를 세는 명부(소스 가드)는 두지 않는다.** 새 자리는 그 자리를 만드는 사람이 위 형태의
시험을 함께 쓴다.

**이 결정이 다루지 않는 것**: 반복해도 해가 없는 멱등 소비(예: `purge_stale_semaphore_holders` 의
`release` — 두 번 불러도 결과가 같고, `Failed` 전이가 다음 훑기에서 그 task 를 빼낸다)는 "정확히
한 번" 의 대상이 아니다.

## Consequences

- **얻은 것**: 위 세 자리에서 소비 줄을 지우는 변이, id 하나 대신 목록을 통째로 비우는 과소비
  변이, 소비를 부르는 호출을 막는 변이, once 훅의 멈춤을 지우는 변이, 제거 `retain` 을 지우는
  변이가 이제 각각 시험 하나 이상에 죽는다(변이마다 해당 크레이트 lib 시험을 전량으로 돌려 잼).
- **잃은 것**: 명부가 없으므로 새 자리가 시험 없이 들어오는 것을 아무 채널도 못 본다.
- **운영 비용 / 유지 부담**: once 훅이 한 호출에서 첫 사건만 돌려주는 동작은 지금 호출자에게는
  관측 차이가 없다(모두 사건 하나를 넘긴다). 여러 사건을 한 번에 넘기는 호출자가 생기면 once 훅이
  받는 `received` 는 그 가운데 첫 맞는 사건이다.

## Alternatives Considered

- **A: "훑고 소비하는" 자리를 세는 소스 가드(명부)** — 기각. "훑어서 모은 id 를 소비한다" 는
  문법으로 안 갈린다. 같은 `iter().filter().map().collect()` 모양이 화면 갱신 목록도 만들고,
  효과가 호출자에게 가는 자리(id 목록을 돌려주는 함수)는 그 파일만 읽어서는 소비인지 안 보인다.
  판정기가 성질을 못 재면 명부는 손으로 채워지고, 손으로 채운 명부가 잡는 것은 이름의 추가뿐이다
  ([ADR-0243](0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md) 의
  첫 물음 — 판정할 성질이 기계로 읽히는가 — 에서 멈춘다).
- **B: 따로 선 줄을 전부 구조형(효과를 `remove` 의 반환값에 묶기)으로 고쳐 시험 없이 둔다** —
  안 고른다. 효과가 호출자에게 넘어가는 자리(id 목록을 돌려주는 sweep)는 함수 안에서 효과를
  묶을 수 없고, 묶을 수 있는 자리도 "제거와 게이트가 한 줄이 된" 형태일 뿐이라 그 줄을 다른
  모양으로 바꾸는 변이에는 여전히 행동 시험이 있어야 잡힌다. 고치는 폭에 비해 얻는 것이 없다.
- **C: once 훅도 맞는 사건마다 돌려주고 끝에 지운다** — 기각. once 의 사용자 약속("한 번 실행 후
  자동 삭제")과 어긋나고, 완료 알림처럼 once 훅에 한 번의 부수효과를 거는 쓰임이 중복 발사를
  받는다.
- **D: 여러 사건을 한 번에 넘기는 호출을 막는다** — 기각. 지속 훅은 맞는 사건마다 발사하는 것이
  옳고, `check_and_fire` 가 슬라이스를 받는 계약은 그대로 쓸모가 있다. 문제는 once 훅 하나다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `clear_streak_if_late_namespace_answer` 가 늦은 응답의 id 를 목록에서 더 이상 빼지 않게 됐을
  때, 또는 재시작 경로가 계수·거둔 id 목록을 더 이상 비우지 않게 됐을 때. 좌변에 붙은 판정기는
  위 `tasty-host-plugin` 의 두 시험이다.
- `check_and_fire` 가 once 훅을 한 호출에서 둘 이상 돌려주게 됐을 때. 좌변에 붙은 판정기는
  `tasty-hooks` 의 `a_once_hook_fires_once_even_when_several_events_match` 다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 따로 선 한 줄에 기대는 새 소비 자리가 시험 없이 들어왔을 때. 재는 법: 컬렉션을 훑어 id 를
  모으는 자리(`let ids: Vec<_> = <컬렉션>.iter()…collect()` · `.position(` 뒤의 `.remove(pos)` ·
  `retain` 이 제거하는 자리)를 `git grep` 으로 모으고, 효과가 제거의 반환값에 묶이지 않은
  자리마다 제거 줄을 지운 변이로 그 크레이트 lib 시험을 전량으로 돌린다. 초록이면 이 결정이
  지켜지지 않은 자리다.
- 명부 가드가 성질을 잴 수 있게 됐을 때(예: 소비를 한 타입으로 모아 그 타입만 효과를 낼 수 있게
  바뀐 경우). 재는 법: 그 타입 밖에서 같은 효과를 내는 호출이 남아 있는지 센다.

## References

- 선행 결정: [ADR-0311](0311-a-namespace-call-expires-into-an-error-not-a-fail-open.md) (늦은 응답은
  기억한 id 일 때만 계수를 지우고, 재시작은 두 자료를 함께 비운다 — 그 결정은 그대로이고 이 ADR 은
  그 두 조항의 한 번 소비에 채널을 붙인다)
- 선행 결정: [ADR-0243](0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md)
  (판정기를 안 짓는 이유의 순서 — 대안 A 의 기각 근거)
- 선행 결정: [ADR-0183](0183-a-green-check-is-not-evidence-without-a-control.md) (초록은 대조가 있어야
  측정이다 — 세 시험을 변이로 확인한 근거)
- 탐색: `git grep -l 'check_and_fire\|expired_namespace_calls\|namespace_expiries' -- docs/adr/` ·
  `git grep -il 'exactly once\|정확히 한 번' -- docs/adr/`
- 코드 근거(결정이 실현된 현재 위치): `tasty-host-plugin` 의
  `PluginManager::clear_streak_if_late_namespace_answer` · `PluginManager::cancel_pending_namespace_calls` ·
  `PluginManager::restart_unresponsive_plugins`, `tasty-hooks` 의 `HookManager::check_and_fire`.
- 운영 문서: [features/hooks](../features/hooks/index.md) (once 훅 시맨틱)
