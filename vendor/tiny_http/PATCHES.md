# tiny_http 0.12.0 — tasty 사본

crates.io 의 `tiny_http` 0.12.0(상류 커밋 `212b1c45852fef2093dc1374875a9393c55eb4b9`)을 옮기고
아래 패치 하나만 얹은 사본이다. 루트 `Cargo.toml` 의 `[patch.crates-io]` 가 이 디렉토리를
가리키므로 워크스페이스의 모든 `tiny_http` 소비자(본체 웹훅 리스너 · agent-stream plugin 의
SSE 서버)가 이 사본으로 빌드된다. 결정·근거·대안·탈출 조건은
[ADR-0516](../../docs/adr/0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md).

## 옮긴 것과 뺀 것

- 옮긴 것: `src/` 전체, `Cargo.toml`(crates.io 가 정규화한 판), `LICENSE-MIT`, `LICENSE-APACHE`.
  라이선스는 상류 그대로 `MIT OR Apache-2.0` 이다.
- 뺀 것: `tests/` · `examples/`(TLS 시험용 개인키 PEM 포함) · `benches/` · `README.md` ·
  `CHANGELOG.md` · `Cargo.toml.orig`. 그래서 `Cargo.toml` 의 `[dev-dependencies]` 도 뺐다.
- `Cargo.toml` 끝의 `[lints.rust] unused = "allow"` 는 tasty 가 더한 것이다 — 상류 소스가 이미 내는
  경고 둘(미사용 re-export, 쓰이지 않는 dummy trait)이 path 의존이라 모든 빌드에 뜨므로 끈다.

## 패치 — `Request::respond_and_close`

상류의 `Content-Length` 리더(`EqualReader`)는 요청이 파괴될 때 읽히지 않은 선언 길이를 끝까지
읽는다. 연결을 다음 요청에 재사용하려면 스트림을 요청 경계에 맞춰야 하기 때문이다. 거부한
요청(웹훅 413)에서는 그 읽기가 선언 길이에 비례한 임시 할당과 worker 대기가 된다.

새 공개 메서드 `Request::respond_and_close(response)` 는 응답에 `Connection: close` 를 붙여 보내고,
그 연결을 재사용하지 않는다:

- `src/request.rs` — `respond_and_close` 추가. 연결이 공유하는 닫기 플래그를 세운 뒤 기존
  `respond` 로 보낸다. `new_request` 가 그 플래그를 받는다(크레이트 내부 함수).
- `src/util/equal_reader.rs` — 플래그가 서 있으면 Drop 에서 잔여를 읽지 않는다.
- `src/client.rs` — 연결마다 플래그 하나. 다음 요청 헤더를 읽기 전에 앞 요청이 스트림을 넘겨줄
  때까지 기다리고, 플래그가 서 있으면 요청을 더 읽지 않고 연결을 끝낸다(소켓의 두 방향이 닫힌다).
- `src/util/sequential.rs` — 위 "넘겨줄 때까지 기다림" 을 읽기 없이 하는 `wait_turn`.
- `src/response.rs` — `add_header` 가 거부하는 `Connection: close` 를 붙이는 크레이트 내부 메서드.
- `src/test.rs` — `TestRequest` 가 `new_request` 의 새 인자를 채운다.

`respond_and_close` 를 부르지 않는 요청의 동작은 상류와 같다(drain 포함). 패치 자리는 전부
`tasty patch` 주석으로 표시돼 있다. 상류 원본과의 차이는 crates.io 의 0.12.0 을 받아
`diff -r <원본>/src src` 로 본다.

## 걷는 조건

상류가 요청 단위로 잔여 body 를 읽지 않고 연결을 닫는 공개 API 를 내면 이 사본을 걷고
`[patch.crates-io]` 줄을 지운다. 조건의 전문은 ADR-0516 의 재검토 트리거에 있다.
