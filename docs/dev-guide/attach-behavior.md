# Attach 메커니즘

attach 가 *어떻게 구현되는가*. 사용자/에이전트가 보는 **동작·점유 규칙·인터페이스는** [`features/remote-attach`](../features/remote-attach/index.md) 에, 여기엔 **후속 작업자가 서버 핸들러를 잘못 건드리지 않도록 못 박는 내부 메커니즘**만 둔다.

가장 헷갈리는 핵심: **서버는 transport 를 모르고 항상 loopback 으로 받는다 — 로컬/원격 구분은 전적으로 클라이언트 측 개념이다.**

## 서버 / 클라이언트 계층 (가장 먼저 읽을 것)

attach 는 **server**(피점유 — PTY/grid 소유)와 **client**(점유 — mirror 표시) 두 쪽이다.

- **서버측** (`src/core/attach_runtime.rs`, IPC `attach.*`) — **transport 를 모른다.** 항상 `127.0.0.1` 로만 client 를 받는다. 로컬에서 붙든 SSH 터널 너머에서 붙든 서버 입장엔 전부 loopback 이다. 서버는 SSH 를 전혀 모른다.
  - 연결마다의 push sink·입력 프레임 분류·bulk 연결 결속은 `StreamHub`(`crates/tasty-ipc/src/stream_hub.rs`)가 든다. sink 는 채널이라 허브도 TCP 를 모른다 — 소켓을 읽고 쓰는 accept 스레드는 본체 adapter `src/adapters/production/tcp_ipc_server.rs` 에 있다. core 는 adapter 를 거치지 않고 크레이트의 허브를 직접 부른다([ADR-0350](../adr/0350-the-stream-hub-lives-in-the-ipc-crate.md)).
- **클라이언트측** — "원격성" 을 전부 흡수한다. 두 종류:
  - **로컬 client**: 포트 파일(`~/.tasty/tasty.port`)을 읽어 그 loopback 포트로 직결. **release 에서 제거 → debug 전용**(`tasty debug attach`).
  - **원격 client**: `ssh -L 127.0.0.1:<localport>:127.0.0.1:<remoteport> -N` 터널 후 그 **localport 로 직결**. 터널은 바이트 파이프라 스트림 프로토콜에 투명 — 원격 client 도 결국 자기 머신 loopback 에 붙는다(`tasty remote attach --ssh|--profile`).

### "로컬 attach 제거" 의 정확한 의미

> **attach 는 원격을 대상으로 한다** — 로컬 self-attach 는 release 에서 제거하고 debug 격리한다. 이 결정의 *근거·대안·재검토 조건* 은 [ADR-0007](../adr/0007-attach-targets-remote.md). 아래는 그 결정이 구현에 어떻게 드러나는지다.

원격 attach 도 **서버 입장엔 loopback** 이다. 따라서 release 에서 "로컬 attach 제거" 는 **서버를 바꾼 게 아니라 client 의 로컬 진입점(`tasty attach` → `tasty debug attach`)만 제거**한 것이다. 서버의 attach 수신 경로는 로컬/원격 공용으로 보존된다. SSH 터널 + attach 세션 머신(`run_attach_*`)은 `crates/tasty-cli/src/local/attach.rs` 에 공용으로 남고, `remote`/`debug` 네임스페이스는 그 위에서 디스패치만 한다.

### `remote` / `debug` 는 CLI 디스패치 네임스페이스 (IPC 와 비대칭)

`remote`·`debug attach` 는 **IPC 네임스페이스가 아니다.** attach 의 IPC 표면은 `attach.*`(아래) 그대로이고, `remote attach`/`remote check`/`debug attach` 는 그 위(+`system.info`)에서 *원격성·debug 격리만 분기*하는 CLI 계층이다.

## 점유 레지스트리 (`OccupancyRegistry`)

`src/core/attach.rs`. 휘발성(직렬화/복원 안 함 — 재시작 시 빈 registry → 전부 free).

이 registry 는 [ADR-0040](../adr/0040-occupancy-soft-hard-tiers-agent-occupant.md) 의 **약한/강한(soft/hard) 2계층 점유**를 함께 담는다. **attach 는 강한(hard) 점유의 한 사례** — 아래는 그 hard 경로다. 약한(soft) 점유(advisory 마커, write 미차단; 현 소비자 = `terminal` 명령의 child-terminal)는 [`features/child-terminal`](../features/child-terminal/index.md) 를 보라. 두 계층은 같은 registry 에 있지만 별도 저장이다(hard=`surface_locks`/`workspace_locks`, soft=별도 soft 엔트리).

- `surface_locks: HashMap<SurfaceId, AttachLock>` — surface 단위 배타(hard) lock. `acquire` 가 동시 점유를 `AlreadyAttached{holder}` 로 거부.
- `workspace_locks` + `surface_to_workspace` — workspace 단위 점유. workspace 점유 시 멤버 *터미널* 은 `surface_locks` 에도 동일 holder 로 등록(서버측 placeholder 렌더·입력차단을 surface 단위와 동일 적용). 비-터미널은 역매핑(`surface_to_workspace`)으로만 "점유 표시".
- `release` (holder 본인) / `force_detach` (서버 권한) / `release_all_for_client`(연결 생존 판정 실패 시 일괄 — workspace + 멤버 + 잔여 surface). 트리거는 4 종: **self-release**(holder 의 명시적 release) · **force-detach**(로컬 사용자 강제 해제) · **EOF-or-TTL**(연결 종료 EOF 또는 attach heartbeat TTL 만료 — 둘 다 `tcp_ipc_server.rs` read 루프의 `Err(_) => break` 를 거쳐 `StreamInbound::Disconnected` → `release_all_for_client` 로 합류한다; TTL 은 소켓 read timeout 이 heartbeat 미수신으로 만료되는 경우로, 실제 EOF 와 동일한 `io::Error` 취급이라 코드 경로가 갈라지지 않는다) · **forwarded-op cascade purge**(forward 된 구조 변경 자체가 workspace 를 통째로 지우는 경우 — 예: workspace 의 마지막 surface 를 forward `CloseSurface` 로 닫으면 `close_case_workspace` 의 "Case 4: last pane in workspace" 가 workspace 를 purge 한다. `execute_forwarded_structural_op`(`src/core/attach_runtime.rs`)이 실행 후 `ws_id` 로 재조회해 delta 를 만들려다 실패하면, 더 이상 존재하지 않는 workspace 를 향한 delta 대신 `force_detach_workspace` 를 호출해 holder 를 강제 detach 하고 stale lock 을 정리한다). 강한 점유 해제 사유 확장의 근거는 [ADR-0052](../adr/0052-attach-heartbeat-ttl-hard-occupancy-release.md).
- **끊긴 holder 는 재attach 를 막지 못한다** ([ADR-0157](../adr/0157-a-disconnected-holder-does-not-block-a-reattach.md)). inbound 한 배치(`PumpOutcome`)의 적용 순서는 attach 결선이 먼저, 연결 종료 *정리*가 마지막이다 — 끊긴 client 의 잔여 입력 프레임이 그 client 의 점유가 살아 있는 동안 적용돼야 하기 때문이다. 그래서 한 배치에 "C1 끊김" 과 "C2 attach" 가 함께 실리면 C2 가 곧 사라질 C1 의 lock 에 막힌다. 이를 막기 위해 두 pump(`App::apply_stream_outcome` · `boot::headless_stream::apply`)가 배치 **머리**에서 `mark_clients_disconnected` 로 *사실만* 먼저 알리고, `acquire`/`acquire_workspace` 는 자기를 막고 선 holder 가 그 표시를 가지면 그 자리에서 점유를 회수한다. 회수는 경쟁이 있을 때만 하므로 경쟁이 없는 잔여 입력은 그대로 처리된다. 표시는 `release_all_for_client` 가 lock 과 함께 지워 한 배치를 넘지 않는다.
- **입력 격리**: `apply_send_to_surface` 가 `is_hard_occupied` 면 서버 로컬 입력 거부, client 입력만 `feed_attached_input` 우회 경로로 PTY 도달. (soft 점유는 write 를 막지 않는다 — hard 만 격리.)
- **점유는 핸드셰이크가 검증된 뒤에만 잡힌다** ([ADR-0116](../adr/0116-attach-handshake-validated-before-occupancy.md)). 점유를 잡는 유일한 진입점은 `dispatch_stream_attach` → `attach_workspace_for_stream`/`attach_surface_for_stream` 인데, 그 **앞에** `tcp_ipc_server.rs::validate_stream_proto` 가 있다. `stream.open` params 의 `proto` 가 `STREAM_PROTO` 와 다르면(생략 시 serde default `0`) attach 를 dispatch 하지 않고 `StreamAck{ok:false, proto, error}` 로 거절한다 — 점유가 애초에 잡히지 않는다. 없을 때의 문제: 프로토콜이 안 맞는 client 는 그 점유를 **쓸 수 없는데도** 가져가고, 소켓을 닫지 않는 구버전/hung peer 면 아래 EOF 가 오지 않아 heartbeat TTL(20초)까지 그 workspace 가 붙잡혀 정상 attach 가 `already_attached` 로 거절됐다. 거절 ack 는 client(`StreamConnection::open_with`)가 이미 검사하는 형식이라 실패 사유가 그대로 사용자에게 전달된다.
- **self-attach 는 그보다 앞, client 측 dispatch 에서 거절된다**: `attach_client/dispatch.rs::connect_unless_self` 가 요청 포트를 이 인스턴스의 IPC 포트와 비교한다(debug/release 공통). 이 경로의 핸드셰이크는 GUI 메인 스레드에서 동기 블로킹으로 도는데 그 응답을 만드는 것도 같은 메인 스레드라 자기 자신 대상이면 교착으로 **반드시 실패**하고, 실패하는 동안 대상 workspace 점유만 남는다. 서버 accept 층에서는 막을 수 없다 — 자기 자신과 `ssh -L` 로 도착하는 정상 원격 mirror 는 둘 다 loopback 연결이라 구분되지 않고, 요청 포트와 자기 IPC 포트를 함께 아는 것은 client 측 dispatch 뿐이다. 로컬 self-mirror 검증은 별도 프로세스인 `tasty debug attach` 로 한다.

## 초기 스냅샷 + delta

attach 직후 서버가 현재 visible 화면을 `snapshot_as_vt` 로 **1회** 직렬화 push(셀 속성 + 커서 + alt-screen/DECCKM/bracketed 모드 복원). 이후 변화는 output tap delta(Data 프레임). client 는 받은 바이트를 PTY 없는 mirror 터미널(`Terminal::new_detached` + `feed_bytes`)에 먹여 같은 termwiz 파서로 grid 재구성.

## workspace mux

한 연결로 N 터미널 출력을 나르므로 workspace 모드 Data 프레임은 **surface-prefixed**(`encode_mux`/`decode_mux`). surface 단위 단일 연결은 prefix 없음. attach 직후 서버가 `attached_workspace` Control 로 트리(분할 방향/비율) + per-surface 디스크립터를 보내고, client 는 원격↔로컬 surface_id 재매핑으로 트리를 재구성한다. per-surface role 은 5종:

- `{remote_id, role:"terminal", cols, rows}` — mirror 가능한 터미널.
- `{remote_id, role:"mesh", kind, plugin_id, display_name}` — bundled egui-mesh 화이트리스트 통과(mesh mirror, 아래 절).
- `{remote_id, role:"explorer", root}` — 활성 탭의 현재 디렉토리만 싣고 목록은 `list_dir` 채널로 lazy 조회([ADR-0059](../adr/0059-explorer-remote-attach-list-dir-reuse-browse-only.md)).
- `{remote_id, role:"markdown", file, display_name}` — 원문은 안 싣고 `markdown_content` 채널로 lazy 조회(아래 절, [ADR-0255](../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md)). `file` 은 표시·제목 전용 opaque 문자열이라 client 가 그 경로로 자기 로컬 파일을 열지 않는다. **비어 도착할 수 있다** — 그 값은 plugin 의 `surface.create` snapshot 에서 오는데 그 도착이 `tab.create` 반환보다 뒤라, 갓 연 surface 에 곧바로 attach 하면 `role` 만 맞고 `file` 이 빈 문자열이다(핸드셰이크는 동기 경로라 host 가 기다리지 않는다). wire 상 "파일 없이 열린 surface" 와 구별되지 않으므로, 원문은 아래 채널로 가져온다(그 회신의 `file` 은 요청 시점 값이라 채워져 있다).
- `{remote_id, role:"placeholder", kind}` — 그 외 비-터미널, mirror 불가.

이 role 분류는 `build_workspace_tree_surfaces`(`src/core/attach_runtime.rs`)가 만든다. mesh 와 markdown 은 **두 단**이다 — `tasty-model` 이 `Surface::attach_mesh_info()`/`attach_content_info()` 로 후보만 모으고(crate 가 화이트리스트를 모른다), 앱 계층이 `is_egui_mesh_allowed`/`is_attach_content_allowed` 로 재검증해 떨어진 후보를 placeholder 로 내린다.

## 프레임 전송 지연 (Nagle 금지)

attach 스트림은 **프레임 하나 = 상호작용 하나**(키 입력 · 리사이즈 요청 · 구조 op · 출력 delta)다. 그래서 프레임 단위 전송 지연이 그대로 체감 지연이 된다 — 후속 작업자가 이 두 가지를 되돌리면 왕복 지연이 20 배로 뛴다(실측: loopback 왕복 1.9ms → 43ms).

- **프레임은 한 번의 `write` 로 나간다** — `tasty_ipc::stream::write_frame` 이 `[tag][len][payload]` 를 한 버퍼에 합쳐 `write_all` 1 회로 보낸다. `TcpStream` 은 버퍼링이 없어 헤더/payload 를 나눠 쓰면 그대로 TCP 세그먼트가 쪼개지고, Nagle 이 켜진 소켓은 첫 조각이 unACKed 인 동안 뒷조각을 상대의 delayed ACK(~40ms)까지 붙잡는다. `crates/tasty-ipc` 의 유닛 테스트 `stream::tests::write_frame_emits_one_write_call` 이 write 횟수를 고정한다.
- **양쪽 소켓 모두 `TCP_NODELAY`** — 서버 `prepare_stream`(`src/adapters/production/tcp_ipc_server.rs`, 일반 JSON-RPC 연결 포함)과 client `StreamConnection::open_with`(`crates/tasty-ipc/src/client/stream.rs`). 위에서 합쳐도 payload 가 MSS 를 넘으면 마지막 조각이 다시 Nagle 에 걸리므로 이중 방어다.
- 호스트 ↔ plugin 메인 채널도 같은 두 가지를 한다 — 한 줄을 한 번의 write 로(`tasty_plugin_protocol::write_line`), 양 끝 `TCP_NODELAY`. [`plugin-development.md` "전송 지연"](plugin-development.md#전송-지연-nagle-금지).

**SSH 터널은 이 지연을 흡수해 주지 않는다.** 터널 너머든 아니든 tasty 소켓의 양 끝은 항상 loopback이고, 위 지연은 그 loopback 구간에서 발생한다. 그리고 mirror 를 다시 mirror 하는 다단 구성(A → B → C)에서는 홉마다 입력·출력 양방향으로 얹히므로 지연이 홉 수에 비례해 누적된다.

## 밀어내기 실패와 누적 손실

`StreamHub::push` 는 막히지 않는다 — client 의 sink 가 차 있으면 그 프레임을 **버리고**
돌아온다(`PushResult::Dropped`). 연속으로 버린 수가 한도를 넘으면 그 연결을 끊는다
(`PushResult::Disconnected`). 끊는 갈래도 그 프레임을 못 보내므로, 그 한 장 역시 손실이다.

- **`StreamSink::lag` 은 연속 drop 수다 — 성공 한 번에 0 으로 돌아간다.** 그래서 단발 손실이
  섞여 있어도 나중에 보면 0 이고, "조용히 한 장 잃었나" 를 그 값으로는 못 묻는다.
- **`StreamHub::loss()` 는 누적이고 안 내려간다.** 두 칸이며 물음이 다르다:
  `frames_dropped` 는 연결이 **살아 있는데** 사라진 프레임 수(소비자가 알 길이 없는 손실),
  `clients_lagged_out` 은 한도를 넘겨 끊은 연결 수(소비자가 이미 아는 손실)다. 한 수로
  합치면 앞의 물음이 사라진다.
- **지금 이 값을 읽는 제품 경로는 없다.** 호출자 쪽에서도 손실이 안 보인다 — `push` 를 부르는
  제품 코드 **37** 자리 중 31 이 결과를 `let _ =` 로 버리고, 결과를 보는 **여섯**도
  `Dropped` 를 `Unknown`/`Disconnected` 와 함께 묶어 다룬다. 그 여섯 중 다섯
  (`core/attach_runtime.rs` 의 출력 tap 루프 넷 · `plugin_bridge/mesh_forward.rs`)은
  `Unknown`/`Disconnected` 에만 `break` 하고 `Dropped` 면 **남은 chunk 를 계속 보낸다** —
  순서 있는 열에서 가운데 한 장이 빠진 채 나머지가 간다. 나머지 하나
  (`core/attach_runtime.rs` 의 markdown 변경 신호)는 `Sent` 외 전부를 한 덩어리로 debug
  로그한다. 즉 이 카운터는 **손실이 있었는지를 값으로 남기는 자리**이고, 밖으로는
  `system.pressure` 의 `stream_push` 덩어리(CLI `tasty list pressure`)로 나간다
  ([telemetry](../features/telemetry/index.md)) — **그 자리와 아래 "client 에게 공백을
  알린다" 는 물음이 다르다.** 이 카운터는 *서버 전체*의 누적이고, 아래 통지는 *그 연결
  하나*가 이번에 몇 장을 잃었는가다.
- **`backlog` 은 지금의 적체다 — 누계와 다르고 `lag` 과도 다르다.** 연결마다
  `StreamSink::queued` 가 push 성공에 +1, write 스레드가 `SinkReceiver` 로 꺼낼 때 −1 이다
  (`SyncSender` 가 길이를 안 주므로 넣는 쪽과 꺼내는 쪽이 함께 센다 — 꺼내는 경로가 그 타입의
  메서드뿐이라 −1 을 빠뜨릴 수 없다). `StreamHub::loss()` 는 **살아 있는 연결의 몫만** 더한다 —
  끊긴 연결의 큐는 채널과 함께 사라지므로 전체 카운터 하나로 세면 그 몫이 영구히 떠오른다.
  세 값의 구분은 [ADR-0400](../adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md).
- **그 37 은 `#[cfg(test)]` 밖만 센 값이다.** 세는 법은 수신자에 숫자 접미사가 붙는 것
  (`hub2` · `hub3`)까지 포함해야 한다 — 그것을 빼면 넷이 빠지고, 하필 그 넷이 결과를
  **보는** 자리라 위 "여섯" 이 "둘" 로 줄어든다.

### client 에게 공백을 알린다 (`Loss` / `client_loss_notify`)

버린 것을 client 가 **모르는 채로** 계속 그리면, 그 화면은 이전과 연속이 아닌데도 연속인
것처럼 보인다. 그래서 버린 수를 그 연결에 되돌려 준다 — 단, **선언한 client 에게만**.

- **선언은 client→server 한 번**: `StreamControl::ClientLossNotify{}`. GUI mirror client 는
  `attached_workspace` 핸드셰이크 직후 `src/app/attach_client.rs` 에서 보내고, CLI 는
  `tasty attach <surface>` · `tasty attach --workspace` 의 attach 결과 프레임을 읽은 직후
  `crates/tasty-cli/src/local/attach.rs` 의 `declare_loss_notify` 에서 보낸다.
  `StreamHub::pump_inbound` 이 이것을 `StreamHub::enable_loss_notify` 로 분류해 그 client 의
  sink 에 표시한다.
  - **선언하지 않는 소비자는 debug 스트림 하나다**(`crates/tasty-cli/src/local/debug.rs` 의
    `StreamConnection::open` — attach 가 아니라 echo 확인용이라 재생할 화면이 없다). 그 연결은
    **종전 동작 그대로** 조용히 손실을 겪는다. 좌변: `grep -rn 'ClientLossNotify {}'` 의 송신
    자리. **선언하지 않은 peer 의 바이트 열은 한 바이트도 안 바뀐다** — 옛 client 는
  `Loss` 를 역직렬화 못 해 조용히 무시할 텐데, 그 무시가 곧 "손실이 없었다" 로 읽히기
  때문이다(아래 "왜 판을 안 올리는가").
- **통지는 server→client `StreamControl::Loss{frames}`**: 직전 `Loss` 이후(첫 통지면 연결을 연
  이후) 이 연결에서 버린 프레임 수.
- **통지 자체가 막힐 수 있다 — 그래서 빚으로 든다.** 버리는 순간은 정의상 sink 가 찬
  순간이라 통지도 못 넣는다. `StreamSink::pending_loss` 에 수를 쌓아 두고, **sink 에 자리가 나는
  첫 순간** — write 스레드가 큐에서 한 장을 꺼낸 직후(`SinkReceiver`) — 에 통지를 넣는다. 그
  순간 큐에 든 것은 전부 버리기 전에 들어간 프레임이라, 통지는 **마지막 생존 프레임 바로 뒤**,
  즉 공백 이후 첫 프레임 앞에 정확히 앉는다 — 위치가 곧 "여기서 끊겼다" 는 뜻이다. 넣기에
  실패하면 빚을 **안 지운다**(다음 기회에 전액 갚는다). `push` 의 맨 앞(`StreamHub::repay_pending_loss`)
  에서도 한 번 더 시도한다.
  - **통지의 지연 상한 — 소비자가 공백 앞을 다 읽는 순간 통지는 큐의 맨 앞에 있다.** 뒤따르는
    push 에도, 소비자의 inbound 에도 기대지 않는다. server→client push 는 전부 **변화 구동**이라
    (1 Hz tick 에 올라타는 `Activity`·`Attention`·`Cwd` 도 diff 만 민다 —
    `busy_activity_forwards_only_on_change`), 갚는 자리가 `push` · `pump_inbound` 뿐이면 폭주 직후
    조용해진 연결에서 통지가 다음 사건이나 그 연결의 다음 inbound 를 기다린다. **심장박동이 없는
    CLI mirror-dump** 는 그 사이에 창이 끝나 공백 앞까지의 화면을 경고 없이 찍었다
    ([ADR-0450](../adr/0450-a-pending-loss-notice-is-queued-the-moment-the-sink-has-room.md)).
    sink 가 계속 차 있으면(소비자가 안 읽으면) 여전히 안 나가고, 그 경우는 `LAG_LIMIT` 강제분리가
    끝낸다. 시험 `a_notice_follows_the_last_survivor_with_nothing_pushed_and_nothing_sent_after_the_loss`.
  - **`pump_inbound` 도 갚는다 — 선언이 손실보다 늦게 온 연결.** 소비자가 선언 전에 큐를 다
    비웠으면 꺼낼 때는 갚을 것이 없었고 선언 뒤에는 꺼낼 것이 없다. `pump_inbound` 가 끝에서 모든
    연결의 빚을 갚으므로(`repay_all_pending_loss`) 선언을 적은 그 배치에서 통지가 나간다. 시험
    `a_declaration_that_arrives_after_the_loss_is_answered_in_the_same_inbound_batch`.
  - **평상시 비용.** write 스레드는 연결마다 둔 원자 사본(`StreamSink::owes_notice` —
    `loss_notify && pending_loss != 0`)만 읽고, 빚이 있을 때만 sink 맵 잠금을 잡는다. 선언이
    손실 뒤·비우기 전에 오면 선언 자리가 사본을 켜서 꺼내는 순간 갚는다 — 시험
    `a_declaration_between_the_loss_and_the_drain_is_repaid_by_the_write_thread`.
- **통지 성공은 소비자가 따라잡은 것으로 안 센다.** `repay_pending_loss` 는 `StreamSink::lag`
  을 건드리지 않는다 — 서버가 스스로 넣은 프레임이 `LAG_LIMIT` 강제분리 시계를 되돌리면,
  영원히 안 읽는 소비자가 영원히 안 끊긴다.
  - **★ 그 이면 — 선언한 연결은 회복 문턱이 한 칸에서 두 칸으로 올라간다.** 통지가 본 프레임보다
    먼저 빈 칸을 가져가는데 `lag` 은 본 프레임이 들어가야 0 이 되므로, 소비자가 **push 한 번당
    정확히 한 칸씩** 비우는 구간에서는 빚이 있는 선언 연결이 본 프레임을 한 장도 못 넣고
    `LAG_LIMIT` 에서 끊긴다. 같은 속도의 안 선언한 연결은 매번 `Sent` 라 안 끊긴다. 실측
    (`StreamHub` 단위, 264 회 상한, `(마지막 PushResult, Sent 수)`): 한 칸이면 미선언
    `(Sent, 264)` · 선언 `(Disconnected, 0)`, 두 칸이면 둘 다 `(Sent, 264)`. 차이가 나는 구간은
    **정확히 1:1 소비**뿐이다. 근거와 기각한 대안(`lag` 되돌리기)은
    [ADR-0334](../adr/0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md).

**왜 `ipc.stream` 의 판을 안 올리는가.** `STREAM_PROTO` 는 서버가 핸드셰이크에서 **동등
비교**하는 수다(`validate_stream_proto`). 올리면 기능이 좁아지는 것이 아니라 구 peer 의 연결이
**거절**된다. 그래서 더해지는 스트림 기능은 판이 아니라 **이름**으로 선언한다 —
`capability::CAPABILITIES` 의 `ipc.stream.loss-notify`(`system.info` 로 노출) 가 서버 쪽,
위 `client_loss_notify` 프레임이 client 쪽이다. 결정은
[ADR-0334](../adr/0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md).

### 통지를 받은 client 가 하는 일 (재동기화 계약)

통지는 "몇 장 잃었다" 까지만 말하고 무엇을 잃었는지는 안 싣는다(연결 단위). 그래서 계약은
**데이터 종류마다** 두고, 연결 하나에는 그 연결이 나르는 종류 중 **가장 강한** 것을 건다.
근거·기각한 대안은 [ADR-0400](../adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md).

| 종류 | 계약 |
|---|---|
| PTY byte delta | **snapshot 재요청** — 공백 뒤 바이트를 공백 앞 화면에 잇지 않는다 |
| 상태 snapshot (`Resize`·`Activity`·`Attention`·`Cwd`·구조) | **낡음 표시** — 새 값이 올 때까지 최신이라고 말하지 않는다 |
| mesh | **종료** — 공백을 건넌 조립·캐시를 잇지 않고, 처음부터(full texture) 다시 구독한다 |
| bulk | **전송 중단** — 결과를 모르는 전송을 성공으로도 실패로도 확정하지 않는다 |

- **snapshot 재요청의 수단은 재attach 다.** 서버가 PTY snapshot 을 만드는 자리는 attach 경로
  하나뿐이다(`snapshot_as_vt()` 와 출력 tap 을 메인 루프의 한 턴에 함께 잡는다). 새 wire 가
  없으므로 구 서버에서도 그대로 동작한다.
- **순서: `Detach` → 옛 연결의 EOF → 새 연결.** 서버는 옛 연결의 `Disconnected` 를 inbound
  채널에 넣은 **뒤에** 소켓을 닫는다(`finish_stream_connection`). 그래서 EOF 를 본 client 가
  여는 새 연결의 attach 요청은 같은 FIFO 에서 반드시 점유 해제 뒤에 처리된다 — 점유가 옛
  client 에 남아 새 attach 가 `already_attached` 로 거절되는 경합이 없다.
- **GUI mirror**(PTY + 상태 + mesh 를 한 연결로): 리더 스레드가 `Loss` 를
  `MirrorEvent::Desynced` 로 옮기고, 적용(`begin_resync`)이 셋을 한다 — 이 세션의 mirror
  터미널마다 출력 stream 표지를 새로 만들고(아래), 옛 연결에 `Detach` 를 보내고, 경고 toast
  (`attach.toast.mirror_desynced`)로 화면·상태가 낡았음을 알린다. 세션의 `resync_pending` 이 선
  동안 EOF 가 오면 `apply_attach_client_output` 이 끊김 정리 대신 `resync_session` →
  `reconnect_session` 으로 다시 붙는다 — anchor 유무와 무관하다. 성공하면 재연결 toast 가
  뒤따르고, 실패하면 끊김과 같은 갈래다(anchor 가 있으면 `Reconnecting`, 없으면 정리). 세션마다
  재attach 는 한 번에 하나이고, 기다리는 동안 온 통지는 수만 더한다. 끊김 판정 규칙은
  `disconnect_disposition` 한 함수에 있다.
- **창 없는(parked) engine 의 손실은 재attach 를 미룬다.** `begin_resync` 는 host 가 parked 면
  표지만 갱신하고 `resync_awaiting_window` 를 세운다 — 옛 연결에 `Detach` 를 안 보낸다.
  `reconnect_session` 은 mirror 를 담은 창을 `find_main_with_workspace` 로 찾으므로 parked 에서
  걸면 실패하고, 실패 갈래는 anchor 없는 세션을 정리하기 때문이다. 그 engine 이 창에 다시
  붙으면 `apply_attach_client_output` 의 창 갈래가 `resume_resync_in_window` 로 옛 연결을 놓고
  낡음 toast 를 띄운다. 이 함수는 `AttachClientData` wake 와 `Tick::AttachView`(3 초 backstop)
  둘 다에서 돌므로, 복원 뒤 원격이 조용해도 늦어도 한 주기 안에 시작한다. 미루는 동안 옛
  연결이 따로 끊기면 `resync_released()` 가 거짓이라 손실과 무관한 끊김 갈래를 탄다.
  - **이 재개 배선에는 시험 채널이 없다.** 단위 시험은 `resume_resync_in_window` 와
    `disconnect_disposition` 을 직접 부른다 — 그 함수가 `apply_attach_client_output` 의 창 갈래에
    이어져 있는지는 아무 시험도 안 본다. 그 호출을 `if false { … }` 로 무력화해도
    `cargo test -p tasty --lib` 가 초록이다(실측 2026-09-21: 2543 passed). 창 갈래를 시험하려면
    `App`(winit `EventLoopProxy` 필요)과 창 있는 `MainView` 를 조립해야 해서 붙이지 않았다. 배선이
    빠지면 parked 손실은 창이 돌아와도 재attach 되지 않는다 — mirror 는 남지만 공백은 안 메워진다.
    **재는 법**: 격리 `TASTY_HOME` 인스턴스 둘(A 서버, B GUI)을 띄우고 B 에서
    `tasty remote attach --into-gui --target-port <A 포트> --workspace <N>` 으로 수동 mirror 를 연다.
    B 의 마지막 창을 닫아 engine 을 parked 로 보낸 뒤, B 를 `SIGSTOP` 하고 A 의 그 터미널에 출력을
    흘려 `tasty list pressure` 의 `stream_push.frames_dropped` 가 64 미만으로 오르면 `SIGCONT` 한다.
    B 로그에 "재attach 를 창이 돌아올 때까지 미룬다" 가 찍혀야 한다. `tasty new window` 로 창을
    되살리면 3 초 안에 "미뤄 둔 재attach 를 시작한다" 가 찍히고 A 로그에 새 client id 의 attach 가
    나야 한다. 위 변이를 넣은 빌드에서는 두 번째 줄이 끝내 안 나온다.
- **재연결은 mesh 를 처음부터 다시 구독한다.** 서버의 mesh 구독은 client id 에 묶여 옛 연결과
  함께 사라진다. `reconnect_session` 이 그 세션의 mesh surface 마다 캐시된 frame
  (`attach_mesh_frames`)과 구독 dedup 상태(`MainView::attach_mesh_input`)를 지워, 다음 렌더가
  `MeshContext` 를 다시 보내게 한다. dedup 상태가 남으면 크기·테마가 안 바뀌는 한 구독이 다시
  안 나간다. 이 정리는 손실 재attach 와 네트워크 재연결에 똑같이 걸린다.
- **CLI**(`crates/tasty-cli/src/local/attach.rs`): 세 루프가 Control 을 `classify_control`
  로 가른다. 손실이면 옛 연결에 `Detach` 를 쓰고 서버가 소켓을 닫을 때까지(dump 는 reader
  스레드의 EOF, raw 브리지는 `ServerRecvErr`) 기다린 뒤 `run_attach_on_port` ·
  `run_attach_workspace_on_port` 가 다시 붙는다. 재attach 는 호출자에게 안 보인다 —
  반환 타입 `AttachExit` 에는 그 갈래가 없고 파일 안의 `SessionEnd` 에만 있다.
  - **dump**(`--dump-after`): 화면을 찍지 않고 다시 붙어 처음부터 수집한다. 한 실행에
    `DUMP_RESYNC_LIMIT`(3) 회까지이고, 넘으면 수집을 이어가 화면을 찍은 뒤 stderr 로 공백이
    남았다고 알린다(`cli.attach.desync_unrecovered`). stdout 형식은 안 바뀐다.
  - **raw 브리지**: 횟수 제한이 없다 — 대화형이라 사용자가 `Ctrl+\` 로 끝낼 수 있다. 새
    snapshot 이 화면을 다시 그리고, 전환하는 동안 들어온 stdin 은 원격에 안 간다(재연결 때의
    전환 창과 같다). 다만 그 창에서도 **끝내라는 신호는 살아 있다** — stdin EOF 와 detach 키
    (`Ctrl+\`)를 만나면 다시 붙지 않고 정상 루프와 같이 끝난다. 이 창에서는 stdin 슬롯이 아직
    옛 세션의 sender 를 들고 있어 EOF 가 latch 에 남지 않으므로, 삼키면 다음 세션이 EOF 를 영영
    못 받는다. 서버가 `HEARTBEAT_TIMEOUT` 안에 소켓을 안 닫으면 기다림을 포기하고 붙는다.
  - `--send` 입력은 첫 attach 에서만 보낸다 — 되풀이하면 원격에서 명령이 두 번 돈다.
- **bulk 연결**(`open_bulk_connection`): 핸드셰이크 직후 선언하고, `await_bulk_result` 가
  결과를 기다리는 중 `Loss` 를 받으면 `Detach` 후 **중단** 오류로 끝낸다 — 이 연결로 서버가
  미는 것은 `BulkResult` 하나라, 그것이 버려졌을 수 있다. 자동 재시도는 없다(서버가 이미
  저장했을 수 있다). 오류에 거부 접두(`BULK_REJECT_PREFIX`)를 달지 않으므로 이미지 붙여넣기
  업로드의 오류 행에는 재시도가 열린다.
- **mirror 출력의 stream 표지가 바뀐다.** mirror 터미널도 받은 바이트를 자기 출력 버퍼에
  쌓고, 에이전트는 그것을 위치 커서로 읽는다([ADR-0341](../adr/0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md)).
  `Desynced` 를 받을 때와 재연결로 snapshot 을 다시 받을 때 `Terminal::renew_output_stream` 이
  표지를 새로 만든다 — 옛 표지를 들고 읽는 소비자는 공백을 건넌 바이트 대신 stream 불일치를
  받는다. 위치 수는 이어서 세므로 표지를 안 싣는 소비자에게는 무변경이다.

## 갱신 cadence 분리

- **서버측 readonly 뷰**(피점유 — 대상 부하 절약): **3초 polling**(`Tick::AttachView`, `src/app/attach_poll.rs`)으로 self-snapshot 적용.
- **client mirror**(내가 다루는 대상): 원격 출력이 올 때마다 즉시 갱신, 3초 tick 은 backstop(누락 출력 적용·끊긴 세션 정리)으로만.

## 리사이즈 전파 (mirror geometry)

mirror grid 는 **client 가 구동(client-driven)** 한다(ADR-0045) — mirror 를 띄운 **로컬 pane 의 크기**가 grid 를 정하고, 원격 PTY 를 그 크기로 reflow 시킨다. "remote authoritative" 는 **메커니즘으로만 유지**: 원격 PTY 가 실제 크기의 단일 진실원(reflow 담당)이고 그 settled 크기를 echo 로 되돌린다. 즉 **의도(intent)는 client, 확정(confirm)은 remote** 의 요청→확정 협상이다.

- **client 구동 (forward)**: mirror 는 detached 터미널(PTY 없음)이라, 매 프레임 도는 로컬 레이아웃 리사이즈 스윕(`Core::resize_all_terminals` / `AppState::resize_all`)이 detached 터미널을 로컬에 적용하는 대신, 목표 grid `(cols, rows)` 를 `CoreState.pending_resize_forward`(로컬 surface id → grid)에 넣는다(목표가 이미 현재 mirror grid 면 생략). 이 한 곳에서 걸러 모든 리사이즈 진입점(창 resize·divider drag·단축키·redraw)을 커버한다. `App::dispatch_pending_resize_forwards`(`about_to_wait`, gui)가 drain 해 로컬 id 를 세션 매핑으로 원격 id 로 치환하고 `StreamControl::ClientResize{surface_id, cols, rows}` 를 `Control` 프레임으로 forward 한다. 세션의 last-forwarded dedup 이 echo 왕복(약 1 RTT) 동안의 매 프레임 재전송을 억제한다(coalesce; 서버측 동일값 `resize_grid=false` no-op 이 2차 방어).
- **서버 적용 (server)**: `StreamHub::pump_inbound` 이 `ClientResize` 를 `PumpOutcome.resize_requests` 로 분류 → 메인루프(gui `event_handler`/headless `boot`)가 anchor 워크스페이스의 **holder 를 검증**(hard 점유 = geometry 구동 권한, ADR-0040/0045)한 뒤 `CoreState::apply_attached_workspace_resize` 로 원격 **실제 PTY** 를 `Terminal::resize` 한다(reflow).
- **로컬 grid 는 echo 로만 갱신 (desync 방지)**: client 는 목표 크기를 로컬 mirror grid 에 **낙관적으로 먼저 적용하지 않는다**. 원격 reflow 전 잘못된 grid 에 바이트가 재생되는 desync 를 막기 위해, mirror grid 는 아래 `Resize` echo 가 도착할 때만 바뀐다.
- **원격→client 확정 echo**: 원격 터미널 grid 가 실제로 바뀌면(`TerminalState::resize_grid` 이 `true`) 서버의 resize tap(`Terminal::add_resize_tap`)이 새 `(cols, rows)` 를 fan-out 하고, attach forwarder 스레드가 `Control` 프레임에 `StreamControl::Resize{surface_id, cols, rows}` 를 실어 client 에 push 한다. workspace 모드는 `surface_id` 에 remote surface_id 를 실어 client 가 remote→local 매핑으로 해당 mirror 만 리사이즈한다. **이 echo 경로는 client-driven 전환 전과 무변경 재사용** — client 요청이 원격을 구동하면 결과가 이 경로로 되돌아온다.
- **순서 보존**: client reader 는 Data(출력)와 Resize 를 **한 버퍼에 도착 순서대로**(`MirrorEvent`) 쌓고, 메인 스레드가 순서대로 적용한다 — resize 앞뒤 출력이 올바른 그리드에서 재생되도록.
- **`StreamControl` 확장성**: `event` 태그 기반 enum. mid-session 이벤트는 새 `StreamTag` 없이 variant 로 추가한다 — 현재 `Resize`(server→client, 확정 echo), `Activity`(server→client, busy/idle 활동 상태), `Cwd`(server→client, surface cwd — 아래 "surface cwd 전파" 절), `ClientResize`(client→server, geometry 구동 요청), `ClientAttentionClear`(client→server, 주의 환기 해제 edge), `StructuralOp`(client→server, 구조 변경 forward), `StructuralResult`(server→client, forward 회신), `StructuralDelta`(server→client, 구조 역반영), `MeshContext`(client→server, mesh 구독+geometry/theme/focus — 아래 "mesh mirror 채널" 절), `MeshInput`(client→server, 누적 입력), `MeshFullResendRequest`(client→server, 텍스처 delta 체인 복구 요청), `MeshError`(server→client, mesh 단발 실패 통지), `Loss`(server→client, 이 연결에서 버린 프레임 수 — 위 "client 에게 공백을 알린다"), `ClientLossNotify`(client→server, 그 통지를 받겠다는 선언). 알 수 없는 event 는 역직렬화 실패로 무시(전방/후방 호환 — **다만 그 무시가 공짜일 때만 안전하다**: `Loss` 는 못 읽으면 "손실 없음" 으로 읽히므로 위 선언으로 게이트한다) — 구버전 서버는 `ClientResize` 를 무시해 기존 remote-authoritative 로 graceful degrade 한다. 단발 핸드셰이크 디스크립터(`attached`/`attached_workspace`/`attach_error`)와 `force_detached` 는 여전히 ad-hoc JSON 으로 위치 기반 파싱.

## 활동(busy) 상태 전파

사이드바 워크스페이스 리스트의 "실행 중" status dot(`WorkspaceEntryView.busy_count`, `docs/features/remote-attach/index.md` "GUI mirror" 참조)은 surface 의 busy/idle 상태를 본다. mirror(detached) 터미널은 로컬 PTY 가 없어 `CoreState::refresh_busy_surfaces`(foreground-process 폴링, `src/core/state/busy.rs`)가 절대 채울 수 없으므로, **원격이 직접 자기 surface 의 busy 상태를 계산해 client 로 forward** 한다 — resize 의 client-driven 협상과 반대로, busy 는 순수 **server→client** 단방향이다(원격 foreground 프로세스 이름은 애초에 client 에 존재하지 않는 정보라 client 가 요청할 수도 없다).

- **서버측 계산·forward**: 서버는 1Hz `Tick::Busy`(gui/headless 공통 — `crates/tasty-timer` 의 중앙 타이머 허브가 발화, [`timer-hub.md`](timer-hub.md) 참조)마다 `CoreState::forward_busy_activity`(`core/attach_runtime.rs`)를 호출한다. 이 메서드는 `busy_activity_forwards`(`core/state/busy.rs`)가 계산한 **점유 중인 surface 의 busy 값 변화분**만 `StreamControl::Activity{surface_id, busy}` 로 그 workspace/surface 의 holder client 에 push 한다. `last_forwarded_busy` 캐시로 값이 실제로 바뀐 경우에만 forward(중복 억제)하되, surface 가 점유 해제됐다가 재점유되면(다른 client 일 수 있음) 캐시를 버려 값이 이전과 같아도 항상 fresh 하게 1회 다시 push한다. 캐시는 **(holder, 값)** 을 기억한다 — 점유 해제 엔트리를 버리는 것은 점유 공백이 tick 경계를 넘을 때만 효과가 있고, 해제와 다른 client 의 획득이 한 tick 창 안에 끝나면 엔트리가 살아남으므로 holder 가 달라진 것을 그 자체로 변화로 센다(cwd push 와 같은 규칙) — resize 의 `last_forwarded_resize` dedup 과 동형이나 방향이 반대(client→server 아닌 server→client)다.
- **headless 는 별도 ticker 스레드가 필요 없다**: headless 메인 루프(`boot::run_headless`)가 `rx.recv_timeout(next_deadline)` 로 직접 타이머 허브의 다음 데드라인까지만 대기하다 `Tick::Busy` 를 스스로 드레인한다 — gui 전용 스레드나 이벤트 브리지에 의존하지 않는다([`timer-hub.md`](timer-hub.md) "headless — `run_headless`" 절 참조). **원격 attach 의 주 시나리오가 headless 서버**이므로 이 tick 이 없으면 활동 상태 forward 가 전혀 동작하지 않는다.
- **client 적용**: reader 스레드가 `Activity` Control 프레임을 `MirrorEvent::Activity(remote_surface_id, busy)` 로 버퍼링하고, `apply_attach_client_output` 이 세션의 `remote_to_local` 매핑으로 로컬 mirror surface id 를 찾아 `CoreState::set_mirror_surface_busy` 를 호출한다. 이 값은 **`busy_surfaces`(로컬 폴링 결과)와 분리된 `mirror_busy_surfaces` 별도 집합**에 저장된다 — 같은 집합에 합쳤다면 1Hz `refresh_busy_surfaces` 가 매 tick 로컬 폴링 결과로 집합을 통째로 교체하며 mirror 값을 지워버렸을 것이다. `is_surface_busy`/`any_busy`/`busy_count`(사이드바·상태바·탭 바 busy dot·`surface.list` IPC 가 공유하는 단일 진입점)는 두 집합의 합집합을 본다.
- **정리**: mirror surface 가 없어지면(`cleanup_mirror_workspace`, `apply_mirror_structural_delta` 의 removed 처리) `CoreState::forget_mirror_surface_busy` 로 `mirror_busy_surfaces` 에서도 제거해, 로컬 id 가 재사용될 때 stale busy 값이 새 surface 에 잘못 붙는 것을 막는다.

## surface cwd 전파

mirror 터미널은 로컬 PTY 가 없어 `Terminal::get_cwd` 의 pid 폴백이 돌지 않는다 — 원격 셸이 OSC 7 을 방출해 출력 바이트에 실려 올 때만 cwd 를 안다. 서버는 PTY 를 소유하므로 OSC 7 이 없어도 OS 조회로 안다. 그래서 busy·attention 과 같은 형태로 **서버가 cwd 를 push** 한다(server→client 단방향, [ADR-0267](../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md)).

- **서버측 계산·forward**: 같은 1Hz `Tick::Busy` 에서 `CoreState::forward_surface_cwd`(`core/attach_runtime.rs`)가 `surface_cwd_forwards`(`core/state/surface_cwd.rs`)의 계산 결과를 `StreamControl::Cwd{surface_id, cwd}` 로 holder 에 push 한다. 값은 서버 **자기 트리** 기준의 `CoreState::surface_cwd` — terminal 은 OSC 7 캐시 → PTY 프로세스의 OS cwd, 그 외 kind 는 `source_cwd()`(explorer root · markdown 파일 부모) — 이고 **모든 kind** 가 대상이다. `inherit_cwd` 설정은 보지 않는다(관측이지 실행이 아니다). 배선 지점은 busy/attention 과 같은 3 곳(`src/app/busy.rs` 의 main window·parked engine, `src/boot.rs` 의 headless).
- **diff 캐시는 (holder, 값)**: `last_forwarded_cwd` 는 값과 함께 holder 를 기억한다. 값만 기억하면 같은 tick 창 안에서 점유가 풀리고 다른 client 가 잡았을 때 엔트리가 점유 해제 `retain` 을 살아남아 새 holder 가 초기값을 못 받는다. 초기 push 는 값이 `None` 이어도 나간다.
- **값 소멸은 `cwd: null`**: 원격도 cwd 를 모르게 되면 `null` 이 나가고 client 는 엔트리를 지운다 — 표현이 없으면 옛 원격 경로가 영구히 남는다.
- **client 적용**: `MirrorEvent::Cwd(remote_surface_id, cwd)` → 세션 `remote_to_local` 치환 → `CoreState::set_mirror_surface_cwd`. 값은 `Terminal` 캐시가 아니라 **별도 맵 `mirror_surface_cwd`** 에 `RemoteCwd` 로 저장되고, `CoreState::surface_cwd` 가 mirror surface 에 대해 이 값을 우선한다. 원격 경로라 로컬 실행 자리로는 나가지 않는다([surface-cwd §3-2](../architecture/invariants/surface-cwd.md)).
- **정리**: busy 와 같은 teardown 두 곳(`remove_mirror_workspace_from_engine` · delta 의 removed 처리)에 더해, delta 의 survivor **kind 전환 전부**에서 `forget_mirror_surface_cwd` 를 부른다 — busy 는 terminal 에서 출발한 전환만 정리하지만 cwd 는 비-terminal kind 에도 있다.
- **구버전 호환**: 구버전 client 는 `Cwd` 를 파싱 실패로 무시하고, 구버전 server 는 보내지 않아 맵이 빈다 — 그때 mirror convert 는 종전대로 서버측 resolve 폴백(`execute_forwarded_structural_op`)으로 동작한다.

## 주의 환기(attention) 전파

사이드바 워크스페이스 행의 kind 별 개수 배지, surface 테두리, 탭 제목 색([features/surface-highlight](../features/surface-highlight/index.md))은 `AttentionStore`(`CoreState.attention`, `src/core/state/attention.rs`)를 읽는다. 이 store 는 **인스턴스 로컬**이고 인스턴스 간 동기화 개념이 없다 — 그래서 busy 와 같은 이유로 **원격이 자기 surface 의 attention 을 client 로 forward** 한다(순수 **server→client** 단방향).

- **진실 원천은 surface 를 소유한 인스턴스다.** 미러는 서버가 push 한 값을 반영만 하고 자기 판단으로 레코드를 만들지 않는다(아래 "미러의 로컬 발동은 억제된다"). 다만 그 이유가 producer 마다 같지는 않다 — `AttentionKind::NeedsInput` 은 Claude 훅에서만 나오고 그 훅은 PTY 가 있는 쪽에서만 발화하므로 mirror 가 로컬로 도출할 방법이 **원천적으로 없다**(push 가 유일한 물리적 경로). 반면 `Completion` 축의 OSC 133 명령 완료·Bell·OSC 9/777 은 서버 바이트를 파싱하는 미러에서도 **실제로 발화하므로**, 이쪽은 물리적 불가능이 아니라 진실 원천 단일화를 위한 **정책적 억제**다. busy 는 그래도 로컬 폴링을 시도라도 할 수 있다는 점에서 attention 의 조건이 더 강하다.
- **서버측 계산·forward**: busy 와 **같은 1Hz `Tick::Busy`** 에서 `CoreState::forward_attention`(`core/attach_runtime.rs`)이 `attention_forwards`(`core/state/attention.rs`)의 계산 결과 — 점유 중 surface 의 attention 변화분 — 을 `StreamControl::Attention{surface_id, kind}` 로 holder client 에 push 한다. 배선 지점도 busy 와 동일한 3 곳(`src/app/busy.rs` 의 main window·parked engine, `src/boot.rs` 의 headless). `last_forwarded_attention` 캐시가 값이 실제로 바뀐 tick 에만 프레임을 내보내고(스팸 억제), 점유 해제 → 재점유(다른 client 일 수 있음) 시에는 캐시를 버려 값이 같아도 baseline 을 다시 1회 push 한다. 캐시가 (holder, 값) 을 기억하는 것도, 그래서 한 tick 창 안의 holder 교체에서도 새 holder 가 baseline 을 받는 것도 `last_forwarded_busy` 와 동형.
- **해제는 별도 변형이 아니라 `kind: null`**: `Attention` 프레임 하나가 raise/clear 를 모두 표현한다. 프레임은 델타가 아니라 멱등 상태이고, 서버는 client ack 이 아니라 항상 자기 live store 에서 매 tick 재-diff 한다.
- **수렴 보장 범위**: 위 재-diff 가 **보장하는 것은 wire 축의 유실뿐**이다 — 프레임이 드롭되거나 지연돼도 서버 값이 그대로면 다음 tick 에 같은 값이 다시 나가 미러가 서버 기준으로 수렴한다. 미러가 자기 store 를 로컬로 바꾸는 축은 이 재-diff 로 메워지지 않는다(서버는 자기 값이 바뀌지 않는 한 다시 push 하지 않는다) — 그래서 그 축은 자동 수렴이 아니라 전용 장치 둘로 맞춘다. **발동**은 미러에서 아예 일어나지 않게 막고(아래 "미러의 로컬 발동은 억제된다"), **해제**는 그 edge 를 서버로 되돌린다(아래 "미러의 해제는 서버로 전달된다"). 그래서 `src/gfx/gpu.rs` 의 매 프레임 포커스 해제가 mirror surface 를 지우는 것도 divergence 를 만들지 않는다 — 그 제거가 곧 서버로 전달되는 edge 이고, 서버 값이 실제로 내려간다.
- **wire 표현**: `AttentionKindWire`(`crates/tasty-ipc/src/stream.rs`) — host 의 `AttentionKind` 는 `pub(crate)` 이고 `tasty-ipc` 는 host crate 를 의존하지 않으므로 wire 전용 enum 을 두고 경계에서 `AttentionKind::to_wire`/`from_wire` 로 변환한다. 직렬화 문자열(`"completion"`/`"needs_input"`)은 `surface.completion` IPC 의 `kind` 파라미터와 **같은 어휘**다.
- **client 적용**: reader 스레드가 `MirrorEvent::Attention(remote_surface_id, kind)` 로 버퍼링하고, `apply_attach_client_output` 이 세션의 `remote_to_local` 매핑으로 로컬 mirror surface id 를 찾아 `CoreState::set_mirror_surface_attention` 을 호출한다. 값은 busy 와 달리 **별도 집합이 아니라 기존 `AttentionStore` 에 그대로** 들어간다 — busy 가 `mirror_busy_surfaces` 를 분리한 이유는 `refresh_busy_surfaces` 가 매 tick `busy_surfaces` 를 통째로 교체하기 때문인데, attention 에는 그런 wholesale 교체가 없어 분리할 이유가 없고, 같은 store 에 두어야 세 소비처가 코드 변경 없이 미러에서도 동작한다.
- **미러의 로컬 발동은 억제된다 — push 가 유일한 소스**: 미러 터미널은 로컬 PTY 가 없어도 서버가 흘려준 바이트를 그대로 파싱하므로 OSC 133 D·Bell·OSC 9/777 이 미러에서도 발화하고, `surface.completion` IPC/CLI 도 미러 인스턴스에서 로컬 mirror surface id 를 대상으로 불릴 수 있다. 이들이 미러에 레코드를 만들면 같은 사건에 서버·미러가 각각 별개 레코드를 갖는 이중 상태가 되므로, `CoreState::raise_attention` 이 `is_mirror_surface` 로 걸러 **로컬 발동을 no-op** 으로 끝낸다(단일 진입점 집행 — producer 별 게이트가 아니다). 서버 쪽 동일 사건은 서버에서 발동해 push 로 내려오므로 정보는 사라지지 않는다. 억제 대상은 attention 레코드 하나뿐이라 같은 cascade 의 **알림 패널 아이템·토스트·훅 발화는 미러에서도 그대로** 동작한다. 근거·대안: [ADR-0098](../adr/0098-mirror-local-attention-raise-suppressed.md).
- **미러의 해제는 서버로 전달된다 (client→server, 해제 edge 1 회)**: 해제 판정 경로 두 개(실 렌더 포커스 `gpu.rs`, 알림 읽음 `mark_notification_read`/`mark_all_notifications_read`)는 전부 **그 인스턴스의 로컬 GUI 사건**이라, 미러 사용자의 어떤 행동도 서버의 해제 경로를 발동시키지 못한다. 반대로 서버 로컬 사건은 점유 중 그 레코드를 지우지 못하도록 **게이트돼 있다**(아래 "점유 중 해제는 홀더만"). 그대로 두면 **해제 주체가 아예 없어** 서버 배지가 영구히 남는다. 그래서 `CoreState::clear_attention` 이 **실제로 레코드를 제거했는지**를 반환하고(그 `true` 가 곧 edge 다), mirror surface 에서 edge 가 나면 `CoreState::pending_attention_clear_forward` 에 넣는다. App 이 `about_to_wait`(`dispatch_pending_attention_clear_forwards`)에서 drain 해 로컬 id 를 세션 매핑으로 원격 id 로 치환하고 `StreamControl::ClientAttentionClear` 를 보낸다. 서버는 `pump_inbound` 가 `attention_clear_requests` 로 분류하고 GUI(`apply_attention_clear_requests_batch`)/headless(`boot`) 양쪽이 `CoreState::apply_attached_attention_clear` 로 적용한다 — **holder 검증**은 `apply_attached_workspace_resize` 와 같은 형태(anchor surface 의 워크스페이스 holder 여야 한다, ADR-0040). 결정 근거는 [ADR-0104](../adr/0104-mirror-attention-clear-forwarded-to-owner.md).
  - **큐 push 는 호출부가 아니라 `clear_attention` 안에서 한다.** 세 호출부가 모두 미러에서 발생할 수 있고(미러 로컬 Bell/OSC 9 로 만들어진 알림을 미러 알림 패널에서 읽음 처리하는 경우 포함), 포커스 경로에만 큐잉하면 알림으로 확인한 경우가 전달되지 않아 다음 push 에서 배지가 되살아난다. 한 곳에 두면 세 경로가 균일하게 덮이고 해제 producer 가 늘어도 누락이 없다.
  - **주기 전송 없음 — 제거 edge 1 회.** 레코드가 없는 상태의 clear 는 전부 no-op 이라 포커스를 유지해도 프레임이 반복되지 않는다. 별도 last-sent 추적이 필요 없다.
  - **에코 루프 없음.** 미러 clear → 서버 clear → 다음 diff 가 `kind: null` 을 미러로 push → 미러엔 이미 레코드가 없어 edge 가 생기지 않는다(idempotent 수렴). 서버 push 적용(`set_mirror_surface_attention`)과 teardown(`forget_mirror_surface_attention`)이 `clear_attention` 을 타지 않는 것이 이 성질의 근거다.
  - **알려진 엣지(의도된 동작).** 미러가 그 surface 를 **이미 포커스한 상태**에서 서버가 새 raise 를 push 하면, 미러는 레코드를 심자마자 다음 렌더 프레임에서 지우고 해제 edge 를 보내 배지가 사실상 뜨지 않는다. 단일 인스턴스 로컬 동작과 같은 규칙이라(포커스된 surface 에 raise 하면 지금도 다음 프레임에 지워진다) 그대로 둔다.
- **점유 중 해제는 홀더만 — 서버 로컬 사건은 게이트된다 (client→server 의 대칭)**: 하드 점유된 surface 의 주체는 홀더이고 로컬 사용자는 readonly 이므로(ADR-0040), "확인했다" 는 판정도 홀더의 것이다. 위 해제 호출부 셋(실-포커스 `gpu.rs`, 알림 읽음 2종)은 **그 인스턴스의 로컬 사용자 사건**이라 전부 로컬 축 진입점 `CoreState::clear_attention_local` 을 지나고, 그 안에서 게이트 술어 `local_attention_clear_allowed`(= `!OccupancyRegistry::is_hard_occupied`)가 한 번 평가된다. 점유 중이면 제거 edge 자체가 만들어지지 않는다(→ 미러 forward 큐 적재도 없다). 근거·대안: [ADR-0109](../adr/0109-hard-occupancy-attention-clear-holder-only.md).
  - **게이트는 primitive `clear_attention` 이 아니라 래퍼에 있다.** 홀더의 해제를 적용하는 서버측 경로(`apply_attached_attention_clear` → `clear_attention`)가 게이트를 지나면 자기 요청이 막혀 점유 중 해제 주체가 다시 0 이 된다. 그래서 primitive 는 게이트 없이 남고 로컬 축만 래퍼를 탄다 — ADR-0104 가 이 게이트에 걸어둔 제약이다.
  - **알림 읽음 자체는 점유와 무관하게 처리된다.** 게이트가 걸리는 것은 attention 해제 하나뿐이라 `notifications.mark_read`/`mark_all_read` 는 그대로 실행된다. 읽음은 이 인스턴스 사용자의 알림 패널 상태이고, attention 은 홀더와 공유하는 상태다.
  - **soft 점유·미러에는 걸리지 않는다.** 술어가 `is_hard_occupied` 하나라 soft 점유(ADR-0040, write 제한 없음)는 통과한다. 미러 인스턴스도 마찬가지다 — 점유는 surface 를 **소유한** 인스턴스의 `OccupancyRegistry` 에만 기록되므로 미러의 술어는 항상 false 이고, 위 해제 forward 경로가 그대로 성립한다.
  - **데드락 없음.** 게이트는 상태를 저장하지 않고 매 호출 점유를 다시 묻는다 — detach / force-detach / 연결 끊김으로 lock 이 풀리면 서버 로컬 포커스가 자동으로 해제 주체로 복귀해 stale 레코드를 회수한다.
- **적용 API 를 로컬 producer 와 분리한다**: `set_mirror_surface_attention` / `forget_mirror_surface_attention` 은 `raise_attention`/`clear_attention` 을 타지 않는다. 후자는 로컬 producer 축이다 — `raise_attention` 에는 위 "미러의 로컬 발동은 억제된다" 게이트가 있고, `clear_attention` 에는 위 해제 forward 의 큐 push 가 있다. 서버 push 의 적용이 같은 함수를 타면 서버 값이 그 억제 게이트에 막히고, 서버가 내려준 해제가 곧바로 서버로 되돌아가는 에코가 된다.
- **구조 delta 로 갓 생긴 surface 의 baseline**: 서버가 새로 점유된 surface 의 `kind: null` baseline 을 push 하는 tick 이 client 의 `remote_to_local` 갱신보다 앞서면 그 프레임은 매핑 없음으로 버려지고, dedup 때문에 재-push 되지 않는다. 다만 갓 생긴 surface 는 서버에도 attention 레코드가 없어 양쪽이 "레코드 없음" 으로 이미 일치하므로 실질 무해하다. 신규 attach 경로는 애초에 이 경합이 없다 — `start_gui_attach` 가 핸드셰이크 디스크립터로 매핑을 **동기 구성한 뒤** reader 스레드를 띄운다.
- **정리**: mirror surface 가 없어지면(`cleanup_mirror_workspace` 의 세션 정리, `apply_mirror_structural_delta` 의 removed 처리) `forget_mirror_surface_attention` 으로 레코드를 버린다 — 로컬 id 재사용 시 stale attention 이 새 surface 에 붙는 것을 막는다. **kind 변환(terminal → 다른 kind) 경로는 대상이 아니다**: 거기서는 local id 도 surface 도 계속 살아 있고 서버도 여전히 그 remote surface 의 레코드를 들고 있으므로, 미러만 지우면 diff 기반 push 가 값이 안 바뀌었다고 보고 다시 보내주지 않아 세션이 끝날 때까지 복구되지 않는 divergence 가 생긴다(같은 자리에서 busy·mesh 캐시는 지우는 것과 대비 — 그 둘은 새 kind 와 무관해진 로컬 리소스라 버리는 것이 맞다).

## mesh mirror 채널

bundled egui-mesh surface(image/mesh_demo — `is_egui_mesh_allowed` 화이트리스트. markdown 은 Stage B(webview 전환)로 이 화이트리스트에서 빠졌다 — attach 시 mesh 가 아니라 아래 "markdown content 채널" 로 mirror 된다)가 mirror pane 에 뜨면, 원격의 실제 plugin 프로세스가 그리는 GPU mesh 프레임을 client 로 스트리밍해 렌더하고 client 입력을 원격으로 forward 한다. 개념·기능 범위는 [features/remote-attach](../features/remote-attach/index.md#surface-단위-vs-workspace-단위), 헤드리스 부트스트랩·로컬 egui-mesh 파이프라인 자체는 [egui-mesh-channel.md](egui-mesh-channel.md#attach-mesh-mirror-소비-경로), 여기엔 attach 프로토콜 결선만.

- **구독 = `MeshContext` (별도 핸드셰이크 없음)**: client 가 mesh surface 를 그리기 시작하면 `StreamControl::MeshContext{surface_id, width_px, height_px, pixels_per_point, theme, focused}` 를 보내는 것 자체가 구독 신호를 겸한다 — capability negotiation 을 위한 별도 확인/ack 프레임이 없다. 서버 `MeshMirrorRegistry::upsert`(`src/core/mesh_mirror.rs`)가 이 정보를 최초 수신 시점에 등록하고, 이후 값이 실제로 바뀔 때만(geometry/theme/focus 변경 시) client 가 재전송한다(`forward_attach_mesh_context`, `src/view/main/attach_mesh_input.rs`) — 매 프레임 재전송하지 않는다.
- **`MeshInput` 누적**: 포인터/키/스크롤/IME 이벤트는 `AttachMeshForwardState.events`(client, dedup 없이 순서 보존)에 프레임마다 쌓였다가, 다음 redraw 의 `forward_attach_mesh_context` 호출에서 `RawInputWire{modifiers, events, ..}` 로 묶여 `MeshInput{surface_id, input}` 1회 전송된다. 서버는 `MeshMirrorRegistry::push_input` 이 `pending_events` 에 extend 하고 `last_modifiers` 를 갱신 + `dirty=true` 로 표시 — 구독이 없는 surface_id 면 `false` 를 반환해 서버 dispatch 가 `MeshError` 로 회신한다(아래).
- **frame 소비·forward (headless-as-server / gui parked engine)**: 실제 구동/relay 로직은
  `plugin_bridge::mesh_forward::forward_mesh_frames_for_engine`(`src/plugin_bridge/mesh_forward.rs`,
  gui/headless 공용)에 있다 — `mesh_mirror.take_dirty`/`take_pending_events`/`last_modifiers`
  를 읽어 로컬 `PluginManager` 에 `SurfaceSetContextParams.raw_input` 으로 흘려보내고, plugin
  이 그린 결과 mesh 프레임을 `StreamTag::MeshData` 청크로 client 에 push 한다(청크 분할
  이유: 텍스처 델타 포함 paint frame 이 `MAX_FRAME_LEN` 을 초과할 수 있음). 이 함수는 **로컬
  authoritative render loop(살아있는 window)가 없는 engine 전용** — 두 소비처가 있다:
  헤드리스 부트스트랩(`src/boot/headless_plugins.rs::pump_plugins` 가 매 tick 호출)과, gui
  인스턴스에서 macOS 최소화로 window 가 파괴되고 `CoreState` 만 `App::parked_states` 로
  옮겨진 engine(`App::about_to_wait`, `src/app/event_handler.rs` 가 매 tick `parked_states`
  전부를 순회하며 호출 — `window_lifecycle.rs` 의 창 복원이 `remove(0)` 으로 1개씩만
  꺼내므로, 여러 window 가 동시에 최소화돼 있어도 나머지는 계속 이 순회 대상으로 남는다).
  client reader 스레드는 `tasty_ipc::mesh_stream::MeshFrameAssembler` 로 청크를 재조립해
  `MirrorEvent::Mesh(remote_surface_id, generation, frame_seq, full_textures, bytes)` 를
  메인 스레드 버퍼에 쌓고, `AttachMeshFrameStore::update`(`src/core/attach_mesh_frames.rs`)에
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
- **`MeshFullResendRequest` 복구**: 텍스처 델타 체인이 깨졌다고 판단되면(로컬 `EguiMeshRenderTarget` 의 generation 검증 실패 등) client 가 `attach_mesh_full_requests`(`GpuState`)에 surface_id 를 쌓고, `App::dispatch_pending_mesh_full_resend_forwards`가 `MeshFullResendRequest{surface_id}` 를 서버로 forward 한다. 서버는 이를 받으면 `MeshMirrorRegistry::request_full_resend` 로 해당 surface 의 다음 프레임을 풀 텍스처 포함(full_textures=true)으로 강제한다.
- **`MeshError` — 명시적 단발 실패**: 구독 안 된 surface 로의 `MeshInput`(홀더 불일치 포함, `CoreState::apply_attached_mesh_input` 의 holder 검증) 등은 조용히 drop 하지 않고 `MeshError{surface_id, reason}` 를 1회 회신한다(설계 결정: 조용한 drop 보다 명시적 실패가 디버깅에 유리). client 측 소비는 현재 로그 레벨 처리만(재시도/toast 없음) — 세션이 정상이면 애초에 발생하지 않는 방어적 경로.
- **App/CoreState 경계를 건너는 forward-queue 패턴**: `MainView`(redraw 시점)는 `App.attach_client_sessions`(소켓 writer 보유)에 접근할 수 없다. `forward_attach_mesh_context`/입력 캡처(`mouse.rs`/`keyboard.rs`/`ime.rs`)는 `CoreState.pending_mesh_context_forward`/`pending_mesh_input_forward`/`pending_mesh_full_resend_forward` 에 쌓아두기만 하고, `App::about_to_wait`(`attach_client.rs`)의 `dispatch_pending_mesh_*_forwards` 가 다음 tick 에 drain 해 세션 매핑으로 원격 id 를 치환한 뒤 실제 소켓 write 를 한다 — `pending_resize_forward`/`dispatch_pending_resize_forwards`(ADR-0045)와 동형 패턴을 3개 방향(context/input/full-resend)에 재사용한 것.
- **gui-as-server 의 살아있는 window 는 attach client 의 입력 역방향 forward 를 아직 로컬 plugin 에 되먹이지 않는다**: 위 "gui-as-server, 살아있는 window" 훅은 mesh 바이트 forward(서버→client)만 구현한다 — `MeshInput` 으로 도착해 `MeshMirrorRegistry::push_input`/`pending_events` 에 쌓인 attach client 의 클릭/키 입력을 로컬 plugin 의 `raw_input` 에 병합하는 배선은 `forward_mesh_frames_for_engine`(`take_pending_events` 소비)에만 있다 — gui 살아있는 window 는 이 함수를 쓰지 않으므로(경합 회피, 위 참조) 후속 작업으로 남는다. **예외**: gui parked engine 은 headless 와 동일하게 `forward_mesh_frames_for_engine` 을 그대로 쓰므로, window 가 최소화돼 있는 동안은 오히려 입력 forward 가 이미 동작한다 — window 복원 후 살아있는 window 로 전환되면 다시 이 제약이 적용된다.
- **surface 디스크립터의 display_name**: `build_workspace_tree_surfaces` 가 보내는 mesh 디스크립터는 `{remote_id, role:"mesh", kind, plugin_id, display_name}` — `Surface::attach_mesh_info()`(kind/plugin_id 만 반환)와 별개로, 이미 존재하는 `Surface::display_name()`(`EguiMeshSurface` 는 실제 파일명 등을 반환)도 함께 조회해 실어보낸다. client 의 `MirrorMeshInfo` 구성(`merge_survivor_mapping`)은 이 필드를 우선 쓰고, 필드가 없는 경우(구버전 서버 등)에만 `kind` 문자열로 fallback한다 — 과거엔 이 필드 자체가 없어 탭 타이틀이 항상 mesh kind(예: `"image"`)로 뭉뚱그려졌다(image/mesh_demo 전부 동일 증상 — 당시엔 markdown 도 이 경로를 탔으나 Stage B 이후로는 webview 로 이관돼 더 이상 mesh 디스크립터를 타지 않는다).

## markdown content 채널

markdown surface 는 webview kind 라 plugin 에 egui-mesh paint 채널이 없다([ADR-0065](../adr/0065-markdown-webview-render-channel.md)). 그래서 mirror 는 렌더 결과가 아니라 **원문 문자열**을 나르고, client 의 markdown plugin 이 자기 테마·자기 recent 로 다시 그린다. 결정의 근거·대안·재검토 조건은 [ADR-0255](../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md), 여기엔 결선만.

- **핸드셰이크는 좌표만 싣는다**: `{remote_id, role:"markdown", file, display_name}`. 원문은 트리 디스크립터를 문서 크기만큼 부풀리므로 싣지 않는다 — client 가 필요할 때 아래 채널로 따로 가져온다(lazy).
- **client 의 surface 구성**: `merge_survivor_mapping`(`src/app/attach_client.rs`)이 `role:"markdown"` leaf 를 `EmptySurface` 대신 registry 의 `markdown` kind 로 만든다 — 조건은 그 kind 가 **번들 plugin(`com.tasty.markdown`)의 것**인가 하나다. kind 가 **아직 등록되지 않았으면**(꺼져 있던 plugin 을 세션 중에 켜는 경우 등) leaf 는 layout 복원과 같은 kind 대기 placeholder(`EmptySurface::new_deferred_plugin`, `deferred_mirror_markdown_surface`)가 되고, 표시 시점의 reify(`CoreState::reify_plugin_surface`)가 kind 등록 뒤 registry 의 `restore` 로 실제화한다 — plugin 에는 `surface.restore` 의 data 로 생성 params 와 같은 모양이 가서 plugin 은 어느 경로든 `remote` 키로 mirror 문서임을 안다(탭 제목은 그 경로에 생성 params 가 없어 plugin 이 응답의 `display_name` 으로 돌려준다). 같은 이름을 **다른** plugin 이 이미 등록했으면 대기하지 않고 빈 surface 로 남는다 — reify 는 소유자를 가리지 않아 그 plugin 으로 실제화되기 때문이다. 생성 params 는 `{display_name, remote:{file}}` 이다: 최상위 `file` 이 아니라 `remote.file` 에 싣는 것은 plugin 이 그 경로로 **자기 로컬 파일을 읽지 않게** 하려는 것이다(최상위 `file` 은 로컬 열기·cwd 도출·recent 기록을 모두 켠다). 구조 delta 로 트리가 다시 와도 이미 markdown 이던 leaf 는 `RemoteSurface::share_handles` 로 **같은 plugin surface 를 재사용**한다 — 새로 만들면 plugin 이 문서를 다시 받는다. `RemoteSurface` 는 drop 이 plugin 에 destroy 를 보내지 않으므로, 트리에서 빠진 leaf·재연결로 사라진 leaf·mirror 정리는 `destroy_mirror_markdown_surfaces` 가 명시적으로 destroy 한다. 세션이 든 로컬 markdown id 집합은 `AttachClientSession.markdown_locals` 이고 아래 요청·회신의 인가 대상도 이 집합이다.
- **plugin → host 요청**: markdown plugin 은 `markdown_mirror.content_request {surface_id(로컬), agent_origin?}` 를 호출한다(`agent_origin: true` 는 바깥 호출자의 `markdown.reload` 가 건 요청 표시 — 아래 절단 표시 참조. `src/adapters/ipc/handler/markdown_mirror.rs`, 권한 `fs.read`). host 는 `request_id` 만 즉시 회신하고 `CoreState.pending_markdown_content_forward` 에 쌓는다 — 원문은 attach 왕복을 기다려야 해서 `git_viewer.query` 와 같은 비동기 accept 다. namespace 가 `markdown.` 이 아닌 것은 그 prefix 가 plugin 으로 forward 되기 때문이다([ADR-0153](../adr/0153-a-bundled-namespace-hands-host-methods-back.md) 참고). `App::dispatch_pending_markdown_content_forwards`(`about_to_wait`)가 drain 해 원격 id 로 치환한 뒤 아래 조회 이벤트를 보낸다. 세션을 못 찾거나 송신이 실패하면 그 자리에서 같은 `request_id` 로 `ok:false` 결과를 plugin 에 돌려준다.
- **host → plugin 결과**: 회신 이벤트는 로컬 id 로 치환돼 `markdown_mirror.content_result {surface_id, request_id, ok, file, source, truncated, reason}` host event 로 번들 plugin 에만 unicast 된다. 연결이 끊겨 Reconnecting 에 들어가면(`enter_reconnecting`) 그 세션의 모든 markdown leaf 에 `request_id: 0` + `ok:false` 를 보낸다 — `0` 은 발급되지 않는 값이라 "연결이 끊겼다" 는 abandon 신호다. plugin 은 대기 중인 요청을 끝내고, 원문을 이미 표시 중인 문서도 **끊김 상태**로 둔다(`MdDoc::apply_remote_result`) — 끊김 상태의 렌더는 로딩·실패·원문보다 앞서 끊김 문구 하나다(`render_document`). 끊김은 다음 성공 회신이 지운다. **재연결**(`reconnect_session`)은 survivor leaf 의 plugin surface 를 `share_handles` 로 이어 받아 원문 요청이 저절로 나가지 않으므로, `Connected` 로 되돌린 직후 세션의 로컬 markdown id 마다 아래 변경 신호(`markdown_mirror.changed`)를 한 번 보낸다. 근거·대안은 [ADR-0268](../adr/0268-a-disconnected-mirror-markdown-shows-the-disconnect-and-reconnect-refetches.md).
- **조회 (client→server)**: `{"event":"markdown_content_request", "request_id", "surface_id"}`. `list_dir`/`git_query` 와 같은 형태 — `StreamControl` enum **밖**의 raw JSON `event` 태그를 같은 `StreamTag::Control` 채널에 싣고, 서버는 알 수 없는 event 를 조용히 무시한다(전방/후방 호환). 파싱은 `tasty_ipc::stream_hub::MarkdownContentRequestMsg`, 소비는 `event_handler.rs::apply_markdown_content_request_msg`(gui, holder engine 순회)와 `boot/headless_stream.rs`(headless, 단일 engine).
- **회신 (server→client)**: 성공은 `{"event":"markdown_content_result", request_id, surface_id, ok:true, file, source, truncated}`, 실패는 같은 event 에 `ok:false` + `reason`. **에러 채널은 하나다** — client 가 그 `reason` 을 렌더의 `load_error` 로 옮긴다. **파일 없이 열린 markdown surface 는 에러가 아니다**: 서버에서도 빈 문서가 보이므로 `ok:true` + 빈 `file`/`source` 로 답한다.
- **파일은 서버 host 가 직접 읽는다** (`attach_runtime.rs::handle_markdown_content_request`). 서버측 markdown plugin 에 되묻지 않는다 — 이 핸들러는 동기 경로이고 plugin 왕복은 비동기라 그 안에서 기다릴 수 없다(`handle_git_query_request` 가 `tasty-git-core` 를 host 에서 직접 부르는 것과 같은 형태). 에러 구분 수준도 `list_dir_for_request` 와 같다(`permission denied` 대 그 외 io 에러 문자열).
- **예산**: `MARKDOWN_CONTENT_BYTE_BUDGET = 700 KiB` — `LIST_DIR_ENTRIES_BYTE_BUDGET`·`GIT_QUERY_BYTE_BUDGET` 과 같은 근거(프레임 하드 상한 `MAX_FRAME_LEN` 1 MiB 보다 충분히 작게)이자 **같은 재는 대상**: 원문 바이트가 아니라 `serde_json` 문자열이 된 뒤의 바이트다(감싸는 따옴표 포함). 이스케이프는 `"`·`\`·개행에서 2 배, 그 밖의 제어문자에서 6 배(`\u00XX`)까지 부푸므로 원문으로 재면 이스케이프가 많은 문서가 예산을 통과한 뒤 프레임 상한을 넘어 세션이 끊긴다 — 예산이 막으려던 바로 그 사고다. 그래서 실리는 **원문** 길이는 이스케이프가 많을수록 짧다(최악 약 116 KiB). 비용표는 `json_escaped_char_len` 이고 BMP 전수 대조 테스트가 serde_json 과의 어긋남을 잡는다. 자르는 자리는 **UTF-8 문자 경계**(`char` 단위로 걷는다), 잘리면 `truncated: true` 로 알린다. markdown plugin 의 대용량 게이트(1 MiB)와는 다른 층이며, 그 게이트를 건드릴 파일은 직렬화하면 더 커지므로 항상 잘려서 도착한다.
- **인가**: `client_holds_workspace(client_id)` 하나("attach 점유 = 신뢰"). 새 permission 토큰을 만들지 않는다 — 이 채널이 나르는 것은 SSH 로 붙은 사용자가 이미 읽을 수 있는 그 호스트의 파일 원문이다. **인가되는 집합은 engine 전체다** — 그 술어는 **어떤** 워크스페이스든 점유했는가만 보고(`workspace_locks` 전체 스캔), 대상 조회는 `find_surface_by_id`(전 워크스페이스 순회)라 W2 만 점유한 client 도 W1 의 markdown 원문을 받는다. list_dir(wire 에 `dir` 문자열만·워크스페이스 바인딩 필드 없음)·git_query(engine 전역 `TerminalStore` 조회 또는 임의 `worktree_path`)와 같은 갈래이며, anchor 의 holder 를 직접 검증하는 채널(입력·resize·구조 op forward, `workspace_holder(ws) == client`)과는 다르다.
- **절단 표시**: client 는 `truncated: true` 를 **toast**(`attach.toast.mirror_markdown_truncated`, `apply_markdown_content_result_event`) 로 알리고 문서 본문에는 심지 않는다 — `list_dir` 의 `filepicker.remote_listing_truncated` 선례와 같다(본문에 심으면 원문의 일부인지 구분되지 않고, markdown 은 심는 자리가 코드펜스 안일 수 있다). 단 `agent_origin: true` 로 건 요청(에이전트의 `markdown.reload`)의 회신은 toast 없이 로그로만 남긴다 — 송신 때 세션의 `agent_requests`(`src/app/attach_client/agent_origin.rs`)에 request_id 를 넣고 회신 때 꺼내 가른다([ADR-0503](../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md)).
- **변경 신호 (server→client)**: `{"event":"markdown_changed", "surface_id"(원격)}`. 발신은 `attach_runtime.rs::notify_markdown_changed` 이고 호출처는 `webview.set_url` 핸들러(`src/adapters/ipc/handler/webview.rs`) 하나다 — markdown plugin 의 재렌더(파일 감시·`markdown.reload`·테마 변경·최초 생성)는 전부 그 IPC 로 host 에 오므로 plugin 을 고치지 않고 신호원을 갖는다. 그래서 신호는 실제 파일 변경의 **상위 집합**이다. 화이트리스트(`is_attach_content_allowed`) 밖 kind 는 신호가 없다. 수신자는 `OccupancyRegistry::workspace_holders` — 위 인가 술어가 참인 그 집합(이 engine 의 워크스페이스를 하나라도 점유한 client 전부, 중복 없이)이고, 그 surface 를 담은 워크스페이스의 holder 로 좁히지 않는 근거는 [ADR-0255](../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md) 항목 5 에 있다. 점유가 없거나 notifier 가 없으면 아무것도 안 한다. client 는 reader 체인의 `parse_markdown_changed` → `MirrorEvent::MarkdownChanged` 로 받아 `markdown_mirror_local` 로 로컬 mirror markdown leaf 에 매핑되는 것만 `markdown_mirror.changed {surface_id(로컬)}` 로 plugin 에 unicast 한다(자기가 mirror 하지 않는 문서의 신호는 버린다). plugin 의 반응은 문서가 무엇을 보여 주는가로 갈린다(`MdDoc::on_remote_changed`): 원문을 보여 주는 중이면 **다시 받지 않는다** — 새로고침 버튼 색만 바꾸고, 원문은 사용자가 누를 때 위 조회로 온다. 원문을 못 보여 주는 중(끊김·실패·로드 전)이고 요청이 진행 중이 아니면 다시 요청한다(재연결 신호가 이 갈래를 쓴다). 요청이 진행 중이면 무시한다. **서버가 헤드리스면 신호가 없다**: `webview.set_url` 이 gui 전용이라 헤드리스에서는 plugin 의 재렌더가 host 에 닿지 않는다(조회·회신은 헤드리스에서도 된다). 테스트: `attach_runtime.rs` 의 `markdown_changed_tests`(수신자 집합·중복 없음·점유 없음·화이트리스트 밖). `webview.set_url` 에서 그 함수로 이어지는 호출은 `webview.rs` 의 `set_url_on_markdown_surface_signals_attached_clients` 가 잰다(점유 client 의 스트림에 신호 한 번, 터미널 leaf 에는 없음 — 호출을 빼는 변이에서 실패한다).
- **서버는 헤드리스여도 된다.** 이 채널의 서버측 코드에는 feature 게이트가 하나도 없고(파일 read + control 프레임), 요청 소비도 두 조합에 각각 있다(gui `event_handler.rs`, headless `boot/headless_stream.rs`). 다만 **surface 를 만들 수 있어야** 그 앞이 성립하는데, 한때 헤드리스가 `webview` kind 를 등록하지 않아 `tab.create {type:"markdown"}` 이 `unknown surface kind` 로 죽었다 — 지금은 `boot/headless_plugins.rs` 의 `register_one_surface_kind` 가 세 rendering 을 전부 등록하고, 그 kind 를 지목한 생성 요청이 plugin 기동을 유발한다(`ensure_plugin_for_surface_kind`). 판정과 실측은 [headless-ipc-surface.md](headless-ipc-surface.md) 의 "등록된 kind 조회" 절.
- **테스트**: `tests/attach_markdown_content_loopback.rs` 가 role 직렬화·왕복·없는 파일·점유 없는 client 거절·예산 초과 절단·**다른 워크스페이스만 점유한 client 의 인가**(engine 전체임을 값으로 고정) 여섯을 loopback 프로토콜 레벨로 고정한다. 그 여섯의 **자동 채널은 헤드리스 조합 하나뿐**이다(`check-headless` 의 전체 스위트 — [ci-gates.md](ci-gates.md)), 그래서 위 항목이 헤드리스에서 성립하지 않으면 이 시험들에는 도는 자리가 없다. 절단 테스트는 본문을 따옴표로만 채워(원문 400 KiB → 직렬화 800 KiB) **무엇을 세어 잘랐는지**까지 잰다 — 원문으로 쟀다면 잘리지 않고 통과한다.

## mirror 구조 변경 forward

mirror 워크스페이스의 구조 변경(split/new-tab/close/move-tab/닫은 항목 복원)은 로컬에서 실행하지 않고 원격(authoritative)에서 실행되도록 forward 한다. intent 로 표현되는 경로(split·CLI/IPC)는 `Core::apply` 로 수렴해 거기서 forward 큐에 쌓이고, `Core::apply` 를 우회하는 UI-layer 직접 조작(`close_active_*`/`close_tab`/`add_tab`/`add_kind_tab`, 탭 드래그·컨텍스트 메뉴 이동)은 `AppState::forward_mirror_structural` 가 같은 큐(`pending_structural_forward`)에 대응 op 를 직접 쌓는다 — 두 경로 모두 동일 큐로 모여 아래 결선을 공유한다. 개념·정책은 [features/remote-attach](../features/remote-attach/index.md#mirror-워크스페이스-내-구조-변경), 여기엔 결선만.

- **request (client→server)**: `Core::apply` 가 mirror 구조 op 를 로컬 차단(`MirrorStructuralBlocked`)하면서 `StructuralOp`(anchor = **로컬** surface id)를 `CoreState.pending_structural_forward` 에 push. `App::dispatch_pending_structural_forwards`(`about_to_wait`, gui)가 drain 해 anchor 를 세션 매핑(`AttachClientSession.remote_to_local` 역방향)으로 **원격 id 로 치환**(`StructuralOp::with_anchor_surface_id`)한 뒤 `StreamTag::Control` 로 전송. anchor 를 surface id 로 잡는 이유: client 는 pane/tab 의 원격 id 매핑을 갖지 않으므로, 원격이 surface 로부터 pane/tab/workspace 를 자기 트리에서 resolve 한다.
- **진입 경로별 응답 정합성**: mirror 구조 op 는 사용자 GUI·에이전트 CLI/IPC 어느 경로든 `Core::apply` 로 수렴해 forward 된다(에이전트의 원격 mirror 조작은 금지가 아니라 정식 기능 — 원칙2). `MirrorStructuralBlocked` 의 `forwarded` 플래그가 "로컬 차단했으나 원격으로 큐잉됨" 을 표시하며, 두 경로가 이를 **성공으로** 처리한다: GUI 는 `report_apply_error` 가 `forwarded` 마커를 삼켜 차단 toast 를 띄우지 않고(성공 무음, 실패는 회신 toast — 에이전트 origin intent 의 op 는 회신 실패도 로그), IPC/CLI 는 구조변경 핸들러(split·tab.create/close/move·pane.close·surface.close·`image.open`)가 `structural_apply_error`(`adapters/ipc/handler.rs`)로 `{forwarded:true, workspace_index}` **success** 를 회신한다(이 핸들러들은 에이전트 요청이라 그 forward op 에 `silent_failure` 를 붙여, 원격 실패가 사용자 toast 가 아니라 warn 로그로 간다 — `structural_exec::apply_as_agent` · [ADR-0503](../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) 보강 절). forward 는 비동기라 이 응답은 "원격에 큐잉됨" 을 뜻하고 실제 결과는 역반영 delta(아래)로 관측한다 — 성공을 `internal_error` 로 오보하지 않는다. `markdown.navigate`는 예외로 intent 출구(`out.push`)에 큐잉만 하고 항상 즉시 `{accepted:true}` 를 회신한다(실제 forward/실행 결과는 이 응답에 안 실린다 — mirror 에서의 원격 실패는 에이전트 origin 이라 사용자 toast 가 아니라 warn 로그로만 남고(`report_apply_error` 가 그 forward op 에 `silent_failure` 를 표시한다 — [ADR-0503](../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md)), 로컬 워크스페이스에서의 실패도 warn 로그뿐이다. 같은 op 를 사용자가 GUI 로 하면 원격 실패는 forward 실패 toast(`attach.toast.mirror_structural_forward_failed`)로 드러난다). `forwarded:false`(mirror↔local 경계를 넘는 move-surface 등 forward 불가 op)는 기존대로 에러.
- **실행 (server)**: `StreamHub::pump_inbound` 이 client Control 프레임을 `StructuralOp` 로 분류(`PumpOutcome.structural_ops`) → 메인루프(gui `event_handler`/headless `boot`)가 anchor 워크스페이스의 **holder 를 검증**(hard 점유 = 구조 변경 권한, ADR-0040)한 뒤 `execute_forwarded_structural_op`(`core/attach_runtime.rs`)로 실행. split/tab.create/tab.close/tab.move/pane.close/surface.close 6종은 IPC 핸들러가 부르는 것과 같은 도메인 실행 함수(`core/structural_exec.rs`)를 부르고(핸들러는 거치지 않는다 — ADR-0395), convert/move-surface 는 대응하는 도메인 실행 함수가 없어(IPC 쪽도 image/markdown 처럼 kind 별 전용 핸들러만 존재) `Core::apply(DomainIntent)` 를 직접 호출한 뒤 그 cascade(PTY cleanup·egui-mesh stale-frame 방지)도 이 함수 안에서 재현한다. 서버 ws 는 mirror 가 아니라 `Core::apply` 가드를 통과해 실제 PTY 를 spawn 한다.
- **회신 (server→client)**: 실행 결과를 `StructuralResult{op_id, ok, reason?}` 로 push. 원격 미등록 kind 는 `create_surface_via_registry` 가 Err → `ok:false`+reason. client reader 가 실패 회신을 `MirrorEvent::StructuralFailed` 로 올려 실패 toast. 성공은 무음. anchor 가 점유 워크스페이스에서 안 풀리면 도메인 실행 전에 호출측(gui `apply_forwarded_structural_op` · headless `apply_structural_ops`)이 거절한다 — 요청 client 가 이 인스턴스에서 **살아 있는 워크스페이스를 점유 중이면** 사라진 것은 anchor surface 이므로(서버에서 PTY 가 끝나 먼저 닫힌 surface 를 늦게 지목한 경우) IPC 의 같은 상황과 같은 생성기로 `no live surface N (named by 'structural_op.<kind>'); …` 를 싣고(`attach_structure_sync::unresolved_anchor_reason`), 점유 워크스페이스가 없거나 anchor 가 인스턴스 어딘가(다른 워크스페이스 · 다른 창 engine)에 살아 있으면 `workspace not found` 다 — 살아 있는 surface 를 "no live" 라 하지 않는다([ADR-0482](../adr/0482-a-forward-naming-a-gone-surface-is-answered-like-ipc.md)). 시험: `tests/attach_structure_sync_loopback.rs`(gui·headless 두 조합에서 같은 시험).
- **역반영 (server→client) — 성공한 forward 와 서버 쪽 구조 변경**: 성공한 forward 는 원격에 생긴/사라진 surface 를 mirror 트리에 반영한다. `execute_forwarded_structural_op` 이 실행 **전/후** anchor 워크스페이스의 `all_surface_ids` diff 로 added(신규 터미널)를 계산하고, 실행 후 전체 트리+surfaces(`build_workspace_tree_surfaces`, 핸드셰이크 디스크립터와 공유)를 `StructuralDelta{workspace_id, tree, surfaces}` 로 만든다. 순서는 **`StructuralResult` → `StructuralDelta` → 신규 surface tap**(`tap_surface_for_stream`; client 가 매핑을 만든 뒤 스냅샷을 받도록 tap 을 delta 다음으로). 핸들러 응답 파싱이 아니라 트리 diff 라 close cascade·move-tab 도 균일 커버. client(`apply_mirror_structural_delta`)는 surfaces 를 세션 매핑과 대조해 survivor(기존 remote_id)는 **로컬 id 재사용**(터미널 재생성 금지 → scrollback 보존), 신규는 `Terminal::new_detached`+입력 forwarder, 사라진 것은 제거한 뒤 `build_mirror_workspace` 를 재실행해 같은 local ws id 로 in-place 교체한다. pane 상위 배치는 핸드셰이크와 동일한 방식(`to_attach_tree_json`의 `pane_layout` 트리 필드, direction/ratio 보존)으로 정확히 승계된다(3단계도 동일 정합성).
- **forward 가 아닌 원인의 구조 변경도 같은 메시지로 역반영한다**: 점유 워크스페이스의 구조는 forward 말고도 바뀐다 — 서버에서 PTY 가 끝나 surface/tab/pane 이 닫히거나(`app::process_exit::handle`), 로컬 생성 경로로 멤버가 편입될 때(`tap_new_workspace_member`). 새 wire 메시지를 만들지 않고 **기존 `StructuralDelta` 를 기존 의미(실행 후 전체 트리)로** 보낸다([ADR-0481](../adr/0481-a-server-side-structure-change-reaches-the-holder-as-a-delta.md)). 표시는 `OccupancyRegistry` 가 워크스페이스 단위로 쌓는다 — `forget_closed_surface` 가 점유 워크스페이스의 멤버를 잊을 때와 `mark_structure_changed`. 비우는 것은 `CoreState::push_structure_changes`(`src/core/attach_structure_sync.rs`) 이고 부르는 자리는 셋이다: PTY 종료 처리 끝, 로컬 멤버 편입(tap **전** — client 가 매핑을 만든 뒤 스냅샷을 받게, forward 의 "delta → tap" 과 같은 순서), 두 빌드의 `StreamReady` 처리 끝(그 밖의 원인이 남긴 표시를 다음 스트림 활동에서 비운다). forward 실행은 자기 delta 를 호출측이 보내므로 anchor 워크스페이스의 표시를 지워 같은 트리가 두 번 나가지 않게 한다. 워크스페이스가 통째로 사라졌으면(마지막 멤버의 PTY 종료) 보낼 트리가 없어 forward 경로의 같은 상황과 똑같이 `force_detach_workspace` 로 holder 를 끊고 lock 을 정리한다. 서버 IPC 로 점유 워크스페이스의 구조를 바꾸는 요청은 하드 점유 가드가 거절하므로(아래 "서버 로컬(비-holder) 구조 변경 차단") 대개 이 경로의 원인이 되지 않는다 — 가드 밖에 남은 `pty.attach_surface` 는 로컬 멤버 편입으로 여기 들어온다. 시험: `forward_exec_tests` 의 `a_pty_exit_in_an_attached_workspace_reaches_the_holder_as_a_delta` · `a_forwarded_close_leaves_no_structure_change_behind` · `the_last_member_exiting_force_detaches_the_holder` · `a_local_member_reaches_the_holder_as_a_delta_before_its_snapshot`.
- **focus 보존 (client-only 보정)**: 위 트리 재구성이 실어 보내는 `tree.focused_pane`/pane 별 `active_tab` 은 **원격의** 값이다 — 순수 pane/tab 전환(클릭·키보드 이동)은 forward 되는 `StructuralOp` 가 없으므로(위 목록에 없음), 원격의 이 값은 대개 워크스페이스 생성 시점(첫 pane/첫 탭)에 고정된 채 절대 바뀌지 않는다. 과거엔 매 delta 마다 이 고정값을 그대로 실어 로컬 워크스페이스를 통째로 교체해, 사용자가 로컬에서만 다른 pane/탭으로 이동해둔 focus 가 구조 변경(새 탭/닫기 등) 때마다 첫 pane/첫 탭으로 되돌아가는 버그가 있었다. `apply_mirror_structural_delta` 는 교체 **전** 로컬에서 실제로 focus 돼 있던 surface 를 remote id 기준으로 캡처(`capture_focused_remote` — local pane/tab id 는 매 delta 마다 재발급돼 불안정하므로 remote surface id 를 기준으로 삼는다)해뒀다가, 교체 **후** 새 트리에서 그 surface 의 위치를 찾아(`find_pane_and_tab_for_surface`, 실제 갱신은 공용 헬퍼 `set_focus_to_surface`) `ws.focused_pane`/해당 `Pane.active_tab`/그 `Tab.focused_surface` 를 되돌린다(`restore_focus_after_delta`, 성공하면 `true`). 서버 상태는 전혀 건드리지 않는 순수 client-side 보정이다.
- **신규 리소스로 focus 추적 + close 인접 fallback (client-only, 사용자 GUI 조작 한정)**: 위 "옛 focus 복원"만으로는 두 상황을 못 다뤘다 — (1) 사용자가 mirror 안에서 새 탭/split 을 만들면 옛 focus(=새 리소스를 만들기 전 위치)가 그대로 살아남아 복원되므로 새로 생긴 리소스로 focus 가 전혀 안 옮겨간다. (2) focus 중인 surface 자체를 닫으면 캡처한 옛 focus 가 이번 op 로 사라져 복원이 실패하고, 그 경우 재구성된 `ws` 는 원격의 고정값(대개 워크스페이스의 첫 pane/첫 탭/첫 surface)을 그대로 담고 있어 그리로 튄다.
  - **user_triggered 태그**: `CoreState.pending_structural_forward` 의 각 원소(`PendingStructuralForward{op, user_triggered, close_focus_candidates}`)는 이 op 이 실제 사용자 GUI 조작(단축키/버튼/컨텍스트 메뉴)에서 나왔는지를 함께 싣는다. `AppState::forward_mirror_structural`(UI-layer 직접 조작 경로)는 호출부가 전부 GUI 라 항상 `true`. `Core::apply` 의 mirror-block 경로는 origin 을 모르므로 기본 `false`로 push 하고, origin 을 아는 GUI intent 핸들러(`intent::pane`/`intent::surface`/`intent::tab`)가 `core.apply` 실패 시 `crate::core::mark_last_forward_user_triggered` 로 방금 push 된 마지막 op 를 `origin.is_user()` 일 때만 뒤집는다 — IPC/CLI/플러그인 유래는 이 뒤집기가 없어 항상 `false` 로 남고, 그 결과 기존과 동일하게 focus 가 움직이지 않는다("포커스 독립성" 유지).
  - **close 인접 후보 계산(client, 닫기 전)**: `close_active_surface`/`close_active_tab`/`close_tab`(`state/pane.rs`, `state/tab.rs`)은 forward 하기 **직전** — 아직 로컬 트리가 마지막 delta 기준 그대로일 때 — 닫히는 surface/tab 이 focus 였을 경우의 client-side fallback 후보(로컬 surface id, 우선순위 순)를 계산해 `close_focus_candidates` 에 싣는다: split 된 tab 이면 같은 tab 안의 다른 leaf surface, split 안 된 tab(닫으면 탭 전체가 사라짐)이면 같은 pane 의 다른 탭의 focused surface(로컬의 "탭 하나만 남기고 닫음" 규칙과 동형 — 마지막 탭이 아니면 다음 탭, 마지막이면 이전 탭이 1순위). `close_active_pane`(pane 자체를 닫음)은 후보를 계산하지 않는다 — 로컬도 pane cascade close 에서는 무조건 첫 pane 으로 이동하므로 이 스코프 밖.
  - **op_id 상관 + PendingOpFocus**: `forward_one_structural_op` 이 op 을 세션에 실어 보낼 때, `user_triggered` 면 `pending_op_focus_for(op, close_focus_candidates, remote_to_local)` 로 이 op 이 `NewResource`(new-tab/split)인지 `Close{candidates}`(close 계열, 로컬 후보를 원격 id 로 치환한 것)인지 판정해 `AttachClientSession.pending_op_focus: HashMap<op_id, PendingOpFocus>` 에 등록한다. 이 op 의 성공 회신(`StructuralResult{ok:true, op_id}`, reader 가 `MirrorEvent::StructuralSucceeded(op_id)` 로 전달)이 오면 그 엔트리를 `next_delta_focus` 로 옮겨두고, 프로토콜이 보장하는 대로 그 직후 도착하는 `StructuralDelta` 적용 시 1회 소비(`take`)한다.
  - **적용 순서(`apply_mirror_structural_delta`)**: `NewResource` 면 이번 delta 의 surfaces 중 이전 매핑에 없던(=새로 생긴) remote id 를 찾아 그 local surface 로 `set_focus_to_surface` — 옛 focus 복원은 아예 건너뛴다(안 그러면 옛 focus 가 거의 항상 살아남아 복원이 새-리소스-focus 를 덮어써 버린다). 그 외의 경우 기존처럼 `restore_focus_after_delta` 를 먼저 시도하고, **그게 실패했을 때만**(=캡처해둔 focus 가 이번 op 로 사라짐) `Close{candidates}` 가 있으면 후보를 원격 id 순서대로 순회해 delta 이후에도 살아있는 첫 번째로 `set_focus_to_surface` — 후보가 다 없으면(예상 밖) 기존 그대로 원격의 고정값이 남는다.
  - **서버는 무관**: 위 어떤 보정도 `ws.focused_pane`/`pane.active_tab` 등 **서버측 상태를 바꾸지 않는다** — client 가 재구성한 로컬 `Workspace` 값만 조정하는 순수 client-only 보정이다. IPC/CLI(`tasty new tab` 등 진짜 에이전트 호출)로 같은 mirror workspace 를 조작하면 `user_triggered=false` 로 남아 이 보정 전부가 스킵되고, focus 는 여전히 움직이지 않는다(회귀 없음).
- **닫은 항목 복원(`RestoreClosedItem`)**: forward 대상이다 — 복원은 새 PTY spawn 이고 스냅샷의 스크롤백은 서버 디스크 참조(`ClosedScrollback::Persisted`)라 서버만 실행할 수 있다([ADR-0264](../adr/0264-mirror-restore-closed-item-runs-on-the-remote.md)). 무엇이 복원될지는 클라이언트가 정하지 않고 **서버 스택이** 정하므로 op 에는 anchor 밖에 없다. **원격 스택이 비어도 로컬 스택으로 폴백하지 않는다** — 그 폴백의 결과가 곧 이 결선이 없애려던 버그(로컬 PTY 탭이 mirror 안에 생김)다. 실패 회신은 일반 forward 실패와 구분된 사유(`restore: nothing to restore in this workspace`)로 나가고 client 는 전용 안내 toast(`attach.toast.mirror_restore_empty`)를 띄운다.
  - **서버 복원 스택은 워크스페이스로 스코프된다**: `ClosedItemStore` 의 각 엔트리가 닫힐 당시의 출처 워크스페이스 id 를 함께 싣고(`CoreState::push_closed_item` 이 항목의 구조 id 로 판정 — push 는 항상 트리 재배치 **전**이라 그 시점 트리가 답을 갖는다), forward 된 복원은 **anchor 워크스페이스 출처 항목만** pop 한다(`RestoreScope::Workspace`). 서버 로컬 복원은 **기존 전역 LIFO 그대로**다(`RestoreScope::Local`) — forward 된 close 는 서버 자신의 트리에서 탭을 없애므로 서버 앞 사용자에게도 undo 가 남아야 한다. 워크스페이스 통째 항목은 출처가 `None` 이라 forward 스코프에 원리적으로 안 걸린다 — 그 갈래는 anchor 밖에 새 workspace 를 만들어 delta 에 안 잡히므로, pop 뒤 거부(항목 유실)가 아니라 pop 이전 배제로 닫는다.
  - **forward 된 close 는 원격 사용자가 손으로 닫았을 때만 서버 복원 스택에 남는다**: forward op 는 사용자 GUI 에서도, 에이전트 CLI/IPC 에서도 온다(위 "진입 경로별 응답 정합성"). 그래서 `StreamControl::StructuralOp` 가 선택 칸 `origin`(`"user"` | `"agent"`)을 싣는다 — 클라이언트는 forward 큐 원소의 `user_triggered` 로 채워 언제나 명시하고, 서버는 **칸이 없으면 `user`** 로 읽는다(`ForwardOrigin::of_wire` — 칸 이전 클라이언트의 close 는 전부 복원 가능했으므로 종전 동작 그대로). 칸을 이해하는 서버는 **모르는 값을 거절하지 않고 `agent` 로** 읽는다 — 파생 역직렬화였다면 프레임이 통째로 버려져 회신 없이 사라진다. 모르는 값을 복원 스택에 안 넣는 쪽으로 읽는 것이 원칙 1 쪽이다. `user` close 는 `CloseSurface` 가 도메인 함수 `structural_exec::close_surface` 를 `save_snapshot=true` **인자**로 부르고(IPC 핸들러는 `false` 로 부른다 — params 로는 표현하지 않는다), `CloseTab`/`ClosePane` 은 `apply_close_*` 에 스냅샷 축이 없어 `execute_forwarded_structural_op` 이 도메인 함수 호출 **전** `CoreState::capture_closed_tab`/`capture_closed_pane` 으로 직접 캡처해 `push_closed_item` 한다(pane 은 트리 재배치 전이어야 split context 가 남는다). `agent` close 는 `save_snapshot=false` 로 부르고 캡처하지 않는다 — 복원 스택은 사용자 상태라 에이전트 행동이 닿으면 안 된다([identity](../identity.md) 원칙 1, [ADR-0480](../adr/0480-a-forwarded-close-carries-who-asked-for-it.md)). 옛 서버는 모르는 칸을 버리므로(serde 기본) 새 클라이언트 × 옛 서버에서는 에이전트 close 도 종전처럼 남는다. `is_user_close` 는 독립 축이라 origin 과 무관하게 그대로 `false` 다.
  - **실행은 `Core::apply` 직접 호출이다**: 복원은 IPC/CLI 로 노출된 적이 없어 대응하는 도메인 실행 함수가 없다 — `ConvertSurface`/`MoveSurface` 와 같은 형태로 `execute_forwarded_structural_op` 이 `DomainIntent::RestoreClosedItem` 을 직접 부르고 `CoreEvent::ClosedItemRestored{restored}` 로 성공을 판정한다. 복원된 터미널은 이 함수의 기존 before/after diff 에 잡혀 `added_terminals` → 점유 편입 → tap 을 그대로 탄다(별도 배선 없음).
  - **실행측은 cascade 를 재현하지 않는다**: 로컬 경로가 부르는 `cascade_closed_item_restored` 는 `AppState::active_workspace`/`focused_pane` 을 바꾼다 — forward 경로에서 그것을 부르면 원격 사용자의 조작이 서버 앞 로컬 사용자의 화면을 움직인다(원칙 1·3 위반). forward 실행은 engine 변경만 하고 AppState 를 안 만지며, client focus 는 `PendingOpFocus::NewResource` 가 delta 적용 시점에 client-only 로 보정한다.
- **convert/move-surface**: 둘 다 forward 대상이다. convert(`ConvertSurface`)는 항상 forward. move-surface(`MoveSurface`)는 **source/target 이 같은 mirror workspace 안에 있을 때만** `build_mirror_forward_op` 가 forward 한다 — mirror↔local(또는 서로 다른 mirror) workspace 경계를 넘는 이동은 로컬 전용 surface_id 를 원격에 그대로 보내는 꼴이 되어(u32 네임스페이스 분리가 없어 원격 트리의 무관한 surface 와 우연히 겹칠 위험) 여전히 로컬 차단으로 남는다. `execute_forwarded_structural_op` 의 move-surface 실행은 target(B) 의 PTY cleanup 을 `core::structural_cascade::cascade_surface_closed` 로 명시 재현한다(대응하는 도메인 실행 함수가 없어 이 함수가 직접 호출해야 함 — CloseSurface 가 `structural_exec::close_surface` 안에서 부르는 것과 같은 cascade). convert 실행 성공(`replaced:true`) 시 `ForwardedDelta.converted_surface` 에 대상 surface_id 를 실어, 메인루프(`event_handler.rs`/`boot/headless_stream.rs`)가 `PluginManager::drop_egui_mesh_frame` 을 호출해 egui-mesh(image 등) stale frame 을 방지한다 — 로컬(비-forward) 변환 경로의 `SurfaceConverted` cascade(`app/dispatch_domain.rs`)와 동일 처리를 forward 경로에도 재현한 것. plugin manager 는 forward 실행이 받는 상태 어디에도 없고 두 빌드 모두 호출자가 소유하므로, 결과를 값으로 돌려주고 호출자가 투영한다(ADR-0395 결정 5).

## 서버 로컬(비-holder) 구조 변경 차단

위 절이 다루는 forward 실행(`execute_forwarded_structural_op`)은 hard 점유 **holder** 가 mirror 안에서 만든 정당한 구조 변경이 서버에 도달해 실행되는 경로다. 이 절은 반대 경우 — **서버 자신의 workspace 가 hard-occupied 상태일 때, holder 가 아닌 서버 로컬 IPC/CLI/agent 가 직접** `split`/`tab.create`/`terminal.spawn`/`pane.close`/`tab.close`/`tab.move`/`surface.close`/`markdown.navigate`/`image.open` 를 호출하는 경로를 다룬다. 개념·사용자 영향은 [features/remote-attach "서버(피점유)측 비-holder 구조 변경 차단"](../features/remote-attach/index.md#서버피점유측-비-holder-구조-변경-차단), 여기엔 메커니즘만.

- **왜 `Core::apply` 나 핸들러 함수 내부가 아닌가**: `execute_forwarded_structural_op`(`src/core/attach_runtime.rs`)는 holder 의 forward 실행 시 split/tab.create/tab.close/pane.close/surface.close/tab.move 6종은 IPC 핸들러가 부르는 것과 같은 도메인 실행 함수(`core::structural_exec` 의 `split`/`create_tab`/`close_tab`/`close_pane`/`close_surface`/`move_tab`)를 **직접 함수 호출**하고(IPC 디스패치와 핸들러는 거치지 않는다 — ADR-0395), convert/move-surface 는 대응하는 도메인 실행 함수가 없어 `Core::apply(DomainIntent)` 를 직접 호출한다 — 어느 쪽이든 이 함수 안에서, 또는 그 안에서 공통으로 거치는 `Core::apply` 에 가드를 걸면 holder 본인의 forward 요청까지 함께 막혀 위 "mirror 구조 변경 forward" 기능 전체가 회귀한다. `terminal.spawn` 은 forward 대상 8종(위 6종 + convert/move-surface)에 애초에 포함되지 않으므로([ADR-0060](../adr/0060-block-terminal-spawn-into-hard-occupied-workspace.md)) 이 가드를 추가해도 같은 회귀 위험이 없다.
- **가드 위치**: `hard_occupied_structural_guard`(`src/adapters/ipc/handler.rs`) — `route_engine_handler` 의 method-string dispatch 최상단에서, `execute_forwarded_structural_op` 가 우회하는 그 dispatch 지점에서만 검사한다. params 에서 대상 pane_id/tab_id/surface_id(`split` 은 `target_pane`/`target_surface`, nickname 포함; `markdown.navigate`/`image.open` 은 `surface_id`)를 뽑아 소속 workspace 를 찾고, `OccupancyRegistry::workspace_holder`(`src/core/attach.rs`)가 `Some` 이면 `invalid_params` 로 거부한다("점유 중" + "다른 workspace 사용" 안내). `terminal.spawn` 만 이 가드에 없다 — 대상이 `workspace` 파라미터가 아니라 `pane` 오버라이드까지 반영해 확정된 **최종 pane** 이라, 그것을 아는 `spawn_target_guard`(같은 파일, `handle_spawn` 이 pane 확정 직후 호출)에서 건다. 거부 문구는 `hard_occupied_denial` 로 라우터 가드와 공유한다. 이 이동으로 `--workspace <비점유 ws>` + `--pane <hard-occupied ws 의 pane>` 조합으로 가드를 우회하던 구멍도 닫혔다([ADR-0086](../adr/0086-reject-terminal-spawn-into-mirror-workspace.md)). 대상을 resolve 할 수 없으면(params 누락 등) `None`(가드 미개입) — 원래 핸들러의 통상 검증 에러로 흘려보낸다. `markdown.navigate`/`image.open` 커버는 완전하지 않다 — convert 진입점은 kind 별로 흩어져 있고(host 범용 convert 팝업은 `state.dispatch_intent` 를 직접 호출해 이 IPC 라우팅 자체를 안 탐), 향후 새 kind 가 자기 전용 convert 진입 method 를 추가하면 이 목록에 없는 한 가드가 적용되지 않는다.
- **CLI 는 무수정**: `crates/tasty-ipc/src/client.rs` 의 `IpcConnection` 이 JSON-RPC `error.message` 를 그대로 노출하므로, `tasty new tab`/`tasty split`/`tasty close tab|pane|surface`/`tasty claude spawn`/`tasty codex spawn` 는 서버 에러 메시지를 그대로 보여준다.
- **테스트**: `src/core/attach_runtime.rs` `forward_exec_tests` 모듈 — `dispatch_denies_structural_create_when_hard_occupied`/`dispatch_denies_structural_close_move_when_hard_occupied`(비-holder 일반 IPC 호출 거부 + 트리 불변), `dispatch_denies_terminal_spawn_when_hard_occupied`(`terminal.spawn` 거부 + tab 미생성), `dispatch_denies_terminal_spawn_into_mirror_workspace`(mirror 거부 + forward 큐 미증가), `dispatch_still_forwards_tab_create_in_mirror_workspace`(mirror 의 다른 구조 변경은 여전히 forward — 설계 보존), `dispatch_denies_terminal_spawn_when_pane_override_targets_blocked_workspace`(`--pane` 교차 워크스페이스 우회 차단, mirror·hard-occupied 양쪽), `dispatch_denies_convert_entrypoints_when_hard_occupied`(`markdown.navigate`/`image.open` 거부), `dispatch_allows_tab_create_when_not_occupied`/`dispatch_allows_markdown_navigate_when_not_occupied`(비점유 workspace 오탐 방지), `forward_*_succeeds_when_hard_occupied` 계열(holder 의 forward 는 hard-occupied 여도 정상 성공 — 회귀 방지, convert/move-surface 포함). move-surface 의 same-mirror-workspace 검증은 `src/core/impl_mirror.rs` `mirror_structural_guard_tests` 모듈의 `mirror_move_surface_enqueues_forward_when_same_workspace`/`mirror_move_surface_blocked_when_crossing_workspace_boundary` 가 커버한다.
- **극단 케이스(마지막 tab/pane close)는 이 가드로 도달 불가능해짐**: close/이동 계열을 이 가드에 넣기 전 남아 있던 의문은 "비-holder 가 hard-occupied workspace 의 마지막 tab/pane 을 서버 로컬에서 직접 닫으면 `workspaces_now_empty` → auto-recreate 경로가 발동해, 원래 `attach.workspace_locks` 가 가리키던 workspace_id 가 트리에서 사라지고 holder 는 더 이상 존재하지 않는 workspace_id 를 계속 점유 중인 것으로 남는 게 아닌지" 였다. `hard_occupied_structural_guard` 가 `pane.close`/`tab.close`/`surface.close` 를 **dispatch 레벨에서 대상 workspace 가 hard-occupied 인 한 통째로 차단**하므로, 애초에 그 요청이 `Core::apply`/cascade 근처에 도달하지 못한다 — 마지막 tab/pane 인지 아닌지와 무관하게 비-holder 요청은 전부 거부되고 트리는 항상 불변이라, 이 극단 케이스 자체가 이 가드 도입 이후로는 발생할 수 없는 경로가 됐다. `dispatch_denies_structural_close_move_when_hard_occupied` 테스트가 단일 pane·단일 tab fixture(=정확히 "마지막 tab/pane" 상황)로 이를 검증한다.
- **`terminal.spawn` 은 이제 이 가드 대상이다 — 과거 서술(생성 경로는 예외) 정정**: 이전 판에선 "새 리소스 추가는 점유 holder 가 보고 있는 화면을 흔들지 않으므로 차단 대상이 아니다"로, `tasty claude/codex spawn` 이 실제로 타는 경로를 `pty.attach_surface`(`AdoptTerminal`)로 서술했다. 코드 재확인 결과 그 경로는 실제로는 **`terminal.spawn`**(→ `tab::handle_tab_create` → `apply_create_tab`, `src/adapters/ipc/handler/terminal.rs::handle_spawn`)이었다 — `pty.attach_surface` 서술은 오기였다. 그리고 "생성 경로는 홀더의 화면을 안 흔드니 예외"라는 논거는 **holder 관점만** 다뤘을 뿐, spawn 을 호출한 로컬 agent 자신이 그 직후 자기 결과물(새 surface)에 입력을 못 넣게 되는 부작용은 검토하지 않았다(`send_text_to_surface_with_ack` 가 hard-occupied surface 에 `None` 반환 → `"Surface {id} not found"` 로 뭉뚱그려짐, `send_text_to_surface_with_ack` 의 doc). 이 사각지대를 근거로 [ADR-0060](../adr/0060-block-terminal-spawn-into-hard-occupied-workspace.md) 이 정책을 뒤집어 `terminal.spawn` 을 차단 대상 7종에 포함시켰다.
- **mirror(원격 attach client) 워크스페이스로의 `terminal.spawn` 도 같은 자리에서 거부된다**: 이 절의 나머지는 **서버(피점유)측** 축이지만, `spawn_target_guard` 는 client 측 축인 mirror 도 함께 판정한다 — 같은 "최종 pane 이 어느 워크스페이스에 속하는가" 조회 하나로 둘 다 결정되기 때문이다. mirror 워크스페이스의 구조 변경은 원격으로 forward 되고 그 응답은 fire-and-forget 이라 생성된 surface id 를 담지 않는데, `handle_spawn` 은 그 id 를 동기로 꺼내 child registry 등록·soft 점유·command 주입까지 이어가야 한다. 막지 않으면 핸들러는 에러를 반환하지만 `pending_structural_forward` 는 IPC 응답과 무관하게 `about_to_wait` 에서 드레인되므로 **원격에만 탭이 남는 고아**가 생긴다. mirror 판정은 `terminal.spawn` 에만 적용한다 — 라우터 가드의 나머지 8종에 넣으면 mirror 구조 변경 forward 전체가 회귀한다. 근거 → [ADR-0086](../adr/0086-reject-terminal-spawn-into-mirror-workspace.md).
- **`pty.attach_surface` 는 여전히 이 가드 대상이 아니다(알려진 갭)**: `AdoptTerminal` intent 로 실행되는 이 경로도 아래와 동일한 `apply_adopt_terminal` → `tap_new_workspace_member` 후처리를 타 이론상 `terminal.spawn` 과 동일한 부작용(호출자 본인의 입력 차단)을 가질 수 있으나, 이번 변경의 확인된 필수 스코프는 `terminal.spawn` 로 한정했다 — 소비자(`tasty terminal` child-terminal, soft 점유 기반)에 대한 영향 검토가 아직 없다(재검토 조건은 ADR-0060 참고).
- **점유 상속(tap 편입)은 차단을 통과한 생성 경로에 계속 적용된다**: `apply_create_tab`(`src/core/impl_tab.rs`)/`apply_split_pane`/`apply_split_surface`(`src/core/impl_split.rs`)/`apply_adopt_terminal`(`src/core/impl_attach.rs`)로 hard-occupied workspace 에 새로 생긴 터미널 surface 는, forward-op 경로(`execute_forwarded_structural_op`)와 마찬가지로 그 점유를 상속하고 스트림 tap 을 받아야 한다 — 안 그러면 PTY/화면버퍼는 정상인데 attach client 로의 스트리밍만 안 걸려 그 tab 이 검정 화면으로만 보인다. `CoreState::tap_new_workspace_member`(`src/core/attach_runtime.rs`)가 위 4개 생성 경로 공통 후처리로 `OccupancyRegistry::add_workspace_member` + (holder 에게 새 트리를 담은 `StructuralDelta` — 위 "forward 가 아닌 원인의 구조 변경") + `tap_surface_for_stream` 을 이 순서로 실행한다. `hub`/`client_id` 를 `Core::apply` 호출 체인까지 파라미터로 꿰는 대신, `OccupancyRegistry::notifier()`(boot 시 주입된 `StreamHub` clone — `force_detach_workspace` 의 `notify_detached` 와 동일 패턴)와 `workspace_holder()` 로 이미 점유 중인 workspace 의 holder client_id 를 그 자리에서 조회해 재사용한다. 이 경로는 (홀더 본인의 forward 로 생긴 surface 등) 가드를 통과한 경우에만 도달한다 — `terminal.spawn` 은 이제 애초에 가드에서 걸러지므로 비-holder 로부터는 이 지점에 도달하지 않는다. 테스트: `src/core/impl_mirror.rs` `mirror_structural_guard_tests` 의 `*_in_occupied_workspace_inherits_occupancy` 4종(단위, occupancy 등록만 검증) + `tests/attach_local_creation_tap.rs`(loopback 통합, 실제 mux `Data` 프레임 수신까지 검증 — `pty.spawn`+`pty.attach_surface` 로 실제 프로덕션 repro 경로 재현).
- **forward-op 경로는 이 즉시-tap 을 스킵한다(이중 tap 방지)**: 위 4개 생성 경로는 `execute_forwarded_structural_op` 가 재사용하는 `handle_split`/`handle_tab_create` 에서도 그대로 호출된다 — 즉 holder 자신의 forward 실행 중에도 (가드를 통과한 뒤) `tap_new_workspace_member` 가 타지만, 그 경로는 위 §"역반영" 이 이미 delta 전송 **후** `tap_surface_for_stream` 을 직접 걸므로 여기서 또 tap 하면 같은 surface 에 tap 이 2개 등록돼 매 PTY 청크(타이핑 echo 포함)가 client 에 2번 도착한다("tttttest" 증상). `OccupancyRegistry::set_auto_tap_suppressed(true/false)` 로 `execute_forwarded_structural_op` 가 `pane::handle_split`/`tab::handle_tab_create` 호출 구간만 감싸 이 즉시-tap 을 억제하고(멤버 편입은 그대로 진행), 로컬 생성 경로(플래그 항상 `false` — `terminal.spawn`/`pty.attach_surface` 포함, 가드를 통과했든 안 했든 무관)는 영향받지 않는다. 테스트: `forward_exec_tests::forward_split_surface_taps_exactly_once_with_real_stream_hub`(실제 `StreamHub` 주입, tap 개수 검증 — `Terminal::output_tap_count`).

## SSH 터널 (원격 client 공통)

`crates/tasty-ssh/src/lib.rs` — `remote attach` 와 `remote check` 가 공유.

- **시스템 ssh 위임**: 자체 암호화 없이 시스템 `ssh` 를 자식 프로세스로 실행. 사용자 `~/.ssh/config`·agent·known_hosts 재사용. **Windows 는 시스템 OpenSSH 풀경로**(`%WINDIR%\System32\OpenSSH\ssh.exe`) 우선 — git 번들 ssh 는 윈도우 ssh-agent(named pipe)를 못 봐 무암호 인증 실패.
- **원격 포트 발견**: 기본 `auto` = subcommand → file-unix → file-windows 순서로 원격 DefaultShell 4종(PowerShell/cmd/git bash/unix) 커버. `--remote-port-mode` 로 고정, `--remote-tasty <path>` 로 원격 바이너리 경로(기본 `tasty`).
- **포트 발견 상한(no-hang)**: 포트 발견/셸 감지는 무기한 블록하지 않는다 — ssh `-o ConnectTimeout`(`SSH_CONNECT_TIMEOUT` 10초, 연결 수립) + ssh 자식 1개당 프로세스 레벨 감시(`PORT_DISCOVERY_STEP_TIMEOUT` 20초, kill + wait 로 좀비 없이 회수) + 호출 1회 전체 예산(`PORT_DISCOVERY_TOTAL_TIMEOUT` 45초, 체인 단계 수만큼 곱해지는 것을 차단) 3겹. 상수는 `ssh.rs` 에 `pub const`. 프로필 `extra_options` 의 `ConnectTimeout` 이 기본값을 이긴다(ssh(1) first-wins → 기본값을 뒤에 push). 근거 → [ADR-0070](../adr/0070-port-discovery-timeout.md), 상세 → [features/remote-attach "연결 시도 상한"](../features/remote-attach/index.md#연결-시도-상한-no-hang).
- **포트 발견 자식 ssh 의 조기 취소**: 위 상한은 **시간**이 끊는 것이고, 그와 별개로 **사용자 의도**로 끊는 경로가 있다. 상한 감시를 위해 `run_capture_with_budget` 이 이미 `spawn` + `try_wait` 로 자식 핸들을 쥐고 있으므로, 그 핸들을 **취소 스코프**(`ssh::SshCancel` — 스레드로컬 설치, `SshCancel::scope()`)에도 맡긴다. 조회 워커 스레드가 스코프를 설치해두면 다른 스레드에서 `cancel()` 로 그 자식을 상한 만료 **전에** kill + wait(좀비 방지, `SshTunnel::drop`·타임아웃 경로와 같은 계약) 할 수 있고, 취소 후에는 auto 체인의 남은 단계도 ssh 를 띄우지 않는다(`PortDiscoveryFailureKind::Cancelled` 는 예산 소진과 마찬가지로 fallback 대상이 아니다). 현재 이 스코프를 설치하는 것은 RA02 GUI picker 워커뿐이다 — `remote.workspaces`/`remote.attach` IPC 와 `auto_attach` 워커는 취소를 표현할 사용자 표면이 없고(요청자가 끊어도 알릴 채널이 없다), 도구 메뉴 > Remote connections 의 셸 감지 워커(`remote_tool.rs`/`remote_profile.rs` 의 `spawn_detect`)는 같은 구멍을 공유하지만 별도 상태 머신이라 함께 손대지 않았다. 어느 쪽이든 필요해지면 워커 진입부에 같은 스코프를 설치하고 취소 신호만 연결하면 된다.
- **포트 발견 실패 진단**: 전 단계 실패 시 `PortDiscoveryError`(`ssh.rs`)가 exit code 기반(로케일 무관)으로 SSH 연결 실패 / 원격 인스턴스 미실행 / 포트 파싱 실패 + 위 상한 초과 시 타임아웃 + 사용자 취소, 5분류한다 — 원격 raw stderr 는 `Display` 에 노출하지 않고 `tracing::debug!` 로만 남긴다(타임아웃·취소 경로도 동일). auto 체인이 전 단계 실패하면 가장 확정적인 분류를 대표로 고른다(`pick_most_informative` — 취소 > 타임아웃 순 우선. 취소는 사용자가 끊었다는 확정 사실이라 시간·연결 실패로 보고하면 오도한다). 상세 → [features/remote-attach "원격 포트 발견 실패 진단"](../features/remote-attach/index.md#원격-포트-발견-실패-진단).
- **터널 생명주기**: detach/종료 시 자식 ssh kill(고아 터널 방지)하되 **원격 데몬은 생존**(server-owns-PTY persistence = detach 의 본질). 자동 재연결(attach 한정): 지수 백오프(0.5s→30s)로 터널+attach 재수립(`--no-reconnect` 로 끔) — 이 재연결은 `run_attach_on_port` 가 반환하는 `AttachExit::Disconnected` 를 전제로 한다. mirror-dump/workspace-mirror-dump 모드는 처음부터 reader 를 별도 스레드 + `mpsc::channel` 로 분리해 `rx.recv_timeout` 의 `RecvTimeoutError::Disconnected`(끊김) vs `Timeout`(정상 deadline) 을 구분해 이를 반환해왔다. `--raw` 모드(`run_raw_bridge`)도 동일 계약을 만족한다 — server reader 스레드가 `mpsc` 채널에 보내고(`RawEvent::Server`/`ServerRecvErr`), main 은 `rx.recv()`(deadline 없이 블로킹)만 기다린다. server reader 의 `conn.recv()` 가 `Err` 면 `RawEvent::ServerRecvErr` 를 명시적으로 보내 `AttachExit::Disconnected` 로 이어진다 — 과거엔 이 reader 스레드가 종료 사유와 무관하게 `std::process::exit(0)` 을 호출해 이 반환 경로 자체에 절대 도달하지 못했다(재연결 완전 불능이던 결함, 해소됨).
- **stdin 라우팅(단일 영속 리더 + 슬롯 교체)**: stdin 은 `run_raw_bridge` 가 직접 스레드를 스폰하지 않는다 — 프로세스 생애주기 동안 [`spawn_stdin_reader`](../../crates/tasty-cli/src/local/attach.rs) 가 **1 회만** 스폰한 리더 스레드가 `Arc<Mutex<Option<mpsc::Sender<RawEvent>>>>`(`StdinSlot`) 를 들고 blocking `read()` 결과를 매번 그 슬롯의 현재 sender 로 라우팅한다. `run_raw_bridge` 는 재연결마다 `install_sender` 로 자기 세션의 sender 를 그 슬롯에 설치(교체)할 뿐이다. 과거엔 재연결마다 새 stdin 스레드를 스폰해, 이전(좀비) 스레드가 blocking read 에 갇힌 채 새 스레드와 전역 `std::io::Stdin`(내부 `Mutex<BufReader<..>>`) 을 두고 경쟁했다 — 좀비가 먼저 읽어간 바이트는 자신의 죽은 채널로 송신을 시도하다 실패해 그대로 유실됐고, 재연결 횟수만큼 좀비가 누적돼 "프로세스 생애주기 내내 언제든 입력을 훔칠 수 있는" 비결정적 결함이었다. **지금은 stdin 을 읽는 스레드가 항상 정확히 1 개이므로 이 경쟁 자체가 구조적으로 불가능하다.**
- **남는 유실 창(정확한 범위)**: 세션 전환의 아주 짧은 순간 — 이전 세션이 끝나 슬롯이 아직 `None` 이거나 이미 rx 가 drop 된 죽은 sender 를 가리키는 동안 사용자가 타이핑하면, 리더 스레드는 그 청크의 송신에 실패한다. 이때 리더 스레드는 (좀비가 되지 않도록) **종료하지 않고 그 청크만 버린 뒤 계속 읽는다** — 슬롯 자체는 절대 건드리지 않는다(ABA 경쟁 방지 불변식: 슬롯에 대한 쓰기는 `install_sender` 만 수행). 이 창은 "재연결이라는 명확한 이벤트 근방"으로 국한되며, 좀비 누적처럼 시간이 지날수록 커지지 않는다. 진짜 stdin EOF/에러가 이 창(슬롯이 비어있는 동안)에 겹치면 별도 `AtomicBool` latch 에 기억해뒀다가, 다음 세션이 `install_sender` 로 sender 를 설치하는 시점에 즉시 `RawEvent::StdinEof` 를 전달한다(그러지 않으면 EOF 통지가 영영 유실돼 다음 세션이 이미 닫힌 stdin 을 무한정 기다리게 될 수 있다). 이 창을 버퍼링으로 완전히 닫는 것은 이번 스코프 밖이다(선택적 후속 확장).
- **loopback 직결**: 인라인 host 가 `127.0.0.1:PORT`/`localhost:PORT` 면 SSH 없이 직접 attach(동일 머신 다중 인스턴스 검증).

## 연결 생존 확인 (read timeout + heartbeat)

attach 스트림은 read timeout 이 없는 순수 blocking I/O 라 네트워크가 FIN/RST 없이 **조용히** 끊기면(케이블 단절·NAT 타임아웃 등) 소켓의 read 가 영원히 대기해 어느 쪽도 끊김을 감지하지 못한다. 아래는 이를 감지하는 배선이다 — 상수는 `crates/tasty-ipc/src/stream.rs` 의 `HEARTBEAT_INTERVAL`(5초)/`HEARTBEAT_TIMEOUT`(20초, interval 의 4배 — 일시적 jitter 로 인한 오탐 방지).

- **read timeout**: 서버(`tcp_ipc_server.rs::handle_stream_connection`)·GUI client(`attach_client.rs::start_gui_attach`)·CLI client(`tasty-ipc/src/client/stream.rs::StreamConnection::open_with`) 3곳 모두 소켓에 `HEARTBEAT_TIMEOUT` read timeout 을 건다. **서버는 그것을 attach dispatch 보다 먼저 건다**(`arm_stream_read_timeout` 이 `validate_stream_proto` 앞이다) — 즉 판정은 **연결 단위**이고 점유 유무와 무관하다: 아무 workspace 도 점유하지 않은 단순 upgrade 연결도 20초 침묵하면 닫힌다. 그래서 client 는 점유를 잡았든 안 잡았든 Ping 을 보내야 한다. `try_clone` 된 reader/writer 는 같은 소켓 옵션을 공유하므로 한 번만 걸면 양쪽 다 적용된다. CLI 는 핸드셰이크 ack/`attached(_workspace)` 디스크립터 대기(둘 다 `recv()`)에도 자동으로 적용된다.
- **`StreamTag::Ping`**: 빈 payload 의 keepalive 프레임(값 `2`, 코덱은 이전부터 예약돼 있었다). 수신측은 아무 처리도 하지 않는다 — `read_frame` 호출이 성공적으로 리턴하는 것 자체가 read timeout 을 리셋하므로, Ping 이든 실제 Data/Control 이든 도착만 하면 liveness 로 인정된다.
- **송신은 write 쪽이 idle 할 때만**: 서버의 write thread(`handle_stream_connection`)는 기존 `for frame in sink_rx` blocking iterator 대신 `sink_rx.recv_timeout(HEARTBEAT_INTERVAL)` 루프를 쓴다 — sink 에 실제 Data/Control 프레임이 흐르면 그게 곧 liveness 라 Ping 을 보내지 않고, `HEARTBEAT_INTERVAL` 동안 조용하면 빈 Ping 을 대신 흘린다. GUI/CLI(raw 브리지) client 는 각각 별도 heartbeat 스레드가 `HEARTBEAT_INTERVAL` 마다 무조건 Ping 을 보낸다(활성 트래픽 여부를 별도로 추적하지 않음 — 5바이트 프레임 오버헤드가 그 추적 비용보다 훨씬 싸다).
- **CLI dump 모드(mirror-dump/workspace-mirror-dump)는 client 발 heartbeat 이 없다**: `--dump-after` 기본 500ms 로 짧게 끝나 `HEARTBEAT_TIMEOUT`(20초) 안에서 항상 종료되고, 서버가 보내는 Ping 만으로 client 쪽 read timeout 은 충분히 방지된다. raw 브리지(대화형, 장시간 idle 가능)만 client 쪽 heartbeat 스레드를 띄운다.
- **양방향이 필요한 이유**: 서버 write thread 의 Ping 은 client 의 read timeout 을(idle mirror 뷰), client 의 Ping 은 서버의 read timeout 을(client 가 오래 아무 입력도 안 보내는 세션) 각각 갱신한다 — 한쪽만 보내면 반대 방향의 read loop 가 오탐 disconnect 된다.
- **timeout 만료 → 기존 disconnect 경로 재사용**: read timeout 으로 인한 `WouldBlock`/`TimedOut` io 에러는 서버의 `Err(_) => break`(`handle_stream_connection`)·GUI client 의 `Err(_) => disconnected.store(true, ...)`·CLI 의 채널 기반 끊김 신호(`run_mirror_dump`/`run_workspace_mirror_dump` 의 `mpsc::RecvTimeoutError::Disconnected`, `run_raw_bridge` 의 `RawEvent::ServerRecvErr` — 위 "SSH 터널" 절 참고)를 그대로 타 EOF 와 동일하게 처리된다 — 별도 sweep 스레드나 새 상태 없이, 아래 "mirror 세션 종료"·[`features/remote-attach`](../features/remote-attach/index.md) 의 "자동 해제" 가 조용한 네트워크 단절까지 커버하게 된다.
- **GUI heartbeat 스레드 정리**: `cleanup_mirror_workspace` 가 세션 종료 시(원격발이든 사용자 close 든) `sess.disconnected` 를 set 해, `writer: Arc<Mutex<TcpStream>>` 를 계속 붙들고 있는 heartbeat 스레드가 다음 tick 에 스스로 종료하게 한다 — 안 하면 세션이 정리된 뒤에도 소켓/스레드가 새는 leak.
- **버전 skew 리스크(heartbeat 축은 완화 로직 없음, 의도적)**: read timeout(20초)이 걸린 이후부터 이 프로토콜을 도입한 버전이 적용된다. 구버전 프로세스(Ping 미전송)와 신버전 프로세스가 같이 떠 있는 상태(예: 호스트 재시작 없이 CLI/플러그인만 재배포)에서는 idle 상태의 mirror 세션이 20초 뒤 오탐 disconnect 될 수 있다 — 이 프로젝트는 단일 사용자 로컬 앱이라 그런 skew 창이 짧고 드물어 허용 가능하다고 판단했다. 다중 버전 동시운영이 필요해지면 capability 협상을 재검토한다. **단 스트림 프로토콜 버전(`proto`) 자체는 핸드셰이크에서 검증된다** — 위 "점유 레지스트리" 의 `validate_stream_proto` 항목 참고. 지금은 정확히 같은 값만 통과하는 엄격 매칭이라, `STREAM_PROTO` 를 올리면 구버전 client 는 조용한 실패가 아니라 명시적 거절을 받는다.

## 클라이언트측 비대칭("서버는 점유 유지, 클라이언트는 사라짐") 원인

read timeout 프로토콜(위 절) 반영 *이전에도* "네트워크가 불안정해지면 클라이언트 mirror
workspace 는 사라지는데 서버는 여전히 점유 상태로 남는" 비대칭이 관찰됐다. 서버측 무한 블록
문제는 서버가 자기 소켓에 read timeout 을 걸어 스스로 해소한다(위 절). 아래는 **클라이언트가
왜 서버보다 먼저 사라지고 있었는가**의 실측 확인 결과다.

- **원인**: SSH 원격 attach(`crates/tasty-ssh/src/lib.rs::push_common_opts`)는 최초 SSH 터널
  구현부터 `ServerAliveInterval=15` + `ServerAliveCountMax=3` 를 걸어왔다. 이는 **로컬 ssh
  자식 프로세스 자신의 keepalive** 다 — 원격 sshd 가 15초
  간격 keepalive 요청에 3회(최대 45초) 응답하지 않으면 로컬 ssh 프로세스가 스스로 종료한다.
  ssh 가 죽으면 그 프로세스가 열어둔 로컬 loopback 포트(`ssh -L` 의 local 소켓)도 함께 닫혀
  GUI/CLI client 의 `read`가 **EOF** 를 받는다 — 이건 이미 있던 "원격발 종료" 경로(위 "연결
  생존 확인" 참조)를 그대로 타므로 read timeout 프로토콜 없이도 동작했다. 원격 tasty 서버
  자신의 소켓은 이 keepalive 와 무관하게 열린 채로 남으므로(서버는 SSH 를 전혀 모른다 — 항상
  loopback 만 본다) 무한 블록됐다 — 이게 서버/클라이언트 비대칭의 실체다.
- **read timeout 프로토콜 반영 후의 위치**: 이 SSH self-kill 경로는 여전히 유효하지만,
  이제는 **두 독립 메커니즘 중 하나**일 뿐이다 — SSH 터널이 죽지 않고 데이터만 조용히
  막히는 경우(예: 같은 머신 loopback attach, 또는 SSH 는 살아있지만 tasty 프로세스 자체가
  멎은 경우)에도 `HEARTBEAT_TIMEOUT`(20초) 가 독자적으로 클라이언트를 깨운다. 즉 SSH
  self-kill 은 클라이언트 정리를 **더 빠르게**(최대 45초) 만들 뿐, read timeout 프로토콜의
  전제 조건이 아니다.
- **실측 검증(2026-07-20)**: `ps`/`kill` 로 실제 SSH 프로세스 종료를 관찰하는 대신(이 sandbox
  는 self-loopback SSH 인증 수단이 없어 재현 불가), 동일한 근본 시나리오("피어가 조용히
  응답을 멈춤")를 `SIGSTOP`으로 직접 재현했다. 격리된 debug 인스턴스(`--port-file` 커스텀)를
  loopback 으로 띄우고 `tasty debug attach
  <surface> --dump-after 30000`로 attach, 3초 뒤 서버 프로세스에 `SIGSTOP` → 클라이언트가
  `--dump-after` 로 요청한 30초를 다 기다리지 않고 **~21초**(≈`HEARTBEAT_TIMEOUT`)만에
  스스로 종료함을 확인(`AttachExit::Disconnected` 경로). `SIGCONT` 로 서버를 살려도 이미
  종료한 클라이언트는 영향 없음 — 정상 동작.
- **회귀 확인**: 서버가 건강한 상태에서 `HEARTBEAT_TIMEOUT` 보다 오래(raw 브리지, idle 25초+)
  열어둔 세션은 끊기지 않고 유지됨을 같은 방식으로 확인(client 발 heartbeat 스레드가
  서버측 read timeout 을 계속 갱신). 단, CLI **mirror-dump 모드**(`--raw` 미사용)는
  client 발 heartbeat 이 없어 `--dump-after` 를 `HEARTBEAT_TIMEOUT`(20초) 이상으로 주면
  **서버가 먼저** client 를 idle 로 보고 끊는다(서버도 20초 read timeout 을 걸기 때문) —
  이건 버그가 아니라 문서화된 설계(위 "연결 생존 확인"의 "CLI dump 모드는 client 발
  heartbeat 이 없다" 참조): 기본값 500ms 로 짧게 끝나는 검증 전용 모드라 20초를 넘기는
  사용은 애초에 지원 대상이 아니다.

## GUI 자동 재연결 스코프

silent disconnect 정리(`cleanup_mirror_workspace`) 자체는 heartbeat TTL 만료로도 자동 발동한다
(위 "연결 생존 확인"). anchor(매핑된 로컬 워크스페이스)에 물린 세션이 이렇게 끊기면
mirror workspace 는 즉시 걷히지 않는다 — 세션이 `SessionState::Reconnecting` 으로 전이해
"limbo" 상태로 살아남고, 아래 backoff 스케줄이 자동으로 재연결을 시도한다.
anchor 가 없는 mirror(IPC `remote.attach` 로 연 임시 mirror 등)는 대상이 아니라 기존처럼
즉시 정리된다.

- **레벨/엣지 트리거(기존, 신규 attach 전담)** — `src/app/auto_attach.rs::maybe_trigger_auto_attach`
  는 "활성 워크스페이스가 매핑 Some & `auto_attach_active` 에 없으면 트리거"를 **매 프레임**
  재평가한다. 예를 들어 이미 활성인 워크스페이스에 `attach_mapping` 을 방금 새로 설정하면
  (`tasty set workspace --ssh-profile ...`) 워크스페이스 전환 없이도 다음 프레임에 즉시
  트리거된다. "재진입 대기(pending reactivation)" anchor(과거엔 disconnect 로 정리된
  모든 anchor 가 여기 들어갔지만, 지금은 이 표시가 남아있는 것 자체는 드물다 — 아래
  backoff 스케줄이 대부분 먼저 처리한다)만 엣지(전환) 게이팅: 활성 워크스페이스 id 가
  **직전 프레임과 달라진 프레임에서만**(`App.auto_attach_last_active_ws` 로 직전 값을 들고
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
  아니어도 `App.auto_attach_reconnect: HashMap<u32, ReconnectSlot>` 에 저장된 `next_attempt`
  시각이 지났으면(Reconnecting 진입 직후 첫 시도는 슬롯이 없어 즉시). 두 조건 모두
  `auto_attach_active` 게이트를 공유해 중복 attach 를 막는다. anchor 워크스페이스 자체가
  삭제됐거나 `attach_mapping` 이 그 사이 사라지면 스케줄을 정리하고 skip 한다.
  - **non-blocking 스케줄링**: `tasty_ssh::Backoff::sleep()`(CLI `--ssh` 재연결 루프가
    쓰는 blocking API)을 GUI 메인 스레드에서 그대로 쓸 수 없어, `current()`/`advance()`
    (peek/advance-only, non-blocking)를 새로 추가해 "다음 시도 시각"만 계산해 두고 매
    프레임 `Instant::now()` 와 비교한다.
  - **성공/실패 처리(`drain_auto_attach_results`)**: `is_reconnect` 플래그로 신규
    attach(`start_gui_attach`)와 재연결(`reconnect_session`, survivor mapping — 아래
    "재연결 시 세션 상태 보존" 참고)을 분기한다. 재연결 성공 시 `auto_attach_reconnect`
    슬롯을 제거(다음 disconnect 때 새로 시작)한다. 실패는 `on_reconnect_attempt_failed`
    로 처리: 에러 메시지에 `already_attached` 가 포함되면(다른 클라이언트가 그 원격
    워크스페이스를 여전히 점유 — 영구적 충돌일 수 있음) 지수 증가 없이 max(30초) 간격에
    고정해 폭주 없이 계속 대기하고, 그 외(네트워크/SSH 등 일시적 실패)는 통상적인 지수
    백오프(0.5s→30s)로 늘린다. 간격에는 ±20% jitter 를 둬 여러 anchor 가 동시에 끊겼을
    때 재시도가 한 tick 에 몰리는 것을 막는다. 시도 횟수가 `MAX_RECONNECT_ATTEMPTS`(20)에
    도달하면 `ReconnectSlot.given_up` 을 세우고 안내 toast(`mirror_reconnect_giveup`)를
    1회 띄운다 — `auto_attach_pending_reactivation` 은 건드리지 않으므로 사용자가 그
    워크스페이스를 왕복하는 수동 재시도(엣지 트리거)는 계속 유효하다.
    - **give-up 은 슬롯 제거가 아니라 플래그다(버그수정, Gate4)**: give-up 시 슬롯을
      지워 부재(`None`)로 표현하면, `reconnect_due` 가 이를 "아직 한 번도 실패하지
      않음"과 구분하지 못해 바로 다음 프레임에 자동 재시도가 재개돼버린다 — 실패하면
      시도 횟수가 0부터 다시 쌓이고 상한마다 give-up→즉시재개를 무한 반복해
      `MAX_RECONNECT_ATTEMPTS` 자체가 무의미해지는 회귀였다. `given_up: bool` 이 세워진
      슬롯은 `next_attempt` 시각과 무관하게 `reconnect_due` 가 항상 `false` 를 반환해
      자동 재시도 대상에서 계속 제외되고, 사용자가 그 워크스페이스로 돌아오는 edge
      트리거(`should_reset_given_up`)만이 슬롯을 리셋해 재개를 허용한다. 실패 횟수
      비교도 `attempts >= MAX_RECONNECT_ATTEMPTS`(`reconnect_exhausted`)로 — `attempts`
      는 이번 실패를 반영해 1부터 증가시킨 값이라 `>` 를 쓰면 상한보다 한 번 더
      실패해야(21번째) give-up 하는 off-by-one 이었다.
- **`detach_orphaned_mirror_sessions` 와의 관계**: 간섭 없음. `apply_attach_client_output`
  (disconnect 감지, anchor 있으면 `Reconnecting` 전이 / anchor 없으면
  `cleanup_mirror_workspace(sess, from_disconnect=true)`)과
  `detach_orphaned_mirror_sessions`(사용자가 mirror ws 자체를 닫은 고아 세션 정리,
  `from_disconnect=false`)는 서로 다른 세션 식별 기준(전자는 `disconnected` 플래그, 후자는
  `local_workspace` 를 들고 있는 engine 이 창 있는 쪽·parked 쪽 어디에도 없음)으로 동작하고, 둘 다 단일 스레드 메인루프에서
  순차 실행돼(레이스 없음) 겹치는 세션이 있어도 먼저 처리한 쪽이 세션을 vec 에서 제거해
  뒤쪽은 자연히 skip 한다. `from_disconnect` 플래그가 두 경로를 구분해 사용자가 mirror ws 를
  직접 닫은 경우는 `auto_attach_pending_reactivation`/`auto_attach_reconnect` 에 들어가지
  않는다(재진입 대기·재연결 스케줄 의미가 없음 — 사용자 스스로 걷어낸 것).

**서버측 점유 해제([ADR-0052](../adr/0052-attach-heartbeat-ttl-hard-occupancy-release.md))와는 독립이다**: 위 backoff 재연결은 순수 **client(GUI mirror)측** 복원력이고, 서버의 `OccupancyRegistry` 해제 타이밍(EOF-or-TTL, 위 "점유 레지스트리" 절)은 전혀 건드리지 않는다 — TTL 만료로 서버가 이미 lock 을 free 로 되돌린 뒤에도 client 는 그저 다시 처음부터 attach 를 재시도할 뿐이고, 재연결 도중 다른 client 가 그 lock 을 먼저 잡으면 `already_attached` 로 거부돼 위 영구 충돌 backoff 로 흡수된다. 서버가 원 client 를 위해 lock 을 더 오래 붙들어주는 "재연결 유예 창구"는 도입하지 않았으므로 ADR-0052 의 Reconsideration Trigger("TTL 기반 해제와 다른 규칙 — 재연결 유예 창구가 필요해질 때")는 충족되지 않는다.

## 재연결 시 세션 상태 보존

backoff 재연결이 매번 `start_gui_attach` 로 완전 신규 mirror workspace/터미널을
만들면 scrollback 과 local id 가 재연결마다 사라진다. 이를 막기 위해 `reconnect_session`
(`src/app/attach_client.rs`)은 `resolve_endpoint`/handshake 는 `start_gui_attach` 와 동일하게
수행하되, 워크스페이스/터미널은 **기존 것을 그대로 재사용**한다:

- **`merge_survivor_mapping`**: 기존 `apply_mirror_structural_delta` 의 survivor-mapping
  로직(옛 remote_id→local_id 매핑과 새 원격 구조를 비교해 양쪽에 다 있는 surface 는
  local id/Terminal 인스턴스를 그대로 재사용)을 공용 함수로 추출해, `start_gui_attach`(빈
  old_map)·`apply_mirror_structural_delta`·`reconnect_session` 세 곳이 공유한다.
  `reconnect_session` 은 재연결 직전의 `remote_to_local` 매핑을 old_map 으로 넘겨 재연결
  전후 살아있는 surface 의 scrollback/local id 를 보존한다.
- **`SharedFrameSender`(`Arc<Mutex<FrameSender>>`)**: 입력 forwarder 스레드(`make_mirror_surface`
  가 만드는, surface 입력을 원격에 쓰는 장기 생존 스레드)는 연결이 바뀌어도(재연결로
  reader/writer/heartbeat 스레드와 소켓 자체가 통째로 교체된다) 살아있는 채로 새 연결의
  sender 를 봐야 한다. 그래서 `frame_tx` 를 값이 아니라 `Arc<Mutex<>>` 로 감싸 forwarder
  스레드에 공유하고, `reconnect_session` 은 이 Arc 는 그대로 두고 **내부의 raw sender만
  교체**한다. forwarder 스레드는 send 실패 시에도 (구 채널이 재연결 중 잠깐 죽어있는
  것일 수 있으므로) 스레드를 종료하지 않고 그 청크만 drop 하고 계속 루프한다 — Codex
  크로스체크가 지적한 "재연결 후 forwarder 가 죽은 채널을 계속 참조해 survivor 터미널
  입력이 조용히 유실되는" 결함의 수정.
- **`SessionState`(`Connected`/`Reconnecting`)**: 기존 `cleanup_mirror_workspace` 는
  disconnect 즉시 mirror workspace/터미널을 통째로 걷어내, 재연결 트리거가
  발동할 시점엔 survivor-mapping 을 적용할 대상 자체가 남아있지 않았다(Codex 크로스체크
  지적). 이를 막기 위해 anchor 가 있는 세션의 disconnect 는 `cleanup_mirror_workspace`
  를 부르지 않고 `enter_reconnecting`(mirror workspace/터미널을 그대로 둔 채 상태만
  `Reconnecting` 으로 전이, `auto_attach_active` 에서 제거해 재연결 트리거가 자유롭게
  spawn 하게 함)으로 분기한다. anchor 가 없는 세션(임시 mirror)은 기존처럼 즉시
  `cleanup_mirror_workspace`.
- **레이스**: 재연결 워커가 엔드포인트 해석(`resolve_endpoint` — 프로필/포트 발견/SSH
  터널 수립)을 끝내고 결과를 메인 루프로 보내기 전에, 사용자가 그 mirror workspace 를
  직접 닫으면 `detach_orphaned_mirror_sessions` 가 먼저 세션을 정리해 vec 에서 제거한다.
  `drain_auto_attach_results` 는 그 시점의 `anchor_ws_id() == Some(anchor) && state() ==
  Reconnecting` 조건으로 세션을 다시 찾으므로, 이미 사라진 세션에 대해서는 매치가 없어
  `reconnect_session` 자체를 부르지 않고 no-op(성공 취급)으로 넘어간다 — 해석해둔
  터널 핸들은 그 자리에서 drop 되며(Drop 시 자식 ssh kill), 되살아난 연결이 이미 닫힌
  workspace 에 잘못 쓰이는 사고는 없다.

## mirror 세션 종료 (client → 원격 점유 해제)

client 가 mirror 를 걷어내면 원격에 `Detach` 를 보내 원격 점유(hard workspace lock)를 해제해야 한다. 종료 트리거는 두 가지다.

- **원격발 종료(EOF/force-detach/read timeout)**: reader 스레드가 `Detach`/`force_detached`/EOF 를 받거나(위 "연결 생존 확인" 참조) read timeout 으로 소켓 read 가 에러를 반환하면 `disconnected` 플래그가 선다. `apply_attach_client_output` 이 이를 상태별로 분기한다 —
  - **anchor 있는 세션(매핑된 워크스페이스)**: `cleanup_mirror_workspace` 를 부르지 않고 `enter_reconnecting` 으로 전이(위 "재연결 시 세션 상태 보존" 참고) — mirror workspace/터미널을 살려둔 채 `Reconnecting` 상태로 두고 backoff 재연결에 맡긴다.
  - **anchor 없는 세션(임시 mirror, IPC `remote.attach` 등)**: 기존과 동일하게 세션 제거 + `cleanup_mirror_workspace(sess, from_disconnect=true)`. 그 mirror 가 창의 유일한 워크스페이스였으면 정리 뒤 기본 터미널 워크스페이스를 다시 만든다(`AppState::recreate_workspace_if_empty`) — 창을 닫지도, 워크스페이스 0 개로 남기지도 않는다([ADR-0536](../adr/0536-a-remote-drop-that-empties-a-window-recreates-a-default-workspace.md)).
  - 이미 `Reconnecting` 상태인 세션(재연결 시도 자체가 실패해 다시 disconnected 로 관측되는 경우)은 이 분기에 다시 들어오지 않는다 — `disconnected && state == Connected` 조건이라 진입 시 이미 걸러진다.
- **로컬발 종료(사용자 close)**: 사용자가 mirror 워크스페이스 **자체**를 닫으면(`close_workspace_at`, context menu/단축키) 로컬 ws 는 즉시 사라지지만 세션은 남는다 — 소켓이 열린 채라 원격은 계속 점유로 본다("사용 중" 잔류). 이는 세션이 `Connected`/`Reconnecting` 어느 쪽이어도 동일하다. `App::detach_orphaned_mirror_sessions`(`about_to_wait`)가 매 프레임 세션의 `local_workspace` 를 들고 있는 engine 이 살아 있는지(`mirror_workspace_engine_alive`)를 확인해, 하나도 없으면 고아로 보고 `cleanup_mirror_workspace` 로 정리한다. 판정 대상은 **창 있는 engine + parked engine** 이다 — 창이 하나도 없는 parked 상태(마지막 창 닫기 / macOS 최소화)에서도 engine 과 mirror 워크스페이스는 살아 있으므로 고아가 아니다(→ [remote-attach 기능 문서 "창 없는 상태(parked)에서의 세션 수명"](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명)). 세션 push 는 항상 ws 생성(같은 동기 함수) 뒤라 attach 셋업 중 false-positive 는 없다.
- **`cleanup_mirror_workspace`(공용, `from_disconnect: bool` 파라미터로 위 두 트리거 구분)**: mirror ws·터미널·mirror busy 엔트리·mesh 프레임 캐시 제거(창 있는 engine → parked engine 순으로 찾는다. 고아 판정과 순회 범위가 같아야 잔류가 없다. 이미 없으면 skip) → 원격에 `Detach` push → anchor 게이트(`auto_attach_active`) 해제 → 터널 kill. `from_disconnect=true`(원격발, anchor 없는 세션 한정)일 때만 anchor 를 `auto_attach_pending_reactivation` 에 추가(위 "GUI 자동 재연결 스코프" 참고) — `false`(로컬발/사용자 close)는 그 항목과 `auto_attach_reconnect` 스케줄 모두 명시적으로 제거해 이미 걷어낸 세션에 대한 재연결 시도를 남기지 않는다. 원격은 `Detach` 수신 시 read loop break → `Disconnected` → `release_all_for_client`(workspace+surface lock 해제).
- **적용 순회도 같은 범위다**: `apply_attach_client_output` 은 mirror 이벤트의 적용 대상을 창 있는 engine → parked engine 순으로 찾고(`mirror_output_host`), 찾은 뒤에만 reader 버퍼를 drain 한다 — 창이 없는 parked 구간에 도착한 출력·구조 delta 도 그 engine 에 즉시 적용된다(ADR-0110). 고아 판정·정리·적용 세 순회의 범위가 같아야 "살아 있다고 판정된 engine 에 적용이 닿지 않는" 유실 구간이 생기지 않는다.

## force-detach

서버 권한으로 점유 강제 해제(attach 하지 않음). holder client 에 `Control{force_detached}` + `Detach` push → client 가 mirror 정리·종료, 서버는 lock free.

- IPC: `attach.force_detach{surface_id}` / `attach.force_detach_workspace{workspace_id}`.
- CLI: `tasty remote attach --force-detach`(surface) / `--workspace <id> --force-detach`(workspace).
- **`--ssh` + `--force-detach` 미지원**(에러): force-detach 는 *이 서버* 에 붙은 client 락을 끊는 로컬 JSON-RPC 이지 터널 너머 원격 서버의 락을 끊는 게 아니다.

## 원격 생존 확인 (`remote check`)

`crates/tasty-cli/src/local/remote_check.rs`. 포트 발견만으론 **stale 포트 파일**(죽은 인스턴스 잔재)을 오판하므로 3단계: ① 포트 발견 → ② `ssh -L` 터널 → ③ 터널 localport 로 `system.info` **1회**. 응답이 와야 alive(stdout `alive: …`, exit 0). 거부/EOF/타임아웃 = dead(stderr, exit≠0). 단계별 상한은 ① 포트 발견 45초(위 "SSH 터널" 절의 3겹 상한) → ② 터널 ready-probe 5초 → ③ IPC 프로브 5초(`remote_browse::PROBE_TIMEOUT`) — 어느 단계도 무한 대기하지 않으므로 "단발 판정" 계약이 전 구간에서 성립한다. 1회성이라 재연결 백오프 없음. SSH 부품은 attach 와 공유, list 핸들러는 안 건드림.

## IPC 표면 (`attach.*`)

`src/adapters/ipc/handler/attach.rs`.

| 메서드 | 경로 | 설명 |
|--------|------|------|
| `attach.acquire` / `attach.release` | `stream.open{target}` 핸드셰이크 | 배타 lock 획득/해제 |
| `attach.force_detach` / `attach.force_detach_workspace` | JSON-RPC | 점유 강제 해제 |
| `attach.into_gui` | JSON-RPC | 실행 GUI 가 원격 워크스페이스를 mirror 로 재구성(`App::start_gui_attach`) |
| `attach.list` | JSON-RPC | 현재 점유 목록 조회(read) |

**권한**: attach 보안은 **연결 경계(SSH + 127.0.0.1 loopback)** 에 위임(decision 5). 자체 권한 레이어 없음 — 소켓에 도달한 caller 는 attach 제어를 호출할 수 있다. 근거는 [identity §2](../identity.md), 주체 계약은 [actors](../concepts/actors.md).

## 커스텀 이벤트 확장 (`StreamControl` 밖 raw JSON `event` 태그)

`StreamControl` enum 을 건드리지 않고 같은 `StreamTag::Control` 채널에 raw JSON `{"event": "...",
...}` 를 얹어 요청/응답 쌍을 왕복시키는 패턴 — 프로토콜 자체를 확장하지 않고 attach 채널을 다른
용도(원격 디렉토리 나열, 스크린샷 업로드, git 조회 등)로 재사용할 때 쓴다. 인가는 전부 "attach 점유
= 신뢰"(`engine.attach.client_holds_workspace(client_id)`, ADR-0040) — 별도 권한 게이트가 없다.
수신측(`StreamHub::pump_inbound`)은 알려진 `StreamControl` 파싱이 실패하면 이 raw 이벤트들을
순서대로 시도하고, 전부 실패하면 조용히 무시한다(전방 호환 — 모르는 이벤트를 에러로 취급하지
않음).

- **`list_dir_request`/`list_dir_result`**(ADR-0053) — 원격 디렉토리 나열(file picker). host popup
  wrapper 가 요청을 소유하고 응답을 직접 소비 — 소유자가 host 프로세스 내부라 `sent_at.elapsed()`
  로 매 프레임 soft timeout(8초)을 직접 판정할 수 있다.
- **`git_query_request`/`git_query_result`**(ADR-0056) — 원격 저장소 status/log/diff/worktrees 조회
  (git-viewer). 응답의 **소비자가 host 가 아니라 별도 프로세스인 plugin** 이라는 점이 file picker와
  다르다 — host(`attach_client.rs`)는 payload 를 해석하지 않고 그대로 `PluginManager::
  emit_host_event_to_plugin`(Event Bus 의 owner-unicast 경로, 구독 등록 여부 무관 —
  `command.invoked` 와 동일 메커니즘)으로 `git_viewer.query_result` 이벤트를 plugin 에 전달만
  한다. 요청 트리거도 plugin→host `git_viewer.query` IPC(비동기 accept — `request_id` 만 즉시
  회신하고 실제 forward 는 `CoreState.pending_git_query_forward` 를 다음 tick 에 drain)로 plugin
  이 직접 건다. **soft timeout 없음** — 소유자가 host 프로세스 밖(plugin)이라 ADR-0053 과 같은
  매 프레임 판정 루프를 둘 자리가 마땅치 않아 이번 스코프에서 구현하지 않았다. 세션 자체가
  끊기는 경우는 보내기 전(send-time 실패)이든 보낸 뒤(`cleanup_mirror_workspace` 가
  `request_id=0` sentinel 로 강제 abandon)든 즉시 `ok:false` 로 커버된다 — 빠진 건 "연결은 계속
  살아있는데 원격 tasty 프로세스 자체가 응답만 안 주는" 좁은 케이스뿐, ADR-0056
  Reconsideration Trigger 참고. egui-mesh popup 은 `popup.set_context`
  가 dirty 상태 변경시에만 나가 비동기 push 만으로는 다음 프레임 렌더가 보장되지 않으므로,
  `AppState.plugin_mesh_popup_pending_repaint: HashSet<u64>` 로 강제 repaint 를 예약한다
  (`popup_render.rs` 의 dirty OR-체인에 합류).
- **cwd 판정 — client 신뢰 vs server 직접 판정**: `list_dir_request` 는 client 가 forward 한 `dir`
  문자열을 그대로 쓴다. `git_query_request` 는 `worktree_path` 가 없으면 client 문자열을 신뢰하는
  대신 서버가 `surface_id` 로 자신의 실제 원격 PTY 를 찾아 `Terminal::get_cwd()`(OSC-7 캐시 우선,
  `/proc`·`proc_pidinfo` 폴백)를 직접 호출한다 — client 의 OSC-7 재생에 의존하지 않아 "원격 셸이
  OSC 7 을 방출하지 않는 경우"도 커버한다. 두 표면이 다른 이유는 각자 작업 시점의 판단이며,
  `list_dir_request` 를 이후 동일 방식으로 바꿀지는 별도 결정 사항.

## 관련

- 결정 근거(원격 대상·로컬 debug 격리): [`ADR-0007`](../adr/0007-attach-targets-remote.md)
- 동작·점유 규칙·CLI/IPC 사용법: [`features/remote-attach`](../features/remote-attach/index.md)
- 주체(원격 사용자)·점유 모델 개념: [`concepts/actors`](../concepts/actors.md)
- 원격 디렉토리 나열: [`ADR-0053`](../adr/0053-native-file-picker-remote-attach-channel.md)
- 원격 git 조회(git-viewer): [`ADR-0056`](../adr/0056-git-viewer-remote-attach-git-query-channel.md)
- SSH 프로필 관리: [`features/ssh-tool`](../features/remote-profiles/index.md)
- 로컬 self attach 격리: [`dev-guide/debug-ipc`](debug-ipc.md)
</content>


## Self-attach 거절 검증

GUI의 IPC/user dispatch 두 경로는 `attach_client/dispatch.rs::dispatch_attach`를 공유한다.
자기 포트면 `RejectedSelf`로 반환하고 connector를 호출하지 않는다. 다른 포트면 connector를
정확히 한 번 호출하며 성공값/오류를 그대로 돌려준다. IPC는 성공해도 포커스를 옮기지 않고,
사용자 경로만 기존처럼 새 mirror를 포커스한다. 이 분기는 raw stream client의 정상 로컬
attach와 별개다.

정확성 시험은 connector 진입 횟수와 결과를 직접 단언한다. debug GUI의 통합시험은
`attach_dispatch_completed` 로그에서 자기 port/workspace/source에 대응하는 **완료**와
`connector_entries=0`, `outcome=rejected_self`를 확인한다. 이 기록은 dispatcher 반환 뒤에
생기며, connector에 들어갔으면 실패해 돌아와도 진입 횟수가 남는다. 기록 구현은 모듈 선언에
`cfg(debug_assertions)`가 붙은 별도 `dispatch/debug_completion.rs`에 있다. 새 IPC나 전역
진행 카운터는 없다.

`tests/attach_silent_disconnect.rs`는 점유가 잡히지 않는 관측과 정상 재attach도 유지한다.
6초 관측 창의 RTT 수열·최악·중앙값·2초 이상 표본 수는 진단이며 정확성 단언에 쓰지 않는다.
GUI debug 시험의 완료 기록이 없으면 queued 응답만으로 통과하지 않는다. headless에서
GUI 큐가 처리되지 않는 현재 동작은 별도 시험 이름으로 검사하므로, 그 초록을 GUI 거절
검증으로 세지 않는다. release의 공통 분기는 유닛시험 대상이며 debug 로그를 요구하는
통합시험은 debug 조합에만 있다. 근거와 대안은 [ADR-0284](../adr/0284-self-attach-is-judged-by-connector-entry.md).
