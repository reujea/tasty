# 지금 할 수 없는 액션 (비활성 표시)

메뉴·툴바에서 대상의 상태만 바뀌면 할 수 있는 액션은 비활성으로 보이고, 대상의 종류상 원래 할 수 없는 액션은 숨긴다. 비활성 항목으로 사용자는 그 액션이 있다는 것과 지금 쓸 수 없다는 것을 함께 본다. 이유를 보일지와 그 형태는 디자인이 정한다.

종류 기준으로 숨기는 예다. 정확한 조건은 각 코드와 시험이 정본이다.

- 기본(예약) 카테고리의 이름 바꾸기·삭제: 그 카테고리는 영구히 바꿀 수 없다(`src/view/main/redraw.rs`의 `category_header_menu_items`와 시험 `category_header_menu_normal_is_additive_only`, 레일 팝업 `src/adapters/ui/popup/rail_category.rs`).
- Explorer 다중 선택의 이름 바꾸기·파일 관리자에서 보기: 한 항목에만 있는 액션이다(`build_explorer_context_menu`와 시험 `explorer_menu_multi`).
- 터미널 선택이 경로로 풀리지 않을 때의 파일 열기·폴더 열기: 라벨이 대상의 종류로 정해진다(`terminal_selection_items`).
- 열 대상이 없는 scheme(mailto 등) 링크의 열기 방법 선택: 그 scheme에는 고를 앱 목록이 없다(`src/view/main/link_menu.rs`의 시험 `menu_items_hide_open_with_when_no_target`).

## 모양

Tasty가 그리는 메뉴·컨트롤의 비활성 모양은 [테마 시스템](../systems/theme.md)의 "disabled 컨트롤"을 따른다(`MenuItem`은 박스 없이 disabled 잉크만 바꾼다). OS 네이티브 메뉴는 그 OS의 비활성 상태로 표시한다. 새 모양이 필요하면 디자인이 정한다.

## 현재 적용된 자리

- 도구 메뉴: 대상 surface도 생성 대기 초안도 없을 때 `surface` 범위 팝업을 여는 플러그인 항목([도구 메뉴](../../features/tools-menu/index.md)의 "항목 실행").
- 로컬 Explorer 빈 영역 메뉴: 이력이 없을 때의 Undo·Redo([탐색기](../../surfaces/explorer/index.md)의 "컨텍스트 메뉴 · 파일 조작").
- 원격 쓰기를 받지 않는 server의 mirror Explorer: 쓰기 항목과 툴바 create 묶음. 이유를 어떻게 알리는지는 OS마다 다르다([탐색기](../../surfaces/explorer/index.md)의 "받지 않는 server").
- 쓸 수 없는 로컬 폴더의 Explorer 툴바 create 묶음([탐색기](../../surfaces/explorer/index.md)의 툴바 명령 묶음 설명).
- Explorer 우클릭 메뉴의 붙여넣기(빈 영역의 붙여넣기, 폴더의 붙여넣기): 지금 붙여넣을 수 없으면 이유 없이 꺼진다. 경우의 정본은 [Explorer 파일 작업](../../surfaces/explorer/file-operations.md)의 "경로 출처와 작업 대상 (공통 계약)"과 "OS 파일 클립보드" 절이다.
- Explorer의 들어 있는 폴더에서 보기: 대상이 사라진 링크([탐색기](../../surfaces/explorer/index.md)의 "들어 있는 폴더에서 보기").
- 터미널 우클릭 메뉴의 복사·줄바꿈 없이 복사: 선택이 없을 때([터미널 링크](../../features/terminal-link/index.md)의 "노출 조건").
- 서피스·탭·페인을 이곳으로 이동: 대기 중인 대상이 없거나 종류가 다를 때, 탭·페인은 자기 자신이 대기 중일 때([위치 이동](../../features/surface-move/index.md)).

## 규칙과 다른 현재 동작

상태 조건인데 아직 숨기는 자리다. 비활성 형태(이유 표시 여부와 문구)를 디자인에 요청했고, 정해지면 이 목록에서 뺀다.

- mirror Explorer의 시스템에서 열기·파일 관리자에서 보기와 그 구분선, 새 탭으로 열기, 빈 영역 메뉴의 Undo·Redo 묶음([탐색기](../../surfaces/explorer/index.md)).
- 사이드바 workspace 메뉴의 강제 끊기: 이 Tasty가 그 workspace를 다른 client에 내보이는 중일 때만 붙는다.
