# 지금 할 수 없는 액션 (비활성 표시)

메뉴·툴바에서 대상의 상태만 바뀌면 할 수 있는 액션은 비활성으로 보이고, 대상의 종류상 원래 할 수 없는 액션은 숨긴다. 비활성 항목으로 사용자는 그 액션이 있다는 것과 지금 쓸 수 없다는 것을 함께 본다. 이유를 보일지와 그 형태는 디자인이 정한다.

종류 기준으로 숨기는 예다. 정확한 조건은 각 코드와 시험이 정본이다.

- 기본(예약) 카테고리의 이름 바꾸기·삭제: 그 카테고리는 영구히 바꿀 수 없다(`src/view/main/redraw.rs`의 `category_header_menu_items`와 시험 `category_header_menu_normal_is_additive_only`, 레일 팝업 `src/adapters/ui/popup/rail_category.rs`).
- Explorer 다중 선택의 이름 바꾸기·파일 관리자에서 보기: 한 항목에만 있는 액션이다(`build_explorer_context_menu`와 시험 `explorer_menu_multi`).
- 터미널 선택이 경로로 풀리지 않을 때의 파일 열기·폴더 열기: 라벨이 대상의 종류로 정해진다(`terminal_selection_items`).
- 원격(mirror) Explorer의 들어 있는 폴더에서 보기·열기: 이 컴퓨터에서 원격 경로를 풀지 않는다. 살아 있는 링크에도 없으므로 대상이 사라진 링크에도 없다(`src/view/main/explorer_show_in_folder.rs`의 `show_in_folder_row`).
- 열 대상이 없는 scheme(mailto 등) 링크의 열기 방법 선택: 그 scheme에는 고를 앱 목록이 없다(`src/view/main/link_menu.rs`의 시험 `menu_items_hide_open_with_when_no_target`).

## 모양

Tasty가 그리는 메뉴·컨트롤의 비활성 모양은 [테마 시스템](../systems/theme.md)의 "disabled 컨트롤"을 따른다(`MenuItem`은 박스 없이 disabled 잉크만 바꾼다). OS 네이티브 메뉴는 그 OS의 비활성 상태로 표시한다. 새 모양이 필요하면 디자인이 정한다.

## 현재 적용된 자리

메뉴별로 어떤 행이 비활성으로 남는지만 적는다. 켜고 끄는 조건과 이유 문구는 괄호 안의 함수와 그 시험이 정본이다.

- 도구 메뉴의 `surface` 범위 플러그인 항목([도구 메뉴](../../features/tools-menu/index.md)의 "항목 실행").
- Explorer 우클릭 메뉴의 쓰기 행(붙여넣기·잘라내기·새 항목·이름 바꾸기·휴지통), 로컬 전용 행(시스템에서 열기·파일 관리자에서 보기·새 탭으로 열기)과 붙여넣기 행(`build_explorer_context_menu`, 붙여넣기 출처는 `explorer_paste_row`).
- Explorer 즐겨찾기 행 메뉴의 새 탭으로 열기(`local_only_row`, `src/view/main/redraw.rs`).
- Explorer 빈 영역 메뉴의 Undo·Redo(`push_explorer_history_items`, `src/view/main/explorer_history.rs`).
- Explorer 툴바와 More 메뉴의 create 묶음([탐색기](../../surfaces/explorer/index.md)의 툴바 명령 묶음 설명).
- Explorer의 들어 있는 폴더에서 보기(`show_in_folder_row`, `src/view/main/explorer_show_in_folder.rs`).
- 터미널 우클릭 메뉴의 복사·줄바꿈 없이 복사(`terminal_selection_items`).
- 로컬 workspace 메뉴의 강제 끊기(`force_detach_items`). 저장된 원격 행·에이전트 mirror는 다른 client가 점유할 수 없어 종류 기준으로 숨긴다.
- 서피스·탭·페인을 이곳으로 이동(`push_surface_move_items`, `push_tab_move_items`, `src/view/main/move_menu.rs`).
