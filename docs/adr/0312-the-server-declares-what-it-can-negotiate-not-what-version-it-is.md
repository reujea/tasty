# ADR-0312: 서버는 자기 버전이 아니라 **협상 가능한 것**을 선언한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: ipc, capability, compatibility, method-table, system-info, adr-0306
- **Group**: ipc-contract

## Context

응답의 **모양**을 바꾸는 작업이 줄지어 있다 — 포화 시 거절 응답, "결과 불명" 을 실패와
구분하는 회신, 멱등 키, plugin 포화 통지, 손실 Control 프레임. 전부 같은 물음에 걸려
있다: **구 client 가 이 응답을 이해하나.**

지금 그것을 물어볼 자리가 RPC 쪽에 없다. 실측:

- `crates/tasty-ipc/src/protocol.rs` 는 `JsonRpcRequest`/`JsonRpcResponse`/`JsonRpcError`
  **봉투 타입 셋뿐**이고 `version`·`capabilit` 어느 이름도 **0 건**이다.
- `system.info` 가 내던 키는 **7 개**이고 그중 버전은 `env!("CARGO_PKG_VERSION")`
  하나였다. **패키지 버전은 기능 목록이 아니다** — 같은 버전의 두 빌드가 feature 조합에
  따라 다른 것을 한다(이 레포는 `gui` 를 패키지 단위 feature 로 쓰고 헤드리스 바이너리를
  따로 짓는다).

stream 쪽에는 이미 답이 있다. `STREAM_PROTO` 를 서버가 handshake 에서 동등 비교하고,
거절 ack 에 자기 판을 실어 client 가 서버 값을 알 수 있다. RPC 쪽에만 그 자리가 없었다.

## Decision

**서버가 이름 붙은 capability 목록을 `system.info` 응답에 싣는다.** 각 항목은 안정
이름과 판(`{name, version}`)을 든다.

- **새 handshake 를 만들지 않는다.** 이미 있는 응답에 키를 하나 더한다. 구 client 는
  모르는 키를 무시하므로 부수효과가 없고, 동결 baseline 가드와도 부딪히지 않는다 —
  그 가드가 얼리는 것은 **메서드 이름 집합**이지 응답 키가 아니다.
- **없는 기능을 적지 않는다.** 착지한 것만 이름을 받는다. 선언이 "곧 할 것" 을 담기
  시작하면 그 목록으로 분기한 client 가 깨진다.
- **판은 가능하면 근거에서 유도한다.** `ipc.stream` 의 판은 리터럴이 아니라 서버가
  비교하는 그 상수(`stream::STREAM_PROTO`)다. 둘로 적으면 갈린다.

**그리고 표가 메서드마다 "0.7.0 동결 표면에 있었나" 를 답한다**(`MethodSince`).
capability 가 *응답의 모양*을 묻는다면 이쪽은 *그 이름이 있기는 한가*를 묻는다 —
다른 물음이라 다른 자리에 둔다.

**그 값은 두 갈래뿐이고, 손으로 안 채운다.** 동결 파일이 유일한 모수이고 함수가 그것을
읽는다. 표에 이름을 더하면 답이 자동으로 따라온다. 미등록 이름은 `None` 이다 — "없다" 와
"예전부터 있었다" 를 같은 값으로 답하지 않는다.

## Consequences

- **얻은 것**: 뒤따르는 계약 변경들이 물어볼 자리를 갖는다. 이름을 하나 더하는 것이
  그 작업의 마지막 한 줄이 된다.
- **얻은 것**: 동결 파일이 표를 든 크레이트로 옮겨 와, 표와 그 스냅샷이 같은 자리에
  산다. **`tasty-ipc` 가** 자기 밖 파일을 굽지 않는다.
- **잃은 것**: 그 대신 **경계를 넘는 쪽이 바뀌었다.** 옮기기 전에는 그 파일이 루트 패키지
  안에 있어 동결 가드가 경계를 안 넘었는데, 이제
  `tests/api_baseline_0_7.rs` 가 크레이트 안으로 `include_str!` 한다. 경계 넘기가
  사라진 것이 아니다 — 그 경로를 또 옮기면 이번엔 루트 시험이 깨진다. 그쪽은 컴파일
  타임이라 시끄럽게 깨지지만, 이 결정을 "이제 아무도 경계를 안 넘는다" 로 읽으면 틀린다.
- **잃은 것**: `system.info` 응답이 항목 수만큼 길어진다. `window.list` 는 안 길어진다 —
  capability 는 서버의 성질이라 창마다 재사용되는 필드에 안 넣었다.
- **잃은 것 / 한계**: **"언제부터" 는 두 값이 한계다.** 동결 파일은 한 버전의 집합 하나
  이고, 그 뒤 더해진 이름들이 각각 언제 들어왔는지는 레포 어디에도 없다. 더 잘게 나누려면
  커밋 로그에서 추정해야 하는데 그 수는 재현되지 않는다. **그 "그 뒤" 가 몇 개인지는 여기
  적지 않는다** — 표에 이름이 하나 더해질 때마다 움직이는 값이라, 적는 순간 낡고 그것을
  근거로 범위를 잡은 사람이 하나를 빠뜨린다([ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md)).
  세는 법은 남긴다 — `METHOD_TABLE` 의 이름 중 동결 파일에 없는 것이고,
  `method_meta::method_since` 가 `AfterFrozenBaseline` 로 답하는 이름이 그것이다.
- **잃은 것 / 한계**: **`debug.` 이름의 "언제부터" 는 빌드 조합에 따라 갈린다.** debug 전용
  표가 release 에서 비므로 그 이름들은 debug 에서 `AfterFrozenBaseline`, release 에서
  `None` 이다. 동결 baseline 에 그 접두사가 0 개라 분류는 안 흔들리지만,
  `ipc.method-since` 는 조합과 **무관하게** 선언되므로 client 가 이 답으로 분기하기
  시작하면 그때 갈린다. 그때의 처방은 이 항목의 판을 올리는 것이다.
- **운영 비용**: 응답 모양을 바꾸는 작업마다 사람이 "이름을 줄 것인가" 를 한 번 판단한다.

## Alternatives Considered

- **A: 봉투에 프로토콜 판 필드를 하나 넣는다** — 안 골랐다. 한 수가 올라가면 **무엇이**
  바뀌었는지를 client 가 알 수 없어, 하나만 필요해도 전부를 포기하거나 전부를 가정해야
  한다. 이름 목록은 부분 채택을 표현한다.
- **B: RPC 에 stream 같은 handshake 를 새로 만든다** — 안 골랐다. 연결마다 왕복이 하나
  늘고, 그 왕복을 모르는 구 client 는 **연결 자체가** 안 된다. 지금 고치려는 것은 호환
  이지 호환을 깨는 것이 아니다.
- **C: 메서드마다 도입 버전을 정확히 적는다** — 안 골랐다. 값의 출처가 없다(위 한계).
  손으로 채우면 추정이 표의 값으로 굳고, 그 뒤로는 틀린 줄도 모른다.
- **D: `MethodSince` 를 표의 칸(생성자 인자)으로 둔다** — 안 골랐다. [ADR-0306] 이
  `effect` 를 그렇게 둔 이유는 **사람이 판단해야 하는 값**이었기 때문이다. 이쪽은
  판단이 아니라 파생이라, 칸으로 두면 동결 파일의 둘째 사본이 생기고 표에 이름을
  더하면서 그 칸을 빠뜨리는 것이 기본 동작이 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `ipc.stream` 의 판이 `stream::STREAM_PROTO` 가 아닌 리터럴로 바뀌었을 때. 유도가 이
  결정의 내용이다. (이 좌변에는 판정기가 있다 — `the_stream_capability_carries_the_constant_the_server_compares`.)
- 동결 파일이 major bump 로 갱신됐을 때. 그러면 `MethodSince` 의 두 값이 가리키는 버전이
  바뀌므로 이름(`FrozenBaseline`)이 뜻을 잃는다.
- capability 목록에 **이 트리에 근거가 없는 이름**이 들어왔을 때. "착지한 것만 적는다" 가
  결정의 내용이다. (이 좌변에 붙은 판정기는 지금 없다 — 이름과 구현을 잇는 것은 사람이다.)

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 구 client 가 `capabilities` 키 때문에 깨졌을 때. 재는 법: 그 client 가 응답을 엄격
  스키마로 파싱하는지 본다(알려진 소비자 둘은 안 그런다 — CLI 의 `list info` 는 결과를
  그대로 내보내고 `remote check` 는 두 키만 읽는다).
- 판을 올려야 할 변경과 이름을 더해야 할 변경이 실무에서 안 갈릴 때. 재는 법: 한 회차에
  같은 이름의 판이 두 번 오르면 그 이름의 범위가 너무 넓은 것이다.

## References

- [ADR-0306](0306-a-method-declares-what-a-second-delivery-leaves-behind.md) — 같은 표에
  재전달 축을 더한 결정. 값을 **칸**으로 둘지 **파생**으로 둘지의 갈림이 여기서 대비된다.
- [release](../dev-guide/release.md) — 0.7.x 동안 "추가만 가능, 제거 금지" 와 동결 파일
  갱신 절차.
- [ADR-0334](0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) —
  Context 가 후속으로 이름지어 둔 "손실 Control 프레임" 을 소진한 결정. 이름으로 선언하는
  형태(`ipc.stream.loss-notify`)가 거기서 처음 쓰였다.
- 코드 근거(결정이 실현된 현재 위치): `tasty-ipc` 의 `capability::CAPABILITIES` ·
  `capability::capabilities_json` · `method_meta::MethodSince` ·
  `method_meta::method_since`, 그리고 `handler::handle_system_info`.

[ADR-0306]: 0306-a-method-declares-what-a-second-delivery-leaves-behind.md
