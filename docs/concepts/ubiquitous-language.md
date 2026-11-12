# 유비쿼터스 언어 (통합 용어집)

코드·문서·IPC·CLI에서 사용하는 용어를 정리한다. 여기서는 짧은 정의와 상세 문서 링크를 제공한다. 정확한 동작과 제약은 각 개념 문서에서 확인한다.

> 용어를 잘못 쓰면 코드·문서·API 일관성이 깨진다. 특히 **Window/View**, **Workspace/Pane/Tab/Surface** 계층, **상위/하위 레이아웃**, **Modal/Popup/Toast/Banner/Modifier-hint 오버레이** 구분을 혼동하지 않는다.

## 정본 문서

| 영역 | 정본 | 다루는 용어 |
|------|------|-------------|
| 주체 | [actors.md](actors.md) | 로컬 사용자 · AI Agent · 원격 사용자 · 점유 |
| 구조 | [hierarchy.md](hierarchy.md) | Engine · View · Workspace · Pane · Tab · Surface · 두 레벨 레이아웃 |
| 상태 | [state-model](state-model/index.md) | 계층별 상태 축 · 활성화 단계 · busy · attention · 점유 · 선택 범위 |
| 플러그인 | [plugins.md](plugins.md) | 배포/통합 축 · surface_kind · 권한 |
| Surface 종류 | [`../surfaces/index.md`](../surfaces/index.md) | kind별 명세 |
| attach | [`../dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) | server/client · mirror · lock |
| 원격 연결 | [`../features/remote-profiles/index.md`](../features/remote-profiles/index.md) | 원격 접속 프로필 · Passkey · kind |
| 훅 | [`../features/hooks/index.md`](../features/hooks/index.md) | Surface hook · Global hook · hook handler 레지스트리 |
| Claude plugin | [`../plugins/claude/index.md`](../plugins/claude/index.md) | Claude 세션 프로필 · reboot · spawn/respawn |

## 용어 한 줄 정의

### 주체 (→ [actors.md](actors.md))

- **로컬 사용자** — 이 머신에서 GUI 를 직접 쓰는 사람. 포커스의 주인. 점유 불필요. 점유를 끊을 수 있는 유일한 주체.
- **AI Agent** — IPC·CLI로 Tasty를 조작하는 AI. 대상을 ID로 지정하며 사용자의 포커스·히스토리·선택을 바꾸지 않는다. 점유 없이 작업하는 것이 기본이며 필요하면 soft/hard 점유를 사용한다. child-terminal의 soft 점유가 한 예다.
- **원격 접속 사용자** — SSH로 접속한 사람. 에이전트와 같은 동작 권한을 가지며 SSH 셸·터널로 CLI·IPC를 쓸 수 있다. 로컬 GUI 입력은 쓰지 않는다. attach 스트림으로 화면을 조작할 때는 hard 점유한 대상만 조작한다.
- **점유(occupation)** — 원격 사용자나 AI가 surface/workspace를 사용 중임을 표시하는 관계. soft는 입력을 제한하지 않는 표시, hard는 다른 주체를 읽기 전용으로 제한하는 점유다(docs/dev-guide/attach-behavior.md). 같은 계층(soft 또는 hard) 안에서 한 대상의 점유자는 하나이고 한 주체는 여러 대상을 점유할 수 있다. 계층이 다르면 soft 위에 hard가 함께 있을 수 있다. 현재 동작과 근거는 [주체](actors.md#점유-occupation-모델)에 있다. 본인의 self-release나 로컬 사용자의 force-detach(GUI 조작)로 해제한다.

### 원격 연결 (→ [features/remote-profiles](../features/remote-profiles/index.md))

- **원격 접속 프로필(Remote profile)** — 확장 가능한 문자열 `kind`로 연결 유형을 구분한다. 비밀 값은 담지 않고 Passkey 이름을 참조한다. `ssh`는 연결 정보, `tasty-attach`는 attach 설정이다. `tasty-attach`는 `ssh_ref` 또는 인라인 SSH 정보와 `remote_tasty`·`port_mode`·`port_file`을 사용한다. attach는 `tasty-attach`만 받는다(docs/features/remote-profiles/index.md).
- **Passkey** — 이름으로 관리하는 자격증명. `kind=path`는 기존 파일을 참조하고 `inline`은 값을 권한 0600 파일에 저장한다. TOML에는 비밀 대신 파일 경로를 기록한다. 값은 로컬 GUI의 Reveal에서만 볼 수 있고 IPC·에이전트에는 공개하지 않는다([에이전트 메모리 시스템](../design/systems/memory.md)).
- **미등록 타입** — 내장 기능(ssh/smb)이나 설치된 플러그인이 처리한다고 선언하지 않은 `kind`. 등록은 허용하되 노란 배지로 알린다.

### 구조 (→ [hierarchy.md](hierarchy.md))

- **App** — 프로세스 진입점과 공유 서비스·IPC Hub·실행 조정의 소유자. GUI에서는 ViewRegistry와 EngineRegistry를 관리한다.
- **AppState** — App의 부팅·종료 단계와 앱 수준 대기 값. ViewRegistry·EngineRegistry·AppServices의 원본을 복제하지 않는다.
- **ViewState** — View마다 공통인 identity·focus·modifier·repaint·close 상태. MainView의 선택·viewport·popup 등 종류별 상태는 `MainViewState`에 합성한다.
- **구조 journal** — 데이터 홈에서 확정 구조 사건·명령 결과·effect 의무를 보관하는 원본. engine별 stream을 같은 저장소에서 관리하며 terminal 출력이나 사용자 선택 자체를 구조 사건으로 기록하지 않는다.
- **Engine** — EngineSession이 소유하는 도메인·실행 범위. CoreState projection, LiveDomainState, EngineRuntime과 task·hook·remote 범위를 포함한다. headless도 App과 EngineSession을 실행하되 로컬 View는 없다.
- **Window** — winit OS 창 자원(`winit::window::Window`). tasty 쪽 `Window` 타입은 **없다** — 이 단어는 OS 창만 가리킨다.
- **View** — tasty 쪽 윈도우 표현(종류+콘텐츠+행동). winit Window 를 소유. **1 View : 1 Window.** `MainView`/`SettingsView`/… 가 구현체.
- **CoreState** — 확정된 로컬 구조의 읽기 projection과 별도 mirror 트리. 로컬 writer는 journal projection 적용기다. 실행 인스턴스는 EngineRuntime, 사용자 선택은 View가 소유한다.
- **Workspace** — 도메인 최상위 컨테이너. 사이드바에서 전환.
- **Workspace Category(카테고리/사이드바 폴더)** — 워크스페이스를 묶는 **그룹 계층**(사이드바 섹션). `workspace_categories_enabled` 설정으로 on/off. 예약 카테고리 **`normal`**(id `0`, `categories[0]` 위치 고정, rename/delete 불가)가 항상 존재하고, 미지정 워크스페이스의 기본 소속이다. 카테고리 *CRUD·reorder·소속 변경*은 에이전트 작업(IPC/CLI 양면, release) — *선택(active)·접힘 토글*은 사용자 UI 상태(IPC 노출 안 함). 정본 [`features/workspace-category`](../features/workspace-category/index.md).
- **Pane** — 독립 탭 바를 가진 영역. **상위 레이아웃**이 위치 결정(탭 무관 고정). tasty 고유.
- **Tab** — Pane 안의 탭 하나. 내부에 Surface 들의 **하위 레이아웃**을 가짐(탭 전환 시 함께 전환).
- **Surface** — 최하위 컨테이너. `surface_id` + **kind(타입)** 를 가짐. 닫기/포커스/리스트는 kind 무관 동일. kind마다의 명세는 [`surfaces/`](../surfaces/index.md).
- **상위 레이아웃 / 하위 레이아웃** — Pane 배치(탭 무관) / Surface 배치(탭 종속). 두 레벨을 **둘 다** 제공하는 게 tasty 핵심 설계. 동의어: **PaneGroup** = 상위 레이아웃(`PaneNode`), **SurfaceGroup** = 하위 레이아웃(`SurfaceLayout`) — 코드 타입이 아니라 주석·문서가 쓰는 이름이다.

### 사용자 화면 표기

위 정의는 영어 용어와 코드 심볼을 연결한다. 화면 문구에는 아래 한국어·일본어·중국어(간체) 표기를 일관되게 사용한다. 간체 열은 [tasty-page 용어집](https://github.com/zilhak/tasty-page/blob/develop/vendor/i18n/zh-Hans/glossary.json)의 값이다.

| 용어 | 한국어 | 일본어 | 중국어(간체) |
|---|---|---|---|
| Window | 윈도우 | ウィンドウ | 窗口 |
| Pane | 페인 | ペイン | 窗格 |
| Tab | 탭 | タブ | 标签页 |
| Surface | 서피스 | サーフェス | Surface |
| Workspace | 워크스페이스 | ワークスペース | 工作区 |

- **이 표기 규칙은 화면 문구와 사용자 가이드에 적용한다.** `lang/*.toml`(호스트·번들 플러그인)과 tasty-page 레포의 한국어 사용자 가이드가 대상이다.
  `docs/`의 개발자 설명에는 적용하지 않는다. 예를 들어 위 Window 정의의 "winit OS 창 자원"처럼 개념을 설명할 때 보통명사를 사용할 수 있다.
- **`창` 은 Window 를 가리킬 때만 금지다.** Popup 이나 Modal 을 가리키는 `창`(알림 창 ·
  Git 뷰어 창 · 확인 창)과 낱말이 다른 `주소창` 은 그대로 둔다 — 판정 기준은 아래 §View 내부
  의 Window / Popup 구분이고, OS 창을 소유하는 것(= `View` 구현)만 `윈도우` 다. 이상 신호 설정의 `판정 창`은 호출을 세는 시간 구간이며 초 단위 기간을 뜻하므로 그대로 쓴다.
- 일본어 “새 윈도우”는 메뉴·설정에서 모두 `新しいウィンドウ`로 쓴다. 이 규칙은 그 동작 이름에만 적용한다. `[pane_context_menu]`의 `新規ターミナル`과 `[explorer.tab]`의 `新しいタブ`까지 같은 단어로 바꾸지 않는다.
- 화면 문구(`lang/*.toml`)는 `crates/tasty-doc-guards/tests/one_concept_one_word_on_the_user_facing_surface.rs`에서 검사한다. 이 검사는 한국어·일본어 표기만 본다. 간체 표기는 검사하지 않는다.
  사용자 가이드는 [tasty-page 레포의 checks](https://github.com/zilhak/tasty-page/tree/develop/checks)가 검사한다.
  이전 표기인 `패인`·`ウインドウ`는 사용하지 않는다. `창`을 OS Window 이외의 뜻으로 사용하는 경우에는 해당 검사 파일에 위치와 근거를 등록한다.

### View 내부 (→ hierarchy.md, `design/systems/`)

- **Modal** — 전역 1개, 활성 시 입력 차단하는 View 의 한 형태(별개 엔티티 아님). `SettingsView`/`QuitView`/`PluginsView`.
- **Popup** — View 내부 가상 창(타이틀바+콘텐츠, 드래그·z-order). 스코프 가짐. 상세 [`design/systems/popup.md`](../design/systems/popup.md).
- **Toast** — View 안에 잠깐 표시하는 알림. 포인터가 카드 위에 있는 동안은 사라지지 않는다. 포커스를 받거나 입력을 소비하지 않는다. 사용자 행동에서만 표시하며, 예외로 attach mirror의 끊김·손실·구조 전달 실패를 알릴 수 있다. 에이전트 IPC로 직접 표시하지 않는다([토스트](../design/systems/toast.md)).
- **Banner** — parent 영역 상단의 안내와 조치 버튼. 포커스는 받지 않지만 마우스 입력을 처리한다. TTL·z-order를 사용하며 scope마다 하나를 표시하고 최대 5개를 대기시킨다. 사용자 행동에서만 표시하고 에이전트 IPC로 표시하지 않는다([배너](../design/systems/banner.md)).
- **Modifier-hint 오버레이** — modifier를 누르고 있으면 기본 500ms 뒤, Shift 단독은 1200ms 뒤에 200ms 페이드로 단축키 목록을 보여 준다. 현재 누른 조합을 포함하는 단축키만 표시하며 키를 떼면 바로 사라진다.
  키보드 포커스는 그대로 두고 마우스로 이동·크기 조정·닫기를 할 수 있다. `ModifiersChanged`로 실제 사용자 입력만 받으며 IPC·CLI로 강제 표시하지 않는다. `enabled`가 꺼져 있으면 표시하지 않고 위치·크기는 `Settings::modifier_hint`에 저장한다.
  [디자인 매핑](../design/systems/design-token-mapping.md)의 modifier-hint 절을 참고한다. 모델은 `src/adapters/ui/input/shortcuts/modifier_hint.rs`, 화면은 `src/adapters/ui/modifier_hint_overlay.rs`에 있다.
- **마커 오버레이(Marker overlay)** — 대상 rect 위 최상위에 링·glow를 그리는 장치. 메시지나 심각도는 다루지 않는다. 마커와 scrim은 입력을 통과시키며(`pointer-events:none`) 옆 안내 말풍선만 상호작용을 처리한다.
  좌표는 매 프레임 `LayoutContext`·`terminal_rect`·`tab_bar_height`로 다시 계산한다. 현재는 튜토리얼에서만 사용하고 메뉴 선택·Next 클릭 등 사용자 행동으로 진행한다. 에이전트 IPC·CLI에는 표시 API가 없다([튜토리얼](../features/tutorial/index.md), `src/adapters/ui/tutorial/`).
- **전체화면 무대(Fullscreen stage)** — 창 전체에 별도 콘텐츠를 표시한다. Workspace·Pane·Tab·Surface 트리의 요소를 확대하는 기능이 아니며, popup을 표시할 때도 별도 인스턴스를 만든다. 무대 뒤 콘텐츠는 redraw하지 않고 무대를 닫으면 다시 그린다.
  창마다 최대 1개이며 `StageDef`에 선언한 것만 표시하고 영속화하지 않는다. popup 타이틀바 버튼 등 사용자 조작으로 열며 release에는 제어 API가 없다. 자체 검증은 debug 전용 `debug.fullscreen.*`을 사용한다([전체화면 무대](../design/systems/fullscreen-stage.md), [스크린샷 방법](../ai-verification/screenshot-methods.md)).
  - **Zoom 과 혼동 금지** — tasty 에서 `Zoom` 은 **UI 배율**(설정 › 단축키 › Zoom)로 이미 선점된 용어다. tmux 식 "pane zoom" 명칭을 쓰지 않고 **전체화면 / 무대(stage)** 로 통일한다.
- **상태바(Workspace status bar)** — 작업 영역 하단을 항상 차지하는 고정 strip(타이틀바 `top_inset` 과 대칭인 `bottom_inset`). focus surface 컨텍스트 표시 + 우측 빠른 액션(팔레트·테마). GUI 전용 표시 위젯(에이전트 표면 없음). 정본 [`features/workspace-status-bar`](../features/workspace-status-bar/index.md).

### Surface 주의 환기 (→ [`features/surface-highlight`](../features/surface-highlight/index.md))

- **Attention** — surface에 확인할 일이 남았음을 나타내는 공유 상태. EngineSession의 `LiveDomainState.attention`가 surface ID별 `{ kind, raised_at }`을 저장한다. kind는 Completion·NeedsInput이다.
  알림 패널의 `NotificationStore` 항목과는 별개다. `effects_of().panel_item`이 패널 표시 여부를 정하고 이벤트를 발생시킨 쪽이 패널 항목을 만든다. 원 SurfaceBinding을 확인한 App의 FocusObserved 실행에서 해제한다. toast, completion, Claude 훅, OSC133 명령 완료 등 여러 경로가 같은 attention을 사용한다.
- **Highlight** — attention을 화면에 표시하는 이름. 테두리, 노란 탭 제목, 소속 워크스페이스의 개수 배지로 나타낸다. `draw_surface_highlights`·`SurfaceHighlightRegion` 같은 화면 코드가 사용한다. Core 상태 이름 Attention과 구분하며, 잠깐 나타나는 Toast와 달리 surface에 남아 있다.
- **Completion** — surface의 작업 완료 신호. release의 `surface.completion`·`tasty surface completion`으로 보고한다. `AttentionKind::Completion`을 발생시키는 경로이지만 attention 전체를 뜻하지는 않는다. 에이전트가 자기 결과를 보고하는 기능이며, 완료 전용 효과가 필요해지면 이 처리 경로를 확장한다.

### Surface 종류 (→ [hierarchy.md](hierarchy.md#surface-타입) · [plugins.md](plugins.md))

- **host 내장** — `terminal`(PTY·GPU 셰이더), `empty`, `dag_graph`, `explorer`.
- **egui-mesh plugin** — `image` (plugin 이 `rendering=egui-mesh` 선언, plugin 프로세스가 tessellate 한 mesh 를 host 가 합성).
- **webview plugin** — `html` / `markdown`([WebView 호스트 계약](../design/systems/webview.md)) — `rendering=webview`, host 의 네이티브 WebView 오버레이로 그림.

### Claude plugin (→ [plugins/claude](../plugins/claude/index.md))

- **Claude 세션 프로필(Claude session profile)** — `tasty claude spawn/respawn/launch/reboot` 가 `--settings <경로>` 로 Claude Code 에 주입하는 `settings.json` 조각. 두 가지 방식으로 지정한다: `--profile-file <경로>`(파일 직접 지정) 또는 `--profile <이름[,이름2,...]>`(아래 레지스트리에 등록해 둔 이름, 서로 상호 배타적). Claude Code 는 훅을 프로세스 기동 시 한 번만 읽으므로, 이미 떠 있는 세션에 새 훅을 걸 유일한 창구가 이 재기동 시점의 `--settings` 주입이다 — **대체가 아니라 추가**로 병합되어 tasty 내장 훅과 함께 발화한다. `reboot` 는 부착 상태(경로 또는 이름)를 surface meta 에 남겨 다음 무인자 reboot 가 승계하고, `--clear-profile` 로 뗀다.
  - **Claude 세션 프로필 레지스트리** — 이름으로 등록해 둔 프로필을 `--profile` 로 부착하는 계층(`crates/tasty-plugin-claude/src/profile.rs`). `tasty claude profile-register/-unregister/-list/-show/-current`. 이름을 둘 이상 동시 부착하면 `--settings` 반복 지정이 last-wins 인 함정을 피하기 위해 등록 내용을 하나의 파일로 **머지**한다(`profile_merge.rs`) — 훅 배열은 union, 객체는 키 단위 병합, `permissions.allow`/`deny` 는 union 후 **deny 가 allow 를 이김**, 그 외 스칼라는 last-wins(충돌 시 경고), `permissions.defaultMode` 는 충돌 시 거부. 호스트 레지스트리(`src/hook_handler/registry.rs` 등)의 형태(patch semantics · `<owner>/<short>` id)를 미러링하지만 소비자가 이 plugin 하나뿐이라 plugin 내부에 둔다(타입 공유 없음).
  아래 두 용어와 이름만 "프로필"을 공유할 뿐 서로 무관하다:
  - **원격 접속 프로필**(위 "원격 연결" 절) — SSH/attach 연결 디스크립터. Claude 세션과 무관.
  - **surface hook / hook handler**(위 "훅" 절) — tasty 가 소유한 이벤트→핸들러 바인딩. Claude 세션 프로필은 그 반대편, 즉 **Claude Code 프로세스 자신**의 훅 설정 파일이다 — tasty 의 hook handler 레지스트리를 거치지 않는다.

### attach (→ [attach-behavior.md](../dev-guide/attach-behavior.md))

- **server / client** — 서버는 PTY를 관리하며 점유 요청을 받고, loopback 주소에서만 연결을 받는다. 클라이언트는 서버에 연결해 대상을 점유하고 입력·출력을 전달한다. "로컬/원격"은 **클라이언트에서 서버에 연결하는 방식**을 구분하는 말이다.
- **mirror** — client 가 받은 출력으로 PTY 없이 재구성한 복제 화면. GUI mirror = 원격 워크스페이스를 로컬 GUI 에 일반 워크스페이스로 띄운 것.
- **remote** — client 가 SSH 너머인 경우. tasty 는 자체 원격 프로토콜 없이 SSH 에 위임 → release CLI `tasty remote attach`. (로컬 self-attach 는 debug 전용, [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md).)
- **SSH 위임(SSH delegation)** — 시스템 `ssh` 바이너리에 원격 연결을 맡기는 기능. 프로세스 실행, 터널 수명, 원격 포트 찾기, 재시도 간격, 취소를 `tasty-ssh`가 담당하며 CLI와 GUI가 함께 사용한다. 포트 찾기·프로필 재감지·대화형 접속은 터널 기능과 구분한다.
- **원격 인스턴스 능력(remote capability)** — 원격 Tasty의 워크스페이스를 조회하고 만드는 기능. `tasty-remote`가 담당한다. `tasty-ssh`는 연결 방법, `tasty-remote`는 연결 후의 작업, `tasty-remote-profiles`는 이름으로 관리하는 연결 설정을 담당한다.

### CLI 명령 갈래 (→ [`crates/tasty-cli/src/dispatch.rs`](../../crates/tasty-cli/src/dispatch.rs))

- **단발 RPC(one-shot RPC)** — CLI 명령 하나가 JSON-RPC 요청 **하나**로 끝나는 갈래. 보내고 응답을 출력하면 끝이라 client 는 흐름을 주도하지 않는다. 대부분의 명령이 여기 속한다.
- **클라이언트 주도 실행(client-driven execution)** — 클라이언트가 여러 단계를 진행하는 명령. 로컬 파일·프로세스 작업(`tasty port`, `tool passkey`), raw 스트림(`remote attach`), 폴링(`plugin audit-follow`), SSH를 통한 조회(`remote workspaces`)가 해당한다. IPC를 여러 번 사용할 수도 있으므로 “로컬 실행”과 같은 뜻은 아니다.

## 기존 터미널과의 대응

Pane 은 tmux/iTerm2 에 대응 개념이 **없는** tasty 고유 설계다. 그래서 분할 정책을 두 레벨로 가진다.

| 동작 | tmux | iTerm2 | tasty |
|------|------|--------|-------|
| 화면 분할 위치 | window 고정 | tab 종속 | **두 레벨 선택** (상위=Pane / 하위=Surface) |
| 탭 전환 시 분할 | 유지 | 전환 | 상위 유지 + 하위 전환 |

| tasty | tmux | iTerm2 |
|-------|------|--------|
| Workspace | Session | Window |
| Pane | — | — |
| Tab | Window(탭) | Tab |
| Surface(terminal) | Pane | Pane(split) |

<a id="코드-심볼-크로스워크"></a>

## 용어와 코드 심볼의 대응

| 용어 | Rust 심볼 |
|------|-----------|
| App / Engine | `app::App` / `runtime::engine_session::EngineSession` |
| 구조 원본 / 읽기 트리 | `tasty_core::JournalModel` / `tasty_core::CoreState` (Workspace…SurfaceDescriptor) |
| View(상위) | `view::ui::View` (sealed trait) |
| View 계열 | `ModalView` supertrait(모달 외 구현체는 `View`+`sealed::Sealed` 직접 구현) |
| View 구현체 | `MainView` / `SettingsView` / `QuitView` / `PluginsView` / `PresetView` |
| 상위 레이아웃 | `PaneNode` (이진 트리: Leaf/Split) |
| Pane / Tab | `Pane` / `Tab` |
| 하위 레이아웃 | `SurfaceLayout` (이진 트리: Leaf/Split) |
| Surface | `Surface` trait; plugin surface 는 host 에 `RemoteSurface`(webview) / `EguiMeshSurface`(egui-mesh) 로 보관 |
| Popup / Toast / Banner | `PopupDef`+`PopupManager` / `ToastState`+`ToastManager` / `BannerDef`+`BannerManager` |
| 상태바 | `StatusBar` 계열 (`StatusBarData`/`StatusBarAction`/`draw_status_bar`) |
| 길이 타입 | `PhysicalPx` / `LogicalPx` (→ [typed-length.md](typed-length.md)) |
| 단발 RPC / 클라이언트 주도 실행 | `dispatch::Dispatch::Rpc` / `dispatch::Dispatch::ClientDriven`(+`ClientCommand`) |

## 관련

- [identity.md](../identity.md) — 정체성, [제품 원칙](../principles/index.md) — 판단 기준 (이 용어들의 *왜*)
- [typed-length.md](typed-length.md) — 길이 타입 newtype
