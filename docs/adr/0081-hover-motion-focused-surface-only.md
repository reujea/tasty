# ADR-0081: 버튼 없는 hover motion(1003)은 focused surface 에만 보고한다

- **Status**: Accepted
- **Date**: 2026-08-24
- **Tags**: mouse, input, tracking, focus, terminal, adr-0019
- **Group**: terminal-input

## Context

DECSET 1003(AnyEventMouse)은 버튼과 무관하게 커서가 셀을 넘을 때마다 motion 을 보고하는 모드다. tasty 는 이 모드를 지원 목록에 올려두고도 hover 를 전혀 내보내지 않았다 — 유일한 motion 보고 경로가 좌버튼이 눌린 동안에만 도는 드래그 블록 안에 있었기 때문에 1003 이 실질적으로 1002 와 같게 동작했다.

hover 를 구현하면 드래그·클릭에는 없던 문제가 생긴다. **클릭은 사용자가 대상을 고르는 행위지만 hover 는 아니다.** 커서가 창을 가로지르는 동안 지나치는 모든 surface 가 잠재적 수신자가 되므로, 대상 선정 규칙을 정하지 않으면 마우스를 움직이기만 해도 배경에서 돌고 있는 트래킹 앱들에 입력 바이트가 지속적으로 흘러든다. [ADR-0019](0019-mouse-button-reporting-app-delegation.md)는 드래그 motion 까지만 결정했고 이 축을 다루지 않았다.

[`docs/architecture/input-layer.md`](../architecture/input-layer.md)의 "비활성 surface 클릭 = 전환 우선" 절은 같은 종류의 누수를 이미 한 번 판단했다 — 비활성 surface 클릭을 앱에 보고하지 않고 포커스 전환으로 소비하는 근거가 **"배경 캡쳐 TUI 로 마우스가 새는 것을 막는다"** 다.

## Decision

**hover motion 은 focused surface 에만 보고한다.** 커서 아래 surface 가 focused surface 가 아니거나 tasty 창 자체가 비포커스면 아무것도 보내지 않으며, **포커스를 옮기지도 않는다**(hover 는 대상을 고르는 행위가 아니다). 여기에 경계면 가드가 더해진다 — divider 히트 밴드(분할선 ±`DIVIDER_HIT_THRESHOLD`), OS 창 리사이즈 가장자리 밴드, divider 드래그 진행 중에는 보고하지 않는다. 입력 z-order 상 Divider(순서 6)가 surface 콘텐츠(순서 7)보다 위라는 기존 규칙을 hover 에도 그대로 적용한 것이다.

버튼을 누른 채 시작한 드래그 motion 은 이 가드들에 걸리지 않는다. 대상 surface 를 press 시점에 고정하고 좌표를 그 surface 로 클램프하므로, 커서가 밴드나 이웃 surface 로 넘어가도 원래 surface 기준으로 계속 보고된다.

## Consequences

- **얻은 것**: 마우스 이동만으로 배경 TUI 들에 바이트가 흘러드는 경로가 원천 차단된다. PTY 쓰기는 아무리 많은 트래킹 앱이 떠 있어도 최대 한 surface 분량이다. 커서 아이콘(↔/↕)과 hover 수신 여부가 항상 일치해 "리사이즈하려고 경계에 붙였는데 TUI 가 가장자리 셀을 계속 강조" 하는 불일치가 없다.
- **잃은 것**: 비포커스 pane 의 TUI 는 마우스를 올려도 반응하지 않는다. 한 번 클릭해 활성화한 뒤부터 hover 가 동작한다 — click-to-activate 와 같은 모델이라 학습 비용은 새로 생기지 않지만, 여러 pane 의 TUI 를 동시에 hover 로 훑는 사용법은 불가능하다.
- **운영 비용 / 유지 부담**: hover 는 `CursorMoved` 마다 평가된다. 셀 dedup(`last_mouse_report_cell`, `(surface_id, col, row)`)이 유일한 방어선이므로 그 키가 어긋나면 곧바로 PTY 쓰기 폭주가 된다 — surface 축을 키에서 빼면 안 된다. 경계 판정은 press 경로·커서 아이콘 경로와 **같은 threshold 상수**를 공유해야 하며, 리터럴로 흩어지면 세 판정이 조용히 드리프트한다.

## Alternatives Considered

- **커서 아래 surface 에 보고(포커스 무관)**: 1003 규약에 가장 충실하고 여러 pane 을 동시에 쓰는 사용감이 좋다. 그러나 위 누수를 막을 수단이 없고, 비포커스 pane 의 surface divider 는 현재 hit-test 대상이 아니라(focused pane 만 검사) 경계면 가드가 그 pane 에서 뚫린다 — 전체 pane 순회 hit-test 를 새로 만들어야 성립한다.
- **hover 가 포커스를 옮기게 한다(focus-follows-mouse)**: 대상 문제는 사라지지만 마우스가 스쳐 지나가는 것만으로 포커스가 바뀌어 키 입력 목적지가 흔들린다. tasty 는 click-to-activate 를 택한 제품이라 정면으로 어긋난다.
- **설정으로 열어둔다**: 두 모델 중 무엇이 기본인지부터 정해야 하고, 어느 쪽이든 위 트레이드오프는 그대로 남는다. 사용 요구가 실제로 관측되기 전에 축을 늘리지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 비포커스 pane 의 TUI hover 가 필요하다는 실사용 요구가 관측된다 — 그때는 전체 pane 순회 divider hit-test 가 선행 조건이다.
- tasty 가 focus-follows-mouse 를 도입한다(그 경우 대상 선정 규칙이 통째로 바뀐다).
- 셀 dedup 만으로 PTY 쓰기 부하가 감당되지 않아 프레임 단위 코얼레싱이 필요해진다.

## References

- [ADR-0019](0019-mouse-button-reporting-app-delegation.md) — 트래킹 ON 시 마우스 앱 위임, 드래그 motion 결정
- [ADR-0013](0013-niche-input-private-modes-unsupported.md) — 1000/1002/1003 표준 경로 전제
- [`docs/architecture/input-layer.md`](../architecture/input-layer.md) — 입력 z-order(순서 6 Divider > 7 Terminal), 비활성 surface 클릭 전환 우선
- [`docs/features/terminal/index.md`](../features/terminal/index.md) — 마우스 트래킹 지원 범위
