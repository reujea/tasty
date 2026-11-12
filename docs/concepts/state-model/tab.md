# Tab 상태 모델

Tab은 구조 트리에서 하위 레이아웃 하나와 이름만 가진다. 선택·busy·attention은 Tab에 저장되지 않는다. 선택은 View가, busy·attention은 surface가 가지며 탭바가 그 값을 모아 표시한다([상태 모델](index.md#계층-사이의-관계)).

## 상태 축

| 축 | 값 | 소유자 | 저장 | 관측·파생 | 변경 주체 |
|---|---|---|---|---|---|
| 구조 | 있음·없음, 소속 pane과 순서, 하위 레이아웃 | `CoreState`의 `Tab`(`crates/tasty-model/src/tab.rs`) | journal | 확정 구조 | 생성·이동·닫기·복원 명령 |
| 명시 이름 | 없음·있음(`explicit_name`) | `Tab` | journal | — | 이름 변경, `tab.create --name` |
| 표시 이름 | 문자열 | 계산 | 저장하지 않음 | 파생 | 명시 이름, 선택 surface의 제목 관측값 |
| pane 안 선택 | 이 탭이 pane의 선택 탭인가 | View의 `NavigationState.selected_tabs`(pane ID별) | 메모리 | View 선택 | 로컬 사용자 |
| 탭 안 선택 surface | surface ID | View의 `NavigationState.surfaces`(tab ID별) | 메모리 | View 선택 | 로컬 사용자 |
| busy 표시 | 점 표시·없음 | 계산 | 저장하지 않음 | 탭의 모든 surface busy의 OR | surface busy 변화 |
| attention 표시 | 없음, `Completion`, `NeedsInput` | 계산 | 저장하지 않음 | 탭의 모든 surface attention 중 가장 높은 kind | surface attention 변화 |
| 점유 | N/A | — | — | — | 점유는 surface·workspace 단위다. 탭 닫기는 탭 안 surface의 hard 점유를 검사한다 |

### 활성·표시·포커스의 구분

"활성 탭"이라는 말은 아래 네 가지 중 무엇인지 정해 쓴다.

| 이름 | 범위 | 판정 |
|---|---|---|
| pane의 선택 탭 | pane마다 하나 | `NavigationState`의 `tab_id`. 선택 기록이 없으면 첫 탭, 선택한 탭이 사라졌으면 직전 순서의 다음 생존 탭, 없으면 이전 생존 탭([포커스 정책](../../design/policies/focus.md#삭제로-인한-인덱스-이동에서도-포커스-대상은-보존된다)) |
| 화면에 보이는 탭 | 보이는 workspace의 pane마다 하나 | 선택 탭이 그려진다. 여러 pane이면 여러 탭이 동시에 보인다 |
| 입력 포커스 탭 | View마다 하나 | 포커스 pane의 선택 탭 |
| OS 포커스 | 창마다 | 창이 OS 포커스를 가졌는가([window](window.md#포커스와-모달)) |

선택은 pane 범위다. 한 pane의 탭 전환은 다른 pane의 선택 탭과 pane 배치를 바꾸지 않는다. 표시 탭의 하위 레이아웃 안에서 입력을 받는 surface는 탭 안 선택 surface다(`src/state/accessors.rs`의 포커스 surface 계산). 원격 생성 대기 탭이 선택된 동안 포커스 surface는 없다.

### 제목

표시 이름의 우선순위(명시 이름, OSC 제목, cwd 이름, 자동 이름)와 "그 View가 탭에서 선택한 surface의 관측값만 쓴다"는 규칙은 [작업 영역](../../features/work-area/index.md#tab--하위-레이아웃-탭-종속)이 정본이다. 제목은 이름 결정 규칙이며 busy·attention과 같은 상태 열거형이 아니다. 같은 탭도 View마다 선택 surface가 다르면 제목이 다를 수 있다.

### busy와 attention 집계

탭바는 탭마다 `Tab::all_surface_ids`로 하위 레이아웃의 모든 surface를 모은다(`src/adapters/ui/tab_bar.rs`).

- busy는 그 집합의 OR이다(`compute_tab_is_busy`). 선택되지 않은 surface도 포함한다. 판정은 [busy 표시 정책](../../design/policies/busy-indicator.md#집계--or)을 따른다.
- attention은 그 집합에서 가장 높은 kind다(`attention_dominant_kind`). `NeedsInput`이 `Completion`보다 높다.
- 두 값은 따로 그린다. 제목색은 attention이 정하고, busy는 별도 점이다. 제목색의 우선순위는 `NeedsInput`, `Completion`, 활성 탭, hover, 기본 순서다(`src/adapters/ui/tab_bar/tab.rs`의 `tab_fill_and_ink`). 표시 규칙의 정본은 [Surface Highlight](../../features/surface-highlight/index.md#표시)다.
- 원격 생성 대기 탭은 busy·attention이 없다.

## 전이

구조 변경은 journal 명령으로 확정되고, 선택 보정은 App이 결과 ID로 `NavigationState`를 맞춘다. 보정 규칙(살아 있는 선택 ID 유지, 사라진 선택은 다음·이전 생존 ID)의 정본은 [포커스 정책](../../design/policies/focus.md#삭제로-인한-인덱스-이동에서도-포커스-대상은-보존된다)이다.

| 이전 조건 | 사건·조작 | 이후 | 실패·거절 | 다른 계층 영향 |
|---|---|---|---|---|
| pane 있음 | 사용자 탭 생성 | 새 탭이 생기고 선택된다 | — | 포커스 pane 불변 |
| pane 있음 | 에이전트 `tab.create` | 새 탭이 생기고 선택은 그대로다 | — | 없음([포커스 정책](../../design/policies/focus.md#에이전트가-만든-탭과-선택)) |
| 탭 둘 이상 | 사용자 탭 선택 | pane의 선택 탭 변경 | — | 다른 pane 불변. 새로 보이는 복원 자리는 활성화된다([empty](../../surfaces/empty/index.md#생성갱신종료)) |
| 탭 둘 이상 | 재정렬·다른 pane으로 이동 | 구조 순서·소속 변경 | — | 선택 ID 유지([포커스 정책](../../design/policies/focus.md#재정렬에서도-포커스-대상은-보존된다)) |
| pane에 탭 둘 이상 | 탭 닫기 | 탭과 그 surface 제거 | hard 점유 surface가 있으면 거절. mirror 탭은 서버로 전달 | 닫힌 탭이 선택이었으면 생존 탭으로 보정. 닫은 항목 기록에 남는다 |
| pane에 탭 하나 | 탭 닫기 | 변화 없음 | 구조 명령이 거절한다(`close_facts`, `crates/tasty-core/src/command/retirement.rs`) | pane은 닫히지 않는다 |
| 탭에 surface 하나 | 그 surface 닫기 | 탭 닫기로 올라간다. 탭이 pane의 마지막이면 pane, workspace의 마지막 pane이면 workspace까지 올라간다 | 각 단계의 거절 조건은 해당 계층 | [pane](pane.md#전이) |
| 닫은 항목 기록 있음 | 닫은 항목 복원 | 새 ID로 탭 생성 | 대상 pane이 없으면 실패하고 기록에 돌려놓지 않는다 | [닫힌 항목 복원](../../features/closed-tab-restore/index.md) |
| surface busy·attention 변화 | 폴링, 발생, 해제 | 다음 그리기에서 탭 표시 변경 | — | 선택·순서 불변 |

탭 닫기의 허용 여부는 위 구조 명령의 경계(탭 수)와 닫기 진입점의 hard 점유 검사([포커스 정책](../../design/policies/focus.md#hard-점유-대상의-닫기-보호))로 정한다. 마지막 탭을 닫는 사용자 조작이 pane을 닫으려면 surface 닫기나 pane 닫기를 써야 한다.

## 가능한 조합과 적용되지 않는 조합

| 조합 | 판정 | 이유 |
|---|---|---|
| 선택되지 않은 탭 + busy 점 | 가능 | 집계는 선택과 무관하다 |
| 선택되지 않은 surface의 attention이 탭 제목색을 정함 | 가능 | 탭의 모든 surface를 모은다 |
| 두 pane에 각각 선택 탭 | 가능 | 선택은 pane마다 하나다 |
| 한 pane에 선택 탭 둘 | 불가 | `selected_tabs`는 pane당 하나다 |
| 탭이 0개인 pane | 불가 | 마지막 탭 닫기는 거절되고 surface 닫기는 pane 닫기로 올라간다 |
| 탭 자체의 점유·busy 저장 값 | N/A | 저장하지 않는다 |
| headless에서 선택 탭 | N/A | 로컬 View가 없다. 명령 기본 문맥만 있다 |

## 문서 예시

pane P에 탭 A·B가 있고 A가 선택돼 있다. A에는 surface a1·a2가 있고 a1이 선택돼 있다. B에는 busy인 surface b1이 있다.

1. B는 선택되지 않았지만 탭바에 busy 점이 보인다. A를 계속 보고 있어도 B의 busy는 사라지지 않는다. b1의 출력이 멈춰 busy 폴링이 idle로 바꿀 때 사라진다.
2. 창이 OS 포커스를 잃은 동안 a1에 `Completion`, a2에 `NeedsInput`이 생긴다. 포커스 해제는 창이 OS 포커스를 가져야 일어나므로 둘 다 남는다. A의 제목색은 높은 kind인 `NeedsInput`이다.
3. 이 상태에서 B를 선택하거나 탭 순서를 바꿔도 a1·a2의 attention은 바뀌지 않는다. 선택·재정렬은 구조와 View 선택의 사건이고 attention 해제가 아니다. A가 선택에서 빠지면 제목색 순위에서 "활성 탭"만 빠지고 `NeedsInput` 색은 그대로다.
4. A를 다시 선택하고 창이 OS 포커스를 되찾으면 다음 렌더가 포커스 surface 하나를 확인 처리한다(`src/view/main/redraw.rs`의 `FocusObserved`). a1이 포커스 surface이므로 a1의 `Completion`만 지워진다. a2의 `NeedsInput`은 남아 A의 제목색은 계속 `NeedsInput`이다. a2로 포커스를 옮겨야 지워진다. 전체화면 무대가 열려 있으면 이 확인을 하지 않는다.

원격 생성 대기 탭은 위 표의 탭이 아니다. 응답 전에는 탭바 끝에 대기 탭이 보이지만 도메인 ID가 없고 `tasty list tabs`에 나오지 않는다. 구조 응답이 적용되면 실제 탭이 생기고, 그 전에 연결 epoch가 끝나면 대기 탭만 사라진다([원격 생성 대기 그림](index.md#원격-생성-대기-그림)).

## 관련

- [상태 모델](index.md)
- [surface 상태 모델](surface.md)
- [pane 상태 모델](pane.md)
- [작업 영역](../../features/work-area/index.md)
- [포커스 정책](../../design/policies/focus.md)
