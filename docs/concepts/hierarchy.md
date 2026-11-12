# 구조 계층 (Structural hierarchy)

tasty 화면 구조는 객체 계층 하나와, 그 위의 **두 레벨 레이아웃** 으로 이뤄진다. 모든 윈도우·surface 기능 문서가 이 용어를 쓴다. (화면을 보는 *주체* 는 [actors.md](actors.md).)

## Window 과 View

- **`winit::window::Window`** — OS 가 주는 창 자원 (창틀 / 이벤트 소스 / 렌더 표면). winit `WindowId` 로 식별.
- **`View`** — tasty 쪽 윈도우 표현. 그 창의 *종류 + 콘텐츠 + 행동*(render / 이벤트 / modality)을 묶은 객체로, winit Window 를 `Arc` 로 소유한다. **1 View : 1 Window.**

`Window`는 winit의 OS 창을, `View`는 Tasty가 그 창에 표시하는 내용과 동작을 뜻한다. Tasty에는 `Window` trait이 없다. 터미널을 표시하는 주 윈도우의 타입은 `MainView`다. 요청에서 “윈도우”의 의미가 모호하면 문맥을 확인하고, 구현 대상이 달라질 때만 사용자에게 확인한다.

## View 의 종류 (= 윈도우 종류)

`View` trait 의 구현체가 곧 윈도우 종류다. **`MainView` 도 그중 하나** — 터미널을 호스팅하는 View 다. 각 구현체는 별개 OS 윈도우(winit Window)이고, App의 `ViewRegistry`는 이들을 `HashMap<WindowId, Box<dyn View>>`로 관리한다.

| 구현체 | 계열 (supertrait) | 무엇 |
|--------|-------------------|------|
| **`MainView`** | `View` + `sealed::Sealed` 직접 구현 | 사이드바 + 워크스페이스를 호스팅하는 주 윈도우. 여러 개 가능. ← 이 문서가 주로 다루는 것 |
| `SettingsView` / `PluginsView` / `QuitView` | `ModalView` | 모달 윈도우 — 전역 1개, 활성 시 입력 차단 |
| `PresetView` | `View` + `sealed::Sealed` 직접 구현 | 에디터 윈도우 — modeless |

**App**은 프로세스의 진입점·서비스·통신·실행 조정을 맡는다. IPC 서버와 포트는 App의 Hub가 소유하고, GUI의 View 목록과 모달은 ViewRegistry가 관리한다.

**EngineSession**은 한 engine의 구조 projection(`CoreState`), 휘발성 도메인 상태(`LiveDomainState`), 실행 자원(`EngineRuntime`), task·hook·observer·remote 범위를 소유한다. engine은 창 없이도 살아 있을 수 있다. headless는 App과 EngineSession을 실행하며 로컬 View를 만들지 않는다.
## 구조 계층 = CoreState 도메인 (GUI 없이도 구성)

구조 계층은 `CoreState`가 제공하는 Workspace·Pane·Tab·Surface 읽기 트리다. 로컬 구조의 원본은 journal의 `JournalModel`이며 확정 batch를 projection에 적용한다. 실행 surface와 Terminal/Pty는 EngineRuntime에 있고 트리에는 descriptor가 있다. headless도 같은 명령·실행 경계를 사용하고, GUI의 MainView는 EngineRead로 값을 조회한다. 동작은 [작업 영역 문서](../features/work-area/index.md)를 따른다.

```
CoreState   도메인 트리 — headless 에서도 구성·동작
└── Workspace   최상위 컨테이너. 여러 개, 사이드바에서 전환.
    └── Pane    독립 탭 바. **상위 레이아웃**이 위치 결정 (탭 무관 고정).
        └── Tab        **하위 레이아웃**(Surface 배치)을 가짐.
            └── Surface   최하위. 타입(Terminal/Markdown/…)을 가짐.
```

GUI에서는 MainView와 EngineSession의 관계를 App의 EngineRegistry가 관리한다. MainView는 구조나 실행 자원을 직접 소유하지 않는다. 선택·viewport·편집 draft는 View에 남으며, EngineRead는 이를 바꾸거나 실행 자원을 제어하는 포트가 아니다. 창이 사라져도 parked 또는 retiring engine은 수명이 끝날 때까지 유지된다([multi-window](../architecture/multi-window.md#engine-registry와-parked--pty-생존)).
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

Surface는 호스트 내장, egui-mesh 플러그인, webview 플러그인으로 나뉜다. 호스트 내장은 `register_builtin_kinds`로 등록한다. egui-mesh는 허용된 플러그인이 자기 프로세스에서 만든 mesh를 호스트가 합성한다. webview는 호스트의 `RemoteSurface`와 네이티브 WebView를 사용한다. 등록·렌더 분기의 공통 규칙은 [작업 영역의 Surface 종류](../features/work-area/index.md#surface-종류), kind마다의 명세는 [Surface 종류 문서](../surfaces/index.md)를 참고한다.

## 관련

- [actors.md](actors.md) — 이 구조를 사용하는 주체 (로컬/AI/원격)
- [상태 모델](state-model/index.md) — 각 계층의 상태 축과 전이, 계층 사이의 집계·제한
- View 내부 오버레이: [`design/systems/popup.md`](../design/systems/popup.md) · [`design/systems/toast.md`](../design/systems/toast.md). 모달 계열 View 는 위 "View 의 종류" 표 참조.
