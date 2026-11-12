# Window 상태 모델

Window는 winit OS 창이고 View는 Tasty가 그 창에 두는 표현이다(1 View : 1 Window, [구조 계층](../hierarchy.md#window-과-view)). 이 문서의 상태는 OS 창의 상태, View의 공통 상태, 앱 수준의 모달·포커스, View와 engine의 관계로 나뉜다. workspace를 표시하는 View는 `MainView`뿐이다. headless는 로컬 View를 만들지 않으므로 이 문서의 축은 모두 N/A다.

## 상태 축

| 축 | 값 | 소유자 | 저장 | 관측·파생 | 변경 주체 |
|---|---|---|---|---|---|
| OS 창 | 존재, 최소화, 최대화, OS 전체화면 | winit `Window`(`ViewBase.winit`, `src/view/base.rs`) | 저장하지 않는다. 필요할 때 winit에 묻는다 | OS 관측 | 로컬 사용자, OS, 창 장식 조작 |
| View 종류 | `MainView`, 모달(`SettingsView`·`PluginsView`·`QuitView`), modeless `PresetView` | App의 `ViewRegistry`(`src/view/mod.rs`) | 메모리 | — | 창 생성·닫기 |
| View 공통 | `focused`, `dirty`, `repaint`, `modifiers`, `close_requested`, View incarnation `identity` | `ViewState`(`src/view/state.rs`) | 메모리 | `focused`·`modifiers`는 OS 이벤트 관측 | OS 이벤트, 렌더, 닫기 요청 |
| 활성 모달 | 없음, 모달 하나 | `ViewRegistry.active_modal` | 메모리 | — | 모달 열기·닫기 |
| 포커스 View | 없음, MainView 하나 | `ViewRegistry.focused_view_id` | 메모리 | OS 포커스 이벤트에서 파생 | OS 포커스, 창 등록·닫기 |
| engine 관계 | 창에 묶임, parked, pending, retiring(`Discard`·`Preserve`·`Releasing`) | App의 `EngineRegistry`(`src/app/engine_registry.rs`) | 메모리. 레이아웃 슬롯은 journal에 있다 | — | 창 생성·닫기, 최소화, 회수 완료 |
| 전체화면 무대 | 없음, 무대 하나 | `MainViewState.fullscreen_stage`와 진입 전 OS 창 모드 기록(`MainView.stage_saved_window_mode`) | 메모리 | — | 로컬 사용자 |
| busy·attention·점유 | N/A | — | — | — | 창에는 집계 표시가 없다. 창이 없는 engine의 busy·attention·원격 세션도 engine에 남는다 |

### OS 창 상태

최소화·최대화·OS 전체화면은 `ViewState`에 필드가 없다. 코드는 필요할 때 `is_minimized`·`is_maximized`·`fullscreen`으로 winit에 묻는다. 플랫폼 차이는 다음과 같다.

- Windows에서 최소화 중인 창은 작업 표시줄 아이콘 크기를 client 크기로 보고하므로 표면과 터미널 격자 크기를 유지한다([작업 영역](../../features/work-area/index.md#분할-비율과-탐색기-칸-하한)).
- 최소화 요청의 결과가 다르다. macOS는 모든 MainView를 park한다. Windows와 Linux는 트레이가 있으면 창을 숨기고 없으면 OS 최소화를 한다(`handle_minimize`, `src/app/event_handler.rs`). 트레이 동작은 [시스템 트레이](../../design/policies/system-tray.md)에 있다.
- 최대화·전체화면 창은 Tasty의 가장자리 리사이즈 밴드를 쓰지 않는다. macOS는 OS 장식이 리사이즈를 맡는다.

### 포커스와 모달

| 이름 | 범위 | 소유자 |
|---|---|---|
| OS 포커스 | 창마다 | `ViewState.focused` |
| 포커스 View | 앱에 하나, MainView만 | `ViewRegistry.focused_view_id` |
| 활성 모달 | 앱에 하나 | `ViewRegistry.active_modal` |
| View 안의 선택 | View마다 | `MainViewState`의 `NavigationState`([workspace](workspace.md#선택과-active)) |

활성 모달이 있으면 모달이 아닌 View는 크기·다시 그리기·배율·modifier·포커스 이벤트만 받고 입력을 버린다. 모달 차단은 앱 수준 게이트이며 OS 창을 비활성화하지 않는다. 정본은 [포커스 정책](../../design/policies/focus.md#modal-포커스-차단)이다. 포커스 View는 MainView가 OS 포커스를 받을 때 바뀐다. 모달은 포커스 View가 되지 않으며, 모달이 열린 동안 MainView가 포커스를 받으면 모달에 포커스를 다시 요청한다.

세 축은 서로 독립이다. 모달 차단은 workspace 선택을 바꾸지 않고, OS 포커스를 잃어도 선택은 남는다. OS 포커스는 attention의 포커스 확인 해제 조건이다([surface](surface.md#attention)).

### 사용자 창과 에이전트 창

| 출처 | 생성 속성 | 등록 뒤 포커스 View |
|---|---|---|
| 사용자 | 활성 창으로 만든다 | 새 창 |
| 에이전트(IPC `window.create`) | 비활성·숨김으로 만든 뒤 사용자 창 뒤에 표시한다. 실패하면 일반 표시로 대체한다 | 기존 포커스 View를 유지한다. 기존 값이 없을 때만 새 창 |

판정은 `focus_after_register`·`origin_window_attributes`(`src/app/window_lifecycle.rs`)에 있다. 에이전트 창 생성은 사용자의 포커스 창을 바꾸지 않는다([포커스 정책](../../design/policies/focus.md#에이전트가-만든-창과-포커스)).

### engine 관계

한 engine은 창·parked·pending·retiring 관계 중 하나에 있고, 관계가 바뀌어도 `EngineId`와 `EngineSession`은 그대로다. PTY·구조·busy·attention·원격 세션은 engine에 있으므로 창이 없어도 유지된다. 관계표와 탐색 규칙의 정본은 [멀티 윈도우 아키텍처](../../architecture/multi-window.md#engine-registry와-parked--pty-생존)다.

| 관계 | 뜻 | 창 |
|---|---|---|
| 창 | View가 engine을 표시한다 | 있음 |
| parked | 창이 없지만 engine과 View 복원 자료(`MainViewState`)를 보관한다 | 없음 |
| pending | 창에 배정되기 전의 engine 하나 | 없음 |
| retiring `Preserve` | 창을 닫았고 마지막 View checkpoint를 기다린다(레이아웃 복원이 켜진 경우) | 없음 |
| retiring `Discard` | 창을 닫았고 stream retirement를 확정한다 | 없음 |
| retiring `Releasing` | 실행 자원의 실제 회수 receipt를 기다린다 | 없음 |

parked는 OS 최소화가 아니고 headless도 아니다. OS 최소화된 창은 여전히 창 관계이고, headless에는 View가 없어 parked 관계가 쓰이지 않는다.

## 전이

| 이전 조건 | 사건·조작 | 이후 | 실패·거절 | 다른 계층 영향 |
|---|---|---|---|---|
| — | 새 창 | parked engine이 있으면 가장 먼저 보관한 것을 이어받고(`unpark_first`), 없으면 새 engine | — | 이어받은 engine의 workspace와 선택이 그대로 보인다 |
| MainView 둘 이상 | 사용자 창 닫기, IPC `window.close` | View 제거, engine retiring | IPC는 마지막 MainView 닫기를 거절한다 | 포커스 View였으면 남은 MainView 하나로 바꾼다 |
| MainView 하나 | 사용자 창 닫기 | 설정 `close_behavior`에 따라 종료, 최소화, 종료 확인 모달(`src/app/modal/quit.rs`) | 확인 모달을 만들 수 없으면 확인 없이 종료를 진행한다 | — |
| MainView 하나 | 마지막 workspace가 닫혀 View가 스스로 닫기 요청(`close_requested`) | engine을 park | — | 레이아웃 슬롯 유지 |
| MainView 둘 이상 | 위와 같은 스스로 닫기 요청 | engine retiring | — | — |
| macOS MainView들 | 최소화 | 모든 MainView를 park | — | engine·PTY·원격 세션 유지 |
| retiring | 저장·retirement·자원 회수 완료 | engine 제거(`finish_retiring`) | 회수는 timeout으로 완료를 가장하지 않는다 | 이 시점에야 실행 자원이 사라진다 |
| 창 | 전체화면 무대 열기 | 무대 표시. 진입 전 OS 전체화면·최대화를 기록하고, OS 전체화면이 아니었으면 테두리 없는 전체화면으로 바꾼다 | — | 무대 뒤 콘텐츠는 다시 그리지 않는다 |
| 무대 | 무대 닫기 | 진입 전 OS 전체화면이었으면 그대로 둔다. 아니었으면 전체화면을 풀고 기록한 최대화 상태로 되돌린다 | — | 무대가 열린 동안에는 attention 포커스 확인을 하지 않는다 |
| 모달 없음 | 모달 열기 | 활성 모달 설정 | 모달은 한 번에 하나 | 다른 View 입력 차단 |
| 모달 | 모달 닫기 | 활성 모달 없음 | — | 기존 포커스로 돌아간다 |

창 닫기 요청을 받아들인 시점, View를 제거한 시점, 구조 저장이 확정된 시점, 실행 자원 회수가 끝난 시점은 서로 다르다. 앞의 두 단계는 위 표의 닫기 행에서 즉시 일어나고, 뒤의 두 단계는 retiring 관계에서 끝난다. 단계와 결과 불명 처리는 [닫기와 실행 자원 회수](../../architecture/close-sequence.md#engine과-workspace-수명)에 있다. 전체화면 무대 자체의 규칙은 [전체화면 무대](../../design/systems/fullscreen-stage.md)에 있다.

## 가능한 조합과 적용되지 않는 조합

| 조합 | 판정 | 이유 |
|---|---|---|
| MainView 둘 + 모달 하나 | 가능 | 모달이 열린 동안 두 MainView 모두 입력을 받지 않는다 |
| 모달 둘 | 불가 | `active_modal`은 하나다 |
| 포커스 View가 모달 | 불가 | `focused_view_id`는 MainView만 가리킨다 |
| 창 0개 + engine 살아 있음 | 가능 | parked. 트레이가 프로세스를 유지할 수 있다 |
| parked engine + busy·attention 변화 | 가능 | busy 폴링(`poll_busy_states`, `src/app/busy.rs`)과 mirror 이벤트는 parked engine에도 적용된다 |
| 최소화된 창 + parked | 불가 | Windows·Linux의 최소화(또는 트레이로 숨김)는 창 관계를 유지한다. macOS의 최소화는 MainView를 없애고 park하므로 최소화된 창이 남지 않는다 |
| 이미 OS 전체화면 + 무대 | 가능 | 무대를 닫아도 OS 전체화면은 유지된다 |
| headless + 창 상태 | N/A | View가 없다 |

## 문서 예시

- **MainView 둘과 모달 하나**: 창 A·B가 열려 있고 설정 모달이 열렸다. A·B는 입력을 무시하지만 크기 변경과 다시 그리기는 처리한다. A를 클릭하면 OS 포커스는 A로 가도 모달에 포커스를 다시 요청하고, A·B의 workspace 선택은 그대로다. 모달을 닫으면 각 창은 OS 포커스를 따른다.
- **마지막 MainView를 park했다가 복원**: macOS에서 최소화하면 MainView가 사라지고 engine은 parked가 된다. PTY와 원격 attach 세션은 그대로이고 그동안 온 출력·mirror 이벤트도 engine에 적용된다. 새 창을 열면 그 engine과 보관한 `MainViewState`를 이어받아 같은 workspace·선택으로 다시 보인다.
- **headless**: View가 없으므로 OS 창, 모달, 포커스 View, parked가 모두 N/A다. engine은 하나이며 busy·attention·원격 세션은 engine에 있다. `window.close`는 headless에 없다.
- **전체화면 무대**: 이미 OS 전체화면인 창은 무대를 열고 닫아도 OS 전체화면이 유지된다. 일반 창은 무대를 열 때 전체화면이 되고, 닫을 때 원래의 일반 또는 최대화 상태로 돌아간다.

## 관련

- [상태 모델](index.md)
- [구조 계층](../hierarchy.md)
- [멀티 윈도우 아키텍처](../../architecture/multi-window.md)
- [닫기와 실행 자원 회수](../../architecture/close-sequence.md)
- [포커스 정책](../../design/policies/focus.md)
