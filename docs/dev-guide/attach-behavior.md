# Attach 메커니즘

## 원칙과 현재 동작

[결과 전달 원칙](../principles/feedback-and-recovery.md)에 따라 원격 요청의 성공·실패를 명확히 전달한다. 아래 Agent 파일 열기 실패·일부 적용 실패를 로그만 남기는 경로는 현재 구현의 미충족이며 계속 유지할 예외로 승인된 것이 아니다.

attach의 서버·클라이언트 처리와 연결·점유·복구 규칙을 설명한다.
사용법과 사용자에게 보이는 동작은 [원격 attach](../features/remote-attach/index.md)를 따른다.
서버는 loopback 연결을 받고, 로컬 직결과 SSH 터널의 선택은 클라이언트가 맡는다.

## GUI client 모듈과 대기 상태

`src/app/attach_client.rs`는 모듈 선언과 외부 호출용 export만 둔다. 하위 모듈은 사용하는 타입과 함수의 소유 모듈을 직접 import하며, 부모를 통한 glob import는 사용하지 않는다.

| 모듈 (`src/app/attach_client/`) | 책임 |
|---|---|
| `attempts.rs` | endpoint·connection 대기 target의 등록, 일회성 인수, 취소, 교체 |
| `pending.rs` | 원 engine·View·선택·mapping·epoch 검증과 handshake/ID 예약 완료 대기 |
| `connection.rs` | mirror 설치·재연결·정리 |
| `output.rs` / `output/apply_event.rs` | 수신 순서와 epoch를 보존하며 창/parked engine에 출력 적용 / 종류별 회신 분배 |
| `projection.rs` | 예약한 논리 ID로 descriptor 트리 구성; Engine 대여 없음 |
| `navigation.rs` | 살아남은 remote/local ID로 선택 위치 복원; 실행 자원 접근 없음 |
| `survivors.rs` | 서버 descriptor와 기존 자원을 비교해 재사용·교체·회수 |
| `resources.rs` | 터미널·종류 인스턴스 생성, 입력 binding, 플러그인 통지 |
| `forward.rs` | 원 target에 묶인 구조·입력·조회 요청 전달 |
| `bulk.rs` | 캡처·bulk 전송과 원 connection epoch 검사 |
| `wire.rs` | control payload 해석·인코딩; App/Engine 접근 없음 |
| `html.rs` | opt-in HTML 요청 dispatch·응답 적용·문서 수명 정리; native viewer는 별도 capability |
| `user_waits.rs` | 사용자 요청 접수와 대기: 구조 요청의 View draft 등록과 journal 접수, mirror surface로 보낸 사용자 입력의 확인 receipt, 원격 구조 결과·Delta 적용 뒤 run·draft 정산과 포커스 대상 결정 |

내부 파서·트리 구성 보조 함수·출력 적용 함수는 해당 모듈에서만 보인다. 회귀 테스트도
소유 모듈의 `tests.rs` 아래에 두며, 테스트를 위해 제품 함수의 공개 범위를 넓히지 않는다.
루트 `tests.rs`는 테스트 전용 fixture만 제공한다.

ID 임대는 `IdReservations`와 waker만, 터미널 생성은 `TerminalStore`와 출력 관측 여부만
받는다. fallback 설치는 surface 저장소만, markdown kind 확인·생성은 종류 레지스트리만
받는다. 통지 정책은 toast callback만 받아 Engine에 접근하지 않는다.
연결 설치·출력 적용·survivor 교체는 구조와 여러 자원을 함께 변경하므로 `EngineMut`를
유지한다. 그 대여는 개별 파서나 자원 하나만 다루는 보조 함수로 전달하지 않는다.


AppState는 `MirrorAttempts` 하나를 보유하며 내부 맵을 직접 변경하지 않는다.
연결 교체·취소는 대기 target을 제거하고 Remote의 해당 ticket/token만 취소한다.
원격 완료는 원 token으로 한 번만 인수할 수 있고, 이미 취소된 token의 늦은 완료는
새 시도에 적용되지 않는다. 연결·터널·worker의 실제 수명과 회수는 `Remote`가 소유한다.
대기 객체의 target은 자원 소유권을 복제하지 않는다.
mirror publication의 허용 진입점은 `connection.rs`의 설치·재설치·제거와
`output.rs`의 구조 delta 적용으로 한정하며, 구조 writer 검사는 파일과 함수의 쌍으로 이를 대조한다.


## 서버 / 클라이언트 계층 (가장 먼저 읽을 것)

attach 는 **server**(피점유 — PTY/grid 소유)와 **client**(점유 — mirror 표시) 두 쪽이다.

- **서버측** (`src/remote/server.rs`, IPC `attach.*`) — **transport 를 모른다.** 항상 `127.0.0.1` 로만 client 를 받는다. 로컬에서 붙든 SSH 터널 너머에서 붙든 서버 입장엔 전부 loopback 이다. 서버는 SSH 를 전혀 모른다.
  - 연결마다의 push sink·입력 프레임 분류·bulk 연결 결속은 `StreamHub`(`crates/tasty-ipc/src/stream_hub.rs`)가 든다. sink 는 채널이라 허브도 TCP 를 모른다 — 소켓을 읽고 쓰는 accept 스레드는 본체 adapter `src/adapters/production/tcp_ipc_server.rs` 에 있다. root Remote adapter가 원 등록 binding을 확인해 허브에 전달한다. 순수 tasty-core은 StreamHub를 소유하거나 호출하지 않는다([아키텍처 개요](../architecture/index.md)).
  - 점유된 workspace의 디렉터리·Git·markdown 조회와 응답 직렬화 예산은 `src/remote/server/content_queries.rs`가 담당한다. 호출자는 기존 `server` handler 경로를 사용한다.
- **클라이언트측** — "원격성" 을 전부 흡수한다. 두 종류:
  - **로컬 CLI client**: 포트 파일(`~/.tasty/tasty.port`)을 읽어 그 loopback 포트로 직결. **release CLI 에서 제거 → debug 전용**(`tasty debug attach`).
  - **loopback 직결 대상 지정**: GUI 원격 연결 팝업·IPC `remote.attach`·자동 attach 는 대상이 `127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT` 면 SSH 없이 그 포트로 직결한다(`tasty_remote::browse::resolve_endpoint`, `src/app/auto_attach.rs`). release 에도 있으며, 같은 머신의 다른 인스턴스를 mirror 할 수 있다. 자기 인스턴스 포트는 아래 self-attach 거절을 따른다.
  - **원격 client**: `ssh -L 127.0.0.1:<localport>:127.0.0.1:<remoteport> -N` 터널 후 그 **localport 로 직결**. 터널은 바이트 파이프라 스트림 프로토콜에 투명 — 원격 client 도 결국 자기 머신 loopback 에 붙는다(`tasty remote attach --ssh|--profile`).

### "로컬 attach 제거" 의 정확한 의미

> **attach 는 원격을 대상으로 한다** — 로컬 attach 의 CLI 진입점은 release 에서 제거하고 debug 격리하며, 자기 인스턴스(self) attach 는 거절한다. 같은 머신 다른 인스턴스로의 loopback 직결은 위 GUI·IPC·자동 attach 경로에 남는다. 이 결정의 *근거·대안·재검토 조건* 은 [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md). 아래는 그 결정이 구현에 어떻게 드러나는지다.

원격 attach 도 **서버 입장엔 loopback** 이다. 따라서 release 에서 "로컬 attach 제거" 는 **서버를 바꾼 게 아니라 client 의 로컬 진입점(`tasty attach` → `tasty debug attach`)만 제거**한 것이다. 서버의 attach 수신 경로는 로컬/원격 공용으로 보존된다. SSH 터널 + attach 세션 머신(`run_attach_*`)은 `crates/tasty-cli/src/local/attach.rs` 에 공용으로 남고, `remote`/`debug` 네임스페이스는 그 위에서 디스패치만 한다.

### `remote` / `debug` 는 CLI 디스패치 네임스페이스 (IPC 와 비대칭)

`remote attach`/`remote check`/`debug attach`는 CLI 명령 이름이다.
이 이름을 그대로 IPC 네임스페이스로 사용하지 않는다. CLI는 `attach.*`와 `system.info`를
이용하며, 원격 연결 여부와 debug 전용 진입점을 명령별로 구분한다.

## 점유 레지스트리 (`OccupancyRegistry`)

`src/core/attach.rs`. 휘발성(직렬화/복원 안 함 — 재시작 시 빈 registry → 전부 free).

이 registry 는 이 문서 의 **약한/강한(soft/hard) 2계층 점유**를 함께 담는다. **attach 는 강한(hard) 점유의 한 사례** — 아래는 그 hard 경로다. 약한(soft) 점유(advisory 마커, write 미차단; 현 소비자 = `terminal` 명령의 child-terminal)는 [`features/child-terminal`](../features/child-terminal/index.md) 를 보라. 두 계층은 같은 registry 에 있지만 별도 저장이다(hard=`surface_locks`/`workspace_locks`, soft=별도 soft 엔트리).

- `surface_locks: HashMap<SurfaceId, AttachLock>` — surface 단위 배타(hard) lock. `acquire` 가 동시 점유를 `AlreadyAttached{holder}` 로 거부.
- `workspace_locks` + `surface_to_workspace` — workspace 단위 점유. workspace 점유 시 멤버 *터미널* 은 `surface_locks` 에도 동일 holder 로 등록(서버측 placeholder 렌더·입력차단을 surface 단위와 동일 적용). 비-터미널은 역매핑(`surface_to_workspace`)으로만 "점유 표시".
- `release` (holder 본인) / `force_detach` (서버 권한 — 로컬 GUI·내부 정리 전용, 아래 force-detach 절) / `release_all_for_client`(연결 생존 판정 실패 시 일괄 — workspace + 멤버 + 잔여 surface). 트리거는 4 종: **self-release**(holder 의 명시적 release) · **force-detach**(로컬 사용자 강제 해제) · **EOF-or-TTL**(연결 종료 EOF 또는 attach heartbeat TTL 만료 — 둘 다 `tcp_ipc_server.rs` read 루프(`run_stream_read_loop`)의 오류 분기를 거쳐 `StreamInbound::Disconnected` → `release_all_for_client` 로 합류한다; TTL 은 소켓 read timeout 이 heartbeat 미수신으로 만료되는 경우로, 실제 EOF 와 같은 경로로 연결을 끝낸다. 원인 분류는 로그에만 쓰인다([끊김 원인 로그](#끊김-원인-로그))) · **forwarded-op에 따른 workspace 제거**. 원 holder의 요청은 `app/journal/commands/inbound.rs`에서 admission·구조 commit·필요한 resource 완료를 거친다. `deliver_result`는 원 연결 binding을 확인하고 결과를 먼저 보낸 뒤, workspace가 사라졌으면 `force_detach_workspace`로 점유를 정리한다. 남아 있으면 delta를 보내고 성공한 publication 뒤 terminal tap을 연결한다. forward 외의 committed 변경도 `remote/structure_sync.rs::push_structure_changes`에서 사라진 workspace를 강제 분리한다. 옛 `close_case_workspace`/`execute_forwarded_structural_op`의 동기 실행 순서를 현재 경로로 사용하지 않는다.
- **끊긴 holder 는 재attach 를 막지 못한다** (이 문서). inbound 한 배치(`PumpOutcome`)의 적용 순서는 attach 연결이 먼저, 연결 종료 *정리*가 마지막이다 — 끊긴 client 의 잔여 입력 프레임이 그 client 의 점유가 살아 있는 동안 적용돼야 하기 때문이다. 그래서 한 배치에 "C1 끊김" 과 "C2 attach" 가 함께 실리면 C2 가 곧 사라질 C1 의 lock 에 막힌다. 이를 막기 위해 두 pump(`App::apply_stream_outcome` · `boot::headless_stream::apply`)가 배치 **머리**에서 `mark_clients_disconnected` 로 *사실만* 먼저 알리고, `acquire`/`acquire_workspace` 는 자기를 막고 선 holder 가 그 표시를 가지면 그 자리에서 점유를 회수한다. 회수는 경쟁이 있을 때만 하므로 경쟁이 없는 잔여 입력은 그대로 처리된다. 표시는 `release_all_for_client` 가 lock 과 함께 지워 한 배치를 넘지 않는다.
- **입력 격리**: `apply_send_to_surface` 와 terminal broadcast·respawn이 함께 쓰는 `TerminalInputAccess::send` 가 `is_hard_occupied` 면 서버 로컬 입력 거부(tell의 `write_then_submit` 도 같은 검사를 한다), client 입력만 `feed_attached_input` 우회 경로로 PTY 도달. (soft 점유는 write 를 막지 않는다 — hard 만 격리.)
- **점유는 핸드셰이크가 검증된 뒤에만 잡힌다** (이 문서). 점유를 잡는 유일한 진입점은 `dispatch_stream_attach` → `attach_workspace_for_stream`/`attach_surface_for_stream` 인데, 그 **앞에** `tcp_ipc_server.rs::validate_stream_proto` 가 있다. `stream.open` params 의 `proto` 가 `STREAM_PROTO` 와 다르면(생략 시 serde default `0`) attach 를 dispatch 하지 않고 `StreamAck{ok:false, proto, error}` 로 거절한다 — 점유가 애초에 잡히지 않는다. 없을 때의 문제: 프로토콜이 안 맞는 client 는 그 점유를 **쓸 수 없는데도** 가져가고, 소켓을 닫지 않는 구버전/hung peer 면 아래 EOF 가 오지 않아 heartbeat 제한 시간까지 그 workspace 가 붙잡혀 정상 attach 가 `already_attached` 로 거절됐다. 거절 ack 는 client(`StreamConnection::open_with`)가 이미 검사하는 형식이라 실패 사유가 그대로 사용자에게 전달된다.
- **self-attach는 client에서 거절한다**: GUI 원격 연결 팝업(`queue_browser_mirror`), IPC `attach.into_gui`(`dispatch_pending_gui_attach`), IPC `remote.attach`와 자동 attach(`auto_attach.rs::handle_auto_attach_connected` → `queue_endpoint_mirror`)는 모두 `attach_client/dispatch.rs::dispatch_attach`를 거친다. 그 안의 `connect_unless_self`가 요청 포트를 자기 IPC 포트와 비교한다(debug/release 공통). SSH 터널 경로의 요청 포트는 터널의 로컬 포트라 자기 IPC 포트와 겹치지 않으므로, SSH 경로는 터널을 연 워커에서 `tasty_remote::self_instance::refuse_this_instance`가 상대 `system.info`의 `instance_id`(`tasty_ipc::instance::instance_id`)를 비교해 `ThisInstance` 오류로 끝낸다. 호출 위치는 `remote.attach` 워커 두 곳(`app_methods/remote.rs`의 `resolve_existing_endpoint`·`prepare_create_endpoint`), 자동 attach의 `resolve_endpoint_bound_with`, GUI 팝업의 `browser.rs::list_endpoint`다. 네 함수는 엔드포인트 해석을 인자로 받으므로, 각 모듈의 단위 시험이 SSH 없이 해석 결과(터널 여부·포트)와 가짜 `system.info` 상대를 넣어 터널이면 판정을 묻고 같은 `instance_id`면 거절하는지, 직결이면 묻지 않는지 확인한다. 실제 해석 함수를 그 자리에 넘기는지는 시험하지 않는다. 메인 루프는 `ThisInstance`를 `record_self_refusal(.., via_ssh: true)`로 기록한다. `tasty debug attach`는 별도 CLI 프로세스가 서버 포트에 직접 연결하므로 이 검사를 거치지 않는다. 이 검사는 GUI가 원격 대상을 선택한다는 정책이며, 현재 handshake는 `queue_mirror_connection`을 통해 Remote worker가 수행한다. 서버는 정상 SSH 터널과 self 연결을 loopback 주소만으로 구별할 수 없다. 로컬 검증은 별도 프로세스의 `tasty debug attach`를 사용한다.
  IPC `attach.into_gui`와 loopback 대상의 `remote.attach`는 이 검사보다 앞서 처리 시점에 `ipc/handler/attach.rs::refuse_own_port`로 거절한다. `remote.attach`는 `app_methods/remote.rs`가 시도를 시작하기 전에 판정한다. `attach.into_gui`의 인자 해석과 자기 포트 판정은 `attach.rs::into_gui_target` 하나다. GUI는 App 층(`attach_client/into_gui.rs`, 외부 IPC는 `ipc_step_app_methods`, plugin host-call은 `process_plugin_ipc_calls`)이 이를 부른 뒤, 다른 포트는 워커가 그 포트의 `system.info`(세션 토큰 없음)를 물어 `instance_id`가 같으면 `-32602`(`... reaches this instance itself ...`)로, 아니면 메인 루프가 routing과 같은 규칙으로 고른 engine의 큐에 넣고 `{queued:true}`로 답한다. 판정을 워커에 두는 이유는 이 프로세스의 `system.info`를 메인 루프가 답하기 때문이다 — 메인 루프에서 물으면 자기 자신일 때 응답을 기다리며 멈춘다. 상대가 답하지 않거나 `instance_id`가 없으면 판정 없이 큐에 넣는다. 연결을 받고 답하지 않는 상대는 `browse::PROBE_TIMEOUT`(5초)까지 기다리므로 응답도 최대 그만큼 늦어진다. 워커의 판정은 `catch_unwind`로 감싸, 패닉하면 응답 없이 끝나지 않고 internal_error로 답한다. 헤드리스에는 큐를 처리해 mirror를 만드는 쪽이 없으므로 engine 처리기에 arm을 두지 않고, 자기 포트를 포함한 모든 요청을 `-32017`(메서드 이름 포함)로 거절한다. 자기 포트 판정 함수들도 GUI 빌드에만 있다. App 층이 처리하므로 멱등 키 선언은 `.kept_by_app_layer()`(버전 2)다. 자기 포트는 IPC 서버 시작 뒤 `AppServices::own_ipc_port`에 주입한 값이다. 그래서 `new_workspace:true`가 원격(=자기)에 workspace를 만들기 전에 끝난다. 아래 기록 경로는 주로 자동 attach가 쓴다.
  엔드포인트 경로(IPC `remote.attach`·자동 attach)의 거절은 `auto_attach.rs::record_self_refusal`이 `Remote.refusals`(`tasty_remote::refusal`)에 남기고 IPC `remote.refusals`가 읽는다. 첫 attach의 거절은 anchor를 `pending_reactivation`에 넣어 다음 프레임의 재트리거를 막고, 인라인 매핑은 `Refusals::holds`로 매핑이 바뀔 때까지 후보에서 뺀다. 재연결 중의 거절은 `on_reconnect_attempt_failed`로 백오프를 기록한다.
  같은 `Refusals`가 anchor별 안내(`MappingNotice`: 이유·대상·매핑)도 보관한다. 자기 자신 거절은 `record_self_refusal`이, 첫 attach의 해석 실패는 `apply_auto_attach_outcome`이 남긴다. 없는 프로필은 `resolve_endpoint`가 `auto_attach/notice.rs::ProfileNotFound`로 돌려줘 `profile_missing`으로 구별하고, 나머지 해석 실패는 `unresolved`다. `poll_auto_attach` 끝의 `sync_mapping_notices`가 창과 parked engine의 지금 매핑과 다른 안내를 지우고, 안내 세대가 바뀐 창의 `MainViewState.attach_notices`에 넘긴다. pending mirror 설치 성공이 그 anchor의 안내를 지운다. 행 표지와 Workspace 배너, ×의 활성화 단위 숨김은 창 쪽(`adapters/ui/attach_notice.rs`)이 맡는다.

## 조회 팝업과 연결 시도의 소유

원격 workspace 조회·생성의 socket, SSH tunnel, attempt worker는 `tasty-remote::Remote`가 소유한다. 팝업은 목록과 요청 ID만 보관한다. App은 Browse 요청이 생성된 View identity와 EngineId를 고정하고, Connect에서 같은 원본을 확인한 뒤 PendingMirrorInstall로 전달한다. 연결 결과로 표시를 바꿀 때도 원 View와 selection generation을 확인한다.

자동 attach의 SSH probe는 원 AttemptToken의 취소 scope에서 실행한다. 원 engine·workspace mapping이 사라지면 진행 중 시도를 취소하며, 늦게 받은 결과로 현재 연결을 덮지 않는다. 조회 결과 큐가 가득 차면 worker가 원 결과를 보유해 기다리되 취소 시 빠져나온다. 준비된 attach 연결은 실제 I/O retirement receipt를 App이 수집하도록 별도 bounded 결과 큐로 전달한다.

UI 대기 기한과 worker 종료는 별개다. 기한 만료는 취소 요청이며, Remote의 종료 관측은 실제 thread join 및 연결 retirement receipt를 사용한다. 원 raw 입력을 새 connection epoch에 재전송하지 않는다.

원격 프로필 감지는 AppServices의 `ProfileDetections`가 GUI와 IPC에서 함께 실행한다. 최대 8개 probe worker를 소유하고, 완료 후 시작 때의 프로필과 현재 항목이 같을 때만 감지 결과를 저장한다. 삭제·편집된 항목과 무관한 다른 프로필을 시작 snapshot으로 덮어쓰지 않는다. GUI의 프로필·Passkey 저장과 파일 읽기는 `remote_tool_files`가 실행하며, 비밀 표시 응답은 원 View와 팝업 identity에만 적용한다.

SSH의 `request_cancel`은 취소 flag만 설정한다. 실제 자식 kill/reap는 probe worker가 수행하고, 기존 동기 `cancel` API와 구별한다. worker가 끝나기 전에는 종료 완료로 표시하지 않는다.

이미지 업로드 worker는 Remote의 attempt 예산과 실제 join 관측에 포함된다. bulk socket은 handshake 전에 원 attempt에 등록하며 종료 취소가 그 socket만 닫는다. 업로드 결과를 붙여넣을 때는 원 connection epoch·remote/local mapping·terminal physical generation과 원 View identity를 함께 확인한다.

OS 화면 캡처는 App의 ScreenshotWorkers가 최대4개 worker를 소유한다. 현재 OS API에 취소 handle이 없으므로 종료 때 신규 요청을 막고 실제 완료한 worker만 join한다. 남은 수는 미완으로 보고하며 원 engine/View가 사라진 결과를 새 창이나 클립보드에 적용하지 않는다.

연결된 SSH tunnel은 `ClientTransport`가 내려갈 때 기존 ConnectionWorkers 회수 작업으로 넘긴다. 조회 취소·늦은 endpoint 결과·연결 admission 거절도 Remote가 별도 retirement receipt로 tunnel을 넘긴다. App 스레드에서는 tunnel의 blocking Drop을 실행하지 않으며, child wait 결과를 확인한 뒤에만 회수 receipt를 완료한다. 미완 retirement가 예산에 도달하면 새 연결 시도를 거절한다. CLI의 동기 SshTunnel Drop 계약은 유지한다.

서버의 attach 출력 구독은 RemoteState가 receiver를 소유하고 정상 engine pump에서 전달한다. 한 pump는 모든 구독을 합쳐128 events/약1MiB를 처리하고 남은 일은 기존 waker로 예약한다(마지막 단일 event는 바이트 예산을 넘길 수 있다). 별도 tap forwarding thread를 만들지 않는다. 구독은 terminal physical generation·점유 granted_seq·StreamHub registration에 묶이고, 서로 다른 원본이면 receiver만 버린다. 이 정리는 PTY 입력이나 parser를 종료하지 않는다.

`Terminal::snapshot_and_stream`은 snapshot·행/열·ordered Output/Resize receiver를 같은 parser lock에서 만든다. 구독당 256개 event/1MiB를 넘으면 이미 큐에 든 prefix 뒤 Loss 한 번을 전달하고 종료한다. Remote는 원 registration에 Loss를 통지하고 그 연결만 끊어 새 snapshot으로 재동기화한다. 느린 소비자가 parser를 블로킹하지 않으며 GUI 없는 서버도 같은 primitive를 쓴다. 기존 snapshot_and_tap API는 유지한다.

mesh context도 원 granted_seq·StreamHub registration·activation에 묶는다. holder나 grant가 바뀌면 이전 raw 입력과 texture/frame dedup 상태를 버린다. 누적 입력은1024 events/1MiB로 제한하고 초과 시 원 연결을 명시적으로 끊는다. window의 로컬 geometry와 parked/headless의 context pump 역할은 유지한다.

bulk·capture 수신 내용은 연결별 owned 임시 파일에 순서대로 기록하고 commit에서 목적 파일로 복사한다. 파일 전체를 Vec에 누적하지 않으며 IPC 큐 바이트 상한을 새 파일 크기 제한으로 사용하지 않는다. bulk의 기존 `remote_transfer.max_mb` 폴더 용량 정책은 유지한다. bulk는 declared total_size와 연속 seq를 검사하고, 잘못된 순서·길이·임시파일 I/O 실패는 명시 실패와 원 연결 종료로 처리한다. 동시에 열어 둔 미완 전송의 내부 입장 예산은 종류별256개다.

전송은 시작 때의 StreamHub registration과 workspace holder/granted_seq에 묶인다. commit 시 그 관계가 사라졌으면 이전 전송을 현재 holder의 성공으로 저장하지 않는다. 취소·TTL·disconnect·engine 소멸은 owned 임시 파일을 지운다. 프로세스가 강제 종료되면 OS 임시 폴더에 파일이 남을 수 있으며, 이 경로는 재시작 복원용 영속 전송으로 취급하지 않는다.

## 초기 스냅샷 + delta

attach 직후 서버가 현재 visible 화면을 `snapshot_and_stream` 으로 구독 등록과 같은 lock 안에서 **1회** 직렬화 push(셀 속성 + 현재 펜 + 커서 + alt-screen/DECCKM/bracketed 모드 복원. vertical_align과 하이퍼링크는 칸과 펜 모두 제외). 이후 변화는 output tap delta(Data 프레임). client 는 받은 바이트를 PTY 없는 mirror 터미널(`Terminal::new_detached` + `feed_bytes`)에 먹여 같은 termwiz 파서로 grid 재구성.

## workspace mux

한 연결로 N 터미널 출력을 나르므로 workspace 모드 Data 프레임은 **surface-prefixed**(`encode_mux`/`decode_mux`). surface 단위 단일 연결은 prefix 없음. attach 직후 서버가 `attached_workspace` Control 로 트리(분할 방향/비율) + per-surface 디스크립터를 보내고, client 는 원격↔로컬 surface_id 재매핑으로 트리를 재구성한다. client(GUI `crates/tasty-remote/src/transport.rs`, CLI `crates/tasty-cli/src/local/attach.rs`)는 attach 연결의 첫 Control 이 `attached_workspace`(단일 surface 면 `attached`) 또는 `attach_error` 라고 보고, 그 밖의 event 가 먼저 오면 attach 전체를 실패시킨다. 그래서 서버는 descriptor 전에 다른 Control 을 보내지 않는다. 점유는 `attach_activation::begin` 에서 잡히고 terminal 활성화가 끝난 `poll` 에서야 descriptor 가 나가므로, 그 사이의 점유는 준비 안 됨(`AttachLock.ready = false`)이다. Busy tick 의 상태 전송(`busy_activity_forwards`·`attention_forwards`·`surface_cwd_forwards`)은 준비된 lock 만 보는 `ready_locks_snapshot` 으로 후보를 만든다. 준비 전 점유는 후보에서도 전송 캐시에서도 빠지므로, 준비된 뒤 descriptor 다음 Busy tick 에서 점유 터미널마다 busy·attention·cwd 의 그때 값이 최초 값으로 한 번씩 나간다. 순서는 `src/app/attach_activation/order_tests.rs` 가 begin → Busy tick → poll → Busy tick 으로 재현해 본다.

per-surface role 은 5종:

- `{remote_id, role:"terminal", cols, rows}` — mirror 가능한 터미널.
- `{remote_id, role:"mesh", kind, plugin_id, display_name}` — 같은 plugin 소유의 egui-mesh 로 등록된 kind(mesh mirror, 아래 절).
- `{remote_id, role:"explorer", root}` — 활성 탭의 현재 디렉토리만 싣고 목록은 `list_dir` 채널로 lazy 조회([원격 attach (Remote attach)](../features/remote-attach/index.md)).
- `{remote_id, role:"markdown", file, display_name}` — 원문은 안 싣고 `markdown_content` 채널로 lazy 조회(아래 절, [원격 attach (Remote attach)](../features/remote-attach/index.md)). `file` 은 표시·제목 전용 opaque 문자열이라 client 가 그 경로로 자기 로컬 파일을 열지 않는다. 값은 plugin 의 `surface.create` snapshot 에서 오는데 그 도착이 `tab.create` 반환이나 convert 직후의 역반영보다 뒤다(핸드셰이크·역반영은 동기 경로라 host 가 기다리지 않는다). 그래서 snapshot 이 오기 전에는 서버 `RemoteSurface` 가 생성 인자의 `file` 을 대신 알린다(`attach_content_info`). 이 폴백이 없으면 갓 연·갓 변환한 surface 의 `file` 이 빈 문자열로 가고, 원문 조회도 빈 문서로 답해 mirror 가 reload 전까지 빈 문서를 보인다. 생성 인자에 `file` 이 없던 surface 는 여전히 빈 값이며 wire 상 "파일 없이 열린 surface" 와 같다. 원문은 아래 채널로 가져온다.
- `{remote_id, role:"placeholder", kind}` — 그 외 비-터미널, mirror 불가.

이 role 분류는 `build_workspace_tree_surfaces`(`src/remote/server.rs`)가 만든다. mesh 와 markdown 은 **두 단**이다 — `tasty-model` 이 `Surface::attach_mesh_info()`/`attach_content_info()` 로 후보만 모으고(crate 가 앱의 등록부와 허용 목록을 모른다), 앱 계층이 `is_registered_egui_mesh_kind`(등록부 대조)/`is_attach_content_allowed`(원문 전달 허용 목록)로 재검증해 떨어진 후보를 placeholder 로 내린다.

## 입력 처리 확인과 View의 선행 배치

workspace descriptor의 `ipc.stream.input-ack`는 `InputCheckpoint{sequence}`와
`InputProcessed{sequence}`를 지원한다는 뜻이다. 프로토콜 동등 비교값은 바꾸지 않는다.
클라이언트는 사용자 입력을 큐에 넣은 뒤 확인 지점을 보낸다. 연결당 현재 확인과 후속
입력 묶음만 보관하며 후속 묶음의 최초 입력 시각을 유지한다. 서로 다른 surface의 송신 순서는
접수 시각 순서와 다를 수 있으므로 현재·후속 묶음 전체의 최소 접수 시각을 표시한다.
ACK가 확인하는 wire 순서 범위는 시각 정렬로 바꾸지 않는다. 에이전트 쓰기와 터미널의
자동 응답은 사용자 확인을 시작하지 않는다. 실패한 연결의 입력은 새 연결에 재생하지 않는다.

`PumpOutcome.input_events`는 Data·MeshContext·MeshInput·확인 지점을 받은 순서대로
유지한다. GUI·headless의 메인 루프가 그 순서로 전달한 뒤 확인 지점에 답한다. 확인은
PTY writer flush·셸 소비·명령 종료·렌더 완료를 보장하지 않으며, 대상 거절은 기존 오류
경로를 따른다. GUI는 `src/app/attach_input.rs`, headless는 `src/boot/headless_stream.rs`가
이 순서로 적용한다. reader는 유효한 연결의 ACK를 즉시 관측한다. 구조 Delta의 ID 예약이
Pending이라 뒤 이벤트가 보류되어도 입력 확인 수명은 그 보류에 묶이지 않는다.

사용자 생성 요청은 `state::attach_pending`의 View draft에만 선행 배치한다. 별도
PendingId로 식별하며 CoreState·journal·surface 저장소에 임시 도메인 객체를 넣지 않는다.
실제 leaf와 임시 leaf는 같은 분할 기하를 사용한다. 기존 terminal은 대기 중 좁아진 화면에
잘라 그리고, 임시 분할을 이유로 원격 PTY를 resize하지 않는다. 입력 대상 조회는 선택된
임시 영역에서 None을 반환한다. IME는 조합 시작의 선택·임시 선택 세대를 확인한다.

`StructuralResult.created_surface_id`는 서버가 생성 결과에서 알고 있는 정확한 remote ID다.
이 필드를 모르는 peer는 무시하고, 옛 서버는 생략한다. 성공 결과와 Delta의 실제 적용을
모두 확인한 요청만 완료한다. 생성 ID가 없으면 요청 종류·원래 anchor의 pane/tab·분할 축과
신규 leaf의 종류를 대조하며, 그 범위에서 요청과 생성 객체가 하나로 대응할 때만 연결한다.
무관한 영역의 Delta나 아직 송신하지 않은 요청은 이 연결의 근거가 아니다. 합쳐진 스냅샷은
실제로 겹칠 수 있는 송신된 임시 영역만 접는다. 어느 요청의 생성인지 모호하면 확정 구조를
보이고 요청 확인은 남기며 임의의 surface를 선택하거나 완료로 처리하지 않는다. 원 View·선택·epoch가 바뀐 결과는
다른 View나 새 연결의 선택을 바꾸지 않는다.

`StructuralResult.history_omitted`는 성공한 원격 사용자 닫기가 닫은 항목 기록에 내용 없이 남긴
terminal 수다([닫힌 항목 복원](../features/closed-tab-restore/index.md#내부-동작)). 서버는 처음 확정한
회신에만 싣고 idempotent replay 회신과 옛 서버는 생략한다. client는 연결 epoch·op_id에 대응하는
아직 정산하지 않은 사용자 요청의 View를 먼저 찾고, 그 View가 현재 창과 같을 때만 경고 toast로
한 번 알린다. 요청한 View가 사라졌거나 바뀌었으면 생략하며, parked engine에도 표시하지 않는다.
원 요청 View 확인은 결과 정산 전에 수행한다. 이 필드를 모르는 옛 client는 무시한다.
`output.rs`는 결과와 Delta를 받은 순서대로 `user_waits.rs`의 정산을 호출하고, 정산이 돌려준
local surface만 포커스한다. 클라이언트의 사용자 입력 확인은 입력이 실제로 surface에
전달됐을 때만 시작한다.

대기 중인 draft·응답이 있는 workspace의 사이드바 항목은 `src/adapters/ui/attach_loading.rs`가
`loading_overlay_delay`가 지난 뒤에만 로딩 overlay로 덮는다. 지연 전에는 그 시각에 다시 그리도록
예약한다.

## 키 인코딩은 서버 PTY를 따른다

mirror 키 입력은 서버 PTY에 그대로 들어가므로 수식키가 붙은 줄바꿈 키의 인코딩을 클라이언트 OS로 정하면 안 된다. Windows client가 ConPTY용 win32-input 시퀀스를 Linux PTY로 보내면 Claude Code 같은 응용 프로그램이 이를 Enter로 읽어 Shift+Enter가 제출이 된다.

- **서버**: workspace attach descriptor의 `pty_input`에 `win32-console`(Windows) 또는 `vt`(그 밖)를 싣는다(`tasty_ipc::stream::local_pty_input`).
- **client**: handshake에서 읽은 값을 연결의 `ConnectionSender`에 두고, `bind_mirror_input`이 mirror `Terminal::set_win32_console_input`으로 옮긴다. 재연결도 같은 함수를 거쳐 새 연결의 값으로 바뀐다. 키 디스패처는 focused terminal의 이 값으로 Shift+Enter·Ctrl+J 인코딩을 고른다([터미널 키보드 입력](../surfaces/terminal/index.md#키보드-입력)).
- **구 서버**: 키가 없거나 모르는 값이면 `vt`로 읽는다. 구 Windows 서버에 붙은 mirror만 Shift+Enter·Ctrl+J를 VT로 보낸다.
- **한계**: 앱별 Shift+Enter 규칙은 foreground 프로세스 이름으로 찾는데, mirror에는 로컬 PTY가 없고 서버가 이름을 보내지 않아 mirror에서는 적용되지 않는다. 플랫폼 기본 인코딩만 쓴다.
- **시험**: `newline_keys_follow_the_target_console_encoding`(`src/view/main/keyboard.rs`), `a_mirror_takes_the_key_encoding_of_its_server`(`src/app/attach_client/resources.rs`), `a_descriptor_without_pty_input_reads_as_vt`(`crates/tasty-ipc/src/stream.rs`), `workspace_descriptor_announces_the_resize_ack_capability`(`src/remote/server/attach_resize.rs`).

## 프레임 전송 지연 (Nagle 금지)

attach 스트림은 **프레임 하나 = 상호작용 하나**(키 입력 · 리사이즈 요청 · 구조 op · 출력 delta)다. 그래서 프레임 단위 전송 지연이 그대로 체감 지연이 된다 — 본문·헤더를 나눠 쓰거나 Nagle을 켜면 전송 지연이 늘 수 있다.

- **헤더와 payload를 한 버퍼로 보낸다** — `tasty_ipc::stream::write_frame` 이 `[tag][len][payload]` 를 한 버퍼에 합쳐 `write_all` 1 회로 보낸다. 헤더와 payload를 나눠 쓰면 Nagle과 delayed ACK 때문에 전송 지연이 생길 수 있다. write_all 호출 수가 실제 시스템 호출이나 TCP 세그먼트 수를 보장하지는 않는다. `crates/tasty-ipc` 의 유닛 테스트 `stream::tests::write_frame_emits_one_write_call` 이 시험 writer의 호출 횟수를 확인한다.
- **양쪽 소켓 모두 `TCP_NODELAY`** — 서버 `prepare_stream`(`src/adapters/production/tcp_ipc_server.rs`, 일반 JSON-RPC 연결 포함)과 client `StreamConnection::open_with`(`crates/tasty-ipc/src/client/stream.rs`). 위에서 합쳐도 payload 가 MSS 를 넘으면 마지막 조각이 다시 Nagle 에 걸리므로 이중 방어다.
- 호스트 ↔ plugin 메인 채널도 같은 두 가지를 한다 — 한 줄을 한 번의 write 로(`tasty_plugin_protocol::write_line`), 양 끝 `TCP_NODELAY`. [`plugin-development.md` "전송 지연"](plugin-development.md#전송-지연-nagle-금지).

**SSH 터널은 이 지연을 흡수해 주지 않는다.** 터널 너머든 아니든 tasty 소켓의 양 끝은 항상 loopback이고, 위 지연은 그 loopback 구간에서 발생한다. 그리고 mirror 를 다시 mirror 하는 다단 구성(A → B → C)에서는 홉마다 입력·출력 양방향으로 얹히므로 지연이 홉 수에 비례해 누적된다.

## 밀어내기 실패와 누적 손실

`StreamHub::push`는 client 때문에 host를 멈추지 않는다. 큐가 가득 차면 프레임을 버리고
Dropped를 반환하며, 연속 실패가 LAG_LIMIT을 넘으면 연결을 끊는다.
호출자가 Dropped 뒤 다음 데이터를 계속 보낼 수 있으므로 연결이 살아 있어도 출력 중간이 빠질 수 있다.

| 값 | 의미 | 외부 조회 |
|---|---|---|
| `lag` | 연결별 연속 drop. 데이터 전송 성공 시 0 | 내부 연결 해제 판단 |
| `frames_dropped` | 살아 있는 연결에서 버린 프레임 누계 | system.pressure.stream_push |
| `clients_lagged_out` | 지연 한도로 끊은 연결 누계 | system.pressure.stream_push |
| `backlog` | 살아 있는 연결의 큐에 아직 남은 프레임 합 | system.pressure.stream_push |

backlog는 연결별 queued 카운터로 계산한다. push 성공에 증가하고 SinkReceiver가 꺼내면 감소한다.
끊긴 연결은 합에서 제외한다. sink_capacity와 함께 읽으면 현재 적체를 해석할 수 있다.
누계는 어느 client의 어느 출력이 빠졌는지를 알려주지 않으므로 별도 Loss 통지를 사용한다.

### client 에게 공백을 알린다 (`Loss` / `client_loss_notify`)

서버는 `ipc.stream.loss-notify` capability를 알린다. client가 `ClientLossNotify {}`를 선언하면
직전 통지 뒤 이 연결에서 버린 프레임 수를 `Loss { frames }`로 보낸다.
GUI mirror와 CLI surface/workspace dump·raw bridge가 선언한다. attach가 아닌 debug echo 스트림은 선언하지 않는다.
선언하지 않은 peer에는 Loss를 보내지 않는다. STREAM_PROTO는 정확히 같은 버전만 연결하므로
기능 추가만으로 번호를 올려 구 peer를 거절하지 않는다.

큐가 가득 차 통지도 넣을 수 없으면 `pending_loss`에 수를 보관한다.
`SinkReceiver`가 한 프레임을 꺼내는 즉시 통지를 넣어 마지막 보존 데이터 뒤에 도착하게 한다.
성공한 때만 pending 수를 지운다. push 앞과 pump_inbound 끝에서도 시도하므로
손실 뒤 늦게 선언한 client가 이미 큐를 비운 경우도 처리한다.

평소 write 스레드는 원자 `owes_notice`만 읽으며 통지가 있을 때만 sink map을 잠근다.
선언·drop·통지 성공 시 원자 사본을 갱신한다. 수신 측은 map의 Weak를 사용하며 sender를 소유하지 않는다.
따라서 registry에서 연결을 지우면 write 스레드도 채널 종료를 알 수 있다.

Loss를 보냈다고 lag를 초기화하지 않는다. 서버가 만든 통지 성공은 client가 데이터를 따라잡았다는 뜻이 아니다.
이 때문에 push 한 번당 한 칸만 비우는 client는 통지가 그 공간을 차지해 데이터 전송에 계속 실패할 수 있다.
두 칸 이상을 비우는 경우와 구분해 부하를 검증한다.

알려진 경합도 있다. push가 통지를 넣지 못한 직후 write 스레드가 공간을 만들면 데이터 한 장이 Loss보다
먼저 들어갈 가능성이 있다. 과거 실행 실험에서는 재현하지 못했다. 실제 앞지르기 관측이나 원자적 삽입 수단이
생기면 이를 다시 검토한다. 손실 client는 아래 재attach로 화면을 다시 받는다.

### 통지를 받은 client 가 하는 일 (재동기화 계약)

Loss는 surface나 데이터 종류를 담지 않는다. 한 연결이 여러 종류를 운반하면 가장 강한 복구 방법을 적용한다.

| 종류 | 복구 |
|---|---|
| PTY delta | 새 snapshot을 받기 전에는 불연속 출력을 정상 화면으로 간주하지 않음 |
| Resize·Activity·Attention·Cwd·구조 상태 | 새로운 값을 받을 때까지 오래된 상태로 표시 |
| mesh | 조립기와 캐시를 버리고 full texture부터 다시 구독 |
| bulk | 결과 불명인 중단으로 처리; 이미 commit됐을 수 있어 자동 재시도하지 않음 |

snapshot과 output tap을 같은 순간에 확보하는 경로가 attach이므로 재attach를 사용한다.
Detach를 보낸 뒤 이전 연결의 EOF를 확인하고 새로 연결한다.
서버는 Disconnected를 inbound에 넣은 뒤 소켓을 닫아 이전 점유 정리가 새 요청보다 먼저 처리되게 한다.
raw bridge는 서버가 HEARTBEAT_TIMEOUT 안에 닫지 않으면 대기를 끝내고 연결을 다시 시도한다.

- GUI mirror는 손실을 표시하고 한 세션에서 재attach를 한 번에 하나만 진행한다.
  진행 중 추가 통지는 합산하며, 완료 뒤 다시 손실이 나면 재attach할 수 있다.
  기존 reconnect_session이 survivor ID와 scrollback을 유지하며 mesh frame·구독 중복 방지 캐시는 초기화한다.
  살아남은 terminal mirror의 grid는 새 descriptor의 서버 크기로 맞춘다(아래 "리사이즈 전파").
  실패하면 anchor가 있는 세션은 Reconnecting, 나머지는 정리한다.
- parked engine은 stream ID와 오래됨 표시만 갱신하고 연결은 유지한다. 창을 복원하면
  `resume_resync_in_window`가 재attach를 시작한다. AttachClientData 또는 3초 AttachView가 이 처리를 실행한다.
  이 호출 연결은 기존 단위 테스트만으로 검증되지 않는다. 실제로 park→손실→창 복원을 재현해야 한다.
- 창에서 시작한 재attach가 EOF를 기다리는 사이 park되면 anchor 없는 세션이 정리될 수 있다.
  소스에서 확인한 한계이며 실행 재현이 확인된 것은 아니다.
- CLI dump는 최대 세 번 재attach하고 수집을 처음부터 시작한다. 그 뒤 손실은 결과를 출력하면서 stderr로 알린다.
  `--send`는 첫 attach에서만 실행한다. raw bridge는 제한 없이 재attach하고 매번 stderr로 알린다.
  전환 중 일반 stdin은 버릴 수 있지만 EOF와 Ctrl+\는 종료로 처리해야 한다.
- bulk는 Loss를 받으면 Detach 후 중단 오류를 반환한다. 서버 거부를 뜻하는 BULK_REJECT_PREFIX는 붙이지 않으므로
  이미지 업로드 UI의 수동 재시도는 가능하다.

Loss와 재연결 snapshot에서 mirror Terminal의 output stream ID를 바꾼다.
이전 ID를 가진 위치 읽기는 stream 불일치를 받으며, ID 없이 읽는 기존 소비자를 위해 위치 숫자는 이어 센다.
재attach snapshot 때문에 scrollback에 화면 한 벌이 중복될 수 있다.

parked 복구 검증은 두 격리 인스턴스에서 실제로 park 가능한 OS 경로를 사용한다.
client 수신을 늦춰 LAG_LIMIT 미만의 drop을 만든 뒤 복원하고, 지연 복구 로그와 새로운 attach를 확인한다.
Linux·Windows의 일반 최소화는 창을 유지하므로 macOS의 parked 동작을 확인한 것으로 보고하지 않는다.

## 갱신 cadence 분리

- **서버측 readonly 뷰**(피점유 — 대상 부하 절약): **3초 polling**(`Tick::AttachView`, `src/app/attach_poll.rs`)으로 self-snapshot 적용.
- **client mirror**(내가 다루는 대상): 원격 출력이 올 때마다 즉시 갱신, 3초 tick 은 backstop(누락 출력 적용·끊긴 세션 정리)으로만.

hard 점유된 서버 surface의 드래그 선택과 복사는 허용한다.
좌표 변환과 복사 문자열은 EngineRef::visible_terminal이 반환하는 readonly mirror를 사용한다.
live 터미널을 읽으면 사용자가 보고 선택한 내용과 달라질 수 있다.
클릭 트래킹은 None으로 처리해 로컬 선택을 사용한다. 휠은 live scroll_offset이 바뀌는 문제 때문에 차단하고,
링크 실행은 지연된 화면에서 외부 파일·URL을 여는 부수효과 때문에 차단한다.
IME·vi 커서·링크·검색 하이라이트는 표시하지 않는다. soft 점유는 이 제한을 받지 않는다.

## 리사이즈 전파 (mirror geometry)

mirror grid 는 **client 가 구동(client-driven)** 한다(docs/features/remote-attach/index.md) — mirror 를 띄운 **로컬 pane 의 크기**가 grid 를 정하고, 원격 PTY 를 그 크기로 reflow 시킨다. "remote authoritative" 는 **메커니즘으로만 유지**: 원격 PTY 가 실제 크기를 확정하는 주체(reflow 담당)이고 그 settled 크기를 echo 로 되돌린다. 즉 **의도(intent)는 client, 확정(confirm)은 remote** 의 요청→확정 협상이다.

- **client 구동 (forward)**: mirror 는 detached 터미널(PTY 없음)이라, 매 프레임 도는 로컬 레이아웃 리사이즈 스윕(View의 resize 요청 / `AppServices::resize_terminals`)이 detached 터미널을 로컬에 적용하는 대신, 목표 grid `(cols, rows)` 를 `RemoteState.pending_resize_forward`(로컬 surface id → grid)에 넣는다(목표가 이미 현재 mirror grid 면 생략). 이 한 곳에서 걸러 모든 리사이즈 진입점(창 resize·divider drag·단축키·redraw)을 커버한다. `App::dispatch_pending_resize_forwards`(`about_to_wait`, gui)가 drain 해 로컬 id 를 세션 매핑으로 원격 id 로 치환하고 `StreamControl::ClientResize{surface_id, cols, rows}` 를 `Control` 프레임으로 forward 한다. 세션의 last-forwarded dedup(`tasty_remote::resize_sync::ResizeSync`)이 응답 왕복(약 1 RTT) 동안의 매 프레임 재전송을 억제한다(coalesce). 응답 보장과 재시도는 아래 "크기 요청의 응답과 재시도"를 따른다.
- **서버 적용 (server)**: `StreamHub::pump_inbound` 이 `ClientResize` 를 `PumpOutcome.resize_requests` 로 분류 → 메인루프(gui `event_handler`/headless `boot`)가 anchor 워크스페이스의 **holder 를 검증**(hard 점유 = geometry 구동 권한, docs/dev-guide/attach-behavior.md)한 뒤 `EngineMut::apply_attached_workspace_resize` 로 원격 **실제 PTY** 를 `Terminal::resize` 한다(reflow).
- **로컬 grid 는 echo 로만 갱신 (desync 방지)**: client 는 목표 크기를 로컬 mirror grid 에 **낙관적으로 먼저 적용하지 않는다**. 원격 reflow 전 잘못된 grid 에 바이트가 재생되는 desync 를 막기 위해, mirror grid 는 아래 `Resize` echo 가 도착할 때만 바뀐다.
- **재attach 는 살아남은 mirror 를 서버 확정 크기로 맞춘다**: 재연결 설치는 kind 가 그대로인 terminal mirror 를 재사용하지만, 그 grid 를 새 연결 descriptor 의 `cols`/`rows`(서버 grid)로 맞춘 뒤 새 연결의 프레임을 적용한다. 이 descriptor 는 새 snapshot 과 같은 cut 의 크기라 snapshot 이 맞는 grid 에 그려진다. 끊긴 연결과 함께 echo 가 사라져 mirror 와 서버 grid 가 달라졌어도, 다음 리사이즈 스윕이 목표를 새 mirror grid 와 비교해 `ClientResize` 를 다시 보낸다(재연결 때 last-forwarded dedup 도 비운다). 크기 필드가 없는 descriptor 는 맞추지 않는다. 구조 delta 의 descriptor 크기는 출력과 같은 순서의 값이 아니므로 delta 적용은 살아남은 mirror 의 크기를 바꾸지 않는다.
- **원격→client 확정 echo**: terminal parser lock 아래 실제 grid 변경과 `AttachEvent::Resize` 등록을 순서대로 수행한다. `snapshot_and_stream`의 snapshot·구독 등록은 같은 lock cut이며, 출력과 resize는 하나의 bounded ordered stream을 따른다. Engine의 `RemoteState`가 receiver를 소유하고 `remote/subscription.rs::poll_attach_subscriptions`가 원 terminal generation·holder grant·StreamHub binding을 확인한 뒤 `StreamControl::Resize`를 보낸다. 별도 attach forwarder 스레드는 없다. client는 remote→local 매핑으로 해당 mirror를 갱신한다. overflow의 `Loss`는 연결 종료와 새 snapshot 복구로 이어지며, 이전 raw 출력을 재생하지 않는다.
- **순서 보존**: client reader 는 Data(출력)와 Resize 를 **한 버퍼에 도착 순서대로**(`MirrorEvent`) 쌓고, 메인 스레드가 순서대로 적용한다 — resize 앞뒤 출력이 올바른 그리드에서 재생되도록.

### 크기 요청의 응답과 재시도

`ipc.stream.resize-ack` 를 알리는 서버는 모든 `ClientResize` 에 응답한다. client 는 이 응답으로 요청이 처리됐는지 판단한다.

| 서버가 받은 요청 | 응답 | 순서 |
|---|---|---|
| holder 의 요청이고 grid 크기가 바뀐다 | `Resize`(바뀐 크기) | attach tap — 출력과 같은 순서 |
| holder 의 요청이고 grid 가 이미 그 크기다 | `Resize`(현재 크기) | attach tap — 출력과 같은 순서 |
| 요청한 연결이 그 workspace 의 holder 가 아니거나 대상이 terminal 이 아니다 | `ResizeRejected`(요청 값) | 크기를 바꾸지 않으므로 허브로 바로 |
| 어느 engine 의 점유에도 그 surface 가 없다 | `ResizeRejected`(요청 값) | 위와 같음 |

- **서버 구현**: `EngineMut::apply_attached_workspace_resize`(`src/remote/server/attach_resize.rs`)가 `Answered`·`NotHere`·`Rejected` 를 돌려준다. 같은 크기 확인은 `Terminal::resize_or_confirm_attach` 가 parser lock 아래에서 현재 크기를 tap 에 넣는다. 회신 규칙은 `finish_resize_request` 한 곳에 있다 — 요청을 받은 engine 의 결과가 `Answered` 일 때만 회신하지 않고(tap 몫), 그 밖(받은 engine 의 거절, 어느 engine 도 받지 않음)은 거절을 회신한다. GUI 루프는 `answer_resize_on_engines` 로 창·parked engine 을 차례로 묻고 첫 수락 engine 에서 멈추며, headless 루프(`apply_mirror_state`)는 engine 이 하나라 그 결과를 그대로 넘긴다. 시험: `attach_resize.rs` 의 `only_an_answered_request_goes_without_a_rejection`·`the_gui_loop_asks_engines_in_order_and_rejects_what_none_took`, `boot/headless_stream.rs` 의 `the_headless_loop_rejects_only_requests_its_engine_did_not_take`(헤드리스 조합에서만 돈다).
- **응답이 보장하는 것**: grid 크기가 확정됐다는 것이다. OS PTY 크기 알림은 그 뒤 예약되며(`Pty::schedule_resize`), 그 적용 실패는 로그만 남기고 응답·재시도·배너에 반영하지 않는다.
- **연결별 확인**: client 는 `system.info` 를 조회하지 않는다. workspace attach descriptor 의 `stream_capabilities` 목록에 `ipc.stream.resize-ack` 가 있을 때만 응답을 기다린다. 목록이 없는 구 서버는 같은 크기·거절에 응답하지 않으므로 기다리지 않는다(dedup 만 동작). 구 client 는 `ResizeRejected` 를 역직렬화하지 못해 무시하고, 같은 크기의 추가 `Resize` 는 이미 그 크기라 결과가 같다.
- **대기와 짝짓기**: client 는 원격 surface 마다 마지막 요청 하나만 기다린다. 기다리는 요청과 크기가 같은 `Resize`·`ResizeRejected` 만 대기를 끝낸다. 연속 리사이즈에서 늦게 온 이전 요청의 응답은 크기가 달라 최신 대기를 풀지 않는다. 요청 식별자는 쓰지 않는다. 서버는 요청 크기를 그대로 적용하므로 같은 크기의 응답이 그 요청의 응답이다.
- **짝짓기의 한계 — 같은 크기로 돌아가는 요청**: A→B→A 를 빠르게 보내면(디바이더를 앞뒤로 끄는 경우) 첫 A 의 응답이 마지막 A 의 대기를 끝낸다. 그 뒤 서버가 B 까지만 처리하고 멈추면 mirror grid 는 B, 목표는 A 인데 dedup 이 A 를 기억해 다시 보내지 않고 대기도 없어 실패로 오르지 않는다. 데이터는 손상되지 않으며, 서버가 마지막 A 를 처리하면 맞춰지고 `Loss`·재연결이면 dedup 이 비워져 복구된다. 남는 영향은 "멈춘 서버 + ABA" 에서 배너가 뜨지 않는 것뿐이다. 아래 늦은 확정도 같은 성격이라 오래된 같은 크기 응답이 실패를 풀 수 있다. 같은 크기의 오래된 응답이 최신 요청을 확정하는 문제가 실제 사용에서 반복되거나 서버가 요청 크기를 바꿔 적용하게 되면, 크기 비교 대신 요청 세대 번호로 짝짓는 계약을 검토한다.
- **마감과 재시도**: 마감은 요청 후 5 초(`RESIZE_ACK_TIMEOUT`)다. 첫 요청이 마감까지 응답이 없거나 거절되면, 마감에 같은 요청을 한 번 다시 보내고 다시 5 초를 잰다. 거절을 받아도 바로 다시 보내지 않고 마감까지 기다린다. 다시 보낸 요청도 실패하면(마감 경과 또는 거절 즉시) 그 surface 를 실패 목록에 올리고 warn 로그를 남긴다. 자동 재시도는 한 번뿐이다. 응답 없는 서버에서는 첫 요청부터 약 10 초 뒤 실패로 판정된다.
- **배너**: 실패 목록과 다시 시도 중인 surface 가 있으면 그 mirror 워크스페이스에 `attach-size-sync` Workspace 배너를 띄운다. App 이 `about_to_wait` 마다 세션별 목록을 로컬 surface ID 로 바꿔 각 창의 `AttachSizeSyncs` 에 넘기고(`sync_attach_size_sync_banners`, 바뀐 창만 다시 그림), overlay 가 활성 워크스페이스가 그 mirror 일 때 탭 제목으로 이름을 구해 배너를 맞춘다. 분할한 탭처럼 같은 이름이 여럿이면 배너는 이름을 한 번만 보이고 뒤에 ` ×k` 를 붙인다. 수(`N surfaces`)는 surface 수이고 `+n` 은 앞 두 항목 뒤에 남은 항목 수다. 모두 다시 시도는 이름과 관계없이 모든 surface 에 적용한다. 버튼은 `Intent::AttachSizeSync` 로 App 에 돌아와 그 세션에 적용된다(`apply_attach_size_sync_action`).
- **수동 재시도**: 배너의 다시 시도는 실패 목록의 요청을 다시 보내고 5 초를 잰다. 기다리는 동안 버튼은 비활성이고 앞에 Spinner 가 돈다. 자동 재시도 없이 판정하며, 실패하면 다시 목록에 올라 배너가 다시 시도 가능한 상태로 돌아온다. 성공한 `Resize` 는 그 surface 를 목록에서 빼고, 목록이 비면 배너가 사라진다.
- **닫기**: 배너의 × 는 실패 목록만 비운다. 진행 중인 다시 시도는 계속 기다리되 배너에서는 빠진다. 이후 새로 실패하면(그 다시 시도의 실패 포함) 배너가 다시 뜬다.
- **늦은 확정**: 기다리는 요청이 없는 실패 surface 에 실패한 크기와 같은 `Resize` 가 오면 목록에서 뺀다. 멈췄던 서버가 밀린 요청을 실패 판정 뒤에 처리한 경우다. 다른 크기의 `Resize` 는 실패를 풀지 않는다.
- **타이머**: 모든 세션의 가장 이른 마감 하나로 `Tick::AttachResizeAck` 를 건다(`sync_attach_resize_timer`). 루프가 깨면 `App::poll_attach_resize_sync` 가 마감을 처리한다. surface 마다 타이머를 따로 두지 않는다. 재연결·layout flush 처럼 파생 마감을 `arm_derived` 로 다시 거는 관례를 따르며, 정밀도(Strict)는 같고 surface 별 타이머 키를 정리할 일이 없다. 3 초 `Tick::AttachView` 는 5 초를 정확히 재지 못해 쓰지 않는다.
- **정리**: `Loss` 통지는 응답이 빠졌을 수 있어 대기와 dedup 을 버린다(실패 목록은 유지). 연결이 끊긴 세션은 다음 처리에서 대기를 버린다. 재연결 설치(`install_reconnected_survivors`)는 survivor 병합 → terminal mirror 를 descriptor 의 서버 크기로 맞춤 → 대기·dedup·실패 목록 초기화를 이 순서로 한다. 그래서 다음 리사이즈 스윕이 새 mirror grid 와 목표를 비교해 요청을 다시 보내고 그 응답을 새 연결에서 기다린다. 구조 delta 뒤에는 terminal mirror 로 남은 surface 의 상태만 유지한다(닫힘·kind 변경 정리). 세션이 사라지면 상태도 함께 사라진다.
- **재시도 수단 — 재전송**: 재시도는 같은 `ClientResize` 의 재전송이며 재attach 를 쓰지 않는다. surface 단위 재attach 는 없고(이미 workspace 점유에 속한 surface 의 surface attach 는 `AlreadyAttached` 로 거절된다), 있는 재attach 는 workspace 연결 전체를 다시 붙여 점유 공백·scrollback 중복·mesh full texture 재수신을 부른다. 결정 근거는 [원격 attach (Remote attach)](../features/remote-attach/index.md).
- **`StreamControl` 확장성**: `event` 태그 기반 enum. mid-session 이벤트는 새 `StreamTag` 없이 variant 로 추가한다 — 현재 `Resize`(server→client, 확정 echo), `Activity`(server→client, busy/idle 활동 상태), `Cwd`(server→client, surface cwd — 아래 "surface cwd 전파" 절), `Foreground`(server→client, 전경 프로세스 이름 — 아래 "전경 이름 전파" 절), `ClientResize`(client→server, geometry 구동 요청), `ResizeRejected`(server→client, 적용하지 않은 크기 요청의 거절 — 위 "크기 요청의 응답과 재시도"), `ClientAttentionClear`(client→server, 주의 환기 해제 edge), `StructuralOp`(client→server, 구조 변경 forward), `StructuralResult`(server→client, forward 회신), `StructuralDelta`(server→client, 구조 역반영), `MeshContext`(client→server, mesh 구독+geometry/theme/focus — 아래 "mesh mirror 채널" 절), `MeshInput`(client→server, 누적 입력), `MeshFullResendRequest`(client→server, 텍스처 delta 체인 복구 요청), `MeshError`(server→client, mesh 단발 실패 통지), `Attention`(server→client, 주의 환기), `BulkBegin`/`BulkCommit`(client→server, bulk 전송)·`BulkResult`(server→client), `Loss`(server→client, 이 연결에서 버린 프레임 수 — 위 "client 에게 공백을 알린다"), `ClientLossNotify`(client→server, 그 통지를 받겠다는 선언). 알 수 없는 event 는 역직렬화 실패로 무시(전방/후방 호환 — **다만 그 무시가 공짜일 때만 안전하다**: `Loss` 는 못 읽으면 "손실 없음" 으로 읽히므로 위 선언으로 게이트한다) — 구버전 서버는 `ClientResize` 를 무시해 기존 remote-authoritative 로 graceful degrade 한다. 단발 핸드셰이크 디스크립터(`attached`/`attached_workspace`/`attach_error`)와 `force_detached` 는 여전히 ad-hoc JSON 으로 위치 기반 파싱.

## 활동(busy) 상태 전파

사이드바 워크스페이스 리스트의 "실행 중" status dot과 개수 숫자(`WorkspaceEntryView.busy_count`, `docs/features/remote-attach/index.md` "GUI mirror" 참조)은 surface 의 busy/idle 상태를 본다. mirror(detached) 터미널은 로컬 PTY 가 없어 `EngineMut::refresh_busy_surfaces`(foreground-process 폴링, `src/runtime/terminal_activity.rs`)가 절대 채울 수 없으므로, **원격이 직접 자기 surface 의 busy 상태를 계산해 client 로 forward** 한다 — resize 의 client-driven 협상과 반대로, busy 는 순수 **server→client** 단방향이다(원격 foreground 프로세스 이름은 애초에 client 에 존재하지 않는 정보라 client 가 요청할 수도 없다).

- **서버측 계산·forward**: 서버는 1Hz `Tick::Busy`(gui/headless 공통 — `crates/tasty-timer` 의 중앙 타이머 허브가 발화, [`timer-hub.md`](timer-hub.md) 참조)마다 `EngineMut::forward_busy_activity`(`remote/server.rs`)를 호출한다. 이 메서드는 `busy_activity_forwards`(`runtime/terminal_activity.rs`)가 계산한 **점유 중인 surface 의 busy 값 변화분**만 `StreamControl::Activity{surface_id, busy}` 로 그 workspace/surface 의 holder client 에 push 한다. `last_forwarded_busy` 캐시로 값이 실제로 바뀐 경우에만 forward(중복 억제)하되, surface 가 점유 해제됐다가 재점유되면(다른 client 일 수 있음) 캐시를 버려 값이 이전과 같아도 항상 fresh 하게 1회 다시 push한다. 캐시는 **(holder, 값)** 을 기억한다 — 점유 해제 엔트리를 버리는 것은 점유 공백이 tick 경계를 넘을 때만 효과가 있고, 해제와 다른 client 의 획득이 한 tick 창 안에 끝나면 엔트리가 살아남으므로 holder 가 달라진 것을 그 자체로 변화로 센다(cwd push 와 같은 규칙) — resize 의 `last_forwarded_resize` dedup 과 동형이나 방향이 반대(client→server 아닌 server→client)다.
- **headless 는 별도 ticker 스레드가 필요 없다**: headless 메인 루프(`boot::run_headless`)가 `rx.recv_timeout(next_deadline)` 로 직접 타이머 허브의 다음 데드라인까지만 대기하다 `Tick::Busy` 를 스스로 드레인한다 — gui 전용 스레드나 이벤트 브리지에 의존하지 않는다([`timer-hub.md`](timer-hub.md) "headless — `run_headless`" 절 참조). **원격 attach 의 주 시나리오가 headless 서버**이므로 이 tick 이 없으면 활동 상태 forward 가 전혀 동작하지 않는다.
- **client 적용**: reader 스레드가 `Activity` Control 프레임을 `MirrorEvent::Activity(remote_surface_id, busy)` 로 버퍼링하고, `apply_attach_client_output` 이 세션의 `remote_to_local` 매핑으로 로컬 mirror surface id 를 찾아 `EngineMut::set_mirror_surface_busy` 를 호출한다. 이 값은 **`busy_surfaces`(로컬 폴링 결과)와 분리된 `mirror_busy_surfaces` 별도 집합**에 저장된다 — 같은 집합에 합쳤다면 1Hz `refresh_busy_surfaces` 가 매 tick 로컬 폴링 결과로 집합을 통째로 교체하며 mirror 값을 지워버렸을 것이다. `is_surface_busy`/`any_busy`/`busy_count`(사이드바·상태바·탭 바 busy dot·`surface.list` IPC 가 공유하는 단일 진입점)는 두 집합의 합집합을 본다.
- **정리**: mirror surface 가 없어지면(`cleanup_mirror_workspace`, `apply_mirror_structural_delta` 의 removed 처리) `EngineMut::forget_mirror_surface_busy` 로 `mirror_busy_surfaces` 에서도 제거해, 로컬 id 가 재사용될 때 stale busy 값이 새 surface 에 잘못 붙는 것을 막는다. 사용자가 mirror workspace 를 닫으면 workspace 가 먼저 사라져 `remove_mirror_workspace_from_engine` 이 아무것도 지우지 못하므로, 모든 닫기 경로가 지나는 `EngineMut::cleanup_surface_traced` 도 닫힌 surface 의 busy·cwd·attention·mesh frame 항목을 함께 지운다. 반대로 원격이 닫아 delta 로 사라진 surface 와 teardown 으로 정리되는 surface 는 이 두 경로가 `OccupancyRegistry::forget_closed_surface` 로 soft 점유 등 로컬 점유 기록도 지운다(로컬 닫기 정리와 같은 함수).

## surface cwd 전파

mirror 터미널은 로컬 PTY가 없어 `TerminalStore::cwd`에서 OS PID 조회로 보완할 수 없다 — 원격 셸이 OSC 7 을 방출해 출력 바이트에 실려 올 때만 cwd 를 안다. 서버는 PTY 를 소유하므로 OSC 7 이 없어도 OS 조회로 안다. 그래서 busy·attention 과 같은 형태로 **서버가 cwd 를 push** 한다(server→client 단방향, [원격 attach (Remote attach)](../features/remote-attach/index.md)).

- **서버측 계산·forward**: 같은 1Hz `Tick::Busy` 에서 `EngineMut::forward_surface_cwd`(`remote/server.rs`)가 `surface_cwd_forwards`(`core/state/surface_cwd.rs`)의 계산 결과를 `StreamControl::Cwd{surface_id, cwd}` 로 holder 에 push 한다. 값은 서버 **자기 트리** 기준의 `EngineRef::surface_cwd` — terminal 은 OSC 7 캐시 → PTY 프로세스의 OS cwd, 그 외 kind 는 `source_cwd()`(explorer root · markdown 파일 부모) — 이고 **모든 kind** 가 대상이다. `inherit_cwd` 설정은 보지 않는다(관측이지 실행이 아니다). 연결 지점은 busy/attention 과 같은 3 곳(`src/app/busy.rs` 의 main window·parked engine, `src/boot.rs` 의 headless).
- **diff 캐시는 (holder, 값)**: `last_forwarded_cwd` 는 값과 함께 holder 를 기억한다. 값만 기억하면 같은 tick 창 안에서 점유가 풀리고 다른 client 가 잡았을 때 엔트리가 점유 해제 `retain` 을 살아남아 새 holder 가 초기값을 못 받는다. 초기 push 는 값이 `None` 이어도 나간다.
- **값 소멸은 `cwd: null`**: 원격도 cwd 를 모르게 되면 `null` 이 나가고 client 는 엔트리를 지운다 — 표현이 없으면 옛 원격 경로가 영구히 남는다.
- **client 적용**: `MirrorEvent::Cwd(remote_surface_id, cwd)` → 세션 `remote_to_local` 치환 → `EngineMut::set_mirror_surface_cwd`. 값은 `Terminal` 캐시가 아니라 **별도 맵 `mirror_surface_cwd`** 에 `RemoteCwd` 로 저장되고, `EngineRef::surface_cwd` 가 mirror surface 에 대해 이 값을 우선한다. 원격 경로라 로컬 실행 자리로는 나가지 않는다([surface-cwd §3-2](../design/policies/cwd.md#3-2-원격-출처-cwd-는-로컬-실행-경로로-새지-않는다)).
- **정리**: busy 와 같은 teardown 두 곳(`remove_mirror_workspace_from_engine` · delta 의 removed 처리)에 더해, delta 의 survivor **kind 전환 전부**에서 `forget_mirror_surface_cwd` 를 부른다 — busy 는 terminal 에서 출발한 전환만 정리하지만 cwd 는 비-terminal kind 에도 있다.
- **구버전 호환**: 구버전 client 는 `Cwd` 를 파싱 실패로 무시하고, 구버전 server 는 보내지 않아 맵이 빈다 — 그때 mirror convert 는 종전대로 서버측 convert 요청 해석(`app/journal/commands/inbound.rs::resolve_remote_request`, `inherit_cwd` 설정에 따른 원 surface cwd 조회)으로 동작한다.

## 전경 이름 전파

mirror 터미널은 로컬 PTY 가 없어 1Hz `refresh_busy_surfaces` 가 전경 프로세스 이름을 얻지 못한다. 이름을 쓰는 기능(앱별 Shift+Enter 규칙, 상태바 프로그램 이름, 마우스 캡처 목록과 배너 더보기 메뉴, 프로그램이 바뀔 때 배너 닫기, 자식 생존 판정의 셸 복귀, 클릭 커서 이동, `surface.foreground_process`)이 mirror 에서도 로컬과 같게 동작하도록 **서버가 이름을 push** 한다(server→client 단방향).

- **서버측 계산·forward**: 같은 1Hz `Tick::Busy` 에서 `EngineMut::forward_surface_foreground`(`remote/server.rs`)가 `surface_foreground_forwards`(`core/state/surface_foreground.rs`)의 결과를 `StreamControl::Foreground{surface_id, name}` 로 holder 에 push 한다. 값은 서버의 `EngineRef::foreground_name` 이다. 연결 지점은 busy·attention·cwd 와 같은 3 곳(`src/app/busy.rs` 의 main window·parked engine, `src/boot.rs` 의 headless)이고, 준비 전 점유는 후보에서 빠져 descriptor 를 앞지르지 않는다(`src/app/attach_activation/order_tests.rs`).
- **diff 캐시는 (holder, 값)**: `last_forwarded_foreground` 는 cwd 와 같은 규칙이다. 초기 push 는 값이 `None` 이어도 나간다.
- **값 소멸은 `name: null`**: 서버가 이름을 모르게 되면(프로그램이 끝나 셸 조회가 실패하는 경우 포함) `null` 이 나가고 client 는 이름과 세대를 지운다. 표현이 없으면 Claude Code 가 끝난 뒤에도 옛 이름이 남아 셸에서 LF 가 나간다.
- **client 적용**: `MirrorEvent::Foreground(remote_surface_id, name)` → 세션 `remote_to_local` 치환 → `EngineMut::set_mirror_surface_foreground`. 값은 `live.foreground_names` 가 아니라 **별도 저장소 `mirror_foreground`** 에 둔다 — 1Hz 폴링이 `live.foreground_names` 를 통째로 교체하기 때문이다. 수신 버퍼 예산(`MirrorEvent::retained_bytes`)은 이름 문자열도 센다.
- **읽기**: `EngineRef`·`EngineMut`·`EngineRead` 의 `foreground_name` 과 `foreground_generation` 은 로컬 값이 없으면 mirror 값을 돌려준다. 그래서 호출처는 mirror 여부로 분기하지 않는다. 규칙과 목록은 **client 의 설정**으로 판단한다. 인코딩 주체와 설정 화면이 client 의 것이고, 이름 정규화(경로·대소문자·`.exe`)가 원격 OS 차이를 흡수한다.
- **세대**: mirror 이름이 바뀌면 그 저장소의 세대가 오르고 `None` 이면 세대도 지운다(로컬 폴링이 이름을 잃을 때와 같다). 마우스 캡처 배너는 이 세대로 원격 프로그램 변경을 감지해 닫힌다.
- **캡처 제외 목록**: `refresh_busy_surfaces` 가 `mouse_capture_disabled_surfaces` 를 교체할 때 mirror 이름도 함께 판정한다. 따로 계산하지 않으면 다음 tick 에 mirror 판정이 지워진다.
- **PTY 를 직접 조회하던 곳**: 클릭 커서 이동은 로컬에서 PTY 를 직접 조회하고, mirror 에서는 1Hz 캐시 값을 쓴다(`EngineRead::foreground_process_name_now`). `surface.foreground_process` 는 mirror 에 대해 받은 이름과 이름으로 판정한 `is_shell` 을 답하고, 이 머신의 프로세스가 아닌 원격 PID 는 `null` 이다.
- **자식 생존 판정**: 원격 자식의 mirror 도 이름이 셸이면 `Stale`/`Confirmed` 가 된다. 첫 값을 받기 전에는 이름이 없어 이 근거를 쓰지 않는다. holder 연결이 끊긴 동안에는 받은 이름이 남아 있어도 `terminal.children`·`terminal.state` 가 원격 자식을 `unknown` 으로 덮는다.
- **정리**: cwd 와 같은 지점에서 지운다 — `remove_mirror_workspace_from_engine`, delta 의 removed 처리, survivor 의 terminal 에서 출발한 kind 전환, 모든 닫기 경로의 `cleanup_surface_observations`. 재연결 설치(`install_reconnected_survivors`, 재동기화의 재attach 포함)는 살아남은 surface 의 이름을 지우고 세대를 올린다. 새 연결의 첫 값 전(구버전 서버면 계속)에는 이름 미상이고, 옛 연결에서 연 캡처 배너는 닫힌다. 같은 연결의 구조 delta 병합은 서버가 같은 값을 다시 보내지 않으므로 살아남은 terminal 의 이름을 유지한다.
- **구버전 호환**: 구버전 client 는 `Foreground` 를 파싱 실패로 무시한다. 구버전 server 는 보내지 않아 저장소가 비고, 이름을 쓰는 기능은 지금처럼 이름 미상으로 동작한다(규칙 미일치, 메뉴 비활성 형태).

## 주의 환기(attention) 전파

attention의 생성·해제·표시 규칙은 [주의 환기 기능](../features/surface-highlight/index.md)에 모아 둔다.
원격 전달은 다음 순서로 처리한다.

1. 서버의 `forward_attention`이 1Hz `Tick::Busy`에서 `attention_forwards`의 변화분을 읽는다.
   main·parked·headless 모두 실행한다. 캐시는 holder와 kind를 함께 비교한다.
2. `Attention { surface_id, kind }`를 해당 holder에 보낸다. 원격 ID와 completion/needs_input 또는 null을 사용한다.
   host 타입과 wire 타입은 `AttentionKind::to_wire`/`from_wire`로 변환한다.
3. client reader가 MirrorEvent에 넣고, 적용할 engine을 찾은 뒤 원격 ID를 로컬 ID로 바꾼다.
   `set_mirror_surface_attention`이 기존 AttentionStore에 적용하므로 같은 테두리·탭·배지가 사용된다.
4. mirror의 실제 사용자 확인은 레코드를 제거한 경우에만 `pending_attention_clear_forward`에 넣는다.
   `dispatch_pending_attention_clear_forwards`가 원격 ID로 바꾸어 ClientAttentionClear를 보낸다.
5. server의 `apply_attached_attention_clear`는 surface의 workspace holder인지 검증하고 기본 clear 함수를 부른다.
   서버 로컬 GUI에 적용되는 hard 점유 검사를 이 함수에 다시 넣지 않는다.

서버 push와 mirror 삭제는 로컬 raise/clear 함수를 사용하지 않는다.
raise에는 mirror 차단이 있고 clear에는 서버로 전달할 메시지 생성이 있기 때문이다.
삭제 시에는 `forget_mirror_surface_attention`으로 정리한다. kind 변환은 같은 surface이므로 레코드를 유지한다.

차분 전송은 같은 값의 재전송을 보장하지 않는다. send가 실패해도 다음 tick의 값이 같으면 다시 보내지 않는다.
손실은 이 문서의 Loss·재attach 규칙으로 처리한다. 새 surface의 null 초기값이 매핑보다 먼저 도착해
버려져도 양쪽에 레코드가 없으므로 결과는 같다. 신규 attach는 매핑을 동기 구성한 뒤 reader를 시작한다.

## mesh mirror 채널

egui-mesh surface(번들 image/mesh_demo 와 커스텀 plugin 의 egui-mesh kind — 서버 등록부에서 같은 plugin 소유의 egui-mesh 로 확인된 것. markdown 은 WebView 본문을 사용해 대상이 아니다 — attach 시 mesh 가 아니라 아래 "markdown content 채널" 로 mirror 된다)가 mirror pane 에 뜨면, 원격의 실제 plugin 프로세스가 그리는 GPU mesh 프레임을 client 로 스트리밍해 렌더하고 client 입력을 원격으로 forward 한다. 개념·기능 범위는 [features/remote-attach](../features/remote-attach/index.md#surface-단위-vs-workspace-단위), 헤드리스 부트스트랩·로컬 egui-mesh 파이프라인 자체는 [egui-mesh-channel.md](egui-mesh-channel.md#attach-mesh-mirror-소비-경로), 여기엔 attach 프로토콜 전송 경로만 설명한다.

- **구독 = `MeshContext` (별도 핸드셰이크 없음)**: client 가 mesh surface 를 그리기 시작하면 `StreamControl::MeshContext{surface_id, width_px, height_px, pixels_per_point, theme, focused}` 를 보내는 것 자체가 구독 신호를 겸한다 — capability negotiation 을 위한 별도 확인/ack 프레임이 없다. 서버 `MeshMirrorRegistry::upsert`(`src/remote/mesh_mirror.rs`)가 이 정보를 최초 수신 시점에 등록하고, 이후 값이 실제로 바뀔 때만(geometry/theme/focus 변경 시) client 가 재전송한다(`forward_attach_mesh_context`, `src/view/main/attach_mesh_input.rs`) — 매 프레임 재전송하지 않는다.
- **`MeshInput` 누적**: 포인터/키/스크롤/IME 이벤트는 `AttachMeshForwardState.events`(client, dedup 없이 순서 보존)에 프레임마다 쌓였다가, 다음 redraw 의 `forward_attach_mesh_context` 호출에서 `RawInputWire{modifiers, events, ..}` 로 묶여 `MeshInput{surface_id, input}` 1회 전송된다. 서버는 `MeshMirrorRegistry::push_input` 이 `pending_events` 에 extend 하고 `last_modifiers` 를 갱신 + `dirty=true` 로 표시 — 구독이 없는 surface_id 면 `false` 를 반환해 서버 dispatch 가 `MeshError` 로 회신한다(아래).
- **frame 소비·forward (headless-as-server / gui parked engine)**: 실제 구동/relay 로직은
  `plugin_bridge::mesh_forward::forward_mesh_frames_for_engine`(`src/plugin_bridge/mesh_forward.rs`,
  gui/headless 공용)에 있다 — `mesh_mirror.take_dirty`/`take_pending_events`/`last_modifiers`
  를 읽어 로컬 `PluginManager` 에 `SurfaceSetContextParams.raw_input` 으로 흘려보내고, plugin
  이 그린 결과 mesh 프레임을 `StreamTag::MeshData` 청크로 client 에 push 한다(청크 분할
  이유: 텍스처 델타 포함 paint frame 이 `MAX_FRAME_LEN` 을 초과할 수 있음). 이 함수는 **로컬
  authoritative render loop(살아있는 window)가 없는 engine 전용** — 두 소비처가 있다:
  헤드리스 부트스트랩(`src/boot/headless_plugins.rs::pump_plugins` 가 매 tick 호출)과, gui
  인스턴스에서 macOS 최소화로 window 가 파괴되고 `App.engines` 에서 parked 관계가 된
  engine(`App::about_to_wait`, `src/app/event_handler.rs` 가 매 tick parked engine
  전부를 순회하며 호출 — `window_lifecycle.rs` 의 창 복원이 `unpark_first` 로 1개씩만
  꺼내므로, 여러 window 가 동시에 최소화돼 있어도 나머지는 계속 이 순회 대상으로 남는다).
  client reader 스레드는 `tasty_ipc::mesh_stream::MeshFrameAssembler` 로 청크를 재조립해
  `MirrorEvent::Mesh(remote_surface_id, generation, frame_seq, full_textures, bytes)` 를
  메인 스레드 버퍼에 쌓고, `AttachMeshFrameStore::update`(`src/remote/mesh_frames.rs`)에
  저장된 것을 GPU 렌더 경로(`render_attach_mesh_surfaces`, `src/gfx/gpu/egui_mesh_prepare.rs`)
  가 소비해 화면에 그린다.
- **frame 소비·forward (gui-as-server, 살아있는 window)**: gui 인스턴스가 attach 서버이고
  그 mesh surface 가 실제로 window 를 가진 경우는 `MainView::forward_mesh_to_attach_subscribers`
  (`src/view/main/egui_mesh.rs`, 매 redraw 마다 `forward_egui_mesh_context` 직후 호출)가
  대칭 역할을 한다. 다만 로컬 창의 `forward_egui_mesh_context` 가 화면에 보이는 mesh surface
  의 `set_context` 를 이미 매 프레임 권위 있게 구동하므로, 이 훅은 위 `forward_mesh_frames_for_engine`
  전체가 아니라 그 꼬리 로직(`plugin_bridge::mesh_forward::relay_mesh_frame_if_new`)만
  재사용해 별도 `set_context` 를 보내지 않고 **이미 만들어진 `EguiMeshFrame` 바이트를 읽어
  relay** 만 한다(로컬 authoritative loop 와 경합 회피 — mesh_mirror ctx 의 width_px/height_px
  는 attach client 가 요청한 값이라 로컬 렌더 해상도와 다를 수 있다). 예외는 attach 구독
  대상이 로컬 어디에서도 렌더되지 않는 surface(다른 탭/워크스페이스에 있어 로컬 target
  목록에 전혀 없음)인 경우뿐 — 이땐 plugin 이 그 surface_id 자체를 모르므로 경합 없이 이
  훅이 최소 `surface.create` + `set_context` bootstrap 을 1회 대신 보낸다(`find_egui_mesh_surface`
  로 메타데이터 조회). 이미 렌더 중인 surface 에 새 구독이 들어와 전체 텍스처가 필요하면
  (신규 구독/명시 재전송), 직접 보내지 않고 로컬 `MeshForwardState::pending_full` 메커니즘에
  위임해 다음 tick 의 authoritative loop 가 `need_full_textures` 를 실어 보내게 한다 —
  그동안(next tick 까지) 이 훅은 캐시된(델타뿐일 수 있는) frame 을 새 구독자에 흘리지 않고
  건너뛴다.
- **`MeshFullResendRequest` 복구**: 텍스처 델타 체인이 깨졌다고 판단되면(로컬 `EguiMeshRenderTarget` 의 generation 검증 실패 등) client 가 `attach_mesh_full_requests`(`GpuState`)에 surface_id 를 쌓고, `App::dispatch_pending_mesh_full_resend_forwards`가 `MeshFullResendRequest{surface_id}` 를 서버로 forward 한다. 서버는 이를 받으면 `MeshMirrorRegistry::request_full_resend` 로 해당 surface 의 다음 프레임을 풀 텍스처 포함(full_textures=true)으로 강제한다. 서버는 표시만 세우므로 같은 요청을 여러 번 받아도 풀 프레임은 한 번 나가고, 재요청은 진행 중인 요청을 취소하지 않는다. 그래서 client 는 단절을 발견하면 바로 한 번 보내고, 풀 프레임을 받지 못한 동안에는 0.5초에서 시작해 두 배씩, 최대 5초 간격으로만 다시 보낸다(`src/gfx/gpu/egui_mesh_prepare/full_request.rs`). 간격이 차지 않은 동안에는 요청도 redraw 표시도 만들지 않고 그 시각에 다시 그리도록 예약한다. 매 프레임 다시 보내면 바쁜 서버에 요청이 쌓여 서버 회차를 잡아먹고, client 도 응답을 기다리며 계속 다시 그린다. 풀 프레임을 받으면 간격은 처음으로 돌아간다. 로컬 plugin 경로는 응답이 latest-wins 버퍼에서 덮일 수 있어 지금처럼 매 tick 다시 요청한다([egui-mesh-channel](egui-mesh-channel.md)).
- **`MeshError` — 명시적 단발 실패**: 구독 안 된 surface 로의 `MeshInput`(홀더 불일치 포함, `EngineMut::apply_attached_mesh_input` 의 holder 검증) 등은 조용히 drop 하지 않고 `MeshError{surface_id, reason}` 를 1회 회신한다(설계 결정: 조용한 drop 보다 명시적 실패가 디버깅에 유리). client 측 소비는 현재 로그 레벨 처리만(재시도/toast 없음) — 세션이 정상이면 애초에 발생하지 않는 방어적 경로.
- **View/App 실행 경계를 건너는 forward 요청**: `MainView`는 `EngineRead`로 표시 값을 읽고 입력·geometry를 모은 뒤 원 `SurfaceBinding`을 담은 프레임 효과 `InputEffect::RemoteMesh`/`DrawEffect::RemoteMeshFull`을 큐에 넣는다. App의 프레임 효과 적용이 binding을 확인해 `RemoteState.pending_mesh_*`를 갱신한다. View가 실행 큐나 socket writer를 직접 빌리지 않는다. 이후 `App::about_to_wait`의 `dispatch_pending_mesh_*_forwards`가 owning 세션의 remote ID로 치환해 전송한다. resize도 App의 `resize_terminals` 실행에서 forward 큐에 넣고 같은 세션 경계를 따른다.
- **gui-as-server 의 살아있는 window 는 attach client 의 입력 역방향 forward 를 아직 로컬 plugin 에 되먹이지 않는다**: 위 "gui-as-server, 살아있는 window" 훅은 mesh 바이트 forward(서버→client)만 구현한다 — `MeshInput` 으로 도착해 `MeshMirrorRegistry::push_input`/`pending_events` 에 쌓인 attach client 의 클릭/키 입력을 로컬 plugin 의 `raw_input` 에 병합하는 연결은 `forward_mesh_frames_for_engine`(`take_pending_events` 소비)에만 있다 — gui 살아있는 window 는 이 함수를 쓰지 않으므로(경합 회피, 위 참조) 후속 작업으로 남는다. **예외**: gui parked engine 은 headless 와 동일하게 `forward_mesh_frames_for_engine` 을 그대로 쓰므로, window 가 최소화돼 있는 동안은 오히려 입력 forward 가 이미 동작한다 — window 복원 후 살아있는 window 로 전환되면 다시 이 제약이 적용된다.
- **surface 디스크립터의 display_name**: `build_workspace_tree_surfaces` 가 보내는 mesh 디스크립터는 `{remote_id, role:"mesh", kind, plugin_id, display_name}` — `Surface::attach_mesh_info()`(kind/plugin_id 만 반환)와 별개로, 이미 존재하는 `Surface::display_name()`(`EguiMeshSurface` 는 실제 파일명 등을 반환)도 함께 조회해 실어보낸다. client 의 `MirrorMeshInfo` 구성(`merge_survivor_mapping`)은 이 필드를 우선 쓰고, 필드가 없는 경우(구버전 서버 등)에만 `kind` 문자열로 fallback한다 — 과거엔 이 필드 자체가 없어 탭 타이틀이 항상 mesh kind(예: `"image"`)로 뭉뚱그려졌다(image/mesh_demo 전부 동일 증상 — 당시엔 markdown 도 이 경로를 탔으나 현재 WebView를 사용해 mesh 디스크립터 대상이 아니다).

## markdown content 채널

markdown surface 는 webview kind 라 plugin 에 egui-mesh paint 채널이 없다([WebView 호스트 계약](../design/systems/webview.md)). 그래서 mirror 는 렌더 결과가 아니라 **원문 문자열**을 나르고, client 의 markdown plugin 이 자기 테마·자기 recent 로 다시 그린다. 결정의 근거·대안·재검토 조건은 [원격 attach (Remote attach)](../features/remote-attach/index.md), 여기서는 전송 경로를 설명한다.

- **핸드셰이크는 좌표만 싣는다**: `{remote_id, role:"markdown", file, display_name}`. 원문은 트리 디스크립터를 문서 크기만큼 부풀리므로 싣지 않는다 — client 가 필요할 때 아래 채널로 따로 가져온다(lazy).
- **client 의 surface 구성**: `merge_survivor_mapping`(`src/app/attach_client/survivors.rs`)이 `role:"markdown"` leaf 를 `EmptySurface` 대신 registry 의 `markdown` kind 로 만든다 — 조건은 그 kind 가 **번들 plugin(`com.tasty.markdown`)의 것**인가 하나다. kind 가 **아직 등록되지 않았으면**(꺼져 있던 plugin 을 세션 중에 켜는 경우 등) leaf 는 layout 복원과 같은 kind 대기 placeholder(`EmptySurface::new_deferred_plugin`, `deferred_mirror_markdown_surface`)가 되고, 표시 시점의 reify(`EngineMut::reify_deferred_surface` 가 kind 대기 placeholder 에 대해 부르는 `reify_plugin_surface`)가 kind 등록 뒤 registry 의 `restore` 로 실제화한다 — plugin 에는 `surface.restore` 의 data 로 생성 params 와 같은 모양이 가서 plugin 은 어느 경로든 `remote` 키로 mirror 문서임을 안다(탭 제목은 그 경로에 생성 params 가 없어 plugin 이 응답의 `display_name` 으로 돌려준다). 같은 이름을 **다른** plugin 이 이미 등록했으면 대기하지 않고 빈 surface 로 남는다 — reify 는 소유자를 가리지 않아 그 plugin 으로 실제화되기 때문이다. 생성 params 는 `{display_name, remote:{file}}` 이다: 최상위 `file` 이 아니라 `remote.file` 에 싣는 것은 plugin 이 그 경로로 **자기 로컬 파일을 읽지 않게** 하려는 것이다(최상위 `file` 은 로컬 열기·cwd 도출·recent 기록을 모두 켠다). 구조 delta 로 트리가 다시 와도 이미 markdown 이던 leaf 는 `RemoteSurface::share_handles` 로 **같은 plugin surface 를 재사용**한다 — 새로 만들면 plugin 이 문서를 다시 받는다. 트리에서 빠진 leaf·재연결로 사라진 leaf·mirror 정리는 `destroy_mirror_markdown_surfaces`가 manager의 명시 destroy 경로로 통지한다. wrapper의 Rust Drop이나 destroy enqueue 자체는 plugin에서 파괴가 끝났다는 증거가 아니다. 관측이 필요한 resource retirement는 원 instance에 묶인 receipt를 별도로 기다린다. 세션이 든 로컬 markdown id 집합은 `AttachClientSession.markdown_locals` 이고 아래 요청·회신의 인가 대상도 이 집합이다.
- **plugin → host 요청**: markdown plugin 은 `markdown_mirror.content_request {surface_id(로컬), agent_origin?}` 를 호출한다(`agent_origin: true` 는 바깥 호출자의 `markdown.reload` 가 건 요청 표시 — 아래 절단 표시 참조. `src/adapters/ipc/handler/markdown_mirror.rs`, 권한 `fs.read`). host 는 `request_id` 만 즉시 회신하고 `RemoteState.pending_markdown_content_forward` 에 쌓는다 — 원문은 attach 왕복을 기다려야 해서 `git_viewer.query` 와 같은 비동기 accept 다. namespace 가 `markdown.` 이 아닌 것은 그 prefix 가 plugin 으로 forward 되기 때문이다([플러그인 제작 가이드](plugin-development.md) 참고). `App::dispatch_pending_markdown_content_forwards`(`about_to_wait`)가 drain 해 원격 id 로 치환한 뒤 아래 조회 이벤트를 보낸다. 세션을 못 찾거나 송신이 실패하면 그 자리에서 같은 `request_id` 로 `ok:false` 결과를 plugin 에 돌려준다.
- **host → plugin 결과**: 회신 이벤트는 로컬 id 로 치환돼 `markdown_mirror.content_result {surface_id, request_id, ok, file, source, truncated, reason}` host event 로 번들 plugin 에만 unicast 된다. 연결이 끊겨 Reconnecting 에 들어가면(`enter_reconnecting`) 그 세션의 모든 markdown leaf 에 `request_id: 0` + `ok:false` 를 보낸다 — `0` 은 발급되지 않는 값이라 "연결이 끊겼다" 는 abandon 신호다. plugin 은 대기 중인 요청을 끝내고, 원문을 이미 표시 중인 문서도 **끊김 상태**로 둔다(`MdDoc::apply_remote_result`) — 끊김 상태의 렌더는 로딩·실패·원문보다 앞서 끊김 문구 하나다(`render_document`). 끊김은 다음 성공 회신이 지운다. **재연결**(`reconnect_session`)은 survivor leaf 의 plugin surface 를 `share_handles` 로 이어 받아 원문 요청이 저절로 나가지 않으므로, `Connected` 로 되돌린 직후 세션의 로컬 markdown id 마다 아래 변경 신호(`markdown_mirror.changed`)를 한 번 보낸다. 근거·대안은 [원격 attach (Remote attach)](../features/remote-attach/index.md).
- **조회 (client→server)**: `{"event":"markdown_content_request", "request_id", "surface_id"}`. `list_dir`/`git_query` 와 같은 형태 — `StreamControl` enum **밖**의 raw JSON `event` 태그를 같은 `StreamTag::Control` 채널에 싣고, 서버는 알 수 없는 event 를 조용히 무시한다(전방/후방 호환). 파싱은 `tasty_ipc::stream_hub::MarkdownContentRequestMsg`, 소비는 `event_handler.rs::apply_markdown_content_request_msg`(gui, holder engine 순회)와 `boot/headless_stream.rs`(headless, 단일 engine).
- **회신 (server→client)**: 성공은 `{"event":"markdown_content_result", request_id, surface_id, ok:true, file, source, truncated}`, 실패는 같은 event 에 `ok:false` + `reason`. **에러 채널은 하나다** — client 가 그 `reason` 을 렌더의 `load_error` 로 옮긴다. **파일 없이 열린 markdown surface 는 에러가 아니다**: 서버에서도 빈 문서가 보이므로 `ok:true` + 빈 `file`/`source` 로 답한다.
- **파일은 서버 host 가 직접 읽는다** (`src/remote/server.rs::handle_markdown_content_request`). 서버측 markdown plugin 에 되묻지 않는다 — 이 핸들러는 동기 경로이고 plugin 왕복은 비동기라 그 안에서 기다릴 수 없다(`handle_git_query_request` 가 `tasty-git-core` 를 host 에서 직접 부르는 것과 같은 형태). 에러 구분 수준도 `list_dir_for_request` 와 같다(`permission denied` 대 그 외 io 에러 문자열).
- **예산**: `MARKDOWN_CONTENT_BYTE_BUDGET = 700 KiB` — `LIST_DIR_ENTRIES_BYTE_BUDGET`·`GIT_QUERY_BYTE_BUDGET` 과 같은 근거(프레임 하드 상한 `MAX_FRAME_LEN` 1 MiB 보다 충분히 작게)이자 **같은 재는 대상**: 원문 바이트가 아니라 `serde_json` 문자열이 된 뒤의 바이트다(감싸는 따옴표 포함). 이스케이프는 `"`·`\`·개행에서 2 배, 그 밖의 제어문자에서 6 배(`\u00XX`)까지 부푸므로 원문으로 재면 이스케이프가 많은 문서가 예산을 통과한 뒤 프레임 상한을 넘어 세션이 끊긴다 — 예산이 막으려던 바로 그 사고다. 그래서 실리는 **원문** 길이는 이스케이프가 많을수록 짧다(최악 약 116 KiB). 비용표는 `json_escaped_char_len` 이고 BMP 전수 대조 테스트가 serde_json 과의 어긋남을 잡는다. 자르는 자리는 **UTF-8 문자 경계**(`char` 단위로 걷는다), 잘리면 `truncated: true` 로 알린다. markdown plugin 의 대용량 게이트(1 MiB)와는 다른 층이며, 그 게이트를 건드릴 파일은 직렬화하면 더 커지므로 항상 잘려서 도착한다.
- **인가**: `client_holds_workspace(client_id)` 하나("attach 점유 = 신뢰"). 새 permission 토큰을 만들지 않는다 — 이 채널이 나르는 것은 SSH 로 붙은 사용자가 이미 읽을 수 있는 그 호스트의 파일 원문이다. **인가되는 집합은 engine 전체다** — 그 술어는 **어떤** 워크스페이스든 점유했는가만 보고(`workspace_locks` 전체 스캔), 대상 조회는 `find_surface_by_id`(전 워크스페이스 순회)라 W2 만 점유한 client 도 W1 의 markdown 원문을 받는다. list_dir(wire 에 `dir` 문자열만·워크스페이스 바인딩 필드 없음)·git_query(engine 전역 `TerminalStore` 조회 또는 임의 `worktree_path`)와 같은 갈래이며, anchor 의 holder 를 직접 검증하는 채널(입력·resize·구조 op forward, `workspace_holder(ws) == client`)과는 다르다.
- **절단 표시**: client 는 `truncated: true` 를 **toast**(`attach.toast.mirror_markdown_truncated`, `apply_markdown_content_result_event`) 로 알리고 문서 본문에는 심지 않는다 — `list_dir` 의 `filepicker.remote_listing_truncated` 선례와 같다(본문에 심으면 원문의 일부인지 구분되지 않고, markdown 은 심는 자리가 코드펜스 안일 수 있다). 단 `agent_origin: true` 로 건 요청(에이전트의 `markdown.reload`)의 회신은 toast 없이 로그로만 남긴다 — 송신 때 세션의 `agent_requests`(`src/app/attach_client/agent_origin.rs`)에 request_id 를 넣고 회신 때 꺼내 가른다([내부 팝업 시스템](../design/systems/popup.md)).
- **변경 신호 (server→client)**: `{"event":"markdown_changed", "surface_id"(원격)}`. 발신은 `src/remote/server.rs::notify_markdown_changed` 이고 호출처는 `webview.set_url` 핸들러(`src/adapters/ipc/handler/webview.rs`) 하나다 — markdown plugin 의 재렌더(파일 감시·`markdown.reload`·테마 변경·최초 생성)는 전부 그 IPC 로 host 에 오므로 plugin 을 고치지 않고 신호원을 갖는다. 그래서 신호는 실제 파일 변경의 **상위 집합**이다. 화이트리스트(`is_attach_content_allowed`) 밖 kind 는 신호가 없다. 수신자는 `OccupancyRegistry::workspace_holders` — 위 인가 술어가 참인 그 집합(이 engine 의 워크스페이스를 하나라도 점유한 client 전부, 중복 없이)이고, 그 surface 를 담은 워크스페이스의 holder 로 좁히지 않는 근거는 [원격 attach (Remote attach)](../features/remote-attach/index.md) 에 있다. 점유가 없거나 notifier 가 없으면 아무것도 안 한다. client 는 reader 체인의 `parse_markdown_changed` → `MirrorEvent::MarkdownChanged` 로 받아 `markdown_mirror_local` 로 로컬 mirror markdown leaf 에 매핑되는 것만 `markdown_mirror.changed {surface_id(로컬)}` 로 plugin 에 unicast 한다(자기가 mirror 하지 않는 문서의 신호는 버린다). plugin 의 반응은 문서가 무엇을 보여 주는가로 갈린다(`MdDoc::on_remote_changed`): 원문을 보여 주는 중이면 **다시 받지 않는다** — 새로고침 버튼 색만 바꾸고, 원문은 사용자가 누를 때 위 조회로 온다. 원문을 못 보여 주는 중(끊김·실패·로드 전)이고 요청이 진행 중이 아니면 다시 요청한다(재연결 신호가 이 갈래를 쓴다). 요청이 진행 중이면 무시한다. **서버가 헤드리스면 신호가 없다**: `webview.set_url`은 두 빌드에서 URL을 기록하지만 markdown 변경 신호 발신은 GUI 전용이다(조회·회신은 헤드리스에서도 된다). 테스트: `src/remote/server.rs`의 `markdown_changed_tests`(수신자 집합·중복 없음·점유 없음·화이트리스트 밖). `webview.set_url` 에서 그 함수로 이어지는 호출은 `webview.rs` 의 `set_url_on_markdown_surface_signals_attached_clients` 가 잰다(점유 client 의 스트림에 신호 한 번, 터미널 leaf 에는 없음 — 호출을 빼는 변이에서 실패한다).
- **서버는 헤드리스여도 된다.** 이 채널의 서버측 코드에는 feature 게이트가 하나도 없고(파일 read + control 프레임), 요청 소비도 두 조합에 각각 있다(gui `event_handler.rs`, headless `boot/headless_stream.rs`). 다만 **surface 를 만들 수 있어야** 그 앞이 성립하는데, 한때 헤드리스가 `webview` kind 를 등록하지 않아 `tab.create {type:"markdown"}` 이 `unknown surface kind` 로 죽었다 — 지금은 `boot/headless_plugins.rs` 의 `register_one_surface_kind` 가 세 rendering 을 전부 등록하고, 그 kind 를 지목한 생성 요청이 plugin 기동을 유발한다(`ensure_plugin_for_surface_kind`). 판정과 실측은 [headless-ipc-surface.md](headless-ipc-surface.md) 의 "등록된 kind 조회" 절.
- **테스트**: `tests/attach_markdown_content_loopback.rs` 가 role 직렬화·왕복·없는 파일·점유 없는 client 거절·예산 초과 절단·**다른 워크스페이스만 점유한 client 의 인가**(engine 전체임을 값으로 고정) 여섯을 loopback 프로토콜 레벨로 고정한다. 그 여섯의 **자동 채널은 헤드리스 조합 하나뿐**이다(`check-headless` 의 전체 스위트 — [ci-gates.md](ci-gates.md)), 그래서 위 항목이 헤드리스에서 성립하지 않으면 이 시험들에는 도는 자리가 없다. 절단 테스트는 본문을 따옴표로만 채워(원문 400 KiB → 직렬화 800 KiB) **무엇을 세어 잘랐는지**까지 잰다 — 원문으로 쟀다면 잘리지 않고 통과한다.

## mirror 구조 변경 forward

mirror workspace의 split/new-tab/close/move/convert/restore는 로컬 journal의 구조를 직접 바꾸지 않고 원격 서버로 전달한다. 로컬 mirror 트리는 원격 확정 결과의 projection이다.

- App은 원 EngineId·connection epoch·sender와 local↔remote mapping, origin을 요청 값으로 고정한다. `prepare_journal_forward`와 `src/app/journal/forward.rs`가 durable enqueue 명령을 claim한 뒤 같은 연결·mapping인지 다시 검사한다. 기존 raw 입력이나 구조 전송을 재연결 뒤 새 sender로 자동 재송신하지 않는다.
- 로컬 forward operation의 Succeeded는 그 원 연결 큐에 접수됐다는 뜻이다. 원격 서버의 실행·확정 완료와 같지 않다. 서버가 해당 구조의 writer이며 원격 result/delta가 mirror 변경을 전달한다.
- 서버의 `src/app/journal/commands/inbound.rs`가 holder와 대상 workspace를 검증하고 명령으로 실행한다. result → delta → 새 surface snapshot/tap 순서를 유지해 ID mapping보다 raw 출력이 먼저 오지 않게 한다.
- source와 target을 함께 갖는 move는 둘 다 같은 세션 mapping으로 원격 ID를 해소한다. local↔mirror 또는 다른 mirror workspace 사이 이동은 로컬 번호를 원격 번호처럼 보내지 않는다.
- user/agent origin은 요청과 함께 전달한다. origin이 없으면 옛 client의 user 동작을 유지하고, 모르는 값은 agent로 처리한다. 원격 user 표지가 로컬 View 포커스를 바꿀 권한을 주지는 않는다. 사용자 요청의 실패는 해당 View 정책에 따른 표시, 에이전트 실패는 로그로 처리한다.
- 원격 RestoreClosedItem은 anchor workspace의 서버 복원 기록만 사용한다. 비어 있다고 로컬 스택으로 fallback하지 않는다. 서버 로컬 복원의 전역 LIFO와 원격 workspace 범위는 구분한다.
- mirror 파일 경로는 원격 경로다. 로컬 존재 검사나 로컬 recent-file 기록으로 바꾸지 않는다. markdown.navigate의 accepted 응답 등 개별 비동기 API의 접수 의미는 실제 원격 실행 성공과 구별한다.
- headless attach client는 지원하지 않는다. 원격 서버로서 명령을 받는 headless와 GUI mirror를 연결해 외부 서버로 보내는 client를 혼동하지 않는다.

wire와 origin 호환의 결정 근거는 이 문서, close cleanup의 실제 완료는 [닫기 순서](../architecture/close-sequence.md)를 따른다. source 경로 정합이 구 client/server·지연·손실의 실행 검증을 대신하지 않는다.

## 서버 로컬(비-holder) 구조 변경 차단

hard 점유의 holder가 원격으로 전달한 구조 명령과, holder가 아닌 로컬 IPC/CLI가 같은 workspace를 바꾸려는 요청을 구별한다. 사용자 영향과 허용 범위는 [원격 attach 정책](../features/remote-attach/index.md#서버피점유측-비-holder-구조-변경-차단)을 따른다.

- 로컬 IPC의 `hard_occupied_structural_guard`는 명시 대상의 workspace를 해소해 점유를 검사한다. terminal.spawn은 pane override까지 반영한 최종 대상에서 검사한다. 대상 해소 실패는 원래 파라미터 검증 오류로 처리하며 현재 포커스로 바꾸지 않는다.
- holder의 forward는 `src/app/journal/commands/inbound.rs`에서 원 stream client·점유·대상 범위를 검사한다. 로컬 비-holder 가드를 공통 execution 함수 안에 무조건 넣어 정당한 holder까지 막지 않는다.
- 새 터미널의 논리 ID가 공개되기 전에 raw 출력이 도착하지 않아야 한다. 구조 observation은 멤버 등록과 pending tap의 physical generation을 기록하고, `src/remote/structure_sync.rs`가 필요한 result/delta 뒤에 새 tap을 연다.
- `RemoteState`가 notifier·대기 reply/tap을 소유한다. `take_structure_changed`는 reply가 남은 workspace를 보류 집합에 유지한다. OccupancyRegistry는 점유 판단 원본이며 socket/worker를 소유하지 않는다. forward reply가 남은 workspace의 tap을 먼저 열거나 같은 surface에 두 번 등록하지 않는다.
- 실제 구독은 `src/remote/subscription.rs`가 원 grant·terminal generation·StreamHub registration을 검사한다. 큐 손실과 stale 구독은 명시적으로 끊거나 버리고, parser/PTY를 대신 종료하지 않는다.

`source_guards::committed_tap_order`는 지정한 result/delta 생산자의 호출 순서와 `Sent` arm, pending reply 보류 및 tap 직전 generation 조회를 텍스트로 대조한다. 임의의 다른 생산자, 분기 도달성, 실제 송신·수신 성공까지 증명하지 않는다.

이 경로의 기존 loopback·tap-count·권한 fixture는 현재 API에 맞춰 정합한 뒤 실행해야 한다. 시험 이름이나 과거 성공만으로 journal publication과 현재 bounded stream 경계가 검증됐다고 보지 않는다.

## SSH 터널 (원격 client 공통)

`crates/tasty-ssh/src/lib.rs` — `remote attach` 와 `remote check` 가 공유.

- **시스템 ssh 위임**: 자체 암호화 없이 시스템 `ssh` 를 자식 프로세스로 실행. 사용자 `~/.ssh/config`·agent·known_hosts 재사용. **Windows 는 시스템 OpenSSH 풀경로**(`%WINDIR%\System32\OpenSSH\ssh.exe`) 우선 — git 번들 ssh 는 윈도우 ssh-agent(named pipe)를 못 봐 무암호 인증 실패.
- **`TASTY_SSH_VERIFY`(검증용 host key 예외)**: Tasty 나 `tasty` CLI 프로세스 환경에 이 변수가 있으면 값과 관계없이(존재만 본다) Tasty 가 실행하는 ssh 명령에 `-o StrictHostKeyChecking=accept-new` 를 넣는다. 사용자 확인 없이 자동 검증을 돌리기 위한 수단이며 사용자 가이드에는 싣지 않는다. 인자 순서는 `crates/tasty-ssh/src/lib.rs` 의 `push_common_opts` 가 정한다 — 이 옵션은 프로필 `extra_options` 보다 앞에 오고, 명령행 옵션이라 `~/.ssh/config` 보다도 우선한다(ssh(1) 은 같은 키의 처음 값을 쓴다). 그래서 처음 보는 host key 를 묻지 않고 `known_hosts` 에 추가한다. 이미 기록된 key 와 다르면 accept-new 도 연결을 거절한다. 변수가 없으면 Tasty 는 host key 옵션을 넘기지 않고 사용자 ssh 설정(설정이 없으면 ssh 기본값 `ask`)을 따른다. 읽는 곳은 `git grep -n TASTY_SSH_VERIFY` 로 찾는다(attach·auto-attach·원격 목록·`remote check`·프로필 감지·`tool ssh` 접속·포트 발견).
- **원격 포트 발견**: 기본 `auto` = subcommand → file-unix → file-windows 순서로 원격 DefaultShell 4종(PowerShell/cmd/git bash/unix) 커버. `--remote-port-mode` 로 고정, `--remote-tasty <path>` 로 원격 바이너리 경로(기본 `tasty`).
- **포트 발견 상한(no-hang)**: 포트 발견/셸 감지는 무기한 블록하지 않는다 — ssh `-o ConnectTimeout`(`SSH_CONNECT_TIMEOUT` 10초, 연결 수립) + ssh 자식 1개당 프로세스 레벨 감시(`PORT_DISCOVERY_STEP_TIMEOUT` 20초, kill + wait 로 좀비 없이 회수) + 호출 1회 전체 예산(`PORT_DISCOVERY_TOTAL_TIMEOUT` 45초, 체인 단계 수만큼 곱해지는 것을 차단) 3겹. 상수는 `crates/tasty-ssh/src/lib.rs` 에 `pub const`. 프로필 `extra_options` 의 `ConnectTimeout` 이 기본값을 이긴다(ssh(1) first-wins → 기본값을 뒤에 push). 근거 → [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md), 상세 → [features/remote-attach "연결 시도 상한"](../features/remote-attach/index.md#연결-시도-상한-no-hang).
- **포트 발견용 ssh 자식의 취소**: `run_capture_with_budget`은 worker에서 child를 관리한다. GUI browser, auto-attach의 AttemptToken, ProfileDetections worker가 `SshCancel::scope()`를 설치한다. App의 `request_cancel()`은 취소 flag만 세우며 worker가 kill/wait와 결과 회수를 담당한다. 취소 요청, child reap, worker join은 별도 관측이다. 취소 이후에는 auto 체인의 나머지 단계를 실행하지 않으며 `Cancelled`는 fallback 대상이 아니다. IPC 소켓이 끊겼다는 사실만으로 모든 작업에 취소가 전달된다고 보장하지 않는다.
- **포트 발견 실패 진단**: 전 단계 실패 시 `PortDiscoveryError`(`crates/tasty-ssh/src/lib.rs`)가 exit code 기반(로케일 무관)으로 SSH 연결 실패 / 원격 인스턴스 미실행 / 포트 파싱 실패 + 위 상한 초과 시 타임아웃 + 사용자 취소, 5분류한다 — 원격 raw stderr 는 `Display` 에 노출하지 않고 `tracing::debug!` 로만 남긴다(타임아웃·취소 경로도 동일). auto 체인이 전 단계 실패하면 가장 확정적인 분류를 대표로 고른다(`pick_most_informative` — 취소 > 타임아웃 순 우선. 취소는 사용자가 끊었다는 확정 사실이라 시간·연결 실패로 보고하면 오도한다). 상세 → [features/remote-attach "원격 포트 발견 실패 진단"](../features/remote-attach/index.md#원격-포트-발견-실패-진단).
- **터널 생명주기**: detach/종료 시 자식 ssh kill(고아 터널 방지)하되 **원격 데몬은 생존**(server-owns-PTY persistence = detach 의 본질). 종료 경로를 거치지 않는 비정상 종료(SIGTERM·SIGKILL 등)에 대비해 `SshTunnel::establish`는 ssh를 `tasty_reaper::spawn_bound_to_host`로 띄운다. Linux는 `PR_SET_PDEATHSIG`(SIGTERM)를 프로세스 수명의 전용 spawner 스레드에서 걸어 터널을 띄운 워커 스레드가 끝나도 유지되고 호스트가 끝나면 함께 끝난다. Windows는 호스트 Job Object(`init_host_reaper`를 부른 호스트만, CLI 제외)에 등록하고, macOS는 결속하지 않는다. OS와 관계없이 정상 종료(메뉴 종료·마지막 창 닫기·debug `system.shutdown` 등 종료 머신을 거치는 경로)에서는 `Remote::begin_shutdown`이 세션과 대기 연결을 비우고 시도를 취소해 터널을 회수한다. 그래서 OS별 보장은 다음과 같다. Linux는 정상 종료와 SIGTERM·SIGKILL·크래시 모두 터널이 끝난다(SIGTERM만 실측). Windows는 Job Object의 `KILL_ON_JOB_CLOSE`로 호스트 프로세스가 어떻게 끝나도 함께 끝나지만, job 생성·등록이 실패하면 정상 종료만 보장한다(코드 대조, 실 기기 미측정). macOS는 정상 종료만 보장하며 SIGTERM·SIGKILL·크래시로 끝나면 터널이 남을 수 있다(실 기기 미측정). 정상 종료 경로는 Linux에서 PDEATHSIG를 끈 빌드로 `system.shutdown` 뒤 터널 0, SIGTERM 뒤 터널 1을 확인했다. 자동 재연결(attach 한정): 지수 백오프(0.5s→30s)로 터널+attach 재수립(`--no-reconnect` 로 끔) — 이 재연결은 `run_attach_on_port` 가 반환하는 `AttachExit::Disconnected` 를 전제로 한다. mirror-dump/workspace-mirror-dump 모드는 처음부터 reader 를 별도 스레드 + `mpsc::channel` 로 분리해 `rx.recv_timeout` 의 `RecvTimeoutError::Disconnected`(끊김) vs `Timeout`(정상 deadline) 을 구분해 이를 반환해왔다. `--raw` 모드(`run_raw_bridge`)도 동일 계약을 만족한다 — server reader 스레드가 `mpsc` 채널에 보내고(`RawEvent::Server`/`ServerRecvErr`), main 은 `rx.recv()`(deadline 없이 블로킹)만 기다린다. server reader 의 `conn.recv()` 가 `Err` 면 `RawEvent::ServerRecvErr` 를 명시적으로 보내 `AttachExit::Disconnected` 로 이어진다.
- **stdin은 스레드 하나가 읽는다.** 프로세스 시작 때
  [`spawn_stdin_reader`](../../crates/tasty-cli/src/local/attach.rs)를 한 번 호출한다.
  읽은 청크는 `StdinSlot`(`Arc<Mutex<Option<mpsc::Sender<RawEvent>>>>`)의 현재 sender로
  보낸다. 재연결 때 `install_sender`로 sender만 교체해 두 리더가 같은 stdin을 읽는
  경합을 피한다.

- **재연결 중 입력이 사라질 수 있는 구간**: 이전 세션이 끝나 슬롯이 `None`이거나 수신 측이 이미 종료된 sender를 가리킬 때는 입력 청크를 보내지 못한다. 리더 스레드는 **그 청크만 버리고 계속 읽는다.**
  슬롯을 바꾸는 함수는 `install_sender`뿐이다. 리더 스레드가 슬롯까지 수정해 새 연결을 덮어쓰지 않도록 한다(ABA 경쟁 방지).
  입력 유실은 재연결 중 세션을 바꾸는 구간에서 발생하며, 리더 스레드가 누적돼 시간이 갈수록 커지는 문제는 아니다.
  슬롯이 비어 있는 동안 stdin EOF나 오류가 발생하면 별도 `AtomicBool`에 기록한다. 다음 세션의 `install_sender`가 sender를 설치할 때 `RawEvent::StdinEof`를 바로 전달해, 이미 닫힌 stdin을 계속 기다리지 않도록 한다.
  현재는 재연결 중 보내지 못한 입력을 버퍼에 보관하지 않는다.
- **loopback 직결 없음**: 이 공용 경로(`--into-gui` 없는 `remote attach`·`tool attach`·`remote check`)는 인라인 host 가 `127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT` 여도 SSH 포트 발견과 터널을 거치므로 SSH 없이 붙지 않는다. loopback 직결 분기는 `tasty_remote::browse::resolve_endpoint`(browse·`remote new-workspace`·IPC `remote.*`와 이를 부르는 `remote attach --into-gui --ssh`·GUI 원격 브라우저 워커)와 자동 attach 의 `src/app/auto_attach.rs` 에만 있다. 같은 머신 다중 인스턴스에서 attach 를 재현할 때는 debug 빌드의 `tasty debug attach` 로 서버 인스턴스의 포트에 직접 붙는다([debug-ipc](debug-ipc.md)).

## 연결 생존 확인 (read timeout + heartbeat)

attach 스트림은 read timeout 이 없는 순수 blocking I/O 라 네트워크가 FIN/RST 없이 **조용히** 끊기면(케이블 단절·NAT 타임아웃 등) 소켓의 read 가 영원히 대기해 어느 쪽도 끊김을 감지하지 못한다. 아래는 이를 감지하는 연결이다 — 상수는 `crates/tasty-ipc/src/stream.rs` 의 `HEARTBEAT_INTERVAL`(5초)/`HEARTBEAT_TIMEOUT`(30초, interval 의 6배 — 일시적 jitter 나 짧은 경로 정체로 인한 오탐 방지).

- **read timeout**: 서버(`tcp_ipc_server.rs::handle_stream_connection`)·GUI client(`crates/tasty-remote/src/transport.rs`의 `PreparedConnection::connect`)·CLI client(`tasty-ipc/src/client/stream.rs::StreamConnection::open_with`) 3곳 모두 소켓에 무응답 제한을 read timeout 으로 건다. GUI client·bulk·서버는 연결을 시작할 때의 `[remote_attach] heartbeat_timeout_secs` 를 쓰고, 설정을 읽지 않는 CLI 프로세스는 `HEARTBEAT_TIMEOUT` 을 쓴다([설정](#무응답-제한-설정)). **서버는 그것을 attach dispatch 보다 먼저 건다**(`arm_stream_read_timeout` 이 `validate_stream_proto` 앞이다) — 즉 판정은 **연결 단위**이고 점유 유무와 무관하다: 아무 workspace 도 점유하지 않은 단순 upgrade 연결도 제한 시간 동안 침묵하면 닫힌다. 그래서 client 는 점유를 잡았든 안 잡았든 Ping 을 보내야 한다. `try_clone` 된 reader/writer 는 같은 소켓 옵션을 공유하므로 한 번만 걸면 양쪽 다 적용된다. CLI 는 핸드셰이크 ack/`attached(_workspace)` 디스크립터 대기(둘 다 `recv()`)에도 자동으로 적용된다.
- **`StreamTag::Ping`**: 빈 payload 의 keepalive 프레임(값 `2`, 코덱은 이전부터 예약돼 있었다). 수신측은 아무 처리도 하지 않는다 — `read_frame` 호출이 성공적으로 리턴하는 것 자체가 read timeout 을 리셋하므로, Ping 이든 실제 Data/Control 이든 도착만 하면 liveness 로 인정된다.
- **송신은 write 쪽이 idle 할 때만**: 서버의 write thread(`handle_stream_connection`)는 기존 `for frame in sink_rx` blocking iterator 대신 `sink_rx.recv_timeout(HEARTBEAT_INTERVAL)` 루프를 쓴다 — sink 에 실제 Data/Control 프레임이 흐르면 그게 곧 liveness 라 Ping 을 보내지 않고, `HEARTBEAT_INTERVAL` 동안 조용하면 빈 Ping 을 대신 흘린다. GUI/CLI(raw 브리지) client 는 각각 별도 heartbeat 스레드가 `HEARTBEAT_INTERVAL` 마다 무조건 Ping 을 보낸다(활성 트래픽 여부를 별도로 추적하지 않음 — 5바이트 프레임 오버헤드가 그 추적 비용보다 훨씬 싸다).
- **CLI dump 모드(mirror-dump/workspace-mirror-dump)도 client 발 heartbeat 을 보낸다 — 스레드 없이 수집 루프 안에서**: 수집 대기를 `min(창 끝, 다음 Ping)` 으로 끊고 `HEARTBEAT_INTERVAL` 마다 `Ping` 을 쓴다(`DumpHeartbeat`). 그래서 `--dump-after` 가 `HEARTBEAT_TIMEOUT` 보다 길어도 서버가 창 도중에 끊지 않는다. 첫 Ping 은 창을 연 지 한 주기 뒤라 기본 500 ms 창의 송신 프레임은 heartbeat 이 없던 때와 같다. `Ping` 쓰기가 실패하면 `Disconnected` 로 끝난다. writer 를 루프 하나가 쥐므로 raw 브리지(stdin 라우팅과 writer 공유 → 별도 heartbeat 스레드 + `Mutex`)와 형태가 다르다.
- **양방향이 필요한 이유**: 서버 write thread 의 Ping 은 client 의 read timeout 을(idle mirror 뷰), client 의 Ping 은 서버의 read timeout 을(client 가 오래 아무 입력도 안 보내는 세션) 각각 갱신한다 — 한쪽만 보내면 반대 방향의 read loop 가 오탐 disconnect 된다.
- **timeout 만료 → 기존 disconnect 경로 재사용**: read timeout 으로 인한 `WouldBlock`/`TimedOut` io 에러는 서버 `run_stream_read_loop` 의 오류 분기·GUI client reader 의 오류 분기(`disconnected` 를 세운다)·CLI 의 채널 기반 끊김 신호(`run_mirror_dump`/`run_workspace_mirror_dump` 의 `mpsc::RecvTimeoutError::Disconnected`, `run_raw_bridge` 의 `RawEvent::ServerRecvErr` — 위 "SSH 터널" 절 참고)를 그대로 타 EOF 와 동일하게 처리된다 — 별도 sweep 스레드나 새 상태 없이, 아래 "mirror 세션 종료"·[`features/remote-attach`](../features/remote-attach/index.md) 의 "자동 해제" 가 조용한 네트워크 단절까지 커버하게 된다.
- <a id="끊김-원인-로그"></a>**끊김 원인 로그**: 읽기 실패는 `tasty_ipc::stream::ReadFailure` 로 분류한다. client 와 서버가 같은 함수를 쓴다. errno 는 OS마다 달라(같은 read timeout 만료가 macOS 35, Linux 11) `io::ErrorKind` 로 판정한다.

  | `io::ErrorKind` | 분류 | 로그 문구 |
  |---|---|---|
  | `WouldBlock`, `TimedOut` | `HeartbeatTimeout` | `하트비트 timeout: 수신 대기 timeout(N초) 안에 프레임을 끝까지 받지 못했다` |
  | `UnexpectedEof`, `ConnectionReset`, `ConnectionAborted` | `PeerClosed` | `원격이 연결을 닫았다` |
  | `InvalidData` | `Protocol` | `프로토콜 오류: 프레임 형식이 맞지 않는다` |
  | 그 밖 | `Other` | `기타 수신 오류` |

  - read timeout 만료는 macOS·Linux 에서 `WouldBlock`(EAGAIN), Windows 에서 `TimedOut` 으로 온다. Windows 쪽은 Rust std 문서 기준이며 실 기기에서 측정하지 않았다. 프레임 일부를 받는 도중의 만료도 같은 kind 라 "아무 프레임도 받지 못함"으로 단정하지 않는다.
  - N 은 상수가 아니라 그 소켓에 실제로 건 값이다. GUI client 는 연결을 시작할 때 넘긴 값, 서버는 연결마다 `StreamContext` 에서 읽은 값을 쓴다.
  - **GUI client**(`spawn_attach_reader_thread`): `attach reader thread: remote workspace <원격 workspace id> 연결 종료를 알린다. 원인 <문구>: <원본 오류>` 를 `warn` 으로 남긴다. 정리나 쓰기 실패로 이쪽이 먼저 `disconnected` 를 세운 뒤의 수신 오류는 원격 원인이 아니라 `debug` 로만 남긴다. 손실 뒤 재동기화(`release_for_resync`)처럼 `disconnected` 를 세우지 않고 Detach 만 보내는 종료도 같다. 쓰기 스레드가 Detach 를 쓰기 **전에** reader 와 공유하는 `detach_sent` 를 세우고(쓰기 뒤에는 이쪽이 소켓을 닫는다), reader 는 뒤따르는 EOF 를 `debug` 로 남기면서 재attach 가 이어지도록 `disconnected` 는 세운다. 재동기화 사유는 그 직전의 손실 `warn` 과 toast 가 알린다.
  - **서버 읽기 루프**(`run_stream_read_loop`): `stream client <id> from <peer>: 읽기 루프가 끝났다. 원인 <문구> (<분류>): <원본 오류>` 를 `warn` 으로 남긴다. client 가 보낸 Detach, 메인 루프 소멸, 서버가 연결을 끝내는 프레임(Detach 나 그보다 먼저 가는 `force_detached` Control)을 보내기 시작한 뒤의 오류는 정상 종료라 `debug` 이하다. client 는 그 프레임을 받자마자 닫을 수 있으므로 쓰기 스레드는 그 표지를 프레임을 쓰기 **전에** 세운다. 그 프레임의 쓰기 실패는 따로 `warn` 으로 남는다.
  - **서버 쓰기 스레드**(`spawn_stream_write_thread`): 데이터 프레임이나 Ping 을 쓰지 못하면 `stream client <id>: ... 송신 실패로 쓰기를 멈춘다(수신 timeout과 별개인 송신 오류)` 를 `warn` 으로 남긴다. 읽기 쪽 하트비트 timeout 과 문구로 구분된다.
  - bulk 채널(`src/app/attach_client/bulk.rs`)은 읽기 실패를 직접 로그하지 않고 업로드 결과의 오류로 돌려준다. 그래서 이 분류를 적용하지 않는다.
- **GUI heartbeat 스레드 정리**: `cleanup_mirror_workspace` 가 세션 종료 시(원격발이든 사용자 close 든) `sess.disconnected` 를 set 해, `writer: Arc<Mutex<TcpStream>>` 를 계속 붙들고 있는 heartbeat 스레드가 다음 tick 에 스스로 종료하게 한다 — 안 하면 세션이 정리된 뒤에도 소켓/스레드가 새는 leak.
- **버전 skew 리스크(heartbeat 축은 완화 로직 없음, 의도적)**: read timeout이 걸린 이후부터 이 프로토콜을 도입한 버전이 적용된다. 구버전 프로세스(Ping 미전송)와 신버전 프로세스가 같이 떠 있는 상태(예: 호스트 재시작 없이 CLI/플러그인만 재배포)에서는 idle 상태의 mirror 세션이 제한 시간 뒤 오탐 disconnect 될 수 있다 — 이 프로젝트는 단일 사용자 로컬 앱이라 그런 skew 창이 짧고 드물어 허용 가능하다고 판단했다. 다중 버전 동시운영이 필요해지면 capability 협상을 재검토한다. **단 스트림 프로토콜 버전(`proto`) 자체는 핸드셰이크에서 검증된다** — 위 "점유 레지스트리" 의 `validate_stream_proto` 항목 참고. 지금은 정확히 같은 값만 통과하는 엄격 매칭이라, `STREAM_PROTO` 를 올리면 구버전 client 는 조용한 실패가 아니라 명시적 거절을 받는다.

<a id="클라이언트측-비대칭서버는-점유-유지-클라이언트는-사라짐-원인"></a>

<a id="무응답-제한-설정"></a>

## 무응답 제한 설정

`config.toml` 의 `[remote_attach] heartbeat_timeout_secs` 가 무응답 제한(초)을 정한다. 기본값은 30, 범위는 10~600이다. 하한은 `HEARTBEAT_INTERVAL` 의 2배다. 파일의 값이 범위를 벗어나면 `normalize` 가 30으로 되돌려 저장한다. 송신 주기 5초는 설정과 관계없이 같다.

- **적용 시점**: 새 연결부터다. 값은 `AppServices::attach_heartbeat`(`SharedHeartbeatTimeout`)에 있다. 부팅은 IPC를 열기 전에, 설정 변경은 GUI·headless 의 반영 경로에서 이 값을 바꾼다. 서버는 `StreamContext` 의 같은 핸들을 연결마다 한 번 읽고, GUI client 와 bulk 는 시도를 시작할 때 읽어 재연결도 그 시점의 값을 쓴다. 이미 열린 소켓의 timeout 은 바꾸지 않는다.
- **handshake**: `StreamConnection::open_with` 가 소켓 read timeout 을 건다. 넘겨받은 값을 쓰므로 handshake ack 대기부터 설정값이 적용된다.
- **프로세스별 값**: 양쪽 프로세스가 각자 자기 설정을 쓰고 값을 협상하지 않는다. client 가 60초, 서버가 30초면 client→서버 방향의 정체는 서버가 30초에 끊는다. 설정을 읽지 않는 CLI 프로세스의 쪽은 `HEARTBEAT_TIMEOUT`(30초)이다.
- **쓰기 timeout**: GUI client 와 bulk 는 같은 값을 쓰기 timeout 으로도 건다. 서버 stream writer 에는 쓰기 timeout 이 없다.
- **별개인 상한**: 연결 직후 첫 줄 기한과 RPC 응답·거절 ack 쓰기 상한은 heartbeat 와 무관한 20초 상수다([IPC 서버](../architecture/ipc-server.md)). raw bridge 가 서버의 점유 해제를 기다리는 기한은 `HEARTBEAT_TIMEOUT` 이다.

## SSH keepalive와 attach heartbeat의 차이

SSH 터널은 `ServerAliveInterval=15`와 `ServerAliveCountMax=3`을 사용한다.
이는 로컬 ssh 자식이 원격 sshd의 응답을 확인하는 장치다. ssh가 종료하면 client의
loopback 연결도 EOF를 받지만, 원격 Tasty의 소켓 생존을 직접 검사하는 것은 아니다.

attach의 `HEARTBEAT_TIMEOUT`은 별개다. SSH가 살아 있어도 Tasty의 데이터가
제한 시간 동안 도착하지 않으면 끊김을 처리한다. SSH 없이 loopback에 직접 연결한 경우에도
적용된다. 두 제한을 더하거나 어느 쪽이 항상 빠르다고 가정하지 않는다.

2026-11-06의 격리 loopback 측정(GUI 인스턴스 두 개, 서버 또는 client에 SIGSTOP)에서 끊김까지
기본값 약 29초, 12초 설정의 새 attach 약 11~12초가 걸렸다. 마지막 프레임은 멈추기 최대 5초 전에
도착하므로 측정값은 설정값보다 짧을 수 있다. 연결 중 12초로 바꾼 뒤 멈춘 기존 연결은 약 29초였다.
이는 attach heartbeat의 기록이며 실제 SSH 인증·keepalive를 측정한 결과는 아니다.

## GUI 자동 재연결 스코프

silent disconnect 정리(`cleanup_mirror_workspace`) 자체는 heartbeat TTL 만료로도 자동 발동한다
(위 "연결 생존 확인"). anchor(매핑된 로컬 워크스페이스)에 물린 세션이 이렇게 끊기면
mirror workspace 는 즉시 걷히지 않는다 — 세션이 `SessionState::Reconnecting` 으로 전이해
재연결 대기 상태를 유지하고, 아래 backoff 스케줄이 자동으로 재연결을 시도한다.
anchor 가 없는 mirror(IPC `remote.attach` 로 연 임시 mirror 등)는 대상이 아니라 기존처럼
즉시 정리된다.

`enter_reconnecting` 은 옛 transport 의 소켓을 그 자리에서 닫는다(`ConnectionWorkers::close_socket`, 작업 스레드 join 은 transport 를 바꿀 때의 drop 이 맡는다). 소켓을 재연결 성공까지 열어 두면 서버는 EOF 를 보지 못해 옛 점유를 heartbeat TTL 까지 쥐고, 서버가 멈췄다 돌아온 직후 도착한 새 attach 를 `already_attached` 로 거절한다. 닫아 두면 서버는 재개하자마자 옛 연결의 끝을 읽어 docs/dev-guide/attach-behavior.md 의 같은 배치 회수 규칙으로 새 attach 를 받는다. 재연결 설치(`install_reconnected_mirror`, 재동기화도 같은 설치를 거친다)는 끝에 `finish_reconnected_view` 로 재연결 toast 를 띄우고 그 mirror 의 explorer View 를 `reload_mirror` 로 다시 읽게 한다(옛 연결의 목록 요청 소비자는 비워지고 응답이 오지 않는다). 다시 읽기는 `connection/tests.rs` 의 `finishing_a_reconnect_rereads_that_mirrors_explorers` 가 본다. 설치 본문에서 이 함수를 부르는지는 App 시험 하네스가 없어 단위 시험으로 보지 않는다.

- **레벨/엣지 트리거(기존, 신규 attach 전담)** — `src/app/auto_attach.rs::maybe_trigger_auto_attach`
  는 "활성 워크스페이스가 매핑 Some & `Remote.active` 에 없으면 트리거"를 **매 프레임**
  재평가한다. 단 첫 attach 가 실패한 anchor 는 `Remote.attach_retry`(`AttachRetry`)의 시각까지
  건너뛴다(`first_attach_waits` — 같은 매핑·엣지 아님일 때만). 실패는 `back_off_first_attach`
  (해석·접수 실패는 `on_first_attach_failed`, 접수 뒤 연결 실패는 `pending.rs::fail_pending_mirror`)가
  재연결과 같은 `Backoff`(0.5초→30초)와 jitter 로 `Remote::record_first_attach_failure` 에 기록하고, `sync_reconnect_timers` 가 아직
  오지 않은 시각만 `Tick::Reconnect(anchor)` 로 예약한다(지난 시각을 예약하면 다른 워크스페이스에
  있는 동안 루프를 계속 깨운다). 기록은 `forget_anchor_backoff` 가 재연결 기록(`Remote.reconnect`)과 함께 지운다 — 설치 성공(`pending.rs`), 재연결 중 매핑 소멸(`trigger_reconnect_anchor`), mirror 닫기(`connection.rs`). 또 `poll_auto_attach` 머리의 `prune_first_attach_retries` 가 창과 parked engine 을 훑어 워크스페이스가 닫혔거나 매핑이 지워지거나 바뀐 anchor 의 기록을 지운다. 예를 들어 이미 활성인 워크스페이스에 `attach_mapping` 을 방금 새로 설정하면
  (`tasty set workspace --ssh-profile ...`) 워크스페이스 전환 없이도 다음 프레임에 즉시
  트리거된다. "재진입 대기(pending reactivation)" anchor(과거엔 disconnect 로 정리된
  모든 anchor 가 여기 들어갔지만, 지금은 이 표시가 남아있는 것 자체는 드물다 — 아래
  backoff 스케줄이 대부분 먼저 처리한다)만 엣지(전환) 게이팅: 활성 워크스페이스 id 가
  **직전 프레임과 달라진 프레임에서만**(`App.remote.last_active_ws` 로 직전 값을 들고
  비교, 술어는 `is_reactivation_edge`) 트리거를 허용한다. 최종 판정은
  `is_attach_trigger_allowed(pending_reactivation, current, previous)`(단위 테스트
  `new_mapping_triggers_immediately_without_transition`/
  `disconnected_anchor_waits_for_transition_before_retrigger`).
  - **backoff 재연결과의 레이스 회피**: anchor 에 이미 `Reconnecting` 세션이 있으면 이
    트리거는 즉시 스킵한다(`maybe_trigger_reconnect` 전담 대상). 이 가드가 없으면 두
    트리거가 같은 anchor 에 대해 동시에 `start_gui_attach`/`reconnect_session` 을 각각
    spawn 해 mirror workspace 가 중복 생성될 수 있다.
- **backoff 재연결 트리거(`maybe_trigger_reconnect`)** — `Reconnecting` 상태인
  세션이 있는 각 anchor 마다 매 프레임 확인한다: ① 사용자가 **지금** 그 anchor
  워크스페이스로 전환해 돌아왔으면(엣지, `current_ws_id == anchor` 로 한정 — 다른
  워크스페이스로의 무관한 전환까지 모든 Reconnecting anchor 를 깨우지 않는다) 즉시, ②
  아니어도 `App.remote.reconnect: HashMap<u32, ReconnectSlot>` 에 저장된 `next_attempt`
  시각이 지났으면(Reconnecting 진입 직후 첫 시도는 슬롯이 없어 즉시). 두 조건 모두
  `Remote.active` 게이트를 공유해 중복 attach 를 막는다. anchor 워크스페이스 자체가
  삭제됐거나 `attach_mapping` 이 그 사이 사라지면 스케줄을 정리하고 skip 한다.
  - **GUI는 기다리지 않고 예약한다.** CLI의 `Backoff::sleep()` 대신 `current()`와
    `advance()`로 다음 시각을 계산하고 프레임마다 확인한다.

  - **성공/실패 처리(`drain_auto_attach_results`)**: `is_reconnect` 플래그로 신규
    attach(`start_gui_attach`)와 재연결(`reconnect_session`, survivor mapping — 아래
    "재연결 시 세션 상태 보존" 참고)을 분기한다. 재연결 성공 시 `Remote.reconnect`
    슬롯을 제거(다음 disconnect 때 새로 시작)한다. 실패는 `on_reconnect_attempt_failed`
    로 처리: 에러 메시지에 `already_attached` 가 포함되면(다른 클라이언트가 그 원격
    워크스페이스를 여전히 점유 — 영구적 충돌일 수 있음) 지수 증가 없이 max(30초) 간격에
    고정해 폭주 없이 계속 대기하고, 그 외(네트워크/SSH 등 일시적 실패)는 통상적인 지수
    백오프(0.5s→30s)로 늘린다. 간격에는 ±20% jitter 를 둬 여러 anchor 가 동시에 끊겼을
    때 재시도가 한 tick 에 몰리는 것을 막는다. 시도 횟수가 `MAX_RECONNECT_ATTEMPTS`(20)에
    도달하면 `ReconnectSlot.given_up` 을 세우고 안내 toast(`mirror_reconnect_giveup`)를
    1회 띄운다 — `Remote.pending_reactivation` 은 건드리지 않으므로 사용자가 그
    워크스페이스를 왕복하는 수동 재시도(엣지 트리거)는 계속 유효하다.
    - **give-up은 슬롯에 남긴다.** 슬롯을 지우면 `reconnect_due(None, _)`가 첫 시도로
      해석해 다시 시작한다. `given_up`이면 시각과 관계없이 자동 재시도를 막고, 사용자가
      해당 workspace로 돌아오는 `should_reset_given_up` 조건에서만 재개한다.
      실패 횟수는 이번 실패를 반영한 값이며 `attempts >= MAX_RECONNECT_ATTEMPTS`로 비교한다.

- **`detach_orphaned_mirror_sessions` 와의 관계**: 간섭 없음. `apply_attach_client_output`
  (disconnect 감지, anchor 있으면 `Reconnecting` 전이 / anchor 없으면
  `cleanup_mirror_workspace(sess, from_disconnect=true)`)과
  `detach_orphaned_mirror_sessions`(사용자가 mirror ws 자체를 닫은 고아 세션 정리,
  `from_disconnect=false`)는 서로 다른 세션 식별 기준(전자는 `disconnected` 플래그, 후자는
  `local_workspace` 를 들고 있는 engine 이 창 있는 쪽·parked 쪽 어디에도 없음)으로 동작하고, 둘 다 단일 스레드 메인루프에서
  순차 실행돼(레이스 없음) 겹치는 세션이 있어도 먼저 처리한 쪽이 세션을 vec 에서 제거해
  뒤쪽은 자연히 skip 한다. `from_disconnect` 플래그가 두 경로를 구분해 사용자가 mirror ws 를
  직접 닫은 경우는 `Remote.pending_reactivation`/`Remote.reconnect` 에 들어가지
  않는다(재진입 대기·재연결 스케줄 의미가 없음 — 사용자 스스로 걷어낸 것).

**서버측 점유 해제(이 문서)와는 독립이다**: 위 backoff 재연결은 순수 **client(GUI mirror)측** 복원력이고, 서버의 `OccupancyRegistry` 해제 타이밍(EOF-or-TTL, 위 "점유 레지스트리" 절)은 전혀 건드리지 않는다 — TTL 만료로 서버가 이미 lock 을 free 로 되돌린 뒤에도 client 는 그저 다시 처음부터 attach 를 재시도할 뿐이고, 재연결 도중 다른 client 가 그 lock 을 먼저 잡으면 `already_attached` 로 거부돼 위 영구 충돌 backoff 로 흡수된다. 서버가 원 client 를 위해 lock 을 더 오래 붙들어주는 "재연결 유예 창구"는 도입하지 않았으므로 서버가 점유를 유지하는 유예 기능과 구분해야 한다.

## 재연결 시 세션 상태 보존

backoff 재연결이 매번 `start_gui_attach` 로 완전 신규 mirror workspace/터미널을
만들면 scrollback 과 local id 가 재연결마다 사라진다. 이를 막기 위해 `reconnect_session`
(`src/app/attach_client/connection.rs`)은 endpoint 해소 뒤 신규 연결과 같은 Remote handshake 경로를
수행하되, 워크스페이스/터미널은 **기존 것을 그대로 재사용**한다:

- **`merge_survivor_mapping`**: 기존 `apply_mirror_structural_delta` 의 survivor-mapping
  로직(옛 remote_id→local_id 매핑과 새 원격 구조를 비교해 양쪽에 다 있는 surface 는
  local id/Terminal 인스턴스를 그대로 재사용)을 공용 함수로 추출해, `start_gui_attach`(빈
  old_map)·`apply_mirror_structural_delta`·`reconnect_session` 세 곳이 공유한다.
  `reconnect_session` 은 재연결 직전의 `remote_to_local` 매핑을 old_map 으로 넘겨 재연결
  전후 살아있는 surface 의 scrollback/local id 를 보존한다.
- **`SharedFrameSender`(`Arc<ConnectionSender>`)**: sender는 원 연결의 bounded queue를 가리킨다. 연결 epoch와 mapping/grant binding을 확인한 요청만 전송하며, 옛 입력을 재연결 뒤 새 sender로 다시 해소하지 않는다. 재연결은 새 transport를 설치하고 살아남은 표시 ID를 재사용할 수 있지만, 이전 연결의 pending 입력·frame 상태를 successor로 넘기지 않는다.
- **`SessionState`(`Connected`/`Reconnecting`)**: 기존 `cleanup_mirror_workspace` 는
  disconnect 즉시 mirror workspace/터미널을 통째로 걷어내, 재연결 트리거가
  발동할 시점엔 survivor-mapping 을 적용할 대상 자체가 남아있지 않았다. 이를 막기 위해 anchor 가 있는 세션의 disconnect 는 `cleanup_mirror_workspace`
  를 부르지 않고 `enter_reconnecting`(mirror workspace/터미널을 그대로 둔 채 상태만
  `Reconnecting` 으로 전이, `Remote.active` 에서 제거해 재연결 트리거가 자유롭게
  spawn 하게 함)으로 분기한다. anchor 가 없는 세션(임시 mirror)은 기존처럼 즉시
  `cleanup_mirror_workspace`.
- **늦은 결과**: App은 endpoint 결과의 AttemptToken, 원 View/engine 설치 대상, anchor와 현재 매핑을 확인한다. 닫히거나 변경된 대상의 결과는 `Remote::discard_endpoint_outcome`으로 넘긴다. 원 tunnel은 Remote retirement worker가 보유해 kill/wait하며 App drain에서 blocking Drop하지 않는다. 원 worker와 retirement receipt의 실제 join 관측이 끝나야 회수가 완료된다.

## mirror 세션 종료 (client → 원격 점유 해제)

client 가 mirror 를 걷어내면 원격에 `Detach` 를 보내 원격 점유(hard workspace lock)를 해제해야 한다. 종료 트리거는 두 가지다.

- **원격발 종료(EOF/force-detach/read timeout)**: reader 스레드가 `Detach`/`force_detached`/EOF 를 받거나(위 "연결 생존 확인" 참조) read timeout 으로 소켓 read 가 에러를 반환하면 `disconnected` 플래그가 선다. `apply_attach_client_output` 이 이를 상태별로 분기한다 —
  - **anchor 있는 세션(매핑된 워크스페이스)**: `cleanup_mirror_workspace` 를 부르지 않고 `enter_reconnecting` 으로 전이(위 "재연결 시 세션 상태 보존" 참고) — mirror workspace/터미널을 살려둔 채 `Reconnecting` 상태로 두고 backoff 재연결에 맡긴다.
  - **anchor 없는 세션(임시 mirror, IPC `remote.attach` 등)**: 기존과 동일하게 세션 제거 + `cleanup_mirror_workspace(sess, from_disconnect=true)`. 그 mirror 가 창의 유일한 워크스페이스였으면 정리 뒤 기본 터미널 워크스페이스를 다시 만든다(`MainViewState::recreate_workspace_if_empty`) — 창을 닫지도, 워크스페이스 0 개로 남기지도 않는다(이 문서).
  - 이미 `Reconnecting` 상태인 세션(재연결 시도 자체가 실패해 다시 disconnected 로 관측되는 경우)은 이 분기에 다시 들어오지 않는다 — `disconnected && state == Connected` 조건이라 진입 시 이미 걸러진다.
- **로컬발 종료(사용자 연결 끊기)**: mirror 카드의 **워크스페이스 연결 끊기**가 `Intent::DisconnectMirror`로
  `App::disconnect_mirror_workspace`(`attach_client/workspace_close.rs`)를 부른다. 에이전트용 IPC `remote.disconnect`
  (`app/ipc/app_methods/remote.rs`, CLI `tasty remote disconnect`)도 같은 함수를 부른다. `Connected`와 `Reconnecting`
  모두 같은 경로다. 먼저 `cancel_mirror_attempts`가 `reconnect`가 그 mirror를 가리키는 endpoint·connection 시도를
  취소하고(run은 `cancelled`), 세션을 vec에서 뺀 뒤 `end_mirror_locally`가 `cleanup_mirror_workspace(.., false)`로
  정리하고 transport를 drop해 socket·worker·소유 터널을 retirement에 넘긴다. anchor는 `pending_reactivation`에 다시
  넣고 `Remote.last_active_ws`를 정리 뒤의 활성 workspace로 맞춘다. 정리로 선택이 anchor로 옮겨져도 그 프레임이
  전환 엣지가 되지 않아, 사용자가 다른 workspace를 거쳐 anchor로 돌아올 때만 다시 attach한다.
  mirror의 **워크스페이스 닫기**는 이 경로가 아니라 아래 "서버 workspace 닫기"다.
- **서버 workspace 닫기**: `build_mirror_forward_op`가 `DomainIntent::CloseWorkspace`를 `StructuralOp::CloseWorkspace`로 바꾼다.
  `apply_domain_intent`는 admission 전에 `workspace_close_refusal`로 연결(`Connected`·활성 epoch)·capability
  (`ipc.stream.workspace-close`, `ClientTransport.workspace_close`)·대기 중 닫기를 확인하고, 거절이면 보내지 않고 안내한다.
  `prepare_journal_forward`가 같은 판정을 다시 하고, 실제 전송 성공 때 세션이 `WorkspaceClose::Pending(op_id)`가 된다.
  준비 단계 판정은 아직 보내지 않은 닫기를 모르므로 `execute_journal_forwards`가 전송 직전에 `send_refusal`로 `Idle`이 아닌
  세션의 닫기 draft를 `Cancelled`로 끝낸다. 전송은 이 루프에서 하나씩 일어나 두 번째 닫기가 기다리던 op id를 덮어쓰지 않는다.
  그 op의 `StructuralSucceeded`만 `Closed`로, `StructuralFailed`는 `Idle`로 바꾼다. 출력 루프의 `session_end`는 `Closed`를
  설치 continuation과 단절 판정보다 먼저 보고 `ServerClosed`로 정리한다. 서버는 성공 회신 뒤 `force_detach_workspace`로
  `force_detached`와 EOF를 보내므로 그 EOF가 같은 묶음이든 다음 tick이든 재연결로 가지 않는다(`output/close_order_tests.rs`).
  `Pending`인 채로 단절이 처리되면 결과를 모른다는 안내를 띄우고 기존 단절 처리를 따른다. 재연결 설치는 `Idle`로 초기화한다.
  정리는 연결 끊기와 같은 `end_mirror_locally`이며, 그 전에 `remember_closed_mapping`이 anchor 매핑을 `Remote.closed_mappings`에
  기록해 `auto_attach_candidate`가 같은 매핑으로 다시 attach하지 않게 한다. 매핑이 바뀌거나 사라지면 `prune_first_attach_retries`가 지운다.
  - 서버의 `resolve_remote_request`는 op를 `intent.close` + `CloseTarget::Workspace(held)`로 바꾼다. `close.rs::Request::workspace`는
    `Cause::RemoteHolder`이면 holder와 대상 workspace를 확인하고, 그 holder 자신의 hard 점유는 허용하되 다른 client의 hard 점유는
    거절한다. 서버의 마지막 workspace면 replacement workspace를 만든다. 서버 로컬 IPC `workspace.close`의 거절(점유·마지막)은 그대로다.
  - `deliver_result`는 workspace 닫기 회신이 `closed:false`면 실패로 돌려준다. 그 밖의 성공 회신은 기존 순서(결과 → 강제 분리)를 따른다.

  `App::detach_orphaned_mirror_sessions`는 `about_to_wait`에서 세션의 `local_workspace`를
  가진 engine이 있는지 `mirror_workspace_engine_alive`로 확인한다.
  창에 속한 engine과 parked engine을 모두 찾고, 없으면 `cleanup_mirror_workspace`로 정리한다.
  창이 없어도 parked engine에 mirror 워크스페이스가 남아 있다면 세션을 유지한다
  ([창 없는 상태의 세션 수명](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명)).
  attach 설정 중에는 같은 동기 함수에서 워크스페이스를 만든 뒤 세션을 추가하므로,
  아직 워크스페이스를 만들지 않은 세션이 이 검사에 걸리지는 않는다.
- **`cleanup_mirror_workspace`(공용, `from_disconnect: bool` 파라미터로 위 두 트리거 구분)**: mirror ws·터미널·mirror busy 엔트리·mesh 프레임 캐시 제거(창 있는 engine → parked engine 순으로 찾는다. 고아 판정과 순회 범위가 같아야 잔류가 없다. 이미 없으면 skip) → 원격에 `Detach` push → anchor 게이트(`Remote.active`) 해제 → 원 transport와 터널을 Remote retirement에 넘겨 실제 회수 관측. `from_disconnect=true`(원격발, anchor 없는 세션 한정)일 때만 anchor 를 `Remote.pending_reactivation` 에 추가(위 "GUI 자동 재연결 스코프" 참고) — `false`(로컬발/사용자 close)는 그 항목과 `Remote.reconnect` 스케줄 모두 명시적으로 제거해 이미 걷어낸 세션에 대한 재연결 시도를 남기지 않는다. 원격은 `Detach` 수신 시 read loop break → `Disconnected` → `release_all_for_client`(workspace+surface lock 해제).
- **적용 순회도 같은 범위다**: `apply_attach_client_output` 은 mirror 이벤트의 적용 대상을 창 있는 engine → parked engine 순으로 찾고(`mirror_output_host`), 찾은 뒤에만 reader 버퍼를 drain 한다 — 창이 없는 parked 구간에 도착한 출력·구조 delta 도 그 engine 에 즉시 적용된다(docs/dev-guide/attach-behavior.md). 고아 판정·정리·적용 세 순회의 범위가 같아야 "살아 있다고 판정된 engine 에 적용이 닿지 않는" 유실 구간이 생기지 않는다.

## force-detach

서버 권한으로 점유 강제 해제(attach 하지 않음). holder client 에 `Control{force_detached}` + `Detach` push → client 가 mirror 정리·종료, 서버는 lock free. 엔진 함수는 `EngineMut::force_detach` / `force_detach_workspace`(`src/runtime/engine_access.rs`)다.

- **로컬 사용자 GUI**: 점유 테두리의 강제 끊기 버튼(`EngineAction::DetachSurface` → `release_occupancy`)과 사이드바 우클릭 확인 팝업(`EngineAction::DetachWorkspace`). 두 intent 모두 확인 시점의 `granted_seq` 가 같을 때만 푼다 — 그 사이 다시 붙은 다른 점유를 끊지 않는다.
- **내부 정리**: 연결 종료 일괄 해제(`release_all_for_client`)와 workspace 제거에 따른 분리는 같은 엔진 함수를 쓴다.
- **공개 IPC·CLI 없음**: release 에는 남의 점유를 강제로 푸는 IPC 메서드나 CLI 플래그가 없다. 에이전트가 원격 사용자를 쉽게 끊는 전용 인터페이스를 두지 않는다는 [사용자 통제와 데이터 원칙](../principles/control-and-data.md)을 따른다. 토큰 없는 CLI·네트워크 호출자(Local)도 사람의 GUI 조작을 증명하지 않으므로 Local 전용으로 남기지 않는다. 점유자 본인은 `attach.release`(self-release)나 연결 종료로 끊는다. 헤드리스 서버처럼 GUI 가 없는 곳의 로컬 사용자는 원격 세션을 OS·SSH 수단(sshd 세션 종료 등)으로 끊을 수 있다 — 원칙은 이런 우회를 막지 않는다.
- **debug 재현**: `debug.attach.force_detach{surface_id}` / `debug.attach.force_detach_workspace{workspace_id}`(local 전용, `src/adapters/ipc/handler/debug_attach.rs`)와 `tasty debug attach [SURFACE|--workspace <id>] --force-detach`([debug IPC](debug-ipc.md)).

## 원격 생존 확인 (`remote check`)

`crates/tasty-cli/src/local/remote_check.rs`. 포트 발견만으론 **stale 포트 파일**(죽은 인스턴스 잔재)을 오판하므로 3단계: ① 포트 발견 → ② `ssh -L` 터널 → ③ 터널 localport 로 `system.info` **1회**. 응답이 와야 alive(stdout `alive: …`, exit 0). 거부/EOF/타임아웃 = dead(stderr, exit≠0). 단계별 상한은 ① 포트 발견 45초(위 "SSH 터널" 절의 3겹 상한) → ② 터널 ready-probe 5초 → ③ IPC 프로브 5초(`remote_check.rs` 의 `PROBE_TIMEOUT`) — 어느 단계도 무한 대기하지 않으므로 "단발 판정" 계약이 전 구간에서 성립한다. 1회성이라 재연결 백오프 없음. SSH 부품은 attach 와 공유, list 핸들러는 안 건드림.

## 원격 자식 에이전트 (`ipc.stream.agent-children`)

attach 로 연결한 mirror 에 `tasty claude spawn`·`tasty codex spawn` 으로 자식을 만들고, 그 자식을 client 의 자식처럼 tell·children·state·kill 로 다룬다. 자식 프로세스는 서버에서 서버의 provider 설치·설정·토큰으로 돈다. 부모·완료 알림 훅·Codex 결과 수신처는 client 의 mirror surface(L)에 묶인다. provider plugin 이 그 위에서 하는 일은 [원격 attach 자식](../plugins/remote-attach-children.md)에 있다.

- 이 기능은 기존 attach holder 연결의 제어 채널만 쓴다. 별도 서버·포트·부모 주소 등록을 만들지 않고, 훅 명령에 부모 Tasty 주소나 비밀을 넣지 않는다. 메시지는 아래 [커스텀 이벤트 확장](#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그)과 같은 raw JSON `event` 다(`crates/tasty-ipc/src/agent_child.rs`).
- 서버는 workspace attach descriptor 의 `stream_capabilities` 에 `ipc.stream.agent-children` 을 싣는다. client 는 이 이름이 있는 연결에만 보낸다. 없는 구 서버에 대한 `attach.agent_target` 은 `capable: false` 로 답하고, plugin 은 터미널을 만들지 않고 오류로 끝낸다.
- 실행할 수 있는 plugin 과 메서드는 각 plugin 매니페스트의 [`contributes.remote_attach`](plugin-development.md#원격-attach-실행-contributesremote_attach) 선언으로만 정한다. host 에는 provider 이름·메서드 목록·훅 이름이 없다.

### 출처

- client 는 holder 연결마다 `agent_hello` 를 보내고, 서버는 그 연결에 묶인 비공개 128비트 출처 토큰과 공개 출처 번호를 `agent_hello_ack` 로 돌려준다. 재연결 때 같은 토큰을 내면 같은 출처로 이어진다(`resumed`). 공개 `instance_id` 나 연결 ID 로는 출처를 정하지 않는다.
- 서버 child registry 에서 원격 출처 자식의 부모 키는 `attach:o<n>`(공개 순번)이다. 토큰은 registry·응답·훅에 싣지 않는다.
- 서버 프로세스가 다시 시작하면 출처 기록이 없어진다. 그 전에 만든 자식의 결과·통지는 새 출처로 이어지지 않으며 결과불명이다. 연결만 끊겼다가 같은 서버 프로세스에 다시 붙으면 쌓인 통지를 이어서 받는다. 서버 상태 전체를 디스크에 남겨 재시작 뒤 복원하지는 않는다.

### 원격 call

1. client plugin 이 `attach.agent_target{workspace|pane|surface_id}` 로 mirror 와 기준 surface(`anchor_surface_id`)를 확인한 뒤 `attach.agent_call{anchor_surface_id, method, target_surface_id?, parent?, role?, nickname?, params}` 를 부른다. `method` 는 호출 plugin 자기 namespace 의 접두사 없는 메서드 이름이고, `target_surface_id` 는 tell·kill 이 다룰 기존 원격 자식의 mirror surface(L)다. 응답은 `{call_id, result, children:[{surface_id, remote_surface_id, index}]}` 다. client host 는 결과를 받을 때까지 응답을 미루며 상한은 35초다(plugin SDK 전용 핸들 상한 40초보다 짧다).
2. client host 는 `agent_call_request{call_id, namespace, method, anchor_surface_id, target_surface_id?, params}` 를 holder 연결로 보낸다. 두 surface 는 서버 쪽 id(R)로 바꿔 싣고, 실행 위치(workspace·pane)는 서버가 `anchor_surface_id` 에서 정한다. 부모·`_tasty_context`·`caller_surface` 는 보내지 않는다. `call_id` 는 출처 안에서 유일하고, 같은 `call_id` 를 다시 보내면 서버는 다시 실행하지 않고 보관한 결과를 돌려준다.
3. 서버 host 는 그 plugin 의 `remote_attach.methods` 에 있는지, workspace 를 이 holder 가 점유하는지 확인한다. 인자에서 host 소유 이름(`parent`·`surface`·`parent_external`·`workspace`·`pane`·`caller_surface`·`attach_context`·`attach_target_surface`·`_tasty*`)을 지우고 host 가 정한 값을 넣는다. spawn 에는 `parent_external = attach:o<n>`·`attach_context`(call 문맥 토큰)·`workspace`·`pane` 을, tell·kill 에는 확인한 대상 R 을 `attach_target_surface` 로 넣는다.
4. 서버 host 는 그 호출을 일반 namespace 진입점이 아닌 원격 전용 전달로 plugin 에 넘긴다. plugin 의 host 호출은 매니페스트 권한·사용자 허가 검사를 그대로 받는다.
5. 서버 plugin 이 `terminal.spawn` 에 `attach_context` 를 실으면 host 는 문맥 토큰이 그 출처·그 plugin·그 workspace 의 진행 중인 call 인지 확인한 뒤에만 `attach:o<n>` 부모로 등록한다. 만든 자식(R)은 그 call 의 결과에 귀속한다. 같은 출처의 call 이 겹쳐도 각 call 은 자기가 만든 자식만 받는다.
6. 서버는 `agent_call_result{call_id, outcome, spawned}` 를 보낸다. client 는 `spawned` 의 R 이 mirror 에 나타나면 L 로 바꿔 client child registry 에 plugin 부모 아래로 등록한 뒤 plugin 에 답한다.
7. plugin 이 L 에 수신처를 묶은 뒤 `attach.agent_commit{call_id}` 를 부르면 client 는 `agent_call_commit{call_id, remote_surface_ids}` 를 보낸다. 서버는 commit 전에는 그 자식의 통지를 보내지 않으므로 첫 완료가 수신처 없이 지나가지 않는다.

결과를 확정할 수 없으면 client host 는 성공이나 실패로 단정하지 않고 `-32010`(결과불명)으로 답한다. 35초 상한, 연결 종료, 서버 재시작이 이 경우다.
- 상한 뒤에도 그 call 은 10분 동안 결과를 기다린다. 결과가 와서 자식이 mirror 에 나타나면 plugin 의 `receive` 메서드에 `kind: late_spawn` 으로 넘기고, plugin 이 `recorded` 로 답하면 host 가 commit 한다. plugin 이 받지 않으면(`rejected`) 그 자식을 원격 kill 로 닫는다.
- mirror 에 나타난 자식을 client child registry 에 등록하지 못하면 성공으로 돌려주지 않는다. 그 자식은 원격 자식으로만 기록해 원격 kill 대상이 되게 하고, plugin 에는 오류로 답한 뒤 `late_spawn` 으로 넘긴다. plugin 은 기다리는 부모가 없으므로 그 자식을 원격 kill 로 닫는다.
- 결과에 자식이 있는데 mirror 에 아직 나타나지 않았다는 것만으로 자식이 닫혔다고 보지 않는다. 그 call 은 나타날 때까지 기다리고, 상한이 지나면 결과불명으로 답한다. 끝내 나타나지 않으면 10분 보관 기한에 기록이 정리된다.

### 통지

서버는 원격 출처 자식의 변화를 `child_notice{key, seq, remote_surface_id, notice}` 로 client 에 보낸다. 같은 `key` 는 같은 통지의 재전송이다. `seq` 는 서버 프로세스 안의 순서이며 client 는 State 통지에만 써서 오래된 상태가 새 상태를 덮지 않게 한다.

| 통지 | 서버가 보내는 조건 | client 가 하는 일 |
|---|---|---|
| State | 항상 | L 의 상태·근거 갱신 |
| Event | `process-exit`, 또는 자식을 만든 plugin 의 `hook_events` 선언에 있는 이벤트 | L 에 같은 이벤트로 훅 발화(`fire_as`) |
| Meta | 자식을 만든 plugin 의 `relay_meta` 선언에 있는 키 | L 의 surface 메타 갱신 |
| Plugin | 자식을 만든 plugin 이 `attach.child_notice` 로 맡긴 결과 | 같은 plugin 의 `receive` 메서드에 `kind: notice` 로 전달 |

- 통지는 자식마다 FIFO outbox 에 쌓이고 한 번에 하나씩 나간다. client 는 `child_notice_ack{key, outcome}` 으로 답하며, 서버는 `recorded`·`duplicate`·`rejected` 를 받은 뒤에만 지운다. `retry_later` 는 짧은 간격 뒤 다시 보낸다. 연결이 끊기면 보낸 통지를 다음 연결에서 다시 보낸다. 다른 출처는 그 통지를 받거나 확인할 수 없다.
- client 는 key 를 세션마다 기억해 같은 통지를 한 번만 적용한다. key 에는 서버 프로세스 접두사가 붙어 서버가 재시작한 뒤의 key 와 겹치지 않는다.
- `attach.child_notice` 는 plugin 결과를 맡긴 상태를 `pending`·`delivered`·`rejected` 로 답한다. 같은 key 를 다시 부르면 먼저 그 key 의 기록된 상태를 돌려준다.
  - 새 key 는 이 서버 프로세스의 자식 기록(출처·만든 plugin·workspace)으로 쌓는다. 자식 surface 가 닫혀도 기록은 10분(client tombstone 과 같은 기간) 남으므로, 닫힌 직후 확정된 종료·마지막 턴 결과도 맡길 수 있다. holder 가 잠시 끊겨 있어도 `pending` 으로 쌓아 두고 재연결 뒤 보낸다.
  - 기록이 없으면(그 plugin 이 만든 원격 자식이 아님, 서버 재시작, 보존 기간 지남) 다시 보내도 받을 수 없으므로 오류 대신 `rejected` 와 사유를 돌려준다. plugin 에 `remote_attach` 선언이 없어도 `rejected` 다. 형식이 틀린 요청만 JSON-RPC 오류다.
- mirror surface 가 사라지면(원격 자식 종료, mirror 정리) client 는 그 자식의 훅을 10분 동안 tombstone 으로 남겨 늦게 온 통지를 받아 준다. 그 사이 온 Event 는 surface 없이 같은 훅으로 발화한다(`fire_detached`).

### 서버 점유와 공개 호출

- 원격 출처 자식은 서버에서 hard 점유된 workspace 안에 있다. 서버 host 는 자식을 만든 plugin 이 그 출처의 call·훅 처리로 하는 입력만 점유 예외로 받는다. 서버 사용자의 키 입력·`surface.send`·다른 plugin 의 입력은 기존처럼 막힌다.
- 서버의 공개 IPC·CLI·다른 plugin 이 보낸 일반 namespace 요청은 원격 출처 표지(`attach_context`·`attach_target_surface`·`attach:` 로 시작하는 `parent_external`)를 실을 수 없다. 실려 있으면 host 가 plugin 에 넘기기 전에 `-32602` 로 거절한다. 호출자 plugin ID 만으로 원격 출처로 보지 않는다. `tell`·`kill`·`respawn` 같은 일반 메서드가 `attach:` 부모의 자식을 대상으로 하는 경우는 provider plugin 이 거절한다([원격 attach 자식](../plugins/remote-attach-children.md#서버s-plugin)).
- 원격 kill 은 서버에서 `Cause::RemoteHolder` 로 닫는다.

### client 표시

- `terminal.children`·`terminal.state` 는 원격 자식 L 에 `remote: true` 와 `liveness` 를 싣는다. 연결 중이면 `connected`, 연결이 끊겨 재연결을 기다리는 동안은 `unknown` 이다. `unknown` 이면 `state` 를 `unknown`, `evidence` 를 `remote_disconnected`, `confidence` 를 `unobserved` 로 바꿔 마지막 상태를 현재 상태처럼 보이지 않게 한다.
- 세션이 끝나면 그 세션의 진행 중 call 은 결과불명으로 끝나고, mirror 자식은 정리될 때 client child registry 와 훅에서 빠진다.

### CLI 경로 인자

plugin CLI 의 `path_kind` 인자는 보통 CLI 가 자기 디렉터리 기준 절대 경로로 바꾸고 검사한다. 원격 실행을 선언한 메서드(`remote_attach.methods`)에 경로 인자와 숫자 `--workspace` 또는 `--pane` 이 있으면, CLI 는 보내기 전에 `attach.agent_target` 으로 대상이 mirror 인지 묻는다. mirror 면 경로를 바꾸지 않고 그대로 보내 서버가 확인하게 한다. 그 밖의 명령(예: 숫자 이름을 받는 `launch --workspace`)과 로컬 대상은 기존 처리 그대로다. 질문이 실패하면 로컬로 다룬다.

서버 host 는 원격 holder spawn 의 `cwd` 가 절대 경로가 아니면 거절한다. 상대 경로를 서버 프로세스의 작업 위치로 풀지 않기 위해서다.

### 한계

- 서버 재시작 뒤에는 그 전에 만든 원격 자식의 결과를 이어 받지 못한다(위 출처 절). 닫힌 자식의 결과는 닫힌 뒤 10분까지만 맡길 수 있다.
- 결과불명 call 의 늦은 재결합은 plugin 이 그 call 의 부모를 아직 기억할 때만 된다.
- 재연결은 같은 컴퓨터의 두 인스턴스 사이에 둔 TCP 중계를 끊었다 같은 주소로 되살려 확인했다(Linux). 끊긴 동안 `liveness: unknown`, 그동안 서버에 쌓인 통지는 복구 뒤 한 번만 기록됐고 자식 매핑은 그대로였다. SSH 터널 경유와 force-detach 뒤 재연결은 실제 실행으로 확인하지 않았다.
- 구 서버에 대한 거절은 capability 판정과 plugin 단위 시험으로 확인했고, 구 버전 서버 실제 실행으로는 확인하지 않았다.

## mirror 터미널 읽기 (`ipc.stream.terminal-read`)

mirror 터미널은 attach snapshot(보이는 화면·커서·주요 모드)과 그 뒤의 출력으로 화면을 다시 만든다. 보이는 화면은 원본과 같지만 그 출력 보관 버퍼는 원본이 아니다. attach 이전 원문과 scrollback 이 없고, 버퍼 앞에는 합성 snapshot 바이트가 있으며, 손실(`Loss`) 뒤 재동기화로 버린 원문은 `skipped` 에 남지 않는다. 그래서 client 는 mirror 터미널의 `surface.screen_text`·`surface.read_since_mark`·`surface.set_mark` 를 holder 연결로 서버 원본에 보낸다. 시험 `tasty-terminal` 의 `a_mirror_matches_the_visible_screen_but_not_the_output_ring_or_scrollback` 가 이 차이를 고정한다.

- **capability**: 서버는 workspace attach descriptor 의 `stream_capabilities` 에 `ipc.stream.terminal-read` 를 싣는다. 없는 구 서버의 mirror 에서 세 메서드는 로컬 사본으로 답하지 않고 `-32000`·`data.reason: "remote_read_unsupported"` 로 답한다. 연결이 Connected 가 아니면 `remote_disconnected` 다. mirror 가 아닌 surface 와 헤드리스 인스턴스는 기존 로컬 경로 그대로다.
- **wire**: client → 서버 `terminal_read_request{request_id, surface_id(원격), op}`, 서버 → client `terminal_read_result{request_id, outcome}`(`crates/tasty-ipc/src/terminal_read.rs`). op 는 `screen_text{lines?, show_dim}`·`output{from, max_bytes?, strip_ansi}`·`position` 이다. `from` 은 `cursor{cursor, stream}` 또는 `start` 이며 서버의 공유 마크는 읽지도 바꾸지도 않는다. 결과는 로컬 IPC 응답과 같은 `result` 또는 `code`·`message`·`data(reason)` 다.
- **대상 검사**: 서버(`src/remote/terminal_read.rs`)는 GUI 의 창·parked engine 과 헤드리스 engine 중 그 터미널이 있는 engine 을 먼저 찾고, 요청 연결이 그 터미널의 hard lock holder 일 때만 읽는다. 다른 workspace 를 점유했다는 사실만으로는 읽지 않는다(`not_attached`). 어느 engine 에도 터미널이 없으면 `no_terminal` 이다.
- **추출**: 화면은 `surface.screen_text` 핸들러, 원문은 `surface.read_since_mark` 와 같은 `read_output_response` 를 서버 터미널에 실행한다. 그래서 `dim_omitted_rows`·`scrollback_len`·`alt_screen`·`stream`·`retention_*`·`skipped`·오류 reason 이 원본 값이다.
- **프레임 크기**: 결과 한 프레임의 JSON 은 `MAX_FRAME_LEN`(1 MiB)의 절반 이하로 둔다. 원문은 제어 문자가 JSON 에서 최대 6바이트가 되므로 서버가 한 번에 읽는 원문을 256 KiB 이하로 줄이고, 그래도 넘으면 절반씩 줄여 다시 읽는다. 줄인 만큼은 `next_cursor < retention_end` 로 남으므로 기존처럼 `next_cursor` 로 이어 읽는다. 화면 읽기가 넘으면 줄 수를 몰래 줄이지 않고 `too_large` 로 거절한다.
- **client 대기**(`src/remote/terminal_read/client.rs`): 핸들러는 요청을 넣고 `attach.agent_call` 처럼 응답을 미룬다(상한 10초, 넘으면 `remote_timeout`). 메인 루프의 `about_to_wait` 와 attach 출력 처리가 `pump_terminal_reads` 로 세션 상태를 알리고 요청을 보낸다. 요청은 보낸 연결 세대(`ConnectionEpoch`)를 기록하며 다른 세대로 온 결과는 버린다. 세션이 Connected 가 아니게 되거나 세대가 바뀌면 대기 읽기는 `remote_disconnected` 로 끝난다. 응답의 `surface_id` 는 로컬 mirror id 로 바꾸고 `remote_surface_id`·`remote: true` 를 붙인다.
- **mirror 마크**: client 는 (mirror workspace, 원격 surface) 마다 서버 stream·위치를 보관한다. `set_mark` 는 서버에 `position` 을 묻고, 답을 받은 뒤 로컬 마크(로컬 `parse_since_mark` 용)와 mirror 마크를 함께 바꾼다. 실패하면 둘 다 그대로다. cursor 없는 `read_since_mark` 는 mirror 마크부터, 마크가 없으면 서버 보관 시작부터 읽는다(잃은 앞부분은 `skipped`). 마크의 stream 이 서버와 다르면(서버 터미널 교체) 처음부터 조용히 읽지 않고 `stream_mismatch` 로 답하며 마크를 지우고 set mark 를 다시 하라고 알린다. 호출자가 준 cursor 의 오류는 바꾸지 않고 그대로 전달한다.
- **범위 밖**: `parse_since_mark`·`read_since_scan_mark`·명령 인덱스·output observe 는 mirror 에서 로컬 사본 기준이다. 화면 렌더도 스트림 재구성이다.

## 원격 파일 다운로드 (`ipc.stream.file-download`)

"다운로드 후 실행"([원격 attach](../features/remote-attach/index.md#원격-파일-다운로드-후-실행))이 쓰는 경로 지정 파일 읽기다. 별도 포트나 서비스 없이 holder 연결의 Control 프레임으로 오간다.

- **capability**: 서버는 workspace attach descriptor 의 `stream_capabilities` 에 `ipc.stream.file-download` 를 싣는다. client 는 이 값이 있는 연결에만 요청을 보낸다. 구 서버는 이 event 를 모르고 버린다.
- **wire**(`crates/tasty-ipc/src/file_download.rs`): client → 서버 `file_download_request{request_id, path(서버 호스트 경로), op}`, 서버 → client `file_download_result{request_id, outcome}`. op 는 `stat` 과 `read{offset, identity}` 다. outcome 은 `stat{name, identity}`·`chunk{offset, data_b64, eof}`·`refused{reason}` 이다. `identity` 는 크기·수정 시각(ns)·생성 시각(ns)·Unix (device, inode)이며 서버 OS·파일시스템이 주지 않는 값은 비운다. (device, inode)가 없는 Windows 서버에서는 생성 시각이 같은 크기·수정 시각의 새 파일을 가려낸다. 생성 시각은 나중에 더한 선택 필드라 capability 이름·버전은 그대로다. 이 필드를 보내지 않는 상대와 주고받으면 양쪽 모두 비어 같은 값으로 비교한다. 거절 이유는 `directory`·`not_regular`·`not_found`·`permission_denied`·`changed`·`not_attached`·`io` 다.
- **서버**(`src/remote/file_download.rs`): 요청 사이에 상태를 두지 않는다. 요청 연결이 어느 workspace 든 점유하지 않았으면 `not_attached` 다. `stat` 은 파일을 열지 않고 링크를 따라간 메타데이터만 본다. `read` 는 열기 전 메타데이터를 `identity` 와 대조하고, Unix 에서는 `O_NONBLOCK` 으로 열어 FIFO 로 바뀐 경로에서 멈추지 않으며, 열린 파일의 메타데이터를 다시 대조한다. 한 조각은 최대 256 KiB 이고 base64 로도 프레임 상한(`MAX_FRAME_LEN`, 1 MiB)의 절반 아래다. 덜 읽혔거나 읽는 동안 바뀌면 `changed` 다. GUI(`event_handler.rs`)와 헤드리스(`boot/headless_stream.rs`) 서버가 같은 처리부를 쓴다.
- **client 대기**(`src/remote/file_download/client.rs`): 다운로드마다 한 번에 요청 하나만 보낸다. 메인 루프와 attach 출력 처리가 `pump_file_downloads` 로 세션 상태를 알리고 요청을 보낸다. 요청은 보낸 연결 세대(`ConnectionEpoch`)를 기록하며 다른 세대로 온 결과는 버린다. 보낸 뒤 세션이 Connected 가 아니게 되거나 세대가 바뀌면 그 다운로드는 실패하고 받은 부분을 지운다. 크기가 1 MiB 를 넘으면 확인 전까지 `read` 를 보내지 않는다. 취소는 다음 조각을 요청하지 않는 것으로 끝난다.
- **로컬 실행**: 다 받은 파일은 origin 없는 사용자 `DispatchFile` 로 연다. 탭을 여는 `open_surface` 핸들러가 mirror 활성 상태에서 받은 파일을 열면 `NewTab` 대신 새 로컬 workspace 를 만드는 `CreateWorkspace` 를 보낸다(`src/file/dispatch.rs`). 그렇지 않으면 로컬 임시 경로가 원격으로 forward 된다.

## IPC 표면 (`attach.*`)

`src/adapters/ipc/handler/attach.rs`.

| 메서드 | 경로 | 설명 |
|--------|------|------|
| `attach.acquire` / `attach.release` | `stream.open{target}` 핸드셰이크 | 배타 lock 획득/해제 |
| `attach.into_gui` | JSON-RPC | 실행 GUI 가 원격 워크스페이스를 mirror 로 재구성(`App::start_gui_attach`). GUI 는 App 층이 자기 자신 여부를 판정한 뒤 답한다(위 self-attach 절). 헤드리스는 접수하지 않고 `-32017` 로 거절한다 |
| `attach.list` | JSON-RPC | 현재 점유 목록 조회(read) |
| `attach.agent_target` | JSON-RPC | `workspace`(숫자 id)·`pane`·`surface_id` 가 mirror 인지와 서버의 원격 자식 지원(`capable`)을 답한다(read). CLI 도 경로 인자를 다루려고 묻는다 |
| `attach.agent_call` / `attach.agent_commit` | JSON-RPC(plugin 전용) | 원격 call 실행과 수신처를 묶은 뒤의 commit(위 원격 자식 에이전트 절). 호출 plugin 의 `terminal.spawn`·`terminal.write` 권한이 필요하다 |
| `attach.child_notice` / `attach.child_notices` | JSON-RPC | 서버 plugin 이 원격 자식의 결과를 맡기고(plugin 전용, `terminal.write`) 남은 통지를 조회한다(read) |

**권한**: attach 보안은 **연결 경계(SSH + 127.0.0.1 loopback)** 에 위임. 자체 권한 레이어 없음 — 소켓에 도달한 caller 는 위 attach 제어를 호출할 수 있다. 호스트가 터널을 직접 여는 `remote.attach` 와 그 거절 조회 `remote.refusals` 도 같다. 권한 토큰을 요구하지 않으며 세션 토큰을 가진 에이전트도 호출할 수 있다. plugin 호스트 호출의 한계는 [원격 attach (IPC — focus 중립)](../features/remote-attach/index.md#원격-attach-ipc--focus-중립)에 있다. 원격 자식의 call·commit·결과 맡기기는 그 plugin 의 기존 터미널 권한을 그대로 요구한다. 남의 점유를 강제로 푸는 메서드는 이 표에 없다(위 force-detach 절). 근거는 [사용자 통제와 데이터 원칙](../principles/control-and-data.md), 주체 계약은 [actors](../concepts/actors.md).

## 커스텀 이벤트 확장 (`StreamControl` 밖 raw JSON `event` 태그)

`StreamControl` enum 을 건드리지 않고 같은 `StreamTag::Control` 채널에 raw JSON `{"event": "...",
...}` 를 얹어 요청/응답 쌍을 왕복시키는 패턴 — 프로토콜 자체를 확장하지 않고 attach 채널을 다른
용도(원격 디렉토리 나열, 스크린샷 업로드, git 조회 등)로 재사용할 때 쓴다. 인가는 전부 "attach 점유
= 신뢰"(`engine.attach.client_holds_workspace(client_id)`, docs/dev-guide/attach-behavior.md) — 별도 권한 게이트가 없다.
수신측(`StreamHub::pump_inbound`)은 알려진 `StreamControl` 파싱이 실패하면 이 raw 이벤트들을
순서대로 시도하고, 전부 실패하면 조용히 무시한다(전방 호환 — 모르는 이벤트를 에러로 취급하지
않음).

- **`list_dir_request`/`list_dir_result`**(docs/features/remote-attach/index.md) — 원격 디렉토리 나열(file picker). host popup
  wrapper 가 요청을 소유하고 응답을 직접 소비 — 소유자가 host 프로세스 내부라 `sent_at.elapsed()`
  로 매 프레임 soft timeout(8초)을 직접 판정할 수 있다. `dir` 가 비어 있으면 서버는 자기 홈을 읽고
  응답의 `dir` 에 그 경로를 싣는다. client 는 연결·재연결마다 이 형태로 원격 홈을 한 번 묻고
  (`home_probe.rs`), 응답을 explorer 주소창의 `~` 에 쓴다. 이 요청에는 소비자 surface 가 없고 응답의
  목록은 버린다. 응답의 `volume` 은 읽은 폴더가 있는 볼륨의 식별자다(Unix 는 장치 번호, Windows 는
  드라이브·UNC 접두에서 만든 값). 같은 서버에서 읽은 값끼리만 비교할 수 있고, 모르면 싣지 않는다.
- **`git_query_request`/`git_query_result`**(docs/features/remote-attach/index.md) — 원격 저장소 status/log/diff/worktrees 조회
  (git-viewer). 응답의 **소비자가 host 가 아니라 별도 프로세스인 plugin** 이라는 점이 file picker와
  다르다 — host(`src/app/attach_client/output/apply_event.rs`)는 payload 를 해석하지 않고 그대로 `PluginManager::
  emit_host_event_to_plugin`(Event Bus 의 owner-unicast 경로, 구독 등록 여부 무관 —
  `command.invoked` 와 동일 메커니즘)으로 `git_viewer.query_result` 이벤트를 plugin 에 전달만
  한다. 요청 트리거도 plugin→host `git_viewer.query` IPC(비동기 accept — `request_id` 만 즉시
  회신하고 실제 forward 는 `RemoteState.pending_git_query_forward` 를 다음 tick 에 drain)로 plugin
  이 직접 건다. **응답 대기 시간 제한(soft timeout)은 없다.** 파일 피커와 달리 요청을 관리하는
  플러그인이 별도 프로세스로 실행되며, 호스트의 프레임마다 대기 시간을 확인하는 처리가 없다. 세션 자체가
  끊기는 경우는 보내기 전(send-time 실패)이든 보낸 뒤(`cleanup_mirror_workspace` 가
  `request_id=0` sentinel 로 강제 abandon)든 즉시 `ok:false` 로 커버된다 — 빠진 건 "연결은 계속
  살아있는데 원격 tasty 프로세스 자체가 응답만 안 주는" 좁은 경우다. docs/features/remote-attach/index.md의
  원격 조회 한계를 참고한다. egui-mesh popup 은 `popup.set_context`
  가 dirty 상태 변경시에만 나가 비동기 push 만으로는 다음 프레임 렌더가 보장되지 않으므로,
  `MainViewState.plugin_mesh_popup_pending_repaint: HashSet<u64>` 로 강제 repaint 를 예약한다
  (`popup_render.rs` 의 dirty OR-체인에 합류).
- **`explorer_op_request`/`explorer_op_answer`/`explorer_op_cancel` → `explorer_op_progress`/`explorer_op_result`**
  (docs/features/remote-attach/index.md) — 원격 explorer 의 파일 작업(새 폴더·새 파일·이름 변경·휴지통·복사·이동·되돌리기·남은
  원본 지우기). 조회와 달리 서버 파일을 바꾸므로 서버가 descriptor 의 `stream_capabilities` 에
  `ipc.stream.explorer-write`(버전 1)를 알리고, client 는 그 목록이 있는 연결에만 보낸다. 구 서버는
  이 event 를 조용히 버려 응답이 오지 않기 때문이다. workspace 전체 닫기 op 도 같은 이유로
  `ipc.stream.workspace-close` 가 있는 연결에만 보낸다(위 "서버 workspace 닫기").
  - **실행**: 서버는 로컬 explorer 와 같은 작업 엔진(`src/app/file_jobs/`)을 작업 스레드에서 돌린다
    (`file_jobs::remote::RemoteJobs`). gui·headless 서버 모두 같다. 연결마다 한 번에 하나만 실행하고,
    실행 중에 온 요청은 `busy` 로 거절한다. client 가 결과를 받은 뒤 다음 요청을 보낸다.
  - **요청**: `{"event":"explorer_op_request","op_id","op"}`. `op` 는 `{"kind":"paste","paths","destination","cut"}`·
    `trash{paths}`·`rename{path,name}`·`create{dir,name,folder}`·`undo{steps}`·`remove_leftovers{dest,leftovers}`
    이다(`file_jobs::remote::RemoteOp`). 경로는 서버 경로다.
  - **진행·질문**: 엔진이 진행 표시를 깨울 때마다(최소 100ms 간격, 질문을 띄우고 거둘 때는 바로)
    `{"event":"explorer_op_progress","op_id","progress","ask"}` 를 보낸다. `ask` 가 null 이 아니면 이름
    충돌의 답을 기다리는 중이다. client 는 `{"event":"explorer_op_answer","op_id","choice","apply_all"}`
    (`choice`: `keep_both`·`skip`·`replace`·`cancel_rest`)로 답하고 `explorer_op_cancel` 로 남은 항목을 멈춘다.
  - **결과**: `{"event":"explorer_op_result","op_id","result"}`. `result` 는 항목별 결과
    `{"report":{"report","truncated"}}`, 이름 변경·새 항목의 `{"simple":{"error"}}`, 시작하지 않은
    `{"refused":{"reason"}}`(`not_holder`·`busy`·`invalid`·`unavailable`) 중 하나다. 항목별 결과가 700 KiB 를
    넘으면 되돌리기 단계와 남은 원본을 빼고 목록을 반씩 줄여 `truncated: true` 로 보낸다. client 는 줄인 결과의
    Retry 에 줄인 목록이 아니라 자기가 보냈던 경로와 남은 원본(`Report::resend`)을 그대로 다시 보낸다.
  - **인가**: 디렉토리 조회와 같이 `client_holds_workspace(client_id)` 하나다. 경로를 점유 workspace 안으로
    제한하지 않는다. SSH 로 그 호스트에 셸을 쓸 수 있는 사용자가 이미 할 수 있는 일이다. 서버 사용자의 파일
    권한이 그대로 적용되고 권한 거부는 항목별 실패가 된다.
  - **끊김**: 연결이 끊기면 서버는 그 연결의 작업을 취소한다. 엔진의 취소 규칙대로 지금 항목에서 멈추고 준비
    중 사본은 지운다. 답을 기다리던 작업도 멈춘다. 서버가 내려갈 때는 실행 중인 작업을 취소하고 2초까지 기다린다.
- **조회할 cwd 선택**: 두 API는 경로를 얻는 방식이 다르다.
  `list_dir_request`는 클라이언트가 보낸 `dir` 문자열을 그대로 사용한다.
  `git_query_request`는 `worktree_path`가 없으면 서버가 `surface_id`로 해당 PTY를 찾아
  `TerminalStore::cwd(surface_id)`를 호출한다. Terminal의 OSC-7 캐시를 먼저 보고 같은 항목의 Pty에서 PID를 빌려 `/proc`·`proc_pidinfo`로
  보완하므로, 원격 셸이 OSC 7을 보내지 않아도 OS에서 cwd를 조회할 수 있다.
  클라이언트가 재생한 OSC-7 정보에 의존하지 않는다.

## 관련

- 결정 근거(원격 대상·로컬 debug 격리): [원격 접속 프로필 + Passkey (remote_tool)](../features/remote-profiles/index.md)
- 동작·점유 규칙·CLI/IPC 사용법: [`features/remote-attach`](../features/remote-attach/index.md)
- 주체(원격 사용자)·점유 모델 개념: [`concepts/actors`](../concepts/actors.md)
- 원격 디렉토리 나열: [원격 attach (Remote attach)](../features/remote-attach/index.md)
- 원격 git 조회(git-viewer): [원격 attach (Remote attach)](../features/remote-attach/index.md)
- SSH 프로필 관리: [`features/ssh-tool`](../features/remote-profiles/index.md)
- 로컬 self attach 격리: [`dev-guide/debug-ipc`](debug-ipc.md)


## Self-attach 거절 검증

GUI가 mirror를 만드는 네 경로는 `attach_client/dispatch.rs::dispatch_attach`를 공유한다.
source는 셋이다. `User`는 원격 연결 팝업, `Ipc`는 IPC `attach.into_gui`, `Endpoint`는 IPC
`remote.attach`와 자동 attach다. 자기 포트면 `RejectedSelf`로 반환하고 connector를 호출하지
않는다. 다른 포트면 connector를 정확히 한 번 호출하며 성공값/오류를 그대로 돌려준다.
`Ipc`와 `Endpoint`는 성공해도 포커스를 옮기지 않고(설치 대상의 `activate=false`), 사용자
경로 `User`만 기존처럼 새 mirror를 포커스한다. 이 분기는 raw stream client의 정상 로컬
attach와 별개다.

정확성 시험은 세 source 모두에 대해 connector 진입 횟수와 결과를 직접 단언한다. debug GUI의
통합시험은 `attach_dispatch_completed` 로그에서 자기 port/workspace와 source 값에 대응하는
**완료**와 `connector_entries=0`, `outcome=rejected_self`를 확인한다. source 별로 따로 본다.
`attach.into_gui` 요청은 source `attach.into_gui` 기록을, 자기 포트로 보낸 `remote.attach`
요청은 source `remote.attach/auto-attach` 기록을 찾는다. 자동 attach는 같은 `Endpoint`
source를 쓰지만 통합시험이 직접 실행하지 않는다. 이 기록은 dispatcher 반환 뒤에
생기며, connector에 들어갔으면 실패해 돌아와도 진입 횟수가 남는다. 기록 구현은 모듈 선언에
`cfg(debug_assertions)`가 붙은 별도 `dispatch/debug_completion.rs`에 있다. 새 IPC나 전역
진행 카운터는 없다.

`tests/attach_silent_disconnect.rs`는 점유가 잡히지 않는 관측과 정상 재attach도 유지한다.
6초 관측 창의 RTT 수열·최악·중앙값·2초 이상 표본 수는 진단이며 정확성 단언에 쓰지 않는다.
GUI debug 시험의 완료 기록이 없으면 queued 응답만으로 통과하지 않는다. headless에서
GUI 큐가 처리되지 않는 현재 동작은 별도 시험 이름으로 검사하므로, 그 통과를 GUI 거절
검증으로 세지 않는다. release의 공통 분기는 유닛시험 대상이며 debug 로그를 요구하는
통합시험은 debug 조합에만 있다. 근거와 대안은 [준비 부족과 제품 결함을 구분한다](self-verification.md#준비-부족과-제품-결함을-구분한다).

서버 로컬 변경의 구조 통지는 GUI·headless 모두 journal publication이 풀리고 원격 응답이 전달된 뒤 보낸다. 셸 종료로 surface가 삭제된 경우에도 다음 client 입력이나 StreamReady를 기다리지 않는다. 아직 원격 Result가 대기 중인 workspace의 Delta는 기존 pending reply 경계에서 보류하며, 성공한 Result 뒤에 전달한다.

journal이 원격 요청을 실제 해소할 때도 anchor 부재 판정은 모든 현재 engine을 대상으로 한다. 어디에도 anchor가 없고 원 holder의 workspace가 남은 경우에만 공통 `no live surface` 사유와 원 structural op 이름을 반환한다. 다른 engine에서 살아 있는 anchor는 `workspace not found`로 거절한다. MoveSurface의 범위 밖 명시 target은 같은 명시 surface 거절 형식을 사용하며 다른 workspace를 변경하지 않는다.

## CLI 출력 종료

`remote attach`, `tool attach`, debug 전용 `debug attach`의 raw 모드는 같은
출력 처리를 사용한다. stdout 쓰기나 flush가 BrokenPipe로 끝나면 세션을 정상 종료한다.
다른 출력 오류는 호출자에게 반환한다. detach 통지 실패나 reader 스레드 패닉은
진단을 남기며 후속 종료 처리를 계속한다. 종료 직전 입력 송신 실패도 기록한다.

Markdown 조회 대기 표시는 plugin의 `user_initiated` 출처를 함께 확인한다. 문서 안 새로고침 버튼은 표시 대상이며, 최초 mirror 원문 설치·변경 신호에 따른 자동 재조회·외부 `markdown.reload`는 표시 대상으로 삼지 않는다. 기존 `agent_origin`의 잘림 알림 정책과는 별도 필드다.

## HTML entry 전송

연결별 opt-in metadata·분할 전송의 [HTML 전송 계약](html-transfer.md)을 따른다. 전송 capability는 HTML viewer capability와 별개다. Linux·Windows·macOS GUI client는 viewer capability까지 협상한 연결의 파일 entry를 native로 표시한다. 구 peer·미지원 client는 placeholder를 유지한다. 하위 리소스와 HTTP(S)는 차단하며 client fallback이 없다.

capture·Git·markdown의 custom control event 태그 검사는 client wire 파서의 `control_event`가 공유한다. 태그가 없거나 다르면 해당 파서는 인식하지 않은 것으로 처리하고 다음 파서로 넘긴다.
