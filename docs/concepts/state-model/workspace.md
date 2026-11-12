# Workspace 상태 모델

Workspace는 구조 트리의 최상위 객체다. 구조 외에 로컬·mirror 구분과 저장된 원격 행 정보를 가진다(`crates/tasty-model/src/workspace.rs`). 선택은 View가, 연결 상태는 원격 세션이, 점유는 `LiveDomainState`가 가진다. busy·attention 표시는 하위 surface에서 계산한다.

## 상태 축

| 축 | 값 | 소유자 | 저장 | 관측·파생 | 변경 주체 |
|---|---|---|---|---|---|
| 구조 | 있음·없음, 이름·부제·설명, 카테고리, 상위 레이아웃 | `CoreState`의 `Workspace` | 로컬 workspace는 journal. mirror 형상은 저장하지 않는다 | 확정 구조 | 생성·닫기·이동 명령, mirror 구조 Delta |
| 종류 | 로컬, mirror(`mirror`), 저장된 원격 행(`mirror`이고 `remote_row` 있음) | `Workspace` | 저장된 원격 행은 journal의 원격 행 항목 | — | 원격 연결 팝업(저장 행), 에이전트 attach(저장하지 않는 mirror) |
| 선택 | View마다 workspace 하나 | View의 `NavigationState`의 workspace 선택 | 메모리 | View 선택 | 로컬 사용자 |
| 연결 세션 단계 | `Connected`, `Reconnecting`, `Disconnected` | `tasty_remote::client_session::SessionState`(`crates/tasty-remote/src/client_session.rs`) | 메모리 | 연결 관측 | 연결·재연결·끊기 처리 |
| 원격 행 상태 | 아래 표의 6개 문자열 | 계산(`row_state_name`, `src/app/auto_attach/row_use.rs`) | 메모리(engine마다 `row_states`) | 파생 | 세션 단계, 진행 중 시도, 실패 표지, 소켓 생존 |
| 점유 | 없음, hard(workspace lock) | `LiveDomainState.occupancy`의 workspace lock(`src/core/attach.rs`) | 메모리 | 관계 기록 | 원격 attach, 연결 종료, 로컬 사용자 강제 해제 |
| busy | 개수 | 계산(`busy_count`) | 저장하지 않음 | 하위 surface busy 개수 | surface busy 변화 |
| attention | kind별 개수 | 계산(사이드바 행) | 저장하지 않음 | 하위 surface attention 개수 | surface attention 변화 |

### 선택과 active

선택 workspace는 View마다 하나다. IPC `workspace.list`의 `active`는 요청한 창의 선택이다(`src/adapters/ipc/handler/workspace.rs`). 여러 engine을 합산한 목록에서는 창마다 active가 있으므로 true가 여러 개일 수 있다([포커스 정책](../../design/policies/focus.md#목록-조회)). active는 OS 포커스 창을 뜻하지 않는다. 선택이 사라지면 이전 순서의 다음 생존 workspace, 없으면 이전 생존 workspace를 고른다([포커스 정책](../../design/policies/focus.md#삭제로-인한-인덱스-이동에서도-포커스-대상은-보존된다)).

### 연결 상태

저장된 원격 행과 에이전트 mirror의 연결 단계는 `SessionState`다. 세션이 없는 동안의 상태는 별도 표지로 구분한다. `workspace.list`의 `remote_state`와 사이드바·연결 팝업은 아래 값을 쓴다.

| `row_state_name` 값 | 조건 | 사이드바 상태(`RemoteRowState`) |
|---|---|---|
| `connected` | 세션 `Connected`이고 소켓이 살아 있음 | `Connected` |
| `reconnecting` | 세션 `Reconnecting`, 또는 `Connected`인데 손실 뒤 재동기화로 소켓을 놓음 | `Reconnecting` |
| `disconnected` | 세션 `Disconnected`(재연결 포기, 사용자가 끊음) | `Disconnected` |
| `connecting` | 세션 없음, 시도 진행 중 | `Connecting` |
| `failed` | 세션 없음, 첫 연결 실패 뒤 재시도 대기 | `NotConnected` |
| `not_connected` | 세션 없음, 시도 없음 | `NotConnected` |

- `workspace.list`의 `remote_state`는 저장된 원격 행이 아니면 `null`이다. 저장 행인데 상태가 실리지 않았으면 `not_connected`다. headless는 원격 행을 연결하지 않으므로 늘 `not_connected`다.
- 에이전트 mirror(저장하지 않는 mirror)의 `remote_state`는 `null`이다. 연결 상태는 세션에 있지만 이 필드로 보고하지 않는다.
- 연결 상태는 구조 필드가 아니다. 끊겨도 `mirror`·`remote_row` 값은 바뀌지 않는다.
- 연결 시작·재시도·시간 창·사용자 끊기의 규칙은 [원격 attach](../../features/remote-attach/index.md#저장된-원격-행의-연결과-재연결)가 정본이다.

### 점유

workspace hard 점유(`acquire_workspace`)는 workspace lock을 기록하고, 그 workspace의 터미널에는 같은 holder의 surface lock을, 나머지 멤버에는 소속 매핑을 등록한다. 그래서 터미널은 입력이 막히고(`is_hard_occupied`), 비터미널 멤버는 입력 판정에서는 점유가 아니지만 렌더에서 내용이 가려진다(`is_content_hidden`). 강제 해제(`force_detach_workspace`)와 연결 종료는 workspace lock과 멤버의 lock·매핑을 함께 지운다. 서버 로컬의 비-holder가 점유 workspace의 구조를 바꾸는 IPC는 거절한다([원격 attach](../../features/remote-attach/index.md#서버피점유측-비-holder-구조-변경-차단)). soft 점유는 workspace 단위가 없다.

### busy와 attention 집계

- busy는 하위 surface 가운데 busy인 개수다(`busy_count`, `src/runtime/terminal_activity.rs`). `workspace.list`의 `busy_count`가 이 값이다. 사이드바는 연결되지 않은 저장 행의 busy를 세지 않는다([원격 attach](../../features/remote-attach/index.md#저장된-원격-행의-표시)).
- attention은 사이드바 행이 kind별 개수를 센다(`src/adapters/ui/sidebar/view.rs`의 `needs_input_count`·`completion_count`). `workspace.list`에는 attention 필드가 없다.
- 두 집계는 선택과 무관하다. 선택되지 않은 workspace도 개수를 표시한다. 표시 규칙은 [Surface Highlight](../../features/surface-highlight/index.md#표시)와 [busy 표시 정책](../../design/policies/busy-indicator.md)에 있다.

## 전이

| 이전 조건 | 사건·조작 | 이후 | 실패·거절 | 다른 계층 영향 |
|---|---|---|---|---|
| — | 사용자 생성 | 새 workspace가 생기고 선택된다 | — | — |
| — | 에이전트 `workspace.create` | 새 workspace, 선택 불변 | 지정 surface를 가진 창이 없으면 거절 | — |
| workspace 둘 이상 | 사용자 전환 | 그 View의 선택 변경 | — | 다른 View 불변. 새로 보이는 복원 자리는 활성화된다 |
| 로컬 workspace | 닫기 | workspace와 모든 하위 객체 제거 | 마지막 workspace, mirror, hard 점유 surface 포함, 호출자 자기 surface 포함이면 거절(`src/app/journal/commands/close.rs`) | 선택이었으면 생존 workspace로 보정 |
| mirror | 사용자 닫기(서버 닫기) | 서버 성공 회신 뒤 mirror·세션 정리 | 재연결 중·미지원·이미 닫기 대기 중이면 보내지 않는다(`src/app/attach_client/workspace_close.rs`). 서버 거절이면 mirror 유지 | 저장 행이면 행도 지운다 |
| mirror, 닫기 회신 대기 | 회신 전 연결 종료 | 결과 불명 안내. 기존 단절 처리를 따른다 | 성공으로 단정하지 않는다 | — |
| mirror | 연결 끊기(`remote.disconnect`) | mirror 정리. 서버 workspace와 프로세스는 남는다 | — | 저장 행은 `disconnected`로 남는다 |
| 저장된 원격 행, 연결됨 | 네트워크 단절 | `reconnecting`. 형상 유지 | 시간 창이 지나거나 다른 client가 점유하면 `disconnected` | — |
| 에이전트 mirror, 연결됨 | 단절 | mirror 정리 | — | 마지막 workspace였으면 기본 터미널 workspace를 만든다 |
| 로컬 workspace | 원격 client의 hard 점유 | workspace lock, 터미널 surface lock | 살아 있는 다른 holder가 있으면 거절 | 서버 로컬의 구조 변경 IPC 거절 |
| hard 점유 | 연결 종료, 강제 해제 | 점유 없음 | release에는 강제 해제 IPC가 없다 | 멤버 lock 해제 |
| — | 앱 재시작 | 로컬 workspace는 journal에서, 저장된 원격 행은 형상 없는 행으로 돌아온다 | — | 에이전트 mirror는 돌아오지 않는다 |

mirror 닫기와 연결 끊기, 저장된 원격 행의 메뉴는 [원격 attach](../../features/remote-attach/index.md#mirror-닫기와-연결-끊기)가 정본이다. 로컬 닫기의 거절 문구와 마지막 workspace 경계는 [작업 영역](../../features/work-area/index.md#인터페이스)에 있다. mirror는 무조건 닫기 거절 대상이 아니다. 로컬 닫기 명령은 mirror를 거절하지만 사용자 닫기는 서버 닫기로 전달된다.

단절은 원격 프로세스 종료가 아니다. 연결이 끊겨도 서버의 workspace와 PTY는 그대로 남는다.

## 가능한 조합과 적용되지 않는 조합

| 조합 | 판정 | 이유 |
|---|---|---|
| 선택되지 않은 workspace + busy 개수·attention 배지 | 가능 | 집계는 선택과 무관하다 |
| 여러 View에서 active | 가능 | 선택은 View마다 있다 |
| 로컬 workspace + `remote_state` 값 | N/A | `null`이다 |
| 저장된 원격 행 + 형상 없음 | 가능 | 재시작 뒤 연결 전, 첫 연결 실패 |
| `reconnecting` + 형상 유지 | 가능 | 재연결 중에도 mirror를 걷어내지 않는다 |
| 두 holder의 workspace hard 점유 | 불가 | `acquire_workspace`가 거절한다 |
| workspace 0개인 View | 불가 | 마지막 workspace 닫기를 거절하고, mirror 정리로 비면 기본 workspace를 만든다 |
| headless + 선택 | N/A | 로컬 View가 없다 |

## 문서 예시

- **두 MainView의 workspace 목록**: 창 1은 W1, 창 2는 W2를 선택했다. 합산 목록에서는 W1과 W2의 `active`가 모두 true다. OS 포커스가 창 1에 있어도 W2의 `active`는 그대로다.
- **비활성 workspace의 집계**: 창 1이 W1을 보는 동안 W2에 busy surface 둘과 `NeedsInput` attention 하나가 있다. 집계 시점까지 attention 해제와 busy 갱신이 없으면 사이드바의 W2 행은 busy 2와 `NeedsInput` 배지 1을 보인다. `workspace.list`의 W2는 `busy_count: 2`, `active: false`다. attention 개수는 이 응답에 없다.
- **연결 단절**: 저장된 원격 행 R과 에이전트 mirror M이 같은 서버에 붙어 있다가 네트워크가 끊긴다. R은 형상을 남긴 채 `reconnecting`이 되고, 재연결 시간 창이 지나면 `disconnected`로 남는다. M은 정리돼 사라진다. 서버의 두 workspace와 프로세스는 둘 다 남는다.
- **hard 점유 로컬 workspace 닫기**: 원격 client가 로컬 workspace L의 터미널을 hard 점유한 동안 로컬 사용자나 에이전트가 L을 닫으면 거절된다. 로컬 사용자는 강제 끊기로 점유를 회수한 뒤 닫을 수 있다.
- **mirror 서버 닫기**: 연결이 살아 있고 서버가 닫기를 지원하면 요청을 보내고 성공 회신 뒤 정리한다. 재연결 중이면 보내지 않고 안내한다. 서버가 지원하지 않으면 보내지 않는다. 같은 mirror의 닫기가 회신을 기다리면 두 번째는 보내지 않는다. 회신 전에 연결이 끊기면 결과 불명 안내를 띄우고 닫혔다고 단정하지 않는다.

## 관련

- [상태 모델](index.md)
- [pane 상태 모델](pane.md)
- [window 상태 모델](window.md)
- [원격 attach](../../features/remote-attach/index.md)
- [작업 영역](../../features/work-area/index.md)
- [포커스 정책](../../design/policies/focus.md)
