# 작업 영역 (Work area)

- **Status**: Implemented
- **주체**: 로컬 사용자(GUI 직접) · AI Agent(IPC/CLI 로 ID 지정 조작) · 원격 접속 사용자(surface/workspace 점유)
- **ADR**: 없음
- **코드**: `crates/tasty-model/` (`workspace.rs`/`pane.rs`/`tab.rs`/`surface_layout.rs`/`pane_tree.rs`/surface 타입들), `src/core/state.rs` (`CoreState`), `src/state/` (`workspace.rs`/`pane.rs`/`tab.rs` 동작)
- **화면**: [screens/work-area.md](screens/work-area.md)

## 목적

[MainView](../main-view/index.md) 의 중앙 — 사용자가 실제로 일하는 영역. **Workspace › Pane › Tab › Surface** 컨테인먼트 도메인과 그 위의 **두 레벨 레이아웃**을 보유한다. 이 도메인은 GUI 없이도 성립하는 `CoreState` 의 본체이며, MainView 는 그것을 사람에게 투영할 뿐이다 (→ [identity](../../identity.md) headless). 계층 용어 자체는 [구조 계층](../../concepts/hierarchy.md).

## 내부 동작 (headless-valid)

### 도메인 트리

`CoreState` 가 `workspaces: Vec<Workspace>` 를 들고, 각 객체는 아래로 중첩된다. 두 군데에 **이진 트리(분할 트리)** 가 있다 — 상위(Pane)와 하위(Surface).

```
CoreState
└── Workspace          (Vec — 사이드바로 전환)
    └── PaneNode        ← 상위 레이아웃: Pane 들의 이진 분할 트리 (탭 무관)
        └── Pane         (탭 바 하나를 가진 화면 영역)
            └── Tab       (Vec — active_tab 하나가 활성)
                └── SurfaceLayout   ← 하위 레이아웃: Surface 들의 이진 분할 트리 (탭 종속)
                    └── Surface      (leaf — 타입을 가짐)
```

### Workspace

MainView 의 최상위 컨테이너. 한 MainView 가 **여러 개**를 갖고 사이드바에서 전환한다.

- 필드: `id` · `name` · `subtitle` · `description` · `pane_layout`(상위 레이아웃 `PaneNode`) · `focused_pane`(이 워크스페이스에서 포커스된 Pane) · `attach_mapping`(원격 attach 매핑, 있으면 활성화 시 자동 attach) · `mirror`(원격을 attach 한 client mirror 인지 — 사이드바 이름 앞 하늘색 glyph/레일 corner chip 으로 구분).
- `focused_pane` 는 워크스페이스마다 따로 기억된다 — 전환해도 각자의 포커스가 보존된다.
- 변형: 일반 워크스페이스 / **mirror 워크스페이스**(원격 attach 의 client측). mirror 는 런타임 전용(영속 안 함, 재시작 시 재attach), attach 의 점유 모델은 [actors](../../concepts/actors.md#점유-occupation-모델).

### Pane — 상위 레이아웃 (탭 무관)

Pane 은 **독립적인 탭 바를 가진 화면 영역**이다. Workspace 안에서 Pane 들의 배치는 `PaneNode` 이진 트리(`Leaf(Pane)` | `Split { direction, ratio, first, second }`)로 결정되고, **탭 전환과 무관하게 고정**된다 — tmux 의 "분할이 window 에 고정" 에 대응.

- 필드: `id` · `tabs: Vec<Tab>` · `active_tab`(인덱스) · `tab_scroll_offset`(탭 바 가로 스크롤).
- 탭 동작: 추가(`add_*_tab`, 활성/백그라운드) · 닫기(`close_tab`/`close_tab_by_id` — **마지막 탭은 못 닫음**, `active_tab` 은 제거 위치 기준으로 자동 보정돼 보던 탭을 계속 가리킨다) · 전환(`goto_tab`/`next_tab`/`prev_tab`) · 이동(`move_tab`, `active_tab` 자동 보정).
- **활성 탭 추종 스크롤**: 탭이 많아 탭 바에 좌우 화살표가 뜬 상태에서 전환(단축키·클릭 공통)하거나 pane 이 리사이즈돼 활성 탭이 뷰포트 밖으로 밀려나면 `tab_scroll_offset` 을 자동 보정해 다시 보이게 한다(`src/adapters/ui/tab_bar.rs` `TabBarAction::AutoScrollToActiveTab`). 활성 인덱스/지오메트리가 실제로 바뀐 시점에만 보정하므로, 사용자가 화살표로 수동 스크롤해 둔 상태(활성 탭 불변)는 덮어쓰지 않는다.
- 분할(상위): `PaneNode::split_pane_in_place` 로 Pane 을 좌우/상하로 쪼갠다. 새 Pane 의 PTY 는 구조 변경 *전에* 미리 생성(트리가 빈 store 상태를 보지 않도록).

### Tab — 하위 레이아웃 (탭 종속)

Pane 안의 탭 하나. 내부에 Surface 들의 `SurfaceLayout` 이진 트리(`Leaf(Box<dyn Surface>)` | `Split { direction, ratio, first, second, focus_second }`)를 가진다. **탭을 전환하면 이 분할 전체가 함께 전환**된다 — iTerm2 의 "분할이 tab 에 종속" 에 대응.

- 필드: `id` · `name`(자동 생성, 예 "Shell") · `explicit_name`(명시 지정 — 최우선) · `osc_title`(OSC 0/2 터미널 타이틀) · `layout`(`SurfaceLayout`) · `focused_surface` · `cached_display_name`.
- **표시명 우선순위**: `explicit_name` > `osc_title` > cwd 파생 캐시명 > `name`. cwd 변경 시 shell prompt 가 새 OSC title 을 자연 발화해 cwd 도 자연 반영. `explicit_name` 은 cwd/OSC 로 덮이지 않음(에이전트 `tab.create --name`).
- **셸 exe 경로 형태의 OSC 제목은 소스에서 무시된다.** ConPTY 는 spawn 시 콘솔 기본 제목으로 셸 실행파일 경로(예 `C:\Program Files/Git/bin/bash.exe`)를 OSC 0/2 로 발화하는데, 이 값이 `osc_title` 에 고정되면 복구 후 탭 제목이 exe 경로로 보인다. `map_osc`(`crates/tasty-terminal/src/vte_handler/osc.rs`)가 "경로 형태(`/`·`\`·`:` 포함) + basename 이 known shell(`is_known_shell_name`)" 인 제목을 `current_title` 세팅·`TitleChanged` 발화 없이 버린다 — bare `bash` 같은 비경로 제목은 통과(오탐 방지). 보강으로 Windows 빌트인 bashrc(`__tasty_title`)가 매 프롬프트에 cwd 기반 이름(`~`/`/`/basename)을 OSC 0 으로 발화해 `osc_title` 도 cwd 를 따른다.
- **`osc_title` 은 탭의 `focused_surface` 가 발화한 title 만 반영한다.** 한 탭 안의 병렬 surface(split) 중 비-focused surface 가 OSC 0/2 를 발화해도 탭 제목은 흔들리지 않는다(last-writer-wins flicker 방지). 판정 기준은 **탭별 `focused_surface`** 이며 앱-전역 포커스가 아니다 — 배경 탭도 자기 focused surface 의 title 을 계속 반영한다. cwd 파생 캐시명(`refresh_tab_display_name`)이 이미 focused surface 만 쓰던 정책을 OSC 경로(`refresh_tab_osc_title`)도 동일하게 따른다. 탭 내 포커스가 다른 surface 로 이동하거나(포커스 전환 폴링), surface close/move 로 `focused_surface` 가 재배정되거나, `explicit_name` 이 해제되면 새 focused surface 의 최신 title 로 재투영한다. 새 focused surface 가 title 미보유(non-terminal 등)면 `osc_title` 을 clear 해 cwd 파생명 → `name` fallback 이 동작한다. (`SurfaceTitleChanged` host event 는 비-focused surface 발화 시에도 종전대로 surface 단위로 발화 — plugin 호환 유지.)
- 하위 동작: surface 닫기(`close_surface`, 포커스 이전) · 포커스 이동(`move_focus_forward`/`backward`/`directional_focus`) · 분할(`split_focused_surface`/`split_surface_by_id[_generic]`).

### Surface

Tab 의 SurfaceLayout 트리 leaf, 최하위 컨테이너. 고유 `surface_id` 를 갖고, **타입(kind)** 을 가진다(아래). `Surface` trait 의 핵심: `kind()`(불변 식별자) · `type_name()`(표시 라벨) · `surface_id()` · `source_cwd()`(새 surface 생성 시 상속할 시작 cwd — Surface cwd invariant, [`architecture/invariants/surface-cwd`](../../architecture/invariants/surface-cwd.md)) · `display_name()`. 닫기/포커스/리스트 동작은 타입과 무관하게 동일하다.

#### Deferred 터미널

레이아웃 복원 시 비활성 탭의 PTY 는 **지연 생성**된다(런타임에 새로 만드는 탭/분할은 항상 즉시 spawn — 지연 대상은 복원되는 비활성 탭뿐이다). 이 경우 트리 leaf 는 `deferred_spawn` 을 가진 `EmptySurface` placeholder 로 들어가고(빈 layout 이 아님), **화면에 표시되기 직전 단일 지점**(`AppState::reify_displayed_surfaces`, 매 프레임 렌더 직전 호출)에서 `ensure_initialized` 가 PTY 를 띄워 `TerminalSurface` marker 로 교체한다. "표시되는 deferred 는 반드시 reify 된다" 가 불변식이며, 이 단일 지점이 모든 노출 경로(키보드 탭 전환, 탭 close, pane focus 전환, 워크스페이스 전환, window 복원)를 한 번에 커버한다 — 전환 입력 핸들러마다 reify 를 흩뿌리지 않는다. 외부(IPC `surface.list`, 트리 JSON)에는 `type:"Terminal"`, `pty_ready:false` 로 보고된다 — 아직 안 뜬 터미널 자리. (IPC `surface.send` 등 표시와 무관한 경로는 여전히 `ensure_surface_initialized` 로 개별 reify.)

### 두 레벨 레이아웃 (tasty 핵심 설계)

기존 도구는 분할 정책이 하나뿐(tmux=window 고정, iTerm2=tab 종속). tasty 는 **둘 다** 제공한다:

- **상위 레이아웃 (탭 무관)** — Workspace 안에서 `PaneNode` 로 **Pane** 배치. 탭을 바꿔도 이 분할은 고정. 화면을 물리 영역으로 나눠 각 영역이 독립적으로 탭을 전환.
- **하위 레이아웃 (탭 종속)** — Tab 안에서 `SurfaceLayout` 으로 **Surface** 배치. 탭 전환 시 함께 전환. 한 탭에서 여러 surface 동시 표시.

예: 상위로 좌우 Pane 분할 — 왼쪽은 Claude Code 전용, 오른쪽은 탭 여럿(logs/build). 오른쪽 탭을 바꿔도 왼쪽 Claude 는 영향 없음.

### Surface 종류

`kind()` 가 식별자, `type_name()` 이 표시 라벨. 세 출처가 있다 — host 내장, plugin 이 egui-mesh 로 자가 렌더하는 것(EguiMeshSurface), webview overlay 로 그려지는 것(RemoteSurface).

| kind | type_name | 출처 | 렌더 | 비고 |
|------|-----------|------|------|------|
| `terminal` | Terminal | **host 내장** | GPU 셰이더 | 쉘 PTY. deferred 가능(아래 `empty`) |
| `empty` | Empty | **host 내장** | egui | 빈 자리(타입 선택 UI). **deferred 터미널 placeholder 도 이 타입** |
| `markdown` | Markdown | `com.tasty.markdown` plugin (`rendering=webview`) | 네이티브 WebView overlay — plugin 이 sanitize HTML 문서 생성(`RemoteSurface`) | [ADR-0065](../../adr/0065-markdown-webview-render-channel.md), 대용량/파일열기 확인 팝업 2개만 egui-mesh |
| `image` | Image | `com.tasty.image` plugin (`rendering=egui-mesh`) | plugin 자가 렌더 mesh (비트맵=egui 텍스처) | egui-mesh whitelist |
| `explorer` | Explorer | **host 내장** (T11) | egui | host builtin surface |
| `dag_graph` | DAG | **host 내장** | egui | agent task DAG 뷰 ([agent-collaboration](../agent-collaboration/index.md)) |
| `html` | (plugin 제공) | `com.tasty.html` plugin (`rendering=webview`) | 네이티브 WebView overlay (`RemoteSurface`) | plugin 은 URL/navigation 만 제어 |
| `mesh_demo` | egui-mesh Demo | `com.tasty.mesh-demo` plugin (`rendering=egui-mesh`) | plugin 자가 렌더 mesh | 개발/검증용. 매니페스트가 `bundle = false` 라 배포 패키징에는 안 들어간다 |

- **host 내장**은 `register_builtin_kinds`(`terminal`/`empty`/`explorer`/`dag_graph`) 가 부팅 시 등록.
- **egui-mesh plugin**(`image`, 그리고 markdown 의 대용량/파일열기 확인 팝업 2개만)은 plugin 매니페스트가 `rendering="egui-mesh"` 로 선언하고 host 화이트리스트 + api_version 게이트에 매칭되면 `EguiMeshSurface` stand-in 으로 등록된다 — 콘텐츠는 plugin 프로세스가 tessellate 한 mesh 를 host 가 합성 (ADR-0028).
- **webview plugin**(`html`/`markdown`)은 `RemoteSurface` stand-in 위에 host 가 native WebView overlay 를 자동 관리한다. `html` 은 `webview.set_url` IPC 로 URL/navigation 만 제어하고, `markdown` 은 plugin 이 직접 sanitize 된 HTML 문서 전체를 생성해 로드시킨다([ADR-0065](../../adr/0065-markdown-webview-render-channel.md)).
  - **overlay 생성에 실패하면 그 surface 는 비어 있고, 앱은 계속 돈다.** 실패는 두 종류로 갈린다 — 다음 시도에 달라질 수 있는 것(서버 자원 고갈 등)은 상한까지 다시 시도하고, 이 프로세스에서 달라지지 않는 것(창 종류·라이브러리 부재 등)은 한 번에 포기한다. 어느 쪽이든 시도 횟수에 상한이 있어 실패가 무한히 반복되지 않는다. 로그에는 첫 실패와 포기하는 순간만 남고, 포기 줄이 실제로 몇 번 시도했는지를 적는다. 그 surface 를 닫았다 다시 열면 시도 예산도 새로 생긴다. 근거·재검토 조건은 [ADR-0159](../../adr/0159-a-null-gdk-window-is-a-value-not-a-crash.md).
  - webview kind 는 **탭 내부 분할(SurfaceGroup)의 어느 leaf 에서도** 동작한다. host 는 탭의 `SurfaceLayout` 트리 전체를 순회해 URL 을 가진 leaf 마다 overlay 를 만들고(포커스 leaf 로 한정하지 않는다), overlay 의 bounds 는 pane 전체가 아니라 `SurfaceLayout::compute_rects` 가 준 **그 leaf 의 rect** 다 — 같은 탭의 옆 surface 를 덮지 않는다. divider 드래그용 4px inset 은 leaf 의 변이 **pane 콘텐츠 영역 외곽에 닿을 때만** 적용하고, 분할된 leaf 사이 내부 경계에는 divider gap 만 둔다(터미널끼리의 분할과 같은 간격). `webview.set_url` / `webview.navigation_attempt` 의 surface 조회도 같은 기준이라 비포커스 leaf 도 도달한다.
- 새 kind 는 `SurfaceKindRegistry` 에 동적 등록 — plugin 이 hello 후 추가 가능.
- **등록된 kind 는 `surface.kinds` / `tasty list surface-kinds` 로 묻는다.** 이 조회가 읽는 것은 매니페스트가 아니라 `SurfaceKindRegistry` — 런타임의 사실이다. 여기 나오는 kind 가 정확히 `--type <kind>` 로 만들 수 있는 kind 이고, host 내장 4 종도 함께 나온다(그쪽은 plugin 이 아니라 어느 `plugin.*` 조회에도 안 나온다). 칸은 kind · 표시명 i18n 키 · icon · **실제** 렌더 경로(`rendering`: `host-egui`/`egui-mesh`/`webview`/`remote`) · 출처(`source`: `host`/`plugin` + `plugin_id`) · 필수 params 다.
- **`plugin.show` 는 선언과 사실을 갈라 낸다.** `declared_rendering` 이 매니페스트가 요청한 값이고, `registered` 가 그 선언이 **이 plugin 의 것으로** 등록됐는지, `effective_rendering` 이 등록됐을 때 host 가 실제로 쓰는 경로다. kind 이름이 registry 에 있는데 임자가 다르면(host 내장 kind 를 remote 로 재선언 · 다른 plugin 이 먼저 등록) `registered` 는 false 이고 임자가 `registered_by` 로 나온다. 이 갈림은 오류 상태에서만 나는 것이 아니다 — 헤드리스는 `webview`/`remote` 선언을 설계대로 등록하지 않으므로 **정상 상태**에서 갈린다. `plugin.list` 는 여전히 kind **이름만** 배열로 준다.
- plugin 이 제공하는 kind 각각의 동작은 [번들 플러그인](../../plugins/index.md)(markdown/image/html). 분류 축·렌더 분기 개념은 [concepts/plugins](../../concepts/plugins.md).

## 인터페이스

- **AI Agent (IPC/CLI)**: 작업 영역의 도메인을 ID 로 직접 조작.
  - 생성: `tasty new workspace` · `tasty new tab --pane <P> [--type terminal|markdown|explorer|html|image]`.
    에이전트가 만든 탭은 kind 와 무관하게 선택되지 않는다 — pane 의 `active_tab` 과 포커스가 그대로다([focus 정책](../../design/policies/focus.md) "에이전트가 만든 탭과 선택").
  - 분할: `tasty split --level pane|surface --target <ID> [--direction …]` (상위/하위 레이아웃 각각).
  - 닫기: `tasty close tab|pane|surface --… <ID>` · `tasty close workspace --id <W>`(안의 모든 pane/tab/surface 포함) · `tasty close window --id <N>`.
    워크스페이스 닫기는 마지막 하나, mirror 워크스페이스, **원격 attach 가 하드 점유 중인 surface 를 든 워크스페이스**를 거부한다 — 창까지 없앨지는 별개의 결정이라 `close window` 로 명시하고(헤드리스는 `window.close` 가 없어 거절 문구가 그것을 권하지 않고 마지막 워크스페이스를 닫을 수 없다고만 말한다), mirror 는 attach 세션 쪽에서 거두며, 점유 중인 터미널은 그것을 쓰고 있는 원격 세션이 놓아야 닫힌다. **되돌릴 수 없다**(안의 터미널이 죽고 되돌리기 스택·스크롤백에 남지 않는다). 경계와 근거는 [ADR-0120](../../adr/0120-agent-workspace-close-boundaries.md).
    사용자가 보고 있지 않은 워크스페이스를 닫아도 화면에 있는 워크스페이스는 그대로다([포커스 독립성](../../design/policies/focus.md)).
  - 조회: `tasty list workspaces|panes|surfaces` · `tasty list tabs --pane <P>` (전 워크스페이스 순회, 포커스 무관 — [포커스 독립성](../../identity.md)).
- **사용자 트리거**: 단축키/마우스로 탭 추가·전환·이동, Pane/Surface 분할, 닫기. (단축키는 `KeybindingSettings` — 하드코딩 금지.)
- **원격 / 점유**: **Workspace 와 Surface 는 점유(attach) 대상**이다. 원격 접속 사용자가 attach 로 배타 **점유**하면 그 대상은 점유자만 조작하고 로컬·AI 는 readonly 가 된다. 점유된 surface 는 트리에서 원본 kind(terminal)를 그대로 유지하고, 점유는 `OccupancyRegistry`(`is_hard_occupied`)가 추적하며 서버측은 readonly mirror 오버레이(`src/core/attach_readonly.rs`)로 렌더 + 로컬 입력을 차단한다(전용 트리 marker kind 없음). 점유된 워크스페이스는 mirror 면 사이드바 이름 앞 하늘색 glyph(레일=우하단 corner chip)로 구분된다. 동작은 [remote-attach](../remote-attach/index.md), 개념은 [actors 점유](../../concepts/actors.md#점유-occupation-모델).

## 비-목표 (Out of scope)

- **사이드바의 워크스페이스 목록/전환 UI** — [sidebar](../sidebar/index.md) 영역.
- **탭 스트립의 시각/드래그 동작** — [`features/workspace-tabs/`](../workspace-tabs/index.md). 여기선 Pane 의 `tabs`/`active_tab` *도메인* 만.
- **상태바** — [`features/workspace-status-bar/`](../workspace-status-bar/index.md).
- **터미널 PTY/그리드/스크롤백 내부** — surface 는 leaf marker 일 뿐, 터미널 데이터는 `TerminalStore`.
- **attach/detach 실행 메커니즘** — surface 는 원본 kind 를 유지하고 점유는 `OccupancyRegistry` 로 추적; 점유 동작·실행은 [remote-attach](../remote-attach/index.md), 메커니즘은 [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).

## Acceptance Criteria

- Given 빈 워크스페이스 When `tasty new tab --pane <P>` Then 새 탭이 추가되고 `tasty list tabs --pane <P>` 에 보인다.
- Given 사용자가 보던 탭이 있는 Pane When 에이전트가 `tasty new tab --pane <P> --type html|markdown|…` 로 탭을 만든다 Then `tasty list tree` 의 활성 탭과 focus 가 그대로이고, 단축키·메뉴로 사용자가 연 탭은 선택된다.
- Given Pane 하나 When `tasty split --level pane --target <P>` Then 워크스페이스에 Pane 이 둘이 되고 탭 전환과 무관하게 분할이 유지된다.
- Given 탭 안 Surface 하나 When `tasty split --level surface --target <S>` Then 그 탭에서만 Surface 가 둘이 되고, 다른 탭으로 전환하면 분할이 사라졌다 돌아온다.
- Given 마지막 탭 하나 When 닫기 Then 닫히지 않는다.
- Given 사용자가 보고 있지 않은 탭 · Pane · 워크스페이스 When 그것이 닫힌다(에이전트 `tasty close`/`surface.close` 포함) Then 사용자가 보고 있던 대상은 그대로다 — 시야는 보던 대상 **자체**가 사라졌을 때만 움직인다 ([focus 정책](../../design/policies/focus.md) "삭제로 인한 인덱스 이동").
- Given deferred 탭 When `tasty list surfaces` Then `Terminal` / `pty_ready:false` 로 보고되고, 활성화하면 `pty_ready:true` 로 바뀐다.
- Given `--type markdown` 으로 만든 surface When `tasty list surfaces` Then `kind:"markdown"` 으로 보고된다.

> 전부 headless(IPC/CLI)로 검증 가능 — 트리 조작·분할·닫기·종류는 `tasty list/new/split/close` 시나리오로 확인.

## 구현

- 도메인 모델: `crates/tasty-model/` — `Workspace`(`workspace.rs`) · `Pane`+`PaneNode`(`pane.rs`/`pane_tree.rs`, 상위 레이아웃) · `Tab`(`tab.rs`) · `SurfaceLayout`(`surface_layout.rs`, 하위 레이아웃) · `Surface` trait(`surface_trait.rs`) · 타입(`terminal_surface.rs`/`empty_surface.rs`/`explorer_panel.rs`/`attach_mesh_surface.rs`). markdown/image 는 별도 domain 타입이 아니라 host `src/core/egui_mesh_surface.rs`의 `EguiMeshSurface`(plugin 공용 mesh surface)로 구현된다.
- 이진 트리 공통: `binary_tree.rs` (`BinaryTree` trait — Pane/Surface 양쪽이 구현).
- 보유/동작: `src/core/state.rs` `CoreState`(`workspaces`, `surface_registry`, `terminals`, `attach`), `src/state/` (`workspace.rs`/`pane.rs`/`tab.rs`).
- 종류 레지스트리: `src/core/surface_registry/` (`register_builtin_kinds`, egui-mesh whitelist `egui_mesh.rs`), RemoteSurface: `src/plugin_bridge/remote_kind.rs`.

## 화면

- [screens/work-area.md](screens/work-area.md) — 중앙 영역(탭 스트립 + Pane/Surface 분할)의 시각과 각 부분 연결.
</content>
</invoke>
