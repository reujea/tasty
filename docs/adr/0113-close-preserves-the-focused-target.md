# ADR-0113: 삭제로 인한 인덱스 이동에서도 포커스 대상을 보존한다 — 사라진 것을 보고 있었을 때만 시야가 움직인다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: focus, user-agent-separation, close, cascade, workspace, tab, pane, index-vs-id, remote-attach, invariant
- **Group**: window-workspace-lifecycle

## Context

Tasty 의 활성 포인터 세 개 중 둘은 **인덱스**가 진실 소스다 —
`AppState::active_workspace: usize`(보고 있는 워크스페이스)와 `Pane::active_tab: usize`(보고 있는
탭). 나머지 하나 `Workspace::focused_pane` 은 id 다.

close 경로는 세 계층 모두 **범위를 벗어났을 때만 보정**했다. 앞쪽 원소가 빠져 뒤 원소들의
인덱스가 한 칸씩 당겨지는 경우를 다루지 않아, 손대지 않은 인덱스가 **다른 대상**을 가리키게
됐다. pane 계층은 한술 더 떠 닫힌 pane 이 포커스였는지 보지 않고 무조건 `focused_pane` 을
첫 pane 으로 재배정했다.

결과는 [identity §2.1 ①](../identity.md)("에이전트 행동의 부수효과가 사용자 상태에 닿지
않는다 — **포커스**")의 정면 위반이다. 사용자가 아무 조작도 하지 않았는데, 에이전트가 release
IPC `surface.close` 로 **자기 워크스페이스를 정리**하는 것만으로 사용자가 보던 화면이 바뀐다.
동시성(로컬 사용자 + 여러 AI 에이전트)이 Tasty 의 정체성이므로 이건 부수적 UX 흠이 아니라
정체성 결함이다.

같은 코드가 **사용자 경로에도** 쓰인다 — 컨텍스트 메뉴로 앞쪽 탭/워크스페이스를 닫아도 같은
밀림이 난다. 그리고 원격 attach 의 구조 변경 forward(`execute_forwarded_structural_op`)도 같은
close 경로를 타므로, 원격 사용자의 정리가 로컬 사용자 화면을 흔들 수 있었다.

이미 **올바른 대조 구현이 레포 안에 있었다**: `AppState::move_workspace` 는 from/to 를 비교해
`active_workspace` 를 정확히 따라 움직이고, `Pane::move_tab` 도 같다. `Core::apply_close_pane`
에는 `was_focused` 가드가 있다. 결함은 규칙이 없어서가 아니라 **같은 규칙이 close 경로에만
빠져 있어서** 생겼다.

## Decision

**삭제로 인한 인덱스 이동에서도 활성 포인터는 같은 대상을 계속 가리킨다. 시야가 움직이는
경우는 단 하나 — 사용자가 보고 있던 대상 자체가 사라졌을 때다.**

이 규칙을 origin(User/Agent/System)으로 분기하지 **않는다.** 대상 기준 보정은 사용자 경로에서도
옳은 동작이므로, 분기를 두면 같은 결함을 사용자 경로에 남기는 것밖에 안 된다. origin 게이트는
"에이전트가 새로 만든 것으로 포커스를 옮기지 않는다"(`cascade_workspace_created` ·
`cascade_surface_split`)처럼 **이동이 정책적으로 갈리는** 곳에만 쓴다.

세부 결정 넷:

1. **규칙은 계층마다 한 곳에 둔다.** tab 은 `Pane::remove_tab_preserving_active`,
   workspace 는 `state::workspace::active_index_after_removal` +
   `AppState::fix_workspace_pointers_after_removal`. 같은 규칙이 네 곳(cascade · AppState 인라인 ·
   GUI 메뉴 · mirror teardown)에 흩어져 있던 것이 결함의 직접 원인이라, 재발 방지는 헬퍼로
   수렴시키는 것이다.
2. **pane 은 `was_focused` 가드로 통일한다.** 닫힌 pane 이 포커스가 아니었으면 `focused_pane`
   을 건드리지 않는다. `close_active_pane`(정의상 포커스 pane 을 닫는 사용자 단축키)의 무조건
   재배정은 그대로 둔다 — 그건 "대상이 사라진" 경우다.
3. **인덱스 SoT 를 id SoT 로 바꾸지 않는다.** 근본 해법은 `active_workspace`/`active_tab` 을 id 로
   바꾸는 것이지만, 두 필드는 저장 스키마(`SavedLayout`) · 사이드바 · 카테고리 전환 · 프리셋 등
   수십 곳이 인덱스로 읽는다. 이 결함은 "인덱스가 대상을 놓친다" 는 것이고, 그건 제거 시점에
   인덱스를 옮겨주면 해소된다. id 전환은 별도 트랙의 판단으로 남긴다(재검토 조건 참조).
4. **제거 위치는 이벤트로 실어 보내되, id 와 한 필드로 묶는다.** `Core` 는
   `AppState::active_workspace` 를 모르므로 보정은 cascade 몫인데, cascade 시점엔 워크스페이스가
   이미 사라져 위치를 알 수 없다. 그래서 `CoreEvent::SurfaceClosed`/`MoveSurfaceApplied` 의
   `workspace_id_purged: Option<u32>` 를 **`workspace_purged: Option<(usize, u32)>`** 로 바꿔
   인덱스와 id 를 함께 싣는다. 둘을 별도 `Option` 두 개로 두면 "id 는 실렸는데 인덱스는 빠졌다"
   가 타입상 표현 가능해지고, 그 경우 보정이 조용히 건너뛰어져 사용자 화면이 밀린다 — 이 결함이
   원래 그런 종류였으므로 재발 가능성을 타입에서 없앤다. `CoreEvent` 는 `pub(crate)` 이고 어떤
   와이어에도 직렬화되지 않아(파생은 `Debug, Clone` 뿐, 소비자는 전부 프로세스 내부) 이 변경에
   호환성 부담이 없다. 남는 런타임 불변식은 "Workspace level cascade ⟺ `workspace_purged`
   가 Some" 하나뿐이고 그건 cascade 의 `debug_assert` 로 고정한다.

카테고리 quick-switch 착지점(`AppState::category_last_active`)도 전역 인덱스를 값으로 들고 있어
같은 밀림을 겪는다. 같은 헬퍼에서 함께 보정하고, 제거된 워크스페이스를 가리키던 항목은 지운다
(사용 시점에 first 로 폴백).

## Consequences

- **얻은 것**: 에이전트의 close 가 사용자 시야를 흔들지 않는다(3 계층 전부). 같은 보정이 사용자
  경로·원격 attach forward 경로에도 적용돼, "앞쪽 탭을 닫으면 보던 탭이 바뀐다" 는 오래된
  체감 결함이 함께 사라진다. `surface.move`(A 의 옛 자리 구조적 close)도 같은 규칙을 탄다.
  mirror 워크스페이스 teardown(`remove_mirror_workspace_from_engine`)은 **이미 같은 보정을
  하고 있었다** — 이번에 고쳐진 것이 아니라 같은 헬퍼로 수렴시킨 것이고, 거기서 순증한 것은
  카테고리 착지점 보정뿐이다.
- **빌드 형태 parity**: 보정은 gui cascade 와 headless cascade(`dispatch_domain_stubs.rs`)
  양쪽에서 같은 헬퍼를 지난다. 오늘의 headless 는 `active_workspace` 가 0 을 벗어날 경로가
  없어(레이아웃 복원 미적용, `preset.apply` 의 focus 강제 off, 워크스페이스 전환은 gui 전용
  debug IPC) 결과가 옛 clamp 와 같지만, 한쪽만 고쳐 두면 같은 불변식이 실행 형태에 따라 다르게
  성립한다. headless 는 오늘의 구조상 그 어긋남을 **행동으로 드러낼 수단이 없어**(위 세 조건) 실행
  테스트로는 잡히지 않는다 — 그래서 두 cascade 가 모두 헬퍼를 지나는지 소스 수준 가드
  (`both_close_cascades_route_through_the_pointer_helper`)로 기본 빌드에서 고정한다.
- **잃은 것**: `CoreEvent` 두 variant 의 필드 타입이 바뀌었다(그 이벤트를 구조분해하는 4 개
  호출부가 함께 변경). 인덱스 SoT 자체는 그대로라, 앞으로 workspace 를 제거하는 **새 경로**를
  추가하는 사람은 여전히 헬퍼를 부를 책임을 진다 — 이벤트 안에서 id 와 인덱스가 짝이라는 것은
  타입이 강제하지만, "제거했으면 이벤트에 싣는다" 는 강제하지 못한다.
- **운영 비용 / 유지 부담**: workspace 제거 지점은 현재 다섯 곳
  (`Core::close_case_workspace` · `Core::detach_surface_for_move` · `AppState::close_case_workspace` ·
  `AppState::close_workspace_at` · mirror teardown)이고 모두 헬퍼를 지난다. 여섯 번째가 생기면
  같은 규칙을 태워야 한다. 회귀는 세 계층 각각의 단위 테스트가 **id 기준으로** 고정한다.

## Alternatives Considered

- **A. `active_workspace`/`active_tab` 을 id 로 바꾼다** — 결함의 원인을 없애는 근본 해법이고
  컴파일러가 "인덱스가 대상을 놓치는" 실수 자체를 불가능하게 만든다. 채택하지 않은 이유는
  변경 폭이다: 두 필드를 인덱스로 읽는 지점이 저장 스키마 · 사이드바 · 카테고리 전환 · 프리셋
  전반에 걸쳐 있어, 포커스 결함 하나를 고치려고 그 전부를 같은 커밋에서 흔드는 것은 위험 대비
  과하다. 대신 재검토 조건으로 남긴다.
- **B. origin 이 Agent 일 때만 보정한다** — "에이전트가 사용자 상태에 닿지 않는다" 는 조문에
  가장 문자 그대로 대응하지만, 사용자가 컨텍스트 메뉴로 앞쪽 탭을 닫았을 때 보던 탭이 바뀌는
  것도 그 자체로 결함이다. 대상 기준 보정은 양쪽 모두에서 옳으므로 분기가 정당화되지 않는다.
- **C. 보정을 `close_pane`/`remove` 같은 자료구조 API 안에 넣는다** — 호출자가 잊을 수 없게
  된다는 장점이 있지만, `Pane`/`PaneLayout` 은 "누가 보고 있는지" 를 모르는 순수 레이아웃
  자료구조이고 `active_workspace` 는 아예 다른 타입(`AppState`)에 산다. 계층을 섞는 대신 계층
  별로 헬퍼를 하나씩 두는 쪽을 택했다.
- **D. close 직전에 활성 대상의 id 를 기억했다가 close 후 다시 찾는다** — 인덱스를 몰라도 되고
  대상 소멸 판정도 "못 찾음" 으로 자연스럽다. 다만 그 캡처를 close 를 부르는 **모든** 경로에
  심어야 하고(빠뜨리면 조용히 옛 동작), cascade 는 Core 가 이미 자료구조를 바꾼 뒤에 도는지라
  캡처 지점이 경로마다 달라진다. 제거 위치 하나를 이벤트에 싣는 쪽이 검증 가능한 불변식이 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 활성 포인터를 id SoT 로 옮기는 트랙이 생기면(대안 A) — 그때는 인덱스 보정 헬퍼가 통째로
  불필요해지므로 제거한다. workspace 제거 경로가 계속 늘어 헬퍼 호출을 빠뜨리는 사고가 실제로
  나면 그 자체가 전환 신호다.
- 사용자가 **여러 워크스페이스/탭을 한 번에** 닫는 기능(카테고리 통째 닫기 등)이 생기면 —
  단일 제거 인덱스로는 표현되지 않으므로 규칙을 "제거 집합" 기준으로 일반화해야 한다.
- 워크스페이스 목록이 정렬/필터되어 표시 순서와 저장 순서가 갈리면 — 그 시점에는 "인덱스가
  같은 대상을 가리킨다" 는 명제 자체가 표시 계층에서 깨진다.
- `active_workspace` 를 window 별이 아니라 다른 단위로 들게 되면 보정 대상 목록이 바뀐다.
- 워크스페이스 **재정렬**(`AppState::move_workspace` · `cascade_workspace_moved`)에서
  `category_last_active` 가 밀리는 것이 실제 문제로 보고되면 — 지금은 `switch_to_category` 가
  착지점의 카테고리 소속을 재검증해 다른 카테고리로 튀지는 않으므로 제거 축만 보정한다.
  그 완충이 사라지거나(카테고리 재검증 제거) 같은 카테고리 안 오착지가 보고되면 재정렬 축도
  같은 헬퍼 계열로 수렴시킨다.

## References

- [identity §2.1 ①·§2.3](../identity.md) — 에이전트 부수효과가 사용자 포커스에 닿지 않는다
- [design/policies/focus.md](../design/policies/focus.md) — 포커스 정책 운영 상세("삭제로 인한 인덱스 이동")
- [architecture/close-sequence.md](../architecture/close-sequence.md) — close cascade 단계(C1~C5)
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 점유 모델. 원격 attach forward 가 같은 close 경로를 탄다
- 대조 구현: `AppState::move_workspace` · `Pane::move_tab` · `Core::apply_close_pane` 의 `was_focused` 가드
- [ADR-0125](0125-category-landing-points-hold-ids.md) — 위 재검토 조건 중 재정렬 축(`category_last_active` 오착지)이 실제로 충족돼 내려진 후속 결정
