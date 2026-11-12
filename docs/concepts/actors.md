# 주체 (Actors)

Tasty는 한 인스턴스를 여러 주체가 동시에 사용하도록 설계한다. 주체마다 조작 방법과 권한이 다르다([사용자 통제와 데이터 원칙](../principles/control-and-data.md#사용자에이전트원격-사용자)).

## 로컬 사용자 (Local user)

로컬 사용자는 이 머신에서 키보드·마우스·OS 입력으로 Tasty GUI를 직접 쓰는 사람이다. 포커스를 정하고 점유되지 않은 surface·workspace를 자유롭게 조작한다. 한 인스턴스에 보통 한 명이며, GUI에서 다른 주체의 점유를 강제로 해제할 수 있다.

## AI Agent (에이전트)

에이전트는 IPC·CLI로 대상을 ID로 지정해 자기 작업을 수행한다. 여러 에이전트가 동시에 동작하더라도 사용자의 포커스·닫은 항목 히스토리·선택을 바꾸지 않는다. `surface.send`·`surface.read`처럼 점유 없이 요청할 수 있으며 필요하면 soft 또는 hard 점유를 사용한다. `terminal` 명령이 만든 child-terminal의 soft 점유가 한 예다([Attach 메커니즘](../dev-guide/attach-behavior.md)).

## 원격 접속 사용자 (Remote user)

원격 사용자는 SSH를 통해 접속하는 사람이며 에이전트와 같은 동작 권한을 가진다. SSH 셸이나 터널로 서버의 CLI·IPC를 쓸 수 있고 로컬 포커스는 바꿀 수 없다. attach 스트림으로 화면을 조작할 때는 surface나 workspace를 hard 점유하고 점유한 대상만 조작한다.

tasty 는 자체 원격 프로토콜이 없고 SSH 에 위임한다 — attach 동작은 [`../features/remote-attach/`](../features/remote-attach/index.md), 메커니즘은 [`../dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md).

## 점유 (Occupation) 모델

점유는 원격 사용자나 에이전트가 surface·workspace를 사용 중임을 계속 표시하는 관계다. 일회성 `surface.send`·`surface.read` 요청과 구분한다. soft는 green, hard는 peach 테두리로 표시한다. 선택 이유와 표시 규칙은 [Attach 메커니즘](../dev-guide/attach-behavior.md)에 있다.

### 약한 점유 (soft)

- soft 점유는 사용 중임을 알리는 표시다. 대상이 닫히거나 상태가 바뀔 수 있음을 알리지만 입력을 제한하지 않는다. 로컬 사용자는 평소처럼 조작할 수 있다.
- 현 소비자: `terminal` 명령이 spawn 한 **child-terminal**(주체 = 그 child 를 spawn 한 parent surface) → [`../features/child-terminal/`](../features/child-terminal/index.md).

### 강한 점유 (hard)

- **배타 + readonly.** 점유한 주체만 조작하고, 그동안 **로컬 사용자·다른 주체는 그 대상에 대해 readonly** — 무슨 일이 일어나는지 *볼 수만* 있다.
- **원격 attach 가 이 계층의 사례** 다(attach 스트림으로 화면을 조작하는 원격 사용자는 hard 점유 안에서만 조작한다. SSH 셸·터널로 CLI·IPC 를 쓸 때는 에이전트와 같은 조건이다) → [`../features/remote-attach/`](../features/remote-attach/index.md), 메커니즘 [`../dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md).

### 공통 규칙

- **해제 권한**: 어느 계층이든 점유는 **점유 주체 본인(self-release) 또는 로컬 사용자의 GUI 강제 끊기(force-detach)** 로 끊는다. 연결이 끊기면 그 연결의 점유도 풀린다. 끊으면 대상은 다시 **일반 surface/workspace 로 복귀** 한다. release 에는 다른 주체의 점유를 끊는 IPC·CLI 가 없고, debug 빌드만 사용자 조작 재현용 `debug.attach.*` 를 둔다([Attach 메커니즘](../dev-guide/attach-behavior.md#force-detach)).
- **한 대상의 점유자(현재 코드 동작)**: soft와 hard는 `OccupancyRegistry`(`src/core/attach.rs`)의 서로 다른 표에 기록된다.
  - 같은 계층 안에서는 한 대상의 점유자가 하나다. soft는 다른 parent의 soft가 있으면 `acquire_soft`가 거절하고, surface hard는 살아 있는 다른 holder가 있으면 `acquire`가 거절한다. workspace hard는 `acquire_workspace`가 같은 방식으로 거절한다.
  - 계층 사이의 검사는 진입점마다 다르다. soft를 거는 `terminal.adopt`는 대상이 hard 점유면 거절한다(`src/adapters/ipc/handler/terminal.rs`의 `handle_adopt`). 원격 attach는 soft를 보지 않고 hard를 건다(`src/adapters/ipc/handler/attach.rs`). 그래서 soft가 걸린 surface에 hard가 올라가 둘이 함께 있을 수 있다.
  - 둘이 함께 있으면 대표 점유 조회 `occupancy_of`는 hard를 돌려주고, 입력 제한은 hard만 본다(`is_hard_occupied`).
  - 둘이 함께 있을 때의 정리와 해제는 soft를 남긴다. 사용자 포커스 정리 `reconcile_soft_occupancy_on_focus`는 대표 점유가 hard면 아무것도 하지 않는다. 로컬 사용자 강제 해제 `release_occupancy`는 hard를 풀면 거기서 끝난다(`src/core/state/soft_occupancy.rs`). 연결 종료 해제 `release_all_for_client`는 그 client의 hard lock만 지운다.
  - 점유 상태는 soft/hard 외 상태를 담는 별도 상태 모델로 다시 설계할 예정이다.
- **다중성**: 원격 사용자는 여럿이 동시에 접속할 수 있고, 한 주체가 여러 대상을 동시에 점유할 수 있다.

## 정리

| | 로컬 사용자 | AI Agent | 원격 접속 사용자 |
|---|---|---|---|
| 부류 | 사람 | AI | 사람 |
| 동작 경로 | 직접 GUI 입력 | IPC / CLI | SSH 셸·터널의 CLI·IPC, attach 연결 |
| 분류 성격 | 사용자 행동 | 에이전트 행동 | **에이전트와 같은 권한** |
| 점유 | 불필요 | **선택** (soft/hard) | CLI·IPC 는 에이전트와 같음(선택). attach 스트림 조작은 **필수** (강한 점유 안에서만) |
| 포커스 | 주인 | 안 건드림 | 로컬 포커스 비주인 |
| 타 점유 강제해제(force-detach) | **있음** (GUI 강제 끊기) | 없음 — release 에 공개 IPC·CLI 없음 (자기 점유 self-release 만) | 없음 — 에이전트와 같음 (자기 점유 self-release 만) |
| 동시 수 | 보통 1 | 0..N | 0..N |

API를 설계할 때는 로컬 직접 입력과 에이전트·원격 요청을 구분한다. [사용자와 에이전트 행동의 분리](../principles/control-and-data.md#사용자에이전트원격-사용자)를 따른다.