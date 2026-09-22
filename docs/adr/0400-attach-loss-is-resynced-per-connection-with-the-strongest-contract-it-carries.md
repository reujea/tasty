# ADR-0400: attach 손실은 연결 단위로 재동기화한다 — 종류마다 계약을 두고, 한 연결에는 그중 가장 강한 것을 쓴다 — ADR-0334 의 통지 지연 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: attach, stream, ipc, backpressure, resync, mirror, cli, observability, adr-0334
- **Group**: remote-attach

## Context

[ADR-0334](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) 로 서버는
버린 프레임을 **선언한 연결에** `StreamControl::Loss{frames}` 로 알린다. 그 ADR 은 통지까지만
정했고 받은 쪽이 무엇을 하는지는 미뤘다. 결정 시점의 상태는 이랬다.

1. **GUI mirror 는 통지를 받고 로그만 남겼다.** `src/app/attach_client.rs` 의
   `mirror_event_from_control` 이 `Loss` 에 `tracing::warn!` 후 `None` 을 돌려, mirror 는 공백이
   난 화면을 그대로 그렸다.
2. **CLI 는 선언조차 안 했다.** `crates/tasty-cli/src/local/attach.rs` 의 세 루프(workspace
   dump · surface dump · raw 브리지)는 `ClientLossNotify` 를 보내지 않았고, 서버는 그 연결에서
   종전대로 조용히 버렸다.
3. **통지의 지연에 상한이 없었다.** 빚(`pending_loss`)을 갚는 자리가 `StreamHub::push` 의
   맨 앞 하나였고, server→client push 는 전부 변화 구동이라 폭주 뒤 그 연결이 조용해지면 통지가
   무기한 안 나갔다(ADR-0334 "잃은 것").
4. **밖에서 읽을 수 있는 손실 값이 없었다.** `StreamHub::loss()` 누계는 프로세스 안에만 있었고,
   sink 에 쌓인 양(backlog)은 값 자체가 없었다 — `SyncSender` 는 길이를 안 준다. 있는 것은
   `lag`(성공 한 번에 0 이 되는 **연속** drop 수)뿐이었다.

`Loss` 는 **연결 단위**다. surface 도 데이터 종류도 싣지 않는다(ADR-0334 가 그것을 기각했다 —
버리는 자리에서 프레임을 해석해야 한다). 한편 한 연결이 나르는 종류는 여럿이다. workspace
mirror 연결 하나가 PTY Data(surface 접두 mux) · 상태 Control(`Resize`·`Activity`·`Attention`·
`Cwd`·`StructuralDelta`) · `MeshData` 를 함께 나르고, bulk 전송은 별도 연결로 `BulkResult`
하나를 받는다.

서버가 PTY snapshot 을 만드는 자리는 **attach 시점 하나뿐**이다 — `snapshot_as_vt()` 와
`add_output_tap()` 을 메인 루프의 한 턴에 함께 잡아(`attach_surface_for_stream` /
`attach_workspace_for_stream`) 그 사이에 바이트가 새지 않게 한다.

## Decision

**종류마다 재동기화 계약을 두고, 연결 하나에는 그 연결이 나르는 종류 중 가장 강한 계약을 쓴다.**

1. **종류별 계약.**

   | 종류 | 계약 | 뜻 |
   |---|---|---|
   | PTY byte delta | **snapshot 재요청** | 공백 뒤의 바이트를 공백 앞 화면에 이어 그리지 않는다. 새 snapshot 을 받고 그 위에서 다시 잇는다. |
   | 상태 snapshot (`Resize`·`Activity`·`Attention`·`Cwd`·구조) | **낡음 표시** | 통지를 받은 순간부터 새 값이 올 때까지 그 값은 최신이라고 말하지 않는다. |
   | mesh | **종료** | 공백을 건넌 조립·캐시를 잇지 않는다. 그 스트림은 끝나고, 이어서 보려면 처음부터(full texture) 다시 구독한다. |
   | bulk | **전송 중단** | 결과를 모르는 전송을 성공으로도 실패로도 확정하지 않고 중단으로 보고한다. 자동 재시도는 없다(서버가 이미 commit 했을 수 있다). |

2. **snapshot 재요청의 수단은 재attach 다.** 서버의 snapshot 생산자가 attach 하나뿐이므로
   "다시 달라" 는 곧 그 연결을 놓고 다시 붙는 것이다. 새 wire 는 없다 — 그래서 **구 서버에서도
   그대로 동작한다.** 순서가 계약의 일부다: client 는 옛 연결에 `Detach` 를 보내고, **옛
   연결의 EOF 를 본 뒤에** 새로 붙는다. 서버는 옛 연결의 `Disconnected` 를 inbound 채널에 넣은
   뒤에 소켓을 닫으므로(`finish_stream_connection`), EOF 를 본 client 가 여는 새 연결의 attach
   요청은 같은 FIFO 에서 반드시 그 뒤에 온다 — 점유가 옛 client 에 남아 새 attach 가
   `already_attached` 로 거절되는 경합이 없다.

3. **연결별 적용.**
   - **GUI workspace mirror 연결**(PTY + 상태 + mesh): 재attach. 통지를 받으면 경고 toast 로
     낡음을 알리고, 재attach 가 끝나면 재연결 성공 toast 가 난다. 재attach 는 기존
     `reconnect_session` 을 그대로 탄다 — survivor 매핑으로 로컬 id·scrollback 을 보존한다.
     mesh 는 재attach 가 옛 조립기(reader 스레드)를 버리고, 그 세션의 mesh surface 마다 캐시된
     frame 과 구독 dedup 상태를 지워 새 연결에서 처음부터 다시 구독한다. 세션마다 재attach 는
     **한 번에 하나**다 — 진행 중에 온 통지는 합산만 한다. 재attach 가 실패하면 disconnect 와
     같은 갈래다(anchor 가 있으면 `Reconnecting`, 없으면 정리).
     **창 없는(parked) engine 에서 온 통지는 재attach 를 미룬다** — 마지막 창을 닫았거나
     macOS 최소화로 engine 이 parked 에 있으면, 표지 갱신과 로그만 하고 옛 연결에 `Detach` 를
     보내지 않는다. 재attach 의 마지막 단계는 mirror 를 담은 창을 찾으므로 parked 에서 걸면
     실패하고, 실패 갈래는 anchor 없는 수동 attach 를 정리한다 — 손실 한 번에 mirror 가
     사라진다(아래 "손실이 나면 mirror 를 닫는다" 를 기각한 결과와 같다). 그 engine 이 다시
     창에 붙으면 `apply_attach_client_output`(3 초 주기 backstop 포함)이 그 창에서 옛 연결을
     놓고 낡음 toast 를 띄운다. 미루는 동안 옛 연결은 계속 출력을 실어 오고, 그 사이 연결이
     따로 끊기면 손실과 무관한 끊김 갈래를 탄다(이 결정 이전과 같다).
     **알려진 한계**: 창에서 시작한 재attach 가 옛 연결의 EOF 를 보기 전(서버가 소켓을 닫는 데 걸리는
     시간, 보통 ms)에 사용자가 마지막 창을 닫거나 최소화하면, 그 EOF 는 재attach 차례로 판정되지만
     `reconnect_session` 이 창을 못 찾아 실패하고, anchor 없는 세션은 정리된다. 소스 추적으로 찾은 창이고
     실행으로 재현하지 않았다.
   - **CLI surface / workspace dump**: 재attach 후 처음부터 다시 수집한다. 한 번의 실행에서
     재attach 는 최대 3 회이고, 넘으면 수집을 이어가 결과를 출력하되 stderr 로 공백이 있다고
     알린다. `--send` 입력은 첫 attach 에서만 보낸다(재attach 가 입력을 되풀이하지 않는다).
   - **CLI raw 브리지**: 재attach. 새 snapshot 이 화면을 다시 그린다. 횟수 제한은 없다 —
     대화형 세션이라 사용자가 `Ctrl+\` 로 끝낼 수 있고, 매 재attach 를 stderr 로 알린다.
     옛 연결을 놓고 기다리는 창에서도 stdin EOF 와 `Ctrl+\` 는 재attach 가 아니라 종료다.
   - **bulk 연결**: `ClientLossNotify` 를 선언하고, 결과를 기다리는 중 `Loss` 를 받으면 전송을
     중단으로 끝낸다.

4. **mirror 출력 위치의 stream 표지를 갈아 끼운다.** mirror 터미널도 받은 바이트를 자기 출력
   버퍼에 쌓고, 에이전트는 그 버퍼를 위치 커서로 읽는다([ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md)).
   공백을 건넌 위치는 "이어지는 바이트" 가 아니다. 그래서 mirror 가 `Loss` 를 받을 때와 재attach
   (손실이든 네트워크 끊김이든)로 snapshot 을 다시 받을 때 그 터미널의 stream 표지를 새로 만든다.
   옛 표지를 들고 읽는 소비자는 공백을 건넌 바이트 대신 stream 불일치 오류를 받는다. 위치 수는
   이어서 센다 — 표지를 안 싣는 옛 소비자에게는 무변경이다.

5. **통지 지연의 상한 — ADR-0334 의 해당 조항 개정.** 빚은 `push` 의 맨 앞에 더해
   **`pump_inbound` 가 돌 때마다** 모든 연결에 대해 갚는다. `pump_inbound` 는 어느 client 든
   프레임을 보낼 때마다 돈다. 살아 있는 연결은 `HEARTBEAT_TIMEOUT` 안에 무엇이든 보내야 하므로
   (안 보내면 서버 read 가 끊는다), 통지는 **그 연결의 sink 에 자리가 난 뒤 그 연결 자신의 다음
   inbound 프레임까지** — 심장박동을 보내는 client(GUI · raw 브리지)는 `HEARTBEAT_INTERVAL`,
   어떤 살아 있는 연결이든 `HEARTBEAT_TIMEOUT` — 안에 나간다. sink 가 계속 차 있으면 안 나가고,
   그 경우는 소비자가 안 읽는 것이라 `LAG_LIMIT` 강제분리가 끝낸다(0334 의 나머지 결정 그대로).

6. **세 값은 서로 다른 것이고, 밖으로 나간다.**
   - `lag` — 연결별 **연속** drop 수. 성공 한 번에 0. 강제분리의 좌변. 밖에 안 나간다.
   - `frames_dropped` · `clients_lagged_out` — 프로세스 수명 **누계**. 안 내려간다.
   - `backlog` — 지금 살아 있는 연결들의 sink 에 **쌓여 있고 아직 write 스레드가 안 가져간**
     프레임 수의 합. 게이지라 내려간다. sink 마다 원자 카운터가 push 성공에 +1, write 스레드의
     수신에 −1 이고, 끊긴 연결의 몫은 합에서 빠진다.

   셋 중 뒤의 둘을 `system.pressure` 의 `stream_push` 덩어리로 낸다(`sink_capacity` 와 함께 —
   `backlog` 가 얼마나 상한에 가까운지가 한 응답에서 읽히게).

**개정하지 않는 것**(ADR-0334): 선언 게이트(`ClientLossNotify`) · 판 비교(`STREAM_PROTO`
동등) · 통지를 큐에 자리가 날 때까지 빚으로 들고 있는 방식 · 통지 성공이 `lag` 을 되돌리지 않는
규칙 · `Loss` 가 무엇을 잃었는지 싣지 않는 결정. 바뀌는 것은 "지연에 상한이 없다" 는 조항 하나다.

## Consequences

- **얻은 것**:
  - 단발 손실도 화면에 조용히 남지 않는다. mirror 는 낡았다고 말하고 다시 그린다.
  - 새 wire 가 없어 구/신 조합이 전부 동작한다 — 구 서버는 `Loss` 를 안 보내므로 재attach 도
    안 일어나고, 구 client 는 선언을 안 하므로 서버가 종전대로 조용히 버린다.
  - 재attach 가 네트워크 재연결과 같은 경로라, 그 경로의 개선(survivor 보존 · 이번에 더한 mesh
    재구독 · stream 표지 갱신)이 두 원인에 함께 들어간다.
  - 운영자가 `tasty list pressure` 하나로 "조용한 손실이 있었나" 와 "지금 쌓여 있나" 를 가른다.
- **잃은 것**:
  - 프레임 한 장을 잃어도 연결 전체를 다시 붙는다 — descriptor 와 모든 터미널 snapshot 을
    다시 받는다. `Loss` 가 무엇을 잃었는지 모르므로 더 좁힐 수 없다.
  - 재attach 는 snapshot 을 survivor 터미널에 이어 넣는다. 네트워크 재연결과 같은 성질이라
    scrollback 에 화면 한 벌이 겹칠 수 있다.
  - 느린 소비자에서는 재attach 가 되풀이될 수 있다(snapshot 도 프레임이다). GUI 는 세션당 한
    번에 하나, CLI dump 는 3 회로 막고, raw 브리지는 막지 않는다.
  - 재attach 는 메인 스레드에서 핸드셰이크를 기다린다(기존 `reconnect_session` 과 같다).
- **운영 비용 / 유지 부담**: sink 당 원자 카운터 하나와 수신 쪽 래퍼 타입 하나. `pump_inbound`
  마다 sink 맵을 한 번 훑는다(빚이 없는 연결은 분기 하나). `system.pressure` 덩어리가 하나 늘어
  그 수를 적은 자리들이 함께 움직인다.

## Alternatives Considered

- **서버가 `Loss` 를 보낸 뒤 스스로 snapshot 을 다시 민다** — 안 골랐다. 서버는 어느 surface
  가 잃었는지 모르고(연결 단위), snapshot 과 tap 을 원자적으로 잡는 자리가 attach 경로
  하나라, 그 밖에서 snapshot 을 다시 찍으면 옛 forwarder 가 이미 큐에 넣은 tap 조각과 순서가
  섞인다. 구 client 는 그 snapshot 을 설명 없는 화면 재그리기로 받는다.
- **새 `StreamControl::ResyncRequest{surface_id}` 를 client→server 로 둔다** — 안 골랐다. 구
  서버는 그 변종을 무시하므로 client 가 답을 무기한 기다리고, 그것을 피하려면 협상이 하나 더
  필요하다. 재attach 는 이미 모든 서버가 아는 요청이다.
- **mesh 는 `MeshFullResendRequest` 로 따로 복구한다** — 안 골랐다. 같은 연결의 PTY 가 어차피
  재attach 를 요구하고, 재attach 가 구독을 새로 세우므로 full resend 는 그 안에 들어 있다.
  연결 하나에 계약 둘을 걸면 둘의 순서를 또 정해야 한다.
- **손실이 나면 mirror 를 닫는다** — 안 골랐다. 사용자의 작업 공간이 사라진다. 끊김에서도
  anchor 세션은 살려 두는 기존 방침과 어긋난다.
- **parked 에서도 곧바로 옛 연결을 놓고, 재attach 실패가 "창 없음" 때문이면 정리 대신
  `Reconnecting` 으로 남긴다** — 안 골랐다. anchor 없는 세션에는 `Reconnecting` 을 다시 깨울
  트리거(backoff 스케줄러)가 없어, 창이 돌아와도 mirror 가 끊긴 채 남는다. 미루는 쪽은 연결을
  살려 둬 parked 동안의 출력도 계속 받고, 이 결정 이전에 parked 손실이 mirror 를 남기던 동작을
  그대로 보존한다.
- **CLI dump 는 재attach 없이 공백을 표시하고 끝낸다** — 3 회 소진 뒤의 갈래로만 남겼다.
  dump 는 한 번 찍고 끝나는 검증 경로라, 다시 붙으면 공백 없는 화면을 얻을 수 있는데 그것을
  안 하면 검증 결과가 우연에 달린다.
- **빚을 갚는 주기 타이머를 둔다** — 안 골랐다. 메인 루프에 새 tick 을 걸어야 하고, 주기는
  또 하나의 상수다. `pump_inbound` 는 이미 연결마다 심장박동 주기로 돌고, 그 주기가 곧 상한의
  근거(`HEARTBEAT_TIMEOUT` 은 서버가 집행한다)다.
- **backlog 를 허브 전체 원자 카운터 하나로 센다** — 안 골랐다. 연결이 끊기며 큐에 남은
  프레임이 버려지면 전체 카운터에서 그 몫을 뺄 자리가 없어 값이 영구히 떠오른다. 연결별로
  세고 살아 있는 연결만 합하면 끊긴 연결의 몫은 저절로 빠진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- `Loss` 가 surface 나 데이터 종류를 싣게 된다 — 그러면 연결 전체가 아니라 그 surface 만
  다시 받을 수 있다.
- 서버에 attach 경로 밖의 안전한(tap 순서를 지키는) per-surface snapshot 생산자가 생긴다.
- 상태만 나르는 연결 종류가 생긴다 — 그 연결에는 재attach 가 아니라 낡음 표시만 걸면 된다.
- `STREAM_PROTO` 비교가 동등에서 범위로 바뀐다 — 선언 프레임 대신 판으로 게이트할 수 있다.

**원리적으로 안 붙는 것**

- 큰 workspace 에서 재attach 한 번의 비용(모든 snapshot 재전송)이 사용자에게 보일 만큼
  크다. 재는 법: 격리 인스턴스 두 개를 loopback attach 로 잇고 서버 쪽 터미널에 대량 출력을
  흘려 손실을 낸 뒤, client 로그의 재attach 시작·완료 시각 차를 잰다.
- 느린 링크에서 GUI 재attach 가 되풀이돼 화면이 계속 다시 그려진다. 재는 법: 같은 구성에서
  재연결 성공 toast 가 난 횟수와 `tasty list pressure` 의 `stream_push.frames_dropped` 증가를
  함께 본다.

## References

- 개정 대상: [ADR-0334](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) (통지 지연 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- StreamHub 의 위치: [ADR-0350](0350-the-stream-hub-lives-in-the-ipc-crate.md)
- 출력 위치 계약: [ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md)
- 요청 압력 게이지: [ADR-0305](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md)
- 동작 문서: [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md)
- 낡음 toast 가 기대는 toast 트리거 정책의 허용 부류: [ADR-0401](0401-remote-connection-events-may-raise-a-toast-without-a-user-action.md)
- 코드 근거(결정이 실현된 현재 위치): `tasty_ipc::stream_hub::{StreamHub::pump_inbound, StreamHub::loss, SinkReceiver}` · `app::attach_client::{MirrorEvent::Desynced, App::apply_attach_client_output}` · `tasty_cli::local::attach::{run_attach_on_port, run_attach_workspace_on_port}` · `adapters::ipc::handler::pressure::stream_push_json`
- 부분 개정: [0450](0450-a-pending-loss-notice-is-queued-the-moment-the-sink-has-room.md) (결정 5 — 통지 지연의 상한 개정)
