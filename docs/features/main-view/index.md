# MainView (메인 윈도우)

- **Status**: Implemented
- **주체**: 로컬 사용자(GUI 직접) · AI Agent(IPC/CLI 로 내부 surface/tab/workspace 조작) · 원격 접속 사용자(내부 surface/workspace 점유)
- **설계**: 없음
- **코드**: `src/view/main.rs`, `src/view/main/`, `src/app/window_lifecycle.rs`
- **화면**: [아래 절](#화면)

## 목적

tasty 의 주 윈도우. 워크스페이스를 호스팅하고 사이드바·탭·surface·상태바로 사용자가 실제 작업하는 메인 화면이다. `View` + `sealed::Sealed` 를 직접 구현한다 ([구조 계층](../../concepts/hierarchy.md)).

## 내부 동작

### 무엇을 호스팅하나

`MainView` 는 UI 상태(`MainViewState`)를 보유하고, 창에 연결된 도메인 트리(`CoreState` — Workspace › Pane › Tab › Surface)를 표시한다. `CoreState` 는 App 이 소유하며 창 ID 로 찾아 MainView 에 넘긴다. 한 MainView 가 **여러 Workspace** 를 갖고 사이드바에서 전환한다 (계층·두 레벨 레이아웃은 [hierarchy](../../concepts/hierarchy.md)).

### 멀티 윈도우

`create_new_window` 로 **MainView 를 여러 개** 띄울 수 있다. 각 MainView 는 독립 winit Window(1:1)이고 자기 `MainViewState` 를 가지며 독립 EngineSession의 구조 projection이 연결된다. View 는 `views: HashMap<WindowId, Box<dyn View>>`, engine 은 `App.engines` 가 관리한다([multi-window](../../architecture/multi-window.md#engine-registry와-parked--pty-생존)).

release는 같은 데이터 홈에 프로세스 하나만 둔다. 같은 홈으로 다시 실행하면 두 번째 프로세스는 창을 만들지 않는다. 사용자 실행 증거가 있으면 실행 중인 MainView를 다시 보이고 OS에 활성화를 요청하며, 없으면 `tasty new window`와 같은 새 MainView 하나를 요청한다([같은 홈의 다시 실행](../layout-persistence/index.md#같은-홈의-다시-실행)).

### headless 와의 관계

MainView는 EngineRead와 자기 View 상태로 그리는 GUI다. headless는 MainView 없이 App·EngineSession·실행 서비스를 사용한다. CoreState에는 구조 descriptor가 있고 PTY 같은 실행 자원은 EngineRuntime에 있다. (→ [헤드리스 정의 경계](../../dev-guide/headless-build-boundaries.md))

### 크롬 합성

화면은 **사이드바 + 작업 영역(탭 스트립 + surface 들) + 상태바** 로 구성되고, 윈도우 테두리는 CSD 타이틀바다. 각 영역의 상세는 화면 문서에서 하위 feature 로 연결한다.

### 입력

키보드/마우스/IME/vi-copy(키보드 복사 모드)/텍스트 선택/클립보드/링크 hover 등 사용자 입력을 받아 처리한다. 세부 동작은 각 해당 feature 문서.

## 인터페이스

- **사용자**: GUI 직접 입력(단축키/마우스). 사이드바·탭·surface 조작.
- **AI Agent (IPC/CLI)**: 이 윈도우 *안의* surface/tab/workspace 를 ID 로 생성·조회·닫기 등. `window.create`로 창도 생성할 수 있다. 에이전트가 만든 창은 사용자의 현재 포커스를 바꾸지 않는다.
- **원격 접속 사용자**: 내부 surface/workspace 를 attach 로 점유.

## 비-목표

- 단일 surface/workspace 만 가지는 경량 View 는 별도 — MainView 는 풀 셸이다.
- 모달/에디터 계열 윈도우(설정·preset 등)는 MainView 가 아니라 다른 View 구현체.

## Acceptance Criteria

- 복원할 레이아웃이 없으면 MainView가 기본 Workspace와 터미널 Surface 1개로 열린다.
- 사이드바에서 Workspace 를 전환하면 작업 영역이 해당 Workspace 로 바뀐다.
- 새 윈도우(단축키) 시 독립 `CoreState` 를 가진 MainView 가 추가로 열린다.
- AI Agent 가 IPC/CLI 로 특정 MainView 의 surface/tab/workspace 를 ID 로 조작할 수 있다.

> 윈도우 셸이라 시각 검증은 스크린샷, 멀티 윈도우/도메인 동작은 IPC/CLI 시나리오로 검증.

## 구현

- struct: `src/view/main.rs` `MainView` (`ViewBase` + `MainViewState` + 입력 상태). 연결된 `CoreState` 는 `src/app/engine_registry.rs` 의 `EngineRegistry` 가 소유한다.
- 렌더: `src/view/main/redraw.rs` (`handle_redraw` 경로. `View::render` 는 trait 호환용 빈 구현).
- 멀티 윈도우 생성: `src/app/window_lifecycle.rs` `create_new_window` → `views.insert`.
- 다시 실행 요청 처리: `src/app/external_activation.rs`(실행 중 쪽), `src/boot/single_instance/`(두 번째 프로세스·인스턴스 파일·D-Bus).
- 사이드바/크롬: `src/adapters/ui/sidebar/`.

## 화면

화면정의서 — **MainView 화면 (전체 레이아웃)**.

- **시각 소스**: tasty-page `vendor/ui_kits/terminal/` (`chrome.jsx`, `work.jsx`)

이 화면은 **합성 화면** 이다 — 이 문서에는 영역의 위치와 역할을 적고, 세부 동작은 각 기능 문서로 연결한다.

### 레이아웃

```
┌──────────────────────────────────────────────┐
│ 타이틀바 (CSD, OS별)                            │  → window-chrome
├──────────┬───────────────────────────────────┤
│          │ 탭 스트립                            │  → workspace-tabs
│ 사이드바  ├───────────────────────────────────┤
│          │ 작업 영역 (Pane/Tab/Surface)        │  → hierarchy (상위/하위 레이아웃)
│          │                                    │
│          ├───────────────────────────────────┤
│          │ 상태바                              │  → workspace-status-bar
└──────────┴───────────────────────────────────┘
```

### 영역

- **타이틀바** (최상단, OS별 CSD) → [`features/window-chrome/`](../window-chrome/index.md)
- **사이드바** (좌측, 전체 높이) → [`features/sidebar/`](../sidebar/index.md)
- **작업 영역** (중앙) — Workspace/Pane/Tab/Surface 도메인 + 두 레벨 레이아웃 → [`features/work-area/`](../work-area/index.md). 탭 스트립 시각은 [`features/workspace-tabs/`](../workspace-tabs/index.md).
- **상태바** (하단) → [`features/workspace-status-bar/`](../workspace-status-bar/index.md)

### 상태별 시각

- Workspace 0개/전환 등 상태는 각 하위 feature(사이드바·탭) 문서에서 다룬다. MainView 화면 자체는 영역 배치만 정의한다.

### 시각 소스

tasty-page `vendor/ui_kits/terminal/` — 전체 셸의 치수·색·영역 배치 단일 출처. 스크린샷: tasty-page `vendor/screens/sidebar-full.png`, `terminal-surface.png` 등.
