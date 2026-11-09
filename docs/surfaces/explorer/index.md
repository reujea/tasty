# Explorer (내장 파일 관리자 surface)

- **Status**: Partial — 탐색·파일 작업은 구현돼 있으나 `DISPLAY`가 없는 네이티브 Wayland의 메뉴 미지원과 조합 종료 키 전달 결함, 밖에서 들어온 포인터의 파일 놓기 한계가 남아 있다. OS별 실제 확인 범위와 한계는 [파일 작업 Status](file-operations.md#확인된-linux-범위와-한계)를 따른다.
- **주체**: 로컬 사용자 · AI Agent ([주체](../../concepts/actors.md))
- **설계**: [원격 attach (Remote attach)](../../features/remote-attach/index.md) (원격 attach mirror 브라우징 — list_dir 채널 재사용, 파일 변경은 원격 쓰기를 알린 server 가 실행, 더블클릭은 원격 탭 열기)
- **코드**: surface kind 등록 `register_explorer` (`src/runtime/surface_registry/builtins.rs`), 모델 `ExplorerPanel`/`ExplorerTab` (`crates/tasty-model/src/explorer_panel.rs`), 뷰 스토어 `ExplorerView`/`ExplorerViewStore` (`src/adapters/ui/surface/explorer/view.rs`), 렌더 `draw_explorer` (`src/adapters/ui/surface/explorer.rs`), deferred action 적용 `apply_explorer_action` (`src/adapters/ui/egui_panels.rs`)
- **화면**: 호스트 내장 egui surface

## 목적·지원 범위

OS 파일 관리자에 의존하지 않고 tasty surface 안에서 디렉토리를 탐색하고 파일을 열기 위한 내장 파일 관리자다. 다른 surface(terminal/markdown/image)와 동일하게 pane/tab 레이아웃에 들어가고, surface 변환·이동·레이아웃 영속화 대상이 된다. surface kind `"explorer"`는 부팅 시 `register_builtin_kinds`가 등록한다.

지원하지 않는 범위:

- 파일 식별/렌더 정책 — explorer 는 열기를 [file-handler](../../features/file-handler/index.md) 에 위임한다.
- 컨텍스트 메뉴 파일 조작(복사/잘라내기/붙여넣기/이름변경)은 **에이전트(IPC/CLI) 노출 대상이 아니다** — 사용자 우클릭 조작 전용. surface 단위 이동은 [surface-move](../../features/surface-move/index.md) 가 별도 제공한다.

## 상태 소유자

| 상태 | 소유자 | 바꾸는 경로 |
|---|---|---|
| 구조 트리의 leaf(`SurfaceDescriptor`, kind `explorer`) | engine 의 구조 트리 | 공통 생성·닫기·이동·변환([작업 영역](../../features/work-area/index.md)) |
| 내비게이션 모델 `ExplorerPanel`(내부 탭·cwd·current·히스토리·뷰 모드·정렬·미리보기 토글과 폭) | engine 의 surface 실행 객체(`EngineRuntime::surfaces`) | View 가 `EngineAction::Explorer`·`EngineAction::ExplorerCwd`·`EngineAction::ExplorerPreview` 로 요청하고, App 이 요청에 담긴 `SurfaceBinding` 이 아직 그 surface 를 가리킬 때만 적용한 뒤 레이아웃을 dirty 로 표시한다(`EngineAction::Explorer` 는 `src/app/explorer_action.rs`, 나머지는 `src/app/engine_action.rs`) |
| 즐겨찾기 목록 | 프로세스 원본 `RuntimeRegistries::explorer_favorites`(`SharedList<ExplorerFavorites>`). engine 의 `EngineRuntime::explorer_favorites` 는 그리기용 사본(`Replica`) | `EngineAction::AddExplorerFavorite`·`RemoveExplorerFavorite` 가 원본을 바꾸고 불변 snapshot 저장을 접수한다(아래 "즐겨찾기") |
| 목록 캐시·로딩 상태·선택·트리 펼침·주소창 편집·타입어헤드 | 창의 View 상태 `MainViewState::explorer_views`(surface id 별 `ExplorerView`) | View 가 직접 바꾼다. 저장하지 않는다 |
| 파일 클립보드 | 앱 공용 슬롯 `AppServices::explorer_clipboard`(창마다 `MainViewState::explorer_clipboard` 로 같은 슬롯을 받는다), OS 쪽은 `MainViewState::file_clipboard` | 복사·잘라내기가 채우고 로컬 경로는 OS 파일 클립보드에도 올린다. 붙여넣기는 나중에 복사한 쪽을 쓰고 잘라내기를 소비한다([OS 파일 클립보드](file-operations.md#os-파일-클립보드)). 저장하지 않는다 |
| 로컬 디렉터리 읽기 | App 의 읽기 worker(`src/app/local_reads.rs`, 동시 실행 최대 4개) | View 가 요청을 만들고 받은 receipt 로 결과를 가져간다 |
| 파일 변경 작업 | App 의 `explorer_files` worker(한 번에 한 작업) | View 가 대상 경로와 binding 을 고정한 요청을 넘긴다([파일 작업 계약](file-operations.md)) |
| mirror 목록 조회 | 원격 attach 채널(`list_dir_request`/`list_dir_result`) | View 가 요청을 outbox 에 쌓고 렌더 뒤 App 이 원격으로 보낸다 |

View 는 모델을 직접 바꾸지 않고 파일시스템도 직접 읽지 않는다. 경계 전체는 [Model·View 분리](../../dev-guide/model-view-split.md)에 있다.

### 모델 (`ExplorerPanel`)

- `ExplorerPanel` 은 식별(`id`)과 내비게이션 상태, 미리보기 패널 상태를 보유한다 — 내부 탭 목록(`tabs`)·활성 탭 인덱스(`active`)·미리보기(`preview`: `ExplorerPreview { open, width }`, 내부 탭과 관계없이 surface 에 하나, 폭은 UI 배율 1 기준이며 `None` 이면 토큰 기본 폭)·숨김 파일 표시 여부(`show_hidden`, surface 에 하나, 기본 끔). 각 `ExplorerTab` 은 **cwd(고정 루트)** 와 **current(현재 폴더, 필드명 `root`)** 를 분리해 보유하고, 히스토리(back/forward 스택), 뷰 모드(`view_mode`), 정렬 컬럼/방향(`sort_column`/`sort_dir`)을 가진다.
- **cwd ↔ current 분리** (VS Code 식 "고정 프로젝트 + 자유 탐색"): `cwd()` 는 explorer 를 연 프로젝트 루트로 **좌측 사이드바 트리 루트**·**스폰 cwd**(`source_cwd()`)·**surface/탭 표시명**의 기준이며 내비게이션에 불변. `current()`(=`current_root()`) 는 **우측 목록**·**상단 주소창(편집형 PathField)** 이 따라가는 탐색 폴더로, back/forward/go_up 이 이것만 움직인다. current 는 cwd 하위로 제한되지 않고 파일시스템 어디로든 자유 이동한다.
- 내비게이션: `navigate_to(dir)` / `go_back` / `go_forward` / `go_up` — 모두 **current 에만** 작용. `can_go_up` 은 current 의 파일시스템 부모 존재만 본다(cwd 경계로 clamp 안 함). `set_cwd(folder)` 는 cwd·current 를 folder 로 재설정하고 히스토리를 비운다(explorer-03 "이 폴더로 루트 설정"). 히스토리는 탭별로 독립.
- **`..` 상위 이동**: current 에 부모가 있으면(파일시스템 루트 아님) 우측 목록 최상단에 `..` 특수 행을 그려 상위 폴더로 이동한다. `..` 는 **렌더 전용**이라 `view.entries`/선택/상태줄/컨텍스트 메뉴 대상이 아니며 더블클릭 시 `Navigate(parent)` 만 emit 한다.
- 내부 탭: `add_tab`(활성 탭 cwd 복제, current=cwd) / `close_tab(idx)` / `active_tab[_mut]`. surface 하나 안에 여러 디렉토리 탭을 둔다 (상위 pane 탭과 별개). 탭별 cwd 는 독립(per-tab).

## 생성·갱신·종료

- **생성**: 공통 생성 경로(`tasty new tab --type explorer`, split, 변환, 단축키 `open_explorer`, 메뉴 "새 탭으로 열기")가 kind `explorer` 의 `create` 를 부른다. params 는 `path`(선택)와 `view_mode`(선택, 없으면 설정의 마지막 뷰 모드)다. root 결정 규칙은 아래 [IPC·CLI](#ipccli)에 있다. 메뉴 "새 탭으로 열기"는 우클릭한 surface 의 소유 pane 을 찾아 `DomainIntent::CreateTab` 을 보낸다(`RequestContext::add_kind_tab_by_owner`, `src/state/tab.rs`). 요청의 상속 cwd 는 우클릭한 surface 에서 가져오며 포커스와 무관하다([cwd 정책](../../design/policies/cwd.md)). Pane 을 직접 바꾸지 않는다.
- **목록 갱신**: View 를 그릴 때 `sync` 가 활성 탭의 (current, 정렬) 이 마지막으로 읽은 값과 다르거나 새로고침이 요청됐으면 읽기 요청을 새로 만든다. 상태는 `Loading` 이 되고, 이전 요청의 receipt 는 버린다. 버린 receipt 의 결과는 새 목록에 반영되지 않는다. 폴더가 바뀌었을 때만 선택을 비운다. 같은 폴더를 다시 읽으면 결과 목록에 없는 경로만 선택과 범위 선택 기준(anchor)에서 뺀다. 남은 선택은 같은 선택으로 본다(선택 식별자를 바꾸지 않는다). 폴더가 바뀌었거나 새로고침이 요청됐으면 사이드바 트리 캐시도 비워, 펼친 하위 폴더를 다시 읽는다. 단, 사용자 새로고침(`F5`·툴바·Retry, `ExplorerAction::Refresh` → `request_refresh`)이 목록을 읽어 보이고 있는(`Ok`) 같은 폴더·같은 정렬에 오면 [외부 변경 확인](#외부-변경-확인)의 다시 읽기처럼 보이던 목록과 트리 캐시를 둔 채 현재 폴더와 캐시해 둔 트리 폴더를 다시 읽고, 결과가 오면 바꾼다. `Loading` 화면을 거치지 않으므로 목록·트리의 스크롤과 선택이 그대로이고, 결과 목록에 없는 항목만 선택과 anchor 에서 빠진다. 타입어헤드 초기화와 하위 폴더 검색 재시작은 새로고침과 같다. 읽기 오류·권한 거부 화면에서의 Retry 와 파일 작업 뒤 다시 읽기(`request_reload`)는 목록과 트리 캐시를 비우고 `Loading` 으로 읽는다. mirror 의 새로고침은 응답을 기다리는 경로를 빼고 경로별 결과를 지워 현재 폴더와 펼친 트리를 다시 요청한다.
- **결과 상태**: 읽은 목록은 `Ok`(항목 0개면 빈 폴더 화면), 권한 거부는 `NoPermission`, 그 밖의 실패(경로 없음·폴더 아님·IO 오류·worker 끊김)는 `Error` 다. 자동 재시도는 없고 Retry(새로고침)가 같은 경로를 다시 읽는다. mirror 는 응답이 8초 안에 오지 않으면 `Error`(시간 초과)로 바꾼다.
- **보이지 않는 동안**: 읽기 요청은 View 를 그릴 때만 만든다. 이미 보낸 요청의 결과는 그 View 가 남아 있는 한 다른 탭에 가려져 있어도 받아 둔다.
- **파일 작업 뒤 갱신**: 로컬 파일 작업이 끝나면 성공·실패와 관계없이 모든 윈도우의 로컬 explorer 중 작업이 바꿀 수 있는 폴더를 보는 View 를 다시 읽고, 그 폴더의 트리 캐시를 지운다. 범위는 [파일 조작](file-operations.md#작업-뒤-목록-갱신) 에 있다. 원격 파일 작업은 같은 mirror workspace 의 explorer 만 같은 규칙으로 다시 읽게 한다.
- **현재 폴더가 사라짐**: 다시 읽은 결과가 경로 없음이면 읽기 오류 화면(다시 시도 · 상위 폴더로)을 보인다. 다른 경로로 자동으로 옮기지 않는다. 상위 폴더도 함께 사라졌으면 "상위 폴더로" 는 남아 있는 가장 가까운 상위 폴더로 간다. 다른 프로그램이 지웠다면 아래 [외부 변경 확인](#외부-변경-확인)이 찾았을 때 같은 화면이 된다.
- **늦은 결과**: 목록 결과는 receipt 를 가진 View 에만 들어간다. 그 사이 사용자가 바꾼 선택은 지우지 않는다. 파일 작업 완료의 선택 정리와 결과 카드·오류 토스트는 원 View 와 binding 이 유효할 때만 낸다. 실패한 rename·trash 는 선택을 유지하고 목록을 다시 읽는다. 부분 성공한 붙여넣기는 결과 카드에 실패 경로를 보이고 cut 클립보드를 유지한다. 사용자가 그 사이 새로 담은 클립보드는 건드리지 않는다.
- **종료**: surface 를 닫으면 engine 이 모델을 지우고, 창은 `release_surface_views` 로 그 surface 의 `ExplorerView` 를 지운다. 대기 중인 로컬 읽기의 receipt 도 함께 버려지고, 늦게 온 원격 응답은 기다리는 View 가 없어 버려진다. 아직 시작하지 않은 파일 작업 요청은 binding 이 더는 그 surface 를 가리키지 않아 시작하지 않는다. 이미 시작한 작업은 끝까지 실행되고 결과 카드·목록 갱신만 생략된다. 단, 이름 충돌 답을 기다리던 작업은 물을 칸이 없어졌으므로 취소한다(`src/app/explorer_files.rs`·`src/app/explorer_files/ui_sync.rs`). 진행·충돌·결과 표시는 [파일 작업](file-operations.md#작업-실행--진행충돌결과)에 있다.

### 외부 변경 확인

다른 프로그램이 바꾼 폴더는 OS 파일 감시 없이 주기 확인으로 찾는다. 결정과 대안은 이 문서에 있다(`src/adapters/ui/surface/explorer/view/poll.rs`).

- **대상 칸**: 입력을 받는 창(OS 창 포커스)에서 포커스를 가진 surface 가 속한 탭의 로컬 explorer 만 확인한다(`checks_outside_changes`). 같은 탭을 나눈 다른 surface 에 포커스가 있어도 그 탭의 explorer 는 확인한다. 다른 탭에 가려졌거나 다른 Pane 의 탭에 있는 explorer, 포커스가 없는 창의 explorer 는 확인하지 않는다. Tasty 가 배경 앱이면 어느 explorer 도 확인하지 않는다. 창 포커스는 egui-winit 이 winit `Focused` 이벤트로 갱신하는 `RawInput.focused` 를 읽는다.
- **주기**: 탭이나 창이 포커스를 얻은 프레임에 바로 한 번 확인하고, 그 뒤로는 2 초(`EXTERNAL_POLL_INTERVAL`)마다 확인한다. 포커스를 잃으면 멈추고, 다시 얻으면 주기를 기다리지 않고 확인한다. 창 포커스가 바뀌면 `Focused` 이벤트가 그 창을 다시 그리게 표시하므로 그 프레임이 확인을 시작하거나 멈춘다. 주기 확인은 창을 그리지 않는다. App 은 가장 이른 다음 확인 시각에 `Tick::ExplorerPoll` 로 깨어나 마지막 프레임에 포커스 창의 포커스 탭으로 그려진 칸의 확인을 시작하고(`ExplorerViewStore::start_due_checks`), 확인 결과에서 바뀐 폴더만 다시 읽는다. 바뀐 폴더가 없으면 프레임을 그리지 않으며, 다시 읽은 결과가 와야 그린다.
- **확인 대상**: 현재 폴더와, 하위 목록을 읽어 둔 펼친 트리 폴더다. 펼치지 않은 트리 노드의 펼침 표시는 확인하지 않는다. 한 회차는 폴더 32 개(`MAX_POLL_TARGETS`)까지만 표지를 읽는다. 현재 폴더를 먼저 넣고, 남은 자리는 가장 오래 확인하지 않은 트리 폴더부터(같으면 경로 순) 채운다. 넣은 폴더와 방금 목록을 읽은 폴더는 순서의 뒤로 가고, 새로 펼친 폴더는 확인한 적이 없으므로 앞에 선다. 트리 폴더가 N 개면 현재 폴더는 매 회차, 트리 폴더는 ⌈N / 31⌉ 회차 안에 한 번씩 확인된다. 이번 회차에 넣지 못한 폴더도 기준 표지를 지우지 않으므로 그 사이의 변경은 차례가 오면 찾는다. 접은 폴더의 기록은 지운다.
- **worker 사용**: 한 회차는 읽기 worker(동시 4 개, `src/app/local_reads.rs`) 하나에서 차례로 읽는다. 회차가 끝나기 전에는 다음 회차를 시작하지 않으므로 확인이 차지하는 worker 는 언제나 하나 이하이고, 나머지 worker 는 목록·썸네일·속성 읽기가 쓴다. 상한 32 는 느린 매체에서 폴더 하나에 수십 ms 가 걸려도 한 회차가 주기(2 초) 안에 끝나게 하는 값이다. 확인 중에 포커스를 잃어도 결과는 거둬 다음 확인을 막지 않고, 칸이 닫히면 결과는 버려지며 그 job 도 끝나 거둬진다.
- **비교**: worker 에서 폴더 metadata(수정 시각·크기)만 읽어 마지막으로 읽었을 때의 표지와 비교한다. 항목은 읽지 않으므로 바뀌지 않은 폴더는 `read_dir` 하지 않는다. 목록을 읽을 때는 표지를 항목보다 먼저 읽어, 그 사이의 변경은 다음 확인에서 다시 읽힌다. 다른 읽기를 기다리는 동안에는 그 회차를 건너뛴다.
- **파일 정보 확인(설정, 기본 꺼짐)**: `general.explorer_refresh_file_details`(설정 › 일반 › "Explorer: refresh file details")를 켜면 같은 회차·같은 worker 에서 폴더 표지를 다 읽은 뒤 현재 폴더의 항목을 읽어, 경로와 content key(수정 시각·크기·Unix inode·ctime)를 합친 항목 서명을 마지막으로 읽은 목록의 서명과 비교한다(`dir_stamp.rs` 의 `entries_key`). 다르면 표지가 바뀐 때와 같이 보이던 목록을 둔 채 다시 읽으므로 선택·스크롤·포커스가 그대로이고, content key 가 바뀐 항목은 Detail 의 크기·날짜 열, 미리보기, 썸네일이 새 값으로 바뀐다. 비교 대상은 현재 폴더뿐이며 트리 폴더는 표지만 비교한다. 비용은 매 회차 현재 폴더 항목 수만큼의 metadata 읽기라 폴더 표지 확인(회차당 32 개 이하)과 따로 늘어난다. 느린 드라이브나 항목이 아주 많은 폴더에서는 끄고 새로고침(`F5`)으로 맞추는 것이 기본값의 취지다. 설정은 다음 회차부터 적용하고, 켜기 전에 읽은 목록의 서명과도 비교한다. 꺼도 폴더 목록 변화 확인과 2 초 주기는 그대로다. mirror 는 이 설정과 관계없이 확인하지 않는다.
- **다시 읽기**: 표지가 바뀐 현재 폴더는 보이던 목록을 둔 채 다시 읽고, 결과가 오면 목록만 바꾼다. `Loading` 화면, 타입어헤드 초기화, 하위 폴더 검색 재시작은 하지 않는다. 선택·스크롤·포커스는 그대로이고, 결과 목록에 없는 항목만 선택과 anchor 에서 빠진다. 표지가 바뀐 펼친 트리 폴더는 보이던 하위 목록을 둔 채 그 폴더만 다시 읽고 결과로 바꾼다. 캐시를 먼저 비우지 않으므로 사이드바 트리의 높이와 스크롤도 그대로다. 트리에 캐시된 현재 폴더의 하위 목록은 목록 결과로 함께 바꾸므로 따로 읽지 않는다. 폴더가 사라졌으면 읽기 오류 화면이 된다.
- **mirror**: 원격 IO 비용 때문에 확인하지 않는다. 새로고침(`F5`)으로 맞춘다.
- **한계**: 폴더의 수정 시각은 항목이 생기거나 사라지거나 이름이 바뀔 때 바뀐다. 파일 정보 확인을 끈 상태(기본)에서는 폴더 안 파일의 내용·크기·수정 시각만 바뀌면 찾지 못하므로 Detail 의 크기·날짜 열은 새로고침해야 맞는다. 켜도 트리 폴더 안 파일의 변경은 찾지 않는다. 권한만 바뀌는 변경(항목이나 폴더 자신의 `chmod`)은 ctime 만 바꾸므로 폴더 표지로는 찾지 못한다(`dir_stamp.rs` 의 `a_permission_change_alone_leaves_the_folder_stamp`, Linux 에서 확인). 파일 정보 확인을 켜면 Unix 에서는 현재 폴더 항목의 ctime 이 서명에 들어가 항목의 권한 변경도 다시 읽기를 부르지만, 목록에 권한 열이 없어 화면은 같다. Windows 는 content key 에 inode·ctime 이 없어 크기·수정 시각이 같은 교체를 찾지 못한다. 수정 시각 해상도가 거친 파일시스템(FAT 2 초, HFS+ 1 초 등)에서는 목록을 읽은 직후 같은 시각 단위 안의 변경을 놓칠 수 있고, 다음 변경이나 새로고침 때 보인다. 확인 사이에 생겼다 사라진 항목은 보이지 않는다.

### 사용자 조작의 적용

렌더 중 발생한 사용자 상호작용은 `ExplorerAction`(OpenFile / Navigate / GoBack / GoForward / GoUp / Refresh / SetViewMode / SetSort / NewTab / CloseTab / SelectTab / ContextMenu / AddressRejected / Create / MoreMenu / Ops) 으로 모았다가 `apply_explorer_action(state, engine, sid, act)` 에서 적용한다. 파일 열기/새로고침은 뷰 스토어만 다룬다. Create 는 새 항목 만들기(아래 "새 폴더·새 파일"), MoreMenu 는 좁은 칸의 More 메뉴, Ops 는 파일 작업 표시(진행·대기열·결과 카드)에서 고른 일이다. 내비게이션·뷰모드·정렬·내부 탭 조작만 실행 요청 `ExplorerCommand`(Navigate / Back / Forward / Up / ViewMode / Sort / NewTab / CloseTab / SelectTab, `src/app/requests/explorer.rs`)로 바꾼다. 내부 탭은 index 로 고른다. 이 명령을 origin surface 의 `SurfaceBinding` 과 묶은 `BoundExplorerCommand` 를 사용자 출처의 `EngineAction::Explorer` 로 요청하고, App 이 binding 이 아직 그 surface 를 가리킬 때만 `ExplorerPanel` 에 적용한 뒤 레이아웃을 dirty 로 표시한다(`src/app/explorer_action.rs`, 포커스 독립). 닫히거나 바뀐 대상과 탐색기가 아닌 surface 는 거절하고 패널과 dirty 표시를 바꾸지 않는다. 메뉴·주소 오류·새로고침·파일 열기·새 항목·파일 작업은 이 명령으로 표현할 수 없다. 경로가 바뀌면 `ExplorerView` 가 다음 draw 에서 자동 감지해 재로드한다.

- `AddressRejected` 는 주소 입력을 이동하지 않은 이유를 그 surface 범위의 오류 toast 로 알린다. 패널은 바꾸지 않는다.
- 파일 열기는 `DomainIntent::DispatchFile { origin_surface_id: Some(sid) }` 로 [file-handler](../../features/file-handler/index.md) 에 위임한다 — explorer 자신은 파일 식별/디스패치 정책을 모른다.

## 저장·복원

- **저장하는 값**: 내부 탭마다 cwd·current(`root`)·뷰 모드·정렬 열·정렬 방향, 활성 내부 탭 번호, 미리보기 패널, 숨김 파일 표시 여부(`register_explorer` 의 `snapshot`). 레이아웃 journal 에 들어간다.
- **미리보기 키**: `"preview": {"open": true, "width": 428.0}`. 닫혀 있으면 `open` 을, 기본 폭이면 `width` 를 싣지 않고, 둘 다 기본값이면 `preview` 키 자체를 싣지 않는다. preset capture 는 이 snapshot 을 TOML 로 저장하는데 TOML 에는 null 이 없기 때문이다. 형식이 맞지 않는 값(문자열 `open`, 0 이하·유한하지 않은 `width`)은 기본값으로 읽는다. 폭은 그릴 때 토큰 범위(200…460)와 칸 폭으로 다시 제한한다.
- **숨김 파일 키**: 켜져 있을 때만 `"show_hidden": true` 를 싣는다. 끔(기본)이면 키를 싣지 않는다. bool 이 아니면 끔으로 읽는다. 복원과 preset `create` 가 같은 규칙으로 읽는다.
- **저장하지 않는 값**: back/forward 히스토리, 목록 캐시, 선택, 트리 펼침, 주소창 편집, 타입어헤드, Find 바, 파일 클립보드. 복원하면 히스토리는 비어 있고 목록은 다시 읽는다.
- **복원**: `restore` 가 저장한 탭으로 `ExplorerPanel` 을 만든다. 탭이 없으면 홈 하나로, 활성 번호가 범위를 넘으면 마지막 탭으로 맞춘다. 경로가 없거나 상대 경로면 홈으로 교정한다. 복원은 create 를 거치지 않으므로 설정의 뷰 모드가 아니라 저장한 탭별 값을 쓴다. 저장한 폴더가 사라졌으면 복원은 성공하고 목록 자리에 읽기 오류 화면이 나온다. 미리보기 토글·폭도 같이 복원한다.
- **preset**: preset 적용은 `restore` 가 아니라 `create` 로 explorer 를 만든다. capture 한 preset 의 params 는 위 snapshot 그대로다(`tabs`·`active`·`preview`). `create` 는 명시 `path` param 도 cwd 도 없고 `tabs` 가 비어 있지 않으면 `restore` 와 같은 함수(`explorer_panel_from_snapshot`)로 내부 탭·활성 탭·미리보기를 되살린다. 탭 보정 규칙(빈 목록, 범위를 넘은 활성 번호, 없거나 상대 경로인 cwd·current)도 레이아웃 복원과 같고, 사라진 절대 경로는 그대로 두어 읽기 오류 화면이 나온다(홈으로 바꾸지 않고 알림도 띄우지 않는다). preset 계획이 채우는 기본 `view_mode` param 은 보지 않고 탭마다 저장한 뷰 모드를 쓴다. preset 편집기에서 작업 디렉터리(cwd 필드)를 지정하거나 명시 `path` 가 있으면 저장한 탭 대신 그 폴더 하나로 연다. capture 는 cwd 를 채우지 않으므로 편집기에서 지정하지 않는 한 저장한 탭이 열린다. 미리보기는 이때도 params 에서 읽는다.
- **preset 탭 이름**: preset 계획은 surface 를 만들기 전에 첫 leaf 의 cwd·params 로 페인 탭 이름을 정한다(`default_tab_name_for_kind`). explorer 는 `path` param 이 있으면 그 폴더 이름이고, 없으면 `tab_name_from_params`(`explorer_tab_name`)가 create 와 같은 순서로 열릴 폴더의 이름을 고른다. cwd 가 있으면 그 폴더, 없고 capture 한 탭이 있으면 활성 탭의 cwd, 둘 다 없으면 create 가 여는 기본 폴더(홈)의 이름이다. 폴더 이름이 없는 파일시스템 루트일 때만 종류 이름("Explorer")이다. 같은 규칙을 IPC 의 explorer 워크스페이스 생성도 쓴다.
- **즐겨찾기**는 레이아웃이 아니라 별도 파일에 저장한다(아래 "즐겨찾기").
- mirror workspace 는 로컬에 저장하지 않는다([원격 attach](../../features/remote-attach/index.md)).

## IPC·CLI

explorer 는 일반 surface 생성 메커니즘으로 다룬다 (전용 IPC 추가 없이 generic 경로):

- 생성: `tasty new tab --type explorer [--path <dir>] [--cwd <dir>]` / `tasty new workspace --type explorer [--path <dir>] [--cwd <dir>]` / `tasty split ... --type explorer [--cwd <dir>]`. 명시 `--cwd` 는 아래 root 결정 규칙의 carry cwd 로 들어간다(`--path` 가 있으면 `--path` 가 이긴다). 탭 이름도 그 폴더 이름이다. 둘 다 없으면 새 탭은 explorer `default_params` 의 `path = "@home"` 로 home 이 주입되고(fresh-context), 워크스페이스·split 은 cwd 없이 만들어 홈을 연다. CLI 는 빠진 `--path`·`--cwd` 를 null 키로 보내므로 이때는 주입을 건너뛰지만(`apply_kind_default_params` 는 키가 있으면 채우지 않는다) create 의 기본값도 홈이라 결과는 같다. 탭 이름은 어느 경우든 연 폴더(홈)의 이름이다. explorer 는 다른 surface 의 cwd 를 이어받지 않는다([cwd 정책](../../design/policies/cwd.md#2-carry-경로-강제--surfacekinddefcreate-시그니처)). (IPC: `DomainIntent::CreateTab { kind: "explorer", surface_params }`.)
- **root 결정 규칙**: `path` param → carry cwd → (둘 다 없고 params 에 capture 한 `tabs` 가 있으면 저장한 내부 탭, 위 "preset") → `$HOME`/`%USERPROFILE%` → (홈 조회 실패 시) 절대경로로 확정한 프로세스 cwd. 앞 두 단계의 값이 **상대경로면 채택하지 않고** 홈으로 내려간다 — explorer root 는 어떤 생성 경로(`split`/`new tab`/`new workspace`/convert)에서도 **항상 절대경로**다. 상대 root 는 프로세스 cwd 를 root 로 승격시키고 그 문자열이 주소창·경로 복사·attach `list_dir` wire 로 새어나가기 때문이다. `"."` 로 저장된 구 `layout.json` 스냅샷도 복원 시 홈으로 교정된다. 근거·강제 수단: [surface cwd 불변식 §5](../../design/policies/cwd.md#5-explorer-root-fallback-host-builtin).
- 조회/닫기: `tasty list surfaces` 에 `foreground_process`/`pane_id`/`workspace_id` 와 함께 나타나고, `tasty close ...` 로 닫는다 — 전 워크스페이스 순회·ID 직접 지정(포커스 독립).
- 변환: 다른 surface 를 explorer 로 in-place 변환 — `Intent::ConvertSurface { surface_id, target: ConvertTarget::Kind { kind: "explorer", .. } }`. cwd 미지정 시 source surface 에서 carry. [convert-surface](../../features/convert-surface/index.md) 의 generic convert popup 도 registry kind 열거로 explorer 를 노출한다.

## headless·원격 제약

- **headless**: kind 는 GUI 와 관계없이 부팅 때 등록된다. 생성·조회·닫기·변환·저장·복원은 모델만으로 동작한다. 목록 읽기·화면·컨텍스트 메뉴·파일 작업은 View 가 있는 창에서만 일어난다.
- **mirror**: 목록은 원격 조회로 받고 로컬 파일시스템을 읽지 않는다. 파일 변경은 원격 쓰기를 알린 server 에 요청하고, 즐겨찾기 추가는 막으며, 파일 열기는 원격에 탭을 만든다. 아래 "mirror(attach) explorer 의 파일 변경"과 "mirror explorer 의 파일 열기"에 있다.

### mirror(attach) explorer 의 파일 변경

docs/features/remote-attach/index.md에 따라 mirror explorer 의 파일 변경(새 폴더·새 파일·이름 변경·휴지통·같은 연결 안의 붙여넣기·드래그 놓기)은 원격 server 가 실행한다. 이쪽은 요청을 보내고 server 가 돌려준 진행·충돌 질문·결과를 로컬 작업과 같은 진행 표시·충돌 popup·결과 카드로 보인다. 동작 규칙과 진행·대기열의 좁은 폭 경계는 [파일 작업 계약](file-operations.md)에 있다. 파일 더블클릭 열기(`OpenFile`)는 로컬과 같은 `DispatchFile` 로 가고, origin 이 mirror surface 라 원격 열기 규칙을 따른다(아래 "mirror explorer 의 파일 열기"). mirror 워크스페이스(`ws.mirror`)에 속한 explorer surface 에서는:

- **원격 쓰기 지원 여부**: 연결의 server 가 attach descriptor 에 원격 쓰기 capability 를 알렸고 연결이 살아 있으면 쓰기를 허용한다. App 이 매 회차 세션에서 이 목록을 engine 과 explorer View 에 맞추고, 메뉴·단축키·툴바·끌기는 `EngineRead::blocks_explorer_write` 와 View 의 `writes_allowed` 로 판정한다.
- **받지 않는 server**: 새 폴더/새 파일/붙여넣기/잘라내기/이름 변경/휴지통으로 이동 항목을 `build_explorer_context_menu` 에서 숨기지 않고 꺼 두며, 툴팁(Linux·macOS 네이티브 메뉴)에 이유를 단다. More 메뉴의 생성 행과 툴바의 create 묶음도 같은 툴팁으로 꺼 둔다. 이유는 `EngineRead::explorer_write_refusal` 이 정한다. 연결돼 있지만 server 가 원격 쓰기 capability 를 알리지 않은 옛 Tasty 면 `explorer.remote.write_needs_update`("Update Tasty on {host} to change remote files.", host 는 연결의 `remote_label`)이고, 연결이 끊기는 중이라 호스트 정보가 없으면 `explorer.state.remote_write_unsupported` 다. 메뉴가 아닌 경로(키보드 단축키 등)로 같은 핸들러가 불리면(`explorer_menu_paste`/`_trash`/`_rename`, `explorer_menu_set_clipboard`의 `cut=true`, 새 항목) 진입부의 `refuse_remote_write` 가 다시 확인하고 같은 문구를 그 칸의 info toast 로 알린 뒤 반환한다. Windows 네이티브 메뉴에는 툴팁이 없어 이 토스트가 이유를 알리는 경로다. copy_path/복사/이 폴더로 루트 설정은 켜져 있다. server 는 attach descriptor 에 원격 쓰기 capability 를 늘 싣는다. 그래서 화면 없는 server 를 따로 알아볼 신호가 없고, 화면 없는 server 용 이유 문구는 쓰지 않는다.
- **재연결 뒤 다시 읽기**: 연결이 끊겼다가 재연결 설치에 성공하면, 그리고 프레임 손실 뒤 재동기화(resync)로 같은 설치를 다시 하면(`attach_client/connection.rs` 의 `install_reconnected_mirror` → `finish_reconnected_view`) 그 mirror workspace 의 explorer View 마다 원격 목록 상태(`remote_state`, 응답을 기다리던 폴더와 응답 시간 초과로 오류가 된 폴더 포함)를 비우고 다시 읽는다(`view/remote_reload.rs` 의 `reload_mirror`). 옛 연결의 목록 요청은 응답이 오지 않기 때문이다. 끊긴 동안 원격에서 바뀐 내용도 이때 보인다. Retry 를 누르지 않아도 된다.
- **시스템에서 열기·즐겨찾기 추가·새 탭으로 열기**: server 의 쓰기 지원과 관계없이 mirror 에서는 숨기고 핸들러에서 막는다(`CoreState::is_mirror_surface`).
- **rename 팝업의 대상 게이트**(`rename_target_exists`, `src/adapters/ui/dialog.rs`)는 메뉴 시점 세대가 그대로일 때만 팝업을 유지한다. 로컬 경로는 경로가 남아 있는지도 확인하고, 원격 경로는 이 컴퓨터에서 확인하지 않는다. 이름 규칙은 원격 경로의 모양(드라이브 문자·UNC 면 Windows 규칙, 아니면 POSIX 규칙)으로 고르고 최종 판정은 server 가 한다.
- **즐겨찾기**: `~/.tasty/explorer-favorites.toml` 는 surface/host 무관 전역 저장소다. mirror explorer 의 경로(원격 호스트 경로)가 이 전역 목록에 섞이면 로컬/다른 호스트 explorer 의 사이드바를 오염시키므로, 즐겨찾기 추가는 mirror 에서 팝업을 열기 전에 차단된다.
- **새 탭으로 열기**: 메뉴 숨김(탐색기 항목 메뉴와 즐겨찾기 행 메뉴)과 핸들러 가드(`explorer_menu_open_in_new_tab`, `remote_write_unsupported` 토스트)로 막는다. 파일을 바꾸는 작업이 아니므로 원격 쓰기 지원과는 별개의 제한이다. 이 메뉴가 쓰는 `DomainIntent::CreateTab` 은 mirror pane 이면 App 이 원격 `StructuralOp::NewTab` 으로 전달하므로(`src/app/services/impl_mirror.rs`) 탭이 로컬에만 생기는 경로는 없다. 이 제한을 유지할지는 정해지지 않았다.

### mirror explorer 의 파일 열기

mirror explorer 에서 파일을 더블클릭하면 원격 호스트에 그 파일의 탭을 만든다.

- 식별은 파일 이름만 본다(`DetectDepth::Name` — 확장자·path glob). client 에 같은 경로의 파일·디렉터리가 있어도 읽지 않는다.
- 매칭 핸들러 중 `open_surface` 이면서 client 가 그 kind 의 콘텐츠를 mirror 하는 것(현재 markdown, 허용된 egui-mesh kind)만 실행한다. 1순위가 그런 핸들러면 바로 열고, 아니면 그런 핸들러만 담은 핸들러 picker 를 띄운다. 선택한 핸들러의 `CreateTab` 은 원격 `StructuralOp::NewTab` 으로 forward 되고 사용자 origin 으로 표시돼 원격과 client 양쪽에서 새 탭이 선택된다.
- 그런 핸들러가 하나도 없으면(`system`·`ipc` 핸들러뿐, html 처럼 placeholder 로 보이는 kind, 매칭 없음) picker 없이 `explorer.state.remote_open_unsupported` toast 로 안내한다.
- 원격 경로는 로컬 최근 목록에 기록하지 않는다. 규칙 전체는 [파일 핸들러](../../features/file-handler/index.md) 의 원격 대상 절을 따른다.

## 화면

화면의 픽셀·색 값은 Theme 토큰의 scale 1 값이다. 정본은 토큰과 [디자인·갤러리 매핑](../../design/systems/design-gallery-mapping.md)이다.

### 내부 탭별 상태

`ExplorerTabId`는 실행 중 탭의 식별자다. 같은 폴더를 연 탭도 서로 다른 값을 받고, 탐색·정렬·다른 탭 닫기로 목록 번호가 바뀌어도 유지한다. 레이아웃이나 프리셋에는 저장하지 않아 복원한 탭은 새 식별자를 받는다. 같은 연결에서 원격 surface와 서버 descriptor의 root가 그대로인 mirror 레이아웃 갱신은 기존 패널 전체를 유지한다. 내부 탭 목록·활성 탭·클라이언트에서 탐색한 위치·표시 설정도 그대로다. 서버에서 마지막으로 받은 root는 클라이언트의 current와 별도로 기억하므로 사용자 탐색을 서버 변경으로 오인하지 않는다. 서버 descriptor의 root 또는 연결 epoch가 바뀌면 서버 root의 새 패널·새 탭 ID로 시작한다.

창의 `ExplorerViewStore`는 surface마다 `ExplorerView`를 두고, 그 안에서 목록·로딩 상태·선택·anchor·키보드 현재 항목·트리 펼침·Find와 읽기 receipt를 내부 탭 ID별로 보관한다(`view/tab_state.rs`). 방문했던 탭으로 돌아오면 그 목록을 바로 그리며 탭 전환만으로 `Loading` 화면을 거치지 않는다. 같은 폴더의 두 탭도 선택과 Find를 공유하지 않는다. 본문 세 보기와 Files 트리의 스크롤·Find 입력·주소창 위젯 ID도 내부 탭 ID를 포함하므로 탭 번호가 옮겨져도 스크롤은 이어진다. 창과 surface의 범위는 그대로다.

비활성 탭이 요청한 목록·트리 읽기와 하위 폴더 검색의 결과는 그 탭에 반영한다. 새 외부 변경 확인은 활성 내부 탭만 시작하고 돌아오면 바로 확인한다. 원격 목록 요청과 캐시도 내부 탭이 소유하며 request ID가 맞는 탭에만 반영한다. 탭을 닫으면 보관 상태·receipt·보내지 않은 목록 요청을 버린다. 이미 실행 중인 일반 OS 읽기를 강제 중단하지는 않으며, 검색은 항목 경계와 송신 대기에서 취소를 확인한다. 이미 실행 중인 OS 파일 조회는 강제 중단할 수 없다. 재연결은 해당 mirror 패널을 새로 만들며 옛 탭의 선택·Find·요청·캐시는 다음 동기화에서 폐기된다.

탭 전환은 주소 편집·예약 reveal·입력 중인 새 항목·타입어헤드·영역 선택과 드래그 상태를 끝낸다. 확정한 선택은 보존한다. 미리보기 토글과 폭, 숨김 파일 표시, 파일 작업 진행·충돌·결과 카드는 surface 범위 그대로이며 클립보드도 기존 공용 범위다. 진행 중 파일 변경은 탭 전환이나 내부 탭 닫기로 취소하지 않는다. 완료에 따른 선택 정리는 원래 선택 identity가 남아 있는 탭에만 적용한다. 파일 작업으로 바뀐 폴더는 비활성 탭에도 다시 읽기를 표시한다.

### 파일 영역 바탕과 행 글자

파일 영역(미리보기를 뺀 목록 자리)은 포커스 바탕 쌍(`surface-bed-focused`·`-unfocused`)으로 칠한다. 값은 [포커스 표현](../../design/policies/focus.md#surface-포커스-표현)의 판정을 따르며 입력 판단과 별개다. 탭 바·툴바·사이드바·상태 줄·미리보기는 바탕이 바뀌지 않는다.

행 글자는 `RowInk`(`src/adapters/ui/surface/explorer/entry_style.rs`)가 행 상태와 바탕으로 고른다. 칸의 정본은 tasty-page `vendor/RULES.md` 6절 표다. hover·press·놓기 대상 행의 글자는 포커스 바탕에서 base 칸을, 비포커스 바탕에서 mantle 칸을 읽는다. 그래서 비포커스 파일 영역의 이런 행은 `-raised` 글자를 쓴다. 놓기 대상 틴트는 본문 전체(본문 놓기)나 폴더 행 하나(행 놓기)를 덮는다. 덮인 쉬는 행은 Detail·List·Grid와 `..` 행 모두 놓기 대상 칸을 읽는다.

칠 순서는 바탕, 틴트, 행 fill, 잉크, 링이다. 틴트 대상은 행을 그린 뒤에 정해지므로 파일 영역과 사이드바는 바탕을 칠한 직후 틴트 자리(`DropTintSlot`)를 잡아 두고, 대상이 정해지면 그 자리를 채운다. 링은 그 프레임의 마지막에 칠한다. 불투명한 선택 행은 틴트를 가리고 링은 보인다. 틴트와 링은 칸의 레이어에 있어 칸 위의 popup·카드보다 아래다.

폴더·파일 글리프는 `text-secondary`다. 이미지 글리프는 `accent-info-raised`이고, 틴트 아래 쉬는 이미지 행에서는 `text-secondary`이며 틴트가 사라지면 다음 프레임에 돌아온다. 숨김 항목과 끊어진 링크의 글리프는 틴트 아래에서도 바뀌지 않는다.

### 뷰 상태 (`ExplorerView`, surface id 로 keying)

디렉토리 엔트리 캐시·선택 집합·트리 펼침 같은 무거운 GUI 상태는 모델이 아니라 per-surface 뷰 스토어에 둔다.

- **엔트리 캐시**: `sync(panel)` 이 활성 탭의 `(root, sort_column, sort_dir)` 키를 보고 디렉토리/정렬이 바뀌었거나 새로고침이 요청됐을 때만 App 읽기 worker 에 다시 읽기를 요청한다(위 "생성·갱신·종료"). 디렉토리가 바뀌면 선택을 초기화한다. 읽기 실패는 `LoadState::NoPermission`(권한 거부) / `LoadState::Error(msg)` 로 분류해 콘텐츠 중앙 상태 화면으로 표현한다.
- **상태 화면**: 내용 영역이 목록 대신 가운데 정렬한 글리프 · 제목 · 선택 보조 줄을 보여 준다(`explorer/state_screen.rs`, 시안 `ExpState`). 배치는 공용 `tasty_ui_widgets::state_screen` 이 그리며 갤러리 탐색기 상태 예제와 이미지 캔버스 상태 화면도 같은 함수를 쓴다.
  - **줄바꿈**: 제목과 보조 줄은 줄 길이를 고르게 바꾼다(`tasty-ui-widgets` 내부 `balanced_wrap::balanced_galley`, 시안 `text-wrap: balance`). 최대 폭에서 나온 줄 수를 유지하는 가장 좁은 폭으로 다시 배치해 가장 긴 줄을 최소로 만들므로, 마지막 줄에 한두 글자만 남지 않는다. 미리보기 패널의 상태 화면도 같다. 이유 줄(mono)은 그대로 둔다.
  - **compact 한 줄**: 내용 영역 높이가 `explorer_state_compact_below()`(120px) 미만이면 모든 상태 화면이 한 줄로 바뀐다. 확대하지 않은 16px 글리프(`icon_glyph_size_md`) · 제목(body, 넘치면 끝 말줄임) · 그 상태의 버튼(읽기 오류의 Retry · Go up, 간격 space-xs)을 space-sm 간격으로 놓고 줄 전체를 내용 영역 가운데에 둔다. 보조 줄과 OS 이유 문구는 제목의 툴팁으로 옮긴다. 창을 줄여 탐색기 칸이 하한 아래가 돼도 블록이 잘리지 않게 하기 위한 형태다. 공용 `tasty_ui_widgets::compact_state_row` 가 그리며 갤러리 Short cell 예제와 같은 함수다.
  - 빈 폴더(`Ok`이고 항목 0개): `folderOpen` 글리프(text-muted) + "This folder is empty"(text-secondary). 시안 탐색기 Spec 은 항목 0개와 빈 폴더를 구분하지 않는다. 새 항목 이름을 입력하는 중(`ExplorerView::create`)이면 이 화면 대신 목록 영역을 그려 이름 행을 맨 앞 자리에 보이고, 입력을 확정하거나 취소하면 다음 프레임에 다시 이 화면이다. 로컬과 mirror explorer 가 같다.
  - 읽기 오류(`Error`, 권한 거부 밖의 실패 — 경로 없음, 디렉터리 아님, 로컬 IO 오류, 원격 읽기 실패·응답 시간 초과): `alertTriangle` 글리프와 제목 "Can't read this folder"를 `explorer_error_fg`(→ accent-danger)로 칠한다. 그 아래 받은 오류 문구를 mono caption(text-muted, 최대 폭 200) 한 줄로 번역하지 않고 보인다(원격 시간 초과는 `explorer.state.error_conn_timeout`). 버튼 줄은 space-xs 를 더 띄우고 Retry(Secondary sm, 같은 경로를 다시 읽음 = `ExplorerAction::Refresh`)와 Go up(Ghost sm, 상위 폴더 = `ExplorerAction::GoUp`)을 space-sm 간격으로 가운데에 둔다. 로컬 폴더가 없거나 폴더가 아니어서 실패했으면 읽기 worker 가 남아 있는 가장 가까운 상위 폴더를 오류에 붙여 보내고(`local_reads::MissingFolder`), 그 폴더가 바로 위 폴더가 아니면 Go up 은 그 폴더로 `Navigate` 한다. 툴바의 위로·`Alt+Up` 은 한 단계 위 그대로다. 현재 경로에 상위가 없으면(루트) Go up 을 숨긴다. 툴바·트리·상태줄은 그대로 쓸 수 있다(design `ExpState` read error).
  - 권한 거부(`NoPermission`): `lock` 글리프와 제목 "Permission denied"를 accent-warning으로 칠하고, 보조 줄(caption, text-muted, 최대 폭 200)에 이유를 적는다.
  - 불러오는 중(`Loading`): 글리프 자리에 Spinner + "Loading…".
  - 글리프 배치 크기는 `icon-glyph-size-md`이고 그림만 그 칸 가운데에서 1.6배로 그린다(시안 `transform: scale(1.6)`은 배치에 영향이 없다). 줄 간격은 `space-sm`이다.
- **주소창 편집 상태**: `addr_buffer`(편집 텍스트) / `addr_editing`(포커스=편집모드) / `addr_active`(후보 드롭다운 keyboard-active 행)를 뷰가 소유한다(PathField 계약 — 상태는 호출측 소유). `sync()` 는 **비편집 시** 버퍼를 활성 탭 current(`root`) 로 재동기화하고, 편집 중이면 사용자 입력을 보존한다. 내부 탭은 surface 단위 `ExplorerView` 를 공유하므로, cwd/내부 탭을 바꾸는 액션(`Navigate/GoBack/GoForward/GoUp/NewTab/CloseTab/SelectTab`) 적용 시 `cancel_addr_edit()` 로 편집을 취소해 버퍼가 다른 탭/경로로 새지 않게 하고(다음 `sync()` 가 새 current 로 맞춘다), id_salt 는 surface+내부탭 index 로 고유화한다.
- **타입어헤드 상태**: `type_ahead`(입력 버퍼와 마지막 입력 시각)와 `scroll_to: Option<PathBuf>`(이번 프레임에 화면에 보이게 할 항목). `scroll_to`는 한 프레임만 유지한다. 남겨두면 매 프레임 다시 스크롤해서 사용자가 휠로 다른 곳을 보는 동안 화면이 끌려간다. 순환 시작 위치, 접두사 확장, 입력 되돌리기 같은 규칙은 egui와 파일시스템에 의존하지 않는 `explorer/type_ahead.rs`에 있고, 그리는 쪽은 그 결과를 선택과 스크롤로 옮기는 일만 한다.
- **선택**: `selected: HashSet<PathBuf>` + `anchor`(범위 선택 기준). 클릭은 그 항목만 고르고, Ctrl·Cmd+클릭은 그 항목을 넣거나 빼며, 둘 다 앵커를 그 항목으로 옮긴다. Shift+클릭은 앵커부터 클릭한 항목까지 목록 순서의 범위를 고르고, Ctrl·Cmd+Shift+클릭은 기존 선택에 그 범위를 더한다. Shift 클릭은 앵커를 옮기지 않는다. 앵커가 없거나 다시 읽은 목록에 없으면 Shift 를 뺀 클릭과 같다. grid·list·detail 이 같은 `ExplorerView::click_select` 를 쓴다. `select_all()` 은 보이는 항목 전체(Find 로 거르는 중이면 걸러진 항목)를 선택, `selected_paths_text()` 는 선택 경로를 정렬·개행 결합한 클립보드 페이로드를 만든다.
- **사이드바 트리**: `expanded` 펼침 집합 + `tree_children` lazy 하위 디렉토리 캐시. 폭 196(design `ExpSidebar`). 사이드바는 **2-region 고정 분할**이다 — 상단 **Files**(트리, cwd 루트 고정)는 사이드바 본문 남는 공간 전부를 차지하며 자체 스크롤되고, 하단 **Favorites**는 계산된 고정 높이 영역에서 독립적으로 스크롤된다(Files 를 아무리 스크롤해도 Favorites 위치는 움직이지 않고, 반대도 마찬가지). 두 영역 사이 1px 구분선은 **하단 고정 영역의 상단 경계**에 고정 좌표로 그려진다 — 트리 길이와 무관하며, 트리가 짧아도 그 위 빈 공간은 배경만 남고 구분선이 따라 올라오지 않는다. 트리에서 **현재 폴더(current)** 노드는 surface-active 배경 + text-primary 로 하이라이트되고, 폴더 아이콘은 text-muted. 섹션 캡션은 monospace·micro·uppercase(design `SideHead`).
- **Favorites 고정 높이 계산**(design `favPinHeight`): 사이드바 본문 높이가 600px 이상이면 240px 고정. 600px 미만이면 `round(본문높이 × 0.4 / 4) × 4`(4px 그리드 스냅)와 120px(하한) 중 큰 값. 임계값 전환은 보간 없는 하드 전환이다. 본문 높이가 `explorer_favorites_hide_below`(240px) 미만이면 Favorites 영역과 그 위 구분선을 그리지 않고 Files 가 본문 전체를 쓴다(design Short cell). 본문이 다시 240px 이상이 되면 위 사다리대로 돌아온다. 사이드바 자체는 숨기지 않는다. 본체 구현은 `src/adapters/ui/surface/explorer/favorites_pin.rs`.
- **낮은 칸**: 툴바 아래 행은 칸에 남은 높이의 고정 사각형이고, 사이드바 열과 내용 열은 각자 자기 사각형 안에서만 그린다. 본문이 240px 미만이면 Favorites 를 빼므로 사이드바는 Files 캡션과 트리만 그리고, 트리는 자기 스크롤 영역 안에서 넘친다. 내용 열과 상태줄은 칸 안에 남는다. 내용 목록의 ScrollArea 는 최소 높이를 0 으로 두어 본문이 낮아도 상태줄을 밀어내지 않는다.
- **분할 하한**(design Short cell): pane·surface 분할선을 끌어 탐색기 칸을 줄이면 칸 높이 `explorer_min_height`(180px)에서 멈춘다. 대상은 각 pane 의 활성 탭에 보이는 탐색기 surface 다. 드래그 미리보기 비율이 어떤 탐색기 칸을 하한 아래로 줄이면 이분 탐색으로 하한을 지키는 마지막 비율에 둔다. 창 크기 변경처럼 드래그가 아닌 이유로 이미 하한보다 낮은 칸은 드래그로 더 줄지만 않는다. 창 크기 변경은 하한을 지키지 않는다(낮아진 칸은 위 compact 한 줄로 그린다). 분할(`tasty split`·분할 메뉴)은 분할을 확정할 때 같은 하한을 지키도록 분할선을 옮긴다. 다른 종류의 형제 칸은 남는 높이를 받되 새 분할에서는 56px(`split_sibling_min_height`) 아래가 되지 않는다. 탐색기 칸끼리 하한을 지킬 수 없거나 형제 칸의 56px 을 지킬 수 없으면 거절한다. 메뉴·단축키로 거절되면 info 토스트, 에이전트 split 은 IPC 오류만 받는다([split 명령](../../features/work-area/index.md#분할-비율과-탐색기-칸-하한)). 탐색기 칸을 키우는 쪽과 다른 종류의 칸은 제한하지 않는다. 구현은 `src/state/layout_preview/explorer_floor.rs`.

### 뷰 모드 / 정렬

- 뷰 모드 3 종(grid / list / detail)을 toolbar 우측의 **아이콘 view-mode 토글**(`seg_toggle`, design `SegToggle`)로 전환한다 — grid/list/detail 아이콘 세그먼트, active = segtoggle-on-bg(accent-primary) 채움 + segtoggle-on-fg(text-on-accent) 글리프, inactive = text-muted. detail 뷰는 정렬 컬럼 헤더를 클릭하면 해당 컬럼으로 정렬(같은 컬럼 재클릭 시 방향 토글).
- toolbar 의 **주소표시줄**(`address/field.rs` 의 `address_bar`, design `ExpToolbar`/`PathField`)은 공용 **편집형 `PathField`** 다 — folderOpen leading 아이콘 + mono 경로(비편집=text-secondary / 편집=text-primary) + 우측 Go(arrow-right) 버튼(input-bg/input-border(-focus) 토큰).
  클릭하면 편집 모드로 들어가 임의 디렉토리 경로를 타이핑하고 `↵` 또는 Go 로 **current 이동**한다. 입력을 폴더로 바꾸는 규칙과 거부는 아래 "주소 입력" 절에 있다.
  `Esc` 또는 확정 없는 포커스 이탈은 현재 current 로 원복.
  상위 폴더는 Back/Forward/Up 버튼, 사이드바 트리, 경로 입력으로 이동한다.
  편집 진입 시 **최근 방문 디렉토리** 자동완성 후보 드롭다운이 뜨고(타이핑에 맞춰 substring 필터), 이 후보는 `RecentFiles` 의 `"directory"` kind(markdown 의 파일 recent 와 대칭·영속)에서 온다 — 사용자가 `ExplorerAction::Navigate` 로 이동 확정한 cwd 를 host 가 kind 로 적재(`egui_panels`), draw 경계로 slice 주입.
  최근 후보를 클릭하면 그 원문 경로로 이동한다. 후보를 누르는 동안은 입력을 유지하고 행 안에서 버튼을 떼면 확정한다. 후보 밖에서 떼면 선택하지 않으며, 입력·후보 밖을 새로 누르거나 Esc로 취소하면 현재 경로로 돌아간다.
  주소표시줄 flex:1 / 토글 flex:none.
- **마지막 view mode 기억**: 사용자가 뷰 모드를 바꾸면 그 값이 `Settings.general.explorer_view_mode`(`~/.tasty/config.toml`)에 영속되고, **새로 생성되는** explorer surface 는 이 값으로 열린다(주입 지점: 생성 요청을 만들 때 `EngineRef::apply_kind_default_params` 가 explorer 의 `default_params` `view_mode = "@settings.explorer_view_mode"` 정책 토큰을 `view_mode` param 미지정 시 해석해 params 에 넣고, 그 params 가 explorer `create` 에 전달된다 — kind별 default_params 는 [plugin-development.md](../../dev-guide/plugin-development.md) 참조). 같은 surface 안의 새 내부 탭(`add_tab`)은 활성 탭의 view mode 를 승계한다. snapshot 복원 경로는 create 를 거치지 않아 per-tab 저장값을 그대로 유지한다.
- list 행과 사이드바 디렉토리 행은 공용 `tree_row`(`tree_row_height` 22)를, detail 데이터 행은 공용 `Table`(selectable)을 재사용한다. 탐색기 전용 행 높이는 없다. detail 헤더와 본문 행은 Table 기본 높이 `table_cell_height`(28)이고 헤더 채움은 `table_header_bg`다. detail 컬럼은 Name(1fr)/Size(80)/Date(132)/Type(92)이며, 이름은 선택 행만 text-on-active 이고 나머지 행은 `table_row_fg`(text-secondary)다. 선택 행의 글리프·링크 꼬리·나머지 칸도 surface-active 위 글자색이다. 일치 구간은 `explorer-match-fg-active` 에 `autocomplete-match-underline-width` 밑줄을 그어 구분한다. Size·Date 는 **monospace·caption(11)·text-muted**, Size 는 우측 정렬 + 8px 우측 패딩으로 Date 와 시각적 간격을 둔다(design `DetailRow`). Size 열 제목도 같은 8px(`spacing_sm`) 오른쪽 여백을 둔다(design `DetailHeader`, 공용 Table `header_pad_right`). 열 제목은 공용 Table 머리글 그대로 대문자에 `table_header_tracking` 자간이다.
- 세 뷰 모두 **화면에 걸친 항목만 그린다**. 행 높이가 모드 안에서 모두 같으므로(list `tree_row_height`, grid 칸 높이 + `spacing_md`, detail `table_cell_height`) 화면 밖 행은 같은 높이의 빈자리로 두고 스크롤 길이는 항목 수 그대로다. 프레임 비용은 항목 수가 아니라 보이는 행 수를 따른다. 그리는 행만 목록에서 복제하고, detail 은 복제 없이 참조로 표에 넘긴다.
  - 새 항목 이름 입력은 스크롤해 화면 밖으로 나가도 그린다. 그래야 포커스와 글자가 유지된다. list·grid 는 `..` 와 이름 입력 줄(grid 는 그 둘이 놓인 첫 줄)을 늘 그리고, detail 은 그 행이 화면 밖이면 그린 행의 위치와 행 번호로 입력 자리를 계산해 표 밖에서 그린다.
  - 타입어헤드·새 항목 고르기처럼 화면 밖 항목으로 가야 할 때는 그 항목의 자리를 행 높이로 계산해 가운데로 스크롤한다.
  - list·grid 의 항목 위젯 id 는 화면 위치가 아니라 항목 경로에 묶는다. 스크롤해도 누름·hover 상태가 같은 항목에 남는다. detail 은 공용 `Table` 의 `virtual_rows`·`scroll_to_row` 를 쓴다.
  - Find 로 이름을 거르면 맞는 항목의 목록 번호를 한 번 만들어 두고(`find.rs` 의 `refresh_filter`, Find 바를 그릴 때) 검색어와 목록이 그대로인 동안 다시 쓴다. 목록은 `set_entries` 로 바꿀 때마다 세대가 오르고, 거른 결과는 그 세대·목록 주소·길이와 검색어가 모두 같을 때만 쓴다. 맞지 않는 프레임에는 그 자리에서 다시 거른다. 그래서 거르는 동안 목록 전체의 이름을 보는 것은 검색어나 목록이 바뀐 프레임뿐이다. 검색어나 하위 폴더 검색 여부가 바뀌면 보이지 않게 된 항목을 선택에서 뺀다(`retain_shown_selection`). 선택과 anchor 가 모두 없으면 이 정리를 건너뛴다. 있으면 지금 보이는 목록(`shown`: 거른 결과, 하위 폴더 검색 결과, 거르지 않을 때는 목록 전체)을 한 번 훑어 선택되었거나 anchor 인 경로만 모으고, 그 안에 없는 선택과 anchor 를 푼다.
- detail 행은 **행 전체가 클릭 타겟**이다 — 파일 이름·Size·Date·Type 글자 위에서도 좌클릭 선택 / Ctrl·Cmd+클릭 토글 / Shift+클릭 범위 선택 / 더블클릭 열기(Navigate·OpenFile) / 우클릭 컨텍스트 메뉴가 동일하게 동작한다. 그 대가로 셀 텍스트를 드래그로 선택·복사할 수는 없다(대체: 우클릭 "경로 복사"). 이 정합은 공용 `Table` 이 selectable 모드에서 셀 라벨 선택성을 끄는 계약으로 보장한다 — [`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../../architecture/ui-widgets-crate.md). 헤더 컬럼 제목 클릭(정렬 토글)은 영향을 받지 않는다. 우클릭이 좌클릭과 섞이지 않고 스크롤한 뒤에도 그 행 번호로 오는지는 공용 Table 의 회귀 검사가 본다([공용 위젯 크레이트](../../architecture/ui-widgets-crate.md)).

### 주소 입력

`Enter`·Go 의 입력은 `src/adapters/ui/surface/explorer/address.rs` 가 이동할 폴더로 바꾼다. 앞뒤 공백은 지운다. 빈 입력은 아무것도 하지 않는다. 이동하지 않으면 무반응으로 끝내지 않고 이유를 오류 toast 로 알린다.

로컬 explorer:

- 절대 경로는 그대로, 상대 경로는 프로세스 cwd 가 아니라 **현재 보고 있는 폴더(current)** 기준으로 잇는다.
- `~` 와 `~/…`(Windows 는 `~\…` 도)는 로컬 홈 아래로 펼친다. `~name` 은 펼치지 않고 그 이름의 상대 경로로 본다. 홈을 찾지 못하면 거부한다.
- Windows 에서 드라이브만 있는 입력(`C:`, `C:dir`)은 프로세스의 드라이브별 현재 폴더가 아니라 그 드라이브의 루트 기준이다. UNC(`\\server\share\…`)는 절대 경로다.
- `.` 과 `..` 은 글자 그대로 정리해 절대 경로로 확정한다. 링크를 풀지 않으므로 폴더 링크 안에서 `..` 은 링크의 부모다. 루트 위의 `..` 는 루트에 머문다.
- 확정한 경로가 폴더(폴더를 가리키는 링크 포함)면 그 폴더로 간다. 파일(파일을 가리키는 링크 포함)이면 파일을 열지 않고 그 파일이 든 폴더로 가서, 목록이 들어오면 그 파일을 고르고 보이는 곳으로 스크롤한다. "들어 있는 폴더에서 보기"와 같은 `reveal_after_load` 를 쓰므로 열린 Find 바는 닫힌다. 링크는 풀지 않는다. 파일 링크와 대상이 없는 링크는 링크 대상이 든 폴더가 아니라 링크가 든 폴더에서 링크를 고른다. 대상이 없는 링크도 목록에 항목으로 보이기 때문이다. 이동은 폴더 이동과 같은 `Navigate` 라 뒤로 히스토리와 최근 폴더에 남는다. 지금 보는 폴더의 파일이면 이동 없이 고르기만 한다.
- 없으면 "Path not found: {경로}"(`explorer.address.not_found`, 경로는 확정한 절대 경로), 권한 거부 등 확인하지 못하면 OS 가 알려 준 이유로 거부한다. 주소창이 폴더와 파일을 모두 받으므로 문구와 빈 주소창의 안내(`explorer.address.placeholder`, "Go to folder or file…")는 폴더가 아니라 경로·파일을 말한다.
- 공백·Unicode 이름은 그대로 경로의 일부다.

mirror(원격) explorer:

- 로컬 파일시스템을 보지 않는다. 입력은 원격 호스트의 경로로 다룬다.
- `~` 와 `~/…`·`~\…` 는 원격 홈 아래다. 원격 홈은 연결(재연결 포함)마다 한 번 조회한다. 빈 경로의 `list_dir` 를 보내면 서버가 자기 홈을 읽고 그 경로를 돌려준다(`src/app/attach_client/home_probe.rs`). 응답 전이거나 조회에 실패하면 원격 홈을 모르므로 거부하고 전체 경로를 적으라고 알린다. 로컬 홈으로 펼치지는 않는다. `~name` 은 펼치지 않고 그 이름의 상대 경로로 본다.
- 절대 경로(`/…`, `\…`, `X:\…`, `X:/…`)는 그대로, 나머지는 현재 원격 폴더 뒤에 잇는다. 잇는 구분자는 원격 현재 경로를 따른다(경로에 `\` 가 있으면 Windows 형식으로 보고 `\`, 아니면 `/`). 파일 선택기의 원격 경로 결합(`join_dir`)과 같은 함수다. 그래서 Windows 클라이언트에서도 POSIX 원격 경로를 `/` 로 잇는다.
- 그 뒤 `.` 과 `..` 을 글자 그대로 정리한다(`explorer/address/remote.rs`). 로컬 OS 의 경로 규칙이 아니라 원격 경로의 형식을 따른다. POSIX 형식은 `/` 루트, Windows 형식(`\` 나 드라이브 접두가 있는 경로)은 드라이브 루트(`C:\`)나 UNC 공유(`\\host\share\`)를 루트로 보고 `/` 도 구분자로 받아 `\` 로 잇는다. 원격 링크는 풀지 않으므로 폴더 링크 안의 `..` 은 링크의 부모다. 루트 위의 `..` 는 루트에 머문다.
- 파일 경로도 로컬과 같은 규칙이다. 원격 경로가 파일인지는 로컬에서 알 수 없으므로 이동하기 전에 부모 폴더의 원격 목록을 한 번 요청한다(`explorer/address/probe.rs`). 응답에 같은 이름의 폴더가 아닌 항목이 있으면 부모 폴더로 가서 그 파일을 고르고, 받은 목록을 그 폴더의 목록으로 써서 다시 요청하지 않는다. server 는 링크 항목의 종류를 대상으로 정해 보내므로 폴더 링크는 폴더, 파일 링크와 대상이 없는 링크는 파일로 보여 로컬과 같은 결과(링크를 풀지 않고 링크가 든 폴더에서 고르기)가 된다. 받은 목록의 링크 상태도 그대로 쓰므로 옮긴 뒤 대상이 없는 링크는 아래 "심볼릭 링크" 표시로 보인다. 링크 상태를 싣지 않는 server 의 목록도 종류는 같은 규칙이라 이동 결과는 같다. 폴더이거나, 그 이름이 없거나, 목록을 받지 못했거나, 목록 조회 제한 시간(8초) 안에 응답이 없으면 입력한 경로로 간다. 확인을 기다리는 동안 남은 시간 뒤의 다시 그리기를 예약하므로 입력이 없어도 제한 시간에 이동한다. 기다리는 동안 주소창은 현재 폴더로 돌아가지 않고 입력한 경로와 포커스 테두리를 그대로 둔다(`sync` 가 확인이 있는 동안 버퍼를 덮지 않고, PathField 의 `pending` 이 포커스 없이도 포커스 테두리를 그린다). 목록과 상태줄은 바꾸지 않는다. `explorer_address_pending_delay`(→ motion-ui-fade 200ms)가 지나도 응답이 없으면 주소창 뒤 칸에 `explorer_address_pending_size`(→ icon-size-sm) Spinner 를 spinner-indicator 색으로 보이고, 툴팁은 `explorer.address.checking`("Checking the remote path…")이다(`explorer/address/field.rs`). 로컬 explorer 는 바로 답하므로 이 상태가 없다. 그 탐색기에 포커스가 있을 때 `Esc` 는 확인을 버리고 주소창을 현재 폴더로 되돌린다(`shortcuts/explorer_keys.rs` 의 `cancel_explorer_address_check`, popup 이 떠 있으면 popup 이 먼저 받는다). 기다리는 동안 주소창을 다시 눌러 편집하면 그 확인도 버린다. 응답을 기다리는 동안 다른 폴더로 가면 확인을 버리고, 새 입력을 확정하면(거부되는 입력과 빈 입력 포함) 이전 확인을 버린다. 루트처럼 부모가 없는 경로는 확인 없이 바로 간다. 부모 폴더 목록이 크기 제한으로 잘려 그 이름이 빠지면 입력한 경로로 가고, 잘렸다는 경고 toast 가 함께 뜬다.
- 원격 파일 확인의 새 부모 목록은 이미 같은 폴더를 표시 중이어도 한 번 적용한 뒤 파일을 고른다. 응답별 적용 여부를 원격 캐시에 함께 보관하므로, 그 뒤 경로·정렬·응답이 그대로인 프레임에서는 목록 복제·정렬·세대 증가를 반복하지 않는다. 다른 폴더의 캐시 응답은 현재 목록을 다시 만들지 않는다.
- 존재 여부는 이동 뒤 원격 목록 조회가 알려 준다. 원격 조회 실패는 목록 자리의 읽기 오류 화면(`Error`/`NoPermission`)으로 보인다.

### 미리보기 패널

툴바 view 묶음의 미리보기 토글(`COLUMNS` 글리프, Find 다음. 좁은 칸은 More 메뉴의 Show preview·Hide preview)이 목록 오른쪽 패널을 켜고 끈다(시안 `YPreview`, `explorer/preview.rs`). View(`ExplorerView::preview`)가 토글·폭을 바로 바꿔 그리고, 토글하거나 경계선 끌기를 마쳤을 때 한 번 `EngineAction::ExplorerPreview` 로 model(`ExplorerPanel::preview`)에 남긴다. 그래서 레이아웃 snapshot 에 실려 재시작 뒤에도 남는다(위 "저장·복원"). View 는 view 와 model 이 마지막으로 같았던 값을 기억하고, 그릴 때마다 model 값이 그 값과 다르면 model 값을 받아 온다. 처음 그릴 때는 복원한 값을 받는다. 같은 surface id 로 다른 kind 로 바꿨다가 explorer 로 돌아오면 view 는 id 로 남지만 새 `ExplorerPanel` 은 기본값이라, 그 값을 받아 화면과 저장되는 값이 같다. 끌기 중처럼 view 만 바뀐 동안에는 model 이 그대로라 덮어쓰지 않는다. 단축키 `explorer_toggle_preview` 도 같은 경로다.

- **폭**: `explorer_preview_width`(288)에서 시작하고 패널 왼쪽 경계선(잡는 폭은 pane 분할선과 같은 `DIVIDER_HIT_THRESHOLD`)을 끌어 `explorer_preview_min_width`(200)…`explorer_preview_max_width`(460) 사이로 바꾼다. 목록에는 `explorer_list_min_width`(200)를 남긴다. 칸이 패널 하한 + 경계선 + 목록 하한(합 400 + 경계선)보다 좁으면 패널만 숨기고 토글은 켜진 채 둔다.
- **대상**: 선택이 정확히 하나일 때 그 항목. 하위 폴더 검색 결과를 골랐으면 그 결과다(`ExplorerView::selected_shown`, 보이는 목록에서 고른 항목). 선택이 없으면 "Select a file" 상태다. 여럿이면 머리에 "N items" 와 파일·폴더 수, 본문에 layers 글리프와 `explorer.preview.multi`("{n} items selected")·`multi_sub`("Select one file to preview it.")를 보인다. 대상이 바뀌면 이전 미리보기를 바로 지우고 Loading 상태를 보인다. 같은 항목이라도 아래 내용 키가 바뀌면 다시 읽는다.
- **내용 키**(`DirEntryInfo::content_key`, 미리보기와 Grid 썸네일 공통): 수정 시각 · 크기 · Unix 의 inode 와 상태 변경 시각(ctime)이다. 경로와 함께 이 값이 같으면 같은 내용으로 보고 다시 읽지 않는다. inode·ctime 은 목록이 이미 읽은 metadata 에 들어 있어 조회가 늘지 않는다(`fs_list.rs` 의 `ChangeStamp`). 값은 목록을 읽을 때 정해지므로, 폴더 안 파일만 바뀐 변경은 [외부 변경 확인](#외부-변경-확인)의 한계대로 F5 로 목록을 다시 읽은 뒤 반영된다.
  - `cp -p`·`rsync -t`·압축 해제처럼 수정 시각을 되돌린 교체는 크기가 다르면 모든 OS 에서 새로 읽는다. Unix(Linux·macOS)는 ctime 을 사용자가 되돌릴 수 없어 크기도 시각도 같은 교체도 새로 읽는다. 권한·소유자만 바꿔도 ctime 이 올라 한 번 다시 읽는다.
  - 한계: Windows 는 std 가 변경 시각과 파일 ID 를 안정 API 로 주지 않아(파일을 따로 열어야 한다) 크기와 수정 시각이 모두 같은 교체는 옛 미리보기·썸네일이 남는다. Unix 에서도 ctime 은 커널의 거친 시계로 매겨져, 같은 틱(수 ms) 안에 크기·수정 시각·inode 가 같은 교체가 두 번 일어나면 구분하지 못할 수 있다. 원격(mirror) 항목은 OS 값이 없지만 원격은 미리보기·썸네일을 만들지 않는다.
- **머리**: 높이 40, 이름 · 종류 · 크기(그림은 픽셀 크기도) 두 줄. 머리 아래 남은 높이가 `explorer_preview_body_min_height()`(120) 미만이면 본문과 머리 아래 구분선을 숨긴다. 정확히 120이면 본문을 표시하며 폭·토글 상태는 유지한다.
- **본문**: 읽기는 App read worker(`local_reads::preview`)가 한다. 텍스트(NUL 이 없는 UTF-8)는 앞 64 KB(`PREVIEW_TEXT_BYTES`)를 mono 로 스크롤해 보인다. 그림(`decodable_image_ext`)은 패널에 맞추되 원래 크기보다 키우지 않는다. worker 가 그림을 패널이 가장 넓을 때의 그림 영역 폭(`explorer_preview_max_width` − 2 × `spacing_md`, 물리 px)과 `PREVIEW_TEXTURE_SIDE`(2048, egui `max_texture_side` 의 보장 하한) 상자에 맞춰 Triangle 필터로 줄여 보낸다. egui-wgpu 텍스처에는 밉맵이 없어 GPU 가 크게 줄이면 앨리어싱이 생기므로, 패널을 좁히거나 넓혀도 GPU 의 추가 축소는 패널 최소 폭에서도 약 2.5 배(scale 1 그림 영역 436 → 176)에 그친다. 배율이 바뀌어 이 폭이 달라지면 다시 읽는다. 썸네일은 더 싼 면적 평균으로 줄인다. 머리의 픽셀 크기는 원본 값이다. 정규 파일만 연다. 폴더·FIFO·장치 파일 같은 특수 파일과 그 밖의 형식은 "No preview for this file type" 이다. 1 MiB(`PREVIEW_MAX_BYTES`, 앱 상한) 초과는 "Too large to preview" 와 "Over 1 MiB." 이다. 상한 문구의 이진 단위는 모두 MiB 이고 숫자와 단위 사이는 NBSP 다. 그림은 디코딩 전에 `image::Limits` 로 한 변 16384px(`MAX_IMAGE_SIDE`)·디코딩 메모리 256 MiB(`MAX_DECODE_ALLOC`)를 넘는지 보고, 넘으면 같은 "Too large to preview" 에 픽셀 상한 문구("Over 16384 px on a side, or needs more than 256 MiB to decode.", 메모리는 이진 단위 MiB)를 붙이고, 머리글에서 읽은 실제 크기를 이유 줄("20000 × 14000 px")과 머리의 종류 뒤("TIF image · 20000 × 14000 · 61.0 MB")에 보인다. 머리글도 읽지 못하면 이유 줄이 없다. 읽기 실패는 "Can't read this file" 과 오류 문구다.
- **결합 문자**: egui 의 글자 배치(epaint)는 글자마다 글리프 하나를 놓고 shaping 을 하지 않아, NFD 로 분해된 글자(`e`+U+0301, 한글 첫가끝 자모)를 앞 글자에 합치지 않고 따로 그린다. 미리보기 본문과 머리 이름, Properties 의 제목과 필드 값은 그리기 직전에만 NFC 로 합성한다(`explorer/display_text.rs` 의 `nfc`, `icu_normalizer`). 읽은 내용·항목 이름·경로·Copy 로 넣는 값·검색 대상은 원래 문자열 그대로다. 합성형이 없는 결합 순서(예: `q`+U+0301)는 NFC 로도 합쳐지지 않아 여전히 따로 그려진다. List·Detail·Grid·폴더 트리·즐겨찾기·안쪽 탭·바깥 탭(Explorer일 때)의 이름과 검색 결과의 경로 표시도 그리는 문자열만 NFC로 만든다. 공용 `explorer_match_job`은 먼저 원문에서 `match_range`로 찾고 그 범위를 표시 문자열에 옮긴다. NFC로 바뀐 grapheme 일부가 맞으면 그 묶음 전체를 강조하며, 바뀌지 않은 묶음은 기존 범위를 유지한다. 그래서 결합 표지·자모의 일부를 찾아도 합성된 글자를 바이트 중간에서 자르지 않는다. 검색 일치 자체는 정규화하지 않는다. NFC query가 NFD 원문을 찾는 기능은 제공하지 않으며, `ext:` 조건은 강조하지 않고 FindQuery의 이름 조건만 강조한다.

주소창은 `PathField::display_nfc(true)`로 비편집 글자만 합성한다. 긴 비편집 경로는 끝을 보이게 clip하며, 합성으로 바뀐 표시에서 편집에 들어가면 원문 전체를 선택한다. 편집 중·자동완성 후보는 원문이고, 선택 커서는 원문 문자 수를 쓴다. `TextEdit` 버퍼는 표시 값으로 교체하지 않으므로 Copy·Go·Enter·후보 선택·경로 확인·이름 변경의 초기값에 합성된 다른 경로가 들어가지 않는다. 포커스 이탈/취소는 원문 현재 경로로 돌아간다. 다른 PathField 소비자는 기본(false)을 유지한다.
- **원격**: mirror explorer 는 파일 내용을 받을 경로가 없어 대상과 관계없이 지원하지 않는 형식 상태를 보인다. 미리보기의 보이는 항목 기준과 썸네일 캐시 조회에 원격 여부를 넣어, 같은 경로를 로컬로 읽은 내용·그림을 mirror 에서 보이지 않는다.

### 숨김 파일

이름이 `.` 으로 시작하는 항목(`..` 행 제외)과 OS 가 숨김으로 표시한 항목을 숨김 파일로 본다(`explorer/view/hidden.rs` 의 `is_hidden`). OS 표시는 Windows 의 파일 속성 `FILE_ATTRIBUTE_HIDDEN` 과 macOS 의 BSD 파일 플래그 `UF_HIDDEN`(`chflags hidden`)이다. 목록을 읽을 때 `read_dir_entries` 가 항목 자신의 metadata(링크면 대상이 아니라 링크 자신)에서 읽어 `DirEntryInfo::os_hidden` 에 남긴다(`src/core/fs_list/os_hidden.rs`). Linux 에는 OS 표시가 없어 점 규칙만 쓴다. 점 규칙은 세 OS 모두 적용한다. 원격 탐색기는 원격 server 가 `list_dir_result` 항목에 실은 `os_hidden` 을 쓴다(server 의 `content_queries/list_dir_wire.rs`, client 의 `attach_client/wire/list_dir.rs` → `output/remote_entry.rs`). server 는 원격 호스트에서 같은 `read_dir_entries` 로 읽은 값을 표시가 있는 항목에만 싣는 선택 필드로 보낸다. 이 필드를 싣지 않는 옛 server 에 붙으면 client 는 오류 없이 거짓으로 읽어 점 규칙만 쓰고, 이 필드를 모르는 옛 client 는 무시한다. 원격 목록과 사이드바 트리 모두 같은 `is_hidden` 을 거친다. 기본은 끔이다. 끈 동안 `set_entries` 가 새 목록에서 숨김 항목을 빼 따로 두고(`HiddenFiles`), 상태줄은 "{n} items · {h} hidden"(`explorer.status.hidden`)처럼 뺀 수를 덧붙인다. 뺀 항목이 없으면 덧붙이지 않는다. 켜면 뺀 항목을 목록에 되돌려 정렬 자리로 넣고, 세 보기 모두 숨김 항목의 이름과 글리프를 explorer-hidden-fg(→ text-muted)로 그린다. 불투명도로 흐리게 하지 않는다. 선택한 항목의 이름은 선택 색(text-primary)이다. 색 규칙은 `explorer/entry_style.rs` 가 세 보기에 공통으로 정하고, List 보기는 공용 `tasty_ui_widgets::tree_row_with_tail_fg` 로 평상시 이름 색만 바꾼다(링크 꼬리 자리도 그대로 뺀다). 대상이 없는 링크는 숨김 항목이어도 링크 경고 글리프와 색을 쓴다. 끄면 숨김 항목의 선택을 푼다. 사이드바 트리도 같은 토글을 따라, 끈 동안 숨김 폴더를 뺀다(`tree_children_of` → `tree_shown`). 트리 캐시에는 모두 남겨 토글만으로 다시 읽지 않고 보인다. 하위 폴더 검색도 같은 토글을 따른다. 끄면 숨김 파일·폴더를 결과에서 제외하고 숨김 폴더 안으로도 들어가지 않는다. 토글이 바뀌면 진행 중이거나 끝난 검색의 영수증과 결과를 버리고 같은 시작 폴더·검색어로 다시 찾는다. 이전 채널을 닫으므로 늦게 온 결과가 새 검색에 섞이지 않는다.

필드가 없는 옛 레이아웃도 숨김 기본값은 끔으로 복원한다. 현재 snapshot도 끔이면 `show_hidden`을 생략하므로 필드 부재만으로 기능 도입 전 데이터와 이미 끔으로 저장한 데이터를 구별할 수 없다. 없는 필드를 켜짐으로 이전하면 후자의 선택을 뒤집는다. 별도 버전 표식을 소급 추정하지 않고 지금 기본값을 유지하며, 사용자는 같은 토글로 즉시 표시할 수 있다. `builtins.rs`의 `old_explorer_json_without_hidden_keeps_the_off_default`가 JSON 역직렬화·복원 결과를 확인한다.

토글 경로는 툴바 view 묶음의 눈(`eye`) 토글, More 메뉴의 행(id 84, 지금 상태가 아니라 할 동작을 쓴다: `explorer.more.hidden_show` "Show hidden files" · `hidden_hide` "Hide hidden files"), 단축키 `explorer_toggle_hidden` 이다. 넓은 칸에서는 툴바의 눈 토글이 켜진 상태를 강조로 보이고 툴팁에 More 행과 같은 동작 문구와 첫 바인딩을 붙인다(`commands.rs` 의 `hidden_tip`). 좁은 칸에서는 view 묶음이 create 묶음과 함께 More 로 접혀 More 행으로 바꾼다. View 가 값을 바로 바꿔 그리고, 렌더 뒤 `EngineAction::ExplorerHidden` 으로 model(`ExplorerPanel::show_hidden`)에 남겨 레이아웃 snapshot 에 싣는다. model 값을 받는 규칙은 미리보기 패널과 같다(마지막으로 맞춘 값과 다를 때만, 처음 그릴 때는 복원한 값).

### Grid 썸네일

Grid 셀은 모두 `explorer_grid_thumb_size`(40) 슬롯을 잡아 썸네일 유무와 관계없이 행 높이가 같다(셀 높이 +24, 시안 "Grid thumbnails", `explorer/thumbs.rs`). 로컬 explorer 의 그림 파일 중 1 MiB 이하인 정규 파일만, 화면에 보인 셀부터 read worker 가 긴 변 80px 로 줄여 만든다. 미리보기와 같은 픽셀 상한을 넘으면 디코딩하지 않는다. 썸네일은 40×40 에 맞추고(키우지 않는다) 1px separator 테두리와 radius-sm 을 둔다. 만드는 동안·상한 초과·디코딩 실패는 16 글리프(accent-info)를 그대로 둔다. 캐시(`ExplorerView::thumbs`)는 경로와 미리보기와 같은 내용 키(위 "미리보기 패널")로 맞추고 512 개까지 둔다. 차면 그 프레임에 보이지 않은 항목 중 가장 오래전에 보인 것부터 버리므로 그림이 512 개를 넘는 폴더도 스크롤한 자리의 썸네일을 만든다. 한 프레임에 보이는 셀이 512 개를 넘으면 넘는 셀은 글리프로 둔다. list·detail 보기와 mirror explorer 는 썸네일을 만들지 않는다.

### Properties popup

`explorer_properties` popup(`src/adapters/ui/popup/explorer_properties.rs`, 시안 `YProps`)은 그 explorer surface 에 묶인(`PopupScope::Surface`) headless popup 이다. 바탕은 시안 YProps 와 같은 bg-panel 이다(읽기 실패 화면의 Secondary Retry 가 surface-raised 바탕에 묻히지 않는다). 폭은 `explorer_props_width`(360)이고, 칸이 그보다 좁아 팝업 관리자가 팝업을 칸 안으로 줄이면 내용과 footer 구분선도 줄어든 폭을 쓴다(머리 이름은 말줄임, 경로는 어디서나, 시각은 부분 사이에서 줄바꿈). 라벨 열은 `explorer_props_label_width`(96, caption text-muted)이고 값은 body 또는 mono caption 이다. 연 대상을 고정해 보이며, 원 explorer 가 사라지면 닫힌다.

- **진입**: 컨텍스트 메뉴 맨 끝 구분선 뒤 "Properties"(id 70, 모든 변형·mirror 에서도 보인다), `explorer_properties` 단축키·Command Palette. 대상 규칙은 [파일 작업 계약](file-operations.md#대상-결정-규칙)이다.
- **종류 문구**(`explorer/kind.rs` 의 `kind_word`·`file_kind_word`): Detail 의 Type 열, Properties 의 Kind, 미리보기 머리가 같은 낱말을 쓴다. 확장자 분류(`core/file_kind.rs` 의 `FileKind`)는 낱말과 Type 정렬이 함께 쓴다.
  - 형식 이름 표: md·markdown "Markdown", txt "Text", html·htm "HTML", json "JSON", toml "TOML", yaml·yml "YAML", csv "CSV", pdf "PDF document", zip·tar·gz·tgz·bz2·xz·zst·7z·rar "Archive", sh·bash·zsh·fish·ps1 "Shell script". HTML·JSON·TOML·YAML·CSV 는 모든 언어에서 같은 낱말이라 번역 키가 없다.
  - 표에 없는 그림 확장자(`is_image_ext`)는 `explorer.kind.image`("PNG image"), 그 밖은 `explorer.kind.ext_file`("RS file"), 확장자가 없으면 "File", 폴더는 "Folder".
  - 링크는 Type 열과 미리보기 머리에서 `explorer.kind.link_to`("Link to Markdown")이고 크기는 대상의 크기다. 링크의 종류는 링크 이름의 확장자와 대상이 폴더인지로 정한다. 대상이 없는 링크는 `explorer.kind.broken_link`("Broken link")다. Properties 의 Kind 는 "Symbolic link" 또는 "Broken link" 다. Target kind 는 실제 링크 체인을 푼 대상의 종류를 쓴다.
  - Type 열은 좁아 낱말이 넘치면 말줄임하고 호버에 전체 낱말을 보인다. Type 열 정렬(`core/fs_list.rs` 의 `type_key`)은 보이는 낱말과 같은 묶음이 붙어 있도록 링크 여부(링크 아님 · 링크 · 끊긴 링크), `FileKind` 선언 순서, 확장자, 이름 순으로 비교한다. 번역문을 비교하지 않으므로 언어와 무관하다(예: gz 와 zip 은 Archive 로 붙는다).
- **파일·폴더 필드 순서**: Kind · Size(사람이 읽는 크기와 바이트 수 또는 폴더 집계) · Modified · Created · Path(항목 자신의 절대 경로, mono, Copy) · Permissions(Unix 는 `rwxr-xr-x` 와 read-only, 그 밖은 read-only 만). 없는 시각은 대시다. 값 글자는 `explorer_props_value_fg`(→ text-secondary)다.
- **시각 형식**: Modified · Created · Target modified 는 설정 `general.explorer_props_time_format`(설정 › 일반 "Time format in Properties", Language 다음 행)의 형식을 쓴다(`explorer/props_time.rs`). `minutes`(기본, `%Y-%m-%d %H:%M`) · `seconds`(`%Y-%m-%d %H:%M:%S`) · `seconds_offset`(`%Y-%m-%d %H:%M:%S %:z`)이고 모두 이 컴퓨터의 시간대다. 오프셋은 OS 마다 다르고 모호한 약어 대신 `+09:00` 같은 숫자이며, 그 시각의 일광 절약 여부를 반영한다. 모르는 값은 기본값으로 보인다. 팝업은 그릴 때마다 설정을 읽으므로 열린 팝업도 다음 프레임에 바뀐다. 시각 값은 날짜 · 시각 · 오프셋 사이에서만 줄을 바꾼다(가장 긴 26 자는 360 폭에서 한 줄이다). 설정 행 caption 은 바뀌는 필드를 말하고 고정 로컬 시각 2026-10-21 12:12:05 를 고른 형식으로 mono 예시로 붙인다. 원격 항목의 Modified 도 같은 형식이다.
- **Open**: 한 항목이면 필드 아래 1px `explorer_props_footer_border` 구분선 위 footer 에 오른쪽 정렬 secondary sm "Open"(`explorer.props.open`)을 둔다. 누르면 팝업을 연 항목에 목록의 Enter 와 같은 동작을 한다: 목록에 보이는 항목이면 Enter 가 쓰는 `open_action`(폴더·폴더 링크는 `Navigate`, 그 밖은 `OpenFile`)을, 목록에서 사라진 로컬 항목이면 읽은 종류로 같은 동작을 만들어 `apply_explorer_action` 에 보낸다. 파일은 파일 핸들러 설정을 따르고 원격 항목은 Enter 와 같이 원격 파일 열기(`DispatchFile`)로 간다. 팝업은 열린 채 남는다. 폴더로 들어가도 팝업은 연 대상을 계속 보이며, 원 explorer 가 닫힐 때만 닫힌다. 대상이 없는 링크는 버튼을 끄고 호버에 `explorer.props.target_missing` 을 보인다. 여러 항목, 읽는 중, 읽기 실패, 지금 보는 폴더 자신(빈 곳에서 연 Properties: Enter 로 열 항목이 아니다)에는 footer 가 없다. 닫기 버튼은 Tab 순서가 필드 버튼과 Open 뒤에 오도록 마지막에 만든다.
- **권한 편집**(`explorer_properties/perm_edit.rs`): 로컬의 링크 아닌 한 항목이면 Permissions 값 끝에 ghost sm "Edit" 를 둔다. 누르면 행 아래에 상자(1px `explorer_props_perm_border`, radius, padding·gap space-sm)를 열고 Edit 를 숨긴다. 상자는 Unix·Windows 모두 라벨 열을 포함한 내용 폭 전체(360 팝업에서 330)를 쓴다. 값 열(약 228)은 격자(칸 4 × `explorer_props_perm_cell_width` 56 + 안쪽 여백 + 테두리 = 242)보다 좁기 때문이다. 값 글자·Edit·소유자 아님 안내는 값 열에 남는다. 상자는 공용 `tasty_ui_widgets::perm_box` 가 그리며 갤러리 예제도 같은 함수를 부른다. Unix 는 소유자 · 그룹 · 기타 × 읽기 · 쓰기 · 실행 3×3 체크박스(칸 폭 `explorer_props_perm_cell_width`, 격자는 상자 왼쪽 정렬)와 바뀔 모드의 `rwxr-x--- · 750` 읽기 표시, 폴더면 "이 폴더만 바꾼다", setuid·setgid·sticky 가 있으면 "그대로 둔다" 안내를 보인다. 열 머리(가운데 정렬, 줄바꿈하면 위로 자라도록 아래 맞춤)와 역할명(왼쪽 정렬, 행 가운데)은 56 칸 안에서 줄바꿈하고 말줄임하지 않으며, 줄바꿈한 행은 그만큼 높아진다. 상자는 받은 내용 폭을 넘지 않고, 상자 자신의 바깥 폭(테두리 포함)으로 배치를 고른다(`perm_layout`). 상자 폭·경계·칸 위치와 버튼 폭(`Button::desired_width`)은 내부에서 `LogicalPx` 로 계산하고, egui 에 넘기는 자리에서만 값을 꺼낸다([타입 있는 길이](../../concepts/typed-length.md)). `explorer_props_perm_grid_min_width`(칸 4 × 56 + 안쪽 여백 + 테두리, 배율 1에서 242) 이상이면 위 격자다. `explorer_props_perm_stack_min_width`(칸 3개, 186) 이상이면 쌓기 배치로, 역할 열 없이 열 머리 아래 56 칸 3개를 두고 역할명을 자기 칸 줄 위의 한 줄(어디서나 줄바꿈, 아래 간격 `explorer_props_perm_role_gap`)에 둔다. 그보다 좁으면 목록 배치로, 열 머리 없이 역할명 아래 읽기·쓰기·실행 라벨 붙은 체크박스를 한 줄(`explorer_props_row_min_height`)씩 두며(역할 묶음 사이 space-sm) 폭 하한이 없다. 체크박스 라벨은 어디서나 줄바꿈한다. 두 경계는 디자인의 `calc()` 토큰이며, 생성된 Theme 접근자가 배율·반올림이 적용된 칸·여백·테두리를 더한다. 칸 폭이 아니라 상자 폭과 비교하며, 상자 폭은 팝업이 칸에 맞춰 줄어든 실제 내용 폭이다. 배치는 그리는 프레임마다 다시 고르므로 칸 분할선을 끌면 바로 바뀐다. 체크 값·바꾼 값·적용 중·실패 상태는 호출자가 가지고, 체크박스 id 는 역할·열로 정해 배치가 바뀌어도 키보드 포커스가 남는다. Tab 순서는 세 배치 모두 소유자 R W X → 그룹 → 기타 → Cancel → Apply 다. 읽기 표시는 모드와 `· 8진수` 사이에서만 줄바꿈한다. Windows 는 "Read-only" 체크박스 하나와 다른 접근 규칙(ACL)은 바꾸지 않는다는 안내다. 격자가 없어 배치를 바꾸지 않고, 좁으면 체크박스 라벨과 안내가 줄바꿈한다. 오른쪽 Cancel(ghost sm)은 상자를 닫고, Apply(primary sm)는 바뀐 값이 있을 때만 켜진다. 두 버튼이 한 줄에 들어가지 않으면 Apply 가 다음 줄로 내려가며 각 줄은 오른쪽 정렬이다. Apply 는 explorer 파일 작업 대기열에 `Operation::SetPermissions` 를 넣는다(로컬 칸만, `file_jobs/permissions.rs`. 소비자가 GUI popup 뿐이고 원격 explorer 요청에는 권한 변경이 없어 이 모듈은 `gui` 기능에서만 컴파일한다). 적용하는 순간 다시 읽은 모드에서 9 비트만 바꾸므로 특수 비트는 남고, 폴더 안으로 내려가지 않으며, 권한 상승을 요청하지 않고 링크는 거절한다. 기다리는 동안 Apply 의 라벨과 폭은 그대로 비활성이고, 버튼 왼쪽에 Spinner 와 "Applying…"(caption `text-muted`)을 보이며 모든 컨트롤을 끈다. 성공하면 상자를 닫고, 실패하면 상자를 둔 채 alertCircle 과 "Couldn't change permissions"(`explorer_props_perm_error_fg`), mono muted OS 이유를 보인다. 상자가 실패를 보이면 일반 파일 작업 실패 토스트는 띄우지 않는다. 둘 다 항목을 다시 읽어 행 값과 체크박스를 실제 값으로 맞춘다. 결과는 요청을 넣은 창의 상자에만 전달한다. 요청 번호는 창마다 따로 세므로, 다른 창이 같은 번호를 기다려도 그 상자는 닫히거나 실패 이유를 받지 않는다(`ui_sync::settle_permissions`). 요청이 시작되지 않고 버려졌으면(`resume_unanswered_permissions`, 그 창이 넣은 실행 중 작업만 대조) 편집 상태로 돌아간다. Esc 는 바꾼 칸을 되돌리지 않는다(팝업 공통 동작만 한다). Unix 에서 소유자도 root 도 아니면 Edit 대신 `explorer_props_perm_locked_fg` 로 "Only the owner can change permissions." 를 보인다. 링크, 여러 항목, 원격, 읽지 못한 항목에는 Edit 가 없다.
- **링크**: 제목 글리프 link · Kind "Symbolic link" · Link target(Copy) · Target kind · Target size · Target modified · Path(링크 자신의 절대 경로, Copy). 상대 경로와 링크 체인은 파일시스템에서 해석해 대상의 메타데이터를 읽으며 링크 자신의 lstat 크기·시각은 표시하지 않는다. 대상이 없으면 Kind "Broken link", 머리 link 글리프는 `explorer_link_broken_fg`, Link target 아래에는 `explorer.props.target_missing` 안내가 나온다. 권한 오류·링크 순환 등은 끊긴 링크로 분류하지 않고 기존 읽기 실패 화면과 Retry로 처리한다. 필드 아래에는 원격 안내와 같은 muted 안내 `explorer.props.link_note`("Deleting or renaming this link doesn't change its target.")를 붙인다. 끊긴 링크도 같다.
- **폴더 집계**: Size 자리에 하위 항목 수와 크기를 read worker 가 배경에서 세며 Spinner 를 보인다. 링크는 항목 수에 포함하지만 따라가지 않고 크기도 더하지 않는다. 하위 폴더·항목 메타데이터를 읽지 못한 부분은 건너뛰고 부분 집계 상태를 남긴다. Size 바로 아래에 alertTriangle(`icon_glyph_size_xs`, 12)과 `explorer.props.partial`을 `explorer_props_partial_fg`로 표시한다. 여러 항목도 Total size 아래에 같은 경고를 보인다. popup 을 닫으면 세기를 멈추며 다시 열기·Retry는 새 집계 상태를 사용한다.
- **여러 항목**: 머리 "N items"(고른 수), Kinds(읽은 파일·폴더 수) · Total size · 공통 Location.
  - **일부 항목을 못 읽음**: worker(`file_info.rs` 의 `read_properties`)는 항목마다 metadata 를 읽어 읽은 항목(`PropertiesFacts::items`)과 읽지 못한 고른 항목(`PropertiesFacts::failed`: 경로와 `io::ErrorKind` 의 쌍 목록, 항목마다 `tracing::warn!` 기록)을 함께 보낸다. 하나라도 읽으면 읽은 항목만 세고 Kinds 아래에 `explorer.props.unreadable_items`("Some items couldn't be read")를 `explorer_props_partial_fg` 경고로 보인다. 모두 못 읽으면 기존 읽기 실패 화면이다. 이 경고는 폴더 집계 중 하위 항목을 건너뛴 `explorer.props.partial`(Total size 아래)과 다른 상태다. 하나만 골랐으면 실패가 곧 전체 실패라 읽기 실패 화면이다.
  - **겹친 선택**: 크기 합과 셀 폴더는 파일 작업과 같은 `drop_nested`(`file_jobs/ops.rs`)로 정한다. 고른 폴더 안에 든 고른 항목(링크를 거치지 않은 실제 폴더 안)은 목록·Kinds 에는 남고 크기는 그 폴더 집계로만 센다.
  - **하드링크**: 링크 수가 둘 이상인 일반 파일을 파일 작업과 같은 식별자(`file_jobs/leftover.rs` 의 `FileId::read_links`)로 기억해 크기를 한 번만 더한다(고른 파일끼리, 폴더 안, 고른 파일과 폴더 안 사이 모두). 항목 수는 이름마다 센다. Unix 는 목록이 이미 읽은 `symlink_metadata` 의 (장치, inode)·링크 수라 조회가 늘지 않는다. Windows 는 std 의 식별자 API 가 stable 이 아니어서 파일마다 `access_mode(0)` + `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT` 로 한 번 열어 `GetFileInformationByHandle` 의 볼륨 일련번호·파일 인덱스·링크 수를 읽는다. 내용 읽기 권한이나 권한 상승은 필요 없다. 이 조회는 폴더 집계 worker 에서 항목마다 하며 취소 확인도 항목마다 한다. 고른 항목이 하나면 가릴 상대가 없어 고른 항목의 식별자는 읽지 않는다.
  - **식별자를 못 읽음**: 같은 파일인지 알 수 없으므로 고른 항목이면 읽지 못한 항목(`failed`, Kinds 경고)으로, 폴더 안 파일이면 크기를 더하지 않고 부분 집계(`explorer.props.partial`)로 다룬다. Windows 에서는 `symlink_metadata` 는 읽혀도 식별자를 읽으려는 열기가 실패하는 파일이 있을 수 있다(예: 시스템이 공유 없이 연 파일). 그런 파일도 같은 규칙을 따라 고른 항목이면 목록·Kinds·합계에서 빠지고 폴더 안이면 크기가 빠진다. 이 경우는 재현해 확인하지 않았고, 식별 실패를 이렇게 처리하는 데서 오는 알려진 한계다.
  - **집계 비용 측정**: `file_info.rs` 의 무시된 시험 `properties_count_cost` 가 식별자 없는 걷기와 `count_folder` 시간을 같은 폴더(기본 20000 파일)에서 세 번씩 `TASTY_PROPS_COST_OUT` 파일에 적는다(`TASTY_PROPS_COST_OUT=<파일> cargo test -p tasty --lib --locked -- --ignored properties_count_cost`, 파일 수는 `TASTY_PROPS_COST_FILES`). Linux 에서는 두 값이 같은 범위다. Windows(NTFS, debug 빌드, 20000 파일)에서는 식별자 없는 걷기 약 0.32 초, `count_folder` 약 0.58 초로 파일마다 여는 비용 때문에 약 1.8 배다. 이 값은 배경 worker 의 집계 시간이며 Properties 팝업의 UI 응답 시간을 잰 것은 아니다. 집계는 worker 에서 돌아 UI 를 막지 않는다.
- **읽기 실패**: 팝업 틀을 그대로 두고, 제목은 그 항목의 글리프(탐색기 목록에서 본 종류)와 이름이다. 본문은 alertTriangle 과 `explorer.properties.unreadable`("Can't read properties", explorer-error-fg), mono muted OS 이유, Retry(secondary sm, 같은 항목을 다시 읽는다)다.
- **제목 굵기**: 제목은 14 regular 다. 테마에 semibold UI 글꼴이 없어 크기로 제목을 구분한다.
- **원격(mirror)**: 원격 목록에 있는 Kind · Size · Modified · Path 만 보이고 그 아래 muted 안내를 붙인다. 원격 파일시스템을 다시 읽지 않는다. 원격 목록에는 폴더 크기가 없어, 폴더 하나의 Size 와 폴더가 섞인 여러 항목의 Total size 는 0 B 가 아니라 알 수 없음 "—"(`human_size` 의 폴더 규칙)이다. 파일만 고르면 파일 크기 합이다.
- **다시 열기**: 같은 surface 에서 다시 요청하면 popup 을 그대로 두고 대상만 바꾸며 이전 읽기·집계를 멈춘다. 다른 surface 에서 요청하면 열린 popup 을 닫고 요청한 surface 범위로 다시 열어, 내용과 범위가 늘 같은 surface 를 가리킨다. Esc 로 포커스만 풀어 popup 이 다른 탭에 남아 있어도 누른 탭에 새 대상의 popup 이 보인다.

### 새 폴더 · 새 파일

툴바의 명령 묶음(`commands.rs`, 공용 `tasty_ui_widgets::explorer_commands`)이 주소창과 보기 전환 사이에 create 묶음(New folder·New file)과 1px 구분선, view 묶음(Find·Preview·숨김 파일 토글)을 icon-only sm 버튼으로 둔다. 숨김 파일 토글은 view 묶음 맨 끝의 `eye` 버튼이고, 숨김 파일을 보이는 동안 눌린(active) 모양이다. 툴팁은 More 행과 같은 동작 문구(`explorer.more.hidden_show`·`hidden_hide`) 뒤에 괄호로 `explorer_toggle_hidden` 의 첫 바인딩을 붙인다(예: "Show hidden files (Ctrl+Shift+.)"). 바인딩 표시는 설정 화면과 같은 `KeybindingSettings::format_display` 이고, 호스트가 프레임마다 `HiddenFiles::shortcut` 에 넣는다(`egui_panels.rs`). 바인딩이 없으면 괄호를 붙이지 않는다. 네이티브 More 메뉴의 숨김 행에는 단축키 칸을 두지 않는다. 칸 폭이 `explorer-toolbar-compact-below`(440)보다 좁으면 두 묶음을 More(`…`) 하나로 접고, 누르면 `ExplorerAction::MoreMenu` → `PendingNativeMenu::ExplorerMore` 로 같은 명령의 네이티브 메뉴를 연다(`src/view/main/explorer_create.rs`, New folder 80 · New file 81 · Find 82 · Preview 83 · 숨김 파일 84). 네이티브 메뉴에는 체크 표시가 없으므로 view 묶음의 행은 누르면 할 동작을 쓴다. Find 바가 열려 있으면 `explorer.more.find_close`("Close find"), 아니면 "Find"이고, 미리보기는 `explorer.more.preview_show`·`preview_hide`, 숨김 파일은 `explorer.more.hidden_show`·`hidden_hide` 다(아래 "숨김 파일"). 원격 쓰기를 받지 않는 server 의 mirror explorer 는 create 묶음을 비활성으로 두고 툴팁을 위 "받지 않는 server" 의 이유 문구로 바꾼다(`commands.rs` 의 `cannot_write_tip`). 로컬 explorer 는 폴더를 읽을 때 읽기 worker 에 쓰기 가능 여부를 함께 묻고(`local_reads::writable`, Unix 는 `access(W_OK)`, 그 밖의 OS 는 확인하지 않고 쓸 수 있다고 본다), 쓸 수 없으면 두 버튼을 비활성으로 두고 툴팁을 `explorer.command.cannot_write` 로 바꾼다. 화면 스레드는 파일시스템을 읽지 않는다.

진입점은 툴바 버튼, More 메뉴, 컨텍스트 메뉴(빈 영역 메뉴의 첫 묶음, 단일 폴더 메뉴의 잘라내기·붙여넣기 뒤 묶음 — id 80·81), 단축키 `explorer_new_folder`·`explorer_new_file` 이다. 모두 이름 입력만 연다(`MainViewState::start_explorer_create` → `ExplorerView::start_create`). 대상 폴더는 시작할 때 고정한다. 폴더 메뉴는 그 폴더, 나머지는 지금 보는 폴더다. 원격 쓰기를 받지 않는 server 의 mirror 는 `MainViewState::refuse_remote_write` 가 그 칸 범위(`ToastScope::Surface`) info 토스트로 거절한다. 문구는 옛 Tasty 면 host 이름을 넣은 `explorer.remote.write_needs_update`, 그 밖의 경우(연결이 끊기는 중 등) `explorer.state.remote_write_unsupported` 다. 쓸 수 없다고 확인한 로컬 현재 폴더는 `cannot_write` 토스트로 거절한다. 폴더 메뉴처럼 지금 보는 폴더가 아닌 로컬 폴더가 대상이면 `ExplorerView::request_create` 가 읽기 worker 에 그 폴더의 쓰기 가능 여부(`local_reads::writable`)를 묻고, 답을 받은 뒤 입력을 열거나 같은 `cannot_write` 토스트로 거절한다(`report_explorer_create_refusals`). 답을 기다리는 동안 다른 폴더로 가면 입력을 열지 않는다. 확인하지 못했거나 Unix 가 아니면 쓸 수 있다고 보고 만들 때의 오류로 알린다. 원격 폴더의 쓰기 권한은 server 의 결과로 알린다.

이름 입력(`create.rs`, 공용 `explorer_name_row`)은 목록 맨 위(`..` 다음)에 열린다. Detail 은 표에 빈 이름의 자리 행을 하나 넣고 그 위에 겹쳐 그리며, 자리 행은 선택·클릭·메뉴 대상이 아니다. List 는 행 높이 28 의 줄, Grid 는 칸 글리프 아래 160 폭 입력이며 입력이 목록의 보이는 영역 밖으로 나가면 안으로 민다. 처음 프레임에 입력에 포커스를 주고 기본 이름을 고르며 그 줄로 스크롤한다.

폴더 행 메뉴로 시작해 대상 폴더가 지금 목록에 있으면 입력을 맨 위가 아니라 그 폴더 바로 다음 자리에 연다(`create/place.rs`, 시안 batch 11 "Create inside a folder"). Detail 은 자리 행을 폴더 행 다음에 끼우고, List 는 폴더 행 다음 줄, Grid 는 폴더 칸 다음 칸이다. Detail·List 의 입력 줄은 글리프 앞을 explorer-create-indent(→ space-lg 16)만큼 들여 쓰고, 입력 칸은 explorer-create-field-max-width(→ size-240)에서 멈추고, 그 뒤 space-sm 간격에 `explorer.create.in_folder`("in {folder}") caption(font-size-caption, text-muted)을 붙인다. 입력 칸 끝이 줄 끝에 닿으면 caption 폭만큼 칸을 줄인다(공용 `explorer_name_row_in` 의 `ExplorerNameOptions`). Grid 칸에는 caption 을 두지 않는다. 입력이 열린 동안 대상 폴더 행·칸에 끌어 놓기 대상과 같은 링·틴트(`DropTintSlot`·`paint_drop_ring`, explorer-drop-target-bg/-border, 모서리 radius)를 그린다. Enter 는 그 폴더 안에 만들고 보던 폴더는 그대로이며, Esc 는 취소한다. 세 보기는 편집 줄 하나를 목록 순번에 끼운 가상 순번으로 보이는 범위를 계산한다. 편집 줄이 보이는 범위 밖이면 계산한 자리에 따로 그리고, 입력에 고정 id(대상 폴더·종류로 만든 `egui::Id`)를 주어 스크롤해도 포커스를 지킨다. 영역 선택·쪽 단위 이동이 읽는 칸 배치(`list_layout`)는 `place::fit_layout` 이 편집 칸에 맞춘다. 맨 앞 편집 칸이면 그 자리만큼 밀고(Grid 는 앞 칸 수, List 는 입력 줄 높이), 항목 사이에 낀 편집 칸이면 칸 간격이 고르지 않아 배치를 두지 않는다. 그동안 영역 선택은 시작되지 않고, 입력 밖을 누르면 입력이 먼저 닫힌다.

- 기본 이름: 폴더 `explorer.new.folder_default` 전체 선택, 파일 `explorer.new.file_default` 확장자 앞까지 선택. 대상 폴더의 이미 아는 이름(지금 목록, 다른 폴더면 사이드바 트리가 읽은 자식)과 겹치면 `"{stem} 2{.ext}"` 부터 비는 번호를 붙인다.
- 입력하는 동안 검사: 빈 이름(공백만인 이름 포함), 금지 글자(모든 OS 에서 `/`·NUL, Windows 는 `<>:"/\|?*` 와 제어 문자), 앞뒤 공백(모든 OS, `explorer.name.edge_space`. 떼지 않고 거절한다. 사용자가 일부러 넣었을 수도 있어 조용히 바꾸면 입력한 이름과 달라진다), 예약 이름(모든 OS 의 `.`·`..`, Windows 의 CON·PRN·AUX·NUL·COM1~9·LPT1~9 — 확장자가 붙어도 같다 — 와 끝이 점인 이름. Windows 는 끝의 점을 떼고 만들어 다른 이름이 되기 때문이다). 이 검사는 새 항목 이름 입력과 이름 바꾸기 대화상자(`rename_name_error`)가 함께 쓴다. 대화상자는 이유를 입력 아래에 보이고 Save 를 막으며, 같은 프레임의 Enter 도 확정할 때 다시 검사해 보내지 않는다. Enter 에서 아는 이름과 겹치는지 본다. 아는 이름은 지금 목록(숨김 파일을 끈 동안 뺀 숨김 항목 포함)이나 사이드바 트리가 읽은 대상 폴더의 하위 항목이다. 대소문자만 다른 이름은 로컬 macOS·Windows 와 Windows 형식 경로의 원격 폴더에서 같은 이름으로 본다(`create::case_insensitive`, 두 OS 의 기본 파일시스템이 대소문자를 가리지 않기 때문이다). Linux 와 Unix 형식 경로의 원격 폴더는 정확히 같은 이름만 겹친다고 보고, 대소문자를 가리지 않는 볼륨이면 만들 때의 오류로 알린다. 기본 이름("New folder 2" 등)을 고를 때도 같은 비교를 쓴다. 오류는 입력 아래 상자(`explorer_name_error`)로 보이고 입력은 열린 채 글자를 유지한다. 글자를 바꾸면 "이미 있음" 오류는 사라진다.
- Enter 는 확정, Esc 는 취소(디스크 변화 없음), 포커스를 잃으면 유효할 때 확정·아니면 취소한다.
- 확정은 `ExplorerAction::Create { dir, name, folder }` 를 내고 `apply_explorer_action` 이 `explorer_files::Operation::Create` 를 요청한다. worker 는 이름이 한 파일 이름인지 다시 확인하고 `create_dir` 또는 `create_new` 로 만들어, 같은 이름이 생겨 있으면 덮어쓰지 않고 실패한다. 실패는 다른 파일 작업과 같은 오류 토스트와 다시 읽기다.
- 성공하면 요청한 explorer 가 아직 그 폴더를 보고 있고 요청한 뒤 선택이 그대로일 때만 새 항목을 선택하고, 다시 읽은 목록에 나타난 프레임에 그 자리로 스크롤한다. 요청 때의 선택 식별자(`selection_identity`)를 결과와 대 보며(`reveal_created_if_unchanged`), 그사이 사용자가 다른 항목을 고르거나 선택을 비웠으면 선택도 스크롤도 바꾸지 않고 다시 읽기만 한다. 같은 폴더를 보는 다른 explorer 는 다시 읽기만 한다.
- 원격 폴더(mirror explorer)는 server 의 답을 기다린다. 확정하면 입력 줄은 제자리에 남아 읽기 전용(input-readonly-bg·fg)이 되고 보낸 글자를 유지한다. 칸 바로 뒤 space-sm 자리에 icon-size-sm Spinner 를 두고, 올려 두면 `explorer.remote.waiting`("Waiting for {host}", host 는 연결의 `remote_label`) 툴팁을 보인다(공용 `explorer_name_row_in` 의 `ExplorerNameOptions::pending`). 기다리는 동안 Esc·Enter·포커스 잃음은 무시한다. 성공하면 App 이 줄을 닫고(`ui_sync::close_created`) 위와 같이 새 항목을 고른다. 성공으로 닫히지 않은 채 요청이 대기열과 실행 중 작업 어디에도 없으면(실패·거절·연결 끊김으로 버려짐) 글자를 둔 채 다시 편집하게 하고 커서를 끝에 둔다(`ui_sync::resume_unanswered_creates`). 실패 이유는 그 경로의 오류 토스트·카드가 알린다. 로컬 폴더는 확정하면 바로 줄을 닫는다.
- 입력이 열려 있는 동안 타입어헤드를 끈다.

### 찾기 · 하위 폴더 검색

툴바 view 묶음의 Find 토글(좁은 칸은 More 메뉴의 Find·Close find, id 82)과 explorer 포커스의 `find` 단축키가 툴바 아래에 Find 바(`find.rs`, 공용 `explorer_find_bar`, 높이 `explorer-search-bar-height` 36, 칸 전체 폭)를 연다. 토글은 열려 있으면 닫고, `find` 단축키는 이미 열린 바에 포커스만 준다. 바 상태(`FindState`: 연 폴더·검색어·Subfolders·하위 폴더 검색)는 내부 탭별로 보관하고 디스크에는 저장하지 않는다. 다른 내부 탭으로 갔다 돌아와도 검색어와 결과가 남으며, 같은 탭에서 폴더를 이동하면 닫는다.

- **거르기**: 입력하는 즉시 지금 보는 폴더의 항목을 이름의 부분 문자열(대소문자 무시, `match_range`)로 거른다. 맞는 부분은 Detail·List·Grid 모두 `accent-primary-raised` 로 칠하고, 선택 행에서는 행 글자색에 밑줄을 긋는다(List 는 공용 `tree_row_matching`). 바 오른쪽에 `explorer.find.count`("{shown} of {total}"), 상태줄에 `explorer.find.status` 를 보인다. 거르는 동안에도 `..` 행은 목록 맨 위에 남는다. 이동용 행이라 이름을 맞춰 보지 않고 개수에도 넣지 않는다. 맞는 항목이 없으면 바는 "0 of N" 을 그대로 두고, 목록 자리에 search 글리프와 `explorer.find.none_filter`("No names match “{query}”", 보조 줄 없음) 상태 화면을 보인다. 새 항목 이름을 입력하는 동안에는 이 화면과 아래 하위 폴더 검색의 결과 없음·실패 화면 대신 목록 영역을 그려 이름 행을 보인다. mirror explorer 도 거르기를 쓴다.
- **보이는 항목만 다룬다**: 검색어나 Subfolders 가 바뀌면 보이지 않게 된 항목을 선택에서 뺀다. 전체 선택·Shift 범위 선택·타입어헤드는 보이는 항목만 대상으로 한다. 숨은 항목에 명령이 닿지 않게 하기 위해서다.
- **확장자 조건**: 현재 입력란에서 공백으로 구분한 `ext:rs`를 쓴다. `ext:rs ext:md`는 둘 중 하나이며 `note ext:rs`는 이름에 note가 들어가고 마지막 확장자가 rs인 파일이다. 폴더는 확장자 조건에 맞지 않는다. 키·확장자·이름은 대소문자를 무시한다. `EXT:.RS`처럼 확장자 앞 점 하나도 허용한다. 확장자 값은 비어 있지 않은 문자·숫자·밑줄·하이픈이며 `ext:`·`ext:*`·`ext:rs,md` 등은 연산자로 버리지 않고 일반 이름 문자열로 찾는다. glob·인용부호 문법은 없다. 유효한 ext 토큰이 있을 때만 나머지 이름 토큰을 공백 하나로 이어 기존 부분 문자열 조건으로 쓴다. 없으면 입력의 공백까지 그대로 찾는다. 강조는 이름 조건에만 적용한다. 로컬 거르기·하위 검색·mirror 거르기가 같은 해석기를 쓰며 mirror의 하위 검색 제한은 그대로다.
- **Subfolders**(로컬만, mirror 는 체크박스를 숨긴다): 켜면 같은 바가 지금 폴더부터의 하위 폴더 검색이 된다. 로컬 읽기 worker(`local_reads::search`)가 너비 우선으로 폴더를 읽어 맞는 항목을 100ms 마다 묶어 보내고 화면을 깨운다. 링크로 된 폴더는 따라 들어가지 않는다. 결과는 목록을 대신하고 `..` 행을 두지 않으며 지금 정렬로 정렬한다. 새 묶음만 정렬해 이분 탐색으로 자리에 끼워 넣으므로 받을 때마다 전체를 다시 정렬하지 않는다. 상태줄·바·상태 화면은 보이는 항목을 복사하지 않고 세거나 참조한다. Detail 은 Type 대신 Name 뒤에 Folder 열(`explorer-search-folder-col-width` 160, mono caption, 시작 폴더 기준 상대 경로, 시작 폴더 자신은 ".")을 둔다. List·Grid 는 행 모양을 바꾸지 않고 든 폴더를 툴팁으로 보인다. 결과 하나를 고르면 상태줄에 시작 폴더 기준 경로를 보인다. 열기·복사·잘라내기·이름 변경·휴지통은 결과의 실제 경로를 쓴다. 파일 작업이 끝나면 요청한 explorer 는 다시 읽으며 검색을 처음부터 다시 하고, 다른 explorer 도 바뀐 경로가 자기 검색 시작 폴더 안이면 다시 찾는다. 결과가 든 폴더를 보는 explorer 는 그 폴더 목록을 다시 읽는다([작업 뒤 목록 갱신](file-operations.md#작업-뒤-목록-갱신)).
- **검색 상태**: 진행 중은 Spinner·`explorer.find.searching`·Stop, 끝나면 `explorer.find.found`, Stop 으로 멈추면 `explorer.find.stopped`(그때까지의 결과 유지). 결과가 5,000 개(`SEARCH_MAX_HITS`)에 닿으면 worker 가 더 읽지 않고 멈춘다. stopped 는 상한에 닿은 뒤 더 맞는 항목이나 읽지 않은 폴더가 남았을 때 쓴다. 그래서 정확히 5,000 개로 끝나도 읽지 않은 폴더가 남았으면 stopped 다. 바는 `explorer.find.capped`("5,000+ found · stopped"), 상태줄은 `explorer.find.capped_hint`(검색어를 더 입력하라는 안내)이고 찾은 결과는 남는다. 읽지 못한 하위 폴더는 건너뛰고 `explorer.find.skipped`(하나면 `explorer.find.skipped_one` "1 folder skipped")를 accent-warning 으로 덧붙이며 툴팁에 그 폴더들을 적는다. 결과가 없으면 `explorer.find.none` 과 보조 줄 `explorer.find.none_sub`, 시작 폴더를 읽지 못하면 error 톤 `explorer.find.failed` 와 OS 이유, Retry(새로고침 — 새로고침은 하위 폴더 검색을 처음부터 다시 한다) 상태 화면이다.
- **닫기**: 입력의 첫 `Esc` 는 글자를 지우고 둘째 `Esc` 는 바를 닫는다. ×·토글로도 닫는다. 연 폴더를 떠나면(뒤로·앞으로·위로·주소창·폴더 열기) 닫는다. Subfolders 를 끄면 지금 폴더 거르기로 돌아간다. 바를 닫거나 검색어를 바꾸면 이전 검색의 영수증을 버려 worker 가 다음 폴더를 읽기 전에 멈춘다.
- 입력에 포커스가 있는 동안 타입어헤드와 explorer 목록 단축키를 끈다(`text_input_active`).

### 컨텍스트 메뉴 · 파일 조작

바인딩이 있는 행은 설정의 첫 단축키를 표시하며, 빈 영역의 Undo·Redo는 이력이 없어도 꺼진 행으로 남는다. [파일 조작](file-operations.md)의 이력 메뉴 규칙을 따른다.

진입점별 대상 결정, 작업별 결과·피드백, 지원하지 않는 작업은 [파일 작업 계약](file-operations.md)에 있다.

**들어 있는 폴더에서 보기**(id 90, `view/main/explorer_show_in_folder.rs`): 대상이 하나이고 로컬 explorer 이며 `ExplorerView::enclosing_target`(`explorer/enclosing.rs`)이 폴더를 돌려줄 때 메뉴 맨 앞에 행과 구분선을 둔다. 링크(lstat 이 symlink)는 링크 자신만 한 번 푼 대상이 든 폴더다(`link_target`: `read_link` 결과를 링크가 든 폴더에 이어 붙이고 `.`·`..` 만 글자로 정리한다). 상위 경로의 링크는 풀지 않으므로 `~/proj` 가 다른 디스크로 링크돼 있어도 `~/proj/…` 경로로 열린다. 대상이 다시 링크면 그 링크가 든 폴더에서 그 링크를 고른다(체인을 끝까지 따라가지 않는다). 하위 폴더 검색 결과는 결과가 든 폴더다. 대상이 없는 링크와 그 밖의 항목에는 행이 없다. 고르면 툴바 이동과 같은 `ExplorerCommand::Navigate` 를 `EngineAction::Explorer` 로 보내(히스토리에 남는다) 그 폴더를 열고, `reveal_after_load` 가 Find 를 닫은 뒤 그 폴더 목록에 항목이 나타나는 프레임에 항목 하나만 고르고 그 자리로 스크롤한다. 고를 항목은 그 폴더에 묶인다. 고르기 전에 그 폴더에 도착했다가 다른 폴더로 가거나(뒤로·앞으로·위로·내부 탭 전환 포함) 그 폴더 목록이 읽기 오류로 끝나면 버린다(`settle_reveal`). 그래서 나중에 그 폴더를 다시 열어도 예전 항목을 고르지 않는다. 목록을 받았는데 항목이 없으면 그 폴더를 떠날 때까지 기다린다. 깜박임 같은 강조는 없다. 키보드 현재 항목(cursor ring)은 이 기능이 놓지 않는다.

**열기**(id 91, 같은 파일): 들어 있는 폴더에서 보기 행이 붙고 목록이 하위 폴더 검색 결과일 때(`ExplorerView::is_search_result`) 그 행 앞에 `button.open` 행을 둔다. 순서는 열기 → 들어 있는 폴더에서 보기 → 구분선 → 일반 파일 메뉴(Copy path부터)다. 링크만인 경우(검색 중이 아님)에는 붙이지 않는다. 고르면 더블클릭과 같은 액션을 `apply_explorer_action` 에 넘긴다. 폴더(폴더를 가리키는 링크 포함)는 `Navigate` 라 `ExplorerCommand::Navigate` 를 담은 `EngineAction::Explorer` 가 되고, 파일은 `OpenFile` 이라 더블클릭과 같은 파일 열기(`DomainIntent::DispatchFile`)로 간다.

우클릭 컨텍스트 메뉴는 **2-단계 네이티브 메뉴 패턴**([context-menu](../../dev-guide/context-menu.md))을 따른다: 렌더 중 우클릭을 감지하면 `ExplorerAction::ContextMenu { target, cwd, x, y }` 를 모으고, `apply_explorer_action` 이 이를 `PendingNativeMenu::Explorer`/`ExplorerFavorite` 슬롯에 선점한다.
비-terminal 컨텍스트 메뉴는 winit 이 만들지 않고 egui 프레임이 단일 생산자다 — explorer 메뉴는 같은 egui 프레임 안에서 `apply_explorer_action`(렌더 루프 종료 직후)이 generic surface fallback(`emit_surface_menu_fallback`)보다 **먼저** 슬롯을 선점하므로, fallback은 `is_none()` 확인 후 건너뛰어 explorer 전용 메뉴를 유지한다.
이후 `MainView::process_pending_native_menu` 가 `open_native_menu` 로 OS 네이티브 메뉴를 띄우고, 선택 id 를 조작으로 번역하는 처리는 continuation 으로 예약된다(Linux 는 메뉴가 닫힌 뒤 프레임에 실행 — [context-menu](../../dev-guide/context-menu.md) · [내부 팝업 시스템](../../design/systems/popup.md)).
메뉴를 열 때 대상 경로·현재 폴더와 그 surface 의 세대(`SurfaceBinding`)를 고정한다. 고른 항목을 실행할지 정하는 규칙과 rename 팝업의 확인은 [파일 작업 계약](file-operations.md#경로-출처와-작업-대상-공통-계약)에 있다. 즐겨찾기 행 메뉴도 같은 규칙을 쓰며 그중 이 폴더로 루트 설정만 파일을 바꾸지 않는 항목으로 본다.

대상(target)은 우클릭 위치/선택 상태로 결정한다(design §3.3 target rule): 선택 안의 항목 → 선택 전체, 선택 밖 → 그 항목으로 선택 리셋, 빈 영역 → 현재 폴더(current). variant 4종(빈 영역 / 파일 / 폴더 / 다중). 좌측 사이드바 트리 폴더 우클릭도 **단일 폴더 target 을 직접 구성**해(선택집합 미조작) 동일 메뉴를 띄운다.

**surface의 나머지 영역**: 위 위치별 핸들러가 처리하지 못한 우클릭(툴바/주소창/내부 탭바/상태줄/빈 사이드바 등 chrome 영역)은 `draw_explorer` 끝의 **표면 전체 rect catch-all** 이 `Empty`(현재 폴더) target 으로 처리한다. 하위 위젯이 이미 `action`을 만들었으면 건너뛰므로 파일/폴더/다중 선택 메뉴를 유지한다. 이로써 generic surface fallback("터미널 ID 복사")이 explorer 표면 어디에서도 뜨지 않는다. 예외: 권한 거부 루트(`LoadState::NoPermission`)는 붙여넣기가 무의미하므로 catch-all 을 건너뛴다(content 빈영역 규칙과 동일).

- **새 폴더 / 새 파일** (빈 영역·단일 폴더, 원격 쓰기를 받지 않는 mirror 에서는 꺼진 행과 이유 툴팁) — 위 "새 폴더 · 새 파일".
- **경로 복사** (`copy_path`, 다중은 개행 결합) → OS 텍스트 클립보드 쓰기 성공 후 `toast.copied_path` 토스트, 실패 시 접근 불가·실패 안내(단축키/Command Palette/우클릭 메뉴 모두 동일).
- **복사 / 잘라내기 / 붙여넣기** — explorer 파일 클립보드(`MainViewState::explorer_clipboard`, 앱의 모든 창이 함께 쓰는 단일 슬롯·세션 종료 시 폐기)에 경로+cut 플래그를 담고, 붙여넣기에서 소비한다. 로컬 경로는 OS 파일 클립보드에도 올리며 실패하면 같은 Tasty 인스턴스 내부에만 보관됐다고 알리고, 붙여넣기는 Tasty 목록과 OS 쪽 중 나중에 복사한 쪽을 쓴다([OS 파일 클립보드](file-operations.md#os-파일-클립보드)).
  파일 복사·이동·휴지통·이름 변경·시스템 열기는 View가 고정 경로와 원 surface/View identity를 요청으로 넘기고 App의 `explorer_files` worker가 실행한다. 파일 이동 헬퍼는 `src/app/file_jobs/ops.rs`에 있으며 충돌 시 `(copy)` 접미사를 붙이며 목적지 공개는 OS의 덮어쓰기 금지 rename으로 수행한다. 복사는 목적지의 전용 임시 디렉터리에서 준비하고 실패 시 제거한다. 심볼릭 링크는 따라가지 않고 링크로 복사하며 별칭을 해소한 실제 하위 디렉터리로의 복사는 거부한다. cut은 교차 파일시스템 오류일 때만 copy+remove로 전환한다.
  View별 대기 요청은 8개, 요청 경로·이름 자료는 1MiB 이내이며 App은 로컬 작업을 한 번에 하나, 원격 작업을 연결마다 하나씩 실행한다. 이 상한에 걸린 사용자 요청은 나누어 보내지 않고 받지 않으며, 그 칸에 경고 카드로 이유를 보인다(대기열이 차면 `explorer.request.queue_full`, 자료가 1MiB 를 넘으면 `explorer.request.too_large`, [파일 작업](file-operations.md)). 시작 전 원 대상과 mirror 제한을 다시 검사한다. 완료 뒤 원 View/surface가 살아 있을 때만 목록 갱신을 요청하고, 변경된 선택이나 새 클립보드는 지우지 않는다. cut은 전부 성공한 원 클립보드만 소진하며 부분 성공은 기존 목록을 유지한다. 소진은 앱 공용 슬롯에서 하므로 원 View/surface 가 닫혔어도 일어난다.
  종료는 신규 실행을 막고 실행 중인 작업에 취소를 건 뒤 최대 5초 실제 worker join을 관측한다. 기한이 지나도 작업 취소나 완료로 기록하지 않으며 남은 worker를 경고한다. join 없이 떼어 둔 worker 도 그 취소를 보고 멈추며 준비 중 사본을 임시 폴더째 지운다. 그 정리보다 프로세스가 먼저 끝나면 목적지에 숨은 임시 폴더가 남을 수 있다([파일 작업](file-operations.md#상태별-결과)). 로컬 목록·트리 읽기는 이 worker 와 분리된 읽기 worker 가 맡아 대형 복사가 목록 조회를 막지 않는다.
  잘라내기는 이동 성공 시 클립보드를 비운다.
  우클릭 메뉴뿐 아니라 키보드 단축키(기본 `copy`/`cut`/`paste` 바인딩, explorer 포커스 시)로도 동일하게 동작한다 — `handle_explorer_shortcut`(`src/adapters/ui/input/shortcuts/copy_paste.rs`)가 선택 항목을 모아 컨텍스트 메뉴와 같은 `explorer_menu_set_clipboard`/`explorer_menu_paste` 를 호출하므로 fs 동작이 두 경로에서 갈라지지 않는다.
  단축키 붙여넣기 대상은 현재 폴더(current)다(선택된 폴더 안으로의 paste-into 는 컨텍스트 메뉴 전용).
  **복사(cut=false)** 는 fs 접근이 없어 어느 mirror explorer 에서도 그대로 동작하고, **잘라내기(cut=true)/붙여넣기**는 원격 쓰기를 받지 않는 server 의 mirror 에서 메뉴 행이 꺼지고 단축키는 이유 토스트로 거절된다(위 "mirror(attach) explorer 의 파일 변경" 참고).
  클립보드는 경로와 함께 그 경로의 출처(`ExplorerPathSource`: 로컬 또는 mirror workspace)를 기록한다(`src/state/explorer_path.rs`). 붙여넣기는 같은 출처의 칸에서만 한다. mirror explorer 에서 복사한 경로를 로컬 explorer 나 다른 연결에 붙여넣으면 메뉴에 나오지 않고 단축키는 `explorer.state.remote_paste_unsupported` toast 로, 로컬에서 복사한 경로를 mirror explorer 에 붙여넣으면 `explorer.state.cross_computer_paste_unsupported` toast 로 거부한다. 같은 문자열의 다른 컴퓨터 파일을 대신 복사하지 않는다.
- **휴지통으로 이동** (`delete`) — `trash` 크레이트로 OS 휴지통에 보낸다(가역적이라 확인 모달 없음). 원격은 server 의 휴지통으로 보낸다. 원격 쓰기를 받지 않는 mirror 에서는 꺼진 행이고 단축키는 이유 토스트로 거절한다.
- **이름 변경** (`rename`, 단일만) — 공용 rename 팝업(`PopupDef`)을 재사용한다. 이름은 드라이브 접두어·경로 구분자 없는 단일 파일명이어야 하며 기존 항목을 덮어쓰지 않는다. 원격 쓰기를 받지 않는 mirror 에서는 꺼진 행이고, 다른 경로로 불리면 가드가 먼저 막아 팝업 자체가 열리지 않는다. mirror 항목이면 확정해도 팝업을 닫지 않고 server 의 답을 기다린다(`DialogState::rename_waiting`). 입력은 보낸 이름을 읽기 전용 모양(input-readonly-bg 바탕·input-readonly-border 테두리·input-readonly-fg 글자)으로 보이고 고칠 수 없으며, 그 아래 icon-size-sm Spinner 와 `explorer.remote.renaming`("Renaming on {host}…", caption·text-muted, space-sm 간격) 줄을 두고, 두 버튼과 Enter·Esc 를 막는다. 성공하면 팝업이 닫히고(`ui_sync::close_renamed`), 성공 없이 요청이 사라지면 기다림을 풀어 같은 글자로 다시 편집하게 한다(`ui_sync::resume_unanswered_rename`). 요청을 받지 않았으면 로컬처럼 팝업을 닫고 이유는 그 칸이 알린다. 팝업의 입력은 공용 Input 이 아니라 egui TextEdit 이라, 기다리는 동안 egui 프레임을 끄고 같은 크기의 칸에 그 바탕과 테두리를 직접 그린다. 팝업 바탕(surface-raised)이 그 바탕·테두리 토큰과 같은 값이라 화면에서는 상자가 보이지 않고 칸이 팝업 바탕에 묻힌다([정합 차이](../../design/systems/design-parity-notes.md)).
- **OS 기본 앱으로 열기** (`open_in_system`, 파일·폴더·여러 항목) — 고른 항목마다 `platform::reveal::open_path`(Windows `explorer` / macOS `open` / Linux `xdg-open`)를 부른다(`Operation::Open`). 하나를 못 열어도 나머지는 열고, 실패한 경로와 이유를 모아 결과로 알린다. 파일은 OS 기본 앱으로, 폴더는 OS 파일 관리자로 열린다. 로컬 칸에서만 휴지통 뒤 구분선 다음에 자기 묶음으로 나오고, 그 묶음 뒤 구분선과 Properties 가 메뉴를 끝낸다. mirror 에서는 두 행과 묶음 구분선을 함께 숨기며 핸들러도 막는다. 행 이름은 sentence case "Open in system" 이다.
- **OS 연계 행의 아이콘**: 시안대로 "시스템에서 열기"에 external, "파일 관리자에서 보기"에 folderOpen 아이콘을 단다(`explorer_os_items.rs`). 컨텍스트 메뉴는 OS 네이티브 메뉴라 `MenuItem::with_icon` 으로 단색 SVG 와 zoom 을 적용하지 않은 `SIZING.icon_glyph_size_md` 를 넘기고, 잉크 색·선택·disabled 표현은 각 OS 메뉴가 정한다([컨텍스트 메뉴 가이드](../../dev-guide/context-menu.md)). 다른 행은 아이콘 없이 둔다.
- **파일 관리자에서 보기** (`reveal_*`, 한 항목) — 그 항목을 담은 폴더를 OS 파일 관리자로 열고 항목을 고른다(`Operation::Reveal` → `platform::reveal::reveal_path`). 행 이름은 OS 마다 다르다: macOS "Show in Finder"(`open -R`), Windows "Show in File Explorer"(`explorer /select,"<path>"`), 그 밖 "Show in File Manager". Linux 는 freedesktop `org.freedesktop.FileManager1.ShowItems` 를 `dbus-send`(응답 2 초 제한)로 부르고, 그 서비스나 `dbus-send` 가 없으면 고르기 없이 부모 폴더를 `xdg-open` 으로 연다. URI 는 경로 바이트를 그대로 퍼센트 인코딩한다(NFD 이름도 합성하지 않음). "OS 기본 앱으로 열기" 바로 뒤에 나오며 mirror 에서는 숨긴다.
- **즐겨찾기 추가** (`add_to_favorites`, 단일 폴더 또는 빈 영역) — 아래 참조. mirror 에서 차단.
- **새 탭으로 열기** (`open_in_new_tab`, 단일 폴더) — 그 폴더를 cwd 로 하는 새 explorer 를 **Pane 탭**(explorer 내부 탭이 아님)으로 연다. 우클릭 대상 surface 의 **소유 pane** 에 추가해(`RequestContext::add_kind_tab_by_owner`) focused pane 이 아니어도 올바른 pane 에 열린다. 기존 explorer 는 불변. mirror 에서 메뉴 자체가 숨겨지고 핸들러도 막는다(위 "mirror(attach) explorer 의 파일 변경").
- **이 폴더로 루트 설정** (`set_as_root`, 단일 폴더) — **현재 explorer** 의 cwd 를 그 폴더로 이동한다(`RequestContext::set_explorer_cwd` 가 `EngineAction::ExplorerCwd` 를 보내고 App 이 `ExplorerTab::set_cwd` 를 적용: 좌측 트리 루트·current 이동 + 히스토리 초기화 + 뷰 리로드). 파일시스템을 바꾸지 않으므로 mirror 에서도 그대로 동작.

### 경로 출처와 작업 대상

파일 작업이 공유하는 규칙(경로의 출처, 요청 시점의 대상 고정, 진입점별 원격 쓰기 거부)은 [파일 작업 계약](file-operations.md#경로-출처와-작업-대상-공통-계약) 한 곳에 있다.

### 심볼릭 링크

목록은 `read_dir_entries`(`src/core/fs_list.rs`)가 읽는다. 링크 항목은 대상의 metadata 로 종류·크기·수정 시각을 정하고 `DirEntryInfo::link` 에 상태를 남긴다(`NotALink` / `Valid` / `Broken`). OS 숨김 표시는 `DirEntryInfo::os_hidden` 이다(위 "숨김 파일").

- **폴더를 가리키는 링크**는 폴더로 보이고 트리에도 나온다. 들어가면 경로는 링크 자신의 경로다. 주소창·히스토리·뒤로/위로는 대상의 실제 경로가 아니라 링크 경로를 쓴다. 따라서 링크 폴더에서 위로 가면 링크가 있던 폴더로 돌아간다.
- **대상이 없는 링크**는 폴더로 보지 않으며 크기·수정 시각은 링크 자신의 값이다. 열면 파일을 찾지 못한 것처럼 보이지 않도록 `explorer.state.broken_link` 오류 toast 로 대상이 없다는 원인을 알린다. 하위 폴더 검색 결과의 링크도 같다(`ExplorerView::listed_entry` 가 지금 폴더 목록과 검색 결과를 함께 본다). 주소창에 넣으면 거부하지 않고 파일처럼 그 링크가 든 폴더로 가서 링크를 고른다([주소 입력](#주소-입력), `address.rs` 의 `a_link_is_not_resolved_and_a_broken_link_is_picked_in_its_folder`).
- **목록 표시**(`explorer/link.rs`, Detail · List · Grid 공통): 대상이 있는 링크는 대상의 글리프(폴더·파일·그림)를 쓰고 이름 뒤 `space-xs` 자리에 link 글리프 12를 붙인다. 세 보기 모두 꼬리 폭(`space-xs` + 글리프)을 먼저 빼고 이름을 그 앞에서 자른다. 그래서 이름이 길어도 글리프는 보인다. Detail 은 이름을 말줄임하고, List 는 이름 칸을 그 폭만큼 줄여 클립한다(공용 `tree_row_with_tail`). Grid 는 이름의 마지막 줄 끝에 붙이고, 그 폭만큼 이름 줄 폭을 줄인다. 대상이 없는 링크는 항목 자리에 link 글리프를 accent-warning 으로 그리고 이름은 보통 색(취소선 없음)이다. 로컬 항목은 호버하면 `explorer.link.target_missing`("Target not found: {path}")으로 `read_link` 가 돌려준 대상 경로를 보인다(호버할 때만 읽는다). 원격 항목은 이 툴팁이 없다. 경로가 원격 호스트의 것이라 로컬에서 읽지 않고, 원격 응답은 대상 경로를 싣지 않는다. 색·크기는 `explorer-link-glyph`(→ text-muted)·`explorer-link-glyph-size`(→ icon-size-xs 12)·`explorer-link-broken-fg`(→ accent-warning) 토큰이다. Type 열 낱말은 위 종류 문구다.
- **링크 자체에 대한 조작**: 복사·붙여넣기는 링크를 링크로 복사한다. 이름 변경과 cut 의 교차 파일시스템 정리(`remove_path`)는 링크만 옮기거나 지운다. 휴지통(`trash` 크레이트)은 모든 OS 에서 부모 경로만 canonicalize 하고, Linux(freedesktop) 구현은 링크 항목 자체를 휴지통으로 옮긴다. Linux 에서는 폴더·파일·대상이 없는 링크를 휴지통으로 보낸 뒤 링크만 휴지통에 링크로 들어가고 대상이 그대로인지를 시험한다(`job_tests::trashing_a_link_keeps_what_it_points_to`, 자식 프로세스의 `XDG_DATA_HOME` 을 임시 폴더로 바꿔 사용자 휴지통을 쓰지 않는다). 이름 변경 뒤 대상 보존은 `ops::tests::renaming_or_removing_a_folder_link_leaves_the_target_folder` 가 본다. macOS·Windows 의 링크 휴지통 동작은 실 기기에서 확인하지 않았다. 어느 조작도 대상 폴더나 그 내용을 바꾸지 않는다.
- **원격 목록**: 원격 server 도 같은 `read_dir_entries` 로 목록을 만들고, `list_dir_result` 의 링크 항목에만 `link` 선택 필드(`"valid"` 대상 있음 · `"broken"` 대상 없음)를 싣는다(server 의 `content_queries/list_dir_wire.rs`). `gui` 기능 없이 빌드한 headless server 도 링크를 같은 방식으로 분류해 싣는다. `os_hidden` 은 GUI server 만 싣는다. client 는 `attach_client/wire/list_dir.rs` → `output/remote_entry.rs` 에서 `DirEntryInfo::link` 로 옮긴다. 그래서 Detail · List · Grid · 사이드바 트리 · 열기 · 주소 입력이 로컬과 같은 규칙을 탄다. 폴더 링크는 폴더로 트리에 나오고 대상이 있는 파일 링크는 이름 뒤 link 글리프가 붙는다. 대상이 없는 링크는 accent-warning link 글리프와 Type "Broken link" 로 보이고, 열면 `explorer.state.broken_link` 원인 toast 가 뜬다. 다른 점은 위 호버 툴팁이 없는 것 하나다.
  - 이 필드를 싣지 않는 server(필드 도입 전 버전)에 붙으면 client 는 오류 없이 모두 링크 아님으로 읽는다. 폴더 링크는 server 가 정한 종류대로 폴더로 보이고, 대상이 없는 링크는 일반 파일처럼 보이며, 열면 링크 원인 안내 없이 일반 파일과 같은 원격 열기(`DispatchFile`)로 가서 결과는 원격 핸들러가 정한다. 모르는 값도 링크 아님으로 읽는다. 이 필드를 모르는 옛 client 는 무시한다.
  - 시험: `list_dir_wire.rs` 의 `only_a_link_carries_its_state`·`listed_links_go_out_with_their_kind_and_state`(실제 링크 세 종류)·`an_old_client_reads_a_marked_entry_without_error`(세 시험은 기본 조합과 `--no-default-features` headless 조합에서 모두 돈다), `wire/list_dir.rs` 의 `a_new_server_link_state_is_read`·`an_old_server_or_an_unknown_link_value_reads_as_not_a_link`, `remote_entry_tests.rs` 의 `a_remote_link_state_reaches_the_explorer_listing`·`an_old_server_entry_is_never_a_broken_link`, `address/probe.rs` 의 `a_remote_broken_link_is_picked_in_its_folder_and_a_folder_link_is_entered`, `link.rs` 의 `a_remote_broken_link_does_not_read_the_local_filesystem`.

### 즐겨찾기 (favorites)

전역(surface 무관)·영속 즐겨찾기 — **로컬 client 파일시스템 경로 전용**. `~/.tasty/explorer-favorites.toml`(`[[favorite]]` 배열, label+path)에 저장된다. 프로세스에 원본 하나(`RuntimeRegistries::explorer_favorites`)를 두고 윈도우마다 있는 engine 은 그리기용 사본만 가진다. 추가·제거는 `EngineAction` 이 `EngineRuntime::explorer_favorites.change` 로 원본을 바꾸고 원본 전체의 불변 snapshot을 file writer에 제출한다. 그래서 한 윈도우의 저장이 다른 윈도우에서 추가한 항목을 지우지 않고, 그리기 밖에서 적용되는 제거(Favorites 행 네이티브 메뉴)도 다른 윈도우에 바로 보인다. 사본 갱신·다른 윈도우 다시 그리기·파일 재읽기 시점은 [App·Engine·View 상태 소유권](../../dev-guide/app-state-ownership.md#윈도우가-함께-쓰는-영속-목록)을 따른다. 메모리 mutator(`add`/`remove`)는 순수하고 디스크 반영은 `SharedList`의 receipt로 확정한다. 저장 중에도 목록 잠금은 비어 있고 다른 윈도우 변경이 진행된다. 저장 실패는 경고 알림에 남기며 메모리 목록은 유지한다. mirror(attach 원격 점유) explorer 의 경로는 원격 호스트의 경로라 이 전역 목록에 섞일 수 없다 — "즐겨찾기 추가"는 mirror explorer 에서 차단된다(위 "mirror(attach) explorer 의 파일 변경" 참고).

- **추가**: 컨텍스트 메뉴 "Add to favorites" → rename 팝업과 동일 골격의 입력 팝업(`RenameTarget::ExplorerAddFavorite`, 확정 버튼 라벨만 "Add")으로 라벨을 받아 등록(같은 경로 재등록 시 라벨만 갱신).
- **표시/이동**: 사이드바 하단 고정(pin) "Favorites" 영역(사이드바 본문이 240px 이상이면 캡션 **항상 표시**)에 **채운 별(STAR_FILL) + accent-warning(골드)** 행으로 나열, 클릭 시 해당 경로로 이동. 현재 폴더인 즐겨찾기는 surface-active 하이라이트. 이 영역은 위 "사이드바 트리"에 서술한 고정 높이로 보이며 자체 스크롤된다. 본문이 240px 미만인 낮은 칸에서는 영역 전체를 숨긴다.
- **여러 윈도우**: 한 윈도우에서 추가·제거한 결과는 다른 윈도우의 Favorites 에도 바로 보이고, 재시작 뒤에도 모두 남는다.
- **빈 상태(empty state)**: 즐겨찾기가 0개여도 섹션이 사라지지 않는다(발견성) — 흐린 별(opacity 0.55) + `explorer.sidebar.favorites_empty`("No favorites yet") + 우클릭 힌트(`favorites_empty_hint`, "Add to favorites" 스팬만 text-muted, 나머지 text-placeholder)를 표시한다(design `FavoritesEmpty`).
- **제거/열기/루트 설정**: 즐겨찾기 행 우클릭 → `PendingNativeMenu::ExplorerFavorite`(우클릭 explorer 의 `surface_id` 동봉) → "새 탭으로 열기" / "이 폴더로 루트 설정" / "즐겨찾기에서 제거". 제거는 전역이라 경로만으로 하지만, "루트 설정" 은 `surface_id` 로 대상 explorer 를 지정한다.
- 즐겨찾기 목록은 `EngineRead` 가 빌려 주는 읽기 전용 slice 로 `draw_explorer` 에 전달된다. 화면은 목록을 바꾸지 않고 추가·제거를 요청만 한다.

### 사용자 트리거 (단축키 — [KeybindingSettings](../../features/keybindings/index.md))

모든 단축키는 `KeybindingSettings` 로 노출되며 하드코딩하지 않는다. `convert_to_explorer` 외에는 explorer 포커스에서만 동작:

| 액션 | 필드 | Tasty 프리셋 기본 |
|------|------|------|
| 새로고침 | `explorer_refresh` | `F5` |
| 상위 폴더로 | `explorer_go_up` | `Alt+Up` |
| 뒤로 | `explorer_back` | `Alt+Left` (macOS 에서 ⌘←) |
| 앞으로 | `explorer_forward` | `Alt+Right` (macOS 에서 ⌘→) |
| 주소창으로 이동 (경로 전체 선택) | `explorer_focus_address` | `Alt+L` (macOS 에서 ⌘L) |
| 미리보기 패널 켜기·끄기 | `explorer_toggle_preview` | (기본 미할당) |
| 숨김 파일 보기·숨기기 | `explorer_toggle_hidden` | `Ctrl+Shift+.` / `Alt+Shift+.` (macOS 프리셋 `⌘⇧.`) |
| Properties 열기 | `explorer_properties` | (기본 미할당) |
| 전체 선택 | `select_all` | `Ctrl+A` / `Alt+A` |
| 경로 복사 | `copy_path` | `Alt+Shift+C` |
| 새 폴더 | `explorer_new_folder` | `F7` |
| 새 파일 | `explorer_new_file` | `Shift+F4` |
| 현재 항목 이동 (위·아래·왼쪽·오른쪽·처음·끝·한 화면 위·아래) | `explorer_cursor_*` | 방향키 · `Home` · `End` · `PageUp` · `PageDown` |
| 선택 넓히기 (같은 이동) | `explorer_extend_*` | 위 키에 `Shift+` |
| 이름 변경 | `explorer_rename` | `F2` |
| 휴지통으로 이동 | `explorer_trash` | `Delete` / `Alt+Backspace`(macOS 에서 ⌘⌫). macOS 프리셋은 `Alt+Backspace` 만, Windows·Linux 프리셋은 `Delete` 만 |
| 고른 항목 열기 | `explorer_open` | `Enter` |
| 선택 해제 | `explorer_clear_selection` | `Esc` |
| 실행 취소 | `explorer_undo` | `Ctrl+Z` / `Alt+Z` |
| 다시 실행 | `explorer_redo` | `Ctrl+Shift+Z` / `Ctrl+Y` / `Alt+Shift+Z` |
| 찾기 | `find` | `Ctrl+F` / `Alt+F` |
| explorer 로 변환 | `convert_to_explorer` | (기본 미할당) |

공통 새로고침(`refresh`, 기본 `F5`)도 탐색기에 포커스가 있으면 목록을 다시 읽는다. 공통 키를 먼저 검사하므로 두 필드가 같은 키여도 한 번만 다시 읽는다([공통 새로고침](../../features/keybindings/index.md#공통-새로고침--refresh)).

뒤로·앞으로·주소창으로 이동·새 폴더·새 파일은 네 프리셋이 같은 `alt+left`·`alt+right`·`alt+l`·`f7`·`shift+f4` 다. 탐색기 키는 전역 단축키와 quick-switch 보다 먼저 키를 보므로, 기본값은 어느 프리셋에서도 다른 바인딩이나 quick-switch 조합과 겹치지 않는 것으로 골랐다(`Cmd+[`·`Cmd+]` 는 `focus_surface_prev`·`focus_surface_next`, `Ctrl+L` 은 탭 quick-switch 의 다음 탭, `Ctrl+Shift+N`·`Cmd+Shift+N` 은 `new_window` 다). `F7`·`Shift+F4` 는 파일 관리자(Total Commander·Far Manager)의 새 폴더·새 파일 키다. macOS 의 기본 설정에서는 F 키가 미디어 키라 `Fn` 을 함께 눌러야 한다. 주소창으로 이동은 다음에 주소창을 그릴 때 입력칸에 포커스를 주고 경로 전체를 고른다(`PathField::focus`). 그다음은 주소창을 눌렀을 때와 같다. 뒤로·앞으로는 툴바의 뒤로·앞으로 버튼과 같은 `GoBack`·`GoForward` 다.

실행 취소·다시 실행의 다른 프리셋 기본은 Mac `Alt+Z`(⌘Z)·`Alt+Shift+Z`(⌘⇧Z), Windows·Linux `Ctrl+Z`·`Ctrl+Shift+Z`/`Ctrl+Y` 다. 이 둘은 explorer 에 포커스가 있을 때만 키를 받으므로 터미널의 `Ctrl+Z`·`Ctrl+Y` 와 부딪치지 않는다.

직접 키 매칭은 `explorer_refresh`·`explorer_go_up`·`explorer_back`·`explorer_forward`·`explorer_focus_address`·`explorer_new_folder`·`explorer_new_file`·`explorer_undo`·`explorer_redo`·`explorer_toggle_preview`·`explorer_toggle_hidden`·`explorer_properties`·`find`·`convert_to_explorer`(포커스 surface 무관) 가 `keybinding.rs`, `select_all`·`copy_path`·`explorer_rename`·`explorer_trash` 가 `copy_paste.rs`, 이동·넓히기·열기·선택 해제가 `explorer_keys.rs` 다. 이동·넓히기·열기·선택 해제·이름 변경·휴지통은 전역 목록이 아닌 탐색기 목록(`EXPLORER_LIST_BINDING_FIELDS`, [단축키](../../features/keybindings/index.md#탐색기-목록-바인딩--explorer_list_binding_fields))에 있다. action-id/Command Palette `dispatch.rs` 는 열여덟 개 모두를, 더블탭 `double_tap.rs` 는 `convert_to_explorer` 만 받는다. `find` 는 터미널 검색과 같은 바인딩이며, 포커스가 explorer 면 터미널 검색 대신 Find 바를 연다([터미널 검색](../../features/terminal-search/index.md)). 설정 UI 서브탭은 `explorer_refresh`·`explorer_go_up`·`explorer_back`·`explorer_forward`·`explorer_new_folder`·`explorer_new_file`·`explorer_undo`·`explorer_redo`·`explorer_rename`·`explorer_trash`·`explorer_open`·`explorer_clear_selection`·`explorer_toggle_preview`·`explorer_toggle_hidden`·`explorer_properties` = **Explorer**, `select_all`·`copy_path` = **Clipboard**, `convert_to_explorer`·`explorer_focus_address` = **Surface** 다. `explorer_focus_address` 는 html surface 의 주소 띠에도 쓰이므로 Surface 탭에 둔다([html 주소 표시줄](../html/index.md#주소-표시줄)).
주소창·새 항목 이름 입력·Find 입력이 키를 받는 동안(`ExplorerView::text_input_active`)에는 explorer 목록 단축키(`keybinding.rs` 의 explorer 묶음, `copy_paste.rs` 의 전체 선택·경로 복사·복사·잘라내기·붙여넣기·이름 변경·휴지통)가 키를 소비하지 않고 글자 편집에 양보한다.

**새 탭으로 탐색기 열기(`open_explorer`, 기본 미할당)는 포커스와 무관하다** — 위 표와 달리 explorer 포커스를 요구하지 않는다. `Intent::NewTab { kind: "explorer" }` 를 발생시키므로 CLI 의 `new tab --type explorer` 와 같은 도메인 인텐트(`CreateTab`)를 쓰되 선택은 다르다 — 단축키는 새 탭을 선택하고, 에이전트(CLI/IPC)는 선택하지 않는다([포커스 정책 (운영 상세)](../../design/policies/focus.md)). 이 액션은 경로를 안 실으므로 홈에서 열린다(명시 경로는 CLI 의 `--path` 가 받는다). 설정 UI 는 **Tab** 서브탭이다 — `open_markdown` 옆, 둘 다 새 탭 열기라서. 이 액션은 `keybinding.rs`·`double_tap.rs`·`dispatch.rs` 세 진입점 전부에서 처리한다.

**드래그 반전 modifier(`explorer_drag_flip_modifier`)** 는 키 조합이 아니라 드래그 중 누르고 있는 modifier 다. 값은 `tab_switch_modifier` 와 같은 modifier 조합 문자열이고, 기본은 macOS `option`, 다른 OS `ctrl` 이다. 누르고 있으면 드래그 놓기의 기본 동작(같은 디스크 이동, 다른 디스크 복사)을 뒤집는다([드래그 앤 드롭](file-operations.md#드래그-앤-드롭)). 설정 › 단축키 › General 서브탭 끝(엔트리 아래 구분선 뒤)의 한 행에서 고르거나 config.toml 의 `[keybindings]` 에서 바꾼다. 단축키 가져오기는 이 값도 그대로 옮기며, macOS 에서 내보낸 `option` 은 다른 OS 에서 Win·Super 를 누른 채 끄는 동작이 된다([key-mapping](../../design/policies/key-mapping.md#이식-시-option-처리)).

### 타입어헤드로 항목 선택

목록에 포커스가 있을 때 영숫자를 입력하면 그 글자로 시작하는 항목이 선택되고 그 항목이 보이도록 스크롤한다. grid·list·detail 세 뷰 모두 같다. 같은 글자를 이어서 누르면 후보를 순환하고 마지막 다음에는 처음으로 돌아간다. 다른 글자를 이어서 입력하면 그 글자를 합친 접두사로 찾는다. 마지막 입력 후 1초가 지나면 버퍼를 비우고 다음 글자를 새 검색으로 읽는다. 일치하는 항목이 없으면 선택과 버퍼가 그대로 남아, 오타 한 글자가 그 뒤의 입력을 막지 않는다. 비교할 때 대소문자는 구분하지 않으며, `..`는 화면에만 있는 행이라 대상이 아니다.

이 동작에는 `KeybindingSettings` 항목이 없다. 키 조합에 액션을 연결하는 것이 아니라 문자 입력을 그대로 소비하는 방식이라 위 표와는 범주가 다르다. 다만 사용자가 수식 키 없이(또는 shift만 붙여) 영숫자를 단축키로 등록하면 그 글자는 타입어헤드가 소비하지 않고 단축키에 넘긴다. 한 번의 키 입력으로 둘이 함께 동작하기 때문이다. 텍스트 이벤트가 단축키 처리보다 먼저 egui 큐에 들어가고 그것을 되돌릴 방법이 없다([key-mapping](../../design/policies/key-mapping.md)의 "텍스트 입력과 단축키의 우선순위"). Tasty 기본 프리셋에는 그런 단축키가 없다.

다음 네 조건 중 하나라도 해당하면 그 프레임의 글자를 무시하고 버퍼도 비운다. 버퍼를 남겨두면 다시 돌아왔을 때 예전 접두사가 이어져, 사용자가 입력하지 않은 글자로 검색하게 된다.

- 그 explorer surface가 포커스를 가지고 있지 않다. egui 이벤트 큐는 창 전체가 공유하므로, 이 조건이 없으면 한 번의 입력이 열려 있는 모든 explorer의 선택을 동시에 움직인다.
- 모달·팝업·입력 다이얼로그가 떠 있다(`keyboard_overlay_open`).
- 주소창을 편집 중이다(`addr_editing`). 그 글자는 `PathField`가 받는다.
- 목록이 `LoadState::Ok`가 아니거나 비어 있다.

폴더나 정렬이 바뀌어 목록을 다시 읽어 오거나(`sync`), cwd나 내부 탭을 바꾸는 액션이 적용되면(주소창 편집을 취소하는 것과 같은 자리) 버퍼를 비운다. 인덱스가 가리키는 항목이 달라지기 때문이다.

### 현재 항목 · 영역 선택

**현재 항목**(`view/cursor.rs`)은 선택과 따로 있는 키보드 위치다. 이동 키를 누르면 현재 항목을 옮기고 그 항목만 고른다. 넓히기 키(기본 Shift+이동)는 기준 항목(`anchor`)부터 새 현재 항목까지 고른다. 시작 위치는 키로 움직이는 중이면 현재 항목, 아니면 클릭·타입어헤드가 정한 기준 항목이고, 둘 다 목록에 없으면 첫 항목(End 는 마지막)이다.

- Detail·List 의 위·아래는 한 항목, 왼쪽·오른쪽은 움직이지 않는다. Grid 의 위·아래는 한 줄(열 수)씩, 왼쪽·오른쪽은 한 칸씩이며 줄을 넘어 이어진다. 아래 줄에 같은 열 칸이 없으면 마지막 항목으로 간다.
- 한 화면 위·아래는 목록 높이에 온전히 들어가는 줄 수 × 열 수만큼 간다. 처음·끝은 목록 양 끝이다.
- 옮긴 항목이 지난 프레임에 온전히 보이지 않았으면 그 항목을 화면 가운데로 스크롤한다(타입어헤드와 같은 `scroll_to` 경로). 보이던 항목이면 스크롤하지 않는다.
- 키로 움직인 뒤에만 그 칸 안쪽에 `explorer_cursor_ring`(border-focus) 색, `explorer_cursor_ring_width`(1px) 테두리를 그린다(선택 채움 위, 모서리 radius-sm). 목록을 누르면 다음 키까지 숨는다. 미리보기 패널을 열면 행의 전체 폭과 그리기 clip을 실제 목록 viewport로 제한하므로 오른쪽 테두리도 목록 경계에서 끝난다. 영역 선택과 목록의 드롭 대상도 이 경계를 따른다.
- 주소창·이름 입력·Find 입력이 키를 받는 동안과 영역 선택 중에는 움직이지 않는다.
- 포커스를 가진 popup·modal·입력 대화상자·plugin popup 이 있으면(`keyboard_overlay_open`) winit 키가 단축키 경로(`handle_shortcut`)에 가지 않으므로 이동·열기·선택 해제·이름 변경·휴지통 키도 받지 않는다. 떠 있어도 포커스가 없는 host popup 은 막지 않는다. Linux X11에서는 실제 물리 누름이 탐색기 목록의 IME 조합 중에 시작됐는지 먼저 기록한다. XIM이 그 누름으로 조합을 확정한 뒤 같은 키를 다시 전달해도 이동·열기·선택 해제·이름 변경·휴지통 동작으로 이어지지 않는다. 다음 실제 누름은 이전 기록을 교체하므로 조합이 끝난 뒤의 정상 키는 그대로 동작한다. 시간 유예나 다음 키 일괄 차단은 쓰지 않는다. Find 입력칸에서는 조합을 끝내는 Escape만 같은 소유권으로 소비하여 검색어가 함께 지워지지 않게 한다. 다음 실제 Escape는 원래 검색어 지우기·닫기 동작을 한다. Find의 다른 키, 주소·이름 입력과 포커스된 popup은 이 목록 차단 대상이 아니다. 탐색기 항목의 이름 변경 popup에서는 그 popup에서 시작한 비어 있지 않은 조합의 실제 Enter 누름만 별도 소유권으로 기록한다. XIM clear/Commit 뒤 같은 누름이 재전달되어도 이름을 확정하지 않으며, 다음 별도 Enter는 확정한다. popup 재열기·다른 popup 포커스·surface binding 변경으로 소유권을 넘기지 않고, IME Disabled·윈도우 포커스 변경은 이름 변경 조합 기록을 지운다. 다른 이름 변경 대상과 Enter 외의 편집 키는 이 처리에 포함하지 않는다. Wayland·Windows·macOS의 입력 전달에는 이 X11 처리를 적용하지 않는다.

**열기·선택 해제**(`view/keys.rs`, `explorer_keys.rs`)도 같은 탐색기 키 경로에서 받는다.

- 열기(`explorer_open`, 기본 `Enter`)는 지금 목록에 보이는 고른 항목만 센다. Find 로 걸러져 보이지 않는 항목은 빼고, 남은 항목이 없으면 아무것도 하지 않고 키도 소비하지 않는다. 항목마다 더블클릭과 같은 일을 하며, 몇 개를 여는지는 고른 폴더 수로 정한다.

  | 고른 항목 | 동작 |
  |---|---|
  | 파일만 (하나 이상) | 목록 순서대로 파일을 하나씩 연다 |
  | 폴더 하나 (파일이 섞여도) | 그 폴더로 들어간다. 함께 고른 파일은 열지 않는다 |
  | 폴더 둘 이상 | 아무것도 하지 않고 키도 소비하지 않는다 |
- 선택 해제(`explorer_clear_selection`, 기본 `Esc`)는 선택과 기준 항목을 모두 비운다. 현재 항목은 그대로 두므로, 키로 움직이던 중이면 다음 이동 키가 그 자리에서 시작하고 아니면 첫 항목(End 는 마지막)에서 시작한다. 고른 항목이 없거나 항목을 끄는 중(Esc 가 드래그를 취소한다)이면 키를 소비하지 않는다.
- 영역 선택 중에는 두 키를 소비만 하고 아무것도 하지 않는다.

**기준 항목 규칙**: 넓히기 키와 Shift+클릭이 범위를 재는 기준 항목(`anchor`)은 아래처럼 바뀐다.

| 동작 | 기준 항목 |
|---|---|
| 클릭, Ctrl·Cmd+클릭, 이동 키 | 그 항목 |
| Shift+클릭, 넓히기 키 | 그대로. 넓히기 키는 기준 항목이 없을 때만 시작 위치를 기준 항목으로 삼는다 |
| 전체 선택(`select_all`) | 보이는 마지막 항목. 넓히기 키는 키로 움직이던 중이면 현재 항목에서, 아니면 마지막 항목에서 시작한다 |
| 영역 선택 | 사각형에 걸친 첫 항목 |
| 선택 해제(`explorer_clear_selection`) | 없음 |
| 숨김 파일 끄기(`explorer_toggle_hidden`) | 숨겨지는 항목이면 없음, 아니면 그대로. 숨겨지는 항목은 선택에서도 빠진다 |
| 숨김 파일 켜기 | 그대로. 선택도 그대로다 |
| 다시 읽은 목록에 기준 항목이 없음 | 없음 |

기준 항목이 없으면 Shift+클릭은 Shift 를 뺀 클릭과 같고, 넓히기 키는 시작 위치를 기준 항목으로 삼는다.

**영역 선택**(`view/marquee.rs`)은 목록의 빈 곳(항목 칸도, 누를 수 있는 다른 위젯도 아닌 곳)을 누르고 끌 때 시작한다. 클릭만 하면 선택을 바꾸지 않는다.

- 사각형에 걸친 항목을 고른다. Ctrl·Cmd·Shift 를 누르고 시작하면 그때의 선택에 더하고, 아니면 새로 고른다. 기준 항목은 걸친 항목의 첫 항목이다.
- 시작점은 목록 좌표로 둔다. 스크롤해도 처음 누른 자리에 남고, 화면 밖 항목도 칸 배치(`view/list_layout.rs` 의 `ListLayout`: 첫 칸 자리·칸 간격·칸 크기·열 수·첫 줄 앞 `..`·이름 입력 칸 수)로 계산해 고른다. 칸 사이 간격에만 걸치면 고르지 않는다. 배치는 세 보기가 그릴 때 기록한다(Grid `note_grid`, List `note_rows`, Detail 은 처음 그린 이름 칸으로 `note_detail`).
- 사각형은 `explorer_marquee_bg`(accent-primary 12%) 채움과 `explorer_marquee_border`(36%) 1px 테두리, 모서리 없음으로 목록 영역 안에만 그린다([수기 접근자](../../design/systems/design-token-mapping.md#explorer-영역-선택-사각형)).
- 포인터가 목록 위·아래 끝 `explorer_autoscroll_zone`(24px, 표시 없음) 띠 안에 있으면 그 쪽으로 스크롤한다. 속도는 `20 줄/초 × t²` 이다. t 는 띠 안쪽 경계에서 0, 목록 끝에서 1 이고, 줄은 지금 보기의 한 줄(Detail·List 행, Grid 칸 줄)이라 세 보기가 같은 시간에 같은 수의 줄을 지난다. 포인터가 목록 밖으로 나가면 거리와 관계없이 20 줄/초를 유지한다. 규칙 상수이며 토큰이 없다.
- 항목 끌기의 자동 스크롤은 각 패스에서 완성된 뷰포트 집합으로 소유자를 정하므로 리사이즈·목록 제거 중에도 현재 영역 하나에만 적용한다([파일 작업](file-operations.md#드래그-앤-드롭)).
- 스크롤 양은 다음 프레임 목록 ScrollArea 앞에서 `Ui::scroll_with_delta_animation`(애니메이션 없음)으로 넣는다(`preview::split`). 휠 입력과 달리 포인터가 목록 밖에 있어도 목록 ScrollArea 가 받는다.
- 영역 선택 중에는 항목 드래그(파일 끌어 놓기)를 시작하지 않는다. 다른 위젯이 먼저 끌기를 가져갔거나 popup·modal 이 떠 있으면 시작하지 않는다.

### 폰트

Appearance → **Explorer** 서브탭에서 surface 폰트를 오버라이드한다 (`appearance.plugin_font_overrides["explorer"]`, `effective_font_for_kind("explorer")` 가 읽음).

### Detail 행 조회

Detail은 공용 Table의 `show_indexed`로 합성 상위 폴더·이름 입력 행을 포함한 전체 행 수와 조회 함수를 넘긴다. Table은 보이는 행만 조회하며, Explorer는 필터 결과의 원본 인덱스로 해당 항목을 빌린다. 전체 목록의 참조 Vec을 프레임마다 만들지 않는다. 선택·열·행 높이·스크롤 번호 계약은 동일하다.


Find의 ASCII 이름·검색어는 빌린 바이트 구간을 대소문자 없이 비교한다. Unicode가 포함되면 기존 Unicode 소문자 매칭을 사용하여 검색·강조 표시의 일치를 유지한다.

타입어헤드는 표시 목록 번호로 이름을 빌리고, 후보의 소문자 비교도 문자 iterator로 수행한다. 전체 항목·이름 사본을 만들지 않는다.

스크롤 대상과 단일 선택의 표시 번호는 내부 탭별 경로 인덱스로 찾는다. 인덱스는 원본 경로를 빌려 해시 슬롯에 표시 번호만 보관한다. 첫 구축은 O(N)이며, 같은 표시 목록의 조회는 평균 O(1)이다. 슬롯 충돌은 실제 경로를 비교하며 다음 슬롯으로 탐사한다. 점유율은 절반 이하이고 슬롯 수는 두 배 항목 수 이상의 최소 2의 거듭제곱이다. 단일 디렉터리의 고유한 파일 이름으로 슬롯을 고르므로 긴 공통 부모 경로를 항목마다 해시하지 않는다. 하위 검색 결과는 같은 이름이 여러 폴더에 있을 수 있어 전체 경로를 해시한다. 두 경우 모두 실제 경로를 비교하여 충돌을 해결한다. 경로 복제와 비교 정렬은 하지 않는다. 목록 세대·검색어·표시 원본이 바뀌거나 검색 묶음을 받으면 다시 만든다. 필터 캐시 준비 전에는 현재 검색어로 직접 조회해 오래된 번호를 쓰지 않는다. 내부 탭을 바꿀 때는 목록·Find·인덱스와 세대를 함께 교환하므로, 그 사이 내용이 바뀌지 않은 탭의 캐시도 재사용한다. 원격 Loaded 목록의 같은 경로·같은 정렬 동기화는 목록을 다시 복제·정렬하거나 세대를 올리지 않는다. 응답·새로고침·정렬 변경이 있을 때만 새 내용을 적용한다. 전체 선택도 항목 사본 없이 원본을 순회한다. 필터가 없는 목록은 불변 목록 스냅샷을 공유하고 선택 경로를 복제하지 않는다.

### 디렉터리 목록 전달

디렉터리 조회는 읽기 worker가 메타데이터 목록 전체를 만든 뒤 돌려주고, View가 현재 정렬을 한 번 적용한다. 분할 목록은 표시하지 않는다. 완성된 전역 정렬 순서, 선택 대상과 상태줄 개수를 한 스냅샷으로 맞추기 위해서다. 메타데이터 조회는 UI 스레드 밖에서 진행하지만, 첫 결과의 정렬과 목록 적용에는 전체 항목 수에 비례하는 비용이 남는다. 하위 검색의 제한된 결과 묶음과 디렉터리 목록 전체 조회는 별개의 경로다.

Linux aarch64의 기존 fixture, release, 캐시를 비우지 않은 100,000항목 단일 관측에서 NVMe 읽기/정렬은 167/81ms, NFS 4.2는 1,689/78ms였다. 1600×1000 Xvfb·lavapipe 실제 GUI의 세 보기에서 첫 완성 목록의 CPU present까지 NVMe 0.63–0.66초, NFS 1.11–1.83초가 관측됐다. 활성 스크롤 프레임 p95는 10.1–11.8ms였고, 스크롤 뒤 3초 idle RSS의 시작/끝 차이는 0–4KiB였다. 읽기/정렬 관측과 GUI 관측은 서로 다른 실행이며, 이 값은 GPU 완료나 다른 매체의 보장값이 아니다.

이 관측에서는 완성 목록 이후 스크롤 비용과 idle 메모리가 항목별 재처리로 계속 늘지 않아 전체 스냅샷 전달을 유지한다. 첫 목록 지연과 O(N) 목록 메모리는 남는다. 분할 수신은 완성 전 정렬·개수·선택의 의미까지 함께 정해야 하므로, 반복 프레임의 전체 목록 사본을 없애는 최적화와 구분한다.

### 선택 경로 집합

선택은 전체 경로의 `Path` 동등성으로 판정한다. 불변 목록을 적용할 때 전체 선택용 경로 인덱스를 한 번 준비한다. 필터가 없는 전체 선택은 현재 목록과 준비된 인덱스를 `Arc`로 공유하므로 입력 시 경로 바이트를 복제하거나 인덱스를 다시 구축하지 않는다. 인덱스 준비는 목록 항목 수에 비례하는 첫 목록 적용 비용과 메모리를 쓰며, 적용 뒤 같은 목록의 전체 선택은 이를 재사용한다. 해시 칸에는 엔트리 번호를 담는다. 토글·일부 선택 해제·retain은 공유 인덱스 사본에서 바꾸므로 원래 목록의 전체 선택 캐시는 유지한다. 선택이 목록 교체 뒤에도 남아 있으면 그 선택이 참조한 옛 목록은 선택과 함께 보관한다. 숨김 파일 재분할은 공유되지 않은 목록을 회수하고, 살아 있는 선택이 공유 중이면 사본으로 새 목록을 만든다. 필터·하위 폴더 검색의 전체 선택은 유효 UTF8 경로를 하나의 소유 버퍼에 담으며, 비UTF8 경로는 원래 OS 바이트를 소유 경로로 보존한다. 선택을 다시 구성할 때 버퍼와 해시 칸을 재사용하고, 필터·선택 해제 뒤에도 확보한 용량은 보관할 수 있다. 파일 작업이 소유 경로를 요구할 때 그 페이로드를 만든다. 일반 디렉터리의 서로 다른 이름은 빌린 파일명으로 해시해 긴 부모 경로의 반복 해시를 피한다. 이름이나 해시가 겹친 버킷은 전체 경로 `HashSet`으로 승격하므로 하위 검색의 동일 이름도 서로 구분한다. 전체 선택은 현재 보이는 항목만 고르고 마지막 항목을 범위 기준으로 삼으며, 선택 식별자·탭별 보관·토글·영역 선택 규칙은 같다. 집합 순서는 보장하지 않으며 경로를 표시하거나 작업에 넘기는 소비처가 기존 정렬 규칙을 적용한다.

### 검색 결과의 역압과 취소

하위 검색 사건 큐는 검색 하나당 16개, 결과 묶음은 최대 256항목이다. 100ms 경과 또는 묶음 상한에서 보내고, 소비가 늦어 큐가 차면 생산을 기다린다. 송신 성공과 역압 대기에서 View를 깨우며, 폴링 한 번에는 최대 16개 사건을 거둔다. Stop·탭 닫기·폴더 이동은 송신 대기 중에도 확인하므로, 가려진 View가 소비를 멈췄다고 worker가 계속 막히지 않는다. Stop으로 완결 사건을 넣지 못하고 송신자가 종료되면 receipt가 큐에 남은 결과를 거둔 뒤 Stopped로 끝낸다. 디렉터리 검색은 항목 사이에도 취소를 확인한다. OS의 실행 중인 metadata·디렉터리 호출 자체를 중단하는 것은 아니다.

## 성능 관측

`TASTY_LOG=tasty::gfx::perf=trace,tasty::explorer::perf=trace`로 프레임별 CPU 렌더 시간(`frame_sample`)과 디렉터리 조회의 항목 수·경과 시간(`directory_read`)을 기록할 수 있다. 기본 로그에서는 출력하지 않으며 디렉터리 경로나 파일 내용은 기록하지 않는다. 프레임 시간은 GPU 완료 시간이나 OS 화면 표시 시간이 아니다. 같은 필터로 표시 항목·선택·썸네일 칸 수(`explorer_state`), 읽기 worker·접수 수(`read_queue`), 파일 작업 대기·실행 수(`file_queue`), 검색 시작·Stop·종료와 폴링 묶음 수(`search_*`)도 기록한다. 검색 큐 포화 로그의 `queued`는 포화 순간의 큐 길이이며 폴링의 `drained`는 그 회차에 꺼낸 수다. 키 누름·휠의 처리 진입(`key_received`·`wheel_received`)과 다음 `frame_sample`의 시각 차이는 입력 처리 진입부터 CPU present 호출 완료까지의 지연이다. OS 입력 생성 전 대기나 GPU 완료 시간을 포함하지 않는다. 전체 선택은 `selection_started`·`selection_packed`·`selection_built`·`selection_applied`로 경로 바이트 구성·해시 삽입·기존 선택 교체 구간을 따로 기록한다. 경과 시간은 입력 대기·렌더 시간을 포함하지 않으므로 앞뒤 입력·프레임 시각과 함께 읽는다. 입력 주입 시각과 프레임 로그를 함께 대조할 때도 측정 환경과 포함한 구간을 구분한다. 목록 적용 시의 전체 선택 인덱스 준비는 `selection_snapshot_prepared`의 항목 수와 경과 시간으로 별도 기록한다.

## 검증 기준

파일 작업 시험의 파일시스템 전제와 디스크 부족 주입 범위는 [파일 작업 시험](file-operations.md#파일-작업-시험의-파일시스템-전제)에 있다.

- Given 내부 탭 A(X)·B(Y)·C(X) When A에서 선택·Find를 정한 뒤 B·C를 거쳐 돌아온다 Then A의 목록·선택·Find가 Loading 없이 돌아오며 같은 경로 C는 독립이다(`tab_state_tests.rs`).
- Given 20,000개 목록의 Detail·List·Grid When A의 15,000번째 항목까지 스크롤하고 같은 폴더의 B를 연 뒤 A로 돌아온다 Then B는 맨 위에서 시작하고 A는 원래 항목의 화면 위치와 선택을 유지한다(`internal_tabs_restore_scroll_in_all_three_views`).
- Given A의 로컬 읽기·검색 또는 원격 목록 응답 대기 When B로 바꾸거나 A를 닫는다 Then 살아 있는 A의 결과만 A에 붙고 B에는 섞이지 않으며 닫힌 A의 응답은 폐기된다(`tab_state_tests.rs`).

- Given 윈도우 두 개 When 각 윈도우의 explorer 에서 다른 폴더를 즐겨찾기에 추가한다 Then 두 윈도우의 Favorites 에 둘 다 보이고 재시작 뒤에도 둘 다 남는다(`engine_action.rs` 의 `favorites_added_in_two_windows_both_survive_a_restart`).
- Given 윈도우 두 개에 같은 즐겨찾기가 보인다 When 한 윈도우에서 즐겨찾기를 제거한다 Then 다른 윈도우의 Favorites 에서도 바로 사라진다(`view_frame.rs` 의 `a_shared_list_change_redraws_every_window_once`).
- Given 로컬 explorer When 하위 폴더로 이동한다 Then 목록이 `Loading` 을 거쳐 새 폴더의 항목으로 바뀌고, 이전 폴더의 늦은 결과는 반영되지 않는다(`src/app/local_reads.rs` 의 `replaced_directory_receipt_cannot_publish_its_old_result`, mirror 는 `view.rs` 의 `apply_remote_list_dir_result_ignores_stale_request_id`).
- Given 사이드바 트리에서 하위 폴더를 펼친 로컬 explorer When 파일 작업 뒤처럼 같은 폴더를 다시 읽으라는 요청(`request_reload`)이 온다 Then 펼친 트리 캐시를 비우고 목록과 함께 다시 읽는다(`view.rs` 의 `an_explicit_reload_of_the_same_folder_rereads_the_tree`). `F5` 의 트리 재읽기는 아래 `a_refresh_rereads_the_same_folder_while_the_old_list_and_tree_stay` 가 잰다.
- Given 목록을 읽어 보이는 로컬 explorer 와 펼친 트리 폴더 When `F5` 를 누른다 Then 결과가 올 때까지 보이던 목록·하위 목록·선택을 그대로 두고 `Loading` 으로 바꾸지 않으며, 결과가 오면 사라진 항목만 선택에서 빠진다(`poll_tests.rs` 의 `a_refresh_rereads_the_same_folder_while_the_old_list_and_tree_stay`).
- Given 포커스를 가진 탭의 로컬 explorer When 다른 프로그램이 현재 폴더에서 항목 하나를 지우고 하나를 만든다 Then 다음 확인에서 보이던 목록을 둔 채 다시 읽어 새 목록으로 바뀌고, 선택은 사라진 항목만 빠진다(`view/poll_tests.rs` 의 `a_changed_folder_is_reread_while_the_old_list_stays`).
- Given 포커스를 가진 탭의 로컬 explorer When 폴더가 바뀌지 않은 채 확인한다 Then 목록을 다시 읽지 않고 창도 다시 그리지 않는다(`an_unchanged_folder_is_not_read_again`). 펼친 트리 폴더 둘 중 하나만 바뀌면 그 폴더만 다시 읽고, 결과가 올 때까지 보이던 하위 목록을 둔다(`only_a_changed_expanded_tree_folder_is_read_again`). 트리에 펼친 현재 폴더가 바뀌면 목록만 한 번 읽고 그 결과로 트리 항목도 바꾼다(`the_current_folder_in_the_tree_is_read_once_and_its_node_is_replaced_in_place`).
- Given 다른 탭에 가려진 explorer 와 mirror explorer When 주기가 지난다 Then 확인하지 않고 App 도 그 때문에 깨어나지 않는다. 그 탭이 포커스를 얻으면 바로 확인한다(`a_tab_without_focus_is_never_checked`, `gaining_focus_checks_at_once_and_then_every_interval`, `a_panel_not_drawn_this_frame_stops_waking`, `a_mirror_explorer_is_not_checked`).
- Given 포커스 탭에 로컬 explorer 가 있는 창 When 다른 앱이 포커스를 가져간 동안 주기가 지난다 Then 확인하지 않고 깨어나지도 않는다. 창이 포커스를 되찾은 프레임에 주기를 기다리지 않고 확인한 뒤 주기를 다시 시작한다(`view/poll_tests.rs` 의 `only_the_focused_tab_of_the_focused_window_is_checked_and_regaining_focus_checks_at_once`).
- Given 로컬 explorer 가 A/B/C 를 보고 있다 When B 가 통째로 지워져 읽기 오류 화면에서 "상위 폴더로" 를 누른다 Then A 로 간다(`local_reads.rs` 의 `a_missing_folder_names_its_nearest_existing_ancestor`, `view.rs` 의 `go_up_from_a_vanished_folder_skips_vanished_parents`).
- Given 읽기 권한이 없는 폴더 When 들어간다 Then 권한 거부 화면이 나오고 툴바·트리는 그대로 쓸 수 있다.
- Given 폴더를 우클릭한다 When "새 탭으로 열기"를 고른다 Then 우클릭한 surface 의 pane 에 그 폴더를 cwd 로 하는 explorer Pane 탭이 생기고 원래 explorer 는 바뀌지 않는다.
- Given mirror explorer When 폴더를 탐색하고 파일을 더블클릭한다 Then 목록은 원격 조회로 오고, 파일은 원격에 탭으로 열리거나 열 수 없다는 토스트가 나온다. 로컬 파일시스템은 읽지 않는다.
- Given 원격 쓰기를 받지 않는 mirror explorer When 잘라내기·붙여넣기·휴지통·이름 변경·새 항목을 메뉴로 시도한다 Then 행이 꺼져 있고 툴팁에 이유가 있다.
- Given 원격 쓰기를 받지 않는 mirror explorer When 같은 조작을 단축키로 시도한다 Then 그 칸에 이유 info 토스트가 나오고 아무것도 바뀌지 않는다.
- Given mirror explorer When 새 탭 열기를 시도한다 Then 메뉴에 없거나 `remote_write_unsupported` 토스트가 나오고 아무것도 바뀌지 않는다.
- Given 붙여넣기가 진행 중이다 When 그 surface 를 닫는다 Then 이미 시작한 작업은 끝까지 실행되고 닫힌 surface 에 결과 카드나 목록 갱신을 내지 않는다. 아직 시작하지 않은 요청은 실행되지 않는다. 이름 충돌 답을 기다리던 작업은 취소되고 기존 항목은 그대로다.
- Given 로컬 explorer 의 폴더에 "New folder" 가 있다 When 툴바의 New folder 를 누른다 Then 목록 맨 위에 "New folder 2" 가 전체 선택된 입력이 열리고, Enter 를 누르면 그 폴더가 생겨 정렬 자리에서 선택된다(`explorer_files/tests.rs` 의 `a_created_entry_is_selected_only_while_its_folder_is_still_shown`, `create/tests.rs`).
- Given 이름 입력이 열려 있다 When 이미 있는 이름으로 Enter 를 누른다 Then 입력은 열린 채 "already exists" 오류가 보이고 디스크는 바뀌지 않는다. Esc 를 누르면 아무것도 만들지 않고 닫힌다.
- Given 탐색기 목록에서 파일 둘과 폴더 하나를 골랐다 When `Enter` 를 누른다 Then 그 폴더로 들어가고 파일은 열리지 않는다. 폴더를 하나 더 고른 뒤 누르면 아무 일도 일어나지 않는다.
- Given 쓸 수 없는 폴더 When 툴바를 본다 Then New folder·New file 이 비활성이다. Given mirror explorer When 툴바·메뉴를 본다 Then 두 명령이 없다.
- Given 로컬 Unix explorer 에 쓸 수 없는 하위 폴더가 보인다 When 그 폴더 행 메뉴의 New folder 를 누른다 Then 이름 입력은 열리지 않고 `cannot_write` 토스트가 뜬다. 쓸 수 있는 하위 폴더면 확인을 받은 뒤 그 폴더 아래에 입력이 열린다(`explorer_create.rs` 의 `a_folder_row_that_cannot_be_written_is_refused_before_the_input_opens`).
- Given 새 폴더를 요청했다 When 결과가 오기 전에 같은 폴더에서 다른 항목을 고른다 Then 만든 뒤에도 고른 항목이 선택으로 남는다(`explorer_files/tests.rs` 의 `a_created_entry_leaves_a_selection_made_after_the_request`).
- Given macOS·Windows 로컬 폴더나 Windows 형식 경로의 원격 폴더에 `Documents` 가 있다 When `documents` 로 Enter 를 누른다 Then 입력 아래에 "already exists" 오류가 보인다. Linux 로컬 폴더에서는 그대로 만든다(`create/tests.rs` 의 `a_name_differing_only_in_case_is_taken_where_the_file_system_ignores_case`·`a_local_name_folds_case_only_on_macos_and_windows`).
- Given 로컬 explorer 에 Report.pdf·report-draft.txt·notes.md 가 있다 When Find 를 열고 "re" 를 입력한다 Then 두 항목만 남고 맞는 부분이 강조되며 바에 "2 of 3" 이 보인다. 첫 `Esc` 는 글자를 지우고 둘째 `Esc` 는 바를 닫는다(`find/tests.rs`).
- Given 거르기 전 notes.md 를 골랐다 When 그 항목이 걸러진다 Then 선택에서 빠져 이후 명령이 닿지 않는다(`find/tests.rs` 의 `the_filter_hides_rows_that_do_not_match_and_counts_what_is_shown`).
- Given 하위 폴더에 맞는 파일과 읽을 수 없는 폴더가 있다 When Subfolders 를 켠다 Then 결과가 들어오는 대로 채워지고 Detail 에 Folder 열이 생기며, 끝나면 "{n} found · 1 folder skipped" 가 보인다. 링크로 된 폴더는 들어가지 않는다(`local_reads/search/tests.rs`).
- Given 로컬 explorer 의 네 번째 항목이 폴더다 When 그 폴더 메뉴의 New file 을 고른다 Then 세 보기 모두 입력이 그 폴더 바로 다음 자리에 열리고 앞쪽 항목 간격은 그대로다(`virtual_tests.rs` 의 `a_new_item_inside_a_folder_opens_right_after_that_folder`). Detail·List 는 16 들여 쓴 줄 뒤에 "in {folder}" 가 보이고 폴더 행에 놓을 대상 표시가 있다.
- Given 칸 폭이 440 보다 좁다 When 툴바를 그린다 Then New folder·New file·Find·Preview 가 모두 More 하나로 접히고, 메뉴의 view 행은 지금 상태에 따라 Find·Close find, Show preview·Hide preview 를 쓴다(`commands.rs` 의 `preview_sits_in_the_view_group_and_folds_into_more_with_it`, `explorer_create.rs` 의 `more_rows_name_the_action_they_take`).
- Given 로컬 explorer 에 .env·.git·a.txt·b.txt 가 있다 When 숨김 파일이 꺼진 채(기본) 목록을 그린다 Then a.txt·b.txt 만 보이고 상태줄은 "2 items · 2 hidden" 이다. When 숨김 파일 단축키를 누른다 Then 네 항목이 이름 순으로 보이고 .env·.git 의 이름·글리프가 text-muted 다. 다시 끄면 숨김 항목의 선택이 풀린다(`view/hidden.rs` 의 `hidden_items_leave_the_listing_until_shown_then_return_in_sort_order`).
- Given 숨김 파일을 켠 explorer When 레이아웃을 저장하고 다시 연다 Then 켜진 채 복원되고, 끔(기본)이면 snapshot 에 `show_hidden` 키가 없다(`builtins.rs` 의 `explorer_hidden_files_toggle_rides_the_snapshot_and_a_preset`).
- Given 이름에 점이 없는 항목에 Windows 숨김 속성이나 macOS `UF_HIDDEN` 이 있다 When 숨김 파일이 꺼진 채 목록을 그린다 Then 그 항목은 목록에서 빠지고 숨김 수에 들어간다. 비트 해석은 `fs_list/os_hidden.rs` 의 단위 시험, 목록 판정은 `hidden.rs` 의 `an_os_hidden_mark_hides_a_name_without_a_dot` 이 확인한다. 실제 표시를 읽는 배선은 같은 파일의 OS 전용 시험이 확인한다. Windows 는 숨김 속성으로 만든 파일, macOS 는 `chflags hidden` 한 파일과 그 파일을 가리키는 링크(링크는 숨김 아님)를 목록으로 읽으며, 그 OS 의 CI 에서만 돈다. Linux 에서는 OS 표시가 늘 거짓이다.
- Given 원격 호스트 폴더에 대상이 있는 파일 링크·폴더 링크·대상이 없는 링크가 있다 When mirror explorer 로 그 폴더를 연다 Then 폴더 링크는 폴더로 트리에도 나오고, 파일 링크는 이름 뒤 link 글리프, 대상이 없는 링크는 accent-warning link 글리프와 "Broken link" 로 세 보기에 보이며 열면 broken link 원인 toast 가 뜬다. 링크 상태를 싣지 않는 server 면 오류 없이 지금처럼 링크 구분이 없다(위 "심볼릭 링크"의 시험).
- Given 원격 호스트(Windows·macOS)에 OS 가 숨김으로 표시한 항목이 있다 When 원격 탐색기에서 숨김 파일이 꺼진 채 목록과 트리를 그린다 Then 그 항목은 빠지고 숨김 수에 들어간다(`output/remote_entry_tests.rs`, `wire/list_dir.rs` 의 `a_new_server_mark_is_read_as_os_hidden`, `list_dir_wire.rs` 의 `only_an_os_hidden_entry_carries_the_mark`). 원격 호스트의 실 기기 측정은 하지 않았다.
- Given `os_hidden` 을 싣지 않는 옛 server When 새 client 가 목록을 받는다 Then 오류 없이 점 규칙만으로 숨긴다(`an_old_server_listing_reads_without_the_mark`). Given 새 server When `os_hidden` 을 모르는 옛 client 가 받는다 Then 그 필드를 무시하고 읽는다(`an_old_client_reads_a_marked_entry_without_error`).
- Given 숨김 폴더 안의 일반 파일과 일반 폴더의 파일이 있다 When 하위 검색 중 숨김 표시를 바꾼다 Then 옛 영수증이 취소되고 큐에 있던 결과도 버려지며 새 정책으로 다시 찾는다(`find/tests.rs`의 `changing_hidden_cancels_the_receipt_discards_queued_hits_and_searches_again`).
- Given 일반 파일·폴더와 여러 확장자가 있다 When `note ext:rs ext:md`를 입력한다 Then 거르기와 하위 검색이 같은 파일을 보이고 제외된 항목의 선택은 풀린다(`extension_filter_matches_recursive_search_and_removes_excluded_selection`).
- Given 하위 폴더 검색이 5,000 개보다 많이 맞는다 When 검색이 끝난다 Then 5,000 개만 남고 바에 "5,000+ found · stopped", 상태줄에 범위를 좁히라는 안내가 보인다(`local_reads/search/tests.rs` 의 `a_search_stops_at_the_hit_cap_and_keeps_what_it_found`, `find/tests.rs` 의 `one_skipped_folder_and_the_hit_cap_have_their_own_words`).
- Given 거르기 검색어에 맞는 이름이 없다 When 목록을 그린다 Then 바는 "0 of N", 목록 자리는 "No names match “{query}”" 이다(`find/tests.rs`).
- Given 거르는 중이다 When 검색어와 목록이 그대로인 채 프레임이 이어진다 Then 다시 거르지 않고, 검색어나 목록이 바뀌면 다시 거른다(`find/tests.rs` 의 `the_filter_is_not_run_again_while_the_query_and_the_list_stay`, `a_new_query_or_a_new_list_filters_again`).
- Given 지금 폴더 항목 둘과 하위 폴더 검색에서 고른 항목 하나가 선택돼 있다 When 검색어를 바꾼다 Then 새 검색어에 맞는 지금 폴더 항목만 선택에 남고, 하위 폴더 검색 중에는 그 결과에 든 항목만 남는다(`find/tests.rs` 의 `a_new_query_keeps_only_the_selected_items_it_still_shows`, `subfolder_results_keep_only_the_selected_hits`).
- Given Find 바가 열려 있다 When 하위 폴더로 이동한다 Then 바가 닫힌다(`find/tests.rs` 의 `leaving_the_folder_closes_the_bar`).
- Given 항목 20,000개 폴더 When detail·list·grid 로 본다 Then 화면에 걸친 항목만 그리고(200개 미만), 15,000번째 항목을 고르거나 타입어헤드로 18,765번째로 가면 그 항목이 화면 가운데로 오고 클릭하면 그 항목이 선택된다. 이름 입력을 연 채 목록 끝으로 스크롤해도 입력은 열린 채 포커스를 유지한다(`explorer/virtual_tests.rs`).
- Given 항목 20,000 개 폴더 When 세 보기에서 End 를 누른다 Then 마지막 항목만 선택되고 화면으로 스크롤된다. 그 뒤 PageUp 은 한 화면만큼 가고, Shift+Up 두 번은 두 줄(Grid 는 열 수 × 2)을 더한다. Detail·List 의 오른쪽 키는 움직이지 않고 Grid 는 한 칸 간다(`virtual_tests/keyboard_select.rs`).
- Given 키로 현재 항목을 옮겨 테두리가 보인다 When 다른 항목을 클릭한다 Then 테두리가 숨고 다음 아래 키는 클릭한 항목 다음으로 간다.
- Given 항목 10 개인 Detail·List When 목록 아래 빈 곳에서 7 번째 줄까지 끈다 Then 7~9 번째가 선택되고 놓은 뒤에도 남는다.
- Given 항목 20,000 개 Grid When 오른쪽 빈 곳에서 아래 끝 띠로 끌고 머문다 Then 목록이 스크롤되며 선택이 화면 밖 항목까지 늘고, 처음 누른 줄은 선택에 남는다.
- Given 내부 탭 둘을 열고 정렬을 바꾼 explorer When 재시작한다 Then 탭·cwd·current·뷰 모드·정렬이 복원되고 히스토리와 선택은 비어 있다.

## 관련

- [work-area](../../features/work-area/index.md)(Surface/Tab/Pane 계층) · [file-handler](../../features/file-handler/index.md)(파일 열기 위임) · [convert-surface](../../features/convert-surface/index.md)(explorer 로/에서 변환) · [keybindings](../../features/keybindings/index.md) · [settings](../../features/settings/index.md)(폰트/단축키 탭)

Windows·macOS·Linux 로컬 항목의 외부 앱 Copy 드래그, X11 프록시 수신 규칙과 플랫폼별 제한은 [파일 작업 계약](file-operations.md#외부-앱으로-끌어-내기)을 따른다.
