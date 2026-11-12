# Pane 상태 모델

Pane은 구조 트리에서 ID와 탭 목록만 가진다(`crates/tasty-model/src/pane.rs`). 포커스, 선택 탭, 탭바 스크롤은 View가 가진다. Pane 자체의 busy·attention·점유 값은 없다. 탭바가 pane마다 그리는 표시는 탭 단위 집계다([tab](tab.md#busy와-attention-집계)).

## 상태 축

| 축 | 값 | 소유자 | 저장 | 관측·파생 | 변경 주체 |
|---|---|---|---|---|---|
| 구조 | 있음·없음, 상위 레이아웃의 위치(분할 방향·비율), 탭 목록 | `CoreState`의 `Pane`과 workspace의 `PaneNode` | journal | 확정 구조 | 분할·닫기·이동·비율 변경 명령 |
| 포커스 pane | workspace마다 pane ID 하나 | View의 `NavigationState.panes`(workspace ID별) | 메모리 | View 선택 | 로컬 사용자 |
| 선택 탭 | pane마다 tab ID 하나 | View의 `NavigationState.selected_tabs`(pane ID별) | 메모리 | View 선택 | 로컬 사용자 |
| 탭바 스크롤 | 논리 px | View의 `MainViewState.tab_bar_scroll`(pane ID별) | 메모리 | View 표시 | 로컬 사용자, 활성 탭 추종 보정 |
| busy·attention | N/A | — | — | — | pane 단위 값과 소비처가 없다 |
| 점유 | N/A | — | — | — | 점유는 surface·workspace 단위다. pane 닫기는 pane 안 모든 surface의 hard 점유를 검사한다 |

포커스 pane이 없거나 사라졌으면 workspace의 첫 pane을 쓴다(`NavigationState::pane_id`). 선택 탭도 같은 방식으로 첫 탭으로 내려간다. 활성 탭 추종 스크롤 규칙은 [작업 영역](../../features/work-area/index.md#pane--상위-레이아웃-탭-무관)에 있다.

### 표시와 포커스

보이는 workspace의 모든 pane은 각자의 선택 탭을 동시에 그린다. 그중 입력을 받는 것은 포커스 pane의 선택 탭 안에서 선택된 surface 하나다. 그 surface도 창이 OS 포커스를 가지고 활성 모달이 없을 때만 입력을 받는다([포커스 정책](../../design/policies/focus.md#view-내부-포커스)). 따라서 "보인다"와 "입력 포커스"는 다른 축이다.

pane 포커스는 탭바 클릭, 영역 클릭, 방향 이동 단축키처럼 사용자 조작으로 바뀐다. 에이전트의 분할·생성은 포커스를 옮기지 않는다([작업 영역](../../features/work-area/index.md#포커스-정책)).

## 전이

| 이전 조건 | 사건·조작 | 이후 | 실패·거절 | 다른 계층 영향 |
|---|---|---|---|---|
| pane P | pane 분할(`split --level pane`) | P 옆에 새 pane이 생기고 새 탭 하나를 가진다 | 공간 부족이면 거절([작업 영역](../../features/work-area/index.md#분할-비율과-탐색기-칸-하한)) | 사용자 분할이면 새 pane으로 포커스, 에이전트 분할이면 포커스 유지 |
| pane P | P의 탭 전환 | P의 선택 탭 변경 | — | 다른 pane의 선택, 상위 레이아웃, 포커스 pane 불변 |
| pane P | P의 탭 재정렬 | 순서 변경 | — | P의 선택 탭 ID 유지 |
| workspace에 pane 둘 이상 | pane 닫기 | pane과 그 안의 모든 탭·surface 제거. 탭을 다른 pane으로 옮기지 않는다(`drop_pane`, `crates/tasty-core/src/evolve.rs`) | hard 점유 surface가 있으면 거절. mirror pane은 서버로 전달 | 닫힌 pane이 포커스였으면 첫 생존 pane으로 보정. 그 pane의 선택 탭·탭바 스크롤·분할 힌트는 회수 |
| workspace에 pane 하나 | pane 닫기 | 변화 없음 | 구조 명령이 거절한다(`close_facts`) | — |
| 마지막 탭의 마지막 surface | surface 닫기 | pane 닫기로 올라간다. workspace의 마지막 pane이면 workspace 닫기로 올라간다 | 각 단계의 거절 조건 | workspace 구조 변경 |
| 분할선 | 비율 변경 | 상위 레이아웃 비율 변경 | 0.1~0.9 밖이면 거절 | 탭·선택 불변 |

pane을 합치는 구조 명령은 없다. pane을 줄이는 방법은 닫기뿐이며 닫힌 pane의 탭은 함께 사라진다.

## 가능한 조합과 적용되지 않는 조합

| 조합 | 판정 | 이유 |
|---|---|---|
| 두 pane이 각자 선택 탭을 동시에 표시 | 가능 | 선택은 pane마다 있다 |
| 포커스 pane 둘 | 불가 | `panes`는 workspace당 하나다 |
| View마다 다른 포커스 pane | 가능 | 선택은 View가 소유한다 |
| 탭이 0개인 pane | 불가 | [tab](tab.md#가능한-조합과-적용되지-않는-조합) |
| pane이 0개인 workspace | 불가 | 마지막 pane 닫기는 거절되고 surface 닫기는 workspace 닫기로 올라간다 |
| pane busy·attention·점유 저장 값 | N/A | 소비처가 없다 |

## 문서 예시

workspace W가 좌우 pane P·Q로 나뉘고 각각 탭 둘을 가진다. 두 pane의 선택 탭은 모두 보이고 P가 포커스 pane이다.

1. P의 탭을 전환하면 P의 선택 탭만 바뀐다. Q의 선택 탭과 P·Q의 좌우 배치는 그대로다. 입력은 여전히 P로 간다.
2. P의 탭을 재정렬하면 순서만 바뀌고 P의 선택 탭 ID는 그대로다.
3. Q를 닫으면 Q와 Q의 두 탭과 그 surface가 사라지고 P가 W 전체를 차지한다. 포커스 pane은 P였으므로 바뀌지 않는다. Q 안에 hard 점유 surface가 있었다면 닫기는 거절된다.
4. Q의 선택 탭이 보이는 것은 Q에 입력이 간다는 뜻이 아니다. Q를 클릭해야 포커스 pane이 Q가 된다.

원격 생성 대기 pane은 응답 전에 분할 자리로 그려지지만 도메인 ID가 없다. 응답이 적용되면 실제 pane이 생기고, 그 전에 연결 epoch가 끝나면 대기 그림만 사라진다([원격 생성 대기 그림](index.md#원격-생성-대기-그림)).

## 관련

- [상태 모델](index.md)
- [tab 상태 모델](tab.md)
- [작업 영역](../../features/work-area/index.md)
- [포커스 정책](../../design/policies/focus.md)
