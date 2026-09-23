# Explorer (내장 파일 관리자 surface)

- **Status**: Implemented
- **주체**: 로컬 사용자 · AI Agent ([주체](../../concepts/actors.md))
- **ADR**: [ADR-0059](../../adr/0059-explorer-remote-attach-list-dir-reuse-browse-only.md) (원격 attach mirror 브라우징 — list_dir 채널 재사용 + browse-only)
- **코드**: surface kind 등록 `register_explorer` (`src/core/surface_registry/builtins.rs`), 모델 `ExplorerPanel`/`ExplorerTab` (`crates/tasty-model/src/explorer_panel.rs`), 뷰 스토어 `ExplorerView`/`ExplorerViewStore` (`src/adapters/ui/surface/explorer/view.rs`), 렌더 `draw_explorer` (`src/adapters/ui/surface/explorer.rs`), deferred action 적용 `apply_explorer_action` (`src/adapters/ui/egui_panels.rs`)
- **화면**: host 내장 egui surface (터미널과 동궤의 surface 타입, T11 host builtin)

## 목적

OS 파일 관리자에 의존하지 않고 tasty surface 안에서 디렉토리를 탐색하고 파일을 열기 위한 내장 파일 관리자다. 다른 surface(terminal/markdown/image)와 동일하게 pane/tab 레이아웃에 들어가고, surface 변환·이동·레이아웃 영속화 대상이 된다. (과거 `com.tasty.explorer` plugin 이 제공하던 기능을 본체 host builtin surface 로 승격한 것 — surface kind `"explorer"` 는 부팅 시 `register_builtin_kinds` 가 등록한다.)

## 내부 동작 (headless-valid)

### 모델 (`ExplorerPanel`)

- `ExplorerPanel` 은 식별(`id`)과 내비게이션 상태만 보유한다 — 내부 탭 목록(`tabs`)·활성 탭 인덱스(`active`). 각 `ExplorerTab` 은 **cwd(고정 루트)** 와 **current(현재 폴더, 필드명 `root`)** 를 분리해 보유하고, 히스토리(back/forward 스택), 뷰 모드(`view_mode`), 정렬 컬럼/방향(`sort_column`/`sort_dir`)을 가진다.
- **cwd ↔ current 분리** (VS Code 식 "고정 프로젝트 + 자유 탐색"): `cwd()` 는 explorer 를 연 프로젝트 루트로 **좌측 사이드바 트리 루트**·**스폰 cwd**(`source_cwd()`)·**surface/탭 표시명**의 기준이며 내비게이션에 불변. `current()`(=`current_root()`) 는 **우측 목록**·**상단 주소창(편집형 PathField)** 이 따라가는 탐색 폴더로, back/forward/go_up 이 이것만 움직인다. current 는 cwd 하위로 제한되지 않고 파일시스템 어디로든 자유 이동한다.
- 내비게이션: `navigate_to(dir)` / `go_back` / `go_forward` / `go_up` — 모두 **current 에만** 작용. `can_go_up` 은 current 의 파일시스템 부모 존재만 본다(cwd 경계로 clamp 안 함). `set_cwd(folder)` 는 cwd·current 를 folder 로 재설정하고 히스토리를 비운다(explorer-03 "이 폴더로 루트 설정"). 히스토리는 탭별로 독립.
- **`..` 상위 이동**: current 에 부모가 있으면(파일시스템 루트 아님) 우측 목록 최상단에 `..` 특수 행을 그려 상위 폴더로 이동한다. `..` 는 **렌더 전용**이라 `view.entries`/선택/상태줄/컨텍스트 메뉴 대상이 아니며 더블클릭 시 `Navigate(parent)` 만 emit 한다.
- 내부 탭: `add_tab`(활성 탭 cwd 복제, current=cwd) / `close_tab(idx)` / `active_tab[_mut]`. surface 하나 안에 여러 디렉토리 탭을 둔다 (상위 pane 탭과 별개). 탭별 cwd 는 독립(per-tab).

### 뷰 상태 (`ExplorerView`, surface id 로 keying)

디렉토리 엔트리 캐시·선택 집합·트리 펼침 같은 무거운 GUI 상태는 모델이 아니라 per-surface 뷰 스토어에 둔다.

- **엔트리 캐시**: `sync(panel)` 이 활성 탭의 `(root, sort_column, sort_dir)` 키를 보고 디렉토리/정렬이 바뀌었거나 새로고침이 요청됐을 때만 디스크에서 다시 읽는다. 디렉토리가 바뀌면 선택을 초기화한다. 읽기 실패는 `LoadState::NoPermission`(권한 거부) / `LoadState::Error(msg)` 로 분류해 콘텐츠 중앙 상태 텍스트로 표현한다.
- **주소창 편집 상태**: `addr_buffer`(편집 텍스트) / `addr_editing`(포커스=편집모드) / `addr_active`(후보 드롭다운 keyboard-active 행)를 뷰가 소유한다(PathField 계약 — 상태는 호출측 소유). `sync()` 는 **비편집 시** 버퍼를 활성 탭 current(`root`) 로 재동기화하고, 편집 중이면 사용자 입력을 보존한다. 내부 탭은 surface 단위 `ExplorerView` 를 공유하므로, cwd/내부 탭을 바꾸는 액션(`Navigate/GoBack/GoForward/GoUp/NewTab/CloseTab/SelectTab`) 적용 시 `cancel_addr_edit()` 로 편집을 취소해 버퍼가 다른 탭/경로로 새지 않게 하고(다음 `sync()` 가 새 current 로 맞춘다), id_salt 는 surface+내부탭 index 로 고유화한다.
- **선택**: `selected: HashSet<PathBuf>` + `anchor`(shift 범위 기준). `select_all()` 은 현재 디렉토리 전체를 선택, `selected_paths_text()` 는 선택 경로를 정렬·개행 결합한 클립보드 페이로드를 만든다.
- **사이드바 트리**: `expanded` 펼침 집합 + `tree_children` lazy 하위 디렉토리 캐시. 폭 196(design `ExpSidebar`). 사이드바는 **2-region 고정 분할**이다 — 상단 **Files**(트리, cwd 루트 고정)는 사이드바 본문 남는 공간 전부를 차지하며 자체 스크롤되고, 하단 **Favorites**는 계산된 고정 높이 영역에서 독립적으로 스크롤된다(Files 를 아무리 스크롤해도 Favorites 위치는 움직이지 않고, 반대도 마찬가지). 두 영역 사이 1px 구분선은 **하단 고정 영역의 상단 경계**에 고정 좌표로 그려진다 — 트리 길이와 무관하며, 트리가 짧아도 그 위 빈 공간은 배경만 남고 구분선이 따라 올라오지 않는다. 트리에서 **현재 폴더(current)** 노드는 surface-active 배경 + text-primary 로 하이라이트되고, 폴더 아이콘은 text-muted. 섹션 캡션은 monospace·micro·uppercase(design `SideHead`).
- **Favorites 고정 높이 계산**(design `favPinHeight`): 사이드바 본문 높이가 600px 이상이면 240px 고정. 600px 미만이면 `round(본문높이 × 0.4 / 4) × 4`(4px 그리드 스냅)와 120px(하한) 중 큰 값. 임계값 전환은 보간 없는 하드 전환이다. 본체 구현은 `favorites_pin_height`(`src/adapters/ui/surface/explorer.rs`).

### 뷰 모드 / 정렬

- 뷰 모드 3 종(grid / list / detail)을 toolbar 우측의 **아이콘 view-mode 토글**(`seg_toggle`, design `SegToggle`)로 전환한다 — grid/list/detail 아이콘 세그먼트, active = surface-active 배경 + text-primary. detail 뷰는 정렬 컬럼 헤더를 클릭하면 해당 컬럼으로 정렬(같은 컬럼 재클릭 시 방향 토글).
- toolbar 의 **주소표시줄**(`address_bar`, design `ExpToolbar`/`PathField`)은 공용 **편집형 `PathField`** 다 — folderOpen leading 아이콘 + mono 경로(비편집=text-secondary / 편집=text-primary) + 우측 Go(arrow-right) 버튼(input-bg/input-border(-focus) 토큰). 클릭하면 편집 모드로 들어가 임의 디렉토리 경로를 타이핑하고 `↵` 또는 Go 로 **current 이동**한다(존재하는 디렉토리만 — `navigate_target` 가 `exists() && is_dir()` 를 통과해야 `ExplorerAction::Navigate` emit, 파일/오타는 no-op). `Esc` 또는 확정 없는 포커스 이탈은 현재 current 로 원복. 과거 breadcrumb(조각 클릭 상위 점프)는 폐기 — 대체는 Back/Forward/Up + 사이드바 트리 + 타이핑 이동. 편집 진입 시 **최근 방문 디렉토리** 자동완성 후보 드롭다운이 뜨고(타이핑에 맞춰 substring 필터), 이 후보는 `RecentFiles` 의 `"directory"` kind(markdown 의 파일 recent 와 대칭·영속)에서 온다 — 사용자가 `ExplorerAction::Navigate` 로 이동 확정한 cwd 를 host 가 kind 로 적재(`egui_panels`), draw 경계로 slice 주입. 주소표시줄 flex:1 / 토글 flex:none.
- **마지막 view mode 기억**: 사용자가 뷰 모드를 바꾸면 그 값이 `Settings.general.explorer_view_mode`(`~/.tasty/config.toml`)에 영속되고, **새로 생성되는** explorer surface 는 이 값으로 열린다(주입 지점: `create_surface_via_registry` 가 explorer 의 `default_params` `view_mode = "@settings.explorer_view_mode"` 정책 토큰을 `view_mode` param 미지정 시 해석해 explorer `create` 에 실어 전달 — kind별 default_params 는 [plugin-development.md](../../dev-guide/plugin-development.md) 참조). 같은 surface 안의 새 내부 탭(`add_tab`)은 활성 탭의 view mode 를 승계한다. snapshot 복원 경로는 create 를 거치지 않아 per-tab 저장값을 그대로 유지한다.
- list/detail 데이터 행은 공용 `Table`(selectable)을, 사이드바 디렉토리 행은 공용 `tree_row` 를 재사용한다. detail 컬럼은 Name(1fr)/Size(80)/Date(132)/Type(92)이며, Size·Date 는 **monospace·caption(11)·text-muted**, Size 는 우측 정렬 + 8px 우측 패딩으로 Date 와 시각적 간격을 둔다(design `DetailRow`).
- detail 행은 **행 전체가 클릭 타겟**이다 — 파일 이름·Size·Date·Type 글자 위에서도 좌클릭 선택 / Ctrl·Cmd+클릭 토글 / 더블클릭 열기(Navigate·OpenFile) / 우클릭 컨텍스트 메뉴가 동일하게 동작한다. 그 대가로 셀 텍스트를 드래그로 선택·복사할 수는 없다(대체: 우클릭 "경로 복사"). 이 정합은 공용 `Table` 이 selectable 모드에서 셀 라벨 선택성을 끄는 계약으로 보장한다 — [ADR-0069](../../adr/0069-table-row-click-over-cell-text-selection.md). 헤더 컬럼 제목 클릭(정렬 토글)은 영향을 받지 않는다.

### deferred action 적용

렌더 중 발생한 사용자 상호작용은 `ExplorerAction`(OpenFile / Navigate / GoBack / GoForward / GoUp / Refresh / SetViewMode / SetSort / NewTab / CloseTab / SelectTab / ContextMenu) 으로 모았다가 `apply_explorer_action(state, engine, sid, act)` 에서 적용한다. 파일 열기/새로고침은 뷰 스토어만, 내비게이션·뷰모드·탭 조작은 **origin surface id 로 직접 지정**한 `ExplorerPanel` 을 가변 차용해 처리한다(포커스 독립). 경로가 바뀌면 `ExplorerView` 가 다음 draw 에서 자동 감지해 재로드한다.

- 파일 열기는 `DomainIntent::DispatchFile { origin_surface_id: Some(sid) }` 로 [file-handler](../file-handler/index.md) 에 위임한다 — explorer 자신은 파일 식별/디스패치 정책을 모른다.

### 컨텍스트 메뉴 · 파일 조작

우클릭 컨텍스트 메뉴는 **2-단계 네이티브 메뉴 패턴**([context-menu](../../dev-guide/context-menu.md))을 따른다: 렌더 중 우클릭을 감지하면 `ExplorerAction::ContextMenu { target, cwd, x, y }` 를 모으고, `apply_explorer_action` 이 이를 `PendingNativeMenu::Explorer`/`ExplorerFavorite` 슬롯에 선점한다. 비-terminal 컨텍스트 메뉴는 winit 이 만들지 않고 egui 프레임이 단일 생산자다 — explorer 메뉴는 같은 egui 프레임 안에서 `apply_explorer_action`(렌더 루프 종료 직후)이 generic surface fallback(`emit_surface_menu_fallback`)보다 **먼저** 슬롯을 선점하므로, fallback 은 `is_none()` 가드로 이를 건너뛰고 explorer 전용 메뉴가 이긴다. 이후 `MainView::process_pending_native_menu` 가 `open_native_menu` 로 OS 네이티브 메뉴를 띄우고, 선택 id 를 조작으로 번역하는 처리는 continuation 으로 예약된다(Linux 는 메뉴가 닫힌 뒤 프레임에 실행 — [context-menu](../../dev-guide/context-menu.md) · [ADR-0071](../../adr/0071-native-context-menu-async-contract.md)).

대상(target)은 우클릭 위치/선택 상태로 결정한다(design §3.3 target rule): 선택 안의 항목 → 선택 전체, 선택 밖 → 그 항목으로 선택 리셋, 빈 영역 → cwd. variant 4종(빈 영역 / 파일 / 폴더 / 다중). 좌측 사이드바 트리 폴더 우클릭도 **단일 폴더 target 을 직접 구성**해(선택집합 미조작) 동일 메뉴를 띄운다.

**표면 전체 커버리지**: 위 위치별 핸들러가 처리하지 못한 우클릭(툴바/주소창/내부 탭바/상태줄/빈 사이드바 등 chrome 영역)은 `draw_explorer` 끝의 **표면 전체 rect catch-all** 이 `Empty`(cwd) target 으로 흡수한다. 하위 위젯이 이미 `action` 을 세웠으면 건너뛰므로 파일/폴더/다중 메뉴는 그대로 이긴다. 이로써 generic surface fallback("터미널 ID 복사")이 explorer 표면 어디에서도 뜨지 않는다(불가침 원칙 §1·§2). 예외: 권한 거부 루트(`LoadState::NoPermission`)는 붙여넣기가 무의미하므로 catch-all 을 건너뛴다(content 빈영역 규칙과 동일).

- **경로 복사** (`copy_path`, 다중은 개행 결합) → OS 텍스트 클립보드 + `toast.copied_path` 토스트(단축키/Command Palette/우클릭 메뉴 모두 동일).
- **복사 / 잘라내기 / 붙여넣기** — explorer 내부 파일 클립보드(`CoreState::explorer_clipboard`, 단일 슬롯·세션 휘발)에 경로+cut 플래그를 담고, 붙여넣기에서 소비한다. 실제 파일 이동은 `explorer/ops.rs`(순수 fs 헬퍼 — 충돌 시 `(copy)` 접미사, 자기 자신/하위로 붙여넣기 거부, cut 의 cross-volume 은 copy+remove 폴백). 잘라내기는 이동 성공 시 클립보드를 비운다. 우클릭 메뉴뿐 아니라 키보드 단축키(기본 `copy`/`cut`/`paste` 바인딩, explorer 포커스 시)로도 동일하게 동작한다 — `handle_explorer_shortcut`(`src/adapters/ui/input/shortcuts/copy_paste.rs`)가 선택 항목을 모아 컨텍스트 메뉴와 같은 `explorer_menu_set_clipboard`/`explorer_menu_paste` 를 호출하므로 fs 동작이 두 경로에서 갈라지지 않는다. 붙여넣기 대상은 현재 디렉토리(cwd) 고정(선택된 폴더 안으로의 paste-into 는 컨텍스트 메뉴 전용). **복사(cut=false)** 는 fs 접근이 없어 mirror explorer 에서도 그대로 동작하지만, **잘라내기(cut=true)/붙여넣기**는 mirror 에서 메뉴·단축키 모두 차단된다(아래 "mirror(attach) explorer 의 browse-only 강제" 참고).
- **휴지통으로 이동** (`delete`) — `trash` 크레이트로 OS 휴지통에 보낸다(가역적이라 확인 모달 없음). mirror 에서 차단.
- **이름 변경** (`rename`, 단일만) — 공용 rename 팝업(`PopupDef`)을 재사용해 `std::fs::rename`. mirror 에서 차단(가드가 먼저 막아 팝업 자체가 열리지 않는다).
- **OS 기본 앱으로 열기** (`open_in_system`, 단일 폴더만) — `platform::reveal::open_path`(Windows `explorer` / macOS `open` / Linux `xdg-open`). mirror 에서 차단.
- **즐겨찾기 추가** (`add_to_favorites`, 단일 폴더 또는 빈 영역) — 아래 참조. mirror 에서 차단.
- **새 탭으로 열기** (`open_in_new_tab`, 단일 폴더) — 그 폴더를 cwd 로 하는 새 explorer 를 **Pane 탭**(explorer 내부 탭이 아님)으로 연다. 우클릭 대상 surface 의 **소유 pane** 에 추가해(`AppState::add_kind_tab_by_owner`) focused pane 이 아니어도 올바른 pane 에 열린다. 기존 explorer 는 불변. mirror 에서 메뉴 자체가 숨겨지고 클릭 시에도 차단된다(아래 참고 — `add_kind_tab_by_owner` 는 mirror 구조 변경 forward 를 거치지 않는다).
- **이 폴더로 루트 설정** (`set_as_root`, 단일 폴더) — **현재 explorer** 의 cwd 를 그 폴더로 이동한다(`AppState::set_explorer_cwd` → `ExplorerTab::set_cwd`: 좌측 트리 루트·current 이동 + 히스토리 초기화 + 뷰 리로드). 순수 로컬 뷰 상태 이동이라 mirror 에서도 그대로 동작.

### mirror(attach) explorer 의 browse-only 강제

ADR-0059 Decision 2("mirror explorer 는 browse-only — rename/delete/새폴더 등은 스코프 밖")는 파일 더블클릭 열기(`OpenFile`)뿐 아니라 컨텍스트 메뉴·키보드 단축키 레벨까지 강제된다. mirror 워크스페이스(`ws.mirror`)에 속한 explorer surface 에서는:

- **컨텍스트 메뉴에서부터 숨김**: 붙여넣기/잘라내기/이름 변경/휴지통으로 이동/시스템에서 열기/새 탭으로 열기 항목이 `build_explorer_context_menu`(즐겨찾기 행은 `handle_explorer_favorite_native_menu`)에서 아예 노출되지 않는다. copy_path/복사/즐겨찾기 추가/이 폴더로 루트 설정은 그대로 노출된다.
- **액션별 개별 가드**: 메뉴가 아닌 다른 경로(키보드 단축키 등)로 같은 핸들러가 호출되는 경우를 방어하기 위해, 각 핸들러(`explorer_menu_paste`/`_trash`/`_rename`/`_open_in_system`/`_add_favorite`/`_open_in_new_tab`, `explorer_menu_set_clipboard`의 `cut=true`)가 진입부에서 `CoreState::is_mirror_surface(surface_id)` 로 재확인하고, mirror 면 로컬 fs 를 건드리지 않고 `explorer.state.remote_write_unsupported` toast 로 안내한 뒤 반환한다.
- **rename 팝업의 `path.exists()` 게이트**(`draw_rename_popup`, `src/adapters/ui/dialog.rs`)는 위 가드가 먼저 막기 때문에 mirror 경로에서는 도달하지 않는다 — 이 게이트는 로컬(비-mirror) 시나리오에서 대상이 그 사이 사라진 경우를 위한 안전장치로만 남는다.
- **즐겨찾기**: `~/.tasty/explorer-favorites.toml` 는 surface/host 무관 전역 저장소다. mirror explorer 의 경로(원격 호스트 경로)가 이 전역 목록에 섞이면 로컬/다른 호스트 explorer 의 사이드바를 오염시키므로, 즐겨찾기 추가는 mirror 에서 팝업을 열기 전에 차단된다.
- **새 탭으로 열기가 차단되는 이유**: `AppState::add_kind_tab_by_owner`(`src/state/tab.rs`)는 `add_tab`/`add_kind_tab`과 달리 mirror 구조 변경을 원격으로 forward하는 `forward_mirror_structural` 을 거치지 않고 로컬 pane 을 직접 mutate한다. mirror 트리 동기화(`apply_mirror_structural_delta`, `src/app/attach_client.rs`)는 원격 authoritative 트리 기준 전체 재구성이므로, 이렇게 로컬에서만 생긴 탭은 원격 트리에 없어 다음 구조 델타 수신 시 제거된다 — 즉 조용히 유령 tab 이 생겼다 사라지는 혼란스러운 UX가 된다. 정확한 forward 지원(owner surface 를 anchor 로 한 `StructuralOp::NewTab` forward)은 별도 작업으로 남아 있다.

### 즐겨찾기 (favorites)

전역(surface 무관)·영속 즐겨찾기 — **로컬 client 파일시스템 경로 전용**. `~/.tasty/explorer-favorites.toml`(`[[favorite]]` 배열, label+path)에 저장되며 부팅 시 `CoreState::explorer_favorites`(`ExplorerFavorites`)로 로드된다. 메모리 mutator(`add`/`remove`)는 순수하고 디스크 반영은 호출처가 `save()` 로 한다(테스트가 디스크를 건드리지 않게 분리). mirror(attach 원격 점유) explorer 의 경로는 원격 호스트의 경로라 이 전역 목록에 섞일 수 없다 — "즐겨찾기 추가"는 mirror explorer 에서 차단된다(위 "mirror(attach) explorer 의 browse-only 강제" 참고).

- **추가**: 컨텍스트 메뉴 "Add to favorites" → rename 팝업과 동일 골격의 입력 팝업(`RenameTarget::ExplorerAddFavorite`, 확정 버튼 라벨만 "Add")으로 라벨을 받아 등록(같은 경로 재등록 시 라벨만 갱신).
- **표시/이동**: 사이드바 하단 고정(pin) "Favorites" 영역(캡션 **항상 표시**)에 **채운 별(STAR_FILL) + accent-warning(골드)** 행으로 나열, 클릭 시 해당 경로로 이동. 현재 폴더인 즐겨찾기는 surface-active 하이라이트. 이 영역은 위 "사이드바 트리"에 서술한 고정 높이로 항상 화면에 보이며 자체 스크롤된다.
- **빈 상태(empty state)**: 즐겨찾기가 0개여도 섹션이 사라지지 않는다(발견성) — 흐린 별(opacity 0.55) + `explorer.sidebar.favorites_empty`("No favorites yet") + 우클릭 힌트(`favorites_empty_hint`, "Add to favorites" 스팬만 text-muted, 나머지 text-placeholder)를 표시한다(design `FavoritesEmpty`).
- **제거/열기/루트 설정**: 즐겨찾기 행 우클릭 → `PendingNativeMenu::ExplorerFavorite`(우클릭 explorer 의 `surface_id` 동봉) → "새 탭으로 열기" / "이 폴더로 루트 설정" / "즐겨찾기에서 제거". 제거는 전역이라 경로만으로 하지만, "루트 설정" 은 `surface_id` 로 대상 explorer 를 지정한다.
- 즐겨찾기 목록은 프레임당 1회 스냅샷으로 `draw_explorer` 에 전달된다(렌더 루프에서 `engine` 가변 차용 충돌 회피).

## 인터페이스

### AI Agent (IPC/CLI)

explorer 는 일반 surface 생성 메커니즘으로 다룬다 (전용 IPC 추가 없이 generic 경로):

- 생성: `tasty new tab --type explorer [--path <dir>]` / `tasty new workspace --type explorer [--path <dir>]`. `--path` 미지정 시 새 탭은 explorer `default_params` 의 `path = "@home"` 로 home 이 주입된다(fresh-context). (IPC: `DomainIntent::CreateTab { kind: "explorer", surface_params }`.)
- **root 결정 규칙**: `path` param → carry cwd → `$HOME`/`%USERPROFILE%` → (홈 조회 실패 시) 절대경로로 확정한 프로세스 cwd. 앞 두 단계의 값이 **상대경로면 채택하지 않고** 홈으로 내려간다 — explorer root 는 어떤 생성 경로(`split`/`new tab`/`new workspace`/convert)에서도 **항상 절대경로**다. 상대 root 는 프로세스 cwd 를 root 로 승격시키고 그 문자열이 주소창·경로 복사·attach `list_dir` wire 로 새어나가기 때문이다. `"."` 로 저장된 구 `layout.json` 스냅샷도 복원 시 홈으로 교정된다. 근거·강제 수단: [surface cwd 불변식 §5](../../design/policies/cwd.md#5-explorer-root-fallback-host-builtin).
- 조회/닫기: `tasty list surfaces` 에 `foreground_process`/`pane_id`/`workspace_id` 와 함께 나타나고, `tasty close ...` 로 닫는다 — 전 워크스페이스 순회·ID 직접 지정(포커스 독립).
- 변환: 다른 surface 를 explorer 로 in-place 변환 — `Intent::ConvertSurface { surface_id, target: ConvertTarget::Kind { kind: "explorer", .. } }`. cwd 미지정 시 source surface 에서 carry. [convert-surface](../convert-surface/index.md) 의 generic convert popup 도 registry kind 열거로 explorer 를 노출한다.

### 사용자 트리거 (단축키 — [KeybindingSettings](../keybindings/index.md))

모든 단축키는 `KeybindingSettings` 로 노출되며 하드코딩하지 않는다. `convert_to_explorer` 외에는 explorer 포커스에서만 동작:

| 액션 | 필드 | Tasty 프리셋 기본 |
|------|------|------|
| 새로고침 | `explorer_refresh` | `F5` |
| 상위 폴더로 | `explorer_go_up` | `Alt+Up` |
| 전체 선택 | `select_all` | `Ctrl+A` / `Alt+A` |
| 경로 복사 | `copy_path` | `Alt+Shift+C` |
| explorer 로 변환 | `convert_to_explorer` | (기본 미할당) |

직접 키 매칭은 `explorer_refresh`·`explorer_go_up`·`convert_to_explorer`(포커스 surface 무관) 가 `keybinding.rs`, `select_all`·`copy_path` 가 `copy_paste.rs` 다. action-id/Command Palette `dispatch.rs` 는 다섯 모두를, 더블탭 `double_tap.rs` 는 `convert_to_explorer` 만 받는다. 설정 UI 서브탭은 `explorer_refresh`·`explorer_go_up` = **Explorer**, `select_all`·`copy_path` = **Clipboard**, `convert_to_explorer` = **Surface**.

**새 탭으로 탐색기 열기(`open_explorer`, 기본 미할당)는 포커스와 무관하다** — 위 표와 달리 explorer 포커스를 요구하지 않는다. `Intent::NewTab { kind: "explorer" }` 를 발화하므로 CLI 의 `new tab --type explorer` 와 같은 도메인 인텐트(`CreateTab`)를 쓰되 선택은 다르다 — 단축키는 새 탭을 선택하고, 에이전트(CLI/IPC)는 선택하지 않는다([ADR-0502](../../adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md)). 이 액션은 경로를 안 실으므로 홈에서 열린다(명시 경로는 CLI 의 `--path` 가 받는다). 설정 UI 는 **Tab** 서브탭이다 — `open_markdown` 옆, 둘 다 새 탭 열기라서. 이 액션은 `keybinding.rs`·`double_tap.rs`·`dispatch.rs` 세 진입점 전부에 배선돼 있다.

### 폰트

Appearance → **Explorer** 서브탭에서 surface 폰트를 오버라이드한다 (`appearance.plugin_font_overrides["explorer"]`, `effective_font_for_kind("explorer")` 가 읽음).

## 비-목표 (Out of scope)

- 파일 식별/렌더 정책 — explorer 는 열기를 [file-handler](../file-handler/index.md) 에 위임한다.
- 컨텍스트 메뉴 파일 조작(복사/잘라내기/붙여넣기/이름변경)은 **에이전트(IPC/CLI) 노출 대상이 아니다** — 사용자 우클릭 조작 전용. surface 단위 이동은 [surface-move](../surface-move/index.md) 가 별도 제공한다.

## 관련

- [work-area](../work-area/index.md)(Surface/Tab/Pane 계층) · [file-handler](../file-handler/index.md)(파일 열기 위임) · [convert-surface](../convert-surface/index.md)(explorer 로/에서 변환) · [keybindings](../keybindings/index.md) · [settings](../settings/index.md)(폰트/단축키 탭)
