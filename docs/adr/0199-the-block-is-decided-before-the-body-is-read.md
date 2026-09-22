# ADR-0199: 차단 판정은 body 를 읽기 전에 끝난다 — 순서를 주석이 아니라 소유권으로 적는다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: webhook, security, rate-limit, abuse, resource-bound, type-enforcement, adr-0046, adr-0112

## Context

[ADR-0112](0112-agent-stream-turn-correlation.md) 가 Consequences 에 남긴 관측이 있다 — 웹훅 리스너에는 **body 바이트 상한이 없고**, 인증은 그것을 막지 못한다(리스너가 body 를 먼저 전부 읽고 그 뒤에 경로·토큰을 본다). 그래서 비-loopback 노출에서는 리버스 프록시의 body 크기 제한이 유일한 실효 방어라고 적혀 있다. 그 문장은 지금도 맞다.

맞지 않는 것은 **범위**다. 남용차단은 그 예외다 — 쿨다운 중인 출처의 요청은 `reject_if_abusive` 가 `handle_request` 의 **첫 자리**에서 소비하고 `429` 로 답한다. 경로 파싱도, 헤더 수집도, body 읽기도 그 뒤다. 즉 차단된 출처에 대해서는 tasty 자신이 body 를 메모리에 올리지 않는다.

**단, 1024 바이트 이하는 예외다**(사실 정정 — 2026-09-08 에 `tiny_http` 0.12.0 의 `tiny_http/src/request.rs` 를 읽고 확인했다). `Content-Length` 가 1024 이하이면 그 라이브러리가 **요청 객체를 만드는 단계에서** body 를 통째로 버퍼에 읽어 둔다. 그 읽기는 tasty 의 코드가 요청을 보기 전에 끝나므로, 아래 결정이 무엇을 하든 그 크기의 body 는 이미 메모리에 있다. 이 ADR 이 지키는 범위는 **1024 바이트를 넘는 body** 다.

그것이 [ADR-0196](0196-abuse-thresholds-and-source-key.md) 이 관찰로 남긴 여덟 중 하나였다("429 선검사가 매칭 앞"). 재검토 기준을 "이 값이 바뀌면 사용자에게 보이는 것이 바뀌는가" 로 좁히면 이 항목은 걸린다 — 순서가 뒤집히면 차단이 **자원을 지키지 않게** 되고, 그 차이는 남용차단이 실제로 무엇을 막는지를 바꾼다. 그런데 그 순서를 지키는 것은 코드 배치뿐이었다. 한 줄을 위로 옮기는 것으로 조용히 깨지고, 깨져도 응답은 똑같이 `429` 다.

## Decision

**남용차단 선검사를 통과한 요청만 body 를 읽을 수 있게 타입으로 강제한다.**

`reject_if_abusive` 의 반환형을 `Option<tiny_http::Request>` 에서 `Option<Screened>` 로 바꾼다. `Screened` 는 통과한 요청을 감싸는 newtype 이고, body 를 읽는 `read_json_body` 는 그 타입의 메서드로만 존재한다. 쿨다운 중인 출처의 요청은 `Screened` 가 **만들어지지 않으므로**, 그 body 를 읽는 코드를 쓰는 것 자체가 컴파일되지 않는다. 헤더 수집·URL·메서드도 같은 타입으로 옮겨 요청을 다루는 입구를 하나로 모은다.

이 형태를 고른 이유는 [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) 의 불변식과 같다 — **"안 하기로 함" 이 아니라 할 수 없게** 만든다. 응답 빌더가 실행 결과를 인자로 받지 않아 정보 유출 경로가 아예 없는 것과 같은 선이다.

**이 결정이 하지 않는 것**: body 바이트 상한을 두지 않는다. 차단되지 **않은** 출처(정상 발신자와 아직 임계치에 닿지 않은 발신자 모두)의 body 는 여전히 통째로 메모리에 올라간다. ADR-0112 의 관측은 그 범위에서 그대로 유효하고, 상한은 별도 결정이다.

## Consequences

- **얻은 것**: 남용차단이 처리율만이 아니라 **자원**도 지킨다는 것이 구조로 고정된다. 순서를 바꾸는 변경은 컴파일 에러로 나타나므로, 그 성질을 시험 없이도 유지할 수 있다 — 이 항목은 애초에 시험으로 못 박기 어려웠다(`tiny_http::Request` 를 단위 시험에서 만들 수 없고, "읽었는가" 는 밖에서 관측되지 않는다).
- **잃은 것**: 요청 접근이 newtype 을 한 번 거친다. 위임 메서드 넷(`url`·`method`·`headers`·`read_json_body`)과 `respond` 가 늘었다.
- **운영 비용 / 유지 부담**: `Screened` 를 만드는 자리가 하나뿐이라는 것이 이 강제의 전부다. 다른 곳에서 `Screened(request)` 를 직접 만들면 강제가 사라진다 — 같은 모듈 안이라 컴파일러가 막지 못하고, 그것이 이 방식의 한계다.

## Alternatives Considered

- **주석으로 순서를 적는다(현상 유지)**: 비용 0. 그러나 이번 재검토가 발견한 것이 정확히 "그 순서를 지키는 것이 배치뿐" 이라는 사실이었다. 깨져도 응답이 같아 조용하다. 거부.
- **시험으로 고정한다**: `tiny_http::Request` 를 단위 시험에서 합성할 수 없고, e2e 로도 "body 를 읽었는가" 는 관측되지 않는다(응답은 고정 `429`, 시간차는 노이즈에 묻힌다). 관측 가능한 대리 지표가 없어 시험이 성립하지 않는다.
- **body 상한을 여기서 함께 둔다**: ADR-0112 가 가리킨 진짜 gap 이다. 그러나 상한값은 모든 웹훅이 공유하는 정책이고(어떤 발신자는 큰 payload 가 정상이다), 이 결정과 다른 물음이다. 섞으면 둘 다 근거 없이 정해진다. 별도로 둔다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **body 바이트 상한이 생기면** — 차단 여부와 무관하게 읽기가 제한되므로, 이 타입 강제가 여전히 값을 내는지 다시 본다.
- **`Screened` 를 만드는 자리가 둘 이상이 되면** — 강제가 이름뿐인 상태가 된다.
- **선검사가 요청 단위보다 앞(연결 수락 단계 등)으로 옮겨가면** — 그때는 body 뿐 아니라 요청 파싱 자체가 일어나지 않으므로 이 타입이 필요 없어진다.

## References

- [ADR-0112](0112-agent-stream-turn-correlation.md) — body 상한 부재와 그 범위(이 결정이 좁히는 관측)
- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — 불변식을 타입/함수 경계로 강제한다는 같은 형태
- [ADR-0196](0196-abuse-thresholds-and-source-key.md) — 이 항목을 관찰로 남긴 판정과 그 소비처 계수
- 코드 근거(결정이 실현된 현재 위치): `src/webhook/listener.rs` 의 `Screened`·`reject_if_abusive`·`handle_request`

- 후속 증거: [ADR-0281](0281-webhook-parser-cap-and-connection-drain.md) (Superseded by ADR-0516) — parser 보장과 Content-Length 정리의 선행 가정 오류·미충족 transport 요구. 기존 목표를 변경하거나 구현 완료를 선언하지 않는다.
- 잔여 읽기 중단의 구현: [ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) — 차단된 출처의 429 도 413 과 같이 레포에 둔 tiny_http 패치로 잔여 body 를 읽지 않고 연결을 닫는다.
