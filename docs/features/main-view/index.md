# MainView (메인 윈도우)

- **Status**: Implemented
- **주체**: 로컬 사용자(GUI 직접) · AI Agent(IPC/CLI 로 내부 surface/tab/workspace 조작) · 원격 접속 사용자(내부 surface/workspace 점유)
- **ADR**: 없음
- **코드**: `src/view/main.rs`, `src/view/main/`, `src/app/window_lifecycle.rs`
- **화면**: [screens/main-view.md](screens/main-view.md)

## 목적

tasty 의 주 윈도우. 워크스페이스를 호스팅하고 사이드바·탭·surface·상태바로 사용자가 실제 작업하는 메인 화면이다. `View` + `sealed::Sealed` 를 직접 구현한다 ([구조 계층](../../concepts/hierarchy.md)).

## 내부 동작

### 무엇을 호스팅하나

`MainView` 는 도메인 트리(`CoreState` — Workspace › Pane › Tab › Surface)와 UI 상태(`AppState`)를 함께 보유한다. 한 MainView 가 **여러 Workspace** 를 갖고 사이드바에서 전환한다 (계층·두 레벨 레이아웃은 [hierarchy](../../concepts/hierarchy.md)).

### 멀티 윈도우

`create_new_window` 로 **MainView 를 여러 개** 띄울 수 있다. 각 MainView 는 독립 winit Window(1:1)이고 자기 `CoreState`/`AppState` 를 가진다. 엔진은 전부 `views: HashMap<WindowId, Box<dyn View>>` 로 관리.

### headless 와의 관계

`MainView` 는 `CoreState` 위에 얹힌 **GUI 셸** 이다. headless 에선 MainView 가 없고 `CoreState`(Workspace/Surface/PTY) 만 동작한다 — 즉 도메인은 MainView 없이도 살아있고, MainView 는 그것을 사람에게 보여주는 투영. (→ [identity](../../identity.md) headless)

### 크롬 합성

화면은 **사이드바 + 작업 영역(탭 스트립 + surface 들) + 상태바** 로 구성되고, 윈도우 테두리는 CSD 타이틀바다. 각 영역의 상세는 화면 문서에서 하위 feature 로 연결한다 (연결 개념).

### 입력

키보드/마우스/IME/vi-copy(키보드 복사 모드)/텍스트 선택/클립보드/링크 hover 등 사용자 입력을 받아 처리한다. 세부 동작은 각 해당 feature 문서.

## 인터페이스

- **사용자**: GUI 직접 입력(단축키/마우스). 사이드바·탭·surface 조작.
- **AI Agent (IPC/CLI)**: 이 윈도우 *안의* surface/tab/workspace 를 ID 로 생성·조회·닫기 등. (MainView 자체를 여는 건 사용자 행동 — 멀티 윈도우 생성 단축키.)
- **원격 접속 사용자**: 내부 surface/workspace 를 attach 로 점유.

## 비-목표

- 단일 surface/workspace 만 가지는 경량 View 는 별도 — MainView 는 풀 셸이다.
- 모달/에디터 계열 윈도우(설정·preset 등)는 MainView 가 아니라 다른 View 구현체.

## Acceptance Criteria

- 앱 시작 시 MainView 가 기본 Workspace + 터미널 Surface 1개로 열린다.
- 사이드바에서 Workspace 를 전환하면 작업 영역이 해당 Workspace 로 바뀐다.
- 새 윈도우(단축키) 시 독립 `CoreState` 를 가진 MainView 가 추가로 열린다.
- AI Agent 가 IPC/CLI 로 특정 MainView 의 surface/tab/workspace 를 ID 로 조작할 수 있다.

> 윈도우 셸이라 시각 검증은 스크린샷, 멀티 윈도우/도메인 동작은 IPC/CLI 시나리오로 검증.

## 구현

- struct: `src/view/main.rs` `MainView` (`ViewBase` + `CoreState` + `AppState` + 입력 상태).
- 렌더: `src/view/main/redraw.rs` (`handle_redraw` 경로. `View::render` 는 trait 호환용 빈 구현).
- 멀티 윈도우 생성: `src/app/window_lifecycle.rs` `create_new_window` → `views.insert`.
- 사이드바/크롬: `src/adapters/ui/sidebar/`.

## 화면

- [screens/main-view.md](screens/main-view.md) — 전체 레이아웃(사이드바/작업영역/상태바/타이틀바)과 각 영역의 하위 feature 연결.
