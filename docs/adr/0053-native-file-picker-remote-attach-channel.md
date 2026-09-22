# ADR-0053: 로컬+원격 겸용 native 파일 피커 — attach 커스텀 이벤트 채널 + 하이브리드 신뢰 모델

- **Status**: Accepted
- **Date**: 2026-07-23
- **Tags**: file-picker, popup, attach, mirror, ipc, permission, fs-read, occupancy-trust, timeout, wire-format, tools-menu, adr-0042, adr-0032, adr-0040
- **Group**: remote-attach

## Context

ADR-0042 는 "파일 열기"를 host `fs.pick_file`(`FsRead` 권한) IPC 로 위임해 native OS 다이얼로그
(`rfd::FileDialog::pick_file()`)를 동기 모달로 여는 결정을 내렸다. 이 모델은 **로컬 파일시스템
한정** — 호출자(plugin)가 host 메인 스레드에서 동기 회신을 기다리는 구조라, "원격(attach mirror
워크스페이스)의 디렉토리를 브라우징"이라는 개념 자체가 들어설 자리가 없다. 원격 브라우징은
필연적으로 attach 채널을 통한 비동기 요청/응답이 되는데, 이는 ADR-0042 의 Reconsideration
Trigger("host IPC dispatch 가 동기 inline 모델에서 벗어나 async 가 될 때")가 정확히 가리키는
상황이다.

동시에 attach 서브시스템은 이미 원격 스크린샷 캡처 기능(`capture_chunk`/`capture_commit`/
`capture_result`)에서 "`StreamControl` enum 이 인식하지 못하는 `event` 태그를 같은
`StreamTag::Control` 채널에 실어 보내고, 수신측이 그 태그로 별도 구조체를 시도 파싱한다"는 패턴을
이미 갖추고 있었다. 원격 디렉토리 나열은 이 패턴을 그대로 재사용할 수 있는 동일 성격의
요청/응답이다.

## Decision

**신규 in-app 파일 피커 popup**(`src/adapters/ui/popup/file_picker.rs`, `PopupDef` id
`"file_picker"`)을 만들어 로컬/원격 두 모드를 한 UI 로 겸한다. 다음 다섯 가지를 확정한다.

1. **관계**: ADR-0042 는 그대로 유지(Accepted, 로컬 전용·동기 모달 유스케이스에 계속 유효 —
   plugin 의 `host.call` 처럼 host 메인 스레드 동기 회신을 전제하는 호출자). 본 ADR 은 그
   전제가 성립하지 않는 "원격 인지 + 비동기" 유스케이스를 위한 **별개의 신규 메커니즘**이다.
2. **트리거 지점**: 사이드바 Tools 메뉴에 신규 항목(`filepicker.tools_menu_item`)을 추가해
   진입점으로 삼는다(`src/adapters/ui/tools_menu.rs`). 클릭 시 현재 활성 workspace
   (`state.active_workspace`)의 `Workspace.mirror` 플래그로 로컬/원격을 1회 판별해 popup 상태를
   채운다.
3. **권한 모델(하이브리드)**: 로컬 브라우징은 이 popup 자체가 host 프로세스 내부 함수
   (`crate::core::fs_list::read_dir_entries`)를 직접 호출하므로 별도 IPC 권한 게이트가 없다(로컬
   host UI 자체 기능이라 plugin 권한 체계 밖). 원격 브라우징은 attach 서버측
   (`src/core/attach_runtime.rs::handle_list_dir_request`)이 **attach 점유 = 신뢰**
   원칙(`engine.attach.client_holds_workspace(client_id)`)만으로 인가한다 — 구조 op forward나
   캡처 업로드와 동일한 신뢰 모델이며, 로컬 `fs.pick_file`(`FsRead`)과는 별개 표면이라 사분화하지
   않는다.
4. **요청 타임아웃**: 원격 요청은 popup 상태를 `FpLoadState::Loading { request_id, sent_at }` 로
   전이시키고 매 프레임 `sent_at.elapsed()` 를 판정해 soft timeout(8초, 응답 없음)이 지나면
   `ErrorConn` 으로 전이한다. 이와 별개로, mirror workspace 자체가(원격 disconnect 로) 사라지면
   `engine.find_workspace_index_for_id` 조회가 실패하는 것을 관측해 즉시 `ErrorConn` 으로
   전이한다 — attach 세션의 raw `disconnected` `AtomicBool` 은 App 소유라 popup wrapper
   (`AppState`/`CoreState` 만 접근 가능)에서 직접 읽을 수 없으므로, "mirror workspace 소멸"이라는
   더 상위의 관측 가능한 결과로 그 신호를 대체한다. 두 메커니즘이 상호보완적이다 — soft timeout
   은 "서버는 살아있는데 응답이 없는" 케이스를, mirror workspace 소멸 관측은 "세션 자체가 끊긴"
   케이스를 잡는다.
5. **wire 포맷**: `list_dir_request { request_id, dir } → list_dir_result { request_id, ok, dir?,
   entries?, reason? }` 이벤트 쌍을 capture 패턴과 동일하게 raw JSON `event` 태그로 같은
   `StreamTag::Control` 채널에 얹는다(`ListDirRequestMsg`/`MirrorEvent::ListDirResult`). 엔트리의
   `modified` 시각은 wire 조립/파싱 경계에서만 `modified_unix: u64`(unix epoch 초)로
   변환한다 — `DirEntryInfo`(`crate::core::fs_list`) 자체는 로컬/원격 어디서 만들어졌든 항상
   `Option<SystemTime>` 을 들고, 사람이 읽는 포맷(`format_modified` → `"YYYY-MM-DD"`)은 view
   렌더 직전에만 계산한다.

디렉토리 나열 로직(`read_dir_entries`/`sort_entries`/`human_size`/`format_modified`,
`src/core/fs_list.rs`)은 로컬 Explorer surface, 로컬 피커, 원격 attach 서버 핸들러가 모두 공유한다.

## Consequences

- **얻은 것**: 원격 attach 세션에서도 in-app 으로 디렉토리를 브라우징할 수 있다. 로컬/원격이
  완전히 동일한 `DirEntryInfo` 스키마를 쓰므로 view 레이어(`FilePickerProps`/
  `draw_file_picker_view`)가 분기 없이 하나의 렌더 경로로 양쪽을 그린다. 기존 capture 패턴을
  재사용해 `StreamControl` enum 확장 없이 새 이벤트 쌍을 추가했다 — attach 프로토콜 표면을
  건드리지 않는다.
- **잃은 것**: 원격 확정(Confirm)은 **디렉토리 나열까지만** 설계됐다 — 원격 파일 **내용**을 이
  세션으로 가져오는 fetch 는 스코프 밖이다. 원격에서 확정한 경로는 클립보드 복사 + toast 로만
  통지한다(`src/app/dispatch/file_picker.rs::apply_remote_confirm`). 로컬 확정은 기존
  `DomainIntent::DispatchFile` 로 실제 오픈까지 이어진다.
- **운영 비용 / 유지 부담**: attach 서버측 `read_dir_entries` 는 홈 디렉토리 이상 임의 경로를
  노출한다(soft/readonly 검증 없음) — attach 점유 신뢰 모델을 그대로 상속하므로 그 모델의 위험
  프로파일을 그대로 물려받는다. `list_dir_request`/`result` 는 `StreamControl` 밖의 raw JSON
  이라 스키마 변경 시 두 태그(로컬/원격 필드) 를 항상 동기화해야 한다(capture 패턴과 동일한
  유지비용).

## Alternatives Considered

- **ADR-0042 를 개정(amend)**: `fs.pick_file` 에 원격 분기를 추가. 기각 — ADR-0042 의 전제(동기
  inline `host.call` 회신)가 원격 비동기 요청과 근본적으로 양립하지 않는다. 개정이 아니라 신규
  ADR 로 분리하는 편이 "동기 로컬 유스케이스"와 "비동기 원격 유스케이스"를 문서 레벨에서도
  명확히 가른다.
- **트리거 지점으로 기존 `fs.pick_file` 호출부(markdown plugin browse 버튼) 교체**: 이 호출은
  cross-process 이자 동기(`host.call` 블로킹)라 스왑하려면 async plugin↔host IPC 시맨틱을
  새로 발명해야 한다 — 이는 ADR-0042 Reconsideration Trigger 가 예견한 **별개의** 미래 결정
  지점이지, 이번 작업에 묶을 범위가 아니다. 기각(대신 Tools 메뉴로 별도 진입점을 신설).
  자세한 판단 근거는 이 ADR 의 작성 시점 구현 로그(커밋 이력)에 남는다.
- **원격도 별도 `FilePickerRead` 권한 신설**: attach 채널의 다른 커스텀 이벤트(구조 op forward,
  캡처 업로드)가 전부 "점유 = 신뢰"를 쓰는데 이것만 별도 권한 게이트를 두면 attach 신뢰 모델
  일관성이 깨진다. 기각.
- **원격 확정 시 파일 내용까지 fetch**: 별도의 청크 전송/크기 제한/캐싱 설계가 필요한 큰 작업이라
  이번 스코프에 넣지 않았다. 클립보드 복사로 최소 동작만 제공하고, 필요해지면 후속 ADR 로
  분리한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 원격 확정된 파일의 **내용**을 이 세션으로 가져와야 하는 요구가 생길 때(현재는 경로
  클립보드 복사로 그친다).
- attach 점유 신뢰 모델 자체가 세분화(예: read-only 점유와 read-write 점유 구분)될 때 —
  `handle_list_dir_request` 의 인가 판정도 함께 재검토해야 한다.
- markdown plugin 등 기존 `fs.pick_file` 호출부를 실제로 이 신규 피커로 교체하기로 결정할 때 —
  그때는 async plugin↔host IPC 시맨틱 설계가 선행돼야 한다(ADR-0042 Trigger 참고).

## References

- [ADR-0042](0042-fs-pick-file-native-dialog-host-delegation.md) — 로컬 전용 `fs.pick_file`(FsRead)
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 점유 계층/신뢰 모델
- [`docs/dev-guide/popup-implementation.md`](../dev-guide/popup-implementation.md) — `PopupDef` 시스템
- [`docs/dev-guide/gallery-first.md`](../dev-guide/gallery-first.md) — gallery specimen 선행 정책
- `src/adapters/ui/popup/file_picker.rs` — popup wrapper/view/action
- `src/core/fs_list.rs` — 공유 디렉토리 나열 계층
- `src/core/attach_runtime.rs::handle_list_dir_request` — attach 서버측 핸들러
- `src/app/attach_client.rs` — reader thread 파싱 + `MirrorEvent::ListDirResult` 반영
- `crates/tasty-gallery/src/catalog/components/file_picker.rs` — gallery specimen
