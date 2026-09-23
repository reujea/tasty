# 사이드바 화면

- **부모 기획**: [../index.md](../index.md)
- **상위 화면**: [MainView 전체 레이아웃](../../main-view/screens/main-view.md) 의 좌측 영역
- **시각 소스**: `site/vendor/ui_kits/terminal/chrome.jsx` — claude design

각 영역은 자기 위치/역할만 적고, 다른 기능으로 위임되는 버튼은 그 문서를 **링크만** 한다 (연결 개념).

## 레이아웃 (full)

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

### 카테고리 토글 on

워크스페이스 카테고리 토글이 on 이면 워크스페이스 영역만 아래처럼 바뀐다 (헤더·하단은 동일).

```
│ ▼ Workspaces     │  ← normal 카테고리 헤더 (클릭 접힘 토글 / 우클릭 메뉴)
│ ┌──────────────┐ │
│ │ workspace A  │ │  소속 카드 (다른 섹션으로 드래그 → 카테고리 이동)
│ └──────────────┘ │
│ ▶ work           │  ← 접힌 카테고리 (헤더만)
│        ⋮         │  ([+ New workspace] 버튼 없음 — 헤더 메뉴·레일 팝업으로 생성)
```

collapsed 레일은 카테고리마다 경계 `---` 버튼 + 소속 아바타로 같은 그룹을 그리고, `---` 클릭 → 우측 앵커드 팝업. 헤더/`---`/팝업의 요소 구성·색·키캡 등 시각과 동작은 [`features/workspace-category/`](../../workspace-category/index.md) 가 단일 출처다 (여기 복제하지 않음).

## UI 요소 인벤토리

- **헤더**: 워드마크 `tasty.` + 수박 로고(`icon_256.png`) + **접기 버튼**(full↔collapsed 토글).
- **워크스페이스 영역** (남는 높이): 토글 off — `Workspaces` heading → 워크스페이스 카드 목록 → `New workspace` 버튼. 토글 on — **카테고리 섹션**(헤더 + 소속 카드; 레일은 `---` 버튼 + 아바타)으로 그룹. → [`features/workspace-category/`](../../workspace-category/index.md)
  - 카드: 클릭=전환, 드래그=재정렬(토글 on 이면 다른 섹션 드롭 = 카테고리 이동), 점유 중이면 인디케이터.
  - `New workspace`(+) 버튼은 **워크스페이스 카테고리 토글 off 일 때만** 표시 (full/collapsed 공통). on 이면 카테고리 헤더 메뉴·레일 `---` 팝업의 Add workspace 로 생성 (배경 우클릭은 새 카테고리·원격 워크스페이스 추가). → [`features/workspace-category/`](../../workspace-category/index.md)
  - 카테고리 헤더(full) / `---` 경계 버튼(collapsed) — 토글 on 에만 존재. 접힘 토글·컨텍스트 메뉴·앵커드 팝업의 요소와 동작은 → [`features/workspace-category/`](../../workspace-category/index.md)
- **하단 버튼** (각 버튼이 뭘 하는지 한 줄 + 상세는 해당 문서로 링크 — 연결 개념):
  - **도구** (`icons::TOOLS`) — 클릭 시 **도구 메뉴**를 연다 (리스닝 포트 등 빌트인 진단/유틸 항목 모음). → [`features/tools-menu/`](../../tools-menu/index.md)
  - **플러그인** (`icons::PLUG`) — **플러그인 관리 창**을 연다 (설치/활성·비활성). → [`features/plugin-system/`](../../plugin-system/index.md)
  - **설정** (`icons::SETTINGS`) — **설정 창**을 연다 (탭별 환경설정). → [`features/settings/`](../../settings/index.md)

## 상태별 시각

- **full / collapsed**: collapsed 는 아이콘만 남긴 좁은 형태 (워드마크/라벨 숨김, 카드→정사각 슬롯; 카테고리 on 이면 `---` 경계 버튼 추가).
- **카테고리 on**: 워크스페이스 영역이 섹션 헤더(full)·`---` 경계 버튼(collapsed)으로 그룹되고 `New workspace` 버튼이 사라진다. 카테고리 조합 홀드 시 헤더/`---` 에 숫자 키캡. 헤더·레일·키캡의 시각과 갤러리 specimen 위치는 [`features/workspace-category/`](../../workspace-category/index.md) 참조.
- **워크스페이스 점유**: 다른 client 가 점유 중인 카드에 인디케이터 표시.
- **드래그 중**: 카드 재정렬 드래그 스냅샷.

## 시각 소스

`site/vendor/ui_kits/terminal/chrome.jsx` — 사이드바 치수·색·로고·버튼 배치의 단일 출처. 스크린샷: `site/vendor/screens/sidebar-full.png`, `sidebar-collapsed.png`, `sidebar-workspaces.png`.
