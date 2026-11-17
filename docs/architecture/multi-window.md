# 멀티 윈도우 아키텍처

tasty 는 **단일 프로세스 · 메인 스레드 단일 winit 이벤트 루프**로 여러 OS 윈도우(모달 포함)를 돌린다. `App`(winit `ApplicationHandler`)이 도메인·외부통신·GUI 세 부분을 합성한다.

## 구조

```text
App (프로세스 실행 조정; GUI에서는 winit ApplicationHandler)
├── services: AppServices       공유 저장소·registry·TaskService·port
├── state: AppState             boot/shutdown·요청 진행 값
├── hub: Hub                    IPC 서버·포트 파일
├── journal: JournalApplication CommandExecutor worker와 완료·publication 조정
├── remote: Remote              GUI attach client·SSH 시도와 실제 연결 자원
├── engines: EngineRegistry     GUI engine의 단일 소유 표와 관계
│   └── EngineSession          CoreState + live/runtime/task/hook/remote/receipt
└── view: ViewRegistry          GUI View·active_modal·focused_view_id
    └── views: HashMap<WindowId, Box<dyn View>>
```

headless는 로컬 ViewRegistry나 attach client를 만들지 않는다. 실행 루프가 EngineSession을 소유하며 AppServices·journal·서버 attach를 사용한다.

`focused_view_id` 는 대상 없는 IPC 요청이 떨어지는 main 창이다. 창을 등록할 때는 그 창을 사용자가 만들었을 때만 옮긴다 — 에이전트가 만든 창은 옮기지 않는다([포커스 정책](../design/policies/focus.md#에이전트가-만든-창과-포커스), [포커스 정책 (운영 상세)](../design/policies/focus.md)).

모든 View는 하나의 `views` 맵에 보관한다. 모달도 별도 객체 집합으로 관리하지 않고 `active_modal`로 활성 View와 종류를 표시한다. 활성 모달의 원본은 이 필드 하나이며 창별 MainViewState에는 사본이 없다.

## View 트레잇 계층 (`src/view/`)

```text
View (sealed trait, : sealed::Sealed + std::any::Any)
├── ModalView         (supertrait)
│   ├── SettingsView   (설정 모달)
│   ├── QuitView       (종료 확인 모달)
│   └── PluginsView    (plugin 매니저 모달)
└── (그 외 — View + sealed::Sealed 직접 구현)
    ├── MainView       (터미널 호스트 — 워크스페이스/페인/탭/서피스)
    └── PresetView     (프리셋 편집기, modeless)
```

- **`View`**: 공통 인터페이스. `sealed::Sealed` supertrait 으로 크레이트 외부 구현 차단. `Any` 로 downcast.
- **`ModalView`**: 모달 계열의 default 동작 marker(`shown`/`set_shown`/`reveal_after_first_render`/`on_escape`). 그 외 구현체(`MainView`/`PresetView`)는 `View` + `sealed::Sealed` 를 직접 구현한다.
- **`ViewBase`**: 모든 구현체가 `pub base: ViewBase` 로 합성하는 공통 필드(gpu·winit·dirty·modifiers·focused·close_requested).

> 용어: 여기서 "윈도우"는 winit OS-level 윈도우다. Workspace·Pane·Tab·Surface 도메인 계층과 구분한다 — [구조 계층](../concepts/hierarchy.md).

## 모달 (Modal modality)

프로세스 전역 최대 1개. 설정창·종료 다이얼로그가 대표.

- Modal View 가 열리면 `set_active_modal` 로 ID·종류를 세우고, 닫히면 `take_active_modal` 로 비운다(`src/app/modal.rs`).
- 이벤트 디스패처는 다른 modeless View 에 `modal_active: true` 를 전달하고, 각 View 의 입력 핸들러가 이때 입력을 차단한다(Resized/RedrawRequested/ModifiersChanged/Focused 만 허용).
- 모달도 같은 `views` 맵에 있어 단일 이벤트 디스패처가 전부 처리한다.

## engine registry와 parked — PTY 생존

App이 가진 모든 engine은 `App.engines`(`EngineRegistry`, `src/app/engine_registry.rs`)에 있다. engine과 그 id(`EngineSession`, `src/runtime/engine_session.rs`)는 `EngineId`를 키로 한 곳에 머물고, 창을 열거나 닫아도 다른 컬렉션으로 옮겨 가지 않는다. 창과의 관계만 바뀐다.

- **창**: 창 ID → engine id. MainView는 engine을 소유하지 않고 id도 들지 않는다. App이 창 ID로 registry에서 engine을 찾아 View에 넘긴다(`ViewCtx.engine`, 창 루프의 `window_pairs`, 창 ID 접근의 `window_pair`).
- **parked**: 창이 없는 engine과 창을 다시 만들 때 쓸 MainViewState. 보관 순서를 유지한다. 모든 윈도우가 닫혀도 PTY 세션을 잃지 않는 근거다. 윈도우가 0개여도 프로세스(트레이)는 살아 있을 수 있다 — [system-tray 정책](../design/policies/system-tray.md).
- **임시(pending)**: 창에 배정되기 전의 engine 하나. 부팅과 새 창 생성이 이 관계를 거쳐 창에 붙인다.
- **retiring**: 창과 분리됐지만 저장·확정 retirement·실제 자원 회수가 남은 owner. Discard/Preserve 이후 Releasing 상태에서도 같은 sessions 표에 남는다.

한 engine은 창·parked·pending·retiring 관계 중 하나에 속한다. 전체 owner 순회는 retiring까지 포함해야 한다. 창 전이는 관계만 바꾼다.

`EngineSession`은 `CoreState`와 `LiveDomainState`·`EngineRuntime`, engine별 `HookRuntimeState`·`TaskScope`·`ObserverRouter`·`RemoteState`를 직접 소유한다.
`EngineRuntime`에는 Terminal/Pty 쌍의 단일 TerminalStore, child terminal 관계와 GUI의 readonly Terminal 사본이 있다.
park/unpark는 이 객체나 작업 대기 허브를 교체하지 않는다. 창의 사용자 선택·viewport는 MainViewState에 남는다.
headless도 같은 Session을 지역 변수로 소유하며 로컬 View를 만들지 않는다.

View는 `EngineRead`(`src/runtime/engine_read.rs`)를 받는다. 실행 adapter는 `EngineMut`/`EngineRef`(`src/runtime/engine_access.rs`)로 필요한 원본을 나눠 빌린다.
이 대여 타입들은 참조만 가지며 engine id·창 관계·종료 책임은 없다. 구조만 필요한 함수는 `CoreState`를 그대로 받는다.
Core에 상위 `EngineSession` 전체를 전달하지 않는다. Terminal 읽기는 기존 내용 잠금과 snapshot/tap 경계를 유지한다.

생성 조립은 `src/runtime/engine_session/bootstrap.rs`가 담당한다. 설정·슬롯·registry·child 관계 읽기와 기본 shell 생성은
기존 시작 조건을 따른다. Session Drop은 hook·task·observer를 Terminal보다 먼저 정리하고, observer는 남은 worker를 join한다.
TaskScope Drop은 task 취소나 OS 자식 종료를 새로 요청하지 않는다. Terminal과 Pty는 store의 같은 항목에서 각각 소유하며 standalone에서도 Pty가 child kill/wait를 맡는다.
순수 journal replay와 외부 materialization은 분리돼 있다. 실제 종료·재시작 동작의 검증은 source 배치만으로 보장되지 않는다.


| 전이 | 동작 |
|------|------|
| 창 등록 | 임시 engine을 창 관계로 옮긴다(`register_window`). |
| park | 창 관계를 parked로 바꾸고 View 복원 자료(MainViewState)를 뒤에 붙인다. 마지막 창 닫기와 macOS 최소화가 쓴다. |
| unpark | 가장 먼저 보관한 parked engine을 임시 관계로 옮긴다. 새 창이 이어받는다. |
| 은퇴 | 창 관계를 retiring으로 옮긴다. 보존 checkpoint 또는 stream retirement와 수락된 작업을 정리한 뒤 Releasing에서 원 runner·물리 자원 receipt를 기다린다. 완료한 owner만 제거한다. |

parked 상태에서도 engine은 살아 있으므로 레이아웃 슬롯 점유를 유지한다. 종료 때 저장은 창 engine과 parked engine을 한 번씩 본다. `App.engines`는 `App.view` 바로 뒤에 있어 프로세스 종료 때 모든 창 View가 먼저, 그 뒤 engine이 drop된다.

### engine 탐색

engine만 다루는 App의 순회는 `src/app/window_access.rs`의 `EngineScan`(읽기)·`EngineScanMut`(쓰기)를 거친다. 이 두 핸들은 `App::engines`/`App::engines_mut`로 얻고, 같은 함수에서 `App.services` 같은 다른 필드를 함께 빌려야 하면 `engines_mut!` 매크로로 `views`와 `engines` 필드만 빌린다. 창 View 작업(다시 그리기 표시, toast 등)이 함께 필요한 창 루프는 MainView와 그 engine을 쌍으로 주는 `window_pairs`를, 창 ID로 고른 접근(`find_main_with_*`의 결과 등)은 `window_pair`를 쓴다. engine 기준 요청(`DispatchSource::Engine`)은 `resolve`가 MainViewState·engine과 창이 있으면 그 창의 `ViewBase`를 함께 돌려준다.

- 방문 순서는 창(`views` 순회 순서) → parked(보관 순서) → 임시 engine이다. 호출부가 필요한 관계만 고른다(`windowed_and_parked`, `windows_and_pending`, `primary` 등). 창 목록의 순서는 `HashMap` 순회 순서라 고정된 의미가 없다. 창 관계가 있어도 `views`에 View가 없으면 창 순회에 나오지 않는다.
- `EngineScan::all`은 창·parked·pending을 읽으며 retiring을 포함하지 않는다. 반면 `EngineRegistry::all_sessions`는 실제 owner 전체를 순회한다. 슬롯 점유·종료·미완 receipt를 다루는 코드는 후자를 사용한다. 탐색 API의 이름만으로 같은 집합이라고 가정하지 않는다.
- parked 보관과 복원은 `park`(뒤에 붙인다)·`unpark_first`(가장 먼저 보관한 항목)로 한다.
- engine 은 생성할 때 받은 `EngineId` 를 관계가 바뀌어도 유지한다. engine 기준 요청과 특정 parked engine 은 보관 위치가 아니라 이 id 로 찾는다(`parked_session`, `DispatchSource::Engine`, attach mirror 출력 대상). 다른 engine 을 보관하거나 꺼내 위치가 바뀌어도 같은 engine 을 가리킨다.

engine의 존재 여부는 창 관계와 parked 관계를 함께 확인한다. 창이 없다는 이유만으로 원격 attach 세션을 끊으면 안 된다. mirror 이벤트도 parked engine에 적용하며 창을 복원하면 그 상태를 표시한다([원격 세션 수명](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명), [Attach 메커니즘](../dev-guide/attach-behavior.md)).

### 창이 스스로 닫히는 자리 — `close_requested`

마지막 워크스페이스가 닫힌 MainView 는 `request_close()` 로 `ViewBase.close_requested` 만 세우고, App 이 그 플래그를 보고 창을 치운다(마지막 main 창이면 파킹, 아니면 은퇴). **플래그는 세운 경로 안에서 소비한다** — 세운 채 이벤트 루프로 돌아가면 워크스페이스가 빈 창이 다음 창 이벤트를 받고, 그것이 `RedrawRequested` 면 렌더 경로가 active workspace 를 묻다 죽는다.

- **창 이벤트 경로**(winit 키 · 마우스 · redraw 안의 메뉴 continuation): `App::dispatch_window_event_to_view` 가 handler 직후 그 창을 치운다.
- **창 이벤트 밖의 App 경로**(`about_to_wait` 의 webview 포워딩 키 단축키 · 네이티브 메뉴 폴링): 그 App 함수가 MainView 를 부른 뒤 같은 함수 안에서 `App::close_self_requesting_windows` 를 부른다.

렌더 경로 쪽을 "빈 워크스페이스면 건너뛴다" 로 누그러뜨리지 않는다 — 그 창은 이미 닫혀야 할 창이고, 건너뛰어도 다음 IPC·입력 핸들러가 같은 전제로 active workspace 를 묻는다. 두 번째 목록의 짝과 `request_close()` 생산자 명부는 `crates/tasty-doc-guards/tests/close_request_consumed_in_place.rs` 가 고정한다.

## 레이아웃 슬롯

슬롯을 가진 GUI engine은 한 **레이아웃 슬롯**에 연결되고, MainView는 연결돼 있는 동안 한 engine을 표시한다. parked·retiring처럼 창이 없는 owner도 회수가 끝날 때까지 슬롯을 유지한다. headless의 새 구조 stream은 GUI 슬롯을 갖지 않는다. 각 EngineSession의 persistence/binding은 해당 슬롯(있는 경우)과 journal stream을 가리킨다. 창마다 워크스페이스 목록이 독립이라는 구조적 사실이 저장소까지 이어진 형태다 — 두 창이 같은 목록을 복제하거나 서로의 저장을 덮어쓰지 않는다.

**점유는 살아있는 engine 에서 파생된다.** 프로세스 내 실행 owner의 슬롯 점유를 위한 별도 레지스트리나 디스크 기록은 없다. 복원 원본인 DB manifest와 그 binding은 이 실행 중 점유 집합과 구별한다. 점유 집합은 sessions의 전체 owner에서 파생하며 창·parked·pending·retiring을 포함한다. 갓 만들어진 engine 은 창에 붙기 전까지 임시 관계에 머물기 때문에, 그 구간을 빠뜨리면 같은 슬롯이 두 번 배정된다. 따라서

- engine이 실제로 drop되면 그 슬롯은 그 순간 free 가 된다 — 해제 호출이 없으니 해제 누락도 없다.
- **parked engine 은 슬롯을 계속 쥔다.** 창이 없어도 engine 이 살아 있으므로 점유에 포함되고, 다시 창을 열 때 그 engine 이 같은 슬롯을 이어쓴다. 재배정했다면 남의 슬롯 파일을 덮어썼을 것이다.
- 프로세스가 죽으면 점유는 전부 사라진다 — 크래시가 슬롯을 영구 점유로 남기지 않는다.

결정의 근거·대안·재검토 조건은 [포커스 정책 (운영 상세)](../design/policies/focus.md), 배정 규칙과 창 닫힘 정책의 현재 동작은 [layout-persistence](../features/layout-persistence/index.md).

## 윈도우 간 GPU·통신

- 각 View 는 자체 wgpu surface + egui 컨텍스트 보유(`ViewBase.gpu`). wgpu adapter/device 는 공유.
- 윈도우 간 직접 통신 없음 — 모두 도메인(Core)·Intent 큐를 경유.

## 단일 프로세스 · 단일 스레드 근거

| 관점 | 근거 |
|------|------|
| 상태 접근 | 메인 루프가 engine을 대여하고 공유 서비스는 필요한 Arc/Mutex를 사용한다. 창끼리 구조 사본을 동기화하는 IPC는 없다 |
| GPU 리소스 | wgpu adapter/device 윈도우 간 공유 가능 |
| winit 호환 | winit 은 프로세스당 이벤트 루프 1개를 전제 |
| 크래시 격리 | 셸은 이미 별도 OS 프로세스(PTY) — 셸 크래시가 tasty 로 전파 안 됨 |
| 리소스 방어 | 스크롤백 상한 + PTY 읽기 채널 버퍼 제한 |

플러그인은 별도 프로세스로 실행한다. 설치한 플러그인은 권한 없이 플러그인에 연 호스트 API를 호출하며, OS 자원 접근을 격리하는 샌드박스도 없다([호스트 호출 계약](../dev-guide/plugin-permissions.md#호출자별-판정)).

## 관련

- [아키텍처 개요](index.md) — headless `gui` feature 분리
- [input-layer](input-layer.md) — 윈도우 내부 마우스 입력 계층
- [concepts/hierarchy](../concepts/hierarchy.md) — Workspace·Pane·Tab·Surface 도메인 계층
- [포커스 정책 (운영 상세)](../design/policies/focus.md) — 레이아웃 슬롯 점유 모델의 근거·대안
- [features/layout-persistence](../features/layout-persistence/index.md) — 슬롯 배정·저장·복원의 현재 동작
