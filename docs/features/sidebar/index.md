# 사이드바 (Sidebar)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **ADR**: 없음
- **코드**: `src/adapters/ui/sidebar/`
- **화면**: [아래 절](#화면)

## 목적

[MainView](../main-view/index.md) 좌측 패널. 워크스페이스를 전환·관리하고, 도구/플러그인/설정으로 들어가는 진입점을 모아둔 영역.

## 내부 동작

### 두 상태 — full / collapsed

접기 버튼으로 full ↔ collapsed 토글. collapsed 는 아이콘만 남긴 좁은 형태 (카테고리 토글 on 이면 레일에 카테고리 경계 `---` 버튼도 놓인다 — 아래 영역 구성).

### 영역 구성

- **헤더**: 워드마크 `tasty.` + 수박 로고 + 접기 버튼.
- **워크스페이스 영역** (남는 높이 전부): "Workspaces" heading + 워크스페이스 카드 목록 + New workspace 버튼.
  - **카테고리 토글 on** (설정 → 일반 → "워크스페이스 카테고리", `workspace_categories_enabled`): 같은 영역이 **카테고리 섹션으로 그룹 렌더**된다 — full 은 카테고리 헤더 + 소속 카드, collapsed 레일은 카테고리 경계 `---` 버튼 + 소속 아바타 — 그리고 New workspace 버튼은 full/collapsed 양쪽에서 숨는다. 섹션·헤더·레일 팝업의 동작과 시각은 [`features/workspace-category/`](../workspace-category/index.md) 가 정의한다 (여기엔 복제하지 않음 — 연결 개념).
- **하단**: 도구 / 플러그인 / 설정 버튼.

### 워크스페이스 조작

- **전환**: 카드 클릭 → 해당 Workspace 활성.
- **활성 카드 자동 스크롤**: 전환(클릭·quick-switch 단축키·카테고리 경계 이동 등, 전부 단일 관문
  `AppState::switch_workspace` 경유) 으로 활성 Workspace 가 바뀌었을 때 그 카드가 목록
  `ScrollArea` 뷰포트 밖이면 자동으로 스크롤해 보이게 한다. 활성 인덱스가 실제로 바뀐
  프레임에만 최소 이동으로 보정하므로(egui `scroll_to_rect`, align 없음) 사용자가 수동으로
  스크롤해 둔 상태를 매 프레임 덮어쓰지 않는다. collapsed 사이드바는 `ScrollArea` 자체가
  없어 해당 없음.
- **재정렬**: 카드 drag-and-drop (full). 카테고리 토글 on 이면 다른 카테고리 섹션에 드롭해 소속을 옮기는 경로가 추가된다 → [`features/workspace-category/`](../workspace-category/index.md).
- **생성**: New workspace 버튼(좌클릭) / 우클릭 → 프리셋으로 생성 — 카테고리 토글 off 일 때. on 이면 버튼이 숨고 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 생성한다 (경로 전체는 → [`features/workspace-category/`](../workspace-category/index.md)).
- **attach 인디케이터**: 다른 client(원격 사용자)가 그 Workspace 를 점유 중이면 status dot 둘레의 attached ring(`border_attached` — red 는 error 전용) ([점유 모델](../../concepts/actors.md)).
- **mirror 인디케이터**: 그 Workspace 가 원격 인스턴스의 로컬 mirror([remote-attach](../remote-attach/index.md))이면 — full 은 이름과 subtitle 사이 별도 줄에 하늘색 fill+border "REMOTE" pill(`>_→` glyph 포함), collapsed 레일은 아바타 우하단 하늘색 corner chip(변경 없음). 좌측 status dot 은 실행상태(running/idle) 전용이라 mirror 색을 싣지 않으며, notif(우상단)·attached(둘레 ring)와 시각 채널이 분리된다.

## 인터페이스

- **사용자**: 클릭(전환/버튼), 드래그(재정렬), 접기 토글.
- **워크스페이스 영역의 카테고리 조작은 다른 기능으로 위임** (토글 on 시 헤더 클릭/우클릭·레일 `---` 클릭·섹션 간 드래그): → [`features/workspace-category/`](../workspace-category/index.md)
- **하단 버튼은 다른 기능으로 위임** (연결 개념 — 사이드바는 진입점만, 내용은 각 문서):
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
- **시각 소스**: `site/vendor/ui_kits/terminal/chrome.jsx` — claude design

각 영역은 자기 위치/역할만 적고, 다른 기능으로 위임되는 버튼은 그 문서를 **링크만** 한다 (연결 개념).

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

collapsed 레일은 카테고리마다 경계 `---` 버튼 + 소속 아바타로 같은 그룹을 그리고, `---` 클릭 → 우측 앵커드 팝업. 헤더/`---`/팝업의 요소 구성·색·키캡 등 시각과 동작은 [`features/workspace-category/`](../workspace-category/index.md) 가 단일 출처다 (여기 복제하지 않음).

### UI 요소 인벤토리

- **헤더**: 워드마크 `tasty.` + 수박 로고(`icon_256.png`) + **접기 버튼**(full↔collapsed 토글).
- **워크스페이스 영역** (남는 높이): 토글 off — `Workspaces` heading → 워크스페이스 카드 목록 → `New workspace` 버튼. 토글 on — **카테고리 섹션**(헤더 + 소속 카드; 레일은 `---` 버튼 + 아바타)으로 그룹. → [`features/workspace-category/`](../workspace-category/index.md)
  - 카드: 클릭=전환, 드래그=재정렬(토글 on 이면 다른 섹션 드롭 = 카테고리 이동), 점유 중이면 인디케이터.
  - `New workspace`(+) 버튼은 **워크스페이스 카테고리 토글 off 일 때만** 표시 (full/collapsed 공통). on 이면 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 생성 (배경 우클릭은 새 카테고리·원격 워크스페이스 추가). → [`features/workspace-category/`](../workspace-category/index.md)
  - 카테고리 헤더(full) / `---` 경계 버튼(collapsed) — 토글 on 에만 존재. 접힘 토글·컨텍스트 메뉴·앵커드 팝업의 요소와 동작은 → [`features/workspace-category/`](../workspace-category/index.md)
- **하단 버튼** (각 버튼이 뭘 하는지 한 줄 + 상세는 해당 문서로 링크 — 연결 개념):
  - **도구** (`icons::TOOLS`) — 클릭 시 **도구 메뉴**를 연다 (리스닝 포트 등 빌트인 진단/유틸 항목 모음). → [`features/tools-menu/`](../tools-menu/index.md)
  - **플러그인** (`icons::PLUG`) — **플러그인 관리 창**을 연다 (설치/활성·비활성). → [`features/plugin-system/`](../plugin-system/index.md)
  - **설정** (`icons::SETTINGS`) — **설정 창**을 연다 (탭별 환경설정). → [`features/settings/`](../settings/index.md)

### 상태별 시각

- **full / collapsed**: collapsed 는 아이콘만 남긴 좁은 형태 (워드마크/라벨 숨김, 카드→정사각 슬롯; 카테고리 on 이면 `---` 경계 버튼 추가).
- **카테고리 on**: 워크스페이스 영역이 섹션 헤더(full)·`---` 경계 버튼(collapsed)으로 그룹되고 `New workspace` 버튼이 사라진다. 카테고리 조합 홀드 시 헤더/`---` 에 숫자 키캡. 헤더·레일·키캡의 시각과 갤러리 specimen 위치는 [`features/workspace-category/`](../workspace-category/index.md) 참조.
- **워크스페이스 점유**: 다른 client 가 점유 중인 카드에 인디케이터 표시.
- **드래그 중**: 카드 재정렬 드래그 스냅샷.

### 시각 소스

`site/vendor/ui_kits/terminal/chrome.jsx` — 사이드바 치수·색·로고·버튼 배치의 단일 출처. 스크린샷: `site/vendor/screens/sidebar-full.png`, `sidebar-collapsed.png`, `sidebar-workspaces.png`.
