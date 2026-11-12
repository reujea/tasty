# 상태 모델 (State model)

Window·Workspace·Pane·Tab·Surface 다섯 계층이 가지는 상태를 축별로 정리한다. 이 폴더는 상태가 어디에 저장되고 누가 바꾸는지, 상태가 어떻게 전이하고 함께 존재할 수 있는지를 설명한다. 포함 관계와 Window/View/Engine의 구분은 [구조 계층](../hierarchy.md), 구조 트리의 동작은 [작업 영역](../../features/work-area/index.md)에 있다. 이 문서들은 그 정본의 규칙을 복사하지 않고 해당 위치로 연결한다.

| 계층 | 문서 | 대표 축 |
|---|---|---|
| Surface | [surface](surface.md) | 구조 leaf와 kind, 활성화 단계, 실행 객체, busy, attention, soft·hard 점유, 자식 상태 |
| Tab | [tab](tab.md) | pane별 선택, 탭 안 선택 surface, 제목 결정, busy·attention 집계 |
| Pane | [pane](pane.md) | workspace별 포커스 pane, pane별 선택 탭, 탭바 스크롤 |
| Workspace | [workspace](workspace.md) | View별 선택, 로컬·mirror·저장된 원격 행, 연결 상태, workspace 점유, 집계 |
| Window | [window](window.md) | OS 창 상태, View 종류와 모달, 포커스, engine 관계(창·parked·pending·retiring) |

## 읽는 방법

계층 문서는 상태 축마다 다음 항목을 적는다.

- **값**: 코드에 있는 enum 값이나 필드. 코드에 없는 상태 이름은 만들지 않는다.
- **소유자**: 그 값을 보관하는 타입. 아래 "상태의 소유자"의 다섯 자리 중 하나다.
- **저장**: 구조 journal에 남는지, 프로세스 메모리에만 있는지, 별도 파일에 저장하는지.
- **관측·파생**: 외부(PTY·OS·원격)를 관측한 값인지, 다른 값에서 계산한 값인지.
- **변경 주체**: 로컬 사용자, 에이전트(IPC·CLI), 원격 사용자, 시스템(폴링·연결 감시) 가운데 누가 바꾸는지. 주체의 정의는 [주체](../actors.md)에 있다.

"N/A"는 그 계층에 축이 적용되지 않는다는 뜻이다. "관측 불가"는 축은 적용되지만 지금 값을 알 수 없다는 뜻이다(예: 원격 연결이 끊긴 동안의 원격 자식 상태). 두 경우를 구분한다.

## 상태의 소유자

같은 객체의 상태라도 소유자가 다르다. 한 소유자의 값을 다른 소유자의 값으로 추정하지 않는다.

| 소유자 | 무엇을 보관하나 | 저장 | 코드 |
|---|---|---|---|
| 구조 journal과 `CoreState` | Workspace·Pane·Tab·Surface 트리, 분할, surface의 kind와 활성화 generation | journal에 저장. `CoreState`는 확정 batch의 읽기 projection | `crates/tasty-core/src/state.rs` |
| `LiveDomainState` | busy, attention, 점유, 제목 관측값, 알림 | 메모리 | `src/core/live.rs` |
| `EngineRuntime` | surface 실행 객체, PTY, kind 레지스트리, 자식 터미널 목록 | 메모리. 자식 목록만 파일에 저장 | `src/runtime/engine_session.rs` |
| View의 `NavigationState` | 선택한 workspace·pane·탭·surface, 분할 노드 선택 힌트 | 메모리. View 복원 자료로 보존할 수 있다 | `src/state/navigation.rs` |
| App의 `ViewRegistry`·`EngineRegistry` | View 목록, 활성 모달, 포커스 View, View와 engine의 관계 | 메모리 | `src/view/mod.rs`, `src/app/engine_registry.rs` |

headless는 App과 EngineSession을 실행하지만 로컬 View를 만들지 않는다. 그래서 View가 소유하는 축(선택, 포커스, 탭바 스크롤, 창 상태)은 headless에서 N/A다. 명령 기본 문맥이 같은 `NavigationState` 타입을 쓰지만 로컬 사용자의 선택이 아니다([포커스 정책](../../design/policies/focus.md#삭제로-인한-인덱스-이동에서도-포커스-대상은-보존된다)).

## 계층 사이의 관계

상태는 세 방향으로 계층을 건넌다.

- **아래에서 위로 집계**: busy와 attention은 surface에만 저장된다. Tab·Workspace의 표시는 포함한 surface 집합에서 매번 계산한다. Pane과 Window에는 집계 표시가 없다.
- **위에서 아래로 제한**: workspace를 hard 점유하면 그 workspace의 터미널 surface에도 surface lock이 등록되고 나머지 멤버는 내용 가림 대상이 된다. 활성 모달은 다른 모든 View의 입력을 막는다. 상위 제한은 하위 객체의 상태 값을 바꾸지 않고 하위 조작의 허용 여부만 바꾼다.
- **선택의 범위**: 선택은 범위가 정해져 있다. 선택 workspace는 View마다, 포커스 pane은 workspace마다, 선택 탭은 pane마다, 선택 surface는 탭마다 하나다. 한 범위의 선택을 바꿔도 다른 범위의 선택은 바뀌지 않는다.

에이전트는 ID로 대상을 지정하며 위의 선택 축을 바꾸지 않는다([포커스 정책](../../design/policies/focus.md#cliipc-포커스-독립-원칙)).

## 원격 생성 대기 그림

attach mirror에서 사용자가 탭·pane·surface 생성을 요청하면 응답이 오기 전까지 View가 생성될 자리를 미리 그린다. 이 그림은 surface·tab·pane 계층에 공통으로 적용된다.

- 그림은 `src/state/attach_pending.rs`의 draft다. 종류는 `Creation`의 `Tab`·`Surface`·`Pane` 셋이고 `PendingId`로 식별한다. `CoreState`·journal·surface 저장소에는 임시 객체를 넣지 않는다.
- 그래서 draft는 도메인 ID가 없고 `tasty list` 조회에 나오지 않는다. 복원용 `JournalPlaceholder`([empty surface](../../surfaces/empty/index.md))와도 다르다. 복원 자리는 확정된 surface ID를 가진 실행 객체다.
- draft 항목은 receipt가 끝났거나 연결 epoch가 끝났거나 기준 surface(workspace 생성이면 workspace)가 사라지면 살아 있지 않은 것으로 판정되어 버려진다(`Entry::live`).
- 확정된 객체는 서버의 구조 결과와 구조 Delta가 mirror 트리에 적용될 때 생긴다. 요청과 생성 객체를 대응하는 규칙, 모호할 때의 처리, 포커스 결정은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#입력-처리-확인과-view의-선행-배치)에 있다.
- 응답 전에 연결 epoch가 끝나면 draft만 사라진다. 이것은 객체가 닫힌 것이 아니다. 서버에서 생성이 실제로 일어났는지는 다음 연결의 구조 스냅샷이 보여 준다.

## 관련

- [구조 계층](../hierarchy.md) — 포함 관계, Window·View·Engine
- [작업 영역](../../features/work-area/index.md) — 구조 트리, 분할, 생성·닫기
- [포커스 정책](../../design/policies/focus.md) — 선택 소유, 삭제·재정렬 보정
- [주체](../actors.md) — 점유 모델
- [Surface 종류](../../surfaces/index.md) — kind별 동작
