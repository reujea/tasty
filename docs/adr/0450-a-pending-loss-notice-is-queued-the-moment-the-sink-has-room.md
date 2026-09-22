# ADR-0450: 밀린 손실 통지는 sink 에 자리가 나는 순간 갚는다 — ADR-0400 의 통지 지연 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: attach, stream, ipc, backpressure, resync, cli, adr-0400, adr-0334
- **Group**: remote-attach

## Context

[ADR-0334](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) 는 버린 프레임을
선언한 연결에 `StreamControl::Loss{frames}` 로 알리고, 통지가 막히면 빚(`StreamSink::pending_loss`)
으로 들고 있다가 다음 `push` 의 맨 앞에서 갚게 했다. [ADR-0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md)
결정 5 는 그 지연에 상한을 두려고 `pump_inbound` 가 돌 때마다 모든 연결의 빚을 갚게 하고, 상한을
"sink 에 자리가 난 뒤 **그 연결 자신의 다음 inbound 프레임까지**" 로 적었다.

그 상한은 **inbound 를 보내는 소비자**에게만 닿는다. CLI mirror-dump(`tasty debug attach`
기본 모드)는 attach 직후 선언 프레임 하나를 보낸 뒤 아무것도 안 보낸다(심장박동이 없다 —
`--dump-after` 창이 `HEARTBEAT_TIMEOUT` 보다 짧다는 전제). 그래서 다음이 겹치면 dump 는 공백을
모른 채 끝난다.

1. 버린 프레임이 그 surface 의 **마지막 출력**이다 — 뒤따르는 push 가 없다.
2. 소비자는 공백 앞 프레임을 전부 읽는다 — 큐에 자리가 났지만 갚는 자리(`push` · `pump_inbound`)
   가 안 돈다.
3. 창이 끝난다 — dump 는 공백 앞까지의 화면을 stdout 에 찍고, 서버가 이미 알고 있는 공백은
   말하지 않는다.

실측(격리 headless 인스턴스, 결정 시점 main `e33bc1758`):

- 원시 소비자(선언 후 읽기를 멈추고, sink 를 정확히 1024 장 채운 뒤 한 줄을 버리게 하고, 그 뒤로
  아무것도 안 보냄): 공백 앞 마지막 데이터를 0.011 s 에 받았고 `Loss` 는 **1.703 s** 에, 무관한
  `activity busy:false` push 와 같은 배치로 왔다. 그 push 가 없었으면 통지는 오지 않았다.
- CLI dump(`--dump-after 18000`, SIGSTOP 으로 멈춘 동안 같은 방식으로 한 줄을 버리고, 창 종료
  0.8 s 전에 재개, 그 사이의 `activity` push 도 정지 중에 버려지게 4 s 이상 둠): **8 회 중 8 회**
  경고 없이 버린 줄이 빠진 화면을 찍었다. 서버 화면에는 그 줄이 있었다.
- 검증 회차에서 본 1/4 간헐은 같은 기전이다 — 무관한 push 가 dump 창 **안에** 떨어졌느냐의
  우연이었다(떨어진 3 회는 재attach 했다).

## Decision

**빚은 sink 에 자리가 나는 첫 순간 — write 스레드가 큐에서 한 장을 꺼낸 직후 — 에 갚는다.**
꺼내는 쪽(`SinkReceiver`)이 빚이 있으면 곧바로 통지를 큐에 넣는다. 그 순간 큐에 든 것은 전부
버리기 전에 들어간 프레임이므로, 통지는 **공백 앞 마지막 프레임 바로 뒤**에 선다. 소비자는 공백
앞을 다 읽는 즉시 통지를 읽는다 — 뒤따르는 트래픽에도, 소비자의 inbound 에도 기대지 않는다.

- 빚이 없는 평상시에 write 스레드가 sink 맵 잠금을 잡지 않도록, 연결마다 "갚을 통지가 있다"
  (`loss_notify && pending_loss != 0`)의 원자 사본(`StreamSink::owes_notice`)을 둔다. 참값은
  언제나 두 칸이고, 사본은 두 칸이 바뀌는 자리에서만 따라 쓴다.
- `push` 맨 앞의 상환과 `pump_inbound` 끝의 상환은 남긴다. 뒤엣것은 이제 **선언이 손실보다 늦게
  온** 연결을 위한 것이다 — 소비자가 선언 전에 큐를 다 비웠으면 꺼낼 때는 갚을 것이 없었고,
  선언 뒤에는 꺼낼 것이 없다.
- 통지 지연의 상한은 이렇게 바뀐다: **통지는 소비자가 공백 앞 프레임을 다 읽는 순간 큐의 맨
  앞에 있다.** sink 가 계속 차 있으면(소비자가 안 읽으면) 여전히 안 나가고, 그 경우는
  `LAG_LIMIT` 강제분리가 끝낸다.

**개정하지 않는 것**: ADR-0400 의 결정 1–4 · 6(종류별 계약 · 재attach 수단 · 연결별 적용 ·
mirror stream 표지 · 세 값의 노출) 전부. ADR-0334 의 선언 게이트(`ClientLossNotify`) · 판 비교
(`STREAM_PROTO` 동등) · 통지가 막히면 빚으로 드는 방식 · 통지 성공이 `lag` 을 되돌리지 않는 규칙 ·
`Loss` 가 무엇을 잃었는지 싣지 않는 결정 · wire 형태. CLI dump 의 재attach 한도(3 회)와 심장박동
부재. 바뀌는 것은 "빚을 **언제** 갚는가" 하나다.

## Consequences

- **얻은 것**:
  - 조용해진 연결에서도 소비자가 따라잡는 즉시 공백을 안다. CLI dump 의 조용한 누락이 없어진다
    — 같은 결정적 재현에서 10 회 중 10 회 재attach 해 버린 줄이 든 화면을 찍었다(수정 전 8/8 누락).
  - 원시 소비자 측정에서 `Loss` 가 마지막 데이터와 같은 시각(0.021 s)에 왔다. 무관한 push 는 그
    뒤 2.372 s 에 따로 왔다.
  - 새 wire 가 없다. 선언하지 않은 연결은 바이트 단위로 무변경이고, 구 client 는 선언을 안 하므로
    영향이 없으며, 구 서버에 붙은 신 client 는 종전과 같다(구 서버의 지연이 그대로 남을 뿐이다).
- **잃은 것**:
  - 빚이 있는 동안 write 스레드가 한 장 꺼낼 때마다 sink 맵 잠금을 잡는다(메인 루프의 `push` 와
    같은 잠금). 빚은 손실 직후에만 생기고 갚는 즉시 사라지므로 평상시 비용은 원자 load 하나다.
  - ADR-0334 의 "1:1 소비에서 선언 연결이 끊긴다" 는 관찰은 **그대로다** — 비워진 한 칸을 통지가
    먼저 가져가는 것은 같고, 시점이 `push` 에서 꺼냄으로 앞당겨졌을 뿐이다. 실측(이 결정 뒤의
    `StreamHub` 단위, 0334 와 같은 264 회 상한 · `(마지막 PushResult, Sent 수, 소비자가 받은 Control
    수)`): 한 칸씩 비울 때 미선언 `(Sent, 264, 0)` · 선언 `(Disconnected, 0, 0)`, 두 칸씩 비우면 둘 다
    `(Sent, 264, 0)` — 0334 가 적은 값과 같다.
- **이 결정 전부터 있던 한계 — 이 결정이 넓히지도 좁히지도 않는다**:
  - `push` 가 통지 상환에 실패한 직후 write 스레드가 잠금 없이 한 칸을 비우면, 그 push 의 본
    프레임이 통지보다 먼저 들어갈 수 있다(공백 뒤 프레임이 통지를 앞지른다). `push` 의 "상환
    시도 → 본 프레임 넣기" 순서는 이 결정 전(`e33bc1758`)과 같아서 경합이 나는 창도 같다. 코드
    읽기로 찾은 경합이고 실측으로는 관측하지 못했다 — 두 스레드를 실제로 경합시킨 시험(200 000
    프레임 × 20 회)이 그 갈래를 막는 가드를 **빼도** 한 번도 실패하지 않았다. 재지 못하는 동작
    변경을 넣지 않으려고 이 ADR 은 그 가드를 두지 않았다. 아래 재검토 조건.
  - 앞질러도 일시적이다 — 소비자는 공백 뒤 한 장을 받은 직후 `Loss` 를 받고, 선언 소비자(GUI ·
    CLI)는 재attach 스냅샷으로 화면을 덮는다. "직후" 는 코드 읽기다: 앞지른 한 장이 큐의 마지막
    칸을 채우므로 그 뒤의 push 는 통지를 먼저 넣으려다 실패하고 본 프레임을 버리며, 다음에 한
    칸이 비는 순간(write 스레드의 꺼냄)에 통지가 들어간다 — 같은 경합이 되풀이되지 않는 한 통지는
    그 한 장 바로 뒤에 선다.
- **운영 비용 / 유지 부담**: sink 당 `AtomicBool` 하나, 수신 끝에 연결 id 와 sink 맵의 `Weak`
  하나. `loss_notify` · `pending_loss` 를 바꾸는 자리마다 사본을 맞추는 호출 하나(`sync_owed`) —
  빠뜨리면 사본이 참값과 어긋난다. 그 자리는 셋(선언 · 버림 · 상환 성공)이다. 선언·버림 자리의 호출과
  꺼낼 때의 상환을 각각 지우는 변이는 아래 시험 중 하나 이상을 실패시킨다. 상환 성공 자리는 지워도
  사본이 참으로 남을 뿐이고 `repay_pending_loss` 가 참값으로 다시 판정하므로, 잠금을 한 번 더 잡는
  비용 말고는 달라지는 것이 없다(그 변이에서 `stream_hub` 시험 37/37 통과 — 막을 동작이 없다).

## Alternatives Considered

- **CLI dump 가 창 끝에 Ping 을 보내고 잠깐 더 기다린다** — 안 골랐다. 얼마나 기다릴지가 또
  하나의 상수이고, 통지가 오지 않는 것(공백이 없음)과 아직 안 온 것을 가를 수 없어 기다림이
  늘 그 상수만큼 든다. 게다가 고치는 곳이 소비자 하나뿐이라 inbound 가 뜸한 다른 소비자는 그대로
  남는다. 서버가 자리가 나는 순간을 알고 있으므로 거기서 푸는 것이 맞다.
- **CLI dump 에도 심장박동을 붙인다** — 안 골랐다. 상한이 `HEARTBEAT_INTERVAL` 이 되는데, dump 의
  기본 창(500 ms)이 그보다 짧아 아무것도 바뀌지 않는다.
- **client→server 동기화 요청(`Sync` → `SyncAck`)을 새로 둔다** — 안 골랐다. 새 wire 라 선언
  게이트나 capability 협상이 하나 더 필요하고, 구 서버에 대해서는 여전히 기다림이 무기한이다.
- **빚을 갚는 주기 타이머를 둔다** — ADR-0400 이 기각한 그대로다(메인 루프 tick 과 새 상수).
  게다가 주기만큼 늦는다. 꺼내는 순간은 주기가 아니라 사건이라 늦지 않는다.
- **수신 끝이 `SyncSender` 사본을 들고 스스로 통지를 넣는다** — 안 골랐다. 사본이 살아 있으면
  허브가 sink 를 지워도(강제분리 · 등록 해제) 채널이 안 끊겨 write 스레드가 종료를 못 본다.
  그래서 수신 끝은 sink 맵의 `Weak` 를 들고, 맵 안의 sink 를 거쳐 넣는다.
- **`push` 가 상환에 실패하면 본 프레임도 버려 빚에 더한다(앞지르기 가드)** — 이번에는 안
  골랐다. 위 "이 결정 전부터 있던 한계" 의 경합을 닫지만, 그 경합을 재는 채널이 없어 가드를 빼도 아무 시험이
  실패하지 않는다. 재지 못하는 동작 변경은 넣지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것**

- `SinkReceiver` 를 거치지 않고 sink 를 비우는 경로가 생긴다 — 그 경로에서는 빚이 다시 다음
  push 나 inbound 를 기다린다. 판정: `crates/tasty-ipc/src/stream_hub.rs` 의 `rx` 필드를
  `SinkReceiver` 밖에서 읽는 자리.
- `push` 와 write 스레드 사이에 두 프레임을 원자적으로 넣는 수단이 생긴다(예: 잠금을 잡은 채
  꺼내는 채널) — 그러면 앞지르기 경합을 구조로 닫을 수 있다.

**원리적으로 안 붙는 것**

- 공백 뒤 프레임이 통지보다 먼저 도착한 사례가 관측된다. 재는 법: 순번을 실은 출력을 loopback
  attach 소비자로 받으며 소비자를 간헐적으로 멈추고, 순번이 건너뛴 자리 바로 앞이 `Loss` 인지를
  센다. 한 건이라도 나오면 앞지르기 가드를 넣는다.
- 빚이 있는 동안의 잠금 경합이 메인 루프 지연으로 보인다. 재는 법: 느린 소비자로 손실을 반복
  시키며 같은 인스턴스에 가벼운 IPC(`system.info`)를 반복 보내 왕복 시간을 재고,
  `system.pressure` 의 `stream_push.frames_dropped` 증가 구간과 겹쳐 본다.

## References

- 개정 대상: [ADR-0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md) (결정 5 — 통지 지연의 상한)
- 개정 대상: [ADR-0334](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) (막힌 통지를 갚는 자리)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 동작 문서: [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — "client 에게 공백을 알린다" 절
- 코드 근거(결정이 실현된 현재 위치): `tasty_ipc::stream_hub::{SinkReceiver::took, StreamHub::repay_pending_loss, StreamHub::sync_owed, StreamHub::repay_all_pending_loss}` · 시험 `a_notice_follows_the_last_survivor_with_nothing_pushed_and_nothing_sent_after_the_loss` · `a_declaration_that_arrives_after_the_loss_is_answered_in_the_same_inbound_batch` · `a_declaration_between_the_loss_and_the_drain_is_repaid_by_the_write_thread`
