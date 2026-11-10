# 작업 영역 (Work area)

- **Status**: Implemented
- **주체**: 로컬 사용자(GUI 직접) · AI Agent(IPC/CLI 로 ID 지정 조작) · 원격 접속 사용자(surface/workspace 점유)
- **설계**: 없음
- **코드**: `crates/tasty-model/` (`workspace.rs`/`pane.rs`/`tab.rs`/`surface_layout.rs`/`pane_tree.rs`), `crates/tasty-core/src/state.rs` (`CoreState`), `src/runtime/engine_session.rs` (`EngineSession`), `src/state/navigation.rs` (View 선택 `NavigationState`), `src/state/` (`workspace.rs`/`pane.rs`/`tab.rs` 요청 구성)
- **화면**: [아래 절](#화면)

## 목적

[MainView](../main-view/index.md) 의 중앙 — 사용자가 실제로 일하는 영역. **Workspace › Pane › Tab › Surface** 포함 관계와 그 위의 **두 레벨 레이아웃**을 보유한다. 이 도메인은 GUI 없이도 성립하는 `CoreState` 의 본체이며, MainView는 이 상태를 화면에 표시한다 (→ [헤드리스 정의 경계](../../dev-guide/headless-build-boundaries.md)). 계층 용어 자체는 [구조 계층](../../concepts/hierarchy.md).

## 내부 동작 (headless-valid)

### 도메인 트리

`CoreState` 가 로컬 워크스페이스와 attach mirror 워크스페이스를 확정 구조로 들고, 각 객체는 아래로 중첩된다. 두 군데에 **이진 트리(분할 트리)** 가 있다 — 상위(Pane)와 하위(Surface).

```
CoreState
└── Workspace          (Vec — 사이드바로 전환)
    └── PaneNode        ← 상위 레이아웃: Pane 들의 이진 분할 트리 (탭 무관)
        └── Pane         (탭 바 하나를 가진 화면 영역)
            └── Tab       (Vec — active_tab 하나가 활성)
                └── SurfaceLayout   ← 하위 레이아웃: Surface 들의 이진 분할 트리 (탭 종속)
                    └── SurfaceDescriptor  (leaf — surface ID 와 kind)
```

이 트리는 구조만 가진다. 실행 자원과 관측값, 사용자의 선택은 다른 소유자에 있다.

| 상태 | 소유자 |
|---|---|
| 구조 트리(워크스페이스·pane·탭·분할·leaf) | `CoreState` — journal 이 확정한 projection |
| surface 실행 객체(`Surface` trait 구현), PTY, kind registry | `EngineSession` 의 runtime(`EngineRuntime`) |
| 탭 제목 관측값(OSC 제목·cwd 이름), 점유, 알림, busy | `EngineSession.live`(`LiveDomainState`, `src/core/live.rs`) |
| 선택된 워크스페이스·pane·탭·surface, 분할 노드별 선택 힌트 | View 의 `NavigationState`(`src/state/navigation.rs`) — 창마다 따로 둔다 |

선택이 View 에 있는 이유와 ID 지정 원칙은 [포커스 정책 (운영 상세)](../../design/policies/focus.md)에 있다.

### Workspace

MainView 의 최상위 컨테이너. 한 MainView 가 **여러 개**를 갖고 사이드바에서 전환한다.

- 필드: `id` · `name` · `subtitle` · `description` · 상위 레이아웃 `PaneNode` · `mirror`(원격을 attach 한 client mirror 인지 — 사이드바 REMOTE 표시/레일 corner chip 으로 구분) · `remote_row`(저장된 원격 행의 연결 대상, attach 프로필과 원격 workspace id) · `category`(사이드바 카테고리).
- 포커스된 Pane 은 워크스페이스 필드가 아니라 View 의 `NavigationState` 가 워크스페이스마다 기억한다 — 전환해도 각자의 포커스가 보존된다.
- 변형: 일반 워크스페이스 / **mirror 워크스페이스**(원격 attach 의 client측). mirror 의 형상은 런타임 전용이고, 원격 연결 팝업에서 붙인 mirror 는 [저장된 원격 행](../remote-attach/index.md#저장된-원격-행과-mirror-내용)으로 남아 재시작 뒤 다시 연결한다, attach 의 점유 모델은 [actors](../../concepts/actors.md#점유-occupation-모델).

### Pane — 상위 레이아웃 (탭 무관)

Pane 은 **독립적인 탭 바를 가진 화면 영역**이다. Workspace 안에서 Pane 들의 배치는 `PaneNode` 이진 트리(`Leaf(Pane)` | `Split { direction, ratio, first, second }`)로 결정되고, **탭 전환과 무관하게 고정**된다 — tmux 의 "분할이 window 에 고정" 에 대응.

- 필드: `id` · `tabs: Vec<Tab>`. 탭 바 가로 스크롤은 도메인 Pane과 별도로 View가 pane ID별 논리 픽셀 값으로 소유한다.
- 탭 동작: 추가(`add_*_tab`, 활성/백그라운드) · 닫기(`close_tab`/`close_tab_by_id` — **마지막 탭은 못 닫음**, `active_tab` 은 제거 위치 기준으로 자동 보정돼 보던 탭을 계속 가리킨다) · 전환(`goto_tab`/`next_tab`/`prev_tab`) · 이동(`move_tab`, `active_tab` 자동 보정).
- **활성 탭 추종 스크롤**: 탭이 많아 탭 바에 좌우 화살표가 뜬 상태에서 전환(단축키·클릭 공통)하거나 pane 이 리사이즈돼 활성 탭이 뷰포트 밖으로 밀려나면 View의 pane ID별 `tab_bar_scroll`을 자동 보정해 다시 보이게 한다(`src/adapters/ui/tab_bar.rs` `TabBarAction::AutoScrollToActiveTab`). 활성 인덱스/지오메트리가 실제로 바뀐 시점에만 보정하므로, 사용자가 화살표로 수동 스크롤해 둔 상태(활성 탭 불변)는 덮어쓰지 않는다.
- 분할(상위): `PaneNode::split_pane_in_place` 로 Pane 을 좌우/상하로 쪼갠다. 새 Pane 의 PTY 는 구조 변경 *전에* 미리 생성(트리가 빈 store 상태를 보지 않도록).

### Tab — 하위 레이아웃 (탭 종속)

Pane 안의 탭 하나. 내부에 Surface 들의 `SurfaceLayout` 이진 트리(`Leaf(SurfaceDescriptor)` | `Split { direction, ratio, first, second, node_id }`)를 가진다. `node_id` 는 실행 중에만 안정적인 분할 노드 식별자이며, 어느 쪽을 선택했는지는 View 가 이 ID 로 기억한다. **탭을 전환하면 이 분할 전체가 함께 전환**된다 — iTerm2 의 "분할이 tab 에 종속" 에 대응.

- 필드: `id` · `name`(자동 생성, 예 "Shell") · `explicit_name`(명시 지정 — 최우선) · `layout_opt`(`SurfaceLayout`. 구조 변경 중에만 비어 있다).
- **표시명 우선순위**: `explicit_name` > OSC 제목 > cwd 이름 > `name`(`Tab::display_name`). OSC 제목과 cwd 이름은 탭이 아니라 surface 마다 `LiveDomainState.surface_titles` 에 관측값으로 저장된다(`refresh_tab_osc_title` · `refresh_tab_display_name`). cwd 변경 시 shell prompt 가 새 OSC title 을 보내면 cwd가 반영. `explicit_name` 은 cwd/OSC 로 덮이지 않음(에이전트 `tab.create --name`).
- **셸 exe 경로 형태의 OSC 제목은 소스에서 무시된다.** ConPTY 는 spawn 시 콘솔 기본 제목으로 셸 실행파일 경로(예 `C:\Program Files/Git/bin/bash.exe`)를 OSC 0/2로 보내는데, 이 값이 surface 의 OSC 제목 관측값에 고정되면 복구 후 탭 제목이 exe 경로로 보인다. `map_osc`(`crates/tasty-terminal/src/vte_handler/osc.rs`)가 "경로 형태(`/`·`\`·`:` 포함) + basename 이 known shell(`is_known_shell_name`)" 인 제목을 `current_title` 세팅·`TitleChanged` 발생 없이 버린다 — bare `bash` 같은 비경로 제목은 통과(오탐 방지). 보강으로 Windows 빌트인 bashrc(`__tasty_title`)가 매 프롬프트에 cwd 기반 이름(`~`/`/`/basename)을 OSC 0으로 보내 `osc_title` 도 cwd 를 따른다.
- **탭 제목은 그 View 가 탭에서 선택한 surface 의 관측값만 쓴다**(`tab_display_name(tab, navigation.surface_id(tab))`). 한 탭 안의 병렬 surface(split) 중 선택되지 않은 surface 가 OSC 0/2를 보내도 탭 제목은 흔들리지 않는다(last-writer-wins flicker 방지).
  판정 기준은 **탭별 선택 surface** 이며 앱-전역 포커스가 아니다 — 배경 탭도 자기 선택 surface 의 title 을 계속 반영한다.
  관측값이 surface 마다 남아 있으므로 선택이 다른 surface 로 바뀌면 그 surface 의 최신 값이 곧바로 쓰인다. 선택 surface 가 제목이 없으면(non-terminal 등) cwd 이름 → `name` 순으로 내려간다.
  (`SurfaceTitleChanged` host event 는 선택되지 않은 surface가 제목을 보낼 때도 surface 단위로 발생 — plugin 호환 유지.)
- 하위 동작: surface 닫기(`close_surface`) · 방향 포커스 이동(`directional_focus`) · 분할(`split_surface_by_id`).

### Surface

Tab 의 SurfaceLayout 트리 leaf, 최하위 컨테이너. 트리에는 고유 `surface_id` 와 **타입(kind)** 만 담은 `SurfaceDescriptor` 가 있고, 실행 객체는 runtime 이 같은 ID 로 가진다. 실행 객체가 구현하는 `Surface` trait 의 핵심: `kind()`(불변 식별자) · `type_name()`(표시 라벨) · `surface_id()` · `source_cwd()`(새 surface 생성 시 상속할 시작 cwd — Surface cwd invariant, [`design/policies/cwd` Surface cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant)) · `display_name()`. 닫기/포커스/리스트 동작은 타입과 무관하게 동일하다.

#### 아직 콘텐츠가 없는 자리

빈 칸, attach mirror 의 plugin 대기 자리, 앱 재시작 뒤 아직 활성화하지 않은 복원 자리는 모두 `empty` kind 로 트리에 들어간다. 세 자리의 차이, 활성화 시점(부팅 단계·화면에 보일 때·`tasty wake`), 실패 상한, IPC 보고 형태는 [empty surface](../../surfaces/empty/index.md)에 있다.

### 두 레벨 레이아웃 (tasty 핵심 설계)

기존 도구는 분할 정책이 하나뿐(tmux=window 고정, iTerm2=tab 종속). tasty 는 **둘 다** 제공한다:

- **상위 레이아웃 (탭 무관)** — Workspace 안에서 `PaneNode` 로 **Pane** 배치. 탭을 바꿔도 이 분할은 고정. 화면을 물리 영역으로 나눠 각 영역이 독립적으로 탭을 전환.
- **하위 레이아웃 (탭 종속)** — Tab 안에서 `SurfaceLayout` 으로 **Surface** 배치. 탭 전환 시 함께 전환. 한 탭에서 여러 surface 동시 표시.

예: 상위로 좌우 Pane 분할 — 왼쪽은 Claude Code 전용, 오른쪽은 탭 여럿(logs/build). 오른쪽 탭을 바꿔도 왼쪽 Claude 는 영향 없음.

### Surface 종류

`kind()` 가 식별자, `type_name()` 이 표시 라벨. 세 출처가 있다 — host 내장, plugin 이 egui-mesh 로 자가 렌더하는 것(EguiMeshSurface), webview overlay 로 그려지는 것(RemoteSurface). kind 마다의 명세는 [Surface 종류](../../surfaces/index.md)에 있고, 이 절은 등록과 렌더 분기의 공통 규칙만 다룬다.

| kind | type_name | 출처 | 렌더 | 비고 |
|------|-----------|------|------|------|
| `terminal` | Terminal | **host 내장** | GPU 셰이더 | 쉘 PTY. 재시작 뒤 지연 활성화 가능(`empty` 의 복원 자리) |
| `empty` | Empty | **host 내장** | egui | 빈 자리(변환 버튼). plugin 대기 자리·복원 자리도 이 kind |
| `markdown` | Remote | `com.tasty.markdown` plugin (`rendering=webview`) | 네이티브 WebView overlay — plugin 이 sanitize HTML 문서 생성(`RemoteSurface`) | [WebView 호스트 계약](../../design/systems/webview.md). 대용량/파일열기 확인 popup 2개는 surface 가 아니라 egui-mesh popup 기여 |
| `image` | EguiMesh | `com.tasty.image` plugin (`rendering=egui-mesh`) | plugin 자가 렌더 mesh (비트맵=egui 텍스처) | egui-mesh 등록(api_version·이름 충돌) |
| `explorer` | Explorer | **host 내장** | egui | host builtin surface |
| `dag_graph` | DAG | **host 내장** | egui | agent task DAG 뷰 ([agent-collaboration](../agent-collaboration/index.md)) |
| `html` | Remote | `com.tasty.html` plugin (`rendering=webview`) | 네이티브 WebView overlay (`RemoteSurface`) | plugin 은 URL/navigation 만 제어 |
| `mesh_demo` | EguiMesh | `com.tasty.mesh-demo` plugin (`rendering=egui-mesh`) | plugin 자가 렌더 mesh | 개발/검증용. 매니페스트가 `bundle = false` 라 배포 패키징에는 안 들어간다 |

- **host 내장**은 `register_builtin_kinds`(`terminal`/`empty`/`explorer`/`dag_graph`) 가 부팅 시 등록.
- **egui-mesh plugin surface**(`image`, 개발용 `mesh_demo`)는 매니페스트의 `[[surface_kinds]]` 가 `rendering="egui-mesh"` 로 선언하고 api_version 이 일치하며 kind 이름이 host 내장·다른 plugin 의 등록과 충돌하지 않으면 `EguiMeshSurface` stand-in 으로 등록된다. 커스텀 plugin 도 같은 조건이다 — 콘텐츠는 plugin 프로세스가 tessellate 한 mesh 를 host 가 합성 (docs/dev-guide/egui-mesh-channel.md). markdown 의 확인 popup 2개는 같은 egui-mesh 채널을 쓰지만 `[[contributes.popup]]` 기여이며 surface kind 로 등록되지 않는다([popup 구현](../../dev-guide/popup-implementation.md)).
- **webview plugin**(`html`/`markdown`)은 `RemoteSurface` stand-in 위에 host 가 native WebView overlay 를 자동 관리한다. `html` 은 `webview.set_url` IPC 로 URL/navigation 만 제어하고, `markdown` 은 plugin 이 직접 sanitize 된 HTML 문서 전체를 생성해 로드시킨다([WebView 호스트 계약](../../design/systems/webview.md)).
  - **overlay 생성에 실패하면 그 surface 는 비어 있고, 앱은 계속 돈다.** 실패는 두 종류로 갈린다 — 다음 시도에 달라질 수 있는 것(서버 자원 고갈 등)은 상한까지 다시 시도하고, 이 프로세스에서 달라지지 않는 것(창 종류·라이브러리 부재 등)은 한 번에 포기한다. 어느 쪽이든 시도 횟수에 상한이 있어 실패가 무한히 반복되지 않는다. 로그에는 첫 실패와 포기하는 순간만 남고, 포기 줄이 실제로 몇 번 시도했는지를 적는다. 그 surface 를 닫았다 다시 열면 시도 예산도 새로 생긴다. 근거·재검토 조건은 [WebView 호스트 계약](../../design/systems/webview.md).
  - webview kind 는 **탭 내부 분할(SurfaceGroup)의 어느 leaf 에서도** 동작한다. host 는 탭의 `SurfaceLayout` 트리 전체를 순회해 URL 을 가진 leaf 마다 overlay 를 만들고(포커스 leaf 로 한정하지 않는다), overlay 의 bounds 는 pane 전체가 아니라 `SurfaceLayout::compute_rects` 가 준 **그 leaf 의 rect** 다 — 같은 탭의 옆 surface 를 덮지 않는다. Linux 에서는 overlay 가 leaf rect 를 꽉 채운다. native 창은 마우스를 직접 받으므로, pane·surface 분할선의 hit 띠(`DIVIDER_HIT_THRESHOLD` 양쪽)와 창 가장자리 리사이즈 밴드(`RESIZE_EDGE_MARGIN`, 최대화·전체화면이면 없음)에 겹치는 부분만 X input shape 에서 뺀다(`state::webview_edges`). 빼는 픽셀은 host 의 분할선·리사이즈 판정이 보는 픽셀과 같다. 그 띠의 포인터 입력은 host 가 받고, 띠 밖의 클릭·스크롤은 페이지로 간다. macOS·Windows 는 아직 input shape 에 해당하는 처리가 없어, leaf 의 변이 **pane 콘텐츠 영역 외곽에 닿을 때만** divider 드래그용 4px inset 을 두고 분할된 leaf 사이 내부 경계에는 divider gap 만 둔다. 그 OS 의 webview chrome 은 배경·테두리를 leaf 전체에 그리고 안내 글자는 WebView 가 덮는 영역 안으로 자르는 코드 경로를 쓴다(그 OS 실 기기에서는 미측정). `webview.set_url` / `webview.navigation_attempt` 의 surface 조회도 같은 기준이라 비포커스 leaf 도 도달한다.
- 새 kind 는 `SurfaceKindRegistry` 에 동적 등록 — plugin 이 hello 후 추가 가능.
- **등록된 kind 는 `surface.kinds` / `tasty list surface-kinds` 로 묻는다.** 이 조회가 읽는 것은 매니페스트가 아니라 `SurfaceKindRegistry` — 런타임의 사실이다. 여기 나오는 kind 가 정확히 `--type <kind>` 로 만들 수 있는 kind 이고(끄거나 지운 plugin 의 kind 는 곧바로 빠진다 — [플러그인 제작 가이드](../../dev-guide/plugin-development.md)), host 내장 4 종도 함께 나온다(그쪽은 plugin 이 아니라 어느 `plugin.*` 조회에도 안 나온다). 칸은 kind · 표시명 i18n 키 · icon · **실제** 렌더 경로(`rendering`: `host-egui`/`egui-mesh`/`webview`/`remote`) · 출처(`source`: `host`/`plugin` + `plugin_id`) · 필수 params 다.
- **`plugin.show` 는 선언과 사실을 갈라 낸다.** `declared_rendering` 이 매니페스트가 요청한 값이고, `registered` 가 그 선언이 **이 plugin 의 것으로** 등록됐는지, `effective_rendering` 이 등록됐을 때 host 가 실제로 쓰는 경로다. kind 이름이 registry 에 있는데 소유자가 다르면(host 내장 kind 를 remote 로 재선언 · 다른 plugin 이 먼저 등록) `registered` 는 false 이고 소유자가 `registered_by`로 나온다. 헤드리스도 `webview`·`remote`·`egui-mesh` 선언을 GUI 와 같이 모두 등록한다(`src/boot/headless_plugins.rs` 의 `register_one_surface_kind` — 서버는 원문·제어를 맡고 렌더는 attach 한 클라이언트가 할 수 있다). 그래서 이 갈림은 kind 소유가 겹칠 때만 난다. `plugin.list` 는 여전히 kind **이름만** 배열로 준다.
- kind 각각의 동작은 [Surface 종류](../../surfaces/index.md)(terminal/empty/explorer/dag_graph/markdown/image/html). 분류 축·렌더 분기 개념은 [concepts/plugins](../../concepts/plugins.md).

## 인터페이스

- **AI Agent (IPC/CLI)**: 작업 영역의 도메인을 ID 로 직접 조작.
  - 생성: `tasty new workspace [--surface <S>]` · `tasty new tab --pane <P> [--type terminal|markdown|explorer|html|image]`. `--surface` 는 새 워크스페이스를 **그 surface 를 가진 창**에 만든다(IPC `workspace.create` 의 `surface_id` — 라우터가 주인 창을 고르고, 그 surface 를 가진 창이 없으면 포커스로 새지 않고 거절한다). 생략하면 사용자가 보고 있는 창이다([CLI 크레이트 내부 구조 — 세 갈래 대칭](../../dev-guide/cli-structure.md)). `--cwd` 를 생략한 terminal 워크스페이스는 `--surface` 를 주면 **그 surface** 의 cwd 를, 안 주면 그 창의 포커스 surface 의 cwd 를 상속한다(`inherit_cwd` 설정이 켜져 있을 때 · [CLI 크레이트 내부 구조 — 세 갈래 대칭](../../dev-guide/cli-structure.md)).
    에이전트가 만든 탭은 kind 와 무관하게 선택되지 않는다 — pane 의 `active_tab` 과 포커스가 그대로다([focus 정책](../../design/policies/focus.md) "에이전트가 만든 탭과 선택").
  - 분할: `tasty split --level pane|surface [--surface <S>] [--pane <P>] [--direction …]` (상위/하위 레이아웃 각각, `--pane` 은 `--level pane` 전용).
  - 닫기: `tasty close tab|pane|surface --… <ID>` · `tasty close workspace --workspace <W>`(안의 모든 pane/tab/surface 포함) · `tasty close window --window <N>`.
    워크스페이스 닫기는 마지막 하나, mirror 워크스페이스, **원격 attach 가 하드 점유 중인 surface 를 든 워크스페이스**를 거부한다 — 창까지 없앨지는 별개의 결정이라 `close window` 로 명시하고(헤드리스는 `window.close` 가 없어 거절 문구가 그것을 권하지 않고 마지막 워크스페이스를 닫을 수 없다고만 말한다), mirror 는 attach 세션 쪽에서 거두며, 점유 중인 터미널은 그것을 쓰고 있는 원격 세션이 놓아야 닫힌다. **되돌릴 수 없다**(안의 터미널이 죽고 되돌리기 스택·스크롤백에 남지 않는다). 경계와 근거는 [포커스 정책 (운영 상세)](../../design/policies/focus.md).
    사용자가 보고 있지 않은 워크스페이스를 닫아도 화면에 있는 워크스페이스는 그대로다([포커스 독립성](../../design/policies/focus.md)).
  - 조회: `tasty list workspaces|panes|surfaces` · `tasty list tabs --pane <P>` (전 워크스페이스 순회, 포커스 무관 — [포커스 독립성](../../design/policies/focus.md#cliipc-포커스-독립-원칙)).
- **사용자 트리거**: 단축키/마우스로 탭 추가·전환·이동, Pane/Surface 분할, 닫기. (단축키는 `KeybindingSettings` — 하드코딩 금지.)
- **원격 / 점유**: **Workspace 와 Surface 는 점유(attach) 대상**이다. 원격 접속 사용자가 attach 로 배타 **점유**하면 그 대상은 점유자만 조작하고 로컬·AI 는 readonly 가 된다. 점유된 surface 는 트리에서 원본 kind(terminal)를 그대로 유지하고, 점유는 `OccupancyRegistry`(`is_hard_occupied`)가 추적하며 서버측은 readonly mirror 오버레이(`src/remote/readonly.rs`)로 렌더 + 로컬 입력을 차단한다(전용 트리 marker kind 없음). 점유된 워크스페이스는 mirror 면 사이드바의 하늘색 REMOTE 표시(레일=우하단 corner chip)로 구분된다. 동작은 [remote-attach](../remote-attach/index.md), 개념은 [actors 점유](../../concepts/actors.md#점유-occupation-모델).

## split 명령

IPC/CLI 모두 **단일 `split` 명령**으로 상위(Pane)/하위(Surface) 레이아웃 분할을 통합한다(위 [내부 동작](#내부-동작-headless-valid)의 두 레벨 레이아웃).

```bash
tasty split --level surface --surface this --direction vertical --meta '{"nickname":"logs"}'
tasty split --level pane --pane 2 --direction horizontal
tasty split --level pane --surface this --type markdown --file /path/doc.md
```

### 파라미터

| 파라미터 | 필수 | 설명 |
|----------|------|------|
| `level` | yes | `pane` \| `surface` |
| `target_surface` | * | surface ID / `"this"` / nickname |
| `target_pane` | * | pane ID (pane level 만) |
| `direction` | no | `vertical`(기본) \| `horizontal` |
| `type` | no | `terminal`(기본) \| `markdown` \| `explorer` \| `html` + plugin kind |
| `file`/`path`/`url` | type별 | markdown=file 필수, explorer=path, html=url 필수 |
| `file_base` | no | 상대 `file`의 기준 폴더. CLI가 호출자 폴더를 싣는다. 로컬 대상이면 호스트가 `file`을 절대 경로로 바꾸고, mirror 대상이면 `file`을 그대로 원격에 보낸다. 플러그인과 원격에는 전달하지 않는다 |
| `cwd` | no | 터미널 작업 디렉토리 |
| `meta` | no | 새 surface 에 설정할 메타데이터(JSON) |

`target_surface` 와 `target_pane` 중 정확히 하나(둘 다 지정 시 에러). ID 는 전역 고유 → target 주어지면 **전 workspace 검색**.

#### target 해석

- `target_surface`: 숫자=ID 직접 / `"this"`=`TASTY_SURFACE_ID` 환경변수(자기 surface) / 문자열=surface_meta `nickname` 검색.
- level별: pane+target_surface = surface 가 속한 pane 옆 분할 / pane+target_pane = 그 pane 옆 / surface+target_surface = 그 surface 내부 분할 / **surface+target_pane = 에러**.

### 분할 비율과 탐색기 칸 하한

새 분할의 비율은 0.5 다. 다만 그 split 으로 탐색기 칸이 `explorer_min_height`(기본 180px, UI zoom 을 곱해 반올림한 논리 px) 아래가 되면, 하한을 지키는 비율 가운데 0.5 에 가장 가까운 값으로 분할선을 옮긴다. 판정과 보정은 엔진이 분할을 확정할 때 한 번 한다(`src/app/journal/commands/split_floor.rs`). IPC·CLI·분할 메뉴·단축키가 모두 이 경로를 지난다.

- 판정 대상은 split 으로 높이가 바뀌는 탐색기 칸이다. surface 분할에서는 대상 surface 와 새 surface 다. pane 분할에서는 대상 pane 의 모든 탭(활성 탭이 아니어도)에 있는 탐색기 칸과 새 pane 의 칸이다. 다른 칸은 보지 않는다.
- 이미 하한보다 낮은 탐색기 칸(창이 작아서)은 지금 높이 아래로만 줄지 않으면 된다. 그래서 옆으로 나누는 split 처럼 높이가 바뀌지 않는 분할은 그대로 받는다. 분할선 드래그 하한과 같은 규칙이다.
- 하한은 탐색기 칸에만 있다. 다른 종류의 칸은 분할선 드래그와 창 크기 변경에서 최소가 0 이다. 다만 탐색기가 걸린 새 분할(대상이나 새 칸이 탐색기)은 탐색기가 아닌 쪽 칸을 `split_sibling_min_height`(기본 56px = pane 탭 줄 28 + 한 줄 28, UI zoom 적용) 아래로 만들지 않는다. pane 분할은 탭 줄을 포함한 pane 높이로, surface 분할은 그 칸 높이로 잰다. 비율 0~1 안에서 모든 탐색기 칸의 하한과 이 최소를 함께 지키는 비율이 없으면 split 을 거절하고 레이아웃을 바꾸지 않는다. 예: 두 탐색기 칸이 모두 하한을 지킬 높이가 없을 때, 탐색기를 하한에 맞추면 다른 종류의 형제 칸이 56px 아래가 될 때. 탐색기가 걸리지 않은 분할은 비율을 고치지 않는다.
- 거절 응답(IPC·CLI·plugin 등 에이전트 split): `-32602`, 메시지 `split refused: not enough room`, `error.data` `{"reason": "explorer_min_height", "explorer_min_height": <N>}`. `<N>` 은 UI zoom 을 곱한 하한(기본 180, 예: zoom 1.25 면 225)이며 `data` 에는 실수(`180.0`)로 실린다. CLI 는 이 메시지와 `data:` 줄을 stderr 에 내고 종료 코드 1 로 끝난다. 에이전트 split 의 거절은 토스트를 띄우지 않고 포커스도 바꾸지 않는다.
- 분할 메뉴·단축키(사용자) split 이 거절되면 레이아웃은 그대로이고, 그 창에 info 토스트 "Not enough room to split this pane"(`toast.split_refused`, 모든 내장 언어 번역 있음)를 표준 토스트 표시 시간 동안 띄운다. 토스트 문구에는 숫자가 없다.
- 픽셀 높이는 split 대상 칸이 속한 workspace 를 지금 보여 주는 창의 터미널 영역·탭 바 높이·배율로 계산한다. 그런 창이 여럿이면 배율을 걷어 낸 터미널 영역 넓이가 가장 큰 창으로 판정한다. 창이 journal 을 돌릴 때마다 창이 보여 주는 workspace 와 함께 엔진별로 기록하며 journal 에 저장하지 않는다. 그 workspace 를 보여 주는 창이 없으면(다른 workspace 를 보고 있거나 헤드리스) 하한 없이 0.5 로 나눈다. 칸 크기는 다음 레이아웃 패스의 기존 규칙을 따른다.
- attach mirror 워크스페이스의 split 은 원격(서버) 엔진으로 넘어가므로 서버 창의 크기로 판정한다(서버에 그 엔진을 보여 주는 창이 없으면 하한 없이 0.5). client 창이 서버 창보다 낮으면 client 화면의 탐색기 칸은 180 아래로 보일 수 있다.
- 창 크기 변경은 하한을 지키지 않는다([탐색기](../../surfaces/explorer/index.md)의 compact 상태가 그 경우를 맡는다).
- Windows 에서 창을 최소화하면 장식 없는 Tasty 창은 작업 표시줄 아이콘 크기(측정 160x28)를 client 크기로 보고한다. 이 값은 창 크기 변경으로 보지 않는다. 최소화 중에는 표면과 터미널 격자(PTY 크기)를 그대로 두고, 복원할 때 오는 크기로 맞춘다.

### cwd 결정 (우선순위)

1. 호출자가 명시한 `cwd`(IPC/CLI).
2. `inherit_cwd` 켜져 있으면 source surface 의 `Surface::source_cwd()`.
3. 그 외 None → 셸 home.

source 별 `source_cwd()` 는 [cwd 정책](../../design/policies/cwd.md). 이 정책은 새 탭·새 워크스페이스·pane/surface 분할·타입 변환 등 **모든 생성 경로**에 동일 적용(carry invariant: [surface-cwd](../../design/policies/cwd.md#surface-cwd-invariant)).

### 포커스 정책

**split 은 포커스를 이동하지 않는다.** workspace.create/tab.create 도 IPC/CLI 시 포커스 유지:

| 동작 | UI(키보드/클릭) | IPC/CLI |
|------|-----------------|---------|
| split / workspace 생성 / tab 생성 | 새 영역으로 포커스 | **포커스 유지** |

포커스 이동은 CLI/IPC 로 불가, 단축키/마우스로만([focus 독립성](../../design/policies/focus.md)).

### meta

새 surface 에 key-value 설정(각각 `surface.meta.set`). 주 용도: `nickname`(이름 참조), 커스텀 태그(에이전트가 surface 분류/추적). 응답: `{new_pane_id?, new_surface_id}`.

### 관련

- 두 레벨 레이아웃은 위 [내부 동작](#내부-동작-headless-valid) · [cwd 정책](../../design/policies/cwd.md) · [surface-cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant) · [reference/api](../../reference/api.md)

## 비-목표 (Out of scope)

- **사이드바의 워크스페이스 목록/전환 UI** — [sidebar](../sidebar/index.md) 영역.
- **탭 스트립의 시각/드래그 동작** — [`features/workspace-tabs/`](../workspace-tabs/index.md). 여기선 Pane 의 `tabs`/`active_tab` *도메인* 만.
- **상태바** — [`features/workspace-status-bar/`](../workspace-status-bar/index.md).
- **터미널 PTY/그리드/스크롤백 내부** — surface 는 leaf marker 일 뿐, 터미널 데이터는 `TerminalStore`.
- **attach/detach 실행 메커니즘** — surface 는 원본 kind 를 유지하고 점유는 `OccupancyRegistry` 로 추적; 점유 동작·실행은 [remote-attach](../remote-attach/index.md), 메커니즘은 [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).

## Acceptance Criteria

- Given 빈 워크스페이스 When `tasty new tab --pane <P>` Then 새 탭이 추가되고 `tasty list tabs --pane <P>` 에 보인다.
- Given 사용자가 보던 탭이 있는 Pane When 에이전트가 `tasty new tab --pane <P> --type html|markdown|…` 로 탭을 만든다 Then `tasty list tree` 의 활성 탭과 focus 가 그대로이고, 단축키·메뉴로 사용자가 연 탭은 선택된다.
- Given Pane 하나 When `tasty split --level pane --pane <P>` Then 워크스페이스에 Pane 이 둘이 되고 탭 전환과 무관하게 분할이 유지된다.
- Given 탭 안 Surface 하나 When `tasty split --level surface --surface <S>` Then 그 탭에서만 Surface 가 둘이 되고, 다른 탭으로 전환하면 분할이 사라졌다 돌아온다.
- Given Pane 모델에 마지막 탭 하나 When close_tab/close_tab_by_id 호출 Then 닫히지 않는다. surface 닫기에 따른 상위 pane 정리는 별도 닫기 경로다.
- Given 사용자가 보고 있지 않은 탭 · Pane · 워크스페이스 When 그것이 닫힌다(에이전트 `tasty close`/`surface.close` 포함) Then 사용자가 보고 있던 대상은 그대로다 — 시야는 보던 대상 **자체**가 사라졌을 때만 움직인다 ([focus 정책](../../design/policies/focus.md) "삭제로 인한 인덱스 이동").
- Given 비활성 탭에 터미널이 있는 상태로 저장한 레이아웃 When 앱을 다시 시작한다 Then 그 터미널은 `tasty list tree` 에서 `type:"Pending"`, `kind:"terminal"` 로 보고되고, 탭을 화면에 보이거나 `tasty wake --surface <ID>` 를 보내면 PTY 가 생긴다([empty surface](../../surfaces/empty/index.md)).
- Given `--type markdown` 으로 만든 surface When `tasty list tree` Then `kind:"markdown"` 으로 보고된다.

> 전부 headless(IPC/CLI)로 검증 가능 — 트리 조작·분할·닫기·종류는 `tasty list/new/split/close` 시나리오로 확인.

## 구현

- 도메인 모델: `crates/tasty-model/` — `Workspace`(`workspace.rs`) · `Pane`+`PaneNode`(`pane.rs`/`pane_tree.rs`, 상위 레이아웃) · `Tab`(`tab.rs`) · `SurfaceLayout`(`surface_layout.rs`, 하위 레이아웃) · `SurfaceDescriptor` 값(`surface_descriptor.rs`). 실행 `Surface` trait과 구체 surface는 host runtime이 소유한다. markdown/image 는 별도 domain 타입이 아니라 image 는 host `src/runtime/egui_mesh_surface.rs`의 `EguiMeshSurface`(plugin 공용 mesh surface), markdown 은 `src/plugin_bridge/remote_surface.rs` 의 `RemoteSurface`(webview)로 구현된다.
- 이진 트리 공통: `binary_tree.rs` (`BinaryTree` trait — Pane/Surface 양쪽이 구현).
- 보유/동작: `crates/tasty-core/src/state.rs`의 `CoreState`는 확정 구조를 제공하고, `src/runtime/engine_session.rs`의 `EngineSession`은 `runtime`·`remote`·`live`를 소유한다. 실행 surface/registry는 `EngineRuntime`, 점유는 `live.occupancy`에 있다. `src/state/`의 `workspace.rs`/`pane.rs`/`tab.rs`는 View 요청을 구성한다.
- 종류 레지스트리: `src/runtime/surface_registry/` (`register_builtin_kinds`, egui-mesh 등록 `egui_mesh.rs`), RemoteSurface: `src/plugin_bridge/remote_kind.rs`.

## 화면

화면정의서 — **작업 영역 화면**.

- **시각 소스**: tasty-page `vendor/ui_kits/terminal/work.jsx`

[MainView](../main-view/index.md#화면) 중앙. 이 화면은 위에서 설명한 **두 레벨 레이아웃**(상위 Pane / 하위 Surface)을 표시한다. 아래에서는 시각 배치를 설명한다.

### 트리거

MainView 가 열리면 항상 표시(중앙 고정 영역). 사이드바에서 Workspace 를 전환하면 해당 Workspace 의 Pane/Tab/Surface 트리로 내용이 바뀐다.

### UI 요소 인벤토리

```
┌─ 작업 영역 ────────────────────────────────┐
│ [Pane A 탭바] tab1 tab2 + │ [Pane B 탭바] …  │  → workspace-tabs (탭 스트립)
│ ┌───────────────────────┐ │ ┌────────────┐  │
│ │ Surface (분할 가능)    │ │ │ Surface     │  │
│ │  ┌─────────┬────────┐ │ │ │             │  │  ← 상위 레이아웃: Pane A | Pane B (탭 무관)
│ │  │ surface │ surface│ │ │ │             │  │  ← 하위 레이아웃: 탭 안 surface 분할 (탭 종속)
│ │  └─────────┴────────┘ │ │ └────────────┘  │
│ └───────────────────────┘ │                  │
└────────────────────────────────────────────┘
```

- **Pane 영역(상위 레이아웃)** — 워크스페이스를 물리적으로 나눈 칸. 각 Pane 은 자기 **탭 스트립**을 머리에 둔다. Pane 사이 경계는 분할 보더(`PANE_BORDER_WIDTH`).
- **탭 스트립** (각 Pane 상단) — 그 Pane 의 탭 목록 + active 탭 강조. 시각/드래그/추가 버튼은 → [`features/workspace-tabs/`](../workspace-tabs/index.md). 표시명 규칙은 부모 기획.
- **Surface 타일(하위 레이아웃)** — active 탭의 SurfaceLayout 을 타일로 렌더. 분할 시 surface 사이 경계는 `SURFACE_BORDER_WIDTH`. 포커스된 surface 강조.
- **Surface 콘텐츠** — 타입별로 다르게 렌더(terminal=GPU, image=egui-mesh, markdown/html=WebView, empty=변환 버튼). 종류 표는 부모 기획 [Surface 종류](#surface-종류), 타입별 화면은 [Surface 종류](../../surfaces/index.md)의 각 문서.

### 상태별 시각

- **단일 / 분할** — Pane·Surface 모두 1개면 보더 없음, 분할되면 방향(좌우/상하)·비율(`ratio`)대로 타일 + 보더.
- **포커스** — View 가 선택한 Pane / surface 가 강조된다.
- **탭 전환** — 하위 레이아웃 전체가 함께 전환(상위 Pane 분할은 불변).
- **복원 자리 / readonly** — 복원 자리는 활성화 전([empty surface](../../surfaces/empty/index.md#상태별-시각)), attach 점유된 surface 는 readonly mirror 로 표시(내용 보임 + 조작 차단).

### 시각 소스

tasty-page `vendor/ui_kits/terminal/work.jsx` — 작업 영역 치수·보더·타일 배치의 단일 출처. 보더 폭은 코드 상수와 일치하되 **두 상수의 좌표계가 다르다**: `PANE_BORDER_WIDTH` 는 논리 2px(디자인이 정한 두께라 배율을 따라 커진다 — 배율 2 에서 4 물리px), `SURFACE_BORDER_WIDTH` 는 물리 1px(hairline 이라 밀도와 무관하게 1 device px 로 남는다). 근거는 [docs/concepts/typed-length.md](../../concepts/typed-length.md).
