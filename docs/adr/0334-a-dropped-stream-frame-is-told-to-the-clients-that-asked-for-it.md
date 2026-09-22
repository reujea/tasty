# ADR-0334: 버린 스트림 프레임은 그것을 받겠다고 말한 client 에게만 알린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: attach, stream, ipc, capability, backpressure, forward-compat, adr-0312
- **Group**: remote-attach

## Context

`StreamHub::push` 는 막히지 않는다 — client 의 sink 가 차 있으면 그 프레임을 버리고
`PushResult::Dropped` 를 돌려준다. 버린 수는 `StreamHub::loss()` 의 누적 카운터에 남지만 그
값을 읽는 제품 경로가 없고, `push` 를 부르는 자리 대부분이 결과를 버린다. 그래서 **연결이
살아 있는데 가운데 프레임이 사라진 경우 client 는 그 사실을 알 길이 없다.** 연속 drop 이 한도를
넘어 연결을 끊는 갈래는 이미 시끄럽지만(client 가 재연결한다), 그 아래의 단발·소규모 손실은
조용하다. 화면은 이전과 연속이 아닌데 연속인 것처럼 그려진다.

[ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) 이
"손실 Control 프레임" 을 후속으로 이름지어 뒀다.

통지를 붙이려 할 때 두 가지가 걸린다.

1. **`StreamControl` 의 전방 호환 규약이 여기서는 안전하지 않다.** 이 enum 은 `event` 태그
   기반이고, 모르는 event 는 역직렬화 실패로 **조용히 무시**된다. 그 무시가 공짜인 것은 그
   프레임을 놓쳐도 잃는 것이 없을 때뿐이다. `Loss` 는 반대다 — 못 읽은 client 에게 그 무시는
   "손실이 없었다" 와 구별되지 않는다. 옛 client 는 새 서버에 붙었다는 이유만으로 **더** 틀린
   화면을 갖게 된다.
2. **`STREAM_PROTO` 를 올리는 것은 게이트가 아니라 차단이다.** 서버는 client 가 선언한
   `StreamOpenParams::proto` 를 이 상수와 **동등 비교**한다(`validate_stream_proto`). 올리면
   기능이 좁아지는 것이 아니라 구 peer 가 **아예 못 붙는다.** 이 수는 기능 수준이 아니라 하드
   호환 게이트다.

그리고 통지 자체도 막힌다. 버리는 순간은 정의상 sink 가 찬 순간이라, 그 순간에 통지를 넣으려
해도 같은 이유로 실패한다.

## Decision

**손실 통지를 더하되, 판이 아니라 이름과 선언으로 게이트한다. 통지가 막히면 빚으로 들고 다음
빈 칸에서 갚는다.**

- 서버는 `StreamControl::Loss{frames}` 를 보낸다 — 직전 `Loss` 이후(첫 통지면 연결을 연 이후)
  **그 연결에서** 버린 프레임 수.
- **선언한 client 에게만 보낸다.** client 가 `StreamControl::ClientLossNotify{}` 를 한 번 보내면
  `StreamHub::pump_inbound` 이 `StreamHub::enable_loss_notify` 로 그 sink 에 표시한다. 선언하지
  않은 peer 가 받는 바이트 열은 이 변경 전과 **동일**하다.
- **`STREAM_PROTO` 는 움직이지 않는다.** 더해지는 스트림 기능은 이름으로 선언한다 —
  `capability::CAPABILITIES` 에 `ipc.stream.loss-notify` 를 더하고 `system.info` 로 노출한다.
  항목 추가는 구 client 에 영향이 없다(ADR-0312 이 그 성질을 정한 자리다). `STREAM_PROTO` 는
  프레임의 *기존* 의미가 바뀔 때만 움직인다.
- **막힌 통지는 빚이다.** `StreamSink::pending_loss` 에 수를 쌓고, 다음 `push` 의 맨
  앞(`StreamHub::repay_pending_loss`)에서 한 칸이 비면 통지를 **먼저** 넣는다. 넣기에 실패하면
  빚을 지우지 않는다 — 전액을 다음 기회로 미룬다.
- **통지 성공은 소비자가 따라잡은 것으로 세지 않는다.** `repay_pending_loss` 는
  `StreamSink::lag` 을 건드리지 않는다. 건드리면 서버가 스스로 넣은 프레임이 `LAG_LIMIT`
  강제분리 시계를 되돌려, 영원히 안 읽는 소비자가 영원히 안 끊긴다.

**이 ADR 이 정하지 않는 것**: 통지를 받은 client 가 *무엇을 어떻게 복구하는가*. 복구 수단은
프레임 종류마다 다르고(터미널 출력은 재요청할 곳이 없고, mesh 는 `MeshFullResendRequest` 가
있고, 1Hz diff push 는 다음 tick 이 스스로 고친다) 그 계약은 별도다. 지금 client 측 소비는
경고 로그 한 줄이다.

## Consequences

- **얻은 것**: 조용하던 손실이 말을 한다. 통지의 **위치**가 곧 의미다 — 마지막 생존 프레임과
  공백 이후 첫 프레임 사이에 앉으므로 "여기서 끊겼다" 를 수 없이도 읽을 수 있다. 선언하지 않은
  peer 는 바이트 단위로 무변경이라 롤아웃 순서에 제약이 없다.
- **잃은 것**: `StreamControl` 의 "모르면 무시" 규약이 이제 **무조건** 참이 아니다. 변종을 더할
  때마다 "놓치면 공짜인가" 를 물어야 하고, 아니면 이것과 같은 선언 게이트를 붙여야 한다. 그
  물음이 enum 의 doc 에 적혀 있지만 강제 채널은 없다.
- **★ 잃은 것 (선언이 공짜가 아니다 — 회복 문턱이 한 칸에서 두 칸으로 올라간다)**: 통지는
  본 프레임보다 **먼저** 빈 칸을 가져가는데 `lag` 은 본 프레임이 들어갔을 때만 0 이 된다.
  그래서 소비자가 **push 한 번당 정확히 한 칸씩** 비우는 구간에서는, 빚이 있는 선언 연결이
  본 프레임을 한 장도 못 넣고 `lag` 이 단조증가해 `LAG_LIMIT` 에서 끊긴다. 같은 속도의
  **안 선언한** 연결은 매번 `Sent` 라 안 끊긴다. 실측(`StreamHub` 단위, 상한 `LAG_LIMIT+200`
  = 264 회 · `(마지막 PushResult, Sent 수, 소비자가 받은 Control 수)`): 한 칸씩 비울 때
  미선언 `(Sent, 264, 0)` · 선언 `(Disconnected, 0, 0)`, 두 칸씩 비우면 둘 다
  `(Sent, 264, 0)` 으로 **차이가 사라진다**. 즉 비용이 나는 구간은 정확히 1:1 소비다.
  `src/app/attach_client.rs` 가 무조건 선언하므로 **모든 tasty mirror** 가 그 쪽에 있다.
  대안(통지 성공에 `lag` 을 되돌리는 것)은 아래 Alternatives 에서 기각했고 그 기각은
  유효하다 — 그러면 영원히 안 읽는 소비자가 영원히 안 끊긴다. 이 항목은 그 기각의
  **반대쪽 값**을 적는 자리다.
- **★ 잃은 것 (통지의 지연에 상한이 없다)**: 빚은 **다음 `push` 의 맨 앞**에서만 갚히고
  `StreamHub::repay_pending_loss` 의 호출자는 `push` 하나뿐이다. 그런데 server→client push 는
  전부 **변화 구동**이다 — 1 Hz tick 에 올라타는 셋(`Activity`·`Attention`·`Cwd`)이 diff 만
  밀고(그 성질을 `busy_activity_forwards_only_on_change` 가 고정한다), 나머지는 PTY 출력 tap ·
  구조 회신 · mesh 처럼 사건이 있을 때만 민다. **무조건 도는 주기 push 가 없다.** 그래서
  폭주 직후 그 surface 가 조용해지면 빚은 **무기한** 남고, mirror 는 이미 공백이 난 화면을
  경고 없이 그린 채로 있는다. 통지가 도착하는 시점은 그 연결에 다음 프레임이 밀리는 때이고
  그것은 사용자 입력 같은 외부 사건에 달렸다.
- **운영 비용 / 유지 부담**: sink 당 `bool` + `u64` 두 칸. `push` 의 hot path 에 분기 하나
  (`loss_notify && pending_loss != 0`). capability 이름이 하나 늘었고, 그 이름은
  `system.info` 의 공개 표면이라 지우려면 같은 절차를 밟아야 한다.

## Alternatives Considered

- **`STREAM_PROTO` 를 2 로 올려 게이트한다** — 배차가 처방한 형태인데 **불가능**하다. 서버가
  동등 비교를 하므로 판을 올리는 순간 판 1 을 선언하는 모든 peer 가 거절된다. 게이트가 아니라
  전면 차단이고, 비교하는 자리는 이 결정의 범위 밖 파일이다.
- **선언 없이 모두에게 보낸다** — 옛 client 가 조용히 무시하고, 그 무시가 "손실 없음" 으로
  읽힌다. 통지를 붙인 결과가 *덜* 정확한 client 를 만든다.
- **`loss()` 누적 카운터를 IPC 로 노출하고 client 가 폴링한다** — 그 값은 서버 전체의 누적이라
  "내 연결이 이번에 몇 장 잃었나" 에 답하지 못하고, 폴링 시점과 공백 위치의 대응이 사라진다.
- **통지를 못 넣으면 버린다** — 통지가 막히는 순간이 정확히 손실이 일어나는 순간이라, 가장
  필요할 때 통지가 사라진다.
- **통지를 넣을 때 `lag` 을 0 으로 돌린다** — 강제분리가 무력화된다. 변이로 확인했다
  (`sending_the_notice_does_not_count_as_the_consumer_keeping_up`).
- **`Loss` 에 무엇을 잃었는지(surface·종류)를 싣는다** — `push` 는 프레임을 불투명하게 다루고
  버리는 자리에 그 정보가 없다. 실으려면 drop 시점에 프레임을 해석해야 하는데, 그것이 곧
  복구 계약이고 위에서 미룬 것이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `STREAM_PROTO` 가 1 이 아니게 된다 — 판이 움직였다면 "동등 비교라 못 올린다" 는 전제가
  바뀐 것이다. `crates/tasty-ipc/src/stream.rs` 의
  `adding_a_control_variant_does_not_move_the_handshake_version` 이 그 좌변을 고정한다.
- `StreamControl::Loss` 말고 **두 번째** 선언 게이트가 생긴다 — 그때는 게이트가 패턴이므로
  이름 하나씩이 아니라 일반 협상(capability 목록을 client 가 되돌려 주는 형태)을 다시 본다.
  좌변: `capability::CAPABILITIES` 에 `ipc.stream.` 접두 항목이 둘 이상인가.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 통지를 받은 client 가 경고 로그 이상을 해야 한다는 필요가 실제로 나온다(종류별 복구 계약).
  재는 법: mirror 세션에서 `attach mirror: 서버가 이 연결의 프레임` 경고가 뜬 뒤 화면이 실제로
  어긋난 채 남는지 본다 — 어긋나지 않으면 그 종류는 다음 push 가 스스로 고치는 것이다.
- 빚 갚기가 실전에서 도달 가능한가. 재는 법: loopback 으로 느린 소비자를 만들고 서버 로그에서
  `repay_pending_loss` 가 통지를 넣는 순간과 client 가 `Loss` 를 받는 순간을 견준다. **지속
  과부하에서는 `LAG_LIMIT` 강제분리가 먼저 온다** — 이 갈래가 답하는 것은 일시적 지연이다.
- **정지 구간에서 통지가 얼마나 늦는가.** 위 항목의 반대편이다 — 그쪽은 부하가 계속될 때를
  묻고 이쪽은 **부하가 멈췄을 때**를 묻는다. 위 Consequences 가 적었듯 지연 상한이 없다.
  재는 법: mirror 세션에서 출력을 폭주시켜 drop 을 만든 뒤 그 surface 를 **조용히 두고**,
  client 에 `Loss` 가 도착하는 시각과 다음 프레임이 밀린 시각을 견준다. 둘이 같으면 상한이
  없는 것이 관측된 것이다. 주기 push 를 하나 두는 것이 처방 후보인데, 그것은 조용한 연결에
  프레임을 만드는 일이라 별개 결정이다.

## References

- [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) — 서버가
  판이 아니라 협상 가능한 것을 이름으로 선언한다. 이 ADR 은 그 결정이 이름지어 둔 후속 하나를
  소진한다.
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — "밀어내기 실패와
  누적 손실" 절과 그 아래 "client 에게 공백을 알린다" 절이 현재 운영 상태다.
- [`docs/dev-guide/api-conventions.md`](../dev-guide/api-conventions.md) — 스트림에 기능을
  더할 때 판을 올리지 말라는 규칙.
- 코드 근거(결정이 실현된 현재 위치): `StreamControl::Loss`·`StreamControl::ClientLossNotify`·
  `STREAM_PROTO` (`crates/tasty-ipc/src/stream.rs`), `capability::CAPABILITIES`
  (`crates/tasty-ipc/src/capability.rs`), `StreamHub::enable_loss_notify`·
  `StreamHub::repay_pending_loss`·`StreamSink::pending_loss`
  (`crates/tasty-ipc/src/stream_hub.rs`).
- 부분 개정: [0400](0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md) (통지 지연 조항 개정)
- 부분 개정: [0450](0450-a-pending-loss-notice-is-queued-the-moment-the-sink-has-room.md) (막힌 통지를 갚는 자리 개정)
