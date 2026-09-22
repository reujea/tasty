# ADR-0251: 폴백으로 가는 dispatch 메서드는 술어가 아니라 사유로 판정한다

- **Status**: Accepted
- **Date**: 2026-09-09
- **Tags**: ipc, routing, focus, guards, roster, multi-window, adr-0133, adr-0175
- **Group**: window-workspace-lifecycle

## Context

`src/app/ipc/routing.rs` 는 요청의 주인 창을 못 찾으면 **포커스된 창**으로 보낸다.
[`docs/design/policies/focus.md`](../design/policies/focus.md) 는 그 폴백이 답이 되는
순간 그 메서드가 포커스 의존이 된다고 적는다. 증상은 에러가 아니라 "다른 창에서
not found" 라 원인이 라우팅이라는 것이 드러나지 않는다.

그런데 **어느 메서드가 그 상태인지**를 값으로 든 자리가 없었다. 기존 두 판정기는
물음이 다르다 — `routing_key_coverage` 는 "핸들러가 읽는 id 키가 라우팅에 나오는가"
를 키 단위로, `routing_key_method_scope` 는 같은 명제를 (메서드, 키) 쌍으로 묻는다.
둘 다 **키를 읽는 메서드**만 본다. 키를 하나도 안 읽는 메서드 — 즉 폴백으로 가는
바로 그 집합 — 은 두 가드의 모수 밖이다. 실측(2026-09-09) 그 수가 dispatch arm
261 중 **113** 이다.

이 집합을 자동 술어로 좁히려면 "대상처럼 생긴 키" 를 이름이 아니라 성질로 정의해야
하는데 그 술어가 아직 없다. 이름 모양(`_id` 로 끝나는가)으로 좁히면 `surface` ·
`parent` · `target` · `pane` 을 놓쳐 `terminal.*` 일곱이 지목 없음으로 잡히고
(실측), 반대로 시간 창을 뜻하는 `window` 같은 이름이 대상으로 잡힌다.

## Decision

**자동 술어를 기다리지 않고, 손으로 유지하는 명부에 갈래와 사유를 적는다.**
`src/source_guards/unrouted_dispatch_reasons.rs` 가 그 자리다.

모수는 "`params.get("…")` 형태로 **라우팅이 인식하는 키**를 하나도 안 읽는 dispatch
arm" 이고, 명부와 **집합 동등**이다([ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) ③).
갈래는 일곱이며, 묻는 것은 하나다 — *주인 창이 안 정해져도 답이 옳은 이유가
무엇인가.* 이유가 다르면 고치는 방법이 다르기 때문이다: 저장소가 창 밖이면
(`NotWindowOwned`) 고칠 것이 없고, 창 소유인데 합산이 답하면 (`AggregatedList`)
정본이 합산 명부이며, 생성이면 (`CreatesWithoutATarget`) 애초에 실을 id 가 없고,
열린 결함이면 (`PerWindowOpenDefect`) 축을 세워야 한다.

**그 술어가 완벽하지 않다는 것을 명부가 갈래로 흡수한다.** 스캔이 못 보는 두 형태에
각각 갈래를 준다 — 대상 키를 serde 구조체로 읽어 안 보이는 것
(`TargetReadByDeserializer`)과 `request_target` 밖(`App::find_request_owner`)에서
풀려 안 보이는 것(`RoutedOutsideRequestTarget`). 사각을 술어에서 지우는 대신 명부의
행으로 만들어, 그 자리가 검토받게 한다.

## Consequences

- **얻은 것**: 폴백으로 가는 113 개가 이름과 사유로 남았다. 새 메서드가 지목 없이
  들어오면 집합 동등이 잡고, 지목이 생겨 명부가 낡아도 반대 방향으로 잡는다.
  창 소유 상태를 읽거나 큐를 쓰는 후보로 `notification.list` · `system.info` ·
  `recent.query` · `git_viewer.query` · `file_handler.dispatch`가 분류됐다.
  이들을 모두 실행으로 확인한 결함이라고 한 서술은 부정확했다.
  특히 `git_viewer.query`는 창별 큐 이후 전역 세션에서 `local_surface_id`를
  해소하므로, 큐가 다른 창에 있다는 사실만으로 대상 선택 결함이 되지 않는다.
- **잃은 것**: 명부가 크다(113 행). 크기가 뜻을 죽이지 않도록 같은 근거를 공유하는
  행은 "상동 —" 으로 줄이되, 그룹의 첫 행이 근거를 온전히 든다.
- **운영 비용 / 유지 부담**: dispatch arm 이 늘 때마다 그 메서드가 지목을 받는지
  판정해야 한다. 그 판정이 곧 이 명부의 값이다 — 안 하면 조용히 포커스 의존이 된다.

### 구현 확정 보강 — 관측값의 소속 (2026-09-15)

기존 일곱 갈래에 `ScopedObservation`을 더한다. 창별 값이라는 사실을 응답의
소유 ID로 명시하는 읽기 전용 관측이다. `system.info`의 기존 count/index 의미를
바꾸지 않고 workspace ID와 scope를 함께 반환하는 경우가 여기에 해당한다.
버전 같은 프로세스 전역값과 창별 관측값을 한 범위로 설명하지 않는다.

## Alternatives Considered

- **A: 술어를 먼저 짓고 가드를 세운다** — "대상처럼 생긴 키" 를 성질로 정의하는
  조사가 선행이라 그 사이 113 개가 계속 이름 없이 남는다. 그리고 술어가 서더라도
  serde 로 읽는 키는 정적으로 안 보이므로 사각이 0 이 되지 않는다.
- **B: 기존 두 명부 중 하나에 합친다** — 물음이 다르다. 합산 명부는 "이 목록을
  합쳐야 하는가", `PAIR_EXEMPT` 는 "한정 키를 한정 밖에서 읽는가" 다. 합치면 그중 한
  물음의 답이 사라진다([ADR-0175](0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md)
  가 같은 이유로 두 명부를 갈라 두었다).
- **C: 개수 하한만 둔다** — 하한은 무엇이 빠졌는지 이름으로 말하지 못한다. 빠진 것에
  위반이 없으면 신호가 아예 없다(ADR-0133 이 같은 이유로 집합 동등을 요구한다).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 대상 키를 성질로 판정하는 술어가 서고 그것이 `request_target` 에 반영되면, 이
  명부의 모수가 그만큼 줄어든다 — 그때 남는 행이 무엇인지 다시 센다.
- serde 로 읽는 params 키를 정적으로 뽑는 수단이 생기면
  (`TargetReadByDeserializer` 갈래가 빈다), 그 갈래를 지운다.
- `PerWindowOpenDefect` 가 0 이 되면 이 명부의 남은 값은 회귀 방지뿐이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 명부의 사유가 실제 저장소와 어긋나는가. 재는 법: 그 핸들러가 `state`/`engine` 을
  실제로 쓰는지와, 창 생성 경로(`App::ensure_engine_and_plugins`)가 그 Arc 를
  공유시키는지를 **함께** 읽는다. 한쪽만 읽으면 공유를 창별로도, 창별을 공유로도
  잘못 읽는다 — `approval_store` 에서 실제로 그 오독이 났다.

## References

- 코드 근거(현재 위치): `src/source_guards/unrouted_dispatch_reasons.rs`
- [`docs/design/policies/focus.md`](../design/policies/focus.md) — 폴백과 그 아래 층
- [ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) — 모수 고정
- [ADR-0175](0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md) — 옆 명부와 물음이 다르다
