# 도구 메뉴 (Tools menu)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **설계**: 없음
- **코드**: `src/adapters/ui/tools_menu.rs`, `src/adapters/ui/sidebar/tools.rs`
- **화면**: [아래 절](#화면)

## 목적

[사이드바](../sidebar/index.md) 의 **도구 버튼**이 여는 메뉴. 빌트인 + 플러그인 기여 도구 항목을 한곳에 모아 실행 진입점을 제공한다. 각 항목의 세부 동작은 해당 기능 문서에서 설명한다.

## 내부 동작

### 두 출처

- **빌트인 항목** (`BUILTIN_TOOLS`, 플러그인 무관 — 현재 7개):
  - Command palette → 명령 팔레트 popup
  - Listening ports → 리스닝 포트 popup
  - Remote connections → 원격 접속 popup
  - Presets → 레이아웃 프리셋 창
  - Tutorial → 튜토리얼 토픽 popup
  - Task DAGs → DAG 목록 popup (**workspace 스코프** — 아래 참조)
  - Open File → 네이티브 파일 피커
- **플러그인 기여 항목**: 활성 + `ui.tool_item` 권한을 grant 받은 plugin 이 `[[contributes.tool]]` 로 선언한 항목. `MainViewState::tool_registry` 에 동기화된다.

### 레이아웃 / 크기

빌트인 섹션 + (둘 다 있으면 구분선) + 플러그인 섹션. 메뉴 높이는 **현재 등록된 항목 수로 매 프레임 동적 계산**(`tools_menu_sizer`). 높이는 위아래 안쪽 여백 `popup-content-margin` 둘, 행 높이 `menu-item-height` × 항목 수, 구분선 여백의 합이며 여분을 더하지 않는다. 그래서 위아래 여백이 같다.

### 항목 실행 (`invoke_tool`)

- **빌트인**: 해당 popup 을 연다 (`BuiltinAction::OpenPopup` — 중앙 정렬 + 포커스). 대상 스코프를 여는 시점에야 아는 항목은 `BuiltinAction::OpenWorkspacePopup` 으로 갈라져 `OpenPopupMode::WithScope(PopupScope::Workspace(활성 workspace ID))` 를 주입한다(현재 Task DAGs 하나). 별도 winit 창(`OpenWindow`)·파일 피커(`OpenFilePicker`)도 각각 자기 분기를 쓴다.
- **플러그인**: `ToolAction` 종류별 — 이벤트 발생 또는 `<plugin_id>/<popup_id>` 형식 popup open (활성 surface 의 상속 cwd 를 실어 전달).

## 인터페이스

- **사용자**: 사이드바 도구 버튼 클릭 → 메뉴 표시, 항목 클릭 → 실행.
- **각 항목은 그 기능으로 연결**:
  - Command palette → [`features/command-palette/`](../command-palette/index.md)
  - Listening ports → [`features/listening-ports/`](../listening-ports/index.md)
  - Remote connections → [`features/remote-profiles/`](../remote-profiles/index.md)
  - Presets → [`features/layout-presets/`](../layout-presets/index.md)
  - Tutorial → [`features/tutorial/`](../tutorial/index.md)
  - Task DAGs → [`features/agent-collaboration/screens/dag-list-popup.md`](../agent-collaboration/screens/dag-list-popup.md)
  - Open File → [`features/native-file-picker/`](../native-file-picker/index.md)
  - 플러그인 기여 도구 → **[번들 플러그인 문서](../../plugins/index.md)** (예: [clipboard-viewer](../../plugins/clipboard-viewer/index.md) · [git-viewer](../../plugins/git-viewer/index.md)). 이 메뉴 문서에는 항목을 나열하지 않는다 — 공식 플러그인 메뉴는 해당 플러그인 쪽에서 다룬다.

## 비-목표

- 각 도구 항목의 *내용/동작* — 메뉴는 목록 + 실행 진입점만. 내용은 각 기능 문서.
- 플러그인 도구의 기여/권한 메커니즘 — `features/plugin-system/` 영역.

## Acceptance Criteria

- 사이드바 도구 버튼 클릭 시 빌트인 7개(Command palette / Listening ports / Remote connections / Presets / Tutorial / Task DAGs / Open File)가 표시된다.
- `ui.tool_item` 권한을 가진 활성 플러그인의 기여 항목이 빌트인 아래 구분선과 함께 추가된다.
- 항목 클릭 시 해당 popup(빌트인) 또는 plugin action(플러그인)이 실행된다.
- 등록 항목 수에 따라 메뉴 높이가 달라진다.
- 메뉴 폭은 가장 넓은 항목 라벨에 행 패딩(`menu-item-padding-x` 양쪽), 안쪽 고리(`popup-content-margin` 양쪽), 테두리를 더한 값이다. 테두리를 포함해 `tools-menu-min-width`(160)보다 좁아지지 않고 `tools-menu-max-width`(240)보다 넓어지지 않는다.
- 상한에 걸린 라벨은 끝을 말줄임표로 줄인다. 번역 라벨도 언어별 폭 없이 같은 규칙으로 맞춘다.
- 항목 행은 공용 MenuItem(`menu-item-height` 28, 행 사이 간격 0)이다. 글자는 `menu-item-fg`(text-secondary)이고 호버하면 `menu-item-fg-hover`(text-on-active)와 `menu-item-bg-hover` 배경(`menu-item-radius`)으로 바뀐다.

> GUI 메뉴라 시각 검증은 스크린샷, 항목 등록/실행은 `debug.tool.list`/`debug.tool.invoke`(debug IPC)로 검증 가능.

## 구현

- `src/adapters/ui/tools_menu.rs` — `BUILTIN_TOOLS`, `draw_tools_menu`, `invoke_tool`, `tools_menu_sizer`.
- `src/adapters/ui/sidebar/tools.rs` — `open_tools_menu` (도구 버튼 → 메뉴 popup). 열기 전에 `measure_on_open`으로 폭을 재고, 그리는 동안에도 매 프레임 다시 재어 플러그인 항목 변화를 따른다. 폭 계산은 `tasty-ui-widgets`의 `fit_menu_width`, 행은 `menu_item`을 갤러리와 함께 쓴다. 라벨 말줄임은 공용 행이 처리한다.
- 플러그인 항목: `MainViewState::tool_registry` (plugin `[[contributes.tool]]` 동기화).

## 화면

화면정의서 — **도구 메뉴 화면**.

- **트리거 위치**: [사이드바](../sidebar/index.md#화면) 하단 **도구 버튼**
- **시각 소스**: tasty-page `vendor/ui_kits/terminal/overlays/tools_menu.jsx`

아래에서는 메뉴의 배치를 설명한다. 항목별 동작은 연결된 기능 문서를 참고한다.

### 트리거

사이드바 하단 **도구 버튼** 클릭 → 버튼 위치에 메뉴 popup 이 뜬다.

### 레이아웃

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

### UI 요소 인벤토리

- **빌트인 항목** (각 → 해당 기능):
  - **Command palette** — 명령 팔레트를 연다. → [`features/command-palette/`](../command-palette/index.md)
  - **Listening ports** — 리스닝 포트 뷰어를 연다. → [`features/listening-ports/`](../listening-ports/index.md)
  - **Remote connections** — 원격 접속 도구를 연다. → [`features/remote-profiles/`](../remote-profiles/index.md)
  - **Presets** — 레이아웃 프리셋 창을 연다. → [`features/layout-presets/`](../layout-presets/index.md)
  - **Tutorial** — 튜토리얼 토픽 popup 을 연다. → [`features/tutorial/`](../tutorial/index.md)
  - **Task DAGs** — 활성 워크스페이스의 DAG 목록 popup 을 연다. → [`features/agent-collaboration/`](../agent-collaboration/index.md)
  - **Open File** — 네이티브 파일 피커를 연다. → [`features/native-file-picker/`](../native-file-picker/index.md)
- **구분선** — 빌트인과 플러그인 항목 사이 (둘 다 있을 때만).
- **플러그인 기여 항목** — `ui.tool_item` 권한 플러그인이 추가한 항목 (예: Clipboard Viewer). **이 문서엔 항목을 나열하지 않는다** — 공식(번들) 플러그인 메뉴는 [번들 플러그인 문서](../../plugins/index.md)에서 다룬다.

### 상태별 시각

- **빌트인만 / 플러그인 포함**: 플러그인 항목 유무에 따라 구분선·높이가 달라진다 (동적 크기).

### 시각 소스

tasty-page `vendor/ui_kits/terminal/overlays/tools_menu.jsx` — 메뉴 치수·항목 행·구분선의 단일 출처. 스크린샷: tasty-page `vendor/screens/tools_menu.png`, `tools_menu-ko.png`.
