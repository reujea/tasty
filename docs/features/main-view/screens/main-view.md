# MainView 화면 (전체 레이아웃)

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: `site/vendor/ui_kits/terminal/` (`chrome.jsx`, `work.jsx`) — claude design

이 화면은 **합성 화면** 이다 — 각 영역은 *자기 위치/역할만* 적고, 내용은 하위 feature 문서로 **링크만** 한다 (연결 개념).

## 레이아웃

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

## 영역

- **타이틀바** (최상단, OS별 CSD) → [`features/window-chrome/`](../../window-chrome/index.md)
- **사이드바** (좌측, 전체 높이) → [`features/sidebar/`](../../sidebar/index.md)
- **작업 영역** (중앙) — Workspace/Pane/Tab/Surface 도메인 + 두 레벨 레이아웃 → [`features/work-area/`](../../work-area/index.md). 탭 스트립 시각은 [`features/workspace-tabs/`](../../workspace-tabs/index.md).
- **상태바** (하단) → [`features/workspace-status-bar/`](../../workspace-status-bar/index.md)

## 상태별 시각

- Workspace 0개/전환 등 상태는 각 하위 feature(사이드바·탭) 문서에서 다룬다. MainView 화면 자체는 영역 배치만 정의한다.

## 시각 소스

`site/vendor/ui_kits/terminal/` — 전체 셸의 치수·색·영역 배치 단일 출처. 스크린샷: `site/vendor/screens/sidebar-full.png`, `terminal-surface.png` 등.
