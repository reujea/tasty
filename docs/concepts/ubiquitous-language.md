# 유비쿼터스 언어 (통합 용어집)

tasty 의 코드·문서·IPC/CLI 표면 전체가 같은 용어를 쓴다. 이 문서는 **용어집(색인)** 이다 — 각 용어의 *정의 본체* 는 해당 개념 문서에 있고, 여기서는 한 줄 정의 + 정본(canonical) 링크만 둔다. 깊이가 필요하면 링크로 간다.

> 용어를 잘못 쓰면 코드·문서·API 일관성이 깨진다. 특히 **Window/View**, **Workspace/Pane/Tab/Surface** 계층, **상위/하위 레이아웃**, **Modal/Popup/Toast/Banner/Modifier-hint 오버레이** 구분을 혼동하지 않는다.

## 정본 문서

| 영역 | 정본 | 다루는 용어 |
|------|------|-------------|
| 주체 | [actors.md](actors.md) | 로컬 사용자 · AI Agent · 원격 사용자 · 점유 |
| 구조 | [hierarchy.md](hierarchy.md) | Engine · View · Workspace · Pane · Tab · Surface · 두 레벨 레이아웃 |
| 플러그인 | [plugins.md](plugins.md) | 배포/통합 축 · surface_kind · 권한 |
| attach | [`../dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) | server/client · mirror · lock |
| 원격 연결 | [`../features/remote-profiles/index.md`](../features/remote-profiles/index.md) | 원격 접속 프로필 · Passkey · kind |
| 훅 | [`../features/hooks/index.md`](../features/hooks/index.md) | Surface hook · Global hook · hook handler 레지스트리 |
| Claude plugin | [`../plugins/claude/index.md`](../plugins/claude/index.md) | Claude 세션 프로필 · reboot · spawn/respawn |

## 용어 한 줄 정의

### 주체 (→ [actors.md](actors.md))

- **로컬 사용자** — 이 머신에서 GUI 를 직접 쓰는 사람. 포커스의 주인. 점유 불필요. 점유를 끊을 수 있는 유일한 주체.
- **AI Agent** — IPC/CLI 로 tasty 를 조작하는 AI. 대상은 ID 직접 지정. **기본은 점유 없이** 동작하되 필요하면 점유(soft/hard)를 걸 수 있다(예: `terminal` child-terminal 의 soft 점유). **격리 계약**(부수효과가 사용자 상태에 안 닿음)을 따른다.
- **원격 접속 사용자** — SSH 너머에서 attach 로 접속하는 사람. 행동은 AI Agent 에 가깝고(연결 기반), **점유**라는 관문을 반드시 통과한다.
- **점유(occupation)** — 주체(원격 사용자 | AI Agent)가 surface/workspace 에 대해 선언하는 지속·가시 관계. **약한(soft: advisory 마커, write 허용) / 강한(hard: 배타 + 다른 주체 readonly; 원격 attach 가 사례)** 2계층(ADR-0040). 계층과 무관하게 대상→점유자는 1:1(배타), 주체→대상은 1:N. self-release 또는 로컬 사용자 force-detach 로만 해제.

### 원격 연결 (→ [features/remote-profiles](../features/remote-profiles/index.md))

- **원격 접속 프로필(Remote profile)** — 타입(`kind`, 열린 string) 태그가 붙은 범용 연결 디스크립터. 비밀을 담지 않고 Passkey 를 이름으로 참조만 한다. 2-레이어(ADR-0032): **`ssh`** = 순수 연결 정보, **`tasty-attach`** = attach 스펙(ssh 를 `ssh_ref` 로 참조하거나 인라인 + remote_tasty/port_mode/port_file). attach 는 tasty-attach kind 를 읽는 **소비자** — "주소 저장(ssh) ≠ attach 스펙(tasty-attach)".
- **Passkey** — 별도 named 자격증명 저장소. `kind = path`(파일 참조) | `inline`(0600 파일로 materialize). at-rest 는 항상 파일 경로(toml 에 비밀 0). 값은 로컬 GUI Reveal 로만 열람, IPC/agent 엔 영구 마스킹([ADR-0016](../adr/0016-passkey-store-path-convergence.md)).
- **미등록 타입** — core 내장(ssh/smb)도 설치 플러그인도 claim 하지 않는 `kind`. 등록은 허용하되 노란 배지로 경고.

### 구조 (→ [hierarchy.md](hierarchy.md))

- **Engine** — 진입점 + 서버. IPC 포트 소유, 모든 View 생명주기 관리. **headless 에선 View 없이 Engine + `CoreState` 만 동작.**
- **Window** — winit OS 창 자원(`winit::window::Window`). tasty 쪽 `Window` 타입은 **없다** — 이 단어는 OS 창만 가리킨다.
- **View** — tasty 쪽 윈도우 표현(종류+콘텐츠+행동). winit Window 를 소유. **1 View : 1 Window.** `MainView`/`SettingsView`/… 가 구현체.
- **CoreState** — 구조 계층의 도메인 트리(Workspace…Surface). **GUI 없이도 구성**되며, GUI 에선 `MainView` 가 이를 호스팅·투영.
- **Workspace** — 도메인 최상위 컨테이너. 사이드바에서 전환.
- **Workspace Category(카테고리/사이드바 폴더)** — 워크스페이스를 묶는 **그룹 계층**(사이드바 섹션). `workspace_categories_enabled` 설정으로 on/off. 예약 카테고리 **`normal`**(id `0`, `categories[0]` 위치 고정, rename/delete 불가)가 항상 존재하고, 미지정 워크스페이스의 기본 소속이다. 카테고리 *CRUD·reorder·소속 변경*은 에이전트 작업(IPC/CLI 양면, release) — *선택(active)·접힘 토글*은 사용자 UI 상태(IPC 노출 안 함). 정본 [`features/workspace-category`](../features/workspace-category/index.md).
- **Pane** — 독립 탭 바를 가진 영역. **상위 레이아웃**이 위치 결정(탭 무관 고정). tasty 고유.
- **Tab** — Pane 안의 탭 하나. 내부에 Surface 들의 **하위 레이아웃**을 가짐(탭 전환 시 함께 전환).
- **Surface** — 최하위 컨테이너. `surface_id` + **kind(타입)** 를 가짐. 닫기/포커스/리스트는 kind 무관 동일.
- **상위 레이아웃 / 하위 레이아웃** — Pane 배치(탭 무관) / Surface 배치(탭 종속). 두 레벨을 **둘 다** 제공하는 게 tasty 핵심 설계. 동의어: **PaneGroup** = 상위 레이아웃(`PaneNode`), **SurfaceGroup** = 하위 레이아웃(`SurfaceLayout`) — 코드 타입이 아니라 주석·문서가 쓰는 이름이다.

### 사용자 화면 표기

위 정의는 **개발자 어휘**다 — 영어 용어와 Rust 심볼을 잇는다. 사용자가 화면에서 읽는
낱말은 그것과 별개로 정한다. 한 개념이 화면에서 두 낱말로 불리면 사용자는 그것을 두 개로
읽는다.

| 용어 | 한국어 | 일본어 |
|---|---|---|
| Window | 윈도우 | ウィンドウ |
| Pane | 페인 | ペイン |
| Tab | 탭 | タブ |
| Surface | 서피스 | サーフェス |
| Workspace | 워크스페이스 | ワークスペース |

- **범위는 사용자 표면이다** — `lang/*.toml`(호스트 · 번들 plugin)과 `site/content` 의
  한국어 가이드. `docs/` 산문은 안 든다: 바로 위 Window 정의가 "winit OS 창 자원" 이라고
  쓰는 것처럼, 개발자 문서는 개념을 풀어 설명하느라 보통명사를 쓴다. 그쪽까지 한 낱말로
  모으려면 별개 결정이 필요하고 그 결정은 없다.
- **`창` 은 Window 를 가리킬 때만 금지다.** Popup 이나 Modal 을 가리키는 `창`(알림 창 ·
  Git 뷰어 창 · 확인 창)과 낱말이 다른 `주소창` 은 그대로 둔다 — 판정 기준은 아래 §View 내부
  의 Window / Popup 구분이고, OS 창을 소유하는 것(= `View` 구현)만 `윈도우` 다.
- **같은 동작의 이름도 한 낱말이다 — 일본어 "새 윈도우" 는 `新しいウィンドウ` 다.**
  한 파일 안에서 `[menu.macos]` 만 `新規ウィンドウ` 였고 `[tray]`·`[jump_list]` 는
  `新しいウィンドウ` 였다. `新規` 가 macOS 일본어 메뉴의 관례어라 그 절만 다른 것이
  의도일 수 있었는데, **그 관례를 저장소 안에서 확인할 자리가 없다** — tasty 는
  `with_default_menu(false)` 로 NSMenu 를 직접 소유해서 OS 가 제목을 주는 항목이 하나도
  없고, 그래서 Apple 의 표기는 저장소에도 런타임 화면에도 안 들어온다. 잴 수 없는 것을
  근거로 삼는 대신 저장소 안의 선례를 썼다: 한국어에서 같은 갈림을 설정 쪽 표기
  (`새 윈도우`)로 모았고, 일본어의 설정 쪽 표기가 `新しいウィンドウ:`
  (`settings.keybindings.new_window_label`) 다. 그 동작의 네 자리 중 셋이 이미 그 낱말이었다.
  ☆ 이 판정의 범위는 **"새 윈도우" 라는 동작 이름 하나**다. 일본어의 "새 X" 계열 전체가
  한 낱말이라는 뜻이 아니다 — `[pane_context_menu]` 는 `新規ターミナル` 계열이고
  `[explorer.tab]` 은 `新しいタブ` 다. 그 갈림은 여기서 판정하지 않았다.
- 집행: `crates/tasty-doc-guards/tests/one_concept_one_word_on_the_user_facing_surface.rs`.
  은퇴한 표기(`패인` · `ウインドウ`)는 예외 없이 0 이고, `창` 은 Window 가 아닌 자리만
  좌표와 근거를 붙여 그 파일의 명부에 남는다.

### View 내부 (→ hierarchy.md, `design/systems/`)

- **Modal** — 전역 1개, 활성 시 입력 차단하는 View 의 한 형태(별개 엔티티 아님). `SettingsView`/`QuitView`/`PluginsView`.
- **Popup** — View 내부 가상 창(타이틀바+콘텐츠, 드래그·z-order). 스코프 가짐. 상세 [`design/systems/popup.md`](../design/systems/popup.md).
- **Toast** — View 내부 휘발성 알림. 포커스 안 받고 입력 비소비. **사용자 행동에서만** 발사(에이전트 IPC 는 발사 안 함) — 예외는 attach mirror 의 원격 연결 상태 사건(끊김·재연결·손실·구조 전달 실패) 한 부류다. 상세 [`design/systems/toast.md`](../design/systems/toast.md).
- **Banner** — parent 스코프 상단에 떠서 **info + 조치(action)** 를 제공하는 지속·인터랙티브 오버레이. 포커스는 안 받지만 **마우스를 소비하고 내부 버튼을 가짐**(Toast/Popup 어디에도 안 맞는 4번째 개념). TTL·큐(스코프당 1+최대 5 대기)·계층 z-index. **사용자 행동에서만** 발사(에이전트 IPC 는 발사 안 함). 상세 [`design/systems/banner.md`](../design/systems/banner.md).
- **Modifier-hint 오버레이** — modifier 를 홀드하면(기본 **500ms**, **Shift 단독만 1200ms**) 200ms 페이드로 떠서 눌린 **조합을 포함하는(부분집합)** 조합의 단축키 목록을 보여주고 **키를 떼면 즉시 소멸**하는 오버레이. 조합을 좁혀 누르면 목록도 즉시 좁혀진다(Ctrl→Ctrl+Shift). **키보드 포커스를 절대 안 받고**(입력은 그대로 터미널로), **마우스만 소비**(드래그 이동·테두리/코너 리사이즈·X 닫기). Popup(포커스/타이틀바/z-order)도 Toast(비인터랙티브 TTL)도 Banner(상단 고정 action)도 아닌 **홀드 수명 + 마우스 인터랙티브 + focus-less** 의 5번째 개념. 홀드 상태는 winit `ModifiersChanged`(실사용자 입력)만 반영 — IPC/CLI 로 강제 표시 불가(원칙1). `enabled` 설정 off 면 전혀 안 뜸. 지오메트리(pos/size)는 사용자가 이동/리사이즈하면 `Settings::modifier_hint` 에 영속. 상세 [`design/systems/design-token-mapping.md`](../design/systems/design-token-mapping.md) 의 modifier-hint 절 · 콘텐츠 모델은 `src/adapters/ui/input/shortcuts/modifier_hint.rs`, 본체는 `src/adapters/ui/modifier_hint_overlay.rs`.
- **마커 오버레이(Marker overlay)** — 대상 위젯의 테두리를 건드리지 않고 **좌표(rect) 위에 독립된 floating 도형(링/glow)을 최상위 z 로 얹는** 오버레이. Modal/Popup/Toast/Banner/Modifier-hint 와 결정적으로 다른 점: **메시지·심각도 모델이 전혀 없는 순수 기하 마커**(Banner=info+action, Toast=message 와 대비) — 외부 로직(튜토리얼 런타임)이 좌표를 주입하면 그 위치를 링으로 그릴 뿐 의미를 담지 않는 **6번째 개념**. `pointer-events:none`(마커/scrim 은 클릭을 하위로 통과), 상호작용은 옆의 **안내 말풍선(callout)** 만 담당. 좌표는 매 프레임 `LayoutContext`/`terminal_rect`/`tab_bar_height` 로 재해석한다(정적 stale 없음). **사용자 행동에서만** 발사(도구 메뉴 → 튜토리얼 진입, Next 클릭 진행 — 에이전트 IPC/CLI 발화 API 없음, Toast/Banner/Modifier-hint 계열 · 원칙1). 현재 유일한 producer 는 튜토리얼. 상세 [`features/tutorial`](../features/tutorial/index.md) · 본체 `src/adapters/ui/tutorial/`.
- **전체화면 무대(Fullscreen stage)** — 창 전체를 독점하는 **독립 표면**. 기존 요소를 확대한 것이 아니라 Workspace/Pane/Tab/Surface 트리와 **병렬로** 존재하며, 뒤의 개체와 내부 로직상 연관이 없는 **별개 데이터**를 담는다("이 popup 을 전체화면으로" = 같은 형상의 별개 인스턴스를 무대에 구성). 무대가 유지되는 동안 뒤는 가려져 있으므로 redraw 하지 않고, 나올 때 다시 그린다. **창당 최대 1 개**(창이 여럿이면 창마다 독립), 정적 테이블(`StageDef`)에 선언된 것만 올라갈 수 있으며, 영속화하지 않는다. **사용자 행동에서만** 발사(진입은 popup 타이틀바 버튼 — 에이전트 표면은 debug 전용 `debug.fullscreen.*` 뿐이고 release 에는 없다, Toast/Banner/마커 계열 · 원칙1). Modal(입력 차단 View) / Popup(가상 창) 어디에도 안 맞는 7 번째 개념 — 포커스나 z-order 를 다투는 것이 아니라 프레임 자체를 갈아끼운다. 상세 [`design/systems/fullscreen-stage.md`](../design/systems/fullscreen-stage.md) · 근거 [ADR-0082](../adr/0082-fullscreen-independent-stage.md).
  - **Zoom 과 혼동 금지** — tasty 에서 `Zoom` 은 **UI 배율**(설정 › 단축키 › Zoom)로 이미 선점된 용어다. tmux 식 "pane zoom" 명칭을 쓰지 않고 **전체화면 / 무대(stage)** 로 통일한다.
- **상태바(Workspace status bar)** — 작업 영역 하단을 항상 차지하는 고정 strip(타이틀바 `top_inset` 과 대칭인 `bottom_inset`). focus surface 컨텍스트 표시 + 우측 빠른 액션(팔레트·테마). GUI 전용 표시 위젯(에이전트 표면 없음). 정본 [`features/workspace-status-bar`](../features/workspace-status-bar/index.md).

### Surface 주의 환기 (→ [`features/surface-highlight`](../features/surface-highlight/index.md))

- **Attention** — surface 가 "확인 대기(주의 환기)" 상태임을 나타내는 **producer 중립 공유 상태**(CoreState `attention: AttentionStore`, surface id → `{ kind, raised_at }`). **Notification 과는 별개 개념** — attention 레코드가 곧 알림 패널 아이템은 아니다(패널 노출 여부는 kind 별 정책 `effects_of().panel_item` 이 결정하고, 실제 패널 아이템 생성은 producer 가 `NotificationStore` 를 직접 호출해 만든다). surface 가 **실제 렌더 시점 포커스**를 얻으면 자동 해제(`gpu.rs`, 에이전트 주입 아님 → 불가침 원칙 1 안전). **여러 producer**(toast 알림, completion, Claude hook, OSC 133 명령 완료)가 발동시킬 수 있다 — 특정 producer 의 소유물이 아니다. kind 는 현재 `Completion`·`NeedsInput` 2종.
- **Highlight** — Attention 이 화면에 투영되는 **View 계층 이름**(effect 3채널: 테두리 강조 + 탭 제목 강조(yellow) + 소속 워크스페이스 우측 개수 배지). 소비처 함수/타입명(`draw_surface_highlights`, `SurfaceHighlightRegion` 등)은 이 이름을 그대로 쓴다 — Core 상태 이름(Attention)과 View 표시 이름(Highlight)이 의도적으로 분리돼 있다. Toast(휘발성 View 오버레이)와도 별개 개념: highlight 는 surface 에 붙는 지속 상태다.
- **Completion** — "surface 가 작업을 완료했다"는 이벤트/신호(release 정식 IPC/CLI: `surface.completion` · `tasty surface completion`). **Attention 의 kind 중 하나(`AttentionKind::Completion`)이자 그것을 발동하는 producer 중 하나일 뿐**이다 — completion ≠ attention. 에이전트가 자기 작업 결과를 보고하는 것이라 release 정당(PushNotification 과 동류). 향후 completion 고유 효과가 생기면 cascade 를 확장한다.

### Surface 종류 (→ [hierarchy.md](hierarchy.md#surface-타입) · [plugins.md](plugins.md))

- **host 내장** — `terminal`(PTY+GPU 셰이더) / `empty` / `dag_graph` / `explorer`(plugin 에서 host-native 로 역이전).
- **egui-mesh plugin** — `image` (plugin 이 `rendering=egui-mesh` 선언, plugin 프로세스가 tessellate 한 mesh 를 host 가 합성).
- **webview plugin** — `html` / `markdown`([ADR-0065](../adr/0065-markdown-webview-render-channel.md), Stage B 부터) — `rendering=webview`, host 의 네이티브 WebView 오버레이로 그림.

### Claude plugin (→ [plugins/claude](../plugins/claude/index.md))

- **Claude 세션 프로필(Claude session profile)** — `tasty claude spawn/respawn/launch/reboot` 가 `--settings <경로>` 로 Claude Code 에 주입하는 `settings.json` 조각. 두 가지 방식으로 지정한다: `--profile-file <경로>`(파일 직접 지정) 또는 `--profile <이름[,이름2,...]>`(아래 레지스트리에 등록해 둔 이름, 서로 상호 배타적). Claude Code 는 훅을 프로세스 기동 시 한 번만 읽으므로, 이미 떠 있는 세션에 새 훅을 걸 유일한 창구가 이 재기동 시점의 `--settings` 주입이다 — **대체가 아니라 추가**로 병합되어 tasty 내장 훅과 함께 발화한다. `reboot` 는 부착 상태(경로 또는 이름)를 surface meta 에 남겨 다음 무인자 reboot 가 승계하고, `--clear-profile` 로 뗀다.
  - **Claude 세션 프로필 레지스트리** — 이름으로 등록해 둔 프로필을 `--profile` 로 부착하는 계층(`crates/tasty-plugin-claude/src/profile.rs`). `tasty claude profile-register/-unregister/-list/-show/-current`. 이름을 둘 이상 동시 부착하면 `--settings` 반복 지정이 last-wins 인 함정을 피하기 위해 등록 내용을 하나의 파일로 **머지**한다(`profile_merge.rs`) — 훅 배열은 union, 객체는 키 단위 병합, `permissions.allow`/`deny` 는 union 후 **deny 가 allow 를 이김**, 그 외 스칼라는 last-wins(충돌 시 경고), `permissions.defaultMode` 는 충돌 시 거부. 호스트 레지스트리(`src/hook_handler/registry.rs` 등)의 형태(patch semantics · `<owner>/<short>` id)를 미러링하지만 소비자가 이 plugin 하나뿐이라 plugin 내부에 둔다(타입 공유 없음).
  아래 두 용어와 이름만 "프로필"을 공유할 뿐 서로 무관하다:
  - **원격 접속 프로필**(위 "원격 연결" 절) — SSH/attach 연결 디스크립터. Claude 세션과 무관.
  - **surface hook / hook handler**(위 "훅" 절) — tasty 가 소유한 이벤트→핸들러 바인딩. Claude 세션 프로필은 그 반대편, 즉 **Claude Code 프로세스 자신**의 훅 설정 파일이다 — tasty 의 hook handler 레지스트리를 거치지 않는다.

### attach (→ [attach-behavior.md](../dev-guide/attach-behavior.md))

- **server / client** — 점유당하는 쪽(PTY 권위 owner, 항상 loopback 으로만 받음) / 점유하는 쪽(원격성을 흡수). "로컬/원격" 은 **client 측 개념**.
- **mirror** — client 가 받은 출력으로 PTY 없이 재구성한 복제 화면. GUI mirror = 원격 워크스페이스를 로컬 GUI 에 일반 워크스페이스로 띄운 것.
- **remote** — client 가 SSH 너머인 경우. tasty 는 자체 원격 프로토콜 없이 SSH 에 위임 → release CLI `tasty remote attach`. (로컬 self-attach 는 debug 전용, [ADR-0007](../adr/0007-attach-targets-remote.md).)
- **SSH 위임(SSH delegation)** — 원격성을 흡수하는 client 측 계층 전체를 가리키는 말. tasty 어휘에서 **"SSH" 는 프로토콜 구현이 아니라 시스템 `ssh` 바이너리에 위임하는 행위**를 뜻한다 — 프로세스 spawn · 터널 수명 · 원격 포트 발견 · 백오프 · 취소가 여기 속한다. 그 계층의 거처가 `tasty-ssh` 크레이트(`crates/tasty-ssh/`)이고, 소비자는 CLI 와 본체 GUI 둘 다다. 터널은 이 계층의 **일부**이지 전부가 아니다(포트 발견·프로필 재감지·대화형 접속은 터널이 아니다).
- **원격 인스턴스 능력(remote capability)** — SSH 위임 *위에* 얹혀 원격 tasty 인스턴스에 실제로 말을 거는 층 — 워크스페이스 **조회(browse)** 와 **생성(create)**. 거처는 `tasty-remote` 크레이트다. 이름이 비슷한 셋을 구분한다: `tasty-ssh`(어떻게 닿는가) → `tasty-remote`(닿아서 무엇을 하는가) → `tasty-remote-profiles`(어디에 닿을지를 이름으로 저장해 둔 레지스트리, 위 "원격 접속 프로필" 항목).

### CLI 명령 갈래 (→ [`crates/tasty-cli/src/dispatch.rs`](../../crates/tasty-cli/src/dispatch.rs))

- **단발 RPC(one-shot RPC)** — CLI 명령 하나가 JSON-RPC 요청 **하나**로 끝나는 갈래. 보내고 응답을 출력하면 끝이라 client 는 흐름을 주도하지 않는다. 대부분의 명령이 여기 속한다.
- **클라이언트 주도 실행(client-driven execution)** — 단발 RPC 로 끝나지 않고 **client 가 흐름을 쥐는** 갈래. 로컬 파일·프로세스 조작(`tasty port`, `tool passkey`), raw 스트림(`remote attach`), 폴링 루프(`plugin audit-follow`), SSH 터널 경유 조회(`remote workspaces`)가 전부 여기다. "client" 는 위 attach 절의 그 client 와 같은 뜻이다 — 여기서 강조하는 축은 **주도권**이지 통신 유무가 아니다. 그래서 "로컬(local)" 로 부르지 않는다: 이 갈래의 절반은 IPC 를 (여러 번) 탄다.

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

## 코드 심볼 크로스워크

| 용어 | Rust 심볼 |
|------|-----------|
| Engine | `core::Core` + `core::CoreState` |
| 구조 도메인 트리 | `core::CoreState` (Workspace…Surface 보유) |
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

- [identity.md](../identity.md) — 정체성·불가침 원칙 (이 용어들의 *왜*)
- [typed-length.md](typed-length.md) — 길이 타입 newtype
