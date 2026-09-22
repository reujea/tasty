# ADR-0200: 웹훅 body 는 요청당 바이트 상한을 갖는다 — 선언된 길이와 chunked 를 함께 막는다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: webhook, security, resource-bound, dos, body-limit, measurement, adr-0112, adr-0199, adr-0046
- **Group**: webhook-hooks

## Context

[ADR-0112](0112-agent-stream-turn-correlation.md) 가 Consequences 에 남긴 빚이다 — 리스너에 body 바이트 상한이 없고, 인증은 그것을 막지 못한다. [ADR-0199](0199-the-block-is-decided-before-the-body-is-read.md) 가 그 범위를 좁혔지만(차단된 출처의 1024 바이트 초과 body 는 안 읽는다), **차단되지 않은 발신자에게는 그대로 남아 있었다.**

재보니 그 자리가 생각보다 넓다. 실측(2026-09-08, 헤드리스 인스턴스 · 격리 홈 · 루프백):

| 보낸 것 | 응답 | 프로세스 RSS 최고치 |
|---|---|---|
| (기준선) | — | 27,368 KB |
| `Content-Length` 10 MB → **등록 없는 경로** | 404 | 37,648 KB |
| 같은 경로 100 MB | 404 | 146,608 KB |
| 같은 경로 300 MB | 404 | 352,308 KB |
| **`Transfer-Encoding: chunked` 20 초** (10.45 GB 업로드) | 404 | **10,245,168 KB** |

읽는 법. **웹훅을 하나도 등록하지 않아도, 인증을 걸어도 막히지 않는다** — body 는 경로 매칭보다 앞에서 읽히므로 응답이 `404` 인 요청도 제 크기만큼 메모리를 쓴다. 걸리는 것은 주로 **메모리**이고(크기의 1.0~1.2 배가 그대로 RSS), 시간은 부차적이다(300 MB 가 루프백에서 0.39 초). 그리고 리스너는 요청마다 스레드를 띄우므로 그 점유는 **동시 요청 수만큼 곱해진다.**

가장 나쁜 갈래는 마지막 줄이다. `tiny_http` 0.12.0 의 계약은 셋으로 갈린다(`tiny_http/src/request.rs` 를 읽고 확인) — `Content-Length` 가 1024 이하면 **요청 객체를 만드는 단계에서 통째로 버퍼에 읽고**(우리 코드가 보기 전이다), 1024 초과면 그 길이만큼의 리더를 주며, `Transfer-Encoding` 이 있으면 `Content-Length` 를 **무시하고** chunked 디코더를 준다. 마지막 갈래에는 **선언된 길이가 아예 없다.** 그래서 선언값만 보고 막는 상한은 상한이 아니다 — 초당 약 512 MB 로 20 초를 밀어 넣으면 10.2 GB 가 쌓인다. 그 프로세스는 터미널 본체다.

## Decision

**요청 하나가 읽는 body 에 바이트 상한을 두고, 넘으면 `413` 으로 답한다. 기본 1 MiB, `TASTY_WEBHOOK_MAX_BODY_BYTES` 로 오버라이드한다.**

1. **두 갈래를 함께 막는다.** 선언된 `Content-Length` 가 이미 상한을 넘으면 **한 바이트도 읽지 않고** 거절하고, 선언이 없는 chunked 는 **상한 + 1 바이트**에서 멈춘다(딱 상한만 읽으면 "정확히 상한" 과 "더 있다" 를 못 가른다). 뒤엣것이 없으면 상한이 상한이 아니다.
2. **멈추면 거기서 끝난다.** `tiny_http` 의 `respond` 와 `Drop` 은 읽지 않은 body 를 소비하지 않는다(소스 확인). 그래서 남은 바이트는 소켓에 남고 우리 메모리로 오지 않는다.
3. **초과는 `413 payload too large` 이고, 남용차단이 센다.** 상한이 한 건의 값을 묶었지만 **반복 횟수는 안 묶는다** — 큰 body 를 반복해 보내는 것은 그 자체가 자원을 겨눈 요청이고, 그 요청도 아무것도 얻지 못하고 끝난다. `AckStatus` 에 variant 를 더하자 [ADR-0195](0195-abuse-counting-includes-rejected-tokens.md) 가 만든 전수 `match` 가 그 자리에서 컴파일을 멈춰 답을 요구했다 — 그 설계가 의도한 대로 동작했다.
4. **값 1 MiB 가 지금 보장하는 것.** 이 값이 처음부터 옳다는 근거는 없다. 재서 적는다 — 단일 요청의 최악 점유가 **10.2 GB 에서 1 MiB + 파싱 상수**로 묶인다(아래 실측). 아래에서는 `tiny_http` 이 1024 바이트 이하를 **어차피 먼저 읽으므로** 상한은 그보다 충분히 커야 의미가 있고, 1 MiB 는 그 1024 배다. 이 레포가 아는 웹훅 payload(agent-stream 이 받는 프롬프트, e2e 가 쓰는 통지 body)는 수 KB 이하라 정상 사용의 수백 배 여유다. 더 큰 payload 를 받는 발신자(수십 MB 를 보내는 SaaS 훅 등)가 있으면 owner 가 env 로 올린다 — 강제하지 않는다.
5. **이름이 약속하는 것은 요청 하나다**. `MAX_BODY_BYTES` 는 요청당 상한이고 실제로 그것을 지킨다. **동시 요청 수는 이 값이 묶지 않는다** — 최악 점유는 `상한 × 동시 요청 수` 이고 리스너는 요청마다 스레드를 띄운다. 그 곱은 이 결정의 범위 밖이며, 상수 문서와 이 절에 그 사실을 함께 적는다.

**적용 후 같은 공격을 다시 쟀다**(같은 절차): `Content-Length` 300 MB → `413`, RSS 27,360 KB(기준선 27,244 — **최고치도 안 움직였다**). chunked 20 초 → `413`, RSS 27,524 KB · 최고치 28,124 KB 로 20 초 내내 평평. 발신자가 소켓에 밀어 넣은 양은 1,700,928 바이트에서 멎었다(서버가 읽은 양이 아니라 응답 전까지 in-flight 로 나간 양이다). **10,245,168 KB → 28,124 KB.**

## Consequences

- **얻은 것**: 등록도 인증도 없는 요청 하나로 호스트 메모리를 무한정 밀어 올리는 경로가 닫혔다. 그 프로세스가 터미널 본체라 OOM 은 세션 전체를 잃는 사건이었다. 반복 시도는 `413` 집계로 쿨다운에 걸린다.
- **잃은 것**: 1 MiB 를 넘는 정상 payload 는 **기본값으로는 거절된다.** owner 가 env 로 올려야 하고, 그 사실은 응답 `413` 과 문서로만 전해진다(단방향 ACK 라 "얼마까지 되는지" 를 응답에 실을 수 없다 — [ADR-0046](0046-webhook-owner-trust-one-way-ack.md)).
- **운영 비용 / 유지 부담**: **동시 요청 곱은 그대로 열려 있다.** 1 MiB × 동시 요청 수가 새 최악값이고, 동시 요청 수에는 아직 상한이 없다. 그리고 `Content-Length` 1024 이하 구간은 `tiny_http` 이 먼저 읽으므로 이 상한도 [ADR-0199](0199-the-block-is-decided-before-the-body-is-read.md) 의 타입 강제도 그 아래에는 닿지 않는다.

## Alternatives Considered

- **상한을 두지 않는다(현상 유지)**: 큰 payload 를 보내는 발신자를 owner 가 자유롭게 붙일 수 있다. 그러나 위 실측이 보인 것은 선택이 아니라 **한 요청으로 프로세스를 죽일 수 있다**는 것이고, 그 프로세스는 사용자의 터미널이다. 거부.
- **`413` 대신 연결을 끊는다**: 응답 바이트조차 아낀다. 그러나 발신자는 그것을 네트워크 오류와 구분하지 못해 재시도 폭풍이 되고, 고정 ACK 계약(ADR-0046)에서 벗어난다. 거부.
- **스트리밍 파싱(전체를 메모리에 안 담고 훑는다)**: 상한 자체가 필요 없어진다. 그러나 치환 엔진이 `${body.x}` 를 임의 경로로 해소하려면 파싱된 트리가 필요하고, 스트리밍으로 그것을 만들면 결국 같은 크기를 든다. 상한이 없으면 트리 크기도 무한이다. 거부.
- **선언된 `Content-Length` 만 본다**: 구현이 한 줄이다. 그러나 chunked 에는 선언이 없어 **가장 나쁜 갈래를 통째로 놓친다**(위 표의 마지막 줄이 그 갈래다). 거부.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **동시 요청 수에 상한이 생기면** — 최악 점유의 곱셈 항이 묶이므로 요청당 상한값을 다시 정할 수 있다.
- **1 MiB 로 막히는 정상 발신자가 관측되면**(env 를 올리는 것이 예외가 아니라 기본이 되면) — 기본값을 다시 잰다.
- **`tiny_http` 을 교체하거나 그 버전이 올라가면** — 세 갈래 계약(1024 프리로드 · `EqualReader` · chunked 디코더)과 `Drop` 이 미읽은 body 를 소비하지 않는다는 전제가 이 결정의 근거다.
- **body 가 IPC 로 넘어간 뒤의 크기가 문제가 되면** — 이 상한은 리스너가 읽는 양만 묶는다. 치환 결과가 IPC params 로 어디까지 부풀 수 있는지는 별도 물음이다.

## References

- [ADR-0112](0112-agent-stream-turn-correlation.md) — 이 gap 을 Consequences 에 빚으로 적어 둔 자리
- [ADR-0199](0199-the-block-is-decided-before-the-body-is-read.md) — 차단된 출처에 한해 좁혔던 범위, 그리고 1024 프리로드로 정정된 한계
- [ADR-0195](0195-abuse-counting-includes-rejected-tokens.md) — 전수 `match` 가 새 상태의 답을 요구한 자리
- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — 고정 ACK 계약(413 을 고르고 연결 끊기를 버린 근거)
- 코드 근거(결정이 실현된 현재 위치): `src/webhook/listener.rs` 의 `max_body_bytes`·`BodyTooLarge`·`Screened::read_json_body`, `src/webhook/ack.rs` 의 `AckStatus::PayloadTooLarge`, `src/webhook/abuse.rs` 의 `counts_as_failure`

- 후속 증거: [ADR-0281](0281-webhook-parser-cap-and-connection-drain.md) (Superseded by ADR-0516) — parser 보장과 Content-Length 정리의 선행 가정 오류·미충족 transport 요구. 기존 목표를 변경하거나 구현 완료를 선언하지 않는다.
- 잔여 읽기 중단의 구현: [ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) — 413 은 레포에 둔 tiny_http 패치로 잔여 body 를 읽지 않고 연결을 닫는다.
