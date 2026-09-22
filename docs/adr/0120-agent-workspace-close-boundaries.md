# ADR-0120: 에이전트가 부르는 워크스페이스 닫기의 경계

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: agent-surface, workspace, close, remote-attach, occupancy, identity-principles, adr-0040, adr-0113
- **Group**: window-workspace-lifecycle

## Context

`workspace.create` 는 release IPC/CLI 에 있는데 닫기는 없었다. 에이전트는 워크스페이스를
치우려면 안의 surface 를 전부 열거해 하나씩 닫는 우회를 써야 했고, 그건 원칙 2("에이전트
기능은 IPC + CLI 양면으로 동작한다")가 말하는 상태가 아니다. `workspace.close` 를 열면서
세 가지를 정해야 했다.

**① 마지막 워크스페이스를 닫으면 무엇이 일어나는가.** GUI 에서 사용자가 마지막
워크스페이스를 닫으면 창까지 닫힌다(`view/main/redraw.rs` 의 컨텍스트 메뉴 경로가
`request_close()` 를 부른다). 같은 동작을 IPC 에 그대로 옮기면 "워크스페이스 하나를
치워라" 는 요청이 **창 종료로 번진다.** 에이전트는 id 를 루프로 훑기 때문에 마지막
하나에 도달하는 것이 예외가 아니라 흔한 경로다.

**② mirror 워크스페이스는 어떻게 다루는가.** 원격을 attach 해 들고 있는 mirror
워크스페이스는 거두는 절차가 따로 있다 — `app::attach_client` 의
`remove_mirror_workspace_from_engine` 이 mirror 터미널 · mirror busy 엔트리 ·
mirror attention · mesh 프레임 캐시를 워크스페이스 행과 **함께** 걷어낸다. 일반 close
경로(`AppState::close_workspace_at`)는 그중 아무것도 하지 않는다.

**③ 파괴력에 대한 안전장치를 둘 것인가.** 워크스페이스 하나를 닫으면 그 안에서 돌던
터미널 세션이 전부 죽는다. surface 하나를 닫는 것과 규모가 다르다.

**④ 원격 attach 가 점유 중인 surface 가 들어 있으면 어떻게 하는가.** ②의 mirror 와는
다른 상태다 — mirror 는 "이 인스턴스가 원격을 비추는 그림자" 이고, 하드 점유는 "이
인스턴스가 소유한 surface 를 원격 클라이언트가 배타적으로 잡고 있는" 상태다
(`OccupancyRegistry::is_hard_occupied`, [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)). 후자에서
그 터미널을 실제로 쓰고 있는 것은 **다른 사람**이고, 로컬 입력은 이미 차단돼 있다.

배경 원칙은 [`identity.md`](../identity.md) 원칙 1(사용자 행동 ↔ 에이전트 행동 분리)과
원칙 3(포커스 독립성), 포커스 보존의 구체는 [ADR-0113](0113-close-preserves-the-focused-target.md).

## Decision

**① 마지막 워크스페이스는 거절한다.** 창을 없애는 것은 별개의 결정이므로 에이전트에게
`window.close` 라는 명시적 수단을 따로 준다. 워크스페이스 하나를 닫으라는 요청이 창
종료로 번지지 않는다. (정정: 이 결정은 헤드리스를 다루지 않았다 — 헤드리스에는 `window.close`
가 없어(`-32017` gated out) 마지막 워크스페이스를 닫을 수단이 없다. 거절 자체는 같고, 헤드리스
거절 문구는 `window.close` 를 권하지 않는다.)

**② mirror 워크스페이스도 거절한다.** 전용 teardown 이 따로 있는 대상을 그것을 지나지
않는 경로로 없애지 않는다. detach 는 attach 세션 쪽 수단을 쓴다. 같은 라운드에 병합된
`surface.attention.clear` 가 mirror surface 에 대해 정확히 같은 선택을 했다.

**③ 확인 절차(force 플래그 등)는 두지 않되, 비가역성을 문서로 알린다.** 자기 자신이 든
워크스페이스 거절과 ①의 마지막 워크스페이스 거절이 최악의 사고를 막고, 에이전트
`surface.close` 도 동일하게 비가역이라 규약이 일관된다. 대신 **CLI `--help` 와 사용자
가이드에 "되돌릴 수 없다" 를 명시**한다 — 안전장치를 두지 않는 대신 호출자가 그 사실을
모르는 상태로 두지는 않는다.

**④ 원격 attach 가 하드 점유 중인 surface 를 든 워크스페이스도 거절한다.** 워크스페이스의
surface 를 훑어 하나라도 하드 점유 상태면 거절하고, 어느 surface 때문인지 응답에 싣는다.
점유 중인 터미널에서 일하고 있는 것은 이 인스턴스의 사용자가 아니라 원격 사용자이고,
이 경로는 비가역이라 "닫고 나서 되돌리기" 가 없다. 로컬 입력조차 차단된 대상을 로컬
에이전트가 파괴할 수 있는 것은 점유 모델의 구멍이다. `surface.attention.clear` 가 하드
점유 surface에 대해 이미 같은 선택을 했다(그쪽은 상태 소유권 논거, 이쪽은 파괴 논거지만
둘 다 "점유 중에는 holder 가 주인" 이다).

넷은 한 가족이다: **에이전트가 부르는 파괴적 워크스페이스 명령은, 회복 경로가 따로 있는
대상(창 · mirror)이나 지금 다른 주체가 쓰고 있는 대상(하드 점유)에는 손대지 않고, 손대는
범위에 대해서는 비가역성을 숨기지 않는다.**

## Consequences

- **얻은 것**
  - 원칙 2 충족 — 만들 수 있는 주체가 닫을 수도 있다. 우회(surface 를 전부 열거해 하나씩
    닫기)가 사라진다.
  - "워크스페이스를 닫아라" 가 창 종료로 번지지 않는다. 에이전트 루프가 마지막 하나에
    도달해도 결과가 예측 가능하다.
  - mirror 의 전용 teardown 을 우회하는 경로가 생기지 않는다.
  - 원격 사용자가 점유해 쓰고 있는 터미널이 로컬 에이전트의 루프에 휩쓸려 죽지 않는다.
  - 비가역성이 호출 지점(`--help`)에서 보인다.
- **잃은 것**
  - "워크스페이스를 다 치우고 창도 닫는다" 를 한 번에 못 한다 — `workspace.close` 반복 후
    `window.close` 두 단계다. (정정: 헤드리스에는 둘째 단계가 없다 — `window.close` 가 `-32017`
    이라 마지막 워크스페이스는 닫을 수 없다.)
  - mirror 워크스페이스를 IPC 로 치우려면 attach 세션 쪽 수단을 알아야 한다.
  - 실수로 닫은 워크스페이스를 되살릴 방법이 없다. `Ctrl+Shift+T` 되돌리기 스택은 사용자가
    자기 손으로 닫은 것만 담는다(원칙 1 — 에이전트 행동의 부수효과는 사용자 상태에 닿지
    않는다). 스크롤백도 디스크에 남지 않는다.
- **운영 비용 / 유지 부담**
  - 워크스페이스를 제거하는 새 경로가 생길 때마다 `WorkspaceCloseOrigin` 과
    `fix_workspace_pointers_after_removal` 을 함께 태워야 한다(후자는 ADR-0113).
  - mirror 판정은 `Workspace::mirror` 플래그 한 줄이라 attach 모델이 바뀌면 함께 본다.
  - ④는 워크스페이스의 surface 를 전수 훑는다. 점유 술어가 hard 전용이라는 ADR-0040 의
    계약에 기대므로, soft 점유가 그 술어에 섞이면 정상 요청이 거절되기 시작한다.

## Alternatives Considered

- **①-A 마지막이면 창까지 닫는다 (GUI 동작을 그대로 옮김)** — GUI 와 동작이 같아진다는
  장점이 있으나, 에이전트에게는 "워크스페이스 정리" 요청 하나가 사용자의 창을 없애는
  결과가 된다. 사용자 행동과 에이전트 행동을 가르는 원칙 1 의 취지에 정면으로 어긋난다.
- **①-B 마지막이면 빈 워크스페이스로 대체한다** — 창은 살아남으므로 ①-A 의 문제는 없다.
  하지만 "닫아라" 에 대해 **새 워크스페이스를 만들어 주는** 것이라, 요청하지 않은 생성이
  일어나고 그 새 워크스페이스가 사용자 화면에 나타난다. 거절이 더 정직하다.
- **②-A mirror 면 `cleanup_mirror_workspace` 로 라우팅한다** — 기능적으로 가장 완전하지만
  attach 세션 상태(소켓 · parked engine 순회)까지 다뤄야 해서 close 경로의 책임을 넘는다.
  거절로 시작하고, 필요가 실제로 생기면 그때 연다.
- **②-B 그냥 닫는다 (GUI 컨텍스트 메뉴와 동일)** — GUI 도 같은 구멍이 있다는 것은 "이
  경로가 만든 문제가 아니다" 의 근거일 뿐 "안 막아도 된다" 의 근거가 아니다. 도달 확률도
  다르다 — GUI 는 사용자가 mirror 워크스페이스에 우클릭해 닫기를 골라야 하지만, 에이전트는
  루프에서 id 를 훑는다.
- **③-A 파괴 규모 기반 게이트 (`force: true` 요구)** — 예: 실행 중 foreground 프로세스가
  있는 surface 가 N 개 이상이면 `force` 없이는 거절. 사고는 줄지만 이 프로젝트에 그런
  선례가 없고, 에이전트 자동화 코드에 조건 분기를 늘린다. 무엇보다 "어디부터 위험한가" 의
  임계값을 근거 없이 정하게 된다.
- **④-A 하드 점유는 무시하고 닫는다** — 점유는 "입력 차단" 이지 "삭제 금지" 가 아니라는
  독해. 그러나 입력을 막아 둔 이유가 "그 surface 의 주인은 지금 holder 다" 라면, 같은
  이유가 파괴에는 더 강하게 적용된다. 입력은 막고 삭제는 허용하는 조합은 설명할 수 없다.
- **④-B 점유된 surface 만 남기고 나머지를 닫는다(부분 close)** — "요청을 최대한 수행한다"
  는 매력이 있으나 `workspace.close` 의 계약("워크스페이스를 통째로 닫는다")이 조건부로
  변한다. 워크스페이스가 남을지 사라질지가 원격 세션의 상태에 따라 갈리면 에이전트가
  결과를 예측할 수 없다. 부분 정리는 이미 `surface.close` 로 가능하다.
- **③-B 에이전트 경로도 되돌리기 스택에 넣어 되살릴 수 있게 한다** — 비가역성 자체가
  사라지므로 매력적이지만, 에이전트 행동이 사용자의 "닫은 항목" 히스토리를 오염시킨다.
  원칙 1 과 정면 충돌이라 채택 불가.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- mirror 워크스페이스를 IPC 로 거두어야 할 실제 요구가 생긴다 — 그때는 ②-A(전용 teardown
  라우팅)를 다시 본다. 거절 메시지로 인한 우회 문의가 반복되는 것도 같은 신호다.
- 파괴 규모에 비례한 게이트(`force` 플래그 등)의 선례가 프로젝트 다른 곳에 생긴다 — 그러면
  ③-A 가 "선례 없음" 이라는 기각 사유를 잃는다.
- 에이전트가 닫은 것을 되살려야 한다는 요구가 생긴다 — 사용자 히스토리와 **분리된** 별도
  스택(에이전트 전용 되돌리기)이라면 원칙 1 과 충돌하지 않으므로 ③-B 와 다른 선택지가 된다.
- 점유 tier 모델이 바뀌어 `is_hard_occupied` 가 hard 전용이 아니게 된다 — ④가 그 술어의
  hard 전용 계약(ADR-0040)에 기대므로 함께 본다.
- `window.close` 가 마지막 창 거절 정책을 바꾼다 — ① 이 "창 종료는 `window.close` 로" 라는
  위임에 기대고 있으므로 함께 본다.

## References

- [`docs/identity.md`](../identity.md) — 원칙 1(사용자/에이전트 행동 분리) · 원칙 2(IPC+CLI 양면) · 원칙 3(포커스 독립성)
- [ADR-0113](0113-close-preserves-the-focused-target.md) — close 가 포커스된 대상을 보존한다
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 점유 tier(soft/hard)와 `is_hard_occupied` 의 hard 전용 계약
- [`docs/design/policies/focus.md`](../design/policies/focus.md) — 자기 자신 닫기 보호 · 삭제 시 활성 포인터 보정
- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) — mirror 워크스페이스와 attach 점유 모델
- [`docs/features/work-area/index.md`](../features/work-area/index.md) — 작업 영역 계층과 에이전트 조작
