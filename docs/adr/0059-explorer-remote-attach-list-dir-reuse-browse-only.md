# ADR-0059: explorer 원격(attach mirror) 브라우징 — 기존 list_dir 채널 재사용 + browse-only

- **Status**: Accepted
- **Date**: 2026-08-01
- **Tags**: explorer, attach, mirror, remote, list-dir, browse-only, occupancy-trust, view-store, wire-format, adr-0053, adr-0054, adr-0056
- **Group**: remote-attach

## Context

attach mirror 워크스페이스에서 terminal/markdown/image/mesh_demo surface 는 이미 실 데이터를 mirror 하지만, explorer surface 는 content-less placeholder 로만 표시된다(`docs/features/remote-attach/index.md` 에 "기술적 미구현"으로 명시). 원인은 다음과 같이 이어지는 파이프라인 전체에서 explorer 가 아직 고려되지 않았기 때문이다:

1. **분류 단계** — `crates/tasty-model/src/workspace.rs::classify_attach_surfaces` 는 explorer 를 plugin_id 없는 host-native surface 로 보고 mesh 후보에서 제외해 `non_terminals` 버킷으로 분류한다.
2. **wire 스냅샷** — `src/core/attach_runtime.rs::build_workspace_tree_surfaces` 는 `non_terminals`(+ mesh 화이트리스트 탈락분)를 `{"remote_id", "role": "placeholder", "kind"}` 로만 내려보낸다. 경로/root 정보가 없다.
3. **클라이언트 트리 재구성** — `src/app/attach_client.rs::build_layout` 은 remote surface id 가 `term`/`mesh` 맵 어디에도 없으면 무조건 `EmptySurface` placeholder leaf 로 만든다. explorer 전용 4번째 분기가 없다.
4. **로컬 explorer 의 실제 데이터 접근** — 로컬 `ExplorerPanel`(`crates/tasty-model/src/explorer_panel.rs`, `tabs: Vec<ExplorerTab>` + `active`, 각 tab 은 `cwd`(고정 루트)/`root`(=current, 자유 이동 가능한 현재 폴더) 분리)의 렌더/상태는 `src/adapters/ui/surface/explorer/view.rs::ExplorerView`(surface id 로 keying 하는 `ExplorerViewStore` 소유)가 담당한다. `sync()`(메인 목록)과 `tree_children_of()`(좌측 트리)가 각각 직접 로컬 `read_dir_entries`(`src/core/fs_list.rs`, 동기 `std::fs`)를 호출한다.
5. **list_dir 요청/응답 인프라는 이미 존재하지만 File Picker 전용으로 좁게 설계됨**:
   - `src/core/mod.rs::PendingListDirForward { local_ws_id, request_id, dir }` — 문서 주석이 "popup wrapper 가 push"라고 명시, File-Picker 전용.
   - `src/app/attach_client.rs` 의 `MirrorEvent::ListDirResult` 처리(약 1090~1147행)는 `main.state.dialogs.file_picker` 하나만 보고 `crate::state::FpLoadState::Loading{request_id, ..}` 와 request_id 일치 여부로 매칭한다 — 다른 소비자(explorer surface/tab)로 라우팅할 방법이 없다.
   - 서버측 `handle_list_dir_request`(`attach_runtime.rs:949`)의 인가는 `engine.attach.client_holds_workspace(client_id)`(이 engine 이 호스팅하는 **어떤** workspace 든 점유했는가)만 확인한다 — 특정 workspace/surface/root 로 제한하지 않는다. wire 스키마(`crates/tasty-ipc/src/stream_hub.rs` `ListDirRequestMsg::ListDirRequest{request_id, dir}`)도 `dir` 문자열만 실어보내 surface/root 바인딩 필드가 아예 없다. File Picker(범용, 사용자가 임의 경로를 타이핑)에는 이 넓은 인가로 충분했지만, explorer surface 하나가 자신의 root 밖 경로까지 볼 수 있어야 하는지는 별개 판단이다.
6. **응답 크기 상한은 이미 존재** — `LIST_DIR_ENTRIES_BYTE_BUDGET = 700 * 1024`(700 KiB), 초과 시 `list_dir_entries_wire_capped` 가 자르고 `"truncated": true` 를 실어 보낸다. File Picker 는 이미 이를 toast(`filepicker.remote_listing_truncated`)로 노출 중이다.
7. **인가 범위 — 현재 surface/root 바인딩 없음** — 5번 항목과 동일 사실의 재확인: wire 는 `{request_id, dir}` 뿐이고 인가는 "이 engine 의 아무 workspace 나 점유"만 확인한다.
8. **`fs_list.rs` 자체는 이미 견고 — 변경 불필요할 가능성 높음**: `read_dir_entries`(`src/core/fs_list.rs:28-59`) 는 (a) 개별 엔트리 `metadata()` 실패를 무시하고 계속 진행, (b) 비-UTF8 파일명을 이미 `to_string_lossy()` 로 손실 변환(로컬에서도 동일 — 원격이 새로 만든 문제가 아님), (c) 심볼릭 링크는 `DirEntry::metadata()` 가 따라가지 않으므로 이미 `is_dir=false` 로 표시된다(로컬과 동일 동작, 이 함수 레벨에서 순환/재귀 위험 없음). 상위 `list_dir_for_request`(`attach_runtime.rs:990`)는 이미 `PermissionDenied` → `"permission denied"` 대 그 외 io 에러 → `e.to_string()` 을 구분하고, 응답 전 `sort_entries` 로 Name/Asc 정렬까지 마친다.

`ExplorerViewStore` 는 이미 `SurfaceId` 로 keying 되어(git-viewer 의 `PendingGitQueryForward` 가 `local_ws_id` 대신 mirror **surface_id** 로 앵커링하는 것과 동일한, 최근 기능일수록 surface 단위로 세밀화되는 추세와 일치), `sync()`/`tree_children_of()` 가 이미 "현재 로드된 (경로, 정렬) 키"와 "디렉토리별 트리 캐시" 를 각각 추적하는 구조다. 이는 지금부터 판단할 다중 소비자 라우팅 설계에 직접적인 선례가 된다.

ADR-0053(File Picker)은 원격 파일 **내용** fetch 를 명시적으로 스코프 밖에 두었다(확정 경로는 클립보드 복사 + toast 로만 처리, 열지 않음). ADR-0054(bulk 파일 전송)는 "로컬→원격 붙여넣기" 방향의 별도 커넥션 + binary `Data` frame 설계를 다루며, Context 에 "원격 폴더 브라우징(explorer 원격 탐색)은 범위 밖이며, 필요해지면 이 전송 계층 위에서 별도로 설계한다" 고 명시한다 — 본 ADR 이 그 "필요해지는 시점"의 절반(브라우징)만 다룬다. ADR-0056(git-viewer)은 동일한 `StreamTag::Control` raw-JSON-event 패턴의 세 번째 재사용 사례이며, 비동기 결과를 최종 소비자(별도 프로세스인 plugin)에 전달하기 위해 Event Bus unicast + 수동 repaint 플래그 + `request_id=0` abandon sentinel 을 쓴다 — host 는 어떤 특정 id 를 plugin 이 기다리는지 추적하지 않고 plugin 내부에만 그 상태가 있다. explorer 는 host-native(view-store 소유)라 이 정확한 메커니즘을 그대로 가져오긴 어렵지만, "비동기 응답을 host 가 대신 추적하지 않고 실제 소비자(view/plugin)가 스스로 추적하게 한다"는 핵심 아이디어는 재사용 가능한 선례다.

## Decision

explorer 원격 mirror 브라우징은 **File Picker 와 동일한 `list_dir_request`/`list_dir_result` wire 채널을 그대로 재사용**하고, 다음 10개 항목으로 설계를 확정한다.

1. **초기 root/tab 범위**: mirror 시작 시 원격 explorer 의 **활성 탭(active tab)의 root 만** 클라이언트에 보낸다(전체 탭을 미리 다 보내지 않음 — File Picker 도 요청 시점에만 조회하는 lazy 패턴과 동일). 원격에서 탭 추가/삭제/root 변경이 일어나면 기존 `StructuralDelta` 채널이 아니라, 클라이언트가 필요해질 때(탭 전환/새 탭 열람) 그 시점에 새 `list_dir_request` 를 보내는 **on-demand 재조회**로 처리한다 — 구조 델타에 디렉토리 스냅샷까지 얹으면 두 종류의 변경(구조 vs 파일시스템 내용)이 뒤섞여 무효화 규칙이 복잡해진다.
2. **쓰기 동작 스코프 — browse-only, 읽기 전용으로 한정한다.** rename/delete/새폴더 등은 이번 스코프에 포함하지 않는다.
   - 근거 1(선례): File Picker 가 이미 이 정확한 선례다 — read-only, 쓰기 없음.
   - 근거 2(기존 정책, 더 강함): `docs/features/explorer/index.md` 의 "비-목표" 절이 이미 **로컬** explorer 의 컨텍스트 메뉴 쓰기 조작(복사/잘라내기/붙여넣기/이름변경/휴지통삭제)을 "에이전트(IPC/CLI) 노출 대상이 아니다 — 사용자 우클릭 조작 전용" 이라고 명시한다. 원격 mirror 는 attach 채널을 경유하는 시점에 이미 에이전트/IPC 성격의 채널이므로, 로컬에서조차 IPC 비노출로 못박은 조작을 원격에 새로 여는 것은 기존 정책과 정면으로 충돌한다.
   - 근거 3(비용): 쓰기를 허용하려면 새 wire event pair, 원격 파일시스템 mutation 에 대한 별도 인가 판단(read 보다 훨씬 높은 리스크), 동시 mutation 충돌/원자성 처리가 추가로 필요하다 — 이번 ADR 의 목적(placeholder 를 실제 브라우징으로 바꾸는 것)에 비해 과도한 스코프 확장이다.
   - 향후 필요해지면 별도 ADR 로 다룬다(본 ADR 은 "쓰기는 아직 아니다"만 확정, "영원히 안 한다"가 아니다).
3. **파일 내용 fetch(더블클릭 열기) — 이번 스코프에서 제외한다.**
   - ADR-0053 이 File Picker 에서 이미 동일한 선택을 했다(확정 경로는 클립보드 복사만, 열람 없음) — 목록(메타데이터: 이름/크기/mtime/타입)만 노출하는 것과 내용을 노출하는 것은 리스크 등급이 다르다.
   - ADR-0054 의 bulk 채널(별도 connection, binary `Data` frame)을 역방향(원격→로컬)으로 재사용하는 것은 프로토콜상 대칭이라 기술적으로는 그럴듯하지만, 그 결정은 "전송 계층"만 다루며 임시파일 수명/크기 상한/MIME 안전성 같은 상위 정책은 별도로 설계해야 한다고 ADR-0054 자신이 명시한다. 이 정책들은 아직 이 ADR 에서 분석되지 않았으므로, 지금 함께 결정하면 근거 없는 결정이 된다.
   - 따라서: mirror explorer 에서 디렉토리 더블클릭은 내비게이션으로 처리하고, 파일 더블클릭은 이번 스코프에서 동작 없음(또는 "원격 파일 열기는 지원하지 않음" toast)으로 둔다. `src/adapters/ui/egui_panels.rs` 의 `OpenFile` action 은 이번 구현에서 mirror surface 에 대해 트리거되지 않도록 한다(로컬 surface 에서는 기존과 동일하게 동작). 파일 fetch 는 필요해지면 별도 ADR 로 ADR-0054 확장을 다룬다.
4. **`ListDirResult` 다중 소비자 라우팅 — `ExplorerViewStore` 가 자체적으로 pending 상태를 소유한다(Codex 제안 채택).** host/core 레이어에 "request_id → consumer" 범용 레지스트리를 새로 만들지 않는다.
   - 근거: `ExplorerViewStore` 는 이미 `SurfaceId` 로 keying 되어 있고, `ExplorerView` 는 이미 "현재 로드된 (경로, 정렬 기준) 키"(`sync()`)와 "디렉토리별 트리 캐시"(`tree_children_of()`, `HashMap<PathBuf, Vec<DirEntryInfo>>`)를 **자체 소유**하고 있다. explorer 의 실제 요구사항 — 여러 mirror surface, surface 당 여러 tab, 메인 목록 요청과 트리 펼침 요청이 동시에 여러 개 뜰 수 있음 — 은 request_id 하나짜리 단일 `Loading` 상태(`FpLoadState`, File Picker 는 popup 이 하나뿐이라 이걸로 충분했다)로는 표현할 수 없고, 애초에 "경로별" 다중 pending 을 표현해야 한다. 이는 host 레이어의 범용 레지스트리보다 `ExplorerView` 가 이미 갖고 있는 "경로 키 → 캐시" 구조를 그대로 "경로 키 → Loading{request_id, sent_at} | Loaded | Error" 로 확장하는 쪽이 자연스럽다.
   - 범용 레지스트리를 host 에 새로 만드는 대안을 기각한 이유: (a) view 가 이미 추적해야 하는 상태(어떤 경로가 pending 인지)를 host 에도 중복 추적해야 해서 두 곳의 상태를 동기화해야 한다, (b) surface 종료 시 정리(cleanup)를 host 레지스트리와 `ExplorerViewStore::drop_view` 양쪽에서 해야 한다(현재는 `drop_view` 하나로 충분), (c) File Picker/git-viewer 선례 어느 쪽도 실제로 범용 레지스트리를 쓰지 않는다 — File Picker 는 단일 소비자를 직접 매칭하고, git-viewer 는 소비자(plugin)가 스스로 추적한다(request_id=0 sentinel 로 abandon). 즉 이 코드베이스의 기존 패턴은 "host 는 보내기만 하고, 최종 소비자가 자기 pending 상태를 스스로 추적한다"는 방향으로 이미 일관되어 있다.
   - 구체적 라우팅: `PendingListDirForward` 는 요청 발신 시 소비자 태그(예: `None` = File Picker, `Some(surface_id)` = explorer)를 함께 실어보내고, `MirrorEvent::ListDirResult` 도착 시 App 레이어는 태그를 보고 File Picker 의 기존 매칭 로직(request_id 비교) 또는 해당 `surface_id` 의 `ExplorerView` 안 pending 경로 매칭 로직 중 하나로 분기한다. request_id 자체는 기존처럼 `next_list_dir_request_id()` 로 프로세스 전역 유일성만 보장하면 충분하다(별도 consumer-keyed 채번 불필요).
5. **`handle_list_dir_request` 인가 범위 — 기존의 넓은 인가("이 engine 의 어떤 workspace 든 점유")를 그대로 둔다. wire 에 surface_id/root 필드를 추가하지 않는다.**
   - 근거 1(일관성): 이 코드베이스의 attach 관련 모든 유사 기능(구조 op forward, capture 업로드, list_dir, git_query)이 예외 없이 동일한 "attach 점유 = 신뢰" 원칙을 쓴다. explorer 만 다른(더 좁은) 모델을 적용할 근거가 없다.
   - 근거 2(실효성 부재 — 더 중요): `list_dir_request`/`list_dir_result` 는 File Picker 와 explorer 가 **동일한 wire 이벤트 쌍을 공유**한다. 이 채널 자체가 이미 임의 경로 조회를 허용하도록(File Picker 의 존재 이유 자체가 "임의 경로 탐색") 설계되어 있으므로, explorer 요청에만 별도 `surface_id`/root 필드를 얹어 서버가 그 범위로 제한하더라도, 동일 client 는 그냥 File Picker 경로(또는 동일 wire 포맷을 흉내낸 임의 요청)로 우회해 동일 정보를 얻을 수 있다 — 실질적인 접근 통제 효과가 없는데 wire 스키마/핸들러 분기만 늘어난다.
   - 근거 3(기존 신뢰 경계 정합): attach 노출 기능의 표준 판정 기준은 "SSH 로 이미 가능한가" 이지, 별도 permission gate 를 새로 요구하는 게 아니다(이 레포의 확립된 판단 기준). attach 를 점유한 client 는 이미 그 원격 호스트에 대한 사실상 전체 로컬 사용자 권한과 동등한 신뢰를 받고 있으므로(File Picker 가 이미 그 전제로 동작), explorer 만 좁히는 것은 이 전제와 모순된다.
   - 결론: `crates/tasty-ipc/src/stream_hub.rs::ListDirRequestMsg`, `src/app/event_handler.rs::apply_list_dir_request_msg`, `src/boot.rs` 의 헤드리스 대응 경로는 **변경 없음** — 기존 핸들러(`handle_list_dir_request`)를 그대로 재사용한다.
6. **동시성/신뢰성 — 기존 동작을 그대로 계승한다.** disconnect/reconnect 시 pending 요청 폐기, mirror surface 제거 시 정리(`ExplorerViewStore::drop_view` 확장), out-of-order/stale 응답 무시(request_id 불일치는 조용히 무시 — File Picker 와 동일 패턴), 삭제된 cwd/주소 히스토리 폴백, 심볼릭 링크 표시(`is_dir=false`, 로컬과 동일), 에러 사유 노출(`"permission denied"` 대 raw `e.to_string()` 구분, 그대로 노출) — 전부 File Picker/기존 `fs_list.rs` 동작을 상속하며 explorer 전용 변경이 필요 없다.
7. **700 KiB truncation 이후 UX — File Picker 의 기존 toast(`filepicker.remote_listing_truncated`)를 explorer 전용 i18n 키로 재사용한다(문구만 explorer 컨텍스트에 맞게, 메커니즘은 동일).** 별도의 in-list "더 보기" 인디케이터는 이번 스코프에서 추가하지 않는다 — File Picker 선례를 그대로 따르는 것으로 충분하고, 별도 UI 는 필요해지면 후속으로 고려한다.
8. **비-UTF8 경로 wire 표현 — 기존 동작(`to_string_lossy()` 손실 변환)을 그대로 상속한다.** 이는 로컬 explorer 에서도 이미 발생하는 기존 동작이라 원격이 새로 만드는 문제가 아니다.
9. **정렬 위치 — 서버가 이미 수행하는 Name/Asc 1차 정렬 위에, explorer 의 컬럼 정렬(Size/Modified/Type, `SortColumn`)은 클라이언트가 응답 수신 후 기존 `sort_entries` 로 재정렬하는 것으로 충분하다.** 서버가 정렬 파라미터를 받아 재정렬해 보내는 기능은 추가하지 않는다 — 700 KiB 예산 안의 엔트리 수(수천 개 이하)는 클라이언트 재정렬 비용이 무시할 만하고, 서버에 정렬 파라미터를 추가하면 wire 스키마가 늘어나는데 얻는 성능 이득이 없다.
10. **occupancy/신뢰 모델 — 5번과 동일한 결론이다.** explorer 에 별도 신뢰 모델을 새로 만들 근거가 없다: "attach 점유 = 신뢰" 원칙(ADR-0053 과 동일)을 그대로 적용한다.

## Consequences

- **얻은 것**: mirror 워크스페이스의 explorer surface 가 placeholder 대신 실제 원격 디렉토리 목록을 표시할 수 있게 된다. 새 wire 프로토콜/새 인가 모델 없이 기존 `list_dir_request`/`list_dir_result` 채널·`fs_list.rs`·700 KiB 예산·에러 구분을 그대로 재사용해 구현 범위가 최소화된다(`stream_hub.rs`/`event_handler.rs`/`boot.rs` 변경 없음).
- **잃은 것**: 원격 explorer 에서 파일 rename/delete/새폴더/열기가 불가능하다(browse-only) — 원격 파일을 조작·열람하려는 사용자는 여전히 로컬 File Picker 복사 흐름이나 별도 방법(예: SSH 직접 접속)을 써야 한다. explorer 요청도 File Picker 와 동일한 인가 범위(root 밖 경로 조회 가능)를 공유하므로, "이 explorer surface 는 자기 root 아래만 봐야 한다"는 직관적 기대는 서버 단에서 강제되지 않는다(클라이언트 UI 가 root 밖으로 나가는 UI 흐름을 아예 제공하지 않는 것으로 사실상 억제할 뿐).
- **운영 비용 / 유지 부담**: `ExplorerView` 가 pending 요청 상태(경로별 Loading/Loaded/Error)를 새로 추적해야 해 그 구조체의 책임이 늘어난다. `PendingListDirForward`/`MirrorEvent::ListDirResult` 처리부가 이제 2개 소비자(File Picker, explorer)를 분기해야 해 그 부분의 복잡도가 약간 늘어난다. 향후 세 번째 소비자가 생기면 이 분기 로직을 다시 검토해야 할 수 있다(단, 그 시점에도 host 범용 레지스트리보다 "소비자가 스스로 추적" 패턴을 우선 검토할 것을 권장 — References 참고).

## Alternatives Considered

- **File Picker 의 `FpLoadState` 를 explorer 가 직접 재사용/공유한다**: 기각. `FpLoadState` 는 애초에 "popup 인스턴스 하나, pending 요청 하나"를 전제로 설계됐다(`Loading{request_id, sent_at}` 단일 필드, 배열/맵이 아님). explorer 는 (a) 여러 mirror surface 가 동시에 존재할 수 있고, (b) 한 surface 안에서도 tab 전환·좌측 트리 펼침이 각각 별도 경로에 대한 독립 요청을 만들어낼 수 있어(예: 메인 목록 로딩 중에 트리에서 다른 폴더를 펼치는 동시 요청), 단일 `Loading` 슬롯으로는 상태를 표현할 수 없다. 억지로 재사용하려면 `FpLoadState` 자체를 맵으로 바꿔야 하는데, 그러면 File Picker 의 단순한 단일-슬롯 의미가 오염된다 — 차라리 `ExplorerView` 전용의(개념적으로 유사하지만 독립적인) pending 상태를 두는 것이 File Picker 코드에 영향을 주지 않고 explorer 의 실제 동시성 요구를 정확히 표현한다.
- **host 레이어에 `request_id → consumer` 범용 라우팅 레지스트리를 신설한다**: 기각. Decision 4번의 근거 참조 — 이미 `ExplorerViewStore` 가 갖고 있는 경로별 상태 추적 구조와 중복되고, 정리(cleanup) 지점이 두 곳(레지스트리 + view store)으로 늘어나며, File Picker/git-viewer 선례 어느 쪽도 이 패턴을 쓰지 않아 새 패턴을 이 코드베이스에 처음 도입하는 셈이 된다.
- **wire 에 `surface_id`/root 필드를 추가해 인가를 그 root 이하로 제한한다**: 기각. Decision 5번의 근거 참조 — File Picker 와 채널을 공유하는 한 실질적 접근 통제 효과가 없고, "attach 점유 = 신뢰" 라는 이 코드베이스의 확립된 신뢰 경계와도 어긋난다.
- **explorer 원격 브라우징에 rename/delete 등 쓰기와 파일 열기까지 한 번에 포함시킨다**: 기각. Decision 2/3번의 근거 참조 — 로컬 explorer 조차 쓰기 조작을 IPC 비노출로 못박아둔 기존 정책과 충돌하고, 파일 내용 fetch 는 ADR-0054 가 별도 설계 필요 항목으로 명시적으로 남겨둔 것이라 이번 ADR 에서 근거 없이 함께 결정하면 안 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 사용자가 원격 explorer 에서 rename/delete/새폴더 등 쓰기 조작을 명시적으로 요청하는 경우 — 그때 별도 ADR 로 원격 파일시스템 mutation 의 인가 모델을 새로 설계한다.
- 사용자가 원격 파일 더블클릭으로 열람(내용 fetch)을 명시적으로 요청하는 경우 — 그때 ADR-0054 의 bulk 채널을 역방향으로 확장하는 별도 ADR 로 temp-file 수명/크기 상한/MIME 안전성을 함께 설계한다.
- `list_dir_request`/`list_dir_result` 채널의 세 번째 이상 소비자가 생겨(예: 다른 원격 브라우징 UI) 소비자 분기 로직이 감당하기 어려울 만큼 늘어나는 경우 — 그때 host 범용 레지스트리 도입을 재검토한다(단, 그 시점에도 "소비자가 스스로 추적" 패턴을 우선 검토).
- attach 의 "점유 = 신뢰" 원칙 자체가 조직 차원에서 재검토되는 경우(예: 세분화된 원격 permission 모델 도입) — 그때 explorer 뿐 아니라 File Picker/git-viewer/capture 업로드 전체를 함께 재검토한다.

## References

- [`docs/adr/0053-native-file-picker-remote-attach-channel.md`](0053-native-file-picker-remote-attach-channel.md) — `list_dir_request`/`list_dir_result` 채널·`FpLoadState`·"attach 점유 = 신뢰" 모델의 원조.
- [`docs/adr/0054-remote-filesystem-native-over-attach-stream.md`](0054-remote-filesystem-native-over-attach-stream.md) — bulk 파일 전송 채널(역방향 재사용 시 확장 대상), explorer 원격 탐색을 명시적으로 스코프 밖에 둔 문서.
- [`docs/adr/0056-git-viewer-remote-attach-git-query-channel.md`](0056-git-viewer-remote-attach-git-query-channel.md) — 동일 패턴의 세 번째 재사용, "소비자가 스스로 pending 상태를 추적" 접근의 선례(Event Bus unicast + abandon sentinel).
- [`docs/features/explorer/index.md`](../features/explorer/index.md) — `ExplorerPanel`/`ExplorerTab`/`ExplorerView` 구조, 컨텍스트 메뉴 쓰기 조작의 "비-목표"(IPC/CLI 비노출) 정책.
