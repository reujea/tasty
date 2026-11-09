# 네이티브 파일 피커 (로컬+원격 겸용)

- **Status**: Implemented
- **주체**: 로컬 사용자 (Tools 메뉴 트리거 · 탭 스트립 메뉴 "새 HTML..." 트리거 · 설정·플러그인·프리셋의 로컬 파일/폴더 선택 · 이미지 다른 이름으로 저장) + plugin(`file_picker.trigger` IPC — plugin 호출자 전용, CLI·agent 는 `-32016`)
- **설계**: [원격 attach (Remote attach)](../remote-attach/index.md) (attach 커스텀 이벤트 채널 + 하이브리드 신뢰 모델), [내부 팝업 시스템](../../design/systems/popup.md) (plugin 트리거 — 즉시 ack + 이벤트 push). 관련: [파일 핸들러 (File handler)](../file-handler/index.md)(옛 `fs.pick_file` 제거 — 이 피커가 그 자리를 대신한다)
- **코드**: `src/adapters/ui/popup/file_picker.rs`(popup wrapper/view/action), `src/core/fs_list.rs`(공유 디렉토리 나열), `src/app/local_reads.rs`(`directory` 로컬 읽기 요청 · `poll_file_picker_reads` 결과 반영), `src/adapters/ui/file_open_tool.rs`(Tools 메뉴·단축키 호출자 — 열기와 결과 적용), `src/adapters/ui/new_tab_picker.rs`(탭 스트립 메뉴 "새 HTML..." 호출자 — 열기와 결과 적용, 진입은 `src/view/main/redraw.rs`의 `apply_pane_menu_selection`), `src/adapters/ipc/handler/file_picker.rs`(`file_picker.trigger` — plugin 호출자, `"file_picker.result"` push), `src/app/dispatch/file_picker.rs`(결과를 꺼내 호출자에게 넘기는 drain), `src/remote/server/content_queries.rs`(서버측 `handle_list_dir_request`), `src/app/attach_client/wire.rs`(client 원격 파싱 + `MirrorEvent::ListDirResult`), `crates/tasty-ipc/src/stream_hub.rs`(`ListDirRequestMsg` 분류), `crates/tasty-plugin-markdown/src/popup.rs`(파일 열기 팝업 Browse caller), `crates/tasty-plugin-markdown/src/browse.rs`(주소창 폴더 버튼 caller), `crates/tasty-plugin-image/src/picker.rs`(이미지 저장 경로 caller), `src/adapters/ipc/handler/picker_proofs.rs`(확정 선택의 사용자 출처 증명), `src/view/file_chooser.rs`(Settings·Plugins·Preset의 로컬 전용 재사용)
- **화면**: 없음 (popup 은 갤러리 specimen `crates/tasty-gallery/src/catalog/components/file_picker.rs` 로 시각 확인)

## 목적

Tasty 안에서 파일을 선택하는 창이다. 로컬 파일시스템과 attach mirror workspace의
원격 파일시스템을 같은 UI로 탐색한다. OS 파일 선택기 대신 자체 창을 사용하는 이유는
[파일 핸들러 (File handler)](../file-handler/index.md)에 설명한다.

## 내부 동작 (headless-valid)

### 피커와 호출자의 책임

피커는 호출하면 경로 문자열 배열을 돌려주는 함수처럼 동작한다. 피커 자신은 고른 경로로 파일을 열거나
탭을 만들거나 클립보드에 쓰지 않으며 intent·toast 도 만들지 않는다. 결과의 의미는 피커를 연 호출자가 정한다.

- **피커**: 호출자가 넘긴 위치(로컬 폴더, 또는 mirror workspace 와 원격 폴더)를 둘러본다. 확정하면 경로
  배열, 취소하면 빈 배열과 취소 표시를 `FilePickerData.result` 에 남긴다. 원격 목록은 읽기 전용 나열이다.
- **호출자**: 열기 전에 둘러볼 위치(`FilePickerLocation`)를 정해 피커에 넘기고, 요청 식별자
  (`FilePickerRequestId`)와 함께 자기 요청 정보로 보관한다. 결과로 하는 일과 실패 안내도 호출자가 정한다.
- **요청 식별**: 피커를 열 때마다 새 식별자를 쓴다. 결과는 그 식별자와 함께 호출자에게 가고, 호출자는
  보관한 값과 같은 결과만 한 번 적용한 뒤 보관한 요청을 비운다. 같은 호출자가 다시 열었다면 이전 요청의
  결과는 적용하지 않는다. 취소도 호출자에게 전달된다.
- **결과 전달**: App 루프의 `dispatch_pending_file_picker_results` 가 매 반복 닫힌 피커의 결과를 한 번
  꺼내(`take_outcome`) 피커 상태를 지우고 `deliver_outcome` 으로 호출자들에게 넘긴다. Tools 메뉴처럼
  피커를 연 직후 닫히는 메뉴도 결과는 이 drain 지점에서 처리된다.

### 로컬/원격 판별

어디를 둘러볼지는 피커의 입력이다. 호출자는 `popup::file_picker::locate` 로 출발 surface 가 속한
workspace, 찾지 못하면 호출자가 고른 workspace 의 `Workspace.mirror` 플래그를 1 회 확인해 로컬/원격을
고정한다. 원격이면 결과를 열 때 출발점으로 쓸 그 mirror workspace 의 surface 도 이때 정한다. 출발
surface 가 없으면 그 workspace 에서 이 창이 선택한 surface(없으면 첫 surface)다. 피커는 받은 위치를
`FilePickerData.mirror_ws_id: Option<u32>`(`Some` 이면 원격)로만 쓴다.

- **Tools 메뉴·단축키**(`src/adapters/ui/file_open_tool.rs::open`): 출발 surface 는 focus surface,
  없으면 활성 workspace 다.
- **탭 스트립 빈 곳 메뉴의 새 HTML...**(`src/adapters/ui/new_tab_picker.rs::open`): 메뉴를 연 pane 의
  선택 탭 cwd 에서 로컬로 출발하고, 필터는 `html`·`htm`·`svg` 다. 메뉴를 연 pane 이 mirror workspace 에
  속하고 그 연결이 HTML entry viewer 를 협상하지 않았거나 client 의 html kind 를 `com.tasty.html` 이 등록하지
  않았으면 피커를 열지 않고 `explorer.state.remote_open_unsupported` 토스트만 띄운다. 그 상태의 html 은 mirror 에서
  내용이 보이지 않아(원격 파일 열기가 같은 조건에서 html 핸들러를 거절하는 [원격 attach (Remote attach)](../remote-attach/index.md)와
  같은 이유) 원격에 탭을 만들어도 사용자가 볼 수 없기 때문이다.
- **plugin `file_picker.trigger`**: 아래 "시작 위치" 절의 `origin_surface_id`, 없거나 찾지 못하면 활성
  workspace 다.

### 로컬 브라우징

popup wrapper 는 `crate::app::local_reads::directory` 로 디렉토리 읽기를 요청하고
`FpLoadState::Loading` 으로 전이한다. App 의 로컬 읽기 worker(동시 실행 최대 4개,
[요청 문맥과 headless](../../dev-guide/model-view-split.md#요청-문맥과-headless))가
`crate::core::fs_list::read_dir_entries` 로 읽는다(host 프로세스 안 — 별도 IPC 왕복 없음).
UI 스레드는 읽기를 기다리지 않는다. `poll_file_picker_reads` 가 결과를 받아 `sort_entries` 로 이름순 정렬하고
`Loaded`/`Empty` 로, 실패하면 `ErrorPerm`(권한 거부는 번역 문구, 그 밖은 OS 오류 문구)으로 전이한다.
popup 이 닫히거나 다른 디렉토리로 옮기면 이전 요청의 receipt 를 버려 늦은 결과가 새 표시를 바꾸지 않는다.

### 원격 브라우징 (attach 커스텀 이벤트)

원격은 attach 세션이 이미 열어둔 `StreamTag::Control` 채널을 그대로 재사용한다(스크린샷 캡처
기능의 `capture_chunk`/`capture_commit`/`capture_result` 와 동일한 "`StreamControl` enum 이
인식 못 하는 `event` 태그" 패턴).

1. wrapper는 원 mirror projection token과 `{ local_ws_id, request_id, dir }`를
   `EngineAction::ListDirectory`로 보내고 popup을 `FpLoadState::Loading { request_id, sent_at }`로 전이한다.
   App은 projection과 선택적 원 target이 여전히 유효할 때만 해당 `RemoteState.pending_list_dir_forward`에 넣는다.
2. App 이 `about_to_wait` 에서 큐를 drain 해 attach 세션 writer 로 `list_dir_request` 프레임을
   전송(`src/app/attach_client/forward.rs::send_list_dir_request`).
3. 원격 인스턴스 서버측(`src/remote/server/content_queries.rs::handle_list_dir_request`)이
   **attach 점유 = 신뢰**(`engine.live.occupancy.client_holds_workspace(client_id)`)만으로 인가 판정 —
   별도 permission 게이트 없음. 인가되면 로컬 worker 와 같은 `read_dir_entries` 로 대상 디렉토리를 읽어
   `list_dir_result` 로 회신.
4. client 의 attach reader thread 가 `list_dir_result` 를 파싱해 `MirrorEvent::ListDirResult` 로
   변환하고, `apply_attach_client_output` 이 그 popup 상태(`request_id` 일치 확인 — stale reply
   무시)에 직접 반영한다.

### 타임아웃(soft timeout + mirror 소멸 관측)

- `Loading` 상태에서 8초(`LIST_DIR_SOFT_TIMEOUT`) 안에 응답이 없으면 wrapper 가 매 프레임
  `sent_at.elapsed()` 를 확인해 `ErrorConn` 으로 전이한다.
- 별도로, mirror workspace 자체가 사라지면(원격 disconnect 로 `cleanup_mirror_workspace` 가
  실행돼) `engine.find_workspace_index_for_id` 조회가 실패하는 것을 관측해 즉시 `ErrorConn` 으로
  전이한다 — attach 세션의 raw `disconnected` 플래그는 App 소유라 popup wrapper 에서 직접 읽을
  수 없어, 그 대신 이 "mirror workspace 소멸"이라는 상위 결과로 판별한다.

### wire 포맷 (epoch ↔ SystemTime)

`DirEntryInfo`(`src/core/fs_list.rs`)는 로컬/원격 어디서 만들어졌든 항상 `Option<SystemTime>` 을
든다. wire 조립/파싱 경계(`list_dir_entry_wire`/`parse_list_dir_result`)에서만
`modified_unix: u64`(unix epoch 초)로 변환한다. OS 숨김 표시는 표시가 있는 항목에만 `os_hidden: true` 로 싣는 선택 필드다. 없으면 받는 쪽이 거짓으로 읽으므로 옛 server·옛 client 와 섞여도 오류가 없다(탐색기의 숨김 판정은 [Explorer](../../surfaces/explorer/index.md#숨김-파일)). 링크 상태도 링크 항목에만 `link: "valid"|"broken"` 으로 싣는 선택 필드이며 같은 규칙으로 섞여도 오류가 없다. 없거나 모르는 값은 링크 아님이다(탐색기의 표시는 [Explorer](../../surfaces/explorer/index.md#심볼릭-링크)). 사람이 읽는 포맷(`"YYYY-MM-DD"`,
`fs_list::format_modified`)은 view 렌더 직전에만 계산 — 로컬/원격 어느 쪽도 이 함수 하나를
공유한다.

### 대형 디렉토리 truncation (프레임 크기 안전장치)

attach 채널의 프레임 하드 상한(`crate::ipc::stream::MAX_FRAME_LEN`, 1MiB)을 넘는 `list_dir_result`
는 `write_frame` 이 에러를 반환하고, 그 결과 그 attach 세션의 write thread 전체가 종료된다(다른
forward/tap 도 동반 — file picker 뿐 아니라 mirror 연결 자체가 끊긴다). `handle_list_dir_request`
는 이를 막기 위해 entries 를 직렬화하며 누적 바이트를 추적하다 예산
(`LIST_DIR_ENTRIES_BYTE_BUDGET`, 700KiB)을 넘기기 직전에 멈추고 `truncated: true` 를 wire 에
싣는다. client 는 `truncated` 를 받으면 toast(`filepicker.remote_listing_truncated`)로 알린다 —
현재 UI 는 "다음 페이지" 개념이 없어 사실상 상위 700KiB 분량만 보여주는 제약이다(비-목표 참고).

### 확정(Confirm)

피커의 확정은 고른 경로를 결과로 남기는 데서 끝난다. 원격 목록에서 확정해도 원격 파일 **내용**을 이
세션으로 가져오는 fetch 는 없다. 확정 뒤의 동작은 호출자별로 다르다.

| 호출자 | 결과 처리 | 위치 |
|---|---|---|
| Tools 메뉴·단축키 | 로컬 경로마다 `DomainIntent::DispatchFile { depth: Deep, origin_surface_id: None, dispatch_origin: User }` 를 `from_user_menu("file_picker_confirm")` 로 보내 파일 핸들러로 연다. 원격 경로는 위치 확정 때 정한 mirror surface 를 `origin_surface_id` 로 실은 같은 `DispatchFile` 로 원격 파일을 연다(원격 탐색기 더블클릭과 같은 경로). 원격에 열 수 있는 핸들러가 없으면 기존 안내가 뜬다. 클립보드는 바꾸지 않는다. | `src/adapters/ui/file_open_tool.rs::apply_outcome` |
| 탭 스트립 "새 HTML..." | 경로마다 `DomainIntent::CreateTab { pane_id, kind: "html", surface_params: {"url": 경로}, activate: true }` 를 사용자 컨텍스트 메뉴 출처로 보낸다. pane 은 ID 로 고정하므로 피커가 열린 동안 활성 pane·탭·workspace 가 바뀌어도 메뉴를 연 pane 에 만든다. 그 pane 이 닫혔으면 아무것도 만들지 않는다. | `src/adapters/ui/new_tab_picker.rs::apply_outcome` |
| plugin `file_picker.trigger` | 요청한 plugin 에 `"file_picker.result"` 만 보낸다. 호스트는 경로로 아무것도 열지 않는다. | `src/adapters/ipc/handler/file_picker.rs::deliver_result` |
| 설정 창 파일 선택 | 연 화면(`consumer`)이 경로를 받아 입력칸 채우기·내보내기·가져오기에 쓴다. | `src/view/settings/ui.rs::apply_file_chooser_outcomes` |

취소하면 어느 호출자도 동작하지 않는다. Tools 원격 열기에서 보관한 surface 가 피커가 열린 동안
닫혔으면 같은 mirror workspace 의 다른 surface 를 출발점으로 쓴다. 그 workspace 까지 사라졌으면
원격 경로를 로컬로 오해해 열지 않도록 warn 로그만 남기고 무시한다.

- **디렉토리는 파일로 확정되지 않는다**: [열기] 가 폴더 하나를 골랐을 때 하는 일은 **그 폴더로
  들어가는 것**이다 — 더블클릭 말고 키보드로 내려가는 길이 그것이다. 그 밖에는 선택된 엔트리가
  전부 파일일 때만 활성화되고(`draw_file_picker_view` 의 `can_confirm`), wrapper 의
  `apply_action` 도 확정 직전에 같은 조건을 다시 확인한다(방어적 중복 검증).
- **행 제스처 표** — 저장/열기가 같은 view 라 모든 갈래가 두 모드에서 성립해야 한다.

  | | 단일 클릭 | 더블클릭 |
  |---|---|---|
  | 폴더 | 고른다(두 모드) | 들어간다(두 모드) |
  | 파일 | 고른다 — 저장 모드는 이름 칸도 채운다 | 열기는 확정 · 저장은 고르기까지 |

  폴더를 고르면 footer 에 톤 없는 안내 줄이 표시된다(`folder` 글리프 · caption · `text-muted`).
  저장 모드는 `filepicker.folder_not_save_target`(그것이 저장 대상이 아니라는 사실), 열기
  모드는 `filepicker.folder_open_enters`(확정하면 들어간다). 열기 문구의 `{confirm}` 자리는
  `confirm_label` 에서 오므로 문구와 버튼 이름이 갈리지 않는다. 경고가 아닌 이유는 잘못된
  것이 없기 때문이다 — 확정 버튼이 읽는 값은 이름 칸 하나이고, 고른 폴더는 그 값을 못 바꾼다.
  그 사실을 **읽는 자리에서** 말한다.

### 목록의 긴 파일명

이름 열은 크기·수정일 열을 제외한 남은 폭을 사용하고, 긴 이름은 끝을 `…`로 말줄임한다.
말줄임은 표시만 바꾼다 — 선택·더블클릭·확정은 전체 파일명을 사용한다. 열 배치는
[갤러리 매핑](../../design/systems/design-gallery-mapping.md)의 `FpRow`를 따른다.
`src/adapters/ui/popup/file_picker/layout_tests.rs`의 `filenames_stay_inside_the_name_column_at_default_width`와
`filenames_stay_inside_the_name_column_at_narrow_width`는
기본·좁은 popup에서 긴 이름과 짧은 이름의 실제 galley가 콘텐츠 안에 있고 크기·수정일과
겹치지 않는지 검사한다.

<a id="목록의-긴-파일명-1"></a>

### 창 구성 — 헤더는 하나다

본체 file picker popup 은 셸 타이틀바 없이(`headless`) 뷰가 그린 헤더 한 줄로 시작한다. 헤더는
파일 글리프 · 제목 · 원격이면 host 배지 · 닫기 ✕ 다. 이 헤더 줄이 이동 손잡이다 — 뷰가 그린 헤더
사각형을 매니저에 보고하고(`DragHandle::Region`), 빈 곳을 끌면 popup 이 움직인다. 헤더 안의 ✕ 는
드래그보다 먼저 입력을 받는다. 닫는 길은 헤더 ✕ · Cancel · Esc 이고 셋 다 `Cancelled` 를 남긴다.

헤더 아래 path bar 줄은 `bg-sidebar` 로 창 좌우 끝까지 칠해져 헤더·목록(`bg-panel`)과 구분된다.

목록이 보일 때는 path bar 아래에 열 이름 줄(목록 머리)이 있다. 이름·크기·수정일 열 이름을 mono
micro 대문자로 쓰고, 열 위치는 행과 같은 계산을 써서 값 바로 위에 선다. 문구는 파일 선택기 전용
`filepicker.column.*` 키다. kit 의 열 이름(Name·Size·Modified)을 따르며, 탐색기 열 이름과
따로 둔다. 선택한 행은 `surface-active` 로 채우고 왼쪽 한 변에만
`selection-edge-width` 폭의 `accent-primary` 막대를 둔다. 헤더 제목은 모달 제목 크기(`font-size-max`, 14)다.
`layout_tests.rs`의 `the_list_head_names_the_columns_above_the_rows`,
`the_list_head_has_one_separator_along_its_bottom`, `the_selected_row_has_one_edge_selection_bar`,
`the_header_title_uses_the_modal_title_size`가 실제로 칠한 모양으로 이를 검사한다. 목록은 머리를 뺀
나머지 높이 안에서 스크롤한다. `the_last_row_is_fully_visible_after_scrolling_to_the_end`는 행이 많은
목록을 끝까지 스크롤했을 때 마지막 행이 푸터와 겹치지 않고 온전히 보이는지 검사한다.

설정 창의 파일 선택기도 같은 뷰를 같은 셸 구조로 그린다. 셸 타이틀바가 없고(`headless`), 뷰가
보고한 헤더 줄이 이동 손잡이이며, 창 배경은 `bg-panel` 이다.

### 긴 경로 — 넘침은 path bar 가 흡수한다

경로가 아무리 깊거나 성분 이름이 길어도 footer 의 취소·확정 버튼은 popup 안에 온전히 남는다.
`draw_file_picker_view` 는 헤더와 path bar 를 위에서, **footer 를 아래에서 먼저** 자리 잡고 남은
높이를 본문(목록·상태 화면)에 준다. 그래서 footer 가 커지는 경우(저장 모드의 덮어쓰기 경고 줄)에도
줄어드는 쪽은 본문이다.

- **path bar**: 상위 폴더·새로고침 버튼이 오른쪽 끝을 먼저 차지하고(경로·버튼 사이는 `fp-section-gap`), breadcrumb 은 남은 폭 안에서만
  그려지고 그 밖은 잘린다. 가로 스크롤은 없다.
- **가운데 생략**: 전체 breadcrumb 이 그 폭에 안 들어가면 `crumb_alloc::plan`(`crates/tasty-ui-widgets/src/crumb_alloc.rs`)
  순서로 줄인다 — 조상 한 칸씩 `…` 로 → 부모 축소 → 현재 폴더 축소 → `root › … › current` → `… › current`. 들어가면 접지 않는다. 성분 하나는 180px 에서 말줄임한다.
- **`…` 메뉴**: `…` 를 누르면 숨긴 조상들이 메뉴로 나열되고, 고르면 그 폴더로 이동한다. 행은 다른 메뉴처럼
  간격 없이 붙인다(행 높이 `menu-item-height` 28). hover 하면
  **누르면 무엇이 되는지**를 말한다 — `Show 3 hidden folders`(`filepicker.hidden_folders`),
  하나면 en 단수 변형(`filepicker.hidden_folders_one`). 상태 서술("N folders hidden")이 아닌 이유는
  그 툴팁이 클릭 대상 위에 뜨기 때문이다. 단수형은 영어만 갈린다 — ko·ja·zh-Hans 는 굴절이 없다.
- **생략은 렌더 규칙이다**: view 가 받는 `FilePickerProps.crumbs` 는 항상 root 부터 현재 폴더까지
  전체이고, 접는 판단은 그리는 순간에만 한다 — `…` 메뉴가 숨긴 조상을 열 수 있는 이유다.
- **footer 행**: 이름 행은 라벨(고정폭, 줄지 않음) + 이름 칸(남은 폭), 버튼 행은 오른쪽 끝에서 확정 ·
  취소 순으로 자리 잡는다. 줄어드는 것은 이름 칸뿐이다.
- **고정 수단**: `src/adapters/ui/popup/file_picker/layout_tests.rs` 가 popup 매니저와 같은
  조건(콘텐츠 사각형 고정 · 그 사각형으로 clip)으로 한 프레임을 헤드리스로 그리고, 칠해진 버튼 라벨이
  보이는 영역 안에 온전히 들어갔는지를 경로 깊이 · 성분 길이 · 모드(열기/저장/덮어쓰기) · popup 크기
  조합마다 확인한다. 깊은 경로에서 root · `…` · 마지막 두 성분이 보이고 숨긴 성분이 안 칠해지는 것도
  같은 방식으로 본다.

### 원격 경로 구분자(POSIX/Windows)

원격 host 의 OS 는 client 가 사전에 알 수 없다 — `path_ancestors`/`join_dir`/`crumb_label` 은
서버가 돌려준 경로 문자열 자체에 `\` 가 있는지로 Windows 스타일(`C:\Users\alice`, 드라이브 루트
보존)과 POSIX(`/`)를 구분한다(`is_windows_style_remote_path`).

<a id="plugin-트리거docs/dev-guide/headless-ipc-surface.md--즉시-ack--이벤트-push"></a>

### 플러그인의 피커 요청 — 즉시 접수하고 결과는 이벤트로 전달

CallerContext::Plugin 검사는 이미 열린 popup(-32000)이나 잘못된 인자(-32602)보다 먼저다.
외부 CLI·Agent는 -32016으로 거절한다. FsRead 요구는 그대로다.
이 메서드는 window router의 외부 arm을 사용하므로 method table의 plugin_only 표식을 붙이지 않는다.
그 표식은 별도 host-call interception 경로를 뜻한다. 에이전트는 파일 경로를 직접 지정한다.

markdown의 Browse와 주소창 폴더 버튼, 이미지 다른 이름으로 저장의 찾아보기, agent-session 의 폴더 선택처럼 plugin이 호스트의 파일 선택 창을 열어야 할 때 사용한다.
사용자 응답까지 `host.call`을 잡아 두면 plugin의 렌더·입력 루프가 멈추고 60초
`HostCallTimeout`이 생길 수 있으므로 요청을 즉시 접수하고 결과는 나중에 이벤트로 보낸다.

1. plugin 이 `file_picker.trigger { filters?: string[], user_navigation_url?: string }` 를 호출한다(`FsRead` 권한,
   `gui` feature 전용). CLI·agent 호출자는 popup 을 열지 않고 `-32016` 을 받는다 — 결과를 받을
   곳이 없고 popup 이 사용자 입력 포커스를 가져가기 때문이다([파일 핸들러 (File handler)](../file-handler/index.md)). host 는 popup 확정을 **기다리지 않고** `{ request_id }` 만 즉시
   회신한다(`src/adapters/ipc/handler/file_picker.rs::handle_trigger_with_proofs`, `handler.rs` 의 `file_picker.trigger` 분기가 부른다).
2. host 는 `(plugin_id, request_id)` 를 `FilePickerData.requester`(`FilePickerRequester`)
   에 기록하고 popup 을 연다. `request_id` 는 피커 요청 식별자와 같은 값이다. 이후 로컬/원격
   판별·엔트리 로드는 위 경로(Tools 메뉴 트리거와 동일)를 그대로 탄다.
3. 사용자가 확정/취소하면 drain(`dispatch_pending_file_picker_results`)이 결과를 꺼내
   `PluginManager::emit_host_event_to_plugin` 으로 `"file_picker.result"` 이벤트를 그
   plugin 에만 unicast 한다: `{ request_id, paths, cancelled }`(확정도 취소도 항상 이 세
   필드를 전부 채운다 — 취소는 `paths: []`/`cancelled: true`). 호스트는 그 경로로 파일을 열거나
   클립보드에 쓰지 않는다.
4. plugin 은 `on_event` 에서 이 key 를 받아 자기 `request_id` pending-map 으로 상관관계를
   맞춘다(신규 SDK 콜백 불필요 — 기존 `EventDispatchCtx` 재사용).
5. 확정 결과(`cancelled: false`)를 보낼 때 host 는 `(plugin_id, request_id, paths)` 를 사용자 출처
   증명으로 남긴다(`picker_proofs`, 최근 32건). 확정은 사용자가 피커에서 직접 한 입력이기 때문이다.
   그 plugin 이 이 선택으로 `markdown.navigate {…, picker_request_id}` 를 보내면 host 는 plugin·요청 번호·
   경로가 모두 맞을 때 증명을 한 번 소비하고 사용자 요청으로 판정한다. 외부 호출자나 다른 plugin,
   고르지 않은 경로, 이미 쓴 증명은 통하지 않는다. 피커는 여전히 경로만 돌려주며 이 증명은 출처 판정에만 쓴다.
   증명을 남기는 `file_picker` 와 소비하는 `markdown` handler 가 `gui` feature 전용이므로 증명 저장소도
   `gui` 빌드에만 있다. headless 빌드에는 피커와 이 판정이 없다.

**폴더 모드**: `mode: "folder"` 는 메인 popup 을 Settings chooser 와 같은 폴더 선택 화면으로 연다(제목 `picker.folder.title`).
파일 행은 보이되 고를 수 없고, 확정하면 고른 하위 폴더를, 고른 것이 없으면 지금 보고 있는 폴더를 `paths[0]` 으로
돌려준다. 폴더를 더블클릭하면 들어간다. 원격 위치도 받는다. `Loading` 중에는 확정하지 않는다.
agent-session 의 폴더 선택 단추가 이 모드를 쓴다.

**저장 모드**: `mode?: "open" | "save" | "folder"`(기본 `open`), `default_name?: string`(기본 빈 이름)을
받는다. `location: "local"`은 활성 workspace와 무관하게 로컬을 지정한다. 생략하면 기존 출발 surface
판정을 유지하되 저장 모드에서 원격 위치는 `-32602`로 거절한다. 이미지 호출은 `mode: "save"`,
`location: "local"`, `filters: ["png"]`, 현재 버퍼의 파일명(없으면 `image.png`), surface ID와
로컬 시작 폴더를 보낸다. 절대 경로 버퍼의 부모, 원본 이미지의 절대 부모, 플러그인 작업 폴더 순이다.

메인 popup과 Settings chooser는 `file_picker::save`의 이름·덮어쓰기 판정을 공유한다.
`Loaded`/`Empty`에서 한 경로 성분인 이름만 확정하며 나열된 폴더 이름은 거절한다. 파일 행 클릭과
더블클릭은 이름만 채우므로 덮어쓰기 경고를 건너뛰지 않는다. 확정 결과는 경로 하나의 배열이며
호스트는 파일을 만들거나 쓰지 않는다. `FsRead` 권한은 그대로다. 실제 PNG 저장과 확장자 보충은
이미지 플러그인의 [저장]이 담당한다. 피커가 이미 열렸으면 `-32000`으로 거절하고 이미지 쪽은
경고 로그를 남기며 입력칸을 보존한다. 취소·저장 카드 닫힘·surface 재생성·새 요청 뒤의 이전 결과는
버리고 일치하는 request ID와 카드 세대의 결과만 한 번 받는다. 이미지 카드는 host popup이 아니므로
`owner_popup_instance`를 보내지 않으며 피커는 창 범위로 열린다.

**`filters`**: 확장자 목록(점 없이, 예: `["md", "markdown"]`) — `draw_file_picker` 가 렌더
직전 `matches_filters` 로 파일 엔트리만 걸러낸다(디렉토리는 필터와 무관하게 항상 표시 —
내비게이션 대상이라 숨기면 하위로 못 들어간다).

**필터 칩**: `filters` 가 있으면 footer 이름 행의 오른쪽 끝에 그 목록을 읽기 전용 칩으로
보여 준다(`tasty_ui_widgets::filter_readout`). 글자는 `*.toml, *.json` 처럼 호출자 순서·소문자로
mono caption 이며, 칩은 내용 폭이다. `fp-filter-max-width` 를 넘으면 끝을 `…` 로 줄이고, hover 툴팁
(`filepicker.filter_showing`, "Showing {list}")이 전체 목록을 보여 준다. 메뉴·chevron·채움·hover·
포커스가 없다 — 바꿀 수 있는 것이 없기 때문이다. 필터가 없으면 칩이 없고 이름 칸이 그 폭을 갖는다.
메인 피커와 설정 창의 파일 선택, 열기·저장 모드 모두 같다. 저장 모드에서 이름에 확장자를 덧붙이지 않는다.

**시작 위치 — `start_dir?: string` · `origin_surface_id?: u32`**: 피커는 그것을 띄운 surface 의
폴더에서 출발한다(`popup::file_picker::locate`). 두 필드 모두
옵셔널이라 기존 호출자는 그대로 동작한다.

- **로컬/원격 판정은 출발 surface 의 workspace 로 한다** — `origin_surface_id` 가 있고 찾아지면
  그 surface 가 속한 workspace 가 mirror 인지 보고, 없으면 활성 workspace 로 폴백한다. plugin
  트리거는 활성 workspace 와 무관할 수 있기 때문이다(포커스 독립성).
- **시작 디렉토리**: `start_dir` 가 있으면 그것, 없으면 출발 surface 의 cwd
  (`EngineRef::surface_cwd` — mirror 면 서버가 push 한 원격 cwd). 로컬은 **절대경로인 디렉토리일
  때만** 채택하고 아니면 홈으로 폴백한다(없는 경로로 열면 빈 에러 화면이 뜬다). 원격은 로컬에서
  stat 할 수 없으므로 그대로 `list_dir_request` 에 싣고 서버의 에러 회신에 맡긴다. 둘 다 없으면
  종전대로 로컬 홈 / 원격 홈(빈 `dir`)이다.
- **`inherit_cwd` 설정과 무관하다** — 그 설정은 "새 surface 가 cwd 를 상속하는가" 이고 피커는 새
  surface 를 만들지 않는다([원격 attach (Remote attach)](../remote-attach/index.md)
  참조). Tools 메뉴·단축키로 연 피커도 focus surface 의 폴더에서 출발한다.
- plugin 은 popup context 의 `observed_cwd`(로컬) / `remote_cwd`(mirror) 와 `origin_surface_id` 를
  그대로 실어 보내면 된다(키 의미는 `MainViewState::popup_surface_context`). 게이트가 걸린 `cwd` 키를
  쓰면 설정을 끈 사용자에게서 시작 위치가 사라진다.
- `path_input` 등 plugin 팝업에 사용자가 이미 적어 둔 경로를 시작점으로 삼는 것은 다루지 않는다.

**부모 범위와 숨김**: `owner_popup_instance`가 요청자 plugin의 부모 popup을 가리키면 host가
선언 종류와 target으로 유효 scope를 해석해 첫 paint 전에 적용한다([내부 팝업 시스템](../../design/systems/popup.md)).
Surface 부모가 보이지 않으면 자식도 paint/hit/Esc/키 게이트에서 빠진다. 숨김은 close가 아니며
부모·자식 draft, 선택, 요청 ID를 보존한다. 돌아오면 같은 작업을 계속한다. Window 부모와
owner 없는 단독 피커는 창 범위다. surface가 작으면 기존 clamp가 피커 크기를 제한한다.
부모 close는 기존 cancel/settled 규약을 따른다. 확정 이벤트를 받은 markdown은 경로 입력만
채우며, 부모 Open에서 최초 context의 대상을 실행한다. 호스트는 확정한 경로로 파일을 따로 열지 않는다.

**동시 요청**: 메인 창의 파일 피커는 한 번에 요청 하나만 받는다. Tools 메뉴·단축키·새 HTML·plugin
모두 `file_picker::open`의 공통 진입 검사를 거친다. 기존 피커가 열려 있거나 닫힌 결과를 아직
`take_outcome`으로 꺼내지 않았다면 새 요청을 거절한다. 기존 요청의 ID·caller 정보·requester·부모 범위·
선택·확정 또는 취소 결과는 그대로 둔다. 본체 caller는 열기가 성공한 뒤에만 자기 요청 정보를 저장한다.

사용자의 메뉴·단축키 재진입에는 기존 선택을 마치거나 취소하라는 안내 토스트를 보여 준다. plugin의
중복 요청은 종전처럼 `-32000` busy 응답으로 돌려주고 사용자 토스트나 새 popup intent를 만들지 않는다.
기존 요청의 결과는 원래 호출자가 한 번만 받는다. 그 결과를 꺼낸 뒤 새로 연 요청은 다른 ID를 쓰므로
이전 결과가 늦게 와도 새 요청에 적용하지 않는다. 숨은 plugin 자식 피커도 같은 규칙으로 보호한다.

**요청 주체 기록**: 피커를 여는 `OpenPopup`은 plugin 요청이면 `Intent::from_agent_plugin(plugin_id)`로,
탭 스트립 메뉴 "새 HTML..."이면 `from_user_context_menu`로, Tools 메뉴·단축키는
`from_user_menu("tools_menu")`로 기록한다. 호출자가 넘긴 `FilePickerOpenedBy` 로 정한다(`popup::file_picker::open`). IPC 요청을 사용자 메뉴 조작으로 기록하지 않는다.

### 설정 창에서의 로컬 전용 재사용

설정 창은 메인 윈도우와 별개의 winit 창이라, 메인 창 popup 스택(`MainViewState.dialogs.file_picker`)
에 상태를 둔 이 피커를 그대로 열 수 없다. 대신 **순수 view(`draw_file_picker_view`)만 재사용**한다 —
view 는 `FilePickerProps` 만 받고 `FilePickerAction` 만 돌려주므로 상태를 어디에 두든 그릴 수 있다.

- **상태**: 설정 창의 `SettingsUiState.file_chooser`(`LocalFileChooser`) 가 갖는다. 메인 창의
  `FilePickerData` 와 별개이며 메인 창 상태 모듈(`src/state/dialogs.rs`)을 거치지 않는다. 한 번에 하나만 열리며, 열린 요청이나 미소비 결과가 있으면
  새 요청을 거절해 기존 결과를 보존한다.
- **popup**: 설정 창 자체 `PopupManager` 에 `local_file_chooser` PopupDef로 등록된다(단축키 충돌 확인
  popup 과 같은 매니저). Open/Save는 메인 피커와 같은640×480, Folder는480×360이다.
- **로컬 전용**: 원격(mirror) 조회 경로(`pending_list_dir_forward` → attach client)는 메인 창 App
  루프가 소유하므로 설정 창에는 없다. 설정은 이 인스턴스 자신의 구성이라 로컬 파일시스템만 본다.
  목록은 위 "로컬 브라우징" 과 같은 `local_reads::directory` worker 요청으로 채운다. `reload` 가
  `FpViewState::Loading` 으로 전이하고, `accept_read` 가 결과를 `sort_entries` 로 정렬해
  `Loaded`/`Empty`/`ErrorPerm` 으로 바꾼다.
- **블로킹의 성질**: 읽기는 worker 에서 하므로 느리거나 응답하지 않는 파일시스템이어도 설정 창 프레임은
  기다리지 않고 `Loading` 을 표시한다 — 메인 피커의 로컬 경로와 같다. OS 네이티브 다이얼로그는 쓰지 않는다(포털 없는 Linux 에서
  끝나지 않는다, docs/features/file-handler/index.md).
- **모드**: `Open`(기존 파일 하나) · `Save { default_name }`(디렉토리를 고르고 파일명을 정한다).
  view 는 `FilePickerMode` 로 두 모드를 함께 그린다. 저장 모드는 파일명을 정할 뿐 파일을 만들지 않는다.
- **저장 모드의 확정 수단은 하나다**: footer 의 이름 칸이 편집 가능해지고 footer primary 버튼만이
  확정한다 — view 아래에 덧붙는 행은 없다. 버튼은 **이름 칸만** 읽는다.
  - 목록에서 **파일** 행을 고르면 확정이 아니라 그 이름이 이름 칸에 들어가고 그 행이 선택된다.
    나열된 이름이므로 곧 덮어쓰기 상태가 된다. **폴더** 행 한 번 클릭은 그 행을 고르되 이름 칸은
    건드리지 않는다 — 폴더는 저장 대상이 아니라 확정이 읽는 값이 될 수 없다(진입은 더블클릭).
  - 이름을 고쳐 고른 행과 달라지는 순간 선택이 풀린다(`FilePickerAction::EditName`). 같은 값이면
    선택이 유지된다. 그래서 "고른 파일" 과 "입력한 이름" 이 서로 다른 경로를 가리키는 상태가 없다.
  - **덮어쓰기**: 입력한 이름(앞뒤 공백 제외)이 지금 나열된 폴더에 **보이는 파일**로 있으면 버튼 위에
    경고 줄(`filepicker.save.overwrite_warning`, 이름만 mono · `accent-warning`)이 뜨고 버튼 라벨이
    **Overwrite** 로 바뀐다. 확인 대화상자는 없다. 판정은 나열된 목록 조회뿐이다 — 필터에 걸려 안 보이는
    파일이나 나열되지 않은 경로는 stat 하지 않는다.
  - **확정 조건**: 현재 디렉토리가 읽혔고, 이름이 한 경로 성분이며(`/`·`\`·`.`·`..`·빈 이름 거부),
    나열된 폴더 이름과 겹치지 않을 때만 버튼이 활성이다.
  - 저장 모드에서 파일 행 **더블클릭**은 확정하지 않고 고르기와 같다 — 덮어쓰기 경고를 보지 않고 기존
    파일로 확정하는 길을 두지 않는다.
  - 열기 모드의 이름 칸은 읽기 전용으로 현재 선택을 보여 주고, 비었으면 `filepicker.no_file_selected`
    를 placeholder 로 쓴다.
- **호출자**: Misc › Scripts(Lua 스크립트 파일 열기) · Keybindings › Import / Export(번들 내보내기는
  저장 모드, 가져오기는 열기 모드 — [단축키 가져오기 / 내보내기](../keybindings/index.md#가져오기--내보내기)).
- **제목**: 기본은 모드의 제목이고, 여는 쪽이 `set_title` 로 덮어쓸 수 있다(가져오기/내보내기가 자기 제목을 쓴다).
- **확장자 필터**: 메인 피커와 같은 `matches_filters` — 디렉토리는 거르지 않는다.
- **결과 전달**: 여는 쪽이 `consumer` 키(`&'static str`)를 주고, 닫힌 뒤 같은 키로
  `take_outcome` 해 메인 피커와 같은 경로 문자열 배열 `Confirmed(Vec<String>)` 또는 `Cancelled` 를 1 회
  가져간다. 단일 선택이라 확정 배열은 경로 하나다. 헤더 ✕ · Cancel · Esc 로
  닫히면 취소로 남는다.
- **Esc**: 설정 창 popup 중 열려 있는 것의 z 순서가 가장 높은 하나만 받는다
  (`settings_escape_owner`). 파일 선택이 충돌 확인 popup 위에 떠 있으면 충돌 popup 의 키 처리
  (Enter/Y/Esc/N)는 돌지 않는다. Esc 는 설정 창 자체를 닫지 않는다.
- **호출처**: 설정 › 기타 › 스크립트의 Add card **Browse…**(`Open`, `lua` 필터) · Keybindings ›
  Import / Export 의 **Export…**(`Save`, `toml` 필터) · **Import…**(`Open`, `toml` 필터).
- **배치**: 갤러리 specimen `Overlays › File picker › Save mode — one confirm, in the footer`
  (`crates/tasty-gallery/src/catalog/components/file_picker.rs` 의 `draw_save_mode`)가 정본 배치다.

## 인터페이스

- **사용자 트리거**: Tools 메뉴 "파일 열기…"(`filepicker.tools_menu_item`), 탭 스트립 빈 곳 메뉴
  "새 HTML..."(`pane_context_menu.new_html` — 확정하면 그 pane 에 html 탭을 만든다), 설정 › 기타 › 스크립트
  Add card 의 Browse…(설정 창 안의 로컬 전용 재사용). 목록 행 더블클릭
  (디렉토리는 진입, 파일은 즉시 확정 — 저장 모드에서는 이름 칸 채우기) / 브레드크럼 클릭 / `…`
  메뉴의 숨긴 조상 / 상위 폴더 버튼 / 새로고침 버튼 / ESC(취소) / X 버튼(취소). 저장 모드는 이름 칸
  편집이 더해진다.
- **AI Agent (IPC/CLI)**: 없음 — popup 조작(선택/확정/취소) 자체는 순수 로컬 사용자 입력
  UI 다(release 의 사용자 입력 재현 금지 원칙). 단, **popup 을 여는 트리거**는
  `file_picker.trigger` IPC 로 plugin 에 열려 있다(docs/design/systems/popup.md) — markdown 파일 열기
  팝업의 Browse 와 주소창 폴더 버튼이 실사용처. 이건 "에이전트가 사용자 대신 파일을 고른다"가 아니라 "plugin 이 host 소유
  UI 를 사용자에게 대신 띄워준다" 는 위임이라 원칙과 상충하지 않는다(뒤이은 선택/확정은
  여전히 사용자 몫).
- **원격 / 점유**: 원격 디렉토리 요청은 그 attach 세션이 대상 workspace 를 이미 점유하고 있어야
  서버가 응답한다(mirror 워크스페이스 존재의 전제조건과 동일).

## 비-목표 (Out of scope)

- **원격 파일 내용 fetch** — 피커는 디렉토리 나열만 한다. 확정 결과는 경로 문자열이며,
  `file_picker.trigger` 로 열린 경우 plugin 에 `paths` 로만 전달된다. 그 경로의 내용을 이 세션으로
  가져오는 fetch 는 피커에 없다. Tools 메뉴의 원격 확정은 원격 파일 열기 경로로 넘긴다.
- **멀티 셀렉트 / 열기 모드의 파일명 직접 입력** — 현재는 단일 선택만 지원(`FilePickerData::selected`
  는 매번 교체). 열기 모드 이름 칸은 읽기 전용이며 메인 popup과 설정 창 모두 저장 모드에서만 편집한다.
- **필터 선택** — 사용자가 필터를 바꾸는 선택기는 없다. 필터는 호출처가 정하는 `filters` 뿐이고
  footer 칩은 그것을 보여 주기만 한다.
- **`StreamControl` enum 확장** — capture 패턴과 동일하게 그 enum 을 건드리지 않고 별도 `event`
  태그를 같은 채널에 얹었다.
- **대형 원격 디렉토리의 페이지네이션** — 700KiB 예산을 넘는 나머지는 truncation 으로만
  처리한다(toast 통지). "다음 페이지 로드" 는 이번 스코프 밖.
- **다중 plugin 트리거 큐잉/대체** — 동시성 정책은 "거부" 뿐이다. 대체/큐잉을 원하는
  케이스가 실사용에서 반복되면 동시 피커 요청 처리 방식을 다시 검토한다.

## Acceptance Criteria

- Given 로컬(비-mirror) workspace 가 활성이고 focus surface 의 cwd 를 알 수 없음 When Tools 메뉴에서
  파일 피커를 열면 Then `Loading` 을 거쳐 로컬 홈 디렉토리 엔트리가 표시된다.
- Given focus 된 로컬 터미널의 cwd 가 `/tmp` When Tools 메뉴에서 파일 피커를 열면 Then `/tmp` 의
  엔트리가 표시된다(`inherit_cwd` 가 꺼져 있어도 같다).
- Given mirror workspace 가 활성이고 원격 cwd 를 모름 When Tools 메뉴에서 파일 피커를 열면 Then
  `Loading` 상태를 거쳐 attach 채널로 받은 원격 홈 디렉토리 엔트리가 표시되고 헤더에 host 배지가
  뜬다. 배지 글자는 [원격 위치 표시](../remote-attach/index.md#원격-위치-표시)와 같은 attach 프로필 이름이다.
- Given mirror surface 에 서버가 원격 cwd 를 push 함 When 그 surface 에서 파일 피커를 열면 Then 원격
  홈이 아니라 그 원격 cwd 의 엔트리를 요청한다.
- Given `file_picker.trigger` 에 존재하지 않는 로컬 `start_dir` 를 실음 Then 에러 화면이 아니라 홈에서
  출발한다.
- Given 활성 workspace 는 로컬이고 `origin_surface_id` 가 mirror workspace 의 surface 임 When
  `file_picker.trigger` 가 옴 Then 피커는 원격으로 열린다.
- Given 원격 요청 전송 후 8 초 안에 응답이 없음 Then `ErrorConn` 상태로 전이한다.
- Given mirror workspace 가 도중에 사라짐(disconnect) Then popup 이 즉시 `ErrorConn` 으로
  전이한다(soft timeout 만료를 기다리지 않음).
- Given 원격 디렉토리 읽기가 권한 거부로 실패 Then `ErrorPerm` 상태로 전이한다.
- Given `ErrorPerm` 또는 `ErrorConn` 상태 When 본문이 그려짐 Then 오류 글리프·제목·사유가 목록 자리
  가운데에 놓이고, refresh 아이콘을 단 Retry(연결 끊김은 Reconnect) Secondary 버튼이 그 아래에
  매달린다. 창 배경은 bg-panel 이라 버튼 채움·테두리가 창과 구분된다.
- Given attach 점유가 없는 client 가 `list_dir_request` 를 보냄 Then 서버가 거부 회신한다
  (`ok: false`).
- Given 어느 호출자(Tools 메뉴·"새 HTML..."·plugin)로 연 피커든 When 파일을 확정 Then 피커와 결과
  drain 은 intent·클립보드·toast 를 만들지 않고, 호출자가 가져갈 결과는 경로 배열과 요청 식별자뿐이다.
- Given Tools 메뉴로 연 로컬 피커에서 파일을 확정 Then `DomainIntent::DispatchFile` 이 발생해 기존
  오픈 경로로 이어진다.
- Given Tools 메뉴로 연 원격 피커에서 파일을 확정 Then 출발 mirror surface 를 `origin_surface_id` 로 실은
  `DomainIntent::DispatchFile` 이 하나 발생해 그 파일이 원격 workspace 에 열리고, 클립보드는 바뀌지 않는다.
- Given 출발 surface 없이 Tools 메뉴 피커가 mirror workspace 를 둘러봄 Then 위치를 정할 때 그 workspace 의
  surface 를 함께 정하고, 확정하면 그 surface 를 출발점으로 연다.
- Given Tools 메뉴 원격 피커가 열린 동안 보관한 surface 가 닫힘 When 확정 Then 같은 mirror workspace 의
  다른 surface 로 열고, workspace 까지 사라졌으면 아무것도 열지 않는다.
- Given 탭 스트립 "새 HTML..." 피커가 열린 동안 메뉴를 연 pane 이 닫힘 When 확정 Then 탭이 생기지 않는다.
- Given 기존 피커가 열려 있거나 확정·취소 결과가 아직 전달되지 않음 When 어느 호출자가 다시 열기를
  요청해도 Then 기존 요청과 부모 관계·결과를 보존하고 새 요청을 거절한다. 사용자 조작은 안내 토스트,
  plugin은 `-32000` 응답만 받으며 새 popup이나 후속 동작은 생기지 않는다.
- Given 기존 요청의 결과를 꺼낸 뒤 같은 호출자가 피커를 다시 열었음 When 이전 요청 식별자의 결과가 도착
  Then 그 결과는 적용되지 않고, 현재 결과는 한 번만 적용된다.
- Given markdown 파일 열기 팝업의 찾아보기로 연 피커에서 확정 Then 호스트는 탭을 만들거나 클립보드를
  바꾸지 않고, markdown 이 받은 경로를 입력칸에 채운다.
- Given X 버튼/ESC 로 popup 이 닫힘 Then 결과가 `Cancelled` 로 명시되어 dialog
  상태가 정리된다(다음 오픈에 이전 상태가 새지 않음).
- Given 열기 모드에서 디렉토리 엔트리가 선택됨(더블클릭 아님) Then [열기] 가 활성이고 누르면
  그 디렉토리로 들어간다 — 결과는 나지 않는다(디렉토리는 파일로 확정되지 않는다). 이름 칸은
  비어 placeholder 를 보인다.
- Given 저장 모드에서 디렉토리 엔트리가 선택됨 Then 이름 칸과 확정 버튼은 그대로이고 footer 에
  `filepicker.folder_not_save_target` 안내 줄이 표시된다.
- Given entries 직렬화가 바이트 예산(700KiB)을 넘는 대형 원격 디렉토리 Then 서버가
  entries 를 잘라 `truncated: true` 로 회신하고, client 는 경고 toast 를 띄운다(attach 세션
  자체는 끊기지 않는다).
- Given 원격 host 의 경로가 Windows 스타일(`C:\Users\alice`) Then 브레드크럼/내비게이션이
  `\` 구분자와 드라이브 루트를 올바르게 다룬다(POSIX 원격도 계속 정상 동작).
- Given plugin 이 `file_picker.trigger` 를 호출 When popup 이 아직 열려있지 않음 Then
  즉시 `{ request_id }` 로 회신하고 popup 이 열린다(확정을 기다리지 않음).
- Given CLI(`Local`) 또는 agent 토큰 호출자 When `file_picker.trigger` 를 호출 Then `-32016` 으로
  거부되고 popup 이 열리지 않으며 사용자 입력 포커스·단축키 게이트가 그대로다.
- Given `file_picker` popup 이 이미 열려 있음 When 두 번째 `file_picker.trigger` 가 옴
  Then 거부(`-32000` 에러) — 첫 요청의 `requester` 는 대체되지 않고 그대로 유지된다.
- Given passive 로 열린(또는 passive 로 열릴 Intent 가 대기 중인) 피커 When 같은 plugin·같은 `origin_surface_id` 의 유효한 `user_navigation_url` 로
  `file_picker.trigger` 가 옴 Then 증명을 소비하고 피커가 포커스를 받아 맨 앞에 오며 응답은 첫 요청의 `request_id` 다. 피커는 둘이 되지 않고
  결과는 첫 요청자에게 한 번만 간다.
- Given passive 피커 When 증명 없는 요청, 다른 plugin 의 요청, 다른 surface 의 요청 Then `-32000` 이고 passive 를 유지하며 증명을 소비하지 않는다.
  이미 포커스된 피커, 범위 밖에 숨은 피커, 닫혀 결과 전달을 기다리는 피커도 같다.
- Given Markdown 문서의 Folder 에 키보드 포커스 When Enter 또는 Space(Windows·Linux) Then 피커가 사용자 요청으로 열려 페이지 위에 보이고 Esc 로 닫힌다.
  Enter 연타·누르고 있기도 피커가 포커스를 받는다.
- Given Markdown 문서의 Folder When 마우스로 빠르게 두 번 클릭 Then 피커가 보이고 쓸 수 있다. Windows 80ms 측정에서는 둘째 클릭이 상자 밖 Folder 자리에 떨어져 일반 바깥 클릭 규칙대로 포커스가 풀렸다. 이 상태에서는 Esc 로 닫히지 않고, 상자를 누르면 포커스를 받아 Esc 로 닫힌다. Linux 80ms 측정에서는 포커스를 유지했다.
- Given plugin 이 `mode: "folder"` 로 트리거함 When 하위 폴더를 고르지 않고 확정함 Then `paths` 는 지금 보고 있는
  폴더 하나다. 하위 폴더를 고르고 확정하면 들어가지 않고 그 폴더를 돌려주며, 파일 행으로는 확정되지 않는다.
- Given `filters: ["md"]` 로 트리거됨 Then 렌더된 엔트리 목록에서 `.md` 가 아닌 파일은
  제외되고(디렉토리는 필터와 무관하게 항상 표시), 확정/취소 시 `"file_picker.result"` 가
  그 요청을 낸 plugin 에만(unicast) push 된다.
- Given plugin 이 자기 페이지에서 사용자가 누른 navigation URL 을 `user_navigation_url` 과 `origin_surface_id` 로 실음
  When host 가 그 surface 의 사용자 navigation 기록으로 확인함 Then 피커는 사용자 요청으로 열려 키보드 포커스를 받고
  WebView 는 일반 대상 팝업의 오버레이 모드를 따른다(투명이면 피커를 페이지 위에 합성하고, 불투명이면 피커 상자만 가리고, 숨김이면 페이지를 숨긴다). 기록이 없거나 한 번 쓴 기록이면 포커스 없는 passive 피커가 된다(`file_handler.dispatch` 의 같은 필드와 같은 규칙).
- Given plugin 이 연 피커에서 사용자가 경로를 확정함 When 그 plugin 이 같은 경로와 `picker_request_id` 로
  `markdown.navigate` 를 보냄 Then host 는 사용자 요청으로 판정한다(원격 실패는 toast). 같은 증명을 다시
  쓰거나 외부 호출자가 같은 번호를 실으면 에이전트 요청으로 남는다. 취소는 증명을 남기지 않는다.
- Given Tools 메뉴로 연 기존 흐름(`requester: None`) Then `file_picker.trigger` 도입 후에도
  동일하게 동작하고 결과 이벤트가 발생하지 않는다(회귀 없음).
- Given 본체 file picker 를 연다 When 첫 프레임이 그려진다 Then 제목과 닫기 ✕ 는 헤더에 한 번씩만
  보이고, 셸 타이틀바는 없다.
- Given 본체 file picker 가 열려 있다 When 헤더의 빈 곳을 끌면 Then popup 이 이동하고, 헤더 ✕ 를
  누르면 이동하지 않고 닫히며 결과는 `Cancelled` 다.
- Given 본체 file picker 를 연다 When path bar 가 그려진다 Then 그 줄 배경은 창 좌우 끝까지 `bg-sidebar` 이고
  헤더·목록(`bg-panel`)과 구분된다.
- Given 설정 › 기타 › 스크립트 Add card When Browse… 를 누르면 Then 설정 창 안에 로컬 홈
  디렉토리를 보여주는 파일 선택 popup 이 열리고, 떠 있는 동안 IPC 왕복(`tasty list info`)이
  응답한다.
- Given 설정 창 파일 선택이 `lua` 필터로 열림 Then `.lua` 가 아닌 파일은 목록에 없고 디렉토리는
  보이며, 디렉토리나 필터 밖 파일은 확정되지 않는다.
- Given 피커가 `filters: ["toml", "json"]` 으로 열림 Then footer 이름 행 오른쪽 끝에 `*.toml, *.json`
  칩이 있고, 칩에 마우스를 올리면 "Showing *.toml, *.json" 툴팁이 뜬다. 칩을 눌러도 아무것도 열리지 않는다.
- Given 피커가 필터 없이 열림 Then footer 에 필터 칩이 없고 이름 칸이 행 끝까지 간다.
- Given 확장자가 많아 목록이 `fp-filter-max-width` 보다 김 Then 칩은 그 폭에서 끝이 `…` 로 줄고 툴팁은 전체 목록이다.
- Given 설정 창 파일 선택에서 파일을 확정 Then 그 절대 경로가 연 쪽(`consumer`)으로 돌아간다
  (스크립트 Add card 는 파일 경로와, 비어 있으면 표시 이름을 채운다).
- Given 설정 창 파일 선택이 Esc 또는 헤더 ✕ 로 닫힘 Then 결과는 `Cancelled` 이고 연 쪽의
  입력은 바뀌지 않는다.
- Given 설정 창 파일 선택기를 연다 When 첫 프레임이 그려진다 Then 제목과 닫기 ✕ 는 헤더에 한 번씩만
  보이고, 셸 타이틀바는 없다. 헤더의 빈 곳을 끌면 popup 이 이동한다.
- Given 저장 모드 When 파일명을 입력하고 확정하면 Then 현재 디렉토리 + 파일명 경로가 돌아가고,
  파일명이 비었거나 경로 구분자·`.`·`..` 이거나 나열된 폴더 이름이면 확정 버튼이 비활성이다.
- Given 저장 모드가 열려 있다 When 화면을 본다 Then 확정 수단은 footer primary 버튼 하나뿐이다.
- Given 저장 모드 When 목록에서 기존 파일을 고른다 Then 확정되지 않고 그 이름이 이름 칸에 들어가며,
  경고 줄이 뜨고 버튼 라벨이 Overwrite 가 된다.
- Given 기존 파일을 고른 뒤 When 이름을 고친다 Then 선택이 풀리고 라벨이 Save 로 돌아오며, 확정하면
  고친 이름의 경로가 돌아간다.
- Given 어떤 깊이 · 어떤 성분 길이의 경로든 When 메인 피커나 설정 창 파일 선택이 열린다 Then footer 의
  취소·확정 버튼이 온전히 보인다.
- Given breadcrumb 이 path bar 폭을 넘는 깊은 경로 When 그린다 Then 조상이 `…` 로 접히고(`crumb_alloc`의 축약 순서),
  `…` 를 누르면 숨긴 조상이 나열되며 고르면 그 폴더로 이동한다.

### 검증 범위

`tests/attach_list_dir_loopback.rs`는 실제 서버에 연결해 attach 점유 후 디렉토리 조회를
확인한다. 성공, 없는 디렉토리, 점유 없는 호출의 거절을 다룬다. 이 시험은 popup의 실제
픽셀 렌더링이나 원격 GUI 두 인스턴스의 상호작용까지 확인하지는 않는다.

`file_picker.trigger`의 Local 호출 거절은 격리한 debug GUI 인스턴스에서 확인한 이력이
있다. 거절 전후 popup 포커스와 단축키 차단 상태가 바뀌지 않았다. plugin 성공·requester
기록·동시 요청 거절·agent 토큰 거절은 핸들러 단위 시험이 확인한다. plugin 프로세스의
요청부터 결과 이벤트 수신까지 이어지는 실제 GUI 동작은 이 검증 이력에 포함되지 않는다.

## 구현

- Popup: `src/adapters/ui/popup/file_picker.rs`(`FilePickerProps`/`FilePickerMode`/`FilePickerAction`/
  `draw_file_picker_view`/`draw_file_picker`/`on_close_file_picker` — X 버튼 등
  draw_fn 을 거치지 않는 닫힘도 `PopupDef.on_close` 훅으로 `Cancelled` 명시),
  같은 디렉토리의 `file_picker/path_bar.rs`(breadcrumb · 가운데 생략 · `…` 메뉴) ·
  `file_picker/footer.rs`(이름 행 · 덮어쓰기 경고 · 버튼 행) · `file_picker/layout_tests.rs`,
  `src/adapters/ui/popup/defs.rs`(`PopupDef` 등록), `src/adapters/ui/popup.rs`(모듈 선언).
- 공유 나열: `src/core/fs_list.rs`(`DirEntryInfo`/`read_dir_entries`/`sort_entries`/`human_size`/
  `format_modified`) — `src/adapters/ui/surface/explorer/view.rs`(Explorer surface)와 공유.
- 로컬 읽기: `src/app/local_reads.rs`(`directory` 요청 생성 · worker 실행 · `poll_file_picker_reads` 결과 반영).
  설정 창은 `src/view/file_chooser.rs`(`reload`/`accept_read`)가 같은 요청을 쓴다.
- 피커 열기: `src/adapters/ui/popup/file_picker.rs`(`open`, `FilePickerOpenedBy`), 위치 결정은
  `file_picker/location.rs`(`locate`). `ensure_available`이 열린 요청과 미전달 결과를 보호하며 `open`도 같은 검사를 수행한다.
- 호출자: `src/adapters/ui/file_open_tool.rs`(Tools 메뉴 `BuiltinAction::FilePicker` 와 단축키
  `open_file_picker_tool` 이 함께 쓰는 `open`/`apply_outcome`), `src/adapters/ui/new_tab_picker.rs`
  ("새 HTML..." 의 `open`/`apply_outcome`), `src/adapters/ipc/handler/file_picker.rs`(`file_picker.trigger` —
  plugin 트리거, `deliver_result` 로 `"file_picker.result"` unicast, docs/design/systems/popup.md),
  `crates/tasty-ipc/src/method_meta.rs`(`file_picker.trigger` → `FsRead`).
- Result drain: `src/app/dispatch/file_picker.rs`(`dispatch_pending_file_picker_results`,
  `take_outcome`, `deliver_outcome`), `src/app/event_handler.rs`(`about_to_wait` 호출).
- 요청 상태: `src/state/dialogs.rs`(`FilePickerRequestId`, `FilePickerLocation`, `FilePickerOutcome`,
  호출자별 보관 칸 `FilePickerCallers`/`OpenFilePickerRequest`/`NewTabPickerRequest`, `FilePickerRequester`,
  `FilePickerData.requester`/`filters`), `src/core/mod.rs`(`next_file_picker_request_id` — `FpLoadState::Loading`
  의 내부 `request_id` 와 별개 네임스페이스), `src/intent.rs`(`Intent`/`UiIntent::
  from_agent_plugin`).
- plugin caller: `crates/tasty-plugin-markdown/src/popup.rs`(`trigger_file_picker`,
  `FILE_PICKER_RESULT_EVENT`), `crates/tasty-plugin-markdown/src/browse.rs`(주소창 폴더 버튼 —
  `trigger_params`·`request_id_of`·`navigate_params`), `crates/tasty-plugin-markdown/src/main.rs`(`pending_file_picker`
  의 팝업·surface 출처 구분, `take_picker_result`, `on_event` 의 `"file_picker.result"` 수신).
- 사용자 출처 증명: `src/adapters/ipc/handler/picker_proofs.rs`(`record`·`take`), 기록은
  `deliver_result`, 소비는 `src/adapters/ipc/handler/markdown.rs`(`picked_by_user`).
- 원격 요청: `src/core/mod.rs`의 요청 값 `PendingListDirForward`/`next_list_dir_request_id`,
  `src/app/engine_action.rs`의 원 projection에 묶인 `ListDirectory` → `src/remote/state.rs`의 pending list 큐.
- 원격 전송(client): `src/app/attach_client/forward.rs`(조회 전달), `src/app/attach_client/wire/list_dir.rs`(`parse_list_dir_result`), `src/app/attach_client/output/apply_event.rs`(`MirrorEvent::ListDirResult` 적용, 항목 변환은 `output/remote_entry.rs`).
- 원격 수신(server): `crates/tasty-ipc/src/stream_hub.rs`(`ListDirRequestMsg`, `pump_inbound`
  분류), `src/remote/server/content_queries.rs`(`handle_list_dir_request`, `list_dir_for_request`,
  `list_dir_entries_wire_capped`, 항목 하나는 `content_queries/list_dir_wire.rs` 의 `list_dir_entry_wire`/`LIST_DIR_ENTRIES_BYTE_BUDGET`). GUI
  (`src/app/event_handler.rs::apply_list_dir_request_msg`)와 headless(`src/boot/headless_stream.rs`) 양쪽
  진입점에서 동일 서버 로직을 호출.
- Popup 상태: `src/state/dialogs.rs`(`FilePickerData`, `FpLoadState`, `FilePickerResult` — 경로 배열 또는 취소).
- 설정 창 재사용: `src/view/file_chooser.rs`(`LocalFileChooser`/`FileChooserMode`/
  `FileChooserOutcome`, 저장 모드 이름 칸·덮어쓰기 판정), `src/view/settings/ui.rs`(`PopupManager` 등록 ·
  `open_file_chooser` · `settings_escape_owner` · `apply_file_chooser_outcomes`),
  `src/view/settings/ui/tabs/misc.rs`(`ScriptsUiState::BROWSE_CONSUMER`/`apply_browsed_file`).
- i18n: `lang/{en,ko,ja,zh-Hans}.toml` `[filepicker]`/`[filepicker.error_perm]`/`[filepicker.error_conn]`/`[filepicker.save]`/`[picker.folder]`.
- 갤러리 specimen: `crates/tasty-gallery/src/catalog/components/file_picker.rs`(`draw` · `draw_states` · `draw_save_mode` · `draw_folder_mode`) + `file_picker/{path_bar,footer,folder}.rs`.
- 테스트: `crates/tasty-ipc/src/stream_hub.rs`(`pump_inbound_classifies_list_dir_request`),
  `src/core/fs_list.rs`(`human_size_units`/`sort_dirs_first`/`read_dir_entries_lists_files_and_dirs`),
  `src/remote/server/content_queries.rs`(`list_dir_entries_wire_capped_tests` — byte-budget truncation),
  `src/adapters/ui/popup/file_picker.rs`(`path_helper_tests` — POSIX/Windows 원격 경로 처리 +
  `matches_filters_*` 확장자 필터), `src/adapters/ui/popup/file_picker/admission_tests.rs`(세 호출자 사이 재진입 · 미전달 확정/취소 · requester/범위 · 원래 결과의 한 번 소비), `src/adapters/ui/popup/file_picker/pure_return_tests.rs`(호출자별 확정이
  경로 배열과 식별자만 남김 · 취소 · popup 출처 · 위치 결정), `src/adapters/ui/file_open_tool_tests.rs`(로컬·원격
  열기 · surface 대체 · workspace 소멸 · 취소 · 늦은 결과), `src/adapters/ui/new_tab_picker_tests.rs`(pane 고정 ·
  pane 닫힘 · 경로별 탭 · 늦은 결과 · mirror pane 거절), `src/app/dispatch/file_picker_tests.rs`(plugin 결과
  payload · 한 번만 꺼냄), `src/adapters/ipc/handler/file_picker.rs`(`tests` —
  trigger 성공/requester 기록/busy 거부/filters 전달, 실제 `MainViewState`/`CoreState` fixture),
  `crates/tasty-ipc/src/method_meta_tests.rs`(`file_picker_trigger_requires_fs_read`),
  `tests/attach_list_dir_loopback.rs`(실제 서버 인스턴스 상대 loopback 왕복 3 종 — 성공/디렉토리
  없음 에러/attach 점유 없는 client 거부), `src/view/file_chooser.rs`(`tests` — 설정 창
  재사용의 확정·필터·이동·읽기 실패·저장 이름 검증·외부 닫힘 · 저장 모드의 단일 확정 대상·선택 해제·
  덮어쓰기 판정·더블클릭), `src/adapters/ui/popup/file_picker/layout_tests.rs`(헤드리스 렌더로 footer
  버튼 무잘림 · 깊은 경로 가운데 생략 · 짧은 경로 비생략 · 모든 조상이 칠해지거나 `…` 뒤에 있음).

로컬 목록은 App read worker에 제출하고 현재 파일 선택 세션의 receipt로 결과를 받는다. 이동·닫기·재개방으로 receipt가 교체되면 이전 결과를 버린다. 로딩 중에도 메인 이벤트 루프는 파일시스템 읽기를 기다리지 않는다. 원격 연결의 8초 soft timeout은 로컬 읽기에 적용하지 않는다.

### 이미지 저장 경로 완료 조건

- Given 이미지 저장 카드가 열림 When 찾아보기를 누르면 Then 로컬 Tasty 저장 피커가 PNG 필터와 현재 파일명으로 열리고 렌더 루프는 사용자 응답을 기다리지 않는다.
- Given 저장 피커에서 경로를 확정 When 결과를 받으면 Then 그 카드의 경로 입력만 바뀌고 파일은 아직 쓰이지 않는다.
- Given 취소하거나 카드·surface가 닫힘 또는 새 요청을 보냄 When 이전 결과가 도착하면 Then 버퍼는 바뀌지 않는다.
- Given 기존 파일명을 선택 When 저장 피커를 보면 Then 덮어쓰기 경고가 보이며 더블클릭만으로 확정되지 않는다.

### 독립 View의 폴더 선택

`src/view/file_chooser.rs`의 LocalFileChooser는 Settings, Plugins, Preset이 공유하는 로컬 상태이며
`file_chooser/window.rs`는 기존 file_picker PopupDef의 표시 계약을 재사용한 local_file_chooser 정의를 각 View의 PopupManager에 등록한다.
메인 엔진의 file_picker 상태와 섞이지 않는다. 원격 전송 Browse는 설정 draft의 dir, Plugins의
Find folder는 add_path_input을 채운 뒤 기존 매니페스트 검증, Preset의 dir/file_path는 각각 Folder/Open
모드로 해당 필드 draft만 바꾼다. 실제 설정 저장·설치·프리셋 저장은 각 화면의 기존 확정 동작이다.

Folder는 선택된 폴더가 있으면 그 경로, 없으면 현재 폴더를 한 원소 배열로 반환한다. 파일은 모두
나열하되 비활성 잉크로 표시하며 클릭·키 이동에서 제외한다. Enter는 확정, 더블클릭/오른쪽 화살표는
진입, 폴더 재클릭/빈 곳 클릭은 선택 해제다. 로딩·오류에는 확정할 수 없고 빈 폴더는 확정할 수 있다.
필터 칩은 숨기며 footer는 readonly 경로·안내·버튼의 3행이다. 로컬 Folder popup은480×360이고
Open/Save는 기존640×480을 유지한다. 최소폭320과 3언어 안내 줄바꿈은 갤러리와 공용 footer로 검사한다.

시작 위치는 현재 값이 가리키는 로컬 디렉터리, 그렇지 않으면 존재하는 부모, 둘 다 없으면 홈이다. `~`/`~/`를 홈으로
풀며 목록은 비동기 read receipt로 받는다. 취소는 호출자의 값을 바꾸지 않고, 미소비 결과가 있으면
다음 요청으로 덮어쓰지 않는다. chooser가 열린 동안 Esc는 chooser만 닫으며 플러그인 제거 확인·미인증 미리보기 취소나
프리셋 surface 설정 취소로 전달하지 않는다. 프리셋 결과는 preset key·leaf·필드 ID/target/input·kind와
편집 세대가 모두 맞아야 적용한다. 이름 변경, 다른 leaf/편집 세션, kind 변경, 새 요청은 이전 응답을
무효화하며 소유 설정 화면이 사라지면 picker도 취소한다. 기존 스크립트 Browse·Import·Export와
이미지 Save 모드는 같은 파일/저장 판정과 반환 계약을 유지한다.

- Given Folder 모드에서 하위 폴더를 선택 When Enter Then 그 폴더 경로를 반환하고 진입하지 않는다.
- Given Folder 모드에서 하위 폴더를 선택 When 오른쪽 화살표나 더블클릭 Then 진입하며 선택을 비운다.
- Given 프리셋 필드 picker가 열림 When kind/이름/편집 대상이 바뀌고 이전 결과 도착 Then draft를 바꾸지 않는다.
- Given 프리셋 설정 또는 플러그인 제거 확인 뒤에 chooser가 열림 When Esc Then chooser만 취소한다.

폴더 모드 회귀는 `src/view/file_chooser/folder_tests.rs`(현재/선택 폴더, 키 이동, 취소·한 번 소비),
`src/view/settings/ui/file_chooser_popup_tests.rs`(크기·Esc·draft 보존),
`src/view/plugins/ui/add/folder_tests.rs`(기존 검증 연결), `src/view/plugins/ui/add.rs`(미인증 미리보기의 Esc 보존)와 `src/view/plugins/ui/tests.rs`(제거 확인 Esc 순서),
`src/adapters/ui/preset/surface_settings/browse_tests.rs`(필드/세대/rename/kind/leaf),
`src/adapters/ui/popup/file_picker/folder_tests.rs`(plugin `mode: "folder"` 의 현재/선택 폴더 확정, 파일 행·Loading 거절)를 검사한다.
공용 footer의 3언어·너비·비활성 잉크는 `crates/tasty-ui-widgets/src/folder_picker/tests.rs`,
갤러리 ID 정합은 `crates/tasty-gallery/tests/specimen_smoke.rs`에서 검사한다.
