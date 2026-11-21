# tiny_http 0.12.0 — tasty 사본

crates.io의 `tiny_http` 0.12.0(상류 커밋 `212b1c45852fef2093dc1374875a9393c55eb4b9`)에 아래 패치를 적용한 사본이다. 루트 `Cargo.toml`의 `[patch.crates-io]`가 이 디렉토리를 가리키므로 본체 웹훅 리스너가 이 사본으로 빌드된다. 공식 agent-stream 플러그인의 SSE 서버도 공식 플러그인 레포의 `[patch.crates-io]`가 이 레포의 커밋을 git 의존으로 가리켜 같은 사본을 쓴다. 결정 이유와 대안은 [웹훅 (Inbound webhook listener)](../../docs/features/webhook/index.md)에 있다.

## 옮긴 것과 뺀 것

- 포함: `src/` 전체, crates.io가 정규화한 `Cargo.toml`, `LICENSE-MIT`, `LICENSE-APACHE`. 라이선스는 상류의 `MIT OR Apache-2.0`을 유지한다.
- 제외: `tests/`, `examples/`(TLS 시험용 개인키 PEM 포함), `benches/`, `README.md`, `CHANGELOG.md`, `Cargo.toml.orig`, `[dev-dependencies]`.
- `Cargo.toml`에 `[lints.rust] unused = "allow"`를 추가했다. path 의존에서 표시되는 상류의 기존 미사용 re-export와 dummy trait 경고를 끄며, 이를 위해 상류 소스를 바꾸지는 않는다.

## 패치 — `Request::respond_and_close`

상류의 `Content-Length` 리더(`EqualReader`)는 요청을 해제할 때 선언된 길이 중 읽지 않은 부분을 끝까지 읽는다. 다음 요청에 연결을 재사용하려면 스트림을 요청 경계까지 이동해야 하기 때문이다. 웹훅의 413 응답처럼 요청을 거절할 때도 이 처리가 실행되어, 선언 길이에 비례하는 임시 할당과 worker 대기가 발생한다.

`Request::respond_and_close(response)`는 `Connection: close`를 붙여 응답하고 연결을 재사용하지 않는다.

- `src/request.rs`: 연결의 닫기 플래그를 설정한 뒤 기존 `respond`로 응답한다. 내부 함수 `new_request`는 이 플래그를 인자로 받는다.
- `src/util/equal_reader.rs`: 플래그가 설정되면 Drop에서 남은 body를 읽지 않는다.
- `src/client.rs`: 연결마다 플래그를 둔다. 이전 요청이 스트림을 반환할 때까지 기다린 뒤 플래그를 확인한다. 설정되어 있으면 다음 요청을 읽지 않고 소켓 양방향을 닫는다.
- `src/util/sequential.rs`: 데이터를 읽지 않고 스트림 반환을 기다리는 `wait_turn`을 제공한다.
- `src/response.rs`: 일반 `add_header`가 거절하는 `Connection: close`를 추가할 내부 메서드를 제공한다.
- `src/test.rs`: `TestRequest`가 `new_request`에 새 인자를 전달한다.

`respond_and_close`를 호출하지 않으면 남은 body를 읽는 처리를 포함해 상류 동작을 유지한다. 패치에는 `tasty patch` 주석이 있다. crates.io의 0.12.0과 `diff -r <원본>/src src`로 비교할 수 있다.

이 사본은 워크스페이스에서 제외되어 `cargo fmt`와 저장소 가드가 포맷을 검사하지 않는다. 패치를 수정한 뒤 다음 명령을 실행한다. 상류와 같은 포맷을 유지하면 이후 패치를 다시 적용할 때 불필요한 차이를 줄일 수 있다.

```sh
find vendor/tiny_http/src -name '*.rs' -exec rustfmt --edition 2018 --check {} +
```

## 걷는 조건

상류가 요청별로 남은 body를 읽지 않고 연결을 닫는 공개 API를 제공하면 이 사본과 `[patch.crates-io]` 항목을 제거한다. 검증 조건은 [웹훅 body 제한](../../docs/features/webhook/index.md#body-상한-요청당)을 따른다.
