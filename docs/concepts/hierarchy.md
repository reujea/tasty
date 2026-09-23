# 구조 계층 (Structural hierarchy)

tasty 화면 구조는 객체 계층 하나와, 그 위의 **두 레벨 레이아웃** 으로 이뤄진다. 모든 윈도우·surface 기능 문서가 이 용어를 쓴다. (화면을 보는 *주체* 는 [actors.md](actors.md).)

## Window 과 View

- **`winit::window::Window`** — OS 가 주는 창 자원 (창틀 / 이벤트 소스 / 렌더 표면). winit `WindowId` 로 식별.
- **`View`** — tasty 쪽 윈도우 표현. 그 창의 *종류 + 콘텐츠 + 행동*(render / 이벤트 / modality)을 묶은 객체로, winit Window 를 `Arc` 로 소유한다. **1 View : 1 Window.**

> 옛 tasty `Window` trait 이 winit 의 `Window` 와 헷갈려서 **`View` 로 rename** 됐다 (`WindowBase`→`ViewBase`, `*Window`→`*View`). **tasty 쪽 `Window` trait 은 없다** — 지금 `Window` 는 winit OS 창만 가리킨다. 즉 *View = 윈도우의 tasty 쪽 용어*. 같은 맥락에서 옛 gloss **"터미널 윈도우" 도 쓰지 않는다** — 정식 명칭은 `MainView`.
>
> **AI Agent 지침**: 유비쿼터스 언어상 "window(윈도우)" 는 모호하다 — 사용자가 *View*(tasty 쪽 윈도우, 예: `MainView`)를 의도했을 수도, *진짜 winit OS 창*을 의도했을 수도 있다. 대화/요청에서 이 용어가 나오면 둘 중 무엇인지 단정하지 말고 **한 번 되물어 확인하는 것을 권장한다.**

## View 의 종류 (= 윈도우 종류)

`View` trait 의 구현체가 곧 윈도우 종류다. **`MainView` 도 그중 하나** — 터미널을 호스팅하는 View 다. 각 구현체는 별개 OS 윈도우(winit Window)이고, 엔진은 이들을 `HashMap<WindowId, Box<dyn View>>` 로 균일하게 관리한다.

| 구현체 | 계열 (supertrait) | 무엇 |
|--------|-------------------|------|
| **`MainView`** | `View` + `sealed::Sealed` 직접 구현 | 사이드바 + 워크스페이스를 호스팅하는 주 윈도우. 여러 개 가능. ← 이 문서가 주로 다루는 것 |
| `SettingsView` / `PluginsView` / `QuitView` | `ModalView` | 모달 윈도우 — 전역 1개, 활성 시 입력 차단 |
| `PresetView` | `View` + `sealed::Sealed` 직접 구현 | 에디터 윈도우 — modeless |

(**Engine** = 진입점 + 서버. IPC 포트 소유, 모든 윈도우 생명주기 관리. **headless 에선 View(GUI) 없이 Engine + `CoreState` 만 동작** — 아래 구조 계층은 그 `CoreState` 의 도메인이라 GUI 없이도 구성된다.)

## 구조 계층 = CoreState 도메인 (GUI 없이도 구성)

containment 계층 — 이게 "구조 계층" 의 본체다. 이것은 **`CoreState` 의 도메인 트리**이며 **GUI(View) 없이도 구성·동작한다.** headless 에선 부팅이 `CoreState` 를 직접 만들어 Workspace/Pane/Tab/Surface 와 PTY 가 살아있고, **`MainView` 는 GUI 가 있을 때 이 `CoreState` 를 호스팅·투영하는 셸**일 뿐이다 (→ [identity](../identity.md) headless 동작-우선; 도메인 동작은 [`features/work-area/`](../features/work-area/index.md)).

```
CoreState   도메인 트리 — headless 에서도 구성·동작
└── Workspace   최상위 컨테이너. 여러 개, 사이드바에서 전환.
    └── Pane    독립 탭 바. **상위 레이아웃**이 위치 결정 (탭 무관 고정).
        └── Tab        **하위 레이아웃**(Surface 배치)을 가짐.
            └── Surface   최하위. 타입(Terminal/Markdown/…)을 가짐.
```

GUI 에서는 `MainView`(View) 가 이 `CoreState` 를 호스팅·렌더한다. 윈도우가 여럿이면 각 MainView 가 자기 `CoreState` 를 가진다. headless 엔 MainView 없이 `CoreState` 만 존재한다.

- **Workspace** — 도메인의 최상위 컨테이너. (GUI 에선) 한 MainView 가 여러 워크스페이스를 갖고 사이드바에서 전환한다.
- **Pane** — 독립적인 탭 바를 가진 화면 영역. 위치는 **상위 레이아웃**으로 결정되고 탭 전환과 무관하게 고정된다. tmux/iTerm2 에 대응 개념이 없는 tasty 고유 설계.
- **Tab** — Pane 안의 탭 하나. 내부에 Surface 들의 **하위 레이아웃**을 가진다. 탭 전환 시 하위 레이아웃 전체가 함께 전환된다.
- **Surface** — Tab 안의 최하위 컨테이너. **타입**을 가지며(아래), 고유 `surface_id` 를 갖는다. 닫기/포커스/리스트 동작은 타입과 무관하게 동일하다.

## 두 레벨 레이아웃 (tasty 핵심 설계)

기존 도구는 분할 정책이 하나뿐이다 — tmux 는 분할이 window 에 고정(탭 전환해도 유지), iTerm2 는 분할이 tab 에 종속(탭 전환 시 바뀜). tasty 는 **둘 다** 제공한다:

- **상위 레이아웃 (탭 무관)** — Workspace 안에서 **Pane** 들을 배치. 탭을 전환해도 이 분할은 고정. 화면을 물리 영역으로 나눠 각 영역이 독립적으로 탭을 전환하게 한다.
- **하위 레이아웃 (탭 종속)** — Tab 안에서 **Surface** 들을 배치. 탭 전환 시 이 분할도 함께 전환. 한 탭 안에서 여러 터미널을 동시에 본다.

예: 상위 레이아웃으로 좌우 Pane 분할 — 왼쪽은 Claude Code 전용, 오른쪽은 탭 여럿(logs/build). 오른쪽 탭을 전환해도 왼쪽 Claude 는 영향 없다.

## Surface 타입

| kind | 출처 | 콘텐츠 | 렌더 |
|------|------|--------|------|
| `terminal` | **host 내장** | 쉘 세션 (PTY 연결) | GPU 셰이더 |
| `empty` | **host 내장** | 빈 surface (타입 전환 버튼); deferred 터미널 자리 | egui |
| `markdown` | `com.tasty.markdown` plugin (`rendering=webview`) | 마크다운 뷰어 | 네이티브 WebView overlay — plugin 이 sanitize HTML 문서를 생성(`RemoteSurface`) |
| `image` | `com.tasty.image` plugin (`rendering=egui-mesh`) | 이미지 뷰어/편집 | plugin 자가 렌더 mesh (비트맵=egui 텍스처) |
| `explorer` | **host 내장** | 파일 탐색기 | egui |
| `dag_graph` | **host 내장** | agent task DAG 뷰 | egui |
| `html` | `com.tasty.html` plugin (`rendering=webview`) | HTML/웹 뷰어 | 네이티브 WebView overlay (`RemoteSurface`) |

출처 3종: **host 내장**(`register_builtin_kinds`) / **egui-mesh plugin**(plugin 이 `rendering=egui-mesh` 선언 + host 화이트리스트, plugin 프로세스가 자가 렌더한 mesh 를 host 가 합성) / **webview plugin**(`rendering=webview`, RemoteSurface stand-in + native WebView overlay). 종류별 상세·동작은 [`features/work-area/`](../features/work-area/index.md#surface-종류).

## 관련

- [actors.md](actors.md) — 이 구조를 사용하는 주체 (로컬/AI/원격)
- View 내부 오버레이: [`design/systems/popup.md`](../design/systems/popup.md) · [`design/systems/toast.md`](../design/systems/toast.md). 모달 계열 View 는 위 "View 의 종류" 표 참조.
