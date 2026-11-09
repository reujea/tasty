# Empty (`empty`)

- **Status**: Implemented
- **kind**: `empty` — host 내장
- **렌더 경로**: host-egui
- **주체**: 로컬 사용자 · AI Agent · 원격 접속 사용자(mirror)
- **설계**: [이벤트 저장소 (`tasty-event-store`)](../../architecture/event-store.md) · [이벤트 저장소 (`tasty-event-store`)](../../architecture/event-store.md) — 복원 자리와 지연 활성화
- **코드**: 등록 `register_empty`(`src/runtime/surface_registry/builtins.rs`), 모델 `EmptySurface`(`crates/tasty-model/src/empty_surface.rs`), 복원 자리 `JournalPlaceholder`(`src/runtime/surface_restorer.rs`), 화면 `draw_empty`(`src/adapters/ui/surface/empty.rs`)
- **화면**: [아래 절](#화면)

## 목적·지원 범위

콘텐츠가 아직 정해지지 않은 surface 자리다. 사용자가 다른 kind로 바꿀 수 있는 빈 칸을 제공한다. 같은 `empty` kind 이름을 쓰는 객체가 셋 있으며 역할이 다르다.

| 상태 | 객체 | 트리의 `type` | 뜻 |
|---|---|---|---|
| 빈 자리 | `EmptySurface`(`deferred` 없음) | `Empty` | 정상 상태. 사용자가 다른 kind로 변환하기 전까지 빈 칸으로 남는다 |
| plugin 대기 자리 | `EmptySurface`(`deferred` 있음) | `Pending` | attach mirror에서 원격 markdown 문서를 보여 줄 plugin kind가 아직 등록되지 않은 자리 |
| 복원 자리 | `JournalPlaceholder` | `Pending` | 앱을 다시 시작했을 때 아직 활성화하지 않은 surface. 목표 kind(`terminal`·`markdown` 등)를 따로 기록한다 |

`EmptySurface`의 `deferred`는 plugin 복원 정보(kind·snapshot)만 담는다. 앱 재시작 때 터미널을 지연 생성하는 일은 복원 자리가 맡는다.

## 상태 소유자

- 구조 트리의 leaf는 다른 kind와 같이 `SurfaceDescriptor`다([작업 영역](../../features/work-area/index.md#도메인-트리)).
- 실행 객체(`EmptySurface`·`JournalPlaceholder`)는 엔진 runtime의 surface 저장소(`EngineRuntime.surfaces`)에 있다.
- `EmptySurface`는 생성 때 받은 시작 cwd(`cwd`)를 보관한다. 다른 kind로 변환할 때 그 cwd를 후보로 쓴다([cwd 정책](../../design/policies/cwd.md#surface-cwd-invariant)).
- 복원 자리는 목표 kind, 저장된 자료 참조(`data`·`creation_seed`), 직전 activation, 연속 실패 횟수(`attempts`), 마지막 실패 이유(`failure`), 자동 복구 차단 여부(`recovery_blocked`)를 가진다.

## 생성·갱신·종료

- **빈 자리**: `tasty new tab --pane <P> --type empty` 또는 `tasty split ... --type empty`로 만든다. 필수 params는 없다. attach client는 mirror 워크스페이스에서 대응하는 로컬 객체가 없는 leaf를 빈 자리로 채운다(`install_mirror_fallbacks`, `src/app/attach_client/survivors.rs`).
- **plugin 대기 자리**: attach client가 원격 markdown leaf를 받았는데 로컬에 markdown kind가 없을 때 만든다(`deferred_mirror_markdown_surface`). kind가 등록된 뒤 화면에 보이면 그 kind의 `restore`로 바뀐다(`reify_displayed_mirror_resources`). 이 자리는 로컬 journal에 들어가지 않는다.
- **복원 자리**: journal 복원 때 `initialize_instances`가 실행 객체가 없는 모든 surface에 하나씩 만든다. 활성화는 별도 확정 작업이다.
  - 부팅 단계(`poll_restore_bootstrap`)는 복원한 View에서 선택된 터미널과 kind가 이미 등록된 non-terminal surface를 활성화한다. 선택되지 않은 터미널과 아직 등록되지 않은 kind는 자리로 남는다. 부팅은 복원 자리가 기다리는 plugin kind의 등록을 기한 안에서 기다린다(`boot_required_plugin_kinds`).
  - 부팅 뒤 창에 보이는 surface(활성 워크스페이스에서 각 pane의 선택된 탭)는 `App::activate_displayed_restorations`(`src/app/window_lifecycle/pending.rs`)가 차례로 활성화한다. 한 번의 갱신에서 창마다 하나씩 시작한다.
  - 보이지 않는 터미널은 `tasty wake --surface <ID>`(IPC `surface.wake`)로 입력 없이 PTY만 띄울 수 있다. CLI 도움말에 따르면 입력을 보내는 명령은 대상을 자동으로 활성화한다.
  - 활성화에 실패하면 `attempts`가 1 늘고 이유가 `failure`에 남는다. 연속 5회(`MAX_ACTIVATION_ATTEMPTS`) 실패하면 자동 재시도를 멈추고 자리를 유지한다.
  - 이전 생성 작업의 결과가 확정되지 않은 surface는 `recovery_blocked`로 표시하고 자동 활성화 대상에서 뺀다.
- 닫기는 다른 surface와 같다. 빈 자리와 복원 자리는 정리할 실행 자원이 없다.

## 저장·복원

- 빈 자리의 저장 자료는 빈 JSON 객체다. 복원하면 cwd 없이 새 빈 자리가 된다.
- 복원 자리는 저장 대상이 아니라 목표 kind의 저장 자료를 가리킨다. 활성화 전에 다시 저장해도 원래 kind의 자료 참조를 유지한다([이벤트 저장소](../../architecture/event-store.md)).

## IPC·CLI

- `tasty list tree`: 빈 자리는 `type:"Empty"`로 보고한다. plugin 대기 자리는 `type:"Pending"`, `kind:<목표 kind>`, `ready:false`, `pending_reason:"plugin_not_loaded"`로, 복원 자리는 `type:"Pending"`, `kind:<목표 kind>`, `pty_ready:false`, `restore_error:<실패 이유 또는 null>`로 보고한다.
- `tasty list surfaces`(IPC `surface.list`): 복원 자리는 탭이 분할됐는지와 관계없이 `type:"Pending"`, `kind:<목표 kind>`, `pty_ready:false`, `restore_error:<실패 이유 또는 null>`로 보고한다(`list tree`와 같은 필드). plugin 대기 자리도 같은 모양으로 `type:"Pending"`, `kind:<목표 kind>`, `pty_ready:false`, `restore_error:null`이고, `list tree`와 같은 `pending_reason:"plugin_not_loaded"`가 붙는다. 빈 자리는 `type:"Empty"`로 보고한다.
- `tasty list tabs`(IPC `tab.list`): 탭의 `type`은 plugin 대기 자리와 복원 자리 모두 `Pending`, 빈 자리는 `Empty`다. `kind`·`pending_reason` 같은 상세 필드는 싣지 않는다.
- `tasty wake --surface <ID>`: 터미널 복원 자리만 받는다. 이미 PTY가 있으면 `{"woke":false,"pty_ready":true}`를 돌려준다. 터미널 복원 자리가 아니면 `Surface <ID> not found`로 거절한다.

## headless·원격 제약

- 헤드리스에도 등록된다. 헤드리스는 레이아웃을 저장·복원하지 않으므로(`EngineSelection::FreshHeadless`) 앱 재시작에서 오는 복원 자리가 없다.
- attach mirror의 빈 자리와 plugin 대기 자리는 위 생성 절을 따른다. 점유 규칙은 [remote-attach](../../features/remote-attach/index.md)에 있다.

## 검증 기준

- Given 실행 중인 Tasty When `tasty new tab --pane <P> --type empty` Then `tasty list tree`에 `type:"Empty"` leaf가 생긴다.
- Given 비활성 탭에 터미널이 있는 상태로 저장한 레이아웃 When 앱을 다시 시작한다 Then 그 터미널은 `tasty list tree`에서 `type:"Pending"`, `kind:"terminal"`로 보이고, 그 탭을 화면에 보이거나 `tasty wake --surface <ID>`를 보내면 PTY가 생긴다.
- Given markdown surface 하나만 있는 탭과 분할된 탭의 markdown surface 가 복원 자리로 남은 상태 When `tasty list surfaces` Then 두 항목 모두 `type:"Pending"`, `kind:"markdown"`, `pty_ready:false`, `restore_error:null` 이다.
- Given attach mirror에서 로컬 markdown kind가 없어 원격 markdown leaf가 plugin 대기 자리로 남은 상태 When `tasty list surfaces` Then 탭이 분할됐는지와 관계없이 그 항목은 `type:"Pending"`, `kind:"markdown"`, `pty_ready:false`, `restore_error:null`, `pending_reason:"plugin_not_loaded"`다.
- Given 활성화가 계속 실패하는 복원 자리 When 자동 활성화가 5회 실패한다 Then 자동 재시도를 멈추고 `restore_error`에 마지막 이유가 남는다.

## 화면

- **시각 소스**: 전용 디자인 원본 없음.

### 트리거

`empty` surface를 만들거나 attach mirror에서 빈 자리·plugin 대기 자리가 생길 때 표시한다.

### UI 요소 인벤토리

- **배경** — 포커스 바탕 쌍(`surface-bed-focused`·`-unfocused`)으로 칸 전체를 칠한다. 값은 [포커스 표현](../../design/policies/focus.md#surface-포커스-표현)의 판정을 따른다.
- **변환 버튼** — 칸 가운데의 버튼 하나(`convert_popup.title`). 누르면 이 surface를 대상으로 [surface 변환](../../features/convert-surface/index.md) popup을 연다.

### 상태별 시각

- 빈 자리와 plugin 대기 자리는 같은 변환 버튼 화면을 쓴다.
- 복원 자리는 `EmptySurface`가 아니므로 변환 버튼을 그리지 않는다. 활성화 전 칸의 표시는 이 문서에서 정하지 않으며 실제 실행으로 확인하지 않았다.

### 시각 소스

전용 시안이 없다. 버튼과 배경은 Theme 토큰(`bg_app`, `font_size_body`, `text_primary`)을 쓴다. 작업 영역 타일 배치는 [작업 영역 화면](../../features/work-area/index.md#화면)을 따른다.
