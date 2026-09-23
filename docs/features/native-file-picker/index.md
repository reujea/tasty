# 네이티브 파일 피커 (로컬+원격 겸용)

- **Status**: Implemented
- **주체**: 로컬 사용자 (Tools 메뉴 트리거 · 설정 창 안의 파일 선택) + plugin(`file_picker.trigger` IPC — plugin 호출자 전용, CLI·agent 는 `-32016`)
- **ADR**: [ADR-0053](../../adr/0053-native-file-picker-remote-attach-channel.md) (attach 커스텀 이벤트 채널 + 하이브리드 신뢰 모델), [ADR-0058](../../adr/0058-plugin-triggered-host-popup-async-ack-push.md) (plugin 트리거 — 즉시 ack + 이벤트 push). 관련: [ADR-0162](../../adr/0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md)(옛 `fs.pick_file` 제거 — 이 피커가 그 자리를 대신한다)
- **코드**: `src/adapters/ui/popup/file_picker.rs`(popup wrapper/view/action), `src/core/fs_list.rs`(공유 디렉토리 나열), `src/adapters/ui/tools_menu.rs`(Tools 메뉴 트리거), `src/adapters/ipc/handler/file_picker.rs`(`file_picker.trigger` — plugin 트리거), `src/app/dispatch/file_picker.rs`(result drain + plugin 에게 `"file_picker.result"` push), `src/core/attach_runtime.rs`(서버측 `handle_list_dir_request`), `src/app/attach_client.rs`(client 원격 파싱 + `MirrorEvent::ListDirResult`), `crates/tasty-ipc/src/stream_hub.rs`(`ListDirRequestMsg` 분류), `crates/tasty-plugin-markdown/src/popup.rs`(Browse 버튼 caller), `src/view/settings/ui/file_chooser.rs`(설정 창 안의 로컬 전용 재사용)
- **화면**: 없음 (popup 은 갤러리 specimen `crates/tasty-gallery/src/catalog/components/file_picker.rs` 로 시각 확인)

## 목적

Tasty 자체 in-app "파일 열기" 다이얼로그. 로컬 파일시스템뿐 아니라 **attach mirror 워크스페이스가
브라우징하고 있는 원격 파일시스템**도 같은 UI 로 탐색할 수 있게 한다 — native OS 다이얼로그
(옛 `fs.pick_file`, ADR-0042 → ADR-0162 로 제거)는 host 프로세스 로컬 파일시스템만 알 뿐
원격이라는 개념이 없었다.

## 내부 동작 (headless-valid)

### 로컬/원격 판별

Tools 메뉴에서 파일 피커 항목을 클릭하면(`src/adapters/ui/tools_menu.rs::open` 경유,
`popup::file_picker::open`, 출발 surface = focus surface), 출발 surface 가 속한 workspace(없으면 활성
workspace)의 `Workspace.mirror` 플래그를 1회 확인해 로컬/원격을 고정한다. `FilePickerData.mirror_ws_id: Option<u32>` 가 `Some`
이면 원격, `None` 이면 로컬이다.

### 로컬 브라우징

`crate::core::fs_list::read_dir_entries` 를 popup wrapper 가 직접 동기 호출한다(host 프로세스
내부 함수 — 별도 IPC 왕복 없음). 결과를 `sort_entries` 로 정렬해 즉시 `Loaded`/`Empty` 로 전이한다.

### 원격 브라우징 (attach 커스텀 이벤트)

원격은 attach 세션이 이미 열어둔 `StreamTag::Control` 채널을 그대로 재사용한다(스크린샷 캡처
기능의 `capture_chunk`/`capture_commit`/`capture_result` 와 동일한 "`StreamControl` enum 이
인식 못 하는 `event` 태그" 패턴).

1. wrapper 가 `CoreState::pending_list_dir_forward` 에 `{ local_ws_id, request_id, dir }` 를
   push 하고 popup 상태를 `FpLoadState::Loading { request_id, sent_at }` 로 전이.
2. App 이 `about_to_wait` 에서 큐를 drain 해 attach 세션 writer 로 `list_dir_request` 프레임을
   전송(`src/app/attach_client.rs::send_list_dir_request`).
3. 원격 인스턴스 서버측(`src/core/attach_runtime.rs::handle_list_dir_request`)이
   **attach 점유 = 신뢰**(`engine.attach.client_holds_workspace(client_id)`)만으로 인가 판정 —
   별도 permission 게이트 없음. 인가되면 같은 `read_dir_entries` 로 대상 디렉토리를 읽어
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
`modified_unix: u64`(unix epoch 초)로 변환한다. 사람이 읽는 포맷(`"YYYY-MM-DD"`,
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

- **로컬**: 선택 경로로 `DomainIntent::DispatchFile { depth: Deep, .. }` 를 발화해 기존 파일 핸들러
  디스패치 경로(explorer/markdown 오픈과 동일)로 넘긴다.
- **원격**: 이번 구현은 **디렉토리 나열까지만** 스코프이며 원격 파일 **내용**을 이 세션으로 가져오는
  fetch 는 하지 않는다. 선택 경로를 클립보드에 복사하고 결과 toast 를 띄운다
  (`src/app/dispatch/file_picker.rs::apply_remote_confirm`).
- **디렉토리는 파일로 확정되지 않는다**: [열기] 가 폴더 하나를 골랐을 때 하는 일은 **그 폴더로
  들어가는 것**이다 — 더블클릭 말고 키보드로 내려가는 길이 그것이다. 그 밖에는 선택된 엔트리가
  전부 파일일 때만 활성화되고(`draw_file_picker_view` 의 `can_confirm`), wrapper 의
  `apply_action` 도 확정 직전에 같은 조건을 다시 확인한다(방어적 중복 검증).
- **행 제스처 표** — 저장/열기가 같은 view 라 모든 갈래가 두 모드에서 성립해야 한다.

  | | 단일 클릭 | 더블클릭 |
  |---|---|---|
  | 폴더 | 고른다(두 모드) | 들어간다(두 모드) |
  | 파일 | 고른다 — 저장 모드는 이름 칸도 채운다 | 열기는 확정 · 저장은 고르기까지 |

  폴더를 고르면 footer 에 톤 없는 안내 줄이 선다(`folder` 글리프 · caption · `text-muted`).
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

### 목록의 긴 파일명

이름 열은 크기·수정일 열을 제외한 남은 폭을 사용하고, 긴 이름은 끝을 `…`로 말줄임한다.
말줄임은 표시만 바꾼다 — 선택·더블클릭·확정은 전체 파일명을 사용한다. 열 배치는
[갤러리 매핑](../../design/systems/design-gallery-mapping.md)의 `FpRow`를 따른다.
`src/adapters/ui/popup/file_picker/layout_tests.rs`의 `filenames_stay_inside_the_name_column_at_default_width`와
`filenames_stay_inside_the_name_column_at_narrow_width`는
기본·좁은 popup에서 긴 이름과 짧은 이름의 실제 galley가 콘텐츠 안에 있고 크기·수정일과
겹치지 않는지 검사한다.

### 긴 경로 — 넘침은 path bar 가 흡수한다

경로가 아무리 깊거나 성분 이름이 길어도 footer 의 취소·확정 버튼은 popup 안에 온전히 남는다.
`draw_file_picker_view` 는 헤더와 path bar 를 위에서, **footer 를 아래에서 먼저** 자리 잡고 남은
높이를 본문(목록·상태 화면)에 준다. 그래서 footer 가 커지는 경우(저장 모드의 덮어쓰기 경고 줄)에도
줄어드는 쪽은 본문이다.

- **path bar**: 상위 폴더·새로고침 버튼이 오른쪽 끝을 먼저 차지하고, breadcrumb 은 남은 폭 안에서만
  그려지고 그 밖은 잘린다. 가로 스크롤은 없다.
- **가운데 생략**: 전체 breadcrumb 이 그 폭에 안 들어가면 `crumb_alloc::plan`(`crates/tasty-ui-widgets/src/crumb_alloc.rs`)
  사다리로 접는다 — 조상 한 칸씩 `…` 로 → 부모 축소 → 현재 폴더 축소 → `root › … › current` → `… › current`. 들어가면 접지 않는다. 성분 하나는 180px 에서 말줄임한다.
- **`…` 메뉴**: `…` 를 누르면 숨긴 조상들이 메뉴로 나열되고, 고르면 그 폴더로 이동한다. hover 하면
  **누르면 무엇이 되는지**를 말한다 — `Show 3 hidden folders`(`filepicker.hidden_folders_many`),
  하나면 단수형(`filepicker.hidden_folders_one`). 상태 서술("N folders hidden")이 아닌 이유는
  그 툴팁이 클릭 대상 위에 뜨기 때문이다. 단수형은 영어만 갈린다 — ko·ja 는 굴절이 없다.
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

### plugin 트리거(ADR-0058) — 즉시 ack + 이벤트 push

markdown plugin 의 "파일 열기" 팝업 Browse 버튼처럼, host 소유 popup 을 열고 사용자가 몇 프레임
뒤에나 확정/취소할지 모르는 인터랙션을 **plugin 이** 트리거해야 하는 경우의 IPC 경로다.
옛 `fs.pick_file` 은 동기 inline dispatch 였고 "OS native 모달이 자기 run loop 를 돌리므로
host 메인 스레드를 블로킹해도 안전" 하다고 적혀 있었다. 그 전제는 실측으로 뒤집혔고 그
메서드는 제거됐다([ADR-0162](../../adr/0162-a-host-blocking-native-dialog-is-not-an-agent-surface.md)).
host 자체 egui popup 은 그와 별개로 OS 가 대신 블로킹해주지 않는다 —
지연 회신 방식(`host.call` 자체를 확정 시점까지 붙잡아 둠)은 plugin 의 렌더/입력 루프를
멈추고 60 초 `HostCallTimeout` 위험을 진다(ADR-0058 Alternatives Considered).

1. plugin 이 `file_picker.trigger { filters?: string[] }` 를 호출한다(`FsRead` 권한,
   `gui` feature 전용). CLI·agent 호출자는 popup 을 열지 않고 `-32016` 을 받는다 — 결과를 받을
   곳이 없고 popup 이 사용자 입력 포커스를 가져가기 때문이다([ADR-0504](../../adr/0504-the-file-picker-trigger-answers-only-a-plugin-caller.md)). host 는 popup 확정을 **기다리지 않고** `{ request_id }` 만 즉시
   회신한다(`src/adapters/ipc/handler/file_picker.rs::handle_trigger`).
2. host 는 `(plugin_id, request_id)` 를 `FilePickerData.requester`(`FilePickerRequester`)
   에 기록하고 popup 을 연다 — 이후 로컬/원격 판별·엔트리 로드는 위 기존 경로(Tools 메뉴
   트리거와 동일)를 그대로 탄다.
3. 사용자가 확정/취소하면(`dispatch_pending_file_picker_results`/`apply_remote_confirm`),
   기존 동작(로컬 `DomainIntent::DispatchFile`, 원격 클립보드 복사+toast)에 **더해**
   `PluginManager::emit_host_event_to_plugin` 으로 `"file_picker.result"` 이벤트를 그
   plugin 에만 unicast 한다: `{ request_id, paths, cancelled }`(확정도 취소도 항상 이 세
   필드를 전부 채운다 — 취소는 `paths: []`/`cancelled: true`).
4. plugin 은 `on_event` 에서 이 key 를 받아 자기 `request_id` pending-map 으로 상관관계를
   맞춘다(신규 SDK 콜백 불필요 — 기존 `EventDispatchCtx` 재사용).

**`filters`**: 확장자 목록(점 없이, 예: `["md", "markdown"]`) — `draw_file_picker` 가 렌더
직전 `matches_filters` 로 파일 엔트리만 걸러낸다(디렉토리는 필터와 무관하게 항상 표시 —
내비게이션 대상이라 숨기면 하위로 못 들어간다).

**시작 위치 — `start_dir?: string` · `origin_surface_id?: u32`**: 피커는 그것을 띄운 surface 의
폴더에서 출발한다(`FilePickerStart`, `src/adapters/ui/popup/file_picker.rs`). 두 필드 모두
옵셔널이라 기존 호출자는 그대로 동작한다.

- **로컬/원격 판정은 출발 surface 의 workspace 로 한다** — `origin_surface_id` 가 있고 찾아지면
  그 surface 가 속한 workspace 가 mirror 인지 보고, 없으면 활성 workspace 로 폴백한다. 에이전트
  트리거는 활성 workspace 와 무관할 수 있기 때문이다(포커스 독립성).
- **시작 디렉토리**: `start_dir` 가 있으면 그것, 없으면 출발 surface 의 cwd
  (`CoreState::surface_cwd` — mirror 면 서버가 push 한 원격 cwd). 로컬은 **절대경로인 디렉토리일
  때만** 채택하고 아니면 홈으로 폴백한다(없는 경로로 열면 빈 에러 화면이 뜬다). 원격은 로컬에서
  stat 할 수 없으므로 그대로 `list_dir_request` 에 싣고 서버의 에러 회신에 맡긴다. 둘 다 없으면
  종전대로 로컬 홈 / 원격 홈(빈 `dir`)이다.
- **`inherit_cwd` 설정과 무관하다** — 그 설정은 "새 surface 가 cwd 를 상속하는가" 이고 피커는 새
  surface 를 만들지 않는다([ADR-0267](../../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md)
  결정 5). Tools 메뉴·단축키로 연 피커도 focus surface 의 폴더에서 출발한다.
- plugin 은 popup context 의 `observed_cwd`(로컬) / `remote_cwd`(mirror) 와 `origin_surface_id` 를
  그대로 실어 보내면 된다(키 의미는 `AppState::popup_surface_context`). 게이트가 걸린 `cwd` 키를
  쓰면 설정을 끈 사용자에게서 시작 위치가 사라진다.
- `path_input` 등 plugin 팝업에 사용자가 이미 적어 둔 경로를 시작점으로 삼는 것은 다루지 않는다.

**부모 범위와 숨김**: `owner_popup_instance`가 요청자 plugin의 부모 popup을 가리키면 host가
선언 종류와 target으로 유효 scope를 해석해 첫 paint 전에 적용한다([ADR-0278](../../adr/0278-child-file-picker-inherits-parent-scope-and-preserves-hidden-work.md)).
Surface 부모가 보이지 않으면 자식도 paint/hit/Esc/키 게이트에서 빠진다. 숨김은 close가 아니며
부모·자식 draft, 선택, 요청 ID를 보존한다. 돌아오면 같은 작업을 계속한다. Window 부모와
owner 없는 단독 피커는 창 범위다. surface가 작으면 기존 clamp가 피커 크기를 제한한다.
부모 close는 기존 cancel/settled 규약을 따른다. 확정 이벤트를 받은 markdown은 경로 입력만
채우며, 부모 Open에서 최초 context의 대상을 실행한다. host의 별도 로컬 DispatchFile은 유지한다.

**동시성 정책(ADR-0058 이 이 구현에 위임한 결정)**: `file_picker` popup 은 단일 인스턴스만
존재한다. 이미 열려 있는 상태에서 두 번째 `file_picker.trigger` 가 오면 **거부**한다(즉시
`-32000` JSON-RPC 에러) — "이전 요청을 대체" 는 채택하지 않았다. 트리거 핸들러는 `CoreState`
에만 접근하고 `PluginManager`(이벤트 emit 에 필요)에 접근권이 없어(이 코드베이스의 확립된
관례 — IPC 핸들러는 pending 을 큐잉하고 App 레벨 dispatch 가 실제 emit), "대체" 를 택하면
밀려난 요청의 plugin 에게 즉시 취소를 통지할 방법이 없어 그 plugin 의 pending-map 항목이
응답을 영영 못 받는다(ADR-0058 이 세운 "모든 트리거는 정확히 하나의 결과를 받는다" 계약
위반). 거부는 두 번째 plugin 의 `host.call` 이 그 자리에서 에러로 끝나 재시도 여부를
판단하게 하므로 이 계약을 지킨다. 근거 전문: `src/adapters/ipc/handler/file_picker.rs` 모듈
doc.

**Origin 태깅**: `file_picker.trigger` 로 연 popup 의 `OpenPopup` intent 는
`Intent::from_agent_plugin(plugin_id)` 로 발화한다(Tools 메뉴는 `from_user_menu` 그대로). IPC 로
여는 popup 은 늘 requester 를 가지므로 사용자 메뉴 발화로 기록되는 IPC 경로는 없다 —
`from_agent_plugin` 은 이 배선 전까지 실사용처가 없던 builder 였다(`src/intent.rs`).

### 설정 창에서의 로컬 전용 재사용

설정 창은 메인 윈도우와 별개의 winit 창이라, 메인 창 popup 스택(`AppState.dialogs.file_picker`)
에 사는 이 피커를 그대로 열 수 없다. 대신 **순수 view(`draw_file_picker_view`)만 재사용**한다 —
view 는 `FilePickerProps` 만 받고 `FilePickerAction` 만 돌려주므로 상태를 어디에 두든 그릴 수 있다.

- **상태**: 설정 창의 `SettingsUiState.file_chooser`(`SettingsFileChooser`) 가 갖는다. 메인 창의
  `FilePickerData` 와 별개이며 메인 창 상태 모듈(`src/state/dialogs.rs`)을 거치지 않는다. 한 번에 하나만 열리고, 새로 열면
  열려 있던 선택은 취소로 끝난다.
- **popup**: 설정 창 자체 `PopupManager` 에 `settings_file_chooser` 로 등록된다(단축키 충돌 확인
  popup 과 같은 매니저). 크기는 두 모드 모두 메인 피커와 같은 상수(640×480)다.
- **로컬 전용**: 원격(mirror) 조회 경로(`pending_list_dir_forward` → attach client)는 메인 창 App
  루프가 소유하므로 설정 창에는 없다. 설정은 이 인스턴스 자신의 구성이라 로컬 파일시스템만 본다.
  목록은 위 "로컬 브라우징" 과 같은 `read_dir_entries` + `sort_entries` 동기 호출로 채운다.
- **블로킹의 성질**: 동기 I/O 라 느린 디스크에서는 그 프레임이 늘어지지만 유한하게 끝난다 — 메인
  피커의 로컬 경로와 같은 성질이다. OS 네이티브 다이얼로그는 쓰지 않는다(포털 없는 Linux 에서
  끝나지 않는다, ADR-0162).
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
  `take_outcome` 해 `Confirmed(PathBuf)` 또는 `Cancelled` 를 1 회 가져간다. 타이틀바 ✕ 로 닫히면
  취소로 남는다.
- **Esc**: 설정 창 popup 중 열려 있는 것의 z 순서가 가장 높은 하나만 받는다
  (`settings_escape_owner`). 파일 선택이 충돌 확인 popup 위에 떠 있으면 충돌 popup 의 키 처리
  (Enter/Y/Esc/N)는 돌지 않는다. Esc 는 설정 창 자체를 닫지 않는다.
- **호출처**: 설정 › 기타 › 스크립트의 Add card **Browse…**(`Open`, `lua` 필터) · Keybindings ›
  Import / Export 의 **Export…**(`Save`, `toml` 필터) · **Import…**(`Open`, `toml` 필터).
- **배치**: 갤러리 specimen `Overlays › File picker › Save mode — one confirm, in the footer`
  (`crates/tasty-gallery/src/catalog/components/file_picker.rs` 의 `draw_save_mode`)가 정본 배치다.

## 인터페이스

- **사용자 트리거**: Tools 메뉴 "파일 열기…"(`filepicker.tools_menu_item`), 설정 › 기타 › 스크립트
  Add card 의 Browse…(설정 창 안의 로컬 전용 재사용). 목록 행 더블클릭
  (디렉토리는 진입, 파일은 즉시 확정 — 저장 모드에서는 이름 칸 채우기) / 브레드크럼 클릭 / `…`
  메뉴의 숨긴 조상 / 상위 폴더 버튼 / 새로고침 버튼 / ESC(취소) / X 버튼(취소). 저장 모드는 이름 칸
  편집이 더해진다.
- **AI Agent (IPC/CLI)**: 없음 — popup 조작(선택/확정/취소) 자체는 순수 로컬 사용자 입력
  UI 다(release 의 사용자 입력 재현 금지 원칙). 단, **popup 을 여는 트리거**는
  `file_picker.trigger` IPC 로 plugin 에 열려 있다(ADR-0058) — markdown Browse
  버튼이 실사용처. 이건 "에이전트가 사용자 대신 파일을 고른다"가 아니라 "plugin 이 host 소유
  UI 를 사용자에게 대신 띄워준다" 는 위임이라 원칙과 상충하지 않는다(뒤이은 선택/확정은
  여전히 사용자 몫).
- **원격 / 점유**: 원격 디렉토리 요청은 그 attach 세션이 대상 workspace 를 이미 점유하고 있어야
  서버가 응답한다(mirror 워크스페이스 존재의 전제조건과 동일).

## 비-목표 (Out of scope)

- **원격 파일 내용 fetch** — 디렉토리 나열만. 확정 시 클립보드 복사 + toast 로 그친다. 이건
  `file_picker.trigger` 로 열린 경우도 동일 — 확정 결과는 plugin 에 `paths` 로만 전달되고,
  그 경로의 내용을 이 세션으로 가져오는 fetch 는 없다.
- **멀티 셀렉트 / 메인 피커의 파일명 직접 입력** — 현재는 단일 선택만 지원(`FilePickerData::selected`
  는 매번 교체). 메인 피커는 열기 전용이라 footer 이름 칸이 읽기 전용이다 — 편집 가능한 이름 칸은
  설정 창의 저장 모드에만 있다.
- **타입 필터 칩** — 디자인 footer 의 "All files ▾" 칩은 구현되지 않았다. 필터는 호출처가 정하는
  `filters` 뿐이다.
- **`StreamControl` enum 확장** — capture 패턴과 동일하게 그 enum 을 건드리지 않고 별도 `event`
  태그를 같은 채널에 얹었다.
- **대형 원격 디렉토리의 페이지네이션** — 700KiB 예산을 넘는 나머지는 truncation 으로만
  처리한다(toast 통지). "다음 페이지 로드" 는 이번 스코프 밖.
- **다중 plugin 트리거 큐잉/대체** — 동시성 정책은 "거부" 뿐이다. 대체/큐잉을 원하는
  케이스가 실사용에서 반복되면 ADR-0058 의 Reconsideration Triggers 대상이다.

## Acceptance Criteria

- Given 로컬(비-mirror) workspace 가 활성이고 focus surface 의 cwd 를 알 수 없음 When Tools 메뉴에서
  파일 피커를 열면 Then 로컬 홈 디렉토리 엔트리가 즉시(동기) 로드되어 표시된다.
- Given focus 된 로컬 터미널의 cwd 가 `/tmp` When Tools 메뉴에서 파일 피커를 열면 Then `/tmp` 의
  엔트리가 표시된다(`inherit_cwd` 가 꺼져 있어도 같다).
- Given mirror workspace 가 활성이고 원격 cwd 를 모름 When Tools 메뉴에서 파일 피커를 열면 Then
  `Loading` 상태를 거쳐 attach 채널로 받은 원격 홈 디렉토리 엔트리가 표시되고 헤더에 host 배지가
  뜬다.
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
- Given attach 점유가 없는 client 가 `list_dir_request` 를 보냄 Then 서버가 거부 회신한다
  (`ok: false`).
- Given 로컬 파일을 확정 Then `DomainIntent::DispatchFile` 이 발화되어 기존 오픈 경로로
  이어진다.
- Given 원격 파일을 확정 Then 선택 경로가 로컬 클립보드에 복사되고 toast 가 뜬다(원격 콘텐츠
  fetch 는 일어나지 않는다).
- Given X 버튼/ESC 로 popup 이 닫힘 Then 결과가 `Cancelled` 로 명시되어 dialog
  상태가 정리된다(다음 오픈에 이전 상태가 새지 않음).
- Given 열기 모드에서 디렉토리 엔트리가 선택됨(더블클릭 아님) Then [열기] 가 활성이고 누르면
  그 디렉토리로 들어간다 — 결과는 나지 않는다(디렉토리는 파일로 확정되지 않는다). 이름 칸은
  비어 placeholder 를 보인다.
- Given 저장 모드에서 디렉토리 엔트리가 선택됨 Then 이름 칸과 확정 버튼은 그대로이고 footer 에
  `filepicker.folder_not_save_target` 안내 줄이 선다.
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
- Given `filters: ["md"]` 로 트리거됨 Then 렌더된 엔트리 목록에서 `.md` 가 아닌 파일은
  제외되고(디렉토리는 필터와 무관하게 항상 표시), 확정/취소 시 `"file_picker.result"` 가
  그 요청을 낸 plugin 에만(unicast) push 된다.
- Given Tools 메뉴로 연 기존 흐름(`requester: None`) Then `file_picker.trigger` 도입 후에도
  동일하게 동작하고 결과 이벤트가 발화되지 않는다(회귀 없음).
- Given 설정 › 기타 › 스크립트 Add card When Browse… 를 누르면 Then 설정 창 안에 로컬 홈
  디렉토리를 보여주는 파일 선택 popup 이 열리고, 떠 있는 동안 IPC 왕복(`tasty list info`)이
  응답한다.
- Given 설정 창 파일 선택이 `lua` 필터로 열림 Then `.lua` 가 아닌 파일은 목록에 없고 디렉토리는
  보이며, 디렉토리나 필터 밖 파일은 확정되지 않는다.
- Given 설정 창 파일 선택에서 파일을 확정 Then 그 절대 경로가 연 쪽(`consumer`)으로 돌아간다
  (스크립트 Add card 는 파일 경로와, 비어 있으면 표시 이름을 채운다).
- Given 설정 창 파일 선택이 Esc 또는 타이틀바 ✕ 로 닫힘 Then 결과는 `Cancelled` 이고 연 쪽의
  입력은 바뀌지 않는다.
- Given 저장 모드 When 파일명을 입력하고 확정하면 Then 현재 디렉토리 + 파일명 경로가 돌아가고,
  파일명이 비었거나 경로 구분자·`.`·`..` 이거나 나열된 폴더 이름이면 확정 버튼이 비활성이다.
- Given 저장 모드가 열려 있다 When 화면을 본다 Then 확정 수단은 footer primary 버튼 하나뿐이다.
- Given 저장 모드 When 목록에서 기존 파일을 고른다 Then 확정되지 않고 그 이름이 이름 칸에 들어가며,
  경고 줄이 뜨고 버튼 라벨이 Overwrite 가 된다.
- Given 기존 파일을 고른 뒤 When 이름을 고친다 Then 선택이 풀리고 라벨이 Save 로 돌아오며, 확정하면
  고친 이름의 경로가 돌아간다.
- Given 어떤 깊이 · 어떤 성분 길이의 경로든 When 메인 피커나 설정 창 파일 선택이 열린다 Then footer 의
  취소·확정 버튼이 온전히 보인다.
- Given breadcrumb 이 path bar 폭을 넘는 깊은 경로 When 그린다 Then 조상이 `…` 로 접히고(`crumb_alloc` 사다리),
  `…` 를 누르면 숨긴 조상이 나열되며 고르면 그 폴더로 이동한다.

> **검증 한계(문서화)**: 원격 attach loopback e2e(`--ssh 127.0.0.1:<port>`)로 실제 GUI 두
> 인스턴스를 띄워 popup 을 열고 눈으로 확인하는 것은 이 headless 작업 환경(GPU 디스플레이
> 없음)에서 실행할 수 없었다. 대신 `tests/attach_list_dir_loopback.rs` 가
> `tests/attach_silent_disconnect.rs` 와 동일한 방식으로 **실제로 기동한 `tasty` 서버
> 인스턴스**에 raw `TcpStream` 으로 `stream.open{target_workspace}` 핸드셰이크를 걸어 진짜
> attach 점유를 획득한 뒤, `list_dir_request` 를 보내 서버가 **실제 디스크의 임시 디렉토리**를
> 읽어 `list_dir_result` 로 정확히 회신하는 전체 왕복을 검증한다(성공 케이스, 존재하지 않는
> 디렉토리의 에러 케이스, attach 점유 없는 client 의 거부 케이스 3가지). GUI 렌더링(popup 이
> 그 결과를 실제로 화면에 그리는 것)만 코드 리뷰로 대체했다 — `draw_file_picker`/
> `draw_file_picker_view` 는 `AppState`/`CoreState` 를 받는 순수 함수라 GUI 없이도 로직은
> 동일 경로를 타지만, 실제 픽셀 렌더는 이 환경에서 확인하지 못했다. 이는
> `docs/features/remote-attach/index.md` / `remote-screenshot-clipboard/index.md` 가 이미
> 기록한 것과 동일한 종류의 한계이되, 이번 작업은 실제 서버 프로세스를 상대로 한 프로토콜
> 왕복까지는 실행 검증했다는 점에서 그 두 문서보다 한 단계 더 나아간 커버리지다.
>
> **`file_picker.trigger` 검증**: 격리된 `TASTY_HOME` 으로 기동한 debug gui 인스턴스(Xvfb)에
> raw `TcpStream` 으로 JSON-RPC(`file_picker.trigger {}`)를 보내면 — 이 경로의 호출자는
> `CallerContext::Local` 이다 — `-32016` 으로 거부되고, 호출 전후 `ui.state` 의
> `gate_host_popup_focused` · `keyboard_shortcuts_gated` 가 둘 다 `false` 그대로다(2026-09-23 실측).
> 이 거부 전에는 같은 호출이 popup 을 열어 두 값이 `true` 로 바뀌고 intent 감시 로그에
> `origin=User { source: Menu("tools_menu") }` 가 찍혔다(2026-09-22 실측, ADR-0504 Context).
> plugin 호출자의 성공·requester 기록·busy 거부(`-32000`)는 단위 시험
> (`src/adapters/ipc/handler/file_picker.rs` 의 `tests`)이 본다. agent 토큰 호출자의 거부도 같은
> 시험이 보고, 실제 인스턴스에서는 재지 않았다. plugin 프로세스(markdown)가 실제로
> `trigger_file_picker`/`on_event` 를 왕복하는 것과 popup 의 픽셀 렌더는 이 환경에서
> 실행하지 못해 코드 리뷰로 대체했다 — 다만 그 왕복이 재사용하는 `emit_host_event_to_plugin`
> 자체는 `git_viewer.query_result` 로 이미 프로덕션에서 검증된 동일 경로다.

## 구현

- Popup: `src/adapters/ui/popup/file_picker.rs`(`FilePickerProps`/`FilePickerMode`/`FilePickerAction`/
  `draw_file_picker_view`/`draw_file_picker`/`on_close_file_picker` — X 버튼 등
  draw_fn 을 거치지 않는 닫힘도 `PopupDef.on_close` 훅으로 `Cancelled` 명시),
  같은 디렉토리의 `file_picker/path_bar.rs`(breadcrumb · 가운데 생략 · `…` 메뉴) ·
  `file_picker/footer.rs`(이름 행 · 덮어쓰기 경고 · 버튼 행) · `file_picker/layout_tests.rs`,
  `src/adapters/ui/popup/defs.rs`(`PopupDef` 등록), `src/adapters/ui/popup.rs`(모듈 선언).
- 공유 나열: `src/core/fs_list.rs`(`DirEntryInfo`/`read_dir_entries`/`sort_entries`/`human_size`/
  `format_modified`) — `src/adapters/ui/surface/explorer/view.rs`(Explorer surface)와 공유.
- 트리거: `src/adapters/ui/tools_menu.rs`(`BuiltinAction::OpenFilePicker`, `popup::file_picker::open`,
  `requester: None`), `src/adapters/ipc/handler/file_picker.rs`(`file_picker.trigger` — plugin
  트리거, ADR-0058), `crates/tasty-ipc/src/method_meta.rs`(`file_picker.trigger` →
  `FsRead`).
- Result drain: `src/app/dispatch/file_picker.rs`(`dispatch_pending_file_picker_results`,
  `apply_remote_confirm`, `emit_file_picker_result` — `requester` 가 `Some` 이면
  `"file_picker.result"` unicast), `src/app/event_handler.rs`(`about_to_wait` 호출).
- plugin 요청자 상태: `src/state/dialogs.rs`(`FilePickerRequester`, `FilePickerData.requester`/
  `filters`), `src/core/mod.rs`(`next_file_picker_trigger_request_id` — `FpLoadState::Loading`
  의 내부 `request_id` 와 별개 네임스페이스), `src/intent.rs`(`Intent`/`UiIntent::
  from_agent_plugin` — 이 트리거가 첫 실사용처).
- plugin caller: `crates/tasty-plugin-markdown/src/popup.rs`(`trigger_file_picker`,
  `FILE_PICKER_RESULT_EVENT`), `crates/tasty-plugin-markdown/src/main.rs`(`pending_file_picker`,
  `on_event` 의 `"file_picker.result"` 수신).
- 원격 요청 큐: `src/core/mod.rs`(`PendingListDirForward`, `next_list_dir_request_id`),
  `src/core/state.rs`(`CoreState.pending_list_dir_forward`).
- 원격 전송(client): `src/app/attach_client.rs`(`send_list_dir_request`, `parse_list_dir_result`,
  `MirrorEvent::ListDirResult`, `apply_attach_client_output` 반영, `dispatch_pending_list_dir_forwards`).
- 원격 수신(server): `crates/tasty-ipc/src/stream_hub.rs`(`ListDirRequestMsg`, `pump_inbound`
  분류), `src/core/attach_runtime.rs`(`handle_list_dir_request`, `list_dir_for_request`,
  `list_dir_entry_wire`, `list_dir_entries_wire_capped`/`LIST_DIR_ENTRIES_BYTE_BUDGET`). GUI
  (`src/app/event_handler.rs::apply_list_dir_request_msg`)와 headless(`src/boot/headless_stream.rs`) 양쪽
  진입점에서 동일 서버 로직을 호출.
- Popup 상태: `src/state/dialogs.rs`(`FilePickerData`, `FpLoadState`, `FilePickerResult`).
- 설정 창 재사용: `src/view/settings/ui/file_chooser.rs`(`SettingsFileChooser`/`FileChooserMode`/
  `FileChooserOutcome`, 저장 모드 이름 칸·덮어쓰기 판정), `src/view/settings/ui.rs`(`PopupManager` 등록 ·
  `open_file_chooser` · `settings_escape_owner` · `apply_file_chooser_outcomes`),
  `src/view/settings/ui/tabs/misc.rs`(`ScriptsUiState::BROWSE_CONSUMER`/`apply_browsed_file`).
- i18n: `lang/{en,ko,ja}.toml` `[filepicker]`/`[filepicker.error_perm]`/`[filepicker.error_conn]`/`[filepicker.save]`.
- 갤러리 specimen: `crates/tasty-gallery/src/catalog/components/file_picker.rs`(`draw` · `draw_states` · `draw_save_mode`) + `file_picker/{path_bar,footer}.rs`.
- 테스트: `crates/tasty-ipc/src/stream_hub.rs`(`pump_inbound_classifies_list_dir_request`),
  `src/core/fs_list.rs`(`human_size_units`/`sort_dirs_first`/`read_dir_entries_lists_files_and_dirs`),
  `src/core/attach_runtime.rs`(`list_dir_entries_wire_capped_tests` — byte-budget truncation),
  `src/adapters/ui/popup/file_picker.rs`(`path_helper_tests` — POSIX/Windows 원격 경로 처리 +
  `matches_filters_*` 확장자 필터), `src/adapters/ipc/handler/file_picker.rs`(`tests` —
  trigger 성공/requester 기록/busy 거부/filters 전달, 실제 `AppState`/`CoreState` fixture),
  `crates/tasty-ipc/src/method_meta_tests.rs`(`file_picker_trigger_requires_fs_read`),
  `tests/attach_list_dir_loopback.rs`(실제 서버 인스턴스 상대 loopback 왕복 3종 — 성공/디렉토리
  없음 에러/attach 점유 없는 client 거부), `src/view/settings/ui/file_chooser.rs`(`tests` — 설정 창
  재사용의 확정·필터·이동·읽기 실패·저장 이름 검증·외부 닫힘 · 저장 모드의 단일 확정 대상·선택 해제·
  덮어쓰기 판정·더블클릭), `src/adapters/ui/popup/file_picker/layout_tests.rs`(헤드리스 렌더로 footer
  버튼 무잘림 · 깊은 경로 가운데 생략 · 짧은 경로 비생략 · 모든 조상이 칠해지거나 `…` 뒤에 있음).
