# 작업 영역 화면

- **부모 기획**: [../index.md](../index.md)
- **시각 소스**: `site/vendor/ui_kits/terminal/work.jsx` — claude design

[MainView](../../main-view/screens/main-view.md) 중앙. 이 화면은 부모 기획의 **두 레벨 레이아웃**(상위 Pane / 하위 Surface)을 투영한다 — 동작 정의는 부모에, 여기선 시각 배치만. 위임 요소는 링크만 둔다(연결 개념).

## 트리거

MainView 가 열리면 항상 표시(중앙 고정 영역). 사이드바에서 Workspace 를 전환하면 해당 Workspace 의 Pane/Tab/Surface 트리로 내용이 바뀐다.

## UI 요소 인벤토리

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
- **탭 스트립** (각 Pane 상단) — 그 Pane 의 탭 목록 + active 탭 강조. 시각/드래그/추가 버튼은 → [`features/workspace-tabs/`](../../workspace-tabs/index.md). 표시명 규칙은 부모 기획.
- **Surface 타일(하위 레이아웃)** — active 탭의 SurfaceLayout 을 타일로 렌더. 분할 시 surface 사이 경계는 `SURFACE_BORDER_WIDTH`. 포커스된 surface 강조.
- **Surface 콘텐츠** — 타입별로 다르게 렌더(terminal=GPU, image=egui-mesh, markdown/html=WebView, empty=타입 선택 UI). 종류 표는 부모 기획 [Surface 종류](../index.md#surface-종류).
- **Empty surface** — 빈 자리. 타입 선택 버튼을 보여 다른 종류로 전환. deferred 터미널이면 PTY 준비 전 표시.

## 상태별 시각

- **단일 / 분할** — Pane·Surface 모두 1개면 보더 없음, 분할되면 방향(좌우/상하)·비율(`ratio`)대로 타일 + 보더.
- **포커스** — 포커스된 Pane / focused_surface 가 강조된다.
- **탭 전환** — 하위 레이아웃 전체가 함께 전환(상위 Pane 분할은 불변).
- **deferred / readonly** — deferred 터미널은 PTY 준비 전, attach 점유된 surface 는 readonly mirror 로 표시(내용 보임 + 조작 차단).

## 시각 소스

`site/vendor/ui_kits/terminal/work.jsx` — 작업 영역 치수·보더·타일 배치의 단일 출처. 보더 폭은 코드 상수와 일치하되 **두 상수의 좌표계가 다르다**: `PANE_BORDER_WIDTH` 는 논리 2px(디자인이 정한 두께라 배율을 따라 커진다 — 배율 2 에서 4 물리px), `SURFACE_BORDER_WIDTH` 는 물리 1px(hairline 이라 밀도와 무관하게 1 device px 로 남는다). 근거는 [`docs/adr/0148-physical-px-constants-are-split-by-what-they-are-for.md`](../../../adr/0148-physical-px-constants-are-split-by-what-they-are-for.md).
</content>
