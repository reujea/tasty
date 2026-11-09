# 사이드바 (Sidebar)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **설계**: 없음
- **코드**: `src/adapters/ui/sidebar/`
- **화면**: [아래 절](#화면)

## 목적

[MainView](../main-view/index.md) 좌측 패널. 워크스페이스를 전환·관리하고, 도구/플러그인/설정으로 들어가는 진입점을 모아둔 영역.

## 내부 동작

### 두 상태 — full / collapsed

접기 버튼으로 full ↔ collapsed 토글. collapsed 는 아이콘만 남긴 좁은 형태 (카테고리 토글 on 이면 레일에 카테고리 경계 `---` 버튼도 놓인다 — 아래 영역 구성).

### 폭

full 의 폭은 `appearance.ui_scale` 이 정한다(small 150 · medium 180 · large 220, `AppearanceSettings::scaled_sidebar_width`). collapsed 는 48, 숨김은 0 이다. 끌어서 폭을 바꾸는 경로는 없다(`resizable(false)`). 부팅은 펼친 상태로 시작하므로 첫 터미널 격자도 같은 `scaled_sidebar_width` 로 계산한다.

사이드바 폭만 따로 정하는 설정 키는 없다. 예전 `appearance.sidebar_width` 는 읽었지만 적용되지 않던 키라 폐기했다. 이 키가 남은 config.toml 도 오류 없이 읽고, 경고 로그(`settings key appearance.sidebar_width is no longer used …`)를 남긴 뒤 무시한다. 다음 저장에서 키가 빠진다(`Settings::RETIRED_KEYS`).

### 영역 구성

- **헤더**: 워드마크 `tasty.` + 수박 로고 + 접기 버튼.
- **워크스페이스 영역** (남는 높이 전부): "Workspaces" heading + 워크스페이스 카드 목록 + New workspace 버튼.
  - **카테고리 토글 on** (설정 → 일반 → "워크스페이스 카테고리", `workspace_categories_enabled`): 같은 영역이 **카테고리 섹션으로 그룹 렌더**된다 — full 은 카테고리 헤더 + 소속 카드, collapsed 레일은 카테고리 경계 `---` 버튼 + 소속 아바타 — 그리고 New workspace 버튼은 full/collapsed 양쪽에서 숨는다. 섹션·헤더·레일 팝업의 동작과 시각은 [`features/workspace-category/`](../workspace-category/index.md) 가 정의한다.
- **하단**: 현재 창의 미확인 알림 요약, 도구 / 플러그인 / 설정 버튼. 알림 요약은 full에서 레벨별 숫자와 최신 미확인 제목, collapsed에서 벨과 세로 배지를 표시한다. 워크스페이스 attention 배지와 별도이며 [알림](../notifications/index.md)에서 동작을 정의한다.

### 워크스페이스 조작

- **전환**: 카드 클릭 → 해당 Workspace 활성.
- **활성 카드 자동 스크롤**: 전환(클릭·quick-switch 단축키·카테고리 경계 이동 등, 전부 단일 관문
  `MainViewState::switch_workspace` 경유) 으로 활성 Workspace 가 바뀌었을 때 그 카드가 목록
  `ScrollArea` 뷰포트 밖이면 자동으로 스크롤해 보이게 한다. 활성 인덱스가 실제로 바뀐
  프레임에만 최소 이동으로 보정하므로(egui `scroll_to_rect`, align 없음) 사용자가 수동으로
  스크롤해 둔 상태를 매 프레임 덮어쓰지 않는다. collapsed 사이드바는 `ScrollArea` 자체가
  없어 해당 없음.
- **재정렬**: 카드 drag-and-drop (full). 카테고리 토글 on 이면 다른 카테고리 섹션에 드롭해 소속을 옮기는 경로가 추가된다 → [`features/workspace-category/`](../workspace-category/index.md).
  같은 목록·카테고리 안에서는 표시선의 삽입 슬롯에 놓인다. 이동 전 슬롯을 이동 후 전역 인덱스로
  바꿀 때 아래 방향은 한 칸 보정하며, 자기 앞뒤 슬롯은 순서를 바꾸지 않는다.
- **생성**: New workspace 버튼(좌클릭) / 우클릭 → 프리셋으로 생성 — 카테고리 토글 off 일 때. on 이면 버튼이 숨고 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 생성한다 (경로 전체는 → [`features/workspace-category/`](../workspace-category/index.md)).
- **hover**: 포인터를 올리면 카드 전체를 `overlay-hover`로, 누르면 `overlay-active`로 채운다. 선택 카드는 `surface-active` 위에 같은 overlay를 합성한다. 채움은 점·스피너·키캡·글자 아래에 깐다. 이름·mirror 글리프·부제의 색은 `RowState`가 상태별로 고른다. hover 판정은 카드 전체의 클릭 응답을 따른다.
- **실행 중 surface 개수**: 펼친 행의 점 칸 바로 뒤, 이름 앞에 채움 없는 mono 숫자로 표시한다. 기준은 실행 중 점과 같은 `busy_count`다. 숨은 탭의 터미널을 세고 플러그인 surface와 markdown·explorer는 세지 않는다. 값이 0이면 숫자와 툴팁이 없고 칸은 비워 두므로 이름 위치가 그대로다. 100 이상은 `99+`이고 그 행의 칸만 늘어나 이름·REMOTE 줄·부제·설명이 함께 밀린다. 숫자 칸에 포인터를 올리면 `sidebar.running_surfaces_tooltip` 툴팁이 정확한 값을 보여 준다. 행의 전환·드래그·우클릭은 숫자 칸 위에서도 같다. 접힌 레일과 드래그 고스트에는 숫자가 없다.
- **attach 인디케이터**: 다른 client(원격 사용자)가 그 Workspace 를 점유 중이면 status dot 둘레의 attached ring(`border_attached`, 활성 행은 `status-dot-attached-ring-active` — red 는 error 전용) ([점유 모델](../../concepts/actors.md)). 점 칸에 포인터를 올리면 `attach.occupied_workspace` 툴팁을 띄운다. 카드 전체가 클릭 위젯이라 egui 는 그보다 먼저 등록한 hover 영역을 hover 로 보지 않는다. 점 칸과 아래 REMOTE 줄의 툴팁 영역은 카드 응답 뒤에 등록한다.
- **mirror 인디케이터**: 그 Workspace 가 원격 인스턴스의 로컬 mirror([remote-attach](../remote-attach/index.md))이면 — full 은 이름과 subtitle 사이 별도 줄에 `workspace-mirror-fg` 글리프(`>_→`, 활성 행은 `workspace-mirror-fg-active`)와 Tag `info` "REMOTE", collapsed 레일은 아바타 우하단 corner chip — 12 원 바탕은 `workspace-mirror-chip-bg`(= bg-sidebar), 원 안쪽에 1px `workspace-mirror-chip-border`(= workspace-mirror-fg, 하늘색) 테두리, 8 glyph, 바깥 2px bg-sidebar halo. 테두리 덕분에 아바타가 idle·active·hover 어느 바탕이어도 칩이 보인다. 12 원은 move·refusal 칩과 같은 크기 토큰을 쓴다. 좌측 status dot 은 실행상태(running/idle) 전용이라 mirror 색을 싣지 않으며, notif(우상단)·attached(둘레 ring)와 시각 채널이 분리된다. full 의 REMOTE 줄에 포인터를 올리면 `attach.mirror_workspace` 툴팁을 띄운다. 로딩 중인 카드의 툴팁 둘째 줄은 점 칸과 REMOTE 줄에서는 띄우지 않아 두 툴팁이 겹치지 않는다.

## 인터페이스

- **사용자**: 클릭(전환/버튼), 드래그(재정렬), 접기 토글.
- **워크스페이스 영역의 카테고리 조작은 다른 기능으로 위임** (토글 on 시 헤더 클릭/우클릭·레일 `---` 클릭·섹션 간 드래그): → [`features/workspace-category/`](../workspace-category/index.md)
- **하단 버튼은 다른 기능으로 위임** (세부 동작은 각 기능 문서 참고):
  - 알림 요약 → [`features/notifications/`](../notifications/index.md)
  - 도구 버튼 → [`features/tools-menu/`](../tools-menu/index.md)
  - 플러그인 버튼 → [`features/plugin-system/`](../plugin-system/index.md)
  - 설정 버튼 → [`features/settings/`](../settings/index.md)

## 비-목표

- 워크스페이스/Pane/Tab/Surface 자체의 동작 정의 — 사이드바는 *전환·목록* 만. 도메인은 [구조 계층](../../concepts/hierarchy.md) 및 해당 feature.
- 도구/플러그인/설정 *창의 내용* — 버튼은 진입점일 뿐, 내용은 각 feature.
- 카테고리 자체의 동작(CRUD·전환·접힘 영속·키캡) — 사이드바는 그룹 렌더의 *자리* 만 제공, 동작은 [`features/workspace-category/`](../workspace-category/index.md).

## Acceptance Criteria

- 접기 버튼 클릭 시 full ↔ collapsed 가 토글된다.
- 워크스페이스 카드 클릭 시 해당 Workspace 로 전환된다.
- Given 평면 목록 A B C D When A를 C와 D 사이의 표시선에 놓는다 Then 순서는 B C A D가 된다. 위 방향과 목록 처음·끝도 표시선의 슬롯과 결과가 일치한다.
- Given 워크스페이스 카드를 드래그 중 When 자기 바로 앞이나 바로 뒤 슬롯에 놓는다 Then 순서 변경을 요청하지 않는다. 카테고리 안에서 전역 인덱스가 떨어져 있는 이웃 카드도 같다.
- Given 카테고리 normal에 a0 a1, 다음 섹션에 b0 b1이 있다 When a0를 normal의 마지막 카드 아래에 놓는다 Then 표시선은 normal 안의 a1 아래에 있고 normal 순서는 a1 a0가 된다. 슬롯 후보는 포인터가 속한 섹션의 카드만 사용한다.
- Given 빈·접힌 카테고리에 카드를 드래그한다 When 그 섹션에 포인터가 있다 Then 표시선은 해당 헤더 아래에 있다. 다른 카테고리로의 드롭은 기존 소속 변경·전역 인덱스 유지 계약을 따른다.
- 워크스페이스 이름·부제 변경 팝업이 열린 동안 에이전트가 다른 Workspace 를 닫거나 옮겨 인덱스가 바뀌어도, 저장한 값은 팝업을 연 Workspace 에 적용된다. 그 Workspace 가 닫히면 팝업이 닫힌다.
- Given 워크스페이스 우클릭 메뉴가 열림 When 에이전트가 다른 Workspace 를 닫거나 옮겨 인덱스가 바뀐다 Then 고른 항목(이름 변경·이동·프리셋 저장·닫기·연결 끊기·카테고리 이동)은 메뉴를 연 Workspace 에 적용된다. 그 Workspace 가 먼저 닫혔으면 아무것도 하지 않는다.
- Given 원격 mirror 카드 When 우클릭 Then 맨 아래 묶음에 **워크스페이스 연결 끊기**(로컬 mirror 만 정리)와 **워크스페이스 닫기**(원격 서버에서 닫기)가 함께 있다. 로컬 카드에는 닫기만 있다([원격 attach](../remote-attach/index.md#mirror-닫기와-연결-끊기)).
- 카테고리 토글 off: New workspace 버튼으로 새 Workspace 가 생성된다 (우클릭 시 프리셋).
- 카테고리 토글 on: New workspace 버튼이 full/collapsed 모두에서 숨고, 워크스페이스 영역이 카테고리 섹션으로 그룹 렌더된다 (섹션·생성 경로의 세부 AC 는 [`features/workspace-category/`](../workspace-category/index.md)).
- 다른 client 가 점유 중인 Workspace 카드에 점유 인디케이터가 표시된다.
- 도구/플러그인/설정 버튼이 각각 도구 메뉴 / 플러그인 창 / 설정 창을 연다.

> GUI 컴포넌트라 시각 검증은 스크린샷, 전환/생성 동작은 IPC/CLI(workspace 조작) + 스크린샷 병행.

## 구현

- `src/adapters/ui/sidebar/full.rs` (full), `collapsed.rs` (collapsed), `view.rs` (공통 레이아웃 — 헤더/하단/목록, 카테고리 on 시 섹션 헤더·레일 `---` 버튼 포함), `tools.rs` (도구 버튼 → 도구 메뉴).
- 헤더 로고: `assets/icons/icon_256.png`. 하단 버튼 아이콘: `icons::{TOOLS, PLUG, SETTINGS}`.

## 화면

화면정의서 — **사이드바 화면**.

- **상위 화면**: [MainView 전체 레이아웃](../main-view/index.md#화면) 의 좌측 영역
- **시각 소스**: tasty-page `vendor/ui_kits/terminal/chrome.jsx`

아래에서는 영역의 위치와 역할을 설명한다. 버튼이 여는 기능은 각 문서에서 다룬다.

### 레이아웃 (full)

```
┌──────────────────┐
│ tasty.   [접기]   │  헤더 — 워드마크 + 수박 로고 + 접기 버튼
├──────────────────┤
│ Workspaces       │  ← 섹션 heading
│ ┌──────────────┐ │
│ │ workspace A  │ │  워크스페이스 카드 (클릭 전환 / 드래그 재정렬 /
│ │ workspace B ●│ │   점유 시 인디케이터 ●)
│ └──────────────┘ │
│ [+ New workspace]│
│        ⋮         │  (남는 높이 전부 워크스페이스 영역)
├──────────────────┤
│ ⚙ 도구            │  → features/tools-menu/ 참조
│ 🔌 플러그인        │  → features/plugin-system/ 참조
│ ⚙ 설정            │  → features/settings/ 참조
└──────────────────┘
```

#### 카테고리 토글 on

워크스페이스 카테고리 토글이 on 이면 워크스페이스 영역만 아래처럼 바뀐다 (헤더·하단은 동일).

```
│ ▼ Workspaces     │  ← normal 카테고리 헤더 (클릭 접힘 토글 / 우클릭 메뉴)
│ ┌──────────────┐ │
│ │ workspace A  │ │  소속 카드 (다른 섹션으로 드래그 → 카테고리 이동)
│ └──────────────┘ │
│ ▶ work           │  ← 접힌 카테고리 (헤더만)
│        ⋮         │  ([+ New workspace] 버튼 없음 — 헤더 메뉴·레일 팝업으로 생성)
```

collapsed 레일은 카테고리마다 경계 `---` 버튼 + 소속 아바타로 같은 그룹을 그리고, `---` 클릭 → 우측 앵커드 팝업. 헤더/`---`/팝업의 요소 구성·색·키캡 등 시각과 동작은 [`features/workspace-category/`](../workspace-category/index.md) 가 단일 출처다.

### UI 요소 인벤토리

- **헤더**: 워드마크 `tasty.` + 수박 로고(`icon_256.png`) + **접기 버튼**(full↔collapsed 토글).
- **워크스페이스 영역** (남는 높이): 토글 off — `Workspaces` heading → 워크스페이스 카드 목록 → `New workspace` 버튼. 토글 on — **카테고리 섹션**(헤더 + 소속 카드; 레일은 `---` 버튼 + 아바타)으로 그룹. → [`features/workspace-category/`](../workspace-category/index.md)
  - 카드: 클릭=전환, 드래그=재정렬(토글 on 이면 다른 섹션 드롭 = 카테고리 이동), 점유 중이면 인디케이터.
  - `New workspace`(+) 버튼은 **워크스페이스 카테고리 토글 off 일 때만** 표시 (full/collapsed 공통). on 이면 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 생성 (배경 우클릭은 새 카테고리·원격 워크스페이스 추가). → [`features/workspace-category/`](../workspace-category/index.md)
  - 카테고리 헤더(full) / `---` 경계 버튼(collapsed) — 토글 on 에만 존재. 접힘 토글·컨텍스트 메뉴·앵커드 팝업의 요소와 동작은 → [`features/workspace-category/`](../workspace-category/index.md)
- **하단 버튼**:
  - **도구** (`icons::TOOLS`) — 클릭 시 **도구 메뉴**를 연다 (리스닝 포트 등 빌트인 진단/유틸 항목 모음). → [`features/tools-menu/`](../tools-menu/index.md)
  - **플러그인** (`icons::PLUG`) — **플러그인 관리 창**을 연다 (설치/활성·비활성). → [`features/plugin-system/`](../plugin-system/index.md)
  - **설정** (`icons::SETTINGS`) — **설정 창**을 연다 (탭별 환경설정). → [`features/settings/`](../settings/index.md)

### 상태별 시각

- **full / collapsed**: collapsed 는 아이콘만 남긴 좁은 형태 (워드마크/라벨 숨김, 카드→정사각 슬롯; 카테고리 on 이면 `---` 경계 버튼 추가).
- **카테고리 on**: 워크스페이스 영역이 섹션 헤더(full)·`---` 경계 버튼(collapsed)으로 그룹되고 `New workspace` 버튼이 사라진다. 카테고리 조합 홀드 시 헤더/`---` 에 숫자 키캡. 헤더·레일·키캡의 시각과 갤러리 specimen 위치는 [`features/workspace-category/`](../workspace-category/index.md) 참조.
- **워크스페이스 점유**: 다른 client 가 점유 중인 카드에 인디케이터 표시.
- **드래그 중**: 카드 재정렬 드래그 스냅샷.

### 시각 소스

tasty-page `vendor/ui_kits/terminal/chrome.jsx` — 사이드바 치수·색·로고·버튼 배치의 단일 출처. 스크린샷: tasty-page `vendor/screens/sidebar-full.png`, `sidebar-collapsed.png`, `sidebar-workspaces.png`.
