# ADR-0420: 멱등 키의 봉투 검사는 진입 게이트에서 한다 — 목적지와 무관한 판정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, protocol, idempotency, envelope, gate, compatibility, adr-0338
- **Group**: ipc-contract

## Context

[ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
은 요청 봉투에 `idempotency_key`(1..=256 바이트)를 더하고, 길이 밖 키(빈 문자열 · 상한 초과)를
`-32602` 로 거절하게 했다. 그 검사가 보존소 입구(`idempotency::begin`) **안**에 있었고, 그 함수를
부르는 자리는 engine 라우터(`route_checked_request`) 하나였다. 그래서 검사는 보존소가 닿는 범위를
그대로 물려받았다 — App 층이 직접 끝내는 메서드와 plugin namespace forward 로 가는 요청은 빈 키도
상한을 넘긴 키도 거절되지 않고 그냥 실행됐다. 같은 봉투가 **어디로 가느냐에 따라** 유효하기도
무효하기도 했고, ADR-0338 이 "봉투 수준 계약" 이라 부른 근거가 거기서는 안 섰다. ADR-0338 은 그것을
닫는 형태(검사를 라우터 앞으로)를 적어 두고 후속 조각으로 남겼다.

모든 IPC 진입점은 라우팅 전에 공통 게이트를 지난다 — GUI 의 `App::gates_before_routing`, 헤드리스의
`pump_ipc`, plugin host-call 두 진입부가 전부 `check_request` 를 부르고, 창도 parked engine 도 없는
GUI 부팅 구간은 `check_without_engine` 을 부른다. App 층 가로채기와 namespace forward 는 그 **뒤**에
있다.

## Decision

**봉투 검사를 `idempotency::check_envelope` 로 떼어 `check_request` 와 `check_without_engine`
(Local 갈래)에서 부른다. `begin` 은 더 이상 키 모양을 보지 않는다.**

- **자리는 세 게이트(권한 · cap · rate)와 허용 관측 뒤, `CheckedRequest` 를 돌려주기 직전이다.** 옛
  자리(engine 라우터의 보존소 입구)가 게이트 뒤였으므로, 거기 닿던 요청에서는 응답이 한 글자도 안
  바뀐다 — 권한 없는 호출자는 여전히 `-32001`(격상 레코드 포함)을 먼저 받고, 통과한 요청의 허용
  관측과 rate 소비도 예전처럼 한 번 남는다.
- **달라지는 것은 옛날에 검사를 안 받던 요청뿐이다.** App 층 메서드와 namespace forward 로 가는
  요청에 길이 밖 키가 실리면 이제 실행 전에 `-32602` 다. 그 요청은 전에는 키를 무시하고 실행됐다.
- plugin 이 호스트를 부르는 경로는 봉투를 스스로 만들며 키를 싣지 않으므로(`idempotency_key: None`)
  이 검사가 그쪽 동작을 바꾸지 않는다.

## Consequences

- **얻은 것**: 같은 봉투가 목적지와 무관하게 같은 판정을 받는다. "봉투 수준 계약" 이 이제 말 그대로
  성립한다.
- **얻은 것**: 판정이 한 함수에 있다. `begin` 이 같은 검사를 다시 하지 않으므로 두 자리가 갈릴 일이
  없다.
- **잃은 것**: 길이 밖 키를 실은 채 App 층 메서드나 plugin namespace 를 부르던 client 는 이제 거절을
  받는다. 그 client 는 봉투 계약을 이미 어기고 있었고, 받던 것은 "키를 무시한 실행" 이었다.
- **운영 비용**: 없다 — 검사는 문자열 길이 비교 하나다.

## Alternatives Considered

- **게이트보다 앞(권한 검사 전)에 둔다** — 잘못된 봉투를 가장 먼저 자르는 형태다. 그러나 engine
  라우터로 가던 요청에서 응답 순서가 바뀐다(권한 없는 호출자가 `-32001` 대신 `-32602` 를 받고, 격상
  레코드가 안 생긴다). 외부 호환을 가장 많이 지키는 쪽을 골랐다.
- **App 층과 forward 앞에 각각 검사를 더한다** — 검사가 세 자리가 되고, 새 층이 생기면 네 번째를
  빠뜨린다. 모든 진입점이 이미 지나는 게이트 한 자리가 있으므로 거기 둔다.
- **그대로 둔다** — 봉투 수준 계약이 목적지에 따라 참이기도 거짓이기도 한 상태가 남는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 게이트에서 검사가 빠지면
  `idempotency::tests::the_envelope_is_judged_at_the_gate_whatever_the_destination` 이 App 층 ·
  namespace · engine 세 목적지 모두에서 빨개진다(변이 확인: `check_request` 의 호출 한 줄을 지우면
  그 시험이 실패했다, 2026-09-21). 창 없는 GUI 구간의 `check_without_engine` 갈래도 같은 시험이 gui
  조합에서 함께 잰다(변이 확인: 그 갈래의 호출 한 줄을 지우면 실패했다, 2026-09-21).
- `check_request` 를 안 지나는 새 IPC 진입점이 생기면 — 그 진입점은 권한 게이트도 건너뛰므로 게이트
  자신의 시험들(`checked::tests`)이 먼저 문제를 드러낸다. 이 검사는 그 게이트에 얹혀 있다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 길이 밖 키로 거절당한 client 의 보고가 온다면 — 그 client 가 App 층이나 plugin namespace 에 키를
  싣고 있었다는 뜻이다. 재는 법: 보고된 요청의 메서드가 `method_meta` 에서 어느 층인지 본다.

## References

- 관련 ADR: [ADR-0338](0338-a-mutation-retry-is-told-apart-by-a-caller-key-and-the-peer-is-asked-before-the-effect.md)
  — 봉투 계약. 이 ADR 이 그 결정이 후속으로 남긴 검사 위치를 확정한다.
- 관련 dev-guide: [api-conventions](../dev-guide/api-conventions.md) 의 멱등 키 절.
- **코드 근거 (결정이 실현된 현재 위치)**: `idempotency::check_envelope`, `checked::check_request` ·
  `checked::check_without_engine`.
