# Explorer 파일 작업 — 대상과 결과의 계약

- **Status**: Partial — 새 폴더·새 파일·이름 변경·복사·잘라내기·붙여넣기·휴지통 이동·드래그 놓기·충돌 선택·진행 취소·실행 취소·다시 실행은 구현돼 있다. `DISPLAY`가 없는 네이티브 Wayland의 메뉴와 조합 종료 키 전달, 밖에서 들어온 포인터의 파일 놓기에는 아래 Linux 한계가 남아 있다. 속성 조회·미리보기 패널·Grid 썸네일·찾기(거르기와 하위 폴더 검색)는 파일을 바꾸지 않는 읽기로 동작한다. 복사·잘라내기·붙여넣기는 OS 파일 클립보드와 이어진다(Linux 는 X11 선택, Wayland 세션은 XWayland 경유. GNOME Files 와 양방향 확인. Windows·macOS 는 실 기기 미측정). 원격(mirror) explorer 에서도 그 서버가 원격 쓰기를 알리면 새 폴더·새 파일·이름 변경·휴지통 이동·같은 연결 안의 붙여넣기·드래그 놓기·충돌 선택·진행 취소·결과 카드의 Retry·Undo 가 동작한다. 원격 작업의 실행 취소 이력과 이 컴퓨터와 원격 사이의 전송은 없다. 로컬 Explorer는 Windows OLE·macOS AppKit·Linux X11/Wayland에서 외부 앱으로 Copy 드래그를 시작한다. Windows·macOS는 크로스 타깃 컴파일 확인, 실 기기 동작 미측정이다.
- **주체**: 로컬 사용자 ([주체](../../concepts/actors.md)). 에이전트(IPC/CLI)는 이 문서의 파일 작업을 호출하지 않는다.
- **설계**: [원격 attach (Remote attach)](../../features/remote-attach/index.md) (mirror explorer 의 파일 작업은 원격 server 가 실행한다), 이 문서 (OS 파일 클립보드 공유와 나중 복사 우선)
- **코드**: 메뉴 구성 `build_explorer_context_menu`·메뉴 핸들러 `explorer_menu_*` (`src/view/main/redraw.rs`), 단축키·Command Palette 진입 `handle_explorer_shortcut`·`run_explorer_action` (`src/adapters/ui/input/shortcuts/copy_paste.rs`), 실행 worker `src/app/explorer_files.rs`·`src/app/file_jobs/ops.rs`·작업 엔진 `src/app/file_jobs/job.rs`·화면 동기화 `src/app/explorer_files/ui_sync.rs`·원격 요청 `src/app/explorer_files/remote.rs`·원격 server 실행 `src/app/file_jobs/remote.rs`, 새 항목 입력 `src/adapters/ui/surface/explorer/create.rs`·툴바 More 메뉴 `src/view/main/explorer_create.rs`, 찾기 `src/adapters/ui/surface/explorer/find.rs`, 칸 표시 `src/adapters/ui/surface/explorer/view/ops.rs`·드래그 `src/adapters/ui/surface/explorer/view/drag.rs`·충돌 popup `src/adapters/ui/popup/explorer_conflict.rs`, OS 파일 드롭 `src/view/main/file_drop.rs`, OS 파일 클립보드 `src/clipboard/files.rs`·창 목록과의 조정 `src/state/explorer_os_clipboard.rs`
- **화면**: [Explorer](index.md) 의 컨텍스트 메뉴·rename 팝업·토스트, 상태줄 진행 표시·대기열 팝오버·충돌 popup·결과 카드·드래그 칩 (갤러리 `explorer-ops-*` 예제)

## 확인된 Linux 범위와 한계

`DISPLAY`가 있으면 Tasty는 X11로 뜬다. Wayland 세션의 기본 경로는 XWayland이고 네이티브 Wayland는 `DISPLAY`가 없는 환경의 경로다([백엔드 선택](../../design/systems/linux-display-backend.md)).

- 격리 X11과 rootful XWayland에서 네이티브 항목·빈 영역 메뉴, Undo 사유 툴팁·단축키, OS 파일 클립보드 왕복과 잘라내기 한 번 소비, 내부·외부 DnD, 링크 휴지통의 대상 보존, 복사 metadata, 작업 중 마지막 창 닫기와 앱 Quit를 확인했다. 작업 중 추가 창만 닫고 원래 창에서 복사를 계속하는 경로도 두 환경에서 확인했다.
- 일반 이름 변경은 네이티브 Wayland에서 Enter 확정·Esc 취소, rootful XWayland와 X11에서 Enter·Save 확정·Esc 취소를 확인했다. 네이티브 Wayland의 Save도 실제 포인터 입력으로 확인했다. rootful XWayland의 실제 한글 조합에서는 조합을 끝내는 Enter가 팝업을 열어 둔 채 글자만 확정하고, 다음 별도 Enter·Save로 저장하거나 Esc로 취소하는 것을 확인했다.
- X11과 rootful XWayland의 실제 IBus 한글 조합에서 목록 Down·Delete·Enter·Esc와 Find Down·Esc, 팝업의 Delete 우선 처리를 확인했다. 조합 종료 누름과 그다음 별도 누름을 구분한 범위이며, 다른 OS·입력기의 확인을 대신하지 않는다.
- 네이티브 Wayland에서는 실제 포인터 전달, 새 폴더 Enter, 내부 DnD, 내부 클립보드의 copy/cut·Undo/Redo와 metadata, 링크 휴지통의 대상 보존, 일반 외부 변경 표시·Find·Properties, GTK 앱으로 내보내는 COPY를 확인했다. Properties는 별도로 지정한 단축키로 열었다. 내부 클립보드와 외부 COPY는 OS 파일 클립보드 왕복이나 외부 파일 가져오기의 확인이 아니다.
- 네이티브 Wayland의 항목·빈 영역 컨텍스트 메뉴와 툴바 More 메뉴는 미지원이고 화면 안내는 없다. 포인터는 앱에 전달되지만 `crates/tasty-platform/src/native_menu/linux.rs`의 `show_context_menu`가 Xlib 윈도우만 받으므로, 메뉴 행·Undo 사유·단축키 열의 확인은 이 환경에서 완료되지 않았다.
- 네이티브 Wayland에서는 실제 한글 조합을 끝내는 Down·Delete·Enter·Esc가 같은 누름의 목록 동작으로 이어지고, Find의 Down·Esc도 항목 선택·검색어 지우기로 이어지는 결함이 남아 있다. X11의 물리 키 소유권 처리는 이 Wayland 경로에 적용되지 않는다. 현재 text-input·keyboard 이벤트에는 조합 확정과 전달된 물리 누름을 연결하는 원인 식별자가 없어, 같은 이벤트 묶음이나 시간 간격만으로 다음 정상 키와 안전하게 구분할 수 없다.
- GNOME 통합 rootless XWayland의 격리 세션([측정 방법](../../ai-verification/gnome-session-verification.md))에서 우클릭 메뉴와 항목 선택, GNOME Files와 OS 파일 클립보드 왕복, GNOME Files와 GTK 앱으로 끌어내기, 내부 끌기, 실제 IBus 한글 조합의 목록 Down·Delete와 Find Down·Esc를 확인했다. 툴바 More 메뉴와 조합 종료 Enter의 목록 동작은 이 환경에서 재지 않았다. 실제 사용자 GNOME 세션과 물리 화면의 확인을 대신하지 않는다.
- 밖에서 끌어온 파일은 창 위 포인터 좌표를 winit이 아직 받지 못했으면 `file drop with no cursor position` 경고와 함께 버려진다. 파일 관리자에서 포인터가 들어오는 일반 경로가 여기에 걸린다.
- 일반 로컬 외부 변경 표시와 Properties의 OS metadata 대조는 확인했다. 느린 매체의 외부 변경 지연과 네이티브 Wayland 대용량 작업 중 앱 종료는 미측정이다. 네이티브 Wayland에서 X 서버 없이 OS 파일 클립보드·외부 파일 가져오기를 지원하지 않는 현재 계약은 아래 절과 같다.

### Windows·macOS 확인 범위

Windows에서 파일 식별 유틸리티·IPC publication과 Explorer 선택·HTML root의 선택된 네이티브 시험을 확인했다. macOS에서도 선택된 Explorer 비입력 시험을 확인했다. 이 시험들은 실제 데스크톱 메뉴·DnD·클립보드·링크 휴지통·앱 종료·입력기 조합 경로를 확인하지 않는다. Windows 실제 한글 IME와 macOS 사용자 수동 입력을 포함한 GUI 확인은 남아 있다. Linux 결과와 과거 OS별 시험 결과를 현재 전체 OS acceptance 완료로 확대하지 않는다.

시험 빌드는 실제 OS 휴지통과 macOS Finder를 부르지 않는다. 휴지통 호출은 `src/app/file_jobs/os_trash.rs` 한 곳에서만 하고(`trash_has_one_door` 검사), 시험 빌드에서는 항목을 지우는 대역으로 바뀐다. 대역은 임시 폴더 밖 항목을 거부한다. 실패 전달, 영구 삭제 없음, 항목 사이 취소는 대역으로 모든 OS에서 잰다. 실제 구현의 성공과 실패 전달은 Linux의 격리한 자식 프로세스 시험만 잰다(`a_real_trash_*`). macOS Finder와 Windows Recycle Bin의 실제 동작을 재는 자동 시험은 없다. Finder가 응답하지 않을 때의 작업 대기, 취소 반응, 카드 표시는 미측정이다.

## 목적

탐색기의 파일 작업은 진입점이 여럿이다. 우클릭 메뉴, 키보드 단축키, Command Palette 가 같은 작업을 부른다. 이 문서는 진입점마다 대상과 결과가 같아야 한다는 규칙과, 작업마다 대상·결과·사용자 피드백·상태를 정한다. 지원하지 않는 작업은 그 사실을 적는다. 화면 요소와 탐색 동작은 [Explorer](index.md) 에 있다.

## 내부 동작 (headless-valid)

### 경로 세 가지

| 이름 | 모델 | 쓰는 곳 |
|---|---|---|
| 고정 루트 (cwd) | `ExplorerTab::cwd()` | 사이드바 Files 트리의 뿌리, 스폰 cwd, 탭 이름. 탐색으로 바뀌지 않고 "이 폴더로 루트 설정"만 바꾼다 |
| 현재 폴더 (current) | `ExplorerTab::current()` (필드 `root`) | 오른쪽 목록과 주소창. 뒤로·앞으로·상위·주소창 이동이 바꾼다 |
| 선택 항목 | `ExplorerView::selected` | 현재 폴더 목록 안에서 클릭·Ctrl/Cmd+클릭·Shift+클릭(범위)·Ctrl/Cmd+Shift+클릭(범위 추가)·전체 선택으로 고른 항목. 폴더가 바뀌면 비운다. Find 바가 거르는 동안에는 보이는 항목만 고를 수 있고, 하위 폴더 검색 결과를 고르면 그 결과의 실제 경로가 선택이 된다 |

파일 작업은 고정 루트를 대상으로 쓰지 않는다. 대상은 선택 항목이거나 현재 폴더다.

### 대상 결정 규칙

| 진입 | 대상 |
|---|---|
| 우클릭 — 선택 안의 항목 | 선택 전체 |
| 우클릭 — 선택 밖의 항목 | 그 항목 하나. 선택도 그 항목으로 바꾼다 |
| 우클릭 — 목록 빈 영역과 툴바·주소창·내부 탭바·상태줄 | 현재 폴더 |
| 우클릭 — 사이드바 Files 트리의 폴더 | 그 폴더 하나. 선택은 바꾸지 않는다 |
| 우클릭 — Favorites 행 | 그 즐겨찾기 경로 |
| `explorer_properties` 단축키·Command Palette | 선택 항목(보이는 목록 순서, 하위 폴더 검색 결과 포함). 선택이 없으면 현재 폴더 |
| 단축키·Command Palette 의 복사·잘라내기·경로 복사 | 선택 항목. 선택이 없으면 아무것도 하지 않는다 |
| 단축키·Command Palette 의 붙여넣기 | 현재 폴더 |
| 메뉴 "Paste into" (단일 폴더) | 그 폴더 |
| 드래그 — 끄는 항목 | 선택 안의 항목을 끌면 선택 전체, 선택 밖의 항목을 끌면 그 항목 하나(선택도 그 항목으로 바꾼다) |
| 드래그 — 놓는 곳 | 폴더 행·셀, 사이드바 트리 노드, 즐겨찾기 행이면 그 폴더. 파일 행이나 빈 곳이면 그 칸이 보고 있는 폴더 |
| OS 파일 드롭 | 놓은 explorer 칸이 보고 있는 폴더(위치를 아는 동안은 위 드래그와 같은 대상) |

단축키와 Command Palette 는 같은 함수(`run_explorer_action`)를 거치고, 복사·잘라내기·붙여넣기는 메뉴와 같은 핸들러(`explorer_menu_set_clipboard`·`explorer_menu_paste`)를 부른다. 같은 대상이면 진입점이 달라도 결과가 같다. 대상 surface 는 포커스된 explorer surface 이고, 메뉴는 우클릭한 surface 의 id 를 지닌다.

### 경로 출처와 작업 대상 (공통 계약)

탐색기의 파일 작업(새 폴더·새 파일·복사·잘라내기·붙여넣기·드래그 놓기·OS 파일 드롭·휴지통·이름 변경·실행 취소·시스템 열기)과 파일을 읽기만 하는 속성 조회는 아래 규칙을 함께 따른다.

- **경로는 출처와 함께 다룬다.** explorer 경로는 로컬 경로이거나 mirror workspace 에 보이는 원격 호스트의 경로다(`ExplorerPathSource`). 같은 문자열이라도 출처가 다르면 다른 파일이다. 경로를 보관해 나중에 쓰는 자료(클립보드 등)는 출처를 함께 기록하고, 출처가 원격이거나 알 수 없는 경로를 로컬 파일 작업의 입력으로 해석하지 않는다.
- **대상은 요청할 때 고정한다.** 메뉴·단축키·팝업이 대상 경로와 surface 를 정한 순간의 값을 쓴다. 그 사이 포커스·explorer 내부 탭·선택이 바뀌어도 대상은 바뀌지 않는다. 사용자가 그 사이 다른 폴더로 가도 대상은 바뀌지 않는다.
- **실행 직전에 surface 세대를 다시 확인한다.** 메뉴는 열 때 그 surface 의 세대(`SurfaceBinding`)를 고정하고, 고른 항목은 `explorer_menu_admits`(`src/state/explorer_menu.rs`)가 허용할 때만 실행한다. 파일을 바꾸는 항목은 세대 전체(surface activation·자원·mirror projection)가 그대로여야 한다. 파일을 바꾸지 않는 항목(경로 복사·복사·이 폴더로 루트 설정)은 같은 explorer surface 이기만 하면 되고, mirror projection 이 다시 만들어진 것은 따지지 않는다. 이름 변경은 메뉴의 세대를 rename 팝업에 그대로 실어, 팝업이 떠 있는 사이 surface 가 바뀌면 팝업을 닫고 확정해도 실행하지 않는다.
- **worker 도 다시 확인한다.** View 는 대상 경로와 원 surface·View identity 를 고정해 App 의 `explorer_files` worker 에 넘긴다. worker 는 시작 직전에 원 대상과 mirror 제한을 다시 확인하고, 완료 뒤 원 View 와 surface 가 남아 있을 때만 목록 갱신을 요청한다. 사용자가 그 사이 바꾼 선택이나 새로 담은 클립보드는 건드리지 않는다. 대기 요청 상한과 종료 처리는 [Explorer](index.md#컨텍스트-메뉴--파일-조작) 에 있다.
- **원격 쓰기는 그 서버가 받을 때만 한다.** mirror explorer 의 쓰기는 연결의 server 가 attach descriptor 에 원격 쓰기 capability 를 알렸고 연결이 살아 있을 때만 허용한다(`EngineRead::blocks_explorer_write`). 받지 않는 server(옛 버전)의 mirror explorer 에서는 메뉴의 쓰기 항목과 툴바 create 묶음을 꺼 두고 툴팁에 이유를 달며, 단축키와 메뉴 핸들러는 같은 이유를 그 칸의 info toast 로 알리고 거부한다. worker 는 시작하지 않는다. 이유 문구는 [탐색기 surface](index.md) 의 "받지 않는 server" 에 있다.
- **경로는 그 컴퓨터 안에서만 쓴다.** 클립보드의 경로는 같은 출처의 칸에만 붙여넣는다. 원격에서 복사한 경로를 로컬 explorer 나 다른 연결에 붙여넣으면 `explorer.state.remote_paste_unsupported`, 로컬에서 복사한 경로를 mirror explorer 에 붙여넣으면 `explorer.state.cross_computer_paste_unsupported` 로 거부하고, 메뉴에도 붙여넣기가 나오지 않는다. 같은 문자열의 다른 컴퓨터 파일을 대신 쓰지 않는다. OS 에서 끌어 온 파일을 mirror explorer 칸에 놓으면 거절한다. explorer 가 아닌 칸에 놓은 파일은 로컬 파일 열기(`DispatchFile`)로 처리한다.
- **주소 입력과 링크**는 [Explorer](index.md) 의 [주소 입력](index.md#주소-입력)·[심볼릭 링크](index.md#심볼릭-링크) 절을 따른다. 로컬 경로는 절대 경로로 확정해 다루고, 원격 경로는 로컬 파일시스템으로 확인하지 않는다.

### 작업별 계약

| 작업 | 진입점 | 대상 | 결과 | 사용자 피드백 |
|---|---|---|---|---|
| 새 폴더 | 툴바(좁은 칸은 More 메뉴), 메뉴(빈 영역·단일 폴더), `explorer_new_folder` 단축키(기본 `F7`) | 지금 보는 폴더(검색 결과 중에도 검색 시작 폴더), 폴더 메뉴는 명시한 그 폴더. 시작할 때 고정한다 | 목록 맨 위(폴더 메뉴면 그 폴더 바로 다음, [Explorer](index.md#새-폴더--새-파일)) 인라인 입력으로 이름을 받아 `create_dir` 로 만든다. 같은 이름이 있으면 덮어쓰지 않고 실패한다 | 입력 아래 이름 오류 상자. 쓰기 실패는 오류 토스트와 다시 읽기. 성공하면 그 폴더를 아직 보고 있을 때 새 항목을 선택한다 |
| 새 파일 | 툴바(좁은 칸은 More 메뉴), 메뉴(빈 영역·단일 폴더), `explorer_new_file` 단축키(기본 `Shift+F4`) | 새 폴더와 같다 | 같은 입력으로 빈 파일을 `create_new` 로 만든다 | 새 폴더와 같다 |
| 이름 변경 | 메뉴 (단일 항목), `explorer_rename` 단축키, Command Palette | 그 항목. 단축키·Command Palette 는 선택이 하나일 때만 연다 | 같은 폴더 안에서 이름만 바꾼다. 새 항목 이름과 같은 검사([새 폴더 · 새 파일](index.md#새-폴더--새-파일))로 빈 이름·금지 글자·앞뒤 공백·예약 이름을 거부하고, 이미 있는 이름도 거부한다 | rename 팝업. 실패하면 오류 토스트를 띄우고 선택을 유지한 채 목록을 다시 읽는다 |
| 복사 | 메뉴, `copy` 단축키, Command Palette | 선택 항목 | 앱의 모든 창이 함께 쓰는 explorer 파일 클립보드에 경로를 담고, 로컬 경로면 OS 파일 클립보드에도 올린다(아래 [OS 파일 클립보드](#os-파일-클립보드)). 디스크는 바꾸지 않는다 | 없음 |
| 잘라내기 | 메뉴, `cut` 단축키, Command Palette | 선택 항목 | 클립보드에 잘라내기 표시와 함께 담는다. OS 파일 클립보드에는 잘라내기 표지와 함께 올린다 | 붙여넣기 전까지 잘라낸 항목을 모든 창의 grid·list·detail 에서 `cut-pending-fg` 잉크와 이탤릭 이름으로 그린다. 불투명도는 쓰지 않는다 |
| 붙여넣기 | 메뉴 (빈 영역·"Paste into"), `paste` 단축키, Command Palette | 현재 폴더(검색 중에는 검색 시작 폴더) 또는 명시한 Paste into 폴더 | Tasty 목록과 OS 파일 클립보드 중 나중에 복사한 쪽을 쓴다. 아래 [작업 실행](#작업-실행--진행충돌결과) 의 작업으로 돈다. 복사는 목적지 임시 디렉터리에서 준비한 뒤 덮어쓰기 금지 rename 으로 공개한다. 이동은 rename 하고, 다른 파일시스템이면 복사 뒤 원본을 지운다 | 상태줄 진행 표시, 이름 충돌 popup, 결과 카드 |
| 드래그 놓기 (explorer 항목) | 항목을 끌어 폴더·트리 노드·즐겨찾기·목록 본문에 놓기. 다른 explorer 칸에도 놓을 수 있다 | 위 대상 규칙 | 같은 디스크면 이동, 다른 디스크면 복사. 드래그 반전 modifier 를 누르고 있으면 뒤집는다. 붙여넣기와 같은 작업으로 돈다. 클립보드는 쓰지도 비우지도 않는다 | 포인터 옆 칩(아래 [드래그](#드래그-앤-드롭)), 대상의 링, 그 뒤는 붙여넣기와 같다 |
| 외부 앱으로 끌어 내기 | 로컬 선택을 누른 채 원본 윈도우 밖으로 끌기 | 시작 때 고정한 로컬 경로와 SurfaceBinding | Windows OLE·macOS AppKit·Linux X11/Wayland에서 Copy만 제공. 원본 삭제·Undo 기록 없음 | OS 파일 아이콘·커서. 시작/통신 실패는 안내, 취소·불명 완료는 성공으로 표시하지 않음. Windows·macOS 실 기기 미측정 |
| 드래그 놓기 (OS 파일) | OS 에서 끌어 온 파일을 explorer 칸에 놓기 | 놓은 칸의 폴더 | 항상 복사한다. mirror explorer 는 거절한다. explorer 가 아닌 칸에 놓으면 [파일 핸들러](../../features/file-handler/index.md)로 연다 | 붙여넣기와 같다 |
| 휴지통 이동 | 메뉴, `explorer_trash` 단축키, Command Palette | 선택 항목 또는 우클릭 항목. 단축키·Command Palette 는 선택이 없으면 아무것도 하지 않는다 | OS 휴지통으로 보낸다. 확인 모달은 없다. 영구 삭제 경로는 없다. 그 드라이브에 휴지통이 없으면 아무것도 지우지 않는다 | 결과 카드. 휴지통이 없으면 "Trash isn't available on this drive. Nothing was deleted." 카드 |
| 실행 취소 | 끝난 복사·이동 결과 카드의 Undo, 빈 영역 메뉴의 Undo 행, `explorer_undo` 단축키(아래 [되돌리기·다시 실행 이력](#되돌리기다시-실행-이력)) | 그 작업이 만들거나 옮기거나 이름을 바꾼 항목 | 이동은 원래 자리로 되돌린다(원래 자리가 비어 있고, 옮겨 간 항목의 수정 시각이 옮긴 직후 그대로일 때만. 바뀌었으면 다른 항목일 수 있어 그 자리에 두고 "옮긴 뒤 바뀌어서 그 자리에 둠"(`changed_after_move`)으로 알린다. 폴더는 폴더 자신의 수정 시각만 본다). 복사로 만든 항목은 휴지통으로 보낸다. 다만 작업 뒤 수정 시각이 바뀐 사본은 사용자가 고친 것으로 보고 남긴다(폴더는 폴더 자신의 수정 시각만 본다. 수정 시각 해상도가 거친 파일시스템(FAT/exFAT 2초, HFS+ 1초)에서는 공개 직후 같은 시각 칸 안의 수정을 구분하지 못한다. 그 사본은 휴지통으로 가며 되살릴 수 있다). 이름 변경은 원래 이름으로 되돌린다(원래 이름이 비어 있고 항목의 수정 시각이 바꾼 직후 그대로일 때만. 아니면 그대로 두고 "지금 원래 이름으로 다른 항목이 있습니다"(`name_taken`)나 "이름을 바꾼 뒤 바뀌어서 그대로 둠"(`changed_after_rename`)으로 알린다). 대소문자만 바꾼 이름 변경은 대소문자를 무시하는 파일시스템에서 원래 이름이 바뀐 항목 자신을 가리키므로, 원래 이름 자리의 항목이 이름이 대소문자만 다르고 같은 파일(Unix dev·inode, Windows 볼륨·파일 번호)이면 비어 있는 것으로 본다(`named::taken`). 새 폴더·새 파일은 휴지통으로 보낸다(영구 삭제하지 않는다). 아직 비어 있고(파일 길이 0, 빈 폴더) 수정 시각이 만든 직후 적은 값 그대로일 때만 보내고, 아니면 남기고 "만든 뒤 바뀌어서 남겨 둠"(`changed_after_create`)으로 알린다. 빈 항목 조건은 만들기와 시각 기록 사이에 다른 프로그램이 쓴 경우와 수정 시각 단위가 거친 파일시스템에서 같은 시각 칸 안에 쓴 경우를 막는다(`named::made_blocked`). 판정과 휴지통 이동 사이에 들어온 변경은 막지 못하지만 휴지통에서 되살릴 수 있다(복사로 만든 항목의 되돌리기와 같다). Replace 로 덮어쓴 파일은 백업하지 않으므로 되돌리지 않는다. 휴지통 이동의 복원은 OS 에서 한다 | 결과 카드. 되돌리지 못한 항목과 이유를 나열한다. 이름 변경·새 항목을 되돌린 카드는 수 없이 "Undid rename"·"Couldn't undo new folder" 처럼 결과만 제목에 적는다(`explorer.result.undo_done_*`·`undo_failed_*`) |
| 다시 실행 | 빈 영역 메뉴의 Redo 행, `explorer_redo` 단축키 | 되돌린 작업의 원래 대상 | 되돌린 작업의 원래 요청을 새 작업으로 다시 보낸다. 복사·이동은 원본 경로·목적지·이동 여부(일부만 됐던 작업은 된 항목만)이고 이름이 겹치면 처음처럼 충돌을 묻는다. 이름 변경은 원래 경로와 새 이름, 새 항목은 폴더·이름·종류이며 같은 이름이 있으면 덮어쓰지 않고 실패한다. 되돌린 뒤 대상 자리가 바뀌었으면 보내지 않는다(아래 이력 절) | 복사·이동은 붙여넣기와 같다. 이름 변경·새 항목은 결과 카드 없이 목록에 반영하고, 실패하면 오류 토스트 |
| 찾기 | 툴바 Find 토글(좁은 칸은 More 메뉴), `find` 단축키 | 지금 보는 폴더, Subfolders 를 켜면 그 아래 전체(로컬만) | 디스크를 바꾸지 않는다. 목록을 이름으로 거르거나 하위 폴더 검색 결과로 바꾼다. 결과에 대한 파일 작업은 결과의 실제 경로를 대상으로 한다 | Find 바의 개수·검색 상태, 상태줄 ([Explorer](index.md#찾기--하위-폴더-검색)) |
| 속성 | 메뉴 맨 끝 "Properties"(모든 변형, mirror 포함), `explorer_properties` 단축키, Command Palette | 메뉴의 대상, 또는 위 단축키 규칙 | 디스크를 바꾸지 않는다. 열 때의 대상을 고정해 보이고, 폴더 크기는 read worker 가 배경에서 센다. 닫으면 세기를 멈춘다 | 탐색기 칸에 묶인 Properties popup ([Explorer](index.md#properties-popup)) |
| 미리보기 | 툴바 토글, `explorer_toggle_preview` 단축키, Command Palette | 선택이 하나일 때 그 항목 | 디스크를 바꾸지 않는다. 로컬 파일만 read worker 로 읽는다 | 목록 오른쪽 미리보기 패널 ([Explorer](index.md#미리보기-패널)) |
| 숨김 파일 | 툴바 토글(좁은 칸은 More 메뉴), `explorer_toggle_hidden` 단축키, Command Palette | 지금 보는 폴더에서 이름이 `.` 으로 시작하는 항목과 OS 가 숨김으로 표시한 항목(Windows 숨김 속성·macOS `UF_HIDDEN`). Linux 는 점 규칙만 쓴다. 원격 탐색기는 원격 server 가 실어 보낸 표시를 쓰고, 싣지 않는 옛 server 면 점 규칙만 쓴다 | 디스크를 바꾸지 않는다. 목록에서 빼거나 되돌린다 | 상태줄의 숨김 수, 켜면 숨김 항목을 explorer-hidden-fg 로 ([Explorer](index.md#숨김-파일)) |
| Grid 썸네일 | 없음 (Grid 보기에 자동) | 화면에 보인 로컬 그림 파일 | 디스크를 바꾸지 않는다 | 셀의 40 슬롯 ([Explorer](index.md#grid-썸네일)) |

이름 변경과 휴지통 이동의 단축키는 `explorer_rename`(기본 `F2`)·`explorer_trash`(Tasty 프리셋 기본 `delete`·`alt+backspace`. `alt+backspace` 는 위치 규칙상 macOS 에서 Finder 의 ⌘⌫ 다. macOS 프리셋은 `alt+backspace` 만, Windows·Linux 프리셋은 `delete` 만)다. 단축키와 Command Palette 는 `run_explorer_item_key`(`src/adapters/ui/input/shortcuts/explorer_keys.rs`)를 거쳐 메뉴와 같은 이름 변경 팝업을 열거나 같은 휴지통 작업을 요청한다. 우클릭 메뉴는 이 두 항목과 새 폴더(`explorer_new_folder`)·새 파일(`explorer_new_file`) 행 옆에 각 바인딩의 첫 항목을 보인다(`attach_shortcut_hints`, `src/state/explorer_menu.rs`). 바인딩이 있는 메뉴 행은 단축키를 표시한다는 디자인 결정을 따른다. macOS 는 key equivalent 로 바꿔 AppKit 이 그리고, Windows·Linux 는 설정 화면과 같은 표시에서 앞 지우기 키만 `Del` 로 줄여 항목 오른쪽에 쓴다. 바인딩이 비었으면 표시하지 않는다. 메뉴의 표시는 보이기만 하며 실행은 단축키 경로가 맡는다.

### 기본 규칙

파일을 바꾸는 작업의 기본 동작이다(사용자 확정 2026-10-10). 각 규칙의 세부는 연결한 절에 있다.

| 규칙 | 현재 동작 | 구현 |
|---|---|---|
| 지우기는 휴지통으로 | 휴지통 이동만 있고 영구 삭제 작업은 없다. 휴지통이 항목을 받지 않아도 영구 삭제로 대신하지 않는다. 그 드라이브에 휴지통이 없으면 아무것도 지우지 않고 그 사실을 카드로 알린다 | `job::run_trash`, `OpKind`(`Copy`·`Move`·`Trash`·`Undo`) |
| 이름 충돌의 선택지 | Keep both(기본)·Skip·Replace(파일끼리만)·Cancel the rest 중에서 고르고, "Do this for the other n conflicts" 로 남은 충돌에 같은 답을 쓴다. 폴더는 Replace 하지 않고 다시 묻는다 ([작업 실행](#작업-실행--진행충돌결과)) | `job::Choice`, 충돌 popup `src/adapters/ui/popup/explorer_conflict.rs` |
| 끌어 놓기의 기본 동작 | 같은 파일시스템이면 이동, 다른 파일시스템이면 복사다. 드래그 반전 modifier 로 뒤집는다. OS 에서 끌어 온 파일은 항상 복사한다. 파일시스템이 같은지는 unix 는 장치 번호, Windows 는 링크·마운트 폴더·UNC 를 따라간 실제 볼륨의 일련번호로 보고, 읽지 못하면 다른 파일시스템(복사)으로 본다. Windows 판정은 실 기기에서 측정하지 않았다 ([드래그 앤 드롭](#드래그-앤-드롭)) | `view/drag.rs` 의 `same_volume` |
| 취소는 남은 항목만 | 지금 항목을 멈추고 남은 항목을 하지 않는다. 끝난 항목은 되돌리지 않고, 멈춘 항목의 준비 중 사본은 지운다 ([상태별 결과](#상태별-결과)) | `job.rs` 의 `record`·`Outcome::Cancelled` |
| 다시 시도는 실패·미처리만 | 결과 카드의 Retry 는 실패하거나 건너뛴 항목과, 취소로 하지 않은 항목(멈춘 항목과 그 뒤 항목)만 같은 작업으로 다시 요청한다. 끝난 항목은 다시 하지 않는다. 취소한 작업의 카드도 같다 | `Report::retryable`·`Report::unprocessed`, `view/ops.rs` 의 `card_text` |
| 볼륨 간 이동의 원본 삭제 실패는 별도 단계 | 사본을 공개한 뒤 원본을 다 지우지 못한 항목은 실패로 따로 보이고, Retry 는 다시 옮기지 않고 남은 원본 삭제만 별도 작업으로 요청한다 ([남은 원본 다시 지우기](#작업-실행--진행충돌결과)). 이동을 취소한 카드도 같다 | `Operation::RemoveLeftovers`, `job::leftover` |
| 실행 취소는 자기 작업만 | Undo 는 그 작업이 기록한 단계(만든 사본·옮긴 항목·바꾼 이름·만든 새 항목)만 되돌린다. 다른 작업이나 그 뒤 바뀐 항목은 건드리지 않는다 | `UndoStep`, `job::run_undo` |
| 새 항목은 같은 화면일 때만 선택 | 새 폴더·새 파일은 만들기가 끝났을 때 요청한 칸이 그 폴더를 아직 보고 있을 때만 새 항목을 선택하고 보이게 한다. 붙여넣기·끌어 놓기의 결과는 선택하지 않는다 | `ExplorerView::reveal_created` |

### 상태별 결과

| 상태 | 결과 |
|---|---|
| 정상 | 아래 "작업 뒤 목록 갱신" 범위의 목록을 다시 읽는다. 잘라내기를 붙여넣어 모두 성공했으면 그 클립보드를 비운다 |
| 빈 폴더 | 빈 영역 우클릭은 현재 폴더를 대상으로 한다. 붙여넣을 로컬 파일(Tasty 목록이나 OS 파일 클립보드)이 있으면 붙여넣기를 보인다 |
| 오류 | 오류 토스트. 실패한 이름 변경·휴지통 이동은 선택을 유지한다. 권한 거부 폴더에서는 빈 영역 메뉴를 띄우지 않는다 |
| 받지 못한 요청 | 대기 요청이 8개이거나 한 요청의 경로 자료가 1MiB 를 넘으면 작업을 시작하지 않는다. 그 칸의 결과 카드 자리에 경고 카드(`explorer.request.queue_full`·`explorer.request.too_large`, 문구에 수 없음)를 보이고 표준 시간 뒤 사라진다. 같은 이유가 다시 오면 카드를 더 쌓지 않고 시간만 늘린다. 대기열이 찼고 이 칸의 작업이 실행 중이면 카드에 대기열 보기(`explorer.request.show_queue`)가 붙어 상태줄 위 대기열 팝오버를 연다. 칸을 찾지 못한 요청은 같은 문구의 경고 토스트로 알린다. 클립보드와 선택은 그대로다 |
| 부분 성공 | 결과 카드가 실패·건너뛴 경로를 보이고 닫을 때까지 남는다. 잘라내기 클립보드는 남긴다. 성공한 항목을 되돌리지 않는다 |
| 이름 충돌 | 항목마다 충돌 popup 으로 묻는다. 기본은 Keep both(`(copy)` 접미사 새 이름)이고 기존 항목을 덮어쓰지 않는다. 같은 이름의 폴더는 합치지 않는다 |
| 취소 | 상태줄의 취소(×)는 지금 항목을 멈추고 남은 항목을 하지 않는다. 끝난 항목은 그대로 두고, 멈춘 항목의 준비 중 사본은 지운다. 대기열의 작업은 팝오버에서 뺄 수 있다. 앱 종료는 새 작업을 막고 실행 중인 작업에 취소를 건 뒤 worker 를 최대 5초 기다리며, 기한이 지나도 작업을 취소로 기록하지 않는다. 기한이 지나 join 없이 떼어 둔 worker 도 그 취소를 보고 멈추며 준비 중 사본을 임시 폴더째 지운다(`explorer_files/tests.rs` 의 `a_worker_left_behind_after_the_shutdown_deadline_still_cleans_its_staging`). 그 정리보다 프로세스가 먼저 끝나면 목적지에 숨은 임시 폴더(`.tasty-copy-*`)가 남을 수 있다 |
| 원격 제한 | 원격 쓰기를 받지 않는 server 의 mirror explorer 는 쓰기 항목을 꺼진 행으로 두고 툴팁에 이유를 달며, 단축키로 부르면 같은 이유를 info 토스트로 띄운다. 복사(Tasty 목록에 담기만 하고 OS 에는 올리지 않음)·경로 복사·이 폴더로 루트 설정은 어느 server 에서나 된다. mirror explorer 에서 복사한 클립보드는 원격 출처로 기록되어 같은 연결의 칸에만 붙여넣는다. 어느 창의 로컬 explorer 나 다른 연결의 칸에서는 붙여넣기 메뉴가 나오지 않고 단축키는 `explorer.state.remote_paste_unsupported` 토스트만 띄운다(위 경로 규칙). 원격 작업은 실행 취소 이력에 쌓지 않는다 |
| 원격 결과 모름 | 결과를 받기 전에 연결이 끊기거나 바뀌면 server 는 작업을 멈추지만(지금 항목에서 멈추고 준비 중 사본은 지운다) 이쪽은 어디까지 했는지 모른다. 복사·이동·휴지통 이동·이름 변경·새 폴더·새 파일이면 그 칸에 경고 카드를 둔다(`OpsState::push_lost`). 제목은 `explorer.remote.lost_title`(host = 연결의 호스트 글자(`ClientSessionState::host_text`, attach 프로필 이름)), 본문은 `explorer.remote.lost_body`(작업 동사 `explorer.remote.op_copy`·`op_move`·`op_trash`, `explorer.menu.op_rename`·`op_new_folder`·`op_new_file`)이고, 닫을 때까지 남으며 × 만 있다(Retry·Undo 없음). 작업이 끝나면 그 작업의 충돌 popup 도 닫힌다. 되돌리기와 남은 원본 지우기는 동사가 없어 `explorer.state.remote_result_unknown` 오류 토스트로 알린다. 매핑이 없어 연결이 끊기면 mirror workspace 가 닫히는 연결은 카드를 둘 칸이 없어 `attach.toast.mirror_disconnected` 만 보인다. 어느 쪽이든 그 연결의 원격 목록을 다시 읽게 한다. 끊긴 동안 보낸 목록 요청은 응답이 오지 않아 8초 뒤 읽기 오류 화면이 되므로, 연결이 돌아오면(재연결 설치 성공, 프레임 손실 뒤 재동기화 포함) 그 mirror workspace 의 explorer 는 기다리던 폴더와 오류가 된 폴더를 잊고 지금 폴더를 다시 읽는다(`ExplorerViewStore::reload_mirror`). 카드의 "연결이 돌아오면 폴더를 다시 읽는다" 는 이 동작이다. 요청을 보내지 못했으면 `explorer.state.remote_send_failed`, server 가 거절했으면 `explorer.state.remote_refused` 토스트를 띄운다. 연결이 끊기면 그 연결로 아직 보내지 않은 대기 요청은 시작하지 않고 버리며, 칸마다 버린 수를 `explorer.state.remote_queue_dropped` 오류 토스트로 알린다. server 가 결과 목록을 줄여 보냈으면(`truncated`) 카드의 Retry 는 줄인 목록 대신 처음 보낸 경로와 남은 원본을 그대로 다시 요청한다 |

### OS 파일 클립보드

복사·잘라내기는 Tasty 목록(앱의 모든 창이 함께 쓰는 explorer 목록)과 함께 OS 파일 클립보드에 파일 목록을 올린다. 다른 파일 관리자가 그 목록을 붙여넣고, 그쪽에서 복사한 파일을 explorer 가 붙여넣는다. 경로 복사(`copy_path`)는 이와 별개로 OS 텍스트 클립보드에 글자만 쓴다.

| OS | 올리는 형식 | 읽는 형식 | 잘라내기 |
|---|---|---|---|
| Linux | X11 `CLIPBOARD` 선택을 소유하고 `x-special/gnome-copied-files`·`text/uri-list`·`application/x-kde-cutselection`·경로 글자(`UTF8_STRING`·`text/plain;charset=utf-8`, 한 줄에 경로 하나)를 내준다 | GNOME 형식이 있으면 그것을, 없으면 `text/uri-list` 와 KDE 표지 | GNOME 첫 줄 `cut`, KDE 값 `1` |
| Windows | `CF_HDROP` + `Preferred DropEffect`. 호출 스레드의 메시지 전용 창을 클립보드 소유자로 연다 | 같다 | DropEffect 가 이동만(`DROPEFFECT_MOVE`)이면 잘라내기. 복사 비트가 함께 있으면 복사 |
| macOS | pasteboard 파일 URL | pasteboard 항목마다 파일 URL, 없으면 일반 URL. URL 이 없는 항목(글자 등)은 건너뛴다 | 표지가 없다. Finder 가 올린 목록은 늘 복사로 읽는다. Tasty 의 잘라내기는 Tasty 안에서만 이동이 된다(아래 우선순위) |

- **Wayland**: 텍스트 클립보드와 같이 X11 경로만 쓴다. Wayland 세션에서는 XWayland 가 Wayland 클립보드와 잇는다. GNOME Files 와 양방향으로 확인했다. X 서버에 연결하지 못하면 네이티브 Wayland 미지원/OS 접근 불가를 알린다. 파일 복사·잘라내기는 같은 Tasty 인스턴스의 공용 목록에만 보관했다고 알리고, 해당 목록의 붙여넣기도 OS 내용을 확인하지 못한 내부 목록 사용임을 알린다. 다른 Tasty 프로세스나 외부 앱에 전달됐다고 알리지 않는다.
- **우선순위 — 나중 복사가 이긴다**: Tasty 목록을 정한 직후 OS 클립보드를 한 번 읽어 둔다. 붙여넣을 때 OS 클립보드가 그때와 같으면 Tasty 목록(잘라내기 표시와 원격 출처 포함)을 쓴다. 다르면 다른 앱이 나중에 복사한 것이다. 파일 목록이면 그 목록을 Tasty 목록으로 삼아 붙여넣고, 글자 등 파일이 아닌 내용이면 오래된 Tasty 목록을 버리고 아무것도 하지 않는다. OS 클립보드에 접근하지 못하면(X 서버 연결 실패 등) 같은 출처의 Tasty 목록을 쓴다. 파일 게시에 성공했지만 직후 읽기만 실패하면 플랫폼의 예상 읽기 형식을 비교 기준으로 남겨 잘라내기 식별자를 유지한다. 게시에도 실패해 OS 스냅샷이 없고 접근이 복구되면 현재 외부 내용을 우선한다. 파일이 아닌 내용·원격 URI·응답하지 않는 새 소유자로 오래된 잘라내기를 재사용하지 않는다.
- **접근 불가와 빈 목록**: OS 접근 불가이며 내부 목록도 없으면 로컬 Paste 메뉴를 활성 상태로 남긴다. 실제 실행 시 접근 불가를 알리며 메뉴 조회 자체는 토스트나 슬롯 변경을 하지 않는다. 확인된 빈 클립보드는 기존처럼 붙여넣지 않는다. 원격 쓰기 capability·경로 출처 제한은 이 처리보다 우선한다.
- **응답하지 않는 소유자**: Linux 에서 다른 프로그램이 클립보드를 가졌는데 읽기 한도 안에 내용을 주지 않으면, 그 프로그램이 나중에 복사한 것으로 본다. Tasty 목록을 버리고 붙여넣지 않는다. 메뉴는 붙여넣기를 숨기고, 단축키·Command Palette 는 `explorer.state.os_paste_unreadable` 토스트를 띄운다. 복사 직후 읽었을 때도 같은 소유자가 응답하지 않았다면 바뀐 것이 없으므로 Tasty 목록을 쓴다.
- **원격 출처**: mirror explorer 에서 복사한 경로는 OS 클립보드에 올리지 않는다. 다른 앱이 같은 문자열을 로컬 파일로 읽지 않게 하기 위해서다. OS 쪽 목록이 나중이면 로컬 파일이므로 mirror explorer 칸에서는 붙여넣기 메뉴가 나오지 않고 단축키는 `explorer.state.cross_computer_paste_unsupported` 토스트를 띄운다. OS 쪽 목록에 로컬 파일이 아닌 항목(다른 호스트의 `file://`, `sftp://`, macOS pasteboard 의 웹 주소 등, 절대 경로가 아닌 항목)이 하나라도 있으면 일부만 붙여넣지 않는다. 메뉴는 붙여넣기를 숨기고, 단축키·Command Palette 는 `explorer.state.os_paste_not_local` 토스트를 띄운다.
- **잘라내기 소비**: 잘라낸 목록을 모두 옮기면 OS 클립보드가 아직 그 목록일 때만 비운다. 비교는 그 OS 에서 읽히는 모양으로 한다 — macOS 는 표지 없이 복사로 읽히므로 경로만 같으면 비운다. 다른 앱이 같은 파일을 다시 붙여넣지 못하게 하고, 그 사이 새로 복사한 내용은 지키기 위해서다. 다른 앱이 Tasty 가 올린 잘라내기를 붙여넣어 옮긴 뒤 클립보드를 바꾸면, Tasty 목록은 위 우선순위에 따라 버려진다.
- **수명**: Linux 에서 올린 목록은 Tasty 프로세스가 내준다. 다른 앱이 클립보드를 가져가면 내주기를 멈추고, Tasty 가 끝나면 사라진다(클립보드 관리자가 따로 보관하지 않는 한). 한 X11 요청보다 큰 목록은 내주지 않는다(INCR 분할 송신 없음, 읽기는 INCR 를 받는다).
- **창 사이**: Tasty 목록은 앱에 하나이고 모든 창이 같은 목록을 쓴다. 한 창에서 복사·잘라내기한 목록은 OS 클립보드 없이도 다른 창에서 붙여넣으며, 원격 출처 표시도 그대로 따라간다(다른 창의 로컬 explorer 에도 붙여넣지 않는다). 한 창에서 잘라낸 목록을 다른 창에서 옮기면 목록이 비고, 다른 창의 흐린 표시도 그 창을 다시 그릴 때 사라진다. 붙여넣은 창이나 대상 surface 가 작업 중에 닫혀도 끝난 잘라내기는 공용 목록과 OS 클립보드에서 지운다. 나중 복사 판정도 같은 목록 하나로 한다.
- **읽기 시점**: 붙여넣기와 메뉴를 열 때 UI 스레드에서 읽는다. Linux 는 읽기 한 번 전체(형식 목록·내용·INCR 조각 모두)를 500ms 안에 끝내지 못하면 위 "응답하지 않는 소유자" 로 본다. 메뉴를 열 때와 붙여넣을 때 한 번씩 읽으므로 응답하지 않는 소유자 앞에서 붙여넣기 한 번은 UI 를 최대 약 1초 멈춘다.

### 작업 실행 — 진행·충돌·결과

복사·이동·휴지통·실행 취소는 요청마다 작업 하나로 돈다. 로컬 작업은 앱 전체에서 한 번에 하나만 실행하고, 원격 작업은 연결(mirror workspace)마다 하나씩 실행한다. 나머지는 요청 순서대로 기다리며, 로컬 작업과 원격 작업은 서로 기다리지 않는다. 진행·충돌·결과는 작업을 요청한 explorer 칸에만 보인다. 드래그로 놓은 작업은 놓은 칸의 것이다.

- **겹친 선택**: 찾기의 하위 폴더 결과처럼 폴더와 그 안의 항목을 함께 고르면, 복사·이동·휴지통 이동은 폴더만 처리하고 안쪽 항목을 따로 처리하지 않는다. 같은 경로를 두 번 넣어도 한 번만 처리한다. 결과 카드의 수와 Retry 는 남긴 항목만 센다. 안쪽 항목은 폴더와 그 사이가 모두 실제 폴더일 때만 뺀다. 작업은 링크를 따라가지 않으므로 링크와 그 링크 너머의 항목을 함께 고르면 둘 다 처리한다. `..` 가 든 경로는 문자열만으로 포함 관계를 알 수 없어 빼지 않는다. 원격 server 가 실행하는 작업도 같은 엔진을 쓴다(`ops::drop_nested`).
- **준비 집계**: 복사·이동 worker가 먼저 파일과 바이트를 세며 `explorer.op.counting_copy`·`counting_move`로 찾은 수를 표시한다. 이 단계에는 막대를 그리지 않고 찾은 수의 공개는 250ms 간격(초당 최대 4회)이다. UI 스레드에서는 재귀 탐색하지 않는다. 폴더는 파일 수에 넣지 않으며 심볼릭 링크는 따라가지 않고 파일 항목 하나·0바이트로 센다. 읽기 실패로 합계를 모르면 바이트 총량은 미확정이다. 집계 중 취소하면 전송을 시작하지 않는다. 다시 그릴지는 전체 진행 snapshot과 질문 여부로 판정하므로 바이트가 늘지 않는 집계·빈 파일 전송도 갱신된다.
- **진행**: 요청한 칸의 상태줄이 진행 표시로 바뀐다. 위쪽 가장자리의 2px 막대, "Copying i of n · 파일 이름"(i·n은 중첩 폴더의 파일 수, 이름은 현재 파일명), 바이트(0이 아닌 합계를 아는 경우), 기다리는 작업이 있으면 "+n queued" Tag, 취소(×). Tag 를 누르면 상태줄 위에 대기열 팝오버가 열리고 실행 중인 작업과 기다리는 작업을 보인다. 기다리는 작업은 ×로 뺀다. 원격 칸이면 줄 맨 앞에 원격 글리프와 호스트 이름(연결의 호스트 글자(`ClientSessionState::host_text`, attach 프로필 이름), mono caption, workspace-mirror-fg, 글리프 크기 workspace-mirror-icon-size, 사이 간격 workspace-mirror-gap)을 두고, 대기열 팝오버 행 위에 "On {host} · one at a time"(`explorer.remote.queue_title`, 호스트 자리에 같은 표시) 머리 줄을 둔다. 머리 줄의 문구 조각과 호스트 표시 사이는 explorer-remote-queue-head-gap(낱말 공백 하나 폭)이다. 공용 위젯 `HostMark`·`QueueHeader` 가 그린다. 좁은 칸에서는 취소·대기 개수의 폭을 먼저 확보하고, 바이트와 호스트 표시는 남은 폭 안에서 말줄임한다. 호스트 글리프의 hover 응답과 줄인 글자에 전체 값을 붙인다. 이름이 전부 생략돼도 글리프에 올려 두면 전체 호스트 이름을 읽을 수 있다. 대기열 팝오버는 선호 폭 288을 유지하되 오른쪽 여백을 뺀 칸 폭 이내로 줄이고, 머리 줄은 뒤 문구의 폭을 먼저 남긴 뒤 호스트를 줄인다. 팝오버 행도 실제 할당 폭 안에 놓고, 취소·제거 버튼이 온전히 들어갈 자리를 먼저 남긴다. 글리프와 간격은 남은 폭이 있을 때만 그리며, 그보다 좁으면 글자를 줄인다. 버튼 하나도 들어가지 않는 폭에서는 부분 버튼이나 칸 밖 클릭 영역을 만들지 않는다. 머리 줄의 고정 여백·간격이 들어가지 않으면 머리 줄을 생략하고, 프레임 여백·테두리도 폭 이내로 제한한다. 폭이 0이면 프레임과 클릭 영역을 만들지 않는다. 남은 폭 계산은 `LogicalPx`로 유지하고 egui 호출 직전에만 원시 값으로 푼다. 행 제목·보조 문구도 기존 한 줄 말줄임을 따르며, 올려 두면 전체 문구를 보인다. "+n queued" 는 그 칸의 대기열만 센다. 막대는 알려진 0이 아닌 바이트 총량을 기준으로 하며, 총량이 0이거나 미확정이면 파일 수를 쓴다. 결과·실행 취소 이력의 개수는 계속 요청한 최상위 항목 단위다. 건너뛰기·실패한 최상위 항목은 그 아래 파일의 처리 구간을 마치지만 결과의 성공으로 세지 않는다.

한 줄 말줄임은 0을 포함한 실제 남은 폭을 사용한다. 1 논리 픽셀 미만에서도 줄 높이는 글꼴을 따르고, 말줄임 글리프의 그리기와 hover 영역은 실제 할당 폭 안에 제한한다.
- **이름 충돌**: 목적지에 같은 이름이 있으면 작업이 그 항목에서 멈추고 묻는다. 그 칸에 포커스가 있으면 칸 범위 popup(본문 위 scrim, 너비 `explorer-conflict-width`)이 뜨고, 아니면 상태줄이 "Waiting for your answer" 와 Show 를 보인다. 선택지는 Keep both(기본, Enter)·Skip·Replace(파일끼리만)·Cancel the rest(Esc, popup 을 닫아도 같다). 남은 충돌이 있으면 "Do this for the other n conflicts" 로 남은 충돌에 같은 답을 쓴다. 폴더 충돌은 Skip·Keep both 만 고를 수 있고, 남은 충돌에 Replace 를 고른 경우에도 폴더에서는 다시 묻는다. 질문하는 동안 그 칸이 사라지면 작업을 취소한다.
- **결과**: 끝나면 칸 오른쪽 아래, 상태줄 위에 결과 카드가 쌓인다. 다시 할 항목이 없는 카드(모두 끝났거나, 취소했지만 남은 항목이 없음)는 표준 토스트 시간(`overlay.toast_duration_ms`) 뒤 사라지고, 부분 성공·실패 카드와 취소로 하지 않은 항목이 있는 카드는 닫을 때까지 남는다. 실패·건너뛴 경로는 3개까지 보이고 나머지는 "and n more" 로 줄인다. 다른 디스크로 옮기며 원본을 다 지우지 못한 항목(원본이 전부 또는 일부 남음)은 실패로 보고 따로 적으며, 그 카드도 닫을 때까지 남는다. 원본 삭제는 지울 수 있는 것부터 지우다 멈추므로 원본이 일부만 남을 수 있고, 이때 사본이 유일한 온전한 자료다. 복사·다른 디스크로의 이동·그 이동의 되돌리기가 쓰는 준비용 임시 폴더(`.tasty-copy-*`)는 항목마다 직접 닫아(`TempDir::close`) 정리 오류를 받는다. 지우지 못하면 경고 로그를 남기고 `Report::staging_left` 에 그 경로와 OS 오류를 담는다. 카드 종류는 항목 결과와 Warning 중 높은 쪽이다(성공·정보 카드는 Warning 으로 오르고 Danger 는 그대로다). 제목·Undo 는 항목 결과대로 두고, 항목 줄 뒤에 임시 폴더 경로와 고정 문구 `explorer.result.staging_left`("Temporary folder couldn't be removed ({OS 오류})") 줄을 덧붙이며 닫을 때까지 남는다. 이 줄도 "and n more" 에 센다. Copy paths 는 그 경로도 담고, Retry 는 임시 폴더를 다시 다루지 않는다. 원격 server 의 결과도 같은 필드로 오고, 예산을 넘으면 다른 목록과 함께 반씩 줄인다.
- **카드 동작**: Retry n 은 실패하거나 건너뛴 항목과 취소로 하지 않은 항목(`Report::unprocessed` — 취소 때 멈춘 항목은 준비 중 사본을 지웠으므로 여기 든다)을 같은 작업으로 다시 요청한다. 취소한 작업의 카드도 Retry 를 낸다. 끝난 항목은 다시 요청하지 않는다. 취소 카드의 제목은 지금처럼 끝난 수를 보이고, 하지 않은 항목은 카드 줄에 나열하지 않으며 Retry 의 n 에만 센다. 원본을 다 지우지 못한 이동 항목은 다시 옮기지 않고 남은 원본 삭제만 별도 작업(`Operation::RemoveLeftovers`)으로 요청하며, n 에 함께 센다. 두 작업이 함께 나가면 결과 카드도 따로 뜬다. Copy paths 는 카드의 경로를 클립보드 텍스트로 담는다. Undo 는 실패·건너뜀이 없는 복사·이동 카드가 보이는 동안만 있다. 원본을 다 지우지 못한 항목은 되돌리기 단계로 기록하지 않으므로 어떤 경우에도 그 사본을 휴지통으로 보내지 않는다. 되돌리기의 결과도 카드로 보이며 되돌리지 못한 항목(지금 그 자리에 다른 항목이 있음·항목이 없어짐·Replace 로 덮어씀·복사 뒤 바뀌어 남김·옮긴 뒤 바뀜)을 나열한다. 복사 뒤 바뀌어 남긴 사본만 있으면 제목은 "Undid copy · kept n changed items"(`undo_kept_copy`)다. 지우지 못한 항목이 섞이면 그 수를 센 `undo_partial_copy` 가 제목이 되고, 남긴 사본은 줄로만 남는다. 두 제목 모두 경고 톤이다.
- **남은 원본 다시 지우기**: 이동이 원본 삭제에서 멈추면 그 항목의 원본 경로·공개한 사본 경로·사본의 종류(파일·폴더·링크)와, 원본이 파일이면 지우려던 때의 수정 시각·크기를 기록한다(`job::leftover::Leftover`). Retry 는 항목마다 먼저 사본이 그 경로에 같은 종류로 있는지 보고, 없거나 다른 종류면 원본을 남기고 "사본이 없거나 종류가 바뀌어 원본을 남겨 둠"(`explorer.result.copy_missing`)을 적는다. 원본 파일의 수정 시각·크기가 기록과 다르면 그 뒤 고친 원본으로 보고 남긴다(`explorer.result.changed_kept`). 이렇게 남긴 항목은 보통 실패로 카드에 남고, 그 카드의 Retry 는 원본 삭제가 아니라 이동 자체를 다시 요청한다(이름이 겹치면 충돌 질문을 거친다). 원본이 이미 없으면 지울 것이 없어 끝난 것으로 센다. 사본 경로가 상위 링크·마운트·하드링크로 원본과 같은 파일을 가리키면 비교가 자기 자신과 같다고 나오므로, 비교 전에 원본과 사본의 파일 식별자(unix 는 장치 번호와 inode, Windows 는 볼륨 일련번호와 파일 인덱스)를 대조해 같으면 지우지 않고 "사본이 원본과 같은 파일이라 남겨 둠"(`explorer.result.same_as_copy`)으로 원본이 남은 경고 카드와 Retry 를 보인다. 폴더는 맨 위와 안쪽 항목마다 대조한다. 식별자를 읽지 못하면 같은 파일이 아니라고 보장할 수 없어 지우지 않고 그 OS 오류를 사유로 보인다. Windows 의 식별자 대조는 실 기기에서 측정하지 않았다. 링크 원본은 사본 링크와 대상 경로가 같을 때만, 파일 원본은 사본과 내용이 같을 때만 지운다. 확인을 통과하면 처음 이동과 같은 영구 삭제로 원본을 지운다. 폴더 원본은 안쪽을 걸어, 사본 쪽 같은 상대 경로에 같은 항목이 있을 때만 그 항목을 지운다. 같음은 종류가 같고, 파일은 크기가 같은 뒤 내용을 버퍼 단위(256 KiB)로 바이트 비교해 끝까지 같으며(첫 차이에서 멈춘다), 링크는 따라가지 않고 링크의 대상 경로가 같은 것이다. 비교에 시간이 걸리므로 지우기 직전에 원본 항목의 크기·수정 시각·식별자를 다시 읽어, 비교를 시작할 때와 다르면 그사이 바뀐 것으로 보고 남긴다(파일 원본은 `explorer.result.changed_kept`, 폴더 안 항목은 남긴 개수에 든다). 비교하려고 읽지 못한 파일은 같은지 모르므로 남기고 그 OS 오류를 원본이 남은 사유로 보인다. 진행은 다른 파일 작업과 같은 상태줄에 보이되 이동 문구를 빌리지 않는다. 비교하는 동안은 "Checking i of n · 이름"(`explorer.progress.checking`), 지우는 동안은 "Removing originals i of n · 이름"(`explorer.progress.removing_originals`)이고, 대기열 제목은 "Remove n originals"(`explorer.queue.remove_originals`)다. ×로 취소하면 버퍼 단위로 멈추며 아직 보지 않은 원본은 지우지 않는다. 취소해도 결과는 취소 카드가 아니라 원본이 남은 경고 카드다. 그때까지 다 지운 원본은 빠지고, 도중에 멈췄거나 손대지 못한 원본은 "취소함. 원본이 전부 또는 일부 남아 있음"(`explorer.result.remove_cancelled`)으로 남은 개수에 들며 Retry 가 다시 나온다. 사본에 없는 항목(이동 뒤 넣었거나 바꾼 것)은 남기며, 사본에 없는 하위 폴더는 통째로 남기고 하나로 센다. 다 지워 빈 폴더는 지운다. 남긴 항목이 있으면 그 항목은 "사본에 없는 항목 n개를 남겨 둠"(`explorer.result.kept_not_in_copy`)으로 원본이 남은 경고 카드에 보이고, 원본 삭제만 다시 하는 Retry 가 다시 나온다. 또 지우지 못한 경우도 같은 경고 카드와 Retry 가 다시 나온다. 모두 지우면 성공 카드 "Removed n originals"(`explorer.result.removed_originals`)가 표준 시간 뒤 사라지고, 남긴 원본이 있으면 원본이 남은 경고 카드(`explorer.result.source_left_move`, 남은 것만 센다)다. 어느 쪽도 Undo 는 없다. 한 번의 Retry 가 원본 지우기와 다시 옮기기를 함께 넣으면 두 작업의 결과는 둘 다 끝난 뒤 카드 하나로 합친다. 수는 더하고, 실패나 남은 원본이 있으면 취소보다 그쪽을 보이며, 일부만 되돌릴 수 있으므로 Undo 는 없다. 그중 하나를 대기열에서 빼면 나머지 결과만으로 카드를 낸다.
- **사본 metadata**: 사본 파일은 원본의 권한과 수정 시각을, 사본 폴더는 원본의 권한을 갖는다. 소유자·확장 속성(xattr)·ACL·폴더 수정 시각은 옮기지 않고, 링크는 링크로 다시 만들 뿐 metadata 를 옮기지 않는다. 다른 파일시스템으로의 이동과 그 이동의 되돌리기도 같은 복사를 쓴다(`src/app/file_jobs/copy_meta.rs`).
  - 파일은 내용을 다 쓴 뒤 권한과 수정 시각을 건다. 수정 시각을 걸지 못하면(지원하지 않는 파일시스템) 내용은 이미 옮겼으므로 작업을 실패로 두지 않고 경고 로그만 남긴다.
  - 폴더 권한은 사본 전체를 공개(rename)한 뒤 안쪽 폴더부터 건다. 읽기 전용 폴더에는 안쪽 항목을 만들 수 없고, 다른 부모로 옮기는 rename 도 막히기 때문이다. 그래서 공개 전에 실패하거나 취소해도 임시 디렉터리를 지울 수 있다. 공개 뒤 권한을 걸지 못한 폴더는 경고 로그만 남긴다.
  - 실행 취소의 "작업 뒤 바뀌었나" 판정은 공개 직후 사본의 수정 시각(곧 원본의 시각)을 적어 두고 비교하므로, 사본을 고치면 그대로 알아챈다.
  - 읽기 전용 폴더의 사본도 읽기 전용이므로, 그 사본을 실행 취소(휴지통 이동)하거나 휴지통으로 보낼 때는 원본 읽기 전용 폴더를 휴지통으로 보낼 때와 같은 권한 제약을 받는다. 같은 파일시스템의 휴지통 이동은 폴더를 다른 부모로 옮기는 rename 이라 OS 에 따라 그 폴더의 쓰기 권한이 없으면 실패하고, 그 항목은 실패로 결과에 남는다.
- **안전**: 기본 답은 덮어쓰지 않는 Keep both 다. Replace 는 사용자가 고른 파일에만 쓰고 폴더는 덮어쓰지 않는다. 질문 없이 공개하는 순간에 같은 이름이 생겨도 덮어쓰기 금지 rename 이 실패하면 새 이름으로 둔다.

### 되돌리기·다시 실행 이력

explorer 칸마다 최근 10 단계(`history::MAX_STEPS`)의 되돌리기 이력과, 되돌린 단계의 다시 실행 목록을 둔다. 이력은 메모리에만 있고 칸이 닫히거나 앱을 다시 열면 사라진다. 목록 UI 는 없고 맨 위 단계만 쓴다.

- **쌓는 것**: 복사·이동(붙여넣기, 드래그 놓기, Retry 가 다시 보낸 복사·이동), 이름 변경, 새 폴더·새 파일. 같은 이름으로 바꾼 이름 변경이나 실패한 작업처럼 바뀐 것이 없으면 쌓지 않는다. 휴지통 이동은 쌓지 않는다(복원은 OS 에서 한다). 11 번째 단계가 쌓이면 가장 오래된 단계가 밀려난다.
- **일부만 된 복사·이동**: 실패·건너뜀·취소가 섞여도 된 항목이 있으면 된 항목만 한 단계로 쌓는다. 그 단계의 다시 실행은 된 항목의 원본만 다시 보낸다(실패·건너뜀·취소로 하지 않은 원본은 뺀다). 원본을 다 지우지 못한 이동 항목은 사본이 하나뿐인 자료일 수 있어 단계에 넣지 않고, 결과 카드의 Retry 로 원본 삭제만 다시 한다. 결과 카드의 Undo 는 지금처럼 모두 끝난 복사·이동에만 나오므로, 일부만 된 작업은 메뉴의 Undo 행이나 `explorer_undo` 로 되돌린다.
- **다시 실행 목록**: 이력의 단계를 되돌리기가 끝까지 되면 그 단계가 다시 실행 목록에 오른다. 되돌리기에 실패한 항목이 있으면 그 단계는 다시 실행할 수 없어 버린다. 사용자가 새로 한 복사·이동·이름 변경·새 항목이 무엇이든 바꾸면 다시 실행 목록을 비운다. 다시 실행이 끝나면 그 결과가 되돌리기 맨 위에 쌓이고, 남은 다시 실행 단계는 그대로다.
- **메뉴 행**: 빈 영역 메뉴의 Properties 묶음 앞에 구분선과 함께 "Undo {작업}" · "Redo {작업}" 행을 둔다(`explorer.menu.undo`·`redo`, 작업은 `op_copy`·`op_move` 의 "copying n items" 형태이며 `t_count` 로 읽어 영어는 1개일 때 `_one` 의 "copying 1 item" 을 쓴다. 이름 변경·새 항목은 항목이 하나라 수 없이 `op_rename`·`op_new_folder`·`op_new_file` 의 "rename"·"new folder"·"new file" 이다). 해당 단계가 없으면 `undo_none`·`redo_none`의 "Undo"·"Redo"를 꺼진 행으로 유지한다. 항목 메뉴와 mirror explorer 에는 두지 않는다. 바인딩이 있는 모든 메뉴 행은 `KeybindingSettings`의 첫 바인딩을 단축키 열에 보인다. 경로 복사·복사·잘라내기·붙여넣기·이름 변경·휴지통·새 폴더·새 파일·속성·Undo·Redo 모두 같은 규칙이며, 바인딩을 비우면 표시하지 않는다.
- **되돌릴 수 없게 된 단계**: 메뉴를 열 때 맨 위 단계의 되돌리기 단계를 되돌리기 작업과 같은 규칙으로 확인한다(사본이 없어짐·옮긴 항목이 없어짐·원래 자리에 다른 항목이 있음·복사 뒤 바뀜·옮긴 뒤 바뀜·Replace 로 덮어씀). 이름 변경과 새 항목의 단계는 같은 판정에 그 작업에 맞춘 이유 문구(`name_taken`·`changed_after_rename`·`changed_after_create`)를 쓴다. 하나라도 되돌릴 수 있으면 행을 켜고, 모두 되돌릴 수 없으면 행을 끄고 "Can't undo. {이유}"(`explorer.menu.undo_stale`)를 툴팁으로 보인다. 툴팁은 Linux(GTK)·macOS 메뉴에서 보이고, Windows 팝업 메뉴는 항목 툴팁이 없어 같은 이유 문장을 꺼진 행 이름으로 보인다. 단계가 256 개를 넘으면 확인하지 않고 켠다. 단축키로 그런 단계를 부르면 단계를 꺼내지 않고 그 칸에 같은 문구의 Info 토스트를 띄운다.
- **다시 실행할 수 없게 된 단계**: 되돌리기가 끝날 때 대상 경로마다 그 자리 항목의 종류(파일·폴더·링크)와 수정 시각을 적어 둔다. 대상은 복사·이동의 원본, 이름 변경의 원래 경로, 새 항목의 그 이름이다. 새 항목은 되돌린 뒤 그 이름이 비어 있으므로, 그 자리에 무엇이 생기면 바뀐 것으로 본다. 메뉴를 열 때와 다시 실행할 때 지금 모습과 비교해, 하나라도 없어졌거나(`explorer.result.gone`) 종류·수정 시각이 바뀌었으면(`explorer.menu.changed_after_undo`, "It changed after the undo") 다시 실행하지 않는다. 다시 실행은 붙여넣기를 통째로 다시 보내 항목별로 거르지 않으므로, 하나만 바뀌어도 막는다. 행은 꺼지고 "Can't redo: {이유}"(`explorer.menu.redo_stale`)를 Linux·macOS에서는 툴팁으로, Windows에서는 행 이름으로 보이며, 단축키는 단계를 꺼내지 않고 같은 문구의 Info 토스트를 띄운다. 폴더는 폴더 자신의 수정 시각만 본다.
- **맨 위 단계가 막혔을 때**: 맨 위 단계를 되돌리거나 다시 실행할 수 없으면 그 아래 단계에도 닿지 않는다. 막힌 단계를 버리고 아래로 내려가는 수단은 없으므로, 그 단계와 아래 단계는 새 작업이 쌓여 밀려날 때까지 쓸 수 없다. 디자인 회신의 꺼진 행 그대로이며, 목록 없이 맨 위만 쓰는 설계라 순서를 건너뛰지 않는다.
- **단계가 이력에서 사라지는 경우**: 아래 경우 꺼낸 단계는 되돌리기·다시 실행 어느 쪽에도 남지 않는다. 파일 상태는 실제와 맞으므로 데이터 안전 문제는 아니고, 결과 카드나 토스트가 결과를 보인다.
  - 이력의 되돌리기가 일부만 됐다(실패·건너뜀·취소가 하나라도 있음). 남은 항목은 결과 카드가 나열한다.
  - 다시 실행한 복사·이동에서 된 항목이 없거나, 다시 실행한 이름 변경·새 항목이 실패했다(같은 이름이 생긴 경우 등. 오류 토스트).
  - 작업이 항목별 결과가 아닌 오류로 끝났다(오류 토스트).
  - 실행 직전에 요청한 explorer 칸이나 surface 가 닫히거나 바뀌어 작업을 시작하지 않았다.
  - 작업이 끝났을 때 요청한 창·엔진·surface 가 바뀌어 결과를 그 칸에 반영하지 못했다.
  - 결과 카드의 Undo 가 끝나기 전에 새 복사·이동이 끝나 다시 실행 목록을 비웠다(결과 카드와의 관계).
- **결과 카드와의 관계**: 결과 카드의 Undo 는 그대로 한 단계만 되돌리고 Redo 버튼은 없다. 카드가 되돌린 단계가 이력에 있으면 이력에서 빼고 이력의 되돌리기로 처리하므로, 끝까지 되면 다시 실행할 수 있다. 다만 카드의 Undo 는 대기 중인 새 복사·이동 뒤에 들어갈 수 있다. 그 작업이 먼저 끝나 다시 실행 목록을 비웠다면 되돌린 단계는 다시 실행 목록에 오르지 않는다. 새 작업보다 오래된 단계가 다시 실행 맨 위에 오면 시간 순서가 뒤바뀌기 때문이다.
- **요청하지 못한 경우**: 대기열이 가득하거나 요청이 너무 커서 받지 않으면 꺼낸 단계를 제자리에 돌려놓는다.
- **이력에 반영할 작업이 남아 있을 때**: 그 칸이 요청한 복사·이동·이름 변경·새 항목·이력의 되돌리기·다시 실행이 대기열에 있거나 실행 중이면(충돌 질문을 기다리는 동안 포함) 맨 위 단계가 아직 정해지지 않았으므로 되돌리기·다시 실행을 받지 않는다. 메뉴의 이력이 있는 행은 꺼지고 Linux·macOS에서는 툴팁에, Windows에서는 행 이름에 "Wait until the file operation in progress finishes"(`explorer.menu.history_busy`)를 보이며, 단축키는 같은 문구의 Info 토스트를 띄운다. 그 작업이 끝나면 그것이 맨 위가 되므로, 붙여넣거나 이름을 바꾼 직후 누른 되돌리기가 그 이전 단계를 되돌리는 일이 없다. 이름 변경·새 항목은 진행 표시가 없지만 실행 중인 동안 같은 조건으로 센다. 이력과 무관한 휴지통 이동은 막지 않는다. 결과 카드의 Undo 는 카드가 가리키는 단계를 되돌리므로 이 조건과 무관하게 받는다.
- **메뉴 행과 실행 대상**: 메뉴를 열 때 이력의 번호를 적어 두고, 행을 고를 때 이력이 그 사이 바뀌었으면 아무것도 꺼내지 않고 "The history changed. Open the menu again"(`explorer.menu.history_changed`) Info 토스트를 띄운다. 그래서 고른 행은 늘 그 행 이름에 적힌 단계를 실행한다.

### 드래그 앤 드롭

- **시작**: 목록·격자·자세히 보기의 항목을 누른 채 끈다. mirror explorer 에서도 시작한다. popup·modal 이나 충돌 popup 이 떠 있는 동안은 시작하지도 놓지도 않는다.
- **칩**: 포인터 오른쪽 아래(`space-md`)에 칩이 따라온다. 1줄은 항목 이름 또는 "n items", 2줄은 "Move to 폴더"·"Copy to 폴더" 또는 "Can't drop — 이유" 를 동작 색으로 쓴다.
- **동작**: 같은 디스크면 이동, 다른 디스크면 복사. 원격 경로는 server 가 폴더 목록과 함께 알린 디스크 식별값으로 비교하고, 아직 읽지 않은 하위 폴더는 상위 폴더의 값을 쓴다. 값을 알리지 않는 옛 server 이거나 모르면 복사한다. `KeybindingSettings::explorer_drag_flip_modifier`(기본 macOS `option`, 다른 OS `ctrl`)를 누르고 있으면 뒤집는다. OS 파일은 항상 복사한다.
- **자동 스크롤**: 목록의 항목 끌기와 사이드바 트리는 영역 선택과 같은 `explorer_autoscroll_zone`(24px) 띠를 쓴다. 속도는 `20줄/초 × t²`이며 목록 밖에서는 최고 속도로 유지한다. Detail 행·List 행·Grid 칸 줄·트리 행의 실제 높이를 사용하고 휠 입력도 유지한다. 추가 표시는 없다. 스크롤 거리는 `LogicalPx`로 계산하고 오른쪽 경계는 다음 영역에 속하게 하여 공유 경계에서 두 영역을 동시에 스크롤하지 않는다. 세로로 겹치는 열의 여러 탐색기에서는 포인터가 있는 영역, 밖에서는 가장 가까운 영역 하나만 스크롤한다. Detail은 표 본문의 실제 ScrollArea를, List·Grid·트리는 각 목록의 ScrollArea를 등록한다. 각 egui 패스에서 목록을 모두 그린 뒤 현재 뷰포트만 모아 소유자를 결정하고, 그 ScrollArea의 휠·영역 선택 처리 뒤 offset에 거리를 더한다. 리사이즈·분할 변경·목록 제거 때 이전 패스의 사각형은 후보에 넣지 않는다.
- **대상 표시**: 놓을 수 있는 대상에 1px 안쪽 링과 틴트(`explorer-drop-target-*`)를 그린다. 파일 행이나 빈 곳이면 목록 본문 전체가 대상이다. 닫힌 트리 폴더 위에 `explorer_drag_expand_delay()`(800ms) 머물면 펼친다. 펼침 상태를 바꾼 뒤 다음 프레임을 요청해 포인터를 더 움직이지 않아도 하위 폴더가 나타난다.
- **거절**: 폴더를 자기 자신이나 그 하위로(`a folder can't go inside itself`), 이미 있는 폴더로 이동(`already in this folder`, 복사는 허용), 다른 컴퓨터의 칸으로(`different computer` — 로컬과 원격 사이, 서로 다른 연결 사이, OS 파일을 mirror explorer 로), 원격 쓰기를 받지 않는 server 의 mirror explorer 로(`this remote can't change files`), 쓰기 권한이 없는 로컬 폴더로(`no write access`). 로컬 폴더의 쓰기 권한은 unix 는 `access(W_OK)`, Windows 는 폴더 ACL 의 항목 만들기 권한과 읽기 전용 볼륨 여부로 본다(폴더의 읽기 전용 속성은 쓰기를 막지 않으므로 보지 않는다). 원격 폴더의 쓰기 권한은 이 컴퓨터에서 확인하지 않고 server 의 결과로 알린다. 거절된 대상에는 링을 그리지 않는다. explorer 가 아닌 곳에서는 시작한 칸이 "Can't drop" 칩만 보인다.
- **경로 비교**: 로컬 판정은 작업 엔진처럼 링크를 푼 경로로 한다. 놓을 폴더는 링크를 풀고, 원본은 부모 폴더만 풀어 링크 항목이면 링크 자체를 가리킨다. 그래서 폴더를 자기 자신의 링크 이름 아래로 놓으면 거절하고, 링크 이름으로 본 파일을 원래 폴더로 옮기면 이미 있는 폴더로 본다. 읽지 못한 경로는 그대로 비교하며 작업 엔진이 다시 막는다. Windows 는 대소문자와 `canonicalize` 의 `\\?\` 접두사를 무시하고 비교한다(`view/drag.rs` 의 `resolve`).
- **취소**: Esc 를 누르면 드래그를 취소하고, 버튼을 놓을 때까지 다시 시작하지 않는다.
- **한 번 놓기**: 버튼을 놓은 pass 에서 작업을 한 번 요청하고 payload 를 지운다. egui 가 같은 프레임을 다시 돌려도 다시 요청하지 않는다.
- **OS 파일**: explorer 칸 위에서는 창 전체의 drop overlay 를 그리지 않고, 칸이 링과 "Copy to" 칩을 보인다. 이 표시는 드래그 중 포인터 위치를 아는 동안만 나온다. winit 0.30 의 X11 은 XDND 중 위치를 알려 주지 않아 표시 없이 창 overlay 가 남고, 놓은 뒤의 포인터 위치로 대상 칸을 정한다. 다른 곳에 놓은 파일은 지금처럼 연다.
- **꺼내기**: 로컬 항목의 외부 앱 Copy 드래그와 플랫폼별 제한은 [외부 앱으로 끌어 내기](#외부-앱으로-끌어-내기)를 따른다.

### 외부 앱으로 끌어 내기

로컬 Explorer 항목은 기존 내부 드래그 문턱을 넘은 뒤, 버튼을 누른 채 원본 윈도우 경계를 벗어나면 OS 파일 드래그로 넘긴다. 같은 윈도우 안에서는 기존 폴더 대상 판정과 이동·복사 규칙을 유지한다. 원격 origin의 경로는 OS에 내보내지 않는다.

- Windows는 winit UI 스레드에서 `OleInitialize` 후 Shell `IDataObject`(파일 목록 `CF_HDROP`와 Shell ID 목록)를 `DoDragDrop`에 넘긴다. View·engine borrow를 해제한 App 단계에서 모달 루프를 실행한다. `DRAGDROP_S_DROP`과 `DROPEFFECT_COPY`가 모두 확인돼야 수락으로 처리하며 취소·실패·불명은 구분한다. OLE 기본 커서를 쓰고 클립보드를 바꾸지 않는다.
- macOS는 실제 held gesture의 `NSEvent`, 파일 URL, `NSWorkspace`의 시스템 파일 아이콘으로 `NSDraggingSession`을 시작한다. main thread에서 source와 session을 실제 종료 콜백까지 보관한다. 콜백은 View를 직접 빌리지 않고 윈도우·세션 ID를 가진 App 이벤트를 보낸다. 완료 시 원래 `SurfaceBinding`을 재확인하며 늦은 완료는 다른 제스처를 종료하지 않는다. source가 닫혀도 네이티브 객체를 먼저 해제하지 않는다. AppKit에는 공개 세션 강제 종료 API가 없으므로 호스트 취소는 operation mask와 완료 소비를 무효화하고 실제 종료까지 객체를 유지한다.
- X11은 별도 연결의 `XdndSelection`에 `text/uri-list`를 제공한다. winit이 전달한 원래 제스처의 좌표와 버튼 해제만 사용하며 포인터를 조회하거나 grab하지 않는다. XDND v5의 Copy 수락·완료를 구분하고 큰 목록은 INCR로 전송한다. `CLIPBOARD`는 건드리지 않는다.
- X11의 `XdndProxy`는 자기 자신을 가리키는 실제 윈도우인지 먼저 검증한 뒤, 그 프록시의 `XdndAware`와 버전으로 수신 여부를 결정한다. 원래 대상에 `XdndAware`가 없어도 유효한 프록시로 보낼 수 있다. 메시지는 프록시에 전달하되 대상 ID는 원래 윈도우를 유지한다. 자기 참조가 없거나 파괴된 프록시는 무시하고 원래 대상의 awareness로 돌아간다. 다른 연결·프로토콜 오류는 전파한다([XDND 명세](https://johnlindal.wixsite.com/xdnd/technical-details)).
- Wayland는 winit과 같은 libwayland 연결에 별도 event queue·seat·pointer를 바인딩한다. data-device는 outbound 시작 시에만 생성하고 종료 시 해제한다. 활성 self-origin 세션만 자체 클라이언트 대상을 거절하며 외부 incoming offer나 텍스트 selection에 accept/set_actions를 보내지 않는다. 실제 왼쪽 버튼 press serial과 원본 surface의 implicit grab이 일치할 때만 시작한다. `wl_data_device_manager` v3와 `wl_seat` v5 이상이 필요하다. FD 전송은 비차단이며 Copy action과 `dnd_finished`를 확인한다. 새 Wayland 연결이나 임의 serial을 만들지 않는다.
- Wayland의 외부 프록시는 원본 `Arc<Window>`와 winit `OwnedDisplayHandle`을 함께 보관한다. 윈도우 객체만으로 event-loop 연결 수명을 가정하지 않는다. 닫힌·교체된 SurfaceBinding, popup/modal·무대 전환은 해당 View의 세션을 취소한다.
- OS가 제공하는 파일 아이콘·커서 피드백을 쓰고 Tasty 비트맵·배지를 만들지 않는다. 새 시각 컴포넌트나 갤러리 변형은 없다.
- 외부에는 Copy만 제안한다. Tasty는 원본을 지우지 않으며 외부 앱의 복사·파일 변경을 Tasty Undo 이력에 넣지 않는다. 시작·수신기 수락·완료는 서로 다른 상태이며 시작만으로 성공을 알리지 않는다. 실패는 기존 Info 토스트, 취소와 확인하지 못한 완료는 진단 로그로 구분한다.
- X11에서 취소는 기존 `explorer_clear_selection` 바인딩(기본 Esc)을 따른다. 네이티브 세션 중에는 선택을 지우는 대신 드래그를 취소하고 그 키를 소비한다. Windows OLE·macOS AppKit·Wayland compositor가 관리하는 Esc 취소는 OS 동작이며 Tasty가 변경하지 않는다.
- 한 제스처는 한 번만 넘긴다. 넘긴 뒤 egui payload와 눌린 버튼을 정리하여 원본으로 돌아와도 내부 Drop이 중복 실행되지 않는다. 세션 결과는 그 View·SurfaceBinding에만 적용한다.
- 같은 프로세스의 다른 Tasty 윈도우는 수신 대상으로 허용하지 않는다. X11은 대상 조상의 `_NET_WM_PID`를 검사하고 Wayland는 활성 outbound의 자체 data-device offer를 거절한다. Windows는 대상 HWND의 프로세스를 확인해 자체 대상 release를 취소하고 macOS는 WithinApplication에 None을 반환한다. 별도 프로세스의 Tasty는 X11에서 일반 외부 수신기로 보일 수 있다. 프로세스 간 Tasty 드래그를 지원하는 기능으로 보장하지 않는다.
- 경로는 절대 경로·NUL 없음·비어 있지 않음을 검사하고 인코딩 상한을 1 MiB로 제한한다. 심볼릭 링크는 선택한 경로 그대로 제공한다. X11 응답·완료의 10초 대기 만료, Wayland 전송·drop 완료의 30초 대기 만료는 성공으로 처리하지 않는다.

구현: `src/view/main/file_drag.rs`, `src/file/drag.rs`, `src/file/drag/desktop/windows.rs`, `src/file/drag/desktop/macos.rs`, `src/file/drag/x11.rs`, `src/file/drag/wayland.rs`. Windows는 호스트 라이브러리 전체를, macOS는 AppKit 어댑터와 공통 native 요청 모듈을 고정 의존성으로 크로스 타깃 컴파일했다. macOS 호스트 전체는 Apple C 툴체인이 없는 환경의 SQLite 빌드에서 중단됐으며, 두 OS의 실 기기 파일 관리자 수신·취소는 미측정이다. Linux는 격리된 X11 및 Weston/GTK 수신기로 실측했다. 현재 고정 winit에는 네이티브 Wayland incoming 파일 이벤트 구현이 없어, 파일 관리자에서 Tasty로 끌어오는 Linux 경로는 X11/XWayland에서 제공한다.

### 작업 뒤 목록 갱신

작업이 끝나면 성공·실패·부분 성공과 관계없이 실제 상태를 다시 읽는다. 요청한 surface 가 그 사이 닫혔어도 다른 explorer 는 갱신한다. 로컬 작업의 대상은 모든 윈도우의 로컬 explorer 이고, 원격 작업의 대상은 같은 mirror workspace 의 explorer 다. 원격 explorer 는 해당 폴더의 목록 캐시를 지우고 응답을 기다리는 폴더는 그대로 둔다.

| 작업 | 다시 읽는 폴더 | 사라질 수 있는 경로 |
|---|---|---|
| 복사 후 붙여넣기 | 붙여넣은 폴더 | 없음 |
| 잘라내기 후 붙여넣기 | 붙여넣은 폴더, 각 원본의 부모 폴더 | 각 원본 |
| 휴지통 이동 | 각 항목의 부모 폴더 | 각 항목 |
| 이름 변경 | 항목의 부모 폴더 | 원래 이름의 경로 |
| 새 폴더·새 파일 | 만든 폴더 | 없음 |
| 드래그 놓기·OS 파일 드롭 | 복사·이동에 따라 위 붙여넣기 두 행과 같다 | 같다 |
| 실행 취소 | 되돌린 항목이 있던 폴더와 옮겨 놓을 폴더 | 복사로 만든 항목, 이동해 놓은 자리 |
| 외부 프로그램으로 열기 | 없음 | 없음 |

검색 결과를 보는 동안 툴바·빈 영역 메뉴·단축키로 새 폴더·새 파일을 만들거나 일반 붙여넣기를 하면 검색 시작 폴더가 목적지다. 선택한 결과의 부모 폴더로 바꾸지 않는다. 생성·붙여넣기가 끝나면 검색을 다시 하며 현재 이름·확장자·숨김 조건에 맞는 새 항목은 결과에 나타난다. 폴더 메뉴에서 명시한 대상과 Paste into는 그 폴더를 쓰는 기존 계약을 유지한다. `search_result_tests.rs`의 `creating_and_pasting_while_searching_use_the_start_folder_and_reappear`가 목적지와 재등장을 확인한다.

- 현재 폴더가 다시 읽는 폴더이거나 사라질 수 있는 경로(또는 그 안)이면 목록을 다시 읽는다. 사라진 폴더 안을 보던 explorer 는 읽기 오류 화면을 보이며 다른 경로로 옮기지 않는다. 그 화면의 "상위 폴더로" 는 남아 있는 가장 가까운 상위 폴더로 간다.
- 현재 폴더가 해당하지 않아도 사이드바 트리에 그 폴더의 하위 목록이 있으면 그 항목만 지워 다시 읽게 한다.
- 하위 폴더 검색 결과를 보이는 로컬 explorer 는 다시 읽는 폴더나 사라질 수 있는 경로가 검색 시작 폴더 안에 있으면 검색을 처음부터 다시 한다. 결과는 하위 폴더의 항목이라 현재 폴더 비교로는 잡히지 않기 때문이다.
- 경로는 문자열 그대로 비교한다(정규화하지 않는다). 심볼릭 링크를 거쳐 들어간 같은 폴더처럼 다른 경로 문자열로 보는 explorer 는 갱신 대상이 아니며, 새로고침(`F5`)으로 맞춘다.
- 보던 폴더의 이름이 바뀐 경우도 원래 경로가 사라진 것으로 본다. 그 안을 보던 explorer 는 읽기 오류 화면을 보이며 새 이름을 따라가지 않는다.
- 다시 읽어도 선택은 비우지 않는다. 폴더가 그대로면 남아 있는 항목의 선택이 유지되고, 다시 읽은 목록에 없는 경로는 선택에서 빠진다(상태줄의 선택 수도 그만큼 준다).

### 사용자 작업과 에이전트 작업의 경계

- 이 문서의 파일 작업은 사용자 입력으로만 시작한다. release IPC 는 컨텍스트 메뉴를 열거나 그 항목을 누르는 메서드를 제공하지 않는다 ([debug IPC](../../dev-guide/debug-ipc.md)).
- 에이전트는 셸에서 파일을 직접 다룬다. 탐색기 전용 파일 API 는 없다.
- 작업 완료가 포커스를 가져가지 않는다. 다른 surface 의 선택·스크롤도 바꾸지 않는다 ([포커스 정책](../../design/policies/focus.md)).

## 인터페이스

- **AI Agent (IPC/CLI)**: 없음. 위 경계 절을 따른다.
- **사용자 트리거**: 컨텍스트 메뉴, 툴바·More 메뉴, 드래그 앤 드롭, 결과 카드의 Retry·Undo, `copy`·`cut`·`paste`·`explorer_new_folder`·`explorer_new_file`·`explorer_rename`·`explorer_trash`·`explorer_properties`·`explorer_toggle_preview`·`explorer_toggle_hidden`·`find` 바인딩, Command Palette. 단축키 표는 [Explorer](index.md#사용자-트리거-단축키--keybindingsettings) 에 있다.
- **원격 / 점유**: mirror explorer 는 위 원격 제한을 따른다. 원격 쓰기 요청은 그 server 의 workspace 를 점유한 연결만 보낼 수 있고, server 는 연결마다 한 번에 하나를 실행한다. 메시지 형식은 [attach 동작](../../dev-guide/attach-behavior.md)의 사용자 정의 이벤트 절에 있다.

## 비-목표 (Out of scope)

- 이 컴퓨터와 원격 호스트 사이의 파일 전송(복사·이동·드래그). 원격 쓰기는 같은 연결 안에서만 한다 (docs/features/remote-attach/index.md).
- 원격 작업의 실행 취소 이력. 결과 카드의 Undo 만 쓴다.
- surface 이동. [surface-move](../../features/surface-move/index.md) 가 맡는다.

## 수용 기준

- Given 로컬 explorer 에서 파일 둘과 폴더 하나를 잘라냈다, When 다른 프로그램이 클립보드를 읽는다, Then Linux 에서는 GNOME 형식 첫 줄이 `cut`, KDE 표지가 `1` 이고, `text/uri-list` 와 경로 글자에 세 경로가 있다.
- Given 파일 관리자에서 파일 하나를 잘라냈다, When explorer 빈 영역에서 붙여넣는다, Then 그 파일이 현재 폴더로 옮겨지고 OS 클립보드가 비워진다.
- Given 한 Tasty 창에서 파일을 잘라냈다, When 다른 Tasty 창의 explorer 에 붙여넣는다, Then 파일이 한 번 옮겨지고 OS 클립보드가 비워지며 처음 창의 흐린 표시도 사라진다.
- Given OS 클립보드에 접근하지 못하는 환경에서 한 창이 파일을 복사했다, When 다른 창의 explorer 에서 붙여넣는다, Then 그 파일이 붙여넣어진다.
- Given 한 창의 mirror explorer 에서 파일을 복사했다, When 다른 창의 로컬 explorer 에서 메뉴를 연다, Then 붙여넣기가 없다.
- Given explorer 에서 파일을 복사한 뒤 다른 앱에서 글자를 복사했다, When explorer 에서 붙여넣는다, Then 메뉴에 붙여넣기가 없고 단축키로도 아무 파일도 붙여넣지 않는다.
- Given OS 클립보드 목록에 `sftp://` 항목이 섞였다, When Command Palette 의 Paste 를 실행한다, Then 아무것도 붙여넣지 않고 `explorer.state.os_paste_not_local` 토스트를 띄운다.
- Given explorer 에서 파일을 복사한 뒤 응답하지 않는 다른 프로그램이 클립보드를 가졌다, When Command Palette 의 Paste 를 실행한다, Then 0.5초 남짓 안에 아무것도 붙여넣지 않고 `explorer.state.os_paste_unreadable` 토스트를 띄운다.
- Given 소유자가 INCR 조각을 천천히 보낸다, When explorer 메뉴를 연다, Then 읽기는 500ms 안에 끝나고 붙여넣기는 숨겨진다.
- Given mirror explorer 에서 파일을 복사했다, When 다른 프로그램이 클립보드를 읽는다, Then 그 원격 경로는 클립보드에 없다.
- Given 폴더 A 를 연 탐색기에서 하위 폴더 B 로 이동했고 선택이 없다, When `paste` 를 누른다, Then 붙여넣기 대상은 B 다.
- Given 세 항목을 선택했다, When 그중 하나를 우클릭해 Copy 를 고르거나 `copy` 를 누르거나 Command Palette 에서 Copy 를 고른다, Then 세 경우 모두 클립보드에 같은 세 경로가 담긴다.
- Given 선택 밖의 항목을 우클릭했다, When 메뉴를 연다, Then 선택이 그 항목 하나로 바뀌고 메뉴 대상도 그 항목이다.
- Given 사이드바 Files 트리의 폴더를 우클릭했다, When 메뉴를 연다, Then 목록의 선택은 바뀌지 않는다.
- Given 붙여넣을 위치에 같은 이름의 파일이 있다, When 붙여넣고 충돌 popup 에서 Enter 를 누른다, Then 기존 항목은 그대로이고 새 항목은 `(copy)` 접미사 이름이다.
- Given 같은 이름의 파일 하나와 폴더 하나가 충돌한다, When 남은 충돌에도 적용을 켜고 Replace 를 고른다, Then 파일은 바뀌고 폴더는 다시 묻는다(Skip·Keep both 만).
- Given 충돌 popup 이 떠 있다, When popup 을 닫거나 Esc 를 누른다, Then 남은 항목을 하지 않고 기존 항목은 그대로다.
- Given 복사가 진행 중이다, When 상태줄의 ×를 누른다, Then 목적지에 준비 중 사본이 남지 않고 결과 카드가 취소와 끝난 수를 보인다.
- Given 세 항목 복사 중 첫 항목이 끝나고 둘째 항목의 충돌 질문에서 ×로 취소했다, When 결과 카드의 Retry 를 누른다, Then 둘째·셋째 항목만 다시 요청하고 첫째 항목은 다시 복사하지 않는다(`job_tests` 의 `cancel_records_the_item_it_stopped_and_the_rest_as_not_done`, `ops_tests` 의 취소 카드 시험).
- Given 원본이 남은 항목이 생긴 뒤 그 이동을 취소했다, When 결과 카드의 Retry 를 누른다, Then 남은 원본 삭제는 사본 검사를 거치며 사본이 없으면 원본을 남긴다(`a_cancelled_move_still_retries_its_leftovers_through_the_copy_check`).
- Given 잘라내기·붙여넣기가 모두 끝났다, When 결과 카드의 Undo 를 누른다, Then 항목이 원래 자리로 돌아간다. 원래 자리에 그새 다른 항목이 생겼으면 옮기지 않고 그 이유를 카드에 보인다.
- Given 수정 시각이 오래된 파일과 읽기 전용 하위 폴더가 든 폴더가 있다, When 다른 폴더로 복사한다, Then 사본 파일의 수정 시각과 사본 폴더들의 권한이 원본과 같고, 읽기 전용 폴더 안의 파일도 복사된다. 사본을 고친 뒤 Undo 를 누르면 그 사본은 남는다(`copy_meta_tests.rs`).
- Given 다른 디스크로 옮기는데 원본 폴더 안에 지울 수 없는 하위 폴더가 있다, When 작업이 끝난다, Then 사본은 온전하고 결과 카드는 원본이 남았다는 경고로 닫을 때까지 남으며 Undo 가 없고 Retry 가 있다.
- Given 원본이 남은 이동 카드가 있고 막던 권한을 풀었다, When Retry 를 누른다, Then 사본은 그대로이고 남은 원본만 지워진다.
- Given 원본이 남은 이동 카드가 있고 그 뒤 사본을 지웠다, When Retry 를 누른다, Then 원본은 지워지지 않고 카드가 사본이 없어 원본을 남겼다고 보인다.
- Given 원본이 남은 이동 카드가 있고 사본 경로의 상위 폴더가 원본 폴더로 가는 링크로 바뀌었다, When Retry 를 누른다, Then 원본은 하나도 지워지지 않고 "사본이 원본과 같은 파일이라 남겨 둠" 과 Retry 가 경고 카드에 남는다.
- Given 원본이 남은 이동 카드의 Retry 가 큰 파일을 비교하는 중이다, When 그 원본 파일을 고친다, Then 비교가 끝나도 그 원본은 지워지지 않고 남은 사유가 카드에 보인다.
- Given 원본이 남은 이동 카드 여러 개 항목의 Retry 가 진행 중이다, When 상태줄의 ×를 누른다, Then 이미 지운 원본은 빠지고 남은 원본만 원본이 남은 경고 카드에 남으며 Retry 가 다시 있다.
- Given 원본이 남은 폴더 이동 카드가 있고 그 뒤 원본 폴더에 파일을 넣었다, When 막던 권한을 풀고 Retry 를 누른다, Then 사본과 같은 항목만 지워지고 넣은 파일은 남으며 카드가 남긴 개수를 보이고 Retry 가 다시 있다.
- Given 이동을 되돌린 뒤 원래 자리에 같은 이름의 다른 파일이 생겼다, When `explorer_redo` 를 누른다, Then 아무것도 옮기지 않고 "Can't redo: It changed after the undo" 토스트를 띄우며 단계는 다시 실행 목록에 남는다.
- Given `a.txt` 를 `b.txt` 로 바꿨다, When `explorer_undo` 를 누른다, Then 이름이 `a.txt` 로 돌아가고, `explorer_redo` 를 누르면 다시 `b.txt` 가 된다.
- Given `a.txt` 를 `b.txt` 로 바꾼 뒤 새 `a.txt` 가 생겼다, When 빈 영역 메뉴를 연다, Then Undo 행은 꺼지고 툴팁이 원래 이름에 다른 항목이 있다고 알리며, `explorer_undo` 는 아무것도 바꾸지 않는다.
- Given 새 폴더를 만들었다, When `explorer_undo` 를 누른다, Then 그 폴더는 영구 삭제가 아니라 휴지통으로 간다. 만든 뒤 그 폴더에 파일을 넣었으면 되돌리지 않고 "만든 뒤 바뀌어서 남겨 둠" 으로 알린다.
- Given 두 항목을 옮겼는데 하나만 옮겨지고 하나는 실패했다, When `explorer_undo` 뒤 `explorer_redo` 를 누른다, Then 옮겨졌던 항목만 돌아왔다가 다시 옮겨지고, 실패한 항목은 다시 시도하지 않는다.
- Given 이동이 끝난 뒤 옮겨 간 파일을 고쳤다, When 이력의 Undo 를 부른다, Then 메뉴 행은 꺼지고 단축키는 아무것도 옮기지 않으며, 결과 카드의 Undo 도 그 파일을 "옮긴 뒤 바뀌어서 그 자리에 둠" 으로 남긴다.
- Given 이동이 끝났고 새 복사가 대기 중이다, When 이동 결과 카드의 Undo 를 누르고 두 작업이 차례로 끝난다, Then 항목은 원래 자리로 돌아가고 되돌리기 맨 위는 그 복사이며 다시 실행할 것은 없다.
- Given 이동이 끝났고 새 복사가 대기열에 있다, When `explorer_undo` 를 누른다, Then 아무것도 꺼내지 않고 Info 토스트를 띄우며, 복사가 끝난 뒤 누른 되돌리기는 그 복사를 되돌린다.
- Given 메뉴를 연 뒤 다른 작업이 끝나 이력이 바뀌었다, When 메뉴의 Undo 행을 고른다, Then 아무것도 되돌리지 않고 "The history changed" 토스트를 띄운다.
- Given 복사가 끝난 뒤 사본을 고쳤다, When 결과 카드의 Undo 를 누른다, Then 그 사본은 남고 카드 제목이 "Undid copy · kept 1 changed item"(`explorer.result.undo_kept_copy`)이며 줄에 "복사 뒤 바뀌어서 남겨 둠" 을 보인다.
- Given 되돌리기에서 지우지 못한 사본과 바뀌어 남긴 사본이 함께 있다, When 결과가 나온다, Then 제목은 지우지 못한 수만 센 `explorer.result.undo_partial_copy` 이고 남긴 사본도 줄에 남는다.
- Given 같은 디스크의 항목을 폴더 행으로 끈다, When 놓는다, Then 이동하고, 드래그 반전 modifier 를 누른 채 놓으면 복사한다.
- Given 항목을 끌고 있다, When Esc 를 누른 뒤 놓는다, Then 아무것도 바뀌지 않는다.
- Given 항목을 끌고 있다, When 폴더 위에서 놓는다, Then 그 놓기 작업은 한 번만 요청된다(`view/drag_flow_tests.rs` 의 `a_drop_is_requested_once_even_when_the_frame_runs_again`).
- Given 폴더 `src` 와 그 링크 `alias` 가 있다, When `src` 를 `alias` 나 그 하위 폴더로 끈다, Then "Can't drop" 과 자기 안으로라는 이유를 보이고 놓아도 작업을 요청하지 않는다.
- Given 같은 디스크의 여러 항목을 골랐다, When 다른 폴더로 끌어 놓는다, Then 놓인 내용이 원본과 바이트 단위로 같고, 이동이면 원본이 없어지며 반전 modifier 로 복사하면 원본이 남는다(`a_multi_item_drag_runs_to_the_end_with_the_same_content`).
- Given OS 에서 파일을 끌어 왔다, When 로컬 explorer 칸에 놓는다, Then 그 칸이 보고 있는 폴더로 복사하고 파일 핸들러를 열지 않는다.
- Given 원격 쓰기를 알리지 않는 옛 server 의 mirror explorer 다, When `cut` 또는 `paste` 를 누른다, Then 어느 파일시스템도 바뀌지 않고 그 칸에 "Update Tasty on {host} to change remote files." info 토스트가 뜬다.
- Given 원격 쓰기를 알린 server 의 mirror explorer 다, When 새 폴더를 만들거나 이름을 바꾸거나 휴지통으로 보낸다, Then 원격 호스트의 파일이 바뀌고 같은 연결의 explorer 가 그 폴더를 다시 읽으며 로컬 파일시스템은 바뀌지 않는다.
- Given 원격 붙여넣기가 이름 충돌로 멈췄다, When 충돌 popup 에서 답한다, Then server 의 작업이 그 답으로 이어지고 결과 카드가 이 칸에 뜬다.
- Given 원격 작업이 실행 중이다, When 연결이 끊긴다, Then server 는 작업을 멈추고 이 칸에는 결과를 모른다는 토스트가 뜬다.
- Given 로컬 explorer 에서 항목을 복사했다, When mirror explorer 에서 `paste` 를 누르거나 그 항목을 mirror explorer 로 끈다, Then 아무것도 바뀌지 않고 다른 컴퓨터라는 이유를 보인다.
- Given 두 윈도우에서 같은 폴더를 연 로컬 explorer 가 있다, When 한쪽에서 항목을 휴지통으로 보낸다, Then 두 explorer 모두 그 폴더를 다시 읽어 항목이 사라진다.
- Given 한 explorer 가 폴더 A 안을 보고 있다, When 다른 explorer 에서 A 를 휴지통으로 보낸다, Then 앞의 explorer 는 읽기 오류 화면을 보이고 경로는 A 안 그대로다.
- Given 붙여넣기가 진행 중이다, When 사용자가 다른 폴더로 이동한다, Then 작업 대상은 요청 시점의 폴더이고 포커스는 바뀌지 않는다.
- Given mirror explorer 에서 항목을 복사했다, When 로컬 explorer 에서 우클릭하거나 `paste` 를 누른다, Then 메뉴에 붙여넣기가 없고 단축키는 원격 붙여넣기 미지원 토스트만 띄우며 로컬 파일시스템은 바뀌지 않는다.
- Given 컨텍스트 메뉴가 열려 있다, When 그 사이 surface 가 닫히거나 다른 surface 로 바뀐 뒤 항목을 고른다, Then 아무 작업도 실행하지 않는다. 경로 복사·복사·이 폴더로 루트 설정은 mirror projection 이 다시 만들어지기만 했으면 그대로 실행한다.
- Given rename 팝업이 떠 있다, When 그 사이 surface 가 다른 surface 로 바뀐다, Then 팝업이 닫히고 확정해도 이름 변경 요청이 나가지 않는다.
- Given 새 이름 입력을 확정했다, When worker 가 만들기 전에 같은 이름이 생겼다, Then 기존 항목은 그대로이고 오류 토스트가 뜬다(`explorer_files/tests.rs` 의 `create_makes_a_folder_or_an_empty_file_and_never_replaces_an_entry`).
- Given 하위 폴더 찾기 결과에서 폴더와 그 안의 파일을 함께 골랐다, When 복사·이동·휴지통 이동을 한다, Then 폴더만 한 번 처리되고 안쪽 파일의 사본이 목적지 맨 위에 따로 생기거나 없는 항목으로 실패하지 않는다(`job_tests.rs` 의 `a_folder_and_its_own_entries_are_transferred_once`·`trashing_a_folder_with_its_own_entries_sends_the_folder_once`).
- Given 복사 중 준비용 임시 폴더를 지울 수 없게 됐다, When 작업이 끝난다, Then 경고 로그가 남고 결과 카드가 Warning 으로 올라 제목은 항목 결과 그대로, 항목 줄 뒤에 그 임시 폴더 경로와 "Temporary folder couldn't be removed (OS 오류)" 줄을 보이며 닫을 때까지 남는다(`job_tests.rs` 의 `a_staging_folder_that_cannot_be_removed_is_reported`, `ops_tests.rs` 의 `a_staging_folder_left_behind_raises_the_card_and_lists_its_path`).
- Given 남은 임시 폴더와 실패 항목이 함께 있다, When 결과 카드가 뜬다, Then 폴더 줄은 실패 줄 뒤에 오고 Retry 는 실패 항목만 세며 Danger 카드는 Danger 로 남는다(`ops_tests.rs` 의 `a_staging_folder_line_comes_after_the_item_lines_and_counts_toward_more`, `a_staging_folder_raises_info_and_success_but_keeps_danger`).
- Given 한 칸에서 새 폴더 → 이름 변경 → 이동 → 복사를 했다, When 되돌리기를 네 번 한다, Then 복사·이동·이름 변경·새 폴더 순으로 하나씩 되돌아가 처음 모습이 되고, 복사본과 새 폴더는 휴지통에 있으며 다시 실행 목록에 네 단계가 역순으로 오른다(`history_steps_tests.rs` 의 `create_rename_move_copy_are_undone_in_reverse_order`, Linux 격리 휴지통).

### 파일 작업 시험의 파일시스템 전제

목록 열기 시험은 표시 문자열의 경로 구분자 대신 액션의 경로를 비교한다. 대소문자만 다른 이름 변경 시험은 대소문자 구분 파일시스템에서만 하드 링크로 이름 별칭을 만들며, 비구분 파일시스템에서는 실제 별칭을 사용한다. 두 이름이 서로 다른 파일을 가리키는 추가 대조는 구분 파일시스템에서만 실행한다. 임시 복사 폴더의 정리 실패 시험은 목적지를 canonicalize한 뒤 결과 경로와 비교한다. 이 시험들의 통과는 각 OS의 네이티브 메뉴·입력·드래그 동작 확인을 대신하지 않는다.

- Given 복사 쓰기 중 디스크 공간 부족 오류가 발생한다, When 실패한 항목의 준비 중 사본을 정리한다, Then 원본과 기존 목적지 파일은 유지되고 부분 사본은 공개하지 않는다. 먼저 끝난 항목은 성공으로 남고 실패한 항목만 재시도 대상이다(`storage_failure_tests.rs`). 시험은 작업별 쓰기 실패를 주입하며 실제 디스크를 가득 채우지 않는다.

- Given App 종료 중 Explorer·이미지·로컬 읽기·DAG 읽기 worker가 남아 있다, When 기존 종료 시한에 도달한다, Then 더 기다리지 않고 시한을 지우며 미회수 경고를 한 번 남긴다. 남은 worker를 join한 것으로 바꾸지는 않는다. `explorer_shutdown_wait_tests.rs`는 App이 위임하는 같은 시한 판정에 현재 시각과 남은 수를 주입한다. 실제 worker 회수와 취소 뒤 임시 사본 정리는 `explorer_files/tests.rs`에서 별도로 검사한다.
