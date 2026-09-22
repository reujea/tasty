# ADR-0125: 카테고리 착지점은 id 를 들고, 재정렬 축은 제거 축과 같은 초크포인트로 모은다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: focus, workspace, reorder, index-vs-id, cascade, invariant, adr-0113
- **Group**: window-workspace-lifecycle

## Context

[ADR-0113](0113-close-preserves-the-focused-target.md) 이 **제거** 축에서 "인덱스로 저장된
활성 포인터가 계속 같은 대상을 가리킨다" 는 불변식을 세우고
`AppState::fix_workspace_pointers_after_removal` 로 적용했다. 그때 세 포인터 중
`AppState::category_last_active`(카테고리 quick-switch 착지점)도 전역 인덱스를 값으로
들고 있어 함께 보정 대상이 됐다.

ADR-0113 은 **재정렬** 축을 의도적으로 남겼고, 재검토 조건에 등재했다 —
"`switch_to_category` 가 착지점의 카테고리 소속을 재검증해 다른 카테고리로 튀지는
않으므로 제거 축만 보정한다. 그 완충이 사라지거나 **같은 카테고리 안 오착지가
보고되면** 재정렬 축도 같은 헬퍼 계열로 수렴시킨다."

그 조건이 충족됐다. 재정렬 경로 두 곳(`AppState::move_workspace` ·
`cascade_workspace_moved`)은 `active_workspace` 만 보정하고 착지점 맵은 건드리지
않는다. 밀린 인덱스가 **같은 카테고리의 다른 워크스페이스**를 가리키면 재검증 필터를
통과해 그대로 착지한다. 재현: A={ws0, ws2} 에서 ws2 를 마지막으로 본 뒤 ws0 을 맨 뒤로
옮기면, A 로 quick-switch 했을 때 ws0 에 착지한다.

ADR-0113 은 이 경우의 처방까지 적어뒀다("같은 헬퍼 계열로 수렴"). 그런데 그것은 축을
하나 더 늘리는 처방이다 — 인덱스를 값으로 드는 한 워크스페이스 목록을 건드리는 연산이
새로 생길 때마다 이 맵을 함께 밀어줘야 하고, 그중 하나를 잊는 것이 이 결함의 형태였다.

## Decision

**두 가지를 함께 정한다.**

**① `category_last_active` 는 워크스페이스 id 를 값으로 든다.** 인덱스를 값으로 드는
것을 그만두면 제거·재정렬 어느 축도 이 맵을 건드릴 필요가 없다 — 밀어주기를 잊을
자리가 애초에 사라진다. 쓰기(`switch_workspace`)는 그 워크스페이스의 id 를 넣고,
읽기(`switch_to_category`)는 id → 인덱스를 한 번 찾는다. 못 찾으면(제거됐거나
카테고리가 바뀌었으면) 기존과 같이 그 카테고리의 first 로 폴백한다.
`fix_workspace_pointers_after_removal` 의 맵 재계산은 삭제한다.

이것은 ADR-0113 의 대안 A(활성 포인터를 id SoT 로)를 **포인터별로 갈라서** 채택한
것이다. `active_workspace` 와 `Pane::active_tab` 은 인덱스 SoT 로 남는다 — 그 둘은
렌더·순회·단축키가 매 프레임 인덱스로 읽는 값이라 id SoT 로 바꾸면 조회가 뜨거운
경로로 들어간다. 착지점은 다르다: **사용자 키 입력당 한 번** 읽히고 워크스페이스 수는
수십 규모라, 선형 탐색 한 번이 유지보수 전체를 대체한다. 세 포인터를 한 덩어리로 볼
이유가 없다.

**② 재정렬 축의 `active_workspace` 보정은 초크포인트 하나를 지난다.** 순수함수
`active_index_after_move(active, from, to)` 가 규칙의 유일한 정의이고,
`AppState::fix_workspace_pointers_after_move(from, to)` 가 그것을 적용한다. 재정렬
경로 셋(`move_workspace` · gui `cascade_workspace_moved` · headless stub)이 전부
그것만 부른다. 제거 축의 `fix_workspace_pointers_after_removal` 과 대칭이다.

headless stub 은 이 cascade 를 **빈 함수**로 두고 있었다. 오늘의 headless 는
`active_workspace` 가 0 을 벗어날 수단이 없어 결과가 같지만, 그래서 어떤 실행 테스트도
이 회귀를 못 잡는다 — ADR-0113 이 제거 축에서 소스 가드를 둔 것과 정확히 같은 이유로
`both_move_cascades_route_through_the_pointer_helper` 를 함께 둔다.

## Consequences

- **얻은 것**: 착지점이 인덱스를 움직이는 모든 연산과 무관해졌다. 워크스페이스 목록을
  건드리는 새 연산(정렬·필터·일괄 이동 등)이 생겨도 이 맵은 손댈 것이 없다. 값 타입이
  `usize` 에서 `WorkspaceId`(u32) 로 바뀌어 인덱스를 넣는 실수는 컴파일되지 않는다.
  재정렬 규칙은 정의가 하나뿐이라 경로별로 갈릴 수 없다.
- **잃은 것**: 활성 포인터 셋의 표현이 균일하지 않다 — 둘은 인덱스, 하나는 id 다.
  "활성 포인터는 인덱스 SoT" 라는 한 문장으로 설명되지 않으므로,
  [`focus.md`](../design/policies/focus.md) 가 어느 것이 어느 쪽인지를 명시한다.
- **운영 비용 / 유지 부담**: 착지 시 선형 탐색 한 번(사용자 키 입력당). 인덱스를 값으로
  드는 상태를 **새로** 만들면 여전히 두 초크포인트에 등록해야 한다 — 그 등록 지점이
  `fix_workspace_pointers_after_removal` 과 `fix_workspace_pointers_after_move` 로
  두 곳이라는 것이 유지 부담의 전부다.

## Alternatives Considered

- **A: ADR-0113 의 처방대로 착지점도 재정렬 헬퍼에 태운다** — 예고된 처방이라 가장
  마찰이 적다. 그러나 밀어주기가 필요한 축이 둘로 늘 뿐 결함의 형태(인덱스를 값으로
  들어 매 연산마다 동기화해야 함)는 그대로 남는다. 세 번째 축이 생기면 같은 사고가
  다시 난다.
- **B: 세 활성 포인터를 모두 id SoT 로 옮긴다** — 표현이 균일해지고 보정 헬퍼가 통째로
  사라진다(ADR-0113 대안 A). 그러나 `active_workspace` 와 `Pane::active_tab` 은 렌더
  루프와 순회가 매 프레임 인덱스로 읽는 값이라, 조회를 뜨거운 경로에 넣게 된다.
  범위도 이 결함의 수정을 훨씬 넘는다. 필요해지면 별도 트랙이다.
- **C: 재정렬 시 착지점 항목을 그냥 지운다** — 밀림은 사라지지만 사용자가 재정렬할
  때마다 "마지막으로 쓰던 워크스페이스" 기억이 날아간다. 결함을 기능 상실로 바꾸는
  교환이라 기각.
- **D: `switch_to_category` 의 재검증 필터를 강화한다** — 완충을 두껍게 하는 방향.
  같은 카테고리 안에서는 어떤 필터로도 "그 인덱스가 원래 그 워크스페이스였는지" 를
  알 수 없다(그 정보가 인덱스에 없다). 원리적으로 못 막는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `active_workspace` 나 `Pane::active_tab` 을 id SoT 로 옮기는 트랙이 생기면 — 그때는
  표현이 다시 균일해지므로 두 보정 헬퍼가 통째로 불필요해진다(대안 B).
- 워크스페이스 수가 선형 탐색이 부담될 규모로 커지면 — id → 인덱스 조회를 맵으로
  받쳐야 한다. 지금은 사용자 키 입력당 한 번이라 근거가 없다.
- 인덱스를 값으로 드는 활성 상태가 새로 생기면 — 두 초크포인트에 등록하는 대신 그것도
  id 로 드는 것이 맞는지 먼저 본다.
- 사용자가 **여러 워크스페이스를 한 번에** 재정렬하는 기능이 생기면 — 단일 (from, to)
  로 표현되지 않으므로 규칙을 일반화해야 한다(ADR-0113 의 "제거 집합" 트리거와 동형).

## References

- [ADR-0113](0113-close-preserves-the-focused-target.md) — 제거 축 결정. 그 재검토
  조건("같은 카테고리 안 오착지가 보고되면")이 이 ADR 을 촉발했다.
- [`docs/design/policies/focus.md`](../design/policies/focus.md) — 운영 상태(제거 축 ·
  재정렬 축 · 코드 위치).
- `src/state/workspace.rs` — `active_index_after_move` ·
  `AppState::fix_workspace_pointers_after_move` · `switch_to_category`.
