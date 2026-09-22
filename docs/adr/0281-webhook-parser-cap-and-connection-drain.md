# ADR-0281: 웹훅 body 상한의 선행 가정 오류와 미충족 연결 정리를 기록한다

- **Status**: Superseded by ADR-0516
- **Date**: 2026-09-15
- **Tags**: webhook, body-limit, tiny-http, connection-drain, resource-bound, adr-0200

## Context

[ADR-0200](0200-webhook-body-has-a-per-request-byte-cap.md)은 요청 body의 바이트 상한과 초과 시 잔여 읽기 중단을 정했다. 그 결정은 `tiny_http`의 `respond`/`Drop`이 미읽은 body를 소비하지 않는다는 선행 가정에 기대었다. 현재 고정된 `tiny_http` 0.12.0의 일반 Content-Length 경로에서는 그 가정이 틀리다.

현재 `Screened::read_json_body`는 바이트를 모으고 UTF-8 변환 전에 길이를 검사한다. Content-Length가 이미 상한을 넘으면 이 함수의 body 읽기를 건너뛰고, 선언 길이가 없는 chunked는 판정용 1바이트를 포함해 최대 상한+1바이트를 모은다. 초과는 `413 payload too large`이며 매칭·인증·시퀀스 실행으로 진행하지 않는다. 이 parser의 입력 제한은 실제 구현된 보장이다.

그러나 `tiny_http`의 `request::new_request`는 일반적인 큰 Content-Length에 `FusedReader<EqualReader<_>>`를 사용한다. `Request::respond`가 응답을 flush한 뒤 요청 필드가 파괴되면 `EqualReader::drop`이 잔여 선언 길이를 읽어 버린다(drain). 이 루프는 잔여 길이만큼의 임시 버퍼를 요청하며 잔여 길이 소진·EOF·읽기 오류까지 진행한다. 작은 body의 사전 버퍼링도 parser 밖이다.

실측 2026-09-15, Linux x86_64, tiny_http 0.12.0/TLS 미사용, 독립 raw TCP 프로브: parser limit=8192, Content-Length=8193. body 0바이트에서 413을 받았지만 요청 정리는 끝나지 않았다. 1바이트를 더 보내도 끝나지 않았고 남은 8192바이트를 보내면 끝났다. 요청의 `Connection: close`나 `Expect: 100-continue`도 이를 제거하지 않았다. close 요청에서는 응답 EOF를 받은 뒤에도 서버의 body 수신이 이어졌다. 응답의 Connection 헤더는 무시됐고, `into_writer`는 drain 완료 전에 반환하지 않았다. `Server::unblock`/Drop도 이미 반환한 요청의 잔여 읽기를 취소하지 않았다.

따라서 parser의 입력 바이트 상한을 근거로 ADR-0200 Decision 2의 transport 목표가 충족됐다고 판단하거나, Decision 4·5와 Consequences의 메모리 설명을 전체 요청 수명의 RSS 상한으로 일반화할 수 없다. 과거 측정값과 본문은 당시 기록으로 보존하고, 새 증거와 현재의 미충족 상태를 여기서 구분한다.

## Decision

**기존 잔여 읽기 중단·자원 제한 목표를 유지한다. 이 Proposed 문서는 선행 가정 오류, 현재 구현된 parser 보장, 미충족 transport 요구와 완결 조건을 기록한다.** 목표를 낮추거나 포기하는 정책 결정이 아니며, 문서 정정을 자원 문제의 구현 완료로 처리하지 않는다.

### 현재 구현된 보장

- 기본 1 MiB와 `TASTY_WEBHOOK_MAX_BODY_BYTES` 설정, 0·파싱 실패의 기본값 처리.
- UTF-8 변환 전 바이트 길이 판정, chunked 상한+1 판정 읽기, Content-Length 선언값 선거부.
- 상한 이내 무효 UTF-8·비-JSON의 기존 `null` 정책과 정상 JSON 처리.
- 초과 시 `413 payload too large`, 매칭·인증보다 앞선 판정, 시퀀스 무실행, 413 실패 집계.
- [ADR-0199](0199-the-block-is-decided-before-the-body-is-read.md)의 남용차단 뒤 parser를 호출하는 타입 경계.

이 보장은 parser에 저장하는 입력 바이트 수에 관한 것이다. JSON 값 표현의 추가 메모리, HTTP 라이브러리의 사전 읽기·정리 할당·소켓 I/O를 포함한 전체 프로세스 메모리 상한을 뜻하지 않는다. 동시 요청 수·연결 수도 이 값으로 제한하지 않는다.

### 미충족 transport 요구와 완결 조건

현재 구현은 일반 Content-Length 초과 요청의 잔여 읽기 중단을 충족하지 못한다. 거부 응답 뒤에도 body를 읽어 버리며, 그 정리에 선언 길이 비례 임시 할당과 worker 대기가 생길 수 있다. 잔여 읽기 없이 해당 연결을 정리한다는 목표를 완료로 판정하려면 다음을 함께 만족해야 한다.

- 일반 Content-Length=상한+1에서 body를 보내지 않아도 `413 payload too large` 전체를 수신한다. 요청 close/keep-alive/Expect 경로를 각각 대조한다.
- 거부 판정 뒤 요청 body를 추가로 읽지 않고, 상대가 나머지 body나 FIN을 보내 주지 않아도 해당 요청의 수신 방향과 worker가 정리된다. 응답 수신이나 서버 송신 EOF만을 완료 신호로 쓰지 않는다.
- 거부된 선언 길이에 비례한 drain 임시 할당이 없어야 한다. parser 입력 버퍼와 정리 할당을 따로 계측하고 전체 프로세스 RSS 상한으로 확대해 주장하지 않는다.
- 초과 요청의 시퀀스 무실행·CountLimit 미차감·413 집계와 다른 정상 연결의 동작을 보존한다. Content-Length 경계·chunked·작은 무효 UTF-8도 대조한다.
- 같은 HTTP 의존성을 쓰는 SSE의 정상 구독과 빈 오류 ACK를 함께 대조한다. 기존 correlation·재시작·턴 종료 검증을 보존한다.

현재 고정 의존성의 공개 API에는 일반 요청의 413 본문을 보낸 뒤 미읽은 입력을 폐기하고 해당 연결의 수신 방향을 종료하는 경로가 없다. 구현 방식은 이 문서에서 선택하지 않는다. 의존성 패치·교체는 별도 검토 대상이며, FD 해킹·`mem::forget`·요청/스레드 누수·위장 upgrade로 충족한 것처럼 처리하지 않는다.

### 다른 계약과의 경계

[ADR-0112](0112-agent-stream-turn-correlation.md)의 요청자 id·correlation·턴 직렬화·timeout·SSE 및 웹훅 lifetime·인증·단방향 ACK 계약은 유지한다. 연결 정리의 새 발견을 원래 correlation 요구에 소급 추가하거나 이미 구현된 턴 기능을 미구현으로 되돌리지 않는다. parser의 UTF-8 수정과 transport 정리의 미충족 상태도 별개로 판정한다.

## Consequences

- **얻은 것**: 실제 parser 보장과 미충족 transport 목표를 각각 검증할 수 있다. 선행 가정 오류를 숨긴 자원 보호 완료 주장을 막는다.
- **현재 한계**: Content-Length drain과 그 작업 스레드 대기·임시 할당은 남는다. 문서 갱신으로 이 동작이 바뀌지 않는다.
- **운영 비용 / 유지 부담**: 413 전송, 응답 EOF, 서버 수신 정리 완료를 구분해야 한다. 현재 메모리 보호 범위를 입력 버퍼의 길이만으로 설명하지 않으며, transport 구현의 완결 조건은 별도로 유지한다.

## Alternatives Considered

- 응답에 Connection: close만 추가 — `Response::add_header`가 무시하며, 클라이언트 close 요청도 EqualReader drain을 제거하지 않는다.
- SSE처럼 `into_writer` 사용 — 일반 Content-Length에서는 writer 반환 전에 drain이 발생해 413 전송까지 지연된다.
- `Request::upgrade` 재사용 — 일반 요청의 EqualReader가 반환 스트림 안으로 이동하며 Drop 때 여전히 drain한다. 응답도 upgrade 헤더와 본문 생략으로 바뀐다.
- `Server::unblock`/Drop 또는 SSE stop/join — accept/스레드 조정과 해당 요청의 소켓 수신 취소는 다르다. 정상 리스너 전체 중단도 요청당 거부의 처방이 아니다.
- 의존성 vendor/fork 또는 HTTP 스택 교체 — 이 문서가 구현 선택을 승인하지 않는다. 소유권·미읽은 body 처리·keep-alive·다른 소비자·플랫폼을 포함한 별도 판단이 필요하다.
- 문구만 바꿔 기존 목표를 철회하거나 자원 보호 완료로 유지 — 미충족 요구를 지우거나 실제 I/O를 숨기는 것이므로 채택하지 않는다.

## Reconsideration Triggers

**채널이 붙는 것**

- Cargo.lock의 tiny_http 버전·출처, 직접 소비자의 의존 선언, Request/Connection 소유권 경로가 변경되면 변경 diff와 고정 버전 소스로 공개 취소 API와 drain 경로를 대조한다. 이 검토를 자동 실행하는 새 가드는 이 문서에 포함하지 않는다.
- 웹훅의 응답/연결 종료 경로가 변경되면 위 완결 조건을 적용한다. 응답만 관측하는 검사는 잔여 읽기 중단의 증거로 세지 않는다.

**원리적으로 안 붙는 것**

- 상류가 새 요청 단위 폐기/종료 API를 제공하면 — 공개 API와 구현을 읽고, body를 보내지 않은 일반 Content-Length 요청에서 413 후 수신 취소가 완료되는지 재현한다.
- 운영 중 drain의 메모리·지연 비용이 문제가 되면 — 동일 선언 길이에서 전송을 멈춘 경우와 계속 전송한 경우의 worker 수·정리 시간·할당/RSS를 구분해 잰다. 소규모 측정값을 큰 요청의 상한으로 외삽하지 않는다.

## References

- [ADR-0200](0200-webhook-body-has-a-per-request-byte-cap.md) — 기존 목표와 이번에 틀렸음이 확인된 정리 단계의 선행 가정
- [ADR-0199](0199-the-block-is-decided-before-the-body-is-read.md) — 남용차단과 parser 호출의 타입 경계
- [ADR-0048](0048-webhook-http-tiny-http-blocking.md) — HTTP 레이어 선택
- [웹훅](../features/webhook/index.md) — 현재 보장과 운영 한계
- 현재 코드: `src/webhook/listener.rs`의 `Screened::read_json_body`·`Screened::respond`, `src/webhook/ack.rs`의 `PayloadTooLarge`, `crates/tasty-plugin-agent-stream/src/sse/server.rs`의 `stream`·`respond_empty`·`SseServer::shutdown`
- [tiny_http 0.12.0 Request 소스](https://docs.rs/crate/tiny_http/0.12.0/source/src/request.rs) — new_request / respond / into_writer / upgrade
- [tiny_http 0.12.0 EqualReader 소스](https://docs.rs/crate/tiny_http/0.12.0/source/src/util/equal_reader.rs) — Drop의 잔여 읽기와 임시 할당
- [tiny_http 0.12.0 Response API](https://docs.rs/tiny_http/0.12.0/tiny_http/struct.Response.html) — Connection 헤더 제한
