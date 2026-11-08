# 포커스 정책 (운영 상세)

포커스는 사용자가 보고 입력하는 창·워크스페이스·pane·탭·surface를 나타낸다. 에이전트의 IPC/CLI 요청은 이를 바꾸지 않으며 release에는 포커스 변경 API가 없다. 이유는 [사용자 통제와 데이터 원칙](../../principles/control-and-data.md#사용자에이전트원격-사용자), 계층 용어는 [구조 계층](../../concepts/hierarchy.md)을 따른다.

## 계층

```
App
└── ViewRegistry (여러 View, HashMap<WindowId, …>)
    ├── ModalView    — 활성 시 모든 입력 독점 (앱 전역 최대 1개)
    └── 그 외 (MainView / PresetView 등) — Modal 없을 때 OS 네이티브 포커스
        └── Pane / Surface — View 내부 포커스
```

각 MainView는 EngineRegistry의 binding으로 EngineSession을 가리킨다. Engine은 프로세스나 OS 창과 같은 뜻이 아니며 View 없이도 살아 있을 수 있다.

## Modal 포커스 차단

앱(ViewRegistry)이 활성 모달의 ID·종류를 `active_modal` 하나로 보유하고 `active_modal_id()` 로 ID를 읽는다.

- **Modal 없음**(`active_modal_id() == None`): 각 View 는 OS 네이티브 포커스를 따른다. View 들은 독립적으로 포커스를 받고(z-order 독립), 사이에 다른 앱 창이 있을 수 있다.
- **Modal 있음**(`active_modal_id() == Some(id)`): 이벤트 디스패처가 각 View 에 `modal_active: bool` 을 전달한다. Modal 이 아닌 View 는 입력 이벤트를 무시하고(`Resized`/`RedrawRequested`/`ScaleFactorChanged`/`ModifiersChanged`/`Focused` 만 통과), Modal View 만 `modal_active: false` 로 받아 정상 동작한다. Modal 을 닫으면 기존 포커스로 자연 복귀한다.

| 상태 | 구현체 | 동작 |
|----------|--------|------|
| Modal | `SettingsView` · `PluginsView` · `QuitView` (`ModalView`) | 전체 입력 차단, 닫기 전까지 다른 조작 불가 |
| Modeless | `MainView` · `PresetView` (`View` + `sealed::Sealed` 직접 구현) | 독립 포커스, 다른 윈도우와 공존 |

**OS 네이티브 윈도우 비활성화(Win32 `EnableWindow` 등)는 쓰지 않는다** — 플랫폼별 동작 차이로 크로스플랫폼 일관성이 깨진다. 앱 레벨 `modal_active` 게이트로 처리한다.

## View 내부 포커스

Modal/View 레벨과 별개로, 각 View 내부에서 Pane 간·Surface 간 포커스 이동과 탭 전환이 일어난다 (단축키/클릭). 단축키는 [`KeybindingSettings`](key-mapping.md) — 하드코딩 아님. 이 내부 포커스는 그 View 가 OS 포커스를 갖고 Modal 이 비활성일 때만 동작한다.

탭바를 기본 버튼으로 클릭하면 해당 pane으로 포커스를 옮긴다. 탭 자체뿐 아니라 빈 영역·스크롤 화살표·추가·분할·검색 버튼도 포함한다. 빈 영역에서는 탭을 바꾸지 않고 pane 포커스만 옮긴다.

HTML 탭의 lock 표지(스크립트 차단 안내를 닫은 뒤 탭 제목 뒤에 남는 자물쇠) 클릭은 예외다. 안내만 다시 띄우고 pane 포커스와 활성 탭을 바꾸지 않는다. 비활성 탭의 lock을 눌러도 그 탭으로 전환하지 않는다.

pane·surface 분할선 hit 띠를 좌클릭하면 포커스를 옮기지 않고 분할선 드래그를 시작한다. 비활성 surface 쪽 띠도 같다. 판정 순서는 [입력 계층](../../architecture/input-layer.md)에 있다.

우클릭 메뉴는 `pane_id`·`tab_index`로 대상을 전달하므로 포커스를 옮기지 않는다. 이 동작은 사용자가 GUI에서 직접 클릭한 결과이며, 에이전트가 포커스를 바꾸지 못하게 하는 IPC 규칙과는 별개다.

구현은 `src/adapters/ui/tab_bar.rs`의 `TabBarAction::focus_target_pane`과 `src/adapters/ui/tab_bar/apply.rs`의 `apply_tab_bar_actions`에 있다.

## CLI/IPC 포커스 독립 원칙

release IPC/CLI에는 사용자 포커스를 바꾸는 API가 없다.
요청은 대상 ID가 가리키는 engine에서 실행하고, 대상을 지정했는데 찾지 못하면 오류를 반환한다.
포커스된 창의 다른 대상으로 바꾸어 실행하지 않는다. 대상을 받지 않는 생성 요청이나 호환용 기본값은
아래 예외를 따르며, 명시 ID를 주는 방법을 우선한다.

### 목록 조회

창별 자원 목록은 모든 main·parked engine에서 모은다. `src/app/dispatch/list_global.rs`가 처리한다.
새 목록을 추가할 때 메서드 이름이 list로 끝나는지만 보지 말고 실제로 읽는 컬렉션과 대상 인자를 확인한다.
필터 인자는 대상 지정과 다르다. tree와 workspace.list는 같은 workspace 집합을 반환해야 한다.

| 자원 | 조회 방식 |
|---|---|
| workspace, pane, tab, surface, tree | 전체 engine 합산 |
| attach | 한 번의 engine 순회에서 surface·workspace 점유 배열을 함께 합산 |
| hook, global hook | 공유 ID 카운터로 ID를 보장한 뒤 합산 |
| notification | 전체 생성 ID 역순 최대 50개. 병합은 생성 ID 유지, UI·읽음·보존은 창별 |
| approval | 창 생성 때 같은 approval_store Arc를 공유하므로 공유 목록 조회 |
| image.list | plugin이 host-call로 반환한 요청도 host 합산 경로 사용 |

합산한 active는 각 창의 활성 상태이므로 여러 개가 true일 수 있다.
일반 자원 ID는 IdGenerator의 engine 간 공유 카운터로 유일하게 만든다.
예외인 예약 카테고리 normal(ID 0)은 한 행으로 합친다. rename·delete·move 대상이 아니므로
창을 별도로 지정할 필요가 없다. count는 전 창 합, 위치는 고정값, collapsed는 모든 창에서 접혔을 때만 true다.

분류 명부는 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs`다.
명부는 기존 분류 누락을 검사하지만 새 종류의 목록을 자동 발견하지 않는다.
공유 여부는 CoreState 생성자뿐 아니라 `App::ensure_engine_and_plugins`의 Arc 전달·교체도 읽어 판단한다.

### 대상 해소

라우터는 surface·workspace·pane·tab·PTY·hook·global hook·output observer·preset source·split 대상을 해소한다.
같은 메서드의 핸들러와 같은 표현·우선순위로 읽는다. 예를 들어 split은 숫자 문자열을 먼저 해석하고
nickname은 메모리 저장소에서 해소한다. nickname 매핑은 창에 종속되지 않는다.
발신자 from_surface_id, 호출자 caller_surface_id, stream client ID는 대상 ID가 아니다.

명시한 대상이 없으면 무엇을 찾지 못했는지 오류를 반환한다. headless도 같은 검사를 수행한다.
headless의 사전 검사는 host 예약 prefix에 한정한다. plugin이 처리할 요청을 미리 거절하지 않기 위해서다.

순서 변경은 대상 ID와 목적지 to_index로 표현한다. tab.move는 pane_id와 탭 인덱스로 pane을 먼저 지정한다.
workspace.move·workspace_category.move의 옛 index-only 입력은 호환상 focused 창을 사용하지만,
ID와 옛 index를 동시에 주면 거절한다.

terminal.kill·release·respawn·broadcast의 parent 생략은 현재 engine에 parent가 정확히 하나일 때만 허용한다.
main 창이 둘 이상이면 engine 자체가 불분명하므로 --surface를 요구한다.
TASTY_SURFACE_ID는 호출자가 있는 surface이며 사용자 포커스와 다르다. CLI가 지원하는 --surface 기본값으로 사용할 수 있다.

### 검토 기준

활성 상태를 응답으로 조회하는 것은 허용한다. 요청 대상과 포커스는 별도로 취급한다.
생성·삭제에 내부적인 임시 focus 변경이 필요하면 원래 focus를 복원해야 한다.
새 대상 키는 `every_id_key_a_handler_reads_is_routed_or_exempt`의 대조 대상이며,
대상이 아닌 키는 구체적인 사유와 함께 제외한다.

<a id="폴백으로-가는-메서드는-이름과-사유로-남는다"></a>

## 기본 라우팅을 사용하는 메서드

`src/source_guards/unrouted_dispatch_reasons.rs`는 라우팅 대상 키가 드러나지 않는 메서드를 분류한다.
명부에는 주인 창이 정해지지 않아도 답이 올바른 이유를 적는다.

| 분류 | 이유 |
|---|---|
| NotWindowOwned | 저장소가 창에 속하지 않음 |
| AggregatedList | 모든 engine의 목록을 합산함 |
| CreatesWithoutATarget | 새 대상을 생성함 |
| ScopedObservation | 창별 관측이며 응답에 소유 ID와 scope를 표시함 |
| TargetReadByDeserializer | serde 구조체가 대상을 읽어 단순 키 검사가 찾지 못함 |
| RoutedOutsideRequestTarget | 다른 단계에서 대상 해소 |
| DebugOnly | 사용자 조작 재현용 debug 기능 |
| PerWindowOpenDefect | 아직 해결되지 않은 창별 처리 문제 |

현재 명부의 PerWindowOpenDefect 항목은 없다. 이것이 모든 IPC 관측 문제의 해결을 뜻하지는 않는다.
system.info는 engine count/index를 유지하고 scope=engine, workspace_ids, active_workspace_id,
layout_slot을 함께 반환한다. window.list는 OS ID와 함께 이 값을 제공하고 전역 workspace 목록은 workspace.list가 담당한다.
버전은 프로세스 전역 값이다.

Git 조회는 창별 큐를 수집한 뒤 전역 attach 세션에서 local_surface_id를 찾는다.
따라서 큐 위치만 보고 조회 대상 오류라고 판단하지 않는다.
recent.query는 state.db의 공유 캐시에서 종류별 최근 항목 최대 10개를 읽으며 순서와 focus를 변경하지 않는다.
파일 열기는 명시 origin의 engine·pane을 비동기 완료와 picker 선택까지 유지하고,
대상이 사라졌으면 다른 창에 열지 않는다. 사용자 파일 열기와 에이전트 파일 열기의 선택 차이는 아래 절을 따른다.

<a id="라우팅-아래에도-층이-하나-더-있다--그-층은-대상을-안-고른다"></a>

## 핸들러의 활성 상태 조회

라우터가 창을 정한 뒤 핸들러도 그 창의 활성 상태를 읽을 수 있다. 응답·기본값·계측·알림에 사용하는 것과 명시한 요청 대상을 포커스로 바꾸는 것은 구분한다. `src/adapters/ipc/`에서 사용하는 경우는 다음 다섯 부류다.

| 부류 | 하는 일 | 판정 |
|---|---|---|
| 보고 | 응답에 활성 상태를 싣는다(`"focused"` · `"active"` · `active_workspace`) | 위 "활성 상태 *조회* 는 허용" 그대로 |
| 기본값 채우기 | 대상은 인자로 지목됐고, **미지정 인자**만 포커스가 채운다 — 새 탭·split 의 cwd 상속, `surface_id` 를 안 실은 `workspace.create` 의 cwd 상속, `telemetry.record` 의 workspace, `approval.request` 의 workspace(단 `surface_id` 를 줬으면 그 surface 의 워크스페이스라 포커스를 안 읽는다) | 호출자가 명시하면 안 읽는다. 명시하지 않으면 같은 인자로 두 번 불러도 값이 다를 수 있다 |
| 계측 태그 | 공통 요청 게이트(`handler/gate.rs`)가 audit 귀속에 engine의 활성 workspace를 읽는다. telemetry는 같은 engine의 첫 workspace를 태그로 쓴다 | **판정에는 안 쓰인다** — 게이트가 빌린 engine의 관측 문맥이며, 요청이 작용한 대상이나 사용자의 포커스를 증명하지 않는다 |
| 알림 배치 | cap 임계·이상 탐지·승인 요청 알림이 활성 워크스페이스에 뜬다 | 사용자에게 보이라고 두는 자리라 에이전트 대상 결정이 아니다 |
| 효과 scope | `debug.host_popup.open` 의 `workspace_scope` | debug 전용. 사용자 조작 재현이라 창 종속이 뜻 자체다 |

기본값에 활성 상태를 사용하는 요청은 인자를 생략하면 같은 호출도 결과가 달라질 수 있다. 재현 가능한 결과가 필요하면 인자를 명시한다. 요청이 대상을 지정했다면 기록도 그 대상에 귀속한다. 구체적인 예외와 이유는 이 문서을 따른다.

계측 태그의 workspace는 요청 대상이 아닐 수 있다. 따라서 이 필드만으로 해당 workspace에서 발생한 작업이라고 판단하지 않는다.

### 재는 명령

    # 테스트 전용 코드를 제외한 사본을 만든다(테스트에서도 같은 이름을 쓴다)
    cargo run -q -p tasty-doc-guards --bin strip-cfg-test -- \
        --blank-test-only-files <out> . src/adapters/ipc src/app
    # 다섯 포인터를 센다
    grep -rn 'focused_view_id\|active_workspace\|focused_pane\|active_tab\|focused_surface' <out>

    # 합산 집합의 소속 판정(docs/design/policies/focus.md) — 창 소유 컬렉션을 순회하는 핸들러를 뽑아
    # `src/app/dispatch/list_global.rs` 의 arm 과 대조한다. 대상 인자가 있는 것
    # (`surface_id` 등을 받는 것)은 라우터가 주인 창을 푸니 합산 대상이 아니다.
    grep -rn 'for ws in &engine.workspaces\|engine.workspaces.iter()' <out>/src/adapters/ipc
    grep -oE '"[a-z_.]+" =>' src/app/dispatch/list_global.rs

핸들러는 라우터가 정한 창 안에서 실행하므로 `focused_view_id`를 직접 읽는 곳이 0이어야 한다. 새 접근이 생기면 창 선택을 핸들러가 대신하고 있는지 확인한다.

`approval.request`의 기록은 명시 workspace_id, 지정 surface의 workspace, 활성 workspace 순서로 귀속한다.
외부 요청의 없는 surface는 라우터가 먼저 거절한다. 핸들러를 직접 부른 내부 경로는 같은 검사를 중복하지 않는다.
telemetry.record와 record_batch는 workspace_id를 생략하면 활성 workspace를 사용하므로 재현 가능한 기록에는 값을 명시한다.

비용 상한은 agent와 metric의 누적값이며 여러 workspace의 이벤트가 섞인다.
마지막 이벤트가 전체 비용의 소속이라고 볼 수 없어 자동 승인과 알림은 현재 활성 workspace를 사용한다.
영속 승인 기록까지 그 scope에 남는 한계가 있다. workspace별 상한 또는 신뢰할 수 있는 원인 surface가 생기면
이 귀속을 다시 정한다. audit의 활성 workspace 태그와 IPC telemetry의 첫 workspace 태그 역시 요청 대상의 증거가 아니다.

## 삭제로 인한 인덱스 이동에서도 포커스 대상은 보존된다

사용자가 보던 대상 자체가 사라졌을 때만 다른 대상으로 이동한다. 보지 않던 workspace·tab·pane을 닫아도 보고 있는 대상은 유지한다(이 문서).

`MainViewState.navigation`이 workspace ID, workspace별 pane ID, pane별 tab ID, tab별 surface ID를 소유한다. 기존 응답의 index는 현재 구조에서 계산한다.

- 살아 있는 선택 ID는 앞쪽 항목 삭제나 재정렬로 바뀌지 않는다.
- 선택한 workspace/tab이 사라지면 이전 순서의 다음 생존 ID, 없으면 직전 생존 ID를 고른다. 여러 항목이 함께 삭제된 snapshot에도 같은 규칙을 쓴다.
- pane/surface 선택이 삭제되면 첫 생존 대상을 고른다. 구조 replace가 대상 B를 A로 대체했다면 그 결과의 ID 대응으로 B의 선택을 A에 연결한다.
- 이 보정은 origin과 무관하다. 사용자 생성 결과를 새로 선택하는 continuation과 삭제 보정은 별개다.
- category 복귀 기록은 workspace ID다. 조회할 때 현재 소속을 검사하고 없으면 카테고리의 첫 workspace를 고른다.
- 삭제된 pane의 탭바 offset, category 표시 자료, split hint는 결과 적용 시 회수한다.

Core의 결과 ID를 App의 공통 구조 adapter가 받아 navigation을 보정한다. Core 모델의 필드를 고쳐 View 선택을 바꾸지 않는다. headless의 같은 ID 해소 알고리즘은 로컬 사용자 포커스가 아니라 명령 기본 문맥을 유지한다.

## 자기 자신 닫기 보호 (Self-Close Protection)

**명령으로 자신이 속한 리소스를 닫을 수 없다** — 에이전트가 target ID 를 잘못 지정해 자기 터미널을 종료하는 사고 방지.

- ID 지정 close(`close surface --surface <ID>` / `close pane --pane <ID>` / `close tab --tab <ID>`)에서 caller 자신이 속한 대상을 지정하면 거부한다.
- **자기 자신을 닫는 유일한 방법은 `tasty close self`** (`TASTY_SURFACE_ID` 로 자신을 식별, 해당 surface 만 닫음).
- 워크스페이스를 통째로 정리할 때는 `tasty close workspace --workspace <W>` 를 쓴다 — 안의 surface 를 하나씩 닫을 필요가 없다. 자기 자신 닫기 보호는 여기에도 걸린다: caller 의 surface 가 그 워크스페이스 안에 있으면 거부한다.

## 에이전트 닫기와 포커스

에이전트가 **사용자가 보고 있지 않은** 대상을 닫아도 사용자 화면은 움직이지 않는다.

- `workspace.close` 후 App이 navigation을 현재 구조와 대조한다. 선택 ID가 살아 있으면 유지하고 삭제된 선택만 보정한다. 새 제거 경로도 같은 결과 적용을 거친다.
- **활성 워크스페이스 자신을 닫을 때만** 이웃으로 이동한다.
- 에이전트가 닫은 것은 사용자의 "닫은 항목" 되돌리기 스택에 쌓이지 않는다. View의 `WorkspaceCloseOrigin`은 고정 ID의 Intent를 만드는 입력 표지다. App journal close admission이 원 Reply/Intent origin에서 `RetirementPlan.is_user_close`와 `remote_user_close`를 정하고, 확정된 계획을 undo·회수·완료 통지까지 유지한다. 원격 사용자 닫기는 undo를 허용하지만 로컬 plugin 이벤트의 User 이유로 바꾸지 않는다.
- 확정 workspace 닫기의 통지와 memory 정리는 origin에 따른 표시 정책과 구별한다. `ResourceRetirement`가 원 자원 receipt와 metadata 정리를 완료하고 App이 완료 통지를 해소한다. View는 선택·cache만 보정하며 새 제거 경로도 이 필수 정리를 우회하지 않는다([닫기 순서](../../architecture/close-sequence.md)).

### 파일 열기의 사용자 동작 판정

`FileDispatchOrigin`은 결과 탭 선택 여부와 후속 Intent의 출처를 정한다. 비동기 파일 식별이 끝날 때까지 이 값을 전달한다. plugin이 사용자 클릭을 IPC로 중계할 수 있으므로 IPC를 사용했다는 사실만으로 에이전트 요청이라고 판단하지 않는다.

- plugin이 `owner_popup_instance`를 보내면 호스트는 그 plugin이 팝업 소유자인지, 팝업이 포인터 버튼이나 키 누름으로 확정 입력을 받았는지 확인한다. 외부 IPC 호출자는 같은 값을 보내도 에이전트 요청이다.
- WebView 링크는 plugin이 통지받은 URL을 `user_navigation_url`로 보낸다. 호스트가 확인한 사용자 제스처이며, 현재 페이지를 소유 plugin이 작성했고, 그 plugin에 알린 마지막 탐색 시도일 때만 한 번 사용자 요청으로 인정한다. `file_picker.trigger`도 같은 필드와 규칙으로 피커를 사용자 요청으로 연다.
- `webview.set_url`은 에이전트도 호출할 수 있다. 에이전트가 작성한 페이지의 클릭은 사용자 요청으로 인정할 근거가 아니다. 페이지 작성자가 바뀐 뒤 늦게 도착한 탐색 이벤트의 구분은 아직 완전히 검증되지 않았다.
- 근거가 없는 plugin 중계 요청은 `PluginUnverified`다. macOS는 엔진 대신 WebView가 받은 OS 클릭으로 제스처를 정한다. Windows의 같은 문서 이동은 host 입력 경로가 본 OS 클릭과 페이지로 간 Enter·Space로 정한다([WebView 탐색](../systems/webview.md#탐색-상태와-실행-소유)). 결과 탭 선택은 에이전트 요청처럼 유지하고, 매칭 핸들러가 없을 때의 picker는 사용자 클릭일 수 있어 연다. 외부 IPC 요청(`Agent`)은 picker를 열지 않는다.

상세 조건과 한계는 [파일 열기 가이드](../../features/file-handler/index.md), 결정 이유는 [파일 핸들러 (File handler)](../../features/file-handler/index.md)에 있다.

workspace.close는 마지막 workspace, mirror workspace, hard 점유 surface가 포함된 workspace를 거절한다.
호출자 자신을 포함한 대상도 자기 닫기 보호를 따른다. mirror는 attach 해제로 정리한다.
GUI 사용자가 mirror 카드에서 닫기를 고르면 서버 workspace 닫기로 forward하고, 연결 끊기는 로컬 mirror만 정리한다([원격 attach](../../features/remote-attach/index.md#mirror-닫기와-연결-끊기)).
GUI 창 종료는 window.close를 사용하되 headless에는 이 API가 없어 마지막 workspace를 닫을 수 없다.
확인용 force 플래그는 요구하지 않지만 되돌릴 수 없는 동작임을 도움말과 사용자 문서에 표시한다.
일부 점유 surface만 남기는 부분 workspace.close는 수행하지 않는다.

## 에이전트가 만든 창과 포커스

에이전트가 창을 만들어도 사용자가 보던 창은 그대로 focused 창이다.

- 창을 만든 주체는 `WindowRequestOrigin` **하나**가 정한다.
  - `User`: 부팅 첫 창 · 단축키 · 명령 팔레트 · CSD 버튼 · 트레이 · macOS dock.
  - `Agent`: IPC `window.create` / `view.create`. CLI `tasty new window` 와 hook 의
    `ipc_sequence` 도 여기로 온다.
  - 창 생성 실패를 누구에게 알릴지도 같은 값이 가른다. 새 경로도 이 값을 정해 넘긴다.
- `User` 창은 `focused_view_id` 를 새 창으로 옮긴다. `Agent` 창은 옮기지 않는다. 그래서 뒤이은
  대상 없는 요청(`tasty new workspace` 등)은 사용자가 보던 창에서 실행된다. 새 창에 워크스페이스를
  만들려면 그 창의 surface 를 `workspace.create` 의 `surface_id`(CLI `tasty new workspace --surface`)로
  지목한다 — `window_id` 는 창 자체를 다루는 요청(닫기 · 스크린샷 등)에만 쓴다
  ([CLI 크레이트 내부 구조 — 세 갈래 대칭](../../dev-guide/cli-structure.md)).
  그 `surface_id` 는 `cwd` 를 생략했을 때의 상속 원본도 정한다 — 지목했는데 그 창의 포커스 surface 를
  읽으면 결과가 사용자가 그 창에서 보는 탭에 좌우되기 때문이다
  ([CLI 크레이트 내부 구조 — 세 갈래 대칭](../../dev-guide/cli-structure.md)).
  - 예외: 가리키던 창이 없으면(main 창이 0 개였으면) 에이전트 창을 기본 대상으로 삼는다.
- 에이전트 창은 숨긴 채 만들어, 등록 뒤 사용자가 보던 창 **뒤에** 키 포커스 없이 보인다
  (`tasty_platform::window_stacking::show_behind`). OS 마다 할 수 있는 데까지다.
  - macOS · Windows: 사용자 창 바로 아래에 둔다. 키 포커스를 가져가지 않는다.
  - X11: 창 관리자에게 **요청한다** — `_NET_WM_USER_TIME = 0`(포커스를 주지 말라)과
    `_NET_RESTACK_WINDOW`(사용자 창 아래). 창 관리자가 무시하거나 "맨 아래" 로 다룰 수 있다.
    사용자 창을 트레이로 숨겼으면 restack 없이 보이기만 한다(숨긴 창을 형제로 가리키면 openbox 3.6.1이 죽는다).
    `_NET_WM_USER_TIME` 은 map 된 뒤 지운다 — 사용자가 나중에 그 창을 고를 때 걸림이 없게.
  - 네이티브 Wayland(`DISPLAY` 없는 환경): 할 수단이 없다. 컴포지터가 정한다.
  - 네이티브 호출이 실패하면 경고 뒤 winit 기본 경로로 보인다.
- 사용자가 에이전트 창을 직접 고르면 `WindowEvent::Focused(true)` 추적이 `focused_view_id` 를
  옮긴다.

근거와 플랫폼별 결과는 이 문서.

## 에이전트가 연 팝업과 포커스

사용자 조작 근거가 없는 팝업 열기 요청은 [passive 팝업](../systems/popup.md#passive-팝업)으로 열어 키보드 포커스를 옮기지 않는다. 승인 요청·권한 상승 요청이 이 경로를 쓴다. 사용자가 팝업을 누르면 포커스가 옮겨 간다. debug의 `debug.host_popup.open`은 사용자 조작 재현이라 포커스를 준다.

## 같은 홈으로 다시 실행했을 때 (release 단일 실행)

다시 실행한 두 번째 프로세스가 창을 올릴 수 있는 것은 OS가 사용자 실행에 붙여 준 활성화 증거가 있을 때뿐이다.
Tasty에는 포커스를 주는 IPC 메서드가 없다.

- 증거 없음(Linux에서 터미널·에이전트·스크립트로 `tasty` 실행, Windows에서 `AllowSetForegroundWindow`가 거절된 실행, macOS에서 바이너리 직접 실행): `window.create`와 같은 `Agent` 창 하나로 끝난다.
  위 "에이전트가 만든 창과 포커스" 규칙을 그대로 따르며 숨긴 창·최소화한 창·`focused_view_id`를 건드리지 않는다.
- 증거 있음(앱 목록·실행기·시작 메뉴로 실행. Windows는 포그라운드 프로세스나 그것이 시작한 프로세스면 `AllowSetForegroundWindow`가 성공하므로 앞에 있는 터미널에서 친 `tasty`도 여기에 든다. 실 기기 미측정): 실행 중인 Tasty가 숨기거나 최소화한 View를 트레이 복원처럼 다시 보이고,
  `focused_view_id`의 View 활성화를 OS에 요청한다. `focus_window()`는 부르지 않는다.
  - X11(Wayland 세션의 XWayland 포함): 창에 `_NET_STARTUP_ID`를 걸고 startup id의 `_TIME` 타임스탬프로 `_NET_ACTIVE_WINDOW`(source 1)를 보낸다. GNOME의 XWayland에서 실행기 토큰만 받아도 포커스가 옮겨 간다(실행 중 인스턴스에 `Activate`를 직접 보내 격리 세션에서 확인했고 두 번째 프로세스 경로는 미측정).
  - Windows: 두 번째 프로세스가 `AllowSetForegroundWindow`로 넘긴 권한으로 `SetForegroundWindow`를 부른다.
  - 네이티브 Wayland(`DISPLAY` 없는 환경): 두 번째 실행이 받은 xdg-activation 토큰으로 기존 창 surface에 `xdg_activation_v1.activate`를 보낸다(winit 포크의 `WindowExtWayland::activate_with_token`). 위 복원 대신 새 `User` 창을 열고 숨김·최소화 창은 그대로 두는 경우가 둘이다. 토큰이 없으면 토큰 없는 새 창이고, 컴포지터가 `xdg_activation_v1`을 제공하지 않으면 토큰을 실은 새 창이다. MainView가 없어 새 창을 열 때는 토큰을 생성 속성에만 싣고 등록 뒤 다시 요청하지 않는다.
  - MainView가 하나도 없으면 증거를 실은 `User` 창을 연다. 만드는 중인 창이 있으면 새로 만들지 않고 그 창이 등록될 때 요청한다.
- 증거는 OS 판단의 근거일 뿐 위조 불가능한 증명은 아니다. X11 startup id는 같은 사용자의 어떤 프로세스든 만들 수 있다.
  한계와 근거는 이 문서의 "같은 홈의 다시 실행" 절에 있다.

## 에이전트가 만든 탭과 선택

에이전트가 탭을 만들어도 그 pane 의 활성 탭은 그대로다.

- 각 진입점은 `DomainIntent::CreateTab`의 `activate`로 사용자 후속 선택 의도를 전달한다.
  Core는 구조를 만들고 결과 ID를 반환하며, App의 사용자 continuation이 해당 View의 선택을 적용한다.
  원격 전송의 active/focused 값은 별도 명시 projection으로 합성한다.
  - IPC `tab.create`(CLI `tasty new tab`): `false`. 새 탭은 뒤에 붙기만 한다.
  - attach forward 의 `NewTab`: 원격 **사용자**의 손 조작이면 `true`, 원격 에이전트면 `false`
    (복원 스택을 가르는 `ForwardOrigin` 과 같은 축).
  - 파일 열기의 origin 갈래: `FileDispatchOrigin` 에서 파생(위 "에이전트 닫기와 포커스" 의 파일
    열기 항목).
  - `Intent::NewTab`: 발화 origin 이 사용자면 `true`, 아니면 `false`. 에이전트 라벨로 오는
    발화점은 origin 없는 `file_handler.dispatch` 하나이고, 그 갈래는 이제 사용자가 보던 탭을
    바꾸지 않는다. markdown plugin 의 파일열기 팝업(사용자의 `open_markdown`)은 같은 호출에 자기
    popup 을 실어 사용자로 도착하므로 종전대로 새 탭을 선택한다(아래 "에이전트 닫기와 포커스" 의
    파일 열기 항목 · [파일 핸들러 (File handler)](../../features/file-handler/index.md)).
- terminal kind 는 `activate` 와 무관하게 background 다. 사용자의 새 터미널 탭은 이 인텐트가
  아니라 `MainViewState::add_tab` 이 연다.
- 에이전트가 만든 새 비터미널 탭은 사용자가 선택하기 전까지 렌더되지 않는다.
- 응답의 `active_tab` 은 "생성 뒤 그 pane 의 활성 탭" 이다 — 에이전트가 만든 탭이면 사용자가
  보던 탭의 인덱스다. 새 탭은 응답의 `surface_id` 로 다룬다.


<a id="원격이-점유한-surface-는-닫기-요청이-죽이지-않는다"></a>

## hard 점유 대상의 닫기 보호

hard 점유 대상은 원격 사용자가 사용 중이므로 로컬 사용자와 에이전트의 닫기 요청을 모두 거절한다. 닫은 항목 복원은 같은 명령으로 새 세션을 만들 뿐 기존 PTY 작업을 되살리지 못한다. 로컬 사용자는 점유 표시의 강제 끊기로 점유를 회수한 뒤 닫을 수 있다([Attach 메커니즘](../../dev-guide/attach-behavior.md)).

닫기 진입점은 `MainViewState::refuse_if_hard_occupied`에 닫을 surface 집합을 넘긴다. workspace·tab·pane은 그 안의 모든 surface, surface 닫기는 해당 하나를 검사한다.

이 검사는 사용자·에이전트의 닫기 요청에만 적용한다. 이미 종료된 셸의 사후 정리를 막으면 화면에 종료된 surface가 남으므로 공용 정리 함수에서는 거절하지 않는다.

IPC에는 토스트 대신 사유를 담은 오류를 반환한다. `surface.close_self`도 예외가 아니다. 해당 메서드는 호출자 대신 params의 ID로 대상을 받으므로 예외를 두면 보호를 우회할 수 있다.

## 재정렬에서도 포커스 대상은 보존된다

워크스페이스와 탭 재정렬은 구조 순서만 바꾼다. 사용자가 선택한 ID는 그대로 유지하며 UI와 IPC가 필요한 index를 새 순서에서 계산한다. 카테고리 안의 표시 순서는 입력 단계에서 전체 workspace 위치로 변환한다. category CRUD나 소속 변경만으로 workspace 배열 순서와 선택 ID를 바꾸지 않는다.

## 코드 위치

- `active_modal_id()` / `modal_active` 게이트: `src/app/event_handler.rs`(`self.view.active_modal_id()`), View 디스패치.
- focus 대상 해석 / `TASTY_SURFACE_ID` / `this`: `crates/tasty-cli/src/request.rs`.
- `tasty close self`: `crates/tasty-cli/src/commands/new_close.rs`(`CloseCommands::CloseSelf`).
- 창 생성의 origin 분기: `WindowRequestOrigin`(`src/app/event.rs`) → `focus_after_register` · `origin_window_attributes`(`src/app/window_lifecycle.rs`) — 등록 뒤 focused 창과 생성 속성(`with_active` · `with_visible`)이 여기서 파생된다. 에이전트 창을 사용자 창 뒤에 보이는 OS 호출은 `crates/tasty-platform/src/window_stacking.rs`.
- 다시 실행 요청: 두 번째 프로세스는 `src/boot/single_instance/second.rs`, 실행 중 쪽 판단은 `plan_activation`(`src/app/external_activation.rs`), OS 활성화 요청은 `crates/tasty-platform/src/window_activation.rs`.
- 탭 생성의 선택 분기: `DomainIntent::CreateTab`의 `activate`를 Core 결과에 연결하고 App adapter가 사용자 continuation으로 처리한다. 값을 정하는 진입점은 App journal 명령 admission의 호출자 · `src/app/creation_intent.rs` · `open_surface_tab`(`src/file/dispatch.rs`).
- 워크스페이스 close의 origin: View producer는 `src/state/workspace.rs`, 실제 원 reply/origin 분류와 retirement 계획은 `src/app/journal/commands/close.rs`다.
- 워크스페이스 제거 후 뒷정리: 확정 닫기의 `workspace.closed` 전달은 App의 완료 후처리이며 workspace 범위 memory 정리는 `ResourceRetirement`의 metadata 정리(`src/runtime/resource_retirement.rs`).

외부 소켓의 전 창 합산·namespace·App 조기 응답도 일반 handler와 같은 진입 검사와 허용된 요청의 사용량 집계를 한 번 거친다. 검사 완료 요청을 하위 라우터에 전달하므로 라우팅 층 수만큼 예산이 소비되지 않는다. [플러그인 권한 모델](../../dev-guide/plugin-permissions.md).

순서 이동도 source를 ID로 고정한다. IPC·remote의 기존 `from_index`는 App adapter에서 `workspace_id` 또는 `tab_id`로 해소한 뒤 Core 명령을 만든다. `to_index`는 목적지 순서 좌표이며 기존 범위 밖·동일 위치 no-op을 유지한다. 먼저 실행된 이동이 목록을 재정렬해도 이미 수락한 명령의 source가 다른 객체로 바뀌지 않는다.
