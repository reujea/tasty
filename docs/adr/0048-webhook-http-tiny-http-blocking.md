# ADR-0048: 웹훅 HTTP 레이어 = tiny_http (blocking, tokio 없음, TLS 는 위임)

- **Status**: Accepted
- **Date**: 2026-07-11
- **Tags**: webhook, http, tiny-http, blocking, tokio, dependency, tls, request-smuggling, cross-platform, adr-0004, adr-0046

## Context

인바운드 웹훅 리스너([ADR-0046](0046-webhook-owner-trust-one-way-ack.md))는 외부 HTTP/1.1 요청을 파싱해야 한다. tasty 워크스페이스에는 **async HTTP 스택 직접 의존이 없었다** — tokio/hyper/axum/tiny_http/httparse 모두 부재(tokio 는 전이 의존으로만 Cargo.lock 에 존재)했고, 기존 IPC 서버(`tcp_ipc_server.rs`)를 포함해 코드베이스가 **`std::thread` blocking I/O 로 일관**돼 있었다. HTTP 레이어 선택지는 세 갈래였다: (1) tokio 기반 async 스택, (2) `std::net::TcpStream` 위 수제 HTTP/1.1 파싱, (3) blocking HTTP 라이브러리.

또한 HTTP/1.1 의 엣지케이스(chunked transfer, keep-alive, 100-continue)와 request smuggling(Transfer-Encoding 혼동) 같은 파싱 취약점을 직접 감당할지, 라이브러리에 위임할지가 문제였다. TLS 는 사용자 요구가 "포워딩은 OS/공유기 몫" 이라 종단을 tasty 가 질 필요가 없었다.

## Decision

웹훅 HTTP 레이어로 **`tiny_http` 0.12.0(ssl feature 없이)** 를 쓴다. 포트마다 `tiny_http::Server` + `recv()`/`incoming_requests()` 블로킹 스레드를 두고, 요청별 worker thread 로 처리한다 — 기존 IPC 서버의 `std::thread accept + IpcWaker 로 메인 루프 깨우기` 패턴과 동형이다. **tokio 는 도입하지 않는다**(코드베이스 일관성 — 전이 의존 tokio 를 hot path 로 끌어오지 않는다). **TLS 는 켜지 않고**(feature-gated `ssl-*` 제외) 리버스 프록시/공유기에 위임한다. HTTP/1.1 준수(chunked/keep-alive/100-continue)와 request smuggling 방어는 라이브러리에 위임한다.

## Consequences

- **얻은 것**: blocking·thread-per-request 모델이 기존 std::thread I/O 와 정합해 async 런타임을 새로 들이지 않는다. HTTP/1.1 엣지케이스를 라이브러리가 처리해 수제 파싱 버그(smuggling 류)를 회피한다. 새로 추가되는 직접 의존은 3개뿐(`ascii`/`chunked_transfer`/`httpdate`, `log` 은 이미 트리에 존재) — 전부 MIT/Apache-2.0 계열로 `deny.toml` licenses allowlist 통과. `0.0.0.0` bind + HTTP 파싱은 OS 무관(원칙 4).
- **잃은 것**: tiny_http 마지막 릴리스가 0.12.0(2022-10)으로 **비활성(안정적이나 유지보수 뜸)**. async 스택 대비 커넥션당 스레드라 초고동시성엔 부적합(웹훅 트래픽 특성상 무관). TLS 를 자체 종단하지 않아 평문 HTTP 만 직접 받는다(외부 TLS 종단 필요 시 프록시 전제).
- **운영 비용 / 유지 부담**: 향후 tiny_http 에 unmaintained RUSTSEC 권고가 붙으면 `deny.toml ignore`(근거 주석)로 격리한다(기존 트랜지티브 unmaintained 처리 선례 존재). 활성 취약점 RUSTSEC-2020-0031(Transfer-Encoding request smuggling)은 0.8.0 에서 패치돼 0.12.0 영향 없음. 의존 추가는 빌드타임 `cargo deny check` 로 지속 검증.

## Alternatives Considered

- **tokio + hyper/axum**: 성숙한 async HTTP. — 코드베이스가 std::thread blocking 으로 일관돼 있어 async 런타임을 새로 들이면 hot path 에 tokio 가 침투하고 일관성이 깨진다. 웹훅 트래픽은 초고동시성이 아니라 async 이득이 작다. 거부.
- **`std::net::TcpStream` 위 수제 HTTP/1.1 파싱**(의존 0): 새 의존이 없다. — 그러나 chunked/keep-alive/헤더 엣지케이스와 request smuggling 방어를 직접 져야 한다. 의존 3개(2.5K SLoC, 매우 작음)를 아끼자고 파싱 취약점 리스크를 떠안는 건 부당한 트레이드. 거부.
- **tiny_http + ssl feature**(TLS 자체 종단): 웹훅이 직접 HTTPS 를 받음. — 사용자 요구가 "포워딩은 OS/공유기 몫" 이라 TLS 종단은 프록시/공유기 책임. 인증서 관리 부담만 늘어 기본 빌드에서 제외.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- tiny_http 에 **활성 RUSTSEC 취약점**이 붙고 `deny.toml ignore` 로 격리 불가한 심각도면 — 대체 라이브러리/수제 파싱으로 전환.
- 웹훅이 **초고동시성/롱-커넥션**(SSE/WebSocket 등) 트래픽을 요구하면 — thread-per-request 모델의 한계로 async 스택 재평가.
- tasty 가 다른 이유로 **tokio 를 1급 의존으로 채택**하면 — HTTP 레이어를 그 런타임으로 통합할지 재검토.
- 웹훅이 **자체 TLS 종단**을 정당하게 요구받으면(프록시 전제가 깨짐) — ssl feature 활성 + 인증서 관리를 재설계.

## References

- [`features/webhook/index.md`](../features/webhook/index.md) — 웹훅 리스너 동작(bind/accept/요청 처리 흐름)
- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — 웹훅 신뢰 모델/불변식(HTTP 레이어가 담는 요청의 처리 규칙)
- [ADR-0004](0004-ipc-transport-tcp.md) — 제어용 IPC 의 std::thread blocking TCP(동형 런타임 패턴)
- 코드: `src/webhook/listener.rs`(tiny_http bind/accept), `Cargo.toml`(tiny_http 0.12.0), `deny.toml`
- [ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) — tiny_http 0.12.0 은 이제 레포의 `vendor/tiny_http/` 사본(최소 패치 하나)으로 빌드된다
