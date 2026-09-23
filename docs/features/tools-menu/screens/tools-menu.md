# 도구 메뉴 화면

- **부모 기획**: [../index.md](../index.md)
- **트리거 위치**: [사이드바](../../sidebar/screens/sidebar.md) 하단 **도구 버튼**
- **시각 소스**: `site/vendor/ui_kits/terminal/overlays/tools_menu.jsx` — claude design

각 항목은 이름 + (있으면) 한 줄 + 해당 기능 문서 **링크만** — 항목 내용은 그 문서에 (연결 개념).

## 트리거

사이드바 하단 **도구 버튼** 클릭 → 버튼 위치에 메뉴 popup 이 뜬다.

## 레이아웃

```
┌─────────────────────────┐
│ Command palette          │  → command-palette
│ Listening ports          │  → listening-ports
│ Remote connections       │  → remote_tool
│ Presets                  │  → preset
│ Tutorial                 │  → tutorial
│ Task DAGs                │  → dag-list (workspace 스코프)
│ Open File                │  → file-picker
├─────────────────────────┤  (빌트인 ↔ 플러그인 구분선)
│ Clipboard Viewer         │  → (플러그인 기여)
│ …(plugin 항목)           │
└─────────────────────────┘
```

## UI 요소 인벤토리

- **빌트인 항목** (각 → 해당 기능):
  - **Command palette** — 명령 팔레트를 연다. → [`features/command-palette/`](../../command-palette/index.md)
  - **Listening ports** — 리스닝 포트 뷰어를 연다. → [`features/listening-ports/`](../../listening-ports/index.md)
  - **Remote connections** — 원격 접속 도구를 연다. → [`features/remote-profiles/`](../../remote-profiles/index.md)
  - **Presets** — 레이아웃 프리셋 창을 연다. → [`features/layout-presets/`](../../layout-presets/index.md)
  - **Tutorial** — 튜토리얼 토픽 popup 을 연다. → [`features/tutorial/`](../../tutorial/index.md)
  - **Task DAGs** — 활성 워크스페이스의 DAG 목록 popup 을 연다. → [`features/agent-collaboration/`](../../agent-collaboration/index.md)
  - **Open File** — 네이티브 파일 피커를 연다. → [`features/native-file-picker/`](../../native-file-picker/index.md)
- **구분선** — 빌트인과 플러그인 항목 사이 (둘 다 있을 때만).
- **플러그인 기여 항목** — `ui.tool_item` 권한 플러그인이 추가한 항목 (예: Clipboard history). **이 문서엔 항목을 나열하지 않는다** — 공식(번들) 플러그인 메뉴는 [번들 플러그인 문서](../../../plugins/index.md)에서 다룬다.

## 상태별 시각

- **빌트인만 / 플러그인 포함**: 플러그인 항목 유무에 따라 구분선·높이가 달라진다 (동적 크기).

## 시각 소스

`design-system/ui_kits/terminal/overlays/tools_menu.jsx` — 메뉴 치수·항목 행·구분선의 단일 출처. 스크린샷: `design-system/assets/screens/tools_menu.png`, `tools_menu-ko.png`. (design-system vendor 후 링크 resolve.)
